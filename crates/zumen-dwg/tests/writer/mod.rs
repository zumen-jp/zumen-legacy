//! テストの図面を書く最小の DWG の書き手（R2000 と R2018）。読み取りが前提とする並びだけを書き、CRC は 0 にする
//! （読み取りは CRC を確かめない）。Open Design Specification for .dwg files 5.4.1 の節の番号は読み取り側の注記を見る。
#![allow(dead_code)]

use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Version {
    R2000,
    R2018,
}

impl Version {
    pub fn magic(self) -> &'static [u8; 6] {
        match self {
            Self::R2000 => b"AC1015",
            Self::R2018 => b"AC1032",
        }
    }

    /// R2007 以降の書き方（文字列の並び・UTF-16）。R2010 以降の型（OT）とハンドルの並びの大きさ（MC）もこれで決める
    fn r2018(self) -> bool {
        self == Self::R2018
    }
}

/// 上位のビットから詰めるビットの並び
#[derive(Default, Clone)]
pub struct Bits {
    pub bytes: Vec<u8>,
    pub len: u64,
}

impl Bits {
    pub fn bit(&mut self, value: bool) {
        if self.len.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if value {
            let last = self.bytes.len() - 1;
            self.bytes[last] |= 0x80 >> (self.len % 8);
        }
        self.len += 1;
    }

    pub fn bits(&mut self, value: u64, count: u32) {
        for index in (0..count).rev() {
            self.bit(value >> index & 1 == 1);
        }
    }

    pub fn append(&mut self, other: &Bits) {
        for index in 0..other.len {
            self.bit(other.bytes[(index / 8) as usize] & (0x80 >> (index % 8)) != 0);
        }
    }

    pub fn rc(&mut self, value: u8) {
        self.bits(u64::from(value), 8);
    }

    pub fn rs(&mut self, value: u16) {
        for byte in value.to_le_bytes() {
            self.rc(byte);
        }
    }

    pub fn rl(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            self.rc(byte);
        }
    }

    pub fn rd(&mut self, value: f64) {
        for byte in value.to_le_bytes() {
            self.rc(byte);
        }
    }

    pub fn bb(&mut self, value: u8) {
        self.bits(u64::from(value), 2);
    }

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

    pub fn bd(&mut self, value: f64) {
        if value == 0.0 && value.is_sign_positive() {
            self.bb(2);
        } else if value == 1.0 {
            self.bb(1);
        } else {
            self.bb(0);
            self.rd(value);
        }
    }

    pub fn bd2(&mut self, value: [f64; 2]) {
        self.bd(value[0]);
        self.bd(value[1]);
    }

    pub fn bd3(&mut self, value: [f64; 3]) {
        for v in value {
            self.bd(v);
        }
    }

    pub fn rd2(&mut self, value: [f64; 2]) {
        self.rd(value[0]);
        self.rd(value[1]);
    }

    /// DD（2.9）。既定の値と同じなら 00、ほかは 11 と RD
    pub fn dd(&mut self, value: f64, default: f64) {
        if value.to_bits() == default.to_bits() {
            self.bb(0);
        } else {
            self.bb(3);
            self.rd(value);
        }
    }

    /// BLL。バイト数（3 ビット）と、下位のバイトから
    pub fn bll(&mut self, value: u64) {
        let bytes = value.to_le_bytes();
        let length = bytes.iter().rposition(|byte| *byte != 0).map_or(0, |at| at + 1);
        self.bits(length as u64, 3);
        for byte in &bytes[..length] {
            self.rc(*byte);
        }
    }

    /// H（2.13）。CODE・COUNTER と上位のバイトから
    pub fn handle(&mut self, code: u8, value: u64) {
        let bytes = value.to_be_bytes();
        let skip = bytes.iter().position(|byte| *byte != 0).unwrap_or(8);
        self.rc((code << 4) | (8 - skip) as u8);
        for byte in &bytes[skip..] {
            self.rc(*byte);
        }
    }

    /// BE。R2000 以降は (0, 0, 1) なら 1 ビット
    pub fn be(&mut self, value: [f64; 3]) {
        if value == [0.0, 0.0, 1.0] {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd3(value);
        }
    }

    /// BT。R2000 以降は 0 なら 1 ビット
    pub fn bt(&mut self, value: f64) {
        if value == 0.0 {
            self.bit(true);
        } else {
            self.bit(false);
            self.bd(value);
        }
    }

    /// OT（2.12）。R2010 より前は BS
    pub fn ot(&mut self, value: u16, r2010: bool) {
        if !r2010 {
            self.bs(value as i16);
        } else if value < 256 {
            self.bb(0);
            self.rc(value as u8);
        } else if (0x1f0..0x1f0 + 256).contains(&value) {
            self.bb(1);
            self.rc((value - 0x1f0) as u8);
        } else {
            self.bb(2);
            self.rs(value);
        }
    }

    pub fn pad(&mut self) {
        while !self.len.is_multiple_of(8) {
            self.bit(false);
        }
    }
}

/// 文字列を R2007 より前は図面の文字コード（Shift_JIS）、R2007 以降は UTF-16 の並びで書く
pub fn text_bytes(version: Version, text: &str) -> (i16, Vec<u8>) {
    if version.r2018() {
        let units: Vec<u16> = text.encode_utf16().collect();
        (units.len() as i16, units.iter().flat_map(|unit| unit.to_le_bytes()).collect())
    } else {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(text);
        (bytes.len() as i16, bytes.into_owned())
    }
}

/// 1 つのオブジェクト。値・文字列・ハンドルの並びを分けて書き、最後に版の形にまとめる
pub struct Obj {
    pub version: Version,
    pub kind: u16,
    pub handle: u64,
    pub data: Bits,
    pub strings: Bits,
    pub handles: Bits,
}

impl Obj {
    pub fn new(version: Version, kind: u16, handle: u64) -> Self {
        Self { version, kind, handle, data: Bits::default(), strings: Bits::default(), handles: Bits::default() }
    }

    /// TV。R2007 以降は文字列の並びに書く
    pub fn tv(&mut self, text: &str) {
        let (length, bytes) = text_bytes(self.version, text);
        let target = if self.version.r2018() { &mut self.strings } else { &mut self.data };
        target.bs(length);
        for byte in bytes {
            target.rc(byte);
        }
    }

    pub fn h(&mut self, code: u8, value: u64) {
        self.handles.handle(code, value);
    }

    /// CMC（2.11）。R2004 以降は BS（0）・BL（0xC3 と ACI、0xC2 と RGB、0xC0 BYLAYER、0xC1 BYBLOCK）・RC
    pub fn cmc(&mut self, index: i16, rgb: Option<u32>) {
        if self.version.r2018() {
            self.data.bs(0);
            let raw = match (index, rgb) {
                (_, Some(rgb)) => 0xc200_0000 | rgb,
                (256, None) => 0xc000_0000,
                (0, None) => 0xc100_0000,
                (index, None) => 0xc300_0000 | index as u32,
            };
            self.data.bl(raw as i32);
            self.data.rc(0);
        } else {
            self.data.bs(index);
        }
    }

    /// 図形でないオブジェクトの共通の値（20.1）。リアクタなし、R2018 は拡張辞書なし
    pub fn non_entity(&mut self, owner: u64) {
        self.data.bl(0);
        if self.version.r2018() {
            // XDic Missing Flag と（R2013 以降）Has DS binary data
            self.data.bit(true);
            self.data.bit(false);
        }
        self.h(4, owner);
        if !self.version.r2018() {
            self.h(3, 0);
        }
    }

    /// 図形の共通の値（20.4.1、20.4.2）。色は ACI、線種・印刷スタイル・マテリアルは BYLAYER。owner は図形のモード 00
    /// （ブロックの中）の持ち主。R2000 の前後の図形は links（なければ Nolinks = 1 でハンドル ±1）
    pub fn entity(&mut self, layer: u64, color: i16, links: Option<(u64, u64)>, owner: Option<u64>) {
        self.data.bit(false);
        match owner {
            Some(owner) => {
                self.data.bb(0);
                self.h(4, owner);
            }
            None => self.data.bb(2),
        }
        self.data.bl(0);
        if self.version.r2018() {
            self.data.bit(true);
            self.data.bit(false);
        } else {
            self.h(3, 0);
            match links {
                Some((previous, next)) => {
                    self.data.bit(false);
                    self.h(4, previous);
                    self.h(4, next);
                }
                None => self.data.bit(true),
            }
        }
        self.data.bs(color);
        self.data.bd(1.0);
        self.h(5, layer);
        self.data.bb(0);
        self.data.bb(0);
        if self.version.r2018() {
            self.data.bb(0);
            self.data.rc(0);
            self.data.bit(false);
            self.data.bit(false);
            self.data.bit(false);
        }
        self.data.bs(0);
        self.data.rc(29);
    }

    /// 版の形にまとめる。MS の大きさ、（R2010 以降）ハンドルの並びのビット数、型、（R2000〜R2007）ハンドルの位置、
    /// ハンドル、拡張データ（なし）、値、（R2007 以降）文字列の並び、ハンドルの並び、CRC
    pub fn bytes(&self) -> Vec<u8> {
        let r2010 = self.version.r2018();
        let mut kind = Bits::default();
        kind.ot(self.kind, r2010);
        let mut rest = Bits::default();
        rest.handle(0, self.handle);
        rest.bs(0);
        rest.append(&self.data);
        if r2010 {
            // 文字列の並び、その大きさ（RS）、ある印（B）（20.1）
            if self.strings.len > 0 {
                rest.append(&self.strings);
                rest.rs(self.strings.len as u16);
                rest.bit(true);
            } else {
                rest.bit(false);
            }
        }
        let mut body = kind.clone();
        if !r2010 {
            // ハンドルの並びの位置は、型の始まりからのビット数
            body.rl((kind.len + 32 + rest.len) as u32);
        }
        body.append(&rest);
        let data_bits = body.len;
        body.append(&self.handles);
        body.pad();
        let handle_bits = body.len - data_bits;
        let mut out = Bits::default();
        let mut size = body.bytes.len() as u64;
        loop {
            let word = (size & 0x7fff) as u16;
            size >>= 15;
            out.rs(if size > 0 { word | 0x8000 } else { word });
            if size == 0 {
                break;
            }
        }
        if r2010 {
            let mut value = handle_bits;
            loop {
                let byte = (value & 0x7f) as u8;
                value >>= 7;
                out.rc(if value > 0 { byte | 0x80 } else { byte });
                if value == 0 {
                    break;
                }
            }
        }
        let mut bytes = out.bytes;
        bytes.extend_from_slice(&body.bytes);
        bytes.extend_from_slice(&[0, 0]);
        bytes
    }
}

/// ヘッダ変数（9）に書く値と表のハンドル
#[derive(Clone)]
pub struct Header {
    pub insunits: i16,
    pub ltscale: f64,
    pub tilemode: bool,
    pub block_control: u64,
    pub layer_control: u64,
    pub style_control: u64,
    pub linetype_control: u64,
    pub appid_control: u64,
    pub dimstyle_control: u64,
    pub mlinestyle_dictionary: u64,
    pub named_objects: u64,
    pub layouts: u64,
    pub paper_space: u64,
    pub model_space: u64,
    pub bylayer: u64,
    pub byblock: u64,
    pub continuous: u64,
}

pub const HEADER_SENTINEL: [u8; 16] =
    [0xcf, 0x7b, 0x1f, 0x23, 0xfd, 0xde, 0x38, 0xa9, 0x5f, 0x7c, 0x68, 0xb8, 0x4e, 0x6d, 0x33, 0x5f];
pub const HEADER_END: [u8; 16] =
    [0x30, 0x84, 0xe0, 0xdc, 0x02, 0x21, 0xc7, 0x56, 0xa0, 0x83, 0x97, 0x47, 0xb1, 0x92, 0xcc, 0xa0];
pub const CLASSES_SENTINEL: [u8; 16] =
    [0x8d, 0xa1, 0xc4, 0xb8, 0xc4, 0xa9, 0xf8, 0xc5, 0xc0, 0xdc, 0xf4, 0x5f, 0xe7, 0xcf, 0xb6, 0x8a];
pub const CLASSES_END: [u8; 16] =
    [0x72, 0x5e, 0x3b, 0x47, 0x3b, 0x56, 0x07, 0x3a, 0x3f, 0x23, 0x0b, 0xa0, 0x18, 0x30, 0x49, 0x75];

/// ヘッダ変数の節（区切り・大きさ・値・CRC・区切り）。値は読み取り（header.rs）と同じ並びで、多くは 0
pub fn header_section(version: Version, h: &Header) -> Vec<u8> {
    let r2018 = version.r2018();
    let mut o = Obj::new(version, 0, 0);
    // R2013 以降の REQUIREDVERSIONS
    if r2018 {
        o.data.bll(0);
    }
    for value in [412148564080.0, 1.0, 1.0, 1.0] {
        o.data.bd(value);
    }
    for _ in 0..4 {
        o.tv("");
    }
    o.data.bl(24);
    o.data.bl(0);
    // R2007 より前はハンドルも値の並びの中
    let handle = |o: &mut Obj, code: u8, value: u64| {
        if r2018 { o.handles.handle(code, value) } else { o.data.handle(code, value) }
    };
    if !r2018 {
        handle(&mut o, 5, 0);
    }
    for _ in 0..7 {
        o.data.bit(false);
    }
    // PSLTSCALE、LIMCHECK、（R2004 以降）不明
    o.data.bit(true);
    o.data.bit(false);
    if r2018 {
        o.data.bit(false);
    }
    for _ in 0..6 {
        o.data.bit(false);
    }
    o.data.bit(h.tilemode);
    for _ in 0..4 {
        o.data.bit(false);
    }
    for _ in 0..8 {
        o.data.bs(0);
    }
    if r2018 {
        for _ in 0..3 {
            o.data.bl(0);
        }
    }
    for _ in 0..19 {
        o.data.bs(0);
    }
    o.data.bd(h.ltscale);
    o.data.bd(2.5);
    for _ in 0..19 {
        o.data.bd(0.0);
    }
    o.tv("");
    for _ in 0..4 {
        o.data.bl(0);
    }
    if r2018 {
        for _ in 0..3 {
            o.data.bl(0);
        }
    }
    for _ in 0..4 {
        o.data.bl(0);
    }
    o.cmc(256, None);
    // HANDSEED はつねに値の並び
    o.data.handle(0, 0x1000);
    for _ in 0..if r2018 { 6 } else { 5 } {
        handle(&mut o, 5, 0);
    }
    o.data.bd(1.0);
    // 紙空間: INSBASE、EXTMIN、EXTMAX、LIMMIN、LIMMAX、ELEVATION、UCS
    o.data.bd3([0.0; 3]);
    o.data.bd3([0.0; 3]);
    o.data.bd3([0.0; 3]);
    o.data.rd2([0.0, 0.0]);
    o.data.rd2([420.0, 297.0]);
    o.data.bd(0.0);
    for _ in 0..3 {
        o.data.bd3([0.0; 3]);
    }
    handle(&mut o, 5, 0);
    handle(&mut o, 5, 0);
    o.data.bs(0);
    handle(&mut o, 5, 0);
    for _ in 0..6 {
        o.data.bd3([0.0; 3]);
    }
    // モデル空間
    o.data.bd3([0.0; 3]);
    o.data.bd3([0.0; 3]);
    o.data.bd3([100.0, 100.0, 0.0]);
    o.data.rd2([0.0, 0.0]);
    o.data.rd2([420.0, 297.0]);
    o.data.bd(0.0);
    for _ in 0..3 {
        o.data.bd3([0.0; 3]);
    }
    handle(&mut o, 5, 0);
    handle(&mut o, 5, 0);
    o.data.bs(0);
    handle(&mut o, 5, 0);
    for _ in 0..6 {
        o.data.bd3([0.0; 3]);
    }
    o.tv("");
    o.tv("");
    // DIMSCALE、DIMASZ と 7 つの BD
    o.data.bd(1.0);
    o.data.bd(2.5);
    for _ in 0..7 {
        o.data.bd(0.0);
    }
    if r2018 {
        o.data.bd(0.0);
        o.data.bd(0.0);
        o.data.bs(0);
        o.cmc(0, None);
    }
    for _ in 0..6 {
        o.data.bit(false);
    }
    for _ in 0..3 {
        o.data.bs(0);
    }
    if r2018 {
        o.data.bs(0);
    }
    // DIMTXT と 8 つの BD
    o.data.bd(2.5);
    for _ in 0..8 {
        o.data.bd(0.0);
    }
    o.data.bit(false);
    o.data.bs(0);
    for _ in 0..4 {
        o.data.bit(false);
    }
    for _ in 0..3 {
        o.cmc(0, None);
    }
    for _ in 0..11 {
        o.data.bs(0);
    }
    o.data.bit(false);
    o.data.bit(false);
    for _ in 0..4 {
        o.data.bs(0);
    }
    o.data.bit(false);
    o.data.bs(0);
    if r2018 {
        // R2007 以降の DIMFXLON、R2010 以降の DIMTXTDIRECTION・DIMALTMZF・DIMALTMZS・DIMMZF・DIMMZS
        o.data.bit(false);
        o.data.bit(false);
        o.data.bd(0.0);
        o.tv("");
        o.data.bd(0.0);
        o.tv("");
    }
    for _ in 0..if r2018 { 8 } else { 5 } {
        handle(&mut o, 5, 0);
    }
    o.data.bs(0);
    o.data.bs(0);
    handle(&mut o, 3, h.block_control);
    handle(&mut o, 3, h.layer_control);
    handle(&mut o, 3, h.style_control);
    handle(&mut o, 3, h.linetype_control);
    for _ in 0..3 {
        handle(&mut o, 3, 0);
    }
    handle(&mut o, 3, h.appid_control);
    handle(&mut o, 3, h.dimstyle_control);
    if !r2018 {
        handle(&mut o, 3, 0);
    }
    handle(&mut o, 5, 0);
    handle(&mut o, 5, h.mlinestyle_dictionary);
    handle(&mut o, 3, h.named_objects);
    o.data.bs(1);
    o.data.bs(70);
    o.tv("");
    o.tv("");
    handle(&mut o, 5, h.layouts);
    for _ in 0..if r2018 { 6 } else { 2 } {
        handle(&mut o, 5, 0);
    }
    o.data.bl(0);
    o.data.bs(h.insunits);
    o.data.bs(0);
    o.tv("");
    o.tv("");
    if r2018 {
        for _ in 0..6 {
            o.data.rc(0);
        }
        o.data.bs(0);
        o.data.bs(0);
        o.data.rc(0);
        o.data.rc(0);
        o.tv("");
    }
    handle(&mut o, 5, h.paper_space);
    handle(&mut o, 5, h.model_space);
    handle(&mut o, 5, h.bylayer);
    handle(&mut o, 5, h.byblock);
    handle(&mut o, 5, h.continuous);
    // 並びをまとめる（R2007 以降は RL のビット数、値、文字列、その大きさ、印、ハンドル）
    let mut body = Bits::default();
    if r2018 {
        let mut data = o.data.clone();
        if o.strings.len > 0 {
            data.append(&o.strings);
            data.rs(o.strings.len as u16);
            data.bit(true);
        } else {
            data.bit(false);
        }
        body.rl((data.len + 32) as u32);
        body.append(&data);
        body.append(&o.handles);
    } else {
        body.append(&o.data);
    }
    body.pad();
    section(&HEADER_SENTINEL, &body.bytes, version, &HEADER_END)
}

/// 区切り・大きさ（RL）・（R2018 は RL の 0）・値・CRC・区切り
fn section(start: &[u8; 16], body: &[u8], version: Version, end: &[u8; 16]) -> Vec<u8> {
    let mut out = start.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    if version == Version::R2018 {
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    out.extend_from_slice(body);
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(end);
    out
}

/// クラスの節（10）。classes は（番号、DXF の名前、図形か）
pub fn classes_section(version: Version, classes: &[(u16, &str, bool)]) -> Vec<u8> {
    let mut o = Obj::new(version, 0, 0);
    for &(number, name, entity) in classes {
        o.data.bs(number as i16);
        o.data.bs(0);
        o.tv("ObjectDBX Classes");
        o.tv(name);
        o.tv(name);
        o.data.bit(false);
        o.data.bs(if entity { 0x1f2 } else { 0x1f3 });
        if version.r2018() {
            for _ in 0..5 {
                o.data.bl(0);
            }
        }
    }
    let mut body = Bits::default();
    if version.r2018() {
        let mut data = Bits::default();
        data.bl(0);
        data.bit(true);
        data.append(&o.data);
        data.append(&o.strings);
        data.rs(o.strings.len as u16);
        data.bit(true);
        body.rl((data.len + 32) as u32);
        body.append(&data);
    } else {
        body.append(&o.data);
    }
    body.pad();
    section(&CLASSES_SENTINEL, &body.bytes, version, &CLASSES_END)
}

/// オブジェクトの表（23）。2032 バイトまでの区切りに、ハンドルと位置の差を MC で並べる。区切りごとに差は 0 から
pub fn object_map(locations: &BTreeMap<u64, usize>) -> Vec<u8> {
    fn item(handle: u64, location: i64) -> Vec<u8> {
        let mut out = Vec::new();
        let mut delta = handle;
        loop {
            let byte = (delta & 0x7f) as u8;
            delta >>= 7;
            out.push(if delta > 0 { byte | 0x80 } else { byte });
            if delta == 0 {
                break;
            }
        }
        let (negative, mut value) = (location < 0, location.unsigned_abs());
        loop {
            if value < 0x40 {
                out.push(value as u8 | if negative { 0x40 } else { 0 });
                break;
            }
            out.push((value & 0x7f) as u8 | 0x80);
            value >>= 7;
        }
        out
    }
    let mut out = Vec::new();
    let mut chunk: Vec<u8> = Vec::new();
    let (mut last_handle, mut last_location) = (0u64, 0i64);
    for (&handle, &location) in locations {
        let mut bytes = item(handle - last_handle, location as i64 - last_location);
        if chunk.len() + bytes.len() > 2032 {
            out.extend_from_slice(&((chunk.len() + 2) as u16).to_be_bytes());
            out.append(&mut chunk);
            out.extend_from_slice(&[0, 0]);
            bytes = item(handle, location as i64);
        }
        chunk.extend_from_slice(&bytes);
        last_handle = handle;
        last_location = location as i64;
    }
    if !chunk.is_empty() {
        out.extend_from_slice(&((chunk.len() + 2) as u16).to_be_bytes());
        out.append(&mut chunk);
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 2, 0, 0]);
    out
}

/// 図面の全体。objects はハンドルとオブジェクトのバイト列
pub struct Drawing {
    pub version: Version,
    pub codepage: u16,
    pub header: Header,
    pub classes: Vec<(u16, &'static str, bool)>,
    pub objects: Vec<(u64, Vec<u8>)>,
    pub measurement: u16,
}

impl Drawing {
    pub fn bytes(&self) -> Vec<u8> {
        match self.version {
            Version::R2000 => self.r2000(),
            Version::R2018 => self.r2018(),
        }
    }

    /// R2000 の入れ物（3）。ファイルの頭と節の位置、ヘッダ変数、クラス、オブジェクト、オブジェクトの表、MEASUREMENT
    fn r2000(&self) -> Vec<u8> {
        let header = header_section(self.version, &self.header);
        let classes = classes_section(self.version, &self.classes);
        let records = 5usize;
        let start = 0x19 + records * 9 + 2 + 16;
        let mut out = vec![0u8; start];
        out[0..6].copy_from_slice(self.version.magic());
        out[0x0c] = 1;
        out[0x13..0x15].copy_from_slice(&self.codepage.to_le_bytes());
        out[0x15..0x19].copy_from_slice(&(records as u32).to_le_bytes());
        let header_at = out.len();
        out.extend_from_slice(&header);
        let classes_at = out.len();
        out.extend_from_slice(&classes);
        let mut locations = BTreeMap::new();
        for (handle, bytes) in &self.objects {
            locations.insert(*handle, out.len());
            out.extend_from_slice(bytes);
        }
        let map_at = out.len();
        let map = object_map(&locations);
        out.extend_from_slice(&map);
        let template_at = out.len();
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&self.measurement.to_le_bytes());
        let list = [
            (0u8, header_at, header.len()),
            (1, classes_at, classes.len()),
            (2, map_at, map.len()),
            (3, 0, 0),
            (4, template_at, 4),
        ];
        for (index, (number, at, size)) in list.iter().enumerate() {
            let base = 0x19 + index * 9;
            out[base] = *number;
            out[base + 1..base + 5].copy_from_slice(&(*at as u32).to_le_bytes());
            out[base + 5..base + 9].copy_from_slice(&(*size as u32).to_le_bytes());
        }
        out
    }

    /// R2004 以降の入れ物（4）。節は 1 つのページずつ、文字だけの圧縮で書く
    fn r2018(&self) -> Vec<u8> {
        let header = header_section(self.version, &self.header);
        let classes = classes_section(self.version, &self.classes);
        // AcDbObjects の始まりは RL の 0x0DCA（20）
        let mut objects = 0x0dcau32.to_le_bytes().to_vec();
        let mut locations = BTreeMap::new();
        for (handle, bytes) in &self.objects {
            locations.insert(*handle, objects.len());
            objects.extend_from_slice(bytes);
        }
        let map = object_map(&locations);
        let mut template = 0u16.to_le_bytes().to_vec();
        template.extend_from_slice(&self.measurement.to_le_bytes());
        let sections: Vec<(&str, Vec<u8>)> = vec![
            ("AcDb:Header", header),
            ("AcDb:Classes", classes),
            ("AcDb:Handles", map),
            ("AcDb:AcDbObjects", objects),
            ("AcDb:Template", template),
        ];
        let mut out = vec![0u8; 0x100];
        out[0..6].copy_from_slice(self.version.magic());
        out[0x0c] = 3;
        out[0x13..0x15].copy_from_slice(&self.codepage.to_le_bytes());
        // データのページ（4.6）。32 バイトの頭は 0x4164536b ^ 位置 で隠す
        let mut pages: Vec<(i32, usize)> = Vec::new();
        let mut compressed_sizes: Vec<usize> = Vec::new();
        let mut descriptions: Vec<(String, usize, i32)> = Vec::new();
        for (index, (name, data)) in sections.iter().enumerate() {
            let number = index as i32 + 1;
            let address = out.len();
            let compressed = compress_literals(data);
            let mut head = [0u32; 8];
            head[0] = 0x4163_043b;
            head[1] = index as u32 + 1;
            head[2] = compressed.len() as u32;
            head[3] = 0x7400;
            let mask = 0x4164_536b_u32 ^ address as u32;
            for value in head {
                out.extend_from_slice(&(value ^ mask).to_le_bytes());
            }
            out.extend_from_slice(&compressed);
            while !out.len().is_multiple_of(0x20) {
                out.push(0);
            }
            compressed_sizes.push(compressed.len());
            pages.push((number, out.len() - address));
            descriptions.push((name.to_string(), data.len(), number));
        }
        // 節の表（4.5）
        let mut map = Vec::new();
        map.extend_from_slice(&(descriptions.len() as u32).to_le_bytes());
        for value in [2u32, 0x7400, 0, descriptions.len() as u32] {
            map.extend_from_slice(&value.to_le_bytes());
        }
        for (index, (name, size, page)) in descriptions.iter().enumerate() {
            map.extend_from_slice(&(*size as u64).to_le_bytes());
            for value in [1u32, 0x7400, 0, 2, index as u32 + 1, 0] {
                map.extend_from_slice(&value.to_le_bytes());
            }
            let mut field = [0u8; 64];
            field[..name.len()].copy_from_slice(name.as_bytes());
            map.extend_from_slice(&field);
            map.extend_from_slice(&(*page as u32).to_le_bytes());
            map.extend_from_slice(&(compressed_sizes[*page as usize - 1] as u32).to_le_bytes());
            map.extend_from_slice(&0u64.to_le_bytes());
        }
        let section_map_id = pages.len() as i32 + 1;
        let section_map_at = out.len();
        system_page(&mut out, 0x4163_003b, &map);
        pages.push((section_map_id, out.len() - section_map_at));
        // ページの表（4.4）
        let page_map_id = section_map_id + 1;
        let mut page_map = Vec::new();
        for (number, size) in &pages {
            page_map.extend_from_slice(&number.to_le_bytes());
            page_map.extend_from_slice(&(*size as u32).to_le_bytes());
        }
        let page_map_at = out.len();
        system_page(&mut out, 0x4163_0e3b, &page_map);
        // 0x80 の隠したファイルの頭（4.1）
        let mut data = vec![0u8; 0x6c];
        data[0..11].copy_from_slice(b"AcFssFcAJMB");
        data[0x50..0x54].copy_from_slice(&(page_map_id as u32).to_le_bytes());
        data[0x54..0x5c].copy_from_slice(&((page_map_at - 0x100) as u64).to_le_bytes());
        data[0x5c..0x60].copy_from_slice(&(section_map_id as u32).to_le_bytes());
        let mut seed: u32 = 1;
        for byte in &mut data {
            seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
            *byte ^= (seed >> 16) as u8;
        }
        out[0x80..0x80 + 0x6c].copy_from_slice(&data);
        out
    }
}

/// システムのページ（4.3）。0x14 バイトの頭と、文字だけの圧縮
fn system_page(out: &mut Vec<u8>, kind: u32, data: &[u8]) {
    let compressed = compress_literals(data);
    for value in [kind, data.len() as u32, compressed.len() as u32, 2, 0] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&compressed);
    while !out.len().is_multiple_of(0x20) {
        out.push(0);
    }
}

/// 4.7 の圧縮の形で、すべてを文字として書く（最初の文字の長さと 0x11）
pub fn compress_literals(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let length = data.len();
    if length == 0 {
    } else if length <= 3 {
        out.push(length as u8 + 0x11);
    } else if length <= 0x12 {
        out.push((length - 3) as u8);
    } else {
        out.push(0);
        let mut rest = length - 3 - 0x0f;
        while rest > 0xff {
            out.push(0);
            rest -= 0xff;
        }
        out.push(rest as u8);
    }
    out.extend_from_slice(data);
    out.push(0x11);
    out
}

// --- 表・ブロック・図形を作る関数（読み取りの document.rs・entities.rs と同じ並び） ---------------------------

/// 表の項目の名前と外部参照のビット（R2007 以降は BS、それより前は B・BS・B）
fn entry_name(o: &mut Obj, name: &str) {
    o.tv(name);
    if o.version.r2018() {
        o.data.bs(0);
    } else {
        o.data.bit(false);
        o.data.bs(0);
        o.data.bit(false);
    }
}

/// 表の管理のオブジェクト（20.4.51 など）。項目の後に extra のハンドル（BLOCK_CONTROL のモデル・紙空間など）
pub fn control(version: Version, kind: u16, handle: u64, entries: &[u64], extra: &[u64]) -> Vec<u8> {
    let mut o = Obj::new(version, kind, handle);
    o.non_entity(0);
    o.data.bl(entries.len() as i32);
    for entry in entries {
        o.h(2, *entry);
    }
    for entry in extra {
        o.h(3, *entry);
    }
    o.bytes()
}

/// LAYER（20.4.54）。off は非表示（BS の 2 のビット）
pub fn layer(version: Version, handle: u64, control: u64, name: &str, color: i16, off: bool, linetype: u64) -> Vec<u8> {
    let mut o = Obj::new(version, 51, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    // 印刷する（16）と、線の太さ（既定 31）
    o.data.bs(16 | if off { 2 } else { 0 } | (31 << 5));
    o.cmc(color, None);
    o.h(5, 0);
    o.h(5, 0);
    if version.r2018() {
        o.h(5, 0);
    }
    o.h(5, linetype);
    if version.r2018() {
        o.h(5, 0);
    }
    o.bytes()
}

/// LTYPE（20.4.58）
pub fn linetype(version: Version, handle: u64, control: u64, name: &str, description: &str, dashes: &[f64]) -> Vec<u8> {
    let mut o = Obj::new(version, 57, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    o.tv(description);
    o.data.bd(dashes.iter().map(|dash| dash.abs()).sum());
    o.data.rc(b'A');
    o.data.rc(dashes.len() as u8);
    for dash in dashes {
        o.data.bd(*dash);
        o.data.bs(0);
        o.data.rd2([0.0, 0.0]);
        o.data.bd(1.0);
        o.data.bd(0.0);
        o.data.bs(0);
    }
    if !version.r2018() {
        for _ in 0..256 {
            o.data.rc(0);
        }
    }
    o.h(5, 0);
    for _ in dashes {
        o.h(5, 0);
    }
    o.bytes()
}

/// STYLE（20.4.56）
pub fn style(version: Version, handle: u64, control: u64, name: &str, font: &str) -> Vec<u8> {
    let mut o = Obj::new(version, 53, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    o.data.bit(false);
    o.data.bit(false);
    o.data.bd(0.0);
    o.data.bd(1.0);
    o.data.bd(0.0);
    o.data.rc(0);
    o.data.bd(2.5);
    o.tv(font);
    o.tv("");
    o.h(5, 0);
    o.bytes()
}

/// DIMSTYLE（20.4.68）の、読み取りが使う DIMSCALE・DIMASZ・DIMTXT まで
pub fn dimstyle(version: Version, handle: u64, control: u64, name: &str, dimasz: f64) -> Vec<u8> {
    let mut o = Obj::new(version, 69, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    o.tv("");
    o.tv("");
    o.data.bd(1.0);
    o.data.bd(dimasz);
    for _ in 0..7 {
        o.data.bd(0.0);
    }
    if version.r2018() {
        o.data.bd(0.0);
        o.data.bd(0.0);
        o.data.bs(0);
        o.cmc(0, None);
    }
    for _ in 0..6 {
        o.data.bit(false);
    }
    for _ in 0..3 {
        o.data.bs(0);
    }
    if version.r2018() {
        o.data.bs(0);
    }
    o.data.bd(2.5);
    o.bytes()
}

/// ブロック（BLOCK_HEADER、20.4.52）。header_name は BLOCK_HEADER の名前（無名なら *D など）、entities は持つ図形
#[allow(clippy::too_many_arguments)]
pub fn block_header(
    version: Version,
    handle: u64,
    header_name: &str,
    anonymous: bool,
    block: u64,
    endblk: u64,
    entities: &[u64],
    layout: u64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 49, handle);
    o.non_entity(1);
    entry_name(&mut o, header_name);
    o.data.bit(anonymous);
    o.data.bit(false);
    o.data.bit(false);
    o.data.bit(false);
    o.data.bit(false);
    if version.r2018() {
        o.data.bl(entities.len() as i32);
    }
    o.data.bd3([0.0, 0.0, 0.0]);
    o.tv("");
    o.data.rc(0);
    o.tv("");
    o.data.bl(0);
    if version.r2018() {
        o.data.bs(0);
        o.data.bit(true);
        o.data.rc(1);
    }
    o.h(5, 0);
    o.h(3, block);
    if version.r2018() {
        for entity in entities {
            o.h(3, *entity);
        }
    } else {
        o.h(4, entities.first().copied().unwrap_or(0));
        o.h(4, entities.last().copied().unwrap_or(0));
    }
    o.h(3, endblk);
    o.h(5, layout);
    o.bytes()
}

/// BLOCK（4）。名前は無名のブロックの番号まで持つ（*D3 など）
pub fn block_entity(version: Version, handle: u64, record: u64, name: &str) -> Vec<u8> {
    let mut o = Obj::new(version, 4, handle);
    o.entity(0x10, 256, None, Some(record));
    o.tv(name);
    o.bytes()
}

/// ENDBLK（5）
pub fn endblk(version: Version, handle: u64, record: u64) -> Vec<u8> {
    let mut o = Obj::new(version, 5, handle);
    o.entity(0x10, 256, None, Some(record));
    o.bytes()
}

/// DICTIONARY（20.4.44）
pub fn dictionary(version: Version, handle: u64, owner: u64, entries: &[(&str, u64)]) -> Vec<u8> {
    let mut o = Obj::new(version, 42, handle);
    o.non_entity(owner);
    o.data.bl(entries.len() as i32);
    o.data.bs(0);
    o.data.rc(1);
    for (name, item) in entries {
        o.tv(name);
        o.h(2, *item);
    }
    o.bytes()
}

/// LAYOUT（20.4.84）と、その前の PLOTSETTINGS の値。用紙は mm、印刷の尺度は 1:1
pub fn layout(
    version: Version,
    handle: u64,
    owner: u64,
    name: &str,
    tab: i32,
    record: u64,
    paper: [f64; 2],
) -> Vec<u8> {
    let mut o = Obj::new(version, 82, handle);
    o.non_entity(owner);
    o.tv("");
    o.tv("");
    o.data.bs(0);
    for _ in 0..4 {
        o.data.bd(0.0);
    }
    o.data.bd(paper[0]);
    o.data.bd(paper[1]);
    o.tv("ISO_A3_(420.00_x_297.00_MM)");
    o.data.bd(0.0);
    o.data.bd(0.0);
    o.data.bs(1);
    o.data.bs(0);
    o.data.bs(5);
    for _ in 0..4 {
        o.data.bd(0.0);
    }
    if !version.r2018() {
        o.tv("");
    }
    o.data.bd(1.0);
    o.data.bd(1.0);
    o.tv("");
    o.data.bs(0);
    o.data.bd(1.0);
    o.data.bd2([0.0, 0.0]);
    if version.r2018() {
        o.data.bs(0);
        o.data.bs(0);
        o.data.bs(0);
        o.h(5, 0);
        o.h(4, 0);
    }
    o.tv(name);
    o.data.bl(tab);
    o.data.bs(0);
    o.data.bd3([0.0; 3]);
    o.data.rd2([0.0, 0.0]);
    o.data.rd2(paper);
    o.data.bd3([0.0; 3]);
    o.data.bd3([1.0, 0.0, 0.0]);
    o.data.bd3([0.0, 1.0, 0.0]);
    o.data.bd(0.0);
    o.data.bs(0);
    o.data.bd3([0.0; 3]);
    o.data.bd3([0.0; 3]);
    if version.r2018() {
        o.data.bl(0);
    }
    o.h(4, record);
    o.h(4, 0);
    o.h(5, 0);
    o.h(5, 0);
    o.bytes()
}

/// 図形の置き場所。links は R2000 の前後の図形、owner はブロックの中の図形の持ち主
pub struct Place {
    pub layer: u64,
    pub color: i16,
    pub links: Option<(u64, u64)>,
    pub owner: Option<u64>,
}

pub fn line(version: Version, handle: u64, place: &Place, start: [f64; 3], end: [f64; 3]) -> Vec<u8> {
    let mut o = Obj::new(version, 19, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    let z_zero = start[2] == 0.0 && end[2] == 0.0;
    o.data.bit(z_zero);
    o.data.rd(start[0]);
    o.data.dd(end[0], start[0]);
    o.data.rd(start[1]);
    o.data.dd(end[1], start[1]);
    if !z_zero {
        o.data.rd(start[2]);
        o.data.dd(end[2], start[2]);
    }
    o.data.bt(0.0);
    o.data.be([0.0, 0.0, 1.0]);
    o.bytes()
}

/// ARC（20.4.18）。角度はラジアン
pub fn arc(version: Version, handle: u64, place: &Place, center: [f64; 3], radius: f64, angles: [f64; 2]) -> Vec<u8> {
    let mut o = Obj::new(version, 17, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bd3(center);
    o.data.bd(radius);
    o.data.bt(0.0);
    o.data.be([0.0, 0.0, 1.0]);
    o.data.bd(angles[0]);
    o.data.bd(angles[1]);
    o.bytes()
}

/// TEXT（20.4.3）。回転はラジアン
#[allow(clippy::too_many_arguments)]
pub fn text(
    version: Version,
    handle: u64,
    place: &Place,
    value: &str,
    insert: [f64; 2],
    height: f64,
    rotation: f64,
    style: u64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 1, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    // 標高・揃え点・斜体・幅・生成・揃えを省き、回転だけを持つ
    o.data.rc(0x01 | 0x02 | 0x04 | 0x10 | 0x20 | 0x40 | 0x80);
    o.data.rd2(insert);
    o.data.be([0.0, 0.0, 1.0]);
    o.data.bt(0.0);
    o.data.rd(rotation);
    o.data.rd(height);
    o.tv(value);
    o.h(5, style);
    o.bytes()
}

/// MTEXT（20.4.46）
pub fn mtext(
    version: Version,
    handle: u64,
    place: &Place,
    value: &str,
    insert: [f64; 3],
    height: f64,
    style: u64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 44, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bd3(insert);
    o.data.bd3([0.0, 0.0, 1.0]);
    o.data.bd3([1.0, 0.0, 0.0]);
    o.data.bd(0.0);
    if version.r2018() {
        o.data.bd(0.0);
    }
    o.data.bd(height);
    o.data.bs(1);
    o.data.bs(1);
    o.data.bd(0.0);
    o.data.bd(0.0);
    o.tv(value);
    o.h(5, style);
    o.data.bs(1);
    o.data.bd(1.0);
    o.data.bit(false);
    if version.r2018() {
        o.data.bl(0);
        // R2018: 注釈の MTEXT（重ねた値と段組みの値を持たない）
        o.data.bit(true);
    }
    o.bytes()
}

/// INSERT（20.4.9）。回転はラジアン
#[allow(clippy::too_many_arguments)]
pub fn insert(
    version: Version,
    handle: u64,
    place: &Place,
    block: u64,
    point: [f64; 3],
    scale: [f64; 3],
    rotation: f64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 7, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bd3(point);
    o.data.bb(0);
    o.data.rd(scale[0]);
    o.data.dd(scale[1], scale[0]);
    o.data.dd(scale[2], scale[0]);
    o.data.bd(rotation);
    o.data.bd3([0.0, 0.0, 1.0]);
    o.data.bit(false);
    o.h(5, block);
    o.bytes()
}

/// LWPOLYLINE（20.4.85）。bulges は頂点ごと
pub fn lwpolyline(
    version: Version,
    handle: u64,
    place: &Place,
    points: &[[f64; 2]],
    bulges: &[f64],
    closed: bool,
) -> Vec<u8> {
    let mut o = Obj::new(version, 77, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bs(16 | if closed { 512 } else { 0 });
    o.data.bl(points.len() as i32);
    o.data.bl(bulges.len() as i32);
    for (index, point) in points.iter().enumerate() {
        if index == 0 {
            o.data.rd2(*point);
        } else {
            o.data.dd(point[0], points[index - 1][0]);
            o.data.dd(point[1], points[index - 1][1]);
        }
    }
    for bulge in bulges {
        o.data.bd(*bulge);
    }
    o.bytes()
}

/// DIMENSION（LINEAR、20.4.24）。points は 10・13・14、表示のブロックと寸法スタイルを持つ
#[allow(clippy::too_many_arguments)]
pub fn linear_dimension(
    version: Version,
    handle: u64,
    place: &Place,
    points: [[f64; 3]; 3],
    text_midpoint: [f64; 2],
    measurement: f64,
    dimstyle: u64,
    block: u64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 21, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    if version.r2018() {
        o.data.rc(0);
    }
    o.data.bd3([0.0, 0.0, 1.0]);
    o.data.rd2(text_midpoint);
    o.data.bd(0.0);
    o.data.rc(1);
    o.tv("");
    o.data.bd(0.0);
    o.data.bd(0.0);
    o.data.bd3([1.0, 1.0, 1.0]);
    o.data.bd(0.0);
    o.data.bs(5);
    o.data.bs(1);
    o.data.bd(1.0);
    o.data.bd(measurement);
    if version.r2018() {
        o.data.bit(false);
        o.data.bit(false);
        o.data.bit(false);
    }
    o.data.rd2([0.0, 0.0]);
    o.data.bd3(points[1]);
    o.data.bd3(points[2]);
    o.data.bd3(points[0]);
    o.data.bd(0.0);
    o.data.bd(0.0);
    o.h(5, dimstyle);
    o.h(5, block);
    o.bytes()
}

/// SPLINE（20.4.40）の当てはめ点と端の接線（scenario 2）。R2013 以降は節点の取り方（弦の長さ）を持つ
pub fn fit_spline(version: Version, handle: u64, place: &Place, fit: &[[f64; 3]], tangents: [[f64; 3]; 2]) -> Vec<u8> {
    let mut o = Obj::new(version, 36, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bl(2);
    if version.r2018() {
        // 当てはめ点で作り（1）、節点の取り方を使う（8）。弦の長さ（0）
        o.data.bl(1 | 8);
        o.data.bl(0);
    }
    o.data.bl(3);
    o.data.bd(1e-10);
    o.data.bd3(tangents[0]);
    o.data.bd3(tangents[1]);
    o.data.bl(fit.len() as i32);
    for point in fit {
        o.data.bd3(*point);
    }
    o.bytes()
}

/// LEADER（20.4.47）。注釈（MTEXT）のハンドルと寸法スタイルを持つ。フックは横の向き (1, 0, 0) の反対
pub fn leader(
    version: Version,
    handle: u64,
    place: &Place,
    points: &[[f64; 3]],
    annotation: u64,
    dimstyle: u64,
) -> Vec<u8> {
    let mut o = Obj::new(version, 45, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bit(false);
    o.data.bs(0);
    o.data.bs(0);
    o.data.bl(points.len() as i32);
    for point in points {
        o.data.bd3(*point);
    }
    o.data.bd3(points[0]);
    o.data.bd3([0.0, 0.0, 1.0]);
    o.data.bd3([1.0, 0.0, 0.0]);
    o.data.bd3([0.0; 3]);
    o.data.bd3([0.0; 3]);
    if !version.r2018() {
        // 文字の枠の高さと幅は R2007 まで
        o.data.bd(1.0);
        o.data.bd(5.0);
    }
    o.data.bit(false);
    o.data.bit(true);
    o.data.bs(0);
    o.data.bit(false);
    o.data.bit(false);
    o.h(5, annotation);
    o.h(5, dimstyle);
    o.bytes()
}
