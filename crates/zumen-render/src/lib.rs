//! zumen の表示用モデル（`Scene`）を PNG・SVG・PDF に描く。
//!
//! ブラウザの描画（`@zumenjs/core` の render.ts）と同じ規則で描く: 背景と見分けにくい色の反転、
//! 線の太さと 1 デバイス px の下限、短すぎる破線の実線化、偶奇規則の塗り、領域で切り取った線、
//! 文字の 1 文字ごとの配置と、小さすぎる文字の代わりの線。
//! 文字は同梱の BIZ UDゴシック（SIL Open Font License 1.1）で描く。座標の変換は f64 で行い、
//! 測量座標のような大きな値でも位置がずれないようにする。
#![forbid(unsafe_code)]

mod pdf;
mod svg;
mod vector;

pub use pdf::render_pdf;
pub use svg::render_svg;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform,
};
use ttf_parser::{Face, OutlineBuilder};
use zumen_core::extract::{EntityFilter, select_entities, select_underlays};
use zumen_core::{
    BACKGROUND_COLOR, BBox, Background, Matrix, Notice, NoticeLevel, Primitive, Scene, SceneEntity, StrokePrimitive,
    TextPrimitive,
};

pub use zumen_core::extract::is_entity_visible;

/// 同梱の日本語フォント（BIZ UDゴシック Regular、SIL Open Font License 1.1。fonts/OFL.txt）
pub const BUNDLED_FONT: &[u8] = include_bytes!("../fonts/BIZUDGothic-Regular.ttf");

/// これより小さい文字は字形を描かず、文字の範囲を線で示す（CSS px）
const MIN_GLYPH_PX: f64 = 2.5;

/// 一度に描ける画素数の上限（RGBA で約 400 MB）。大きな用紙を高い解像度で描くときは dpi を下げる
pub const MAX_PIXELS: u64 = 100_000_000;

/// 用紙の mm から画像の px への変換。x_px = offset_x + x × scale、y_px = offset_y − y × scale
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl View {
    /// 用紙の範囲 region を、dpi（1 インチ = 25.4 mm あたりの px）で描く変換と画像の大きさ
    pub fn for_region(region: &BBox, dpi: f64) -> Result<(View, u32, u32), RenderError> {
        if !dpi.is_finite() || dpi <= 0.0 {
            return Err(RenderError::new("dpi は正の数で指定してください"));
        }
        let width = region.max_x - region.min_x;
        let height = region.max_y - region.min_y;
        if !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0 {
            return Err(RenderError::new("描く範囲がありません"));
        }
        let scale = dpi / 25.4;
        let pixels_x = (width * scale).ceil().max(1.0);
        let pixels_y = (height * scale).ceil().max(1.0);
        if pixels_x * pixels_y > MAX_PIXELS as f64 {
            return Err(RenderError::new(format!(
                "画像が大きすぎます（{} × {} px）。dpi を下げるか、範囲を狭めてください",
                pixels_x, pixels_y
            )));
        }
        let view = View { scale, offset_x: -region.min_x * scale, offset_y: region.max_y * scale };
        Ok((view, pixels_x as u32, pixels_y as u32))
    }

    /// TypeScript の fitView と同じ: 外接矩形を、余白 margin を残して画像に収める
    pub fn fit(bounds: &BBox, width: f64, height: f64, margin: f64) -> View {
        let w = (bounds.max_x - bounds.min_x).max(1e-6);
        let h = (bounds.max_y - bounds.min_y).max(1e-6);
        let scale = ((width - margin * 2.0) / w).min((height - margin * 2.0) / h).max(1e-9);
        View {
            scale,
            offset_x: width / 2.0 - ((bounds.min_x + bounds.max_x) / 2.0) * scale,
            offset_y: height / 2.0 + ((bounds.min_y + bounds.max_y) / 2.0) * scale,
        }
    }

    /// 用紙の mm を画像の px にする（左上が原点で、Y は下向き）
    pub fn to_pixel(&self, x: f64, y: f64) -> (f64, f64) {
        (self.offset_x + x * self.scale, self.offset_y - y * self.scale)
    }

    /// 画像の px を用紙の mm にする
    pub fn to_paper(&self, px: f64, py: f64) -> (f64, f64) {
        ((px - self.offset_x) / self.scale, (self.offset_y - py) / self.scale)
    }

    /// width × height px の画像に写る用紙の範囲（mm）
    pub fn visible_region(&self, width: u32, height: u32) -> BBox {
        let (min_x, min_y) = self.to_paper(0.0, f64::from(height));
        let (max_x, max_y) = self.to_paper(f64::from(width), 0.0);
        BBox { min_x, min_y, max_x, max_y }
    }

    /// 用紙の矩形を、width × height px の画像の上の矩形にする。画像からはみ出す部分は切り、
    /// 画像と重ならなければ None
    pub fn pixel_box(&self, bbox: &BBox, width: u32, height: u32) -> Option<PixelBox> {
        let (x0, y0) = self.to_pixel(bbox.min_x, bbox.max_y);
        let (x1, y1) = self.to_pixel(bbox.max_x, bbox.min_y);
        let clipped = PixelBox {
            left: x0.max(0.0),
            top: y0.max(0.0),
            right: x1.min(f64::from(width)),
            bottom: y1.min(f64::from(height)),
        };
        (clipped.left <= clipped.right && clipped.top <= clipped.bottom).then_some(clipped)
    }
}

/// 画像の上の矩形（px。左上が原点で、Y は下向き）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBox {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// 背景。None なら図面の想定（Scene.background）
    pub theme: Option<Background>,
    /// 線幅を太さどおりに描く。false なら細線（1 px）
    pub line_weights: bool,
    /// 非表示のレイヤ。None なら図面の表示指定（Scene.layers の visible）に従う
    pub hidden_layers: Option<HashSet<u32>>,
    /// 用紙の枠を描く
    pub sheet_frame: bool,
    /// 画像の px と CSS px の比（ブラウザの devicePixelRatio）。点の大きさと、字形を描く最小の文字の大きさに使う
    pub device_pixel_ratio: f64,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self { theme: None, line_weights: true, hidden_layers: None, sheet_frame: true, device_pixel_ratio: 1.0 }
    }
}

/// 描いた結果と、描けなかった内容
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderReport {
    /// 同梱のフォントに字形がなく、描かなかった文字
    pub missing_glyphs: BTreeSet<char>,
    /// 描かなかった内容の種類と件数（`underlay-not-drawn`: 範囲に写る underlay）
    pub notices: Vec<Notice>,
}

/// 描く範囲 region に写り、表示するレイヤにある underlay（画像・PDF のページ）の notice。zumen-render は underlay の
/// 中身（画像・PDF のバイト）を受け取らないので描かず、件数を返す（docs/decisions/0022）
pub(crate) fn underlay_notices(scene: &Scene, region: BBox, options: &RenderOptions) -> Vec<Notice> {
    let filter = EntityFilter { region: Some(region), hidden_layers: options.hidden_layers.clone() };
    let count = select_underlays(scene, &filter).len();
    if count == 0 {
        return Vec::new();
    }
    vec![Notice {
        level: NoticeLevel::Warning,
        code: "underlay-not-drawn".into(),
        message: "underlay（画像・PDF のページ）は中身を受け取らないので描いていません".into(),
        count: count as u64,
    }]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError {
    message: String,
}

impl RenderError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RenderError {}

/// 図面を描いた画像（RGBA）を返す
pub fn render(
    scene: &Scene,
    view: &View,
    width: u32,
    height: u32,
    options: &RenderOptions,
) -> Result<(Pixmap, RenderReport), RenderError> {
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(RenderError::new(format!("画像が大きすぎます（{width} × {height} px）")));
    }
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).ok_or_else(|| RenderError::new("画像を作れません"))?;
    let face = Face::parse(BUNDLED_FONT, 0).map_err(|_| RenderError::new("同梱のフォントを読めません"))?;
    let theme = options.theme.unwrap_or(scene.background);
    let background = background_rgb(theme);
    pixmap.fill(Color::from_rgba8(background.0, background.1, background.2, 255));
    let mut painter = Painter {
        pixmap: &mut pixmap,
        base: [view.scale, 0.0, 0.0, -view.scale, view.offset_x, view.offset_y],
        scale: view.scale,
        theme,
        line_weights: options.line_weights,
        device_pixel_ratio: options.device_pixel_ratio.max(1e-6),
        face,
        glyphs: HashMap::new(),
        report: RenderReport::default(),
        batch: None,
    };
    if options.sheet_frame {
        painter.sheet(scene);
    }
    for entity in drawn_entities(scene, view, width, height, options) {
        for primitive in &entity.primitives {
            painter.primitive(primitive, None);
        }
    }
    painter.flush(None);
    let mut report = painter.report;
    report.notices = underlay_notices(scene, view.visible_region(width, height), options);
    Ok((pixmap, report))
}

/// 画像の大きさの決め方
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageSize {
    /// 用紙の範囲を dpi（1 インチ = 25.4 mm あたりの px）で描く
    Dpi(f64),
    /// 用紙の範囲を width × height px に、余白 margin px を残して収める
    Fit { width: u32, height: u32, margin: f64 },
}

/// PNG にした図面と、画像の px と用紙の mm の変換。描いた図形は `drawn_entities` で分かる
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedImage {
    pub png: Vec<u8>,
    pub view: View,
    pub width: u32,
    pub height: u32,
    pub report: RenderReport,
}

/// 図面の範囲 region（用紙の mm、None なら図面全体）を描き、PNG にする
pub fn render_image(
    scene: &Scene,
    region: Option<BBox>,
    size: ImageSize,
    options: &RenderOptions,
) -> Result<RenderedImage, RenderError> {
    let region = region.unwrap_or(scene.bounds);
    let (view, width, height) = match size {
        ImageSize::Dpi(dpi) => View::for_region(&region, dpi)?,
        ImageSize::Fit { width, height, margin } => {
            (View::fit(&region, f64::from(width), f64::from(height), margin), width, height)
        }
    };
    let (pixmap, report) = render(scene, &view, width, height, options)?;
    let png = pixmap.encode_png().map_err(|error| RenderError::new(format!("PNG を作れません: {error}")))?;
    Ok(RenderedImage { png, view, width, height, report })
}

/// 図面の範囲 region（用紙の mm、None なら図面全体）を dpi で描き、PNG にする
pub fn render_png(
    scene: &Scene,
    region: Option<BBox>,
    dpi: f64,
    options: &RenderOptions,
) -> Result<(Vec<u8>, RenderReport), RenderError> {
    let image = render_image(scene, region, ImageSize::Dpi(dpi), options)?;
    Ok((image.png, image.report))
}

/// 図面の範囲 region（None なら図面全体）を width × height px に、余白 margin px を残して収めて描き、PNG にする
pub fn render_png_fit(
    scene: &Scene,
    region: Option<BBox>,
    width: u32,
    height: u32,
    margin: f64,
    options: &RenderOptions,
) -> Result<(Vec<u8>, RenderReport), RenderError> {
    let image = render_image(scene, region, ImageSize::Fit { width, height, margin }, options)?;
    Ok((image.png, image.report))
}

/// render が描く図形を、描く順に返す。表示するレイヤにあり、外接矩形が画像に写る範囲と重なる図形
pub fn drawn_entities<'a>(
    scene: &'a Scene,
    view: &View,
    width: u32,
    height: u32,
    options: &RenderOptions,
) -> Vec<&'a SceneEntity> {
    let filter =
        EntityFilter { region: Some(view.visible_region(width, height)), hidden_layers: options.hidden_layers.clone() };
    select_entities(scene, &filter)
}

/// #rrggbb・#rrggbbaa を RGBA にする。読めない色は None
fn parse_color(color: &str) -> Option<[u8; 4]> {
    let hex = color.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return None;
    }
    let part = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
    let alpha = if hex.len() == 8 { part(6)? } else { 255 };
    Some([part(0)?, part(2)?, part(4)?, alpha])
}

/// 線の太さ・破線・色が同じ線をまとめて 1 本の Path で描く（Canvas と同じく、重なりを二重に塗らない）
struct Batch {
    key: (u32, u32, Vec<u32>),
    color: [u8; 4],
    width: f32,
    dashes: Vec<f32>,
    path: PathBuilder,
}

struct Painter<'a> {
    pixmap: &'a mut Pixmap,
    /// 用紙の mm から画像の px への変換
    base: Matrix,
    /// 1 mm が何 px か
    scale: f64,
    theme: Background,
    line_weights: bool,
    device_pixel_ratio: f64,
    face: Face<'static>,
    glyphs: HashMap<char, Option<Rc<Glyph>>>,
    report: RenderReport,
    batch: Option<Batch>,
}

/// 背景の色（RGB）
pub(crate) fn background_rgb(theme: Background) -> (u8, u8, u8) {
    match theme {
        Background::Dark => (0, 0, 0),
        Background::Light => (255, 255, 255),
    }
}

/// 描く色。背景色（BACKGROUND_COLOR）は背景の色にし、背景と見分けにくい色（白背景の白に近い色、
/// 黒背景の黒に近い色）は反対の色にする。不透明度は保つ。読めない色は None
pub(crate) fn resolve_color(color: &str, theme: Background) -> Option<[u8; 4]> {
    if color == BACKGROUND_COLOR {
        let (r, g, b) = background_rgb(theme);
        return Some([r, g, b, 255]);
    }
    let [r, g, b, a] = parse_color(color)?;
    let value = (f64::from(r) * 0.299 + f64::from(g) * 0.587 + f64::from(b) * 0.114) / 255.0;
    Some(match theme {
        Background::Light if value > 0.85 => [0, 0, 0, a],
        Background::Dark if value < 0.05 => [255, 255, 255, a],
        _ => [r, g, b, a],
    })
}

impl Painter<'_> {
    fn color(&self, color: &str) -> Option<[u8; 4]> {
        resolve_color(color, self.theme)
    }

    fn to_px(&self, x: f64, y: f64) -> (f32, f32) {
        let m = &self.base;
        ((m[0] * x + m[2] * y + m[4]) as f32, (m[1] * x + m[3] * y + m[5]) as f32)
    }

    fn line_width(&self, width: f64, boost: f64) -> f64 {
        if self.line_weights { (width * self.scale).max(boost) } else { boost }
    }

    fn dashes(&self, dashes: &[f64]) -> Vec<f32> {
        if dashes.is_empty() {
            return Vec::new();
        }
        let total: f64 = dashes.iter().sum::<f64>() * self.scale;
        // 1 周期が 3 px より短い破線は、実線と見分けられないので実線で描く
        if total < 3.0 {
            return Vec::new();
        }
        let mut px: Vec<f32> = dashes.iter().map(|value| (value * self.scale).max(1.0) as f32).collect();
        // Canvas の setLineDash と同じく、奇数個は 2 回繰り返す
        if px.len() % 2 == 1 {
            px.extend_from_within(..);
        }
        px
    }

    fn flush(&mut self, mask: Option<&Mask>) {
        let Some(batch) = self.batch.take() else {
            return;
        };
        let Some(path) = batch.path.finish() else {
            return;
        };
        let mut paint = Paint::default();
        paint.set_color_rgba8(batch.color[0], batch.color[1], batch.color[2], batch.color[3]);
        let stroke = Stroke {
            width: batch.width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            dash: if batch.dashes.is_empty() { None } else { StrokeDash::new(batch.dashes, 0.0) },
            ..Stroke::default()
        };
        self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), mask);
    }

    fn stroke(&mut self, primitive: &StrokePrimitive, mask: Option<&Mask>) {
        if primitive.color == "transparent" {
            return;
        }
        let Some(color) = self.color(&primitive.color) else {
            return;
        };
        let width = self.line_width(primitive.width, 1.0) as f32;
        let dashes = self.dashes(&primitive.dashes);
        let key = (u32::from_be_bytes(color), width.to_bits(), dashes.iter().map(|value| value.to_bits()).collect());
        if self.batch.as_ref().is_none_or(|batch| batch.key != key) {
            self.flush(mask);
            self.batch = Some(Batch { key, color, width, dashes, path: PathBuilder::new() });
        }
        let points = &primitive.points;
        let mut segments = Vec::with_capacity(points.len() / 2);
        for [x, y] in points.as_chunks::<2>().0 {
            segments.push(self.to_px(*x, *y));
        }
        let path = &mut self.batch.as_mut().expect("batch").path;
        if primitive.segments == Some(true) {
            for [start, end] in segments.as_chunks::<2>().0 {
                path.move_to(start.0, start.1);
                path.line_to(end.0, end.1);
            }
            return;
        }
        let Some((first, rest)) = segments.split_first() else {
            return;
        };
        path.move_to(first.0, first.1);
        for point in rest {
            path.line_to(point.0, point.1);
        }
        if primitive.closed {
            path.close();
        }
    }

    fn rings_path(&self, rings: &[Vec<f64>]) -> Option<Path> {
        let mut path = PathBuilder::new();
        for ring in rings {
            if ring.len() < 6 {
                continue;
            }
            let (x, y) = self.to_px(ring[0], ring[1]);
            path.move_to(x, y);
            for [x, y] in ring[2..].as_chunks::<2>().0 {
                let (x, y) = self.to_px(*x, *y);
                path.line_to(x, y);
            }
            path.close();
        }
        path.finish()
    }

    fn fill_path(&mut self, path: &Path, color: [u8; 4], rule: FillRule) {
        let mut paint = Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
        self.pixmap.fill_path(path, &paint, rule, Transform::identity(), None);
    }

    fn primitive(&mut self, primitive: &Primitive, mask: Option<&Mask>) {
        match primitive {
            Primitive::Stroke(stroke) => self.stroke(stroke, mask),
            Primitive::Fill(fill) => {
                self.flush(mask);
                if let (Some(color), Some(path)) = (self.color(&fill.color), self.rings_path(&fill.rings)) {
                    self.fill_path(&path, color, FillRule::EvenOdd);
                }
            }
            Primitive::Clipped(clipped) => {
                self.flush(mask);
                let Some(path) = self.rings_path(&clipped.rings) else {
                    return;
                };
                let Some(mut clip) = Mask::new(self.pixmap.width(), self.pixmap.height()) else {
                    return;
                };
                clip.fill_path(&path, FillRule::EvenOdd, true, Transform::identity());
                for stroke in &clipped.strokes {
                    self.stroke(stroke, Some(&clip));
                }
                self.flush(Some(&clip));
            }
            Primitive::Dot(dot) => {
                self.flush(mask);
                let Some(color) = self.color(&dot.color) else {
                    return;
                };
                let (x, y) = self.to_px(dot.x, dot.y);
                let r = (1.5 * self.device_pixel_ratio) as f32;
                if let Some(rect) = Rect::from_xywh(x - r, y - r, r * 2.0, r * 2.0) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(color[0], color[1], color[2], color[3]);
                    self.pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
            Primitive::Text(text) => {
                self.flush(mask);
                self.text(text);
            }
        }
    }

    fn glyph(&mut self, character: char) -> Option<Rc<Glyph>> {
        let face = &self.face;
        let glyph = self.glyphs.entry(character).or_insert_with(|| {
            let id = face.glyph_index(character)?;
            let mut outline = Outline::default();
            face.outline_glyph(id, &mut outline);
            Some(Rc::new(Glyph {
                advance: f64::from(face.glyph_hor_advance(id).unwrap_or(0)),
                commands: outline.commands,
            }))
        });
        if glyph.is_none() {
            self.report.missing_glyphs.insert(character);
        }
        glyph.clone()
    }

    fn text(&mut self, primitive: &TextPrimitive) {
        let Some(color) = self.color(&primitive.color) else {
            return;
        };
        let m = &primitive.matrix;
        let b = &self.base;
        // 文字の座標 → 画像の px
        let t: Matrix = [
            b[0] * m[0] + b[2] * m[1],
            b[1] * m[0] + b[3] * m[1],
            b[0] * m[2] + b[2] * m[3],
            b[1] * m[2] + b[3] * m[3],
            b[0] * m[4] + b[2] * m[5] + b[4],
            b[1] * m[4] + b[3] * m[5] + b[5],
        ];
        let vertical = t[2].hypot(t[3]);
        if vertical * primitive.height / self.device_pixel_ratio < MIN_GLYPH_PX {
            // 小さい文字は、文字範囲の中央に線を引いて位置だけ示す
            let y = primitive.box_height / 2.0;
            let start = apply(&t, 0.0, y);
            let end = apply(&t, primitive.box_width, y);
            let width = (primitive.box_height * 0.4 * vertical).max(1.0) as f32;
            let mut path = PathBuilder::new();
            path.move_to(start.0, start.1);
            path.line_to(end.0, end.1);
            if let Some(path) = path.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(color[0], color[1], color[2], (f64::from(color[3]) * 0.6).round() as u8);
                let stroke = Stroke { width, line_cap: LineCap::Round, ..Stroke::default() };
                self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
            return;
        }
        let units_per_em = f64::from(self.face.units_per_em());
        // textBaseline = "middle" と同じく、em の枠の中央を文字の中心に合わせる
        let middle = (f64::from(self.face.ascender()) + f64::from(self.face.descender())) / 2.0;
        let sx = primitive.scale_x * primitive.height / units_per_em;
        let sy = primitive.height / units_per_em;
        let mut path = PathBuilder::new();
        for cell in &primitive.cells {
            let Some(character) = cell.character.chars().next() else {
                continue;
            };
            let Some(glyph) = self.glyph(character) else {
                continue;
            };
            // 字形の単位 → 文字の座標（字送りの中央と em の中央をセルの中心に置く）→ 画像の px
            let place = |x: f64, y: f64| apply(&t, cell.x + (x - glyph.advance / 2.0) * sx, cell.y + (y - middle) * sy);
            for command in &glyph.commands {
                match *command {
                    Command::MoveTo(x, y) => {
                        let p = place(x, y);
                        path.move_to(p.0, p.1);
                    }
                    Command::LineTo(x, y) => {
                        let p = place(x, y);
                        path.line_to(p.0, p.1);
                    }
                    Command::QuadTo(x1, y1, x, y) => {
                        let (c, p) = (place(x1, y1), place(x, y));
                        path.quad_to(c.0, c.1, p.0, p.1);
                    }
                    Command::CurveTo(x1, y1, x2, y2, x, y) => {
                        let (c1, c2, p) = (place(x1, y1), place(x2, y2), place(x, y));
                        path.cubic_to(c1.0, c1.1, c2.0, c2.1, p.0, p.1);
                    }
                    Command::Close => path.close(),
                }
            }
        }
        if let Some(path) = path.finish() {
            self.fill_path(&path, color, FillRule::Winding);
        }
    }

    fn sheet(&mut self, scene: &Scene) {
        let Some(sheet) = &scene.sheet else {
            return;
        };
        let mut path = PathBuilder::new();
        let corners = [(0.0, 0.0), (sheet.width, 0.0), (sheet.width, sheet.height), (0.0, sheet.height)];
        for (index, (x, y)) in corners.into_iter().enumerate() {
            let (x, y) = self.to_px(x, y);
            if index == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
        }
        path.close();
        let Some(path) = path.finish() else {
            return;
        };
        let mut paint = Paint::default();
        let alpha = (0.35f64 * 255.0).round() as u8;
        match self.theme {
            Background::Light => paint.set_color_rgba8(0, 0, 0, alpha),
            Background::Dark => paint.set_color_rgba8(255, 255, 255, alpha),
        }
        let stroke = Stroke { width: 1.0, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Stroke::default() };
        self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn apply(m: &Matrix, x: f64, y: f64) -> (f32, f32) {
    ((m[0] * x + m[2] * y + m[4]) as f32, (m[1] * x + m[3] * y + m[5]) as f32)
}

#[derive(Debug, Clone, Copy)]
enum Command {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    QuadTo(f64, f64, f64, f64),
    CurveTo(f64, f64, f64, f64, f64, f64),
    Close,
}

/// 字形の輪郭（フォントの単位、Y 軸は上向き）と字送り
#[derive(Debug, Clone)]
struct Glyph {
    advance: f64,
    commands: Vec<Command>,
}

#[derive(Default)]
struct Outline {
    commands: Vec<Command>,
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::MoveTo(f64::from(x), f64::from(y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::LineTo(f64::from(x), f64::from(y)));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(Command::QuadTo(f64::from(x1), f64::from(y1), f64::from(x), f64::from(y)));
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(Command::CurveTo(
            f64::from(x1),
            f64::from(y1),
            f64::from(x2),
            f64::from(y2),
            f64::from(x),
            f64::from(y),
        ));
    }

    fn close(&mut self) {
        self.commands.push(Command::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zumen_core::{Primitive, SceneLayer, StrokePrimitive};

    fn line(id: &str, layers: Vec<u32>, points: [f64; 4]) -> SceneEntity {
        let [x0, y0, x1, y1] = points;
        SceneEntity {
            id: id.into(),
            kind: zumen_core::EntityKind::Line,
            type_name: "線分".into(),
            source: "test".into(),
            layers,
            primitives: vec![Primitive::Stroke(StrokePrimitive {
                points: points.to_vec(),
                closed: false,
                segments: None,
                color: "#000000".into(),
                width: 0.0,
                dashes: Vec::new(),
            })],
            bbox: BBox { min_x: x0.min(x1), min_y: y0.min(y1), max_x: x0.max(x1), max_y: y0.max(y1) },
            properties: Vec::new(),
            placement: None,
        }
    }

    fn scene() -> Scene {
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: None,
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 100.0, max_y: 50.0 },
            layers: vec![
                SceneLayer { id: 0, name: "表示".into(), visible: true, group: None },
                SceneLayer { id: 1, name: "非表示".into(), visible: false, group: None },
            ],
            underlays: Vec::new(),
            entities: vec![
                line("a", vec![0], [0.0, 0.0, 100.0, 50.0]),
                line("b", vec![1], [10.0, 10.0, 20.0, 10.0]),
                line("c", vec![0], [80.0, 40.0, 90.0, 45.0]),
            ],
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    #[test]
    fn converts_between_pixels_and_paper_millimeters() {
        // 用紙の (0, 0)〜(100, 50) を 1 mm = 2 px で描く
        let (view, width, height) = View::for_region(&scene().bounds, 50.8).unwrap();
        assert_eq!((view.scale, width, height), (2.0, 200, 100));
        assert_eq!(view.to_pixel(0.0, 50.0), (0.0, 0.0));
        assert_eq!(view.to_pixel(100.0, 0.0), (200.0, 100.0));
        assert_eq!(view.to_paper(50.0, 25.0), (25.0, 37.5));
        assert_eq!(view.visible_region(width, height), scene().bounds);
        let inside = BBox { min_x: 10.0, min_y: 10.0, max_x: 20.0, max_y: 15.0 };
        assert_eq!(
            view.pixel_box(&inside, width, height),
            Some(PixelBox { left: 20.0, top: 70.0, right: 40.0, bottom: 80.0 })
        );
        // 画像からはみ出す部分は切り、重ならなければ None
        let across = BBox { min_x: -10.0, min_y: 40.0, max_x: 10.0, max_y: 60.0 };
        assert_eq!(
            view.pixel_box(&across, width, height),
            Some(PixelBox { left: 0.0, top: 0.0, right: 20.0, bottom: 20.0 })
        );
        let outside = BBox { min_x: 120.0, min_y: 0.0, max_x: 130.0, max_y: 10.0 };
        assert_eq!(view.pixel_box(&outside, width, height), None);
    }

    #[test]
    fn reports_the_entities_it_draws() {
        let scene = scene();
        let ids = |entities: Vec<&SceneEntity>| entities.into_iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        let options = RenderOptions::default();
        let image =
            render_image(&scene, None, ImageSize::Fit { width: 200, height: 100, margin: 0.0 }, &options).unwrap();
        assert!(image.png.starts_with(b"\x89PNG"));
        assert_eq!((image.width, image.height, image.view.scale), (200, 100, 2.0));
        assert_eq!(ids(drawn_entities(&scene, &image.view, image.width, image.height, &options)), ["a", "c"]);
        // 非表示のレイヤも描くとき
        let all = RenderOptions { hidden_layers: Some(HashSet::new()), ..RenderOptions::default() };
        assert_eq!(ids(drawn_entities(&scene, &image.view, image.width, image.height, &all)), ["a", "b", "c"]);
        // 範囲を指定すると、その範囲に写る図形だけ
        let part = BBox { min_x: 0.0, min_y: 0.0, max_x: 30.0, max_y: 20.0 };
        let image = render_image(&scene, Some(part), ImageSize::Dpi(25.4), &all).unwrap();
        assert_eq!((image.width, image.height), (30, 20));
        assert_eq!(ids(drawn_entities(&scene, &image.view, image.width, image.height, &all)), ["a", "b"]);
    }
}

#[cfg(test)]
mod vector_tests {
    use super::*;
    use zumen_core::geometry::placement;
    use zumen_core::text::{TextLayoutInput, layout_text};
    use zumen_core::{ClippedPrimitive, DotPrimitive, FillPrimitive, SceneLayer, Sheet, Underlay, UnderlayKind};

    fn entity(id: &str, layers: Vec<u32>, primitives: Vec<Primitive>) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: zumen_core::EntityKind::Line,
            type_name: "test".into(),
            source: "test".into(),
            layers,
            primitives,
            bbox: BBox { min_x: 0.0, min_y: 0.0, max_x: 420.0, max_y: 297.0 },
            properties: Vec::new(),
            placement: None,
        }
    }

    fn stroke(points: &[f64], color: &str) -> StrokePrimitive {
        StrokePrimitive {
            points: points.to_vec(),
            closed: false,
            segments: None,
            color: color.into(),
            width: 0.35,
            dashes: Vec::new(),
        }
    }

    /// A3 の用紙に、線・半透明の塗り・切り取ったハッチング・点・文字と、非表示のレイヤの線
    fn sheet_scene(text: &str) -> Scene {
        let label = layout_text(
            TextLayoutInput {
                text,
                x: 100.0,
                y: 100.0,
                height: 5.0,
                width: 0.0,
                spacing: 0.0,
                angle: 0.0,
                slant: 0.0,
                anchor: 1.0,
                vertical: false,
                color: "#000000".into(),
                font_family: "sans-serif".into(),
            },
            &placement(0.0, 0.0, 0.0, 1.0, 1.0),
        )
        .expect("text");
        let ring = vec![10.0, 10.0, 60.0, 10.0, 60.0, 60.0, 10.0, 60.0];
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: Some(Sheet { name: "平面図".into(), size: "A3".into(), width: 420.0, height: 297.0 }),
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 420.0, max_y: 297.0 },
            layers: vec![
                SceneLayer { id: 0, name: "0".into(), visible: true, group: None },
                SceneLayer { id: 1, name: "非表示".into(), visible: false, group: None },
            ],
            underlays: Vec::new(),
            entities: vec![
                entity("1", vec![0], vec![Primitive::Stroke(stroke(&[0.0, 0.0, 420.0, 297.0], "#ff0000"))]),
                entity(
                    "2",
                    vec![0],
                    vec![Primitive::Fill(FillPrimitive { color: "#0000ff80".into(), rings: vec![ring.clone()] })],
                ),
                entity(
                    "3",
                    vec![0],
                    vec![Primitive::Clipped(ClippedPrimitive {
                        rings: vec![ring],
                        strokes: vec![stroke(&[0.0, 0.0, 100.0, 100.0], "#00ff00")],
                    })],
                ),
                entity(
                    "4",
                    vec![0],
                    vec![Primitive::Dot(DotPrimitive { x: 200.0, y: 200.0, color: "#000000".into() })],
                ),
                entity("5", vec![0], vec![Primitive::Text(label)]),
                entity("6", vec![1], vec![Primitive::Stroke(stroke(&[1.0, 2.0, 3.0, 4.0], "#123456"))]),
            ],
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    #[test]
    fn writes_svg_in_paper_millimeters_with_searchable_text() {
        let scene = sheet_scene("平面図");
        let (svg, report) = render_svg(&scene, None, &RenderOptions::default()).unwrap();
        assert!(report.notices.is_empty());
        assert!(svg.starts_with(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"420mm\" height=\"297mm\" viewBox=\"0 0 420 297\">"
        ));
        assert!(svg.contains("stroke=\"#ff0000\" stroke-width=\"0.35\""));
        assert!(svg.contains("fill=\"#0000ff\" fill-rule=\"evenodd\" fill-opacity=\"0.502\""));
        assert!(svg.contains("<clipPath id=\"c1\">") && svg.contains("clip-path=\"url(#c1)\""));
        assert!(svg.contains(">平</tspan>") && svg.contains(">図</tspan>"));
        // 非表示のレイヤは描かず、ページが用紙そのものなら用紙の枠も描かない
        assert!(!svg.contains("#123456") && !svg.contains("stroke-opacity=\"0.35\""));
        // 範囲を指定すると、その範囲がページになり、用紙の枠を描く
        let part = render_svg(
            &scene,
            Some(BBox { min_x: -10.0, min_y: -10.0, max_x: 90.0, max_y: 40.0 }),
            &RenderOptions::default(),
        )
        .unwrap()
        .0;
        assert!(part.contains("viewBox=\"0 0 100 50\"") && part.contains("stroke-opacity=\"0.35\""));
        // 細い線は 1 px（0.265 mm）より細くしない
        let (thin, _) =
            render_svg(&scene, None, &RenderOptions { line_weights: false, ..RenderOptions::default() }).unwrap();
        assert!(thin.contains("stroke=\"#ff0000\" stroke-width=\"0.265\""));
    }

    #[test]
    fn reports_the_underlays_it_does_not_draw() {
        let underlay = |id: &str, layers: Vec<u32>, min_x: f64| Underlay {
            id: id.into(),
            kind: UnderlayKind::Image,
            file: Some(format!("{id}.png")),
            page: None,
            width: 10.0,
            height: 10.0,
            matrix: [1.0, 0.0, 0.0, -1.0, min_x, 10.0],
            clip: None,
            opacity: None,
            layers,
            content_layers: None,
            bbox: BBox { min_x, min_y: 0.0, max_x: min_x + 10.0, max_y: 10.0 },
        };
        // レイヤ 1 は非表示
        let mut scene = sheet_scene("図");
        scene.underlays = vec![
            underlay("shown", vec![0], 0.0),
            underlay("hidden", vec![1], 20.0),
            underlay("outside", vec![0], 500.0),
        ];
        let options = RenderOptions::default();
        let count = |report: &RenderReport| {
            report.notices.iter().find(|notice| notice.code == "underlay-not-drawn").map(|notice| notice.count)
        };
        // 範囲に写り、表示するレイヤにある underlay だけを数える
        let sheet = Some(BBox { min_x: 0.0, min_y: 0.0, max_x: 420.0, max_y: 297.0 });
        let (_, png) = render_png(&scene, sheet, 2.54, &options).unwrap();
        let (_, svg) = render_svg(&scene, None, &options).unwrap();
        let (_, pdf) = render_pdf(&scene, None, &options).unwrap();
        for report in [&png, &svg, &pdf] {
            assert_eq!(count(report), Some(1));
        }
        let all = RenderOptions { hidden_layers: Some(HashSet::new()), ..RenderOptions::default() };
        let wide = Some(BBox { min_x: 0.0, min_y: 0.0, max_x: 600.0, max_y: 297.0 });
        assert_eq!(count(&render_svg(&scene, wide, &all).unwrap().1), Some(3));
        let away = Some(BBox { min_x: 50.0, min_y: 20.0, max_x: 60.0, max_y: 30.0 });
        assert_eq!(count(&render_png(&scene, away, 25.4, &options).unwrap().1), None);
    }

    #[test]
    fn writes_a_pdf_page_of_the_sheet_with_an_embedded_font_subset() {
        let scene = sheet_scene("平面図😀");
        let (pdf, report) = render_pdf(&scene, None, &RenderOptions::default()).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.7"));
        assert!(text.contains("/MediaBox [0 0 1190.5511 841.8898]"), "{}", &text[..400.min(text.len())]);
        for key in [
            "/Type0",
            "/CIDFontType2",
            "/Identity-H",
            "/FontFile2",
            "/ToUnicode",
            "+BIZUDGothic-Regular",
            "/FlateDecode",
        ] {
            assert!(text.contains(key), "{key}");
        }
        assert!(text.contains("/Producer (zumen") && text.contains("/Title"));
        assert_eq!(report.missing_glyphs.into_iter().collect::<String>(), "😀");
        // 文字がなければフォントを埋め込まない
        let mut plain = sheet_scene("x");
        plain.entities.retain(|entity| entity.id != "5");
        let (pdf, _) = render_pdf(&plain, None, &RenderOptions::default()).unwrap();
        assert!(!String::from_utf8_lossy(&pdf).contains("/FontFile2"));
        assert!(
            render_pdf(
                &plain,
                Some(BBox { min_x: 0.0, min_y: 0.0, max_x: 0.0, max_y: 10.0 }),
                &RenderOptions::default()
            )
            .is_err()
        );
    }
}
