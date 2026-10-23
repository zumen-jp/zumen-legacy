//! DWG のその場の編集（docs/decisions/0012、docs/formats/dwg.md「Editing」）。図面を開き（write::rewrite::Source）、
//! 操作したオブジェクトだけを可逆の読み書き（src/codec）で読み直して書き換え、新しいオブジェクトを足す。触れていない
//! オブジェクトはビット列のまま写すので、拡張データ・リアクタ・拡張辞書と、読み取りが扱わないオブジェクトも残る。
//! 版・文字コードも元のまま。値のキーと決まりは DXF の編集（zumen-dxf の edit）と同じ。
//!
//! - 足す図形と消す図形は、持ち主のブロック（BLOCK_HEADER、20.4.52）の図形の並び（R2004 以降）か、前後の図形の
//!   つながりと最初・最後の図形（R13〜R2000、20.4.1 の Nolinks）を直す
//! - R13・R14 は線の太さとレイヤを印刷しない設定を持たない（指定は書かずに知らせる）。R14 の LWPOLYLINE はクラスの型
//!   （図面のクラスになければ足す）、R13 には LWPOLYLINE がないので折線は足せない（DXF の R13 の編集と同じ）
//! - 新しいオブジェクトのハンドルは $HANDSEED（大きいハンドルがあればその次）から振り、$HANDSEED と AcDb:AuxHeader の
//!   値（27）を次のハンドルにする
//! - 消す図形は、従う図形（頂点・属性と SEQEND）、拡張辞書とその中のオブジェクトも消し、描く順（SORTENTSTABLE）と
//!   グループ（GROUP）、リアクタから除く

mod build;

use crate::bits::{HandleRef, Read};
use crate::codec::entities::Image;
use crate::codec::entities::{Attribute, Dimension, DimensionKind as DwgDimensionKind, Leader, LeaderR14, Text};
use crate::codec::entities::{Body, Entity, fixed_name};
use crate::codec::header::{HeaderVars, Value, read_header_vars, write_header_vars};
use crate::codec::object::{EntityColor, EntityCommon, ObjectCommon, open_raw, writer};
use crate::codec::objects::{Dictionary, Group, ImageDef, ImageDefReactor, Layout, SortEntsTable};
use crate::codec::tables::{
    Appid, BlockHeader, Control, Dash, DimStyle, DimValue, EntryName, Layer, Linetype, TextStyle,
};
use crate::codec::{CmColor, In, RawText};
use crate::file::Version;
use crate::write::classes::{classes_bits, classes_data, read_class_defs};
use crate::write::database::has_extra;
use crate::write::object::ObjectWriter;
use crate::write::rewrite::Source;
use crate::write::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL};
use encoding_rs::Encoding;
use std::collections::{HashMap, HashSet};
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValue, EditValues, Edited, StyleKind,
    ValueReader,
};
use zumen_core::{NoticeLevel, NoticeList};
use zumen_dxf::attributes::{self, Definition, Placed};
use zumen_dxf::dimension::{self, DimStyleValues, DimensionInput, DimensionKind, Part, Role};

/// 足せる図形の種類（DXF の編集と同じ）
const ADDABLE: [&str; 15] = [
    "line",
    "polyline",
    "arc",
    "circle",
    "ellipse",
    "ellipse-arc",
    "point",
    "text",
    BLOCK_REFERENCE,
    "dimension",
    "hatch",
    "attribute-definition",
    "viewport",
    "leader",
    "image",
];

/// 図形・レイヤに共通する値のキー
const COMMON: [&str; 4] = ["layer", "color", "lineType", "lineWidth"];

/// レイヤの値のキー
const LAYER_KEYS: [&str; 7] = ["color", "lineType", "lineWidth", "visible", "frozen", "locked", "plot"];

/// 線の太さ（1/100 mm）。番号は DWG の線の太さの番号（0〜23）
const LINEWEIGHTS: [f64; 24] = [
    0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0, 106.0,
    120.0, 140.0, 158.0, 200.0, 211.0,
];

/// 固定の型の番号（20.3）
mod kind {
    pub const TEXT: u16 = 1;
    pub const ATTRIB: u16 = 2;
    pub const ATTDEF: u16 = 3;
    pub const INSERT: u16 = 7;
    pub const MINSERT: u16 = 8;
    pub const POLYLINE_2D: u16 = 15;
    pub const POLYLINE_3D: u16 = 16;
    pub const ARC: u16 = 17;
    pub const CIRCLE: u16 = 18;
    pub const LINE: u16 = 19;
    pub const POINT: u16 = 27;
    pub const POLYLINE_PFACE: u16 = 29;
    pub const POLYLINE_MESH: u16 = 30;
    pub const ELLIPSE: u16 = 35;
    pub const DICTIONARY: u16 = 42;
    pub const MTEXT: u16 = 44;
    pub const LAYER_CONTROL: u16 = 50;
    pub const LAYER: u16 = 51;
    pub const GROUP: u16 = 72;
    pub const LWPOLYLINE: u16 = 77;
    pub const BLOCK: u16 = 4;
    pub const ENDBLK: u16 = 5;
    pub const BLOCK_CONTROL: u16 = 48;
    pub const BLOCK_HEADER: u16 = 49;
    pub const SOLID: u16 = 31;
    pub const DIMENSION_LINEAR: u16 = 21;
    pub const DIMENSION_ALIGNED: u16 = 22;
    pub const DIMENSION_ANG3PT: u16 = 23;
    pub const DIMENSION_RADIUS: u16 = 25;
    pub const DIMENSION_DIAMETER: u16 = 26;
    pub const STYLE_CONTROL: u16 = 52;
    pub const STYLE: u16 = 53;
    pub const LTYPE_CONTROL: u16 = 56;
    pub const LTYPE: u16 = 57;
    pub const DIMSTYLE_CONTROL: u16 = 68;
    pub const DIMSTYLE: u16 = 69;
    pub const HATCH: u16 = 78;
    pub const SEQEND: u16 = 6;
    pub const VIEWPORT: u16 = 34;
    pub const LEADER: u16 = 45;
}

/// DWG を編集する。すべての操作を確かめてから書き、1 つでも通らなければエラーを返して何も書かない
pub fn edit_dwg(bytes: &[u8], operations: &[EditOperation]) -> Result<Edited, EditError> {
    if !crate::is_dwg(bytes) {
        return Err(EditError::new("unreadable", "DWG として読めません"));
    }
    if crate::r12::Old::from_magic(bytes).is_some() {
        return Err(EditError::new("unreadable", "AutoCAD Release 12 以前の DWG はまだ編集できません"));
    }
    let mut notices = NoticeList::new();
    let source = Source::open(bytes, &mut notices).map_err(|message| EditError::new("unreadable", message))?;
    if operations.is_empty() {
        // 操作のない編集は元のバイト列（docs/decisions/0012）
        return Ok(Edited { bytes: bytes.to_vec(), created: Vec::new(), notices: Vec::new() });
    }
    let mut editor = Editor::new(source, notices)?;
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            EditOperation::Add { kind, container, values } => editor.add(index, kind, container.as_deref(), values)?,
            EditOperation::Update { id, set } => editor.update(index, id, set)?,
            EditOperation::Delete { id } => editor.delete(index, id)?,
            EditOperation::AddLayer { name, values } => editor.add_layer(index, name, values, false)?,
            EditOperation::UpdateLayer { name, set } => editor.update_layer(index, name, set)?,
            EditOperation::AddBlock { name, values } => editor.add_block(index, name, values)?,
            EditOperation::AddStyle { kind, name, values } => editor.add_style(index, *kind, name, values)?,
            EditOperation::AddLayout { name, values } => editor.add_layout(index, name, values)?,
            EditOperation::UpdateLayout { name, set } => editor.update_layout(index, name, set)?,
        }
    }
    Ok(editor.finish())
}

/// 新しい図面（AutoCAD 2013 の DWG）。units は長さの単位（mm、cm、m、inch、foot、unitless）で、$INSUNITS と
/// $MEASUREMENT にする（DXF の新しい図面と同じ）
pub fn new_dwg(units: &str) -> Result<Vec<u8>, EditError> {
    let (insunits, measurement) = match units.trim().to_ascii_lowercase().as_str() {
        "mm" => (4.0, 1.0),
        "cm" => (5.0, 1.0),
        "m" => (6.0, 1.0),
        "inch" | "in" => (1.0, 0.0),
        "foot" | "ft" => (2.0, 0.0),
        "unitless" => (0.0, 1.0),
        other => {
            return Err(EditError::new(
                "invalid-value",
                format!("長さの単位 {other} は使えません（mm、cm、m、inch、foot、unitless）"),
            ));
        }
    };
    let mut doc = zumen_dxf::parse_dxf(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
    doc.header.insunits = insunits;
    doc.header.measurement = Some(measurement);
    Ok(crate::write_dwg(&doc).bytes)
}

fn upper(text: &str) -> String {
    text.trim().to_uppercase()
}

fn href(code: u8, value: u64) -> HandleRef {
    HandleRef { code, value }
}

/// 名前に使えない文字（AutoCAD の記号表の名前の規則。DXF の編集と同じ）
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 255
        && !name.chars().any(|c| c < ' ' || "<>/\\\":;?*|=`".contains(c))
}

/// 読んだオブジェクト
struct Decoded<T> {
    kind: u16,
    eed: Vec<(HandleRef, Vec<u8>)>,
    value: T,
}

/// 足す先: ブロック（BLOCK_HEADER）と図形のモード（2 モデル空間、1 紙空間、0 そのほかのブロック）
#[derive(Debug, Clone, Copy)]
struct Place {
    block: u64,
    mode: u8,
}

/// 編集する図形
struct Target {
    handle: u64,
    entity: Decoded<Entity>,
    name: String,
}

/// 図形の共通の値（指定されたものだけ）
struct Common {
    layer: Option<u64>,
    color: Option<EntityColor>,
    linetype: Option<(u8, Option<HandleRef>)>,
    lineweight: Option<u8>,
}

/// ヘッダ変数の書き換え方
enum HeaderForm {
    /// すべての変数を読めた。書くときに $HANDSEED を変えて書き直す
    Codec { vars: Box<HeaderVars>, extra: Option<[u8; 4]> },
    /// 読み取りが読む変数だけを読めた（テストの図面など）。$HANDSEED の値のバイトを同じ長さで書き換える
    Patch { at: (u64, u64) },
}

struct Editor {
    source: Source,
    version: Version,
    encoding: &'static Encoding,
    header: HeaderForm,
    /// 読み取りのヘッダ変数（表・辞書のハンドル）
    tables: crate::header::Header,
    /// クラスの番号 → (DXF の名前, 図形か)
    classes: HashMap<u16, (String, bool)>,
    /// ハンドル → オブジェクトの並び（Objects::items）の位置
    index: HashMap<u64, usize>,
    /// オブジェクトの並びの位置ごとの、それを指すハンドルの数
    sharing: HashMap<usize, usize>,
    seed: u64,
    seed_used: bool,
    layer_control: u64,
    /// 名前（大文字）→ ハンドル
    layers: HashMap<String, u64>,
    linetypes: HashMap<String, u64>,
    styles: HashMap<String, u64>,
    /// ブロックの名前（大文字）→ BLOCK_HEADER
    blocks: HashMap<String, u64>,
    /// 寸法スタイルの名前（大文字）→ DIMSTYLE
    dimstyles: HashMap<String, u64>,
    /// レイアウトの名前（大文字）→ BLOCK_HEADER
    layouts: HashMap<String, u64>,
    /// レイアウトの名前（大文字）→ LAYOUT と、ACAD_LAYOUT の辞書
    layout_objects: HashMap<String, u64>,
    layout_dictionary: u64,
    model: u64,
    paper: u64,
    deleted: HashSet<u64>,
    created: Vec<String>,
    notices: NoticeList,
}

fn handle_var(vars: &HeaderVars, name: &str) -> u64 {
    match vars.get(name) {
        Some(Value::Handle(handle)) => handle.value,
        _ => 0,
    }
}

/// buffer の at ビット目から、value の下位 bits ビットを上位から書く
fn put_bits(buffer: &mut [u8], at: u64, value: u64, bits: u32) {
    for index in 0..u64::from(bits) {
        let bit = (value >> (u64::from(bits) - 1 - index)) & 1 != 0;
        let position = at + index;
        let (byte, mask) = ((position / 8) as usize, 0x80u8 >> (position % 8));
        if let Some(target) = buffer.get_mut(byte) {
            *target = if bit { *target | mask } else { *target & !mask };
        }
    }
}

impl Editor {
    fn new(source: Source, notices: NoticeList) -> Result<Self, EditError> {
        let version = source.version;
        let unreadable = |what: &str| EditError::new("unreadable", format!("DWG の{what}が読めません"));
        let extra = has_extra(version, source.maintenance);
        let encoding = crate::document::text_encoding(source.codepage);
        let format = crate::objects::Format {
            version,
            maintenance: source.maintenance,
            codec: if version >= Version::R2007 {
                crate::bits::TextCodec::Utf16
            } else {
                crate::bits::TextCodec::Bytes(encoding)
            },
        };
        let tables = crate::header::read_header(&source.header, format).map_err(|_| unreadable("ヘッダ変数"))?;
        let header = match Framed::parse(&source.header, &HEADER_SENTINEL, &HEADER_END, extra)
            .and_then(|framed| Some((read_header_vars(&framed.data, version).ok()?, framed.extra)))
        {
            Some((vars, extra)) => HeaderForm::Codec { vars: Box::new(vars), extra },
            None => HeaderForm::Patch { at: tables.handseed_at },
        };
        let classes = crate::objects::read_classes(&source.classes, format)
            .into_iter()
            .map(|class| (class.number, (class.dxf_name, class.entity)))
            .collect();
        let index: HashMap<u64, usize> = source.objects.handles.iter().map(|(handle, item)| (*handle, *item)).collect();
        let mut sharing: HashMap<usize, usize> = HashMap::new();
        for item in index.values() {
            *sharing.entry(*item).or_default() += 1;
        }
        let largest = index.keys().copied().max().unwrap_or(0);
        let seed = tables.handseed.max(largest.saturating_add(1));
        let mut editor = Self {
            version,
            encoding,
            classes,
            index,
            sharing,
            seed,
            seed_used: false,
            layer_control: tables.layer_control,
            layers: HashMap::new(),
            linetypes: HashMap::new(),
            styles: HashMap::new(),
            blocks: HashMap::new(),
            dimstyles: HashMap::new(),
            layouts: HashMap::new(),
            layout_objects: HashMap::new(),
            layout_dictionary: 0,
            model: tables.model_space,
            paper: tables.paper_space,
            deleted: HashSet::new(),
            created: Vec::new(),
            notices,
            header,
            tables,
            source,
        };
        editor.index_tables();
        Ok(editor)
    }

    // ---- オブジェクトの読み書き ----

    fn bytes(&self, handle: u64) -> Option<&[u8]> {
        if self.deleted.contains(&handle) {
            return None;
        }
        self.index.get(&handle).and_then(|&item| self.source.objects.items.get(item)).map(Vec::as_slice)
    }

    /// オブジェクトを読む
    fn read<T>(&self, handle: u64, read: impl FnOnce(&mut In<'_>, u16) -> Read<T>) -> Option<Decoded<T>> {
        let mut raw = open_raw(self.bytes(handle)?, 0, self.version).ok()?;
        if raw.handle != handle {
            return None;
        }
        let value = read(&mut raw.input, raw.kind).ok()?;
        Some(Decoded { kind: raw.kind, eed: raw.eed, value })
    }

    /// 型の名前（固定の型か、クラスの DXF の名前）と、図形か
    fn kind_of(&self, kind: u16) -> Option<(String, bool)> {
        if let Some(name) = fixed_name(kind) {
            let entity = matches!(kind, 1..=41 | 43..=47 | 74 | 76..=78 | 0x1f2);
            return Some((name.to_string(), entity));
        }
        self.classes.get(&kind).cloned()
    }

    /// 図形を読む（読み書きできる種類だけ）
    fn read_entity(&self, handle: u64) -> Option<(Decoded<Entity>, String)> {
        let raw = open_raw(self.bytes(handle)?, 0, self.version).ok()?;
        let (name, entity) = self.kind_of(raw.kind)?;
        if !entity {
            return None;
        }
        let decoded = self.read(handle, |s, _| Entity::read(s, &name))?;
        let value = decoded.value?;
        Some((Decoded { kind: decoded.kind, eed: decoded.eed, value }, name))
    }

    /// オブジェクトを書く（なければ足す）。ほかのハンドルと同じオブジェクトを指していたら、別のオブジェクトにする
    fn store(&mut self, handle: u64, kind: u16, eed: &[(HandleRef, Vec<u8>)], write: impl FnOnce(&mut ObjectWriter)) {
        let mut out = writer(self.version, kind, handle, eed);
        write(&mut out);
        let bytes = out.bytes();
        let objects = &mut self.source.objects;
        match self.index.get(&handle).copied() {
            Some(item) if self.sharing.get(&item).copied().unwrap_or(1) <= 1 => objects.items[item] = bytes,
            existing => {
                if let Some(item) = existing
                    && let Some(count) = self.sharing.get_mut(&item)
                {
                    *count -= 1;
                }
                let item = objects.items.len();
                objects.items.push(bytes);
                objects.handles.retain(|(other, _)| *other != handle);
                objects.handles.push((handle, item));
                self.index.insert(handle, item);
                self.sharing.insert(item, 1);
            }
        }
    }

    fn store_entity(&mut self, handle: u64, decoded: &Decoded<Entity>) {
        self.store(handle, decoded.kind, &decoded.eed, |out| decoded.value.write(out));
    }

    /// 新しいハンドル。$HANDSEED をその場で書き換える図面は、元の値のバイトの数に収まる間だけ振れる
    fn next_handle(&mut self, index: usize) -> Result<u64, EditError> {
        let next = self
            .seed
            .checked_add(1)
            .ok_or_else(|| EditError::at(index, "handle-exhausted", "DWG に追加する図形のハンドルを確保できません"))?;
        if let HeaderForm::Patch { at } = self.header {
            let bytes = (at.1.saturating_sub(at.0) / 8).saturating_sub(1);
            if bytes < 8 && next >> (bytes * 8) != 0 {
                return Err(EditError::at(
                    index,
                    "unreadable",
                    "DWG のヘッダ変数を読み切れないため、これより多くのオブジェクトは足せません",
                ));
            }
        }
        let handle = self.seed;
        self.seed = next;
        self.seed_used = true;
        Ok(handle)
    }

    // ---- 文字列 ----

    /// 読み取りと同じ名前（キャレット表記、\U+XXXX を文字に）。R2007 より前の AutoCAD は文字列の終わりに NUL を
    /// 置く（長さに数える）ので除く
    fn text_of(&self, text: &RawText) -> String {
        let raw = match text {
            RawText::Units(units) => String::from_utf16_lossy(units),
            RawText::Bytes(bytes) => self.encoding.decode_without_bom_handling(bytes).0.into_owned(),
        };
        let raw = raw.trim_end_matches('\0').to_string();
        zumen_dxf::decode_escapes(&crate::objects::caret_notation(raw))
    }

    /// 文字列を図面の形にする。R2007 より前は図面の文字コードで、表せない文字は \U+XXXX、終わりに NUL（どちらも
    /// AutoCAD と同じ）
    fn raw_text(&self, text: &str) -> RawText {
        if self.version >= Version::R2007 {
            return RawText::Units(text.encode_utf16().collect());
        }
        let mut out = Vec::with_capacity(text.len());
        let mut buffer = [0u8; 4];
        let mut units = [0u16; 2];
        for c in text.chars() {
            let (bytes, _, unmappable) = self.encoding.encode(c.encode_utf8(&mut buffer));
            if unmappable {
                for unit in c.encode_utf16(&mut units) {
                    out.extend_from_slice(format!("\\U+{unit:04X}").as_bytes());
                }
            } else {
                out.extend_from_slice(&bytes);
            }
        }
        out.push(0);
        RawText::Bytes(out)
    }

    // ---- 表 ----

    /// 表の管理のオブジェクトの項目（と後に続くハンドル）
    fn control(&self, handle: u64) -> Vec<u64> {
        self.read(handle, Control::read)
            .map(|control| {
                control.value.entries.iter().chain(&control.value.extra).map(|entry| entry.resolve(handle)).collect()
            })
            .unwrap_or_default()
    }

    /// 名前の表、ブロック、レイアウトを集める
    fn index_tables(&mut self) {
        for handle in self.control(self.layer_control) {
            if let Some(layer) = self.read(handle, |s, _| Layer::read(s)) {
                let name = self.text_of(&layer.value.name.name);
                self.layers.entry(upper(&name)).or_insert(handle);
            }
        }
        for handle in self.control(self.tables.linetype_control) {
            if let Some(linetype) = self.read(handle, |s, _| Linetype::read(s)) {
                let name = self.text_of(&linetype.value.name.name);
                self.linetypes.entry(upper(&name)).or_insert(handle);
            }
        }
        for handle in self.control(self.tables.style_control) {
            if let Some(style) = self.read(handle, |s, _| TextStyle::read(s)) {
                let name = self.text_of(&style.value.name.name);
                if !name.trim().is_empty() && !style.value.shape {
                    self.styles.entry(upper(&name)).or_insert(handle);
                }
            }
        }
        for handle in self.control(self.tables.dimstyle_control) {
            if let Some(style) = self.read(handle, |s, _| DimStyle::read(s)) {
                let name = self.text_of(&style.value.name.name);
                self.dimstyles.entry(upper(&name)).or_insert(handle);
            }
        }
        // ブロックの名前は BLOCK の図形のもの（BLOCK_HEADER の名前は、紙空間はどれも *Paper_Space、無名のブロックは
        // *U などの頭だけ。読み取りと同じ）
        for handle in self.control(self.tables.block_control) {
            if let Some(header) = self.read(handle, |s, _| BlockHeader::read(s)) {
                let block = header.value.block.resolve(handle);
                let name = match self.read_entity(block).map(|(entity, _)| entity.value.body) {
                    Some(Body::Block(name)) => self.text_of(&name),
                    _ => self.text_of(&header.value.name.name),
                };
                self.blocks.entry(upper(&name)).or_insert(handle);
            }
        }
        // LAYOUT は R2000 から。AutoCAD 2000 以降が R13・R14 に保存した図面は、名前付きオブジェクトの辞書の
        // ACAD_LAYOUT に持つ（読み取りと同じ）
        let layouts = if self.version >= Version::R2000 {
            self.tables.layouts
        } else {
            let named = self.tables.named_objects;
            self.read(named, |s, _| Dictionary::read(s, false))
                .and_then(|dictionary| {
                    dictionary
                        .value
                        .entries
                        .iter()
                        .find(|(name, _)| upper(&self.text_of(name)) == "ACAD_LAYOUT")
                        .map(|(_, entry)| entry.resolve(named))
                })
                .unwrap_or(0)
        };
        self.layout_dictionary = layouts;
        if let Some(dictionary) = self.read(layouts, |s, _| Dictionary::read(s, false)) {
            for (_, entry) in &dictionary.value.entries {
                let handle = entry.resolve(layouts);
                if let Some(layout) = self.read(handle, |s, _| Layout::read(s)) {
                    let name = self.text_of(&layout.value.name);
                    self.layouts.insert(upper(&name), layout.value.block_record.resolve(handle));
                    self.layout_objects.insert(upper(&name), handle);
                }
            }
        }
    }

    /// 図形の線種（R2000 以降の BB と、3 のときのハンドル）。R13・R14 は BYLAYER のビットだけを持つので、BYBLOCK は
    /// 線種の表の BYBLOCK の項目を指す（AutoCAD の R14 の図面と同じ）
    fn line_type(&self, index: usize, name: &str) -> Result<(u8, Option<HandleRef>), EditError> {
        match upper(name).as_str() {
            "BYLAYER" => Ok((0, None)),
            "BYBLOCK" if self.version < Version::R2000 => self
                .linetypes
                .get("BYBLOCK")
                .map(|handle| (3, Some(href(5, *handle))))
                .ok_or_else(|| EditError::at(index, "missing-definition", "線種 BYBLOCK は図面にありません")),
            "BYBLOCK" => Ok((1, None)),
            key => {
                self.linetypes.get(key).map(|handle| (3, Some(href(5, *handle)))).ok_or_else(|| {
                    EditError::at(index, "missing-definition", format!("線種 {name} は図面にありません"))
                })
            }
        }
    }

    fn text_style(&self, index: usize, name: Option<&str>) -> Result<Option<u64>, EditError> {
        match name {
            None => Ok(None),
            Some(name) => self.styles.get(&upper(name)).copied().map(Some).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("文字スタイル {name} は図面にありません"))
            }),
        }
    }

    /// 新しい文字の文字スタイル（STANDARD、なければ $TEXTSTYLE）
    fn default_style(&self) -> u64 {
        self.styles.get("STANDARD").copied().unwrap_or_else(|| match &self.header {
            HeaderForm::Codec { vars, .. } => handle_var(vars, "$TEXTSTYLE"),
            HeaderForm::Patch { .. } => 0,
        })
    }

    /// レイヤのハンドル。なければ既定の値で足して知らせる
    fn layer(&mut self, index: usize, name: &str) -> Result<u64, EditError> {
        if !valid_name(name) {
            return Err(EditError::at(index, "invalid-value", format!("レイヤの名前 {name} は使えません")));
        }
        if let Some(handle) = self.layers.get(&upper(name)) {
            return Ok(*handle);
        }
        self.add_layer(index, name, &EditValues::new(), true)?;
        self.notices.add(NoticeLevel::Info, "edit-added-layer", "図面になかったレイヤを既定の値で足しました");
        Ok(self.layers[&upper(name)])
    }

    // ---- 値の書き方 ----

    fn no_lineweight(&mut self) {
        self.notices.add(
            NoticeLevel::Info,
            "edit-no-lineweight",
            "AutoCAD 2000 より前の DWG には線の太さがないため、書きませんでした",
        );
    }

    /// 線の太さの番号（R2000 以降の RC。29 BYLAYER、30 BYBLOCK、31 既定）
    fn lineweight(&mut self, width: EditLineWidth) -> u8 {
        match width {
            EditLineWidth::ByLayer => 29,
            EditLineWidth::ByBlock => 30,
            EditLineWidth::Default => 31,
            EditLineWidth::Millimeters(mm) => {
                let wanted = mm * 100.0;
                let (index, nearest) = LINEWEIGHTS
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (*a - wanted).abs().total_cmp(&(*b - wanted).abs()))
                    .map_or((31, 0.0), |(index, weight)| (index as u8, *weight));
                if (nearest - wanted).abs() > 0.5 {
                    self.notices.add(
                        NoticeLevel::Info,
                        "edit-rounded-lineweight",
                        "線の太さを、DWG に書ける最も近い太さにしました",
                    );
                }
                index
            }
        }
    }

    /// ACI と、R2004 以降のトゥルーカラー（0xC2RRGGBB）。R2000 は最も近い ACI にして知らせる
    fn color_parts(&mut self, index: usize, color: EditColor) -> Result<(u16, Option<u32>), EditError> {
        Ok(match color {
            EditColor::ByLayer | EditColor::Index(256) => (256, None),
            EditColor::ByBlock | EditColor::Index(0) => (0, None),
            EditColor::Index(value @ 1..=255) => (value as u16, None),
            EditColor::Index(value) => {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("DWG の色の番号（ACI）は 1〜255 です: {value}"),
                ));
            }
            EditColor::Rgb(rgb) => {
                let aci = nearest_aci(rgb);
                if self.version >= Version::R2004 {
                    (aci, Some(0xc200_0000 | (rgb & 0xff_ffff)))
                } else {
                    self.notices.add(
                        NoticeLevel::Info,
                        "edit-nearest-color",
                        "AutoCAD 2004 より前の DWG にはトゥルーカラーがないため、最も近い色の番号（ACI）にしました",
                    );
                    (aci, None)
                }
            }
        })
    }

    /// 図形の色（ENC）。transparency は元の透過度（0x2000 のとき）
    fn entity_color(
        &mut self,
        index: usize,
        color: EditColor,
        transparency: Option<u32>,
    ) -> Result<EntityColor, EditError> {
        let (aci, rgb) = self.color_parts(index, color)?;
        let mut value = match rgb {
            Some(_) => 0x8000 | (aci & 0xff),
            None => aci,
        };
        if transparency.is_some() && self.version >= Version::R2004 {
            value |= 0x2000;
        }
        Ok(EntityColor { value, rgb, transparency })
    }

    /// 表の項目の色（CMC）。R2004 以降は AutoCAD と同じく番号を 0、RGB の値に種類（0xC3 ACI、0xC2 RGB）を持つ
    fn cmc(&self, aci: u16, rgb: Option<u32>) -> CmColor {
        if self.version < Version::R2004 {
            return CmColor { index: aci as i16, rgb: 0, name: None, book: None };
        }
        let raw = match (aci, rgb) {
            (_, Some(rgb)) => rgb,
            (0, None) => 0xc100_0000,
            (256, None) => 0xc000_0000,
            (aci, None) => 0xc300_0000 | u32::from(aci),
        };
        CmColor { index: 0, rgb: raw, name: None, book: None }
    }

    fn common(
        &mut self,
        index: usize,
        reader: &ValueReader<'_>,
        transparency: Option<u32>,
    ) -> Result<Common, EditError> {
        let layer = match reader.name("layer")? {
            Some(name) => Some(self.layer(index, name)?),
            None => None,
        };
        let linetype = match reader.name("lineType")? {
            Some(name) => Some(self.line_type(index, name)?),
            None => None,
        };
        let color = match reader.color("color")? {
            Some(color) => Some(self.entity_color(index, color, transparency)?),
            None => None,
        };
        let lineweight = match reader.line_width("lineWidth")? {
            // R13・R14 の図形の線の太さは BYLAYER だけ
            Some(width) if self.version < Version::R2000 => {
                if width != EditLineWidth::ByLayer {
                    self.no_lineweight();
                }
                None
            }
            Some(width) => Some(self.lineweight(width)),
            None => None,
        };
        Ok(Common { layer, color, linetype, lineweight })
    }

    /// 共通の値を図形に当てる
    fn apply_common(common: &Common, entity: &mut EntityCommon) {
        if let Some(layer) = common.layer {
            entity.layer = href(5, layer);
        }
        if let Some((flags, handle)) = common.linetype {
            entity.ltype_flags = flags;
            entity.ltype = handle;
        }
        if let Some(color) = &common.color {
            entity.color = *color;
            entity.book_color = None;
        }
        if let Some(weight) = common.lineweight {
            entity.lineweight = weight;
        }
    }

    // ---- 足す先・図形 ----

    /// 足す先。省略・model はモデル空間、layout:<名前> はレイアウトの紙空間、block:<名前> はブロック定義
    fn place(&self, index: usize, container: Option<&str>) -> Result<Place, EditError> {
        let block_place = |block: u64| -> Place {
            if block == self.model {
                Place { block, mode: 2 }
            } else if block == self.paper {
                Place { block, mode: 1 }
            } else {
                Place { block, mode: 0 }
            }
        };
        let container = container.map(str::trim).unwrap_or("model");
        if container.eq_ignore_ascii_case("model") {
            return Ok(block_place(self.model));
        }
        if let Some(name) = container.strip_prefix("layout:") {
            if name.eq_ignore_ascii_case("model") {
                return Ok(block_place(self.model));
            }
            // LAYOUT のない R13・R14 の図面の紙空間は 1 つ（DXF の R12 の編集と同じく、どの名前もその紙空間）
            if self.layouts.is_empty() && self.version < Version::R2000 {
                return Ok(block_place(self.paper));
            }
            return self.layouts.get(&upper(name)).map(|block| block_place(*block)).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("レイアウト {name} は図面にありません"))
            });
        }
        if let Some(name) = container.strip_prefix("block:") {
            return self.blocks.get(&upper(name)).map(|block| block_place(*block)).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))
            });
        }
        Err(EditError::at(
            index,
            "invalid-value",
            format!("container の {container} は使えません（model、layout:<名前>、block:<名前>）"),
        ))
    }

    /// 新しい図形の共通の値
    fn new_common(&mut self, index: usize, place: Place, reader: &ValueReader<'_>) -> Result<EntityCommon, EditError> {
        let given = self.common(index, reader, None)?;
        let layer = match given.layer {
            Some(layer) => layer,
            None => self.layer(index, "0")?,
        };
        let r2004 = self.version >= Version::R2004;
        let mut common = EntityCommon {
            graphic: None,
            mode: place.mode,
            owner: (place.mode == 0).then_some(href(4, place.block)),
            reactors: Vec::new(),
            xdictionary: if r2004 { None } else { Some(href(3, 0)) },
            ds_binary: false,
            links: None,
            color: EntityColor::index(256),
            book_color: None,
            linetype_scale: 1.0,
            layer: href(5, layer),
            ltype_flags: 0,
            ltype: None,
            plotstyle_flags: 0,
            plotstyle: None,
            material_flags: 0,
            material: None,
            shadow: 0,
            visual_styles: [None; 3],
            invisible: 0,
            lineweight: 29,
        };
        Self::apply_common(&given, &mut common);
        Ok(common)
    }

    /// 図形をブロックの終わりに足す。R2004 以降は BLOCK_HEADER の図形の並び、R2000 は前後のつながりと最後の図形
    fn append(&mut self, index: usize, place: Place, handle: u64, entity: &mut EntityCommon) -> Result<(), EditError> {
        let unreadable = || EditError::at(index, "unreadable", "図形を足すブロック（BLOCK_HEADER）が読めません");
        let mut header = self.read(place.block, |s, _| BlockHeader::read(s)).ok_or_else(unreadable)?;
        if self.version >= Version::R2004 {
            header.value.entities.push(href(3, handle));
        } else {
            let (first, last) = header.value.first_last.ok_or_else(unreadable)?;
            let chain = self.chain(first.resolve(place.block), last.resolve(place.block)).ok_or_else(unreadable)?;
            match chain.last() {
                None => {
                    entity.links = Some((href(4, 0), href(4, 0)));
                    header.value.first_last = Some((href(4, handle), href(4, handle)));
                }
                Some(&last) => {
                    let before = chain.len().checked_sub(2).map_or(0, |at| chain[at]);
                    self.relink(index, last, before, handle)?;
                    entity.links = Some((href(4, last), href(4, 0)));
                    header.value.first_last = Some((href(4, chain[0]), href(4, handle)));
                }
            }
        }
        let (kind, eed) = (header.kind, header.eed.clone());
        self.store(place.block, kind, &eed, |out| header.value.write(out));
        Ok(())
    }

    /// 図形の共通の値（どの種類の図形でも読める）
    fn entity_common(&self, handle: u64) -> Option<EntityCommon> {
        let mut raw = open_raw(self.bytes(handle)?, 0, self.version).ok()?;
        EntityCommon::read(&mut raw.input).ok()
    }

    /// R2000 のブロックの図形のつながり。最初の図形から次の図形をたどって最後の図形まで（Nolinks の図形は次の
    /// ハンドル）。たどれなければ None
    fn chain(&self, first: u64, last: u64) -> Option<Vec<u64>> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut current = first;
        while current != 0 {
            if !seen.insert(current) || out.len() > self.index.len() {
                return None;
            }
            out.push(current);
            if current == last {
                return Some(out);
            }
            current = linked_next(&self.entity_common(current)?, current);
        }
        (last == 0 || out.is_empty()).then_some(out)
    }

    /// R2000 の図形の前と次の図形を書き換える（書き換える図形は読み書きできる種類だけ）
    fn relink(&mut self, index: usize, handle: u64, previous: u64, next: u64) -> Result<(), EditError> {
        let (mut entity, name) = self.read_entity(handle).ok_or_else(|| {
            EditError::at(
                index,
                "unsupported-kind",
                format!("隣の図形 {handle:X} が読めないため、図形のつながりを直せません"),
            )
        })?;
        let _ = name;
        entity.value.common.links = Some((href(4, previous), href(4, next)));
        self.store_entity(handle, &entity);
        Ok(())
    }

    /// 足す LWPOLYLINE の型。R2000 以降は決まった型 77、R14 はクラスの型（図面のクラスになければ足す）。R13 には
    /// LWPOLYLINE がない
    fn lwpolyline_kind(&mut self, index: usize) -> Result<u16, EditError> {
        match self.version {
            Version::R13 => Err(EditError::at(
                index,
                "unsupported-kind",
                "AutoCAD R13 の DWG には LWPOLYLINE がないため、折線は足せません",
            )),
            Version::R14 => self.class_number(index, "LWPOLYLINE"),
            _ => Ok(kind::LWPOLYLINE),
        }
    }

    /// クラスの番号。図面のクラス（AcDb:Classes）になければ、最後の番号の次の番号で足す。クラスの節を読み直して同じ
    /// ビットに書けるときだけ書き換える（書けなければ足さずにエラー）
    fn class_number(&mut self, index: usize, name: &str) -> Result<u16, EditError> {
        if let Some((&number, _)) = self.classes.iter().find(|(_, (dxf, _))| dxf.eq_ignore_ascii_case(name)) {
            return Ok(number);
        }
        let unreadable = || {
            EditError::at(index, "unreadable", format!("DWG のクラスの節が読めないため、{name} のクラスを足せません"))
        };
        let extra = has_extra(self.version, self.source.maintenance);
        let framed =
            Framed::parse(&self.source.classes, &CLASSES_SENTINEL, &CLASSES_END, extra).ok_or_else(unreadable)?;
        let codec = crate::bits::TextCodec::Bytes(self.encoding);
        let mut defs = read_class_defs(&framed.data, self.version, codec);
        // 読んだクラスを書いたビットが元の節と同じ（終わりの埋めを除く）か
        let bits = classes_bits(self.version, self.encoding, &defs);
        let length = bits.bit_len();
        let written = bits.into_bytes();
        let bit = |bytes: &[u8], at: u64| bytes.get((at / 8) as usize).map(|byte| byte & (0x80 >> (at % 8)) != 0);
        let same = (0..length).all(|at| bit(&written, at) == bit(&framed.data, at));
        if defs.len() != self.classes.len() || !same || framed.data.len() as u64 * 8 - length >= 8 {
            return Err(unreadable());
        }
        let number = defs.iter().map(|class| class.number).max().unwrap_or(499).max(499) + 1;
        let class = crate::write::drawing::class_def(name, number, 0).ok_or_else(unreadable)?;
        let entity = class.item_class == 0x1f2;
        defs.push(class);
        let data = classes_data(self.version, self.encoding, &defs);
        // R2004 以降のクラスの節は、終わりの区切りの後に 8 バイトの 0 が続く（10.1）
        let trailing = if self.version >= Version::R2004 { 8 } else { 0 };
        self.source.classes =
            Framed { extra: framed.extra, data, crc_ok: true }.bytes(&CLASSES_SENTINEL, &CLASSES_END, trailing);
        self.classes.insert(number, (name.to_string(), entity));
        Ok(number)
    }

    fn add(&mut self, index: usize, kind: &str, container: Option<&str>, values: &EditValues) -> Result<(), EditError> {
        if !ADDABLE.contains(&kind) {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                format!("DWG の編集では {kind} を足せません（足せるのは {}）", ADDABLE.join("、")),
            ));
        }
        let reader = ValueReader::new(values, index);
        let keys = |own: &[&'static str]| -> Vec<&'static str> { COMMON.iter().chain(own).copied().collect() };
        let place = self.place(index, container)?;
        // ブロック参照に書く属性と、その属性の定義の値
        let mut pending: Vec<(Placed, EntityCommon, Attribute)> = Vec::new();
        // 足す画像と、その定義・リアクタのハンドル
        let mut image_objects: Option<(zumen_dxf::image::ImageInput, u64, u64)> = None;
        let (type_number, body) = match kind {
            "line" => {
                reader.allow(&keys(&["start", "end"]), "線分")?;
                let start = reader.required("start", reader.point("start")?)?;
                let end = reader.required("end", reader.point("end")?)?;
                (kind::LINE, build::line(start, end))
            }
            "circle" | "arc" => {
                let own: &[&'static str] =
                    if kind == "arc" { &["center", "radius", "angle"] } else { &["center", "radius"] };
                reader.allow(&keys(own), if kind == "arc" { "円弧" } else { "円" })?;
                let center = reader.required("center", reader.point("center")?)?;
                let radius = reader.required("radius", reader.positive("radius")?)?;
                if kind == "arc" {
                    let angle = reader.required("angle", reader.numbers("angle", 2)?)?;
                    (kind::ARC, build::arc(center, radius, [angle[0], angle[1]]))
                } else {
                    (kind::CIRCLE, build::circle(center, radius))
                }
            }
            "ellipse" | "ellipse-arc" => {
                reader.allow(&keys(&["center", "majorAxis", "ratio", "parameter"]), "楕円")?;
                let center = reader.required("center", reader.point("center")?)?;
                let axis = reader.required("majorAxis", reader.point("majorAxis")?)?;
                let ratio = reader.required("ratio", reader.positive("ratio")?)?;
                if ratio > 1.0 || axis == [0.0; 3] {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        "ratio は 0 より大きく 1 以下、majorAxis は 0 でない長さにしてください",
                    ));
                }
                let parameter = match reader.numbers("parameter", 2)? {
                    Some(parameter) => [parameter[0], parameter[1]],
                    None if kind == "ellipse" => [0.0, std::f64::consts::TAU],
                    None => return Err(EditError::at(index, "invalid-value", "parameter がありません")),
                };
                (kind::ELLIPSE, build::ellipse(center, axis, ratio, parameter))
            }
            "point" => {
                reader.allow(&keys(&["position"]), "点")?;
                let position = reader.required("position", reader.point("position")?)?;
                (kind::POINT, build::point(position))
            }
            "polyline" => {
                reader.allow(&keys(&["vertices", "closed", "bulges", "width"]), "折線")?;
                let vertices = reader.required("vertices", reader.points("vertices", 2)?)?;
                let bulges = match reader.numbers("bulges", vertices.len())? {
                    Some(bulges) => bulges,
                    None if reader.has("bulges") => {
                        return Err(EditError::at(index, "invalid-value", "bulges は頂点と同じ数にしてください"));
                    }
                    None => vec![0.0; vertices.len()],
                };
                let elevation = vertices[0][2];
                if vertices.iter().any(|vertex| vertex[2] != elevation) {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        "polyline の頂点の z はすべて同じにしてください",
                    ));
                }
                let closed = reader.bool("closed")?.unwrap_or(false);
                let width = reader.number("width")?;
                (self.lwpolyline_kind(index)?, build::lwpolyline(&vertices, &bulges, closed, width))
            }
            "text" => {
                let mut own = vec!["position", "text", "height", "rotation", "textStyle", "wrapWidth", "widthFactor"];
                own.extend(zumen_dxf::placement::KEYS);
                reader.allow(&keys(&own), "文字")?;
                let placement = zumen_dxf::placement::read(&reader, index)?;
                let width_factor = zumen_dxf::placement::width_factor(&reader)?;
                let position = reader.required("position", reader.point("position")?)?;
                let text = reader.required("text", reader.text("text")?)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let style = self.text_style(index, reader.name("textStyle")?)?.unwrap_or_else(|| self.default_style());
                let wrap = reader.number("wrapWidth")?;
                if text.contains('\n') || wrap.is_some() || placement.multiline {
                    let content =
                        self.raw_text(&zumen_dxf::placement::with_width_factor(mtext_content(text), width_factor));
                    let attachment = placement.attachment as i16;
                    let mut body =
                        build::mtext(self.version, position, content, height, rotation, wrap, style, attachment);
                    if let (Body::MText(mtext), Some(factor)) = (&mut body, placement.line_spacing) {
                        mtext.line_spacing_factor = factor;
                        // MTEXT の行間は R2000 から（20.4.46）
                        if self.version < Version::R2000 {
                            self.notices.add(
                                NoticeLevel::Info,
                                "edit-no-line-spacing",
                                "R13・R14 の MTEXT には行間がないため、書きませんでした",
                            );
                        }
                    }
                    (kind::MTEXT, body)
                } else {
                    let value = self.raw_text(text);
                    let mut body = build::text(position, value, height, rotation, style);
                    if let Body::Text(text) = &mut body {
                        text.width = width_factor;
                    }
                    if let Body::Text(text) = &mut body
                        && placement.aligned()
                    {
                        text.halign = Some(placement.horizontal as i16);
                        text.valign = Some(placement.vertical as i16);
                        text.align = Some([position[0], position[1]]);
                    }
                    (kind::TEXT, body)
                }
            }
            "dimension" => {
                reader.allow(&keys(&dimension::KEYS), "寸法")?;
                self.dimension(index, &reader)?
            }
            "hatch" => {
                reader.allow(&keys(&zumen_dxf::hatch::KEYS), "ハッチング")?;
                let input = zumen_dxf::hatch::read(&reader, index)?;
                // HATCH は R14 から（R14 はクラスの型、R2000 から決まった型 78。モデルの書き出しと同じ）
                let type_number = match self.version {
                    Version::R13 => {
                        return Err(EditError::at(
                            index,
                            "unsupported-kind",
                            "AutoCAD R13 の DWG にはハッチング（HATCH）がありません",
                        ));
                    }
                    Version::R14 => self.class_number(index, "HATCH")?,
                    _ => kind::HATCH,
                };
                (type_number, build::hatch(&input, self.raw_text(&input.pattern)))
            }
            "leader" => {
                reader.allow(&keys(&zumen_dxf::leader::KEYS), "引出線")?;
                let (vertices, height) = zumen_dxf::leader::read(&reader)?;
                let text = reader.text("text")?.unwrap_or("");
                let text_style = self.text_style(index, reader.name("textStyle")?)?;
                let (style_handle, style, dimension_text_style) =
                    self.dimension_style(index, reader.name("dimensionStyle")?)?;
                let annotation = zumen_dxf::leader::annotation(&vertices, height, &style);
                // 注釈の MTEXT を先に足し、LEADER（20.4.47）が指す
                let mut annotation_handle = 0;
                if !text.is_empty() {
                    let content = self.raw_text(&mtext_content(text));
                    let text_style = text_style.unwrap_or(dimension_text_style);
                    let attachment = annotation.attachment as i16;
                    let body = build::mtext(
                        self.version,
                        annotation.position,
                        content,
                        annotation.height,
                        0.0,
                        None,
                        text_style,
                        attachment,
                    );
                    let mut common = self.new_common(index, place, &reader)?;
                    annotation_handle = self.next_handle(index)?;
                    self.append(index, place, annotation_handle, &mut common)?;
                    let entity = Decoded { kind: kind::MTEXT, eed: Vec::new(), value: Entity { common, body } };
                    self.store_entity(annotation_handle, &entity);
                }
                let factor = if style.scale > 0.0 { style.scale } else { 1.0 };
                // LEADER（20.4.47）の Hooklineonxdir。左の注釈は +X に戻ってから最後の頂点へ進む。
                let hook_on_x = annotation.attachment == 6;
                // 注釈つきの DWG は読み取りで水平のフックを補う（entities.rs の推定）。最後の線分に収まらないと
                // 指定した折線と形が変わるので、黙って書かず、変わる引出線の数を知らせる。
                let last = vertices[vertices.len() - 1];
                let before = vertices[vertices.len() - 2];
                let hook_length = style.arrow_size * if style.scale == 0.0 { 1.0 } else { style.scale };
                if !text.is_empty()
                    && hook_length > 0.0
                    && (last[1] != before[1] || last[2] != before[2] || (last[0] - before[0]).abs() < hook_length)
                {
                    self.notices.add(
                        NoticeLevel::Info,
                        "edit-leader-hook",
                        "DWG の注釈つき引出線は、終端に水平なフック線を補って描きます",
                    );
                }
                let body = Leader {
                    unknown_bit: false,
                    annotation_type: if text.is_empty() { 3 } else { 0 },
                    path_type: 0,
                    points: vertices.clone(),
                    origin: vertices[0],
                    extrusion: [0.0, 0.0, 1.0],
                    x_direction: [1.0, 0.0, 0.0],
                    block_offset: [0.0; 3],
                    end_projection: [0.0; 3],
                    box_size: None,
                    hook_on_x,
                    arrowhead: true,
                    unknown_short: 0,
                    unknown_bits: [false, false],
                    // R13・R14 は作ったときの DIMGAP・DIMASZ に DIMSCALE を掛けた値を持つ（モデルの書き出しと同じ）
                    r14: (self.version < Version::R2000).then_some(LeaderR14 {
                        dimgap: style.gap * factor,
                        arrowhead_type: 0,
                        dimasz: style.arrow_size * factor,
                        unknown: (false, true, 256),
                        byblock_color: 256,
                        unknown_bits: [false, false],
                    }),
                    annotation: href(5, annotation_handle),
                    dimstyle: href(5, style_handle),
                };
                (kind::LEADER, Body::Leader(body))
            }
            "image" => {
                if self.version < Version::R14 {
                    return Err(EditError::at(
                        index,
                        "unsupported-kind",
                        "AutoCAD R14 より前の DWG には画像（IMAGE）を足せません",
                    ));
                }
                reader.allow(&keys(&zumen_dxf::image::KEYS), "画像")?;
                let input = zumen_dxf::image::read(&reader, index)?;
                let type_number = self.class_number(index, "IMAGE")?;
                let definition = self.next_handle(index)?;
                let reactor = self.next_handle(index)?;
                let [width, height] = input.pixels;
                // IMAGE（20.4.80）。値は DXF の編集と同じ
                let body = Image {
                    class_version: 0,
                    insert: input.position,
                    u_vector: input.u_pixel,
                    v_vector: input.v_pixel,
                    size: input.pixels,
                    display: 7,
                    clipping: false,
                    brightness: 50,
                    contrast: 50,
                    fade: 0,
                    clip_mode: false,
                    clip_type: 1,
                    boundary: vec![[-0.5, -0.5], [width - 0.5, height - 0.5]],
                    imagedef: href(5, definition),
                    reactor: href(3, reactor),
                };
                image_objects = Some((input, definition, reactor));
                (type_number, Body::Image(Box::new(body)))
            }
            "viewport" => {
                reader.allow(&keys(&["center", "width", "height", "viewCenter", "scale"]), "ビューポート")?;
                self.viewport_place(index, place)?;
                let center = reader.required("center", reader.point("center")?)?;
                let width = reader.required("width", reader.positive("width")?)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let view_center = reader.required("viewCenter", reader.point("viewCenter")?)?;
                let scale = reader.positive("scale")?.unwrap_or(1.0);
                let sheet = RawText::empty(self.version);
                let view = zumen_dxf::view_height(height, scale, self.tables.insunits);
                let body = build::viewport(self.version, center, width, height, view_center, view, sheet);
                (kind::VIEWPORT, body)
            }
            "attribute-definition" => {
                let mut own = attributes::DEFINITION_KEYS.to_vec();
                own.push("justification");
                reader.allow(&keys(&own), "属性の定義")?;
                if !container.is_some_and(|name| name.trim().starts_with("block:")) || place.mode != 0 {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        "属性の定義（attribute-definition）は container の block:<名前> でブロックに足してください",
                    ));
                }
                let (tag, prompt, default) = attributes::read_definition(&reader, index)?;
                let position = reader.required("position", reader.point("position")?)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let style = self.text_style(index, reader.name("textStyle")?)?.unwrap_or_else(|| self.default_style());
                let justification = zumen_dxf::placement::read(&reader, index)?;
                let mut text = build::text(position, self.raw_text(&default), height, rotation, style);
                if let Body::Text(text) = &mut text
                    && justification.aligned()
                {
                    text.halign = Some(justification.horizontal as i16);
                    text.valign = Some(justification.vertical as i16);
                    text.align = Some([position[0], position[1]]);
                }
                let Body::Text(text) = text else { unreachable!() };
                let body = Attribute {
                    text,
                    version: 0,
                    attribute_type: 1,
                    embedded: None,
                    tag: self.raw_text(&tag),
                    field_length: 0,
                    flags: 0,
                    lock: false,
                    prompt: Some((0, self.raw_text(&prompt))),
                };
                self.mark_attributes(place.block);
                (kind::ATTDEF, Body::Attdef(body))
            }
            _ => {
                reader.allow(&keys(&["block", "position", "rotation", "scale", "attributes"]), "ブロック参照")?;
                let name = reader.required("block", reader.name("block")?)?;
                let block = self
                    .blocks
                    .get(&upper(name))
                    .copied()
                    .filter(|block| *block != self.model && *block != self.paper && !upper(name).starts_with('*'))
                    .ok_or_else(|| {
                        EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))
                    })?;
                let position = reader.required("position", reader.point("position")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let scale = scale(index, &reader)?.unwrap_or([1.0; 3]);
                if let Some(values) = reader.strings("attributes")? {
                    let (definitions, bodies, base) = self.attribute_definitions(block);
                    let placed = attributes::place(&definitions, values, base, position, scale, rotation, index)?;
                    for attribute in placed {
                        let (common, body) = &bodies[attribute.source];
                        pending.push((attribute, common.clone(), body.clone()));
                    }
                }
                (kind::INSERT, build::insert(position, scale, rotation, block))
            }
        };
        let mut common = self.new_common(index, place, &reader)?;
        let handle = self.next_handle(index)?;
        self.append(index, place, handle, &mut common)?;
        let mut body = body;
        if let Body::Insert(insert) = &mut body {
            self.add_insert(insert.block.value, handle);
            if !pending.is_empty() {
                self.add_attributes(index, handle, &common, insert, pending)?;
            }
        }
        let is_viewport = matches!(body, Body::Viewport(_));
        let entity = Decoded { kind: type_number, eed: Vec::new(), value: Entity { common, body } };
        self.store_entity(handle, &entity);
        if is_viewport {
            self.add_layout_viewport(place.block, handle);
        }
        if let Some((input, definition, reactor)) = image_objects {
            self.add_image_objects(index, handle, &input, definition, reactor)?;
        }
        self.created.push(format!("{handle:X}"));
        Ok(())
    }

    /// 画像の定義（IMAGEDEF）とリアクタ（IMAGEDEF_REACTOR）を書き、定義を ACAD_IMAGE_DICT に載せる。ACAD_IMAGE_DICT が
    /// なければ作り、名前つきオブジェクトの辞書に載せる。クラスがなければ足す
    fn add_image_objects(
        &mut self,
        index: usize,
        image: u64,
        input: &zumen_dxf::image::ImageInput,
        definition: u64,
        reactor: u64,
    ) -> Result<(), EditError> {
        let unreadable = || EditError::at(index, "unreadable", "DWG の名前つきオブジェクトの辞書が読めません");
        let definition_kind = self.class_number(index, "IMAGEDEF")?;
        let reactor_kind = self.class_number(index, "IMAGEDEF_REACTOR")?;
        let named = self.tables.named_objects;
        let mut root = self.read(named, |s, _| Dictionary::read(s, false)).ok_or_else(unreadable)?;
        let r2004 = self.version >= Version::R2004;
        let common = |owner: u64, reactors: Vec<HandleRef>| ObjectCommon {
            owner: href(4, owner),
            reactors,
            xdictionary: if r2004 { None } else { Some(href(3, 0)) },
            ds_binary: false,
        };
        let existing = root
            .value
            .entries
            .iter()
            .find(|(name, _)| upper(&self.text_of(name)) == zumen_dxf::image::DICTIONARY)
            .map(|(_, entry)| entry.resolve(named));
        let mut dictionary =
            match existing.and_then(|handle| Some((handle, self.read(handle, |s, _| Dictionary::read(s, false))?))) {
                Some(found) => found,
                None => {
                    let handle = self.next_handle(index)?;
                    root.value.entries.push((self.raw_text(zumen_dxf::image::DICTIONARY), href(2, handle)));
                    let (kind, eed) = (root.kind, root.eed.clone());
                    self.store(named, kind, &eed, |out| root.value.write(out));
                    let value = Dictionary {
                        common: common(named, vec![href(4, named)]),
                        cloning: 1,
                        hard_owner: 1,
                        r14_unknown: 0,
                        entries: Vec::new(),
                        default: None,
                    };
                    (handle, Decoded { kind: kind::DICTIONARY, eed: Vec::new(), value })
                }
            };
        let names: Vec<String> = dictionary.1.value.entries.iter().map(|(name, _)| self.text_of(name)).collect();
        let name =
            zumen_dxf::image::entry_name(&input.path, |name| names.iter().any(|used| used.eq_ignore_ascii_case(name)));
        dictionary.1.value.entries.push((self.raw_text(&name), href(2, definition)));
        let (handle, dictionary) = dictionary;
        self.store(handle, dictionary.kind, &dictionary.eed, |out| dictionary.value.write(out));
        // IMAGEDEF（20.4.82）。画素の大きさは単位なしの 1（DXF の編集と同じ）
        let value = ImageDef {
            common: common(handle, vec![href(4, handle), href(4, reactor)]),
            class_version: 0,
            size: input.pixels,
            path: self.raw_text(&input.path),
            loaded: true,
            resolution_units: 0,
            pixel_size: [1.0, 1.0],
        };
        self.store(definition, definition_kind, &[], |out| value.write(out));
        // IMAGEDEF_REACTOR（20.4.83）
        let value = ImageDefReactor { common: common(image, Vec::new()), class_version: 2 };
        self.store(reactor, reactor_kind, &[], |out| value.write(out));
        Ok(())
    }

    /// ビューポートを足せる先か。R2000 以降の、モデルでないレイアウトの紙空間。R2000 の作業中の紙空間（*Paper_Space）は
    /// VIEWPORT ENTITY HEADER を要るので受け付けない
    fn viewport_place(&self, index: usize, place: Place) -> Result<(), EditError> {
        if self.version < Version::R2000 {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "AutoCAD 2000 より前の DWG にはビューポートを足せません",
            ));
        }
        if place.block == self.model || !self.layouts.values().any(|block| *block == place.block) {
            return Err(EditError::at(
                index,
                "invalid-value",
                "ビューポートは container の layout:<名前> でレイアウトに足してください",
            ));
        }
        if self.version < Version::R2004 && place.block == self.paper {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "AutoCAD 2000 の DWG の作業中のレイアウトにはビューポートを足せません（add-layout で足したレイアウトには足せます）",
            ));
        }
        Ok(())
    }

    /// レイアウト（LAYOUT）のビューポートの並び（R2004 以降）に足す。最初のビューポートは最後に使ったビューポートにもする
    fn add_layout_viewport(&mut self, block: u64, viewport: u64) {
        let Some(header) = self.read(block, |s, _| BlockHeader::read(s)) else { return };
        let layout = header.value.layout.resolve(block);
        let Some(mut decoded) = self.read(layout, |s, _| Layout::read(s)) else { return };
        if decoded.value.last_viewport.value == 0 {
            decoded.value.last_viewport = href(4, viewport);
        }
        if self.version >= Version::R2004 {
            decoded.value.viewports.push(href(4, viewport));
        }
        let (kind, eed) = (decoded.kind, decoded.eed.clone());
        self.store(layout, kind, &eed, |out| decoded.value.write(out));
    }

    /// レイアウトを足す（docs/decisions/0017。LAYOUT は 20.4.84）。図面のレイアウトを写して名前・用紙・紙空間のブロックを
    /// 変え、ACAD_LAYOUT の辞書に足す。紙空間のブロック（BLOCK_HEADER の名前は *Paper_Space、BLOCK は
    /// *Paper_Space<n>）と、用紙の全体を表すビューポートも作る。AutoCAD 2000 から
    fn add_layout(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        if self.version < Version::R2000 {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "AutoCAD 2000 より前の DWG にはレイアウト（LAYOUT）を足せません",
            ));
        }
        if !valid_name(name) || name.eq_ignore_ascii_case("model") {
            return Err(EditError::at(index, "invalid-value", format!("レイアウトの名前 {name} は使えません")));
        }
        if self.layouts.contains_key(&upper(name)) {
            return Err(EditError::at(index, "already-exists", format!("レイアウト {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&["paperSize", "margins"], "レイアウト")?;
        let paper = reader.required("paperSize", reader.numbers("paperSize", 2)?)?;
        if paper[0] <= 0.0 || paper[1] <= 0.0 {
            return Err(EditError::at(
                index,
                "invalid-value",
                "paperSize は 0 より大きい [幅, 高さ]（mm）にしてください",
            ));
        }
        // 用紙の余白（左・下・右・上、mm）。省略すると写したレイアウトのまま
        let margins = reader.numbers("margins", 4)?;
        if margins.as_ref().is_some_and(|margins| margins.iter().any(|margin| *margin < 0.0)) {
            return Err(EditError::at(
                index,
                "invalid-value",
                "margins は 0 以上の [左, 下, 右, 上]（mm）にしてください",
            ));
        }
        let unreadable =
            || EditError::at(index, "unreadable", "DWG のレイアウト（LAYOUT）と ACAD_LAYOUT の辞書が読めません");
        let template = self
            .layout_objects
            .iter()
            .max_by_key(|(name, _)| name.as_str() != "MODEL")
            .map(|(_, handle)| *handle)
            .ok_or_else(unreadable)?;
        let template = self.read(template, |s, _| Layout::read(s)).ok_or_else(unreadable)?;
        let mut dictionary =
            self.read(self.layout_dictionary, |s, _| Dictionary::read(s, false)).ok_or_else(unreadable)?;
        let tab = self
            .layout_objects
            .values()
            .filter_map(|handle| self.read(*handle, |s, _| Layout::read(s)).map(|layout| layout.value.tab_order))
            .max()
            .unwrap_or(0)
            + 1;
        let suffix = self
            .blocks
            .keys()
            .filter_map(|block| block.strip_prefix("*PAPER_SPACE").and_then(|number| number.parse::<u64>().ok()))
            .max()
            .map_or(0, |number| number + 1);
        let block = self.create_block(index, &format!("*Paper_Space{suffix}"), [0.0; 3], false)?;
        let layout_handle = self.next_handle(index)?;
        // BLOCK_HEADER: 名前は *Paper_Space（どの紙空間も同じ。AutoCAD と同じ）、レイアウトはこのレイアウト
        let mut header = self.read(block, |s, _| BlockHeader::read(s)).ok_or_else(unreadable)?;
        header.value.name = EntryName::new(self.raw_text("*Paper_Space"));
        header.value.layout = href(5, layout_handle);
        let (kind, eed) = (header.kind, header.eed.clone());
        self.store(block, kind, &eed, |out| header.value.write(out));
        // 用紙の全体を表すビューポート（紙空間の最初のビューポート）
        let place = Place { block, mode: 0 };
        let none = EditValues::new();
        let mut common = self.new_common(index, place, &ValueReader::new(&none, index))?;
        let overall = self.next_handle(index)?;
        self.append(index, place, overall, &mut common)?;
        let middle = [paper[0] / 2.0, paper[1] / 2.0, 0.0];
        let sheet = RawText::empty(self.version);
        let body = build::viewport(self.version, middle, paper[0], paper[1], middle, paper[1], sheet);
        let viewport = Decoded { kind: kind::VIEWPORT, eed: Vec::new(), value: Entity { common, body } };
        self.store_entity(overall, &viewport);
        let r2004 = self.version >= Version::R2004;
        let mut layout = template.value;
        layout.common = ObjectCommon {
            owner: href(4, self.layout_dictionary),
            reactors: vec![href(4, self.layout_dictionary)],
            xdictionary: if r2004 { None } else { Some(href(3, 0)) },
            ds_binary: false,
        };
        layout.name = self.raw_text(name);
        layout.tab_order = tab;
        // PLOTSETTINGS（20.4.84）: 用紙の座標は mm・等倍・回転なしで、左下を原点にする。
        // 雛形の印刷設定を継ぐと、指定した大きさやビューポートの縮尺が変わる。
        layout.plot_flags = 0;
        layout.margins = [0.0; 4];
        layout.plot_origin = [0.0; 2];
        layout.paper_units = 1;
        layout.plot_rotation = 0;
        layout.plot_type = 5;
        layout.window_min = [0.0; 2];
        layout.window_max = [0.0; 2];
        layout.real_units = 1.0;
        layout.drawing_units = 1.0;
        layout.scale_type = 16;
        layout.scale_factor = 1.0;
        layout.paper_image_origin = [0.0; 2];
        layout.paper = [paper[0], paper[1]];
        if let Some(margins) = &margins {
            layout.margins = [margins[0], margins[1], margins[2], margins[3]];
        }
        layout.paper_size = self.raw_text(&zumen_dxf::paper_name(paper[0], paper[1]));
        layout.limmin = [0.0, 0.0];
        layout.limmax = [paper[0], paper[1]];
        layout.extmin = [1e20; 3];
        layout.extmax = [-1e20; 3];
        layout.block_record = href(4, block);
        layout.last_viewport = href(4, overall);
        layout.viewports = if r2004 { vec![href(4, overall)] } else { Vec::new() };
        self.store(layout_handle, template.kind, &[], |out| layout.write(out));
        dictionary.value.entries.push((self.raw_text(name), href(2, layout_handle)));
        let (kind, eed) = (dictionary.kind, dictionary.eed.clone());
        let handle = self.layout_dictionary;
        self.store(handle, kind, &eed, |out| dictionary.value.write(out));
        self.layouts.insert(upper(name), block);
        self.layout_objects.insert(upper(name), layout_handle);
        self.created.push(name.to_string());
        Ok(())
    }

    /// ODA「LAYOUT / PLOTSETTINGS」: 用紙幅・高さは mm。所有関係・余白・印刷設定を保って用紙と全体表示を変える。
    fn update_layout(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        let reader = ValueReader::new(values, index);
        reader.allow(&["paperSize"], "レイアウトの変更")?;
        let paper = reader.required("paperSize", reader.numbers("paperSize", 2)?)?;
        if paper.iter().any(|number| *number <= 0.0) {
            return Err(EditError::at(
                index,
                "invalid-value",
                "paperSize は 0 より大きい [幅, 高さ]（mm）にしてください",
            ));
        }
        let handle = self
            .layout_objects
            .get(&upper(name))
            .copied()
            .filter(|_| !name.eq_ignore_ascii_case("model"))
            .ok_or_else(|| EditError::at(index, "not-found", format!("レイアウト {name} は図面にありません")))?;
        let mut decoded = self
            .read(handle, |s, _| Layout::read(s))
            .ok_or_else(|| EditError::at(index, "unreadable", "DWG のレイアウト（LAYOUT）が読めません"))?;
        decoded.value.paper = [paper[0], paper[1]];
        decoded.value.paper_size = self.raw_text(&zumen_dxf::paper_name(paper[0], paper[1]));
        let scale = if decoded.value.real_units > 0.0 && decoded.value.drawing_units > 0.0 {
            decoded.value.real_units / decoded.value.drawing_units
        } else {
            1.0
        };
        let unit = (if decoded.value.paper_units == 0 { 25.4 } else { 1.0 }) * scale;
        let mut size = [paper[0] / unit, paper[1] / unit];
        if matches!(decoded.value.plot_rotation, 1 | 3) {
            size.swap(0, 1);
        }
        let min = decoded.value.limmin;
        decoded.value.limmax = [min[0] + size[0], min[1] + size[1]];
        let block = decoded.value.block_record.resolve(handle);
        let header = self
            .read(block, |s, _| BlockHeader::read(s))
            .ok_or_else(|| EditError::at(index, "unreadable", "DWG のレイアウトのブロックが読めません"))?;
        let members: Vec<_> = if self.version >= Version::R2004 {
            header.value.entities.iter().map(|entity| entity.resolve(block)).collect()
        } else {
            header
                .value
                .first_last
                .and_then(|(first, last)| self.chain(first.resolve(block), last.resolve(block)))
                .unwrap_or_default()
        };
        // DWG はビューポート番号を持たない。DXF の番号 1 は紙空間の最初の VIEWPORT（document.rs と同じ）。
        for member in members {
            let Some((mut entity, _)) = self.read_entity(member) else { continue };
            let Body::Viewport(viewport) = &mut entity.value.body else { continue };
            viewport.center = [min[0] + size[0] / 2.0, min[1] + size[1] / 2.0, 0.0];
            viewport.width = size[0];
            viewport.height = size[1];
            viewport.view_center = [min[0] + size[0] / 2.0, min[1] + size[1] / 2.0];
            viewport.view_height = size[1];
            self.store_entity(member, &entity);
            break;
        }
        self.store(handle, decoded.kind, &decoded.eed, |out| decoded.value.write(out));
        Ok(())
    }

    /// ブロック参照の属性（ATTRIB）と SEQEND を書き、参照の属性の並び（R2004 以降）か最初と最後の属性（R2000 まで）に
    /// する。ATTRIB は属性の定義の値を写し、位置・高さ・回転・値を参照に合わせる（モデルの書き出しと同じ形）
    fn add_attributes(
        &mut self,
        index: usize,
        insert_handle: u64,
        insert_common: &EntityCommon,
        insert: &mut crate::codec::entities::Insert,
        pending: Vec<(Placed, EntityCommon, Attribute)>,
    ) -> Result<(), EditError> {
        let mut handles = Vec::with_capacity(pending.len());
        for _ in &pending {
            handles.push(self.next_handle(index)?);
        }
        let seqend = self.next_handle(index)?;
        let follower = |source: &EntityCommon, links: Option<(HandleRef, HandleRef)>| {
            // ATTRIB の見た目は ATTDEF から写す（DXF Reference「ATTRIB」「Common Group Codes」）。
            let mut common = source.clone();
            common.graphic = None;
            common.xdictionary = None;
            common.mode = 0;
            common.owner = Some(href(4, insert_handle));
            common.reactors = Vec::new();
            common.links = links;
            common
        };
        let r2004 = self.version >= Version::R2004;
        for (at, (placed, appearance, mut body)) in pending.into_iter().enumerate() {
            // R2000 までは属性どうしをつなぐ（前の属性と次の属性、端は 0）
            let links = (!r2004).then(|| {
                let previous = at.checked_sub(1).map_or(0, |before| handles[before]);
                let next = handles.get(at + 1).copied().unwrap_or(0);
                (href(4, previous), href(4, next))
            });
            body.text.insert = [placed.position[0], placed.position[1]];
            body.text.elevation = (placed.position[2] != 0.0).then_some(placed.position[2]);
            body.text.align = placed.align.map(|align| [align[0], align[1]]);
            body.text.height = placed.height;
            body.text.width = (placed.width != 1.0).then_some(placed.width);
            body.text.oblique = (placed.oblique != 0.0).then(|| placed.oblique.to_radians());
            body.text.generation = (placed.generation != 0).then_some(placed.generation as i16);
            body.text.rotation = (placed.rotation != 0.0).then(|| placed.rotation.to_radians());
            body.text.value = self.raw_text(&placed.value);
            body.prompt = None;
            body.version = 0;
            let entity = Decoded {
                kind: kind::ATTRIB,
                eed: Vec::new(),
                value: Entity { common: follower(&appearance, links), body: Body::Attrib(body) },
            };
            self.store_entity(handles[at], &entity);
        }
        let end = Decoded {
            kind: kind::SEQEND,
            eed: Vec::new(),
            value: Entity { common: follower(insert_common, None), body: Body::Empty },
        };
        self.store_entity(seqend, &end);
        insert.has_attributes = true;
        insert.seqend = Some(href(3, seqend));
        if r2004 {
            insert.attributes = handles.iter().map(|handle| href(3, *handle)).collect();
        } else {
            insert.first_last = Some((href(4, handles[0]), href(4, handles[handles.len() - 1])));
        }
        Ok(())
    }

    /// ブロックの属性の定義（ATTDEF）の値と、その図形の値、ブロックの基点
    fn attribute_definitions(&self, block: u64) -> (Vec<Definition>, Vec<(EntityCommon, Attribute)>, [f64; 3]) {
        let Some(header) = self.read(block, |s, _| BlockHeader::read(s)) else {
            return (Vec::new(), Vec::new(), [0.0; 3]);
        };
        let members: Vec<u64> = if self.version >= Version::R2004 {
            header.value.entities.iter().map(|entity| entity.resolve(block)).collect()
        } else {
            header
                .value
                .first_last
                .and_then(|(first, last)| self.chain(first.resolve(block), last.resolve(block)))
                .unwrap_or_default()
        };
        let mut definitions = Vec::new();
        let mut bodies = Vec::new();
        for member in members {
            let Some((entity, _)) = self.read_entity(member) else { continue };
            let Body::Attdef(mut body) = entity.value.body else { continue };
            // 相対参照は元の ATTDEF を基準に解決してから写す（DWG 仕様 2.13 Handle references）。
            let absolute = |handle: HandleRef| {
                if handle.code >= 6 { href(5, handle.resolve(member)) } else { handle }
            };
            body.text.style = absolute(body.text.style);
            let mut common = entity.value.common;
            common.layer = absolute(common.layer);
            common.ltype = common.ltype.map(absolute);
            common.book_color = common.book_color.map(absolute);
            common.plotstyle = common.plotstyle.map(absolute);
            common.material = common.material.map(absolute);
            common.visual_styles = common.visual_styles.map(|style| style.map(absolute));
            let text: &Text = &body.text;
            let z = text.elevation.unwrap_or(0.0);
            let (horizontal, vertical) = (i64::from(text.halign.unwrap_or(0)), i64::from(text.valign.unwrap_or(0)));
            definitions.push(Definition {
                tag: self.text_of(&body.tag),
                prompt: body.prompt.as_ref().map(|(_, prompt)| self.text_of(prompt)).unwrap_or_default(),
                default: self.text_of(&text.value),
                position: [text.insert[0], text.insert[1], z],
                align: text.align.filter(|_| horizontal != 0 || vertical != 0).map(|align| [align[0], align[1], z]),
                height: text.height,
                rotation: text.rotation.unwrap_or(0.0).to_degrees(),
                width: text.width.unwrap_or(1.0),
                oblique: text.oblique.unwrap_or(0.0).to_degrees(),
                generation: i64::from(text.generation.unwrap_or(0)),
                horizontal,
                vertical,
                flags: i64::from(body.flags),
                style: None,
            });
            bodies.push((common, body));
        }
        (definitions, bodies, header.value.base)
    }

    /// ブロック（BLOCK_HEADER）に「属性を持つ」を立てる
    fn mark_attributes(&mut self, block: u64) {
        if let Some(mut header) = self.read(block, |s, _| BlockHeader::read(s))
            && !header.value.has_attributes
        {
            header.value.has_attributes = true;
            let (kind, eed) = (header.kind, header.eed.clone());
            self.store(block, kind, &eed, |out| header.value.write(out));
        }
    }

    /// ブロックの配置の並び（BLOCK_HEADER の inserts）に INSERT を足す
    fn add_insert(&mut self, block: u64, insert: u64) {
        if let Some(mut header) = self.read(block, |s, _| BlockHeader::read(s)) {
            header.value.insert_count.push(1);
            header.value.inserts.push(href(4, insert));
            let (kind, eed) = (header.kind, header.eed.clone());
            self.store(block, kind, &eed, |out| header.value.write(out));
        }
    }

    /// ブロックの配置の並びから INSERT を除く
    fn remove_insert(&mut self, block: u64, insert: u64) {
        if let Some(mut header) = self.read(block, |s, _| BlockHeader::read(s)) {
            let Some(position) = header.value.inserts.iter().position(|other| other.resolve(block) == insert) else {
                return;
            };
            header.value.inserts.remove(position);
            if position < header.value.insert_count.len() {
                header.value.insert_count.remove(position);
            }
            let (kind, eed) = (header.kind, header.eed.clone());
            self.store(block, kind, &eed, |out| header.value.write(out));
        }
    }

    /// id を図形にする。ビューポートを通した id（VP:…）はその図形、ブロック参照の中（…>…）は、参照の属性のほかは
    /// ブロックの定義を変えることになるのでエラーにする（DXF の編集と同じ）
    fn target(&self, index: usize, id: &str) -> Result<Target, EditError> {
        let id = id.trim();
        let id = id.split_once(':').map_or(id, |(_, rest)| rest);
        let segments: Vec<&str> = id.split('>').collect();
        let clean = |segment: &str| upper(segment.split('[').next().unwrap_or(segment));
        let last = clean(segments[segments.len() - 1]);
        let not_found = || EditError::at(index, "not-found", format!("図形 {id} は図面にありません"));
        let handle = u64::from_str_radix(&last, 16).map_err(|_| not_found())?;
        let raw = self.bytes(handle).and_then(|bytes| open_raw(bytes, 0, self.version).ok()).ok_or_else(not_found)?;
        let (name, entity) = self.kind_of(raw.kind).ok_or_else(not_found)?;
        if !entity {
            return Err(not_found());
        }
        let (decoded, name) = self.read_entity(handle).ok_or_else(|| {
            EditError::at(index, "unsupported-kind", format!("DWG の編集では {name} の図形を読み書きできません"))
        })?;
        if segments.len() > 1 {
            let parent = clean(segments[segments.len() - 2]);
            let owner = decoded.value.common.owner.map(|owner| owner.resolve(handle));
            let is_parent = u64::from_str_radix(&parent, 16).ok().is_some_and(|parent| owner == Some(parent));
            if decoded.kind != kind::ATTRIB || !is_parent {
                return Err(EditError::at(
                    index,
                    "inside-block",
                    format!(
                        "図形 {id} はブロック参照 {parent} の中にあります。参照は id {parent} で、ブロックの定義の中の図形は id {last} で変えられます（定義を変えると、そのブロックのすべての参照が変わります）"
                    ),
                ));
            }
        }
        Ok(Target { handle, entity: decoded, name })
    }

    fn update(&mut self, index: usize, id: &str, set: &EditValues) -> Result<(), EditError> {
        let Target { handle, entity: mut decoded, name } = self.target(index, id)?;
        let insertion_before = match &decoded.value.body {
            Body::Insert(insert) => Some((
                zumen_dxf::attributes::Insertion {
                    position: insert.point,
                    scale: insert.scale,
                    rotation: insert.rotation.to_degrees(),
                },
                insert.extrusion,
                self.followers(handle, &decoded.value),
            )),
            _ => None,
        };
        let reader = ValueReader::new(set, index);
        // R14 の LWPOLYLINE はクラスの型
        let fixed = if name == "LWPOLYLINE" { kind::LWPOLYLINE } else { decoded.kind };
        let own: &[&'static str] = match fixed {
            kind::LINE => &["start", "end"],
            kind::CIRCLE => &["center", "radius"],
            kind::ARC => &["center", "radius", "angle"],
            kind::ELLIPSE => &["center", "majorAxis", "ratio", "parameter"],
            kind::POINT => &["position"],
            kind::LWPOLYLINE => &["vertices", "closed", "bulges", "width"],
            kind::TEXT | kind::ATTRIB | kind::ATTDEF => &["position", "text", "height", "rotation", "textStyle"],
            kind::MTEXT => &["position", "text", "height", "rotation", "textStyle", "wrapWidth"],
            kind::INSERT | kind::MINSERT => &["block", "position", "rotation", "scale"],
            _ => &[],
        };
        let keys: Vec<&str> = COMMON.iter().chain(own).copied().collect();
        reader.allow(&keys, &format!("{name} の変更"))?;
        let transparency = decoded.value.common.color.transparency;
        let common = self.common(index, &reader, transparency)?;
        Self::apply_common(&common, &mut decoded.value.common);
        let style = self.text_style(index, reader.name("textStyle")?)?;
        let invalid = |message: &str| EditError::at(index, "invalid-value", message.to_string());
        match &mut decoded.value.body {
            Body::Line(line) => {
                if let Some(start) = reader.point("start")? {
                    line.start = start;
                }
                if let Some(end) = reader.point("end")? {
                    line.end = end;
                }
            }
            Body::Circle(circle) => {
                if let Some(center) = reader.point("center")? {
                    circle.center = center;
                }
                if let Some(radius) = reader.positive("radius")? {
                    circle.radius = radius;
                }
            }
            Body::Arc(arc) => {
                if let Some(center) = reader.point("center")? {
                    arc.circle.center = center;
                }
                if let Some(radius) = reader.positive("radius")? {
                    arc.circle.radius = radius;
                }
                if let Some(angle) = reader.numbers("angle", 2)? {
                    (arc.start_angle, arc.end_angle) = crate::write::drawing::rad_pair(angle[0], angle[1]);
                }
            }
            Body::Ellipse(ellipse) => {
                if let Some(center) = reader.point("center")? {
                    ellipse.center = center;
                }
                if let Some(axis) = reader.point("majorAxis")? {
                    if axis == [0.0; 3] {
                        return Err(invalid("majorAxis は 0 でない長さにしてください"));
                    }
                    ellipse.major_axis = axis;
                }
                if let Some(ratio) = reader.positive("ratio")? {
                    if ratio > 1.0 {
                        return Err(invalid("ratio は 0 より大きく 1 以下にしてください"));
                    }
                    ellipse.ratio = ratio;
                }
                if let Some(parameter) = reader.numbers("parameter", 2)? {
                    ellipse.start = parameter[0];
                    ellipse.end = parameter[1];
                }
            }
            Body::Point(point) => {
                if let Some(position) = reader.point("position")? {
                    point.location = position;
                }
            }
            Body::LwPolyline(polyline) => build::update_lwpolyline(index, polyline, &reader)?,
            Body::Text(text) => {
                if reader.text("text")?.is_some_and(|text| text.contains('\n')) {
                    return Err(invalid("1 行の文字（TEXT・ATTRIB）に改行は書けません"));
                }
                self.update_text(text, &reader, style)?;
            }
            Body::Attrib(attribute) | Body::Attdef(attribute) => {
                let text = reader.text("text")?;
                if text.is_some_and(|text| text.contains('\n')) {
                    return Err(invalid("1 行の文字（TEXT・ATTRIB）に改行は書けません"));
                }
                if attribute.embedded.is_some() && text.is_some() {
                    return Err(EditError::at(index, "unsupported-kind", "複数行の属性の文字は変えられません"));
                }
                self.update_text(&mut attribute.text, &reader, style)?;
            }
            Body::MText(mtext) => {
                let content = reader.text("text")?.map(|text| self.raw_text(&mtext_content(text)));
                build::update_mtext(
                    mtext,
                    reader.point("position")?,
                    content,
                    reader.positive("height")?,
                    reader.number("rotation")?,
                    reader.number("wrapWidth")?,
                    style,
                );
            }
            Body::Insert(insert) => {
                let block = match reader.name("block")? {
                    Some(name) => Some(
                        self.blocks
                            .get(&upper(name))
                            .copied()
                            .filter(|block| *block != self.model && *block != self.paper)
                            .ok_or_else(|| {
                                EditError::at(
                                    index,
                                    "missing-definition",
                                    format!("ブロック {name} は図面にありません"),
                                )
                            })?,
                    ),
                    None => None,
                };
                if let Some(position) = reader.point("position")? {
                    insert.point = position;
                }
                if let Some(rotation) = reader.number("rotation")? {
                    insert.rotation = crate::write::drawing::rad(rotation);
                }
                if let Some(scale) = scale(index, &reader)? {
                    insert.scale = scale;
                    insert.scale_flags = crate::codec::entities::Insert::scale_flags_of(scale);
                }
                if let Some(block) = block {
                    let old = insert.block.resolve(handle);
                    insert.block = href(5, block);
                    if old != block {
                        self.remove_insert(old, handle);
                        self.add_insert(block, handle);
                    }
                }
            }
            _ => {}
        }
        if let Some((before, extrusion, followers)) = insertion_before
            && let Body::Insert(insert) = &decoded.value.body
        {
            let after = zumen_dxf::attributes::Insertion {
                position: insert.point,
                scale: insert.scale,
                rotation: insert.rotation.to_degrees(),
            };
            self.remap_attributes(index, &followers, extrusion, before, after)?;
        }
        // POLYLINE のレイヤは頂点と SEQEND にも当てる（DXF の編集と同じ）
        let polyline =
            matches!(decoded.kind, kind::POLYLINE_2D | kind::POLYLINE_3D | kind::POLYLINE_PFACE | kind::POLYLINE_MESH);
        if let Some(layer) = common.layer.filter(|_| polyline) {
            for follower in self.followers(handle, &decoded.value) {
                if let Some((mut vertex, _)) = self.read_entity(follower) {
                    vertex.value.common.layer = href(5, layer);
                    self.store_entity(follower, &vertex);
                }
            }
        }
        self.store_entity(handle, &decoded);
        Ok(())
    }

    /// ODA「ATTRIB / TEXT」: 属性は INSERT のローカル座標ではなく、属性自身の OCS に整列点を持つ。
    fn remap_attributes(
        &mut self,
        index: usize,
        followers: &[u64],
        extrusion: [f64; 3],
        before: zumen_dxf::attributes::Insertion,
        after: zumen_dxf::attributes::Insertion,
    ) -> Result<(), EditError> {
        if before == after {
            return Ok(());
        }
        for &handle in followers {
            let Some((mut decoded, _)) = self.read_entity(handle) else {
                return Err(EditError::at(index, "unsupported-kind", "ブロック参照の属性が読めません"));
            };
            let Body::Attrib(attribute) = &mut decoded.value.body else { continue };
            let text = &mut attribute.text;
            if extrusion != [0.0, 0.0, 1.0] || text.extrusion != [0.0, 0.0, 1.0] || attribute.embedded.is_some() {
                return Err(EditError::at(
                    index,
                    "unsupported-kind",
                    "傾いた平面・複数行の属性を持つブロック参照の配置は変えられません",
                ));
            }
            let z = text.elevation.unwrap_or(0.0);
            let definition = Definition {
                tag: String::new(),
                prompt: String::new(),
                default: String::new(),
                position: [text.insert[0], text.insert[1], z],
                align: text.align.map(|point| [point[0], point[1], z]),
                height: text.height,
                rotation: text.rotation.unwrap_or(0.0).to_degrees(),
                width: text.width.unwrap_or(1.0),
                oblique: text.oblique.unwrap_or(0.0).to_degrees(),
                generation: i64::from(text.generation.unwrap_or(0)),
                horizontal: 0,
                vertical: 0,
                flags: 0,
                style: None,
            };
            let moved = zumen_dxf::attributes::remap(&definition, before, after, index)?;
            text.insert = [moved.position[0], moved.position[1]];
            text.elevation = (moved.position[2] != 0.0).then_some(moved.position[2]);
            text.align = moved.align.map(|point| [point[0], point[1]]);
            if before.scale != after.scale || before.rotation != after.rotation {
                text.height = moved.height;
                text.width = Some(moved.width);
                text.rotation = Some(moved.rotation.to_radians());
                text.oblique = Some(moved.oblique.to_radians());
                text.generation = Some(moved.generation as i16);
            }
            self.store_entity(handle, &decoded);
        }
        Ok(())
    }

    /// 1 行の文字の値を変える。位置合わせが左・ベースライン以外なら、配置に使われる位置合わせの点も同じだけ動かす
    fn update_text(
        &self,
        text: &mut crate::codec::entities::Text,
        reader: &ValueReader<'_>,
        style: Option<u64>,
    ) -> Result<(), EditError> {
        if let Some(position) = reader.point("position")? {
            build::move_text(text, position);
        }
        if let Some(value) = reader.text("text")? {
            text.value = self.raw_text(value);
        }
        if let Some(height) = reader.positive("height")? {
            text.height = height;
        }
        if let Some(rotation) = reader.number("rotation")? {
            text.rotation = (rotation != 0.0).then(|| crate::write::drawing::rad(rotation));
        }
        if let Some(style) = style {
            text.style = href(5, style);
        }
        Ok(())
    }

    /// 図形に従う図形（POLYLINE の頂点、INSERT の属性と、SEQEND）
    fn followers(&self, handle: u64, entity: &Entity) -> Vec<u64> {
        let (list, first_last, seqend): (Vec<u64>, Option<(u64, u64)>, Option<u64>) = match &entity.body {
            Body::Polyline2d(polyline) => owned(&polyline.owned, handle),
            Body::Polyline3d(polyline) => owned(&polyline.owned, handle),
            Body::PolylinePface(polyline) => owned(&polyline.owned, handle),
            Body::PolylineMesh(polyline) => owned(&polyline.owned, handle),
            Body::Insert(insert) if insert.has_attributes => (
                insert.attributes.iter().map(|attribute| attribute.resolve(handle)).collect(),
                insert.first_last.map(|(first, last)| (first.resolve(handle), last.resolve(handle))),
                insert.seqend.map(|seqend| seqend.resolve(handle)),
            ),
            _ => (Vec::new(), None, None),
        };
        let mut out = list;
        // R2000 は最初から次の図形をたどる
        if let Some((first, last)) = first_last {
            let mut current = first;
            let mut seen = HashSet::new();
            while current != 0 && seen.insert(current) {
                out.push(current);
                if current == last {
                    break;
                }
                current = match self.read_entity(current) {
                    Some((follower, _)) => linked_next(&follower.value.common, current),
                    None => 0,
                };
            }
        }
        out.extend(seqend.filter(|seqend| *seqend != 0));
        out
    }

    fn delete(&mut self, index: usize, id: &str) -> Result<(), EditError> {
        let Target { handle, entity, .. } = self.target(index, id)?;
        if entity.kind == kind::ATTRIB {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "属性だけは消せません。ブロック参照ごと消してください",
            ));
        }
        let mut gone: Vec<u64> = vec![handle];
        gone.extend(self.followers(handle, &entity.value));
        // IMAGEDEF_REACTOR（20.4.83）は IMAGE が所有し、IMAGEDEF（20.4.82）がリアクタとして指す。
        // 定義はほかの画像も共有できるので残し、この画像のリアクタだけを両側から除く。
        if let Body::Image(image) = &entity.value.body {
            let reactor = image.reactor.resolve(handle);
            let owned = self.read(reactor, |s, kind| {
                if self.kind_of(kind).is_some_and(|(name, _)| name.eq_ignore_ascii_case("IMAGEDEF_REACTOR")) {
                    ImageDefReactor::read(s)
                } else {
                    Err(crate::bits::Eof)
                }
            });
            if owned.is_some_and(|decoded| decoded.value.common.owner.resolve(reactor) == handle) {
                gone.push(reactor);
                let definition = image.imagedef.resolve(handle);
                if let Some(mut decoded) = self.read(definition, |s, kind| {
                    if self.kind_of(kind).is_some_and(|(name, _)| name.eq_ignore_ascii_case("IMAGEDEF")) {
                        ImageDef::read(s)
                    } else {
                        Err(crate::bits::Eof)
                    }
                }) {
                    decoded.value.common.reactors.retain(|entry| entry.resolve(definition) != reactor);
                    self.store(definition, decoded.kind, &decoded.eed, |out| decoded.value.write(out));
                }
            }
        }
        // 図形とそれに従う図形が持つ拡張辞書と、その中のオブジェクト
        let mut queue: Vec<u64> = Vec::new();
        for member in gone.clone() {
            if let Some((follower, _)) = self.read_entity(member)
                && let Some(dictionary) = follower.value.common.xdictionary.map(|xdic| xdic.resolve(member))
                && dictionary != 0
            {
                queue.push(dictionary);
            }
        }
        while let Some(object) = queue.pop() {
            if gone.contains(&object) || self.bytes(object).is_none() {
                continue;
            }
            gone.push(object);
            if let Some(dictionary) = self.read(object, |s, kind| {
                if kind == kind::DICTIONARY { Dictionary::read(s, false).map(Some) } else { Ok(None) }
            }) && let Some(dictionary) = dictionary.value
            {
                queue.extend(dictionary.entries.iter().map(|(_, entry)| entry.resolve(object)));
                if let Some(xdic) = dictionary.common.xdictionary.map(|xdic| xdic.resolve(object))
                    && xdic != 0
                {
                    queue.push(xdic);
                }
            }
        }
        // 持ち主のブロックの図形の並び・つながりから除く。描く順もそのブロックのもの
        let gone_set: HashSet<u64> = gone.iter().copied().collect();
        if let Some(block) = self.containing_block(handle, &entity.value.common) {
            self.unlink(index, block, &gone_set)?;
            self.remove_from_sort_table(block, &gone_set);
        }
        if let Body::Insert(insert) = &entity.value.body {
            self.remove_insert(insert.block.resolve(handle), handle);
        }
        // リアクタのグループと、リアクタの並び
        for reactor in entity.value.common.reactors.iter().map(|reactor| reactor.resolve(handle)) {
            self.remove_reactor(reactor, &gone_set);
        }
        self.deleted.extend(gone);
        Ok(())
    }

    /// 図形の持ち主のブロック（モデル空間、紙空間、そのほかは持ち主のハンドル）
    fn owner_block(&self, handle: u64, common: &EntityCommon) -> u64 {
        match common.mode {
            2 => self.model,
            1 => self.paper,
            _ => common.owner.map_or(0, |owner| owner.resolve(handle)),
        }
    }

    /// 図形を並べている（つないでいる）ブロック。図形のモードのブロックを先に見て、なければほかのブロックを探す
    /// （読み取りと同じく、ブロックの並び・つながりを正とする）
    fn containing_block(&self, handle: u64, common: &EntityCommon) -> Option<u64> {
        let implied = self.owner_block(handle, common);
        if self.block_contains(implied, handle) {
            return Some(implied);
        }
        let mut blocks: Vec<u64> = self.blocks.values().copied().filter(|block| *block != implied).collect();
        blocks.sort_unstable();
        blocks.into_iter().find(|block| self.block_contains(*block, handle))
    }

    fn block_contains(&self, block: u64, handle: u64) -> bool {
        let Some(header) = self.read(block, |s, _| BlockHeader::read(s)) else { return false };
        if self.version >= Version::R2004 {
            return header.value.entities.iter().any(|entity| entity.resolve(block) == handle);
        }
        header
            .value
            .first_last
            .and_then(|(first, last)| self.chain(first.resolve(block), last.resolve(block)))
            .is_some_and(|chain| chain.contains(&handle))
    }

    /// 消す図形をブロックから除く。R2004 以降は図形の並びから、R2000 はつながりから除き、前後が変わった図形の
    /// つながりと、ブロックの最初・最後の図形を直す（つながりに頂点・属性が入っている図面もあるので、消すものを
    /// まとめて除く）
    fn unlink(&mut self, index: usize, block: u64, gone: &HashSet<u64>) -> Result<(), EditError> {
        let Some(mut header) = self.read(block, |s, _| BlockHeader::read(s)) else {
            return Ok(());
        };
        if self.version >= Version::R2004 {
            header.value.entities.retain(|entity| !gone.contains(&entity.resolve(block)));
        } else if let Some((first, last)) = header.value.first_last {
            let chain = self
                .chain(first.resolve(block), last.resolve(block))
                .ok_or_else(|| EditError::at(index, "unreadable", "図形のブロックの図形のつながりが読めません"))?;
            let kept: Vec<u64> = chain.iter().copied().filter(|entity| !gone.contains(entity)).collect();
            for (at, &entity) in kept.iter().enumerate() {
                let previous = if at == 0 { 0 } else { kept[at - 1] };
                let next = kept.get(at + 1).copied().unwrap_or(0);
                let place = chain.iter().position(|other| *other == entity).unwrap_or(0);
                let was_previous = if place == 0 { 0 } else { chain[place - 1] };
                let was_next = chain.get(place + 1).copied().unwrap_or(0);
                if (previous, next) != (was_previous, was_next) {
                    self.relink(index, entity, previous, next)?;
                }
            }
            let first = kept.first().copied().unwrap_or(0);
            let last = kept.last().copied().unwrap_or(0);
            header.value.first_last = Some((href(4, first), href(4, last)));
        }
        let (kind, eed) = (header.kind, header.eed.clone());
        self.store(block, kind, &eed, |out| header.value.write(out));
        Ok(())
    }

    /// ブロックの描く順（拡張辞書の ACAD_SORTENTS の SORTENTSTABLE）から図形を除く
    fn remove_from_sort_table(&mut self, block: u64, gone: &HashSet<u64>) {
        let Some(header) = self.read(block, |s, _| BlockHeader::read(s)) else { return };
        let Some(xdic) = header.value.common.xdictionary.map(|xdic| xdic.resolve(block)).filter(|xdic| *xdic != 0)
        else {
            return;
        };
        let Some(dictionary) = self.read(xdic, |s, _| Dictionary::read(s, false)) else { return };
        for (name, entry) in &dictionary.value.entries {
            if upper(&self.text_of(name)) != "ACAD_SORTENTS" {
                continue;
            }
            let table = entry.resolve(xdic);
            let Some(mut sort) = self.read(table, |s, _| SortEntsTable::read(s)) else { continue };
            let before = sort.value.entries.len();
            sort.value.entries.retain(|(_, entity)| !gone.contains(&entity.resolve(table)));
            if sort.value.entries.len() != before {
                let (kind, eed) = (sort.kind, sort.eed.clone());
                self.store(table, kind, &eed, |out| sort.value.write(out));
            }
        }
    }

    /// リアクタのオブジェクトから、消す図形を除く（グループの図形の並びと、リアクタの並び）
    fn remove_reactor(&mut self, reactor: u64, gone: &HashSet<u64>) {
        if let Some(mut group) =
            self.read(reactor, |s, kind| if kind == kind::GROUP { Group::read(s).map(Some) } else { Ok(None) })
            && let Some(value) = &mut group.value
        {
            value.entries.retain(|entity| !gone.contains(&entity.resolve(reactor)));
            value.common.reactors.retain(|entity| !gone.contains(&entity.resolve(reactor)));
            let (kind, eed) = (group.kind, group.eed.clone());
            self.store(reactor, kind, &eed, |out| value.write(out));
            return;
        }
        if let Some(mut dictionary) =
            self.read(
                reactor,
                |s, kind| {
                    if kind == kind::DICTIONARY { Dictionary::read(s, false).map(Some) } else { Ok(None) }
                },
            )
            && let Some(value) = &mut dictionary.value
        {
            let before = value.common.reactors.len();
            value.common.reactors.retain(|entity| !gone.contains(&entity.resolve(reactor)));
            if value.common.reactors.len() != before {
                let (kind, eed) = (dictionary.kind, dictionary.eed.clone());
                self.store(reactor, kind, &eed, |out| value.write(out));
            }
        }
    }

    // ---- レイヤ ----

    fn add_layer(&mut self, index: usize, name: &str, values: &EditValues, implicit: bool) -> Result<(), EditError> {
        if !valid_name(name) {
            return Err(EditError::at(index, "invalid-value", format!("レイヤの名前 {name} は使えません")));
        }
        if self.layers.contains_key(&upper(name)) {
            return Err(EditError::at(index, "already-exists", format!("レイヤ {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&LAYER_KEYS, "レイヤ")?;
        let control = self.layer_control;
        let mut table = self
            .read(control, Control::read)
            .ok_or_else(|| EditError::at(index, "unreadable", "DWG のレイヤの表（LAYER_CONTROL）が読めません"))?;
        // レイヤ 0 を写して、名前と値を変える（印刷スタイル・マテリアルの参照を保つ）。拡張辞書・リアクタ・
        // 拡張データは写さない
        let template = self.layers.get("0").and_then(|handle| self.read(*handle, |s, _| Layer::read(s)));
        let continuous = self.linetypes.get("CONTINUOUS").copied().unwrap_or(0);
        let r2004 = self.version >= Version::R2004;
        let mut layer = match template {
            Some(template) => template.value,
            None => Layer {
                common: ObjectCommon {
                    owner: href(4, control),
                    reactors: Vec::new(),
                    xdictionary: None,
                    ds_binary: false,
                },
                name: EntryName::new(RawText::empty(self.version)),
                values: 0,
                color: self.cmc(7, None),
                xref_block: href(5, 0),
                plotstyle: href(5, 0),
                material: (self.version >= Version::R2007).then(|| href(5, 0)),
                linetype: href(5, continuous),
                unknown: (self.version >= Version::R2013).then(|| href(5, 0)),
            },
        };
        layer.common = ObjectCommon {
            owner: href(4, control),
            reactors: Vec::new(),
            xdictionary: if r2004 { None } else { Some(href(3, 0)) },
            ds_binary: false,
        };
        layer.name = EntryName::new(self.raw_text(name));
        // 凍結・非表示・ロックなし、印刷する、線の太さは既定（31）
        layer.values = (31 << 5) | 16;
        layer.color = self.cmc(7, None);
        layer.linetype = href(5, continuous);
        layer.xref_block = href(5, 0);
        self.apply_layer_values(index, &mut layer, &reader)?;
        let handle = self.next_handle(index)?;
        self.store(handle, kind::LAYER, &[], |out| layer.write(out));
        table.value.entries.push(href(2, handle));
        let (kind, eed) = (table.kind, table.eed.clone());
        self.store(control, kind, &eed, |out| table.value.write(out, kind::LAYER_CONTROL));
        self.layers.insert(upper(name), handle);
        if !implicit {
            self.created.push(name.to_string());
        }
        Ok(())
    }

    /// 空のブロック定義を足す（docs/decisions/0017）。BLOCK_HEADER（20.4.52）と、その BLOCK・ENDBLK の図形を作り、
    /// BLOCK_CONTROL の項目に足す。中の図形は後の add が container の block:<名前> で足す
    fn add_block(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        if !valid_name(name) || name.trim().starts_with('*') {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("ブロックの名前 {name} は使えません（* で始まる名前は名前のないブロックのためのものです）"),
            ));
        }
        if self.blocks.contains_key(&upper(name)) {
            return Err(EditError::at(index, "already-exists", format!("ブロック {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&["basePoint"], "ブロックの定義")?;
        let base = reader.point("basePoint")?.unwrap_or([0.0; 3]);
        self.create_block(index, name, base, false)?;
        self.created.push(name.to_string());
        Ok(())
    }

    /// ブロックの定義（BLOCK_HEADER と BLOCK・ENDBLK）を作り、BLOCK_HEADER のハンドルを返す
    fn create_block(&mut self, index: usize, name: &str, base: [f64; 3], anonymous: bool) -> Result<u64, EditError> {
        let control = self.tables.block_control;
        let mut table = self
            .read(control, Control::read)
            .ok_or_else(|| EditError::at(index, "unreadable", "DWG のブロックの表（BLOCK_CONTROL）が読めません"))?;
        let header_handle = self.next_handle(index)?;
        let begin = self.next_handle(index)?;
        let end = self.next_handle(index)?;
        let r2004 = self.version >= Version::R2004;
        let header = BlockHeader {
            common: ObjectCommon {
                owner: href(4, control),
                reactors: Vec::new(),
                xdictionary: if r2004 { None } else { Some(href(3, 0)) },
                ds_binary: false,
            },
            // 名前のないブロックの BLOCK_HEADER の名前は頭（*D）だけ（読み取りが BLOCK の名前を使うのと同じ）
            name: EntryName::new(self.raw_text(if anonymous { &name[..2] } else { name })),
            anonymous,
            has_attributes: false,
            xref: false,
            overlaid: false,
            loaded: false,
            base,
            xref_path: RawText::empty(self.version),
            insert_count: Vec::new(),
            description: RawText::empty(self.version),
            preview: Vec::new(),
            insert_units: 0,
            explodable: true,
            scaling: 0,
            null: href(5, 0),
            block: href(3, begin),
            entities: Vec::new(),
            // R2000 までは最初と最後の図形（図形のないブロックは 0。モデルの書き出しと同じ）
            first_last: (!r2004).then(|| (href(4, 0), href(4, 0))),
            endblk: href(3, end),
            inserts: Vec::new(),
            layout: href(5, 0),
        };
        self.store(header_handle, kind::BLOCK_HEADER, &[], |out| header.write(out));
        let place = Place { block: header_handle, mode: 0 };
        let none = EditValues::new();
        let common = self.new_common(index, place, &ValueReader::new(&none, index))?;
        let block = Decoded {
            kind: kind::BLOCK,
            eed: Vec::new(),
            value: Entity { common: common.clone(), body: Body::Block(self.raw_text(name)) },
        };
        self.store_entity(begin, &block);
        let endblk = Decoded { kind: kind::ENDBLK, eed: Vec::new(), value: Entity { common, body: Body::Empty } };
        self.store_entity(end, &endblk);
        table.value.entries.push(href(2, header_handle));
        let (kind, eed) = (table.kind, table.eed.clone());
        self.store(control, kind, &eed, |out| table.value.write(out, kind::BLOCK_CONTROL));
        self.blocks.insert(upper(name), header_handle);
        Ok(header_handle)
    }

    /// 寸法スタイルのハンドルと値、文字スタイル。名前の省略は見出しの $DIMSTYLE、読めなければ Standard
    fn dimension_style(&mut self, index: usize, name: Option<&str>) -> Result<(u64, DimStyleValues, u64), EditError> {
        let handle = match name {
            Some(name) => self.dimstyles.get(&upper(name)).copied().ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("寸法スタイル {name} は図面にありません"))
            })?,
            None => {
                let current = match &self.header {
                    HeaderForm::Codec { vars, .. } => handle_var(vars, "$DIMSTYLE"),
                    HeaderForm::Patch { .. } => 0,
                };
                if self.dimstyles.values().any(|handle| *handle == current) {
                    current
                } else {
                    self.dimstyles.get("STANDARD").copied().ok_or_else(|| {
                        EditError::at(index, "missing-definition", "寸法スタイル Standard は図面にありません")
                    })?
                }
            }
        };
        let style = self
            .read(handle, |s, _| DimStyle::read(s))
            .ok_or_else(|| EditError::at(index, "unreadable", "寸法スタイル（DIMSTYLE）が読めません"))?
            .value;
        let mut values = DimStyleValues::default();
        let number = |value: &DimValue| match value {
            DimValue::Bit(bit) => Some(f64::from(u8::from(*bit))),
            DimValue::Short(value) => Some(f64::from(*value)),
            DimValue::Double(value) => Some(*value),
            DimValue::Color(color) => Some(f64::from(color.index)),
            DimValue::Text(_) => None,
        };
        for (_, variable) in DimStyleValues::CODES {
            if let Some(value) = style.get(variable).and_then(number) {
                values.set(variable, value);
            }
        }
        // 描かない変数（矢印のブロック・許容差・換算単位・寸法補助線の省略）
        let flags = ["DIMSAH", "DIMTOL", "DIMALT", "DIMSE1", "DIMSE2", "DIMSD1", "DIMSD2"]
            .iter()
            .any(|variable| style.get(variable).and_then(number).is_some_and(|value| value != 0.0));
        let blocks = style.handles.iter().skip(1).take(4).any(|block| block.value != 0);
        if flags || blocks {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-style",
                "寸法スタイルの矢印のブロック・許容差・換算単位・寸法補助線の省略は描かず、塗りつぶしの閉じた矢印で描きました",
            );
        }
        let text_style = style
            .handles
            .first()
            .map(|style| style.resolve(handle))
            .filter(|style| self.styles.values().any(|known| known == style))
            .unwrap_or_else(|| self.default_style());
        Ok((handle, values, text_style))
    }

    /// 寸法（20.4.22〜20.4.26 の回転寸法・平行寸法・3 点角度寸法・半径寸法・直径寸法）と、その表示の名前のない
    /// ブロック（*D…）
    fn dimension(&mut self, index: usize, reader: &ValueReader<'_>) -> Result<(u16, Body), EditError> {
        let input = DimensionInput::read(reader, index)?;
        let text = reader.text("text")?.unwrap_or("<>");
        let (style_handle, style, text_style) = self.dimension_style(index, reader.name("dimensionStyle")?)?;
        if style.text_above == 3 {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-text-position",
                "寸法文字の JIS 配置（DIMTAD=3）は寸法線の上に置いて近似しました",
            );
        }
        if matches!(input, DimensionInput::Angular { .. }) && style.angle_unit != 0 {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-angle-unit",
                "角度寸法の単位（DIMAUNIT）が 10 進の度でないため、10 進の度で描きました",
            );
        }
        let layout = input.layout(text, &style).map_err(|message| EditError::at(index, "invalid-value", message))?;
        let next = self
            .blocks
            .keys()
            .filter_map(|name| name.strip_prefix("*D").and_then(|number| number.parse::<u64>().ok()))
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| {
                EditError::at(index, "name-exhausted", "新しい寸法の表示のブロック名を割り当てられません")
            })?;
        let block_name = format!("*D{next}");
        let block = self.create_block(index, &block_name, [0.0; 3], true)?;
        let place = Place { block, mode: 0 };
        for (part, role) in &layout.parts {
            let aci = style.colors[match role {
                Role::DimensionLine => 0,
                Role::ExtensionLine => 1,
                Role::Text => 2,
            }];
            let color = match aci {
                0 => EditValue::Text("byblock".into()),
                256 => EditValue::Text("bylayer".into()),
                value => EditValue::Number(value.clamp(1, 255) as f64),
            };
            let values = EditValues::from([("color".to_string(), color)]);
            let mut common = self.new_common(index, place, &ValueReader::new(&values, index))?;
            let (type_number, body) = match part {
                Part::Line(from, to) => (kind::LINE, build::line(*from, *to)),
                Part::Arc { center, radius, angle } => (kind::ARC, build::arc(*center, *radius, *angle)),
                Part::Solid(corners) => (kind::SOLID, build::solid(*corners)),
                Part::Text { position, height, rotation, text } => {
                    let content = self.raw_text(&dimension_content(text));
                    let body = build::mtext(self.version, *position, content, *height, *rotation, None, text_style, 5);
                    (kind::MTEXT, body)
                }
            };
            let handle = self.next_handle(index)?;
            self.append(index, place, handle, &mut common)?;
            let entity = Decoded { kind: type_number, eed: Vec::new(), value: Entity { common, body } };
            self.store_entity(handle, &entity);
        }
        let (type_number, dimension_kind) = match input {
            DimensionInput::Linear { kind: DimensionKind::Linear(angle), start, end, .. } => (
                kind::DIMENSION_LINEAR,
                DwgDimensionKind::Linear {
                    p13: start,
                    p14: end,
                    p10: layout.definition,
                    oblique: 0.0,
                    rotation: Some(angle.to_radians()),
                },
            ),
            DimensionInput::Linear { kind: DimensionKind::Aligned, start, end, .. } => (
                kind::DIMENSION_ALIGNED,
                DwgDimensionKind::Linear { p13: start, p14: end, p10: layout.definition, oblique: 0.0, rotation: None },
            ),
            DimensionInput::Angular { center, start, end, .. } => (
                kind::DIMENSION_ANG3PT,
                DwgDimensionKind::Angular3 { p10: layout.definition, p13: start, p14: end, p15: center },
            ),
            DimensionInput::Radius { point, .. } => {
                (kind::DIMENSION_RADIUS, DwgDimensionKind::Radius { p10: layout.definition, p15: point, leader: 0.0 })
            }
            DimensionInput::Diameter { point, .. } => (
                kind::DIMENSION_DIAMETER,
                DwgDimensionKind::Diameter { p15: point, p10: layout.definition, leader: 0.0 },
            ),
        };
        let shown = if text == "<>" { "" } else { text };
        let body = Dimension {
            version: 0,
            extrusion: [0.0, 0.0, 1.0],
            text_midpoint: [layout.text_midpoint[0], layout.text_midpoint[1]],
            elevation: layout.text_midpoint[2],
            // 1 は 70 の 128 が立っていない、2 はこの寸法だけが表示のブロックを使う（70 の 32）、8 は AutoCAD が
            // どの寸法にも立てる（モデルの書き出しと同じ）
            flags1: 8 | 1 | 2,
            text: self.raw_text(shown),
            text_rotation: 0.0,
            horizontal_direction: 0.0,
            insert_scale: [1.0; 3],
            insert_rotation: 0.0,
            attachment: 5,
            line_spacing_style: 1,
            line_spacing_factor: 1.0,
            measurement: layout.measurement,
            flips: [false; 3],
            insert: [0.0, 0.0],
            kind: dimension_kind,
            dimstyle: href(5, style_handle),
            block: href(5, block),
        };
        Ok((type_number, Body::Dimension(body)))
    }

    /// 表の管理のオブジェクトに項目を足す
    fn add_entry(&mut self, index: usize, control: u64, control_kind: u16, entry: u64) -> Result<(), EditError> {
        let mut table = self
            .read(control, Control::read)
            .ok_or_else(|| EditError::at(index, "unreadable", "DWG の表の管理のオブジェクトが読めません"))?;
        table.value.entries.push(href(2, entry));
        let (kind, eed) = (table.kind, table.eed.clone());
        self.store(control, kind, &eed, |out| table.value.write(out, control_kind));
        Ok(())
    }

    /// APPID の ACAD のハンドル
    fn acad_appid(&self) -> Option<u64> {
        self.control(self.tables.appid_control).into_iter().find(|handle| {
            self.read(*handle, |s, _| Appid::read(s))
                .is_some_and(|appid| upper(&self.text_of(&appid.value.name.name)) == "ACAD")
        })
    }

    /// 拡張データの文字列（1000）。R2007 以降は印 0・RS の文字の数・UTF-16、それより前は印 0・RC のバイト数・文字コードの
    /// 番号（上位のバイトが先）・バイト（モデルの書き出しの eed_string と同じ）
    fn eed_string(&self, out: &mut Vec<u8>, value: &str) {
        out.push(0);
        match self.raw_text(value) {
            RawText::Bytes(mut bytes) => {
                bytes.pop();
                bytes.truncate(255);
                out.push(bytes.len() as u8);
                out.extend_from_slice(&self.source.codepage.to_be_bytes());
                out.extend_from_slice(&bytes);
            }
            RawText::Units(units) => {
                out.extend_from_slice(&(units.len().min(i16::MAX as usize) as u16).to_le_bytes());
                for unit in units.iter().take(i16::MAX as usize) {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
            }
        }
    }

    /// 文字スタイル・寸法スタイル・線種を足す（docs/decisions/0017。値は DXF の編集と同じ）
    fn add_style(&mut self, index: usize, kind: StyleKind, name: &str, values: &EditValues) -> Result<(), EditError> {
        if !valid_name(name) {
            return Err(EditError::at(index, "invalid-value", format!("名前 {name} は使えません")));
        }
        let key = upper(name);
        let exists = match kind {
            StyleKind::Text => self.styles.contains_key(&key),
            StyleKind::Dimension => self.dimstyles.contains_key(&key),
            StyleKind::LineType => self.linetypes.contains_key(&key),
        };
        if exists {
            return Err(EditError::at(index, "already-exists", format!("{name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        let r2004 = self.version >= Version::R2004;
        let common = |control: u64| ObjectCommon {
            owner: href(4, control),
            reactors: Vec::new(),
            xdictionary: if r2004 { None } else { Some(href(3, 0)) },
            ds_binary: false,
        };
        match kind {
            StyleKind::Text => {
                reader.allow(&["font", "bigFont", "height", "widthFactor", "oblique"], "文字スタイル")?;
                let font = reader.name("font")?.unwrap_or("txt");
                let big_font = reader.text("bigFont")?.unwrap_or("");
                let height = reader.number("height")?.unwrap_or(0.0);
                if height < 0.0 {
                    return Err(EditError::at(index, "invalid-value", "height は 0 以上の数にしてください"));
                }
                let (file, family) = zumen_dxf::font_file(font);
                let control = self.tables.style_control;
                let style = TextStyle {
                    common: common(control),
                    name: EntryName::new(self.raw_text(name)),
                    shape: false,
                    vertical: false,
                    height,
                    width: reader.positive("widthFactor")?.unwrap_or(1.0),
                    oblique: reader.number("oblique")?.unwrap_or(0.0).to_radians(),
                    generation: 0,
                    last_height: 2.5,
                    font: self.raw_text(&file),
                    bigfont: if big_font.is_empty() { RawText::empty(self.version) } else { self.raw_text(big_font) },
                    xref_block: href(5, 0),
                };
                // TrueType の書体の名前は拡張データ（ACAD の 1000 と 1071。モデルの書き出しと同じ）
                let mut eed = Vec::new();
                if let Some(family) = family {
                    match self.acad_appid() {
                        Some(app) => {
                            let mut data = Vec::new();
                            self.eed_string(&mut data, &family);
                            data.push(71);
                            data.extend_from_slice(&0i32.to_le_bytes());
                            eed.push((href(5, app), data));
                        }
                        None => self.notices.add(
                            NoticeLevel::Info,
                            "edit-font-family",
                            "APPID の ACAD がない図面なので、TrueType の書体の名前を書かず、フォントのファイルだけを書きました",
                        ),
                    }
                }
                let handle = self.next_handle(index)?;
                self.store(handle, kind::STYLE, &eed, |out| style.write(out));
                self.add_entry(index, control, kind::STYLE_CONTROL, handle)?;
                self.styles.insert(key, handle);
            }
            StyleKind::LineType => {
                reader.allow(&["description", "pattern"], "線種")?;
                let description = reader.text("description")?.unwrap_or("");
                let pattern = match reader.value("pattern") {
                    None => Vec::new(),
                    Some(EditValue::Numbers(values)) if values.len() <= 12 => values.clone(),
                    Some(EditValue::Number(value)) => vec![*value],
                    Some(_) => {
                        return Err(EditError::at(
                            index,
                            "invalid-value",
                            "pattern は 12 個までの数の配列にしてください",
                        ));
                    }
                };
                let control = self.tables.linetype_control;
                let linetype = Linetype {
                    common: common(control),
                    name: EntryName::new(self.raw_text(name)),
                    description: self.raw_text(description),
                    pattern_length: pattern.iter().map(|dash| dash.abs()).sum(),
                    alignment: b'A',
                    dashes: pattern
                        .iter()
                        .map(|length| Dash {
                            length: *length,
                            shape: 0,
                            offset: [0.0; 2],
                            scale: 1.0,
                            rotation: 0.0,
                            flags: 0,
                        })
                        .collect(),
                    // 文字の要素の領域は R2004 までは 256 バイト、R2007 以降は文字の要素があるときだけ（モデルの書き出しと同じ）
                    strings: if self.version >= Version::R2007 { Vec::new() } else { vec![0; 256] },
                    xref_block: href(5, 0),
                    shapefiles: vec![href(5, 0); pattern.len()],
                };
                let handle = self.next_handle(index)?;
                self.store(handle, kind::LTYPE, &[], |out| linetype.write(out));
                self.add_entry(index, control, kind::LTYPE_CONTROL, handle)?;
                self.linetypes.insert(key, handle);
            }
            StyleKind::Dimension => {
                reader.allow(&zumen_dxf::dimension::STYLE_KEYS, "寸法スタイル")?;
                let variables = zumen_dxf::dimension::style_variables(&reader, index)?;
                let from = reader.name("from")?.unwrap_or("Standard");
                let template = self.dimstyles.get(&upper(from)).copied().ok_or_else(|| {
                    EditError::at(index, "missing-definition", format!("寸法スタイル {from} は図面にありません"))
                })?;
                let text_style = self.text_style(index, reader.name("textStyle")?)?;
                let mut style = self
                    .read(template, |s, _| DimStyle::read(s))
                    .ok_or_else(|| EditError::at(index, "unreadable", "寸法スタイル（DIMSTYLE）が読めません"))?
                    .value;
                let control = self.tables.dimstyle_control;
                // DWG 2.13: 相対ハンドルの基準はそのオブジェクト。別のハンドルに写す前に、元のスタイルを基準に解く。
                style.xref_block = href(5, style.xref_block.resolve(template));
                for handle in &mut style.handles {
                    *handle = href(5, handle.resolve(template));
                }
                style.common = common(control);
                style.name = EntryName::new(self.raw_text(name));
                for (variable, number) in variables {
                    if let Some((_, value)) = style.values.iter_mut().find(|(key, _)| *key == variable) {
                        *value = match value {
                            DimValue::Bit(_) => DimValue::Bit(number != 0.0),
                            DimValue::Short(_) => DimValue::Short(number as i16),
                            _ => DimValue::Double(number),
                        };
                    }
                }
                if let (Some(text_style), Some(first)) = (text_style, style.handles.first_mut()) {
                    *first = href(5, text_style);
                }
                let handle = self.next_handle(index)?;
                self.store(handle, kind::DIMSTYLE, &[], |out| style.write(out));
                self.add_entry(index, control, kind::DIMSTYLE_CONTROL, handle)?;
                self.dimstyles.insert(key, handle);
            }
        }
        self.created.push(name.to_string());
        Ok(())
    }

    fn update_layer(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        let reader = ValueReader::new(values, index);
        reader.allow(&LAYER_KEYS, "レイヤの変更")?;
        let handle = self
            .layers
            .get(&upper(name))
            .copied()
            .ok_or_else(|| EditError::at(index, "not-found", format!("レイヤ {name} は図面にありません")))?;
        let mut layer = self
            .read(handle, |s, _| Layer::read(s))
            .ok_or_else(|| EditError::at(index, "unreadable", format!("レイヤ {name} が読めません")))?;
        self.apply_layer_values(index, &mut layer.value, &reader)?;
        let (kind, eed) = (layer.kind, layer.eed.clone());
        self.store(handle, kind, &eed, |out| layer.value.write(out));
        Ok(())
    }

    /// レイヤの値（LAYER の BS: 1 凍結、2 非表示、8 ロック、16 印刷、0x3E0 線の太さの番号。色は CMC）
    fn apply_layer_values(
        &mut self,
        index: usize,
        layer: &mut Layer,
        reader: &ValueReader<'_>,
    ) -> Result<(), EditError> {
        match reader.color("color")? {
            Some(EditColor::ByLayer | EditColor::ByBlock) => {
                return Err(EditError::at(index, "invalid-value", "レイヤの色に bylayer・byblock は使えません"));
            }
            Some(color) => {
                let (aci, rgb) = self.color_parts(index, color)?;
                layer.color = self.cmc(aci, rgb);
            }
            None => {}
        }
        match reader.name("lineType")? {
            Some(name) if matches!(upper(name).as_str(), "BYLAYER" | "BYBLOCK") => {
                return Err(EditError::at(index, "invalid-value", "レイヤの線種に ByLayer・ByBlock は使えません"));
            }
            Some(name) => {
                let (_, handle) = self.line_type(index, name)?;
                if let Some(handle) = handle {
                    layer.linetype = handle;
                }
            }
            None => {}
        }
        let before_2000 = self.version < Version::R2000;
        match reader.line_width("lineWidth")? {
            Some(EditLineWidth::ByLayer | EditLineWidth::ByBlock) => {
                return Err(EditError::at(index, "invalid-value", "レイヤの線の太さに bylayer・byblock は使えません"));
            }
            // R13・R14 のレイヤは線の太さを持たない（既定の太さ）
            Some(width) if before_2000 => {
                if width != EditLineWidth::Default {
                    self.no_lineweight();
                }
            }
            Some(width) => {
                let weight = self.lineweight(width);
                layer.values = (layer.values & !0x3e0) | (i16::from(weight) << 5);
            }
            None => {}
        }
        let mut flags = layer.values;
        let set = |flags: &mut i16, bit: i16, on: Option<bool>| {
            if let Some(on) = on {
                *flags = if on { *flags | bit } else { *flags & !bit };
            }
        };
        set(&mut flags, 1, reader.bool("frozen")?);
        set(&mut flags, 2, reader.bool("visible")?.map(|visible| !visible));
        set(&mut flags, 8, reader.bool("locked")?);
        match reader.bool("plot")? {
            // R13・R14 のレイヤはどれも印刷する
            Some(false) if before_2000 => self.notices.add(
                NoticeLevel::Info,
                "edit-no-layer-plot",
                "AutoCAD 2000 より前の DWG にはレイヤを印刷しない設定がないため、書きませんでした",
            ),
            Some(_) if before_2000 => {}
            plot => set(&mut flags, 16, plot),
        }
        layer.values = flags;
        // R2000 の非表示のレイヤは色の番号を負にもする（DXF の 62 と同じ）
        if self.version < Version::R2004 {
            let off = flags & 2 != 0;
            let aci = layer.color.index.abs().max(1);
            layer.color.index = if off { -aci } else { aci };
        }
        Ok(())
    }

    // ---- 書く ----

    fn finish(mut self) -> Edited {
        if self.seed_used {
            match &mut self.header {
                HeaderForm::Codec { vars, extra } => {
                    vars.set("$HANDSEED", Value::Handle(href(0, self.seed)));
                    let data = write_header_vars(vars);
                    self.source.header =
                        Framed { extra: *extra, data, crc_ok: true }.bytes(&HEADER_SENTINEL, &HEADER_END, 0);
                }
                HeaderForm::Patch { at } => {
                    // 値のバイト（コードと数の 1 バイトの後ろ、上位から）を書き換え、CRC を計算し直す
                    let bytes = ((at.1 - at.0) / 8).saturating_sub(1);
                    put_bits(&mut self.source.header, at.0 + 8, self.seed, (bytes * 8) as u32);
                    let extra = has_extra(self.version, self.source.maintenance);
                    if let Some(framed) = Framed::parse(&self.source.header, &HEADER_SENTINEL, &HEADER_END, extra) {
                        self.source.header = framed.bytes(&HEADER_SENTINEL, &HEADER_END, 0);
                    }
                }
            }
            // AcDb:AuxHeader の HANDSEED（27。0x7FFFFFFF 以上なら -1）
            let seed = if self.seed < 0x7fff_ffff { self.seed as u32 } else { u32::MAX };
            if let Some(aux) = self.source.aux_header_mut()
                && aux.len() >= 83
            {
                aux[79..83].copy_from_slice(&seed.to_le_bytes());
            }
        }
        let deleted = std::mem::take(&mut self.deleted);
        self.source.objects.handles.retain(|(handle, _)| !deleted.contains(handle));
        let bytes = self.source.write();
        Edited { bytes, created: self.created, notices: self.notices.into_vec() }
    }
}

/// POLYLINE が持つ頂点と SEQEND（R2004 以降は並び、R2000 は最初と最後）
fn owned(owned: &crate::codec::entities::Owned, handle: u64) -> (Vec<u64>, Option<(u64, u64)>, Option<u64>) {
    (
        owned.vertices.iter().map(|vertex| vertex.resolve(handle)).collect(),
        owned.first_last.map(|(first, last)| (first.resolve(handle), last.resolve(handle))),
        Some(owned.seqend.resolve(handle)),
    )
}

/// R2000 の前の図形（Nolinks なら 1 つ前のハンドル）
#[cfg(test)]
fn linked_previous(common: &EntityCommon, handle: u64) -> u64 {
    match common.links {
        Some((previous, _)) => previous.resolve(handle),
        None => handle.wrapping_sub(1),
    }
}

/// R2000 の次の図形（Nolinks なら 1 つ後のハンドル）
fn linked_next(common: &EntityCommon, handle: u64) -> u64 {
    match common.links {
        Some((_, next)) => next.resolve(handle),
        None => handle.wrapping_add(1),
    }
}

/// 尺度。数（すべての向き）、[x, y]、[x, y, z]（DXF の編集と同じ）
fn scale(index: usize, reader: &ValueReader<'_>) -> Result<Option<[f64; 3]>, EditError> {
    let scale = match reader.value("scale") {
        None => return Ok(None),
        Some(EditValue::Number(value)) => [*value; 3],
        Some(_) => {
            let point = reader.required("scale", reader.point("scale")?)?;
            [
                point[0],
                point[1],
                if reader.numbers("scale", 3).is_ok_and(|values| values.is_some()) { point[2] } else { 1.0 },
            ]
        }
    };
    if scale.contains(&0.0) {
        return Err(EditError::at(index, "invalid-value", "scale は 0 でない数にしてください"));
    }
    Ok(Some(scale))
}

/// RGB に最も近い ACI（1〜255）
fn nearest_aci(rgb: u32) -> u16 {
    let channels = |value: u32| [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
    let target = channels(rgb);
    let mut best = (7u16, u32::MAX);
    for (index, hex) in zumen_dxf::ACI.iter().enumerate().skip(1) {
        let Ok(value) = u32::from_str_radix(hex.trim_start_matches('#'), 16) else {
            continue;
        };
        let color = channels(value);
        let distance: u32 = (0..3).map(|i| color[i].abs_diff(target[i]).pow(2)).sum();
        if distance < best.1 {
            best = (index as u16, distance);
        }
    }
    best.0
}

/// MTEXT の文字列にする（\ { } を書式コードから区別し、改行は \P。DXF の編集と同じ）
fn mtext_content(text: &str) -> String {
    // raw_text はモデルの正規形ではなく、元の文字列を受け取る。
    zumen_dxf::escape_mtext(text).replace("^ ", "^")
}

/// 寸法の表示が作った %%d・%%c は DXF Reference「Control Codes and Special Characters」の制御コード。
fn dimension_content(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\P"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests;
