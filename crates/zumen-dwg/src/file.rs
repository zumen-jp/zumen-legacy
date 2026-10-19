//! DWG の入れ物を開き、ヘッダ変数・クラス・オブジェクトの表・オブジェクトの節のバイト列を取り出す。
//! R13〜R2000 はファイルの中の位置（3「R13-R15 DWG FILE FORMAT ORGANIZATION」）、R2004・R2010〜R2018 は
//! 圧縮したページ（4「R2004 DWG FILE FORMAT ORGANIZATION」、6〜8）、R2007 は Reed-Solomon で符号化したページ
//! （5「R2007 DWG FILE FORMAT ORGANIZATION」）に置かれる

use crate::bits::{Bits, Read};
use crate::compress::{decompress_r18, decompress_r21, deinterleave};
use std::borrow::Cow;
use std::collections::HashMap;
use zumen_core::{NoticeLevel, NoticeList};

/// DWG の版（3.2.1）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Version {
    R13,
    R14,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl Version {
    pub fn from_magic(magic: &[u8]) -> Option<Self> {
        Some(match magic {
            b"AC1012" => Self::R13,
            b"AC1014" => Self::R14,
            b"AC1015" => Self::R2000,
            b"AC1018" => Self::R2004,
            b"AC1021" => Self::R2007,
            b"AC1024" => Self::R2010,
            b"AC1027" => Self::R2013,
            b"AC1032" => Self::R2018,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::R13 => "AC1012",
            Self::R14 => "AC1014",
            Self::R2000 => "AC1015",
            Self::R2004 => "AC1018",
            Self::R2007 => "AC1021",
            Self::R2010 => "AC1024",
            Self::R2013 => "AC1027",
            Self::R2018 => "AC1032",
        }
    }
}

/// 読み取りに使う節のバイト列。ヘッダ変数とクラスは始まりの区切り（SN）から
pub(crate) struct Sections<'a> {
    pub header: Cow<'a, [u8]>,
    pub classes: Cow<'a, [u8]>,
    /// オブジェクトの表（AcDb:Handles、23）
    pub handles: Cow<'a, [u8]>,
    /// オブジェクトの節。R2000 まではファイル全体（表の位置はファイルの中の位置）
    pub objects: Cow<'a, [u8]>,
    /// MEASUREMENT を持つ節（AcDb:Template、22）
    pub template: Option<Cow<'a, [u8]>>,
}

pub(crate) struct DwgFile<'a> {
    pub version: Version,
    /// 保存した AutoCAD の保守版（0x0B）
    pub maintenance: u8,
    /// 図面の文字コード（0x13、3.2.5）
    pub codepage: u16,
    pub sections: Sections<'a>,
}

/// 1 つの節を伸長した後の大きさの上限。壊れた図面や悪意のある図面がメモリを使い尽くさないようにする
pub(crate) const MAX_SECTION_BYTES: usize = 256 << 20;

/// ファイルの先頭の 6 バイトが DWG の版か
pub fn is_dwg(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && bytes.starts_with(b"AC10") && bytes[4..6].iter().all(u8::is_ascii_digit)
}

pub(crate) fn open<'a>(bytes: &'a [u8], notices: &mut NoticeList) -> Result<DwgFile<'a>, String> {
    if !is_dwg(bytes) {
        return Err("DWG の図面ではありません".into());
    }
    let magic = &bytes[0..6];
    let name = String::from_utf8_lossy(magic).into_owned();
    let version = Version::from_magic(magic).ok_or_else(|| format!("対応していない DWG の版です（{name}）"))?;
    if version < Version::R2000 {
        return Err(format!("対応していない DWG の版です（{name}、AutoCAD R13・R14）。R2000 以降の DWG を読めます"));
    }
    if bytes.len() < 0x100 {
        return Err("DWG のファイルの頭が途中で切れています".into());
    }
    let maintenance = bytes[0x0b];
    let codepage = u16::from_le_bytes([bytes[0x13], bytes[0x14]]);
    let sections = match version {
        Version::R13 | Version::R14 | Version::R2000 => r2000_sections(bytes),
        Version::R2007 => r2007_sections(bytes, notices),
        _ => r2004_sections(bytes, notices),
    }?;
    Ok(DwgFile { version, maintenance, codepage, sections })
}

// --- R13〜R2000 ---------------------------------------------------------------

/// 3.2.6 の節の位置（番号・位置・大きさ）。0 ヘッダ変数、1 クラス、2 オブジェクトの表、4 MEASUREMENT
fn r2000_sections(bytes: &[u8]) -> Result<Sections<'_>, String> {
    let mut reader = Bits::at(bytes, 0x15);
    let count = reader.rl().map_err(|_| "DWG の節の位置が読めません".to_string())?;
    let mut records: HashMap<u8, (usize, usize)> = HashMap::new();
    for _ in 0..count.min(16) {
        let Ok((number, seeker, size)) =
            (|| -> Read<(u8, u32, u32)> { Ok((reader.rc()?, reader.rl()?, reader.rl()?)) })()
        else {
            break;
        };
        records.entry(number).or_insert((seeker as usize, size as usize));
    }
    let slice = |number: u8| -> Option<Cow<'_, [u8]>> {
        let &(seeker, size) = records.get(&number)?;
        let end = seeker.checked_add(size)?.min(bytes.len());
        (seeker < end).then(|| Cow::Borrowed(&bytes[seeker..end]))
    };
    let missing = |name: &str| format!("DWG の節（{name}）が見つかりません");
    Ok(Sections {
        header: slice(0).ok_or_else(|| missing("ヘッダ変数"))?,
        classes: slice(1).ok_or_else(|| missing("クラス"))?,
        handles: slice(2).ok_or_else(|| missing("オブジェクトの表"))?,
        objects: Cow::Borrowed(bytes),
        template: slice(4),
    })
}

// --- R2004・R2010〜R2018 ------------------------------------------------------

/// 4.1 の 0x80 からの 0x6C バイトを解く数の並び（randseed = randseed * 0x343fd + 0x269ec3）
fn magic_sequence(length: usize) -> Vec<u8> {
    let mut seed: u32 = 1;
    (0..length)
        .map(|_| {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            (seed >> 16) as u8
        })
        .collect()
}

/// 4.5 のデータの節の説明
struct SectionInfo {
    size: u64,
    max_size: usize,
    compressed: bool,
    /// (ページの番号, データの大きさ, 伸長した節の中の位置)
    pages: Vec<(i32, usize, u64)>,
}

/// システムの節のページ（4.3）。0x14 バイトの頭と圧縮したデータ
fn r18_system_page(bytes: &[u8], address: usize) -> Option<Vec<u8>> {
    let mut reader = Bits::at(bytes, address);
    let _kind = reader.rl().ok()?;
    let decompressed = reader.rl().ok()? as usize;
    let compressed = reader.rl().ok()? as usize;
    let start = address.checked_add(0x14)?;
    let end = start.checked_add(compressed)?.min(bytes.len());
    let data = bytes.get(start..end)?;
    Some(decompress_r18(data, decompressed.min(MAX_SECTION_BYTES)))
}

fn r2004_sections<'a>(bytes: &'a [u8], notices: &mut NoticeList) -> Result<Sections<'a>, String> {
    let broken = || "DWG のページの表が読めません".to_string();
    let mut header: Vec<u8> = bytes[0x80..0x80 + 0x6c].to_vec();
    for (byte, mask) in header.iter_mut().zip(magic_sequence(0x6c)) {
        *byte ^= mask;
    }
    if !header.starts_with(b"AcFssFcAJMB") {
        return Err("DWG のファイルの頭（AcFssFcAJMB）が読めません".into());
    }
    let field = |at: usize| u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
    let field64 = |at: usize| u64::from(field(at)) | (u64::from(field(at + 4)) << 32);
    let page_map_address = usize::try_from(field64(0x54).saturating_add(0x100)).map_err(|_| broken())?;
    let section_map_id = field(0x5c) as i32;

    // ページの表（4.4）: ページの番号と大きさ。番号が負なら空き
    let page_map = r18_system_page(bytes, page_map_address).ok_or_else(broken)?;
    let mut pages: HashMap<i32, usize> = HashMap::new();
    let mut reader = Bits::new(&page_map);
    let mut address: usize = 0x100;
    while reader.remaining() >= 64 {
        let (Ok(number), Ok(size)) = (reader.rl(), reader.rl()) else { break };
        let number = number as i32;
        if number >= 0 {
            pages.entry(number).or_insert(address);
        } else if reader.skip(128).is_err() {
            break;
        }
        address = address.saturating_add(size as usize);
    }

    // データの節の表（4.5）
    let section_map_address = *pages.get(&section_map_id).ok_or_else(broken)?;
    let section_map = r18_system_page(bytes, section_map_address).ok_or_else(broken)?;
    let mut infos: HashMap<String, SectionInfo> = HashMap::new();
    let mut reader = Bits::new(&section_map);
    let descriptions = reader.rl().map_err(|_| broken())?;
    reader.skip(4 * 32).map_err(|_| broken())?;
    for _ in 0..descriptions.min(1024) {
        let Ok(info) = (|| -> Read<(String, SectionInfo)> {
            let size = reader.rll()?;
            let page_count = reader.rl()?;
            let max_size = reader.rl()? as usize;
            let _unknown = reader.rl()?;
            let compressed = reader.rl()? == 2;
            let _id = reader.rl()?;
            let _encrypted = reader.rl()?;
            let name_bytes = reader.bytes(64)?;
            let name: String = name_bytes.iter().take_while(|byte| **byte != 0).map(|byte| char::from(*byte)).collect();
            let mut pages = Vec::new();
            for _ in 0..page_count {
                pages.push((reader.rl()? as i32, reader.rl()? as usize, reader.rll()?));
            }
            Ok((name, SectionInfo { size, max_size, compressed, pages }))
        })() else {
            break;
        };
        infos.entry(info.0).or_insert(info.1);
    }

    let mut section = |name: &str| -> Option<Vec<u8>> {
        let info = infos.get(name)?;
        Some(r18_section(bytes, &pages, info, name, notices))
    };
    let missing = |name: &str| format!("DWG の節（{name}）が見つかりません");
    Ok(Sections {
        header: Cow::Owned(section("AcDb:Header").ok_or_else(|| missing("AcDb:Header"))?),
        classes: Cow::Owned(section("AcDb:Classes").ok_or_else(|| missing("AcDb:Classes"))?),
        handles: Cow::Owned(section("AcDb:Handles").ok_or_else(|| missing("AcDb:Handles"))?),
        objects: Cow::Owned(section("AcDb:AcDbObjects").ok_or_else(|| missing("AcDb:AcDbObjects"))?),
        template: section("AcDb:Template").map(Cow::Owned),
    })
}

/// データの節を、ページを順に伸長してつなげる（4.5、4.6）。書かれていないページ（0 だけのページ）は 0 で埋める
fn r18_section(
    bytes: &[u8],
    pages: &HashMap<i32, usize>,
    info: &SectionInfo,
    name: &str,
    notices: &mut NoticeList,
) -> Vec<u8> {
    let total = usize::try_from(info.size).unwrap_or(usize::MAX);
    let limit = total.min(MAX_SECTION_BYTES);
    if total > MAX_SECTION_BYTES {
        notices.add(
            NoticeLevel::Warning,
            "dwg-section-large",
            format!("DWG の節 {name} が大きすぎるため、{} MB までを読みました", MAX_SECTION_BYTES >> 20),
        );
    }
    let page_size = info.max_size.clamp(1, MAX_SECTION_BYTES);
    let mut out: Vec<u8> = Vec::new();
    let mut broken = 0u64;
    for &(number, data_size, start) in &info.pages {
        let start = usize::try_from(start).unwrap_or(usize::MAX);
        if start >= limit {
            break;
        }
        if start > out.len() {
            out.resize(start, 0);
        }
        out.truncate(start);
        let capacity = page_size.min(limit - start);
        let page =
            pages.get(&number).and_then(|&address| r18_page(bytes, address, data_size, info.compressed, capacity));
        match page {
            Some(page) => out.extend_from_slice(&page),
            None => broken += 1,
        }
        // 伸長した結果が短いページは、残りを 0 とする（次のページの位置で揃える）
        let expected = (start + capacity).min(limit);
        if out.len() < expected {
            out.resize(expected, 0);
        }
    }
    if out.len() < limit && !info.pages.is_empty() {
        out.resize(limit, 0);
    }
    if broken > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-broken-page",
            format!("DWG の節 {name} のページが読めず、0 で埋めました"),
            broken,
        );
    }
    out
}

/// データのページ（4.6）。32 バイトの頭は 0x4164536b ^ ページの位置 との排他的論理和で隠してある
fn r18_page(bytes: &[u8], address: usize, data_size: usize, compressed: bool, capacity: usize) -> Option<Vec<u8>> {
    let mask = 0x4164_536b_u32 ^ (address as u32);
    let mut reader = Bits::at(bytes, address);
    let kind = reader.rl().ok()? ^ mask;
    if kind != 0x4163_043b {
        return None;
    }
    let _section = reader.rl().ok()? ^ mask;
    let size = (reader.rl().ok()? ^ mask) as usize;
    let size = if size == 0 { data_size } else { size };
    let start = address.checked_add(0x20)?;
    let end = start.checked_add(size)?.min(bytes.len());
    let data = bytes.get(start..end)?;
    if compressed { Some(decompress_r18(data, capacity)) } else { Some(data[..data.len().min(capacity)].to_vec()) }
}

// --- R2007 ------------------------------------------------------------------

/// R2007 のシステムのページ（5.3）。0x480 からの位置にある Reed-Solomon（255, 239）の組を解き、伸長する
fn r21_system_page(bytes: &[u8], offset: u64, compressed: u64, uncompressed: u64, factor: u64) -> Option<Vec<u8>> {
    let aligned = compressed.checked_add(7)? & !7;
    let total = usize::try_from(aligned.checked_mul(factor)?).ok()?.min(MAX_SECTION_BYTES);
    let blocks = total.div_ceil(239);
    let start = usize::try_from(offset.checked_add(0x480)?).ok()?;
    let end = start.checked_add(blocks.checked_mul(255)?)?.min(bytes.len());
    let encoded = bytes.get(start..end)?;
    let data = deinterleave(encoded, blocks, 239, total);
    let uncompressed = usize::try_from(uncompressed).ok()?.min(MAX_SECTION_BYTES);
    let compressed = usize::try_from(compressed).ok()?;
    if compressed >= uncompressed {
        return Some(data[..data.len().min(uncompressed)].to_vec());
    }
    Some(decompress_r21(&data, 0, compressed, uncompressed))
}

struct Page21 {
    offset: u64,
    id: i64,
    uncompressed: u64,
    compressed: u64,
}

struct Section21 {
    size: u64,
    encoding: u64,
    pages: Vec<Page21>,
}

fn r2007_sections<'a>(bytes: &'a [u8], notices: &mut NoticeList) -> Result<Sections<'a>, String> {
    let broken = || "DWG（AutoCAD 2007）のページの表が読めません".to_string();
    // 0x80 からの 0x400 バイトのうち、0x3D8 バイトが Reed-Solomon（255, 239）の 3 つの組（5.2）
    let encoded = bytes.get(0x80..0x80 + 0x3d8).ok_or_else(broken)?;
    let decoded = deinterleave(encoded, 3, 239, 3 * 239);
    let mut reader = Bits::new(&decoded);
    reader.skip(24 * 8).map_err(|_| broken())?;
    let length = reader.rl().map_err(|_| broken())? as i32;
    let header = if length < 0 {
        decoded.get(32..32 + 0x110).ok_or_else(broken)?.to_vec()
    } else {
        decompress_r21(&decoded, 32, length as usize, 0x110)
    };
    let mut reader = Bits::new(&header);
    let mut values = [0u64; 34];
    for value in &mut values {
        *value = reader.rll().map_err(|_| broken())?;
    }
    let [
        _header_size,
        _file_size,
        _pages_crc,
        pages_factor,
        _pages_seed,
        _pages2_offset,
        _pages2_id,
        pages_offset,
        _pages_id,
        _header2_offset,
        pages_compressed,
        pages_uncompressed,
        _pages_amount,
        _pages_max_id,
        _x20,
        _x40,
        _pages_crc_uncompressed,
        _xf800,
        _x4,
        _x1,
        _sections_amount,
        _sections_crc_uncompressed,
        sections_compressed,
        _sections2_id,
        sections_id,
        sections_uncompressed,
        _sections_crc_compressed,
        sections_factor,
        ..,
    ] = values;

    // ページの表: 大きさと番号の組。位置は大きさを足していく
    let page_map =
        r21_system_page(bytes, pages_offset, pages_compressed, pages_uncompressed, pages_factor).ok_or_else(broken)?;
    let mut pages: HashMap<i64, (u64, u64)> = HashMap::new();
    let mut reader = Bits::new(&page_map);
    let mut offset = 0u64;
    while reader.remaining() >= 128 {
        let (Ok(size), Ok(id)) = (reader.rll(), reader.rll()) else { break };
        pages.entry((id as i64).wrapping_abs()).or_insert((offset, size));
        offset = offset.saturating_add(size);
    }

    // 節の表（5.2 の Section Map）
    let &(map_offset, _) = pages.get(&(sections_id as i64)).ok_or_else(broken)?;
    let section_map = r21_system_page(bytes, map_offset, sections_compressed, sections_uncompressed, sections_factor)
        .ok_or_else(broken)?;
    let mut sections: HashMap<String, Section21> = HashMap::new();
    let mut reader = Bits::new(&section_map);
    while reader.remaining() >= 64 * 8 {
        let Ok((name, section)) = (|| -> Read<(String, Section21)> {
            let size = reader.rll()?;
            let _max_size = reader.rll()?;
            let _encryption = reader.rll()?;
            let _hash = reader.rll()?;
            let name_length = reader.rll()?;
            let _unknown = reader.rll()?;
            let encoding = reader.rll()?;
            let page_count = reader.rll()?;
            // 名前の長さはバイトの数（終わりの 2 バイトの 0 を含む）。仕様は文字の数と書くが、ACadSharp と同じく
            // バイトの数として読むと、続くページの値が揃う
            let mut name = String::new();
            if name_length > 0 {
                let units: Vec<u16> = reader
                    .bytes(name_length.min(512) as usize)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .take_while(|unit| *unit != 0)
                    .collect();
                name = String::from_utf16_lossy(&units);
            }
            let mut pages = Vec::new();
            for _ in 0..page_count.min(1 << 20) {
                let offset = reader.rll()?;
                let _size = reader.rll()?;
                let id = reader.rll()? as i64;
                let uncompressed = reader.rll()?;
                let compressed = reader.rll()?;
                let _checksum = reader.rll()?;
                let _crc = reader.rll()?;
                pages.push(Page21 { offset, id, uncompressed, compressed });
            }
            Ok((name, Section21 { size, encoding, pages }))
        })() else {
            break;
        };
        if !name.is_empty() {
            sections.entry(name).or_insert(section);
        }
    }

    let mut section = |name: &str| -> Option<Vec<u8>> {
        let info = sections.get(name)?;
        Some(r21_section(bytes, &pages, info, name, notices))
    };
    let missing = |name: &str| format!("DWG の節（{name}）が見つかりません");
    Ok(Sections {
        header: Cow::Owned(section("AcDb:Header").ok_or_else(|| missing("AcDb:Header"))?),
        classes: Cow::Owned(section("AcDb:Classes").ok_or_else(|| missing("AcDb:Classes"))?),
        handles: Cow::Owned(section("AcDb:Handles").ok_or_else(|| missing("AcDb:Handles"))?),
        objects: Cow::Owned(section("AcDb:AcDbObjects").ok_or_else(|| missing("AcDb:AcDbObjects"))?),
        template: section("AcDb:Template").map(Cow::Owned),
    })
}

/// R2007 のデータの節（5.4）。符号化 4 のページは Reed-Solomon（255, 251）の組が交互に並ぶ
fn r21_section(
    bytes: &[u8],
    pages: &HashMap<i64, (u64, u64)>,
    info: &Section21,
    name: &str,
    notices: &mut NoticeList,
) -> Vec<u8> {
    let total = usize::try_from(info.size).unwrap_or(usize::MAX);
    let limit = total.min(MAX_SECTION_BYTES);
    if total > MAX_SECTION_BYTES {
        notices.add(
            NoticeLevel::Warning,
            "dwg-section-large",
            format!("DWG の節 {name} が大きすぎるため、{} MB までを読みました", MAX_SECTION_BYTES >> 20),
        );
    }
    let mut out: Vec<u8> = Vec::new();
    let mut broken = 0u64;
    for page in &info.pages {
        let start = usize::try_from(page.offset).unwrap_or(usize::MAX);
        if start >= limit {
            break;
        }
        if start > out.len() {
            out.resize(start, 0);
        }
        out.truncate(start);
        let capacity = usize::try_from(page.uncompressed).unwrap_or(usize::MAX).min(limit - start);
        match r21_page(bytes, pages, page, info.encoding, capacity) {
            Some(data) => out.extend_from_slice(&data),
            None => broken += 1,
        }
        if out.len() < start + capacity {
            out.resize(start + capacity, 0);
        }
    }
    if out.len() < limit && !info.pages.is_empty() {
        out.resize(limit, 0);
    }
    if broken > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-broken-page",
            format!("DWG の節 {name} のページが読めず、0 で埋めました"),
            broken,
        );
    }
    out
}

fn r21_page(
    bytes: &[u8],
    pages: &HashMap<i64, (u64, u64)>,
    page: &Page21,
    encoding: u64,
    capacity: usize,
) -> Option<Vec<u8>> {
    let &(offset, size) = pages.get(&page.id)?;
    let start = usize::try_from(offset.checked_add(0x480)?).ok()?;
    let end = start.checked_add(usize::try_from(size).ok()?)?.min(bytes.len());
    let raw = bytes.get(start..end)?;
    let compressed = usize::try_from(page.compressed).ok()?;
    let data: Cow<'_, [u8]> = if encoding == 4 {
        let aligned = compressed.checked_add(7)? & !7;
        let blocks = aligned.div_ceil(251);
        Cow::Owned(deinterleave(raw, blocks, 251, blocks * 251))
    } else {
        Cow::Borrowed(raw)
    };
    if page.compressed != page.uncompressed {
        Some(decompress_r21(&data, 0, compressed, capacity))
    } else {
        Some(data[..data.len().min(capacity)].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_magic_sequence_matches_the_specification() {
        // 4.1 の並びの始まり: 29 23 BE 84 E1 6C D6 AE
        assert_eq!(magic_sequence(8), [0x29, 0x23, 0xbe, 0x84, 0xe1, 0x6c, 0xd6, 0xae]);
    }
}
