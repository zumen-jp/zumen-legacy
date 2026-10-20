//! 実物の図面（ZUMEN_SAMPLES のフォルダの DWG。リポジトリには入れない）で、書き出しの検査の値と入れ物を確かめる。
//! フォルダがなければ何もしない。
//!
//! - CRC と検査の値: 図面に記録された値（ファイルの頭、ページ、ヘッダ変数・クラスの節、オブジェクト、
//!   オブジェクトの表、R2000 の 2 つ目のファイルの頭）を、書き出しと同じ関数で計算して同じになる
//! - 書き直し: rewrite_dwg した図面の節の中身が、元の図面と同じになる

use crate::crc::{CRC8_SEED, crc8, crc32, page_checksum};
use crate::file::{self, R2004Container, Version, magic_sequence};
use crate::objects::{object_length, read_object_map};
use crate::write::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL};
use std::path::PathBuf;
use zumen_core::NoticeList;

fn samples() -> Vec<(String, Vec<u8>)> {
    let Some(folder) = std::env::var_os("ZUMEN_SAMPLES") else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(PathBuf::from(folder)) else { return Vec::new() };
    let mut out: Vec<(String, Vec<u8>)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("dwg")))
        .filter_map(|path| Some((path.file_name()?.to_string_lossy().into_owned(), std::fs::read(&path).ok()?)))
        .collect();
    out.sort();
    out
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// オブジェクトの表の区切りの CRC（上位のバイトが先）と、表が指すオブジェクトの CRC を数える。(区切り, オブジェクト)
fn check_objects(name: &str, map_section: &[u8], objects: &[u8], r2010: bool) -> (usize, usize) {
    let mut at = 0;
    let mut chunks = 0;
    while at + 2 <= map_section.len() {
        let size = usize::from(u16::from_be_bytes([map_section[at], map_section[at + 1]]));
        let stored = u16::from_be_bytes([map_section[at + size], map_section[at + size + 1]]);
        assert_eq!(crc8(CRC8_SEED, &map_section[at..at + size]), stored, "{name}: object map chunk at {at}");
        chunks += 1;
        at += size + 2;
        if size <= 2 {
            break;
        }
    }
    let map = read_object_map(map_section);
    for (handle, &offset) in &map {
        let length = object_length(objects, offset, r2010).unwrap_or_else(|| panic!("{name}: object {handle:X}"));
        let bytes = &objects[offset..offset + length];
        let stored = u16::from_le_bytes([bytes[length - 2], bytes[length - 1]]);
        assert_eq!(crc8(CRC8_SEED, &bytes[..length - 2]), stored, "{name}: object {handle:X}");
    }
    (chunks, map.len())
}

/// 2 つのバイト列の先頭の bits ビットが同じなら None、違えば最初に違うビットの位置
fn same_bits(a: &[u8], b: &[u8], bits: u64) -> Option<u64> {
    (0..bits).find(|&at| {
        let bit = |bytes: &[u8]| bytes.get((at / 8) as usize).map(|byte| byte & (0x80 >> (at % 8)) != 0);
        bit(a) != bit(b)
    })
}

fn check_framed(name: &str, section: &[u8], start: &[u8; 16], end: &[u8; 16], extra: bool) {
    let framed = Framed::parse(section, start, end, extra).unwrap_or_else(|| panic!("{name}: framing"));
    assert!(framed.crc_ok, "{name}: CRC");
}

fn check_r2000(name: &str, bytes: &[u8]) {
    let records = file::r2000_records(bytes).unwrap();
    // ファイルの頭の CRC: 0 から始めて節の数ごとの値と排他的論理和をとる（3.2.6）のと、0xC0C1 から始めるのが同じ
    let crc_at = 0x19 + records.len() * 9;
    let stored = u16::from_le_bytes([bytes[crc_at], bytes[crc_at + 1]]);
    let xor = match records.len() {
        3 => 0xa598,
        4 => 0x8101,
        5 => 0x3cc4,
        _ => 0x8461,
    };
    assert_eq!(crc8(0, &bytes[..crc_at]) ^ xor, stored, "{name}: file header CRC (spec)");
    assert_eq!(crc8(CRC8_SEED, &bytes[..crc_at]), stored, "{name}: file header CRC");
    let slice = |number| file::r2000_slice(bytes, &records, number).unwrap();
    check_framed(name, slice(0), &HEADER_SENTINEL, &HEADER_END, false);
    check_framed(name, slice(1), &CLASSES_SENTINEL, &CLASSES_END, false);
    check_objects(name, slice(2), bytes, false);
    // 2 つ目のファイルの頭（26）: 大きさの欄からビットの値の終わりまでの CRC
    let sentinel = [0xd4, 0x7b, 0x21, 0xce, 0x28, 0x93, 0x9f, 0xbf, 0x53, 0x24, 0x40, 0x09, 0x12, 0x3c, 0xaa, 0x01];
    let at = bytes.windows(16).position(|window| window == sentinel).expect("second header");
    let size = u32_at(bytes, at + 16) as usize;
    // 大きさは大きさの欄から終わりの区切りの前まで。CRC の後に 8 バイトが続く
    let crc_at = at + 16 + size - 10;
    let stored = u16::from_le_bytes([bytes[crc_at], bytes[crc_at + 1]]);
    assert_eq!(crc8(CRC8_SEED, &bytes[at + 16..crc_at]), stored, "{name}: second header CRC");
}

fn check_r2004(name: &str, bytes: &[u8], version: Version) {
    let magic = magic_sequence(0x100);
    // ファイルの頭（4.1）: CRC32 は CRC の欄を 0 にした 0x6C バイトから、初めの値 0
    let mut header = file::r2004_file_header(bytes).unwrap();
    let stored = u32_at(&header, 0x68);
    header[0x68..0x6c].fill(0);
    assert_eq!(crc32(0, &header), stored, "{name}: file header CRC32");
    assert_eq!(bytes[0xec..0x100], magic[0xec..], "{name}: magic after the file header");
    let second = u32_at(&header, 0x34) as usize;
    assert_eq!(bytes[second..second + 0x6c], bytes[0x80..0xec], "{name}: copy of the file header");
    // ページ（4.3、4.6）
    let page_map_at = u32_at(&header, 0x54) as usize + 0x100;
    let system = |at: usize| {
        let head: Vec<u8> = bytes[at..at + 0x14].to_vec();
        let compressed = u32_at(&head, 8) as usize;
        let mut zeroed = head.clone();
        zeroed[0x10..0x14].fill(0);
        let expected = page_checksum(page_checksum(0, &zeroed), &bytes[at + 0x14..at + 0x14 + compressed]);
        assert_eq!(expected, u32_at(&head, 0x10), "{name}: system page at {at:#x}");
    };
    system(page_map_at);
    let container = R2004Container::open(bytes).unwrap();
    let mut pages = 0;
    for info in &container.sections {
        for &(number, ..) in &info.pages {
            let at = container.page_address(number).unwrap();
            let mask = 0x4164_536b_u32 ^ at as u32;
            let mut head: Vec<u8> =
                (0..8).flat_map(|index| (u32_at(bytes, at + index * 4) ^ mask).to_le_bytes()).collect();
            let size = u32_at(&head, 8) as usize;
            let data_checksum = page_checksum(0, &bytes[at + 0x20..at + 0x20 + size]);
            assert_eq!(data_checksum, u32_at(&head, 0x1c), "{name}: data checksum of page {number}");
            let stored = u32_at(&head, 0x18);
            head[0x18..0x1c].fill(0);
            assert_eq!(page_checksum(data_checksum, &head), stored, "{name}: header checksum of page {number}");
            pages += 1;
        }
    }
    system(container.page_address(u32_at(&header, 0x5c) as i32).unwrap());
    assert!(pages > 0, "{name}: pages");
    let mut notices = NoticeList::new();
    let extra = crate::write::database::has_extra(version, bytes[0x0b]);
    check_framed(name, &container.section("AcDb:Header", &mut notices).unwrap(), &HEADER_SENTINEL, &HEADER_END, extra);
    check_framed(
        name,
        &container.section("AcDb:Classes", &mut notices).unwrap(),
        &CLASSES_SENTINEL,
        &CLASSES_END,
        extra,
    );
    let handles = container.section("AcDb:Handles", &mut notices).unwrap();
    let objects = container.section("AcDb:AcDbObjects", &mut notices).unwrap();
    check_objects(name, &handles, &objects, version >= Version::R2010);
    assert!(notices.into_vec().is_empty(), "{name}: notices");
}

/// 図面に記録された CRC と検査の値を、書き出しと同じ関数で計算すると同じ値になる
#[test]
fn the_crcs_and_checksums_of_real_drawings_are_reproduced() {
    for (name, bytes) in samples() {
        match Version::from_magic(&bytes[..6]) {
            Some(Version::R2000) => check_r2000(&name, &bytes),
            Some(version @ (Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018)) => {
                check_r2004(&name, &bytes, version)
            }
            _ => {}
        }
    }
}

/// 書き直した図面の節は、元の図面の節と同じ中身になる（オブジェクトの表は同じ並べ方で同じバイト列になる）
#[test]
fn rewritten_drawings_keep_every_section() {
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        if !matches!(version, Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018) {
            continue;
        }
        let written = crate::rewrite_dwg(&bytes).unwrap();
        assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
        let before = R2004Container::open(&bytes).unwrap();
        let after = R2004Container::open(&written.bytes).unwrap();
        let names = |container: &R2004Container| -> Vec<String> {
            container.sections.iter().map(|section| section.name.clone()).collect()
        };
        assert_eq!(names(&before), names(&after), "{name}");
        let mut notices = NoticeList::new();
        for info in &before.sections {
            // 0 だけのページは書かれないので、節の大きさまで 0 で埋めて比べる
            let mut original = before.data(info, &mut notices);
            original.resize(info.size as usize, 0);
            let rewritten_info = after.info(&info.name).unwrap();
            let mut rewritten = after.data(rewritten_info, &mut notices);
            rewritten.resize(rewritten_info.size as usize, 0);
            assert_eq!(
                (rewritten_info.max_size, rewritten_info.compressed, rewritten_info.encrypted),
                (info.max_size, info.compressed, info.encrypted),
                "{name}: {}",
                info.name
            );
            if info.name == "AcDb:Header" {
                // 終わりの区切りの後の値（以前の保存の残り）は写さない
                assert_eq!(original[..rewritten.len()], rewritten, "{name}: {}", info.name);
            } else {
                assert_eq!(original, rewritten, "{name}: {}", info.name);
            }
        }
        assert!(notices.into_vec().is_empty(), "{name}");
    }
}

/// R2000 の書き直し: ヘッダ変数・クラス・AuxHeader・Template が同じで、どのハンドルも同じオブジェクトを指す
#[test]
fn rewritten_r2000_drawings_keep_every_object() {
    for (name, bytes) in samples() {
        if Version::from_magic(&bytes[..6]) != Some(Version::R2000) {
            continue;
        }
        let written = crate::rewrite_dwg(&bytes).unwrap();
        assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
        let after = &written.bytes;
        check_r2000(&name, after);
        let records = [file::r2000_records(&bytes).unwrap(), file::r2000_records(after).unwrap()];
        let section = |index: usize, number: u8| {
            file::r2000_slice(if index == 0 { &bytes } else { after }, &records[index], number).map(<[u8]>::to_vec)
        };
        for number in [0, 1, 4, 5] {
            assert_eq!(section(0, number), section(1, number), "{name}: record {number}");
        }
        let maps = [read_object_map(&section(0, 2).unwrap()), read_object_map(&section(1, 2).unwrap())];
        assert_eq!(maps[0].len(), maps[1].len(), "{name}");
        for (handle, &offset) in &maps[0] {
            let length = object_length(&bytes, offset, false).unwrap();
            let moved = maps[1][handle];
            assert_eq!(bytes[offset..offset + length], after[moved..moved + length], "{name}: object {handle:X}");
        }
    }
}

/// クラスの節の値を読んで書くと、同じバイト列になる
#[test]
fn classes_of_real_drawings_are_written_back_unchanged() {
    use crate::bits::TextCodec;
    use crate::write::classes::{classes_bits, read_class_defs};
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        let section = match version {
            Version::R2000 => {
                let records = file::r2000_records(&bytes).unwrap();
                file::r2000_slice(&bytes, &records, 1).unwrap().to_vec()
            }
            Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018 => {
                let mut notices = NoticeList::new();
                R2004Container::open(&bytes).unwrap().section("AcDb:Classes", &mut notices).unwrap()
            }
            _ => continue,
        };
        let extra = crate::write::database::has_extra(version, bytes[0x0b]);
        let framed = Framed::parse(&section, &CLASSES_SENTINEL, &CLASSES_END, extra).unwrap();
        let codec =
            if version >= Version::R2007 { TextCodec::Utf16 } else { TextCodec::Bytes(encoding_rs::WINDOWS_1252) };
        let classes = read_class_defs(&framed.data, version, codec);
        assert!(!classes.is_empty(), "{name}");
        // 終わりの埋めるビットは AutoCAD では不定なので、意味のあるビットだけを比べる
        let bits = classes_bits(version, encoding_rs::WINDOWS_1252, &classes);
        assert_eq!(same_bits(bits.as_bytes(), &framed.data, bits.bit_len()), None, "{name}: {} classes", classes.len());
        assert_eq!(bits.bit_len().div_ceil(8), framed.data.len() as u64, "{name}");
    }
}

/// 区切りで囲む節の値（R2004 以降は伸長したもの）。number は R2000 の節の位置の記録の番号
fn framed_section(bytes: &[u8], version: Version, number: u8, name: &str) -> Framed {
    let section = match version {
        Version::R2000 => {
            let records = file::r2000_records(bytes).unwrap();
            file::r2000_slice(bytes, &records, number).unwrap().to_vec()
        }
        _ => {
            let mut notices = NoticeList::new();
            R2004Container::open(bytes).unwrap().section(name, &mut notices).unwrap()
        }
    };
    let extra = crate::write::database::has_extra(version, bytes[0x0b]);
    let (start, end) = if number == 0 { (&HEADER_SENTINEL, &HEADER_END) } else { (&CLASSES_SENTINEL, &CLASSES_END) };
    Framed::parse(&section, start, end, extra).unwrap()
}

/// ヘッダ変数をすべて読んで書くと、同じビット列になる（読み残しもない）
#[test]
fn header_variables_of_real_drawings_are_written_back_unchanged() {
    use crate::codec::header::{LEFTOVER, header_bits, read_header_vars};
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        if !matches!(version, Version::R2000 | Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018) {
            continue;
        }
        let framed = framed_section(&bytes, version, 0, "AcDb:Header");
        let vars = read_header_vars(&framed.data, version).unwrap_or_else(|_| panic!("{name}: read"));
        // 残りは終わりを埋めるビットだけ（AutoCAD は 1 バイトを足して埋めることもある）
        let [data, strings, handles] = LEFTOVER.get();
        assert!(data < 16 && strings == 0 && handles < 16, "{name}: left {data} {strings} {handles} bits");
        let bits = header_bits(&vars);
        let differs = same_bits(bits.as_bytes(), &framed.data, bits.bit_len());
        assert_eq!(differs, None, "{name}: {} values", vars.values.len());
        let length = bits.bit_len().div_ceil(8);
        assert!((length..=length + 1).contains(&(framed.data.len() as u64)), "{name}");
    }
}

#[test]
#[ignore]
fn dump_object_types() {
    use crate::bits::TextCodec;
    use crate::objects::{Format, open_object};
    for (name, bytes) in samples() {
        if !name.starts_with("sample_AC1027") && !name.starts_with("BLOCKPOINT") {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let mut notices = NoticeList::new();
        let container = R2004Container::open(&bytes).unwrap();
        let objects = container.section("AcDb:AcDbObjects", &mut notices).unwrap();
        let handles = container.section("AcDb:Handles", &mut notices).unwrap();
        let classes = framed_section(&bytes, version, 1, "AcDb:Classes");
        let defs = crate::write::classes::read_class_defs(&classes.data, version, TextCodec::Utf16);
        let map = read_object_map(&handles);
        let format = Format { version, maintenance: bytes[0x0b], codec: TextCodec::Utf16 };
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for (handle, offset) in &map {
            let Ok(object) = open_object(&objects, *offset, format) else { continue };
            let kind = if object.kind >= 500 {
                defs.iter().find(|class| class.number == object.kind).map_or("?".into(), |class| class.dxf_name.clone())
            } else {
                format!("{}", object.kind)
            };
            *counts.entry(kind).or_default() += 1;
            let _ = handle;
        }
        println!("{name}: {counts:?}");
    }
}

/// 読み書きできるオブジェクトの種類と、読んで書く関数（型の番号、図形の種類の名前）
type Codec = fn(&mut crate::codec::In<'_>, u16, &str, &mut crate::write::object::ObjectWriter) -> crate::bits::Read<()>;

/// 型の番号とクラスの DXF の名前（500 から）から、読んで書く関数
fn codec(kind: u16, class: &str) -> Option<Codec> {
    use crate::codec::objects::*;
    use crate::codec::tables::*;
    macro_rules! with {
        ($read:expr) => {
            |s, _, _, o| {
                $read(s)?.write(o);
                Ok(())
            }
        };
    }
    Some(match (kind, class) {
        (48 | 50 | 52 | 56 | 60 | 62 | 64 | 66 | 68 | 70, _) => |s, kind, _, o| {
            Control::read(s, kind)?.write(o, kind);
            Ok(())
        },
        (49, _) => with!(BlockHeader::read),
        (51, _) => with!(Layer::read),
        (53, _) => with!(TextStyle::read),
        (57, _) => with!(Linetype::read),
        (65, _) => with!(Vport::read),
        (67, _) => with!(Appid::read),
        (69, _) => with!(DimStyle::read),
        (42, _) => |s, _, _, o| {
            Dictionary::read(s, false)?.write(o);
            Ok(())
        },
        (72, _) => with!(Group::read),
        (73, _) => with!(MLineStyle::read),
        (79, _) | (_, "XRECORD") => with!(XRecord::read),
        (80, _) | (_, "ACDBPLACEHOLDER") => with!(Placeholder::read),
        (82, _) | (_, "LAYOUT") => with!(Layout::read),
        (_, "ACDBDICTIONARYWDFLT") => |s, _, _, o| {
            Dictionary::read(s, true)?.write(o);
            Ok(())
        },
        (_, "DICTIONARYVAR") => with!(DictionaryVar::read),
        (_, "SCALE") => with!(Scale::read),
        (_, "SORTENTSTABLE") => with!(SortEntsTable::read),
        (_, "MLEADERSTYLE") => with!(MLeaderStyle::read),
        (_, "MATERIAL") => with!(Material::read),
        _ if crate::codec::entities::Entity::knows(entity_name(kind, class)) => |s, _, name, o| {
            crate::codec::entities::Entity::read(s, name)?.ok_or(crate::bits::Eof)?.write(o);
            Ok(())
        },
        _ => return None,
    })
}

/// 図形の種類の名前（固定の型の名前か、クラスの DXF の名前）
fn entity_name<'a>(kind: u16, class: &'a str) -> &'a str {
    crate::codec::entities::fixed_name(kind).unwrap_or(class)
}

/// 図面のオブジェクトの節とオブジェクトの表
fn objects_of(bytes: &[u8], version: Version) -> (Vec<u8>, std::collections::HashMap<u64, usize>) {
    match version {
        Version::R2000 => {
            let records = file::r2000_records(bytes).unwrap();
            (bytes.to_vec(), read_object_map(file::r2000_slice(bytes, &records, 2).unwrap()))
        }
        _ => {
            let mut notices = NoticeList::new();
            let container = R2004Container::open(bytes).unwrap();
            let handles = container.section("AcDb:Handles", &mut notices).unwrap();
            (container.section("AcDb:AcDbObjects", &mut notices).unwrap(), read_object_map(&handles))
        }
    }
}

/// 読み書きできる種類のオブジェクトをすべて読んで書くと、同じビット列になる（読み残しもない）
#[test]
fn objects_of_real_drawings_are_written_back_unchanged() {
    use crate::codec::object::{open_raw, writer};
    let mut checked: std::collections::BTreeMap<String, usize> = Default::default();
    let mut failures: Vec<String> = Vec::new();
    let mut unknown: std::collections::BTreeMap<String, usize> = Default::default();
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        if !matches!(version, Version::R2000 | Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018) {
            continue;
        }
        let (objects, map) = objects_of(&bytes, version);
        let codec_text = if version >= Version::R2007 {
            crate::bits::TextCodec::Utf16
        } else {
            crate::bits::TextCodec::Bytes(encoding_rs::WINDOWS_1252)
        };
        let classes = crate::write::classes::read_class_defs(
            &framed_section(&bytes, version, 1, "AcDb:Classes").data,
            version,
            codec_text,
        );
        let mut handles: Vec<(&u64, &usize)> = map.iter().collect();
        handles.sort();
        for (handle, &offset) in handles {
            let Ok(mut raw) = open_raw(&objects, offset, version) else { continue };
            let class =
                classes.iter().find(|class| class.number == raw.kind).map_or("", |class| class.dxf_name.as_str());
            let Some(codec) = codec(raw.kind, class) else {
                *unknown.entry(if class.is_empty() { raw.kind.to_string() } else { class.to_string() }).or_default() +=
                    1;
                continue;
            };
            let kind = raw.kind;
            let mut out = writer(version, kind, raw.handle, &raw.eed);
            if codec(&mut raw.input, kind, entity_name(kind, class), &mut out).is_err() {
                failures.push(format!("{name} {handle:X} type {kind} {class}: read"));
                continue;
            }
            let left = [
                raw.input.data.remaining(),
                raw.input.strings.as_ref().map_or(0, crate::bits::Bits::remaining),
                raw.input.handles.as_ref().map_or(0, crate::bits::Bits::remaining),
            ];
            let length = object_length(&objects, offset, version >= Version::R2010).unwrap();
            let original = &objects[offset..offset + length];
            let (written, padding) = out.frame();
            let meaningful = (written.len() as u64 - 2) * 8 - padding;
            let same = written.len() == original.len() && same_bits(&written, original, meaningful).is_none();
            if left[0] != 0 || left[1] != 0 || left[2] >= 8 || !same {
                if failures.iter().filter(|line| line.contains(&format!("type {kind} {class}:"))).count() < 3 {
                    failures.push(format!(
                        "{name} {handle:X} type {kind} {class}: left {left:?}, length {} vs {}, first different bit {:?}",
                        written.len(),
                        original.len(),
                        same_bits(&written, original, meaningful)
                    ));
                }
            } else {
                *checked.entry(if class.is_empty() { kind.to_string() } else { class.to_string() }).or_default() += 1;
            }
        }
    }
    println!("checked {checked:?}");
    println!("not checked {unknown:?}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore]
fn debug_one_object() {
    use crate::codec::In;
    use crate::codec::object::{EntityCommon, open_raw};
    let wanted = std::env::var("ZUMEN_DEBUG_OBJECT").unwrap_or_default();
    let (file, handle) = wanted.split_once(':').unwrap();
    let handle = u64::from_str_radix(handle, 16).unwrap();
    for (name, bytes) in samples() {
        if name != file {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let (objects, map) = objects_of(&bytes, version);
        let offset = map[&handle];
        let mut raw = open_raw(&objects, offset, version).unwrap();
        println!("kind {} eed {:?}", raw.kind, raw.eed);
        println!(
            "data {} strings {:?} handles {:?}",
            raw.input.data.remaining(),
            raw.input.strings.as_ref().map(crate::bits::Bits::remaining),
            raw.input.handles.as_ref().map(crate::bits::Bits::remaining)
        );
        let mut probe = In { data: raw.input.data.clone(), strings: None, handles: raw.input.handles.clone(), version };
        let s = &mut probe;
        let graphic = s.data.bit();
        let mode = s.data.bb();
        println!("graphic {graphic:?} mode {mode:?}");
        if mode == Ok(0) {
            println!("owner {:?}", s.handle());
        }
        let reactors = s.data.bl();
        println!("reactors {reactors:?}");
        for _ in 0..reactors.unwrap_or(0).clamp(0, 10) {
            println!("  reactor {:?}", s.handle());
        }
        let missing = s.data.bit();
        println!("xdict missing {missing:?}");
        if missing == Ok(false) {
            println!("xdict {:?}", s.handle());
        }
        let color = s.data.bs();
        println!("color {color:?} (as u16 {:x?})", color.map(|c| c as u16));
        let common = EntityCommon::read(&mut raw.input);
        println!("common {common:#?}");
        println!(
            "data {} handles {:?}",
            raw.input.data.remaining(),
            raw.input.handles.as_ref().map(crate::bits::Bits::remaining)
        );
        let mut values = Vec::new();
        while raw.input.data.remaining() >= 2 {
            match raw.input.data.bd() {
                Ok(value) => values.push(value),
                Err(_) => break,
            }
        }
        println!("BDs {values:?} left {}", raw.input.data.remaining());
    }
}

#[test]
#[ignore]
fn dump_splines() {
    use crate::codec::object::{EntityCommon, open_raw};
    let mut seen: std::collections::BTreeMap<(String, i32, i32, i32, bool), usize> = Default::default();
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        if version < Version::R2013 {
            continue;
        }
        let (objects, map) = objects_of(&bytes, version);
        for &offset in map.values() {
            let Ok(mut raw) = open_raw(&objects, offset, version) else { continue };
            if raw.kind != 36 {
                continue;
            }
            let s = &mut raw.input;
            if EntityCommon::read(s).is_err() {
                continue;
            }
            let scenario = s.data.bl().unwrap();
            let flags1 = s.data.bl().unwrap();
            let knot = s.data.bl().unwrap();
            let _degree = s.data.bl().unwrap();
            // 当てはめ点の読み方で読み切れるか
            let mut probe = s.data.clone();
            let fit = (|| -> crate::bits::Read<bool> {
                probe.bd()?;
                probe.bd3()?;
                probe.bd3()?;
                let count = probe.bl()?;
                for _ in 0..count.clamp(0, 100000) {
                    probe.bd3()?;
                }
                Ok(probe.remaining() == 0)
            })()
            .unwrap_or(false);
            *seen.entry((name.clone(), scenario, flags1, knot, fit)).or_default() += 1;
        }
    }
    for (key, count) in seen {
        println!("{key:?}: {count}");
    }
}

#[test]
#[ignore]
fn dump_sections() {
    let wanted = std::env::var("ZUMEN_DUMP").unwrap_or_default();
    for (name, bytes) in samples() {
        if !wanted.split(',').any(|w| w == name) {
            continue;
        }
        let container = R2004Container::open(&bytes).unwrap();
        println!("{name}");
        let mut notices = NoticeList::new();
        for info in &container.sections {
            let data = container.data(info, &mut notices);
            println!(
                "  {:24} size {:7} max {:#7x} compressed {} encrypted {} pages {:?} first {:?} head {:02x?}",
                info.name,
                info.size,
                info.max_size,
                info.compressed,
                info.encrypted,
                info.pages.iter().map(|page| page.0).collect::<Vec<_>>(),
                container.address(info),
                &data[..data.len().min(std::env::var("ZUMEN_BYTES").ok().and_then(|v| v.parse().ok()).unwrap_or(48))]
            );
        }
    }
}

#[test]
#[ignore]
fn dump_dictionaries() {
    use crate::codec::object::open_raw;
    use crate::codec::objects::Dictionary;
    let wanted = std::env::var("ZUMEN_DUMP").unwrap_or_default();
    for (name, bytes) in samples() {
        if !wanted.split(',').any(|w| w == name) {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let (objects, map) = objects_of(&bytes, version);
        let text = if version >= Version::R2007 {
            crate::bits::TextCodec::Utf16
        } else {
            crate::bits::TextCodec::Bytes(encoding_rs::WINDOWS_1252)
        };
        let classes = crate::write::classes::read_class_defs(
            &framed_section(&bytes, version, 1, "AcDb:Classes").data,
            version,
            text,
        );
        let header = framed_section(&bytes, version, 0, "AcDb:Header");
        let vars = crate::codec::header::read_header_vars(&header.data, version).unwrap();
        let kind_name = |kind: u16| -> String {
            crate::codec::entities::fixed_name(kind).map(str::to_string).unwrap_or_else(|| {
                classes
                    .iter()
                    .find(|class| class.number == kind)
                    .map_or(kind.to_string(), |class| class.dxf_name.clone())
            })
        };
        let show = |handle: u64, depth: usize, label: &str, recurse: &dyn Fn(u64, usize, &str)| {
            let Some(&offset) = map.get(&handle) else {
                println!("{:indent$}{label} {handle:X} missing", "", indent = depth * 2);
                return;
            };
            let Ok(mut raw) = open_raw(&objects, offset, version) else { return };
            let kind = raw.kind;
            let kname = kind_name(kind);
            println!("{:indent$}{label} {handle:X} {kname}", "", indent = depth * 2);
            if (kind == 42 || kname == "ACDBDICTIONARYWDFLT") && depth < 2 {
                if let Ok(dict) = Dictionary::read(&mut raw.input, kname == "ACDBDICTIONARYWDFLT") {
                    println!(
                        "{:indent$}  (cloning {} hard_owner {} owner {:X} reactors {:?} xdict {:?})",
                        "",
                        dict.cloning,
                        dict.hard_owner,
                        dict.common.owner.value,
                        dict.common.reactors.iter().map(|r| r.value).collect::<Vec<_>>(),
                        dict.common.xdictionary.map(|x| x.value),
                        indent = depth * 2
                    );
                    for (entry_name, entry) in &dict.entries {
                        let label = entry_name.text(encoding_rs::WINDOWS_1252);
                        recurse(entry.value, depth + 1, &label);
                    }
                }
            }
        };
        fn walk(f: &dyn Fn(u64, usize, &str, &dyn Fn(u64, usize, &str)), handle: u64, depth: usize, label: &str) {
            f(handle, depth, label, &|h, d, l| walk(f, h, d, l));
        }
        println!("{name} ({version:?})");
        for (key, value) in &vars.values {
            if let crate::codec::header::Value::Handle(handle) = value {
                println!(
                    "  header {key} = {:X} ({})",
                    handle.value,
                    map.get(&handle.value)
                        .and_then(|&o| open_raw(&objects, o, version).ok())
                        .map_or("-".into(), |raw| kind_name(raw.kind))
                );
            }
        }
        let root = vars.handle("DICTIONARY_NAMED_OBJECTS").unwrap();
        walk(&show, root, 0, "ROOT");
    }
}

/// 種類ごとに最初のいくつかのオブジェクトの値を表示する（新しい図面の既定の値を決めるため）
#[test]
#[ignore]
fn dump_objects() {
    use crate::codec::entities::{Entity, fixed_name};
    use crate::codec::object::open_raw;
    use crate::codec::objects::*;
    use crate::codec::tables::*;
    let wanted = std::env::var("ZUMEN_DUMP").unwrap_or_default();
    let kinds: Vec<String> = std::env::var("ZUMEN_KINDS").unwrap_or_default().split(',').map(str::to_string).collect();
    let limit: usize = std::env::var("ZUMEN_LIMIT").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
    for (name, bytes) in samples() {
        if !wanted.split(',').any(|w| w == name) {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let (objects, map) = objects_of(&bytes, version);
        let text = if version >= Version::R2007 {
            crate::bits::TextCodec::Utf16
        } else {
            crate::bits::TextCodec::Bytes(encoding_rs::WINDOWS_1252)
        };
        let classes = crate::write::classes::read_class_defs(
            &framed_section(&bytes, version, 1, "AcDb:Classes").data,
            version,
            text,
        );
        let mut handles: Vec<(&u64, &usize)> = map.iter().collect();
        handles.sort();
        let mut seen: std::collections::HashMap<String, usize> = Default::default();
        for (handle, &offset) in handles {
            let Ok(mut raw) = open_raw(&objects, offset, version) else { continue };
            let class =
                classes.iter().find(|class| class.number == raw.kind).map_or("", |class| class.dxf_name.as_str());
            let kname = fixed_name(raw.kind)
                .map(str::to_string)
                .unwrap_or_else(|| if class.is_empty() { raw.kind.to_string() } else { class.to_string() });
            if !kinds.iter().any(|k| *k == kname) {
                continue;
            }
            let count = seen.entry(kname.clone()).or_default();
            if *count >= limit {
                continue;
            }
            *count += 1;
            let s = &mut raw.input;
            let value: String = match (raw.kind, class) {
                (48 | 50 | 52 | 56 | 60 | 62 | 64 | 66 | 68 | 70, _) => format!("{:?}", Control::read(s, raw.kind)),
                (49, _) => format!("{:?}", BlockHeader::read(s)),
                (51, _) => format!("{:?}", Layer::read(s)),
                (53, _) => format!("{:?}", TextStyle::read(s)),
                (57, _) => format!("{:?}", Linetype::read(s)),
                (65, _) => format!("{:?}", Vport::read(s)),
                (67, _) => format!("{:?}", Appid::read(s)),
                (69, _) => format!("{:?}", DimStyle::read(s)),
                (42, _) => format!("{:?}", Dictionary::read(s, false)),
                (72, _) => format!("{:?}", Group::read(s)),
                (73, _) => format!("{:?}", MLineStyle::read(s)),
                (79, _) | (_, "XRECORD") => format!("{:?}", XRecord::read(s)),
                (80, _) | (_, "ACDBPLACEHOLDER") => format!("{:?}", Placeholder::read(s)),
                (82, _) | (_, "LAYOUT") => format!("{:?}", Layout::read(s)),
                (_, "ACDBDICTIONARYWDFLT") => format!("{:?}", Dictionary::read(s, true)),
                (_, "DICTIONARYVAR") => format!("{:?}", DictionaryVar::read(s)),
                (_, "SCALE") => format!("{:?}", Scale::read(s)),
                (_, "SORTENTSTABLE") => format!("{:?}", SortEntsTable::read(s)),
                (_, "MLEADERSTYLE") => format!("{:?}", MLeaderStyle::read(s)),
                (_, "MATERIAL") => format!("{:?}", Material::read(s)),
                _ => format!("{:?}", Entity::read(s, &kname)),
            };
            println!("{name} {handle:X} {kname} eed {:?}\n  {value}", raw.eed);
        }
    }
}

#[test]
#[ignore]
fn dump_header() {
    let wanted = std::env::var("ZUMEN_DUMP").unwrap_or_default();
    for (name, bytes) in samples() {
        if !wanted.split(',').any(|w| w == name) {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let header = framed_section(&bytes, version, 0, "AcDb:Header");
        let vars = crate::codec::header::read_header_vars(&header.data, version).unwrap();
        for (key, value) in &vars.values {
            println!("{key} = {value:?}");
        }
    }
}

#[test]
#[ignore]
fn dump_header_tails() {
    use crate::codec::header::{header_bits, read_header_vars};
    for (name, bytes) in samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        if !matches!(version, Version::R2000 | Version::R2004 | Version::R2010 | Version::R2013 | Version::R2018) {
            continue;
        }
        let framed = framed_section(&bytes, version, 0, "AcDb:Header");
        let vars = read_header_vars(&framed.data, version).unwrap();
        let written = header_bits(&vars).bit_len();
        let total = framed.data.len() as u64 * 8;
        let tail: String = (written..total)
            .map(|bit| if framed.data[(bit / 8) as usize] & (0x80 >> (bit % 8)) != 0 { '1' } else { '0' })
            .collect();
        println!("{name} {version:?} maintenance {} tail {tail}", bytes[0x0b]);
    }
}

#[test]
#[ignore]
fn dump_classes() {
    let wanted = std::env::var("ZUMEN_DUMP").unwrap_or_default();
    for (name, bytes) in samples() {
        if !wanted.split(',').any(|w| w == name) {
            continue;
        }
        let version = Version::from_magic(&bytes[..6]).unwrap();
        let text = if version >= Version::R2007 {
            crate::bits::TextCodec::Utf16
        } else {
            crate::bits::TextCodec::Bytes(encoding_rs::WINDOWS_1252)
        };
        let framed = framed_section(&bytes, version, 1, "AcDb:Classes");
        println!("{name} extra {:?}", framed.extra);
        for class in crate::write::classes::read_class_defs(&framed.data, version, text) {
            println!("  {class:?}");
        }
    }
}
