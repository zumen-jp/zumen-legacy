//! 矢印・点マーカの形（`@zumenjs/sxf` の symbols.ts）。大きさは用紙上の mm で、部分図の縮尺や部品の倍率の
//! 影響を受けない（SXF 実装規約 1-4・1-6）。矢印は線種によらず実線で描く。

use std::f64::consts::{FRAC_1_SQRT_2, PI};
use zumen_core::{DotPrimitive, FillPrimitive, Primitive, StrokePrimitive, js};

fn stroke(points: Vec<f64>, closed: bool, color: &str, width: f64) -> Primitive {
    Primitive::Stroke(StrokePrimitive {
        points,
        closed,
        segments: None,
        color: color.to_string(),
        width,
        dashes: Vec::new(),
    })
}

fn fill(ring: Vec<f64>, color: &str) -> Primitive {
    Primitive::Fill(FillPrimitive { color: color.to_string(), rings: vec![ring] })
}

fn circle_points(cx: f64, cy: f64, r: f64) -> Vec<f64> {
    const SEGMENTS: usize = 24;
    let mut out = Vec::with_capacity(SEGMENTS * 2);
    for i in 0..SEGMENTS {
        let t = (i as f64 / SEGMENTS as f64) * PI * 2.0;
        out.push(cx + r * js::cos(t));
        out.push(cy + r * js::sin(t));
    }
    out
}

/// 局所座標（x: 矢印の向き、y: 左）で作った点列を、先端 (x, y)・向き (ux, uy) に置く
fn place(points: &[f64], x: f64, y: f64, ux: f64, uy: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(points.len());
    for [px, py] in points.as_chunks::<2>().0 {
        out.push(x + px * ux - py * uy);
        out.push(y + px * uy + py * ux);
    }
    out
}

/// 矢印を作る。先端 (x, y)、矢印が指す向き (ux, uy)（単位ベクトル）、倍率 scale。
/// 基本の大きさは長さ 7.5mm・幅 2.5mm、箱と点は 2.5mm（附属書 SFC 編 図 19）
#[allow(clippy::too_many_arguments)]
pub fn arrow_primitives(
    code: f64,
    x: f64,
    y: f64,
    ux: f64,
    uy: f64,
    scale: f64,
    color: &str,
    width: f64,
    background: &str,
) -> Vec<Primitive> {
    let s = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let length = 7.5 * s;
    let half = 1.25 * s;
    let size = 2.5 * s;
    // 先端から後ろへ伸びる三角形（局所座標では先端が原点で、本体は -x 側）
    let triangle = || place(&[0.0, 0.0, -length, half, -length, -half], x, y, ux, uy);
    let square = || place(&[0.0, -size / 2.0, -size, -size / 2.0, -size, size / 2.0, 0.0, size / 2.0], x, y, ux, uy);
    if code == 1.0 {
        vec![fill(triangle(), background), stroke(triangle(), true, color, width)]
    } else if code == 2.0 {
        vec![fill(square(), background), stroke(square(), true, color, width)]
    } else if code == 3.0 {
        let dot = circle_points(x, y, size / 2.0);
        vec![fill(dot.clone(), background), stroke(dot, true, color, width)]
    } else if code == 4.0 {
        vec![stroke(circle_points(x, y, size / 2.0), true, color, width)]
    } else if code == 5.0 {
        vec![fill(square(), color)]
    } else if code == 6.0 {
        vec![fill(triangle(), color)]
    } else if code == 7.0 {
        vec![fill(circle_points(x, y, size / 2.0), color)]
    } else if code == 8.0 {
        // 積分記号：先端を中心に 45 度傾けた S 字
        let mut curve = Vec::with_capacity(34);
        for i in 0..=16 {
            let t = -1.0 + (2.0 * f64::from(i)) / 16.0;
            curve.push(t * size * 0.35 + js::sin(t * PI) * size * 0.15);
            curve.push(t * size * 0.5);
        }
        vec![stroke(place(&curve, x, y, ux, uy), false, color, width)]
    } else if code == 9.0 {
        vec![stroke(place(&[-length, half, 0.0, 0.0, -length, -half], x, y, ux, uy), false, color, width)]
    } else if code == 10.0 {
        let d = (size / 2.0) * FRAC_1_SQRT_2;
        vec![stroke(place(&[-d, -d, d, d], x, y, ux, uy), false, color, width)]
    } else if code == 11.0 {
        vec![stroke(triangle(), true, color, width)]
    } else {
        Vec::new()
    }
}

/// 点マーカ。dot（3）は画面上で一定の大きさ。他は 2.5mm × 倍率を、用紙に対する角度で描く
pub fn marker_primitives(
    code: f64,
    x: f64,
    y: f64,
    angle_deg: f64,
    scale: f64,
    color: &str,
    width: f64,
) -> Vec<Primitive> {
    let dot = || Primitive::Dot(DotPrimitive { x, y, color: color.to_string() });
    if code == 3.0 {
        return vec![dot()];
    }
    let s = (if scale.is_finite() && scale > 0.0 { scale } else { 1.0 }) * 2.5;
    let h = s / 2.0;
    let r = (angle_deg * PI) / 180.0;
    let ux = js::cos(r);
    let uy = js::sin(r);
    let at = |points: &[f64]| place(points, x, y, ux, uy);
    if code == 1.0 {
        [0.0, 60.0, 120.0]
            .iter()
            .map(|deg: &f64| {
                let t = (deg * PI) / 180.0;
                stroke(at(&[-h * js::cos(t), -h * js::sin(t), h * js::cos(t), h * js::sin(t)]), false, color, width)
            })
            .collect()
    } else if code == 2.0 {
        vec![stroke(circle_points(x, y, h), true, color, width)]
    } else if code == 4.0 {
        vec![stroke(at(&[-h, 0.0, h, 0.0]), false, color, width), stroke(at(&[0.0, -h, 0.0, h]), false, color, width)]
    } else if code == 5.0 {
        vec![stroke(at(&[-h, -h, h, -h, h, h, -h, h]), true, color, width)]
    } else if code == 6.0 {
        let top = (s * 3f64.sqrt()) / 3.0;
        let bottom = -(s * 3f64.sqrt()) / 6.0;
        vec![stroke(at(&[0.0, top, -h, bottom, h, bottom]), true, color, width)]
    } else if code == 7.0 {
        vec![stroke(at(&[-h, -h, h, h]), false, color, width), stroke(at(&[-h, h, h, -h]), false, color, width)]
    } else {
        vec![dot()]
    }
}
