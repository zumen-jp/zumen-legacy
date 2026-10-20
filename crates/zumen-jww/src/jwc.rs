//! Jw_cad for DOS の図面ファイル JWC を読み書きする（docs/formats/jww.md「JWC」）。
//!
//! JWC の公式の形式説明は見つからない。並びは ezjww（MIT、https://github.com/monozukuri-ai/ezjww）が、Jw_cad 10.02.1 が
//! 保存した JWC と元の図形を比べて調べた結果（docs/JWC_FORMAT.md と jwc の読み取り）に拠る。
//!
//! - 固定長のヘッダ（縮尺が f32 なら 2421 バイト、u16 なら 2389 バイト）: 40 バイトの識別子、改行で区切った 3 つの CSV の
//!   欄（件数・書込みレイヤ・用紙など、設定、文字列の領域の DOS のアドレス）、仮点の表、文字種の表、レイヤグループの
//!   縮尺、レイヤグループとレイヤの編集・表示の状態、各グループの書込みレイヤ。
//! - その後ろに、線（22 バイト）、円弧（32）、文字（24）、文字列の領域、点（12）、レイヤとグループの名前（2304）。
//! - 座標は単精度で、用紙の左下が原点、1 は用紙の幅 / 518 mm。
//!
//! 読んだファイルは、ヘッダの知らない欄も元のバイト列のまま持ち、書くと元と同じバイト列になる。

use crate::archive::{Result, encode_shift_jis};
use crate::create::JWW_VERSION;
use crate::edit::DEFAULT_FONT;
use crate::file::{JwwWritten, stop_at_limit};
use crate::reader::{
    JwwArc, JwwBase, JwwDocument, JwwEntity, JwwLayer, JwwLayerGroup, JwwLine, JwwPoint, JwwText, WidthUnit,
};
use crate::style::paper_size;
use encoding_rs::SHIFT_JIS;
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use zumen_core::{NoticeLevel, NoticeList, ReadError};

/// JWC の識別子
pub const JWC_SIGNATURE: &[u8; 13] = b"jw_cad(c)data";

/// 識別子の 40 バイト（23 バイト目は縮尺の書き方: f は f32、. は u16）
const SIGNATURE: &[u8; 40] = b"jw_cad(c)data.......a.f.m...............";
const VARIANT: usize = 22;
/// 3 つの CSV の欄（各 199 バイトと改行）
const CSV: [usize; 3] = [200, 400, 600];
const CSV_LENGTH: usize = 199;
/// 仮点の表（番号 1〜100。0 は使わない）: X（f32）、Y（f32）、レイヤ（グループ << 4 | レイヤ）
const TEMPORARY_X: usize = 800;
const TEMPORARY_Y: usize = 1204;
const TEMPORARY_LAYER: usize = 1608;
const TEMPORARY_SLOTS: usize = 101;
/// 文字種 0〜10 の線色・幅・高さ・間隔（u16、幅・高さ・間隔は 0.1 mm）
const TEXT_STYLES: usize = 1709;
/// レイヤグループの縮尺
const SCALES: usize = 1797;
/// 名前の領域（レイヤ名 8 バイト × 256、レイヤグループ名 16 バイト × 16）
const NAMES_LENGTH: usize = 2304;
const LAYER_NAME: usize = 8;
const GROUP_NAME: usize = 16;
/// レコードの長さ
const LINE: usize = 22;
const ARC: usize = 32;
const TEXT: usize = 24;
const POINT: usize = 12;
/// 用紙の幅の座標の数（CSV0 の 30 番）
const EXTENT: f64 = 518.0;
/// 図形の数の上限（読み取り）
const MAX_RECORDS: usize = 1_000_000;
/// 文字列の領域の上限（DOS の 1 つのセグメント）
const MAX_POOL: usize = 0xffff;
/// 書くときにたどる図形の数の上限（ブロックの分解で図形が増えすぎないように）
const MAX_VISITS: usize = 4 * MAX_RECORDS;

/// 名前と文字列。text は Shift_JIS を復号したもの。raw は元のバイト列（名前は欄の全体）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcString {
    pub text: String,
    raw: Vec<u8>,
}

impl JwcString {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), raw: Vec::new() }
    }

    fn from_bytes(raw: &[u8]) -> Self {
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Self { text: SHIFT_JIS.decode_without_bom_handling(&raw[..end]).0.into_owned(), raw: raw.to_vec() }
    }

    /// 元のバイト列（text が変わっていなければ）か、Shift_JIS にしたもの。長さは length（足りなければ 0 で埋め、
    /// 長ければ文字の途中で切らずに縮める）。表せない文字は ? にし、切ったか・? にしたかを返す
    fn slot(&self, length: usize) -> (Vec<u8>, bool) {
        if !self.raw.is_empty() && Self::from_bytes(&self.raw).text == self.text && self.raw.len() == length {
            return (self.raw.clone(), false);
        }
        let (mut bytes, lossy) = shift_jis(&self.text, length - 1);
        bytes.resize(length, 0);
        (bytes, lossy)
    }
}

/// Shift_JIS にする。表せない文字は ?、limit バイトを超えるところは文字の途中で切らずに除く。失ったものがあれば true
fn shift_jis(text: &str, limit: usize) -> (Vec<u8>, bool) {
    let (mut bytes, replaced) = encode_shift_jis(text);
    let mut lossy = replaced > 0;
    if bytes.len() > limit {
        let mut end = 0;
        while end < limit {
            let lead = bytes[end];
            let width = if (0x81..=0x9f).contains(&lead) || (0xe0..=0xfc).contains(&lead) { 2 } else { 1 };
            if end + width > limit {
                break;
            }
            end += width;
        }
        bytes.truncate(end);
        lossy = true;
    }
    (bytes, lossy)
}

/// レイヤ
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcLayer {
    /// 編集の状態（1 ビット目: 編集できる、2 ビット目: プロテクト）
    pub edit: u8,
    /// 表示の状態（1 ビット目: 表示）
    pub visible: u8,
    pub name: JwcString,
}

/// レイヤグループ
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcGroup {
    /// 縮尺の分母
    pub scale: f64,
    pub edit: u8,
    pub visible: u8,
    /// 書込みレイヤ
    pub write_layer: u8,
    pub name: JwcString,
    /// 16 のレイヤ
    pub layers: Vec<JwcLayer>,
}

/// 文字種の線色と、幅・高さ・間隔（0.1 mm）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JwcTextStyle {
    pub color: u16,
    pub width: u16,
    pub height: u16,
    pub spacing: u16,
}

/// 線と円弧の属性
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JwcAttributes {
    /// 線種（下位 4 ビットが 1〜9）
    pub style: u8,
    /// 線色 1〜9
    pub color: u8,
    /// レイヤ（グループ << 4 | レイヤ）
    pub layer: u8,
    pub spare: u8,
    /// 曲線の印などの旗（0x40 曲線の始まり、0x80 続き、0xC0 終わり）
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcLine {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub attributes: JwcAttributes,
}

/// 円弧。角度は 1/65536 度、扁平率は 1/10000。始角と終角が同じなら全円
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcArc {
    pub center: [f32; 2],
    pub radius: f32,
    pub flatness: u16,
    pub start: i32,
    pub end: i32,
    pub tilt: i32,
    pub attributes: JwcAttributes,
}

/// 文字。大きさと色は文字種（1〜10）。向きは始点から終点
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcText {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub style: u8,
    pub layer: u8,
    pub flags: u16,
    pub content: JwcString,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcPoint {
    pub position: [f32; 2],
    pub layer: u8,
    pub color: u8,
    pub flags: u16,
}

/// 仮点（ヘッダの表。印刷されない）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JwcTemporaryPoint {
    pub position: [f32; 2],
    pub layer: u8,
}

/// JWC ファイル
#[derive(Debug, Clone, PartialEq)]
pub struct JwcFile {
    /// 縮尺を f32 で書く（2421 バイトのヘッダ）か、u16 で書く（2389 バイト）か
    pub float_scales: bool,
    /// 用紙（0〜4: A0〜A4）
    pub paper_size: u32,
    pub write_group: u8,
    pub write_layer: u8,
    /// 16 のレイヤグループ
    pub groups: Vec<JwcGroup>,
    /// 文字種 0〜10（0 は使わない）
    pub text_styles: Vec<JwcTextStyle>,
    /// 仮点（100 まで）
    pub temporary_points: Vec<JwcTemporaryPoint>,
    pub lines: Vec<JwcLine>,
    pub arcs: Vec<JwcArc>,
    pub texts: Vec<JwcText>,
    pub points: Vec<JwcPoint>,
    /// 固定長のヘッダ。上の値の欄は書くときに書き直し、ほかはそのまま書く
    fixed: Vec<u8>,
}

/// JWC ファイルか（識別子）
pub fn is_jwc(bytes: &[u8]) -> bool {
    bytes.starts_with(JWC_SIGNATURE)
}

fn invalid(message: impl Into<String>) -> ReadError {
    ReadError::new(format!("JWC を読めません: {}", message.into()))
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// CSV の欄の値（NUL まで、カンマ区切り）
fn csv_fields(fixed: &[u8], block: usize) -> Vec<String> {
    let start = CSV[block];
    let data = &fixed[start..start + CSV_LENGTH];
    let end = data.iter().position(|&b| b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).split(',').map(|field| field.trim().to_string()).collect()
}

/// CSV の欄の値を変える。数・DOS のアドレスとして同じ値なら元の書き方のまま（変わらなければ元のバイト列のまま）
fn patch_csv(fixed: &mut [u8], block: usize, updates: &[(usize, String)]) {
    let mut fields = csv_fields(fixed, block);
    let mut changed = false;
    let same = |current: &str, value: &str| {
        current == value
            || matches!((current.parse::<f64>(), value.parse::<f64>()), (Ok(a), Ok(b)) if a == b)
            || matches!((far_pointer(current), far_pointer(value)), (Some(a), Some(b)) if a == b)
    };
    for (index, value) in updates {
        if fields.len() <= *index {
            fields.resize(*index + 1, "0".into());
        }
        if !same(&fields[*index], value) {
            fields[*index] = value.clone();
            changed = true;
        }
    }
    if !changed {
        return;
    }
    let text = fields.join(",");
    let start = CSV[block];
    let slot = &mut fixed[start..start + CSV_LENGTH];
    let length = text.len().min(CSV_LENGTH - 1);
    slot[..length].copy_from_slice(&text.as_bytes()[..length]);
    slot[length] = 0;
    slot[length + 1..].fill(b' ');
}

/// DOS のアドレス（SSSS:OOOO、16 進）
fn far_pointer(text: &str) -> Option<u32> {
    let (segment, offset) = text.split_once(':')?;
    let parse = |part: &str| (part.len() == 4).then(|| u32::from_str_radix(part, 16).ok()).flatten();
    Some((parse(segment)? << 16) | parse(offset)?)
}

/// DOS のアドレスの書き方（Jw_cad と同じく 16 進の小文字）
fn far_pointer_text(value: u32) -> String {
    format!("{:04x}:{:04x}", value >> 16, value & 0xffff)
}

/// 数の欄（整数は整数で）
fn number_text(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e9 { format!("{}", value as i64) } else { format!("{value}") }
}

impl JwcFile {
    fn header_length(float_scales: bool) -> usize {
        Self::write_layers(float_scales) + 16
    }

    fn edit_flags(float_scales: bool) -> usize {
        SCALES + if float_scales { 64 } else { 32 }
    }

    fn visible_flags(float_scales: bool) -> usize {
        Self::edit_flags(float_scales) + 272
    }

    fn write_layers(float_scales: bool) -> usize {
        Self::visible_flags(float_scales) + 272
    }

    /// JWC を読む。並びが違えばエラー（途中までは読まない）
    pub fn read(bytes: &[u8]) -> Result<Self> {
        if !is_jwc(bytes) {
            return Err(ReadError::new("JWC ファイルではありません"));
        }
        let signature = bytes.get(..SIGNATURE.len()).ok_or_else(|| invalid("識別子が途中で終わっています"))?;
        let float_scales = match signature[VARIANT] {
            b'f' => true,
            b'.' => false,
            other => return Err(invalid(format!("識別子の種類 {:?} は知りません", char::from(other)))),
        };
        let length = Self::header_length(float_scales);
        let fixed = bytes.get(..length).ok_or_else(|| invalid("ヘッダが途中で終わっています"))?.to_vec();
        if [199, 399, 599, 799].iter().any(|&at| fixed[at] != b'\n') {
            return Err(invalid("ヘッダの改行の位置が違います"));
        }
        let csv0 = csv_fields(&fixed, 0);
        let csv2 = csv_fields(&fixed, 2);
        let count = |index: usize| -> Result<usize> {
            let value = csv0.get(index).and_then(|field| field.parse::<usize>().ok());
            value.filter(|value| *value <= MAX_RECORDS).ok_or_else(|| invalid(format!("件数の欄 {index} が読めません")))
        };
        let (lines, arcs, texts, points, temporary) = (count(0)?, count(1)?, count(2)?, count(3)?, count(4)?);
        if temporary >= TEMPORARY_SLOTS {
            return Err(invalid("仮点が 100 を超えています"));
        }
        let write = count(10)?;
        let paper_size = count(11)? as u32;
        let pool_start = csv2
            .first()
            .and_then(|text| far_pointer(text))
            .ok_or_else(|| invalid("文字列の領域の始まりが読めません"))?;
        let pool_end = csv2
            .get(1)
            .and_then(|text| far_pointer(text))
            .ok_or_else(|| invalid("文字列の領域の終わりが読めません"))?;
        let pool_length =
            pool_end.checked_sub(pool_start).map(|length| length as usize).filter(|length| *length <= MAX_POOL);
        let pool_length = pool_length.ok_or_else(|| invalid("文字列の領域の長さが読めません"))?;
        let lines_at = length;
        let arcs_at = lines_at + lines * LINE;
        let texts_at = arcs_at + arcs * ARC;
        let pool_at = texts_at + texts * TEXT;
        let points_at = pool_at + pool_length;
        let names_at = points_at + points * POINT;
        if bytes.len() != names_at + NAMES_LENGTH {
            return Err(invalid("図形の件数とファイルの長さが合いません"));
        }
        let names = &bytes[names_at..];
        let edit = Self::edit_flags(float_scales);
        let visible = Self::visible_flags(float_scales);
        let write_layers = Self::write_layers(float_scales);
        let groups = (0..16)
            .map(|g| {
                let scale = if float_scales {
                    f64::from(f32_at(&fixed, SCALES + 4 * g))
                } else {
                    f64::from(u16_at(&fixed, SCALES + 2 * g))
                };
                let layers = (0..16)
                    .map(|l| {
                        let index = 16 + 16 * g + l;
                        let at = LAYER_NAME * (16 * g + l);
                        JwcLayer {
                            edit: fixed[edit + index],
                            visible: fixed[visible + index],
                            name: JwcString::from_bytes(&names[at..at + LAYER_NAME]),
                        }
                    })
                    .collect();
                let at = LAYER_NAME * 256 + GROUP_NAME * g;
                JwcGroup {
                    scale,
                    edit: fixed[edit + g],
                    visible: fixed[visible + g],
                    write_layer: fixed[write_layers + g] & 0x0f,
                    name: JwcString::from_bytes(&names[at..at + GROUP_NAME]),
                    layers,
                }
            })
            .collect();
        let text_styles = (0..11)
            .map(|i| JwcTextStyle {
                color: u16_at(&fixed, TEXT_STYLES + 2 * i),
                width: u16_at(&fixed, TEXT_STYLES + 22 + 2 * i),
                height: u16_at(&fixed, TEXT_STYLES + 44 + 2 * i),
                spacing: u16_at(&fixed, TEXT_STYLES + 66 + 2 * i),
            })
            .collect();
        let temporary_points = (1..=temporary)
            .map(|i| JwcTemporaryPoint {
                position: [f32_at(&fixed, TEMPORARY_X + 4 * i), f32_at(&fixed, TEMPORARY_Y + 4 * i)],
                layer: fixed[TEMPORARY_LAYER + i],
            })
            .collect();
        let xy = |at: usize| [f32_at(bytes, at), f32_at(bytes, at + 4)];
        let attributes = |at: usize| JwcAttributes {
            style: bytes[at],
            color: bytes[at + 1],
            layer: bytes[at + 2],
            spare: bytes[at + 3],
            flags: u16_at(bytes, at + 4),
        };
        // 読み取りの上限（zumen_core::limits）を超えたら、途中までは読まずにエラーにする
        let lines = (0..lines)
            .map(|i| {
                stop_at_limit()?;
                let at = lines_at + i * LINE;
                Ok(JwcLine { start: xy(at), end: xy(at + 8), attributes: attributes(at + 16) })
            })
            .collect::<Result<_>>()?;
        let arcs = (0..arcs)
            .map(|i| {
                stop_at_limit()?;
                let at = arcs_at + i * ARC;
                Ok(JwcArc {
                    center: xy(at),
                    radius: f32_at(bytes, at + 8),
                    flatness: u16_at(bytes, at + 12),
                    start: u32_at(bytes, at + 14) as i32,
                    end: u32_at(bytes, at + 18) as i32,
                    tilt: u32_at(bytes, at + 22) as i32,
                    attributes: attributes(at + 26),
                })
            })
            .collect::<Result<_>>()?;
        // 文字の文字列は、領域の中に文字の順に並ぶ。文字の内容は最初の NUL まで。次の文字の場所（最後の文字は領域の
        // 終わり）までを元のバイト列のまま持つ（Jw_cad は 256 バイトを超える文字を NUL で切り、残りをそのまま書く）
        let pool = &bytes[pool_at..points_at];
        let references: Vec<usize> =
            (0..texts).map(|i| u32_at(bytes, texts_at + i * TEXT + 16).wrapping_sub(pool_start) as usize).collect();
        let mut text_records = Vec::with_capacity(texts);
        for i in 0..texts {
            stop_at_limit()?;
            let at = texts_at + i * TEXT;
            let begin = references[i];
            let finish = references.get(i + 1).copied().unwrap_or(pool.len());
            let expected = if i == 0 { 0 } else { references[i - 1] };
            if begin < expected || finish < begin || finish > pool.len() || (i == 0 && begin != 0) {
                return Err(invalid("文字の文字列の場所が並びと合いません"));
            }
            let slot = &pool[begin..finish];
            if !slot.contains(&0) {
                return Err(invalid("文字列が NUL で終わっていません"));
            }
            text_records.push(JwcText {
                start: xy(at),
                end: xy(at + 8),
                style: bytes[at + 20],
                layer: bytes[at + 21],
                flags: u16_at(bytes, at + 22),
                content: JwcString::from_bytes(slot),
            });
        }
        if texts == 0 && !pool.is_empty() {
            return Err(invalid("文字列の領域に、文字が使わないバイトがあります"));
        }
        let points = (0..points)
            .map(|i| {
                stop_at_limit()?;
                let at = points_at + i * POINT;
                Ok(JwcPoint {
                    position: xy(at),
                    layer: bytes[at + 8],
                    color: bytes[at + 9],
                    flags: u16_at(bytes, at + 10),
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            float_scales,
            paper_size,
            write_group: (write >> 4) as u8 & 0x0f,
            write_layer: write as u8 & 0x0f,
            groups,
            text_styles,
            temporary_points,
            lines,
            arcs,
            texts: text_records,
            points,
            fixed,
        })
    }

    /// 新しい JWC（Jw_cad 10 が書く 2421 バイトのヘッダ。用紙は A3、縮尺はすべて 1/1、レイヤ・グループは編集・表示）
    pub fn new() -> Self {
        let mut fixed = vec![0u8; Self::header_length(true)];
        fixed[..SIGNATURE.len()].copy_from_slice(SIGNATURE);
        // 識別子の後ろは CR LF、0、空白、改行
        fixed[40] = b'\r';
        fixed[41] = b'\n';
        fixed[104..199].fill(b' ');
        for (block, text) in [
            (0, "0,0,0,0,0,3,2,1,1,1,0,3,2,1,0,0,5,10,20,1,2,3,4,5,6,5,5,259,183.155,0,518,0"),
            (
                1,
                "0.5,0.0,3.0,15.0,1,1,0,0,0,2,0.0,0.0,0.0,0.0,0.0,0.0,0,0,0,1000.000,100.000,200.000,300.000,400.000,500.000,0,0,1",
            ),
            (2, "4000:0000,4000:0000,0,0,1,0,0,1,0,0,1,0,0,1,0,0,1,36,-23.45,1.5,0,-1,14,0"),
        ] {
            let start = CSV[block];
            fixed[start..start + text.len()].copy_from_slice(text.as_bytes());
            fixed[start + text.len() + 1..start + CSV_LENGTH].fill(b' ');
            fixed[start + CSV_LENGTH] = b'\n';
        }
        fixed[199] = b'\n';
        // 各グループの書込みレイヤ（上位 4 ビットはグループの番号）
        let write_layers = Self::write_layers(true);
        for g in 0..16 {
            fixed[write_layers + g] = (g as u8) << 4;
        }
        let text_styles = [
            (0, 0, 0),
            (1, 20, 0),
            (1, 25, 0),
            (2, 30, 5),
            (2, 40, 5),
            (3, 50, 5),
            (3, 60, 5),
            (4, 70, 10),
            (4, 80, 10),
            (5, 90, 10),
            (5, 100, 10),
        ]
        .iter()
        .map(|&(color, size, spacing)| JwcTextStyle { color, width: size, height: size, spacing })
        .collect();
        let groups = (0..16)
            .map(|_| JwcGroup {
                scale: 1.0,
                edit: 1,
                visible: 1,
                write_layer: 0,
                name: JwcString::default(),
                layers: vec![JwcLayer { edit: 1, visible: 1, name: JwcString::default() }; 16],
            })
            .collect();
        Self {
            float_scales: true,
            paper_size: 3,
            write_group: 0,
            write_layer: 0,
            groups,
            text_styles,
            temporary_points: Vec::new(),
            lines: Vec::new(),
            arcs: Vec::new(),
            texts: Vec::new(),
            points: Vec::new(),
            fixed,
        }
    }

    /// JWC に書く。読んだファイルで値が変わっていない欄は元のバイト列のまま
    pub fn write(&self) -> JwwWritten {
        let mut notices = NoticeList::new();
        let float_scales = self.float_scales;
        let length = Self::header_length(float_scales);
        let mut fixed = self.fixed.clone();
        // 縮尺の書き方を変えたときは、縮尺から後ろの欄をこの書き方で作り直す
        let relayout = fixed.len() != length;
        if relayout {
            fixed.resize(length, 0);
        }
        fixed[VARIANT] = if float_scales { b'f' } else { b'.' };
        let mut lossy_names = 0u64;
        let mut lossy_text = 0u64;
        // 文字列の領域と、文字の文字列の場所
        let csv2 = csv_fields(&fixed, 2);
        let mut pool_start = csv2.first().and_then(|text| far_pointer(text)).unwrap_or(0x4000_0000);
        let mut pool = Vec::new();
        let mut references = Vec::with_capacity(self.texts.len());
        let mut kept = 0usize;
        for text in &self.texts {
            // 読んだ文字が変わっていなければ、元の領域（NUL とその後ろを含む）のまま
            let (raw, lossy) =
                if !text.content.raw.is_empty() && JwcString::from_bytes(&text.content.raw).text == text.content.text {
                    (text.content.raw.clone(), false)
                } else {
                    let (mut raw, lossy) = shift_jis(&text.content.text, 255);
                    raw.push(0);
                    (raw, lossy)
                };
            lossy_text += u64::from(lossy);
            if pool.len() + raw.len() > MAX_POOL {
                break;
            }
            references.push(pool_start.wrapping_add(pool.len() as u32));
            pool.extend_from_slice(&raw);
            kept += 1;
        }
        // 追加した文字列が元の DOS アドレスの範囲を超えたときだけ、領域と各文字の参照を移す。
        if pool_start.checked_add(pool.len() as u32).is_none() {
            for reference in &mut references {
                *reference = 0x4000_0000 + reference.wrapping_sub(pool_start);
            }
            pool_start = 0x4000_0000;
            patch_csv(&mut fixed, 2, &[(0, far_pointer_text(pool_start))]);
        }
        if kept < self.texts.len() {
            notices.add_count(
                NoticeLevel::Warning,
                "jwc-text-pool",
                "JWC の文字列の領域（64 KB）に入らない文字は書いていません",
                (self.texts.len() - kept) as u64,
            );
        }
        let temporary = self.temporary_points.len().min(TEMPORARY_SLOTS - 1);
        if temporary < self.temporary_points.len() {
            notices.add_count(
                NoticeLevel::Warning,
                "jwc-temporary-points",
                "JWC の仮点は 100 までのため、残りを書いていません",
                (self.temporary_points.len() - temporary) as u64,
            );
        }
        let write = (u32::from(self.write_group & 0x0f) << 4) | u32::from(self.write_layer & 0x0f);
        let write_scale = self.groups.get(usize::from(self.write_group & 0x0f)).map_or(1.0, |group| group.scale);
        let mut csv0 = vec![
            (0, self.lines.len().to_string()),
            (1, self.arcs.len().to_string()),
            (2, kept.to_string()),
            (3, self.points.len().to_string()),
            (4, temporary.to_string()),
            (10, write.to_string()),
            (11, self.paper_size.to_string()),
        ];
        csv0.push((9, number_text(write_scale)));
        let fields = csv_fields(&fixed, 0);
        // 用紙の縦の半分（座標の数）。用紙が変わったときだけ書き直す
        if let Some((_, width, height)) = paper_size(self.paper_size) {
            let half = EXTENT / 2.0 * height / width;
            let current = fields.get(28).and_then(|field| field.parse::<f64>().ok());
            if current.is_none_or(|current| (current - half).abs() > 0.0005) {
                csv0.push((28, format!("{half:.3}")));
            }
        }
        patch_csv(&mut fixed, 0, &csv0);
        patch_csv(&mut fixed, 2, &[(1, far_pointer_text(pool_start.wrapping_add(pool.len() as u32)))]);
        // 仮点の表（番号 1〜100）。使う番号を書き、読んだときより減った番号は 0 にする。ほかの番号は元のまま
        let before = csv_fields(&self.fixed, 0)
            .get(4)
            .and_then(|field| field.parse::<usize>().ok())
            .map_or(0, |count| count.min(TEMPORARY_SLOTS - 1));
        for i in 1..=temporary.max(before) {
            let point = self.temporary_points.get(i - 1).filter(|_| i <= temporary);
            let (x, y, layer) =
                point.map_or((0.0, 0.0, 0), |point| (point.position[0], point.position[1], point.layer));
            put_f32(&mut fixed, TEMPORARY_X + 4 * i, x, point.is_some());
            put_f32(&mut fixed, TEMPORARY_Y + 4 * i, y, point.is_some());
            fixed[TEMPORARY_LAYER + i] = layer;
        }
        for (i, style) in self.text_styles.iter().take(11).enumerate() {
            put_u16(&mut fixed, TEXT_STYLES + 2 * i, style.color);
            put_u16(&mut fixed, TEXT_STYLES + 22 + 2 * i, style.width);
            put_u16(&mut fixed, TEXT_STYLES + 44 + 2 * i, style.height);
            put_u16(&mut fixed, TEXT_STYLES + 66 + 2 * i, style.spacing);
        }
        let edit = Self::edit_flags(float_scales);
        let visible = Self::visible_flags(float_scales);
        let write_layers = Self::write_layers(float_scales);
        let mut names = vec![0u8; NAMES_LENGTH];
        for (g, group) in self.groups.iter().take(16).enumerate() {
            if float_scales {
                let current = f32_at(&fixed, SCALES + 4 * g);
                put_f32(&mut fixed, SCALES + 4 * g, group.scale as f32, f64::from(current) != group.scale);
            } else {
                let value = group.scale.round().clamp(1.0, f64::from(u16::MAX)) as u16;
                put_u16(&mut fixed, SCALES + 2 * g, value);
            }
            fixed[edit + g] = group.edit;
            fixed[visible + g] = group.visible;
            // 書込みレイヤ（上位 4 ビットはグループの番号）は、レイヤが変わったときだけ書き直す
            let byte = &mut fixed[write_layers + g];
            if relayout || *byte & 0x0f != group.write_layer & 0x0f {
                *byte = ((g as u8) << 4) | (group.write_layer & 0x0f);
            }
            let (name, lossy) = group.name.slot(GROUP_NAME);
            lossy_names += u64::from(lossy);
            let at = LAYER_NAME * 256 + GROUP_NAME * g;
            names[at..at + GROUP_NAME].copy_from_slice(&name);
            for (l, layer) in group.layers.iter().take(16).enumerate() {
                let index = 16 + 16 * g + l;
                fixed[edit + index] = layer.edit;
                fixed[visible + index] = layer.visible;
                let (name, lossy) = layer.name.slot(LAYER_NAME);
                lossy_names += u64::from(lossy);
                let at = LAYER_NAME * (16 * g + l);
                names[at..at + LAYER_NAME].copy_from_slice(&name);
            }
        }
        let mut out = fixed;
        out.reserve(self.lines.len() * LINE + self.arcs.len() * ARC + kept * TEXT + pool.len() + NAMES_LENGTH);
        for line in &self.lines {
            put_xy(&mut out, line.start);
            put_xy(&mut out, line.end);
            put_attributes(&mut out, &line.attributes);
        }
        for arc in &self.arcs {
            put_xy(&mut out, arc.center);
            out.extend_from_slice(&arc.radius.to_le_bytes());
            out.extend_from_slice(&arc.flatness.to_le_bytes());
            for angle in [arc.start, arc.end, arc.tilt] {
                out.extend_from_slice(&angle.to_le_bytes());
            }
            put_attributes(&mut out, &arc.attributes);
        }
        for (text, reference) in self.texts.iter().zip(&references) {
            put_xy(&mut out, text.start);
            put_xy(&mut out, text.end);
            out.extend_from_slice(&reference.to_le_bytes());
            out.push(text.style);
            out.push(text.layer);
            out.extend_from_slice(&text.flags.to_le_bytes());
        }
        out.extend_from_slice(&pool);
        for point in &self.points {
            put_xy(&mut out, point.position);
            out.push(point.layer);
            out.push(point.color);
            out.extend_from_slice(&point.flags.to_le_bytes());
        }
        out.extend_from_slice(&names);
        if lossy_names > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "jwc-name",
                "JWC のレイヤ名（7 バイト）・レイヤグループ名（15 バイト）に入らない名前を切り詰めました",
                lossy_names,
            );
        }
        if lossy_text > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "text-encoding",
                "JWC の文字（Shift_JIS、255 バイトまで）に入らない文字を ? にするか、切り詰めました",
                lossy_text,
            );
        }
        JwwWritten { bytes: out, notices: notices.into_vec() }
    }
}

impl Default for JwcFile {
    fn default() -> Self {
        Self::new()
    }
}

/// f32 を書く。値が同じ（-0 と 0 など、元のビットの違いを保つ）なら書き直さない
fn put_f32(bytes: &mut [u8], at: usize, value: f32, changed: bool) {
    if changed || f32_at(bytes, at) != value {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_xy(out: &mut Vec<u8>, point: [f32; 2]) {
    out.extend_from_slice(&point[0].to_le_bytes());
    out.extend_from_slice(&point[1].to_le_bytes());
}

fn put_attributes(out: &mut Vec<u8>, attributes: &JwcAttributes) {
    out.extend_from_slice(&[attributes.style, attributes.color, attributes.layer, attributes.spare]);
    out.extend_from_slice(&attributes.flags.to_le_bytes());
}

/// JWC を読み、JWW と同じ図面の内容にする（表示・DXF への変換に使う）
pub fn read_jwc(bytes: &[u8]) -> Result<JwwDocument> {
    Ok(JwcFile::read(bytes)?.document())
}

/// 角度（1/65536 度）をラジアンにする
fn angle_radians(value: i32) -> f64 {
    (f64::from(value) / 65536.0).to_radians()
}

impl JwcFile {
    /// 座標の 1 の長さ（mm）と、用紙の幅・高さ
    fn unit(&self) -> (f64, f64, f64) {
        let (width, height) = paper_size(self.paper_size).map_or((420.0, 297.0), |(_, width, height)| (width, height));
        let extent = csv_fields(&self.fixed, 0)
            .get(30)
            .and_then(|field| field.parse::<f64>().ok())
            .filter(|extent| *extent > 0.0);
        (width / extent.unwrap_or(EXTENT), width, height)
    }

    /// JWW と同じ図面の内容。座標は用紙の中心が原点の mm。仮点、線、円弧、文字、点の順
    pub fn document(&self) -> JwwDocument {
        let (unit, width, height) = self.unit();
        let paper = |p: [f32; 2]| (f64::from(p[0]) * unit - width / 2.0, f64::from(p[1]) * unit - height / 2.0);
        let base = |style: u8, color: u8, layer: u8| JwwBase {
            group: 0,
            pen_style: style,
            pen_color: u16::from(color),
            pen_width: 0,
            layer: u16::from(layer & 0x0f),
            layer_group: u16::from(layer >> 4),
            flag: 0,
        };
        let state = |edit: u8, visible: u8, write: bool| match (visible & 1, edit & 1) {
            (0, _) => 0,
            (_, 0) => 1,
            _ if write => 3,
            _ => 2,
        };
        let groups = self
            .groups
            .iter()
            .enumerate()
            .map(|(g, group)| JwwLayerGroup {
                name: if group.name.text.trim().is_empty() {
                    format!("グループ {g:X}")
                } else {
                    group.name.text.clone()
                },
                state: state(group.edit, group.visible, g == usize::from(self.write_group)),
                scale: if group.scale > 0.0 { group.scale } else { 1.0 },
                layers: group
                    .layers
                    .iter()
                    .enumerate()
                    .map(|(l, layer)| JwwLayer {
                        name: if layer.name.text.trim().is_empty() {
                            format!("{g:X}-{l:X}")
                        } else {
                            layer.name.text.clone()
                        },
                        state: state(layer.edit, layer.visible, l == usize::from(group.write_layer)),
                    })
                    .collect(),
            })
            .collect();
        let mut entities = Vec::new();
        for point in &self.temporary_points {
            let (x, y) = paper(point.position);
            let base = base(1, 2, point.layer);
            entities.push(JwwEntity::Point(JwwPoint { base, x, y, temporary: true, code: 0, angle: 0.0, scale: 0.0 }));
        }
        for line in &self.lines {
            let (x1, y1) = paper(line.start);
            let (x2, y2) = paper(line.end);
            let a = &line.attributes;
            entities.push(JwwEntity::Line(JwwLine { base: base(a.style & 0x0f, a.color, a.layer), x1, y1, x2, y2 }));
        }
        for arc in &self.arcs {
            let (cx, cy) = paper(arc.center);
            let a = &arc.attributes;
            let full = arc.start == arc.end;
            // 始角は Jw_cad と同じく −180°〜180°
            let start = angle_radians(arc.start);
            let start = if start > PI { start - TAU } else { start };
            let sweep = if full { TAU } else { (angle_radians(arc.end) - start).rem_euclid(TAU) };
            entities.push(JwwEntity::Arc(JwwArc {
                base: base(a.style & 0x0f, a.color, a.layer),
                cx,
                cy,
                radius: f64::from(arc.radius) * unit,
                start: if full { 0.0 } else { start },
                sweep,
                tilt: angle_radians(arc.tilt),
                flatness: if arc.flatness == 0 { 1.0 } else { f64::from(arc.flatness) / 10000.0 },
                full,
            }));
        }
        for text in &self.texts {
            let (x1, y1) = paper(text.start);
            let (x2, y2) = paper(text.end);
            let style = self.text_styles.get(usize::from(text.style)).copied().unwrap_or_default();
            entities.push(JwwEntity::Text(JwwText {
                base: base(1, style.color.min(255) as u8, text.layer),
                x1,
                y1,
                x2,
                y2,
                text_type: u32::from(text.style),
                size_x: f64::from(style.width) / 10.0,
                size_y: f64::from(style.height) / 10.0,
                spacing: f64::from(style.spacing) / 10.0,
                angle: (y2 - y1).atan2(x2 - x1).to_degrees(),
                font: DEFAULT_FONT.to_string(),
                content: text.content.text.clone(),
            }));
        }
        for point in &self.points {
            let (x, y) = paper(point.position);
            let base = base(1, point.color, point.layer);
            entities.push(JwwEntity::Point(JwwPoint { base, x, y, temporary: false, code: 0, angle: 0.0, scale: 0.0 }));
        }
        JwwDocument {
            version: JWW_VERSION,
            memo: String::new(),
            paper_size: self.paper_size,
            groups,
            pen_colors: None,
            extended_colors: None,
            pen_widths: None,
            width_unit: WidthUnit::default(),
            line_types: BTreeMap::new(),
            entities,
            blocks: Vec::new(),
            truncated: None,
        }
    }
}

/// JWC が持てない内容の数（notices にする）
#[derive(Default)]
struct Lost {
    blocks: u64,
    arcs: u64,
    dimensions: u64,
    solids: u64,
    colors: u64,
    line_types: u64,
    widths: u64,
    fonts: u64,
    text_sizes: u64,
    symbols: u64,
    images: u64,
    depth: u64,
    /// 図形の数の上限で書かなかったもの
    overflow: u64,
}

/// 図形の配置の変換（用紙の mm）: x' = a x + c y + e、y' = b x + d y + f
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn apply(m: &Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn multiply(m: &Matrix, n: &Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// JWW の図面の内容を JWC に書く。ブロックと寸法は分解し、ソリッドは輪郭の線にし、JWC にない線色・線種は近いものに、
/// 文字の大きさは文字種（10 まで）にする。変えた内容は notices で返す
pub fn write_jwc(doc: &JwwDocument) -> JwwWritten {
    let mut notices = NoticeList::new();
    let file = jwc_from_document(doc, &mut notices);
    let written = file.write();
    for notice in &written.notices {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    JwwWritten { bytes: written.bytes, notices: notices.into_vec() }
}

struct Converter<'a> {
    doc: &'a JwwDocument,
    file: JwcFile,
    lost: Lost,
    unit: f64,
    width: f64,
    height: f64,
    /// 文字種（線色・幅・高さ・間隔）→ 番号
    styles: Vec<JwcTextStyle>,
    /// 図形が使う文字種の番号
    used_styles: Vec<bool>,
    /// たどった図形の数（MAX_VISITS まで）
    visits: usize,
}

fn jwc_from_document(doc: &JwwDocument, notices: &mut NoticeList) -> JwcFile {
    let mut file = JwcFile::new();
    let paper = if paper_size(doc.paper_size).is_some() && doc.paper_size <= 4 {
        doc.paper_size
    } else {
        notices.add(NoticeLevel::Warning, "jwc-paper", "JWC の用紙は A0〜A4 なので、A0 にしました");
        0
    };
    file.paper_size = paper;
    let (_, width, height) = paper_size(paper).unwrap_or(("A0", 1189.0, 841.0));
    // レイヤグループとレイヤ（書込みは状態 3 のもの）
    for (g, (group, target)) in doc.groups.iter().zip(file.groups.iter_mut()).enumerate() {
        target.scale = if group.scale > 0.0 && group.scale.is_finite() { group.scale } else { 1.0 };
        let flags = |state: u32| match state {
            0 => (1, 0),
            1 => (0, 1),
            _ => (1, 1),
        };
        (target.edit, target.visible) = flags(group.state);
        if group.name != format!("グループ {g:X}") {
            target.name = JwcString::new(group.name.clone());
        }
        if group.state == 3 {
            file.write_group = g as u8;
        }
        for (l, (layer, slot)) in group.layers.iter().zip(target.layers.iter_mut()).enumerate() {
            (slot.edit, slot.visible) = flags(layer.state);
            if !layer.name.eq_ignore_ascii_case(&format!("{g:X}-{l:X}")) {
                slot.name = JwcString::new(layer.name.clone());
            }
            if layer.state == 3 {
                target.write_layer = l as u8;
            }
        }
    }
    let mut converter = Converter {
        doc,
        file,
        lost: Lost::default(),
        unit: EXTENT / width,
        width,
        height,
        styles: Vec::new(),
        used_styles: vec![false; 11],
        visits: 0,
    };
    converter.styles = converter.file.text_styles.clone();
    for entity in &doc.entities {
        converter.entity(entity, &IDENTITY, 0);
    }
    converter.file.text_styles = converter.styles.clone();
    converter.report(notices);
    converter.file
}

impl Converter<'_> {
    /// 用紙の mm（中心が原点）を JWC の座標にする
    fn point(&self, m: &Matrix, x: f64, y: f64) -> [f32; 2] {
        let (x, y) = apply(m, x, y);
        [((x + self.width / 2.0) * self.unit) as f32, ((y + self.height / 2.0) * self.unit) as f32]
    }

    fn layer(base: &JwwBase) -> u8 {
        ((base.layer_group as u8 & 0x0f) << 4) | (base.layer as u8 & 0x0f)
    }

    /// 線色（1〜9）。SXF 対応色やソリッドの任意色（custom）は、画面色が最も近い線色 1〜9
    fn color(&mut self, color: u16, custom: Option<u32>) -> u8 {
        if (1..=9).contains(&color) {
            return color as u8;
        }
        self.lost.colors += 1;
        let target = self.doc.pen_color(&JwwBase { pen_color: color, ..JwwBase::default() }, custom);
        self.doc.nearest_pen_color(target.as_deref()) as u8
    }

    /// 線種（1〜9）。ランダム線は実線、倍長線種は同じ形の線種、SXF 対応線種は近い Jw_cad の線種
    fn line_type(&mut self, style: u8) -> u8 {
        let mapped = match style {
            1..=9 => return style,
            16..=19 => style - 10,
            31 => 1,
            32 | 46 => 2,
            33 => 3,
            37 => 4,
            34 | 40 => 5,
            38 => 6,
            35 | 36 | 39 | 41..=45 => 7,
            _ => 1,
        };
        self.lost.line_types += 1;
        mapped
    }

    fn attributes(&mut self, base: &JwwBase) -> JwcAttributes {
        if base.pen_width != 0 {
            self.lost.widths += 1;
        }
        JwcAttributes {
            style: self.line_type(base.pen_style),
            color: self.color(base.pen_color, None),
            layer: Self::layer(base),
            spare: 0,
            flags: 0,
        }
    }

    /// ソリッドの輪郭の属性。線種は実線、任意色（線色 10）は画面色が最も近い線色
    fn solid_base(&mut self, base: &JwwBase, custom: Option<u32>) -> JwwBase {
        JwwBase { pen_style: 1, pen_color: u16::from(self.color(base.pen_color, custom)), ..*base }
    }

    /// 種類ごとの図形の数が上限（読み取りと同じ）に達したか。達していれば数えて、書かない
    fn full(&mut self, count: usize) -> bool {
        let full = count >= MAX_RECORDS;
        self.lost.overflow += u64::from(full);
        full
    }

    fn line(&mut self, base: &JwwBase, m: &Matrix, (x1, y1): (f64, f64), (x2, y2): (f64, f64)) {
        if self.full(self.file.lines.len()) {
            return;
        }
        let attributes = self.attributes(base);
        let line = JwcLine { start: self.point(m, x1, y1), end: self.point(m, x2, y2), attributes };
        self.file.lines.push(line);
    }

    fn entity(&mut self, entity: &JwwEntity, m: &Matrix, depth: usize) {
        // ブロックの分解で図形が増えすぎないように、たどる数を抑える
        self.visits += 1;
        if self.visits > MAX_VISITS {
            self.lost.overflow += 1;
            return;
        }
        match entity {
            JwwEntity::Line(e) => self.line(&e.base, m, (e.x1, e.y1), (e.x2, e.y2)),
            JwwEntity::Arc(e) => self.arc(e, m),
            JwwEntity::Point(e) => {
                if e.base.pen_style == 100 {
                    self.lost.symbols += 1;
                }
                let position = self.point(m, e.x, e.y);
                if e.temporary {
                    self.file.temporary_points.push(JwcTemporaryPoint { position, layer: Self::layer(&e.base) });
                } else if !self.full(self.file.points.len()) {
                    let color = self.color(e.base.pen_color, None);
                    self.file.points.push(JwcPoint { position, layer: Self::layer(&e.base), color, flags: 0 });
                }
            }
            JwwEntity::Text(e) => self.text(e, m),
            JwwEntity::Solid(e) => {
                self.lost.solids += 1;
                let base = self.solid_base(&e.base, e.color);
                let p = &e.points;
                let corners = [(p[0], p[1]), (p[2], p[3]), (p[4], p[5]), (p[6], p[7])];
                for i in 0..4 {
                    let (a, b) = (corners[i], corners[(i + 1) % 4]);
                    if a != b {
                        self.line(&base, m, a, b);
                    }
                }
            }
            JwwEntity::CircleSolid(e) => {
                self.lost.solids += 1;
                let arc = JwwArc {
                    base: self.solid_base(&e.base, e.color),
                    cx: e.cx,
                    cy: e.cy,
                    radius: e.radius.abs(),
                    start: e.start,
                    sweep: e.sweep,
                    tilt: e.tilt,
                    flatness: if e.flatness == 0.0 { 1.0 } else { e.flatness },
                    full: e.mode == 100.0 || (e.sweep.abs() - TAU).abs() < 1e-6,
                };
                self.arc(&arc, m);
            }
            JwwEntity::Block(e) => {
                self.lost.blocks += 1;
                if depth >= 16 {
                    self.lost.depth += 1;
                    return;
                }
                let Some(block) = self.doc.blocks.iter().find(|block| block.number == e.definition) else {
                    self.lost.depth += 1;
                    return;
                };
                let (cos, sin) = (e.rotation.cos(), e.rotation.sin());
                let local = [cos * e.scale_x, sin * e.scale_x, -sin * e.scale_y, cos * e.scale_y, e.x, e.y];
                let child = multiply(m, &local);
                for member in &block.entities {
                    if self.visits >= MAX_VISITS {
                        self.lost.overflow += 1;
                        break;
                    }
                    self.entity(member, &child, depth + 1);
                }
            }
            JwwEntity::Dimension(e) => {
                self.lost.dimensions += 1;
                let line = &e.line;
                self.line(&line.base, m, (line.x1, line.y1), (line.x2, line.y2));
                for extension in &e.lines {
                    if extension.x1 != extension.x2 || extension.y1 != extension.y2 {
                        self.line(&extension.base, m, (extension.x1, extension.y1), (extension.x2, extension.y2));
                    }
                }
                for point in e.points.iter().filter(|point| !point.temporary) {
                    self.entity(&JwwEntity::Point(point.clone()), m, depth);
                }
                self.text(&e.text, m);
            }
        }
    }

    /// 円弧・楕円。配置の変換が縦横同じ倍率なら円弧のまま、違えば線分にする
    fn arc(&mut self, e: &JwwArc, m: &Matrix) {
        let scale_x = m[0].hypot(m[1]);
        let scale_y = m[2].hypot(m[3]);
        let mirrored = m[0] * m[3] - m[1] * m[2] < 0.0;
        let uniform = (scale_x - scale_y).abs() <= 1e-9 * scale_x.max(1.0)
            && (m[0] * m[2] + m[1] * m[3]).abs() <= 1e-9 * (scale_x * scale_y).max(1.0);
        if !uniform {
            self.lost.arcs += 1;
            // 縦横の倍率が違う配置の円弧は、表示と同じ点を結ぶ線にする
            let segments = 64usize;
            let sweep = if e.full { TAU } else { e.sweep };
            let start = if e.full { 0.0 } else { e.start };
            let (cos, sin) = (e.tilt.cos(), e.tilt.sin());
            let flatness = if e.flatness > 0.0 { e.flatness } else { 1.0 };
            let at = |t: f64| {
                let (x, y) = (e.radius * t.cos(), e.radius * flatness * t.sin());
                (e.cx + x * cos - y * sin, e.cy + x * sin + y * cos)
            };
            for i in 0..segments {
                let a = at(start + sweep * i as f64 / segments as f64);
                let b = at(start + sweep * (i + 1) as f64 / segments as f64);
                self.line(&e.base, m, a, b);
            }
            return;
        }
        if self.full(self.file.arcs.len()) {
            return;
        }
        let rotation = m[1].atan2(m[0]);
        // 扁平率は 1 以下（長い方の軸を半径にする）
        let (mut radius, mut flatness, mut tilt, mut start, mut sweep) =
            (e.radius, e.flatness, e.tilt, e.start, e.sweep);
        if flatness.is_nan() || flatness <= 0.0 {
            flatness = 1.0;
        }
        if flatness > 1.0 {
            radius *= flatness;
            flatness = 1.0 / flatness;
            tilt += PI / 2.0;
            start -= PI / 2.0;
        }
        if mirrored {
            // 鏡に映した円弧は、媒介変数の向きが逆になる
            tilt = -tilt;
            start = -(start + sweep);
        }
        // 時計回りの円弧は、終わりから始める
        if sweep < 0.0 {
            start += sweep;
            sweep = -sweep;
        }
        let to_fixed = |radians: f64| ((radians.rem_euclid(TAU)).to_degrees() * 65536.0).round() as i32 % (360 * 65536);
        let full = e.full || sweep >= TAU - 1e-9;
        let start_value = if full { 0 } else { to_fixed(start) };
        let end_value = if full { 0 } else { to_fixed(start + sweep) };
        let attributes = self.attributes(&e.base);
        let arc = JwcArc {
            center: self.point(m, e.cx, e.cy),
            radius: (radius * scale_x * self.unit) as f32,
            flatness: (flatness * 10000.0).round().clamp(1.0, 10000.0) as u16,
            start: start_value,
            end: if !full && end_value == start_value { (start_value + 1) % (360 * 65536) } else { end_value },
            tilt: to_fixed(tilt + rotation),
            attributes,
        };
        self.file.arcs.push(arc);
    }

    fn text(&mut self, e: &JwwText, m: &Matrix) {
        if e.content.is_empty() {
            return;
        }
        if e.content.starts_with("^@") {
            self.lost.images += 1;
            return;
        }
        if self.full(self.file.texts.len()) {
            return;
        }
        if e.font.starts_with('@') || (!e.font.is_empty() && e.font != DEFAULT_FONT) {
            self.lost.fonts += 1;
        }
        let scale = m[0].hypot(m[1]);
        let tenths = |mm: f64| (mm * scale * 10.0).round().clamp(0.0, f64::from(u16::MAX)) as u16;
        let wanted = JwcTextStyle {
            color: u16::from(self.color(e.base.pen_color, None)),
            // 負の大きさ（Jw_cad 10.03 以降の反転した文字）は、JWC では反転しない文字にする
            width: tenths(e.size_x.abs()),
            height: tenths(if e.size_y != 0.0 { e.size_y.abs() } else { 2.5 }),
            spacing: tenths(e.spacing),
        };
        let style = self.style(wanted);
        // 始点から終点（文字の幅と向き）。終点が始点と同じなら、文字の幅で求める
        let (mut x2, mut y2) = (e.x2, e.y2);
        if x2 == e.x1 && y2 == e.y1 {
            let count = e.content.chars().count();
            let units: f64 =
                e.content.chars().map(|c| if zumen_core::text::is_half_width(c) { 0.5 } else { 1.0 }).sum();
            let length = units * e.size_x + count.saturating_sub(1) as f64 * e.spacing;
            let angle = e.angle.to_radians();
            (x2, y2) = (e.x1 + length.max(0.1) * angle.cos(), e.y1 + length.max(0.1) * angle.sin());
        }
        let text = JwcText {
            start: self.point(m, e.x1, e.y1),
            end: self.point(m, x2, y2),
            style,
            layer: Self::layer(&e.base),
            flags: 0,
            content: JwcString::new(e.content.clone()),
        };
        self.file.texts.push(text);
    }

    /// 文字種の番号。同じものがあればそれ、なければ文字が使っていない番号に入れ、空きがなければ高さの最も近いもの
    fn style(&mut self, wanted: JwcTextStyle) -> u8 {
        if let Some(index) = (1..11).find(|&i| self.styles[i] == wanted) {
            self.used_styles[index] = true;
            return index as u8;
        }
        if let Some(index) = (1..11).find(|&i| !self.used_styles[i]) {
            self.styles[index] = wanted;
            self.used_styles[index] = true;
            return index as u8;
        }
        self.lost.text_sizes += 1;
        let distance = |style: &JwcTextStyle| {
            (i64::from(style.height) - i64::from(wanted.height)).abs() * 1000
                + (i64::from(style.width) - i64::from(wanted.width)).abs()
        };
        (1..11).min_by_key(|&i| distance(&self.styles[i])).unwrap_or(1) as u8
    }

    fn report(&self, notices: &mut NoticeList) {
        let lost = &self.lost;
        let reports = [
            (lost.blocks, "jwc-block", "JWC はブロックを持たないため、ブロックの配置を図形に分解しました"),
            (lost.arcs, "jwc-arc", "縦横の倍率が違う配置やせん断された配置の円弧を、線分で近似しました"),
            (lost.depth, "missing-block", "定義のないブロック・入れ子が深すぎるブロックの配置を省きました"),
            (lost.dimensions, "jwc-dimension", "JWC は寸法図形を持たないため、線・点・文字に分解しました"),
            (lost.solids, "jwc-solid", "JWC はソリッドを持たないため、輪郭の線・円弧にしました"),
            (lost.colors, "jwc-color", "JWC の線色は 1〜9 なので、SXF 対応色などは画面色が最も近い線色にしました"),
            (lost.line_types, "jwc-line-type", "JWC の線種は 1〜9 なので、ほかの線種は近い線種にしました"),
            (lost.widths, "jwc-width", "JWC は図形ごとの線幅を持たないため、線色の線幅になります"),
            (
                lost.fonts,
                "jwc-font",
                "JWC は文字のフォント（縦書きを含む）を持たないため、Jw_cad の既定のフォントになります",
            ),
            (
                lost.text_sizes,
                "jwc-text-size",
                "JWC の文字種（10 まで）に入らない文字の大きさは、最も近い文字種にしました",
            ),
            (lost.symbols, "point-symbol", "JWC は点の記号を持たないため、ふつうの点にしました"),
            (lost.images, "embedded-image", "JWC は画像を持たないため、画像の配置の文字を書いていません"),
            (
                lost.overflow,
                "jwc-limit",
                "図形が多すぎる（線・円弧・文字・点がそれぞれ 100 万、ブロックの分解でたどる図形が 400 万を超えた）ため、残りを書いていません",
            ),
        ];
        for (count, code, message) in reports {
            if count > 0 {
                notices.add_count(NoticeLevel::Warning, code, message, count);
            }
        }
        if !self.doc.memo.trim().is_empty() {
            notices.add(NoticeLevel::Info, "memo", "JWC は図面のメモを持たないため、書いていません");
        }
    }
}

#[cfg(test)]
mod tests;
