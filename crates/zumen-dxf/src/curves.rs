//! OCS・円弧・膨らみ・NURBS・当てはめ点・切り抜きなどの曲線の補助（`@zumenjs/dxf` の curves.ts と同じ計算を同じ順で行う）

use crate::model::{Vec2, Vec3};
use zumen_core::geometry::arc_segments;
use zumen_core::{BBox, Matrix, js};

fn cross(a: &Vec3, b: &Vec3) -> Vec3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: &Vec3) -> Vec3 {
    let length = js::hypot(&[v[0], v[1], v[2]]);
    if length > 0.0 { [v[0] / length, v[1] / length, v[2] / length] } else { [0.0, 0.0, 1.0] }
}

/// 任意軸アルゴリズム（DXF Reference「About Arbitrary Axis Algorithm」）。押し出し方向から OCS の X・Y・Z 軸を求める。
/// 押し出し方向が WCS の Z なら None（OCS と WCS が同じ）
pub fn ocs_axes(extrusion: Option<&Vec3>) -> Option<[Vec3; 3]> {
    let az = normalize(extrusion?);
    if az[0].abs() < 1e-12 && az[1].abs() < 1e-12 && az[2] > 0.0 {
        return None;
    }
    let ax = normalize(&if az[0].abs() < 1.0 / 64.0 && az[1].abs() < 1.0 / 64.0 {
        cross(&[0.0, 1.0, 0.0], &az)
    } else {
        cross(&[0.0, 0.0, 1.0], &az)
    });
    let ay = normalize(&cross(&az, &ax));
    Some([ax, ay, az])
}

/// OCS の (x, y) と標高 elevation を、WCS を上から見た (x, y) にする変換
pub fn ocs_matrix(extrusion: Option<&Vec3>, elevation: f64) -> Matrix {
    let Some([ax, ay, az]) = ocs_axes(extrusion) else {
        return [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    };
    [ax[0], ax[1], ay[0], ay[1], az[0] * elevation, az[1] * elevation]
}

/// 円弧を点列にする（角度は度、反時計回り）。始点を含む
pub fn arc_points(cx: f64, cy: f64, radius: f64, start_deg: f64, end_deg: f64, tolerance: f64) -> Vec<f64> {
    let mut sweep = end_deg - start_deg;
    sweep = ((sweep % 360.0) + 360.0) % 360.0;
    if sweep < 1e-9 {
        sweep = 360.0;
    }
    let start = (start_deg * std::f64::consts::PI) / 180.0;
    let radians = (sweep * std::f64::consts::PI) / 180.0;
    let n = arc_segments(radius, radians, tolerance);
    let mut out = Vec::new();
    let mut i = 0.0;
    while i <= n {
        let a = start + (radians * i) / n;
        out.push(cx + radius * js::cos(a));
        out.push(cy + radius * js::sin(a));
        i += 1.0;
    }
    out
}

/// 膨らみ（bulge = tan(中心角 / 4)）つきの頂点列を点列にする（DXF Reference「LWPOLYLINE」の 42）。
/// 正の膨らみは反時計回りの弧
pub fn bulge_points(points: &[Vec3], bulges: Option<&[f64]>, closed: bool, tolerance: &dyn Fn(f64) -> f64) -> Vec<f64> {
    let n = points.len();
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    out.push(points[0][0]);
    out.push(points[0][1]);
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        let a = &points[i];
        let b = &points[(i + 1) % n];
        let (x1, y1, x2, y2) = (a[0], a[1], b[0], b[1]);
        let bulge = bulges.and_then(|bulges| bulges.get(i)).copied().unwrap_or(0.0);
        if bulge.abs() < 1e-12 || (x1 == x2 && y1 == y2) {
            out.push(x2);
            out.push(y2);
            continue;
        }
        let theta = 4.0 * js::atan(bulge);
        let chord = js::hypot2(x2 - x1, y2 - y1);
        let radius = chord / (2.0 * js::sin(theta / 2.0).abs());
        // 弦の中点から、弦に垂直に中心へ
        let k = (1.0 - bulge * bulge) / (4.0 * bulge);
        let cx = (x1 + x2) / 2.0 - (y2 - y1) * k;
        let cy = (y1 + y2) / 2.0 + (x2 - x1) * k;
        let a1 = js::atan2(y1 - cy, x1 - cx);
        let segments = arc_segments(radius, theta.abs(), tolerance(radius));
        let mut s = 1.0;
        while s < segments {
            let angle = a1 + (theta * s) / segments;
            out.push(cx + radius * js::cos(angle));
            out.push(cy + radius * js::sin(angle));
            s += 1.0;
        }
        out.push(x2);
        out.push(y2);
    }
    out
}

/// 端を固定した一様なノット列（ノットがない・数が合わないスプライン）
fn clamped_knots(count: usize, degree: usize) -> Vec<f64> {
    let spans = count as f64 - degree as f64;
    (0..count + degree + 1)
        .map(|i| {
            if i <= degree {
                0.0
            } else if i >= count {
                spans
            } else {
                i as f64 - degree as f64
            }
        })
        .collect()
}

fn flat(control: &[&[f64]]) -> Vec<f64> {
    control.iter().flat_map(|point| [point[0], point[1]]).collect()
}

/// NURBS を de Boor の算法で点列にする。control は [x, y, ...] の並び
pub fn nurbs_points(
    degree: f64,
    control: &[&[f64]],
    knot_values: &[f64],
    weight_values: &[f64],
    steps: f64,
) -> Vec<f64> {
    let n = control.len();
    let p = js::max(1.0, js::min(degree.trunc(), n as f64 - 1.0));
    if n < 2 {
        return flat(control);
    }
    // p は 1 から n - 1 の整数（degree が NaN なら NaN で、点列は作らない）
    if p.is_nan() {
        return flat(control);
    }
    let p = p as usize;
    let clamped;
    let knots: &[f64] = if knot_values.len() == n + p + 1 {
        knot_values
    } else {
        clamped = clamped_knots(n, p);
        &clamped
    };
    let weights = (weight_values.len() == n).then_some(weight_values);
    let lo = knots[p];
    let hi = knots[n];
    if hi.is_nan() || lo.is_nan() || hi <= lo {
        return flat(control);
    }
    let mut out = Vec::new();
    let mut d = vec![[0.0f64; 3]; p + 1];
    let mut s = 0.0;
    while s <= steps {
        let u = if s == steps { hi } else { lo + ((hi - lo) * s) / steps };
        let mut k = p;
        while k < n - 1 && u >= knots[k + 1] {
            k += 1;
        }
        for (j, target) in d.iter_mut().enumerate() {
            let point = control[k - p + j];
            let w = match weights {
                Some(weights) => {
                    let w = weights[k - p + j];
                    if w == 0.0 || w.is_nan() { 1.0 } else { w }
                }
                None => 1.0,
            };
            *target = [point[0] * w, point[1] * w, w];
        }
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = k - p + j;
                let left = knots[i];
                let denominator = knots[i + p - r + 1] - left;
                let alpha = if denominator == 0.0 { 0.0 } else { (u - left) / denominator };
                let a = d[j - 1];
                let b = &mut d[j];
                b[0] = (1.0 - alpha) * a[0] + alpha * b[0];
                b[1] = (1.0 - alpha) * a[1] + alpha * b[1];
                b[2] = (1.0 - alpha) * a[2] + alpha * b[2];
            }
        }
        let result = d[p];
        let w = if result[2] == 0.0 || result[2].is_nan() { 1.0 } else { result[2] };
        out.push(result[0] / w);
        out.push(result[1] / w);
        s += 1.0;
    }
    out
}

/// 当てはめ点だけのスプラインを、点を通る 3 次の曲線（Catmull-Rom）で近似する。
/// start_tangent・end_tangent（向き）を渡すと、両端をその向きで出入りさせる
pub fn through_points(
    points: &[&[f64]],
    steps_per_span: usize,
    start_tangent: Option<&[f64]>,
    end_tangent: Option<&[f64]>,
) -> Vec<f64> {
    let n = points.len();
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    // 端の外に、Catmull-Rom の端の接線がその向き（長さは隣の弦）になる仮の点を置く
    let phantom = |tangent: Option<&[f64]>, end: usize, next: usize, sign: f64| -> [f64; 2] {
        let p = points[end];
        let length = js::hypot2(
            tangent.and_then(|t| t.first()).copied().unwrap_or(0.0),
            tangent.and_then(|t| t.get(1)).copied().unwrap_or(0.0),
        );
        let Some(tangent) = tangent else {
            return [p[0], p[1]];
        };
        if length.is_nan() || length <= 0.0 || n < 2 {
            return [p[0], p[1]];
        }
        let q = points[next];
        let chord = js::hypot2(q[0] - p[0], q[1] - p[1]);
        let scale = (2.0 * chord * sign) / length;
        [q[0] - tangent[0] * scale, q[1] - tangent[1] * scale]
    };
    let before = phantom(start_tangent, 0, 1, 1.0);
    let after = phantom(end_tangent, n - 1, n.saturating_sub(2), -1.0);
    let at = |i: isize| -> [f64; 2] {
        if i < 0 {
            before
        } else if i as usize >= n {
            after
        } else {
            let point = points[i as usize];
            [point[0], point[1]]
        }
    };
    let first = at(0);
    out.push(first[0]);
    out.push(first[1]);
    for i in 0..n as isize - 1 {
        let p0 = at(i - 1);
        let p1 = at(i);
        let p2 = at(i + 1);
        let p3 = at(i + 2);
        for s in 1..=steps_per_span {
            let t = s as f64 / steps_per_span as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let f = |c: usize| {
                0.5 * (2.0 * p1[c]
                    + (-p0[c] + p2[c]) * t
                    + (2.0 * p0[c] - 5.0 * p1[c] + 4.0 * p2[c] - p3[c]) * t2
                    + (-p0[c] + 3.0 * p1[c] - 3.0 * p2[c] + p3[c]) * t3)
            };
            out.push(f(0));
            out.push(f(1));
        }
    }
    out
}

/// 折線（[x, y, ...]）から、切れ目（始点と終点の組）の間を除いた部分ごとの点列にする。
/// 切れ目の点は折線の最も近い位置に寄せる（MULTILEADER の寸法の切断）
pub fn cut_polyline(points: &[f64], gaps: &[[f64; 4]]) -> Vec<Vec<f64>> {
    let n = points.len() / 2;
    if gaps.is_empty() || n < 2 {
        return vec![points.to_vec()];
    }
    let x = |i: usize| points[i * 2];
    let y = |i: usize| points[i * 2 + 1];
    let mut lengths = vec![0.0];
    for i in 1..n {
        lengths.push(lengths[i - 1] + js::hypot2(x(i) - x(i - 1), y(i) - y(i - 1)));
    }
    let total = lengths[n - 1];
    // 点に最も近い折線の位置（始点からの長さ）
    let project = |px: f64, py: f64| -> f64 {
        let mut best = f64::INFINITY;
        let mut at = 0.0;
        for (i, length) in lengths.iter().enumerate().take(n - 1) {
            let dx = x(i + 1) - x(i);
            let dy = y(i + 1) - y(i);
            let length2 = dx * dx + dy * dy;
            let t = if length2 > 0.0 {
                js::max(0.0, js::min(1.0, ((px - x(i)) * dx + (py - y(i)) * dy) / length2))
            } else {
                0.0
            };
            let distance = js::hypot2(x(i) + dx * t - px, y(i) + dy * t - py);
            if distance < best {
                best = distance;
                at = length + length2.sqrt() * t;
            }
        }
        at
    };
    let point_at = |s: f64| -> [f64; 2] {
        let mut i = 0;
        while i + 2 < n && lengths[i + 1] < s {
            i += 1;
        }
        let length = lengths[i + 1] - lengths[i];
        let t = if length > 0.0 { (s - lengths[i]) / length } else { 0.0 };
        [x(i) + (x(i + 1) - x(i)) * t, y(i) + (y(i + 1) - y(i)) * t]
    };
    let mut intervals: Vec<(f64, f64)> = gaps
        .iter()
        .map(|gap| {
            let a = project(gap[0], gap[1]);
            let b = project(gap[2], gap[3]);
            (js::min(a, b), js::max(a, b))
        })
        .collect();
    js::sort_by(&mut intervals, |p, q| p.0 - q.0);
    let mut out = Vec::new();
    let mut piece = |s0: f64, s1: f64| {
        let span = s1 - s0;
        if span.is_nan() || span <= 1e-9 {
            return;
        }
        let mut part = point_at(s0).to_vec();
        for (i, length) in lengths.iter().enumerate().take(n - 1).skip(1) {
            if *length > s0 && *length < s1 {
                part.push(x(i));
                part.push(y(i));
            }
        }
        part.extend(point_at(s1));
        out.push(part);
    };
    let mut cursor = 0.0;
    for (a, b) in intervals {
        piece(cursor, a);
        cursor = js::max(cursor, b);
    }
    piece(cursor, total);
    out
}

/// 楕円（WCS の中心・長軸・短軸の向き）を、媒介変数 start から end まで点列にする（ラジアン、反時計回り）
pub fn ellipse_points(center: &Vec3, major: &Vec3, minor: &Vec3, start: f64, end: f64, tolerance: f64) -> Vec<f64> {
    let tau = std::f64::consts::PI * 2.0;
    let mut sweep = end - start;
    sweep = ((sweep % tau) + tau) % tau;
    if sweep < 1e-9 {
        sweep = tau;
    }
    let radius = js::max(js::hypot(&[major[0], major[1], major[2]]), js::hypot(&[minor[0], minor[1], minor[2]]));
    let n = arc_segments(radius, sweep, tolerance);
    let mut out = Vec::new();
    let mut i = 0.0;
    while i <= n {
        let t = start + (sweep * i) / n;
        let c = js::cos(t);
        let s = js::sin(t);
        out.push(center[0] + major[0] * c + minor[0] * s);
        out.push(center[1] + major[1] * c + minor[1] * s);
        i += 1.0;
    }
    out
}

/// 楕円の短軸（押し出し方向 × 長軸、長さは比 × 長軸）
pub fn minor_axis(major: &Vec3, ratio: f64, extrusion: Option<&Vec3>) -> Vec3 {
    let normal = normalize(extrusion.unwrap_or(&[0.0, 0.0, 1.0]));
    let axis = cross(&normal, major);
    [axis[0] * ratio, axis[1] * ratio, axis[2] * ratio]
}

/// 線分を矩形で切る（Liang–Barsky）。外なら None
fn clip_segment(x1: f64, y1: f64, x2: f64, y2: f64, b: &BBox) -> Option<[f64; 4]> {
    let mut t0 = 0.0;
    let mut t1 = 1.0;
    let dx = x2 - x1;
    let dy = y2 - y1;
    for (p, q) in [(-dx, x1 - b.min_x), (dx, b.max_x - x1), (-dy, y1 - b.min_y), (dy, b.max_y - y1)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            if r > t1 {
                return None;
            }
            if r > t0 {
                t0 = r;
            }
        } else {
            if r < t0 {
                return None;
            }
            if r < t1 {
                t1 = r;
            }
        }
    }
    Some([x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy])
}

/// 折線を矩形で切り、内側の部分ごとの点列にする
pub fn clip_polyline(points: &[f64], closed: bool, b: &BBox) -> Vec<Vec<f64>> {
    let mut out = Vec::new();
    let mut current: Vec<f64> = Vec::new();
    let count = points.len() / 2;
    let segments = if closed && count > 2 { count } else { count.saturating_sub(1) };
    for i in 0..segments {
        let j = (i + 1) % count;
        let Some([ax, ay, bx, by]) =
            clip_segment(points[i * 2], points[i * 2 + 1], points[j * 2], points[j * 2 + 1], b)
        else {
            if current.len() >= 4 {
                out.push(std::mem::take(&mut current));
            }
            current.clear();
            continue;
        };
        let last = current.len().checked_sub(2).map(|at| (current[at], current[at + 1]));
        if current.is_empty() || last != Some((ax, ay)) {
            if current.len() >= 4 {
                out.push(std::mem::take(&mut current));
            }
            current = vec![ax, ay];
        }
        current.push(bx);
        current.push(by);
    }
    if current.len() >= 4 {
        out.push(current);
    }
    out
}

/// 多角形を矩形で切る（Sutherland–Hodgman）。切り抜く側が凸なので、塗りの多角形はどの形でもよい
pub fn clip_polygon(ring: &[f64], b: &BBox) -> Vec<f64> {
    let mut input: Vec<f64> = ring.to_vec();
    type Inside = fn(f64, f64, &BBox) -> bool;
    type Intersect = fn(f64, f64, f64, f64, &BBox) -> Vec2;
    let edges: [(Inside, Intersect); 4] = [
        (|x, _, b| x >= b.min_x, |ax, ay, bx, by, b| [b.min_x, ay + ((by - ay) * (b.min_x - ax)) / (bx - ax)]),
        (|x, _, b| x <= b.max_x, |ax, ay, bx, by, b| [b.max_x, ay + ((by - ay) * (b.max_x - ax)) / (bx - ax)]),
        (|_, y, b| y >= b.min_y, |ax, ay, bx, by, b| [ax + ((bx - ax) * (b.min_y - ay)) / (by - ay), b.min_y]),
        (|_, y, b| y <= b.max_y, |ax, ay, bx, by, b| [ax + ((bx - ax) * (b.max_y - ay)) / (by - ay), b.max_y]),
    ];
    for (inside, intersect) in edges {
        let mut output = Vec::new();
        let n = input.len() / 2;
        for i in 0..n {
            let cx = input[i * 2];
            let cy = input[i * 2 + 1];
            let previous = (i + n - 1) % n;
            let px = input[previous * 2];
            let py = input[previous * 2 + 1];
            if inside(cx, cy, b) {
                if !inside(px, py, b) {
                    output.extend(intersect(px, py, cx, cy, b));
                }
                output.push(cx);
                output.push(cy);
            } else if inside(px, py, b) {
                output.extend(intersect(px, py, cx, cy, b));
            }
        }
        input = output;
        if input.len() < 6 {
            return Vec::new();
        }
    }
    input
}
