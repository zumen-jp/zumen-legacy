//! 文字の配置（`@zumenjs/core/text` と同じ規則）

use crate::geometry::multiply;
use crate::js;
use crate::types::{GlyphCell, Matrix, TextPrimitive};
use std::f64::consts::PI;
use unicode_normalization::UnicodeNormalization;

const SANS: &str = "\"MS Gothic\", \"Hiragino Kaku Gothic ProN\", \"Hiragino Sans\", \"Yu Gothic\", \"Noto Sans JP\", \"Noto Sans CJK JP\", sans-serif";
const SERIF: &str =
    "\"MS Mincho\", \"Hiragino Mincho ProN\", \"Yu Mincho\", \"Noto Serif JP\", \"Noto Serif CJK JP\", serif";

/// 図面のフォント名から CSS の font-family を選ぶ。
/// 扱えないフォントは MS ゴシック相当で表示する（SXF 実装規約 1-3）
pub fn font_family_for(name: &str) -> &'static str {
    let normalized = name.nfkc().collect::<String>().to_lowercase();
    if ["明朝", "mincho", "serif", "century", "times"].iter().any(|word| normalized.contains(word)) {
        SERIF
    } else {
        SANS
    }
}

/// 明朝体の font-family か（描画で字形を選ぶのに使う）
pub fn is_serif_family(family: &str) -> bool {
    family == SERIF
}

/// CP932 で 1 バイトの文字（ASCII と半角カナ）。全角の半分の幅で並べる（SXF 実装規約 1-5）
pub fn is_half_width(c: char) -> bool {
    let code = c as u32;
    (0x20..=0x7e).contains(&code) || (0xff61..=0xff9f).contains(&code) || code == 0xa5 || code == 0x203e
}

#[derive(Debug, Clone)]
pub struct TextLayoutInput<'a> {
    pub text: &'a str,
    /// 配置基点（元の座標）
    pub x: f64,
    pub y: f64,
    /// 文字範囲の高さと幅
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
    /// 文字列の回転角（度、反時計回り）
    pub angle: f64,
    /// スラント角度（度、時計回りが正）
    pub slant: f64,
    /// 配置基点の位置（1:左下 2:中下 3:右下 4:左中 5:中中 6:右中 7:左上 8:中上 9:右上）
    pub anchor: f64,
    pub vertical: bool,
    pub color: String,
    pub font_family: String,
}

/// 文字範囲（高さ・幅・文字間隔）に収まるように 1 文字ずつ並べた文字列を作る。
/// 全角を 1、半角を 0.5 として文字の幅（縦書きは高さ）を割り振る
pub fn layout_text(input: TextLayoutInput<'_>, transform: &Matrix) -> Option<TextPrimitive> {
    let chars: Vec<char> = input.text.chars().collect();
    if chars.is_empty() || input.height.is_nan() || input.height <= 0.0 {
        return None;
    }
    let units: Vec<f64> = chars.iter().map(|c| if is_half_width(*c) { 0.5 } else { 1.0 }).collect();
    let total_units: f64 = units.iter().fold(0.0, |sum, unit| sum + unit);
    let spacing = js::max(0.0, input.spacing);
    let gaps = js::max(0.0, chars.len() as f64 - 1.0) * spacing;
    let mut cells = Vec::with_capacity(chars.len());
    let box_width;
    let box_height;
    let glyph_height;
    let scale_x;
    if input.vertical {
        box_height = input.height;
        box_width = if input.width > 0.0 { input.width } else { input.height / js::max(total_units, 1.0) };
        let unit =
            if total_units > 0.0 && box_height - gaps > 0.0 { (box_height - gaps) / total_units } else { box_width };
        glyph_height = unit;
        scale_x = if unit > 0.0 { box_width / unit } else { 1.0 };
        let mut top = box_height;
        for (c, unit_count) in chars.iter().zip(&units) {
            let cell_height = unit_count * unit;
            cells.push(GlyphCell { character: c.to_string(), x: box_width / 2.0, y: top - cell_height / 2.0 });
            top -= cell_height + spacing;
        }
    } else {
        box_height = input.height;
        box_width = if input.width > 0.0 { input.width } else { total_units * input.height + gaps };
        let unit =
            if total_units > 0.0 && box_width - gaps > 0.0 { (box_width - gaps) / total_units } else { input.height };
        glyph_height = input.height;
        scale_x = unit / input.height;
        let mut left = 0.0;
        for (c, unit_count) in chars.iter().zip(&units) {
            let cell_width = unit_count * unit;
            cells.push(GlyphCell { character: c.to_string(), x: left + cell_width / 2.0, y: box_height / 2.0 });
            left += cell_width + spacing;
        }
    }
    let anchor =
        if input.anchor.fract() == 0.0 && input.anchor >= 1.0 && input.anchor <= 9.0 { input.anchor } else { 1.0 };
    let column = (anchor - 1.0) % 3.0;
    let row = ((anchor - 1.0) / 3.0).floor();
    let offset_x = (column * box_width) / 2.0;
    let offset_y = (row * box_height) / 2.0;
    let angle = (input.angle * PI) / 180.0;
    let cos = js::cos(angle);
    let sin = js::sin(angle);
    let shear = js::tan((js::max(-85.0, js::min(85.0, input.slant)) * PI) / 180.0);
    // 文字の座標 → ベースラインを軸にスラント → 基点を原点へ → 回転 → 配置基点へ
    let local = multiply(
        &[cos, sin, -sin, cos, input.x, input.y],
        &multiply(&[1.0, 0.0, 0.0, 1.0, -(offset_x + offset_y * shear), -offset_y], &[1.0, 0.0, shear, 1.0, 0.0, 0.0]),
    );
    Some(TextPrimitive {
        text: input.text.to_string(),
        color: input.color,
        font_family: input.font_family,
        height: glyph_height,
        scale_x: if scale_x.is_finite() && scale_x > 0.0 { scale_x } else { 1.0 },
        cells,
        box_width,
        box_height,
        matrix: multiply(transform, &local),
    })
}
