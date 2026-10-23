//! 足す寸法を SXF の寸法のフィーチャにする（docs/decisions/0025）。回転・平行寸法は直線寸法（linear_dim_feature）、
//! 3 点角度寸法は角度寸法（angular_dim_feature）、半径・直径寸法は半径寸法・直径寸法（radius_dim_feature・
//! diameter_dim_feature）。SXF は寸法スタイルを持たないので、大きさは用紙の mm で決めた値（ISO-25 と同じ: 文字の
//! 高さ 2.5、矢印 2.5、文字と寸法線の間 0.625、寸法補助線の起点からの離れ 0.625、寸法線を越える長さ 1.25）を、
//! 入れ物の座標（用紙なら mm、部分図ならその配置の尺度で割った長さ）にする。矢印の大きさは配置の尺度によらない
//! （SXF 実装規約 1-4）。

use crate::features::{Annotation, ArcDimension, Arrow, ExtensionLine, Styled, SxfFeature};
use zumen_core::edit::dimension::DimensionGeometry;

/// 寸法値の文字の高さ・文字と寸法線の間・寸法補助線の離れと越える長さ（用紙の mm）
pub(super) const TEXT_HEIGHT: f64 = 2.5;
const TEXT_GAP: f64 = 0.625;
const EXTENSION_OFFSET: f64 = 0.625;
const EXTENSION_EXTENSION: f64 = 1.25;
/// 矢印（塗りつぶしの矢印、コード 6。附属書 SFC 編 図 19 の大きさ: 倍率 1 で長さ 7.5 mm・幅 2.5 mm）の長さ
const ARROW_LENGTH: f64 = 2.5;
const FILLED_ARROW: f64 = 6.0;
/// 寸法値の文字の配置基点（中下）
const BOTTOM_CENTER: f64 = 2.0;

/// 寸法を描く入れ物の座標の 1 が、用紙の何 mm か
#[derive(Debug, Clone, Copy)]
pub(super) struct Container {
    pub(super) scale: f64,
}

impl Container {
    fn length(&self, millimeters: f64) -> f64 {
        millimeters / self.scale
    }
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

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-12).then(|| [v[0] / length, v[1] / length])
}

/// 向き along の文字の角度（度）と読む向き。読める向き（-90° より大きく 90° 以下）にする
fn reading(along: [f64; 2]) -> (f64, [f64; 2]) {
    let angle = along[1].atan2(along[0]).to_degrees();
    if angle > 90.0 + 1e-9 || angle <= -90.0 + 1e-9 {
        (if angle > 0.0 { angle - 180.0 } else { angle + 180.0 }, scaled(along, -1.0))
    } else {
        (angle, along)
    }
}

/// 計測値の文字（小数点以下 0 桁）
fn number(value: f64) -> String {
    let text = format!("{value:.0}");
    if text == "-0" { "0".to_string() } else { text }
}

fn arrow(at: [f64; 2]) -> Option<Arrow> {
    Some(Arrow { code: FILLED_ARROW, direction: 1.0, x: at[0], y: at[1], scale: ARROW_LENGTH / 7.5 })
}

/// 寸法補助線。起点 origin から out の向きに、離れの分だけ空けて始め、寸法線（起点から along の長さ）を越える
fn extension(origin: [f64; 2], out: [f64; 2], along: f64, container: &Container) -> ExtensionLine {
    let start = add(origin, scaled(out, container.length(EXTENSION_OFFSET)));
    let end = add(origin, scaled(out, along + container.length(EXTENSION_EXTENSION)));
    ExtensionLine { x0: origin[0], y0: origin[1], x1: start[0], y1: start[1], x2: end[0], y2: end[1] }
}

/// 寸法値の文字。読む向き reading の上へ、文字と寸法線の間だけ離した点を中下の基点にする
fn annotation(
    text: String,
    at: [f64; 2],
    reading: [f64; 2],
    angle: f64,
    container: &Container,
    width: fn(&str, f64) -> f64,
) -> Annotation {
    let up = [-reading[1], reading[0]];
    let position = add(at, scaled(up, container.length(TEXT_GAP)));
    let height = container.length(TEXT_HEIGHT);
    Annotation {
        font: 0.0,
        width: width(&text, height),
        text,
        x: position[0],
        y: position[1],
        height,
        spacing: 0.0,
        angle,
        slant: 0.0,
        anchor: BOTTOM_CENTER,
        direction: 1.0,
    }
}

/// 寸法のフィーチャ。geometry は入れ物の座標、text は <> を計測値にする文字、width は文字範囲の幅の見積もり
pub(super) fn feature(
    geometry: &DimensionGeometry,
    text: &str,
    style: Styled,
    container: &Container,
    width: fn(&str, f64) -> f64,
) -> SxfFeature {
    match *geometry {
        DimensionGeometry::Linear { origins, feet, direction, measurement } => {
            let [first, second] = feet;
            let extensions = [0, 1].map(|i| {
                let to = sub(feet[i], origins[i]);
                unit(to).map(|out| extension(origins[i], out, to[0].hypot(to[1]), container))
            });
            let along = unit(sub(second, first)).unwrap_or(direction);
            let (angle, reading) = reading(along);
            let middle = scaled(add(first, second), 0.5);
            let shown = text.replace("<>", &number(measurement));
            let [ext1, ext2] = extensions;
            SxfFeature::LinearDim {
                style,
                line: [first[0], first[1], second[0], second[1]],
                ext1,
                ext2,
                arrow1: arrow(first),
                arrow2: arrow(second),
                text: Some(annotation(shown, middle, reading, angle, container, width)),
            }
        }
        DimensionGeometry::Radial { from, to, measurement, diameter, .. } => {
            let along = unit(sub(to, from)).unwrap_or([1.0, 0.0]);
            let (angle, reading) = reading(along);
            let middle = scaled(add(from, to), 0.5);
            let mark = if diameter { "φ" } else { "R" };
            let shown = text.replace("<>", &format!("{mark}{}", number(measurement)));
            let text = Some(annotation(shown, middle, reading, angle, container, width));
            let line = [from[0], from[1], to[0], to[1]];
            if diameter {
                SxfFeature::DiameterDim { style, line, arrow1: arrow(from), arrow2: arrow(to), text }
            } else {
                SxfFeature::RadiusDim { style, line, arrow: arrow(to), text }
            }
        }
        DimensionGeometry::Angular { center, radius, from, span, origins } => {
            let to = from + span;
            let at = |angle: f64| add(center, [radius * angle.cos(), radius * angle.sin()]);
            // 寸法補助線は、弧の内側の起点からだけ（起点が弧の外なら、起点までの線が弧に届いている）
            let extensions = [0, 1].map(|i| {
                let out = unit(sub(origins[i], center))?;
                let distance = (origins[i][0] - center[0]).hypot(origins[i][1] - center[1]);
                let begin = distance + container.length(EXTENSION_OFFSET);
                let finish = radius + container.length(EXTENSION_EXTENSION);
                (begin < finish).then(|| {
                    let start = add(center, scaled(out, begin));
                    let end = add(center, scaled(out, finish));
                    ExtensionLine {
                        x0: origins[i][0],
                        y0: origins[i][1],
                        x1: start[0],
                        y1: start[1],
                        x2: end[0],
                        y2: end[1],
                    }
                })
            });
            let middle = from + span / 2.0;
            let (angle, reading) = reading([-middle.sin(), middle.cos()]);
            let shown = text.replace("<>", &format!("{}°", number(span.to_degrees())));
            let [ext1, ext2] = extensions;
            SxfFeature::AngularDim(ArcDimension {
                style,
                cx: center[0],
                cy: center[1],
                radius,
                angle0: from.to_degrees().rem_euclid(360.0),
                angle1: to.to_degrees().rem_euclid(360.0),
                ext1,
                ext2,
                arrow1: arrow(at(from)),
                arrow2: arrow(at(to)),
                text: Some(annotation(shown, at(middle), reading, angle, container, width)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Styled {
        Styled { id: -1.0, layer: 0.0, color: 1.0, line_type: 1.0, line_width: 3.0 }
    }

    fn width(text: &str, height: f64) -> f64 {
        text.chars().count() as f64 * height / 2.0
    }

    #[test]
    fn writes_linear_dimensions_with_extension_lines_arrows_and_text() {
        let geometry = DimensionGeometry::Linear {
            origins: [[0.0, 0.0], [5000.0, 300.0]],
            feet: [[0.0, -800.0], [5000.0, -800.0]],
            direction: [1.0, 0.0],
            measurement: 5000.0,
        };
        // 1/100 の部分図: 2.5 mm の文字は 250
        let SxfFeature::LinearDim { line, ext1, ext2, arrow1, text, .. } =
            feature(&geometry, "<>", style(), &Container { scale: 0.01 }, width)
        else {
            panic!()
        };
        assert_eq!(line, [0.0, -800.0, 5000.0, -800.0]);
        let ext1 = ext1.unwrap();
        assert_eq!((ext1.x0, ext1.y0, ext1.x1, ext1.y1, ext1.x2, ext1.y2), (0.0, 0.0, 0.0, -62.5, 0.0, -925.0));
        assert_eq!(ext2.unwrap().y1, 300.0 - 62.5);
        let arrow1 = arrow1.unwrap();
        assert_eq!((arrow1.code, arrow1.direction, arrow1.x, arrow1.y), (6.0, 1.0, 0.0, -800.0));
        assert!((arrow1.scale - 1.0 / 3.0).abs() < 1e-12);
        let text = text.unwrap();
        assert_eq!(
            (text.text.as_str(), text.x, text.y, text.height, text.anchor),
            ("5000", 2500.0, -737.5, 250.0, 2.0)
        );
    }

    #[test]
    fn writes_angular_and_radial_dimensions() {
        let geometry = DimensionGeometry::Angular {
            center: [0.0, 0.0],
            radius: 1000.0,
            from: 0.0,
            span: std::f64::consts::FRAC_PI_2,
            origins: [[500.0, 0.0], [0.0, 2000.0]],
        };
        let SxfFeature::AngularDim(dimension) = feature(&geometry, "<>", style(), &Container { scale: 1.0 }, width)
        else {
            panic!()
        };
        assert_eq!((dimension.angle0, dimension.angle1, dimension.radius), (0.0, 90.0, 1000.0));
        // 起点が弧の内側の寸法補助線だけ
        assert!(dimension.ext1.is_some() && dimension.ext2.is_none());
        assert_eq!(dimension.text.unwrap().text, "90°");
        let radius = DimensionGeometry::Radial {
            center: [0.0, 0.0],
            from: [0.0, 0.0],
            to: [600.0, 0.0],
            measurement: 600.0,
            diameter: false,
        };
        let SxfFeature::RadiusDim { arrow, text, .. } =
            feature(&radius, "<>", style(), &Container { scale: 1.0 }, width)
        else {
            panic!()
        };
        assert_eq!((arrow.unwrap().x, text.unwrap().text.as_str()), (600.0, "R600"));
    }
}
