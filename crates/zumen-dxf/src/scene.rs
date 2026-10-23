//! AutoCAD の図面のモデルを、モデル空間か 1 つのレイアウトの表示用の図面にする（`@zumenjs/dxf` の scene.ts と同じ順で描く）

use crate::colors::{aci_color, is_transparent, true_color};
use crate::curves::{
    arc_points, bulge_points, clip_polygon, clip_polyline, cut_polyline, ellipse_points, minor_axis, nurbs_points,
    ocs_axes, ocs_matrix, through_points,
};
use crate::model::*;
use crate::mtext::{
    ParagraphAlign, RunColor, RunFont, RunStack, TextRun, mtext_plain_text, parse_mtext, parse_text_codes,
};
use crate::parse::or;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use zumen_core::geometry::{
    IDENTITY, apply, apply_vector, empty_box, extend_box, extend_box_with_points, invert, is_empty_box, mean_scale,
    multiply, transform_points, union_box,
};
use zumen_core::text::{EM_LATIN, TextLayoutInput, font_family_for, is_half_width, layout_text};
use zumen_core::{
    BACKGROUND_COLOR, BBox, Background, ClippedPrimitive, DotPrimitive, EntityKind, FillPrimitive, LengthUnit, Matrix,
    NoticeLevel, NoticeList, Placement, Primitive, Property, Scene, SceneEntity, SceneLayer, SceneView, Sheet,
    StrokePrimitive, js,
};

/// $INSUNITS の単位の mm（DXF Reference「HEADER Section Group Codes」の $INSUNITS）
pub(crate) fn unit_to_mm(code: f64) -> Option<f64> {
    if !(1.0..=21.0).contains(&code) || code.fract() != 0.0 {
        return None;
    }
    Some(match code as u32 {
        1 => 25.4,
        2 => 304.8,
        3 => 1_609_344.0,
        4 => 1.0,
        5 => 10.0,
        6 => 1000.0,
        7 => 1_000_000.0,
        8 => 0.0000254,
        9 => 0.0254,
        10 => 914.4,
        11 => 1e-7,
        12 => 1e-6,
        13 => 0.001,
        14 => 100.0,
        15 => 10_000.0,
        16 => 100_000.0,
        17 => 1e12,
        18 => 1.495978707e14,
        19 => 9.4607304725808e18,
        20 => 3.0856775814914e19,
        _ => 1_200_000.0 / 3937.0,
    })
}

fn unit_name(code: f64) -> Option<&'static str> {
    if code.fract() != 0.0 || !(0.0..=21.0).contains(&code) {
        return None;
    }
    match code as u32 {
        0 => Some("単位なし"),
        1 => Some("インチ"),
        2 => Some("フィート"),
        3 => Some("マイル"),
        4 => Some("mm"),
        5 => Some("cm"),
        6 => Some("m"),
        7 => Some("km"),
        9 => Some("ミル"),
        10 => Some("ヤード"),
        13 => Some("μm"),
        14 => Some("dm"),
        21 => Some("US 測量フィート"),
        _ => None,
    }
}

/// $INSUNITS の単位の、形式によらないキー（Scene::unit）
fn length_unit(code: f64) -> Option<LengthUnit> {
    if code.fract() != 0.0 || !(1.0..=21.0).contains(&code) {
        return None;
    }
    Some(match code as u32 {
        1 => LengthUnit::Inch,
        2 => LengthUnit::Foot,
        3 => LengthUnit::Mile,
        4 => LengthUnit::Millimeter,
        5 => LengthUnit::Centimeter,
        6 => LengthUnit::Meter,
        7 => LengthUnit::Kilometer,
        8 => LengthUnit::Microinch,
        9 => LengthUnit::Mil,
        10 => LengthUnit::Yard,
        11 => LengthUnit::Angstrom,
        12 => LengthUnit::Nanometer,
        13 => LengthUnit::Micrometer,
        14 => LengthUnit::Decimeter,
        15 => LengthUnit::Decameter,
        16 => LengthUnit::Hectometer,
        17 => LengthUnit::Gigameter,
        18 => LengthUnit::AstronomicalUnit,
        19 => LengthUnit::LightYear,
        20 => LengthUnit::Parsec,
        _ => LengthUnit::UsSurveyFoot,
    })
}

fn release(version: &str) -> Option<&'static str> {
    match version {
        "AC1009" => Some("R12"),
        "AC1012" => Some("R13"),
        "AC1014" => Some("R14"),
        "AC1015" => Some("2000"),
        "AC1018" => Some("2004"),
        "AC1021" => Some("2007"),
        "AC1024" => Some("2010"),
        "AC1027" => Some("2013"),
        "AC1032" => Some("2018"),
        _ => None,
    }
}

fn type_name(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "LINE" => "線分",
        "CIRCLE" => "円",
        "ARC" => "円弧",
        "ELLIPSE" => "楕円",
        "POINT" => "点",
        "LWPOLYLINE" | "POLYLINE" => "ポリライン",
        "SPLINE" => "スプライン",
        "HELIX" => "らせん",
        "TEXT" => "文字",
        "MTEXT" => "複数行文字",
        "ATTRIB" => "属性",
        "ATTDEF" => "属性定義",
        "DIMENSION" => "寸法",
        "ARC_DIMENSION" => "弧長寸法",
        "LARGE_RADIAL_DIMENSION" => "折り曲げ半径寸法",
        "HATCH" => "ハッチング",
        "MPOLYGON" => "塗りの多角形",
        "SOLID" => "塗り（ソリッド）",
        "TRACE" => "太線（トレース）",
        "3DFACE" => "3D 面",
        "LEADER" => "引出線",
        "MULTILEADER" | "MLEADER" => "マルチ引出線",
        "MLINE" => "マルチライン",
        "VIEWPORT" => "ビューポート",
        "XLINE" => "構築線",
        "RAY" => "放射線",
        "IMAGE" => "画像",
        "WIPEOUT" => "ワイプアウト",
        _ => return None,
    })
}

/// 図形の、形式によらない種類（docs/architecture.md の「Stable keys」）
fn entity_kind(kind: &str) -> Option<EntityKind> {
    Some(match kind {
        "LINE" => EntityKind::Line,
        "CIRCLE" => EntityKind::Circle,
        "ARC" => EntityKind::Arc,
        "ELLIPSE" => EntityKind::Ellipse,
        "POINT" => EntityKind::Point,
        "LWPOLYLINE" | "POLYLINE" => EntityKind::Polyline,
        "SPLINE" => EntityKind::Spline,
        "HELIX" => EntityKind::Helix,
        "TEXT" | "MTEXT" => EntityKind::Text,
        "ATTRIB" => EntityKind::Attribute,
        "ATTDEF" => EntityKind::AttributeDefinition,
        "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => EntityKind::Dimension,
        "HATCH" | "MPOLYGON" => EntityKind::Hatch,
        "SOLID" | "TRACE" => EntityKind::Solid,
        "3DFACE" => EntityKind::Mesh,
        "LEADER" | "MULTILEADER" | "MLEADER" => EntityKind::Leader,
        "MLINE" => EntityKind::Multiline,
        "VIEWPORT" => EntityKind::Viewport,
        "XLINE" | "RAY" => EntityKind::ConstructionLine,
        "IMAGE" => EntityKind::Image,
        "WIPEOUT" => EntityKind::Wipeout,
        _ => return None,
    })
}

const DIMENSION_KINDS: [&str; 7] =
    ["長さ寸法", "平行寸法", "角度寸法", "直径寸法", "半径寸法", "3 点角度寸法", "座標寸法"];
/// 寸法の種類の、形式によらないキー（70 の下位 3 ビット、ARC_DIMENSION、LARGE_RADIAL_DIMENSION）
const DIMENSION_TYPES: [&str; 7] =
    ["linear", "aligned", "angular", "diameter", "radius", "angular-3-point", "ordinate"];

/// 弦と弧のずれの許容（表示用の図面の mm）。大きい円は半径に比例して粗くする
const TOLERANCE: f64 = 0.02;
const MAX_DEPTH: u32 = 32;
/// ブロック参照・寸法の中で描く図形の数の上限。自分を 2 回以上含むブロックは、入れ子の深さの上限（32 段）より前に
/// 図形が指数的に増えて、読み取りが終わらずにメモリを使い果たす
const MAX_BLOCK_ENTITIES: u64 = 1_000_000;
const MAX_PATTERN_LINES: f64 = 5000.0;
/// JavaScript の Number.MAX_SAFE_INTEGER（2⁵³ − 1）。これより大きい数は、1 を足しても変わらないことがある
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
const DEFAULT_LINEWEIGHT: f64 = 0.25;
/// em の下端からベースラインまで（em に対する比）
pub const DESCENT: f64 = 0.12;
/// MTEXT の既定の行の間隔（文字の高さの 5/3、DXF Reference「MTEXT」の 44）
const LINE_SPACING: f64 = 5.0 / 3.0;
/// 引出線を矢印の先から始める矢印（ほかの矢印は、矢印の大きさだけ離れた矢印の根元から線を始める）。
/// AutoCAD の矢印のブロックの名前（_ArchTick など）から _ を除いたもの。ezdxf の render/arrows.py の ORIGIN_ZERO と同じ
const ARROWS_FROM_TIP: [&str; 6] = ["ARCHTICK", "OBLIQUE", "DOTSMALL", "SMALL", "INTEGRAL", "NONE"];

/// MLINESTYLE が見つからないマルチラインの要素（AutoCAD の STANDARD と同じ ±0.5 の 2 本）
static STANDARD_MLINE_STYLE: LazyLock<DxfMLineStyle> = LazyLock::new(|| DxfMLineStyle {
    handle: String::new(),
    name: "STANDARD".into(),
    flags: 0.0,
    description: String::new(),
    fill_color: 256.0,
    start_angle: 90.0,
    end_angle: 90.0,
    elements: vec![
        DxfMLineStyleElement { offset: 0.5, color: 256.0, linetype: "BYLAYER".into() },
        DxfMLineStyleElement { offset: -0.5, color: 256.0, linetype: "BYLAYER".into() },
    ],
});

#[derive(Debug, Clone)]
struct Ctx {
    /// 今の座標系（ブロックの中など） → 表示用の図面（mm）
    m: Matrix,
    /// 今の座標系 → その空間の WCS（属性に示す座標）
    local: Matrix,
    layers: Vec<u32>,
    /// ブロック参照のレイヤ。ブロックの中のレイヤ 0 の図形が継ぐ。最上位では None
    parent_layer: Option<String>,
    /// BYBLOCK の色・線の太さ（mm）・線種
    color: String,
    lineweight: f64,
    linetype: String,
    path: String,
    blocks: Vec<String>,
    depth: u32,
    placement: Option<Arc<Placement>>,
    /// 線種の長さの補正（紙空間の線種の尺度を揃えるビューポート）
    dash_factor: f64,
    /// ビューポートの切り抜き（表示用の図面の座標）
    clip: Option<BBox>,
    /// ビューポートで凍結したレイヤ（大文字）
    frozen: Option<Rc<HashSet<String>>>,
}

/// `Intl.NumberFormat("ja-JP", { maximumFractionDigits: 4 }).format(Number(value.toFixed(4)))`
fn fmt(value: f64) -> String {
    if !value.is_finite() {
        return js::number_to_string(value);
    }
    js::format_ja(js::to_fixed(value, 4).parse::<f64>().unwrap_or(f64::NAN), 4)
}

fn point2(x: f64, y: f64) -> String {
    format!("{}, {}", fmt(x), fmt(y))
}

/// 座標の属性。raw は [x, y]
fn point_property(key: &'static str, label: &str, x: f64, y: f64) -> Property {
    Property::new(key, label, point2(x, y)).raw([x, y])
}

/// 数の属性
fn number_property(key: &'static str, label: &str, value: f64) -> Property {
    Property::new(key, label, fmt(value)).raw(value)
}

/// 「n 点」のように数える属性
fn count_property(key: &'static str, label: &str, count: usize, value: String) -> Property {
    Property::new(key, label, value).raw(count as f64)
}

fn translate(x: f64, y: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

fn rotate(deg: f64) -> Matrix {
    let r = (deg * PI) / 180.0;
    let c = js::cos(r);
    let s = js::sin(r);
    [c, s, -s, c, 0.0, 0.0]
}

fn scale(sx: f64, sy: f64) -> Matrix {
    [sx, 0.0, 0.0, sy, 0.0, 0.0]
}

/// `value` が真（0 でも NaN でもない）か
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

/// `Math.max(...values)`
fn max_of(values: &[f64]) -> f64 {
    values.iter().fold(f64::NEG_INFINITY, |max, value| js::max(max, *value))
}

/// `Math.min(...values)`
fn min_of(values: &[f64]) -> f64 {
    values.iter().fold(f64::INFINITY, |min, value| js::min(min, *value))
}

fn extend_primitive_box(b: &mut BBox, primitive: &Primitive) {
    match primitive {
        Primitive::Stroke(stroke) => extend_box_with_points(b, &stroke.points),
        Primitive::Fill(FillPrimitive { rings, .. }) | Primitive::Clipped(ClippedPrimitive { rings, .. }) => {
            for ring in rings {
                extend_box_with_points(b, ring);
            }
        }
        Primitive::Dot(dot) => extend_box(b, dot.x, dot.y),
        Primitive::Text(text) => {
            let m = text.matrix;
            for (x, y) in [(0.0, 0.0), (text.box_width, 0.0), (text.box_width, text.box_height), (0.0, text.box_height)]
            {
                extend_box(b, m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            }
        }
    }
}

fn inside(inner: &BBox, outer: &BBox) -> bool {
    inner.min_x >= outer.min_x && inner.max_x <= outer.max_x && inner.min_y >= outer.min_y && inner.max_y <= outer.max_y
}

fn intersects(a: &BBox, b: &BBox) -> bool {
    !(a.max_x < b.min_x || a.min_x > b.max_x || a.max_y < b.min_y || a.min_y > b.max_y)
}

/// 線種（49）を Canvas の線・空白の並びにする。正は線、負は空白、0 は点
pub fn dash_array(pattern: &[f64], factor: f64) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    if factor.is_nan() || factor <= 0.0 {
        return out;
    }
    for &value in pattern {
        let length = value.abs() * factor;
        let on = value >= 0.0;
        let expect_on = out.len().is_multiple_of(2);
        if on == expect_on {
            out.push(length);
        } else if out.is_empty() {
            out.push(0.0);
            out.push(length);
        } else {
            let last = out.len() - 1;
            out[last] += length;
        }
    }
    // 線で終わる並びは、次の周期の初めの線とつながる
    if out.len() % 2 == 1 {
        let tail = out.pop().expect("要素がある");
        if !out.is_empty() {
            out[0] += tail;
        }
    }
    if out.is_empty() || out.iter().enumerate().all(|(index, value)| index % 2 == 1 || *value == 0.0) {
        // 空白だけ・点だけの並びも描けるように残す。空白がない並びは実線
        return if out.iter().enumerate().any(|(index, value)| index % 2 == 1 && *value > 0.0) {
            out
        } else {
            Vec::new()
        };
    }
    out
}

fn units(text: &str) -> f64 {
    text.chars().fold(0.0, |total, c| total + if is_half_width(c) { 0.5 } else { 1.0 })
}

/// 文字の高さ（TEXT の 40）に対する字形の em の大きさ（`zumen_core::text::em_for`）
pub fn em_for(text: &str) -> f64 {
    zumen_core::text::em_for(text)
}

/// 描いた MTEXT の範囲。frame は文字の座標系から今の座標系（ctx.local の前）への変換
struct MTextLayout {
    frame: Matrix,
    bbox: BBox,
}

/// ハンドルの大小（16 進の文字列）
fn compare_handles(a: &str, b: &str) -> f64 {
    let (length_a, length_b) = (a.encode_utf16().count(), b.encode_utf16().count());
    if length_a != length_b {
        return length_a as f64 - length_b as f64;
    }
    match a.encode_utf16().cmp(b.encode_utf16()) {
        std::cmp::Ordering::Less => -1.0,
        std::cmp::Ordering::Greater => 1.0,
        std::cmp::Ordering::Equal => 0.0,
    }
}

/// ブロックの中のレイヤ 0 の図形は、ブロック参照のレイヤに属する
fn effective_layer<'b>(entity: &'b DxfEntityBase, ctx: &'b Ctx) -> &'b str {
    match &ctx.parent_layer {
        Some(parent) if entity.layer == "0" => parent,
        _ => &entity.layer,
    }
}

/// 弦と弧のずれの許容（m で表示用の図面に写す前の座標）
fn tolerance(m: &Matrix, radius: f64) -> f64 {
    let s = js::max(mean_scale(m), 1e-12);
    js::max(TOLERANCE, radius * s * 1e-4) / s
}

/// id のうち図形の部分。ハンドル、ハンドルのない図形（R12 の図面など）はファイルの中の位置（#3）、
/// どちらもなければ ?（docs/architecture.md の「Entity ids」）
fn handle_or_unknown(entity: &DxfEntityBase) -> Cow<'_, str> {
    if !entity.handle.is_empty() {
        Cow::Borrowed(&entity.handle)
    } else if entity.ordinal > 0 {
        Cow::Owned(format!("#{}", entity.ordinal))
    } else {
        Cow::Borrowed("?")
    }
}

fn flat<const N: usize>(points: &[[f64; N]]) -> Vec<f64> {
    points.iter().flat_map(|point| [point[0], point[1]]).collect()
}

fn slices<const N: usize>(points: &[[f64; N]]) -> Vec<&[f64]> {
    points.iter().map(|point| point.as_slice()).collect()
}

fn stroke_primitive(points: Vec<f64>, closed: bool, color: String) -> StrokePrimitive {
    StrokePrimitive { points, closed, segments: None, color, width: 0.0, dashes: Vec::new() }
}

struct BlockContents<'a> {
    block: Option<&'a DxfBlock>,
    entities: Rc<Vec<&'a DxfEntity>>,
}

/// 部品の色（MULTILEADER の生の色の値から）
#[derive(Clone, Copy)]
struct PartColor {
    color_index: f64,
    true_color: Option<f64>,
}

struct Builder<'a> {
    doc: &'a DxfDocument,
    entities: Vec<SceneEntity>,
    used_layers: HashMap<u32, u64>,
    notices: NoticeList,
    layer_ids: HashMap<String, u32>,
    layer_list: Vec<DxfLayer>,
    linetypes: HashMap<String, &'a DxfLinetype>,
    styles: HashMap<String, &'a DxfTextStyle>,
    dimstyles: HashMap<String, &'a DxfDimStyle>,
    blocks: HashMap<String, BlockContents<'a>>,
    /// ハンドルで引く表（MULTILEADER・MLINE はハンドルで線種・文字スタイル・ブロックを指す）
    linetype_handles: HashMap<String, &'a DxfLinetype>,
    style_handles: HashMap<String, &'a DxfTextStyle>,
    block_records: HashMap<String, &'a DxfBlock>,
    mline_style_handles: HashMap<String, &'a DxfMLineStyle>,
    mline_style_names: HashMap<String, &'a DxfMLineStyle>,
    model: Rc<Vec<&'a DxfEntity>>,
    /// SHX のフォント（加えた順で重複なし）
    shx_fonts: Vec<String>,
    shx_count: u64,
    /// ブロック参照・寸法の中で描いた図形の数
    expanded: u64,
    infinite: Vec<(DxfEntity, Ctx)>,
    /// Some なら、描いた文字の run を 1 行の文字として覚える（`Exploder`）
    pieces: Option<Vec<TextPiece>>,
}

impl<'a> Builder<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        let mut builder = Builder {
            doc,
            entities: Vec::new(),
            used_layers: HashMap::new(),
            notices: NoticeList::from_notices(&doc.notices),
            layer_ids: HashMap::new(),
            layer_list: Vec::new(),
            linetypes: HashMap::new(),
            styles: HashMap::new(),
            dimstyles: HashMap::new(),
            blocks: HashMap::new(),
            linetype_handles: HashMap::new(),
            style_handles: HashMap::new(),
            block_records: HashMap::new(),
            mline_style_handles: HashMap::new(),
            mline_style_names: HashMap::new(),
            model: Rc::new(Vec::new()),
            shx_fonts: Vec::new(),
            shx_count: 0,
            expanded: 0,
            infinite: Vec::new(),
            pieces: None,
        };
        for layer in &doc.layers {
            builder.layer(&layer.name, Some(layer));
        }
        for linetype in &doc.linetypes {
            builder.linetypes.insert(linetype.name.to_uppercase(), linetype);
            if !linetype.handle.is_empty() {
                builder.linetype_handles.insert(linetype.handle.clone(), linetype);
            }
        }
        for style in &doc.styles {
            builder.styles.insert(style.name.to_uppercase(), style);
            if !style.handle.is_empty() {
                builder.style_handles.insert(style.handle.clone(), style);
            }
        }
        for style in &doc.dimstyles {
            builder.dimstyles.insert(style.name.to_uppercase(), style);
        }
        let mut contents: HashMap<String, (Option<&'a DxfBlock>, Vec<&'a DxfEntity>)> = HashMap::new();
        for block in &doc.blocks {
            contents.insert(block.name.to_uppercase(), (Some(block), Vec::new()));
            if let Some(handle) = block.block_record_handle.as_ref().filter(|handle| !handle.is_empty()) {
                builder.block_records.insert(handle.clone(), block);
            }
        }
        for style in doc.mline_styles.iter().flatten() {
            if !style.handle.is_empty() {
                builder.mline_style_handles.insert(style.handle.clone(), style);
            }
            builder.mline_style_names.insert(style.name.to_uppercase(), style);
        }
        let mut model = Vec::new();
        for entity in &doc.entities {
            if entity.base.space == DxfSpace::Model {
                model.push(entity);
                continue;
            }
            let name = entity.base.block.as_deref().unwrap_or("*Paper_Space").to_uppercase();
            contents.entry(name).or_insert_with(|| (None, Vec::new())).1.push(entity);
        }
        builder.sort_by_draw_order("*MODEL_SPACE", &mut model);
        builder.model = Rc::new(model);
        for (name, (block, mut entities)) in contents {
            builder.sort_by_draw_order(&name, &mut entities);
            builder.blocks.insert(name, BlockContents { block, entities: Rc::new(entities) });
        }
        builder
    }

    /// SORTENTSTABLE の並べ替えのハンドルの順に描く（表にない図形は自分のハンドル）
    fn sort_by_draw_order(&self, block_name: &str, list: &mut Vec<&'a DxfEntity>) {
        let Some((_, table)) = self.doc.sort_handles.iter().find(|(name, _)| name.to_uppercase() == block_name) else {
            return;
        };
        let keys: Vec<&str> = list
            .iter()
            .map(|entity| table.get(&entity.base.handle).map_or(entity.base.handle.as_str(), String::as_str))
            .collect();
        let mut order: Vec<usize> = (0..list.len()).collect();
        js::sort_by(&mut order, |&a, &b| {
            let first =
                if !keys[a].is_empty() && !keys[b].is_empty() { compare_handles(keys[a], keys[b]) } else { 0.0 };
            if truthy(first) { first } else { a as f64 - b as f64 }
        });
        *list = order.iter().map(|&index| list[index]).collect();
    }

    fn block_entities(&self, name: &str) -> Rc<Vec<&'a DxfEntity>> {
        self.blocks.get(&name.to_uppercase()).map_or_else(|| Rc::new(Vec::new()), |contents| contents.entities.clone())
    }

    fn layers(&self) -> Vec<SceneLayer> {
        let mut out = Vec::new();
        for (index, layer) in self.layer_list.iter().enumerate() {
            let id = index as u32 + 1;
            if !self.used_layers.contains_key(&id) {
                continue;
            }
            out.push(SceneLayer { id, name: layer.name.clone(), visible: !layer.off && !layer.frozen, group: None });
        }
        out
    }

    fn layer(&mut self, name: &str, definition: Option<&DxfLayer>) -> u32 {
        let key = name.to_uppercase();
        if let Some(id) = self.layer_ids.get(&key) {
            return *id;
        }
        self.layer_list.push(definition.cloned().unwrap_or_else(|| DxfLayer {
            handle: String::new(),
            name: name.to_string(),
            color_index: 7.0,
            true_color: None,
            frozen: false,
            off: false,
            locked: false,
            plot: true,
            linetype: "CONTINUOUS".into(),
            lineweight: -3.0,
            transparency: None,
        }));
        let id = self.layer_list.len() as u32;
        self.layer_ids.insert(key, id);
        id
    }

    fn layer_of(&mut self, name: &str) -> &DxfLayer {
        let id = self.layer(name, None);
        &self.layer_list[id as usize - 1]
    }

    /// 色（#rrggbb）。透過度があれば不透明度を足して #rrggbbaa にする
    fn color(&mut self, entity: &DxfEntityBase, ctx: &Ctx) -> String {
        let color = self.base_color(entity, ctx);
        // 透過度（440）がなければレイヤの透過度。0x020000TT の TT が不透明さ（DXF Reference「Common Group Codes for Entities」）
        let transparency = match entity.transparency {
            Some(value) => Some(value),
            None => self.layer_of(effective_layer(entity, ctx)).transparency,
        };
        if !is_transparent(transparency) || color.len() != 7 {
            return color;
        }
        format!("{color}{:02x}", js::to_int32(transparency.unwrap_or(0.0)) & 0xff)
    }

    fn base_color(&mut self, entity: &DxfEntityBase, ctx: &Ctx) -> String {
        if let Some(value) = entity.true_color {
            return true_color(value);
        }
        let index = entity.color_index;
        if index == 0.0 {
            return ctx.color.chars().take(7).collect();
        }
        if index == 256.0 || index == 257.0 || index < 0.0 || index > 257.0 {
            let layer = self.layer_of(effective_layer(entity, ctx));
            return match layer.true_color {
                Some(value) => true_color(value),
                None => aci_color(layer.color_index),
            };
        }
        aci_color(index)
    }

    /// 線の太さ（mm）
    fn lineweight(&mut self, entity: &DxfEntityBase, ctx: &Ctx) -> f64 {
        let value = entity.lineweight;
        if value == -2.0 {
            return ctx.lineweight;
        }
        if value == -1.0 {
            let layer = self.layer_of(effective_layer(entity, ctx));
            return if layer.lineweight >= 0.0 { layer.lineweight / 100.0 } else { DEFAULT_LINEWEIGHT };
        }
        if value >= 0.0 { value / 100.0 } else { DEFAULT_LINEWEIGHT }
    }

    fn linetype_name(&mut self, entity: &DxfEntityBase, ctx: &Ctx) -> String {
        let name = if entity.linetype.is_empty() { "BYLAYER" } else { entity.linetype.as_str() };
        let upper = name.to_uppercase();
        if upper == "BYBLOCK" {
            return ctx.linetype.clone();
        }
        if upper == "BYLAYER" {
            let linetype = &self.layer_of(effective_layer(entity, ctx)).linetype;
            return if linetype.is_empty() { "CONTINUOUS".into() } else { linetype.clone() };
        }
        name.to_string()
    }

    fn dashes(&mut self, entity: &DxfEntityBase, ctx: &Ctx, m: &Matrix) -> Vec<f64> {
        let name = self.linetype_name(entity, ctx);
        let upper = name.to_uppercase();
        if upper == "CONTINUOUS" || upper == "BYLAYER" || upper == "BYBLOCK" {
            return Vec::new();
        }
        let Some(linetype) = self.linetypes.get(&upper).copied() else {
            return Vec::new();
        };
        if linetype.dashes.is_empty() {
            return Vec::new();
        }
        if linetype.complex {
            self.notices.add(NoticeLevel::Info, "complex-linetype", "文字・形を含む線種は、線と空白だけで描きました");
        }
        let factor = self.doc.header.ltscale * entity.linetype_scale * mean_scale(m) * ctx.dash_factor;
        dash_array(&linetype.dashes, factor)
    }

    fn stroke(
        &mut self,
        entity: &DxfEntityBase,
        ctx: &Ctx,
        m: &Matrix,
        points: &[f64],
        closed: bool,
    ) -> StrokePrimitive {
        let points = transform_points(m, points);
        let color = self.color(entity, ctx);
        let width = self.lineweight(entity, ctx);
        let dashes = self.dashes(entity, ctx, m);
        StrokePrimitive { points, closed, segments: None, color, width, dashes }
    }

    /// 子の文脈（ブロック参照・寸法の中）。BYBLOCK の値とレイヤ 0 の継ぎ先を entity から決める
    fn child(&mut self, entity: &DxfEntityBase, ctx: &Ctx, local: &Matrix, path: String, block: &str) -> Ctx {
        let layer = effective_layer(entity, ctx).to_string();
        let id = self.layer(&layer, None);
        let mut layers = ctx.layers.clone();
        if !layers.contains(&id) {
            layers.push(id);
        }
        let color = self.color(entity, ctx);
        let lineweight = self.lineweight(entity, ctx);
        let linetype = self.linetype_name(entity, ctx);
        let mut blocks = ctx.blocks.clone();
        blocks.push(block.to_string());
        Ctx {
            m: multiply(&ctx.m, local),
            local: multiply(&ctx.local, local),
            layers,
            parent_layer: Some(layer),
            color,
            lineweight,
            linetype,
            path,
            blocks,
            depth: ctx.depth + 1,
            placement: ctx.placement.clone(),
            dash_factor: ctx.dash_factor,
            clip: ctx.clip,
            frozen: ctx.frozen.clone(),
        }
    }

    fn color_label(entity: &DxfEntityBase) -> String {
        if let Some(value) = entity.true_color {
            return true_color(value);
        }
        if entity.color_index == 0.0 {
            return "BYBLOCK".into();
        }
        if entity.color_index == 256.0 {
            return "BYLAYER".into();
        }
        format!("ACI {}", js::number_to_string(entity.color_index))
    }

    fn lineweight_label(value: f64) -> String {
        if value == -1.0 {
            return "BYLAYER".into();
        }
        if value == -2.0 {
            return "BYBLOCK".into();
        }
        if value == -3.0 {
            return "既定".into();
        }
        format!("{} mm", fmt(value / 100.0))
    }

    /// 表示用の図面の要素を加える。ビューポートの中では、切り抜きの矩形で線・塗りを切る
    fn add(
        &mut self,
        entity: &DxfEntityBase,
        ctx: &Ctx,
        primitives: Vec<Primitive>,
        properties: Vec<Property>,
        name: Option<(&str, EntityKind)>,
    ) {
        if primitives.is_empty() {
            return;
        }
        // 種類の名前と、形式によらない種類（楕円弧・メッシュ・寸法の種類など、DXF の名前だけでは決まらないもの）
        let (kind, entity_kind) = match name {
            Some((name, kind)) => (name.to_string(), kind),
            // 描く図形はどれも entity_kind にある
            None => (
                type_name(&entity.kind).unwrap_or(&entity.kind).to_string(),
                entity_kind(&entity.kind).unwrap_or(EntityKind::Line),
            ),
        };
        let mut parts = primitives;
        let mut bbox = empty_box();
        for primitive in &parts {
            extend_primitive_box(&mut bbox, primitive);
        }
        if is_empty_box(&bbox) {
            return;
        }
        if let Some(clip) = ctx.clip {
            if !intersects(&bbox, &clip) {
                return;
            }
            if !inside(&bbox, &clip) {
                parts = self.clip(parts, &clip);
                if parts.is_empty() {
                    return;
                }
                bbox = empty_box();
                for primitive in &parts {
                    extend_primitive_box(&mut bbox, primitive);
                }
                if is_empty_box(&bbox) {
                    return;
                }
            }
        }
        let layer = effective_layer(entity, ctx).to_string();
        let own = self.layer(&layer, None);
        let mut layers = ctx.layers.clone();
        if !layers.contains(&own) {
            layers.push(own);
        }
        for id in &layers {
            *self.used_layers.entry(*id).or_insert(0) += 1;
        }
        let id = format!("{}{}", ctx.path, handle_or_unknown(entity));
        let linetype = if entity.linetype.is_empty() { "BYLAYER" } else { entity.linetype.as_str() };
        let resolved = self.linetype_name(entity, ctx);
        let mut common = vec![
            Property::new("type", "種類", format!("{kind}（{}）", entity.kind)),
            Property::new("id", "ハンドル", id.clone()),
            Property::new("layer", "レイヤ", layer).raw(f64::from(own)),
        ];
        if !ctx.blocks.is_empty() {
            common.push(Property::new("block", "ブロック", ctx.blocks.join(" › ")));
        }
        let color = Property::new("color", "色", Self::color_label(entity));
        common.push(if entity.true_color.is_some() { color } else { color.raw(entity.color_index) });
        common.push(Property::new(
            "lineType",
            "線種",
            if linetype.to_uppercase() == resolved.to_uppercase() {
                linetype.to_string()
            } else {
                format!("{linetype}（{resolved}）")
            },
        ));
        let lineweight = Property::new("lineWidth", "線の太さ", Self::lineweight_label(entity.lineweight));
        common.push(if entity.lineweight >= 0.0 { lineweight.raw(entity.lineweight / 100.0) } else { lineweight });
        common.extend(properties);
        // 読み取りの上限（zumen_core::limits）。上限を超えたら図形を加えない
        if !zumen_core::limits::admit() {
            return;
        }
        self.entities.push(SceneEntity {
            id,
            kind: entity_kind,
            type_name: kind,
            source: entity.kind.clone(),
            layers,
            primitives: parts,
            bbox,
            properties: common,
            placement: ctx.placement.clone(),
        });
    }

    fn clip(&mut self, primitives: Vec<Primitive>, b: &BBox) -> Vec<Primitive> {
        let mut out = Vec::new();
        let mut partial_text = false;
        for primitive in primitives {
            match primitive {
                Primitive::Stroke(stroke) => {
                    if stroke.segments == Some(true) {
                        let mut kept: Vec<f64> = Vec::new();
                        let mut i = 0;
                        while i + 3 < stroke.points.len() {
                            for piece in clip_polyline(&stroke.points[i..i + 4], false, b) {
                                kept.extend(piece);
                            }
                            i += 4;
                        }
                        if kept.len() >= 4 {
                            out.push(Primitive::Stroke(StrokePrimitive { points: kept, ..stroke }));
                        }
                    } else {
                        for piece in clip_polyline(&stroke.points, stroke.closed, b) {
                            out.push(Primitive::Stroke(StrokePrimitive {
                                points: piece,
                                closed: false,
                                ..stroke.clone()
                            }));
                        }
                    }
                }
                Primitive::Fill(fill) => {
                    let rings: Vec<Vec<f64>> =
                        fill.rings.iter().map(|ring| clip_polygon(ring, b)).filter(|ring| ring.len() >= 6).collect();
                    if !rings.is_empty() {
                        out.push(Primitive::Fill(FillPrimitive { rings, ..fill }));
                    }
                }
                Primitive::Clipped(clipped) => {
                    let rings: Vec<Vec<f64>> =
                        clipped.rings.iter().map(|ring| clip_polygon(ring, b)).filter(|ring| ring.len() >= 6).collect();
                    if !rings.is_empty() {
                        out.push(Primitive::Clipped(ClippedPrimitive { rings, ..clipped }));
                    }
                }
                Primitive::Dot(dot) => {
                    if dot.x >= b.min_x && dot.x <= b.max_x && dot.y >= b.min_y && dot.y <= b.max_y {
                        out.push(Primitive::Dot(dot));
                    }
                }
                Primitive::Text(text) => {
                    let primitive = Primitive::Text(text);
                    let mut bbox = empty_box();
                    extend_primitive_box(&mut bbox, &primitive);
                    if !intersects(&bbox, b) {
                        continue;
                    }
                    if !inside(&bbox, b) {
                        partial_text = true;
                    }
                    out.push(primitive);
                }
            }
        }
        if partial_text {
            self.notices.add(
                NoticeLevel::Info,
                "viewport-text",
                "ビューポートの境界にかかる文字は切り取らずに描きました",
            );
        }
        out
    }

    // --- 図形 -----------------------------------------------------------------

    fn emit(&mut self, entity: &DxfEntity, ctx: &Ctx) {
        // 読み取りの上限（zumen_core::limits）を超えたら、残りの図形とブロックの中身を描かない
        if zumen_core::limits::tick() {
            return;
        }
        let base = &entity.base;
        if base.invisible {
            return;
        }
        if let Some(frozen) = &ctx.frozen
            && frozen.contains(&effective_layer(base, ctx).to_uppercase())
        {
            return;
        }
        if ctx.depth > 0 {
            self.expanded += 1;
            if self.expanded > MAX_BLOCK_ENTITIES {
                self.notices.add(
                    NoticeLevel::Warning,
                    "block-limit",
                    "ブロックの中の図形が 100 万個を超えたため、残りを描いていません",
                );
                return;
            }
        }
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                let (x1, y1) = apply(&ctx.local, start[0], start[1]);
                let (x2, y2) = apply(&ctx.local, end[0], end[1]);
                let stroke = self.stroke(base, ctx, &ctx.m, &[start[0], start[1], end[0], end[1]], false);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Stroke(stroke)],
                    vec![
                        point_property("start", "始点", x1, y1),
                        point_property("end", "終点", x2, y2),
                        number_property("length", "長さ", js::hypot2(x2 - x1, y2 - y1)),
                    ],
                    None,
                );
            }
            DxfEntityData::Circle { center, radius } => self.circle(base, ctx, center, *radius, None),
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                self.circle(base, ctx, center, *radius, Some((*start_angle, *end_angle)))
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                let minor = minor_axis(major_axis, *ratio, base.extrusion.as_ref());
                let tau = PI * 2.0;
                let full =
                    (end_param - start_param).abs() < 1e-9 || ((end_param - start_param).abs() - tau).abs() < 1e-9;
                let radius = js::hypot(major_axis);
                let mut points = ellipse_points(
                    center,
                    major_axis,
                    &minor,
                    if full { 0.0 } else { *start_param },
                    if full { tau } else { *end_param },
                    tolerance(&ctx.m, radius),
                );
                if full {
                    points.truncate(points.len().saturating_sub(2));
                }
                let (wx, wy) = apply(&ctx.local, center[0], center[1]);
                let stroke = self.stroke(base, ctx, &ctx.m, &points, full);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Stroke(stroke)],
                    vec![
                        point_property("center", "中心", wx, wy),
                        number_property("majorRadius", "長軸の半径", radius * mean_scale(&ctx.local)),
                        number_property("ratio", "短軸 / 長軸", *ratio),
                    ],
                    Some(if full { ("楕円", EntityKind::Ellipse) } else { ("楕円弧", EntityKind::EllipseArc) }),
                );
            }
            DxfEntityData::Point { location, .. } => {
                let (x, y) = apply(&ctx.m, location[0], location[1]);
                let (wx, wy) = apply(&ctx.local, location[0], location[1]);
                let color = self.color(base, ctx);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Dot(DotPrimitive { x, y, color })],
                    vec![point_property("position", "位置", wx, wy)],
                    None,
                );
            }
            DxfEntityData::LwPolyline(polyline) => {
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), polyline.elevation.unwrap_or(0.0)));
                self.polyline(base, &polyline.vertices, polyline.const_width.unwrap_or(0.0), ctx, &m, None);
            }
            DxfEntityData::Polyline(polyline) => self.polyline_entity(base, polyline, ctx),
            DxfEntityData::Spline(spline) => {
                let points = if spline.control_points.len() >= 2 {
                    let steps =
                        js::min(4096.0, js::max(32.0, (spline.control_points.len() as f64 - spline.degree) * 16.0));
                    nurbs_points(spline.degree, &slices(&spline.control_points), &spline.knots, &spline.weights, steps)
                } else {
                    let points = through_points(&slices(&spline.fit_points), 16, None, None);
                    if spline.fit_points.len() >= 3 {
                        self.notices.add(
                            NoticeLevel::Info,
                            "spline-fit",
                            "当てはめ点だけのスプラインは、点を通る曲線で近似しました",
                        );
                    }
                    points
                };
                let stroke = self.stroke(base, ctx, &ctx.m, &points, false);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Stroke(stroke)],
                    vec![
                        Property::new("degree", "次数", js::number_to_string(spline.degree)).raw(spline.degree),
                        count_property(
                            "controlPointCount",
                            "制御点",
                            spline.control_points.len(),
                            format!("{} 点", spline.control_points.len()),
                        ),
                    ],
                    None,
                );
            }
            DxfEntityData::Text(text) => self.text(base, text, None, 0.0, None, ctx),
            DxfEntityData::Attrib { text, tag, attribute_flags, mtext } => {
                self.text(base, text, Some(tag), *attribute_flags, mtext.as_deref(), ctx)
            }
            DxfEntityData::Attdef { text, tag, attribute_flags, mtext, .. } => {
                self.text(base, text, Some(tag), *attribute_flags, mtext.as_deref(), ctx)
            }
            DxfEntityData::MText(mtext) => {
                self.mtext(base, mtext, None, ctx);
            }
            DxfEntityData::Insert(_) | DxfEntityData::Table(_) => self.insert(entity, ctx, None),
            DxfEntityData::Dimension(dimension) => self.dimension(base, dimension, ctx),
            DxfEntityData::Hatch(hatch) => self.hatch(base, hatch, ctx),
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges } => {
                if base.kind == "3DFACE" {
                    let v = [vtx0, vtx1, vtx2, vtx3];
                    let mut segments: Vec<f64> = Vec::new();
                    let hidden = js::to_int32(invisible_edges.unwrap_or(0.0));
                    for i in 0..4 {
                        if hidden & (1 << i) != 0 {
                            continue;
                        }
                        let a = v[i];
                        let b = v[(i + 1) % 4];
                        if a[0] == b[0] && a[1] == b[1] {
                            continue;
                        }
                        segments.extend([a[0], a[1], b[0], b[1]]);
                    }
                    if segments.is_empty() {
                        return;
                    }
                    let mut stroke = self.stroke(base, ctx, &ctx.m, &segments, false);
                    stroke.segments = Some(true);
                    self.add(base, ctx, vec![Primitive::Stroke(stroke)], Vec::new(), None);
                    return;
                }
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), vtx0[2]));
                let same = vtx2[0] == vtx3[0] && vtx2[1] == vtx3[1];
                let corners: Vec<&Vec3> = if same { vec![vtx0, vtx1, vtx2] } else { vec![vtx0, vtx1, vtx3, vtx2] };
                let ring = transform_points(&m, &corners.iter().flat_map(|p| [p[0], p[1]]).collect::<Vec<f64>>());
                let color = self.color(base, ctx);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Fill(FillPrimitive { color, rings: vec![ring] })],
                    Vec::new(),
                    None,
                );
            }
            DxfEntityData::Leader(leader) => self.leader(base, leader, ctx),
            DxfEntityData::MLeader(mleader) => self.mleader(base, mleader, ctx),
            DxfEntityData::MLine(mline) => self.mline(base, mline, ctx),
            DxfEntityData::XLine { .. } => self.infinite.push((entity.clone(), ctx.clone())),
            DxfEntityData::Image(image) => self.image(base, image, ctx),
            DxfEntityData::Viewport(_) => {}
        }
    }

    /// CIRCLE（arc が None）と ARC
    fn circle(&mut self, base: &DxfEntityBase, ctx: &Ctx, center: &Vec3, radius: f64, arc: Option<(f64, f64)>) {
        let ocs = ocs_matrix(base.extrusion.as_ref(), center[2]);
        let m = multiply(&ctx.m, &ocs);
        let (cx, cy) = (center[0], center[1]);
        let full = arc.is_none();
        let (start, end) = arc.unwrap_or((0.0, 360.0));
        let mut points = arc_points(cx, cy, radius, start, end, tolerance(&m, radius));
        if full {
            points.truncate(points.len().saturating_sub(2));
        }
        let (wx, wy) = apply(&multiply(&ctx.local, &ocs), cx, cy);
        let mut properties = vec![
            point_property("center", "中心", wx, wy),
            number_property("radius", "半径", radius * mean_scale(&ctx.local)),
        ];
        if let Some((start, end)) = arc {
            properties
                .push(Property::new("angle", "角度", format!("{}° → {}°", fmt(start), fmt(end))).raw([start, end]));
        }
        let stroke = self.stroke(base, ctx, &m, &points, full);
        self.add(base, ctx, vec![Primitive::Stroke(stroke)], properties, None);
    }

    /// LWPOLYLINE と 2D の POLYLINE。幅のある線分は塗りの帯で描く
    fn polyline(
        &mut self,
        base: &DxfEntityBase,
        vertices: &DxfVertexList,
        constant: f64,
        ctx: &Ctx,
        m: &Matrix,
        keep: Option<&[usize]>,
    ) {
        let all: Vec<usize>;
        let indices = match keep {
            Some(keep) => keep,
            None => {
                all = (0..vertices.points.len()).collect();
                &all
            }
        };
        let points: Vec<Vec3> = indices.iter().map(|&index| vertices.points[index]).collect();
        let bulges: Vec<f64> =
            indices.iter().map(|&index| vertices.bulges.get(index).copied().unwrap_or(0.0)).collect();
        if points.is_empty() {
            return;
        }
        let closed = vertices.closed;
        let tolerance = |radius: f64| tolerance(m, radius);
        let line = bulge_points(&points, Some(&bulges), closed, &tolerance);
        let mut primitives: Vec<Primitive> = Vec::new();
        let widths: Vec<Vec2> =
            indices.iter().map(|&index| vertices.widths.get(index).copied().unwrap_or([0.0, 0.0])).collect();
        let wide = constant > 0.0 || widths.iter().any(|[start, end]| *start > 0.0 || *end > 0.0);
        if wide {
            let color = self.color(base, ctx);
            let mut rings: Vec<Vec<f64>> = Vec::new();
            let count = if closed { points.len() } else { points.len() - 1 };
            for i in 0..count {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                let segment = bulge_points(&[a, b], Some(&[bulges.get(i).copied().unwrap_or(0.0)]), false, &tolerance);
                let [w0, w1] = if constant > 0.0 { [constant, constant] } else { widths[i] };
                if let Some(ring) = band(&segment, w0, w1) {
                    rings.push(transform_points(m, &ring));
                }
            }
            if !rings.is_empty() {
                primitives.push(Primitive::Fill(FillPrimitive { color, rings }));
            }
        }
        let outline = if line.len() > 2 && closed { &line[..line.len() - 2] } else { &line[..] };
        let mut stroke = self.stroke(base, ctx, m, outline, closed);
        if wide {
            stroke.width = 0.0;
        }
        primitives.push(Primitive::Stroke(stroke));
        let mut properties = vec![count_property(
            "vertexCount",
            "頂点",
            points.len(),
            format!("{} 点{}", points.len(), if closed { "（閉じる）" } else { "" }),
        )];
        if wide {
            properties.push(if constant > 0.0 {
                number_property("width", "幅", constant)
            } else {
                Property::new("width", "幅", "頂点ごと")
            });
        }
        self.add(base, ctx, primitives, properties, None);
    }

    fn polyline_entity(&mut self, base: &DxfEntityBase, entity: &DxfPolyline, ctx: &Ctx) {
        let flags = js::to_int32(entity.vertices.flags);
        let points = &entity.vertices.points;
        // 頂点の番号（1 から）・格子の位置が、整数の範囲内の頂点を指していればその頂点
        let at = |index: f64| -> Option<&Vec3> {
            (index >= 0.0 && index.fract() == 0.0 && index < points.len() as f64).then(|| &points[index as usize])
        };
        if flags & 64 != 0 {
            // ポリフェースメッシュ: 面の見える辺（番号が正）
            let mut segments: Vec<f64> = Vec::new();
            for face in entity.faces.iter().flatten() {
                for i in 0..face.len() {
                    let a = face[i];
                    let b = face[(i + 1) % face.len()];
                    if a <= 0.0 {
                        continue;
                    }
                    if let (Some(p), Some(q)) = (at(a - 1.0), at(b.abs() - 1.0)) {
                        segments.extend([p[0], p[1], q[0], q[1]]);
                    }
                }
            }
            if !segments.is_empty() {
                let mut stroke = self.stroke(base, ctx, &ctx.m, &segments, false);
                stroke.segments = Some(true);
                let faces = entity.faces.as_ref().map_or(0, Vec::len);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Stroke(stroke)],
                    vec![count_property("faceCount", "面", faces, format!("{faces} 面"))],
                    Some(("ポリフェースメッシュ", EntityKind::Mesh)),
                );
            }
            return;
        }
        if flags & 16 != 0 {
            // ポリゴンメッシュ: M × N の頂点の格子
            let m_count = entity.m_count.unwrap_or(0.0);
            let n_count = entity.n_count.unwrap_or(0.0);
            let mut segments: Vec<f64> = Vec::new();
            let grid = |i: f64, j: f64| at(i * n_count + j);
            // 頂点の数を超える格子の位置には頂点がないので、そこで止める（71・72 が大きすぎる図面でも終わる）
            let rows = if n_count > 0.0 { js::min(m_count, (points.len() as f64 / n_count).ceil()) } else { 0.0 };
            let mut i = 0.0;
            while i < rows {
                let mut j = 0.0;
                while j < n_count && i * n_count + j < points.len() as f64 {
                    let p = grid(i, j);
                    let right = if j + 1.0 < n_count {
                        grid(i, j + 1.0)
                    } else if flags & 32 != 0 {
                        grid(i, 0.0)
                    } else {
                        None
                    };
                    let down = if i + 1.0 < m_count {
                        grid(i + 1.0, j)
                    } else if flags & 1 != 0 {
                        grid(0.0, j)
                    } else {
                        None
                    };
                    if let (Some(p), Some(right)) = (p, right) {
                        segments.extend([p[0], p[1], right[0], right[1]]);
                    }
                    if let (Some(p), Some(down)) = (p, down) {
                        segments.extend([p[0], p[1], down[0], down[1]]);
                    }
                    j += 1.0;
                }
                i += 1.0;
            }
            if !segments.is_empty() {
                let mut stroke = self.stroke(base, ctx, &ctx.m, &segments, false);
                stroke.segments = Some(true);
                self.add(
                    base,
                    ctx,
                    vec![Primitive::Stroke(stroke)],
                    vec![
                        Property::new(
                            "meshSize",
                            "頂点",
                            format!("{} × {}", js::number_to_string(m_count), js::number_to_string(n_count)),
                        )
                        .raw([m_count, n_count]),
                    ],
                    Some(("ポリゴンメッシュ", EntityKind::Mesh)),
                );
            }
            return;
        }
        // スプラインで当てはめたポリラインは、当てはめた頂点（8）を描き、制御点（16）を描かない
        let keep: Vec<usize> = (0..points.len())
            .filter(|&index| {
                let vertex_flags = js::to_int32(entity.vertex_flags.get(index).copied().unwrap_or(0.0));
                if flags & 4 != 0 { vertex_flags & 16 == 0 } else { true }
            })
            .collect();
        if flags & 8 != 0 {
            let line: Vec<f64> = keep.iter().flat_map(|&index| [points[index][0], points[index][1]]).collect();
            let closed = entity.vertices.closed;
            let stroke = self.stroke(base, ctx, &ctx.m, &line, closed);
            self.add(
                base,
                ctx,
                vec![Primitive::Stroke(stroke)],
                vec![count_property(
                    "vertexCount",
                    "頂点",
                    keep.len(),
                    format!("{} 点{}", keep.len(), if closed { "（閉じる）" } else { "" }),
                )],
                Some(("3D ポリライン", EntityKind::Polyline)),
            );
            return;
        }
        let elevation = points.first().map_or(0.0, |point| point[2]);
        let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), elevation));
        self.polyline(base, &entity.vertices, 0.0, ctx, &m, Some(&keep));
    }

    // --- 文字 -----------------------------------------------------------------

    fn style(&self, name: &str) -> Option<&'a DxfTextStyle> {
        self.styles.get(&name.to_uppercase()).or_else(|| self.styles.get("STANDARD")).copied()
    }

    /// フォントの CSS の名前と、SHX か、縦書きか。SHX は同梱しないので代わりのフォントで描き、数を知らせる。
    /// 縦書きは Windows の縦書きのフォント（名前の先頭が @ の TrueType）
    fn font_family(
        &mut self,
        style: Option<&DxfTextStyle>,
        override_font: Option<&RunFont>,
    ) -> (&'static str, bool, bool) {
        let file = match override_font {
            Some(font) => font.name.clone(),
            None => style.map(|style| style.font.clone()).unwrap_or_default(),
        };
        let family = style.and_then(|style| style.font_family.as_deref()).filter(|family| !family.is_empty());
        let truetype = override_font.is_some_and(|font| font.truetype)
            || ends_with_any(&file, &[".ttf", ".ttc", ".otf"])
            || (override_font.is_none() && family.is_some())
            || (!file.is_empty()
                && !ends_with_any(&file, &[".shx"])
                && file.chars().any(|c| !(' '..='~').contains(&c)));
        let name = if truetype {
            if override_font.is_some() { file.as_str() } else { family.unwrap_or(&file) }
        } else {
            &file
        };
        let name = strip_suffix_any(name, &[".ttf", ".ttc", ".otf", ".shx"]);
        if !truetype {
            let font = strip_suffix_any(if file.is_empty() { "txt" } else { &file }, &[".shx"]).to_lowercase();
            if !self.shx_fonts.contains(&font) {
                self.shx_fonts.push(font);
            }
        }
        (font_family_for(name), !truetype, truetype && name.starts_with('@'))
    }

    /// 1 行の run を、ベースラインの左端を原点とする文字の座標系に並べて描く。
    /// transform は文字の座標系から表示用の図面への変換。返すのは行の幅と SHX を使ったか
    fn draw_line(
        &mut self,
        runs: &[TextRun],
        transform: &Matrix,
        base_color: &str,
        style: Option<&DxfTextStyle>,
        out: &mut Vec<Primitive>,
    ) -> (f64, bool) {
        let mut x = 0.0;
        let mut shx = false;
        for run in runs {
            let color = match &run.color {
                None => base_color.to_string(),
                Some(RunColor::Aci(index)) => aci_color(*index),
                Some(RunColor::Rgb(color)) => color.clone(),
            };
            let (family, uses_shx, vertical) = self.font_family(style, run.font.as_ref());
            if uses_shx {
                shx = true;
            }
            if let Some(stack) = &run.stack {
                x += self.draw_stack(run, stack, x, transform, &color, family, style, out);
                continue;
            }
            let text = run.text.replace('\t', "    ");
            if text.is_empty() {
                continue;
            }
            let em = em_for(&text) * run.height;
            let width = units(&text) * em * run.width_factor;
            if let Some(pieces) = self.pieces.as_mut().filter(|_| !js::trim(&text).is_empty()) {
                pieces.push(TextPiece {
                    text: text.clone(),
                    matrix: multiply(transform, &translate(x, 0.0)),
                    height: run.height,
                    width_factor: run.width_factor,
                    oblique: run.oblique,
                    color: color.clone(),
                    font: run.font.clone(),
                    style: style.map(|style| style.name.clone()),
                });
            }
            if !js::trim(&text).is_empty() && vertical {
                // 縦書きのフォントは、字形をベースラインに対して 90° 左に回して並べる（ベースラインを 270° に向けると字が
                // 立つ）。文字範囲は、ベースラインの向きに長さ width、横に em（em の下端から DESCENT 上がベースライン）
                let primitive = layout_text(
                    TextLayoutInput {
                        text: &text,
                        x: 0.0,
                        y: 0.0,
                        height: width,
                        width: em,
                        spacing: 0.0,
                        angle: 0.0,
                        slant: run.oblique,
                        anchor: 1.0,
                        vertical: true,
                        color: color.clone(),
                        font_family: family.to_string(),
                    },
                    &multiply(transform, &multiply(&translate(x, 0.0), &[0.0, 1.0, -1.0, 0.0, width, -DESCENT * em])),
                );
                if let Some(mut primitive) = primitive {
                    // DXF Reference「TEXT」40: 規定の文字高を、描画用の字形の em と別に保つ。
                    primitive.character_height = Some(run.height);
                    out.push(Primitive::Text(primitive));
                }
            } else if !js::trim(&text).is_empty() {
                let primitive = layout_text(
                    TextLayoutInput {
                        text: &text,
                        x: 0.0,
                        y: 0.0,
                        height: em,
                        width,
                        spacing: 0.0,
                        angle: 0.0,
                        slant: run.oblique,
                        anchor: 1.0,
                        vertical: false,
                        color: color.clone(),
                        font_family: family.to_string(),
                    },
                    &multiply(transform, &translate(x, -DESCENT * em)),
                );
                if let Some(mut primitive) = primitive {
                    primitive.character_height = Some(run.height);
                    out.push(Primitive::Text(primitive));
                }
            }
            for (on, y) in
                [(run.underline, -0.2 * run.height), (run.overline, 1.2 * run.height), (run.strike, 0.45 * run.height)]
            {
                if !on {
                    continue;
                }
                out.push(Primitive::Stroke(stroke_primitive(
                    transform_points(transform, &[x, y, x + width, y]),
                    false,
                    color.clone(),
                )));
            }
            x += width;
        }
        (x, shx)
    }

    /// 積み重ね（\S）。上と下を 70% の高さで重ね、/ は横線を引く
    #[allow(clippy::too_many_arguments)]
    fn draw_stack(
        &mut self,
        run: &TextRun,
        stack: &RunStack,
        x: f64,
        transform: &Matrix,
        color: &str,
        family: &str,
        style: Option<&DxfTextStyle>,
        out: &mut Vec<Primitive>,
    ) -> f64 {
        let h = run.height * 0.7;
        let widths = [&stack.upper, &stack.lower].map(|part| units(part) * em_for(part) * h * run.width_factor);
        let width = if stack.kind == "#" { widths[0] + widths[1] } else { js::max(widths[0], widths[1]) };
        let pieces = &mut self.pieces;
        let mut place = |text: &str, px: f64, baseline: f64, w: f64| {
            if text.is_empty() {
                return;
            }
            if let Some(pieces) = pieces.as_mut() {
                pieces.push(TextPiece {
                    text: text.to_string(),
                    matrix: multiply(transform, &translate(px, baseline)),
                    height: h,
                    width_factor: run.width_factor,
                    oblique: run.oblique,
                    color: color.to_string(),
                    font: run.font.clone(),
                    style: style.map(|style| style.name.clone()),
                });
            }
            let em = em_for(text) * h;
            let primitive = layout_text(
                TextLayoutInput {
                    text,
                    x: 0.0,
                    y: 0.0,
                    height: em,
                    width: w,
                    spacing: 0.0,
                    angle: 0.0,
                    slant: run.oblique,
                    anchor: 1.0,
                    vertical: false,
                    color: color.to_string(),
                    font_family: family.to_string(),
                },
                &multiply(transform, &translate(px, baseline - DESCENT * em)),
            );
            if let Some(mut primitive) = primitive {
                primitive.character_height = Some(h);
                out.push(Primitive::Text(primitive));
            }
        };
        if stack.kind == "#" {
            place(&stack.upper, x, run.height * 0.45, widths[0]);
            place(&stack.lower, x + widths[0], -run.height * 0.1, widths[1]);
        } else {
            place(&stack.upper, x + (width - widths[0]) / 2.0, run.height * 0.55, widths[0]);
            place(&stack.lower, x + (width - widths[1]) / 2.0, -run.height * 0.35, widths[1]);
            if stack.kind == "/" {
                out.push(Primitive::Stroke(stroke_primitive(
                    transform_points(transform, &[x, run.height * 0.42, x + width, run.height * 0.42]),
                    false,
                    color.to_string(),
                )));
            }
        }
        self.notices.add(NoticeLevel::Info, "mtext-stack", "積み重ねた文字（分数など）は小さく上下に並べました");
        width
    }

    fn count_shx(&mut self, shx: bool) {
        if shx {
            self.shx_count += 1;
        }
    }

    /// TEXT・ATTRIB・ATTDEF（DXF Reference「TEXT」の 72・73 の表）
    fn text(
        &mut self,
        base: &DxfEntityBase,
        entity: &DxfTextBase,
        tag: Option<&str>,
        attribute_flags: f64,
        mtext: Option<&DxfEmbeddedMText>,
        ctx: &Ctx,
    ) {
        let kind = base.kind.as_str();
        let flags = js::to_int32(attribute_flags);
        if kind == "ATTRIB" && flags & 1 != 0 {
            return;
        }
        if kind == "ATTDEF" && flags & 1 != 0 && ctx.depth == 0 {
            return;
        }
        if let Some(content) = mtext.filter(|_| kind != "TEXT") {
            self.multiline_attribute(base, content, tag.unwrap_or(""), ctx);
            return;
        }
        let content = if kind == "ATTDEF" && ctx.depth == 0 { tag.unwrap_or("") } else { entity.text.as_str() };
        if content.is_empty() {
            return;
        }
        let style = self.style(&entity.style);
        let height = if entity.height > 0.0 {
            entity.height
        } else if let Some(style) = style.filter(|style| style.height > 0.0) {
            style.height
        } else {
            self.doc.header.textsize.unwrap_or(2.5)
        };
        let runs = parse_text_codes(content, height, or(entity.width, 1.0), entity.oblique);
        let width = runs
            .iter()
            .fold(0.0, |sum, run| sum + units(&run.text) * em_for(&run.text) * run.height * run.width_factor);
        let ocs = ocs_matrix(base.extrusion.as_ref(), entity.insert[2]);
        let align = entity.align_point.unwrap_or(entity.insert);
        let h = entity.halign;
        let v = entity.valign;
        let origin = if (h == 3.0 || h == 5.0) && entity.align_point.is_some() && width > 0.0 {
            // 両端揃え（3）は高さも、フィット（5）は幅だけを、2 点の間に合わせる
            let dx = align[0] - entity.insert[0];
            let dy = align[1] - entity.insert[1];
            let length = js::hypot2(dx, dy);
            let factor = if length > 0.0 { length / width } else { 1.0 };
            let angle = (js::atan2(dy, dx) * 180.0) / PI;
            multiply(
                &multiply(
                    &translate(entity.insert[0], entity.insert[1]),
                    &rotate(if length > 0.0 { angle } else { entity.rotation }),
                ),
                &scale(factor, if h == 3.0 { factor } else { 1.0 }),
            )
        } else {
            let point = if h == 0.0 && v == 0.0 { entity.insert } else { align };
            let ax = if h == 1.0 || h == 4.0 {
                width / 2.0
            } else if h == 2.0 {
                width
            } else {
                0.0
            };
            let ay = if h == 4.0 {
                height / 2.0
            } else if v == 1.0 {
                -DESCENT * EM_LATIN * height
            } else if v == 2.0 {
                height / 2.0
            } else if v == 3.0 {
                height
            } else {
                0.0
            };
            let generation = js::to_int32(entity.text_generation_flag);
            let mirror =
                scale(if generation & 2 != 0 { -1.0 } else { 1.0 }, if generation & 4 != 0 { -1.0 } else { 1.0 });
            multiply(
                &multiply(&multiply(&translate(point[0], point[1]), &rotate(entity.rotation)), &mirror),
                &translate(-ax, -ay),
            )
        };
        let transform = multiply(&multiply(&ctx.m, &ocs), &origin);
        let mut primitives = Vec::new();
        let color = self.color(base, ctx);
        let (_, shx) = self.draw_line(&runs, &transform, &color, style, &mut primitives);
        if primitives.is_empty() {
            return;
        }
        self.count_shx(shx);
        let plain: String = runs.iter().map(|run| run.text.as_str()).collect();
        let mut properties = vec![Property::new("text", "文字列", plain)];
        if kind != "TEXT" {
            properties.push(Property::new("tag", "タグ", tag.unwrap_or("")));
        }
        properties.push(number_property("height", "高さ", height * mean_scale(&ctx.local)));
        properties.push(Property::new("textStyle", "文字スタイル", style_label(style, &entity.style)));
        self.add(base, ctx, primitives, properties, None);
    }

    /// 複数行の属性（AutoCAD 2018 以降）は、埋め込みの MTEXT を、属性の色・レイヤ・線種と MTEXT の押し出し方向で描く
    /// （ezdxf の virtual_mtext_entity と同じ）。ブロックの外の属性定義は、1 行の属性定義と同じくタグを描く
    /// （書式コードとしては読まない）
    fn multiline_attribute(&mut self, base: &DxfEntityBase, embedded: &DxfEmbeddedMText, tag: &str, ctx: &Ctx) {
        let base = DxfEntityBase { extrusion: embedded.extrusion, ..base.clone() };
        if base.kind == "ATTDEF" && ctx.depth == 0 {
            let mut raw = String::with_capacity(tag.len());
            for c in tag.chars() {
                if matches!(c, '\\' | '{' | '}') {
                    raw.push('\\');
                }
                raw.push(c);
            }
            let shown = DxfMText { text: tag.to_string(), raw_text: raw, ..embedded.content.clone() };
            self.mtext(&base, &shown, Some(tag), ctx);
        } else {
            self.mtext(&base, &embedded.content, Some(tag), ctx);
        }
    }

    /// MTEXT（DXF Reference「MTEXT」）。折り返しの幅があれば、語・全角の文字の境目で折り返す。
    /// 描いた文字の範囲を、文字の座標系（配置点が原点、文字の進む向きが X）で返す。tag は複数行の属性のタグ
    fn mtext(&mut self, base: &DxfEntityBase, entity: &DxfMText, tag: Option<&str>, ctx: &Ctx) -> Option<MTextLayout> {
        struct Line {
            runs: Vec<TextRun>,
            width: f64,
            height: f64,
            align: Option<ParagraphAlign>,
        }
        let style = self.style(&entity.style);
        let base_height = if entity.char_height > 0.0 {
            entity.char_height
        } else if let Some(style) = style.filter(|style| style.height > 0.0) {
            style.height
        } else {
            self.doc.header.textsize.unwrap_or(2.5)
        };
        let paragraphs = parse_mtext(
            &entity.raw_text,
            base_height,
            style.map_or(1.0, |style| or(style.width, 1.0)),
            style.map_or(0.0, |style| style.oblique),
        );
        let wrap = if entity.rect_width > 0.0 { entity.rect_width } else { 0.0 };
        let run_width = |run: &TextRun| match &run.stack {
            Some(stack) => {
                js::max(units(&stack.upper), units(&stack.lower)) * EM_LATIN * run.height * 0.7 * run.width_factor
            }
            None => units(&run.text) * em_for(&run.text) * run.height * run.width_factor,
        };
        let width_of = |run: &TextRun, text: &str| units(text) * em_for(text) * run.height * run.width_factor;
        let mut lines: Vec<Line> = Vec::new();
        for paragraph in &paragraphs {
            let new_line = || Line { runs: Vec::new(), width: 0.0, height: 0.0, align: paragraph.align };
            let mut line = new_line();
            let push = |line: &mut Line, run: TextRun| {
                line.width += run_width(&run);
                line.height = js::max(line.height, run.height);
                line.runs.push(run);
            };
            let finish = |line: &mut Line, lines: &mut Vec<Line>| {
                if line.height == 0.0 {
                    line.height = paragraph.runs.first().map_or(base_height, |run| run.height);
                }
                lines.push(std::mem::replace(line, new_line()));
            };
            for run in &paragraph.runs {
                if wrap == 0.0 || run.stack.is_some() {
                    if wrap != 0.0 && !line.runs.is_empty() && line.width + run_width(run) > wrap {
                        finish(&mut line, &mut lines);
                    }
                    push(&mut line, run.clone());
                    continue;
                }
                // 1 文字ずつ足し、幅を超えたら直前の空白か全角の文字の境目で折り返す
                let mut piece = String::new();
                for c in run.text.chars() {
                    let candidate = format!("{piece}{c}");
                    let width = width_of(run, &candidate);
                    if line.width + width <= wrap || (line.runs.is_empty() && piece.is_empty()) {
                        piece = candidate;
                        continue;
                    }
                    // 半角の語の途中では折り返さない。直前の空白か、全角の文字の直後で折り返す
                    let mut head = piece.clone();
                    let mut tail = c.to_string();
                    if is_half_width(c) && c != ' ' {
                        let chars: Vec<char> = piece.chars().collect();
                        let mut at = chars.len();
                        while at > 0 && chars[at - 1] != ' ' && is_half_width(chars[at - 1]) {
                            at -= 1;
                        }
                        if at > 0 {
                            head = chars[..at].iter().collect::<String>().trim_end_matches(' ').to_string();
                            tail = chars[at..].iter().chain([c].iter()).collect();
                        }
                    }
                    if !head.is_empty() {
                        push(&mut line, TextRun { text: head, ..run.clone() });
                    }
                    finish(&mut line, &mut lines);
                    piece = tail.trim_start_matches(' ').to_string();
                }
                if !piece.is_empty() {
                    push(&mut line, TextRun { text: piece, ..run.clone() });
                }
            }
            finish(&mut line, &mut lines);
        }
        while lines.len() > 1 && lines.last().is_some_and(|line| line.runs.is_empty()) {
            lines.pop();
        }
        if !lines.iter().any(|line| line.runs.iter().any(|run| run.stack.is_some() || !js::trim(&run.text).is_empty()))
        {
            return None;
        }
        let factor = if entity.line_spacing_factor > 0.0 { entity.line_spacing_factor } else { 1.0 };
        let mut baselines: Vec<f64> = Vec::new();
        let mut y = -lines[0].height;
        for (index, line) in lines.iter().enumerate() {
            if index > 0 {
                y -= line.height * LINE_SPACING * factor;
            }
            baselines.push(y);
        }
        let top = 0.0;
        let bottom = baselines[baselines.len() - 1] - DESCENT * EM_LATIN * lines[lines.len() - 1].height;
        let attachment = if entity.attachment_point >= 1.0 && entity.attachment_point <= 9.0 {
            entity.attachment_point
        } else {
            1.0
        };
        let column = (attachment - 1.0) % 3.0;
        let row = ((attachment - 1.0) / 3.0).floor();
        let block_width =
            if wrap != 0.0 { wrap } else { max_of(&lines.iter().map(|line| line.width).collect::<Vec<f64>>()) };
        let x0 = if column == 0.0 {
            0.0
        } else if column == 1.0 {
            -block_width / 2.0
        } else {
            -block_width
        };
        let dy = if row == 0.0 {
            0.0
        } else if row == 1.0 {
            (top - bottom) / 2.0
        } else {
            top - bottom
        };
        // 文字の向き: 11（WCS のベクトル）があればそれを、なければ OCS の回転角
        let frame: Matrix = match entity.text_direction.filter(|d| js::hypot2(d[0], d[1]) > 1e-12) {
            Some([dx0, dy0, _]) => {
                let length = js::hypot2(dx0, dy0);
                let ux = dx0 / length;
                let uy = dy0 / length;
                let flip = if base.extrusion.map_or(1.0, |extrusion| extrusion[2]) < 0.0 { -1.0 } else { 1.0 };
                [ux, uy, -uy * flip, ux * flip, entity.insert[0], entity.insert[1]]
            }
            None => {
                let ocs = ocs_matrix(base.extrusion.as_ref(), 0.0);
                multiply(
                    &translate(entity.insert[0], entity.insert[1]),
                    &multiply(&[ocs[0], ocs[1], ocs[2], ocs[3], 0.0, 0.0], &rotate(entity.rotation)),
                )
            }
        };
        if entity.drawing_direction == 3.0 {
            self.notices.add(NoticeLevel::Info, "mtext-vertical", "縦書きの複数行文字は横書きで並べました");
        }
        let transform = multiply(&ctx.m, &frame);
        let color = self.color(base, ctx);
        let mut primitives: Vec<Primitive> = Vec::new();
        let background = js::to_int32(entity.background_flags);
        if background & 3 != 0 {
            let margin = (entity.background_scale.unwrap_or(1.5) - 1.0) * base_height;
            let fill = if background & 2 != 0 {
                BACKGROUND_COLOR.to_string()
            } else if let Some(value) = entity.background_true_color {
                true_color(value)
            } else if let Some(index) = entity.background_color_index {
                aci_color(index)
            } else {
                BACKGROUND_COLOR.to_string()
            };
            let x1 = x0 - margin;
            let x2 = x0 + block_width + margin;
            let y1 = top + dy + margin;
            let y2 = bottom + dy - margin;
            primitives.push(Primitive::Fill(FillPrimitive {
                color: fill,
                rings: vec![transform_points(&transform, &[x1, y1, x2, y1, x2, y2, x1, y2])],
            }));
        }
        let mut shx = false;
        for (index, line) in lines.iter().enumerate() {
            let align = line.align.unwrap_or(if column == 0.0 {
                ParagraphAlign::Left
            } else if column == 1.0 {
                ParagraphAlign::Center
            } else {
                ParagraphAlign::Right
            });
            let offset = match align {
                ParagraphAlign::Center => (block_width - line.width) / 2.0,
                ParagraphAlign::Right => block_width - line.width,
                ParagraphAlign::Left => 0.0,
            };
            let line_transform = multiply(&transform, &translate(x0 + offset, baselines[index] + dy));
            let (_, uses_shx) = self.draw_line(&line.runs, &line_transform, &color, style, &mut primitives);
            if uses_shx {
                shx = true;
            }
        }
        self.count_shx(shx);
        let mut properties = vec![Property::new("text", "文字列", entity.text.clone())];
        if let Some(tag) = tag {
            properties.push(Property::new("tag", "タグ", tag));
        }
        properties.push(number_property("height", "高さ", base_height * mean_scale(&ctx.local)));
        properties.push(Property::new("textStyle", "文字スタイル", style_label(style, &entity.style)));
        if wrap != 0.0 {
            properties.push(number_property("wrapWidth", "折り返しの幅", wrap * mean_scale(&ctx.local)));
        }
        self.add(base, ctx, primitives, properties, None);
        Some(MTextLayout {
            frame,
            bbox: BBox { min_x: x0, min_y: bottom + dy, max_x: x0 + block_width, max_y: top + dy },
        })
    }

    // --- ブロック・寸法 -------------------------------------------------------

    /// values は ATTDEF のハンドル → 値。MULTILEADER のブロックの属性を、ATTDEF の位置に描く
    fn insert(&mut self, entity: &DxfEntity, ctx: &Ctx, values: Option<&HashMap<String, String>>) {
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
        let Some(contents) = self.blocks.get(&name.to_uppercase()) else {
            self.notices.add(NoticeLevel::Warning, "missing-block", "定義の見つからないブロック参照を省きました");
            return;
        };
        let (block, members) = (contents.block, contents.entities.clone());
        if ctx.depth >= MAX_DEPTH {
            self.notices.add(NoticeLevel::Warning, "block-depth", "入れ子が深すぎるブロック参照を省きました");
            return;
        }
        let base_point = block.map_or([0.0, 0.0, 0.0], |block| block.base_point);
        let ocs = ocs_matrix(base.extrusion.as_ref(), point[2]);
        let columns = insert.map_or(1.0, |insert| insert.column_count);
        let rows = insert.map_or(1.0, |insert| insert.row_count);
        let many = columns * rows > 1.0;
        if many && columns * rows > 10_000.0 {
            self.notices.add(
                NoticeLevel::Warning,
                "minsert-large",
                "配列が多すぎるブロック参照は、先頭の 1 つだけを描きました",
            );
        }
        let limit = |count: f64| if columns * rows > 10_000.0 { 1.0 } else { count };
        let mut r = 0.0;
        while r < limit(rows) {
            let mut c = 0.0;
            while c < limit(columns) {
                let offset = match insert {
                    Some(insert) => translate(c * insert.column_spacing, r * insert.row_spacing),
                    None => translate(0.0, 0.0),
                };
                let local = multiply(
                    &multiply(&multiply(&multiply(&ocs, &translate(point[0], point[1])), &rotate(rotation)), &offset),
                    &multiply(&scale(xscale, yscale), &translate(-base_point[0], -base_point[1])),
                );
                let suffix = if many {
                    format!("[{},{}]", js::number_to_string(r), js::number_to_string(c))
                } else {
                    String::new()
                };
                let path = format!("{}{}{suffix}>", ctx.path, handle_or_unknown(base));
                let child = self.child(base, ctx, &local, path, name);
                for member in members.iter() {
                    // 属性定義は、定数（2）のものだけを描く。値は ATTRIB が持つ
                    if let DxfEntityData::Attdef { text, tag, attribute_flags, mtext, .. } = &member.data
                        && member.base.kind == "ATTDEF"
                        && js::to_int32(*attribute_flags) & 2 == 0
                    {
                        if let Some(value) = values.and_then(|values| values.get(&member.base.handle)) {
                            let attribute = DxfEntity {
                                base: DxfEntityBase { kind: "ATTRIB".into(), ..member.base.clone() },
                                data: DxfEntityData::Attrib {
                                    text: DxfTextBase { text: value.clone(), ..text.clone() },
                                    tag: tag.clone(),
                                    attribute_flags: *attribute_flags,
                                    // 複数行の属性定義は、値を埋め込みの MTEXT の文字にする
                                    mtext: mtext.as_ref().map(|mtext| {
                                        Box::new(DxfEmbeddedMText {
                                            content: DxfMText {
                                                text: mtext_plain_text(value),
                                                raw_text: value.clone(),
                                                ..mtext.content.clone()
                                            },
                                            extrusion: mtext.extrusion,
                                        })
                                    }),
                                },
                            };
                            self.emit(&attribute, &child);
                        }
                        continue;
                    }
                    self.emit(member, &child);
                }
                c += 1.0;
            }
            r += 1.0;
        }
        if let Some(insert) = insert
            && !insert.attribs.is_empty()
        {
            let path = format!("{}{}>", ctx.path, handle_or_unknown(base));
            let attribute_ctx = self.child(base, ctx, &IDENTITY, path, name);
            for attribute in &insert.attribs {
                self.emit(attribute, &attribute_ctx);
            }
        }
    }

    /// 寸法は表示のブロック（*D…）の図形を、寸法の 1 つの要素にまとめる（DXF Reference「Common Dimension Group Codes」の 2）
    fn dimension(&mut self, base: &DxfEntityBase, entity: &DxfDimension, ctx: &Ctx) {
        let contents = if entity.anonymous_block_name.is_empty() {
            None
        } else {
            self.blocks.get(&entity.anonymous_block_name.to_uppercase())
        };
        let Some(contents) = contents else {
            self.notices.add(NoticeLevel::Warning, "dimension-block", "表示のブロックがない寸法を省きました");
            return;
        };
        let members = contents.entities.clone();
        // 表示のブロックが自分を指す寸法を含んでも、ブロック参照と同じ深さで止める
        if ctx.depth >= MAX_DEPTH {
            self.notices.add(NoticeLevel::Warning, "block-depth", "入れ子が深すぎるブロック参照を省きました");
            return;
        }
        // ブロックの中の図形は寸法の OCS にあり、12 の位置へ動かす（ezdxf の Dimension.__virtual_entities__ と同じ）
        let ocs = ocs_matrix(base.extrusion.as_ref(), 0.0);
        let insert = entity.insert.unwrap_or([0.0, 0.0, 0.0]);
        let (ix, iy) = apply(&ocs, insert[0], insert[1]);
        let local = multiply(&translate(ix, iy), &ocs);
        let path = format!("{}{}>", ctx.path, handle_or_unknown(base));
        let child = self.child(base, ctx, &local, path, &entity.anonymous_block_name);
        let start = self.entities.len();
        for member in members.iter() {
            self.emit(member, &child);
        }
        let primitives = self.take_parts(start);
        let kind = if base.kind == "DIMENSION" {
            DIMENSION_KINDS.get((js::to_int32(entity.dimtype) & 7) as usize).copied()
        } else {
            type_name(&base.kind)
        }
        .unwrap_or("寸法");
        let dimension_type = match base.kind.as_str() {
            "DIMENSION" => DIMENSION_TYPES.get((js::to_int32(entity.dimtype) & 7) as usize).copied(),
            "ARC_DIMENSION" => Some("arc-length"),
            _ => Some("jogged-radius"),
        };
        let value = if !entity.text.is_empty() && entity.text != "<>" && !entity.text.contains("<>") {
            entity.text.clone()
        } else if let Some(measurement) = entity.actual_measurement {
            fmt(measurement)
        } else {
            String::new()
        };
        self.add(
            base,
            ctx,
            primitives,
            vec![
                match dimension_type {
                    Some(raw) => Property::new("dimensionType", "寸法の種類", kind).raw(raw),
                    None => Property::new("dimensionType", "寸法の種類", kind),
                },
                match entity.actual_measurement {
                    Some(measurement) => Property::new("dimensionText", "寸法値", value).raw(measurement),
                    None => Property::new("dimensionText", "寸法値", value),
                },
                Property::new("dimensionStyle", "寸法スタイル", entity.dimstyle.clone()),
            ],
            Some((kind, EntityKind::Dimension)),
        );
    }

    /// start 以降に加えた要素を取り除き、その描画の要素を返す（1 つの要素にまとめるため）。add が数えたレイヤの使用も戻す
    fn take_parts(&mut self, start: usize) -> Vec<Primitive> {
        let parts = self.entities.split_off(start);
        zumen_core::limits::release(parts.len() as u64);
        for part in &parts {
            for id in &part.layers {
                let count = self.used_layers.get(id).copied().unwrap_or(0);
                if count > 1 {
                    self.used_layers.insert(*id, count - 1);
                } else {
                    self.used_layers.remove(id);
                }
            }
        }
        parts.into_iter().flat_map(|part| part.primitives).collect()
    }

    fn leader(&mut self, base: &DxfEntityBase, entity: &DxfLeader, ctx: &Ctx) {
        if entity.points.len() < 2 {
            return;
        }
        let points = if entity.path_type == 1.0 && entity.points.len() > 2 {
            through_points(&slices(&entity.points), 12, None, None)
        } else {
            flat(&entity.points)
        };
        let mut primitives = vec![Primitive::Stroke(self.stroke(base, ctx, &ctx.m, &points, false))];
        if entity.has_arrowhead {
            let style = self.dimstyles.get(&entity.dimstyle.to_uppercase()).copied();
            let header = &self.doc.header;
            let dimasz = style.map(|style| style.dimasz).filter(|value| truthy(*value)).unwrap_or(header.dimasz);
            let dimscale = style.map(|style| style.dimscale).filter(|value| truthy(*value)).unwrap_or(header.dimscale);
            let size = dimasz * or(dimscale, 1.0);
            let (tx, ty) = (entity.points[0][0], entity.points[0][1]);
            let (nx, ny) = (entity.points[1][0], entity.points[1][1]);
            let length = js::hypot2(nx - tx, ny - ty);
            if length > 0.0 && size > 0.0 {
                let ux = (nx - tx) / length;
                let uy = (ny - ty) / length;
                let bx = tx + ux * size;
                let by = ty + uy * size;
                let w = size / 6.0;
                let color = self.color(base, ctx);
                primitives.insert(
                    0,
                    Primitive::Fill(FillPrimitive {
                        color,
                        rings: vec![transform_points(
                            &ctx.m,
                            &[tx, ty, bx - uy * w, by + ux * w, bx + uy * w, by - ux * w],
                        )],
                    }),
                );
            }
        }
        let count = entity.points.len();
        self.add(
            base,
            ctx,
            primitives,
            vec![count_property("vertexCount", "頂点", count, format!("{count} 点"))],
            None,
        );
    }

    // --- マルチ引出線・マルチライン ---------------------------------------------

    /// マルチ引出線（DXF Reference「MLEADER」）。内容（文字かブロック）、引出線（矢印・折れ線）、枠・下線を 1 つの要素にまとめる。
    /// 描き方は、AutoCAD が図面に書いた代替表示（proxy graphic）と ezdxf の render/mleader.py に合わせた（docs/formats/dxf.md）
    fn mleader(&mut self, base: &DxfEntityBase, entity: &DxfMLeader, ctx: &Ctx) {
        let context = &entity.context;
        // 全体の尺度が 0 の引出線は、縮んで見えない（ezdxf の RenderEngine.run と同じ）
        let overall = context.scale.abs();
        if overall.is_nan() || overall <= 1e-9 {
            return;
        }
        let path = format!("{}{}>", ctx.path, handle_or_unknown(base));
        let child = self.child(base, ctx, &IDENTITY, path, &base.kind);
        let start = self.entities.len();
        let mut properties: Vec<Property> = Vec::new();
        let mut text: Option<MTextLayout> = None;
        let mut text_part: Option<DxfEntityBase> = None;
        if let Some(mtext) = &context.mtext {
            let part = part_of(base, raw_color(mtext.color), "CONTINUOUS");
            text = self.mleader_text(mtext, context, &part, &child);
            text_part = Some(part);
            properties.push(Property::new("text", "文字列", mtext_plain_text(&mtext.default_content)));
            properties.push(number_property("height", "高さ", context.char_height * mean_scale(&ctx.local)));
        } else if let Some(block) = &context.block {
            let name = self.mleader_block(base, entity, block, &child);
            if !name.is_empty() {
                properties.push(Property::new("contentBlock", "内容のブロック", name));
            }
        } else if entity.content_type == 3.0 {
            self.notices.add(
                NoticeLevel::Info,
                "mleader-tolerance",
                "公差を内容とするマルチ引出線は、引出線だけを描きました",
            );
        }
        let mut lines: Vec<Primitive> = Vec::new();
        let mut connections: Vec<Vec2> = Vec::new();
        let count = self.mleader_leaders(base, entity, text.as_ref(), &child, &mut lines, &mut connections);
        if let (Some(text), Some(part)) = (&text, &text_part)
            && entity.has_text_frame
        {
            // 枠は文字の範囲を着地の隙間だけ広げた矩形で、引出線の接続点（折れ線の先）が辺に乗る（AutoCAD の代替表示と同じ）
            let gap = context.landing_gap_size;
            let inverse = invert(&text.frame);
            let mut min_x = text.bbox.min_x - gap;
            let mut min_y = text.bbox.min_y - gap;
            let mut max_x = text.bbox.max_x + gap;
            let mut max_y = text.bbox.max_y + gap;
            for [px, py] in &connections {
                let (x, y) = apply(&inverse, *px, *py);
                min_x = js::min(min_x, x);
                max_x = js::max(max_x, x);
                min_y = js::min(min_y, y);
                max_y = js::max(max_y, y);
            }
            let frame = [min_x, min_y, max_x, min_y, max_x, max_y, min_x, max_y];
            let stroke = self.stroke(part, &child, &multiply(&child.m, &text.frame), &frame, true);
            lines.push(Primitive::Stroke(stroke));
        }
        properties.push(Property::new("leaderCount", "引出線", format!("{count} 本")).raw(count as f64));
        let mut primitives = self.take_parts(start);
        primitives.extend(lines);
        self.add(base, ctx, primitives, properties, None);
    }

    /// 文字の内容を、CONTEXT_DATA の値（12 の位置、171 の配置点、13 の向き）の MTEXT として描く
    fn mleader_text(
        &mut self,
        mtext: &DxfMLeaderMText,
        context: &DxfMLeaderContext,
        part: &DxfEntityBase,
        child: &Ctx,
    ) -> Option<MTextLayout> {
        let mut content_base = DxfEntityBase { kind: "MTEXT".into(), ..part.clone() };
        let mut content = DxfMText {
            text: mtext_plain_text(&mtext.default_content),
            raw_text: mtext.default_content.clone(),
            insert: mtext.insert,
            char_height: context.char_height,
            // 折り返しの幅は全体の尺度を掛ける前の値（ezdxf の copy_mtext_data と同じ）
            rect_width: mtext.width * context.scale,
            attachment_point: mtext.alignment,
            drawing_direction: mtext.flow_direction,
            text_direction: Some(mtext.text_direction),
            rotation: 0.0,
            style: self
                .style_handles
                .get(&mtext.style_handle)
                .map_or_else(|| "STANDARD".into(), |style| style.name.clone()),
            line_spacing_style: mtext.line_spacing_style,
            line_spacing_factor: mtext.line_spacing_factor,
            background_flags: 0.0,
            background_color_index: None,
            background_true_color: None,
            background_scale: None,
        };
        let [nx, ny, nz] = mtext.extrusion;
        if !(nx.abs() < 1e-12 && ny.abs() < 1e-12 && nz > 0.0) {
            content_base.extrusion = Some(mtext.extrusion);
        }
        if mtext.has_bg_fill {
            // 背景の色の種類が 0xC8 か、291 が 1 なら画面の背景色で塗る（ezdxf の set_mtext_bg_fill と同じ）
            let kind = (js::to_uint32(mtext.bg_color) >> 24) & 0xff;
            content.background_flags =
                if mtext.use_window_bg_color || (kind != 0xc2 && kind != 0xc3) { 2.0 } else { 1.0 };
            if kind == 0xc2 {
                content.background_true_color = Some(f64::from(js::to_int32(mtext.bg_color) & 0xff_ffff));
            } else if kind == 0xc3 {
                content.background_color_index = Some(f64::from(js::to_int32(mtext.bg_color) & 0xff));
            }
            content.background_scale = Some(mtext.bg_scale_factor);
        }
        if mtext.column_type != 0.0 {
            self.notices.add(
                NoticeLevel::Info,
                "mleader-columns",
                "段組みのあるマルチ引出線の文字は、1 段で並べました",
            );
        }
        self.mtext(&content_base, &content, None, child)
    }

    /// ブロックの内容。属性（block_attribs）の値は、ブロックの中の ATTDEF の位置に描く。描いたブロックの名前を返す
    fn mleader_block(
        &mut self,
        base: &DxfEntityBase,
        entity: &DxfMLeader,
        block: &DxfMLeaderBlock,
        child: &Ctx,
    ) -> String {
        let Some(record) = self.block_records.get(&block.block_record_handle).copied() else {
            self.notices.add(NoticeLevel::Warning, "missing-block", "定義の見つからないブロック参照を省きました");
            return String::new();
        };
        // 配置点は OCS ではなく WCS で書かれる（ezdxf の RenderEngine.add_block_content と同じ）
        let axes = ocs_axes(Some(&block.extrusion));
        let [px, py, pz] = block.insert;
        let mut insert_base =
            DxfEntityBase { kind: "INSERT".into(), ..part_of(base, raw_color(block.color), "BYBLOCK") };
        let point = match axes {
            Some(axes) => axes.map(|axis| axis[0] * px + axis[1] * py + axis[2] * pz),
            None => block.insert,
        };
        if axes.is_some() {
            insert_base.extrusion = Some(block.extrusion);
        }
        let insert = DxfEntity {
            base: insert_base,
            data: DxfEntityData::Insert(DxfInsert {
                name: record.name.clone(),
                insert: point,
                xscale: block.scale[0],
                yscale: block.scale[1],
                zscale: block.scale[2],
                rotation: (block.rotation * 180.0) / PI,
                column_count: 1.0,
                row_count: 1.0,
                column_spacing: 0.0,
                row_spacing: 0.0,
                attribs: Vec::new(),
            }),
        };
        let values: HashMap<String, String> =
            entity.block_attribs.iter().map(|attrib| (attrib.handle.clone(), attrib.text.clone())).collect();
        self.insert(&insert, child, Some(&values));
        record.name.clone()
    }

    /// 引出線を描き、描いた線の数を返す。線は頂点から着地点へ、折れ線は着地点から内容の側へ引く。
    /// 折れ線を描かないとき（スプライン、長さ 0、291 が 0）は、線を折れ線の先まで延ばす。上下に接続する引出線は着地点で終わる
    fn mleader_leaders(
        &mut self,
        base: &DxfEntityBase,
        entity: &DxfMLeader,
        text: Option<&MTextLayout>,
        child: &Ctx,
        out: &mut Vec<Primitive>,
        connections: &mut Vec<Vec2>,
    ) -> u64 {
        let context = &entity.context;
        // 引出線の種類が 0 なら、内容だけを描く
        if entity.leader_type == 0.0 {
            return 0;
        }
        let linetype = self
            .linetype_handles
            .get(&entity.leader_linetype_handle)
            .map_or_else(|| "BYBLOCK".to_string(), |linetype| linetype.name.clone());
        // 線ごとの色（92）が BYBLOCK なら、MULTILEADER の引出線の色（91）
        let leader_part = |raw: f64| {
            let color = if (js::to_uint32(raw) >> 24) & 0xff == 0xc1 { entity.leader_line_color } else { raw };
            DxfEntityBase { lineweight: entity.leader_lineweight, ..part_of(base, raw_color(color), &linetype) }
        };
        let common = leader_part(entity.leader_line_color);
        let size = context.arrow_head_size;
        let mut drawn = 0;
        let mut hidden = 0;
        for leader in &context.leaders {
            if !leader.has_last_leader_line {
                hidden += 1;
                continue;
            }
            let landing: Vec2 = [leader.last_leader_point[0], leader.last_leader_point[1]];
            let horizontal = leader.attachment_direction != 1.0;
            let (dx, dy) =
                if leader.has_dogleg_vector { (leader.dogleg_vector[0], leader.dogleg_vector[1]) } else { (1.0, 0.0) };
            let vector = js::hypot2(dx, dy);
            let ux = if vector > 1e-12 { dx / vector } else { 1.0 };
            let uy = if vector > 1e-12 { dy / vector } else { 0.0 };
            let length = if vector > 1e-12 { leader.dogleg_length } else { 0.0 };
            let dogleg_end: Vec2 = [landing[0] + ux * length, landing[1] + uy * length];
            let dogleg = horizontal && entity.has_dogleg && entity.leader_type == 1.0 && length.abs() > 1e-12;
            let end = if horizontal && !dogleg { dogleg_end } else { landing };
            let connection = if horizontal { dogleg_end } else { landing };
            let mut lines = 0;
            for line in &leader.lines {
                let mut points: Vec<Vec2> = Vec::new();
                for [x, y] in line.vertices.iter().map(|vertex| [vertex[0], vertex[1]]).chain([end]) {
                    let far = points.last().is_none_or(|last| js::hypot2(x - last[0], y - last[1]) > 1e-9);
                    if far {
                        points.push([x, y]);
                    }
                }
                if points.len() < 2 {
                    continue;
                }
                lines += 1;
                let part = leader_part(line.color);
                let (p0, p1) = (points[0], points[1]);
                let segment = js::hypot2(p1[0] - p0[0], p1[1] - p0[1]);
                let direction: Vec2 = [(p1[0] - p0[0]) / segment, (p1[1] - p0[1]) / segment];
                // AutoCAD は、最初の線分が矢印の大きさの 2 倍以下なら矢印を描かない（ezdxf の RenderEngine.add_leader_line の注記）
                if size > 0.0 && segment > size * 2.0 {
                    let handle = entity
                        .arrow_heads
                        .iter()
                        .find(|head| head.index == line.index)
                        .map_or(&entity.arrow_head_handle, |head| &head.handle)
                        .clone();
                    let back = self.mleader_arrow(&handle, p0, direction, size, &part, child, out);
                    points[0] = [p0[0] + direction[0] * back, p0[1] + direction[1] * back];
                }
                if entity.leader_type == 2.0 {
                    // スプラインは点を通り、矢印の向きで出て、折れ線の向き（上下の接続は最後の線分の向き）で入る
                    let last = points[points.len() - 1];
                    let before = points[points.len() - 2];
                    let tangent = if horizontal { [ux, uy] } else { [last[0] - before[0], last[1] - before[1]] };
                    let curve = through_points(&slices(&points), 16, Some(&direction), Some(&tangent));
                    let stroke = self.stroke(&part, child, &child.m, &curve, false);
                    out.push(Primitive::Stroke(stroke));
                    self.notices.add(
                        NoticeLevel::Info,
                        "mleader-spline",
                        "スプラインのマルチ引出線は、点を通る曲線で近似しました",
                    );
                    continue;
                }
                let gaps: Vec<[f64; 4]> =
                    line.breaks.iter().map(|gap| [gap.start[0], gap.start[1], gap.end[0], gap.end[1]]).collect();
                for piece in cut_polyline(&flat(&points), &gaps) {
                    let stroke = self.stroke(&part, child, &child.m, &piece, false);
                    out.push(Primitive::Stroke(stroke));
                }
            }
            if lines == 0 {
                continue;
            }
            drawn += lines;
            connections.push(connection);
            if dogleg {
                let gaps: Vec<[f64; 4]> =
                    leader.breaks.iter().map(|gap| [gap.start[0], gap.start[1], gap.end[0], gap.end[1]]).collect();
                for piece in cut_polyline(&[landing[0], landing[1], dogleg_end[0], dogleg_end[1]], &gaps) {
                    let stroke = self.stroke(&common, child, &child.m, &piece, false);
                    out.push(Primitive::Stroke(stroke));
                }
            }
            if let Some(text) = text {
                self.mleader_text_line(entity, horizontal, connection, text, &common, child, out);
            }
        }
        if hidden > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "mleader-hidden",
                "着地点のない引出線（MULTILEADER の LEADER の 290 が 0）は、AutoCAD と同じく描いていません",
                hidden,
            );
        }
        drawn
    }

    /// 下線（左右の接続の 5・6・8）と、上下の接続の線（10）。ezdxf の add_text_underline・add_overline と同じ位置で、
    /// 長さは描いた文字の範囲に合わせる
    #[allow(clippy::too_many_arguments)]
    fn mleader_text_line(
        &mut self,
        entity: &DxfMLeader,
        horizontal: bool,
        connection: Vec2,
        text: &MTextLayout,
        part: &DxfEntityBase,
        child: &Ctx,
        out: &mut Vec<Primitive>,
    ) {
        let (cx, cy) = apply(&invert(&text.frame), connection[0], connection[1]);
        let b = &text.bbox;
        let points = if horizontal {
            let left = cx < (b.min_x + b.max_x) / 2.0;
            let kind = if left { entity.text_left_attachment_type } else { entity.text_right_attachment_type };
            if kind != 5.0 && kind != 6.0 && kind != 8.0 {
                return;
            }
            [cx, cy, if left { b.max_x } else { b.min_x }, cy]
        } else {
            let bottom = cy < (b.min_y + b.max_y) / 2.0;
            let kind = if bottom { entity.text_bottom_attachment_type } else { entity.text_top_attachment_type };
            if kind != 10.0 {
                return;
            }
            [b.min_x, cy, b.max_x, cy]
        };
        let stroke = self.stroke(part, child, &multiply(&child.m, &text.frame), &points, false);
        out.push(Primitive::Stroke(stroke));
    }

    /// 矢印を描き、引出線を始める位置（矢印の先からの距離）を返す。矢印のブロック（_Dot など）は先端が原点で -X に伸びるので、
    /// 線の向き + 180 度に回して矢印の大きさで拡大する（ezdxf の RenderEngine.add_arrow と同じ）。ブロックがなければ塗りつぶしの矢印
    #[allow(clippy::too_many_arguments)]
    fn mleader_arrow(
        &mut self,
        handle: &str,
        tip: Vec2,
        direction: Vec2,
        size: f64,
        part: &DxfEntityBase,
        child: &Ctx,
        out: &mut Vec<Primitive>,
    ) -> f64 {
        let named = !handle.is_empty() && handle != "0";
        let record = if named { self.block_records.get(handle).copied() } else { None };
        if let Some(record) = record {
            let insert = DxfEntity {
                // 矢印は引出線の線種によらず実線で描く
                base: DxfEntityBase { kind: "INSERT".into(), linetype: "CONTINUOUS".into(), ..part.clone() },
                data: DxfEntityData::Insert(DxfInsert {
                    name: record.name.clone(),
                    insert: [tip[0], tip[1], 0.0],
                    xscale: size,
                    yscale: size,
                    zscale: size,
                    rotation: (js::atan2(direction[1], direction[0]) * 180.0) / PI + 180.0,
                    column_count: 1.0,
                    row_count: 1.0,
                    column_spacing: 0.0,
                    row_spacing: 0.0,
                    attribs: Vec::new(),
                }),
            };
            self.insert(&insert, child, None);
            let name = record.name.strip_prefix('_').unwrap_or(&record.name).to_uppercase();
            return if ARROWS_FROM_TIP.contains(&name.as_str()) { 0.0 } else { size };
        }
        if named {
            self.notices.add(
                NoticeLevel::Info,
                "mleader-arrow",
                "見つからない矢印のブロックは、塗りつぶしの矢印で描きました",
            );
        }
        // 塗りつぶしの矢印（_ClosedFilled）: 長さは矢印の大きさ、根元の幅は大きさの 1/3（AutoCAD の代替表示と同じ）
        let [dx, dy] = direction;
        let bx = tip[0] + dx * size;
        let by = tip[1] + dy * size;
        let w = size / 6.0;
        let color = self.color(part, child);
        out.push(Primitive::Fill(FillPrimitive {
            color,
            rings: vec![transform_points(
                &child.m,
                &[tip[0], tip[1], bx - dy * w, by + dx * w, bx + dy * w, by - dx * w],
            )],
        }));
        size
    }

    /// マルチライン（DXF Reference「MLINE」「MLINESTYLE」）。要素の線は頂点から留め継ぎの向きへ 41 の 1 つ目の距離にあり、
    /// 41 の 2 つ目以降は線分の上で線を描く区間（頂点からの長さ。MLEDIT で切った切れ目）。塗り・端・留め継ぎは MLINESTYLE の 70 による。
    /// ezdxf の render/mline.py と同じ形に描き、端を描かない指定（71 の 4・8）と、内側の弧のすべての組（AutoCAD の MLSTYLE の説明）を足した
    fn mline(&mut self, base: &DxfEntityBase, entity: &DxfMLine, ctx: &Ctx) {
        let n = entity.points.len();
        if n < 2 {
            return;
        }
        let style = entity
            .mlinestyle_handle
            .as_ref()
            .filter(|handle| !handle.is_empty())
            .and_then(|handle| self.mline_style_handles.get(handle).copied())
            .or_else(|| self.mline_style_names.get(&entity.style_name.to_uppercase()).copied());
        let style: &DxfMLineStyle = match style {
            Some(style) if !style.elements.is_empty() => style,
            _ => {
                self.notices.add(
                    NoticeLevel::Info,
                    "mline-style",
                    "スタイルの見つからないマルチラインは、STANDARD（2 本の線）で描きました",
                );
                &STANDARD_MLINE_STYLE
            }
        };
        let elements = &style.elements;
        let count = elements.len();
        let mut order: Vec<usize> = (0..count).collect();
        js::sort_by(&mut order, |a, b| elements[*a].offset - elements[*b].offset);
        let bottom = order[0];
        let top = order[count - 1];
        // マルチラインの真色は使わない（CAD は無視する。ezdxf の render/mline.py の注記）。BYBLOCK の要素はマルチラインの色
        let owner = DxfEntityBase { true_color: None, ..base.clone() };
        let path = format!("{}{}>", ctx.path, handle_or_unknown(base));
        let child = self.child(&owner, ctx, &IDENTITY, path, &base.kind);
        let unit = |x: f64, y: f64| -> Option<Vec2> {
            let length = js::hypot2(x, y);
            (length > 1e-12).then(|| [x / length, y / length])
        };
        let point = |v: usize| entity.points[v];
        let direction = |v: usize| -> Vec2 {
            let i = v.min(n - 2);
            entity
                .vertex_directions
                .get(v)
                .and_then(|d| unit(d[0], d[1]))
                .or_else(|| unit(point(i + 1)[0] - point(i)[0], point(i + 1)[1] - point(i)[1]))
                .unwrap_or([1.0, 0.0])
        };
        let miter = |v: usize| -> Vec2 {
            let d = direction(v);
            entity.miter_directions.get(v).and_then(|m| unit(m[0], m[1])).unwrap_or([-d[1], d[0]])
        };
        let offsets: Vec<f64> = elements.iter().map(|element| element.offset).collect();
        let shift = if entity.justification == 0.0 {
            -max_of(&offsets)
        } else if entity.justification == 2.0 {
            -min_of(&offsets)
        } else {
            0.0
        };
        let line_params = |v: usize, e: usize| -> Option<&Vec<f64>> {
            entity.line_params.as_ref().and_then(|params| params.get(v)).and_then(|vertex| vertex.get(e))
        };
        let at: Vec<Vec<Vec2>> = entity
            .points
            .iter()
            .enumerate()
            .map(|(v, p)| {
                elements
                    .iter()
                    .enumerate()
                    .map(|(e, element)| {
                        let m = miter(v);
                        let t = match line_params(v, e).and_then(|params| params.first()) {
                            Some(t) => *t,
                            None => {
                                // 41 がないときは、スタイルのずれ・位置合わせ・尺度から、留め継ぎの向きへの距離を求める
                                let d = direction(v);
                                let across = -m[0] * d[1] + m[1] * d[0];
                                let offset = (element.offset + shift) * entity.scale;
                                if across.abs() > 1e-9 { offset / across } else { offset }
                            }
                        };
                        [p[0] + m[0] * t, p[1] + m[1] * t]
                    })
                    .collect()
            })
            .collect();
        let position = |v: usize, e: usize| at[v][e];
        let closed = entity.closed;
        let segments = if closed { n } else { n - 1 };
        let element_part = |e: usize, linetype: Option<&str>| {
            let color = PartColor { color_index: elements[e].color, true_color: None };
            let linetype =
                linetype.unwrap_or(if elements[e].linetype.is_empty() { "BYLAYER" } else { &elements[e].linetype });
            part_of(&owner, color, linetype)
        };
        let mut primitives: Vec<Primitive> = Vec::new();
        let mut strokes: Vec<Primitive> = Vec::new();
        // 要素の線。切れ目のない区間は頂点をまたいでつなぎ、線種の破線を続ける
        for e in 0..count {
            let mut pieces: Vec<Vec<f64>> = Vec::new();
            let mut current: Option<Vec<f64>> = None;
            for i in 0..segments {
                let [px, py] = position(i, e);
                let [qx, qy] = position((i + 1) % n, e);
                let length = js::hypot2(qx - px, qy - py);
                if length.is_nan() || length <= 1e-12 {
                    continue;
                }
                let ux = (qx - px) / length;
                let uy = (qy - py) / length;
                let params = line_params(i, e).map_or(&[][..], |params| params.get(1..).unwrap_or(&[]));
                for (a, b) in mline_dashes(params, length) {
                    match current.as_mut() {
                        Some(piece) if a < 1e-9 => piece.extend([px + ux * b, py + uy * b]),
                        _ => {
                            if let Some(piece) = current.take() {
                                pieces.push(piece);
                            }
                            current = Some(vec![px + ux * a, py + uy * a, px + ux * b, py + uy * b]);
                        }
                    }
                    if b < length - 1e-9
                        && let Some(piece) = current.take()
                    {
                        pieces.push(piece);
                    }
                }
            }
            if let Some(piece) = current {
                pieces.push(piece);
            }
            let part = element_part(e, None);
            let single = pieces.len() == 1;
            for piece in &pieces {
                let length = piece.len();
                let ring = closed
                    && single
                    && length >= 8
                    && js::hypot2(piece[0] - piece[length - 2], piece[1] - piece[length - 1]) < 1e-9;
                let points = if ring { &piece[..length - 2] } else { &piece[..] };
                let stroke = self.stroke(&part, &child, &child.m, points, ring);
                strokes.push(Primitive::Stroke(stroke));
            }
        }
        // 2 つの要素を結ぶ線（端の線・留め継ぎ）。中点までをそれぞれの要素の色・線種で描く（ezdxf の create_miter と同じ）
        let across = |builder: &mut Self, strokes: &mut Vec<Primitive>, v: usize| {
            let a = position(v, top);
            let b = position(v, bottom);
            let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
            let stroke =
                builder.stroke(&element_part(top, None), &child, &child.m, &[a[0], a[1], mid[0], mid[1]], false);
            strokes.push(Primitive::Stroke(stroke));
            let stroke =
                builder.stroke(&element_part(bottom, None), &child, &child.m, &[b[0], b[1], mid[0], mid[1]], false);
            strokes.push(Primitive::Stroke(stroke));
        };
        // 要素 from から to への半円（外側の向き outward にふくらむ）。色が違えば半分ずつ（ezdxf の _arc_caps と同じ）
        let half_circle = |from: Vec2, to: Vec2, outward: Vec2| -> (Vec<f64>, Vec<f64>) {
            let cx = (from[0] + to[0]) / 2.0;
            let cy = (from[1] + to[1]) / 2.0;
            let radius = js::hypot2(from[0] - cx, from[1] - cy);
            let start = (js::atan2(from[1] - cy, from[0] - cx) * 180.0) / PI;
            let ccw = -js::sin((start * PI) / 180.0) * outward[0] + js::cos((start * PI) / 180.0) * outward[1] >= 0.0;
            let tolerance = tolerance(&child.m, radius);
            // a から b（ccw なら a + 90、でなければ a − 90）へ回る 4 分の 1 の弧
            let half = |a: f64, b: f64| {
                if ccw {
                    arc_points(cx, cy, radius, a, b, tolerance)
                } else {
                    reverse_pairs(&arc_points(cx, cy, radius, b, a, tolerance))
                }
            };
            if ccw {
                (half(start, start + 90.0), half(start + 90.0, start + 180.0))
            } else {
                (half(start, start - 90.0), half(start - 90.0, start - 180.0))
            }
        };
        let cap =
            |builder: &mut Self, strokes: &mut Vec<Primitive>, from: usize, to: usize, v: usize, outward: Vec2| {
                let (first, second) = half_circle(position(v, from), position(v, to), outward);
                if elements[from].color == elements[to].color {
                    let mut points = first;
                    points.extend_from_slice(second.get(2..).unwrap_or(&[]));
                    let stroke = builder.stroke(&element_part(from, Some("BYBLOCK")), &child, &child.m, &points, false);
                    strokes.push(Primitive::Stroke(stroke));
                } else {
                    let stroke = builder.stroke(&element_part(from, Some("BYBLOCK")), &child, &child.m, &first, false);
                    strokes.push(Primitive::Stroke(stroke));
                    let stroke = builder.stroke(&element_part(to, Some("BYBLOCK")), &child, &child.m, &second, false);
                    strokes.push(Primitive::Stroke(stroke));
                }
            };
        let flags = js::to_int32(style.flags);
        let entity_flags = js::to_int32(entity.flags);
        let backward = direction(0).map(|value| -value);
        let forward =
            unit(point(n - 1)[0] - point(n - 2)[0], point(n - 1)[1] - point(n - 2)[1]).unwrap_or(direction(n - 1));
        let joined = |(first, second): (Vec<f64>, Vec<f64>)| [first, second].concat();
        let mut rounds: Vec<Vec<f64>> = Vec::new();
        if !closed {
            if entity_flags & 4 == 0 {
                if flags & 16 != 0 {
                    across(self, &mut strokes, 0);
                }
                if flags & 64 != 0 {
                    cap(self, &mut strokes, top, bottom, 0, backward);
                    rounds.push(joined(half_circle(position(0, top), position(0, bottom), backward)));
                }
                if flags & 32 != 0 {
                    // 内側の弧は、外から 2 番目どうし、3 番目どうし…の組を結ぶ（要素が奇数なら中央は結ばない）
                    let mut k = 1;
                    while k + k + 1 < count {
                        cap(self, &mut strokes, order[count - 1 - k], order[k], 0, backward);
                        k += 1;
                    }
                }
            }
            if entity_flags & 8 == 0 {
                if flags & 256 != 0 {
                    across(self, &mut strokes, n - 1);
                }
                if flags & 1024 != 0 {
                    cap(self, &mut strokes, bottom, top, n - 1, forward);
                    rounds.push(joined(half_circle(position(n - 1, bottom), position(n - 1, top), forward)));
                }
                if flags & 512 != 0 {
                    let mut k = 1;
                    while k + k + 1 < count {
                        cap(self, &mut strokes, order[k], order[count - 1 - k], n - 1, forward);
                        k += 1;
                    }
                }
            }
        }
        if flags & 2 != 0 {
            let (from, to) = if closed { (0, n) } else { (1, n - 1) };
            for v in from..to {
                across(self, &mut strokes, v);
            }
        }
        if flags & 1 != 0 {
            // 塗りは外側の 2 つの要素の間。線分ごとの四角形と丸い端の半円を偶奇規則で塗る
            let mut rings: Vec<Vec<f64>> = rounds;
            for i in 0..segments {
                let j = (i + 1) % n;
                rings.push([position(i, bottom), position(j, bottom), position(j, top), position(i, top)].concat());
            }
            if entity
                .fill_params
                .as_ref()
                .is_some_and(|params| params.iter().any(|vertex| vertex.iter().any(|values| !values.is_empty())))
            {
                self.notices.add(
                    NoticeLevel::Info,
                    "mline-fill-breaks",
                    "マルチラインの塗りの切れ目（MLEDIT）は描かず、全体を塗りました",
                );
            }
            let color = self.color(
                &part_of(&owner, PartColor { color_index: style.fill_color, true_color: None }, "BYBLOCK"),
                &child,
            );
            primitives.push(Primitive::Fill(FillPrimitive {
                color,
                rings: rings.iter().map(|ring| transform_points(&child.m, ring)).collect(),
            }));
        }
        primitives.extend(strokes);
        let justification = match entity.justification {
            0.0 => Property::new("justification", "位置合わせ", "上").raw("top"),
            1.0 => Property::new("justification", "位置合わせ", "ゼロ").raw("zero"),
            2.0 => Property::new("justification", "位置合わせ", "下").raw("bottom"),
            value => Property::new("justification", "位置合わせ", js::number_to_string(value)).raw(value),
        };
        self.add(
            base,
            ctx,
            primitives,
            vec![
                Property::new("style", "スタイル", style.name.clone()),
                number_property("widthScale", "尺度", entity.scale * mean_scale(&ctx.local)),
                justification,
                count_property("vertexCount", "頂点", n, format!("{n} 点{}", if closed { "（閉じる）" } else { "" })),
            ],
            None,
        );
    }

    // --- ハッチング -----------------------------------------------------------

    /// 境界を点列（OCS）にする。辺の境界は、つながる向きに辺を並べ直す
    fn boundary(path: &DxfHatchPath, m: &Matrix) -> Vec<f64> {
        let tolerance = |radius: f64| tolerance(m, radius);
        let Some(edges) = &path.edges else {
            return bulge_points(&path.points, path.bulges.as_deref(), true, &tolerance);
        };
        let mut out: Vec<f64> = Vec::new();
        for edge in edges {
            let mut points = match edge {
                DxfHatchEdge::Line { start, end } => vec![start[0], start[1], end[0], end[1]],
                DxfHatchEdge::Arc { center, radius, start_angle, end_angle, ccw } => {
                    // 時計回りの弧は、360 から引いた角度を入れ替えて書く（ezdxf の ArcEdge.load_tags と同じ）
                    let (start, end) =
                        if *ccw { (*start_angle, *end_angle) } else { (360.0 - end_angle, 360.0 - start_angle) };
                    let points = arc_points(center[0], center[1], *radius, start, end, tolerance(*radius));
                    if *ccw { points } else { reverse_pairs(&points) }
                }
                DxfHatchEdge::Ellipse { center, major_axis, ratio, start_angle, end_angle, ccw } => {
                    let [mx, my] = *major_axis;
                    let (start_angle, end_angle) =
                        if *ccw { (*start_angle, *end_angle) } else { (360.0 - end_angle, 360.0 - start_angle) };
                    // 楕円の辺の角度は長軸からの実際の角度。媒介変数に直す（ezdxf の angle_to_param）
                    let param = |deg: f64| {
                        let a = (deg * PI) / 180.0;
                        js::atan2(js::sin(a) / or(*ratio, 1.0), js::cos(a))
                    };
                    let p0 = param(start_angle);
                    let mut p1 = param(end_angle);
                    if (end_angle - start_angle).abs() >= 360.0 - 1e-9 {
                        p1 = p0 + PI * 2.0;
                    }
                    if p1 <= p0 {
                        p1 += PI * 2.0;
                    }
                    let points = ellipse_points(
                        &[center[0], center[1], 0.0],
                        &[mx, my, 0.0],
                        &[-my * ratio, mx * ratio, 0.0],
                        p0,
                        p1,
                        tolerance(js::hypot2(mx, my)),
                    );
                    if *ccw { points } else { reverse_pairs(&points) }
                }
                DxfHatchEdge::Spline { degree, knots, control_points, weights, fit_points, .. } => {
                    if control_points.len() >= 2 {
                        nurbs_points(*degree, &slices(control_points), knots, weights, 64.0)
                    } else {
                        through_points(&slices(fit_points), 16, None, None)
                    }
                }
            };
            if points.len() < 2 {
                continue;
            }
            if out.len() >= 2 {
                let lx = out[out.len() - 2];
                let ly = out[out.len() - 1];
                let d_start = js::hypot2(points[0] - lx, points[1] - ly);
                let d_end = js::hypot2(points[points.len() - 2] - lx, points[points.len() - 1] - ly);
                if d_end < d_start {
                    points = reverse_pairs(&points);
                }
            }
            out.extend(points);
        }
        out
    }

    fn hatch(&mut self, base: &DxfEntityBase, entity: &DxfHatch, ctx: &Ctx) {
        let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), entity.elevation));
        let mut paths: Vec<&DxfHatchPath> = entity.paths.iter().collect();
        if entity.hatch_style == 2.0 {
            let outer: Vec<&DxfHatchPath> =
                paths.iter().copied().filter(|path| js::to_int32(path.flags) & 17 != 0).collect();
            if !outer.is_empty() {
                paths = outer;
            }
        }
        let local: Vec<Vec<f64>> =
            paths.iter().map(|path| Self::boundary(path, &m)).filter(|ring| ring.len() >= 6).collect();
        if local.is_empty() {
            return;
        }
        let rings: Vec<Vec<f64>> = local.iter().map(|ring| transform_points(&m, ring)).collect();
        let color = self.color(base, ctx);
        let mut properties = vec![if entity.solid_fill {
            Property::new("pattern", "模様", "塗りつぶし").raw("SOLID")
        } else {
            let name = if entity.pattern_name.is_empty() {
                "（名前なし）".to_string()
            } else {
                entity.pattern_name.clone()
            };
            Property::new("pattern", "模様", name).raw(entity.pattern_name.clone())
        }];
        if entity.gradient.is_some() {
            self.notices.add(NoticeLevel::Info, "hatch-gradient", "グラデーションのハッチングは 1 色で塗りました");
            let first = entity.gradient_colors.as_ref().and_then(|colors| colors.first()).copied();
            let fill = FillPrimitive { color: first.map_or(color, true_color), rings };
            self.add(base, ctx, vec![Primitive::Fill(fill)], properties, None);
            return;
        }
        if entity.solid_fill {
            self.add(base, ctx, vec![Primitive::Fill(FillPrimitive { color, rings })], properties, None);
            return;
        }
        properties.push(number_property("patternScale", "模様の尺度", entity.pattern_scale));
        properties.push(
            Property::new("patternAngle", "模様の角度", format!("{}°", fmt(entity.pattern_angle)))
                .raw(entity.pattern_angle),
        );
        let mut bbox = empty_box();
        for ring in &local {
            extend_box_with_points(&mut bbox, ring);
        }
        let outlines = |rings: Vec<Vec<f64>>, color: &str| -> Vec<Primitive> {
            rings.into_iter().map(|ring| Primitive::Stroke(stroke_primitive(ring, true, color.to_string()))).collect()
        };
        match Self::pattern(entity, &bbox, &m, &color) {
            None => {
                self.notices.add(
                    NoticeLevel::Info,
                    "hatch-dense",
                    "線の多すぎるハッチングの模様は、外形だけを描きました",
                );
                self.add(base, ctx, outlines(rings, &color), properties, None);
            }
            Some(strokes) if strokes.is_empty() => {
                self.notices.add(
                    NoticeLevel::Info,
                    "hatch-pattern",
                    "模様の定義がないハッチングは、外形だけを描きました",
                );
                self.add(base, ctx, outlines(rings, &color), properties, None);
            }
            Some(strokes) => {
                self.add(base, ctx, vec![Primitive::Clipped(ClippedPrimitive { rings, strokes })], properties, None)
            }
        }
    }

    /// 模様の線（OCS）を境界の外接矩形いっぱいに引く。境界での切り抜きは描画が行う。多すぎれば None
    fn pattern(entity: &DxfHatch, b: &BBox, m: &Matrix, color: &str) -> Option<Vec<StrokePrimitive>> {
        let mut strokes = Vec::new();
        let mut count = 0.0;
        let corners: [Vec2; 4] = [[b.min_x, b.min_y], [b.max_x, b.min_y], [b.max_x, b.max_y], [b.min_x, b.max_y]];
        for line in &entity.pattern_lines {
            let a = (line.angle * PI) / 180.0;
            let d: Vec2 = [js::cos(a), js::sin(a)];
            let n: Vec2 = [-d[1], d[0]];
            let spacing = line.offset[0] * n[0] + line.offset[1] * n[1];
            if spacing.is_nan() || spacing.abs() <= 1e-9 {
                continue;
            }
            let ts = corners.map(|c| ((c[0] - line.base[0]) * n[0] + (c[1] - line.base[1]) * n[1]) / spacing);
            let k0 = min_of(&ts).floor();
            let k1 = max_of(&ts).ceil();
            count += k1 - k0 + 1.0;
            // 本数が数えられない（基点・間隔が NaN の）ときと、基点が遠すぎて線の番号に 1 を足しても変わらない
            // （2⁵³ を超える）ときも、多すぎるものとして扱う
            if count.is_nan() || count > MAX_PATTERN_LINES || k0.abs().max(k1.abs()) > MAX_SAFE_INTEGER {
                return None;
            }
            let period = line.dashes.iter().fold(0.0, |sum, value| sum + value.abs());
            let mut segments: Vec<f64> = Vec::new();
            let mut k = k0;
            while k <= k1 {
                let px = line.base[0] + k * line.offset[0];
                let py = line.base[1] + k * line.offset[1];
                let us = corners.map(|c| (c[0] - px) * d[0] + (c[1] - py) * d[1]);
                let mut u0 = min_of(&us);
                let u1 = max_of(&us);
                // 破線の位相は各線の原点（基点 + k × 間隔）から数える
                if period > 0.0 {
                    u0 = (u0 / period).floor() * period;
                }
                segments.extend([px + u0 * d[0], py + u0 * d[1], px + u1 * d[0], py + u1 * d[1]]);
                k += 1.0;
            }
            if segments.is_empty() {
                continue;
            }
            strokes.push(StrokePrimitive {
                points: transform_points(m, &segments),
                closed: false,
                segments: Some(true),
                color: color.to_string(),
                width: 0.0,
                dashes: dash_array(&line.dashes, mean_scale(m)),
            });
        }
        Some(strokes)
    }

    fn image(&mut self, base: &DxfEntityBase, entity: &DxfImage, ctx: &Ctx) {
        let [w, h] = entity.image_size;
        let mut pixels: Vec<Vec2> = entity.boundary.clone();
        if pixels.is_empty() {
            pixels = vec![[-0.5, -0.5], [w - 0.5, h - 0.5]];
        }
        if pixels.len() == 2 {
            let (a, b) = (pixels[0], pixels[1]);
            pixels = vec![a, [b[0], a[1]], b, [a[0], b[1]]];
        }
        // 画素の座標は左上が原点で、Y は下向き（ezdxf の Image.boundary_path_wcs と同じ）
        let mut ring: Vec<f64> = Vec::new();
        for [px, py] in pixels {
            let u = px + 0.5;
            let v = h - py - 0.5;
            ring.push(entity.insert[0] + entity.u_pixel[0] * u + entity.v_pixel[0] * v);
            ring.push(entity.insert[1] + entity.u_pixel[1] * u + entity.v_pixel[1] * v);
        }
        let points = transform_points(&ctx.m, &ring);
        if base.kind == "WIPEOUT" {
            let fill = FillPrimitive { color: BACKGROUND_COLOR.into(), rings: vec![points] };
            self.add(base, ctx, vec![Primitive::Fill(fill)], Vec::new(), None);
            return;
        }
        self.notices.add(NoticeLevel::Info, "image", "画像（IMAGE）は読み込まず、外形だけを描きました");
        let color = self.color(base, ctx);
        self.add(
            base,
            ctx,
            vec![Primitive::Stroke(stroke_primitive(points, true, color))],
            vec![Property::new("imageSize", "画素", format!("{} × {}", fmt(w), fmt(h))).raw([w, h])],
            None,
        );
    }

    /// 構築線・放射線を、ほかの図形の範囲（limit）で切って描く
    fn finish_infinite(&mut self, limit: BBox) {
        if self.infinite.is_empty() {
            return;
        }
        let b = if is_empty_box(&limit) { None } else { Some(limit) };
        for (entity, ctx) in std::mem::take(&mut self.infinite) {
            let DxfEntityData::XLine { start, unit_vector } = &entity.data else {
                continue;
            };
            let (px, py) = apply(&ctx.m, start[0], start[1]);
            let (dx, dy) = apply_vector(&ctx.m, unit_vector[0], unit_vector[1]);
            let length = js::hypot2(dx, dy);
            if length.is_nan() || length <= 0.0 {
                continue;
            }
            let ux = dx / length;
            let uy = dy / length;
            let reach = match &b {
                Some(b) => {
                    js::hypot2(b.max_x - b.min_x, b.max_y - b.min_y) * 2.0 + js::hypot2(px - b.min_x, py - b.min_y)
                }
                None => 1000.0,
            };
            let from = if entity.base.kind == "XLINE" { -reach } else { 0.0 };
            let mut points = vec![px + ux * from, py + uy * from, px + ux * reach, py + uy * reach];
            if let Some(b) = &b {
                match clip_polyline(&points, false, b).into_iter().next() {
                    Some(clipped) => points = clipped,
                    None => continue,
                }
            }
            let color = self.color(&entity.base, &ctx);
            let width = self.lineweight(&entity.base, &ctx);
            let dashes = self.dashes(&entity.base, &ctx, &ctx.m);
            let stroke = StrokePrimitive { points, closed: false, segments: None, color, width, dashes };
            self.notices.add(NoticeLevel::Info, "xline", "構築線・放射線は、ほかの図形の範囲で切って描きました");
            let flat_ctx = Ctx { m: IDENTITY, ..ctx };
            self.add(&entity.base, &flat_ctx, vec![Primitive::Stroke(stroke)], Vec::new(), None);
        }
    }

    fn finish_notices(&mut self) {
        if self.shx_count > 0 {
            let mut fonts = self.shx_fonts.clone();
            fonts.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            self.notices.add_count(
                NoticeLevel::Info,
                "shx-font",
                format!("SHX フォント（{}）は同梱していないため、代わりのフォントで描きました", fonts.join("・")),
                self.shx_count,
            );
        }
    }
}

/// `/\.(ttf|ttc|otf)$/i.test(file)` など（ASCII の大文字・小文字を区別しない末尾）
fn ends_with_any(file: &str, suffixes: &[&str]) -> bool {
    suffixes.iter().any(|suffix| {
        file.len() >= suffix.len()
            && file.is_char_boundary(file.len() - suffix.len())
            && file[file.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
    })
}

fn strip_suffix_any<'s>(file: &'s str, suffixes: &[&str]) -> &'s str {
    for suffix in suffixes {
        if ends_with_any(file, &[suffix]) {
            return &file[..file.len() - suffix.len()];
        }
    }
    file
}

/// 文字スタイルの表示（名前とフォント）
fn style_label(style: Option<&DxfTextStyle>, fallback: &str) -> String {
    match style {
        Some(style) => {
            let font = style
                .font_family
                .as_deref()
                .filter(|family| !family.is_empty())
                .or(Some(style.font.as_str()).filter(|font| !font.is_empty()))
                .unwrap_or("txt");
            format!("{}（{font}）", style.name)
        }
        None => fallback.to_string(),
    }
}

/// 幅が w0 から w1 に変わる帯の外形（中心線の左右に半分ずつ）
fn band(line: &[f64], w0: f64, w1: f64) -> Option<Vec<f64>> {
    let n = line.len() / 2;
    if n < 2 {
        return None;
    }
    let mut total = 0.0;
    let mut lengths = vec![0.0];
    for i in 1..n {
        total += js::hypot2(line[i * 2] - line[i * 2 - 2], line[i * 2 + 1] - line[i * 2 - 1]);
        lengths.push(total);
    }
    if total.is_nan() || total <= 0.0 {
        return None;
    }
    let mut left: Vec<f64> = Vec::new();
    let mut right: Vec<f64> = Vec::new();
    for i in 0..n {
        let j = (i + 1).min(n - 1);
        let k = i.saturating_sub(1);
        let dx = line[j * 2] - line[k * 2];
        let dy = line[j * 2 + 1] - line[k * 2 + 1];
        let length = or(js::hypot2(dx, dy), 1.0);
        let half = (w0 + ((w1 - w0) * lengths[i]) / total) / 2.0;
        let nx = (-dy / length) * half;
        let ny = (dx / length) * half;
        left.extend([line[i * 2] + nx, line[i * 2 + 1] + ny]);
        right.extend([line[i * 2] - nx, line[i * 2 + 1] - ny]);
    }
    let mut ring = left;
    for i in (0..n).rev() {
        ring.extend([right[i * 2], right[i * 2 + 1]]);
    }
    Some(ring)
}

/// MULTILEADER の色の生の値（上位 1 バイトが種類）。0xC0 BYLAYER、0xC1 BYBLOCK、0xC2 RGB、0xC3 ACI（ezdxf の colors.py）
fn raw_color(raw: f64) -> PartColor {
    let kind = (js::to_uint32(raw) >> 24) & 0xff;
    match kind {
        0xc2 => PartColor { color_index: 256.0, true_color: Some(f64::from(js::to_int32(raw) & 0xff_ffff)) },
        0xc3 => PartColor { color_index: f64::from(js::to_int32(raw) & 0xff), true_color: None },
        0xc0 => PartColor { color_index: 256.0, true_color: None },
        _ => PartColor { color_index: 0.0, true_color: None },
    }
}

/// 図形の部品（マルチ引出線の文字・引出線、マルチラインの要素）の属性。BYBLOCK は親の図形の値になる
fn part_of(entity: &DxfEntityBase, color: PartColor, linetype: &str) -> DxfEntityBase {
    DxfEntityBase {
        kind: entity.kind.clone(),
        handle: entity.handle.clone(),
        ordinal: entity.ordinal,
        owner: None,
        space: entity.space,
        block: None,
        layer: entity.layer.clone(),
        linetype: linetype.to_string(),
        color_index: color.color_index,
        true_color: color.true_color,
        transparency: entity.transparency,
        lineweight: -2.0,
        linetype_scale: entity.linetype_scale,
        invisible: false,
        thickness: None,
        extrusion: None,
    }
}

/// MLINE の線のパラメータの 2 つ目以降（線の始まり、切れ目の始まり、線の再開…。頂点からの長さ）を、
/// 長さ length の線分の上で描く区間にする。値がなければ全体を描く
fn mline_dashes(params: &[f64], length: f64) -> Vec<(f64, f64)> {
    if params.is_empty() {
        return vec![(0.0, length)];
    }
    let mut out = Vec::new();
    let mut k = 0;
    while k < params.len() {
        let a = js::max(0.0, js::min(length, params[k]));
        let b = js::max(0.0, js::min(length, params.get(k + 1).copied().unwrap_or(length)));
        if b - a > 1e-9 {
            out.push((a, b));
        }
        k += 2;
    }
    out
}

fn reverse_pairs(points: &[f64]) -> Vec<f64> {
    let mut out = Vec::with_capacity(points.len());
    let mut i = points.len() as isize - 2;
    while i >= 0 {
        out.push(points[i as usize]);
        out.push(points[i as usize + 1]);
        i -= 2;
    }
    out
}

/// 長さの単位の mm、表示する名前、形式によらないキー（Scene::unit）
fn unit_info(doc: &DxfDocument, notices: &mut NoticeList) -> (f64, String, LengthUnit) {
    let code = doc.header.insunits;
    if let (Some(unit), Some(length_unit)) = (unit_to_mm(code), length_unit(code)) {
        let name = unit_name(code).map_or_else(|| format!("$INSUNITS {}", js::number_to_string(code)), str::to_string);
        return (unit, name, length_unit);
    }
    if code == 0.0 && doc.header.measurement == Some(0.0) {
        notices.add(
            NoticeLevel::Info,
            "unit-assumed",
            "長さの単位の指定がなく、ヤード・ポンド法の図面なのでインチとみなしました",
        );
        return (25.4, "インチ（指定なし）".into(), LengthUnit::Inch);
    }
    notices.add(NoticeLevel::Info, "unit-assumed", "長さの単位の指定がないため、mm とみなしました");
    (1.0, "mm（指定なし）".into(), LengthUnit::Millimeter)
}

/// 図形の代表の点（原点を寄せる位置を決める）
pub(crate) fn anchor_of(entity: &DxfEntity) -> Option<Vec2> {
    let first = |points: &[Vec3]| points.first().map(|point| [point[0], point[1]]);
    match &entity.data {
        DxfEntityData::Line { start, .. } => Some([start[0], start[1]]),
        DxfEntityData::Circle { center, .. }
        | DxfEntityData::Arc { center, .. }
        | DxfEntityData::Ellipse { center, .. } => Some([center[0], center[1]]),
        DxfEntityData::Point { location, .. } => Some([location[0], location[1]]),
        DxfEntityData::LwPolyline(DxfLwPolyline { vertices, .. })
        | DxfEntityData::Polyline(DxfPolyline { vertices, .. }) => first(&vertices.points),
        DxfEntityData::Spline(spline) => first(&spline.control_points).or_else(|| first(&spline.fit_points)),
        DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
            Some([text.insert[0], text.insert[1]])
        }
        DxfEntityData::MText(DxfMText { insert, .. })
        | DxfEntityData::Insert(DxfInsert { insert, .. })
        | DxfEntityData::Table(DxfTable { insert, .. })
        | DxfEntityData::Image(DxfImage { insert, .. }) => Some([insert[0], insert[1]]),
        DxfEntityData::Dimension(dimension) => Some([dimension.defpoint[0], dimension.defpoint[1]]),
        DxfEntityData::Solid { vtx0, .. } => Some([vtx0[0], vtx0[1]]),
        DxfEntityData::Leader(DxfLeader { points, .. }) | DxfEntityData::MLine(DxfMLine { points, .. }) => {
            first(points)
        }
        DxfEntityData::MLeader(mleader) => {
            let context = &mleader.context;
            context
                .mtext
                .as_ref()
                .map(|mtext| mtext.insert)
                .or_else(|| context.block.as_ref().map(|block| block.insert))
                .or_else(|| context.leaders.first().map(|leader| leader.last_leader_point))
                .map(|point| [point[0], point[1]])
        }
        DxfEntityData::Hatch(hatch) => {
            let path = hatch.paths.first()?;
            if let Some(point) = path.points.first() {
                return Some([point[0], point[1]]);
            }
            match path.edges.as_ref()?.first()? {
                DxfHatchEdge::Line { start, .. } => Some(*start),
                DxfHatchEdge::Arc { center, .. } | DxfHatchEdge::Ellipse { center, .. } => Some(*center),
                DxfHatchEdge::Spline { .. } => None,
            }
        }
        DxfEntityData::Viewport(_) | DxfEntityData::XLine { .. } => None,
    }
}

/// 座標の大きい図面（公共座標の測量図など）は、Canvas の単精度の計算で線が揺れないよう、
/// 図形の中央の近くを原点に寄せる。寄せた分は Placement の inverse で実寸の座標に戻す
fn origin_shift(entities: &[&DxfEntity], unit: f64) -> Vec2 {
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for entity in entities {
        if let Some(p) = anchor_of(entity)
            && p[0].is_finite()
            && p[1].is_finite()
        {
            xs.push(p[0]);
            ys.push(p[1]);
        }
    }
    if xs.is_empty() {
        return [0.0, 0.0];
    }
    let median = |values: &mut Vec<f64>| {
        js::sort_by(values, |a, b| a - b);
        values[values.len() / 2]
    };
    let cx = median(&mut xs);
    let cy = median(&mut ys);
    if js::hypot2(cx, cy) * unit < 100_000.0 {
        return [0.0, 0.0];
    }
    let step = 1000.0 / unit;
    [js::round(cx / step) * step, js::round(cy / step) * step]
}

fn scale_label(paper_per_real: f64) -> String {
    if paper_per_real.is_nan() || paper_per_real <= 0.0 {
        return "-".into();
    }
    let denominator = 1.0 / paper_per_real;
    if denominator >= 1.0 {
        return format!("1:{}", fmt(js::string_to_number(&js::to_precision(denominator, 6))));
    }
    format!("{}:1", fmt(js::string_to_number(&js::to_precision(paper_per_real, 6))))
}

fn default_view(doc: &DxfDocument, builder: &Builder<'_>) -> String {
    let paper: Vec<&DxfLayout> = doc.layouts.iter().filter(|layout| !layout.model).collect();
    let active = paper.iter().find(|layout| layout.active);
    let has_content = |layout: &DxfLayout| !builder.block_entities(&layout.block).is_empty();
    if doc.header.tilemode == 0.0
        && let Some(active) = active
        && has_content(active)
    {
        return active.name.clone();
    }
    if !builder.model.is_empty() {
        return "Model".into();
    }
    paper.iter().find(|layout| has_content(layout)).map_or_else(|| "Model".into(), |layout| layout.name.clone())
}

/// 形式の名前。DWG から読んだ文書（zumen-dwg）は DWG
fn format_name(doc: &DxfDocument) -> &'static str {
    if doc.format == "dwg" { "DWG" } else { "DXF" }
}

fn metadata_base(doc: &DxfDocument, unit_name: &str) -> Vec<Property> {
    let encoding = match doc.encoding {
        "utf-8" => "UTF-8",
        "utf-16le" => "UTF-16",
        "shift_jis" => "Shift_JIS（CP932）",
        "windows-1252" => "西欧（Windows-1252）",
        other => other,
    };
    let source = match doc.encoding_source {
        EncodingSource::Version => format!("AutoCAD 2007 以降の {}", format_name(doc)),
        EncodingSource::Codepage => {
            js::trim(&format!("$DWGCODEPAGE {}", doc.header.codepage.as_deref().unwrap_or(""))).to_string()
        }
        EncodingSource::Detected => "中身から判断".into(),
        EncodingSource::Default => "指定なし".into(),
    };
    let blocks = doc.blocks.iter().filter(|block| !block.layout && !block.anonymous).count();
    let format = match release(&doc.version) {
        Some(release) => format!("AutoCAD {release}"),
        None => doc.version.clone(),
    };
    let mut metadata = vec![
        Property::new(
            "format",
            "形式",
            if doc.format == "dwg" {
                format!("DWG {format}（{}）", doc.version)
            } else {
                format!("DXF {format}（{}、{}）", doc.version, if doc.binary { "バイナリ" } else { "テキスト" })
            },
        ),
        Property::new("encoding", "文字コード", format!("{encoding}（{source}）")).raw(doc.encoding),
        Property::new("units", "単位", unit_name),
    ];
    if blocks > 0 {
        metadata.push(count_property("blockCount", "ブロック", blocks, format!("{blocks} 件")));
    }
    let layouts: Vec<&str> =
        doc.layouts.iter().filter(|layout| !layout.model).map(|layout| layout.name.as_str()).collect();
    if !layouts.is_empty() {
        metadata.push(Property::new("layouts", "レイアウト", layouts.join("・")));
    }
    metadata
}

fn root_ctx(m: Matrix, local: Matrix, placement: Option<Arc<Placement>>) -> Ctx {
    Ctx {
        m,
        local,
        layers: Vec::new(),
        parent_layer: None,
        color: aci_color(7.0),
        lineweight: DEFAULT_LINEWEIGHT,
        linetype: "CONTINUOUS".into(),
        path: String::new(),
        blocks: Vec::new(),
        depth: 0,
        placement,
        dash_factor: 1.0,
        clip: None,
        frozen: None,
    }
}

fn views(doc: &DxfDocument) -> Vec<SceneView> {
    let mut views = vec![SceneView { id: "Model".into(), name: "モデル".into() }];
    views.extend(
        doc.layouts
            .iter()
            .filter(|layout| !layout.model)
            .map(|layout| SceneView { id: layout.name.clone(), name: layout.name.clone() }),
    );
    views
}

/// 図形を描く順（SORTENTSTABLE）に、空間ごとに並べたもの。表示（dxf_to_scene）と同じ順で、ほかの形式に写すときに使う
pub struct DrawOrder<'a> {
    /// モデル空間の図形
    pub model: Vec<&'a DxfEntity>,
    /// ブロック・紙空間のブロック（名前は大文字）の図形
    pub blocks: HashMap<String, Vec<&'a DxfEntity>>,
}

/// 図形を、表示と同じく空間ごとに描く順に並べる
pub fn draw_order(doc: &DxfDocument) -> DrawOrder<'_> {
    let builder = Builder::new(doc);
    DrawOrder {
        model: builder.model.to_vec(),
        blocks: builder.blocks.iter().map(|(name, contents)| (name.clone(), contents.entities.to_vec())).collect(),
    }
}

/// 図面の単位の mm（表示と同じく、$INSUNITS がなければヤード・ポンド法の図面はインチ、ほかは mm とみなす）
pub fn drawing_unit(doc: &DxfDocument) -> f64 {
    unit_info(doc, &mut NoticeList::new()).0
}

/// AutoCAD の図面（DXF・DWG から読んだモデル）を、モデル空間か 1 つのレイアウトの表示用の図面にする。
/// layout は "Model"（モデル空間）かレイアウトの名前。None なら図面を保存したときの空間
/// レイアウトの用紙。紙空間の座標 × paper_unit + origin が用紙の mm（左下が原点、幅 width・高さ height）
pub(crate) struct LayoutSheet {
    pub paper_unit: f64,
    pub width: f64,
    pub height: f64,
    pub origin: Vec2,
}

pub(crate) fn layout_sheet(layout: &DxfLayout) -> LayoutSheet {
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
        // 用紙の大きさがない（R12 など）ときは、紙空間の範囲を用紙とする
        let [x0, y0] = layout.limmin;
        let [x1, y1] = layout.limmax;
        width = (x1 - x0) * paper_unit;
        height = (y1 - y0) * paper_unit;
        ox = -x0 * paper_unit;
        oy = -y0 * paper_unit;
    }
    LayoutSheet { paper_unit, width, height, origin: [ox, oy] }
}

/// 描いた文字の 1 つの run（`draw_line`・`draw_stack`）。同じ高さ・幅の係数・斜体・書体の 1 行の文字（TEXT）を
/// matrix の位置と向きに置けば、同じに描ける
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextPiece {
    pub text: String,
    /// ベースラインの左端を原点とする文字の座標系 → 図形の座標系
    pub matrix: Matrix,
    pub height: f64,
    pub width_factor: f64,
    pub oblique: f64,
    pub color: String,
    /// 書式コード（\f）の書体
    pub font: Option<RunFont>,
    /// 文字スタイルの名前（ないときは Standard）
    pub style: Option<String>,
}

/// 図形を、zumen が描く形のまま R12 の図形に崩すための描く要素（`r12::prepare`）
pub(crate) struct Exploded {
    /// 描く順の要素。文字（Primitive::Text）は texts の同じ順のものに対応する
    pub primitives: Vec<Primitive>,
    pub texts: Vec<TextPiece>,
    /// 図形そのものの色。部品の色がこれと同じなら、部品は図形の色を継ぐ
    pub color: String,
    /// 図形の線種が破線か（実線の部品は CONTINUOUS にする）
    pub dashed: bool,
}

/// 図形を、図形の座標系（モデル空間・紙空間・ブロックの WCS）のまま描く。曲線の細かさはモデル空間の表示と同じ
/// （図面の単位を mm にして弦のずれを決める）。ブロックの中の図形は、ブロックが配置される最大の尺度で細かくする
pub(crate) struct Exploder<'a> {
    builder: Builder<'a>,
    unit: f64,
}

impl<'a> Exploder<'a> {
    pub(crate) fn new(doc: &'a DxfDocument) -> Self {
        Self { builder: Builder::new(doc), unit: drawing_unit(doc) }
    }

    pub(crate) fn explode(&mut self, entity: &DxfEntity, scale: f64) -> Exploded {
        let unit = self.unit * scale;
        let ctx = root_ctx([unit, 0.0, 0.0, unit, 0.0, 0.0], IDENTITY, None);
        let start = self.builder.entities.len();
        self.builder.pieces = Some(Vec::new());
        self.builder.emit(entity, &ctx);
        let texts = self.builder.pieces.take().unwrap_or_default();
        let back: Matrix = [1.0 / unit, 0.0, 0.0, 1.0 / unit, 0.0, 0.0];
        let points = |points: &[f64]| transform_points(&back, points);
        let stroke = |stroke: StrokePrimitive| StrokePrimitive {
            points: points(&stroke.points),
            width: stroke.width / unit,
            dashes: stroke.dashes.iter().map(|dash| dash / unit).collect(),
            ..stroke
        };
        let primitives = self
            .builder
            .entities
            .drain(start..)
            .flat_map(|entity| entity.primitives)
            .map(|primitive| match primitive {
                Primitive::Stroke(value) => Primitive::Stroke(stroke(value)),
                Primitive::Fill(fill) => Primitive::Fill(FillPrimitive {
                    rings: fill.rings.iter().map(|ring| points(ring)).collect(),
                    ..fill
                }),
                Primitive::Clipped(clipped) => Primitive::Clipped(ClippedPrimitive {
                    rings: clipped.rings.iter().map(|ring| points(ring)).collect(),
                    strokes: clipped.strokes.into_iter().map(stroke).collect(),
                }),
                Primitive::Dot(dot) => Primitive::Dot(DotPrimitive { x: dot.x / unit, y: dot.y / unit, ..dot }),
                Primitive::Text(text) => Primitive::Text(text),
            })
            .collect();
        let texts =
            texts.into_iter().map(|piece| TextPiece { matrix: multiply(&back, &piece.matrix), ..piece }).collect();
        let color = self.builder.color(&entity.base, &ctx);
        let dashed = !self.builder.dashes(&entity.base, &ctx, &ctx.m).is_empty();
        Exploded { primitives, texts, color, dashed }
    }
}

pub fn dxf_to_scene(doc: &DxfDocument, layout: Option<&str>) -> Scene {
    let mut builder = Builder::new(doc);
    let (unit, unit_name, length_unit) = unit_info(doc, &mut builder.notices);
    let mut view = layout.map_or_else(|| default_view(doc, &builder), str::to_string);
    let layout = if view == "Model" {
        None
    } else {
        doc.layouts.iter().find(|candidate| !candidate.model && candidate.name == view)
    };
    if view != "Model" && layout.is_none() {
        builder.notices.add(
            NoticeLevel::Warning,
            "unknown-layout",
            format!("レイアウト {view} が見つからないため、モデル空間を表示しました"),
        );
        view = "Model".into();
    }
    let mut metadata = metadata_base(doc, &unit_name);
    let mut sheet: Option<Sheet> = None;
    let mut background = Background::Dark;
    match layout {
        None => {
            let model = builder.model.clone();
            let [ox, oy] = origin_shift(&model, unit);
            let placement = Arc::new(Placement {
                name: format!("モデル空間（{unit_name}）"),
                scale_x: 1.0,
                scale_y: 1.0,
                inverse: [1.0, 0.0, 0.0, 1.0, ox * unit, oy * unit],
            });
            let root = root_ctx([unit, 0.0, 0.0, unit, -ox * unit, -oy * unit], IDENTITY, Some(placement));
            for entity in model.iter() {
                builder.emit(entity, &root);
            }
            metadata.push(Property::new("view", "表示", "モデル空間").raw("Model"));
            if ox != 0.0 || oy != 0.0 {
                metadata.push(
                    Property::new(
                        "viewOrigin",
                        "表示の原点",
                        format!("{}（座標の大きい図面のため寄せた）", point2(ox, oy)),
                    )
                    .raw([ox, oy]),
                );
            }
            let paper = doc
                .entities
                .iter()
                .filter(|entity| entity.base.space == DxfSpace::Paper && entity.base.kind != "VIEWPORT")
                .count();
            if paper > 0 {
                builder.notices.add_count(
                    NoticeLevel::Info,
                    "paper-space",
                    "レイアウト（紙空間）の図形はモデル空間の表示に含めていません。表示を切り替えると見られます",
                    paper as u64,
                );
            }
        }
        Some(layout) => {
            background = Background::Light;
            let LayoutSheet { paper_unit, width, height, origin: [ox, oy] } = layout_sheet(layout);
            let paper_matrix: Matrix = [paper_unit, 0.0, 0.0, paper_unit, ox, oy];
            if width > 0.0 && height > 0.0 {
                let size = layout.paper_size.replace('_', " ");
                sheet = Some(Sheet {
                    name: layout.name.clone(),
                    size: if size.is_empty() { "レイアウト".into() } else { size },
                    width,
                    height,
                });
            }
            let root = root_ctx(paper_matrix, IDENTITY, None);
            let content = builder.block_entities(&layout.block);
            // レイアウトの全体を表すビューポート（ID 1、なければ最初のもの）は、モデルの表示ではない
            let viewports: Vec<(usize, &DxfViewport)> = content
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| match &entity.data {
                    DxfEntityData::Viewport(viewport) => Some((index, viewport)),
                    _ => None,
                })
                .collect();
            let overall = viewports
                .iter()
                .find(|(_, viewport)| viewport.id == 1.0)
                .or(viewports.first())
                .map(|(index, _)| *index);
            let mut scales: Vec<String> = Vec::new();
            for (index, entity) in content.iter().enumerate() {
                let DxfEntityData::Viewport(viewport) = &entity.data else {
                    builder.emit(entity, &root);
                    continue;
                };
                if Some(index) == overall
                    || viewport.status <= 0.0
                    || js::to_int32(viewport.status_flags) & 0x20000 != 0
                {
                    continue;
                }
                if let Some(label) =
                    draw_viewport(&mut builder, doc, &entity.base, viewport, &root, &paper_matrix, unit, paper_unit)
                {
                    scales.push(label);
                }
            }
            metadata
                .push(Property::new("view", "表示", format!("レイアウト {}", layout.name)).raw(layout.name.clone()));
            if let Some(sheet) = &sheet {
                metadata.push(Property::new(
                    "sheet",
                    "用紙",
                    format!("{}（{}×{} mm）", sheet.size, fmt(sheet.width), fmt(sheet.height)),
                ));
            }
            if !scales.is_empty() {
                let mut unique: Vec<String> = Vec::new();
                for scale in scales {
                    if !unique.contains(&scale) {
                        unique.push(scale);
                    }
                }
                metadata.push(Property::new("viewportScales", "ビューポートの縮尺", unique.join("・")));
            }
        }
    }
    let mut bounds = empty_box();
    for entity in &builder.entities {
        union_box(&mut bounds, &entity.bbox);
    }
    if let Some(sheet) = &sheet {
        extend_box(&mut bounds, 0.0, 0.0);
        extend_box(&mut bounds, sheet.width, sheet.height);
    }
    let limit = match &sheet {
        Some(sheet) => BBox { min_x: 0.0, min_y: 0.0, max_x: sheet.width, max_y: sheet.height },
        None => bounds,
    };
    builder.finish_infinite(limit);
    for entity in &builder.entities {
        union_box(&mut bounds, &entity.bbox);
    }
    builder.finish_notices();
    let layers = builder.layers();
    Scene {
        format: doc.format.into(),
        version: format!("{} {}", format_name(doc), doc.version),
        unit: Some(length_unit),
        views: Some(views(doc)),
        view: Some(view),
        sheet,
        bounds,
        layers,
        entities: builder.entities,
        underlays: Vec::new(),
        notices: builder.notices.into_vec(),
        metadata,
        background,
    }
}

/// ビューポートの中にモデル空間を描く。モデルから紙空間への変換は ezdxf の Viewport.get_transformation_matrix と同じ
/// （表示の目標点を原点にし、縮尺 = 高さ / 表示の高さ、ねじれの角度で回し、中心をビューポートの中心へ）
#[allow(clippy::too_many_arguments)]
fn draw_viewport(
    builder: &mut Builder<'_>,
    doc: &DxfDocument,
    base: &DxfEntityBase,
    viewport: &DxfViewport,
    root: &Ctx,
    paper_matrix: &Matrix,
    unit: f64,
    paper_unit: f64,
) -> Option<String> {
    let [vx, vy, vz] = viewport.view_direction;
    let length = js::hypot(&[vx, vy, vz]);
    if length > 0.0 && (vx.abs() > 1e-9 * length || vy.abs() > 1e-9 * length || vz < 0.0) {
        builder.notices.add(
            NoticeLevel::Info,
            "viewport-view",
            "上から見た向きでないビューポート（3D の表示）は描いていません",
        );
        return None;
    }
    let (view_height, height) = (viewport.view_height, viewport.height);
    if view_height.is_nan() || view_height <= 0.0 || height.is_nan() || height <= 0.0 {
        return None;
    }
    let s = viewport.height / viewport.view_height;
    let (cx, cy) = (viewport.center[0], viewport.center[1]);
    let model = multiply(
        &multiply(
            &translate(cx - viewport.view_center[0] * s, cy - viewport.view_center[1] * s),
            &rotate(viewport.view_twist_angle),
        ),
        &multiply(&scale(s, s), &translate(-viewport.view_target[0], -viewport.view_target[1])),
    );
    let m = multiply(paper_matrix, &model);
    // 紙空間の mm → モデルの実寸の mm
    let inverse = multiply(&scale(unit, unit), &invert(&m));
    let paper_per_real = (s * paper_unit) / unit;
    let label = scale_label(paper_per_real);
    let placement = Arc::new(Placement {
        name: format!("ビューポート（{label}）"),
        scale_x: paper_per_real,
        scale_y: paper_per_real,
        inverse,
    });
    let half_width = viewport.width / 2.0;
    let half_height = viewport.height / 2.0;
    if viewport.clipping_boundary_handle.is_some() {
        builder.notices.add(NoticeLevel::Info, "viewport-clip", "矩形でないビューポートは、外接矩形で切り取りました");
    }
    let (x0, y0) = apply(paper_matrix, cx - half_width, cy - half_height);
    let (x1, y1) = apply(paper_matrix, cx + half_width, cy + half_height);
    let clip = BBox { min_x: js::min(x0, x1), min_y: js::min(y0, y1), max_x: js::max(x0, x1), max_y: js::max(y0, y1) };
    let mut frozen: HashSet<String> = HashSet::new();
    for handle in &viewport.frozen_layer_handles {
        if let Some(layer) = doc.layers.iter().find(|candidate| candidate.handle == *handle) {
            frozen.insert(layer.name.to_uppercase());
        }
    }
    frozen.extend(viewport.frozen_layer_names.iter().map(|name| name.to_uppercase()));
    let ctx = Ctx {
        m,
        local: IDENTITY,
        placement: Some(placement),
        // $PSLTSCALE が 1 なら、線種の長さは紙空間の単位（ビューポートの縮尺で割り戻す）
        dash_factor: if doc.header.psltscale == 1.0 { 1.0 / s } else { 1.0 },
        clip: Some(clip),
        frozen: Some(Rc::new(frozen)),
        path: format!(
            "{}:",
            if base.handle.is_empty() && base.ordinal == 0 { "VP".into() } else { handle_or_unknown(base) }
        ),
        ..root.clone()
    };
    let model_entities = builder.model.clone();
    for entity in model_entities.iter() {
        builder.emit(entity, &ctx);
    }
    // ビューポートの枠
    let frame = [
        cx - half_width,
        cy - half_height,
        cx + half_width,
        cy - half_height,
        cx + half_width,
        cy + half_height,
        cx - half_width,
        cy + half_height,
    ];
    let stroke = builder.stroke(base, root, &root.m, &frame, true);
    let mut properties = vec![
        Property::new("scale", "縮尺", label.clone()).raw(1.0 / paper_per_real),
        point_property(
            "viewCenter",
            "表示の中心（モデル）",
            viewport.view_target[0] + viewport.view_center[0],
            viewport.view_target[1] + viewport.view_center[1],
        ),
        number_property("viewHeight", "表示の高さ（モデル）", viewport.view_height),
    ];
    if truthy(viewport.view_twist_angle) {
        properties.push(
            Property::new("twistAngle", "ねじれの角度", format!("{}°", fmt(viewport.view_twist_angle)))
                .raw(viewport.view_twist_angle),
        );
    }
    builder.add(base, root, vec![Primitive::Stroke(stroke)], properties, None);
    Some(label)
}
