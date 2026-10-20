//! DWG の検査の値（Open Design Specification for .dwg files 5.4.1、2.14「CRCS」、4.2「Section page checksum」）。
//! 書き出しは、AutoCAD が書いた図面に記録された値と同じ値を作る（tests/write.rs で実物の図面と照らす）

/// 2.14.1 の 8 ビットの CRC の表（256 個の 16 ビットの値）。仕様の表は、多項式 0xA001 を下位のビットから
/// 割る CRC-16 の表と同じなので、ここで作る（先頭の値は仕様の表と照らす。下のテストを見る）
const CRC8_TABLE: [u16; 256] = {
    let mut table = [0u16; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u16;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 { (value >> 1) ^ 0xa001 } else { value >> 1 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// 2.14.2 の 32 ビットの CRC の表。多項式 0xEDB88320 を下位のビットから割る CRC-32 の表と同じ
const CRC32_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u32;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 { (value >> 1) ^ 0xedb8_8320 } else { value >> 1 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// オブジェクト・ヘッダ変数・クラス・オブジェクトの表の区切りの CRC の初めの値（2.14.1 の「starting value」）。
/// R2000 のファイルの頭は 0 から始めて節の位置の数ごとの値と排他的論理和をとる（3.2.6）が、それはこの値から
/// 始めるのと同じになる（実物の図面で確かめた）
pub(crate) const CRC8_SEED: u16 = 0xc0c1;

/// 2.14.1 の 8 ビットの CRC（値は 2 バイト）。seed は初めの値か、続きを計算するときは前の結果
pub(crate) fn crc8(seed: u16, data: &[u8]) -> u16 {
    let mut value = seed;
    for byte in data {
        let index = (byte ^ (value as u8)) as usize;
        value = (value >> 8) ^ CRC8_TABLE[index];
    }
    value
}

/// 2.14.2 の 32 ビットの CRC。R2004 以降のファイルの頭（4.1）に使う
pub(crate) fn crc32(seed: u32, data: &[u8]) -> u32 {
    let mut inverted = !seed;
    for byte in data {
        inverted = (inverted >> 8) ^ CRC32_TABLE[((inverted ^ u32::from(*byte)) & 0xff) as usize];
    }
    !inverted
}

/// 4.2 のページの検査の値。R2004 以降のシステムのページとデータのページの頭に書く
pub(crate) fn page_checksum(seed: u32, data: &[u8]) -> u32 {
    let mut sum1 = seed & 0xffff;
    let mut sum2 = seed >> 16;
    for chunk in data.chunks(0x15b0) {
        for byte in chunk {
            sum1 += u32::from(*byte);
            sum2 += sum1;
        }
        sum1 %= 0xfff1;
        sum2 %= 0xfff1;
    }
    (sum2 << 16) | (sum1 & 0xffff)
}

/// 5.12.1 の normal な 64 ビットの CRC の表。ECMA-182 の多項式 0x42F0E1EBA9EA3693 を上位のビットから割る
const CRC64_NORMAL_TABLE: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = (index as u64) << 56;
        let mut bit = 0;
        while bit < 8 {
            value = if value >> 63 != 0 { (value << 1) ^ 0x42f0_e1eb_a9ea_3693 } else { value << 1 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// 5.12.2 の mirrored な 64 ビットの CRC の表。多項式 0xAD93D23594C935A9（Jones）を下位のビットから割る
/// （ビットを逆にした 0x95AC9329AC4BC9B5 で割る。仕様の表と同じになる）
const CRC64_MIRRORED_TABLE: [u64; 256] = {
    let mut table = [0u64; 256];
    let mut index = 0;
    while index < 256 {
        let mut value = index as u64;
        let mut bit = 0;
        while bit < 8 {
            value = if value & 1 != 0 { (value >> 1) ^ 0x95ac_9329_ac4b_c9b5 } else { value >> 1 };
            bit += 1;
        }
        table[index] = value;
        index += 1;
    }
    table
};

/// 5.12 と 5.4.1 のバイトを扱う順。8 バイトの組は 6, 7, 4, 5, 2, 3, 0, 1 の順、残りの 1〜7 バイトは仕様の表の順
fn r21_order(data: &[u8], mut byte: impl FnMut(u8)) {
    let (chunks, rest) = data.as_chunks::<8>();
    for chunk in chunks {
        for index in [6, 7, 4, 5, 2, 3, 0, 1] {
            byte(chunk[index]);
        }
    }
    let order: &[usize] = match rest.len() {
        1 => &[0],
        2 => &[0, 1],
        3 => &[0, 1, 2],
        4 => &[2, 3, 0, 1],
        5 => &[2, 3, 0, 1, 4],
        6 => &[2, 3, 0, 1, 4, 5],
        7 => &[2, 3, 0, 1, 4, 5, 6],
        _ => &[],
    };
    for &index in order {
        byte(rest[index]);
    }
}

/// 5.12.1 の normal な 64 ビットの CRC。seed から始め、終わりにビットを反転する
pub(crate) fn crc64_normal(seed: u64, data: &[u8]) -> u64 {
    let mut crc = seed;
    r21_order(data, |byte| crc = CRC64_NORMAL_TABLE[((u64::from(byte) ^ (crc >> 56)) & 0xff) as usize] ^ (crc << 8));
    !crc
}

/// 5.12.2 の mirrored な 64 ビットの CRC。seed から始める（反転しない）
pub(crate) fn crc64_mirrored(seed: u64, data: &[u8]) -> u64 {
    let mut crc = seed;
    r21_order(data, |byte| crc = CRC64_MIRRORED_TABLE[((crc ^ u64::from(byte)) & 0xff) as usize] ^ (crc >> 8));
    crc
}

/// 5.12 の UpdateSeed1（CRC の前に初めの値を作る）
pub(crate) fn update_seed1(seed: u64, length: u64) -> u64 {
    let mut seed = seed.wrapping_add(length).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    seed |= seed.wrapping_mul(0x343fd << 32).wrapping_add(0x269ec3 << 32);
    !seed
}

/// 5.12 の UpdateSeed2
pub(crate) fn update_seed2(seed: u64, length: u64) -> u64 {
    let seed = seed.wrapping_add(length).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    !seed.wrapping_mul((1 << 32) + 0x343fd).wrapping_add(length.wrapping_add(0x269ec3))
}

/// 5.4.1 の R2007 のデータのページの検査の値。4.2 と同じ Adler-32 の形で、初めの値を seed と長さから作り、
/// バイトを r21_order の順に足す
pub(crate) fn page_checksum_r21(seed: u64, data: &[u8]) -> u32 {
    let seed = seed.wrapping_add(data.len() as u64).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    let mut sum1 = (seed & 0xffff) as u32;
    let mut sum2 = ((seed >> 16) & 0xffff) as u32;
    for chunk in data.chunks(0x15b0) {
        r21_order(chunk, |byte| {
            sum1 += u32::from(byte);
            sum2 += sum1;
        });
        sum1 %= 0xfff1;
        sum2 %= 0xfff1;
    }
    (sum2 << 16) | (sum1 & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crc64_tables_are_the_tables_of_the_specification() {
        // 5.12.1 と 5.12.2 の表の 1 行目と最後の値
        assert_eq!(
            CRC64_NORMAL_TABLE[..4],
            [0x0000000000000000, 0x42f0e1eba9ea3693, 0x85e1c3d753d46d26, 0xc711223cfa3e5bb5]
        );
        assert_eq!(CRC64_NORMAL_TABLE[255], 0x9afce626ce85b507);
        assert_eq!(
            CRC64_MIRRORED_TABLE[..4],
            [0x0000000000000000, 0x7ad870c830358979, 0xf5b0e190606b12f2, 0x8f689158505e9b8b]
        );
        assert_eq!(CRC64_MIRRORED_TABLE[128], 0x95ac9329ac4bc9b5);
        assert_eq!(CRC64_MIRRORED_TABLE[255], 0x29b7d047efec8728);
    }

    #[test]
    fn the_tables_are_the_tables_of_the_specification() {
        // 2.14.1 の表の 1 行目と最後の行
        assert_eq!(CRC8_TABLE[..8], [0x0000, 0xC0C1, 0xC181, 0x0140, 0xC301, 0x03C0, 0x0280, 0xC241]);
        assert_eq!(CRC8_TABLE[248..], [0x8201, 0x42C0, 0x4380, 0x8341, 0x4100, 0x81C1, 0x8081, 0x4040]);
        // 2.14.2 の表の 1 行目と最後の行
        assert_eq!(CRC32_TABLE[..4], [0x00000000, 0x77073096, 0xee0e612c, 0x990951ba]);
        assert_eq!(CRC32_TABLE[252..], [0xb40bbe37, 0xc30c8ea1, 0x5a05df1b, 0x2d02ef8d]);
    }

    #[test]
    fn the_values_match_known_results() {
        // オブジェクトの表の終わりの区切り（大きさ 2 だけ）の CRC は、どの図面でも 0x01D0（実物の図面で確かめた）
        assert_eq!(crc8(CRC8_SEED, &[0x00, 0x02]), 0x01d0);
        // 標準の CRC-32 の検査の値（"123456789" → 0xCBF43926）
        assert_eq!(crc32(0, b"123456789"), 0xcbf4_3926);
        // 続きの計算は、まとめて計算したのと同じ
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xcbf4_3926);
        assert_eq!(crc8(crc8(CRC8_SEED, b"abc"), b"def"), crc8(CRC8_SEED, b"abcdef"));
        // 4.2 の検査の値は Adler-32 の形（剰余 0xFFF1 = 65521）
        assert_eq!(page_checksum(1, b"Wikipedia"), 0x11e6_0398);
        assert_eq!(page_checksum(0, &[]), 0);
    }
}
