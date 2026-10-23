//! 用紙を 1 枚だけ持つ JWW・SXF の図面にその場で描く（draft_into、決定 0027）。仕様から描いた図面のモデル
//! （`DxfDocument`）を、JWW・SXF の編集の操作に写す。DXF・DWG に描くとき（into.rs）と同じく、操作を `zumen::edit`
//! で当てるので、元の図面のレコード・フィーチャはそのまま残り、描いたものだけが加わる。
//!
//! - 用紙: 描き込む先の 1 枚に、その大きさで描く。仕様の 2 枚目からの用紙は描かずに知らせる。
//! - ビュー: JWW はビューの縮尺のレイヤグループ（なければ図形のないレイヤグループの縮尺を変える）に、SXF はビューごとに
//!   足す部分図（viewport の操作）に、実寸で描く。用紙の図形は、JWW は縮尺 1 のレイヤグループ、SXF は用紙に置く。
//! - JWW・SXF は BYLAYER・BYBLOCK を持たず、線種も足せない。色・線種・線の太さは図形ごとに決めて書き、線種は図面にある
//!   線分・空白の長さの近い線種にする。
//! - 文字は行ごとに、文字範囲の左下を位置合わせから決める。折線は JWW では線と円弧、SXF では折線と円弧にし、幅のある
//!   折線は塗りにする。属性は値を文字にする。

use super::{Planned, hatch_value, mtext_width_factor, solid_ring};
use crate::dimension::{DimStyle, format_number};
use crate::{DraftError, DraftedView, Target, draft_on, spec};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use std::f64::consts::TAU;
use zumen_core::text::{EM_LATIN, is_half_width};
use zumen_core::{NoticeLevel, NoticeList, js};
use zumen_dxf::{
    DxfDocument, DxfEntity, DxfEntityBase, DxfEntityData, DxfLayer, DxfSpace, DxfTextBase, TEXT_DESCENT, aci_color,
    dash_array, parse_text_codes, true_color,
};
use zumen_jww::{JwwDocument, JwwEntity, paper_size};
use zumen_sxf::SxfDrawing;
use zumen_sxf::tables::{PREDEFINED_LINE_TYPES, sheet_size};

/// MTEXT の行の間隔（文字の高さの 5/3、DXF Reference「MTEXT」の 44。表示と同じ）
const LINE_SPACING: f64 = 5.0 / 3.0;
/// 点の線種の点を比べる長さ（SXF の既定義線種の点と同じ、用紙の mm）
const DOT: f64 = 0.25;
/// 幅のある折線の弧を塗りの多角形にするときの、1 周の分割の数
const ARC_SEGMENTS: f64 = 64.0;
/// ビューポートからはみ出したとみなす長さ（用紙の mm）
const CLIP_TOLERANCE: f64 = 0.5;

/// 仕様から、JWW の図面（template）に描き込む編集の操作を作る（決定 0027）
pub fn plan_jww(value: &Value, template: &JwwDocument) -> Result<Planned, DraftError> {
    let (paper, unknown) = match paper_size(template.paper_size) {
        Some((_, width, height)) => ((width, height), false),
        None => ((420.0, 297.0), true),
    };
    let mut planned = plan(value, Some(paper), Format::Jww(Jww::new(template)))?;
    if unknown {
        let mut notices = NoticeList::from_notices(&planned.notices);
        notices.add(
            NoticeLevel::Warning,
            "draft-into-paper",
            format!("JWW の用紙サイズの番号 {} が分からないため、A3 の用紙として描きました", template.paper_size),
        );
        planned.notices = notices.into_vec();
    }
    Ok(planned)
}

/// 仕様から、SXF の図面（template）に描き込む編集の操作を作る（決定 0027）。用紙のない SXF は仕様の用紙の大きさに描く
pub fn plan_sxf(value: &Value, template: &SxfDrawing) -> Result<Planned, DraftError> {
    let paper = template.document.sheet.as_ref().map(|sheet| match sheet_size(sheet.sheet_type) {
        // 用紙サイズ種別の寸法は縦長。向き（orient）が 0 でなければ横長
        Some((_, width, height)) if sheet.orient != 0.0 => (height, width),
        Some((_, width, height)) => (width, height),
        None => (js::max(sheet.x, 1.0), js::max(sheet.y, 1.0)),
    });
    plan(value, paper, Format::Sxf(Sxf::new(template)))
}

fn plan(value: &Value, paper: Option<(f64, f64)>, format: Format<'_>) -> Result<Planned, DraftError> {
    // 描き込む先は用紙を 1 枚だけ持つので、1 枚目だけを描く
    let mut value = value.clone();
    let mut notices = NoticeList::new();
    if let Some(Value::Array(sheets)) = value.get_mut("sheets")
        && sheets.len() > 1
    {
        notices.add_count(
            NoticeLevel::Info,
            "draft-into-sheets",
            format!(
                "{} の図面は用紙を 1 枚だけ持つため、1 枚目のほかの用紙は描いていません（sheet で選べます）",
                format.label()
            ),
            (sheets.len() - 1) as u64,
        );
        sheets.truncate(1);
    }
    let spec = spec::parse(&value)?;
    let (drafted, drafter) = draft_on(&value, Some(&Target { sheet: paper, ..Target::default() }))?;
    // ビューはそれぞれのレイヤグループ・部分図に描くので、モデルの範囲が重なっても互いの図形は映らない
    for notice in drafted.notices.iter().filter(|notice| notice.code != "view-overlap") {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    let doc = &drafted.document;
    let size = doc
        .layouts
        .iter()
        .find(|layout| !layout.model)
        .map_or((420.0, 297.0), |layout| (layout.paper_width, layout.paper_height));
    let mut planner = Planner {
        doc,
        sources: &drafted.sources,
        looks: &drafter.dimension_looks,
        format,
        size,
        layers: doc.layers.iter().map(|layer| (layer.name.to_uppercase(), layer)).collect(),
        prelude: Vec::new(),
        body: Vec::new(),
        notices,
        line_types: HashMap::new(),
        inexact_line_types: BTreeSet::new(),
        layer_values: BTreeSet::new(),
        blocks: HashMap::new(),
        homes: HashMap::new(),
        figures: Vec::new(),
        attributes: 0,
        definitions: 0,
        inherit_differs: 0,
        angular: 0,
        clipped: 0,
    };
    let views: Vec<&DraftedView> = drafter.views.iter().filter(|view| view.sheet == 0).collect();
    planner.run(&views)?;
    // 用紙の fields は、仕様のブロック参照の属性に埋めた（描き込む先の JWW・SXF には属性がない）
    for (index, sheet) in spec.sheets.iter().enumerate() {
        let filled = drafter.filled.get(index).cloned().unwrap_or_default();
        let unused: Vec<&str> =
            sheet.fields.keys().filter(|tag| !filled.contains(&tag.to_uppercase())).map(String::as_str).collect();
        if !unused.is_empty() {
            planner.notices.add(
                NoticeLevel::Info,
                "unused-field",
                format!("sheets[{index}].fields の {} の属性を持つブロック参照が用紙にありません", unused.join("・")),
            );
        }
    }
    Ok(planner.finish())
}

/// 描き込む先の形式
enum Format<'a> {
    Jww(Jww<'a>),
    Sxf(Sxf<'a>),
}

impl Format<'_> {
    fn label(&self) -> &'static str {
        match self {
            Format::Jww(_) => "JWW",
            Format::Sxf(_) => "SXF",
        }
    }
}

/// JWW の図面のレイヤグループとレイヤ
struct Jww<'a> {
    doc: &'a JwwDocument,
    /// 図形のあるレイヤ（グループ、レイヤ）
    used: BTreeSet<(usize, usize)>,
    /// この描き込みで決めた縮尺（グループ → 縮尺）
    chosen: HashMap<usize, f64>,
    /// (グループ, 仕様のレイヤの大文字の名前) → レイヤ
    layers: HashMap<(usize, String), usize>,
    /// この描き込みで名前を付けたレイヤ
    named: BTreeSet<(usize, usize)>,
}

impl<'a> Jww<'a> {
    fn new(doc: &'a JwwDocument) -> Self {
        let mut used = BTreeSet::new();
        let entities = doc.entities.iter().chain(doc.blocks.iter().flat_map(|block| block.entities.iter()));
        for entity in entities {
            for base in jww_bases(entity) {
                used.insert((usize::from(base.layer_group & 0x0f), usize::from(base.layer & 0x0f)));
            }
        }
        Jww { doc, used, chosen: HashMap::new(), layers: HashMap::new(), named: BTreeSet::new() }
    }

    fn scale(&self, group: usize) -> f64 {
        self.chosen.get(&group).copied().unwrap_or_else(|| self.doc.group_scale(group as u16))
    }

    /// 書込みレイヤグループ（状態 3）
    fn write_group(&self) -> usize {
        self.doc.groups.iter().position(|group| group.state == 3).unwrap_or(0)
    }

    /// グループの書込みレイヤ（状態 3）
    fn write_layer(&self, group: usize) -> usize {
        self.doc.groups.get(group).and_then(|group| group.layers.iter().position(|layer| layer.state == 3)).unwrap_or(0)
    }

    /// 図形を置けるグループ（編集できる・書込み）
    fn usable(&self, group: usize) -> bool {
        self.doc.groups.get(group).is_some_and(|group| group.state >= 2)
    }
}

/// 図形の属性（寸法は線・文字・補助線・点の属性も）
fn jww_bases(entity: &JwwEntity) -> Vec<&zumen_jww::JwwBase> {
    match entity {
        JwwEntity::Dimension(e) => {
            let mut bases = vec![&e.base, &e.line.base, &e.text.base];
            bases.extend(e.lines.iter().map(|line| &line.base));
            bases.extend(e.points.iter().map(|point| &point.base));
            bases
        }
        other => vec![other.base()],
    }
}

/// SXF の図面のレイヤと複合図形
struct Sxf<'a> {
    drawing: &'a SxfDrawing,
    /// 仕様のレイヤの大文字の名前 → SXF のレイヤの名前
    layers: HashMap<String, String>,
    /// この描き込みで足した複合図形の名前
    figures: BTreeSet<String>,
}

impl<'a> Sxf<'a> {
    fn new(drawing: &'a SxfDrawing) -> Self {
        Sxf { drawing, layers: HashMap::new(), figures: BTreeSet::new() }
    }

    fn figure_exists(&self, name: &str) -> bool {
        self.drawing.document.sub_figures.contains(name) || self.figures.contains(name)
    }
}

/// 解決した見た目（BYLAYER・BYBLOCK を辿った値）
#[derive(Debug, Clone, PartialEq)]
struct Look {
    /// 仕様のレイヤの名前
    layer: String,
    /// #rrggbb
    color: String,
    /// 線種の名前（実線は None）
    line_type: Option<String>,
    /// 線の太さ（mm。既定は None）
    line_width: Option<f64>,
}

/// 図形を置く場所
#[derive(Debug, Clone)]
struct Place {
    /// container の値（None は既定）
    container: Option<String>,
    /// JWW のレイヤグループ（SXF は 0）
    group: usize,
    /// 座標を置く場所の座標にする移動
    offset: [f64; 2],
    /// 用紙の mm で持つ大きさ（JWW の文字の大きさ・ブロックの配置の倍率）を割る数（JWW のレイヤグループの縮尺）
    paper: f64,
    /// ブロックの中で、レイヤ 0 の図形と BYBLOCK が継ぐ見た目
    inherit: Option<Look>,
}

impl Place {
    fn at(&self, p: &[f64]) -> [f64; 2] {
        [p[0] + self.offset[0], p[1] + self.offset[1]]
    }
}

/// ブロックの図形を置く場所（最初の配置のレイヤグループと、継ぐ見た目）
#[derive(Debug, Clone)]
struct Home {
    group: usize,
    inherit: Look,
}

struct Planner<'a> {
    doc: &'a DxfDocument,
    sources: &'a HashMap<String, String>,
    looks: &'a HashMap<String, (f64, DimStyle)>,
    format: Format<'a>,
    /// 用紙の幅・高さ
    size: (f64, f64),
    layers: HashMap<String, &'a DxfLayer>,
    /// レイヤ・レイヤグループの操作（図形より先に当てる）
    prelude: Vec<(Value, String)>,
    /// ブロックの定義・ビューポート・図形の操作
    body: Vec<(Value, String)>,
    notices: NoticeList,
    /// 線種の大文字の名前 → 書く値（JWW は番号、SXF は名前。実線は None）
    line_types: HashMap<String, Option<Value>>,
    inexact_line_types: BTreeSet<String>,
    /// 形式のレイヤにない値（印刷しない・ロック）を持つ仕様のレイヤ
    layer_values: BTreeSet<String>,
    /// 描いたブロックの大文字の名前 → 書く名前
    blocks: HashMap<String, String>,
    homes: HashMap<String, Home>,
    /// ビューごとの、置く場所
    figures: Vec<Place>,
    attributes: u64,
    definitions: u64,
    inherit_differs: u64,
    angular: u64,
    clipped: u64,
}

impl<'a> Planner<'a> {
    fn finish(mut self) -> Planned {
        if !self.inexact_line_types.is_empty() {
            self.notices.add_count(
                NoticeLevel::Info,
                "draft-into-line-type",
                format!(
                    "{} の編集は線種を足せないため、図面にない線種は、線分と空白の長さの近い図面の線種にしました",
                    self.format.label()
                ),
                self.inexact_line_types.len() as u64,
            );
        }
        if !self.layer_values.is_empty() {
            self.notices.add_count(
                NoticeLevel::Info,
                "draft-into-layer-attributes",
                format!(
                    "{} のレイヤは印刷しない設定{}を持たないため、書いていません",
                    self.format.label(),
                    if matches!(self.format, Format::Sxf(_)) { "・ロック" } else { "" }
                ),
                self.layer_values.len() as u64,
            );
        }
        let counts = [
            (
                self.attributes + self.definitions,
                "draft-into-attributes",
                "属性を持たないため、ブロック参照の属性の値を文字にしました（属性の定義は書いていません）",
            ),
            (
                self.inherit_differs,
                "draft-into-block-inherit",
                "ブロックの図形は配置ごとに色・線種・レイヤを継がないため、最初の配置の値で描きました",
            ),
            (
                self.angular,
                "draft-into-angular-dimension",
                "寸法図形は角度寸法を持てないため、角度寸法を線・円弧・文字で描きました",
            ),
            (
                self.clipped,
                "draft-into-view-clip",
                "レイヤグループ・部分図はビューポートで切り取らないため、ビューポートの外にかかる図形もそのまま描きました",
            ),
        ];
        let label = self.format.label();
        for (count, code, message) in counts {
            if count > 0 {
                self.notices.add_count(NoticeLevel::Info, code, format!("{label} は{message}"), count);
            }
        }
        let (operations, sources): (Vec<Value>, Vec<String>) = self.prelude.into_iter().chain(self.body).unzip();
        Planned { operations: Value::Array(operations), notices: self.notices.into_vec(), sources }
    }

    fn info(&mut self, code: &str, message: impl Into<String>) {
        self.notices.add(NoticeLevel::Info, code, message);
    }

    fn run(&mut self, views: &[&DraftedView]) -> Result<(), DraftError> {
        let doc = self.doc;
        let (width, height) = self.size;
        let sheet_block = doc
            .layouts
            .iter()
            .find(|layout| !layout.model)
            .map_or_else(|| "*PAPER_SPACE".to_string(), |layout| layout.block.to_uppercase());
        let on_sheet = |entity: &DxfEntity| {
            entity.base.space == DxfSpace::Paper
                && entity.base.block.as_deref().unwrap_or("*Paper_Space").to_uppercase() == sheet_block
        };
        // 用紙の図形の置き場所（JWW は縮尺 1 のレイヤグループ、座標は用紙の中心から。SXF は用紙）。描く図形のない
        // レイヤグループ・部分図は選ばない
        let unused = Place { container: None, group: 0, offset: [0.0; 2], paper: 1.0, inherit: None };
        let sheet_items =
            doc.entities.iter().any(|entity| on_sheet(entity) && !matches!(entity.data, DxfEntityData::Viewport(_)));
        let sheet = match &self.format {
            Format::Jww(_) if sheet_items => {
                let group = self.group_for(1.0, "sheets[0]")?;
                Place {
                    container: Some(format!("group:{group:X}")),
                    group,
                    offset: [-width / 2.0, -height / 2.0],
                    paper: 1.0,
                    inherit: None,
                }
            }
            _ => unused.clone(),
        };
        // ビューの置き場所
        let mut owners: HashMap<&str, usize> = HashMap::new();
        for (index, view) in views.iter().enumerate() {
            if view.handles.is_empty() {
                self.figures.push(unused.clone());
                continue;
            }
            for handle in &view.handles {
                owners.insert(handle.as_str(), index);
            }
            let place = self.view_place(view)?;
            self.figures.push(place);
            if let Some([x0, y0, x1, y1]) = view.extent {
                let paper = |x: f64, y: f64| {
                    [
                        (view.rect.min_x + view.rect.max_x) / 2.0 + (x - view.center[0]) / view.scale,
                        (view.rect.min_y + view.rect.max_y) / 2.0 + (y - view.center[1]) / view.scale,
                    ]
                };
                let (low, high) = (paper(x0, y0), paper(x1, y1));
                if low[0] < view.rect.min_x - CLIP_TOLERANCE
                    || low[1] < view.rect.min_y - CLIP_TOLERANCE
                    || high[0] > view.rect.max_x + CLIP_TOLERANCE
                    || high[1] > view.rect.max_y + CLIP_TOLERANCE
                {
                    self.clipped += 1;
                }
            }
        }
        // 図形を置く場所（用紙の図形とビューの図形）
        let place_of = |entity: &DxfEntity, figures: &[Place]| -> Option<Place> {
            match entity.base.space {
                DxfSpace::Model => owners.get(entity.base.handle.as_str()).map(|index| figures[*index].clone()),
                DxfSpace::Paper => on_sheet(entity).then(|| sheet.clone()),
                DxfSpace::Block => None,
            }
        };
        let placed: Vec<(&DxfEntity, Place)> = doc
            .entities
            .iter()
            .filter_map(|entity| place_of(entity, &self.figures).map(|place| (entity, place)))
            .collect();
        self.blocks(&placed)?;
        for (entity, place) in &placed {
            self.entity(entity, place)?;
        }
        Ok(())
    }

    /// ビューの図形を置く場所。JWW はビューの縮尺のレイヤグループ、SXF はビューの部分図（viewport の操作で足す）
    fn view_place(&mut self, view: &DraftedView) -> Result<Place, DraftError> {
        let (width, height) = self.size;
        let viewport = [(view.rect.min_x + view.rect.max_x) / 2.0, (view.rect.min_y + view.rect.max_y) / 2.0];
        match &mut self.format {
            Format::Jww(_) => {
                let group = self.group_for(view.scale, &view.path)?;
                // 実寸の座標: (ビューポートの中心 − 用紙の中心) × 縮尺 + (モデル − ビューの中心)
                let s = view.scale;
                Ok(Place {
                    container: Some(format!("group:{group:X}")),
                    group,
                    offset: [
                        (viewport[0] - width / 2.0) * s - view.center[0],
                        (viewport[1] - height / 2.0) * s - view.center[1],
                    ],
                    paper: s,
                    inherit: None,
                })
            }
            Format::Sxf(sxf) => {
                let wanted = match view.name.as_deref().map(str::trim) {
                    Some(name) if !name.is_empty() && !name.starts_with("$$") => name.to_string(),
                    _ => format!("部分図 {}", sxf.figures.len() + 1),
                };
                let name = (1..)
                    .map(|number| if number == 1 { wanted.clone() } else { format!("{wanted} ({number})") })
                    .find(|name| !sxf.figure_exists(name))
                    .expect("使われていない名前");
                sxf.figures.insert(name.clone());
                let operation = json!({
                    "op": "add", "kind": "viewport", "name": name,
                    "center": viewport, "viewCenter": view.center, "scale": view.scale,
                });
                self.body.push((operation, view.path.clone()));
                Ok(Place {
                    container: Some(format!("figure:{name}")),
                    group: 0,
                    offset: [0.0; 2],
                    paper: 1.0,
                    inherit: None,
                })
            }
        }
    }

    /// JWW の、縮尺 scale のレイヤグループ。書込みレイヤグループ、同じ縮尺のレイヤグループ、図形のないレイヤグループ
    /// （縮尺を変える）の順に選ぶ。非表示・表示のみのレイヤグループは使わない
    fn group_for(&mut self, scale: f64, path: &str) -> Result<usize, DraftError> {
        let Format::Jww(jww) = &mut self.format else { return Ok(0) };
        let same = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(b.abs());
        let write = jww.write_group();
        let found = std::iter::once(write)
            .chain((0..16).filter(|group| *group != write))
            .find(|&group| jww.usable(group) && same(jww.scale(group), scale));
        if let Some(group) = found {
            jww.chosen.insert(group, jww.scale(group));
            return Ok(group);
        }
        let free = (0..16).find(|&group| {
            jww.usable(group) && !jww.chosen.contains_key(&group) && !jww.used.iter().any(|(used, _)| *used == group)
        });
        let Some(group) = free else {
            return Err(DraftError::new(
                "invalid-value",
                path,
                format!(
                    "JWW の図面に、縮尺 1:{} のレイヤグループも、図形のないレイヤグループもありません（非表示・表示のみのレイヤグループは使いません）",
                    js::number_to_string(scale)
                ),
            ));
        };
        jww.chosen.insert(group, scale);
        self.prelude.push((
            json!({"op": "update-layer", "name": format!("{group:X}-0"), "set": {"scale": scale}}),
            path.to_string(),
        ));
        Ok(group)
    }

    /// ブロックの定義と中身。描き込む先は用紙を 1 枚だけ持つので、その用紙が置くブロックだけを足す。中身は最初の配置の
    /// レイヤグループに置き、レイヤ 0 と BYBLOCK は最初の配置から継ぐ
    fn blocks(&mut self, placed: &[(&DxfEntity, Place)]) -> Result<(), DraftError> {
        let doc = self.doc;
        let members = |name: &str| {
            let upper = name.to_uppercase();
            doc.entities.iter().filter(move |entity| {
                entity.base.space == DxfSpace::Block
                    && entity.base.block.as_deref().is_some_and(|block| block.to_uppercase() == upper)
            })
        };
        // 最初の配置（用紙・ビューの配置、その中の配置の順）
        let mut queue: Vec<(String, usize, Look)> = Vec::new();
        for (entity, place) in placed {
            if let DxfEntityData::Insert(insert) = &entity.data {
                let look = self.resolve(&entity.base, place.inherit.as_ref());
                queue.push((insert.name.to_uppercase(), place.group, look));
            }
        }
        let mut index = 0;
        while index < queue.len() {
            let (name, group, look) = queue[index].clone();
            index += 1;
            if let Some(home) = self.homes.get(&name) {
                if home.inherit != look && self.inherits(&name) {
                    self.inherit_differs += 1;
                }
                continue;
            }
            self.homes.insert(name.clone(), Home { group, inherit: look.clone() });
            for member in members(&name) {
                if let DxfEntityData::Insert(insert) = &member.data {
                    let inner = self.resolve(&member.base, Some(&look));
                    queue.push((insert.name.to_uppercase(), group, inner));
                }
            }
        }
        // ブロックの定義（名前を先に足し、中身は後で入れる）
        let mut new_blocks = Vec::new();
        let placed_blocks = doc.blocks.iter().filter(|block| {
            !block.anonymous
                && !block.layout
                && !block.name.starts_with('*')
                && self.homes.contains_key(&block.name.to_uppercase())
        });
        for block in placed_blocks.collect::<Vec<_>>() {
            let key = block.name.to_uppercase();
            if let Some(existing) = self.template_block(&block.name) {
                self.info(
                    "draft-into-block-exists",
                    format!("ブロック {} は描き込む先にあるため、その定義を使いました", block.name),
                );
                self.blocks.insert(key, existing);
                continue;
            }
            self.body.push((
                json!({"op": "add-block", "name": block.name, "basePoint": [block.base_point[0], block.base_point[1]]}),
                String::new(),
            ));
            self.blocks.insert(key.clone(), block.name.clone());
            new_blocks.push(key);
        }
        for key in new_blocks {
            let home = self.homes[&key].clone();
            let scale = match &self.format {
                Format::Jww(jww) => jww.scale(home.group),
                Format::Sxf(_) => 1.0,
            };
            let place = Place {
                container: Some(format!("block:{}", self.blocks[&key])),
                group: home.group,
                offset: [0.0; 2],
                paper: scale,
                inherit: Some(home.inherit),
            };
            for member in members(&key).collect::<Vec<_>>() {
                self.entity(member, &place)?;
            }
        }
        Ok(())
    }

    /// ブロックが、配置から継ぐ図形（レイヤ 0・BYBLOCK）を持つか
    fn inherits(&self, name: &str) -> bool {
        let upper = name.to_uppercase();
        self.doc.entities.iter().any(|entity| {
            let base = &entity.base;
            base.space == DxfSpace::Block
                && base.block.as_deref().is_some_and(|block| block.to_uppercase() == upper)
                && (base.layer == "0"
                    || base.color_index == 0.0
                    || base.linetype.eq_ignore_ascii_case("ByBlock")
                    || base.lineweight == -2.0)
        })
    }

    /// 描き込む先にある、同じ名前のブロック（JWW は作図部品の印を除いた名前、SXF は複合図形）
    fn template_block(&self, name: &str) -> Option<String> {
        match &self.format {
            Format::Jww(jww) => jww.doc.blocks.iter().find_map(|block| {
                let plain = block.name.split("@@SfigorgFlag@@").next().unwrap_or(&block.name).trim();
                plain.eq_ignore_ascii_case(name.trim()).then(|| plain.to_string())
            }),
            Format::Sxf(sxf) => sxf.drawing.document.sub_figures.contains(name).then(|| name.to_string()),
        }
    }

    /// BYLAYER・BYBLOCK を辿った見た目。inherit はブロックの中で継ぐ見た目
    fn resolve(&self, base: &DxfEntityBase, inherit: Option<&Look>) -> Look {
        let layer_name = match inherit {
            Some(inherit) if base.layer == "0" => inherit.layer.clone(),
            _ => base.layer.clone(),
        };
        let layer = self.layers.get(&layer_name.to_uppercase()).copied();
        let foreground = || "#000000".to_string();
        let aci = |index: f64, true_color_value: Option<f64>| match true_color_value {
            Some(value) => true_color(value),
            None if index.abs() == 7.0 => foreground(),
            None => aci_color(index),
        };
        let color = if base.true_color.is_some() {
            aci(base.color_index, base.true_color)
        } else {
            match base.color_index as i64 {
                256 => layer.map_or_else(foreground, |layer| aci(layer.color_index, layer.true_color)),
                0 => inherit.map_or_else(foreground, |inherit| inherit.color.clone()),
                _ => aci(base.color_index, None),
            }
        };
        let line_type = match base.linetype.to_uppercase().as_str() {
            "BYLAYER" | "" => layer.map(|layer| layer.linetype.clone()),
            "BYBLOCK" => inherit.and_then(|inherit| inherit.line_type.clone()),
            _ => Some(base.linetype.clone()),
        }
        .filter(|name| !name.eq_ignore_ascii_case("Continuous") && !name.is_empty());
        let width = |lineweight: f64| (lineweight >= 0.0).then_some(lineweight / 100.0);
        let line_width = match base.lineweight as i64 {
            -1 => layer.and_then(|layer| width(layer.lineweight)),
            -2 => inherit.and_then(|inherit| inherit.line_width),
            _ => width(base.lineweight),
        };
        Look { layer: layer_name, color, line_type, line_width }
    }

    /// 書く線種（JWW は番号、SXF は名前）。図面にない線種は、線分・空白の長さの近いもの
    fn line_type(&mut self, name: &str) -> Option<Value> {
        let key = name.to_uppercase();
        if let Some(value) = self.line_types.get(&key) {
            return value.clone();
        }
        let pattern: Vec<f64> = self
            .doc
            .linetypes
            .iter()
            .find(|line_type| line_type.name.eq_ignore_ascii_case(name))
            .filter(|line_type| !line_type.complex)
            .map(|line_type| dash_array(&line_type.dashes, 1.0))
            .unwrap_or_default()
            .into_iter()
            .map(|length| if length == 0.0 { DOT } else { length })
            .collect();
        let value = if pattern.len() < 2 {
            None
        } else {
            let candidates: Vec<(Value, Vec<f64>)> = match &self.format {
                Format::Jww(jww) => {
                    // SXF 対応線種（31〜62）は版 4.20 から
                    let sxf = (31..=62u8).filter(|_| jww.doc.version >= 420);
                    (2..=9u8)
                        .chain(sxf)
                        .map(|style| (json!(style), jww.doc.line_dashes(style)))
                        .filter(|(_, dashes)| dashes.len() >= 2)
                        .collect()
                }
                Format::Sxf(sxf) => PREDEFINED_LINE_TYPES
                    .iter()
                    .skip(1)
                    .map(|line_type| (json!(line_type.name), line_type.pitch.to_vec()))
                    .chain(
                        sxf.drawing
                            .document
                            .user_line_types
                            .iter()
                            .filter(|line_type| line_type.pitch.len() >= 2)
                            .map(|line_type| (json!(line_type.name), line_type.pitch.clone())),
                    )
                    .collect(),
            };
            let same =
                |a: &[f64]| a.len() == pattern.len() && a.iter().zip(&pattern).all(|(x, y)| (x - y).abs() < 1e-6);
            match candidates.iter().find(|(_, dashes)| same(dashes)) {
                Some((value, _)) => Some(value.clone()),
                None => {
                    // 周期の比・線の割合・線分の数の近いもの（zumen-convert の dxf_to_jww と同じ測り方）
                    let signature = |segments: &[f64]| {
                        let period: f64 = segments.iter().sum();
                        let on: f64 = segments.iter().step_by(2).sum();
                        (period, on / period, segments.len() as f64)
                    };
                    let (period, duty, count) = signature(&pattern);
                    let distance = |segments: &[f64]| {
                        let (other, other_duty, other_count) = signature(segments);
                        (other / period).ln().abs()
                            + (other_duty - duty).abs() * 2.0
                            + (other_count - count).abs() * 0.1
                    };
                    let nearest = candidates
                        .iter()
                        .min_by(|a, b| distance(&a.1).total_cmp(&distance(&b.1)))
                        .map(|(value, _)| value.clone());
                    self.inexact_line_types.insert(key.clone());
                    nearest
                }
            }
        };
        self.line_types.insert(key, value.clone());
        value
    }

    /// 書くレイヤ（JWW は「0-A」の番号、SXF は名前）。仕様のレイヤがなければ足す
    fn layer(&mut self, name: &str, place: &Place, path: &str) -> Result<String, DraftError> {
        let spec_layer = self.layers.get(&name.to_uppercase()).copied();
        let (off, locked, plot) =
            spec_layer.map_or((false, false, true), |layer| (layer.off, layer.locked, layer.plot));
        match &mut self.format {
            Format::Jww(jww) => {
                let group = place.group;
                let key = (group, name.to_uppercase());
                if let Some(layer) = jww.layers.get(&key) {
                    return Ok(format!("{group:X}-{layer:X}"));
                }
                let names = |layer: usize| {
                    jww.doc.groups.get(group).and_then(|g| g.layers.get(layer)).map_or("", |l| l.name.trim())
                };
                let code = |layer: usize| format!("{group:X}-{layer:X}");
                // レイヤ 0 もほかの名前と同じ（書込みレイヤは描き込む先の図形のレイヤのことがある）
                let existing = (0..16)
                    .find(|&layer| names(layer) == name.trim())
                    .or_else(|| (0..16).find(|&layer| names(layer).eq_ignore_ascii_case(name.trim())));
                let layer = match existing {
                    Some(layer) => layer,
                    None => {
                        // 名前（か番号のままの名前）も図形もないレイヤ。書込みレイヤは非表示・表示のみにできない
                        let write = jww.write_layer(group);
                        let free = (0..16).find(|&layer| {
                            (names(layer).is_empty() || names(layer).eq_ignore_ascii_case(&code(layer)))
                                && !jww.used.contains(&(group, layer))
                                && !jww.named.contains(&(group, layer))
                                && !(layer == write && (off || locked))
                        });
                        let Some(layer) = free else {
                            return Err(DraftError::new(
                                "invalid-value",
                                path,
                                format!(
                                    "JWW のレイヤグループ {group:X} に、レイヤ {name} にできる名前も図形もないレイヤがありません"
                                ),
                            ));
                        };
                        jww.named.insert((group, layer));
                        let operation = json!({
                            "op": "update-layer", "name": code(layer),
                            "set": {"name": name, "visible": !off, "locked": locked},
                        });
                        self.prelude.push((operation, String::new()));
                        if !plot {
                            self.layer_values.insert(name.to_uppercase());
                        }
                        layer
                    }
                };
                jww.layers.insert(key, layer);
                Ok(code(layer))
            }
            Format::Sxf(sxf) => {
                let key = name.to_uppercase();
                if let Some(written) = sxf.layers.get(&key) {
                    return Ok(written.clone());
                }
                let layers = &sxf.drawing.document.layers;
                let existing = layers
                    .iter()
                    .find(|layer| layer.name == name)
                    .or_else(|| layers.iter().find(|layer| layer.name.eq_ignore_ascii_case(name)))
                    .map(|layer| layer.name.clone());
                let written = match existing {
                    Some(existing) => existing,
                    None => {
                        self.prelude.push((json!({"op": "add-layer", "name": name, "visible": !off}), String::new()));
                        if !plot || locked {
                            self.layer_values.insert(key.clone());
                        }
                        name.to_string()
                    }
                };
                sxf.layers.insert(key, written.clone());
                Ok(written)
            }
        }
    }

    /// 操作の共通の値（レイヤ・色・線種・線の太さ）。keys はその種類が受け取るもの
    fn common(
        &mut self,
        look: &Look,
        place: &Place,
        keys: &[&str],
        path: &str,
    ) -> Result<Map<String, Value>, DraftError> {
        let mut values = Map::new();
        values.insert("op".into(), json!("add"));
        if let Some(container) = &place.container {
            values.insert("container".into(), json!(container));
        }
        let layer = self.layer(&look.layer, place, path)?;
        values.insert("layer".into(), json!(layer));
        if keys.contains(&"color") {
            values.insert("color".into(), json!(look.color));
        }
        if keys.contains(&"lineType")
            && let Some(name) = &look.line_type
            && let Some(value) = self.line_type(name)
        {
            values.insert("lineType".into(), value);
        }
        if keys.contains(&"lineWidth")
            && let Some(width) = look.line_width
        {
            values.insert("lineWidth".into(), json!(width));
        }
        Ok(values)
    }

    fn push(&mut self, mut values: Map<String, Value>, kind: &str, data: Value, path: &str) {
        values.insert("kind".into(), json!(kind));
        if let Value::Object(data) = data {
            values.extend(data);
        }
        self.body.push((Value::Object(values), path.to_string()));
    }

    fn is_jww(&self) -> bool {
        matches!(self.format, Format::Jww(_))
    }

    /// 図形 1 つを操作にする
    fn entity(&mut self, entity: &DxfEntity, place: &Place) -> Result<(), DraftError> {
        let path = self.sources.get(&entity.base.handle).cloned().unwrap_or_default();
        let look = self.resolve(&entity.base, place.inherit.as_ref());
        const ALL: [&str; 3] = ["color", "lineType", "lineWidth"];
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                let values = self.common(&look, place, &ALL, &path)?;
                self.push(values, "line", json!({"start": place.at(start), "end": place.at(end)}), &path);
            }
            DxfEntityData::Circle { center, radius } => {
                let values = self.common(&look, place, &ALL, &path)?;
                self.push(values, "circle", json!({"center": place.at(center), "radius": radius}), &path);
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                let values = self.common(&look, place, &ALL, &path)?;
                let data = json!({"center": place.at(center), "radius": radius, "angle": [start_angle, end_angle]});
                self.push(values, "arc", data, &path);
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                let values = self.common(&look, place, &ALL, &path)?;
                let full = (end_param - start_param - TAU).abs() < 1e-9;
                let mut data =
                    json!({"center": place.at(center), "majorAxis": [major_axis[0], major_axis[1]], "ratio": ratio});
                if !full {
                    data["parameter"] = json!([start_param, end_param]);
                }
                self.push(values, if full { "ellipse" } else { "ellipse-arc" }, data, &path);
            }
            DxfEntityData::Point { location, .. } => {
                let keys: &[&str] = if self.is_jww() { &["color", "lineWidth"] } else { &["color"] };
                let values = self.common(&look, place, keys, &path)?;
                self.push(values, "point", json!({"position": place.at(location)}), &path);
            }
            DxfEntityData::LwPolyline(polyline) => self.polyline(polyline, &look, place, &path)?,
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, .. } => {
                if let Some(ring) = solid_ring([vtx0, vtx1, vtx2, vtx3]) {
                    let ring: Vec<[f64; 2]> = ring.iter().map(|p| place.at(p)).collect();
                    self.fill(vec![ring], &look, place, &path)?;
                }
            }
            DxfEntityData::Hatch(hatch) => {
                let keys: &[&str] = if hatch.solid_fill { &["color"] } else { &ALL };
                let values = self.common(&look, place, keys, &path)?;
                let data = hatch_value(hatch, &|p| place.at(p));
                self.push(values, "hatch", data, &path);
            }
            DxfEntityData::Text(text) => self.text(text, &look, place, &path)?,
            DxfEntityData::MText(mtext) => {
                let height = mtext.char_height;
                let factor = mtext_width_factor(&mtext.raw_text).unwrap_or(1.0);
                let spacing = if mtext.line_spacing_factor > 0.0 { mtext.line_spacing_factor } else { 1.0 };
                let lines: Vec<&str> = mtext.text.split('\n').collect();
                // 表示と同じ行の並び: 1 行目のベースラインは上端から高さ 1 つ下、行の間隔は高さの 5/3
                let baselines: Vec<f64> =
                    (0..lines.len()).map(|index| -height - index as f64 * height * LINE_SPACING * spacing).collect();
                let bottom = baselines[baselines.len() - 1] - TEXT_DESCENT * EM_LATIN * height;
                let attachment =
                    if (1.0..=9.0).contains(&mtext.attachment_point) { mtext.attachment_point } else { 1.0 };
                let column = ((attachment - 1.0) % 3.0) as usize;
                let row = ((attachment - 1.0) / 3.0).floor();
                let dy = if row == 0.0 {
                    0.0
                } else if row == 1.0 {
                    -bottom / 2.0
                } else {
                    -bottom
                };
                let style = self.font(&mtext.style);
                for (line, baseline) in lines.iter().zip(&baselines) {
                    let box_width = units(line) * height * factor;
                    let x = -[0.0, 0.5, 1.0][column] * box_width;
                    let at = rotate([x, baseline + dy - TEXT_DESCENT * height], mtext.rotation);
                    let position = [mtext.insert[0] + at[0], mtext.insert[1] + at[1]];
                    let text = TextLine { text: line, position, height, factor, rotation: mtext.rotation };
                    self.text_line(&text, style.as_deref(), &look, place, &path)?;
                }
            }
            DxfEntityData::Insert(insert) => {
                let values = self.common(&look, place, &[], &path)?;
                let name = self.blocks.get(&insert.name.to_uppercase()).cloned().unwrap_or_else(|| insert.name.clone());
                // JWW のブロックの中身は、最初の配置のレイヤグループの用紙の mm。配置の倍率はその縮尺との比を掛ける
                let factor = match &self.format {
                    Format::Jww(jww) => {
                        let home = self.homes.get(&insert.name.to_uppercase()).map_or(place.group, |home| home.group);
                        jww.scale(home) / place.paper
                    }
                    Format::Sxf(_) => 1.0,
                };
                let data = json!({
                    "block": name, "position": place.at(&insert.insert),
                    "scale": [insert.xscale * factor, insert.yscale * factor], "rotation": insert.rotation,
                });
                self.push(values, "block-reference", data, &path);
                // 属性の値は、表示と同じ所に文字で描く（JWW・SXF に属性はない）
                for attribute in &insert.attribs {
                    let DxfEntityData::Attrib { text, attribute_flags, .. } = &attribute.data else { continue };
                    if js::to_int32(*attribute_flags) & 1 != 0 || text.text.is_empty() {
                        continue;
                    }
                    self.attributes += 1;
                    let look = self.resolve(&attribute.base, place.inherit.as_ref());
                    self.text(text, &look, place, &path)?;
                }
            }
            DxfEntityData::Attdef { .. } => self.definitions += 1,
            DxfEntityData::Dimension(dimension) => self.dimension(dimension, &look, place, &path)?,
            DxfEntityData::Viewport(_) => {}
            _ => {
                self.info("draft-into-skipped", format!("{} は編集で足せないため、描きませんでした", entity.base.kind))
            }
        }
        Ok(())
    }

    /// 塗り（SOLID の hatch）
    fn fill(&mut self, rings: Vec<Vec<[f64; 2]>>, look: &Look, place: &Place, path: &str) -> Result<(), DraftError> {
        let values = self.common(look, place, &["color"], path)?;
        self.push(values, "hatch", json!({"boundaries": rings, "pattern": "SOLID"}), path);
        Ok(())
    }

    /// 折線。JWW は線と円弧、SXF は折線と円弧にする。幅のある折線は、線分ごとの塗りにする
    fn polyline(
        &mut self,
        polyline: &zumen_dxf::DxfLwPolyline,
        look: &Look,
        place: &Place,
        path: &str,
    ) -> Result<(), DraftError> {
        let vertices = &polyline.vertices;
        let points: Vec<[f64; 2]> = vertices.points.iter().map(|p| place.at(p)).collect();
        let count = if vertices.closed { points.len() } else { points.len().saturating_sub(1) };
        let bulge = |index: usize| vertices.bulges.get(index).copied().unwrap_or(0.0);
        let width = |index: usize| -> [f64; 2] {
            match vertices.widths.get(index) {
                Some(&[start, end]) if start > 0.0 || end > 0.0 => [start, end],
                _ => {
                    let width = polyline.const_width.unwrap_or(0.0);
                    [width, width]
                }
            }
        };
        let wide = (0..count).any(|index| width(index).iter().any(|w| *w > 0.0));
        let all = ["color", "lineType", "lineWidth"];
        if wide {
            for index in 0..count {
                let (a, b) = (points[index], points[(index + 1) % points.len()]);
                let [w0, w1] = width(index);
                let ring = match arc_of(a, b, bulge(index)) {
                    None => {
                        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                        let length = dx.hypot(dy);
                        if length == 0.0 {
                            continue;
                        }
                        let n = [-dy / length, dx / length];
                        vec![
                            [a[0] + n[0] * w0 / 2.0, a[1] + n[1] * w0 / 2.0],
                            [b[0] + n[0] * w1 / 2.0, b[1] + n[1] * w1 / 2.0],
                            [b[0] - n[0] * w1 / 2.0, b[1] - n[1] * w1 / 2.0],
                            [a[0] - n[0] * w0 / 2.0, a[1] - n[1] * w0 / 2.0],
                        ]
                    }
                    Some(arc) => arc.band(w0.max(w1)),
                };
                self.fill(vec![ring], look, place, path)?;
            }
            return Ok(());
        }
        // 弧のない線分の続きと、弧
        let mut run: Vec<[f64; 2]> = Vec::new();
        let flush = |planner: &mut Self, run: &mut Vec<[f64; 2]>, closed: bool| -> Result<(), DraftError> {
            if run.len() < 2 {
                run.clear();
                return Ok(());
            }
            if planner.is_jww() {
                let pairs: Vec<([f64; 2], [f64; 2])> = run.windows(2).map(|pair| (pair[0], pair[1])).collect();
                for (start, end) in pairs {
                    let values = planner.common(look, place, &all, path)?;
                    planner.push(values, "line", json!({"start": start, "end": end}), path);
                }
            } else {
                let values = planner.common(look, place, &all, path)?;
                let mut vertices = run.clone();
                if closed {
                    vertices.pop();
                }
                planner.push(values, "polyline", json!({"vertices": vertices, "closed": closed}), path);
            }
            run.clear();
            Ok(())
        };
        let straight = (0..count).all(|index| bulge(index) == 0.0);
        for index in 0..count {
            let (a, b) = (points[index], points[(index + 1) % points.len()]);
            match arc_of(a, b, bulge(index)) {
                None => {
                    if run.is_empty() {
                        run.push(a);
                    }
                    run.push(b);
                }
                Some(arc) => {
                    flush(self, &mut run, false)?;
                    let values = self.common(look, place, &all, path)?;
                    let data = json!({"center": arc.center, "radius": arc.radius, "angle": [arc.from, arc.to]});
                    self.push(values, "arc", data, path);
                }
            }
        }
        flush(self, &mut run, vertices.closed && straight)?;
        Ok(())
    }

    /// 1 行の文字（TEXT・ATTRIB）。文字範囲の左下を、表示と同じ位置合わせで決める
    fn text(&mut self, text: &DxfTextBase, look: &Look, place: &Place, path: &str) -> Result<(), DraftError> {
        let factor = if text.width > 0.0 { text.width } else { 1.0 };
        let plain: String = parse_text_codes(&text.text, text.height, factor, text.oblique)
            .iter()
            .map(|run| run.text.as_str())
            .collect();
        let height = text.height;
        let box_width = units(&plain) * height * factor;
        let (h, v) = (text.halign, text.valign);
        let point = if h == 0.0 && v == 0.0 { text.insert } else { text.align_point.unwrap_or(text.insert) };
        let ax = if h == 1.0 || h == 4.0 {
            box_width / 2.0
        } else if h == 2.0 {
            box_width
        } else {
            0.0
        };
        // 位置合わせの点からベースラインまで（DXF Reference「TEXT」の 72・73、表示と同じ）
        let ay = if h == 4.0 || v == 2.0 {
            height / 2.0
        } else if v == 1.0 {
            -TEXT_DESCENT * EM_LATIN * height
        } else if v == 3.0 {
            height
        } else {
            0.0
        };
        // 文字範囲の下端は、ベースラインから高さの 0.12 下（決定 0019、zumen-convert の text_height_box と同じ）
        let at = rotate([-ax, -ay - TEXT_DESCENT * height], text.rotation);
        let position = [point[0] + at[0], point[1] + at[1]];
        let style = self.font(&text.style);
        let line = TextLine { text: &plain, position, height, factor, rotation: text.rotation };
        self.text_line(&line, style.as_deref(), look, place, path)
    }

    /// 文字範囲の左下に置く 1 行の文字
    fn text_line(
        &mut self,
        line: &TextLine<'_>,
        font: Option<&str>,
        look: &Look,
        place: &Place,
        path: &str,
    ) -> Result<(), DraftError> {
        if line.text.trim().is_empty() {
            return Ok(());
        }
        let values = self.common(look, place, &["color"], path)?;
        let position = place.at(&line.position);
        let mut data = json!({"position": position, "text": line.text, "rotation": line.rotation});
        if self.is_jww() {
            // JWW の文字の大きさは用紙の mm。文字の幅は全角 1 文字の幅
            data["height"] = json!(line.height / place.paper);
            data["width"] = json!(line.height * line.factor / place.paper);
            if let Some(font) = font {
                data["font"] = json!(font);
            }
        } else {
            data["height"] = json!(line.height);
            data["width"] = json!(units(line.text) * line.height * line.factor);
            if let Some(font) = font {
                data["textStyle"] = json!(font);
            }
        }
        self.push(values, "text", data, path);
        Ok(())
    }

    /// 文字スタイルのフォントの名前
    fn font(&self, style: &str) -> Option<String> {
        let style = self.doc.styles.iter().find(|candidate| candidate.name.eq_ignore_ascii_case(style))?;
        let name = style.font_family.clone().unwrap_or_else(|| style.font.clone());
        (!name.trim().is_empty()).then_some(name)
    }

    /// 寸法。JWW の角度寸法は、表示のブロックの線・円弧・文字で描く
    fn dimension(
        &mut self,
        dimension: &zumen_dxf::DxfDimension,
        look: &Look,
        place: &Place,
        path: &str,
    ) -> Result<(), DraftError> {
        let kind = dimension.dimtype as i64 & 7;
        if self.is_jww() && matches!(kind, 2 | 5) {
            self.angular += 1;
            let block = dimension.anonymous_block_name.to_uppercase();
            let parts: Vec<&DxfEntity> = self
                .doc
                .entities
                .iter()
                .filter(|part| {
                    part.base.space == DxfSpace::Block
                        && part.base.block.as_deref().is_some_and(|name| name.to_uppercase() == block)
                })
                .collect();
            // 表示のブロックの図形は寸法の見た目を継ぐ（BYBLOCK）
            let inner = Place { inherit: Some(look.clone()), ..place.clone() };
            for part in parts {
                self.entity(part, &inner)?;
            }
            return Ok(());
        }
        let p = |point: &[f64; 3]| place.at(point);
        let mut data = match kind {
            0 | 1 => {
                let (Some(start), Some(end)) = (dimension.defpoint2, dimension.defpoint3) else { return Ok(()) };
                let aligned = kind == 1;
                let mut data = json!({
                    "dimensionType": if aligned { "aligned" } else { "linear" },
                    "start": p(&start), "end": p(&end), "position": p(&dimension.defpoint),
                });
                if !aligned {
                    data["rotation"] = json!(dimension.angle.unwrap_or(0.0));
                }
                data
            }
            2 | 5 => {
                let (Some(start), Some(end), Some(center)) =
                    (dimension.defpoint2, dimension.defpoint3, dimension.defpoint4)
                else {
                    return Ok(());
                };
                json!({
                    "dimensionType": "angular-3-point", "center": p(&center), "start": p(&start), "end": p(&end),
                    "position": p(&dimension.defpoint),
                })
            }
            3 | 4 => {
                let Some(point) = dimension.defpoint4 else { return Ok(()) };
                let diameter = kind == 3;
                let center = if diameter {
                    [(dimension.defpoint[0] + point[0]) / 2.0, (dimension.defpoint[1] + point[1]) / 2.0, 0.0]
                } else {
                    dimension.defpoint
                };
                json!({"dimensionType": if diameter { "diameter" } else { "radius" }, "center": p(&center), "point": p(&point)})
            }
            _ => return Ok(()),
        };
        data["dimensionStyle"] = json!(dimension.dimstyle);
        // 利用者の文字（DXF の書き方を戻す）。<> は測った値
        let user: String =
            parse_text_codes(&dimension.text, 1.0, 1.0, 0.0).iter().map(|run| run.text.as_str()).collect();
        match &self.format {
            // JWW の値は、描き込む先の寸法の設定で書く（Jw_cad の寸法と同じ）
            Format::Jww(_) => {
                if !user.is_empty() {
                    data["text"] = json!(user);
                }
            }
            // SXF に寸法の設定はないので、仕様の書き方（桁・3 桁の区切り）の値を渡す
            Format::Sxf(_) => {
                let look = self.looks.get(&dimension.dimstyle).map(|(_, look)| look);
                if let (Some(look), Some(measured)) = (look, dimension.actual_measurement) {
                    let value = match kind {
                        2 | 5 => format!("{}°", format_number(measured.to_degrees(), look.precision, false)),
                        3 => format!("φ{}", format_number(measured, look.precision, look.thousands)),
                        4 => format!("R{}", format_number(measured, look.precision, look.thousands)),
                        _ => format_number(measured, look.precision, look.thousands),
                    };
                    let shown = if user.is_empty() { value.clone() } else { user.replace("<>", &value) };
                    data["text"] = json!(shown);
                } else if !user.is_empty() {
                    data["text"] = json!(user);
                }
            }
        }
        let values = self.common(look, place, &["color", "lineType", "lineWidth"], path)?;
        self.push(values, "dimension", data, path);
        Ok(())
    }
}

/// 置く 1 行の文字（位置は文字範囲の左下、置く前の座標）
struct TextLine<'t> {
    text: &'t str,
    position: [f64; 2],
    height: f64,
    factor: f64,
    rotation: f64,
}

/// 全角を 1、半角を 0.5 とした文字の数
fn units(text: &str) -> f64 {
    text.chars().fold(0.0, |total, c| total + if is_half_width(c) { 0.5 } else { 1.0 })
}

fn rotate(p: [f64; 2], degrees: f64) -> [f64; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
}

/// 折線の弧（中心、半径、反時計回りの始角・終角）
struct Arc {
    center: [f64; 2],
    radius: f64,
    from: f64,
    to: f64,
    /// 反時計回りに回る角度（度）
    sweep: f64,
}

impl Arc {
    /// 幅 width の弧の帯（内側の半径が 0 以下なら、中心までの扇形）
    fn band(&self, width: f64) -> Vec<[f64; 2]> {
        let steps = ((self.sweep / 360.0 * ARC_SEGMENTS).ceil() as usize).max(2);
        let point = |radius: f64, step: usize| {
            let angle = (self.from + self.sweep * step as f64 / steps as f64).to_radians();
            [self.center[0] + radius * angle.cos(), self.center[1] + radius * angle.sin()]
        };
        let (outer, inner) = (self.radius + width / 2.0, self.radius - width / 2.0);
        let mut ring: Vec<[f64; 2]> = (0..=steps).map(|step| point(outer, step)).collect();
        if inner > 1e-9 {
            ring.extend((0..=steps).rev().map(|step| point(inner, step)));
        } else {
            ring.push(self.center);
        }
        ring
    }
}

/// a から b への膨らみ bulge（tan(中心角 / 4)、正は反時計回り）の弧。膨らみが 0 なら None
fn arc_of(a: [f64; 2], b: [f64; 2], bulge: f64) -> Option<Arc> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let chord = dx.hypot(dy);
    if bulge == 0.0 || !bulge.is_finite() || chord == 0.0 {
        return None;
    }
    // 中心は弦の中点から、左の法線の向きに (1 − b²) / (4b) × 弦の長さ
    let k = (1.0 - bulge * bulge) / (4.0 * bulge);
    let center = [(a[0] + b[0]) / 2.0 - k * dy, (a[1] + b[1]) / 2.0 + k * dx];
    let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge.abs());
    let angle = |p: [f64; 2]| (p[1] - center[1]).atan2(p[0] - center[0]).to_degrees();
    let (from, to) = if bulge > 0.0 { (angle(a), angle(b)) } else { (angle(b), angle(a)) };
    let sweep = (4.0 * bulge.abs().atan()).to_degrees();
    Some(Arc { center, radius, from, to, sweep })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_bulges_into_counterclockwise_arcs() {
        // (1, 0) から (0, 1) への 1/4 円は、反時計回りなら中心が原点、時計回りなら (1, 1)
        let quarter = (std::f64::consts::PI / 8.0).tan();
        let ccw = arc_of([1.0, 0.0], [0.0, 1.0], quarter).unwrap();
        assert!(ccw.center[0].abs() < 1e-12 && ccw.center[1].abs() < 1e-12);
        assert!((ccw.radius - 1.0).abs() < 1e-12 && (ccw.sweep - 90.0).abs() < 1e-9);
        assert!((ccw.from - 0.0).abs() < 1e-9 && (ccw.to - 90.0).abs() < 1e-9);
        let cw = arc_of([1.0, 0.0], [0.0, 1.0], -quarter).unwrap();
        assert!((cw.center[0] - 1.0).abs() < 1e-12 && (cw.center[1] - 1.0).abs() < 1e-12);
        assert!((cw.from - 180.0).abs() < 1e-9 && (cw.to + 90.0).abs() < 1e-9);
        assert!(arc_of([0.0, 0.0], [1.0, 0.0], 0.0).is_none());
        // 幅が直径と同じ半円の帯は、中心までの半円（作図キットの黒丸）
        let half = arc_of([-1.0, 0.0], [1.0, 0.0], 1.0).unwrap();
        let ring = half.band(2.0);
        assert_eq!(ring.last(), Some(&half.center));
        assert!(ring[..ring.len() - 1].iter().all(|p| (p[0].hypot(p[1]) - 2.0).abs() < 1e-9));
    }
}
