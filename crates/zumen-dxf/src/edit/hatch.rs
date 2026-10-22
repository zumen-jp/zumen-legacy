//! 足すハッチングの模様（docs/decisions/0017）。DXF と DWG の編集が、HATCH の模様の線（DXF Reference「HATCH」の
//! Pattern Data、53・43・44・45・46・79・49）を、模様の名前か PAT の形の線と、尺度・角度から求める。
//!
//! 組み込みの模様は zumen が持つ定義で、CAD の模様のファイルの写しではない。名前は CAD で同じ名前の模様として
//! 見えるよう、よく使われる名前にした。長さは mm（尺度 1 で 3.175 mm 間隔の斜線など）。

use zumen_core::edit::{EditError, EditValue, ValueReader};

/// 模様の定義の 1 本の線（PAT の形: 角度（度）、基点、線の向きの座標系での次の線へのずれ、線・空白の長さ）
#[derive(Debug, Clone, PartialEq)]
pub struct PatternDefinition {
    pub angle: f64,
    pub base: [f64; 2],
    pub offset: [f64; 2],
    pub dashes: Vec<f64>,
}

/// HATCH に書く模様の線（尺度と角度を掛けた後。オフセットは図面の座標系）
#[derive(Debug, Clone, PartialEq)]
pub struct PatternLine {
    /// 度
    pub angle: f64,
    pub base: [f64; 2],
    pub offset: [f64; 2],
    pub dashes: Vec<f64>,
}

/// 組み込みの模様の名前（SOLID は塗りつぶし）
pub const BUILT_IN: [&str; 7] = ["ANSI31", "ANSI32", "ANSI37", "LINE", "NET", "DOTS", "SOLID"];

fn line(angle: f64, base: [f64; 2], offset: [f64; 2], dashes: &[f64]) -> PatternDefinition {
    PatternDefinition { angle, base, offset, dashes: dashes.to_vec() }
}

/// 組み込みの模様の定義
pub fn built_in(name: &str) -> Option<Vec<PatternDefinition>> {
    Some(match name.to_ascii_uppercase().as_str() {
        // 斜線（一般の断面）
        "ANSI31" => vec![line(45.0, [0.0, 0.0], [0.0, 3.175], &[])],
        // 鋼の断面: 斜線を 2 本ずつ
        "ANSI32" => vec![line(45.0, [0.0, 0.0], [0.0, 9.525], &[]), line(45.0, [4.490128, 0.0], [0.0, 9.525], &[])],
        // 交差する斜線
        "ANSI37" => vec![line(45.0, [0.0, 0.0], [0.0, 3.175], &[]), line(135.0, [0.0, 0.0], [0.0, 3.175], &[])],
        "LINE" => vec![line(0.0, [0.0, 0.0], [0.0, 3.175], &[])],
        "NET" => vec![line(0.0, [0.0, 0.0], [0.0, 3.175], &[]), line(90.0, [0.0, 0.0], [0.0, 3.175], &[])],
        // 点（長さ 0 の線と空白）
        "DOTS" => vec![line(0.0, [0.0, 0.0], [0.79375, 1.5875], &[0.0, -1.5875])],
        _ => return None,
    })
}

fn rotate(point: [f64; 2], degrees: f64) -> [f64; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [point[0] * cos - point[1] * sin, point[0] * sin + point[1] * cos]
}

/// 模様の定義に尺度と角度を掛けて、HATCH に書く線にする（ezdxf の pattern.scale_pattern と同じ計算）。
/// PAT のずれは線の向きの座標系なので、線の角度と模様の角度だけ回す
pub fn pattern_lines(definitions: &[PatternDefinition], scale: f64, angle: f64) -> Vec<PatternLine> {
    definitions
        .iter()
        .map(|definition| {
            let scaled = |point: [f64; 2]| [point[0] * scale, point[1] * scale];
            PatternLine {
                angle: definition.angle + angle,
                base: rotate(scaled(definition.base), angle),
                offset: rotate(scaled(definition.offset), definition.angle + angle),
                dashes: definition.dashes.iter().map(|dash| dash * scale).collect(),
            }
        })
        .collect()
}

/// 足すハッチングの値
#[derive(Debug, Clone, PartialEq)]
pub struct HatchInput {
    /// 境界（閉じた折線。最初が外側、ほかは穴）
    pub boundaries: Vec<Vec<[f64; 3]>>,
    pub pattern: String,
    pub solid: bool,
    pub scale: f64,
    /// 度
    pub angle: f64,
    pub lines: Vec<PatternLine>,
    /// 0 利用者定義、1 定義済み、2 カスタム（DXF の 76）
    pub pattern_type: i64,
}

/// add の hatch の値のキー
pub const KEYS: [&str; 5] = ["boundaries", "pattern", "patternScale", "patternAngle", "patternLines"];

/// add の hatch の値を読む。boundaries は [[x, y], ...] の並び（1 つなら [[x, y], ...] だけでもよい）
pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<HatchInput, EditError> {
    let invalid = |message: &str| EditError::at(index, "invalid-value", message.to_string());
    let to_point = |values: &Vec<f64>| -> Option<[f64; 3]> {
        match values.as_slice() {
            [x, y] => Some([*x, *y, 0.0]),
            [x, y, z] => Some([*x, *y, *z]),
            _ => None,
        }
    };
    let ring = |points: &Vec<Vec<f64>>| -> Result<Vec<[f64; 3]>, EditError> {
        let ring: Option<Vec<[f64; 3]>> = points.iter().map(to_point).collect();
        match ring {
            Some(ring) if ring.len() >= 3 => Ok(ring),
            _ => Err(invalid("boundaries の境界は 3 点以上の [[x, y], ...] にしてください")),
        }
    };
    let boundaries = match reader.value("boundaries") {
        Some(EditValue::Paths(paths)) if !paths.is_empty() => paths.iter().map(ring).collect::<Result<Vec<_>, _>>()?,
        Some(EditValue::Points(points)) => vec![ring(points)?],
        Some(_) => return Err(invalid("boundaries は境界（[[x, y], ...]）の配列にしてください")),
        None => return Err(invalid("boundaries がありません")),
    };
    let elevation = boundaries[0][0][2];
    if boundaries.iter().flatten().any(|point| point[2] != elevation) {
        return Err(invalid("boundaries の点の z はすべて同じにしてください"));
    }
    let pattern = reader.name("pattern")?.unwrap_or("SOLID").trim().to_string();
    let scale = reader.positive("patternScale")?.unwrap_or(1.0);
    let angle = reader.number("patternAngle")?.unwrap_or(0.0);
    let solid = pattern.eq_ignore_ascii_case("SOLID");
    let custom = match reader.value("patternLines") {
        None => None,
        Some(EditValue::Points(lines)) => Some(
            lines
                .iter()
                .map(|line| match line.as_slice() {
                    [angle, x, y, dx, dy, dashes @ ..] => Ok(PatternDefinition {
                        angle: *angle,
                        base: [*x, *y],
                        offset: [*dx, *dy],
                        dashes: dashes.to_vec(),
                    }),
                    _ => Err(invalid("patternLines の線は [角度, x, y, dx, dy, 線・空白の長さ…] にしてください")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err(invalid("patternLines は [[角度, x, y, dx, dy, …], …] にしてください")),
    };
    let (definitions, pattern_type) = match (solid, custom) {
        (true, Some(_)) => return Err(invalid("SOLID に patternLines は指定できません")),
        (true, None) => (Vec::new(), 1),
        (false, Some(lines)) => (lines, 2),
        (false, None) => match built_in(&pattern) {
            Some(lines) => (lines, 1),
            None => {
                return Err(EditError::at(
                    index,
                    "missing-definition",
                    format!(
                        "模様 {pattern} の定義がありません（組み込みは {}。ほかは patternLines で定義してください）",
                        BUILT_IN.join("、")
                    ),
                ));
            }
        },
    };
    let pattern = if solid { "SOLID".to_string() } else { pattern.to_ascii_uppercase() };
    let lines = pattern_lines(&definitions, scale, angle);
    Ok(HatchInput { boundaries, pattern, solid, scale, angle, lines, pattern_type })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_and_rotates_pattern_lines() {
        let lines = pattern_lines(&built_in("ansi31").unwrap(), 10.0, 0.0);
        assert_eq!(lines[0].angle, 45.0);
        // 線の向きの座標系の (0, 31.75) を 45° 回したずれ
        assert!((lines[0].offset[0] + 22.450640303).abs() < 1e-6 && (lines[0].offset[1] - 22.450640303).abs() < 1e-6);
        let rotated = pattern_lines(&built_in("LINE").unwrap(), 2.0, 90.0);
        assert_eq!(rotated[0].angle, 90.0);
        assert!((rotated[0].offset[0] + 6.35).abs() < 1e-9 && rotated[0].offset[1].abs() < 1e-9);
        let dots = pattern_lines(&built_in("DOTS").unwrap(), 2.0, 0.0);
        assert_eq!(dots[0].dashes, [0.0, -3.175]);
    }
}
