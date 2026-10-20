//! DWG のビット単位の値（Open Design Specification for .dwg files 5.4.1、2「BIT CODES AND DATA DEFINITIONS」）。
//! 読む前に範囲を確かめ、足りなければ `Eof` を返す。壊れた図面でも panic せず、読める所まで読む

use encoding_rs::Encoding;

/// 値を読み切る前にデータが終わった（または値が仕様にない形だった）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Eof;

pub(crate) type Read<T> = Result<T, Eof>;

/// 文字列（TV）の読み方。R2007 以降は UTF-16（TU）、それより前は図面の文字コードの 1 バイト・2 バイトの文字
#[derive(Debug, Clone, Copy)]
pub(crate) enum TextCodec {
    Utf16,
    Bytes(&'static Encoding),
}

/// ハンドルの参照（2.13）。code は参照の種類（2〜5 は値そのもの、6・8・A・C は基準のハンドルからのずれ）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandleRef {
    pub code: u8,
    pub value: u64,
}

impl HandleRef {
    /// 参照先のハンドル。reference はずれの基準（その図形自身のハンドル）。0 は参照なし
    pub fn resolve(self, reference: u64) -> u64 {
        match self.code {
            0x6 => reference.wrapping_add(1),
            0x8 => reference.wrapping_sub(1),
            0xA => reference.wrapping_add(self.value),
            0xC => reference.wrapping_sub(self.value),
            _ => self.value,
        }
    }
}

/// バイト列をビットの並びとして先頭から読む（各バイトの上位ビットが先）
#[derive(Debug, Clone)]
pub(crate) struct Bits<'a> {
    data: &'a [u8],
    /// 次に読むビットの位置
    pos: u64,
    /// 読める範囲の終わり（このビットからは読まない）
    end: u64,
}

impl<'a> Bits<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0, end: data.len() as u64 * 8 }
    }

    /// byte 目から読む
    pub fn at(data: &'a [u8], byte: usize) -> Self {
        let mut bits = Self::new(data);
        bits.set_pos((byte as u64).saturating_mul(8));
        bits
    }

    pub fn pos(&self) -> u64 {
        self.pos
    }

    pub fn set_pos(&mut self, pos: u64) {
        self.pos = pos;
    }

    /// 読める範囲を end までにする（データの長さを超えない）
    pub fn set_end(&mut self, end: u64) {
        self.end = end.min(self.data.len() as u64 * 8);
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    /// n ビットを読めるか
    fn need(&self, n: u64) -> Read<()> {
        if self.pos.checked_add(n).is_some_and(|after| after <= self.end) { Ok(()) } else { Err(Eof) }
    }

    pub fn skip(&mut self, n: u64) -> Read<()> {
        self.need(n)?;
        self.pos += n;
        Ok(())
    }

    /// B
    pub fn bit(&mut self) -> Read<bool> {
        self.need(1)?;
        let byte = self.data[(self.pos >> 3) as usize];
        let value = byte & (0x80 >> (self.pos & 7)) != 0;
        self.pos += 1;
        Ok(value)
    }

    /// 上位から n ビット（n は 8 まで）
    fn small(&mut self, n: u32) -> Read<u8> {
        let mut value = 0;
        for _ in 0..n {
            value = (value << 1) | u8::from(self.bit()?);
        }
        Ok(value)
    }

    /// BB
    pub fn bb(&mut self) -> Read<u8> {
        self.small(2)
    }

    /// RC（バイトの境目にないこともある）
    pub fn rc(&mut self) -> Read<u8> {
        self.need(8)?;
        let index = (self.pos >> 3) as usize;
        let shift = (self.pos & 7) as u32;
        let value = if shift == 0 {
            self.data[index]
        } else {
            (self.data[index] << shift) | (self.data[index + 1] >> (8 - shift))
        };
        self.pos += 8;
        Ok(value)
    }

    pub fn bytes(&mut self, n: usize) -> Read<Vec<u8>> {
        self.need(n as u64 * 8)?;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.rc()?);
        }
        Ok(out)
    }

    /// RS（リトルエンディアン）
    pub fn rs(&mut self) -> Read<u16> {
        let low = self.rc()?;
        let high = self.rc()?;
        Ok(u16::from_le_bytes([low, high]))
    }

    /// RL（リトルエンディアン）
    pub fn rl(&mut self) -> Read<u32> {
        let mut bytes = [0; 4];
        for byte in &mut bytes {
            *byte = self.rc()?;
        }
        Ok(u32::from_le_bytes(bytes))
    }

    /// 8 バイトのリトルエンディアンの整数
    pub fn rll(&mut self) -> Read<u64> {
        let mut bytes = [0; 8];
        for byte in &mut bytes {
            *byte = self.rc()?;
        }
        Ok(u64::from_le_bytes(bytes))
    }

    /// RD
    pub fn rd(&mut self) -> Read<f64> {
        Ok(f64::from_bits(self.rll()?))
    }

    /// 2RD
    pub fn rd2(&mut self) -> Read<[f64; 2]> {
        Ok([self.rd()?, self.rd()?])
    }

    /// 3RD
    pub fn rd3(&mut self) -> Read<[f64; 3]> {
        Ok([self.rd()?, self.rd()?, self.rd()?])
    }

    /// BS（2.2）。00 は RS、01 は RC、10 は 0、11 は 256
    pub fn bs(&mut self) -> Read<i16> {
        Ok(match self.bb()? {
            0 => self.rs()? as i16,
            1 => i16::from(self.rc()?),
            2 => 0,
            _ => 256,
        })
    }

    /// BL（2.3）。00 は RL、01 は RC、10 は 0。11 は使われない
    pub fn bl(&mut self) -> Read<i32> {
        match self.bb()? {
            0 => Ok(self.rl()? as i32),
            1 => Ok(i32::from(self.rc()?)),
            2 => Ok(0),
            _ => Err(Eof),
        }
    }

    /// BLL（2.4）。バイト数は 3 ビット（2.1 の 3B ではなく、つねに 3 ビット。ezdwg・ACadSharp と同じで、実際の図面で確かめた）
    pub fn bll(&mut self) -> Read<u64> {
        let length = self.small(3)?;
        let mut value = 0u64;
        for index in 0..length {
            value |= u64::from(self.rc()?) << (8 * u32::from(index));
        }
        Ok(value)
    }

    /// BD（2.5）。00 は RD、01 は 1.0、10 は 0.0
    pub fn bd(&mut self) -> Read<f64> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(Eof),
        }
    }

    /// 2BD
    pub fn bd2(&mut self) -> Read<[f64; 2]> {
        Ok([self.bd()?, self.bd()?])
    }

    /// 3BD
    pub fn bd3(&mut self) -> Read<[f64; 3]> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    /// DD（2.9）。既定の値の一部のバイトを置き換える
    pub fn dd(&mut self, default: f64) -> Read<f64> {
        let mut bytes = default.to_le_bytes();
        match self.bb()? {
            0 => {}
            1 => {
                for byte in &mut bytes[0..4] {
                    *byte = self.rc()?;
                }
            }
            2 => {
                bytes[4] = self.rc()?;
                bytes[5] = self.rc()?;
                for byte in &mut bytes[0..4] {
                    *byte = self.rc()?;
                }
            }
            _ => return self.rd(),
        }
        Ok(f64::from_le_bytes(bytes))
    }

    /// 2DD
    pub fn dd2(&mut self, default: [f64; 2]) -> Read<[f64; 2]> {
        Ok([self.dd(default[0])?, self.dd(default[1])?])
    }

    /// 3DD
    pub fn dd3(&mut self, default: [f64; 3]) -> Read<[f64; 3]> {
        Ok([self.dd(default[0])?, self.dd(default[1])?, self.dd(default[2])?])
    }

    /// BE（2.8）。R2000 以降は 1 なら (0, 0, 1)。R13・R14 は 3BD
    pub fn be(&mut self, r2000: bool) -> Read<[f64; 3]> {
        if r2000 && self.bit()? { Ok([0.0, 0.0, 1.0]) } else { self.bd3() }
    }

    /// BT（2.10）。R2000 以降は 1 なら 0.0。R13・R14 は BD
    pub fn bt(&mut self, r2000: bool) -> Read<f64> {
        if r2000 && self.bit()? { Ok(0.0) } else { self.bd() }
    }

    /// 符号のない MC（2.6）。オブジェクトの表のハンドルのずれと、R2010 以降のハンドルの並びの長さ
    pub fn umc(&mut self) -> Read<u64> {
        let mut value = 0u64;
        for shift in (0..63).step_by(7) {
            let byte = self.rc()?;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(Eof)
    }

    /// 符号のある MC（2.6）。最後のバイトの 0x40 が負の印
    pub fn mc(&mut self) -> Read<i64> {
        let mut value = 0i64;
        for shift in (0..56).step_by(7) {
            let byte = self.rc()?;
            if byte & 0x80 == 0 {
                value |= i64::from(byte & 0x3f) << shift;
                return Ok(if byte & 0x40 != 0 { -value } else { value });
            }
            value |= i64::from(byte & 0x7f) << shift;
        }
        Err(Eof)
    }

    /// MS（2.7）。15 ビットずつの RS
    pub fn ms(&mut self) -> Read<u64> {
        let mut value = 0u64;
        for shift in (0..60).step_by(15) {
            let word = self.rs()?;
            value |= u64::from(word & 0x7fff) << shift;
            if word & 0x8000 == 0 {
                return Ok(value);
            }
        }
        Err(Eof)
    }

    /// OT（2.12）。R2010 以降はビットの組と 1・2 バイト、それより前は BS
    pub fn ot(&mut self, r2010: bool) -> Read<u16> {
        if !r2010 {
            return Ok(self.bs()? as u16);
        }
        Ok(match self.bb()? {
            0 => u16::from(self.rc()?),
            1 => u16::from(self.rc()?) + 0x1f0,
            _ => self.rs()?,
        })
    }

    /// H（2.13）。CODE（4 ビット）・COUNTER（4 ビット）と、上位のバイトから COUNTER バイトの値
    pub fn handle(&mut self) -> Read<HandleRef> {
        let form = self.rc()?;
        let code = form >> 4;
        let counter = form & 0x0f;
        if counter > 8 {
            return Err(Eof);
        }
        let mut value = 0u64;
        for _ in 0..counter {
            value = (value << 8) | u64::from(self.rc()?);
        }
        Ok(HandleRef { code, value })
    }

    /// TV。R2007 以降は BS の文字数と UTF-16、それより前は BS のバイト数と図面の文字コード。NUL は除く
    pub fn tv(&mut self, codec: TextCodec) -> Read<String> {
        let length = self.bs()?;
        if length <= 0 {
            return Ok(String::new());
        }
        self.text_body(length as usize, codec)
    }

    /// 長さ length（UTF-16 は文字数、それ以外はバイト数）の文字列の中身
    pub fn text_body(&mut self, length: usize, codec: TextCodec) -> Read<String> {
        let text = match codec {
            TextCodec::Utf16 => {
                self.need(length as u64 * 16)?;
                let mut units = Vec::with_capacity(length);
                for _ in 0..length {
                    units.push(self.rs()?);
                }
                String::from_utf16_lossy(&units)
            }
            TextCodec::Bytes(encoding) => {
                let bytes = self.bytes(length)?;
                encoding.decode_without_bom_handling(&bytes).0.into_owned()
            }
        };
        Ok(if text.contains('\0') { text.replace('\0', "") } else { text })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_codes_follow_the_examples_of_the_specification() {
        // 2.2 の例: 00 00000001 00000001 | 10 | 11 | 01 00001111 | 10 は 257、0、256、15、0
        let bits = "0000000001000000011011010000111110";
        let mut bytes = vec![0u8; bits.len().div_ceil(8)];
        for (index, bit) in bits.chars().enumerate() {
            if bit == '1' {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        let mut reader = Bits::new(&bytes);
        let values: Vec<i16> = (0..5).map(|_| reader.bs().unwrap()).collect();
        assert_eq!(values, [257, 0, 256, 15, 0]);
        // 2.6 の例: 10000010 00100100 は 4610、11101001 10010111 11100110 00110101 は 112823273、
        // 10000101 01001011 は -1413
        assert_eq!(Bits::new(&[0b1000_0010, 0b0010_0100]).mc(), Ok(4610));
        assert_eq!(Bits::new(&[0b1110_1001, 0b1001_0111, 0b1110_0110, 0b0011_0101]).mc(), Ok(112_823_273));
        assert_eq!(Bits::new(&[0b1000_0101, 0b0100_1011]).mc(), Ok(-1413));
        // 2.7 の例: 00110001 11110100 10001101 00000000 は 4650033
        assert_eq!(Bits::new(&[0b0011_0001, 0b1111_0100, 0b1000_1101, 0]).ms(), Ok(4_650_033));
        // 2.13 の例: 0101.0010.00000101.11100111 は LAYER 5E7 へのハード参照
        assert_eq!(Bits::new(&[0x52, 0x05, 0xe7]).handle(), Ok(HandleRef { code: 5, value: 0x5e7 }));
    }

    #[test]
    fn reading_past_the_end_fails_without_panicking() {
        let mut reader = Bits::new(&[0xff]);
        assert_eq!(reader.rl(), Err(Eof));
        // BS（01 と 5）は長さ 5 の文字列。続く UTF-16 の 10 バイトがない
        let mut reader = Bits::new(&[0b0100_0001, 0b0100_0000]);
        assert_eq!(reader.tv(TextCodec::Utf16), Err(Eof));
    }
}
