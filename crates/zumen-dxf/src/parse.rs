//! DXF のタグを AutoCAD の図面のモデルにする（`@zumenjs/dxf` の parse.ts と同じ順で読む）

use crate::model::*;
use crate::mtext::mtext_plain_text;
use crate::tags::{TagValue, decode_escapes, read_tags};
use std::borrow::Cow;
use std::collections::HashMap;
use zumen_core::{NoticeLevel, NoticeList, js};

/// 図形のタグのうち、102 の組・拡張データ（1001 以降）・埋め込みの図形（101 以降）を除いた位置
type Body = Vec<usize>;

#[derive(Debug, Clone)]
struct Owner {
    space: DxfSpace,
    block: Option<String>,
}

const MODEL_SPACE: &str = "*MODEL_SPACE";
const PAPER_SPACE: &str = "*PAPER_SPACE";

/// `name.toUpperCase() === value`（value は ASCII の大文字）。ASCII の文字列は大文字に直さずに比べる
fn upper_is(name: &str, value: &str) -> bool {
    if name.is_ascii() { name.eq_ignore_ascii_case(value) } else { name.to_uppercase() == value }
}

pub(crate) fn is_model_block(name: &str) -> bool {
    upper_is(name, MODEL_SPACE) || upper_is(name, "$MODEL_SPACE")
}

pub(crate) fn is_paper_block(name: &str) -> bool {
    let upper = name.to_uppercase();
    upper.starts_with(PAPER_SPACE) || upper == "$PAPER_SPACE"
}

/// 図形の種類ごとに読む図形。ここにない種類は件数だけを skipped に数える
const READ_TYPES: [&str; 32] = [
    "LINE",
    "POINT",
    "CIRCLE",
    "ARC",
    "ELLIPSE",
    "LWPOLYLINE",
    "POLYLINE",
    "SPLINE",
    "HELIX",
    "TEXT",
    "ATTRIB",
    "ATTDEF",
    "MTEXT",
    "INSERT",
    "DIMENSION",
    "ARC_DIMENSION",
    "LARGE_RADIAL_DIMENSION",
    "HATCH",
    "MPOLYGON",
    "SOLID",
    "TRACE",
    "3DFACE",
    "LEADER",
    "MULTILEADER",
    "MLEADER",
    "MLINE",
    "VIEWPORT",
    "XLINE",
    "RAY",
    "IMAGE",
    "WIPEOUT",
    "ACAD_TABLE",
];

/// MULTILEADER の色の生の値の BYBLOCK（`0xc1000000 | 0`。上位 1 バイトが種類: 0xC0 BYLAYER、0xC1 BYBLOCK、0xC2 RGB、0xC3 ACI）
pub(crate) const BY_BLOCK_RAW_COLOR: f64 = -1_056_964_608.0;

/// MULTILEADER・MLINE で、Y・Z（+10・+20）が続く点の X のグループコード
const MLEADER_POINT_CODES: [i32; 10] = [10, 11, 12, 13, 14, 15, 16, 110, 111, 112];
const MLINE_POINT_CODES: [i32; 4] = [10, 11, 12, 13];

/// グループコード（範囲の外は、どのコードとも等しくない値）
fn code_at(codes: &[i32], k: usize) -> i64 {
    codes.get(k).map_or(i64::MIN, |code| i64::from(*code))
}

/// HATCH の値を決まった順に読む位置
struct Cursor<'p> {
    own: &'p [usize],
    codes: &'p [i32],
    k: usize,
}

impl Cursor<'_> {
    fn more(&self) -> bool {
        self.k < self.own.len()
    }

    /// 今の位置のグループコード。終わりなら -1
    fn peek(&self) -> i64 {
        if self.more() { code_at(self.codes, self.own[self.k]) } else { -1 }
    }

    fn take(&mut self) -> usize {
        let index = self.own.get(self.k).copied().unwrap_or(usize::MAX);
        self.k += 1;
        index
    }
}

struct Parser<'a> {
    codes: &'a [i32],
    values: &'a [TagValue],
    length: usize,
    i: usize,
    doc: DxfDocument,
    notices: NoticeList,
    /// BLOCK_RECORD のハンドル → ブロックの名前
    block_records: HashMap<String, String>,
    /// LAYOUT の BLOCK_RECORD のハンドルとレイアウト
    layout_records: Vec<(String, DxfLayout)>,
    sort_tables: Vec<(String, HashMap<String, String>)>,
    has_paper_entities: bool,
}

impl<'a> Parser<'a> {
    fn code(&self, k: usize) -> i64 {
        code_at(self.codes, k)
    }

    /// 数の値。文字列は `Number(value) || 0`
    fn num(&self, k: usize) -> f64 {
        match self.values.get(k) {
            Some(TagValue::Num(value)) => *value,
            Some(TagValue::Str(text)) => {
                let value = js::string_to_number(text);
                if value == 0.0 || value.is_nan() { 0.0 } else { value }
            }
            None => 0.0,
        }
    }

    fn raw(&self, k: usize) -> Cow<'a, str> {
        match self.values.get(k) {
            Some(TagValue::Str(text)) => Cow::Borrowed(text.as_str()),
            Some(TagValue::Num(value)) => Cow::Owned(js::number_to_string(*value)),
            None => Cow::Borrowed("undefined"),
        }
    }

    /// 文字列の値。\U+XXXX・\M+nXXXX を文字にする
    fn str(&self, k: usize) -> String {
        decode_escapes(&self.raw(k))
    }

    /// `raw(k).trim().toUpperCase()`
    fn upper(&self, k: usize) -> String {
        js::trim(&self.raw(k)).to_uppercase()
    }

    fn trimmed(&self, k: usize) -> String {
        js::trim(&self.raw(k)).to_string()
    }

    fn is(&self, k: usize, code: i64, value: &str) -> bool {
        k < self.length && self.code(k) == code && upper_is(js::trim(&self.raw(k)), value)
    }

    /// k より後の、グループコード 0 の位置
    fn next_zero(&self, k: usize) -> usize {
        let mut j = k + 1;
        while j < self.length && self.code(j) != 0 {
            j += 1;
        }
        j
    }

    fn parse(mut self) -> DxfDocument {
        let mut sections = 0;
        while self.i < self.length {
            if self.is(self.i, 0, "EOF") {
                break;
            }
            if !self.is(self.i, 0, "SECTION") {
                self.i += 1;
                continue;
            }
            sections += 1;
            let name = if self.i + 1 < self.length && self.code(self.i + 1) == 2 {
                self.trimmed(self.i + 1)
            } else {
                String::new()
            };
            self.i += 2;
            match name.to_uppercase().as_str() {
                "HEADER" => self.header(),
                "TABLES" => self.tables(),
                "BLOCKS" => self.blocks(),
                "ENTITIES" => self.entities_section(),
                "OBJECTS" => self.objects(),
                _ => self.skip_section(),
            }
        }
        if sections == 0 {
            self.notices.add(NoticeLevel::Warning, "no-section", "DXF のセクションが見つかりませんでした");
        }
        self.finish();
        self.doc.notices = std::mem::take(&mut self.notices).into_vec();
        self.doc
    }

    fn skip_section(&mut self) {
        while self.i < self.length && !self.is(self.i, 0, "ENDSEC") {
            if self.is(self.i, 0, "SECTION") || self.is(self.i, 0, "EOF") {
                return;
            }
            self.i += 1;
        }
        self.i += 1;
    }

    // --- HEADER ---------------------------------------------------------------

    fn header(&mut self) {
        let mut variables: Vec<(String, DxfHeaderValue)> = std::mem::take(&mut self.doc.header.variables);
        let set = |variables: &mut Vec<(String, DxfHeaderValue)>, name: String, value: DxfHeaderValue| match variables
            .iter_mut()
            .find(|(key, _)| *key == name)
        {
            Some(entry) => entry.1 = value,
            None => variables.push((name, value)),
        };
        while self.i < self.length && !self.is(self.i, 0, "ENDSEC") {
            if self.code(self.i) != 9 {
                if self.is(self.i, 0, "SECTION") {
                    self.doc.header.variables = variables;
                    return;
                }
                self.i += 1;
                continue;
            }
            let name = self.trimmed(self.i);
            self.i += 1;
            let first = self.i;
            while self.i < self.length && self.code(self.i) != 9 && self.code(self.i) != 0 {
                self.i += 1;
            }
            let count = self.i - first;
            if count == 0 {
                continue;
            }
            let first_code = self.code(first);
            let value = if (10..=39).contains(&first_code) && count > 1 {
                DxfHeaderValue::Point(
                    (first..self.i)
                        .map(|k| match &self.values[k] {
                            TagValue::Num(value) => *value,
                            TagValue::Str(text) => js::string_to_number(text),
                        })
                        .collect(),
                )
            } else {
                match &self.values[first] {
                    TagValue::Str(text) => DxfHeaderValue::Text(decode_escapes(text)),
                    TagValue::Num(value) => DxfHeaderValue::Number(*value),
                }
            };
            set(&mut variables, name, value);
        }
        self.i += 1;
        let number = |name: &str, fallback: f64| match variables.iter().find(|(key, _)| key == name) {
            Some((_, DxfHeaderValue::Number(value))) if value.is_finite() => *value,
            _ => fallback,
        };
        let point = |name: &str| match variables.iter().find(|(key, _)| key == name) {
            Some((_, DxfHeaderValue::Point(values))) => {
                Some([0, 1, 2].map(|index| values.get(index).copied().unwrap_or(0.0)))
            }
            _ => None,
        };
        let raw_number = |name: &str| match variables.iter().find(|(key, _)| key == name) {
            Some((_, DxfHeaderValue::Number(value))) => Some(*value),
            _ => None,
        };
        let header = &mut self.doc.header;
        header.insunits = number("$INSUNITS", 0.0);
        if let Some(measurement) = raw_number("$MEASUREMENT") {
            header.measurement = Some(measurement);
        }
        header.ltscale = number("$LTSCALE", 1.0);
        header.psltscale = number("$PSLTSCALE", 1.0);
        if let Some(textsize) = raw_number("$TEXTSIZE") {
            header.textsize = Some(textsize);
        }
        header.dimscale = number("$DIMSCALE", 1.0);
        let metric = header.measurement == Some(1.0);
        header.dimasz = number("$DIMASZ", if metric { 2.5 } else { 0.18 });
        header.dimtxt = number("$DIMTXT", if metric { 2.5 } else { 0.18 });
        header.tilemode = number("$TILEMODE", 1.0);
        header.extmin = point("$EXTMIN");
        header.extmax = point("$EXTMAX");
        header.limmin = point("$LIMMIN").map(|[x, y, _]| [x, y]);
        header.limmax = point("$LIMMAX").map(|[x, y, _]| [x, y]);
        header.plimmin = point("$PLIMMIN").map(|[x, y, _]| [x, y]);
        header.plimmax = point("$PLIMMAX").map(|[x, y, _]| [x, y]);
        header.variables = variables;
    }

    // --- TABLES ---------------------------------------------------------------

    fn tables(&mut self) {
        while self.i < self.length && !self.is(self.i, 0, "ENDSEC") {
            if self.is(self.i, 0, "SECTION") || self.is(self.i, 0, "EOF") {
                return;
            }
            if self.code(self.i) != 0 {
                self.i += 1;
                continue;
            }
            let kind = self.upper(self.i);
            let end = self.next_zero(self.i);
            let body = self.body(self.i + 1, end);
            match kind.as_str() {
                "LAYER" => {
                    let layer = self.layer(&body, self.i + 1, end);
                    self.doc.layers.push(layer);
                }
                "LTYPE" => {
                    let linetype = self.linetype(&body);
                    self.doc.linetypes.push(linetype);
                }
                "STYLE" => {
                    let style = self.style(self.i + 1, end, &body);
                    self.doc.styles.push(style);
                }
                "DIMSTYLE" => {
                    let style = self.dimstyle(self.i + 1, end);
                    self.doc.dimstyles.push(style);
                }
                "BLOCK_RECORD" => {
                    let mut handle = String::new();
                    let mut name = String::new();
                    for &k in &body {
                        match self.code(k) {
                            5 => handle = self.upper(k),
                            2 => name = self.str(k),
                            _ => {}
                        }
                    }
                    if !handle.is_empty() && !name.is_empty() {
                        self.block_records.insert(handle, name);
                    }
                }
                _ => {}
            }
            self.i = end;
        }
        self.i += 1;
    }

    fn handle_of(&self, body: &[usize], code: i64) -> String {
        body.iter().find(|&&k| self.code(k) == code).map(|&k| self.upper(k)).unwrap_or_default()
    }

    fn layer(&self, body: &[usize], start: usize, end: usize) -> DxfLayer {
        let mut layer = DxfLayer {
            handle: self.handle_of(body, 5),
            name: String::new(),
            color_index: 7.0,
            true_color: None,
            frozen: false,
            off: false,
            locked: false,
            plot: true,
            linetype: "CONTINUOUS".into(),
            lineweight: -3.0,
            transparency: None,
        };
        for &k in body {
            match self.code(k) {
                2 => layer.name = self.str(k),
                70 => {
                    let flags = js::to_int32(self.num(k));
                    layer.frozen = flags & 1 != 0;
                    layer.locked = flags & 4 != 0;
                }
                62 => {
                    // 色が負のレイヤは非表示（DXF Reference「LAYER」）
                    let color = self.num(k);
                    layer.off = color < 0.0;
                    layer.color_index = or(color.abs(), 7.0);
                }
                420 => layer.true_color = Some(f64::from(js::to_int32(self.num(k)) & 0xff_ffff)),
                6 => layer.linetype = self.str(k),
                290 => layer.plot = self.num(k) != 0.0,
                370 => layer.lineweight = self.num(k),
                _ => {}
            }
        }
        // レイヤの透過度は拡張データ AcCmTransparency の 1071 に入る
        for k in start..end {
            if self.code(k) == 1001 && js::trim(&self.raw(k)) == "AcCmTransparency" {
                if k + 1 < end && self.code(k + 1) == 1071 {
                    layer.transparency = Some(self.num(k + 1));
                }
                break;
            }
        }
        layer
    }

    fn linetype(&self, body: &[usize]) -> DxfLinetype {
        let mut linetype = DxfLinetype {
            handle: self.handle_of(body, 5),
            name: String::new(),
            description: String::new(),
            pattern_length: 0.0,
            dashes: Vec::new(),
            complex: false,
        };
        for &k in body {
            match self.code(k) {
                2 => linetype.name = self.str(k),
                3 => linetype.description = self.str(k),
                40 => linetype.pattern_length = self.num(k),
                49 => linetype.dashes.push(self.num(k)),
                74 if self.num(k) != 0.0 => linetype.complex = true,
                _ => {}
            }
        }
        linetype
    }

    fn style(&self, start: usize, end: usize, body: &[usize]) -> DxfTextStyle {
        let mut style = DxfTextStyle {
            handle: self.handle_of(body, 5),
            name: String::new(),
            font: String::new(),
            bigfont: String::new(),
            height: 0.0,
            width: 1.0,
            oblique: 0.0,
            flags: 0.0,
            generation_flags: 0.0,
            font_family: None,
        };
        for &k in body {
            match self.code(k) {
                2 => style.name = self.str(k),
                3 => style.font = self.str(k),
                4 => style.bigfont = self.str(k),
                40 => style.height = self.num(k),
                41 => style.width = or(self.num(k), 1.0),
                50 => style.oblique = self.num(k),
                70 => style.flags = self.num(k),
                71 => style.generation_flags = self.num(k),
                _ => {}
            }
        }
        // TrueType のフォント名は拡張データ ACAD の 1000 に入る
        for k in start..end {
            if self.code(k) == 1001 && upper_is(js::trim(&self.raw(k)), "ACAD") {
                if k + 1 < end && self.code(k + 1) == 1000 {
                    style.font_family = Some(self.str(k + 1));
                }
                break;
            }
        }
        style
    }

    fn dimstyle(&self, start: usize, end: usize) -> DxfDimStyle {
        // AutoCAD の DXF（R2000 以降）は既定値と同じ値を書かない。既定値は $MEASUREMENT によらず AutoCAD の寸法スタイルの
        // 初期値（DIMSCALE 1、DIMASZ・DIMTXT 0.18）で、同じ図面の DWG はその値を持つ（ACadSharp・netDxf の読み取りも同じ）
        let mut style =
            DxfDimStyle { handle: String::new(), name: String::new(), dimscale: 1.0, dimasz: 0.18, dimtxt: 0.18 };
        for k in start..end {
            match self.code(k) {
                105 => style.handle = self.upper(k),
                2 => style.name = self.str(k),
                40 => style.dimscale = self.num(k),
                41 => style.dimasz = self.num(k),
                140 => style.dimtxt = self.num(k),
                _ => {}
            }
        }
        style
    }

    // --- BLOCKS ---------------------------------------------------------------

    fn blocks(&mut self) {
        while self.i < self.length && !self.is(self.i, 0, "ENDSEC") {
            if self.is(self.i, 0, "SECTION") || self.is(self.i, 0, "EOF") {
                return;
            }
            if !self.is(self.i, 0, "BLOCK") {
                self.i += 1;
                continue;
            }
            let end = self.next_zero(self.i);
            let body = self.body(self.i + 1, end);
            let mut block = DxfBlock {
                handle: self.handle_of(&body, 5),
                name: String::new(),
                base_point: [0.0, 0.0, 0.0],
                flags: 0.0,
                anonymous: false,
                layout: false,
                block_record_handle: None,
                xref_path: None,
                description: None,
            };
            for &k in &body {
                match self.code(k) {
                    2 => block.name = self.str(k),
                    70 => block.flags = self.num(k),
                    10 => block.base_point[0] = self.num(k),
                    20 => block.base_point[1] = self.num(k),
                    30 => block.base_point[2] = self.num(k),
                    1 => {
                        if !self.raw(k).is_empty() {
                            block.xref_path = Some(self.str(k));
                        }
                    }
                    4 => {
                        if !self.raw(k).is_empty() {
                            block.description = Some(self.str(k));
                        }
                    }
                    330 => block.block_record_handle = Some(self.upper(k)),
                    _ => {}
                }
            }
            if block.name.is_empty() {
                for &k in &body {
                    if self.code(k) == 3 {
                        block.name = self.str(k);
                    }
                }
            }
            block.anonymous = js::to_int32(block.flags) & 1 != 0 || block.name.starts_with('*');
            block.layout = is_model_block(&block.name) || is_paper_block(&block.name);
            let owner = if is_model_block(&block.name) {
                Owner { space: DxfSpace::Model, block: None }
            } else {
                let space = if is_paper_block(&block.name) { DxfSpace::Paper } else { DxfSpace::Block };
                Owner { space, block: Some(block.name.clone()) }
            };
            self.doc.blocks.push(block);
            self.i = end;
            self.entity_list(Some(&owner), "ENDBLK");
            if self.is(self.i, 0, "ENDBLK") {
                self.i = self.next_zero(self.i);
            }
        }
        self.i += 1;
    }

    // --- ENTITIES -------------------------------------------------------------

    fn entities_section(&mut self) {
        self.entity_list(None, "ENDSEC");
        self.i += 1;
    }

    /// 図形を順に読む。owner が None なら ENTITIES セクション（67 と 330 でモデル・紙空間を決める）
    fn entity_list(&mut self, owner: Option<&Owner>, terminator: &str) {
        while self.i < self.length {
            if self.code(self.i) != 0 {
                self.i += 1;
                continue;
            }
            let kind = self.upper(self.i);
            if kind == terminator || kind == "ENDSEC" || kind == "EOF" || kind == "SECTION" {
                return;
            }
            let start = self.i;
            let end = self.next_zero(start);
            self.i = end;
            if kind == "SEQEND" || kind == "VERTEX" || kind == "ATTRIB" {
                // POLYLINE・INSERT に続かない頂点・属性
                if kind == "ATTRIB" {
                    let attribute = self.attribute(start, end, "ATTRIB", owner);
                    self.push(Some(attribute));
                }
                continue;
            }
            if !READ_TYPES.contains(&kind.as_str()) {
                match self.doc.skipped.iter_mut().find(|(name, _)| *name == kind) {
                    Some(entry) => entry.1 += 1,
                    None => self.doc.skipped.push((kind, 1)),
                }
                continue;
            }
            let entity = self.entity(&kind, start, end, owner);
            self.push(entity);
        }
    }

    fn push(&mut self, entity: Option<DxfEntity>) {
        let Some(entity) = entity else {
            return;
        };
        if entity.base.space == DxfSpace::Paper {
            self.has_paper_entities = true;
        }
        self.doc.entities.push(entity);
    }

    /// 102 の組（{ACAD_REACTORS など）、拡張データ、埋め込みの図形を除いたタグの位置
    fn body(&self, start: usize, end: usize) -> Body {
        let mut out = Vec::new();
        let mut k = start;
        while k < end {
            let code = self.code(k);
            if code == 102 {
                if js::trim(&self.raw(k)).starts_with('{') {
                    while k + 1 < end && !(self.code(k + 1) == 102 && js::trim(&self.raw(k + 1)) == "}") {
                        k += 1;
                    }
                    k += 1;
                }
                k += 1;
                continue;
            }
            if code >= 1000 || code == 101 {
                break;
            }
            out.push(k);
            k += 1;
        }
        out
    }

    /// 共通の属性。R13 以降は AcDbEntity の中の値だけを使う（ACAD_TABLE のセルの色などと取り違えない）。
    /// 厚さ（39）と押し出し方向（210）は図形の種類の側にあるので、全体から読む
    fn base(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntityBase {
        let mut entity = DxfEntityBase {
            kind: kind.to_string(),
            handle: String::new(),
            owner: None,
            space: owner.map_or(DxfSpace::Model, |owner| owner.space),
            block: None,
            layer: "0".into(),
            linetype: "BYLAYER".into(),
            color_index: 256.0,
            true_color: None,
            transparency: None,
            lineweight: -1.0,
            linetype_scale: 1.0,
            invisible: false,
            thickness: None,
            extrusion: None,
        };
        if let Some(block) = owner.and_then(|owner| owner.block.as_ref()).filter(|block| !block.is_empty()) {
            entity.block = Some(block.clone());
        }
        let mut subclass = String::new();
        let mut marked = false;
        let mut paper = false;
        let mut extrusion: Option<Vec3> = None;
        for &k in body {
            let code = self.code(k);
            if code == 100 {
                subclass = self.trimmed(k);
                marked = true;
                continue;
            }
            if code == 5 && entity.handle.is_empty() {
                entity.handle = self.upper(k);
                continue;
            }
            if code == 330 && entity.owner.is_none() && !marked {
                entity.owner = Some(self.upper(k));
                continue;
            }
            if code == 39 {
                entity.thickness = Some(self.num(k));
                continue;
            }
            if code == 210 || code == 220 || code == 230 {
                let value = extrusion.get_or_insert([0.0, 0.0, 1.0]);
                value[if code == 210 {
                    0
                } else if code == 220 {
                    1
                } else {
                    2
                }] = self.num(k);
                continue;
            }
            if marked && subclass != "AcDbEntity" {
                continue;
            }
            match code {
                8 => entity.layer = self.str(k),
                6 => entity.linetype = self.str(k),
                62 => entity.color_index = self.num(k),
                420 => entity.true_color = Some(f64::from(js::to_int32(self.num(k)) & 0xff_ffff)),
                440 => entity.transparency = Some(self.num(k)),
                370 => entity.lineweight = self.num(k),
                48 => entity.linetype_scale = or(self.num(k), 1.0),
                60 => entity.invisible = self.num(k) != 0.0,
                67 => paper = self.num(k) == 1.0,
                _ => {}
            }
        }
        if let Some(value) = extrusion
            && !(value[0] == 0.0 && value[1] == 0.0 && value[2] == 1.0)
        {
            entity.extrusion = Some(value);
        }
        if owner.is_none() {
            // ENTITIES セクションはモデル空間と、現在のレイアウトの紙空間（*Paper_Space）を持つ
            let owner_name = entity
                .owner
                .as_ref()
                .filter(|owner| !owner.is_empty())
                .and_then(|owner| self.block_records.get(owner))
                .filter(|name| !name.is_empty());
            let paper_owner = owner_name.filter(|name| is_paper_block(name));
            if paper || paper_owner.is_some() {
                entity.space = DxfSpace::Paper;
                entity.block = Some(paper_owner.cloned().unwrap_or_else(|| "*Paper_Space".into()));
            }
        }
        entity
    }

    fn point(&self, body: &[usize], x: i64) -> Vec3 {
        let mut out = [0.0, 0.0, 0.0];
        for &k in body {
            let code = self.code(k);
            if code == x {
                out[0] = self.num(k);
            } else if code == x + 10 {
                out[1] = self.num(k);
            } else if code == x + 20 {
                out[2] = self.num(k);
            }
        }
        out
    }

    fn has(&self, body: &[usize], code: i64) -> bool {
        body.iter().any(|&k| self.code(k) == code)
    }

    fn number(&self, body: &[usize], code: i64, fallback: f64) -> f64 {
        body.iter().find(|&&k| self.code(k) == code).map_or(fallback, |&k| self.num(k))
    }

    fn text(&self, body: &[usize], code: i64, fallback: &str) -> String {
        body.iter().find(|&&k| self.code(k) == code).map_or_else(|| fallback.to_string(), |&k| self.str(k))
    }

    fn entity(&mut self, kind: &str, start: usize, end: usize, owner: Option<&Owner>) -> Option<DxfEntity> {
        let body = self.body(start + 1, end);
        let entity = |base: DxfEntityBase, data: DxfEntityData| Some(DxfEntity { base, data });
        match kind {
            "LINE" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::Line { start: self.point(&body, 10), end: self.point(&body, 11) },
            ),
            "POINT" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::Point { location: self.point(&body, 10), x_axis_angle: self.number(&body, 50, 0.0) },
            ),
            "CIRCLE" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::Circle { center: self.point(&body, 10), radius: self.number(&body, 40, 0.0) },
            ),
            "ARC" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::Arc {
                    center: self.point(&body, 10),
                    radius: self.number(&body, 40, 0.0),
                    start_angle: self.number(&body, 50, 0.0),
                    end_angle: self.number(&body, 51, 360.0),
                },
            ),
            "ELLIPSE" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::Ellipse {
                    center: self.point(&body, 10),
                    major_axis: self.point(&body, 11),
                    ratio: self.number(&body, 40, 1.0),
                    start_param: self.number(&body, 41, 0.0),
                    end_param: self.number(&body, 42, std::f64::consts::PI * 2.0),
                },
            ),
            "LWPOLYLINE" => Some(self.lwpolyline(&body, owner)),
            "POLYLINE" => Some(self.polyline(&body, owner)),
            "SPLINE" | "HELIX" => Some(self.spline(kind, &body, owner)),
            "TEXT" => entity(self.base(kind, &body, owner), DxfEntityData::Text(self.text_base(&body, 73))),
            "ATTDEF" => Some(self.attribute(start, end, "ATTDEF", owner)),
            "MTEXT" => Some(self.mtext(&body, owner)),
            "INSERT" => Some(self.insert(&body, owner)),
            "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION" => Some(self.dimension(kind, &body, owner)),
            "HATCH" | "MPOLYGON" => Some(self.hatch(kind, &body, owner)),
            "SOLID" | "TRACE" | "3DFACE" => {
                let vtx2 = self.point(&body, 12);
                let base = self.base(kind, &body, owner);
                let vtx0 = self.point(&body, 10);
                let vtx1 = self.point(&body, 11);
                let vtx3 = if self.has(&body, 13) { self.point(&body, 13) } else { vtx2 };
                let invisible_edges = (kind == "3DFACE").then(|| self.number(&body, 70, 0.0));
                entity(base, DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges })
            }
            "LEADER" => {
                let mut points: Vec<Vec3> = Vec::new();
                for &k in &body {
                    match self.code(k) {
                        10 => points.push([self.num(k), 0.0, 0.0]),
                        20 => {
                            if let Some(last) = points.last_mut() {
                                last[1] = self.num(k);
                            }
                        }
                        30 => {
                            if let Some(last) = points.last_mut() {
                                last[2] = self.num(k);
                            }
                        }
                        _ => {}
                    }
                }
                entity(
                    self.base(kind, &body, owner),
                    DxfEntityData::Leader(DxfLeader {
                        points,
                        path_type: self.number(&body, 72, 0.0),
                        annotation_type: self.number(&body, 73, 3.0),
                        has_arrowhead: self.number(&body, 71, 1.0) != 0.0,
                        dimstyle: self.text(&body, 3, "STANDARD"),
                    }),
                )
            }
            "MULTILEADER" | "MLEADER" => Some(self.mleader(kind, &body, owner)),
            "MLINE" => Some(self.mline(&body, owner)),
            "VIEWPORT" => Some(self.viewport(start, end, &body, owner)),
            "XLINE" | "RAY" => entity(
                self.base(kind, &body, owner),
                DxfEntityData::XLine { start: self.point(&body, 10), unit_vector: self.point(&body, 11) },
            ),
            "IMAGE" | "WIPEOUT" => Some(self.image(kind, &body, owner)),
            "ACAD_TABLE" => {
                // 表は AcDbBlockReference の部分（ブロックの名前と位置）だけを読む
                let reference = self.subclass_body(&body, "AcDbBlockReference");
                entity(
                    self.base(kind, &body, owner),
                    DxfEntityData::Table(DxfTable {
                        name: self.text(&reference, 2, ""),
                        insert: self.point(&reference, 10),
                        xscale: self.number(&reference, 41, 1.0),
                        yscale: self.number(&reference, 42, 1.0),
                        zscale: self.number(&reference, 43, 1.0),
                        rotation: self.number(&reference, 50, 0.0),
                    }),
                )
            }
            _ => None,
        }
    }

    /// 指定のサブクラスの中のタグ。サブクラスの印がない図面（R12）は全体
    fn subclass_body(&self, body: &[usize], name: &str) -> Body {
        let mut current = String::new();
        let mut marked = false;
        let mut out = Vec::new();
        for &k in body {
            if self.code(k) == 100 {
                current = self.trimmed(k);
                marked = true;
                continue;
            }
            if current == name {
                out.push(k);
            }
        }
        if marked { out } else { body.to_vec() }
    }

    fn text_base(&self, body: &[usize], valign_code: i64) -> DxfTextBase {
        DxfTextBase {
            text: self.text(body, 1, ""),
            insert: self.point(body, 10),
            align_point: self.has(body, 11).then(|| self.point(body, 11)),
            height: self.number(body, 40, 0.0),
            rotation: self.number(body, 50, 0.0),
            oblique: self.number(body, 51, 0.0),
            width: or(self.number(body, 41, 1.0), 1.0),
            style: self.text(body, 7, "STANDARD"),
            halign: self.number(body, 72, 0.0),
            valign: self.number(body, valign_code, 0.0),
            text_generation_flag: self.number(body, 71, 0.0),
        }
    }

    fn attribute(&self, start: usize, end: usize, kind: &str, owner: Option<&Owner>) -> DxfEntity {
        let body = self.body(start + 1, end);
        // ATTRIB・ATTDEF の縦の揃えは AcDbAttribute の 74（73 は文字数の上限）
        let base = self.base(kind, &body, owner);
        let mut text = self.text_base(&body, 74);
        // 生成フラグは AcDbText の 71。DXF Reference の ATTRIB の表は AcDbAttribute にも 71 を載せるが、AutoCAD 2018 の DXF は
        // そこに属性の種類（1 は 1 行、2 は複数行の属性、4 は複数行の属性定義）を書く（ezdxf の attribute_type、ACadSharp の
        // AttributeType と同じ読み方）
        text.text_generation_flag = self.number(&self.subclass_body(&body, "AcDbText"), 71, 0.0);
        let tag = self.text(&body, 2, "");
        let attribute_flags = self.number(&body, 70, 0.0);
        let data = if kind == "ATTDEF" {
            DxfEntityData::Attdef { text, tag, prompt: self.text(&body, 3, ""), attribute_flags }
        } else {
            DxfEntityData::Attrib { text, tag, attribute_flags }
        };
        DxfEntity { base, data }
    }

    fn lwpolyline(&self, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let mut points: Vec<Vec3> = Vec::new();
        let mut bulges: Vec<f64> = Vec::new();
        let mut widths: Vec<Vec2> = Vec::new();
        let mut flags = 0.0;
        let mut const_width: Option<f64> = None;
        let mut elevation: Option<f64> = None;
        for &k in body {
            let last = points.len().checked_sub(1);
            match self.code(k) {
                70 => flags = self.num(k),
                43 => const_width = Some(self.num(k)),
                38 => elevation = Some(self.num(k)),
                10 => {
                    points.push([self.num(k), 0.0, 0.0]);
                    bulges.push(0.0);
                    widths.push([0.0, 0.0]);
                }
                20 => {
                    if let Some(last) = last {
                        points[last][1] = self.num(k);
                    }
                }
                40 => {
                    if let Some(last) = last {
                        widths[last][0] = self.num(k);
                    }
                }
                41 => {
                    if let Some(last) = last {
                        widths[last][1] = self.num(k);
                    }
                }
                42 => {
                    if let Some(last) = last {
                        bulges[last] = self.num(k);
                    }
                }
                _ => {}
            }
        }
        if let Some(elevation) = elevation {
            for point in &mut points {
                point[2] = elevation;
            }
        }
        DxfEntity {
            base: self.base("LWPOLYLINE", body, owner),
            data: DxfEntityData::LwPolyline(DxfLwPolyline {
                vertices: DxfVertexList { points, bulges, widths, closed: js::to_int32(flags) & 1 != 0, flags },
                const_width: const_width.filter(|width| *width != 0.0),
                elevation,
            }),
        }
    }

    /// POLYLINE に続く VERTEX を SEQEND まで読む（DXF Reference「POLYLINE」「VERTEX」）
    fn polyline(&mut self, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let flags = self.number(body, 70, 0.0);
        let elevation = self.number(body, 30, 0.0);
        let default_start = self.number(body, 40, 0.0);
        let default_end = self.number(body, 41, 0.0);
        let base = self.base("POLYLINE", body, owner);
        let bits = js::to_int32(flags);
        let mut polyline = DxfPolyline {
            vertices: DxfVertexList {
                points: Vec::new(),
                bulges: Vec::new(),
                widths: Vec::new(),
                closed: bits & 1 != 0,
                flags,
            },
            vertex_flags: Vec::new(),
            faces: None,
            m_count: None,
            n_count: None,
        };
        if bits & 16 != 0 {
            polyline.m_count = Some(self.number(body, 71, 0.0));
            polyline.n_count = Some(self.number(body, 72, 0.0));
        }
        let mut faces: Vec<Vec<f64>> = Vec::new();
        let is_3d = bits & (8 | 16 | 64) != 0;
        while self.i < self.length && self.is(self.i, 0, "VERTEX") {
            let start = self.i;
            let end = self.next_zero(start);
            self.i = end;
            let vertex = self.body(start + 1, end);
            let vertex_flags = self.number(&vertex, 70, 0.0);
            let vertex_bits = js::to_int32(vertex_flags);
            if bits & 64 != 0 && vertex_bits & 128 != 0 && vertex_bits & 64 == 0 {
                // ポリフェースメッシュの面の頂点の番号（71〜74）
                let face: Vec<f64> = [71, 72, 73, 74]
                    .iter()
                    .map(|&code| self.number(&vertex, code, 0.0))
                    .filter(|&index| index != 0.0)
                    .collect();
                faces.push(face);
                continue;
            }
            let mut point = self.point(&vertex, 10);
            if !is_3d {
                point[2] = or(point[2], elevation);
            }
            polyline.vertices.points.push(point);
            polyline.vertices.bulges.push(self.number(&vertex, 42, 0.0));
            polyline
                .vertices
                .widths
                .push([self.number(&vertex, 40, default_start), self.number(&vertex, 41, default_end)]);
            polyline.vertex_flags.push(vertex_flags);
        }
        if self.is(self.i, 0, "SEQEND") {
            self.i = self.next_zero(self.i);
        }
        if !faces.is_empty() {
            polyline.faces = Some(faces);
        }
        DxfEntity { base, data: DxfEntityData::Polyline(polyline) }
    }

    fn spline(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let own = if kind == "HELIX" { self.subclass_body(body, "AcDbSpline") } else { body.to_vec() };
        let flags = self.number(&own, 70, 0.0);
        let bits = js::to_int32(flags);
        let base = self.base(kind, body, owner);
        let mut spline = DxfSpline {
            degree: self.number(&own, 71, 3.0),
            flags,
            closed: bits & 1 != 0,
            periodic: bits & 2 != 0,
            rational: bits & 4 != 0,
            knots: Vec::new(),
            control_points: Vec::new(),
            weights: Vec::new(),
            fit_points: Vec::new(),
            start_tangent: None,
            end_tangent: None,
        };
        for &k in &own {
            let code = self.code(k);
            match code {
                40 => spline.knots.push(self.num(k)),
                41 => spline.weights.push(self.num(k)),
                10 => spline.control_points.push([self.num(k), 0.0, 0.0]),
                20 | 30 => {
                    if let Some(last) = spline.control_points.last_mut() {
                        last[if code == 20 { 1 } else { 2 }] = self.num(k);
                    }
                }
                11 => spline.fit_points.push([self.num(k), 0.0, 0.0]),
                21 | 31 => {
                    if let Some(last) = spline.fit_points.last_mut() {
                        last[if code == 21 { 1 } else { 2 }] = self.num(k);
                    }
                }
                _ => {}
            }
        }
        if self.has(&own, 12) {
            spline.start_tangent = Some(self.point(&own, 12));
        }
        if self.has(&own, 13) {
            spline.end_tangent = Some(self.point(&own, 13));
        }
        DxfEntity { base, data: DxfEntityData::Spline(spline) }
    }

    fn mtext(&self, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        // 250 文字を超える文字列は 3 に分けて書き、最後の 1 が残り（DXF Reference「MTEXT」）
        let mut raw = String::new();
        let mut tail = Cow::Borrowed("");
        for &k in body {
            match self.code(k) {
                3 => raw.push_str(&self.raw(k)),
                1 => tail = self.raw(k),
                _ => {}
            }
        }
        raw.push_str(&tail);
        let base = self.base("MTEXT", body, owner);
        let mtext = DxfMText {
            text: mtext_plain_text(&raw),
            raw_text: raw,
            insert: self.point(body, 10),
            char_height: self.number(body, 40, 0.0),
            rect_width: self.number(body, 41, 0.0),
            attachment_point: self.number(body, 71, 1.0),
            drawing_direction: self.number(body, 72, 1.0),
            rotation: self.number(body, 50, 0.0),
            style: self.text(body, 7, "STANDARD"),
            line_spacing_style: self.number(body, 73, 1.0),
            line_spacing_factor: self.number(body, 44, 1.0),
            background_flags: self.number(body, 90, 0.0),
            text_direction: self.has(body, 11).then(|| self.point(body, 11)),
            background_color_index: self.has(body, 63).then(|| self.number(body, 63, 0.0)),
            background_true_color: self
                .has(body, 421)
                .then(|| f64::from(js::to_int32(self.number(body, 421, 0.0)) & 0xff_ffff)),
            background_scale: self.has(body, 45).then(|| self.number(body, 45, 1.5)),
        };
        DxfEntity { base, data: DxfEntityData::MText(mtext) }
    }

    /// INSERT と、66 が 1 なら続く ATTRIB を SEQEND まで読む
    fn insert(&mut self, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let base = self.base("INSERT", body, owner);
        let mut insert = DxfInsert {
            name: self.text(body, 2, ""),
            insert: self.point(body, 10),
            xscale: self.number(body, 41, 1.0),
            yscale: self.number(body, 42, 1.0),
            zscale: self.number(body, 43, 1.0),
            rotation: self.number(body, 50, 0.0),
            column_count: js::max(1.0, self.number(body, 70, 1.0)),
            row_count: js::max(1.0, self.number(body, 71, 1.0)),
            column_spacing: self.number(body, 44, 0.0),
            row_spacing: self.number(body, 45, 0.0),
            attribs: Vec::new(),
        };
        if self.number(body, 66, 0.0) == 1.0 {
            // 属性は配置と同じ空間・ブロックに属する
            let attribute_owner = Owner { space: base.space, block: base.block.clone() };
            while self.i < self.length && self.is(self.i, 0, "ATTRIB") {
                let start = self.i;
                let end = self.next_zero(start);
                self.i = end;
                insert.attribs.push(self.attribute(start, end, "ATTRIB", Some(&attribute_owner)));
            }
            if self.is(self.i, 0, "SEQEND") {
                self.i = self.next_zero(self.i);
            }
        }
        DxfEntity { base, data: DxfEntityData::Insert(insert) }
    }

    fn dimension(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let base = self.base(kind, body, owner);
        let optional_point = |code: i64| self.has(body, code).then(|| self.point(body, code));
        let optional_number =
            |code: i64, fallback: f64| self.has(body, code).then(|| self.number(body, code, fallback));
        let dimension = DxfDimension {
            anonymous_block_name: self.text(body, 2, ""),
            dimtype: self.number(body, 70, 0.0),
            defpoint: self.point(body, 10),
            text_midpoint: self.point(body, 11),
            text: self.text(body, 1, ""),
            dimstyle: self.text(body, 3, "STANDARD"),
            insert: optional_point(12),
            defpoint2: optional_point(13),
            defpoint3: optional_point(14),
            defpoint4: optional_point(15),
            defpoint5: optional_point(16),
            actual_measurement: optional_number(42, 0.0),
            attachment_point: optional_number(71, 5.0),
            text_rotation: optional_number(53, 0.0),
            horizontal_direction: optional_number(51, 0.0),
            angle: optional_number(50, 0.0),
            oblique_angle: optional_number(52, 0.0),
        };
        DxfEntity { base, data: DxfEntityData::Dimension(dimension) }
    }

    /// HATCH は境界・模様の値を決まった順に書く（DXF Reference「HATCH」「Boundary Path Data」「Pattern Data」）
    fn hatch(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let own = self.subclass_body(body, if kind == "HATCH" { "AcDbHatch" } else { "AcDbMPolygon" });
        let base = self.base(kind, body, owner);
        let mut hatch = DxfHatch {
            pattern_name: String::new(),
            solid_fill: false,
            associative: false,
            hatch_style: 0.0,
            pattern_type: 1.0,
            pattern_angle: 0.0,
            pattern_scale: 1.0,
            pattern_double: false,
            elevation: 0.0,
            paths: Vec::new(),
            pattern_lines: Vec::new(),
            gradient: None,
            gradient_colors: None,
            gradient_angle: 0.0,
            gradient_shift: 0.0,
            gradient_one_color: false,
            gradient_tint: 0.0,
        };
        let mut cursor = Cursor { own: &own, codes: self.codes, k: 0 };
        // 境界の前: 標高（10・20・30）、名前（2）、塗りつぶし（70）、関連付け（71）、境界の数（91）
        let mut paths = 0.0;
        while cursor.more() {
            let code = cursor.peek();
            if code == 91 {
                paths = self.num(cursor.take());
                break;
            }
            let index = cursor.take();
            // MPOLYGON は 70 が版、71 が塗りつぶし（DXF Reference「HATCH」の MPolygon の注記）
            if code == 30 {
                hatch.elevation = self.num(index);
            } else if code == 2 {
                hatch.pattern_name = self.str(index);
            } else if code == 70 && kind == "HATCH" {
                hatch.solid_fill = self.num(index) == 1.0;
            } else if code == 71 && kind == "HATCH" {
                hatch.associative = self.num(index) == 1.0;
            } else if code == 71 {
                hatch.solid_fill = self.num(index) == 1.0;
            }
        }
        let mut p = 0.0;
        while p < paths && cursor.more() {
            while cursor.more() && cursor.peek() != 92 {
                cursor.k += 1;
            }
            if !cursor.more() {
                break;
            }
            let flags = self.num(cursor.take());
            let mut path = DxfHatchPath { flags, closed: true, points: Vec::new(), bulges: None, edges: None };
            if js::to_int32(flags) & 2 != 0 {
                let mut has_bulge = false;
                let mut count = 0.0;
                while cursor.more() && cursor.peek() != 93 {
                    let index = cursor.take();
                    match self.code(index) {
                        72 => has_bulge = self.num(index) != 0.0,
                        73 => path.closed = self.num(index) != 0.0,
                        _ => {}
                    }
                }
                if cursor.more() {
                    count = self.num(cursor.take());
                }
                let mut bulges: Vec<f64> = Vec::new();
                let mut v = 0.0;
                while v < count && cursor.more() {
                    if cursor.peek() != 10 {
                        break;
                    }
                    let x = self.num(cursor.take());
                    let y = if cursor.peek() == 20 { self.num(cursor.take()) } else { 0.0 };
                    let mut bulge = 0.0;
                    if has_bulge && cursor.peek() == 42 {
                        bulge = self.num(cursor.take());
                    }
                    path.points.push([x, y, hatch.elevation]);
                    bulges.push(bulge);
                    v += 1.0;
                }
                if bulges.iter().any(|value| *value != 0.0) {
                    path.bulges = Some(bulges);
                }
            } else {
                while cursor.more() && cursor.peek() != 93 {
                    cursor.k += 1;
                }
                let count = if cursor.more() { self.num(cursor.take()) } else { 0.0 };
                let mut edges = Vec::new();
                let mut e = 0.0;
                while e < count && cursor.more() {
                    if cursor.peek() != 72 {
                        break;
                    }
                    let edge_kind = self.num(cursor.take());
                    if let Some(edge) = self.hatch_edge(edge_kind, &mut cursor) {
                        edges.push(edge);
                    }
                    e += 1.0;
                }
                path.edges = Some(edges);
            }
            // 境界の元の図形への参照（97・330）
            if cursor.peek() == 97 {
                let sources = self.num(cursor.take());
                let mut s = 0.0;
                while s < sources && cursor.peek() == 330 {
                    cursor.take();
                    s += 1.0;
                }
            }
            hatch.paths.push(path);
            p += 1.0;
        }
        // 境界の後: 種類（75・76）、角度・尺度（52・41）、二重（77）、模様の線（78 と 53・43〜46・79・49）
        while cursor.more() {
            let code = cursor.peek();
            if code == 78 {
                let count = self.num(cursor.take());
                let mut l = 0.0;
                while l < count && cursor.more() {
                    while cursor.more() && cursor.peek() != 53 {
                        if cursor.peek() == 47 || cursor.peek() == 98 || cursor.peek() == 450 {
                            break;
                        }
                        cursor.k += 1;
                    }
                    if cursor.peek() != 53 {
                        break;
                    }
                    let mut line = DxfPatternLine {
                        angle: self.num(cursor.take()),
                        base: [0.0, 0.0],
                        offset: [0.0, 0.0],
                        dashes: Vec::new(),
                    };
                    while cursor.more() {
                        match cursor.peek() {
                            43 => line.base[0] = self.num(cursor.take()),
                            44 => line.base[1] = self.num(cursor.take()),
                            45 => line.offset[0] = self.num(cursor.take()),
                            46 => line.offset[1] = self.num(cursor.take()),
                            79 => {
                                cursor.take();
                            }
                            49 => line.dashes.push(self.num(cursor.take())),
                            _ => break,
                        }
                    }
                    hatch.pattern_lines.push(line);
                    l += 1.0;
                }
                continue;
            }
            let index = cursor.take();
            match code {
                75 => hatch.hatch_style = self.num(index),
                76 => hatch.pattern_type = self.num(index),
                52 => hatch.pattern_angle = self.num(index),
                41 => hatch.pattern_scale = self.num(index),
                77 => hatch.pattern_double = self.num(index) != 0.0,
                450 => {
                    if self.num(index) != 0.0 {
                        hatch.gradient = Some(String::new());
                    }
                }
                421 => {
                    if hatch.gradient.is_some() {
                        hatch
                            .gradient_colors
                            .get_or_insert_with(Vec::new)
                            .push(f64::from(js::to_int32(self.num(index)) & 0xff_ffff));
                    }
                }
                470 if hatch.gradient.is_some() => hatch.gradient = Some(self.str(index)),
                460 => hatch.gradient_angle = self.num(index),
                461 => hatch.gradient_shift = self.num(index),
                452 => hatch.gradient_one_color = self.num(index) != 0.0,
                462 => hatch.gradient_tint = self.num(index),
                _ => {}
            }
        }
        DxfEntity { base, data: DxfEntityData::Hatch(hatch) }
    }

    fn hatch_edge(&self, kind: f64, cursor: &mut Cursor<'_>) -> Option<DxfHatchEdge> {
        let read = |cursor: &mut Cursor<'_>, codes: &[i64]| -> HashMap<i64, f64> {
            let mut values = HashMap::new();
            while codes.contains(&cursor.peek()) {
                let index = cursor.take();
                values.insert(self.code(index), self.num(index));
            }
            values
        };
        let get = |values: &HashMap<i64, f64>, code: i64, fallback: f64| values.get(&code).copied().unwrap_or(fallback);
        if kind == 1.0 {
            let v = read(cursor, &[10, 20, 11, 21]);
            return Some(DxfHatchEdge::Line {
                start: [get(&v, 10, 0.0), get(&v, 20, 0.0)],
                end: [get(&v, 11, 0.0), get(&v, 21, 0.0)],
            });
        }
        if kind == 2.0 {
            let v = read(cursor, &[10, 20, 40, 50, 51, 73]);
            return Some(DxfHatchEdge::Arc {
                center: [get(&v, 10, 0.0), get(&v, 20, 0.0)],
                radius: get(&v, 40, 0.0),
                start_angle: get(&v, 50, 0.0),
                end_angle: get(&v, 51, 360.0),
                ccw: get(&v, 73, 1.0) != 0.0,
            });
        }
        if kind == 3.0 {
            let v = read(cursor, &[10, 20, 11, 21, 40, 50, 51, 73]);
            return Some(DxfHatchEdge::Ellipse {
                center: [get(&v, 10, 0.0), get(&v, 20, 0.0)],
                major_axis: [get(&v, 11, 1.0), get(&v, 21, 0.0)],
                ratio: get(&v, 40, 1.0),
                start_angle: get(&v, 50, 0.0),
                end_angle: get(&v, 51, 360.0),
                ccw: get(&v, 73, 1.0) != 0.0,
            });
        }
        if kind != 4.0 {
            return None;
        }
        let mut degree = 3.0;
        let mut rational = false;
        let mut periodic = false;
        let mut knots: Vec<f64> = Vec::new();
        let mut control_points: Vec<Vec2> = Vec::new();
        let mut weights: Vec<f64> = Vec::new();
        let mut fit_points: Vec<Vec2> = Vec::new();
        let mut knot_count = 0.0;
        let mut control_count = 0.0;
        loop {
            match cursor.peek() {
                94 => degree = self.num(cursor.take()),
                73 => rational = self.num(cursor.take()) != 0.0,
                74 => periodic = self.num(cursor.take()) != 0.0,
                95 => knot_count = self.num(cursor.take()),
                96 => control_count = self.num(cursor.take()),
                _ => break,
            }
        }
        let mut n = 0.0;
        while n < knot_count && cursor.peek() == 40 {
            knots.push(self.num(cursor.take()));
            n += 1.0;
        }
        let mut n = 0.0;
        while n < control_count && cursor.peek() == 10 {
            let x = self.num(cursor.take());
            let y = if cursor.peek() == 20 { self.num(cursor.take()) } else { 0.0 };
            control_points.push([x, y]);
            if cursor.peek() == 42 {
                weights.push(self.num(cursor.take()));
            }
            n += 1.0;
        }
        // 当てはめ点（97・11・21）と接線（12・13）は R2010 以降
        if cursor.peek() == 97 {
            let fits = self.num(cursor.take());
            let mut n = 0.0;
            while n < fits && cursor.peek() == 11 {
                let x = self.num(cursor.take());
                let y = if cursor.peek() == 21 { self.num(cursor.take()) } else { 0.0 };
                fit_points.push([x, y]);
                n += 1.0;
            }
            while [12, 22, 13, 23].contains(&cursor.peek()) {
                cursor.take();
            }
        }
        Some(DxfHatchEdge::Spline { degree, rational, periodic, knots, control_points, weights, fit_points })
    }

    /// タグを順に返し、点の X のコード（codes）に続く Y・Z（+10・+20）を 1 つの点にまとめる
    fn points(&self, own: &[usize], codes: &[i32]) -> Vec<(i64, usize, Vec3)> {
        let mut out = Vec::with_capacity(own.len());
        let mut i = 0;
        while i < own.len() {
            let k = own[i];
            let code = self.code(k);
            let mut point = [0.0, 0.0, 0.0];
            if codes.iter().any(|candidate| i64::from(*candidate) == code) {
                point[0] = self.num(k);
                if i + 1 < own.len() && self.code(own[i + 1]) == code + 10 {
                    i += 1;
                    point[1] = self.num(own[i]);
                }
                if i + 1 < own.len() && self.code(own[i + 1]) == code + 20 {
                    i += 1;
                    point[2] = self.num(own[i]);
                }
            }
            out.push((code, k, point));
            i += 1;
        }
        out
    }

    /// MULTILEADER は、CONTEXT_DATA{（300〜301）・LEADER{（302〜303）・LEADER_LINE{（304〜305）の区切りの中と外で
    /// 同じグループコードを別の意味に使う（DXF Reference「Common MLeader Group Codes」「MLeader Context Data Group Codes」
    /// 「MLeader Leader Node Group Codes」「MLeader Leader Line Group Codes」）。区切りを追って順に読む
    fn mleader(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        #[derive(PartialEq)]
        enum Section {
            Common,
            Context,
            Leader,
            Line,
        }
        let base = self.base(kind, body, owner);
        let mut entity = DxfMLeader {
            style_handle: String::new(),
            leader_type: 1.0,
            leader_line_color: BY_BLOCK_RAW_COLOR,
            leader_linetype_handle: String::new(),
            leader_lineweight: -2.0,
            has_dogleg: true,
            arrow_head_handle: String::new(),
            content_type: 2.0,
            has_text_frame: false,
            text_left_attachment_type: 1.0,
            text_right_attachment_type: 1.0,
            text_top_attachment_type: 9.0,
            text_bottom_attachment_type: 9.0,
            arrow_heads: Vec::new(),
            block_attribs: Vec::new(),
            context: DxfMLeaderContext {
                scale: 1.0,
                base_point: [0.0, 0.0, 0.0],
                char_height: 0.0,
                arrow_head_size: 0.0,
                landing_gap_size: 0.0,
                mtext: None,
                block: None,
                leaders: Vec::new(),
            },
        };
        let mut section = Section::Common;
        // 内容（文字・ブロック）の値を読んでいる間は true
        let mut in_mtext = false;
        let mut in_block = false;
        let mut leader: Option<DxfMLeaderLeader> = None;
        let mut line: Option<DxfMLeaderLine> = None;
        let mut break_start: Option<Vec3> = None;
        let mut arrow_index = 0.0;
        for (code, k, point) in self.points(&self.subclass_body(body, "AcDbMLeader"), &MLEADER_POINT_CODES) {
            if section == Section::Line
                && let Some(current) = line.as_mut()
            {
                match code {
                    305 => {
                        let finished = line.take().expect("線を読んでいる");
                        if let Some(leader) = leader.as_mut() {
                            leader.lines.push(finished);
                        }
                        section = Section::Leader;
                    }
                    10 => current.vertices.push(point),
                    11 => break_start = Some(point),
                    12 if break_start.is_some() => {
                        current
                            .breaks
                            .push(DxfMLeaderBreak {
                                start: break_start.take().expect("切れ目の始まり"), end: point
                            });
                    }
                    91 => current.index = self.num(k),
                    92 => current.color = self.num(k),
                    _ => {}
                }
                continue;
            }
            if section == Section::Leader
                && let Some(current) = leader.as_mut()
            {
                if code == 303 {
                    let finished = leader.take().expect("引出線を読んでいる");
                    entity.context.leaders.push(finished);
                    section = Section::Context;
                } else if code == 304 && js::trim(&self.raw(k)) == "LEADER_LINE{" {
                    line = Some(DxfMLeaderLine {
                        vertices: Vec::new(),
                        breaks: Vec::new(),
                        index: 0.0,
                        color: BY_BLOCK_RAW_COLOR,
                    });
                    break_start = None;
                    section = Section::Line;
                } else if code == 290 {
                    current.has_last_leader_line = self.num(k) != 0.0;
                } else if code == 291 {
                    current.has_dogleg_vector = self.num(k) != 0.0;
                } else if code == 10 {
                    current.last_leader_point = point;
                } else if code == 11 {
                    current.dogleg_vector = point;
                } else if code == 12 {
                    break_start = Some(point);
                } else if code == 13 && break_start.is_some() {
                    current
                        .breaks
                        .push(DxfMLeaderBreak {
                            start: break_start.take().expect("切れ目の始まり"), end: point
                        });
                } else if code == 90 {
                    current.index = self.num(k);
                } else if code == 40 {
                    current.dogleg_length = self.num(k);
                } else if code == 271 {
                    current.attachment_direction = self.num(k);
                }
                continue;
            }
            if section == Section::Context {
                // 290・296 の後は、内容（文字・ブロック）の値が続く。内容の値でないコードで内容が終わる（ezdxf の MLeaderContext.load と同じ）
                if in_mtext
                    && let Some(mtext) = entity.context.mtext.as_mut()
                    && self.mleader_mtext(mtext, code, k, point)
                {
                    continue;
                }
                in_mtext = false;
                if in_block
                    && let Some(block) = entity.context.block.as_mut()
                    && self.mleader_block(block, code, k, point)
                {
                    continue;
                }
                in_block = false;
                let context = &mut entity.context;
                if code == 301 {
                    section = Section::Common;
                } else if code == 302 && js::trim(&self.raw(k)) == "LEADER{" {
                    leader = Some(DxfMLeaderLeader {
                        lines: Vec::new(),
                        has_last_leader_line: true,
                        has_dogleg_vector: true,
                        last_leader_point: [0.0, 0.0, 0.0],
                        dogleg_vector: [1.0, 0.0, 0.0],
                        dogleg_length: 0.0,
                        index: 0.0,
                        attachment_direction: 0.0,
                        breaks: Vec::new(),
                    });
                    break_start = None;
                    section = Section::Leader;
                } else if code == 290 && self.num(k) == 1.0 {
                    context.mtext = Some(DxfMLeaderMText {
                        default_content: String::new(),
                        extrusion: [0.0, 0.0, 1.0],
                        style_handle: String::new(),
                        insert: [0.0, 0.0, 0.0],
                        text_direction: [1.0, 0.0, 0.0],
                        width: 0.0,
                        line_spacing_factor: 1.0,
                        line_spacing_style: 1.0,
                        color: BY_BLOCK_RAW_COLOR,
                        alignment: 1.0,
                        flow_direction: 1.0,
                        bg_color: 0.0,
                        bg_scale_factor: 1.5,
                        use_window_bg_color: false,
                        has_bg_fill: false,
                        column_type: 0.0,
                    });
                    in_mtext = true;
                } else if code == 296 && self.num(k) == 1.0 {
                    context.block = Some(DxfMLeaderBlock {
                        block_record_handle: String::new(),
                        extrusion: [0.0, 0.0, 1.0],
                        insert: [0.0, 0.0, 0.0],
                        scale: [1.0, 1.0, 1.0],
                        rotation: 0.0,
                        color: BY_BLOCK_RAW_COLOR,
                    });
                    in_block = true;
                } else if code == 40 {
                    context.scale = self.num(k);
                } else if code == 10 {
                    context.base_point = point;
                } else if code == 41 {
                    context.char_height = self.num(k);
                } else if code == 140 {
                    context.arrow_head_size = self.num(k);
                } else if code == 145 {
                    context.landing_gap_size = self.num(k);
                }
                continue;
            }
            match code {
                300 => section = Section::Context,
                340 => entity.style_handle = self.upper(k),
                170 => entity.leader_type = self.num(k),
                91 => entity.leader_line_color = self.num(k),
                341 => entity.leader_linetype_handle = self.upper(k),
                171 => entity.leader_lineweight = self.num(k),
                291 => entity.has_dogleg = self.num(k) != 0.0,
                342 => entity.arrow_head_handle = self.upper(k),
                172 => entity.content_type = self.num(k),
                292 => entity.has_text_frame = self.num(k) != 0.0,
                173 => entity.text_left_attachment_type = self.num(k),
                95 => entity.text_right_attachment_type = self.num(k),
                272 => entity.text_bottom_attachment_type = self.num(k),
                273 => entity.text_top_attachment_type = self.num(k),
                94 => arrow_index = self.num(k),
                345 => entity.arrow_heads.push(DxfMLeaderArrowHead { index: arrow_index, handle: self.upper(k) }),
                330 => entity.block_attribs.push(DxfMLeaderAttrib {
                    handle: self.upper(k),
                    index: 0.0,
                    width: 1.0,
                    text: String::new(),
                }),
                177 | 44 | 302 => {
                    if let Some(attrib) = entity.block_attribs.last_mut() {
                        match code {
                            177 => attrib.index = self.num(k),
                            44 => attrib.width = self.num(k),
                            _ => attrib.text = self.str(k),
                        }
                    }
                }
                _ => {}
            }
        }
        DxfEntity { base, data: DxfEntityData::MLeader(Box::new(entity)) }
    }

    /// CONTEXT_DATA の文字の内容の値。内容の値でなければ false
    fn mleader_mtext(&self, mtext: &mut DxfMLeaderMText, code: i64, k: usize, point: Vec3) -> bool {
        match code {
            // 書式コードを含むので、\U+ などは MTEXT の読み取りが復号する
            304 => mtext.default_content = self.raw(k).into_owned(),
            11 => mtext.extrusion = point,
            340 => mtext.style_handle = self.upper(k),
            12 => mtext.insert = point,
            13 => mtext.text_direction = point,
            43 => mtext.width = self.num(k),
            45 => mtext.line_spacing_factor = self.num(k),
            170 => mtext.line_spacing_style = self.num(k),
            90 => mtext.color = self.num(k),
            171 => mtext.alignment = self.num(k),
            172 => mtext.flow_direction = self.num(k),
            91 => mtext.bg_color = self.num(k),
            141 => mtext.bg_scale_factor = self.num(k),
            291 => mtext.use_window_bg_color = self.num(k) != 0.0,
            292 => mtext.has_bg_fill = self.num(k) != 0.0,
            173 => mtext.column_type = self.num(k),
            // 回転（42、13 の向きと同じ）、高さ、背景の透過、段組みの寸法は使わない
            42 | 44 | 92 | 293 | 142 | 143 | 294 | 144 | 295 => {}
            _ => return false,
        }
        true
    }

    /// CONTEXT_DATA のブロックの内容の値。内容の値でなければ false
    fn mleader_block(&self, block: &mut DxfMLeaderBlock, code: i64, k: usize, point: Vec3) -> bool {
        match code {
            341 => block.block_record_handle = self.upper(k),
            14 => block.extrusion = point,
            15 => block.insert = point,
            16 => block.scale = point,
            46 => block.rotation = self.num(k),
            93 => block.color = self.num(k),
            // 変換の行列（47 × 16）は AutoCAD も使わない（ezdxf の BlockData.export_dxf の注記）
            47 => {}
            _ => return false,
        }
        true
    }

    /// MLINE（DXF Reference「MLINE」）。頂点（11・12・13）ごとに、要素ごとの 74・41 と 75・42 が続く
    fn mline(&self, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let own = self.subclass_body(body, "AcDbMline");
        let base = self.base("MLINE", body, owner);
        let mut line_params: Vec<Vec<Vec<f64>>> = Vec::new();
        let mut fill_params: Vec<Vec<Vec<f64>>> = Vec::new();
        let mut entity = DxfMLine {
            style_name: "Standard".into(),
            mlinestyle_handle: None,
            scale: 1.0,
            justification: 0.0,
            flags: 0.0,
            closed: false,
            line_count: 0.0,
            base_point: [0.0, 0.0, 0.0],
            points: Vec::new(),
            vertex_directions: Vec::new(),
            miter_directions: Vec::new(),
            line_params: None,
            fill_params: None,
        };
        /// 41・42 を足す先（74・75 の後）。頂点より前の 74・75 の値は捨てる
        enum Params {
            None,
            Line(usize, usize),
            Fill(usize, usize),
            Orphan,
        }
        let mut params = Params::None;
        for (code, k, point) in self.points(&own, &MLINE_POINT_CODES) {
            match code {
                2 => entity.style_name = self.str(k),
                340 => entity.mlinestyle_handle = Some(self.upper(k)),
                40 => entity.scale = self.num(k),
                70 => entity.justification = self.num(k),
                71 => entity.flags = self.num(k),
                73 => entity.line_count = self.num(k),
                10 => entity.base_point = point,
                11 => {
                    entity.points.push(point);
                    line_params.push(Vec::new());
                    fill_params.push(Vec::new());
                    params = Params::None;
                }
                12 => entity.vertex_directions.push(point),
                13 => entity.miter_directions.push(point),
                74 | 75 => {
                    let list = if code == 74 { &mut line_params } else { &mut fill_params };
                    params = match list.last_mut() {
                        Some(vertex) => {
                            vertex.push(Vec::new());
                            let (v, e) = (list.len() - 1, list[list.len() - 1].len() - 1);
                            if code == 74 { Params::Line(v, e) } else { Params::Fill(v, e) }
                        }
                        None => Params::Orphan,
                    };
                }
                41 | 42 => match params {
                    Params::Line(v, e) => line_params[v][e].push(self.num(k)),
                    Params::Fill(v, e) => fill_params[v][e].push(self.num(k)),
                    Params::None | Params::Orphan => {}
                },
                _ => {}
            }
        }
        entity.closed = js::to_int32(entity.flags) & 2 != 0;
        entity.line_params = Some(line_params);
        entity.fill_params = Some(fill_params);
        DxfEntity { base, data: DxfEntityData::MLine(entity) }
    }

    fn viewport(&self, start: usize, end: usize, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let frozen: Vec<String> = body.iter().filter(|&&k| self.code(k) == 331).map(|&k| self.upper(k)).collect();
        let base = self.base("VIEWPORT", body, owner);
        let mut viewport = DxfViewport {
            center: self.point(body, 10),
            width: self.number(body, 40, 0.0),
            height: self.number(body, 41, 0.0),
            status: self.number(body, 68, 1.0),
            id: self.number(body, 69, 1.0),
            view_center: [self.number(body, 12, 0.0), self.number(body, 22, 0.0)],
            view_target: self.point(body, 17),
            view_direction: if self.has(body, 16) { self.point(body, 16) } else { [0.0, 0.0, 1.0] },
            view_height: self.number(body, 45, 0.0),
            view_twist_angle: self.number(body, 51, 0.0),
            status_flags: self.number(body, 90, 0.0),
            frozen_layer_handles: frozen,
            clipping_boundary_handle: None,
        };
        let clip = self.text(body, 340, "");
        if !clip.is_empty() && clip != "0" {
            viewport.clipping_boundary_handle = Some(js::trim(&clip).to_uppercase());
        }
        if !self.has(body, 45) {
            self.viewport_r12(start, end, &mut viewport);
        }
        DxfEntity { base, data: DxfEntityData::Viewport(viewport) }
    }

    /// R12 のビューポートは表示の値を拡張データ ACAD の MVIEW に持つ（ezdxf の viewport.py と同じ並び）
    fn viewport_r12(&self, start: usize, end: usize, viewport: &mut DxfViewport) {
        let mut k = start;
        while k < end && !(self.code(k) == 1000 && upper_is(js::trim(&self.raw(k)), "MVIEW")) {
            k += 1;
        }
        if k >= end {
            return;
        }
        let mut reals: Vec<f64> = Vec::new();
        let mut points: Vec<Vec3> = Vec::new();
        k += 1;
        while k < end {
            let code = self.code(k);
            if code == 1010 {
                points.push([self.num(k), 0.0, 0.0]);
            } else if code == 1020 && !points.is_empty() {
                points.last_mut().expect("点がある")[1] = self.num(k);
            } else if code == 1030 && !points.is_empty() {
                points.last_mut().expect("点がある")[2] = self.num(k);
            } else if code == 1040 {
                reals.push(self.num(k));
            } else if code == 1002 && js::trim(&self.raw(k)) == "{" && !reals.is_empty() {
                break;
            }
            k += 1;
        }
        if let Some(target) = points.first() {
            viewport.view_target = *target;
        }
        if let Some(direction) = points.get(1) {
            viewport.view_direction = *direction;
        }
        let real = |index: usize| reals.get(index).copied().unwrap_or(0.0);
        viewport.view_twist_angle = real(0);
        viewport.view_height = real(1);
        viewport.view_center = [real(2), real(3)];
    }

    fn image(&self, kind: &str, body: &[usize], owner: Option<&Owner>) -> DxfEntity {
        let mut boundary: Vec<Vec2> = Vec::new();
        for &k in body {
            match self.code(k) {
                14 => boundary.push([self.num(k), 0.0]),
                24 => {
                    if let Some(last) = boundary.last_mut() {
                        last[1] = self.num(k);
                    }
                }
                _ => {}
            }
        }
        let size = self.point(body, 13);
        let base = self.base(kind, body, owner);
        let image = DxfImage {
            insert: self.point(body, 10),
            u_pixel: self.point(body, 11),
            v_pixel: self.point(body, 12),
            image_size: [or(size[0], 1.0), or(size[1], 1.0)],
            boundary,
            flags: self.number(body, 70, 0.0),
            clipping: self.number(body, 280, 0.0) != 0.0,
        };
        DxfEntity { base, data: DxfEntityData::Image(image) }
    }

    // --- OBJECTS --------------------------------------------------------------

    fn objects(&mut self) {
        while self.i < self.length && !self.is(self.i, 0, "ENDSEC") {
            if self.is(self.i, 0, "SECTION") || self.is(self.i, 0, "EOF") {
                return;
            }
            if self.code(self.i) != 0 {
                self.i += 1;
                continue;
            }
            let kind = self.upper(self.i);
            let end = self.next_zero(self.i);
            let body = self.body(self.i + 1, end);
            match kind.as_str() {
                "LAYOUT" => self.layout(&body),
                "SORTENTSTABLE" => self.sort_table(&body),
                "MLINESTYLE" => self.mline_style(&body),
                _ => {}
            }
            self.i = end;
        }
        self.i += 1;
    }

    /// LAYOUT は PLOTSETTINGS（AcDbPlotSettings）と AcDbLayout の 2 つのサブクラスで同じコードを使う
    fn layout(&mut self, body: &[usize]) {
        let plot = self.subclass_body(body, "AcDbPlotSettings");
        let own = self.subclass_body(body, "AcDbLayout");
        let record = self.handle_of(&own, 330);
        let layout = DxfLayout {
            handle: self.handle_of(body, 5),
            name: self.text(&own, 1, "Layout"),
            tab_order: self.number(&own, 71, 0.0),
            model: false,
            active: false,
            block: String::new(),
            block_record_handle: (!record.is_empty()).then(|| record.clone()),
            paper_width: self.number(&plot, 44, 0.0),
            paper_height: self.number(&plot, 45, 0.0),
            margins: [
                self.number(&plot, 40, 0.0),
                self.number(&plot, 41, 0.0),
                self.number(&plot, 42, 0.0),
                self.number(&plot, 43, 0.0),
            ],
            paper_size: self.text(&plot, 4, ""),
            paper_units: self.number(&plot, 72, 1.0),
            plot_rotation: self.number(&plot, 73, 0.0),
            plot_scale: [self.number(&plot, 142, 1.0), self.number(&plot, 143, 1.0)],
            plot_origin: [self.number(&plot, 46, 0.0), self.number(&plot, 47, 0.0)],
            limmin: [self.number(&own, 10, 0.0), self.number(&own, 20, 0.0)],
            limmax: [self.number(&own, 11, 0.0), self.number(&own, 21, 0.0)],
        };
        self.layout_records.push((record, layout));
    }

    /// MLINESTYLE（DXF Reference「MLINESTYLE」）。要素の数（71）の後の 62 は要素の色で、前の 62 は塗りの色
    fn mline_style(&mut self, body: &[usize]) {
        let mut style = DxfMLineStyle {
            handle: self.handle_of(body, 5),
            name: String::new(),
            flags: 0.0,
            description: String::new(),
            fill_color: 256.0,
            start_angle: 90.0,
            end_angle: 90.0,
            elements: Vec::new(),
        };
        let mut elements = false;
        for k in self.subclass_body(body, "AcDbMlineStyle") {
            match self.code(k) {
                2 => style.name = self.str(k),
                70 => style.flags = self.num(k),
                3 => style.description = self.str(k),
                62 => {
                    if !elements {
                        style.fill_color = self.num(k);
                    } else if let Some(last) = style.elements.last_mut() {
                        last.color = self.num(k);
                    }
                }
                51 => style.start_angle = self.num(k),
                52 => style.end_angle = self.num(k),
                71 => elements = true,
                49 => style.elements.push(DxfMLineStyleElement {
                    offset: self.num(k),
                    color: 0.0,
                    linetype: "BYLAYER".into(),
                }),
                6 => {
                    if let Some(last) = style.elements.last_mut() {
                        last.linetype = self.str(k);
                    }
                }
                _ => {}
            }
        }
        self.doc.mline_styles.get_or_insert_with(Vec::new).push(style);
    }

    fn sort_table(&mut self, body: &[usize]) {
        let own = self.subclass_body(body, "AcDbSortentsTable");
        let mut owner = String::new();
        let mut entity = String::new();
        let mut handles: HashMap<String, String> = HashMap::new();
        for &k in &own {
            let code = self.code(k);
            if code == 330 && owner.is_empty() {
                owner = self.upper(k);
            } else if code == 331 {
                entity = self.upper(k);
            } else if code == 5 && !entity.is_empty() {
                handles.insert(std::mem::take(&mut entity), self.upper(k));
            }
        }
        if !owner.is_empty() {
            self.sort_tables.push((owner, handles));
        }
    }

    // --- 仕上げ ---------------------------------------------------------------

    fn finish(&mut self) {
        for (record, mut layout) in std::mem::take(&mut self.layout_records) {
            let name = self.block_records.get(&record).cloned().unwrap_or_default();
            layout.model = is_model_block(&name) || upper_is(&layout.name, "MODEL");
            layout.active = upper_is(&name, PAPER_SPACE);
            layout.block = name;
            self.doc.layouts.push(layout);
        }
        // R12 には LAYOUT がない。紙空間の図形があれば 1 つのレイアウトとして持つ
        if !self.doc.layouts.iter().any(|layout| !layout.model) && self.has_paper_entities {
            let min = self.doc.header.plimmin.unwrap_or([0.0, 0.0]);
            let max = self.doc.header.plimmax.unwrap_or([0.0, 0.0]);
            self.doc.layouts.push(DxfLayout {
                handle: String::new(),
                name: "Layout1".into(),
                tab_order: 1.0,
                model: false,
                active: true,
                block: "*Paper_Space".into(),
                block_record_handle: None,
                paper_width: 0.0,
                paper_height: 0.0,
                margins: [0.0, 0.0, 0.0, 0.0],
                paper_size: String::new(),
                paper_units: 1.0,
                plot_rotation: 0.0,
                plot_scale: [1.0, 1.0],
                plot_origin: [0.0, 0.0],
                limmin: min,
                limmax: max,
            });
        }
        js::sort_by(&mut self.doc.layouts, |a, b| a.tab_order - b.tab_order);
        for (owner, handles) in std::mem::take(&mut self.sort_tables) {
            let Some(name) = self.block_records.get(&owner).filter(|name| !name.is_empty()) else {
                continue;
            };
            match self.doc.sort_handles.iter_mut().find(|(key, _)| key == name) {
                Some(entry) => entry.1 = handles,
                None => self.doc.sort_handles.push((name.clone(), handles)),
            }
        }
        // 名前をキーにした表（JavaScript のオブジェクト）は、整数の名前を先に小さい順に並べる
        js::order_object_keys(&mut self.doc.sort_handles);
        js::order_object_keys(&mut self.doc.skipped);
        if !self.doc.skipped.is_empty() {
            let total = self.doc.skipped.iter().map(|(_, count)| count).sum();
            let list: Vec<String> = self.doc.skipped.iter().map(|(kind, count)| format!("{kind} {count}")).collect();
            self.notices.add_count(
                NoticeLevel::Info,
                "skipped-types",
                format!("読み取りに対応していない図形を表示していません（{}）", list.join("、")),
                total,
            );
        }
    }
}

/// `value || fallback`（0 と NaN は fallback）
pub(crate) fn or(value: f64, fallback: f64) -> f64 {
    if value == 0.0 || value.is_nan() { fallback } else { value }
}

/// DXF（テキスト・バイナリ）を AutoCAD の図面のモデルにする
pub fn parse_dxf(bytes: &[u8]) -> DxfDocument {
    let mut notices = NoticeList::new();
    let read = read_tags(bytes, &mut notices);
    let parser = Parser {
        codes: &read.tags.codes,
        values: &read.tags.values,
        length: read.tags.codes.len(),
        i: 0,
        doc: DxfDocument {
            format: "dxf",
            version: read.version.clone(),
            binary: read.binary,
            encoding: read.encoding,
            encoding_source: read.encoding_source,
            header: DxfHeader {
                insunits: 0.0,
                measurement: None,
                codepage: read.codepage.clone(),
                ltscale: 1.0,
                psltscale: 1.0,
                textsize: None,
                dimscale: 1.0,
                dimasz: 0.18,
                dimtxt: 0.18,
                tilemode: 1.0,
                extmin: None,
                extmax: None,
                limmin: None,
                limmax: None,
                plimmin: None,
                plimmax: None,
                variables: Vec::new(),
            },
            layers: Vec::new(),
            linetypes: Vec::new(),
            styles: Vec::new(),
            dimstyles: Vec::new(),
            mline_styles: Some(Vec::new()),
            blocks: Vec::new(),
            layouts: Vec::new(),
            entities: Vec::new(),
            sort_handles: Vec::new(),
            skipped: Vec::new(),
            notices: Vec::new(),
        },
        notices,
        block_records: HashMap::new(),
        layout_records: Vec::new(),
        sort_tables: Vec::new(),
        has_paper_entities: false,
    };
    parser.parse()
}
