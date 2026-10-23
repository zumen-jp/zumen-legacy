//! 足す寸法を JWW の寸法図形（CDataSunpou）にする（docs/decisions/0025）。寸法線・寸法値・補助線・端点の色と大きさ、
//! 寸法値の書き方は、図面の寸法の設定（ヘッダの m_lnSunpou1〜5、jwdatafmt.txt「寸法関係の設定」）と文字種に従う。
//! 設定を保存していない図面（すべて 0）は Jw_cad の既定の設定。大きさと座標は用紙の mm。
//!
//! この編集で足した寸法スタイル（add-dimension-style）の値は、図面の設定に重ねる（docs/decisions/0026）。JWW は
//! 名前の付いた寸法スタイルを持たないので、スタイルは編集の中だけにあり、大きさと書き方を寸法図形ごとに書く。

use crate::create::{DIMENSION_SETTINGS, TEXT_STYLES};
use crate::header::JwwSettings;
use crate::reader::{JwwBase, JwwDimension, JwwLine, JwwPoint, JwwText};
use zumen_core::edit::dimension::DimensionGeometry;
use zumen_core::edit::style::DimensionStyleInput;

/// 寸法図形と寸法線・補助線の属性フラグ（jwdatafmt.txt「属性フラグ」: 寸法・線の 0x2000 は寸法）
const DIMENSION_FLAG: u16 = 0x2000;
/// 寸法値の属性フラグ（文字の 0x4000 は寸法、0x0010 は寸法値。半径は 0x0100、直径は 0x0200 も）
const VALUE_FLAG: u16 = 0x4010;
const RADIUS_VALUE_FLAG: u16 = 0x0100;
const DIAMETER_VALUE_FLAG: u16 = 0x0200;
/// 寸法の端点の属性フラグ（実点の 0x0040 は寸法）
const POINT_FLAG: u16 = 0x0040;
/// 寸法値の文字の基点（中下）。Jw_cad が寸法値に書く値
const VALUE_ANCHOR: u8 = 2;

/// 寸法値の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Value {
    Length,
    Radius,
    Diameter,
}

/// 寸法の見た目と寸法値の書き方。図面の寸法の設定（`read`）に、この編集で足した寸法スタイルの値を重ねたもの（`apply`）
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Settings {
    /// 寸法線・引出線（補助線）・寸法点の線色
    pub(super) line_color: u16,
    pub(super) extension_color: u16,
    pub(super) point_color: u16,
    /// 寸法値の小数点以下の桁数（0〜3）
    decimals: u32,
    /// 寸法の単位が m（そうでなければ mm）
    meters: bool,
    /// 寸法線の端部（0: 点、1: 矢印、2: 逆矢印）
    pub(super) end: u32,
    /// 寸法値の文字種（1〜10）の幅・高さ・間隔と色
    text_style: u32,
    text_size: (f64, f64, f64),
    text_color: u16,
    /// 寸法値の寸法線との離れと、引出線の突出寸法（mm）
    gap: f64,
    overshoot: f64,
    /// 寸法値の向きを読める向きに補正しない
    fixed_direction: bool,
    full_width_digits: bool,
    comma_space: bool,
    comma_full_width: bool,
    point_full_width: bool,
    unit_suffix: bool,
    /// 半径・直径の R・φ（0: 付けない、1: 前、2: 後）
    radius_mark: u32,
    radius_comma: bool,
    radius_zeros: bool,
    /// 小数点以下の処理（0: 四捨五入、1: 切捨、2: 切上）
    rounding: u32,
    /// 長さ（寸法値の大きさ・離れ・突出・補助線の起点からの離れ）に掛ける尺度（寸法スタイルの scale）。None は
    /// 用紙の mm のまま（docs/decisions/0026）
    pub(super) scale: Option<f64>,
    /// 補助線を起点から離す長さ（mm）
    extension_offset: f64,
    /// 寸法値を寸法線の上に置く（false は寸法線の中央）
    text_above: bool,
    /// 計測値に掛ける倍率
    linear_factor: f64,
    /// 小数点以下の後ろの 0 を残すか。None は Jw_cad の設定のまま（半径値だけ、設定に従って残す）
    zeros: Option<bool>,
    /// 寸法値のフォント
    font: String,
    /// 設定の文字種の幅・高さ・間隔（寸法値がこの大きさなら文字種の番号を書き、違えば任意サイズ）
    style_size: (f64, f64, f64),
}

/// value の place の位から count 桁
fn digits(value: u32, place: u32, count: u32) -> u32 {
    (value / 10u32.pow(place)) % 10u32.pow(count)
}

/// 0〜99.9 を 10 倍した 3 桁と、千の位の負の印（jwdatafmt.txt「マイナスの場合は1000をプラスして整数化」）
fn signed_tenths(value: u32) -> f64 {
    if value >= 1000 { -f64::from(value - 1000) / 10.0 } else { f64::from(value) / 10.0 }
}

/// 線色 1〜9（ほかは fallback）
fn pen(value: u32, fallback: u16) -> u16 {
    if (1..=9).contains(&value) { value as u16 } else { fallback }
}

impl Settings {
    /// ヘッダの寸法の設定と文字種。設定がすべて 0（環境設定で保存していない図面）なら Jw_cad の既定
    pub(super) fn read(settings: Option<&JwwSettings>) -> Self {
        let stored =
            settings.map(|settings| settings.general.dimension).filter(|values| values.iter().any(|v| *v != 0));
        let [first, second, _, fourth, fifth] = stored.unwrap_or(DIMENSION_SETTINGS);
        let style = match digits(first, 6, 1) {
            style @ 1..=9 => style,
            _ => 2,
        };
        let (width, height, spacing, color) = settings
            .and_then(|settings| settings.text.as_ref())
            .map(|text| text.styles[style as usize - 1])
            .map(|style| (style.width, style.height, style.spacing, style.color))
            .filter(|(width, height, spacing, _)| *width > 0.0 && *height > 0.0 && *spacing >= 0.0)
            .unwrap_or(TEXT_STYLES[style as usize - 1]);
        let line_color = pen(digits(first, 0, 1), 1);
        let text_size = (width, height, spacing);
        Self {
            line_color,
            extension_color: pen(digits(first, 1, 1), line_color),
            point_color: pen(digits(first, 2, 1), line_color),
            decimals: digits(first, 3, 1).min(3),
            meters: digits(first, 4, 1) == 1,
            end: digits(first, 5, 1),
            text_style: style,
            text_size,
            text_color: pen(color, line_color),
            gap: signed_tenths(digits(second, 0, 4)),
            overshoot: signed_tenths(digits(second, 4, 4)),
            fixed_direction: digits(fourth, 0, 1) == 1,
            full_width_digits: digits(fourth, 1, 1) == 1,
            comma_space: digits(fourth, 2, 1) == 1,
            comma_full_width: digits(fourth, 3, 1) == 1,
            point_full_width: digits(fourth, 4, 1) == 1,
            unit_suffix: digits(fourth, 5, 1) == 1,
            radius_mark: digits(fourth, 6, 1).min(2),
            radius_comma: digits(fourth, 7, 1) == 1,
            radius_zeros: digits(fourth, 8, 1) == 1,
            rounding: digits(fifth, 6, 1).min(2),
            scale: None,
            extension_offset: 0.0,
            text_above: true,
            linear_factor: 1.0,
            zeros: None,
            font: super::DEFAULT_FONT.to_string(),
            style_size: text_size,
        }
    }

    /// 寸法スタイルの値を重ねる（docs/decisions/0026）。font は textStyle の文字スタイルのフォント。寸法値の大きさは
    /// 文字種の幅・高さ・間隔の比を保つ。小数点以下の桁数は寸法値設定のフラグに書ける 3 まで、倍率は絶対値
    pub(super) fn apply(&mut self, input: &DimensionStyleInput, font: Option<String>) {
        if let Some(scale) = input.scale {
            self.scale = (scale > 0.0).then_some(scale);
        }
        if let Some(height) = input.text_height {
            let (width, old, spacing) = self.text_size;
            let ratio = if old > 0.0 { height / old } else { 1.0 };
            self.text_size = (width * ratio, height, spacing * ratio);
        }
        if let Some(gap) = input.text_gap {
            self.gap = gap;
        }
        if let Some(overshoot) = input.extension_extension {
            self.overshoot = overshoot;
        }
        if let Some(offset) = input.extension_offset {
            self.extension_offset = offset;
        }
        if let Some(above) = input.text_above {
            self.text_above = above;
        }
        if let Some(decimals) = input.decimals {
            self.decimals = decimals.min(3);
        }
        if let Some(suppress) = input.suppress_trailing_zeros {
            self.zeros = Some(!suppress);
        }
        if let Some(factor) = input.linear_factor {
            self.linear_factor = factor.abs();
        }
        if let Some(font) = font {
            self.font = font;
        }
    }

    /// 寸法値の小数点以下の後ろの 0 を残すか
    fn keeps_zeros(&self, value: Value) -> bool {
        self.zeros.unwrap_or(value == Value::Radius && self.radius_zeros)
    }

    /// 寸法値の文字。測った長さ（実寸の mm）に倍率を掛け、設定の単位・桁・端数の処理・桁区切りで書き、半径・直径は
    /// R・φ を付ける
    pub(super) fn format(&self, measurement: f64, value: Value) -> String {
        let measurement = measurement * self.linear_factor;
        let length = if self.meters { measurement / 1000.0 } else { measurement };
        let scale = 10f64.powi(self.decimals as i32);
        let scaled = length * scale;
        // 2 進の誤差で端数の処理が変わらないよう、わずかにずらす
        let rounded = match self.rounding {
            1 => (scaled + 1e-6).floor(),
            2 => (scaled - 1e-6).ceil(),
            _ => (scaled + 1e-6).round(),
        } / scale;
        let mut text = format!("{rounded:.*}", self.decimals as usize);
        if !self.keeps_zeros(value) && text.contains('.') {
            text = text.trim_end_matches('0').trim_end_matches('.').to_string();
        }
        let (integer, fraction) = match text.split_once('.') {
            Some((integer, fraction)) => (integer.to_string(), Some(fraction.to_string())),
            None => (text, None),
        };
        let comma = value != Value::Radius || self.radius_comma;
        let separator = if self.comma_space {
            " "
        } else if self.comma_full_width {
            "，"
        } else {
            ","
        };
        let mut out = String::new();
        let count = integer.chars().count();
        for (position, c) in integer.chars().enumerate() {
            if comma && position > 0 && (count - position) % 3 == 0 && c.is_ascii_digit() {
                out.push_str(separator);
            }
            out.push(c);
        }
        if let Some(fraction) = fraction {
            out.push_str(if self.point_full_width { "．" } else { "." });
            out.push_str(&fraction);
        }
        if self.full_width_digits {
            out = out
                .chars()
                .map(|c| if c.is_ascii_digit() { char::from_u32(c as u32 - 0x30 + 0xff10).unwrap_or(c) } else { c })
                .collect();
        }
        if self.unit_suffix {
            out.push_str(if self.meters { "ｍ" } else { "ｍｍ" });
        }
        let mark = match value {
            Value::Length => return out,
            Value::Radius => "R",
            Value::Diameter => "φ",
        };
        match self.radius_mark {
            1 => format!("{mark}{out}"),
            2 => format!("{out}{mark}"),
            _ => out,
        }
    }

    /// 寸法値の文字の寸法値設定のフラグ（jwdatafmt.txt「寸法値設定のフラグ」。文字の線幅の欄に書く）
    fn value_flags(&self, value: Value) -> u16 {
        let mut flags = 0x0002 | (self.decimals as u16) << 12;
        if self.full_width_digits {
            flags |= 0x0008;
        }
        if self.meters {
            flags |= 0x0010;
        }
        if self.unit_suffix {
            flags |= 0x0020;
        }
        if value != Value::Radius || self.radius_comma {
            flags |= 0x0040;
        }
        if self.keeps_zeros(value) {
            flags |= 0x0080;
        }
        flags |= match self.rounding {
            1 => 0x0100,
            2 => 0x0200,
            _ => 0,
        };
        if value != Value::Length {
            flags |= match self.radius_mark {
                1 => 0x0400,
                2 => 0x0800,
                _ => 0,
            };
        }
        flags
    }
}

/// 寸法図形の部品の属性。layer・color・line_type・width は操作の値（なければ設定の色、線種 1、線幅 0）
#[derive(Debug, Clone, Copy)]
pub(super) struct Look {
    pub(super) layer: (u16, u16),
    pub(super) color: Option<u16>,
    pub(super) line_type: Option<u8>,
    pub(super) width: Option<u16>,
}

impl Look {
    fn base(&self, color: u16, flag: u16) -> JwwBase {
        JwwBase {
            group: 0,
            pen_style: self.line_type.unwrap_or(1),
            pen_color: self.color.unwrap_or(color),
            pen_width: self.width.unwrap_or(0),
            layer: self.layer.1,
            layer_group: self.layer.0,
            flag,
        }
    }
}

fn line(base: JwwBase, a: [f64; 2], b: [f64; 2]) -> JwwLine {
    JwwLine { base, x1: a[0], y1: a[1], x2: b[0], y2: b[1] }
}

fn point(base: JwwBase, at: [f64; 2], temporary: bool) -> JwwPoint {
    JwwPoint { base: JwwBase { pen_style: 1, ..base }, x: at[0], y: at[1], temporary, code: 0, angle: 0.0, scale: 0.0 }
}

fn unit(v: [f64; 2]) -> Option<[f64; 2]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-12).then(|| [v[0] / length, v[1] / length])
}

/// 寸法図形を作る。geometry は用紙の mm の形（measurement だけは実寸）、text は <> を寸法値にする文字、factor は
/// 寸法の長さ（文字の大きさ・離れ・突出・補助線の起点からの離れ）の 1 が用紙の何 mm か（尺度のないスタイルは 1）。
/// 角度寸法は寸法図形にできないので呼ばない
pub(super) fn dimension(
    settings: &Settings,
    geometry: &DimensionGeometry,
    measurement: f64,
    text: &str,
    look: &Look,
    factor: f64,
    text_width: fn(&mut JwwText),
) -> JwwDimension {
    let (first, second, origins, value, fallback) = match *geometry {
        DimensionGeometry::Linear { origins, feet, direction, .. } => {
            (feet[0], feet[1], Some(origins), Value::Length, direction)
        }
        DimensionGeometry::Radial { from, to, diameter, .. } => {
            (from, to, None, if diameter { Value::Diameter } else { Value::Radius }, [1.0, 0.0])
        }
        DimensionGeometry::Angular { .. } => unreachable!("角度寸法は寸法図形にできない"),
    };
    let line_base = look.base(settings.line_color, DIMENSION_FLAG);
    let extension_base = look.base(settings.extension_color, DIMENSION_FLAG);
    let point_base = JwwBase { pen_width: 0, ..look.base(settings.point_color, POINT_FLAG) };
    // 補助線: 起点から（スタイルの離れだけ空けて）寸法線まで、突出寸法だけ越える。起点が寸法線の上なら長さ 0（描かない）
    let (overshoot, offset) = (settings.overshoot * factor, settings.extension_offset * factor);
    let mut lines = Vec::new();
    let mut points = Vec::new();
    if let Some(origins) = origins {
        for (origin, foot) in origins.iter().zip([first, second]) {
            let (start, end) = match unit([foot[0] - origin[0], foot[1] - origin[1]]) {
                Some(out) => (
                    if offset == 0.0 { *origin } else { [origin[0] + out[0] * offset, origin[1] + out[1] * offset] },
                    [foot[0] + out[0] * overshoot, foot[1] + out[1] * overshoot],
                ),
                None => (foot, foot),
            };
            lines.push(line(extension_base, start, end));
        }
    } else {
        lines.push(line(extension_base, first, first));
        lines.push(line(extension_base, second, second));
    }
    // 矢印（点）1・2: 寸法線の端の実点（半径寸法の中心の側は仮点）。基準点 1・2: 寸法補助線の起点の仮点
    points.push(point(point_base, first, value == Value::Radius));
    points.push(point(point_base, second, false));
    let references = origins.unwrap_or([first, second]);
    points.push(point(point_base, references[0], true));
    points.push(point(point_base, references[1], true));
    // 寸法値: 寸法線の中央に、読める向き（設定で補正しないなら寸法線の向き）で、寸法線から離れの分だけ上に置く。
    // 寸法線の上に置かないスタイルは、寸法線を文字の高さの中央に通す（寸法図形の寸法線は 1 本で、文字で切れない）
    let along = unit([second[0] - first[0], second[1] - first[1]]).unwrap_or(fallback);
    let mut angle = along[1].atan2(along[0]).to_degrees();
    if !settings.fixed_direction && (angle > 90.0 + 1e-9 || angle <= -90.0 + 1e-9) {
        angle += if angle > 0.0 { -180.0 } else { 180.0 };
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let content = text.replace("<>", &settings.format(measurement, value));
    let (size_x, size_y, spacing) =
        (settings.text_size.0 * factor, settings.text_size.1 * factor, settings.text_size.2 * factor);
    let (style_x, style_y, style_spacing) = settings.style_size;
    let same = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b.abs().max(1.0);
    // 文字種の大きさのままなら文字種の番号、違えば任意サイズ（0）
    let text_type = if same(size_x, style_x) && same(size_y, style_y) && same(spacing, style_spacing) {
        settings.text_style
    } else {
        0
    };
    let flag = VALUE_FLAG
        | match value {
            Value::Length => 0,
            Value::Radius => RADIUS_VALUE_FLAG,
            Value::Diameter => DIAMETER_VALUE_FLAG,
        };
    let text_base = JwwBase {
        pen_style: VALUE_ANCHOR,
        pen_width: settings.value_flags(value),
        ..look.base(settings.text_color, flag)
    };
    let mut value_text = JwwText {
        base: text_base,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        text_type,
        size_x,
        size_y,
        spacing,
        angle,
        font: settings.font.clone(),
        content,
    };
    text_width(&mut value_text);
    let length = value_text.x2.hypot(value_text.y2);
    let middle = [(first[0] + second[0]) / 2.0, (first[1] + second[1]) / 2.0];
    let gap = if settings.text_above { settings.gap * factor } else { -size_y / 2.0 };
    value_text.x1 = middle[0] - cos * length / 2.0 - sin * gap;
    value_text.y1 = middle[1] - sin * length / 2.0 + cos * gap;
    text_width(&mut value_text);
    JwwDimension {
        base: JwwBase { pen_style: 1, pen_width: 0, ..line_base },
        line: line(line_base, first, second),
        text: value_text,
        sxf_mode: 1,
        lines,
        points,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(values: [u32; 5]) -> Settings {
        let mut general = crate::header::JwwHeader::new(700).settings.unwrap();
        general.general.dimension = values;
        Settings::read(Some(&general))
    }

    #[test]
    fn reads_the_dimension_settings_of_the_drawing() {
        let defaults = settings([0; 5]);
        assert_eq!(defaults, settings(DIMENSION_SETTINGS));
        assert_eq!(
            (defaults.text_style, defaults.text_size, defaults.gap, defaults.overshoot),
            (2, (2.5, 2.5, 0.0), 0.5, 0.0)
        );
        assert_eq!((defaults.decimals, defaults.end, defaults.radius_mark), (2, 0, 1));
        // 文字種 3、線色 8、離れ −1.5 mm、突出 2 mm、補正なし、R を後ろに
        let custom = settings([3_002_888, 201_015, 50_150_060, 12_000_001, 14_100]);
        assert_eq!((custom.line_color, custom.text_style, custom.text_size), (8, 3, (3.0, 3.0, 0.5)));
        assert_eq!((custom.gap, custom.overshoot, custom.fixed_direction, custom.radius_mark), (-1.5, 2.0, true, 2));
    }

    #[test]
    fn formats_values_like_jw_cad() {
        let defaults = settings([0; 5]);
        assert_eq!(defaults.format(3700.0, Value::Length), "3,700");
        assert_eq!(defaults.format(1234567.125, Value::Length), "1,234,567.13");
        assert_eq!(defaults.format(600.0, Value::Radius), "R600");
        assert_eq!(defaults.format(1200.5, Value::Diameter), "φ1,200.5");
        // m 単位・切捨・カンマを空白・全角の数字・単位を表示
        let meters = settings([2_012_111, 5, 50_150_030, 11_110_110, 1_004_100]);
        assert_eq!(meters.format(12345.678, Value::Length), "１２．３４ｍ");
        assert_eq!(meters.value_flags(Value::Length) & 0x0130, 0x0130);
    }
}
