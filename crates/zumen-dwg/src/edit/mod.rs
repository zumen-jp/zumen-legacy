//! DWG のその場の編集（docs/decisions/0012、docs/formats/dwg.md「Editing」）。図面を開き（write::rewrite::Source）、
//! 操作したオブジェクトだけを可逆の読み書き（src/codec）で読み直して書き換え、新しいオブジェクトを足す。触れていない
//! オブジェクトはビット列のまま写すので、拡張データ・リアクタ・拡張辞書と、読み取りが扱わないオブジェクトも残る。
//! 版・文字コードも元のまま。値のキーと決まりは DXF の編集（zumen-dxf の edit）と同じ。
//!
//! - 足す図形と消す図形は、持ち主のブロック（BLOCK_HEADER、20.4.52）の図形の並び（R2004 以降）か、前後の図形の
//!   つながりと最初・最後の図形（R2000、20.4.1 の Nolinks）を直す
//! - 新しいオブジェクトのハンドルは $HANDSEED（大きいハンドルがあればその次）から振り、$HANDSEED と AcDb:AuxHeader の
//!   値（27）を次のハンドルにする
//! - 消す図形は、従う図形（頂点・属性と SEQEND）、拡張辞書とその中のオブジェクトも消し、描く順（SORTENTSTABLE）と
//!   グループ（GROUP）、リアクタから除く

mod build;

use crate::bits::{HandleRef, Read};
use crate::codec::entities::{Body, Entity, fixed_name};
use crate::codec::header::{HeaderVars, Value, read_header_vars, write_header_vars};
use crate::codec::object::{EntityColor, EntityCommon, ObjectCommon, open_raw, writer};
use crate::codec::objects::{Dictionary, Group, Layout, SortEntsTable};
use crate::codec::tables::{BlockHeader, Control, EntryName, Layer, Linetype, TextStyle};
use crate::codec::{CmColor, In, RawText};
use crate::file::Version;
use crate::write::database::has_extra;
use crate::write::object::ObjectWriter;
use crate::write::rewrite::Source;
use crate::write::sections::{Framed, HEADER_END, HEADER_SENTINEL};
use encoding_rs::Encoding;
use std::collections::{HashMap, HashSet};
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValue, EditValues, Edited, ValueReader,
};
use zumen_core::{NoticeLevel, NoticeList};

/// 足せる図形の種類（DXF の編集と同じ）
const ADDABLE: [&str; 9] =
    ["line", "polyline", "arc", "circle", "ellipse", "ellipse-arc", "point", "text", BLOCK_REFERENCE];

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
}

/// DWG を編集する。すべての操作を確かめてから書き、1 つでも通らなければエラーを返して何も書かない
pub fn edit_dwg(bytes: &[u8], operations: &[EditOperation]) -> Result<Edited, EditError> {
    if !crate::is_dwg(bytes) {
        return Err(EditError::new("unreadable", "DWG として読めません"));
    }
    if matches!(&bytes[..6], b"AC1012" | b"AC1014") {
        return Err(EditError::new("unreadable", "AutoCAD R13・R14 の DWG はまだ編集できません"));
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
    /// レイアウトの名前（大文字）→ BLOCK_HEADER
    layouts: HashMap<String, u64>,
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
        let seed = tables.handseed.max(largest + 1);
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
            layouts: HashMap::new(),
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
        if let HeaderForm::Patch { at } = self.header {
            let bytes = (at.1.saturating_sub(at.0) / 8).saturating_sub(1);
            if bytes < 8 && (self.seed + 1) >> (bytes * 8) != 0 {
                return Err(EditError::at(
                    index,
                    "unreadable",
                    "DWG のヘッダ変数を読み切れないため、これより多くのオブジェクトは足せません",
                ));
            }
        }
        let handle = self.seed;
        self.seed += 1;
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
        let layouts = self.tables.layouts;
        if let Some(dictionary) = self.read(layouts, |s, _| Dictionary::read(s, false)) {
            for (_, entry) in &dictionary.value.entries {
                let handle = entry.resolve(layouts);
                if let Some(layout) = self.read(handle, |s, _| Layout::read(s)) {
                    let name = self.text_of(&layout.value.name);
                    self.layouts.insert(upper(&name), layout.value.block_record.resolve(handle));
                }
            }
        }
    }

    fn line_type(&self, index: usize, name: &str) -> Result<(u8, Option<HandleRef>), EditError> {
        match upper(name).as_str() {
            "BYLAYER" => Ok((0, None)),
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
        let lineweight = reader.line_width("lineWidth")?.map(|width| self.lineweight(width));
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
                (kind::LWPOLYLINE, build::lwpolyline(&vertices, &bulges, closed, width))
            }
            "text" => {
                reader.allow(&keys(&["position", "text", "height", "rotation", "textStyle", "wrapWidth"]), "文字")?;
                let position = reader.required("position", reader.point("position")?)?;
                let text = reader.required("text", reader.text("text")?)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let style = self.text_style(index, reader.name("textStyle")?)?.unwrap_or_else(|| self.default_style());
                let wrap = reader.number("wrapWidth")?;
                if text.contains('\n') || wrap.is_some() {
                    let content = self.raw_text(&mtext_content(text));
                    let body = build::mtext(self.version, position, content, height, rotation, wrap, style);
                    (kind::MTEXT, body)
                } else {
                    let value = self.raw_text(text);
                    (kind::TEXT, build::text(position, value, height, rotation, style))
                }
            }
            _ => {
                reader.allow(&keys(&["block", "position", "rotation", "scale"]), "ブロック参照")?;
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
                (kind::INSERT, build::insert(position, scale, rotation, block))
            }
        };
        let mut common = self.new_common(index, place, &reader)?;
        let handle = self.next_handle(index)?;
        self.append(index, place, handle, &mut common)?;
        if let Body::Insert(insert) = &body {
            self.add_insert(insert.block.value, handle);
        }
        let entity = Decoded { kind: type_number, eed: Vec::new(), value: Entity { common, body } };
        self.store_entity(handle, &entity);
        self.created.push(format!("{handle:X}"));
        Ok(())
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
        let reader = ValueReader::new(set, index);
        let own: &[&'static str] = match decoded.kind {
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
        match reader.line_width("lineWidth")? {
            Some(EditLineWidth::ByLayer | EditLineWidth::ByBlock) => {
                return Err(EditError::at(index, "invalid-value", "レイヤの線の太さに bylayer・byblock は使えません"));
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
        set(&mut flags, 16, reader.bool("plot")?);
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
