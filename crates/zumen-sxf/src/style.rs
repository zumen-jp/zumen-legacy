//! 色・線種・線幅・文字フォントのコードを値にする（附属書 SFC 編 1-3-8〜1-3-18）。
//! 表示（scene）と、AutoCAD の図面のモデルへの変換（zumen-convert）が同じ規則で引く。

use crate::document::SxfDocument;
use crate::features::Annotation;
use crate::tables::{PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, PREDEFINED_WIDTHS, index};
use zumen_core::js;
use zumen_core::text::TextLayoutInput;

/// 線幅コード 0 と、定義のない線幅コードの太さ（mm）
pub const DEFAULT_WIDTH: f64 = 0.13;

/// `Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, "0")`
fn hex_part(value: f64) -> String {
    let clamped = js::max(0.0, js::min(255.0, js::round(value)));
    if clamped.is_nan() { "NaN".into() } else { format!("{:02x}", clamped as u32) }
}

pub(crate) fn hex(r: f64, g: f64, b: f64) -> String {
    format!("#{}{}{}", hex_part(r), hex_part(g), hex_part(b))
}

impl SxfDocument {
    /// 色コードの色（#rrggbb）。既定義色は 1〜16、ユーザ定義色は 17 から。定義のない色は None
    pub fn color(&self, code: f64) -> Option<String> {
        if let Some(predefined) = index(code - 1.0).and_then(|index| PREDEFINED_COLORS.get(index)) {
            let [r, g, b] = predefined.rgb;
            return Some(hex(f64::from(r), f64::from(g), f64::from(b)));
        }
        if code >= 17.0
            && let Some(user) = index(code - 17.0).and_then(|index| self.user_colors.get(index))
        {
            return Some(hex(user.r, user.g, user.b));
        }
        None
    }

    /// 線種コードの線分・空白の長さ（用紙の mm、実線は空）。既定義線種は 1〜16 で、表にない 16 は実線。
    /// ユーザ定義線種は 17 からで、線分と空白が正の値の対になっていなければ実線。定義のない線種は None
    pub fn dashes(&self, code: f64) -> Option<Vec<f64>> {
        if (1.0..=16.0).contains(&code) {
            return Some(
                index(code - 1.0)
                    .and_then(|index| PREDEFINED_LINE_TYPES.get(index))
                    .map(|line_type| line_type.pitch.to_vec())
                    .unwrap_or_default(),
            );
        }
        if code >= 17.0
            && let Some(user) = index(code - 17.0).and_then(|index| self.user_line_types.get(index))
        {
            let valid = user.pitch.iter().all(|value| *value > 0.0) && user.pitch.len() % 2 == 0;
            return Some(if valid { user.pitch.clone() } else { Vec::new() });
        }
        None
    }

    /// 線幅コードの太さ（mm）。既定義線幅は 1〜9、ユーザ定義線幅は 11 から、0 は 0.13 mm。定義のない線幅は None
    pub fn width(&self, code: f64) -> Option<f64> {
        if let Some(predefined) = index(code - 1.0).and_then(|index| PREDEFINED_WIDTHS.get(index)) {
            return Some(*predefined);
        }
        if code >= 11.0
            && let Some(user) = index(code - 11.0).and_then(|index| self.user_widths.get(index))
        {
            return Some(user.width);
        }
        (code == 0.0).then_some(DEFAULT_WIDTH)
    }

    /// 文字フォントコードのフォント名
    pub fn font(&self, code: f64) -> Option<&str> {
        index(code - 1.0).and_then(|index| self.fonts.get(index)).map(|font| font.name.as_str())
    }
}

impl Annotation {
    /// 文字範囲に並べる入力。x・y は表示の向きにした配置基点、font_family は CSS の font-family
    pub fn layout_input(&self, x: f64, y: f64, color: String, font_family: String) -> TextLayoutInput<'_> {
        TextLayoutInput {
            text: &self.text,
            x,
            y,
            height: self.height,
            width: self.width,
            spacing: self.spacing,
            angle: self.angle,
            slant: self.slant,
            anchor: self.anchor,
            vertical: self.direction == 2.0,
            color,
            font_family,
        }
    }
}
