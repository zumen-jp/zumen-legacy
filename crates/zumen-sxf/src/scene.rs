//! SXF の図面を、用紙の座標に解決した表示用の図面にする（`@zumenjs/sxf` の scene.ts と同じ規則・同じ順）。
//! 読み取りの注意に、描画で省いた内容を加える。

use crate::document::{SxfDocument, SxfDrawing};
use crate::features::{Annotation, Arrow, ExtensionLine, HatchPattern, Region, SxfFeature};
use crate::symbols::{arrow_primitives, marker_primitives};
use crate::style::DEFAULT_WIDTH;
use crate::tables::{
    PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, arrow_name, display_name, index, marker_name, sfig_kind, sheet_size,
};
use std::collections::HashSet;
use std::f64::consts::PI;
use std::sync::Arc;
use zumen_core::geometry::{
    IDENTITY, apply, apply_vector, arc_segments, bezier_points, clothoid_points, ellipse_arc_points, empty_box,
    extend_box, extend_box_with_points, invert, is_empty_box, mean_scale, multiply, placement, rotation_deg,
    sweep_radians, transform_points, union_box,
};
use zumen_core::text::{TextLayoutInput, font_family_for, layout_text};
use zumen_core::{
    BACKGROUND_COLOR, BBox, Background, ClippedPrimitive, FillPrimitive, Matrix, NoticeLevel, NoticeList, Placement,
    Primitive, Property, Scene, SceneEntity, SceneLayer, Sheet, StrokePrimitive, TextPrimitive, js,
};

const DEFAULT_COLOR: &str = "#ffffff";
const MAX_DEPTH: usize = 32;
/// Scene のレイヤの id の上限（32 ビットの符号なし整数）
const MAX_LAYER_CODE: f64 = 4_294_967_295.0;
const MAX_HATCH_LINES: f64 = 20_000.0;

/// 円弧・曲線を折線にするときの許容誤差（用紙の mm）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SxfSceneOptions {
    pub tolerance: f64,
}

impl Default for SxfSceneOptions {
    fn default() -> Self {
        Self { tolerance: 0.02 }
    }
}

/// 属性に表示する数値（`Intl.NumberFormat("ja-JP", { maximumFractionDigits })`）
fn format(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return js::number_to_string(value);
    }
    js::format_ja(value, digits)
}

fn number(value: f64) -> String {
    format(value, 3)
}

/// `${value}`
fn string(value: f64) -> String {
    js::number_to_string(value)
}

fn property(label: &str, value: impl Into<String>) -> Property {
    Property::new(label, value)
}

/// 曲線を点列にする分割数。NaN なら点を作らない（TypeScript の `for (i = 0; i <= NaN; …)`）
#[allow(clippy::too_many_arguments)]
fn clothoid(x: f64, y: f64, a: f64, ccw: bool, angle: f64, s0: f64, s1: f64, segments: f64) -> Vec<f64> {
    if segments.is_nan() {
        return Vec::new();
    }
    clothoid_points(x, y, a, ccw, angle, s0, s1, segments as usize)
}

#[derive(Clone)]
struct Context {
    /// 元の座標（表示の向き）から用紙への変換
    m: Matrix,
    /// 測地座標系の部分図の中なら、座標の X と Y を入れ替えて表示の向きにする
    geodetic: bool,
    layers: Vec<f64>,
    path: String,
    placement: Option<Arc<Placement>>,
    stack: Vec<String>,
}

/// SXF の図面を、用紙の座標に解決した表示用の図面にする
pub fn sxf_to_scene(drawing: &SxfDrawing) -> Scene {
    sxf_to_scene_with(drawing, &SxfSceneOptions::default())
}

pub fn sxf_to_scene_with(drawing: &SxfDrawing, options: &SxfSceneOptions) -> Scene {
    let doc = &drawing.document;
    let mut builder = SceneBuilder {
        doc,
        notices: NoticeList::from_notices(&drawing.notices),
        tolerance: options.tolerance,
        entities: Vec::new(),
        layer_codes: Vec::new(),
        seen_layer_codes: HashSet::new(),
        background: Background::Dark,
    };
    builder.run();
    let sheet = sheet_of(doc);
    let mut bounds = empty_box();
    for entity in &builder.entities {
        union_box(&mut bounds, &entity.bbox);
    }
    if let Some(sheet) = &sheet {
        extend_box(&mut bounds, 0.0, 0.0);
        extend_box(&mut bounds, sheet.width, sheet.height);
    }
    let mut layers: Vec<SceneLayer> = doc
        .layers
        .iter()
        .enumerate()
        .map(|(index, layer)| SceneLayer {
            id: index as u32 + 1,
            name: if layer.name.is_empty() { format!("レイヤ {}", index + 1) } else { layer.name.clone() },
            visible: layer.visible,
            group: None,
        })
        .collect();
    for code in builder.layer_codes.clone() {
        if code > 0.0 && !layers.iter().any(|layer| f64::from(layer.id) == code) {
            layers.push(SceneLayer {
                id: code as u32,
                name: format!("未定義のレイヤ {}", string(code)),
                visible: true,
                group: None,
            });
            builder.notices.add(
                NoticeLevel::Warning,
                "unknown-layer",
                "定義のないレイヤコードを表示中のレイヤとして扱いました",
            );
        }
    }
    layers.sort_by_key(|layer| layer.id);
    Scene {
        format: drawing.format.to_string(),
        version: drawing.version.clone(),
        views: None,
        view: None,
        metadata: metadata_of(doc, drawing, sheet.as_ref()),
        sheet,
        bounds,
        layers,
        entities: builder.entities,
        notices: builder.notices.into_vec(),
        background: builder.background,
    }
}

fn sheet_of(doc: &SxfDocument) -> Option<Sheet> {
    let feature = doc.sheet.as_ref()?;
    let Some((label, width, height)) = sheet_size(feature.sheet_type) else {
        return Some(Sheet {
            name: feature.name.clone(),
            size: "自由".into(),
            width: js::max(feature.x, 1.0),
            height: js::max(feature.y, 1.0),
        });
    };
    let landscape = feature.orient != 0.0;
    Some(Sheet {
        name: feature.name.clone(),
        size: format!("{label}{}", if landscape { " 横" } else { " 縦" }),
        width: if landscape { height } else { width },
        height: if landscape { width } else { height },
    })
}

fn metadata_of(doc: &SxfDocument, drawing: &SxfDrawing, sheet: Option<&Sheet>) -> Vec<Property> {
    let mut items = vec![property("形式", format!("{}（{}）", drawing.format.to_uppercase(), drawing.version))];
    if let Some(sheet) = sheet {
        let name = if js::trim(&sheet.name).is_empty() { String::new() } else { format!("{}・", sheet.name) };
        items.push(property(
            "用紙",
            format!("{name}{}（{}×{}mm）", sheet.size, string(sheet.width), string(sheet.height)),
        ));
    }
    if let Some(title) = &doc.title {
        for value in &title.values {
            if !value.value.is_empty() && value.value != " " {
                items.push(property(value.label, value.value.clone()));
            }
        }
    }
    if !drawing.header.originating_system.is_empty() {
        items.push(property("作成ソフト", drawing.header.originating_system.clone()));
    }
    if !drawing.header.preprocessor.is_empty() {
        items.push(property("出力ライブラリ", drawing.header.preprocessor.clone()));
    }
    let figures = doc.sub_figures.len();
    let partial =
        doc.sub_figures.values().filter(|figure| figure.feature.flag == 1.0 || figure.feature.flag == 2.0).count();
    if figures > 0 {
        items.push(property("複合図形", format!("{figures} 件（部分図 {partial} 件）")));
    }
    items
}

/// `/\$\$背景色\$\$色\$\$(\d+)_(\d+)_(\d+)/` の 3 つの数
fn background_rgb(name: &str) -> Option<(f64, f64, f64)> {
    const PREFIX: &str = "$$背景色$$色$$";
    let digits = |text: &str| text.bytes().take_while(u8::is_ascii_digit).count();
    let mut from = 0;
    while let Some(offset) = name[from..].find(PREFIX) {
        let start = from + offset + PREFIX.len();
        let rest = &name[start..];
        let a = digits(rest);
        if a > 0 && rest[a..].starts_with('_') {
            let rest_b = &rest[a + 1..];
            let b = digits(rest_b);
            if b > 0 && rest_b[b..].starts_with('_') {
                let rest_c = &rest_b[b + 1..];
                let c = digits(rest_c);
                if c > 0 {
                    let parse = |text: &str| text.parse::<f64>().unwrap_or(f64::NAN);
                    return Some((parse(&rest[..a]), parse(&rest_b[..b]), parse(&rest_c[..c])));
                }
            }
        }
        // "$$" は 1 バイトなので、1 バイト先は文字の境目
        from = from + offset + 1;
    }
    None
}

fn scale_label(scale: f64) -> String {
    if scale.is_nan() || scale <= 0.0 {
        return "-".into();
    }
    let inverse = 1.0 / scale;
    if (inverse - js::round(inverse)).abs() < 1e-6 {
        format!("1/{}", string(js::round(inverse)))
    } else {
        format(scale, 6)
    }
}

/// 曲線の点列を、向きをそろえてつなぐ
fn append_chained(ring: &mut Vec<f64>, points: Vec<f64>) {
    if points.len() < 4 {
        return;
    }
    if ring.is_empty() {
        ring.extend(points);
        return;
    }
    let lx = ring[ring.len() - 2];
    let ly = ring[ring.len() - 1];
    let n = points.len();
    let start_distance = js::hypot2(points[0] - lx, points[1] - ly);
    let end_distance = js::hypot2(points[n - 2] - lx, points[n - 1] - ly);
    if end_distance < start_distance {
        let mut i = n as isize - 2;
        let mut first = true;
        while i >= 0 {
            if !first {
                ring.push(points[i as usize]);
                ring.push(points[i as usize + 1]);
            }
            first = false;
            i -= 2;
        }
    } else {
        ring.extend_from_slice(&points[2..]);
    }
}

fn extend_primitive_box(bbox: &mut BBox, primitive: &Primitive) {
    match primitive {
        Primitive::Stroke(stroke) => extend_box_with_points(bbox, &stroke.points),
        Primitive::Fill(fill) => {
            for ring in &fill.rings {
                extend_box_with_points(bbox, ring);
            }
        }
        Primitive::Clipped(clipped) => {
            for ring in &clipped.rings {
                extend_box_with_points(bbox, ring);
            }
        }
        Primitive::Dot(dot) => extend_box(bbox, dot.x, dot.y),
        Primitive::Text(text) => {
            let m = &text.matrix;
            for (x, y) in [(0.0, 0.0), (text.box_width, 0.0), (text.box_width, text.box_height), (0.0, text.box_height)]
            {
                extend_box(bbox, m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            }
        }
    }
}

struct SceneBuilder<'a> {
    doc: &'a SxfDocument,
    notices: NoticeList,
    tolerance: f64,
    entities: Vec<SceneEntity>,
    /// 使われたレイヤコード（最初に現れた順）
    layer_codes: Vec<f64>,
    seen_layer_codes: HashSet<u64>,
    background: Background,
}

impl SceneBuilder<'_> {
    // 色・線種・線幅・フォントのコードを値にする（附属書 SFC 編 1-3-8〜1-3-18）

    fn color(&mut self, code: f64) -> String {
        self.doc.color(code).unwrap_or_else(|| {
            self.notices.add(NoticeLevel::Warning, "unknown-color", "定義のない色コードを白で表示しました");
            DEFAULT_COLOR.into()
        })
    }

    fn color_label(&self, code: f64) -> String {
        if let Some(predefined) = index(code - 1.0).and_then(|index| PREDEFINED_COLORS.get(index)) {
            return format!("{}（{}）", predefined.label, string(code));
        }
        if code >= 17.0
            && let Some(user) = index(code - 17.0).and_then(|index| self.doc.user_colors.get(index))
        {
            return format!(
                "ユーザ定義 RGB({}, {}, {})（{}）",
                string(user.r),
                string(user.g),
                string(user.b),
                string(code)
            );
        }
        format!("未定義（{}）", string(code))
    }

    fn dashes(&mut self, code: f64) -> Vec<f64> {
        self.doc.dashes(code).unwrap_or_else(|| {
            self.notices.add(NoticeLevel::Warning, "unknown-line-type", "定義のない線種コードを実線で表示しました");
            Vec::new()
        })
    }

    fn line_type_label(&self, code: f64) -> String {
        if let Some(predefined) = index(code - 1.0).and_then(|index| PREDEFINED_LINE_TYPES.get(index))
            && code <= 15.0
        {
            return format!("{}（{}）", predefined.label, string(code));
        }
        if code >= 17.0
            && let Some(user) = index(code - 17.0).and_then(|index| self.doc.user_line_types.get(index))
        {
            return format!("{}（{}）", user.name, string(code));
        }
        format!("未定義（{}）", string(code))
    }

    fn width(&mut self, code: f64) -> f64 {
        self.doc.width(code).unwrap_or_else(|| {
            self.notices.add(NoticeLevel::Warning, "unknown-width", "定義のない線幅コードを 0.13mm で表示しました");
            DEFAULT_WIDTH
        })
    }

    fn font(&self, code: f64) -> Option<&str> {
        self.doc.font(code)
    }

    fn layer_name(&self, code: f64) -> String {
        if code == 0.0 {
            return "なし".into();
        }
        match index(code - 1.0).and_then(|index| self.doc.layers.get(index)) {
            Some(layer) => layer.name.clone(),
            None => format!("未定義（{}）", string(code)),
        }
    }

    fn add_layer_code(&mut self, code: f64) {
        if self.seen_layer_codes.insert(code.to_bits()) {
            self.layer_codes.push(code);
        }
    }

    fn run(&mut self) {
        for figure in self.doc.sub_figures.values() {
            if let Some((r, g, b)) = background_rgb(&figure.feature.name) {
                let luminance = r * 0.299 + g * 0.587 + b * 0.114;
                self.background = if luminance > 128.0 { Background::Light } else { Background::Dark };
            }
            if figure.feature.name.contains("$$RASTER$$") {
                self.notices.add(
                    NoticeLevel::Info,
                    "raster",
                    "ラスタ画像（別の TIFF・JPEG ファイル）は表示していません",
                );
            }
        }
        let root = Context {
            m: IDENTITY,
            geodetic: false,
            layers: Vec::new(),
            path: String::new(),
            placement: None,
            stack: Vec::new(),
        };
        let doc = self.doc;
        for feature in &doc.sheet_members {
            self.emit(feature, &root);
        }
    }

    /// 元の座標を表示の向きにして用紙へ写す
    fn point(ctx: &Context, x: f64, y: f64) -> (f64, f64) {
        if ctx.geodetic { apply(&ctx.m, y, x) } else { apply(&ctx.m, x, y) }
    }

    fn local(ctx: &Context, x: f64, y: f64) -> (f64, f64) {
        if ctx.geodetic { (y, x) } else { (x, y) }
    }

    fn local_points(ctx: &Context, xs: &[f64], ys: &[f64]) -> Vec<f64> {
        let count = xs.len().min(ys.len());
        let mut out = Vec::with_capacity(count * 2);
        for i in 0..count {
            let (x, y) = Self::local(ctx, xs[i], ys[i]);
            out.push(x);
            out.push(y);
        }
        out
    }

    /// 元の座標での弦の許容誤差
    fn local_tolerance(&self, ctx: &Context) -> f64 {
        self.tolerance / js::max(mean_scale(&ctx.m), 1e-12)
    }

    fn stroke(
        &mut self,
        ctx: &Context,
        points: &[f64],
        closed: bool,
        color: f64,
        line_type: f64,
        line_width: f64,
    ) -> Primitive {
        let color = self.color(color);
        let width = self.width(line_width);
        let dashes = self.dashes(line_type);
        Primitive::Stroke(StrokePrimitive {
            points: transform_points(&ctx.m, points),
            closed,
            segments: None,
            color,
            width,
            dashes,
        })
    }

    fn add(
        &mut self,
        feature: &SxfFeature,
        ctx: &Context,
        layer: f64,
        primitives: Vec<Primitive>,
        properties: Vec<Property>,
    ) {
        if primitives.is_empty() {
            return;
        }
        let mut bbox = empty_box();
        for primitive in &primitives {
            extend_primitive_box(&mut bbox, primitive);
        }
        if is_empty_box(&bbox) {
            return;
        }
        let mut layers = ctx.layers.clone();
        let own = self.layer_code(layer);
        if own > 0.0 {
            layers.push(own);
        }
        for code in &layers {
            self.add_layer_code(*code);
        }
        let kind = feature.kind();
        let id = format!("{}#{}", ctx.path, string(feature.id()));
        let mut all = vec![
            property("種類", kind.label()),
            property("番号", id.clone()),
            property("レイヤ", self.layer_name(layer)),
        ];
        if let Some(placement) = &ctx.placement {
            let vertical = if (placement.scale_x - placement.scale_y).abs() > 1e-12 {
                format!("・縦 {}", scale_label(placement.scale_y))
            } else {
                String::new()
            };
            all.push(property(
                "配置",
                format!("{}（縮尺 {}{vertical}）", placement.name, scale_label(placement.scale_x)),
            ));
        }
        all.extend(properties);
        self.entities.push(SceneEntity {
            id,
            kind: kind.label().into(),
            source: format!("{}_feature", kind.name()),
            layers: layers.iter().map(|code| *code as u32).collect(),
            primitives,
            bbox,
            properties: all,
            placement: ctx.placement.clone(),
        });
    }

    /// Scene のレイヤの id にできないほど大きいレイヤコードは、レイヤなし（0）として扱う
    fn layer_code(&mut self, code: f64) -> f64 {
        if code <= MAX_LAYER_CODE {
            return code;
        }
        self.notices.add(NoticeLevel::Warning, "layer-range", "範囲外のレイヤコードを、レイヤなしとして扱いました");
        0.0
    }

    fn style_properties(&mut self, color: f64, line_type: f64, line_width: f64) -> Vec<Property> {
        let color_label = self.color_label(color);
        let line_type_label = self.line_type_label(line_type);
        let width = self.width(line_width);
        vec![
            property("色", color_label),
            property("線種", line_type_label),
            property("線幅", format!("{}mm（{}）", number(width), string(line_width))),
        ]
    }

    fn emit(&mut self, feature: &SxfFeature, ctx: &Context) {
        let tol = self.local_tolerance(ctx);
        match feature {
            SxfFeature::Line { style, x1, y1, x2, y2 } => {
                let (ax, ay) = Self::local(ctx, *x1, *y1);
                let (bx, by) = Self::local(ctx, *x2, *y2);
                let stroke = self.stroke(ctx, &[ax, ay, bx, by], false, style.color, style.line_type, style.line_width);
                let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
                properties.push(property("始点", format!("{}, {}", number(*x1), number(*y1))));
                properties.push(property("終点", format!("{}, {}", number(*x2), number(*y2))));
                properties.push(property("長さ", number(js::hypot2(x2 - x1, y2 - y1))));
                self.add(feature, ctx, style.layer, vec![stroke], properties);
            }
            SxfFeature::Polyline { style, xs, ys } => {
                let points = Self::local_points(ctx, xs, ys);
                let stroke = self.stroke(ctx, &points, false, style.color, style.line_type, style.line_width);
                let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
                properties.push(property("頂点数", string(points.len() as f64 / 2.0)));
                self.add(feature, ctx, style.layer, vec![stroke], properties);
            }
            SxfFeature::Circle { style, .. }
            | SxfFeature::Arc { style, .. }
            | SxfFeature::Ellipse { style, .. }
            | SxfFeature::EllipseArc { style, .. } => {
                let (fcx, fcy, rx, ry, rotation, full, start, end, ccw) = match feature {
                    SxfFeature::Circle { cx, cy, r, .. } => (*cx, *cy, *r, *r, 0.0, true, 0.0, 360.0, true),
                    SxfFeature::Arc { cx, cy, r, ccw, start, end, .. } => {
                        (*cx, *cy, *r, *r, 0.0, false, *start, *end, *ccw)
                    }
                    SxfFeature::Ellipse { cx, cy, rx, ry, rotation, .. } => {
                        (*cx, *cy, *rx, *ry, *rotation, true, 0.0, 360.0, true)
                    }
                    SxfFeature::EllipseArc { cx, cy, rx, ry, ccw, rotation, start, end, .. } => {
                        (*cx, *cy, *rx, *ry, *rotation, false, *start, *end, *ccw)
                    }
                    _ => unreachable!(),
                };
                let (cx, cy) = Self::local(ctx, fcx, fcy);
                let sweep = if full { PI * 2.0 } else { sweep_radians(start, end, ccw) };
                let segments = arc_segments(js::max(rx, ry), sweep, tol);
                let mut points = ellipse_arc_points(cx, cy, rx, ry, rotation, start, end, ccw, segments);
                if full {
                    points.truncate(points.len().saturating_sub(2));
                }
                let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
                properties.push(property("中心", format!("{}, {}", number(fcx), number(fcy))));
                if matches!(feature, SxfFeature::Circle { .. } | SxfFeature::Arc { .. }) {
                    properties.push(property("半径", number(rx)));
                } else {
                    properties.push(property("半径（X・Y）", format!("{}・{}", number(rx), number(ry))));
                }
                if !full {
                    properties.push(property(
                        "角度",
                        format!(
                            "{}° → {}°（{}）",
                            number(start),
                            number(end),
                            if ccw { "反時計回り" } else { "時計回り" }
                        ),
                    ));
                }
                let stroke = self.stroke(ctx, &points, full, style.color, style.line_type, style.line_width);
                self.add(feature, ctx, style.layer, vec![stroke], properties);
            }
            SxfFeature::Spline { style, xs, ys, .. } => {
                let control = Self::local_points(ctx, xs, ys);
                let count = control.len() / 2;
                if count > 1 && (count - 1) % 3 != 0 {
                    self.notices.add(
                        NoticeLevel::Warning,
                        "spline-points",
                        "頂点数が 3n+1 でないスプラインの余りの点を折線でつなぎました",
                    );
                }
                let points = bezier_points(&control, 24);
                let stroke = self.stroke(ctx, &points, false, style.color, style.line_type, style.line_width);
                let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
                properties.push(property("制御点数", string(count as f64)));
                self.add(feature, ctx, style.layer, vec![stroke], properties);
            }
            SxfFeature::Clothoid { style, x, y, parameter, ccw, angle, start_length, end_length } => {
                let (lx, ly) = Self::local(ctx, *x, *y);
                let length = (end_length - start_length).abs();
                let segments = js::min(js::max((length / js::max(tol * 20.0, 1e-6)).ceil(), 16.0), 2048.0);
                let points = clothoid(lx, ly, *parameter, *ccw, *angle, *start_length, *end_length, segments);
                let stroke = self.stroke(ctx, &points, false, style.color, style.line_type, style.line_width);
                let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
                properties.push(property("パラメータ A", number(*parameter)));
                properties.push(property("曲線長", format!("{} → {}", number(*start_length), number(*end_length))));
                self.add(feature, ctx, style.layer, vec![stroke], properties);
            }
            SxfFeature::PointMarker { layer, color, x, y, code, angle, scale, .. } => {
                let (px, py) = Self::point(ctx, *x, *y);
                let color_value = self.color(*color);
                let primitives =
                    marker_primitives(*code, px, py, angle + rotation_deg(&ctx.m), *scale, &color_value, 0.0);
                let properties = vec![
                    property("色", self.color_label(*color)),
                    property("マーカ", marker_name(*code).map_or_else(|| string(*code), str::to_string)),
                    property("位置", format!("{}, {}", number(*x), number(*y))),
                ];
                self.add(feature, ctx, *layer, primitives, properties);
            }
            SxfFeature::TextString { layer, color, text, .. } => {
                let Some(primitive) = self.text(ctx, text, *color) else {
                    return;
                };
                let properties = vec![
                    property("文字列", text.text.clone()),
                    property("色", self.color_label(*color)),
                    property(
                        "フォント",
                        self.font(text.font).map_or_else(|| format!("未定義（{}）", string(text.font)), str::to_string),
                    ),
                    property("文字範囲", format!("高さ {}・幅 {}", number(text.height), number(text.width))),
                    property("位置", format!("{}, {}", number(text.x), number(text.y))),
                    property("回転角", format!("{}°", number(text.angle))),
                ];
                self.add(feature, ctx, *layer, vec![Primitive::Text(primitive)], properties);
            }
            SxfFeature::SfigLocate { .. } => self.locate(feature, ctx),
            SxfFeature::ExternallyDefinedSymbol { layer, color, name, x, y, angle, scale, .. } => {
                let (px, py) = Self::point(ctx, *x, *y);
                let color = match color {
                    None => DEFAULT_COLOR.to_string(),
                    Some(code) => self.color(*code),
                };
                let label = layout_text(
                    TextLayoutInput {
                        text: if name.is_empty() { "?" } else { name },
                        x: px,
                        y: py,
                        height: 2.5 * (if *scale > 0.0 { *scale } else { 1.0 }),
                        width: 0.0,
                        spacing: 0.0,
                        angle: angle + rotation_deg(&ctx.m),
                        slant: 0.0,
                        anchor: 5.0,
                        vertical: false,
                        color,
                        font_family: font_family_for("").to_string(),
                    },
                    &IDENTITY,
                );
                self.notices.add(NoticeLevel::Info, "symbol", "既定義シンボルは形の代わりに名前を表示しました");
                let properties = vec![
                    property("シンボル名", name.clone()),
                    property("位置", format!("{}, {}", number(*x), number(*y))),
                ];
                self.add(feature, ctx, *layer, label.map(Primitive::Text).into_iter().collect(), properties);
            }
            SxfFeature::LinearDim { .. }
            | SxfFeature::RadiusDim { .. }
            | SxfFeature::DiameterDim { .. }
            | SxfFeature::CurveDim(_)
            | SxfFeature::AngularDim(_) => self.dimension(feature, ctx),
            SxfFeature::Label { .. } | SxfFeature::Balloon { .. } => self.leader(feature, ctx),
            SxfFeature::FillAreaStyleColour { .. }
            | SxfFeature::FillAreaStyleHatching { .. }
            | SxfFeature::FillAreaStyleTiles { .. }
            | SxfFeature::ExternallyDefinedHatch { .. } => self.hatch(feature, ctx),
            _ => {}
        }
    }

    fn text(&mut self, ctx: &Context, value: &Annotation, color: f64) -> Option<TextPrimitive> {
        let (x, y) = Self::local(ctx, value.x, value.y);
        if value.text.is_empty() {
            return None;
        }
        let color = self.color(color);
        let font_family = font_family_for(self.font(value.font).unwrap_or("")).to_string();
        layout_text(value.layout_input(x, y, color, font_family), &ctx.m)
    }

    fn locate(&mut self, feature: &SxfFeature, ctx: &Context) {
        let SxfFeature::SfigLocate { id, layer, name, x, y, angle, ratio_x, ratio_y } = feature else {
            return;
        };
        let doc = self.doc;
        let Some(figure) = doc.sub_figures.get(name) else {
            self.notices.add(NoticeLevel::Warning, "missing-sfig", "定義のない複合図形の配置を省きました");
            return;
        };
        if ctx.stack.contains(name) || ctx.stack.len() >= MAX_DEPTH {
            self.notices.add(NoticeLevel::Warning, "sfig-cycle", "循環または深すぎる複合図形の配置を省きました");
            return;
        }
        let flag = figure.feature.flag;
        let (lx, ly) = Self::local(ctx, *x, *y);
        // 作図グループは配置位置 (0, 0)・回転 0・尺度 1 とみなす（フィーチャ仕様編 3-5 補足）
        let local = if flag == 3.0 { IDENTITY } else { placement(lx, ly, *angle, *ratio_x, *ratio_y) };
        let m = multiply(&ctx.m, &local);
        let mut layers = ctx.layers.clone();
        let own = self.layer_code(*layer);
        if own > 0.0 {
            layers.push(own);
        }
        for code in &layers {
            self.add_layer_code(*code);
        }
        let mut next_placement = ctx.placement.clone();
        if flag != 3.0 {
            let scale = mean_scale(&local);
            next_placement = Some(Arc::new(Placement {
                name: format!("{} {}", sfig_kind(flag).unwrap_or("複合図形"), display_name(name)),
                scale_x: js::hypot2(m[0], m[1]),
                scale_y: js::hypot2(m[2], m[3]),
                inverse: invert(&m),
            }));
            if scale.is_nan() || scale <= 0.0 {
                return;
            }
        }
        let mut stack = ctx.stack.clone();
        stack.push(name.clone());
        let child = Context {
            m,
            geodetic: if flag == 2.0 {
                true
            } else if flag == 1.0 {
                false
            } else {
                ctx.geodetic
            },
            layers,
            path: format!("{}#{}>", ctx.path, string(*id)),
            placement: next_placement,
            stack,
        };
        for member in &figure.members {
            self.emit(member, &child);
        }
    }

    /// 矢印の先端と向き。向きは、寸法線の近い端から遠い端と反対（外向き）を基本にする
    fn arrow_at(ctx: &Context, arrow: Option<&Arrow>, a: (f64, f64), b: (f64, f64), color: &str) -> Vec<Primitive> {
        let Some(arrow) = arrow.filter(|arrow| arrow.code > 0.0) else {
            return Vec::new();
        };
        let (tx, ty) = Self::point(ctx, arrow.x, arrow.y);
        let (ax, ay) = apply(&ctx.m, a.0, a.1);
        let (bx, by) = apply(&ctx.m, b.0, b.1);
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

    /// 円弧の寸法線の矢印。矢印の位置での接線方向（始角側は時計回り、終角側は反時計回りが外向き）
    fn arc_arrow(
        ctx: &Context,
        arrow: Option<&Arrow>,
        center: (f64, f64),
        start_side: bool,
        color: &str,
    ) -> Vec<Primitive> {
        let Some(arrow) = arrow.filter(|arrow| arrow.code > 0.0) else {
            return Vec::new();
        };
        let (lx, ly) = Self::local(ctx, arrow.x, arrow.y);
        let angle = js::atan2(ly - center.1, lx - center.0);
        let mut tx = if start_side { js::sin(angle) } else { -js::sin(angle) };
        let mut ty = if start_side { -js::cos(angle) } else { js::cos(angle) };
        if arrow.direction == 2.0 {
            tx = -tx;
            ty = -ty;
        }
        let (ux, uy) = apply_vector(&ctx.m, tx, ty);
        let length = js::hypot2(ux, uy);
        if length.is_nan() || length <= 0.0 {
            return Vec::new();
        }
        let (px, py) = apply(&ctx.m, lx, ly);
        arrow_primitives(arrow.code, px, py, ux / length, uy / length, arrow.scale, color, 0.0, BACKGROUND_COLOR)
    }

    fn extension(&mut self, ctx: &Context, line: Option<&ExtensionLine>, style: (f64, f64, f64)) -> Vec<Primitive> {
        let Some(line) = line else {
            return Vec::new();
        };
        let (ax, ay) = Self::local(ctx, line.x1, line.y1);
        let (bx, by) = Self::local(ctx, line.x2, line.y2);
        vec![self.stroke(ctx, &[ax, ay, bx, by], false, style.0, style.1, style.2)]
    }

    fn dimension(&mut self, feature: &SxfFeature, ctx: &Context) {
        let (style, text) = match feature {
            SxfFeature::LinearDim { style, text, .. }
            | SxfFeature::RadiusDim { style, text, .. }
            | SxfFeature::DiameterDim { style, text, .. } => (style, text),
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => (&dimension.style, &dimension.text),
            _ => return,
        };
        let style_values = (style.color, style.line_type, style.line_width);
        let color = self.color(style.color);
        let mut primitives = Vec::new();
        let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
        match feature {
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => {
                let center = Self::local(ctx, dimension.cx, dimension.cy);
                let sweep = sweep_radians(dimension.angle0, dimension.angle1, true);
                let points = ellipse_arc_points(
                    center.0,
                    center.1,
                    dimension.radius,
                    dimension.radius,
                    0.0,
                    dimension.angle0,
                    dimension.angle1,
                    true,
                    arc_segments(dimension.radius, sweep, self.local_tolerance(ctx)),
                );
                let stroke = self.stroke(ctx, &points, false, style_values.0, style_values.1, style_values.2);
                primitives.push(stroke);
                let ext1 = self.extension(ctx, dimension.ext1.as_ref(), style_values);
                let ext2 = self.extension(ctx, dimension.ext2.as_ref(), style_values);
                primitives.extend(ext1);
                primitives.extend(ext2);
                primitives.extend(Self::arc_arrow(ctx, dimension.arrow1.as_ref(), center, true, &color));
                primitives.extend(Self::arc_arrow(ctx, dimension.arrow2.as_ref(), center, false, &color));
                properties.push(property(
                    "中心・半径",
                    format!("{}, {}・{}", number(dimension.cx), number(dimension.cy), number(dimension.radius)),
                ));
            }
            SxfFeature::LinearDim { line, .. }
            | SxfFeature::RadiusDim { line, .. }
            | SxfFeature::DiameterDim { line, .. } => {
                let [x1, y1, x2, y2] = *line;
                let a = Self::local(ctx, x1, y1);
                let b = Self::local(ctx, x2, y2);
                let stroke =
                    self.stroke(ctx, &[a.0, a.1, b.0, b.1], false, style_values.0, style_values.1, style_values.2);
                primitives.push(stroke);
                match feature {
                    SxfFeature::LinearDim { ext1, ext2, arrow1, arrow2, .. } => {
                        let ext1 = self.extension(ctx, ext1.as_ref(), style_values);
                        let ext2 = self.extension(ctx, ext2.as_ref(), style_values);
                        primitives.extend(ext1);
                        primitives.extend(ext2);
                        primitives.extend(Self::arrow_at(ctx, arrow1.as_ref(), a, b, &color));
                        primitives.extend(Self::arrow_at(ctx, arrow2.as_ref(), a, b, &color));
                    }
                    SxfFeature::RadiusDim { arrow, .. } => {
                        primitives.extend(Self::arrow_at(ctx, arrow.as_ref(), a, b, &color));
                    }
                    SxfFeature::DiameterDim { arrow1, arrow2, .. } => {
                        primitives.extend(Self::arrow_at(ctx, arrow1.as_ref(), a, b, &color));
                        primitives.extend(Self::arrow_at(ctx, arrow2.as_ref(), a, b, &color));
                    }
                    _ => {}
                }
                properties.push(property(
                    "寸法線",
                    format!("{}, {} → {}, {}", number(x1), number(y1), number(x2), number(y2)),
                ));
            }
            _ => {}
        }
        if let Some(text) = text {
            if let Some(primitive) = self.text(ctx, text, style.color) {
                primitives.push(Primitive::Text(primitive));
            }
            properties.insert(0, property("寸法値", text.text.clone()));
        }
        let arrows: Vec<&Arrow> = match feature {
            SxfFeature::RadiusDim { arrow, .. } => arrow.iter().collect(),
            SxfFeature::LinearDim { arrow1, arrow2, .. } | SxfFeature::DiameterDim { arrow1, arrow2, .. } => {
                arrow1.iter().chain(arrow2.iter()).collect()
            }
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => {
                dimension.arrow1.iter().chain(dimension.arrow2.iter()).collect()
            }
            _ => Vec::new(),
        };
        if !arrows.is_empty() {
            let value = arrows
                .iter()
                .map(|arrow| {
                    format!(
                        "{}（{}向き）",
                        arrow_name(arrow.code).map_or_else(|| string(arrow.code), str::to_string),
                        if arrow.direction == 2.0 { "内" } else { "外" }
                    )
                })
                .collect::<Vec<_>>()
                .join("・");
            properties.push(property("矢印", value));
        }
        self.add(feature, ctx, style.layer, primitives, properties);
    }

    fn leader(&mut self, feature: &SxfFeature, ctx: &Context) {
        let (style, xs, ys, arrow_code, arrow_scale, text, balloon) = match feature {
            SxfFeature::Label { style, xs, ys, arrow_code, arrow_scale, text } => {
                (style, xs, ys, *arrow_code, *arrow_scale, text, None)
            }
            SxfFeature::Balloon { style, xs, ys, cx, cy, radius, arrow_code, arrow_scale, text } => {
                (style, xs, ys, *arrow_code, *arrow_scale, text, Some((*cx, *cy, *radius)))
            }
            _ => return,
        };
        let color = self.color(style.color);
        let points = Self::local_points(ctx, xs, ys);
        let mut primitives = vec![self.stroke(ctx, &points, false, style.color, style.line_type, style.line_width)];
        if points.len() >= 4 && arrow_code > 0.0 {
            let (tx, ty) = apply(&ctx.m, points[0], points[1]);
            let (nx, ny) = apply(&ctx.m, points[2], points[3]);
            let length = js::hypot2(tx - nx, ty - ny);
            if length > 0.0 {
                primitives.extend(arrow_primitives(
                    arrow_code,
                    tx,
                    ty,
                    (tx - nx) / length,
                    (ty - ny) / length,
                    arrow_scale,
                    &color,
                    0.0,
                    BACKGROUND_COLOR,
                ));
            }
        }
        if let Some((bcx, bcy, radius)) = balloon {
            let (cx, cy) = Self::local(ctx, bcx, bcy);
            let mut circle = ellipse_arc_points(
                cx,
                cy,
                radius,
                radius,
                0.0,
                0.0,
                360.0,
                true,
                arc_segments(radius, PI * 2.0, self.local_tolerance(ctx)),
            );
            circle.truncate(circle.len().saturating_sub(2));
            primitives.push(self.stroke(ctx, &circle, true, style.color, style.line_type, style.line_width));
        }
        let mut properties = self.style_properties(style.color, style.line_type, style.line_width);
        if let Some(text) = text {
            if let Some(primitive) = self.text(ctx, text, style.color) {
                primitives.push(Primitive::Text(primitive));
            }
            properties.insert(0, property("文字列", text.text.clone()));
        }
        self.add(feature, ctx, style.layer, primitives, properties);
    }

    /// 複合曲線を閉じた点列にする（元の座標、表示の向き）
    fn ring(&self, ctx: &Context, id: f64) -> Option<Vec<f64>> {
        let curve = index(id - 1.0).and_then(|index| self.doc.composite_curves.get(index))?;
        let tol = self.local_tolerance(ctx);
        let mut ring = Vec::new();
        for member in &curve.members {
            let points = match member {
                SxfFeature::Polyline { xs, ys, .. } => Self::local_points(ctx, xs, ys),
                SxfFeature::Line { x1, y1, x2, y2, .. } => {
                    let (ax, ay) = Self::local(ctx, *x1, *y1);
                    let (bx, by) = Self::local(ctx, *x2, *y2);
                    vec![ax, ay, bx, by]
                }
                SxfFeature::Arc { cx, cy, r, ccw, start, end, .. } => {
                    let (lx, ly) = Self::local(ctx, *cx, *cy);
                    let sweep = sweep_radians(*start, *end, *ccw);
                    ellipse_arc_points(
                        lx,
                        ly,
                        *r,
                        *r,
                        0.0,
                        *start,
                        *end,
                        *ccw,
                        arc_segments(js::max(*r, *r), sweep, tol),
                    )
                }
                SxfFeature::EllipseArc { cx, cy, rx, ry, ccw, rotation, start, end, .. } => {
                    let (lx, ly) = Self::local(ctx, *cx, *cy);
                    let sweep = sweep_radians(*start, *end, *ccw);
                    ellipse_arc_points(
                        lx,
                        ly,
                        *rx,
                        *ry,
                        *rotation,
                        *start,
                        *end,
                        *ccw,
                        arc_segments(js::max(*rx, *ry), sweep, tol),
                    )
                }
                SxfFeature::Spline { xs, ys, .. } => bezier_points(&Self::local_points(ctx, xs, ys), 24),
                SxfFeature::Clothoid { x, y, parameter, ccw, angle, start_length, end_length, .. } => {
                    let (lx, ly) = Self::local(ctx, *x, *y);
                    clothoid(lx, ly, *parameter, *ccw, *angle, *start_length, *end_length, 64.0)
                }
                _ => continue,
            };
            append_chained(&mut ring, points);
        }
        (ring.len() >= 6).then_some(ring)
    }

    fn hatch(&mut self, feature: &SxfFeature, ctx: &Context) {
        let (layer, region) = match feature {
            SxfFeature::FillAreaStyleColour { layer, region, .. }
            | SxfFeature::FillAreaStyleHatching { layer, region, .. }
            | SxfFeature::FillAreaStyleTiles { layer, region, .. }
            | SxfFeature::ExternallyDefinedHatch { layer, region, .. } => (*layer, region),
            _ => return,
        };
        let Region { out_id, in_ids } = region;
        let Some(outer) = self.ring(ctx, *out_id) else {
            self.notices.add(NoticeLevel::Warning, "hatch-boundary", "外形の複合曲線がないハッチングを省きました");
            return;
        };
        let mut local_rings = vec![outer];
        local_rings.extend(in_ids.iter().filter_map(|id| self.ring(ctx, *id)));
        let rings: Vec<Vec<f64>> = local_rings.iter().map(|ring| transform_points(&ctx.m, ring)).collect();
        let mut primitives = Vec::new();
        let mut properties = vec![property("中抜き", format!("{} 件", in_ids.len()))];
        match feature {
            SxfFeature::FillAreaStyleColour { color, .. } => {
                let fill_color = self.color(*color);
                primitives.push(Primitive::Fill(FillPrimitive { color: fill_color, rings: rings.clone() }));
                properties.insert(0, property("色", self.color_label(*color)));
            }
            SxfFeature::FillAreaStyleHatching { patterns, .. } => {
                let strokes: Vec<StrokePrimitive> =
                    patterns.iter().filter_map(|pattern| self.hatch_lines(ctx, pattern, &rings)).collect();
                primitives.push(Primitive::Clipped(ClippedPrimitive { rings: rings.clone(), strokes }));
                let value = patterns
                    .iter()
                    .map(|pattern| {
                        format!(
                            "間隔 {}・{}°・{}",
                            number(pattern.spacing),
                            number(pattern.angle),
                            self.color_label(pattern.color)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" / ");
                properties.insert(0, property("ハッチング線", value));
            }
            SxfFeature::FillAreaStyleTiles { name, .. } => {
                self.notices.add(
                    NoticeLevel::Info,
                    "hatch-pattern",
                    "パターンハッチングの模様は描かず、外形だけを表示しました",
                );
                properties.insert(0, property("シンボル", name.clone()));
            }
            SxfFeature::ExternallyDefinedHatch { name, .. } => {
                if name.to_lowercase() == "area_control" {
                    self.notices.add(
                        NoticeLevel::Info,
                        "area-control",
                        "既定義ハッチング（Area_control）は空白の領域のため描いていません",
                    );
                } else {
                    self.notices.add(
                        NoticeLevel::Info,
                        "hatch-external",
                        "外部定義ハッチングの模様は描かず、外形だけを表示しました",
                    );
                }
                properties.insert(0, property("名前", name.clone()));
            }
            _ => {}
        }
        // 外形・中抜きの複合曲線のうち、表示する指定のものを線で描く
        let ids: Vec<f64> = std::iter::once(*out_id).chain(in_ids.iter().copied()).collect();
        let doc = self.doc;
        for (position, id) in ids.iter().enumerate() {
            let curve = index(id - 1.0).and_then(|index| doc.composite_curves.get(index));
            let (Some(curve), Some(ring)) = (curve, local_rings.get(position)) else {
                continue;
            };
            if !curve.feature.visible {
                continue;
            }
            let stroke =
                self.stroke(ctx, ring, true, curve.feature.color, curve.feature.line_type, curve.feature.line_width);
            primitives.push(stroke);
        }
        if primitives.is_empty() {
            // 描く形がない領域（Area_control など）も、選択できるように外形を透明な線で持つ
            primitives.push(Primitive::Stroke(StrokePrimitive {
                points: rings[0].clone(),
                closed: true,
                segments: None,
                color: "transparent".into(),
                width: 0.0,
                dashes: Vec::new(),
            }));
        }
        self.add(feature, ctx, layer, primitives, properties);
    }

    /// ハッチング線を、領域の外接矩形を覆う平行線として作る。描画時に領域で切り取る。
    /// 間隔と角度は部分図・部品の配置の影響を受ける（SXF 実装規約 1-7）
    fn hatch_lines(&mut self, ctx: &Context, pattern: &HatchPattern, rings: &[Vec<f64>]) -> Option<StrokePrimitive> {
        if pattern.spacing.is_nan() || pattern.spacing <= 0.0 {
            return None;
        }
        let mut bbox = empty_box();
        for ring in rings {
            extend_box_with_points(&mut bbox, ring);
        }
        let theta = (pattern.angle * PI) / 180.0;
        let (px, py) = Self::point(ctx, pattern.x, pattern.y);
        let (ux, uy) = apply_vector(&ctx.m, js::cos(theta), js::sin(theta));
        let (sx, sy) = apply_vector(&ctx.m, -js::sin(theta) * pattern.spacing, js::cos(theta) * pattern.spacing);
        let cross = |ax: f64, ay: f64, bx: f64, by: f64| ax * by - ay * bx;
        let denominator = cross(ux, uy, sx, sy);
        if denominator.abs() < 1e-15 {
            return None;
        }
        let corners =
            [(bbox.min_x, bbox.min_y), (bbox.max_x, bbox.min_y), (bbox.max_x, bbox.max_y), (bbox.min_x, bbox.max_y)];
        let ks: Vec<f64> = corners.iter().map(|(cx, cy)| cross(ux, uy, cx - px, cy - py) / denominator).collect();
        let k_min = ks.iter().copied().fold(f64::INFINITY, js::min).ceil();
        let k_max = ks.iter().copied().fold(f64::NEG_INFINITY, js::max).floor();
        if k_max - k_min + 1.0 > MAX_HATCH_LINES {
            self.notices.add(NoticeLevel::Warning, "hatch-dense", "間隔が細かすぎるハッチング線を省きました");
            return None;
        }
        let length = js::hypot2(ux, uy);
        let dx = ux / length;
        let dy = uy / length;
        let span = js::hypot2(bbox.max_x - bbox.min_x, bbox.max_y - bbox.min_y);
        let mut points = Vec::new();
        if k_min.is_finite() {
            let mut k = k_min;
            while k <= k_max {
                let ox = px + k * sx;
                let oy = py + k * sy;
                // 外接矩形の中心へ最も近い線上の点から、対角線の長さだけ両側へ伸ばす
                let cx = (bbox.min_x + bbox.max_x) / 2.0;
                let cy = (bbox.min_y + bbox.max_y) / 2.0;
                let t = (cx - ox) * dx + (cy - oy) * dy;
                let mx = ox + t * dx;
                let my = oy + t * dy;
                points.extend([mx - dx * span, my - dy * span, mx + dx * span, my + dy * span]);
                k += 1.0;
            }
        }
        let color = self.color(pattern.color);
        let width = self.width(pattern.line_width);
        let dashes = self.dashes(pattern.line_type);
        Some(StrokePrimitive { points, closed: false, segments: Some(true), color, width, dashes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::hex;

    #[test]
    fn reads_background_and_display_names_like_the_regular_expressions() {
        assert_eq!(background_rgb("$$ATRF$$背景色$$色$$255_255_255x"), Some((255.0, 255.0, 255.0)));
        assert_eq!(background_rgb("$$背景色$$色$$1_2_"), None);
        assert_eq!(display_name("$$SXF_FG_測地"), "測地");
        assert_eq!(display_name("$$SXF_FMx_a"), "$$SXF_FMx_a");
        assert_eq!(display_name("$$SXF_P_"), "$$SXF_P_");
    }

    #[test]
    fn labels_scales_like_javascript() {
        assert_eq!(scale_label(0.05), "1/20");
        assert_eq!(scale_label(0.3), "0.3");
        assert_eq!(scale_label(0.0), "-");
        assert_eq!(hex(255.4, -3.0, f64::NAN), "#ff00NaN");
    }
}
