//! DXF のその場の編集（docs/decisions/0012、docs/formats/dxf.md「Editing」）。元のタグの並びを保ち、操作した図形の
//! タグだけを書き換える。触れていないタグは元のバイト列のまま書き戻すので、拡張データ・リアクタ・拡張辞書・
//! 読み取りが扱わない図形とオブジェクトも残る。版・テキストかバイナリか・改行・文字コードも元のまま。
//!
//! グループコードは DXF Reference（AutoCAD 2026）の各図形の節のとおり。R12（AC1009）にはサブクラスの印（100）・
//! 所有者（330）・線の太さ（370）・トゥルーカラー（420）がなく、ハンドル（5）は $HANDLING が 1 のときだけある。

mod raw;

use crate::colors::ACI;
use crate::write::real;
use raw::{RawDxf, Tag};
use std::collections::{BTreeMap, HashMap, HashSet};
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValue, EditValues, Edited, ValueReader,
};
use zumen_core::{NoticeLevel, NoticeList};

/// 線の太さ（370）に書ける値（1/100 mm。DXF Reference「Common Group Codes for Entities」の lineweight の列挙）
const LINEWEIGHTS: [i64; 24] =
    [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

/// 足せる図形の種類（DXF）
const ADDABLE: [&str; 9] =
    ["line", "polyline", "arc", "circle", "ellipse", "ellipse-arc", "point", "text", BLOCK_REFERENCE];

/// 図形・レイヤに共通する値のキー
const COMMON: [&str; 4] = ["layer", "color", "lineType", "lineWidth"];

/// レイヤの値のキー
const LAYER_KEYS: [&str; 7] = ["color", "lineType", "lineWidth", "visible", "frozen", "locked", "plot"];

/// DXF を編集する。すべての操作を確かめてから書き、1 つでも通らなければエラーを返して何も書かない
pub fn edit_dxf(bytes: &[u8], operations: &[EditOperation]) -> Result<Edited, EditError> {
    if !crate::is_dxf(bytes) {
        return Err(EditError::new("unreadable", "DXF として読めません"));
    }
    let source = RawDxf::parse(bytes);
    let mut editor = Editor::new(&source);
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            EditOperation::Add { kind, container, values } => editor.add(index, kind, container.as_deref(), values)?,
            EditOperation::Update { id, set } => editor.update(index, id, set)?,
            EditOperation::Delete { id } => editor.delete(index, id)?,
            EditOperation::AddLayer { name, values } => editor.add_layer(index, name, values, false)?,
            EditOperation::UpdateLayer { name, set } => editor.update_layer(index, name, set)?,
        }
    }
    editor.finish_handles();
    let bytes = editor.write();
    Ok(Edited { bytes, created: editor.created, notices: editor.notices.into_vec() })
}

/// 図形のレコード（コード 0 のタグから次のコード 0 の前まで）と、それに従うレコード（VERTEX・ATTRIB・SEQEND）
#[derive(Debug, Clone)]
struct EntityInfo {
    record: usize,
    /// 従うレコードの終わり（record + 1 + 従う数）
    end: usize,
    kind: String,
    /// ATTRIB が従う INSERT のハンドル
    parent: Option<String>,
}

/// ブロック定義
#[derive(Debug, Clone)]
struct BlockInfo {
    name: String,
    /// ENDBLK のレコード
    end: Option<usize>,
}

/// 足す先
struct Place {
    /// このレコードの前に足す
    before: usize,
    owner: Option<String>,
    paper: bool,
}

struct Editor<'a> {
    source: &'a RawDxf<'a>,
    records: Vec<Vec<Tag>>,
    /// R13 以降（サブクラスの印・所有者がある）
    r13: bool,
    /// 図形がハンドルを持つ
    handles: bool,
    header: Option<usize>,
    entities_end: Option<usize>,
    entities: HashMap<String, EntityInfo>,
    blocks: HashMap<String, BlockInfo>,
    /// BLOCK_RECORD の名前（大文字）→ ハンドル
    block_records: HashMap<String, String>,
    /// BLOCK_RECORD のハンドル → 名前
    block_record_names: HashMap<String, String>,
    /// レイアウトの名前（大文字）→ 紙空間の BLOCK_RECORD のハンドル
    layouts: HashMap<String, String>,
    /// 表（LAYER など）の TABLE と ENDTAB のレコード
    tables: HashMap<String, (usize, Option<usize>)>,
    /// 節（HEADER など）の SECTION と ENDSEC のレコード
    sections: HashMap<String, (usize, Option<usize>)>,
    /// 図面にレイヤの表がないときに作った表（足す位置、TABLE のレコードの位置、ハンドル）
    created_table: Option<(usize, usize, Option<String>)>,
    /// 足したレコードの後に閉じるレコード（作った表の ENDTAB・ENDSEC）
    closers: HashMap<usize, Vec<Vec<Tag>>>,
    /// レイヤの名前（大文字）→ レコード（足したレイヤは足すレコードの番号）
    layers: HashMap<String, LayerRef>,
    linetypes: HashMap<String, String>,
    styles: HashMap<String, String>,
    /// オブジェクトの所有者のハンドル → 所有されるオブジェクトのレコード
    owned: HashMap<String, Vec<usize>>,
    /// レコードのハンドル
    record_handles: HashMap<usize, String>,
    /// SORTENTSTABLE・GROUP と、リアクタを持つレコード（消した図形の参照を除く）
    references: Vec<usize>,
    seed: u64,
    seed_used: bool,
    modified: HashMap<usize, Vec<Tag>>,
    deleted: HashSet<usize>,
    inserted: BTreeMap<usize, Vec<Vec<Tag>>>,
    created: Vec<String>,
    notices: NoticeList,
}

/// レイヤのレコードのありか
#[derive(Debug, Clone)]
enum LayerRef {
    Record(usize),
    /// 足したレイヤ（inserted の ENDTAB の前の、何番目か）
    Added(usize, usize),
}

fn upper(text: &str) -> String {
    text.trim().to_uppercase()
}

/// ハンドルの 16 進の値
fn handle_value(text: &str) -> Option<u64> {
    u64::from_str_radix(text.trim(), 16).ok()
}

/// コードの最初のタグ。102 の組（{ACAD_REACTORS など）の中と、拡張データ（1001 から）は見ない
fn find(tags: &[Tag], code: i32) -> Option<usize> {
    find_from(tags, code, 1)
}

fn find_from(tags: &[Tag], code: i32, from: usize) -> Option<usize> {
    let mut group = false;
    for (index, tag) in tags.iter().enumerate().skip(from) {
        match tag.code {
            1001 => return None,
            102 => group = tag.text().starts_with('{'),
            _ if !group && tag.code == code => return Some(index),
            _ => {}
        }
    }
    None
}

fn value(tags: &[Tag], code: i32) -> Option<&str> {
    find(tags, code).map(|index| tags[index].text())
}

fn number(tags: &[Tag], code: i32) -> Option<f64> {
    find(tags, code).map(|index| tags[index].number()).filter(|value| value.is_finite())
}

/// 拡張データ（1001）の始まり、なければ終わり
fn data_end(tags: &[Tag]) -> usize {
    tags.iter().position(|tag| tag.code == 1001).unwrap_or(tags.len())
}

/// タグの値を変える。なければ after のコードのうち最も後ろのタグの後（なければ拡張データの前）に足す
fn set(tags: &mut Vec<Tag>, code: i32, value: String, after: &[i32]) {
    if let Some(index) = find(tags, code) {
        tags[index] = Tag::new(code, value);
        return;
    }
    let at = after.iter().filter_map(|code| find(tags, *code)).max().map_or_else(|| data_end(tags), |index| index + 1);
    tags.insert(at, Tag::new(code, value));
}

fn remove(tags: &mut Vec<Tag>, code: i32) {
    while let Some(index) = find(tags, code) {
        tags.remove(index);
    }
}

fn set_point(tags: &mut Vec<Tag>, code: i32, point: [f64; 3], after: &[i32]) {
    set(tags, code, real(point[0]), after);
    set(tags, code + 10, real(point[1]), &[code]);
    if point[2] != 0.0 || find(tags, code + 20).is_some() {
        set(tags, code + 20, real(point[2]), &[code + 10]);
    }
}

fn get_point(tags: &[Tag], code: i32) -> Option<[f64; 3]> {
    Some([number(tags, code)?, number(tags, code + 10)?, number(tags, code + 20).unwrap_or(0.0)])
}

/// 新しいレコードのタグを作る
struct Record {
    tags: Vec<Tag>,
}

impl Record {
    fn new(kind: &str) -> Self {
        Self { tags: vec![Tag::new(0, kind)] }
    }

    fn tag(&mut self, code: i32, value: impl Into<String>) -> &mut Self {
        self.tags.push(Tag::new(code, value));
        self
    }

    fn real(&mut self, code: i32, value: f64) -> &mut Self {
        self.tag(code, real(value))
    }

    fn int(&mut self, code: i32, value: i64) -> &mut Self {
        self.tag(code, value.to_string())
    }

    fn point(&mut self, code: i32, point: [f64; 3], z: bool) -> &mut Self {
        self.real(code, point[0]).real(code + 10, point[1]);
        if z {
            self.real(code + 20, point[2]);
        }
        self
    }
}

/// 新しい図面（AutoCAD 2013 の DXF、UTF-8）。units は長さの単位（mm、cm、m、inch、foot、unitless）で、
/// $INSUNITS と $MEASUREMENT にする（DXF Reference「HEADER Section Group Codes」）
pub fn new_dxf(units: &str) -> Result<Vec<u8>, EditError> {
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
    let mut doc = crate::parse_dxf(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
    doc.header.insunits = insunits;
    doc.header.measurement = Some(measurement);
    Ok(crate::write_dxf(&doc).bytes)
}

/// 図形の共通の値
struct Common {
    layer: String,
    color: Option<EditColor>,
    line_type: Option<String>,
    line_width: Option<EditLineWidth>,
}

/// RGB に最も近い ACI（1〜255）
fn nearest_aci(rgb: u32) -> i64 {
    let channels = |value: u32| [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
    let target = channels(rgb);
    let mut best = (7, u32::MAX);
    for (index, hex) in ACI.iter().enumerate().skip(1) {
        let Ok(value) = u32::from_str_radix(hex.trim_start_matches('#'), 16) else {
            continue;
        };
        let color = channels(value);
        let distance: u32 = (0..3).map(|i| color[i].abs_diff(target[i]).pow(2)).sum();
        if distance < best.1 {
            best = (index as i64, distance);
        }
    }
    best.0
}

/// 名前に使えない文字（AutoCAD の記号表の名前の規則）
fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.chars().count() <= 255
        && !name.chars().any(|c| c < ' ' || "<>/\\\":;?*|=`".contains(c))
}

impl<'a> Editor<'a> {
    fn new(source: &'a RawDxf<'a>) -> Self {
        let mut records: Vec<Vec<Tag>> = vec![Vec::new()];
        for tag in &source.tags {
            if tag.code == 0 {
                records.push(Vec::new());
            }
            records.last_mut().expect("a record").push(tag.clone());
        }
        let version = source.version.as_str();
        let mut editor = Editor {
            source,
            records,
            r13: version >= "AC1012",
            handles: version >= "AC1012",
            header: None,
            entities_end: None,
            entities: HashMap::new(),
            blocks: HashMap::new(),
            block_records: HashMap::new(),
            block_record_names: HashMap::new(),
            layouts: HashMap::new(),
            tables: HashMap::new(),
            sections: HashMap::new(),
            created_table: None,
            closers: HashMap::new(),
            layers: HashMap::new(),
            linetypes: HashMap::new(),
            styles: HashMap::new(),
            owned: HashMap::new(),
            record_handles: HashMap::new(),
            references: Vec::new(),
            seed: 0,
            seed_used: false,
            modified: HashMap::new(),
            deleted: HashSet::new(),
            inserted: BTreeMap::new(),
            created: Vec::new(),
            notices: NoticeList::new(),
        };
        editor.index();
        editor
    }

    fn kind(&self, record: usize) -> &str {
        self.records[record].first().filter(|tag| tag.code == 0).map_or("", |tag| tag.text())
    }

    /// レコードの並びから、節・表・ブロック・図形・オブジェクトの場所を集める
    fn index(&mut self) {
        let mut section = String::new();
        let mut table = String::new();
        let mut block: Option<String> = None;
        let mut max_handle = 0u64;
        let mut record = 1;
        while record < self.records.len() {
            let kind = self.kind(record).to_string();
            let tags = &self.records[record];
            // 見出しの節の 5 は $HANDSEED の値で、レコードのハンドルではない
            let handle = if kind == "SECTION" {
                None
            } else {
                value(tags, if kind == "DIMSTYLE" { 105 } else { 5 }).map(str::to_string)
            };
            if let Some(handle) = &handle {
                max_handle = max_handle.max(handle_value(handle).unwrap_or(0));
                self.record_handles.insert(record, upper(handle));
            }
            match kind.as_str() {
                "SECTION" => {
                    section = upper(value(tags, 2).unwrap_or(""));
                    self.sections.insert(section.clone(), (record, None));
                    if section == "HEADER" {
                        self.header = Some(record);
                        self.read_header();
                    }
                }
                "ENDSEC" => {
                    if let Some(entry) = self.sections.get_mut(&section) {
                        entry.1 = Some(record);
                    }
                    if section == "ENTITIES" {
                        self.entities_end = Some(record);
                    }
                    section.clear();
                }
                _ if section == "TABLES" => match kind.as_str() {
                    "TABLE" => {
                        table = upper(value(tags, 2).unwrap_or(""));
                        self.tables.insert(table.clone(), (record, None));
                    }
                    "ENDTAB" => {
                        if let Some(entry) = self.tables.get_mut(&table) {
                            entry.1 = Some(record);
                        }
                    }
                    _ => {
                        let name = value(tags, 2).unwrap_or("").to_string();
                        match kind.as_str() {
                            "LAYER" => {
                                self.layers.insert(upper(&name), LayerRef::Record(record));
                            }
                            "LTYPE" => {
                                self.linetypes.insert(upper(&name), name);
                            }
                            "STYLE" if !name.is_empty() => {
                                self.styles.insert(upper(&name), name);
                            }
                            "BLOCK_RECORD" => {
                                if let Some(handle) = &handle {
                                    self.block_records.insert(upper(&name), upper(handle));
                                    self.block_record_names.insert(upper(handle), name);
                                }
                            }
                            _ => {}
                        }
                    }
                },
                _ if section == "BLOCKS" => match kind.as_str() {
                    "BLOCK" => {
                        let name = value(tags, 2).unwrap_or("").to_string();
                        self.blocks.insert(upper(&name), BlockInfo { name: name.clone(), end: None });
                        block = Some(name);
                    }
                    "ENDBLK" => {
                        if let Some(name) = block.take()
                            && let Some(info) = self.blocks.get_mut(&upper(&name))
                        {
                            info.end = Some(record);
                        }
                    }
                    _ => {
                        record = self.index_entity(record, &kind);
                        continue;
                    }
                },
                _ if section == "ENTITIES" => {
                    record = self.index_entity(record, &kind);
                    continue;
                }
                _ if section == "OBJECTS" => self.index_object(record, &kind),
                _ => {}
            }
            record += 1;
        }
        self.seed = self.seed.max(max_handle + 1);
    }

    fn read_header(&mut self) {
        let Some(header) = self.header else {
            return;
        };
        let tags = &self.records[header];
        for (index, tag) in tags.iter().enumerate() {
            if tag.code != 9 {
                continue;
            }
            let next = tags.get(index + 1);
            match tag.text() {
                "$HANDSEED" => {
                    if let Some(seed) = next.and_then(|tag| handle_value(tag.text())) {
                        self.seed = self.seed.max(seed);
                    }
                }
                "$HANDLING" if next.is_some_and(|tag| tag.number() == 1.0) => self.handles = true,
                _ => {}
            }
        }
    }

    /// 図形と、それに従うレコードを集める。次に見るレコードを返す
    fn index_entity(&mut self, record: usize, kind: &str) -> usize {
        let tags = &self.records[record];
        let handle = value(tags, 5).map(upper);
        let mut end = record + 1;
        let attributes = kind == "INSERT" && number(tags, 66) == Some(1.0);
        if kind == "POLYLINE" || attributes {
            let follower = if kind == "POLYLINE" { "VERTEX" } else { "ATTRIB" };
            while end < self.records.len() && self.kind(end) == follower {
                if attributes && let Some(attribute) = value(&self.records[end], 5).map(upper) {
                    self.entities.insert(
                        attribute,
                        EntityInfo { record: end, end: end + 1, kind: "ATTRIB".into(), parent: handle.clone() },
                    );
                }
                end += 1;
            }
            if end < self.records.len() && self.kind(end) == "SEQEND" {
                end += 1;
            }
        }
        if let Some(handle) = handle {
            self.entities.insert(handle, EntityInfo { record, end, kind: kind.to_string(), parent: None });
        }
        end
    }

    fn index_object(&mut self, record: usize, kind: &str) {
        let tags = &self.records[record];
        if let Some(owner) = value(tags, 330) {
            self.owned.entry(upper(owner)).or_default().push(record);
        }
        if kind == "SORTENTSTABLE" || kind == "GROUP" || tags.iter().any(|tag| tag.code == 102) {
            self.references.push(record);
        }
        if kind == "LAYOUT"
            && let Some(start) = tags.iter().position(|tag| tag.code == 100 && tag.text() == "AcDbLayout")
        {
            let name = find_from(tags, 1, start).map(|index| tags[index].text().to_string());
            let block_record = find_from(tags, 330, start).map(|index| upper(tags[index].text()));
            if let (Some(name), Some(block_record)) = (name, block_record) {
                self.layouts.insert(upper(&name), block_record);
            }
        }
    }

    /// 今のタグ（変えたものがあればそれ）
    fn tags(&self, record: usize) -> &[Tag] {
        self.modified.get(&record).unwrap_or(&self.records[record])
    }

    fn tags_mut(&mut self, record: usize) -> &mut Vec<Tag> {
        let records = &self.records;
        self.modified.entry(record).or_insert_with(|| records[record].clone())
    }

    fn next_handle(&mut self) -> Option<String> {
        if !self.handles {
            return None;
        }
        let handle = format!("{:X}", self.seed);
        self.seed += 1;
        self.seed_used = true;
        Some(handle)
    }

    /// 足したハンドルの次を $HANDSEED にする
    fn finish_handles(&mut self) {
        if !self.seed_used {
            return;
        }
        let Some(header) = self.header else {
            return;
        };
        let seed = format!("{:X}", self.seed);
        let tags = self.tags_mut(header);
        match tags.iter().position(|tag| tag.code == 9 && tag.text() == "$HANDSEED") {
            Some(index) if tags.get(index + 1).is_some_and(|tag| tag.code == 5) => tags[index + 1] = Tag::new(5, seed),
            _ => {
                let at = tags.len();
                tags.insert(at, Tag::new(9, "$HANDSEED"));
                tags.insert(at + 1, Tag::new(5, seed));
            }
        }
    }

    fn write(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.source.bytes.len() + 4096);
        self.source.write_prefix(&mut out);
        for (index, record) in self.records.iter().enumerate() {
            for added in [self.inserted.get(&index), self.closers.get(&index)].into_iter().flatten() {
                for tags in added {
                    for tag in tags {
                        self.source.write_tag(tag, &mut out);
                    }
                }
            }
            if self.deleted.contains(&index) {
                continue;
            }
            for tag in self.modified.get(&index).unwrap_or(record) {
                self.source.write_tag(tag, &mut out);
            }
        }
        self.source.write_trailer(&mut out);
        out
    }

    fn insert(&mut self, before: usize, tags: Vec<Tag>) -> usize {
        let list = self.inserted.entry(before).or_default();
        list.push(tags);
        list.len() - 1
    }

    // ---- 名前の確かめ ----

    fn line_type(&self, index: usize, name: &str) -> Result<String, EditError> {
        match upper(name).as_str() {
            "BYLAYER" => Ok("ByLayer".into()),
            "BYBLOCK" => Ok("ByBlock".into()),
            key => {
                self.linetypes.get(key).cloned().ok_or_else(|| {
                    EditError::at(index, "missing-definition", format!("線種 {name} は図面にありません"))
                })
            }
        }
    }

    fn text_style(&self, index: usize, name: Option<&str>) -> Result<Option<String>, EditError> {
        match name {
            None => Ok(None),
            Some(name) => self.styles.get(&upper(name)).cloned().map(Some).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("文字スタイル {name} は図面にありません"))
            }),
        }
    }

    /// レイヤの名前（図面の書き方）。なければ既定の値で足して知らせる
    fn layer(&mut self, index: usize, name: &str) -> Result<String, EditError> {
        if !valid_name(name) {
            return Err(EditError::at(index, "invalid-value", format!("レイヤの名前 {name} は使えません")));
        }
        if let Some(existing) = self.layers.get(&upper(name)) {
            return Ok(self.layer_name(existing).unwrap_or_else(|| name.to_string()));
        }
        if !self.tables.contains_key("LAYER") && self.created_table.is_none() {
            // 表のない図面（ENTITIES だけの DXF など）は、読む側がレイヤを作る
            return Ok(name.to_string());
        }
        self.add_layer(index, name, &EditValues::new(), true)?;
        self.notices.add(NoticeLevel::Info, "edit-added-layer", "図面になかったレイヤを既定の値で足しました");
        Ok(name.to_string())
    }

    fn layer_name(&self, layer: &LayerRef) -> Option<String> {
        let tags = match layer {
            LayerRef::Record(record) => self.tags(*record),
            LayerRef::Added(before, position) => self.inserted.get(before)?.get(*position)?,
        };
        value(tags, 2).map(str::to_string)
    }

    fn common(&mut self, index: usize, reader: &ValueReader<'_>, default_layer: bool) -> Result<Common, EditError> {
        let layer = match reader.name("layer")? {
            Some(name) => self.layer(index, name)?,
            None if default_layer => "0".to_string(),
            None => String::new(),
        };
        let line_type = match reader.name("lineType")? {
            Some(name) => Some(self.line_type(index, name)?),
            None => None,
        };
        Ok(Common { layer, color: reader.color("color")?, line_type, line_width: reader.line_width("lineWidth")? })
    }

    // ---- 値の書き方 ----

    fn lineweight(&mut self, width: EditLineWidth) -> i64 {
        match width {
            EditLineWidth::ByLayer => -1,
            EditLineWidth::ByBlock => -2,
            EditLineWidth::Default => -3,
            EditLineWidth::Millimeters(mm) => {
                let wanted = mm * 100.0;
                let nearest = LINEWEIGHTS
                    .iter()
                    .copied()
                    .min_by(|a, b| (*a as f64 - wanted).abs().total_cmp(&(*b as f64 - wanted).abs()))
                    .unwrap_or(0);
                if (nearest as f64 - wanted).abs() > 0.5 {
                    self.notices.add(
                        NoticeLevel::Info,
                        "edit-rounded-lineweight",
                        "線の太さを、DXF に書ける最も近い太さにしました",
                    );
                }
                nearest
            }
        }
    }

    /// 色のタグ（62、420）。ACI の範囲外ならエラー
    fn color_tags(&mut self, index: usize, color: EditColor) -> Result<(Option<i64>, Option<i64>), EditError> {
        Ok(match color {
            EditColor::ByLayer => (None, None),
            EditColor::ByBlock => (Some(0), None),
            EditColor::Index(256) => (None, None),
            EditColor::Index(0) => (Some(0), None),
            EditColor::Index(value @ 1..=255) => (Some(i64::from(value)), None),
            EditColor::Index(value) => {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("DXF の色の番号（ACI）は 1〜255 です: {value}"),
                ));
            }
            EditColor::Rgb(rgb) => {
                if self.source.version.as_str() >= "AC1018" {
                    (Some(nearest_aci(rgb)), Some(i64::from(rgb)))
                } else {
                    self.notices.add(
                        NoticeLevel::Info,
                        "edit-nearest-color",
                        "AutoCAD 2004 より前の DXF にはトゥルーカラーがないため、最も近い色の番号（ACI）にしました",
                    );
                    (Some(nearest_aci(rgb)), None)
                }
            }
        })
    }

    /// 新しい図形のレコードの始まり（種類、ハンドル、所有者、レイヤ、線種、色、線の太さ）
    fn entity_record(
        &mut self,
        index: usize,
        kind: &str,
        place: &Place,
        common: &Common,
    ) -> Result<(Record, Option<String>), EditError> {
        let mut record = Record::new(kind);
        let handle = self.next_handle();
        if let Some(handle) = &handle {
            record.tag(5, handle.clone());
        }
        if self.r13 {
            if let Some(owner) = &place.owner {
                record.tag(330, owner.clone());
            }
            record.tag(100, "AcDbEntity");
        }
        if place.paper {
            record.int(67, 1);
        }
        record.tag(8, common.layer.clone());
        if let Some(line_type) = &common.line_type
            && line_type != "ByLayer"
        {
            record.tag(6, line_type.clone());
        }
        if let Some(color) = common.color {
            let (aci, rgb) = self.color_tags(index, color)?;
            if let Some(aci) = aci {
                record.int(62, aci);
            }
            if let Some(rgb) = rgb {
                record.int(420, rgb);
            }
        }
        if let Some(width) = common.line_width {
            let weight = self.lineweight(width);
            if self.source.version.as_str() >= "AC1015" {
                record.int(370, weight);
            } else {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-no-lineweight",
                    "AutoCAD 2000 より前の DXF には線の太さがないため、書きませんでした",
                );
            }
        }
        Ok((record, handle))
    }

    fn subclass(&self, record: &mut Record, name: &str) {
        if self.r13 {
            record.tag(100, name);
        }
    }

    /// 足す先。省略・model はモデル空間、layout:<名前> はレイアウトの紙空間、block:<名前> はブロック定義
    fn place(&self, index: usize, container: Option<&str>) -> Result<Place, EditError> {
        let entities = |paper: bool, owner: Option<String>| -> Result<Place, EditError> {
            let before = self
                .entities_end
                .ok_or_else(|| EditError::at(index, "unreadable", "図面に ENTITIES の節がありません"))?;
            Ok(Place { before, owner, paper })
        };
        let model_owner = || self.block_records.get("*MODEL_SPACE").cloned();
        let container = container.map(str::trim).unwrap_or("model");
        if container.eq_ignore_ascii_case("model") {
            return entities(false, model_owner());
        }
        let block_place = |name: &str| -> Result<Place, EditError> {
            let key = upper(name);
            if key == "*MODEL_SPACE" {
                return entities(false, model_owner());
            }
            let owner = self.block_records.get(&key).cloned();
            if key == "*PAPER_SPACE" {
                return entities(true, owner);
            }
            let info = self.blocks.get(&key).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))
            })?;
            let before = info
                .end
                .ok_or_else(|| EditError::at(index, "unreadable", format!("ブロック {name} に ENDBLK がありません")))?;
            Ok(Place { before, owner, paper: key.starts_with("*PAPER_SPACE") })
        };
        if let Some(name) = container.strip_prefix("layout:") {
            if name.eq_ignore_ascii_case("model") {
                return entities(false, model_owner());
            }
            if !self.r13 {
                // R12 の紙空間は 1 つ（ENTITIES の 67 が 1 の図形）
                return entities(true, None);
            }
            let record = self.layouts.get(&upper(name)).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("レイアウト {name} は図面にありません"))
            })?;
            let block = self.block_record_names.get(record).cloned().unwrap_or_default();
            return block_place(&block);
        }
        if let Some(name) = container.strip_prefix("block:") {
            return block_place(name);
        }
        Err(EditError::at(
            index,
            "invalid-value",
            format!("container の {container} は使えません（model、layout:<名前>、block:<名前>）"),
        ))
    }

    // ---- 操作 ----

    fn add(&mut self, index: usize, kind: &str, container: Option<&str>, values: &EditValues) -> Result<(), EditError> {
        if !ADDABLE.contains(&kind) {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                format!("DXF の編集では {kind} を足せません（足せるのは {}）", ADDABLE.join("、")),
            ));
        }
        let reader = ValueReader::new(values, index);
        let keys = |own: &[&'static str]| -> Vec<&'static str> { COMMON.iter().chain(own).copied().collect() };
        let place = self.place(index, container)?;
        let r13 = self.r13;
        let version = self.source.version.clone();
        let tags = match kind {
            "line" => {
                reader.allow(&keys(&["start", "end"]), "線分")?;
                let start = reader.required("start", reader.point("start")?)?;
                let end = reader.required("end", reader.point("end")?)?;
                let common = self.common(index, &reader, true)?;
                let (mut record, _) = self.entity_record(index, "LINE", &place, &common)?;
                self.subclass(&mut record, "AcDbLine");
                record.point(10, start, true).point(11, end, true);
                vec![record.tags]
            }
            "circle" | "arc" => {
                let own: &[&'static str] =
                    if kind == "arc" { &["center", "radius", "angle"] } else { &["center", "radius"] };
                reader.allow(&keys(own), if kind == "arc" { "円弧" } else { "円" })?;
                let center = reader.required("center", reader.point("center")?)?;
                let radius = reader.required("radius", reader.positive("radius")?)?;
                let angle =
                    if kind == "arc" { Some(reader.required("angle", reader.numbers("angle", 2)?)?) } else { None };
                let common = self.common(index, &reader, true)?;
                let (mut record, _) =
                    self.entity_record(index, if kind == "arc" { "ARC" } else { "CIRCLE" }, &place, &common)?;
                self.subclass(&mut record, "AcDbCircle");
                record.point(10, center, true).real(40, radius);
                if let Some(angle) = angle {
                    self.subclass(&mut record, "AcDbArc");
                    record.real(50, angle[0]).real(51, angle[1]);
                }
                vec![record.tags]
            }
            "ellipse" | "ellipse-arc" => {
                if !r13 {
                    return Err(EditError::at(index, "unsupported-kind", "R12 の DXF には楕円（ELLIPSE）がありません"));
                }
                reader.allow(&keys(&["center", "majorAxis", "ratio", "parameter"]), "楕円")?;
                let center = reader.required("center", reader.point("center")?)?;
                let axis = reader.required("majorAxis", reader.point("majorAxis")?)?;
                let ratio = reader.required("ratio", reader.positive("ratio")?)?;
                if ratio > 1.0 || axis[0] == 0.0 && axis[1] == 0.0 && axis[2] == 0.0 {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        "ratio は 0 より大きく 1 以下、majorAxis は 0 でない長さにしてください",
                    ));
                }
                let parameter = match reader.numbers("parameter", 2)? {
                    Some(parameter) => parameter,
                    None if kind == "ellipse" => vec![0.0, std::f64::consts::TAU],
                    None => return Err(EditError::at(index, "invalid-value", "parameter がありません")),
                };
                let common = self.common(index, &reader, true)?;
                let (mut record, _) = self.entity_record(index, "ELLIPSE", &place, &common)?;
                self.subclass(&mut record, "AcDbEllipse");
                record.point(10, center, true).point(11, axis, true);
                record.real(210, 0.0).real(220, 0.0).real(230, 1.0);
                record.real(40, ratio).real(41, parameter[0]).real(42, parameter[1]);
                vec![record.tags]
            }
            "point" => {
                reader.allow(&keys(&["position"]), "点")?;
                let position = reader.required("position", reader.point("position")?)?;
                let common = self.common(index, &reader, true)?;
                let (mut record, _) = self.entity_record(index, "POINT", &place, &common)?;
                self.subclass(&mut record, "AcDbPoint");
                record.point(10, position, true);
                vec![record.tags]
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
                let closed = reader.bool("closed")?.unwrap_or(false);
                let width = reader.number("width")?;
                let common = self.common(index, &reader, true)?;
                self.polyline_records(index, &place, &common, &vertices, &bulges, closed, width, &version)?
            }
            "text" => {
                reader.allow(&keys(&["position", "text", "height", "rotation", "textStyle", "wrapWidth"]), "文字")?;
                let position = reader.required("position", reader.point("position")?)?;
                let text = reader.required("text", reader.text("text")?)?;
                let height = reader.required("height", reader.positive("height")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let style = self.text_style(index, reader.name("textStyle")?)?;
                let wrap = reader.number("wrapWidth")?;
                let common = self.common(index, &reader, true)?;
                let multiline = text.contains('\n') || wrap.is_some();
                if multiline && !r13 {
                    return Err(EditError::at(
                        index,
                        "unsupported-kind",
                        "R12 の DXF には複数行の文字（MTEXT）がありません",
                    ));
                }
                if multiline {
                    let (mut record, _) = self.entity_record(index, "MTEXT", &place, &common)?;
                    self.subclass(&mut record, "AcDbMText");
                    record
                        .point(10, position, true)
                        .real(40, height)
                        .real(41, wrap.unwrap_or(0.0))
                        .int(71, 7)
                        .int(72, 1);
                    let content = mtext_content(text);
                    let chunks = chunks(&content, 250);
                    for chunk in &chunks[..chunks.len() - 1] {
                        record.tag(3, chunk.clone());
                    }
                    record.tag(1, chunks[chunks.len() - 1].clone());
                    if let Some(style) = style {
                        record.tag(7, style);
                    }
                    record.real(50, rotation);
                    vec![record.tags]
                } else {
                    let (mut record, _) = self.entity_record(index, "TEXT", &place, &common)?;
                    self.subclass(&mut record, "AcDbText");
                    record.point(10, position, true).real(40, height).tag(1, text.to_string());
                    if rotation != 0.0 {
                        record.real(50, rotation);
                    }
                    if let Some(style) = style {
                        record.tag(7, style);
                    }
                    self.subclass(&mut record, "AcDbText");
                    vec![record.tags]
                }
            }
            _ => {
                reader.allow(&keys(&["block", "position", "rotation", "scale"]), "ブロック参照")?;
                let name = reader.required("block", reader.name("block")?)?;
                let block = self
                    .blocks
                    .get(&upper(name))
                    .filter(|block| {
                        !upper(&block.name).starts_with("*MODEL_SPACE")
                            && !upper(&block.name).starts_with("*PAPER_SPACE")
                    })
                    .map(|block| block.name.clone())
                    .ok_or_else(|| {
                        EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))
                    })?;
                let position = reader.required("position", reader.point("position")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let scale = self.scale(index, &reader)?.unwrap_or([1.0; 3]);
                let common = self.common(index, &reader, true)?;
                let (mut record, _) = self.entity_record(index, "INSERT", &place, &common)?;
                self.subclass(&mut record, "AcDbBlockReference");
                record.tag(2, block).point(10, position, true);
                if scale != [1.0; 3] {
                    record.real(41, scale[0]).real(42, scale[1]).real(43, scale[2]);
                }
                if rotation != 0.0 {
                    record.real(50, rotation);
                }
                vec![record.tags]
            }
        };
        let handle = value(&tags[0], 5).map(str::to_string).unwrap_or_default();
        for record in tags {
            self.insert(place.before, record);
        }
        self.created.push(handle);
        Ok(())
    }

    /// 尺度。数（すべての向き）、[x, y]、[x, y, z]
    fn scale(&self, index: usize, reader: &ValueReader<'_>) -> Result<Option<[f64; 3]>, EditError> {
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

    /// 折線のレコード。R14 以降は LWPOLYLINE、それより前は POLYLINE・VERTEX・SEQEND
    #[allow(clippy::too_many_arguments)]
    fn polyline_records(
        &mut self,
        index: usize,
        place: &Place,
        common: &Common,
        vertices: &[[f64; 3]],
        bulges: &[f64],
        closed: bool,
        width: Option<f64>,
        version: &str,
    ) -> Result<Vec<Vec<Tag>>, EditError> {
        let elevation = vertices[0][2];
        if vertices.iter().any(|vertex| vertex[2] != elevation) {
            return Err(EditError::at(index, "invalid-value", "polyline の頂点の z はすべて同じにしてください"));
        }
        if version >= "AC1014" {
            let (mut record, _) = self.entity_record(index, "LWPOLYLINE", place, common)?;
            self.subclass(&mut record, "AcDbPolyline");
            record.int(90, vertices.len() as i64).int(70, i64::from(closed));
            if let Some(width) = width {
                record.real(43, width);
            }
            if elevation != 0.0 {
                record.real(38, elevation);
            }
            for (vertex, bulge) in vertices.iter().zip(bulges) {
                record.point(10, *vertex, false);
                if *bulge != 0.0 {
                    record.real(42, *bulge);
                }
            }
            return Ok(vec![record.tags]);
        }
        if self.r13 {
            return Err(EditError::at(index, "unsupported-kind", "R13 の DXF の折線は足せません"));
        }
        let (mut head, _) = self.entity_record(index, "POLYLINE", place, common)?;
        head.int(66, 1).point(10, [0.0, 0.0, elevation], true).int(70, i64::from(closed));
        if let Some(width) = width {
            head.real(40, width).real(41, width);
        }
        let mut records = vec![head.tags];
        let vertex_common = Common { layer: common.layer.clone(), color: None, line_type: None, line_width: None };
        for (vertex, bulge) in vertices.iter().zip(bulges) {
            let (mut record, _) = self.entity_record(index, "VERTEX", place, &vertex_common)?;
            record.point(10, *vertex, true);
            if *bulge != 0.0 {
                record.real(42, *bulge);
            }
            records.push(record.tags);
        }
        let (end, _) = self.entity_record(index, "SEQEND", place, &vertex_common)?;
        records.push(end.tags);
        Ok(records)
    }

    /// id を図形にする。ビューポートを通した id（VP:…）はその図形、ブロック参照の中（…>…）は、参照の属性のほかは
    /// ブロックの定義を変えることになるのでエラーにする
    fn target(&self, index: usize, id: &str) -> Result<EntityInfo, EditError> {
        let id = id.trim();
        let id = id.split_once(':').map_or(id, |(_, rest)| rest);
        let segments: Vec<&str> = id.split('>').collect();
        let clean = |segment: &str| upper(segment.split('[').next().unwrap_or(segment));
        let handle = clean(segments[segments.len() - 1]);
        if handle == "?" || handle.is_empty() {
            return Err(EditError::at(
                index,
                "not-found",
                "ハンドルのない図形（ハンドルを持たない R12 の図面など）は id で選べません",
            ));
        }
        let info = self
            .entities
            .get(&handle)
            .filter(|info| !self.deleted.contains(&info.record))
            .cloned()
            .ok_or_else(|| EditError::at(index, "not-found", format!("図形 {id} は図面にありません")))?;
        if segments.len() > 1 {
            let parent = clean(segments[segments.len() - 2]);
            if info.kind != "ATTRIB" || info.parent.as_deref() != Some(parent.as_str()) {
                return Err(EditError::at(
                    index,
                    "inside-block",
                    format!(
                        "図形 {id} はブロック参照 {parent} の中にあります。参照は id {parent} で、ブロックの定義の中の図形は id {handle} で変えられます（定義を変えると、そのブロックのすべての参照が変わります）"
                    ),
                ));
            }
        }
        Ok(info)
    }

    fn update(&mut self, index: usize, id: &str, set_values: &EditValues) -> Result<(), EditError> {
        let target = self.target(index, id)?;
        let reader = ValueReader::new(set_values, index);
        let own: &[&'static str] = match target.kind.as_str() {
            "LINE" => &["start", "end"],
            "CIRCLE" => &["center", "radius"],
            "ARC" => &["center", "radius", "angle"],
            "ELLIPSE" => &["center", "majorAxis", "ratio", "parameter"],
            "POINT" => &["position"],
            "LWPOLYLINE" => &["vertices", "closed", "bulges", "width"],
            "TEXT" | "ATTRIB" | "ATTDEF" => &["position", "text", "height", "rotation", "textStyle"],
            "MTEXT" => &["position", "text", "height", "rotation", "textStyle", "wrapWidth"],
            "INSERT" => &["block", "position", "rotation", "scale"],
            _ => &[],
        };
        let keys: Vec<&str> = COMMON.iter().chain(own).copied().collect();
        reader.allow(&keys, &format!("{} の変更", target.kind))?;
        let common = self.common(index, &reader, false)?;
        let style = self.text_style(index, reader.name("textStyle")?)?;
        let block = match reader.name("block")? {
            Some(name) => Some(self.blocks.get(&upper(name)).map(|block| block.name.clone()).ok_or_else(|| {
                EditError::at(index, "missing-definition", format!("ブロック {name} は図面にありません"))
            })?),
            None => None,
        };
        let scale = self.scale(index, &reader)?;
        let color = match common.color {
            Some(color) => Some(self.color_tags(index, color)?),
            None => None,
        };
        let weight = common.line_width.map(|width| self.lineweight(width));
        if weight.is_some() && self.source.version.as_str() < "AC1015" {
            self.notices.add(
                NoticeLevel::Info,
                "edit-no-lineweight",
                "AutoCAD 2000 より前の DXF には線の太さがないため、書きませんでした",
            );
        }
        let lineweights = self.source.version.as_str() >= "AC1015";
        let kind = target.kind.clone();
        // 共通の値（AcDbEntity。DXF Reference「Common Group Codes for Entities」の並び）
        let records: Vec<usize> =
            if kind == "POLYLINE" { (target.record..target.end).collect() } else { vec![target.record] };
        for record in records {
            let tags = self.tags_mut(record);
            if !common.layer.is_empty() {
                set(tags, 8, common.layer.clone(), &[100, 330, 5]);
            }
            if record != target.record {
                continue;
            }
            if let Some(line_type) = &common.line_type {
                if line_type == "ByLayer" {
                    remove(tags, 6);
                } else {
                    set(tags, 6, line_type.clone(), &[8]);
                }
            }
            if let Some((aci, rgb)) = color {
                remove(tags, 430);
                match aci {
                    Some(aci) => set(tags, 62, aci.to_string(), &[8, 6]),
                    None => remove(tags, 62),
                }
                match rgb {
                    Some(rgb) => set(tags, 420, rgb.to_string(), &[62, 8, 6]),
                    None => remove(tags, 420),
                }
            }
            if let Some(weight) = weight
                && lineweights
            {
                set(tags, 370, weight.to_string(), &[420, 62, 6, 8]);
            }
        }
        let record = target.record;
        let invalid = |message: &str| EditError::at(index, "invalid-value", message.to_string());
        match kind.as_str() {
            "LINE" => {
                let start = reader.point("start")?;
                let end = reader.point("end")?;
                let tags = self.tags_mut(record);
                if let Some(start) = start {
                    set_point(tags, 10, start, &[100]);
                }
                if let Some(end) = end {
                    set_point(tags, 11, end, &[30, 20]);
                }
            }
            "CIRCLE" | "ARC" | "ELLIPSE" | "POINT" => {
                let center = reader.point(if kind == "POINT" { "position" } else { "center" })?;
                let radius = reader.positive("radius")?;
                let angle = reader.numbers("angle", 2)?;
                let axis = reader.point("majorAxis")?;
                let ratio = reader.positive("ratio")?;
                if ratio.is_some_and(|ratio| ratio > 1.0) {
                    return Err(invalid("ratio は 0 より大きく 1 以下にしてください"));
                }
                let parameter = reader.numbers("parameter", 2)?;
                let tags = self.tags_mut(record);
                if let Some(center) = center {
                    set_point(tags, 10, center, &[100]);
                }
                if let Some(radius) = radius {
                    set(tags, 40, real(radius), &[30, 20]);
                }
                if let Some(angle) = angle {
                    set(tags, 50, real(angle[0]), &[100, 40]);
                    set(tags, 51, real(angle[1]), &[50]);
                }
                if let Some(axis) = axis {
                    set_point(tags, 11, axis, &[30, 20]);
                }
                if let Some(ratio) = ratio {
                    set(tags, 40, real(ratio), &[230, 31, 21]);
                }
                if let Some(parameter) = parameter {
                    set(tags, 41, real(parameter[0]), &[40]);
                    set(tags, 42, real(parameter[1]), &[41]);
                }
            }
            "LWPOLYLINE" => self.update_lwpolyline(index, record, &reader)?,
            "TEXT" | "ATTRIB" | "ATTDEF" | "MTEXT" => {
                let position = reader.point("position")?;
                let text = reader.text("text")?;
                let height = reader.positive("height")?;
                let rotation = reader.number("rotation")?;
                let wrap = reader.number("wrapWidth")?;
                let mtext = kind == "MTEXT";
                if !mtext && text.is_some_and(|text| text.contains('\n')) {
                    return Err(invalid("1 行の文字（TEXT・ATTRIB）に改行は書けません"));
                }
                if kind == "ATTRIB" && find(self.tags(record), 101).is_some() && text.is_some() {
                    return Err(EditError::at(index, "unsupported-kind", "複数行の属性の文字は変えられません"));
                }
                let tags = self.tags_mut(record);
                if let Some(position) = position {
                    if mtext {
                        set_point(tags, 10, position, &[100]);
                    } else {
                        move_text(tags, position, if kind == "TEXT" { 73 } else { 74 });
                    }
                }
                if let Some(text) = text {
                    if mtext {
                        remove(tags, 3);
                        let content = mtext_content(text);
                        let chunks = chunks(&content, 250);
                        let at = find(tags, 1).unwrap_or_else(|| data_end(tags));
                        if find(tags, 1).is_some() {
                            tags.remove(at);
                        }
                        for (offset, chunk) in chunks.iter().enumerate() {
                            let code = if offset + 1 == chunks.len() { 1 } else { 3 };
                            tags.insert(at + offset, Tag::new(code, chunk.clone()));
                        }
                    } else {
                        set(tags, 1, text.to_string(), &[40]);
                    }
                }
                if let Some(height) = height {
                    set(tags, 40, real(height), &[30, 20]);
                }
                if let Some(rotation) = rotation {
                    if mtext {
                        // 11 の向きがあれば 50 より先に使われるので除く
                        for code in [11, 21, 31] {
                            remove(tags, code);
                        }
                        set(tags, 50, real(rotation), &[7, 1]);
                    } else {
                        set(tags, 50, real(rotation), &[1]);
                    }
                }
                if let Some(style) = style {
                    set(tags, 7, style, &[50, 1]);
                }
                if let Some(wrap) = wrap {
                    set(tags, 41, real(wrap), &[40]);
                }
            }
            "INSERT" => {
                let position = reader.point("position")?;
                let rotation = reader.number("rotation")?;
                let tags = self.tags_mut(record);
                if let Some(block) = block {
                    set(tags, 2, block, &[100, 66]);
                }
                if let Some(position) = position {
                    set_point(tags, 10, position, &[2]);
                }
                if let Some(scale) = scale {
                    set(tags, 41, real(scale[0]), &[30, 20]);
                    set(tags, 42, real(scale[1]), &[41]);
                    set(tags, 43, real(scale[2]), &[42]);
                }
                if let Some(rotation) = rotation {
                    set(tags, 50, real(rotation), &[43, 42, 41, 30, 20]);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn update_lwpolyline(&mut self, index: usize, record: usize, reader: &ValueReader<'_>) -> Result<(), EditError> {
        let vertices = reader.points("vertices", 2)?;
        let closed = reader.bool("closed")?;
        let width = reader.number("width")?;
        let tags = self.tags_mut(record);
        let start = tags.iter().position(|tag| tag.code == 100 && tag.text() == "AcDbPolyline").unwrap_or(0);
        let end = data_end(tags);
        // 頂点のタグ（10・20・40・41・42・91）の並び
        let vertex_codes = [10, 20, 40, 41, 42, 91];
        let first = (start..end).find(|&at| tags[at].code == 10);
        let current: Vec<f64> = (start..end).filter(|&at| tags[at].code == 42).map(|at| tags[at].number()).collect();
        let count = (start..end).filter(|&at| tags[at].code == 10).count();
        let bulges = match reader.numbers("bulges", vertices.as_ref().map_or(count, Vec::len))? {
            Some(bulges) => Some(bulges),
            None if reader.has("bulges") => {
                return Err(EditError::at(index, "invalid-value", "bulges は頂点と同じ数にしてください"));
            }
            None => None,
        };
        if vertices.is_some() || bulges.is_some() {
            let points: Vec<[f64; 2]> = match &vertices {
                Some(vertices) => vertices.iter().map(|vertex| [vertex[0], vertex[1]]).collect(),
                None => {
                    let xs: Vec<f64> =
                        (start..end).filter(|&at| tags[at].code == 10).map(|at| tags[at].number()).collect();
                    let ys: Vec<f64> =
                        (start..end).filter(|&at| tags[at].code == 20).map(|at| tags[at].number()).collect();
                    xs.into_iter().zip(ys).map(|(x, y)| [x, y]).collect()
                }
            };
            let bulges = bulges.unwrap_or_else(|| if vertices.is_some() { vec![0.0; points.len()] } else { current });
            let at = first.unwrap_or(end);
            let mut kept: Vec<Tag> = Vec::with_capacity(tags.len());
            for (position, tag) in tags.iter().enumerate() {
                if position >= start && position < end && vertex_codes.contains(&tag.code) {
                    continue;
                }
                kept.push(tag.clone());
            }
            let removed_before = (start..at).filter(|&position| vertex_codes.contains(&tags[position].code)).count();
            let mut insert_at = at - removed_before;
            *tags = kept;
            for (point, bulge) in points.iter().zip(&bulges) {
                tags.insert(insert_at, Tag::new(10, real(point[0])));
                tags.insert(insert_at + 1, Tag::new(20, real(point[1])));
                insert_at += 2;
                if *bulge != 0.0 {
                    tags.insert(insert_at, Tag::new(42, real(*bulge)));
                    insert_at += 1;
                }
            }
            set(tags, 90, points.len().to_string(), &[100]);
        }
        if let Some(closed) = closed {
            let flags = number(tags, 70).unwrap_or(0.0) as i64;
            set(tags, 70, ((flags & !1) | i64::from(closed)).to_string(), &[90]);
        }
        if let Some(width) = width {
            set(tags, 43, real(width), &[70, 90]);
        }
        Ok(())
    }

    fn delete(&mut self, index: usize, id: &str) -> Result<(), EditError> {
        let target = self.target(index, id)?;
        if target.kind == "ATTRIB" {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                "属性だけは消せません。ブロック参照ごと消してください",
            ));
        }
        let mut handles: Vec<String> = Vec::new();
        for record in target.record..target.end {
            self.deleted.insert(record);
            if let Some(handle) = self.record_handles.get(&record) {
                handles.push(handle.clone());
            }
        }
        // 図形が持つオブジェクト（拡張辞書など）も消す
        let mut queue = handles.clone();
        while let Some(handle) = queue.pop() {
            for record in self.owned.get(&handle).cloned().unwrap_or_default() {
                if self.deleted.insert(record)
                    && let Some(owned) = self.record_handles.get(&record)
                {
                    handles.push(owned.clone());
                    queue.push(owned.clone());
                }
            }
        }
        let gone: HashSet<String> = handles.into_iter().collect();
        for record in self.references.clone() {
            if self.deleted.contains(&record) {
                continue;
            }
            let kind = self.kind(record).to_string();
            let tags = self.tags(record);
            let mut keep = Vec::with_capacity(tags.len());
            let mut changed = false;
            let mut group = false;
            let mut skip_sort_handle = false;
            for tag in tags {
                if tag.code == 102 {
                    group = tag.text().starts_with('{');
                }
                let refers = gone.contains(&upper(tag.text()));
                let drop = match tag.code {
                    330 if group => refers,
                    331 if kind == "SORTENTSTABLE" => {
                        skip_sort_handle = refers;
                        refers
                    }
                    5 if kind == "SORTENTSTABLE" && skip_sort_handle => {
                        skip_sort_handle = false;
                        true
                    }
                    340 if kind == "GROUP" => refers,
                    _ => false,
                };
                changed |= drop;
                if !drop {
                    keep.push(tag.clone());
                }
            }
            if changed {
                *self.tags_mut(record) = keep;
            }
        }
        Ok(())
    }

    /// レイヤのレコードを足す位置と、表のハンドル。表がなければ作る（TABLES の節もなければ、BLOCKS・ENTITIES の前に作る。
    /// DXF Reference「TABLES Section」の並び）
    fn layer_table(&mut self, index: usize) -> Result<(usize, Option<String>), EditError> {
        if let Some(&(table, Some(end))) = self.tables.get("LAYER") {
            return Ok((end, self.record_handles.get(&table).cloned()));
        }
        if let Some((at, _, handle)) = &self.created_table {
            return Ok((*at, handle.clone()));
        }
        let (at, section) = match self.sections.get("TABLES") {
            Some(&(_, Some(end))) => (end, false),
            _ => {
                let start = ["BLOCKS", "ENTITIES", "OBJECTS"]
                    .iter()
                    .filter_map(|name| self.sections.get(*name).map(|entry| entry.0))
                    .min()
                    .ok_or_else(|| EditError::at(index, "unreadable", "図面に節（SECTION）がありません"))?;
                (start, true)
            }
        };
        let handle = self.next_handle();
        let mut opener = Record::new("TABLE");
        opener.tag(2, "LAYER");
        if let Some(handle) = &handle {
            opener.tag(5, handle.clone());
        }
        if self.r13 {
            opener.tag(330, "0").tag(100, "AcDbSymbolTable");
        }
        opener.int(70, 0);
        let mut closers = vec![Record::new("ENDTAB").tags];
        if section {
            let mut start = Record::new("SECTION");
            start.tag(2, "TABLES");
            self.insert(at, start.tags);
            closers.push(Record::new("ENDSEC").tags);
        }
        let position = self.insert(at, opener.tags);
        self.closers.insert(at, closers);
        self.created_table = Some((at, position, handle.clone()));
        Ok((at, handle))
    }

    fn add_layer(&mut self, index: usize, name: &str, values: &EditValues, implicit: bool) -> Result<(), EditError> {
        if !valid_name(name) {
            return Err(EditError::at(index, "invalid-value", format!("レイヤの名前 {name} は使えません")));
        }
        if self.layers.contains_key(&upper(name)) {
            return Err(EditError::at(index, "already-exists", format!("レイヤ {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&LAYER_KEYS, "レイヤ")?;
        let (end, owner) = self.layer_table(index)?;
        // レイヤ 0 のレコードを写して、名前・ハンドル・値を変える（線種・印刷スタイルなどの参照を保つ）
        let template: Vec<Tag> = match self.layers.get("0") {
            Some(LayerRef::Record(record)) => self.tags(*record).to_vec(),
            _ => Vec::new(),
        };
        let mut tags: Vec<Tag> = Vec::new();
        let mut group = false;
        for tag in template.iter().take(data_end(&template)) {
            if tag.code == 102 {
                group = tag.text().starts_with('{');
                continue;
            }
            if group || matches!(tag.code, 5 | 2 | 70 | 62 | 420 | 430 | 6 | 370 | 290 | 360) {
                continue;
            }
            tags.push(tag.clone());
        }
        if tags.is_empty() {
            tags.push(Tag::new(0, "LAYER"));
            if self.r13 {
                if let Some(owner) = &owner {
                    tags.push(Tag::new(330, owner.clone()));
                }
                tags.push(Tag::new(100, "AcDbSymbolTableRecord"));
                tags.push(Tag::new(100, "AcDbLayerTableRecord"));
            }
        }
        let name_at = tags
            .iter()
            .position(|tag| tag.code == 100 && tag.text() == "AcDbLayerTableRecord")
            .map_or(tags.len(), |at| at + 1);
        let mut own = vec![
            Tag::new(2, name),
            Tag::new(70, "0"),
            Tag::new(62, "7"),
            Tag::new(6, self.linetypes.get("CONTINUOUS").cloned().unwrap_or_else(|| "CONTINUOUS".into())),
        ];
        if self.source.version.as_str() >= "AC1015" {
            own.push(Tag::new(370, "-3"));
        }
        for (offset, tag) in own.into_iter().enumerate() {
            tags.insert(name_at + offset, tag);
        }
        if let Some(handle) = self.next_handle() {
            tags.insert(1, Tag::new(5, handle));
        }
        self.apply_layer_values(index, &mut tags, &reader)?;
        let position = self.insert(end, tags);
        self.layers.insert(upper(name), LayerRef::Added(end, position));
        // 表の項目の数（70）を数え直す
        let count = self.layers.len().to_string();
        let table_tags = match (self.tables.get("LAYER").copied(), self.created_table.clone()) {
            (Some((table, _)), _) => Some(self.tags_mut(table)),
            (None, Some((at, position, _))) => self.inserted.get_mut(&at).and_then(|list| list.get_mut(position)),
            _ => None,
        };
        if let Some(table_tags) = table_tags
            && number(table_tags, 70).is_none_or(|current| current < count.parse::<f64>().unwrap_or(0.0))
        {
            set(table_tags, 70, count, &[100, 2]);
        }
        if !implicit {
            self.created.push(name.to_string());
        }
        Ok(())
    }

    fn update_layer(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        let reader = ValueReader::new(values, index);
        reader.allow(&LAYER_KEYS, "レイヤの変更")?;
        let layer = self
            .layers
            .get(&upper(name))
            .cloned()
            .ok_or_else(|| EditError::at(index, "not-found", format!("レイヤ {name} は図面にありません")))?;
        let mut tags = match &layer {
            LayerRef::Record(record) => self.tags(*record).to_vec(),
            LayerRef::Added(before, position) => self.inserted[before][*position].clone(),
        };
        self.apply_layer_values(index, &mut tags, &reader)?;
        match layer {
            LayerRef::Record(record) => *self.tags_mut(record) = tags,
            LayerRef::Added(before, position) => {
                if let Some(list) = self.inserted.get_mut(&before) {
                    list[position] = tags;
                }
            }
        }
        Ok(())
    }

    /// レイヤの値（DXF Reference「LAYER」: 70 の 1 は凍結、4 はロック、62 が負なら非表示、290 は印刷）
    fn apply_layer_values(
        &mut self,
        index: usize,
        tags: &mut Vec<Tag>,
        reader: &ValueReader<'_>,
    ) -> Result<(), EditError> {
        let color = match reader.color("color")? {
            Some(EditColor::ByLayer | EditColor::ByBlock) => {
                return Err(EditError::at(index, "invalid-value", "レイヤの色に bylayer・byblock は使えません"));
            }
            Some(color) => Some(self.color_tags(index, color)?),
            None => None,
        };
        let line_type = match reader.name("lineType")? {
            Some(name) if matches!(upper(name).as_str(), "BYLAYER" | "BYBLOCK") => {
                return Err(EditError::at(index, "invalid-value", "レイヤの線種に ByLayer・ByBlock は使えません"));
            }
            Some(name) => Some(self.line_type(index, name)?),
            None => None,
        };
        let weight = match reader.line_width("lineWidth")? {
            Some(EditLineWidth::ByLayer | EditLineWidth::ByBlock) => {
                return Err(EditError::at(index, "invalid-value", "レイヤの線の太さに bylayer・byblock は使えません"));
            }
            Some(width) => Some(self.lineweight(width)),
            None => None,
        };
        let visible = reader.bool("visible")?;
        let frozen = reader.bool("frozen")?;
        let locked = reader.bool("locked")?;
        let plot = reader.bool("plot")?;
        let current = number(tags, 62).unwrap_or(7.0);
        let off = visible.map_or(current < 0.0, |visible| !visible);
        let aci = match color {
            Some((aci, rgb)) => {
                remove(tags, 430);
                match rgb {
                    Some(rgb) => set(tags, 420, rgb.to_string(), &[62]),
                    None => remove(tags, 420),
                }
                aci.unwrap_or(7)
            }
            None => current.abs() as i64,
        };
        set(tags, 62, (if off { -aci.max(1) } else { aci.max(1) }).to_string(), &[70, 2]);
        let mut flags = number(tags, 70).unwrap_or(0.0) as i64;
        if let Some(frozen) = frozen {
            flags = (flags & !1) | i64::from(frozen);
        }
        if let Some(locked) = locked {
            flags = (flags & !4) | (i64::from(locked) << 2);
        }
        set(tags, 70, flags.to_string(), &[2]);
        if let Some(line_type) = line_type {
            set(tags, 6, line_type, &[62, 420]);
        }
        if let Some(weight) = weight
            && self.source.version.as_str() >= "AC1015"
        {
            set(tags, 370, weight.to_string(), &[6, 290]);
        }
        if let Some(plot) = plot
            && self.source.version.as_str() >= "AC1015"
        {
            set(tags, 290, i64::from(plot).to_string(), &[6]);
        }
        Ok(())
    }
}

/// 1 行の文字の位置を動かす。位置合わせが左・ベースライン以外なら、配置に使われる 11 も同じだけ動かす
/// （DXF Reference「TEXT」: 72・73 が 0 でなければ 11 が位置を決める。ATTRIB・ATTDEF の縦の位置合わせは 74）
fn move_text(tags: &mut Vec<Tag>, position: [f64; 3], vertical: i32) {
    let aligned = number(tags, 72).unwrap_or(0.0) != 0.0 || number(tags, vertical).unwrap_or(0.0) != 0.0;
    let first = get_point(tags, 10).unwrap_or([0.0; 3]);
    let second = get_point(tags, 11);
    let anchor = match second {
        Some(second) if aligned => second,
        _ => first,
    };
    let delta = [position[0] - anchor[0], position[1] - anchor[1], position[2] - anchor[2]];
    let shift = |point: [f64; 3]| [point[0] + delta[0], point[1] + delta[1], point[2] + delta[2]];
    set_point(tags, 10, shift(first), &[100]);
    if let Some(second) = second {
        set_point(tags, 11, shift(second), &[72, 51, 41, 7, 50, 1]);
    }
}

/// MTEXT の文字列にする（\ { } を書式コードから区別し、改行は \P）
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

/// 文字数で分ける（MTEXT の 3 と 1 は 250 文字ずつ。DXF Reference「MTEXT」）。\ の書式コードの途中では分けない
fn chunks(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    while chars.len() - start > size {
        let mut end = start + size;
        // 分ける位置の直前の \ の書式コード（\P、\U+XXXX など）を次の塊に回す
        if let Some(back) = chars[end.saturating_sub(7)..end].iter().rposition(|c| *c == '\\') {
            let at = end.saturating_sub(7) + back;
            if at > start {
                end = at;
            }
        }
        out.push(chars[start..end].iter().collect());
        start = end;
    }
    out.push(chars[start..].iter().collect());
    out
}

#[cfg(test)]
mod tests;
