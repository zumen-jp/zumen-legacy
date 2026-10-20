//! R13 より前の DWG（AutoCAD 1.0〜Release 12）を、DXF から読んだときと同じ AutoCAD の図面のモデル（DxfDocument）に
//! する。R13 からの形（オブジェクトの表とビット単位の値）とは違い、表は決まった大きさの記録の並び、図形は種類・モードと
//! 大きさを持つバイト単位の記録の並び（records.rs）。表の項目と図形は番号で引き合い、ハンドルは R10 から図形だけが持つ。
//!
//! - AC1.50（AutoCAD 2.0）から: ファイルの頭が図形・ブロックの図形・追加の図形の節と表の位置を持つ。追加の図形の節は、
//!   後から足した図形を JUMP の記録でつなぐ
//! - AutoCAD 1.x（MC0.0・AC1.2・AC1.40）: ヘッダ変数の後に図形が並び、ブロックの定義も同じ並びの中。レイヤは 0〜127 の
//!   番号で、ヘッダ変数が色を持つ

mod records;

use crate::document::{add_paper_layout, codepage_name};
use crate::entities::{deg, hex, mview};
use crate::objects::{Eed, caret_notation};
use records::{Body, Cursor, Record, Step, Vars, kind};
use std::collections::{HashMap, HashSet};
use zumen_core::{NoticeLevel, NoticeList, limits};
use zumen_dxf::*;

/// R13 より前の版（ファイルの頭の文字列）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Old {
    /// AutoCAD 1.0
    Mc00,
    /// AutoCAD 1.2
    Ac12,
    /// AutoCAD 1.4
    Ac140,
    /// AutoCAD 2.0
    Ac150,
    /// AutoCAD 2.1
    Ac210,
    /// AutoCAD 2.2〜2.4
    Ac1001,
    /// AutoCAD 2.5
    Ac1002,
    /// AutoCAD 2.6
    Ac1003,
    /// Release 9
    Ac1004,
    /// Release 10
    Ac1006,
    /// Release 11・12
    Ac1009,
}

/// 版の文字列。ファイルの頭の 12 バイトに、NUL で終えて置く
const MAGICS: [(&str, Old); 11] = [
    ("MC0.0", Old::Mc00),
    ("AC1.2", Old::Ac12),
    ("AC1.40", Old::Ac140),
    ("AC1.50", Old::Ac150),
    ("AC2.10", Old::Ac210),
    ("AC1001", Old::Ac1001),
    ("AC1002", Old::Ac1002),
    ("AC1003", Old::Ac1003),
    ("AC1004", Old::Ac1004),
    ("AC1006", Old::Ac1006),
    ("AC1009", Old::Ac1009),
];

impl Old {
    pub fn from_magic(bytes: &[u8]) -> Option<Self> {
        MAGICS.iter().find(|(magic, _)| is_magic(bytes, magic)).map(|(_, version)| *version)
    }

    pub fn name(self) -> &'static str {
        MAGICS.iter().find(|(_, version)| *version == self).map_or("", |(magic, _)| magic)
    }
}

/// 版の文字列が NUL で終わる（AC1001〜AC1009 は R13 からと同じ 6 文字）
fn is_magic(bytes: &[u8], magic: &str) -> bool {
    bytes.starts_with(magic.as_bytes()) && bytes.get(magic.len()).is_none_or(|next| *next == 0 || magic.len() == 6)
}

/// R13 より前の DWG の版の文字列で始まるか（DWG の見分け。AC1001〜AC1009 は R13 からの見分けと同じ形）
pub(crate) fn is_old_dwg(bytes: &[u8]) -> bool {
    MAGICS[..5].iter().any(|(magic, _)| is_magic(bytes, magic) && bytes.get(magic.len()) == Some(&0))
}

/// 読む図形の数の上限（JUMP が輪になっていても止まる）
const MAX_RECORDS: usize = 1_000_000;
const MAX_WALK_BYTES: usize = 64 * 1024 * 1024;

/// 読んだ表と図形（文字列は文字コードを決める前のバイト列）
#[derive(Default)]
struct Parsed {
    layers: Vec<Option<records::Layer>>,
    /// AutoCAD 1.x のレイヤ（0〜127）の色
    early_layers: Option<Vec<i16>>,
    linetypes: Vec<Option<records::Linetype>>,
    styles: Vec<Option<records::Style>>,
    dimstyles: Vec<Option<records::DimStyle>>,
    /// ブロックの表の項目（AutoCAD 1.x は BLOCK の名前）と、BLOCK から ENDBLK までの記録
    blocks: Vec<(Vec<u8>, u8, Vec<Record>)>,
    entities: Vec<Record>,
    /// 記録の位置 → ファイルの中の記録の番号（1 から。ハンドルのない図形の id）
    numbers: HashMap<usize, u32>,
    /// 読めなかった記録の数
    broken: u64,
}

/// R13 より前の DWG を読む
pub(crate) fn read_old(bytes: &[u8], version: Old) -> Result<DxfDocument, String> {
    let (vars, parsed) = if version >= Old::Ac150 { sectioned(bytes, version)? } else { early(bytes, version)? };
    Ok(build(version, vars, parsed))
}

// --- AC1.50 から ---------------------------------------------------------------

fn sectioned(bytes: &[u8], version: Old) -> Result<(Vars, Parsed), String> {
    let header =
        records::file_header(bytes, version).map_err(|_| "DWG のファイルの頭が途中で切れています".to_string())?;
    let vars = records::header_vars(bytes, version).map_err(|_| "DWG のヘッダ変数が読めません".to_string())?;
    let tables = &header.tables;
    let mut parsed = Parsed {
        layers: records::records(bytes, tables.layer, |c| records::layer(c, version)),
        linetypes: records::records(bytes, tables.linetype, |c| records::linetype(c, version)),
        styles: records::records(bytes, tables.style, |c| records::style(c, version)),
        dimstyles: records::records(bytes, tables.dimstyle, |c| records::dimstyle(c, version)),
        ..Parsed::default()
    };
    // 記録の番号: 図形・ブロックの図形・追加の図形の節の順に、消した図形と JUMP も数える
    let mut number = 0u32;
    for section in [header.entities, header.blocks, header.extras] {
        let mut position = section.start;
        while section.start > 0 && position < section.end && (number as usize) < MAX_RECORDS {
            if limits::tick() {
                break;
            }
            let Ok((_, next)) = records::record(bytes, position, section.end, version) else { break };
            number += 1;
            parsed.numbers.insert(position, number);
            position = next;
        }
    }
    parsed.broken += parsed.layers.iter().filter(|r| r.is_none()).count() as u64
        + parsed.linetypes.iter().filter(|r| r.is_none()).count() as u64
        + parsed.styles.iter().filter(|r| r.is_none()).count() as u64
        + parsed.dimstyles.iter().filter(|r| r.is_none()).count() as u64;
    let mut walker = Walker { bytes, version, header: &header, broken: 0, remaining: MAX_RECORDS, walked_bytes: 0 };
    for block in records::records(bytes, tables.block, |c| records::block(c, version)) {
        // 消したブロックも番号を数える（INSERT はブロックの表の中の番号で引く）
        let Some(block) = block else {
            parsed.broken += 1;
            parsed.blocks.push((Vec::new(), 0x80, Vec::new()));
            continue;
        };
        let mut definition = Vec::new();
        if block.entry.live()
            && let Some(start) = walker.block_start(block.address)
        {
            walker.walk(start, None, |record| {
                let end = record.kind == kind::ENDBLK;
                definition.push(record);
                end
            });
        }
        parsed.blocks.push((block.entry.name, block.entry.flags, definition));
    }
    let entities = header.entities;
    if entities.start > 0 {
        walker.walk(entities.start, Some(entities.end), |record| {
            parsed.entities.push(record);
            false
        });
    }
    parsed.broken += walker.broken;
    Ok((vars, parsed))
}

/// 図形の記録を、JUMP をたどりながら順に読む
struct Walker<'a> {
    bytes: &'a [u8],
    version: Old,
    header: &'a records::FileHeader,
    broken: u64,
    remaining: usize,
    walked_bytes: usize,
}

impl Walker<'_> {
    /// ブロックの表の BLOCK の位置。上位のバイトの 0x40 か、ブロックの図形の節より前の位置は、節の中の位置
    fn block_start(&self, raw: u32) -> Option<usize> {
        let blocks = self.header.blocks;
        let mut address = (raw & 0x00ff_ffff) as usize;
        if (raw >> 24) & 0x40 != 0 || address < blocks.start {
            address += blocks.start;
        }
        blocks.contains(address).then_some(address)
    }

    /// JUMP の行き先。上位のバイトの 0x80 は追加の図形、0x40 はブロックの図形の節の中の位置、0 はファイルの中の位置
    fn jump_target(&self, raw: u32) -> usize {
        let address = (raw & 0x00ff_ffff) as usize;
        match raw >> 24 {
            0x80 => self.header.extras.start + address,
            0x40 => self.header.blocks.start + address,
            _ => address,
        }
    }

    /// 記録を含む節
    fn section(&self, position: usize) -> Option<records::Section> {
        [self.header.entities, self.header.blocks, self.header.extras]
            .into_iter()
            .find(|section| section.contains(position))
    }

    /// start から記録を読み、visit に渡す。end（図形の節の終わり）に着くか、visit が真を返す（ENDBLK）と止まる。
    /// 追加の図形の節の終わりに JUMP で戻る記録がなければ、最後の JUMP の次へ戻る（ACadSharp の派生版と同じ）
    fn walk(&mut self, start: usize, end: Option<usize>, mut visit: impl FnMut(Record) -> bool) {
        let mut position = start;
        let mut back: Option<usize> = None;
        let mut seen: HashSet<usize> = HashSet::new();
        while Some(position) != end {
            if self.remaining == 0 || self.walked_bytes >= MAX_WALK_BYTES {
                self.broken += 1;
                break;
            }
            self.remaining -= 1;
            // 読み取りの上限（zumen_core::limits）
            if zumen_core::limits::tick() {
                break;
            }
            let Some(section) = self.section(position).filter(|_| seen.insert(position)) else {
                self.broken += 1;
                match back.take() {
                    Some(to) if !seen.contains(&to) => {
                        position = to;
                        continue;
                    }
                    _ => break,
                }
            };
            let result = records::record(self.bytes, position, section.end, self.version);
            if let Ok((_, next)) = &result {
                self.walked_bytes += next - position;
            }
            match result {
                Ok((Step::Record(record), next)) => {
                    position = next;
                    if visit(*record) {
                        break;
                    }
                }
                Ok((Step::Deleted, next)) => position = next,
                Ok((Step::Jump(raw), next)) => {
                    back = Some(next);
                    position = self.jump_target(raw);
                    continue;
                }
                Err(_) => {
                    self.broken += 1;
                    break;
                }
            }
            if position == self.header.extras.end
                && let Some(to) = back.take()
            {
                position = to;
            }
        }
    }
}

// --- AutoCAD 1.x -----------------------------------------------------------------

fn early(bytes: &[u8], version: Old) -> Result<(Vars, Parsed), String> {
    let header = records::early_header(bytes, version).map_err(|_| "DWG のヘッダ変数が読めません".to_string())?;
    let mut parsed = Parsed { early_layers: Some(header.layer_colors), ..Parsed::default() };
    let mut c = Cursor::new(bytes, header.start, bytes.len());
    // BLOCK から ENDBLK までがブロックの定義。記録の番号は並びの順（消した図形も数える）
    let mut block: Option<(Vec<u8>, Vec<Record>)> = None;
    for number in 1..=header.entities.min(MAX_RECORDS) {
        if zumen_core::limits::tick() {
            break;
        }
        let position = c.pos;
        let record = match records::early_record(&mut c) {
            Ok(Some(record)) => {
                parsed.numbers.insert(position, number as u32);
                record
            }
            Ok(None) => continue,
            Err(_) => {
                parsed.broken += 1;
                break;
            }
        };
        match (&mut block, &record.body) {
            (None, Body::Block { name, .. }) => {
                let name = name.clone().unwrap_or_default();
                block = Some((name, vec![record]));
            }
            (Some((_, list)), Body::EndBlock) => {
                list.push(record);
                if let Some((name, list)) = block.take() {
                    parsed.blocks.push((name, 0, list));
                }
            }
            (Some((_, list)), _) => list.push(record),
            (None, _) => parsed.entities.push(record),
        }
    }
    Ok((header.vars, parsed))
}

// --- 図面のモデルにする -----------------------------------------------------------

/// 文字列を図面の文字コードで読む
struct Text {
    encoding: &'static encoding_rs::Encoding,
}

impl Text {
    fn string(&self, bytes: &[u8]) -> String {
        caret_notation(self.encoding.decode_without_bom_handling(bytes).0.into_owned())
    }

    /// DXF の文字列と同じく、\U+XXXX などを読んだ文字
    fn escaped(&self, bytes: &[u8]) -> String {
        decode_escapes(&self.string(bytes))
    }
}

/// 日本語の文字列らしいか（DXF の読み取りと同じ。全角のかなが 1 字か、漢字が 2 字以上）
fn looks_japanese(text: &str) -> bool {
    if text.chars().any(|c| ('\u{3041}'..='\u{30fa}').contains(&c)) {
        return true;
    }
    text.chars().filter(|c| ('\u{3400}'..='\u{4dbf}').contains(c) || ('\u{4e00}'..='\u{9fff}').contains(c)).count() >= 2
}

/// 文字列のバイト（文字コードを決めるのに使う）
fn text_bytes(parsed: &Parsed) -> Vec<u8> {
    let mut out = Vec::new();
    let mut push = |bytes: &[u8]| {
        out.extend_from_slice(bytes);
        out.push(b'\n');
    };
    for layer in parsed.layers.iter().flatten() {
        push(&layer.entry.name);
    }
    for linetype in parsed.linetypes.iter().flatten() {
        push(&linetype.entry.name);
        push(&linetype.description);
    }
    for (name, _, _) in &parsed.blocks {
        push(name);
    }
    let records = parsed.entities.iter().chain(parsed.blocks.iter().flat_map(|(_, _, list)| list));
    for record in records {
        if limits::tick() {
            break;
        }
        match &record.body {
            Body::Text(text) => push(&text.value),
            Body::Attdef { text, prompt, tag, .. } => {
                push(&text.value);
                push(prompt);
                push(tag);
            }
            Body::Attrib { text, tag, .. } => {
                push(&text.value);
                push(tag);
            }
            Body::Dimension(dimension) => push(&dimension.text),
            _ => {}
        }
    }
    out
}

/// 文字コード。R11 からは $DWGCODEPAGE（R13 からと同じ番号）。指定がなければ、DXF の読み取りと同じく、Shift_JIS として
/// 正しく読めて日本語を含むなら Shift_JIS、ほかは Windows-1252
fn choose_encoding(
    vars: &Vars,
    parsed: &Parsed,
    notices: &mut NoticeList,
) -> (&'static str, EncodingSource, Option<String>) {
    let name = codepage_name(vars.codepage);
    if let Some(name) = name
        && let Some(label) = codepage_encoding(name)
    {
        return (label, EncodingSource::Codepage, Some(name.to_string()));
    }
    if vars.codepage != 0 {
        notices.add(
            NoticeLevel::Warning,
            "unknown-codepage",
            format!(
                "対応していない文字コード {} の文字列を西欧の文字として読みました",
                name.map_or_else(|| vars.codepage.to_string(), str::to_string)
            ),
        );
        return ("windows-1252", EncodingSource::Default, name.map(str::to_string));
    }
    let bytes = text_bytes(parsed);
    if bytes.iter().any(|byte| *byte >= 0x80)
        && let Some(text) = encoding_rs::SHIFT_JIS.decode_without_bom_handling_and_without_replacement(&bytes)
        && looks_japanese(&text)
    {
        notices.add(
            NoticeLevel::Info,
            "guessed-shift-jis",
            "文字コードの指定がないため、日本語（Shift_JIS）と判断して読みました",
        );
        return ("shift_jis", EncodingSource::Detected, None);
    }
    ("windows-1252", EncodingSource::Default, None)
}

/// 名前で引く表
struct Names {
    layers: Vec<String>,
    linetypes: Vec<String>,
    styles: Vec<String>,
    dimstyles: Vec<String>,
    blocks: Vec<String>,
    r11: bool,
}

impl Names {
    fn layer(&self, index: i16) -> String {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.layers.get(index))
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "0".into())
    }

    /// 線種。R11 は 32767 が BYLAYER、32766 が BYBLOCK、R10 までは 255 が BYBLOCK（ACadSharp の派生版と同じ）
    fn linetype(&self, index: Option<i16>) -> String {
        match index {
            None => "BYLAYER".into(),
            Some(32767) if self.r11 => "BYLAYER".into(),
            Some(32766) if self.r11 => "BYBLOCK".into(),
            Some(255) if !self.r11 => "BYBLOCK".into(),
            Some(index) => usize::try_from(index)
                .ok()
                .and_then(|index| self.linetypes.get(index))
                .filter(|name| !name.is_empty())
                .cloned()
                .unwrap_or_else(|| "BYLAYER".into()),
        }
    }

    fn style(&self, index: u8) -> String {
        self.styles
            .get(usize::from(index))
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "STANDARD".into())
    }

    fn dimstyle(&self, index: Option<i16>) -> String {
        index
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.dimstyles.get(index))
            .filter(|name| !name.is_empty())
            .cloned()
            .unwrap_or_else(|| "STANDARD".into())
    }

    fn block(&self, index: i16) -> String {
        usize::try_from(index).ok().and_then(|index| self.blocks.get(index)).cloned().unwrap_or_default()
    }
}

fn build(version: Old, vars: Vars, parsed: Parsed) -> DxfDocument {
    let mut notices = NoticeList::new();
    let (encoding, encoding_source, codepage) = choose_encoding(&vars, &parsed, &mut notices);
    let text =
        Text { encoding: encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252) };
    let r11 = version >= Old::Ac1009;
    let name_of =
        |entry: Option<&records::Entry>| entry.filter(|entry| entry.live()).map(|entry| text.string(&entry.name));
    let mut names = Names {
        layers: parsed
            .layers
            .iter()
            .map(|layer| name_of(layer.as_ref().map(|layer| &layer.entry)).unwrap_or_default())
            .collect(),
        linetypes: parsed
            .linetypes
            .iter()
            .map(|linetype| name_of(linetype.as_ref().map(|linetype| &linetype.entry)).unwrap_or_default())
            .collect(),
        styles: parsed
            .styles
            .iter()
            .map(|style| name_of(style.as_ref().map(|style| &style.entry)).unwrap_or_default())
            .collect(),
        dimstyles: parsed
            .dimstyles
            .iter()
            .map(|style| name_of(style.as_ref().map(|style| &style.entry)).unwrap_or_default())
            .collect(),
        blocks: parsed
            .blocks
            .iter()
            .enumerate()
            .map(|(index, (name, _, _))| block_name(text.string(name), index))
            .collect(),
        r11,
    };
    let mut doc = DxfDocument {
        format: "dwg",
        version: version.name().to_string(),
        binary: true,
        encoding,
        encoding_source,
        header: header(version, &vars, codepage),
        layers: Vec::new(),
        linetypes: Vec::new(),
        styles: Vec::new(),
        dimstyles: Vec::new(),
        mline_styles: None,
        blocks: Vec::new(),
        layouts: Vec::new(),
        entities: Vec::new(),
        sort_handles: Vec::new(),
        skipped: Vec::new(),
        notices: Vec::new(),
    };
    tables(&parsed, &text, &mut names, &mut doc);
    let mut builder = Builder {
        names: &names,
        text: &text,
        numbers: &parsed.numbers,
        skipped: Vec::new(),
        viewports: 0.0,
        next_ordinal: parsed.numbers.values().copied().max().unwrap_or(0) + 1,
        admitted: 0,
        repeat_cost: 0,
        repeat_omitted: 0,
    };
    for (index, (_, flags, list)) in parsed.blocks.iter().enumerate() {
        let Some(name) = names.blocks.get(index).filter(|name| !name.is_empty()) else { continue };
        if flags & 0x80 != 0 || list.is_empty() {
            continue;
        }
        let base_point = match list.first().map(|record| &record.body) {
            Some(Body::Block { base, .. }) => *base,
            _ => [0.0; 3],
        };
        let flags = f64::from(flags & 0x7f);
        doc.blocks.push(DxfBlock {
            handle: String::new(),
            anonymous: flags as u8 & 1 != 0 || name.starts_with('*'),
            name: name.clone(),
            base_point,
            flags,
            layout: false,
            block_record_handle: None,
            xref_path: None,
            description: None,
        });
        let place = Place { space: DxfSpace::Block, block: Some(name.clone()) };
        builder.entities(list, &place, &mut doc.entities);
    }
    builder.entities(&parsed.entities, &Place { space: DxfSpace::Model, block: None }, &mut doc.entities);
    add_paper_layout(&mut doc);

    // Scene construction counts the visible entities again. Release this temporary
    // model budget, keeping any exceeded limit for the caller to report.
    limits::release(builder.admitted);
    if builder.repeat_omitted > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-repeat-limit",
            "繰り返しの図形が多すぎるため、上限を超えた写しを省きました",
            builder.repeat_omitted,
        );
    }
    let mut skipped = std::mem::take(&mut builder.skipped);
    zumen_core::js::order_object_keys(&mut skipped);
    if !skipped.is_empty() {
        let total = skipped.iter().map(|(_, count)| count).sum();
        let list: Vec<String> = skipped.iter().map(|(kind, count)| format!("{kind} {count}")).collect();
        notices.add_count(
            NoticeLevel::Info,
            "skipped-types",
            format!("読み取りに対応していない図形を表示していません（{}）", list.join("、")),
            total,
        );
    }
    doc.skipped = skipped;
    if parsed.broken > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-broken-object",
            "読めない記録、循環参照、または読み取り上限を超えた記録を省きました",
            parsed.broken,
        );
    }
    doc.notices = notices.into_vec();
    doc
}

/// ヘッダ変数を DXF の読み取りと同じ値にする。R13 より前の図面は $INSUNITS と $MEASUREMENT を持たない
fn header(version: Old, vars: &Vars, codepage: Option<String>) -> DxfHeader {
    let point = |p: [f64; 3]| DxfHeaderValue::Point(p.to_vec());
    let r11 = version >= Old::Ac1009;
    let mut variables = vec![("$ACADVER".to_string(), DxfHeaderValue::Text(version.name().into()))];
    if let Some(codepage) = &codepage {
        variables.push(("$DWGCODEPAGE".into(), DxfHeaderValue::Text(codepage.clone())));
    }
    variables.extend([
        ("$LTSCALE".to_string(), DxfHeaderValue::Number(vars.ltscale)),
        ("$TEXTSIZE".into(), DxfHeaderValue::Number(vars.textsize)),
        ("$DIMSCALE".into(), DxfHeaderValue::Number(vars.dimscale)),
        ("$DIMASZ".into(), DxfHeaderValue::Number(vars.dimasz)),
        ("$DIMTXT".into(), DxfHeaderValue::Number(vars.dimtxt)),
        ("$EXTMIN".into(), point(vars.extmin)),
        ("$EXTMAX".into(), point(vars.extmax)),
        ("$LIMMIN".into(), DxfHeaderValue::Point(vars.limmin.to_vec())),
        ("$LIMMAX".into(), DxfHeaderValue::Point(vars.limmax.to_vec())),
    ]);
    if r11 {
        variables.extend([
            ("$PSLTSCALE".to_string(), DxfHeaderValue::Number(f64::from(u8::from(vars.psltscale)))),
            ("$TILEMODE".into(), DxfHeaderValue::Number(f64::from(u8::from(vars.tilemode)))),
            ("$PEXTMIN".into(), point(vars.pextmin)),
            ("$PEXTMAX".into(), point(vars.pextmax)),
            ("$PLIMMIN".into(), DxfHeaderValue::Point(vars.plimmin.to_vec())),
            ("$PLIMMAX".into(), DxfHeaderValue::Point(vars.plimmax.to_vec())),
        ]);
    }
    let finite = |value: f64, fallback: f64| if value.is_finite() { value } else { fallback };
    DxfHeader {
        insunits: 0.0,
        measurement: None,
        codepage,
        ltscale: finite(vars.ltscale, 1.0),
        psltscale: f64::from(u8::from(vars.psltscale)),
        textsize: Some(vars.textsize),
        dimscale: finite(vars.dimscale, 1.0),
        dimasz: finite(vars.dimasz, 0.18),
        dimtxt: finite(vars.dimtxt, 0.18),
        tilemode: f64::from(u8::from(vars.tilemode)),
        extmin: Some(vars.extmin),
        extmax: Some(vars.extmax),
        limmin: Some(vars.limmin),
        limmax: Some(vars.limmax),
        plimmin: r11.then_some(vars.plimmin),
        plimmax: r11.then_some(vars.plimmax),
        variables,
    }
}

/// ブロックの名前。無名のブロックは表も BLOCK も *D・*U などの頭だけを持つので、R13 からの読み取りと同じく、表の中の
/// 番号を付ける（*D0 など）
fn block_name(name: String, index: usize) -> String {
    if name.starts_with('*') && name.chars().count() <= 2 { format!("{name}{index}") } else { name }
}

/// 表（レイヤ・線種・文字スタイル・寸法スタイル）。AutoCAD 1.x のレイヤは 0〜127 の番号が名前で、色の 255 は使って
/// いないレイヤ、0x80 は非表示
fn tables(parsed: &Parsed, text: &Text, names: &mut Names, doc: &mut DxfDocument) {
    for linetype in parsed.linetypes.iter().flatten().filter(|linetype| linetype.entry.live()) {
        doc.linetypes.push(DxfLinetype {
            handle: String::new(),
            name: text.escaped(&linetype.entry.name),
            description: text.escaped(&linetype.description),
            pattern_length: linetype.pattern_length,
            dashes: linetype.dashes.clone(),
            complex: false,
        });
    }
    for layer in parsed.layers.iter().flatten().filter(|layer| layer.entry.live()) {
        let index = f64::from(layer.color.unsigned_abs());
        doc.layers.push(DxfLayer {
            handle: String::new(),
            name: text.escaped(&layer.entry.name),
            color_index: if index == 0.0 || index > 255.0 { 7.0 } else { index },
            true_color: None,
            frozen: layer.entry.flags & 1 != 0,
            off: layer.color < 0,
            locked: layer.entry.flags & 4 != 0,
            plot: true,
            linetype: names.linetype(Some(layer.linetype)).replace("BYLAYER", "CONTINUOUS"),
            lineweight: -3.0,
            transparency: None,
        });
    }
    if let Some(colors) = &parsed.early_layers {
        names.layers = (0..colors.len()).map(|number| number.to_string()).collect();
        for (number, &color) in colors.iter().enumerate() {
            if color == 255 {
                continue;
            }
            let index = f64::from(color & 0x7f);
            doc.layers.push(DxfLayer {
                handle: String::new(),
                name: number.to_string(),
                color_index: if index == 0.0 { 7.0 } else { index },
                true_color: None,
                frozen: false,
                off: color & 0x80 != 0,
                locked: false,
                plot: true,
                linetype: "CONTINUOUS".into(),
                lineweight: -3.0,
                transparency: None,
            });
        }
    }
    // 形のファイル（70 の 1）の文字スタイルは名前を持たない
    for style in parsed.styles.iter().flatten().filter(|style| style.entry.live() || style.entry.flags & 0x81 == 1) {
        doc.styles.push(DxfTextStyle {
            handle: String::new(),
            name: text.escaped(&style.entry.name),
            font: text.escaped(&style.font),
            bigfont: text.escaped(&style.bigfont),
            height: style.height,
            width: if style.width == 0.0 || style.width.is_nan() { 1.0 } else { style.width },
            oblique: deg(style.oblique),
            flags: f64::from(style.entry.flags & 0x7f),
            generation_flags: f64::from(style.generation),
            font_family: None,
        });
    }
    for style in parsed.dimstyles.iter().flatten().filter(|style| style.entry.live()) {
        doc.dimstyles.push(DxfDimStyle {
            handle: String::new(),
            name: text.escaped(&style.entry.name),
            dimscale: style.dimscale,
            dimasz: style.dimasz,
            dimtxt: style.dimtxt,
        });
    }
}

/// 図形の置き場所
struct Place {
    space: DxfSpace,
    block: Option<String>,
}

/// 記録を図面のモデルの図形にする。POLYLINE は続く VERTEX、属性を持つ INSERT は続く ATTRIB を SEQEND まで集める
struct Builder<'a> {
    names: &'a Names,
    text: &'a Text,
    numbers: &'a HashMap<usize, u32>,
    skipped: Vec<(String, u64)>,
    /// 紙空間のビューポートの番号（DXF の 69）
    viewports: f64,
    next_ordinal: u32,
    admitted: u64,
    repeat_cost: usize,
    repeat_omitted: u64,
}

/// 集めている途中の図形（頂点・属性を待つ）
enum Pending {
    Polyline(Box<DxfEntity>, f64, [f64; 2]),
    Insert(Box<DxfEntity>),
}

/// 繰り返し（REPEAT〜ENDREP）の中の図形
struct Repeat {
    start: usize,
}

impl Builder<'_> {
    fn skip(&mut self, name: &str) {
        match self.skipped.iter_mut().find(|(kind, _)| kind == name) {
            Some(entry) => entry.1 += 1,
            None => self.skipped.push((name.to_string(), 1)),
        }
    }

    fn entities(&mut self, list: &[Record], place: &Place, out: &mut Vec<DxfEntity>) {
        let mut pending: Option<Pending> = None;
        let mut repeats: Vec<Repeat> = Vec::new();
        for record in list {
            if limits::tick() {
                break;
            }
            match (&record.body, &mut pending) {
                (Body::Vertex(vertex), Some(Pending::Polyline(polyline, elevation, widths))) => {
                    add_vertex(polyline, vertex, *elevation, *widths);
                    continue;
                }
                (Body::Attrib { .. }, Some(Pending::Insert(insert))) => {
                    if let Some(attribute) = self.entity(record, &insert_place(insert))
                        && let DxfEntityData::Insert(data) = &mut insert.data
                    {
                        data.attribs.push(attribute);
                    }
                    continue;
                }
                _ => {}
            }
            if let Some(done) = pending.take() {
                out.push(match done {
                    Pending::Polyline(entity, _, _) | Pending::Insert(entity) => *entity,
                });
            }
            match &record.body {
                Body::Seqend | Body::Vertex(_) | Body::Attrib { .. } | Body::Block { .. } | Body::EndBlock => {}
                Body::Repeat => repeats.push(Repeat { start: out.len() }),
                Body::EndRepeat { columns, rows, spacing } => {
                    if let Some(repeat) = repeats.pop() {
                        self.repeat_entities(out, repeat.start, *columns, *rows, *spacing);
                    }
                }
                Body::Other => self.skip(if record.kind == kind::SHAPE { "SHAPE" } else { "LOAD" }),
                Body::Polyline(fields) => {
                    let elevation = record.common.elevation;
                    if let Some(entity) = self.entity(record, place) {
                        pending = Some(Pending::Polyline(Box::new(entity), elevation, fields.widths));
                    }
                }
                Body::Insert(_) if record.common.follows => {
                    if let Some(entity) = self.entity(record, place) {
                        pending = Some(Pending::Insert(Box::new(entity)));
                    }
                }
                _ => {
                    if let Some(entity) = self.entity(record, place) {
                        out.push(entity);
                    }
                }
            }
        }
        if let Some(done) = pending.take() {
            out.push(match done {
                Pending::Polyline(entity, _, _) | Pending::Insert(entity) => *entity,
            });
        }
    }

    /// 1 つの記録の図形
    fn entity(&mut self, record: &Record, place: &Place) -> Option<DxfEntity> {
        let names = self.names;
        let text = self.text;
        let common = &record.common;
        let (kind, data) = match &record.body {
            Body::Line { start, end, extrusion } => {
                ("LINE", DxfEntityData::Line { start: *start, end: *end }).with(*extrusion)
            }
            Body::Point { location, extrusion, angle } => {
                ("POINT", DxfEntityData::Point { location: *location, x_axis_angle: deg(*angle) }).with(*extrusion)
            }
            Body::Circle { center, radius, extrusion } => {
                ("CIRCLE", DxfEntityData::Circle { center: *center, radius: *radius }).with(*extrusion)
            }
            Body::Arc { center, radius, angles, extrusion } => (
                "ARC",
                DxfEntityData::Arc {
                    center: *center,
                    radius: *radius,
                    start_angle: deg(angles[0]),
                    end_angle: deg(angles[1]),
                },
            )
                .with(*extrusion),
            Body::Text(fields) => ("TEXT", DxfEntityData::Text(self.text_base(fields))).with(fields.extrusion),
            Body::Attdef { text: fields, prompt, tag, flags } => (
                "ATTDEF",
                DxfEntityData::Attdef {
                    text: self.text_base(fields),
                    tag: text.escaped(tag),
                    prompt: text.escaped(prompt),
                    attribute_flags: f64::from(*flags),
                    mtext: None,
                },
            )
                .with(fields.extrusion),
            Body::Attrib { text: fields, tag, flags } => (
                "ATTRIB",
                DxfEntityData::Attrib {
                    text: self.text_base(fields),
                    tag: text.escaped(tag),
                    attribute_flags: f64::from(*flags),
                    mtext: None,
                },
            )
                .with(fields.extrusion),
            Body::Quad { corners, invisible, extrusion } => {
                let kind = match record.kind {
                    kind::TRACE => "TRACE",
                    kind::FACE3D => "3DFACE",
                    _ => "SOLID",
                };
                let [vtx0, vtx1, vtx2, vtx3] = *corners;
                (kind, DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges: invisible.map(f64::from) })
                    .with(*extrusion)
            }
            Body::Insert(insert) => {
                let name = match &insert.name {
                    Some(name) => text.string(name),
                    None => names.block(insert.block),
                };
                (
                    "INSERT",
                    DxfEntityData::Insert(DxfInsert {
                        name,
                        insert: insert.point,
                        xscale: insert.scale[0],
                        yscale: insert.scale[1],
                        zscale: insert.scale[2],
                        rotation: deg(insert.rotation),
                        column_count: f64::from(insert.columns.max(1)),
                        row_count: f64::from(insert.rows.max(1)),
                        column_spacing: insert.spacing[0],
                        row_spacing: insert.spacing[1],
                        attribs: Vec::new(),
                    }),
                )
                    .with(insert.extrusion)
            }
            Body::Polyline(polyline) => {
                let flags = polyline.flags;
                let mesh = flags & 16 != 0;
                (
                    "POLYLINE",
                    DxfEntityData::Polyline(DxfPolyline {
                        vertices: DxfVertexList {
                            points: Vec::new(),
                            bulges: Vec::new(),
                            // 頂点の幅の既定の値（DXF の 40・41）。頂点を足すときに使う
                            widths: vec![polyline.widths],
                            closed: flags & 1 != 0,
                            flags: f64::from(flags),
                        },
                        vertex_flags: Vec::new(),
                        faces: None,
                        m_count: mesh.then_some(f64::from(polyline.m_count)),
                        n_count: mesh.then_some(f64::from(polyline.n_count)),
                    }),
                )
                    .with(polyline.extrusion)
            }
            Body::Dimension(dimension) => (
                "DIMENSION",
                DxfEntityData::Dimension(DxfDimension {
                    anonymous_block_name: names.block(dimension.block),
                    dimtype: f64::from(dimension.dimtype),
                    defpoint: dimension.defpoint,
                    defpoint2: dimension.point13,
                    defpoint3: dimension.point14,
                    defpoint4: dimension.point15,
                    defpoint5: dimension.point16,
                    text_midpoint: dimension.text_midpoint,
                    insert: dimension.insert,
                    text: text.escaped(&dimension.text),
                    actual_measurement: None,
                    attachment_point: None,
                    text_rotation: dimension.text_rotation.map(deg),
                    horizontal_direction: None,
                    angle: dimension.rotation.map(deg),
                    oblique_angle: dimension.oblique.map(deg),
                    dimstyle: names.dimstyle(dimension.dimstyle),
                }),
            )
                .with(dimension.extrusion),
            Body::Viewport { center, width, height } => {
                self.viewports += 1.0;
                let mut viewport = DxfViewport {
                    center: *center,
                    width: *width,
                    height: *height,
                    status: self.viewports,
                    id: self.viewports,
                    view_center: [0.0, 0.0],
                    view_target: [0.0; 3],
                    view_direction: [0.0, 0.0, 1.0],
                    view_height: 0.0,
                    view_twist_angle: 0.0,
                    status_flags: 0.0,
                    frozen_layer_handles: Vec::new(),
                    frozen_layer_names: Vec::new(),
                    clipping_boundary_handle: None,
                };
                // 表示の値は拡張データ ACAD の MVIEW（R13・R14 と同じ）。凍結したレイヤ（1003）は層の表の番号で、DXF の
                // R12 の読み取りと同じく名前（大文字）で持つ
                let eed = Eed {
                    apps: common
                        .eed
                        .iter()
                        .map(|(app, values)| (u64::try_from(*app).unwrap_or(0), values.clone()))
                        .collect(),
                };
                mview(&eed, &mut viewport);
                viewport.frozen_layer_names = frozen_layers(&common.eed, names);
                ("VIEWPORT", DxfEntityData::Viewport(viewport)).with(None)
            }
            _ => return None,
        };
        let (paper, block) = if common.paper && place.space == DxfSpace::Model {
            (DxfSpace::Paper, Some("*Paper_Space".to_string()))
        } else {
            (place.space, place.block.clone())
        };
        let base = DxfEntityBase {
            kind: kind.to_string(),
            handle: common.handle.map(hex).unwrap_or_default(),
            // ハンドルのない図形の id は、DXF の R12 の図面と同じくファイルの中の記録の番号（#n）
            ordinal: if common.handle.is_some() { 0 } else { self.numbers.get(&record.position).copied().unwrap_or(0) },
            owner: None,
            space: paper,
            block,
            layer: names.layer(common.layer),
            linetype: names.linetype(common.linetype),
            color_index: common.color.map_or(256.0, f64::from),
            true_color: None,
            transparency: None,
            lineweight: -1.0,
            linetype_scale: 1.0,
            invisible: false,
            thickness: (common.thickness != 0.0).then_some(common.thickness),
            extrusion: data.1.filter(|extrusion| *extrusion != [0.0, 0.0, 1.0]),
        };
        if !limits::admit() {
            return None;
        }
        self.admitted += 1;
        Some(DxfEntity { base, data: data.0 })
    }

    fn text_base(&self, fields: &records::TextFields) -> DxfTextBase {
        DxfTextBase {
            text: self.text.escaped(&fields.value),
            insert: fields.insert,
            align_point: fields.align,
            height: fields.height,
            rotation: deg(fields.rotation),
            oblique: deg(fields.oblique),
            width: if fields.width == 0.0 || fields.width.is_nan() { 1.0 } else { fields.width },
            style: self.names.style(fields.style),
            halign: f64::from(fields.halign),
            valign: f64::from(fields.valign),
            text_generation_flag: f64::from(fields.generation),
        }
    }
}

/// R11・R12 のビューポートの拡張データの MVIEW の後の、凍結したレイヤ（1003、層の表の番号）の名前
fn frozen_layers(eed: &records::EedApps, names: &Names) -> Vec<String> {
    use crate::objects::EedValue;
    for (_, values) in eed {
        let mview = values.iter().position(|(code, value)| {
            *code == 1000 && matches!(value, EedValue::Text(text) if text.trim().eq_ignore_ascii_case("MVIEW"))
        });
        if let Some(start) = mview {
            return values[start + 1..]
                .iter()
                .filter_map(|(code, value)| match (code, value) {
                    (1003, EedValue::Number(index)) => usize::try_from(*index as i16)
                        .ok()
                        .and_then(|i| names.layers.get(i))
                        .filter(|name| !name.is_empty())
                        .map(|name| name.to_uppercase()),
                    _ => None,
                })
                .collect();
        }
    }
    Vec::new()
}

/// 図形の種類と値に押し出しを添える
trait With {
    fn with(self, extrusion: Option<[f64; 3]>) -> (&'static str, (DxfEntityData, Option<[f64; 3]>));
}

impl With for (&'static str, DxfEntityData) {
    fn with(self, extrusion: Option<[f64; 3]>) -> (&'static str, (DxfEntityData, Option<[f64; 3]>)) {
        (self.0, (self.1, extrusion))
    }
}

/// 属性の置き場所（配置と同じ空間・ブロック）
fn insert_place(insert: &DxfEntity) -> Place {
    Place { space: insert.base.space, block: insert.base.block.clone() }
}

/// POLYLINE に頂点を足す（DXF の読み取りと同じ）。ポリフェースメッシュの面の記録は面にし、2D の頂点の Z は POLYLINE の
/// 標高、幅がなければ POLYLINE の既定の幅
fn add_vertex(entity: &mut DxfEntity, vertex: &records::Vertex, elevation: f64, defaults: [f64; 2]) {
    let DxfEntityData::Polyline(polyline) = &mut entity.data else { return };
    let bits = js_bits(polyline.vertices.flags);
    if polyline.vertices.points.is_empty() {
        polyline.vertices.widths.clear();
    }
    let flags = vertex.flags;
    if bits & 64 != 0 && (vertex.face || (flags & 128 != 0 && flags & 64 == 0)) {
        let face: Vec<f64> =
            vertex.indexes.iter().map(|index| f64::from(*index)).filter(|index| *index != 0.0).collect();
        polyline.faces.get_or_insert_with(Vec::new).push(face);
        return;
    }
    let mut point = vertex.point;
    if bits & (8 | 16 | 64) == 0 && point[2] == 0.0 {
        point[2] = elevation;
    }
    polyline.vertices.points.push(point);
    polyline.vertices.bulges.push(vertex.bulge);
    polyline.vertices.widths.push(vertex.widths.unwrap_or(defaults));
    polyline.vertex_flags.push(f64::from(flags));
}

fn js_bits(value: f64) -> i32 {
    zumen_core::js::to_int32(value)
}

/// 繰り返し（AutoCAD 1.x〜2.x の REPEAT〜ENDREP）。start からの図形を、列（X）と行（Y）の間隔で並べた写しを足す
impl Builder<'_> {
    fn repeat_entities(&mut self, out: &mut Vec<DxfEntity>, start: usize, columns: i16, rows: i16, spacing: [f64; 2]) {
        let end = out.len();
        let start = start.min(end);
        if start == end {
            return;
        }
        let columns = columns.max(1);
        let rows = rows.max(1);
        let requested = (u64::from(columns as u16) * u64::from(rows as u16) - 1) * (end - start) as u64;
        let mut added = 0;
        for row in 0..rows {
            for column in 0..columns {
                if row == 0 && column == 0 {
                    continue;
                }
                let offset = [f64::from(column) * spacing[0], f64::from(row) * spacing[1]];
                for index in start..end {
                    let cost = repeat_cost(&out[index]);
                    // Bound all expansions together, including nested REPEATs and
                    // large polylines/text. Do not clone the original list first.
                    if self.repeat_cost.saturating_add(cost) > 100_000 || !limits::admit() {
                        self.repeat_omitted += requested - added;
                        return;
                    }
                    self.repeat_cost += cost;
                    self.admitted += 1;
                    let mut copy = out[index].clone();
                    self.number_copy(&mut copy);
                    translate(&mut copy.data, offset);
                    out.push(copy);
                    added += 1;
                }
            }
        }
    }

    fn number_copy(&mut self, copy: &mut DxfEntity) {
        copy.base.handle.clear();
        copy.base.ordinal = self.next_ordinal;
        self.next_ordinal += 1;
        if let DxfEntityData::Insert(insert) = &mut copy.data {
            for attribute in &mut insert.attribs {
                self.number_copy(attribute);
            }
        }
    }
}

fn repeat_cost(entity: &DxfEntity) -> usize {
    let extra = match &entity.data {
        DxfEntityData::Polyline(p) => p.vertices.points.len() + p.faces.as_ref().map_or(0, Vec::len),
        DxfEntityData::Text(t) | DxfEntityData::Attrib { text: t, .. } | DxfEntityData::Attdef { text: t, .. } => {
            t.text.len().div_ceil(64)
        }
        DxfEntityData::Insert(i) => i.attribs.iter().map(repeat_cost).sum(),
        DxfEntityData::Dimension(d) => d.text.len().div_ceil(64),
        _ => 0,
    };
    1 + extra
}

fn translate(data: &mut DxfEntityData, [dx, dy]: [f64; 2]) {
    let shift = |p: &mut [f64; 3]| {
        p[0] += dx;
        p[1] += dy;
    };
    match data {
        DxfEntityData::Line { start, end } => {
            shift(start);
            shift(end);
        }
        DxfEntityData::Point { location, .. } => shift(location),
        DxfEntityData::Circle { center, .. } | DxfEntityData::Arc { center, .. } => shift(center),
        DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
            shift(&mut text.insert);
            if let Some(align) = &mut text.align_point {
                shift(align);
            }
        }
        DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, .. } => {
            for p in [vtx0, vtx1, vtx2, vtx3] {
                shift(p);
            }
        }
        DxfEntityData::Insert(insert) => {
            shift(&mut insert.insert);
            for attribute in &mut insert.attribs {
                translate(&mut attribute.data, [dx, dy]);
            }
        }
        DxfEntityData::Dimension(d) => {
            shift(&mut d.defpoint);
            shift(&mut d.text_midpoint);
            if d.insert.is_none() {
                d.insert = Some([0.0; 3]);
            }
            for point in [&mut d.defpoint2, &mut d.defpoint3, &mut d.defpoint4, &mut d.defpoint5, &mut d.insert]
                .into_iter()
                .flatten()
            {
                shift(point);
            }
        }
        DxfEntityData::Viewport(v) => shift(&mut v.center),
        DxfEntityData::Polyline(polyline) => polyline.vertices.points.iter_mut().for_each(shift),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
