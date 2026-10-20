//! オブジェクトの枠と共通の値（20.1「Common non-entity object format」、20.2「Common entity format」、
//! 20.4.1「Common Entity Data」、20.4.2「Common Entity Handle Data」）をすべて読み書きする。
//! 拡張データ（28）はバイトのまま持つ

use super::In;
use crate::bits::{Bits, Eof, HandleRef, Read};
use crate::file::Version;
use crate::objects::{Text, string_stream};
use crate::write::object::ObjectWriter;

/// 開いたオブジェクト。input は拡張データの後（図形は代わりの表示の印の前）にある
pub(crate) struct RawObject<'a> {
    pub kind: u16,
    pub handle: u64,
    /// 拡張データ（APPID のハンドル, 値のバイト列）
    pub eed: Vec<(HandleRef, Vec<u8>)>,
    pub input: In<'a>,
}

/// オブジェクトを開く（20.1）。MS の大きさ、（R2010 以降）ハンドルの並びの大きさ、型、（R2010 より前）ハンドルの並びの
/// 位置、ハンドル、拡張データまで読み、値・文字列・ハンドルの並びを分ける
pub(crate) fn open_raw(objects: &[u8], offset: usize, version: Version) -> Read<RawObject<'_>> {
    let r2010 = version >= Version::R2010;
    let mut head = Bits::at(objects, offset);
    let size = head.ms()?;
    let handle_bits = if r2010 { head.umc()? } else { 0 };
    let start = head.pos();
    let end = start.checked_add(size.checked_mul(8).ok_or(Eof)?).ok_or(Eof)?;
    if end > objects.len() as u64 * 8 {
        return Err(Eof);
    }
    let mut data = Bits::new(objects);
    data.set_pos(start);
    data.set_end(end);
    let kind = data.ot(r2010)?;
    let handles_start = if r2010 { end.checked_sub(handle_bits).ok_or(Eof)? } else { start + u64::from(data.rl()?) };
    if handles_start > end || handles_start < data.pos() {
        return Err(Eof);
    }
    let mut data_end = handles_start;
    let mut strings = None;
    if version >= Version::R2007 {
        let (text, text_start) = string_stream(&objects[..(end / 8) as usize], handles_start - 1);
        if let Text::Stream(stream) = text {
            strings = Some(stream);
            data_end = text_start;
        } else {
            // 文字列の並びがない印（0 の 1 ビット）
            data_end = handles_start - 1;
        }
    }
    let mut handles = Bits::new(objects);
    handles.set_pos(handles_start);
    handles.set_end(end);
    data.set_end(data_end);
    let handle = data.handle()?.value;
    let mut eed = Vec::new();
    loop {
        let size = data.bs()?;
        if size <= 0 {
            break;
        }
        let app = data.handle()?;
        eed.push((app, data.bytes(size as usize)?));
    }
    Ok(RawObject { kind, handle, eed, input: In { data, strings, handles: Some(handles), version } })
}

/// 書く枠を作る（型・ハンドル・拡張データ）
pub(crate) fn writer(version: Version, kind: u16, handle: u64, eed: &[(HandleRef, Vec<u8>)]) -> ObjectWriter {
    let mut out = ObjectWriter::new(version, encoding_rs::WINDOWS_1252, kind, handle);
    out.eed = eed.iter().map(|(app, data)| (app.value, data.clone())).collect();
    out
}

/// 図形でないオブジェクトの共通の値（20.1）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObjectCommon {
    pub owner: HandleRef,
    pub reactors: Vec<HandleRef>,
    /// 拡張辞書。R2004 以降の「なし」の印が立っていれば None。R2000 は 0 のハンドルを持つ
    pub xdictionary: Option<HandleRef>,
    /// データの保存の節（AcDsPrototype_1b）に値を持つ（R2013 以降）
    pub ds_binary: bool,
}

impl ObjectCommon {
    /// 持ち主だけの値（リアクタなし、拡張辞書なし）
    pub fn owned_by(owner: u64, version: Version) -> Self {
        Self {
            owner: HandleRef { code: 4, value: owner },
            reactors: Vec::new(),
            xdictionary: (version < Version::R2004).then_some(HandleRef { code: 3, value: 0 }),
            ds_binary: false,
        }
    }

    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let reactors = s.data.bl()?;
        let missing = s.r2004() && s.data.bit()?;
        let ds_binary = s.version >= Version::R2013 && s.data.bit()?;
        let owner = s.handle()?;
        let mut list = Vec::new();
        for _ in 0..reactors.clamp(0, 1 << 16) {
            list.push(s.handle()?);
        }
        let xdictionary = if missing { None } else { Some(s.handle()?) };
        Ok(Self { owner, reactors: list, xdictionary, ds_binary })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        o.data.bl(self.reactors.len() as i32);
        if o.r2004() {
            o.data.bit(self.xdictionary.is_none());
        }
        if o.r2013() {
            o.data.bit(self.ds_binary);
        }
        o.h(self.owner.code, self.owner.value);
        for reactor in &self.reactors {
            o.h(reactor.code, reactor.value);
        }
        match self.xdictionary {
            Some(xdictionary) => o.h(xdictionary.code, xdictionary.value),
            // R2004 より前は拡張辞書のハンドルをつねに書く
            None if !o.r2004() => o.h(3, 0),
            None => {}
        }
    }
}

/// 図形の色（2.11 の ENC。R2004 より前は BS の色番号だけ）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EntityColor {
    /// BS の値。R2004 以降は上位のビットが印（0x8000 RGB、0x4000 色見本の色、0x2000 透過度）と、下位が色番号
    pub value: u16,
    /// RGB の BL（0x8000 だけのとき。色見本の色 0x4000 は持たない）
    pub rgb: Option<u32>,
    /// 透過度の BL（0x2000 のとき）
    pub transparency: Option<u32>,
}

impl EntityColor {
    pub fn index(index: u16) -> Self {
        Self { value: index, rgb: None, transparency: None }
    }
}

/// 図形の共通の値（20.2、20.4.1、20.4.2）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EntityCommon {
    /// 代わりの表示（プロキシの図形の絵）のバイト列
    pub graphic: Option<Vec<u8>>,
    /// 図形のモード（BB）。0 は持ち主のハンドルがある、1 紙空間、2 モデル空間
    pub mode: u8,
    pub owner: Option<HandleRef>,
    pub reactors: Vec<HandleRef>,
    pub xdictionary: Option<HandleRef>,
    pub ds_binary: bool,
    /// R2000 の前と次の図形（Nolinks が 0 のとき）
    pub links: Option<(HandleRef, HandleRef)>,
    pub color: EntityColor,
    /// 色見本の色（DBCOLOR）のハンドル（ENC の 0x4000 のとき）
    pub book_color: Option<HandleRef>,
    pub linetype_scale: f64,
    pub layer: HandleRef,
    /// 線種の印（00 BYLAYER、01 BYBLOCK、10 CONTINUOUS、11 ハンドル）
    pub ltype_flags: u8,
    pub ltype: Option<HandleRef>,
    pub plotstyle_flags: u8,
    pub plotstyle: Option<HandleRef>,
    /// R2007 以降のマテリアルの印とハンドル、影の印
    pub material_flags: u8,
    pub material: Option<HandleRef>,
    pub shadow: u8,
    /// R2010 以降の表示スタイル（全体・面・辺）
    pub visual_styles: [Option<HandleRef>; 3],
    pub invisible: i16,
    /// 線の太さの番号（RC）
    pub lineweight: u8,
}

impl EntityCommon {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let graphic = if s.data.bit()? {
            let size = if s.version >= Version::R2010 { s.data.bll()? } else { u64::from(s.data.rl()?) };
            Some(s.data.bytes(usize::try_from(size).map_err(|_| Eof)?)?)
        } else {
            None
        };
        Self::read_mode(s, graphic)
    }

    /// 図形のモード（BB）からの値（R2018 の複数行の属性の埋め込みの MTEXT はここから、20.4.4）
    pub fn read_mode(s: &mut In<'_>, graphic: Option<Vec<u8>>) -> Read<Self> {
        let version = s.version;
        let mode = s.data.bb()?;
        let owner = if mode == 0 { Some(s.handle()?) } else { None };
        let count = s.data.bl()?;
        let mut reactors = Vec::new();
        for _ in 0..count.clamp(0, 1 << 16) {
            reactors.push(s.handle()?);
        }
        let missing = s.r2004() && s.data.bit()?;
        let xdictionary = if missing { None } else { Some(s.handle()?) };
        let ds_binary = version >= Version::R2013 && s.data.bit()?;
        let links = if !s.r2004() && !s.data.bit()? { Some((s.handle()?, s.handle()?)) } else { None };
        let value = s.data.bs()? as u16;
        let mut color = EntityColor::index(value);
        let mut book_color = None;
        if s.r2004() {
            // 色見本の色（0x4000、0x8000 も立つ）は RGB の BL を持たず、DBCOLOR のハンドルを持つ
            if value & 0x4000 != 0 {
                book_color = Some(s.handle()?);
            } else if value & 0x8000 != 0 {
                color.rgb = Some(s.data.bl()? as u32);
            }
            if value & 0x2000 != 0 {
                color.transparency = Some(s.data.bl()? as u32);
            }
        }
        let linetype_scale = s.data.bd()?;
        let layer = s.handle()?;
        let ltype_flags = s.data.bb()?;
        let ltype = if ltype_flags == 3 { Some(s.handle()?) } else { None };
        let plotstyle_flags = s.data.bb()?;
        let (mut material_flags, mut material, mut shadow) = (0, None, 0);
        if s.r2007() {
            material_flags = s.data.bb()?;
            if material_flags == 3 {
                material = Some(s.handle()?);
            }
            shadow = s.data.rc()?;
        }
        let plotstyle = if plotstyle_flags == 3 { Some(s.handle()?) } else { None };
        let mut visual_styles = [None; 3];
        if version >= Version::R2010 {
            for style in &mut visual_styles {
                if s.data.bit()? {
                    *style = Some(s.handle()?);
                }
            }
        }
        let invisible = s.data.bs()?;
        let lineweight = s.data.rc()?;
        Ok(Self {
            graphic,
            mode,
            owner,
            reactors,
            xdictionary,
            ds_binary,
            links,
            color,
            book_color,
            linetype_scale,
            layer,
            ltype_flags,
            ltype,
            plotstyle_flags,
            plotstyle,
            material_flags,
            material,
            shadow,
            visual_styles,
            invisible,
            lineweight,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        match &self.graphic {
            Some(graphic) => {
                o.data.bit(true);
                if o.r2010() {
                    o.data.bll(graphic.len() as u64);
                } else {
                    o.data.rl(graphic.len() as u32);
                }
                o.data.raw(graphic);
            }
            None => o.data.bit(false),
        }
        self.write_mode(o);
    }

    pub fn write_mode(&self, o: &mut ObjectWriter) {
        o.data.bb(self.mode);
        if self.mode == 0 {
            let owner = self.owner.unwrap_or(HandleRef { code: 4, value: 0 });
            o.h(owner.code, owner.value);
        }
        o.data.bl(self.reactors.len() as i32);
        for reactor in &self.reactors {
            o.h(reactor.code, reactor.value);
        }
        if o.r2004() {
            o.data.bit(self.xdictionary.is_none());
        }
        match self.xdictionary {
            Some(xdictionary) => o.h(xdictionary.code, xdictionary.value),
            None if !o.r2004() => o.h(3, 0),
            None => {}
        }
        if o.r2013() {
            o.data.bit(self.ds_binary);
        }
        if !o.r2004() {
            match self.links {
                Some((previous, next)) => {
                    o.data.bit(false);
                    o.h(previous.code, previous.value);
                    o.h(next.code, next.value);
                }
                None => o.data.bit(true),
            }
        }
        o.data.bs(self.color.value as i16);
        if o.r2004() {
            if self.color.value & 0x4000 != 0 {
                let book = self.book_color.unwrap_or(HandleRef { code: 5, value: 0 });
                o.h(book.code, book.value);
            } else if self.color.value & 0x8000 != 0 {
                o.data.bl(self.color.rgb.unwrap_or(0) as i32);
            }
            if self.color.value & 0x2000 != 0 {
                o.data.bl(self.color.transparency.unwrap_or(0) as i32);
            }
        }
        o.data.bd(self.linetype_scale);
        o.h(self.layer.code, self.layer.value);
        o.data.bb(self.ltype_flags);
        if self.ltype_flags == 3 {
            let ltype = self.ltype.unwrap_or(HandleRef { code: 5, value: 0 });
            o.h(ltype.code, ltype.value);
        }
        o.data.bb(self.plotstyle_flags);
        if o.r2007() {
            o.data.bb(self.material_flags);
            if self.material_flags == 3 {
                let material = self.material.unwrap_or(HandleRef { code: 5, value: 0 });
                o.h(material.code, material.value);
            }
            o.data.rc(self.shadow);
        }
        if self.plotstyle_flags == 3 {
            let plotstyle = self.plotstyle.unwrap_or(HandleRef { code: 5, value: 0 });
            o.h(plotstyle.code, plotstyle.value);
        }
        if o.r2010() {
            for style in &self.visual_styles {
                o.data.bit(style.is_some());
                if let Some(style) = style {
                    o.h(style.code, style.value);
                }
            }
        }
        o.data.bs(self.invisible);
        o.data.rc(self.lineweight);
    }
}
