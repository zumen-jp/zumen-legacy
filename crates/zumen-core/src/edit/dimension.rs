//! 足す寸法の種類と定義点（docs/decisions/0017）。add の dimension の値の読み方と、寸法線・寸法の弧がどこを通り何を
//! 測るかの、形式によらない形。DXF・DWG・SXF・JWW の編集が使い、矢印・文字の大きさと書き方は形式ごとに決める。

use super::{EditError, ValueReader};
use std::f64::consts::TAU;

/// 寸法の種類
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimensionKind {
    /// 回転寸法（寸法線の角度、度）
    Linear(f64),
    /// 平行寸法（寸法補助線の起点を結ぶ向き）
    Aligned,
}

/// add の dimension の値のキー（docs/decisions/0017）
pub const KEYS: [&str; 9] =
    ["dimensionType", "start", "end", "position", "rotation", "center", "point", "dimensionStyle", "text"];

/// 足す寸法の種類と定義点（WCS）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimensionInput {
    /// 回転寸法・平行寸法。start・end は寸法補助線の起点（13・14）、position は寸法線の上の点
    Linear { kind: DimensionKind, start: [f64; 3], end: [f64; 3], position: [f64; 3] },
    /// 3 点角度寸法。center は頂点（15）、start・end は寸法補助線の起点（13・14）、position は寸法の弧の上の点（10）。
    /// 測る角は、2 本の半直線に挟まれた 2 つの角のうち position の向きを含むほう
    Angular { center: [f64; 3], start: [f64; 3], end: [f64; 3], position: [f64; 3] },
    /// 半径寸法。center は中心（10）、point は円の上の点（15）
    Radius { center: [f64; 3], point: [f64; 3] },
    /// 直径寸法。point は円の上の点（15）、その反対の点（10）を center から求める
    Diameter { center: [f64; 3], point: [f64; 3] },
}

impl DimensionInput {
    /// 値から読む。dimensionType は scene の寸法の種類のキー（linear、aligned、angular-3-point、radius、diameter）
    pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<Self, EditError> {
        let kind = reader.text("dimensionType")?.unwrap_or("linear");
        let used: &[&str] = match kind {
            "linear" | "aligned" => &["start", "end", "position", "rotation"],
            "angular-3-point" => &["center", "start", "end", "position"],
            "radius" | "diameter" => &["center", "point"],
            other => {
                return Err(EditError::at(
                    index,
                    "unsupported-kind",
                    format!(
                        "寸法の種類 {other} はまだ足せません（足せるのは linear、aligned、angular-3-point、radius、diameter）"
                    ),
                ));
            }
        };
        if kind == "aligned" && reader.has("rotation") {
            return Err(EditError::at(index, "invalid-value", "平行寸法（aligned）に rotation は指定できません"));
        }
        for key in ["start", "end", "position", "rotation", "center", "point"] {
            if !used.contains(&key) && reader.has(key) {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("寸法の種類 {kind} に {key} は指定できません"),
                ));
            }
        }
        let point = |key: &str| reader.required(key, reader.point(key)?);
        Ok(match kind {
            "linear" | "aligned" => Self::Linear {
                kind: if kind == "linear" {
                    DimensionKind::Linear(reader.number("rotation")?.unwrap_or(0.0))
                } else {
                    DimensionKind::Aligned
                },
                start: point("start")?,
                end: point("end")?,
                position: point("position")?,
            },
            "angular-3-point" => Self::Angular {
                center: point("center")?,
                start: point("start")?,
                end: point("end")?,
                position: point("position")?,
            },
            "radius" => Self::Radius { center: point("center")?, point: point("point")? },
            _ => Self::Diameter { center: point("center")?, point: point("point")? },
        })
    }

    /// 寸法線・寸法の弧がどこを通り何を測るか（2 次元。z は使わない）。点が重なって寸法にならなければ、その理由
    pub fn geometry(&self) -> Result<DimensionGeometry, &'static str> {
        let flat = |p: [f64; 3]| [p[0], p[1]];
        match *self {
            Self::Linear { kind, start, end, position } => {
                let (start, end, position) = (flat(start), flat(end), flat(position));
                let linear = || {
                    let direction = match kind {
                        DimensionKind::Linear(angle) => {
                            let radians = angle.to_radians();
                            [radians.cos(), radians.sin()]
                        }
                        DimensionKind::Aligned => unit(sub(end, start))?,
                    };
                    let measurement = dot(sub(end, start), direction).abs();
                    if measurement <= 1e-12 {
                        return None;
                    }
                    // 寸法線の上の、寸法補助線の足
                    let foot = |p: [f64; 2]| add(position, scaled(direction, dot(sub(p, position), direction)));
                    Some(DimensionGeometry::Linear {
                        origins: [start, end],
                        feet: [foot(start), foot(end)],
                        direction,
                        measurement,
                    })
                };
                linear().ok_or("start と end は寸法の向きで離れた点にしてください")
            }
            Self::Angular { center, start, end, position } => {
                let (center, start, end, position) = (flat(center), flat(start), flat(end), flat(position));
                let angular = || {
                    let first = unit(sub(start, center))?;
                    let second = unit(sub(end, center))?;
                    let toward = unit(sub(position, center))?;
                    let radius = dot(sub(position, center), toward);
                    let direction = |v: [f64; 2]| v[1].atan2(v[0]);
                    let ccw = |from: f64, to: f64| (to - from).rem_euclid(TAU);
                    let (a, b) = (direction(first), direction(second));
                    let span = ccw(a, b);
                    if span <= 1e-9 || TAU - span <= 1e-9 {
                        return None;
                    }
                    // 弧は反時計回りに from から span だけ。position の向きを含むほうの角を測る
                    Some(if ccw(a, direction(toward)) <= span {
                        DimensionGeometry::Angular { center, radius, from: a, span, origins: [start, end] }
                    } else {
                        DimensionGeometry::Angular { center, radius, from: b, span: TAU - span, origins: [end, start] }
                    })
                };
                angular().ok_or("start・end・position は center から離し、start と end は違う向きにしてください")
            }
            Self::Radius { center, point } | Self::Diameter { center, point } => {
                let diameter = matches!(self, Self::Diameter { .. });
                let (center, point) = (flat(center), flat(point));
                let radial = || {
                    let out = unit(sub(point, center))?;
                    let radius = dot(sub(point, center), out);
                    let opposite = sub(scaled(center, 2.0), point);
                    let (from, measurement) = if diameter { (opposite, radius * 2.0) } else { (center, radius) };
                    Some(DimensionGeometry::Radial { center, from, to: point, measurement, diameter })
                };
                radial().ok_or("point は center から離してください")
            }
        }
    }
}

/// 寸法線・寸法の弧がどこを通り何を測るか（2 次元）
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimensionGeometry {
    /// 回転寸法・平行寸法。寸法線は feet[0] から feet[1]（寸法補助線の起点 origins の、寸法線の上の足）で、向きは
    /// direction（単位ベクトル）。measurement は起点の間の direction の向きの長さ
    Linear { origins: [[f64; 2]; 2], feet: [[f64; 2]; 2], direction: [f64; 2], measurement: f64 },
    /// 3 点角度寸法。寸法の弧は center を中心に半径 radius、角 from（ラジアン）から反時計回りに span（ラジアン、測る角）。
    /// origins は寸法補助線の起点で、origins[0] が from の側
    Angular { center: [f64; 2], radius: f64, from: f64, span: f64, origins: [[f64; 2]; 2] },
    /// 半径寸法・直径寸法。寸法線は from（半径は中心、直径は円の上の点の反対の点）から to（円の上の点）
    Radial { center: [f64; 2], from: [f64; 2], to: [f64; 2], measurement: f64, diameter: bool },
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scaled(v: [f64; 2], k: f64) -> [f64; 2] {
    [v[0] * k, v[1] * k]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-12).then(|| [v[0] / length, v[1] / length])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{EditOperation, parse_operations};

    fn input(json: &str) -> Result<DimensionInput, EditError> {
        let operations = parse_operations(json).unwrap();
        let EditOperation::Add { values, .. } = &operations[0] else { panic!() };
        DimensionInput::read(&ValueReader::new(values, 0), 0)
    }

    #[test]
    fn measures_linear_dimensions_along_their_direction() {
        let linear = input(
            r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [5000, 300], "position": [0, -800]}]"#,
        )
        .unwrap();
        let DimensionGeometry::Linear { feet, measurement, .. } = linear.geometry().unwrap() else { panic!() };
        assert_eq!((feet, measurement), ([[0.0, -800.0], [5000.0, -800.0]], 5000.0));
        let aligned = input(
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "aligned", "start": [0, 0], "end": [300, 400], "position": [0, 0]}]"#,
        )
        .unwrap();
        let DimensionGeometry::Linear { measurement, direction, .. } = aligned.geometry().unwrap() else { panic!() };
        assert!((measurement - 500.0).abs() < 1e-12 && (direction[0] - 0.6).abs() < 1e-12);
        let vertical = input(
            r#"[{"op": "add", "kind": "dimension", "rotation": 90, "start": [0, 0], "end": [10, 0], "position": [0, 0]}]"#,
        )
        .unwrap();
        assert!(vertical.geometry().is_err());
    }

    /// 3 点角度寸法は position の向きを含む角を測り、origins は弧の始まりの側から並べる
    #[test]
    fn angular_dimensions_measure_the_angle_that_holds_their_position() {
        let inside = input(
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "angular-3-point", "center": [0, 0], "start": [1000, 0], "end": [0, 1000], "position": [700, 700]}]"#,
        )
        .unwrap();
        let DimensionGeometry::Angular { from, span, origins, .. } = inside.geometry().unwrap() else { panic!() };
        assert!(from.abs() < 1e-12 && (span - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(origins, [[1000.0, 0.0], [0.0, 1000.0]]);
        let outside = input(
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "angular-3-point", "center": [0, 0], "start": [1000, 0], "end": [0, 1000], "position": [-500, -500]}]"#,
        )
        .unwrap();
        let DimensionGeometry::Angular { span, origins, .. } = outside.geometry().unwrap() else { panic!() };
        assert!((span - 1.5 * std::f64::consts::PI).abs() < 1e-12);
        assert_eq!(origins, [[0.0, 1000.0], [1000.0, 0.0]]);
    }

    #[test]
    fn radial_dimensions_start_at_the_center_or_across_the_circle() {
        let diameter = input(
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "diameter", "center": [0, 0], "point": [300, 400]}]"#,
        )
        .unwrap();
        let DimensionGeometry::Radial { from, measurement, .. } = diameter.geometry().unwrap() else { panic!() };
        assert_eq!((from, measurement), ([-300.0, -400.0], 1000.0));
        let error =
            input(r#"[{"op": "add", "kind": "dimension", "dimensionType": "ordinate", "start": [0, 0]}]"#).unwrap_err();
        assert_eq!(error.code, "unsupported-kind");
        let error = input(
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "radius", "center": [0, 0], "point": [1, 0], "rotation": 5}]"#,
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid-value");
    }
}
