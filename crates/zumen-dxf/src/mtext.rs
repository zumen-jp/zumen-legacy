//! TEXT の制御コード（%%c など）と MTEXT の書式コード（\P、\H など）を読み、同じ書式の文字の並び（run）にする
//! （`@zumenjs/dxf` の mtext.ts）。書式コードは AutoCAD の「Format Multiline Text in an Alternate Text Editor」の一覧と、
//! ezdxf（MIT）の tools/text.py の MTextParser の解釈に合わせた。JavaScript と同じく UTF-16 の単位で読む。

use crate::tags::decode_mif;
use zumen_core::js;

/// run の色。ACI（数値）か #rrggbb
#[derive(Debug, Clone, PartialEq)]
pub enum RunColor {
    Aci(f64),
    Rgb(String),
}

/// \F で変えたフォントのファイル（txt、romans.shx など）か、\f で変えた TrueType のフォント名
#[derive(Debug, Clone, PartialEq)]
pub struct RunFont {
    pub name: String,
    pub truetype: bool,
}

/// 積み重ね（\S）。kind は ^（公差）、/（横線の分数）、#（斜めの分数）
#[derive(Debug, Clone, PartialEq)]
pub struct RunStack {
    pub upper: String,
    pub lower: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    /// 文字の高さ（図面の単位）
    pub height: f64,
    pub width_factor: f64,
    /// 斜体の角度（度）
    pub oblique: f64,
    /// None は図形の色
    pub color: Option<RunColor>,
    pub font: Option<RunFont>,
    pub underline: bool,
    pub overline: bool,
    pub strike: bool,
    pub stack: Option<RunStack>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextParagraph {
    pub runs: Vec<TextRun>,
    /// \pxq の段落の揃え。None は配置点（71）に従う
    pub align: Option<ParagraphAlign>,
}

#[derive(Debug, Clone)]
struct Style {
    height: f64,
    width_factor: f64,
    oblique: f64,
    color: Option<RunColor>,
    font: Option<RunFont>,
    underline: bool,
    overline: bool,
    strike: bool,
}

impl Style {
    fn new(height: f64, width_factor: f64, oblique: f64) -> Self {
        Self {
            height,
            width_factor,
            oblique,
            color: None,
            font: None,
            underline: false,
            overline: false,
            strike: false,
        }
    }

    fn run(&self, text: String, stack: Option<RunStack>) -> TextRun {
        TextRun {
            text,
            height: self.height,
            width_factor: self.width_factor,
            oblique: self.oblique,
            color: self.color.clone(),
            font: self.font.clone(),
            underline: self.underline,
            overline: self.overline,
            strike: self.strike,
            stack,
        }
    }
}

const PERCENT: u16 = b'%' as u16;

/// 1 単位の文字を小文字にした文字（`code.toLowerCase()`）。1 文字にならなければ None
fn lower(unit: u16) -> Option<char> {
    let mut lowered = char::from_u32(u32::from(unit))?.to_lowercase();
    let first = lowered.next()?;
    lowered.next().is_none().then_some(first)
}

/// TEXT・MTEXT に共通の %% コード。%%c は直径、%%d は度、%%p はプラスマイナス
fn special_character(unit: u16) -> Option<&'static str> {
    match lower(unit)? {
        'c' => Some("Ø"),
        'd' => Some("°"),
        'p' => Some("±"),
        '%' => Some("%"),
        _ => None,
    }
}

/// `#rrggbb`（下位 24 ビット）
pub fn rgb_hex(rgb: i32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

fn text_of(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn is(unit: Option<&u16>, c: char) -> bool {
    unit.is_some_and(|unit| u32::from(*unit) == c as u32)
}

/// at から count 桁の 16 進の数字（[0-9A-Fa-f]{count}）
fn hex_at(units: &[u16], at: usize, count: usize) -> Option<u32> {
    let digits = units.get(at..at + count)?;
    let mut value = 0;
    for unit in digits {
        value = value * 16 + char::from_u32(u32::from(*unit))?.to_digit(16)?;
    }
    Some(value)
}

/// TEXT・ATTRIB の文字列を run にする。%%u・%%o・%%k は下線・上線・取り消し線の切り替え、
/// %%nnn は文字コード nnn の文字（DXF Reference「TEXT」、AutoCAD の「Control Codes and Special Characters」）
pub fn parse_text_codes(text: &str, height: f64, width_factor: f64, oblique: f64) -> Vec<TextRun> {
    let text: Vec<u16> = text.encode_utf16().collect();
    let mut runs = Vec::new();
    let mut style = Style::new(height, width_factor, oblique);
    let mut buffer: Vec<u16> = Vec::new();
    let flush = |runs: &mut Vec<TextRun>, buffer: &mut Vec<u16>, style: &Style| {
        if !buffer.is_empty() {
            runs.push(style.run(text_of(buffer), None));
        }
        buffer.clear();
    };
    let mut i = 0;
    while i < text.len() {
        let ch = text[i];
        if ch != PERCENT || text.get(i + 1) != Some(&PERCENT) || i + 2 >= text.len() {
            buffer.push(ch);
            i += 1;
            continue;
        }
        let code = text[i + 2];
        let lowered = lower(code);
        if matches!(lowered, Some('u' | 'o' | 'k')) {
            flush(&mut runs, &mut buffer, &style);
            match lowered {
                Some('u') => style.underline = !style.underline,
                Some('o') => style.overline = !style.overline,
                _ => style.strike = !style.strike,
            }
            i += 3;
            continue;
        }
        let digits = text.get(i + 2..i + 5).filter(|digits| digits.iter().all(|unit| (0x30..=0x39).contains(unit)));
        if let Some(digits) = digits {
            let value = digits.iter().fold(0u16, |value, unit| value * 10 + (unit - 0x30));
            buffer.push(value);
            i += 5;
            continue;
        }
        if let Some(special) = special_character(code) {
            buffer.extend(special.encode_utf16());
            i += 3;
            continue;
        }
        buffer.push(ch);
        i += 1;
    }
    flush(&mut runs, &mut buffer, &style);
    runs
}

/// \H・\W の値。末尾の x は今の値に対する倍率
fn relative(value: &[u16], current: f64) -> f64 {
    let value = text_of(value);
    let trimmed = js::trim(&value);
    let is_relative = trimmed.ends_with(['x', 'X']);
    let number = js::parse_float(if is_relative { &trimmed[..trimmed.len() - 1] } else { trimmed });
    if !number.is_finite() || number <= 0.0 {
        return current;
    }
    if is_relative { current * number } else { number }
}

struct MTextParser {
    paragraphs: Vec<TextParagraph>,
    style: Style,
    align: Option<ParagraphAlign>,
    buffer: Vec<u16>,
}

impl MTextParser {
    fn current(&mut self) -> &mut TextParagraph {
        self.paragraphs.last_mut().expect("段落は 1 つ以上ある")
    }

    fn flush(&mut self) {
        if !self.buffer.is_empty() {
            let run = self.style.run(text_of(&self.buffer), None);
            self.current().runs.push(run);
        }
        self.buffer.clear();
    }

    fn new_paragraph(&mut self) {
        self.flush();
        self.paragraphs.push(TextParagraph { runs: Vec::new(), align: self.align });
    }
}

/// i から ; までの値（; は含まない）と、; の次の位置
fn value_at(raw: &[u16], i: usize) -> (&[u16], usize) {
    let rest = raw.get(i..).unwrap_or(&[]);
    match rest.iter().position(|unit| *unit == u16::from(b';')) {
        Some(end) => (&rest[..end], i + end + 1),
        None => (rest, raw.len()),
    }
}

/// MTEXT の文字列を段落と run にする。height・width_factor・oblique は図形と文字スタイルの値。
/// 段組み・タブ位置・字間（\T）・行の中の縦の揃え（\A）は読み飛ばす
pub fn parse_mtext(raw: &str, height: f64, width_factor: f64, oblique: f64) -> Vec<TextParagraph> {
    let raw: Vec<u16> = raw.encode_utf16().collect();
    let mut parser = MTextParser {
        paragraphs: vec![TextParagraph { runs: Vec::new(), align: None }],
        style: Style::new(height, width_factor, oblique),
        align: None,
        buffer: Vec::new(),
    };
    let mut groups: Vec<Style> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let ch = raw[i];
        if ch == u16::from(b'{') {
            parser.flush();
            groups.push(parser.style.clone());
            i += 1;
            continue;
        }
        if ch == u16::from(b'}') {
            parser.flush();
            if let Some(style) = groups.pop() {
                parser.style = style;
            }
            i += 1;
            continue;
        }
        if ch == u16::from(b'^') && i + 1 < raw.len() {
            // キャレット表記（^I はタブ、^J は改行、^ と空白は ^ そのもの）
            let next = raw[i + 1];
            if next == u16::from(b' ') {
                parser.buffer.push(u16::from(b'^'));
            } else if next == u16::from(b'I') {
                parser.buffer.extend("    ".encode_utf16());
            } else if next == u16::from(b'J') || next == u16::from(b'M') {
                parser.new_paragraph();
            } else {
                parser.buffer.push(u16::from(b' '));
            }
            i += 2;
            continue;
        }
        if ch == PERCENT
            && raw.get(i + 1) == Some(&PERCENT)
            && i + 2 < raw.len()
            && let Some(special) = special_character(raw[i + 2])
        {
            parser.buffer.extend(special.encode_utf16());
            i += 3;
            continue;
        }
        if ch != u16::from(b'\\') || i + 1 >= raw.len() {
            parser.buffer.push(ch);
            i += 1;
            continue;
        }
        let unit = raw[i + 1];
        // サロゲートは書式コードではない（既定の扱い）
        let command = char::from_u32(u32::from(unit)).unwrap_or('\u{ffff}');
        match command {
            '\\' | '{' | '}' => {
                parser.buffer.push(unit);
                i += 2;
            }
            'P' | 'X' | 'N' => {
                parser.new_paragraph();
                i += 2;
            }
            '~' => {
                parser.buffer.push(0x00a0);
                i += 2;
            }
            'U' | 'u' => {
                let code = if is(raw.get(i + 2), '+') { hex_at(&raw, i + 3, 4) } else { None };
                if let Some(code) = code {
                    parser.buffer.push(code as u16);
                    i += 7;
                } else {
                    parser.buffer.push(unit);
                    i += 2;
                }
            }
            'M' | 'm' => {
                let page = raw
                    .get(i + 3)
                    .and_then(|unit| char::from_u32(u32::from(*unit)))
                    .filter(|c| ('1'..='5').contains(c));
                let decoded = match (is(raw.get(i + 2), '+'), page, hex_at(&raw, i + 4, 4)) {
                    (true, Some(page), Some(code)) => decode_mif(page, code),
                    _ => None,
                };
                if let Some(decoded) = decoded {
                    parser.buffer.extend(decoded.encode_utf16());
                    i += 8;
                } else {
                    parser.buffer.push(unit);
                    i += 2;
                }
            }
            'L' | 'l' | 'O' | 'o' | 'K' | 'k' => {
                parser.flush();
                let on = command.is_ascii_uppercase();
                match command.to_ascii_lowercase() {
                    'l' => parser.style.underline = on,
                    'o' => parser.style.overline = on,
                    _ => parser.style.strike = on,
                }
                i += 2;
            }
            'H' | 'W' | 'Q' | 'C' | 'c' | 'F' | 'f' => {
                parser.flush();
                let (value, next) = value_at(&raw, i + 2);
                let style = &mut parser.style;
                match command {
                    'H' => style.height = relative(value, style.height),
                    'W' => style.width_factor = relative(value, style.width_factor),
                    'Q' => {
                        let angle = js::parse_float(&text_of(value));
                        if angle.is_finite() {
                            style.oblique = angle;
                        }
                    }
                    'C' => {
                        let aci = js::parse_int(&text_of(value));
                        // 256 は BYLAYER（図形の色に戻す）
                        if aci.is_finite() {
                            style.color = if aci == 256.0 { None } else { Some(RunColor::Aci(aci)) };
                        }
                    }
                    'c' => {
                        // 値は 0xBBGGRR の並び（ezdxf の parse_rgb_color と同じ）
                        let bgr = js::parse_int(&text_of(value));
                        if bgr.is_finite() {
                            let bgr = js::to_int32(bgr);
                            let r = bgr & 0xff;
                            let g = (bgr >> 8) & 0xff;
                            let b = (bgr >> 16) & 0xff;
                            style.color = Some(RunColor::Rgb(rgb_hex((r << 16) | (g << 8) | b)));
                        }
                    }
                    _ => {
                        let value = text_of(value);
                        let name = js::trim(value.split('|').next().unwrap_or(""));
                        style.font =
                            (!name.is_empty()).then(|| RunFont { name: name.to_string(), truetype: command == 'f' });
                    }
                }
                i = next;
            }
            'T' | 'A' => {
                i = value_at(&raw, i + 2).1;
            }
            'p' => {
                let (value, next) = value_at(&raw, i + 2);
                let kind = value
                    .windows(2)
                    .find(|pair| pair[0] == u16::from(b'q') && b"lcrjd".iter().any(|c| u16::from(*c) == pair[1]))
                    .map(|pair| pair[1] as u8);
                if let Some(kind) = kind {
                    parser.align = match kind {
                        b'c' => Some(ParagraphAlign::Center),
                        b'r' => Some(ParagraphAlign::Right),
                        b'l' => Some(ParagraphAlign::Left),
                        _ => None,
                    };
                    // 段落の先頭の \p は、その段落の揃えにする
                    if parser.buffer.is_empty() && parser.current().runs.is_empty() {
                        let align = parser.align;
                        parser.current().align = align;
                    }
                }
                i = next;
            }
            'S' => {
                parser.flush();
                // \S上^下; の区切り（^ / #）。\ の後の文字は区切りとみなさない
                let mut j = i + 2;
                let mut upper: Vec<u16> = Vec::new();
                let mut lower: Vec<u16> = Vec::new();
                let mut kind: Option<u16> = None;
                let mut escaped = false;
                while j < raw.len() {
                    let c = raw[j];
                    if !escaped && c == u16::from(b';') {
                        break;
                    }
                    if !escaped && c == u16::from(b'\\') {
                        escaped = true;
                        j += 1;
                        continue;
                    }
                    if !escaped
                        && kind.is_none()
                        && (c == u16::from(b'^') || c == u16::from(b'/') || c == u16::from(b'#'))
                    {
                        kind = Some(c);
                        escaped = false;
                        j += 1;
                        continue;
                    }
                    if kind.is_some() {
                        lower.push(c);
                    } else {
                        upper.push(c);
                    }
                    escaped = false;
                    j += 1;
                }
                let stack = RunStack {
                    upper: js::trim(&text_of(&upper)).to_string(),
                    lower: js::trim(&text_of(&lower)).to_string(),
                    kind: kind.map(|kind| text_of(&[kind])).unwrap_or_default(),
                };
                let run = parser.style.run(String::new(), Some(stack));
                parser.current().runs.push(run);
                i = j + 1;
            }
            _ => {
                parser.buffer.push(unit);
                i += 2;
            }
        }
    }
    parser.flush();
    parser.paragraphs
}

/// MTEXT の書式コードを除いた文字列。段落は改行、積み重ねは「上/下」
pub fn mtext_plain_text(raw: &str) -> String {
    parse_mtext(raw, 1.0, 1.0, 0.0)
        .iter()
        .map(|paragraph| {
            paragraph
                .runs
                .iter()
                .map(|run| match &run.stack {
                    Some(stack) => format!("{}/{}", stack.upper, stack.lower),
                    None => run.text.clone(),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
