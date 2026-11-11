//! JWW に写した図面が、元の図面と同じに描けるかを比べる。
//!
//! 元の図面の表示を JWW の用紙の座標（左下が原点の用紙の mm）に移し、写した JWW を読み直した表示の図形を、
//! 写すときに覚えた「JWW の図形 → 元の図形の id」で元の図形に対応させ、元の図形ごとに比べる。1 つの元の図形は
//! 複数の JWW の図形になる（折線は線と円弧、塗りは台形のソリッド、文字・寸法は線・ソリッド・文字）ので、同じ
//! 元の図形から書いた図形はまとめて比べる。
//!
//! 比べるもの（用紙の mm）:
//! - 外接矩形。円弧・曲線を折線にする規則が DXF と JWW の表示で違うので、0.05 mm と大きさの 1/10,000 まで
//! - 線: 色・線の太さ（JWW の 1/100 mm に丸めた値）・破線（点は 0.25 mm の線分）の組と、線の集まりどうしの
//!   ハウスドルフ距離
//! - 塗り: 色と、偶奇規則で塗る面積（走査線で数える）
//! - ハッチング線: 線が模様の線の上にあり、領域の内側にあること、長さの合計
//! - 点: 位置と色、文字: 文字列・色・書体と文字範囲の四隅、1 文字ずつの位置
//! - 描くか: 描かない図形（JWW の画像の配置、DXF の外形を表示しない画像）は描かない図形と、描く図形は描く図形と比べる
//! - underlay（`underlay_differences`）: ファイル、中身の全体を置く場所（中身の原点と幅・高さの辺）、幅と高さの比、
//!   切り取りと不透明度。中身の単位は形式ごとに違う（JWW・SXF は用紙の 0.01 mm、DXF は画素。decision 0031）

#![allow(dead_code)]

use std::collections::HashMap;
use zumen_core::{BBox, Matrix, Primitive, Scene, SceneEntity, StrokePrimitive, TextPrimitive, Underlay};
use zumen_jww::{JwwDocument, JwwEntity};

pub const TOLERANCE: f64 = 0.05;

/// 表示を変換する（点・輪郭・文字・外接矩形・underlay。破線の長さは dash_scale 倍）
pub fn transform_scene(scene: &mut Scene, m: &Matrix, dash_scale: f64) {
    let points = |points: &mut Vec<f64>| *points = zumen_core::geometry::transform_points(m, points);
    let transform_box = |b: BBox| {
        let mut bbox = zumen_core::geometry::empty_box();
        for (x, y) in [(b.min_x, b.min_y), (b.max_x, b.min_y), (b.max_x, b.max_y), (b.min_x, b.max_y)] {
            let (px, py) = zumen_core::geometry::apply(m, x, y);
            zumen_core::geometry::extend_box(&mut bbox, px, py);
        }
        bbox
    };
    for underlay in &mut scene.underlays {
        underlay.matrix = zumen_core::geometry::multiply(m, &underlay.matrix);
        underlay.clip.iter_mut().flatten().for_each(points);
        underlay.bbox = transform_box(underlay.bbox);
    }
    for entity in &mut scene.entities {
        entity.bbox = transform_box(entity.bbox);
        for primitive in &mut entity.primitives {
            match primitive {
                Primitive::Stroke(stroke) => {
                    points(&mut stroke.points);
                    stroke.dashes.iter_mut().for_each(|value| *value *= dash_scale);
                }
                Primitive::Fill(fill) => fill.rings.iter_mut().for_each(points),
                Primitive::Clipped(clipped) => {
                    clipped.rings.iter_mut().for_each(points);
                    for stroke in &mut clipped.strokes {
                        points(&mut stroke.points);
                        stroke.dashes.iter_mut().for_each(|value| *value *= dash_scale);
                    }
                }
                Primitive::Dot(dot) => (dot.x, dot.y) = zumen_core::geometry::apply(m, dot.x, dot.y),
                Primitive::Text(text) => text.matrix = zumen_core::geometry::multiply(m, &text.matrix),
            }
        }
    }
}

/// 描く図形の外接矩形の和（描かない画像の外形は、写さないことがある）
pub fn entity_bounds(scene: &Scene) -> BBox {
    let mut bounds = zumen_core::geometry::empty_box();
    for entity in scene.entities.iter().filter(|entity| entity.drawn) {
        zumen_core::geometry::union_box(&mut bounds, &entity.bbox);
    }
    bounds
}

/// JWW の表示の図形の id（#n>#m…）を、写すときに覚えた元の図形の id にする
pub fn jww_source(id: &str, doc: &JwwDocument, sources: &HashMap<String, String>) -> Option<String> {
    let mut definition: Option<u32> = None;
    let mut out = String::new();
    for part in id.split('>') {
        let n: usize = part.strip_prefix('#')?.parse().ok()?;
        let (key, entity) = match definition {
            None => (format!("#{n}"), doc.entities.get(n.checked_sub(1)?)?),
            Some(number) => {
                let block = doc.blocks.iter().find(|block| block.number == number)?;
                (format!("{number}#{n}"), block.entities.get(n.checked_sub(1)?)?)
            }
        };
        let source = sources.get(&key)?;
        if !out.is_empty() && !out.ends_with(':') && !out.ends_with('>') && !source.is_empty() {
            out.push('>');
        }
        out.push_str(source);
        definition = match entity {
            JwwEntity::Block(block) => Some(block.definition),
            _ => None,
        };
    }
    Some(out)
}

/// DXF の表示の id（[ビューポート:]ハンドル>ハンドル…）を、DXF に写す前の図形の id にする（ハンドル → 元の id）
pub fn through_handles(id: &str, sources: &HashMap<String, String>) -> String {
    let handles = id.split([':', '>']).filter(|handle| !handle.is_empty());
    let original: Vec<&str> = handles
        .map(|handle| sources.get(handle.split('[').next().unwrap_or(handle)).map_or("?", String::as_str))
        .filter(|source| !source.is_empty())
        .collect();
    original.join(">")
}

fn luminance(color: &str) -> Option<f64> {
    let hex = color.strip_prefix('#')?;
    if hex.len() < 6 {
        return None;
    }
    let part = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok().map(f64::from);
    Some((part(0)? * 0.299 + part(2)? * 0.587 + part(4)? * 0.114) / 255.0)
}

/// 背景（白・黒）に描く色（zumen-render の Painter::color と同じ規則）
fn drawn(color: &str, light: bool) -> String {
    let rgb: String = color.chars().take(7).collect();
    match luminance(color) {
        Some(value) if light && value > 0.85 => "#000000".into(),
        Some(value) if !light && value < 0.05 => "#ffffff".into(),
        _ => rgb,
    }
}

pub fn same_color(a: &str, b: &str) -> bool {
    // 背景色の塗り（ワイプアウト）は、JWW には白のソリッドで書く
    let background = |x: &str, y: &str| x == zumen_core::BACKGROUND_COLOR && y == "#ffffff";
    a == b || background(a, b) || (drawn(a, true) == drawn(b, true) && drawn(a, false) == drawn(b, false))
}

/// JWW の線幅（1/100 mm、1 以上）にした線の太さ
pub fn jww_width(width: f64) -> f64 {
    (width * 100.0).round().max(1.0) / 100.0
}

/// 線種の点（長さ 0 の線分）は、JWW には 0.25 mm の線分にして書く（zumen-convert の pitch と同じ）。用紙で周期が
/// 0.05 mm に満たない破線は実線
pub fn jww_dashes(dashes: &[f64]) -> Vec<f64> {
    let mut out = dashes.to_vec();
    if out.is_empty() || dashes.iter().map(|value| value.abs()).sum::<f64>() < 0.05 {
        return Vec::new();
    }
    if out.len() % 2 == 1 {
        out.push(0.0);
    }
    for i in (0..out.len()).step_by(2) {
        if out[i] <= 0.0 {
            out[i] = 0.25;
            if out[i + 1] > 0.5 {
                out[i + 1] -= 0.25;
            }
        }
        if out[i + 1] <= 0.0 {
            out[i + 1] = 0.25;
        }
    }
    out.truncate(10);
    out
}

fn same_dashes(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 1e-6 * x.abs().max(1.0))
}

fn distance_to(points: &[f64], closed: bool, px: f64, py: f64) -> f64 {
    let n = points.len() / 2;
    if n == 0 {
        return f64::INFINITY;
    }
    if n == 1 {
        return (points[0] - px).hypot(points[1] - py);
    }
    let segments = if closed { n } else { n - 1 };
    (0..segments)
        .map(|i| {
            let j = (i + 1) % n;
            zumen_core::geometry::distance_to_segment_squared(
                px,
                py,
                points[i * 2],
                points[i * 2 + 1],
                points[j * 2],
                points[j * 2 + 1],
            )
            .sqrt()
        })
        .fold(f64::INFINITY, f64::min)
}

/// 線の集まりどうしのハウスドルフ距離（頂点から相手の線分まで）
fn hausdorff(a: &[&StrokePrimitive], b: &[&StrokePrimitive]) -> f64 {
    let one = |from: &[&StrokePrimitive], to: &[&StrokePrimitive]| {
        from.iter()
            .flat_map(|stroke| stroke.points.chunks(2))
            .map(|point| {
                to.iter().map(|other| polyline_distance(other, point[0], point[1])).fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    };
    one(a, b).max(one(b, a))
}

fn polyline_distance(stroke: &StrokePrimitive, x: f64, y: f64) -> f64 {
    if stroke.segments == Some(true) {
        return stroke
            .points
            .chunks(4)
            .filter(|chunk| chunk.len() == 4)
            .map(|chunk| {
                zumen_core::geometry::distance_to_segment_squared(x, y, chunk[0], chunk[1], chunk[2], chunk[3]).sqrt()
            })
            .fold(f64::INFINITY, f64::min);
    }
    distance_to(&stroke.points, stroke.closed, x, y)
}

/// 偶奇規則で塗る輪郭の組の面積（走査線で数える）
pub fn scan_area(rings: &[&Vec<f64>]) -> f64 {
    let mut bounds = zumen_core::geometry::empty_box();
    for ring in rings {
        zumen_core::geometry::extend_box_with_points(&mut bounds, ring);
    }
    if bounds.min_y >= bounds.max_y || bounds.min_y.is_nan() {
        return 0.0;
    }
    let steps = 4000;
    let dy = (bounds.max_y - bounds.min_y) / steps as f64;
    let mut area = 0.0;
    for step in 0..steps {
        let y = bounds.min_y + dy * (step as f64 + 0.5);
        let mut xs: Vec<f64> = Vec::new();
        for ring in rings {
            let n = ring.len() / 2;
            for i in 0..n {
                let j = (i + 1) % n;
                let (x1, y1, x2, y2) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
                if (y1 > y) != (y2 > y) {
                    xs.push(x1 + (x2 - x1) * (y - y1) / (y2 - y1));
                }
            }
        }
        xs.sort_by(f64::total_cmp);
        area += xs.chunks(2).filter(|pair| pair.len() == 2).map(|pair| pair[1] - pair[0]).sum::<f64>() * dy;
    }
    area
}

fn ring_length(rings: &[&Vec<f64>]) -> f64 {
    rings
        .iter()
        .map(|ring| {
            let n = ring.len() / 2;
            (0..n)
                .map(|i| {
                    let j = (i + 1) % n;
                    (ring[j * 2] - ring[i * 2]).hypot(ring[j * 2 + 1] - ring[i * 2 + 1])
                })
                .sum::<f64>()
        })
        .sum()
}

fn inside(rings: &[Vec<f64>], x: f64, y: f64) -> bool {
    let mut result = false;
    for ring in rings {
        let n = ring.len() / 2;
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let (xi, yi, xj, yj) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                result = !result;
            }
            j = i;
        }
    }
    result
}

fn corners(text: &TextPrimitive) -> [[f64; 2]; 4] {
    let m = &text.matrix;
    let at = |x: f64, y: f64| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
    [at(0.0, 0.0), at(text.box_width, 0.0), at(text.box_width, text.box_height), at(0.0, text.box_height)]
}

fn compare_text(a: &TextPrimitive, b: &TextPrimitive) -> Option<String> {
    if a.text.replace('\t', "    ") != b.text {
        return Some(format!("text {:?} → {:?}", a.text, b.text));
    }
    if !same_color(&a.color, &b.color) {
        return Some(format!("text color {} → {}", a.color, b.color));
    }
    if a.font_family != b.font_family {
        return Some(format!("font {} → {}", a.font_family, b.font_family));
    }
    let (ca, cb) = (corners(a), corners(b));
    let extent = ca.iter().chain(&cb).flat_map(|p| [p[0].abs(), p[1].abs()]).fold(1.0, f64::max);
    for (p, q) in ca.iter().zip(&cb) {
        if (p[0] - q[0]).hypot(p[1] - q[1]) > 1e-6 * extent {
            return Some(format!("text box {ca:?} → {cb:?}"));
        }
    }
    if a.cells.len() != b.cells.len() {
        return Some(format!("{} cells → {}", a.cells.len(), b.cells.len()));
    }
    let glyph = |text: &TextPrimitive| {
        let m = &text.matrix;
        (m[2] * text.height).hypot(m[3] * text.height).min((m[0] * text.height).hypot(m[1] * text.height))
    };
    // 字の大きさの 1/4 まで。DXF を経た文字は文字間隔を文字の幅に含めるので、間隔の半分まで
    let tolerance = (glyph(a).max(glyph(b)) * 0.25).max(gap_allowance(a) + 1e-6);
    let place = |text: &TextPrimitive, x: f64, y: f64| {
        let m = &text.matrix;
        [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
    };
    for (p, q) in a.cells.iter().zip(&b.cells) {
        let (pa, pb) = (place(a, p.x, p.y), place(b, q.x, q.y));
        if p.character != q.character || (pa[0] - pb[0]).hypot(pa[1] - pb[1]) > tolerance {
            return Some(format!("cell {:?} at {pa:?} → {:?} at {pb:?}", p.character, q.character));
        }
    }
    None
}

/// 文字間隔（文字範囲の長さから字の並びの長さを引いた分）の半分（用紙の mm）。間隔を文字の幅に含めると、
/// 1 文字ずつの位置がこれだけずれる
fn gap_allowance(text: &TextPrimitive) -> f64 {
    let (Some(first), Some(second)) = (text.cells.first(), text.cells.get(1)) else {
        return 0.0;
    };
    let units: f64 = text.text.chars().map(|c| if zumen_core::text::is_half_width(c) { 0.5 } else { 1.0 }).sum();
    let m = &text.matrix;
    let vertical = (second.y - first.y).abs() > (second.x - first.x).abs();
    let (length, unit, scale) = if vertical {
        (text.box_height, text.height, m[2].hypot(m[3]))
    } else {
        (text.box_width, text.scale_x * text.height, m[0].hypot(m[1]))
    };
    ((length - units * unit) / 2.0).max(0.0) * scale
}

/// 描いても見えない要素（透明な線、点が 1 つの線、空白だけの文字、面積のない塗り）
fn invisible(primitive: &Primitive) -> bool {
    // 偶奇規則の面積（蝶ネクタイの四角形は符号つきの面積が 0 でも塗る）
    let flat = |ring: &Vec<f64>| scan_area(&[ring]) <= 1e-12;
    match primitive {
        Primitive::Stroke(stroke) => stroke.color == "transparent" || stroke.points.len() < 4,
        Primitive::Fill(fill) => fill.rings.iter().all(flat),
        Primitive::Text(text) => text.text.trim().is_empty(),
        Primitive::Clipped(clipped) => clipped.strokes.iter().all(|stroke| stroke.points.len() < 4),
        _ => false,
    }
}

/// 比べ方の規則: 元の線の太さ → JWW の線の太さと、破線を比べないか（ユーザー定義の線種に入らない線種を、近い線種に
/// したとき）
#[derive(Clone, Copy)]
pub struct Rules {
    pub width: fn(f64) -> f64,
    pub any_dashes: bool,
}

impl Default for Rules {
    fn default() -> Self {
        Rules { width: jww_width, any_dashes: false }
    }
}

/// DXF で使える線の太さ（mm）
pub fn dxf_lineweight(width: f64) -> f64 {
    const WEIGHTS: [f64; 24] = [
        0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        106.0, 120.0, 140.0, 158.0, 200.0, 211.0,
    ];
    let hundredths = width * 100.0;
    WEIGHTS.iter().copied().min_by(|a, b| (a - hundredths).abs().total_cmp(&(b - hundredths).abs())).unwrap_or(25.0)
        / 100.0
}

/// DXF を経て JWW に書いた線の太さ（DXF の線の太さに丸め、JWW の 1/100 mm にする）
pub fn width_through_dxf(width: f64) -> f64 {
    jww_width(dxf_lineweight(width))
}

/// 元の図形の要素と、そこから写した JWW の図形の要素を比べる
pub fn compare(expected: &[&Primitive], actual: &[&Primitive], tolerance: f64, rules: Rules) -> Vec<String> {
    let mut out = Vec::new();
    let strokes = |list: &[&Primitive]| -> Vec<StrokePrimitive> {
        list.iter()
            .filter_map(|primitive| match primitive {
                Primitive::Stroke(stroke) if !invisible(primitive) => Some(stroke.clone()),
                _ => None,
            })
            .collect()
    };
    let (expected_strokes, mut actual_strokes) = (strokes(expected), strokes(actual));
    // ハッチング線: 模様の線を領域で切った線分
    let clipped: Vec<&zumen_core::ClippedPrimitive> = expected
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Clipped(clipped) if !invisible(primitive) => Some(clipped),
            _ => None,
        })
        .collect();
    if !clipped.is_empty() {
        let mut hatch: Vec<StrokePrimitive> = Vec::new();
        actual_strokes.retain(|stroke| {
            let (mx, my) = ((stroke.points[0] + stroke.points[2]) / 2.0, (stroke.points[1] + stroke.points[3]) / 2.0);
            let scale = 1.0 + mx.abs().max(my.abs());
            let on_pattern = clipped.iter().any(|clip| {
                inside(&clip.rings, mx, my)
                    && clip.strokes.iter().any(|line| {
                        stroke.points.chunks(2).all(|point| polyline_distance(line, point[0], point[1]) <= 1e-6 * scale)
                    })
            });
            if on_pattern {
                hatch.push(stroke.clone());
            }
            !on_pattern
        });
        let length = |points: &[f64]| (points[2] - points[0]).hypot(points[3] - points[1]);
        let actual_length: f64 = hatch.iter().map(|stroke| length(&stroke.points)).sum();
        // 模様の線の、領域の内側の長さ（線分を細かく分けて数える）
        let mut expected_length = 0.0;
        for clip in &clipped {
            for line in &clip.strokes {
                let segments: Vec<[f64; 4]> = if line.segments == Some(true) {
                    line.points.chunks(4).filter(|c| c.len() == 4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
                } else {
                    line.points.windows(4).step_by(2).map(|c| [c[0], c[1], c[2], c[3]]).collect()
                };
                for [x1, y1, x2, y2] in segments {
                    let steps = 400;
                    let piece = (x2 - x1).hypot(y2 - y1) / steps as f64;
                    for step in 0..steps {
                        let t = (step as f64 + 0.5) / steps as f64;
                        if inside(&clip.rings, x1 + (x2 - x1) * t, y1 + (y2 - y1) * t) {
                            expected_length += piece;
                        }
                    }
                }
            }
        }
        if (actual_length - expected_length).abs() > expected_length * 0.02 + tolerance {
            out.push(format!("hatch lines {expected_length:.3} mm → {actual_length:.3} mm"));
        }
        // ハッチング線は色と破線を比べる（DXF の HATCH の模様の線は太さを持たない）
        let style = |stroke: &StrokePrimitive| (stroke.color.clone(), jww_dashes(&stroke.dashes));
        let styles: Vec<_> = clipped.iter().flat_map(|clip| clip.strokes.iter().map(style)).collect();
        if let Some(stroke) = hatch.iter().find(|stroke| {
            !styles.iter().any(|(color, dashes)| {
                same_color(color, &stroke.color) && (rules.any_dashes || same_dashes(dashes, &stroke.dashes))
            })
        }) {
            out.push(format!("hatch line style {} {:?} not in {styles:?}", stroke.color, stroke.dashes));
        }
    }
    if expected_strokes.is_empty() != actual_strokes.is_empty() {
        out.push(format!("{} strokes → {}", expected_strokes.len(), actual_strokes.len()));
    } else if !expected_strokes.is_empty() {
        let style =
            |stroke: &StrokePrimitive| (stroke.color.clone(), (rules.width)(stroke.width), jww_dashes(&stroke.dashes));
        for stroke in &actual_strokes {
            let found = expected_strokes.iter().any(|other| {
                let (color, width, dashes) = style(other);
                same_color(&color, &stroke.color)
                    && (width - stroke.width).abs() < 1e-9
                    && (rules.any_dashes || same_dashes(&dashes, &stroke.dashes))
            });
            if !found {
                out.push(format!(
                    "stroke style {} {} {:?} not in {:?}",
                    stroke.color,
                    stroke.width,
                    stroke.dashes,
                    expected_strokes.iter().map(style).collect::<Vec<_>>()
                ));
                break;
            }
        }
        let a: Vec<&StrokePrimitive> = expected_strokes.iter().collect();
        let b: Vec<&StrokePrimitive> = actual_strokes.iter().collect();
        let distance = hausdorff(&a, &b);
        if distance > tolerance {
            out.push(format!("stroke moved by {distance:.4} mm"));
        }
    }
    // 塗り
    let fills = |list: &[&Primitive]| -> Vec<(String, Vec<f64>)> {
        list.iter()
            .flat_map(|primitive| match primitive {
                Primitive::Fill(fill) => fill.rings.iter().map(|ring| (fill.color.clone(), ring.clone())).collect(),
                _ => Vec::new(),
            })
            .collect()
    };
    let (expected_fills, actual_fills) = (fills(expected), fills(actual));
    if expected_fills.is_empty() != actual_fills.is_empty() {
        out.push(format!("{} fill rings → {}", expected_fills.len(), actual_fills.len()));
    } else if !expected_fills.is_empty() {
        for (color, _) in &actual_fills {
            if !expected_fills.iter().any(|(other, _)| same_color(other, color)) {
                out.push(format!("fill color {color}"));
                break;
            }
        }
        let a: Vec<&Vec<f64>> = expected_fills.iter().map(|(_, ring)| ring).collect();
        let b: Vec<&Vec<f64>> = actual_fills.iter().map(|(_, ring)| ring).collect();
        // 台形のソリッドは重ならないので、面積の和が塗りの面積
        let actual_area: f64 = b.iter().map(|ring| scan_area(&[ring])).sum();
        let expected_area = scan_area(&a);
        if (actual_area - expected_area).abs() > expected_area * 0.01 + tolerance * ring_length(&a) {
            out.push(format!("fill area {expected_area:.3} → {actual_area:.3}"));
        }
    }
    // 点
    let dots = |list: &[&Primitive]| -> Vec<(f64, f64, String)> {
        list.iter()
            .filter_map(|primitive| match primitive {
                Primitive::Dot(dot) => Some((dot.x, dot.y, dot.color.clone())),
                _ => None,
            })
            .collect()
    };
    let (expected_dots, actual_dots) = (dots(expected), dots(actual));
    if expected_dots.len() != actual_dots.len() {
        out.push(format!("{} dots → {}", expected_dots.len(), actual_dots.len()));
    } else {
        for (a, b) in expected_dots.iter().zip(&actual_dots) {
            if (a.0 - b.0).hypot(a.1 - b.1) > 1e-6 * (1.0 + a.0.abs().max(a.1.abs())) || !same_color(&a.2, &b.2) {
                out.push(format!("dot {a:?} → {b:?}"));
                break;
            }
        }
    }
    // 文字
    let texts = |list: &[&Primitive]| -> Vec<TextPrimitive> {
        list.iter()
            .filter_map(|primitive| match primitive {
                Primitive::Text(text) if !invisible(primitive) => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let (expected_texts, actual_texts) = (texts(expected), texts(actual));
    if expected_texts.len() != actual_texts.len() {
        out.push(format!("{} texts → {}", expected_texts.len(), actual_texts.len()));
    } else if let Some(difference) = expected_texts.iter().zip(&actual_texts).find_map(|(a, b)| compare_text(a, b)) {
        out.push(difference);
    }
    out
}

/// 写した図面の図形を、元の図形の id ごとにまとめる
pub fn group<'a>(scene: &'a Scene, source: &dyn Fn(&str) -> Option<String>) -> HashMap<String, Vec<&'a SceneEntity>> {
    let mut out: HashMap<String, Vec<&SceneEntity>> = HashMap::new();
    for entity in &scene.entities {
        let id = source(&entity.id).unwrap_or_else(|| format!("?{}", entity.id));
        out.entry(id).or_default().push(entity);
    }
    out
}

/// 元の図面（JWW の用紙の座標に移したもの）と、写した JWW の図面の違い
/// 文字の高さの数値を保って書き直した図面に描くはずの、元の図面（decision 0019）。SXF・JWW から DXF へは
/// （`to_em`）字形を em に対する文字の高さの比の逆数で大きく、DXF から SXF・JWW へはその比で小さくする。
/// ベースラインの左端（縦書きはベースラインの上端）は動かさない。縦と横の縮尺が違う配置の中の文字は、斜体の
/// ずれと配置のゆがみを matrix から分けられず、挿入点がわからないので、その図形の id も返す
pub fn scale_texts(scene: &Scene, to_em: bool) -> (Scene, std::collections::HashSet<String>) {
    let mut out = scene.clone();
    let mut distorted = std::collections::HashSet::new();
    for entity in &mut out.entities {
        let mut scaled = false;
        for primitive in &mut entity.primitives {
            let Primitive::Text(text) = primitive else { continue };
            let em = zumen_core::text::em_for(&text.text);
            let k = if to_em { em } else { 1.0 / em };
            let vertical = match text.cells.as_slice() {
                [first, second, ..] => (first.y - second.y).abs() > (first.x - second.x).abs(),
                _ => (text.height - text.box_height).abs() > 1e-9 * text.box_height.max(1e-12),
            };
            // TEXT の挿入点（ベースラインの左端、縦書きは上端）を中心に大きさを変える。斜体のずれは文字の X 方向
            let m = text.matrix;
            let shear = (m[0] * m[2] + m[1] * m[3]) / (m[0] * m[0] + m[1] * m[1]);
            let (ux, uy) = (m[2] - shear * m[0], m[3] - shear * m[1]);
            if shear.abs() > 1e-9 && (ux.hypot(uy) - m[0].hypot(m[1])).abs() > 1e-9 * m[0].hypot(m[1]) {
                distorted.insert(entity.id.clone());
            }
            let descent = zumen_dxf::TEXT_DESCENT;
            let (x, y) =
                if vertical { (descent * text.box_width, text.box_height) } else { (0.0, descent * text.box_height) };
            let (px, py) = (m[4] + m[0] * x + ux * y, m[5] + m[1] * x + uy * y);
            text.matrix = [k * m[0], k * m[1], k * m[2], k * m[3], px + k * (m[4] - px), py + k * (m[5] - py)];
            scaled = true;
        }
        if scaled {
            let mut bbox = zumen_core::geometry::empty_box();
            for primitive in &entity.primitives {
                extend_primitive_box(&mut bbox, primitive);
            }
            entity.bbox = bbox;
        }
    }
    (out, distorted)
}

/// `scale_texts` が挿入点をわからなかった文字の、位置の違い（外接矩形・文字範囲・1 文字ずつの位置）
pub fn is_text_place(line: &str, distorted: &std::collections::HashSet<String>) -> bool {
    let id = line.split(' ').next().unwrap_or("");
    distorted.contains(id) && [": box [", ": text box ", ": cell "].iter().any(|part| line.contains(part))
}

fn extend_primitive_box(bbox: &mut BBox, primitive: &Primitive) {
    use zumen_core::geometry::{extend_box, extend_box_with_points};
    match primitive {
        Primitive::Stroke(stroke) => extend_box_with_points(bbox, &stroke.points),
        Primitive::Fill(fill) => fill.rings.iter().for_each(|ring| extend_box_with_points(bbox, ring)),
        Primitive::Clipped(clipped) => clipped.rings.iter().for_each(|ring| extend_box_with_points(bbox, ring)),
        Primitive::Dot(dot) => extend_box(bbox, dot.x, dot.y),
        Primitive::Text(text) => corners(text).iter().for_each(|[x, y]| extend_box(bbox, *x, *y)),
    }
}

pub fn differences(
    original: &Scene,
    again: &Scene,
    source: &dyn Fn(&str) -> Option<String>,
    rules: Rules,
) -> Vec<String> {
    let groups = group(again, source);
    let mut out = Vec::new();
    let mut matched = 0;
    for entity in &original.entities {
        let expected: Vec<&Primitive> = entity.primitives.iter().filter(|primitive| !invisible(primitive)).collect();
        let id = &entity.id;
        let Some(converted) = groups.get(id) else {
            // 描かない図形（選ぶためだけの外形）は、写さなくても見た目が変わらない。画像の中身は underlay で比べる
            if !expected.is_empty() && entity.drawn {
                out.push(format!("{id} ({}): missing", entity.type_name));
            }
            continue;
        };
        matched += 1;
        // 描く図形は描く図形と、描かない図形は描かない図形と比べる（SXF の画像の範囲の折線は描き、JWW の画像の配置は
        // 描かない。DXF の画像の外形は IMAGEFRAME によって描く）
        let converted: &[&SceneEntity] =
            &converted.iter().copied().filter(|part| part.drawn == entity.drawn).collect::<Vec<_>>();
        if converted.is_empty() {
            out.push(format!("{id} ({}): drawn {} → {}", entity.type_name, entity.drawn, !entity.drawn));
            continue;
        }
        let actual: Vec<&Primitive> = converted.iter().flat_map(|entity| entity.primitives.iter()).collect();
        // 描かない図形の外形は、線の太さを持たない（JWW の線の太さに丸めない）
        let rules = if entity.drawn { rules } else { Rules { width: |width| width, ..rules } };
        let mut bbox = zumen_core::geometry::empty_box();
        for part in converted {
            zumen_core::geometry::union_box(&mut bbox, &part.bbox);
        }
        let b = &entity.bbox;
        let size = (b.max_x - b.min_x).max(b.max_y - b.min_y).max(0.0);
        let tolerance = TOLERANCE + size * 1e-4;
        let close = |p: f64, q: f64| (p - q).abs() <= tolerance;
        let same = close(b.min_x, bbox.min_x)
            && close(b.min_y, bbox.min_y)
            && close(b.max_x, bbox.max_x)
            && close(b.max_y, bbox.max_y);
        // ハッチングの模様は領域の輪郭を描かないので、模様の線は元の外接矩形（輪郭）の内側にあればよい
        let hatched = expected.iter().any(|primitive| matches!(primitive, Primitive::Clipped(_)));
        let inside = bbox.min_x >= b.min_x - tolerance
            && bbox.min_y >= b.min_y - tolerance
            && bbox.max_x <= b.max_x + tolerance
            && bbox.max_y <= b.max_y + tolerance;
        if !expected.is_empty() && !(same || (hatched && inside)) {
            out.push(format!(
                "{id} ({}): box [{:.3}, {:.3}, {:.3}, {:.3}] → [{:.3}, {:.3}, {:.3}, {:.3}]",
                entity.type_name, b.min_x, b.min_y, b.max_x, b.max_y, bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y
            ));
        }
        for difference in compare(&expected, &actual, tolerance, rules) {
            out.push(format!("{id} ({}): {difference}", entity.type_name));
        }
    }
    let expected_ids: std::collections::HashSet<&str> =
        original.entities.iter().map(|entity| entity.id.as_str()).collect();
    for (id, entities) in &groups {
        if !expected_ids.contains(id.as_str()) {
            out.push(format!("{id}: not in the original ({} entities, {})", entities.len(), entities[0].type_name));
        }
    }
    if matched == 0 && !original.entities.is_empty() {
        out.push("no entity matched".into());
    }
    out
}

/// underlay の中身の全体を置く場所: 中身の原点と、幅・高さの辺のベクトル（用紙の mm）
fn underlay_place(underlay: &Underlay) -> [f64; 6] {
    let m = &underlay.matrix;
    [m[4], m[5], m[0] * underlay.width, m[1] * underlay.width, m[2] * underlay.height, m[3] * underlay.height]
}

/// 元の図面と、写した図面の underlay の違い。中身の単位は形式ごとに違ってよいので、matrix は中身の全体を置く場所で
/// 比べ、幅と高さの比は、画素数を整数に丸める違い（幅と高さの 1 単位分）まで同じであること。中身の全体と同じ
/// 切り取りは、切り取らないのと同じ
pub fn underlay_differences(original: &Scene, again: &Scene, source: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let mut converted: HashMap<String, Vec<&Underlay>> = HashMap::new();
    for underlay in &again.underlays {
        let id = source(&underlay.id).unwrap_or_else(|| format!("?{}", underlay.id));
        converted.entry(id).or_default().push(underlay);
    }
    let mut out = Vec::new();
    for before in &original.underlays {
        let id = &before.id;
        let after = match converted.remove(id).as_deref() {
            Some([after]) => *after,
            Some(list) => {
                out.push(format!("{id}: {} underlays", list.len()));
                continue;
            }
            None => {
                out.push(format!("{id}: underlay missing"));
                continue;
            }
        };
        if (before.kind, &before.file, before.page) != (after.kind, &after.file, after.page) {
            out.push(format!(
                "{id}: underlay {:?} {:?} {:?} → {:?} {:?} {:?}",
                before.kind, before.file, before.page, after.kind, after.file, after.page
            ));
        }
        let b = &before.bbox;
        let tolerance = TOLERANCE + (b.max_x - b.min_x).max(b.max_y - b.min_y).max(0.0) * 1e-4;
        let (a, b) = (underlay_place(before), underlay_place(after));
        if a.iter().zip(&b).any(|(p, q)| (p - q).abs() > tolerance) {
            out.push(format!("{id}: underlay placed {a:?} → {b:?}"));
        }
        let (ratio, again_ratio) = (before.width / before.height, after.width / after.height);
        let allowance = ratio * (1.0 / before.width.min(after.width) + 1.0 / before.height.min(after.height));
        let same_ratio = (ratio - again_ratio).abs() <= allowance;
        if !same_ratio {
            out.push(format!(
                "{id}: underlay size {} × {} → {} × {}",
                before.width, before.height, after.width, after.height
            ));
        }
        let clip = |underlay: &Underlay| -> Option<Vec<Vec<f64>>> {
            let full = zumen_core::geometry::transform_points(
                &underlay.matrix,
                &[0.0, 0.0, underlay.width, 0.0, underlay.width, underlay.height, 0.0, underlay.height],
            );
            underlay.clip.clone().filter(|rings| !(rings.len() == 1 && hausdorff_rings(&rings[0], &full) <= tolerance))
        };
        match (clip(before), clip(after)) {
            (None, None) => {}
            (Some(a), Some(b)) => {
                if a.len() != b.len()
                    || a.iter().any(|ring| !b.iter().any(|other| hausdorff_rings(ring, other) <= tolerance))
                {
                    out.push(format!("{id}: underlay clip moved"));
                }
            }
            (a, b) => out.push(format!("{id}: underlay clip {} → {}", a.is_some(), b.is_some())),
        }
        if (before.opacity.unwrap_or(1.0) - after.opacity.unwrap_or(1.0)).abs() > 1e-9 {
            out.push(format!("{id}: underlay opacity {:?} → {:?}", before.opacity, after.opacity));
        }
    }
    for (id, list) in converted {
        out.push(format!("{id}: underlay not in the original ({})", list.len()));
    }
    out.sort();
    out
}

/// 閉じた折線どうしのハウスドルフ距離
fn hausdorff_rings(a: &[f64], b: &[f64]) -> f64 {
    let one_way = |from: &[f64], to: &[f64]| {
        from.as_chunks::<2>().0.iter().map(|[x, y]| distance_to(to, true, *x, *y)).fold(0.0, f64::max)
    };
    one_way(a, b).max(one_way(b, a))
}
