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

/// R2007 の図面の検査の値（5.2〜5.4、5.12、5.13）と、ヘッダ変数・クラスの節とオブジェクトの CRC。確かめたデータの
/// ページの数を返す
fn check_r2007(name: &str, bytes: &[u8]) -> usize {
    let pages = check_r2007_container(name, bytes);
    let container = crate::file::R2007Container::open(bytes).unwrap();
    let mut notices = NoticeList::new();
    check_framed(name, &container.section("AcDb:Header", &mut notices).unwrap(), &HEADER_SENTINEL, &HEADER_END, false);
    check_framed(
        name,
        &container.section("AcDb:Classes", &mut notices).unwrap(),
        &CLASSES_SENTINEL,
        &CLASSES_END,
        false,
    );
    let handles = container.section("AcDb:Handles", &mut notices).unwrap();
    let objects = container.section("AcDb:AcDbObjects", &mut notices).unwrap();
    check_objects(name, &handles, &objects, false);
    assert!(notices.into_vec().is_empty(), "{name}: notices");
    pages
}

/// R2007 の入れ物の検査の値（5.2〜5.4、5.12、5.13）: ファイルの頭のページ、ページの表と節の表、データのページの
/// Reed-Solomon の誤り訂正と組を埋める詰め物、64 ビットの CRC、ページの検査の値。確かめたデータのページの数を返す
pub(crate) fn check_r2007_container(name: &str, bytes: &[u8]) -> usize {
    use crate::compress::{decompress_r21, deinterleave};
    use crate::crc::{crc64_mirrored, crc64_normal, page_checksum_r21, update_seed1, update_seed2};
    use crate::file::{R2007Container, r21, r2007_file_header};
    use crate::write::container21::{Random21, ReedSolomon, check_crcs};
    let u64_at = |data: &[u8], at: usize| u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
    let system = ReedSolomon::system();
    let data_code = ReedSolomon::data();
    // 1 バイトずつ交互に並ぶ組の誤り訂正
    let interleaved = |code: &ReedSolomon, k: usize, encoded: &[u8], blocks: usize, what: &str| {
        for number in 0..blocks {
            let word: Vec<u8> = (0..255).map(|index| encoded[number + index * blocks]).collect();
            assert_eq!(code.parity(&word[..k]), word[k..], "{name}: Reed-Solomon of {what}, block {number}");
        }
    };

    // ファイルの頭（5.2）: 3 つの組、CRC、終わりの確かめのデータ、ファイルの終わりの写し
    let page = &bytes[0x80..0x480];
    interleaved(&system, 239, page, 3, "the file header");
    let (decoded, data) = r2007_file_header(bytes).unwrap();
    let mut zeroed = data.clone();
    zeroed[0x108..0x110].fill(0);
    assert_eq!(crc64_normal(update_seed2(0, 0x110), &zeroed), u64_at(&data, 0x108), "{name}: file header CRC");
    let length = i32::from_le_bytes(decoded[24..28].try_into().unwrap()).unsigned_abs() as usize;
    let packed = &decoded[32..32 + length];
    assert_eq!(
        crc64_normal(update_seed2(0, length as u64), packed),
        u64_at(&decoded, 16),
        "{name}: CRC of the compressed file header"
    );
    let first = u64_at(&decoded, 8);
    let sequence: Vec<u8> =
        [first, first.rotate_left((first & 0x1f) as u32)].iter().flat_map(|value| value.to_le_bytes()).collect();
    assert_eq!(crc64_normal(update_seed1(0, 16), &sequence), u64_at(&decoded, 0), "{name}: check sequence CRC");
    let check = &page[0x3d8..];
    assert_eq!(
        check_crcs(u64_at(check, 16), u64_at(check, 24)),
        (u64_at(check, 0), u64_at(check, 8)),
        "{name}: check data"
    );
    let container = R2007Container::open(bytes).unwrap();
    let header = container.header;
    let copy = (0x480 + header[r21::HEADER2_OFFSET]) as usize;
    assert_eq!(bytes[copy..copy + 0x400], *page, "{name}: copy of the file header");
    let padding = Random21::new(header[r21::RANDOM_SEED]).padding().to_vec();

    // システムのページ（5.3）: 繰り返したデータ、組を埋める詰め物、圧縮した・伸長したデータの CRC
    let system_page = |at: usize, values: [u64; 5], what: &str| {
        let [compressed, uncompressed, factor, crc_compressed, crc_uncompressed] = values;
        let aligned = (compressed as usize).div_ceil(8) * 8;
        let total = aligned * factor as usize;
        let blocks = total.div_ceil(239);
        let encoded = &bytes[at..at + blocks * 255];
        interleaved(&system, 239, encoded, blocks, what);
        let data = deinterleave(encoded, blocks, 239, blocks * 239);
        for copy in 1..factor as usize {
            assert_eq!(data[copy * aligned..(copy + 1) * aligned], data[..aligned], "{name}: copies of {what}");
        }
        assert_eq!(data[total..], padding[..blocks * 239 - total], "{name}: fill of {what}");
        let packed = &data[..compressed as usize];
        let plain = if compressed < uncompressed {
            decompress_r21(&data, 0, compressed as usize, uncompressed as usize)
        } else {
            packed.to_vec()
        };
        assert_eq!(crc64_mirrored(update_seed1(0, compressed), packed), crc_compressed, "{name}: CRC of {what}");
        assert_eq!(crc64_mirrored(update_seed1(0, uncompressed), &plain), crc_uncompressed, "{name}: CRC of {what}");
    };
    // ファイルの頭のデータの値: 2 と 16 はページの表の CRC、21 と 26 は節の表の CRC
    let page_map_at = 0x480 + header[r21::PAGES_MAP_OFFSET] as usize;
    let values = |indexes: [usize; 5]| indexes.map(|index| header[index]);
    system_page(page_map_at, values([10, 11, 3, 2, 16]), "the page map");
    let (sections_at, _) = container.page(header[r21::SECTIONS_MAP_ID] as i64).unwrap();
    system_page(sections_at, values([22, 25, 27, 26, 21]), "the section map");

    // データのページ（5.4）: 誤り訂正（符号化 4 は交互、1 はデータの後に組ごと）、CRC、検査の値
    let mut pages = 0;
    for section in &container.sections {
        for page in &section.pages {
            let (at, size) = container.page(page.id).unwrap();
            let encoded = &bytes[at..at + size];
            let aligned = (page.compressed as usize).div_ceil(8) * 8;
            let blocks = aligned.div_ceil(251);
            let what = format!("page {} of {}", page.id, section.name);
            let data = if section.encoding == 4 {
                interleaved(&data_code, 251, encoded, blocks, &what);
                let data = deinterleave(encoded, blocks, 251, blocks * 251);
                assert_eq!(data[aligned..], padding[..blocks * 251 - aligned], "{name}: fill of {what}");
                data
            } else {
                for (number, block) in encoded[..aligned].chunks(251).enumerate() {
                    let mut full = block.to_vec();
                    full.extend_from_slice(&padding[..251 - block.len()]);
                    let at = aligned + number * 4;
                    assert_eq!(
                        data_code.parity(&full),
                        encoded[at..at + 4],
                        "{name}: Reed-Solomon of {what}, block {number}"
                    );
                }
                encoded[..aligned].to_vec()
            };
            let packed = &data[..page.compressed as usize];
            assert_eq!(crc64_mirrored(update_seed1(0, page.compressed), packed), page.crc, "{name}: CRC of {what}");
            let plain = if page.compressed < page.uncompressed {
                decompress_r21(&data, 0, page.compressed as usize, page.uncompressed as usize)
            } else {
                packed.to_vec()
            };
            assert_eq!(u64::from(page_checksum_r21(0, &plain)), page.checksum, "{name}: checksum of {what}");
            pages += 1;
        }
    }
    assert!(pages > 0, "{name}: pages");
    pages
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
            Some(Version::R2007) => {
                check_r2007(&name, &bytes);
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

/// R2007 の書き直し: 書いた図面も AutoCAD の図面と同じ検査に通り、節は元と同じ中身と値（ページの大きさ、符号化、
/// 暗号化、ハッシュ）になる
#[test]
fn rewritten_r2007_drawings_keep_every_section() {
    use crate::file::R2007Container;
    for (name, bytes) in samples() {
        if Version::from_magic(&bytes[..6]) != Some(Version::R2007) {
            continue;
        }
        let written = crate::rewrite_dwg(&bytes).unwrap();
        assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
        check_r2007(&name, &written.bytes);
        let before = R2007Container::open(&bytes).unwrap();
        let after = R2007Container::open(&written.bytes).unwrap();
        let names = |container: &R2007Container| -> Vec<String> {
            container.sections.iter().map(|section| section.name.clone()).collect()
        };
        assert_eq!(names(&before), names(&after), "{name}");
        let mut notices = NoticeList::new();
        // プレビューの画像の位置はファイルの中の位置なので、ページの始まりからの位置にして比べる
        let relative = |container: &R2007Container, info: &crate::file::Section21, mut data: Vec<u8>| {
            if info.name == "AcDb:Preview" {
                let (page, _) = container.page(info.pages[0].id).unwrap();
                for index in 0..usize::from(data[20]) {
                    let field = 21 + index * 9 + 1;
                    let start = u32_at(&data, field) - page as u32;
                    data[field..field + 4].copy_from_slice(&start.to_le_bytes());
                }
            }
            data
        };
        for info in &before.sections {
            let original = relative(&before, info, before.data(info, &mut notices));
            let rewritten_info = after.info(&info.name).unwrap();
            let rewritten = relative(&after, rewritten_info, after.data(rewritten_info, &mut notices));
            assert_eq!(
                (rewritten_info.max_size, rewritten_info.encoding, rewritten_info.encryption, rewritten_info.hash),
                (info.max_size, info.encoding, info.encryption, info.hash),
                "{name}: {}",
                info.name
            );
            if info.name == "AcDb:Header" {
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
        if matches!(version, Version::R13 | Version::R14) {
            continue;
        }
        let section = section_of(&bytes, version, 1, "AcDb:Classes");
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

/// 節の中身（R2004 以降は伸長したもの）。number は R2000 の節の位置の記録の番号、name は R2004 以降の節の名前
fn section_of(bytes: &[u8], version: Version, number: u8, name: &str) -> Vec<u8> {
    let mut notices = NoticeList::new();
    match version {
        Version::R2000 => {
            let records = file::r2000_records(bytes).unwrap();
            file::r2000_slice(bytes, &records, number).unwrap().to_vec()
        }
        Version::R2007 => file::R2007Container::open(bytes).unwrap().section(name, &mut notices).unwrap(),
        _ => R2004Container::open(bytes).unwrap().section(name, &mut notices).unwrap(),
    }
}

/// 区切りで囲む節の値（R2004 以降は伸長したもの）。number は R2000 の節の位置の記録の番号
fn framed_section(bytes: &[u8], version: Version, number: u8, name: &str) -> Framed {
    let section = section_of(bytes, version, number, name);
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
        if matches!(version, Version::R13 | Version::R14) {
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
fn entity_name(kind: u16, class: &str) -> &str {
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
            let handles = section_of(bytes, version, 2, "AcDb:Handles");
            (section_of(bytes, version, 0, "AcDb:AcDbObjects"), read_object_map(&handles))
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
        if matches!(version, Version::R13 | Version::R14) {
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

/// 節の表（名前・大きさ・ページの大きさ・圧縮・暗号化・ページ）と中身の頭を表示する（新しい図面の決め方を AutoCAD の
/// 図面から調べるため）。`ZUMEN_DUMP=<名前,…> ZUMEN_BYTES=<表示するバイト数>`
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
            if !kinds.contains(&kname) {
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

/// ヘッダ変数をすべて表示する。`ZUMEN_DUMP=<名前,…>`
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

/// クラスの定義を表示する。`ZUMEN_DUMP=<名前,…>`
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
