//! R2007（AC1021）の入れ物を書く（5「R2007 DWG FILE FORMAT ORGANIZATION」）。ページは Reed-Solomon で符号化し
//! （5.13）、検査の値は 64 ビットの CRC（5.12）、詰め物はランダムの並び（5.11）から作る。仕様が書かない決まりは、
//! AutoCAD が書いた図面のすべてのページの値と照らした（samples.rs）:
//!
//! - Reed-Solomon: システムのページは (255, 239) で原始多項式 x^8 + x^6 + x^5 + x^3 + 1（0x169）、データのページは
//!   (255, 251) で x^8 + x^4 + x^3 + x^2 + 1（0x11D）。生成多項式の根は α^1 から。組はデータの後に誤り訂正の
//!   バイトを次数の低い順に置く（仕様が挙げる Rockliff の実装と同じ並び）
//! - 詰め物: ファイルの頭の RandomSeed から作る 512 バイトの並び（5.11 の padding）を、初めから使う。組を埋める
//!   バイト、ページの後ろ、ファイルの頭のページの後ろのどれも同じ
//! - 符号化 1 のページ（AcDb:Preview など、圧縮しない節）は、データ（8 の倍数に 0 で揃える）の後に組ごとの誤り訂正
//!   （4 バイト）を並べる。終わりの組の足りない分は詰め物で埋めて計算し、書かない。ページの大きさは、ページの最大の
//!   大きさのデータが入る大きさ（0x20 の倍数）
//! - 符号化 4 のページは、組を 1 バイトずつ交互に並べ、ページの大きさは組の大きさを 0x20 の倍数に揃えたもの
//! - CRC: データのページとシステムのページは mirrored（UpdateSeed1(0, 長さ)）、ファイルの頭のデータとその圧縮は
//!   normal（UpdateSeed2）、確かめの並びは normal（UpdateSeed1）。データのページの検査の値（5.4.1）は伸長した
//!   データから、初めの値 0 で計算する
//! - ページの番号は、データのページがファイルの中の順に 3 から、続いて節の表とその写し、2 つ空けてページの表と
//!   その写し（AutoCAD と同じ）。ページの表の写しはページの表のすぐ後、節の表の写しは節の表のすぐ後、ファイルの頭の
//!   写しはファイルの終わり
//!
//! 0 だけのページも書く（読み取りは、ページのない節を空と読むため）。圧縮は compress21.rs

use super::compress21::compress_r21;
use super::container::Preview;
use crate::crc::{crc64_mirrored, crc64_normal, page_checksum_r21, update_seed1, update_seed2};

const SYSTEM_K: usize = 239;
const DATA_K: usize = 251;
/// 5.11 の表の大きさ
const TABLE_SIZE: usize = 0x270;

/// GF(2^8) の Reed-Solomon (255, k) の符号（5.13）
pub(crate) struct ReedSolomon {
    k: usize,
    exp: [u8; 510],
    log: [u8; 256],
    /// 生成多項式の係数（次数の低い順、最高次の 1 を除く）
    generator: Vec<u8>,
}

impl ReedSolomon {
    /// poly は x^8 を含む原始多項式
    fn new(k: usize, poly: u16) -> Self {
        let mut exp = [0u8; 510];
        let mut log = [0u8; 256];
        let mut x: u16 = 1;
        for i in 0..255 {
            exp[i] = x as u8;
            exp[i + 255] = x as u8;
            log[usize::from(x)] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= poly;
            }
        }
        let mul = |a: u8, b: u8| {
            if a == 0 || b == 0 { 0 } else { exp[usize::from(log[usize::from(a)]) + usize::from(log[usize::from(b)])] }
        };
        // g(x) = (x + α)(x + α^2)…(x + α^(255 − k))
        let mut generator = vec![1u8];
        for &root in &exp[1..=255 - k] {
            let mut next = vec![0u8; generator.len() + 1];
            for (degree, &coefficient) in generator.iter().enumerate() {
                next[degree + 1] ^= coefficient;
                next[degree] ^= mul(coefficient, root);
            }
            generator = next;
        }
        generator.pop();
        Self { k, exp, log, generator }
    }

    /// システムのページの符号（255, 239）
    pub fn system() -> Self {
        Self::new(SYSTEM_K, 0x169)
    }

    /// データのページの符号（255, 251）
    pub fn data() -> Self {
        Self::new(DATA_K, 0x11d)
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[usize::from(self.log[usize::from(a)]) + usize::from(self.log[usize::from(b)])]
        }
    }

    /// k バイトの組の誤り訂正のバイト（255 − k 個、次数の低い順）。組の初めのバイトが次数の低い係数
    pub fn parity(&self, block: &[u8]) -> Vec<u8> {
        let n = 255 - self.k;
        let mut parity = vec![0u8; n];
        for &byte in block.iter().rev() {
            let feedback = byte ^ parity[n - 1];
            for j in (1..n).rev() {
                parity[j] = parity[j - 1] ^ self.mul(self.generator[j], feedback);
            }
            parity[0] = self.mul(self.generator[0], feedback);
        }
        parity
    }

    /// blocks 個の組を 1 バイトずつ交互に並べる（5.13.2）。data は blocks × k バイト
    fn interleaved(&self, data: &[u8], blocks: usize) -> Vec<u8> {
        let mut out = vec![0u8; blocks * 255];
        for (number, block) in data.chunks(self.k).enumerate().take(blocks) {
            for (index, byte) in block.iter().chain(self.parity(block).iter()).enumerate() {
                out[number + index * blocks] = *byte;
            }
        }
        out
    }
}

/// 5.11 の CRC のランダムの符号化。種から表と 512 バイトの詰め物を作る
pub(crate) struct Random21 {
    table: [u32; TABLE_SIZE],
    index: usize,
    padding: [u8; 512],
}

impl Random21 {
    pub fn new(seed: u64) -> Self {
        let mut table = [0u32; TABLE_SIZE];
        table[0] = (seed as u32).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        table[1] = ((seed >> 32) as u32).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        let mut value = table[1];
        for (index, entry) in table.iter_mut().enumerate().skip(2) {
            value = ((value >> 0x1e) ^ value).wrapping_mul(0x6c07_8965).wrapping_add(index as u32);
            *entry = value;
        }
        let mut padding = [0u8; 512];
        for (index, chunk) in padding.chunks_mut(4).enumerate() {
            chunk.copy_from_slice(&table[index].to_le_bytes());
        }
        Self { table, index: 0x80, padding }
    }

    /// 次の 64 ビットの値（表の続く 2 つ）
    pub fn next(&mut self) -> u64 {
        self.index += 2;
        if self.index + 1 >= TABLE_SIZE {
            self.index = 0;
        }
        u64::from(self.table[self.index]) | (u64::from(self.table[self.index + 1]) << 32)
    }

    /// value の下位 10 ビットをランダムの値の決まったビットに置く（5.11 の Encode）
    pub fn encode(&mut self, value: u32) -> u64 {
        let random = self.next();
        let mut low = (random as u32) & 0xdf7d_f7df;
        let mut high = ((random >> 32) as u32) & 0xf7df_7df7;
        for (bit, mask) in [(0x200, 0x20), (0x100, 0x800), (0x80, 0x20000), (0x40, 0x80_0000), (0x20, 0x2000_0000)] {
            if value & bit != 0 {
                low |= mask;
            }
        }
        for (bit, mask) in [(0x10, 0x08), (0x8, 0x200), (0x4, 0x8000), (0x2, 0x20_0000), (0x1, 0x800_0000)] {
            if value & bit != 0 {
                high |= mask;
            }
        }
        u64::from(low) | (u64::from(high) << 32)
    }

    /// 詰め物の並び（512 バイト）
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn padding(&self) -> &[u8] {
        &self.padding
    }

    /// out が length バイトになるまで詰め物を足す（512 バイトの並びを初めから繰り返す）
    fn pad_to(&self, out: &mut Vec<u8>, length: usize) {
        let count = length.saturating_sub(out.len());
        out.extend((0..count).map(|index| self.padding[index % self.padding.len()]));
    }
}

/// 確かめのデータの Encode（5.2.1.1.5）。control の下位 5 ビットだけ左へ回す
fn rotate(value: u64, control: u64) -> u64 {
    value.rotate_left((control & 0x1f) as u32)
}

fn words(values: &[u64]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

/// ファイルの頭のページの終わりの確かめのデータ（5.2.1.1.5）: normal・mirrored の CRC、2 つのランダムの値、
/// 符号化した CRC の種
fn check_data(random: &mut Random21) -> [u64; 5] {
    let random1 = random.next();
    let random2 = random.next();
    let seed = random.encode(0);
    let (normal, mirrored) = check_crcs(random1, random2);
    [normal, mirrored, random1, random2, seed]
}

/// 確かめのデータの 2 つの CRC（normal, mirrored）。2 つのランダムの値から作る
pub(crate) fn check_crcs(random1: u64, random2: u64) -> (u64, u64) {
    let mut buffer = [0u64; 8];
    buffer[0] = rotate(random1, random2);
    buffer[1] = rotate(buffer[0], buffer[0]);
    buffer[2] = rotate(random2, buffer[1]);
    buffer[3] = rotate(buffer[2], buffer[2]);
    buffer[4] = rotate(random1, buffer[3]);
    buffer[5] = rotate(buffer[4], buffer[4]);
    buffer[6] = rotate(buffer[5], buffer[5]);
    buffer[7] = rotate(buffer[6], buffer[6]);
    let normal = crc64_normal(!random2, &words(&buffer));
    buffer[0] = rotate(random1, random2);
    buffer[1] = rotate(normal, buffer[0]);
    buffer[2] = rotate(random2, buffer[1]);
    buffer[3] = rotate(normal, buffer[2]);
    buffer[4] = rotate(random1, buffer[3]);
    buffer[5] = rotate(normal, buffer[4]);
    buffer[6] = rotate(random2, buffer[5]);
    buffer[7] = rotate(buffer[6], buffer[6]);
    let mirrored = crc64_mirrored(!random1, &words(&buffer));
    (normal, mirrored)
}

fn round_up(value: usize, unit: usize) -> usize {
    value.div_ceil(unit) * unit
}

/// システムのページの大きさ（5.3.1）。圧縮する前のデータが 2 回入る大きさで、0x400 以上
fn system_page_size(size: usize) -> usize {
    let file_page = (round_up(size, 8) * 2).div_ceil(SYSTEM_K) * 255;
    if file_page < 0x400 { 0x400 } else { round_up(file_page, 0x20) }
}

/// 圧縮して短くなれば圧縮したデータ、ならなければ元のデータ
fn compressed(data: &[u8]) -> (Vec<u8>, bool) {
    match compress_r21(data) {
        Some(packed) if packed.len() < data.len() => (packed, true),
        _ => (data.to_vec(), false),
    }
}

/// 符号化したシステムのページ（5.3）と、ファイルの頭に書く値
struct SystemPage {
    bytes: Vec<u8>,
    crc_compressed: u64,
    crc_uncompressed: u64,
    size_compressed: u64,
    size_uncompressed: u64,
    factor: u64,
}

fn system_page(data: &[u8], random: &Random21) -> SystemPage {
    let crc_uncompressed = crc64_mirrored(update_seed1(0, data.len() as u64), data);
    let (packed, _) = compressed(data);
    let crc_compressed = crc64_mirrored(update_seed1(0, packed.len() as u64), &packed);
    let aligned = round_up(packed.len(), 8).max(8);
    let page_size = system_page_size(data.len());
    // 符号化したデータが入るだけ繰り返す
    let factor = ((page_size / 255) * SYSTEM_K / aligned).max(1);
    let mut buffer = Vec::with_capacity(page_size);
    for _ in 0..factor {
        buffer.extend_from_slice(&packed);
        buffer.resize(buffer.len() + aligned - packed.len(), 0);
    }
    let blocks = buffer.len().div_ceil(SYSTEM_K);
    random.pad_to(&mut buffer, blocks * SYSTEM_K);
    let mut bytes = ReedSolomon::system().interleaved(&buffer, blocks);
    random.pad_to(&mut bytes, page_size);
    SystemPage {
        bytes,
        crc_compressed,
        crc_uncompressed,
        size_compressed: packed.len() as u64,
        size_uncompressed: data.len() as u64,
        factor: factor as u64,
    }
}

/// 書く R2007 のデータの節
pub(crate) struct DataSection21 {
    pub name: String,
    pub data: Vec<u8>,
    /// ページの大きさ（伸長した後のデータ）
    pub max_size: usize,
    pub encryption: u64,
    pub hash: u64,
    /// 1: 組を順に並べ、圧縮しない。4: 組を交互に並べ、圧縮する
    pub encoding: u64,
    /// AcDb:Preview の画像の位置の基準（Preview::base と同じ。データはページの始まりに置かれる）
    pub preview_base: Option<u64>,
}

/// 書く R2007 の図面
pub(crate) struct R2007 {
    /// ファイルの頭の 0x80 バイト（5.2 の Meta Data）。節の位置（0x0D、0x20、0x24、0x2C）は書くときに決める
    pub head: [u8; 0x80],
    /// 節の表に並べる順
    pub sections: Vec<DataSection21>,
    /// ファイルの中の節の順（sections の添字）
    pub stream_order: Vec<usize>,
    /// ランダムの並びの種（5.11）。同じ図面から同じバイト列を作るため、元の図面の値を使う
    pub random_seed: u64,
}

/// 節の表に書くページ
struct PageEntry {
    offset: u64,
    id: u64,
    uncompressed: u64,
    compressed: u64,
    checksum: u64,
    crc: u64,
}

/// データのページ（5.4）。符号化したバイト列（0x20 の倍数）と、節の表に書く値
fn data_page(chunk: &[u8], section: &DataSection21, random: &Random21) -> (Vec<u8>, u64, u64, u64) {
    let checksum = u64::from(page_checksum_r21(0, chunk));
    let (packed, _) = if section.encoding == 4 { compressed(chunk) } else { (chunk.to_vec(), false) };
    let crc = crc64_mirrored(update_seed1(0, packed.len() as u64), &packed);
    let aligned = round_up(packed.len(), 8);
    let mut data = packed.clone();
    data.resize(aligned, 0);
    let blocks = aligned.div_ceil(DATA_K);
    let rs = ReedSolomon::data();
    let mut bytes = if section.encoding == 4 {
        random.pad_to(&mut data, blocks * DATA_K);
        rs.interleaved(&data, blocks)
    } else {
        // 組を順に並べる（5.13.1）: データの後に誤り訂正。終わりの組は詰め物で埋めて計算する
        let mut out = data.clone();
        for block in data.chunks(DATA_K) {
            let mut full = block.to_vec();
            random.pad_to(&mut full, DATA_K);
            out.extend_from_slice(&rs.parity(&full));
        }
        out
    };
    let size = if section.encoding == 4 {
        round_up(bytes.len(), 0x20)
    } else {
        round_up(bytes.len().max(section.max_size + section.max_size.div_ceil(DATA_K) * 4), 0x20)
    };
    random.pad_to(&mut bytes, size);
    (bytes, packed.len() as u64, checksum, crc)
}

/// 節の表（5.2）。節ごとの値と名前、ページの値。終わりに名前のない空の節（AutoCAD と同じ）
fn section_map(sections: &[DataSection21], pages: &[Vec<PageEntry>]) -> Vec<u8> {
    let mut map = Vec::new();
    let put = |map: &mut Vec<u8>, value: u64| map.extend_from_slice(&value.to_le_bytes());
    for (section, entries) in sections.iter().zip(pages) {
        let name: Vec<u16> = section.name.encode_utf16().collect();
        let name_length = if name.is_empty() { 0 } else { (name.len() + 1) * 2 };
        for value in [
            section.data.len() as u64,
            section.max_size as u64,
            section.encryption,
            section.hash,
            name_length as u64,
            0,
            section.encoding,
            entries.len() as u64,
        ] {
            put(&mut map, value);
        }
        if !name.is_empty() {
            for unit in name.iter().chain(&[0]) {
                map.extend_from_slice(&unit.to_le_bytes());
            }
        }
        for page in entries {
            for value in [
                page.offset,
                section.max_size as u64,
                page.id,
                page.uncompressed,
                page.compressed,
                page.checksum,
                page.crc,
            ] {
                put(&mut map, value);
            }
        }
    }
    for value in [0, 0xf800, 0, 0, 0, 0, 4, 0] {
        put(&mut map, value);
    }
    map
}

/// 0x80 からの 0x400 バイトのファイルの頭のページ（5.2.1）。fields はファイルの頭のデータ（CRC は 0）
fn file_header_page(fields: &mut [u64; 34], random: &mut Random21) -> Vec<u8> {
    let data = words(&fields[..]);
    fields[33] = crc64_normal(update_seed2(0, data.len() as u64), &data);
    let data = words(&fields[..]);
    let (packed, is_compressed) = compressed(&data);
    let packed_crc = crc64_normal(update_seed2(0, packed.len() as u64), &packed);
    let first = random.next();
    let sequence = [first, rotate(first, first)];
    let sequence_crc = crc64_normal(update_seed1(0, 16), &words(&sequence));
    let mut block = words(&[sequence_crc, first, packed_crc]);
    let length = if is_compressed { packed.len() as i32 } else { -(packed.len() as i32) };
    block.extend_from_slice(&length.to_le_bytes());
    block.extend_from_slice(&0i32.to_le_bytes());
    block.extend_from_slice(&packed);
    let aligned = round_up(block.len(), 8);
    random.pad_to(&mut block, aligned);
    // 3 つの組に入るだけ繰り返し、残りを詰め物で埋める
    let mut buffer = Vec::with_capacity(3 * SYSTEM_K);
    while buffer.len() + block.len() <= 3 * SYSTEM_K {
        buffer.extend_from_slice(&block);
    }
    random.pad_to(&mut buffer, 3 * SYSTEM_K);
    let mut page = ReedSolomon::system().interleaved(&buffer, 3);
    random.pad_to(&mut page, 0x3d8);
    page.extend_from_slice(&words(&check_data(random)));
    page
}

/// R2007 の図面を書く
pub(crate) fn write_r2007(file: &R2007) -> Vec<u8> {
    let mut random = Random21::new(file.random_seed);
    // データのページ（ファイルの中の順）。番号は 3 から
    let mut stream: Vec<u8> = Vec::new();
    let mut page_sizes: Vec<(u64, u64)> = Vec::new();
    let mut entries: Vec<Vec<PageEntry>> = file.sections.iter().map(|_| Vec::new()).collect();
    let mut first_page: Vec<Option<usize>> = vec![None; file.sections.len()];
    // データのページはページの表と写しの後ろに置く。ページの表の大きさはページの数から決まる
    let page_count: usize = file
        .stream_order
        .iter()
        .filter_map(|&index| file.sections.get(index))
        .map(|section| section.data.len().div_ceil(section.max_size.max(1)))
        .sum();
    let page_map_size = system_page_size((page_count + 4) * 16);
    let data_start = 0x480 + 2 * page_map_size;
    let mut id = 3u64;
    for &index in &file.stream_order {
        let Some(section) = file.sections.get(index) else { continue };
        let page_size = section.max_size.max(1);
        let placed;
        let data = match section.preview_base {
            // プレビューは 1 つのページに収まるときだけ、置く位置に合わせて画像の位置を直す
            Some(base) if section.data.len() <= page_size => {
                placed = Preview { data: section.data.clone(), base }.placed((data_start + stream.len()) as u64);
                &placed
            }
            _ => &section.data,
        };
        for (number, chunk) in data.chunks(page_size).enumerate() {
            let address = data_start + stream.len();
            let (bytes, compressed, checksum, crc) = data_page(chunk, section, &random);
            first_page[index].get_or_insert(address);
            entries[index].push(PageEntry {
                offset: (number * page_size) as u64,
                id,
                uncompressed: chunk.len() as u64,
                compressed,
                checksum,
                crc,
            });
            page_sizes.push((bytes.len() as u64, id));
            stream.extend_from_slice(&bytes);
            id += 1;
        }
    }

    // 節の表とその写し、ページの表とその写し
    let sections_map = system_page(&section_map(&file.sections, &entries), &random);
    let (sections_id, sections2_id) = (id, id + 1);
    let (pages_id, pages2_id) = (id + 4, id + 5);
    let sections_size = sections_map.bytes.len() as u64;
    let mut map_entries: Vec<(u64, u64)> = vec![(page_map_size as u64, pages_id), (page_map_size as u64, pages2_id)];
    map_entries.extend_from_slice(&page_sizes);
    map_entries.push((sections_size, sections_id));
    map_entries.push((sections_size, sections2_id));
    let page_map_data: Vec<u8> = map_entries.iter().flat_map(|(size, id)| words(&[*size, *id])).collect();
    let mut pages_map = system_page(&page_map_data, &random);
    pages_map.bytes.resize(page_map_size, 0);

    let mut out = vec![0u8; 0x480];
    out[..0x80].copy_from_slice(&file.head);
    out.extend_from_slice(&pages_map.bytes);
    out.extend_from_slice(&pages_map.bytes);
    out.extend_from_slice(&stream);
    out.extend_from_slice(&sections_map.bytes);
    out.extend_from_slice(&sections_map.bytes);
    let header2_offset = out.len() as u64 - 0x480;
    let file_size = out.len() as u64 + 0x400;

    // ファイルの頭のデータ（5.2）
    let mut fields = [0u64; 34];
    fields[0] = 0x70;
    fields[1] = file_size;
    fields[2] = pages_map.crc_compressed;
    fields[3] = pages_map.factor;
    fields[4] = random.encode(0);
    fields[5] = page_map_size as u64;
    fields[6] = pages2_id;
    fields[7] = 0;
    fields[8] = pages_id;
    fields[9] = header2_offset;
    fields[10] = pages_map.size_compressed;
    fields[11] = pages_map.size_uncompressed;
    fields[12] = map_entries.len() as u64;
    fields[13] = pages2_id;
    fields[14] = 0x20;
    fields[15] = 0x40;
    fields[16] = pages_map.crc_uncompressed;
    fields[17] = 0xf800;
    fields[18] = 4;
    fields[19] = 1;
    fields[20] = file.sections.len() as u64 + 1;
    fields[21] = sections_map.crc_uncompressed;
    fields[22] = sections_map.size_compressed;
    fields[23] = sections2_id;
    fields[24] = sections_id;
    fields[25] = sections_map.size_uncompressed;
    fields[26] = sections_map.crc_compressed;
    fields[27] = sections_map.factor;
    fields[28] = random.encode(0);
    fields[29] = 0x60100;
    fields[30] = 0;
    fields[31] = random.encode(0);
    fields[32] = file.random_seed;
    let header = file_header_page(&mut fields, &mut random);
    out[0x80..0x480].copy_from_slice(&header);
    out.extend_from_slice(&header);

    // Meta Data の節の位置（ページの始まりのファイルの中の位置）
    let address = |name: &str| -> u32 {
        file.sections
            .iter()
            .position(|section| section.name == name)
            .and_then(|index| first_page[index])
            .map_or(0, |address| address as u32)
    };
    out[0x0d..0x11].copy_from_slice(&address("AcDb:Preview").to_le_bytes());
    out[0x20..0x24].copy_from_slice(&address("AcDb:SummaryInfo").to_le_bytes());
    out[0x24..0x28].copy_from_slice(&address("AcDb:VBAProject").to_le_bytes());
    out[0x2c..0x30].copy_from_slice(&address("AcDb:AppInfo").to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reed_solomon_codewords_are_divisible_by_the_generator() {
        // 符号語 c(x)（データの後に誤り訂正。初めのバイトが低い次数のデータ）は α^1…α^(255−k) で 0 になる
        for (k, poly) in [(SYSTEM_K, 0x169), (DATA_K, 0x11d)] {
            let rs = ReedSolomon::new(k, poly);
            let data: Vec<u8> = (0..k).map(|index| (index * 37 + 11) as u8).collect();
            let parity = rs.parity(&data);
            // c(x) = Σ parity[i] x^i + Σ data[i] x^(i + 255 − k)
            let word: Vec<u8> = parity.iter().chain(data.iter()).copied().collect();
            for power in 1..=255 - k {
                let root = rs.exp[power];
                let mut value = 0u8;
                for &coefficient in word.iter().rev() {
                    value = rs.mul(value, root) ^ coefficient;
                }
                assert_eq!(value, 0, "k {k} root α^{power}");
            }
        }
    }

    #[test]
    fn written_containers_read_back() {
        use crate::file::R2007Container;
        use zumen_core::NoticeList;
        // 圧縮しない節（符号化 1）、1 ページ、何ページにもなる節、0 だけの節、空の節
        let objects: Vec<u8> = (0..150_000u32).map(|index| ((index * 7) % 251) as u8 ^ (index / 1000) as u8).collect();
        let section = |name: &str, data: Vec<u8>, max_size: usize, hash: u64, encoding: u64| DataSection21 {
            name: name.into(),
            data,
            max_size,
            encryption: 0,
            hash,
            encoding,
            preview_base: None,
        };
        let sections = vec![
            section("AcDb:SummaryInfo", b"summary information".to_vec(), 0x80, 0x717a_060f, 1),
            section("AcDb:Preview", Preview::empty().data, 0x400, 0x40aa_0473, 1),
            section("AcDb:AcDbObjects", objects, 0xf800, 0x674c_05a9, 4),
            section("AcDb:Template", vec![0; 6], 0x400, 0x4a14_04ce, 4),
            section("AcDb:Empty", Vec::new(), 0xf800, 0, 4),
            section("AcDb:Header", (0..0x340).map(|index| index as u8).collect(), 0x800, 0x32b8_03d9, 4),
        ];
        let mut head = [0u8; 0x80];
        head[..6].copy_from_slice(b"AC1021");
        head[0x28] = 0x80;
        let expected: Vec<(String, Vec<u8>)> =
            sections.iter().map(|section| (section.name.clone(), section.data.clone())).collect();
        let bytes = write_r2007(&R2007 { head, sections, stream_order: vec![0, 1, 2, 3, 4, 5], random_seed: 0x5eed });
        crate::samples::check_r2007_container("written", &bytes);
        let container = R2007Container::open(&bytes).unwrap();
        let mut notices = NoticeList::new();
        let read: Vec<(String, Vec<u8>)> =
            container.sections.iter().map(|info| (info.name.clone(), container.data(info, &mut notices))).collect();
        assert_eq!(read, expected);
        assert!(notices.into_vec().is_empty());
        // Meta Data の節の位置はページの始まり
        let page = |name: &str| container.page(container.info(name).unwrap().pages[0].id).unwrap().0 as u32;
        let at = |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        assert_eq!(at(0x0d), page("AcDb:Preview"));
        assert_eq!(at(0x20), page("AcDb:SummaryInfo"));
        // 同じ図面からは同じバイト列
        assert_eq!(bytes.len() % 0x20, 0);
    }

    #[test]
    fn the_system_page_size_follows_the_specification() {
        // ページの表（0x130 バイト）は 0x400、節の表（0x854 バイト）は 0x1200（AutoCAD の図面と同じ）
        assert_eq!(system_page_size(0x130), 0x400);
        assert_eq!(system_page_size(0x854), 0x1200);
    }

    #[test]
    fn random_values_encode_ten_bits() {
        let mut random = Random21::new(0x0123_4567_89ab_cdef);
        for value in [0u32, 1, 0x155, 0x2aa, 0x3ff] {
            let encoded = random.encode(value);
            let (low, high) = (encoded as u32, (encoded >> 32) as u32);
            let mut decoded = 0;
            for (bit, mask) in [(0x200, 0x20), (0x100, 0x800), (0x80, 0x20000), (0x40, 0x80_0000), (0x20, 0x2000_0000)]
            {
                if low & mask != 0 {
                    decoded |= bit;
                }
            }
            for (bit, mask) in [(0x10, 0x08), (0x8, 0x200), (0x4, 0x8000), (0x2, 0x20_0000), (0x1, 0x800_0000)] {
                if high & mask != 0 {
                    decoded |= bit;
                }
            }
            assert_eq!(decoded, value);
        }
    }
}
