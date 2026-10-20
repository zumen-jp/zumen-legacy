//! 値をすべて読み、同じビット列に書き戻す（ヘッダ変数・オブジェクト）。読み取り（document.rs など）が使わない値も
//! 持つので、読んだ図面の値を変えて書き直す（その場の編集）と、新しい図面を AutoCAD の図面と同じ並びで書く
//! （実物の図面を読んで書くと同じビット列になることで並びを確かめる）のに使う

pub(crate) mod header;
pub(crate) mod object;
pub(crate) mod tables;

use crate::bits::{Bits, HandleRef, Read};
use crate::file::Version;
use crate::write::bits::BitWriter;
use encoding_rs::Encoding;

/// 文字列（TV）をそのままの形で持つ。R2007 より前は図面の文字コードのバイト（終わりの NUL を含むことがある）、
/// R2007 以降は UTF-16 の単位
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawText {
    Bytes(Vec<u8>),
    Units(Vec<u16>),
}

impl RawText {
    /// 版の形にする。R2007 より前は encoding のバイト（表せない文字は ? になる）
    pub fn new(text: &str, version: Version, encoding: &'static Encoding) -> Self {
        if version >= Version::R2007 {
            Self::Units(text.encode_utf16().collect())
        } else {
            Self::Bytes(encoding.encode(text).0.into_owned())
        }
    }

    pub fn empty(version: Version) -> Self {
        if version >= Version::R2007 { Self::Units(Vec::new()) } else { Self::Bytes(Vec::new()) }
    }

    /// 文字列（NUL は除く）
    pub fn text(&self, encoding: &'static Encoding) -> String {
        let text = match self {
            Self::Bytes(bytes) => encoding.decode_without_bom_handling(bytes).0.into_owned(),
            Self::Units(units) => String::from_utf16_lossy(units),
        };
        if text.contains('\0') { text.replace('\0', "") } else { text }
    }
}

/// CMC（2.11）。R2004 より前は色の番号だけ、R2004 以降は番号（ふつう 0）・RGB の値（上位 1 バイトが種類）・
/// 名前の印と、色の名前・色見本の名前
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CmColor {
    pub index: i16,
    pub rgb: u32,
    pub name: Option<RawText>,
    pub book: Option<RawText>,
}

impl CmColor {
    /// BYLAYER（R2004 以降は 0xC0000000、それより前は 256）
    pub fn by_layer() -> Self {
        Self { index: 256, rgb: 0xc000_0000, name: None, book: None }
    }

    /// BYBLOCK
    pub fn by_block() -> Self {
        Self { index: 0, rgb: 0xc100_0000, name: None, book: None }
    }

    /// ACI の色（1〜255）
    pub fn aci(index: i16) -> Self {
        Self { index, rgb: 0xc300_0000 | (index as u32 & 0xff), name: None, book: None }
    }
}

/// 値・文字列・ハンドルの読む並び
pub(crate) struct In<'a> {
    pub data: Bits<'a>,
    /// R2007 以降の文字列の並び。None なら文字列は値の並びの中（R2007 より前）か、空
    pub strings: Option<Bits<'a>>,
    /// ハンドルの並び。None なら値の並びの中（R2007 より前のヘッダ変数）
    pub handles: Option<Bits<'a>>,
    pub version: Version,
}

impl In<'_> {
    pub fn r2004(&self) -> bool {
        self.version >= Version::R2004
    }

    pub fn r2007(&self) -> bool {
        self.version >= Version::R2007
    }

    /// TV をそのままの形で読む。R2007 以降は文字列の並びの BS の文字の数と UTF-16、それより前は BS のバイト数とバイト
    pub fn text(&mut self) -> Read<RawText> {
        if self.r2007() {
            let Some(strings) = &mut self.strings else { return Ok(RawText::Units(Vec::new())) };
            let length = strings.bs()?.max(0) as usize;
            let mut units = Vec::with_capacity(length.min(4096));
            for _ in 0..length {
                units.push(strings.rs()?);
            }
            Ok(RawText::Units(units))
        } else {
            let length = self.data.bs()?.max(0) as usize;
            Ok(RawText::Bytes(self.data.bytes(length)?))
        }
    }

    pub fn handle(&mut self) -> Read<HandleRef> {
        match &mut self.handles {
            Some(handles) => handles.handle(),
            None => self.data.handle(),
        }
    }

    pub fn cmc(&mut self) -> Read<CmColor> {
        let index = self.data.bs()?;
        if !self.r2004() {
            return Ok(CmColor { index, rgb: 0, name: None, book: None });
        }
        let rgb = self.data.bl()? as u32;
        let flags = self.data.rc()?;
        let name = if flags & 1 != 0 { Some(self.text()?) } else { None };
        let book = if flags & 2 != 0 { Some(self.text()?) } else { None };
        Ok(CmColor { index, rgb, name, book })
    }
}

/// 値・文字列・ハンドルの書く並び
pub(crate) struct Out {
    pub data: BitWriter,
    pub strings: BitWriter,
    pub handles: BitWriter,
    pub version: Version,
    /// ハンドルを値の並びに書く（R2007 より前のヘッダ変数）
    pub inline_handles: bool,
}

impl Out {
    pub fn new(version: Version, inline_handles: bool) -> Self {
        Self {
            data: BitWriter::new(),
            strings: BitWriter::new(),
            handles: BitWriter::new(),
            version,
            inline_handles,
        }
    }

    pub fn r2004(&self) -> bool {
        self.version >= Version::R2004
    }

    pub fn r2007(&self) -> bool {
        self.version >= Version::R2007
    }

    pub fn text(&mut self, text: &RawText) {
        match text {
            RawText::Units(units) => {
                let target = if self.r2007() { &mut self.strings } else { &mut self.data };
                target.bs(units.len().min(i16::MAX as usize) as i16);
                for unit in units.iter().take(i16::MAX as usize) {
                    target.rs(*unit);
                }
            }
            RawText::Bytes(bytes) => {
                self.data.bs(bytes.len().min(i16::MAX as usize) as i16);
                self.data.raw(&bytes[..bytes.len().min(i16::MAX as usize)]);
            }
        }
    }

    pub fn handle(&mut self, handle: HandleRef) {
        let target = if self.inline_handles { &mut self.data } else { &mut self.handles };
        target.handle(handle.code, handle.value);
    }

    pub fn cmc(&mut self, color: &CmColor) {
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
}
