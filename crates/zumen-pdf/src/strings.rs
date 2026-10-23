//! PDF の文字列と名前を人に見せる文字列にする

use hayro_interpret::hayro_syntax::object::DateTime;

/// テキスト文字列（7.9.2.2）。先頭が FE FF なら UTF-16BE、EF BB BF なら UTF-8（PDF 2.0）、それ以外は PDFDocEncoding
/// （Annex D の表 D.2）
pub(crate) fn text_string(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units = rest.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair));
        return char::decode_utf16(units).map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER)).collect();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    bytes.iter().map(|byte| pdf_doc_char(*byte)).collect()
}

/// PDFDocEncoding の 1 バイト。ISO 8859-1 と違うところだけを表に持つ。定義のないバイトは U+FFFD
fn pdf_doc_char(byte: u8) -> char {
    const LOW: [char; 8] = ['˘', 'ˇ', 'ˆ', '˙', '˝', '˛', '˚', '˜'];
    const HIGH: [char; 33] = [
        '•', '†', '‡', '…', '—', '–', 'ƒ', '⁄', '‹', '›', '−', '‰', '„', '“', '”', '‘', '’', '‚', '™', 'ﬁ', 'ﬂ', 'Ł',
        'Œ', 'Š', 'Ÿ', 'Ž', 'ı', 'ł', 'œ', 'š', 'ž', '\u{fffd}', '€',
    ];
    match byte {
        0x18..=0x1f => LOW[usize::from(byte - 0x18)],
        0x80..=0xa0 => HIGH[usize::from(byte - 0x80)],
        0x7f | 0xad => char::REPLACEMENT_CHARACTER,
        _ => char::from(byte),
    }
}

/// 名前（7.3.5）のバイト列。UTF-8 でなければ Shift_JIS（CP932）とみなす（日本語のフォント名を Shift_JIS のまま書く
/// 作図ソフトがある）
pub(crate) fn name(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode_without_bom_handling(bytes).0.into_owned(),
    }
}

/// 日付（7.9.4）。2026-10-23 09:30:00+09:00 の形
pub(crate) fn date(date: &DateTime) -> String {
    let sign = if date.utc_offset_hour < 0 { '-' } else { '+' };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}{sign}{:02}:{:02}",
        date.year,
        date.month,
        date.day,
        date.hour,
        date.minute,
        date.second,
        date.utc_offset_hour.unsigned_abs(),
        date.utc_offset_minute
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_text_strings_and_names() {
        assert_eq!(text_string(b"Plan \x84 A"), "Plan — A");
        assert_eq!(text_string(&[0xfe, 0xff, 0x56, 0xf3, 0x97, 0x62]), "図面");
        assert_eq!(text_string(&[0xef, 0xbb, 0xbf, 0xe5, 0x9b, 0xb3]), "図");
        assert_eq!(text_string(b"\x18\xa0\xe9"), "˘€é");
        assert_eq!(name(b"Helvetica"), "Helvetica");
        // ＭＳ ゴシック（Shift_JIS）
        assert_eq!(
            name(&[0x82, 0x6c, 0x82, 0x72, 0x20, 0x83, 0x53, 0x83, 0x56, 0x83, 0x62, 0x83, 0x4e]),
            "ＭＳ ゴシック"
        );
    }
}
