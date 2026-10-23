//! 足すハッチングを JWW の図形にする（docs/decisions/0025）。JWW にはハッチングの図形がないので、Jw_cad の
//! 多角形のソリッドとハッチと同じく、塗りは台形のソリッド（CDataSolid）に分け、模様は領域で切った線（CDataSen）と
//! 点（CDataTen。模様の長さ 0 の線）にする。座標はすべて用紙の mm。

use std::collections::HashMap;
use zumen_core::edit::hatch::PatternLine;
use zumen_core::geometry::inside_rings;

/// 交わりを調べる辺の数の上限（zumen-convert の DXF → JWW と同じ）
const MAX_CROSSING_EDGES: usize = 2_000;

/// 塗りの辺の数の上限
const MAX_FILL_EDGES: usize = 100_000;

/// 1 つのハッチングから作る線と点の数の上限（SXF の読み取りのハッチング線の上限と同じ）
pub(super) const MAX_PATTERN_ENTITIES: usize = 20_000;

/// 塗りの辺（下端から上端へ）
#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Edge {
    fn x_at(&self, y: f64) -> f64 {
        if y <= self.y0 {
            return self.x0;
        }
        if y >= self.y1 {
            return self.x1;
        }
        self.x0 + (self.x1 - self.x0) * (y - self.y0) / (self.y1 - self.y0)
    }
}

/// 偶奇規則で塗る輪郭の組を、横の帯ごとの台形（JWW のソリッドの 4 点: 左下 → 右下 → 右上 → 左上）に分ける
/// （zumen-convert の DXF → JWW の塗りと同じ分け方）。帯の境は頂点と辺どうしの交点の高さで、上下の帯で同じ辺の組の
/// 台形は 1 つにつなぐ。辺が多すぎて分けられなければ None
pub(super) fn trapezoids(rings: &[Vec<f64>]) -> Option<Vec<[f64; 8]>> {
    let mut edges: Vec<Edge> = Vec::new();
    for ring in rings {
        let n = ring.len() / 2;
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (ax, ay, bx, by) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
            if ay == by {
                continue;
            }
            edges.push(if ay < by {
                Edge { x0: ax, y0: ay, x1: bx, y1: by }
            } else {
                Edge { x0: bx, y0: by, x1: ax, y1: ay }
            });
        }
    }
    if edges.len() > MAX_CROSSING_EDGES.min(MAX_FILL_EDGES) {
        return None;
    }
    let mut ys: Vec<f64> = edges.iter().flat_map(|edge| [edge.y0, edge.y1]).collect();
    for (i, a) in edges.iter().enumerate() {
        for b in &edges[i + 1..] {
            if a.y1 <= b.y0 || b.y1 <= a.y0 {
                continue;
            }
            let (dax, day, dbx, dby) = (a.x1 - a.x0, a.y1 - a.y0, b.x1 - b.x0, b.y1 - b.y0);
            let denominator = dax * dby - day * dbx;
            if denominator == 0.0 {
                continue;
            }
            let t = ((b.x0 - a.x0) * dby - (b.y0 - a.y0) * dbx) / denominator;
            let u = ((b.x0 - a.x0) * day - (b.y0 - a.y0) * dax) / denominator;
            if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                ys.push(a.y0 + day * t);
            }
        }
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|a, b| edges[*a].y0.total_cmp(&edges[*b].y0));
    let mut next = 0;
    let mut active: Vec<usize> = Vec::new();
    let mut out: Vec<[f64; 8]> = Vec::new();
    // 続けられる台形: (左の辺, 右の辺) → out の添字
    let mut open: HashMap<(usize, usize), usize> = HashMap::new();
    for pair in ys.windows(2) {
        let (ya, yb) = (pair[0], pair[1]);
        if yb <= ya {
            continue;
        }
        while next < order.len() && edges[order[next]].y0 <= ya {
            active.push(order[next]);
            next += 1;
        }
        active.retain(|&index| edges[index].y1 > ya);
        let ym = (ya + yb) / 2.0;
        let mut crossing: Vec<(f64, usize)> = active
            .iter()
            .filter(|&&index| edges[index].y1 >= yb)
            .map(|&index| (edges[index].x_at(ym), index))
            .collect();
        crossing.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut continued: HashMap<(usize, usize), usize> = HashMap::new();
        for couple in crossing.chunks(2).filter(|couple| couple.len() == 2) {
            let (left, right) = (&edges[couple[0].1], &edges[couple[1].1]);
            let key = (couple[0].1, couple[1].1);
            let (la, ra, lb, rb) = (left.x_at(ya), right.x_at(ya), left.x_at(yb), right.x_at(yb));
            if ra - la <= 0.0 && rb - lb <= 0.0 {
                continue;
            }
            match open.get(&key) {
                Some(&index) if out[index][5] == ya => {
                    out[index][4] = rb;
                    out[index][5] = yb;
                    out[index][6] = lb;
                    out[index][7] = yb;
                    continued.insert(key, index);
                }
                _ => {
                    out.push([la, ya, ra, ya, rb, yb, lb, yb]);
                    continued.insert(key, out.len() - 1);
                }
            }
        }
        open = continued;
    }
    Some(out)
}

/// 模様の 1 つの部分（領域の内側の線分か、長さ 0 の線の点）
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Piece {
    Line([f64; 4]),
    Point([f64; 2]),
}

/// 線分 a → b のうち、偶奇規則で塗る輪郭の組の内側の部分（線分の上の媒介変数 0〜1 の区間）
fn inside_intervals(a: [f64; 2], b: [f64; 2], rings: &[Vec<f64>]) -> Vec<(f64, f64)> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let mut ts = vec![0.0, 1.0];
    for ring in rings {
        let n = ring.len() / 2;
        for i in 0..n {
            let j = (i + 1) % n;
            let (ax, ay, bx, by) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
            let (ex, ey) = (bx - ax, by - ay);
            let denominator = dx * ey - dy * ex;
            if denominator == 0.0 {
                continue;
            }
            let t = ((ax - a[0]) * ey - (ay - a[1]) * ex) / denominator;
            let u = ((ax - a[0]) * dy - (ay - a[1]) * dx) / denominator;
            if t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for pair in ts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if to - from <= 1e-12 {
            continue;
        }
        let middle = (from + to) / 2.0;
        if !inside_rings(a[0] + dx * middle, a[1] + dy * middle, rings) {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 == from => last.1 = to,
            _ => out.push((from, to)),
        }
    }
    out
}

/// 模様の線（用紙の mm）を輪郭の組の内側で切った線分と点。線の破線（正は線、負は空白、0 は点）は、基点から
/// 線の向きに数えて当てる。上限（MAX_PATTERN_ENTITIES）を超えるか、間隔が 0 の線があれば None
pub(super) fn pattern_pieces(lines: &[PatternLine], rings: &[Vec<f64>]) -> Option<Vec<Piece>> {
    let points = rings.iter().flat_map(|ring| ring.as_chunks::<2>().0.iter().copied());
    let (mut min, mut max) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for [x, y] in points {
        min = [min[0].min(x), min[1].min(y)];
        max = [max[0].max(x), max[1].max(y)];
    }
    let corners = [[min[0], min[1]], [max[0], min[1]], [max[0], max[1]], [min[0], max[1]]];
    let mut out = Vec::new();
    for line in lines {
        let (sin, cos) = line.angle.to_radians().sin_cos();
        let (direction, normal) = ([cos, sin], [-sin, cos]);
        let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
        let spacing = dot(line.offset, normal);
        if spacing.abs() <= 1e-12 || !spacing.is_finite() {
            return None;
        }
        let across: Vec<f64> = corners
            .iter()
            .map(|corner| dot([corner[0] - line.base[0], corner[1] - line.base[1]], normal) / spacing)
            .collect();
        let (low, high) =
            across.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), k| (low.min(*k), high.max(*k)));
        let (low, high) = (low.ceil(), high.floor());
        if !(low.is_finite() && high.is_finite()) || high - low + 1.0 > MAX_PATTERN_ENTITIES as f64 {
            return None;
        }
        let period: f64 = line.dashes.iter().map(|dash| dash.abs()).sum();
        let mut k = low;
        while k <= high {
            // k 本目の線の基点（線の向きのずれも含むので、破線の位置は線ごとにずれる）
            let origin = [line.base[0] + k * line.offset[0], line.base[1] + k * line.offset[1]];
            let along: Vec<f64> =
                corners.iter().map(|corner| dot([corner[0] - origin[0], corner[1] - origin[1]], direction)).collect();
            let (from, to) = along.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), t| (a.min(*t), b.max(*t)));
            let at = |t: f64| [origin[0] + direction[0] * t, origin[1] + direction[1] * t];
            for (t0, t1) in inside_intervals(at(from), at(to), rings) {
                let (a, b) = (from + (to - from) * t0, from + (to - from) * t1);
                if line.dashes.is_empty() || period <= 1e-12 {
                    let (p, q) = (at(a), at(b));
                    out.push(Piece::Line([p[0], p[1], q[0], q[1]]));
                } else {
                    dashes(&line.dashes, period, a, b, &at, &mut out);
                }
                if out.len() > MAX_PATTERN_ENTITIES {
                    return None;
                }
            }
            k += 1.0;
        }
    }
    Some(out)
}

/// 区間 a〜b（線の基点からの長さ）に破線を当てる。破線は基点から繰り返す
fn dashes(pattern: &[f64], period: f64, a: f64, b: f64, at: &dyn Fn(f64) -> [f64; 2], out: &mut Vec<Piece>) {
    let mut start = (a / period).floor() * period;
    while start < b {
        let mut position = start;
        for dash in pattern {
            let length = dash.abs();
            if *dash > 0.0 {
                let (from, to) = (position.max(a), (position + length).min(b));
                if to - from > 1e-9 {
                    let (p, q) = (at(from), at(to));
                    out.push(Piece::Line([p[0], p[1], q[0], q[1]]));
                }
            } else if *dash == 0.0 && position >= a && position <= b {
                out.push(Piece::Point(at(position)));
            }
            position += length;
            if out.len() > MAX_PATTERN_ENTITIES {
                return;
            }
        }
        start += period;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zumen_core::edit::hatch::{built_in, pattern_lines};

    fn square(size: f64) -> Vec<f64> {
        vec![0.0, 0.0, size, 0.0, size, size, 0.0, size]
    }

    fn area(quads: &[[f64; 8]]) -> f64 {
        quads.iter().map(|q| ((q[2] - q[0]) + (q[4] - q[6])) / 2.0 * (q[5] - q[1])).sum()
    }

    #[test]
    fn splits_fills_with_holes_into_trapezoids() {
        let hole = vec![1.0, 1.0, 3.0, 1.0, 3.0, 3.0, 1.0, 3.0];
        let quads = trapezoids(&[square(4.0), hole]).unwrap();
        assert!((area(&quads) - 12.0).abs() < 1e-9);
        assert_eq!(trapezoids(&[square(2.0)]).unwrap(), [[0.0, 0.0, 2.0, 0.0, 2.0, 2.0, 0.0, 2.0]]);
        assert!(trapezoids(&[vec![0.0, 0.0, 1.0, 0.0, 2.0, 0.0]]).unwrap().is_empty());
    }

    #[test]
    fn cuts_pattern_lines_to_the_boundary() {
        // 10 mm 角の領域に、y = 1 から 2 mm 間隔の横線: 内側の 5 本
        let lines = [PatternLine { angle: 0.0, base: [0.0, 1.0], offset: [0.0, 2.0], dashes: Vec::new() }];
        let pieces = pattern_pieces(&lines, &[square(10.0)]).unwrap();
        let ys: Vec<f64> = pieces
            .iter()
            .map(|piece| match piece {
                Piece::Line([x1, y1, x2, y2]) => {
                    assert!((x1 - 0.0).abs() < 1e-9 && (x2 - 10.0).abs() < 1e-9 && y1 == y2, "{piece:?}");
                    (y1 * 1e6).round() / 1e6
                }
                Piece::Point(_) => panic!("{piece:?}"),
            })
            .collect();
        assert_eq!(ys, [1.0, 3.0, 5.0, 7.0, 9.0]);
        // 穴の部分は切る
        let hole = vec![4.0, 4.0, 6.0, 4.0, 6.0, 6.0, 4.0, 6.0];
        let pieces = pattern_pieces(&lines, &[square(10.0), hole]).unwrap();
        assert_eq!(pieces.len(), 5 + 1);
        assert!(
            pieces.contains(&Piece::Line([0.0, 5.0, 4.0, 5.0])) && pieces.contains(&Piece::Line([6.0, 5.0, 10.0, 5.0]))
        );
        // 点の模様は点、破線は線分
        let dots = pattern_lines(&built_in("DOTS").unwrap(), 2.0, 0.0);
        let pieces = pattern_pieces(&dots, &[square(10.0)]).unwrap();
        assert!(!pieces.is_empty() && pieces.iter().all(|piece| matches!(piece, Piece::Point(_))));
        let dashed = [PatternLine { angle: 0.0, base: [0.0, 5.0], offset: [0.0, 20.0], dashes: vec![2.0, -1.0] }];
        let pieces = pattern_pieces(&dashed, &[square(10.0)]).unwrap();
        // y = 5 の線だけ: 0〜2、3〜5、6〜8、9〜10
        assert_eq!(pieces.len(), 4, "{pieces:?}");
        assert_eq!(pieces[3], Piece::Line([9.0, 5.0, 10.0, 5.0]));
    }

    #[test]
    fn refuses_too_many_pieces() {
        let lines = pattern_lines(&built_in("LINE").unwrap(), 0.0001, 0.0);
        assert!(pattern_pieces(&lines, &[square(1000.0)]).is_none());
    }
}
