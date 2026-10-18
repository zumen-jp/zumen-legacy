//! zumen の表示用モデル（`Scene`）を PNG に描く。
//!
//! ブラウザの描画（`@zumenjs/core` の render.ts）と同じ規則で描く: 背景と見分けにくい色の反転、
//! 線の太さと 1 デバイス px の下限、短すぎる破線の実線化、偶奇規則の塗り、領域で切り取った線、
//! 文字の 1 文字ごとの配置と、小さすぎる文字の代わりの線。
//! 文字は同梱の BIZ UDゴシック（SIL Open Font License 1.1）で描く。座標の変換は f64 で行い、
//! 測量座標のような大きな値でも位置がずれないようにする。
#![forbid(unsafe_code)]

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::rc::Rc;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, StrokeDash, Transform,
};
use ttf_parser::{Face, OutlineBuilder};
use zumen_core::{
    BACKGROUND_COLOR, BBox, Background, Matrix, Primitive, Scene, SceneEntity, StrokePrimitive, TextPrimitive,
};

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
    let hidden: HashSet<u32> = match &options.hidden_layers {
        Some(hidden) => hidden.clone(),
        None => scene.layers.iter().filter(|layer| !layer.visible).map(|layer| layer.id).collect(),
    };
    let background = match theme {
        Background::Dark => (0, 0, 0),
        Background::Light => (255, 255, 255),
    };
    pixmap.fill(Color::from_rgba8(background.0, background.1, background.2, 255));
    let mut painter = Painter {
        pixmap: &mut pixmap,
        base: [view.scale, 0.0, 0.0, -view.scale, view.offset_x, view.offset_y],
        scale: view.scale,
        theme,
        background,
        line_weights: options.line_weights,
        device_pixel_ratio: options.device_pixel_ratio.max(1e-6),
        face,
        glyphs: HashMap::new(),
        report: RenderReport::default(),
        batch: None,
    };
    let visible = BBox {
        min_x: (0.0 - view.offset_x) / view.scale,
        min_y: (view.offset_y - f64::from(height)) / view.scale,
        max_x: (f64::from(width) - view.offset_x) / view.scale,
        max_y: view.offset_y / view.scale,
    };
    if options.sheet_frame {
        painter.sheet(scene);
    }
    for entity in &scene.entities {
        if !intersects(&entity.bbox, &visible) || !is_entity_visible(entity, &hidden) {
            continue;
        }
        for primitive in &entity.primitives {
            painter.primitive(primitive, None);
        }
    }
    painter.flush(None);
    let report = painter.report;
    Ok((pixmap, report))
}

/// 図面の範囲 region（用紙の mm、None なら図面全体）を dpi で描き、PNG にする
pub fn render_png(
    scene: &Scene,
    region: Option<BBox>,
    dpi: f64,
    options: &RenderOptions,
) -> Result<(Vec<u8>, RenderReport), RenderError> {
    let region = region.unwrap_or(scene.bounds);
    let (view, width, height) = View::for_region(&region, dpi)?;
    let (pixmap, report) = render(scene, &view, width, height, options)?;
    let png = pixmap.encode_png().map_err(|error| RenderError::new(format!("PNG を作れません: {error}")))?;
    Ok((png, report))
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
    let region = region.unwrap_or(scene.bounds);
    let view = View::fit(&region, f64::from(width), f64::from(height), margin);
    let (pixmap, report) = render(scene, &view, width, height, options)?;
    let png = pixmap.encode_png().map_err(|error| RenderError::new(format!("PNG を作れません: {error}")))?;
    Ok((png, report))
}

/// TypeScript の isEntityVisible と同じ: 自分か配置元のレイヤが 1 つでも非表示なら描かない
pub fn is_entity_visible(entity: &SceneEntity, hidden: &HashSet<u32>) -> bool {
    hidden.is_empty() || !entity.layers.iter().any(|layer| hidden.contains(layer))
}

fn intersects(b: &BBox, visible: &BBox) -> bool {
    !(b.max_x < visible.min_x || b.min_x > visible.max_x || b.max_y < visible.min_y || b.min_y > visible.max_y)
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
    background: (u8, u8, u8),
    line_weights: bool,
    device_pixel_ratio: f64,
    face: Face<'static>,
    glyphs: HashMap<char, Option<Rc<Glyph>>>,
    report: RenderReport,
    batch: Option<Batch>,
}

impl Painter<'_> {
    /// 背景と見分けにくい色（白背景の白に近い色、黒背景の黒に近い色）を反対の色にする。不透明度は保つ
    fn color(&self, color: &str) -> Option<[u8; 4]> {
        if color == BACKGROUND_COLOR {
            return Some([self.background.0, self.background.1, self.background.2, 255]);
        }
        let [r, g, b, a] = parse_color(color)?;
        let value = (f64::from(r) * 0.299 + f64::from(g) * 0.587 + f64::from(b) * 0.114) / 255.0;
        Some(match self.theme {
            Background::Light if value > 0.85 => [0, 0, 0, a],
            Background::Dark if value < 0.05 => [255, 255, 255, a],
            _ => [r, g, b, a],
        })
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
