//! 足す寸法を JWW の寸法図形（CDataSunpou）にする（docs/decisions/0025）。寸法線・寸法値・補助線・端点の色と大きさ、
//! 寸法値の書き方は、図面の寸法の設定（ヘッダの m_lnSunpou1〜5、jwdatafmt.txt「寸法関係の設定」）と文字種に従う。
//! 設定を保存していない図面（すべて 0）は Jw_cad の既定の設定。大きさと座標は用紙の mm。

use crate::create::{DIMENSION_SETTINGS, TEXT_STYLES};
use crate::header::JwwSettings;
use crate::reader::{JwwBase, JwwDimension, JwwLine, JwwPoint, JwwText};
use zumen_core::edit::dimension::DimensionGeometry;

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

/// 図面の寸法の設定
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
        let text_size = settings
            .and_then(|settings| settings.text.as_ref())
            .map(|text| text.styles[style as usize - 1])
            .map(|style| (style.width, style.height, style.spacing, style.color))
            .filter(|(width, height, spacing, _)| *width > 0.0 && *height > 0.0 && *spacing >= 0.0)
            .unwrap_or(TEXT_STYLES[style as usize - 1]);
        let line_color = pen(digits(first, 0, 1), 1);
        Self {
            line_color,
            extension_color: pen(digits(first, 1, 1), line_color),
            point_color: pen(digits(first, 2, 1), line_color),
            decimals: digits(first, 3, 1).min(3),
            meters: digits(first, 4, 1) == 1,
            end: digits(first, 5, 1),
            text_style: style,
            text_size: (text_size.0, text_size.1, text_size.2),
            text_color: pen(text_size.3, line_color),
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
        }
    }

    /// 寸法値の文字。測った長さ（実寸の mm）を設定の単位・桁・端数の処理・桁区切りで書き、半径・直径は R・φ を付ける
    pub(super) fn format(&self, measurement: f64, value: Value) -> String {
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
        let zeros = value == Value::Radius && self.radius_zeros;
        if !zeros && text.contains('.') {
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
        if value == Value::Radius && self.radius_zeros {
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

/// 寸法図形を作る。geometry は用紙の mm の形（measurement だけは実寸）、text は <> を寸法値にする文字。
/// 角度寸法は寸法図形にできないので呼ばない
pub(super) fn dimension(
    settings: &Settings,
    geometry: &DimensionGeometry,
    measurement: f64,
    text: &str,
    look: &Look,
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
    // 補助線: 起点から寸法線まで、突出寸法だけ越える。起点が寸法線の上なら長さ 0（描かない）
    let mut lines = Vec::new();
    let mut points = Vec::new();
    if let Some(origins) = origins {
        for (origin, foot) in origins.iter().zip([first, second]) {
            let end = match unit([foot[0] - origin[0], foot[1] - origin[1]]) {
                Some(out) => [foot[0] + out[0] * settings.overshoot, foot[1] + out[1] * settings.overshoot],
                None => foot,
            };
            lines.push(line(extension_base, *origin, end));
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
    // 寸法値: 寸法線の中央に、読める向き（設定で補正しないなら寸法線の向き）で、寸法線から離れの分だけ上に置く
    let along = unit([second[0] - first[0], second[1] - first[1]]).unwrap_or(fallback);
    let mut angle = along[1].atan2(along[0]).to_degrees();
    if !settings.fixed_direction && (angle > 90.0 + 1e-9 || angle <= -90.0 + 1e-9) {
        angle += if angle > 0.0 { -180.0 } else { 180.0 };
    }
    let (sin, cos) = angle.to_radians().sin_cos();
    let content = text.replace("<>", &settings.format(measurement, value));
    let (size_x, size_y, spacing) = settings.text_size;
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
        text_type: settings.text_style,
        size_x,
        size_y,
        spacing,
        angle,
        font: super::DEFAULT_FONT.to_string(),
        content,
    };
    text_width(&mut value_text);
    let length = value_text.x2.hypot(value_text.y2);
    let middle = [(first[0] + second[0]) / 2.0, (first[1] + second[1]) / 2.0];
    value_text.x1 = middle[0] - cos * length / 2.0 - sin * settings.gap;
    value_text.y1 = middle[1] - sin * length / 2.0 + cos * settings.gap;
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
