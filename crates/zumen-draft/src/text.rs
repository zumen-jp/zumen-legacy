//! 文字。1 行は TEXT、複数行（改行・折り返し）は MTEXT にする。
//!
//! 文字の幅は見積もる: zumen-dxf の表示と同じく、TEXT の高さの em（全角を含む文字は 1/0.88、ほかは 1/0.72 倍）
//! に、全角を 1、半角を 0.5 とした文字の数と幅の係数を掛ける。

use crate::spec::{Point, StyleSpec, TextSpec};
use crate::{Ctx, DraftError, Drafter, number, point, positive};
use zumen_convert::builder::{Look, Space, xyz};
use zumen_core::text::is_half_width;
use zumen_dxf::{DxfEntityData, DxfMText, DxfTextBase, text_em};

/// 文字の位置の合わせ方
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Align {
    /// 0 左、1 中央、2 右
    pub h: u8,
    /// 0 ベースライン、1 下、2 中、3 上
    pub v: u8,
}

pub(crate) const BOTTOM_LEFT: Align = Align { h: 0, v: 1 };
pub(crate) const MIDDLE_CENTER: Align = Align { h: 1, v: 2 };
pub(crate) const MIDDLE_LEFT: Align = Align { h: 0, v: 2 };
pub(crate) const BOTTOM_CENTER: Align = Align { h: 1, v: 1 };

pub(crate) const ALIGNS: [&str; 12] = [
    "top-left",
    "top-center",
    "top-right",
    "middle-left",
    "middle-center",
    "middle-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
    "left",
    "center",
    "right",
];

impl Align {
    pub fn parse(name: Option<&str>, default: Align, path: &str) -> Result<Align, DraftError> {
        let Some(name) = name else { return Ok(default) };
        let (v, h) = match name {
            "left" => return Ok(Align { h: 0, v: 0 }),
            "center" => return Ok(Align { h: 1, v: 0 }),
            "right" => return Ok(Align { h: 2, v: 0 }),
            "middle" => return Ok(MIDDLE_CENTER),
            _ => name.split_once('-').unwrap_or(("", "")),
        };
        let v = match v {
            "top" => 3,
            "middle" => 2,
            "bottom" => 1,
            _ => 9,
        };
        let h = match h {
            "left" => 0,
            "center" => 1,
            "right" => 2,
            _ => 9,
        };
        if v == 9 || h == 9 {
            return Err(DraftError::new(
                "invalid-value",
                path,
                format!("align の {name} はありません（{}）", ALIGNS.join("・")),
            ));
        }
        Ok(Align { h, v })
    }

    /// MTEXT の合わせ方（71、1 左上〜9 右下）。ベースラインは下にそろえる
    fn attachment(self) -> f64 {
        let row = match self.v {
            3 => 0,
            2 => 1,
            _ => 2,
        };
        f64::from(row * 3 + self.h + 1)
    }
}

/// 全角を 1、半角を 0.5 とした文字の数
fn units(text: &str) -> f64 {
    text.chars().fold(0.0, |total, c| total + if is_half_width(c) { 0.5 } else { 1.0 })
}

/// 1 行の文字の幅の見積もり
pub(crate) fn width(text: &str, height: f64, factor: f64) -> f64 {
    units(text) * height * text_em(text) * factor
}

/// 幅に収まるよう、行を分ける。空白のある行は語の切れ目で、ない行（日本語）は文字の切れ目で分ける
pub(crate) fn wrap(text: &str, max_width: f64, height: f64, factor: f64) -> Vec<String> {
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for c in paragraph.chars() {
            let mut candidate = line.clone();
            candidate.push(c);
            if !line.is_empty() && width(&candidate, height, factor) > max_width {
                // 語の途中なら、最後の空白で分ける
                match line.rfind(' ').filter(|_| c != ' ') {
                    Some(space) if space > 0 => {
                        let rest = line[space + 1..].to_string();
                        out.push(line[..space].to_string());
                        line = rest;
                        line.push(c);
                    }
                    _ => {
                        out.push(std::mem::take(&mut line));
                        if c != ' ' {
                            line.push(c);
                        }
                    }
                }
            } else {
                line = candidate;
            }
        }
        out.push(line);
    }
    out
}

/// 文字列を TEXT の文字列にする。% が 2 つ続くと制御コード（%%c など）になるので、続く % の前の % を %%% にし、
/// ^ はテキストの DXF の書き方（「^ 」）にする（DXF Reference「Control Codes and Special Characters」）
fn escape_text(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, c) in chars.iter().enumerate() {
        match c {
            '%' if chars.get(index + 1) == Some(&'%') => out.push_str("%%%"),
            '^' => out.push_str("^ "),
            c => out.push(*c),
        }
    }
    out
}

/// MTEXT の文字列にする（\ { } を書式コードから区別し、改行は \P。DXF Reference「MTEXT」）
fn escape_mtext(lines: &[String]) -> String {
    let mut out = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            out.push_str("\\P");
        }
        for c in line.chars() {
            match c {
                '\\' => out.push_str("\\\\"),
                '{' => out.push_str("\\{"),
                '}' => out.push_str("\\}"),
                '\r' => {}
                _ => out.push(c),
            }
        }
    }
    out
}

/// 置く文字
pub(crate) struct Placed<'a> {
    pub at: Point,
    pub lines: &'a [String],
    /// 置く場所の長さの高さ
    pub height: f64,
    pub align: Align,
    pub rotation: f64,
    pub width_factor: f64,
    pub line_spacing: f64,
    pub style: String,
}

/// 文字を置く。1 行は TEXT、複数行は MTEXT
pub(crate) fn put(drafter: &mut Drafter, space: &Space, look: &Look, placed: Placed<'_>, path: &str) -> String {
    let rotation = placed.rotation.rem_euclid(360.0);
    if placed.lines.len() <= 1 {
        let text = placed.lines.first().map_or("", String::as_str);
        let data = DxfTextBase {
            text: escape_text(text),
            insert: xyz(placed.at),
            align_point: (placed.align.h != 0 || placed.align.v != 0).then(|| xyz(placed.at)),
            height: placed.height,
            rotation,
            oblique: 0.0,
            width: placed.width_factor,
            style: placed.style,
            halign: f64::from(placed.align.h),
            valign: f64::from(placed.align.v),
            text_generation_flag: 0.0,
        };
        return drafter.add(space, look, "TEXT", DxfEntityData::Text(data), path);
    }
    let mut raw = escape_mtext(placed.lines);
    if placed.width_factor != 1.0 {
        raw = format!("\\W{};{raw}", trim(placed.width_factor));
    }
    let data = DxfMText {
        text: placed.lines.join("\n"),
        raw_text: raw,
        insert: xyz(placed.at),
        char_height: placed.height,
        rect_width: 0.0,
        attachment_point: placed.align.attachment(),
        drawing_direction: 1.0,
        text_direction: None,
        rotation,
        style: placed.style,
        line_spacing_style: 1.0,
        line_spacing_factor: placed.line_spacing,
        background_flags: 0.0,
        background_color_index: None,
        background_true_color: None,
        background_scale: None,
    };
    drafter.add(space, look, "MTEXT", DxfEntityData::MText(data), path)
}

/// 書式コードに書く数（余分な 0 を除く）
fn trim(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// 幅の係数の下限（これより縮めるなら、文字を低くする）
const MIN_WIDTH_FACTOR: f64 = 0.6;

/// 幅に収める: 幅の係数を縮め、下限より縮めるなら高さも下げる。返すのは (高さ, 幅の係数)
pub(crate) fn fit(lines: &[String], max_width: f64, height: f64, factor: f64) -> (f64, f64) {
    let widest = lines.iter().map(|line| width(line, height, factor)).fold(0.0, f64::max);
    if widest <= max_width || widest <= 0.0 || max_width <= 0.0 {
        return (height, factor);
    }
    let ratio = max_width / widest;
    if factor * ratio >= MIN_WIDTH_FACTOR {
        (height, factor * ratio)
    } else {
        let shrink = (factor * ratio) / MIN_WIDTH_FACTOR;
        (height * shrink, MIN_WIDTH_FACTOR)
    }
}

/// 文字の図形を描く
pub(crate) fn draw(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &TextSpec,
    path: &str,
) -> Result<(), DraftError> {
    let default_layer = drafter.defaults.text_layer.clone();
    let look = drafter.look(ctx, style, default_layer.as_deref(), path)?;
    let at = point(spec.at, path)?;
    let height = ctx.size(positive(spec.height.unwrap_or_else(|| drafter.text_height()), "height", path)?);
    let mut factor = positive(spec.width_factor.unwrap_or(1.0), "widthFactor", path)?;
    let align = Align::parse(spec.align.as_deref(), BOTTOM_LEFT, path)?;
    let rotation = number(spec.rotation.unwrap_or(0.0), "rotation", path)?;
    let lines: Vec<String> = match spec.width {
        Some(wrap_width) => wrap(&spec.text, ctx.size(positive(wrap_width, "width", path)?), height, factor),
        None => spec.text.split('\n').map(str::to_string).collect(),
    };
    let mut height = height;
    if let Some(max) = spec.fit {
        (height, factor) = fit(&lines, ctx.size(positive(max, "fit", path)?), height, factor);
    }
    let line_spacing = positive(spec.line_spacing.unwrap_or(1.0), "lineSpacing", path)?;
    let text_style = drafter.text_style(spec.font.as_deref());
    let placed =
        Placed { at, lines: &lines, height, align, rotation, width_factor: factor, line_spacing, style: text_style };
    put(drafter, &ctx.space, &look, placed, path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_japanese_by_characters_and_latin_by_words() {
        let lines = wrap("あいうえおかきくけこ", width("あいうえお", 2.5, 1.0) + 0.01, 2.5, 1.0);
        assert_eq!(lines, ["あいうえお", "かきくけこ"]);
        let lines = wrap("steel deck slab", width("steel deck", 2.5, 1.0) + 0.01, 2.5, 1.0);
        assert_eq!(lines, ["steel deck", "slab"]);
    }

    #[test]
    fn fits_by_width_factor_then_height() {
        let lines = vec!["あいうえお".to_string()];
        let full = width(&lines[0], 2.5, 1.0);
        let (height, factor) = fit(&lines, full * 0.8, 2.5, 1.0);
        assert_eq!(height, 2.5);
        assert!((factor - 0.8).abs() < 1e-9);
        let (height, factor) = fit(&lines, full * 0.3, 2.5, 1.0);
        assert_eq!(factor, MIN_WIDTH_FACTOR);
        assert!((width(&lines[0], height, factor) - full * 0.3).abs() < 1e-9);
    }

    #[test]
    fn parses_alignments() {
        assert_eq!(Align::parse(Some("top-right"), BOTTOM_LEFT, "").unwrap(), Align { h: 2, v: 3 });
        assert_eq!(Align::parse(Some("center"), BOTTOM_LEFT, "").unwrap(), Align { h: 1, v: 0 });
        assert_eq!(Align::parse(None, MIDDLE_CENTER, "").unwrap(), MIDDLE_CENTER);
        assert_eq!(Align::parse(Some("top"), BOTTOM_LEFT, "").unwrap_err().code, "invalid-value");
        assert_eq!(Align { h: 1, v: 2 }.attachment(), 5.0);
        assert_eq!(Align { h: 0, v: 0 }.attachment(), 7.0);
    }
}
