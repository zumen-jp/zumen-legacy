//! AutoCAD の図面のモデル（`DxfDocument`）を SXF の図面（`SxfDrawing`）に写す（決定 0006・0007・0008・0010、
//! docs/formats/sxf.md「Converting to SXF」）。SXF から DXF への写し方（sxf.rs）の逆で、表示（zumen-dxf の scene.rs）と
//! 同じ順・同じ規則で辿り、表示と同じ所に同じ形を描く SXF にする。
//!
//! - 用紙: 表示する空間（`dxf_to_scene` の既定の表示）がレイアウトなら、その紙空間が SXF の用紙（大きさは A0〜A4 の縦・
//!   横か自由）で、紙空間の図形は用紙の要素。ビューポートは、映るモデル空間の図形を実寸（mm）で持つ部分図
//!   （数学座標系）にし、ビューポートの位置・縮尺・ねじれで配置する（配置はレイヤ 0、どのレイヤにも属さない）。同じ図形を映すビューポートは 1 つの部分図を
//!   2 か所に配置する。1:1 で原点を動かさないビューポートの図形は、用紙にじかに置く（部分図のない SXF の図面を
//!   DXF にしたもの）。モデル空間を表示する図面は、モデル空間を 1:1 の部分図にし、図形の範囲の大きさの用紙に置く。
//! - ブロック参照（INSERT）は作図部品（複合図形定義と配置）。BYBLOCK の色・線種・線の太さ、レイヤ 0 の図形は配置から
//!   継ぐので、継ぐ値の違う配置ごとに作図部品を作る。寸法は表示のブロックの図形を、寸法の図形として写す。
//! - 線分・円・円弧・楕円・折線（弧の膨らみは円弧）・スプライン（3 次ベジェ曲線に分ける）・点はそのまま写す。
//!   文字・複数行文字・属性・引出線・塗り（SOLID・HATCH）などは、表示が描く線・塗り・文字を SXF の要素にする。
//!   ハッチングの模様は、HATCH の模様の線の組を SXF のハッチング（ユーザ定義）にする。

use crate::shapes::{bezier, bulge_arc, decompose, is_similar, length_scale, map_arc, mirrors, normalize, regions};
use crate::text::{is_vertical, text_height_box};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use zumen_core::geometry::{IDENTITY, apply, apply_vector, invert, multiply};
use zumen_core::{BACKGROUND_COLOR, Matrix, Notice, NoticeLevel, NoticeList, Primitive, Scene, js};
use zumen_dxf::{
    DxfDocument, DxfEntity, DxfEntityBase, DxfEntityData, DxfHatch, DxfLayout, DxfSpace, DxfViewport, aci_color,
    dash_array, draw_order, drawing_unit, dxf_to_scene, is_transparent, minor_axis, ocs_matrix, true_color,
};
use zumen_sxf::tables::{PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, PREDEFINED_WIDTHS};
use zumen_sxf::{
    Annotation, CompositeCurve, CompositeCurveOrg, DrawingSheet, HatchPattern, LayerDefinition, Region, SfigOrg,
    Styled, SubFigure, SubFigures, SxfDocument, SxfDrawing, SxfFeature, SxfHeader, SxfWritten, TextFont, UserColour,
    UserLineType, WidthDefinition, write_sfc, written_ids,
};

/// ブロックの入れ子の上限（表示と同じ）
const MAX_DEPTH: usize = 32;
/// 線の太さの既定（表示と同じ、mm）
const DEFAULT_LINEWEIGHT: f64 = 0.25;
/// 点の長さ 0 の線種の線分を書く長さ（用紙の mm。SXF の既定義線種の点と同じ）
const DOT: f64 = 0.25;

/// 写した SXF の図面と、写せなかった・形を変えて写した内容
#[derive(Debug, Clone, PartialEq)]
pub struct MappedSxf {
    pub drawing: SxfDrawing,
    pub notices: Vec<Notice>,
    /// SXF のインスタンス番号（#n）→ 元の図形の表示の id の部分（ハンドル。ビューポートの配置は「ハンドル:」、
    /// ブロック参照の配置は「ハンドル>」、属性は「ブロック参照のハンドル>属性のハンドル」）。配置の中の要素の id は、
    /// 配置から順につなぐと元の表示の id になる
    pub sources: HashMap<String, String>,
}

impl MappedSxf {
    /// SFC に書く。書いたインスタンス番号での sources も返す
    pub fn write(&self) -> (SxfWritten, HashMap<String, String>) {
        let written = write_sfc(&self.drawing);
        let ids = written_ids(&written);
        let sources = self
            .sources
            .iter()
            .filter_map(|(id, source)| {
                let number: f64 = id.strip_prefix('#')?.parse().ok()?;
                let new = ids.get(&number.to_bits())?;
                Some((format!("#{}", js::number_to_string(*new)), source.clone()))
            })
            .collect();
        (written, sources)
    }
}

/// AutoCAD の図面のモデルを SXF の図面に写す
pub fn dxf_to_sxf(doc: &DxfDocument) -> MappedSxf {
    let handled;
    let doc = if needs_handles(doc) {
        handled = with_handles(doc);
        &handled
    } else {
        doc
    };
    Mapper::new(doc).run()
}

/// ハンドルのない・重なる図形があるか（R12 の DXF）
pub(crate) fn needs_handles(doc: &DxfDocument) -> bool {
    let mut seen = HashSet::new();
    doc.entities.iter().flat_map(with_attribs).any(|base| base.handle.is_empty() || !seen.insert(base.handle.as_str()))
}

/// 図形と、ブロック参照に続く属性
fn with_attribs(entity: &DxfEntity) -> Vec<&DxfEntityBase> {
    let mut out = vec![&entity.base];
    if let DxfEntityData::Insert(insert) = &entity.data {
        out.extend(insert.attribs.iter().map(|attribute| &attribute.base));
    }
    out
}

pub(crate) fn with_handles(doc: &DxfDocument) -> DxfDocument {
    let mut out = doc.clone();
    let mut seen = HashSet::new();
    let mut next = 0;
    let mut assign = |base: &mut DxfEntityBase| {
        if base.handle.is_empty() || !seen.insert(base.handle.clone()) {
            next += 1;
            base.handle = format!("ZH{next}");
            seen.insert(base.handle.clone());
        }
    };
    for entity in &mut out.entities {
        assign(&mut entity.base);
        if let DxfEntityData::Insert(insert) = &mut entity.data {
            for attribute in &mut insert.attribs {
                assign(&mut attribute.base);
            }
        }
    }
    out
}

/// 色。前景色（ACI 7）は SXF の黒、背景色の塗り（ワイプアウト）は白
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Paint {
    Foreground,
    Background,
    Rgb([u8; 3]),
}

fn rgb_of(hex: &str) -> Option<[u8; 3]> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() < 6 || !digits.is_ascii() {
        return None;
    }
    let part = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// 表示の色（#rrggbb）。白は ACI 7（前景色）の表示の色なので前景色にする
pub(crate) fn paint_of_hex(hex: &str) -> Paint {
    if hex == BACKGROUND_COLOR {
        return Paint::Background;
    }
    match rgb_of(hex) {
        Some([255, 255, 255]) | None => Paint::Foreground,
        Some(rgb) => Paint::Rgb(rgb),
    }
}

pub(crate) fn paint_of_aci(index: f64) -> Paint {
    if index == 7.0 { Paint::Foreground } else { paint_of_hex(&aci_color(index)) }
}

/// ブロック参照・寸法の中の図形が継ぐ値（BYBLOCK の色・線の太さ（mm）・線種、レイヤ 0 の図形のレイヤ）
#[derive(Debug, Clone, PartialEq)]
struct Inherit {
    paint: Paint,
    lineweight: f64,
    linetype: String,
    layer: Option<String>,
}

impl Inherit {
    fn key(&self) -> InheritKey {
        (self.paint, self.lineweight.to_bits(), self.linetype.to_uppercase(), self.layer.clone())
    }
}

/// 辿るときの文脈
#[derive(Debug, Clone)]
struct Ctx {
    /// 図形の座標（DXF）→ 入れ物（用紙・部分図・作図部品）の座標（mm）
    m: Matrix,
    /// 入れ物の座標 → 用紙の mm（線種のピッチ）
    to_paper: Matrix,
    /// 線種の長さの補正（表示と同じ。紙空間で線種の尺度を揃えるビューポートは縮尺の逆数）
    dash_factor: f64,
    inherit: Inherit,
    /// 辿っているブロック（循環を断つ）
    stack: Vec<String>,
    /// 寸法の中: ブロック参照も作図部品にせず、寸法の図形として写す
    flatten: bool,
    /// 要素の元の id の前に付けるもの（用紙にじかに置くビューポートの「ハンドル:」）
    prefix: String,
    /// すべての要素の元の id（寸法の中の図形は寸法の id）
    source: Option<String>,
    /// ビューポートの枠（用紙の mm）。構築線・放射線を切る
    clip: Option<[f64; 4]>,
}

impl Ctx {
    fn source(&self, base: &DxfEntityBase) -> String {
        self.source.clone().unwrap_or_else(|| format!("{}{}", self.prefix, base.handle))
    }
}

/// 入れ物の要素。表示が描く形から写す図形は、表示を作った後で要素にする
enum Item {
    Feature(Box<SxfFeature>, String),
    Pending(usize),
}

impl Item {
    fn feature(feature: SxfFeature, source: String) -> Item {
        Item::Feature(Box::new(feature), source)
    }
}

/// 継ぐ値の比べる形（色、線の太さ、線種、レイヤ 0 の継ぎ先）
type InheritKey = (Paint, u64, String, Option<String>);

/// 文字範囲の変換を分けた値（配置基点の x・y、角度、X の倍率、斜体のずれと Y の倍率）
pub(crate) type TextPlacement = (f64, f64, f64, f64, (f64, f64));

/// 表示が描く形から写す図形
struct Pending {
    /// 継ぐ値を入れた図形（ハンドルは表示の中で見分ける仮のもの）
    entity: DxfEntity,
    m: Matrix,
    /// 表示の線種の長さ → 用紙の mm
    dash_scale: f64,
    /// 入れ物の座標の長さ → 用紙の mm（ハッチングの模様の破線は図形と同じく縮尺で変わる）
    paper_scale: f64,
    layer: f64,
    linetype: String,
    font: f64,
    source: String,
    paint: Paint,
    /// 線の太さ（mm）
    lineweight: f64,
    /// 模様のハッチング（表示には塗りとして描かせ、輪郭を領域にする）
    hatch: Option<DxfHatch>,
}

/// 写す途中の SXF の表（コードは出現順）
#[derive(Default)]
struct Tables {
    layers: Vec<LayerDefinition>,
    layer_codes: HashMap<String, f64>,
    colors: Vec<UserColour>,
    color_codes: HashMap<[u8; 3], f64>,
    line_types: Vec<UserLineType>,
    line_type_codes: HashMap<(String, Vec<u64>), f64>,
    widths: Vec<WidthDefinition>,
    width_codes: HashMap<u64, f64>,
    fonts: Vec<TextFont>,
    font_codes: HashMap<String, f64>,
}

struct Mapper<'a> {
    doc: &'a DxfDocument,
    unit: f64,
    order: zumen_dxf::DrawOrder<'a>,
    tables: Tables,
    notices: NoticeList,
    next_id: f64,
    sources: HashMap<String, String>,
    sheet_items: Vec<Item>,
    figures: Vec<(SfigOrg, Vec<Item>)>,
    figure_names: HashSet<String>,
    /// (ブロックの名前（大文字）, 継ぐ値) → 作図部品の名前
    variants: HashMap<(String, Option<InheritKey>), String>,
    inherits: HashMap<String, bool>,
    pending: Vec<Pending>,
    composite_curves: Vec<CompositeCurve>,
    layers_upper: HashMap<String, &'a zumen_dxf::DxfLayer>,
    /// 用紙の範囲（用紙の mm）。構築線・放射線を切る（表示の finish_infinite と同じ）
    limit: [f64; 4],
    linetypes_upper: HashMap<String, &'a zumen_dxf::DxfLinetype>,
}

/// 用紙の大きさ（A0〜A4 の縦長の幅・高さ、mm）
const SHEETS: [(f64, f64, f64); 5] =
    [(0.0, 841.0, 1189.0), (1.0, 594.0, 841.0), (2.0, 420.0, 594.0), (3.0, 297.0, 420.0), (4.0, 210.0, 297.0)];

/// 用紙の大きさを SXF の用紙サイズ種別（0〜4: A0〜A4、9: 自由）と向き（0: 縦、1: 横）にする
fn sheet_of(name: &str, width: f64, height: f64) -> DrawingSheet {
    for (kind, short, long) in SHEETS {
        let close = |a: f64, b: f64| (a - b).abs() <= 0.5;
        if close(width, short) && close(height, long) {
            return DrawingSheet { id: 0.0, name: name.into(), sheet_type: kind, orient: 0.0, x: 0.0, y: 0.0 };
        }
        if close(width, long) && close(height, short) {
            return DrawingSheet { id: 0.0, name: name.into(), sheet_type: kind, orient: 1.0, x: 0.0, y: 0.0 };
        }
    }
    let size = |value: f64| if value.is_finite() && value > 0.0 { value } else { 1.0 };
    DrawingSheet { id: 0.0, name: name.into(), sheet_type: 9.0, orient: 1.0, x: size(width), y: size(height) }
}

/// 表示の id の先頭（> の前。ビューポートの中は : の後）
pub(crate) fn top_id(id: &str) -> &str {
    let id = id.split_once(':').map_or(id, |(_, rest)| rest);
    id.split('>').next().unwrap_or(id)
}

impl<'a> Mapper<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        Mapper {
            doc,
            unit: drawing_unit(doc),
            order: draw_order(doc),
            tables: Tables::default(),
            notices: NoticeList::new(),
            next_id: 0.0,
            sources: HashMap::new(),
            sheet_items: Vec::new(),
            figures: Vec::new(),
            figure_names: HashSet::new(),
            variants: HashMap::new(),
            inherits: HashMap::new(),
            pending: Vec::new(),
            composite_curves: Vec::new(),
            limit: [0.0; 4],
            layers_upper: doc.layers.iter().map(|layer| (layer.name.to_uppercase(), layer)).collect(),
            linetypes_upper: doc.linetypes.iter().map(|linetype| (linetype.name.to_uppercase(), linetype)).collect(),
        }
    }

    fn id(&mut self) -> f64 {
        self.next_id += 1.0;
        self.next_id
    }

    fn root_inherit() -> Inherit {
        Inherit { paint: Paint::Foreground, lineweight: DEFAULT_LINEWEIGHT, linetype: "CONTINUOUS".into(), layer: None }
    }

    fn run(mut self) -> MappedSxf {
        let doc = self.doc;
        let view = self.default_view();
        let layout = doc.layouts.iter().find(|layout| !layout.model && layout.name == view);
        let sheet = match layout {
            Some(layout) => self.layout(layout),
            None => self.model_sheet(),
        };
        let others = doc
            .layouts
            .iter()
            .filter(|other| !other.model && Some(other.name.as_str()) != layout.map(|l| l.name.as_str()));
        let others = others
            .filter(|other| self.order.blocks.get(&other.block.to_uppercase()).is_some_and(|list| !list.is_empty()))
            .count();
        if others > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "layouts",
                "SXF の用紙は 1 枚なので、開くレイアウトのほかのレイアウトは書いていません",
                others as u64,
            );
        }
        self.finish(sheet)
    }

    /// 表示（dxf_to_scene）の既定の表示と同じ空間
    fn default_view(&self) -> String {
        let doc = self.doc;
        let has_content = |layout: &DxfLayout| {
            self.order.blocks.get(&layout.block.to_uppercase()).is_some_and(|entities| !entities.is_empty())
        };
        let paper: Vec<&DxfLayout> = doc.layouts.iter().filter(|layout| !layout.model).collect();
        if doc.header.tilemode == 0.0
            && let Some(active) = paper.iter().find(|layout| layout.active)
            && has_content(active)
        {
            return active.name.clone();
        }
        if !self.order.model.is_empty() {
            return "Model".into();
        }
        paper.iter().find(|layout| has_content(layout)).map_or_else(|| "Model".into(), |layout| layout.name.clone())
    }

    // --- 用紙 ------------------------------------------------------------------------------------------

    /// モデル空間を表示する図面: モデル空間を 1:1 の部分図にし、図形の範囲の大きさの用紙に置く
    fn model_sheet(&mut self) -> DrawingSheet {
        let scene = dxf_to_scene(self.doc, Some("Model"));
        let shift = scene
            .entities
            .first()
            .and_then(|entity| entity.placement.as_ref())
            .map_or([0.0, 0.0], |placement| [placement.inverse[4], placement.inverse[5]]);
        let b = scene.bounds;
        if !(b.min_x <= b.max_x && b.min_y <= b.max_y) {
            return sheet_of("Model", 1.0, 1.0);
        }
        self.limit = [0.0, 0.0, b.max_x - b.min_x, b.max_y - b.min_y];
        self.notices.add(
            NoticeLevel::Info,
            "model-sheet",
            "レイアウトを開かない図面は、モデル空間を 1:1 の部分図にし、図形の範囲の大きさの用紙（自由）に置きました",
        );
        let unit = self.unit;
        // 表示の座標 = 実寸 − 原点の寄せ。用紙の座標は、表示の範囲の左下を原点にする
        let to_paper: Matrix = [1.0, 0.0, 0.0, 1.0, -shift[0] - b.min_x, -shift[1] - b.min_y];
        let ctx = Ctx {
            m: [unit, 0.0, 0.0, unit, 0.0, 0.0],
            to_paper,
            dash_factor: 1.0,
            inherit: Self::root_inherit(),
            stack: Vec::new(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
        };
        let model = self.order.model.clone();
        let mut items = Vec::new();
        for entity in &model {
            self.emit(entity, &ctx, &mut items);
        }
        let name = self.unique_figure("モデル空間");
        self.push_figure(&name, 1.0, items);
        self.locate(&name, 0.0, &to_paper, String::new(), None);
        sheet_of("Model", b.max_x - b.min_x, b.max_y - b.min_y)
    }

    /// レイアウトを開く図面: 紙空間は用紙、ビューポートは部分図
    fn layout(&mut self, layout: &DxfLayout) -> DrawingSheet {
        let doc = self.doc;
        // 表示と同じ用紙の大きさと、紙空間の座標 → 用紙の mm（scene.rs の dxf_to_scene）
        let paper_unit = (if layout.paper_units == 0.0 { 25.4 } else { 1.0 })
            * (if layout.plot_scale[1] > 0.0 && layout.plot_scale[0] > 0.0 {
                layout.plot_scale[0] / layout.plot_scale[1]
            } else {
                1.0
            });
        let rotated = layout.plot_rotation == 1.0 || layout.plot_rotation == 3.0;
        let (mut left, mut bottom) = (layout.margins[0], layout.margins[1]);
        if rotated {
            (left, bottom) = (bottom, left);
        }
        let mut width = if rotated { layout.paper_height } else { layout.paper_width };
        let mut height = if rotated { layout.paper_width } else { layout.paper_height };
        let mut ox = left + layout.plot_origin[0];
        let mut oy = bottom + layout.plot_origin[1];
        if !(width > 0.0 && height > 0.0) {
            let [x0, y0] = layout.limmin;
            let [x1, y1] = layout.limmax;
            width = (x1 - x0) * paper_unit;
            height = (y1 - y0) * paper_unit;
            ox = -x0 * paper_unit;
            oy = -y0 * paper_unit;
        }
        let paper_matrix: Matrix = [paper_unit, 0.0, 0.0, paper_unit, ox, oy];
        self.limit = [0.0, 0.0, width, height];
        let content = self.order.blocks.get(&layout.block.to_uppercase()).cloned().unwrap_or_default();
        let viewports: Vec<usize> = content
            .iter()
            .enumerate()
            .filter(|(_, entity)| matches!(entity.data, DxfEntityData::Viewport(_)))
            .map(|(index, _)| index)
            .collect();
        let overall = viewports
            .iter()
            .copied()
            .find(|index| matches!(&content[*index].data, DxfEntityData::Viewport(viewport) if viewport.id == 1.0))
            .or(viewports.first().copied());
        let model_scene = if viewports.len() > usize::from(overall.is_some()) {
            Some(dxf_to_scene(doc, Some("Model")))
        } else {
            None
        };
        let sheet_ctx = Ctx {
            m: paper_matrix,
            to_paper: IDENTITY,
            dash_factor: 1.0,
            inherit: Self::root_inherit(),
            stack: Vec::new(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
        };
        let mut shown: HashSet<String> = HashSet::new();
        let mut items = Vec::new();
        for (index, entity) in content.iter().enumerate() {
            let DxfEntityData::Viewport(viewport) = &entity.data else {
                self.emit(entity, &sheet_ctx, &mut items);
                continue;
            };
            if Some(index) == overall || viewport.status <= 0.0 || js::to_int32(viewport.status_flags) & 0x20000 != 0 {
                continue;
            }
            if let Some(scene) = &model_scene {
                self.viewport(&entity.base, viewport, &paper_matrix, scene, &mut items, &mut shown);
            }
        }
        if model_scene.is_some() {
            let hidden = self.order.model.iter().filter(|entity| !shown.contains(&entity.base.handle)).count();
            if hidden > 0 {
                self.notices.add_count(
                    NoticeLevel::Info,
                    "outside-viewports",
                    "どのビューポートにも映らないモデル空間の図形は書いていません",
                    hidden as u64,
                );
            }
        }
        self.sheet_items = items;
        let name = if layout.name.trim().is_empty() { "Layout1" } else { layout.name.as_str() };
        sheet_of(name, width, height)
    }

    /// ビューポートを、映るモデル空間の図形の部分図とその配置にする
    fn viewport(
        &mut self,
        base: &DxfEntityBase,
        viewport: &DxfViewport,
        paper_matrix: &Matrix,
        model_scene: &Scene,
        items: &mut Vec<Item>,
        shown: &mut HashSet<String>,
    ) {
        let [vx, vy, vz] = viewport.view_direction;
        let length = js::hypot(&[vx, vy, vz]);
        if length > 0.0 && (vx.abs() > 1e-9 * length || vy.abs() > 1e-9 * length || vz < 0.0) {
            self.notices.add(
                NoticeLevel::Info,
                "viewport-view",
                "上から見た向きでないビューポート（3D の表示）は書いていません",
            );
            return;
        }
        if !(viewport.view_height > 0.0 && viewport.height > 0.0) {
            return;
        }
        let s = viewport.height / viewport.view_height;
        let (cx, cy) = (viewport.center[0], viewport.center[1]);
        self.viewport_frame(base, viewport, paper_matrix, items);
        let (sin, cos) = viewport.view_twist_angle.to_radians().sin_cos();
        // 表示と同じ変換: 中心 − 縮尺 × 表示の中心 + 縮尺 × R(ねじれ) × (モデル − 目標点)
        let model = multiply(
            &[cos * s, sin * s, -sin * s, cos * s, cx - viewport.view_center[0] * s, cy - viewport.view_center[1] * s],
            &[1.0, 0.0, 0.0, 1.0, -viewport.view_target[0], -viewport.view_target[1]],
        );
        let m_total = multiply(paper_matrix, &model);
        let unit = self.unit;
        let to_paper = multiply(&m_total, &[1.0 / unit, 0.0, 0.0, 1.0 / unit, 0.0, 0.0]);
        let (hw, hh) = (viewport.width / 2.0, viewport.height / 2.0);
        let (x0, y0) = apply(paper_matrix, cx - hw, cy - hh);
        let (x1, y1) = apply(paper_matrix, cx + hw, cy + hh);
        let clip = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
        // R12 のビューポートは凍結したレイヤを名前で持つ
        let frozen: HashSet<String> = viewport
            .frozen_layer_handles
            .iter()
            .filter_map(|handle| self.doc.layers.iter().find(|layer| layer.handle == *handle))
            .map(|layer| layer.name.to_uppercase())
            .chain(viewport.frozen_layer_names.iter().map(|name| name.to_uppercase()))
            .collect();
        // モデル空間の表示の外接矩形（表示の座標 = 実寸 mm − 原点の寄せ）を、図形ごとに
        let mut boxes: HashMap<&str, zumen_core::BBox> = HashMap::new();
        let shift = model_scene
            .entities
            .first()
            .and_then(|entity| entity.placement.as_ref())
            .map_or([0.0, 0.0], |p| [p.inverse[4], p.inverse[5]]);
        for entity in &model_scene.entities {
            let slot = boxes.entry(top_id(&entity.id)).or_insert_with(zumen_core::geometry::empty_box);
            zumen_core::geometry::union_box(slot, &entity.bbox);
        }
        let model_entities = self.order.model.clone();
        let mut members: Vec<&DxfEntity> = Vec::new();
        let mut clipped = 0;
        for entity in &model_entities {
            if frozen.contains(&entity.base.layer.to_uppercase()) {
                continue;
            }
            let Some(bbox) = boxes.get(entity.base.handle.as_str()) else {
                continue;
            };
            // モデルの mm → 用紙
            let mut paper = zumen_core::geometry::empty_box();
            for (x, y) in
                [(bbox.min_x, bbox.min_y), (bbox.max_x, bbox.min_y), (bbox.max_x, bbox.max_y), (bbox.min_x, bbox.max_y)]
            {
                let (px, py) = apply(&to_paper, x + shift[0], y + shift[1]);
                zumen_core::geometry::extend_box(&mut paper, px, py);
            }
            if paper.max_x < clip[0] || paper.min_x > clip[2] || paper.max_y < clip[1] || paper.min_y > clip[3] {
                continue;
            }
            let tolerance = 1e-6 * (clip[2] - clip[0]).max(clip[3] - clip[1]).max(1.0);
            if paper.min_x < clip[0] - tolerance
                || paper.max_x > clip[2] + tolerance
                || paper.min_y < clip[1] - tolerance
                || paper.max_y > clip[3] + tolerance
            {
                clipped += 1;
            }
            members.push(entity);
            shown.insert(entity.base.handle.clone());
        }
        if clipped > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "viewport-clip",
                "ビューポートの枠からはみ出す図形は、SXF の部分図で切り取れないため、はみ出したまま書きました",
                clipped,
            );
        }
        if members.is_empty() {
            return;
        }
        let dash_factor = if self.doc.header.psltscale == 1.0 { 1.0 / s } else { 1.0 };
        let identity = to_paper.iter().zip(IDENTITY.iter()).all(|(a, b)| (a - b).abs() < 1e-9);
        if identity {
            // 1:1 で原点を動かさないビューポート（部分図のない SXF の図面）の図形は、用紙にじかに置く
            let ctx = Ctx {
                m: m_total,
                to_paper: IDENTITY,
                dash_factor,
                inherit: Self::root_inherit(),
                stack: Vec::new(),
                flatten: false,
                prefix: format!("{}:", base.handle),
                source: None,
                clip: Some(clip),
            };
            for entity in &members {
                self.emit(entity, &ctx, items);
            }
            return;
        }
        let key: Vec<String> = members.iter().map(|entity| entity.base.handle.clone()).collect();
        let key_name = format!("\u{0}viewport:{}", key.join(","));
        let name = match self.variants.get(&(key_name.clone(), None)) {
            Some(name) => name.clone(),
            None => {
                let name = self.unique_figure(&format!("部分図 {}", self.figures.len() + 1));
                self.variants.insert((key_name, None), name.clone());
                let ctx = Ctx {
                    m: [unit, 0.0, 0.0, unit, 0.0, 0.0],
                    to_paper,
                    dash_factor,
                    inherit: Self::root_inherit(),
                    stack: Vec::new(),
                    flatten: false,
                    prefix: String::new(),
                    source: None,
                    clip: Some(clip),
                };
                let mut figure_items = Vec::new();
                for entity in &members {
                    self.emit(entity, &ctx, &mut figure_items);
                }
                self.push_figure(&name, 1.0, figure_items);
                name
            }
        };
        self.locate(&name, 0.0, &to_paper, format!("{}:", base.handle), Some(items));
    }

    /// ビューポートの枠（表示と同じ）。印刷しない・表示しないレイヤの枠は、印刷に出ないので書かない
    fn viewport_frame(
        &mut self,
        base: &DxfEntityBase,
        viewport: &DxfViewport,
        paper_matrix: &Matrix,
        items: &mut Vec<Item>,
    ) {
        if let Some(layer) = self.layers_upper.get(&base.layer.to_uppercase())
            && (!layer.plot || layer.off || layer.frozen)
        {
            return;
        }
        let ctx = Ctx {
            m: *paper_matrix,
            to_paper: IDENTITY,
            dash_factor: 1.0,
            inherit: Self::root_inherit(),
            stack: Vec::new(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
        };
        let style = self.style(base, &ctx, paper_matrix);
        let (cx, cy, hw, hh) = (viewport.center[0], viewport.center[1], viewport.width / 2.0, viewport.height / 2.0);
        let corners =
            [(cx - hw, cy - hh), (cx + hw, cy - hh), (cx + hw, cy + hh), (cx - hw, cy + hh), (cx - hw, cy - hh)];
        let (xs, ys) = corners.iter().map(|(x, y)| apply(paper_matrix, *x, *y)).unzip();
        items.push(Item::feature(SxfFeature::Polyline { style, xs, ys }, base.handle.clone()));
    }

    fn unique_figure(&mut self, name: &str) -> String {
        let base = if name.trim().is_empty() { "ブロック" } else { name };
        let mut candidate = base.to_string();
        let mut number = 2;
        while self.figure_names.contains(&candidate) {
            candidate = format!("{base} ({number})");
            number += 1;
        }
        self.figure_names.insert(candidate.clone());
        candidate
    }

    fn push_figure(&mut self, name: &str, flag: f64, items: Vec<Item>) {
        let id = self.id();
        self.figures.push((SfigOrg { id, name: name.to_string(), flag }, items));
    }

    /// 複合図形の配置。placement は複合図形の座標 → 入れ物の座標
    fn locate(&mut self, name: &str, layer: f64, placement: &Matrix, source: String, items: Option<&mut Vec<Item>>) {
        let Some((x, y, angle, ratio_x, ratio_y)) = decompose(placement) else {
            self.notices.add(NoticeLevel::Warning, "skewed", "ゆがんだ配置の複合図形は書いていません");
            return;
        };
        let id = self.id();
        let feature = SxfFeature::SfigLocate { id, layer, name: name.to_string(), x, y, angle, ratio_x, ratio_y };
        match items {
            Some(items) => items.push(Item::feature(feature, source)),
            None => self.sheet_items.push(Item::feature(feature, source)),
        }
    }

    // --- 表 --------------------------------------------------------------------------------------------

    fn layer_code(&mut self, name: &str) -> f64 {
        let key = name.to_uppercase();
        if let Some(code) = self.tables.layer_codes.get(&key) {
            return *code;
        }
        // DEFPOINTS（寸法の定義点）は AutoCAD が印刷しないレイヤなので、SXF では非表示のレイヤにする
        let visible = key != "DEFPOINTS"
            && self.layers_upper.get(&key).is_none_or(|layer| !layer.off && !layer.frozen && layer.plot);
        let written = self.layers_upper.get(&key).map_or(name.to_string(), |layer| layer.name.clone());
        let id = self.id();
        self.tables.layers.push(LayerDefinition { id, name: written, visible });
        let code = self.tables.layers.len() as f64;
        self.tables.layer_codes.insert(key, code);
        code
    }

    fn color_code(&mut self, paint: Paint) -> f64 {
        let rgb = match paint {
            Paint::Foreground => return 1.0,
            Paint::Background => return 8.0,
            Paint::Rgb(rgb) => rgb,
        };
        if let Some(index) = PREDEFINED_COLORS.iter().position(|color| color.rgb == rgb) {
            return index as f64 + 1.0;
        }
        if let Some(code) = self.tables.color_codes.get(&rgb) {
            return *code;
        }
        let id = self.id();
        let [r, g, b] = rgb.map(f64::from);
        self.tables.colors.push(UserColour { id, r, g, b });
        let code = 16.0 + self.tables.colors.len() as f64;
        self.tables.color_codes.insert(rgb, code);
        code
    }

    fn width_code(&mut self, width: f64) -> f64 {
        let width = if width.is_finite() && width >= 0.0 { width } else { DEFAULT_LINEWEIGHT };
        if let Some(index) = PREDEFINED_WIDTHS.iter().position(|value| (value - width).abs() < 1e-6) {
            return index as f64 + 1.0;
        }
        if let Some(code) = self.tables.width_codes.get(&width.to_bits()) {
            return *code;
        }
        let id = self.id();
        self.tables.widths.push(WidthDefinition { id, width });
        let code = 10.0 + self.tables.widths.len() as f64;
        self.tables.width_codes.insert(width.to_bits(), code);
        code
    }

    /// 線種（線分・空白の長さは用紙の mm）。実線は 1。既定義線種と同じ名前・長さなら既定義線種
    fn line_type_code(&mut self, name: &str, dashes: &[f64]) -> f64 {
        if dashes.is_empty() {
            return 1.0;
        }
        let pitch = self.pitch(dashes);
        if pitch.is_empty() {
            return 1.0;
        }
        if let Some(index) = PREDEFINED_LINE_TYPES.iter().position(|kind| {
            kind.name.eq_ignore_ascii_case(name)
                && kind.pitch.len() == pitch.len()
                && kind.pitch.iter().zip(&pitch).all(|(a, b)| (a - b).abs() < 1e-6)
        }) {
            return index as f64 + 1.0;
        }
        let name = if name.trim().is_empty() { "line type" } else { name };
        let key = (name.to_uppercase(), pitch.iter().map(|value| value.to_bits()).collect::<Vec<_>>());
        if let Some(code) = self.tables.line_type_codes.get(&key) {
            return *code;
        }
        let id = self.id();
        self.tables.line_types.push(UserLineType { id, name: name.to_string(), pitch });
        let code = 16.0 + self.tables.line_types.len() as f64;
        self.tables.line_type_codes.insert(key, code);
        code
    }

    /// 表示の線分・空白の並び（dash_array）を、SXF のピッチ（正の線分と空白の組）にする。長さ 0 の線分（点）は
    /// 0.25 mm の線分にし、続く空白をその分だけ短くする
    fn pitch(&mut self, dashes: &[f64]) -> Vec<f64> {
        let mut out = dashes.to_vec();
        if out.len() % 2 == 1 {
            out.push(0.0);
        }
        let mut dotted = false;
        for i in (0..out.len()).step_by(2) {
            if out[i] <= 0.0 {
                dotted = true;
                out[i] = DOT;
                if out[i + 1] > DOT * 2.0 {
                    out[i + 1] -= DOT;
                }
            }
            if out[i + 1] <= 0.0 {
                out[i + 1] = DOT;
            }
        }
        if dotted {
            self.notices.add(
                NoticeLevel::Info,
                "linetype-dot",
                "線種の点（長さ 0 の線分）は、SXF の線種に書けないため 0.25 mm の線分にしました",
            );
        }
        if out.iter().any(|value| !(value.is_finite() && *value > 0.0)) {
            return Vec::new();
        }
        out
    }

    fn font_code(&mut self, name: &str) -> f64 {
        let name = name.strip_prefix('@').unwrap_or(name);
        if name.trim().is_empty() {
            return 0.0;
        }
        if let Some(code) = self.tables.font_codes.get(name) {
            return *code;
        }
        let id = self.id();
        self.tables.fonts.push(TextFont { id, name: name.to_string() });
        let code = self.tables.fonts.len() as f64;
        self.tables.font_codes.insert(name.to_string(), code);
        code
    }

    // --- 見え方（表示の color・lineweight・linetype_name と同じ規則） ------------------------------------

    fn effective_layer(&self, base: &DxfEntityBase, ctx: &Ctx) -> String {
        match &ctx.inherit.layer {
            Some(parent) if base.layer == "0" => parent.clone(),
            _ => base.layer.clone(),
        }
    }

    fn paint(&mut self, base: &DxfEntityBase, ctx: &Ctx) -> Paint {
        let transparency = base.transparency.or_else(|| {
            self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()).and_then(|layer| layer.transparency)
        });
        if is_transparent(transparency) {
            self.notices.add(NoticeLevel::Info, "transparency", "透過度は SXF に書けないため、不透明で書きました");
        }
        if let Some(value) = base.true_color {
            return paint_of_hex(&true_color(value)).with_white();
        }
        let index = base.color_index;
        if index == 0.0 {
            return ctx.inherit.paint;
        }
        if index == 256.0 || index == 257.0 || !(0.0..=257.0).contains(&index) {
            return match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) => match layer.true_color {
                    Some(value) => paint_of_hex(&true_color(value)).with_white(),
                    None => paint_of_aci(layer.color_index),
                },
                None => Paint::Foreground,
            };
        }
        paint_of_aci(index)
    }

    fn lineweight(&self, base: &DxfEntityBase, ctx: &Ctx) -> f64 {
        let value = base.lineweight;
        if value == -2.0 {
            return ctx.inherit.lineweight;
        }
        if value == -1.0 {
            return match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) if layer.lineweight >= 0.0 => layer.lineweight / 100.0,
                _ => DEFAULT_LINEWEIGHT,
            };
        }
        if value >= 0.0 { value / 100.0 } else { DEFAULT_LINEWEIGHT }
    }

    fn linetype_name(&self, base: &DxfEntityBase, ctx: &Ctx) -> String {
        let name = if base.linetype.is_empty() { "BYLAYER" } else { base.linetype.as_str() };
        match name.to_uppercase().as_str() {
            "BYBLOCK" => ctx.inherit.linetype.clone(),
            "BYLAYER" => match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) if !layer.linetype.is_empty() => layer.linetype.clone(),
                _ => "CONTINUOUS".into(),
            },
            _ => name.to_string(),
        }
    }

    /// 線種の線分・空白の長さ（用紙の mm）。m は図形の座標 → 入れ物の座標
    fn dashes(&mut self, base: &DxfEntityBase, ctx: &Ctx, m: &Matrix) -> Vec<f64> {
        let name = self.linetype_name(base, ctx).to_uppercase();
        if matches!(name.as_str(), "CONTINUOUS" | "BYLAYER" | "BYBLOCK") {
            return Vec::new();
        }
        let Some(linetype) = self.linetypes_upper.get(&name).copied() else {
            return Vec::new();
        };
        if linetype.complex {
            self.notices.add(NoticeLevel::Info, "complex-linetype", "文字・形を含む線種は、線と空白だけで書きました");
        }
        let factor =
            self.doc.header.ltscale * base.linetype_scale * length_scale(&multiply(&ctx.to_paper, m)) * ctx.dash_factor;
        dash_array(&linetype.dashes, factor)
    }

    /// 線の要素の共通の値（レイヤ・色・線種・線幅のコード）
    fn style(&mut self, base: &DxfEntityBase, ctx: &Ctx, m: &Matrix) -> Styled {
        let layer = self.effective_layer(base, ctx);
        let layer = self.layer_code(&layer);
        let paint = self.paint(base, ctx);
        let color = self.color_code(paint);
        let dashes = self.dashes(base, ctx, m);
        let name = self.linetype_name(base, ctx);
        let line_type = self.line_type_code(&name, &dashes);
        let width = self.lineweight(base, ctx);
        let line_width = self.width_code(width);
        Styled { id: self.id(), layer, color, line_type, line_width }
    }

    /// ブロック参照・寸法の中の図形が継ぐ値
    fn child_inherit(&mut self, base: &DxfEntityBase, ctx: &Ctx) -> Inherit {
        Inherit {
            paint: self.paint(base, ctx),
            lineweight: self.lineweight(base, ctx),
            linetype: self.linetype_name(base, ctx),
            layer: Some(self.effective_layer(base, ctx)),
        }
    }

    // --- 図形 ------------------------------------------------------------------------------------------

    fn emit(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        if zumen_core::limits::tick() {
            return;
        }
        let base = &entity.base;
        if base.invisible {
            return;
        }
        let source = ctx.source(base);
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                let style = self.style(base, ctx, &ctx.m);
                let (x1, y1) = apply(&ctx.m, start[0], start[1]);
                let (x2, y2) = apply(&ctx.m, end[0], end[1]);
                items.push(Item::feature(SxfFeature::Line { style, x1, y1, x2, y2 }, source));
            }
            DxfEntityData::Circle { center, radius } => {
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), center[2]));
                if !is_similar(&m) || !(radius.is_finite() && *radius > 0.0) {
                    return self.pend(entity, ctx, items);
                }
                let style = self.style(base, ctx, &m);
                let (cx, cy) = apply(&m, center[0], center[1]);
                let r = radius * length_scale(&m);
                items.push(Item::feature(SxfFeature::Circle { style, cx, cy, r }, source));
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), center[2]));
                if !is_similar(&m) || !(radius.is_finite() && *radius > 0.0) {
                    return self.pend(entity, ctx, items);
                }
                let style = self.style(base, ctx, &m);
                let ([cx, cy], r, start, end, ccw) =
                    map_arc(&m, [center[0], center[1]], *radius, *start_angle, *end_angle, true);
                items.push(Item::feature(SxfFeature::Arc { style, cx, cy, r, ccw, start, end }, source));
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                if !is_similar(&ctx.m) {
                    return self.pend(entity, ctx, items);
                }
                let minor = minor_axis(major_axis, *ratio, base.extrusion.as_ref());
                let (ax, ay) = apply_vector(&ctx.m, major_axis[0], major_axis[1]);
                let (bx, by) = apply_vector(&ctx.m, minor[0], minor[1]);
                let (rx, ry) = (ax.hypot(ay), bx.hypot(by));
                if !(rx > 0.0 && ry > 0.0) {
                    return self.pend(entity, ctx, items);
                }
                let style = self.style(base, ctx, &ctx.m);
                let (cx, cy) = apply(&ctx.m, center[0], center[1]);
                let rotation = normalize(ay.atan2(ax).to_degrees());
                // 短軸が長軸の反時計回りの向きでなければ、媒介変数の向きが逆になる
                let ccw = ax * by - ay * bx > 0.0;
                let sweep = (end_param - start_param).abs();
                let full = sweep < 1e-9 || (sweep - PI * 2.0).abs() < 1e-9;
                let feature = if full {
                    SxfFeature::Ellipse { style, cx, cy, rx, ry, rotation }
                } else {
                    let sign = if ccw { 1.0 } else { -1.0 };
                    SxfFeature::EllipseArc {
                        style,
                        cx,
                        cy,
                        rx,
                        ry,
                        ccw,
                        rotation,
                        start: normalize(sign * start_param.to_degrees()),
                        end: normalize(sign * end_param.to_degrees()),
                    }
                };
                items.push(Item::feature(feature, source));
            }
            DxfEntityData::LwPolyline(polyline) => {
                let widths = polyline.const_width.is_some_and(|width| width != 0.0)
                    || polyline.vertices.widths.iter().any(|width| width[0] != 0.0 || width[1] != 0.0);
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), polyline.elevation.unwrap_or(0.0)));
                if widths || !is_similar(&m) {
                    if widths {
                        self.notices.add(
                            NoticeLevel::Info,
                            "polyline-width",
                            "幅のあるポリラインは、表示と同じ塗りにしました",
                        );
                    }
                    return self.pend(entity, ctx, items);
                }
                self.polyline(base, ctx, &m, &polyline.vertices, items, &source);
            }
            DxfEntityData::Polyline(polyline) => {
                let flags = js::to_int32(polyline.vertices.flags);
                let mesh = polyline.faces.is_some() || polyline.m_count.is_some() || flags & (16 | 64) != 0;
                let fitted = flags & (2 | 4) != 0;
                let three_d = flags & 8 != 0;
                if mesh || fitted {
                    return self.pend(entity, ctx, items);
                }
                let elevation = polyline.vertices.points.first().map_or(0.0, |point| point[2]);
                let m = if three_d { ctx.m } else { multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), elevation)) };
                let widths = polyline.vertices.widths.iter().any(|width| width[0] != 0.0 || width[1] != 0.0);
                if widths || !is_similar(&m) {
                    return self.pend(entity, ctx, items);
                }
                let mut vertices = polyline.vertices.clone();
                if three_d {
                    vertices.bulges = vec![0.0; vertices.points.len()];
                }
                self.polyline(base, ctx, &m, &vertices, items, &source);
            }
            DxfEntityData::Spline(spline) => {
                let degree = spline.degree as usize;
                let control: Vec<[f64; 2]> = spline
                    .control_points
                    .iter()
                    .map(|point| {
                        let (x, y) = apply(&ctx.m, point[0], point[1]);
                        [x, y]
                    })
                    .collect();
                let weighted = spline.rational && spline.weights.iter().any(|weight| (weight - 1.0).abs() > 1e-12);
                let points =
                    if weighted || base.kind != "SPLINE" { None } else { bezier(degree, &spline.knots, &control) };
                match points {
                    Some(points) if points.len() >= 4 => {
                        let style = self.style(base, ctx, &ctx.m);
                        let (xs, ys) = points.iter().map(|[x, y]| (*x, *y)).unzip();
                        items.push(Item::feature(SxfFeature::Spline { style, closed: false, xs, ys }, source));
                    }
                    _ if degree == 1 && !weighted && control.len() >= 2 && base.kind == "SPLINE" => {
                        let style = self.style(base, ctx, &ctx.m);
                        let (xs, ys) = control.iter().map(|[x, y]| (*x, *y)).unzip();
                        items.push(Item::feature(SxfFeature::Polyline { style, xs, ys }, source));
                    }
                    _ => {
                        self.notices.add(
                            NoticeLevel::Info,
                            "spline",
                            "3 次ベジェ曲線に分けられないスプライン（有理・当てはめ点だけ・端を通らないもの）とらせんは、表示と同じ折線にしました",
                        );
                        self.pend(entity, ctx, items);
                    }
                }
            }
            DxfEntityData::Point { location, .. } => {
                let layer = self.effective_layer(base, ctx);
                let layer = self.layer_code(&layer);
                let paint = self.paint(base, ctx);
                let color = self.color_code(paint);
                let (x, y) = apply(&ctx.m, location[0], location[1]);
                let id = self.id();
                let feature = SxfFeature::PointMarker { id, layer, color, x, y, code: 3.0, angle: 0.0, scale: 1.0 };
                items.push(Item::feature(feature, source));
            }
            DxfEntityData::Insert(_) | DxfEntityData::Table(_) => self.insert(entity, ctx, items),
            DxfEntityData::Dimension(dimension) => {
                let Some(members) = self.order.blocks.get(&dimension.anonymous_block_name.to_uppercase()).cloned()
                else {
                    self.notices.add(NoticeLevel::Warning, "dimension-block", "表示のブロックがない寸法を省きました");
                    return;
                };
                if ctx.stack.len() >= MAX_DEPTH {
                    self.notices.add(NoticeLevel::Warning, "block-depth", "入れ子が深すぎるブロック参照を省きました");
                    return;
                }
                self.notices.add(
                    NoticeLevel::Info,
                    "dimension",
                    "寸法は、表示のブロックの線・塗り・文字にしました（SXF の寸法の要素にはしていません）",
                );
                // 表示と同じく、ブロックの図形は寸法の OCS にあり、12 の位置へ動かす
                let ocs = ocs_matrix(base.extrusion.as_ref(), 0.0);
                let insert = dimension.insert.unwrap_or([0.0, 0.0, 0.0]);
                let (ix, iy) = apply(&ocs, insert[0], insert[1]);
                let local = multiply(&[1.0, 0.0, 0.0, 1.0, ix, iy], &ocs);
                let mut stack = ctx.stack.clone();
                stack.push(dimension.anonymous_block_name.to_uppercase());
                let child = Ctx {
                    m: multiply(&ctx.m, &local),
                    inherit: self.child_inherit(base, ctx),
                    stack,
                    flatten: true,
                    source: Some(source),
                    ..ctx.clone()
                };
                for member in &members {
                    self.emit(member, &child, items);
                }
            }
            DxfEntityData::Viewport(_) => {}
            DxfEntityData::XLine { start, unit_vector } => self.xline(entity, start, unit_vector, ctx, items),
            DxfEntityData::Image(_) if base.kind == "IMAGE" => {
                self.notices.add(
                    NoticeLevel::Info,
                    "image",
                    "画像（IMAGE）は、その画像のファイルを読まないため、表示と同じ枠だけを書きました",
                );
                self.pend(entity, ctx, items);
            }
            DxfEntityData::Attdef { attribute_flags, .. } if ctx.inherit.layer.is_some() => {
                // ブロックの中の属性定義は、定数（2）のものだけを描く（表示と同じ。値は配置の ATTRIB が持つ）
                if js::to_int32(*attribute_flags) & 2 != 0 {
                    self.pend(entity, ctx, items);
                }
            }
            _ => {
                if base.kind == "WIPEOUT" {
                    self.notices.add(NoticeLevel::Info, "wipeout", "ワイプアウトは、白の塗りにしました");
                }
                if matches!(base.kind.as_str(), "MTEXT" | "LEADER" | "MULTILEADER" | "MLEADER" | "MLINE" | "3DFACE")
                    || matches!(entity.data, DxfEntityData::Hatch(_)) && base.kind == "MPOLYGON"
                {
                    self.notices.add(
                        NoticeLevel::Info,
                        "drawn-shapes",
                        "複数行文字・引出線・マルチラインなど SXF にない図形は、表示と同じ線・塗り・文字にしました",
                    );
                }
                self.pend(entity, ctx, items);
            }
        }
    }

    /// 構築線・放射線は、表示と同じく用紙の範囲（ビューポートの中はその枠も）で切った線分にする
    fn xline(&mut self, entity: &DxfEntity, start: &[f64; 3], direction: &[f64; 3], ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let m = multiply(&ctx.to_paper, &ctx.m);
        let (px, py) = apply(&m, start[0], start[1]);
        let (dx, dy) = apply_vector(&m, direction[0], direction[1]);
        let length = dx.hypot(dy);
        if length.is_nan() || length <= 0.0 {
            return;
        }
        let (ux, uy) = (dx / length, dy / length);
        let b = self.limit;
        let reach = (b[2] - b[0]).hypot(b[3] - b[1]) * 2.0 + (px - b[0]).hypot(py - b[1]);
        let from = if base.kind == "XLINE" { -reach } else { 0.0 };
        let mut segment = [px + ux * from, py + uy * from, px + ux * reach, py + uy * reach];
        for rect in std::iter::once(b).chain(ctx.clip) {
            match clip_segment(segment, rect) {
                Some(clipped) => segment = clipped,
                None => return,
            }
        }
        self.notices.add(NoticeLevel::Info, "xline", "構築線・放射線は、表示と同じく用紙の範囲で切った線分にしました");
        let back = invert(&ctx.to_paper);
        let (x1, y1) = apply(&back, segment[0], segment[1]);
        let (x2, y2) = apply(&back, segment[2], segment[3]);
        let style = self.style(base, ctx, &ctx.m);
        items.push(Item::feature(SxfFeature::Line { style, x1, y1, x2, y2 }, ctx.source(base)));
    }

    /// 折線（弧の膨らみの区間は円弧）
    fn polyline(
        &mut self,
        base: &DxfEntityBase,
        ctx: &Ctx,
        m: &Matrix,
        vertices: &zumen_dxf::DxfVertexList,
        items: &mut Vec<Item>,
        source: &str,
    ) {
        let points: Vec<[f64; 2]> = vertices.points.iter().map(|point| [point[0], point[1]]).collect();
        if points.len() < 2 {
            return;
        }
        let count = if vertices.closed { points.len() } else { points.len() - 1 };
        let style = self.style(base, ctx, m);
        let mut run: Vec<[f64; 2]> = Vec::new();
        let flush = |this: &mut Self, run: &mut Vec<[f64; 2]>, items: &mut Vec<Item>| {
            if run.len() >= 2 {
                let (xs, ys) = run
                    .iter()
                    .map(|[x, y]| {
                        let (px, py) = apply(m, *x, *y);
                        (px, py)
                    })
                    .unzip();
                let style = Styled { id: this.id(), ..style.clone() };
                items.push(Item::feature(SxfFeature::Polyline { style, xs, ys }, source.to_string()));
            }
            run.clear();
        };
        for i in 0..count {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            let bulge = vertices.bulges.get(i).copied().unwrap_or(0.0);
            match bulge_arc(a, b, bulge) {
                Some((center, radius, start, end, ccw)) => {
                    flush(self, &mut run, items);
                    let ([cx, cy], r, start, end, ccw) = map_arc(m, center, radius, start, end, ccw);
                    let style = Styled { id: self.id(), ..style.clone() };
                    items
                        .push(Item::feature(SxfFeature::Arc { style, cx, cy, r, ccw, start, end }, source.to_string()));
                }
                None => {
                    if run.is_empty() {
                        run.push(a);
                    }
                    run.push(b);
                }
            }
        }
        flush(self, &mut run, items);
    }

    /// 線種が破線か
    fn linetype_dashed(&self, name: &str) -> bool {
        self.linetypes_upper
            .get(&name.to_uppercase())
            .is_some_and(|linetype| linetype.dashes.iter().any(|value| *value != 0.0))
    }

    /// ブロックが破線の図形を持つか（BYLAYER はその図形のレイヤの線種）
    fn dashed(&mut self, name: &str, depth: usize) -> bool {
        let key = format!("\u{0}dashed:{}", name.to_uppercase());
        if let Some(value) = self.inherits.get(&key) {
            return *value;
        }
        if depth > MAX_DEPTH {
            return false;
        }
        self.inherits.insert(key.clone(), false);
        let members = self.order.blocks.get(&name.to_uppercase()).cloned().unwrap_or_default();
        let mut value = false;
        for member in members {
            let linetype = match member.base.linetype.to_uppercase().as_str() {
                "" | "BYLAYER" => {
                    self.layers_upper.get(&member.base.layer.to_uppercase()).map(|layer| layer.linetype.clone())
                }
                _ => Some(member.base.linetype.clone()),
            };
            let child = match &member.data {
                DxfEntityData::Insert(insert) => Some(insert.name.clone()),
                DxfEntityData::Table(table) => Some(table.name.clone()),
                DxfEntityData::Dimension(dimension) => Some(dimension.anonymous_block_name.clone()),
                _ => None,
            };
            if linetype.is_some_and(|linetype| self.linetype_dashed(&linetype))
                || child.is_some_and(|child| self.dashed(&child, depth + 1))
            {
                value = true;
                break;
            }
        }
        self.inherits.insert(key, value);
        value
    }

    /// ブロックが、配置の値（BYBLOCK・レイヤ 0）を継ぐ図形を持つか
    fn inherits(&mut self, name: &str, depth: usize) -> bool {
        let key = name.to_uppercase();
        if let Some(value) = self.inherits.get(&key) {
            return *value;
        }
        if depth > MAX_DEPTH {
            return true;
        }
        self.inherits.insert(key.clone(), false);
        let members = self.order.blocks.get(&key).cloned().unwrap_or_default();
        let mut value = false;
        for member in members {
            let base = &member.base;
            if (base.color_index == 0.0 && base.true_color.is_none())
                || base.lineweight == -2.0
                || base.linetype.eq_ignore_ascii_case("BYBLOCK")
                || base.layer == "0"
            {
                value = true;
                break;
            }
            let child = match &member.data {
                DxfEntityData::Insert(insert) => Some(insert.name.clone()),
                DxfEntityData::Table(table) => Some(table.name.clone()),
                DxfEntityData::Dimension(dimension) => Some(dimension.anonymous_block_name.clone()),
                _ => None,
            };
            if child.is_some_and(|child| self.inherits(&child, depth + 1)) {
                value = true;
                break;
            }
        }
        self.inherits.insert(key, value);
        value
    }

    /// ブロック参照は作図部品の配置（寸法の中では、寸法の図形として写す）。配列（MINSERT）は配置を並べる
    fn insert(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let (name, point, xscale, yscale, rotation, insert) = match &entity.data {
            DxfEntityData::Insert(insert) => {
                (&insert.name, insert.insert, insert.xscale, insert.yscale, insert.rotation, Some(insert))
            }
            DxfEntityData::Table(table) => {
                (&table.name, table.insert, table.xscale, table.yscale, table.rotation, None)
            }
            _ => return,
        };
        let key = name.to_uppercase();
        let Some(members) = self.order.blocks.get(&key).cloned() else {
            self.notices.add(NoticeLevel::Warning, "missing-block", "定義の見つからないブロック参照を省きました");
            return;
        };
        if ctx.stack.len() >= MAX_DEPTH || ctx.stack.contains(&key) {
            self.notices.add(NoticeLevel::Warning, "block-depth", "循環する・入れ子が深すぎるブロック参照を省きました");
            return;
        }
        let block = self.doc.blocks.iter().find(|block| block.name.eq_ignore_ascii_case(name));
        let base_point = block.map_or([0.0; 3], |block| block.base_point);
        let ocs = ocs_matrix(base.extrusion.as_ref(), point[2]);
        let columns = insert.map_or(1.0, |insert| insert.column_count);
        let rows = insert.map_or(1.0, |insert| insert.row_count);
        let many = columns * rows > 1.0;
        let limit = |count: f64| if columns * rows > 10_000.0 { 1.0 } else { count };
        if many {
            self.notices.add(NoticeLevel::Info, "minsert", "配列のブロック参照（MINSERT）は、配置を並べて書きました");
        }
        let inherit = self.child_inherit(base, ctx);
        let unit = self.unit;
        // 作図部品の座標 = (ブロックの座標 − 基点) × 単位の mm
        let block_matrix: Matrix = [unit, 0.0, 0.0, unit, -unit * base_point[0], -unit * base_point[1]];
        let mut stack = ctx.stack.clone();
        stack.push(key.clone());
        let mut r = 0.0;
        while r < limit(rows) {
            let mut c = 0.0;
            while c < limit(columns) {
                let offset = match insert {
                    Some(insert) => [1.0, 0.0, 0.0, 1.0, c * insert.column_spacing, r * insert.row_spacing],
                    None => IDENTITY,
                };
                let (sin, cos) = rotation.to_radians().sin_cos();
                let local = multiply(
                    &multiply(&multiply(&ocs, &[cos, sin, -sin, cos, point[0], point[1]]), &offset),
                    &[xscale, 0.0, 0.0, yscale, -xscale * base_point[0], -yscale * base_point[1]],
                );
                let suffix = if many {
                    format!("[{},{}]", js::number_to_string(r), js::number_to_string(c))
                } else {
                    String::new()
                };
                if ctx.flatten {
                    let child = Ctx {
                        m: multiply(&ctx.m, &local),
                        inherit: inherit.clone(),
                        stack: stack.clone(),
                        ..ctx.clone()
                    };
                    for member in &members {
                        self.emit(member, &child, items);
                    }
                } else {
                    let placement = multiply(&multiply(&ctx.m, &local), &invert(&block_matrix));
                    let figure = self.part(name, &key, &members, &inherit, ctx, &placement, &block_matrix, &stack);
                    let layer = self.effective_layer(base, ctx);
                    let layer = self.layer_code(&layer);
                    self.locate(
                        &figure,
                        layer,
                        &placement,
                        format!("{}{}{suffix}>", ctx.prefix, base.handle),
                        Some(items),
                    );
                }
                c += 1.0;
            }
            r += 1.0;
        }
        // 属性は配置と同じ座標系にあり、ブロック参照の値を継ぐ
        if let Some(insert) = insert {
            let child = Ctx { inherit, stack, ..ctx.clone() };
            for attribute in &insert.attribs {
                let source = ctx
                    .source
                    .clone()
                    .unwrap_or_else(|| format!("{}{}>{}", ctx.prefix, base.handle, attribute.base.handle));
                let child = Ctx { source: Some(source), ..child.clone() };
                self.pend(attribute, &child, items);
            }
        }
    }

    /// ブロックの作図部品（継ぐ値ごとに 1 つ）。初めて使うときに、ブロックの図形を写す
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        name: &str,
        key: &str,
        members: &[&'a DxfEntity],
        inherit: &Inherit,
        ctx: &Ctx,
        placement: &Matrix,
        block_matrix: &Matrix,
        stack: &[String],
    ) -> String {
        let variant = self.inherits(name, 0).then(|| inherit.key());
        // 線種のピッチは用紙の mm なので、破線を持つブロックは用紙での縮尺ごとに作図部品を作る
        let scale = length_scale(&multiply(&ctx.to_paper, placement)) * ctx.dash_factor;
        let key = if self.dashed(name, 0) || self.linetype_dashed(&inherit.linetype) {
            format!("{key}\u{0}{}", js::to_precision(scale, 12))
        } else {
            key.to_string()
        };
        let lookup = (key, variant);
        if let Some(figure) = self.variants.get(&lookup) {
            let figure = figure.clone();
            return figure;
        }
        let figure = self.unique_figure(name.trim_start_matches('*'));
        self.variants.insert(lookup, figure.clone());
        let child = Ctx {
            m: *block_matrix,
            to_paper: multiply(&ctx.to_paper, placement),
            dash_factor: ctx.dash_factor,
            inherit: inherit.clone(),
            stack: stack.to_vec(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
        };
        let mut figure_items = Vec::new();
        for member in members {
            self.emit(member, &child, &mut figure_items);
        }
        self.push_figure(&figure, 4.0, figure_items);
        figure
    }

    /// 表示が描く形から写す図形に加える。継ぐ値（BYBLOCK・レイヤ 0）は図形に入れておく
    fn pend(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let mut copy = entity.clone();
        let layer_name = self.effective_layer(base, ctx);
        let paint = self.paint(base, ctx);
        copy.base.layer = layer_name.clone();
        if base.color_index == 0.0 && base.true_color.is_none() {
            match paint {
                Paint::Foreground | Paint::Background => copy.base.color_index = 7.0,
                Paint::Rgb([r, g, b]) => {
                    copy.base.color_index = 7.0;
                    copy.base.true_color = Some(f64::from((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)));
                }
            }
        }
        if base.lineweight == -2.0 {
            copy.base.lineweight = (ctx.inherit.lineweight * 100.0).round();
        }
        let linetype = self.linetype_name(base, ctx);
        if base.linetype.eq_ignore_ascii_case("BYBLOCK") {
            copy.base.linetype = linetype.clone();
        }
        // ブロックの中の属性定義（定数）は、配置の中と同じく値を描くよう属性にする（最上位の属性定義は名札を描く）
        if let DxfEntityData::Attdef { text, tag, attribute_flags, mtext, .. } = &copy.data
            && ctx.inherit.layer.is_some()
        {
            copy.data = DxfEntityData::Attrib {
                text: text.clone(),
                tag: tag.clone(),
                attribute_flags: *attribute_flags,
                mtext: mtext.clone(),
            };
            copy.base.kind = "ATTRIB".into();
        }
        let index = self.pending.len();
        copy.base.handle = format!("ZS{index}");
        copy.base.space = DxfSpace::Model;
        copy.base.block = None;
        copy.base.owner = None;
        let font = self.text_font(entity);
        let layer = self.layer_code(&layer_name);
        // 表示の線種の長さは図面の単位を mm にした長さ
        let dash_scale = length_scale(&multiply(&ctx.to_paper, &ctx.m)) * ctx.dash_factor / self.unit;
        let mut hatch = None;
        if let DxfEntityData::Hatch(data) = &mut copy.data {
            if data.gradient.is_some() {
                self.notices.add(NoticeLevel::Info, "gradient", "グラデーションは、表示と同じ色の塗りにしました");
            } else if !data.solid_fill && !data.pattern_lines.is_empty() {
                hatch = Some(data.clone());
                data.solid_fill = true;
                data.pattern_lines.clear();
            }
        }
        let lineweight = self.lineweight(base, ctx);
        self.pending.push(Pending {
            entity: copy,
            m: ctx.m,
            dash_scale,
            paper_scale: length_scale(&ctx.to_paper),
            layer,
            linetype,
            font,
            source: ctx.source(base),
            paint,
            lineweight,
            hatch,
        });
        items.push(Item::Pending(index));
    }

    /// 文字の図形の文字スタイルのフォント（表示の font_family と同じ名前）
    fn text_font(&mut self, entity: &DxfEntity) -> f64 {
        let style = match &entity.data {
            DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
                text.style.clone()
            }
            DxfEntityData::MText(mtext) => mtext.style.clone(),
            _ => return 0.0,
        };
        let doc = self.doc;
        let Some(style) = doc
            .styles
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&style))
            .or_else(|| doc.styles.iter().find(|candidate| candidate.name.eq_ignore_ascii_case("STANDARD")))
        else {
            return 0.0;
        };
        let file = style.font.as_str();
        let family = style.font_family.as_deref().filter(|family| !family.is_empty());
        let lower = file.to_lowercase();
        let truetype = [".ttf", ".ttc", ".otf"].iter().any(|suffix| lower.ends_with(suffix))
            || family.is_some()
            || (!file.is_empty() && !lower.ends_with(".shx") && file.chars().any(|c| !(' '..='~').contains(&c)));
        let name = if truetype { family.unwrap_or(file) } else { file };
        let name = name.to_string();
        let lower = name.to_lowercase();
        let name = [".ttf", ".ttc", ".otf", ".shx"]
            .iter()
            .find_map(|suffix| lower.ends_with(suffix).then(|| name[..name.len() - suffix.len()].to_string()))
            .unwrap_or(name);
        let name = if !truetype && name.is_empty() { "txt".to_string() } else { name };
        self.font_code(&name)
    }

    // --- 仕上げ ----------------------------------------------------------------------------------------

    /// 表示が描く形から写す図形を、表示を作って SXF の要素にする（すべてを 1 つの図面にまとめて 1 度だけ描く）
    fn render_pending(&mut self) -> HashMap<usize, Vec<(SxfFeature, String)>> {
        let mut out: HashMap<usize, Vec<(SxfFeature, String)>> = HashMap::new();
        if self.pending.is_empty() {
            return out;
        }
        let mut synthetic = self.doc.clone();
        synthetic.entities.retain(|entity| entity.base.space == DxfSpace::Block && !self.is_layout_block(entity));
        synthetic.entities.extend(self.pending.iter().map(|pending| pending.entity.clone()));
        synthetic.sort_handles.retain(|(name, _)| !name.eq_ignore_ascii_case("*MODEL_SPACE"));
        synthetic.notices = Vec::new();
        let scene = dxf_to_scene(&synthetic, Some("Model"));
        for notice in &scene.notices {
            if notice.code != "paper-space" && notice.code != "unit-assumed" {
                self.notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
            }
        }
        let mut by_handle: HashMap<&str, Vec<&zumen_core::SceneEntity>> = HashMap::new();
        for entity in &scene.entities {
            by_handle.entry(top_id(&entity.id)).or_default().push(entity);
        }
        for index in 0..self.pending.len() {
            let handle = format!("ZS{index}");
            let Some(entities) = by_handle.get(handle.as_str()) else {
                continue;
            };
            let mut features = Vec::new();
            for entity in entities {
                // 表示は図面の単位を mm にして描く（弦の誤差も元の表示と同じ）。placement.inverse は表示 → 実寸の mm
                let inverse = entity.placement.as_ref().map_or(IDENTITY, |placement| placement.inverse);
                let unit = self.unit;
                let to_container = multiply(
                    &multiply(&self.pending[index].m, &[1.0 / unit, 0.0, 0.0, 1.0 / unit, 0.0, 0.0]),
                    &inverse,
                );
                for primitive in &entity.primitives {
                    self.primitive(index, primitive, &to_container, &mut features);
                }
            }
            if let Some(hatch) = self.pending[index].hatch.clone() {
                features = self.hatch(index, &hatch, features);
            }
            out.insert(index, features);
        }
        out
    }

    fn is_layout_block(&self, entity: &DxfEntity) -> bool {
        entity.base.block.as_deref().is_some_and(|name| {
            self.doc.blocks.iter().any(|block| block.layout && block.name.eq_ignore_ascii_case(name))
        })
    }

    /// 表示の 1 つの要素を SXF の要素にする（m は表示の座標 → 入れ物の座標）
    fn primitive(&mut self, index: usize, primitive: &Primitive, m: &Matrix, out: &mut Vec<(SxfFeature, String)>) {
        let (layer, dash_scale, source) = {
            let pending = &self.pending[index];
            (pending.layer, pending.dash_scale, pending.source.clone())
        };
        let linetype = self.pending[index].linetype.clone();
        match primitive {
            Primitive::Stroke(stroke) => {
                let dashes: Vec<f64> = stroke.dashes.iter().map(|value| value * dash_scale).collect();
                let color = self.color_code(paint_of_hex(&stroke.color));
                let line_type = self.line_type_code(&linetype, &dashes);
                let line_width = self.width_code(stroke.width);
                let points = zumen_core::geometry::transform_points(m, &stroke.points);
                let pieces: Vec<Vec<f64>> = if stroke.segments == Some(true) {
                    points.chunks(4).filter(|chunk| chunk.len() == 4).map(<[f64]>::to_vec).collect()
                } else {
                    let mut points = points;
                    if stroke.closed && points.len() >= 4 {
                        points.extend_from_slice(&[points[0], points[1]]);
                    }
                    vec![points]
                };
                for piece in pieces {
                    if piece.len() < 4 {
                        continue;
                    }
                    let style = Styled { id: self.id(), layer, color, line_type, line_width };
                    let (xs, ys) = piece.chunks(2).map(|point| (point[0], point[1])).unzip();
                    out.push((SxfFeature::Polyline { style, xs, ys }, source.clone()));
                }
            }
            Primitive::Fill(fill) => {
                let rings: Vec<Vec<f64>> = fill
                    .rings
                    .iter()
                    .map(|ring| zumen_core::geometry::transform_points(m, ring))
                    .filter(|ring| ring.len() >= 6)
                    .collect();
                let color = self.color_code(paint_of_hex(&fill.color));
                for region in self.fill_regions(&rings, layer) {
                    let id = self.id();
                    out.push((SxfFeature::FillAreaStyleColour { id, layer, color, region }, source.clone()));
                }
            }
            Primitive::Clipped(clipped) => {
                for stroke in &clipped.strokes {
                    self.primitive(index, &Primitive::Stroke(stroke.clone()), m, out);
                }
            }
            Primitive::Dot(dot) => {
                let color = self.color_code(paint_of_hex(&dot.color));
                let (x, y) = apply(m, dot.x, dot.y);
                let id = self.id();
                let feature = SxfFeature::PointMarker { id, layer, color, x, y, code: 3.0, angle: 0.0, scale: 1.0 };
                out.push((feature, source));
            }
            Primitive::Text(text) => {
                // SXF の文字範囲の高さは、TEXT の高さ（漢字・大文字の高さ）にする
                let vertical = is_vertical(text);
                let text = &text_height_box(text, vertical);
                let matrix = multiply(m, &text.matrix);
                let Some((x, y, angle, rx, ry)) = decompose_text(&matrix) else {
                    self.notices.add(NoticeLevel::Warning, "text-skew", "ゆがんだ文字は書いていません");
                    return;
                };
                let (x, y) = if mirrors(&matrix) {
                    self.notices.add(
                        NoticeLevel::Info,
                        "text-mirror",
                        "裏返しの文字は、SXF に書けないため、同じ文字範囲に裏返さずに書きました",
                    );
                    // 裏返した文字範囲の上端が、裏返さない文字範囲の下端になる
                    apply(&matrix, 0.0, text.box_height)
                } else {
                    (x, y)
                };
                let (shear, ry) = ry;
                let font = self.pending[index].font;
                let color = self.color_code(paint_of_hex(&text.color));
                let id = self.id();
                let annotation = Annotation {
                    font,
                    text: text.text.clone(),
                    x,
                    y,
                    height: text.box_height * ry,
                    width: text.box_width * rx,
                    spacing: 0.0,
                    angle,
                    slant: shear.atan().to_degrees(),
                    anchor: 1.0,
                    direction: if vertical { 2.0 } else { 1.0 },
                };
                out.push((SxfFeature::TextString { id, layer, color, text: annotation }, source));
            }
        }
    }

    /// 塗りの輪郭を、複合曲線（見えない外形）と SXF のハッチングの領域にする
    fn fill_regions(&mut self, rings: &[Vec<f64>], layer: f64) -> Vec<Region> {
        let mut out = Vec::new();
        let mut numbers: HashMap<usize, f64> = HashMap::new();
        for (outer, inner) in regions(rings) {
            let mut curve = |this: &mut Self, ring: usize| -> f64 {
                if let Some(number) = numbers.get(&ring) {
                    return *number;
                }
                let mut points = rings[ring].clone();
                points.extend_from_slice(&[points[0], points[1]]);
                let (xs, ys) = points.chunks(2).map(|point| (point[0], point[1])).unzip();
                let style = Styled { id: this.id(), layer, color: 1.0, line_type: 1.0, line_width: 1.0 };
                let id = this.id();
                this.composite_curves.push(CompositeCurve {
                    feature: CompositeCurveOrg { id, color: 1.0, line_type: 1.0, line_width: 1.0, visible: false },
                    members: vec![SxfFeature::Polyline { style, xs, ys }],
                });
                let number = this.composite_curves.len() as f64;
                numbers.insert(ring, number);
                number
            };
            let out_id = curve(self, outer);
            let in_ids = inner.iter().map(|ring| curve(self, *ring)).collect();
            out.push(Region { out_id, in_ids });
        }
        out
    }

    /// ハッチングの模様の線は、HATCH の模様の線の組を SXF のハッチング（ユーザ定義）にする（表示は塗りとして描かせ、
    /// その輪郭を領域にする）。間隔と角度は入れ物の座標、線種のピッチは用紙の mm（SXF 実装規約 1-7）
    fn hatch(
        &mut self,
        index: usize,
        hatch: &DxfHatch,
        features: Vec<(SxfFeature, String)>,
    ) -> Vec<(SxfFeature, String)> {
        let pending = &self.pending[index];
        let (layer, m, container_scale, paint, width) =
            (pending.layer, pending.m, pending.paper_scale, pending.paint, pending.lineweight);
        let ocs = multiply(&m, &ocs_matrix(pending.entity.base.extrusion.as_ref(), hatch.elevation));
        let color = self.color_code(paint);
        let line_width = self.width_code(width);
        // 模様の破線は、表示と同じく図形の座標（OCS）の長さに縮尺を掛けた用紙の長さ
        let paper_scale = container_scale * length_scale(&ocs);
        let mut patterns = Vec::new();
        for line in &hatch.pattern_lines {
            let (sin, cos) = line.angle.to_radians().sin_cos();
            let (dx, dy) = apply_vector(&ocs, cos, sin);
            let length = dx.hypot(dy);
            if length.is_nan() || length <= 0.0 {
                continue;
            }
            let (ux, uy) = (dx / length, dy / length);
            let (ox, oy) = apply_vector(&ocs, line.offset[0], line.offset[1]);
            let spacing = (ux * oy - uy * ox).abs();
            if spacing.is_nan() || spacing <= 0.0 {
                continue;
            }
            let (x, y) = apply(&ocs, line.base[0], line.base[1]);
            let dashes = dash_array(&line.dashes, paper_scale);
            let line_type = self.line_type_code(&hatch.pattern_name, &dashes);
            patterns.push(HatchPattern {
                color,
                line_type,
                line_width,
                x,
                y,
                spacing,
                angle: normalize(uy.atan2(ux).to_degrees()),
            });
        }
        // 表示と同じく、模様のない・線の多すぎる（5,000 本を超える）ハッチングは外形の線だけにする
        let rings: Vec<(Vec<f64>, Vec<f64>)> = features
            .iter()
            .filter_map(|(feature, _)| match feature {
                SxfFeature::FillAreaStyleColour { region, .. } => Some(region),
                _ => None,
            })
            .flat_map(|region| std::iter::once(region.out_id).chain(region.in_ids.iter().copied()))
            .filter_map(|number| {
                zumen_sxf::tables::index(number - 1.0).and_then(|index| self.composite_curves.get(index))
            })
            .filter_map(|curve| match curve.members.first() {
                Some(SxfFeature::Polyline { xs, ys, .. }) => Some((xs.clone(), ys.clone())),
                _ => None,
            })
            .collect();
        let mut lines = 0.0;
        for pattern in &patterns {
            let (sin, cos) = pattern.angle.to_radians().sin_cos();
            let along_normal =
                rings.iter().flat_map(|(xs, ys)| xs.iter().zip(ys)).map(|(x, y)| (y * cos - x * sin) / pattern.spacing);
            let (low, high) =
                along_normal.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), k| (low.min(k), high.max(k)));
            lines += high.ceil() - low.floor() + 1.0;
        }
        if patterns.is_empty() || lines.is_nan() || lines > 5000.0 {
            self.notices.add(
                NoticeLevel::Info,
                "hatch-dense",
                "模様のない・線の多すぎるハッチングは、表示と同じく外形の線だけを書きました",
            );
            let source = self.pending[index].source.clone();
            return rings
                .into_iter()
                .map(|(xs, ys)| {
                    let style =
                        Styled { id: self.id(), layer, color, line_type: 1.0, line_width: self.width_code(0.0) };
                    (SxfFeature::Polyline { style, xs, ys }, source.clone())
                })
                .collect();
        }
        let mut out = Vec::new();
        for (feature, source) in features {
            let SxfFeature::FillAreaStyleColour { id, region, .. } = feature else {
                out.push((feature, source));
                continue;
            };
            // 模様の線の基点を、領域の近くの同じ線の上に移す（遠い基点は、線を数える計算の桁が足りなくなる）
            let near = zumen_sxf::tables::index(region.out_id - 1.0)
                .and_then(|index| self.composite_curves.get(index))
                .and_then(|curve| match curve.members.first() {
                    Some(SxfFeature::Polyline { xs, ys, .. }) => Some((xs[0], ys[0])),
                    _ => None,
                });
            let patterns = patterns
                .iter()
                .filter_map(|pattern| {
                    let Some((px, py)) = near else {
                        return Some(pattern.clone());
                    };
                    let (sin, cos) = pattern.angle.to_radians().sin_cos();
                    let (nx, ny) = (-sin, cos);
                    let k = ((pattern.x - px) * nx + (pattern.y - py) * ny) / pattern.spacing;
                    let steps = k.round();
                    let (x, y) = (pattern.x - nx * steps * pattern.spacing, pattern.y - ny * steps * pattern.spacing);
                    // 実線は、線の向きにも基点を領域の近くへ移す（破線は線分の位置が変わるので移さない）
                    let along = if pattern.line_type == 1.0 { (x - px) * cos + (y - py) * sin } else { 0.0 };
                    let (x, y) = (x - cos * along, y - sin * along);
                    (x.is_finite() && y.is_finite()).then(|| HatchPattern { x, y, ..pattern.clone() })
                })
                .collect::<Vec<_>>();
            if !patterns.is_empty() {
                out.push((SxfFeature::FillAreaStyleHatching { id, layer, patterns, region }, source));
            }
        }
        out
    }

    fn finish(mut self, mut sheet: DrawingSheet) -> MappedSxf {
        let rendered = self.render_pending();
        let resolve = |this: &mut Self, items: Vec<Item>| -> Vec<SxfFeature> {
            let mut out = Vec::new();
            for item in items {
                match item {
                    Item::Feature(feature, source) => {
                        let feature = *feature;
                        this.sources.insert(format!("#{}", js::number_to_string(feature.id())), source);
                        out.push(feature);
                    }
                    Item::Pending(index) => {
                        for (feature, source) in rendered.get(&index).cloned().unwrap_or_default() {
                            this.sources.insert(format!("#{}", js::number_to_string(feature.id())), source);
                            out.push(feature);
                        }
                    }
                }
            }
            out
        };
        let items = std::mem::take(&mut self.sheet_items);
        let sheet_members = resolve(&mut self, items);
        let mut sub_figures = SubFigures::default();
        for (feature, items) in std::mem::take(&mut self.figures) {
            let members = resolve(&mut self, items);
            sub_figures.set(feature.name.clone(), SubFigure { feature, members });
        }
        sheet.id = self.id();
        let document = SxfDocument {
            sheet: Some(sheet),
            title: None,
            layers: std::mem::take(&mut self.tables.layers),
            user_colors: std::mem::take(&mut self.tables.colors),
            user_line_types: std::mem::take(&mut self.tables.line_types),
            user_widths: std::mem::take(&mut self.tables.widths),
            fonts: std::mem::take(&mut self.tables.fonts),
            composite_curves: std::mem::take(&mut self.composite_curves),
            sub_figures,
            sheet_members,
            counts: Vec::new(),
        };
        let drawing = SxfDrawing {
            format: "sfc",
            version: "SXF 3.1".into(),
            header: SxfHeader::default(),
            document,
            notices: Vec::new(),
            source: None,
        };
        MappedSxf { drawing, notices: self.notices.into_vec(), sources: self.sources }
    }
}

impl Paint {
    /// トゥルーカラーの白は SXF の白（ACI 7 の前景色と違い、表示の色が白の図形）
    pub(crate) fn with_white(self) -> Paint {
        match self {
            Paint::Foreground => Paint::Rgb([255, 255, 255]),
            other => other,
        }
    }
}

/// 文字範囲の変換（平行移動 · 回転 · 一様な拡大 · 斜体のずれ）を、配置基点・角度・X の倍率と、斜体のずれ・Y の倍率に分ける
pub(crate) fn decompose_text(m: &Matrix) -> Option<TextPlacement> {
    let rx = m[0].hypot(m[1]);
    if !(rx.is_finite() && rx > 0.0) {
        return None;
    }
    let angle = m[1].atan2(m[0]);
    let (sin, cos) = angle.sin_cos();
    // R(−angle) · 2 列目 = (ずれ × ry, ry)。裏返しは ry を正にして書く
    let c = cos * m[2] + sin * m[3];
    let d = (-sin * m[2] + cos * m[3]).abs();
    if d.is_nan() || d <= 0.0 {
        return None;
    }
    Some((m[4], m[5], normalize(angle.to_degrees()), rx, (c / d, d)))
}

/// 線分を矩形（最小 x・y、最大 x・y）で切る（Liang–Barsky）。外なら None
pub(crate) fn clip_segment([x1, y1, x2, y2]: [f64; 4], [min_x, min_y, max_x, max_y]: [f64; 4]) -> Option<[f64; 4]> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, x1 - min_x), (dx, max_x - x1), (-dy, y1 - min_y), (dy, max_y - y1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
        if t0 > t1 {
            return None;
        }
    }
    Some([x1 + dx * t0, y1 + dy * t0, x1 + dx * t1, y1 + dy * t1])
}
