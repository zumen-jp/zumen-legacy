//! R2007 の圧縮（5.10 の伸長の逆）。前の同じ並びは R2004 の圧縮（compress.rs）と同じく 3 バイトの並びのハッシュの
//! 鎖で探し、いちばん長いものを使う。
//!
//! 圧縮したデータは、文字の並びと、伸長済みのデータから写す命令が交互に続く（5.10）。命令は 5.10.2 のどれも使う
//! （AutoCAD の図面の圧縮したページに、どの命令も現れる。実物の図面で数えた）:
//!
//! - 0x30〜0xFF: 3〜15 バイト（命令の上位 4 ビット）、512 バイトまで遡る（2 バイト）
//! - 0x10〜0x1F: 3〜18 バイト、8192 バイトまで（3 バイト）
//! - 0x00〜0x0F: 19〜50 バイト、4096 バイトまで（3 バイト）。写す命令の直後は 0xF0〜0xFF で書く（0x00〜0x0F は
//!   文字の始まりの印になる）
//! - 0x20〜0x27: 255 バイトまで、65535 バイトまで（4 バイト）。0x28〜0x2F: 256 バイト以上（5 バイト）
//!
//! 命令の最後のバイトの下位 3 ビットが、続く文字の数（1〜7）。8 文字以上は 0 にして、0x00〜0x0F の印（数 − 8、
//! 0x0F は続きのバイト）を置く。最初の文字が 8 文字より少ないときは、長さ 0 の 0x20 の命令（AutoCAD と同じ形）で
//! 数を書く。文字は 32 バイトの組と残りの長さごとの決まった順に並べ替えて書く（5.10.1 の逆）

use crate::compress::{BLOCK, tail_pieces};

/// ハッシュの表の大きさ（ビット数）
const HASH_BITS: u32 = 15;
/// 1 つの位置で比べる候補の数の上限
const MAX_CANDIDATES: usize = 48;
/// 写す長さの下限
const MIN_MATCH: usize = 3;
/// 遡れる数の上限（0x20〜0x27 の 16 ビット）
const MAX_OFFSET: usize = 0xffff;
/// 一度に写す長さの上限（0x28〜0x2F の長さ）
const MAX_MATCH: usize = 0x100 + 0xffff;

enum Token {
    /// 文字の並び（data の範囲）
    Literal(usize, usize),
    /// (長さ, 遡る数)
    Match(usize, usize),
}

/// data を 5.10 の形に圧縮する。ページごとに呼ぶ。空のデータは None
pub(crate) fn compress_r21(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    let mut tokens = Vec::new();
    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut previous = vec![u32::MAX; data.len()];
    let mut literal_start = 0usize;
    let mut at = 0usize;
    while at < data.len() {
        // 最初のバイトは文字（写すものがない）
        let (length, offset) = if at > 0 { longest_match(data, at, &head, &previous) } else { (0, 0) };
        if length >= MIN_MATCH {
            if literal_start < at {
                tokens.push(Token::Literal(literal_start, at));
            }
            tokens.push(Token::Match(length, offset));
            for position in at..at + length {
                insert(data, position, &mut head, &mut previous);
            }
            at += length;
            literal_start = at;
        } else {
            insert(data, at, &mut head, &mut previous);
            at += 1;
        }
    }
    if literal_start < data.len() {
        tokens.push(Token::Literal(literal_start, data.len()));
    }

    let mut out = Vec::with_capacity(data.len() / 2 + 16);
    // 直前の命令の、続く文字の数を書くバイトの位置（直前が文字なら None）
    let mut last: Option<usize> = None;
    for (index, token) in tokens.iter().enumerate() {
        match *token {
            Token::Literal(start, end) => {
                let count = end - start;
                match last.take() {
                    None if index == 0 && count < 8 => out.extend_from_slice(&[0x20, 0x00, 0x00, count as u8]),
                    Some(at) if count < 8 => out[at] |= count as u8,
                    _ => literal_length(&mut out, count),
                }
                scrambled(&data[start..end], &mut out);
            }
            Token::Match(length, offset) => last = Some(emit_match(&mut out, length, offset, last.is_some())),
        }
    }
    Some(out)
}

fn hash(data: &[u8], at: usize) -> Option<usize> {
    let bytes = data.get(at..at + 3)?;
    let value = u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16);
    Some((value.wrapping_mul(0x9e37_79b1) >> (32 - HASH_BITS)) as usize)
}

fn insert(data: &[u8], at: usize, head: &mut [u32], previous: &mut [u32]) {
    if let Some(slot) = hash(data, at) {
        previous[at] = head[slot];
        head[slot] = at as u32;
    }
}

/// at から始まる、前にある同じ並びのいちばん長いもの（長さ, 遡る数）
fn longest_match(data: &[u8], at: usize, head: &[u32], previous: &[u32]) -> (usize, usize) {
    let Some(slot) = hash(data, at) else { return (0, 0) };
    let limit = (data.len() - at).min(MAX_MATCH);
    let mut best = (0usize, 0usize);
    let mut candidate = head[slot];
    let mut tried = 0;
    while candidate != u32::MAX && tried < MAX_CANDIDATES {
        let from = candidate as usize;
        let offset = at - from;
        if offset > MAX_OFFSET {
            break;
        }
        if data.get(from + best.0) == data.get(at + best.0) {
            let length = data[from..].iter().zip(&data[at..at + limit]).take_while(|(a, b)| a == b).count();
            if length > best.0 && worth(length, offset) {
                best = (length, offset);
                if length == limit {
                    break;
                }
            }
        }
        candidate = previous[from];
        tried += 1;
    }
    best
}

/// 命令で書くと、文字で書くより短くなるか（命令の直後に続く命令の決まりでも書ける長さで考える）
fn worth(length: usize, offset: usize) -> bool {
    match offset {
        1..=0x200 if length <= 14 => length >= 3,
        1..=0x2000 if length <= 18 => length >= 4,
        1..=0x1000 if (19..=50).contains(&length) => true,
        _ => offset <= MAX_OFFSET && length >= 5,
    }
}

/// 8 文字以上の文字の数の印（0x00〜0x0E は数 − 8、0x0F は続きのバイト）
fn literal_length(out: &mut Vec<u8>, count: usize) {
    if count - 8 < 0x0f {
        out.push((count - 8) as u8);
        return;
    }
    out.push(0x0f);
    let rest = count - 0x17;
    if rest < 0xff {
        out.push(rest as u8);
        return;
    }
    out.push(0xff);
    let mut rest = rest - 0xff;
    loop {
        let chunk = rest.min(0xffff);
        out.extend_from_slice(&(chunk as u16).to_le_bytes());
        rest -= chunk;
        if chunk != 0xffff {
            break;
        }
    }
}

/// 文字を伸長が並べ替える順の逆に並べて書く（5.10.1）
fn scrambled(literal: &[u8], out: &mut Vec<u8>) {
    let unplace = |target: &[u8], source: &mut [u8], pieces: &[(usize, usize, usize)]| {
        for &(from, to, length) in pieces {
            for index in 0..length {
                let at = if length == 2 || length == 3 { from + length - 1 - index } else { from + index };
                source[at] = target[to + index];
            }
        }
    };
    let (chunks, rest) = literal.as_chunks::<32>();
    let mut source = [0u8; 32];
    for chunk in chunks {
        unplace(chunk, &mut source, &BLOCK);
        out.extend_from_slice(&source);
    }
    if !rest.is_empty() {
        unplace(rest, &mut source, tail_pieces(rest.len()));
        out.extend_from_slice(&source[..rest.len()]);
    }
}

/// 写す命令を書き、続く文字の数を書き込むバイトの位置を返す。after_match は直前が写す命令（文字がない）か
fn emit_match(out: &mut Vec<u8>, length: usize, offset: usize, after_match: bool) -> usize {
    let back = offset - 1;
    let short_limit = if after_match { 14 } else { 15 };
    if length <= short_limit && offset <= 0x200 {
        out.push(((length << 4) | (back & 0x0f)) as u8);
        out.push(((back >> 4) << 3) as u8);
    } else if length <= 18 && offset <= 0x2000 {
        out.push(0x10 | (length - 3) as u8);
        out.push(back as u8);
        out.push(((back >> 8) << 3) as u8);
    } else if (19..=50).contains(&length) && offset <= 0x1000 {
        let extra = length - 0x13;
        out.push(if after_match { 0xf0 } else { 0x00 } | (extra & 0x0f) as u8);
        out.push(back as u8);
        out.push((((extra >> 4) << 7) | (((back >> 8) & 0x0f) << 3)) as u8);
    } else if length <= 0xff {
        out.push(0x20 | (length & 7) as u8);
        out.extend_from_slice(&(offset as u16).to_le_bytes());
        out.push((length & 0xf8) as u8);
    } else {
        let value = length - 0x100;
        out.push(0x28 | (value & 7) as u8);
        out.extend_from_slice(&(back as u16).to_le_bytes());
        out.push((value >> 3) as u8);
        out.push((((value >> 11) & 0x1f) << 3) as u8);
    }
    out.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::decompress_r21;

    fn round_trip(data: &[u8]) -> usize {
        let compressed = compress_r21(data).unwrap();
        assert_eq!(decompress_r21(&compressed, 0, compressed.len(), data.len()), data, "{} bytes", data.len());
        compressed.len()
    }

    #[test]
    fn compressed_data_decompresses_to_the_same_bytes() {
        // 文字だけ、短い最初の文字、長い文字、繰り返し、遠い繰り返し、長い繰り返し
        let mut seed = 0x1234_5678u32;
        let mut random = |bound: u32| {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (seed >> 8) % bound
        };
        for length in (1..80).chain([255, 256, 257, 300, 1000, 5000]) {
            let data: Vec<u8> = (0..length).map(|_| random(256) as u8).collect();
            round_trip(&data);
        }
        let text: Vec<u8> = b"AcDbEntity AcDbLine 0.0 1.0 2.0 ".iter().cycle().take(10_000).copied().collect();
        assert!(round_trip(&text) < 400);
        assert!(round_trip(&vec![0u8; 0xf800]) < 300);
        // 遡る数と長さの境目（512、4096、8192、65535）
        for gap in [3usize, 15, 16, 18, 19, 50, 51, 255, 256, 511, 512, 513, 4095, 4096, 4097, 8191, 8192, 8193, 60_000] {
            let mut data: Vec<u8> = (0..gap).map(|_| random(256) as u8).collect();
            let copy: Vec<u8> = data[..gap.min(400)].to_vec();
            data.extend_from_slice(&copy);
            data.extend((0..20).map(|_| random(256) as u8));
            data.extend_from_slice(&copy);
            round_trip(&data);
        }
        // 写す命令が続く（文字のない）並び: 長さ 15 の繰り返しが続く
        let mut data: Vec<u8> = (0..15).map(|value| value as u8 * 7).collect();
        for _ in 0..40 {
            data.extend_from_slice(&data[..15].to_vec());
            data.push(random(256) as u8);
            data.extend_from_slice(&data[..15].to_vec());
        }
        round_trip(&data);
        // 長い文字の続きのバイト（0xFF と 16 ビットの値）
        for length in [0x17 + 0xfe, 0x17 + 0xff, 0x17 + 0xff + 0xffff, 0x17 + 0xff + 0xffff + 3] {
            let data: Vec<u8> = (0..length).map(|_| random(256) as u8).collect();
            round_trip(&data);
        }
    }
}
