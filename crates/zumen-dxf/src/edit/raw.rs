//! その場の編集のための DXF のタグ。読み取り（tags.rs）と同じ規則でタグに分けるが、各タグの元のバイト列の範囲を
//! 保ち、変えていないタグはそのまま書き戻す。変えたタグと足したタグだけを、図面と同じ形（テキストかバイナリか、
//! 改行、文字コード）で書く。

use crate::tags::{BINARY_SENTINEL, Kind, decode, encoding, kind_of, latin1, source_encoding};
use std::ops::Range;
use zumen_core::js;

/// グループコードとして読めなかった行（読み取りと同じく飛ばし、書くときはそのまま写す）
pub(crate) const OPAQUE: i32 = i32::MIN;

/// 1 つのタグ。raw は元のバイト列の範囲で、変えたタグ・足したタグは None
#[derive(Debug, Clone)]
pub(crate) struct Tag {
    pub code: i32,
    /// 文字列は図面の文字コードで復号した値。数はテキストの DXF なら元の文字列、バイナリなら数の文字列
    pub value: String,
    pub raw: Option<Range<usize>>,
}

impl Tag {
    pub fn new(code: i32, value: impl Into<String>) -> Self {
        Self { code, value: value.into(), raw: None }
    }

    /// 前後の空白を除いた値
    pub fn text(&self) -> &str {
        self.value.trim()
    }

    pub fn number(&self) -> f64 {
        js::string_to_number(self.text())
    }
}

/// 図面のタグの並びと、書き戻すための形
pub(crate) struct RawDxf<'a> {
    pub bytes: &'a [u8],
    pub version: String,
    pub binary: bool,
    /// R12 のバイナリ DXF（1 バイトのグループコード）
    r12_binary: bool,
    /// 文字列の文字コード（TextDecoder の名前）
    pub encoding: &'static str,
    /// テキストの DXF の改行
    line_end: &'static [u8],
    /// 先頭の BOM・バイナリの見出し
    prefix: Range<usize>,
    pub tags: Vec<Tag>,
    /// 最後のタグの後ろ（読めなかった末尾）
    trailer: Range<usize>,
}

impl<'a> RawDxf<'a> {
    pub fn parse(bytes: &'a [u8]) -> Self {
        let (version, binary, label) = source_encoding(bytes);
        if binary {
            let r12_binary = version.as_str() <= "AC1009";
            let mut tags = Vec::new();
            let mut pos = BINARY_SENTINEL.len();
            while pos < bytes.len() {
                match binary_tag(bytes, pos, r12_binary, label) {
                    Some((tag, next)) => {
                        tags.push(tag);
                        pos = next;
                    }
                    None => break,
                }
            }
            return Self {
                bytes,
                version,
                binary,
                r12_binary,
                encoding: label,
                line_end: b"\r\n",
                prefix: 0..BINARY_SENTINEL.len(),
                tags,
                trailer: pos..bytes.len(),
            };
        }
        let separator = if bytes.contains(&b'\n') { b'\n' } else { b'\r' };
        let start = if bytes.starts_with(b"\xef\xbb\xbf") { 3 } else { 0 };
        let mut lines = Lines { bytes, pos: start, separator };
        let mut tags = Vec::new();
        let mut line_end: Option<&'static [u8]> = None;
        let mut end = start;
        while let Some((code_start, code_line, after_code)) = lines.next() {
            if line_end.is_none() {
                line_end = Some(match separator {
                    b'\n' if bytes[..after_code].ends_with(b"\r\n") => b"\r\n",
                    b'\n' => b"\n",
                    _ => b"\r",
                });
            }
            let code = js::parse_int(&latin1(code_line));
            if !code.is_finite() {
                // 1 行ずれた図面は、次の行から読み直す（読み取りと同じ）
                tags.push(Tag { code: OPAQUE, value: String::new(), raw: Some(code_start..after_code) });
                end = after_code;
                continue;
            }
            let Some((_, value_line, after_value)) = lines.next() else {
                break;
            };
            let value = if value_line.is_ascii() { latin1(value_line) } else { decode(value_line, label) };
            tags.push(Tag { code: js::to_int32(code), value, raw: Some(code_start..after_value) });
            end = after_value;
        }
        Self {
            bytes,
            version,
            binary,
            r12_binary: false,
            encoding: label,
            line_end: line_end.unwrap_or(b"\r\n"),
            prefix: 0..start,
            tags,
            trailer: end..bytes.len(),
        }
    }

    /// AutoCAD 2007（AC1021）以降か（文字列は UTF-8）
    pub fn utf8(&self) -> bool {
        self.version.as_str() >= "AC1021" || self.encoding == "utf-8"
    }

    pub fn write_prefix(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.bytes[self.prefix.clone()]);
    }

    pub fn write_trailer(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.bytes[self.trailer.clone()]);
    }

    /// タグを書く。元のタグは元のバイト列のまま
    pub fn write_tag(&self, tag: &Tag, out: &mut Vec<u8>) {
        if let Some(raw) = &tag.raw {
            out.extend_from_slice(&self.bytes[raw.clone()]);
            return;
        }
        if self.binary {
            self.write_binary(tag, out);
        } else {
            out.extend_from_slice(format!("{:>3}", tag.code).as_bytes());
            out.extend_from_slice(self.line_end);
            let value = if matches!(kind_of(f64::from(tag.code)), Kind::String) {
                caret(&tag.value)
            } else {
                tag.value.clone()
            };
            out.extend_from_slice(&self.encode(&value));
            out.extend_from_slice(self.line_end);
        }
    }

    fn write_binary(&self, tag: &Tag, out: &mut Vec<u8>) {
        if self.r12_binary {
            if tag.code >= 255 {
                out.push(255);
                out.extend_from_slice(&(tag.code as u16).to_le_bytes());
            } else {
                out.push(tag.code as u8);
            }
        } else {
            out.extend_from_slice(&(tag.code as u16).to_le_bytes());
        }
        let number = || js::string_to_number(tag.text());
        let integer = || {
            let value = number();
            if value.is_finite() { value.trunc() as i64 } else { 0 }
        };
        match kind_of(f64::from(tag.code)) {
            Kind::Float => out.extend_from_slice(&number().to_le_bytes()),
            Kind::Int16 => out.extend_from_slice(&(integer() as i16).to_le_bytes()),
            Kind::Int32 => out.extend_from_slice(&(integer() as i32).to_le_bytes()),
            Kind::Int64 => out.extend_from_slice(&integer().to_le_bytes()),
            Kind::Bool => out.push(u8::from(integer() != 0)),
            Kind::Binary => {
                let data: Vec<u8> = (0..tag.value.len() / 2)
                    .filter_map(|index| u8::from_str_radix(tag.value.get(index * 2..index * 2 + 2)?, 16).ok())
                    .take(255)
                    .collect();
                out.push(data.len() as u8);
                out.extend_from_slice(&data);
            }
            Kind::String => {
                out.extend_from_slice(&self.encode(&tag.value));
                out.push(0);
            }
        }
    }

    /// 文字列を図面の文字コードにする。AutoCAD 2007 より前の文字コードで表せない文字は \U+XXXX にする
    /// （DXF Reference「Storage of String Values」）
    pub fn encode(&self, text: &str) -> Vec<u8> {
        if self.utf8() {
            return text.as_bytes().to_vec();
        }
        let encoder = encoding(self.encoding);
        let mut out = Vec::with_capacity(text.len());
        let mut buffer = [0u8; 4];
        for c in text.chars() {
            if c.is_ascii() {
                out.push(c as u8);
                continue;
            }
            let (bytes, _, unmappable) = encoder.encode(c.encode_utf8(&mut buffer));
            if unmappable {
                let code = u32::from(c);
                out.extend_from_slice(
                    if code <= 0xffff { format!("\\U+{code:04X}") } else { "?".to_string() }.as_bytes(),
                );
            } else {
                out.extend_from_slice(&bytes);
            }
        }
        out
    }
}

/// テキストの DXF の文字列の書き方（DXF Reference「About ASCII Control Characters in DXF Files」）。
/// 制御文字は ^ と 0x40 を足した文字（改行は ^J）、^ は「^ 」
fn caret(text: &str) -> String {
    if !text.chars().any(|c| c < ' ' || c == '^') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 4);
    for c in text.chars() {
        if c < ' ' {
            out.push('^');
            out.push(char::from(c as u8 + 0x40));
        } else if c == '^' {
            out.push_str("^ ");
        } else {
            out.push(c);
        }
    }
    out
}

/// テキストの DXF の行。行の始まり、改行を除いた行、改行の後の位置を返す
struct Lines<'a> {
    bytes: &'a [u8],
    pos: usize,
    separator: u8,
}

impl<'a> Lines<'a> {
    fn next(&mut self) -> Option<(usize, &'a [u8], usize)> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        let end = self.bytes[start..]
            .iter()
            .position(|byte| *byte == self.separator)
            .map_or(self.bytes.len(), |at| start + at);
        let mut line = &self.bytes[start..end];
        if self.separator == b'\n' {
            line = line.strip_suffix(b"\r").unwrap_or(line);
        }
        self.pos = if end < self.bytes.len() { end + 1 } else { end };
        Some((start, line, self.pos))
    }
}

/// バイナリの DXF の 1 タグ（読み取りの BinaryTags と同じ規則）。途中で終わっていれば None
fn binary_tag(bytes: &[u8], pos: usize, r12: bool, label: &str) -> Option<(Tag, usize)> {
    let mut at = pos;
    let code = if r12 {
        let code = i32::from(*bytes.get(at)?);
        at += 1;
        if code == 255 { i32::from(u16::from_le_bytes(take::<2>(bytes, &mut at)?)) } else { code }
    } else {
        i32::from(u16::from_le_bytes(take::<2>(bytes, &mut at)?))
    };
    let value = match kind_of(f64::from(code)) {
        Kind::Float => js::number_to_string(f64::from_le_bytes(take::<8>(bytes, &mut at)?)),
        Kind::Int16 => i16::from_le_bytes(take::<2>(bytes, &mut at)?).to_string(),
        Kind::Int32 => i32::from_le_bytes(take::<4>(bytes, &mut at)?).to_string(),
        Kind::Int64 => i64::from_le_bytes(take::<8>(bytes, &mut at)?).to_string(),
        Kind::Bool => take::<1>(bytes, &mut at)?[0].to_string(),
        Kind::Binary => {
            let size = usize::from(take::<1>(bytes, &mut at)?[0]);
            let data = bytes.get(at..at + size)?;
            at += size;
            data.iter().map(|byte| format!("{byte:02X}")).collect()
        }
        Kind::String => {
            let start = at;
            let end = bytes.get(start..)?.iter().position(|byte| *byte == 0).map(|offset| start + offset)?;
            at = end + 1;
            let slice = &bytes[start..end];
            if slice.is_ascii() { latin1(slice) } else { decode(slice, label) }
        }
    };
    Some((Tag { code, value, raw: Some(pos..at) }, at))
}

fn take<const N: usize>(bytes: &[u8], at: &mut usize) -> Option<[u8; N]> {
    let slice = bytes.get(*at..*at + N)?;
    *at += N;
    slice.try_into().ok()
}
