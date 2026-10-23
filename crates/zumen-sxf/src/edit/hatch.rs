//! 足すハッチングの模様を SXF のハッチング（ユーザ定義、fill_area_style_hatching）の線にする（docs/decisions/0025）。
//! SXF のハッチング線は、色・線種・線幅、基点、間隔、角度を持つ（附属書 SFC 編 1-2「ハッチング（ユーザ定義）」）。
//! PAT の線の破線は線種にし、線ごとの破線のずれ（線の向きのオフセット）は持てない。

use zumen_core::edit::hatch::PatternLine;
use zumen_core::edit::style::pitches;

/// SXF のハッチング線にする模様の線
#[derive(Debug, Clone, PartialEq)]
pub(super) struct HatchLine {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) spacing: f64,
    /// 度（0 以上 360 未満）
    pub(super) angle: f64,
    /// 線種のピッチ（用紙の mm、線と空白の組）。実線は空
    pub(super) pitch: Vec<f64>,
    /// 線ごとの破線のずれを持てずになくした
    pub(super) shifted: bool,
}

/// 模様の線（入れ物の座標）を SXF のハッチング線にする。scale は入れ物の座標の 1 が用紙の何 mm か、near は領域の
/// 近くの点（遠い基点は、読み取りが線を数える計算の桁を失うので、同じ線の上の近い点に移す）。間隔が 0 なら None
pub(super) fn hatch_line(line: &PatternLine, scale: f64, near: [f64; 2]) -> Option<HatchLine> {
    let (sin, cos) = line.angle.to_radians().sin_cos();
    let (direction, normal) = ([cos, sin], [-sin, cos]);
    let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    let spacing = dot(line.offset, normal);
    if !(spacing.abs() > 1e-12 && spacing.is_finite()) {
        return None;
    }
    let paper: Vec<f64> = line.dashes.iter().map(|dash| dash * scale).collect();
    // PAT の線・空白（用紙の mm）を、SXF のユーザ定義線種のピッチにする（docs/decisions/0026）
    let (pitch, shift) = pitches(&paper);
    let period: f64 = pitch.iter().sum::<f64>() / scale;
    // 線ごとのずれ（線の向きのオフセット）は、破線の繰り返しの長さの倍数でなければ描き方が変わる
    let along = dot(line.offset, direction);
    let shifted = !pitch.is_empty() && {
        let rest = along.rem_euclid(period);
        rest > 1e-9 * period.max(1.0) && period - rest > 1e-9 * period.max(1.0)
    };
    let mut base = [line.base[0] + direction[0] * shift / scale, line.base[1] + direction[1] * shift / scale];
    // 線の間隔の倍数だけ法線の向きに、実線は線の向きにも（破線は繰り返しの長さの倍数だけ）近くへ移す
    let steps = (dot([near[0] - base[0], near[1] - base[1]], normal) / spacing).round();
    base = [base[0] + normal[0] * steps * spacing, base[1] + normal[1] * steps * spacing];
    let along_near = dot([near[0] - base[0], near[1] - base[1]], direction);
    let moved = if pitch.is_empty() { along_near } else { (along_near / period).floor() * period };
    if moved.is_finite() {
        base = [base[0] + direction[0] * moved, base[1] + direction[1] * moved];
    }
    Some(HatchLine {
        x: base[0],
        y: base[1],
        spacing: spacing.abs(),
        angle: line.angle.rem_euclid(360.0),
        pitch,
        shifted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zumen_core::edit::hatch::{built_in, pattern_lines};

    #[test]
    fn keeps_the_lines_and_moves_the_base_near_the_region() {
        let lines = pattern_lines(&built_in("ANSI31").unwrap(), 100.0, 0.0);
        let line = hatch_line(&lines[0], 1.0, [100_000.0, 50_000.0]).unwrap();
        assert_eq!(line.angle, 45.0);
        assert!((line.spacing - 317.5).abs() < 1e-9 && line.pitch.is_empty() && !line.shifted);
        assert!((line.x - 100_000.0).abs() < 400.0 && (line.y - 50_000.0).abs() < 400.0, "{line:?}");
        // 同じ線の上の点（法線の向きの位置が間隔の倍数）
        let across = (-line.x + line.y) / 2f64.sqrt() / 317.5;
        assert!((across - across.round()).abs() < 1e-6, "{across}");
        let dots = pattern_lines(&built_in("DOTS").unwrap(), 1.0, 0.0);
        let line = hatch_line(&dots[0], 1.0, [0.0, 0.0]).unwrap();
        assert_eq!(line.pitch, [0.25, 1.3375]);
        assert!(line.shifted);
    }
}
