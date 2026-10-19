//! AutoCAD の図面のモデル（`DxfDocument`）を DXF（ASCII、AutoCAD 2013 形式の AC1027、UTF-8）に書く。
//! DXF・DWG から読んだ図面を DXF にする変換に使う（DXF Reference の各節・各図形のグループコードに従う）。
//!
//! 書くもの: 見出しの主な変数、表（VPORT・LTYPE・LAYER・STYLE・VIEW・UCS・APPID・DIMSTYLE・BLOCK_RECORD）、
//! ブロック定義、図形、OBJECTS の辞書（ACAD_GROUP・ACAD_LAYOUT・ACAD_MLINESTYLE・ACAD_PLOTSETTINGS・
//! ACAD_PLOTSTYLENAME）、レイアウト、MLINESTYLE。ハンドルは元の図面のものを保ち、ないもの・重なるものには
//! 新しいハンドルを振る。図形が参照するのに表にないレイヤ・線種・文字スタイル・寸法スタイル・ブロックは補う。
//! 書けない図形（MULTILEADER・IMAGE・WIPEOUT）は種類ごとの件数を返す。

use crate::model::{
    DxfBlock, DxfDocument, DxfEntity, DxfEntityBase, DxfEntityData, DxfHatch, DxfHatchEdge, DxfLayout, DxfMLine,
    DxfMLineStyle, DxfPolyline, DxfSpace, DxfTextBase, Vec3,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

/// 書いた DXF と、書けなかった図形の種類ごとの件数
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DxfWritten {
    pub bytes: Vec<u8>,
    pub skipped: Vec<(String, u64)>,
}

/// 図形の種類ごとに、書けないもの
const UNWRITTEN: [&str; 4] = ["MULTILEADER", "MLEADER", "IMAGE", "WIPEOUT"];

/// DXF にする
pub fn write_dxf(doc: &DxfDocument) -> DxfWritten {
    let mut writer = Writer::new(doc);
    writer.write();
    let skipped = writer.skipped.into_iter().collect();
    DxfWritten { bytes: writer.out.text.into_bytes(), skipped }
}

/// グループコードと値の行
struct Out {
    text: String,
}

/// 実数。最短で元の値に戻る表し方にし、整数にも小数点を付ける
fn real(value: f64) -> String {
    if !value.is_finite() {
        return "0.0".into();
    }
    let abs = value.abs();
    let text = if abs != 0.0 && !(1e-9..1e16).contains(&abs) { format!("{value:e}") } else { format!("{value}") };
    if text.contains(['.', 'e', 'E']) { text } else { format!("{text}.0") }
}

impl Out {
    fn tag(&mut self, code: i32, value: impl std::fmt::Display) {
        let _ = write!(self.text, "{code:>3}\r\n{value}\r\n");
    }

    /// 文字列。改行は空白にする（MTEXT の改行は \P で書く）
    fn str(&mut self, code: i32, value: &str) {
        let clean: String = value.chars().map(|c| if c == '\r' || c == '\n' { ' ' } else { c }).collect();
        self.tag(code, clean);
    }

    fn real(&mut self, code: i32, value: f64) {
        self.tag(code, real(value));
    }

    fn int(&mut self, code: i32, value: f64) {
        let value = if value.is_finite() { value.trunc() as i64 } else { 0 };
        self.tag(code, value);
    }

    fn handle(&mut self, code: i32, handle: u64) {
        self.tag(code, format!("{handle:X}"));
    }

    /// 点（code、code + 10、code + 20）
    fn point(&mut self, code: i32, point: &[f64]) {
        for (index, value) in point.iter().take(3).enumerate() {
            self.real(code + 10 * index as i32, *value);
        }
    }

    fn point2(&mut self, code: i32, point: &[f64]) {
        self.point(code, &point[..point.len().min(2)]);
    }
}

/// ハンドル。元の図面のハンドルを保ち（重なれば新しく振る）、新しいハンドルは元の最大より後から振る
struct Handles {
    next: u64,
    used: HashSet<u64>,
}

impl Handles {
    fn new(doc: &DxfDocument) -> Self {
        let mut max = 0;
        let mut note = |text: &str| {
            if let Ok(value) = u64::from_str_radix(text, 16) {
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
        for entity in &doc.entities {
            note(&entity.base.handle);
            if let DxfEntityData::Insert(insert) = &entity.data {
                for attrib in &insert.attribs {
                    note(&attrib.base.handle);
                }
            }
        }
        Self { next: max + 1, used: HashSet::new() }
    }

    fn fresh(&mut self) -> u64 {
        while self.used.contains(&self.next) {
            self.next += 1;
        }
        let handle = self.next;
        self.used.insert(handle);
        self.next += 1;
        handle
    }

    /// 元のハンドル。読めないか、0 か、すでに使ったものなら新しく振る（新しいハンドルは元の最大より後なので重ならない）
    fn keep(&mut self, original: &str) -> u64 {
        match u64::from_str_radix(original, 16) {
            Ok(value) if value > 0 && self.used.insert(value) => value,
            _ => self.fresh(),
        }
    }
}

/// ブロックの定義（書く順）
struct BlockRecord {
    name: String,
    handle: u64,
    begin: u64,
    end: u64,
    layout: u64,
    base: Vec3,
    flags: f64,
    xref: Option<String>,
    /// 紙空間のブロック（*Paper_Space…）
    paper: bool,
}

struct Writer<'a> {
    doc: &'a DxfDocument,
    out: Out,
    handles: Handles,
    skipped: BTreeMap<String, u64>,
    /// 名前（大文字）→ ブロックの定義
    blocks: Vec<BlockRecord>,
    block_index: HashMap<String, usize>,
    /// レイアウト（書く順）: 名前、ハンドル、ブロックの名前、元のレイアウト
    layouts: Vec<(String, u64, String, Option<&'a DxfLayout>)>,
    layer_handles: HashMap<String, u64>,
    mline_styles: Vec<(u64, DxfMLineStyle)>,
    placeholder: u64,
}

/// 図形（属性を含む）
fn all_entities(doc: &DxfDocument) -> impl Iterator<Item = &DxfEntity> {
    doc.entities.iter().flat_map(|entity| {
        let attribs: &[DxfEntity] = match &entity.data {
            DxfEntityData::Insert(insert) => &insert.attribs,
            _ => &[],
        };
        std::iter::once(entity).chain(attribs.iter())
    })
}

const MODEL: &str = "*MODEL_SPACE";
const PAPER: &str = "*PAPER_SPACE";

fn upper(name: &str) -> String {
    name.to_uppercase()
}

impl<'a> Writer<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        Self {
            doc,
            out: Out { text: String::new() },
            handles: Handles::new(doc),
            skipped: BTreeMap::new(),
            blocks: Vec::new(),
            block_index: HashMap::new(),
            layouts: Vec::new(),
            layer_handles: HashMap::new(),
            mline_styles: Vec::new(),
            placeholder: 0,
        }
    }

    fn add_block(&mut self, name: &str, source: Option<&DxfBlock>) {
        let key = upper(name);
        if self.block_index.contains_key(&key) {
            return;
        }
        let handle = match source.and_then(|block| block.block_record_handle.as_deref()) {
            Some(original) => self.handles.keep(original),
            None => self.handles.fresh(),
        };
        let begin = match source {
            Some(block) => self.handles.keep(&block.handle),
            None => self.handles.fresh(),
        };
        let end = self.handles.fresh();
        let paper = key.starts_with(PAPER);
        self.block_index.insert(key, self.blocks.len());
        self.blocks.push(BlockRecord {
            name: name.to_string(),
            handle,
            begin,
            end,
            layout: 0,
            base: source.map_or([0.0; 3], |block| block.base_point),
            flags: source.map_or(0.0, |block| block.flags),
            xref: source.and_then(|block| block.xref_path.clone()),
            paper,
        });
    }

    /// 書くブロック・レイアウトを決める（モデル空間と紙空間は必ず置く）
    fn plan(&mut self) {
        let doc = self.doc;
        let model = doc.blocks.iter().find(|block| upper(&block.name) == MODEL);
        self.add_block("*Model_Space", model);
        let paper = doc.blocks.iter().find(|block| upper(&block.name) == PAPER);
        self.add_block("*Paper_Space", paper);
        for block in &doc.blocks {
            self.add_block(&block.name, Some(block));
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
                *self.skipped.entry("（定義のないブロックを空で補った）".into()).or_insert(0) += 1;
                self.add_block(&name, None);
            }
        }
        // レイアウト: モデルと、紙空間のブロックごとに 1 つ（元のレイアウトがあればその名前と設定）
        let mut layouts: Vec<(String, u64, String, Option<&'a DxfLayout>)> = Vec::new();
        let model_layout = doc.layouts.iter().find(|layout| layout.model);
        let handle = match model_layout {
            Some(layout) => self.handles.keep(&layout.handle),
            None => self.handles.fresh(),
        };
        layouts.push(("Model".into(), handle, "*Model_Space".into(), model_layout));
        let mut paper_blocks: Vec<String> =
            self.blocks.iter().filter(|block| block.paper).map(|block| block.name.clone()).collect();
        paper_blocks.sort_by_key(|name| (upper(name) != PAPER, upper(name)));
        for (index, block) in paper_blocks.iter().enumerate() {
            let source = doc.layouts.iter().find(|layout| !layout.model && upper(&layout.block) == upper(block));
            let name = source.map_or_else(|| format!("Layout{}", index + 1), |layout| layout.name.clone());
            let handle = match source {
                Some(layout) => self.handles.keep(&layout.handle),
                None => self.handles.fresh(),
            };
            layouts.push((name, handle, block.clone(), source));
        }
        for (_, handle, block, _) in &layouts {
            if let Some(&index) = self.block_index.get(&upper(block)) {
                self.blocks[index].layout = *handle;
            }
        }
        self.layouts = layouts;
        // MLINESTYLE: Standard と図面のもの（MLINE が 340 で参照するので、図形より先に決める）
        let mut styles: Vec<(u64, DxfMLineStyle)> = Vec::new();
        let mut seen = HashSet::new();
        for style in doc.mline_styles.iter().flatten() {
            if seen.insert(upper(&style.name)) {
                let handle = self.handles.keep(&style.handle);
                styles.push((handle, style.clone()));
            }
        }
        if !seen.contains("STANDARD") {
            let handle = self.handles.fresh();
            let element =
                |offset: f64| crate::model::DxfMLineStyleElement { offset, color: 256.0, linetype: "BYLAYER".into() };
            styles.insert(
                0,
                (
                    handle,
                    DxfMLineStyle {
                        handle: String::new(),
                        name: "Standard".into(),
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

    fn write(&mut self) {
        self.plan();
        self.placeholder = self.handles.fresh();
        // 表と OBJECTS の固定のハンドル
        let tables: Vec<u64> = (0..9).map(|_| self.handles.fresh()).collect();
        let root = self.handles.fresh();
        self.header();
        self.classes();
        self.tables(&tables);
        self.blocks_section();
        self.entities_section();
        self.objects(root);
        self.out.tag(0, "EOF");
        // $HANDSEED は最後に決まるので、見出しの仮の値を置き換える
        let seed = format!("{:X}", self.handles.next + 1);
        self.out.text = self.out.text.replacen("HANDSEED_PLACEHOLDER", &seed, 1);
    }

    fn section(&mut self, name: &str) {
        self.out.tag(0, "SECTION");
        self.out.tag(2, name);
    }

    fn header(&mut self) {
        let header = &self.doc.header;
        self.section("HEADER");
        let out = &mut self.out;
        out.tag(9, "$ACADVER");
        out.tag(1, "AC1027");
        out.tag(9, "$ACADMAINTVER");
        out.tag(70, 105);
        out.tag(9, "$DWGCODEPAGE");
        out.tag(3, "ANSI_1252");
        out.tag(9, "$HANDSEED");
        out.tag(5, "HANDSEED_PLACEHOLDER");
        out.tag(9, "$INSUNITS");
        out.int(70, header.insunits);
        if let Some(measurement) = header.measurement {
            out.tag(9, "$MEASUREMENT");
            out.int(70, measurement);
        }
        for (name, value) in [
            ("$LTSCALE", header.ltscale),
            ("$DIMSCALE", header.dimscale),
            ("$DIMASZ", header.dimasz),
            ("$DIMTXT", header.dimtxt),
        ] {
            out.tag(9, name);
            out.real(40, value);
        }
        if let Some(size) = header.textsize {
            out.tag(9, "$TEXTSIZE");
            out.real(40, size);
        }
        out.tag(9, "$PSLTSCALE");
        out.int(70, header.psltscale);
        out.tag(9, "$TILEMODE");
        out.int(70, header.tilemode);
        for (name, point) in [("$EXTMIN", header.extmin), ("$EXTMAX", header.extmax)] {
            if let Some(point) = point {
                out.tag(9, name);
                out.point(10, &point);
            }
        }
        for (name, point) in [
            ("$LIMMIN", header.limmin),
            ("$LIMMAX", header.limmax),
            ("$PLIMMIN", header.plimmin),
            ("$PLIMMAX", header.plimmax),
        ] {
            if let Some(point) = point {
                out.tag(9, name);
                out.point2(10, &point);
            }
        }
        for (name, value) in
            [("$CLAYER", "0"), ("$CELTYPE", "ByLayer"), ("$TEXTSTYLE", "Standard"), ("$DIMSTYLE", "Standard")]
        {
            out.tag(9, name);
            out.str(
                if name == "$DIMSTYLE" {
                    2
                } else if name == "$TEXTSTYLE" {
                    7
                } else if name == "$CELTYPE" {
                    6
                } else {
                    8
                },
                value,
            );
        }
        self.out.tag(0, "ENDSEC");
    }

    /// LAYOUT などの、AutoCAD の決まったクラス（DXF Reference「CLASSES Section」）
    fn classes(&mut self) {
        self.section("CLASSES");
        for (dxf, cpp) in [
            ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault"),
            ("ACDBPLACEHOLDER", "AcDbPlaceHolder"),
            ("LAYOUT", "AcDbLayout"),
        ] {
            self.out.tag(0, "CLASS");
            self.out.tag(1, dxf);
            self.out.tag(2, cpp);
            self.out.tag(3, "ObjectDBX Classes");
            self.out.tag(90, 0);
            self.out.tag(91, 0);
            self.out.tag(280, 0);
            self.out.tag(281, 0);
        }
        self.out.tag(0, "ENDSEC");
    }

    fn table(&mut self, name: &str, handle: u64, count: usize) {
        self.out.tag(0, "TABLE");
        self.out.tag(2, name);
        self.out.handle(5, handle);
        self.out.tag(330, 0);
        self.out.tag(100, "AcDbSymbolTable");
        self.out.tag(70, count);
        if name == "DIMSTYLE" {
            self.out.tag(100, "AcDbDimStyleTable");
        }
    }

    fn record(&mut self, kind: &str, handle: u64, owner: u64, subclass: &str, name: &str) {
        self.out.tag(0, kind);
        self.out.handle(if kind == "DIMSTYLE" { 105 } else { 5 }, handle);
        self.out.handle(330, owner);
        self.out.tag(100, "AcDbSymbolTableRecord");
        self.out.tag(100, subclass);
        self.out.str(2, name);
    }

    fn tables(&mut self, ids: &[u64]) {
        let doc = self.doc;
        self.section("TABLES");
        // VPORT
        self.table("VPORT", ids[0], 1);
        let handle = self.handles.fresh();
        self.record("VPORT", handle, ids[0], "AcDbViewportTableRecord", "*Active");
        let out = &mut self.out;
        out.tag(70, 0);
        out.point2(10, &[0.0, 0.0]);
        out.point2(11, &[1.0, 1.0]);
        let (center, height) = match (doc.header.extmin, doc.header.extmax) {
            (Some(min), Some(max)) if min[0] <= max[0] && min[1] <= max[1] => {
                ([(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0], (max[1] - min[1]).max(max[0] - min[0]).max(1.0))
            }
            _ => ([0.0, 0.0], 1000.0),
        };
        out.point2(12, &center);
        out.point2(13, &[0.0, 0.0]);
        out.point2(14, &[0.5, 0.5]);
        out.point2(15, &[0.5, 0.5]);
        out.point(16, &[0.0, 0.0, 1.0]);
        out.point(17, &[0.0, 0.0, 0.0]);
        out.real(40, height);
        out.real(41, 1.34);
        out.real(42, 50.0);
        for code in [43, 44, 50, 51] {
            out.real(code, 0.0);
        }
        for (code, value) in
            [(71, 0), (72, 1000), (73, 1), (74, 3), (75, 0), (76, 0), (77, 0), (78, 0), (281, 0), (65, 0)]
        {
            out.tag(code, value);
        }
        out.real(146, 0.0);
        self.out.tag(0, "ENDTAB");

        // LTYPE: ByBlock・ByLayer・Continuous と、図面の線種。図形やレイヤが参照するのに定義がない線種は実線として補う
        let mut linetypes: Vec<(String, String, f64, Vec<f64>, Option<&str>)> = Vec::new();
        let mut seen = HashSet::new();
        // 決まった線種は、図面の綴り（CONTINUOUS など）があればそれを使う
        let spelling = |name: &str| -> String {
            all_entities(doc)
                .map(|entity| entity.base.linetype.as_str())
                .chain(doc.layers.iter().map(|layer| layer.linetype.as_str()))
                .chain(doc.linetypes.iter().map(|linetype| linetype.name.as_str()))
                .find(|candidate| upper(candidate) == upper(name))
                .unwrap_or(name)
                .to_string()
        };
        for name in ["ByBlock", "ByLayer", "Continuous"] {
            seen.insert(upper(name));
            let original = doc.linetypes.iter().find(|linetype| upper(&linetype.name) == upper(name));
            linetypes.push((
                spelling(name),
                String::new(),
                0.0,
                Vec::new(),
                original.map(|linetype| linetype.handle.as_str()),
            ));
        }
        // 文字・形を含む線種は、表示と同じく線と空白だけを書く（文字・形は捨てる）
        let mut complex = 0;
        for linetype in &doc.linetypes {
            if seen.insert(upper(&linetype.name)) {
                complex += u64::from(linetype.complex);
                linetypes.push((
                    linetype.name.clone(),
                    linetype.description.clone(),
                    linetype.pattern_length,
                    linetype.dashes.clone(),
                    Some(&linetype.handle),
                ));
            }
        }
        if complex > 0 {
            *self.skipped.entry("文字・形を含む線種（線と空白だけを書いた）".into()).or_insert(0) += complex;
        }
        let referenced: Vec<String> = all_entities(doc)
            .map(|entity| entity.base.linetype.clone())
            .chain(doc.layers.iter().map(|layer| layer.linetype.clone()))
            .collect();
        for name in referenced {
            if !name.is_empty() && seen.insert(upper(&name)) {
                linetypes.push((name, String::new(), 0.0, Vec::new(), None));
            }
        }
        self.table("LTYPE", ids[1], linetypes.len());
        for (name, description, length, dashes, original) in &linetypes {
            let handle = match original {
                Some(original) => self.handles.keep(original),
                None => self.handles.fresh(),
            };
            self.record("LTYPE", handle, ids[1], "AcDbLinetypeTableRecord", name);
            let out = &mut self.out;
            out.tag(70, 0);
            out.str(3, description);
            out.tag(72, 65);
            out.tag(73, dashes.len());
            out.real(40, if dashes.is_empty() { 0.0 } else { *length });
            for dash in dashes {
                out.real(49, *dash);
                out.tag(74, 0);
            }
        }
        self.out.tag(0, "ENDTAB");

        // LAYER: 0 と図面のレイヤ。図形が参照するのに定義がないレイヤは補う
        struct Layer {
            name: String,
            handle: Option<String>,
            color: f64,
            true_color: Option<f64>,
            flags: i32,
            linetype: String,
            plot: bool,
            lineweight: f64,
        }
        let mut layers: Vec<Layer> = Vec::new();
        let mut seen = HashSet::new();
        for layer in &doc.layers {
            if seen.insert(upper(&layer.name)) {
                let color = layer.color_index.abs().max(1.0);
                layers.push(Layer {
                    name: layer.name.clone(),
                    handle: Some(layer.handle.clone()),
                    color: if layer.off { -color } else { color },
                    true_color: layer.true_color,
                    flags: i32::from(layer.frozen) | (i32::from(layer.locked) << 2),
                    linetype: if layer.linetype.is_empty() { "Continuous".into() } else { layer.linetype.clone() },
                    plot: layer.plot,
                    lineweight: layer.lineweight,
                });
            }
        }
        // レイヤ 0 は必ず置く（図面のレイヤの番号が変わらないよう後ろに）
        let names: Vec<String> =
            std::iter::once("0".to_string()).chain(all_entities(doc).map(|entity| entity.base.layer.clone())).collect();
        for name in names {
            if !name.is_empty() && seen.insert(upper(&name)) {
                layers.push(Layer {
                    name,
                    handle: None,
                    color: 7.0,
                    true_color: None,
                    flags: 0,
                    linetype: "Continuous".into(),
                    plot: true,
                    lineweight: -3.0,
                });
            }
        }
        self.table("LAYER", ids[2], layers.len());
        for layer in &layers {
            let handle = match &layer.handle {
                Some(original) => self.handles.keep(original),
                None => self.handles.fresh(),
            };
            self.layer_handles.insert(upper(&layer.name), handle);
            self.record("LAYER", handle, ids[2], "AcDbLayerTableRecord", &layer.name);
            let out = &mut self.out;
            out.tag(70, layer.flags);
            out.int(62, layer.color);
            if let Some(true_color) = layer.true_color {
                out.int(420, true_color);
            }
            out.str(6, &layer.linetype);
            if !layer.plot {
                out.tag(290, 0);
            }
            out.int(370, layer.lineweight);
            out.handle(390, self.placeholder);
        }
        self.out.tag(0, "ENDTAB");

        // STYLE
        let mut styles: Vec<(String, Option<&crate::model::DxfTextStyle>)> = vec![];
        let mut seen = HashSet::new();
        for style in &doc.styles {
            if seen.insert(upper(&style.name)) {
                styles.push((style.name.clone(), Some(style)));
            }
        }
        let names: Vec<String> = all_entities(doc)
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
                    Some(text.style.clone())
                }
                DxfEntityData::MText(mtext) => Some(mtext.style.clone()),
                _ => None,
            })
            .collect();
        // Standard は必ず置く（図面の綴りがあればそれを使う）
        for name in names.into_iter().chain(std::iter::once("Standard".to_string())) {
            if !name.is_empty() && seen.insert(upper(&name)) {
                styles.push((name, None));
            }
        }
        self.table("STYLE", ids[3], styles.len());
        for (name, style) in &styles {
            let handle = match style {
                Some(style) => self.handles.keep(&style.handle),
                None => self.handles.fresh(),
            };
            self.record("STYLE", handle, ids[3], "AcDbTextStyleTableRecord", name);
            let out = &mut self.out;
            match style {
                Some(style) => {
                    out.int(70, style.flags);
                    out.real(40, style.height);
                    out.real(41, if style.width > 0.0 { style.width } else { 1.0 });
                    out.real(50, style.oblique);
                    out.int(71, style.generation_flags);
                    out.real(42, 2.5);
                    out.str(3, if style.font.is_empty() { "txt" } else { &style.font });
                    out.str(4, &style.bigfont);
                    if let Some(family) = &style.font_family {
                        out.tag(1001, "ACAD");
                        out.str(1000, family);
                        out.tag(1071, 0);
                    }
                }
                None => {
                    out.tag(70, 0);
                    out.real(40, 0.0);
                    out.real(41, 1.0);
                    out.real(50, 0.0);
                    out.tag(71, 0);
                    out.real(42, 2.5);
                    out.str(3, "txt");
                    out.str(4, "");
                }
            }
        }
        self.out.tag(0, "ENDTAB");

        // VIEW・UCS（空）、APPID
        self.table("VIEW", ids[4], 0);
        self.out.tag(0, "ENDTAB");
        self.table("UCS", ids[5], 0);
        self.out.tag(0, "ENDTAB");
        self.table("APPID", ids[6], 1);
        let handle = self.handles.fresh();
        self.record("APPID", handle, ids[6], "AcDbRegAppTableRecord", "ACAD");
        self.out.tag(70, 0);
        self.out.tag(0, "ENDTAB");

        // DIMSTYLE
        let mut dimstyles: Vec<(String, Option<&crate::model::DxfDimStyle>)> = vec![];
        let mut seen = HashSet::new();
        if !doc.dimstyles.iter().any(|style| upper(&style.name) == "STANDARD") {
            seen.insert("STANDARD".to_string());
            dimstyles.push(("Standard".into(), None));
        }
        for style in &doc.dimstyles {
            if seen.insert(upper(&style.name)) {
                dimstyles.push((style.name.clone(), Some(style)));
            }
        }
        let names: Vec<String> = all_entities(doc)
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Dimension(dimension) => Some(dimension.dimstyle.clone()),
                DxfEntityData::Leader(leader) => Some(leader.dimstyle.clone()),
                _ => None,
            })
            .collect();
        for name in names {
            if !name.is_empty() && seen.insert(upper(&name)) {
                dimstyles.push((name, None));
            }
        }
        self.table("DIMSTYLE", ids[7], dimstyles.len());
        for (name, style) in &dimstyles {
            let handle = match style {
                Some(style) => self.handles.keep(&style.handle),
                None => self.handles.fresh(),
            };
            self.record("DIMSTYLE", handle, ids[7], "AcDbDimStyleTableRecord", name);
            let (scale, arrow, text) =
                style.map_or((1.0, 2.5, 2.5), |style| (style.dimscale, style.dimasz, style.dimtxt));
            let out = &mut self.out;
            out.tag(70, 0);
            out.real(40, scale);
            out.real(41, arrow);
            out.real(140, text);
        }
        self.out.tag(0, "ENDTAB");

        // BLOCK_RECORD
        self.table("BLOCK_RECORD", ids[8], self.blocks.len());
        for index in 0..self.blocks.len() {
            let (handle, name, layout) =
                (self.blocks[index].handle, self.blocks[index].name.clone(), self.blocks[index].layout);
            self.record("BLOCK_RECORD", handle, ids[8], "AcDbBlockTableRecord", &name);
            self.out.handle(340, layout);
            self.out.tag(70, 0);
            self.out.tag(280, 1);
            self.out.tag(281, 0);
        }
        self.out.tag(0, "ENDTAB");
        self.out.tag(0, "ENDSEC");
    }

    /// 図形が属するブロック（モデル空間・紙空間・ブロック定義）の名前（大文字）
    fn container(entity: &DxfEntity) -> String {
        match entity.base.space {
            DxfSpace::Model => MODEL.into(),
            DxfSpace::Paper => upper(entity.base.block.as_deref().unwrap_or("*Paper_Space")),
            DxfSpace::Block => upper(entity.base.block.as_deref().unwrap_or("")),
        }
    }

    fn blocks_section(&mut self) {
        self.section("BLOCKS");
        let doc = self.doc;
        for index in 0..self.blocks.len() {
            let block = &self.blocks[index];
            let (name, handle, begin, end, base, flags, xref, paper) = (
                block.name.clone(),
                block.handle,
                block.begin,
                block.end,
                block.base,
                block.flags,
                block.xref.clone(),
                block.paper,
            );
            let key = upper(&name);
            self.out.tag(0, "BLOCK");
            self.out.handle(5, begin);
            self.out.handle(330, handle);
            self.out.tag(100, "AcDbEntity");
            if paper {
                self.out.tag(67, 1);
            }
            self.out.tag(8, "0");
            self.out.tag(100, "AcDbBlockBegin");
            self.out.str(2, &name);
            self.out.int(70, flags);
            self.out.point(10, &base);
            self.out.str(3, &name);
            self.out.str(1, xref.as_deref().unwrap_or(""));
            // モデル空間と、現在のレイアウト（*Paper_Space）の図形は ENTITIES 節に書く
            if key != MODEL && key != PAPER {
                for entity in doc.entities.iter().filter(|entity| Self::container(entity) == key) {
                    self.entity(entity, handle, paper);
                }
            }
            self.out.tag(0, "ENDBLK");
            self.out.handle(5, end);
            self.out.handle(330, handle);
            self.out.tag(100, "AcDbEntity");
            if paper {
                self.out.tag(67, 1);
            }
            self.out.tag(8, "0");
            self.out.tag(100, "AcDbBlockEnd");
        }
        self.out.tag(0, "ENDSEC");
    }

    fn entities_section(&mut self) {
        self.section("ENTITIES");
        let model = self.blocks[self.block_index[MODEL]].handle;
        let paper = self.blocks[self.block_index[PAPER]].handle;
        for entity in &self.doc.entities {
            match Self::container(entity).as_str() {
                MODEL => self.entity(entity, model, false),
                PAPER => self.entity(entity, paper, true),
                _ => {}
            }
        }
        self.out.tag(0, "ENDSEC");
    }

    fn dictionary(&mut self, handle: u64, owner: u64, entries: &[(String, u64)]) {
        self.out.tag(0, "DICTIONARY");
        self.out.handle(5, handle);
        self.out.handle(330, owner);
        self.out.tag(100, "AcDbDictionary");
        self.out.tag(281, 1);
        for (name, entry) in entries {
            self.out.str(3, name);
            self.out.handle(350, *entry);
        }
    }

    fn objects(&mut self, root: u64) {
        self.section("OBJECTS");
        let (group, layout, mline, plot, plotstyle) = (
            self.handles.fresh(),
            self.handles.fresh(),
            self.handles.fresh(),
            self.handles.fresh(),
            self.handles.fresh(),
        );
        self.dictionary(
            root,
            0,
            &[
                ("ACAD_GROUP".into(), group),
                ("ACAD_LAYOUT".into(), layout),
                ("ACAD_MLINESTYLE".into(), mline),
                ("ACAD_PLOTSETTINGS".into(), plot),
                ("ACAD_PLOTSTYLENAME".into(), plotstyle),
            ],
        );
        self.dictionary(group, root, &[]);
        let entries: Vec<(String, u64)> =
            self.layouts.iter().map(|(name, handle, _, _)| (name.clone(), *handle)).collect();
        self.dictionary(layout, root, &entries);
        let styles = self.mline_styles.clone();
        let entries: Vec<(String, u64)> = styles.iter().map(|(handle, style)| (style.name.clone(), *handle)).collect();
        self.dictionary(mline, root, &entries);
        self.dictionary(plot, root, &[]);
        // ACAD_PLOTSTYLENAME（既定の Normal を持つ辞書）と、レイヤが参照する Normal
        self.out.tag(0, "ACDBDICTIONARYWDFLT");
        self.out.handle(5, plotstyle);
        self.out.handle(330, root);
        self.out.tag(100, "AcDbDictionary");
        self.out.tag(281, 1);
        self.out.tag(3, "Normal");
        self.out.handle(350, self.placeholder);
        self.out.tag(100, "AcDbDictionaryWithDefault");
        self.out.handle(340, self.placeholder);
        self.out.tag(0, "ACDBPLACEHOLDER");
        self.out.handle(5, self.placeholder);
        self.out.handle(330, plotstyle);
        // LAYOUT
        let layouts = self.layouts.clone();
        for (tab, (name, handle, block, source)) in layouts.iter().enumerate() {
            let record = self.blocks[self.block_index[&upper(block)]].handle;
            self.layout(*handle, layout, record, name, tab, *source);
        }
        for (handle, style) in &styles {
            self.out.tag(0, "MLINESTYLE");
            self.out.handle(5, *handle);
            self.out.handle(330, mline);
            self.out.tag(100, "AcDbMlineStyle");
            self.out.str(2, &style.name);
            self.out.int(70, style.flags);
            self.out.str(3, &style.description);
            self.out.int(62, style.fill_color);
            self.out.real(51, style.start_angle);
            self.out.real(52, style.end_angle);
            self.out.tag(71, style.elements.len());
            for element in &style.elements {
                self.out.real(49, element.offset);
                self.out.int(62, element.color);
                self.out.str(6, if element.linetype.is_empty() { "BYLAYER" } else { &element.linetype });
            }
        }
        self.out.tag(0, "ENDSEC");
    }

    /// レイアウト（AcDbPlotSettings と AcDbLayout）
    fn layout(&mut self, handle: u64, owner: u64, record: u64, name: &str, tab: usize, source: Option<&DxfLayout>) {
        let out = &mut self.out;
        out.tag(0, "LAYOUT");
        out.handle(5, handle);
        out.handle(330, owner);
        out.tag(100, "AcDbPlotSettings");
        out.tag(1, "");
        out.str(4, source.map_or("A3", |layout| if layout.paper_size.is_empty() { "A3" } else { &layout.paper_size }));
        out.tag(6, "");
        let margins = source.map_or([7.5, 20.0, 7.5, 20.0], |layout| layout.margins);
        for (code, value) in [40, 41, 42, 43].into_iter().zip(margins) {
            out.real(code, value);
        }
        let (width, height) = source
            .filter(|layout| layout.paper_width > 0.0 && layout.paper_height > 0.0)
            .map_or((420.0, 297.0), |layout| (layout.paper_width, layout.paper_height));
        out.real(44, width);
        out.real(45, height);
        let origin = source.map_or([0.0, 0.0], |layout| layout.plot_origin);
        out.real(46, origin[0]);
        out.real(47, origin[1]);
        for code in [48, 49, 140, 141] {
            out.real(code, 0.0);
        }
        let scale = source.map_or([1.0, 1.0], |layout| layout.plot_scale);
        out.real(142, if scale[0] > 0.0 { scale[0] } else { 1.0 });
        out.real(143, if scale[1] > 0.0 { scale[1] } else { 1.0 });
        out.tag(70, if tab == 0 { 1024 } else { 0 });
        out.int(72, source.map_or(1.0, |layout| layout.paper_units));
        out.int(73, source.map_or(0.0, |layout| layout.plot_rotation));
        out.tag(74, 5);
        out.tag(7, "");
        out.tag(75, 16);
        out.tag(76, 0);
        out.tag(77, 2);
        out.tag(78, 300);
        out.real(147, 1.0);
        out.real(148, 0.0);
        out.real(149, 0.0);
        out.tag(100, "AcDbLayout");
        out.str(1, name);
        out.tag(70, 1);
        out.tag(71, tab);
        let (limmin, limmax) = source.map_or(([0.0, 0.0], [420.0, 297.0]), |layout| (layout.limmin, layout.limmax));
        out.point2(10, &limmin);
        out.point2(11, &limmax);
        out.point(12, &[0.0, 0.0, 0.0]);
        out.point(14, &[1e20, 1e20, 1e20]);
        out.point(15, &[-1e20, -1e20, -1e20]);
        out.real(146, 0.0);
        out.point(13, &[0.0, 0.0, 0.0]);
        out.point(16, &[1.0, 0.0, 0.0]);
        out.point(17, &[0.0, 1.0, 0.0]);
        out.tag(76, 1);
        out.handle(330, record);
    }

    fn skip(&mut self, kind: &str) {
        *self.skipped.entry(kind.to_string()).or_insert(0) += 1;
    }

    /// 図形の共通の値（DXF Reference「Common Group Codes for Entities」）
    fn common(&mut self, kind: &str, base: &DxfEntityBase, owner: u64, paper: bool) -> u64 {
        let handle = self.handles.keep(&base.handle);
        let out = &mut self.out;
        out.tag(0, kind);
        out.handle(5, handle);
        out.handle(330, owner);
        out.tag(100, "AcDbEntity");
        if paper {
            out.tag(67, 1);
        }
        out.str(8, if base.layer.is_empty() { "0" } else { &base.layer });
        if !base.linetype.is_empty() && upper(&base.linetype) != "BYLAYER" {
            out.str(6, &base.linetype);
        }
        if base.color_index != 256.0 {
            out.int(62, base.color_index);
        }
        if let Some(color) = base.true_color {
            out.int(420, color);
        }
        if let Some(transparency) = base.transparency {
            out.int(440, transparency);
        }
        if base.lineweight != -1.0 {
            out.int(370, base.lineweight);
        }
        if base.linetype_scale != 1.0 {
            out.real(48, base.linetype_scale);
        }
        if base.invisible {
            out.tag(60, 1);
        }
        handle
    }

    fn thickness(&mut self, base: &DxfEntityBase) {
        if let Some(thickness) = base.thickness.filter(|value| *value != 0.0) {
            self.out.real(39, thickness);
        }
    }

    fn extrusion(&mut self, base: &DxfEntityBase) {
        if let Some(extrusion) = base.extrusion.filter(|value| *value != [0.0, 0.0, 1.0]) {
            self.out.point(210, &extrusion);
        }
    }

    fn entity(&mut self, entity: &DxfEntity, owner: u64, paper: bool) {
        let base = &entity.base;
        let kind = upper(&base.kind);
        if UNWRITTEN.contains(&kind.as_str()) {
            self.skip(&kind);
            return;
        }
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                self.common("LINE", base, owner, paper);
                self.out.tag(100, "AcDbLine");
                self.thickness(base);
                self.out.point(10, start);
                self.out.point(11, end);
                self.extrusion(base);
            }
            DxfEntityData::Circle { center, radius } => {
                self.common("CIRCLE", base, owner, paper);
                self.out.tag(100, "AcDbCircle");
                self.thickness(base);
                self.out.point(10, center);
                self.out.real(40, *radius);
                self.extrusion(base);
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                self.common("ARC", base, owner, paper);
                self.out.tag(100, "AcDbCircle");
                self.thickness(base);
                self.out.point(10, center);
                self.out.real(40, *radius);
                self.extrusion(base);
                self.out.tag(100, "AcDbArc");
                self.out.real(50, *start_angle);
                self.out.real(51, *end_angle);
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                self.common("ELLIPSE", base, owner, paper);
                self.out.tag(100, "AcDbEllipse");
                self.out.point(10, center);
                self.out.point(11, major_axis);
                self.extrusion(base);
                self.out.real(40, *ratio);
                self.out.real(41, *start_param);
                self.out.real(42, *end_param);
            }
            DxfEntityData::Point { location, x_axis_angle } => {
                self.common("POINT", base, owner, paper);
                self.out.tag(100, "AcDbPoint");
                self.out.point(10, location);
                self.thickness(base);
                self.extrusion(base);
                if *x_axis_angle != 0.0 {
                    self.out.real(50, *x_axis_angle);
                }
            }
            DxfEntityData::LwPolyline(polyline) => {
                self.common("LWPOLYLINE", base, owner, paper);
                self.out.tag(100, "AcDbPolyline");
                let vertices = &polyline.vertices;
                self.out.tag(90, vertices.points.len());
                let flags = (js_int(vertices.flags) & !1) | i64::from(vertices.closed);
                self.out.tag(70, flags);
                if let Some(width) = polyline.const_width {
                    self.out.real(43, width);
                }
                if let Some(elevation) = polyline.elevation.filter(|value| *value != 0.0) {
                    self.out.real(38, elevation);
                }
                self.thickness(base);
                for (index, point) in vertices.points.iter().enumerate() {
                    self.out.point2(10, point);
                    if polyline.const_width.is_none()
                        && let Some(width) = vertices.widths.get(index).filter(|width| **width != [0.0, 0.0])
                    {
                        self.out.real(40, width[0]);
                        self.out.real(41, width[1]);
                    }
                    if let Some(bulge) = vertices.bulges.get(index).filter(|bulge| **bulge != 0.0) {
                        self.out.real(42, *bulge);
                    }
                }
                self.extrusion(base);
            }
            DxfEntityData::Polyline(polyline) => self.polyline(base, polyline, owner, paper),
            DxfEntityData::Spline(spline) => {
                if kind == "HELIX" {
                    self.skip("HELIX（SPLINE として書いた）");
                }
                self.common("SPLINE", base, owner, paper);
                self.out.tag(100, "AcDbSpline");
                self.extrusion(base);
                let flags = js_int(spline.flags)
                    | i64::from(spline.closed)
                    | (i64::from(spline.periodic) << 1)
                    | (i64::from(spline.rational) << 2);
                self.out.tag(70, flags);
                self.out.int(71, spline.degree);
                self.out.tag(72, spline.knots.len());
                self.out.tag(73, spline.control_points.len());
                self.out.tag(74, spline.fit_points.len());
                if let Some(tangent) = spline.start_tangent {
                    self.out.point(12, &tangent);
                }
                if let Some(tangent) = spline.end_tangent {
                    self.out.point(13, &tangent);
                }
                for knot in &spline.knots {
                    self.out.real(40, *knot);
                }
                if spline.rational {
                    for weight in &spline.weights {
                        self.out.real(41, *weight);
                    }
                }
                for point in &spline.control_points {
                    self.out.point(10, point);
                }
                for point in &spline.fit_points {
                    self.out.point(11, point);
                }
            }
            DxfEntityData::Text(text) => {
                self.common("TEXT", base, owner, paper);
                self.out.tag(100, "AcDbText");
                self.text_body(base, text);
                self.out.tag(100, "AcDbText");
                if text.valign != 0.0 {
                    self.out.int(73, text.valign);
                }
            }
            DxfEntityData::Attrib { text, tag, attribute_flags } => {
                self.common("ATTRIB", base, owner, paper);
                self.out.tag(100, "AcDbText");
                self.text_body(base, text);
                self.out.tag(100, "AcDbAttribute");
                self.out.tag(280, 0);
                self.out.str(2, tag);
                self.out.int(70, *attribute_flags);
                if text.valign != 0.0 {
                    self.out.int(74, text.valign);
                }
            }
            DxfEntityData::Attdef { text, tag, prompt, attribute_flags } => {
                self.common("ATTDEF", base, owner, paper);
                self.out.tag(100, "AcDbText");
                self.text_body(base, text);
                self.out.tag(100, "AcDbAttributeDefinition");
                self.out.tag(280, 0);
                self.out.str(3, prompt);
                self.out.str(2, tag);
                self.out.int(70, *attribute_flags);
                if text.valign != 0.0 {
                    self.out.int(74, text.valign);
                }
            }
            DxfEntityData::MText(mtext) => {
                self.common("MTEXT", base, owner, paper);
                self.out.tag(100, "AcDbMText");
                self.out.point(10, &mtext.insert);
                self.out.real(40, mtext.char_height);
                self.out.real(41, mtext.rect_width);
                self.out.int(71, if mtext.attachment_point >= 1.0 { mtext.attachment_point } else { 1.0 });
                self.out.int(72, if mtext.drawing_direction > 0.0 { mtext.drawing_direction } else { 1.0 });
                // 250 文字ごとに 3 に分け、最後を 1 に書く
                let raw =
                    if mtext.raw_text.is_empty() { mtext.text.replace('\n', "\\P") } else { mtext.raw_text.clone() };
                let chars: Vec<char> = raw.chars().map(|c| if c == '\r' || c == '\n' { ' ' } else { c }).collect();
                let mut chunks = chars.chunks(250).peekable();
                if chunks.peek().is_none() {
                    self.out.tag(1, "");
                }
                while let Some(chunk) = chunks.next() {
                    let text: String = chunk.iter().collect();
                    self.out.tag(if chunks.peek().is_some() { 3 } else { 1 }, text);
                }
                self.out.str(7, if mtext.style.is_empty() { "Standard" } else { &mtext.style });
                self.extrusion(base);
                match mtext.text_direction {
                    Some(direction) => self.out.point(11, &direction),
                    None if mtext.rotation != 0.0 => self.out.real(50, mtext.rotation),
                    None => {}
                }
                self.out.int(73, if mtext.line_spacing_style > 0.0 { mtext.line_spacing_style } else { 1.0 });
                self.out.real(44, if mtext.line_spacing_factor > 0.0 { mtext.line_spacing_factor } else { 1.0 });
                if mtext.background_flags != 0.0 {
                    self.out.int(90, mtext.background_flags);
                    if let Some(color) = mtext.background_color_index {
                        self.out.int(63, color);
                    }
                    if let Some(color) = mtext.background_true_color {
                        self.out.int(421, color);
                    }
                    self.out.real(45, mtext.background_scale.unwrap_or(1.5));
                    self.out.tag(441, 0);
                }
            }
            DxfEntityData::Insert(insert) => {
                let handle = self.common("INSERT", base, owner, paper);
                let array = insert.column_count > 1.0 || insert.row_count > 1.0;
                self.out.tag(100, if array { "AcDbMInsertBlock" } else { "AcDbBlockReference" });
                if !insert.attribs.is_empty() {
                    self.out.tag(66, 1);
                }
                self.out.str(2, &insert.name);
                self.out.point(10, &insert.insert);
                for (code, value) in [(41, insert.xscale), (42, insert.yscale), (43, insert.zscale)] {
                    if value != 1.0 {
                        self.out.real(code, value);
                    }
                }
                if insert.rotation != 0.0 {
                    self.out.real(50, insert.rotation);
                }
                if array {
                    self.out.int(70, insert.column_count.max(1.0));
                    self.out.int(71, insert.row_count.max(1.0));
                    self.out.real(44, insert.column_spacing);
                    self.out.real(45, insert.row_spacing);
                }
                self.extrusion(base);
                if !insert.attribs.is_empty() {
                    // 属性と SEQEND の所有者はブロックの配置
                    for attrib in &insert.attribs {
                        self.entity(attrib, handle, paper);
                    }
                    self.seqend(handle, handle, paper, base);
                }
            }
            DxfEntityData::Dimension(dimension) => self.dimension(&kind, base, dimension, owner, paper),
            DxfEntityData::Hatch(hatch) => {
                if kind == "MPOLYGON" {
                    self.skip("MPOLYGON（HATCH として書いた）");
                }
                self.hatch(base, hatch, owner, paper);
            }
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges } => {
                let name = match kind.as_str() {
                    "TRACE" => "TRACE",
                    "3DFACE" => "3DFACE",
                    _ => "SOLID",
                };
                self.common(name, base, owner, paper);
                self.out.tag(100, if name == "3DFACE" { "AcDbFace" } else { "AcDbTrace" });
                for (code, point) in [(10, vtx0), (11, vtx1), (12, vtx2), (13, vtx3)] {
                    self.out.point(code, point);
                }
                if name == "3DFACE" {
                    if let Some(flags) = invisible_edges {
                        self.out.int(70, *flags);
                    }
                } else {
                    self.thickness(base);
                    self.extrusion(base);
                }
            }
            DxfEntityData::Leader(leader) => {
                self.common("LEADER", base, owner, paper);
                self.out.tag(100, "AcDbLeader");
                self.out.str(3, if leader.dimstyle.is_empty() { "Standard" } else { &leader.dimstyle });
                self.out.tag(71, i32::from(leader.has_arrowhead));
                self.out.int(72, leader.path_type);
                self.out.int(73, leader.annotation_type);
                self.out.tag(74, 1);
                self.out.tag(75, 0);
                self.out.tag(76, leader.points.len());
                for point in &leader.points {
                    self.out.point(10, point);
                }
                self.extrusion(base);
            }
            DxfEntityData::MLeader(_) => self.skip(&kind),
            DxfEntityData::MLine(mline) => self.mline(base, mline, owner, paper),
            DxfEntityData::Viewport(viewport) => {
                self.common("VIEWPORT", base, owner, paper);
                self.out.tag(100, "AcDbViewport");
                self.out.point(10, &viewport.center);
                self.out.real(40, viewport.width);
                self.out.real(41, viewport.height);
                self.out.int(68, viewport.status);
                self.out.int(69, viewport.id);
                self.out.point2(12, &viewport.view_center);
                self.out.point2(13, &[0.0, 0.0]);
                self.out.point2(14, &[10.0, 10.0]);
                self.out.point2(15, &[10.0, 10.0]);
                self.out.point(16, &viewport.view_direction);
                self.out.point(17, &viewport.view_target);
                self.out.real(42, 50.0);
                self.out.real(43, 0.0);
                self.out.real(44, 0.0);
                self.out.real(45, viewport.view_height);
                self.out.real(50, 0.0);
                self.out.real(51, viewport.view_twist_angle);
                self.out.tag(72, 1000);
                for frozen in &viewport.frozen_layer_handles {
                    if let Ok(handle) = u64::from_str_radix(frozen, 16)
                        && self.layer_handles.values().any(|known| *known == handle)
                    {
                        self.out.handle(331, handle);
                    }
                }
                self.out.int(90, viewport.status_flags);
                self.out.tag(1, "");
                self.out.tag(281, 0);
                self.out.tag(71, 1);
                self.out.tag(74, 0);
                self.out.point(110, &[0.0, 0.0, 0.0]);
                self.out.point(111, &[1.0, 0.0, 0.0]);
                self.out.point(112, &[0.0, 1.0, 0.0]);
                self.out.tag(79, 0);
                self.out.real(146, 0.0);
                self.out.tag(170, 0);
            }
            DxfEntityData::XLine { start, unit_vector } => {
                let name = if kind == "RAY" { "RAY" } else { "XLINE" };
                self.common(name, base, owner, paper);
                self.out.tag(100, if name == "RAY" { "AcDbRay" } else { "AcDbXline" });
                self.out.point(10, start);
                self.out.point(11, unit_vector);
            }
            DxfEntityData::Image(_) => self.skip(&kind),
            DxfEntityData::Table(table) => {
                // 表（ACAD_TABLE）は、表示の図形を持つブロックの配置として書く
                self.skip("ACAD_TABLE（ブロックの配置として書いた）");
                self.common("INSERT", base, owner, paper);
                self.out.tag(100, "AcDbBlockReference");
                self.out.str(2, &table.name);
                self.out.point(10, &table.insert);
                for (code, value) in [(41, table.xscale), (42, table.yscale), (43, table.zscale)] {
                    if value != 1.0 {
                        self.out.real(code, value);
                    }
                }
                if table.rotation != 0.0 {
                    self.out.real(50, table.rotation);
                }
            }
        }
    }

    /// TEXT・ATTRIB・ATTDEF の 1 行の文字（AcDbText）
    fn text_body(&mut self, base: &DxfEntityBase, text: &DxfTextBase) {
        self.thickness(base);
        self.out.point(10, &text.insert);
        self.out.real(40, text.height);
        self.out.str(1, &text.text);
        if text.rotation != 0.0 {
            self.out.real(50, text.rotation);
        }
        if text.width != 1.0 && text.width > 0.0 {
            self.out.real(41, text.width);
        }
        if text.oblique != 0.0 {
            self.out.real(51, text.oblique);
        }
        self.out.str(7, if text.style.is_empty() { "Standard" } else { &text.style });
        if text.text_generation_flag != 0.0 {
            self.out.int(71, text.text_generation_flag);
        }
        if text.halign != 0.0 {
            self.out.int(72, text.halign);
        }
        if text.halign != 0.0 || text.valign != 0.0 {
            self.out.point(11, &text.align_point.unwrap_or(text.insert));
        }
        self.extrusion(base);
    }

    fn seqend(&mut self, _parent: u64, owner: u64, paper: bool, base: &DxfEntityBase) {
        let handle = self.handles.fresh();
        let out = &mut self.out;
        out.tag(0, "SEQEND");
        out.handle(5, handle);
        out.handle(330, owner);
        out.tag(100, "AcDbEntity");
        if paper {
            out.tag(67, 1);
        }
        out.str(8, if base.layer.is_empty() { "0" } else { &base.layer });
    }

    /// POLYLINE と VERTEX（2D・3D の線、ポリフェースメッシュ、ポリゴンメッシュ）
    fn polyline(&mut self, base: &DxfEntityBase, polyline: &DxfPolyline, owner: u64, paper: bool) {
        let vertices = &polyline.vertices;
        let flags = (js_int(vertices.flags) & !1) | i64::from(vertices.closed);
        let polyface = flags & 64 != 0;
        let mesh = flags & 16 != 0;
        let three_d = flags & 8 != 0;
        let handle = self.common("POLYLINE", base, owner, paper);
        self.out.tag(
            100,
            if polyface {
                "AcDbPolyFaceMesh"
            } else if mesh {
                "AcDbPolygonMesh"
            } else if three_d {
                "AcDb3dPolyline"
            } else {
                "AcDb2dPolyline"
            },
        );
        self.out.tag(66, 1);
        self.out.point(
            10,
            &[0.0, 0.0, if three_d || polyface || mesh { 0.0 } else { vertices.points.first().map_or(0.0, |p| p[2]) }],
        );
        self.thickness(base);
        self.out.tag(70, flags);
        if polyface {
            self.out.tag(71, vertices.points.len());
            self.out.tag(72, polyline.faces.as_ref().map_or(0, Vec::len));
        } else if mesh {
            self.out.int(71, polyline.m_count.unwrap_or(0.0));
            self.out.int(72, polyline.n_count.unwrap_or(0.0));
        }
        self.extrusion(base);
        let vertex_class = if polyface {
            "AcDbPolyFaceMeshVertex"
        } else if mesh {
            "AcDbPolygonMeshVertex"
        } else if three_d {
            "AcDb3dPolylineVertex"
        } else {
            "AcDb2dVertex"
        };
        for (index, point) in vertices.points.iter().enumerate() {
            let vertex = self.handles.fresh();
            let out = &mut self.out;
            out.tag(0, "VERTEX");
            out.handle(5, vertex);
            out.handle(330, handle);
            out.tag(100, "AcDbEntity");
            if paper {
                out.tag(67, 1);
            }
            out.str(8, if base.layer.is_empty() { "0" } else { &base.layer });
            out.tag(100, "AcDbVertex");
            out.tag(100, vertex_class);
            out.point(10, point);
            if !(polyface || mesh || three_d) {
                if let Some(width) = vertices.widths.get(index).filter(|width| **width != [0.0, 0.0]) {
                    out.real(40, width[0]);
                    out.real(41, width[1]);
                }
                if let Some(bulge) = vertices.bulges.get(index).filter(|bulge| **bulge != 0.0) {
                    out.real(42, *bulge);
                }
            }
            let default = if polyface {
                192.0
            } else if mesh {
                64.0
            } else if three_d {
                32.0
            } else {
                0.0
            };
            out.int(70, polyline.vertex_flags.get(index).copied().unwrap_or(default));
        }
        if let Some(faces) = polyline.faces.as_ref().filter(|_| polyface) {
            for face in faces {
                let vertex = self.handles.fresh();
                let out = &mut self.out;
                out.tag(0, "VERTEX");
                out.handle(5, vertex);
                out.handle(330, handle);
                out.tag(100, "AcDbEntity");
                if paper {
                    out.tag(67, 1);
                }
                out.str(8, if base.layer.is_empty() { "0" } else { &base.layer });
                out.tag(100, "AcDbVertex");
                out.tag(100, "AcDbFaceRecord");
                out.point(10, &[0.0, 0.0, 0.0]);
                out.tag(70, 128);
                for (code, index) in (71..75).zip(face) {
                    out.int(code, *index);
                }
            }
        }
        self.seqend(handle, handle, paper, base);
    }

    fn dimension(
        &mut self,
        kind: &str,
        base: &DxfEntityBase,
        dimension: &crate::model::DxfDimension,
        owner: u64,
        paper: bool,
    ) {
        let name = match kind {
            "ARC_DIMENSION" => "ARC_DIMENSION",
            "LARGE_RADIAL_DIMENSION" => "LARGE_RADIAL_DIMENSION",
            _ => "DIMENSION",
        };
        self.common(name, base, owner, paper);
        self.out.tag(100, "AcDbDimension");
        self.out.tag(280, 0);
        self.out.str(2, &dimension.anonymous_block_name);
        self.out.point(10, &dimension.defpoint);
        self.out.point(11, &dimension.text_midpoint);
        if let Some(insert) = dimension.insert {
            self.out.point(12, &insert);
        }
        self.out.int(70, dimension.dimtype);
        if let Some(attachment) = dimension.attachment_point {
            self.out.int(71, attachment);
        }
        self.out.str(1, &dimension.text);
        if let Some(measurement) = dimension.actual_measurement {
            self.out.real(42, measurement);
        }
        if let Some(rotation) = dimension.text_rotation {
            self.out.real(53, rotation);
        }
        if let Some(direction) = dimension.horizontal_direction {
            self.out.real(51, direction);
        }
        self.extrusion(base);
        self.out.str(3, if dimension.dimstyle.is_empty() { "Standard" } else { &dimension.dimstyle });
        let points = [dimension.defpoint2, dimension.defpoint3, dimension.defpoint4, dimension.defpoint5];
        let point = |index: usize| points[index].unwrap_or([0.0; 3]);
        match name {
            "ARC_DIMENSION" => {
                self.out.tag(100, "AcDbArcDimension");
                self.out.point(13, &point(0));
                self.out.point(14, &point(1));
                self.out.point(15, &point(2));
                self.out.tag(70, 0);
                self.out.real(40, 0.0);
                self.out.real(41, 0.0);
                self.out.tag(71, 0);
            }
            "LARGE_RADIAL_DIMENSION" => {
                self.out.tag(100, "AcDbRadialDimensionLarge");
                self.out.point(13, &point(0));
                self.out.point(14, &point(1));
                self.out.point(15, &point(2));
                self.out.real(40, 0.0);
            }
            _ => match js_int(dimension.dimtype) & 7 {
                0 | 1 => {
                    self.out.tag(100, "AcDbAlignedDimension");
                    self.out.point(13, &point(0));
                    self.out.point(14, &point(1));
                    if js_int(dimension.dimtype) & 7 == 0 {
                        self.out.real(50, dimension.angle.unwrap_or(0.0));
                        if let Some(oblique) = dimension.oblique_angle {
                            self.out.real(52, oblique);
                        }
                        self.out.tag(100, "AcDbRotatedDimension");
                    }
                }
                2 => {
                    self.out.tag(100, "AcDb2LineAngularDimension");
                    self.out.point(13, &point(0));
                    self.out.point(14, &point(1));
                    self.out.point(15, &point(2));
                    self.out.point(16, &point(3));
                }
                3 => {
                    self.out.tag(100, "AcDbDiametricDimension");
                    self.out.point(15, &point(2));
                    self.out.real(40, 0.0);
                }
                4 => {
                    self.out.tag(100, "AcDbRadialDimension");
                    self.out.point(15, &point(2));
                    self.out.real(40, 0.0);
                }
                5 => {
                    self.out.tag(100, "AcDb3PointAngularDimension");
                    self.out.point(13, &point(0));
                    self.out.point(14, &point(1));
                    self.out.point(15, &point(2));
                }
                _ => {
                    self.out.tag(100, "AcDbOrdinateDimension");
                    self.out.point(13, &point(0));
                    self.out.point(14, &point(1));
                }
            },
        }
    }

    fn hatch(&mut self, base: &DxfEntityBase, hatch: &DxfHatch, owner: u64, paper: bool) {
        self.common("HATCH", base, owner, paper);
        let out = &mut self.out;
        out.tag(100, "AcDbHatch");
        out.point(10, &[0.0, 0.0, hatch.elevation]);
        let extrusion = base.extrusion.unwrap_or([0.0, 0.0, 1.0]);
        out.point(210, &extrusion);
        let solid = hatch.solid_fill || hatch.gradient.is_some();
        out.str(
            2,
            if solid {
                "SOLID"
            } else if hatch.pattern_name.is_empty() {
                "ANSI31"
            } else {
                &hatch.pattern_name
            },
        );
        out.tag(70, i32::from(solid));
        out.tag(71, 0);
        out.tag(91, hatch.paths.len());
        for path in &hatch.paths {
            let flags = js_int(path.flags);
            match &path.edges {
                Some(edges) if flags & 2 == 0 => {
                    out.tag(92, flags);
                    out.tag(93, edges.len());
                    for edge in edges {
                        match edge {
                            DxfHatchEdge::Line { start, end } => {
                                out.tag(72, 1);
                                out.point2(10, start);
                                out.point2(11, end);
                            }
                            DxfHatchEdge::Arc { center, radius, start_angle, end_angle, ccw } => {
                                out.tag(72, 2);
                                out.point2(10, center);
                                out.real(40, *radius);
                                out.real(50, *start_angle);
                                out.real(51, *end_angle);
                                out.tag(73, i32::from(*ccw));
                            }
                            DxfHatchEdge::Ellipse { center, major_axis, ratio, start_angle, end_angle, ccw } => {
                                out.tag(72, 3);
                                out.point2(10, center);
                                out.point2(11, major_axis);
                                out.real(40, *ratio);
                                out.real(50, *start_angle);
                                out.real(51, *end_angle);
                                out.tag(73, i32::from(*ccw));
                            }
                            DxfHatchEdge::Spline {
                                degree,
                                rational,
                                periodic,
                                knots,
                                control_points,
                                weights,
                                fit_points,
                            } => {
                                out.tag(72, 4);
                                out.int(94, *degree);
                                out.tag(73, i32::from(*rational));
                                out.tag(74, i32::from(*periodic));
                                out.tag(95, knots.len());
                                out.tag(96, control_points.len());
                                for knot in knots {
                                    out.real(40, *knot);
                                }
                                for (index, point) in control_points.iter().enumerate() {
                                    out.point2(10, point);
                                    if *rational {
                                        out.real(42, weights.get(index).copied().unwrap_or(1.0));
                                    }
                                }
                                out.tag(97, fit_points.len());
                                for point in fit_points {
                                    out.point2(11, point);
                                }
                            }
                        }
                    }
                }
                _ => {
                    out.tag(92, flags | 2);
                    let bulges = path.bulges.as_ref().filter(|bulges| bulges.iter().any(|bulge| *bulge != 0.0));
                    out.tag(72, i32::from(bulges.is_some()));
                    out.tag(73, i32::from(path.closed));
                    out.tag(93, path.points.len());
                    for (index, point) in path.points.iter().enumerate() {
                        out.point2(10, point);
                        if let Some(bulges) = bulges {
                            out.real(42, bulges.get(index).copied().unwrap_or(0.0));
                        }
                    }
                }
            }
            out.tag(97, 0);
        }
        out.int(75, hatch.hatch_style);
        out.int(76, if hatch.pattern_type > 0.0 { hatch.pattern_type } else { 1.0 });
        if !solid {
            out.real(52, hatch.pattern_angle);
            out.real(41, if hatch.pattern_scale != 0.0 { hatch.pattern_scale } else { 1.0 });
            out.tag(77, i32::from(hatch.pattern_double));
            out.tag(78, hatch.pattern_lines.len());
            for line in &hatch.pattern_lines {
                out.real(53, line.angle);
                out.real(43, line.base[0]);
                out.real(44, line.base[1]);
                out.real(45, line.offset[0]);
                out.real(46, line.offset[1]);
                out.tag(79, line.dashes.len());
                for dash in &line.dashes {
                    out.real(49, *dash);
                }
            }
        }
        out.tag(98, 0);
        if hatch.gradient.is_some() {
            *self.skipped.entry("グラデーションのハッチング（単色で書いた）".into()).or_insert(0) += 1;
        }
    }

    fn mline(&mut self, base: &DxfEntityBase, mline: &DxfMLine, owner: u64, paper: bool) {
        // スタイルは名前で探し、なければ Standard
        let style =
            self.mline_style_handle(&mline.style_name).or_else(|| self.mline_style_handle("Standard")).unwrap_or(0);
        self.common("MLINE", base, owner, paper);
        let out = &mut self.out;
        out.tag(100, "AcDbMline");
        out.str(2, &mline.style_name);
        out.handle(340, style);
        out.real(40, mline.scale);
        out.int(70, mline.justification);
        let flags = (js_int(mline.flags) & !2) | (i64::from(mline.closed) << 1);
        out.tag(71, flags);
        out.tag(72, mline.points.len());
        out.int(73, mline.line_count);
        out.point(10, &mline.base_point);
        out.point(210, &base.extrusion.unwrap_or([0.0, 0.0, 1.0]));
        for (index, point) in mline.points.iter().enumerate() {
            out.point(11, point);
            out.point(12, &mline.vertex_directions.get(index).copied().unwrap_or([1.0, 0.0, 0.0]));
            out.point(13, &mline.miter_directions.get(index).copied().unwrap_or([0.0, 1.0, 0.0]));
            let lines = mline.line_params.as_ref().and_then(|params| params.get(index));
            let fills = mline.fill_params.as_ref().and_then(|params| params.get(index));
            for element in 0..js_int(mline.line_count).max(0) as usize {
                let params = lines.and_then(|lines| lines.get(element)).cloned().unwrap_or_default();
                out.tag(74, params.len());
                for value in &params {
                    out.real(41, *value);
                }
                let fill = fills.and_then(|fills| fills.get(element)).cloned().unwrap_or_default();
                out.tag(75, fill.len());
                for value in &fill {
                    out.real(42, *value);
                }
            }
        }
    }

    /// MLINESTYLE のハンドル（plan で決めたもの）
    fn mline_style_handle(&self, name: &str) -> Option<u64> {
        self.mline_styles.iter().find(|(_, style)| upper(&style.name) == upper(name)).map(|(handle, _)| *handle)
    }
}

/// JavaScript の ToInt32 と同じく、数を 32 ビットの整数にする
fn js_int(value: f64) -> i64 {
    i64::from(zumen_core::js::to_int32(value))
}
