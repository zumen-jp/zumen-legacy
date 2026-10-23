//! 2 つの版の図面（`Scene`）を比べる。形式によっては保存し直すと図形の id が変わるので、id は使わず、
//! 図形の種類・形・文字で照合する。座標は用紙の mm で、前の版の座標で返す。
//!
//! 照合は 3 段で行う: 座標を許容差で丸めた形が同じ図形、丸めの境目で分かれた図形（座標の差が許容差の 2 倍以内）、
//! 残った図形のうち同じ種類で場所が重なるもの（変更）。後の版が全体にずれているとき（DXF の座標の寄せ方が
//! 変わったときなど）は、両方に 1 つずつある同じ文字の位置からずれを見つけて合わせる。

use crate::extract::{entity_text, hidden_layers, is_entity_visible, layer_names};
use crate::geometry::{empty_box, extend_box, is_empty_box, multiply, union_box};
use crate::types::{
    BACKGROUND_COLOR, BBox, Background, Notice, NoticeLevel, Placement, Primitive, Property, Scene, SceneEntity,
    SceneLayer, StrokePrimitive,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// 比べ方
#[derive(Debug, Clone, PartialEq)]
pub struct CompareOptions {
    /// 同じ位置とみなす座標の差（用紙の mm）
    pub tolerance: f64,
    /// 色・線の太さ・線種・レイヤの名前の違いも変更とみなす
    pub style: bool,
    /// 描かないレイヤの図形も比べる。false なら図面の表示指定に従う
    pub all_layers: bool,
    /// 両方の版にある同じ文字の位置から図面全体のずれを見つけ、後の版をずらして合わせる
    pub align: bool,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self { tolerance: 0.01, style: false, all_layers: false, align: true }
    }
}

/// 変わった図形の集まり（前の版の用紙の mm）
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeRegion {
    pub bbox: BBox,
    pub removed: usize,
    pub added: usize,
    pub changed: usize,
}

/// 2 つの版の違い。図形はそれぞれの版の `Scene::entities` の位置で指す
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SceneDiff {
    /// 前の版にだけある図形
    pub removed: Vec<usize>,
    /// 後の版にだけある図形
    pub added: Vec<usize>,
    /// 同じ図形が変わったとみなした組（前の版、後の版）
    pub changed: Vec<(usize, usize)>,
    /// 変わらなかった図形の組（前の版、後の版）
    pub unchanged: Vec<(usize, usize)>,
    /// 後の版を前の版に合わせるために、後の版の座標から引いたずれ
    pub offset: (f64, f64),
    /// 変わった場所。近い図形をまとめ、変わった図形の多い順に並べる
    pub regions: Vec<ChangeRegion>,
}

/// 2 つの版を比べる
pub fn compare_scenes(before: &Scene, after: &Scene, options: &CompareOptions) -> SceneDiff {
    let tolerance = if options.tolerance.is_finite() && options.tolerance > 0.0 { options.tolerance } else { 0.01 };
    let ea = compared_entities(before, options);
    let eb = compared_entities(after, options);
    let offset = if options.align { find_offset(before, &ea, after, &eb, tolerance) } else { (0.0, 0.0) };
    let ka: Vec<Key> =
        ea.iter().map(|&i| Key::of(before, &before.entities[i], (0.0, 0.0), options, tolerance)).collect();
    let kb: Vec<Key> = eb.iter().map(|&i| Key::of(after, &after.entities[i], offset, options, tolerance)).collect();

    // 1. 丸めた形が同じ図形。同じ形が複数あれば、描く順に組にする
    let mut waiting: HashMap<&Key, Vec<usize>> = HashMap::new();
    for (position, key) in kb.iter().enumerate().rev() {
        waiting.entry(key).or_default().push(position);
    }
    let mut unchanged = Vec::new();
    let mut left_a = Vec::new();
    for (position, key) in ka.iter().enumerate() {
        match waiting.get_mut(key).and_then(Vec::pop) {
            Some(other) => unchanged.push((position, other)),
            None => left_a.push(position),
        }
    }
    let mut used_b = vec![false; kb.len()];
    for &(_, other) in &unchanged {
        used_b[other] = true;
    }

    // 2. 丸めの境目で分かれた図形
    let mut cells: HashMap<(&str, i64, i64), Vec<usize>> = HashMap::new();
    for (position, key) in kb.iter().enumerate() {
        if !used_b[position] {
            cells.entry((key.kind.as_str(), key.corner.0, key.corner.1)).or_default().push(position);
        }
    }
    let mut left = Vec::new();
    for position in left_a {
        let key = &ka[position];
        let mut found = None;
        'search: for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(list) = cells.get(&(key.kind.as_str(), key.corner.0 + dx, key.corner.1 + dy)) else {
                    continue;
                };
                for &other in list {
                    if !used_b[other] && key.near(&kb[other]) {
                        found = Some(other);
                        break 'search;
                    }
                }
            }
        }
        match found {
            Some(other) => {
                used_b[other] = true;
                unchanged.push((position, other));
            }
            None => left.push(position),
        }
    }

    // 3. 同じ種類で場所が重なる図形を、変更とみなす
    let shifted = |b: &BBox| BBox {
        min_x: b.min_x - offset.0,
        min_y: b.min_y - offset.1,
        max_x: b.max_x - offset.0,
        max_y: b.max_y - offset.1,
    };
    let rest_b: Vec<usize> = (0..kb.len()).filter(|&position| !used_b[position]).collect();
    let index = SpatialIndex::new(
        &rest_b.iter().map(|&position| shifted(&after.entities[eb[position]].bbox)).collect::<Vec<_>>(),
        tolerance,
    );
    let mut changed = Vec::new();
    let mut removed = Vec::new();
    for position in left {
        let entity = &before.entities[ea[position]];
        let text = entity_text(entity);
        let mut best: Option<(f64, usize)> = None;
        for slot in index.near(&entity.bbox) {
            let other = rest_b[slot];
            if used_b[other] || kb[other].kind != ka[position].kind {
                continue;
            }
            let candidate = &after.entities[eb[other]];
            let score = similarity(entity, text.as_ref(), candidate, offset, tolerance);
            if score > 0.0 && best.is_none_or(|(value, _)| score > value) {
                best = Some((score, other));
            }
        }
        match best {
            Some((_, other)) => {
                used_b[other] = true;
                changed.push((ea[position], eb[other]));
            }
            None => removed.push(ea[position]),
        }
    }
    let added: Vec<usize> = (0..kb.len()).filter(|&position| !used_b[position]).map(|position| eb[position]).collect();
    let mut unchanged: Vec<(usize, usize)> = unchanged.into_iter().map(|(a, b)| (ea[a], eb[b])).collect();
    unchanged.sort_unstable();
    changed.sort_unstable();

    let regions = change_regions(before, after, &removed, &added, &changed, offset);
    SceneDiff { removed, added, changed, unchanged, offset, regions }
}

/// 比べる図形（描く順）
fn compared_entities(scene: &Scene, options: &CompareOptions) -> Vec<usize> {
    let hidden = if options.all_layers { HashSet::new() } else { hidden_layers(scene, None) };
    (0..scene.entities.len()).filter(|&i| is_entity_visible(&scene.entities[i], &hidden)).collect()
}

/// 座標を許容差の単位の整数にする。数でない値は 1 つの値にまとめる
fn quantize(value: f64, tolerance: f64) -> i64 {
    let scaled = (value / tolerance).round();
    if scaled.is_finite() { scaled as i64 } else { i64::MIN }
}

/// 図形を照合するための形。座標は丸め、向きと始点の違いはなくしておく
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    kind: String,
    parts: Vec<Part>,
    layers: Option<Vec<String>>,
    /// 外接矩形の左下の角（2 × 許容差の格子）。境目で分かれた図形を探す
    corner: (i64, i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Part {
    Stroke {
        points: Vec<i64>,
        closed: bool,
        segments: bool,
        style: Option<(String, i64, Vec<i64>)>,
    },
    Fill {
        rings: Vec<Vec<i64>>,
        color: Option<String>,
    },
    Clipped {
        rings: Vec<Vec<i64>>,
        strokes: Vec<Part>,
    },
    Dot {
        x: i64,
        y: i64,
        color: Option<String>,
    },
    /// 文字は、文字列・用紙の上の位置・向き（0.01 度）・高さ・横の倍率（0.001）・裏返し
    Text {
        text: String,
        x: i64,
        y: i64,
        angle: i64,
        height: i64,
        aspect: i64,
        mirrored: bool,
        color: Option<String>,
    },
}

impl Key {
    fn of(scene: &Scene, entity: &SceneEntity, offset: (f64, f64), options: &CompareOptions, tolerance: f64) -> Key {
        let q = |value: f64| quantize(value, tolerance);
        let shift = |points: &[f64]| -> Vec<i64> {
            points.as_chunks::<2>().0.iter().flat_map(|[x, y]| [q(x - offset.0), q(y - offset.1)]).collect()
        };
        let stroke = |stroke: &StrokePrimitive| -> Part {
            let segments = stroke.segments == Some(true);
            let points = shift(&stroke.points);
            let points = if segments {
                normalize_segments(points)
            } else if stroke.closed {
                normalize_ring(points)
            } else {
                normalize_path(points)
            };
            let style = options.style.then(|| {
                (stroke.color.clone(), q(stroke.width), stroke.dashes.iter().map(|&value| q(value)).collect())
            });
            Part::Stroke { points, closed: stroke.closed, segments, style }
        };
        let rings = |rings: &[Vec<f64>]| -> Vec<Vec<i64>> {
            let mut out: Vec<Vec<i64>> = rings.iter().map(|ring| normalize_ring(shift(ring))).collect();
            out.sort_unstable();
            out
        };
        let mut parts: Vec<Part> = entity
            .primitives
            .iter()
            .map(|primitive| match primitive {
                Primitive::Stroke(s) => stroke(s),
                Primitive::Fill(fill) => {
                    Part::Fill { rings: rings(&fill.rings), color: options.style.then(|| fill.color.clone()) }
                }
                Primitive::Clipped(clipped) => {
                    let mut strokes: Vec<Part> = clipped.strokes.iter().map(stroke).collect();
                    strokes.sort_unstable();
                    Part::Clipped { rings: rings(&clipped.rings), strokes }
                }
                Primitive::Dot(dot) => Part::Dot {
                    x: q(dot.x - offset.0),
                    y: q(dot.y - offset.1),
                    color: options.style.then(|| dot.color.clone()),
                },
                Primitive::Text(text) => {
                    let [a, b, c, d, e, f] = text.matrix;
                    let across = a.hypot(b);
                    let up = c.hypot(d);
                    Part::Text {
                        text: text.text.clone(),
                        x: q(e - offset.0),
                        y: q(f - offset.1),
                        angle: quantize(b.atan2(a).to_degrees(), 0.01),
                        height: q(text.height * up),
                        aspect: quantize(if up > 0.0 { across / up * text.scale_x } else { 0.0 }, 0.001),
                        mirrored: a * d - b * c < 0.0,
                        color: options.style.then(|| text.color.clone()),
                    }
                }
            })
            .collect();
        parts.sort_unstable();
        let layers = options.style.then(|| layer_names(scene, entity).into_iter().map(str::to_string).collect());
        let grid = 2.0 * tolerance;
        let corner = (
            quantize((entity.bbox.min_x - offset.0) / grid, 1.0),
            quantize((entity.bbox.min_y - offset.1) / grid, 1.0),
        );
        Key { kind: entity.type_name.clone(), parts, layers, corner }
    }

    /// 丸めた座標の差がどれも 2 以内で、ほかが同じか（丸めの境目で分かれた同じ図形）
    fn near(&self, other: &Key) -> bool {
        self.kind == other.kind
            && self.layers == other.layers
            && self.parts.len() == other.parts.len()
            && self.parts.iter().zip(&other.parts).all(|(a, b)| a.near(b))
    }
}

fn close(a: &[i64], b: &[i64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.abs_diff(*y) <= 2)
}

fn close_rings(a: &[Vec<i64>], b: &[Vec<i64>]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| close(x, y))
}

impl Part {
    fn near(&self, other: &Part) -> bool {
        match (self, other) {
            (
                Part::Stroke { points, closed, segments, style },
                Part::Stroke { points: p, closed: c, segments: s, style: st },
            ) => closed == c && segments == s && style == st && close(points, p),
            (Part::Fill { rings, color }, Part::Fill { rings: r, color: c }) => color == c && close_rings(rings, r),
            (Part::Clipped { rings, strokes }, Part::Clipped { rings: r, strokes: s }) => {
                close_rings(rings, r) && strokes.len() == s.len() && strokes.iter().zip(s).all(|(a, b)| a.near(b))
            }
            (Part::Dot { x, y, color }, Part::Dot { x: x2, y: y2, color: c }) => {
                color == c && x.abs_diff(*x2) <= 2 && y.abs_diff(*y2) <= 2
            }
            (
                Part::Text { text, x, y, angle, height, aspect, mirrored, color },
                Part::Text { text: t, x: x2, y: y2, angle: a, height: h, aspect: r, mirrored: m, color: c },
            ) => {
                text == t
                    && mirrored == m
                    && color == c
                    && x.abs_diff(*x2) <= 2
                    && y.abs_diff(*y2) <= 2
                    && angle.abs_diff(*a) <= 2
                    && height.abs_diff(*h) <= 2
                    && aspect.abs_diff(*r) <= 2
            }
            _ => false,
        }
    }
}

/// 点の並び（x, y を交互）を点の組にする
fn pairs(points: &[i64]) -> Vec<[i64; 2]> {
    points.as_chunks::<2>().0.to_vec()
}

fn flatten(points: &[[i64; 2]]) -> Vec<i64> {
    points.iter().flatten().copied().collect()
}

/// 続けて同じ点を 1 つにする
fn dedup(mut points: Vec<[i64; 2]>) -> Vec<[i64; 2]> {
    points.dedup();
    points
}

/// 開いた折線: 逆向きと比べて小さい方
fn normalize_path(points: Vec<i64>) -> Vec<i64> {
    let forward = dedup(pairs(&points));
    let mut backward = forward.clone();
    backward.reverse();
    flatten(if backward < forward { &backward } else { &forward })
}

/// 閉じた折線: 最も小さい点から始め、向きは小さい方。終点が始点と同じなら除く
fn normalize_ring(points: Vec<i64>) -> Vec<i64> {
    let mut ring = dedup(pairs(&points));
    if ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    if ring.is_empty() {
        return Vec::new();
    }
    let start = (0..ring.len()).min_by_key(|&i| ring[i]).unwrap_or(0);
    let n = ring.len();
    let forward: Vec<[i64; 2]> = (0..n).map(|i| ring[(start + i) % n]).collect();
    let backward: Vec<[i64; 2]> = (0..n).map(|i| ring[(start + n - i) % n]).collect();
    flatten(if backward < forward { &backward } else { &forward })
}

/// 2 点ずつ別の線分: 各線分の端点を並べ替え、線分を並べ替える
fn normalize_segments(points: Vec<i64>) -> Vec<i64> {
    let ends = pairs(&points);
    let mut segments: Vec<[[i64; 2]; 2]> =
        ends.as_chunks::<2>().0.iter().map(|&[a, b]| if b < a { [b, a] } else { [a, b] }).collect();
    segments.sort_unstable();
    segments.iter().flatten().flatten().copied().collect()
}

/// 両方の版に 1 つずつある同じ文字（文字列・高さ・向き）の位置の差のうち、最も多いもの。
/// 3 つ以上の文字が同じずれで、しかも比べた文字の半分以上ならそのずれを返す
fn find_offset(before: &Scene, ea: &[usize], after: &Scene, eb: &[usize], tolerance: f64) -> (f64, f64) {
    type TextKey = (String, i64, i64);
    let anchors = |scene: &Scene, entities: &[usize]| -> HashMap<TextKey, Option<(f64, f64)>> {
        let mut found: HashMap<TextKey, Option<(f64, f64)>> = HashMap::new();
        for &i in entities {
            let Some(text) = entity_text(&scene.entities[i]) else { continue };
            let key = (text.text, quantize(text.height, tolerance), quantize(text.angle, 0.01));
            found.entry(key).and_modify(|value| *value = None).or_insert(Some((text.x, text.y)));
        }
        found
    };
    let a = anchors(before, ea);
    let b = anchors(after, eb);
    let bucket = tolerance.max(0.1);
    let mut votes: HashMap<(i64, i64), Vec<(f64, f64)>> = HashMap::new();
    let mut pairs = 0usize;
    for (key, place) in &a {
        let (Some((ax, ay)), Some(Some((bx, by)))) = (place, b.get(key)) else { continue };
        pairs += 1;
        let (dx, dy) = (bx - ax, by - ay);
        votes.entry((quantize(dx, bucket), quantize(dy, bucket))).or_default().push((dx, dy));
    }
    let Some((cell, list)) = votes.into_iter().max_by(|(ca, la), (cb, lb)| la.len().cmp(&lb.len()).then(cb.cmp(ca)))
    else {
        return (0.0, 0.0);
    };
    if cell == (0, 0) || list.len() < 3 || list.len() * 2 < pairs {
        return (0.0, 0.0);
    }
    let n = list.len() as f64;
    let (sx, sy) = list.iter().fold((0.0, 0.0), |(sx, sy), (dx, dy)| (sx + dx, sy + dy));
    (sx / n, sy / n)
}

/// 変更の組としての近さ。0 なら組にしない。文字は、基準点が近く向きが同じなら組にする。
/// ほかの図形は、外接矩形を少し広げて重なりの割合（IoU）が 0.5 以上なら組にする
fn similarity(
    a: &SceneEntity,
    a_text: Option<&crate::extract::EntityText>,
    b: &SceneEntity,
    offset: (f64, f64),
    tolerance: f64,
) -> f64 {
    if let (Some(ta), Some(tb)) = (a_text, entity_text(b).as_ref()) {
        let reach = (ta.height * 0.5).max(2.0 * tolerance);
        let dx = tb.x - offset.0 - ta.x;
        let dy = tb.y - offset.1 - ta.y;
        let turn = ((tb.angle - ta.angle + 540.0) % 360.0 - 180.0).abs();
        if dx.hypot(dy) <= reach && turn < 0.5 {
            return 2.0 - dx.hypot(dy) / reach;
        }
    }
    let grow = (10.0 * tolerance).max(0.1);
    let expand = |b: &BBox, dx: f64, dy: f64| BBox {
        min_x: b.min_x - dx - grow,
        min_y: b.min_y - dy - grow,
        max_x: b.max_x - dx + grow,
        max_y: b.max_y - dy + grow,
    };
    let ba = expand(&a.bbox, 0.0, 0.0);
    let bb = expand(&b.bbox, offset.0, offset.1);
    let area = |b: &BBox| (b.max_x - b.min_x).max(0.0) * (b.max_y - b.min_y).max(0.0);
    let inter = BBox {
        min_x: ba.min_x.max(bb.min_x),
        min_y: ba.min_y.max(bb.min_y),
        max_x: ba.max_x.min(bb.max_x),
        max_y: ba.max_y.min(bb.max_y),
    };
    let common = area(&inter);
    let union = area(&ba) + area(&bb) - common;
    let iou = if union > 0.0 { common / union } else { 0.0 };
    if iou >= 0.5 { iou } else { 0.0 }
}

/// 外接矩形の中心で引く格子。変更の組の候補を探す
struct SpatialIndex {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// 格子の目 SPAN 個より大きな図形。いつも候補にする
    large: Vec<usize>,
}

impl SpatialIndex {
    const SPAN: f64 = 8.0;
    /// 大きな図形の周りを探す格子の目の数の上限
    const MAX_REACH: i64 = 64;

    fn new(boxes: &[BBox], tolerance: f64) -> Self {
        let mut extent = empty_box();
        for b in boxes {
            union_box(&mut extent, b);
        }
        let span =
            if is_empty_box(&extent) { 1.0 } else { (extent.max_x - extent.min_x).max(extent.max_y - extent.min_y) };
        let cell = if span.is_finite() { (span / 256.0).max(100.0 * tolerance).max(1e-6) } else { 1.0 };
        let mut index = Self { cell, cells: HashMap::new(), large: Vec::new() };
        for (slot, b) in boxes.iter().enumerate() {
            let size = (b.max_x - b.min_x).max(b.max_y - b.min_y);
            if !(size.is_finite() && size <= cell * Self::SPAN) {
                index.large.push(slot);
                continue;
            }
            let key = index.key((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
            index.cells.entry(key).or_default().push(slot);
        }
        index
    }

    fn key(&self, x: f64, y: f64) -> (i64, i64) {
        (quantize(x, self.cell), quantize(y, self.cell))
    }

    /// 外接矩形 b と重なりうる図形。中心どうしの距離は、2 つの大きさの和の半分（と少し）を超えない
    fn near(&self, b: &BBox) -> Vec<usize> {
        let size = (b.max_x - b.min_x).max(b.max_y - b.min_y);
        let mut out = self.large.clone();
        if !size.is_finite() {
            return out;
        }
        let reach = ((size / (2.0 * self.cell)).ceil() as i64 + Self::SPAN as i64 / 2 + 1).min(Self::MAX_REACH);
        let (cx, cy) = self.key((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
        for x in cx - reach..=cx + reach {
            for y in cy - reach..=cy + reach {
                if let Some(list) = self.cells.get(&(x, y)) {
                    out.extend_from_slice(list);
                }
            }
        }
        out.sort_unstable();
        out
    }
}

/// 変わった図形を、近いもの同士でまとめる（前の版の座標）
fn change_regions(
    before: &Scene,
    after: &Scene,
    removed: &[usize],
    added: &[usize],
    changed: &[(usize, usize)],
    offset: (f64, f64),
) -> Vec<ChangeRegion> {
    let shifted = |b: &BBox| BBox {
        min_x: b.min_x - offset.0,
        min_y: b.min_y - offset.1,
        max_x: b.max_x - offset.0,
        max_y: b.max_y - offset.1,
    };
    // (矩形, 種類: 0 削除・1 追加・2 変更)
    let mut items: Vec<(BBox, u8)> = Vec::new();
    items.extend(removed.iter().map(|&i| (before.entities[i].bbox, 0)));
    items.extend(added.iter().map(|&i| (shifted(&after.entities[i].bbox), 1)));
    items.extend(changed.iter().map(|&(a, b)| {
        let mut both = before.entities[a].bbox;
        union_box(&mut both, &shifted(&after.entities[b].bbox));
        (both, 2)
    }));
    items.retain(|(b, _)| !is_empty_box(b));
    if items.is_empty() {
        return Vec::new();
    }
    // 近さの目安: 用紙（なければ図面の範囲）の長い辺の 2%、少なくとも 2 mm
    let span = match &before.sheet {
        Some(sheet) => sheet.width.max(sheet.height),
        None => (before.bounds.max_x - before.bounds.min_x).max(before.bounds.max_y - before.bounds.min_y),
    };
    let margin = if span.is_finite() { (span * 0.02).max(2.0) } else { 2.0 };
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[a].0.min_x.total_cmp(&items[b].0.min_x));
    let mut parent: Vec<usize> = (0..items.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    // 左端の順に見て、まだ右端が届く矩形とだけ比べる
    let mut active: Vec<usize> = Vec::new();
    for &i in &order {
        let b = items[i].0;
        active.retain(|&j| items[j].0.max_x + margin >= b.min_x - margin);
        for &j in &active {
            let o = items[j].0;
            if o.min_y - margin <= b.max_y + margin && b.min_y - margin <= o.max_y + margin {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
        active.push(i);
    }
    let mut groups: HashMap<usize, ChangeRegion> = HashMap::new();
    for (i, (bbox, kind)) in items.iter().enumerate() {
        let r = root(&mut parent, i);
        let region =
            groups.entry(r).or_insert_with(|| ChangeRegion { bbox: empty_box(), removed: 0, added: 0, changed: 0 });
        union_box(&mut region.bbox, bbox);
        match kind {
            0 => region.removed += 1,
            1 => region.added += 1,
            _ => region.changed += 1,
        }
    }
    let mut regions: Vec<ChangeRegion> = groups.into_values().collect();
    let total = |r: &ChangeRegion| r.removed + r.added + r.changed;
    regions.sort_by(|a, b| {
        total(b).cmp(&total(a)).then(b.bbox.max_y.total_cmp(&a.bbox.max_y)).then(a.bbox.min_x.total_cmp(&b.bbox.min_x))
    });
    regions
}

/// 重ねた図で使う色（変わらない図形、前の版だけの図形、後の版だけの図形）
pub const UNCHANGED_COLOR: &str = "#a0a0a0";
pub const BEFORE_COLOR: &str = "#e00000";
pub const AFTER_COLOR: &str = "#0050f0";

/// 比べた結果を重ねた図面。変わらない図形（後の版）は灰、前の版だけの図形（削除と変更前）は赤、
/// 後の版だけの図形（追加と変更後）は青で、この順に描く。後の版は前の版の座標に合わせる。
/// レイヤは「変更なし」「削除・変更前」「追加・変更後」の 3 つで、図形の id には前の版なら A:、後の版なら B: を付ける
pub fn diff_scene(before: &Scene, after: &Scene, diff: &SceneDiff) -> Scene {
    let (ox, oy) = diff.offset;
    let mut entities = Vec::with_capacity(diff.unchanged.len() + diff.removed.len() + diff.added.len());
    let mut add = |source: &SceneEntity, from_after: bool, layer: u32, (label, key): (&str, &str)| {
        let mut entity = source.clone();
        let color = match layer {
            0 => UNCHANGED_COLOR,
            1 => BEFORE_COLOR,
            _ => AFTER_COLOR,
        };
        let fill = format!("{color}40");
        entity.primitives.retain(|p| !matches!(p, Primitive::Fill(f) if f.color == BACKGROUND_COLOR));
        for primitive in &mut entity.primitives {
            recolor(primitive, color, &fill);
            if from_after {
                translate(primitive, -ox, -oy);
            }
        }
        if from_after && (ox != 0.0 || oy != 0.0) {
            let b = &mut entity.bbox;
            *b = BBox { min_x: b.min_x - ox, min_y: b.min_y - oy, max_x: b.max_x - ox, max_y: b.max_y - oy };
            if let Some(placement) = &entity.placement {
                let moved = multiply(&placement.inverse, &[1.0, 0.0, 0.0, 1.0, ox, oy]);
                entity.placement = Some(Arc::new(Placement { inverse: moved, ..(**placement).clone() }));
            }
        }
        entity.id = format!("{}:{}", if from_after { "B" } else { "A" }, source.id);
        entity.layers = vec![layer];
        entity.properties.insert(0, Property::new("comparison", "比較", label).raw(key));
        entities.push(entity);
    };
    for &(_, b) in &diff.unchanged {
        add(&after.entities[b], true, 0, ("変更なし", "unchanged"));
    }
    for &a in &diff.removed {
        add(&before.entities[a], false, 1, ("削除", "removed"));
    }
    for &(a, _) in &diff.changed {
        add(&before.entities[a], false, 1, ("変更前", "changed-before"));
    }
    for &b in &diff.added {
        add(&after.entities[b], true, 2, ("追加", "added"));
    }
    for &(_, b) in &diff.changed {
        add(&after.entities[b], true, 2, ("変更後", "changed-after"));
    }
    let mut bounds = empty_box();
    for entity in &entities {
        union_box(&mut bounds, &entity.bbox);
    }
    if let Some(sheet) = before.sheet.as_ref().or(after.sheet.as_ref()) {
        extend_box(&mut bounds, 0.0, 0.0);
        extend_box(&mut bounds, sheet.width, sheet.height);
    }
    let layer = |id: u32, name: &str| SceneLayer { id, name: name.into(), visible: true, group: None };
    let mut metadata = vec![
        Property::new(
            "before",
            "前の版",
            format!("{} {}", before.version, before.view.as_deref().unwrap_or("")).trim_end(),
        ),
        Property::new(
            "after",
            "後の版",
            format!("{} {}", after.version, after.view.as_deref().unwrap_or("")).trim_end(),
        ),
        Property::new(
            "entityCounts",
            "図形",
            format!(
                "変更なし {}・削除 {}・追加 {}・変更 {}",
                diff.unchanged.len(),
                diff.removed.len(),
                diff.added.len(),
                diff.changed.len()
            ),
        )
        .raw([diff.unchanged.len(), diff.removed.len(), diff.added.len(), diff.changed.len()].map(|n| n as f64)),
    ];
    if ox != 0.0 || oy != 0.0 {
        metadata.push(
            Property::new(
                "offset",
                "後の版のずれ",
                format!("{}, {}", crate::js::number_to_string(ox), crate::js::number_to_string(oy)),
            )
            .raw([ox, oy]),
        );
    }
    let mut notices = Vec::new();
    let underlays = before.underlays.len() + after.underlays.len();
    if underlays > 0 {
        notices.push(Notice {
            level: NoticeLevel::Info,
            code: "underlay-not-compared".into(),
            message: "underlay（画像・PDF のページ）は比べず、差分の図面に描いていません".into(),
            count: underlays as u64,
        });
    }
    Scene {
        format: "diff".into(),
        version: "比較".into(),
        // 2 つの版の単位が違えば、属性の値の単位は 1 つに決まらない
        unit: if before.unit == after.unit { before.unit } else { None },
        views: None,
        view: None,
        sheet: before.sheet.clone().or_else(|| after.sheet.clone()),
        bounds,
        layers: vec![layer(0, "変更なし"), layer(1, "削除・変更前"), layer(2, "追加・変更後")],
        entities,
        // 比べるのは図形だけで、underlay（画像・PDF のページ）は比べず、差分の図面にも描かない
        underlays: Vec::new(),
        notices,
        metadata,
        background: Background::Light,
    }
}

fn recolor(primitive: &mut Primitive, color: &str, fill: &str) {
    match primitive {
        Primitive::Stroke(stroke) => stroke.color = color.into(),
        Primitive::Fill(f) => f.color = fill.into(),
        Primitive::Clipped(clipped) => {
            for stroke in &mut clipped.strokes {
                stroke.color = color.into();
            }
        }
        Primitive::Dot(dot) => dot.color = color.into(),
        Primitive::Text(text) => text.color = color.into(),
    }
}

fn translate_points(points: &mut [f64], dx: f64, dy: f64) {
    for [x, y] in points.as_chunks_mut::<2>().0 {
        *x += dx;
        *y += dy;
    }
}

fn translate(primitive: &mut Primitive, dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    match primitive {
        Primitive::Stroke(stroke) => translate_points(&mut stroke.points, dx, dy),
        Primitive::Fill(f) => f.rings.iter_mut().for_each(|ring| translate_points(ring, dx, dy)),
        Primitive::Clipped(clipped) => {
            clipped.rings.iter_mut().for_each(|ring| translate_points(ring, dx, dy));
            clipped.strokes.iter_mut().for_each(|stroke| translate_points(&mut stroke.points, dx, dy));
        }
        Primitive::Dot(dot) => {
            dot.x += dx;
            dot.y += dy;
        }
        Primitive::Text(text) => {
            text.matrix[4] += dx;
            text.matrix[5] += dy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::placement;
    use crate::types::{Sheet, TextPrimitive};

    fn stroke(points: &[f64], closed: bool, color: &str) -> Primitive {
        Primitive::Stroke(StrokePrimitive {
            points: points.to_vec(),
            closed,
            segments: None,
            color: color.into(),
            width: 0.0,
            dashes: Vec::new(),
        })
    }

    fn text(content: &str, x: f64, y: f64) -> Primitive {
        Primitive::Text(TextPrimitive {
            text: content.into(),
            color: "#000000".into(),
            font_family: "sans-serif".into(),
            height: 3.0,
            character_height: None,
            scale_x: 1.0,
            cells: Vec::new(),
            box_width: content.chars().count() as f64 * 3.0,
            box_height: 3.0,
            matrix: placement(x, y, 0.0, 1.0, 1.0),
        })
    }

    fn entity(id: &str, kind: &str, primitives: Vec<Primitive>) -> SceneEntity {
        let mut bbox = empty_box();
        for primitive in &primitives {
            match primitive {
                Primitive::Stroke(s) => {
                    for [x, y] in s.points.as_chunks::<2>().0 {
                        extend_box(&mut bbox, *x, *y);
                    }
                }
                Primitive::Text(t) => {
                    extend_box(&mut bbox, t.matrix[4], t.matrix[5]);
                    extend_box(&mut bbox, t.matrix[4] + t.box_width, t.matrix[5] + t.box_height);
                }
                _ => {}
            }
        }
        SceneEntity {
            id: id.into(),
            kind: crate::EntityKind::Line,
            type_name: kind.into(),
            source: "test".into(),
            layers: vec![0],
            primitives,
            bbox,
            properties: Vec::new(),
            placement: None,
        }
    }

    fn scene(entities: Vec<SceneEntity>) -> Scene {
        let mut bounds = empty_box();
        for e in &entities {
            union_box(&mut bounds, &e.bbox);
        }
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: Some(Sheet { name: String::new(), size: "A3".into(), width: 420.0, height: 297.0 }),
            bounds,
            layers: vec![SceneLayer { id: 0, name: "0".into(), visible: true, group: None }],
            entities,
            underlays: Vec::new(),
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    fn plan(moved: (f64, f64)) -> Scene {
        let (dx, dy) = moved;
        scene(vec![
            entity("1", "線分", vec![stroke(&[10.0 + dx, 10.0 + dy, 100.0 + dx, 10.0 + dy], false, "#000000")]),
            entity(
                "2",
                "折線",
                vec![stroke(&[0.0 + dx, 0.0 + dy, 50.0 + dx, 0.0 + dy, 50.0 + dx, 50.0 + dy], true, "#000000")],
            ),
            entity("3", "文字", vec![text("平面図", 200.0 + dx, 20.0 + dy)]),
            entity("4", "文字", vec![text("S=1/100", 200.0 + dx, 30.0 + dy)]),
            entity("5", "文字", vec![text("図面番号 A-01", 300.0 + dx, 20.0 + dy)]),
        ])
    }

    fn ids(scene: &Scene, list: &[usize]) -> Vec<String> {
        list.iter().map(|&i| scene.entities[i].id.clone()).collect()
    }

    #[test]
    fn matches_the_same_shapes_whatever_their_ids_direction_and_start() {
        let a = plan((0.0, 0.0));
        let mut b = plan((0.0, 0.0));
        // 逆向きの線分、始点の違う閉じた折線、並べ替えた図形、違う id
        b.entities[0].primitives = vec![stroke(&[100.0, 10.0, 10.0, 10.0], false, "#000000")];
        b.entities[1].primitives = vec![stroke(&[50.0, 50.0, 0.0, 0.0, 50.0, 0.0, 50.0, 50.0], true, "#000000")];
        b.entities.reverse();
        for (i, entity) in b.entities.iter_mut().enumerate() {
            entity.id = format!("#{}", 100 + i);
        }
        let diff = compare_scenes(&a, &b, &CompareOptions::default());
        assert_eq!(diff.unchanged.len(), 5);
        assert!(diff.removed.is_empty() && diff.added.is_empty() && diff.changed.is_empty());
        assert!(diff.regions.is_empty());
        assert_eq!(diff.offset, (0.0, 0.0));
    }

    #[test]
    fn finds_removed_added_and_changed_entities_and_their_regions() {
        let a = plan((0.0, 0.0));
        let mut b = plan((0.0, 0.0));
        b.entities[3].primitives = vec![text("S=1/200", 200.0, 30.0)]; // 文字の変更
        b.entities.remove(0); // 削除
        b.entities.push(entity("9", "線分", vec![stroke(&[300.0, 200.0, 350.0, 250.0], false, "#000000")])); // 追加
        let diff = compare_scenes(&a, &b, &CompareOptions::default());
        assert_eq!(ids(&a, &diff.removed), ["1"]);
        assert_eq!(ids(&b, &diff.added), ["9"]);
        assert_eq!(
            diff.changed
                .iter()
                .map(|&(x, y)| (a.entities[x].id.as_str(), b.entities[y].id.as_str()))
                .collect::<Vec<_>>(),
            [("4", "4")]
        );
        assert_eq!(diff.unchanged.len(), 3);
        // 線分の削除と文字の変更は離れているので別の場所、追加の線分も別の場所
        assert_eq!(diff.regions.len(), 3);
        assert!(diff.regions.iter().all(|r| r.removed + r.added + r.changed == 1));
    }

    #[test]
    fn treats_coordinates_within_the_tolerance_as_the_same() {
        let a = scene(vec![entity("1", "線分", vec![stroke(&[0.004_9, 0.0, 10.0, 0.0], false, "#000000")])]);
        let b = scene(vec![entity("1", "線分", vec![stroke(&[0.005_1, 0.0, 10.0, 0.0], false, "#000000")])]);
        let diff = compare_scenes(&a, &b, &CompareOptions::default());
        assert_eq!((diff.unchanged.len(), diff.changed.len()), (1, 0));
        let c = scene(vec![entity("1", "線分", vec![stroke(&[0.5, 0.0, 10.0, 0.0], false, "#000000")])]);
        let diff = compare_scenes(&a, &c, &CompareOptions::default());
        assert_eq!((diff.unchanged.len(), diff.changed.len()), (0, 1));
    }

    #[test]
    fn compares_styles_only_when_asked() {
        let a = plan((0.0, 0.0));
        let mut b = plan((0.0, 0.0));
        b.entities[0].primitives = vec![stroke(&[10.0, 10.0, 100.0, 10.0], false, "#ff0000")];
        assert_eq!(compare_scenes(&a, &b, &CompareOptions::default()).unchanged.len(), 5);
        let styled = compare_scenes(&a, &b, &CompareOptions { style: true, ..CompareOptions::default() });
        assert_eq!((styled.unchanged.len(), styled.changed.len()), (4, 1));
    }

    #[test]
    fn aligns_a_revision_that_moved_as_a_whole() {
        let a = plan((0.0, 0.0));
        let b = plan((1000.0, -250.5));
        let diff = compare_scenes(&a, &b, &CompareOptions::default());
        assert_eq!(diff.unchanged.len(), 5);
        assert!((diff.offset.0 - 1000.0).abs() < 1e-9 && (diff.offset.1 + 250.5).abs() < 1e-9);
        let fixed = compare_scenes(&a, &b, &CompareOptions { align: false, ..CompareOptions::default() });
        assert_eq!(fixed.unchanged.len(), 0);
    }

    #[test]
    fn draws_the_comparison_as_a_scene_in_three_colors() {
        let a = plan((0.0, 0.0));
        let mut b = plan((5.0, 5.0));
        b.entities.remove(0);
        let diff = compare_scenes(&a, &b, &CompareOptions::default());
        let overlay = diff_scene(&a, &b, &diff);
        assert_eq!(
            overlay.layers.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(),
            ["変更なし", "削除・変更前", "追加・変更後"]
        );
        let removed = overlay.entities.iter().find(|e| e.id == "A:1").unwrap();
        assert_eq!(removed.layers, [1]);
        assert_eq!(removed.properties[0], Property::new("comparison", "比較", "削除").raw("removed"));
        let Primitive::Stroke(line) = &removed.primitives[0] else { panic!() };
        assert_eq!(line.color, BEFORE_COLOR);
        // 後の版は前の版の座標に戻して描く
        let kept = overlay.entities.iter().find(|e| e.id == "B:3").unwrap();
        let Primitive::Text(label) = &kept.primitives[0] else { panic!() };
        assert_eq!((label.matrix[4], label.matrix[5], label.color.as_str()), (200.0, 20.0, UNCHANGED_COLOR));
        assert_eq!(kept.bbox.min_x, 200.0);
    }
}
