//! テストの図面を書く（R2000 と R2018）。ビット単位の値・オブジェクトの枠・節の区切りと CRC・入れ物は、zumen_dwg の
//! 書き出し（zumen_dwg::raw）を使う。ここでは、読み取りが前提とする並びの最小のヘッダ変数と、規則を確かめる
//! オブジェクトを作る。Open Design Specification for .dwg files 5.4.1 の節の番号は読み取り側の注記を見る。
#![allow(dead_code)]

pub use zumen_dwg::raw::{BitWriter as Bits, Version};
use zumen_dwg::raw::{Database, ObjectWriter};

/// R2007 以降の書き方（文字列の並び・UTF-16）。R2010 以降の型（OT）とハンドルの並びの大きさ（MC）もこれで決める
fn r2018(version: Version) -> bool {
    version >= Version::R2018
}

/// 1 つのオブジェクト。文字列は R2007 より前は Shift_JIS（テストの図面の文字コード）
pub type Obj = ObjectWriter;

pub fn obj(version: Version, kind: u16, handle: u64) -> Obj {
    ObjectWriter::new(version, encoding_rs::SHIFT_JIS, kind, handle)
}

/// テストのオブジェクトの共通の値
pub trait Common {
    /// CMC（2.11）。R2004 以降は BS（0）・BL（0xC3 と ACI、0xC2 と RGB、0xC0 BYLAYER、0xC1 BYBLOCK）・RC
    fn cmc(&mut self, index: i16, rgb: Option<u32>);
    /// 図形でないオブジェクトの共通の値（20.1）。リアクタなし、R2018 は拡張辞書なし
    fn non_entity(&mut self, owner: u64);
    /// 図形の共通の値（20.4.1、20.4.2）。色は ACI、線種・印刷スタイル・マテリアルは BYLAYER。owner は図形のモード 00
    /// （ブロックの中）の持ち主。R2000 の前後の図形は links（なければ Nolinks = 1 でハンドル ±1）
    fn entity(&mut self, layer: u64, color: i16, links: Option<(u64, u64)>, owner: Option<u64>);
    /// 図形のモード（BB）からの共通の値。R2018 の複数行の属性の埋め込みの MTEXT も、ここから書く（20.4.4）
    fn entity_mode(&mut self, layer: u64, color: i16, links: Option<(u64, u64)>, owner: Option<u64>);
    /// MTEXT の値（20.4.46）
    fn mtext_fields(&mut self, value: &str, insert: [f64; 3], height: f64, attachment: i16, style: u64);
}

impl Common for Obj {
    fn cmc(&mut self, index: i16, rgb: Option<u32>) {
        if self.r2018() {
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

    fn non_entity(&mut self, owner: u64) {
        self.data.bl(0);
        if self.r2018() {
            // XDic Missing Flag と（R2013 以降）Has DS binary data
            self.data.bit(true);
            self.data.bit(false);
        }
        self.h(4, owner);
        if !self.r2018() {
            self.h(3, 0);
        }
    }

    fn entity(&mut self, layer: u64, color: i16, links: Option<(u64, u64)>, owner: Option<u64>) {
        // 画像（代わりの表示）はない
        self.data.bit(false);
        self.entity_mode(layer, color, links, owner);
    }

    fn entity_mode(&mut self, layer: u64, color: i16, links: Option<(u64, u64)>, owner: Option<u64>) {
        match owner {
            Some(owner) => {
                self.data.bb(0);
                self.h(4, owner);
            }
            None => self.data.bb(2),
        }
        self.data.bl(0);
        if self.r2018() {
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
        if self.r2018() {
            self.data.bb(0);
            self.data.rc(0);
            self.data.bit(false);
            self.data.bit(false);
            self.data.bit(false);
        }
        self.data.bs(0);
        self.data.rc(29);
    }

    /// 向きは X、折り返しなし、背景なし。R2018 は AutoCAD と同じく注釈でない MTEXT（Is NOT annotative が 1）で、
    /// 同じ値（REDUNDANT FIELDS）と段組みなしを続ける
    fn mtext_fields(&mut self, value: &str, insert: [f64; 3], height: f64, attachment: i16, style: u64) {
        let r2018 = self.r2018();
        self.data.bd3(insert);
        self.data.bd3([0.0, 0.0, 1.0]);
        self.data.bd3([1.0, 0.0, 0.0]);
        self.data.bd(0.0);
        if r2018 {
            self.data.bd(0.0);
        }
        self.data.bd(height);
        self.data.bs(attachment);
        self.data.bs(1);
        self.data.bd(0.0);
        self.data.bd(0.0);
        self.tv(value);
        self.h(5, style);
        self.data.bs(1);
        self.data.bd(1.0);
        self.data.bit(false);
        if r2018 {
            self.data.bl(0);
            self.data.bit(true);
            self.data.bs(0);
            self.data.bit(true);
            self.h(5, 0);
            self.data.bl(i32::from(attachment));
            self.data.bd3([1.0, 0.0, 0.0]);
            self.data.bd3(insert);
            for _ in 0..4 {
                self.data.bd(0.0);
            }
            self.data.bs(0);
        }
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

/// テストの図面の次に振るハンドル（HANDSEED）
const HANDSEED: u64 = 0x1000;

/// ヘッダ変数の値（区切り・大きさ・CRC は書き出しが付ける）。読み取り（header.rs）と同じ並びで、多くは 0
pub fn header_section(version: Version, h: &Header) -> Vec<u8> {
    let r2018 = r2018(version);
    let mut o = obj(version, 0, 0);
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
    o.data.handle(0, HANDSEED);
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
    let mut body = Bits::new();
    if r2018 {
        let mut data = o.data.clone();
        if o.strings.bit_len() > 0 {
            data.append(&o.strings);
            data.rs(o.strings.bit_len() as u16);
            data.bit(true);
        } else {
            data.bit(false);
        }
        body.rl((data.bit_len() + 32) as u32);
        body.append(&data);
        body.append(&o.handles);
    } else {
        body.append(&o.data);
    }
    body.pad();
    body.into_bytes()
}

/// 図面の全体。objects はハンドルとオブジェクトのバイト列
pub struct Drawing {
    pub version: Version,
    pub codepage: u16,
    pub header: Header,
    pub objects: Vec<(u64, Vec<u8>)>,
    pub measurement: u16,
}

impl Drawing {
    pub fn bytes(&self) -> Vec<u8> {
        let h = &self.header;
        Database {
            version: self.version,
            maintenance: 0,
            codepage: self.codepage,
            header: header_section(self.version, h),
            classes: Vec::new(),
            objects: self.objects.clone(),
            measurement: self.measurement,
            second_header: [
                HANDSEED,
                h.block_control,
                h.layer_control,
                h.style_control,
                h.linetype_control,
                0,
                0,
                0,
                h.appid_control,
                h.dimstyle_control,
                0,
                h.named_objects,
                0,
                0,
            ],
        }
        .bytes()
        .expect("R2000 と R2018 は書ける")
    }
}

// --- 表・ブロック・図形を作る関数（読み取りの document.rs・entities.rs と同じ並び） ---------------------------

/// 表の項目の名前と外部参照のビット（R2007 以降は BS、それより前は B・BS・B）
fn entry_name(o: &mut Obj, name: &str) {
    o.tv(name);
    if o.r2018() {
        o.data.bs(0);
    } else {
        o.data.bit(false);
        o.data.bs(0);
        o.data.bit(false);
    }
}

/// 表の管理のオブジェクト（20.4.51 など）。項目の後に extra のハンドル（BLOCK_CONTROL のモデル・紙空間など）
pub fn control(version: Version, kind: u16, handle: u64, entries: &[u64], extra: &[u64]) -> Vec<u8> {
    let mut o = obj(version, kind, handle);
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
    let mut o = obj(version, 51, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    // 印刷する（16）と、線の太さ（既定 31）
    o.data.bs(16 | if off { 2 } else { 0 } | (31 << 5));
    o.cmc(color, None);
    o.h(5, 0);
    o.h(5, 0);
    if r2018(version) {
        o.h(5, 0);
    }
    o.h(5, linetype);
    if r2018(version) {
        o.h(5, 0);
    }
    o.bytes()
}

/// LTYPE（20.4.58）
pub fn linetype(version: Version, handle: u64, control: u64, name: &str, description: &str, dashes: &[f64]) -> Vec<u8> {
    let mut o = obj(version, 57, handle);
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
    if !r2018(version) {
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
    let mut o = obj(version, 53, handle);
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
    let mut o = obj(version, 69, handle);
    o.non_entity(control);
    entry_name(&mut o, name);
    o.tv("");
    o.tv("");
    o.data.bd(1.0);
    o.data.bd(dimasz);
    for _ in 0..7 {
        o.data.bd(0.0);
    }
    if r2018(version) {
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
    if r2018(version) {
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
    let mut o = obj(version, 49, handle);
    o.non_entity(1);
    entry_name(&mut o, header_name);
    o.data.bit(anonymous);
    o.data.bit(false);
    o.data.bit(false);
    o.data.bit(false);
    o.data.bit(false);
    if r2018(version) {
        o.data.bl(entities.len() as i32);
    }
    o.data.bd3([0.0, 0.0, 0.0]);
    o.tv("");
    o.data.rc(0);
    o.tv("");
    o.data.bl(0);
    if r2018(version) {
        o.data.bs(0);
        o.data.bit(true);
        o.data.rc(1);
    }
    o.h(5, 0);
    o.h(3, block);
    if r2018(version) {
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
    let mut o = obj(version, 4, handle);
    o.entity(0x10, 256, None, Some(record));
    o.tv(name);
    o.bytes()
}

/// ENDBLK（5）
pub fn endblk(version: Version, handle: u64, record: u64) -> Vec<u8> {
    let mut o = obj(version, 5, handle);
    o.entity(0x10, 256, None, Some(record));
    o.bytes()
}

/// DICTIONARY（20.4.44）
pub fn dictionary(version: Version, handle: u64, owner: u64, entries: &[(&str, u64)]) -> Vec<u8> {
    let mut o = obj(version, 42, handle);
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
    let mut o = obj(version, 82, handle);
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
    if !r2018(version) {
        o.tv("");
    }
    o.data.bd(1.0);
    o.data.bd(1.0);
    o.tv("");
    o.data.bs(0);
    o.data.bd(1.0);
    o.data.bd2([0.0, 0.0]);
    if r2018(version) {
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
    if r2018(version) {
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
    let mut o = obj(version, 19, handle);
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
    let mut o = obj(version, 17, handle);
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
    let mut o = obj(version, 1, handle);
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
    let mut o = obj(version, 44, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.mtext_fields(value, insert, height, 1, style);
    o.bytes()
}

/// 属性（ATTRIB 2、ATTDEF 3。20.4.4、20.4.5）の値。R2018 の複数行の属性（mtext があるもの）は、AutoCAD と同じく文字を
/// 埋め込みの MTEXT（左上の配置点と文字。図形のモード 00、持ち主のハンドル 0）に持ち、1 行の文字（text）は空にする
pub struct Attribute<'a> {
    pub tag: &'a str,
    /// ATTDEF の問い合わせの文字。None なら ATTRIB
    pub prompt: Option<&'a str>,
    /// 1 は非表示、2 は定数、4 は確認、8 は既定値
    pub flags: u8,
    pub text: &'a str,
    pub insert: [f64; 2],
    pub height: f64,
    pub mtext: Option<(&'a str, [f64; 3])>,
}

pub fn attribute(version: Version, handle: u64, place: &Place, a: &Attribute, style: u64) -> Vec<u8> {
    let mut o = obj(version, if a.prompt.is_some() { 3 } else { 2 }, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    // 20.4.3 の値。標高・揃え点・斜体・回転・幅・生成・揃えを省く
    o.data.rc(0xff);
    o.data.rd2(a.insert);
    o.data.be([0.0, 0.0, 1.0]);
    o.data.bt(0.0);
    o.data.rd(a.height);
    o.tv(a.text);
    o.h(5, style);
    if r2018(version) {
        // 版（R2010 以降）と属性の種類（R2018 以降。1 は 1 行、2 は複数行の属性、4 は複数行の属性定義）
        o.data.rc(0);
        o.data.rc(match (a.mtext, a.prompt) {
            (None, _) => 1,
            (Some(_), None) => 2,
            (Some(_), Some(_)) => 4,
        });
        if let Some((value, insert)) = a.mtext {
            o.entity_mode(place.layer, 256, None, Some(0));
            o.mtext_fields(value, insert, a.height, 1, style);
            // 注釈の値の大きさ
            o.data.bs(0);
        }
    }
    o.tv(a.tag);
    o.data.bs(0);
    o.data.rc(a.flags);
    if r2018(version) {
        o.data.bit(false);
    }
    if let Some(prompt) = a.prompt {
        if r2018(version) {
            o.data.rc(0);
        }
        o.tv(prompt);
    }
    o.bytes()
}

/// SEQEND（20.4.8）
pub fn seqend(version: Version, handle: u64, place: &Place) -> Vec<u8> {
    let mut o = obj(version, 6, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
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
    insert_with_attributes(version, handle, place, block, point, scale, rotation, &[], 0)
}

/// 属性（ATTRIB）の続く INSERT（20.4.9）。R2004 以降は持っている図形の並び、それより前は最初と最後の属性と、SEQEND
#[allow(clippy::too_many_arguments)]
pub fn insert_with_attributes(
    version: Version,
    handle: u64,
    place: &Place,
    block: u64,
    point: [f64; 3],
    scale: [f64; 3],
    rotation: f64,
    attributes: &[u64],
    seqend: u64,
) -> Vec<u8> {
    let mut o = obj(version, 7, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bd3(point);
    o.data.bb(0);
    o.data.rd(scale[0]);
    o.data.dd(scale[1], scale[0]);
    o.data.dd(scale[2], scale[0]);
    o.data.bd(rotation);
    o.data.bd3([0.0, 0.0, 1.0]);
    o.data.bit(!attributes.is_empty());
    if r2018(version) && !attributes.is_empty() {
        o.data.bl(attributes.len() as i32);
    }
    o.h(5, block);
    if let (Some(first), Some(last)) = (attributes.first(), attributes.last()) {
        if r2018(version) {
            for attribute in attributes {
                o.h(3, *attribute);
            }
        } else {
            o.h(4, *first);
            o.h(4, *last);
        }
        o.h(3, seqend);
    }
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
    let mut o = obj(version, 77, handle);
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
    let mut o = obj(version, 21, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    if r2018(version) {
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
    if r2018(version) {
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
    let mut o = obj(version, 36, handle);
    o.entity(place.layer, place.color, place.links, place.owner);
    o.data.bl(2);
    if r2018(version) {
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
    let mut o = obj(version, 45, handle);
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
    if !r2018(version) {
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
