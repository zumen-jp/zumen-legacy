//! Jw_cad の図形ファイル JWS（図形登録した図形）を読み書きする（docs/formats/jww.md「JWS」）。
//!
//! JWS の公式の形式説明は見つからない。ヘッダの並びは、Jw_cad の掲示板に公開された説明（版 2.30）と、ezjww（MIT、
//! https://github.com/monozukuri-ai/ezjww）が Jw_cad 10.02.1 で登録した図形と元の図面を比べた結果に拠る。
//!
//! - 452 バイトのヘッダ: 識別子 `JwsData.` と 192 バイトの "."、版（200 バイト目）、配置の基準点、16 のレイヤ
//!   グループの縮尺、予備の整数 6 つと実数 6 つ（Jw_cad 10 は 1〜6 を書く）、図形の範囲（最小 x・y、最大 x・y）。
//! - その後ろは、JWW と同じ図形の一覧とブロック定義の一覧（その版の並び）。同梱画像の欄はない。
//! - 座標は JWW と同じ用紙の mm。線色・線種・レイヤの名前などの設定は持たない（配置する図面の設定で描く）。

use crate::archive::{ReadTable, Reader, Result, Writer, unicode};
use crate::create::file_from_document;
use crate::file::{JwwBlockRecord, JwwRecord, JwwWritten, read_body, report_replaced, write_body};
use crate::reader::{JwwBlockDefinition, JwwDocument, JwwEntity, read_blocks, read_entity_list};
use crate::scene::jww_to_scene;
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use zumen_core::geometry::{IDENTITY, apply, apply_vector, multiply};
use zumen_core::{Matrix, NoticeLevel, NoticeList, Property, ReadError, Scene, js};

/// JWS の識別子
pub const JWS_SIGNATURE: &[u8; 8] = b"JwsData.";

/// ヘッダの長さ（この後ろから図形の一覧）
const HEADER_LENGTH: usize = 452;
/// 版の欄
const VERSION_AT: usize = 200;
/// 予備の整数 6 つと実数 6 つ
const SPARE_LENGTH: usize = 72;
/// 用紙がないことを表す用紙の番号（表示用の図面に用紙を描かない）
const NO_SHEET: u32 = u32::MAX;
/// 図形の範囲を求めるときの、ブロックの入れ子の深さと、たどる図形の数の上限
const MAX_DEPTH: usize = 16;
const MAX_VISITS: usize = 1_000_000;

/// JWS ファイルか（識別子）
pub fn is_jws(bytes: &[u8]) -> bool {
    bytes.starts_with(JWS_SIGNATURE)
}

/// JWS のヘッダ
#[derive(Debug, Clone, PartialEq)]
pub struct JwsHeader {
    /// 版（351、420、600、700 など）。図形の並びもこの版
    pub version: u32,
    /// 配置の基準点（用紙の mm）
    pub origin: [f64; 2],
    /// 16 のレイヤグループの縮尺の分母（図形の layer_group で引く）
    pub group_scales: [f64; 16],
    /// Jw_cad が記録した図形の範囲（最小 x、最小 y、最大 x、最大 y。用紙の mm）
    pub bounds: [f64; 4],
    /// 識別子の後ろの 192 バイトと、予備の整数・実数。意味は分からないので、元のバイト列のまま持つ
    prefix: Vec<u8>,
    spare: Vec<u8>,
}

impl JwsHeader {
    /// 新しいヘッダ。基準点は (0, 0)、縮尺は 1/1、予備は Jw_cad 10 と同じ（"." と 1〜6）
    pub fn new(version: u32) -> Self {
        let mut spare = Vec::with_capacity(SPARE_LENGTH);
        for i in 1..=6u32 {
            spare.extend_from_slice(&i.to_le_bytes());
        }
        for i in 1..=6u32 {
            spare.extend_from_slice(&f64::from(i).to_le_bytes());
        }
        Self {
            version,
            origin: [0.0; 2],
            group_scales: [1.0; 16],
            bounds: [0.0; 4],
            prefix: vec![b'.'; VERSION_AT - JWS_SIGNATURE.len()],
            spare,
        }
    }

    fn read(bytes: &[u8]) -> Result<Self> {
        if !is_jws(bytes) {
            return Err(ReadError::new("JWS ファイルではありません"));
        }
        if bytes.len() < HEADER_LENGTH {
            return Err(ReadError::new("JWS のヘッダが途中で終わっています"));
        }
        let mut reader = Reader::new(bytes, VERSION_AT);
        let version = reader.u32()?;
        let origin = [reader.f64()?, reader.f64()?];
        let mut group_scales = [0.0; 16];
        for scale in &mut group_scales {
            *scale = reader.f64()?;
        }
        let spare = reader.take(SPARE_LENGTH)?.to_vec();
        let mut bounds = [0.0; 4];
        for value in &mut bounds {
            *value = reader.f64()?;
        }
        let prefix = bytes[JWS_SIGNATURE.len()..VERSION_AT].to_vec();
        Ok(Self { version, origin, group_scales, bounds, prefix, spare })
    }

    fn write(&self, out: &mut Writer) {
        out.raw(JWS_SIGNATURE);
        out.raw(&self.prefix);
        out.u32(self.version);
        out.f64s(&self.origin);
        out.f64s(&self.group_scales);
        out.raw(&self.spare);
        out.f64s(&self.bounds);
    }
}

/// JWS ファイルのすべて。図形とブロック定義は JWW と同じレコードで、読んだまま書けば同じバイト列になる
#[derive(Debug, Clone, PartialEq)]
pub struct JwsFile {
    pub header: JwsHeader,
    /// 図形の一覧
    pub entities: Vec<JwwRecord>,
    /// ブロック定義の一覧
    pub blocks: Vec<JwwBlockRecord>,
    /// ブロック定義の一覧より後ろ（Jw_cad の JWS にはない）。そのまま書く
    pub trailer: Vec<u8>,
    /// 読んだときの版と、文字列を UTF-16LE で書いていたか（文字列がなければ None）
    source: Option<(u32, Option<bool>)>,
}

impl JwsFile {
    /// 新しい図形（図形とブロック定義はなし）
    pub fn new(header: JwsHeader) -> Self {
        Self { header, entities: Vec::new(), blocks: Vec::new(), trailer: Vec::new(), source: None }
    }

    /// JWS を読む。図形の一覧とブロック定義の一覧が終わりまで読めなければエラー（表示用の `read_jws` は途中まで読む）
    pub fn read(bytes: &[u8]) -> Result<Self> {
        let header = JwsHeader::read(bytes)?;
        let body = read_body(bytes, HEADER_LENGTH, header.version)?;
        let source = Some((header.version, body.unicode));
        Ok(Self { header, entities: body.entities, blocks: body.blocks, trailer: body.trailer, source })
    }

    /// JWS に書く。読んだファイルを同じ版で書くなら、変わっていない図形は元のバイト列のまま、足した文字列も元の符号で
    /// 書く。ほかは JWW と同じく、版 700 以降なら UTF-16LE、それより前は Shift_JIS
    pub fn write(&self) -> JwwWritten {
        let mut notices = NoticeList::new();
        let version = self.header.version;
        let source = self.source.filter(|(read, _)| *read == version);
        let strings = source.and_then(|(_, strings)| strings).unwrap_or_else(|| unicode(version));
        let mut out = Writer::with_unicode(strings);
        self.header.write(&mut out);
        write_body(&mut out, version, source.is_some(), &self.entities, &self.blocks, &mut notices);
        out.raw(&self.trailer);
        report_replaced(&out, &mut notices);
        JwwWritten { bytes: out.bytes, notices: notices.into_vec() }
    }
}

/// JWS の図形（表示と変換に使う）。図形・ブロック定義・版・レイヤグループの縮尺は、JWW と同じ図面の内容が持つ
#[derive(Debug, Clone, PartialEq)]
pub struct JwsDocument {
    /// 配置の基準点（用紙の mm）
    pub origin: [f64; 2],
    /// Jw_cad が記録した図形の範囲（最小 x、最小 y、最大 x、最大 y）。`write_jws` は図形から求め直す
    pub bounds: [f64; 4],
    /// 図形の内容。JWS は用紙・線色・線種・レイヤの名前を持たないので、それらは新しい JWW の図面の既定（用紙は
    /// A3）。線色は持たない（None）ので、表示は Jw_cad の既定の色になる
    pub content: JwwDocument,
}

/// JWS を読み、表示・変換に使う図形にする。図形の一覧が途中で読めなくても、読めたところまでを返す（`truncated`）
pub fn read_jws(bytes: &[u8]) -> Result<JwsDocument> {
    let header = JwsHeader::read(bytes)?;
    let mut content = JwwDocument::new();
    content.version = header.version;
    content.pen_colors = None;
    content.extended_colors = None;
    content.pen_widths = None;
    content.line_types = BTreeMap::new();
    for (group, scale) in content.groups.iter_mut().zip(header.group_scales) {
        group.scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    }
    let mut body = Reader::new(bytes, HEADER_LENGTH);
    let mut table = ReadTable::new();
    if let Err(error) = read_entity_list(&mut body, header.version, &mut table, &mut content.entities) {
        content.truncated = Some(error.message().to_string());
    } else if let Err(error) = read_blocks(&mut body, header.version, &mut table, &mut content.blocks) {
        content.truncated = Some(format!("ブロック定義: {}", error.message()));
    }
    Ok(JwsDocument { origin: header.origin, bounds: header.bounds, content })
}

/// JWS の図形の表示用の図面。用紙はなく、座標は用紙の mm のまま。形式は jws、配置の基準点はメタデータ
pub fn jws_to_scene(doc: &JwsDocument) -> Scene {
    let mut content = doc.content.clone();
    content.paper_size = NO_SHEET;
    let mut scene = jww_to_scene(&content);
    scene.format = "jws".to_string();
    scene.version = format!("JWS {}", content.version);
    if let Some(format) = scene.metadata.iter_mut().find(|property| property.key == "format") {
        *format = Property::new("format", "形式", format!("JWS（図形、版 {}）", content.version));
    }
    let [x, y] = doc.origin;
    let origin = Property::new("origin", "基準点", format!("{}, {}", js::number_to_string(x), js::number_to_string(y)));
    let at = scene.metadata.len().min(1);
    scene.metadata.insert(at, origin.raw(vec![x, y]));
    scene
}

/// JWS の図形を書く。図形の範囲は図形から求め直す。JWS が持たない線色・線種・レイヤの名前と状態・メモは書かない
/// （notice `jws-settings`）。版は `content.version`（JWW と同じく 200〜700、ほかは 700）
pub fn write_jws(doc: &JwsDocument) -> JwwWritten {
    let mut notices = NoticeList::new();
    let mut converted = NoticeList::new();
    let file = file_from_document(&doc.content, &mut converted);
    // JWW のヘッダの線色・線幅・線種についての notices は、それを書かない JWS には当たらない
    for notice in converted.into_vec() {
        match notice.code.as_str() {
            "pen-width" | "invalid-color" | "line-type" => {}
            "version" => notices.add(
                notice.level,
                "version",
                format!("版 {} の JWS は書けないため、版 {} で書きました", doc.content.version, file.version),
            ),
            _ => notices.add_count(notice.level, &notice.code, notice.message, notice.count),
        }
    }
    if has_settings(&doc.content) {
        notices.add(
            NoticeLevel::Info,
            "jws-settings",
            "JWS は線色・線幅・線種・レイヤの名前と状態・メモを持たないため、図形だけを書きました",
        );
    }
    let mut header = JwsHeader::new(file.version);
    header.origin = doc.origin;
    for (scale, group) in header.group_scales.iter_mut().zip(&file.header.groups) {
        *scale = group.scale;
    }
    header.bounds = bounds(&doc.content).unwrap_or([doc.origin[0], doc.origin[1], doc.origin[0], doc.origin[1]]);
    let mut jws = JwsFile::new(header);
    jws.entities = file.entities;
    jws.blocks = file.blocks;
    let written = jws.write();
    for notice in &written.notices {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    JwwWritten { bytes: written.bytes, notices: notices.into_vec() }
}

/// 図形の範囲（用紙の mm）。図形がなければ None。Jw_cad が図形登録で記録する範囲と同じく、線・曲線は端まで、文字は
/// 始点と終点（画像の配置も）、ブロックは配置した中の図形で求める
fn bounds(content: &JwwDocument) -> Option<[f64; 4]> {
    let blocks: BTreeMap<u32, &JwwBlockDefinition> = content.blocks.iter().map(|block| (block.number, block)).collect();
    let mut extent = Extent { blocks, min: [f64::INFINITY; 2], max: [f64::NEG_INFINITY; 2], visits: 0 };
    for entity in &content.entities {
        extent.entity(entity, &IDENTITY, 0);
    }
    let Extent { min, max, .. } = extent;
    (min[0] <= max[0] && min[1] <= max[1]).then_some([min[0], min[1], max[0], max[1]])
}

/// 図形の範囲を広げていく
struct Extent<'a> {
    blocks: BTreeMap<u32, &'a JwwBlockDefinition>,
    min: [f64; 2],
    max: [f64; 2],
    visits: usize,
}

impl Extent<'_> {
    fn add(&mut self, m: &Matrix, x: f64, y: f64) {
        let (x, y) = apply(m, x, y);
        if x.is_finite() && y.is_finite() {
            self.min = [self.min[0].min(x), self.min[1].min(y)];
            self.max = [self.max[0].max(x), self.max[1].max(y)];
        }
    }

    fn entity(&mut self, entity: &JwwEntity, m: &Matrix, depth: usize) {
        self.visits += 1;
        if self.visits > MAX_VISITS {
            return;
        }
        match entity {
            JwwEntity::Line(e) => {
                self.add(m, e.x1, e.y1);
                self.add(m, e.x2, e.y2);
            }
            JwwEntity::Arc(e) => {
                self.arc(m, (e.cx, e.cy), e.radius, e.flatness, e.tilt, (e.start, e.sweep), e.full);
            }
            JwwEntity::Point(e) => self.add(m, e.x, e.y),
            JwwEntity::Text(e) => {
                self.add(m, e.x1, e.y1);
                self.add(m, e.x2, e.y2);
            }
            JwwEntity::Solid(e) => {
                for [x, y] in e.points.as_chunks::<2>().0 {
                    self.add(m, *x, *y);
                }
            }
            JwwEntity::CircleSolid(e) => {
                let full = e.mode == 100.0;
                self.arc(m, (e.cx, e.cy), e.radius.abs(), e.flatness, e.tilt, (e.start, e.sweep), full);
                // 扇形は中心まで
                if e.mode == 0.0 {
                    self.add(m, e.cx, e.cy);
                }
            }
            JwwEntity::Block(e) => {
                let Some(block) = self.blocks.get(&e.definition).copied() else {
                    return;
                };
                if depth >= MAX_DEPTH {
                    return;
                }
                let (cos, sin) = (e.rotation.cos(), e.rotation.sin());
                let local = [cos * e.scale_x, sin * e.scale_x, -sin * e.scale_y, cos * e.scale_y, e.x, e.y];
                let child = multiply(m, &local);
                for member in &block.entities {
                    if self.visits >= MAX_VISITS {
                        return;
                    }
                    self.entity(member, &child, depth + 1);
                }
            }
            JwwEntity::Dimension(e) => {
                for line in std::iter::once(&e.line).chain(&e.lines) {
                    self.add(m, line.x1, line.y1);
                    self.add(m, line.x2, line.y2);
                }
                self.add(m, e.text.x1, e.text.y1);
                self.add(m, e.text.x2, e.text.y2);
                for point in &e.points {
                    self.add(m, point.x, point.y);
                }
            }
        }
    }

    /// 楕円弧の端。媒介変数の角 start から sweep まで（全周なら 0〜2π）で、配置の変換を掛けた楕円の x・y が最大・
    /// 最小になる角と、両端を見る
    #[allow(clippy::too_many_arguments)]
    fn arc(
        &mut self,
        m: &Matrix,
        (cx, cy): (f64, f64),
        radius: f64,
        flatness: f64,
        tilt: f64,
        (start, sweep): (f64, f64),
        full: bool,
    ) {
        let flatness = if flatness > 0.0 && flatness.is_finite() { flatness } else { 1.0 };
        let (cos, sin) = (tilt.cos(), tilt.sin());
        // 用紙の座標で 中心 + U cos t + V sin t
        let (ux, uy) = apply_vector(m, radius * cos, radius * sin);
        let (vx, vy) = apply_vector(m, -radius * flatness * sin, radius * flatness * cos);
        let (ox, oy) = apply(m, cx, cy);
        let (begin, span) = match (full, sweep >= 0.0) {
            (true, _) => (0.0, TAU),
            (false, true) => (start, sweep.min(TAU)),
            (false, false) => (start + sweep, (-sweep).min(TAU)),
        };
        let mut angles = vec![begin, begin + span];
        for base in [vx.atan2(ux), vy.atan2(uy)] {
            for t in [base, base + PI] {
                let offset = (t - begin).rem_euclid(TAU);
                if offset <= span {
                    angles.push(begin + offset);
                }
            }
        }
        for t in angles {
            self.add(&IDENTITY, ox + ux * t.cos() + vx * t.sin(), oy + uy * t.cos() + vy * t.sin());
        }
    }
}

/// JWS が持たない設定（既定と違うレイヤの名前、非表示・表示のみの状態、線色・線幅・線種、メモ）があるか
fn has_settings(content: &JwwDocument) -> bool {
    let defaults = JwwDocument::new();
    let layers = content.groups.iter().zip(&defaults.groups).any(|(group, default)| {
        group.name != default.name
            || group.state < 2
            || group
                .layers
                .iter()
                .zip(&default.layers)
                .any(|(layer, default)| layer.name != default.name || layer.state < 2)
    });
    let differs = |value: Option<bool>| value.unwrap_or(false);
    layers
        || !content.memo.trim().is_empty()
        || differs(content.pen_colors.as_ref().map(|colors| Some(colors) != defaults.pen_colors.as_ref()))
        || differs(content.pen_widths.as_ref().map(|widths| Some(widths) != defaults.pen_widths.as_ref()))
        || (!content.line_types.is_empty() && content.line_types != defaults.line_types)
}

#[cfg(test)]
mod tests;
