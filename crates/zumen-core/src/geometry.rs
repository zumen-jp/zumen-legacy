//! 幾何の補助（`@zumenjs/core/geometry` と同じ計算を同じ順で行う）

use crate::js;
use crate::types::{BBox, Matrix};
use std::f64::consts::PI;

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// m · n（n を先に適用する）
pub fn multiply(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

pub fn invert(m: &Matrix) -> Matrix {
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-300 {
        return IDENTITY;
    }
    let a = m[3] / det;
    let b = -m[1] / det;
    let c = -m[2] / det;
    let d = m[0] / det;
    [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])]
}

/// 平行移動・回転（度、反時計回り）・拡大の順に合成した配置
pub fn placement(x: f64, y: f64, angle_deg: f64, scale_x: f64, scale_y: f64) -> Matrix {
    let r = (angle_deg * PI) / 180.0;
    let cos = js::cos(r);
    let sin = js::sin(r);
    [cos * scale_x, sin * scale_x, -sin * scale_y, cos * scale_y, x, y]
}

pub fn apply(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// 変換の向き（方向ベクトルだけを変換する）
pub fn apply_vector(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y, m[1] * x + m[3] * y)
}

/// 変換が平均して長さを何倍にするか
pub fn mean_scale(m: &Matrix) -> f64 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

/// 変換の回転角（度）。X 軸の向きから求める
pub fn rotation_deg(m: &Matrix) -> f64 {
    (js::atan2(m[1], m[0]) * 180.0) / PI
}

/// 変換が向きを反転するか（測地座標系の部分図など）
pub fn is_mirrored(m: &Matrix) -> bool {
    m[0] * m[3] - m[1] * m[2] < 0.0
}

pub fn empty_box() -> BBox {
    BBox { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY }
}

pub fn is_empty_box(b: &BBox) -> bool {
    !(b.min_x <= b.max_x && b.min_y <= b.max_y)
}

pub fn extend_box(b: &mut BBox, x: f64, y: f64) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    if x < b.min_x {
        b.min_x = x;
    }
    if y < b.min_y {
        b.min_y = y;
    }
    if x > b.max_x {
        b.max_x = x;
    }
    if y > b.max_y {
        b.max_y = y;
    }
}

pub fn extend_box_with_points(b: &mut BBox, points: &[f64]) {
    for [x, y] in points.as_chunks::<2>().0 {
        extend_box(b, *x, *y);
    }
}

pub fn union_box(b: &mut BBox, other: &BBox) {
    if is_empty_box(other) {
        return;
    }
    extend_box(b, other.min_x, other.min_y);
    extend_box(b, other.max_x, other.max_y);
}

/// 変換後の点列を返す
pub fn transform_points(m: &Matrix, points: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; points.len()];
    for ([x, y], target) in points.as_chunks::<2>().0.iter().zip(out.as_chunks_mut::<2>().0) {
        let (x, y) = (*x, *y);
        target[0] = m[0] * x + m[2] * y + m[4];
        target[1] = m[1] * x + m[3] * y + m[5];
    }
    out
}

/// 弦と弧のずれが tolerance 以下になる分割数。radius と tolerance は同じ座標（用紙の mm）で渡す。
/// 入力が NaN なら NaN を返し、点列は空になる（TypeScript と同じ）
pub fn arc_segments(radius: f64, sweep_radians: f64, tolerance: f64) -> f64 {
    let r = radius.abs();
    let sweep = sweep_radians.abs();
    if r.is_nan() || r <= 0.0 || sweep.is_nan() || sweep <= 0.0 {
        return 1.0;
    }
    let step = if r <= tolerance { PI / 2.0 } else { 2.0 * js::acos(js::max(-1.0, 1.0 - tolerance / r)) };
    let n = (sweep / js::max(step, 1e-4)).ceil();
    js::min(js::max(js::max(n, ((sweep / (2.0 * PI)) * 16.0).ceil()), 1.0), 4096.0)
}

/// 楕円弧を点列にする（元の座標）。角度は度、ccw が false なら時計回り。
/// 楕円の角度は媒介変数の角度（x = rx cos t、y = ry sin t を rotation だけ回す）
#[allow(clippy::too_many_arguments)]
pub fn ellipse_arc_points(
    cx: f64,
    cy: f64,
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    start_deg: f64,
    end_deg: f64,
    ccw: bool,
    segments: f64,
) -> Vec<f64> {
    let rot = (rotation_deg * PI) / 180.0;
    let cos_r = js::cos(rot);
    let sin_r = js::sin(rot);
    let start = (start_deg * PI) / 180.0;
    let sweep = sweep_radians(start_deg, end_deg, ccw);
    let mut out = Vec::new();
    let mut i = 0.0;
    while i <= segments {
        let t = start + (sweep * i) / segments;
        let x = rx * js::cos(t);
        let y = ry * js::sin(t);
        out.push(cx + x * cos_r - y * sin_r);
        out.push(cy + x * sin_r + y * cos_r);
        i += 1.0;
    }
    out
}

/// 始角から終角まで、指定の向きに回る角度（ラジアン、符号付き）。同じ角度は全周とみなす
pub fn sweep_radians(start_deg: f64, end_deg: f64, ccw: bool) -> f64 {
    let raw = if ccw { end_deg - start_deg } else { start_deg - end_deg };
    let mut d = ((raw % 360.0) + 360.0) % 360.0;
    if d < 1e-9 {
        d = 360.0;
    }
    let sweep = (d * PI) / 180.0;
    if ccw { sweep } else { -sweep }
}

/// 3 次ベジェ曲線（制御点 3n+1 個）を点列にする。余りの点は折線としてつなぐ
pub fn bezier_points(points: &[f64], steps_per_segment: usize) -> Vec<f64> {
    let count = points.len() / 2;
    let mut out = Vec::new();
    if count == 0 {
        return out;
    }
    out.push(points[0]);
    out.push(points[1]);
    let mut i = 0;
    while i + 3 < count {
        let p = &points[i * 2..i * 2 + 8];
        for s in 1..=steps_per_segment {
            let t = s as f64 / steps_per_segment as f64;
            let u = 1.0 - t;
            let b0 = u * u * u;
            let b1 = 3.0 * t * u * u;
            let b2 = 3.0 * t * t * u;
            let b3 = t * t * t;
            out.push(b0 * p[0] + b1 * p[2] + b2 * p[4] + b3 * p[6]);
            out.push(b0 * p[1] + b1 * p[3] + b2 * p[5] + b3 * p[7]);
        }
        i += 3;
    }
    for j in i + 1..count {
        out.push(points[j * 2]);
        out.push(points[j * 2 + 1]);
    }
    out
}

/// クロソイド（曲率が曲線長に比例する曲線）を点列にする。
/// 基点で接線の向き angle_deg、パラメータ a（R・L = A²）、基点からの曲線長 s0 から s1 まで。
/// ccw なら接線の左側へ曲がる
#[allow(clippy::too_many_arguments)]
pub fn clothoid_points(
    x: f64,
    y: f64,
    a: f64,
    ccw: bool,
    angle_deg: f64,
    s0: f64,
    s1: f64,
    segments: usize,
) -> Vec<f64> {
    let theta = (angle_deg * PI) / 180.0;
    let cos = js::cos(theta);
    let sin = js::sin(theta);
    let side = if ccw { 1.0 } else { -1.0 };
    let a2 = 2.0 * a * a;
    // 接線角 s²/(2A²) をシンプソン則で積分して、基点からの局所座標を求める
    let local = |s: f64| -> (f64, f64) {
        let n = js::max(8.0, (s.abs() / js::max(a, 1e-9)).ceil() * 16.0);
        let h = s / n;
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut k = 0.0;
        while k <= n {
            let t = k * h;
            let w = if k == 0.0 || k == n {
                1.0
            } else if k % 2.0 == 1.0 {
                4.0
            } else {
                2.0
            };
            let phi = (t * t) / a2;
            sx += w * js::cos(phi);
            sy += w * js::sin(phi);
            k += 1.0;
        }
        ((sx * h) / 3.0, (side * sy * h) / 3.0)
    };
    let mut out = Vec::with_capacity((segments + 1) * 2);
    for i in 0..=segments {
        let s = s0 + ((s1 - s0) * i as f64) / segments as f64;
        let (lx, ly) = local(s);
        out.push(x + lx * cos - ly * sin);
        out.push(y + lx * sin + ly * cos);
    }
    out
}

/// 点 (px, py) から線分までの距離の 2 乗
pub fn distance_to_segment_squared(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length_squared = dx * dx + dy * dy;
    let mut t = if length_squared > 0.0 { ((px - x1) * dx + (py - y1) * dy) / length_squared } else { 0.0 };
    t = js::max(0.0, js::min(1.0, t));
    let qx = x1 + t * dx - px;
    let qy = y1 + t * dy - py;
    qx * qx + qy * qy
}

/// 偶奇規則で点が多角形の内側にあるか
pub fn inside_rings(px: f64, py: f64, rings: &[Vec<f64>]) -> bool {
    let mut inside = false;
    for ring in rings {
        let n = ring.len() / 2;
        if n == 0 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (ring[i * 2], ring[i * 2 + 1]);
            let (xj, yj) = (ring[j * 2], ring[j * 2 + 1]);
            if (yi > py) != (yj > py) && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}
