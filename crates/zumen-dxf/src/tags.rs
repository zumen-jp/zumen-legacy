//! DXF のグループコードと値の並び（タグ）。テキストとバイナリの DXF を同じ形にする（`@zumenjs/dxf` の tags.ts）。
//! 値の型はグループコードの範囲で決まる（DXF Reference「Group Code Value Types Reference」）。

use crate::model::EncodingSource;
use encoding_rs::Encoding;
use zumen_core::{NoticeLevel, NoticeList, js};

/// タグの値。文字列か数
#[derive(Debug, Clone, PartialEq)]
pub enum TagValue {
    Str(String),
    Num(f64),
}

pub struct DxfTags {
    pub codes: Vec<i32>,
    pub values: Vec<TagValue>,
}

pub struct TagReadResult {
    pub tags: DxfTags,
    /// $ACADVER。見つからなければ AC1009（R12）
    pub version: String,
    pub binary: bool,
    /// 文字列の復号に使った文字コード（TextDecoder の名前）
    pub encoding: &'static str,
    pub encoding_source: EncodingSource,
    pub codepage: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Float,
    Int16,
    Int32,
    Int64,
    Bool,
    Binary,
}

/// グループコードの値の型（DXF Reference「Group Code Value Types Reference」の表）
fn kind_of(code: f64) -> Kind {
    let ranges: [(f64, Kind); 32] = [
        (9.0, Kind::String),
        (59.0, Kind::Float),
        (79.0, Kind::Int16),
        (99.0, Kind::Int32),
        (109.0, Kind::String),
        (149.0, Kind::Float),
        (159.0, Kind::String),
        (169.0, Kind::Int64),
        (179.0, Kind::Int16),
        (209.0, Kind::String),
        (239.0, Kind::Float),
        (269.0, Kind::String),
        (289.0, Kind::Int16),
        (299.0, Kind::Bool),
        (309.0, Kind::String),
        (319.0, Kind::Binary),
        (369.0, Kind::String),
        (389.0, Kind::Int16),
        (399.0, Kind::String),
        (409.0, Kind::Int16),
        (419.0, Kind::String),
        (429.0, Kind::Int32),
        (439.0, Kind::String),
        (459.0, Kind::Int32),
        (469.0, Kind::Float),
        (1003.0, Kind::String),
        (1004.0, Kind::Binary),
        (1009.0, Kind::String),
        (1059.0, Kind::Float),
        (1070.0, Kind::Int16),
        (1071.0, Kind::Int32),
        (f64::INFINITY, Kind::String),
    ];
    ranges.iter().find(|(limit, _)| code <= *limit).map_or(Kind::String, |(_, kind)| *kind)
}

/// バイナリ DXF の先頭 22 バイト（DXF Reference「About Binary DXF Files」）
const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF\r\n\x1a\x00";

pub fn is_binary_dxf(bytes: &[u8]) -> bool {
    bytes.starts_with(BINARY_SENTINEL)
}

/// ISO 8859-1 として 1 バイトを 1 文字にする
pub(crate) fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// `/\r\n|\n|\r/` で分ける
fn split_lines(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                lines.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    lines.push(&text[start..]);
    lines
}

/// テキストの DXF か。先頭のコメント（999）を飛ばし、最初のタグが 0 / SECTION かを見る
pub fn is_ascii_dxf(bytes: &[u8]) -> bool {
    let head = latin1(&bytes[..bytes.len().min(4096)]);
    let head = head.strip_prefix("\u{ef}\u{bb}\u{bf}").unwrap_or(&head);
    let lines = split_lines(head);
    let mut i = 0;
    while i + 1 < lines.len() && js::trim(lines[i]) == "999" {
        i += 2;
    }
    lines.get(i).is_some_and(|line| js::trim(line) == "0")
        && lines.get(i + 1).is_some_and(|line| js::trim(line) == "SECTION")
}

/// $DWGCODEPAGE の値から TextDecoder の名前（ezdxf の tools/codepage.py と同じ対応）
fn codepage_label(codepage: &str) -> Option<&'static str> {
    Some(match codepage {
        "ANSI_874" => "windows-874",
        "ANSI_932" | "DOS932" => "shift_jis",
        "ANSI_936" | "GB2312" => "gbk",
        "ANSI_949" | "KSC5601" => "euc-kr",
        "ANSI_950" | "BIG5" => "big5",
        "ANSI_1250" => "windows-1250",
        "ANSI_1251" => "windows-1251",
        "ANSI_1252" | "ISO8859_1" => "windows-1252",
        "ANSI_1253" => "windows-1253",
        "ANSI_1254" | "ISO8859_9" => "windows-1254",
        "ANSI_1255" => "windows-1255",
        "ANSI_1256" => "windows-1256",
        "ANSI_1257" => "windows-1257",
        "ANSI_1258" => "windows-1258",
        "MAC_ROMAN" => "macintosh",
        "ISO8859_2" => "iso-8859-2",
        "ISO8859_3" => "iso-8859-3",
        "ISO8859_4" => "iso-8859-4",
        "ISO8859_5" => "iso-8859-5",
        "ISO8859_6" => "iso-8859-6",
        "ISO8859_7" => "iso-8859-7",
        "ISO8859_8" => "iso-8859-8",
        _ => return None,
    })
}

pub(crate) fn encoding(label: &str) -> &'static Encoding {
    Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252)
}

/// `new TextDecoder(label).decode(bytes)`。UTF-8 の BOM は除く
pub(crate) fn decode(bytes: &[u8], label: &str) -> String {
    encoding(label).decode_with_bom_removal(bytes).0.into_owned()
}

/// `new TextDecoder(label, { fatal: true }).decode(bytes)`。正しく読めなければ None
pub(crate) fn decodes(bytes: &[u8], label: &str) -> Option<String> {
    let encoding = encoding(label);
    let bytes =
        if encoding == encoding_rs::UTF_8 { bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes) } else { bytes };
    encoding.decode_without_bom_handling_and_without_replacement(bytes).map(|text| text.into_owned())
}

/// 日本語の文字列らしいか。全角のかなが 1 字か、漢字が 2 字以上あるとき（半角カナは根拠にしない）
fn looks_japanese(text: &str) -> bool {
    if text.chars().any(|c| ('\u{3041}'..='\u{30fa}').contains(&c)) {
        return true;
    }
    text.chars().filter(|c| ('\u{3400}'..='\u{4dbf}').contains(c) || ('\u{4e00}'..='\u{9fff}').contains(c)).count() >= 2
}

fn has_multibyte_utf8(bytes: &[u8]) -> bool {
    bytes.iter().any(|byte| *byte >= 0xc2)
}

/// 文字列の文字コード。AutoCAD 2007（AC1021）以降の DXF は UTF-8、それより前は $DWGCODEPAGE
/// （DXF Reference「Storage of String Values」）。指定がない、または西欧の指定で、中身が UTF-8 として正しく読めるなら
/// UTF-8 とする。指定がなく Shift_JIS として正しく読めて日本語を含むなら Shift_JIS とする
fn choose_encoding(
    bytes: Option<&[u8]>,
    version: &str,
    codepage: Option<&str>,
    notices: &mut NoticeList,
) -> (&'static str, EncodingSource) {
    if version >= "AC1021" {
        return ("utf-8", EncodingSource::Version);
    }
    let declared = codepage.and_then(|codepage| codepage_label(&js::trim(codepage).to_uppercase()));
    if let Some(codepage) = codepage
        && !codepage.is_empty()
        && declared.is_none()
    {
        notices.add(
            NoticeLevel::Warning,
            "unknown-codepage",
            format!("対応していない文字コード {codepage} の文字列を西欧の文字として読みました"),
        );
    }
    if let Some(declared) = declared
        && declared != "windows-1252"
    {
        return (declared, EncodingSource::Codepage);
    }
    let has_codepage = codepage.is_some_and(|codepage| !codepage.is_empty());
    // バイナリの DXF は数値のバイトを含むので、中身から文字コードを推測しない（bytes が None）
    let Some(bytes) = bytes else {
        return ("windows-1252", if has_codepage { EncodingSource::Codepage } else { EncodingSource::Default });
    };
    if has_multibyte_utf8(bytes) && decodes(bytes, "utf-8").is_some() {
        if declared.is_some() {
            notices.add(NoticeLevel::Info, "utf8-codepage", "西欧の文字コードの指定と違い、UTF-8 として読みました");
        }
        return ("utf-8", EncodingSource::Detected);
    }
    if !has_codepage {
        if decodes(bytes, "shift_jis").is_some_and(|text| looks_japanese(&text)) {
            notices.add(
                NoticeLevel::Info,
                "guessed-shift-jis",
                "文字コードの指定がないため、日本語（Shift_JIS）と判断して読みました",
            );
            return ("shift_jis", EncodingSource::Detected);
        }
        return ("windows-1252", EncodingSource::Default);
    }
    ("windows-1252", EncodingSource::Codepage)
}

/// `/\$NAME\s*(?:\r\n|\n|\r)\s*[0-9]+\s*(?:\r\n|\n|\r)([^\r\n]*)/` の値（$NAME の次のタグの値）を trim したもの。
/// 空白（JavaScript の \s）の並びに改行がなければ一致しない。値は空白の並びの最後の改行の後から行の終わりまで
fn header_string(head: &str, name: &str) -> Option<String> {
    let is_break = |c: char| c == '\r' || c == '\n';
    let mut from = 0;
    while let Some(offset) = head[from..].find(name) {
        let at = from + offset;
        from = at + 1;
        let rest = &head[at + name.len()..];
        let space = rest.find(|c: char| !js::is_whitespace(c)).unwrap_or(rest.len());
        if !rest[..space].contains(is_break) {
            continue;
        }
        let rest = &rest[space..];
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let rest = &rest[digits..];
        let space = rest.find(|c: char| !js::is_whitespace(c)).unwrap_or(rest.len());
        let Some(last_break) = rest[..space].rfind(is_break) else {
            continue;
        };
        let value = &rest[last_break + 1..];
        let end = value.find(is_break).unwrap_or(value.len());
        return Some(js::trim(&value[..end]).to_string());
    }
    None
}

/// `parseNumber`: Number(text) が有限ならその値、でなければ parseFloat(trim(text))、それも読めなければ 0
fn parse_number(text: &str) -> f64 {
    let value = js::string_to_number(text);
    if value.is_finite() {
        return value;
    }
    let fallback = js::parse_float(js::trim(text));
    if fallback.is_finite() { fallback } else { 0.0 }
}

fn ascii_tags(text: &str, notices: &mut NoticeList) -> DxfTags {
    let mut codes = Vec::new();
    let mut values = Vec::new();
    let bytes = text.as_bytes();
    let length = bytes.len();
    let mut pos = if text.starts_with('\u{feff}') { 3 } else { 0 };
    let separator = if text.contains('\n') { b'\n' } else { b'\r' };
    let next_line = |pos: &mut usize| -> Option<&str> {
        if *pos >= length {
            return None;
        }
        let end = bytes[*pos..].iter().position(|byte| *byte == separator).map_or(length, |offset| *pos + offset);
        let mut line = &text[*pos..end];
        *pos = end + 1;
        if separator == b'\n' {
            line = line.strip_suffix('\r').unwrap_or(line);
        }
        Some(line)
    };
    let mut bad = 0;
    while let Some(code_line) = next_line(&mut pos) {
        let value_start = pos;
        let Some(value_line) = next_line(&mut pos) else {
            if !js::trim(code_line).is_empty() {
                bad += 1;
            }
            break;
        };
        let code = js::parse_int(code_line);
        if !code.is_finite() {
            bad += 1;
            // 1 行ずれた図面は、次の行から読み直す（行末の \r の分も戻す）
            pos = value_start;
            continue;
        }
        if code == 999.0 {
            continue;
        }
        codes.push(js::to_int32(code));
        values.push(match kind_of(code) {
            Kind::Float => TagValue::Num(parse_number(value_line)),
            Kind::Int16 | Kind::Int32 | Kind::Int64 | Kind::Bool => TagValue::Num(parse_number(value_line).trunc()),
            _ => TagValue::Str(value_line.to_string()),
        });
    }
    if bad > 0 {
        notices.add_count(NoticeLevel::Warning, "bad-tag", "グループコードとして読めない行を飛ばしました", bad);
    }
    DxfTags { codes, values }
}

struct BinaryTags<'a> {
    bytes: &'a [u8],
    pos: usize,
    label: &'static str,
    r12: bool,
}

/// バイナリ DXF の読み終わり（途中で終わっている）
struct EndOfData;

impl BinaryTags<'_> {
    fn read(mut self, notices: &mut NoticeList) -> DxfTags {
        let mut codes = Vec::new();
        let mut values = Vec::new();
        while self.pos < self.bytes.len() {
            match self.tag() {
                Ok((code, value)) => {
                    codes.push(code);
                    values.push(value);
                }
                Err(EndOfData) => {
                    notices.add(
                        NoticeLevel::Warning,
                        "truncated",
                        "バイナリ DXF が途中で終わっているため、読めたところまでを使いました",
                    );
                    break;
                }
            }
        }
        DxfTags { codes, values }
    }

    fn tag(&mut self) -> Result<(i32, TagValue), EndOfData> {
        let code = if self.r12 {
            // R12 のバイナリ DXF は 1 バイトのグループコード。255 の後の 2 バイトが拡張データのコード
            let code = i32::from(self.u8()?);
            if code == 255 { i32::from(u16::from_le_bytes(self.take()?)) } else { code }
        } else {
            i32::from(u16::from_le_bytes(self.take()?))
        };
        let value = match kind_of(f64::from(code)) {
            Kind::Float => TagValue::Num(f64::from_le_bytes(self.take()?)),
            Kind::Int16 => TagValue::Num(f64::from(i16::from_le_bytes(self.take()?))),
            Kind::Int32 => TagValue::Num(f64::from(i32::from_le_bytes(self.take()?))),
            Kind::Int64 => TagValue::Num(i64::from_le_bytes(self.take()?) as f64),
            Kind::Bool => TagValue::Num(f64::from(self.u8()?)),
            Kind::Binary => {
                // テキストの DXF と同じく 16 進の文字列にする（サムネイル・代理図形などで、表示には使わない）
                let size = usize::from(self.u8()?);
                let start = self.advance(size)?;
                TagValue::Str(upper_hex(&self.bytes[start..start + size]))
            }
            Kind::String => TagValue::Str(self.string()?),
        };
        Ok((code, value))
    }

    fn advance(&mut self, size: usize) -> Result<usize, EndOfData> {
        let start = self.pos;
        if start + size > self.bytes.len() {
            return Err(EndOfData);
        }
        self.pos += size;
        Ok(start)
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], EndOfData> {
        let start = self.advance(N)?;
        let mut out = [0; N];
        out.copy_from_slice(&self.bytes[start..start + N]);
        Ok(out)
    }

    fn u8(&mut self) -> Result<u8, EndOfData> {
        Ok(self.bytes[self.advance(1)?])
    }

    fn string(&mut self) -> Result<String, EndOfData> {
        let start = self.pos;
        let end =
            self.bytes[start..].iter().position(|byte| *byte == 0).map(|offset| start + offset).ok_or(EndOfData)?;
        self.pos = end + 1;
        let slice = &self.bytes[start..end];
        let text = if slice.is_ascii() { latin1(slice) } else { decode(slice, self.label) };
        let caret = slice.iter().any(|byte| *byte < 0x20 || *byte == b'^');
        Ok(if caret { caret_notation(&text) } else { text })
    }
}

/// テキストの DXF の文字列の書き方（DXF Reference「About ASCII Control Characters in DXF Files」）。制御文字は ^ と
/// 0x40 を足した文字（改行は ^J）、^ は「^ 」と書く。バイナリの DXF は文字のまま書くので、テキストの DXF と同じ値にする
fn caret_notation(text: &str) -> String {
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

/// ezdxf の binary_tags_loader と同じく、先頭から変数の名前を探し、続く文字列を読む
fn binary_header_value(bytes: &[u8], name: &str) -> Option<String> {
    let head = &bytes[..bytes.len().min(2048)];
    let name = name.as_bytes();
    let at = head
        .get(BINARY_SENTINEL.len()..)?
        .windows(name.len())
        .position(|window| window == name)
        .map(|offset| BINARY_SENTINEL.len() + offset)?;
    // 名前の終わりの NUL の後に値のグループコード（R13 以降は 2 バイト、R12 は 1 バイト）がある
    let mut start = at + name.len() + 1;
    start += if head.get(start + 1) == Some(&0) { 2 } else { 1 };
    let end = head.get(start..)?.iter().position(|byte| *byte == 0).map(|offset| start + offset)?;
    (end > start).then(|| latin1(&head[start..end]))
}

/// テキスト・バイナリの DXF をタグにする。文字列は図面の文字コードで復号する
pub fn read_tags(bytes: &[u8], notices: &mut NoticeList) -> TagReadResult {
    if is_binary_dxf(bytes) {
        let version = binary_header_value(bytes, "$ACADVER").unwrap_or_else(|| "AC1009".into());
        let codepage = if version.as_str() < "AC1021" { binary_header_value(bytes, "$DWGCODEPAGE") } else { None };
        let (label, source) = choose_encoding(None, &version, codepage.as_deref(), notices);
        let tags =
            BinaryTags { bytes, pos: BINARY_SENTINEL.len(), label, r12: version.as_str() <= "AC1009" }.read(notices);
        return TagReadResult { tags, version, binary: true, encoding: label, encoding_source: source, codepage };
    }
    let head = latin1(&bytes[..bytes.len().min(256 * 1024)]);
    let version = header_string(&head, "$ACADVER").unwrap_or_else(|| "AC1009".into());
    let codepage = header_string(&head, "$DWGCODEPAGE");
    let (label, source) = if bytes.starts_with(b"\xef\xbb\xbf") {
        ("utf-8", EncodingSource::Detected)
    } else {
        choose_encoding(Some(bytes), &version, codepage.as_deref(), notices)
    };
    // CP932・UTF-8 などの 2 バイト目以降は改行（0x0A・0x0D）にならないので、復号してから行に分けてよい
    let text = decode(bytes, label);
    TagReadResult {
        tags: ascii_tags(&text, notices),
        version,
        binary: false,
        encoding: label,
        encoding_source: source,
        codepage,
    }
}

/// バイト列を大文字の 16 進の文字列にする（バイナリの値。TypeScript の HEX の表と同じ）
fn upper_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// `pattern` の後に続く 16 進の数字 count 桁（[0-9A-Fa-f]{count}）
fn hex_at(units: &[u16], at: usize, count: usize) -> Option<u32> {
    let digits = units.get(at..at + count)?;
    let mut value = 0;
    for unit in digits {
        value = value * 16 + char::from_u32(u32::from(*unit))?.to_digit(16)?;
    }
    Some(value)
}

fn starts_with_units(units: &[u16], at: usize, pattern: &str) -> bool {
    let pattern: Vec<u16> = pattern.encode_utf16().collect();
    units.get(at..at + pattern.len()).is_some_and(|slice| slice == pattern.as_slice())
}

/// 1 回の置き換え（正規表現の g フラグと同じく、左から重ならずに）
fn replace_pass(units: &[u16], prefix: &str, replace: impl Fn(&[u16], usize) -> Option<(Vec<u16>, usize)>) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if starts_with_units(units, i, prefix)
            && let Some((replacement, length)) = replace(units, i)
        {
            out.extend(replacement);
            i += length;
            continue;
        }
        out.push(units[i]);
        i += 1;
    }
    out
}

/// 文字列の \U+XXXX（CIF）と \M+nXXXX（MIF）を文字にする（DXF Reference「Storage of String Values」）。
/// TypeScript と同じく \U+、\u+、\M+ の順に 3 回置き換える（前の置き換えで生まれた並びも次で置き換わる）
pub fn decode_escapes(text: &str) -> String {
    if !text.contains("\\U+") && !text.contains("\\M+") && !text.contains("\\u+") && !text.contains("\\m+") {
        return text.to_string();
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let unicode = |units: &[u16], at: usize| hex_at(units, at + 3, 4).map(|code| (vec![code as u16], 7));
    let units = replace_pass(&units, "\\U+", unicode);
    let units = replace_pass(&units, "\\u+", unicode);
    let units = replace_pass(&units, "\\M+", |units, at| {
        let page = char::from_u32(u32::from(*units.get(at + 3)?)).filter(|c| ('1'..='5').contains(c))?;
        let code = hex_at(units, at + 4, 4)?;
        let decoded = decode_mif(page, code)?;
        Some((decoded.encode_utf16().collect(), 8))
    });
    String::from_utf16_lossy(&units)
}

/// MIF（\M+nXXXX）の n と文字コード（ezdxf の lldxf/encoding.py と同じ対応）
pub fn decode_mif(page: char, code: u32) -> Option<String> {
    let label = match page {
        '1' => "shift_jis",
        '2' => "big5",
        '3' => "euc-kr",
        '5' => "gbk",
        _ => return None,
    };
    let bytes: Vec<u8> = if code > 0xff { vec![(code >> 8) as u8, (code & 0xff) as u8] } else { vec![code as u8] };
    decodes(&bytes, label)
}
