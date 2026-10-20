//! R2004 以降の圧縮（4.7）。LZ77 の変種で、伸長は compress.rs の decompress_r18。前の同じ並びを探すのは
//! 3 バイトの並びのハッシュの鎖で、見つけた中でいちばん長いものを使う（仕様は ODA もハッシュで探すと書く）。
//! 文字だけの出力も正しい形だが、AutoCAD の図面と同じくらいの大きさになるよう縮める。
//!
//! 命令は AutoCAD が書く図面に現れるものだけを使う（実物の図面の圧縮したページで数えた）。仕様と ACadSharp の
//! 読み方が分かれる命令は使わない: 最初の 0x12 以上のバイトで始まる文字（LZO の形。仕様にない）と、0x18〜0x1F
//! （仕様は長さ、ACadSharp は遡る数の 0x4000 の位と読む）。そのため最初の文字は 4 バイト以上にし、0x4000 を超えて
//! 遡るのは 0x10 と 0x12〜0x17 だけで 0x7FFF まで。終わりは 0x11 と 2 バイトの 0（AutoCAD が書く形）

/// ハッシュの表の大きさ（ビット数）
const HASH_BITS: u32 = 15;
/// 1 つの位置で比べる候補の数の上限
const MAX_CANDIDATES: usize = 48;
/// 写す長さの下限。2 バイトで書ける 0x40〜0xFF の命令は 3 バイトから
const MIN_MATCH: usize = 3;
/// 遡れる位置の上限（0x10・0x12〜0x17 の命令の 0x7FFF。ページは 0x7400 まで）
const MAX_OFFSET: usize = 0x7fff;
/// 最初の文字の数の下限（Literal Length で書ける 4。1〜3 は仕様の形で書けない）
const FIRST_LITERALS: usize = 4;
/// 一度に写す長さの上限（長さは何バイトでも書けるが、比べる手間を抑える）
const MAX_MATCH: usize = 0x1_0000;

/// data を 4.7 の形に圧縮する。ページごとに呼ぶ（前のページは参照しない）
pub(crate) fn compress_r18(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 16);
    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut previous = vec![u32::MAX; data.len()];
    // 前の命令の後ろの文字の始まり
    let mut literal_start = 0usize;
    // 前の命令の位置（下位 2 ビットに続く文字の数を書き込む）。None は最初の文字の前
    let mut last_opcode: Option<usize> = None;
    let mut at = 0usize;
    while at < data.len() {
        let (length, offset) = if last_opcode.is_some() || at >= FIRST_LITERALS {
            longest_match(data, at, &head, &previous)
        } else {
            (0, 0)
        };
        if length >= MIN_MATCH && worth(length, offset) {
            emit_literals(&mut out, &data[literal_start..at], last_opcode);
            last_opcode = Some(emit_match(&mut out, length, offset));
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
    emit_literals(&mut out, &data[literal_start..], last_opcode);
    out.extend_from_slice(&[0x11, 0x00, 0x00]);
    out
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
        // いま見つけているものより長くなりうる候補だけを比べる
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

/// その長さと遡る数を命令で書けて、文字で書くより短くなるか
fn worth(length: usize, offset: usize) -> bool {
    match offset {
        // 0x40〜0xFF の 2 バイトの命令（3〜14 バイト、1024 まで）
        1..=0x400 if length <= 14 => length >= 3,
        // 0x20〜0x3F の 3 バイト以上の命令（0x4000 まで）
        1..=0x4000 => length >= 4,
        // 0x10・0x12〜0x17。長さ 3 は 0x11（終わりの印）と重なるので 4 から
        _ => offset <= MAX_OFFSET && length >= 4,
    }
}

/// 長さの続きのバイト（Literal Length・Long Compression Offset）。0 のバイトごとに 0xFF、最後は 0 でない値
fn extension(out: &mut Vec<u8>, mut rest: usize) {
    while rest > 0xff {
        out.push(0);
        rest -= 0xff;
    }
    out.push(rest as u8);
}

/// 文字の並び。前の命令があれば、3 文字までは命令の下位 2 ビットに数を書く
fn emit_literals(out: &mut Vec<u8>, literals: &[u8], last_opcode: Option<usize>) {
    let count = literals.len();
    if count == 0 {
        return;
    }
    match last_opcode {
        Some(opcode) if count <= 3 => out[opcode] |= count as u8,
        // 3 バイト以下のデータだけは、0x11 に数を足した最初のバイト（LZO の形。ページは 4 バイトより大きいので使わない）
        None if count <= 3 => out.push(0x11 + count as u8),
        _ => {
            // Literal Length: 0x01〜0x0F は 4〜18 文字、0x00 は 0x0F と続きのバイト（どちらも 3 を足した数）
            let value = count - 3;
            if value <= 0x0f {
                out.push(value as u8);
            } else {
                out.push(0);
                extension(out, value - 0x0f);
            }
        }
    }
    out.extend_from_slice(literals);
}

/// 写す命令を書き、続く文字の数を書き込むバイトの位置を返す
fn emit_match(out: &mut Vec<u8>, length: usize, offset: usize) -> usize {
    if offset <= 0x400 && length <= 14 {
        // 0x40〜0xFF: 長さ + 1 を上位 4 ビット、遡る数 − 1 の下位 2 ビットを 0x0C、残りを次のバイト
        let back = offset - 1;
        let at = out.len();
        out.push((((length + 1) << 4) | ((back & 3) << 2)) as u8);
        out.push((back >> 2) as u8);
        return at;
    }
    let (opcode, mask, two) = if offset <= 0x4000 {
        // 0x20〜0x3F: Two Byte Offset は遡る数 − 1
        (0x20u8, 0x1fusize, offset - 1)
    } else {
        // 0x10・0x12〜0x17: Two Byte Offset は遡る数 − 0x4000
        (0x10u8, 0x07usize, offset - 0x4000)
    };
    let count = length - 2;
    if count <= mask {
        out.push(opcode | count as u8);
    } else {
        out.push(opcode);
        extension(out, count - mask);
    }
    // Two Byte Offset: 最初のバイトの上位 6 ビットと次のバイト。最初のバイトの下位 2 ビットが続く文字の数
    let at = out.len();
    out.push(((two & 0x3f) << 2) as u8);
    out.push((two >> 6) as u8);
    at
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compress::decompress_r18;

    fn round_trip(data: &[u8]) -> Vec<u8> {
        let compressed = compress_r18(data);
        assert_eq!(decompress_r18(&compressed, data.len() + 64), data, "{} bytes", data.len());
        if data.len() >= FIRST_LITERALS {
            assert_eq!(by_the_specification(&compressed), data, "{} bytes", data.len());
        }
        compressed
    }

    /// 4.7 を文字どおりに読む伸長（ezdwg と同じ読み方。最初のバイトは Literal Length か命令、0x12〜0x1F の長さは
    /// 下位 4 ビット + 2、遡る数は Two Byte Offset + 0x3FFF + 1）
    fn by_the_specification(source: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut at = 0;
        let mut byte = || {
            at += 1;
            source[at - 1]
        };
        let mut literal_length = |byte: &mut dyn FnMut() -> u8| -> (usize, u8) {
            let first = byte();
            match first {
                0x01..=0x0f => (usize::from(first) + 3, 0),
                0x00 => {
                    let mut length = 0x0f;
                    let mut next = byte();
                    while next == 0 {
                        length += 0xff;
                        next = byte();
                    }
                    (length + usize::from(next) + 3, 0)
                }
                _ => (0, first),
            }
        };
        let long = |byte: &mut dyn FnMut() -> u8| -> usize {
            let first = byte();
            if first != 0 {
                return usize::from(first);
            }
            let mut value = 0xff;
            let mut next = byte();
            while next == 0 {
                value += 0xff;
                next = byte();
            }
            value + usize::from(next)
        };
        let (count, mut opcode) = literal_length(&mut byte);
        let mut literals: Vec<u8> = (0..count).map(|_| byte()).collect();
        out.append(&mut literals);
        loop {
            if opcode == 0 {
                opcode = byte();
            }
            let two = |byte: &mut dyn FnMut() -> u8| -> (usize, usize) {
                let first = byte();
                let second = byte();
                ((usize::from(first) >> 2) | (usize::from(second) << 6), usize::from(first & 3))
            };
            let (count, offset, literal) = match opcode {
                0x11 => break,
                0x10 => {
                    let count = long(&mut byte) + 9;
                    let (offset, literal) = two(&mut byte);
                    (count, offset + 0x3fff, literal)
                }
                0x12..=0x1f => {
                    let (offset, literal) = two(&mut byte);
                    (usize::from(opcode & 0x0f) + 2, offset + 0x3fff, literal)
                }
                0x20 => {
                    let count = long(&mut byte) + 0x21;
                    let (offset, literal) = two(&mut byte);
                    (count, offset, literal)
                }
                0x21..=0x3f => {
                    let (offset, literal) = two(&mut byte);
                    (usize::from(opcode) - 0x1e, offset, literal)
                }
                0x40..=0xff => {
                    let next = usize::from(byte());
                    ((usize::from(opcode) >> 4) - 1, (next << 2) | ((usize::from(opcode) & 0x0c) >> 2), usize::from(opcode & 3))
                }
                _ => panic!("opcode {opcode:#x}"),
            };
            let start = out.len() - (offset + 1);
            for index in 0..count {
                out.push(out[start + index]);
            }
            let (count, next) = if literal == 0 { literal_length(&mut byte) } else { (literal, 0) };
            let mut literals: Vec<u8> = (0..count).map(|_| byte()).collect();
            out.append(&mut literals);
            opcode = next;
        }
        out
    }

    #[test]
    fn compressed_data_decompresses_to_the_same_bytes() {
        round_trip(b"");
        round_trip(b"a");
        round_trip(b"ab");
        round_trip(b"abc");
        round_trip(b"abcd");
        round_trip(b"abcabcabcabcabcabcabc");
        round_trip(&[0u8; 0x7400]);
        // 繰り返しのない並び（擬似乱数）と、長い距離の繰り返し
        let mut seed = 0x1234_5678u32;
        let random: Vec<u8> = (0..0x7400)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (seed >> 16) as u8
            })
            .collect();
        round_trip(&random);
        let mut far = random[..0x5000].to_vec();
        far.extend_from_slice(&random[..0x2400]);
        round_trip(&far);
        let mut far = random[..0x1000].to_vec();
        far.extend_from_slice(&[7u8; 0x6000]);
        far.extend_from_slice(&random[..0x400]);
        round_trip(&far);
        // 長さ 3〜20 の文字と繰り返しを混ぜた並び
        let mut mixed = Vec::new();
        for index in 0..2000usize {
            mixed.extend_from_slice(&random[index % 97..index % 97 + index % 17 + 1]);
            mixed.extend_from_slice(b"LINE");
        }
        round_trip(&mixed);
    }

    #[test]
    fn repeated_data_is_compressed() {
        let text = b"AcDbEntity AcDbLine AcDbEntity AcDbLine ".repeat(400);
        assert!(round_trip(&text).len() < text.len() / 10);
        assert!(round_trip(&[0u8; 0x7400]).len() < 0x100);
        // 文字だけでも、元の大きさと続きのバイトと終わりの印を合わせた大きさを超えない
        let mut seed = 99u32;
        let random: Vec<u8> = (0..1000)
            .map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                (seed >> 16) as u8
            })
            .collect();
        assert!(round_trip(&random).len() <= random.len() + 10);
    }

    #[test]
    fn the_stream_ends_like_autocad_writes_it() {
        // 最初の文字は Literal Length（4〜18 は 0x01〜0x0F）、終わりは 0x11 0x00 0x00
        assert_eq!(compress_r18(b"abcd"), [0x01, b'a', b'b', b'c', b'd', 0x11, 0x00, 0x00]);
        assert_eq!(compress_r18(b"ab"), [0x13, b'a', b'b', 0x11, 0x00, 0x00]);
        assert_eq!(compress_r18(b""), [0x11, 0x00, 0x00]);
        // 0 で始まるページも、最初の 4 バイトは文字にする（1 バイトの文字と 1 つ前を写す命令にはしない）。
        // 残りの 60 バイトは 1 つ前を写す 0x20 の命令（長さ 60 − 0x21 = 27 の続きのバイト）
        assert_eq!(compress_r18(&[0; 64])[..7], [0x01, 0, 0, 0, 0, 0x20, 27]);
    }
}
