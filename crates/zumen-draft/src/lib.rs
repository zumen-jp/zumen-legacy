//! 図面の仕様（JSON）から、用紙・縮尺のあるビュー・寸法・ハッチング・表・表題欄までそろった図面を、AutoCAD の
//! 図面のモデル（zumen-dxf の `DxfDocument`）に描く作図キット（決定 0018）。描いたモデルは、変換と同じ書き出し
//! （DXF R12〜2018、DWG、JWW、SXF）で書ける。
//!
//! - モデル空間は実寸（決定 0008）。用紙ごとにレイアウトを作り、ビューごとにその縮尺のビューポートを置く。
//! - 大きさ（文字の高さ、矢印、記号、表の幅、模様の間隔）は用紙の mm で受け取り、ビューの縮尺を掛けて描く。
//! - 描いた図形のハンドルから、仕様の図形の場所（`sheets[0].views[0].items[3]`）を引ける（`Drafted::sources`）。
//! - 受け取った図面には、描いたモデルを編集の操作にして描き込む（DXF・DWG は `plan`、JWW は `plan_jww`、SXF は
//!   `plan_sxf`。決定 0027）。
#![forbid(unsafe_code)]

mod dimension;
mod hatch;
mod into;
mod sheet;
mod spec;
mod symbols;
mod table;
mod text;

use serde_json::Value;
use spec::{Item, Point, Shape, StyleSpec};
use std::collections::HashMap;
use std::fmt;
use zumen_convert::builder::{
    BY_BLOCK, BY_LAYER, Builder, Color, LINEWEIGHT_BY_LAYER, Look, Space, lineweight, lwpolyline, xyz,
};
use zumen_core::{Notice, NoticeLevel};
use zumen_dxf::{DxfDocument, DxfEntityData};

pub use into::{Planned, plan, plan_jww, plan_sxf};
pub use spec::TYPES;

/// 仕様の JSON Schema（draft 2020-12）。エージェントやエディタが仕様を組み立て、確かめるのに使う
pub const SCHEMA: &str = include_str!("../schema/drawing-spec.schema.json");

/// 描いた図面
#[derive(Debug, Clone, PartialEq)]
pub struct Drafted {
    pub document: DxfDocument,
    /// 描けなかった・形を変えて描いた内容
    pub notices: Vec<Notice>,
    /// 図形のハンドル → 仕様の図形の場所（寸法・表などの部品は、その図形の場所）
    pub sources: HashMap<String, String>,
}

/// 仕様を読めなかった・描けなかった理由
#[derive(Debug, Clone, PartialEq)]
pub struct DraftError {
    /// 変わらない英語の名前: invalid-spec・unknown-type・unknown-key・invalid-value・missing-definition
    pub code: &'static str,
    /// 仕様の中の場所（`sheets[0].views[1].items[4]`）。全体なら空
    pub path: String,
    pub message: String,
}

impl DraftError {
    pub fn new(code: &'static str, path: &str, message: impl Into<String>) -> Self {
        Self { code, path: path.to_string(), message: message.into() }
    }
}

impl fmt::Display for DraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() { f.write_str(&self.message) } else { write!(f, "{}: {}", self.path, self.message) }
    }
}

impl std::error::Error for DraftError {}

/// 仕様の JSON の文字列から描く
pub fn draft_json(json: &str) -> Result<Drafted, DraftError> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| DraftError::new("invalid-spec", "", format!("仕様の JSON を読めません: {error}")))?;
    draft(&value)
}

/// 仕様から描く
pub fn draft(value: &Value) -> Result<Drafted, DraftError> {
    Ok(draft_on(value, None)?.0)
}

/// 描き込む先の図面（draft_into）の、描くのに使う値
#[derive(Debug, Clone, Default)]
pub(crate) struct Target {
    /// レイアウト（大文字の名前 → 用紙の幅・高さ）。用紙の名前と同じレイアウトがあれば、その用紙の大きさに描く
    pub layouts: HashMap<String, (f64, f64)>,
    /// 用紙を 1 枚だけ持ち、その大きさを変えられない図面（JWW・SXF）の用紙の幅・高さ。どの用紙もこの大きさに描く
    pub sheet: Option<(f64, f64)>,
}

impl Target {
    /// DXF・DWG の図面のレイアウト
    pub fn of(template: &DxfDocument) -> Self {
        let layouts = template
            .layouts
            .iter()
            .filter(|layout| !layout.model)
            .map(|layout| (layout.name.to_uppercase(), (layout.paper_width, layout.paper_height)))
            .collect();
        Target { layouts, sheet: None }
    }
}

/// 描いたビュー（draft_into で、ビューごとに JWW のレイヤグループ・SXF の部分図に分けるのに使う）
#[derive(Debug, Clone)]
pub(crate) struct DraftedView {
    /// 用紙の番号
    pub sheet: usize,
    /// 仕様の場所（sheets[0].views[1]）
    pub path: String,
    pub name: Option<String>,
    /// 縮尺の分母
    pub scale: f64,
    /// ビューポートの中心に映すモデルの点
    pub center: Point,
    /// 用紙の上のビューポート
    pub rect: zumen_core::BBox,
    /// 描いた図形の範囲（モデルの座標。文字の幅などは含まない）
    pub extent: Option<[f64; 4]>,
    /// このビューに描いたモデル空間の図形のハンドル
    pub handles: std::collections::HashSet<String>,
}

/// 仕様から描く。target は描き込む先の図面（draft_into）。描いたものと、寸法スタイルの値やビュー（操作に写すのに
/// 使う）を返す
pub(crate) fn draft_on(value: &Value, target: Option<&Target>) -> Result<(Drafted, Drafter), DraftError> {
    let spec = spec::parse(value)?;
    let mut drafter = Drafter::new(&spec)?;
    if let Some(target) = target {
        drafter.into = true;
        drafter.existing_layouts = target.layouts.clone();
        drafter.sheet_paper = target.sheet;
    }
    sheet::draw(&mut drafter, &spec)?;
    drafter.builder.report_renamed();
    let builder = std::mem::take(&mut drafter.builder);
    let mut document = builder.doc;
    document.notices = Vec::new();
    Ok((Drafted { document, notices: builder.notices.into_vec(), sources: builder.sources }, drafter))
}

/// 仕様の用紙を、名前の 1 枚だけにする（用紙を 1 枚しか持たない JWW・SXF に、2 枚目からの用紙を書くのに使う）
pub fn select_sheet(value: &mut Value, name: &str) -> Result<(), DraftError> {
    let Some(Value::Array(sheets)) = value.get_mut("sheets") else {
        return Err(DraftError::new("invalid-spec", "sheets", "用紙（sheets）がありません"));
    };
    let names: Vec<String> =
        sheets.iter().filter_map(|sheet| sheet.get("name").and_then(Value::as_str).map(str::to_string)).collect();
    sheets.retain(|sheet| {
        sheet.get("name").and_then(Value::as_str).is_some_and(|sheet| sheet.eq_ignore_ascii_case(name))
    });
    if sheets.is_empty() {
        return Err(DraftError::new(
            "invalid-value",
            "sheets",
            format!("用紙 {name} がありません（{}）", names.join("・")),
        ));
    }
    Ok(())
}

/// 文字の既定の高さ（用紙の mm）
const TEXT_HEIGHT: f64 = 2.5;
/// 数の値の上限（座標・長さ）
const MAX_NUMBER: f64 = 1e12;
/// 1 つの図形が作る部品（表のセル、寸法の並び）の数の上限
pub(crate) const MAX_PARTS: usize = 100_000;

/// 図形を描く場所と、用紙の mm からその場所の長さへの倍率
#[derive(Debug, Clone)]
pub(crate) struct Ctx {
    pub space: Space,
    /// 用紙の mm → 置く場所の長さ（ビューは縮尺の分母、用紙とブロックは 1）
    pub scale: f64,
    /// 外側の group から継いだ見た目
    pub style: StyleSpec,
}

impl Ctx {
    /// 用紙の mm の大きさを、置く場所の長さにする
    pub fn size(&self, paper: f64) -> f64 {
        paper * self.scale
    }

    fn with_style(&self, style: &StyleSpec) -> Ctx {
        let merged = StyleSpec {
            layer: style.layer.clone().or_else(|| self.style.layer.clone()),
            color: style.color.clone().or_else(|| self.style.color.clone()),
            line_type: style.line_type.clone().or_else(|| self.style.line_type.clone()),
            line_width: style.line_width.or(self.style.line_width),
        };
        Ctx { style: merged, ..self.clone() }
    }
}

/// 線種の定義（線分・空白の長さ、用紙の mm）
struct LineTypeDef {
    description: String,
    pattern: Vec<f64>,
}

/// 組み込みの線種（JIS Z 8312 の破線・一点鎖線・二点鎖線・点線に近い長さ、用紙の mm）
fn builtin_line_types() -> Vec<(&'static str, &'static str, Vec<f64>)> {
    vec![
        ("DASHED", "破線 __ __ __", vec![4.0, 1.5]),
        ("HIDDEN", "かくれ線 _ _ _", vec![2.5, 1.0]),
        ("CENTER", "一点鎖線 ____ _ ____", vec![12.0, 1.5, 1.0, 1.5]),
        ("PHANTOM", "二点鎖線 ____ _ _ ____", vec![12.0, 1.5, 1.0, 1.5, 1.0, 1.5]),
        ("DOT", "点線 . . . .", vec![0.0, 1.0]),
    ]
}

/// ブロックの定義
pub(crate) struct BlockInfo {
    pub name: String,
    pub attributes: Vec<spec::AttributeSpec>,
    /// 基点
    pub base: Point,
}

pub(crate) struct Drafter {
    pub builder: Builder,
    pub defaults: spec::Defaults,
    /// 大文字の名前 → 書いた名前
    layers: HashMap<String, String>,
    line_types: HashMap<String, String>,
    line_type_defs: HashMap<String, (String, LineTypeDef)>,
    pub blocks: HashMap<String, BlockInfo>,
    /// フォント → 文字スタイルの名前
    styles: HashMap<String, String>,
    pub font: String,
    /// 縮尺と寸法の見た目 → 寸法スタイルの名前
    pub dimension_styles: HashMap<String, String>,
    /// 描いた図形の範囲（ビューの中心を決めるのに、ビューごとに集める）
    pub extent: Option<[f64; 4]>,
    /// 寸法スタイルの名前 → 縮尺と寸法の見た目（draft_into で寸法スタイルを足すのに使う）
    pub dimension_looks: HashMap<String, (f64, dimension::DimStyle)>,
    /// 描き込む先の図面のレイアウト（大文字の名前 → 用紙の幅・高さ）
    pub existing_layouts: HashMap<String, (f64, f64)>,
    /// 描き込む先の、大きさを変えられない用紙（JWW・SXF）の幅・高さ
    pub sheet_paper: Option<(f64, f64)>,
    /// 描き込む先がある（draft_into）。用紙の fields の使われなかったタグは、描き込む先と合わせて確かめる
    pub into: bool,
    /// 用紙ごとの、ブロック参照の属性に埋めた fields のタグ（大文字）
    pub filled: Vec<std::collections::BTreeSet<String>>,
    /// 描いたビュー（用紙・ビューの順）
    pub views: Vec<DraftedView>,
}

impl Drafter {
    fn new(spec: &spec::Spec) -> Result<Self, DraftError> {
        if let Some(units) = &spec.units
            && !units.trim().eq_ignore_ascii_case("mm")
        {
            return Err(DraftError::new("invalid-value", "units", format!("長さの単位は mm だけです（{units}）")));
        }
        let mut drafter = Drafter {
            builder: Builder::new(),
            defaults: spec.defaults.clone(),
            layers: HashMap::new(),
            line_types: HashMap::new(),
            line_type_defs: HashMap::new(),
            blocks: HashMap::new(),
            styles: HashMap::new(),
            font: spec.font.clone().unwrap_or_else(|| zumen_convert::builder::DEFAULT_FONT.to_string()),
            dimension_styles: HashMap::new(),
            extent: None,
            dimension_looks: HashMap::new(),
            existing_layouts: HashMap::new(),
            sheet_paper: None,
            into: false,
            filled: Vec::new(),
            views: Vec::new(),
        };
        drafter.layers.insert("0".into(), "0".into());
        for (name, description, pattern) in builtin_line_types() {
            let def = LineTypeDef { description: description.into(), pattern };
            drafter.line_type_defs.insert(name.into(), (name.into(), def));
        }
        for (index, line_type) in spec.line_types.iter().enumerate() {
            let path = format!("lineTypes[{index}]");
            if line_type.pattern.is_empty() || line_type.pattern.iter().any(|value| !value.is_finite() || *value < 0.0)
            {
                return Err(DraftError::new(
                    "invalid-value",
                    &path,
                    "pattern は線分・空白の長さ（0 以上、用紙の mm）を交互に並べてください",
                ));
            }
            let def = LineTypeDef {
                description: line_type.description.clone().unwrap_or_default(),
                pattern: line_type.pattern.clone(),
            };
            drafter.line_type_defs.insert(line_type.name.to_uppercase(), (line_type.name.clone(), def));
        }
        for (index, layer) in spec.layers.iter().enumerate() {
            drafter.declare_layer(layer, &format!("layers[{index}]"))?;
        }
        Ok(drafter)
    }

    fn declare_layer(&mut self, layer: &spec::LayerSpec, path: &str) -> Result<(), DraftError> {
        let key = layer.name.to_uppercase();
        if self.layers.contains_key(&key) && key != "0" {
            return Err(DraftError::new("invalid-spec", path, format!("レイヤ {} が 2 度あります", layer.name)));
        }
        let name = if key == "0" {
            "0".to_string()
        } else {
            self.builder.layer(&layer.name, layer.off, layer.locked, layer.plot.unwrap_or(true))
        };
        let color = match &layer.color {
            Some(value) => Some(color(value, path)?),
            None => None,
        };
        let line_type = match &layer.line_type {
            Some(line_type) => Some(self.line_type(line_type, path)?),
            None => None,
        };
        let line_width = match layer.line_width {
            Some(width) => Some(width_of(width, path)?),
            None => None,
        };
        if let Some(entry) = self.builder.doc.layers.iter_mut().find(|entry| entry.name == name) {
            if let Some(color) = color {
                entry.color_index = if (1.0..=255.0).contains(&color.index) { color.index } else { 7.0 };
                entry.true_color = color.true_color;
            }
            if let Some(line_type) = line_type {
                entry.linetype = line_type;
            }
            if let Some(width) = line_width {
                entry.lineweight = width;
            }
            if key == "0" {
                entry.off = layer.off;
                entry.locked = layer.locked;
                entry.plot = layer.plot.unwrap_or(true);
            }
        }
        self.layers.insert(key, name);
        Ok(())
    }

    /// レイヤの書いた名前。まだなければ作る
    pub fn layer(&mut self, name: &str) -> String {
        let key = name.to_uppercase();
        if let Some(written) = self.layers.get(&key) {
            return written.clone();
        }
        let written = self.builder.layer(name, false, false, true);
        self.layers.insert(key, written.clone());
        written
    }

    /// 線種の書いた名前。組み込みか仕様の線種なら、初めて使うときに作る
    pub fn line_type(&mut self, name: &str, path: &str) -> Result<String, DraftError> {
        let key = name.to_uppercase();
        match key.as_str() {
            "CONTINUOUS" => return Ok("Continuous".into()),
            "BYLAYER" => return Ok("ByLayer".into()),
            "BYBLOCK" => return Ok("ByBlock".into()),
            _ => {}
        }
        if let Some(written) = self.line_types.get(&key) {
            return Ok(written.clone());
        }
        let Some((display, def)) = self.line_type_defs.get(&key) else {
            let known: Vec<String> = self.line_type_defs.values().map(|(name, _)| name.clone()).collect();
            return Err(DraftError::new(
                "missing-definition",
                path,
                format!("線種 {name} がありません（Continuous と {}、または lineTypes で定義）", known.join("・")),
            ));
        };
        let (display, description, pattern) = (display.clone(), def.description.clone(), def.pattern.clone());
        let written = self.builder.linetype(&display, &description, &pattern);
        self.line_types.insert(key, written.clone());
        Ok(written)
    }

    /// フォントの文字スタイルの名前
    pub fn text_style(&mut self, font: Option<&str>) -> String {
        let font = font.unwrap_or(&self.font).to_string();
        if let Some(style) = self.styles.get(&font) {
            return style.clone();
        }
        let style = self.builder.style(&font, false);
        self.styles.insert(font, style.clone());
        style
    }

    /// 図形の見た目。layer がなければ default_layer、それもなければ defaults.layer か 0
    pub fn look(
        &mut self,
        ctx: &Ctx,
        style: &StyleSpec,
        default_layer: Option<&str>,
        path: &str,
    ) -> Result<Look, DraftError> {
        let layer = style
            .layer
            .clone()
            .or_else(|| ctx.style.layer.clone())
            .or_else(|| default_layer.map(str::to_string))
            .or_else(|| self.defaults.layer.clone())
            .unwrap_or_else(|| "0".into());
        let layer = self.layer(&layer);
        let color = match style.color.as_ref().or(ctx.style.color.as_ref()) {
            Some(value) => color(value, path)?,
            None => BY_LAYER,
        };
        let linetype = match style.line_type.as_ref().or(ctx.style.line_type.as_ref()) {
            Some(name) => self.line_type(name, path)?,
            None => "ByLayer".into(),
        };
        let lineweight = match style.line_width.or(ctx.style.line_width) {
            Some(width) => width_of(width, path)?,
            None => LINEWEIGHT_BY_LAYER,
        };
        Ok(Look { layer, color, linetype, lineweight, linetype_scale: 1.0 })
    }

    /// 図形を加え、ハンドルを返す。描いた範囲を広げる
    pub fn add(&mut self, space: &Space, look: &Look, kind: &str, data: DxfEntityData, path: &str) -> String {
        if !matches!(space, Space::Block(_)) {
            for [x, y] in extent_points(&data) {
                extend(&mut self.extent, x, y);
            }
        }
        self.builder.add(space, look, kind, data, path)
    }

    pub fn notice(&mut self, code: &str, message: impl Into<String>) {
        self.builder.notices.add(NoticeLevel::Info, code, message);
    }

    /// 図形の並びを描く
    pub fn items(&mut self, items: &[Item], ctx: &Ctx) -> Result<(), DraftError> {
        for item in items {
            self.item(item, ctx)?;
        }
        Ok(())
    }

    fn item(&mut self, item: &Item, ctx: &Ctx) -> Result<(), DraftError> {
        let path = item.path.as_str();
        let style = &item.style;
        match &item.shape {
            Shape::Group(items) => self.items(items, &ctx.with_style(style)),
            Shape::Line(line) => {
                let look = self.look(ctx, style, None, path)?;
                let (from, to) = (point(line.from, path)?, point(line.to, path)?);
                self.add(&ctx.space, &look, "LINE", DxfEntityData::Line { start: xyz(from), end: xyz(to) }, path);
                Ok(())
            }
            Shape::Polyline(polyline) => {
                let look = self.look(ctx, style, None, path)?;
                let points = points(&polyline.points, 2, path)?;
                if let Some(bulges) = &polyline.bulges
                    && bulges.len() != points.len()
                {
                    return Err(DraftError::new("invalid-value", path, "bulges は頂点と同じ数にしてください"));
                }
                let mut data = lwpolyline(&points, polyline.bulges.clone(), polyline.closed);
                if let (Some(width), DxfEntityData::LwPolyline(lw)) = (polyline.width, &mut data) {
                    let width = ctx.size(positive(width, "width", path)?);
                    lw.const_width = Some(width);
                    lw.vertices.widths = vec![[width, width]; points.len()];
                }
                self.add(&ctx.space, &look, "LWPOLYLINE", data, path);
                Ok(())
            }
            Shape::Rectangle(rectangle) => {
                let look = self.look(ctx, style, None, path)?;
                let (a, b) = (point(rectangle.from, path)?, point(rectangle.to, path)?);
                let corners = vec![a, [b[0], a[1]], b, [a[0], b[1]]];
                if rectangle.fill {
                    hatch::solid(self, ctx, &look, std::slice::from_ref(&corners), path);
                }
                self.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&corners, None, true), path);
                Ok(())
            }
            Shape::Circle(circle) => {
                let look = self.look(ctx, style, None, path)?;
                let center = point(circle.center, path)?;
                let radius = positive(circle.radius, "radius", path)?;
                if circle.fill {
                    symbols::dot(self, &ctx.space, &look, center, radius * 2.0, path);
                }
                self.add(&ctx.space, &look, "CIRCLE", DxfEntityData::Circle { center: xyz(center), radius }, path);
                Ok(())
            }
            Shape::Arc(arc) => {
                let look = self.look(ctx, style, None, path)?;
                let center = point(arc.center, path)?;
                let radius = positive(arc.radius, "radius", path)?;
                let data = DxfEntityData::Arc {
                    center: xyz(center),
                    radius,
                    start_angle: number(arc.start, "start", path)?.rem_euclid(360.0),
                    end_angle: number(arc.end, "end", path)?.rem_euclid(360.0),
                };
                self.add(&ctx.space, &look, "ARC", data, path);
                Ok(())
            }
            Shape::Ellipse(ellipse) => {
                let look = self.look(ctx, style, None, path)?;
                let center = point(ellipse.center, path)?;
                let major = point(ellipse.major_axis, path)?;
                if major[0].hypot(major[1]) <= 0.0 || !(ellipse.ratio > 0.0 && ellipse.ratio <= 1.0) {
                    return Err(DraftError::new(
                        "invalid-value",
                        path,
                        "majorAxis は 0 でない長さ、ratio は 0 より大きく 1 以下にしてください",
                    ));
                }
                let start = ellipse.start.unwrap_or(0.0).to_radians();
                let end = ellipse.end.map_or(std::f64::consts::TAU, f64::to_radians);
                let data = DxfEntityData::Ellipse {
                    center: xyz(center),
                    major_axis: xyz(major),
                    ratio: ellipse.ratio,
                    start_param: start,
                    end_param: end,
                };
                self.add(&ctx.space, &look, "ELLIPSE", data, path);
                Ok(())
            }
            Shape::Point(spec) => {
                let look = self.look(ctx, style, None, path)?;
                let at = point(spec.at, path)?;
                self.add(
                    &ctx.space,
                    &look,
                    "POINT",
                    DxfEntityData::Point { location: xyz(at), x_axis_angle: 0.0 },
                    path,
                );
                Ok(())
            }
            Shape::Text(spec) => text::draw(self, ctx, style, spec, path),
            Shape::Hatch(spec) => hatch::draw(self, ctx, style, spec, path),
            Shape::Dimension(spec) => dimension::draw(self, ctx, style, spec, path),
            Shape::DimensionChain(spec) => dimension::draw_chain(self, ctx, style, spec, path),
            Shape::Insert(spec) => sheet::insert(self, ctx, style, spec, path),
            Shape::Table(spec) => table::draw(self, ctx, style, spec, path),
            Shape::Grid(spec) => symbols::grid(self, ctx, style, spec, path),
            Shape::NorthArrow(spec) => symbols::north_arrow(self, ctx, style, spec, path),
            Shape::ScaleBar(spec) => symbols::scale_bar(self, ctx, style, spec, path),
            Shape::LevelMark(spec) => symbols::level_mark(self, ctx, style, spec, path),
            Shape::SpotLevel(spec) => symbols::spot_level(self, ctx, style, spec, path),
            Shape::Leader(spec) => symbols::leader(self, ctx, style, spec, path),
            Shape::PolygonLabel(spec) => symbols::polygon_label(self, ctx, style, spec, path),
            Shape::ViewTitle(spec) => symbols::view_title(self, ctx, style, spec, None, path),
        }
    }

    /// 文字の既定の高さ（用紙の mm）
    pub fn text_height(&self) -> f64 {
        self.defaults.text_height.filter(|height| *height > 0.0).unwrap_or(TEXT_HEIGHT)
    }
}

/// 色の指定: ACI の番号（1〜255）、#rrggbb、bylayer、byblock
fn color(value: &Value, path: &str) -> Result<Color, DraftError> {
    let invalid = || {
        DraftError::new("invalid-value", path, "color は 1〜255 の番号、#rrggbb、bylayer、byblock で指定してください")
    };
    match value {
        Value::Number(number) => {
            let index = number.as_f64().ok_or_else(invalid)?;
            if index.fract() != 0.0 || !(1.0..=255.0).contains(&index) {
                return Err(invalid());
            }
            Ok(Color { index, true_color: None })
        }
        Value::String(text) => match text.to_lowercase().as_str() {
            "bylayer" => Ok(BY_LAYER),
            "byblock" => Ok(BY_BLOCK),
            hex if hex.starts_with('#') && hex.len() == 7 && hex[1..].chars().all(|c| c.is_ascii_hexdigit()) => {
                Ok(Color::from_hex(hex))
            }
            _ => Err(invalid()),
        },
        _ => Err(invalid()),
    }
}

/// 線の太さ（mm）を DXF の太さ（1/100 mm）にする
fn width_of(width: f64, path: &str) -> Result<f64, DraftError> {
    if !width.is_finite() || !(0.0..=2.11).contains(&width) {
        return Err(DraftError::new("invalid-value", path, "lineWidth は 0〜2.11 mm で指定してください"));
    }
    Ok(lineweight(width).0)
}

pub(crate) fn number(value: f64, key: &str, path: &str) -> Result<f64, DraftError> {
    if !value.is_finite() || value.abs() > MAX_NUMBER {
        return Err(DraftError::new("invalid-value", path, format!("{key} は有限で大きすぎない数にしてください")));
    }
    Ok(value)
}

pub(crate) fn positive(value: f64, key: &str, path: &str) -> Result<f64, DraftError> {
    let value = number(value, key, path)?;
    if value <= 0.0 {
        return Err(DraftError::new("invalid-value", path, format!("{key} は 0 より大きい数にしてください")));
    }
    Ok(value)
}

pub(crate) fn point(p: Point, path: &str) -> Result<Point, DraftError> {
    Ok([number(p[0], "座標", path)?, number(p[1], "座標", path)?])
}

pub(crate) fn points(list: &[Point], min: usize, path: &str) -> Result<Vec<Point>, DraftError> {
    if list.len() < min {
        return Err(DraftError::new("invalid-value", path, format!("点は {min} つ以上にしてください")));
    }
    if list.len() > MAX_PARTS {
        return Err(DraftError::new("invalid-value", path, format!("点は {MAX_PARTS} までにしてください")));
    }
    list.iter().map(|p| point(*p, path)).collect()
}

fn extend(extent: &mut Option<[f64; 4]>, x: f64, y: f64) {
    match extent {
        Some(e) => {
            e[0] = e[0].min(x);
            e[1] = e[1].min(y);
            e[2] = e[2].max(x);
            e[3] = e[3].max(y);
        }
        None => *extent = Some([x, y, x, y]),
    }
}

/// 図形の範囲を決める点（ビューの中心を決めるだけなので、文字の幅などは含めない）
fn extent_points(data: &DxfEntityData) -> Vec<Point> {
    let p = |v: &[f64; 3]| [v[0], v[1]];
    match data {
        DxfEntityData::Line { start, end } => vec![p(start), p(end)],
        DxfEntityData::Circle { center, radius } | DxfEntityData::Arc { center, radius, .. } => {
            vec![[center[0] - radius, center[1] - radius], [center[0] + radius, center[1] + radius]]
        }
        DxfEntityData::Ellipse { center, major_axis, .. } => {
            let r = major_axis[0].hypot(major_axis[1]);
            vec![[center[0] - r, center[1] - r], [center[0] + r, center[1] + r]]
        }
        DxfEntityData::Point { location, .. } => vec![p(location)],
        DxfEntityData::LwPolyline(lw) => lw.vertices.points.iter().map(p).collect(),
        DxfEntityData::Text(text) => vec![p(&text.insert)],
        DxfEntityData::MText(mtext) => vec![p(&mtext.insert)],
        DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, .. } => vec![p(vtx0), p(vtx1), p(vtx2), p(vtx3)],
        DxfEntityData::Hatch(hatch) => hatch.paths.iter().flat_map(|path| path.points.iter().map(p)).collect(),
        DxfEntityData::Insert(insert) => vec![p(&insert.insert)],
        DxfEntityData::Dimension(dimension) => {
            [Some(dimension.defpoint), dimension.defpoint2, dimension.defpoint3].iter().flatten().map(p).collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod review_tests {
    #[test]
    fn reports_views_overlapping_across_sheets() {
        let spec = serde_json::json!({"sheets": [
            {"name": "First", "views": [{"scale": 100, "viewport": [0, 0, 100, 100],
                "center": [0, 0], "items": [{"type": "line", "from": [0, 0], "to": [10, 0]}]}]},
            {"name": "Second", "views": [{"scale": 100, "viewport": [0, 0, 100, 100],
                "center": [0, 0], "items": [{"type": "line", "from": [0, 0], "to": [0, 10]}]}]}
        ]});
        let drafted = super::draft(&spec).unwrap();
        let notice = drafted
            .notices
            .iter()
            .find(|notice| notice.code == "view-overlap")
            .expect("views on separate sheets still share model geometry");
        assert_eq!(notice.count, 1);
        assert!(notice.message.contains("sheets[0].views[0]"));
        assert!(notice.message.contains("sheets[1].views[0]"));
    }
}
