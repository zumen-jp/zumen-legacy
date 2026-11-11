//! SXF の図面（`SxfDrawing`）を AutoCAD の図面のモデルに写す（docs/formats/sxf.md「Converting to DXF」）。
//!
//! - 用紙に配置した部分図（数学・測地座標系、縦横の縮尺が同じもの）は、モデル空間に実寸で置き、用紙のレイアウトに
//!   その配置の位置・縮尺・回転のビューポートを置く（決定 0008）。測地座標系（X が北、Y が東）の部分図は、表示と同じく
//!   X と Y を入れ替えて表示の向きにする（円弧の角度・文字の向きは入れ替えない）。
//! - 用紙にじかに描いた要素は紙空間に置く。部分図がない図面は、用紙の座標のままモデル空間に置き、1:1 の
//!   ビューポートで見せる。
//! - 作図部品・作図グループ・縦横の縮尺が違う部分図は、ブロック（複合図形定義）とその配置（INSERT）にする。
//!   作図グループは配置値を使わない（フィーチャ仕様編 3-5 補足）。画像（ラスタ）の作図グループのブロックには、
//!   範囲の折線の前に、外形を表示しない IMAGE を置く。
//! - 矢印・点マーカの大きさと線種のピッチは用紙の mm で、配置の縮尺の影響を受けない（SXF 実装規約 1-4・1-6）。
//!   ブロックの中では、最初の配置の縮尺で割り戻して書く。
//!
//! 表示（zumen-sxf の scene.rs）と同じ順・同じ規則で辿り、同じ所に同じ形を描く DXF にする。

use crate::Mapped;
use crate::builder::{
    BY_BLOCK, BY_LAYER, Builder, Color, DEFAULT_FONT, LINEWEIGHT_BY_LAYER, Look, Space, image, lineweight, lwpolyline,
    polyline_path, scale_of, solid_hatch, xyz,
};
use crate::sheet::{Content, View, boxes_by_top_id, overall_viewport, rect_of, region, separate, viewport};
use crate::text::text_entity;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use zumen_core::geometry::{
    IDENTITY, apply, apply_vector, arc_segments, bezier_points, clothoid_points, ellipse_arc_points, empty_box,
    extend_box_with_points, invert, mean_scale, multiply, placement, rotation_deg, sweep_radians, union_box,
};
use zumen_core::text::{TextLayoutInput, font_family_for, layout_text};
use zumen_core::{BACKGROUND_COLOR, BBox, Matrix, NoticeLevel, Primitive, js};
use zumen_dxf::{DxfDimension, DxfEntityData, DxfHatch, DxfHatchPath, DxfPatternLine, DxfSpline, Vec2};
use zumen_sxf::tables::{PREDEFINED_LINE_TYPES, display_name, index, sheet_size};
use zumen_sxf::{
    Annotation, Arrow, ExtensionLine, HatchPattern, Region, SxfDocument, SxfDrawing, SxfFeature, arrow_primitives,
    marker_primitives, raster_file, sxf_to_scene,
};

/// 配置の入れ子の上限（表示と同じ）
const MAX_DEPTH: usize = 32;
/// 1 つの模様のハッチング線の上限（表示と同じ）
const MAX_HATCH_LINES: f64 = 20_000.0;
/// Scene のレイヤの id の上限
const MAX_LAYER_CODE: f64 = 4_294_967_295.0;
/// 表示が円弧を折線にする許容誤差（用紙の mm）
const TOLERANCE: f64 = 0.02;

/// SXF の図面を AutoCAD の図面のモデルに写す
pub fn sxf_to_dxf(drawing: &SxfDrawing) -> Mapped {
    Mapper::new(drawing).run()
}

/// 要素を置く入れ物（用紙、モデル空間のまとまり、ブロック）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Sheet,
    Content(usize),
    Block(String),
}

/// 入れ物の座標
#[derive(Debug, Clone)]
struct Frame {
    key: Key,
    space: Space,
    /// 測地座標系（X と Y を入れ替えて表示の向きにする）
    geodetic: bool,
    /// 表示の向きにした元の座標 → 入れ物の座標
    to_container: Matrix,
    /// 入れ物の座標 → 用紙（矢印・点マーカの大きさ、線種のピッチ）
    to_paper: Matrix,
    /// 線種の尺度（ブロックの中では配置の縮尺の逆数）
    linetype_scale: f64,
}

impl Frame {
    /// 表示の向きにした元の座標 → 用紙
    fn to_sheet(&self) -> Matrix {
        multiply(&self.to_paper, &self.to_container)
    }

    /// 元の座標を表示の向きにする
    fn local(&self, x: f64, y: f64) -> (f64, f64) {
        if self.geodetic { (y, x) } else { (x, y) }
    }

    /// 元の座標 → 入れ物の座標
    fn point(&self, x: f64, y: f64) -> Vec2 {
        let (lx, ly) = self.local(x, y);
        let (px, py) = apply(&self.to_container, lx, ly);
        [px, py]
    }

    /// 入れ物の座標での長さの倍率
    fn scale(&self) -> f64 {
        scale_of(&self.to_container)
    }
}

/// ブロックにする複合図形の、最初の配置の値
#[derive(Debug, Clone)]
struct BlockUse {
    name: String,
    geodetic: bool,
    /// ブロックの座標（表示の向き）→ 用紙
    to_paper: Matrix,
    /// 配置の縮尺の積（モデル空間・紙空間から）
    scale: f64,
}

/// 部分図を置くビューポート
#[derive(Debug, Clone)]
struct ViewPlan {
    content: usize,
    scale: f64,
    angle: f64,
    origin: [f64; 2],
    rect: BBox,
}

/// モデル空間に置く部分図
#[derive(Debug, Clone)]
struct ContentPlan {
    name: String,
    geodetic: bool,
    /// 部分図の座標（表示の向き）→ 用紙（最初のビューポート）
    to_paper: Matrix,
    weight: usize,
}

/// SXF の図面を写す途中の値
struct Mapper<'a> {
    doc: &'a SxfDocument,
    out: Builder,
    layers: Vec<String>,
    undefined_layers: HashMap<u64, String>,
    boxes: HashMap<String, (BBox, usize)>,
    /// 用紙の要素の番号 → ビューポート
    views: HashMap<usize, usize>,
    plans: Vec<ViewPlan>,
    contents: Vec<ContentPlan>,
    content_index: HashMap<String, usize>,
    blocks: HashMap<String, BlockUse>,
    block_order: Vec<String>,
    /// 書かない配置（入れ物、要素の番号）。定義がない・循環する・縮尺が 0
    cuts: HashSet<(Key, usize)>,
    /// ビューポートの範囲のない部分図の配置（描く図形がない）
    empty_views: HashSet<usize>,
}

/// 辿るときの文脈（表示の Context と同じ）
#[derive(Debug, Clone)]
struct Walk {
    m: Matrix,
    geodetic: bool,
    stack: Vec<String>,
    scale: f64,
}

impl<'a> Mapper<'a> {
    fn new(drawing: &'a SxfDrawing) -> Self {
        let scene = sxf_to_scene(drawing);
        Mapper {
            doc: &drawing.document,
            out: Builder::new(),
            layers: Vec::new(),
            undefined_layers: HashMap::new(),
            boxes: boxes_by_top_id(&scene),
            views: HashMap::new(),
            plans: Vec::new(),
            contents: Vec::new(),
            content_index: HashMap::new(),
            blocks: HashMap::new(),
            block_order: Vec::new(),
            cuts: HashSet::new(),
            empty_views: HashSet::new(),
        }
    }

    fn run(mut self) -> Mapped {
        let doc = self.doc;
        for (index, layer) in doc.layers.iter().enumerate() {
            let name = if layer.name.is_empty() { format!("レイヤ {}", index + 1) } else { layer.name.clone() };
            let written = self.out.layer(&name, !layer.visible, false, true);
            self.layers.push(written);
        }
        self.report_document();
        let root = Walk { m: IDENTITY, geodetic: false, stack: Vec::new(), scale: 1.0 };
        self.walk(&doc.sheet_members, &Key::Sheet, &root);

        let (width, height, sheet_name, size_name) = self.sheet();
        let has_views = !self.plans.is_empty();
        // ビューポート（モデル空間のまとまりの置き場所を先に決める）
        let mut views: Vec<View> = Vec::new();
        let mut contents: Vec<Content> =
            self.contents.iter().map(|content| Content { weight: content.weight, offset: [0.0, 0.0] }).collect();
        let mut extents = empty_box();
        if has_views {
            let plans: Vec<View> = self
                .plans
                .iter()
                .map(|plan| View {
                    content: plan.content,
                    scale: plan.scale,
                    angle: plan.angle,
                    origin: plan.origin,
                    rect: plan.rect,
                })
                .collect();
            let moved = separate(&plans, &mut contents);
            if moved > 0 {
                self.out.notices.add_count(
                    NoticeLevel::Info,
                    "view-moved",
                    "縮尺の違う部分図がモデル空間で重ならないよう、ずらして置きました（ずらした部分図の座標は元と変わります）",
                    moved,
                );
            }
            views = plans;
            overall_viewport(&mut self.out, width, height);
            let sheet = Frame {
                key: Key::Sheet,
                space: Space::Paper,
                geodetic: false,
                to_container: IDENTITY,
                to_paper: IDENTITY,
                linetype_scale: 1.0,
            };
            self.emit_members(&doc.sheet_members, &sheet, &views, &contents);
            for index in 0..self.contents.len() {
                let content = self.contents[index].clone();
                let offset = contents[index].offset;
                let frame = Frame {
                    key: Key::Content(index),
                    space: Space::Model,
                    geodetic: content.geodetic,
                    to_container: [1.0, 0.0, 0.0, 1.0, offset[0], offset[1]],
                    to_paper: multiply(&content.to_paper, &[1.0, 0.0, 0.0, 1.0, -offset[0], -offset[1]]),
                    linetype_scale: 1.0,
                };
                let figure = doc.sub_figures.get(&content.name).expect("配置した部分図");
                self.emit_members(&figure.members, &frame, &views, &contents);
            }
            for view in &views {
                let offset = contents[view.content].offset;
                union_box(&mut extents, &region(view, offset));
            }
        } else {
            // 部分図のない図面: 用紙の座標のままモデル空間に置き、1:1 のビューポートで見せる
            let model = Frame {
                key: Key::Sheet,
                space: Space::Model,
                geodetic: false,
                to_container: IDENTITY,
                to_paper: IDENTITY,
                linetype_scale: 1.0,
            };
            self.emit_members(&doc.sheet_members, &model, &views, &contents);
            let mut bounds = empty_box();
            for (bbox, _) in self.boxes.values() {
                union_box(&mut bounds, bbox);
            }
            if let Some(rect) = rect_of(&bounds) {
                overall_viewport(&mut self.out, width, height);
                let view = View { content: 0, scale: 1.0, angle: 0.0, origin: [0.0, 0.0], rect };
                add_viewport(&mut self.out, &view, [0.0, 0.0], 2.0, "");
                extents = rect;
            }
        }
        for name in self.block_order.clone() {
            let block = self.blocks[&name].clone();
            let figure = doc.sub_figures.get(&name).expect("配置した複合図形");
            let frame = Frame {
                key: Key::Block(name.clone()),
                space: Space::Block(block.name.clone()),
                geodetic: block.geodetic,
                to_container: IDENTITY,
                to_paper: block.to_paper,
                linetype_scale: 1.0 / block.scale,
            };
            match raster_file(&name) {
                Some(file) => self.raster(&figure.members, &file, &frame, &views, &contents),
                None => self.emit_members(&figure.members, &frame, &views, &contents),
            }
        }
        finish_header(&mut self.out, &extents, width, height);
        self.out.report_renamed();
        crate::builder::add_layouts(&mut self.out, &sheet_name, &size_name, width, height);
        Mapped { document: self.out.doc, notices: self.out.notices.into_vec(), sources: self.out.sources }
    }

    /// 用紙の大きさ（mm）とレイアウト・用紙の名前。用紙がなければ、描いた図形の範囲を覆う大きさ
    fn sheet(&self) -> (f64, f64, String, String) {
        if let Some(feature) = &self.doc.sheet {
            let (label, width, height) = match sheet_size(feature.sheet_type) {
                Some((label, width, height)) => {
                    let landscape = feature.orient != 0.0;
                    let size = format!("{label}{}", if landscape { " 横" } else { " 縦" });
                    (size, if landscape { height } else { width }, if landscape { width } else { height })
                }
                None => ("自由".to_string(), js::max(feature.x, 1.0), js::max(feature.y, 1.0)),
            };
            let name = if js::trim(&feature.name).is_empty() { label.clone() } else { feature.name.clone() };
            let media = match sheet_size(feature.sheet_type) {
                Some((series, _, _)) => format!("ISO_full_bleed_{series}_({width:.2}_x_{height:.2}_MM)"),
                None => format!("USER_({width:.2}_x_{height:.2}_MM)"),
            };
            return (width, height, layout_name(&name), media);
        }
        let mut bounds = empty_box();
        for (bbox, _) in self.boxes.values() {
            union_box(&mut bounds, bbox);
        }
        let width = if bounds.max_x.is_finite() { js::max(bounds.max_x, 1.0) } else { 1.0 };
        let height = if bounds.max_y.is_finite() { js::max(bounds.max_y, 1.0) } else { 1.0 };
        (width, height, "Layout1".into(), format!("USER_({width:.2}_x_{height:.2}_MM)"))
    }

    /// 図面の情報のうち、DXF に書く場所のないもの
    fn report_document(&mut self) {
        let doc = self.doc;
        if doc.title.as_ref().is_some_and(|title| title.values.iter().any(|value| !js::trim(&value.value).is_empty())) {
            self.out.notices.add(
                NoticeLevel::Info,
                "title",
                "図面表題欄（事業名・図面名・尺度など）は、DXF に書く場所がないため書いていません",
            );
        }
        for figure in doc.sub_figures.values() {
            if figure.feature.name.contains("$$背景色$$") {
                self.out.notices.add(
                    NoticeLevel::Info,
                    "background",
                    "背景色の指定は、DXF に書く場所がないため書いていません",
                );
            }
        }
    }

    // --- 辿る（表示の locate と同じ判定） ---------------------------------------------------------------

    fn cut(&mut self, key: &Key, index: usize, code: &str, message: &str) {
        self.cuts.insert((key.clone(), index));
        self.out.notices.add(NoticeLevel::Warning, code, message);
    }

    fn walk(&mut self, members: &[SxfFeature], key: &Key, ctx: &Walk) {
        let doc = self.doc;
        for (index, feature) in members.iter().enumerate() {
            let SxfFeature::SfigLocate { id, name, x, y, angle, ratio_x, ratio_y, .. } = feature else {
                continue;
            };
            let Some(figure) = doc.sub_figures.get(name) else {
                self.cut(key, index, "missing-sfig", "定義のない複合図形の配置を省きました");
                continue;
            };
            if ctx.stack.contains(name) || ctx.stack.len() >= MAX_DEPTH {
                self.cut(key, index, "sfig-cycle", "循環または深すぎる複合図形の配置を省きました");
                continue;
            }
            let flag = figure.feature.flag;
            let (lx, ly) = if ctx.geodetic { (*y, *x) } else { (*x, *y) };
            let local = if flag == 3.0 { IDENTITY } else { placement(lx, ly, *angle, *ratio_x, *ratio_y) };
            let m = multiply(&ctx.m, &local);
            let local_scale = mean_scale(&local);
            if flag != 3.0 && (local_scale.is_nan() || local_scale <= 0.0) {
                self.cut(key, index, "sfig-scale", "縮尺が 0 の複合図形の配置を省きました");
                continue;
            }
            let geodetic = if flag == 2.0 {
                true
            } else if flag == 1.0 {
                false
            } else {
                ctx.geodetic
            };
            let mut stack = ctx.stack.clone();
            stack.push(name.clone());
            let uniform = ratio_x.is_finite()
                && *ratio_x > 0.0
                && (ratio_x - ratio_y).abs() <= 1e-12 * ratio_x.abs().max(ratio_y.abs());
            if *key == Key::Sheet && (flag == 1.0 || flag == 2.0) && uniform {
                let top = format!("#{}", js::number_to_string(*id));
                let Some((bbox, weight)) = self.boxes.get(&top).copied() else {
                    // 描く図形のない部分図（ビューポートを置かない）
                    self.empty_views.insert(index);
                    continue;
                };
                let rect = rect_of(&bbox).expect("描いた範囲");
                let content = match self.content_index.get(name) {
                    Some(content) => {
                        self.contents[*content].weight += weight;
                        *content
                    }
                    None => {
                        let content = self.contents.len();
                        self.contents.push(ContentPlan { name: name.clone(), geodetic, to_paper: m, weight });
                        self.content_index.insert(name.clone(), content);
                        let child = Walk { m, geodetic, stack, scale: 1.0 };
                        self.walk(&figure.members, &Key::Content(content), &child);
                        content
                    }
                };
                self.views.insert(index, self.plans.len());
                self.plans.push(ViewPlan { content, scale: *ratio_x, angle: *angle, origin: [lx, ly], rect });
                continue;
            }
            if *key == Key::Sheet && (flag == 1.0 || flag == 2.0) {
                self.out.notices.add(
                    NoticeLevel::Info,
                    "sfig-insert",
                    "縦横の縮尺が違う・裏返しの部分図は、ビューポートにできないため、紙空間のブロック参照（INSERT）にしました",
                );
            }
            let scale = ctx.scale * if flag == 3.0 { 1.0 } else { local_scale };
            match self.blocks.get(name) {
                Some(used) => {
                    let differs = used.geodetic != geodetic
                        || (used.scale - scale).abs() > 1e-9 * used.scale.abs().max(scale.abs())
                        || (rotation_deg(&used.to_paper) - rotation_deg(&m)).abs() > 1e-9;
                    if differs {
                        self.out.notices.add(
                            NoticeLevel::Info,
                            "block-scale",
                            "縮尺・向きの違う配置から使われる複合図形の、矢印・点マーカの大きさと線種のピッチは、最初の配置に合わせました",
                        );
                    }
                }
                None => {
                    let block_name = self.out.block(display_name(name));
                    self.blocks.insert(
                        name.clone(),
                        BlockUse { name: block_name, geodetic, to_paper: m, scale: js::max(scale, 1e-12) },
                    );
                    self.block_order.push(name.clone());
                    let child = Walk { m, geodetic, stack, scale };
                    self.walk(&figure.members, &Key::Block(name.clone()), &child);
                }
            }
        }
    }

    // --- 書く --------------------------------------------------------------------------------------

    fn emit_members(&mut self, members: &[SxfFeature], frame: &Frame, views: &[View], contents: &[Content]) {
        for (index, feature) in members.iter().enumerate() {
            self.emit(feature, index, frame, views, contents);
        }
    }

    /// 画像（ラスタ）の作図グループのブロック。表示の raster と同じく、グループの中の 4 点以上の最初の折線を画像の範囲
    /// とし、その折線の前に、画像の定義がそのファイルを指す IMAGE を置く（AutoCAD は画像を描く順に描くので、前の図形を
    /// 隠し、範囲の折線は隠さない）。折線は左下の角から回り、2 番目の点が上（時計回り）なら 4 番目の点が右、そうで
    /// なければ 2 番目の点が右。画素数は用紙の 0.01 mm を 1 画素とした数（docs/decisions/0031）。外形は折線が描くので、
    /// 画像の外形は表示しない。レイヤと元の id は折線のもの
    fn raster(&mut self, members: &[SxfFeature], file: &str, frame: &Frame, views: &[View], contents: &[Content]) {
        let outline = members
            .iter()
            .position(|member| matches!(member, SxfFeature::Polyline { xs, ys, .. } if xs.len() >= 4 && ys.len() >= 4));
        if outline.is_none() {
            self.out.notices.add(
                NoticeLevel::Info,
                "raster",
                "画像の範囲を示す折線のない画像（ラスタ）は書いていません",
            );
        }
        for (index, feature) in members.iter().enumerate() {
            if Some(index) == outline
                && let SxfFeature::Polyline { style, xs, ys } = feature
            {
                self.raster_image(file, style.layer, xs, ys, frame, feature.id());
            }
            self.emit(feature, index, frame, views, contents);
        }
    }

    /// 範囲の折線の点（元の座標）から、画像の定義がそのファイルを指す IMAGE を書く
    fn raster_image(&mut self, file: &str, layer: f64, xs: &[f64], ys: &[f64], frame: &Frame, id: f64) {
        let corner = |i: usize| frame.local(xs[i], ys[i]);
        let (x0, y0) = corner(0);
        let (x1, y1) = corner(1);
        let (x3, y3) = corner(3);
        let cross = (x1 - x0) * (y3 - y0) - (y1 - y0) * (x3 - x0);
        let (right, up) = if cross < 0.0 {
            ((x3 - x0, y3 - y0), (x1 - x0, y1 - y0))
        } else {
            ((x1 - x0, y1 - y0), (x3 - x0, y3 - y0))
        };
        let to_sheet = frame.to_sheet();
        let on_sheet = |(x, y): (f64, f64)| {
            let (px, py) = apply_vector(&to_sheet, x, y);
            js::hypot2(px, py)
        };
        let (width, height) = (on_sheet(right), on_sheet(up));
        if !(cross != 0.0 && width > 0.0 && height > 0.0 && (width * height).is_finite()) {
            self.out.notices.add(
                NoticeLevel::Info,
                "raster",
                "画像の範囲を示す折線のない画像（ラスタ）は書いていません",
            );
            return;
        }
        let (rx, ry) = apply_vector(&frame.to_container, right.0, right.1);
        let (ux, uy) = apply_vector(&frame.to_container, up.0, up.1);
        let (px, py) = apply(&frame.to_container, x0, y0);
        let look = Look {
            layer: self.layer(layer),
            color: BY_LAYER,
            linetype: "ByLayer".into(),
            lineweight: LINEWEIGHT_BY_LAYER,
            linetype_scale: 1.0,
        };
        let data = image(file, [px, py], [rx, ry], [ux, uy], [width, height]);
        let source = format!("#{}", js::number_to_string(id));
        self.out.add_image(&frame.space, &look, data, &source);
    }

    fn layer(&mut self, code: f64) -> String {
        if code > MAX_LAYER_CODE {
            self.out.notices.add(
                NoticeLevel::Warning,
                "layer-range",
                "範囲外のレイヤコードの要素を、レイヤ 0 に書きました",
            );
            return "0".into();
        }
        if code.is_nan() || code <= 0.0 {
            return "0".into();
        }
        if let Some(name) = index(code - 1.0).and_then(|index| self.layers.get(index)) {
            return name.clone();
        }
        if let Some(name) = self.undefined_layers.get(&code.to_bits()) {
            return name.clone();
        }
        let name = self.out.layer(&format!("未定義のレイヤ {}", js::number_to_string(code)), false, false, true);
        self.out.notices.add(
            NoticeLevel::Warning,
            "unknown-layer",
            "定義のないレイヤコードの要素を、そのコードの名前のレイヤに書きました",
        );
        self.undefined_layers.insert(code.to_bits(), name.clone());
        name
    }

    fn color_hex(&mut self, code: f64) -> String {
        self.doc.color(code).unwrap_or_else(|| {
            self.out.notices.add(NoticeLevel::Warning, "unknown-color", "定義のない色コードを白（ACI 7）で書きました");
            "#ffffff".into()
        })
    }

    fn color(&mut self, code: f64) -> Color {
        let hex = self.color_hex(code);
        Color::from_hex(&hex)
    }

    fn linetype(&mut self, code: f64) -> String {
        let Some(dashes) = self.doc.dashes(code) else {
            self.out.notices.add(
                NoticeLevel::Warning,
                "unknown-line-type",
                "定義のない線種コードを実線（Continuous）で書きました",
            );
            return "Continuous".into();
        };
        if dashes.is_empty() {
            return "Continuous".into();
        }
        if (1.0..=16.0).contains(&code) {
            let kind = index(code - 1.0).and_then(|index| PREDEFINED_LINE_TYPES.get(index));
            let (name, label) = kind.map_or(("line type", ""), |kind| (kind.name, kind.label));
            return self.out.linetype(name, label, &dashes);
        }
        let name =
            index(code - 17.0).and_then(|index| self.doc.user_line_types.get(index)).map(|user| user.name.clone());
        self.out.linetype(name.as_deref().unwrap_or("line type"), "", &dashes)
    }

    fn lineweight(&mut self, code: f64) -> f64 {
        let width = self.doc.width(code).unwrap_or_else(|| {
            self.out.notices.add(NoticeLevel::Warning, "unknown-width", "定義のない線幅コードを 0.13 mm で書きました");
            zumen_sxf::DEFAULT_WIDTH
        });
        self.weight(width)
    }

    /// 線の太さ（mm）を、DXF で使える太さにする
    fn weight(&mut self, width: f64) -> f64 {
        let (value, rounded) = lineweight(width);
        if rounded {
            self.out.notices.add(
                NoticeLevel::Info,
                "lineweight",
                "線幅を、DXF で使えるいちばん近い線の太さ（0.13・0.18・0.25・0.3 mm など）にしました",
            );
        }
        value
    }

    fn look(&mut self, frame: &Frame, layer: f64, color: f64, line_type: f64, line_width: f64) -> Look {
        Look {
            layer: self.layer(layer),
            color: self.color(color),
            linetype: self.linetype(line_type),
            lineweight: self.lineweight(line_width),
            linetype_scale: frame.linetype_scale,
        }
    }

    fn degenerate(&mut self) {
        self.out.notices.add(
            NoticeLevel::Info,
            "degenerate",
            "点が足りない線・大きさが 0 の円などの、形にならない要素を省きました",
        );
    }

    fn emit(&mut self, feature: &SxfFeature, index: usize, frame: &Frame, views: &[View], contents: &[Content]) {
        let source = format!("#{}", js::number_to_string(feature.id()));
        match feature {
            SxfFeature::Line { style, x1, y1, x2, y2 } => {
                let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
                let data = DxfEntityData::Line { start: xyz(frame.point(*x1, *y1)), end: xyz(frame.point(*x2, *y2)) };
                self.out.add(&frame.space, &look, "LINE", data, &source);
            }
            SxfFeature::Polyline { style, xs, ys } => {
                let points: Vec<Vec2> = xs.iter().zip(ys).map(|(x, y)| frame.point(*x, *y)).collect();
                if points.len() < 2 {
                    self.degenerate();
                    return;
                }
                let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
                self.out.add(&frame.space, &look, "LWPOLYLINE", lwpolyline(&points, None, false), &source);
            }
            SxfFeature::Circle { style, cx, cy, r } => {
                if !(r.is_finite() && *r > 0.0) {
                    self.degenerate();
                    return;
                }
                let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
                let data = DxfEntityData::Circle { center: xyz(frame.point(*cx, *cy)), radius: r * frame.scale() };
                self.out.add(&frame.space, &look, "CIRCLE", data, &source);
            }
            SxfFeature::Arc { style, cx, cy, r, ccw, start, end } => {
                if !(r.is_finite() && *r > 0.0) {
                    self.degenerate();
                    return;
                }
                let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
                // DXF の円弧は反時計回り（DXF Reference「ARC」）。時計回りは始角と終角を入れ替える
                let (start_angle, end_angle) = if *ccw { (*start, *end) } else { (*end, *start) };
                let data = DxfEntityData::Arc {
                    center: xyz(frame.point(*cx, *cy)),
                    radius: r * frame.scale(),
                    start_angle,
                    end_angle,
                };
                self.out.add(&frame.space, &look, "ARC", data, &source);
            }
            SxfFeature::Ellipse { style, cx, cy, rx, ry, rotation } => {
                self.ellipse(frame, style, (*cx, *cy), (*rx, *ry), *rotation, None, &source);
            }
            SxfFeature::EllipseArc { style, cx, cy, rx, ry, ccw, rotation, start, end } => {
                self.ellipse(frame, style, (*cx, *cy), (*rx, *ry), *rotation, Some((*start, *end, *ccw)), &source);
            }
            SxfFeature::Spline { style, xs, ys, .. } => self.spline(frame, style, xs, ys, &source),
            SxfFeature::Clothoid { style, x, y, parameter, ccw, angle, start_length, end_length } => {
                let (lx, ly) = frame.local(*x, *y);
                let tol = TOLERANCE / js::max(mean_scale(&frame.to_sheet()), 1e-12);
                let length = (end_length - start_length).abs();
                let segments = js::min(js::max((length / js::max(tol * 20.0, 1e-6)).ceil(), 16.0), 2048.0);
                if segments.is_nan() {
                    self.degenerate();
                    return;
                }
                let flat =
                    clothoid_points(lx, ly, *parameter, *ccw, *angle, *start_length, *end_length, segments as usize);
                let points = to_container(frame, &flat);
                let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
                self.out.add(&frame.space, &look, "LWPOLYLINE", lwpolyline(&points, None, false), &source);
                self.out.notices.add(
                    NoticeLevel::Info,
                    "clothoid",
                    "クロソイドは、表示と同じ点を結ぶ折線（LWPOLYLINE）で書きました",
                );
            }
            SxfFeature::PointMarker { layer, color, x, y, code, angle, scale, .. } => {
                let sheet = frame.to_sheet();
                let (lx, ly) = frame.local(*x, *y);
                let (px, py) = apply(&sheet, lx, ly);
                let hex = self.color_hex(*color);
                let primitives = marker_primitives(*code, px, py, angle + rotation_deg(&sheet), *scale, &hex, 0.0);
                let look = Look {
                    layer: self.layer(*layer),
                    color: Color::from_hex(&hex),
                    linetype: "Continuous".into(),
                    lineweight: 0.0,
                    linetype_scale: 1.0,
                };
                let back = invert(&frame.to_paper);
                self.out.primitives(&frame.space, &look, &primitives, &back, look.color, &source);
            }
            SxfFeature::TextString { layer, color, text, .. } => {
                let layer = self.layer(*layer);
                let color = self.color(*color);
                self.text(frame, text, &layer, color, &frame.space, &source);
            }
            SxfFeature::SfigLocate { .. } => self.locate(feature, index, frame, views, contents, &source),
            SxfFeature::ExternallyDefinedSymbol { layer, color, name, x, y, angle, scale, .. } => {
                let sheet = frame.to_sheet();
                let (lx, ly) = frame.local(*x, *y);
                let (px, py) = apply(&sheet, lx, ly);
                let hex = match color {
                    None => "#ffffff".to_string(),
                    Some(code) => self.color_hex(*code),
                };
                let label = layout_text(
                    TextLayoutInput {
                        text: if name.is_empty() { "?" } else { name },
                        x: px,
                        y: py,
                        height: 2.5 * (if *scale > 0.0 { *scale } else { 1.0 }),
                        width: 0.0,
                        spacing: 0.0,
                        angle: angle + rotation_deg(&sheet),
                        slant: 0.0,
                        anchor: 5.0,
                        vertical: false,
                        color: hex.clone(),
                        font_family: font_family_for("").to_string(),
                    },
                    &invert(&frame.to_paper),
                );
                self.out.notices.add(
                    NoticeLevel::Info,
                    "symbol",
                    "既定義シンボルは、形の代わりに名前を文字で書きました",
                );
                if let Some(label) = label {
                    let style = self.out.style(DEFAULT_FONT, false);
                    if let Some(data) = text_entity(&label, false, 0.0, &style) {
                        let look = Look {
                            layer: self.layer(*layer),
                            color: Color::from_hex(&hex),
                            linetype: "Continuous".into(),
                            lineweight: LINEWEIGHT_BY_LAYER,
                            linetype_scale: 1.0,
                        };
                        self.out.add(&frame.space, &look, "TEXT", DxfEntityData::Text(data), &source);
                    }
                }
            }
            SxfFeature::LinearDim { .. }
            | SxfFeature::RadiusDim { .. }
            | SxfFeature::DiameterDim { .. }
            | SxfFeature::CurveDim(_)
            | SxfFeature::AngularDim(_) => self.dimension(feature, frame, &source),
            SxfFeature::Label { .. } | SxfFeature::Balloon { .. } => self.leader(feature, frame, &source),
            SxfFeature::FillAreaStyleColour { .. }
            | SxfFeature::FillAreaStyleHatching { .. }
            | SxfFeature::FillAreaStyleTiles { .. }
            | SxfFeature::ExternallyDefinedHatch { .. } => self.hatch(feature, frame, &source),
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse(
        &mut self,
        frame: &Frame,
        style: &zumen_sxf::Styled,
        center: (f64, f64),
        radii: (f64, f64),
        rotation: f64,
        arc: Option<(f64, f64, bool)>,
        source: &str,
    ) {
        let (rx, ry) = radii;
        if !(rx.is_finite() && ry.is_finite() && rx > 0.0 && ry > 0.0) {
            self.degenerate();
            return;
        }
        let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
        let s = frame.scale();
        let (sin, cos) = rotation.to_radians().sin_cos();
        // 媒介変数 t の点は (rx cos t, ry sin t) を rotation だけ回した点。長軸を X の向きにとると媒介変数は
        // そのまま、Y の向きにとると π/2 だけ戻る（DXF の ELLIPSE は長軸からの媒介変数、ratio ≤ 1）
        let (major, ratio, shift) =
            if rx >= ry { ([rx * cos, rx * sin], ry / rx, 0.0) } else { ([-ry * sin, ry * cos], rx / ry, PI / 2.0) };
        let (start_param, end_param) = match arc {
            None => (0.0, PI * 2.0),
            Some((start, end, ccw)) => {
                let (a, b) = if ccw { (start, end) } else { (end, start) };
                (a.to_radians() - shift, b.to_radians() - shift)
            }
        };
        let data = DxfEntityData::Ellipse {
            center: xyz(frame.point(center.0, center.1)),
            major_axis: [major[0] * s, major[1] * s, 0.0],
            ratio,
            start_param,
            end_param,
        };
        self.out.add(&frame.space, &look, "ELLIPSE", data, source);
    }

    /// 3 次ベジェ曲線（spline_feature、制御点 3n + 1 個）。区間の境の節点を 3 重にした NURBS にし、
    /// 表示が折線でつなぐ余りの点は LWPOLYLINE にする
    fn spline(&mut self, frame: &Frame, style: &zumen_sxf::Styled, xs: &[f64], ys: &[f64], source: &str) {
        let points: Vec<Vec2> = xs.iter().zip(ys).map(|(x, y)| frame.point(*x, *y)).collect();
        if points.len() < 2 {
            self.degenerate();
            return;
        }
        let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
        let pieces = (points.len() - 1) / 3;
        if pieces > 0 {
            let control: Vec<[f64; 3]> = points[..=pieces * 3].iter().map(|point| xyz(*point)).collect();
            let mut knots = vec![0.0; 4];
            for piece in 1..pieces {
                knots.extend([piece as f64; 3]);
            }
            knots.extend([pieces as f64; 4]);
            let data = DxfEntityData::Spline(DxfSpline {
                degree: 3.0,
                flags: 8.0,
                closed: false,
                periodic: false,
                rational: false,
                knots,
                control_points: control,
                weights: Vec::new(),
                fit_points: Vec::new(),
                start_tangent: None,
                end_tangent: None,
            });
            self.out.add(&frame.space, &look, "SPLINE", data, source);
        }
        if points.len() - 1 > pieces * 3 {
            let rest = &points[pieces * 3..];
            self.out.add(&frame.space, &look, "LWPOLYLINE", lwpolyline(rest, None, false), source);
        }
    }

    /// 文字（文字範囲に合わせた TEXT）。書いた文字範囲の中心（入れ物の座標）を返す
    fn text(
        &mut self,
        frame: &Frame,
        value: &Annotation,
        layer: &str,
        color: Color,
        space: &Space,
        source: &str,
    ) -> Option<Vec2> {
        if value.text.is_empty() {
            return None;
        }
        let font = self.doc.font(value.font);
        let (lx, ly) = frame.local(value.x, value.y);
        let input = value.layout_input(lx, ly, String::new(), font_family_for(font.unwrap_or("")).to_string());
        let vertical = input.vertical;
        let primitive = layout_text(input, &frame.to_container)?;
        if value.spacing > 0.0 && value.text.chars().count() > 1 {
            self.text_spacing();
        }
        let style = self.out.style(font.unwrap_or(DEFAULT_FONT), vertical);
        let data = text_entity(&primitive, vertical, value.slant, &style)?;
        let m = &primitive.matrix;
        let (cx, cy) = (primitive.box_width / 2.0, primitive.box_height / 2.0);
        let center = [m[0] * cx + m[2] * cy + m[4], m[1] * cx + m[3] * cy + m[5]];
        let look = Look {
            layer: layer.to_string(),
            color,
            linetype: "Continuous".into(),
            lineweight: LINEWEIGHT_BY_LAYER,
            linetype_scale: 1.0,
        };
        self.out.add(space, &look, "TEXT", DxfEntityData::Text(data), source);
        Some(center)
    }

    fn text_spacing(&mut self) {
        self.out.notices.add(
            NoticeLevel::Info,
            "text-spacing",
            "文字間隔は DXF の文字に書けないため、文字の幅を広げて同じ文字範囲に書きました",
        );
    }

    fn locate(
        &mut self,
        feature: &SxfFeature,
        index: usize,
        frame: &Frame,
        views: &[View],
        contents: &[Content],
        source: &str,
    ) {
        let SxfFeature::SfigLocate { layer, name, x, y, angle, ratio_x, ratio_y, .. } = feature else {
            return;
        };
        if self.cuts.contains(&(frame.key.clone(), index)) {
            return;
        }
        if frame.key == Key::Sheet {
            if self.empty_views.contains(&index) {
                return;
            }
            if let Some(view) = self.views.get(&index).copied() {
                let view_data = &views[view];
                let offset = contents[view_data.content].offset;
                add_viewport(&mut self.out, view_data, offset, view as f64 + 2.0, source);
                return;
            }
        }
        let Some(block) = self.blocks.get(name).cloned() else {
            return;
        };
        let flag = self.doc.sub_figures.get(name).map_or(0.0, |figure| figure.feature.flag);
        let (lx, ly) = frame.local(*x, *y);
        let local = if flag == 3.0 { IDENTITY } else { placement(lx, ly, *angle, *ratio_x, *ratio_y) };
        let m = multiply(&frame.to_container, &local);
        // 配置は 平行移動 → 回転 → 縮尺（placement）で、INSERT の 挿入点・回転・X と Y の尺度と同じ
        let s = frame.scale();
        let (rotation, xscale, yscale) = if flag == 3.0 { (0.0, s, s) } else { (*angle, ratio_x * s, ratio_y * s) };
        let look = Look {
            layer: self.layer(*layer),
            color: BY_LAYER,
            linetype: "ByLayer".into(),
            lineweight: LINEWEIGHT_BY_LAYER,
            linetype_scale: 1.0,
        };
        let data = DxfEntityData::Insert(zumen_dxf::DxfInsert {
            name: block.name.clone(),
            insert: [m[4], m[5], 0.0],
            xscale,
            yscale,
            zscale: 1.0,
            rotation: crate::text::normalize(rotation),
            column_count: 1.0,
            row_count: 1.0,
            column_spacing: 0.0,
            row_spacing: 0.0,
            attribs: Vec::new(),
        });
        self.out.add(&frame.space, &look, "INSERT", data, source);
    }

    // --- 寸法・引出し線 ----------------------------------------------------------------------------

    /// 矢印の先端と向き。向きは、寸法線の近い端から遠い端と反対（外向き）を基本にする（表示の arrow_at）
    fn arrow_at(frame: &Frame, arrow: Option<&Arrow>, a: (f64, f64), b: (f64, f64), color: &str) -> Vec<Primitive> {
        let Some(arrow) = arrow.filter(|arrow| arrow.code > 0.0) else {
            return Vec::new();
        };
        let m = frame.to_sheet();
        let (lx, ly) = frame.local(arrow.x, arrow.y);
        let (tx, ty) = apply(&m, lx, ly);
        let (ax, ay) = apply(&m, a.0, a.1);
        let (bx, by) = apply(&m, b.0, b.1);
        let near_a = js::hypot2(tx - ax, ty - ay) <= js::hypot2(tx - bx, ty - by);
        let mut ux = if near_a { ax - bx } else { bx - ax };
        let mut uy = if near_a { ay - by } else { by - ay };
        let length = js::hypot2(ux, uy);
        if length.is_nan() || length <= 0.0 {
            return Vec::new();
        }
        ux /= length;
        uy /= length;
        if arrow.direction == 2.0 {
            ux = -ux;
            uy = -uy;
        }
        arrow_primitives(arrow.code, tx, ty, ux, uy, arrow.scale, color, 0.0, BACKGROUND_COLOR)
    }

    /// 円弧の寸法線の矢印。矢印の位置での接線方向（表示の arc_arrow）
    fn arc_arrow(
        frame: &Frame,
        arrow: Option<&Arrow>,
        center: (f64, f64),
        start_side: bool,
        color: &str,
    ) -> Vec<Primitive> {
        let Some(arrow) = arrow.filter(|arrow| arrow.code > 0.0) else {
            return Vec::new();
        };
        let m = frame.to_sheet();
        let (lx, ly) = frame.local(arrow.x, arrow.y);
        let angle = js::atan2(ly - center.1, lx - center.0);
        let mut tx = if start_side { js::sin(angle) } else { -js::sin(angle) };
        let mut ty = if start_side { -js::cos(angle) } else { js::cos(angle) };
        if arrow.direction == 2.0 {
            tx = -tx;
            ty = -ty;
        }
        let (ux, uy) = apply_vector(&m, tx, ty);
        let length = js::hypot2(ux, uy);
        if length.is_nan() || length <= 0.0 {
            return Vec::new();
        }
        let (px, py) = apply(&m, lx, ly);
        arrow_primitives(arrow.code, px, py, ux / length, uy / length, arrow.scale, color, 0.0, BACKGROUND_COLOR)
    }

    /// 寸法は、寸法線・補助線・矢印・寸法値を表示のブロック（*D…）に入れた DIMENSION にする
    /// （DXF Reference「DIMENSION」。定義点は SXF の寸法線の端・補助線の基点から求める）
    fn dimension(&mut self, feature: &SxfFeature, frame: &Frame, source: &str) {
        let (style, text) = match feature {
            SxfFeature::LinearDim { style, text, .. }
            | SxfFeature::RadiusDim { style, text, .. }
            | SxfFeature::DiameterDim { style, text, .. } => (style, text),
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => (&dimension.style, &dimension.text),
            _ => return,
        };
        let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
        let hex = self.color_hex(style.color);
        let block = self.out.dimension_block();
        let space = Space::Block(block.clone());
        // 表示のブロックの線は寸法の線種・線の太さ・色を継ぎ（BYBLOCK）、矢印は実線で細く描く（表示と同じ）
        let line_look = Look { linetype_scale: frame.linetype_scale, ..Look::by_block() };
        let arrow_look = Look { linetype: "Continuous".into(), lineweight: 0.0, ..Look::by_block() };
        let s = frame.scale();
        let mut arrows: Vec<Primitive> = Vec::new();
        let mut kind = "DIMENSION";
        let mut dimension = DxfDimension {
            anonymous_block_name: block.clone(),
            dimtype: 32.0,
            defpoint: [0.0; 3],
            defpoint2: None,
            defpoint3: None,
            defpoint4: None,
            defpoint5: None,
            text_midpoint: [0.0; 3],
            insert: None,
            text: String::new(),
            actual_measurement: None,
            attachment_point: Some(5.0),
            text_rotation: None,
            horizontal_direction: None,
            angle: None,
            oblique_angle: None,
            dimstyle: "Standard".into(),
        };
        match feature {
            SxfFeature::CurveDim(d) | SxfFeature::AngularDim(d) => {
                let center = frame.local(d.cx, d.cy);
                let c = frame.point(d.cx, d.cy);
                let radius = d.radius * s;
                let arc = DxfEntityData::Arc { center: xyz(c), radius, start_angle: d.angle0, end_angle: d.angle1 };
                self.out.add(&space, &line_look, "ARC", arc, "");
                for ext in [&d.ext1, &d.ext2].into_iter().flatten() {
                    self.extension_line(&space, &line_look, frame, ext);
                }
                arrows.extend(Self::arc_arrow(frame, d.arrow1.as_ref(), center, true, &hex));
                arrows.extend(Self::arc_arrow(frame, d.arrow2.as_ref(), center, false, &hex));
                let on_arc = |degrees: f64| -> [f64; 3] {
                    let (sin, cos) = degrees.to_radians().sin_cos();
                    [c[0] + radius * cos, c[1] + radius * sin, 0.0]
                };
                let sweep = sweep_radians(d.angle0, d.angle1, true).to_degrees();
                let ext_point = |ext: &Option<ExtensionLine>, fallback: f64| match ext {
                    Some(ext) => xyz(frame.point(ext.x0, ext.y0)),
                    None => on_arc(fallback),
                };
                // 弧長寸法は ARC_DIMENSION（AutoCAD 2013 までの 70 は 5）、角度寸法は 3 点の角度寸法（5）
                if matches!(feature, SxfFeature::CurveDim(_)) {
                    kind = "ARC_DIMENSION";
                    dimension.actual_measurement = Some(radius * sweep.to_radians());
                }
                dimension.dimtype = 5.0 + 32.0;
                dimension.defpoint = on_arc(d.angle0 + sweep / 2.0);
                dimension.defpoint2 = Some(ext_point(&d.ext1, d.angle0));
                dimension.defpoint3 = Some(ext_point(&d.ext2, d.angle1));
                dimension.defpoint4 = Some(xyz(c));
                dimension.text_midpoint = on_arc(d.angle0 + sweep / 2.0);
            }
            SxfFeature::LinearDim { line, .. }
            | SxfFeature::RadiusDim { line, .. }
            | SxfFeature::DiameterDim { line, .. } => {
                let [x1, y1, x2, y2] = *line;
                let (a, b) = (frame.local(x1, y1), frame.local(x2, y2));
                let (pa, pb) = (frame.point(x1, y1), frame.point(x2, y2));
                let data = DxfEntityData::Line { start: xyz(pa), end: xyz(pb) };
                self.out.add(&space, &line_look, "LINE", data, "");
                let length = (pb[0] - pa[0]).hypot(pb[1] - pa[1]);
                dimension.text_midpoint = [(pa[0] + pb[0]) / 2.0, (pa[1] + pb[1]) / 2.0, 0.0];
                match feature {
                    SxfFeature::LinearDim { ext1, ext2, arrow1, arrow2, .. } => {
                        for ext in [ext1, ext2].into_iter().flatten() {
                            self.extension_line(&space, &line_look, frame, ext);
                        }
                        arrows.extend(Self::arrow_at(frame, arrow1.as_ref(), a, b, &hex));
                        arrows.extend(Self::arrow_at(frame, arrow2.as_ref(), a, b, &hex));
                        // 回転寸法（70 の 0）: 寸法線の向きに測る。定義点は補助線の基点（ない時は寸法線の端）
                        let first = ext1.as_ref().map_or(pa, |ext| frame.point(ext.x0, ext.y0));
                        let second = ext2.as_ref().map_or(pb, |ext| frame.point(ext.x0, ext.y0));
                        let (ux, uy) = if length > 0.0 {
                            ((pb[0] - pa[0]) / length, (pb[1] - pa[1]) / length)
                        } else {
                            (1.0, 0.0)
                        };
                        dimension.dimtype = 32.0;
                        dimension.defpoint = xyz(pb);
                        dimension.defpoint2 = Some(xyz(first));
                        dimension.defpoint3 = Some(xyz(second));
                        dimension.angle = Some(crate::text::normalize(uy.atan2(ux).to_degrees()));
                        dimension.actual_measurement =
                            Some(((second[0] - first[0]) * ux + (second[1] - first[1]) * uy).abs());
                    }
                    SxfFeature::RadiusDim { arrow, .. } => {
                        arrows.extend(Self::arrow_at(frame, arrow.as_ref(), a, b, &hex));
                        // 半径寸法（70 の 4）: 10 は中心、15 は円弧の上の点（矢印の側）
                        let tip_near_a = arrow.as_ref().is_some_and(|arrow| {
                            let tip = frame.point(arrow.x, arrow.y);
                            (tip[0] - pa[0]).hypot(tip[1] - pa[1]) < (tip[0] - pb[0]).hypot(tip[1] - pb[1])
                        });
                        let (center, point) = if tip_near_a { (pb, pa) } else { (pa, pb) };
                        dimension.dimtype = 4.0 + 32.0;
                        dimension.defpoint = xyz(center);
                        dimension.defpoint4 = Some(xyz(point));
                        dimension.actual_measurement = Some(length);
                    }
                    SxfFeature::DiameterDim { arrow1, arrow2, .. } => {
                        arrows.extend(Self::arrow_at(frame, arrow1.as_ref(), a, b, &hex));
                        arrows.extend(Self::arrow_at(frame, arrow2.as_ref(), a, b, &hex));
                        // 直径寸法（70 の 3）: 10 と 15 は円の上の向かい合う点
                        dimension.dimtype = 3.0 + 32.0;
                        dimension.defpoint = xyz(pa);
                        dimension.defpoint4 = Some(xyz(pb));
                        dimension.actual_measurement = Some(length);
                    }
                    _ => {}
                }
            }
            _ => return,
        }
        let back = invert(&frame.to_paper);
        self.out.primitives(&space, &arrow_look, &arrows, &back, BY_BLOCK, "");
        if let Some(text) = text {
            let text_look_layer = "0".to_string();
            if let Some(center) = self.text(frame, text, &text_look_layer, BY_BLOCK, &space, "") {
                dimension.text_midpoint = xyz(center);
            }
            dimension.text = dimension_text(&text.text);
        }
        self.out.add(&frame.space, &look, kind, DxfEntityData::Dimension(dimension), source);
    }

    fn extension_line(&mut self, space: &Space, look: &Look, frame: &Frame, ext: &ExtensionLine) {
        let data =
            DxfEntityData::Line { start: xyz(frame.point(ext.x1, ext.y1)), end: xyz(frame.point(ext.x2, ext.y2)) };
        self.out.add(space, look, "LINE", data, "");
    }

    /// 引出し線・バルーンは、線（LWPOLYLINE）・矢印・円・文字に分けて書く。AutoCAD の LEADER・MULTILEADER は
    /// 矢印の形と文字の置き方が SXF と違い、同じ形に描けない
    fn leader(&mut self, feature: &SxfFeature, frame: &Frame, source: &str) {
        let (style, xs, ys, arrow_code, arrow_scale, text, balloon) = match feature {
            SxfFeature::Label { style, xs, ys, arrow_code, arrow_scale, text } => {
                (style, xs, ys, *arrow_code, *arrow_scale, text, None)
            }
            SxfFeature::Balloon { style, xs, ys, cx, cy, radius, arrow_code, arrow_scale, text } => {
                (style, xs, ys, *arrow_code, *arrow_scale, text, Some((*cx, *cy, *radius)))
            }
            _ => return,
        };
        self.out.notices.add(
            NoticeLevel::Info,
            "leader",
            "引出し線・バルーンは、線・矢印・円・文字の図形に分けて書きました",
        );
        let look = self.look(frame, style.layer, style.color, style.line_type, style.line_width);
        let hex = self.color_hex(style.color);
        let color = Color::from_hex(&hex);
        let local: Vec<(f64, f64)> = xs.iter().zip(ys).map(|(x, y)| frame.local(*x, *y)).collect();
        let points: Vec<Vec2> = local
            .iter()
            .map(|(x, y)| {
                let (px, py) = apply(&frame.to_container, *x, *y);
                [px, py]
            })
            .collect();
        if points.len() >= 2 {
            self.out.add(&frame.space, &look, "LWPOLYLINE", lwpolyline(&points, None, false), source);
        }
        if local.len() >= 2 && arrow_code > 0.0 {
            let m = frame.to_sheet();
            let (tx, ty) = apply(&m, local[0].0, local[0].1);
            let (nx, ny) = apply(&m, local[1].0, local[1].1);
            let length = js::hypot2(tx - nx, ty - ny);
            if length > 0.0 {
                let primitives = arrow_primitives(
                    arrow_code,
                    tx,
                    ty,
                    (tx - nx) / length,
                    (ty - ny) / length,
                    arrow_scale,
                    &hex,
                    0.0,
                    BACKGROUND_COLOR,
                );
                let arrow_look =
                    Look { linetype: "Continuous".into(), lineweight: 0.0, linetype_scale: 1.0, ..look.clone() };
                self.out.primitives(&frame.space, &arrow_look, &primitives, &invert(&frame.to_paper), color, source);
            }
        }
        if let Some((cx, cy, radius)) = balloon
            && radius.is_finite()
            && radius > 0.0
        {
            let data = DxfEntityData::Circle { center: xyz(frame.point(cx, cy)), radius: radius * frame.scale() };
            self.out.add(&frame.space, &look, "CIRCLE", data, source);
        }
        if let Some(text) = text {
            let layer = look.layer.clone();
            self.text(frame, text, &layer, color, &frame.space, source);
        }
    }

    /// 複合曲線を、表示と同じ向きでつないだ閉じた境界にする（表示の ring・append_chained と同じ判定）。
    /// 線・折線・円弧は頂点と弧の膨らみ、楕円弧・スプライン・クロソイドは表示と同じ点で書く
    fn ring(&self, frame: &Frame, id: f64, tol: f64) -> Option<Ring> {
        let curve = index(id - 1.0).and_then(|index| self.doc.composite_curves.get(index))?;
        let local_points = |xs: &[f64], ys: &[f64]| -> Vec<f64> {
            xs.iter()
                .zip(ys)
                .flat_map(|(x, y)| {
                    let (lx, ly) = frame.local(*x, *y);
                    [lx, ly]
                })
                .collect()
        };
        let polyline = |flat: &[f64]| -> Vec<(Vec2, f64)> {
            flat.as_chunks::<2>().0.iter().map(|[x, y]| ([*x, *y], 0.0)).collect()
        };
        let mut flat: Vec<f64> = Vec::new();
        let mut vertices: Vec<(Vec2, f64)> = Vec::new();
        for member in &curve.members {
            let (points, piece) = match member {
                SxfFeature::Polyline { xs, ys, .. } => {
                    let points = local_points(xs, ys);
                    let piece = polyline(&points);
                    (points, piece)
                }
                SxfFeature::Line { x1, y1, x2, y2, .. } => {
                    let points = local_points(&[*x1, *x2], &[*y1, *y2]);
                    let piece = polyline(&points);
                    (points, piece)
                }
                SxfFeature::Arc { cx, cy, r, ccw, start, end, .. } => {
                    let (lx, ly) = frame.local(*cx, *cy);
                    let sweep = sweep_radians(*start, *end, *ccw);
                    let points =
                        ellipse_arc_points(lx, ly, *r, *r, 0.0, *start, *end, *ccw, arc_segments(*r, sweep, tol));
                    let piece = match (points.first_chunk::<2>(), points.last_chunk::<2>()) {
                        (Some(first), Some(last)) if points.len() >= 4 => {
                            vec![(*first, (sweep / 4.0).tan()), (*last, 0.0)]
                        }
                        _ => Vec::new(),
                    };
                    (points, piece)
                }
                SxfFeature::EllipseArc { cx, cy, rx, ry, ccw, rotation, start, end, .. } => {
                    let (lx, ly) = frame.local(*cx, *cy);
                    let sweep = sweep_radians(*start, *end, *ccw);
                    let segments = arc_segments(js::max(*rx, *ry), sweep, tol);
                    let points = ellipse_arc_points(lx, ly, *rx, *ry, *rotation, *start, *end, *ccw, segments);
                    let piece = polyline(&points);
                    (points, piece)
                }
                SxfFeature::Spline { xs, ys, .. } => {
                    let points = bezier_points(&local_points(xs, ys), 24);
                    let piece = polyline(&points);
                    (points, piece)
                }
                SxfFeature::Clothoid { x, y, parameter, ccw, angle, start_length, end_length, .. } => {
                    let (lx, ly) = frame.local(*x, *y);
                    let points = clothoid_points(lx, ly, *parameter, *ccw, *angle, *start_length, *end_length, 64);
                    let piece = polyline(&points);
                    (points, piece)
                }
                _ => continue,
            };
            if points.len() < 4 || piece.is_empty() {
                continue;
            }
            if flat.is_empty() {
                flat = points;
                vertices = piece;
                continue;
            }
            let (lx, ly) = (flat[flat.len() - 2], flat[flat.len() - 1]);
            let n = points.len();
            let start_distance = js::hypot2(points[0] - lx, points[1] - ly);
            let end_distance = js::hypot2(points[n - 2] - lx, points[n - 1] - ly);
            let piece = if end_distance < start_distance {
                let mut i = n as isize - 4;
                while i >= 0 {
                    flat.push(points[i as usize]);
                    flat.push(points[i as usize + 1]);
                    i -= 2;
                }
                reverse_piece(&piece)
            } else {
                flat.extend_from_slice(&points[2..]);
                piece
            };
            if let Some(last) = vertices.last_mut() {
                last.1 = piece[0].1;
            }
            vertices.extend_from_slice(&piece[1..]);
        }
        if flat.len() < 6 {
            return None;
        }
        let points: Vec<Vec2> = vertices
            .iter()
            .map(|([x, y], _)| {
                let (px, py) = apply(&frame.to_container, *x, *y);
                [px, py]
            })
            .collect();
        let bulges = vertices.iter().map(|(_, bulge)| *bulge).collect();
        Some(Ring { flat, points, bulges })
    }

    /// ハッチング。境界は複合曲線、塗り（fill_area_style_colour）は塗りつぶしの HATCH、ハッチング線
    /// （fill_area_style_hatching）は線の組ごとに利用者定義の HATCH にする。外形の線は複合曲線の表示の指定に従う
    fn hatch(&mut self, feature: &SxfFeature, frame: &Frame, source: &str) {
        let (layer, region) = match feature {
            SxfFeature::FillAreaStyleColour { layer, region, .. }
            | SxfFeature::FillAreaStyleHatching { layer, region, .. }
            | SxfFeature::FillAreaStyleTiles { layer, region, .. }
            | SxfFeature::ExternallyDefinedHatch { layer, region, .. } => (*layer, region),
            _ => return,
        };
        let Region { out_id, in_ids } = region;
        let tol = TOLERANCE / js::max(mean_scale(&frame.to_sheet()), 1e-12);
        let Some(outer) = self.ring(frame, *out_id, tol) else {
            self.out.notices.add(NoticeLevel::Warning, "hatch-boundary", "外形の複合曲線がないハッチングを省きました");
            return;
        };
        let mut rings = vec![outer];
        rings.extend(in_ids.iter().filter_map(|id| self.ring(frame, *id, tol)));
        let layer_name = self.layer(layer);
        let paths: Vec<DxfHatchPath> = rings
            .iter()
            .enumerate()
            .map(|(position, ring)| {
                polyline_path(&ring.points, Some(ring.bulges.clone()), if position == 0 { 1.0 } else { 0.0 })
            })
            .collect();
        match feature {
            SxfFeature::FillAreaStyleColour { color, .. } => {
                let look = Look {
                    layer: layer_name.clone(),
                    color: self.color(*color),
                    linetype: "Continuous".into(),
                    lineweight: LINEWEIGHT_BY_LAYER,
                    linetype_scale: 1.0,
                };
                self.out.add(&frame.space, &look, "HATCH", solid_hatch(paths.clone()), source);
            }
            SxfFeature::FillAreaStyleHatching { patterns, .. } => {
                for pattern in patterns {
                    self.hatch_lines(frame, pattern, &rings, &paths, &layer_name, source);
                }
            }
            SxfFeature::FillAreaStyleTiles { .. } => {
                self.out.notices.add(
                    NoticeLevel::Info,
                    "hatch-pattern",
                    "パターンハッチングの模様は書かず、外形の線だけを書きました",
                );
            }
            SxfFeature::ExternallyDefinedHatch { name, .. } => {
                if name.to_lowercase() == "area_control" {
                    self.out.notices.add(
                        NoticeLevel::Info,
                        "area-control",
                        "既定義ハッチング（Area_control）は空白の領域のため、外形の線だけを書きました",
                    );
                } else {
                    self.out.notices.add(
                        NoticeLevel::Info,
                        "hatch-external",
                        "外部定義ハッチングの模様は書かず、外形の線だけを書きました",
                    );
                }
            }
            _ => {}
        }
        // 外形・中抜きの複合曲線のうち、表示する指定のもの（表示と同じく、読めた境界を順に当てる）
        let ids: Vec<f64> = std::iter::once(*out_id).chain(in_ids.iter().copied()).collect();
        let doc = self.doc;
        for (position, id) in ids.iter().enumerate() {
            let curve = index(id - 1.0).and_then(|index| doc.composite_curves.get(index));
            let (Some(curve), Some(ring)) = (curve, rings.get(position)) else {
                continue;
            };
            if !curve.feature.visible {
                continue;
            }
            let mut look =
                self.look(frame, layer, curve.feature.color, curve.feature.line_type, curve.feature.line_width);
            look.layer = layer_name.clone();
            let data = lwpolyline(&ring.points, Some(ring.bulges.clone()), true);
            self.out.add(&frame.space, &look, "LWPOLYLINE", data, source);
        }
    }

    /// ハッチング線の 1 組を利用者定義の HATCH にする。間隔と角度は配置の影響を受け（SXF 実装規約 1-7）、
    /// 線種のピッチは用紙の mm。表示が描かない組（間隔が 0 以下、線が多すぎる）は書かない
    fn hatch_lines(
        &mut self,
        frame: &Frame,
        pattern: &HatchPattern,
        rings: &[Ring],
        paths: &[DxfHatchPath],
        layer: &str,
        source: &str,
    ) {
        if pattern.spacing.is_nan() || pattern.spacing <= 0.0 {
            return;
        }
        let m = frame.to_sheet();
        let mut bbox = empty_box();
        for ring in rings {
            extend_box_with_points(&mut bbox, &zumen_core::geometry::transform_points(&m, &ring.flat));
        }
        let theta = (pattern.angle * PI) / 180.0;
        let (lx, ly) = frame.local(pattern.x, pattern.y);
        let (px, py) = apply(&m, lx, ly);
        let (ux, uy) = apply_vector(&m, js::cos(theta), js::sin(theta));
        let (sx, sy) = apply_vector(&m, -js::sin(theta) * pattern.spacing, js::cos(theta) * pattern.spacing);
        let denominator = ux * sy - uy * sx;
        if denominator.abs() < 1e-15 {
            return;
        }
        let corners =
            [(bbox.min_x, bbox.min_y), (bbox.max_x, bbox.min_y), (bbox.max_x, bbox.max_y), (bbox.min_x, bbox.max_y)];
        let ks: Vec<f64> = corners.iter().map(|(cx, cy)| (ux * (cy - py) - uy * (cx - px)) / denominator).collect();
        let k_min = ks.iter().copied().fold(f64::INFINITY, js::min).ceil();
        let k_max = ks.iter().copied().fold(f64::NEG_INFINITY, js::max).floor();
        if k_max - k_min + 1.0 > MAX_HATCH_LINES {
            self.out.notices.add(
                NoticeLevel::Warning,
                "hatch-dense",
                "間隔が細かすぎるハッチング線は、表示と同じく書いていません",
            );
            return;
        }
        let s = frame.scale();
        let (base_x, base_y) = apply(&frame.to_container, lx, ly);
        let paper_dashes = self.doc.dashes(pattern.line_type).unwrap_or_else(|| {
            self.out.notices.add(
                NoticeLevel::Warning,
                "unknown-line-type",
                "定義のない線種コードを実線（Continuous）で書きました",
            );
            Vec::new()
        });
        let k = scale_of(&frame.to_paper);
        let dashes: Vec<f64> =
            paper_dashes.iter().enumerate().map(|(i, value)| if i % 2 == 0 { value / k } else { -value / k }).collect();
        let look = Look {
            layer: layer.to_string(),
            color: self.color(pattern.color),
            linetype: "Continuous".into(),
            lineweight: self.lineweight(pattern.line_width),
            linetype_scale: 1.0,
        };
        let spacing = pattern.spacing * s;
        let dashed = !dashes.is_empty();
        let data = DxfEntityData::Hatch(DxfHatch {
            pattern_name: if dashed { "SXF_USER".into() } else { "_USER".into() },
            solid_fill: false,
            associative: false,
            hatch_style: 0.0,
            // 利用者定義（0）は 1 組の実線、破線はカスタム（2）
            pattern_type: if dashed { 2.0 } else { 0.0 },
            pattern_angle: if dashed { 0.0 } else { crate::text::normalize(pattern.angle) },
            pattern_scale: if dashed { 1.0 } else { spacing },
            pattern_double: false,
            elevation: 0.0,
            paths: paths.to_vec(),
            pattern_lines: vec![DxfPatternLine {
                angle: crate::text::normalize(pattern.angle),
                base: [base_x, base_y],
                offset: [-js::sin(theta) * spacing, js::cos(theta) * spacing],
                dashes,
            }],
            gradient: None,
            gradient_colors: None,
            gradient_angle: 0.0,
            gradient_shift: 0.0,
            gradient_one_color: false,
            gradient_tint: 0.0,
        });
        self.out.add(&frame.space, &look, "HATCH", data, source);
    }
}

/// 複合曲線をつないだ境界。flat は表示と同じ点（表示の向きの元の座標）、points・bulges は入れ物の座標の頂点
struct Ring {
    flat: Vec<f64>,
    points: Vec<Vec2>,
    bulges: Vec<f64>,
}

/// 頂点と弧の膨らみの並びを逆向きにする（膨らみは区間の始まりの頂点が持ち、向きが変われば符号が変わる）
fn reverse_piece(piece: &[(Vec2, f64)]) -> Vec<(Vec2, f64)> {
    let n = piece.len();
    (0..n)
        .map(|i| {
            let point = piece[n - 1 - i].0;
            let bulge = if i + 1 < n { -piece[n - 2 - i].1 } else { 0.0 };
            (point, bulge)
        })
        .collect()
}

/// 寸法の文字（1）。MTEXT の書式で読まれるので、\ と { } を書式でない文字にする
fn dimension_text(text: &str) -> String {
    text.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}")
}

/// 点列（表示の向きの元の座標）を入れ物の座標の頂点にする
fn to_container(frame: &Frame, flat: &[f64]) -> Vec<Vec2> {
    flat.as_chunks::<2>()
        .0
        .iter()
        .map(|[x, y]| {
            let (px, py) = apply(&frame.to_container, *x, *y);
            [px, py]
        })
        .collect()
}

fn add_viewport(builder: &mut Builder, view: &View, offset: [f64; 2], id: f64, source: &str) {
    let look = Look {
        layer: crate::builder::VIEWPORT_LAYER.into(),
        color: BY_LAYER,
        linetype: "ByLayer".into(),
        lineweight: LINEWEIGHT_BY_LAYER,
        linetype_scale: 1.0,
    };
    let data = DxfEntityData::Viewport(viewport(view, offset, id));
    builder.add(&Space::Paper, &look, "VIEWPORT", data, source);
}

/// レイアウトの名前（DXF で使えない文字を _ にする）
fn layout_name(name: &str) -> String {
    let name = crate::builder::valid_name(&name.replace(',', "_"));
    if name.is_empty() { "Layout1".into() } else { name }
}

/// 見出しの範囲（モデル空間の範囲と、紙空間の用紙）
fn finish_header(builder: &mut Builder, extents: &BBox, width: f64, height: f64) {
    let header = &mut builder.doc.header;
    if extents.min_x <= extents.max_x && extents.min_y <= extents.max_y {
        header.extmin = Some([extents.min_x, extents.min_y, 0.0]);
        header.extmax = Some([extents.max_x, extents.max_y, 0.0]);
        header.limmin = Some([extents.min_x, extents.min_y]);
        header.limmax = Some([extents.max_x, extents.max_y]);
    }
    header.plimmin = Some([0.0, 0.0]);
    header.plimmax = Some([width, height]);
}
