//! R13 より前の DWG の読み取りのテスト。図面はテストの中で、読み取り（records.rs）と同じ並びで書く。実物の図面
//! （ZUMEN_R12_SAMPLES）はリポジトリに入れず、手元で記録の並びと CRC を確かめる

use super::records::{self, Step, kind};
use super::*;
use crate::crc::{CRC8_SEED, crc8};

/// バイト単位の値を書く
#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, value: u8) -> &mut Self {
        self.0.push(value);
        self
    }
    fn i16(&mut self, value: i16) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn u16(&mut self, value: u16) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn i32(&mut self, value: i32) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn u32(&mut self, value: u32) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn f64(&mut self, value: f64) -> &mut Self {
        self.0.extend_from_slice(&value.to_le_bytes());
        self
    }
    fn p2(&mut self, [x, y]: [f64; 2]) -> &mut Self {
        self.f64(x).f64(y)
    }
    fn p3(&mut self, [x, y, z]: [f64; 3]) -> &mut Self {
        self.f64(x).f64(y).f64(z)
    }
    fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.0.extend_from_slice(bytes);
        self
    }
    /// 長さ（RS）と文字列
    fn text(&mut self, text: &[u8]) -> &mut Self {
        self.i16(text.len() as i16).raw(text)
    }
    /// 決まった長さの文字列（残りは NUL）
    fn fixed(&mut self, text: &[u8], length: usize) -> &mut Self {
        let mut bytes = text.to_vec();
        bytes.resize(length, 0);
        self.raw(&bytes)
    }
}

/// 図形の記録の値
#[derive(Default, Clone)]
struct E {
    kind: i8,
    layer: i16,
    color: Option<u8>,
    linetype: Option<i16>,
    elevation: Option<f64>,
    thickness: Option<f64>,
    handle: Option<u64>,
    paper: bool,
    eed: Vec<u8>,
    options: u16,
    data: Vec<u8>,
    follows: bool,
    /// LINE・POINT・3DFACE の Z を省く（R10 から、モードの 0x04）
    flat: bool,
}

fn e(kind: i8, options: u16, data: &W) -> E {
    E { kind, options, data: data.0.clone(), ..E::default() }
}

/// 記録を版の形で書く（records::record の読み方と同じ並び）
fn encode(version: Old, e: &E) -> Vec<u8> {
    let r10 = version >= Old::Ac1006;
    let r11 = version >= Old::Ac1009;
    let points_3d = r10 && matches!(e.kind, kind::LINE | kind::POINT | kind::LINE3D | kind::FACE3D);
    let mut mode = 0u8;
    if e.color.is_some() {
        mode |= 0x01;
    }
    if e.linetype.is_some() {
        mode |= 0x02;
    }
    if e.elevation.is_some() || (points_3d && e.flat) {
        mode |= 0x04;
    }
    if e.thickness.is_some() {
        mode |= 0x08;
    }
    if r10 && e.handle.is_some() {
        mode |= 0x20;
    }
    if r11 && (e.paper || !e.eed.is_empty()) {
        mode |= 0x40;
    }
    if e.follows {
        mode |= 0x80;
    }
    let mut w = W::default();
    w.i16(e.layer).u16(e.options);
    if mode & 0x40 != 0 {
        w.u8(if e.eed.is_empty() { 0 } else { 0x02 } | if e.paper { 0x04 } else { 0 });
        if !e.eed.is_empty() {
            w.u16(e.eed.len() as u16).raw(&e.eed);
        }
    }
    if let Some(color) = e.color {
        w.u8(color);
    }
    if let Some(linetype) = e.linetype {
        if r11 {
            w.i16(linetype)
        } else {
            w.u8(linetype as u8)
        };
    }
    let elevation = e.elevation.filter(|_| !points_3d);
    if r11 {
        e.thickness.map(|value| w.f64(value));
        elevation.map(|value| w.f64(value));
    } else {
        elevation.map(|value| w.f64(value));
        e.thickness.map(|value| w.f64(value));
    }
    if let Some(handle) = e.handle.filter(|_| r10) {
        let bytes: Vec<u8> = handle.to_be_bytes().into_iter().skip_while(|byte| *byte == 0).collect();
        w.u8(bytes.len() as u8).raw(&bytes);
    }
    if e.paper {
        w.i16(1);
    }
    w.raw(&e.data);
    frame(version, e.kind, mode, &w.0)
}

/// 型・モード・大きさと値、（R11）CRC
fn frame(version: Old, kind: i8, mode: u8, values: &[u8]) -> Vec<u8> {
    let crc = if version >= Old::Ac1009 { 2 } else { 0 };
    let mut out = vec![kind as u8, mode];
    out.extend_from_slice(&((4 + values.len() + crc) as i16).to_le_bytes());
    out.extend_from_slice(values);
    if crc > 0 {
        let value = crc8(CRC8_SEED, &out);
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// JUMP（行き先の上位のバイトの 0x80 は追加の図形の節）
fn jump(version: Old, raw: u32) -> Vec<u8> {
    frame(version, kind::JUMP, 0, &raw.to_le_bytes())
}

/// 表の記録の頭（フラグ、名前、R11 は使われている数）。R11 は終わりに CRC を足す
fn entry(version: Old, flags: u8, name: &[u8]) -> W {
    let mut w = W::default();
    w.u8(flags).fixed(name, 32);
    if version >= Old::Ac1009 {
        w.i16(0);
    }
    w
}

fn finish_entry(version: Old, mut w: W) -> Vec<u8> {
    if version >= Old::Ac1009 {
        let value = crc8(CRC8_SEED, &w.0);
        w.u16(value);
    }
    w.0
}

/// テストの図面の表と図形
#[derive(Default)]
struct Drawing {
    /// (名前, 色, 線種の番号)
    layers: Vec<(Vec<u8>, i16, i16)>,
    /// (名前, 説明, 線と空白)
    linetypes: Vec<(Vec<u8>, Vec<u8>, Vec<f64>)>,
    /// (名前, フォント)
    styles: Vec<(Vec<u8>, Vec<u8>)>,
    /// (名前, DIMASZ)
    dimstyles: Vec<(Vec<u8>, f64)>,
    /// (名前, フラグ, BLOCK から ENDBLK までの記録)
    blocks: Vec<(Vec<u8>, u8, Vec<Vec<u8>>)>,
    entities: Vec<Vec<u8>>,
    extras: Vec<Vec<u8>>,
    codepage: u16,
}

/// AC1.50 からの図形のファイル。ヘッダ変数の後（0x700）に図形・表・ブロックの図形・追加の図形を置く
fn file(version: Old, d: &Drawing) -> Vec<u8> {
    let mut out = vec![0u8; 0x700];
    out[..version.name().len()].copy_from_slice(version.name().as_bytes());
    let put = |out: &mut Vec<u8>, at: usize, bytes: &[u8]| out[at..at + bytes.len()].copy_from_slice(bytes);
    // ヘッダ変数: 範囲、尺度、文字の高さ、寸法の大きさ
    let mut vars = W::default();
    vars.p3([0.0; 3]).p3([100.0, 50.0, 0.0]).p2([0.0, 0.0]).p2([420.0, 297.0]);
    put(&mut out, 0x78, &vars.0);
    put(&mut out, 0x132, &1.0f64.to_le_bytes());
    put(&mut out, 0x13a, &2.5f64.to_le_bytes());
    put(&mut out, 0x1a3, &1.0f64.to_le_bytes());
    put(&mut out, 0x1ab, &2.5f64.to_le_bytes());
    put(&mut out, 0x1db, &2.5f64.to_le_bytes());
    if version >= Old::Ac1009 {
        put(&mut out, 0x156, &1u16.to_le_bytes());
        put(&mut out, 0x3f9, &d.codepage.to_le_bytes());
        put(&mut out, 0x631, &1u16.to_le_bytes());
        let mut paper = W::default();
        paper.p3([0.0; 3]).p3([0.0; 3]).p2([0.0, 0.0]).p2([420.0, 297.0]);
        put(&mut out, 0x637, &paper.0);
    }
    let entities_start = out.len();
    for record in &d.entities {
        out.extend_from_slice(record);
    }
    let entities_end = out.len();
    // 表（記録の大きさ, 記録）
    let r11 = version >= Old::Ac1009;
    let table = |out: &mut Vec<u8>, at: usize, records: Vec<Vec<u8>>| {
        let begin = out.len();
        let size = records.first().map_or(0, Vec::len);
        for record in &records {
            out.extend_from_slice(record);
        }
        let mut w = W::default();
        w.u16(size as u16).u16(records.len() as u16).u16(0).u32(begin as u32);
        put(out, at, &w.0);
    };
    let layers = d
        .layers
        .iter()
        .map(|(name, color, linetype)| {
            let mut w = entry(version, 0, name);
            w.i16(*color).i16(*linetype);
            finish_entry(version, w)
        })
        .collect();
    table(&mut out, 0x36, layers);
    let linetypes = d
        .linetypes
        .iter()
        .map(|(name, description, dashes)| {
            let mut w = entry(version, 0, name);
            w.fixed(description, 48).u8(b'A').u8(dashes.len() as u8).f64(dashes.iter().map(|dash| dash.abs()).sum());
            for index in 0..12 {
                w.f64(dashes.get(index).copied().unwrap_or(0.0));
            }
            finish_entry(version, w)
        })
        .collect();
    table(&mut out, 0x4a, linetypes);
    let styles = d
        .styles
        .iter()
        .map(|(name, font)| {
            let mut w = entry(version, 0, name);
            w.f64(0.0).f64(1.0).f64(0.0).u8(0).f64(2.5).fixed(font, 64);
            if version >= Old::Ac1001 {
                w.fixed(b"", 64);
            }
            finish_entry(version, w)
        })
        .collect();
    table(&mut out, 0x40, styles);
    if r11 {
        let dimstyles = d
            .dimstyles
            .iter()
            .map(|(name, dimasz)| {
                let mut w = entry(version, 0, name);
                w.f64(1.0).f64(*dimasz);
                for _ in 0..7 {
                    w.f64(0.0);
                }
                w.f64(2.5);
                finish_entry(version, w)
            })
            .collect();
        table(&mut out, 0x522, dimstyles);
    }
    // ブロックの図形の節と、ブロックの表（BLOCK の位置は節の中の位置に 0x40 の印）
    let mut section = Vec::new();
    let mut addresses = Vec::new();
    for (_, _, list) in &d.blocks {
        addresses.push(0x4000_0000 | section.len() as u32);
        for record in list {
            section.extend_from_slice(record);
        }
    }
    let blocks = d
        .blocks
        .iter()
        .zip(&addresses)
        .map(|((name, flags, _), address)| {
            let mut w = entry(version, *flags, name);
            w.u32(*address);
            if r11 {
                w.i16(0).u8(0).u8(0);
            }
            finish_entry(version, w)
        })
        .collect();
    table(&mut out, 0x2c, blocks);
    let blocks_start = out.len();
    out.extend_from_slice(&section);
    let extras_start = out.len();
    for record in &d.extras {
        out.extend_from_slice(record);
    }
    let extras_size = out.len() - extras_start;
    let mut head = W::default();
    head.i32(entities_start as i32)
        .i32(entities_end as i32)
        .i32(blocks_start as i32)
        .u32(0x4000_0000 | section.len() as u32)
        .i32(extras_start as i32)
        .u32(0x8000_0000 | extras_size as u32);
    put(&mut out, 0x14, &head.0);
    out
}

fn sjis(text: &str) -> Vec<u8> {
    encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
}

/// テストの図面（AC1.50 から）。レイヤ 0・平面図・非表示（非表示は切ってあり DASHED）、ブロック BOX と寸法のブロック *D、
/// モデル空間の線（R10 からハンドル 40）・文字（Shift_JIS）・配置・円弧・閉じたポリライン・非表示のレイヤの線・標高と厚さの
/// ある円・寸法・属性のある配置、消した図形、追加の図形の節の点（JUMP で行き来する）。R11 は紙空間のビューポートと線を足す
fn drawing(version: Old) -> Vec<u8> {
    let r10 = version >= Old::Ac1006;
    let r11 = version >= Old::Ac1009;
    let enc = |e: E| encode(version, &e);
    let mut line = W::default();
    line.p2([0.0, 0.0]);
    let point = |w: &mut W, p: [f64; 3]| {
        w.p2([p[0], p[1]]);
    };
    // 線（R10 から LINE は Z を省く印 flat、それより前は平面の点）
    let segment = |start: [f64; 2], end: [f64; 2]| {
        let mut w = W::default();
        w.p2(start).p2(end);
        w
    };
    let _ = (line, point);
    let block_begin = |name: &[u8]| {
        let mut w = W::default();
        w.p2([0.0, 0.0]);
        let mut options = 0;
        if r11 {
            w.text(name);
            options = 0x04;
        }
        enc(e(kind::BLOCK, options, &w))
    };
    let endblk = || enc(e(kind::ENDBLK, 0, &W::default()));
    let mut d = Drawing {
        layers: vec![(b"0".to_vec(), 7, 0), (sjis("平面図"), 2, 0), (sjis("非表示"), -3, 1)],
        linetypes: vec![
            (b"CONTINUOUS".to_vec(), b"Solid line".to_vec(), vec![]),
            (b"DASHED".to_vec(), b"Dashed".to_vec(), vec![0.5, -0.25]),
        ],
        styles: vec![(b"STANDARD".to_vec(), b"txt".to_vec())],
        dimstyles: vec![(b"STANDARD".to_vec(), 2.5)],
        codepage: if r11 { 38 } else { 0 },
        ..Drawing::default()
    };
    d.blocks.push((
        b"BOX".to_vec(),
        0,
        vec![
            block_begin(b"BOX"),
            enc(E { handle: Some(0x30), flat: true, ..e(kind::LINE, 0, &segment([0.0, 0.0], [1.0, 1.0])) }),
            endblk(),
        ],
    ));
    d.blocks.push((
        b"*D".to_vec(),
        1,
        vec![
            block_begin(b"*D"),
            enc(E { handle: Some(0x31), flat: true, ..e(kind::LINE, 0, &segment([0.0, 3.0], [4.0, 3.0])) }),
            endblk(),
        ],
    ));
    let mut text = W::default();
    text.p2([0.0, 1.0]).f64(0.25).text(&sjis("表示ソ能")).f64(std::f64::consts::PI / 6.0);
    let mut insert = W::default();
    insert.i16(0).p2([2.0, 0.0]).f64(2.0).f64(2.0).f64(std::f64::consts::PI / 2.0);
    let mut arc = W::default();
    arc.p2([5.0, 5.0]).f64(1.0).f64(0.0).f64(std::f64::consts::PI / 2.0);
    let mut polyline = W::default();
    polyline.u8(1);
    let vertex = |p: [f64; 2], bulge: Option<f64>| {
        let mut w = W::default();
        w.p2(p);
        if let Some(bulge) = bulge {
            w.f64(bulge);
        }
        enc(e(kind::VERTEX, if bulge.is_some() { 0x04 } else { 0 }, &w))
    };
    let mut seqend = W::default();
    seqend.i32(0);
    let mut circle = W::default();
    circle.p2([8.0, 8.0]).f64(1.0);
    let mut dimension = W::default();
    dimension.i16(1);
    if r10 {
        dimension.p3([4.0, 3.0, 0.0]);
    } else {
        dimension.p2([4.0, 3.0]);
    }
    dimension.p2([2.0, 3.5]);
    for p in [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0]] {
        if r10 {
            dimension.p3(p)
        } else {
            dimension.p2([p[0], p[1]])
        };
    }
    let mut attributed = W::default();
    attributed.i16(0).p2([6.0, 0.0]);
    let mut attribute = W::default();
    attribute.p2([6.0, 0.5]).f64(0.25).text(&sjis("値")).text(b"TAG").u8(0);
    let mut extra_point = W::default();
    extra_point.p2([3.0, 3.0]);
    d.entities = vec![
        enc(E { layer: 1, handle: Some(0x40), flat: true, ..e(kind::LINE, 0, &segment([0.0, 0.0], [1.0, 0.5])) }),
        enc(E { layer: 1, handle: Some(0x41), ..e(kind::TEXT, 0x01, &text) }),
        enc(E { layer: 1, handle: Some(0x43), ..e(kind::INSERT, 0x07, &insert) }),
        enc(E { color: Some(1), handle: Some(0x44), ..e(kind::ARC, 0, &arc) }),
        enc(E { follows: true, handle: Some(0x45), ..e(kind::POLYLINE, 0x01, &polyline) }),
        vertex([0.0, -2.0], Some(0.5)),
        vertex([2.0, -2.0], None),
        vertex([2.0, -1.0], None),
        enc(e(kind::SEQEND, 0, &seqend)),
        enc(E {
            layer: 2,
            linetype: Some(1),
            handle: Some(0x46),
            flat: true,
            ..e(kind::LINE, 0, &segment([0.0, -3.0], [1.0, -3.0]))
        }),
        enc(E { elevation: Some(10.0), thickness: Some(2.0), handle: Some(0x47), ..e(kind::CIRCLE, 0, &circle) }),
        enc(E { handle: Some(0x48), ..e(kind::DIMENSION, 0x18, &dimension) }),
        enc(E { follows: true, handle: Some(0x49), ..e(kind::INSERT, 0, &attributed) }),
        enc(E { handle: Some(0x4a), ..e(kind::ATTRIB, 0, &attribute) }),
        enc(e(kind::SEQEND, 0, &seqend)),
    ];
    // 消した図形（型の 0x80）
    let mut deleted = enc(E { flat: true, ..e(kind::LINE, 0, &segment([9.0, 9.0], [9.0, 9.0])) });
    deleted[0] |= 0x80;
    d.entities.insert(1, deleted);
    // 追加の図形の節の点。図形の節から JUMP で行き、JUMP で戻る（戻り先はファイルの中の位置）
    let at = 0x700 + d.entities.iter().take(3).map(Vec::len).sum::<usize>();
    let jump_size = jump(version, 0).len();
    d.entities.insert(3, jump(version, 0x8000_0000));
    d.extras = vec![
        enc(E { handle: Some(0x42), flat: true, ..e(kind::POINT, 0, &extra_point) }),
        jump(version, (at + jump_size) as u32),
    ];
    if r11 {
        // 紙空間: 全体のビューポート（表示は拡張データ ACAD の MVIEW）と線
        let mut viewport = W::default();
        viewport.p3([210.0, 148.5, 0.0]).f64(420.0).f64(297.0).i16(1);
        let mut eed = W::default();
        eed.u8(1).i16(0).u8(0).u8(5).raw(b"MVIEW").u8(2).u8(0).u8(70).i16(16);
        eed.u8(10).p3([0.0; 3]).u8(10).p3([0.0, 0.0, 1.0]);
        for value in [0.0, 297.0, 210.0, 148.5] {
            eed.u8(40).f64(value);
        }
        // 凍結したレイヤの並び（1003 は層の表の番号。非表示は 2）
        eed.u8(2).u8(0).u8(3).i16(2).u8(2).u8(1).u8(2).u8(1);
        d.entities.push(enc(E { paper: true, eed: eed.0, handle: Some(0x50), ..e(kind::VIEWPORT, 0, &viewport) }));
        d.entities.push(enc(E {
            paper: true,
            handle: Some(0x51),
            flat: true,
            ..e(kind::LINE, 0, &segment([10.0, 10.0], [100.0, 10.0]))
        }));
    }
    file(version, &d)
}

/// 同じ図面の DXF（R12、ANSI_932）。グループコードと値を | で区切る
fn dxf() -> Vec<u8> {
    let text = [
        "0|SECTION|2|HEADER",
        "9|$ACADVER|1|AC1009|9|$DWGCODEPAGE|3|ANSI_932",
        "9|$EXTMIN|10|0|20|0|30|0|9|$EXTMAX|10|100|20|50|30|0|9|$LIMMIN|10|0|20|0|9|$LIMMAX|10|420|20|297",
        "9|$LTSCALE|40|1.0|9|$TEXTSIZE|40|2.5|9|$DIMSCALE|40|1.0|9|$DIMASZ|40|2.5|9|$DIMTXT|40|2.5",
        "9|$PSLTSCALE|70|1|9|$TILEMODE|70|1",
        "9|$PEXTMIN|10|0|20|0|30|0|9|$PEXTMAX|10|0|20|0|30|0|9|$PLIMMIN|10|0|20|0|9|$PLIMMAX|10|420|20|297",
        "0|ENDSEC|0|SECTION|2|TABLES",
        "0|TABLE|2|LTYPE|70|2",
        "0|LTYPE|2|CONTINUOUS|70|0|3|Solid line|72|65|73|0|40|0.0",
        "0|LTYPE|2|DASHED|70|0|3|Dashed|72|65|73|2|40|0.75|49|0.5|49|-0.25",
        "0|ENDTAB|0|TABLE|2|LAYER|70|3",
        "0|LAYER|2|0|70|0|62|7|6|CONTINUOUS",
        "0|LAYER|2|平面図|70|0|62|2|6|CONTINUOUS",
        "0|LAYER|2|非表示|70|0|62|-3|6|DASHED",
        "0|ENDTAB|0|TABLE|2|STYLE|70|1",
        "0|STYLE|2|STANDARD|70|0|40|0.0|41|1.0|50|0.0|71|0|42|2.5|3|txt|4|",
        "0|ENDTAB|0|TABLE|2|DIMSTYLE|70|1",
        "0|DIMSTYLE|2|STANDARD|70|0|40|1.0|41|2.5|140|2.5",
        "0|ENDTAB|0|ENDSEC|0|SECTION|2|BLOCKS",
        "0|BLOCK|8|0|2|BOX|70|0|10|0|20|0|30|0|3|BOX",
        "0|LINE|5|30|8|0|10|0|20|0|30|0|11|1|21|1|31|0",
        "0|ENDBLK|8|0",
        "0|BLOCK|8|0|2|*D1|70|1|10|0|20|0|30|0|3|*D1",
        "0|LINE|5|31|8|0|10|0|20|3|30|0|11|4|21|3|31|0",
        "0|ENDBLK|8|0",
        "0|ENDSEC|0|SECTION|2|ENTITIES",
        "0|LINE|5|40|8|平面図|10|0|20|0|30|0|11|1|21|0.5|31|0",
        "0|TEXT|5|41|8|平面図|10|0|20|1|30|0|40|0.25|1|表示ソ能|50|30",
        "0|POINT|5|42|8|0|10|3|20|3|30|0",
        "0|INSERT|5|43|8|平面図|2|BOX|10|2|20|0|30|0|41|2|42|2|50|90",
        "0|ARC|5|44|8|0|62|1|10|5|20|5|30|0|40|1|50|0|51|90",
        "0|POLYLINE|5|45|8|0|66|1|10|0|20|0|30|0|70|1",
        "0|VERTEX|8|0|10|0|20|-2|30|0|42|0.5",
        "0|VERTEX|8|0|10|2|20|-2|30|0",
        "0|VERTEX|8|0|10|2|20|-1|30|0",
        "0|SEQEND|8|0",
        "0|LINE|5|46|8|非表示|6|DASHED|10|0|20|-3|30|0|11|1|21|-3|31|0",
        "0|CIRCLE|5|47|8|0|39|2|10|8|20|8|30|10|40|1",
        "0|DIMENSION|5|48|8|0|2|*D1|10|4|20|3|30|0|11|2|21|3.5|31|0|70|0|13|0|23|0|33|0|14|4|24|0|34|0",
        "0|INSERT|5|49|8|0|66|1|2|BOX|10|6|20|0|30|0",
        "0|ATTRIB|5|4A|8|0|10|6|20|0.5|30|0|40|0.25|1|値|2|TAG|70|0",
        "0|SEQEND|8|0",
        "0|VIEWPORT|5|50|8|0|67|1|10|210|20|148.5|30|0|40|420|41|297|68|1|69|1",
        "1001|ACAD|1000|MVIEW|1002|{|1070|16|1010|0|1020|0|1030|0|1010|0|1020|0|1030|1",
        "1040|0|1040|297|1040|210|1040|148.5|1002|{|1003|非表示|1002|}|1002|}",
        "0|LINE|5|51|8|0|67|1|10|10|20|10|30|0|11|100|21|10|31|0",
        "0|ENDSEC|0|EOF",
    ]
    .join("|")
    .replace('|', "\r\n");
    encoding_rs::SHIFT_JIS.encode(&text).0.into_owned()
}

/// 数を相対 1e-9 まで同じとみなして比べる（DWG はラジアン、DXF は度で持つ角度の丸めの差）
fn differences(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut Vec<String>) {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() > 1e-9 * x.abs().max(y.abs()).max(1.0) {
                out.push(format!("{path}: {x} ≠ {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: length {} ≠ {}", x.len(), y.len()));
                return;
            }
            for (index, (x, y)) in x.iter().zip(y).enumerate() {
                differences(&format!("{path}[{index}]"), x, y, out);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                match (x.get(key), y.get(key)) {
                    (Some(x), Some(y)) => differences(&format!("{path}.{key}"), x, y, out),
                    (x, y) => out.push(format!("{path}.{key}: {x:?} ≠ {y:?}")),
                }
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: {a} ≠ {b}"));
            }
        }
    }
}

/// 表示用の図面の違い。形式・版・メタデータは比べない
fn scene_differences(dwg: &DxfDocument, dxf: &DxfDocument, view: Option<&str>) -> Vec<String> {
    let mut scenes =
        [dxf_to_scene(dwg, view), dxf_to_scene(dxf, view)].map(|scene| serde_json::to_value(scene).unwrap());
    for scene in &mut scenes {
        let object = scene.as_object_mut().unwrap();
        object.remove("format");
        object.remove("version");
        object.remove("metadata");
    }
    let mut out = Vec::new();
    differences("", &scenes[0], &scenes[1], &mut out);
    out
}

/// R12 の図面は、同じ内容の R12 の DXF と同じに読める（表、ブロック、図形、JUMP の先の図形、属性、ポリラインの頂点、
/// 紙空間のビューポートと Layout1、文字コード）
#[test]
fn an_r12_drawing_reads_like_its_dxf() {
    let dxf = parse_dxf(&dxf());
    assert!(dxf.notices.is_empty(), "{:?}", dxf.notices);
    let dwg = crate::read_dwg(&drawing(Old::Ac1009)).unwrap();
    assert!(dwg.notices.is_empty(), "{:?}", dwg.notices);
    assert_eq!(dwg.version, "AC1009");
    assert_eq!(dwg.entities.len(), dxf.entities.len());
    let kinds: Vec<&str> = dwg.entities.iter().map(|entity| entity.base.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "LINE",
            "LINE",
            "LINE",
            "TEXT",
            "POINT",
            "INSERT",
            "ARC",
            "POLYLINE",
            "LINE",
            "CIRCLE",
            "DIMENSION",
            "INSERT",
            "VIEWPORT",
            "LINE"
        ]
    );
    for view in [None, Some("Model"), Some("Layout1")] {
        let out = scene_differences(&dwg, &dxf, view);
        assert!(out.is_empty(), "{view:?}\n{}", out.join("\n"));
    }
    let DxfEntityData::Viewport(viewport) =
        &dwg.entities.iter().find(|entity| entity.base.kind == "VIEWPORT").unwrap().data
    else {
        panic!("VIEWPORT")
    };
    assert_eq!(viewport.frozen_layer_names, ["非表示"]);
}

/// 表示用の図面から、図形の id（ハンドルか記録の番号）を除く
fn without_ids(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("id");
            map.remove("properties");
            map.values_mut().for_each(without_ids);
        }
        serde_json::Value::Array(list) => list.iter_mut().for_each(without_ids),
        _ => {}
    }
}

/// R10 から AutoCAD 2.0 までの図面（紙空間がない。R10 より前は 3D の点・ハンドルもない）も、同じ図面の DXF とモデル空間が
/// 同じに見える。文字コードの指定がないので、Shift_JIS と判断する
#[test]
fn older_drawings_read_like_the_dxf_in_model_space() {
    let dxf = parse_dxf(&dxf());
    for version in [Old::Ac1006, Old::Ac1004, Old::Ac1003, Old::Ac1002, Old::Ac1001, Old::Ac210, Old::Ac150] {
        let dwg = crate::read_dwg(&drawing(version)).unwrap();
        assert_eq!(dwg.version, version.name());
        let codes: Vec<&str> = dwg.notices.iter().map(|notice| notice.code.as_str()).collect();
        assert_eq!(codes, ["guessed-shift-jis"], "{version:?}");
        assert!(dwg.layouts.is_empty(), "{version:?}");
        let mut scenes = [dxf_to_scene(&dwg, Some("Model")), dxf_to_scene(&dxf, Some("Model"))]
            .map(|scene| serde_json::to_value(scene).unwrap());
        for scene in &mut scenes {
            let object = scene.as_object_mut().unwrap();
            // 注意（文字コードの判断、DXF の紙空間）は doc.notices で確かめる
            for key in ["format", "version", "metadata", "views", "notices"] {
                object.remove(key);
            }
            without_ids(scene);
        }
        let mut out = Vec::new();
        differences("", &scenes[0], &scenes[1], &mut out);
        assert!(out.is_empty(), "{version:?}\n{}", out.join("\n"));
    }
}

/// ハンドルのない図形の id は、ファイルの中の記録の番号（図形・ブロックの図形・追加の図形の節の順。消した図形と JUMP も
/// 数える）
#[test]
fn entities_without_handles_are_numbered_by_their_record() {
    let doc = crate::read_dwg(&drawing(Old::Ac1003)).unwrap();
    let ordinal = |kind: &str, block: Option<&str>| {
        doc.entities
            .iter()
            .find(|entity| entity.base.kind == kind && entity.base.block.as_deref() == block)
            .map(|entity| (entity.base.handle.as_str(), entity.base.ordinal))
    };
    assert_eq!(ordinal("TEXT", None), Some(("", 3)));
    assert_eq!(ordinal("POINT", None), Some(("", 24)));
    assert_eq!(ordinal("LINE", Some("BOX")), Some(("", 19)));
    let DxfEntityData::Insert(insert) =
        &doc.entities.iter().rev().find(|entity| entity.base.kind == "INSERT").unwrap().data
    else {
        panic!("INSERT")
    };
    assert_eq!(insert.attribs[0].base.ordinal, 16);
    // R10 からはハンドル
    let doc = crate::read_dwg(&drawing(Old::Ac1006)).unwrap();
    assert!(doc.entities.iter().all(|entity| !entity.base.handle.is_empty() && entity.base.ordinal == 0));
}

/// AutoCAD 1.x の図面: ヘッダ変数の後に図形の数だけ記録が並ぶ（型・フラグ・層の番号・フラグと、型ごとの値）
fn early_drawing(version: Old) -> Vec<u8> {
    let start = match version {
        Old::Mc00 => 0x1c8,
        Old::Ac12 => 0x1d8,
        _ => 0x202,
    };
    let mut out = vec![0u8; start];
    out[..version.name().len()].copy_from_slice(version.name().as_bytes());
    let mut colors = W::default();
    // レイヤ 0 は色 0（白）、1 は赤、2 は非表示（0x80）の緑。ほかは使っていない（255）
    for number in 0..128 {
        colors.i16(match number {
            0 => 0,
            1 => 1,
            2 => 0x80 | 3,
            _ => 255,
        });
    }
    out[0xc8..0xc8 + 256].copy_from_slice(&colors.0);
    out[0xb4..0xbc].copy_from_slice(&2.5f64.to_le_bytes());
    let mut records: Vec<W> = Vec::new();
    let head = |kind: i8, layer: u8| {
        let mut w = W::default();
        w.u8(kind as u8).u8(0).u8(layer).u8(0);
        w
    };
    let mut line = head(kind::LINE, 1);
    line.p2([0.0, 0.0]).p2([1.0, 1.0]);
    records.push(line);
    let mut deleted = head(-kind::LINE, 1);
    deleted.p2([5.0, 5.0]).p2([6.0, 6.0]);
    records.push(deleted);
    let mut text = head(kind::TEXT, 2);
    text.p2([1.0, 2.0]).f64(2.5).f64(std::f64::consts::FRAC_PI_4).text(b"Foo");
    records.push(text);
    let mut block = head(kind::BLOCK, 0);
    block.text(b"B").p2([0.0, 0.0]);
    records.push(block);
    let mut inside = head(kind::CIRCLE, 0);
    inside.p2([0.0, 0.0]).f64(1.0);
    records.push(inside);
    records.push(head(kind::ENDBLK, 0));
    let mut insert = head(kind::INSERT, 0);
    insert.text(b"B").p2([4.0, 4.0]).f64(2.0).f64(2.0).f64(0.0);
    records.push(insert);
    records.push(head(kind::REPEAT, 0));
    let mut repeated = head(kind::LINE, 0);
    repeated.p2([0.0, 0.0]).p2([0.0, 1.0]);
    records.push(repeated);
    let mut endrep = head(kind::ENDREP, 0);
    endrep.i16(3).i16(1).f64(1.0).f64(0.0);
    records.push(endrep);
    out[0x28..0x2a].copy_from_slice(&(records.len() as i16).to_le_bytes());
    for record in records {
        out.extend_from_slice(&record.0);
    }
    out
}

/// AutoCAD 1.x: レイヤは番号が名前で、色はヘッダ変数（0 は白、0x80 は非表示、255 は使っていない）。ブロックは同じ並びの
/// BLOCK から ENDBLK まで、配置は名前で引く。REPEAT から ENDREP までの図形は列・行に並べる。消した図形（型が負）は読まない
#[test]
fn early_drawings_read_with_numbered_layers() {
    for version in [Old::Mc00, Old::Ac12, Old::Ac140] {
        let bytes = early_drawing(version);
        assert!(crate::is_dwg(&bytes), "{version:?}");
        let doc = crate::read_dwg(&bytes).unwrap();
        assert!(doc.notices.is_empty(), "{version:?}: {:?}", doc.notices);
        let layers: Vec<(&str, f64, bool)> =
            doc.layers.iter().map(|layer| (layer.name.as_str(), layer.color_index, layer.off)).collect();
        assert_eq!(layers, [("0", 7.0, false), ("1", 1.0, false), ("2", 3.0, true)]);
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].name, "B");
        let model: Vec<(&str, &str)> = doc
            .entities
            .iter()
            .filter(|entity| entity.base.space == DxfSpace::Model)
            .map(|entity| (entity.base.kind.as_str(), entity.base.layer.as_str()))
            .collect();
        assert_eq!(model, [("LINE", "1"), ("TEXT", "2"), ("INSERT", "0"), ("LINE", "0"), ("LINE", "0"), ("LINE", "0")]);
        let DxfEntityData::Text(text) = &doc.entities.iter().find(|entity| entity.base.kind == "TEXT").unwrap().data
        else {
            panic!("TEXT")
        };
        assert_eq!((text.text.as_str(), text.insert, text.height), ("Foo", [1.0, 2.0, 0.0], 2.5));
        assert!((text.rotation - 45.0).abs() < 1e-9);
        let repeated: Vec<[f64; 3]> = doc
            .entities
            .iter()
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Line { start, .. } if entity.base.layer == "0" => Some(*start),
                _ => None,
            })
            .collect();
        assert_eq!(repeated, [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        // 記録の番号は並びの順（消した図形も数える）
        assert_eq!(doc.entities.iter().find(|entity| entity.base.kind == "TEXT").unwrap().base.ordinal, 3);
    }
}

/// 壊れた図面（切れた、バイトが変わった、抜けた、繰り返した）も、止まらずに読み終える
#[test]
fn damaged_old_drawings_finish_without_panicking() {
    let mut seed: u64 = 0x5eed;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    for original in [drawing(Old::Ac1009), drawing(Old::Ac1006), drawing(Old::Ac1003), early_drawing(Old::Ac140)] {
        for round in 0..400 {
            let mut bytes = original.clone();
            match round % 4 {
                0 => bytes.truncate(random(bytes.len())),
                1 => {
                    for _ in 0..1 + random(8) {
                        let at = random(bytes.len());
                        bytes[at] = random(256) as u8;
                    }
                }
                2 => {
                    let at = random(bytes.len());
                    let length = random(64).min(bytes.len() - at);
                    bytes.drain(at..at + length);
                }
                _ => {
                    let at = random(bytes.len());
                    let length = random(16).min(bytes.len() - at);
                    let copy: Vec<u8> = bytes[at..at + length].to_vec();
                    bytes.splice(at..at, copy.iter().copied().cycle().take(length * random(4)));
                }
            }
            let start = std::time::Instant::now();
            if let Ok(doc) = crate::read_dwg(&bytes) {
                for view in [None, Some("Layout1")] {
                    dxf_to_scene(&doc, view);
                }
            }
            assert!(start.elapsed().as_secs() < 5, "round {round} took too long");
        }
    }
}

/// 手元の見本の図面（ZUMEN_R12_SAMPLES のフォルダの下の DWG。リポジトリには入れない）
fn samples() -> Vec<(String, Vec<u8>)> {
    let Some(folder) = std::env::var_os("ZUMEN_R12_SAMPLES") else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(folder)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("dwg"))
                && let Ok(bytes) = std::fs::read(&path)
            {
                out.push((path.display().to_string(), bytes));
            }
        }
    }
    out.sort();
    out
}

/// 節の記録を大きさで順にたどり、種類ごとの値の終わりが記録の終わりと合い（R11 は CRC の前）、R11 の CRC が合うことを
/// 確かめる。合わない記録の説明を返す
fn check_section(
    name: &str,
    bytes: &[u8],
    section: records::Section,
    version: Old,
    problems: &mut Vec<String>,
) -> usize {
    let mut position = section.start;
    let mut count = 0;
    while position < section.end {
        match records::record(bytes, position, section.end, version) {
            Ok((step, next)) => {
                if version >= Old::Ac1009 {
                    let stored = u16::from_le_bytes([bytes[next - 2], bytes[next - 1]]);
                    if crc8(CRC8_SEED, &bytes[position..next - 2]) != stored {
                        problems.push(format!("{name}: CRC at {position:#x}"));
                    }
                }
                if let Step::Record(record) = step
                    && record.consumed[0] != record.consumed[1]
                {
                    problems.push(format!(
                        "{name}: type {} at {position:#x} ends at {:#x} instead of {:#x}",
                        record.kind, record.consumed[0], record.consumed[1]
                    ));
                }
                count += 1;
                position = next;
            }
            Err(_) => {
                problems.push(format!("{name}: unreadable record at {position:#x}"));
                break;
            }
        }
    }
    count
}

#[test]
fn records_of_sample_drawings_end_where_their_size_says() {
    let mut problems = Vec::new();
    let mut total = 0;
    for (name, bytes) in samples() {
        let Some(version) = Old::from_magic(&bytes) else { continue };
        if version < Old::Ac150 {
            continue;
        }
        let header = records::file_header(&bytes, version).unwrap();
        for section in [header.entities, header.blocks, header.extras] {
            if section.start > 0 {
                total += check_section(&name, &bytes, section, version, &mut problems);
            }
        }
        // R11 の表の記録も CRC で終わる
        if version < Old::Ac1009 {
            continue;
        }
        let t = &header.tables;
        for table in [t.block, t.layer, t.style, t.linetype, t.dimstyle].into_iter().flatten() {
            for index in 0..table.items {
                let start = table.begin + index * table.item_size;
                let end = start + table.item_size;
                let stored = u16::from_le_bytes([bytes[end - 2], bytes[end - 1]]);
                if crc8(CRC8_SEED, &bytes[start..end - 2]) != stored {
                    problems.push(format!("{name}: CRC of the table record at {start:#x}"));
                }
            }
        }
    }
    eprintln!("{total} records");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// 実物の図面（ZUMEN_R12_SAMPLES）は、読めない記録なしに読める。版ごとの数を数える
#[test]
fn real_old_drawings_read_without_broken_records() {
    let mut versions: Vec<(String, usize)> = Vec::new();
    for (name, bytes) in samples() {
        let Some(version) = Old::from_magic(&bytes) else { continue };
        let doc = crate::read_dwg(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let broken: Vec<&str> = doc
            .notices
            .iter()
            .filter(|notice| notice.code.starts_with("dwg-broken"))
            .map(|notice| notice.message.as_str())
            .collect();
        assert!(broken.is_empty(), "{name}: {broken:?}");
        match versions.iter_mut().find(|(key, _)| key == version.name()) {
            Some(entry) => entry.1 += 1,
            None => versions.push((version.name().to_string(), 1)),
        }
    }
    eprintln!("{versions:?}");
}
