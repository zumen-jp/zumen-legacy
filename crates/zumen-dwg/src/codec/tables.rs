//! 表のオブジェクト（20.4.51〜20.4.70）: 表の管理のオブジェクト、BLOCK_HEADER、LAYER、STYLE、LTYPE、VIEW、UCS、
//! VPORT、APPID、DIMSTYLE。値はすべて持ち、読んだ順に書き戻す

use super::object::ObjectCommon;
use super::{CmColor, In, RawText};
use crate::bits::{HandleRef, Read};
use crate::file::Version;
use crate::write::object::ObjectWriter;

/// 表の項目の名前と外部参照の値。R2007 以降は BS だけ（外部参照に依存すると 0x100）、それより前は 64 の印（B）・
/// 外部参照の番号 + 1（BS）・外部参照に依存（B）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EntryName {
    pub name: RawText,
    pub flag64: bool,
    pub xref_index: i16,
    pub xdep: bool,
}

impl EntryName {
    pub fn new(name: RawText) -> Self {
        Self { name, flag64: false, xref_index: 0, xdep: false }
    }

    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let name = s.text()?;
        if s.r2007() {
            return Ok(Self { name, flag64: false, xref_index: s.data.bs()?, xdep: false });
        }
        Ok(Self { name, flag64: s.data.bit()?, xref_index: s.data.bs()?, xdep: s.data.bit()? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        o.text(&self.name);
        if o.r2007() {
            o.data.bs(self.xref_index);
        } else {
            o.data.bit(self.flag64);
            o.data.bs(self.xref_index);
            o.data.bit(self.xdep);
        }
    }
}

/// 表の管理のオブジェクト（20.4.51 など）。entries の後の extra は、BLOCK_CONTROL のモデル空間・紙空間、
/// LTYPE_CONTROL の BYLAYER・BYBLOCK（どちらも 2 つ）。DIMSTYLE_CONTROL は項目の後に RC の数とその数のハンドルを持つ
/// （仕様にない。実物の図面で確かめた）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Control {
    pub common: ObjectCommon,
    pub entries: Vec<HandleRef>,
    pub extra: Vec<HandleRef>,
}

impl Control {
    /// 項目の後に続くハンドルの数
    pub fn extra_count(kind: u16) -> usize {
        match kind {
            48 | 56 => 2,
            _ => 0,
        }
    }

    pub fn read(s: &mut In<'_>, kind: u16) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let count = s.data.bl()?;
        let mut entries = Vec::new();
        for _ in 0..count.clamp(0, 1 << 20) {
            entries.push(s.handle()?);
        }
        let count = if kind == 68 { usize::from(s.data.rc()?) } else { Self::extra_count(kind) };
        let mut extra = Vec::new();
        for _ in 0..count {
            extra.push(s.handle()?);
        }
        Ok(Self { common, entries, extra })
    }

    pub fn write(&self, o: &mut ObjectWriter, kind: u16) {
        self.common.write(o);
        o.data.bl(self.entries.len() as i32);
        if kind == 68 {
            o.data.rc(self.extra.len().min(255) as u8);
        }
        for entry in self.entries.iter().chain(&self.extra) {
            o.href(*entry);
        }
    }
}

/// BLOCK_HEADER（20.4.52）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BlockHeader {
    pub common: ObjectCommon,
    pub name: EntryName,
    pub anonymous: bool,
    pub has_attributes: bool,
    pub xref: bool,
    pub overlaid: bool,
    /// R2000 以降の読み込みの印（外部参照）
    pub loaded: bool,
    pub base: [f64; 3],
    pub xref_path: RawText,
    /// R2000 以降の配置の数（0 でない RC の並びと 0）
    pub insert_count: Vec<u8>,
    pub description: RawText,
    pub preview: Vec<u8>,
    /// R2007 以降の単位・分解できる・尺度の変え方
    pub insert_units: i16,
    pub explodable: bool,
    pub scaling: u8,
    /// NULL（ハードポインタ）
    pub null: HandleRef,
    pub block: HandleRef,
    /// R2004 以降の持っている図形
    pub entities: Vec<HandleRef>,
    /// R2000 の最初と最後の図形（外部参照でないとき）
    pub first_last: Option<(HandleRef, HandleRef)>,
    pub endblk: HandleRef,
    pub inserts: Vec<HandleRef>,
    pub layout: HandleRef,
}

impl BlockHeader {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let anonymous = s.data.bit()?;
        let has_attributes = s.data.bit()?;
        let xref = s.data.bit()?;
        let overlaid = s.data.bit()?;
        let loaded = s.data.bit()?;
        // 仕様は R2004 以降につねに持つと書くが、外部参照のブロックは数を持たない（ACadSharp・読み取りと同じ）
        let owned = if s.r2004() && !xref && !overlaid { s.data.bl()? } else { 0 };
        let base = s.data.bd3()?;
        let xref_path = s.text()?;
        let mut insert_count = Vec::new();
        loop {
            let byte = s.data.rc()?;
            if byte == 0 {
                break;
            }
            insert_count.push(byte);
        }
        let description = s.text()?;
        let size = s.data.bl()?;
        let preview = s.data.bytes(size.max(0) as usize)?;
        let (mut insert_units, mut explodable, mut scaling) = (0, false, 0);
        if s.r2007() {
            insert_units = s.data.bs()?;
            explodable = s.data.bit()?;
            scaling = s.data.rc()?;
        }
        let null = s.handle()?;
        let block = s.handle()?;
        let mut entities = Vec::new();
        let mut first_last = None;
        if s.r2004() {
            for _ in 0..owned.clamp(0, 1 << 24) {
                entities.push(s.handle()?);
            }
        } else if !xref && !overlaid {
            first_last = Some((s.handle()?, s.handle()?));
        }
        let endblk = s.handle()?;
        let mut inserts = Vec::new();
        for _ in 0..insert_count.len() {
            inserts.push(s.handle()?);
        }
        let layout = s.handle()?;
        Ok(Self {
            common,
            name,
            anonymous,
            has_attributes,
            xref,
            overlaid,
            loaded,
            base,
            xref_path,
            insert_count,
            description,
            preview,
            insert_units,
            explodable,
            scaling,
            null,
            block,
            entities,
            first_last,
            endblk,
            inserts,
            layout,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.bit(self.anonymous);
        o.data.bit(self.has_attributes);
        o.data.bit(self.xref);
        o.data.bit(self.overlaid);
        o.data.bit(self.loaded);
        if o.r2004() && !self.xref && !self.overlaid {
            o.data.bl(self.entities.len() as i32);
        }
        o.data.bd3(self.base);
        o.text(&self.xref_path);
        for byte in &self.insert_count {
            o.data.rc(*byte);
        }
        o.data.rc(0);
        o.text(&self.description);
        o.data.bl(self.preview.len() as i32);
        o.data.raw(&self.preview);
        if o.r2007() {
            o.data.bs(self.insert_units);
            o.data.bit(self.explodable);
            o.data.rc(self.scaling);
        }
        o.href(self.null);
        o.href(self.block);
        if o.r2004() {
            if !self.xref && !self.overlaid {
                for entity in &self.entities {
                    o.href(*entity);
                }
            }
        } else if let Some((first, last)) = self.first_last {
            o.href(first);
            o.href(last);
        }
        o.href(self.endblk);
        for insert in &self.inserts {
            o.href(*insert);
        }
        o.href(self.layout);
    }
}

/// LAYER（20.4.54）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layer {
    pub common: ObjectCommon,
    pub name: EntryName,
    /// 1 凍結、2 非表示、4 新しいビューポートで凍結、8 ロック、16 印刷、0x3E0 線の太さの番号
    pub values: i16,
    pub color: CmColor,
    pub xref_block: HandleRef,
    pub plotstyle: HandleRef,
    /// R2007 以降のマテリアル
    pub material: Option<HandleRef>,
    pub linetype: HandleRef,
    /// R2013 以降の不明のハンドル（つねに NULL）
    pub unknown: Option<HandleRef>,
}

impl Layer {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let values = s.data.bs()?;
        let color = s.cmc()?;
        let xref_block = s.handle()?;
        let plotstyle = s.handle()?;
        let material = if s.r2007() { Some(s.handle()?) } else { None };
        let linetype = s.handle()?;
        let unknown = if s.version >= Version::R2013 { Some(s.handle()?) } else { None };
        Ok(Self { common, name, values, color, xref_block, plotstyle, material, linetype, unknown })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.bs(self.values);
        o.cmc(&self.color);
        o.href(self.xref_block);
        o.href(self.plotstyle);
        if o.r2007() {
            o.href(self.material.unwrap_or(HandleRef { code: 5, value: 0 }));
        }
        o.href(self.linetype);
        if o.r2013() {
            o.href(self.unknown.unwrap_or(HandleRef { code: 5, value: 0 }));
        }
    }
}

/// STYLE（20.4.56、SHAPEFILE）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TextStyle {
    pub common: ObjectCommon,
    pub name: EntryName,
    /// 形のファイル（DXF の 70 の 1）と縦書き（4）。読み取りと同じく形のファイルが先
    pub shape: bool,
    pub vertical: bool,
    pub height: f64,
    pub width: f64,
    /// ラジアン
    pub oblique: f64,
    pub generation: u8,
    pub last_height: f64,
    pub font: RawText,
    pub bigfont: RawText,
    pub xref_block: HandleRef,
}

impl TextStyle {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        Ok(Self {
            common,
            name,
            shape: s.data.bit()?,
            vertical: s.data.bit()?,
            height: s.data.bd()?,
            width: s.data.bd()?,
            oblique: s.data.bd()?,
            generation: s.data.rc()?,
            last_height: s.data.bd()?,
            font: s.text()?,
            bigfont: s.text()?,
            xref_block: s.handle()?,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.bit(self.shape);
        o.data.bit(self.vertical);
        o.data.bd(self.height);
        o.data.bd(self.width);
        o.data.bd(self.oblique);
        o.data.rc(self.generation);
        o.data.bd(self.last_height);
        o.text(&self.font);
        o.text(&self.bigfont);
        o.href(self.xref_block);
    }
}

/// LTYPE の 1 つの要素（線・空白・点、文字・形）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dash {
    pub length: f64,
    pub shape: i16,
    pub offset: [f64; 2],
    pub scale: f64,
    pub rotation: f64,
    pub flags: i16,
}

/// LTYPE（20.4.58）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Linetype {
    pub common: ObjectCommon,
    pub name: EntryName,
    pub description: RawText,
    pub pattern_length: f64,
    pub alignment: u8,
    pub dashes: Vec<Dash>,
    /// 文字の要素の文字列の領域。R2004 までは 256 バイト、R2007 以降は文字の要素があるときだけ 512 バイト
    pub strings: Vec<u8>,
    pub xref_block: HandleRef,
    /// 要素ごとの形のファイル（STYLE）
    pub shapefiles: Vec<HandleRef>,
}

impl Linetype {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let description = s.text()?;
        let pattern_length = s.data.bd()?;
        let alignment = s.data.rc()?;
        let count = s.data.rc()?;
        let mut dashes = Vec::new();
        for _ in 0..count {
            dashes.push(Dash {
                length: s.data.bd()?,
                shape: s.data.bs()?,
                offset: s.data.rd2()?,
                scale: s.data.bd()?,
                rotation: s.data.bd()?,
                flags: s.data.bs()?,
            });
        }
        let strings = if !s.r2007() {
            s.data.bytes(256)?
        } else if dashes.iter().any(|dash| dash.flags & 2 != 0) {
            s.data.bytes(512)?
        } else {
            Vec::new()
        };
        let xref_block = s.handle()?;
        let mut shapefiles = Vec::new();
        for _ in 0..dashes.len() {
            shapefiles.push(s.handle()?);
        }
        Ok(Self { common, name, description, pattern_length, alignment, dashes, strings, xref_block, shapefiles })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.text(&self.description);
        o.data.bd(self.pattern_length);
        o.data.rc(self.alignment);
        o.data.rc(self.dashes.len().min(255) as u8);
        for dash in self.dashes.iter().take(255) {
            o.data.bd(dash.length);
            o.data.bs(dash.shape);
            o.data.rd2(dash.offset);
            o.data.bd(dash.scale);
            o.data.bd(dash.rotation);
            o.data.bs(dash.flags);
        }
        o.data.raw(&self.strings);
        o.href(self.xref_block);
        for shapefile in self.shapefiles.iter().take(255) {
            o.href(*shapefile);
        }
    }
}

/// VIEWPORT ENTITY HEADER（20.4.70、R2000 まで）。紙空間のビューポートごとの項目。名前の後の B は DXF の 70 の 1、
/// ハンドルは外部参照のブロック、ビューポート、次の項目（仕様は前の項目と書くが、AutoCAD の図面では表の順の次を指す）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewportEntityHeader {
    pub common: ObjectCommon,
    pub name: EntryName,
    pub on: bool,
    pub xref_block: HandleRef,
    pub viewport: HandleRef,
    pub next: HandleRef,
}

impl ViewportEntityHeader {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let on = s.data.bit()?;
        Ok(Self { common, name, on, xref_block: s.handle()?, viewport: s.handle()?, next: s.handle()? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.bit(self.on);
        o.href(self.xref_block);
        o.href(self.viewport);
        o.href(self.next);
    }
}

/// APPID（20.4.66）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Appid {
    pub common: ObjectCommon,
    pub name: EntryName,
    pub unknown: u8,
    pub xref_block: HandleRef,
}

impl Appid {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        Ok(Self { common, name, unknown: s.data.rc()?, xref_block: s.handle()? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.rc(self.unknown);
        o.href(self.xref_block);
    }
}

/// VPORT（20.4.64）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Vport {
    pub common: ObjectCommon,
    pub name: EntryName,
    pub view_height: f64,
    /// 縦横比 × 高さ
    pub aspect: f64,
    pub view_center: [f64; 2],
    pub view_target: [f64; 3],
    pub view_direction: [f64; 3],
    pub twist: f64,
    pub lens: f64,
    pub front_clip: f64,
    pub back_clip: f64,
    /// 71 の 1・2・4 と、16 の反対（4 ビット）
    pub view_mode: [bool; 4],
    pub render_mode: u8,
    /// R2007 以降の照明
    pub default_lights: bool,
    pub lighting_type: u8,
    pub brightness: f64,
    pub contrast: f64,
    pub ambient: Option<CmColor>,
    pub lower_left: [f64; 2],
    pub upper_right: [f64; 2],
    pub ucs_follow: bool,
    pub circle_zoom: i16,
    pub fast_zoom: bool,
    pub ucs_icon: [bool; 2],
    pub grid: bool,
    pub grid_spacing: [f64; 2],
    pub snap: bool,
    pub snap_style: bool,
    pub snap_isopair: i16,
    pub snap_rotation: f64,
    pub snap_base: [f64; 2],
    pub snap_spacing: [f64; 2],
    pub unknown: bool,
    pub ucs_per_viewport: bool,
    pub ucs_origin: [f64; 3],
    pub ucs_x: [f64; 3],
    pub ucs_y: [f64; 3],
    pub ucs_elevation: f64,
    pub ucs_ortho: i16,
    /// R2007 以降の格子の印と主線の間隔
    pub grid_flags: i16,
    pub grid_major: i16,
    pub xref_block: HandleRef,
    /// R2007 以降の背景・表示スタイル・太陽
    pub background: Option<HandleRef>,
    pub visual_style: Option<HandleRef>,
    pub sun: Option<HandleRef>,
    pub named_ucs: HandleRef,
    pub base_ucs: HandleRef,
}

impl Vport {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let view_height = s.data.bd()?;
        let aspect = s.data.bd()?;
        let view_center = s.data.rd2()?;
        let view_target = s.data.bd3()?;
        let view_direction = s.data.bd3()?;
        let twist = s.data.bd()?;
        let lens = s.data.bd()?;
        let front_clip = s.data.bd()?;
        let back_clip = s.data.bd()?;
        let view_mode = [s.data.bit()?, s.data.bit()?, s.data.bit()?, s.data.bit()?];
        let render_mode = s.data.rc()?;
        let (mut default_lights, mut lighting_type, mut brightness, mut contrast, mut ambient) =
            (false, 0, 0.0, 0.0, None);
        if s.r2007() {
            default_lights = s.data.bit()?;
            lighting_type = s.data.rc()?;
            brightness = s.data.bd()?;
            contrast = s.data.bd()?;
            ambient = Some(s.cmc()?);
        }
        let lower_left = s.data.rd2()?;
        let upper_right = s.data.rd2()?;
        let ucs_follow = s.data.bit()?;
        let circle_zoom = s.data.bs()?;
        let fast_zoom = s.data.bit()?;
        let ucs_icon = [s.data.bit()?, s.data.bit()?];
        let grid = s.data.bit()?;
        let grid_spacing = s.data.rd2()?;
        let snap = s.data.bit()?;
        let snap_style = s.data.bit()?;
        let snap_isopair = s.data.bs()?;
        let snap_rotation = s.data.bd()?;
        let snap_base = s.data.rd2()?;
        let snap_spacing = s.data.rd2()?;
        let unknown = s.data.bit()?;
        let ucs_per_viewport = s.data.bit()?;
        let ucs_origin = s.data.bd3()?;
        let ucs_x = s.data.bd3()?;
        let ucs_y = s.data.bd3()?;
        let ucs_elevation = s.data.bd()?;
        let ucs_ortho = s.data.bs()?;
        let (mut grid_flags, mut grid_major) = (0, 0);
        if s.r2007() {
            grid_flags = s.data.bs()?;
            grid_major = s.data.bs()?;
        }
        let xref_block = s.handle()?;
        let (mut background, mut visual_style, mut sun) = (None, None, None);
        if s.r2007() {
            background = Some(s.handle()?);
            visual_style = Some(s.handle()?);
            sun = Some(s.handle()?);
        }
        let named_ucs = s.handle()?;
        let base_ucs = s.handle()?;
        Ok(Self {
            common,
            name,
            view_height,
            aspect,
            view_center,
            view_target,
            view_direction,
            twist,
            lens,
            front_clip,
            back_clip,
            view_mode,
            render_mode,
            default_lights,
            lighting_type,
            brightness,
            contrast,
            ambient,
            lower_left,
            upper_right,
            ucs_follow,
            circle_zoom,
            fast_zoom,
            ucs_icon,
            grid,
            grid_spacing,
            snap,
            snap_style,
            snap_isopair,
            snap_rotation,
            snap_base,
            snap_spacing,
            unknown,
            ucs_per_viewport,
            ucs_origin,
            ucs_x,
            ucs_y,
            ucs_elevation,
            ucs_ortho,
            grid_flags,
            grid_major,
            xref_block,
            background,
            visual_style,
            sun,
            named_ucs,
            base_ucs,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        o.data.bd(self.view_height);
        o.data.bd(self.aspect);
        o.data.rd2(self.view_center);
        o.data.bd3(self.view_target);
        o.data.bd3(self.view_direction);
        o.data.bd(self.twist);
        o.data.bd(self.lens);
        o.data.bd(self.front_clip);
        o.data.bd(self.back_clip);
        for bit in self.view_mode {
            o.data.bit(bit);
        }
        o.data.rc(self.render_mode);
        if o.r2007() {
            o.data.bit(self.default_lights);
            o.data.rc(self.lighting_type);
            o.data.bd(self.brightness);
            o.data.bd(self.contrast);
            o.cmc(self.ambient.as_ref().unwrap_or(&CmColor::aci(250)));
        }
        o.data.rd2(self.lower_left);
        o.data.rd2(self.upper_right);
        o.data.bit(self.ucs_follow);
        o.data.bs(self.circle_zoom);
        o.data.bit(self.fast_zoom);
        o.data.bit(self.ucs_icon[0]);
        o.data.bit(self.ucs_icon[1]);
        o.data.bit(self.grid);
        o.data.rd2(self.grid_spacing);
        o.data.bit(self.snap);
        o.data.bit(self.snap_style);
        o.data.bs(self.snap_isopair);
        o.data.bd(self.snap_rotation);
        o.data.rd2(self.snap_base);
        o.data.rd2(self.snap_spacing);
        o.data.bit(self.unknown);
        o.data.bit(self.ucs_per_viewport);
        o.data.bd3(self.ucs_origin);
        o.data.bd3(self.ucs_x);
        o.data.bd3(self.ucs_y);
        o.data.bd(self.ucs_elevation);
        o.data.bs(self.ucs_ortho);
        if o.r2007() {
            o.data.bs(self.grid_flags);
            o.data.bs(self.grid_major);
        }
        o.href(self.xref_block);
        if o.r2007() {
            let none = HandleRef { code: 5, value: 0 };
            o.href(self.background.unwrap_or(HandleRef { code: 4, value: 0 }));
            o.href(self.visual_style.unwrap_or(none));
            o.href(self.sun.unwrap_or(HandleRef { code: 3, value: 0 }));
        }
        o.href(self.named_ucs);
        o.href(self.base_ucs);
    }
}

/// DIMSTYLE（20.4.68）の値。名前は DXF の変数の名前（$ なし）
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DimValue {
    Bit(bool),
    Short(i16),
    Double(f64),
    Text(RawText),
    Color(CmColor),
}

/// DIMSTYLE の値の並び（R2000 以降）。(名前, 形, 版)。形は b・s・d・t・c（CMC）、版は 0 すべて、7 R2007 以降、
/// 10 R2010 以降
const DIM_FIELDS: &[(&str, char, u8)] = &[
    ("DIMPOST", 't', 0),
    ("DIMAPOST", 't', 0),
    ("DIMSCALE", 'd', 0),
    ("DIMASZ", 'd', 0),
    ("DIMEXO", 'd', 0),
    ("DIMDLI", 'd', 0),
    ("DIMEXE", 'd', 0),
    ("DIMRND", 'd', 0),
    ("DIMDLE", 'd', 0),
    ("DIMTP", 'd', 0),
    ("DIMTM", 'd', 0),
    ("DIMFXL", 'd', 7),
    ("DIMJOGANG", 'd', 7),
    ("DIMTFILL", 's', 7),
    ("DIMTFILLCLR", 'c', 7),
    ("DIMTOL", 'b', 0),
    ("DIMLIM", 'b', 0),
    ("DIMTIH", 'b', 0),
    ("DIMTOH", 'b', 0),
    ("DIMSE1", 'b', 0),
    ("DIMSE2", 'b', 0),
    ("DIMTAD", 's', 0),
    ("DIMZIN", 's', 0),
    ("DIMAZIN", 's', 0),
    ("DIMARCSYM", 's', 7),
    ("DIMTXT", 'd', 0),
    ("DIMCEN", 'd', 0),
    ("DIMTSZ", 'd', 0),
    ("DIMALTF", 'd', 0),
    ("DIMLFAC", 'd', 0),
    ("DIMTVP", 'd', 0),
    ("DIMTFAC", 'd', 0),
    ("DIMGAP", 'd', 0),
    ("DIMALTRND", 'd', 0),
    ("DIMALT", 'b', 0),
    ("DIMALTD", 's', 0),
    ("DIMTOFL", 'b', 0),
    ("DIMSAH", 'b', 0),
    ("DIMTIX", 'b', 0),
    ("DIMSOXD", 'b', 0),
    ("DIMCLRD", 'c', 0),
    ("DIMCLRE", 'c', 0),
    ("DIMCLRT", 'c', 0),
    ("DIMADEC", 's', 0),
    ("DIMDEC", 's', 0),
    ("DIMTDEC", 's', 0),
    ("DIMALTU", 's', 0),
    ("DIMALTTD", 's', 0),
    ("DIMAUNIT", 's', 0),
    ("DIMFRAC", 's', 0),
    ("DIMLUNIT", 's', 0),
    ("DIMDSEP", 's', 0),
    ("DIMTMOVE", 's', 0),
    ("DIMJUST", 's', 0),
    ("DIMSD1", 'b', 0),
    ("DIMSD2", 'b', 0),
    ("DIMTOLJ", 's', 0),
    ("DIMTZIN", 's', 0),
    ("DIMALTZ", 's', 0),
    ("DIMALTTZ", 's', 0),
    ("DIMUPT", 'b', 0),
    ("DIMATFIT", 's', 0),
    ("DIMFXLON", 'b', 7),
    ("DIMTXTDIRECTION", 'b', 10),
    ("DIMALTMZF", 'd', 10),
    ("DIMALTMZS", 't', 10),
    ("DIMMZF", 'd', 10),
    ("DIMMZS", 't', 10),
    ("DIMLWD", 's', 0),
    ("DIMLWE", 's', 0),
    // 70 の 1 のビット（仕様は「Unknown」）
    ("FLAG", 'b', 0),
];

fn dim_holds(since: u8, version: Version) -> bool {
    match since {
        7 => version >= Version::R2007,
        10 => version >= Version::R2010,
        _ => true,
    }
}

/// DIMSTYLE（20.4.68）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DimStyle {
    pub common: ObjectCommon,
    pub name: EntryName,
    /// DIM_FIELDS の順の値（その版にない値は持たない）
    pub values: Vec<(&'static str, DimValue)>,
    pub xref_block: HandleRef,
    /// DIMTXSTY、DIMLDRBLK、DIMBLK、DIMBLK1、DIMBLK2、（R2007 以降）DIMLTYPE、DIMLTEX1、DIMLTEX2
    pub handles: Vec<HandleRef>,
}

impl DimStyle {
    /// 版にある値の名前と形（b・s・d・t・c）
    pub fn fields(version: Version) -> impl Iterator<Item = (&'static str, char)> {
        DIM_FIELDS.iter().filter(move |(_, _, since)| dim_holds(*since, version)).map(|(name, kind, _)| (*name, *kind))
    }

    pub fn get(&self, name: &str) -> Option<&DimValue> {
        self.values.iter().find(|(key, _)| *key == name).map(|(_, value)| value)
    }

    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = EntryName::read(s)?;
        let mut values = Vec::with_capacity(DIM_FIELDS.len());
        for &(key, kind, since) in DIM_FIELDS {
            if !dim_holds(since, s.version) {
                continue;
            }
            let value = match kind {
                'b' => DimValue::Bit(s.data.bit()?),
                's' => DimValue::Short(s.data.bs()?),
                'd' => DimValue::Double(s.data.bd()?),
                't' => DimValue::Text(s.text()?),
                _ => DimValue::Color(s.cmc()?),
            };
            values.push((key, value));
        }
        let xref_block = s.handle()?;
        let count = if s.r2007() { 8 } else { 5 };
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            handles.push(s.handle()?);
        }
        Ok(Self { common, name, values, xref_block, handles })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        self.name.write(o);
        for &(key, kind, since) in DIM_FIELDS {
            if !dim_holds(since, o.version) {
                continue;
            }
            match (kind, self.get(key)) {
                ('b', Some(DimValue::Bit(value))) => o.data.bit(*value),
                ('s', Some(DimValue::Short(value))) => o.data.bs(*value),
                ('d', Some(DimValue::Double(value))) => o.data.bd(*value),
                ('t', Some(DimValue::Text(value))) => o.text(value),
                ('c', Some(DimValue::Color(value))) => o.cmc(value),
                ('b', _) => o.data.bit(false),
                ('s', _) => o.data.bs(0),
                ('d', _) => o.data.bd(0.0),
                ('t', _) => o.text(&RawText::empty(o.version)),
                _ => o.cmc(&CmColor::by_block()),
            }
        }
        o.href(self.xref_block);
        let count = if o.r2007() { 8 } else { 5 };
        for index in 0..count {
            o.href(self.handles.get(index).copied().unwrap_or(HandleRef { code: 5, value: 0 }));
        }
    }
}
