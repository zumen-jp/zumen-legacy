//! Jw_cad の JWW ファイル（MFC の CArchive で書いたバイナリ）を読む。
//! ヘッダの並びとクラスごとの値の並びは、Jw_cad の形式説明（jwdatafmt.txt）を読み解いた
//! ezjww（MIT、https://github.com/monozukuri-ai/ezjww）の実装を参照した。`@zumenjs/jww` の reader.ts と同じ手順で読む。

use crate::archive::{ReadTable, Reader, Result};
use std::collections::BTreeMap;
use zumen_core::ReadError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JwwBase {
    pub group: u32,
    /// 線種番号（点では 100 のとき記号の点）
    pub pen_style: u8,
    /// 線色番号（1〜9、SXF 対応色は 100〜356、ソリッドの任意色は 10）
    pub pen_color: u16,
    /// 線幅（0 なら線色ごとの線幅）
    pub pen_width: u16,
    pub layer: u16,
    pub layer_group: u16,
    pub flag: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwLine {
    pub base: JwwBase,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwArc {
    pub base: JwwBase,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    /// 始角・円弧角・傾き（ラジアン）。円弧角が負なら時計回り
    pub start: f64,
    pub sweep: f64,
    pub tilt: f64,
    /// 扁平率（楕円の短径 / 長径）
    pub flatness: f64,
    pub full: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwPoint {
    pub base: JwwBase,
    pub x: f64,
    pub y: f64,
    pub temporary: bool,
    pub code: u32,
    pub angle: f64,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwText {
    pub base: JwwBase,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub text_type: u32,
    pub size_x: f64,
    pub size_y: f64,
    pub spacing: f64,
    /// 回転角（度）
    pub angle: f64,
    pub font: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwSolid {
    pub base: JwwBase,
    /// 四角形の 4 点（始点 → 第 2 点 → 第 3 点 → 終点）
    pub points: [f64; 8],
    pub color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwCircleSolid {
    pub base: JwwBase,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub flatness: f64,
    pub tilt: f64,
    pub start: f64,
    pub sweep: f64,
    pub mode: f64,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwBlock {
    pub base: JwwBase,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    /// 回転角（ラジアン）
    pub rotation: f64,
    pub definition: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwDimension {
    pub base: JwwBase,
    pub line: JwwLine,
    pub text: JwwText,
    pub lines: Vec<JwwLine>,
    pub points: Vec<JwwPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JwwEntity {
    Line(JwwLine),
    Arc(JwwArc),
    Point(JwwPoint),
    Text(JwwText),
    Solid(JwwSolid),
    CircleSolid(JwwCircleSolid),
    Block(JwwBlock),
    Dimension(JwwDimension),
}

impl JwwEntity {
    pub fn base(&self) -> &JwwBase {
        match self {
            JwwEntity::Line(e) => &e.base,
            JwwEntity::Arc(e) => &e.base,
            JwwEntity::Point(e) => &e.base,
            JwwEntity::Text(e) => &e.base,
            JwwEntity::Solid(e) => &e.base,
            JwwEntity::CircleSolid(e) => &e.base,
            JwwEntity::Block(e) => &e.base,
            JwwEntity::Dimension(e) => &e.base,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwBlockDefinition {
    pub number: u32,
    pub name: String,
    pub entities: Vec<JwwEntity>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JwwLayer {
    pub name: String,
    pub state: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwLayerGroup {
    pub name: String,
    /// 0: 非表示、1: 表示のみ、2: 編集可能、3: 書込み
    pub state: u32,
    /// 縮尺の分母（1/100 なら 100）
    pub scale: f64,
    pub layers: Vec<JwwLayer>,
}

/// 図形ごとの線幅の単位（1/100 mm か、印刷のドット）
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WidthUnit {
    pub hundredth: bool,
    pub dpi: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JwwDocument {
    pub version: u32,
    pub memo: String,
    pub paper_size: u32,
    pub groups: Vec<JwwLayerGroup>,
    /// 線色 0〜9 の画面色（0 は背景）。読めなければ None
    pub pen_colors: Option<Vec<String>>,
    /// 線色 100〜356 の画面色
    pub extended_colors: Option<Vec<String>>,
    /// 線色 0〜9 の印刷線幅（mm）。単位が分からなければ None
    pub pen_widths: Option<Vec<Option<f64>>>,
    pub width_unit: WidthUnit,
    /// 線種番号ごとの破線の長さ（mm）
    pub line_types: BTreeMap<u32, Vec<f64>>,
    pub entities: Vec<JwwEntity>,
    pub blocks: Vec<JwwBlockDefinition>,
    /// 読み取りを途中で止めた理由（読めた分は entities に残る）
    pub truncated: Option<String>,
}

/// COLORREF（0x00BBGGRR）を #rrggbb にする
pub(crate) fn colorref(value: u32) -> String {
    format!("#{:02x}{:02x}{:02x}", value & 0xff, (value >> 8) & 0xff, (value >> 16) & 0xff)
}

/// 線種のビット列（下位 unit ビットが 1 周期）を、線分・空白の長さ（mm）にする
pub(crate) fn pattern_segments(pattern: u32, unit: u32, printer_pitch: u32) -> Vec<f64> {
    if !(1..=32).contains(&unit) {
        return Vec::new();
    }
    let bit = |index: u32| ((pattern >> (index % unit)) & 1) == 1;
    let Some(start) = (0..unit).find(|&i| bit(i) && !bit(i + unit - 1)) else {
        return Vec::new();
    };
    let mut runs = Vec::new();
    let mut current = true;
    let mut length = 0u32;
    for offset in 0..unit {
        let value = bit(start + offset);
        if value == current {
            length += 1;
        } else {
            runs.push(length);
            current = value;
            length = 1;
        }
    }
    runs.push(length);
    // 1 ビットは印刷ピッチ × 1/32 mm
    let scale = f64::from(printer_pitch) / 32.0;
    runs.into_iter().map(|run| f64::from(run) * scale).collect()
}

pub(crate) fn read_base(reader: &mut Reader<'_>, version: u32) -> Result<JwwBase> {
    Ok(JwwBase {
        group: reader.u32()?,
        pen_style: reader.u8()?,
        pen_color: reader.u16()?,
        pen_width: if version >= 351 { reader.u16()? } else { 0 },
        layer: reader.u16()?,
        layer_group: reader.u16()?,
        flag: reader.u16()?,
    })
}

pub(crate) fn read_line(reader: &mut Reader<'_>, version: u32) -> Result<JwwLine> {
    let base = read_base(reader, version)?;
    Ok(JwwLine { base, x1: reader.f64()?, y1: reader.f64()?, x2: reader.f64()?, y2: reader.f64()? })
}

pub(crate) fn read_point(reader: &mut Reader<'_>, version: u32) -> Result<JwwPoint> {
    let base = read_base(reader, version)?;
    let x = reader.f64()?;
    let y = reader.f64()?;
    let temporary = reader.u32()? != 0;
    let symbol = base.pen_style == 100;
    let code = if symbol { reader.u32()? } else { 0 };
    let angle = if symbol { reader.f64()? } else { 0.0 };
    let scale = if symbol { reader.f64()? } else { 0.0 };
    Ok(JwwPoint { base, x, y, temporary, code, angle, scale })
}

pub(crate) fn read_text(reader: &mut Reader<'_>, version: u32) -> Result<JwwText> {
    let base = read_base(reader, version)?;
    Ok(JwwText {
        base,
        x1: reader.f64()?,
        y1: reader.f64()?,
        x2: reader.f64()?,
        y2: reader.f64()?,
        text_type: reader.u32()?,
        size_x: reader.f64()?,
        size_y: reader.f64()?,
        spacing: reader.f64()?,
        angle: reader.f64()?,
        font: reader.cstring()?,
        content: reader.cstring()?,
    })
}

pub(crate) fn read_entity_body(reader: &mut Reader<'_>, class_name: &str, version: u32) -> Result<JwwEntity> {
    match class_name {
        "CDataSen" => Ok(JwwEntity::Line(read_line(reader, version)?)),
        "CDataEnko" => {
            let base = read_base(reader, version)?;
            Ok(JwwEntity::Arc(JwwArc {
                base,
                cx: reader.f64()?,
                cy: reader.f64()?,
                radius: reader.f64()?,
                start: reader.f64()?,
                sweep: reader.f64()?,
                tilt: reader.f64()?,
                flatness: reader.f64()?,
                full: reader.u32()? != 0,
            }))
        }
        "CDataTen" => Ok(JwwEntity::Point(read_point(reader, version)?)),
        "CDataMoji" => Ok(JwwEntity::Text(read_text(reader, version)?)),
        "CDataSolid" => {
            let base = read_base(reader, version)?;
            let mut values = [0.0; 8];
            for value in &mut values {
                *value = reader.f64()?;
            }
            let color = if base.pen_color == 10 { Some(reader.u32()?) } else { None };
            if base.pen_style >= 101 {
                let [cx, cy, radius, flatness, tilt, start, sweep, mode] = values;
                return Ok(JwwEntity::CircleSolid(JwwCircleSolid {
                    base,
                    cx,
                    cy,
                    radius,
                    flatness,
                    tilt,
                    start,
                    sweep,
                    mode,
                    color,
                }));
            }
            // JWW 形式説明「ソリッドデータ」CDataSolid::Serialize の格納順で輪郭をたどる。
            // メンバー名の点番号で並べ替えると矩形が交差する（Jw_cad で照合。docs/formats/jww.md）。
            Ok(JwwEntity::Solid(JwwSolid { base, points: values, color }))
        }
        "CDataBlock" => {
            let base = read_base(reader, version)?;
            Ok(JwwEntity::Block(JwwBlock {
                base,
                x: reader.f64()?,
                y: reader.f64()?,
                scale_x: reader.f64()?,
                scale_y: reader.f64()?,
                rotation: reader.f64()?,
                definition: reader.u32()?,
            }))
        }
        "CDataSunpou" => {
            let base = read_base(reader, version)?;
            let line = read_line(reader, version)?;
            let text = read_text(reader, version)?;
            let mut lines = Vec::new();
            let mut points = Vec::new();
            if version >= 420 {
                reader.u16()?;
                for _ in 0..2 {
                    lines.push(read_line(reader, version)?);
                }
                for _ in 0..4 {
                    points.push(read_point(reader, version)?);
                }
            }
            Ok(JwwEntity::Dimension(JwwDimension { base, line, text, lines, points }))
        }
        _ => Err(ReadError::new(format!("対応していないクラス {class_name}"))),
    }
}

/// 図形の一覧を読む。途中で失敗しても、それまでに読んだ図形は out に残る
fn read_entity_list(
    reader: &mut Reader<'_>,
    version: u32,
    table: &mut ReadTable,
    out: &mut Vec<JwwEntity>,
) -> Result<()> {
    let count = reader.count()?;
    for _ in 0..count {
        // 読み取りの上限（zumen_core::limits）。途中でやめると後ろを読めないが、上限のエラーを返す
        if zumen_core::limits::tick() {
            break;
        }
        let Some((class_name, _, _)) = table.read_class(reader)? else {
            continue;
        };
        out.push(read_entity_body(reader, &class_name, version)?);
        table.reserve();
    }
    Ok(())
}

/// 最初の図形クラス（FF FF・版・名前の長さ・"CData"）の直前にある件数の位置
pub(crate) fn find_entity_list(bytes: &[u8]) -> Option<usize> {
    let mut i = 100;
    while i + 12 < bytes.len() {
        if bytes[i] != 0xff || bytes[i + 1] != 0xff {
            i += 1;
            continue;
        }
        let length = u16::from_le_bytes([bytes[i + 4], bytes[i + 5]]);
        if !(8..=32).contains(&length) || &bytes[i + 6..i + 11] != b"CData" {
            i += 1;
            continue;
        }
        // 65535 件以上は WORD の 0xFFFF と DWORD の件数
        if i >= 6 && bytes[i - 6] == 0xff && bytes[i - 5] == 0xff {
            let count = u32::from_le_bytes([bytes[i - 4], bytes[i - 3], bytes[i - 2], bytes[i - 1]]);
            if count >= 0xffff && u64::from(count) * 20 <= bytes.len() as u64 {
                return Some(i - 6);
            }
        }
        return Some(i - 2);
    }
    None
}

struct DisplaySettings {
    pen_colors: Vec<String>,
    extended_colors: Option<Vec<String>>,
    pen_widths: Vec<Option<f64>>,
    line_types: BTreeMap<u32, Vec<f64>>,
    width_unit: WidthUnit,
}

fn read_line_type(reader: &mut Reader<'_>) -> Result<Vec<f64>> {
    let pattern = reader.u32()?;
    let unit = reader.u32()?;
    reader.u32()?;
    let printer_pitch = reader.u32()?;
    Ok(pattern_segments(pattern, unit, printer_pitch))
}

/// SXF 対応線種の欄。読めなくても、それまでに読んだ線種と色は使う
fn read_sxf_line_types(reader: &mut Reader<'_>, line_types: &mut BTreeMap<u32, Vec<f64>>) -> Result<()> {
    for _ in 0..257 {
        reader.cstring()?;
        reader.skip(16)?;
    }
    reader.skip(33 * 16)?;
    for i in 0..33 {
        reader.cstring()?;
        let count = reader.u32()?;
        let mut lengths = [0.0; 10];
        for value in &mut lengths {
            *value = reader.f64()?;
        }
        if count <= 10 && lengths.iter().all(|value| value.is_finite() && *value >= 0.0) {
            line_types.insert(30 + i, lengths[..count as usize].to_vec());
        }
    }
    Ok(())
}

/// レイヤ名の後の、画面色・線幅・線種の設定。版 300 より前は天空図（16 バイト）、マークジャンプ 5〜8 とジャンプの
/// レイヤグループ（ジャンプは 24 バイトが 4 つ）、文字の描画状態（56 バイト）がない（jwdatafmt.txt の注記）
fn read_display_settings(reader: &mut Reader<'_>, version: u32, max_draw_width: i32) -> Result<DisplaySettings> {
    // 日影、天空図、2.5D の単位、画面と範囲記憶の倍率・原点、マークジャンプ、文字の描画状態、複線の間隔と留線出
    let (sky, jumps, text) = if version >= 300 { (16, 8 * 28, 56) } else { (0, 4 * 24, 0) };
    reader.skip(28 + sky + 4 + 24 + 24 + jumps + text + 80 + 8)?;
    let mut pen_colors = Vec::with_capacity(10);
    for _ in 0..10 {
        pen_colors.push(colorref(reader.u32()?));
        reader.u32()?;
    }
    let mut printer_widths = Vec::with_capacity(10);
    for _ in 0..10 {
        reader.u32()?;
        printer_widths.push(reader.u32()?);
        reader.f64()?;
    }
    let mut line_types = BTreeMap::new();
    for i in 0..8 {
        line_types.insert(2 + i, read_line_type(reader)?);
    }
    reader.skip(5 * 20)?; // ランダム線
    for i in 0..4 {
        line_types.insert(16 + i, read_line_type(reader)?);
    }
    // 線幅の単位：m_nMaxDrawWid が負なら 1/100 mm、そうでなければ印刷のドット
    let mut dpi = None;
    let mut extended_colors = None;
    if version >= 420 {
        let mut last = 0;
        for _ in 0..11 {
            last = reader.u32()?;
        }
        dpi = if (20..30).contains(&last) {
            Some(600.0)
        } else if (10..20).contains(&last) {
            Some(300.0)
        } else {
            None
        };
        reader.skip(20 + 40 + 32 + 8)?;
        let mut colors = Vec::with_capacity(257);
        for _ in 0..257 {
            colors.push(colorref(reader.u32()?));
            reader.u32()?;
        }
        extended_colors = Some(colors);
        // SXF 対応線種の欄が読めなくても、色と標準の線種は使う
        let _ = read_sxf_line_types(reader, &mut line_types);
    }
    let hundredth = max_draw_width < 0;
    let pen_widths = printer_widths
        .iter()
        .map(|&width| {
            let width = f64::from(width);
            if hundredth { Some(width / 100.0) } else { dpi.map(|dpi| (width * 25.4) / dpi) }
        })
        .collect();
    Ok(DisplaySettings {
        pen_colors,
        extended_colors,
        pen_widths,
        line_types,
        width_unit: WidthUnit { hundredth, dpi },
    })
}

/// レイヤ名・レイヤグループ名と表示の設定。途中で読めなくなっても、読めた名前は残す
fn read_names_and_settings(
    reader: &mut Reader<'_>,
    version: u32,
    groups: &mut [JwwLayerGroup],
) -> Result<DisplaySettings> {
    reader.skip((14 + 5 + 1) * 4)?;
    let max_draw_width = reader.i32()?;
    reader.skip(16 + 8 + 4 + 4 + 8 + 16 + 16)?;
    for group in groups.iter_mut() {
        for layer in &mut group.layers {
            layer.name = reader.cstring()?;
        }
    }
    for group in groups.iter_mut() {
        group.name = reader.cstring()?;
    }
    read_display_settings(reader, version, max_draw_width)
}

pub fn is_jww(bytes: &[u8]) -> bool {
    bytes.starts_with(b"JwwData.")
}

fn read_blocks(
    body: &mut Reader<'_>,
    version: u32,
    table: &mut ReadTable,
    blocks: &mut Vec<JwwBlockDefinition>,
) -> Result<()> {
    if body.remaining() < 2 {
        return Ok(());
    }
    let count = body.count()?;
    for _ in 0..count.min(10_000) {
        let Some(_) = table.read_class(body)? else {
            continue;
        };
        table.reserve();
        read_base(body, version)?;
        let number = body.u32()?;
        body.u32()?; // 参照されているか
        body.skip(4)?; // CTime
        let name = body.cstring()?;
        let mut entities = Vec::new();
        read_entity_list(body, version, table, &mut entities)?;
        blocks.push(JwwBlockDefinition { number, name, entities });
    }
    Ok(())
}

pub fn read_jww(bytes: &[u8]) -> Result<JwwDocument> {
    if !is_jww(bytes) {
        return Err(ReadError::new("JWW ファイルではありません"));
    }
    let mut reader = Reader::new(bytes, 8);
    let version = reader.u32()?;
    let memo = reader.cstring()?;
    let paper_size = reader.u32()?;
    reader.u32()?; // 書込みレイヤグループ
    let mut groups = Vec::with_capacity(16);
    for _ in 0..16 {
        let state = reader.u32()?;
        reader.u32()?;
        let scale = reader.f64()?;
        reader.u32()?;
        let mut layers = Vec::with_capacity(16);
        for _ in 0..16 {
            let layer_state = reader.u32()?;
            reader.u32()?;
            layers.push(JwwLayer { name: String::new(), state: layer_state });
        }
        groups.push(JwwLayerGroup { name: String::new(), state, scale, layers });
    }
    let settings = read_names_and_settings(&mut reader, version, &mut groups).ok();
    for (g, group) in groups.iter_mut().enumerate() {
        if group.name.is_empty() {
            group.name = format!("グループ {g:X}");
        }
        for (l, layer) in group.layers.iter_mut().enumerate() {
            if layer.name.is_empty() {
                layer.name = format!("{g:X}-{l:X}");
            }
        }
    }
    let (pen_colors, extended_colors, pen_widths, width_unit, line_types) = match settings {
        Some(s) => (Some(s.pen_colors), s.extended_colors, Some(s.pen_widths), s.width_unit, s.line_types),
        None => (None, None, None, WidthUnit::default(), BTreeMap::new()),
    };
    let mut document = JwwDocument {
        version,
        memo,
        paper_size,
        groups,
        pen_colors,
        extended_colors,
        pen_widths,
        width_unit,
        line_types,
        entities: Vec::new(),
        blocks: Vec::new(),
        truncated: None,
    };
    let Some(start) = find_entity_list(bytes) else {
        return Ok(document);
    };
    let mut body = Reader::new(bytes, start);
    let mut table = ReadTable::new();
    if let Err(error) = read_entity_list(&mut body, version, &mut table, &mut document.entities) {
        document.truncated = Some(error.message().to_string());
        return Ok(document);
    }
    // ブロック定義の一覧（CDataList）。中に図形の一覧を持つ
    if let Err(error) = read_blocks(&mut body, version, &mut table, &mut document.blocks) {
        document.truncated = Some(format!("ブロック定義: {}", error.message()));
    }
    Ok(document)
}
