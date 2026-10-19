//! オブジェクトの表（23）・クラス（10）と、1 つのオブジェクトの枠（20.1「Common non-entity object format」、
//! 20.2「Common entity format」、20.4.1「Common Entity Data」、20.4.2「Common Entity Handle Data」）。
//! R2007 以降は、値・文字列・ハンドルを別々の並びから読む

use crate::bits::{Bits, Eof, HandleRef, Read, TextCodec};
use crate::file::Version;
use std::collections::HashMap;

/// 版ごとの読み方
#[derive(Debug, Clone, Copy)]
pub(crate) struct Format {
    pub version: Version,
    pub maintenance: u8,
    /// R2007 より前の文字列の文字コード
    pub codec: TextCodec,
}

impl Format {
    pub fn r2004(&self) -> bool {
        self.version >= Version::R2004
    }
    pub fn r2007(&self) -> bool {
        self.version >= Version::R2007
    }
    pub fn r2010(&self) -> bool {
        self.version >= Version::R2010
    }
    pub fn r2013(&self) -> bool {
        self.version >= Version::R2013
    }
    pub fn r2018(&self) -> bool {
        self.version >= Version::R2018
    }
}

/// 値・文字列・ハンドルの並び。R2007 より前は文字列も値の並びにある
#[derive(Debug, Clone)]
pub(crate) struct Streams<'a> {
    pub data: Bits<'a>,
    text: Text<'a>,
    /// ハンドルの並び。None なら値の並びの中（R2007 より前のヘッダ変数）
    handles: Option<Bits<'a>>,
    pub format: Format,
}

#[derive(Debug, Clone)]
pub(crate) enum Text<'a> {
    /// 値の並びの中（R2007 より前）
    Inline,
    /// 文字列の並び（R2007 以降）
    Stream(Bits<'a>),
    /// 文字列の並びがない（R2007 以降）。文字列は空
    Empty,
}

impl<'a> Streams<'a> {
    /// 文字列もハンドルも値の並びの中にある
    pub fn inline(data: Bits<'a>, format: Format) -> Self {
        Self { data, text: Text::Inline, handles: None, format }
    }

    pub fn with_text(data: Bits<'a>, text: Text<'a>, handles: Bits<'a>, format: Format) -> Self {
        Self { data, text, handles: Some(handles), format }
    }

    /// TV。DXF と同じく、制御文字はキャレット表記（^J など）に、^ は「^ 」にする
    pub fn tv(&mut self) -> Read<String> {
        let text = match &mut self.text {
            Text::Inline => self.data.tv(self.format.codec)?,
            Text::Stream(text) => text.tv(TextCodec::Utf16)?,
            Text::Empty => String::new(),
        };
        Ok(caret_notation(text))
    }

    /// H（ハンドルの並びから）
    pub fn handle(&mut self) -> Read<HandleRef> {
        match &mut self.handles {
            Some(handles) => handles.handle(),
            None => self.data.handle(),
        }
    }

    /// H を reference（そのオブジェクトのハンドル）を基準に参照先のハンドルにする
    pub fn handle_ref(&mut self, reference: u64) -> Read<u64> {
        Ok(self.handle()?.resolve(reference))
    }

    /// CMC（2.11）。R2004 以降は BS・BL（0xC0 BYLAYER、0xC1 BYBLOCK、0xC2 RGB、0xC3 ACI）・RC と色の名前
    pub fn cmc(&mut self) -> Read<Color> {
        let index = self.data.bs()?;
        if !self.format.r2004() {
            return Ok(Color::index(f64::from(index)));
        }
        let rgb = self.data.bl()? as u32;
        let flags = self.data.rc()?;
        if flags & 1 != 0 {
            self.tv()?;
        }
        if flags & 2 != 0 {
            self.tv()?;
        }
        Ok(Color::from_raw(rgb, index))
    }
}

/// DXF の文字列の書き方（DXF Reference「Control Character Handling」）。制御文字は ^ と 0x40 を足した文字、^ は「^ 」
pub(crate) fn caret_notation(text: String) -> String {
    if !text.chars().any(|c| c < ' ' || c == '^') {
        return text;
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

/// 色。ACI（0 BYBLOCK、256 BYLAYER）とトゥルーカラー（0xRRGGBB）
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Color {
    pub index: f64,
    pub rgb: Option<f64>,
}

impl Color {
    pub fn index(index: f64) -> Self {
        Self { index, rgb: None }
    }

    /// R2004 以降の CMC の BL（上位 1 バイトが種類）。fallback は BS の色番号
    pub fn from_raw(raw: u32, fallback: i16) -> Self {
        match raw >> 24 {
            0xc0 => Self::index(256.0),
            0xc1 => Self::index(0.0),
            0xc2 => Self { index: f64::from(fallback), rgb: Some(f64::from(raw & 0xff_ffff)) },
            0xc3 => Self::index(f64::from(raw & 0xff)),
            _ => Self::index(f64::from(fallback)),
        }
    }
}

/// クラス（10）
#[derive(Debug, Clone)]
pub(crate) struct Class {
    pub number: u16,
    pub dxf_name: String,
}

/// クラスの節を読む。読めなくなった所までのクラスを返す
pub(crate) fn read_classes(section: &[u8], format: Format) -> Vec<Class> {
    let mut out = Vec::new();
    let _ = classes(section, format, &mut out);
    out
}

fn classes(section: &[u8], format: Format, out: &mut Vec<Class>) -> Read<()> {
    let mut data = Bits::new(section);
    data.skip(16 * 8)?;
    let size = u64::from(data.rl()?);
    let mut end = data.pos() + size * 8;
    if (format.r2010() && format.maintenance > 3) || format.r2018() {
        data.rl()?;
    }
    let mut text = Text::Inline;
    if format.r2007() {
        // 文字列は節の終わりの並び（5.8）。RL のビット数で終わりの印の位置が決まる
        let flag = data.pos() + u64::from(data.rl()?) - 1;
        let (strings, start) = string_stream(section, flag);
        text = strings;
        end = start;
        data.bl()?;
        data.bit()?;
    } else if format.r2004() {
        // R2004: 最大のクラス番号（BS）・0（RC）・0（RC）・true（B）（10.2）
        data.bs()?;
        data.rc()?;
        data.rc()?;
        data.bit()?;
    }
    data.set_end(end);
    let mut streams = Streams { data, text, handles: None, format };
    while streams.data.pos() < end {
        let number = streams.data.bs()? as u16;
        let _proxy_flags = streams.data.bs()?;
        let _app = streams.tv()?;
        let _cpp = streams.tv()?;
        let dxf_name = streams.tv()?;
        let _zombie = streams.data.bit()?;
        let _item_class = streams.data.bs()?;
        if format.r2004() {
            for _ in 0..5 {
                streams.data.bl()?;
            }
        }
        out.push(Class { number, dxf_name });
    }
    Ok(())
}

/// R2007 以降の文字列の並び（20.1）。flag は値の並びの最後のビット。並びと、その始まりの位置を返す
pub(crate) fn string_stream(data: &[u8], flag: u64) -> (Text<'_>, u64) {
    let mut reader = Bits::new(data);
    reader.set_pos(flag);
    if !matches!(reader.bit(), Ok(true)) || flag < 16 {
        return (Text::Empty, flag);
    }
    let mut at = flag - 16;
    reader.set_pos(at);
    let Ok(mut size) = reader.rs().map(u64::from) else {
        return (Text::Empty, flag);
    };
    if size & 0x8000 != 0 {
        let Some(high_at) = at.checked_sub(16) else {
            return (Text::Empty, flag);
        };
        at = high_at;
        reader.set_pos(at);
        let Ok(high) = reader.rs() else {
            return (Text::Empty, flag);
        };
        size = (size & 0x7fff) | (u64::from(high) << 15);
    }
    let Some(start) = at.checked_sub(size) else {
        return (Text::Empty, flag);
    };
    let mut strings = Bits::new(data);
    strings.set_pos(start);
    strings.set_end(at);
    (Text::Stream(strings), start)
}

/// オブジェクトの表（23）。ハンドル → オブジェクトの節の中の位置
pub(crate) fn read_object_map(section: &[u8]) -> HashMap<u64, usize> {
    let mut map = HashMap::new();
    let mut at = 0usize;
    // 2 バイト（大きさと CRC だけ）の区切りで終わる
    while at + 2 <= section.len() {
        let size = usize::from(u16::from_be_bytes([section[at], section[at + 1]]));
        if size <= 2 {
            break;
        }
        let start = at + 2;
        let end = (start + (size - 2).min(2032)).min(section.len());
        let mut reader = Bits::at(&section[..end], start);
        let mut handle = 0u64;
        let mut location = 0i64;
        while reader.pos() < end as u64 * 8 {
            let (Ok(offset), Ok(delta)) = (reader.umc(), reader.mc()) else { break };
            handle = handle.wrapping_add(offset);
            location = location.wrapping_add(delta);
            if offset > 0
                && let Ok(location) = usize::try_from(location)
            {
                map.insert(handle, location);
            }
        }
        // CRC（上位のバイトが先）
        at = end + 2;
    }
    map
}

/// 拡張データの 1 つの値（28「Extended Entity Data」）
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EedValue {
    Text(String),
    Number(f64),
    Other,
}

/// 拡張データ。登録したアプリケーション（APPID）のハンドルと、グループコード（1000〜）と値の並び
#[derive(Debug, Clone, Default)]
pub(crate) struct Eed {
    pub apps: Vec<(u64, Vec<(i32, EedValue)>)>,
}

/// 1 つのオブジェクトを開いたところ。値の並びは、そのオブジェクトの種類ごとの値の前にある
pub(crate) struct Object<'a> {
    pub handle: u64,
    pub kind: u16,
    pub streams: Streams<'a>,
    pub eed: Eed,
}

/// オブジェクトを開き、型・ハンドル・拡張データまで読む（20.1、20.2）
pub(crate) fn open_object(objects: &[u8], offset: usize, format: Format) -> Read<Object<'_>> {
    let mut head = Bits::at(objects, offset);
    let size = head.ms()?;
    let handle_bits = if format.r2010() { head.umc()? } else { 0 };
    let start = head.pos();
    let end = start.checked_add(size.checked_mul(8).ok_or(Eof)?).ok_or(Eof)?;
    if end > objects.len() as u64 * 8 {
        return Err(Eof);
    }
    let mut data = Bits::new(objects);
    data.set_pos(start);
    data.set_end(end);
    let kind = data.ot(format.r2010())?;
    // ハンドルの並びの始まり。R2000〜R2007 は型の前からのビット数（RL）、R2010 以降は終わりからのビット数（MC）
    let handles_start =
        if format.r2010() { end.checked_sub(handle_bits).ok_or(Eof)? } else { start + u64::from(data.rl()?) };
    if handles_start > end || handles_start < data.pos() {
        return Err(Eof);
    }
    let text =
        if format.r2007() { string_stream(&objects[..(end / 8) as usize], handles_start - 1).0 } else { Text::Inline };
    let mut handles = Bits::new(objects);
    handles.set_pos(handles_start);
    handles.set_end(end);
    data.set_end(handles_start);
    let mut streams = Streams::with_text(data, text, handles, format);
    let handle = streams.data.handle()?.value;
    let eed = read_eed(&mut streams)?;
    Ok(Object { handle, kind, streams, eed })
}

/// 拡張データ（28）。BS の大きさ、APPID のハンドル、その大きさのバイトの値が、大きさ 0 まで続く
fn read_eed(streams: &mut Streams<'_>) -> Read<Eed> {
    let mut eed = Eed::default();
    loop {
        let size = streams.data.bs()?;
        if size <= 0 {
            return Ok(eed);
        }
        let app = streams.data.handle()?.value;
        let bytes = streams.data.bytes(size as usize)?;
        eed.apps.push((app, eed_values(&bytes, streams.format)));
    }
}

fn eed_values(bytes: &[u8], format: Format) -> Vec<(i32, EedValue)> {
    let mut reader = Bits::new(bytes);
    let mut out = Vec::new();
    while reader.remaining() >= 8 {
        let Ok(code) = reader.rc() else { break };
        let value = (|| -> Read<EedValue> {
            Ok(match code {
                // 文字列。R2007 より前は長さ（RC）・文字コード（RS）・バイト、R2007 以降は長さ（RS）と UTF-16
                0 => {
                    if format.r2007() {
                        let length = usize::from(reader.rs()?);
                        EedValue::Text(reader.text_body(length, TextCodec::Utf16)?)
                    } else {
                        let length = usize::from(reader.rc()?);
                        reader.rs()?;
                        EedValue::Text(reader.text_body(length, format.codec)?)
                    }
                }
                2 => {
                    reader.rc()?;
                    EedValue::Other
                }
                3 | 5 => {
                    reader.skip(64)?;
                    EedValue::Other
                }
                4 => {
                    let length = u64::from(reader.rc()?);
                    reader.skip(length * 8)?;
                    EedValue::Other
                }
                10..=13 => {
                    reader.rd3()?;
                    EedValue::Other
                }
                40..=42 => EedValue::Number(reader.rd()?),
                70 => EedValue::Number(f64::from(reader.rs()? as i16)),
                71 => EedValue::Number(f64::from(reader.rl()? as i32)),
                _ => return Err(Eof),
            })
        })();
        match value {
            Ok(value) => out.push((1000 + i32::from(code), value)),
            Err(_) => break,
        }
    }
    out
}

/// オブジェクト（図形でないもの）の共通の値（20.1）。持ち主とリアクタのハンドルは読み飛ばし、拡張辞書を返す
pub(crate) struct NonEntity {
    pub xdictionary: Option<u64>,
}

pub(crate) fn non_entity(object: &mut Object<'_>) -> Read<NonEntity> {
    let s = &mut object.streams;
    let reactors = s.data.bl()?;
    let xdic_missing = s.format.r2004() && s.data.bit()?;
    if s.format.r2013() {
        s.data.bit()?;
    }
    s.handle()?;
    for _ in 0..reactors.clamp(0, 1 << 16) {
        s.handle()?;
    }
    let xdictionary = if xdic_missing { None } else { Some(s.handle()?.value) };
    Ok(NonEntity { xdictionary: xdictionary.filter(|handle| *handle != 0) })
}

/// 図形の共通の値（20.4.1、20.4.2）
#[derive(Debug, Clone)]
pub(crate) struct EntityCommon {
    pub color: Color,
    /// DXF の 440 の値（0x020000TT、BYBLOCK は 0x01000000）
    pub transparency: Option<f64>,
    pub linetype_scale: f64,
    pub layer: u64,
    /// 線種のフラグ（00 BYLAYER、01 BYBLOCK、10 CONTINUOUS、11 ハンドル）と線種のハンドル
    pub ltype_flags: u8,
    pub ltype: Option<u64>,
    pub invisible: bool,
    /// DXF の 370 の値（1/100 mm、-1 BYLAYER、-2 BYBLOCK、-3 既定）
    pub lineweight: f64,
    /// R2000 の前後の図形（Nolinks が 0 のとき）
    pub links: Option<(u64, u64)>,
    /// 色見本の色（DBCOLOR）のハンドル。色は文書を組み立てるときに DBCOLOR から読む
    pub book_color: Option<u64>,
}

/// 線の太さの番号（R2000 以降の RC）から DXF の 370 の値。28・29 は BYLAYER、30 は BYBLOCK、31 は既定
/// （ACadSharp の CadUtils.ToValue と同じ）
pub(crate) fn lineweight(index: u8) -> f64 {
    const WEIGHTS: [f64; 24] = [
        0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        106.0, 120.0, 140.0, 158.0, 200.0, 211.0,
    ];
    match index {
        28 | 29 => -1.0,
        30 => -2.0,
        _ => WEIGHTS.get(usize::from(index)).copied().unwrap_or(-3.0),
    }
}

/// 図形の共通の値を読む。値の並びは図形の種類ごとの値の前、ハンドルの並びは種類ごとのハンドルの前に進む
pub(crate) fn entity_common(object: &mut Object<'_>) -> Read<EntityCommon> {
    let handle = object.handle;
    let s = &mut object.streams;
    // 画像（代わりの表示）の有無と大きさ。R2010 以降は BLL
    if s.data.bit()? {
        let size = if s.format.r2010() { s.data.bll()? } else { u64::from(s.data.rl()?) };
        s.data.skip(size.checked_mul(8).ok_or(Eof)?)?;
    }
    entity_mode_data(s, handle)
}

/// 埋め込みの図形（R2018 以降の複数行の属性の MTEXT）の共通の値。図形のモード（BB）から始まる（20.4.4）
pub(crate) fn embedded_entity_common(s: &mut Streams<'_>, handle: u64) -> Read<EntityCommon> {
    entity_mode_data(s, handle)
}

/// 図形のモード（BB）からの共通の値
fn entity_mode_data(s: &mut Streams<'_>, handle: u64) -> Read<EntityCommon> {
    let format = s.format;
    // 図形のモード（00 は持ち主のハンドルがある、01 紙空間、10 モデル空間）。持ち主はブロックから決める
    if s.data.bb()? == 0 {
        s.handle()?;
    }
    let reactors = s.data.bl()?;
    for _ in 0..reactors.clamp(0, 1 << 16) {
        s.handle()?;
    }
    let xdic_missing = format.r2004() && s.data.bit()?;
    if !xdic_missing {
        s.handle()?;
    }
    if format.r2013() {
        s.data.bit()?;
    }
    // Nolinks は R2000 まで（R2004 以降はない。ACadSharp・ezdwg と同じ）
    let mut links = None;
    if !format.r2004() && !s.data.bit()? {
        links = Some((s.handle_ref(handle)?, s.handle_ref(handle)?));
    }
    let (color, transparency, book) = entity_color(s)?;
    let book_color = if book { Some(s.handle()?.value) } else { None };
    let linetype_scale = s.data.bd()?;
    let layer = s.handle()?.value;
    let ltype_flags = s.data.bb()?;
    let ltype = if ltype_flags == 3 { Some(s.handle()?.value) } else { None };
    let plotstyle_flags = s.data.bb()?;
    if format.r2007() {
        if s.data.bb()? == 3 {
            s.handle()?;
        }
        s.data.rc()?;
    }
    if plotstyle_flags == 3 {
        s.handle()?;
    }
    if format.r2010() {
        for _ in 0..3 {
            if s.data.bit()? {
                s.handle()?;
            }
        }
    }
    let invisible = s.data.bs()? & 1 != 0;
    let lineweight = lineweight(s.data.rc()?);
    Ok(EntityCommon {
        color,
        transparency,
        linetype_scale,
        layer,
        ltype_flags,
        ltype,
        invisible,
        lineweight,
        links,
        book_color,
    })
}

/// 図形の色（2.11 の ENC）。R2000 は BS の色番号。R2004 以降は BS の上位のビットが印
/// （0x8000 RGB の BL が続く、0x4000 色見本の色のハンドルがある、0x2000 透過度の BL が続く）
fn entity_color(s: &mut Streams<'_>) -> Read<(Color, Option<f64>, bool)> {
    let value = s.data.bs()?;
    if !s.format.r2004() {
        return Ok((Color::index(f64::from(value)), None, false));
    }
    let flags = (value as u16) & 0xff00;
    let mut color = Color::index(f64::from((value as u16) & 0x0fff));
    let book = flags & 0x4000 != 0;
    if book {
        // 色見本の色は DBCOLOR のオブジェクトにあり、ここの下位のビットは近い ACI（DXF の 62 と同じ）
        color = Color::index(f64::from((value as u16) & 0xff));
    } else if flags & 0x8000 != 0 {
        let rgb = s.data.bl()? as u32;
        color = Color { index: f64::from((value as u16) & 0xff), rgb: Some(f64::from(rgb & 0xff_ffff)) };
    }
    let mut transparency = None;
    if flags & 0x2000 != 0 {
        // 上位 1 バイトが種類（0 BYLAYER、1 BYBLOCK、3 値）。DXF の 440 は値を 0x020000TT、BYBLOCK を 0x01000000 で書く
        let raw = s.data.bl()? as u32;
        transparency = match raw >> 24 {
            0 => None,
            1 => Some(f64::from(0x0100_0000)),
            _ => Some(f64::from(0x0200_0000 | (raw & 0xff))),
        };
    }
    Ok((color, transparency, book))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_map_accumulates_handles_and_locations_per_section() {
        // 1 つの区切り: 大きさ（値の 5 バイト + 2）、(ハンドル +1, 位置 +100)、(ハンドル +2, 位置 −36)、CRC。
        // 最後は大きさ 2 の区切り
        let section = [0x00, 0x07, 0x01, 0xe4, 0x00, 0x02, 0x64, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00];
        let map = read_object_map(&section);
        assert_eq!(map.get(&1), Some(&100));
        assert_eq!(map.get(&3), Some(&64));
    }
}
