//! 区切り（sentinel）で囲む節（9「AcDb:Header」、10「AcDb:Classes」）と、オブジェクトの表（23「AcDb:Handles」）

use crate::crc::{CRC8_SEED, crc8};

/// ヘッダ変数の節の始まりと終わりの区切り（9）
pub(crate) const HEADER_SENTINEL: [u8; 16] =
    [0xcf, 0x7b, 0x1f, 0x23, 0xfd, 0xde, 0x38, 0xa9, 0x5f, 0x7c, 0x68, 0xb8, 0x4e, 0x6d, 0x33, 0x5f];
pub(crate) const HEADER_END: [u8; 16] =
    [0x30, 0x84, 0xe0, 0xdc, 0x02, 0x21, 0xc7, 0x56, 0xa0, 0x83, 0x97, 0x47, 0xb1, 0x92, 0xcc, 0xa0];
/// クラスの節の始まりと終わりの区切り（10）
pub(crate) const CLASSES_SENTINEL: [u8; 16] =
    [0x8d, 0xa1, 0xc4, 0xb8, 0xc4, 0xa9, 0xf8, 0xc5, 0xc0, 0xdc, 0xf4, 0x5f, 0xe7, 0xcf, 0xb6, 0x8a];
pub(crate) const CLASSES_END: [u8; 16] =
    [0x72, 0x5e, 0x3b, 0x47, 0x3b, 0x56, 0x07, 0x3a, 0x3f, 0x23, 0x0b, 0xa0, 0x18, 0x30, 0x49, 0x75];

/// 区切りで囲む節の中身。大きさの後の不明の RL（R2010・R2013 で保守版が 3 より大きいとき、R2018 以降）を含めて持つ
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Framed {
    /// 大きさの後の 4 バイト（R2010 以降の一部の版だけ。値は 0 のことしか見ていない）
    pub extra: Option<[u8; 4]>,
    /// 値（大きさが数えるバイト）
    pub data: Vec<u8>,
    /// 読んだ節に書いてあった CRC が中身と合っていたか（書き出した節では true）
    pub crc_ok: bool,
}

impl Framed {
    /// 区切り・大きさ（RL）・（不明の RL）・値・CRC・区切りを読む。extra は大きさの後に不明の RL があるか。
    /// 区切りが違う・大きさが節を超えるときは None
    pub fn parse(section: &[u8], start: &[u8; 16], end: &[u8; 16], extra: bool) -> Option<Self> {
        if !section.starts_with(start) {
            return None;
        }
        let size = u32::from_le_bytes(section.get(16..20)?.try_into().ok()?) as usize;
        let mut at = 20;
        let extra = if extra {
            let bytes: [u8; 4] = section.get(at..at + 4)?.try_into().ok()?;
            at += 4;
            Some(bytes)
        } else {
            None
        };
        let data = section.get(at..at.checked_add(size)?)?.to_vec();
        let crc_at = at + size;
        let stored = u16::from_le_bytes(section.get(crc_at..crc_at + 2)?.try_into().ok()?);
        if section.get(crc_at + 2..crc_at + 18)? != end {
            return None;
        }
        let crc_ok = crc8(CRC8_SEED, &section[16..crc_at]) == stored;
        Some(Self { extra, data, crc_ok })
    }

    /// 区切り・大きさ・（不明の RL）・値・CRC（大きさから値の終わりまで、初めの値 0xC0C1）・区切り。
    /// trailing は終わりの区切りの後に足す 0 のバイトの数（R2004 以降のクラスの節は 8、10.1）
    pub fn bytes(&self, start: &[u8; 16], end: &[u8; 16], trailing: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.data.len() + 60);
        out.extend_from_slice(start);
        out.extend_from_slice(&(self.data.len() as u32).to_le_bytes());
        if let Some(extra) = self.extra {
            out.extend_from_slice(&extra);
        }
        out.extend_from_slice(&self.data);
        let crc = crc8(CRC8_SEED, &out[16..]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(end);
        out.resize(out.len() + trailing, 0);
        out
    }
}

/// オブジェクトの表の 1 つの区切りの値の大きさの上限（23.1 の 2032）
const MAP_CHUNK: usize = 2032;

/// オブジェクトの表（23）。locations はハンドルの小さい順の（ハンドル, 位置）。位置は R2000 はファイルの中、R2004
/// 以降は AcDb:AcDbObjects の中。区切りごとに、大きさ（上位のバイトが先、大きさ自身の 2 バイトを含む）・ハンドルと
/// 位置の差（MC）・CRC（上位のバイトが先、大きさから）。差は区切りごとに 0 から数える。最後は大きさ 2 だけの区切り
pub(crate) fn object_map(locations: &[(u64, u64)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(locations.len() * 4 + 16);
    let mut chunk: Vec<u8> = Vec::with_capacity(MAP_CHUNK);
    let (mut last_handle, mut last_location) = (0u64, 0i64);
    let mut item = Vec::with_capacity(16);
    for &(handle, location) in locations {
        let location = location as i64;
        encode_item(&mut item, handle.wrapping_sub(last_handle), location.wrapping_sub(last_location));
        if chunk.len() + item.len() > MAP_CHUNK {
            finish_chunk(&mut out, &chunk);
            chunk.clear();
            encode_item(&mut item, handle, location);
        }
        chunk.extend_from_slice(&item);
        last_handle = handle;
        last_location = location;
    }
    if !chunk.is_empty() {
        finish_chunk(&mut out, &chunk);
    }
    finish_chunk(&mut out, &[]);
    out
}

fn encode_item(out: &mut Vec<u8>, handle: u64, location: i64) {
    out.clear();
    // ハンドルの差は符号のない MC
    let mut rest = handle;
    loop {
        let byte = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
    // 位置の差は符号のある MC（最後のバイトの 0x40 が負の印）
    let negative = location < 0;
    let mut rest = location.unsigned_abs();
    loop {
        if rest < 0x40 {
            out.push(rest as u8 | if negative { 0x40 } else { 0 });
            break;
        }
        out.push((rest & 0x7f) as u8 | 0x80);
        rest >>= 7;
    }
}

fn finish_chunk(out: &mut Vec<u8>, data: &[u8]) {
    let start = out.len();
    out.extend_from_slice(&((data.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(data);
    let crc = crc8(CRC8_SEED, &out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::read_object_map;

    #[test]
    fn the_object_map_reads_back() {
        let locations: Vec<(u64, u64)> = (1..3000u64).map(|handle| (handle * 3, 4 + handle * 37 % 1000)).collect();
        let map = object_map(&locations);
        let read = read_object_map(&map);
        assert_eq!(read.len(), locations.len());
        for (handle, location) in &locations {
            assert_eq!(read.get(handle), Some(&(*location as usize)));
        }
        // 最後の区切りは大きさ 2 と CRC 0x01D0
        assert_eq!(map[map.len() - 4..], [0x00, 0x02, 0x01, 0xd0]);
    }

    #[test]
    fn a_framed_section_reads_back_with_its_crc() {
        let framed = Framed { extra: Some([0; 4]), data: vec![1, 2, 3, 4, 5], crc_ok: true };
        let bytes = framed.bytes(&HEADER_SENTINEL, &HEADER_END, 0);
        assert_eq!(Framed::parse(&bytes, &HEADER_SENTINEL, &HEADER_END, true), Some(framed));
        let mut broken = bytes.clone();
        broken[25] ^= 1;
        assert!(!Framed::parse(&broken, &HEADER_SENTINEL, &HEADER_END, true).unwrap().crc_ok);
    }
}
