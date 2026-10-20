//! AutoCAD の図面のモデル（`DxfDocument`）を DXF（ASCII、既定は AutoCAD 2013 形式の AC1027、UTF-8）に書く。
//! `write_dxf_as` で AutoCAD R12・2000〜2018 の版を選べる（R12 は先に `r12::prepare` で R12 の図形にする）。版にない値（2004 より前のトゥルーカラー・透過度・グラデーション・
//! 新しい寸法、2007 より前の MTEXT の背景）は別の形で書くか省き、notices で知らせる。2007 より前の版は文字列を図面の
//! コードページ（西欧のものなら、文字列を最も多く表せるもの）で書く（表せない文字は \U+XXXX。DXF Reference
//! 「Storage of String Values」）。
//! DXF・DWG から読んだ図面を DXF にする変換に使う（DXF Reference の各節・各図形のグループコードに従う）。
//!
//! 書くもの: 見出しの主な変数、クラス、表（VPORT・LTYPE・LAYER・STYLE・VIEW・UCS・APPID・DIMSTYLE・
//! BLOCK_RECORD）、ブロック定義、図形、OBJECTS の辞書（ACAD_GROUP・ACAD_LAYOUT・ACAD_MLEADERSTYLE・
//! ACAD_MLINESTYLE・ACAD_PLOTSETTINGS・ACAD_PLOTSTYLENAME）、レイアウト、MLINESTYLE、MLEADERSTYLE。
//! ハンドルは元の図面のものを保ち、ないもの・重なるものには新しいハンドルを振る。
//! 図形が参照するのに表にないレイヤ・線種・文字スタイル・寸法スタイル・ブロックは補う。
//! 書けなかった・形を変えて書いた内容は、読み取りと同じ notices（種類ごとの件数）で返す。

use crate::colors::nearest_aci;
use crate::model::{
    DxfBlock, DxfDocument, DxfEmbeddedMText, DxfEntity, DxfEntityBase, DxfEntityData, DxfHatch, DxfHatchEdge, DxfImage,
    DxfLayout, DxfMLeader, DxfMLine, DxfMLineStyle, DxfMText, DxfPolyline, DxfSpace, DxfTextBase, DxfViewport, Vec3,
};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use zumen_core::{Notice, NoticeLevel, NoticeList};

/// 書いた DXF と、書けなかった・形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DxfWritten {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}

/// 色の生の値の BYBLOCK（0xC1000000）
const BY_BLOCK_RAW: f64 = -1_056_964_608.0;

/// 書く DXF の版
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DxfVersion {
    R12,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl DxfVersion {
    pub const ALL: [DxfVersion; 7] = [
        DxfVersion::R12,
        DxfVersion::R2000,
        DxfVersion::R2004,
        DxfVersion::R2007,
        DxfVersion::R2010,
        DxfVersion::R2013,
        DxfVersion::R2018,
    ];

    /// $ACADVER の値
    pub fn code(self) -> &'static str {
        match self {
            DxfVersion::R12 => "AC1009",
            DxfVersion::R2000 => "AC1015",
            DxfVersion::R2004 => "AC1018",
            DxfVersion::R2007 => "AC1021",
            DxfVersion::R2010 => "AC1024",
            DxfVersion::R2013 => "AC1027",
            DxfVersion::R2018 => "AC1032",
        }
    }

    /// 人に見せる名前（R12、2000 など）
    pub fn name(self) -> &'static str {
        match self {
            DxfVersion::R12 => "R12",
            DxfVersion::R2000 => "2000",
            DxfVersion::R2004 => "2004",
            DxfVersion::R2007 => "2007",
            DxfVersion::R2010 => "2010",
            DxfVersion::R2013 => "2013",
            DxfVersion::R2018 => "2018",
        }
    }

    /// $ACADMAINTVER の値（ezdxf 1.4.5 の acad_maint_ver と同じ）
    fn maintenance(self) -> i32 {
        match self {
            DxfVersion::R12 | DxfVersion::R2004 => 0,
            DxfVersion::R2000 | DxfVersion::R2010 => 6,
            DxfVersion::R2007 => 25,
            DxfVersion::R2013 => 105,
            DxfVersion::R2018 => 4,
        }
    }

    /// 名前・$ACADVER の値から（R12・12・AC1009、R2000・2000・AC1015 など。大文字・小文字は問わない）
    pub fn parse(text: &str) -> Option<DxfVersion> {
        let text = text.trim().to_ascii_uppercase();
        let text = text.strip_prefix('R').unwrap_or(&text);
        DxfVersion::ALL.into_iter().find(|version| {
            version.code() == text || version.name().trim_start_matches('R') == text || version.code()[2..] == *text
        })
    }
}

/// DXF にする（AutoCAD 2013 形式）
pub fn write_dxf(doc: &DxfDocument) -> DxfWritten {
    let mut writer = Writer::new(doc, DxfVersion::R2013);
    writer.write();
    DxfWritten { bytes: writer.out.text.into_bytes(), notices: writer.notices.into_vec() }
}

/// 版を選んで DXF にする。R12 は、R12 にない図形を R12 の図形にしてから書く（`r12::prepare`）
pub fn write_dxf_as(doc: &DxfDocument, version: DxfVersion) -> DxfWritten {
    let prepared = (version == DxfVersion::R12).then(|| crate::r12::prepare(doc));
    let doc = prepared.as_ref().map_or(doc, |prepared| &prepared.doc);
    let mut writer = Writer::new(doc, version);
    if let Some(prepared) = &prepared {
        writer.notices = prepared.notices.clone();
    }
    writer.write();
    let text = std::mem::take(&mut writer.out.text);
    let bytes = if version >= DxfVersion::R2007 {
        text.replacen("CODEPAGE_PLACEHOLDER", "ANSI_1252", 1).into_bytes()
    } else {
        let (codepage, encoding) = codepage_for(doc, &text);
        encode_with_escapes(&text.replacen("CODEPAGE_PLACEHOLDER", codepage, 1), encoding)
    };
    DxfWritten { bytes, notices: writer.notices.into_vec() }
}

/// AutoCAD 2007 より前の版の $DWGCODEPAGE と文字コード。元の図面のコードページ（西欧以外）、読んだときの文字コード
/// （西欧以外）の順に決め、どちらもなければ文字列の文字を最も多く表せるものにする。同じ数なら PAGES の順で、1 バイトの
/// ものを先にする（キリル文字・ギリシャ文字は 932 にもあるが、1251・1253 にする）
fn codepage_for(doc: &DxfDocument, text: &str) -> (&'static str, &'static encoding_rs::Encoding) {
    const PAGES: [(&str, &str); 14] = [
        ("ANSI_1252", "windows-1252"),
        ("ANSI_1250", "windows-1250"),
        ("ANSI_1251", "windows-1251"),
        ("ANSI_1253", "windows-1253"),
        ("ANSI_1254", "windows-1254"),
        ("ANSI_1257", "windows-1257"),
        ("ANSI_1258", "windows-1258"),
        ("ANSI_1255", "windows-1255"),
        ("ANSI_1256", "windows-1256"),
        ("ANSI_874", "windows-874"),
        ("ANSI_932", "shift_jis"),
        ("ANSI_936", "gbk"),
        ("ANSI_949", "euc-kr"),
        ("ANSI_950", "big5"),
    ];
    let declared = doc.header.codepage.as_deref().and_then(crate::tags::codepage_encoding);
    let (name, label) = PAGES[1..]
        .iter()
        .find(|(_, label)| declared == Some(*label))
        .or_else(|| PAGES[1..].iter().find(|(_, label)| *label == doc.encoding))
        .copied()
        .unwrap_or_else(|| {
            let mut counts: HashMap<char, usize> = HashMap::new();
            for c in text.chars().filter(|c| !c.is_ascii()) {
                *counts.entry(c).or_default() += 1;
            }
            let unmappable = |label: &str| {
                let encoding = crate::tags::encoding(label);
                let mut buffer = [0u8; 4];
                counts
                    .iter()
                    .filter(|(c, _)| encoding.encode(c.encode_utf8(&mut buffer)).2)
                    .map(|(_, count)| count)
                    .sum::<usize>()
            };
            // 最も少ないもののうち最初のもの
            PAGES.into_iter().min_by_key(|(_, label)| unmappable(label)).unwrap_or(PAGES[0])
        });
    (name, crate::tags::encoding(label))
}

/// 文字コードにする。表せない文字は \U+XXXX にする（基本多言語面の外は UTF-16 のサロゲートの 2 つ。zumen の読み取りは
/// UTF-16 の単位で戻すので、元の文字になる）
fn encode_with_escapes(text: &str, encoding: &'static encoding_rs::Encoding) -> Vec<u8> {
    let mut encoder = encoding.new_encoder();
    let mut out = Vec::with_capacity(text.len() + 64);
    let mut rest = text;
    loop {
        let (result, read) = encoder.encode_from_utf8_to_vec_without_replacement(rest, &mut out, true);
        rest = &rest[read..];
        match result {
            encoding_rs::EncoderResult::InputEmpty => break,
            encoding_rs::EncoderResult::OutputFull => out.reserve(rest.len() + 64),
            encoding_rs::EncoderResult::Unmappable(c) => {
                for unit in c.encode_utf16(&mut [0u16; 2]) {
                    out.extend_from_slice(format!("\\U+{unit:04X}").as_bytes());
                }
            }
        }
    }
    out
}

/// レイヤの色（負は非表示）の符号のまま、色の番号を変える
fn aci_sign(color: f64, aci: f64) -> f64 {
    if color < 0.0 { -aci } else { aci }
}

/// グループコードと値の行
struct Out {
    text: String,
    /// R12 の書き方。サブクラスの印（100）と所有者などのハンドルの参照（330・360）は R13 からなので書かない
    r12: bool,
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

/// 1 行に書けない改行を、AutoCAD の ASCII の DXF と同じキャレット表記（^J・^M）にする
/// （バイナリの DXF・DWG から読んだ文字列は改行を持つことがある。ほかの制御文字はそのまま書く）
fn escape(value: &str) -> std::borrow::Cow<'_, str> {
    if value.contains(['\n', '\r']) { value.replace('\n', "^J").replace('\r', "^M").into() } else { value.into() }
}

/// 32 ビットの整数の値（色の生の値など）。DWG から読んだ符号なしの値も符号付きにする
fn int32(value: f64) -> i32 {
    if value.is_finite() { (value as i64) as u32 as i32 } else { 0 }
}

impl Out {
    fn tag(&mut self, code: i32, value: impl std::fmt::Display) {
        if self.r12 && matches!(code, 100 | 330 | 360) {
            return;
        }
        let _ = write!(self.text, "{code:>3}\r\n{value}\r\n");
    }

    /// 文字列（改行はキャレット表記にする）
    fn str(&mut self, code: i32, value: &str) {
        self.tag(code, escape(value));
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

/// 保つハンドルの上限。これより大きいハンドルは振り直す（次のハンドルを数えてもあふれない）
const MAX_HANDLE: u64 = i64::MAX as u64;

/// ハンドル。元の図面のハンドルを保ち（重なれば新しく振る）、新しいハンドルは元の最大より後から振る
struct Handles {
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

    /// 元のハンドル。読めないか、0 か、大きすぎるか、すでに使ったものなら新しく振る
    /// （新しいハンドルは元の最大より後なので重ならない）
    fn keep(&mut self, original: &str) -> u64 {
        match u64::from_str_radix(original, 16) {
            Ok(value) if value > 0 && value <= MAX_HANDLE && self.used.insert(value) => value,
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
    /// 書く版
    version: DxfVersion,
    /// 次に書く図形の色を、この ACI に変える（2004 より前の版のグラデーションの塗り）
    color_override: Option<f64>,
    out: Out,
    handles: Handles,
    notices: NoticeList,
    /// 名前（大文字）→ ブロックの定義
    blocks: Vec<BlockRecord>,
    block_index: HashMap<String, usize>,
    /// レイアウト（書く順）: 名前、ハンドル、ブロックの名前、元のレイアウト
    layouts: Vec<(String, u64, String, Option<&'a DxfLayout>)>,
    layer_handles: HashMap<String, u64>,
    /// 名前（大文字）→ 書いた文字スタイル・線種のハンドル
    style_handles: HashMap<String, u64>,
    linetype_handles: HashMap<String, u64>,
    /// 元のハンドル（大文字）→ 書いたハンドル。参照（文字スタイル・ブロック・属性定義など）を付け替える
    remap: HashMap<String, u64>,
    mline_styles: Vec<(u64, DxfMLineStyle)>,
    /// マルチ引出線のスタイル（Standard）
    mleader_style: u64,
    /// ブロックの配置の属性を書いている
    in_insert: bool,
    /// 図形（`DxfEntityBase` の場所）→ 書くハンドル。参照（属性定義・切り抜きの境界）を書く順に関係なく
    /// 付け替えられるよう、図形を書く前に決める
    entity_handles: HashMap<usize, u64>,
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

/// 図形のレイヤを、表示が初めて読む順に並べる。モデル空間の図形から、ブロック参照・寸法・表はその場で中の図形へ
/// 進み、最後にすべての図形。定義のないレイヤを、表示と同じ番号になる順に補うために使う（描画順の並べ替えは見ない）。
/// 表示は参照のレイヤを中の図形より先に数え、中のレイヤ 0 は参照のレイヤを継ぐので、ブロックは 1 度だけ読めばよい。
/// 深い入れ子でもスタックを使い切らないよう、再帰しない
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
    // 読んでいる図形の並び、次の位置、ブロックの中か
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
        // ブロックの中の図形の後に、ブロック参照の属性を読む
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

/// 点がいちばん近い線分（path の i 番目から i + 1 番目）の番号
fn nearest_segment(path: &[Vec3], point: &Vec3) -> usize {
    let distance = |a: &Vec3, b: &Vec3| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (a[0] + t * dx - point[0]).hypot(a[1] + t * dy - point[1])
    };
    path.windows(2)
        .enumerate()
        .min_by(|(_, x), (_, y)| distance(&x[0], &x[1]).total_cmp(&distance(&y[0], &y[1])))
        .map_or(0, |(index, _)| index)
}

const MODEL: &str = "*MODEL_SPACE";
const PAPER: &str = "*PAPER_SPACE";

fn upper(name: &str) -> String {
    name.to_uppercase()
}

impl<'a> Writer<'a> {
    fn new(doc: &'a DxfDocument, version: DxfVersion) -> Self {
        Self {
            doc,
            version,
            color_override: None,
            out: Out { text: String::new(), r12: version == DxfVersion::R12 },
            handles: Handles::new(doc),
            notices: NoticeList::new(),
            blocks: Vec::new(),
            block_index: HashMap::new(),
            layouts: Vec::new(),
            layer_handles: HashMap::new(),
            style_handles: HashMap::new(),
            linetype_handles: HashMap::new(),
            remap: HashMap::new(),
            mline_styles: Vec::new(),
            mleader_style: 0,
            in_insert: false,
            entity_handles: HashMap::new(),
            placeholder: 0,
        }
    }

    /// 元のハンドルを保つ（読めないか重なれば新しく振る）。参照を付け替えられるよう、元 → 書いたハンドルを覚える
    fn keep(&mut self, original: &str) -> u64 {
        let handle = self.handles.keep(original);
        if !original.is_empty() {
            self.remap.entry(upper(original)).or_insert(handle);
        }
        handle
    }

    /// 元のハンドルが指すものの、書いたハンドル
    fn mapped(&self, original: &str) -> Option<u64> {
        self.remap.get(&upper(original)).copied()
    }

    fn add_block(&mut self, name: &str, source: Option<&DxfBlock>) {
        let key = upper(name);
        if self.block_index.contains_key(&key) {
            return;
        }
        let handle = match source.and_then(|block| block.block_record_handle.as_deref()) {
            Some(original) => self.keep(original),
            None => self.handles.fresh(),
        };
        let begin = match source {
            Some(block) => self.keep(&block.handle),
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
                self.notices.add(
                    NoticeLevel::Warning,
                    "missing-block",
                    "定義のないブロックを、空のブロックとして補いました",
                );
                self.add_block(&name, None);
            }
        }
        // レイアウト: モデルと、紙空間のブロックごとに 1 つ（元のレイアウトがあればその名前と設定）
        let mut layouts: Vec<(String, u64, String, Option<&'a DxfLayout>)> = Vec::new();
        let model_layout = doc.layouts.iter().find(|layout| layout.model);
        let handle = match model_layout {
            Some(layout) => self.keep(&layout.handle),
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
                Some(layout) => self.keep(&layout.handle),
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
        // MLINESTYLE: STANDARD と図面のもの（MLINE が 340 で参照するので、図形より先に決める）
        let mut styles: Vec<(u64, DxfMLineStyle)> = Vec::new();
        let mut seen = HashSet::new();
        for style in doc.mline_styles.iter().flatten() {
            if seen.insert(upper(&style.name)) {
                let handle = self.keep(&style.handle);
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

    fn write(&mut self) {
        self.plan();
        self.placeholder = self.handles.fresh();
        self.mleader_style = self.handles.fresh();
        // 表と OBJECTS の固定のハンドル
        let tables: Vec<u64> = (0..9).map(|_| self.handles.fresh()).collect();
        let root = self.handles.fresh();
        self.header();
        // R12 はクラスと OBJECTS の節を持たない
        if !self.r12() {
            self.classes();
        }
        self.tables(&tables);
        for entity in all_entities(self.doc) {
            let handle = self.keep(&entity.base.handle);
            self.entity_handles.insert(std::ptr::from_ref(&entity.base) as usize, handle);
        }
        self.blocks_section();
        self.entities_section();
        if !self.r12() {
            self.objects(root);
        }
        self.out.tag(0, "EOF");
        // $HANDSEED は最後に決まるので、見出しの仮の値を置き換える
        let seed = format!("{:X}", self.handles.next + 1);
        self.out.text = self.out.text.replacen("HANDSEED_PLACEHOLDER", &seed, 1);
    }

    /// R12 の書き方か
    fn r12(&self) -> bool {
        self.version == DxfVersion::R12
    }

    fn section(&mut self, name: &str) {
        self.out.tag(0, "SECTION");
        self.out.tag(2, name);
    }

    fn header(&mut self) {
        let header = &self.doc.header;
        let r12 = self.r12();
        self.section("HEADER");
        let out = &mut self.out;
        out.tag(9, "$ACADVER");
        out.tag(1, self.version.code());
        // $ACADMAINTVER・$INSUNITS・$MEASUREMENT は 2000 から（ezdxf の headervars.py の mindxf）。R12 は図形に
        // ハンドルを付けることを $HANDLING で示す
        if !r12 {
            out.tag(9, "$ACADMAINTVER");
            out.tag(70, self.version.maintenance());
        }
        // AutoCAD 2007 以降は UTF-8 で、AutoCAD は値を見ない。それより前は書いた後に文字コードと一緒に決める
        out.tag(9, "$DWGCODEPAGE");
        out.tag(3, "CODEPAGE_PLACEHOLDER");
        if r12 {
            out.tag(9, "$HANDLING");
            out.tag(70, 1);
        }
        out.tag(9, "$HANDSEED");
        out.tag(5, "HANDSEED_PLACEHOLDER");
        if !r12 {
            out.tag(9, "$INSUNITS");
            out.int(70, header.insunits);
            if let Some(measurement) = header.measurement {
                out.tag(9, "$MEASUREMENT");
                out.int(70, measurement);
            }
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

    /// クラス（DXF Reference「CLASSES Section」）。AutoCAD は、ObjectARX で定義した図形・オブジェクトの
    /// クラスがないと図面を開けない。名前、C++ のクラス、アプリケーション、フラグ（90）は AutoCAD 2018 と ezdxf が
    /// 書く値で、図形のクラスは書く図形があるときだけ置く。91 は書く数
    fn classes(&mut self) {
        let doc = self.doc;
        let count = |matches: &dyn Fn(&DxfEntity) -> bool| doc.entities.iter().filter(|entity| matches(entity)).count();
        let mleaders = count(&|entity| matches!(entity.data, DxfEntityData::MLeader(_)));
        let wipeouts = count(&|entity| {
            matches!(entity.data, DxfEntityData::Image(_)) && entity.base.kind.eq_ignore_ascii_case("WIPEOUT")
        });
        // 円弧の長さ・大きい半径の寸法は 2004 から（ezdxf の ArcDimension と同じ）。前の版はブロックの参照で書く
        let old = self.version < DxfVersion::R2004;
        let dimensions = |kind: &str| {
            if old {
                return 0;
            }
            count(&|entity| {
                matches!(&entity.data, DxfEntityData::Dimension(dimension) if !dimension.anonymous_block_name.is_empty())
                    && entity.base.kind.eq_ignore_ascii_case(kind)
            })
        };
        let classes = [
            ("ACDBDICTIONARYWDFLT", "AcDbDictionaryWithDefault", "ObjectDBX Classes", 0, 1, false),
            ("ACDBPLACEHOLDER", "AcDbPlaceHolder", "ObjectDBX Classes", 0, 1, false),
            ("LAYOUT", "AcDbLayout", "ObjectDBX Classes", 0, self.layouts.len(), false),
            ("MLEADERSTYLE", "AcDbMLeaderStyle", "ACDB_MLEADERSTYLE_CLASS", 4095, 1, false),
            ("MULTILEADER", "AcDbMLeader", "ACDB_MLEADER_CLASS", 1025, mleaders, true),
            ("WIPEOUT", "AcDbWipeout", "WipeOut", 127, wipeouts, true),
            ("ARC_DIMENSION", "AcDbArcDimension", "ObjectDBX Classes", 1025, dimensions("ARC_DIMENSION"), true),
            (
                "LARGE_RADIAL_DIMENSION",
                "AcDbRadialDimensionLarge",
                "ObjectDBX Classes",
                1025,
                dimensions("LARGE_RADIAL_DIMENSION"),
                true,
            ),
        ];
        self.section("CLASSES");
        for (name, cpp, application, flags, instances, entity) in classes {
            if entity && instances == 0 {
                continue;
            }
            let out = &mut self.out;
            out.tag(0, "CLASS");
            out.tag(1, name);
            out.tag(2, cpp);
            out.tag(3, application);
            out.tag(90, flags);
            out.tag(91, instances);
            out.tag(280, 0);
            out.tag(281, i32::from(entity));
        }
        self.out.tag(0, "ENDSEC");
    }

    /// 表の始め。R12 の表と表の項目はハンドルを持たない
    fn table(&mut self, name: &str, handle: u64, count: usize) {
        self.out.tag(0, "TABLE");
        self.out.tag(2, name);
        if !self.r12() {
            self.out.handle(5, handle);
        }
        self.out.tag(330, 0);
        self.out.tag(100, "AcDbSymbolTable");
        self.out.tag(70, count);
        if name == "DIMSTYLE" {
            self.out.tag(100, "AcDbDimStyleTable");
        }
    }

    fn record(&mut self, kind: &str, handle: u64, owner: u64, subclass: &str, name: &str) {
        self.out.tag(0, kind);
        if !self.r12() {
            self.out.handle(if kind == "DIMSTYLE" { 105 } else { 5 }, handle);
        }
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
        for (code, value) in [(71, 0), (72, 1000), (73, 1), (74, 3), (75, 0), (76, 0), (77, 0), (78, 0)] {
            out.tag(code, value);
        }
        // UCS の値（281・65・146）は R13 から
        if !out.r12 {
            out.tag(281, 0);
            out.tag(65, 0);
            out.real(146, 0.0);
        }
        self.out.tag(0, "ENDTAB");

        // LTYPE: ByBlock・ByLayer・Continuous と、図面の線種。図形やレイヤが参照するのに定義がない線種は実線として補う
        struct Linetype<'b> {
            name: String,
            description: String,
            /// 1 周期の長さと、線（正）・空白（負）・点（0）
            length: f64,
            dashes: Vec<f64>,
            /// 元の線種のハンドル
            original: Option<&'b str>,
        }
        let plain = |name: String, original: Option<&'a str>| Linetype {
            name,
            description: String::new(),
            length: 0.0,
            dashes: Vec::new(),
            original,
        };
        let mut linetypes: Vec<Linetype> = Vec::new();
        let mut seen = HashSet::new();
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
        for name in ["ByBlock", "ByLayer", "Continuous"] {
            seen.insert(upper(name));
            let original = doc.linetypes.iter().find(|linetype| linetype.name.eq_ignore_ascii_case(name));
            linetypes.push(plain(spelling(name), original.map(|linetype| linetype.handle.as_str())));
        }
        // 文字・形を含む線種は、表示と同じく線と空白だけを書く（文字・形は捨てる）
        let mut complex = 0;
        for linetype in &doc.linetypes {
            if seen.insert(upper(&linetype.name)) {
                complex += u64::from(linetype.complex);
                linetypes.push(Linetype {
                    name: linetype.name.clone(),
                    description: linetype.description.clone(),
                    length: linetype.pattern_length,
                    dashes: linetype.dashes.clone(),
                    original: Some(&linetype.handle),
                });
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
                linetypes.push(plain(name.to_string(), None));
            }
        }
        self.table("LTYPE", ids[1], linetypes.len());
        for linetype in &linetypes {
            let handle = match linetype.original {
                Some(original) => self.keep(original),
                None => self.handles.fresh(),
            };
            self.linetype_handles.insert(upper(&linetype.name), handle);
            self.record("LTYPE", handle, ids[1], "AcDbLinetypeTableRecord", &linetype.name);
            let out = &mut self.out;
            out.tag(70, 0);
            out.str(3, &linetype.description);
            out.tag(72, 65);
            out.tag(73, linetype.dashes.len());
            out.real(40, if linetype.dashes.is_empty() { 0.0 } else { linetype.length });
            // 線・空白の種類（74。文字・形を含むか）は R13 から
            for dash in &linetype.dashes {
                out.real(49, *dash);
                if !out.r12 {
                    out.tag(74, 0);
                }
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
            transparency: Option<f64>,
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
                    transparency: layer.transparency,
                });
            }
        }
        // 定義のないレイヤは表示が読む順に補い（表示と同じ番号になる）、レイヤ 0 は必ず置く（番号が変わらないよう最後に）
        let names = layer_order(doc).into_iter().chain(std::iter::once("0"));
        for name in names {
            if !name.is_empty() && seen.insert(upper(name)) {
                layers.push(Layer {
                    name: name.to_string(),
                    handle: None,
                    color: 7.0,
                    true_color: None,
                    flags: 0,
                    // 表示が定義のないレイヤに使う線種と同じ綴り
                    linetype: "CONTINUOUS".into(),
                    plot: true,
                    lineweight: -3.0,
                    transparency: None,
                });
            }
        }
        self.table("LAYER", ids[2], layers.len());
        let mut lineweights = 0;
        for layer in &layers {
            let handle = match &layer.handle {
                Some(original) => self.keep(original),
                None => self.handles.fresh(),
            };
            self.layer_handles.insert(upper(&layer.name), handle);
            self.record("LAYER", handle, ids[2], "AcDbLayerTableRecord", &layer.name);
            let old = self.version < DxfVersion::R2004;
            let (color, true_color) = match layer.true_color {
                Some(true_color) if old => {
                    self.version_color();
                    (aci_sign(layer.color, nearest_aci(true_color)), None)
                }
                other => (layer.color, other),
            };
            let transparency = layer.transparency.filter(|_| !old);
            if old && layer.transparency.is_some() {
                self.version_transparency();
            }
            let out = &mut self.out;
            out.tag(70, layer.flags);
            out.int(62, color);
            if let Some(true_color) = true_color {
                out.int(420, true_color);
            }
            out.str(6, &layer.linetype);
            // 印刷するか（290）・線の太さ（370）・印刷スタイル（390）は 2000 から。R12 は線の太さを持たない
            if out.r12 {
                if layer.lineweight >= 0.0 {
                    lineweights += 1;
                }
                continue;
            }
            if !layer.plot {
                out.tag(290, 0);
            }
            out.int(370, layer.lineweight);
            out.handle(390, self.placeholder);
            // 透過度は拡張データ AcCmTransparency の 1071
            if let Some(transparency) = transparency {
                out.tag(1001, "AcCmTransparency");
                out.int(1071, transparency);
            }
        }
        if lineweights > 0 {
            self.version_lineweight(lineweights);
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
        let names = all_entities(doc).filter_map(|entity| match &entity.data {
            DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
                Some(text.style.as_str())
            }
            DxfEntityData::MText(mtext) => Some(mtext.style.as_str()),
            _ => None,
        });
        // Standard は必ず置く（図面の綴りがあればそれを使う）
        for name in names.chain(std::iter::once("Standard")) {
            if !name.is_empty() && seen.insert(upper(name)) {
                styles.push((name.to_string(), None));
            }
        }
        self.table("STYLE", ids[3], styles.len());
        let mut families = 0;
        for (name, style) in &styles {
            let handle = match style {
                Some(style) => self.keep(&style.handle),
                None => self.handles.fresh(),
            };
            self.style_handles.insert(upper(name), handle);
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
                    // TrueType のフォントの名前は拡張データ。R12 の表は拡張データを持たない（ezdxf の r12export）
                    if style.font_family.is_some() && out.r12 {
                        families += 1;
                    }
                    if let Some(family) = style.font_family.as_ref().filter(|_| !out.r12) {
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
        if families > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "version-font-family",
                "TrueType のフォント名（縦書きの @ を含む）は、AutoCAD R12 の文字スタイルに書けないため、フォントのファイルだけを書きました",
                families,
            );
        }

        // VIEW・UCS（空）、APPID
        self.table("VIEW", ids[4], 0);
        self.out.tag(0, "ENDTAB");
        self.table("UCS", ids[5], 0);
        self.out.tag(0, "ENDTAB");
        let mut apps = vec!["ACAD"];
        if self.doc.layers.iter().any(|layer| layer.transparency.is_some()) {
            apps.push("AcCmTransparency");
        }
        self.table("APPID", ids[6], apps.len());
        for app in apps {
            let handle = self.handles.fresh();
            self.record("APPID", handle, ids[6], "AcDbRegAppTableRecord", app);
            self.out.tag(70, 0);
        }
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
        let names = all_entities(doc).filter_map(|entity| match &entity.data {
            DxfEntityData::Dimension(dimension) => Some(dimension.dimstyle.as_str()),
            DxfEntityData::Leader(leader) => Some(leader.dimstyle.as_str()),
            _ => None,
        });
        for name in names {
            if !name.is_empty() && seen.insert(upper(name)) {
                dimstyles.push((name.to_string(), None));
            }
        }
        self.table("DIMSTYLE", ids[7], dimstyles.len());
        for (name, style) in &dimstyles {
            let handle = match style {
                Some(style) => self.keep(&style.handle),
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

        // BLOCK_RECORD（R13 から）
        if self.r12() {
            self.out.tag(0, "ENDSEC");
            return;
        }
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
        let mut contents: HashMap<String, Vec<&DxfEntity>> = HashMap::new();
        for entity in &doc.entities {
            contents.entry(Self::container(entity)).or_default().push(entity);
        }
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
            // R12 はモデル空間・紙空間のブロックを持たない（図形は ENTITIES 節にある）
            let r12 = self.r12();
            if r12 && (key == MODEL || paper) {
                continue;
            }
            self.out.tag(0, "BLOCK");
            if !r12 {
                self.out.handle(5, begin);
            }
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
                for entity in contents.get(&key).into_iter().flatten() {
                    self.entity(entity, handle, paper);
                }
            }
            self.out.tag(0, "ENDBLK");
            if !r12 {
                self.out.handle(5, end);
            }
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
        let (group, layout, mleader, mline, plot, plotstyle) = (
            self.handles.fresh(),
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
                ("ACAD_MLEADERSTYLE".into(), mleader),
                ("ACAD_MLINESTYLE".into(), mline),
                ("ACAD_PLOTSETTINGS".into(), plot),
                ("ACAD_PLOTSTYLENAME".into(), plotstyle),
            ],
        );
        self.dictionary(group, root, &[]);
        let entries: Vec<(String, u64)> =
            self.layouts.iter().map(|(name, handle, _, _)| (name.clone(), *handle)).collect();
        self.dictionary(layout, root, &entries);
        self.dictionary(mleader, root, &[("Standard".into(), self.mleader_style)]);
        self.mleader_style_object(mleader);
        let styles = self.mline_styles.clone();
        // 辞書の名前は AutoCAD と同じく Standard（スタイルの名前は STANDARD）
        let entries: Vec<(String, u64)> = styles
            .iter()
            .map(|(handle, style)| {
                let key = if upper(&style.name) == "STANDARD" { "Standard".into() } else { style.name.clone() };
                (key, *handle)
            })
            .collect();
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
        // 元のレイアウトの値はそのまま書く（用紙の大きさが 0 なら、表示は紙空間の範囲から用紙を決める）
        out.str(4, source.map_or("A3", |layout| layout.paper_size.as_str()));
        out.tag(6, "");
        let margins = source.map_or([7.5, 20.0, 7.5, 20.0], |layout| layout.margins);
        for (code, value) in [40, 41, 42, 43].into_iter().zip(margins) {
            out.real(code, value);
        }
        let (width, height) = source.map_or((420.0, 297.0), |layout| (layout.paper_width, layout.paper_height));
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

    /// マルチ引出線のスタイル Standard（ezdxf の雛形と同じ値）。マルチ引出線はすべての値を図形で上書きする
    fn mleader_style_object(&mut self, owner: u64) {
        let text_style = self.style_handles.get("STANDARD").copied().unwrap_or(0);
        let handle = self.mleader_style;
        let out = &mut self.out;
        out.tag(0, "MLEADERSTYLE");
        out.handle(5, handle);
        out.handle(330, owner);
        out.tag(100, "AcDbMLeaderStyle");
        for (code, value) in [
            (179, "2"),
            (170, "2"),
            (171, "1"),
            (172, "0"),
            (90, "2"),
            (40, "0.0"),
            (41, "0.0"),
            (173, "1"),
            (91, "-1056964608"),
            (92, "-2"),
            (290, "1"),
            (42, "2.0"),
            (291, "1"),
            (43, "8.0"),
            (3, "Standard"),
            (44, "4.0"),
            (300, ""),
        ] {
            out.tag(code, value);
        }
        out.handle(342, text_style);
        for (code, value) in [
            (174, "1"),
            (175, "1"),
            (176, "0"),
            (178, "1"),
            (93, "-1056964608"),
            (45, "4.0"),
            (292, "0"),
            (297, "0"),
            (46, "4.0"),
            (94, "-1056964608"),
            (47, "1.0"),
            (49, "1.0"),
            (140, "1.0"),
            (294, "1"),
            (141, "0.0"),
            (177, "0"),
            (142, "1.0"),
            (295, "0"),
            (296, "0"),
            (143, "3.75"),
            (271, "0"),
            (272, "9"),
            (273, "9"),
        ] {
            out.tag(code, value);
        }
    }

    /// マルチ引出線（DXF Reference「MLeader」。ezdxf の MultiLeader.export_entity と同じ順）。
    /// スタイルは Standard を指し、90 の上書きの印をすべて立てて図形の値を使わせる
    fn mleader(&mut self, base: &DxfEntityBase, mleader: &DxfMLeader, owner: u64, paper: bool) {
        let context = &mleader.context;
        let standard = self.style_handles.get("STANDARD").copied().unwrap_or(0);
        let text_style = context.mtext.as_ref().and_then(|mtext| self.mapped(&mtext.style_handle)).unwrap_or(standard);
        let block = context
            .block
            .as_ref()
            .and_then(|block| self.mapped(&block.block_record_handle).map(|handle| (block, handle)));
        if context.block.is_some() && block.is_none() {
            self.notices.add(
                NoticeLevel::Warning,
                "mleader-block",
                "定義のないブロックを内容とするマルチ引出線は、内容を省きました",
            );
        }
        let linetype = self
            .mapped(&mleader.leader_linetype_handle)
            .or_else(|| self.linetype_handles.get("BYBLOCK").copied())
            .unwrap_or(0);
        let arrow = if mleader.arrow_head_handle.is_empty() { None } else { self.mapped(&mleader.arrow_head_handle) };
        let arrow_heads: Vec<(f64, u64)> = mleader
            .arrow_heads
            .iter()
            .filter_map(|head| self.mapped(&head.handle).map(|handle| (head.index, handle)))
            .collect();
        let attribs: Vec<(u64, &crate::model::DxfMLeaderAttrib)> = mleader
            .block_attribs
            .iter()
            .filter_map(|attrib| self.mapped(&attrib.handle).map(|handle| (handle, attrib)))
            .collect();
        let style = self.mleader_style;
        // 文字の配置（MTEXT の 1〜9）から、左・中・右（0・1・2）
        let align = context.mtext.as_ref().map_or(0, |mtext| (int32(mtext.alignment).max(1) - 1) % 3);
        let scale = if context.scale != 0.0 { context.scale } else { 1.0 };
        self.common("MULTILEADER", base, owner, paper);
        let out = &mut self.out;
        out.tag(100, "AcDbMLeader");
        out.tag(270, 2);
        out.tag(300, "CONTEXT_DATA{");
        out.real(40, context.scale);
        out.point(10, &context.base_point);
        out.real(41, context.char_height);
        out.real(140, context.arrow_head_size);
        out.real(145, context.landing_gap_size);
        out.int(174, mleader.text_left_attachment_type);
        out.int(175, mleader.text_right_attachment_type);
        out.tag(176, align);
        out.tag(177, 0);
        match &context.mtext {
            Some(mtext) => {
                out.tag(290, 1);
                out.str(304, &mtext.default_content);
                out.point(11, &mtext.extrusion);
                out.handle(340, text_style);
                out.point(12, &mtext.insert);
                out.point(13, &mtext.text_direction);
                out.real(42, mtext.text_direction[1].atan2(mtext.text_direction[0]));
                out.real(43, mtext.width);
                out.real(44, 0.0);
                out.real(45, mtext.line_spacing_factor);
                out.int(170, mtext.line_spacing_style);
                out.tag(90, int32(mtext.color));
                out.int(171, mtext.alignment);
                out.int(172, mtext.flow_direction);
                out.tag(91, int32(mtext.bg_color));
                out.real(141, mtext.bg_scale_factor);
                out.tag(92, 0);
                out.tag(291, i32::from(mtext.use_window_bg_color));
                out.tag(292, i32::from(mtext.has_bg_fill));
                out.int(173, mtext.column_type);
                out.tag(293, 0);
                out.real(142, 0.0);
                out.real(143, 0.0);
                out.tag(294, 0);
                out.tag(295, 1);
            }
            None => out.tag(290, 0),
        }
        match block {
            Some((block, handle)) => {
                out.tag(296, 1);
                out.handle(341, handle);
                out.point(14, &block.extrusion);
                out.point(15, &block.insert);
                out.point(16, &block.scale);
                out.real(46, block.rotation);
                out.tag(93, int32(block.color));
                // 変換の行列は AutoCAD は使わないが、BricsCAD は求める（ezdxf の BlockData.export_dxf と同じく単位行列）
                for index in 0..16 {
                    out.real(47, if index % 5 == 0 { 1.0 } else { 0.0 });
                }
            }
            None => out.tag(296, 0),
        }
        out.point(110, &context.base_point);
        out.point(111, &[1.0, 0.0, 0.0]);
        out.point(112, &[0.0, 1.0, 0.0]);
        out.tag(297, 0);
        for leader in &context.leaders {
            out.tag(302, "LEADER{");
            out.tag(290, i32::from(leader.has_last_leader_line));
            out.tag(291, i32::from(leader.has_dogleg_vector));
            out.point(10, &leader.last_leader_point);
            out.point(11, &leader.dogleg_vector);
            for gap in &leader.breaks {
                out.point(12, &gap.start);
                out.point(13, &gap.end);
            }
            out.int(90, leader.index);
            out.real(40, leader.dogleg_length);
            for line in &leader.lines {
                out.tag(304, "LEADER_LINE{");
                for vertex in &line.vertices {
                    out.point(10, vertex);
                }
                // 切れ目は、始まりが乗る線分の番号（90）と始まり・終わり（11・12）
                let mut path = line.vertices.clone();
                path.push(leader.last_leader_point);
                for gap in &line.breaks {
                    out.tag(90, nearest_segment(&path, &gap.start));
                    out.point(11, &gap.start);
                    out.point(12, &gap.end);
                }
                out.int(91, line.index);
                out.tag(92, int32(line.color));
                out.tag(305, "}");
            }
            out.int(271, leader.attachment_direction);
            out.tag(303, "}");
        }
        out.int(272, mleader.text_top_attachment_type);
        out.int(273, mleader.text_bottom_attachment_type);
        out.tag(301, "}");
        out.handle(340, style);
        out.tag(90, 0x7FFF_FFFF);
        out.int(170, mleader.leader_type);
        out.tag(91, int32(mleader.leader_line_color));
        out.handle(341, linetype);
        out.int(171, mleader.leader_lineweight);
        out.tag(290, 1);
        out.tag(291, i32::from(mleader.has_dogleg));
        out.real(41, context.leaders.first().map_or(0.0, |leader| leader.dogleg_length) / scale);
        if let Some(arrow) = arrow {
            out.handle(342, arrow);
        }
        out.real(42, context.arrow_head_size / scale);
        out.int(172, mleader.content_type);
        out.handle(343, text_style);
        out.int(173, mleader.text_left_attachment_type);
        out.int(95, mleader.text_right_attachment_type);
        out.tag(174, 1);
        out.tag(175, align);
        out.tag(92, context.mtext.as_ref().map_or(int32(BY_BLOCK_RAW), |mtext| int32(mtext.color)));
        out.tag(292, i32::from(mleader.has_text_frame));
        if let Some((_, handle)) = block {
            out.handle(344, handle);
        }
        out.tag(93, block.map_or(int32(BY_BLOCK_RAW), |(block, _)| int32(block.color)));
        out.point(10, &block.map_or([1.0, 1.0, 1.0], |(block, _)| block.scale));
        out.real(43, block.map_or(0.0, |(block, _)| block.rotation));
        out.tag(176, 0);
        out.tag(293, 0);
        for (index, handle) in arrow_heads {
            out.int(94, index);
            out.handle(345, handle);
        }
        for (handle, attrib) in attribs {
            out.handle(330, handle);
            out.int(177, attrib.index);
            out.real(44, attrib.width);
            out.str(302, &attrib.text);
        }
        out.tag(294, 0);
        out.tag(178, 0);
        out.tag(179, align + 1);
        out.real(45, context.scale);
        out.int(271, context.leaders.first().map_or(0.0, |leader| leader.attachment_direction));
        out.int(272, mleader.text_bottom_attachment_type);
        out.int(273, mleader.text_top_attachment_type);
        out.tag(295, 0);
    }

    /// ワイプアウト（DXF Reference「WIPEOUT」。IMAGE と同じ値で、画像の定義は持たない）
    fn wipeout(&mut self, base: &DxfEntityBase, image: &DxfImage, owner: u64, paper: bool) {
        self.common("WIPEOUT", base, owner, paper);
        let out = &mut self.out;
        out.tag(100, "AcDbWipeout");
        out.tag(90, 0);
        out.point(10, &image.insert);
        out.point(11, &image.u_pixel);
        out.point(12, &image.v_pixel);
        out.point2(13, &image.image_size);
        out.tag(340, 0);
        out.int(70, image.flags);
        out.tag(280, i32::from(image.clipping));
        out.tag(281, 50);
        out.tag(282, 50);
        out.tag(283, 0);
        out.tag(360, 0);
        out.tag(71, if image.boundary.len() == 2 { 1 } else { 2 });
        out.tag(91, image.boundary.len());
        for vertex in &image.boundary {
            out.point2(14, vertex);
        }
        out.tag(290, 0);
    }

    /// 図形の共通の値（DXF Reference「Common Group Codes for Entities」）
    fn common(&mut self, kind: &str, base: &DxfEntityBase, owner: u64, paper: bool) -> u64 {
        let handle = match self.entity_handles.get(&(std::ptr::from_ref(base) as usize)) {
            Some(handle) => *handle,
            None => self.keep(&base.handle),
        };
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
        // 2004 より前の版はトゥルーカラー（420）・透過度（440）を持たない（ezdxf の DXFGraphic と同じ）
        let old = self.version < DxfVersion::R2004;
        let (color_index, true_color) = match (self.color_override.take(), base.true_color) {
            (Some(aci), _) => (aci, None),
            (None, Some(color)) if old => (nearest_aci(color), None),
            (None, color) => (base.color_index, color),
        };
        if color_index != 256.0 {
            out.int(62, color_index);
        }
        if let Some(color) = true_color {
            out.int(420, color);
        }
        let transparency = base.transparency.filter(|_| !old);
        if let Some(transparency) = transparency {
            out.int(440, transparency);
        }
        let dropped_color = old && base.true_color.is_some();
        let dropped_transparency = old && base.transparency.is_some();
        // 線の太さ（370）は 2000 から、線種の尺度（48）・非表示（60）は R13 から
        let r12 = out.r12;
        if base.lineweight != -1.0 && !r12 {
            out.int(370, base.lineweight);
        }
        if base.linetype_scale != 1.0 && !r12 {
            out.real(48, base.linetype_scale);
        }
        if base.invisible && !r12 {
            out.tag(60, 1);
        }
        if dropped_color {
            self.version_color();
        }
        if dropped_transparency {
            self.version_transparency();
        }
        if r12 && base.lineweight >= 0.0 {
            self.version_lineweight(1);
        }
        if r12 && base.linetype_scale != 1.0 {
            self.notices.add(
                NoticeLevel::Info,
                "version-linetype-scale",
                "図形ごとの線種の尺度は、AutoCAD R12 にないため、書きませんでした",
            );
        }
        handle
    }

    fn version_color(&mut self) {
        self.notices.add(
            NoticeLevel::Info,
            "version-color",
            "トゥルーカラーは、AutoCAD 2004 より前の版にないため、最も近い色の番号（ACI）で書きました",
        );
    }

    fn version_transparency(&mut self) {
        self.notices.add(
            NoticeLevel::Info,
            "version-transparency",
            "透過度は、AutoCAD 2004 より前の版にないため、書きませんでした",
        );
    }

    fn version_lineweight(&mut self, count: u64) {
        self.notices.add_count(
            NoticeLevel::Info,
            "version-lineweight",
            "線の太さは、AutoCAD R12 にないため、書きませんでした",
            count,
        );
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
        if kind == "IMAGE" {
            self.notices.add(
                NoticeLevel::Warning,
                "image",
                "画像（IMAGE）は、画像のファイルへの参照を読んでいないため省きました",
            );
            return;
        }
        // R12 に書く図面は、R12 にない図形を先に r12::prepare で R12 の図形にしてある
        if self.r12() && !r12_native(&entity.data) {
            self.notices.add(NoticeLevel::Warning, "version-skipped", "AutoCAD R12 に書けない図形を省きました");
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
                    self.notices.add(
                        NoticeLevel::Info,
                        "helix",
                        "らせん（HELIX）は、スプライン（SPLINE）として書きました",
                    );
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
            DxfEntityData::Text(text) => self.text(base, text, owner, paper),
            DxfEntityData::Attrib { text, mtext, .. } if !self.in_insert => {
                // ブロックの配置に属さない属性は DXF に書けないので、文字として書く（複数行の属性は MTEXT）。
                // 知らせは種類ごとに最初の文言を残すので、どちらにも合う 1 つの文言にする
                self.notices.add(
                    NoticeLevel::Info,
                    "attrib-alone",
                    "ブロック参照に属さない属性（ATTRIB）は、文字（TEXT・MTEXT）として書きました",
                );
                match mtext {
                    Some(embedded) => {
                        self.common("MTEXT", base, owner, paper);
                        self.out.tag(100, "AcDbMText");
                        self.mtext_body(&embedded.content, embedded.extrusion);
                    }
                    None => self.text(base, text, owner, paper),
                }
            }
            DxfEntityData::Attrib { text, tag, attribute_flags, mtext } => {
                self.common("ATTRIB", base, owner, paper);
                self.out.tag(100, "AcDbText");
                self.text_body(base, text);
                self.out.tag(100, "AcDbAttribute");
                if !self.r12() {
                    self.out.tag(280, 0);
                }
                self.out.str(2, tag);
                self.out.int(70, *attribute_flags);
                if text.valign != 0.0 {
                    self.out.int(74, text.valign);
                }
                self.embedded_mtext(mtext.as_deref(), 2);
            }
            DxfEntityData::Attdef { text, tag, prompt, attribute_flags, mtext } => {
                self.common("ATTDEF", base, owner, paper);
                self.out.tag(100, "AcDbText");
                self.text_body(base, text);
                self.out.tag(100, "AcDbAttributeDefinition");
                if !self.r12() {
                    self.out.tag(280, 0);
                }
                self.out.str(3, prompt);
                self.out.str(2, tag);
                self.out.int(70, *attribute_flags);
                if text.valign != 0.0 {
                    self.out.int(74, text.valign);
                }
                self.embedded_mtext(mtext.as_deref(), 4);
            }
            DxfEntityData::MText(mtext) => {
                self.common("MTEXT", base, owner, paper);
                self.out.tag(100, "AcDbMText");
                self.mtext_body(mtext, base.extrusion);
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
                    self.in_insert = true;
                    for attrib in &insert.attribs {
                        self.entity(attrib, handle, paper);
                    }
                    self.in_insert = false;
                    self.seqend(handle, handle, paper, base);
                }
            }
            // 表示のブロックを持たない寸法（AutoCAD の拘束寸法など）は DXF に書けない。表示でも省いている
            DxfEntityData::Dimension(dimension) if dimension.anonymous_block_name.is_empty() => {
                self.notices.add(NoticeLevel::Warning, "dimension-block", "表示のブロックがない寸法を省きました");
            }
            DxfEntityData::Dimension(dimension)
                if self.version < DxfVersion::R2004
                    && matches!(kind.as_str(), "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION") =>
            {
                self.notices.add(
                    NoticeLevel::Info,
                    "version-dimension",
                    "円弧の長さ・大きい半径の寸法は、AutoCAD 2004 より前の版にないため、寸法のブロックの参照（INSERT）として書きました",
                );
                self.common("INSERT", base, owner, paper);
                self.out.tag(100, "AcDbBlockReference");
                self.out.str(2, &dimension.anonymous_block_name);
                self.out.point(10, &[0.0, 0.0, 0.0]);
            }
            DxfEntityData::Dimension(dimension) => self.dimension(&kind, base, dimension, owner, paper),
            DxfEntityData::Hatch(hatch) => {
                if kind == "MPOLYGON" {
                    self.notices.add(NoticeLevel::Info, "mpolygon", "MPOLYGON は、ハッチング（HATCH）として書きました");
                }
                if hatch.gradient.is_some() && self.version < DxfVersion::R2004 {
                    // グラデーションは 2004 から。前の版は 1 色目の塗りにする
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-gradient",
                        "グラデーションは、AutoCAD 2004 より前の版にないため、1 色目の塗りで書きました",
                    );
                    let first = hatch.gradient_colors.as_deref().and_then(<[f64]>::first).copied();
                    self.color_override = first.map(nearest_aci);
                    let solid = DxfHatch { gradient: None, solid_fill: true, ..hatch.clone() };
                    self.hatch(base, &solid, owner, paper);
                } else {
                    self.hatch(base, hatch, owner, paper);
                }
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
            DxfEntityData::MLeader(mleader) => {
                if self.version < DxfVersion::R2007 {
                    // ezdxf と同じく書く（AutoCAD 2008 より前の AutoCAD は代理オブジェクトとして扱う）
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-mleader",
                        "マルチ引出線（MULTILEADER）は AutoCAD 2008 からで、それより前の版を読む CAD では表示されないことがあります",
                    );
                }
                self.mleader(base, mleader, owner, paper)
            }
            DxfEntityData::MLine(mline) => self.mline(base, mline, owner, paper),
            DxfEntityData::Viewport(viewport) if self.r12() => self.viewport_r12(base, viewport, owner, paper),
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
                // 凍結したレイヤと切り抜きの境界は、書いたレイヤ・図形のハンドルに付け替える（見つからなければ書かない）。
                // R12 から読んだ凍結したレイヤは名前で持つ
                for frozen in &viewport.frozen_layer_handles {
                    if let Some(handle) =
                        self.mapped(frozen).filter(|handle| self.layer_handles.values().any(|known| known == handle))
                    {
                        self.out.handle(331, handle);
                    }
                }
                for name in &viewport.frozen_layer_names {
                    if let Some(handle) = self.layer_handles.get(&upper(name)) {
                        self.out.handle(331, *handle);
                    }
                }
                self.out.int(90, viewport.status_flags);
                if let Some(handle) =
                    viewport.clipping_boundary_handle.as_deref().and_then(|handle| self.mapped(handle))
                {
                    self.out.handle(340, handle);
                }
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
            DxfEntityData::Image(image) => self.wipeout(base, image, owner, paper),
            DxfEntityData::Table(table) => {
                // 表（ACAD_TABLE）は、表示の図形を持つブロックの配置として書く
                self.notices.add(
                    NoticeLevel::Info,
                    "table",
                    "表（ACAD_TABLE）は、表示の図形を持つブロックの参照（INSERT）として書きました",
                );
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

    /// R12 のビューポート。表示の値は拡張データ ACAD の MVIEW に持つ（ezdxf の viewport.py の R12 と同じ並び。
    /// 読み取りの viewport_r12 が読み戻す）。このビューポートだけで凍結したレイヤは、名前（1003）で書く
    fn viewport_r12(&mut self, base: &DxfEntityBase, viewport: &DxfViewport, owner: u64, paper: bool) {
        self.common("VIEWPORT", base, owner, paper);
        let frozen: Vec<&str> = viewport
            .frozen_layer_handles
            .iter()
            .filter_map(|handle| self.doc.layers.iter().find(|layer| layer.handle.eq_ignore_ascii_case(handle)))
            .map(|layer| layer.name.as_str())
            .chain(viewport.frozen_layer_names.iter().map(String::as_str))
            .collect();
        let out = &mut self.out;
        out.point(10, &viewport.center);
        out.real(40, viewport.width);
        out.real(41, viewport.height);
        // R12 は表示しないビューポートを状態（68）の 0 で表す（2000 からは 90 の 0x20000）
        let off = js_int(viewport.status_flags) & 0x20000 != 0;
        out.int(68, if off { 0.0 } else { viewport.status });
        out.int(69, viewport.id);
        out.tag(1001, "ACAD");
        out.tag(1000, "MVIEW");
        out.tag(1002, "{");
        out.tag(1070, 16);
        out.point(1010, &viewport.view_target);
        out.point(1010, &viewport.view_direction);
        // ねじれの角度、表示の高さ、表示の中心、レンズの長さ、前と後ろの切り取り
        for value in [
            viewport.view_twist_angle,
            viewport.view_height,
            viewport.view_center[0],
            viewport.view_center[1],
            50.0,
            0.0,
            0.0,
        ] {
            out.real(1040, value);
        }
        // 表示の設定、円の滑らかさ、高速ズーム、UCS アイコン、スナップ、グリッド、スナップの種類と面
        for value in [0, 1000, 1, 1, 0, 0, 0, 0] {
            out.tag(1070, value);
        }
        // スナップの角度・基点・間隔、グリッドの間隔
        for value in [0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5] {
            out.real(1040, value);
        }
        out.tag(1070, 0);
        out.tag(1002, "{");
        for name in frozen {
            out.str(1003, name);
        }
        out.tag(1002, "}");
        out.tag(1002, "}");
    }

    /// TEXT・ATTRIB・ATTDEF の 1 行の文字（AcDbText）
    fn text(&mut self, base: &DxfEntityBase, text: &DxfTextBase, owner: u64, paper: bool) {
        self.common("TEXT", base, owner, paper);
        self.out.tag(100, "AcDbText");
        self.text_body(base, text);
        self.out.tag(100, "AcDbText");
        if text.valign != 0.0 {
            self.out.int(73, text.valign);
        }
    }

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
        match text.align_point {
            Some(point) => self.out.point(11, &point),
            None if text.halign != 0.0 || text.valign != 0.0 => self.out.point(11, &text.insert),
            None => {}
        }
        self.extrusion(base);
    }

    /// MTEXT の値（AcDbMText の後。属性に埋め込む MTEXT にも使う）
    fn mtext_body(&mut self, mtext: &DxfMText, extrusion: Option<Vec3>) {
        self.out.point(10, &mtext.insert);
        self.out.real(40, mtext.char_height);
        self.out.real(41, mtext.rect_width);
        self.out.int(71, if mtext.attachment_point >= 1.0 { mtext.attachment_point } else { 1.0 });
        self.out.int(72, if mtext.drawing_direction > 0.0 { mtext.drawing_direction } else { 1.0 });
        // 250 文字ごとに 3 に分け、最後を 1 に書く
        let raw = if mtext.raw_text.is_empty() { mtext.text.replace('\n', "\\P") } else { mtext.raw_text.clone() };
        let chars: Vec<char> = escape(&raw).chars().collect();
        let mut chunks = chars.chunks(250).peekable();
        if chunks.peek().is_none() {
            self.out.tag(1, "");
        }
        while let Some(chunk) = chunks.next() {
            let text: String = chunk.iter().collect();
            self.out.tag(if chunks.peek().is_some() { 3 } else { 1 }, text);
        }
        self.out.str(7, if mtext.style.is_empty() { "Standard" } else { &mtext.style });
        if let Some(extrusion) = extrusion.filter(|value| *value != [0.0, 0.0, 1.0]) {
            self.out.point(210, &extrusion);
        }
        match mtext.text_direction {
            Some(direction) => self.out.point(11, &direction),
            None if mtext.rotation != 0.0 => self.out.real(50, mtext.rotation),
            None => {}
        }
        self.out.int(73, if mtext.line_spacing_style > 0.0 { mtext.line_spacing_style } else { 1.0 });
        self.out.real(44, if mtext.line_spacing_factor > 0.0 { mtext.line_spacing_factor } else { 1.0 });
        if mtext.background_flags != 0.0 && self.version < DxfVersion::R2007 {
            self.notices.add(
                NoticeLevel::Info,
                "version-background",
                "複数行の文字の背景は、AutoCAD 2007 より前の版にないため、書きませんでした",
            );
        } else if mtext.background_flags != 0.0 {
            self.out.int(90, mtext.background_flags);
            if let Some(color) = mtext.background_color_index {
                self.out.int(63, color);
            }
            if let Some(color) = mtext.background_true_color {
                self.out.int(421, color);
            }
            if let Some(scale) = mtext.background_scale {
                self.out.real(45, scale);
            }
            self.out.tag(441, 0);
        }
    }

    /// 複数行の属性（AutoCAD 2018 以降）の文字。AutoCAD と同じく、属性の種類（71。2 は属性、4 は属性定義）の後の
    /// 「101 Embedded Object」に MTEXT の値を続ける。1 行の文字（AcDbText の 1）は読んだまま（AutoCAD は空にする）
    fn embedded_mtext(&mut self, embedded: Option<&DxfEmbeddedMText>, attribute_type: i64) {
        let Some(embedded) = embedded.filter(|_| !self.r12()) else { return };
        self.out.tag(71, attribute_type);
        self.out.tag(101, "Embedded Object");
        self.mtext_body(&embedded.content, embedded.extrusion);
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
        // 版（280）は 2010 から、文字の位置合わせ（71）・実測値（42）は 2000 から
        let r12 = self.r12();
        if !r12 {
            self.out.tag(280, 0);
        }
        self.out.str(2, &dimension.anonymous_block_name);
        self.out.point(10, &dimension.defpoint);
        self.out.point(11, &dimension.text_midpoint);
        if let Some(insert) = dimension.insert {
            self.out.point(12, &insert);
        }
        self.out.int(70, dimension.dimtype);
        if let Some(attachment) = dimension.attachment_point.filter(|_| !r12) {
            self.out.int(71, attachment);
        }
        self.out.str(1, &dimension.text);
        if let Some(measurement) = dimension.actual_measurement.filter(|_| !r12) {
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
        // 定義点（13〜16）と角度は、元の図面にあるものだけを書く
        let points = [dimension.defpoint2, dimension.defpoint3, dimension.defpoint4, dimension.defpoint5];
        let point = |out: &mut Out, code: i32| {
            if let Some(point) = points[(code - 13) as usize] {
                out.point(code, &point);
            }
        };
        let out = &mut self.out;
        match name {
            "ARC_DIMENSION" => {
                out.tag(100, "AcDbArcDimension");
                point(out, 13);
                point(out, 14);
                point(out, 15);
                out.tag(70, 0);
                out.real(40, 0.0);
                out.real(41, 0.0);
                out.tag(71, 0);
            }
            "LARGE_RADIAL_DIMENSION" => {
                out.tag(100, "AcDbRadialDimensionLarge");
                point(out, 13);
                point(out, 14);
                point(out, 15);
                out.real(40, 0.0);
            }
            _ => match js_int(dimension.dimtype) & 7 {
                0 | 1 => {
                    out.tag(100, "AcDbAlignedDimension");
                    point(out, 13);
                    point(out, 14);
                    if js_int(dimension.dimtype) & 7 == 0 {
                        if let Some(angle) = dimension.angle {
                            out.real(50, angle);
                        }
                        if let Some(oblique) = dimension.oblique_angle {
                            out.real(52, oblique);
                        }
                        out.tag(100, "AcDbRotatedDimension");
                    }
                }
                2 => {
                    out.tag(100, "AcDb2LineAngularDimension");
                    for code in 13..=16 {
                        point(out, code);
                    }
                }
                3 => {
                    out.tag(100, "AcDbDiametricDimension");
                    point(out, 15);
                    out.real(40, 0.0);
                }
                4 => {
                    out.tag(100, "AcDbRadialDimension");
                    point(out, 15);
                    out.real(40, 0.0);
                }
                5 => {
                    out.tag(100, "AcDb3PointAngularDimension");
                    point(out, 13);
                    point(out, 14);
                    point(out, 15);
                }
                _ => {
                    out.tag(100, "AcDbOrdinateDimension");
                    point(out, 13);
                    point(out, 14);
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
            if !hatch.pattern_name.is_empty() {
                &hatch.pattern_name
            } else if solid {
                "SOLID"
            } else {
                "ANSI31"
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
        // 模様の種類（76）: 0 利用者定義、1 定義済み、2 カスタム
        out.int(76, hatch.pattern_type);
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
        // グラデーション（DXF Reference「HATCH」の 450〜470。AutoCAD と同じ順）
        if let Some(name) = &hatch.gradient {
            let colors = hatch.gradient_colors.as_deref().unwrap_or(&[]);
            out.tag(450, 1);
            out.tag(451, 0);
            out.real(460, hatch.gradient_angle);
            out.real(461, hatch.gradient_shift);
            out.tag(452, i32::from(hatch.gradient_one_color));
            out.real(462, hatch.gradient_tint);
            out.tag(453, colors.len());
            for (index, color) in colors.iter().enumerate() {
                out.real(463, index as f64);
                out.int(421, *color);
            }
            out.str(470, if name.is_empty() { "LINEAR" } else { name });
        }
    }

    fn mline(&mut self, base: &DxfEntityBase, mline: &DxfMLine, owner: u64, paper: bool) {
        // スタイルは名前で探し、なければ STANDARD
        let style =
            self.mline_style_handle(&mline.style_name).or_else(|| self.mline_style_handle("STANDARD")).unwrap_or(0);
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
/// R12 にある図形か（表はブロックの参照として、円弧の長さ・大きい半径の寸法はブロックの参照として書く）
fn r12_native(data: &DxfEntityData) -> bool {
    match data {
        DxfEntityData::Line { .. }
        | DxfEntityData::Circle { .. }
        | DxfEntityData::Arc { .. }
        | DxfEntityData::Point { .. }
        | DxfEntityData::Polyline(_)
        | DxfEntityData::Text(_)
        | DxfEntityData::Insert(_)
        | DxfEntityData::Dimension(_)
        | DxfEntityData::Solid { .. }
        | DxfEntityData::Viewport(_)
        | DxfEntityData::Table(_) => true,
        DxfEntityData::Attrib { mtext, .. } | DxfEntityData::Attdef { mtext, .. } => mtext.is_none(),
        _ => false,
    }
}

fn js_int(value: f64) -> i64 {
    i64::from(zumen_core::js::to_int32(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_dxf;
    use std::path::Path;

    /// (グループコード, 値) の並びを ASCII の DXF にする
    fn dxf(tags: &[(i32, &str)]) -> Vec<u8> {
        tags.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect::<String>().into_bytes()
    }

    /// 1 つの節（TABLES・ENTITIES など）だけの図面
    fn section(name: &str, tags: &[(i32, &str)]) -> Vec<u8> {
        let mut all = vec![(0, "SECTION"), (2, name)];
        all.extend_from_slice(tags);
        all.extend([(0, "ENDSEC"), (0, "EOF")]);
        dxf(&all)
    }

    /// 書いた DXF のタグ（グループコードと値）
    fn tags(written: &DxfWritten) -> Vec<(i32, String)> {
        let text = String::from_utf8(written.bytes.clone()).unwrap();
        let lines: Vec<&str> = text.split("\r\n").collect();
        lines.as_chunks::<2>().0.iter().map(|[code, value]| (code.trim().parse().unwrap(), value.to_string())).collect()
    }

    fn codes(written: &DxfWritten) -> Vec<&str> {
        written.notices.iter().map(|notice| notice.code.as_str()).collect()
    }

    #[test]
    fn reads_version_names() {
        for (text, version) in [
            ("2000", DxfVersion::R2000),
            ("R2004", DxfVersion::R2004),
            ("ac1021", DxfVersion::R2007),
            ("r12", DxfVersion::R12),
            ("AC1032", DxfVersion::R2018),
        ] {
            assert_eq!(DxfVersion::parse(text), Some(version), "{text}");
        }
        for text in ["R13", "2005", "", "AC1012"] {
            assert_eq!(DxfVersion::parse(text), None, "{text}");
        }
    }

    #[test]
    fn writes_characters_the_code_page_lacks_as_unicode_escapes() {
        // 基本多言語面の外の文字は UTF-16 のサロゲートの 2 つにし、読み取りで元の文字に戻る
        let bytes = encode_with_escapes("壁 Ā 😀", encoding_rs::SHIFT_JIS);
        let (expected, _, _) = encoding_rs::SHIFT_JIS.encode("壁 \\U+0100 \\U+D83D\\U+DE00");
        assert_eq!(bytes, expected.into_owned());
        assert_eq!(crate::tags::decode_escapes("\\U+0100 \\U+D83D\\U+DE00"), "Ā 😀");
    }

    #[test]
    fn writes_old_versions_in_the_code_page_that_holds_the_texts() {
        // AutoCAD 2000 の DXF。文字列はそのコードページで書き、ない文字は \U+XXXX にする
        let text = |content: &str, codepage: &str| {
            let mut dxf =
                format!("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DWGCODEPAGE\n3\n{codepage}\n").into_bytes();
            dxf.extend_from_slice(b"0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nTEXT\n8\n0\n10\n0\n20\n0\n40\n1\n1\n");
            let encoding = crate::tags::encoding(crate::tags::codepage_encoding(codepage).unwrap());
            dxf.extend_from_slice(&encode_with_escapes(content, encoding));
            dxf.extend_from_slice(b"\n0\nENDSEC\n0\nEOF\n");
            parse_dxf(&dxf)
        };
        // 西欧のコードページの図面は、文字を最も多く表せるもの（同じならキリル文字・ギリシャ文字は 1 バイトのもの）で書く。
        // 西欧以外のコードページの図面は、そのコードページで書く
        for (content, codepage, page, label) in [
            ("Wall", "ANSI_1252", "ANSI_1252", "windows-1252"),
            ("Café 25°", "ANSI_1252", "ANSI_1252", "windows-1252"),
            ("壁 φ25", "ANSI_1252", "ANSI_932", "shift_jis"),
            ("图层", "ANSI_1252", "ANSI_936", "gbk"),
            ("벽 漢字", "ANSI_1252", "ANSI_949", "euc-kr"),
            ("Стена", "ANSI_1252", "ANSI_1251", "windows-1251"),
            ("Τοίχος", "ANSI_1252", "ANSI_1253", "windows-1253"),
            ("Ściana", "ANSI_1252", "ANSI_1250", "windows-1250"),
            ("Wall", "ANSI_932", "ANSI_932", "shift_jis"),
            ("壁", "GB2312", "ANSI_936", "gbk"),
        ] {
            let written = write_dxf_as(&text(content, codepage), DxfVersion::R2000);
            let decoded = crate::tags::decode(&written.bytes, label);
            assert!(decoded.contains(&format!("$DWGCODEPAGE\r\n  3\r\n{page}\r\n")), "{content}: {decoded}");
            assert!(decoded.contains(&format!("\r\n  1\r\n{content}\r\n")), "{content}: {decoded}");
            let again = parse_dxf(&written.bytes);
            let DxfEntityData::Text(text) = &again.entities[0].data else { panic!("{content}") };
            assert_eq!(text.text, content);
        }
    }

    #[test]
    fn writes_numbers_that_read_back_exactly() {
        assert_eq!(real(1.0), "1.0");
        assert_eq!(real(-0.5), "-0.5");
        assert_eq!(real(1e20), "1e20");
        assert_eq!(real(2.5e-12), "2.5e-12");
        assert_eq!(real(f64::NAN), "0.0");
        for value in [0.1 + 0.2, 1.0 / 3.0, 283.481_000_000_417_4, 1e-9, 123_456_789.012_345] {
            assert_eq!(real(value).parse::<f64>().unwrap(), value);
        }
    }

    #[test]
    fn keeps_handles_and_renumbers_the_unusable_ones() {
        // 同じハンドルの線が 2 本、ハンドルのない線、16 進でないハンドルの線
        let line = |handle: &'static str, x: &'static str| {
            let mut tags = vec![(0, "LINE")];
            if !handle.is_empty() {
                tags.push((5, handle));
            }
            tags.extend([(8, "0"), (10, "0"), (20, "0"), (11, x), (21, "0")]);
            tags
        };
        let body: Vec<(i32, &str)> =
            [line("A0", "1"), line("A0", "2"), line("", "3"), line("ZZ", "4")].into_iter().flatten().collect();
        let written = write_dxf(&parse_dxf(&section("ENTITIES", &body)));
        let again = parse_dxf(&written.bytes);
        let handles: Vec<&str> = again.entities.iter().map(|entity| entity.base.handle.as_str()).collect();
        assert_eq!(handles.len(), 4);
        assert_eq!(handles[0], "A0");
        assert_eq!(handles.iter().collect::<HashSet<_>>().len(), 4, "{handles:?}");
    }

    /// 書いた DXF の構造: ハンドルは重ならず $HANDSEED より小さく、参照（330・340〜349・350〜369・390 など）は
    /// 0 か書いたものを指す
    fn check_structure(name: &str, written: &DxfWritten) {
        let tags = tags(written);
        // 見出しの $HANDSEED の値も 5 で書く
        let at = tags.iter().position(|(code, value)| *code == 9 && value == "$HANDSEED").unwrap() + 1;
        let seed = u64::from_str_radix(&tags[at].1, 16).unwrap();
        let mut handles = HashSet::new();
        for (index, (code, value)) in tags.iter().enumerate() {
            if (*code == 5 || *code == 105) && index != at {
                assert!(handles.insert(u64::from_str_radix(value, 16).unwrap()), "{name}: handle {value} twice");
            }
        }
        assert!(handles.iter().all(|handle| *handle < seed), "{name}: $HANDSEED {seed:X}");
        for (code, value) in &tags {
            if matches!(code, 330..=369 | 390 | 1005) && value != "0" {
                let target = u64::from_str_radix(value, 16).unwrap();
                assert!(handles.contains(&target), "{name}: {code} points at {value}, which is not written");
            }
        }
    }

    #[test]
    fn writes_unique_handles_and_references_that_resolve() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf");
        let mut checked = 0;
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|extension| extension == "dxf") {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                check_structure(&name, &write_dxf(&parse_dxf(&std::fs::read(&path).unwrap())));
                checked += 1;
            }
        }
        assert!(checked >= 10);
    }

    /// R12 の図面: クラス・OBJECTS・ブロックの記録の表・サブクラスの印・所有者の参照がなく、表の項目はハンドルを持たず、
    /// 図形は R12 にある種類だけで、ハンドルは重ならない
    #[test]
    fn writes_r12_with_only_what_r12_has() {
        const R12_KINDS: [&str; 19] = [
            "LINE",
            "POINT",
            "CIRCLE",
            "ARC",
            "TRACE",
            "SOLID",
            "TEXT",
            "SHAPE",
            "INSERT",
            "ATTDEF",
            "ATTRIB",
            "POLYLINE",
            "VERTEX",
            "SEQEND",
            "3DFACE",
            "DIMENSION",
            "VIEWPORT",
            "BLOCK",
            "ENDBLK",
        ];
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf");
        let mut checked = 0;
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "dxf") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let written = write_dxf_as(&parse_dxf(&std::fs::read(&path).unwrap()), DxfVersion::R12);
            // 文字コードは図面のコードページなので、構造だけを見る
            let text = String::from_utf8_lossy(&written.bytes).into_owned();
            let lines: Vec<&str> = text.split("\r\n").collect();
            let tags: Vec<(i32, &str)> =
                lines.as_chunks::<2>().0.iter().map(|[code, value]| (code.trim().parse().unwrap(), *value)).collect();
            let at = |variable: &str| tags.iter().position(|tag| *tag == (9, variable)).map(|index| tags[index + 1].1);
            assert_eq!(at("$ACADVER"), Some("AC1009"), "{name}");
            assert_eq!(at("$HANDLING"), Some("1"), "{name}");
            assert_eq!(at("$INSUNITS"), None, "{name}");
            let mut section = "";
            let mut seed = 0;
            let mut handles = HashSet::new();
            for (index, &(code, value)) in tags.iter().enumerate() {
                assert!(!matches!(code, 100 | 102 | 330 | 360 | 370 | 420 | 440), "{name}: {code} {value}");
                if code == 2 && index > 0 && tags[index - 1] == (0, "SECTION") {
                    section = value;
                    assert!(matches!(value, "HEADER" | "TABLES" | "BLOCKS" | "ENTITIES"), "{name}: {value}");
                }
                if code == 2 && index > 0 && tags[index - 1] == (0, "TABLE") {
                    assert!(value != "BLOCK_RECORD", "{name}");
                }
                if code == 5 && index > 0 && tags[index - 1] == (9, "$HANDSEED") {
                    seed = u64::from_str_radix(value, 16).unwrap();
                    continue;
                }
                if code == 0
                    && matches!(section, "BLOCKS" | "ENTITIES")
                    && !matches!(value, "SECTION" | "ENDSEC" | "EOF")
                {
                    assert!(R12_KINDS.contains(&value), "{name}: {value}");
                }
                if code == 2 && index > 0 && tags[index - 1].1 == "BLOCK" {
                    assert!(!value.to_uppercase().starts_with("*MODEL_SPACE"), "{name}");
                    assert!(!value.to_uppercase().starts_with("*PAPER_SPACE"), "{name}");
                }
                if code == 5 || code == 105 {
                    assert!(section == "BLOCKS" || section == "ENTITIES", "{name}: a handle in {section}");
                    assert!(handles.insert(u64::from_str_radix(value, 16).unwrap()), "{name}: handle {value} twice");
                }
            }
            assert!(handles.iter().all(|handle| *handle < seed), "{name}: $HANDSEED {seed:X}");
            checked += 1;
        }
        assert!(checked >= 10);
    }

    /// R12 のビューポートは表示の値と凍結したレイヤの名前を拡張データ MVIEW に持ち、表示しないものは状態 0
    #[test]
    fn writes_r12_viewports_in_their_extended_data() {
        let viewport = |handle: &'static str, status_flags: &'static str| {
            vec![
                (0, "VIEWPORT"),
                (5, handle),
                (67, "1"),
                (8, "0"),
                (10, "100"),
                (20, "80"),
                (40, "120"),
                (41, "90"),
                (68, "2"),
                (69, "2"),
                (12, "30"),
                (22, "40"),
                (17, "1"),
                (27, "2"),
                (45, "60"),
                (51, "15"),
                (90, status_flags),
                (331, "20"),
            ]
        };
        let mut body = vec![(0, "TABLE"), (2, "LAYER"), (0, "LAYER"), (5, "20"), (2, "壁"), (70, "0"), (0, "ENDTAB")];
        body.insert(0, (0, "SECTION"));
        body.insert(1, (2, "TABLES"));
        body.extend([(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
        body.extend(viewport("A1", "0"));
        body.extend(viewport("A2", "131072"));
        body.extend([(0, "ENDSEC"), (0, "EOF")]);
        let written = write_dxf_as(&parse_dxf(&dxf(&body)), DxfVersion::R12);
        let again = parse_dxf(&written.bytes);
        let viewports: Vec<&DxfViewport> = again
            .entities
            .iter()
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Viewport(viewport) => Some(viewport),
                _ => None,
            })
            .collect();
        assert_eq!(viewports.len(), 2);
        let first = viewports[0];
        assert_eq!((first.view_center, first.view_target, first.view_height), ([30.0, 40.0], [1.0, 2.0, 0.0], 60.0));
        assert_eq!((first.view_twist_angle, first.status), (15.0, 2.0));
        assert_eq!(first.frozen_layer_names, ["壁"]);
        // 表示しないビューポート（90 の 0x20000）は、R12 では状態 0
        assert_eq!(viewports[1].status, 0.0);
    }

    #[test]
    fn adds_missing_definitions_in_the_order_the_scene_numbers_them() {
        // 表のない図面: レイヤ B・A、文字スタイル GOTHIC、定義のないブロック NOWHERE
        let written = write_dxf(&parse_dxf(&section(
            "ENTITIES",
            &[
                (0, "LINE"),
                (8, "B"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "1"),
                (0, "TEXT"),
                (8, "A"),
                (7, "GOTHIC"),
                (10, "0"),
                (20, "0"),
                (40, "2.5"),
                (1, "X"),
                (0, "INSERT"),
                (8, "B"),
                (2, "NOWHERE"),
                (10, "0"),
                (20, "0"),
            ],
        )));
        let again = parse_dxf(&written.bytes);
        let layers: Vec<&str> = again.layers.iter().map(|layer| layer.name.as_str()).collect();
        assert_eq!(layers, ["B", "A", "0"]);
        assert!(again.styles.iter().any(|style| style.name == "GOTHIC"));
        assert!(again.blocks.iter().any(|block| block.name == "NOWHERE"));
        assert_eq!(codes(&written), ["missing-block"]);
    }

    #[test]
    fn writes_line_breaks_in_caret_notation() {
        // バイナリの DXF・DWG から読んだ文字列は改行を持つことがある。ASCII の DXF の 1 行に書けないので ^J・^M にする
        let mut doc = parse_dxf(&section(
            "ENTITIES",
            &[(0, "TEXT"), (5, "20"), (8, "0"), (10, "0"), (20, "0"), (40, "1"), (1, "x")],
        ));
        if let DxfEntityData::Text(text) = &mut doc.entities[0].data {
            text.text = "a\nb\rc".into();
        }
        let written = write_dxf(&doc);
        assert!(tags(&written).contains(&(1, "a^Jb^Mc".to_string())));
    }

    #[test]
    fn writes_the_dashes_of_linetypes_with_texts_or_shapes() {
        let written = write_dxf(&parse_dxf(&section(
            "TABLES",
            &[
                (0, "TABLE"),
                (2, "LTYPE"),
                (70, "1"),
                (0, "LTYPE"),
                (5, "14"),
                (100, "AcDbSymbolTableRecord"),
                (100, "AcDbLinetypeTableRecord"),
                (2, "BORDER_TEXT"),
                (70, "0"),
                (3, "border with text"),
                (72, "65"),
                (73, "2"),
                (40, "1.0"),
                (49, "0.75"),
                (74, "0"),
                (49, "-0.25"),
                (74, "2"),
                (75, "0"),
                (340, "0"),
                (46, "1.0"),
                (50, "0.0"),
                (44, "0.0"),
                (45, "0.0"),
                (9, "X"),
                (0, "ENDTAB"),
            ],
        )));
        let again = parse_dxf(&written.bytes);
        let linetype = again.linetypes.iter().find(|linetype| linetype.name == "BORDER_TEXT").unwrap();
        assert_eq!((linetype.dashes.as_slice(), linetype.complex), ([0.75, -0.25].as_slice(), false));
        assert_eq!(codes(&written), ["complex-linetype"]);
    }

    #[test]
    fn writes_gradients_and_wipeouts_as_they_are() {
        let body = [
            (0, "HATCH"),
            (5, "30"),
            (8, "0"),
            (100, "AcDbEntity"),
            (100, "AcDbHatch"),
            (10, "0"),
            (20, "0"),
            (30, "0"),
            (210, "0"),
            (220, "0"),
            (230, "1"),
            (2, "SOLID"),
            (70, "1"),
            (71, "0"),
            (91, "1"),
            (92, "2"),
            (72, "0"),
            (73, "1"),
            (93, "3"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (10, "10"),
            (20, "10"),
            (97, "0"),
            (75, "0"),
            (76, "1"),
            (98, "0"),
            (450, "1"),
            (451, "0"),
            (460, "0.5"),
            (461, "0.25"),
            (452, "1"),
            (462, "0.75"),
            (453, "2"),
            (463, "0"),
            (421, "255"),
            (463, "1"),
            (421, "16776960"),
            (470, "CYLINDER"),
            (0, "WIPEOUT"),
            (5, "31"),
            (8, "0"),
            (100, "AcDbEntity"),
            (100, "AcDbWipeout"),
            (90, "0"),
            (10, "1"),
            (20, "2"),
            (30, "0"),
            (11, "10"),
            (21, "0"),
            (31, "0"),
            (12, "0"),
            (22, "5"),
            (32, "0"),
            (13, "1"),
            (23, "1"),
            (70, "7"),
            (280, "1"),
            (71, "2"),
            (91, "4"),
            (14, "-0.5"),
            (24, "-0.5"),
            (14, "0.5"),
            (24, "-0.5"),
            (14, "0.5"),
            (24, "0.5"),
            (14, "-0.5"),
            (24, "-0.5"),
        ];
        let doc = parse_dxf(&section("ENTITIES", &body));
        let written = write_dxf(&doc);
        let again = parse_dxf(&written.bytes);
        assert_eq!(again.entities.len(), 2);
        for (before, after) in doc.entities.iter().zip(&again.entities) {
            assert_eq!(before.data, after.data, "{}", before.base.kind);
        }
        assert!(
            matches!(&again.entities[0].data, DxfEntityData::Hatch(hatch) if hatch.gradient.as_deref() == Some("CYLINDER"))
        );
        assert_eq!(codes(&written), Vec::<&str>::new());
    }

    #[test]
    fn writes_multileaders_with_a_standard_style() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf/mleader.dxf");
        let doc = parse_dxf(&std::fs::read(path).unwrap());
        let written = write_dxf(&doc);
        let again = parse_dxf(&written.bytes);
        let mleader = |doc: &DxfDocument| {
            doc.entities
                .iter()
                .find_map(|entity| match &entity.data {
                    DxfEntityData::MLeader(mleader) => Some(mleader.as_ref().clone()),
                    _ => None,
                })
                .unwrap()
        };
        let (before, after) = (mleader(&doc), mleader(&again));
        // スタイル・文字スタイル・線種は、書いた MLEADERSTYLE・STYLE・LTYPE を指す
        assert_eq!(before.context.leaders, after.context.leaders);
        assert_eq!(
            before.context.mtext.as_ref().map(|mtext| &mtext.default_content),
            after.context.mtext.as_ref().map(|mtext| &mtext.default_content)
        );
        let tags = tags(&written);
        let style = tags.iter().position(|(code, value)| *code == 0 && value == "MLEADERSTYLE").unwrap();
        assert_eq!(tags[style + 1].1, after.style_handle);
    }

    /// CLASSES 節のクラスと数
    fn classes(written: &DxfWritten) -> Vec<(String, String)> {
        let tags = tags(written);
        tags.iter()
            .enumerate()
            .filter(|(_, (code, value))| *code == 0 && value == "CLASS")
            .map(|(index, _)| (tags[index + 1].1.clone(), tags[index + 5].1.clone()))
            .collect()
    }

    #[test]
    fn declares_the_classes_of_the_objects_written() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf/features.dxf");
        let written = write_dxf(&parse_dxf(&std::fs::read(path).unwrap()));
        let declared = classes(&written);
        for (name, count) in [("MLEADERSTYLE", "1"), ("MULTILEADER", "5"), ("WIPEOUT", "1"), ("LAYOUT", "2")] {
            assert!(declared.contains(&(name.to_string(), count.to_string())), "{name} {count} not in {declared:?}");
        }
        // 書かない図形のクラスは置かない
        let empty = write_dxf(&parse_dxf(&section("ENTITIES", &[])));
        let names: Vec<String> = classes(&empty).into_iter().map(|(name, _)| name).collect();
        assert_eq!(names, ["ACDBDICTIONARYWDFLT", "ACDBPLACEHOLDER", "LAYOUT", "MLEADERSTYLE"]);
    }

    #[test]
    fn orders_missing_layers_of_deeply_nested_blocks_without_recursion() {
        // ブロック B0 が B1 を、B1 が B2 を … と 20,000 段参照し、それぞれ定義のないレイヤ Lk の線を持つ
        let depth = 20_000;
        let mut text = String::from("0\nSECTION\n2\nBLOCKS\n");
        for k in 0..depth {
            let _ = write!(text, "0\nBLOCK\n8\n0\n2\nB{k}\n70\n0\n10\n0\n20\n0\n");
            let _ = write!(text, "0\nLINE\n8\nL{k}\n10\n0\n20\n0\n11\n1\n21\n0\n");
            if k + 1 < depth {
                let _ = write!(text, "0\nINSERT\n8\n0\n2\nB{}\n10\n0\n20\n0\n", k + 1);
            }
            text.push_str("0\nENDBLK\n");
        }
        text.push_str(
            "0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nINSERT\n8\nTOP\n2\nB0\n10\n0\n20\n0\n0\nENDSEC\n0\nEOF\n",
        );
        let again = parse_dxf(&write_dxf(&parse_dxf(text.as_bytes())).bytes);
        let layers: Vec<&str> = again.layers.iter().map(|layer| layer.name.as_str()).collect();
        assert_eq!(layers.len(), depth + 2);
        assert_eq!(&layers[..3], ["TOP", "L0", "L1"]);
        assert_eq!(layers.last(), Some(&"0"));
    }

    #[test]
    fn reports_entities_written_differently_or_left_out() {
        let written = write_dxf(&parse_dxf(&section(
            "ENTITIES",
            &[
                // ブロック参照に属さない属性は文字にする
                (0, "ATTRIB"),
                (5, "40"),
                (8, "0"),
                (100, "AcDbEntity"),
                (100, "AcDbText"),
                (10, "0"),
                (20, "0"),
                (40, "1"),
                (1, "ALONE"),
                (100, "AcDbAttribute"),
                (2, "TAG"),
                (70, "0"),
                // 表示のブロックがない寸法と、画像は書かない
                (0, "DIMENSION"),
                (5, "41"),
                (8, "0"),
                (100, "AcDbEntity"),
                (100, "AcDbDimension"),
                (10, "0"),
                (20, "0"),
                (11, "5"),
                (21, "1"),
                (70, "0"),
                (0, "IMAGE"),
                (5, "42"),
                (8, "0"),
                (100, "AcDbEntity"),
                (100, "AcDbRasterImage"),
                (10, "0"),
                (20, "0"),
                (11, "1"),
                (21, "0"),
                (12, "0"),
                (22, "1"),
                (13, "10"),
                (23, "10"),
            ],
        )));
        let again = parse_dxf(&written.bytes);
        let kinds: Vec<&str> = again.entities.iter().map(|entity| entity.base.kind.as_str()).collect();
        assert_eq!(kinds, ["TEXT"]);
        assert_eq!(codes(&written), ["attrib-alone", "dimension-block", "image"]);
        assert_eq!(
            written.notices[0].message,
            "ブロック参照に属さない属性（ATTRIB）は、文字（TEXT・MTEXT）として書きました"
        );
    }
}
