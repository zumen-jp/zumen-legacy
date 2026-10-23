//! 足す寸法を SXF の寸法のフィーチャにする（docs/decisions/0025）。回転・平行寸法は直線寸法（linear_dim_feature）、
//! 3 点角度寸法は角度寸法（angular_dim_feature）、半径・直径寸法は半径寸法・直径寸法（radius_dim_feature・
//! diameter_dim_feature）。SXF は寸法スタイルを持たないので、大きさは用紙の mm で決めた値（ISO-25 と同じ: 文字の
//! 高さ 2.5、矢印 2.5、文字と寸法線の間 0.625、寸法補助線の起点からの離れ 0.625、寸法線を越える長さ 1.25）を、
//! 入れ物の座標（用紙なら mm、部分図ならその配置の尺度で割った長さ）にする。矢印の大きさは配置の尺度によらない
//! （SXF 実装規約 1-4）。
//!
//! この編集で足した寸法スタイル（add-dimension-style）は、その値で描く（docs/decisions/0026）。スタイルは編集の中
//! だけにあり、大きさと書き方を寸法のフィーチャごとに書く。尺度（scale）のあるスタイルの長さは、尺度を掛けた入れ物の
//! 座標（DXF の DIMSCALE と同じ）。

use crate::features::{Annotation, ArcDimension, Arrow, ExtensionLine, Styled, SxfFeature};
use std::f64::consts::SQRT_2;
use zumen_core::edit::dimension::DimensionGeometry;
use zumen_core::edit::style::DimensionStyleInput;

/// 矢印（塗りつぶしの矢印、コード 6）と斜線（コード 10）。附属書 SFC 編 図 19 の大きさは、倍率 1 で矢印が長さ
/// 7.5 mm・幅 2.5 mm、斜線が長さ 2.5 mm（寸法線に 45°）
const FILLED_ARROW: f64 = 6.0;
const ARROW_LENGTH: f64 = 7.5;
const SLASH: f64 = 10.0;
const SLASH_LENGTH: f64 = 2.5;
/// 寸法値の文字の配置基点（中下）
const BOTTOM_CENTER: f64 = 2.0;

/// 寸法の大きさと寸法値の書き方。長さは用紙の mm（scale があれば、それを掛けた入れ物の座標）
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Style {
    /// 長さに掛ける尺度（DIMSCALE）。None は用紙の mm を入れ物の配置の尺度で割る
    scale: Option<f64>,
    text_height: f64,
    arrow_size: f64,
    extension_offset: f64,
    extension_extension: f64,
    text_gap: f64,
    text_above: bool,
    decimals: usize,
    suppress_trailing_zeros: bool,
    /// 0 より大きければ矢印の代わりの斜線の大きさ（寸法線の向きの長さ。DXF の DIMTSZ と同じ）
    tick_size: f64,
    linear_factor: f64,
    /// 寸法値の文字フォントのコード（0 はなし）
    font: f64,
}

impl Default for Style {
    /// AutoCAD のメートル（ISO-25）の値（docs/decisions/0025）。小数点以下は 0 桁
    fn default() -> Self {
        Self {
            scale: None,
            text_height: 2.5,
            arrow_size: 2.5,
            extension_offset: 0.625,
            extension_extension: 1.25,
            text_gap: 0.625,
            text_above: true,
            decimals: 0,
            suppress_trailing_zeros: true,
            tick_size: 0.0,
            linear_factor: 1.0,
            font: 0.0,
        }
    }
}

impl Style {
    /// 寸法スタイルの値を重ねる。font は textStyle の文字フォントのコード、倍率は絶対値
    pub(super) fn apply(&mut self, input: &DimensionStyleInput, font: Option<f64>) {
        if let Some(scale) = input.scale {
            self.scale = (scale > 0.0).then_some(scale);
        }
        let lengths = [
            (input.text_height, &mut self.text_height),
            (input.arrow_size, &mut self.arrow_size),
            (input.extension_offset, &mut self.extension_offset),
            (input.extension_extension, &mut self.extension_extension),
            (input.text_gap, &mut self.text_gap),
            (input.tick_size, &mut self.tick_size),
        ];
        for (value, slot) in lengths {
            if let Some(value) = value {
                *slot = value;
            }
        }
        if let Some(above) = input.text_above {
            self.text_above = above;
        }
        if let Some(decimals) = input.decimals {
            self.decimals = decimals as usize;
        }
        if let Some(suppress) = input.suppress_trailing_zeros {
            self.suppress_trailing_zeros = suppress;
        }
        if let Some(factor) = input.linear_factor {
            self.linear_factor = factor.abs();
        }
        if let Some(font) = font {
            self.font = font;
        }
    }

    /// 計測値の文字（倍率を掛け、小数点以下の桁に丸め、後ろの 0 を省く）
    fn measured(&self, value: f64) -> String {
        number(value * self.linear_factor, self.decimals, self.suppress_trailing_zeros)
    }
}

/// 寸法を描く入れ物の座標の 1 が、用紙の何 mm か
#[derive(Debug, Clone, Copy)]
pub(super) struct Container {
    pub(super) scale: f64,
}

/// 寸法の長さを入れ物の座標と用紙の mm にする
struct Lengths<'a> {
    style: &'a Style,
    container: Container,
}

impl Lengths<'_> {
    /// スタイルの長さを入れ物の座標に（尺度があれば掛け、なければ配置の尺度で割る）
    fn length(&self, value: f64) -> f64 {
        match self.style.scale {
            Some(scale) => value * scale,
            None => value / self.container.scale,
        }
    }

    /// スタイルの長さを用紙の mm に（矢印は配置の尺度によらず用紙の大きさで描かれる）
    fn sheet(&self, value: f64) -> f64 {
        match self.style.scale {
            Some(scale) => value * scale * self.container.scale,
            None => value,
        }
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

/// 数の文字（小数点以下 decimals 桁、trim なら後ろの 0 を省く。-0 は 0）
fn number(value: f64, decimals: usize, trim: bool) -> String {
    let mut text = format!("{value:.decimals$}");
    if trim && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if text.starts_with('-') && text[1..].chars().all(|c| c == '0' || c == '.') {
        text.remove(0);
    }
    text
}

/// 寸法線の端の矢印（塗りつぶしの矢印、外向き）か、斜線（スタイルの tickSize が 0 より大きいとき）。矢印の長さが
/// 0 なら描かない
fn arrow(at: [f64; 2], lengths: &Lengths<'_>) -> Option<Arrow> {
    let (code, scale) = if lengths.style.tick_size > 0.0 {
        // DXF の斜線（DIMTSZ）は寸法線の向きに DIMTSZ、45° に傾けて DIMTSZ × √2 の長さ
        (SLASH, lengths.sheet(lengths.style.tick_size) * SQRT_2 / SLASH_LENGTH)
    } else {
        (FILLED_ARROW, lengths.sheet(lengths.style.arrow_size) / ARROW_LENGTH)
    };
    (scale > 0.0).then_some(Arrow { code, direction: 1.0, x: at[0], y: at[1], scale })
}

/// 寸法補助線。起点 origin から out の向きに、離れの分だけ空けて始め、寸法線（起点から along の長さ）を越える
fn extension(origin: [f64; 2], out: [f64; 2], along: f64, lengths: &Lengths<'_>) -> ExtensionLine {
    let start = add(origin, scaled(out, lengths.length(lengths.style.extension_offset)));
    let end = add(origin, scaled(out, along + lengths.length(lengths.style.extension_extension)));
    ExtensionLine { x0: origin[0], y0: origin[1], x1: start[0], y1: start[1], x2: end[0], y2: end[1] }
}

/// 寸法値の文字。読む向き reading の上へ、文字と寸法線の間だけ離した点を中下の基点にする。寸法線の上に置かない
/// スタイルは、寸法線を文字の高さの中央に通す（寸法のフィーチャの寸法線は 1 本で、文字で切れない）
fn annotation(
    text: String,
    at: [f64; 2],
    reading: [f64; 2],
    angle: f64,
    lengths: &Lengths<'_>,
    width: fn(&str, f64) -> f64,
) -> Annotation {
    let up = [-reading[1], reading[0]];
    let height = lengths.length(lengths.style.text_height);
    let gap = if lengths.style.text_above { lengths.length(lengths.style.text_gap) } else { -height / 2.0 };
    let position = add(at, scaled(up, gap));
    Annotation {
        font: lengths.style.font,
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

/// 寸法のフィーチャ。geometry は入れ物の座標、text は <> を計測値にする文字、dimension は大きさと書き方、width は
/// 文字範囲の幅の見積もり
pub(super) fn feature(
    geometry: &DimensionGeometry,
    text: &str,
    style: Styled,
    dimension: &Style,
    container: Container,
    width: fn(&str, f64) -> f64,
) -> SxfFeature {
    let lengths = &Lengths { style: dimension, container };
    match *geometry {
        DimensionGeometry::Linear { origins, feet, direction, measurement } => {
            let [first, second] = feet;
            let extensions = [0, 1].map(|i| {
                let to = sub(feet[i], origins[i]);
                unit(to).map(|out| extension(origins[i], out, to[0].hypot(to[1]), lengths))
            });
            let along = unit(sub(second, first)).unwrap_or(direction);
            let (angle, reading) = reading(along);
            let middle = scaled(add(first, second), 0.5);
            let shown = text.replace("<>", &dimension.measured(measurement));
            let [ext1, ext2] = extensions;
            SxfFeature::LinearDim {
                style,
                line: [first[0], first[1], second[0], second[1]],
                ext1,
                ext2,
                arrow1: arrow(first, lengths),
                arrow2: arrow(second, lengths),
                text: Some(annotation(shown, middle, reading, angle, lengths, width)),
            }
        }
        DimensionGeometry::Radial { from, to, measurement, diameter, .. } => {
            let along = unit(sub(to, from)).unwrap_or([1.0, 0.0]);
            let (angle, reading) = reading(along);
            let middle = scaled(add(from, to), 0.5);
            let mark = if diameter { "φ" } else { "R" };
            let shown = text.replace("<>", &format!("{mark}{}", dimension.measured(measurement)));
            let text = Some(annotation(shown, middle, reading, angle, lengths, width));
            let line = [from[0], from[1], to[0], to[1]];
            if diameter {
                SxfFeature::DiameterDim { style, line, arrow1: arrow(from, lengths), arrow2: arrow(to, lengths), text }
            } else {
                SxfFeature::RadiusDim { style, line, arrow: arrow(to, lengths), text }
            }
        }
        DimensionGeometry::Angular { center, radius, from, span, origins } => {
            let to = from + span;
            let at = |angle: f64| add(center, [radius * angle.cos(), radius * angle.sin()]);
            // 寸法補助線は、弧の内側の起点からだけ（起点が弧の外なら、起点までの線が弧に届いている）
            let extensions = [0, 1].map(|i| {
                let out = unit(sub(origins[i], center))?;
                let distance = (origins[i][0] - center[0]).hypot(origins[i][1] - center[1]);
                let begin = distance + lengths.length(dimension.extension_offset);
                let finish = radius + lengths.length(dimension.extension_extension);
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
            // 角度は度の整数（DIMADEC・DIMAZIN は寸法スタイルのキーにない。倍率も角度には掛けない）
            let shown = text.replace("<>", &format!("{}°", number(span.to_degrees(), 0, true)));
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
                arrow1: arrow(at(from), lengths),
                arrow2: arrow(at(to), lengths),
                text: Some(annotation(shown, at(middle), reading, angle, lengths, width)),
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
            feature(&geometry, "<>", style(), &Style::default(), Container { scale: 0.01 }, width)
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
        let SxfFeature::AngularDim(dimension) =
            feature(&geometry, "<>", style(), &Style::default(), Container { scale: 1.0 }, width)
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
            feature(&radius, "<>", style(), &Style::default(), Container { scale: 1.0 }, width)
        else {
            panic!()
        };
        assert_eq!((arrow.unwrap().x, text.unwrap().text.as_str()), (600.0, "R600"));
    }
}
