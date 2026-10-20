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

#[cfg(test)]
mod tests {
    use super::*;

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
