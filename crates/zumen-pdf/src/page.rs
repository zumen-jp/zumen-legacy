//! ページの大きさと向き、ページラベル、用紙、注釈

use crate::strings;
use hayro_interpret::hayro_syntax::object::{Array, Dict, Name, Object, String as PdfString};
use hayro_interpret::hayro_syntax::page::{Page, Rotation};
use kurbo::Affine;
use std::collections::HashSet;
use zumen_core::{Sheet, js, limits};

/// 1 pt（既定のユーザー空間の 1 単位）が何 mm か
pub(crate) const MM_PER_PT: f64 = 25.4 / 72.0;

/// 大きさのないページの大きさ（A4、pt）
const A4: (f64, f64) = (210.0 / MM_PER_PT, 297.0 / MM_PER_PT);

/// ページラベルの数の木（7.9.7）をたどる深さと節の数の上限。壊れた PDF の循環と巨大な木で止まらないように
const MAX_TREE_DEPTH: usize = 32;
const MAX_TREE_NODES: usize = 10_000;

/// 文字で数えるページラベル（A〜Z、AA〜ZZ …）で、同じ文字を並べる数の上限。超えたら数字で書く
const MAX_LETTERS: u64 = 64;

/// ページの大きさと、ユーザー空間から用紙の mm への変換
pub(crate) struct Geometry {
    /// 回転を済ませたページの幅と高さ（pt。UserUnit は掛けない）。pdf.js の getViewport({ scale: 1 }) と同じ
    pub width: f64,
    pub height: f64,
    /// 1 pt が用紙の何 mm か（UserUnit を掛けたもの）
    pub scale: f64,
    /// 既定のユーザー空間 → 用紙の mm（クロップボックスの左下が原点、y は上向き、ページの回転を済ませた向き）
    pub user_space: Affine,
}

impl Geometry {
    /// ページの見える範囲はクロップボックスとメディアボックスの重なり（7.7.3.3、14.11.2）、向きは Rotate（時計回りに
    /// 90 度ずつ）、単位の大きさは UserUnit（表 30。PDF 1.6）。大きさのない範囲は A4、1 pt より小さい辺は 1 pt とみなす
    /// （hayro と同じ）
    pub(crate) fn of(page: &Page<'_>) -> Self {
        let crop = page.intersected_crop_box();
        let (mut width, mut height) = (crop.x1 - crop.x0, crop.y1 - crop.y0);
        if width.abs() < 1e-6 || height.abs() < 1e-6 {
            (width, height) = (A4.0, A4.1);
        }
        let (width, height) = (width.max(1.0), height.max(1.0));
        let user_unit =
            page.raw().get::<f64>(b"UserUnit").filter(|unit| unit.is_finite() && *unit > 0.0).unwrap_or(1.0);
        let scale = MM_PER_PT * user_unit;
        // クロップボックスの左下を原点にしてから、表示の向きに回す（y は上向きのまま）
        let origin = Affine::translate((-crop.x0, -crop.y0));
        let (rotation, displayed) = match page.rotation() {
            Rotation::None => (Affine::IDENTITY, (width, height)),
            // 時計回りに 90 度: (u, v) → (v, w − u)
            Rotation::Horizontal => (Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, width]), (height, width)),
            Rotation::Flipped => (Affine::new([-1.0, 0.0, 0.0, -1.0, width, height]), (width, height)),
            // 時計回りに 270 度: (u, v) → (h − v, u)
            Rotation::FlippedHorizontal => (Affine::new([0.0, 1.0, -1.0, 0.0, height, 0.0]), (height, width)),
        };
        Self { width: displayed.0, height: displayed.1, scale, user_space: Affine::scale(scale) * rotation * origin }
    }

    pub(crate) fn paper_width(&self) -> f64 {
        self.width * self.scale
    }

    pub(crate) fn paper_height(&self) -> f64 {
        self.height * self.scale
    }
}

/// 各ページのページラベル（12.4.2）。カタログの PageLabels の数の木から、ページの番号（0 から）以下でいちばん大きい
/// 開始ページの範囲の書き方で作る。ラベルのないページと、空のラベルは None
pub(crate) fn labels(catalog: Option<&Dict<'_>>, count: usize) -> Vec<Option<String>> {
    let mut ranges: Vec<(i64, Dict<'_>)> = Vec::new();
    if let Some(tree) = catalog.and_then(|catalog| catalog.get::<Dict<'_>>(b"PageLabels")) {
        let mut seen = HashSet::new();
        let mut nodes = 0;
        collect_ranges(&tree, 0, &mut nodes, &mut seen, &mut ranges);
    }
    ranges.sort_by_key(|(start, _)| *start);
    (0..count)
        .map(|index| {
            let (start, range) = ranges.iter().rev().find(|(start, _)| *start <= index as i64)?;
            Some(label(range, index as i64 - start)).filter(|label| !label.is_empty())
        })
        .collect()
}

fn collect_ranges<'a>(
    node: &Dict<'a>,
    depth: usize,
    nodes: &mut usize,
    seen: &mut HashSet<Vec<u8>>,
    out: &mut Vec<(i64, Dict<'a>)>,
) {
    *nodes += 1;
    if depth > MAX_TREE_DEPTH || *nodes > MAX_TREE_NODES || limits::tick() {
        return;
    }
    // 同じ節を 2 度たどらない（循環する木）
    if !seen.insert(node.data().to_vec()) {
        return;
    }
    if let Some(nums) = node.get::<Array<'_>>(b"Nums") {
        let mut items = nums.iter::<Object<'_>>();
        while let (Some(key), Some(value)) = (items.next(), items.next()) {
            if let (Object::Number(key), Object::Dict(range)) = (key, value) {
                out.push((key.as_i64(), range));
            }
        }
    }
    if let Some(kids) = node.get::<Array<'_>>(b"Kids") {
        for kid in kids.iter::<Dict<'_>>() {
            collect_ranges(&kid, depth + 1, nodes, seen, out);
        }
    }
}

/// 範囲の書き方（表 159）で、範囲の先頭から offset 番目のページのラベル。St は始めの番号（既定 1）、S は数の書き方、
/// P は前に付ける文字列
fn label(range: &Dict<'_>, offset: i64) -> String {
    let prefix =
        range.get::<PdfString<'_>>(b"P").map(|prefix| strings::text_string(prefix.as_bytes())).unwrap_or_default();
    let start = range.get::<i64>(b"St").filter(|start| *start >= 1).unwrap_or(1);
    let number = u64::try_from(start.saturating_add(offset)).unwrap_or(1).max(1);
    let style = range.get::<Name<'_>>(b"S");
    let digits = match style.as_deref() {
        Some(b"D") => number.to_string(),
        Some(b"R") => roman(number),
        Some(b"r") => roman(number).to_lowercase(),
        Some(b"A") => letters(number, b'A'),
        Some(b"a") => letters(number, b'a'),
        _ => String::new(),
    };
    format!("{prefix}{digits}")
}

/// ローマ数字（I、IV、MMXXVI …）
fn roman(mut number: u64) -> String {
    const NUMERALS: [(u64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    // 千の位が長くなりすぎるものは数字で書く
    if number / 1000 > MAX_LETTERS {
        return number.to_string();
    }
    let mut out = String::new();
    for (value, numeral) in NUMERALS {
        while number >= value {
            out.push_str(numeral);
            number -= value;
        }
    }
    out
}

/// A〜Z のあとは AA〜ZZ、AAA〜ZZZ …（表 159 の A と a）
fn letters(number: u64, first: u8) -> String {
    let repeat = (number - 1) / 26 + 1;
    if repeat > MAX_LETTERS {
        return number.to_string();
    }
    let letter = char::from(first + ((number - 1) % 26) as u8);
    std::iter::repeat_n(letter, repeat as usize).collect()
}

/// 用紙。大きさが JIS の A0〜A4 に合えばその名前と向き、合わなければ「自由」
pub(crate) fn sheet(name: &str, width: f64, height: f64) -> Sheet {
    const SIZES: [(&str, f64, f64); 5] =
        [("A0", 841.0, 1189.0), ("A1", 594.0, 841.0), ("A2", 420.0, 594.0), ("A3", 297.0, 420.0), ("A4", 210.0, 297.0)];
    // pt の整数で書いた A 列の用紙は 1 mm まではずれる（A4 は 595 × 842 pt）
    let near = |a: f64, b: f64| (a - b).abs() <= 1.0;
    let size = SIZES.iter().find_map(|(label, short, long)| {
        if near(width, *short) && near(height, *long) {
            Some(format!("{label} 縦"))
        } else if near(width, *long) && near(height, *short) {
            Some(format!("{label} 横"))
        } else {
            None
        }
    });
    Sheet { name: name.to_string(), size: size.unwrap_or_else(|| "自由".into()), width, height }
}

/// metadata の用紙の値（A3 横（420×297mm））
pub(crate) fn sheet_label(sheet: &Sheet) -> String {
    let size = |value: f64| js::format_ja(value, 1);
    format!("{}（{}×{}mm）", sheet.size, size(sheet.width), size(sheet.height))
}

/// 外観（AP）を持ち、隠し（F のビット 2）でない注釈の数（12.5）。pdf.js と hayro はこれらを下敷きのページに描く
pub(crate) fn annotation_count(page: &Page<'_>) -> u64 {
    let Some(annotations) = page.raw().get::<Array<'_>>(b"Annots") else {
        return 0;
    };
    annotations
        .iter::<Dict<'_>>()
        .filter(|annotation| annotation.get::<u32>(b"F").unwrap_or(0) & 2 == 0 && annotation.contains_key(b"AP"))
        .count() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_numbers_in_each_style() {
        assert_eq!(roman(1), "I");
        assert_eq!(roman(2026), "MMXXVI");
        assert_eq!(roman(49), "XLIX");
        assert_eq!(letters(1, b'A'), "A");
        assert_eq!(letters(26, b'a'), "z");
        assert_eq!(letters(27, b'A'), "AA");
        assert_eq!(letters(53, b'A'), "AAA");
        assert_eq!(letters(26 * 65, b'A'), (26 * 65).to_string());
    }

    #[test]
    fn names_the_a_series_by_size_and_orientation() {
        assert_eq!(sheet("1", 420.0, 297.0).size, "A3 横");
        assert_eq!(sheet("1", 209.9, 297.0).size, "A4 縦");
        assert_eq!(sheet("1", 215.9, 279.4).size, "自由");
        assert_eq!(sheet_label(&sheet("1", 841.0, 594.0)), "A1 横（841×594mm）");
    }
}
