//! 図面の記号: 通り芯、方位、縮尺の目盛り、高さの記号（▽）、測点の高さ、引出線、境界の辺の長さと面積、
//! ビューの見出し。どれも線・円・塗り・文字の図形で描く（記号の形は zumen で決めたもので、CAD の記号の写しではない）。

use crate::dimension::{self, Arrow, chain, end_mark, format_number};
use crate::spec::{
    GridSpec, LeaderSpec, LevelMarkSpec, NorthArrowSpec, Point, PolygonLabelSpec, ScaleBarSpec, SpotLevelSpec,
    StyleSpec, ViewTitleSpec,
};
use crate::text::{self, Align, BOTTOM_CENTER, BOTTOM_LEFT, MIDDLE_CENTER, Placed};
use crate::{Ctx, DraftError, Drafter, MAX_PARTS, number, point, points, positive};
use serde_json::Value;
use zumen_convert::builder::{Look, Space, lwpolyline, xyz};
use zumen_dxf::{DxfEntityData, DxfLwPolyline};

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: Point, k: f64) -> Point {
    [a[0] * k, a[1] * k]
}

fn rotate(p: Point, degrees: f64) -> Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
}

/// 塗った丸（幅のある 2 つの半円の LWPOLYLINE。AutoCAD の DONUT と同じ形）
pub(crate) fn dot(drafter: &mut Drafter, space: &Space, look: &Look, center: Point, diameter: f64, path: &str) {
    let r = diameter / 4.0;
    let points = [[center[0] - r, center[1]], [center[0] + r, center[1]]];
    let mut data = lwpolyline(&points, Some(vec![1.0, 1.0]), true);
    if let DxfEntityData::LwPolyline(DxfLwPolyline { const_width, vertices, .. }) = &mut data {
        *const_width = Some(diameter / 2.0);
        vertices.widths = vec![[diameter / 2.0, diameter / 2.0]; 2];
    }
    drafter.add(space, look, "LWPOLYLINE", data, path);
}

fn line(drafter: &mut Drafter, space: &Space, look: &Look, from: Point, to: Point, path: &str) {
    drafter.add(space, look, "LINE", DxfEntityData::Line { start: xyz(from), end: xyz(to) }, path);
}

/// 1 行の文字を置く
#[allow(clippy::too_many_arguments)]
fn label(
    drafter: &mut Drafter,
    ctx: &Ctx,
    look: &Look,
    at: Point,
    content: &str,
    height: f64,
    align: Align,
    rotation: f64,
    path: &str,
) {
    let style = drafter.text_style(None);
    let lines = [content.to_string()];
    let placed = Placed { at, lines: &lines, height, align, rotation, width_factor: 1.0, line_spacing: 1.0, style };
    text::put(drafter, &ctx.space, look, placed, path);
}

/// 通り芯の丸の既定の直径・延ばす長さ（用紙の mm）
const BUBBLE: f64 = 8.0;
const OVERHANG: f64 = 20.0;

pub(crate) fn grid(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &GridSpec,
    path: &str,
) -> Result<(), DraftError> {
    if spec.x.is_empty() && spec.y.is_empty() {
        return Err(DraftError::new("invalid-value", path, "x か y の通り芯を 1 つ以上指定してください"));
    }
    if spec.x.len() + spec.y.len() > MAX_PARTS / 8 {
        return Err(DraftError::new("invalid-value", path, "通り芯が多すぎます"));
    }
    let xs: Vec<f64> = spec.x.iter().map(|axis| number(axis.at, "at", path)).collect::<Result<_, _>>()?;
    let ys: Vec<f64> = spec.y.iter().map(|axis| number(axis.at, "at", path)).collect::<Result<_, _>>()?;
    let range = |values: &[f64]| -> Option<(f64, f64)> {
        let min = values.iter().copied().reduce(f64::min)?;
        let max = values.iter().copied().reduce(f64::max)?;
        Some((min, max))
    };
    let [x0, y0, x1, y1] = match spec.extent {
        Some(extent) => {
            let [a, b, c, d] = extent;
            [number(a, "extent", path)?, number(b, "extent", path)?, number(c, "extent", path)?, number(d, "extent", path)?]
        }
        None => match (range(&xs), range(&ys)) {
            (Some((x0, x1)), Some((y0, y1))) => [x0, y0, x1, y1],
            _ => {
                return Err(DraftError::new(
                    "invalid-value",
                    path,
                    "x と y の片方だけのときは extent（通り芯を引く範囲）を指定してください",
                ));
            }
        },
    };
    let s = ctx.scale;
    let over = number(spec.overhang.unwrap_or(OVERHANG), "overhang", path)?.max(0.0) * s;
    let diameter = positive(spec.bubble.unwrap_or(BUBBLE), "bubble", path)? * s;
    let r = diameter / 2.0;
    let (start, end) = match spec.bubbles.as_deref().unwrap_or("start") {
        "start" => (true, false),
        "end" => (false, true),
        "both" => (true, true),
        "none" => (false, false),
        other => {
            return Err(DraftError::new(
                "invalid-value",
                path,
                format!("bubbles の {other} はありません（start・end・both・none）"),
            ));
        }
    };
    let mut axis_style = style.clone();
    if axis_style.line_type.is_none() && ctx.style.line_type.is_none() {
        axis_style.line_type = Some("CENTER".into());
    }
    let axis_look = drafter.look(ctx, &axis_style, None, path)?;
    let mut mark_style = style.clone();
    mark_style.line_type = Some("Continuous".into());
    let mark_look = drafter.look(ctx, &mark_style, None, path)?;
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, &mark_style, text_layer.as_deref(), path)?;
    let label_height = diameter * 0.45;
    let bubble = |drafter: &mut Drafter, center: Point, content: &str| {
        drafter.add(&ctx.space, &mark_look, "CIRCLE", DxfEntityData::Circle { center: xyz(center), radius: r }, path);
        label(drafter, ctx, &text_look, center, content, label_height, MIDDLE_CENTER, 0.0, path);
    };
    for (axis, x) in spec.x.iter().zip(&xs) {
        let (bottom, top) = (y0 - over, y1 + over);
        line(drafter, &ctx.space, &axis_look, [*x, bottom], [*x, top], path);
        if start {
            bubble(drafter, [*x, bottom - r], &axis.label);
        }
        if end {
            bubble(drafter, [*x, top + r], &axis.label);
        }
    }
    for (axis, y) in spec.y.iter().zip(&ys) {
        let (left, right) = (x0 - over, x1 + over);
        line(drafter, &ctx.space, &axis_look, [left, *y], [right, *y], path);
        if start {
            bubble(drafter, [left - r, *y], &axis.label);
        }
        if end {
            bubble(drafter, [right + r, *y], &axis.label);
        }
    }
    if spec.dimensions.unwrap_or(true) {
        let default_layer = drafter.defaults.dimension_layer.clone();
        let dim_look = drafter.look(ctx, &mark_style, default_layer.as_deref(), path)?;
        let dim_style = dimension::style(drafter, &spec.look(), path)?;
        // 通り芯の間の寸法を、図の外（延ばした長さの 1/3）に、全体の寸法をその外に描く
        let step = over / 3.0 / s;
        if xs.len() >= 2 {
            let mut sorted = xs.clone();
            sorted.sort_by(f64::total_cmp);
            let points: Vec<Point> = sorted.iter().map(|x| [*x, y0]).collect();
            let through = [x0, y0 - step * s];
            chain(drafter, ctx, &dim_look, &dim_style, &points, 0.0, through, Some(step), path);
        }
        if ys.len() >= 2 {
            let mut sorted = ys.clone();
            sorted.sort_by(f64::total_cmp);
            let points: Vec<Point> = sorted.iter().map(|y| [x0, *y]).collect();
            let through = [x0 - step * s, y0];
            chain(drafter, ctx, &dim_look, &dim_style, &points, 90.0, through, Some(step), path);
        }
    }
    Ok(())
}

pub(crate) fn north_arrow(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &NorthArrowSpec,
    path: &str,
) -> Result<(), DraftError> {
    let look = drafter.look(ctx, style, None, path)?;
    let center = point(spec.at, path)?;
    let size = positive(spec.size.unwrap_or(15.0), "size", path)? * ctx.scale;
    let angle = number(spec.angle.unwrap_or(0.0), "angle", path)?;
    let r = size / 2.0;
    let at = |p: Point| add(center, rotate(p, angle));
    drafter.add(&ctx.space, &look, "CIRCLE", DxfEntityData::Circle { center: xyz(center), radius: r }, path);
    let tip = at([0.0, r * 0.95]);
    let left = at([-r * 0.38, -r * 0.7]);
    let notch = at([0.0, -r * 0.35]);
    let right = at([r * 0.38, -r * 0.7]);
    drafter.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&[tip, left, notch, right], None, true), path);
    // 右の半分を塗る
    let data = DxfEntityData::Solid { vtx0: xyz(tip), vtx1: xyz(notch), vtx2: xyz(right), vtx3: xyz(right), invisible_edges: None };
    drafter.add(&ctx.space, &look, "SOLID", data, path);
    let content = spec.label.clone().unwrap_or_else(|| "N".into());
    if !content.is_empty() {
        let text_layer = drafter.defaults.text_layer.clone();
        let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
        let position = at([0.0, r + size * 0.08]);
        label(drafter, ctx, &text_look, position, &content, size * 0.25, BOTTOM_CENTER, angle, path);
    }
    Ok(())
}

/// 長さを単位で書く（m は余分な 0 を除く）
fn length_text(value: f64, unit: &str, precision: Option<u32>) -> String {
    match unit {
        "mm" => format_number(value, precision.unwrap_or(0), true),
        _ => {
            let text = format_number(value / 1000.0, precision.unwrap_or(3), true);
            if precision.is_some() || !text.contains('.') {
                text
            } else {
                text.trim_end_matches('0').trim_end_matches('.').to_string()
            }
        }
    }
}

fn unit_of(unit: Option<&str>, path: &str) -> Result<&'static str, DraftError> {
    match unit.unwrap_or("m") {
        "m" => Ok("m"),
        "mm" => Ok("mm"),
        other => Err(DraftError::new("invalid-value", path, format!("unit の {other} はありません（m・mm）"))),
    }
}

pub(crate) fn scale_bar(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &ScaleBarSpec,
    path: &str,
) -> Result<(), DraftError> {
    let look = drafter.look(ctx, style, None, path)?;
    let at = point(spec.at, path)?;
    let length = positive(spec.length, "length", path)?;
    let divisions = spec.divisions.unwrap_or(5).clamp(1, 100) as usize;
    let height = positive(spec.height.unwrap_or(2.0), "height", path)? * ctx.scale;
    let unit = unit_of(spec.unit.as_deref(), path)?;
    let step = length / divisions as f64;
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let text_height = drafter.text_height() * 0.8 * ctx.scale;
    for index in 0..divisions {
        let x = at[0] + step * index as f64;
        let corners = vec![[x, at[1]], [x + step, at[1]], [x + step, at[1] + height], [x, at[1] + height]];
        if index % 2 == 0 {
            crate::hatch::solid(drafter, ctx, &look, &[corners.clone()], path);
        }
        drafter.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&corners, None, true), path);
    }
    for index in 0..=divisions {
        let x = at[0] + step * index as f64;
        let mut content = length_text(step * index as f64, unit, None);
        if index == divisions {
            content.push_str(unit);
        }
        label(drafter, ctx, &text_look, [x, at[1] + height * 1.5], &content, text_height, BOTTOM_CENTER, 0.0, path);
    }
    Ok(())
}

pub(crate) fn level_mark(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &LevelMarkSpec,
    path: &str,
) -> Result<(), DraftError> {
    let look = drafter.look(ctx, style, None, path)?;
    let tip = point(spec.at, path)?;
    let size = positive(spec.size.unwrap_or(3.0), "size", path)? * ctx.scale;
    let rise = size * 3f64.sqrt() / 2.0;
    let corners = vec![tip, [tip[0] + size / 2.0, tip[1] + rise], [tip[0] - size / 2.0, tip[1] + rise]];
    drafter.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&corners, None, true), path);
    if let Some(length) = spec.line {
        let length = positive(length, "line", path)? * ctx.scale;
        line(drafter, &ctx.space, &look, [tip[0] - size, tip[1]], [tip[0] + length, tip[1]], path);
    }
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let height = positive(spec.text_height.unwrap_or_else(|| drafter.text_height()), "textHeight", path)? * ctx.scale;
    let at = [tip[0] + size * 0.75, tip[1] + size * 0.1];
    label(drafter, ctx, &text_look, at, &spec.text, height, BOTTOM_LEFT, 0.0, path);
    Ok(())
}

pub(crate) fn spot_level(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &SpotLevelSpec,
    path: &str,
) -> Result<(), DraftError> {
    let look = drafter.look(ctx, style, None, path)?;
    let at = point(spec.at, path)?;
    let s = ctx.scale;
    dot(drafter, &ctx.space, &look, at, 0.8 * s, path);
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let height = positive(spec.text_height.unwrap_or_else(|| drafter.text_height() * 0.8), "textHeight", path)? * s;
    label(drafter, ctx, &text_look, [at[0] + 0.8 * s, at[1] + 0.4 * s], &spec.text, height, BOTTOM_LEFT, 0.0, path);
    Ok(())
}

pub(crate) fn leader(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &LeaderSpec,
    path: &str,
) -> Result<(), DraftError> {
    let look = drafter.look(ctx, style, None, path)?;
    let points = points(&spec.points, 2, path)?;
    let s = ctx.scale;
    let height = positive(spec.text_height.unwrap_or_else(|| drafter.text_height()), "textHeight", path)? * s;
    let arrow = Arrow::parse(spec.arrow.as_deref().or(drafter.defaults.arrow.as_deref()), path)?;
    let arrow_size =
        positive(spec.arrow_size.or(drafter.defaults.arrow_size).unwrap_or(2.5), "arrowSize", path)? * s;
    let last = points[points.len() - 1];
    let previous = points[points.len() - 2];
    let rightward = last[0] >= previous[0];
    let width = spec.text.split('\n').map(|line| text::width(line, height, 1.0)).fold(0.0, f64::max);
    // 文字の下の水平の線（棚）
    let landing = [last[0] + if rightward { 1.0 } else { -1.0 } * (width + 1.0 * s), last[1]];
    let mut path_points = points.clone();
    path_points.push(landing);
    drafter.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&path_points, None, false), path);
    let first = sub(points[1], points[0]);
    let length = first[0].hypot(first[1]);
    if length > 0.0 {
        end_mark(drafter, &ctx.space, &look, arrow, arrow_size, points[0], mul(first, 1.0 / length), path);
    }
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let lines: Vec<String> = spec.text.split('\n').map(str::to_string).collect();
    let align = Align { h: if rightward { 0 } else { 2 }, v: 1 };
    let at = [last[0] + if rightward { 0.5 * s } else { -0.5 * s }, last[1] + 0.6 * s];
    let text_style = drafter.text_style(None);
    let placed = Placed { at, lines: &lines, height, align, rotation: 0.0, width_factor: 1.0, line_spacing: 1.0, style: text_style };
    text::put(drafter, &ctx.space, &text_look, placed, path);
    Ok(())
}

/// 多角形の符号付きの面積（反時計回りが正）と重心
fn area_and_centroid(points: &[Point]) -> (f64, Point) {
    let (mut area, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for (index, a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        let cross = a[0] * b[1] - b[0] * a[1];
        area += cross;
        cx += (a[0] + b[0]) * cross;
        cy += (a[1] + b[1]) * cross;
    }
    area /= 2.0;
    if area.abs() < 1e-12 {
        let n = points.len() as f64;
        return (0.0, [points.iter().map(|p| p[0]).sum::<f64>() / n, points.iter().map(|p| p[1]).sum::<f64>() / n]);
    }
    (area, [cx / (6.0 * area), cy / (6.0 * area)])
}

pub(crate) fn polygon_label(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &PolygonLabelSpec,
    path: &str,
) -> Result<(), DraftError> {
    let points = points(&spec.points, 3, path)?;
    let s = ctx.scale;
    if spec.outline {
        let look = drafter.look(ctx, style, None, path)?;
        drafter.add(&ctx.space, &look, "LWPOLYLINE", lwpolyline(&points, None, true), path);
    }
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let height = positive(spec.text_height.unwrap_or_else(|| drafter.text_height()), "textHeight", path)? * s;
    let unit = unit_of(spec.unit.as_deref(), path)?;
    let (area, centroid) = area_and_centroid(&points);
    let orientation = if area >= 0.0 { 1.0 } else { -1.0 };
    if spec.lengths.unwrap_or(true) {
        for (index, a) in points.iter().enumerate() {
            let b = points[(index + 1) % points.len()];
            let d = sub(b, *a);
            let length = d[0].hypot(d[1]);
            if length <= 0.0 {
                continue;
            }
            let e = mul(d, 1.0 / length);
            // 反時計回りの多角形の外側は、辺の向きの右
            let outward = mul([e[1], -e[0]], orientation);
            let mut reading = e[1].atan2(e[0]).to_degrees();
            if reading > 90.0 + 1e-9 {
                reading -= 180.0;
            } else if reading <= -90.0 + 1e-9 {
                reading += 180.0;
            }
            let up = rotate([1.0, 0.0], reading + 90.0);
            let align = if outward[0] * up[0] + outward[1] * up[1] >= 0.0 { BOTTOM_CENTER } else { Align { h: 1, v: 3 } };
            let at = add(mul(add(*a, b), 0.5), mul(outward, 0.8 * s));
            let content = length_text(length, unit, spec.precision);
            label(drafter, ctx, &text_look, at, &content, height, align, reading, path);
        }
    }
    let format = match &spec.area {
        None | Some(Value::Bool(false)) | Some(Value::Null) => None,
        Some(Value::Bool(true)) => Some("{}㎡".to_string()),
        Some(Value::String(format)) => Some(format.clone()),
        Some(_) => return Err(DraftError::new("invalid-value", path, "area は true か、{} を含む文字列で指定してください")),
    };
    if let Some(format) = format {
        let value = format_number(area.abs() / 1_000_000.0, spec.area_precision.unwrap_or(2), true);
        let content = if format.contains("{}") { format.replace("{}", &value) } else { format!("{format}{value}") };
        let at = match spec.area_at {
            Some(at) => point(at, path)?,
            None => centroid,
        };
        label(drafter, ctx, &text_look, at, &content, height, MIDDLE_CENTER, 0.0, path);
    }
    Ok(())
}

/// ビューの見出し: 番号の丸、名前、縮尺（S=1:250）と、その下の線。at は番号の丸の中心の高さの左端
pub(crate) fn view_title(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &ViewTitleSpec,
    scale: Option<f64>,
    path: &str,
) -> Result<(), DraftError> {
    let Some(at) = spec.at else {
        return Err(DraftError::new("invalid-value", path, "viewTitle の at がありません"));
    };
    let at = point(at, path)?;
    let look = drafter.look(ctx, style, None, path)?;
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let s = ctx.scale;
    let height = positive(spec.height.unwrap_or(4.0), "height", path)? * s;
    let mut x = at[0];
    let baseline = at[1] - height / 2.0;
    if let Some(number) = spec.number.as_deref().filter(|number| !number.is_empty()) {
        let r = height * 0.85;
        let center = [x + r, at[1]];
        drafter.add(&ctx.space, &look, "CIRCLE", DxfEntityData::Circle { center: xyz(center), radius: r }, path);
        label(drafter, ctx, &text_look, center, number, height * 0.8, MIDDLE_CENTER, 0.0, path);
        x += 2.0 * r + 2.0 * s;
    }
    let title_start = x;
    label(drafter, ctx, &text_look, [x, baseline], &spec.text, height, BOTTOM_LEFT, 0.0, path);
    x += text::width(&spec.text, height, 1.0) + 3.0 * s;
    let scale_text = spec.scale_text.clone().or_else(|| scale.map(|scale| format!("S=1:{}", format_number(scale, 0, false))));
    if let Some(scale_text) = scale_text.filter(|text| !text.is_empty()) {
        let small = height * 0.7;
        label(drafter, ctx, &text_look, [x, baseline], &scale_text, small, BOTTOM_LEFT, 0.0, path);
        x += text::width(&scale_text, small, 1.0);
    }
    line(drafter, &ctx.space, &look, [title_start - 1.0 * s, baseline - 1.2 * s], [x + 1.0 * s, baseline - 1.2 * s], path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_signed_area_and_centroid() {
        let square = [[0.0, 0.0], [2000.0, 0.0], [2000.0, 1000.0], [0.0, 1000.0]];
        let (area, centroid) = area_and_centroid(&square);
        assert_eq!(area, 2_000_000.0);
        assert_eq!(centroid, [1000.0, 500.0]);
        let reversed: Vec<Point> = square.iter().rev().copied().collect();
        assert_eq!(area_and_centroid(&reversed).0, -2_000_000.0);
    }

    #[test]
    fn writes_lengths_in_meters() {
        assert_eq!(length_text(12345.0, "m", None), "12.345");
        assert_eq!(length_text(10000.0, "m", None), "10");
        assert_eq!(length_text(10000.0, "m", Some(2)), "10.00");
        assert_eq!(length_text(6400.0, "mm", None), "6,400");
    }
}
