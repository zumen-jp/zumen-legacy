//! AutoCAD の図面のモデル（`DxfDocument`）を JWW の図面（`JwwDocument`）に写す。JWW から DXF への写し方（jww.rs）の
//! 逆で、表示（zumen-dxf の scene.rs）と同じ順・同じ規則で辿り、表示と同じ所に同じ形を描く JWW にする。
//!
//! - 用紙: JWW の座標は用紙の中心を原点とする用紙の mm で、レイヤグループの縮尺を掛けると実寸になる。表示する空間
//!   （`dxf_to_scene` の既定の表示）がレイアウトなら、その用紙の中心を JWW の用紙の中心に置く。JWW の用紙は同じ大きさの
//!   A0〜A4・2A〜5A（横）で、なければそれを含むいちばん小さい用紙。紙空間の図形は縮尺 1/1 のレイヤグループに置き、
//!   ビューポートに映るモデル空間の図形は、ビューポートの縮尺のレイヤグループに、用紙に映る位置・向きで置く。
//!   モデル空間を表示する図面は、モデル空間を A3 に収まる縮尺（1/1・1/2・1/5・1/10…）のレイヤグループに置く。
//! - レイヤ: 縮尺ごとにレイヤグループ（16 枚のレイヤ）を順に割り当て、DXF のレイヤを 1 枚ずつ置く。「0-A 名前」の
//!   レイヤ（JWW から写した DXF）は、縮尺の合うかぎりその番号のレイヤに戻す。
//! - 色: 前景色（ACI 7）は線色 2（黒）、Jw_cad の線色 1〜9 の画面色と同じ色はその線色、SXF の既定義色は SXF 対応色
//!   101〜116、ほかは SXF 対応色のユーザー定義（117〜355）。塗りは線色 1〜9 にない色を任意色（線色 10）にする。
//! - 線種: Jw_cad の線種 2〜9 か SXF の既定義線種（32〜45）と同じ長さならそれ、ほかは SXF 対応線種のユーザー定義
//!   （47〜62）。線の太さは図形ごとの線幅（1/100 mm）。
//! - ブロック参照は JWW のブロック（継ぐ値・縮尺の違う配置ごとに定義を作る）。線分・円・円弧・楕円・折線・点・
//!   SOLID は JWW の線・円弧・点・ソリッドにする。文字・塗り・ハッチング・スプライン・寸法・引出線などは、表示が描く
//!   線・塗り・文字を、JWW の線・ソリッド（台形に分けた塗り）・文字にする。写せない・形を変えた内容は notices で返す。

use crate::shapes::{bulge_arc, decompose, inside, length_scale, mirrors, normalize};
use crate::to_sxf::{
    Paint, clip_segment, decompose_text, needs_handles, paint_of_aci, paint_of_hex, top_id, with_handles,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::{PI, TAU};
use zumen_core::geometry::{IDENTITY, apply, apply_vector, invert, multiply, transform_points};
use zumen_core::text::{font_family_for, is_half_width, is_serif_family};
use zumen_core::{Matrix, Notice, NoticeLevel, NoticeList, Primitive, TextPrimitive, js};
use zumen_dxf::{
    DxfDocument, DxfEntity, DxfEntityBase, DxfEntityData, DxfLayout, DxfSpace, DxfViewport, dash_array, draw_order,
    drawing_unit, dxf_to_scene, is_transparent, minor_axis, ocs_matrix, true_color,
};
use zumen_jww::{
    JwwArc, JwwBase, JwwBlock, JwwBlockDefinition, JwwDocument, JwwEntity, JwwLine, JwwPoint, JwwSolid, JwwText,
    JwwWritten, SXF_COLORS, WidthUnit, is_setting, paper_size, write_jww,
};

/// ブロックの入れ子の上限（表示と同じ）
const MAX_DEPTH: usize = 32;
/// 線の太さの既定（表示と同じ、mm）
const DEFAULT_LINEWEIGHT: f64 = 0.25;
/// 点の長さ 0 の線種の線分を書く長さ（用紙の mm。SXF の既定義線種の点と同じ）
const DOT: f64 = 0.25;
/// レイヤグループの数と、1 つのレイヤグループのレイヤの数
const GROUPS: usize = 16;
const LAYERS: usize = 16;
/// JWW の用紙（paper_size の番号）。小さい順
const PAPER_CODES: [u32; 9] = [4, 3, 2, 1, 0, 8, 9, 10, 11];
/// モデル空間を表示する図面の用紙（A3）と、図形の範囲を用紙に収める割合
const MODEL_PAPER: u32 = 3;
const MODEL_FILL: f64 = 0.9;
/// SXF 対応色のユーザー定義（線色 117〜355）と、SXF 対応線種のユーザー定義（線種 47〜62）
const USER_COLORS: std::ops::RangeInclusive<u16> = 117..=355;
const USER_LINE_TYPES: std::ops::RangeInclusive<u8> = 47..=62;
/// SXF 対応線種の線分・空白の数の上限
const MAX_SEGMENTS: usize = 10;
/// 実線にする破線の周期（用紙の mm）
const MIN_PERIOD: f64 = 0.05;
/// 文字のフォントが分からない・SHX のときのフォント（Jw_cad の既定）と、明朝体の代わりのフォント
const DEFAULT_FONT: &str = "ＭＳ ゴシック";
const SERIF_FONT: &str = "ＭＳ 明朝";
/// 塗りを台形に分けるとき、辺どうしの交点を求める辺の数の上限と、分ける辺の数の上限
const MAX_CROSSING_EDGES: usize = 2_000;
const MAX_FILL_EDGES: usize = 100_000;
/// ハッチングの線を領域で切る計算（線分の数 × 輪郭の辺の数）の上限
const MAX_CLIP_WORK: usize = 20_000_000;

/// 写した JWW の図面と、写せなかった・形を変えて写した内容
#[derive(Debug, Clone, PartialEq)]
pub struct MappedJww {
    pub document: JwwDocument,
    pub notices: Vec<Notice>,
    /// JWW の図形 → 元の図形の表示の id の部分。最上位の図形は「#n」（図形の一覧の n 番目）、ブロックの定義の図形は
    /// 「ブロックの番号#n」。ビューポートに映る図形は「ビューポートのハンドル:ハンドル」、ブロック参照の配置は
    /// 「ハンドル>」で終わり、配置の中の図形の id は、配置から順につなぐと元の表示の id になる
    pub sources: HashMap<String, String>,
}

impl MappedJww {
    /// JWW に書く
    pub fn write(&self) -> JwwWritten {
        write_jww(&self.document)
    }
}

/// AutoCAD の図面のモデルを JWW の図面に写す
pub fn dxf_to_jww(doc: &DxfDocument) -> MappedJww {
    let handled;
    let doc = if needs_handles(doc) {
        handled = with_handles(doc);
        &handled
    } else {
        doc
    };
    Mapper::new(doc).run()
}

/// ブロック参照・寸法の中の図形が継ぐ値（BYBLOCK の色・線の太さ（mm）・線種、レイヤ 0 の図形のレイヤ）
#[derive(Debug, Clone, PartialEq)]
struct Inherit {
    paint: Paint,
    lineweight: f64,
    linetype: String,
    layer: Option<String>,
}

/// 継ぐ値の比べる形（色、線の太さ、線種、レイヤ 0 の継ぎ先）
type InheritKey = (Paint, u64, String, Option<String>);

impl Inherit {
    fn key(&self) -> InheritKey {
        (self.paint, self.lineweight.to_bits(), self.linetype.to_uppercase(), self.layer.clone())
    }
}

/// 辿るときの文脈
#[derive(Debug, Clone)]
struct Ctx {
    /// 図形の座標（DXF）→ 入れ物（最上位は JWW の座標、ブロックの中はブロックの座標）
    m: Matrix,
    /// 入れ物の座標 → JWW の最上位の座標（用紙の mm。線種のピッチと、構築線を切る範囲）
    to_paper: Matrix,
    /// 線種の長さの補正（表示と同じ。紙空間で線種の尺度を揃えるビューポートは縮尺の逆数）
    dash_factor: f64,
    inherit: Inherit,
    /// 辿っているブロック（循環を断つ）
    stack: Vec<String>,
    /// 寸法の中: ブロック参照もブロックにせず、寸法の図形として写す
    flatten: bool,
    /// 図形の元の id の前に付けるもの（ビューポートに映る図形の「ハンドル:」）
    prefix: String,
    /// すべての図形の元の id（寸法の中の図形は寸法の id）
    source: Option<String>,
    /// ビューポートの枠（JWW の座標）。構築線・放射線を切る
    clip: Option<[f64; 4]>,
    /// 縮尺の番号（Mapper::scales の添字）
    scale: usize,
}

impl Ctx {
    fn source(&self, base: &DxfEntityBase) -> String {
        self.source.clone().unwrap_or_else(|| format!("{}{}", self.prefix, base.handle))
    }
}

/// 入れ物の図形。レイヤはレイヤの割り当て（Slot）の番号で持ち、最後に JWW のレイヤグループ・レイヤにする
enum Item {
    Entity(Box<JwwEntity>, usize, String),
    /// 表示が描く形から写す図形（Mapper::pending の添字）
    Pending(usize),
}

impl Item {
    fn entity(entity: JwwEntity, slot: usize, source: String) -> Item {
        Item::Entity(Box::new(entity), slot, source)
    }
}

/// DXF のレイヤを置く場所（縮尺と DXF のレイヤ）
struct Slot {
    scale: usize,
    /// DXF のレイヤの名前（レイヤの表の名前）と、比べる形（大文字）
    name: String,
    upper: String,
}

/// 表示が描く形から写す図形
struct Pending {
    /// 継ぐ値を入れた図形（ハンドルは表示の中で見分ける仮のもの）
    entity: DxfEntity,
    m: Matrix,
    /// 表示の線種の長さ → 用紙の mm
    dash_scale: f64,
    /// 表示のハッチングの模様の破線の長さ → 用紙の mm（模様は線種の尺度・PSLTSCALE によらない）
    pattern_scale: f64,
    slot: usize,
    font: String,
    source: String,
}

/// 写す途中のブロックの定義
struct Block {
    number: u32,
    name: String,
    items: Vec<Item>,
}

/// レイヤの割り当て: レイヤを置く場所ごとのレイヤグループ・レイヤ、レイヤグループの縮尺の番号、
/// レイヤグループ・レイヤ → そこに置いた最初の場所
struct Assignment {
    places: Vec<(usize, usize)>,
    group_scale: [Option<usize>; GROUPS],
    owner: HashMap<(usize, usize), usize>,
}

/// (ブロックの名前（大文字。破線を持つブロックは用紙での倍率つき）, 継ぐ値, 縮尺の番号) → ブロックの番号
type VariantKey = (String, Option<InheritKey>, usize);

struct Mapper<'a> {
    doc: &'a DxfDocument,
    unit: f64,
    order: zumen_dxf::DrawOrder<'a>,
    notices: NoticeList,
    layers_upper: HashMap<String, &'a zumen_dxf::DxfLayer>,
    linetypes_upper: HashMap<String, &'a zumen_dxf::DxfLinetype>,
    /// レイヤグループの縮尺の分母。添字が縮尺の番号
    scales: Vec<f64>,
    slots: Vec<Slot>,
    slot_index: HashMap<(usize, String), usize>,
    top: Vec<Item>,
    /// ブロックの定義（中のブロックを先に並べる）
    blocks: Vec<Block>,
    block_names: HashSet<String>,
    next_block: u32,
    variants: HashMap<VariantKey, u32>,
    inherits: HashMap<String, bool>,
    pending: Vec<Pending>,
    /// JWW の用紙（paper_size の番号）
    paper: u32,
    /// 用紙の範囲（JWW の座標）。構築線・放射線を切る（表示の finish_infinite と同じ）
    limit: [f64; 4],
    /// Jw_cad の線色 1〜9 の画面色（添字は線色）
    pen_rgb: [Option<[u8; 3]>; 10],
    /// SXF 対応色の画面色（線色 100〜356、添字は線色 − 100）と、ユーザー定義に割り当てた色
    extended: Vec<String>,
    user_colors: HashMap<[u8; 3], u16>,
    /// 既定の線種（Jw_cad の線種 2〜9 と SXF の既定義線種）の線分・空白の長さ
    default_line_types: Vec<(u8, Vec<f64>)>,
    /// ユーザー定義の線種（線分・空白の長さのビット列 → 線種）と、その長さ
    user_line_types: HashMap<Vec<u64>, u8>,
    line_types: BTreeMap<u32, Vec<f64>>,
    /// JWW の空の図面（既定の設定）
    blank: JwwDocument,
}

/// "#rrggbb" の RGB
fn rgb_of(hex: &str) -> Option<[u8; 3]> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() < 6 || !digits.is_ascii() {
        return None;
    }
    let part = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// RGB の COLORREF（0x00BBGGRR）
fn colorref([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

/// 角度（ラジアン）を 0 以上 2π 未満にする
fn normalize_radians(radians: f64) -> f64 {
    let value = radians.rem_euclid(TAU);
    if (value - TAU).abs() < 1e-12 || value.abs() < 1e-12 { 0.0 } else { value }
}

/// 用紙の大きさ（mm）に合う JWW の用紙の番号と、同じ大きさか。同じ大きさの用紙（横）がなければ、それを含む
/// いちばん小さい用紙（5A より大きければ 5A）
fn paper_code(width: f64, height: f64) -> (u32, bool) {
    let close = |a: f64, b: f64| (a - b).abs() <= 0.5;
    let sizes = PAPER_CODES.iter().filter_map(|code| paper_size(*code).map(|(_, w, h)| (*code, w, h)));
    for (code, w, h) in sizes.clone() {
        if close(width, w) && close(height, h) {
            return (code, true);
        }
    }
    let code = sizes.clone().find(|(_, w, h)| width <= w + 0.5 && height <= h + 0.5).map_or(11, |(code, _, _)| code);
    (code, false)
}

/// レイヤの名前「0-A 名前」（JWW から写した DXF のレイヤ）の、レイヤグループ・レイヤの番号と名前
fn parse_layer_name(name: &str) -> Option<(usize, usize, String)> {
    let bytes = name.as_bytes();
    if bytes.len() < 3 || bytes[1] != b'-' {
        return None;
    }
    let digit = |byte: u8| (byte as char).to_digit(16).filter(|_| !byte.is_ascii_lowercase()).map(|d| d as usize);
    let (group, layer) = (digit(bytes[0])?, digit(bytes[2])?);
    let rest = &name[3..];
    if rest.is_empty() {
        return Some((group, layer, String::new()));
    }
    let named = rest.strip_prefix(' ')?;
    Some((group, layer, named.to_string()))
}

/// 縮尺の分母を、整数や 6 桁までの小数に近ければその値にする（ビューポートの縮尺の逆数の丸めの誤差）
fn tidy_scale(scale: f64) -> f64 {
    for step in [1.0, 1e-6] {
        let rounded = (scale / step).round() * step;
        if rounded > 0.0 && (rounded - scale).abs() <= 1e-9 * scale {
            return rounded;
        }
    }
    scale
}

/// モデル空間の図形の範囲（実寸の mm）を用紙の幅・高さに収める縮尺の分母（1・2・5 × 10 の累乗）
fn model_scale(width: f64, height: f64, paper_width: f64, paper_height: f64) -> f64 {
    let mut power = 1.0;
    for _ in 0..12 {
        for step in [1.0, 2.0, 5.0] {
            let scale = step * power;
            if width / scale <= paper_width && height / scale <= paper_height {
                return scale;
            }
        }
        power *= 10.0;
    }
    power
}

/// 円弧・楕円弧の JWW の値（中心・半径・扁平率・傾き・始角・円弧角（ラジアン、反時計回り）・全円か）
#[derive(Debug, Clone, Copy, PartialEq)]
struct ArcShape {
    cx: f64,
    cy: f64,
    radius: f64,
    flatness: f64,
    tilt: f64,
    start: f64,
    sweep: f64,
    full: bool,
}

/// 曲線 c + p·cos t + q·sin t（t は t0 から t0 + sweep、sweep は正）を変換 m で写し、JWW の円弧（反時計回り）にする。
/// 写した曲線は楕円で、その長軸・短軸を、共役な半径 m·p・m·q から求める。つぶれた形は None
fn arc_shape(m: &Matrix, c: [f64; 2], p: [f64; 2], q: [f64; 2], t0: f64, sweep: f64, full: bool) -> Option<ArcShape> {
    let (cx, cy) = apply(m, c[0], c[1]);
    let (px, py) = apply_vector(m, p[0], p[1]);
    let (qx, qy) = apply_vector(m, q[0], q[1]);
    // 共役な半径を直交させる媒介変数のずれ: tan 2t* = 2 p·q / (|p|² − |q|²)
    let shift = 0.5 * (2.0 * (px * qx + py * qy)).atan2(px * px + py * py - qx * qx - qy * qy);
    let (sin, cos) = shift.sin_cos();
    let (mut ux, mut uy) = (px * cos + qx * sin, py * cos + qy * sin);
    let (mut vx, mut vy) = (qx * cos - px * sin, qy * cos - py * sin);
    let mut offset = shift;
    if vx.hypot(vy) > ux.hypot(uy) {
        // 長軸を u にする（媒介変数は 90° ずれる）
        (ux, uy, vx, vy) = (vx, vy, -ux, -uy);
        offset += PI / 2.0;
    }
    let radius = ux.hypot(uy);
    let minor = vx.hypot(vy);
    if !(radius.is_finite() && radius > 0.0 && minor.is_finite() && minor > 0.0) {
        return None;
    }
    // 短軸が長軸の反時計回りの向きでなければ、媒介変数の向きが逆になる
    let ccw = ux * vy - uy * vx > 0.0;
    let (mut start, mut sweep) = if ccw { (t0 - offset, sweep) } else { (offset - t0, -sweep) };
    if sweep < 0.0 {
        start += sweep;
        sweep = -sweep;
    }
    let mut tilt = uy.atan2(ux);
    let mut flatness = minor / radius;
    if (flatness - 1.0).abs() <= 1e-9 {
        // 円は傾きを 0 にし、始角を用紙の向きで持つ
        flatness = 1.0;
        start += tilt;
        tilt = 0.0;
    }
    let full = full || sweep >= TAU - 1e-12;
    if full {
        start = 0.0;
        sweep = TAU;
    }
    Some(ArcShape { cx, cy, radius, flatness, tilt, start: normalize_radians(start), sweep, full })
}

/// 折線の点列（x, y の並び）の線分。segments は 2 点ずつの線分の並び。長さ 0 の線分は除く
fn segments_of(points: &[f64], closed: bool, segments: bool) -> Vec<[f64; 4]> {
    let mut out = Vec::new();
    let mut push = |x1: f64, y1: f64, x2: f64, y2: f64| {
        if (x1 != x2 || y1 != y2) && [x1, y1, x2, y2].iter().all(|value| value.is_finite()) {
            out.push([x1, y1, x2, y2]);
        }
    };
    if segments {
        for chunk in points.chunks(4).filter(|chunk| chunk.len() == 4) {
            push(chunk[0], chunk[1], chunk[2], chunk[3]);
        }
        return out;
    }
    let n = points.len() / 2;
    for i in 1..n {
        push(points[i * 2 - 2], points[i * 2 - 1], points[i * 2], points[i * 2 + 1]);
    }
    if closed && n >= 3 {
        push(points[n * 2 - 2], points[n * 2 - 1], points[0], points[1]);
    }
    out
}

/// 点が輪郭の組（偶奇規則）の内側か
fn inside_rings(rings: &[Vec<f64>], x: f64, y: f64) -> bool {
    rings.iter().filter(|ring| inside(ring, x, y)).count() % 2 == 1
}

/// 線分を、偶奇規則で塗る輪郭の組の内側の部分に切る
fn clip_to_rings([x1, y1, x2, y2]: [f64; 4], rings: &[Vec<f64>]) -> Vec<[f64; 4]> {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let mut ts = vec![0.0, 1.0];
    for ring in rings {
        let n = ring.len() / 2;
        for i in 0..n {
            let j = (i + 1) % n;
            let (ax, ay, bx, by) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
            let (ex, ey) = (bx - ax, by - ay);
            let denominator = dx * ey - dy * ex;
            if denominator == 0.0 {
                continue;
            }
            let t = ((ax - x1) * ey - (ay - y1) * ex) / denominator;
            let u = ((ax - x1) * dy - (ay - y1) * dx) / denominator;
            if t > 0.0 && t < 1.0 && (0.0..=1.0).contains(&u) {
                ts.push(t);
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<[f64; 4]> = Vec::new();
    for pair in ts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b - a <= 1e-12 {
            continue;
        }
        let mid = (a + b) / 2.0;
        if !inside_rings(rings, x1 + dx * mid, y1 + dy * mid) {
            continue;
        }
        let (sx, sy, ex, ey) = (x1 + dx * a, y1 + dy * a, x1 + dx * b, y1 + dy * b);
        // 続く部分は 1 本にする
        match out.last_mut() {
            Some(last) if last[2] == sx && last[3] == sy => {
                last[2] = ex;
                last[3] = ey;
            }
            _ => out.push([sx, sy, ex, ey]),
        }
    }
    out
}

/// 塗りの辺（下端から上端へ）
#[derive(Debug, Clone, Copy)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Edge {
    fn x_at(&self, y: f64) -> f64 {
        if y <= self.y0 {
            return self.x0;
        }
        if y >= self.y1 {
            return self.x1;
        }
        self.x0 + (self.x1 - self.x0) * (y - self.y0) / (self.y1 - self.y0)
    }
}

/// 偶奇規則で塗る輪郭の組を、横の帯ごとの台形（JWW のソリッドの 4 点: 左下 → 右下 → 右上 → 左上）に分ける。
/// 帯の境は頂点と辺どうしの交点の高さで、上下の帯で同じ辺の組の台形は 1 つにつなぐ。辺が多すぎて分けきれない
/// ときは、分けた分と false
fn trapezoids(rings: &[Vec<f64>]) -> (Vec<[f64; 8]>, bool) {
    let mut edges: Vec<Edge> = Vec::new();
    for ring in rings {
        let n = ring.len() / 2;
        if n < 3 {
            continue;
        }
        for i in 0..n {
            let j = (i + 1) % n;
            let (ax, ay, bx, by) = (ring[i * 2], ring[i * 2 + 1], ring[j * 2], ring[j * 2 + 1]);
            if ay == by || ![ax, ay, bx, by].iter().all(|value| value.is_finite()) {
                continue;
            }
            edges.push(if ay < by {
                Edge { x0: ax, y0: ay, x1: bx, y1: by }
            } else {
                Edge { x0: bx, y0: by, x1: ax, y1: ay }
            });
        }
    }
    if edges.len() > MAX_FILL_EDGES {
        return (Vec::new(), false);
    }
    let mut complete = true;
    let mut ys: Vec<f64> = edges.iter().flat_map(|edge| [edge.y0, edge.y1]).collect();
    if edges.len() <= MAX_CROSSING_EDGES {
        for (i, a) in edges.iter().enumerate() {
            for b in &edges[i + 1..] {
                if a.y1 <= b.y0 || b.y1 <= a.y0 {
                    continue;
                }
                let (dax, day, dbx, dby) = (a.x1 - a.x0, a.y1 - a.y0, b.x1 - b.x0, b.y1 - b.y0);
                let denominator = dax * dby - day * dbx;
                if denominator == 0.0 {
                    continue;
                }
                let t = ((b.x0 - a.x0) * dby - (b.y0 - a.y0) * dbx) / denominator;
                let u = ((b.x0 - a.x0) * day - (b.y0 - a.y0) * dax) / denominator;
                if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                    ys.push(a.y0 + day * t);
                }
            }
        }
    } else {
        complete = false;
    }
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let mut order: Vec<usize> = (0..edges.len()).collect();
    order.sort_by(|a, b| edges[*a].y0.total_cmp(&edges[*b].y0));
    let mut next = 0;
    let mut active: Vec<usize> = Vec::new();
    let mut out: Vec<[f64; 8]> = Vec::new();
    // 続けられる台形: (左の辺, 右の辺) → out の添字
    let mut open: HashMap<(usize, usize), usize> = HashMap::new();
    for pair in ys.windows(2) {
        let (ya, yb) = (pair[0], pair[1]);
        if yb <= ya {
            continue;
        }
        while next < order.len() && edges[order[next]].y0 <= ya {
            active.push(order[next]);
            next += 1;
        }
        active.retain(|&index| edges[index].y1 > ya);
        let ym = (ya + yb) / 2.0;
        let mut crossing: Vec<(f64, usize)> = active
            .iter()
            .filter(|&&index| edges[index].y1 >= yb)
            .map(|&index| (edges[index].x_at(ym), index))
            .collect();
        crossing.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut continued: HashMap<(usize, usize), usize> = HashMap::new();
        for couple in crossing.chunks(2).filter(|couple| couple.len() == 2) {
            let (left, right) = (&edges[couple[0].1], &edges[couple[1].1]);
            let key = (couple[0].1, couple[1].1);
            let (la, ra, lb, rb) = (left.x_at(ya), right.x_at(ya), left.x_at(yb), right.x_at(yb));
            if ra - la <= 0.0 && rb - lb <= 0.0 {
                continue;
            }
            match open.get(&key) {
                Some(&index) if out[index][5] == ya => {
                    out[index][4] = rb;
                    out[index][5] = yb;
                    out[index][6] = lb;
                    out[index][7] = yb;
                    continued.insert(key, index);
                }
                _ => {
                    out.push([la, ya, ra, ya, rb, yb, lb, yb]);
                    continued.insert(key, out.len() - 1);
                }
            }
        }
        open = continued;
    }
    (out, complete)
}

impl<'a> Mapper<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        let blank = JwwDocument::new();
        let mut pen_rgb = [None; 10];
        if let Some(colors) = &blank.pen_colors {
            for (slot, color) in pen_rgb.iter_mut().zip(colors).skip(1) {
                *slot = rgb_of(color);
            }
        }
        let extended = blank.extended_colors.clone().unwrap_or_else(|| vec![String::new(); 257]);
        let default_line_types =
            (2..=9u8).chain(32..=45).map(|style| (style, blank.line_dashes(style))).filter(|(_, d)| !d.is_empty());
        Mapper {
            doc,
            unit: drawing_unit(doc),
            order: draw_order(doc),
            notices: NoticeList::new(),
            layers_upper: doc.layers.iter().map(|layer| (layer.name.to_uppercase(), layer)).collect(),
            linetypes_upper: doc.linetypes.iter().map(|linetype| (linetype.name.to_uppercase(), linetype)).collect(),
            scales: Vec::new(),
            slots: Vec::new(),
            slot_index: HashMap::new(),
            top: Vec::new(),
            blocks: Vec::new(),
            block_names: HashSet::new(),
            next_block: 0,
            variants: HashMap::new(),
            inherits: HashMap::new(),
            pending: Vec::new(),
            paper: MODEL_PAPER,
            limit: [0.0; 4],
            pen_rgb,
            extended,
            user_colors: HashMap::new(),
            default_line_types: default_line_types.collect(),
            user_line_types: HashMap::new(),
            line_types: BTreeMap::new(),
            blank,
        }
    }

    fn root_inherit() -> Inherit {
        Inherit { paint: Paint::Foreground, lineweight: DEFAULT_LINEWEIGHT, linetype: "CONTINUOUS".into(), layer: None }
    }

    fn run(mut self) -> MappedJww {
        let doc = self.doc;
        let view = self.default_view();
        let layout = doc.layouts.iter().find(|layout| !layout.model && layout.name == view);
        match layout {
            Some(layout) => self.layout(layout),
            None => self.model_sheet(),
        }
        let others = doc
            .layouts
            .iter()
            .filter(|other| !other.model && Some(other.name.as_str()) != layout.map(|l| l.name.as_str()));
        let others = others
            .filter(|other| self.order.blocks.get(&other.block.to_uppercase()).is_some_and(|list| !list.is_empty()))
            .count();
        if others > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "layouts",
                "JWW の用紙は 1 枚なので、開くレイアウトのほかのレイアウトは書いていません",
                others as u64,
            );
        }
        self.finish()
    }

    /// 表示（dxf_to_scene）の既定の表示と同じ空間
    fn default_view(&self) -> String {
        let doc = self.doc;
        let has_content = |layout: &DxfLayout| {
            self.order.blocks.get(&layout.block.to_uppercase()).is_some_and(|entities| !entities.is_empty())
        };
        let paper: Vec<&DxfLayout> = doc.layouts.iter().filter(|layout| !layout.model).collect();
        if doc.header.tilemode == 0.0
            && let Some(active) = paper.iter().find(|layout| layout.active)
            && has_content(active)
        {
            return active.name.clone();
        }
        if !self.order.model.is_empty() {
            return "Model".into();
        }
        paper.iter().find(|layout| has_content(layout)).map_or_else(|| "Model".into(), |layout| layout.name.clone())
    }

    /// 縮尺の分母の番号
    fn scale_index(&mut self, scale: f64) -> usize {
        let scale = if scale.is_finite() && scale > 0.0 { tidy_scale(scale) } else { 1.0 };
        if let Some(index) = self.scales.iter().position(|value| (value - scale).abs() <= 1e-9 * value.max(scale)) {
            return index;
        }
        self.scales.push(scale);
        self.scales.len() - 1
    }

    fn root_ctx(m: Matrix, scale: usize) -> Ctx {
        Ctx {
            m,
            to_paper: IDENTITY,
            dash_factor: 1.0,
            inherit: Self::root_inherit(),
            stack: Vec::new(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
            scale,
        }
    }

    // --- 用紙 ------------------------------------------------------------------------------------------

    /// モデル空間を表示する図面: モデル空間を、A3 に収まる縮尺のレイヤグループに置く
    fn model_sheet(&mut self) {
        let scene = dxf_to_scene(self.doc, Some("Model"));
        let shift = scene
            .entities
            .first()
            .and_then(|entity| entity.placement.as_ref())
            .map_or([0.0, 0.0], |placement| [placement.inverse[4], placement.inverse[5]]);
        self.paper = MODEL_PAPER;
        let (_, width, height) = paper_size(MODEL_PAPER).unwrap_or(("A3", 420.0, 297.0));
        let b = scene.bounds;
        let (scale, offset) = if b.min_x <= b.max_x && b.min_y <= b.max_y {
            // 表示の座標 = 実寸 − 原点の寄せ
            let (x0, y0, x1, y1) = (b.min_x + shift[0], b.min_y + shift[1], b.max_x + shift[0], b.max_y + shift[1]);
            let scale = model_scale(x1 - x0, y1 - y0, width * MODEL_FILL, height * MODEL_FILL);
            let fits = |low: f64, high: f64, half: f64| low / scale >= -half && high / scale <= half;
            let offset = if fits(x0, x1, width / 2.0) && fits(y0, y1, height / 2.0) {
                [0.0, 0.0]
            } else {
                // 用紙の 10 mm 刻みに丸めた、範囲の中心
                let step = scale * 10.0;
                [((x0 + x1) / 2.0 / step).round() * step, ((y0 + y1) / 2.0 / step).round() * step]
            };
            self.limit = [
                (x0 - offset[0]) / scale,
                (y0 - offset[1]) / scale,
                (x1 - offset[0]) / scale,
                (y1 - offset[1]) / scale,
            ];
            (scale, offset)
        } else {
            self.limit = [-width / 2.0, -height / 2.0, width / 2.0, height / 2.0];
            (1.0, [0.0, 0.0])
        };
        self.notices.add(
            NoticeLevel::Info,
            "model-sheet",
            format!(
                "レイアウトを開かない図面は、モデル空間を A3 の用紙に収まる縮尺（1/{}）のレイヤグループに置きました",
                js::number_to_string(scale)
            ),
        );
        if offset != [0.0, 0.0] {
            self.notices.add(
                NoticeLevel::Info,
                "model-moved",
                format!(
                    "モデル空間の図形を用紙の中心へ寄せたので、JWW の実寸の座標は元の座標から ({}, {}) mm ずれています",
                    js::number_to_string(-offset[0]),
                    js::number_to_string(-offset[1])
                ),
            );
        }
        let unit = self.unit;
        let index = self.scale_index(scale);
        let ctx = Self::root_ctx([unit / scale, 0.0, 0.0, unit / scale, -offset[0] / scale, -offset[1] / scale], index);
        let model = self.order.model.clone();
        let mut items = Vec::new();
        for entity in &model {
            self.emit(entity, &ctx, &mut items);
        }
        self.top = items;
    }

    /// レイアウトを開く図面: 用紙の中心を JWW の用紙の中心に置き、紙空間は 1/1、ビューポートはその縮尺のレイヤグループ
    fn layout(&mut self, layout: &DxfLayout) {
        let doc = self.doc;
        // 表示と同じ用紙の大きさと、紙空間の座標 → 用紙の mm（scene.rs の dxf_to_scene）
        let paper_unit = (if layout.paper_units == 0.0 { 25.4 } else { 1.0 })
            * (if layout.plot_scale[1] > 0.0 && layout.plot_scale[0] > 0.0 {
                layout.plot_scale[0] / layout.plot_scale[1]
            } else {
                1.0
            });
        let rotated = layout.plot_rotation == 1.0 || layout.plot_rotation == 3.0;
        let (mut left, mut bottom) = (layout.margins[0], layout.margins[1]);
        if rotated {
            (left, bottom) = (bottom, left);
        }
        let mut width = if rotated { layout.paper_height } else { layout.paper_width };
        let mut height = if rotated { layout.paper_width } else { layout.paper_height };
        let mut ox = left + layout.plot_origin[0];
        let mut oy = bottom + layout.plot_origin[1];
        if !(width > 0.0 && height > 0.0) {
            let [x0, y0] = layout.limmin;
            let [x1, y1] = layout.limmax;
            width = (x1 - x0) * paper_unit;
            height = (y1 - y0) * paper_unit;
            ox = -x0 * paper_unit;
            oy = -y0 * paper_unit;
        }
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            width = 1.0;
            height = 1.0;
        }
        let (code, exact) = paper_code(width, height);
        self.paper = code;
        if !exact {
            let (label, _, _) = paper_size(code).unwrap_or(("5A", 0.0, 0.0));
            self.notices.add(
                NoticeLevel::Info,
                "paper-size",
                format!(
                    "{}×{} mm の用紙は JWW にないため、それを含む {label} の用紙の中心に置きました",
                    js::format_number(width, 1),
                    js::format_number(height, 1)
                ),
            );
        }
        // 紙空間の座標 → 用紙の mm → JWW の座標（用紙の中心が原点）
        let sheet =
            multiply(&[1.0, 0.0, 0.0, 1.0, -width / 2.0, -height / 2.0], &[paper_unit, 0.0, 0.0, paper_unit, ox, oy]);
        self.limit = [-width / 2.0, -height / 2.0, width / 2.0, height / 2.0];
        let content = self.order.blocks.get(&layout.block.to_uppercase()).cloned().unwrap_or_default();
        let viewports: Vec<usize> = content
            .iter()
            .enumerate()
            .filter(|(_, entity)| matches!(entity.data, DxfEntityData::Viewport(_)))
            .map(|(index, _)| index)
            .collect();
        let overall = viewports
            .iter()
            .copied()
            .find(|index| matches!(&content[*index].data, DxfEntityData::Viewport(viewport) if viewport.id == 1.0))
            .or(viewports.first().copied());
        let model_scene = if viewports.len() > usize::from(overall.is_some()) {
            Some(dxf_to_scene(doc, Some("Model")))
        } else {
            None
        };
        let paper_scale = self.scale_index(1.0);
        let sheet_ctx = Self::root_ctx(sheet, paper_scale);
        let mut shown: HashSet<String> = HashSet::new();
        let mut items = Vec::new();
        for (index, entity) in content.iter().enumerate() {
            let DxfEntityData::Viewport(viewport) = &entity.data else {
                self.emit(entity, &sheet_ctx, &mut items);
                continue;
            };
            if Some(index) == overall || viewport.status <= 0.0 || js::to_int32(viewport.status_flags) & 0x20000 != 0 {
                continue;
            }
            if let Some(scene) = &model_scene {
                self.viewport(&entity.base, viewport, &sheet, scene, &mut items, &mut shown);
            }
        }
        if model_scene.is_some() {
            let hidden = self.order.model.iter().filter(|entity| !shown.contains(&entity.base.handle)).count();
            if hidden > 0 {
                self.notices.add_count(
                    NoticeLevel::Info,
                    "outside-viewports",
                    "どのビューポートにも映らないモデル空間の図形は書いていません",
                    hidden as u64,
                );
            }
        }
        self.top = items;
    }

    /// ビューポートに映るモデル空間の図形を、ビューポートの縮尺のレイヤグループに、用紙に映る位置で置く
    fn viewport(
        &mut self,
        base: &DxfEntityBase,
        viewport: &DxfViewport,
        sheet: &Matrix,
        model_scene: &zumen_core::Scene,
        items: &mut Vec<Item>,
        shown: &mut HashSet<String>,
    ) {
        let [vx, vy, vz] = viewport.view_direction;
        let length = js::hypot(&[vx, vy, vz]);
        if length > 0.0 && (vx.abs() > 1e-9 * length || vy.abs() > 1e-9 * length || vz < 0.0) {
            self.notices.add(
                NoticeLevel::Info,
                "viewport-view",
                "上から見た向きでないビューポート（3D の表示）は書いていません",
            );
            return;
        }
        if !(viewport.view_height > 0.0 && viewport.height > 0.0) {
            return;
        }
        let s = viewport.height / viewport.view_height;
        let (cx, cy) = (viewport.center[0], viewport.center[1]);
        self.viewport_frame(base, viewport, sheet, items);
        let (sin, cos) = viewport.view_twist_angle.to_radians().sin_cos();
        // 表示と同じ変換: 中心 − 縮尺 × 表示の中心 + 縮尺 × R(ねじれ) × (モデル − 目標点)
        let model = multiply(
            &[cos * s, sin * s, -sin * s, cos * s, cx - viewport.view_center[0] * s, cy - viewport.view_center[1] * s],
            &[1.0, 0.0, 0.0, 1.0, -viewport.view_target[0], -viewport.view_target[1]],
        );
        let m_total = multiply(sheet, &model);
        let unit = self.unit;
        // モデルの mm → JWW の座標
        let to_paper = multiply(&m_total, &[1.0 / unit, 0.0, 0.0, 1.0 / unit, 0.0, 0.0]);
        let factor = length_scale(&to_paper);
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let (hw, hh) = (viewport.width / 2.0, viewport.height / 2.0);
        let (x0, y0) = apply(sheet, cx - hw, cy - hh);
        let (x1, y1) = apply(sheet, cx + hw, cy + hh);
        let clip = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
        let frozen: HashSet<String> = viewport
            .frozen_layer_handles
            .iter()
            .filter_map(|handle| self.doc.layers.iter().find(|layer| layer.handle == *handle))
            .map(|layer| layer.name.to_uppercase())
            .collect();
        // モデル空間の表示の外接矩形（表示の座標 = 実寸 mm − 原点の寄せ）を、図形ごとに
        let mut boxes: HashMap<&str, zumen_core::BBox> = HashMap::new();
        let shift = model_scene
            .entities
            .first()
            .and_then(|entity| entity.placement.as_ref())
            .map_or([0.0, 0.0], |p| [p.inverse[4], p.inverse[5]]);
        for entity in &model_scene.entities {
            let slot = boxes.entry(top_id(&entity.id)).or_insert_with(zumen_core::geometry::empty_box);
            zumen_core::geometry::union_box(slot, &entity.bbox);
        }
        let model_entities = self.order.model.clone();
        let mut members: Vec<&DxfEntity> = Vec::new();
        let mut clipped = 0;
        for entity in &model_entities {
            if frozen.contains(&entity.base.layer.to_uppercase()) {
                continue;
            }
            let Some(bbox) = boxes.get(entity.base.handle.as_str()) else {
                continue;
            };
            let mut paper = zumen_core::geometry::empty_box();
            for (x, y) in
                [(bbox.min_x, bbox.min_y), (bbox.max_x, bbox.min_y), (bbox.max_x, bbox.max_y), (bbox.min_x, bbox.max_y)]
            {
                let (px, py) = apply(&to_paper, x + shift[0], y + shift[1]);
                zumen_core::geometry::extend_box(&mut paper, px, py);
            }
            if paper.max_x < clip[0] || paper.min_x > clip[2] || paper.max_y < clip[1] || paper.min_y > clip[3] {
                continue;
            }
            let tolerance = 1e-6 * (clip[2] - clip[0]).max(clip[3] - clip[1]).max(1.0);
            if paper.min_x < clip[0] - tolerance
                || paper.max_x > clip[2] + tolerance
                || paper.min_y < clip[1] - tolerance
                || paper.max_y > clip[3] + tolerance
            {
                clipped += 1;
            }
            members.push(entity);
            shown.insert(entity.base.handle.clone());
        }
        if clipped > 0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "viewport-clip",
                "ビューポートの枠からはみ出す図形は、JWW では切り取れないため、はみ出したまま書きました",
                clipped,
            );
        }
        if members.is_empty() {
            return;
        }
        // 実寸の座標（JWW の座標 × 縮尺）がモデル空間の座標と同じになるのは、モデルの原点が用紙の中心に映り、
        // ねじれのないビューポートだけ
        let (ox, oy) = apply(&to_paper, 0.0, 0.0);
        if ox.abs() > 1e-6 || oy.abs() > 1e-6 || normalize(viewport.view_twist_angle) != 0.0 {
            self.notices.add_count(
                NoticeLevel::Info,
                "viewport-moved",
                "ビューポートの図形は用紙に映る位置・向きに置いたので、その実寸の座標はモデル空間の座標と違います",
                1,
            );
        }
        let scale = self.scale_index(1.0 / factor);
        let dash_factor = if self.doc.header.psltscale == 1.0 { 1.0 / s } else { 1.0 };
        let ctx = Ctx {
            dash_factor,
            prefix: format!("{}:", base.handle),
            clip: Some(clip),
            ..Self::root_ctx(m_total, scale)
        };
        for entity in &members {
            self.emit(entity, &ctx, items);
        }
    }

    /// ビューポートの枠（表示と同じ）。印刷しない・表示しないレイヤの枠は、印刷に出ないので書かない
    fn viewport_frame(&mut self, base: &DxfEntityBase, viewport: &DxfViewport, sheet: &Matrix, items: &mut Vec<Item>) {
        if let Some(layer) = self.layers_upper.get(&base.layer.to_uppercase())
            && (!layer.plot || layer.off || layer.frozen)
        {
            return;
        }
        let paper_scale = self.scale_index(1.0);
        let ctx = Self::root_ctx(*sheet, paper_scale);
        let (style, slot) = self.style(base, &ctx, sheet);
        let (cx, cy, hw, hh) = (viewport.center[0], viewport.center[1], viewport.width / 2.0, viewport.height / 2.0);
        let corners: Vec<f64> = [(cx - hw, cy - hh), (cx + hw, cy - hh), (cx + hw, cy + hh), (cx - hw, cy + hh)]
            .iter()
            .flat_map(|(x, y)| {
                let (px, py) = apply(sheet, *x, *y);
                [px, py]
            })
            .collect();
        for [x1, y1, x2, y2] in segments_of(&corners, true, false) {
            items.push(Item::entity(
                JwwEntity::Line(JwwLine { base: style, x1, y1, x2, y2 }),
                slot,
                base.handle.clone(),
            ));
        }
    }

    // --- レイヤ・色・線種・線幅 ----------------------------------------------------------------------

    /// DXF のレイヤを置く場所（縮尺とレイヤ）の番号
    fn slot(&mut self, scale: usize, layer: &str) -> usize {
        let upper = layer.to_uppercase();
        let key = (scale, upper.clone());
        if let Some(index) = self.slot_index.get(&key) {
            return *index;
        }
        let name = self.layers_upper.get(&upper).map_or_else(|| layer.to_string(), |layer| layer.name.clone());
        self.slots.push(Slot { scale, name, upper });
        self.slot_index.insert(key, self.slots.len() - 1);
        self.slots.len() - 1
    }

    /// 線・点・文字の線色
    fn pen_color(&mut self, paint: Paint) -> u16 {
        let rgb = match paint {
            Paint::Foreground => return 2,
            Paint::Background => [255, 255, 255],
            Paint::Rgb(rgb) => rgb,
        };
        if let Some(pen) = (1..=9).find(|&pen| self.pen_rgb[pen] == Some(rgb)) {
            return pen as u16;
        }
        if let Some(index) = SXF_COLORS.iter().position(|color| *color == rgb) {
            return 101 + index as u16;
        }
        if let Some(pen) = self.user_colors.get(&rgb) {
            return *pen;
        }
        let pen = *USER_COLORS.start() + self.user_colors.len() as u16;
        if USER_COLORS.contains(&pen) {
            self.user_colors.insert(rgb, pen);
            if let Some(slot) = self.extended.get_mut(usize::from(pen - 100)) {
                *slot = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            }
            return pen;
        }
        // ユーザー定義の色が足りない: いちばん近い色
        self.notices.add(
            NoticeLevel::Warning,
            "color-count",
            "SXF 対応色のユーザー定義（239 色）に入らない色は、いちばん近い色にしました",
        );
        let distance =
            |other: [u8; 3]| -> i32 { (0..3).map(|i| (i32::from(rgb[i]) - i32::from(other[i])).pow(2)).sum() };
        let candidates = (1..=9u16)
            .filter_map(|pen| self.pen_rgb[usize::from(pen)].map(|color| (pen, color)))
            .chain(SXF_COLORS.iter().enumerate().map(|(index, color)| (101 + index as u16, *color)))
            .chain(self.user_colors.iter().map(|(color, pen)| (*pen, *color)));
        candidates.min_by_key(|(pen, color)| (distance(*color), *pen)).map_or(2, |(pen, _)| pen)
    }

    /// ソリッドの線色と任意色（線色 10 の COLORREF）
    fn solid_color(&self, paint: Paint) -> (u16, Option<u32>) {
        match paint {
            Paint::Foreground => (2, None),
            Paint::Background => (10, Some(colorref([255, 255, 255]))),
            Paint::Rgb(rgb) => match (1..=9).find(|&pen| self.pen_rgb[pen] == Some(rgb)) {
                Some(pen) => (pen as u16, None),
                None => (10, Some(colorref(rgb))),
            },
        }
    }

    /// 線種（線分・空白の長さは用紙の mm）
    fn pen_style(&mut self, dashes: &[f64]) -> u8 {
        // 用紙で周期のごく短い破線は、実線と見分けがつかないので実線にする
        if dashes.iter().map(|value| value.abs()).sum::<f64>() < MIN_PERIOD {
            return 1;
        }
        let mut pitch = self.pitch(dashes);
        if pitch.is_empty() {
            return 1;
        }
        let same = |a: &[f64], b: &[f64]| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        if let Some((style, _)) = self.default_line_types.iter().find(|(_, segments)| same(segments, &pitch)) {
            return *style;
        }
        if pitch.len() > MAX_SEGMENTS {
            self.notices.add(
                NoticeLevel::Info,
                "linetype-long",
                "線分・空白が 10 を超える線種は、JWW の線種に書けないため、初めの 10 だけにしました",
            );
            pitch.truncate(MAX_SEGMENTS);
        }
        let key: Vec<u64> = pitch.iter().map(|value| value.to_bits()).collect();
        if let Some(style) = self.user_line_types.get(&key) {
            return *style;
        }
        let style = *USER_LINE_TYPES.start() + self.user_line_types.len() as u8;
        if USER_LINE_TYPES.contains(&style) {
            self.user_line_types.insert(key, style);
            self.line_types.insert(u32::from(style), pitch);
            return style;
        }
        self.notices.add(
            NoticeLevel::Warning,
            "linetype-count",
            "SXF 対応線種のユーザー定義（16 種）に入らない線種は、周期と線の割合のいちばん近い線種にしました",
        );
        // 周期の比・線の割合・線分の数の近いもの
        let signature = |segments: &[f64]| {
            let period: f64 = segments.iter().sum();
            let on: f64 = segments.iter().step_by(2).sum();
            (period, on / period, segments.len() as f64)
        };
        let (period, duty, count) = signature(&pitch);
        let distance = |segments: &[f64]| {
            let (other, other_duty, other_count) = signature(segments);
            (other / period).ln().abs() + (other_duty - duty).abs() * 2.0 + (other_count - count).abs() * 0.1
        };
        let users = self.line_types.iter().map(|(style, segments)| (*style as u8, segments));
        let candidates = self.default_line_types.iter().map(|(style, segments)| (*style, segments)).chain(users);
        candidates.min_by(|a, b| distance(a.1).total_cmp(&distance(b.1))).map_or(1, |(style, _)| style)
    }

    /// 表示の線分・空白の並び（dash_array）を、正の線分と空白の組にする。長さ 0 の線分（点）は 0.25 mm の線分にし、
    /// 続く空白をその分だけ短くする
    fn pitch(&mut self, dashes: &[f64]) -> Vec<f64> {
        let mut out = dashes.to_vec();
        if out.len() % 2 == 1 {
            out.push(0.0);
        }
        let mut dotted = false;
        for i in (0..out.len()).step_by(2) {
            if out[i] <= 0.0 {
                dotted = true;
                out[i] = DOT;
                if out[i + 1] > DOT * 2.0 {
                    out[i + 1] -= DOT;
                }
            }
            if out[i + 1] <= 0.0 {
                out[i + 1] = DOT;
            }
        }
        if dotted {
            self.notices.add(
                NoticeLevel::Info,
                "linetype-dot",
                "線種の点（長さ 0 の線分）は、JWW の線種に書けないため 0.25 mm の線分にしました",
            );
        }
        if out.iter().any(|value| !(value.is_finite() && *value > 0.0)) {
            return Vec::new();
        }
        out
    }

    /// 線幅（1/100 mm、1 以上）
    fn pen_width(width: f64) -> u16 {
        let value = (width * 100.0).round();
        if value.is_finite() && value >= 1.0 { value.min(f64::from(u16::MAX)) as u16 } else { 1 }
    }

    // --- 見え方（表示の color・lineweight・linetype_name と同じ規則） ------------------------------------

    fn effective_layer(&self, base: &DxfEntityBase, ctx: &Ctx) -> String {
        match &ctx.inherit.layer {
            Some(parent) if base.layer == "0" => parent.clone(),
            _ => base.layer.clone(),
        }
    }

    fn paint(&mut self, base: &DxfEntityBase, ctx: &Ctx) -> Paint {
        let transparency = base.transparency.or_else(|| {
            self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()).and_then(|layer| layer.transparency)
        });
        if is_transparent(transparency) {
            self.notices.add(NoticeLevel::Info, "transparency", "透過度は JWW に書けないため、不透明で書きました");
        }
        if let Some(value) = base.true_color {
            return paint_of_hex(&true_color(value)).with_white();
        }
        let index = base.color_index;
        if index == 0.0 {
            return ctx.inherit.paint;
        }
        if index == 256.0 || index == 257.0 || !(0.0..=257.0).contains(&index) {
            return match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) => match layer.true_color {
                    Some(value) => paint_of_hex(&true_color(value)).with_white(),
                    None => paint_of_aci(layer.color_index),
                },
                None => Paint::Foreground,
            };
        }
        paint_of_aci(index)
    }

    fn lineweight(&self, base: &DxfEntityBase, ctx: &Ctx) -> f64 {
        let value = base.lineweight;
        if value == -2.0 {
            return ctx.inherit.lineweight;
        }
        if value == -1.0 {
            return match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) if layer.lineweight >= 0.0 => layer.lineweight / 100.0,
                _ => DEFAULT_LINEWEIGHT,
            };
        }
        if value >= 0.0 { value / 100.0 } else { DEFAULT_LINEWEIGHT }
    }

    fn linetype_name(&self, base: &DxfEntityBase, ctx: &Ctx) -> String {
        let name = if base.linetype.is_empty() { "BYLAYER" } else { base.linetype.as_str() };
        match name.to_uppercase().as_str() {
            "BYBLOCK" => ctx.inherit.linetype.clone(),
            "BYLAYER" => match self.layers_upper.get(&self.effective_layer(base, ctx).to_uppercase()) {
                Some(layer) if !layer.linetype.is_empty() => layer.linetype.clone(),
                _ => "CONTINUOUS".into(),
            },
            _ => name.to_string(),
        }
    }

    /// 線種の線分・空白の長さ（用紙の mm）。m は図形の座標 → 入れ物の座標
    fn dashes(&mut self, base: &DxfEntityBase, ctx: &Ctx, m: &Matrix) -> Vec<f64> {
        let name = self.linetype_name(base, ctx).to_uppercase();
        if matches!(name.as_str(), "CONTINUOUS" | "BYLAYER" | "BYBLOCK") {
            return Vec::new();
        }
        let Some(linetype) = self.linetypes_upper.get(&name).copied() else {
            return Vec::new();
        };
        if linetype.complex {
            self.notices.add(NoticeLevel::Info, "complex-linetype", "文字・形を含む線種は、線と空白だけで書きました");
        }
        let factor =
            self.doc.header.ltscale * base.linetype_scale * length_scale(&multiply(&ctx.to_paper, m)) * ctx.dash_factor;
        dash_array(&linetype.dashes, factor)
    }

    /// 線の図形の共通の値（線種・線色・線幅）と、レイヤを置く場所
    fn style(&mut self, base: &DxfEntityBase, ctx: &Ctx, m: &Matrix) -> (JwwBase, usize) {
        let layer = self.effective_layer(base, ctx);
        let slot = self.slot(ctx.scale, &layer);
        let paint = self.paint(base, ctx);
        let pen_color = self.pen_color(paint);
        let dashes = self.dashes(base, ctx, m);
        let pen_style = self.pen_style(&dashes);
        let pen_width = Self::pen_width(self.lineweight(base, ctx));
        (JwwBase { pen_style, pen_color, pen_width, ..JwwBase::default() }, slot)
    }

    /// ブロック参照・寸法の中の図形が継ぐ値
    fn child_inherit(&mut self, base: &DxfEntityBase, ctx: &Ctx) -> Inherit {
        Inherit {
            paint: self.paint(base, ctx),
            lineweight: self.lineweight(base, ctx),
            linetype: self.linetype_name(base, ctx),
            layer: Some(self.effective_layer(base, ctx)),
        }
    }

    // --- 図形 ------------------------------------------------------------------------------------------

    fn emit(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        if zumen_core::limits::tick() {
            return;
        }
        let base = &entity.base;
        if base.invisible {
            return;
        }
        let source = ctx.source(base);
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                let (style, slot) = self.style(base, ctx, &ctx.m);
                let (x1, y1) = apply(&ctx.m, start[0], start[1]);
                let (x2, y2) = apply(&ctx.m, end[0], end[1]);
                items.push(Item::entity(JwwEntity::Line(JwwLine { base: style, x1, y1, x2, y2 }), slot, source));
            }
            DxfEntityData::Circle { center, radius } => {
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), center[2]));
                let shape = (radius.is_finite() && *radius > 0.0)
                    .then(|| arc_shape(&m, [center[0], center[1]], [*radius, 0.0], [0.0, *radius], 0.0, TAU, true))
                    .flatten();
                match shape {
                    Some(shape) => self.push_arc(base, ctx, &m, shape, items, source),
                    None => self.pend(entity, ctx, items),
                }
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), center[2]));
                // 表示と同じ円弧角（0 は 360°）
                let mut sweep = (end_angle - start_angle).rem_euclid(360.0);
                if sweep < 1e-9 {
                    sweep = 360.0;
                }
                let shape = (radius.is_finite() && *radius > 0.0)
                    .then(|| {
                        let r = *radius;
                        arc_shape(
                            &m,
                            [center[0], center[1]],
                            [r, 0.0],
                            [0.0, r],
                            start_angle.to_radians(),
                            sweep.to_radians(),
                            false,
                        )
                    })
                    .flatten();
                match shape {
                    Some(shape) => self.push_arc(base, ctx, &m, shape, items, source),
                    None => self.pend(entity, ctx, items),
                }
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                let minor = minor_axis(major_axis, *ratio, base.extrusion.as_ref());
                let full =
                    (end_param - start_param).abs() < 1e-9 || ((end_param - start_param).abs() - TAU).abs() < 1e-9;
                let mut sweep = (end_param - start_param).rem_euclid(TAU);
                if sweep < 1e-9 {
                    sweep = TAU;
                }
                let shape = arc_shape(
                    &ctx.m,
                    [center[0], center[1]],
                    [major_axis[0], major_axis[1]],
                    [minor[0], minor[1]],
                    if full { 0.0 } else { *start_param },
                    sweep,
                    full,
                );
                match shape {
                    Some(shape) => self.push_arc(base, ctx, &ctx.m, shape, items, source),
                    None => self.pend(entity, ctx, items),
                }
            }
            DxfEntityData::LwPolyline(polyline) => {
                let widths = polyline.const_width.is_some_and(|width| width != 0.0)
                    || polyline.vertices.widths.iter().any(|width| width[0] != 0.0 || width[1] != 0.0);
                if widths {
                    self.notices.add(
                        NoticeLevel::Info,
                        "polyline-width",
                        "幅のあるポリラインは、表示と同じ塗り（ソリッド）にしました",
                    );
                    return self.pend(entity, ctx, items);
                }
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), polyline.elevation.unwrap_or(0.0)));
                self.polyline(base, ctx, &m, &polyline.vertices, items, &source);
            }
            DxfEntityData::Polyline(polyline) => {
                let flags = js::to_int32(polyline.vertices.flags);
                let mesh = polyline.faces.is_some() || polyline.m_count.is_some() || flags & (16 | 64) != 0;
                let fitted = flags & (2 | 4) != 0;
                let three_d = flags & 8 != 0;
                let widths = polyline.vertices.widths.iter().any(|width| width[0] != 0.0 || width[1] != 0.0);
                if mesh || fitted || (widths && !three_d) {
                    if widths && !three_d {
                        self.notices.add(
                            NoticeLevel::Info,
                            "polyline-width",
                            "幅のあるポリラインは、表示と同じ塗り（ソリッド）にしました",
                        );
                    }
                    return self.pend(entity, ctx, items);
                }
                let elevation = polyline.vertices.points.first().map_or(0.0, |point| point[2]);
                let m = if three_d { ctx.m } else { multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), elevation)) };
                let mut vertices = polyline.vertices.clone();
                if three_d {
                    vertices.bulges = vec![0.0; vertices.points.len()];
                }
                self.polyline(base, ctx, &m, &vertices, items, &source);
            }
            DxfEntityData::Spline(spline) => {
                let weighted = spline.rational && spline.weights.iter().any(|weight| (weight - 1.0).abs() > 1e-12);
                if spline.degree == 1.0 && !weighted && spline.control_points.len() >= 2 && base.kind == "SPLINE" {
                    let (style, slot) = self.style(base, ctx, &ctx.m);
                    let points: Vec<f64> = spline
                        .control_points
                        .iter()
                        .flat_map(|point| {
                            let (x, y) = apply(&ctx.m, point[0], point[1]);
                            [x, y]
                        })
                        .collect();
                    for [x1, y1, x2, y2] in segments_of(&points, false, false) {
                        let line = JwwEntity::Line(JwwLine { base: style, x1, y1, x2, y2 });
                        items.push(Item::entity(line, slot, source.clone()));
                    }
                    return;
                }
                self.notices.add(
                    NoticeLevel::Info,
                    "spline",
                    "スプライン・らせんは、JWW にないため、表示と同じ折線（線分）にしました",
                );
                self.pend(entity, ctx, items);
            }
            DxfEntityData::Point { location, .. } => {
                let (mut style, slot) = self.style(base, ctx, &ctx.m);
                style.pen_style = 1;
                let (x, y) = apply(&ctx.m, location[0], location[1]);
                let point = JwwPoint { base: style, x, y, temporary: false, code: 0, angle: 0.0, scale: 0.0 };
                items.push(Item::entity(JwwEntity::Point(point), slot, source));
            }
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, .. } if base.kind != "3DFACE" => {
                // SOLID・TRACE は表示と同じ 1 → 2 → 4 → 3 の順の四角形（3 と 4 が同じなら三角形）
                let m = multiply(&ctx.m, &ocs_matrix(base.extrusion.as_ref(), vtx0[2]));
                let layer = self.effective_layer(base, ctx);
                let slot = self.slot(ctx.scale, &layer);
                let paint = self.paint(base, ctx);
                let (pen_color, color) = self.solid_color(paint);
                let at = |point: &[f64; 3]| apply(&m, point[0], point[1]);
                let (a, b, c, d) = (at(vtx0), at(vtx1), at(vtx3), at(vtx2));
                let last = if vtx2[0] == vtx3[0] && vtx2[1] == vtx3[1] { d } else { c };
                let points = [a.0, a.1, b.0, b.1, last.0, last.1, d.0, d.1];
                let style = JwwBase { pen_style: 1, pen_color, ..JwwBase::default() };
                items.push(Item::entity(JwwEntity::Solid(JwwSolid { base: style, points, color }), slot, source));
            }
            DxfEntityData::Insert(_) | DxfEntityData::Table(_) => self.insert(entity, ctx, items),
            DxfEntityData::Dimension(dimension) => {
                let Some(members) = self.order.blocks.get(&dimension.anonymous_block_name.to_uppercase()).cloned()
                else {
                    self.notices.add(NoticeLevel::Warning, "dimension-block", "表示のブロックがない寸法を省きました");
                    return;
                };
                if ctx.stack.len() >= MAX_DEPTH {
                    self.notices.add(NoticeLevel::Warning, "block-depth", "入れ子が深すぎるブロック参照を省きました");
                    return;
                }
                self.notices.add(
                    NoticeLevel::Info,
                    "dimension",
                    "寸法は、表示のブロックの線・塗り・文字にしました（JWW の寸法図形にはしていません）",
                );
                // 表示と同じく、ブロックの図形は寸法の OCS にあり、12 の位置へ動かす
                let ocs = ocs_matrix(base.extrusion.as_ref(), 0.0);
                let insert = dimension.insert.unwrap_or([0.0, 0.0, 0.0]);
                let (ix, iy) = apply(&ocs, insert[0], insert[1]);
                let local = multiply(&[1.0, 0.0, 0.0, 1.0, ix, iy], &ocs);
                let mut stack = ctx.stack.clone();
                stack.push(dimension.anonymous_block_name.to_uppercase());
                let child = Ctx {
                    m: multiply(&ctx.m, &local),
                    inherit: self.child_inherit(base, ctx),
                    stack,
                    flatten: true,
                    source: Some(source),
                    ..ctx.clone()
                };
                for member in &members {
                    self.emit(member, &child, items);
                }
            }
            DxfEntityData::Viewport(_) => {}
            DxfEntityData::XLine { start, unit_vector } => self.xline(entity, start, unit_vector, ctx, items),
            DxfEntityData::Image(_) if base.kind == "IMAGE" => {
                self.notices.add(
                    NoticeLevel::Info,
                    "image",
                    "画像（IMAGE）は、その画像のファイルを読まないため、表示と同じ枠だけを書きました",
                );
                self.pend(entity, ctx, items);
            }
            DxfEntityData::Attdef { attribute_flags, .. } if ctx.inherit.layer.is_some() => {
                // ブロックの中の属性定義は、定数（2）のものだけを描く（表示と同じ。値は配置の ATTRIB が持つ）
                if js::to_int32(*attribute_flags) & 2 != 0 {
                    self.pend(entity, ctx, items);
                }
            }
            _ => {
                if base.kind == "WIPEOUT" {
                    self.notices.add(NoticeLevel::Info, "wipeout", "ワイプアウトは、白のソリッドにしました");
                }
                if matches!(base.kind.as_str(), "LEADER" | "MULTILEADER" | "MLEADER" | "MLINE" | "3DFACE")
                    || matches!(entity.data, DxfEntityData::Hatch(_)) && base.kind == "MPOLYGON"
                {
                    self.notices.add(
                        NoticeLevel::Info,
                        "drawn-shapes",
                        "引出線・マルチラインなど JWW にない図形は、表示と同じ線・塗り・文字にしました",
                    );
                }
                if let DxfEntityData::Hatch(hatch) = &entity.data
                    && hatch.gradient.is_some()
                {
                    self.notices.add(NoticeLevel::Info, "gradient", "グラデーションは、表示と同じ色の塗りにしました");
                }
                self.pend(entity, ctx, items);
            }
        }
    }

    /// 円弧・円・楕円
    fn push_arc(
        &mut self,
        base: &DxfEntityBase,
        ctx: &Ctx,
        m: &Matrix,
        shape: ArcShape,
        items: &mut Vec<Item>,
        source: String,
    ) {
        let (style, slot) = self.style(base, ctx, m);
        items.push(Item::entity(JwwEntity::Arc(jww_arc(style, &shape)), slot, source));
    }

    /// 構築線・放射線は、表示と同じく用紙の範囲（ビューポートの中はその枠も）で切った線分にする
    fn xline(&mut self, entity: &DxfEntity, start: &[f64; 3], direction: &[f64; 3], ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let m = multiply(&ctx.to_paper, &ctx.m);
        let (px, py) = apply(&m, start[0], start[1]);
        let (dx, dy) = apply_vector(&m, direction[0], direction[1]);
        let length = dx.hypot(dy);
        if length.is_nan() || length <= 0.0 {
            return;
        }
        let (ux, uy) = (dx / length, dy / length);
        let b = self.limit;
        let reach = (b[2] - b[0]).hypot(b[3] - b[1]) * 2.0 + (px - b[0]).hypot(py - b[1]);
        let from = if base.kind == "XLINE" { -reach } else { 0.0 };
        let mut segment = [px + ux * from, py + uy * from, px + ux * reach, py + uy * reach];
        for rect in std::iter::once(b).chain(ctx.clip) {
            match clip_segment(segment, rect) {
                Some(clipped) => segment = clipped,
                None => return,
            }
        }
        self.notices.add(NoticeLevel::Info, "xline", "構築線・放射線は、表示と同じく用紙の範囲で切った線分にしました");
        let back = invert(&ctx.to_paper);
        let (x1, y1) = apply(&back, segment[0], segment[1]);
        let (x2, y2) = apply(&back, segment[2], segment[3]);
        let (style, slot) = self.style(base, ctx, &ctx.m);
        items.push(Item::entity(JwwEntity::Line(JwwLine { base: style, x1, y1, x2, y2 }), slot, ctx.source(base)));
    }

    /// 折線は、直線の区間を線、弧の膨らみの区間を円弧にする
    fn polyline(
        &mut self,
        base: &DxfEntityBase,
        ctx: &Ctx,
        m: &Matrix,
        vertices: &zumen_dxf::DxfVertexList,
        items: &mut Vec<Item>,
        source: &str,
    ) {
        let points: Vec<[f64; 2]> = vertices.points.iter().map(|point| [point[0], point[1]]).collect();
        if points.len() < 2 {
            return;
        }
        let count = if vertices.closed { points.len() } else { points.len() - 1 };
        let (style, slot) = self.style(base, ctx, m);
        for i in 0..count {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            let bulge = vertices.bulges.get(i).copied().unwrap_or(0.0);
            let arc = bulge_arc(a, b, bulge).and_then(|(center, radius, start, end, ccw)| {
                let t0 = if ccw { start } else { end }.to_radians();
                arc_shape(m, center, [radius, 0.0], [0.0, radius], t0, 4.0 * bulge.abs().atan(), false)
            });
            let entity = match arc {
                Some(shape) => JwwEntity::Arc(jww_arc(style, &shape)),
                None => {
                    let (x1, y1) = apply(m, a[0], a[1]);
                    let (x2, y2) = apply(m, b[0], b[1]);
                    if x1 == x2 && y1 == y2 {
                        continue;
                    }
                    JwwEntity::Line(JwwLine { base: style, x1, y1, x2, y2 })
                }
            };
            items.push(Item::entity(entity, slot, source.to_string()));
        }
    }

    /// 線種が破線か
    fn linetype_dashed(&self, name: &str) -> bool {
        self.linetypes_upper
            .get(&name.to_uppercase())
            .is_some_and(|linetype| linetype.dashes.iter().any(|value| *value != 0.0))
    }

    /// ブロックが破線の図形を持つか（BYLAYER はその図形のレイヤの線種）
    fn dashed(&mut self, name: &str, depth: usize) -> bool {
        let key = format!("\u{0}dashed:{}", name.to_uppercase());
        if let Some(value) = self.inherits.get(&key) {
            return *value;
        }
        if depth > MAX_DEPTH {
            return false;
        }
        self.inherits.insert(key.clone(), false);
        let members = self.order.blocks.get(&name.to_uppercase()).cloned().unwrap_or_default();
        let mut value = false;
        for member in members {
            let linetype = match member.base.linetype.to_uppercase().as_str() {
                "" | "BYLAYER" => {
                    self.layers_upper.get(&member.base.layer.to_uppercase()).map(|layer| layer.linetype.clone())
                }
                _ => Some(member.base.linetype.clone()),
            };
            let child = match &member.data {
                DxfEntityData::Insert(insert) => Some(insert.name.clone()),
                DxfEntityData::Table(table) => Some(table.name.clone()),
                DxfEntityData::Dimension(dimension) => Some(dimension.anonymous_block_name.clone()),
                _ => None,
            };
            if linetype.is_some_and(|linetype| self.linetype_dashed(&linetype))
                || child.is_some_and(|child| self.dashed(&child, depth + 1))
            {
                value = true;
                break;
            }
        }
        self.inherits.insert(key, value);
        value
    }

    /// ブロックが、配置の値（BYBLOCK・レイヤ 0）を継ぐ図形を持つか
    fn inherits(&mut self, name: &str, depth: usize) -> bool {
        let key = name.to_uppercase();
        if let Some(value) = self.inherits.get(&key) {
            return *value;
        }
        if depth > MAX_DEPTH {
            return true;
        }
        self.inherits.insert(key.clone(), false);
        let members = self.order.blocks.get(&key).cloned().unwrap_or_default();
        let mut value = false;
        for member in members {
            let base = &member.base;
            if (base.color_index == 0.0 && base.true_color.is_none())
                || base.lineweight == -2.0
                || base.linetype.eq_ignore_ascii_case("BYBLOCK")
                || base.layer == "0"
            {
                value = true;
                break;
            }
            let child = match &member.data {
                DxfEntityData::Insert(insert) => Some(insert.name.clone()),
                DxfEntityData::Table(table) => Some(table.name.clone()),
                DxfEntityData::Dimension(dimension) => Some(dimension.anonymous_block_name.clone()),
                _ => None,
            };
            if child.is_some_and(|child| self.inherits(&child, depth + 1)) {
                value = true;
                break;
            }
        }
        self.inherits.insert(key, value);
        value
    }

    /// ブロック参照は JWW のブロックの配置（寸法の中と、ゆがんだ配置は、ブロックの図形として写す）。配列（MINSERT）は
    /// 配置を並べる
    fn insert(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let (name, point, xscale, yscale, rotation, insert) = match &entity.data {
            DxfEntityData::Insert(insert) => {
                (&insert.name, insert.insert, insert.xscale, insert.yscale, insert.rotation, Some(insert))
            }
            DxfEntityData::Table(table) => {
                (&table.name, table.insert, table.xscale, table.yscale, table.rotation, None)
            }
            _ => return,
        };
        let key = name.to_uppercase();
        let Some(members) = self.order.blocks.get(&key).cloned() else {
            self.notices.add(NoticeLevel::Warning, "missing-block", "定義の見つからないブロック参照を省きました");
            return;
        };
        if ctx.stack.len() >= MAX_DEPTH || ctx.stack.contains(&key) {
            self.notices.add(NoticeLevel::Warning, "block-depth", "循環する・入れ子が深すぎるブロック参照を省きました");
            return;
        }
        let block = self.doc.blocks.iter().find(|block| block.name.eq_ignore_ascii_case(name));
        let base_point = block.map_or([0.0; 3], |block| block.base_point);
        let ocs = ocs_matrix(base.extrusion.as_ref(), point[2]);
        let columns = insert.map_or(1.0, |insert| insert.column_count);
        let rows = insert.map_or(1.0, |insert| insert.row_count);
        let many = columns * rows > 1.0;
        let limit = |count: f64| if columns * rows > 10_000.0 { 1.0 } else { count };
        if many {
            self.notices.add(NoticeLevel::Info, "minsert", "配列のブロック参照（MINSERT）は、配置を並べて書きました");
        }
        let inherit = self.child_inherit(base, ctx);
        // ブロックの定義の座標 = (ブロックの座標 − 基点) × 単位の mm ÷ 縮尺（その縮尺のレイヤグループの用紙の mm）
        let k = self.unit / self.scales[ctx.scale];
        let block_matrix: Matrix = [k, 0.0, 0.0, k, -k * base_point[0], -k * base_point[1]];
        let mut stack = ctx.stack.clone();
        stack.push(key.clone());
        let mut r = 0.0;
        while r < limit(rows) {
            let mut c = 0.0;
            while c < limit(columns) {
                let offset = match insert {
                    Some(insert) => [1.0, 0.0, 0.0, 1.0, c * insert.column_spacing, r * insert.row_spacing],
                    None => IDENTITY,
                };
                let (sin, cos) = rotation.to_radians().sin_cos();
                let local = multiply(
                    &multiply(&multiply(&ocs, &[cos, sin, -sin, cos, point[0], point[1]]), &offset),
                    &[xscale, 0.0, 0.0, yscale, -xscale * base_point[0], -yscale * base_point[1]],
                );
                let suffix = if many {
                    format!("[{},{}]", js::number_to_string(r), js::number_to_string(c))
                } else {
                    String::new()
                };
                let placement = multiply(&multiply(&ctx.m, &local), &invert(&block_matrix));
                let parts = if ctx.flatten { None } else { decompose(&placement) };
                match parts {
                    Some((x, y, angle, scale_x, scale_y)) => {
                        let number = self.part(name, &key, &members, &inherit, ctx, &placement, &block_matrix, &stack);
                        let layer = self.effective_layer(base, ctx);
                        let slot = self.slot(ctx.scale, &layer);
                        let style = JwwBase { pen_style: 1, pen_color: 2, ..JwwBase::default() };
                        let placed = JwwBlock {
                            base: style,
                            x,
                            y,
                            scale_x,
                            scale_y,
                            rotation: angle.to_radians(),
                            definition: number,
                        };
                        let source = format!("{}{}{suffix}>", ctx.prefix, base.handle);
                        items.push(Item::entity(JwwEntity::Block(placed), slot, source));
                    }
                    None => {
                        if !ctx.flatten {
                            self.notices.add(
                                NoticeLevel::Info,
                                "skewed",
                                "ゆがんだ配置のブロック参照は、JWW のブロックにできないため、ブロックの図形を並べて書きました",
                            );
                        }
                        let source =
                            ctx.source.clone().unwrap_or_else(|| format!("{}{}{suffix}>", ctx.prefix, base.handle));
                        let child = Ctx {
                            m: multiply(&ctx.m, &local),
                            inherit: inherit.clone(),
                            stack: stack.clone(),
                            flatten: true,
                            source: Some(source),
                            ..ctx.clone()
                        };
                        for member in &members {
                            self.emit(member, &child, items);
                        }
                    }
                }
                c += 1.0;
            }
            r += 1.0;
        }
        // 属性は配置と同じ座標系にあり、ブロック参照の値を継ぐ
        if let Some(insert) = insert {
            let child = Ctx { inherit, stack, ..ctx.clone() };
            for attribute in &insert.attribs {
                let source = ctx
                    .source
                    .clone()
                    .unwrap_or_else(|| format!("{}{}>{}", ctx.prefix, base.handle, attribute.base.handle));
                let child = Ctx { source: Some(source), ..child.clone() };
                self.pend(attribute, &child, items);
            }
        }
    }

    /// ブロックの定義（継ぐ値・縮尺ごとに 1 つ）。初めて使うときに、ブロックの図形を写す
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        name: &str,
        key: &str,
        members: &[&'a DxfEntity],
        inherit: &Inherit,
        ctx: &Ctx,
        placement: &Matrix,
        block_matrix: &Matrix,
        stack: &[String],
    ) -> u32 {
        let variant = self.inherits(name, 0).then(|| inherit.key());
        // 線種のピッチは用紙の mm なので、破線を持つブロックは用紙での倍率ごとに定義を作る
        let scale = length_scale(&multiply(&ctx.to_paper, placement)) * ctx.dash_factor;
        let key = if self.dashed(name, 0) || self.linetype_dashed(&inherit.linetype) {
            format!("{key}\u{0}{}", js::to_precision(scale, 12))
        } else {
            key.to_string()
        };
        let lookup = (key, variant, ctx.scale);
        if let Some(number) = self.variants.get(&lookup) {
            return *number;
        }
        self.next_block += 1;
        let number = self.next_block;
        self.variants.insert(lookup, number);
        let child = Ctx {
            m: *block_matrix,
            to_paper: multiply(&ctx.to_paper, placement),
            dash_factor: ctx.dash_factor,
            inherit: inherit.clone(),
            stack: stack.to_vec(),
            flatten: false,
            prefix: String::new(),
            source: None,
            clip: None,
            scale: ctx.scale,
        };
        let mut items = Vec::new();
        for member in members {
            self.emit(member, &child, &mut items);
        }
        let name = self.unique_block(name.trim_start_matches('*'));
        self.blocks.push(Block { number, name, items });
        number
    }

    fn unique_block(&mut self, name: &str) -> String {
        let base = if name.trim().is_empty() { "ブロック" } else { name };
        let mut candidate = base.to_string();
        let mut number = 2;
        while self.block_names.contains(&candidate) {
            candidate = format!("{base} ({number})");
            number += 1;
        }
        self.block_names.insert(candidate.clone());
        candidate
    }

    /// 表示が描く形から写す図形に加える。継ぐ値（BYBLOCK・レイヤ 0）は図形に入れておく
    fn pend(&mut self, entity: &DxfEntity, ctx: &Ctx, items: &mut Vec<Item>) {
        let base = &entity.base;
        let mut copy = entity.clone();
        let layer_name = self.effective_layer(base, ctx);
        let paint = self.paint(base, ctx);
        copy.base.layer = layer_name.clone();
        if base.color_index == 0.0 && base.true_color.is_none() {
            match paint {
                Paint::Foreground | Paint::Background => copy.base.color_index = 7.0,
                Paint::Rgb([r, g, b]) => {
                    copy.base.color_index = 7.0;
                    copy.base.true_color = Some(f64::from((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)));
                }
            }
        }
        if base.lineweight == -2.0 {
            copy.base.lineweight = (ctx.inherit.lineweight * 100.0).round();
        }
        if base.linetype.eq_ignore_ascii_case("BYBLOCK") {
            copy.base.linetype = self.linetype_name(base, ctx);
        }
        // ブロックの中の属性定義（定数）は、配置の中と同じく値を描くよう属性にする（最上位の属性定義は名札を描く）
        if let DxfEntityData::Attdef { text, tag, attribute_flags, mtext, .. } = &copy.data
            && ctx.inherit.layer.is_some()
        {
            copy.data = DxfEntityData::Attrib {
                text: text.clone(),
                tag: tag.clone(),
                attribute_flags: *attribute_flags,
                mtext: mtext.clone(),
            };
            copy.base.kind = "ATTRIB".into();
        }
        let index = self.pending.len();
        copy.base.handle = format!("ZJ{index}");
        copy.base.space = DxfSpace::Model;
        copy.base.block = None;
        copy.base.owner = None;
        let font = self.font_name(entity);
        let slot = self.slot(ctx.scale, &layer_name);
        // 表示の線種の長さは図面の単位を mm にした長さ
        let pattern_scale = length_scale(&multiply(&ctx.to_paper, &ctx.m)) / self.unit;
        let dash_scale = pattern_scale * ctx.dash_factor;
        self.pending.push(Pending {
            entity: copy,
            m: ctx.m,
            dash_scale,
            pattern_scale,
            slot,
            font,
            source: ctx.source(base),
        });
        items.push(Item::Pending(index));
    }

    /// 文字の図形の文字スタイルのフォント（表示の font_family と同じ名前）。SHX・分からないものは ＭＳ ゴシック
    fn font_name(&self, entity: &DxfEntity) -> String {
        let style = match &entity.data {
            DxfEntityData::Text(text) | DxfEntityData::Attrib { text, .. } | DxfEntityData::Attdef { text, .. } => {
                text.style.clone()
            }
            DxfEntityData::MText(mtext) => mtext.style.clone(),
            _ => return DEFAULT_FONT.into(),
        };
        let doc = self.doc;
        let Some(style) = doc
            .styles
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&style))
            .or_else(|| doc.styles.iter().find(|candidate| candidate.name.eq_ignore_ascii_case("STANDARD")))
        else {
            return DEFAULT_FONT.into();
        };
        let file = style.font.as_str();
        let family = style.font_family.as_deref().filter(|family| !family.is_empty());
        let lower = file.to_lowercase();
        let truetype = [".ttf", ".ttc", ".otf"].iter().any(|suffix| lower.ends_with(suffix))
            || family.is_some()
            || (!file.is_empty() && !lower.ends_with(".shx") && file.chars().any(|c| !(' '..='~').contains(&c)));
        if !truetype {
            return DEFAULT_FONT.into();
        }
        let name = family.unwrap_or(file).to_string();
        let lower = name.to_lowercase();
        let name = [".ttf", ".ttc", ".otf"]
            .iter()
            .find_map(|suffix| lower.ends_with(suffix).then(|| name[..name.len() - suffix.len()].to_string()))
            .unwrap_or(name);
        let name = name.trim_start_matches('@').trim().to_string();
        if name.is_empty() { DEFAULT_FONT.into() } else { name }
    }

    // --- 仕上げ ----------------------------------------------------------------------------------------

    /// 表示が描く形から写す図形を、表示を作って JWW の図形にする（すべてを 1 つの図面にまとめて 1 度だけ描く）
    fn render_pending(&mut self) -> HashMap<usize, Vec<JwwEntity>> {
        let mut out: HashMap<usize, Vec<JwwEntity>> = HashMap::new();
        if self.pending.is_empty() {
            return out;
        }
        let mut synthetic = self.doc.clone();
        synthetic.entities.retain(|entity| entity.base.space == DxfSpace::Block && !self.is_layout_block(entity));
        synthetic.entities.extend(self.pending.iter().map(|pending| pending.entity.clone()));
        synthetic.sort_handles.retain(|(name, _)| !name.eq_ignore_ascii_case("*MODEL_SPACE"));
        synthetic.notices = Vec::new();
        let scene = dxf_to_scene(&synthetic, Some("Model"));
        for notice in &scene.notices {
            if notice.code != "paper-space" && notice.code != "unit-assumed" {
                self.notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
            }
        }
        let mut by_handle: HashMap<&str, Vec<&zumen_core::SceneEntity>> = HashMap::new();
        for entity in &scene.entities {
            by_handle.entry(top_id(&entity.id)).or_default().push(entity);
        }
        for index in 0..self.pending.len() {
            let handle = format!("ZJ{index}");
            let Some(entities) = by_handle.get(handle.as_str()) else {
                continue;
            };
            let mut converted = Vec::new();
            for entity in entities {
                // 表示は図面の単位を mm にして描く。placement.inverse は表示 → 実寸の mm
                let inverse = entity.placement.as_ref().map_or(IDENTITY, |placement| placement.inverse);
                let unit = self.unit;
                let to_container = multiply(
                    &multiply(&self.pending[index].m, &[1.0 / unit, 0.0, 0.0, 1.0 / unit, 0.0, 0.0]),
                    &inverse,
                );
                for primitive in &entity.primitives {
                    self.primitive(index, primitive, &to_container, &mut converted);
                }
            }
            out.insert(index, converted);
        }
        out
    }

    fn is_layout_block(&self, entity: &DxfEntity) -> bool {
        entity.base.block.as_deref().is_some_and(|name| {
            self.doc.blocks.iter().any(|block| block.layout && block.name.eq_ignore_ascii_case(name))
        })
    }

    /// 線の図形の共通の値（表示の線の色・太さ・破線から。scale は表示の破線の長さ → 用紙の mm）
    fn stroke_base(&mut self, scale: f64, color: &str, width: f64, dashes: &[f64]) -> JwwBase {
        let dashes: Vec<f64> = dashes.iter().map(|value| value * scale).collect();
        let pen_style = self.pen_style(&dashes);
        let pen_color = self.pen_color(paint_of_hex(color));
        JwwBase { pen_style, pen_color, pen_width: Self::pen_width(width), ..JwwBase::default() }
    }

    /// 表示の 1 つの要素を JWW の図形にする（m は表示の座標 → 入れ物の座標）
    fn primitive(&mut self, index: usize, primitive: &Primitive, m: &Matrix, out: &mut Vec<JwwEntity>) {
        match primitive {
            Primitive::Stroke(stroke) => {
                let scale = self.pending[index].dash_scale;
                let base = self.stroke_base(scale, &stroke.color, stroke.width, &stroke.dashes);
                let points = transform_points(m, &stroke.points);
                for [x1, y1, x2, y2] in segments_of(&points, stroke.closed, stroke.segments == Some(true)) {
                    out.push(JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 }));
                }
            }
            Primitive::Fill(fill) => {
                let rings: Vec<Vec<f64>> =
                    fill.rings.iter().map(|ring| transform_points(m, ring)).filter(|ring| ring.len() >= 6).collect();
                let (pen_color, color) = self.solid_color(paint_of_hex(&fill.color));
                let base = JwwBase { pen_style: 1, pen_color, ..JwwBase::default() };
                let (quads, complete) = trapezoids(&rings);
                if !complete {
                    self.notices.add(
                        NoticeLevel::Warning,
                        "fill-complex",
                        "辺の多すぎる塗りは、辺の交わりを調べずにソリッドに分けました（重なる所が欠けることがあります）",
                    );
                }
                if quads.len() > 1 {
                    self.notices.add(
                        NoticeLevel::Info,
                        "fill-solids",
                        "四角形でない塗りは、横の帯ごとの台形のソリッドに分けて書きました",
                    );
                }
                for points in quads {
                    out.push(JwwEntity::Solid(JwwSolid { base, points, color }));
                }
            }
            Primitive::Clipped(clipped) => {
                let rings: Vec<Vec<f64>> =
                    clipped.rings.iter().map(|ring| transform_points(m, ring)).filter(|ring| ring.len() >= 6).collect();
                let edges: usize = rings.iter().map(|ring| ring.len() / 2).sum();
                let lines: usize = clipped.strokes.iter().map(|stroke| stroke.points.len() / 4).sum();
                if edges.saturating_mul(lines) > MAX_CLIP_WORK {
                    self.notices.add(
                        NoticeLevel::Warning,
                        "hatch-dense",
                        "線と輪郭の多すぎるハッチングの模様は書いていません",
                    );
                    return;
                }
                self.notices.add(
                    NoticeLevel::Info,
                    "hatch-lines",
                    "ハッチングの模様は、表示と同じ線を領域で切った線分にしました",
                );
                let scale = self.pending[index].pattern_scale;
                for stroke in &clipped.strokes {
                    let base = self.stroke_base(scale, &stroke.color, stroke.width, &stroke.dashes);
                    let points = transform_points(m, &stroke.points);
                    for segment in segments_of(&points, stroke.closed, stroke.segments == Some(true)) {
                        for [x1, y1, x2, y2] in clip_to_rings(segment, &rings) {
                            out.push(JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 }));
                        }
                    }
                }
            }
            Primitive::Dot(dot) => {
                let pen_color = self.pen_color(paint_of_hex(&dot.color));
                let (x, y) = apply(m, dot.x, dot.y);
                let base = JwwBase { pen_style: 1, pen_color, ..JwwBase::default() };
                out.push(JwwEntity::Point(JwwPoint { base, x, y, temporary: false, code: 0, angle: 0.0, scale: 0.0 }));
            }
            Primitive::Text(text) => {
                if let Some(entity) = self.text(index, text, m) {
                    out.push(entity);
                }
            }
        }
    }

    /// 文字。始点は文字範囲の左下、終点までの長さは文字範囲の幅、大きさの高さは文字範囲の高さ（表示の
    /// text_layout_input の逆）。文字の幅は、全角 1・半角 0.5 で割り振った 1 字の幅
    fn text(&mut self, index: usize, text: &TextPrimitive, m: &Matrix) -> Option<JwwEntity> {
        let matrix = multiply(m, &text.matrix);
        let Some((x, y, angle, rx, (shear, ry))) = decompose_text(&matrix) else {
            self.notices.add(NoticeLevel::Warning, "text-skew", "ゆがんだ文字は書いていません");
            return None;
        };
        let (x, y) = if mirrors(&matrix) {
            self.notices.add(
                NoticeLevel::Info,
                "text-mirror",
                "裏返しの文字は、JWW に書けないため、同じ文字範囲に裏返さずに書きました",
            );
            // 裏返した文字範囲の上端が、裏返さない文字範囲の下端になる
            apply(&matrix, 0.0, text.box_height)
        } else {
            (x, y)
        };
        if shear.abs() > 1e-9 {
            self.notices.add(
                NoticeLevel::Info,
                "text-oblique",
                "斜体の文字（傾き角）は、JWW の文字に書けないため、立てて書きました",
            );
        }
        let vertical = match text.cells.as_slice() {
            [first, second, ..] => (first.y - second.y).abs() > (first.x - second.x).abs(),
            _ => (text.height - text.box_height).abs() > 1e-9 * text.box_height.max(1e-12),
        };
        let width = text.box_width * rx;
        let height = text.box_height * ry;
        if !(width.is_finite() && height.is_finite() && width >= 0.0 && height > 0.0) {
            return None;
        }
        let units: f64 = text.text.chars().map(|c| if is_half_width(c) { 0.5 } else { 1.0 }).sum();
        let size_x = if vertical {
            width
        } else if units > 0.0 {
            width / units
        } else {
            height
        };
        // フォントは表示と同じ書体（明朝体かゴシック体か）になる名前
        let mut font = self.pending[index].font.clone();
        if font_family_for(&font) != text.font_family {
            font = if is_serif_family(&text.font_family) { SERIF_FONT.into() } else { DEFAULT_FONT.into() };
        }
        if vertical {
            font.insert(0, '@');
        }
        let (sin, cos) = angle.to_radians().sin_cos();
        let pen_color = self.pen_color(paint_of_hex(&text.color));
        Some(JwwEntity::Text(JwwText {
            base: JwwBase { pen_style: 1, pen_color, ..JwwBase::default() },
            x1: x,
            y1: y,
            x2: x + width * cos,
            y2: y + width * sin,
            text_type: 0,
            size_x,
            size_y: height,
            spacing: 0.0,
            angle,
            font,
            content: text.text.clone(),
        }))
    }

    /// DXF のレイヤを JWW のレイヤグループ・レイヤに割り当てる。「0-A 名前」は、縮尺が合い、空いていればその番号に、
    /// ほかは縮尺ごとのレイヤグループに出現順に置く。256 枚に入らないレイヤは、同じ縮尺の最後のレイヤにまとめる
    fn assign_slots(&mut self) -> Assignment {
        let mut group_scale: [Option<usize>; GROUPS] = [None; GROUPS];
        let mut owner: HashMap<(usize, usize), usize> = HashMap::new();
        let mut out: Vec<Option<(usize, usize)>> = vec![None; self.slots.len()];
        for (index, slot) in self.slots.iter().enumerate() {
            let Some((group, layer, _)) = parse_layer_name(&slot.name) else {
                continue;
            };
            if group_scale[group].is_none_or(|scale| scale == slot.scale) && !owner.contains_key(&(group, layer)) {
                group_scale[group] = Some(slot.scale);
                owner.insert((group, layer), index);
                out[index] = Some((group, layer));
            }
        }
        let (mut merged, mut rescaled) = (0u64, 0u64);
        for (index, slot) in self.slots.iter().enumerate() {
            if out[index].is_some() {
                continue;
            }
            let free = (0..GROUPS).filter(|group| group_scale[*group] == Some(slot.scale)).find_map(|group| {
                (0..LAYERS).find(|layer| !owner.contains_key(&(group, *layer))).map(|layer| (group, layer))
            });
            let place = free.or_else(|| {
                let group = (0..GROUPS).find(|group| group_scale[*group].is_none())?;
                group_scale[group] = Some(slot.scale);
                Some((group, 0))
            });
            let place = match place {
                Some(place) => place,
                None => match (0..GROUPS).rev().find(|group| group_scale[*group] == Some(slot.scale)) {
                    Some(group) => {
                        merged += 1;
                        (group, LAYERS - 1)
                    }
                    None => {
                        // 同じ縮尺のレイヤグループがない: 縮尺のいちばん近いレイヤグループ（座標は用紙の mm なので形は同じ）
                        rescaled += 1;
                        let wanted = self.scales[slot.scale];
                        let distance = |group: &usize| {
                            group_scale[*group].map_or(f64::INFINITY, |scale| (self.scales[scale] / wanted).ln().abs())
                        };
                        let group = (0..GROUPS).min_by(|a, b| distance(a).total_cmp(&distance(b))).unwrap_or(0);
                        (group, LAYERS - 1)
                    }
                },
            };
            owner.entry(place).or_insert(index);
            out[index] = Some(place);
        }
        if merged > 0 {
            self.notices.add_count(
                NoticeLevel::Warning,
                "layer-merged",
                "JWW のレイヤ（16 グループ × 16 枚）に入らないレイヤの図形は、同じ縮尺の最後のレイヤに書きました",
                merged,
            );
        }
        if rescaled > 0 {
            self.notices.add_count(
                NoticeLevel::Warning,
                "scale-merged",
                "レイヤグループ（16）に入らない縮尺の図形は、縮尺の近いレイヤグループに用紙での位置のまま書きました（実寸の座標は変わります）",
                rescaled,
            );
        }
        Assignment { places: out.into_iter().map(|place| place.unwrap_or((0, 0))).collect(), group_scale, owner }
    }

    /// DXF のレイヤの JWW のレイヤの状態（0 非表示、1 表示のみ、2 編集可能）。DEFPOINTS（寸法の定義点）と、印刷しない
    /// レイヤは非表示
    fn layer_state(&self, upper: &str) -> u32 {
        if upper == "DEFPOINTS" {
            return 0;
        }
        match self.layers_upper.get(upper) {
            Some(layer) if layer.off || layer.frozen || !layer.plot => 0,
            Some(layer) if layer.locked => 1,
            _ => 2,
        }
    }

    fn finish(mut self) -> MappedJww {
        let mut rendered = self.render_pending();
        let Assignment { places, group_scale, owner } = self.assign_slots();
        let mut doc = std::mem::take(&mut self.blank);
        doc.paper_size = self.paper;
        doc.width_unit = WidthUnit { hundredth: true, dpi: None };
        for (group, scale) in group_scale.iter().enumerate() {
            let (Some(scale), Some(info)) = (scale, doc.groups.get_mut(group)) else {
                continue;
            };
            info.scale = self.scales[*scale];
        }
        for (&(group, layer), &index) in &owner {
            let slot = &self.slots[index];
            let name = match parse_layer_name(&slot.name) {
                Some((g, l, name)) if (g, l) == (group, layer) => name,
                _ => slot.name.clone(),
            };
            let state = self.layer_state(&slot.upper);
            if let Some(info) = doc.groups.get_mut(group).and_then(|info| info.layers.get_mut(layer)) {
                info.name = if name.is_empty() { format!("{group:X}-{layer:X}") } else { name };
                info.state = state;
            }
        }
        let mut sources = HashMap::new();
        let mut resolve = |items: Vec<Item>, key: &dyn Fn(usize) -> String, top: bool| -> Vec<JwwEntity> {
            let mut out = Vec::new();
            for item in items {
                let (entities, slot, source) = match item {
                    Item::Entity(entity, slot, source) => (vec![*entity], slot, source),
                    Item::Pending(index) => (
                        rendered.remove(&index).unwrap_or_default(),
                        self.pending[index].slot,
                        self.pending[index].source.clone(),
                    ),
                };
                let (group, layer) = places.get(slot).copied().unwrap_or((0, 0));
                for mut entity in entities {
                    set_layer(&mut entity, group as u16, layer as u16);
                    if top
                        && let JwwEntity::Text(text) = &mut entity
                        && is_setting(text)
                    {
                        // Jw_cad の設定（(0, -1000) の「名前 = 値」）と読まれない位置にずらす
                        text.y1 += 1e-6;
                        text.y2 += 1e-6;
                    }
                    out.push(entity);
                    sources.insert(key(out.len()), source.clone());
                }
            }
            out
        };
        let top = std::mem::take(&mut self.top);
        doc.entities = resolve(top, &|n| format!("#{n}"), true);
        let blocks = std::mem::take(&mut self.blocks);
        doc.blocks = blocks
            .into_iter()
            .map(|block| {
                let number = block.number;
                let entities = resolve(block.items, &|n| format!("{number}#{n}"), false);
                JwwBlockDefinition { number, name: block.name, entities }
            })
            .collect();
        doc.extended_colors = Some(std::mem::take(&mut self.extended));
        for (style, segments) in std::mem::take(&mut self.line_types) {
            doc.line_types.insert(style, segments);
        }
        MappedJww { document: doc, notices: self.notices.into_vec(), sources }
    }
}

/// JWW の円弧
fn jww_arc(base: JwwBase, shape: &ArcShape) -> JwwArc {
    JwwArc {
        base,
        cx: shape.cx,
        cy: shape.cy,
        radius: shape.radius,
        start: shape.start,
        sweep: shape.sweep,
        tilt: shape.tilt,
        flatness: shape.flatness,
        full: shape.full,
    }
}

/// 図形（寸法は中の線・文字も）のレイヤグループ・レイヤ
fn set_layer(entity: &mut JwwEntity, group: u16, layer: u16) {
    let set = |base: &mut JwwBase| {
        base.layer_group = group;
        base.layer = layer;
    };
    match entity {
        JwwEntity::Line(e) => set(&mut e.base),
        JwwEntity::Arc(e) => set(&mut e.base),
        JwwEntity::Point(e) => set(&mut e.base),
        JwwEntity::Text(e) => set(&mut e.base),
        JwwEntity::Solid(e) => set(&mut e.base),
        JwwEntity::CircleSolid(e) => set(&mut e.base),
        JwwEntity::Block(e) => set(&mut e.base),
        JwwEntity::Dimension(e) => {
            set(&mut e.base);
            set(&mut e.line.base);
            set(&mut e.text.base);
            e.lines.iter_mut().for_each(|line| set(&mut line.base));
            e.points.iter_mut().for_each(|point| set(&mut point.base));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_arcs_through_any_affine_transform() {
        // 半径 2 の円を x に 3 倍・y に 1 倍すると、長径 6・短径 2 の楕円
        let shape =
            arc_shape(&[3.0, 0.0, 0.0, 1.0, 10.0, 0.0], [0.0, 0.0], [2.0, 0.0], [0.0, 2.0], 0.0, TAU, true).unwrap();
        assert!((shape.radius - 6.0).abs() < 1e-12 && (shape.flatness - 1.0 / 3.0).abs() < 1e-12, "{shape:?}");
        assert_eq!((shape.cx, shape.cy, shape.full), (10.0, 0.0, true));
        // y に 3 倍なら長軸は y の向き
        let shape =
            arc_shape(&[1.0, 0.0, 0.0, 3.0, 0.0, 0.0], [0.0, 0.0], [2.0, 0.0], [0.0, 2.0], 0.0, TAU, true).unwrap();
        assert!((shape.tilt.abs() - PI / 2.0).abs() < 1e-12, "{shape:?}");
        // 裏返す変換の円弧（0° から 90°）は、反時計回りの 270° から 360°
        let shape =
            arc_shape(&[1.0, 0.0, 0.0, -1.0, 0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0], 0.0, PI / 2.0, false)
                .unwrap();
        assert!((shape.start - 1.5 * PI).abs() < 1e-12 && (shape.sweep - PI / 2.0).abs() < 1e-12, "{shape:?}");
        // 楕円弧の始点・終点は、写した曲線の始点・終点
        let m = [2.0, 1.0, -0.5, 1.5, 3.0, 4.0];
        let (t0, sweep) = (0.3, 2.0);
        let shape = arc_shape(&m, [1.0, 2.0], [3.0, 1.0], [-0.5, 1.0], t0, sweep, false).unwrap();
        let curve = |t: f64| apply(&m, 1.0 + 3.0 * t.cos() - 0.5 * t.sin(), 2.0 + t.cos() + t.sin());
        let point = |angle: f64| {
            let (rx, ry) = (shape.radius * angle.cos(), shape.radius * shape.flatness * angle.sin());
            let (sin, cos) = shape.tilt.sin_cos();
            (shape.cx + rx * cos - ry * sin, shape.cy + rx * sin + ry * cos)
        };
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 1e-9;
        let (start, end) = (curve(t0), curve(t0 + sweep));
        let (a, b) = (point(shape.start), point(shape.start + shape.sweep));
        assert!(
            (close(a, start) && close(b, end)) || (close(a, end) && close(b, start)),
            "{a:?} {b:?} {start:?} {end:?}"
        );
        assert!((shape.sweep - sweep).abs() < 1e-9);
    }

    #[test]
    fn splits_fills_into_trapezoids_by_the_even_odd_rule() {
        let square = |size: f64| vec![-size, -size, size, -size, size, size, -size, size];
        // 中抜きの正方形（外 4 × 4、内 2 × 2）: 面積は 12
        let (quads, complete) = trapezoids(&[square(2.0), square(1.0)]);
        assert!(complete);
        let area = |q: &[f64; 8]| ((q[2] - q[0]) + (q[4] - q[6])) / 2.0 * (q[5] - q[1]);
        let total: f64 = quads.iter().map(area).sum();
        assert!((total - 12.0).abs() < 1e-12, "{quads:?}");
        assert_eq!(quads.len(), 4, "上・下の帯と、中の左右");
        // 交わる辺（蝶ネクタイ）は交点で帯を分ける
        let (quads, _) = trapezoids(&[vec![0.0, 0.0, 2.0, 2.0, 2.0, 0.0, 0.0, 2.0]]);
        let total: f64 = quads.iter().map(area).sum();
        assert!((total - 2.0).abs() < 1e-12, "{quads:?}");
    }

    #[test]
    fn clips_lines_to_rings() {
        let rings =
            vec![vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0], vec![4.0, 4.0, 6.0, 4.0, 6.0, 6.0, 4.0, 6.0]];
        let pieces = clip_to_rings([-5.0, 5.0, 15.0, 5.0], &rings);
        assert_eq!(pieces, vec![[0.0, 5.0, 4.0, 5.0], [6.0, 5.0, 10.0, 5.0]]);
    }

    #[test]
    fn reads_layer_names_written_from_jww() {
        assert_eq!(parse_layer_name("3-A 通り芯"), Some((3, 10, "通り芯".into())));
        assert_eq!(parse_layer_name("0-F"), Some((0, 15, String::new())));
        assert_eq!(parse_layer_name("WALL"), None);
        assert_eq!(parse_layer_name("3-a"), None);
        assert_eq!(parse_layer_name("3-AB"), None);
    }

    #[test]
    fn chooses_papers_and_scales() {
        assert_eq!(paper_code(420.0, 297.0), (3, true));
        assert_eq!(paper_code(210.0, 297.0), (3, false), "A4 縦は、それを含む A3 横");
        assert_eq!(paper_code(5000.0, 5000.0), (11, false));
        assert_eq!(model_scale(30000.0, 20000.0, 378.0, 267.3), 100.0);
        assert_eq!(model_scale(100.0, 50.0, 378.0, 267.3), 1.0);
        assert_eq!(tidy_scale(99.99999999999999), 100.0);
        assert_eq!(tidy_scale(0.3333333432674408), 0.3333333432674408);
    }
}
