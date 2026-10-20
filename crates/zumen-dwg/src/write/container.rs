//! DWG の入れ物を書く。R2000 はファイルの中の位置で節を並べ（3「R13-R15 DWG FILE FORMAT ORGANIZATION」）、
//! R2004・R2010〜R2018 は節を圧縮したページに分ける（4「R2004 DWG FILE FORMAT ORGANIZATION」、6〜8）。
//! 並べ方は AutoCAD が書いた図面に合わせ、読み取り（file.rs）と同じ値に戻ることを確かめる

use super::bits::BitWriter;
use super::compress::compress_r18;
use super::sections::object_map;
use crate::crc::{CRC8_SEED, crc8, crc32, page_checksum};
use crate::file::magic_sequence;

/// AcDb:AcDbObjects の始まりの RL（20。意味は分からないが、AutoCAD はつねに書く。R2000 もクラスの節の後に書く）
pub(crate) const OBJECTS_START: u32 = 0x0dca;

/// 並べるオブジェクト。bytes は MS の大きさから CRC までのそのままのバイト列、handles はそれを指すハンドル
#[derive(Debug, Clone, Default)]
pub(crate) struct Objects {
    pub items: Vec<Vec<u8>>,
    /// (ハンドル, items の添字)
    pub handles: Vec<(u64, usize)>,
}

impl Objects {
    /// start の位置から順に並べたバイト列（0x0DCA の RL から）と、オブジェクトの表に書く (ハンドル, 位置)。
    /// 位置は R2000 はファイルの中（start はこの並びを置く位置）、R2004 以降は AcDb:AcDbObjects の中（start は 0）
    pub fn lay_out(&self, start: u64) -> (Vec<u8>, Vec<(u64, u64)>) {
        let total: usize = self.items.iter().map(Vec::len).sum();
        let mut stream = Vec::with_capacity(total + 4);
        stream.extend_from_slice(&OBJECTS_START.to_le_bytes());
        let mut positions = Vec::with_capacity(self.items.len());
        for item in &self.items {
            positions.push(start + stream.len() as u64);
            stream.extend_from_slice(item);
        }
        let mut locations: Vec<(u64, u64)> = self
            .handles
            .iter()
            .filter_map(|&(handle, index)| positions.get(index).map(|position| (handle, *position)))
            .collect();
        locations.sort_unstable_by_key(|(handle, _)| *handle);
        locations.dedup_by_key(|(handle, _)| *handle);
        (stream, locations)
    }
}

/// プレビューの画像（14.2）。中の画像の位置はファイルの中の位置なので、置いた位置に合わせて直す
#[derive(Debug, Clone)]
pub(crate) struct Preview {
    /// 始まりの区切りから終わりの区切りまで
    pub data: Vec<u8>,
    /// data の位置の基準（元の図面で data が始まっていた位置。新しく作った画像は 0）
    pub base: u64,
}

/// 画像のない、AutoCAD が書く形のプレビュー（大きさ 1、画像の数 0）
pub(crate) const PREVIEW_START: [u8; 16] =
    [0x1f, 0x25, 0x6d, 0x07, 0xd4, 0x36, 0x28, 0x28, 0x9d, 0x57, 0xca, 0x3f, 0x9d, 0x44, 0x10, 0x2b];
pub(crate) const PREVIEW_END: [u8; 16] =
    [0xe0, 0xda, 0x92, 0xf8, 0x2b, 0xc9, 0xd7, 0xd7, 0x62, 0xa8, 0x35, 0xc0, 0x62, 0xbb, 0xef, 0xd4];

impl Preview {
    pub fn empty() -> Self {
        let mut data = PREVIEW_START.to_vec();
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&PREVIEW_END);
        Self { data, base: 0 }
    }

    /// at に置いたときのバイト列。画像の始まりの位置（RL）を、元の基準からの差のまま at を基準にする
    fn placed(&self, at: u64) -> Vec<u8> {
        let mut data = self.data.clone();
        let count = usize::from(*data.get(20).unwrap_or(&0));
        for index in 0..count {
            let field = 21 + index * 9 + 1;
            let Some(bytes) = data.get(field..field + 4) else { break };
            let old = u64::from(u32::from_le_bytes(bytes.try_into().unwrap_or_default()));
            if old >= self.base {
                let new = (old - self.base + at) as u32;
                data[field..field + 4].copy_from_slice(&new.to_le_bytes());
            }
        }
        data
    }
}

// --- R2000 ------------------------------------------------------------------

/// R2000 のファイルの頭の後の区切り（3.2.6）
const R2000_HEAD_END: [u8; 16] =
    [0x95, 0xa0, 0x4e, 0x28, 0x99, 0x82, 0x1a, 0xe5, 0x5e, 0x41, 0xe0, 0x5f, 0x9d, 0x3a, 0x4d, 0x00];
/// 2 つ目のファイルの頭（26）の始まりと終わりの区切り
const SECOND_HEADER_START: [u8; 16] =
    [0xd4, 0x7b, 0x21, 0xce, 0x28, 0x93, 0x9f, 0xbf, 0x53, 0x24, 0x40, 0x09, 0x12, 0x3c, 0xaa, 0x01];
const SECOND_HEADER_END: [u8; 16] =
    [0x2b, 0x84, 0xde, 0x31, 0xd7, 0x6c, 0x60, 0x40, 0xac, 0xdb, 0xbf, 0xf6, 0xed, 0xc3, 0x55, 0xfe];

/// 書く R2000 の図面
pub(crate) struct R2000 {
    /// ファイルの頭の 0x00〜0x15。版（0x00）、0x06〜0x0C、書いたアプリの版と文字コード（0x11〜0x14）を使う。
    /// 0x0D の画像の位置と 0x15 の節の数は書くときに決める
    pub head: [u8; 0x15],
    /// AcDb:AuxHeader（27）。R2000 では 5 番の節の位置の記録で指す
    pub aux_header: Option<Vec<u8>>,
    pub preview: Preview,
    /// ヘッダ変数の節（区切りから区切りまで）
    pub header: Vec<u8>,
    /// クラスの節（区切りから区切りまで）
    pub classes: Vec<u8>,
    pub objects: Objects,
    /// AcDb:Template（22）
    pub template: Option<Vec<u8>>,
    /// 2 つ目のファイルの頭のハンドルの記録（26）。0 HANDSEED、1 ブロック、2 レイヤ、3 文字スタイル、4 線種、5 ビュー、
    /// 6 UCS、7 ビューポート、8 アプリケーション、9 寸法スタイルの表、10 ビューポートの図形のヘッダの表、
    /// 11 名前の付いたオブジェクトの辞書、12 マルチラインのスタイルの辞書、13 グループの辞書
    pub handles: [u64; 14],
}

/// R2000 の図面を、AutoCAD と同じ並べ方で書く: ファイルの頭、AuxHeader、プレビュー、ヘッダ変数、クラス、
/// 0x0DCA とオブジェクト、オブジェクトの表、2 つ目のファイルの頭、Template
pub(crate) fn write_r2000(file: &R2000) -> Vec<u8> {
    // 節の位置の記録: 0 ヘッダ変数、1 クラス、2 オブジェクトの表、3（AutoCAD は書かない）、4 Template、5 AuxHeader
    let records = if file.aux_header.is_some() { 6 } else { 5 };
    let head_size = 0x19 + records * 9 + 2 + 16;
    let mut out = vec![0u8; head_size];
    if let Some(aux) = &file.aux_header {
        out.extend_from_slice(aux);
    }
    let aux_at = head_size;
    let preview_at = out.len();
    out.extend_from_slice(&file.preview.placed(preview_at as u64));
    let header_at = out.len();
    out.extend_from_slice(&file.header);
    let classes_at = out.len();
    out.extend_from_slice(&file.classes);
    let (stream, locations) = file.objects.lay_out(out.len() as u64);
    out.extend_from_slice(&stream);
    let map_at = out.len();
    out.extend_from_slice(&object_map(&locations));
    let map_size = out.len() - map_at;

    let mut list: Vec<(u8, u64, u64)> = vec![
        (0, header_at as u64, file.header.len() as u64),
        (1, classes_at as u64, file.classes.len() as u64),
        (2, map_at as u64, map_size as u64),
        (3, 0, 0),
    ];
    // 2 つ目のファイルの頭の 4 番は、AutoCAD と同じく 0（Template の位置はファイルの頭の記録にだけ書く）
    let mut second_list = list.clone();
    second_list.push((4, 0, 0));
    if let Some(aux) = &file.aux_header {
        second_list.push((5, aux_at as u64, aux.len() as u64));
    }
    let second_at = out.len();
    out.extend_from_slice(&second_header(second_at as u64, &file.head, &second_list, &file.handles));
    let template_at = out.len();
    let template_size = file.template.as_ref().map_or(0, Vec::len);
    if let Some(template) = &file.template {
        out.extend_from_slice(template);
    }
    list.push((4, if template_size > 0 { template_at as u64 } else { 0 }, template_size as u64));
    if let Some(aux) = &file.aux_header {
        list.push((5, aux_at as u64, aux.len() as u64));
    }

    // ファイルの頭（3.2）。版、0x06〜0x0C、画像の位置、アプリの版と文字コード、節の位置の記録、CRC、区切り
    out[..0x0d].copy_from_slice(&file.head[..0x0d]);
    out[0x0d..0x11].copy_from_slice(&(preview_at as u32).to_le_bytes());
    out[0x11..0x15].copy_from_slice(&file.head[0x11..0x15]);
    out[0x15..0x19].copy_from_slice(&(records as u32).to_le_bytes());
    for (index, (number, at, size)) in list.iter().enumerate() {
        let base = 0x19 + index * 9;
        out[base] = *number;
        out[base + 1..base + 5].copy_from_slice(&(*at as u32).to_le_bytes());
        out[base + 5..base + 9].copy_from_slice(&(*size as u32).to_le_bytes());
    }
    // CRC は 0 から始めて節の数ごとの値と排他的論理和をとる（3.2.6）。0xC0C1 から始めるのと同じ値になる
    let crc_at = 0x19 + records * 9;
    let crc = crc8(CRC8_SEED, &out[..crc_at]);
    out[crc_at..crc_at + 2].copy_from_slice(&crc.to_le_bytes());
    out[crc_at + 2..crc_at + 18].copy_from_slice(&R2000_HEAD_END);
    out
}

/// 2 つ目のファイルの頭（26）。仕様の並びは R13・R14 のもので、AutoCAD の R2000 の図面は次の並びをビット単位で書く
/// （実物の図面で確かめた）: この頭の位置（BL）、版の 6 文字と 0x06〜0x0C の 7 バイト（RC）、ファイルの頭の 0x11〜0x14
/// （BL）、節の位置の記録の数（BL）と記録（RC 番号・BL 位置・BL 大きさ）、ハンドルの記録の数（BS 14）と記録（RC の
/// バイト数・RC 番号・上位のバイトからのハンドル）。バイトの境目まで 0 で埋め、大きさの欄から CRC（初めの値 0xC0C1）を
/// とり、8 バイトの値（AutoCAD は不定の値、ここでは 0）を続ける。大きさは大きさの欄から終わりの区切りの前まで
fn second_header(address: u64, head: &[u8; 0x15], records: &[(u8, u64, u64)], handles: &[u64; 14]) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.bl(address as i32);
    w.raw(&head[..0x0d]);
    w.bl(i32::from_le_bytes([head[0x11], head[0x12], head[0x13], head[0x14]]));
    w.bl(records.len() as i32);
    for &(number, at, size) in records {
        w.rc(number);
        w.bl(at as i32);
        w.bl(size as i32);
    }
    w.bs(handles.len() as i16);
    for (index, handle) in handles.iter().enumerate() {
        let bytes = handle.to_be_bytes();
        let skip = bytes.iter().position(|byte| *byte != 0).unwrap_or(8);
        w.rc((8 - skip) as u8);
        w.rc(index as u8);
        w.raw(&bytes[skip..]);
    }
    w.pad();
    let body = w.into_bytes();
    let size = 4 + body.len() + 2 + 8;
    let mut out = SECOND_HEADER_START.to_vec();
    out.extend_from_slice(&(size as u32).to_le_bytes());
    out.extend_from_slice(&body);
    let crc = crc8(CRC8_SEED, &out[16..]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&SECOND_HEADER_END);
    out
}

// --- R2004・R2010〜R2018 ------------------------------------------------------

/// データの節（4.5）
#[derive(Debug, Clone)]
pub(crate) struct DataSection {
    pub name: String,
    /// 伸長した中身
    pub data: Vec<u8>,
    /// ページの大きさ（伸長した後、Max Decompressed Size）
    pub page_size: usize,
    pub compressed: bool,
    /// 0 は暗号化なし、1 はあり、2 は不明（AcDb:FileDepList など、AutoCAD が 2 を書く節）
    pub encrypted: u32,
    /// AcDb:Preview の画像の位置の基準（Preview::base と同じ）。ほかの節は None
    pub preview_base: Option<u64>,
    /// 0 だけのページも書く（AutoCAD は省く。読み取りは省いた節を空と読むので、値を残したい節に使う）
    pub keep_zero_pages: bool,
}

/// 書く R2004 以降の図面
pub(crate) struct R2004 {
    /// ファイルの頭の 0x00〜0x80（4.1）。書くときに、プレビュー（0x0D）・SummaryInfo（0x20）・VBAProject（0x24）・
    /// AppInfo（0x2C）・AppInfoHistory（0x30）のページの位置を書き直す
    pub head: [u8; 0x80],
    /// 節の表に書く順の節。ID は空の名前の節が 0、ほかはこの順に数の多い方から 1 まで（4.5）
    pub sections: Vec<DataSection>,
    /// ファイルの中に置く順（sections の添字）
    pub stream_order: Vec<usize>,
}

const DATA_PAGE: u32 = 0x4163_043b;
const SECTION_MAP: u32 = 0x4163_003b;
const PAGE_MAP: u32 = 0x4163_0e3b;

fn round_up(value: usize, unit: usize) -> usize {
    value.div_ceil(unit) * unit
}

/// R2004 以降の図面を書く。ページは 0x100 から、データのページ、節の表、ページの表の順。ページは 1 から番号を
/// 振り、0 だけのページは書かない（4.5。AutoCAD と同じ）。最後にファイルの頭のデータの写しを置く
pub(crate) fn write_r2004(file: &R2004) -> Vec<u8> {
    let magic = magic_sequence(0x100);
    let mut out = vec![0u8; 0x100];
    let ids = section_ids(&file.sections);
    // (ページの番号, ページの大きさ) をファイルの中の順に
    let mut pages: Vec<(i32, u32)> = Vec::new();
    // 節ごとの (ページの番号, データの大きさ, 伸長した節の中の位置)
    let mut section_pages: Vec<Vec<(i32, u32, u64)>> = vec![Vec::new(); file.sections.len()];
    let mut first_address: Vec<Option<usize>> = vec![None; file.sections.len()];
    for &index in &file.stream_order {
        let Some(section) = file.sections.get(index) else { continue };
        let page_size = if section.page_size == 0 { 0x7400 } else { section.page_size };
        let placed;
        let data = match section.preview_base {
            // プレビューは 1 つのページに収まるときだけ、置く位置に合わせて画像の位置を直す
            Some(base) if section.data.len() <= page_size => {
                placed = Preview { data: section.data.clone(), base }.placed(out.len() as u64 + 0x20);
                &placed
            }
            _ => &section.data,
        };
        for (number, chunk) in data.chunks(page_size).enumerate() {
            if !section.keep_zero_pages && chunk.iter().all(|byte| *byte == 0) {
                continue;
            }
            let start = (number * page_size) as u64;
            let mut page = chunk.to_vec();
            page.resize(page_size, 0);
            let payload = if section.compressed { compress_r18(&page) } else { page };
            let address = out.len();
            let extent = round_up(0x20 + payload.len(), 0x20);
            let page_number = pages.len() as i32 + 1;
            let data_checksum = page_checksum(0, &payload);
            let mut head = [0u32; 8];
            head[0] = DATA_PAGE;
            head[1] = ids[index];
            head[2] = payload.len() as u32;
            head[3] = extent as u32;
            head[4] = start as u32;
            head[5] = (start >> 32) as u32;
            head[7] = data_checksum;
            head[6] = page_checksum(data_checksum, &words(&head));
            let mask = 0x4164_536b_u32 ^ address as u32;
            for value in head {
                out.extend_from_slice(&(value ^ mask).to_le_bytes());
            }
            out.extend_from_slice(&payload);
            let pad = address + extent - out.len();
            out.extend_from_slice(&magic[..pad]);
            pages.push((page_number, extent as u32));
            section_pages[index].push((page_number, payload.len() as u32, start));
            first_address[index].get_or_insert(address);
        }
    }

    // 節の表（4.5）
    let count = file.sections.len() as u32;
    let mut map = Vec::new();
    for value in [count, 2, 0x7400, 0, count] {
        map.extend_from_slice(&value.to_le_bytes());
    }
    for (index, section) in file.sections.iter().enumerate() {
        map.extend_from_slice(&(section.data.len() as u64).to_le_bytes());
        let page_size = if section.page_size == 0 { 0x7400 } else { section.page_size };
        for value in [
            section_pages[index].len() as u32,
            page_size as u32,
            1,
            if section.compressed { 2 } else { 1 },
            ids[index],
            section.encrypted,
        ] {
            map.extend_from_slice(&value.to_le_bytes());
        }
        let mut name = [0u8; 64];
        let bytes: Vec<u8> = section.name.chars().map(|c| if c.is_ascii() { c as u8 } else { b'?' }).take(63).collect();
        name[..bytes.len()].copy_from_slice(&bytes);
        map.extend_from_slice(&name);
        for &(number, size, start) in &section_pages[index] {
            map.extend_from_slice(&number.to_le_bytes());
            map.extend_from_slice(&size.to_le_bytes());
            map.extend_from_slice(&start.to_le_bytes());
        }
    }
    let section_map_id = pages.len() as i32 + 1;
    let section_map_at = out.len();
    let compressed = compress_r18(&map);
    write_system_page(&mut out, SECTION_MAP, &map, &compressed);
    let extent = round_up(out.len() - section_map_at, 0x20);
    let pad = section_map_at + extent - out.len();
    out.extend_from_slice(&magic[..pad]);
    pages.push((section_map_id, extent as u32));

    // ページの表（4.4）。ページの表自身の大きさも並べるので、圧縮した大きさが収まるまで繰り返す
    let page_map_id = section_map_id + 1;
    let page_map_at = out.len();
    let entries = |extent: u32| -> Vec<u8> {
        let mut data = Vec::with_capacity((pages.len() + 1) * 8);
        for &(number, size) in pages.iter().chain(std::iter::once(&(page_map_id, extent))) {
            data.extend_from_slice(&number.to_le_bytes());
            data.extend_from_slice(&size.to_le_bytes());
        }
        data
    };
    let mut extent = round_up(0x28 + compress_r18(&entries(0)).len(), 0x20) as u32;
    let (page_map, compressed) = loop {
        let data = entries(extent);
        let compressed = compress_r18(&data);
        let needed = round_up(0x28 + compressed.len(), 0x20) as u32;
        if needed <= extent {
            break (data, compressed);
        }
        extent = needed;
    };
    write_system_page(&mut out, PAGE_MAP, &page_map, &compressed);

    // ファイルの頭のデータ（4.1）。CRC32 は CRC の欄を 0 にした 0x6C バイトから、初めの値 0
    let second_at = out.len();
    let mut data = [0u8; 0x6c];
    data[..11].copy_from_slice(b"AcFssFcAJMB");
    let mut put = |at: usize, value: u32| data[at..at + 4].copy_from_slice(&value.to_le_bytes());
    put(0x10, 0x6c);
    put(0x14, 4);
    put(0x24, 1);
    put(0x28, page_map_id as u32);
    put(0x40, pages.len() as u32 + 1);
    put(0x44, 0x20);
    put(0x48, 0x80);
    put(0x4c, 0x40);
    put(0x50, page_map_id as u32);
    put(0x5c, section_map_id as u32);
    put(0x60, page_map_id as u32);
    data[0x2c..0x34].copy_from_slice(&((page_map_at + extent as usize - 0x100) as u64).to_le_bytes());
    data[0x34..0x3c].copy_from_slice(&(second_at as u64).to_le_bytes());
    data[0x54..0x5c].copy_from_slice(&((page_map_at - 0x100) as u64).to_le_bytes());
    let crc = crc32(0, &data);
    data[0x68..0x6c].copy_from_slice(&crc.to_le_bytes());
    for (byte, mask) in data.iter_mut().zip(&magic) {
        *byte ^= mask;
    }
    out.extend_from_slice(&data);

    // ファイルの頭（0x00〜0x100）。節のページの位置は、ページの頭（0x20 バイト）の後の中身の位置
    let mut head = file.head;
    let address = |name: &str| -> Option<u32> {
        let index = file.sections.iter().position(|section| section.name == name)?;
        first_address[index].map(|at| at as u32 + 0x20)
    };
    let mut set = |at: usize, value: u32| head[at..at + 4].copy_from_slice(&value.to_le_bytes());
    set(0x0d, address("AcDb:Preview").unwrap_or(0));
    set(0x20, address("AcDb:SummaryInfo").unwrap_or(0));
    set(0x24, address("AcDb:VBAProject").unwrap_or(0));
    set(0x2c, address("AcDb:AppInfo").unwrap_or(0));
    if let Some(at) = address("AcDb:AppInfoHistory") {
        set(0x30, at);
    }
    out[..0x80].copy_from_slice(&head);
    out[0x80..0xec].copy_from_slice(&data);
    out[0xec..0x100].copy_from_slice(&magic[0xec..0x100]);
    out
}

/// 節の ID（4.5）。空の名前の節は 0、ほかは並ぶ順に数の多い方から 1 まで
fn section_ids(sections: &[DataSection]) -> Vec<u32> {
    let mut next = sections.iter().filter(|section| !section.name.is_empty()).count() as u32;
    sections
        .iter()
        .map(|section| {
            if section.name.is_empty() {
                0
            } else {
                let id = next;
                next = next.saturating_sub(1);
                id
            }
        })
        .collect()
}

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

/// システムのページ（4.3）。0x14 バイトの頭（種類、伸長した大きさ、圧縮した大きさ、圧縮の種類 2、検査の値）、
/// 圧縮したデータ、AutoCAD と同じく大きさと検査の値を 0 にした頭の写し
fn write_system_page(out: &mut Vec<u8>, kind: u32, data: &[u8], compressed: &[u8]) {
    let mut head = [kind, data.len() as u32, compressed.len() as u32, 2, 0];
    let checksum = page_checksum(page_checksum(0, &words(&head)), compressed);
    head[4] = checksum;
    out.extend_from_slice(&words(&head));
    out.extend_from_slice(compressed);
    out.extend_from_slice(&words(&[kind, 0, 0, 2, 0]));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::{R2004Container, magic_sequence as magic};
    use zumen_core::NoticeList;

    fn section(name: &str, data: Vec<u8>, page_size: usize, compressed: bool) -> DataSection {
        DataSection {
            name: name.into(),
            data,
            page_size,
            compressed,
            encrypted: 0,
            preview_base: None,
            keep_zero_pages: false,
        }
    }

    #[test]
    fn an_r2004_container_reads_back() {
        let objects: Vec<u8> = (0..70_000u32).map(|index| (index % 251) as u8 ^ (index / 1000) as u8).collect();
        let sections = vec![
            section("", Vec::new(), 0x7400, true),
            section("AcDb:SummaryInfo", vec![1, 2, 3], 0x100, false),
            section("AcDb:AcDbObjects", objects.clone(), 0x7400, true),
            section("AcDb:Template", vec![0, 0, 0, 0], 0x7400, true),
            section("AcDb:Header", b"header".to_vec(), 0x7400, true),
        ];
        let mut head = [0u8; 0x80];
        head[..6].copy_from_slice(b"AC1027");
        let bytes = write_r2004(&R2004 { head, sections, stream_order: vec![1, 2, 3, 4, 0] });
        let container = R2004Container::open(&bytes).unwrap();
        let mut notices = NoticeList::new();
        assert_eq!(container.section("AcDb:AcDbObjects", &mut notices).unwrap(), objects);
        assert_eq!(container.section("AcDb:SummaryInfo", &mut notices).unwrap(), [1, 2, 3]);
        // 0 だけの節はページを書かず、大きさだけを書く（4.5）
        let template = container.info("AcDb:Template").unwrap();
        assert!(template.pages.is_empty());
        assert_eq!(template.size, 4);
        assert_eq!(container.section("AcDb:Header", &mut notices).unwrap(), b"header");
        assert!(notices.into_vec().is_empty());
        // ファイルの頭の写しが終わりにある
        assert_eq!(bytes[bytes.len() - 0x6c..], bytes[0x80..0xec]);
        assert_eq!(bytes[0xec..0x100], magic(0x100)[0xec..]);
        // SummaryInfo は 0x100 の最初のページ
        assert_eq!(u32::from_le_bytes(bytes[0x20..0x24].try_into().unwrap()), 0x120);
    }
}
