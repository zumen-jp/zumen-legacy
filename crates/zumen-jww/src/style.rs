//! 線色・線種・線幅・レイヤの名前・用紙・文字の配置を、ヘッダの設定から決める。
//! 表示（scene）と、AutoCAD の図面のモデルへの変換（zumen-convert）が同じ規則で引く。

use crate::reader::{JwwBase, JwwDocument, JwwText, colorref};
use crate::sxf_tables::{SXF_COLORS, SXF_LINE_PITCHES};
use zumen_core::js;
use zumen_core::text::{TextLayoutInput, font_family_for};

/// 用紙サイズの番号の名前と寸法（横長、mm）
pub fn paper_size(number: u32) -> Option<(&'static str, f64, f64)> {
    Some(match number {
        0 => ("A0", 1189.0, 841.0),
        1 => ("A1", 841.0, 594.0),
        2 => ("A2", 594.0, 420.0),
        3 => ("A3", 420.0, 297.0),
        4 => ("A4", 297.0, 210.0),
        8 => ("2A", 1682.0, 1189.0),
        9 => ("3A", 2378.0, 1682.0),
        10 => ("4A", 3364.0, 2378.0),
        11 => ("5A", 4756.0, 3364.0),
        _ => return None,
    })
}

/// ヘッダの色が読めないときの、白背景の画面色（線色 1〜9）
const DEFAULT_PEN_COLORS: [&str; 10] =
    ["#ffffff", "#00c0c0", "#000000", "#00c000", "#c0c000", "#c000c0", "#0000ff", "#008080", "#ff0080", "#c0c0c0"];

/// ヘッダの線種が読めないときの破線（線種 2〜9、mm）
fn default_line_type(style: u8) -> &'static [f64] {
    match style {
        2 => &[1.0, 1.0],
        3 => &[2.0, 1.0],
        4 => &[4.0, 1.0],
        5 => &[6.0, 1.0, 1.0, 1.0],
        6 => &[10.0, 1.0, 1.0, 1.0],
        7 => &[6.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        8 => &[10.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        9 => &[0.5, 0.5],
        _ => &[],
    }
}

/// 線色・線幅が分からない線の太さ（mm）
pub const DEFAULT_PEN_WIDTH: f64 = 0.18;

/// ランダム線（線種 11〜15）。Jw_cad は手書き風に揺らすが、実線で描く
pub fn is_random_line(style: u8) -> bool {
    (11..=15).contains(&style)
}

/// Jw_cad 8 以降が (0, -1000) に文字として保存する「名前 = 値」の設定（/^[A-Za-z][A-Za-z0-9_]* = /）か
pub fn is_setting(text: &JwwText) -> bool {
    if text.x1 != 0.0 || text.y1 != -1000.0 {
        return false;
    }
    let bytes = text.content.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    let end = bytes.iter().position(|b| !(b.is_ascii_alphanumeric() || *b == b'_')).unwrap_or(bytes.len());
    bytes[end..].starts_with(b" = ")
}

/// 文字に埋め込んだ画像（^@BM など）か
pub fn is_embedded_image(text: &JwwText) -> bool {
    text.content.starts_with("^@")
}

/// 文字範囲に並べる入力。始点が左下、幅は始点から終点まで。フォント名が @ で始まれば縦書き
pub fn text_layout_input(text: &JwwText, color: String) -> TextLayoutInput<'_> {
    TextLayoutInput {
        text: &text.content,
        x: text.x1,
        y: text.y1,
        height: if text.size_y > 0.0 { text.size_y } else { 2.5 },
        width: js::hypot2(text.x2 - text.x1, text.y2 - text.y1),
        spacing: text.spacing,
        angle: text.angle,
        slant: 0.0,
        anchor: 1.0,
        vertical: text.font.starts_with('@'),
        color,
        font_family: font_family_for(text.font.strip_prefix('@').unwrap_or(&text.font)).to_string(),
    }
}

impl JwwDocument {
    /// 図形の色（#rrggbb）。線色 1〜9 はヘッダの画面色、100〜356 は SXF 対応色、ソリッドの任意色（線色 10）は
    /// custom。定義のない線色は None
    pub fn pen_color(&self, base: &JwwBase, custom: Option<u32>) -> Option<String> {
        let n = usize::from(base.pen_color);
        if n == 10
            && let Some(custom) = custom
        {
            return Some(colorref(custom));
        }
        if (1..=9).contains(&n) {
            return Some(match &self.pen_colors {
                Some(colors) => colors[n].clone(),
                None => DEFAULT_PEN_COLORS[n].to_string(),
            });
        }
        if (100..=356).contains(&n) {
            if let Some(extended) = self.extended_colors.as_ref().and_then(|colors| colors.get(n - 100))
                && !extended.is_empty()
            {
                return Some(extended.clone());
            }
            if let Some([r, g, b]) = SXF_COLORS.get(n.wrapping_sub(101)) {
                return Some(format!("#{r:02x}{g:02x}{b:02x}"));
            }
        }
        None
    }

    /// 線種の線分・空白の長さ（mm、実線は空）。ヘッダの設定があればそれを使い、奇数個なら 2 周期にする
    pub fn line_dashes(&self, style: u8) -> Vec<f64> {
        if style <= 1 || is_random_line(style) {
            return Vec::new();
        }
        if let Some(segments) = self.line_types.get(&u32::from(style))
            && segments.len() >= 2
            && segments.iter().all(|value| *value > 0.0)
        {
            return if segments.len() % 2 == 0 { segments.clone() } else { [&segments[..], &segments[..]].concat() };
        }
        if (31..=46).contains(&style) {
            return SXF_LINE_PITCHES.get(usize::from(style - 31)).map(|pitches| pitches.to_vec()).unwrap_or_default();
        }
        default_line_type(style).to_vec()
    }

    /// 線の太さ（mm）。図形ごとの線幅（1/100 mm か印刷のドット）、なければ線色ごとの印刷線幅
    pub fn line_width(&self, base: &JwwBase) -> f64 {
        let pen_width = f64::from(base.pen_width);
        if base.pen_width > 0 && self.width_unit.hundredth {
            return pen_width / 100.0;
        }
        if base.pen_width > 0
            && let Some(dpi) = self.width_unit.dpi
        {
            return (pen_width * 25.4) / dpi;
        }
        let n = usize::from(base.pen_color);
        let width = if (1..=9).contains(&n) {
            self.pen_widths.as_ref().and_then(|widths| widths.get(n).copied().flatten())
        } else {
            None
        };
        width.unwrap_or(DEFAULT_PEN_WIDTH)
    }

    /// レイヤグループの縮尺の分母。0 以下・読めないものは 1
    pub fn group_scale(&self, group: u16) -> f64 {
        match self.groups.get(usize::from(group & 0x0f)) {
            Some(info) if info.scale > 0.0 => info.scale,
            _ => 1.0,
        }
    }

    pub fn group_name(&self, group: usize) -> String {
        match self.groups.get(group).map(|info| js::trim(&info.name)) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => format!("グループ {group:X}"),
        }
    }

    /// レイヤの名前（「0-A 通り芯」）。Jw_cad は既定のレイヤ名を「0-A」のように番号のまま保存するため、
    /// 番号と同じ名前は重ねて示さない
    pub fn layer_name(&self, group: usize, layer: usize) -> String {
        let code = format!("{group:X}-{layer:X}");
        let name = self.groups.get(group).and_then(|info| info.layers.get(layer)).map(|info| js::trim(&info.name));
        match name {
            Some(name) if !name.is_empty() && name.to_uppercase() != code => format!("{code} {name}"),
            _ => code,
        }
    }
}
