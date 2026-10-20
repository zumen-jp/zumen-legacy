//! クラスの節（10「Data section AcDb:Classes」）の値。R2004 以降は最大のクラス番号と 0・0・true が先にあり、
//! クラスごとにオブジェクトの数・版・保守版と 2 つの不明の値（どれも BL）が続く。R2007 以降は文字列を値の後ろの文字列の並びに
//! 書き、値の始まりにビット数（RL）を置く

use super::bits::BitWriter;
use super::object::{append_strings, write_tu};
use crate::bits::{Bits, Read, TextCodec};
use crate::file::Version;
use crate::objects::{Streams, Text, string_stream};
use encoding_rs::Encoding;

/// 1 つのクラス（10.2）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
    /// クラスの番号（500 から。オブジェクトの型に使う）
    pub number: u16,
    /// R2004 以降は代理のフラグ、それより前は版
    pub proxy_flags: u16,
    pub app_name: String,
    pub cpp_name: String,
    pub dxf_name: String,
    /// R2007 より前の文字列が終わりの NUL を持つか（AutoCAD は持たせる。ほかの書き手は持たせないことがある）
    pub terminated: bool,
    pub zombie: bool,
    /// 0x1F2 は図形のクラス、0x1F3 はオブジェクトのクラス
    pub item_class: u16,
    /// 図面の中のこのクラスのオブジェクトの数（R2004 以降、DXF の 91）
    pub instances: i32,
    /// R2004 以降の版と保守版。仕様は BS と書くが、AutoCAD の図面では BL（保守版が 255 を超える図面で確かめた。
    /// 読み取りも BL で読む）
    pub dwg_version: i32,
    pub maintenance: i32,
    /// R2004 以降の 2 つの不明の値（ふつう 0）
    pub unknown: [i32; 2],
}

/// クラスの節の値（区切り・大きさ・CRC を除く）を書く。終わりはバイトの境目まで 0 で埋める（AutoCAD は不定の
/// ビットで埋める）
pub(crate) fn classes_data(version: Version, encoding: &'static Encoding, classes: &[ClassDef]) -> Vec<u8> {
    let mut bits = classes_bits(version, encoding, classes);
    bits.pad();
    bits.into_bytes()
}

/// クラスの節の値の、埋める前のビット
pub(crate) fn classes_bits(version: Version, encoding: &'static Encoding, classes: &[ClassDef]) -> BitWriter {
    let r2004 = version >= Version::R2004;
    let r2007 = version >= Version::R2007;
    let mut data = BitWriter::new();
    let mut strings = BitWriter::new();
    if r2004 {
        let max = classes.iter().map(|class| class.number).max().unwrap_or(499);
        data.bs(max as i16);
        data.rc(0);
        data.rc(0);
        data.bit(true);
    }
    let text = |data: &mut BitWriter, strings: &mut BitWriter, value: &str, terminated: bool| {
        if r2007 {
            write_tu(strings, value);
        } else {
            let (bytes, _, _) = encoding.encode(value);
            data.bs((bytes.len() + usize::from(terminated)) as i16);
            data.raw(&bytes);
            if terminated {
                data.rc(0);
            }
        }
    };
    for class in classes {
        data.bs(class.number as i16);
        data.bs(class.proxy_flags as i16);
        text(&mut data, &mut strings, &class.app_name, class.terminated);
        text(&mut data, &mut strings, &class.cpp_name, class.terminated);
        text(&mut data, &mut strings, &class.dxf_name, class.terminated);
        data.bit(class.zombie);
        data.bs(class.item_class as i16);
        if r2004 {
            data.bl(class.instances);
            data.bl(class.dwg_version);
            data.bl(class.maintenance);
            data.bl(class.unknown[0]);
            data.bl(class.unknown[1]);
        }
    }
    if !r2007 {
        return data;
    }
    // R2007 以降: ビット数（RL 自身から文字列の並びの印まで）、値、文字列の並び、その大きさ、印
    append_strings(&mut data, &strings);
    let mut out = BitWriter::new();
    out.rl((32 + data.bit_len()) as u32);
    out.append(&data);
    out
}

/// クラスの節の値（大きさの後から。R2010 以降の不明の RL の後）を読む。読めなくなった所までのクラスを返す
pub(crate) fn read_class_defs(data: &[u8], version: Version, codec: TextCodec) -> Vec<ClassDef> {
    let mut out = Vec::new();
    let _ = read(data, version, codec, &mut out);
    out
}

fn read(data: &[u8], version: Version, codec: TextCodec, out: &mut Vec<ClassDef>) -> Read<()> {
    let r2004 = version >= Version::R2004;
    let mut bits = Bits::new(data);
    let mut end = data.len() as u64 * 8;
    let mut text = Text::Inline;
    if version >= Version::R2007 {
        let flag = bits.pos() + u64::from(bits.rl()?) - 1;
        let (strings, start) = string_stream(data, flag);
        text = strings;
        end = start;
    }
    if r2004 {
        bits.bs()?;
        bits.rc()?;
        bits.rc()?;
        bits.bit()?;
    }
    bits.set_end(end);
    let format = crate::objects::Format { version, maintenance: 0, codec };
    let mut s = Streams::with_text_inline(bits, text, format);
    // 終わりの 0 のビット（バイトの境目まで）では止まる
    let r2007 = version >= Version::R2007;
    while s.data.remaining() >= 8 {
        let number = s.data.bs()? as u16;
        let proxy_flags = s.data.bs()? as u16;
        let mut terminated = false;
        let mut text = |s: &mut Streams<'_>| -> Read<String> {
            if r2007 {
                return s.tv_raw();
            }
            // 終わりの NUL を見るため、文字列はバイトのまま読む
            let length = s.data.bs()?.max(0) as usize;
            let mut bytes = s.data.bytes(length)?;
            if bytes.last() == Some(&0) {
                bytes.pop();
                terminated = true;
            }
            let TextCodec::Bytes(encoding) = codec else { return Ok(String::new()) };
            Ok(encoding.decode_without_bom_handling(&bytes).0.into_owned())
        };
        let app_name = text(&mut s)?;
        let cpp_name = text(&mut s)?;
        let dxf_name = text(&mut s)?;
        let zombie = s.data.bit()?;
        let item_class = s.data.bs()? as u16;
        let mut class = ClassDef {
            number,
            proxy_flags,
            app_name,
            cpp_name,
            dxf_name,
            terminated,
            zombie,
            item_class,
            instances: 0,
            dwg_version: 0,
            maintenance: 0,
            unknown: [0, 0],
        };
        if r2004 {
            class.instances = s.data.bl()?;
            class.dwg_version = s.data.bl()?;
            class.maintenance = s.data.bl()?;
            class.unknown = [s.data.bl()?, s.data.bl()?];
        }
        out.push(class);
    }
    Ok(())
}
