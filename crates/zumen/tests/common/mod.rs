//! SXF・JWW を DXF に書き直した図面が、元の図面と同じに描けるかを比べる。
//!
//! 書き直した DXF は、モデル空間が実寸で、用紙のレイアウトのビューポートが元の用紙と同じ位置・縮尺に見せる。
//! そのレイアウト（DXF を開いたときの表示）の図形を、写すときに覚えた「ハンドル → 元の図形の id」で元の図形に
//! 対応させ、元の図形ごとに比べる。1 つの元の図形を複数の図形に分けて書くこと（ハッチングと外形の線、引出し線の
//! 線・矢印・文字）があるので、同じ元の図形から書いた図形はまとめて比べる。
//!
//! 比べるもの（用紙の mm）:
//! - 外接矩形。円弧・曲線を折線にする規則が SXF・JWW と DXF の表示で違うので、0.05 mm と大きさの 1/10,000 まで
//! - 描く要素の種類と順。線は色・線の太さ（DXF で使える太さに丸めた値）・破線と、点列の間の距離（ハウスドルフ距離）、
//!   塗りは色と輪郭、ハッチング線は色・破線と線の向き・間隔・位置、点は位置と色、文字は文字列・色・フォントと
//!   文字範囲の四隅、1 文字ずつの位置（文字間隔は DXF に書けず、文字の幅に含めるので、字の大きさの 1/4 まで）
//! - 色は、白い背景と黒い背景に描くときの色で比べる（SXF・JWW の黒と白は AutoCAD の前景色 ACI 7 にする）
//! - レイヤは、書いた図形のレイヤが元の図形のレイヤ（配置のレイヤを含む）にあること（レイヤ 0 を除く）。
//!   ビューポートに映す部分図は、配置のレイヤを持たない
//! - 実寸の縮尺: 部分図・レイヤグループの図形は、ビューポートの縮尺が元の配置の縮尺と同じこと

#![allow(dead_code)]

use std::collections::HashMap;
use zumen::{BBox, Primitive, Scene, SceneEntity, StrokePrimitive, TextPrimitive};

pub const TOLERANCE: f64 = 0.05;

/// 変換した図面の id（`[ビューポート:]ハンドル>ハンドル…`）を、元の図面の id にする
pub fn source_id(id: &str, sources: &HashMap<String, String>) -> Option<String> {
    let (viewport, path) = match id.split_once(':') {
        Some((viewport, path)) => (Some(viewport), path),
        None => (None, id),
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(viewport) = viewport {
        let source = sources.get(viewport)?;
        if !source.is_empty() {
            parts.push(source.clone());
        }
    }
    for handle in path.split('>') {
        let handle = handle.split('[').next().unwrap_or(handle);
        let source = sources.get(handle)?;
        if !source.is_empty() {
            parts.push(source.clone());
        }
    }
    // SXF に書いた図形の元の id の部分は、ビューポートの配置が「ハンドル:」、ブロック参照の配置が「ハンドル>」で終わる
    let mut out = String::new();
    for part in &parts {
        if !out.is_empty() && !out.ends_with(':') && !out.ends_with('>') {
            out.push('>');
        }
        out.push_str(part);
    }
    (!parts.is_empty()).then_some(out)
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
    // 背景色の塗り（白抜きの矢印）は、SXF には白の塗りで書く（SXF の用紙は白）
    let background = |x: &str, y: &str| x == zumen::BACKGROUND_COLOR && y == "#ffffff";
    a == b || background(a, b) || (drawn(a, true) == drawn(b, true) && drawn(a, false) == drawn(b, false))
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// 点 (px, py) から折線までの距離
fn distance_to(points: &[f64], closed: bool, px: f64, py: f64) -> f64 {
    let n = points.len() / 2;
    if n == 0 {
        return f64::INFINITY;
    }
    if n == 1 {
        return (points[0] - px).hypot(points[1] - py);
    }
    let mut best = f64::INFINITY;
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let j = (i + 1) % n;
        let (x1, y1, x2, y2) = (points[i * 2], points[i * 2 + 1], points[j * 2], points[j * 2 + 1]);
        let squared = zumen::geometry::distance_to_segment_squared(px, py, x1, y1, x2, y2);
        best = best.min(squared.sqrt());
    }
    best
}

/// 折線どうしのハウスドルフ距離（頂点から相手の線分まで）
fn hausdorff(a: &[f64], a_closed: bool, b: &[f64], b_closed: bool) -> f64 {
    let one = |from: &[f64], to: &[f64], closed: bool| {
        from.chunks(2).map(|point| distance_to(to, closed, point[0], point[1])).fold(0.0, f64::max)
    };
    one(a, b, b_closed).max(one(b, a, a_closed))
}

fn size(bbox: &BBox) -> f64 {
    (bbox.max_x - bbox.min_x).max(bbox.max_y - bbox.min_y).max(0.0)
}

fn transparent(primitive: &Primitive) -> bool {
    matches!(primitive, Primitive::Stroke(stroke) if stroke.color == "transparent")
}

/// 描いても見えない要素（透明な線、点が 1 つの線、空白だけの文字、線のないハッチング線）
fn invisible(primitive: &Primitive) -> bool {
    match primitive {
        Primitive::Stroke(stroke) => stroke.color == "transparent" || stroke.points.len() < 4,
        Primitive::Text(text) => text.text.trim().is_empty(),
        Primitive::Clipped(clipped) => clipped.strokes.iter().all(|stroke| stroke.points.len() < 4),
        _ => false,
    }
}

/// DXF の名前に使えない文字を _ にした名前（zumen-convert と同じ規則）。重なる名前に付けた「 (2)」は除く
fn dxf_name(name: &str) -> String {
    let name: String =
        name.trim().chars().map(|c| if "<>/\\\":;?*|=`".contains(c) || c.is_control() { '_' } else { c }).collect();
    name
}

fn without_number(name: &str) -> &str {
    match name.rsplit_once(" (") {
        Some((base, number)) if number.ends_with(')') && number[..number.len() - 1].parse::<u32>().is_ok() => base,
        _ => name,
    }
}

/// 比べる要素の並び（見えない要素を除き、同じ領域を切り取るハッチング線と、端がつながる同じ見た目の線をまとめる）
fn normalized(primitives: &[&Primitive]) -> Vec<Primitive> {
    let mut out: Vec<Primitive> = Vec::new();
    for primitive in primitives {
        if invisible(primitive) {
            continue;
        }
        match (primitive, out.last_mut()) {
            (Primitive::Clipped(next), Some(Primitive::Clipped(last))) if last.rings == next.rings => {
                last.strokes.extend(next.strokes.iter().cloned());
                continue;
            }
            (Primitive::Stroke(next), Some(Primitive::Stroke(last)))
                if !last.closed
                    && !next.closed
                    && last.segments.is_none()
                    && next.segments.is_none()
                    && last.color == next.color
                    && last.width == next.width
                    && last.dashes == next.dashes
                    && last.points[last.points.len() - 2..] == next.points[..2] =>
            {
                last.points.extend_from_slice(&next.points[2..]);
                continue;
            }
            _ => {}
        }
        out.push((*primitive).clone());
    }
    out
}

/// DXF で使える線の太さ（mm）
fn lineweight(width: f64) -> f64 {
    const WEIGHTS: [f64; 24] = [
        0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        106.0, 120.0, 140.0, 158.0, 200.0, 211.0,
    ];
    let hundredths = width * 100.0;
    WEIGHTS.iter().copied().min_by(|a, b| (a - hundredths).abs().total_cmp(&(b - hundredths).abs())).unwrap() / 100.0
}

fn same_dashes(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| close(*x, *y, 1e-6 * x.abs().max(1.0)))
}

fn compare_stroke(a: &StrokePrimitive, b: &StrokePrimitive, tolerance: f64) -> Option<String> {
    if !same_color(&a.color, &b.color) {
        return Some(format!("stroke color {} → {}", a.color, b.color));
    }
    if !close(lineweight(a.width), b.width, 1e-9) {
        return Some(format!("stroke width {} → {}", a.width, b.width));
    }
    if !same_dashes(&a.dashes, &b.dashes) {
        return Some(format!("dashes {:?} → {:?}", a.dashes, b.dashes));
    }
    let distance = hausdorff(&a.points, a.closed, &b.points, b.closed);
    if distance > tolerance {
        return Some(format!("stroke moved by {distance:.4} mm"));
    }
    None
}

fn corners(text: &TextPrimitive) -> [[f64; 2]; 4] {
    let m = &text.matrix;
    let at = |x: f64, y: f64| [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
    [at(0.0, 0.0), at(text.box_width, 0.0), at(text.box_width, text.box_height), at(0.0, text.box_height)]
}

fn compare_text(a: &TextPrimitive, b: &TextPrimitive) -> Option<String> {
    if a.text != b.text {
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
    // 1 文字ずつの位置（字の大きさの 1/4 まで。文字間隔は DXF に書けず文字の幅に含めるので、間隔の半分まで）
    let glyph = |text: &TextPrimitive| {
        let m = &text.matrix;
        (m[2] * text.height).hypot(m[3] * text.height).min((m[0] * text.height).hypot(m[1] * text.height))
    };
    let tolerance = (glyph(a).max(glyph(b)) * 0.25).max(gap_allowance(a) + 1e-6);
    if a.cells.len() != b.cells.len() {
        return Some(format!("{} cells → {}", a.cells.len(), b.cells.len()));
    }
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
    let units: f64 = text.text.chars().map(|c| if zumen::text::is_half_width(c) { 0.5 } else { 1.0 }).sum();
    let m = &text.matrix;
    let vertical = (second.y - first.y).abs() > (second.x - first.x).abs();
    let (length, unit, scale) = if vertical {
        (text.box_height, text.height, m[2].hypot(m[3]))
    } else {
        (text.box_width, text.scale_x * text.height, m[0].hypot(m[1]))
    };
    ((length - units * unit) / 2.0).max(0.0) * scale
}

/// ハッチング線の組の向き・間隔・位置（向きの角度と、原点からの符号付きの距離の並び）
fn lines_of(stroke: &StrokePrimitive) -> Option<(f64, f64, f64)> {
    let segments: Vec<&[f64]> = stroke.points.chunks(4).filter(|segment| segment.len() == 4).collect();
    let first = segments.first()?;
    let (dx, dy) = (first[2] - first[0], first[3] - first[1]);
    let length = dx.hypot(dy);
    if length <= 0.0 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    let mut angle = uy.atan2(ux);
    let (mut nx, mut ny) = (-uy, ux);
    if angle < 0.0 {
        angle += std::f64::consts::PI;
        nx = -nx;
        ny = -ny;
    }
    if angle >= std::f64::consts::PI - 1e-12 {
        angle -= std::f64::consts::PI;
        nx = -nx;
        ny = -ny;
    }
    let mut offsets: Vec<f64> = segments.iter().map(|segment| segment[0] * nx + segment[1] * ny).collect();
    offsets.sort_by(f64::total_cmp);
    let spacing = offsets.windows(2).map(|pair| pair[1] - pair[0]).fold(f64::INFINITY, f64::min);
    if !spacing.is_finite() {
        return Some((angle, 0.0, offsets[0]));
    }
    Some((angle, spacing, offsets[0].rem_euclid(spacing)))
}

fn compare_hatch_lines(a: &StrokePrimitive, b: &StrokePrimitive) -> Option<String> {
    if !same_color(&a.color, &b.color) {
        return Some(format!("hatch color {} → {}", a.color, b.color));
    }
    if !same_dashes(&a.dashes, &b.dashes) {
        return Some(format!("hatch dashes {:?} → {:?}", a.dashes, b.dashes));
    }
    match (lines_of(a), lines_of(b)) {
        (Some((angle_a, spacing_a, phase_a)), Some((angle_b, spacing_b, phase_b))) => {
            let phase = (phase_a - phase_b).abs();
            let phase = phase.min((spacing_a - phase).abs());
            if !close(angle_a, angle_b, 1e-6)
                || !close(spacing_a, spacing_b, 1e-6 * spacing_a.max(1.0))
                || (spacing_a > 0.0 && phase > 1e-6 * spacing_a.max(1.0))
            {
                return Some(format!(
                    "hatch lines {angle_a:.6}/{spacing_a:.6}/{phase_a:.6} → {angle_b:.6}/{spacing_b:.6}/{phase_b:.6}"
                ));
            }
            None
        }
        (None, None) => None,
        _ => Some("hatch lines differ".into()),
    }
}

fn compare_rings(a: &[Vec<f64>], b: &[Vec<f64>], tolerance: f64) -> Option<String> {
    if a.len() != b.len() {
        return Some(format!("{} rings → {}", a.len(), b.len()));
    }
    for ring in a {
        if !b.iter().any(|other| hausdorff(ring, true, other, true) <= tolerance) {
            return Some("ring moved".into());
        }
    }
    None
}

fn compare_primitive(a: &Primitive, b: &Primitive, tolerance: f64) -> Option<String> {
    match (a, b) {
        (Primitive::Stroke(a), Primitive::Stroke(b)) => compare_stroke(a, b, tolerance),
        (Primitive::Fill(a), Primitive::Fill(b)) => {
            if !same_color(&a.color, &b.color) {
                return Some(format!("fill color {} → {}", a.color, b.color));
            }
            compare_rings(&a.rings, &b.rings, tolerance)
        }
        (Primitive::Clipped(a), Primitive::Clipped(b)) => {
            if let Some(difference) = compare_rings(&a.rings, &b.rings, tolerance) {
                return Some(difference);
            }
            if a.strokes.len() != b.strokes.len() {
                return Some(format!("{} hatch line sets → {}", a.strokes.len(), b.strokes.len()));
            }
            a.strokes.iter().zip(&b.strokes).find_map(|(a, b)| compare_hatch_lines(a, b))
        }
        (Primitive::Dot(a), Primitive::Dot(b)) => {
            if !same_color(&a.color, &b.color) {
                return Some(format!("dot color {} → {}", a.color, b.color));
            }
            ((a.x - b.x).hypot(a.y - b.y) > 1e-6).then(|| "dot moved".into())
        }
        (Primitive::Text(a), Primitive::Text(b)) => compare_text(a, b),
        _ => Some(format!("{} → {}", kind(a), kind(b))),
    }
}

fn kind(primitive: &Primitive) -> &'static str {
    match primitive {
        Primitive::Stroke(_) => "stroke",
        Primitive::Fill(_) => "fill",
        Primitive::Clipped(_) => "clipped",
        Primitive::Dot(_) => "dot",
        Primitive::Text(_) => "text",
    }
}

fn layer_names(scene: &Scene, entity: &SceneEntity) -> Vec<String> {
    entity
        .layers
        .iter()
        .map(|id| scene.layers.iter().find(|layer| layer.id == *id).map_or(String::new(), |layer| layer.name.clone()))
        .collect()
}

/// 変換した図面の図形を、元の図形の id ごとにまとめる（ビューポートの枠を除く）
pub fn group<'a>(scene: &'a Scene, sources: &HashMap<String, String>) -> HashMap<String, Vec<&'a SceneEntity>> {
    let mut out: HashMap<String, Vec<&SceneEntity>> = HashMap::new();
    for entity in &scene.entities {
        if entity.source == "VIEWPORT" {
            continue;
        }
        let id = source_id(&entity.id, sources).unwrap_or_else(|| format!("?{}", entity.id));
        out.entry(id).or_default().push(entity);
    }
    out
}

/// 元の図面と、書き直した図面（DXF のレイアウト）の違い
pub fn differences(original: &Scene, again: &Scene, sources: &HashMap<String, String>) -> Vec<String> {
    let groups = group(again, sources);
    let mut out = Vec::new();
    let mut matched = 0;
    for entity in &original.entities {
        let primitives: Vec<&Primitive> = entity.primitives.iter().collect();
        let expected = normalized(&primitives);
        let id = &entity.id;
        let Some(converted) = groups.get(id) else {
            if !expected.is_empty() {
                out.push(format!("{id} ({}): missing", entity.type_name));
            }
            continue;
        };
        matched += 1;
        let all: Vec<&Primitive> = converted.iter().flat_map(|entity| entity.primitives.iter()).collect();
        let actual = normalized(&all);
        let mut bbox = zumen::geometry::empty_box();
        for part in converted {
            zumen::geometry::union_box(&mut bbox, &part.bbox);
        }
        let tolerance = TOLERANCE + size(&entity.bbox) * 1e-4;
        let b = &entity.bbox;
        if !expected.is_empty()
            && !(close(b.min_x, bbox.min_x, tolerance)
                && close(b.min_y, bbox.min_y, tolerance)
                && close(b.max_x, bbox.max_x, tolerance)
                && close(b.max_y, bbox.max_y, tolerance))
        {
            out.push(format!(
                "{id} ({}): box [{:.3}, {:.3}, {:.3}, {:.3}] → [{:.3}, {:.3}, {:.3}, {:.3}]",
                entity.type_name, b.min_x, b.min_y, b.max_x, b.max_y, bbox.min_x, bbox.min_y, bbox.max_x, bbox.max_y
            ));
        }
        if expected.len() != actual.len() {
            let kinds = |list: &[Primitive]| list.iter().map(kind).collect::<Vec<_>>().join(",");
            out.push(format!("{id} ({}): [{}] → [{}]", entity.type_name, kinds(&expected), kinds(&actual)));
        } else if let Some(difference) =
            expected.iter().zip(&actual).find_map(|(a, b)| compare_primitive(a, b, tolerance))
        {
            out.push(format!("{id} ({}): {difference}", entity.type_name));
        }
        let own: Vec<String> = layer_names(original, entity).iter().map(|name| dxf_name(name)).collect();
        for part in converted {
            for name in layer_names(again, part) {
                if name != "0" && !own.iter().any(|candidate| candidate == without_number(&name)) {
                    out.push(format!("{id} ({}): layer {name} not in {own:?}", entity.type_name));
                }
            }
        }
        // ビューポートにじかに映る図形（ブロック参照の中でない）の縮尺は、元の部分図・レイヤグループの縮尺と同じ
        let direct = converted[0].id.split_once(':').is_some_and(|(_, path)| !path.contains('>'));
        if let (Some(before), Some(after)) = (&entity.placement, &converted[0].placement)
            && direct
            && !close(before.scale_x, after.scale_x, 1e-9 * before.scale_x.abs().max(1e-12))
        {
            out.push(format!("{id}: scale {} → {}", before.scale_x, after.scale_x));
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
