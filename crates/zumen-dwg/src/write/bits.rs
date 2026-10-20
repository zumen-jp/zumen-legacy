//! ビット単位の値を書く（2「BIT CODES AND DATA DEFINITIONS」）。読み取りの bits.rs と対になり、
//! 読み取りが返す値と同じ値に戻る書き方をする。値を縮める書き方（BS の 0 と 256、BD の 0.0 と 1.0、DD の
//! 既定の値との差など）は、AutoCAD と同じく使える所ではつねに使う

/// ビットの並び。各バイトの上位のビットから詰める
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BitWriter {
    bytes: Vec<u8>,
    /// 書いたビットの数
    len: u64,
}

impl BitWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 書いたビットの数
    pub fn bit_len(&self) -> u64 {
        self.len
    }

    /// 書いたバイト列（最後のバイトの余りのビットは 0）
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// B
    pub fn bit(&mut self, value: bool) {
        self.bits(u64::from(value), 1);
    }

    /// value の下位 count ビット（64 まで）を上位のビットから
    pub fn bits(&mut self, value: u64, count: u32) {
        let mut left = count.min(64);
        while left > 0 {
            let used = (self.len % 8) as u32;
            if used == 0 {
                self.bytes.push(0);
            }
            let take = left.min(8 - used);
            let chunk = ((value >> (left - take)) & ((1u64 << take) - 1)) as u8;
            let last = self.bytes.len() - 1;
            self.bytes[last] |= chunk << (8 - used - take);
            self.len += u64::from(take);
            left -= take;
        }
    }

    /// BB
    pub fn bb(&mut self, value: u8) {
        self.bits(u64::from(value & 3), 2);
    }

    /// RC
    pub fn rc(&mut self, value: u8) {
        if self.len.is_multiple_of(8) {
            self.bytes.push(value);
            self.len += 8;
        } else {
            self.bits(u64::from(value), 8);
        }
    }

    /// RC を並べたバイト列
    pub fn raw(&mut self, bytes: &[u8]) {
        if self.len.is_multiple_of(8) {
            self.bytes.extend_from_slice(bytes);
            self.len += bytes.len() as u64 * 8;
        } else {
            for byte in bytes {
                self.bits(u64::from(*byte), 8);
            }
        }
    }

    /// RS（リトルエンディアン）
    pub fn rs(&mut self, value: u16) {
        self.raw(&value.to_le_bytes());
    }

    /// RL（リトルエンディアン）
    pub fn rl(&mut self, value: u32) {
        self.raw(&value.to_le_bytes());
    }

    /// 8 バイトのリトルエンディアンの整数
    pub fn rll(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    /// RD
    pub fn rd(&mut self, value: f64) {
        self.raw(&value.to_le_bytes());
    }

    /// 2RD
    pub fn rd2(&mut self, value: [f64; 2]) {
        self.rd(value[0]);
        self.rd(value[1]);
    }

    /// 3RD
    pub fn rd3(&mut self, value: [f64; 3]) {
        for v in value {
            self.rd(v);
        }
    }

    /// BS（2.2）。0 は 10、256 は 11、1〜255 は 01 と RC、ほか（負の数を含む）は 00 と RS
    pub fn bs(&mut self, value: i16) {
        match value {
            0 => self.bb(2),
            256 => self.bb(3),
            1..=255 => {
                self.bb(1);
                self.rc(value as u8);
            }
            _ => {
                self.bb(0);
                self.rs(value as u16);
            }
        }
    }

    /// BL（2.3）。0 は 10、1〜255 は 01 と RC、ほかは 00 と RL
    pub fn bl(&mut self, value: i32) {
        match value {
            0 => self.bb(2),
            1..=255 => {
                self.bb(1);
                self.rc(value as u8);
            }
            _ => {
                self.bb(0);
                self.rl(value as u32);
            }
        }
    }

    /// BLL（2.4）。バイト数（3 ビット。読み取りと同じく 3B ではなくつねに 3 ビット）と下位のバイトから
    pub fn bll(&mut self, value: u64) {
        let bytes = value.to_le_bytes();
        let length = bytes.iter().rposition(|byte| *byte != 0).map_or(0, |at| at + 1);
        // 3 ビットで表せるのは 7 バイトまで。8 バイトの値は上位のバイトを落とさないよう 7 バイトに収まらない
        let length = length.min(7);
        self.bits(length as u64, 3);
        self.raw(&bytes[..length]);
    }

    /// BD（2.5）。+0.0 は 10、1.0 は 01、ほか（-0.0 と NaN を含む）は 00 と RD
    pub fn bd(&mut self, value: f64) {
        if value.to_bits() == 0 {
            self.bb(2);
        } else if value.to_bits() == 1f64.to_bits() {
            self.bb(1);
        } else {
            self.bb(0);
            self.rd(value);
        }
    }

    /// 2BD
    pub fn bd2(&mut self, value: [f64; 2]) {
        self.bd(value[0]);
        self.bd(value[1]);
    }

    /// 3BD
    pub fn bd3(&mut self, value: [f64; 3]) {
        for v in value {
            self.bd(v);
        }
    }

    /// DD（2.9）。既定の値と同じなら 00、上位 4 バイトが同じなら 01 と下位 4 バイト、上位 2 バイトが同じなら 10 と
    /// 5・6 バイト目・下位 4 バイト、ほかは 11 と RD
    pub fn dd(&mut self, value: f64, default: f64) {
        let bytes = value.to_le_bytes();
        let base = default.to_le_bytes();
        if bytes == base {
            self.bb(0);
        } else if bytes[4..] == base[4..] {
            self.bb(1);
            self.raw(&bytes[..4]);
        } else if bytes[6..] == base[6..] {
            self.bb(2);
            self.rc(bytes[4]);
            self.rc(bytes[5]);
            self.raw(&bytes[..4]);
        } else {
            self.bb(3);
            self.rd(value);
        }
    }

    /// 2DD
    pub fn dd2(&mut self, value: [f64; 2], default: [f64; 2]) {
        self.dd(value[0], default[0]);
        self.dd(value[1], default[1]);
    }

    /// 3DD
    pub fn dd3(&mut self, value: [f64; 3], default: [f64; 3]) {
        for index in 0..3 {
            self.dd(value[index], default[index]);
        }
    }

    /// BE（2.8、R2000 以降）。(0, 0, 1) なら 1、ほかは 0 と 3BD
    pub fn be(&mut self, value: [f64; 3]) {
        if value.map(f64::to_bits) == [0, 0, 1f64.to_bits()] {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd3(value);
        }
    }

    /// BT（2.10、R2000 以降）。+0.0 なら 1、ほかは 0 と BD
    pub fn bt(&mut self, value: f64) {
        if value.to_bits() == 0 {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd(value);
        }
    }

    /// 符号のない MC（2.6）。7 ビットずつ、続くなら 0x80
    pub fn umc(&mut self, value: u64) {
        let mut rest = value;
        loop {
            let byte = (rest & 0x7f) as u8;
            rest >>= 7;
            if rest == 0 {
                self.rc(byte);
                return;
            }
            self.rc(byte | 0x80);
        }
    }

    /// 符号のある MC（2.6）。最後のバイトは 6 ビットの値と負の印 0x40
    pub fn mc(&mut self, value: i64) {
        let negative = value < 0;
        let mut rest = value.unsigned_abs();
        loop {
            if rest < 0x40 {
                self.rc(rest as u8 | if negative { 0x40 } else { 0 });
                return;
            }
            self.rc((rest & 0x7f) as u8 | 0x80);
            rest >>= 7;
        }
    }

    /// MS（2.7）。15 ビットずつの RS、続くなら 0x8000
    pub fn ms(&mut self, value: u64) {
        let mut rest = value;
        loop {
            let word = (rest & 0x7fff) as u16;
            rest >>= 15;
            if rest == 0 {
                self.rs(word);
                return;
            }
            self.rs(word | 0x8000);
        }
    }

    /// OT（2.12）。R2010 以降は 00 と RC、01 と RC（0x1F0 を引いた値）、10 と RS。それより前は BS
    pub fn ot(&mut self, value: u16, r2010: bool) {
        if !r2010 {
            self.bs(value as i16);
        } else if value < 0x100 {
            self.bb(0);
            self.rc(value as u8);
        } else if (0x1f0..0x2f0).contains(&value) {
            self.bb(1);
            self.rc((value - 0x1f0) as u8);
        } else {
            self.bb(2);
            self.rs(value);
        }
    }

    /// H（2.13）。CODE（4 ビット）・COUNTER（4 ビット）と、上位のバイトから COUNTER バイトの値。0 は COUNTER 0
    pub fn handle(&mut self, code: u8, value: u64) {
        let bytes = value.to_be_bytes();
        let skip = bytes.iter().position(|byte| *byte != 0).unwrap_or(8);
        self.rc((code << 4) | (8 - skip) as u8);
        self.raw(&bytes[skip..]);
    }

    /// ほかの並びをつなげる
    pub fn append(&mut self, other: &BitWriter) {
        self.append_slice(&other.bytes, 0, other.len);
    }

    /// data の start ビット目から count ビットを写す（オブジェクトをビットのまま写すときに使う）
    pub fn append_slice(&mut self, data: &[u8], start: u64, count: u64) {
        let end = start.saturating_add(count).min(data.len() as u64 * 8);
        let mut at = start.min(end);
        if self.len.is_multiple_of(8) && at.is_multiple_of(8) {
            let whole = ((end - at) / 8) as usize;
            let from = (at / 8) as usize;
            self.raw(&data[from..from + whole]);
            at += whole as u64 * 8;
        }
        while at < end {
            let take = (end - at).min(8 - at % 8) as u32;
            let byte = data[(at / 8) as usize];
            let value = (byte >> (8 - (at % 8) as u32 - take)) & ((1u16 << take) - 1) as u8;
            self.bits(u64::from(value), take);
            at += u64::from(take);
        }
    }

    /// 次のバイトの境目まで 0 のビットで埋める
    pub fn pad(&mut self) {
        let rest = (8 - self.len % 8) % 8;
        self.bits(0, rest as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::Bits;

    #[test]
    fn the_values_read_back_as_written() {
        let mut w = BitWriter::new();
        w.bit(true);
        for value in [0i16, 1, 255, 256, 257, -1, i16::MIN] {
            w.bs(value);
        }
        for value in [0i32, 1, 255, 256, -1, i32::MAX] {
            w.bl(value);
        }
        for value in [0.0, -0.0, 1.0, 2.5, f64::MAX] {
            w.bd(value);
        }
        for (value, default) in [(1.5, 1.5), (1.5000001, 1.5), (1.25, 1.5), (-7.0, 1.5), (0.0, 3.0)] {
            w.dd(value, default);
        }
        w.bll(0x0123_4567);
        w.bll(0);
        w.umc(0x12_3456);
        w.mc(-1413);
        w.mc(112_823_273);
        w.ms(4_650_033);
        w.ot(0x1f2, true);
        w.ot(0x31, true);
        w.ot(0x300, true);
        w.ot(500, false);
        w.handle(5, 0x5e7);
        w.handle(4, 0);
        w.be([0.0, 0.0, 1.0]);
        w.be([0.0, 1.0, 0.0]);
        w.bt(0.0);
        w.bt(2.0);
        let bytes = w.as_bytes().to_vec();
        let mut r = Bits::new(&bytes);
        assert!(r.bit().unwrap());
        for value in [0i16, 1, 255, 256, 257, -1, i16::MIN] {
            assert_eq!(r.bs().unwrap(), value);
        }
        for value in [0i32, 1, 255, 256, -1, i32::MAX] {
            assert_eq!(r.bl().unwrap(), value);
        }
        for value in [0.0f64, -0.0, 1.0, 2.5, f64::MAX] {
            assert_eq!(r.bd().unwrap().to_bits(), value.to_bits());
        }
        for (value, default) in [(1.5f64, 1.5), (1.5000001, 1.5), (1.25, 1.5), (-7.0, 1.5), (0.0, 3.0)] {
            assert_eq!(r.dd(default).unwrap().to_bits(), value.to_bits());
        }
        assert_eq!(r.bll().unwrap(), 0x0123_4567);
        assert_eq!(r.bll().unwrap(), 0);
        assert_eq!(r.umc().unwrap(), 0x12_3456);
        assert_eq!(r.mc().unwrap(), -1413);
        assert_eq!(r.mc().unwrap(), 112_823_273);
        assert_eq!(r.ms().unwrap(), 4_650_033);
        assert_eq!(r.ot(true).unwrap(), 0x1f2);
        assert_eq!(r.ot(true).unwrap(), 0x31);
        assert_eq!(r.ot(true).unwrap(), 0x300);
        assert_eq!(r.ot(false).unwrap(), 500);
        assert_eq!(r.handle().unwrap(), crate::bits::HandleRef { code: 5, value: 0x5e7 });
        assert_eq!(r.handle().unwrap(), crate::bits::HandleRef { code: 4, value: 0 });
        assert_eq!(r.be(true).unwrap(), [0.0, 0.0, 1.0]);
        assert_eq!(r.be(true).unwrap(), [0.0, 1.0, 0.0]);
        assert_eq!(r.bt(true).unwrap(), 0.0);
        assert_eq!(r.bt(true).unwrap(), 2.0);
        assert_eq!(r.pos(), w.bit_len());
    }

    #[test]
    fn the_short_forms_match_the_examples_of_the_specification() {
        // 2.2 の例: 257、0、256、15、0 は 00 00000001 00000001 | 10 | 11 | 01 00001111 | 10
        let mut w = BitWriter::new();
        for value in [257, 0, 256, 15, 0] {
            w.bs(value);
        }
        let expected = "0000000001000000011011010000111110";
        let written: String =
            (0..w.bit_len()).map(|at| if w.as_bytes()[(at / 8) as usize] & (0x80 >> (at % 8)) != 0 { '1' } else { '0' }).collect();
        assert_eq!(written, expected);
        // 2.6 の例: 4610 は 10000010 00100100
        let mut w = BitWriter::new();
        w.mc(4610);
        assert_eq!(w.as_bytes(), [0b1000_0010, 0b0010_0100]);
        // 2.13 の例: LAYER 5E7 へのハード参照は 0101.0010.00000101.11100111
        let mut w = BitWriter::new();
        w.handle(5, 0x5e7);
        assert_eq!(w.as_bytes(), [0x52, 0x05, 0xe7]);
    }

    #[test]
    fn bits_are_copied_at_any_alignment() {
        let source = [0b1011_0011, 0b0101_1100, 0b1111_0000];
        for start in 0..12 {
            for prefix in 0..9 {
                let mut w = BitWriter::new();
                w.bits(0, prefix);
                w.append_slice(&source, start, 11);
                let mut r = Bits::new(w.as_bytes());
                r.skip(u64::from(prefix)).unwrap();
                let mut expected = Bits::new(&source);
                expected.skip(start).unwrap();
                for _ in 0..11 {
                    assert_eq!(r.bit(), expected.bit(), "start {start} prefix {prefix}");
                }
            }
        }
    }
}
