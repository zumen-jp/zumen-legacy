//! AutoCAD の図面のモデル（`DxfDocument`）を、AutoCAD 2013 の DWG（AC1027）に書く。DXF・DWG から読んだ図面を
//! DWG にする変換に使う。決め方は DXF の書き出し（zumen-dxf の write.rs）と同じ:
//!
//! - ハンドルは元の図面のものを保ち、ないもの・重なるものには新しいハンドルを振る
//! - 図形が参照するのに表にないレイヤ・線種・文字スタイル・寸法スタイル・ブロックは補う
//! - 書けなかった・形を変えて書いた内容は、読み取りと同じ notices（種類ごとの件数）で返す
//!
//! DWG はすべての値を持つので、モデルにない値は AutoCAD の既定の値（acad.dwt）で書く。オブジェクトは可逆の
//! 読み書き（codec）の構造体に組み立てて書く（実物の図面を読んで書くと同じビット列になることを確かめた並び）

mod entities;
mod header;
mod objects;
mod tables;

use super::DwgWritten;
use super::classes::ClassDef;
use super::container::{Objects, R2004, write_r2004};
use super::object::ObjectWriter;
use crate::bits::HandleRef;
use crate::codec::RawText;
use crate::codec::object::ObjectCommon;
use crate::file::Version;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use zumen_core::{NoticeLevel, NoticeList};
use zumen_dxf::{DxfBlock, DxfDocument, DxfEntity, DxfEntityData, DxfLayout, DxfMLineStyle, DxfSpace, Vec3};

/// 書く版
const VERSION: Version = Version::R2013;
/// 保守版（ファイルの頭の 0x0B と 0x12、AcDb:AuxHeader）。AutoCAD 2013 が書く値
const MAINTENANCE: u8 = 8;

/// モデルを AutoCAD 2013 の DWG にする
pub fn write_dwg(doc: &DxfDocument) -> DwgWritten {
    let mut writer = Writer::new(doc);
    writer.plan();
    writer.write_entities();
    writer.write_tables();
    writer.write_objects();
    writer.finish()
}

/// 保つハンドルの上限。これより大きいハンドルは振り直す（次のハンドルを数えてもあふれない）
const MAX_HANDLE: u64 = (1 << 56) - 1;

/// ハンドル。元の図面のハンドルを保ち（重なれば新しく振る）、新しいハンドルは元の最大より後から振る
pub(super) struct Handles {
    next: u64,
    used: HashSet<u64>,
}

impl Handles {
    fn new(doc: &DxfDocument) -> Self {
        let mut max = 0;
        let mut note = |text: &str| {
            if let Ok(value) = u64::from_str_radix(text, 16)
                && value <= MAX_HANDLE
            {
                max = max.max(value);
            }
        };
        for layer in &doc.layers {
            note(&layer.handle);
        }
        for linetype in &doc.linetypes {
            note(&linetype.handle);
        }
        for style in &doc.styles {
            note(&style.handle);
        }
        for style in &doc.dimstyles {
            note(&style.handle);
        }
        for style in doc.mline_styles.iter().flatten() {
            note(&style.handle);
        }
        for block in &doc.blocks {
            note(&block.handle);
            note(block.block_record_handle.as_deref().unwrap_or(""));
        }
        for layout in &doc.layouts {
            note(&layout.handle);
            note(layout.block_record_handle.as_deref().unwrap_or(""));
        }
        for entity in all_entities(doc) {
            note(&entity.base.handle);
        }
        Self { next: max + 1, used: HashSet::new() }
    }

    pub(super) fn fresh(&mut self) -> u64 {
        while self.used.contains(&self.next) {
            self.next += 1;
        }
        let handle = self.next;
        self.used.insert(handle);
        self.next += 1;
        handle
    }

    /// 元のハンドル。読めないか、0 か、大きすぎるか、すでに使ったものなら新しく振る
    fn keep(&mut self, original: &str) -> u64 {
        match u64::from_str_radix(original, 16) {
            Ok(value) if value > 0 && value <= MAX_HANDLE && self.used.insert(value) => value,
            _ => self.fresh(),
        }
    }
}

/// 書くブロック（BLOCK_HEADER と BLOCK・ENDBLK）
pub(super) struct BlockPlan {
    pub name: String,
    pub record: u64,
    pub begin: u64,
    pub end: u64,
    pub layout: u64,
    pub source: Option<usize>,
    /// 紙空間のブロック（*Paper_Space…）
    pub paper: bool,
    /// 持つ図形（書いた順）
    pub entities: Vec<u64>,
    /// このブロックを参照する INSERT
    pub inserts: Vec<u64>,
    /// 属性定義（ATTDEF）を持つ
    pub has_attributes: bool,
}

/// 書くレイアウト
pub(super) struct LayoutPlan<'a> {
    pub name: String,
    pub handle: u64,
    pub block: String,
    pub source: Option<&'a DxfLayout>,
}

/// 決まった表・辞書・オブジェクトのハンドル
#[derive(Default)]
pub(super) struct Fixed {
    pub block_control: u64,
    pub layer_control: u64,
    pub style_control: u64,
    pub ltype_control: u64,
    pub view_control: u64,
    pub ucs_control: u64,
    pub vport_control: u64,
    pub appid_control: u64,
    pub dimstyle_control: u64,
    pub vport: u64,
    pub root: u64,
    pub color_dict: u64,
    pub group_dict: u64,
    pub layout_dict: u64,
    pub material_dict: u64,
    pub mleader_dict: u64,
    pub mline_dict: u64,
    pub plot_dict: u64,
    pub plotstyle_dict: u64,
    pub scale_dict: u64,
    pub tablestyle_dict: u64,
    pub visualstyle_dict: u64,
    pub variable_dict: u64,
    pub placeholder: u64,
    pub material_byblock: u64,
    pub material_bylayer: u64,
    pub material_global: u64,
    pub mleader_style: u64,
    pub scale: u64,
    /// 辞書の変数（名前, ハンドル, 値）
    pub variables: Vec<(&'static str, u64, &'static str)>,
    /// APPID（名前, ハンドル）
    pub appids: Vec<(&'static str, u64)>,
}

/// ブロックの描画順（SORTENTSTABLE）: (ブロックの添字, 拡張辞書, 表, (並べ替えのハンドル, 図形))
pub(super) type SortTable = (usize, u64, u64, Vec<(u64, u64)>);

/// 線種を決める途中の値（名前, 説明, 1 周期の長さ, 線と空白, 元の線種）
type LinetypeSource<'a> = (String, String, f64, Vec<f64>, Option<&'a zumen_dxf::DxfLinetype>);

/// 書く線種（LTYPE）
pub(super) struct LinetypePlan {
    pub name: String,
    pub handle: u64,
    pub description: String,
    pub length: f64,
    pub dashes: Vec<f64>,
}

/// 書くレイヤ
pub(super) struct LayerPlan {
    pub name: String,
    pub handle: u64,
    pub color: f64,
    pub true_color: Option<f64>,
    pub frozen: bool,
    pub off: bool,
    pub locked: bool,
    pub plot: bool,
    pub linetype: String,
    pub lineweight: f64,
    pub transparency: Option<f64>,
}

pub(super) struct Writer<'a> {
    pub doc: &'a DxfDocument,
    pub handles: Handles,
    pub notices: NoticeList,
    /// 書いたオブジェクト（ハンドル, MS の大きさから CRC まで）
    pub objects: Vec<(u64, Vec<u8>)>,
    /// 使ったクラス（DXF の名前, 数）。番号は並ぶ順に 500 から
    pub classes: Vec<(&'static str, i32)>,
    pub blocks: Vec<BlockPlan>,
    pub block_index: HashMap<String, usize>,
    pub layouts: Vec<LayoutPlan<'a>>,
    pub linetypes: Vec<LinetypePlan>,
    pub layers: Vec<LayerPlan>,
    /// 名前（大文字）→ ハンドル
    pub layer_handles: HashMap<String, u64>,
    pub linetype_handles: HashMap<String, u64>,
    pub style_handles: HashMap<String, u64>,
    pub dimstyle_handles: HashMap<String, u64>,
    /// 文字スタイル（名前, 元のスタイル）と寸法スタイル
    pub styles: Vec<(String, u64, Option<&'a zumen_dxf::DxfTextStyle>)>,
    pub dimstyles: Vec<(String, u64, Option<&'a zumen_dxf::DxfDimStyle>)>,
    /// 元のハンドル（大文字）→ 書いたハンドル。参照（文字スタイル・ブロック・属性定義など）を付け替える
    pub remap: HashMap<String, u64>,
    /// 図形（`DxfEntityBase` の場所）→ 書くハンドル
    pub entity_handles: HashMap<usize, u64>,
    pub mline_styles: Vec<(u64, DxfMLineStyle)>,
    pub fixed: Fixed,
    pub sort_tables: Vec<SortTable>,
    /// 書いた VIEWPORT（レイアウトのビューポートの並び）
    pub viewports: HashSet<u64>,
    /// TV の長さ（BS）に入らないので切った文字列の数
    pub long_texts: Cell<u64>,
}

/// 図形（属性を含む）
pub(super) fn all_entities(doc: &DxfDocument) -> impl Iterator<Item = &DxfEntity> {
    doc.entities.iter().flat_map(|entity| {
        let attribs: &[DxfEntity] = match &entity.data {
            DxfEntityData::Insert(insert) => &insert.attribs,
            _ => &[],
        };
        std::iter::once(entity).chain(attribs.iter())
    })
}

/// 図形のレイヤを、表示が初めて読む順に並べる（DXF の書き出しと同じ。定義のないレイヤを、表示と同じ番号になる順に
/// 補うために使う）。深い入れ子でもスタックを使い切らないよう、再帰しない
fn layer_order(doc: &DxfDocument) -> Vec<&str> {
    let mut blocks: HashMap<String, Vec<&DxfEntity>> = HashMap::new();
    let mut model = Vec::new();
    for entity in &doc.entities {
        match entity.base.space {
            DxfSpace::Model => model.push(entity),
            _ => blocks.entry(upper(entity.base.block.as_deref().unwrap_or(PAPER))).or_default().push(entity),
        }
    }
    let mut names = Vec::new();
    let mut visited = HashSet::new();
    let mut stack: Vec<(Vec<&DxfEntity>, usize, bool)> = vec![(model, 0, false)];
    while let Some((entities, index, inside)) = stack.last_mut() {
        let Some(&entity) = entities.get(*index) else {
            stack.pop();
            continue;
        };
        *index += 1;
        let inside = *inside;
        if !(inside && entity.base.layer == "0") {
            names.push(entity.base.layer.as_str());
        }
        let block = match &entity.data {
            DxfEntityData::Insert(insert) => upper(&insert.name),
            DxfEntityData::Dimension(dimension) => upper(&dimension.anonymous_block_name),
            DxfEntityData::Table(table) => upper(&table.name),
            _ => continue,
        };
        if let DxfEntityData::Insert(insert) = &entity.data {
            stack.push((insert.attribs.iter().collect(), 0, inside));
        }
        if let Some(children) = blocks.get(&block)
            && visited.insert(block)
        {
            stack.push((children.clone(), 0, true));
        }
    }
    names.extend(all_entities(doc).map(|entity| entity.base.layer.as_str()));
    names
}

pub(super) const MODEL: &str = "*MODEL_SPACE";
pub(super) const PAPER: &str = "*PAPER_SPACE";

pub(super) fn upper(name: &str) -> String {
    name.to_uppercase()
}

/// 読み取りのラジアンから度（entities.rs の deg と同じ計算）
fn degrees_of(radians: f64) -> f64 {
    radians * 180.0 / std::f64::consts::PI
}

/// radians の前後 count 個ずつのラジアン（近い順）
fn neighbors(radians: f64, count: usize) -> Vec<f64> {
    let mut out = vec![radians];
    let (mut down, mut up) = (radians, radians);
    for _ in 0..count {
        down = down.next_down();
        up = up.next_up();
        out.push(down);
        out.push(up);
    }
    out
}

/// 度をラジアンに。読み取り（r × 180 / π）で同じ度に戻るラジアンを近くのラジアンから選び、なければいちばん近く戻る
/// もの（二重の丸めのため、戻らない度がある）
pub(crate) fn rad(degrees: f64) -> f64 {
    let first = degrees * std::f64::consts::PI / 180.0;
    if !first.is_finite() || degrees_of(first) == degrees {
        return first;
    }
    let error = |radians: f64| (degrees_of(radians) - degrees).abs();
    neighbors(first, 8).into_iter().min_by(|a, b| error(*a).total_cmp(&error(*b))).unwrap_or(first)
}

/// 始めと終わりの角度（度）をラジアンに。表示の分割の数は掃引角（終わり − 始め）で決まるので、読み取りで戻した度の差が
/// 元の差と同じになる組を選ぶ（ちょうど 180° の円弧で 1 ulp ずれると分割の数が変わる）
pub(crate) fn rad_pair(start: f64, end: f64) -> (f64, f64) {
    let (first_start, first_end) = (rad(start), rad(end));
    let sweep = end - start;
    if !sweep.is_finite() || degrees_of(first_end) - degrees_of(first_start) == sweep {
        return (first_start, first_end);
    }
    let error = |radians: f64, degrees: f64| (degrees_of(radians) - degrees).abs();
    let mut best: Option<(f64, f64, f64)> = None;
    for s in neighbors(first_start, 8) {
        for e in neighbors(first_end, 8) {
            if degrees_of(e) - degrees_of(s) != sweep {
                continue;
            }
            let score = error(s, start) + error(e, end);
            if best.is_none_or(|(_, _, known)| score < known) {
                best = Some((s, e, score));
            }
        }
    }
    best.map_or((first_start, first_end), |(s, e, _)| (s, e))
}

/// ハンドルの参照
pub(super) fn href(code: u8, value: u64) -> HandleRef {
    HandleRef { code, value }
}

/// 持ち主（ソフトポインタ）とリアクタ（持ち主と同じ）を持つ、拡張辞書のないオブジェクトの共通の値
/// （辞書の項目は AutoCAD と同じく持ち主の辞書をリアクタにも持つ）
pub(super) fn owned(owner: u64, reactor: bool) -> ObjectCommon {
    ObjectCommon {
        owner: href(4, owner),
        reactors: if reactor { vec![href(4, owner)] } else { Vec::new() },
        xdictionary: None,
        ds_binary: false,
    }
}

impl<'a> Writer<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        Self {
            doc,
            handles: Handles::new(doc),
            notices: NoticeList::new(),
            objects: Vec::new(),
            classes: Vec::new(),
            blocks: Vec::new(),
            block_index: HashMap::new(),
            layouts: Vec::new(),
            linetypes: Vec::new(),
            layers: Vec::new(),
            layer_handles: HashMap::new(),
            linetype_handles: HashMap::new(),
            style_handles: HashMap::new(),
            dimstyle_handles: HashMap::new(),
            styles: Vec::new(),
            dimstyles: Vec::new(),
            remap: HashMap::new(),
            entity_handles: HashMap::new(),
            mline_styles: Vec::new(),
            fixed: Fixed::default(),
            sort_tables: Vec::new(),
            viewports: HashSet::new(),
            long_texts: Cell::new(0),
        }
    }

    /// 元のハンドルを保つ（読めないか重なれば新しく振る）。参照を付け替えられるよう、元 → 書いたハンドルを覚える
    pub(super) fn keep(&mut self, original: &str) -> u64 {
        let handle = self.handles.keep(original);
        if !original.is_empty() {
            self.remap.entry(upper(original)).or_insert(handle);
        }
        handle
    }

    /// 元のハンドルが指すものの、書いたハンドル
    pub(super) fn mapped(&self, original: &str) -> Option<u64> {
        if original.is_empty() {
            return None;
        }
        self.remap.get(&upper(original)).copied()
    }

    /// 文字列を DWG の形（R2007 以降の UTF-16）にする。モデルの文字列はテキストの DXF と同じキャレット表記（「^ 」は ^、
    /// ^J は改行。DWG の読み取りも同じ形にする）なので、文字に戻す。TV の長さは BS（2.2、読み取りは符号付き）なので、
    /// 32,767 単位を超える文字列はそこまで（サロゲートの対を分けない）で切り、数える
    pub(super) fn text(&self, value: &str) -> RawText {
        let mut text = if value.contains('^') {
            RawText::new(&uncaret(value), VERSION, encoding_rs::UTF_8)
        } else {
            RawText::new(value, VERSION, encoding_rs::UTF_8)
        };
        if let RawText::Units(units) = &mut text
            && units.len() > i16::MAX as usize
        {
            units.truncate(i16::MAX as usize);
            if units.last().is_some_and(|unit| (0xd800..0xdc00).contains(unit)) {
                units.pop();
            }
            self.long_texts.set(self.long_texts.get() + 1);
        }
        text
    }

    /// クラスの番号（500 から）。使った数を数える
    pub(super) fn class_number(&mut self, dxf_name: &'static str) -> u16 {
        match self.classes.iter().position(|(name, _)| *name == dxf_name) {
            Some(index) => {
                self.classes[index].1 += 1;
                500 + index as u16
            }
            None => {
                self.classes.push((dxf_name, 1));
                500 + (self.classes.len() - 1) as u16
            }
        }
    }

    /// 書いたオブジェクトを加える
    pub(super) fn push(&mut self, writer: &ObjectWriter) {
        self.objects.push((writer.handle, writer.bytes()));
    }

    /// 新しいオブジェクトを書き始める（拡張データなし）
    pub(super) fn object(&self, kind: u16, handle: u64) -> ObjectWriter {
        ObjectWriter::new(VERSION, encoding_rs::WINDOWS_1252, kind, handle)
    }

    fn add_block(&mut self, name: &str, source: Option<usize>) {
        let key = upper(name);
        if self.block_index.contains_key(&key) {
            return;
        }
        let block: Option<&DxfBlock> = source.map(|index| &self.doc.blocks[index]);
        let record = match block.and_then(|block| block.block_record_handle.as_deref()) {
            Some(original) => {
                let original = original.to_string();
                self.keep(&original)
            }
            None => self.handles.fresh(),
        };
        let begin = match block {
            Some(block) => {
                let original = block.handle.clone();
                self.keep(&original)
            }
            None => self.handles.fresh(),
        };
        let end = self.handles.fresh();
        let paper = key.starts_with(PAPER);
        self.block_index.insert(key, self.blocks.len());
        self.blocks.push(BlockPlan {
            name: name.to_string(),
            record,
            begin,
            end,
            layout: 0,
            source,
            paper,
            entities: Vec::new(),
            inserts: Vec::new(),
            has_attributes: false,
        });
    }

    /// 書くブロック・レイアウト・表を決め、ハンドルを振る（モデル空間と紙空間は必ず置く）
    fn plan(&mut self) {
        let doc = self.doc;
        let model = doc.blocks.iter().position(|block| upper(&block.name) == MODEL);
        self.add_block("*Model_Space", model);
        let paper = doc.blocks.iter().position(|block| upper(&block.name) == PAPER);
        self.add_block("*Paper_Space", paper);
        for (index, block) in doc.blocks.iter().enumerate() {
            self.add_block(&block.name, Some(index));
        }
        // 図形が参照するのに定義がないブロックは、空の定義を置く
        let mut referenced = Vec::new();
        for entity in all_entities(doc) {
            match &entity.data {
                DxfEntityData::Insert(insert) => referenced.push(insert.name.clone()),
                DxfEntityData::Dimension(dimension) if !dimension.anonymous_block_name.is_empty() => {
                    referenced.push(dimension.anonymous_block_name.clone())
                }
                DxfEntityData::Table(table) if !table.name.is_empty() => referenced.push(table.name.clone()),
                _ => {}
            }
            if let Some(block) = &entity.base.block {
                referenced.push(block.clone());
            }
        }
        for name in referenced {
            if !name.is_empty() && !self.block_index.contains_key(&upper(&name)) {
                self.notices.add(
                    NoticeLevel::Warning,
                    "missing-block",
                    "定義のないブロックを、空のブロックとして補いました",
                );
                self.add_block(&name, None);
            }
        }
        // 属性定義を持つブロック
        for entity in &doc.entities {
            if matches!(entity.data, DxfEntityData::Attdef { .. })
                && let Some(&index) = self.block_index.get(&container(entity))
            {
                self.blocks[index].has_attributes = true;
            }
        }
        // レイアウト: モデルと、紙空間のブロックごとに 1 つ（元のレイアウトがあればその名前と設定）
        let model_layout = doc.layouts.iter().find(|layout| layout.model);
        let handle = match model_layout {
            Some(layout) => self.keep(&layout.handle),
            None => self.handles.fresh(),
        };
        let mut layouts =
            vec![LayoutPlan { name: "Model".into(), handle, block: "*Model_Space".into(), source: model_layout }];
        let mut paper_blocks: Vec<String> =
            self.blocks.iter().filter(|block| block.paper).map(|block| block.name.clone()).collect();
        paper_blocks.sort_by_key(|name| (upper(name) != PAPER, upper(name)));
        // 紙空間のブロック（大文字）→ そのレイアウト（同じブロックのレイアウトが重なれば初めのもの）
        let mut paper_layouts: HashMap<String, &DxfLayout> = HashMap::new();
        for layout in doc.layouts.iter().filter(|layout| !layout.model) {
            paper_layouts.entry(upper(&layout.block)).or_insert(layout);
        }
        for (index, block) in paper_blocks.iter().enumerate() {
            let source = paper_layouts.get(&upper(block)).copied();
            let name = source.map_or_else(|| format!("Layout{}", index + 1), |layout| layout.name.clone());
            let handle = match source {
                Some(layout) => self.keep(&layout.handle),
                None => self.handles.fresh(),
            };
            layouts.push(LayoutPlan { name, handle, block: block.clone(), source });
        }
        for layout in &layouts {
            if let Some(&index) = self.block_index.get(&upper(&layout.block)) {
                self.blocks[index].layout = layout.handle;
            }
        }
        self.layouts = layouts;
        self.plan_mline_styles();
        self.plan_fixed();
        self.plan_tables();
        for entity in all_entities(doc) {
            let handle = self.keep(&entity.base.handle);
            self.entity_handles.insert(std::ptr::from_ref(&entity.base) as usize, handle);
        }
    }

    /// MLINESTYLE: STANDARD と図面のもの（MLINE が参照するので、図形より先に決める）
    fn plan_mline_styles(&mut self) {
        let mut styles: Vec<(u64, DxfMLineStyle)> = Vec::new();
        let mut seen = HashSet::new();
        for style in self.doc.mline_styles.iter().flatten() {
            if seen.insert(upper(&style.name)) {
                let handle = self.keep(&style.handle);
                styles.push((handle, style.clone()));
            }
        }
        if !seen.contains("STANDARD") {
            let handle = self.handles.fresh();
            let element =
                |offset: f64| zumen_dxf::DxfMLineStyleElement { offset, color: 256.0, linetype: "BYLAYER".into() };
            styles.insert(
                0,
                (
                    handle,
                    DxfMLineStyle {
                        handle: String::new(),
                        name: "STANDARD".into(),
                        flags: 0.0,
                        description: String::new(),
                        fill_color: 256.0,
                        start_angle: 90.0,
                        end_angle: 90.0,
                        elements: vec![element(0.5), element(-0.5)],
                    },
                ),
            );
        }
        self.mline_styles = styles;
    }

    /// 表の管理のオブジェクト・辞書・既定のオブジェクトのハンドル
    fn plan_fixed(&mut self) {
        let mut next = || self.handles.fresh();
        let mut fixed = Fixed {
            block_control: next(),
            layer_control: next(),
            style_control: next(),
            ltype_control: next(),
            view_control: next(),
            ucs_control: next(),
            vport_control: next(),
            appid_control: next(),
            dimstyle_control: next(),
            vport: next(),
            root: next(),
            color_dict: next(),
            group_dict: next(),
            layout_dict: next(),
            material_dict: next(),
            mleader_dict: next(),
            mline_dict: next(),
            plot_dict: next(),
            plotstyle_dict: next(),
            scale_dict: next(),
            tablestyle_dict: next(),
            visualstyle_dict: next(),
            variable_dict: next(),
            placeholder: next(),
            material_byblock: next(),
            material_bylayer: next(),
            material_global: next(),
            mleader_style: next(),
            scale: next(),
            variables: Vec::new(),
            appids: Vec::new(),
        };
        fixed.variables = vec![("CANNOSCALE", next(), "1:1"), ("CMLEADERSTYLE", next(), "Standard")];
        let mut appids = vec![("ACAD", next())];
        if self.doc.layers.iter().any(|layer| layer.transparency.is_some()) {
            appids.push(("AcCmTransparency", next()));
        }
        fixed.appids = appids;
        self.fixed = fixed;
    }

    /// 線種・レイヤ・文字スタイル・寸法スタイルを決める（DXF の書き出しと同じ）
    fn plan_tables(&mut self) {
        let doc = self.doc;
        // LTYPE: ByBlock・ByLayer・Continuous と図面の線種。図形やレイヤが参照するのに定義がない線種は実線として補う。
        // 決まった線種は、図面の綴り（CONTINUOUS など）があればそれを使う
        let spelling = |name: &str| -> String {
            all_entities(doc)
                .map(|entity| entity.base.linetype.as_str())
                .chain(doc.layers.iter().map(|layer| layer.linetype.as_str()))
                .chain(doc.linetypes.iter().map(|linetype| linetype.name.as_str()))
                .find(|candidate| candidate.eq_ignore_ascii_case(name))
                .unwrap_or(name)
                .to_string()
        };
        let mut seen = HashSet::new();
        let mut plans: Vec<LinetypeSource<'a>> = Vec::new();
        for name in ["ByBlock", "ByLayer", "Continuous"] {
            seen.insert(upper(name));
            let original = doc.linetypes.iter().find(|linetype| linetype.name.eq_ignore_ascii_case(name));
            let description = if name == "Continuous" { "Solid line" } else { "" };
            plans.push((spelling(name), description.into(), 0.0, Vec::new(), original));
        }
        // 文字・形を含む線種は、DXF の書き出しと同じく線と空白だけを書く（文字・形は捨てる）
        let mut complex = 0;
        for linetype in &doc.linetypes {
            if seen.insert(upper(&linetype.name)) {
                complex += u64::from(linetype.complex);
                plans.push((
                    linetype.name.clone(),
                    linetype.description.clone(),
                    linetype.pattern_length,
                    linetype.dashes.clone(),
                    Some(linetype),
                ));
            }
        }
        if complex > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "complex-linetype",
                "文字・形を含む線種は、線と空白だけを書きました",
                complex,
            );
        }
        let referenced = all_entities(doc)
            .map(|entity| entity.base.linetype.as_str())
            .chain(doc.layers.iter().map(|layer| layer.linetype.as_str()));
        for name in referenced {
            if !name.is_empty() && seen.insert(upper(name)) {
                plans.push((name.to_string(), String::new(), 0.0, Vec::new(), None));
            }
        }
        for (name, description, length, dashes, source) in plans {
            let handle = match source {
                Some(linetype) => {
                    let original = linetype.handle.clone();
                    self.keep(&original)
                }
                None => self.handles.fresh(),
            };
            self.linetype_handles.insert(upper(&name), handle);
            self.linetypes.push(LinetypePlan { name, handle, description, length, dashes });
        }

        // LAYER: 図面のレイヤと、図形が参照するのに定義がないレイヤ（表示が読む順。レイヤ 0 は最後に必ず置く）
        let mut seen = HashSet::new();
        let mut layers = Vec::new();
        for layer in &doc.layers {
            if seen.insert(upper(&layer.name)) {
                let handle = self.keep(&layer.handle);
                layers.push(LayerPlan {
                    name: layer.name.clone(),
                    handle,
                    color: layer.color_index.abs().max(1.0),
                    true_color: layer.true_color,
                    frozen: layer.frozen,
                    off: layer.off,
                    locked: layer.locked,
                    plot: layer.plot,
                    linetype: if layer.linetype.is_empty() { "Continuous".into() } else { layer.linetype.clone() },
                    lineweight: layer.lineweight,
                    transparency: layer.transparency,
                });
            }
        }
        let names: Vec<String> = layer_order(doc).into_iter().chain(std::iter::once("0")).map(str::to_string).collect();
        for name in names {
            if !name.is_empty() && seen.insert(upper(&name)) {
                let handle = self.handles.fresh();
                layers.push(LayerPlan {
                    name,
                    handle,
                    color: 7.0,
                    true_color: None,
                    frozen: false,
                    off: false,
                    locked: false,
                    plot: true,
                    linetype: "CONTINUOUS".into(),
                    lineweight: -3.0,
                    transparency: None,
                });
            }
        }
        for layer in &layers {
            self.layer_handles.insert(upper(&layer.name), layer.handle);
        }
        self.layers = layers;

        // STYLE: 図面の文字スタイルと、図形が参照するのに定義がないもの。Standard は必ず置く
        let mut seen = HashSet::new();
        let mut styles: Vec<(String, Option<&'a zumen_dxf::DxfTextStyle>)> = Vec::new();
        for style in &doc.styles {
            if seen.insert(upper(&style.name)) {
                styles.push((style.name.clone(), Some(style)));
            }
        }
        let names: Vec<&str> = all_entities(doc)
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
                    Some(text.style.as_str())
                }
                DxfEntityData::MText(mtext) => Some(mtext.style.as_str()),
                _ => None,
            })
            .collect();
        for name in names.into_iter().chain(std::iter::once("Standard")) {
            if !name.is_empty() && seen.insert(upper(name)) {
                styles.push((name.to_string(), None));
            }
        }
        for (name, style) in styles {
            let handle = match style {
                Some(style) => {
                    let original = style.handle.clone();
                    self.keep(&original)
                }
                None => self.handles.fresh(),
            };
            self.style_handles.insert(upper(&name), handle);
            self.styles.push((name, handle, style));
        }

        // DIMSTYLE: Standard（図面になければ）と図面の寸法スタイル、図形が参照するのに定義がないもの
        let mut seen = HashSet::new();
        let mut dimstyles: Vec<(String, Option<&'a zumen_dxf::DxfDimStyle>)> = Vec::new();
        if !doc.dimstyles.iter().any(|style| upper(&style.name) == "STANDARD") {
            seen.insert("STANDARD".to_string());
            dimstyles.push(("Standard".into(), None));
        }
        for style in &doc.dimstyles {
            if seen.insert(upper(&style.name)) {
                dimstyles.push((style.name.clone(), Some(style)));
            }
        }
        let names: Vec<&str> = all_entities(doc)
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Dimension(dimension) => Some(dimension.dimstyle.as_str()),
                DxfEntityData::Leader(leader) => Some(leader.dimstyle.as_str()),
                _ => None,
            })
            .collect();
        for name in names {
            if !name.is_empty() && seen.insert(upper(name)) {
                dimstyles.push((name.to_string(), None));
            }
        }
        for (name, style) in dimstyles {
            let handle = match style {
                Some(style) => {
                    let original = style.handle.clone();
                    self.keep(&original)
                }
                None => self.handles.fresh(),
            };
            self.dimstyle_handles.insert(upper(&name), handle);
            self.dimstyles.push((name, handle, style));
        }
    }

    /// 書き終えた図面
    fn finish(mut self) -> DwgWritten {
        let seed = self.handles.next.max(self.objects.iter().map(|(handle, _)| handle + 1).max().unwrap_or(1));
        let header = header::header_section(&self, seed);
        if self.long_texts.get() > 0 {
            self.notices.add_count(
                NoticeLevel::Warning,
                "long-text",
                "DWG の文字列の長さの上限（UTF-16 で 32,767 単位）を超える文字列は、その長さまでを書きました",
                self.long_texts.get(),
            );
        }
        let classes: Vec<ClassDef> = self
            .classes
            .iter()
            .enumerate()
            .filter_map(|(index, (name, count))| objects::class_def(name, 500 + index as u16, *count))
            .collect();
        let mut objects = std::mem::take(&mut self.objects);
        objects.sort_by_key(|(handle, _)| *handle);
        let count = objects.len();
        let layout = Objects {
            items: objects.iter().map(|(_, bytes)| bytes.clone()).collect(),
            handles: objects.iter().enumerate().map(|(index, (handle, _))| (*handle, index)).collect(),
        };
        let (stream, locations) = layout.lay_out(0);
        let measurement = self.doc.header.measurement;
        let sections = header::sections(&self, header, &classes, stream, &locations, count, measurement, seed);
        let mut head = [0u8; 0x80];
        head[..6].copy_from_slice(VERSION.name().as_bytes());
        head[0x0b] = MAINTENANCE;
        head[0x0c] = 3;
        // 書いたアプリケーションの版（AC1027 は 31）と保守版、文字コード
        head[0x11] = 31;
        head[0x12] = MAINTENANCE;
        head[0x13..0x15].copy_from_slice(&header::codepage_number(self.doc).to_le_bytes());
        head[0x28] = 0x80;
        let order = header::stream_order(&sections);
        let bytes = write_r2004(&R2004 { head, sections, stream_order: order });
        DwgWritten { bytes, notices: self.notices.into_vec() }
    }
}

/// 図形が属するブロック（モデル空間・紙空間・ブロック定義）の名前（大文字）
pub(super) fn container(entity: &DxfEntity) -> String {
    match entity.base.space {
        DxfSpace::Model => MODEL.into(),
        DxfSpace::Paper => upper(entity.base.block.as_deref().unwrap_or("*Paper_Space")),
        DxfSpace::Block => upper(entity.base.block.as_deref().unwrap_or("")),
    }
}

/// テキストの DXF のキャレット表記を文字に戻す（「^ 」は ^、^@〜^_ は制御文字）。ほかの ^ はそのまま
fn uncaret(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(' ') => {
                out.push('^');
                chars.next();
            }
            Some(next) if ('@'..='_').contains(&next) => {
                out.push(char::from(next as u8 - 0x40));
                chars.next();
            }
            _ => out.push('^'),
        }
    }
    out
}

/// 押し出し方向の OCS の X・Y 軸（任意の軸の求め方、DXF Reference「Arbitrary Axis Algorithm」）
pub(super) fn ocs_axes(normal: Vec3) -> (Vec3, Vec3) {
    let length = zumen_core::js::hypot(&normal);
    if length.is_nan() || length <= 0.0 {
        return ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
    }
    let n = normal.map(|v| v / length);
    let cross = |a: Vec3, b: Vec3| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let base = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] };
    let ax = cross(base, n);
    let ax_length = zumen_core::js::hypot(&ax);
    let ax = ax.map(|v| v / ax_length);
    let ay = cross(n, ax);
    let ay_length = zumen_core::js::hypot(&ay);
    (ax, ay.map(|v| v / ay_length))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_come_back_close_and_sweeps_the_same() {
        for degrees in [225.0089176309779, 45.0089176309779, 90.0, 0.0, -30.0, 12.5] {
            assert!((degrees_of(rad(degrees)) - degrees).abs() <= 1e-13, "{degrees}");
        }
        for degrees in [45.0089176309779, 90.0, 12.5] {
            assert_eq!(degrees_of(rad(degrees)), degrees);
        }
        for (start, end) in
            [(225.0089176309779, 45.0089176309779), (343.1721871543273, 163.1721871543273), (30.0, 210.0)]
        {
            let (s, e) = rad_pair(start, end);
            assert_eq!(degrees_of(e) - degrees_of(s), end - start, "{start} {end}");
        }
    }

    #[test]
    fn caret_notation_becomes_characters() {
        assert_eq!(uncaret("a^ b^Jc^Id"), "a^b\nc\td");
        assert_eq!(uncaret("^"), "^");
        assert_eq!(uncaret("x^z"), "x^z");
    }
}
