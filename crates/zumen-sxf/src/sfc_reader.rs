//! SFC ファイル（SXF のフィーチャコメントファイル）のバイト列を、フィーチャのレコードに分ける
//! （`@zumenjs/sxf` の sfc-reader.ts）。構造は ASCII で、文字列だけが CP932（Shift_JIS）なので、
//! バイト単位で読んでから文字列を復号する。仕様: SXF Ver.3.1 仕様書・同解説 附属書 SFC 編（第 2 版）1-1。

use crate::header::{SxfHeader, read_header};
use encoding_rs::SHIFT_JIS;
use std::convert::Infallible;
use zumen_core::js;

/// 引数は、引用符で囲んだ値（数値・集合の文字列）、文字列、括弧の集合のいずれか
#[derive(Debug, Clone, PartialEq)]
pub enum SfcValue {
    Text(String),
    List(Vec<SfcValue>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfcRecord {
    /// インスタンス番号（#nn の nn）
    pub id: f64,
    /// フィーチャ名（小文字）
    pub name: String,
    /// /*SXF、/*SXF3、/*SXF3.1 のどれで囲まれていたか
    pub version: &'static str,
    pub params: Vec<SfcValue>,
    /// ファイルの中の位置（/*SXF の先頭から SXF*/ の終わりまで）。書き出しが元の文字列を残すのに使う
    pub span: (usize, usize),
}

pub struct SfcReadResult {
    pub header: SxfHeader,
    pub records: Vec<SfcRecord>,
    /// 読めなかったフィーチャの数
    pub skipped: u64,
}

const BACKSLASH: u8 = 0x5c;
const QUOTE: u8 = 0x27;
const COMMA: u8 = 0x2c;
const OPEN: u8 = 0x28;
const CLOSE: u8 = 0x29;
/// 引数の集合の入れ子の上限。仕様の書き方は 2 段までで、深すぎるものは読めないフィーチャとして省く
const MAX_NESTING: usize = 64;

fn is_space(byte: Option<&u8>) -> bool {
    matches!(byte, Some(0x20 | 0x09 | 0x0a | 0x0d))
}

/// Shift_JIS の 2 バイト文字の先頭バイト
pub(crate) fn is_lead_byte(byte: u8) -> bool {
    (0x81..=0x9f).contains(&byte) || (0xe0..=0xfc).contains(&byte)
}

/// TextDecoder("shift_jis") と同じ WHATWG の復号
pub(crate) fn decode(bytes: &[u8]) -> String {
    SHIFT_JIS.decode_without_bom_handling(bytes).0.into_owned()
}

fn starts_with(bytes: &[u8], at: usize, ascii: &[u8]) -> bool {
    bytes.get(at..).is_some_and(|rest| rest.starts_with(ascii))
}

fn index_of(bytes: &[u8], ascii: &[u8], from: usize) -> Option<usize> {
    bytes.get(from..)?.windows(ascii.len()).position(|window| window == ascii).map(|offset| from + offset)
}

fn without_line_breaks(text: String) -> String {
    if text.contains(['\r', '\n']) { text.replace(['\r', '\n'], "") } else { text }
}

struct ParseError;

/// レコードの引数を読む小さな字句解析器
struct ParamReader<'a> {
    bytes: &'a [u8],
    pos: usize,
    end: usize,
}

impl ParamReader<'_> {
    fn skip_space(&mut self) {
        while self.pos < self.end && is_space(self.bytes.get(self.pos)) {
            self.pos += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        if self.pos < self.end { self.bytes.get(self.pos).copied() } else { None }
    }

    /// "(" の直後から、対応する ")" までの引数の並びを読む
    fn read_list(&mut self, depth: usize) -> Result<Vec<SfcValue>, ParseError> {
        if depth > MAX_NESTING {
            return Err(ParseError);
        }
        let mut values = Vec::new();
        if self.peek() == Some(CLOSE) {
            self.pos += 1;
            return Ok(values);
        }
        loop {
            values.push(self.read_value(depth)?);
            let next = self.peek();
            self.pos += 1;
            match next {
                Some(COMMA) => {}
                Some(CLOSE) => return Ok(values),
                _ => return Err(ParseError),
            }
        }
    }

    fn read_value(&mut self, depth: usize) -> Result<SfcValue, ParseError> {
        let byte = self.peek().ok_or(ParseError)?;
        if byte == BACKSLASH && self.bytes.get(self.pos + 1) == Some(&QUOTE) {
            return self.read_text().map(SfcValue::Text);
        }
        if byte == QUOTE {
            return self.read_quoted();
        }
        if byte == OPEN {
            self.pos += 1;
            return self.read_list(depth + 1).map(SfcValue::List);
        }
        let start = self.pos;
        while self.pos < self.end && self.bytes[self.pos] != COMMA && self.bytes[self.pos] != CLOSE {
            self.pos += 1;
        }
        Ok(SfcValue::Text(js::trim(&decode(&self.bytes[start..self.pos])).to_string()))
    }

    /// '…'。数値や '(1,2,3)' の集合。中身は ASCII
    fn read_quoted(&mut self) -> Result<SfcValue, ParseError> {
        self.pos += 1;
        let start = self.pos;
        let close = self.bytes[start.min(self.bytes.len())..]
            .iter()
            .position(|byte| *byte == QUOTE)
            .map(|offset| start + offset)
            .filter(|close| *close < self.end)
            .ok_or(ParseError)?;
        self.pos = close + 1;
        let raw = without_line_breaks(decode(&self.bytes[start..close]));
        let trimmed = js::trim(&raw);
        if trimmed.starts_with('(') && trimmed.ends_with(')') {
            let inner = js::trim(&trimmed[1..trimmed.len() - 1]);
            if inner.is_empty() {
                return Ok(SfcValue::List(Vec::new()));
            }
            return Ok(SfcValue::List(
                inner.split(',').map(|item| SfcValue::Text(js::trim(item).to_string())).collect(),
            ));
        }
        Ok(SfcValue::Text(trimmed.to_string()))
    }

    /// \'…\'。終わりは \' の後に , か ) が続く位置。\\ は \ 1 文字。2 バイト文字の 2 バイト目は区切らない
    fn read_text(&mut self) -> Result<String, ParseError> {
        self.pos += 2;
        let mut out = Vec::new();
        while self.pos < self.end {
            let byte = self.bytes[self.pos];
            if is_lead_byte(byte) && self.pos + 1 < self.end {
                out.extend_from_slice(&self.bytes[self.pos..self.pos + 2]);
                self.pos += 2;
                continue;
            }
            if byte == BACKSLASH {
                let next = self.bytes.get(self.pos + 1).copied();
                if next == Some(BACKSLASH) {
                    out.push(BACKSLASH);
                    self.pos += 2;
                    continue;
                }
                if next == Some(QUOTE) {
                    let mut after = self.pos + 2;
                    while after < self.end && is_space(self.bytes.get(after)) {
                        after += 1;
                    }
                    let terminator = self.bytes.get(after).copied();
                    if terminator == Some(COMMA) || terminator == Some(CLOSE) || after >= self.end {
                        self.pos += 2;
                        return Ok(without_line_breaks(decode(&out)));
                    }
                }
            }
            if byte != 0x0a && byte != 0x0d {
                out.push(byte);
            }
            self.pos += 1;
        }
        Err(ParseError)
    }
}

struct Opener {
    marker: &'static [u8],
    closer: &'static [u8],
    version: &'static str,
}

const OPENERS: [Opener; 3] = [
    Opener { marker: b"/*SXF3.1", closer: b"SXF3.1*/", version: "3.1" },
    Opener { marker: b"/*SXF3", closer: b"SXF3*/", version: "3" },
    Opener { marker: b"/*SXF", closer: b"SXF*/", version: "2" },
];

fn read_header_strings(bytes: &[u8]) -> SxfHeader {
    let end = match index_of(bytes, b"DATA;", 0) {
        Some(end) if end > 0 => end,
        _ => bytes.len().min(4096),
    };
    let text = decode(&bytes[..end]);
    let Ok(header) = read_header::<Infallible>(&text, |value| Ok(value.replace("''", "'")));
    header
}

pub fn read_sfc(bytes: &[u8]) -> SfcReadResult {
    let mut records = Vec::new();
    let mut skipped = 0;
    let mut pos = 0;
    while let Some(start) = index_of(bytes, b"/*SXF", pos) {
        // 読み取りの上限（zumen_core::limits）
        if zumen_core::limits::tick() {
            break;
        }
        let opener =
            OPENERS.iter().find(|candidate| starts_with(bytes, start, candidate.marker)).unwrap_or(&OPENERS[2]);
        let Some(close) = index_of(bytes, opener.closer, start + opener.marker.len()) else {
            skipped += 1;
            break;
        };
        pos = close + opener.closer.len();
        match read_record(bytes, start + opener.marker.len(), close, opener.version) {
            Ok(Some(mut record)) => {
                record.span = (start, pos);
                records.push(record);
            }
            Ok(None) | Err(ParseError) => skipped += 1,
        }
    }
    SfcReadResult { header: read_header_strings(bytes), records, skipped }
}

fn read_record(bytes: &[u8], from: usize, to: usize, version: &'static str) -> Result<Option<SfcRecord>, ParseError> {
    let mut i = from;
    let skip = |mut i: usize| {
        while i < to && is_space(bytes.get(i)) {
            i += 1;
        }
        i
    };
    i = skip(i);
    if bytes.get(i) != Some(&b'#') {
        return Ok(None);
    }
    i += 1;
    let mut id = 0.0;
    let mut digits = 0;
    while i < to && bytes[i].is_ascii_digit() {
        id = id * 10.0 + f64::from(bytes[i] - b'0');
        i += 1;
        digits += 1;
    }
    if digits == 0 {
        return Ok(None);
    }
    i = skip(i);
    if bytes.get(i) != Some(&b'=') {
        return Ok(None);
    }
    i = skip(i + 1);
    let name_start = i;
    while i < to && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
        i += 1;
    }
    let name = String::from_utf8_lossy(&bytes[name_start..i]).to_ascii_lowercase();
    if name.is_empty() {
        return Ok(None);
    }
    i = skip(i);
    if bytes.get(i) != Some(&OPEN) {
        return Ok(None);
    }
    let mut reader = ParamReader { bytes, pos: i + 1, end: to };
    let params = reader.read_list(0)?;
    Ok(Some(SfcRecord { id, name, version, params, span: (from, to) }))
}
