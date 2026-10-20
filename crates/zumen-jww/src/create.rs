//! 新しい図面を書く（`write_jww`）。図面の内容（`JwwDocument`）を、Jw_cad の新しい図面の既定の設定のヘッダと
//! 合わせて書く。既定の設定は、Jw_cad 8 以降が新しい図面に書く値（日影・2.5D・文字種・線種・SXF 対応の色と
//! 線種など）。図面の内容が持つ値（メモ・用紙・レイヤグループとレイヤの状態・縮尺・名前・線色・線幅・線種）は
//! その値にする。

use crate::archive::UNICODE_VERSION;
use crate::file::{JwwBlockRecord, JwwFile, JwwRecord, JwwWritten};
use crate::header::{
    JwwDisplaySettings, JwwGeneralSettings, JwwGroupState, JwwHeader, JwwLayerState, JwwLinePattern, JwwMarkJump,
    JwwPen, JwwPrinterPen, JwwRandomLine, JwwSettings, JwwSxfLineType, JwwSxfPrinterPen, JwwSxfSettings,
    JwwTextSettings, JwwTextStyle,
};
use crate::reader::{
    JwwBase, JwwBlockDefinition, JwwCircleSolid, JwwDocument, JwwEntity, JwwLayer, JwwLayerGroup, JwwPoint, JwwSolid,
    WidthUnit, pattern_segments, read_jww,
};
use crate::sxf_tables::{SXF_COLORS, SXF_LINE_PITCHES};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::{PI, TAU};
use zumen_core::{NoticeLevel, NoticeList};

/// 新しい図面を書く版。Jw_cad 7.02 以降の版（文字列は UTF-16LE、同梱画像を持てる）で、Jw_cad 8 以降が開ける
pub const JWW_VERSION: u32 = UNICODE_VERSION;

/// 書ける版。版ごとの欄の違いは形式説明（jwdatafmt.txt）の注記のとおりで、300 より前の版はその注記だけに拠る
const WRITABLE_VERSIONS: std::ops::RangeInclusive<u32> = 200..=700;

/// 線色 0〜9 の画面表示色（COLORREF）。0 は背景
const PEN_COLORS: [u32; 10] =
    [0xffffff, 0xc0c000, 0x000000, 0x00c000, 0x00c0c0, 0xc000c0, 0xff0000, 0x808000, 0x8000ff, 0xc0c0c0];

/// 線色 0〜9 のプリンタ出力色（COLORREF）・線幅（ドット）・実点の半径（mm）
const PRINTER_PENS: [(u32, u32, f64); 10] = [
    (0xffffff, 1, 0.1),
    (0xffff00, 1, 0.2),
    (0x000000, 2, 0.3),
    (0x00ff00, 3, 0.4),
    (0x00ffff, 4, 0.5),
    (0xff00ff, 5, 0.5),
    (0xff0000, 6, 0.5),
    (0x808000, 7, 0.5),
    (0x0000ff, 8, 0.5),
    (0xc0c0c0, 1, 0.1),
];

/// 線種 2〜9 のパターン・1 ユニットのドット数（プリンタ出力ピッチは 10、ピッチは 1）
const LINE_TYPES: [(u32, u32); 8] = [
    (0x9999_9999, 4),
    (0xc3c3_c3c3, 8),
    (0xe7e7_e7e7, 8),
    (0xf99f_f99f, 16),
    (0xfff9_9fff, 32),
    (0xf24f_f24f, 16),
    (0xfff2_4fff, 32),
    (0x2222_2222, 4),
];

/// 倍長線種 6〜9（線種 16〜19）のパターン・ピッチ・プリンタ出力ピッチ（1 ユニットは 32 ドット）
const LONG_LINE_TYPES: [(u32, u32, u32); 4] =
    [(0xfff9_9fff, 2, 20), (0xfff2_4fff, 2, 20), (0xfffe_7fff, 2, 20), (0xfffe_7fff, 4, 40)];

/// ランダム線 1〜5 の画面表示の振幅とピッチ、プリンタ出力の振幅とピッチ（パターンは共通）
const RANDOM_LINES: [(u32, u32, u32, u32); 5] =
    [(1, 3, 1, 5), (1, 4, 1, 10), (2, 5, 2, 15), (2, 6, 2, 20), (2, 7, 2, 25)];
const RANDOM_PATTERN: u32 = 0xccb2_b32a;

/// SXF 対応拡張線種（線種 30〜62）のパターン・1 ユニットのドット数・プリンタ出力ピッチ（ピッチは 1）。
/// 31〜45 は既定義線種、46 は未定義
const SXF_LINE_TYPES: [(u32, u32, u32); 17] = [
    (0, 0, 0),
    (0xffff_ffff, 32, 10),
    (0xfe3f_fe3f, 16, 15),
    (0xf00f_f00f, 16, 24),
    (0xfffb_bfff, 32, 15),
    (0xffee_efff, 32, 17),
    (0xffbb_bbff, 32, 19),
    (0x1010_1010, 8, 7),
    (0xfffd_bfff, 32, 19),
    (0xfff6_dfff, 32, 24),
    (0xffe0_83ff, 32, 9),
    (0xfc7f_f11f, 32, 17),
    (0xff84_21ff, 32, 11),
    (0xf8ff_888f, 32, 19),
    (0xfe10_843f, 32, 13),
    (0xf9ff_249f, 32, 20),
    (0xffff_ffff, 32, 10),
];

/// 線種 47〜62 の欄の 1 ユニットのドット数（パターンは 0、プリンタ出力ピッチは 10）
const SXF_SPARE_UNITS: [u32; 16] = [32, 16, 16, 32, 32, 32, 8, 32, 32, 32, 32, 32, 32, 32, 32, 32];

/// SXF の既定義色 1〜16 の名前
const SXF_COLOR_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "blue",
    "yellow",
    "magenta",
    "cyan",
    "white",
    "deeppink",
    "brown",
    "orange",
    "lightgreen",
    "lightblue",
    "lavender",
    "lightgray",
    "darkgray",
];

/// SXF の既定義線種 1〜15 の名前
const SXF_LINE_TYPE_NAMES: [&str; 15] = [
    "continuous",
    "dashed",
    "dashed spaced",
    "long dashed dotted",
    "long dashed double-dotted",
    "long dashed triplicate-dotted",
    "dotted",
    "chain",
    "chain double dash",
    "dashed dotted",
    "double-dashed dotted",
    "dashed double-dotted",
    "double-dashed double-dotted",
    "dashed triplicate-dotted",
    "double-dashed triplicate-dotted",
];

/// ユーザー定義の SXF 対応色（線色 117〜355）の既定の色
const SXF_SPARE_COLOR: u32 = 0xc0c0c0;

/// 文字種 1〜10 の幅・高さ・間隔（mm）・色番号
const TEXT_STYLES: [(f64, f64, f64, u32); 10] = [
    (2.0, 2.0, 0.0, 1),
    (2.5, 2.5, 0.0, 1),
    (3.0, 3.0, 0.5, 2),
    (4.0, 4.0, 0.5, 2),
    (5.0, 5.0, 0.5, 3),
    (6.0, 6.0, 0.5, 3),
    (7.0, 7.0, 1.0, 4),
    (8.0, 8.0, 1.0, 4),
    (9.0, 9.0, 1.0, 5),
    (10.0, 10.0, 1.0, 5),
];

/// 線描画の最大幅（ドット）。線幅を 1/100 mm 単位とするときは、版 600 以降は −(この値 + 200) を書く
const MAX_DRAW_WIDTH: i32 = 100;

/// 印刷の線幅の基準の dpi の目安（版 600 以降、表示のみレイヤを印刷しない設定に足す）
fn dpi_flag(dpi: Option<f64>) -> u32 {
    if dpi == Some(600.0) { 20 } else { 10 }
}

impl JwwHeader {
    /// Jw_cad の新しい図面の既定の設定のヘッダ（A3、すべてのレイヤグループが 1/1、グループ 0 のレイヤ 0 に書込み）。
    /// その版にない欄は書かない
    pub fn new(version: u32) -> Self {
        let groups = (0..16)
            .map(|group| {
                let mut layers = [JwwLayerState { state: 2, protect: 0 }; 16];
                layers[0].state = 3;
                JwwGroupState { state: if group == 0 { 3 } else { 2 }, write_layer: 0, scale: 1.0, protect: 0, layers }
            })
            .collect();
        let settings = Some(JwwSettings {
            general: general_settings(),
            layer_names: vec![String::new(); 256],
            group_names: vec![String::new(); 16],
            display: Some(display_settings(version)),
            sxf: (version >= 420).then(sxf_settings),
            text: Some(text_settings()),
        });
        JwwHeader { memo: String::new(), paper_size: 3, write_group: 0, groups, settings, rest: Vec::new() }
    }
}

fn general_settings() -> JwwGeneralSettings {
    JwwGeneralSettings {
        dummy: [0; 14],
        // 寸法の設定（Jw_cad の既定）。環境設定で保存しない図面では読まれない
        dimension: [2_002_111, 5, 50_150_030, 11_000_000, 4_100],
        dummy2: 0,
        max_draw_width: MAX_DRAW_WIDTH,
        print_origin: [0.0, 0.0],
        print_scale: 1.0,
        print_setting: 0,
        grid_mode: 0,
        grid_min_dots: 15.0,
        grid_spacing: [5.0, 5.0],
        grid_origin: [0.0, 0.0],
    }
}

fn display_settings(version: u32) -> JwwDisplaySettings {
    let pattern = |(pattern, unit): (u32, u32)| JwwLinePattern { pattern, unit, pitch: 1, printer_pitch: 10 };
    JwwDisplaySettings {
        shadow_level: 1.5,
        latitude: 36.0,
        shadow_hours: 0,
        wall_shadow_level: 0.0,
        sky_level: 0.0,
        sky_diameter: 50.0,
        mm_3d: 0,
        view_scale: 1.0,
        view_origin: [0.0, 0.0],
        range_scale: 1.0,
        range_origin: [0.0, 0.0],
        jumps: [JwwMarkJump { scale: 1.0, origin: [0.0, 0.0], group: u32::MAX }; 4],
        more_jumps: [JwwMarkJump { scale: 1.0, origin: [0.0, 0.0], group: u32::MAX }; 4],
        text_dummy: [0.0; 3],
        text_dummy_int: 0,
        text_dummy2: [0.0; 2],
        text_background_margin: 1.0,
        text_background: 1,
        parallel_spacings: [100.0, 200.0, 300.0, 400.0, 500.0, 1000.0, 2000.0, 3000.0, 4000.0, 5000.0],
        double_parallel_end: 0.0,
        pens: PEN_COLORS.map(|color| JwwPen { color, width: 1 }),
        printer_pens: PRINTER_PENS.map(|(color, width, point_radius)| JwwPrinterPen { color, width, point_radius }),
        line_types: LINE_TYPES.map(pattern),
        random_lines: RANDOM_LINES.map(|(width, pitch, printer_width, printer_pitch)| JwwRandomLine {
            pattern: RANDOM_PATTERN,
            width,
            pitch,
            printer_width,
            printer_pitch,
        }),
        long_line_types: LONG_LINE_TYPES.map(|(pattern, pitch, printer_pitch)| JwwLinePattern {
            pattern,
            unit: 32,
            pitch,
            printer_pitch,
        }),
        screen_points: 0,
        printer_points: 0,
        bitmap_first: 1,
        reverse_draw: 0,
        reverse_search: 0,
        color_print: 0,
        layer_order_print: 1,
        color_order_print: 1,
        continuous_print: 0,
        gray_print: 0,
        print_display_only: if version >= 600 { dpi_flag(None) } else { 0 },
        drawing_time: 0,
        eye_set: 0,
        eye_angles: [u32::MAX; 3],
        eye: [2000.0, 1_000_000.0, 1_000_000.0, 1_000_000.0, 35.264_389_682_754_654],
        last_length: 0.0,
        last_box: [0.0, 0.0],
        last_radius: 0.0,
        solid_any_color: 0,
        solid_color: 0x808080,
    }
}

/// SXF の色（RGB）を COLORREF にする
fn sxf_colorref([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

fn sxf_settings() -> JwwSxfSettings {
    let colors: Vec<u32> = (0..257)
        .map(|index| match index {
            1..=16 => sxf_colorref(SXF_COLORS[index - 1]),
            17..=255 => SXF_SPARE_COLOR,
            _ => 0,
        })
        .collect();
    let pens = colors.iter().map(|&color| JwwPen { color, width: 18 }).collect();
    let printer_pens = colors
        .iter()
        .enumerate()
        .map(|(index, &color)| JwwSxfPrinterPen {
            name: index.checked_sub(1).and_then(|n| SXF_COLOR_NAMES.get(n)).map_or_else(String::new, |n| n.to_string()),
            color,
            width: 18,
            point_radius: 0.18,
        })
        .collect();
    let line_types = SXF_LINE_TYPES
        .iter()
        .map(|&(pattern, unit, printer_pitch)| JwwLinePattern {
            pattern,
            unit,
            pitch: u32::from(unit > 0),
            printer_pitch,
        })
        .chain(SXF_SPARE_UNITS.iter().map(|&unit| JwwLinePattern { pattern: 0, unit, pitch: 1, printer_pitch: 10 }))
        .collect();
    let user_line_types = (0..33usize)
        .map(|index| {
            let mut line_type = JwwSxfLineType::default();
            if let Some((name, pitches)) =
                index.checked_sub(1).and_then(|n| SXF_LINE_TYPE_NAMES.get(n).zip(SXF_LINE_PITCHES.get(n)))
            {
                line_type.name = name.to_string();
                line_type.segments = pitches.len() as u32;
                line_type.pitches[..pitches.len()].copy_from_slice(pitches);
            } else if index == 16 {
                line_type.name = "undefined".to_string();
            }
            line_type
        })
        .collect();
    JwwSxfSettings { pens, printer_pens, line_types, user_line_types }
}

fn text_settings() -> JwwTextSettings {
    let style = |(width, height, spacing, color): (f64, f64, f64, u32)| JwwTextStyle { width, height, spacing, color };
    JwwTextSettings {
        styles: TEXT_STYLES.map(style),
        current: style(TEXT_STYLES[2]),
        current_style: 3,
        arrange_spacing: 0.0,
        arrange_count: 0.0,
        offset_enabled: 0,
        offsets_x: [-1.0, 0.0, 1.0],
        offsets_y: [-1.0, 0.0, 1.0],
    }
}

impl JwwDocument {
    /// 新しい図面（版 700、A3、すべてのレイヤグループが 1/1、Jw_cad の既定の色・線幅・線種）。
    /// `read_jww` と同じく、名前のないレイヤグループ・レイヤは「グループ 0」「0-0」のような名前を持つ
    pub fn new() -> Self {
        let written = JwwFile::new(JWW_VERSION, JwwHeader::new(JWW_VERSION)).write();
        read_jww(&written.bytes).unwrap_or_else(|_| empty_document())
    }

    /// レイヤグループの実寸の座標・長さ（縮尺を掛けた mm）を、図形の座標（用紙の mm、用紙の中心が原点）にする倍率
    pub fn paper_factor(&self, group: u16) -> f64 {
        1.0 / self.group_scale(group)
    }
}

impl Default for JwwDocument {
    fn default() -> Self {
        Self::new()
    }
}

/// 読めないはずの既定の図面を読めなかったときの図面
fn empty_document() -> JwwDocument {
    let groups = (0..16)
        .map(|group| JwwLayerGroup {
            name: format!("グループ {group:X}"),
            state: 2,
            scale: 1.0,
            layers: (0..16).map(|layer| JwwLayer { name: format!("{group:X}-{layer:X}"), state: 2 }).collect(),
        })
        .collect();
    JwwDocument {
        version: JWW_VERSION,
        memo: String::new(),
        paper_size: 3,
        groups,
        pen_colors: None,
        extended_colors: None,
        pen_widths: None,
        width_unit: WidthUnit::default(),
        line_types: BTreeMap::new(),
        entities: Vec::new(),
        blocks: Vec::new(),
        truncated: None,
    }
}

/// 図面の内容を JWW に書く。版は doc.version（300〜700。ほかは 700 で書き、notices で知らせる）。
/// 図面の内容にない設定は Jw_cad の新しい図面の既定の値にする
pub fn write_jww(doc: &JwwDocument) -> JwwWritten {
    let mut notices = NoticeList::new();
    let file = file_from_document(doc, &mut notices);
    let written = file.write();
    for notice in &written.notices {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    JwwWritten { bytes: written.bytes, notices: notices.into_vec() }
}

/// 図面の内容から、新しい図面のファイルを作る
pub(crate) fn file_from_document(doc: &JwwDocument, notices: &mut NoticeList) -> JwwFile {
    let version = if WRITABLE_VERSIONS.contains(&doc.version) {
        doc.version
    } else {
        notices.add(
            NoticeLevel::Info,
            "version",
            format!("版 {} の JWW は書けないため、版 {JWW_VERSION} で書きました", doc.version),
        );
        JWW_VERSION
    };
    let mut header = JwwHeader::new(version);
    header.memo = doc.memo.clone();
    header.paper_size = doc.paper_size;
    set_groups(&mut header, &doc.groups, notices);
    if let Some(settings) = &mut header.settings {
        set_names(settings, &doc.groups);
        set_pens(settings, doc, version, notices);
        set_line_types(settings, &doc.line_types, notices);
    }
    if version < 420 {
        let count = sxf_users(doc);
        if count > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "sxf-attributes",
                "版 420 より前の JWW は SXF 対応の線色・線種の定義を持たないため、それを使う図形は Jw_cad の既定の色・線種になります",
                count,
            );
        }
    }
    let mut file = JwwFile::new(version, header);
    let mut older = Older::default();
    file.entities = older.fit(&doc.entities, version).into_iter().map(JwwRecord::new).collect();
    let referenced = referenced_blocks(doc);
    file.blocks = doc
        .blocks
        .iter()
        .map(|block: &JwwBlockDefinition| {
            let entities = older.fit(&block.entities, version).into_iter().map(JwwRecord::new).collect();
            // 版 410 より前のブロック名は、後ろに複合図形の種別（"@@SfigorgFlag@@" と番号）を持たない
            let name = match block.name.split_once("@@SfigorgFlag@@") {
                Some((name, _)) if version < 410 => {
                    older.block_names += 1;
                    name.to_string()
                }
                _ => block.name.clone(),
            };
            let mut record = JwwBlockRecord::new(block.number, name, entities);
            record.referenced = referenced.contains(&block.number);
            record
        })
        .collect();
    older.report(notices);
    file
}

/// 古い版が持たないもの（形式説明の版ごとの注記）を、その版で書ける形にする。数は notices にする
#[derive(Default)]
struct Older {
    /// 版 351 より前: 図形ごとの線幅
    widths: u64,
    /// 版 252 より前: 点の記号（矢印・ポイントマーカー）
    symbols: u64,
    /// 版 230 より前: 円のソリッド（多角形のソリッドにする）
    circle_solids: u64,
    /// 版 410 より前: ブロック名の複合図形の種別
    block_names: u64,
    /// 同梱画像の配置（画像のファイルは図面の内容が持たない）
    images: u64,
}

impl Older {
    fn fit(&mut self, entities: &[JwwEntity], version: u32) -> Vec<JwwEntity> {
        let mut out = Vec::with_capacity(entities.len());
        for entity in entities {
            if let JwwEntity::Text(text) = entity
                && text.content.starts_with("^@BM%temp%")
            {
                self.images += 1;
            }
            if version < 351 && !matches!(entity, JwwEntity::Text(_)) && entity.base().pen_width != 0 {
                self.widths += 1;
            }
            match entity {
                JwwEntity::Point(point) if version < 252 && point.base.pen_style == 100 => {
                    self.symbols += 1;
                    let base = JwwBase { pen_style: 1, ..point.base };
                    out.push(JwwEntity::Point(JwwPoint { base, code: 0, angle: 0.0, scale: 0.0, ..point.clone() }));
                }
                JwwEntity::CircleSolid(solid) if version < 230 => {
                    self.circle_solids += 1;
                    out.extend(circle_solid_parts(solid));
                }
                other => out.push(other.clone()),
            }
        }
        out
    }

    fn report(&self, notices: &mut NoticeList) {
        let reports = [
            (self.widths, "entity-width", "版 351 より前の JWW は図形ごとの線幅を持たないため、線色の線幅になります"),
            (self.symbols, "point-symbol", "版 252 より前の JWW は点の記号を持たないため、ふつうの点にしました"),
            (
                self.circle_solids,
                "circle-solid",
                "版 230 より前の JWW は円のソリッドを持たないため、三角形のソリッドに分けました",
            ),
            (
                self.block_names,
                "block-name",
                "版 410 より前の JWW のブロック名は複合図形の種別を持たないため、除きました",
            ),
            (
                self.images,
                "embedded-image",
                "同梱画像のファイルは図面の内容にないため、画像の配置の文字だけを書きました",
            ),
        ];
        for (count, code, message) in reports {
            if count > 0 {
                notices.add_count(NoticeLevel::Warning, code, message, count);
            }
        }
    }
}

/// 円のソリッドを、表示と同じ多角形（扇形は中心から、全円・弓形は弧だけ）の三角形のソリッドに分ける
fn circle_solid_parts(solid: &JwwCircleSolid) -> Vec<JwwEntity> {
    let full = solid.mode == 100.0 || (solid.sweep.abs() - TAU).abs() < 1e-6;
    let flatness = if solid.flatness == 0.0 || solid.flatness.is_nan() { 1.0 } else { solid.flatness };
    let (start, sweep) = if full { (0.0, TAU) } else { (solid.start, solid.sweep) };
    let segments = ((sweep.abs() / (PI / 16.0)).ceil() as usize).clamp(4, 64);
    let (cos, sin) = (solid.tilt.cos(), solid.tilt.sin());
    let radius = solid.radius.abs();
    let mut ring: Vec<(f64, f64)> = (0..=segments)
        .map(|i| {
            let t = start + sweep * i as f64 / segments as f64;
            let (x, y) = (radius * t.cos(), radius * flatness * t.sin());
            (solid.cx + x * cos - y * sin, solid.cy + x * sin + y * cos)
        })
        .collect();
    if full {
        ring.pop();
    } else if solid.mode != -1.0 && solid.mode != 5.0 {
        ring.insert(0, (solid.cx, solid.cy));
    }
    let base = JwwBase { pen_style: 1, ..solid.base };
    (1..ring.len().saturating_sub(1))
        .map(|i| {
            let (a, b, c) = (ring[0], ring[i], ring[i + 1]);
            JwwEntity::Solid(JwwSolid { base, points: [a.0, a.1, b.0, b.1, c.0, c.1, c.0, c.1], color: solid.color })
        })
        .collect()
}

/// レイヤグループ・レイヤの状態と縮尺。Jw_cad と同じく、書込みレイヤグループと各グループの書込みレイヤだけを
/// 状態 3 にする（状態 3 がなければ、最初の編集可能なもの（状態 2）を書込みにする）
fn set_groups(header: &mut JwwHeader, groups: &[JwwLayerGroup], notices: &mut NoticeList) {
    let write_index = |states: &[u32]| -> usize {
        states.iter().position(|&state| state == 3).or_else(|| states.iter().position(|&state| state == 2)).unwrap_or(0)
    };
    // 書込みのものは編集可能なら 3、ほかの 3 は 2
    let state = |state: u32, write: bool| match state {
        2 | 3 if write => 3,
        3 => 2,
        other => other,
    };
    let group_states: Vec<u32> = (0..16).map(|g| groups.get(g).map_or(2, |group| group.state)).collect();
    let write_group = write_index(&group_states);
    header.write_group = write_group as u32;
    for (g, header_group) in header.groups.iter_mut().enumerate() {
        header_group.state = state(group_states[g], g == write_group);
        let Some(group) = groups.get(g) else {
            continue;
        };
        // 縮尺の分母は正の数（読み取りも 0 以下を 1 として描く）
        if group.scale.is_finite() && group.scale > 0.0 {
            header_group.scale = group.scale;
        } else {
            notices.add(NoticeLevel::Warning, "invalid-scale", "正の数でないレイヤグループの縮尺を 1/1 にしました");
        }
        let layer_states: Vec<u32> = (0..16).map(|l| group.layers.get(l).map_or(2, |layer| layer.state)).collect();
        let write_layer = write_index(&layer_states);
        header_group.write_layer = write_layer as u32;
        for (l, layer) in header_group.layers.iter_mut().enumerate() {
            layer.state = state(layer_states[l], l == write_layer);
        }
    }
}

/// 名前。読み取りが名前のないものに付ける名前（グループ 0、0-0 など）は空にする
fn set_names(settings: &mut JwwSettings, groups: &[JwwLayerGroup]) {
    for (g, group) in groups.iter().take(16).enumerate() {
        if group.name != format!("グループ {g:X}") {
            settings.group_names[g] = group.name.clone();
        }
        for (l, layer) in group.layers.iter().take(16).enumerate() {
            if !layer.name.eq_ignore_ascii_case(&format!("{g:X}-{l:X}")) {
                settings.layer_names[g * 16 + l] = layer.name.clone();
            }
        }
    }
}

/// "#rrggbb" を COLORREF にする
fn parse_color(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(((value >> 16) & 0xff) | (value & 0xff00) | ((value & 0xff) << 16))
}

/// 線色の画面色・SXF 対応色・印刷の線幅と、線幅の単位
fn set_pens(settings: &mut JwwSettings, doc: &JwwDocument, version: u32, notices: &mut NoticeList) {
    let mut invalid = 0u64;
    let WidthUnit { hundredth, dpi } = doc.width_unit;
    settings.general.max_draw_width = match (hundredth, version >= 600) {
        (true, true) => -(MAX_DRAW_WIDTH + 200),
        (true, false) => -101,
        (false, _) => MAX_DRAW_WIDTH,
    };
    let Some(display) = &mut settings.display else {
        return;
    };
    display.print_display_only = if version >= 600 { dpi_flag(dpi) } else { 0 };
    if let Some(colors) = &doc.pen_colors {
        for (pen, color) in display.pens.iter_mut().zip(colors) {
            match parse_color(color) {
                Some(value) => pen.color = value,
                None => invalid += 1,
            }
        }
    }
    // 線幅（mm）を、1/100 mm か印刷のドットにする。版 600 より前のドットは dpi が分からないので書けない
    let dots = |mm: f64| -> Option<u32> {
        let value = if hundredth { mm * 100.0 } else { mm * dpi.filter(|_| version >= 600)? / 25.4 };
        (value.is_finite() && value >= 0.0).then(|| value.round().min(f64::from(u32::MAX)) as u32)
    };
    let mut unwritten = 0u64;
    for (pen, width) in display.printer_pens.iter_mut().zip(doc.pen_widths.iter().flatten()) {
        let Some(width) = width else {
            continue;
        };
        match dots(*width) {
            Some(value) => pen.width = value,
            None => unwritten += 1,
        }
    }
    if unwritten > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "pen-width",
            "線幅の単位（1/100 mm か印刷の dpi）が決まらない線色の線幅は、Jw_cad の既定の線幅にしました",
            unwritten,
        );
    }
    if let (Some(sxf), Some(colors)) = (&mut settings.sxf, &doc.extended_colors) {
        for ((pen, printer), color) in sxf.pens.iter_mut().zip(sxf.printer_pens.iter_mut()).zip(colors) {
            match parse_color(color) {
                Some(value) => {
                    pen.color = value;
                    printer.color = value;
                }
                None => invalid += 1,
            }
        }
    }
    if invalid > 0 {
        notices.add_count(NoticeLevel::Warning, "invalid-color", "#rrggbb でない色は既定の色にしました", invalid);
    }
}

/// 線種の線分・空白の長さ（mm）。線種 2〜9・16〜19 はパターンのビット列に、SXF 対応線種（30〜62）は長さの
/// まま書く。Jw_cad の既定と同じものは既定のまま
fn set_line_types(settings: &mut JwwSettings, line_types: &BTreeMap<u32, Vec<f64>>, notices: &mut NoticeList) {
    let mut unwritten = 0u64;
    for (&style, segments) in line_types {
        let written = match style {
            2..=9 | 16..=19 => {
                let Some(display) = &mut settings.display else {
                    continue;
                };
                let slot = if style <= 9 {
                    &mut display.line_types[style as usize - 2]
                } else {
                    &mut display.long_line_types[style as usize - 16]
                };
                if pattern_segments(slot.pattern, slot.unit, slot.printer_pitch) == *segments {
                    continue;
                }
                match encode_pattern(segments) {
                    Some(pattern) => {
                        *slot = pattern;
                        true
                    }
                    None => false,
                }
            }
            30..=62 => {
                // 版 420 より前は SXF 対応線種の定義を持たない（使う図形は file_from_document が知らせる）
                let Some(sxf) = &mut settings.sxf else {
                    continue;
                };
                let slot = &mut sxf.user_line_types[style as usize - 30];
                if slot.pitches[..(slot.segments as usize).min(10)] == segments[..] {
                    continue;
                }
                if segments.len() > 10 || segments.iter().any(|value| !value.is_finite() || *value < 0.0) {
                    false
                } else {
                    slot.segments = segments.len() as u32;
                    slot.pitches = [0.0; 10];
                    slot.pitches[..segments.len()].copy_from_slice(segments);
                    true
                }
            }
            _ => false,
        };
        if !written {
            unwritten += 1;
        }
    }
    if unwritten > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "line-type",
            "JWW の線種に書けない線分・空白の長さは、Jw_cad の既定の線種のままにしました",
            unwritten,
        );
    }
}

/// 線分・空白の長さ（mm、線分から始まり偶数個）を、1 ビットがプリンタ出力ピッチ × 1/32 mm のパターンにする。
/// 読み取り（pattern_segments）で同じ長さに戻るピッチがなければ None
fn encode_pattern(segments: &[f64]) -> Option<JwwLinePattern> {
    let positive = segments.iter().all(|value| value.is_finite() && *value > 0.0);
    if segments.is_empty() || !segments.len().is_multiple_of(2) || !positive {
        return None;
    }
    (1..=4096u32).find_map(|printer_pitch| {
        let bit = f64::from(printer_pitch) / 32.0;
        let runs: Vec<u32> = segments.iter().map(|value| (value / bit).round() as u32).collect();
        let total: u64 = runs.iter().map(|run| u64::from(*run)).sum();
        let exact = runs.iter().zip(segments).all(|(run, value)| *run >= 1 && f64::from(*run) * bit == *value);
        if !exact || total > 32 {
            return None;
        }
        let unit = total as u32;
        // 線分から始め、1 周期を 32 ビットに繰り返す
        let mut cycle = 0u32;
        let mut position = 0;
        for (index, run) in runs.iter().enumerate() {
            if index % 2 == 0 {
                for offset in 0..*run {
                    cycle |= 1 << (position + offset);
                }
            }
            position += run;
        }
        let mut pattern = 0u32;
        let mut shift = 0;
        while shift < 32 {
            pattern |= cycle << shift;
            shift += unit;
        }
        Some(JwwLinePattern { pattern, unit, pitch: 1, printer_pitch })
    })
}

/// SXF 対応の線色（100〜356）か線種（30〜62）を使う図形の数
fn sxf_users(doc: &JwwDocument) -> u64 {
    let color = |base: &JwwBase| (100..=356).contains(&base.pen_color);
    let style = |base: &JwwBase| (30..=62).contains(&base.pen_style);
    let all = doc.entities.iter().chain(doc.blocks.iter().flat_map(|block| block.entities.iter()));
    all.filter(|entity| match entity {
        JwwEntity::Line(e) => color(&e.base) || style(&e.base),
        JwwEntity::Arc(e) => color(&e.base) || style(&e.base),
        JwwEntity::Dimension(e) => {
            color(&e.line.base)
                || style(&e.line.base)
                || e.lines.iter().any(|line| color(&line.base) || style(&line.base))
        }
        other => color(other.base()),
    })
    .count() as u64
}

/// ブロックの配置が参照する定義の番号（ブロックの中の配置も）
fn referenced_blocks(doc: &JwwDocument) -> BTreeSet<u32> {
    let all = doc.entities.iter().chain(doc.blocks.iter().flat_map(|block| block.entities.iter()));
    all.filter_map(|entity| match entity {
        JwwEntity::Block(block) => Some(block.definition),
        _ => None,
    })
    .collect()
}
