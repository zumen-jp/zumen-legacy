//! AutoCAD の図面のモデル（`DxfDocument`）を組み立てる。ハンドルを振り、レイヤ・線種・文字スタイル・
//! ブロックの名前を DXF で使える形（使えない文字を置き換え、大文字小文字を区別せずに重ならない名前）にそろえる。

use std::collections::HashMap;
use zumen_core::geometry::{apply, mean_scale};
use zumen_core::{BACKGROUND_COLOR, Matrix, NoticeLevel, NoticeList, Primitive};
use zumen_dxf::{
    ACI, DxfBlock, DxfDimStyle, DxfDocument, DxfEntity, DxfEntityBase, DxfEntityData, DxfHatch, DxfHatchPath,
    DxfHeader, DxfImage, DxfLayer, DxfLayout, DxfLinetype, DxfLwPolyline, DxfSpace, DxfTextStyle, DxfVertexList,
    EncodingSource, Vec2,
};

/// 図形を置く場所
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Space {
    Model,
    Paper,
    /// 現在のレイアウトのほかの紙空間（*Paper_Space0 など。`layout_block` で作る）
    Layout(String),
    Block(String),
}

/// 図形の色。ACI の番号と、トゥルーカラー（0xRRGGBB）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub index: f64,
    pub true_color: Option<f64>,
}

/// ACI 7（前景色）。AutoCAD は背景が黒なら白、白なら黒で描く
pub const FOREGROUND: Color = Color { index: 7.0, true_color: None };
pub const BY_BLOCK: Color = Color { index: 0.0, true_color: None };
pub const BY_LAYER: Color = Color { index: 256.0, true_color: None };

/// 線の太さ（370）の BYLAYER・BYBLOCK
pub const LINEWEIGHT_BY_LAYER: f64 = -1.0;
pub const LINEWEIGHT_BY_BLOCK: f64 = -2.0;

/// DXF で使える線の太さ（1/100 mm、DXF Reference「Common Group Codes for Entities」の 370）
const LINEWEIGHTS: [f64; 24] = [
    0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0, 106.0,
    120.0, 140.0, 158.0, 200.0, 211.0,
];

fn rgb(hex: &str) -> Option<[u8; 3]> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() < 6 || !digits.is_ascii() {
        return None;
    }
    let part = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

impl Color {
    /// #rrggbb の色。ACI 1〜9 と同じ色はその番号にし、黒と白は前景色（7）にする（SXF・Jw_cad の黒は、
    /// 白の用紙に描く既定の色）。ほかはトゥルーカラーで、ACI には最も近い色を書く
    pub fn from_hex(hex: &str) -> Color {
        let Some([r, g, b]) = rgb(hex) else {
            return FOREGROUND;
        };
        if [r, g, b] == [0, 0, 0] {
            return FOREGROUND;
        }
        let exact = |index: usize| rgb(&ACI[index]) == Some([r, g, b]);
        if let Some(index) = (1..=9).find(|index| exact(*index)) {
            return Color { index: index as f64, true_color: None };
        }
        let distance = |index: usize| {
            let [ar, ag, ab] = rgb(&ACI[index]).unwrap_or([0, 0, 0]);
            let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
            d(ar, r) + d(ag, g) + d(ab, b)
        };
        let nearest = (1..=255).min_by_key(|index| distance(*index)).unwrap_or(7);
        Color {
            index: nearest as f64,
            true_color: Some(f64::from((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))),
        }
    }
}

/// 図形の共通の値
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub layer: String,
    pub color: Color,
    /// 線種の名前（Continuous・ByBlock・ByLayer か、線種の表の名前）
    pub linetype: String,
    /// 線の太さ（1/100 mm）。-1 は BYLAYER、-2 は BYBLOCK
    pub lineweight: f64,
    pub linetype_scale: f64,
}

impl Look {
    /// ブロックの中で、配置の色・線種・線の太さを継ぐ図形
    pub fn by_block() -> Self {
        Look {
            layer: "0".into(),
            color: BY_BLOCK,
            linetype: "ByBlock".into(),
            lineweight: LINEWEIGHT_BY_BLOCK,
            linetype_scale: 1.0,
        }
    }
}

/// DXF の名前（レイヤ・線種・文字スタイル・ブロック）に使えない文字（DXF Reference と ezdxf の
/// INVALID_NAME_CHARACTERS、外部参照の区切りの |）と制御文字を _ にする。前後の空白は除く
pub fn valid_name(name: &str) -> String {
    let name: String = name
        .trim()
        .chars()
        .map(|c| if "<>/\\\":;?*|=`".contains(c) || c.is_control() { '_' } else { c })
        .take(255)
        .collect();
    name
}

/// 名前の表（大文字小文字を区別しない）
#[derive(Default)]
struct Names {
    used: HashMap<String, ()>,
}

impl Names {
    /// 名前を DXF で使える形にし、使った名前と重なれば「名前 (2)」のように番号を付ける。
    /// 2 つ目の値は、元の名前を変えたか
    fn unique(&mut self, name: &str, fallback: &str) -> (String, bool) {
        let valid = valid_name(name);
        let base = if valid.is_empty() { fallback.to_string() } else { valid };
        let mut candidate = base.clone();
        let mut number = 2;
        while self.used.contains_key(&candidate.to_uppercase()) {
            candidate = format!("{base} ({number})");
            number += 1;
        }
        self.used.insert(candidate.to_uppercase(), ());
        let changed = candidate != name;
        (candidate, changed)
    }

    fn reserve(&mut self, name: &str) {
        self.used.insert(name.to_uppercase(), ());
    }
}

/// よく使う TrueType フォントの名前とファイル（AutoCAD は STYLE の 3 にファイルの名前を持つ）
fn font_file(family: &str) -> String {
    let normalized: String = family.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect();
    let file = match normalized.to_uppercase().as_str() {
        "ＭＳゴシック" | "MSGOTHIC" | "ＭＳＰゴシック" | "MSPGOTHIC" | "MSUIGOTHIC" => "msgothic.ttc",
        "ＭＳ明朝" | "MSMINCHO" | "ＭＳＰ明朝" | "MSPMINCHO" => "msmincho.ttc",
        "CENTURY" => "century.ttf",
        _ => return family.to_string(),
    };
    file.to_string()
}

pub struct Builder {
    pub doc: DxfDocument,
    next: u64,
    pub notices: NoticeList,
    /// 図形のハンドル → 元の図形の id（SXF の #番号、JWW の #順番）
    pub sources: HashMap<String, String>,
    layer_names: Names,
    linetype_names: Names,
    style_names: Names,
    block_names: Names,
    /// (線種の名前, 線分・空白) → 書いた線種の名前
    linetypes: HashMap<(String, Vec<u64>), String>,
    /// (フォント, 縦書き) → 文字スタイルの名前
    styles: HashMap<(String, bool), String>,
    dimension_blocks: u32,
    /// 名前を変えたレイヤ・線種・文字スタイル・ブロックの数
    renamed: u64,
}

/// 文字の既定のフォント（SXF 実装規約 1-3 の MS ゴシック）
pub const DEFAULT_FONT: &str = "ＭＳ ゴシック";
pub const VIEWPORT_LAYER: &str = "VIEWPORTS";

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder {
    pub fn new() -> Self {
        let mut builder = Builder {
            doc: DxfDocument {
                format: "dxf",
                version: "AC1027".into(),
                binary: false,
                encoding: "utf-8",
                encoding_source: EncodingSource::Version,
                header: DxfHeader {
                    insunits: 4.0,
                    measurement: Some(1.0),
                    codepage: None,
                    ltscale: 1.0,
                    psltscale: 1.0,
                    textsize: Some(2.5),
                    dimscale: 1.0,
                    dimasz: 2.5,
                    dimtxt: 2.5,
                    tilemode: 0.0,
                    extmin: None,
                    extmax: None,
                    limmin: None,
                    limmax: None,
                    plimmin: None,
                    plimmax: None,
                    variables: Vec::new(),
                },
                layers: Vec::new(),
                linetypes: Vec::new(),
                styles: Vec::new(),
                dimstyles: Vec::new(),
                mline_styles: None,
                blocks: Vec::new(),
                layouts: Vec::new(),
                entities: Vec::new(),
                sort_handles: Vec::new(),
                skipped: Vec::new(),
                notices: Vec::new(),
            },
            next: 0x100,
            notices: NoticeList::new(),
            sources: HashMap::new(),
            layer_names: Names::default(),
            linetype_names: Names::default(),
            style_names: Names::default(),
            block_names: Names::default(),
            linetypes: HashMap::new(),
            styles: HashMap::new(),
            dimension_blocks: 0,
            renamed: 0,
        };
        for name in ["ByBlock", "ByLayer", "Continuous"] {
            builder.linetype_names.reserve(name);
        }
        builder.layer("0", false, false, true);
        // ビューポートの枠は、ezdxf と同じく VIEWPORTS のレイヤに置き、表示も印刷もしない（中のモデル空間は見える）。
        // 元の図面のレイヤと名前が重ならないよう先に作る
        builder.layer(VIEWPORT_LAYER, true, false, false);
        builder.style(DEFAULT_FONT, false);
        let handle = builder.handle();
        builder.doc.dimstyles.push(DxfDimStyle {
            handle,
            name: "Standard".into(),
            dimscale: 1.0,
            dimasz: 2.5,
            dimtxt: 2.5,
        });
        for name in ["*Model_Space", "*Paper_Space"] {
            builder.block_names.reserve(name);
            let (handle, record) = (builder.handle(), builder.handle());
            builder.doc.blocks.push(DxfBlock {
                handle,
                name: name.into(),
                base_point: [0.0; 3],
                flags: 0.0,
                anonymous: false,
                layout: true,
                block_record_handle: Some(record),
                xref_path: None,
                description: None,
            });
        }
        builder
    }

    pub fn handle(&mut self) -> String {
        let handle = format!("{:X}", self.next);
        self.next += 1;
        handle
    }

    fn renamed(&mut self, changed: bool) {
        if changed {
            self.renamed += 1;
        }
    }

    /// レイヤを加え、書いた名前を返す
    pub fn layer(&mut self, name: &str, off: bool, locked: bool, plot: bool) -> String {
        let (name, changed) = self.layer_names.unique(name, "レイヤ");
        self.renamed(changed);
        let handle = self.handle();
        self.doc.layers.push(DxfLayer {
            handle,
            name: name.clone(),
            color_index: 7.0,
            true_color: None,
            frozen: false,
            off,
            locked,
            plot,
            linetype: "Continuous".into(),
            lineweight: -3.0,
            transparency: None,
        });
        name
    }

    /// 線種（線分・空白の長さは用紙の mm）。実線は Continuous。同じ名前・同じ長さの線種は 1 つにする
    pub fn linetype(&mut self, name: &str, description: &str, dashes: &[f64]) -> String {
        if dashes.is_empty() {
            return "Continuous".into();
        }
        let key = (name.to_string(), dashes.iter().map(|value| value.to_bits()).collect::<Vec<_>>());
        if let Some(name) = self.linetypes.get(&key) {
            return name.clone();
        }
        let (written, changed) = self.linetype_names.unique(name, "線種");
        self.renamed(changed);
        let handle = self.handle();
        // 線分は正、空白は負（DXF Reference「LTYPE」の 49）
        let pattern: Vec<f64> =
            dashes.iter().enumerate().map(|(index, value)| if index % 2 == 0 { *value } else { -value }).collect();
        self.doc.linetypes.push(DxfLinetype {
            handle,
            name: written.clone(),
            description: description.to_string(),
            pattern_length: dashes.iter().sum(),
            dashes: pattern,
            complex: false,
        });
        self.linetypes.insert(key, written.clone());
        written
    }

    /// フォントの文字スタイル。縦書きは、Windows の縦書きのフォント（名前の先頭が @）を使う
    pub fn style(&mut self, font: &str, vertical: bool) -> String {
        let family = if font.trim().is_empty() { DEFAULT_FONT } else { font.trim() };
        let key = (family.to_string(), vertical);
        if let Some(name) = self.styles.get(&key) {
            return name.clone();
        }
        let family_name = if vertical { format!("@{family}") } else { family.to_string() };
        let wanted = if family == DEFAULT_FONT && !vertical { "Standard".to_string() } else { family_name.clone() };
        let (name, changed) = self.style_names.unique(&wanted, "Standard");
        self.renamed(changed && wanted != family_name);
        let handle = self.handle();
        self.doc.styles.push(DxfTextStyle {
            handle,
            name: name.clone(),
            font: font_file(family),
            bigfont: String::new(),
            height: 0.0,
            width: 1.0,
            oblique: 0.0,
            flags: 0.0,
            generation_flags: 0.0,
            font_family: Some(family_name),
        });
        self.styles.insert(key, name.clone());
        name
    }

    /// ブロック定義を加え、書いた名前を返す
    pub fn block(&mut self, name: &str) -> String {
        let (name, changed) = self.block_names.unique(name, "ブロック");
        self.renamed(changed);
        self.push_block(&name, false);
        name
    }

    /// 寸法の表示のブロック（*D1、*D2 …）
    pub fn dimension_block(&mut self) -> String {
        loop {
            self.dimension_blocks += 1;
            let name = format!("*D{}", self.dimension_blocks);
            if !self.block_names.used.contains_key(&name.to_uppercase()) {
                self.block_names.reserve(&name);
                self.push_block(&name, true);
                return name;
            }
        }
    }

    /// 2 つ目からのレイアウトの紙空間のブロック（*Paper_Space0、*Paper_Space1 …）を加え、名前を返す
    pub fn layout_block(&mut self) -> String {
        let mut number = 0;
        loop {
            let name = format!("*Paper_Space{number}");
            if !self.block_names.used.contains_key(&name.to_uppercase()) {
                self.block_names.reserve(&name);
                let (handle, record) = (self.handle(), self.handle());
                self.doc.blocks.push(DxfBlock {
                    handle,
                    name: name.clone(),
                    base_point: [0.0; 3],
                    flags: 0.0,
                    anonymous: false,
                    layout: true,
                    block_record_handle: Some(record),
                    xref_path: None,
                    description: None,
                });
                return name;
            }
            number += 1;
        }
    }

    fn push_block(&mut self, name: &str, anonymous: bool) {
        let (handle, record) = (self.handle(), self.handle());
        self.doc.blocks.push(DxfBlock {
            handle,
            name: name.to_string(),
            base_point: [0.0; 3],
            flags: if anonymous { 1.0 } else { 0.0 },
            anonymous,
            layout: false,
            block_record_handle: Some(record),
            xref_path: None,
            description: None,
        });
    }

    /// 名前を変えたことを知らせる（最後に 1 度）
    pub fn report_renamed(&mut self) {
        if self.renamed > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "name",
                "DXF の名前に使えない文字（< > / \\ \" : ; ? * | = `）を _ にし、重なる名前に番号を付けました",
                self.renamed,
            );
        }
    }

    /// 図形を加え、ハンドルを返す。source は元の図形の id（ブロックの中の部品など、元の図形にあたらないものは空）
    pub fn add(&mut self, space: &Space, look: &Look, kind: &str, data: DxfEntityData, source: &str) -> String {
        let handle = self.handle();
        let (space_kind, block) = match space {
            Space::Model => (DxfSpace::Model, None),
            Space::Paper => (DxfSpace::Paper, Some("*Paper_Space".to_string())),
            Space::Layout(name) => (DxfSpace::Paper, Some(name.clone())),
            Space::Block(name) => (DxfSpace::Block, Some(name.clone())),
        };
        self.doc.entities.push(DxfEntity {
            base: DxfEntityBase {
                kind: kind.to_string(),
                handle: handle.clone(),
                ordinal: 0,
                owner: None,
                space: space_kind,
                block,
                layer: look.layer.clone(),
                linetype: look.linetype.clone(),
                color_index: look.color.index,
                true_color: look.color.true_color,
                transparency: None,
                lineweight: look.lineweight,
                linetype_scale: look.linetype_scale,
                invisible: false,
                thickness: None,
                extrusion: None,
            },
            data,
        });
        self.sources.insert(handle.clone(), source.to_string());
        handle
    }

    /// 用紙の座標で作った矢印・点マーカの図形（塗り・線・点）を、to_container で写して書く。
    /// 背景色の塗りは WIPEOUT、3〜4 点の塗りは SOLID、それより多い点の塗りは HATCH にする
    pub fn primitives(
        &mut self,
        space: &Space,
        look: &Look,
        primitives: &[Primitive],
        to_container: &Matrix,
        color: Color,
        source: &str,
    ) {
        let point = |x: f64, y: f64| apply(to_container, x, y);
        for primitive in primitives {
            match primitive {
                Primitive::Stroke(stroke) => {
                    let points: Vec<Vec2> =
                        stroke.points.as_chunks::<2>().0.iter().map(|[x, y]| point(*x, *y).into()).collect();
                    let look = Look {
                        color,
                        linetype: "Continuous".into(),
                        lineweight: lineweight(stroke.width).0,
                        ..look.clone()
                    };
                    if points.len() == 2 && !stroke.closed {
                        let data = DxfEntityData::Line { start: xyz(points[0]), end: xyz(points[1]) };
                        self.add(space, &look, "LINE", data, source);
                    } else if points.len() >= 2 {
                        self.add(space, &look, "LWPOLYLINE", lwpolyline(&points, None, stroke.closed), source);
                    }
                }
                Primitive::Fill(fill) => {
                    for ring in &fill.rings {
                        let points: Vec<Vec2> =
                            ring.as_chunks::<2>().0.iter().map(|[x, y]| point(*x, *y).into()).collect();
                        if points.len() < 3 {
                            continue;
                        }
                        if fill.color == BACKGROUND_COLOR {
                            let look = Look { color: BY_LAYER, ..look.clone() };
                            self.add(space, &look, "WIPEOUT", wipeout(&points), source);
                            continue;
                        }
                        let look = Look { color, ..look.clone() };
                        if points.len() <= 4 {
                            // SOLID は 1 → 2 → 4 → 3 の順に塗る（DXF Reference「SOLID」）
                            let last = points[points.len() - 1];
                            let data = DxfEntityData::Solid {
                                vtx0: xyz(points[0]),
                                vtx1: xyz(points[1]),
                                vtx2: xyz(last),
                                vtx3: xyz(points[2]),
                                invisible_edges: None,
                            };
                            self.add(space, &look, "SOLID", data, source);
                        } else {
                            let path = polyline_path(&points, None, 1.0);
                            self.add(space, &look, "HATCH", solid_hatch(vec![path]), source);
                        }
                    }
                }
                Primitive::Dot(dot) => {
                    let (x, y) = point(dot.x, dot.y);
                    let look = Look { color, ..look.clone() };
                    let data = DxfEntityData::Point { location: [x, y, 0.0], x_axis_angle: 0.0 };
                    self.add(space, &look, "POINT", data, source);
                }
                Primitive::Text(_) | Primitive::Clipped(_) => {}
            }
        }
    }
}

/// 線の太さ（mm）を、DXF で使えるいちばん近い太さ（1/100 mm）にする。2 つ目の値は丸めたか
pub fn lineweight(width: f64) -> (f64, bool) {
    if !width.is_finite() || width < 0.0 {
        return (0.0, true);
    }
    let hundredths = width * 100.0;
    let nearest = LINEWEIGHTS
        .iter()
        .copied()
        .min_by(|a, b| (a - hundredths).abs().total_cmp(&(b - hundredths).abs()))
        .unwrap_or(0.0);
    (nearest, (nearest - hundredths).abs() > 1e-6)
}

pub fn xyz(point: Vec2) -> [f64; 3] {
    [point[0], point[1], 0.0]
}

/// 開いた・閉じた LWPOLYLINE。bulges は頂点から次の頂点への弧の膨らみ
pub fn lwpolyline(points: &[Vec2], bulges: Option<Vec<f64>>, closed: bool) -> DxfEntityData {
    DxfEntityData::LwPolyline(DxfLwPolyline {
        vertices: DxfVertexList {
            points: points.iter().map(|point| xyz(*point)).collect(),
            bulges: bulges.unwrap_or_else(|| vec![0.0; points.len()]),
            widths: vec![[0.0, 0.0]; points.len()],
            closed,
            flags: if closed { 1.0 } else { 0.0 },
        },
        const_width: None,
        elevation: None,
    })
}

/// ハッチングの多角形の境界（頂点と弧の膨らみ）。flags は 1 外側、0 中抜き
pub fn polyline_path(points: &[Vec2], bulges: Option<Vec<f64>>, flags: f64) -> DxfHatchPath {
    DxfHatchPath {
        flags: flags + 2.0,
        closed: true,
        points: points.iter().map(|point| xyz(*point)).collect(),
        bulges,
        edges: None,
    }
}

pub fn solid_hatch(paths: Vec<DxfHatchPath>) -> DxfEntityData {
    DxfEntityData::Hatch(DxfHatch {
        pattern_name: "SOLID".into(),
        solid_fill: true,
        associative: false,
        hatch_style: 0.0,
        pattern_type: 1.0,
        pattern_angle: 0.0,
        pattern_scale: 1.0,
        pattern_double: false,
        elevation: 0.0,
        paths,
        pattern_lines: Vec::new(),
        gradient: None,
        gradient_colors: None,
        gradient_angle: 0.0,
        gradient_shift: 0.0,
        gradient_one_color: false,
        gradient_tint: 0.0,
    })
}

/// 多角形を背景色で塗るワイプアウト（DXF Reference「WIPEOUT」）。画素の座標は ezdxf の
/// Wipeout.set_masking_area と同じく、外接矩形を 1 画素とし、中心を原点、Y を下向きにする
pub fn wipeout(points: &[Vec2]) -> DxfEntityData {
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for point in points {
        for axis in 0..2 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    let width = (max[0] - min[0]).max(1e-9);
    let height = (max[1] - min[1]).max(1e-9);
    let mut boundary: Vec<Vec2> =
        points.iter().map(|[x, y]| [(x - min[0]) / width - 0.5, 0.5 - (y - min[1]) / height]).collect();
    boundary.push(boundary[0]);
    DxfEntityData::Image(DxfImage {
        insert: [min[0], min[1], 0.0],
        u_pixel: [width, 0.0, 0.0],
        v_pixel: [0.0, height, 0.0],
        image_size: [1.0, 1.0],
        boundary,
        flags: 7.0,
        clipping: true,
    })
}

/// 変換が長さを何倍にするか（0 や NaN なら 1）
pub fn scale_of(m: &Matrix) -> f64 {
    let scale = mean_scale(m);
    if scale.is_finite() && scale > 0.0 { scale } else { 1.0 }
}

/// 図面のモデルに、用紙のレイアウトを加える
pub fn add_layouts(builder: &mut Builder, name: &str, size_name: &str, width: f64, height: f64) {
    let model = builder.handle();
    let paper = builder.handle();
    let record = |builder: &Builder, block: &str| {
        builder.doc.blocks.iter().find(|candidate| candidate.name == block).and_then(|b| b.block_record_handle.clone())
    };
    let model_record = record(builder, "*Model_Space");
    let paper_record = record(builder, "*Paper_Space");
    let layout = |handle: String, name: &str, model: bool, block: &str, record: Option<String>| DxfLayout {
        handle,
        name: name.to_string(),
        tab_order: if model { 0.0 } else { 1.0 },
        model,
        active: !model,
        block: block.to_string(),
        block_record_handle: record,
        paper_width: if model { 0.0 } else { width },
        paper_height: if model { 0.0 } else { height },
        margins: [0.0; 4],
        paper_size: if model { String::new() } else { size_name.to_string() },
        paper_units: 1.0,
        plot_rotation: 0.0,
        plot_scale: [1.0, 1.0],
        plot_origin: [0.0, 0.0],
        limmin: [0.0, 0.0],
        limmax: if model { [0.0, 0.0] } else { [width, height] },
    };
    let model_layout = layout(model, "Model", true, "*Model_Space", model_record);
    let paper_layout = layout(paper, name, false, "*Paper_Space", paper_record);
    builder.doc.layouts = vec![model_layout, paper_layout];
}
