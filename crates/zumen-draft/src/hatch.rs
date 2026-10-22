//! ハッチング。境界は閉じた折線（1 つ目が外側、ほかは穴）で、塗り（SOLID）か模様の線で描く。
//!
//! HATCH は模様のファイルの名前ではなく模様の線を持つ（DXF Reference「HATCH」の Pattern Data）。組み込みの模様は
//! zumen で書いた定義で、よく使われる名前を付けている（CAD の模様のファイルの写しではない）。定義は PAT の書き方
//! （角度, 基点 x, y, ずれ dx, dy, 線分, 空白…）で、長さは用紙の mm。

use crate::spec::{HatchSpec, Point, StyleSpec};
use crate::{Ctx, DraftError, Drafter, MAX_PARTS, number, points, positive};
use zumen_convert::builder::{Look, lwpolyline, polyline_path, solid_hatch};
use zumen_dxf::{DxfEntityData, DxfHatch, DxfPatternLine};

/// 組み込みの模様（名前、説明、PAT の線）
pub(crate) const PATTERNS: [(&str, &str, &[&[f64]]); 8] = [
    ("LINE", "水平の平行線", &[&[0.0, 0.0, 0.0, 0.0, 3.175]]),
    ("ANSI31", "45° の平行線（断面・一般）", &[&[45.0, 0.0, 0.0, 0.0, 3.175]]),
    (
        "ANSI32",
        "45° の平行線の組（鋼）",
        &[
            &[45.0, 0.0, 0.0, 0.0, 4.0],
            &[45.0, -std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2, 0.0, 4.0],
        ],
    ),
    ("ANSI37", "45° と 135° の格子", &[&[45.0, 0.0, 0.0, 0.0, 3.175], &[135.0, 0.0, 0.0, 0.0, 3.175]]),
    ("NET", "水平と垂直の格子", &[&[0.0, 0.0, 0.0, 0.0, 3.175], &[90.0, 0.0, 0.0, 0.0, 3.175]]),
    ("DOTS", "点", &[&[0.0, 0.0, 0.0, 0.79375, 1.5875, 0.0, -1.5875]]),
    ("DASH", "水平の破線", &[&[0.0, 0.0, 0.0, 3.175, 3.175, 3.175, -3.175]]),
    (
        "GROUND",
        "地盤（45° の破線と 135° の短い線）",
        &[&[45.0, 0.0, 0.0, 1.0, 2.0, 2.0, -1.0], &[135.0, 0.0, 0.0, 1.0, 4.0, 1.0, -3.0]],
    ),
];

fn rotate(p: Point, degrees: f64) -> Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
}

/// PAT の線を、模様の角度・尺度を掛けた HATCH の模様の線にする。PAT のずれは線の向きの座標系で、HATCH は図面の
/// 座標系（ezdxf の pattern.load と同じ）
fn pattern_line(def: &[f64], angle: f64, scale: f64) -> DxfPatternLine {
    let line_angle = def[0];
    let base = rotate([def[1], def[2]], angle);
    let offset = rotate([def[3], def[4]], line_angle + angle);
    DxfPatternLine {
        angle: (line_angle + angle).rem_euclid(360.0),
        base: [base[0] * scale, base[1] * scale],
        offset: [offset[0] * scale, offset[1] * scale],
        dashes: def[5..].iter().map(|dash| dash * scale).collect(),
    }
}

fn boundaries(spec: &HatchSpec, path: &str) -> Result<Vec<Vec<Point>>, DraftError> {
    if spec.boundaries.is_empty() {
        return Err(DraftError::new("invalid-value", path, "boundaries は 1 つ以上にしてください"));
    }
    if spec.boundaries.len() > MAX_PARTS {
        return Err(DraftError::new("invalid-value", path, "boundaries が多すぎます"));
    }
    spec.boundaries.iter().map(|boundary| points(boundary, 3, path)).collect()
}

fn paths(boundaries: &[Vec<Point>]) -> Vec<zumen_dxf::DxfHatchPath> {
    boundaries
        .iter()
        .enumerate()
        .map(|(index, points)| polyline_path(points, None, if index == 0 { 1.0 } else { 0.0 }))
        .collect()
}

/// 塗り（SOLID）のハッチング
pub(crate) fn solid(drafter: &mut Drafter, ctx: &Ctx, look: &Look, boundaries: &[Vec<Point>], path: &str) {
    drafter.add(&ctx.space, look, "HATCH", solid_hatch(paths(boundaries)), path);
}

pub(crate) fn draw(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &HatchSpec,
    path: &str,
) -> Result<(), DraftError> {
    let default_layer = drafter.defaults.hatch_layer.clone();
    let look = drafter.look(ctx, style, default_layer.as_deref(), path)?;
    let boundaries = boundaries(spec, path)?;
    let name = spec.pattern.clone().unwrap_or_else(|| "SOLID".into());
    let upper = name.to_uppercase();
    if upper == "SOLID" {
        if spec.pattern_lines.is_some() || spec.scale.is_some() || spec.angle.is_some() {
            return Err(DraftError::new(
                "invalid-value",
                path,
                "SOLID の塗りには patternLines・scale・angle を指定できません",
            ));
        }
        solid(drafter, ctx, &look, &boundaries, path);
    } else {
        let scale = positive(spec.scale.unwrap_or(1.0), "scale", path)?;
        let angle = number(spec.angle.unwrap_or(0.0), "angle", path)?;
        let total = scale * ctx.scale;
        let (definitions, predefined): (Vec<Vec<f64>>, bool) = match &spec.pattern_lines {
            Some(lines) => (lines.clone(), false),
            None => match PATTERNS.iter().find(|(pattern, _, _)| *pattern == upper) {
                Some((_, _, lines)) => (lines.iter().map(|line| line.to_vec()).collect(), true),
                None => {
                    let names: Vec<&str> = PATTERNS.iter().map(|(name, _, _)| *name).collect();
                    return Err(DraftError::new(
                        "missing-definition",
                        path,
                        format!(
                            "模様 {name} がありません（SOLID と {}、または patternLines で定義）",
                            names.join("・")
                        ),
                    ));
                }
            },
        };
        let mut lines = Vec::with_capacity(definitions.len());
        for definition in &definitions {
            if definition.len() < 5 || definition.iter().any(|value| !value.is_finite()) {
                return Err(DraftError::new(
                    "invalid-value",
                    path,
                    "patternLines の線は [角度, x, y, dx, dy, 線分, 空白…] の数で指定してください",
                ));
            }
            if definition[3] == 0.0 && definition[4] == 0.0 {
                return Err(DraftError::new("invalid-value", path, "patternLines の線のずれ（dx, dy）が 0 です"));
            }
            lines.push(pattern_line(definition, angle, total));
        }
        let data = DxfEntityData::Hatch(DxfHatch {
            pattern_name: name.clone(),
            solid_fill: false,
            associative: false,
            hatch_style: 0.0,
            // 1 は定義済みの模様、2 は利用者の模様（DXF Reference「HATCH」の 76）
            pattern_type: if predefined { 1.0 } else { 2.0 },
            pattern_angle: angle.rem_euclid(360.0),
            pattern_scale: total,
            pattern_double: false,
            elevation: 0.0,
            paths: paths(&boundaries),
            pattern_lines: lines,
            gradient: None,
            gradient_colors: None,
            gradient_angle: 0.0,
            gradient_shift: 0.0,
            gradient_one_color: false,
            gradient_tint: 0.0,
        });
        drafter.add(&ctx.space, &look, "HATCH", data, path);
    }
    if spec.outline {
        let outline_look = drafter.look(ctx, style, None, path)?;
        for boundary in &boundaries {
            drafter.add(&ctx.space, &outline_look, "LWPOLYLINE", lwpolyline(boundary, None, true), path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_offsets_into_drawing_coordinates() {
        let line = pattern_line(&[45.0, 0.0, 0.0, 0.0, 3.175], 0.0, 100.0);
        assert_eq!(line.angle, 45.0);
        let spacing = 3.175 * 100.0;
        assert!((line.offset[0] + spacing * 45f64.to_radians().sin()).abs() < 1e-9);
        assert!((line.offset[1] - spacing * 45f64.to_radians().cos()).abs() < 1e-9);
        let turned = pattern_line(&[0.0, 1.0, 0.0, 0.0, 1.0, 2.0, -1.0], 90.0, 2.0);
        assert_eq!(turned.angle, 90.0);
        assert!((turned.base[1] - 2.0).abs() < 1e-9);
        assert_eq!(turned.dashes, vec![4.0, -2.0]);
    }
}
