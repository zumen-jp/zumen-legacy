//! 足す文字の位置合わせと行間（docs/decisions/0017）。DXF と DWG の編集が、TEXT の水平・垂直の位置合わせ（72・73）と
//! MTEXT の配置（71）・行間（44）を同じ値から決める。

use zumen_core::edit::{EditError, ValueReader};

/// 文字の値のキー（位置合わせ・行間・複数行）
pub const KEYS: [&str; 3] = ["justification", "lineSpacing", "multiline"];

/// 位置合わせの名前と、TEXT の 72・73（水平 0 左・1 中央・2 右、垂直 0 ベースライン・1 下・2 中・3 上）、MTEXT の 71
/// （1 左上〜9 右下）。ベースラインの left・center・right は、MTEXT では下の段
const JUSTIFICATIONS: [(&str, i64, i64, i64); 12] = [
    ("left", 0, 0, 7),
    ("center", 1, 0, 8),
    ("right", 2, 0, 9),
    ("bottom-left", 0, 1, 7),
    ("bottom-center", 1, 1, 8),
    ("bottom-right", 2, 1, 9),
    ("middle-left", 0, 2, 4),
    ("middle-center", 1, 2, 5),
    ("middle-right", 2, 2, 6),
    ("top-left", 0, 3, 1),
    ("top-center", 1, 3, 2),
    ("top-right", 2, 3, 3),
];

/// 文字の置き方
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// TEXT の 72・73
    pub horizontal: i64,
    pub vertical: i64,
    /// MTEXT の 71
    pub attachment: i64,
    /// MTEXT の行間の倍率（44）。None は 1
    pub line_spacing: Option<f64>,
    /// MTEXT にする（multiline、または行間の指定）
    pub multiline: bool,
}

impl Placement {
    /// TEXT が位置合わせの点（11）で置かれる（左・ベースラインでない）
    pub fn aligned(&self) -> bool {
        self.horizontal != 0 || self.vertical != 0
    }
}

/// justification・lineSpacing・multiline を読む
pub fn read(reader: &ValueReader<'_>, index: usize) -> Result<Placement, EditError> {
    let (horizontal, vertical, attachment) = match reader.text("justification")? {
        None => (0, 0, 7),
        Some(name) => JUSTIFICATIONS
            .iter()
            .find(|(known, ..)| known.eq_ignore_ascii_case(name.trim()))
            .map(|(_, horizontal, vertical, attachment)| (*horizontal, *vertical, *attachment))
            .ok_or_else(|| {
                let names: Vec<&str> = JUSTIFICATIONS.iter().map(|(name, ..)| *name).collect();
                EditError::at(
                    index,
                    "invalid-value",
                    format!("justification の {name} はありません（{}）", names.join("、")),
                )
            })?,
    };
    let line_spacing = reader.number("lineSpacing")?;
    if line_spacing.is_some_and(|factor| !(0.25..=4.0).contains(&factor)) {
        return Err(EditError::at(index, "invalid-value", "lineSpacing は 0.25〜4 の倍率にしてください"));
    }
    let multiline = reader.bool("multiline")?.unwrap_or(false) || line_spacing.is_some();
    Ok(Placement { horizontal, vertical, attachment, line_spacing, multiline })
}

/// 幅の係数（widthFactor、TEXT の 41）。1 は省く
pub fn width_factor(reader: &ValueReader<'_>) -> Result<Option<f64>, EditError> {
    Ok(reader.positive("widthFactor")?.filter(|factor| *factor != 1.0))
}

/// MTEXT の内容の頭に幅の係数の書式（\W…;）を付ける。MTEXT には幅の係数の値がないため
pub fn with_width_factor(content: String, factor: Option<f64>) -> String {
    match factor {
        Some(factor) => format!("\\W{};{content}", trim_number(factor)),
        None => content,
    }
}

fn trim_number(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

