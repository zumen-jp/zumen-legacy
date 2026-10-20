//! ISO 10303-21（STEP の物理ファイル）の DATA 節を読む汎用の字句・構文解析（`@zumenjs/sxf` の p21-reader.ts）。
//! 複合インスタンス（#n = (A() B() …);）も型ごとの引数に分けて持つ。
//!
//! TypeScript はバイトを 1 文字ずつ U+0000〜U+00FF にした文字列（ISO 8859-1）を読む。ここではバイト列のまま読む。

use crate::header::{SxfHeader, read_header};
use crate::sfc_reader::{decode, is_lead_byte};
use std::collections::HashMap;
use zumen_core::ReadError;

#[derive(Debug, Clone, PartialEq)]
pub enum P21Value {
    /// `$` と `*`
    Null,
    Number(f64),
    Text(String),
    /// `#n`
    Ref(f64),
    /// `.NAME.`（大文字）
    Enum(String),
    /// `NAME(value)`。引数が 1 つでなければ集合
    Typed(String, Box<P21Value>),
    List(Vec<P21Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct P21Entity {
    pub id: f64,
    /// 型名（大文字）ごとの引数。単純インスタンスは 1 つ
    pub parts: Vec<(String, Vec<P21Value>)>,
}

impl P21Entity {
    pub fn args(&self, name: &str) -> Option<&[P21Value]> {
        self.parts.iter().find(|(part, _)| part == name).map(|(_, args)| args.as_slice())
    }

    pub fn has(&self, name: &str) -> bool {
        self.parts.iter().any(|(part, _)| part == name)
    }
}

/// インスタンス番号からインスタンスへ。JavaScript の Map と同じく、最初に現れた順に並び、
/// 同じ番号は値だけを置き換える
#[derive(Debug, Clone, Default)]
pub struct Entities {
    order: Vec<u64>,
    map: HashMap<u64, P21Entity>,
}

impl Entities {
    pub fn get(&self, id: f64) -> Option<&P21Entity> {
        self.map.get(&id.to_bits())
    }

    fn insert(&mut self, entity: P21Entity) {
        let key = entity.id.to_bits();
        if self.map.insert(key, entity).is_none() {
            self.order.push(key);
        }
    }

    /// 最初に現れた順
    pub fn values(&self) -> impl Iterator<Item = &P21Entity> {
        self.order.iter().map(|key| &self.map[key])
    }
}

pub struct P21File {
    pub entities: Entities,
    pub header: SxfHeader,
    /// 読めなかったインスタンスの数
    pub skipped: u64,
    /// DATA 節のすべてのインスタンス（読めなかったもの・番号が重なるものも）の、番号とファイルの中の位置。
    /// 書き出しが元の文字列を残すのに使う
    pub spans: Vec<InstanceSpan>,
    /// DATA 節の終わり（ENDSEC; の位置）
    pub data_end: usize,
}

/// インスタンスのファイルの中の位置（#n から ; まで）
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstanceSpan {
    /// インスタンス番号。読めなかったインスタンスは None
    pub id: Option<f64>,
    pub start: usize,
    pub end: usize,
}

/// ISO 8859-1 として 1 バイトを 1 文字にする
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn hex_run(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|byte| byte.is_ascii_hexdigit()).count()
}

fn hex_value(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0, |value, byte| value * 16 + char::from(*byte).to_digit(16).unwrap_or(0))
}

/// STEP の文字列のエスケープ（'' と \X2\…\X0\ など）を戻す。規格外の 8 ビットは Shift_JIS として読む。
/// \X4\ の値が Unicode の範囲外なら、JavaScript の String.fromCodePoint と同じく失敗する
pub(crate) fn decode_step_string(raw: &[u8]) -> Result<String, String> {
    if !raw.iter().any(|byte| *byte == b'\'' || *byte == b'\\' || *byte >= 0x80) {
        return Ok(latin1(raw));
    }
    let mut out: Vec<u16> = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let byte = raw[i];
        if byte == b'\'' && raw.get(i + 1) == Some(&b'\'') {
            out.push(u16::from(b'\''));
            i += 2;
            continue;
        }
        if byte == b'\\' {
            let rest = &raw[i..];
            if rest.starts_with(b"\\X2\\") {
                let run = hex_run(&rest[4..]);
                if run.is_multiple_of(4) && rest[4 + run..].starts_with(b"\\X0\\") {
                    for group in rest[4..4 + run].chunks(4) {
                        out.push(hex_value(group) as u16);
                    }
                    i += 8 + run;
                    continue;
                }
            }
            if rest.starts_with(b"\\X4\\") {
                let run = hex_run(&rest[4..]);
                if run.is_multiple_of(8) && rest[4 + run..].starts_with(b"\\X0\\") {
                    for group in rest[4..4 + run].chunks(8) {
                        let code = hex_value(group);
                        let Some(c) = char::from_u32(code) else {
                            // サロゲートの符号は JavaScript と同じく UTF-16 の 1 単位として入れる（前後と対になれば 1 文字）
                            if (0xd800..=0xdfff).contains(&code) {
                                out.push(code as u16);
                                continue;
                            }
                            return Err(format!("Invalid code point {code}"));
                        };
                        let mut units = [0; 2];
                        out.extend_from_slice(c.encode_utf16(&mut units));
                    }
                    i += 8 + run;
                    continue;
                }
            }
            if rest.starts_with(b"\\X\\") && rest.len() >= 5 && hex_run(&rest[3..5]) == 2 {
                out.push(hex_value(&rest[3..5]) as u16);
                i += 5;
                continue;
            }
            if rest.starts_with(b"\\S\\") && rest.len() >= 4 {
                out.push(u16::from(rest[3].wrapping_add(128)));
                i += 4;
                continue;
            }
            if rest.len() >= 4 && rest.starts_with(b"\\P") && (b'A'..=b'I').contains(&rest[2]) && rest[3] == b'\\' {
                i += 4;
                continue;
            }
            if raw.get(i + 1) == Some(&b'\\') {
                out.push(u16::from(b'\\'));
                i += 2;
                continue;
            }
        }
        // 規格では 8 ビットの文字は使えないが、Shift_JIS のまま書く出力ソフトもある。
        // 2 バイト文字の 2 バイト目は 0x40〜0x7E（\ を含む）のこともあるので、先頭バイトの次の 1 バイトも含めて復号する
        if byte >= 0x80 {
            let mut j = i;
            while j < raw.len() && raw[j] >= 0x80 {
                j += if is_lead_byte(raw[j]) && j + 1 < raw.len() { 2 } else { 1 };
            }
            out.extend(decode(&raw[i..j]).encode_utf16());
            i = j;
            continue;
        }
        out.push(u16::from(byte));
        i += 1;
    }
    Ok(String::from_utf16_lossy(&out))
}

enum StepError {
    /// インスタンスを読めない（次のインスタンスから読み直す）
    Syntax,
    /// 読み取り全体の失敗（TypeScript で StepSyntaxError 以外の例外になるもの）
    Fatal(String),
}

/// 引数の集合・型付きの値の入れ子の上限。深すぎるインスタンスは読めないものとして省く
const MAX_NESTING: usize = 64;

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'!'
}

fn is_number_char(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'E' | b'e')
}

fn find(text: &[u8], byte: u8, from: usize) -> Option<usize> {
    text.get(from..)?.iter().position(|candidate| *candidate == byte).map(|offset| from + offset)
}

fn find_bytes(text: &[u8], pattern: &[u8], from: usize) -> Option<usize> {
    text.get(from..)?.windows(pattern.len()).position(|window| window == pattern).map(|offset| from + offset)
}

/// `text.slice(start, end)`。end が見つからなかった（-1）ときは、JavaScript と同じく末尾の 1 文字の手前まで
fn slice(text: &[u8], start: usize, end: Option<usize>) -> &[u8] {
    let end = end.unwrap_or(text.len().saturating_sub(1));
    if start < end { &text[start.min(text.len())..end.min(text.len())] } else { &[] }
}

/// 1 文字ずつ読む字句解析
struct Lexer<'a> {
    text: &'a [u8],
    pos: usize,
    end: usize,
}

impl Lexer<'_> {
    fn at(&self, pos: usize) -> Option<u8> {
        self.text.get(pos).copied()
    }

    fn skip(&mut self) {
        loop {
            while self.pos < self.end && matches!(self.at(self.pos), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.pos += 1;
            }
            if self.at(self.pos) == Some(b'/') && self.at(self.pos + 1) == Some(b'*') {
                self.pos = find_bytes(self.text, b"*/", self.pos + 2).map_or(self.end, |close| close + 2);
                continue;
            }
            return;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip();
        if self.pos < self.end { self.at(self.pos) } else { None }
    }

    fn expect(&mut self, byte: u8) -> Result<(), StepError> {
        if self.peek() != Some(byte) {
            return Err(StepError::Syntax);
        }
        self.pos += 1;
        Ok(())
    }

    fn keyword(&mut self) -> Result<String, StepError> {
        self.skip();
        let start = self.pos;
        while self.pos < self.end && self.at(self.pos).is_some_and(is_word) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(StepError::Syntax);
        }
        Ok(String::from_utf8_lossy(&self.text[start..self.pos]).to_ascii_uppercase())
    }

    /// "(" の直後から ")" までの引数
    fn list(&mut self, depth: usize) -> Result<Vec<P21Value>, StepError> {
        if depth > MAX_NESTING {
            return Err(StepError::Syntax);
        }
        let mut values = Vec::new();
        if self.peek() == Some(b')') {
            self.pos += 1;
            return Ok(values);
        }
        loop {
            values.push(self.value(depth)?);
            let next = self.peek();
            self.pos += 1;
            match next {
                Some(b',') => {}
                Some(b')') => return Ok(values),
                _ => return Err(StepError::Syntax),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<P21Value, StepError> {
        let Some(byte) = self.peek() else {
            return Err(StepError::Syntax);
        };
        match byte {
            b'$' | b'*' => {
                self.pos += 1;
                return Ok(P21Value::Null);
            }
            b'\'' => return self.string().map(P21Value::Text),
            b'#' => {
                self.pos += 1;
                let mut id = 0.0;
                while self.pos < self.end
                    && let Some(digit) = self.at(self.pos).filter(u8::is_ascii_digit)
                {
                    id = id * 10.0 + f64::from(digit - b'0');
                    self.pos += 1;
                }
                return Ok(P21Value::Ref(id));
            }
            b'.' => {
                let close = find(self.text, b'.', self.pos + 1);
                let value = latin1(slice(self.text, self.pos + 1, close)).to_uppercase();
                self.pos = close.map_or(0, |close| close + 1);
                return Ok(P21Value::Enum(value));
            }
            b'(' => {
                self.pos += 1;
                return self.list(depth + 1).map(P21Value::List);
            }
            b'"' => {
                let close = find(self.text, b'"', self.pos + 1);
                let value = latin1(slice(self.text, self.pos + 1, close));
                self.pos = close.map_or(0, |close| close + 1);
                return Ok(P21Value::Text(value));
            }
            _ => {}
        }
        if byte.is_ascii_alphabetic() {
            let typed = self.keyword()?;
            self.expect(b'(')?;
            let mut inner = self.list(depth + 1)?;
            let value = if inner.len() == 1 { inner.pop().unwrap_or(P21Value::Null) } else { P21Value::List(inner) };
            return Ok(P21Value::Typed(typed, Box::new(value)));
        }
        let start = self.pos;
        while self.pos < self.end && self.at(self.pos).is_some_and(is_number_char) {
            self.pos += 1;
        }
        // Number(text) と同じ。使える文字（数字・符号・小数点・指数）の範囲では Rust の読み取りと同じ書き方を受け付ける
        let number = std::str::from_utf8(&self.text[start..self.pos])
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(f64::NAN);
        if self.pos == start || !number.is_finite() {
            return Err(StepError::Syntax);
        }
        Ok(P21Value::Number(number))
    }

    fn string(&mut self) -> Result<String, StepError> {
        let mut i = self.pos + 1;
        loop {
            let close = find(self.text, b'\'', i).filter(|close| *close < self.end).ok_or(StepError::Syntax)?;
            if self.at(close + 1) == Some(b'\'') {
                i = close + 2;
                continue;
            }
            let raw: Vec<u8> = self.text[self.pos + 1..close]
                .iter()
                .copied()
                .filter(|byte| *byte != b'\n' && *byte != b'\r')
                .collect();
            self.pos = close + 1;
            return decode_step_string(&raw).map_err(StepError::Fatal);
        }
    }
}

fn header_strings(text: &[u8]) -> Result<SxfHeader, ReadError> {
    let end = match find_bytes(text, b"DATA;", 0) {
        Some(end) if end > 0 => end,
        _ => 4096,
    };
    let head = latin1(&text[..end.min(text.len())]);
    // 文字は U+00FF までなので、1 文字を 1 バイトに戻して復号する
    read_header(&head, |value| decode_step_string(&value.chars().map(|c| c as u8).collect::<Vec<_>>()))
        .map_err(ReadError::new)
}

fn instance(lexer: &mut Lexer<'_>) -> Result<P21Entity, StepError> {
    lexer.pos += 1;
    let mut id = 0.0;
    while let Some(digit) = lexer.at(lexer.pos).filter(u8::is_ascii_digit) {
        id = id * 10.0 + f64::from(digit - b'0');
        lexer.pos += 1;
    }
    lexer.expect(b'=')?;
    let mut parts: Vec<(String, Vec<P21Value>)> = Vec::new();
    let mut set = |name: String, args: Vec<P21Value>| {
        if let Some(part) = parts.iter_mut().find(|(part, _)| *part == name) {
            part.1 = args;
        } else {
            parts.push((name, args));
        }
    };
    if lexer.peek() == Some(b'(') {
        lexer.pos += 1;
        while lexer.peek() != Some(b')') {
            let name = lexer.keyword()?;
            lexer.expect(b'(')?;
            set(name, lexer.list(0)?);
        }
        lexer.pos += 1;
    } else {
        let name = lexer.keyword()?;
        lexer.expect(b'(')?;
        set(name, lexer.list(0)?);
    }
    lexer.expect(b';')?;
    Ok(P21Entity { id, parts })
}

pub fn read_p21(bytes: &[u8]) -> Result<P21File, ReadError> {
    let Some(data_start) = find_bytes(bytes, b"DATA;", 0) else {
        return Err(ReadError::new("P21 の DATA 節がありません"));
    };
    let end = find_bytes(bytes, b"ENDSEC;", data_start).unwrap_or(bytes.len());
    let mut lexer = Lexer { text: bytes, pos: data_start + 5, end };
    let mut entities = Entities::default();
    let mut skipped = 0;
    let mut spans = Vec::new();
    while lexer.peek() == Some(b'#') {
        // 読み取りの上限（zumen_core::limits）
        if zumen_core::limits::tick() {
            break;
        }
        let start = lexer.pos;
        match instance(&mut lexer) {
            Ok(entity) => {
                spans.push(InstanceSpan { id: Some(entity.id), start, end: lexer.pos });
                entities.insert(entity);
            }
            Err(StepError::Syntax) => {
                skipped += 1;
                // 次のインスタンスまで読み飛ばす
                lexer.pos = find(bytes, b';', start).map_or(end, |next| next + 1);
                spans.push(InstanceSpan { id: None, start, end: lexer.pos.min(end) });
            }
            Err(StepError::Fatal(message)) => return Err(ReadError::new(message)),
        }
    }
    Ok(P21File { entities, header: header_strings(bytes)?, skipped, spans, data_end: end })
}
