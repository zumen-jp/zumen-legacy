//! SXF の既定義の色・線種・線幅と、コードの名前（`@zumenjs/sxf` の tables.ts）

use zumen_core::js;

pub struct PredefinedColor {
    pub name: &'static str,
    pub label: &'static str,
    pub rgb: [u8; 3],
}

/// SXF Ver.3.1 共通既定義要素編の既定義色（コード 1〜16）
pub const PREDEFINED_COLORS: [PredefinedColor; 16] = [
    PredefinedColor { name: "black", label: "黒", rgb: [0, 0, 0] },
    PredefinedColor { name: "red", label: "赤", rgb: [255, 0, 0] },
    PredefinedColor { name: "green", label: "緑", rgb: [0, 255, 0] },
    PredefinedColor { name: "blue", label: "青", rgb: [0, 0, 255] },
    PredefinedColor { name: "yellow", label: "黄色", rgb: [255, 255, 0] },
    PredefinedColor { name: "magenta", label: "マジェンタ", rgb: [255, 0, 255] },
    PredefinedColor { name: "cyan", label: "シアン", rgb: [0, 255, 255] },
    PredefinedColor { name: "white", label: "白", rgb: [255, 255, 255] },
    PredefinedColor { name: "deeppink", label: "牡丹", rgb: [192, 0, 128] },
    PredefinedColor { name: "brown", label: "茶", rgb: [192, 128, 64] },
    PredefinedColor { name: "orange", label: "橙", rgb: [255, 128, 0] },
    PredefinedColor { name: "lightgreen", label: "薄緑", rgb: [128, 192, 128] },
    PredefinedColor { name: "lightblue", label: "明青", rgb: [0, 128, 255] },
    PredefinedColor { name: "lavender", label: "青紫", rgb: [128, 64, 255] },
    PredefinedColor { name: "lightgray", label: "明灰", rgb: [192, 192, 192] },
    PredefinedColor { name: "darkgray", label: "暗灰", rgb: [128, 128, 128] },
];

pub struct PredefinedLineType {
    pub name: &'static str,
    pub label: &'static str,
    /// 線幅 0.5mm 相当の参考値（用紙の mm）
    pub pitch: &'static [f64],
}

/// 既定義線種（コード 1〜15）
pub const PREDEFINED_LINE_TYPES: [PredefinedLineType; 15] = [
    PredefinedLineType { name: "continuous", label: "実線", pitch: &[] },
    PredefinedLineType { name: "dashed", label: "破線", pitch: &[6.0, 1.5] },
    PredefinedLineType { name: "dashed spaced", label: "跳び破線", pitch: &[6.0, 6.0] },
    PredefinedLineType { name: "long dashed dotted", label: "一点長鎖線", pitch: &[12.0, 1.5, 0.25, 1.5] },
    PredefinedLineType {
        name: "long dashed double-dotted",
        label: "二点長鎖線",
        pitch: &[12.0, 1.5, 0.25, 1.5, 0.25, 1.5],
    },
    PredefinedLineType {
        name: "long dashed triplicate-dotted",
        label: "三点長鎖線",
        pitch: &[12.0, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5],
    },
    PredefinedLineType { name: "dotted", label: "点線", pitch: &[0.25, 1.5] },
    PredefinedLineType { name: "chain", label: "一点鎖線", pitch: &[12.0, 1.5, 3.5, 1.5] },
    PredefinedLineType { name: "chain double dash", label: "二点鎖線", pitch: &[12.0, 1.5, 3.5, 1.5, 3.5, 1.5] },
    PredefinedLineType { name: "dashed dotted", label: "一点短鎖線", pitch: &[6.0, 1.5, 0.25, 1.5] },
    PredefinedLineType {
        name: "double-dashed dotted", label: "一点二短鎖線", pitch: &[6.0, 1.5, 6.0, 1.5, 0.25, 1.5]
    },
    PredefinedLineType {
        name: "dashed double-dotted", label: "二点短鎖線", pitch: &[6.0, 1.5, 0.25, 1.5, 0.25, 1.5]
    },
    PredefinedLineType {
        name: "double-dashed double-dotted",
        label: "二点二短鎖線",
        pitch: &[6.0, 1.5, 6.0, 1.5, 0.25, 1.5, 0.25, 1.5],
    },
    PredefinedLineType {
        name: "dashed triplicate-dotted",
        label: "三点短鎖線",
        pitch: &[6.0, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5],
    },
    PredefinedLineType {
        name: "double-dashed triplicate-dotted",
        label: "三点二短鎖線",
        pitch: &[6.0, 1.5, 6.0, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5],
    },
];

/// 既定義線幅（コード 1〜9、mm）
pub const PREDEFINED_WIDTHS: [f64; 9] = [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1.0, 1.4, 2.0];

/// 寸法線・引出し線の矢印コード（1〜11）
pub const ARROW_NAMES: [&str; 11] = [
    "blanked arrow",
    "blanked box",
    "blanked dot",
    "dimension origin",
    "filled box",
    "filled arrow",
    "filled dot",
    "integral symbol",
    "open arrow",
    "slash",
    "unfilled arrow",
];

/// 点マーカのコード（1〜7）
pub const MARKER_NAMES: [&str; 7] = ["asterisk", "circle", "dot", "plus", "square", "triangle", "X"];

/// 数を配列の添字にする。JavaScript の `array[value]` と同じく、0 以上の整数（-0 を含む）だけを添字とみなす
pub fn index(value: f64) -> Option<usize> {
    if value == 0.0 {
        return Some(0);
    }
    (value > 0.0 && value.fract() == 0.0 && value < 4_294_967_295.0).then_some(value as usize)
}

/// 用紙サイズ種別（JIS A 列の縦長の寸法、mm）
pub fn sheet_size(kind: f64) -> Option<(&'static str, f64, f64)> {
    Some(match index(kind)? {
        0 => ("A0", 841.0, 1189.0),
        1 => ("A1", 594.0, 841.0),
        2 => ("A2", 420.0, 594.0),
        3 => ("A3", 297.0, 420.0),
        4 => ("A4", 210.0, 297.0),
        _ => return None,
    })
}

pub fn sfig_kind(flag: f64) -> Option<&'static str> {
    Some(match index(flag)? {
        1 => "部分図（数学座標系）",
        2 => "部分図（測地座標系）",
        3 => "作図グループ",
        4 => "作図部品",
        _ => return None,
    })
}

/// P21 の接頭語（$$SXF_FM_ など）と属性付加機構の記述を除いた複合図形名
pub fn display_name(name: &str) -> &str {
    let stripped = name
        .strip_prefix("$$SXF_")
        .and_then(|rest| ["FM_", "FG_", "G_", "P_"].iter().find_map(|prefix| rest.strip_prefix(prefix)));
    match stripped {
        Some(rest) if !rest.is_empty() => rest,
        _ => name,
    }
}

pub fn arrow_name(code: f64) -> Option<&'static str> {
    ARROW_NAMES.get(index(code)?.checked_sub(1)?).copied()
}

pub fn marker_name(code: f64) -> Option<&'static str> {
    MARKER_NAMES.get(index(code)?.checked_sub(1)?).copied()
}

fn normalized(name: &str) -> String {
    js::trim(name).to_lowercase()
}

pub fn predefined_color_code(name: &str) -> usize {
    let name = normalized(name);
    PREDEFINED_COLORS.iter().position(|color| color.name == name).map_or(0, |index| index + 1)
}

pub fn predefined_line_type_code(name: &str) -> usize {
    let name = normalized(name);
    PREDEFINED_LINE_TYPES.iter().position(|kind| kind.name == name).map_or(0, |index| index + 1)
}

/// 既定義線幅と一致すれば 1〜9、一致しなければ 0
pub fn predefined_width_code(width: f64) -> usize {
    PREDEFINED_WIDTHS.iter().position(|value| (value - width).abs() < 1e-6).map_or(0, |index| index + 1)
}
