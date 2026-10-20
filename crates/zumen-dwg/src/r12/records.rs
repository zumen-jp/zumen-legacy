//! R13 より前の DWG の記録（ファイルの頭・ヘッダ変数・表・図形）。値はすべてリトルエンディアンのバイト単位の値で、
//! R13 からのビット単位の値ではない。Open Design Specification はこの形を扱わないので、版ごとの Kaitai Struct の記述
//! （dwg_ac1009.ksy など。CC0-1.0）と、それに従う ACadSharp の派生版の R10〜R12 の読み取り（MIT）に従い、見本の
//! 図面で確かめた。項目の名前は記述の名前、DXF のグループコードは括弧の中

use super::Old;
use crate::bits::{Eof, Read};
use crate::objects::EedValue;

/// 拡張データ。(APPID の番号, グループコードと値)
pub(super) type EedApps = Vec<(i16, Vec<(i32, EedValue)>)>;

/// バイト単位の値を読む。範囲の外は Eof
#[derive(Clone)]
pub(super) struct Cursor<'a> {
    data: &'a [u8],
    pub pos: usize,
    end: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8], pos: usize, end: usize) -> Self {
        Self { data, pos, end: end.min(data.len()) }
    }

    fn take(&mut self, n: usize) -> Read<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Eof)?;
        if end > self.end {
            return Err(Eof);
        }
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn skip(&mut self, n: usize) -> Read<()> {
        self.take(n).map(|_| ())
    }

    pub fn u8(&mut self) -> Read<u8> {
        Ok(self.take(1)?[0])
    }

    pub fn i8(&mut self) -> Read<i8> {
        Ok(self.u8()? as i8)
    }

    pub fn u16(&mut self) -> Read<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Read<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Read<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Read<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn f64(&mut self) -> Read<f64> {
        let b = self.take(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(b);
        Ok(f64::from_le_bytes(bytes))
    }

    pub fn p2(&mut self) -> Read<[f64; 2]> {
        Ok([self.f64()?, self.f64()?])
    }

    pub fn p3(&mut self) -> Read<[f64; 3]> {
        Ok([self.f64()?, self.f64()?, self.f64()?])
    }

    /// 決まった長さの文字列（最初の NUL まで）
    pub fn fixed(&mut self, n: usize) -> Read<Vec<u8>> {
        Ok(until_nul(self.take(n)?).to_vec())
    }

    /// 長さ（RS）と文字列（最初の NUL まで）
    pub fn text(&mut self) -> Read<Vec<u8>> {
        let length = self.i16()?;
        let length = usize::try_from(length).map_err(|_| Eof)?;
        self.fixed(length)
    }
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.iter().position(|byte| *byte == 0).map_or(bytes, |end| &bytes[..end])
}

// --- ファイルの頭とヘッダ変数 ------------------------------------------------------

/// 節（ファイルの中の位置の範囲）
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Section {
    pub start: usize,
    pub end: usize,
}

impl Section {
    pub fn contains(&self, position: usize) -> bool {
        self.start > 0 && position >= self.start && position < self.end
    }
}

/// 表の記録の並び（記録の大きさ・数・始まり）
#[derive(Debug, Clone, Copy)]
pub(super) struct Table {
    pub item_size: usize,
    pub items: usize,
    pub begin: usize,
}

/// 読む表。AC1.50 から BLOCK・LAYER・STYLE・LTYPE・VIEW、R10 から UCS・VPORT・APPID、R11 から DIMSTYLE・VX の表がある
#[derive(Debug, Clone, Default)]
pub(super) struct Tables {
    pub block: Option<Table>,
    pub layer: Option<Table>,
    pub style: Option<Table>,
    pub linetype: Option<Table>,
    pub dimstyle: Option<Table>,
}

/// 読み取りに使うヘッダ変数
#[derive(Debug, Clone)]
pub(super) struct Vars {
    pub extmin: [f64; 3],
    pub extmax: [f64; 3],
    pub limmin: [f64; 2],
    pub limmax: [f64; 2],
    pub ltscale: f64,
    pub textsize: f64,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimtxt: f64,
    /// R11 から
    pub psltscale: bool,
    pub tilemode: bool,
    pub pextmin: [f64; 3],
    pub pextmax: [f64; 3],
    pub plimmin: [f64; 2],
    pub plimmax: [f64; 2],
    /// 図面の文字コードの番号（R11 から。R13 からと同じ番号）。0 は指定なし
    pub codepage: u16,
}

/// ファイルの頭（AC1.50 から）。図形・ブロックの図形・追加の図形の節と表の位置
#[derive(Debug, Clone, Default)]
pub(super) struct FileHeader {
    pub entities: Section,
    pub blocks: Section,
    pub extras: Section,
    pub tables: Tables,
}

/// 表の位置（item_size: u2、items: u2、不明の 2 バイト、begin: u4）
fn table_at(bytes: &[u8], at: usize) -> Option<Table> {
    let mut c = Cursor::new(bytes, at, bytes.len());
    let item_size = usize::from(c.u16().ok()?);
    let items = usize::from(c.u16().ok()?);
    c.skip(2).ok()?;
    let begin = c.u32().ok()? as usize;
    let fits = begin
        .checked_add(item_size.checked_mul(items)?)
        .is_some_and(|end| end <= bytes.len() && begin > 0 && item_size > 0);
    fits.then_some(Table { item_size, items, begin })
}

/// AC1.50 からのファイルの頭（記述の header）。0x14 から図形の始まり・終わり（s4）、ブロックの図形の始まり（s4）と大きさ
/// （u4 の下位 24 ビット）、追加の図形の始まりと大きさ、0x2C から 10 バイトずつ BLOCK・LAYER・STYLE・LTYPE・VIEW の表
pub(super) fn file_header(bytes: &[u8], version: Old) -> Read<FileHeader> {
    // 0x0C から不明の RC、図形の節の数・節の数・ヘッダ変数の数（RS）、版（RC）
    let mut c = Cursor::new(bytes, 0x14, bytes.len());
    let entities_start = c.i32()?;
    let entities_end = c.i32()?;
    let blocks_start = c.i32()?;
    let blocks_size = c.u32()? & 0x00ff_ffff;
    let extras_start = c.i32()?;
    let extras_size = c.u32()? & 0x00ff_ffff;
    let section = |start: i32, end: i64| {
        let start = usize::try_from(start).unwrap_or(0);
        let end = usize::try_from(end).unwrap_or(0).min(bytes.len());
        if start == 0 || end <= start { Section::default() } else { Section { start, end } }
    };
    let mut tables = Tables {
        block: table_at(bytes, 0x2c),
        layer: table_at(bytes, 0x36),
        style: table_at(bytes, 0x40),
        linetype: table_at(bytes, 0x4a),
        ..Tables::default()
    };
    // DIMSTYLE は R11 から（0x522）
    if version >= Old::Ac1009 {
        tables.dimstyle = table_at(bytes, 0x522);
    }
    Ok(FileHeader {
        entities: section(entities_start, i64::from(entities_end)),
        blocks: section(blocks_start, i64::from(blocks_start) + i64::from(blocks_size)),
        extras: section(extras_start, i64::from(extras_start) + i64::from(extras_size)),
        tables,
    })
}

/// AC1.50 からのヘッダ変数。位置は版によらず同じ（後の版は後ろに値を足した）: $EXTMIN 0x78、$EXTMAX 0x90、$LIMMIN 0xA8、
/// $LIMMAX 0xB8、$LTSCALE 0x132、$TEXTSIZE 0x13A、$DIMSCALE 0x1A3、$DIMASZ 0x1AB、$DIMTXT 0x1DB。R11 から $PSLTSCALE
/// 0x156、$DWGCODEPAGE 0x3F9、$TILEMODE 0x631、$PEXTMIN 0x637、$PEXTMAX 0x64F、$PLIMMIN 0x667、$PLIMMAX 0x677
pub(super) fn header_vars(bytes: &[u8], version: Old) -> Read<Vars> {
    let at = |offset: usize| Cursor::new(bytes, offset, bytes.len());
    let mut vars = Vars {
        extmin: at(0x78).p3()?,
        extmax: at(0x90).p3()?,
        limmin: at(0xa8).p2()?,
        limmax: at(0xb8).p2()?,
        ltscale: at(0x132).f64()?,
        textsize: at(0x13a).f64()?,
        dimscale: at(0x1a3).f64()?,
        dimasz: at(0x1ab).f64()?,
        dimtxt: at(0x1db).f64()?,
        psltscale: true,
        tilemode: true,
        pextmin: [0.0; 3],
        pextmax: [0.0; 3],
        plimmin: [0.0; 2],
        plimmax: [12.0, 9.0],
        codepage: 0,
    };
    if version >= Old::Ac1009 {
        vars.psltscale = at(0x156).u16()? != 0;
        vars.codepage = at(0x3f9).u16()?;
        vars.tilemode = at(0x631).u16()? != 0;
        vars.pextmin = at(0x637).p3()?;
        vars.pextmax = at(0x64f).p3()?;
        vars.plimmin = at(0x667).p2()?;
        vars.plimmax = at(0x677).p2()?;
    }
    Ok(vars)
}

/// AutoCAD 1.x（MC0.0・AC1.2・AC1.40）のファイルの頭。図形の数、範囲、文字の高さと、128 のレイヤ（番号が名前）の色。
/// 図形はヘッダ変数の後（MC0.0 は 0x1C8、AC1.2 は 0x1D8、AC1.40 は 0x202）から図形の数だけ続く
pub(super) struct EarlyHeader {
    pub entities: usize,
    pub start: usize,
    pub vars: Vars,
    /// レイヤの色（0〜127 の番号ごと。255 は使っていない）
    pub layer_colors: Vec<i16>,
}

pub(super) fn early_header(bytes: &[u8], version: Old) -> Read<EarlyHeader> {
    let at = |offset: usize| Cursor::new(bytes, offset, bytes.len());
    let entities = usize::try_from(at(0x28).i16()?).map_err(|_| Eof)?;
    let mut colors = at(0xc8);
    let mut layer_colors = Vec::with_capacity(128);
    for _ in 0..128 {
        layer_colors.push(colors.i16()?);
    }
    let start = match version {
        Old::Mc00 => 0x1c8,
        Old::Ac12 => 0x1d8,
        _ => 0x202,
    };
    let dimasz = if version >= Old::Ac12 { at(0x1c8).f64()? } else { 0.18 };
    let vars = Vars {
        extmin: at(0x2a).p3()?,
        extmax: at(0x42).p3()?,
        limmin: at(0x5a).p2()?,
        limmax: at(0x6a).p2()?,
        ltscale: 1.0,
        textsize: at(0xb4).f64()?,
        dimscale: 1.0,
        dimasz,
        dimtxt: 0.18,
        psltscale: true,
        tilemode: true,
        pextmin: [0.0; 3],
        pextmax: [0.0; 3],
        plimmin: [0.0; 2],
        plimmax: [12.0, 9.0],
        codepage: 0,
    };
    Ok(EarlyHeader { entities, start, vars, layer_colors })
}

// --- 表 -------------------------------------------------------------------------

/// 表の記録の頭: フラグ（DXF の 70。0x80 は消した記録）、名前（32 バイト）、（R11 から）使われている数（RS）
pub(super) struct Entry {
    pub flags: u8,
    pub name: Vec<u8>,
}

fn entry(c: &mut Cursor<'_>, version: Old) -> Read<Entry> {
    let flags = c.u8()?;
    let name = c.fixed(32)?;
    if version >= Old::Ac1009 {
        c.i16()?;
    }
    Ok(Entry { flags, name })
}

impl Entry {
    /// 消した記録と名前のない記録は使わない
    pub fn live(&self) -> bool {
        self.flags & 0x80 == 0 && !self.name.is_empty()
    }
}

/// 表の記録を順に読む（i 番目は begin + i × item_size）。読めない記録は None
pub(super) fn records<T>(
    bytes: &[u8],
    table: Option<Table>,
    mut read: impl FnMut(&mut Cursor<'_>) -> Read<T>,
) -> Vec<Option<T>> {
    let Some(table) = table else { return Vec::new() };
    (0..table.items.min(1 << 16))
        .take_while(|_| !zumen_core::limits::tick())
        .map(|index| {
            let start = table.begin + index * table.item_size;
            let mut c = Cursor::new(bytes, start, start + table.item_size);
            read(&mut c).ok()
        })
        .collect()
}

pub(super) struct Layer {
    pub entry: Entry,
    /// 色（負は非表示）
    pub color: i16,
    pub linetype: i16,
}

pub(super) fn layer(c: &mut Cursor<'_>, version: Old) -> Read<Layer> {
    let entry = entry(c, version)?;
    Ok(Layer { entry, color: c.i16()?, linetype: c.i16()? })
}

pub(super) struct Linetype {
    pub entry: Entry,
    pub description: Vec<u8>,
    pub pattern_length: f64,
    pub dashes: Vec<f64>,
}

/// LTYPE: 説明（48 バイト）、揃え（RC）、線の数（RC）、1 周期の長さ（RD）、12 の線と空白の長さ（RD）
pub(super) fn linetype(c: &mut Cursor<'_>, version: Old) -> Read<Linetype> {
    let entry = entry(c, version)?;
    let description = c.fixed(48)?;
    c.u8()?;
    let count = usize::from(c.u8()?).min(12);
    let pattern_length = c.f64()?;
    let mut dashes = Vec::with_capacity(count);
    for index in 0..12 {
        let dash = c.f64()?;
        if index < count {
            dashes.push(dash);
        }
    }
    Ok(Linetype { entry, description, pattern_length, dashes })
}

pub(super) struct Style {
    pub entry: Entry,
    pub height: f64,
    pub width: f64,
    pub oblique: f64,
    pub generation: u8,
    pub font: Vec<u8>,
    pub bigfont: Vec<u8>,
}

/// STYLE: 高さ・幅の比・斜体の角度（RD）、生成（RC）、最後の高さ（RD）、フォント（64 バイト）、（AC1001 から）ビッグフォント
pub(super) fn style(c: &mut Cursor<'_>, version: Old) -> Read<Style> {
    let entry = entry(c, version)?;
    let height = c.f64()?;
    let width = c.f64()?;
    let oblique = c.f64()?;
    let generation = c.u8()?;
    c.f64()?;
    let font = c.fixed(64)?;
    let bigfont = if version >= Old::Ac1001 { c.fixed(64).unwrap_or_default() } else { Vec::new() };
    Ok(Style { entry, height, width, oblique, generation, font, bigfont })
}

pub(super) struct Block {
    pub entry: Entry,
    /// BLOCK の図形の位置（下位 24 ビット。上位のバイトの 0x40 はブロックの節の中の位置）
    pub address: u32,
}

pub(super) fn block(c: &mut Cursor<'_>, version: Old) -> Read<Block> {
    let entry = entry(c, version)?;
    Ok(Block { entry, address: c.u32()? })
}

pub(super) struct DimStyle {
    pub entry: Entry,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimtxt: f64,
}

/// DIMSTYLE（R11）: DIMSCALE、DIMASZ、DIMEXO、DIMDLI、DIMEXE、DIMRND、DIMDLE、DIMTP、DIMTM、DIMTXT（RD）…
pub(super) fn dimstyle(c: &mut Cursor<'_>, version: Old) -> Read<DimStyle> {
    let entry = entry(c, version)?;
    let dimscale = c.f64()?;
    let dimasz = c.f64()?;
    for _ in 0..7 {
        c.f64()?;
    }
    let dimtxt = c.f64()?;
    Ok(DimStyle { entry, dimscale, dimasz, dimtxt })
}

// --- 図形 ---------------------------------------------------------------------

/// 図形の種類（AC1.50 から。AutoCAD 1.x は 1〜14）
pub(super) mod kind {
    pub const LINE: i8 = 1;
    pub const POINT: i8 = 2;
    pub const CIRCLE: i8 = 3;
    pub const SHAPE: i8 = 4;
    pub const REPEAT: i8 = 5;
    pub const ENDREP: i8 = 6;
    pub const TEXT: i8 = 7;
    pub const ARC: i8 = 8;
    pub const TRACE: i8 = 9;
    pub const LOAD: i8 = 10;
    pub const SOLID: i8 = 11;
    pub const BLOCK: i8 = 12;
    pub const ENDBLK: i8 = 13;
    pub const INSERT: i8 = 14;
    pub const ATTDEF: i8 = 15;
    pub const ATTRIB: i8 = 16;
    pub const SEQEND: i8 = 17;
    pub const JUMP: i8 = 18;
    pub const POLYLINE: i8 = 19;
    pub const VERTEX: i8 = 20;
    pub const LINE3D: i8 = 21;
    pub const FACE3D: i8 = 22;
    pub const DIMENSION: i8 = 23;
    pub const VIEWPORT: i8 = 24;
}

/// 図形の共通の値
#[derive(Debug, Clone, Default)]
pub(super) struct Common {
    pub layer: i16,
    /// 色（0 は BYBLOCK）。なければ BYLAYER
    pub color: Option<u8>,
    /// 線種の番号（R11 は 32767 が BYLAYER、32766 が BYBLOCK、R10 までは 255 が BYBLOCK）。なければ BYLAYER
    pub linetype: Option<i16>,
    pub elevation: f64,
    pub thickness: f64,
    pub handle: Option<u64>,
    /// 紙空間（R11 からの、ビューポートの番号を持つ図形）
    pub paper: bool,
    pub eed: EedApps,
    /// 図形のモードの 0x80（INSERT は属性、POLYLINE は頂点が続く）
    pub follows: bool,
    /// 図形のモードの 0x04。R10 から、3D の点を持つ図形（LINE・POINT・3DLINE・3DFACE）では Z を省いた印
    pub flat: bool,
}

/// 文字・属性の値
#[derive(Debug, Clone, Default)]
pub(super) struct TextFields {
    pub insert: [f64; 3],
    pub height: f64,
    pub value: Vec<u8>,
    pub rotation: f64,
    pub width: f64,
    pub oblique: f64,
    pub style: u8,
    pub generation: u8,
    pub halign: u8,
    pub valign: u8,
    pub align: Option<[f64; 3]>,
    pub extrusion: Option<[f64; 3]>,
}

/// 図形の種類ごとの値
#[derive(Debug, Clone)]
pub(super) enum Body {
    Line {
        start: [f64; 3],
        end: [f64; 3],
        extrusion: Option<[f64; 3]>,
    },
    Point {
        location: [f64; 3],
        extrusion: Option<[f64; 3]>,
        angle: f64,
    },
    Circle {
        center: [f64; 3],
        radius: f64,
        extrusion: Option<[f64; 3]>,
    },
    Arc {
        center: [f64; 3],
        radius: f64,
        angles: [f64; 2],
        extrusion: Option<[f64; 3]>,
    },
    Text(TextFields),
    Attdef {
        text: TextFields,
        prompt: Vec<u8>,
        tag: Vec<u8>,
        flags: u8,
    },
    Attrib {
        text: TextFields,
        tag: Vec<u8>,
        flags: u8,
    },
    /// SOLID・TRACE（平面の点）と 3DFACE（3D の点と見えない辺）
    Quad {
        corners: [[f64; 3]; 4],
        invisible: Option<u16>,
        extrusion: Option<[f64; 3]>,
    },
    Block {
        base: [f64; 3],
        name: Option<Vec<u8>>,
    },
    EndBlock,
    Insert(Insert),
    Polyline(Polyline),
    Vertex(Vertex),
    Seqend,
    Dimension(Box<Dimension>),
    Viewport {
        center: [f64; 3],
        width: f64,
        height: f64,
    },
    /// AutoCAD 1.x〜2.x の繰り返し（REPEAT から ENDREP までの図形を列・行に並べる）
    Repeat,
    EndRepeat {
        columns: i16,
        rows: i16,
        spacing: [f64; 2],
    },
    /// 読まない図形（SHAPE、LOAD）
    Other,
}

#[derive(Debug, Clone)]
pub(super) struct Insert {
    /// ブロックの番号。AutoCAD 1.x は名前（name）
    pub block: i16,
    pub name: Option<Vec<u8>>,
    pub point: [f64; 3],
    pub scale: [f64; 3],
    pub rotation: f64,
    pub columns: u16,
    pub rows: u16,
    pub spacing: [f64; 2],
    pub extrusion: Option<[f64; 3]>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Polyline {
    pub flags: u8,
    pub widths: [f64; 2],
    pub extrusion: Option<[f64; 3]>,
    pub m_count: u16,
    pub n_count: u16,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Vertex {
    pub point: [f64; 3],
    pub widths: Option<[f64; 2]>,
    pub bulge: f64,
    pub flags: u8,
    /// ポリフェースメッシュの面の頂点の番号（71〜74）
    pub indexes: [i16; 4],
    /// 位置を持たない面の記録（R11 の印の 0x4000）
    pub face: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Dimension {
    pub block: i16,
    pub defpoint: [f64; 3],
    pub text_midpoint: [f64; 3],
    pub insert: Option<[f64; 3]>,
    pub dimtype: u8,
    pub text: Vec<u8>,
    pub point13: Option<[f64; 3]>,
    pub point14: Option<[f64; 3]>,
    pub point15: Option<[f64; 3]>,
    pub point16: Option<[f64; 3]>,
    pub rotation: Option<f64>,
    pub oblique: Option<f64>,
    pub text_rotation: Option<f64>,
    pub extrusion: Option<[f64; 3]>,
    pub dimstyle: Option<i16>,
}

/// 1 つの図形の記録
#[derive(Debug, Clone)]
pub(super) struct Record {
    pub kind: i8,
    pub common: Common,
    pub body: Body,
    /// ファイルの中の位置
    pub position: usize,
    /// 種類ごとの値を読み終えた位置と、記録の値の終わり（R11 は CRC の前）。テストで並びを確かめる
    #[cfg_attr(not(test), allow(dead_code))]
    pub consumed: [usize; 2],
}

/// 記録を読んだ結果
pub(super) enum Step {
    Record(Box<Record>),
    /// 消した図形（型の 0x80）
    Deleted,
    /// JUMP: 次に読む位置。上位のバイトの 0x80 は追加の図形の節、0x40 はブロックの図形の節の中の位置、0 はファイルの
    /// 中の位置
    Jump(u32),
}

/// AC1.50 からの図形の記録を位置 at から読む（end は記録が収まる節の終わり）。型（RC）、モード（RC）、記録の大きさ
/// （RS。型から、R11 は終わりの CRC まで）、図形ごとの値。次の記録の位置を返す
pub(super) fn record(bytes: &[u8], at: usize, end: usize, version: Old) -> Read<(Step, usize)> {
    let mut head = Cursor::new(bytes, at, end);
    let kind = head.i8()?;
    let mode = head.u8()?;
    let size = usize::try_from(head.i16()?).map_err(|_| Eof)?;
    let next = at.checked_add(size).ok_or(Eof)?;
    if size < 4 || next > end {
        return Err(Eof);
    }
    if kind < 0 {
        return Ok((Step::Deleted, next));
    }
    let crc = if version >= Old::Ac1009 { 2 } else { 0 };
    let values_end = next.checked_sub(crc).filter(|values_end| *values_end >= at + 4).ok_or(Eof)?;
    let mut c = Cursor::new(bytes, at + 4, values_end);
    if kind == kind::JUMP {
        return Ok((Step::Jump(c.u32()?), next));
    }
    let (common, options) = common(&mut c, kind, mode, version)?;
    let body = body(&mut c, kind, options, &common, version)?;
    let record = Record { kind, common, body, position: at, consumed: [c.pos, values_end] };
    Ok((Step::Record(Box::new(record)), next))
}

/// 3D の点を持つ図形（LINE・POINT・3DLINE・3DFACE）
fn points_3d(kind: i8) -> bool {
    matches!(kind, kind::LINE | kind::POINT | kind::LINE3D | kind::FACE3D)
}

/// 共通の値: 層（RS）、種類ごとの値の有無の印（RS）、（R11）拡張の印（RC、モードの 0x40）と拡張データ（拡張の印の
/// 0x02）、色（RC、モードの 0x01）、線種（R11 から RS、それより前は RC。モードの 0x02）、標高（RD、0x04）と厚さ
/// （RD、0x08。R11 から厚さが先）、（R10 から）ハンドル（RC の長さと上位のバイトから、0x20）、（R11）ビューポート
/// （RS、拡張の印の 0x04。紙空間の図形）。R10 から、3D の点を持つ図形の 0x04 は標高ではなく Z を省いた印
fn common(c: &mut Cursor<'_>, kind: i8, mode: u8, version: Old) -> Read<(Common, u16)> {
    let r10 = version >= Old::Ac1006;
    let r11 = version >= Old::Ac1009;
    let mut common = Common { layer: c.i16()?, follows: mode & 0x80 != 0, flat: mode & 0x04 != 0, ..Common::default() };
    let options = c.u16()?;
    let mut extra = 0;
    if r11 && mode & 0x40 != 0 {
        extra = c.u8()?;
        if extra & 0x02 != 0 {
            common.eed = eed(c)?;
        }
    }
    if mode & 0x01 != 0 {
        common.color = Some(c.u8()?);
    }
    if mode & 0x02 != 0 {
        common.linetype = Some(if r11 { c.i16()? } else { i16::from(c.u8()?) });
    }
    let elevation = mode & 0x04 != 0 && !(r10 && points_3d(kind));
    let thickness = mode & 0x08 != 0;
    if r11 && thickness {
        common.thickness = c.f64()?;
    }
    if elevation {
        common.elevation = c.f64()?;
    }
    if !r11 && thickness {
        common.thickness = c.f64()?;
    }
    if r10 && mode & 0x20 != 0 {
        let length = usize::from(c.u8()?);
        if length > 8 {
            return Err(Eof);
        }
        let mut handle = 0u64;
        for _ in 0..length {
            handle = (handle << 8) | u64::from(c.u8()?);
        }
        common.handle = Some(handle);
    }
    if r11 && extra & 0x04 != 0 {
        c.i16()?;
        common.paper = true;
    }
    Ok((common, options))
}

/// 拡張データ（R11）: 大きさ（RS）と値の並び。値はコード − 1000（RC）と値（1000 は長さの RC と文字、1001 は APPID の
/// 番号の RS で、次の 1001 までがそのアプリケーションの値、1002 は RC、1003 は層の番号の RS、1004 は長さの RC とバイト、
/// 1005 は 8 バイト、1010〜1013 は 3RD、1040〜1042 は RD、1070 は RS、1071 は RL）。読めない値からは読まない
fn eed(c: &mut Cursor<'_>) -> Read<EedApps> {
    let size = usize::from(c.u16()?);
    let start = c.pos;
    c.skip(size)?;
    let mut data = Cursor::new(c.data, start, start + size);
    let mut apps: EedApps = Vec::new();
    let _ = eed_values(&mut data, &mut apps);
    Ok(apps)
}

fn eed_values(data: &mut Cursor<'_>, apps: &mut EedApps) -> Read<()> {
    while let Ok(code) = data.u8() {
        let value = match code {
            0 => {
                let length = usize::from(data.u8()?);
                EedValue::Text(String::from_utf8_lossy(&data.fixed(length)?).into_owned())
            }
            1 => {
                apps.push((data.i16()?, Vec::new()));
                continue;
            }
            2 => EedValue::Control(data.u8()? != 0),
            3 => EedValue::Number(f64::from(data.i16()?)),
            4 => {
                let length = usize::from(data.u8()?);
                data.skip(length)?;
                EedValue::Other
            }
            5 => {
                data.skip(8)?;
                EedValue::Other
            }
            10..=13 => EedValue::Point(data.p3()?),
            40..=42 => EedValue::Number(data.f64()?),
            70 => EedValue::Number(f64::from(data.i16()?)),
            71 => EedValue::Number(f64::from(data.i32()?)),
            _ => break,
        };
        match apps.last_mut() {
            Some((_, values)) => values.push((1000 + i32::from(code), value)),
            None => break,
        }
    }
    Ok(())
}

/// 平面の点（2RD。Z は標高）
fn plane(c: &mut Cursor<'_>, common: &Common) -> Read<[f64; 3]> {
    let [x, y] = c.p2()?;
    Ok([x, y, common.elevation])
}

/// 3D の点（2RD と、z が真なら RD）
fn point(c: &mut Cursor<'_>, z: bool) -> Read<[f64; 3]> {
    let [x, y] = c.p2()?;
    Ok([x, y, if z { c.f64()? } else { 0.0 }])
}

/// 印のビットがあるときだけある値
fn option<T>(options: u16, bit: u16, read: impl FnOnce() -> Read<T>) -> Read<Option<T>> {
    if options & bit != 0 { read().map(Some) } else { Ok(None) }
}

/// 種類ごとの値。省ける値は、共通の値の印（options）のビットがあるときだけある
fn body(c: &mut Cursor<'_>, kind: i8, options: u16, common: &Common, version: Old) -> Read<Body> {
    let r10 = version >= Old::Ac1006;
    let r11 = version >= Old::Ac1009;
    // R10 から、LINE・POINT・3DLINE・3DFACE は 0x04 がなければ Z を持つ。それより前は平面の点で、Z は標高
    let z = r10 && !common.flat;
    let corner = |c: &mut Cursor<'_>| if r10 { point(c, z) } else { plane(c, common) };
    Ok(match kind {
        kind::LINE => {
            let start = corner(c)?;
            let end = corner(c)?;
            let extrusion = if r10 { option(options, 0x01, || c.p3())? } else { None };
            Body::Line { start, end, extrusion }
        }
        kind::LINE3D => {
            // R10 は LINE と同じ。それより前は Z を印の 0x01（始点）・0x02（終点）があるときだけ持つ
            if r10 {
                let start = point(c, z)?;
                let end = point(c, z)?;
                Body::Line { start, end, extrusion: option(options, 0x01, || c.p3())? }
            } else {
                let start = point(c, options & 0x01 != 0)?;
                let end = point(c, options & 0x02 != 0)?;
                Body::Line { start, end, extrusion: None }
            }
        }
        kind::POINT => {
            let location = corner(c)?;
            let extrusion = if r11 { option(options, 0x01, || c.p3())? } else { None };
            let angle = if r11 { option(options, 0x02, || c.f64())?.unwrap_or(0.0) } else { 0.0 };
            Body::Point { location, extrusion, angle }
        }
        kind::CIRCLE => {
            let center = plane(c, common)?;
            let radius = c.f64()?;
            let extrusion = if r10 { option(options, 0x01, || c.p3())? } else { None };
            Body::Circle { center, radius, extrusion }
        }
        kind::ARC => {
            let center = plane(c, common)?;
            let radius = c.f64()?;
            let angles = [c.f64()?, c.f64()?];
            let extrusion = if r10 { option(options, 0x01, || c.p3())? } else { None };
            Body::Arc { center, radius, angles, extrusion }
        }
        kind::TEXT => Body::Text(text(c, options, common, 0)?),
        kind::ATTDEF | kind::ATTRIB => {
            // 位置、高さ、既定の値（ATTRIB は値）、（ATTDEF）問い合わせ、タグ、フラグ（RC）。省ける値は TEXT の 1 つ上の
            // ビット
            let insert = plane(c, common)?;
            let height = c.f64()?;
            let value = c.text()?;
            let prompt = if kind == kind::ATTDEF { c.text()? } else { Vec::new() };
            let tag = c.text()?;
            let flags = c.u8()?;
            let mut text = text_options(c, options, common, 1)?;
            text.insert = insert;
            text.height = height;
            text.value = value;
            if kind == kind::ATTDEF {
                Body::Attdef { text, prompt, tag, flags }
            } else {
                Body::Attrib { text, tag, flags }
            }
        }
        kind::SOLID | kind::TRACE => {
            let corners = [plane(c, common)?, plane(c, common)?, plane(c, common)?, plane(c, common)?];
            let extrusion = if r11 { option(options, 0x01, || c.p3())? } else { None };
            Body::Quad { corners, invisible: None, extrusion }
        }
        kind::FACE3D => {
            if r10 {
                let corners = [point(c, z)?, point(c, z)?, point(c, z)?, point(c, z)?];
                let invisible = option(options, 0x01, || if r11 { c.u16() } else { c.u8().map(u16::from) })?;
                Body::Quad { corners, invisible: Some(invisible.unwrap_or(0)), extrusion: None }
            } else {
                // AC1003・AC1004 は Z を角ごとに印の 0x01〜0x08 があるときだけ持つ
                let mut corners = [[0.0; 3]; 4];
                for (index, corner) in corners.iter_mut().enumerate() {
                    *corner = point(c, options & (1 << index) != 0)?;
                }
                Body::Quad { corners, invisible: Some(0), extrusion: None }
            }
        }
        kind::BLOCK => {
            let base = plane(c, common)?;
            let mut name = None;
            if r11 {
                // 外部参照のパス（0x02）、名前（0x04）
                option(options, 0x02, || c.text())?;
                name = option(options, 0x04, || c.text())?;
            }
            Body::Block { base, name }
        }
        kind::ENDBLK => Body::EndBlock,
        kind::SEQEND => {
            c.i32()?;
            Body::Seqend
        }
        kind::INSERT => {
            let block = c.i16()?;
            let point = plane(c, common)?;
            let mut insert = Insert {
                block,
                name: None,
                point,
                scale: [1.0; 3],
                rotation: 0.0,
                columns: 1,
                rows: 1,
                spacing: [0.0; 2],
                extrusion: None,
            };
            if let Some(x) = option(options, 0x01, || c.f64())? {
                insert.scale[0] = x;
            }
            if let Some(y) = option(options, 0x02, || c.f64())? {
                insert.scale[1] = y;
            }
            insert.rotation = option(options, 0x04, || c.f64())?.unwrap_or(0.0);
            if let Some(z) = option(options, 0x08, || c.f64())? {
                insert.scale[2] = z;
            }
            insert.columns = option(options, 0x10, || c.u16())?.unwrap_or(1);
            insert.rows = option(options, 0x20, || c.u16())?.unwrap_or(1);
            insert.spacing[0] = option(options, 0x40, || c.f64())?.unwrap_or(0.0);
            insert.spacing[1] = option(options, 0x80, || c.f64())?.unwrap_or(0.0);
            if r10 {
                insert.extrusion = option(options, 0x100, || c.p3())?;
            }
            Body::Insert(insert)
        }
        kind::POLYLINE => {
            let mut polyline = Polyline {
                flags: option(options, 0x01, || c.u8())?.unwrap_or(0),
                widths: [
                    option(options, 0x02, || c.f64())?.unwrap_or(0.0),
                    option(options, 0x04, || c.f64())?.unwrap_or(0.0),
                ],
                ..Polyline::default()
            };
            if r10 {
                polyline.extrusion = option(options, 0x08, || c.p3())?;
                polyline.m_count = option(options, 0x10, || c.u16())?.unwrap_or(0);
                polyline.n_count = option(options, 0x20, || c.u16())?.unwrap_or(0);
            }
            if r11 {
                // 曲面の密度（0x40・0x80）と種類（0x100）
                option(options, 0x40, || c.u16())?;
                option(options, 0x80, || c.u16())?;
                option(options, 0x100, || c.u16())?;
            }
            Body::Polyline(polyline)
        }
        kind::VERTEX => {
            let face = r11 && options & 0x4000 != 0;
            let mut vertex = Vertex { face, ..Vertex::default() };
            if !face {
                vertex.point = plane(c, common)?;
            }
            let start = option(options, 0x01, || c.f64())?;
            let end = option(options, 0x02, || c.f64())?;
            if start.is_some() || end.is_some() {
                vertex.widths = Some([start.unwrap_or(0.0), end.unwrap_or(0.0)]);
            }
            vertex.bulge = option(options, 0x04, || c.f64())?.unwrap_or(0.0);
            // フラグ（AC1002 から。R10 より前は意味が分からない）
            if version >= Old::Ac1002 {
                let flags = option(options, 0x08, || c.u8())?.unwrap_or(0);
                if r10 {
                    vertex.flags = flags;
                }
            }
            option(options, 0x10, || c.f64())?;
            if r11 {
                for (index, slot) in vertex.indexes.iter_mut().enumerate() {
                    *slot = option(options, 0x20 << index, || c.i16())?.unwrap_or(0);
                }
            }
            Body::Vertex(vertex)
        }
        kind::DIMENSION => Body::Dimension(Box::new(dimension(c, options, common, version)?)),
        kind::VIEWPORT => {
            let center = c.p3()?;
            let width = c.f64()?;
            let height = c.f64()?;
            c.i16()?;
            Body::Viewport { center, width, height }
        }
        kind::REPEAT => Body::Repeat,
        kind::ENDREP => {
            let columns = c.i16()?;
            let rows = c.i16()?;
            Body::EndRepeat { columns, rows, spacing: c.p2()? }
        }
        kind::SHAPE => {
            // 位置（平面の点）、大きさ（RD）、形の番号（RC）、回転（0x01）、文字スタイルの番号（0x02、RC）、（R11）幅の比
            // （0x04）、斜体の角度（0x08）。形は描かない
            plane(c, common)?;
            c.f64()?;
            c.u8()?;
            option(options, 0x01, || c.f64())?;
            option(options, 0x02, || c.u8())?;
            if r11 {
                option(options, 0x04, || c.f64())?;
                option(options, 0x08, || c.f64())?;
            }
            Body::Other
        }
        kind::LOAD => {
            c.text()?;
            Body::Other
        }
        _ => return Err(Eof),
    })
}

/// TEXT: 位置（平面の点）、高さ（RD）、文字（RS の長さとバイト）と、省ける値（shift は ATTDEF・ATTRIB で 1 つ上のビット）
fn text(c: &mut Cursor<'_>, options: u16, common: &Common, shift: u32) -> Read<TextFields> {
    let insert = plane(c, common)?;
    let height = c.f64()?;
    let value = c.text()?;
    let mut text = text_options(c, options, common, shift)?;
    text.insert = insert;
    text.height = height;
    text.value = value;
    Ok(text)
}

/// 文字の省ける値: 回転（RD）、幅の比（RD）、斜体の角度（RD）、文字スタイルの番号（RC）、生成（RC）、横の揃え（RC）、
/// 揃えの点（平面の点）、押し出し（3RD）、縦の揃え（RC）。TEXT は 0x01 から、ATTDEF・ATTRIB は 0x02 から
fn text_options(c: &mut Cursor<'_>, options: u16, common: &Common, shift: u32) -> Read<TextFields> {
    let bit = |n: u32| 1u16 << (n + shift);
    Ok(TextFields {
        rotation: option(options, bit(0), || c.f64())?.unwrap_or(0.0),
        width: option(options, bit(1), || c.f64())?.unwrap_or(1.0),
        oblique: option(options, bit(2), || c.f64())?.unwrap_or(0.0),
        style: option(options, bit(3), || c.u8())?.unwrap_or(0),
        generation: option(options, bit(4), || c.u8())?.unwrap_or(0),
        halign: option(options, bit(5), || c.u8())?.unwrap_or(0),
        align: option(options, bit(6), || plane(c, common))?,
        extrusion: option(options, bit(7), || c.p3())?,
        valign: option(options, bit(8), || c.u8())?.unwrap_or(0),
        ..TextFields::default()
    })
}

/// DIMENSION: ブロックの番号（RS）、定義点（10。R10 から 3RD、それより前は平面の点）、文字の中点（11、平面の点）と、
/// 省ける値: 12（0x01、平面の点）、種類（0x02、RC。DXF の 70）、文字（0x04）、13・14・15（0x08・0x10・0x20）、
/// 16（0x40。R10 は 3RD、R11 は平面の点）、引出線の長さ（0x80、RD）、回転（0x100）、斜線の角度（0x200）、文字の回転
/// （0x400）、押し出し（0x4000）、寸法スタイルの番号（0x8000、RS）
fn dimension(c: &mut Cursor<'_>, options: u16, common: &Common, version: Old) -> Read<Dimension> {
    let r10 = version >= Old::Ac1006;
    let r11 = version >= Old::Ac1009;
    let space = |c: &mut Cursor<'_>| if r10 { c.p3() } else { plane(c, common) };
    let mut d =
        Dimension { block: c.i16()?, defpoint: space(c)?, text_midpoint: plane(c, common)?, ..Dimension::default() };
    d.insert = option(options, 0x01, || plane(c, common))?;
    d.dimtype = option(options, 0x02, || c.u8())?.unwrap_or(0);
    d.text = option(options, 0x04, || c.text())?.unwrap_or_default();
    d.point13 = option(options, 0x08, || space(c))?;
    d.point14 = option(options, 0x10, || space(c))?;
    d.point15 = option(options, 0x20, || space(c))?;
    d.point16 = option(options, 0x40, || if r11 { plane(c, common) } else { space(c) })?;
    option(options, 0x80, || c.f64())?;
    d.rotation = option(options, 0x100, || c.f64())?;
    if r11 {
        d.oblique = option(options, 0x200, || c.f64())?;
        d.text_rotation = option(options, 0x400, || c.f64())?;
        d.extrusion = option(options, 0x4000, || c.p3())?;
        d.dimstyle = option(options, 0x8000, || c.i16())?;
    }
    Ok(d)
}

/// AutoCAD 1.x（MC0.0・AC1.2・AC1.40）の図形の記録。型（RC。負は消した図形）、フラグ（RC）、層の番号（RC）、
/// フラグ（RC）と、型ごとに決まった値（大きさを持たない）
pub(super) fn early_record(c: &mut Cursor<'_>) -> Read<Option<Record>> {
    let position = c.pos;
    let kind = c.i8()?;
    c.u8()?;
    let layer = i16::from(c.u8()?);
    c.u8()?;
    let common = Common { layer, ..Common::default() };
    let p = |c: &mut Cursor<'_>| -> Read<[f64; 3]> {
        let [x, y] = c.p2()?;
        Ok([x, y, 0.0])
    };
    let body = match kind.unsigned_abs() as i8 {
        kind::LINE => Body::Line { start: p(c)?, end: p(c)?, extrusion: None },
        kind::POINT => Body::Point { location: p(c)?, extrusion: None, angle: 0.0 },
        kind::CIRCLE => Body::Circle { center: p(c)?, radius: c.f64()?, extrusion: None },
        kind::SHAPE => {
            p(c)?;
            c.f64()?;
            c.f64()?;
            c.i16()?;
            Body::Other
        }
        kind::REPEAT => Body::Repeat,
        kind::ENDREP => {
            let columns = c.i16()?;
            let rows = c.i16()?;
            Body::EndRepeat { columns, rows, spacing: c.p2()? }
        }
        kind::TEXT => {
            let insert = p(c)?;
            let height = c.f64()?;
            let rotation = c.f64()?;
            let value = c.text()?;
            Body::Text(TextFields { insert, height, rotation, value, width: 1.0, ..TextFields::default() })
        }
        kind::ARC => Body::Arc { center: p(c)?, radius: c.f64()?, angles: [c.f64()?, c.f64()?], extrusion: None },
        kind::TRACE | kind::SOLID => {
            Body::Quad { corners: [p(c)?, p(c)?, p(c)?, p(c)?], invisible: None, extrusion: None }
        }
        kind::LOAD => {
            c.text()?;
            Body::Other
        }
        kind::BLOCK => {
            let name = c.text()?;
            Body::Block { name: Some(name), base: p(c)? }
        }
        kind::ENDBLK => Body::EndBlock,
        kind::INSERT => {
            let name = c.text()?;
            let point = p(c)?;
            let scale = [c.f64()?, c.f64()?, 1.0];
            Body::Insert(Insert {
                block: -1,
                name: Some(name),
                point,
                scale,
                rotation: c.f64()?,
                columns: 1,
                rows: 1,
                spacing: [0.0; 2],
                extrusion: None,
            })
        }
        _ => return Err(Eof),
    };
    if kind < 0 {
        return Ok(None);
    }
    Ok(Some(Record { kind, common, body, position, consumed: [0, 0] }))
}
