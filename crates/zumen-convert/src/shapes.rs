//! SXF に書くための形の計算（AutoCAD の図面のモデルから SXF へ）: 弧の膨らみ、B スプラインのベジェ曲線への分解、
//! 塗りの輪郭の入れ子、文字範囲の変換の分解

use zumen_core::Matrix;
use zumen_core::geometry::{apply, apply_vector};

/// 変換が回転と一様な拡大（裏返しを含む）だけか
pub(crate) fn is_similar(m: &Matrix) -> bool {
    let k = m[0].hypot(m[1]).max(m[2].hypot(m[3]));
    if !(k.is_finite() && k > 0.0) {
        return false;
    }
    let eps = 1e-9 * k;
    let rotation = (m[0] - m[3]).abs() < eps && (m[1] + m[2]).abs() < eps;
    let reflection = (m[0] + m[3]).abs() < eps && (m[1] - m[2]).abs() < eps;
    rotation || reflection
}

/// 変換が裏返すか（行列式が負）
pub(crate) fn mirrors(m: &Matrix) -> bool {
    m[0] * m[3] - m[1] * m[2] < 0.0
}

/// 長さの倍率
pub(crate) fn length_scale(m: &Matrix) -> f64 {
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
}

/// 配置の変換（平行移動・回転・X と Y の尺度）に分ける。SXF の配置（placement）と同じ T · R · S の順。
/// 斜めにゆがむ変換は分けられないので None
pub(crate) fn decompose(m: &Matrix) -> Option<(f64, f64, f64, f64, f64)> {
    let rx = m[0].hypot(m[1]);
    if !(rx.is_finite() && rx > 0.0) {
        return None;
    }
    let angle = m[1].atan2(m[0]);
    let (sin, cos) = angle.sin_cos();
    // R(−angle) · 2 列目 = (0, ry)
    let shear = cos * m[2] + sin * m[3];
    let ry = -sin * m[2] + cos * m[3];
    if shear.abs() > 1e-9 * rx.max(ry.abs()) {
        return None;
    }
    Some((m[4], m[5], normalize(angle.to_degrees()), rx, ry))
}

/// 角度を 0 以上 360 未満にする
pub(crate) fn normalize(degrees: f64) -> f64 {
    let value = degrees.rem_euclid(360.0);
    if (value - 360.0).abs() < 1e-12 || value.abs() < 1e-12 { 0.0 } else { value }
}

/// 弧の膨らみ（tan(中心角 / 4)）の区間を、中心・半径・始角・終角（度）・反時計回りか にする
pub(crate) fn bulge_arc(p1: [f64; 2], p2: [f64; 2], bulge: f64) -> Option<([f64; 2], f64, f64, f64, bool)> {
    let (dx, dy) = (p2[0] - p1[0], p2[1] - p1[1]);
    let chord = dx.hypot(dy);
    if !(chord > 0.0 && bulge.is_finite() && bulge != 0.0) {
        return None;
    }
    let theta = 4.0 * bulge.atan();
    let half = chord / 2.0;
    let (ux, uy) = (dx / chord, dy / chord);
    // 中心は弦の中点から左（進む向きの 90° 反時計回り）へ half · cot(θ/2)
    let offset = half / (theta / 2.0).tan();
    let center = [p1[0] + dx / 2.0 - uy * offset, p1[1] + dy / 2.0 + ux * offset];
    let radius = (half / (theta / 2.0).sin()).abs();
    let start = (p1[1] - center[1]).atan2(p1[0] - center[0]).to_degrees();
    let end = (p2[1] - center[1]).atan2(p2[0] - center[0]).to_degrees();
    Some((center, radius, normalize(start), normalize(end), bulge > 0.0))
}

/// 円弧を変換で写す。中心・半径・始角・終角（度）・反時計回りか。変換は回転と一様な拡大（裏返しを含む）
pub(crate) fn map_arc(
    m: &Matrix,
    center: [f64; 2],
    radius: f64,
    start: f64,
    end: f64,
    ccw: bool,
) -> ([f64; 2], f64, f64, f64, bool) {
    let (cx, cy) = apply(m, center[0], center[1]);
    let angle_of = |degrees: f64| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let (x, y) = apply_vector(m, cos, sin);
        normalize(y.atan2(x).to_degrees())
    };
    let flip = mirrors(m);
    ([cx, cy], radius * length_scale(m), angle_of(start), angle_of(end), ccw != flip)
}

/// 節点の両端が次数 + 1 重（端を通る B スプライン）か
fn clamped(knots: &[f64], degree: usize) -> bool {
    knots.len() >= 2 * (degree + 1)
        && knots[..=degree].iter().all(|knot| *knot == knots[0])
        && knots[knots.len() - degree - 1..].iter().all(|knot| *knot == knots[knots.len() - 1])
}

/// 有理でない B スプライン（次数 1〜3、端を通る節点）を、3 次ベジェ曲線の制御点（3n + 1 個）にする。
/// 節点を次数の重複度まで挿入（Boehm の方法）してベジェ曲線に分け、2 次は 3 次に上げる。次数 1 は折線なので None
pub(crate) fn bezier(degree: usize, knots: &[f64], control: &[[f64; 2]]) -> Option<Vec<[f64; 2]>> {
    if !(2..=3).contains(&degree) || control.len() < degree + 1 || knots.len() != control.len() + degree + 1 {
        return None;
    }
    if !clamped(knots, degree) || knots.windows(2).any(|pair| pair[1] < pair[0] || pair[1].is_nan()) {
        return None;
    }
    let mut knots = knots.to_vec();
    let mut points = control.to_vec();
    let (low, high) = (knots[degree], knots[knots.len() - degree - 1]);
    let mut distinct: Vec<f64> = knots.iter().copied().filter(|knot| *knot > low && *knot < high).collect();
    distinct.dedup();
    for u in distinct {
        let multiplicity = knots.iter().filter(|knot| **knot == u).count();
        for _ in multiplicity..degree {
            // u を含む区間 k（knots[k] ≤ u < knots[k + 1]）
            let k = knots.iter().rposition(|knot| *knot <= u)?;
            let mut inserted = Vec::with_capacity(points.len() + 1);
            for i in 0..=points.len() {
                if i + degree <= k {
                    inserted.push(points[i]);
                } else if i > k {
                    inserted.push(points[i - 1]);
                } else {
                    let a = (u - knots[i]) / (knots[i + degree] - knots[i]);
                    let (p, q) = (points[i - 1], points[i]);
                    inserted.push([(1.0 - a) * p[0] + a * q[0], (1.0 - a) * p[1] + a * q[1]]);
                }
            }
            points = inserted;
            knots.insert(k + 1, u);
        }
    }
    if degree == 3 {
        return Some(points);
    }
    // 2 次のベジェ曲線（P0, P1, P2）を 3 次（P0, (P0 + 2P1)/3, (2P1 + P2)/3, P2）にする
    let mut out = vec![points[0]];
    for piece in points.windows(3).step_by(2) {
        let [p0, p1, p2] = [piece[0], piece[1], piece[2]];
        out.push([(p0[0] + 2.0 * p1[0]) / 3.0, (p0[1] + 2.0 * p1[1]) / 3.0]);
        out.push([(2.0 * p1[0] + p2[0]) / 3.0, (2.0 * p1[1] + p2[1]) / 3.0]);
        out.push(p2);
    }
    Some(out)
}

/// 点が輪郭（偶奇規則）の内側か
fn inside(ring: &[f64], x: f64, y: f64) -> bool {
    let n = ring.len() / 2;
    let mut result = false;
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (xi, yi, xj, yj) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            result = !result;
        }
        j = i;
    }
    result
}

/// 偶奇規則で塗る輪郭を、外形とその中抜きの組に分ける。すべての頂点がほかの輪郭の内側にある輪郭を、その輪郭の
/// 入れ子とする（重なるだけの輪郭は別の外形）。入れ子の深さが偶数なら外形、奇数なら
/// すぐ外の外形の中抜き（SXF のハッチングの領域は 1 つの外形と中抜き。附属書 SFC 編のハッチングの領域）
pub(crate) fn regions(rings: &[Vec<f64>]) -> Vec<(usize, Vec<usize>)> {
    let containers: Vec<Vec<usize>> = rings
        .iter()
        .enumerate()
        .map(|(i, ring)| {
            (0..rings.len())
                .filter(|j| {
                    *j != i
                        && ring.len() >= 2
                        && ring.as_chunks::<2>().0.iter().all(|[x, y]| inside(&rings[*j], *x, *y))
                })
                .collect()
        })
        .collect();
    let depth = |i: usize| containers[i].len();
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..rings.len() {
        if depth(i) % 2 == 0 {
            out.push((i, Vec::new()));
        }
    }
    for (i, outside) in containers.iter().enumerate() {
        if depth(i) % 2 == 1 {
            // すぐ外の外形（入れ子が 1 つ浅い、自分を囲む輪郭）
            if let Some(parent) = outside.iter().copied().find(|j| depth(*j) + 1 == depth(i))
                && let Some(region) = out.iter_mut().find(|(outer, _)| *outer == parent)
            {
                region.1.push(i);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_bulge_into_an_arc() {
        // 半円（膨らみ 1、反時計回り）
        let (center, radius, start, end, ccw) = bulge_arc([1.0, 0.0], [-1.0, 0.0], 1.0).unwrap();
        assert!((center[0]).abs() < 1e-12 && center[1].abs() < 1e-12);
        assert!((radius - 1.0).abs() < 1e-12);
        assert_eq!((start, end, ccw), (0.0, 180.0, true));
        // 時計回りの 90°
        let (center, _, start, end, ccw) =
            bulge_arc([0.0, 1.0], [1.0, 0.0], -(std::f64::consts::PI / 8.0).tan()).unwrap();
        assert!(center[0].abs() < 1e-12 && center[1].abs() < 1e-12, "{center:?}");
        assert_eq!((start.round(), end.round(), ccw), (90.0, 0.0, false));
    }

    #[test]
    fn splits_b_splines_into_bezier_curves() {
        let control = [[0.0, 0.0], [1.0, 2.0], [2.0, 2.0], [3.0, 0.0], [4.0, 1.0]];
        let knots = [0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0];
        let points = bezier(3, &knots, &control).unwrap();
        assert_eq!(points.len(), 7);
        assert_eq!(points[0], [0.0, 0.0]);
        assert_eq!(points[6], [4.0, 1.0]);
        // ベジェ曲線の境は、もとの B スプラインの u = 0.5 の点
        let [x, y] = points[3];
        assert!((x - 2.0).abs() < 1e-12 && (y - 1.5).abs() < 1e-12, "{x} {y}");
        let quadratic = bezier(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], &[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]]).unwrap();
        assert_eq!(quadratic.len(), 4);
    }

    #[test]
    fn nests_rings_into_regions() {
        let square = |size: f64| vec![-size, -size, size, -size, size, size, -size, size];
        let far = vec![10.0, 10.0, 11.0, 10.0, 11.0, 11.0];
        let regions = regions(&[square(3.0), square(2.0), square(1.0), far]);
        assert_eq!(regions, vec![(0, vec![1]), (2, Vec::new()), (3, Vec::new())]);
    }
}
