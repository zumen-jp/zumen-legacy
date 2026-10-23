//! hayro-interpret が塗る内容（経路・字形の並び・画像）を図形にする
//!
//! 図形の id は `#n` で、n はページの中で塗った順の経路・字形の並び・画像の番号（1 から。フォーム XObject の中も
//! 数える）。塗りと線を両方持つ経路（B 演算子）と文字（描画モード 2）を hayro-interpret は塗りと線の 2 回に分けて
//! 渡すので、塗りの直後に同じ形の線が来たら 1 つの図形にまとめ、番号も 1 つにする。読まない内容（シェーディング、
//! パターンの塗り、切り抜きの外、空白だけの文字）も番号を使うので、後の版で読むようになっても番号はずれない。

use crate::fonts::Fonts;
use hayro_interpret::font::{Glyph, GlyphRun};
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::pattern::Pattern;
use hayro_interpret::{
    BlendMode, ClipPath, Device, DrawMode, DrawProps, Image, ImageDrawProps, Paint, SoftMask, StrokeProps,
};
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape, Vec2};
use zumen_core::geometry::{empty_box, extend_box, extend_box_with_points};
use zumen_core::text::font_family_for;
use zumen_core::{
    BBox, EntityKind, FillPrimitive, GlyphCell, Notice, NoticeLevel, NoticeList, Primitive, Property, SceneEntity,
    StrokePrimitive, TextPrimitive, js, limits,
};

/// 曲線を折線にする許容差（用紙の mm）
const TOLERANCE: f64 = 0.01;

/// ページのレイヤ（optional content をまだ分けないので 1 つ）
pub(crate) const LAYER: u32 = 0;
const LAYER_NAME: &str = "ページ";

/// 画像の枠の色（図形は描かないので、選んだときの強調にだけ使う）
const FRAME_COLOR: &str = "#808080";

/// 字送りの分からない字形（Type 3 フォントなど）の、次の字形がないときの字送り（em の 1/1000）
const DEFAULT_ADVANCE: f64 = 500.0;

/// 塗りの種類
#[derive(Debug, Clone, Copy, PartialEq)]
enum Ink {
    /// 色。sRGB と不透明度（0〜255）
    Color([u8; 4]),
    /// シェーディング（グラデーション、8.7.4.3）
    Shading,
    /// タイリングパターン（8.7.3）
    Tiling,
}

impl Ink {
    fn of(paint: &Paint<'_>) -> Self {
        match paint {
            Paint::Color(color) => Ink::Color(color.to_rgba().to_rgba8()),
            Paint::Pattern(pattern) => match **pattern {
                Pattern::Shading(_) => Ink::Shading,
                Pattern::Tiling(_) => Ink::Tiling,
            },
        }
    }
}

/// 線の色と、用紙の mm の太さ・破線
#[derive(Debug, Clone, PartialEq)]
struct Line {
    ink: Ink,
    width: f64,
    dashes: Vec<f64>,
}

impl Line {
    fn of(props: &DrawProps<'_>, stroke: &StrokeProps) -> Self {
        // 線の太さと破線はユーザー空間の長さ。用紙の mm には変換の平均の倍率を掛ける
        let scale = props.transform.determinant().abs().sqrt();
        let width = f64::from(stroke.line_width) * scale;
        let dashes: Vec<f64> = stroke.dash_array.iter().map(|length| f64::from(*length) * scale).collect();
        // 長さがすべて 0 の破線は実線とみなす（8.4.3.6）
        let dashes = if dashes.iter().any(|length| *length > 0.0) { dashes } else { Vec::new() };
        Self { ink: Ink::of(&props.paint), width: if width.is_finite() { width } else { 0.0 }, dashes }
    }
}

/// 字形の並びの 1 字形。用紙の mm への変換は字形の座標（em の 1/1000）から
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    transform: Affine,
    text: Option<String>,
    /// 字送り（em の 1/1000）
    advance: Option<f64>,
    /// フォントを見分ける値（Type 3 は None）
    font: Option<u128>,
}

#[derive(Debug, Clone, PartialEq)]
enum Figure {
    /// 用紙の mm の経路
    Path(BezPath),
    Run(Vec<Placed>),
}

/// 塗った内容（塗りの直後の線を待つ間、ここに置く）
struct Painted {
    number: u64,
    figure: Figure,
    fill: Option<Ink>,
    stroke: Option<Line>,
    /// 描画モード 3（見えない文字。スキャンに重ねた OCR の文字など）
    invisible: bool,
    /// 塗ったときの切り抜きの外接矩形
    clip: Rect,
}

pub(crate) struct Collector<'r> {
    fonts: Fonts<'r>,
    /// 切り抜きの外接矩形の重なり。最初はページ
    clips: Vec<Rect>,
    count: u64,
    pending: Option<Painted>,
    entities: Vec<SceneEntity>,
    notices: NoticeList,
}

impl<'r> Collector<'r> {
    pub(crate) fn new(fonts: Fonts<'r>, page: Rect) -> Self {
        Self { fonts, clips: vec![page], count: 0, pending: None, entities: Vec::new(), notices: NoticeList::new() }
    }

    pub(crate) fn finish(mut self) -> (Vec<SceneEntity>, Vec<Notice>) {
        self.flush();
        (self.entities, self.notices.into_vec())
    }

    fn clip(&self) -> Rect {
        *self.clips.last().expect("the page clip")
    }

    /// 新しく塗った内容に番号を付ける。前に待たせていたものは図形にする
    fn begin(&mut self, figure: Figure, fill: Option<Ink>, stroke: Option<Line>, invisible: bool) {
        self.flush();
        self.count += 1;
        self.pending = Some(Painted { number: self.count, figure, fill, stroke, invisible, clip: self.clip() });
    }

    /// 塗りの直後の同じ形の線なら、塗りと 1 つの図形にする
    fn join(&mut self, figure: &Figure, line: &Line) -> bool {
        // Separate paints can change the clip (ISO 32000-1:2008, 8.5.4); only paints in the same clip can join.
        let clip = self.clip();
        match &mut self.pending {
            Some(painted)
                if painted.fill.is_some()
                    && painted.stroke.is_none()
                    && painted.figure == *figure
                    && painted.clip == clip =>
            {
                painted.stroke = Some(line.clone());
                true
            }
            _ => false,
        }
    }

    fn paint(&mut self, figure: Figure, props: &DrawProps<'_>, mode: &DrawMode) {
        match mode {
            DrawMode::Fill(_) => self.begin(figure, Some(Ink::of(&props.paint)), None, false),
            DrawMode::Stroke(stroke) => {
                let line = Line::of(props, stroke);
                if !self.join(&figure, &line) {
                    self.begin(figure, None, Some(line), false);
                }
                self.flush();
            }
            DrawMode::FillAndStroke(_, stroke) => {
                self.begin(figure, Some(Ink::of(&props.paint)), Some(Line::of(props, stroke)), false);
                self.flush();
            }
            DrawMode::Invisible => {
                self.begin(figure, Some(Ink::of(&props.paint)), None, true);
                self.flush();
            }
        }
    }

    /// 待たせていた内容を図形にする
    fn flush(&mut self) {
        let Some(painted) = self.pending.take() else { return };
        let id = format!("#{}", painted.number);
        let entity = match painted.figure {
            Figure::Path(ref path) => self.path_entity(id, path, &painted),
            Figure::Run(ref run) => self.text_entity(id, run, &painted),
        };
        let Some(entity) = entity else { return };
        if outside(&entity.bbox, &painted.clip) {
            self.notices.add(
                NoticeLevel::Info,
                "clipped-out",
                "切り抜きやページの外にあって見えない内容は読みませんでした",
            );
            return;
        }
        // 読み取りの上限（zumen_core::limits）。上限を超えたら図形を加えない
        if limits::admit() {
            self.entities.push(entity);
        }
    }

    /// 塗りと線の色のうち、読まないもの（シェーディング、パターン）を notices に数えて除く
    fn color(&mut self, ink: Option<Ink>) -> Option<[u8; 4]> {
        match ink? {
            Ink::Color(color) => Some(color),
            Ink::Shading => {
                self.notices.add(
                    NoticeLevel::Info,
                    "shading-not-read",
                    "シェーディング（グラデーション）の塗りは読みませんでした",
                );
                None
            }
            Ink::Tiling => {
                self.notices.add(NoticeLevel::Info, "pattern-not-read", "タイリングパターンの塗りは読みませんでした");
                None
            }
        }
    }

    /// 経路（8.5）。線を引くなら線分（直線 1 本）・折線（直線だけ）・曲線（ベジェ曲線を含む）、塗るだけなら塗り
    fn path_entity(&mut self, id: String, path: &BezPath, painted: &Painted) -> Option<SceneEntity> {
        let fill = self.color(painted.fill);
        let stroke = painted.stroke.as_ref().and_then(|line| self.color(Some(line.ink)).map(|color| (color, line)));
        if fill.is_none() && stroke.is_none() {
            return None;
        }
        let subpaths: Vec<Subpath> = subpaths(path).into_iter().filter(|subpath| subpath.segments > 0).collect();
        let mut primitives = Vec::new();
        if let Some(color) = fill {
            let rings: Vec<Vec<f64>> = subpaths
                .iter()
                .filter(|subpath| subpath.points.len() >= 6)
                .map(|subpath| subpath.points.clone())
                .collect();
            if !rings.is_empty() {
                primitives.push(Primitive::Fill(FillPrimitive { color: hex(color), rings }));
            }
        }
        if let Some((color, line)) = stroke {
            let segments =
                subpaths.len() > 1 && subpaths.iter().all(|subpath| subpath.points.len() == 4 && !subpath.closed);
            let stroke = |points: Vec<f64>, closed: bool, segments: bool| {
                Primitive::Stroke(StrokePrimitive {
                    points,
                    closed,
                    segments: segments.then_some(true),
                    color: hex(color),
                    width: line.width,
                    dashes: line.dashes.clone(),
                })
            };
            if segments {
                primitives.push(stroke(
                    subpaths.iter().flat_map(|subpath| subpath.points.clone()).collect(),
                    false,
                    true,
                ));
            } else {
                for subpath in &subpaths {
                    primitives.push(stroke(subpath.points.clone(), subpath.closed, false));
                }
            }
        }
        if primitives.is_empty() {
            return None;
        }
        let curved = subpaths.iter().any(|subpath| subpath.curved);
        let (kind, type_name) = match stroke {
            None => (EntityKind::Solid, "塗り"),
            Some(_) if curved => (EntityKind::Spline, "曲線"),
            Some(_) if subpaths.len() == 1 && subpaths[0].segments == 1 && !subpaths[0].closed => {
                (EntityKind::Line, "線分")
            }
            Some(_) => (EntityKind::Polyline, "折線"),
        };
        let mut bbox = empty_box();
        for subpath in &subpaths {
            extend_box_with_points(&mut bbox, &subpath.points);
        }
        let mut properties = common(type_name, &id);
        match (fill, stroke) {
            (Some(color), None) => properties.push(Property::new("color", "色", hex(color))),
            (fill, Some((color, line))) => {
                properties.push(Property::new("color", "色", hex(color)));
                if let Some(fill) = fill {
                    properties.push(Property::new("fillColor", "塗りの色", hex(fill)));
                }
                properties
                    .push(Property::new("lineWidth", "線の太さ", format!("{} mm", number(line.width))).raw(line.width));
                if !line.dashes.is_empty() {
                    let dashes: Vec<String> = line.dashes.iter().map(|length| number(*length)).collect();
                    properties.push(Property::new("lineType", "線種", format!("破線（{} mm）", dashes.join(", "))));
                }
            }
            (None, None) => unreachable!("returned above"),
        }
        let length: f64 = subpaths.iter().map(Subpath::length).sum();
        match kind {
            EntityKind::Line => {
                let points = &subpaths[0].points;
                properties.push(point("start", "始点", points[0], points[1]));
                properties.push(point("end", "終点", points[2], points[3]));
                properties.push(Property::new("length", "長さ", number(length)).raw(length));
            }
            EntityKind::Polyline => {
                let count: usize = subpaths.iter().map(|subpath| subpath.points.len() / 2).sum();
                let closed = subpaths.iter().all(|subpath| subpath.closed);
                properties.push(
                    Property::new(
                        "vertexCount",
                        "頂点",
                        format!("{count} 点{}", if closed { "（閉じる）" } else { "" }),
                    )
                    .raw(count as f64),
                );
                properties.push(Property::new("length", "長さ", number(length)).raw(length));
            }
            EntityKind::Spline => properties.push(Property::new("length", "長さ", number(length)).raw(length)),
            _ => {}
        }
        Some(entity(id, kind, type_name, "path", primitives, bbox, properties))
    }

    /// 字形の並び（9.4）。並びの最初の字形のベースラインを文字の座標系の x 軸にし、字形ごとに Unicode（9.10）と
    /// 字送りの中央・em の中央にセルを置く
    fn text_entity(&mut self, id: String, run: &[Placed], painted: &Painted) -> Option<SceneEntity> {
        let first = run.first()?;
        let missing = run.iter().filter(|glyph| glyph.text.is_none()).count();
        if missing > 0 {
            self.notices.add_count(
                NoticeLevel::Warning,
                "glyph-without-unicode",
                "Unicode の分からない字形は U+FFFD にしました",
                missing as u64,
            );
        }
        let texts: Vec<String> =
            run.iter().map(|glyph| glyph.text.clone().unwrap_or_else(|| "\u{fffd}".into())).collect();
        let text = texts.concat();
        if text.chars().all(js::is_whitespace) {
            return None;
        }
        let font = self.fonts.get(first.font).clone();
        let color = [painted.fill, painted.stroke.as_ref().map(|line| line.ink)]
            .into_iter()
            .flatten()
            .find_map(|ink| if let Ink::Color(color) = ink { Some(color) } else { None })
            .unwrap_or([0, 0, 0, 255]);

        // 文字の座標系: u がベースラインの向き、v がそれに直交する em の上向き（鏡に映した文字では向きが変わる）
        let em_x = vector(&first.transform, 1000.0, 0.0);
        let em_y = vector(&first.transform, 0.0, 1000.0);
        let width = em_x.hypot();
        let cross = em_x.cross(em_y);
        if !(width > 0.0 && cross.is_finite() && cross != 0.0) {
            return None;
        }
        let u = em_x / width;
        let v = Vec2::new(-u.y, u.x) * cross.signum();
        // em の高さ（ベースラインに直交する向き）と、横の倍率（水平の伸縮 Tz と、字形の変換の歪み）
        let height = cross.abs() / width;
        let scale_x = width / height;
        let (ascent, descent) = (font.ascent / 1000.0 * height, font.descent / 1000.0 * height);
        let baseline = first.transform * Point::ZERO;
        // 文字範囲の左下（ベースラインの始めから em の下端へ）が文字の座標系の原点
        let origin = baseline + v * descent;
        let local = |point: Point| {
            let offset = point - origin;
            (offset.dot(u), offset.dot(v))
        };
        let mut cells = Vec::with_capacity(run.len());
        let mut right: f64 = 0.0;
        // em の中央の高さ（字形の座標、em の 1/1000）
        let middle = (font.ascent + font.descent) / 2.0;
        for (index, (glyph, text)) in run.iter().zip(&texts).enumerate() {
            // 字送り（字形の座標）。分からなければ次の字形までの距離
            let advance = glyph.advance.unwrap_or_else(|| match run.get(index + 1) {
                Some(next) => (glyph.transform.inverse() * (next.transform * Point::ZERO)).x,
                None => DEFAULT_ADVANCE,
            });
            // セルの中心は字送りの中央と em の中央。字形の変換で写すので、斜めの文字や水平の伸縮もそのまま
            let (x, y) = local(glyph.transform * Point::new(advance / 2.0, middle));
            right = right.max(local(glyph.transform * Point::new(advance, 0.0)).0);
            cells.push(GlyphCell { character: text.clone(), x, y });
        }
        let box_height = ascent - descent;
        let matrix = [u.x, u.y, v.x, v.y, origin.x, origin.y];
        let mut bbox = empty_box();
        for (x, y) in [(0.0, 0.0), (right, 0.0), (right, box_height), (0.0, box_height)] {
            let point = origin + u * x + v * y;
            extend_box(&mut bbox, point.x, point.y);
        }
        let rotation = js::atan2(u.y, u.x).to_degrees();
        let mut properties = common("文字", &id);
        properties.push(Property::new("color", "色", hex(color)));
        properties.push(Property::new("text", "文字", text.clone()));
        if !font.name.is_empty() {
            properties.push(Property::new("font", "フォント", font.name.clone()));
        }
        properties.push(Property::new("height", "文字の大きさ", format!("{} mm", number(height))).raw(height));
        properties.push(point("position", "位置", baseline.x, baseline.y));
        properties.push(Property::new("rotation", "角度", format!("{}°", number(rotation))).raw(rotation));
        if painted.invisible {
            properties.push(Property::new("visibility", "表示", "見えない文字（描画モード 3）"));
        }
        let primitive = TextPrimitive {
            text,
            color: hex(color),
            font_family: font_family_for(&font.name).to_string(),
            height,
            character_height: None,
            scale_x,
            cells,
            box_width: right.max(0.0),
            box_height,
            matrix,
        };
        Some(entity(id, EntityKind::Text, "文字", "text", vec![Primitive::Text(primitive)], bbox, properties))
    }
}

impl<'a> Device<'a> for Collector<'_> {
    fn draw_path(&mut self, path: &BezPath, props: DrawProps<'a>, draw_mode: &DrawMode) {
        if limits::tick() {
            return;
        }
        self.paint(Figure::Path(props.transform * path.clone()), &props, draw_mode);
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        let clip = self.clip().intersect(clip_path.path.bounding_box());
        self.clips.push(clip);
    }

    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}

    fn draw_glyph_run(&mut self, glyph_run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, draw_mode: &DrawMode) {
        if limits::tick() {
            return;
        }
        let run = glyph_run
            .glyphs()
            .iter()
            .map(|glyph| {
                let (advance, font) = match &**glyph {
                    Glyph::Outline(outline) => (outline.advance_width().map(f64::from), Some(outline.font_cache_key())),
                    Glyph::Type3(_) => (None, None),
                };
                Placed {
                    transform: props.transform * glyph.transform(),
                    text: glyph.as_unicode().map(|text| match text {
                        BfString::Char(character) => character.to_string(),
                        BfString::String(text) => text,
                    }),
                    advance,
                    font,
                }
            })
            .collect();
        self.paint(Figure::Run(run), &props, draw_mode);
    }

    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        if limits::tick() {
            return;
        }
        self.flush();
        self.count += 1;
        let id = format!("#{}", self.count);
        // 画像の変換は画素の座標（左上が原点、y は下向き）から（8.9.5）
        let (width, height) = (f64::from(image.width()), f64::from(image.height()));
        let corners: Vec<Point> = [(0.0, height), (width, height), (width, 0.0), (0.0, 0.0)]
            .into_iter()
            .map(|(x, y)| props.transform * Point::new(x, y))
            .collect();
        let points: Vec<f64> = corners.iter().flat_map(|point| [point.x, point.y]).collect();
        let mut bbox = empty_box();
        extend_box_with_points(&mut bbox, &points);
        let source = if matches!(image, Image::Stencil(_)) { "image-mask" } else { "image" };
        let mut properties = common("画像", &id);
        properties.push(point("position", "位置", corners[0].x, corners[0].y));
        properties.push(
            Property::new("imageSize", "画素", format!("{} × {}", number(width), number(height))).raw([width, height]),
        );
        let frame = Primitive::Stroke(StrokePrimitive {
            points,
            closed: true,
            segments: None,
            color: FRAME_COLOR.into(),
            width: 0.0,
            dashes: Vec::new(),
        });
        let entity = entity(id, EntityKind::Image, "画像", source, vec![frame], bbox, properties);
        if outside(&entity.bbox, &self.clip()) {
            self.notices.add(
                NoticeLevel::Info,
                "clipped-out",
                "切り抜きやページの外にあって見えない内容は読みませんでした",
            );
        } else if limits::admit() {
            self.entities.push(entity);
        }
    }

    fn pop_clip(&mut self) {
        // ページの切り抜きは残す
        if self.clips.len() > 1 {
            self.clips.pop();
        }
    }

    fn pop_transparency_group(&mut self) {}
}

/// 経路の 1 つの部分経路を折線にしたもの
struct Subpath {
    points: Vec<f64>,
    closed: bool,
    curved: bool,
    /// 直線と曲線の数（閉じる線分を含む）
    segments: usize,
}

impl Subpath {
    fn start(point: Point) -> Self {
        Self { points: vec![point.x, point.y], closed: false, curved: false, segments: 0 }
    }

    fn last(&self) -> Point {
        Point::new(self.points[self.points.len() - 2], self.points[self.points.len() - 1])
    }

    fn length(&self) -> f64 {
        let (points, _) = self.points.as_chunks::<2>();
        let mut length: f64 =
            points.windows(2).map(|pair| js::hypot2(pair[1][0] - pair[0][0], pair[1][1] - pair[0][1])).sum();
        if self.closed {
            let n = self.points.len();
            length += js::hypot2(self.points[0] - self.points[n - 2], self.points[1] - self.points[n - 1]);
        }
        length
    }
}

/// 経路を部分経路ごとの折線にする。曲線は TOLERANCE の誤差で折線にする
fn subpaths(path: &BezPath) -> Vec<Subpath> {
    let mut out: Vec<Subpath> = Vec::new();
    // 閉じた部分経路の後の線は、その始点から続く（8.5.2.1）
    let current = |out: &mut Vec<Subpath>| -> usize {
        match out.last() {
            Some(subpath) if !subpath.closed => {}
            Some(subpath) => {
                let start = Point::new(subpath.points[0], subpath.points[1]);
                out.push(Subpath::start(start));
            }
            None => out.push(Subpath::start(Point::ZERO)),
        }
        out.len() - 1
    };
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => out.push(Subpath::start(point)),
            PathEl::LineTo(point) => {
                let index = current(&mut out);
                let subpath = &mut out[index];
                subpath.points.extend([point.x, point.y]);
                subpath.segments += 1;
            }
            PathEl::QuadTo(..) | PathEl::CurveTo(..) => {
                let index = current(&mut out);
                let subpath = &mut out[index];
                let segment = [PathEl::MoveTo(subpath.last()), *element];
                kurbo::flatten(segment, TOLERANCE, |flattened| {
                    if let PathEl::LineTo(point) = flattened {
                        subpath.points.extend([point.x, point.y]);
                    }
                });
                subpath.curved = true;
                subpath.segments += 1;
            }
            PathEl::ClosePath => {
                if let Some(subpath) = out.last_mut().filter(|subpath| !subpath.closed) {
                    subpath.closed = true;
                    let n = subpath.points.len();
                    if n >= 4 && subpath.points[..2] == subpath.points[n - 2..] {
                        // 始点に戻ってから閉じた経路は、重なる最後の点を除く
                        if n > 4 {
                            subpath.points.truncate(n - 2);
                        }
                    } else if n >= 4 {
                        subpath.segments += 1;
                    }
                }
            }
        }
    }
    out
}

fn vector(transform: &Affine, x: f64, y: f64) -> Vec2 {
    (*transform * Point::new(x, y)) - (*transform * Point::ZERO)
}

/// 外接矩形が切り抜きの外接矩形と重ならないか
fn outside(bbox: &BBox, clip: &Rect) -> bool {
    const SLACK: f64 = 1e-6;
    bbox.max_x < clip.x0 - SLACK
        || bbox.min_x > clip.x1 + SLACK
        || bbox.max_y < clip.y0 - SLACK
        || bbox.min_y > clip.y1 + SLACK
}

fn entity(
    id: String,
    kind: EntityKind,
    type_name: &str,
    source: &str,
    primitives: Vec<Primitive>,
    bbox: BBox,
    properties: Vec<Property>,
) -> SceneEntity {
    SceneEntity {
        id,
        kind,
        type_name: type_name.into(),
        source: source.into(),
        layers: vec![LAYER],
        primitives,
        bbox,
        properties,
        placement: None,
        // 図形はページの下敷きが描く（決定 0020、0022）
        drawn: false,
    }
}

/// どの図形も持つ属性（種類・id・レイヤ）
fn common(type_name: &str, id: &str) -> Vec<Property> {
    vec![
        Property::new("type", "種類", type_name),
        Property::new("id", "ID", id),
        Property::new("layer", "レイヤ", LAYER_NAME).raw(f64::from(LAYER)),
    ]
}

/// 座標の属性。raw は [x, y]（用紙の mm）
fn point(key: &'static str, label: &str, x: f64, y: f64) -> Property {
    Property::new(key, label, format!("{}, {}", number(x), number(y))).raw([x, y])
}

/// 数を小数 4 桁までで書く（ほかの読み取りの属性と同じ書き方）
fn number(value: f64) -> String {
    if !value.is_finite() {
        return js::number_to_string(value);
    }
    js::format_ja(js::to_fixed(value, 4).parse::<f64>().unwrap_or(f64::NAN), 4)
}

/// #rrggbb、不透明でなければ #rrggbbaa
fn hex([r, g, b, a]: [u8; 4]) -> String {
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}
