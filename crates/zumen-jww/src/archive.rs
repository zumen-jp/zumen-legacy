//! MFC の CArchive の読み書き（数・文字列・件数・オブジェクトのタグ）。
//! 書き方は MFC の CArchive（AfxWriteStringLength・WriteCount・WriteObject・WriteClass）と同じにする
//! （MFC Technical Note TN002「Persistent Object Data Format」）。

use encoding_rs::{EncoderResult, SHIFT_JIS, UTF_16LE};
use std::collections::HashMap;
use std::ops::Range;
use zumen_core::ReadError;

pub(crate) type Result<T> = std::result::Result<T, ReadError>;

pub(crate) const END_OF_DATA: &str = "データが途中で終わっています";

/// 新しい図面の文字列を UTF-16LE で書く版。Jw_cad 10.01 以降は版 700 の文字列を UTF-16LE で書き、それより前の
/// Jw_cad は版 700 でも Shift_JIS で書く（読んだ図面は、その図面の書き方に合わせる）
pub(crate) const UNICODE_VERSION: u32 = 700;

/// 新しいクラスのタグ（wNewClassTag）
const NEW_CLASS_TAG: u16 = 0xffff;
/// クラスのタグの印（wClassTag）
const CLASS_TAG: u16 = 0x8000;
/// 4 バイトのタグが続く印（wBigObjectTag）
const BIG_OBJECT_TAG: u16 = 0x7fff;
/// 4 バイトのクラスのタグの印（dwBigClassTag）
const BIG_CLASS_TAG: u32 = 0x8000_0000;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8], start: usize) -> Self {
        Self { bytes, pos: start }
    }

    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    pub(crate) fn take(&mut self, length: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(length).filter(|end| *end <= self.bytes.len());
        let end = end.ok_or_else(|| ReadError::new(END_OF_DATA))?;
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn skip(&mut self, length: usize) -> Result<()> {
        self.take(length).map(|_| ())
    }

    pub(crate) fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub(crate) fn f64(&mut self) -> Result<f64> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    /// MFC の CString。長さは BYTE、0xFF なら WORD、0xFFFF なら DWORD。
    /// WORD の 0xFFFE は Unicode の印で、その後に長さを改めて書き、本文は UTF-16LE（Jw_cad 10.01 以降の JWW 700）
    pub(crate) fn cstring(&mut self) -> Result<String> {
        self.cstring_span().map(|(text, _)| text)
    }

    /// CString と、その書き方（長さと本文）のバイト列の範囲
    pub(crate) fn cstring_span(&mut self) -> Result<(String, Range<usize>)> {
        let start = self.pos;
        let mut unicode = false;
        let mut length = u32::from(self.u8()?);
        if length == 0xff {
            length = u32::from(self.u16()?);
            if length == 0xfffe {
                unicode = true;
                length = u32::from(self.u8()?);
                if length == 0xff {
                    length = u32::from(self.u16()?);
                    if length == 0xffff {
                        length = self.u32()?;
                    }
                }
            } else if length == 0xffff {
                length = self.u32()?;
            }
        }
        let size = if unicode { u64::from(length) * 2 } else { u64::from(length) };
        let slice = self.take(usize::try_from(size).map_err(|_| ReadError::new(END_OF_DATA))?)?;
        Ok((decode(slice, unicode), start..self.pos))
    }

    /// 件数（CObList の WriteCount）。WORD、0xFFFF なら DWORD
    pub(crate) fn count(&mut self) -> Result<u32> {
        let word = self.u16()?;
        if word == 0xffff { self.u32() } else { Ok(u32::from(word)) }
    }
}

/// 文字列の本文を復号する。TextDecoder と同じく、UTF-16LE は先頭の BOM を除き、Shift_JIS は WHATWG の規則で
/// 復号する。末尾の NUL は除く
fn decode(bytes: &[u8], unicode: bool) -> String {
    let text = if unicode {
        UTF_16LE.decode_with_bom_removal(bytes).0
    } else {
        SHIFT_JIS.decode_without_bom_handling(bytes).0
    };
    text.trim_end_matches('\0').to_string()
}

/// CString の書き方（長さと本文）を復号する
#[cfg(test)]
fn decode_cstring(bytes: &[u8]) -> Option<String> {
    let mut reader = Reader::new(bytes, 0);
    let (text, span) = reader.cstring_span().ok()?;
    (span.end == bytes.len()).then_some(text)
}

/// MFC と同じ書き方の CString。unicode なら UTF-16LE、そうでなければ Shift_JIS（表せない文字は ?）。
/// 表せなかった文字の数も返す
pub(crate) fn encode_cstring(text: &str, unicode: bool) -> (Vec<u8>, usize) {
    let mut out = Vec::with_capacity(text.len() * 2 + 4);
    let (body, units, replaced) = if unicode {
        let units: Vec<u16> = text.encode_utf16().collect();
        let body = units.iter().flat_map(|unit| unit.to_le_bytes()).collect::<Vec<u8>>();
        let count = units.len();
        (body, count, 0)
    } else {
        let (body, replaced) = encode_shift_jis(text);
        let count = body.len();
        (body, count, replaced)
    };
    if unicode {
        out.push(0xff);
        out.extend_from_slice(&0xfffe_u16.to_le_bytes());
    }
    // AfxWriteStringLength: 255 未満は BYTE、0xFFFE 未満は 0xFF と WORD、それ以上は 0xFF・0xFFFF と DWORD
    if units < 0xff {
        out.push(units as u8);
    } else if units < 0xfffe {
        out.push(0xff);
        out.extend_from_slice(&(units as u16).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&0xffff_u16.to_le_bytes());
        out.extend_from_slice(&(u32::try_from(units).unwrap_or(u32::MAX - 1)).to_le_bytes());
    }
    out.extend_from_slice(&body);
    (out, replaced)
}

/// Shift_JIS にする。表せない文字は ? にし、その数も返す
pub(crate) fn encode_shift_jis(text: &str) -> (Vec<u8>, usize) {
    let mut encoder = SHIFT_JIS.new_encoder();
    let mut out = Vec::with_capacity(text.len() * 2);
    let mut buffer = [0u8; 1024];
    let mut rest = text;
    let mut replaced = 0;
    loop {
        let (result, read, written) = encoder.encode_from_utf8_without_replacement(rest, &mut buffer, true);
        out.extend_from_slice(&buffer[..written]);
        rest = &rest[read..];
        match result {
            EncoderResult::InputEmpty => return (out, replaced),
            EncoderResult::OutputFull => {}
            EncoderResult::Unmappable(_) => {
                out.push(b'?');
                replaced += 1;
            }
        }
    }
}

/// 新しい図面を、その版で書くときに文字列を UTF-16LE にするか
pub(crate) fn unicode(version: u32) -> bool {
    version >= UNICODE_VERSION
}

pub(crate) struct Writer {
    pub(crate) bytes: Vec<u8>,
    /// 文字列を UTF-16LE で書く（そうでなければ Shift_JIS）
    pub(crate) unicode: bool,
    /// Shift_JIS で表せず ? にした文字の数
    pub(crate) replaced: usize,
}

impl Writer {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_unicode(false)
    }

    pub(crate) fn with_unicode(unicode: bool) -> Self {
        Self { bytes: Vec::new(), unicode, replaced: 0 }
    }

    pub(crate) fn raw(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub(crate) fn u16(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    pub(crate) fn i32(&mut self, value: i32) {
        self.raw(&value.to_le_bytes());
    }

    pub(crate) fn f64(&mut self, value: f64) {
        self.raw(&value.to_le_bytes());
    }

    pub(crate) fn f64s(&mut self, values: &[f64]) {
        for value in values {
            self.f64(*value);
        }
    }

    pub(crate) fn cstring(&mut self, text: &str, unicode: bool) {
        let (bytes, replaced) = encode_cstring(text, unicode);
        self.raw(&bytes);
        self.replaced += replaced;
    }

    /// 件数（WriteCount）。0xFFFF 未満は WORD、それ以上は 0xFFFF と DWORD
    pub(crate) fn count(&mut self, count: usize) {
        match u16::try_from(count) {
            Ok(word) if word < 0xffff => self.u16(word),
            _ => {
                self.u16(0xffff);
                self.u32(u32::try_from(count).unwrap_or(u32::MAX));
            }
        }
    }
}

/// オブジェクトの先頭のタグの書き方（読んだもの）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassTag {
    /// 新しいクラス（FF FF・スキーマ・名前）
    New { schema: u16 },
    /// 登録済みのクラスの番号。big は 4 バイトの書き方（7F FF の後に DWORD）
    Known { big: bool },
}

/// 読むときのクラスとオブジェクトの表。クラスとオブジェクトは同じ通し番号（1 から）を使い、
/// 図形の一覧・ブロック定義・ブロックの中の一覧で共有する
pub(crate) struct ReadTable {
    classes: HashMap<u32, (String, u16)>,
    next: u32,
}

impl ReadTable {
    pub(crate) fn new() -> Self {
        Self { classes: HashMap::new(), next: 1 }
    }

    /// オブジェクトの番号を 1 つ進める（オブジェクトを読むたび）
    pub(crate) fn reserve(&mut self) {
        self.next = self.next.wrapping_add(1);
    }

    /// オブジェクトの先頭のタグ。新しいクラスならクラス名、既知のクラスならその名前、空なら None
    pub(crate) fn read_class(&mut self, reader: &mut Reader<'_>) -> Result<Option<(String, u16, ClassTag)>> {
        let word = reader.u16()?;
        if word == NEW_CLASS_TAG {
            let schema = reader.u16()?;
            let length = reader.u16()?;
            let mut name = String::with_capacity(usize::from(length));
            for _ in 0..length {
                name.push(char::from(reader.u8()?));
            }
            self.classes.insert(self.next, (name.clone(), schema));
            self.next = self.next.wrapping_add(1);
            return Ok(Some((name, schema, ClassTag::New { schema })));
        }
        let big = word == BIG_OBJECT_TAG;
        let tag = if big { reader.u32()? } else { (u32::from(word & CLASS_TAG) << 16) | u32::from(word & 0x7fff) };
        if tag & BIG_CLASS_TAG != 0 {
            let pid = tag & !BIG_CLASS_TAG;
            if pid == 0 {
                return Ok(None);
            }
            let (name, schema) =
                self.classes.get(&pid).ok_or_else(|| ReadError::new(format!("未登録のクラス番号 {pid}")))?;
            return Ok(Some((name.clone(), *schema, ClassTag::Known { big })));
        }
        if tag == 0 {
            return Ok(None);
        }
        Err(ReadError::new("既に読んだオブジェクトへの参照には対応していません"))
    }
}

/// 書くときのクラスとオブジェクトの表（MFC の m_pStoreMap と同じ番号の振り方）
pub(crate) struct WriteTable {
    classes: HashMap<&'static str, u32>,
    next: u32,
}

impl WriteTable {
    pub(crate) fn new() -> Self {
        Self { classes: HashMap::new(), next: 1 }
    }

    /// オブジェクトの先頭にクラスのタグを書き、オブジェクトの番号を取る（WriteObject・WriteClass）。
    /// 初めてのクラスは名前とスキーマを書いて登録し、登録済みならその番号を書く。
    /// 読んだタグ（hint）が新しいクラスなら、元と同じく登録し直す
    pub(crate) fn write_object(&mut self, out: &mut Writer, class: &'static str, schema: u16, hint: Option<ClassTag>) {
        let known = self.classes.get(class).copied();
        match (known, hint) {
            (Some(index), None | Some(ClassTag::Known { .. })) => {
                let big = matches!(hint, Some(ClassTag::Known { big: true }));
                if index < u32::from(BIG_OBJECT_TAG) && !big {
                    out.u16(CLASS_TAG | index as u16);
                } else {
                    out.u16(BIG_OBJECT_TAG);
                    out.u32(BIG_CLASS_TAG | index);
                }
            }
            (_, hint) => {
                let schema = match hint {
                    Some(ClassTag::New { schema }) => schema,
                    _ => schema,
                };
                out.u16(NEW_CLASS_TAG);
                out.u16(schema);
                out.u16(class.len() as u16);
                out.raw(class.as_bytes());
                self.classes.insert(class, self.next);
                self.next = self.next.wrapping_add(1);
            }
        }
        // オブジェクトの番号（中身を書く前に取る）
        self.next = self.next.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(text: &str, unicode: bool) -> String {
        let (bytes, _) = encode_cstring(text, unicode);
        decode_cstring(&bytes).unwrap()
    }

    #[test]
    fn writes_string_lengths_like_mfc() {
        // 255 未満は BYTE、255 以上は 0xFF と WORD、0xFFFE 以上は 0xFF・0xFFFF と DWORD
        assert_eq!(encode_cstring("ab", false).0, [2, b'a', b'b']);
        let long = "a".repeat(255);
        assert_eq!(&encode_cstring(&long, false).0[..3], [0xff, 0xff, 0x00]);
        let huge = "a".repeat(0xfffe);
        assert_eq!(&encode_cstring(&huge, false).0[..7], [0xff, 0xff, 0xff, 0xfe, 0xff, 0x00, 0x00]);
        // Unicode は FF FE FF の後に文字数
        assert_eq!(encode_cstring("メ", true).0, [0xff, 0xfe, 0xff, 1, 0xe1, 0x30]);
        let long = "メ".repeat(300);
        assert_eq!(&encode_cstring(&long, true).0[..6], [0xff, 0xfe, 0xff, 0xff, 0x2c, 0x01]);
        for text in ["", "通り芯", &"長".repeat(254), &"長".repeat(255), &"長".repeat(0xfffe), "𠮷"] {
            assert_eq!(roundtrip(text, true), text);
        }
        for text in ["", "通り芯", "ｶﾅ", &"a".repeat(0xfffd), &"a".repeat(0xfffe), &"a".repeat(0x1_0000)] {
            assert_eq!(roundtrip(text, false), text);
        }
    }

    #[test]
    fn replaces_characters_that_shift_jis_cannot_hold() {
        let (bytes, replaced) = encode_cstring("a𠮷b", false);
        assert_eq!((bytes, replaced), (vec![3, b'a', b'?', b'b'], 1));
    }

    #[test]
    fn numbers_classes_and_objects_like_mfc() {
        let mut out = Writer::new();
        let mut table = WriteTable::new();
        // 1: CDataSen のクラス、2: 線、3: 線、4: CDataMoji のクラス、5: 文字、6: 線
        table.write_object(&mut out, "CDataSen", 600, None);
        table.write_object(&mut out, "CDataSen", 600, None);
        table.write_object(&mut out, "CDataMoji", 600, None);
        table.write_object(&mut out, "CDataSen", 600, None);
        let mut expected = vec![0xff, 0xff, 0x58, 0x02, 8, 0];
        expected.extend_from_slice(b"CDataSen");
        expected.extend_from_slice(&[0x01, 0x80, 0xff, 0xff, 0x58, 0x02, 9, 0]);
        expected.extend_from_slice(b"CDataMoji");
        expected.extend_from_slice(&[0x01, 0x80]);
        assert_eq!(out.bytes, expected);
        let mut reader = Reader::new(&out.bytes, 0);
        let mut read = ReadTable::new();
        let mut names = Vec::new();
        while reader.remaining() > 0 {
            let (name, schema, _) = read.read_class(&mut reader).unwrap().unwrap();
            names.push((name, schema));
            read.reserve();
        }
        assert_eq!(names.len(), 4);
        assert_eq!(names[3], ("CDataSen".to_string(), 600));
    }

    #[test]
    fn uses_four_byte_tags_from_number_32767() {
        let mut out = Writer::new();
        let mut table = WriteTable::new();
        // 1: CDataSen のクラス、2〜0x7FFE: 線
        table.write_object(&mut out, "CDataSen", 700, None);
        for _ in 0..0x7ffc {
            table.write_object(&mut out, "CDataSen", 700, None);
        }
        // 0x7FFF: CDataEnko のクラス。番号 0x7FFF 以上のクラスは 4 バイトの書き方で参照する
        table.write_object(&mut out, "CDataEnko", 700, None);
        let start = out.bytes.len();
        table.write_object(&mut out, "CDataEnko", 700, None);
        assert_eq!(out.bytes[start..], [0xff, 0x7f, 0xff, 0x7f, 0x00, 0x80]);
    }

    #[test]
    fn writes_counts_like_mfc() {
        let mut out = Writer::new();
        out.count(0xfffe);
        out.count(0xffff);
        assert_eq!(out.bytes, [0xfe, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00]);
        let mut reader = Reader::new(&out.bytes, 0);
        assert_eq!((reader.count().unwrap(), reader.count().unwrap()), (0xfffe, 0xffff));
    }
}
