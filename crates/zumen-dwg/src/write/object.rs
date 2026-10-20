//! 1 つのオブジェクトを書く（20.1「Common non-entity object format」、20.2「Common entity format」）。
//! 値・文字列・ハンドルの並びを分けて書き、最後に版の形にまとめる。R2007 以降は文字列を値の並びの後ろの
//! 文字列の並びに UTF-16 で、R2010 以降はハンドルの並びの大きさを MC で、型を OT で書く

use super::bits::BitWriter;
use crate::crc::{CRC8_SEED, crc8};
use crate::file::Version;
use encoding_rs::Encoding;

/// 書いているオブジェクト。data・strings・handles に値を書き、bytes で MS の大きさから CRC までにする
#[derive(Debug, Clone)]
pub struct ObjectWriter {
    pub version: Version,
    /// R2007 より前の文字列（TV）の文字コード
    pub encoding: &'static Encoding,
    /// 型（20.3。クラスのものは 500 から）
    pub kind: u16,
    pub handle: u64,
    /// 拡張データ（28）。(APPID のハンドル, 値のバイト列)
    pub eed: Vec<(u64, Vec<u8>)>,
    pub data: BitWriter,
    /// R2007 以降の文字列の並び
    pub strings: BitWriter,
    pub handles: BitWriter,
}

impl ObjectWriter {
    pub fn new(version: Version, encoding: &'static Encoding, kind: u16, handle: u64) -> Self {
        Self {
            version,
            encoding,
            kind,
            handle,
            eed: Vec::new(),
            data: BitWriter::new(),
            strings: BitWriter::new(),
            handles: BitWriter::new(),
        }
    }

    pub fn r2004(&self) -> bool {
        self.version >= Version::R2004
    }

    pub fn r2007(&self) -> bool {
        self.version >= Version::R2007
    }

    pub fn r2010(&self) -> bool {
        self.version >= Version::R2010
    }

    pub fn r2013(&self) -> bool {
        self.version >= Version::R2013
    }

    pub fn r2018(&self) -> bool {
        self.version >= Version::R2018
    }

    /// TV。R2007 以降は文字列の並びに BS の文字の数と UTF-16、それより前は値の並びに BS のバイト数と図面の文字コード
    pub fn tv(&mut self, text: &str) {
        if self.r2007() {
            write_tu(&mut self.strings, text);
        } else {
            let (bytes, _, _) = self.encoding.encode(text);
            self.data.bs(bytes.len().min(i16::MAX as usize) as i16);
            self.data.raw(&bytes[..bytes.len().min(i16::MAX as usize)]);
        }
    }

    /// TV をそのままの形で書く（R2007 より前のバイト、R2007 以降の UTF-16 の単位）
    pub(crate) fn text(&mut self, text: &crate::codec::RawText) {
        match text {
            crate::codec::RawText::Units(units) => {
                let target = if self.r2007() { &mut self.strings } else { &mut self.data };
                target.bs(units.len().min(i16::MAX as usize) as i16);
                for unit in units.iter().take(i16::MAX as usize) {
                    target.rs(*unit);
                }
            }
            crate::codec::RawText::Bytes(bytes) => {
                self.data.bs(bytes.len().min(i16::MAX as usize) as i16);
                self.data.raw(&bytes[..bytes.len().min(i16::MAX as usize)]);
            }
        }
    }

    /// H（ハンドルの並び）
    pub fn h(&mut self, code: u8, value: u64) {
        self.handles.handle(code, value);
    }

    /// 読んだハンドルの参照をそのまま書く
    pub(crate) fn href(&mut self, handle: crate::bits::HandleRef) {
        self.handles.handle(handle.code, handle.value);
    }

    /// CMC（2.11）
    pub(crate) fn cmc(&mut self, color: &crate::codec::CmColor) {
        self.data.bs(color.index);
        if !self.r2004() {
            return;
        }
        self.data.bl(color.rgb as i32);
        let flags = u8::from(color.name.is_some()) | (u8::from(color.book.is_some()) << 1);
        self.data.rc(flags);
        if let Some(name) = &color.name {
            self.text(name);
        }
        if let Some(book) = &color.book {
            self.text(book);
        }
    }

    /// 版の形にまとめる（20.1）。MS の大きさ、（R2010 以降）ハンドルの並びのビット数（MC）、型、（R2010 より前）
    /// ハンドルの並びの位置（RL、型の始まりからのビット数）、ハンドル、拡張データ、値、（R2007 以降）文字列の並びと
    /// その大きさと印、ハンドルの並び、バイトの境目までの 0、CRC（MS から、初めの値 0xC0C1）
    pub fn bytes(&self) -> Vec<u8> {
        self.frame().0
    }

    /// bytes と、ハンドルの並びの後ろの埋めたビットの数（テストで、埋めるビットを除いて比べる）
    pub(crate) fn frame(&self) -> (Vec<u8>, u64) {
        let r2010 = self.r2010();
        let mut rest = BitWriter::new();
        rest.handle(0, self.handle);
        for (app, data) in &self.eed {
            rest.bs(data.len().min(i16::MAX as usize) as i16);
            rest.handle(5, *app);
            rest.raw(&data[..data.len().min(i16::MAX as usize)]);
        }
        rest.bs(0);
        rest.append(&self.data);
        if self.r2007() {
            append_strings(&mut rest, &self.strings);
        }
        let mut body = BitWriter::new();
        body.ot(self.kind, r2010);
        if !r2010 {
            // ハンドルの並びの位置は型の始まりからのビット数。RL 自身の 32 ビットを含む
            let kind_bits = body.bit_len();
            body.rl((kind_bits + 32 + rest.bit_len()) as u32);
        }
        body.append(&rest);
        let data_bits = body.bit_len();
        body.append(&self.handles);
        let unpadded = body.bit_len();
        body.pad();
        let padding = body.bit_len() - unpadded;
        let handle_bits = body.bit_len() - data_bits;
        let body = body.into_bytes();
        let mut out = BitWriter::new();
        out.ms(body.len() as u64);
        if r2010 {
            out.umc(handle_bits);
        }
        out.raw(&body);
        let mut bytes = out.into_bytes();
        let crc = crc8(CRC8_SEED, &bytes);
        bytes.extend_from_slice(&crc.to_le_bytes());
        (bytes, padding)
    }
}

/// TU（R2007 以降）。BS の文字の数と UTF-16 のリトルエンディアン
pub(crate) fn write_tu(w: &mut BitWriter, text: &str) {
    let units: Vec<u16> = text.encode_utf16().take(i16::MAX as usize).collect();
    w.bs(units.len() as i16);
    for unit in units {
        w.rs(unit);
    }
}

/// R2007 以降の文字列の並びを値の並びの後ろにつなげる（20.1）。並び、その大きさ（ビット数。0x8000 以上なら上位の
/// 15 ビットの RS を先に置き、下位の RS に 0x8000 の印を付ける）、ある印（B）。並びがなければ印だけ（0）
pub(crate) fn append_strings(rest: &mut BitWriter, strings: &BitWriter) {
    let size = strings.bit_len();
    if size == 0 {
        rest.bit(false);
        return;
    }
    rest.append(strings);
    if size >= 0x8000 {
        rest.rs((size >> 15) as u16);
        rest.rs((size & 0x7fff) as u16 | 0x8000);
    } else {
        rest.rs(size as u16);
    }
    rest.bit(true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bits::TextCodec;
    use crate::objects::{Format, object_length, open_object};

    #[test]
    fn objects_open_as_written_in_every_version() {
        for version in [Version::R2000, Version::R2004, Version::R2007, Version::R2010, Version::R2018] {
            let mut o = ObjectWriter::new(version, encoding_rs::SHIFT_JIS, 19, 0x2a5);
            o.eed.push((0x12, vec![0x46, 1, 2, 3]));
            o.data.bd(2.5);
            o.tv("表示");
            o.h(5, 0x10);
            let bytes = o.bytes();
            let codec =
                if version >= Version::R2007 { TextCodec::Utf16 } else { TextCodec::Bytes(encoding_rs::SHIFT_JIS) };
            let format = Format { version, maintenance: 0, codec };
            assert_eq!(object_length(&bytes, 0, version >= Version::R2010), Some(bytes.len()));
            let mut object = open_object(&bytes, 0, format).unwrap();
            assert_eq!((object.kind, object.handle), (19, 0x2a5), "{version:?}");
            assert_eq!(object.eed.apps.len(), 1);
            assert_eq!(object.eed.apps[0].0, 0x12);
            assert_eq!(object.streams.data.bd().unwrap(), 2.5);
            assert_eq!(object.streams.tv().unwrap(), "表示");
            assert_eq!(object.streams.handle_ref(0x2a5).unwrap(), 0x10);
            let stored = u16::from_le_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]);
            assert_eq!(crc8(CRC8_SEED, &bytes[..bytes.len() - 2]), stored);
        }
    }

    #[test]
    fn a_long_string_stream_keeps_its_size() {
        let mut o = ObjectWriter::new(Version::R2018, encoding_rs::UTF_8, 1, 0x40);
        let text = "あ".repeat(3000);
        o.tv(&text);
        o.tv("終わり");
        let bytes = o.bytes();
        let format = Format { version: Version::R2018, maintenance: 0, codec: TextCodec::Utf16 };
        let mut object = open_object(&bytes, 0, format).unwrap();
        assert_eq!(object.streams.tv().unwrap(), text);
        assert_eq!(object.streams.tv().unwrap(), "終わり");
    }
}
