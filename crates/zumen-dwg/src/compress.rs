//! R2004 以降（4.7）と R2007（5.10）の LZ77 の変種の伸長、R2007 の Reed-Solomon の並べ替えの解き方（5.13）。
//! 仕様の説明はオフセットの +1 や 0x12〜0x1F の上位ビットを省いているため、手順は ACadSharp（MIT）の
//! DwgLZ77AC18Decompressor・DwgLZ77AC21Decompressor に合わせ、実際の図面で確かめた（docs/formats/dwg.md）。
//! どちらも元のデータの外を読まず、伸長の後の大きさ（capacity）を超えて書かない

use crate::bits::{Eof, Read};

struct Source<'a> {
    data: &'a [u8],
    at: usize,
}

impl Source<'_> {
    fn byte(&mut self) -> Read<u8> {
        let value = *self.data.get(self.at).ok_or(Eof)?;
        self.at += 1;
        Ok(value)
    }
}

/// 伸長の先。capacity まで書いたら止める（節の最後のページは、ページ全体を伸長した先頭の部分だけを使う）
struct Sink {
    out: Vec<u8>,
    capacity: usize,
}

impl Sink {
    /// capacity までに書ける数。count より少なければ、書いた後で止める
    fn room(&self, count: usize) -> usize {
        count.min(self.capacity.saturating_sub(self.out.len()))
    }

    fn literal(&mut self, source: &mut Source<'_>, count: usize) -> Read<()> {
        let end = source.at.checked_add(count).ok_or(Eof)?;
        let room = self.room(count);
        let bytes = source.data.get(source.at..source.at + room).ok_or(Eof)?;
        self.out.extend_from_slice(bytes);
        if room < count {
            return Err(Eof);
        }
        source.at = end;
        Ok(())
    }

    /// offset バイト前から count バイトを写す（count が offset より長ければ繰り返す）
    fn back(&mut self, offset: usize, count: usize) -> Read<()> {
        if offset == 0 || offset > self.out.len() {
            return Err(Eof);
        }
        let start = self.out.len() - offset;
        let room = self.room(count);
        for index in 0..room {
            let byte = self.out[start + index];
            self.out.push(byte);
        }
        if room < count { Err(Eof) } else { Ok(()) }
    }
}

/// R2004 以降の圧縮（4.7）。capacity は伸長の後の大きさの上限。途中で壊れていたら、それまでに伸長した分を返す
pub(crate) fn decompress_r18(data: &[u8], capacity: usize) -> Vec<u8> {
    let mut sink = Sink { out: Vec::with_capacity(capacity.min(1 << 20)), capacity };
    let _ = r18(data, &mut sink);
    sink.out
}

fn r18(data: &[u8], sink: &mut Sink) -> Read<()> {
    let mut source = Source { data, at: 0 };
    let mut opcode = source.byte()?;
    if opcode > 0x11 {
        sink.literal(&mut source, usize::from(opcode - 0x11))?;
        opcode = source.byte()?;
    }
    if opcode & 0xf0 == 0 {
        let count = literal_length(&mut source, opcode)? + 3;
        sink.literal(&mut source, count)?;
        opcode = source.byte()?;
    }
    // 0x11 で終わる
    while opcode != 0x11 {
        let (count, offset);
        if opcode >= 0x40 {
            // 0x40〜0xFF: 写す長さは上位 4 ビット − 1、オフセットは次のバイトと 0x0C のビット
            count = usize::from(opcode >> 4) - 1;
            let next = source.byte()?;
            offset = (usize::from((opcode >> 2) & 3) | (usize::from(next) << 2)) + 1;
        } else if opcode < 0x10 {
            // 0x00〜0x0F は文字の数と取り違えるので使われない
            return Err(Eof);
        } else if opcode < 0x20 {
            // 0x10〜0x1F: 0x08 のビットはオフセットの 0x4000 の位
            count = long_count(&mut source, opcode, 0x07)?;
            let (two, first) = two_byte_offset(&mut source)?;
            offset = (usize::from(opcode & 8) << 11) + two + 0x4000;
            opcode = first;
        } else {
            // 0x20〜0x3F
            count = long_count(&mut source, opcode, 0x1f)?;
            let (two, first) = two_byte_offset(&mut source)?;
            offset = two + 1;
            opcode = first;
        }
        sink.back(offset, count)?;
        // 続く文字の数は opcode の下位 2 ビット。0 なら次のバイトが文字の数か次の opcode
        let mut literals = usize::from(opcode & 3);
        if literals == 0 {
            opcode = source.byte()?;
            if opcode & 0xf0 == 0 {
                literals = literal_length(&mut source, opcode)? + 3;
            }
        }
        if literals > 0 {
            sink.literal(&mut source, literals)?;
            opcode = source.byte()?;
        }
    }
    Ok(())
}

/// Literal Length（4.7）。0 なら 0x0F に、続く 0x00 ごとに 0xFF と最後の 0 でないバイトを足す
fn literal_length(source: &mut Source<'_>, opcode: u8) -> Read<usize> {
    let mut count = usize::from(opcode & 0x0f);
    if count == 0 {
        let mut byte = source.byte()?;
        while byte == 0 {
            count += 0xff;
            byte = source.byte()?;
        }
        count += 0x0f + usize::from(byte);
    }
    Ok(count)
}

/// 写す長さ。opcode の mask のビットが 0 なら Long Compression Offset を読む
fn long_count(source: &mut Source<'_>, opcode: u8, mask: u8) -> Read<usize> {
    let mut count = usize::from(opcode & mask);
    if count == 0 {
        let mut byte = source.byte()?;
        while byte == 0 {
            count += 0xff;
            byte = source.byte()?;
        }
        count += usize::from(byte) + usize::from(mask);
    }
    Ok(count + 2)
}

/// Two Byte Offset。オフセットと、下位 2 ビットが続く文字の数になる最初のバイト
fn two_byte_offset(source: &mut Source<'_>) -> Read<(usize, u8)> {
    let first = source.byte()?;
    let second = source.byte()?;
    Ok((usize::from(first >> 2) | (usize::from(second) << 6), first))
}

/// R2007 の文字の写し方（5.10.1）。32 バイトごとに 4 バイトの組の順を入れ替え、残りは長さごとの組み合わせで写す。
/// (元の位置, 先の位置, 長さ)。長さ 2・3 の組はバイトの順を逆にする
pub(crate) const BLOCK: [(usize, usize, usize); 8] =
    [(24, 0, 4), (28, 4, 4), (16, 8, 4), (20, 12, 4), (8, 16, 4), (12, 20, 4), (0, 24, 4), (4, 28, 4)];

pub(crate) fn tail_pieces(n: usize) -> &'static [(usize, usize, usize)] {
    match n {
        1 => &[(0, 0, 1)],
        2 => &[(0, 0, 2)],
        3 => &[(0, 0, 3)],
        4 => &[(0, 0, 4)],
        5 => &[(4, 0, 1), (0, 1, 4)],
        6 => &[(5, 0, 1), (1, 1, 4), (0, 5, 1)],
        7 => &[(5, 0, 2), (1, 2, 4), (0, 6, 1)],
        8 => &[(0, 0, 8)],
        9 => &[(8, 0, 1), (0, 1, 8)],
        10 => &[(9, 0, 1), (1, 1, 8), (0, 9, 1)],
        11 => &[(9, 0, 2), (1, 2, 8), (0, 10, 1)],
        12 => &[(8, 0, 4), (0, 4, 8)],
        13 => &[(12, 0, 1), (8, 1, 4), (0, 5, 8)],
        14 => &[(13, 0, 1), (9, 1, 4), (1, 5, 8), (0, 13, 1)],
        15 => &[(13, 0, 2), (9, 2, 4), (1, 6, 8), (0, 14, 1)],
        16 => &[(8, 0, 8), (0, 8, 8)],
        17 => &[(9, 0, 8), (8, 8, 1), (0, 9, 8)],
        18 => &[(17, 0, 1), (9, 1, 8), (1, 9, 8), (0, 17, 1)],
        19 => &[(16, 0, 3), (8, 3, 8), (0, 11, 8)],
        20 => &[(16, 0, 4), (8, 4, 8), (0, 12, 8)],
        21 => &[(20, 0, 1), (16, 1, 4), (8, 5, 8), (0, 13, 8)],
        22 => &[(20, 0, 2), (16, 2, 4), (8, 6, 8), (0, 14, 8)],
        23 => &[(20, 0, 3), (16, 3, 4), (8, 7, 8), (0, 15, 8)],
        24 => &[(16, 0, 8), (8, 8, 8), (0, 16, 8)],
        25 => &[(17, 0, 8), (16, 8, 1), (8, 9, 8), (0, 17, 8)],
        26 => &[(25, 0, 1), (17, 1, 8), (16, 9, 1), (8, 10, 8), (0, 18, 8)],
        27 => &[(25, 0, 2), (17, 2, 8), (16, 10, 1), (8, 11, 8), (0, 19, 8)],
        28 => &[(24, 0, 4), (16, 4, 8), (8, 12, 8), (0, 20, 8)],
        29 => &[(28, 0, 1), (24, 1, 4), (16, 5, 8), (8, 13, 8), (0, 21, 8)],
        30 => &[(28, 0, 2), (24, 2, 4), (16, 6, 8), (8, 14, 8), (0, 22, 8)],
        31 => &[(30, 0, 1), (26, 1, 4), (18, 5, 8), (10, 13, 8), (2, 21, 8), (0, 29, 2)],
        _ => &[],
    }
}

fn place(source: &[u8], target: &mut [u8], pieces: &[(usize, usize, usize)]) {
    for &(from, to, length) in pieces {
        for index in 0..length {
            // 長さ 2・3 の組は逆の順に写す
            let at = if length == 2 || length == 3 { from + length - 1 - index } else { from + index };
            target[to + index] = source[at];
        }
    }
}

/// R2007 の圧縮（5.10）。data の at から length バイトを伸長する
pub(crate) fn decompress_r21(data: &[u8], at: usize, length: usize, capacity: usize) -> Vec<u8> {
    let mut sink = Sink { out: Vec::with_capacity(capacity.min(1 << 20)), capacity };
    let end = at.saturating_add(length).min(data.len());
    let _ = r21(&data[..end], at, &mut sink);
    sink.out
}

fn r21(data: &[u8], at: usize, sink: &mut Sink) -> Read<()> {
    let mut source = Source { data, at };
    let mut opcode = source.byte()?;
    let mut length = 0usize;
    if source.at >= data.len() {
        return Ok(());
    }
    if opcode & 0xf0 == 0x20 {
        source.at += 3;
        length = usize::from(*data.get(source.at - 1).ok_or(Eof)? & 7);
    }
    while source.at < data.len() {
        if length == 0 {
            length = r21_literal_length(&mut source, opcode)?;
        }
        r21_literal(&mut source, sink, length)?;
        if source.at >= data.len() {
            break;
        }
        // 伸長済みのデータから写す組が続く（5.10.2）
        opcode = source.byte()?;
        let (mut offset, mut count) = r21_instruction(&mut source, &mut opcode)?;
        loop {
            sink.back(offset, count)?;
            length = usize::from(opcode & 7);
            if length != 0 || source.at >= data.len() {
                break;
            }
            opcode = source.byte()?;
            if opcode >> 4 == 0 {
                break;
            }
            if opcode >> 4 == 15 {
                opcode &= 15;
            }
            (offset, count) = r21_instruction(&mut source, &mut opcode)?;
        }
    }
    Ok(())
}

fn r21_literal_length(source: &mut Source<'_>, opcode: u8) -> Read<usize> {
    let mut length = usize::from(opcode) + 8;
    if length == 0x17 {
        let mut n = usize::from(source.byte()?);
        length += n;
        if n == 0xff {
            loop {
                n = usize::from(source.byte()?) | (usize::from(source.byte()?) << 8);
                length += n;
                if n != 0xffff {
                    break;
                }
            }
        }
    }
    Ok(length)
}

fn r21_literal(source: &mut Source<'_>, sink: &mut Sink, length: usize) -> Read<()> {
    let end = source.at.checked_add(length).ok_or(Eof)?;
    let bytes = source.data.get(source.at..end).ok_or(Eof)?;
    let room = sink.room(length);
    let mut out = Vec::with_capacity(length);
    let (chunks, rest) = bytes.as_chunks::<32>();
    let mut block = [0u8; 32];
    for chunk in chunks {
        place(chunk, &mut block, &BLOCK);
        out.extend_from_slice(&block);
    }
    if !rest.is_empty() {
        place(rest, &mut block, tail_pieces(rest.len()));
        out.extend_from_slice(&block[..rest.len()]);
    }
    sink.out.extend_from_slice(&out[..room]);
    if room < length {
        return Err(Eof);
    }
    source.at = end;
    Ok(())
}

/// 写す組のオフセットと長さ（5.10.2 の ReadInstructions）。opcode は次の組の印に置き換わる
fn r21_instruction(source: &mut Source<'_>, opcode: &mut u8) -> Read<(usize, usize)> {
    let code = *opcode;
    Ok(match code >> 4 {
        0 => {
            let mut length = usize::from(code & 0xf) + 0x13;
            let mut offset = usize::from(source.byte()?);
            *opcode = source.byte()?;
            length += usize::from((*opcode >> 3) & 0x10);
            offset += (usize::from(*opcode & 0x78) << 5) + 1;
            (offset, length)
        }
        1 => {
            let length = usize::from(code & 0xf) + 3;
            let mut offset = usize::from(source.byte()?);
            *opcode = source.byte()?;
            offset += (usize::from(*opcode & 0xf8) << 5) + 1;
            (offset, length)
        }
        2 => {
            let mut offset = usize::from(source.byte()?);
            offset |= usize::from(source.byte()?) << 8;
            let mut length = usize::from(code & 7);
            if code & 8 == 0 {
                *opcode = source.byte()?;
                length += usize::from(*opcode & 0xf8);
            } else {
                offset += 1;
                length += usize::from(source.byte()?) << 3;
                *opcode = source.byte()?;
                length += (usize::from(*opcode & 0xf8) << 8) + 0x100;
            }
            (offset, length)
        }
        _ => {
            let length = usize::from(code >> 4);
            let mut offset = usize::from(code & 15);
            *opcode = source.byte()?;
            offset += (usize::from(*opcode & 0xf8) << 1) + 1;
            (offset, length)
        }
    })
}

/// Reed-Solomon で符号化した並び（5.13）から、誤り訂正のバイトを除いたデータを取り出す。
/// factor 個の 255 バイトの組が 1 バイトずつ交互に並ぶ（interleaved）。誤りの訂正はしない
pub(crate) fn deinterleave(encoded: &[u8], factor: usize, block: usize, size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(size);
    let mut left = size;
    for n in 0..factor {
        let take = left.min(block);
        left -= take;
        for index in 0..take {
            match encoded.get(n + index * factor) {
                Some(byte) => out.push(*byte),
                None => return out,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r18_copies_literals_and_overlapping_matches() {
        // 0x14 は 3 文字（0x14 − 0x11）。0x41 と 0x00 は 1 バイト前から 3 バイト（4 − 1）を写し、続けて 1 文字（0x41 & 3）
        let data = [0x14, b'a', b'b', b'c', 0x41, 0x00, b'd', 0x11];
        assert_eq!(decompress_r18(&data, 64), b"abccccd");
    }

    #[test]
    fn r21_literal_blocks_reorder_bytes() {
        // 長さ 2 の文字は逆の順、4 はそのまま
        let mut block = [0u8; 32];
        place(&[1, 2], &mut block, tail_pieces(2));
        assert_eq!(&block[..2], &[2, 1]);
        place(&[1, 2, 3, 4], &mut block, tail_pieces(4));
        assert_eq!(&block[..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn broken_data_stops_without_panicking() {
        assert!(decompress_r18(&[0x20, 0x00], 16).is_empty());
        assert!(decompress_r21(&[0xff, 0xff, 0x30], 0, 3, 16).len() <= 16);
        assert_eq!(deinterleave(&[1, 2, 3, 4, 5, 6], 2, 3, 6), vec![1, 3, 5, 2, 4, 6]);
    }
}
