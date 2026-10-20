//! 線で閉じた領域（部屋など）を、描いた線から求め、その面積と中の文字を返す。
//!
//! 壁・間仕切りなどの線を交点と端点で分け、端点の近いものを 1 つの点にまとめて平面のグラフにし、グラフの面を
//! たどる。どこにもつながらない線（行き止まり）は面を作らないので除く。ほかの線とつながらない内側の輪
//! （柱・設備など）は、それを囲む面の穴になる。縮尺の違う配置（部分図・レイヤグループ・ビューポート）の線は
//! つながないので、配置ごとに別のグラフにする。座標は用紙の mm で、配置の中なら元の座標（実寸の mm）でも返す。
//!
//! 何を部屋とみなすかは決めない。面はすべて返し、中にある文字（室名など）を添える。

use crate::extract::{EntityFilter, EntityText, entity_text, select_entities};
use crate::geometry::{distance_to_segment_squared, empty_box, extend_box, inside_rings, transform_points};
use crate::types::{BBox, EntityKind, Placement, Primitive, Scene, SceneEntity};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

/// 既定で領域の境界にする図形の種類。寸法・文字・引出線・ハッチング・ビューポートの枠などは使わない
pub const BOUNDARY_KINDS: [EntityKind; 10] = [
    EntityKind::Line,
    EntityKind::Polyline,
    EntityKind::Multiline,
    EntityKind::Arc,
    EntityKind::Circle,
    EntityKind::Ellipse,
    EntityKind::EllipseArc,
    EntityKind::Spline,
    EntityKind::Clothoid,
    EntityKind::Solid,
];

/// 中の文字として添える図形の種類
const TEXT_KINDS: [EntityKind; 2] = [EntityKind::Text, EntityKind::Attribute];

/// 領域を求める条件
#[derive(Debug, Clone, PartialEq)]
pub struct RegionOptions {
    /// 境界にする図形と添える文字を選ぶ範囲と、描かないレイヤ（`extract::select_entities` と同じ）
    pub filter: EntityFilter,
    /// 境界にする図形の種類。None なら `BOUNDARY_KINDS`
    pub kinds: Option<Vec<EntityKind>>,
    /// 境界にする図形のレイヤの id。図形か配置元のレイヤが 1 つでも入っていれば使う。None ならすべて
    pub layers: Option<HashSet<u32>>,
    /// この距離（用紙の mm）より近い点は同じ点とし、線からこの距離にある端点はその線につながっているとみなす
    pub tolerance: f64,
    /// これより狭い領域（用紙の mm²、穴を除いた面積）は返さない
    pub min_area: f64,
}

impl Default for RegionOptions {
    fn default() -> Self {
        RegionOptions { filter: EntityFilter::default(), kinds: None, layers: None, tolerance: 0.01, min_area: 0.0 }
    }
}

/// 領域の中の文字
#[derive(Debug, Clone, PartialEq)]
pub struct RegionText {
    /// 文字の図形の id
    pub entity: String,
    pub text: String,
    /// 文字範囲の中心（用紙の mm）
    pub x: f64,
    pub y: f64,
}

/// 領域の形と大きさ。座標は x, y を交互に並べる
#[derive(Debug, Clone, PartialEq)]
pub struct RegionShape {
    /// 外形（反時計回り）
    pub outline: Vec<f64>,
    /// 穴（時計回り）
    pub holes: Vec<Vec<f64>>,
    /// 穴を除いた面積
    pub area: f64,
    /// 外形の囲む面積（穴を除かない）
    pub outline_area: f64,
    /// 外形の長さ
    pub perimeter: f64,
}

/// 線で閉じた領域
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedRegion {
    /// 用紙の mm の形と大きさ（面積は mm²）
    pub paper: RegionShape,
    /// 配置の元の座標（実寸の mm）の形と大きさ（面積は mm²）。配置の外の線でできた領域は None
    pub real: Option<RegionShape>,
    /// 外形と穴の外接矩形（用紙の mm）
    pub bbox: BBox,
    /// 領域を作った線の配置。配置の外なら None
    pub placement: Option<Arc<Placement>>,
    /// 外形と穴の線を描いた図形の id（重ならないように、最初に現れた順）
    pub boundary: Vec<String>,
    /// 中にある文字（穴の中は除く）。描く順
    pub texts: Vec<RegionText>,
}

/// 線で閉じた領域を、配置ごとに、上から下・左から右の順に返す
pub fn closed_regions(scene: &Scene, options: &RegionOptions) -> Vec<ClosedRegion> {
    let tolerance = if options.tolerance.is_finite() && options.tolerance > 0.0 { options.tolerance } else { 0.01 };
    let kinds: &[EntityKind] = options.kinds.as_deref().unwrap_or(&BOUNDARY_KINDS);
    let selected = select_entities(scene, &options.filter);

    // 配置ごとに線分を集める
    let mut groups: Vec<Group> = Vec::new();
    let mut group_of: HashMap<Option<[u64; 8]>, usize> = HashMap::new();
    for entity in &selected {
        if !kinds.contains(&entity.kind) {
            continue;
        }
        if let Some(layers) = &options.layers
            && !entity.layers.iter().any(|layer| layers.contains(layer))
        {
            continue;
        }
        let key = entity.placement.as_deref().map(placement_key);
        let index = *group_of.entry(key).or_insert_with(|| {
            groups.push(Group { placement: entity.placement.clone(), segments: Vec::new(), entities: Vec::new() });
            groups.len() - 1
        });
        let group = &mut groups[index];
        let source = group.entities.len();
        group.entities.push(entity.id.clone());
        add_segments(entity, source, &mut group.segments);
    }

    let texts: Vec<(&SceneEntity, EntityText)> = selected
        .iter()
        .filter(|entity| TEXT_KINDS.contains(&entity.kind))
        .filter_map(|entity| Some((*entity, entity_text(entity)?)))
        .collect();

    let mut regions = Vec::new();
    for group in &groups {
        let faces = faces(&group.segments, tolerance);
        for face in faces {
            if face.area < options.min_area || face.area <= tolerance * tolerance {
                continue;
            }
            let mut bbox = empty_box();
            for ring in std::iter::once(&face.outline).chain(&face.holes) {
                for [x, y] in ring.as_chunks::<2>().0 {
                    extend_box(&mut bbox, *x, *y);
                }
            }
            let real = group.placement.as_ref().map(|placement| {
                let outline = transform_points(&placement.inverse, &face.outline);
                let holes: Vec<Vec<f64>> =
                    face.holes.iter().map(|hole| transform_points(&placement.inverse, hole)).collect();
                shape(outline, holes)
            });
            let boundary = face.sources.iter().map(|&source| group.entities[source].clone()).collect();
            regions.push(ClosedRegion {
                paper: shape(face.outline, face.holes),
                real,
                bbox,
                placement: group.placement.clone(),
                boundary,
                texts: Vec::new(),
            });
        }
    }

    // 文字は、同じ配置でその中心を含む領域に添える。異なる配置の面（用紙の枠と部分図など）は重なることがある
    let grid = BoxGrid::new(regions.iter().enumerate().map(|(index, region)| (index, region.bbox)).collect());
    for (entity, text) in &texts {
        let x = (text.bbox.min_x + text.bbox.max_x) / 2.0;
        let y = (text.bbox.min_y + text.bbox.max_y) / 2.0;
        let found = grid.at(x, y).filter(|&index| {
            let region = &regions[index];
            region.placement.as_deref().map(placement_key) == entity.placement.as_deref().map(placement_key)
                && inside_rings(x, y, std::slice::from_ref(&region.paper.outline))
                && !region.paper.holes.iter().any(|hole| inside_rings(x, y, std::slice::from_ref(hole)))
        });
        if let Some(index) = found.min() {
            regions[index].texts.push(RegionText { entity: entity.id.clone(), text: text.text.clone(), x, y });
        }
    }

    regions.sort_by(|a, b| {
        let group = |region: &ClosedRegion| {
            region.placement.as_deref().map(placement_key).and_then(|key| group_of.get(&Some(key)).copied())
        };
        group(a).cmp(&group(b)).then(b.bbox.max_y.total_cmp(&a.bbox.max_y)).then(a.bbox.min_x.total_cmp(&b.bbox.min_x))
    });
    regions
}

/// 配置の縮尺と変換が同じなら同じ配置とみなす
fn placement_key(placement: &Placement) -> [u64; 8] {
    let m = placement.inverse;
    [
        placement.scale_x.to_bits(),
        placement.scale_y.to_bits(),
        m[0].to_bits(),
        m[1].to_bits(),
        m[2].to_bits(),
        m[3].to_bits(),
        m[4].to_bits(),
        m[5].to_bits(),
    ]
}

struct Group {
    placement: Option<Arc<Placement>>,
    segments: Vec<Segment>,
    /// 線分の source が指す図形の id
    entities: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    source: usize,
}

/// 図形の線と、塗り・ハッチングの輪を線分にする
fn add_segments(entity: &SceneEntity, source: usize, segments: &mut Vec<Segment>) {
    let mut push = |points: &[f64], closed: bool, pairs: bool| {
        let n = points.len() / 2;
        if n < 2 {
            return;
        }
        let mut add = |i: usize, j: usize| {
            let segment =
                Segment { x1: points[i * 2], y1: points[i * 2 + 1], x2: points[j * 2], y2: points[j * 2 + 1], source };
            if [segment.x1, segment.y1, segment.x2, segment.y2].iter().all(|v| v.is_finite()) {
                segments.push(segment);
            }
        };
        if pairs {
            for i in (0..n - 1).step_by(2) {
                add(i, i + 1);
            }
            return;
        }
        for i in 0..n - 1 {
            add(i, i + 1);
        }
        if closed && n > 2 {
            add(n - 1, 0);
        }
    };
    for primitive in &entity.primitives {
        match primitive {
            Primitive::Stroke(stroke) => push(&stroke.points, stroke.closed, stroke.segments == Some(true)),
            Primitive::Fill(fill) => fill.rings.iter().for_each(|ring| push(ring, true, false)),
            Primitive::Clipped(clipped) => clipped.rings.iter().for_each(|ring| push(ring, true, false)),
            _ => {}
        }
    }
}

/// グラフの面
struct Face {
    outline: Vec<f64>,
    holes: Vec<Vec<f64>>,
    /// 穴を除いた面積（用紙の mm²）
    area: f64,
    sources: Vec<usize>,
}

/// 線分の集まりから、閉じた面（内側の面）を求める
fn faces(segments: &[Segment], tolerance: f64) -> Vec<Face> {
    if segments.is_empty() {
        return Vec::new();
    }
    let graph = Graph::build(segments, tolerance);
    graph.faces(tolerance)
}

/// 平面のグラフ。点は位置、辺は点の組と、その辺を描いた図形
struct Graph {
    points: Vec<(f64, f64)>,
    /// 点から出る半辺（行き先の点と辺の番号）を角度の順に並べたもの
    outgoing: Vec<Vec<(usize, usize)>>,
    /// 辺の両端の点と図形
    edges: Vec<(usize, usize, usize)>,
}

/// 点を許容差の格子でまとめる
struct PointIndex {
    tolerance: f64,
    cells: FastMap<(i64, i64), Vec<usize>>,
    points: Vec<(f64, f64)>,
}

impl PointIndex {
    fn cell(&self, x: f64, y: f64) -> (i64, i64) {
        ((x / self.tolerance).floor() as i64, (y / self.tolerance).floor() as i64)
    }

    /// 許容差の内にある点の番号。なければ加える
    fn find_or_add(&mut self, x: f64, y: f64) -> usize {
        let (cx, cy) = self.cell(x, y);
        let limit = self.tolerance * self.tolerance;
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                // 極小の許容差や大きな座標は格子番号を i64 の端に丸める。隣のない側を調べない
                let (Some(nx), Some(ny)) = (cx.checked_add(dx), cy.checked_add(dy)) else { continue };
                if let Some(ids) = self.cells.get(&(nx, ny)) {
                    for &id in ids {
                        let (px, py) = self.points[id];
                        let d = (px - x) * (px - x) + (py - y) * (py - y);
                        if d <= limit && best.is_none_or(|(b, _)| d < b) {
                            best = Some((d, id));
                        }
                    }
                }
            }
        }
        if let Some((_, id)) = best {
            return id;
        }
        let id = self.points.len();
        self.points.push((x, y));
        self.cells.entry((cx, cy)).or_default().push(id);
        id
    }
}

impl Graph {
    fn build(segments: &[Segment], tolerance: f64) -> Graph {
        let splits = split_parameters(segments, tolerance);
        let mut index = PointIndex { tolerance, cells: FastMap::default(), points: Vec::new() };
        let mut edge_of: FastMap<(usize, usize), usize> = FastMap::default();
        let mut edges: Vec<(usize, usize, usize)> = Vec::new();
        let mut last: Option<(f64, f64, usize)> = None;
        for (segment, params) in segments.iter().zip(&splits) {
            let mut previous: Option<usize> = None;
            for &t in params {
                let x = segment.x1 + (segment.x2 - segment.x1) * t;
                let y = segment.y1 + (segment.y2 - segment.y1) * t;
                // 折線の続く線分は、前の線分の終点から始まる
                let id = match last {
                    Some((lx, ly, id)) if lx == x && ly == y => id,
                    _ => index.find_or_add(x, y),
                };
                last = Some((x, y, id));
                if let Some(from) = previous
                    && from != id
                {
                    let key = (from.min(id), from.max(id));
                    edge_of.entry(key).or_insert_with(|| {
                        edges.push((key.0, key.1, segment.source));
                        edges.len() - 1
                    });
                }
                previous = Some(id);
            }
        }
        let points = index.points;

        // 行き止まりの辺を、つながる先がなくなるまで除く
        let mut degree = vec![0usize; points.len()];
        let mut touching: Vec<Vec<usize>> = vec![Vec::new(); points.len()];
        for (e, &(a, b, _)) in edges.iter().enumerate() {
            degree[a] += 1;
            degree[b] += 1;
            touching[a].push(e);
            touching[b].push(e);
        }
        let mut removed = vec![false; edges.len()];
        let mut queue: Vec<usize> = (0..points.len()).filter(|&p| degree[p] == 1).collect();
        while let Some(p) = queue.pop() {
            if degree[p] != 1 {
                continue;
            }
            let Some(&e) = touching[p].iter().find(|&&e| !removed[e]) else { continue };
            removed[e] = true;
            let (a, b, _) = edges[e];
            degree[a] -= 1;
            degree[b] -= 1;
            let other = if a == p { b } else { a };
            if degree[other] == 1 {
                queue.push(other);
            }
        }

        let mut outgoing: Vec<Vec<(usize, usize)>> = vec![Vec::new(); points.len()];
        for (e, &(a, b, _)) in edges.iter().enumerate() {
            if !removed[e] {
                outgoing[a].push((b, e));
                outgoing[b].push((a, e));
            }
        }
        for (p, list) in outgoing.iter_mut().enumerate() {
            let (px, py) = points[p];
            list.sort_by(|&(a, _), &(b, _)| {
                let angle = |q: usize| (points[q].1 - py).atan2(points[q].0 - px);
                angle(a).total_cmp(&angle(b))
            });
        }
        Graph { points, outgoing, edges }
    }

    /// 面をたどる。半辺 u→v の左の面を反時計回りにたどり、v では v→u の 1 つ手前（時計回りに次）の半辺に進む
    fn faces(&self, tolerance: f64) -> Vec<Face> {
        // 半辺の番号: 点 p の outgoing の i 番目は offset[p] + i
        let mut offset = Vec::with_capacity(self.points.len() + 1);
        let mut total = 0;
        for list in &self.outgoing {
            offset.push(total);
            total += list.len();
        }
        offset.push(total);
        // 半辺 (p, i) の逆向きの半辺の位置
        let position = |from: usize, to: usize, edge: usize| -> usize {
            self.outgoing[from].iter().position(|&(q, e)| q == to && e == edge).unwrap_or(0)
        };
        let mut visited = vec![false; total];
        let mut cycles: Vec<Cycle> = Vec::new();
        for start in 0..self.points.len() {
            for i in 0..self.outgoing[start].len() {
                if visited[offset[start] + i] {
                    continue;
                }
                let mut ring = Vec::new();
                let mut sources = Vec::new();
                let (mut p, mut k) = (start, i);
                let mut steps = 0;
                loop {
                    let half = offset[p] + k;
                    if visited[half] || steps > total {
                        break;
                    }
                    visited[half] = true;
                    steps += 1;
                    let (q, edge) = self.outgoing[p][k];
                    ring.push(self.points[p].0);
                    ring.push(self.points[p].1);
                    let source = self.edges[edge].2;
                    if !sources.contains(&source) {
                        sources.push(source);
                    }
                    let back = position(q, p, edge);
                    let len = self.outgoing[q].len();
                    k = (back + len - 1) % len;
                    p = q;
                }
                let area = signed_area(&ring);
                cycles.push(Cycle { ring, sources, area, component: start });
            }
        }
        // 点のつながり（連結成分）
        let component = self.components();
        for cycle in &mut cycles {
            cycle.component = component[cycle.component];
        }

        let epsilon = tolerance * tolerance;
        let inner: Vec<usize> = (0..cycles.len()).filter(|&c| cycles[c].area > epsilon).collect();
        let boxes: Vec<BBox> = cycles.iter().map(|cycle| ring_box(&cycle.ring)).collect();
        // 外側の輪（負の面積）は、ほかの成分の面のうち、それを含む最も小さいものの穴になる。面は外接矩形で格子に入れて探す
        let grid = BoxGrid::new(inner.iter().map(|&f| (f, boxes[f])).collect());
        let mut holes: HashMap<usize, Vec<usize>> = HashMap::new();
        for (c, cycle) in cycles.iter().enumerate() {
            if cycle.area >= -epsilon {
                continue;
            }
            let (x, y) = (cycle.ring[0], cycle.ring[1]);
            let container = grid
                .at(x, y)
                .filter(|&f| cycles[f].component != cycle.component)
                .filter(|&f| inside_rings(x, y, std::slice::from_ref(&cycles[f].ring)))
                .min_by(|&a, &b| cycles[a].area.total_cmp(&cycles[b].area).then(a.cmp(&b)));
            if let Some(f) = container {
                holes.entry(f).or_default().push(c);
            }
        }

        inner
            .into_iter()
            .map(|f| {
                let cycle = &cycles[f];
                let mut sources = cycle.sources.clone();
                let mut area = cycle.area;
                let mut rings = Vec::new();
                for &h in holes.get(&f).map(Vec::as_slice).unwrap_or_default() {
                    area += cycles[h].area;
                    for &source in &cycles[h].sources {
                        if !sources.contains(&source) {
                            sources.push(source);
                        }
                    }
                    rings.push(simplify(&cycles[h].ring, tolerance));
                }
                Face { outline: simplify(&cycle.ring, tolerance), holes: rings, area, sources }
            })
            .collect()
    }

    /// 点ごとの連結成分の代表
    fn components(&self) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.points.len()).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for (p, list) in self.outgoing.iter().enumerate() {
            for &(q, _) in list {
                let (a, b) = (find(&mut parent, p), find(&mut parent, q));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
        (0..self.points.len()).map(|p| find(&mut parent, p)).collect()
    }
}

/// 格子のマスと点の番号のための速いハッシュ（FxHash と同じ計算）。鍵は図面から作るが、衝突しても遅くなるだけ
#[derive(Default)]
struct FastHasher(u64);

impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_i64(&mut self, value: i64) {
        self.write_u64(value as u64);
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// 外接矩形を格子のマスに入れ、点を含むものを探す
struct BoxGrid {
    bounds: BBox,
    side: f64,
    cells: FastMap<(i64, i64), Vec<usize>>,
    boxes: FastMap<usize, BBox>,
}

impl BoxGrid {
    fn new(items: Vec<(usize, BBox)>) -> BoxGrid {
        let mut bounds = empty_box();
        for (_, b) in &items {
            extend_box(&mut bounds, b.min_x, b.min_y);
            extend_box(&mut bounds, b.max_x, b.max_y);
        }
        let width = (bounds.max_x - bounds.min_x).max(1e-9);
        let height = (bounds.max_y - bounds.min_y).max(1e-9);
        let count = items.len().max(1) as f64;
        let side = (width * height / count).sqrt().max(width / 1024.0).max(height / 1024.0);
        let mut grid = BoxGrid { bounds, side, cells: FastMap::default(), boxes: FastMap::default() };
        for (id, b) in items {
            let (x0, y0) = grid.cell(b.min_x, b.min_y);
            let (x1, y1) = grid.cell(b.max_x, b.max_y);
            for cx in x0..=x1 {
                for cy in y0..=y1 {
                    grid.cells.entry((cx, cy)).or_default().push(id);
                }
            }
            grid.boxes.insert(id, b);
        }
        grid
    }

    fn cell(&self, x: f64, y: f64) -> (i64, i64) {
        (((x - self.bounds.min_x) / self.side).floor() as i64, ((y - self.bounds.min_y) / self.side).floor() as i64)
    }

    /// 外接矩形が点を含むもの
    fn at(&self, x: f64, y: f64) -> impl Iterator<Item = usize> + '_ {
        self.cells.get(&self.cell(x, y)).into_iter().flatten().copied().filter(move |id| {
            let b = &self.boxes[id];
            x >= b.min_x && x <= b.max_x && y >= b.min_y && y <= b.max_y
        })
    }
}

struct Cycle {
    ring: Vec<f64>,
    sources: Vec<usize>,
    area: f64,
    /// 連結成分（たどり始めた点で、あとで成分の代表に置き換える）
    component: usize,
}

/// 線分を分ける位置（0 から 1 の媒介変数）。両端、ほかの線分との交点、ほかの線分の端点が近くにある位置
fn split_parameters(segments: &[Segment], tolerance: f64) -> Vec<Vec<f64>> {
    let mut splits: Vec<Vec<f64>> = vec![vec![0.0, 1.0]; segments.len()];
    let mut bounds = empty_box();
    for s in segments {
        extend_box(&mut bounds, s.x1, s.y1);
        extend_box(&mut bounds, s.x2, s.y2);
    }
    // 線分の数に合わせた格子。1 辺のマスは 2048 まで
    let width = (bounds.max_x - bounds.min_x).max(tolerance);
    let height = (bounds.max_y - bounds.min_y).max(tolerance);
    let side =
        ((width * height / segments.len() as f64).sqrt()).max(width / 2048.0).max(height / 2048.0).max(tolerance * 4.0);
    let cell = |x: f64, y: f64| -> (i64, i64) {
        (((x - bounds.min_x) / side).floor() as i64, ((y - bounds.min_y) / side).floor() as i64)
    };
    let expanded = |s: &Segment| -> BBox {
        BBox {
            min_x: s.x1.min(s.x2) - tolerance,
            min_y: s.y1.min(s.y2) - tolerance,
            max_x: s.x1.max(s.x2) + tolerance,
            max_y: s.y1.max(s.y2) + tolerance,
        }
    };
    let boxes: Vec<BBox> = segments.iter().map(expanded).collect();
    let mut grid: FastMap<(i64, i64), Vec<usize>> = FastMap::default();
    for (i, b) in boxes.iter().enumerate() {
        let (x0, y0) = cell(b.min_x, b.min_y);
        let (x1, y1) = cell(b.max_x, b.max_y);
        for cx in x0..=x1 {
            for cy in y0..=y1 {
                grid.entry((cx, cy)).or_default().push(i);
            }
        }
    }
    for (&here, ids) in &grid {
        for (n, &i) in ids.iter().enumerate() {
            for &j in &ids[n + 1..] {
                let (a, b) = (&boxes[i], &boxes[j]);
                if a.max_x < b.min_x || b.max_x < a.min_x || a.max_y < b.min_y || b.max_y < a.min_y {
                    continue;
                }
                // 2 つの矩形の重なりの左下のマスでだけ調べる（同じ組を 2 度調べない）
                if cell(a.min_x.max(b.min_x), a.min_y.max(b.min_y)) != here {
                    continue;
                }
                intersect(&segments[i], &segments[j], tolerance, &mut splits, i, j);
            }
        }
    }
    for params in &mut splits {
        params.sort_by(f64::total_cmp);
        params.dedup();
    }
    splits
}

/// 2 本の線分の交点と、互いの端点が近くにある位置を、それぞれの分ける位置に加える
fn intersect(a: &Segment, b: &Segment, tolerance: f64, splits: &mut [Vec<f64>], i: usize, j: usize) {
    let limit = tolerance * tolerance;
    // 端点がもう一方の線分の近くにあれば、そこで分ける（T 字の接合と、重なった線）
    for (s, t, target) in [(a, b, i), (b, a, j)] {
        for (px, py) in [(t.x1, t.y1), (t.x2, t.y2)] {
            if distance_to_segment_squared(px, py, s.x1, s.y1, s.x2, s.y2) <= limit {
                splits[target].push(project(s, px, py));
            }
        }
    }
    // 線分の内側で交わる点
    let (dx1, dy1) = (a.x2 - a.x1, a.y2 - a.y1);
    let (dx2, dy2) = (b.x2 - b.x1, b.y2 - b.y1);
    let denominator = dx1 * dy2 - dy1 * dx2;
    let (length1, length2) = (dx1.hypot(dy1), dx2.hypot(dy2));
    if length1 == 0.0 || length2 == 0.0 || denominator.abs() <= 1e-12 * length1 * length2 {
        return;
    }
    let (ex, ey) = (b.x1 - a.x1, b.y1 - a.y1);
    let t = (ex * dy2 - ey * dx2) / denominator;
    let u = (ex * dy1 - ey * dx1) / denominator;
    let (slack1, slack2) = (tolerance / length1, tolerance / length2);
    if t >= -slack1 && t <= 1.0 + slack1 && u >= -slack2 && u <= 1.0 + slack2 {
        splits[i].push(t.clamp(0.0, 1.0));
        splits[j].push(u.clamp(0.0, 1.0));
    }
}

/// 点を線分に下ろした位置（0 から 1）
fn project(s: &Segment, x: f64, y: f64) -> f64 {
    let (dx, dy) = (s.x2 - s.x1, s.y2 - s.y1);
    let length = dx * dx + dy * dy;
    if length == 0.0 { 0.0 } else { (((x - s.x1) * dx + (y - s.y1) * dy) / length).clamp(0.0, 1.0) }
}

/// 輪の符号付き面積（反時計回りが正）
fn signed_area(ring: &[f64]) -> f64 {
    let n = ring.len() / 2;
    if n < 3 {
        return 0.0;
    }
    // 原点から遠い小さな領域でも、大きな座標の積どうしを引いて桁を失わないよう、最初の点を原点にする
    let (x, y) = (ring[0], ring[1]);
    let mut sum = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        sum += (ring[i * 2] - x) * (ring[j * 2 + 1] - y) - (ring[j * 2] - x) * (ring[i * 2 + 1] - y);
    }
    sum / 2.0
}

/// 輪の長さ
fn ring_length(ring: &[f64]) -> f64 {
    let n = ring.len() / 2;
    (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            (ring[j * 2] - ring[i * 2]).hypot(ring[j * 2 + 1] - ring[i * 2 + 1])
        })
        .sum()
}

fn ring_box(ring: &[f64]) -> BBox {
    let mut b = empty_box();
    for [x, y] in ring.as_chunks::<2>().0 {
        extend_box(&mut b, *x, *y);
    }
    b
}

/// 輪から、前後の点を結ぶ線の上にある点（線分を分けた点）を除く。折り返す点は残す
fn simplify(ring: &[f64], tolerance: f64) -> Vec<f64> {
    let mut points: Vec<(f64, f64)> = ring.as_chunks::<2>().0.iter().map(|[x, y]| (*x, *y)).collect();
    let limit = tolerance * tolerance;
    let mut changed = true;
    while changed && points.len() > 3 {
        changed = false;
        let mut i = 0;
        while i < points.len() && points.len() > 3 {
            let n = points.len();
            let (ax, ay) = points[(i + n - 1) % n];
            let (bx, by) = points[i];
            let (cx, cy) = points[(i + 1) % n];
            let forward = (bx - ax) * (cx - bx) + (by - ay) * (cy - by) > 0.0;
            if forward && distance_to_segment_squared(bx, by, ax, ay, cx, cy) <= limit {
                points.remove(i);
                changed = true;
            } else {
                i += 1;
            }
        }
    }
    points.into_iter().flat_map(|(x, y)| [x, y]).collect()
}

fn shape(outline: Vec<f64>, holes: Vec<Vec<f64>>) -> RegionShape {
    let outline_area = signed_area(&outline).abs();
    let area = outline_area - holes.iter().map(|hole| signed_area(hole).abs()).sum::<f64>();
    let perimeter = ring_length(&outline);
    RegionShape { outline, holes, area, outline_area, perimeter }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{invert, placement};
    use crate::types::{Background, SceneLayer, StrokePrimitive, TextPrimitive};

    fn stroke(points: &[f64], closed: bool) -> Primitive {
        Primitive::Stroke(StrokePrimitive {
            points: points.to_vec(),
            closed,
            segments: None,
            color: "#000000".into(),
            width: 0.0,
            dashes: Vec::new(),
        })
    }

    fn line(id: &str, layer: u32, points: &[f64], closed: bool) -> SceneEntity {
        let mut bbox = empty_box();
        for [x, y] in points.as_chunks::<2>().0 {
            extend_box(&mut bbox, *x, *y);
        }
        SceneEntity {
            id: id.into(),
            kind: if closed { EntityKind::Polyline } else { EntityKind::Line },
            type_name: "線".into(),
            source: "test".into(),
            layers: vec![layer],
            primitives: vec![stroke(points, closed)],
            bbox,
            properties: Vec::new(),
            placement: None,
        }
    }

    fn label(id: &str, text: &str, x: f64, y: f64) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: EntityKind::Text,
            type_name: "文字".into(),
            source: "test".into(),
            layers: vec![0],
            primitives: vec![Primitive::Text(TextPrimitive {
                text: text.into(),
                color: "#000000".into(),
                font_family: "sans-serif".into(),
                height: 2.0,
                scale_x: 1.0,
                cells: Vec::new(),
                box_width: 2.0 * text.chars().count() as f64,
                box_height: 2.0,
                matrix: placement(x, y, 0.0, 1.0, 1.0),
            })],
            bbox: BBox { min_x: x, min_y: y, max_x: x + 4.0, max_y: y + 2.0 },
            properties: Vec::new(),
            placement: None,
        }
    }

    fn scene(entities: Vec<SceneEntity>) -> Scene {
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: None,
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 200.0, max_y: 200.0 },
            layers: vec![
                SceneLayer { id: 0, name: "壁".into(), visible: true, group: None },
                SceneLayer { id: 1, name: "家具".into(), visible: true, group: None },
            ],
            entities,
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn translated_rooms_keep_their_area() {
        for origin in [0.0, 1e9, -1e9] {
            let points: Vec<f64> =
                [0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0].into_iter().map(|v| v + origin).collect();
            let regions = closed_regions(&scene(vec![line("room", 0, &points, true)]), &RegionOptions::default());
            assert_eq!(regions.len(), 1, "origin {origin}");
            assert!(close(regions[0].paper.area, 100.0), "origin {origin}: {}", regions[0].paper.area);
        }
    }

    #[test]
    fn tiny_tolerances_do_not_overflow_the_point_grid() {
        let room = line("room", 0, &[-10.0, -10.0, 10.0, -10.0, 10.0, 10.0, -10.0, 10.0], true);
        let options = RegionOptions { tolerance: 1e-20, ..RegionOptions::default() };
        let regions = closed_regions(&scene(vec![room]), &options);
        assert_eq!(regions.len(), 1);
        assert!(close(regions[0].paper.area, 400.0));
    }

    #[test]
    fn room_labels_stay_in_their_placement_when_regions_overlap() {
        let placed = Arc::new(Placement {
            name: "plan".into(),
            scale_x: 0.01,
            scale_y: 0.01,
            inverse: invert(&placement(0.0, 0.0, 0.0, 0.01, 0.01)),
        });
        let frame = line("frame", 0, &[0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 0.0, 100.0], true);
        let mut room = frame.clone();
        room.id = "room".into();
        room.placement = Some(placed.clone());
        let mut text = label("name", "Room", 20.0, 20.0);
        text.placement = Some(placed);
        let regions = closed_regions(&scene(vec![frame, room, text]), &RegionOptions::default());
        let room = regions.iter().find(|r| r.real.is_some()).unwrap();
        assert_eq!(room.texts.iter().map(|t| t.entity.as_str()).collect::<Vec<_>>(), ["name"]);
        assert!(regions.iter().find(|r| r.real.is_none()).unwrap().texts.is_empty());
    }

    #[test]
    fn finds_two_rooms_split_by_a_wall_that_ends_on_the_outline() {
        // 100 × 50 の外形を、x = 60 の線（両端が外形の辺の途中に接する T 字）で 2 つに分ける。
        // 外形の下の辺は 2 本の線が重なって描かれ、行き止まりの線も 1 本ある
        let s = scene(vec![
            line("outline", 0, &[0.0, 0.0, 100.0, 0.0, 100.0, 50.0, 0.0, 50.0], true),
            line("wall", 0, &[60.0, 0.0, 60.0, 50.0], false),
            line("overlap", 0, &[20.0, 0.0, 80.0, 0.0], false),
            line("stub", 0, &[10.0, 50.0, 10.0, 40.0], false),
            label("t1", "居室", 20.0, 20.0),
            label("t2", "納戸", 75.0, 20.0),
            label("outside", "外", 150.0, 20.0),
        ]);
        let regions = closed_regions(&s, &RegionOptions::default());
        assert_eq!(regions.len(), 2, "{regions:?}");
        let left = &regions[0];
        assert!(close(left.paper.area, 3000.0), "{}", left.paper.area);
        assert!(close(left.paper.perimeter, 220.0), "{}", left.paper.perimeter);
        assert_eq!(left.paper.outline.len(), 8, "{:?}", left.paper.outline);
        assert_eq!(left.texts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["居室"]);
        assert_eq!(left.texts[0].entity, "t1");
        assert!(left.boundary.contains(&"outline".to_string()) && left.boundary.contains(&"wall".to_string()));
        assert!(!left.boundary.contains(&"stub".to_string()));
        let right = &regions[1];
        assert!(close(right.paper.area, 2000.0), "{}", right.paper.area);
        assert_eq!(right.texts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["納戸"]);
        assert_eq!(left.real, None);
        // 外形は反時計回り
        assert!(signed_area(&left.paper.outline) > 0.0);
    }

    #[test]
    fn crossing_lines_split_a_room_and_an_island_becomes_a_hole() {
        // 100 × 100 の部屋の中に、どの線ともつながらない柱（10 × 10）がある
        let s = scene(vec![
            line("room", 0, &[0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 0.0, 100.0], true),
            line("column", 0, &[40.0, 40.0, 50.0, 40.0, 50.0, 50.0, 40.0, 50.0], true),
            label("in-room", "ホール", 10.0, 10.0),
            label("in-column", "C1", 43.0, 44.0),
        ]);
        let regions = closed_regions(&s, &RegionOptions::default());
        assert_eq!(regions.len(), 2);
        let room = regions.iter().find(|r| r.paper.holes.len() == 1).unwrap();
        assert!(close(room.paper.area, 9900.0) && close(room.paper.outline_area, 10000.0));
        assert!(signed_area(&room.paper.holes[0]) < 0.0);
        assert_eq!(room.texts.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["ホール"]);
        let column = regions.iter().find(|r| r.paper.holes.is_empty()).unwrap();
        assert!(close(column.paper.area, 100.0));
        assert_eq!(column.texts[0].text, "C1");

        // 交差する 2 本の線（端が外形の外に出る）は、部屋を 4 つに分ける。外に出た部分は行き止まり
        let s = scene(vec![
            line("room", 0, &[0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 0.0, 100.0], true),
            line("h", 0, &[-10.0, 30.0, 110.0, 30.0], false),
            line("v", 0, &[70.0, -10.0, 70.0, 110.0], false),
        ]);
        let mut areas: Vec<f64> = closed_regions(&s, &RegionOptions::default()).iter().map(|r| r.paper.area).collect();
        areas.sort_by(f64::total_cmp);
        assert_eq!(areas.len(), 4);
        for (area, expected) in areas.iter().zip([900.0, 2100.0, 2100.0, 4900.0]) {
            assert!(close(*area, expected), "{areas:?}");
        }
    }

    #[test]
    fn closes_gaps_within_the_tolerance_and_filters_by_layer_and_area() {
        // 角の端点が 0.005 mm ずれている部屋と、別のレイヤの家具
        let s = scene(vec![
            line("a", 0, &[0.0, 0.0, 50.0, 0.0], false),
            line("b", 0, &[50.004, 0.003, 50.0, 40.0], false),
            line("c", 0, &[50.0, 40.0, 0.0, 40.0, 0.0, 0.0], false),
            line("table", 1, &[10.0, 10.0, 20.0, 10.0, 20.0, 20.0, 10.0, 20.0], true),
        ]);
        let all = closed_regions(&s, &RegionOptions::default());
        assert_eq!(all.len(), 2);
        let walls = RegionOptions { layers: Some(HashSet::from([0])), ..RegionOptions::default() };
        let regions = closed_regions(&s, &walls);
        assert_eq!(regions.len(), 1);
        assert!((regions[0].paper.area - 2000.0).abs() < 0.5, "{}", regions[0].paper.area);
        // 許容差より大きいすき間は閉じない
        let tight = RegionOptions { tolerance: 0.001, ..walls.clone() };
        assert!(closed_regions(&s, &tight).is_empty());
        let large = RegionOptions { min_area: 150.0, ..RegionOptions::default() };
        assert_eq!(closed_regions(&s, &large).len(), 1);
    }

    #[test]
    fn gives_real_areas_through_the_placement() {
        // 1/100 の部分図。用紙の 40 × 30 mm は実寸の 4 m × 3 m
        let to_paper = placement(10.0, 10.0, 0.0, 0.01, 0.01);
        let placed =
            Arc::new(Placement { name: "平面図".into(), scale_x: 0.01, scale_y: 0.01, inverse: invert(&to_paper) });
        let mut room = line("room", 0, &[10.0, 10.0, 50.0, 10.0, 50.0, 40.0, 10.0, 40.0], true);
        room.placement = Some(placed.clone());
        // 同じ形の、配置の外の線（用紙の枠など）は別に数え、つながない
        let frame = line("frame", 0, &[10.0, 10.0, 50.0, 10.0, 50.0, 40.0, 10.0, 40.0], true);
        let regions = closed_regions(&scene(vec![frame, room]), &RegionOptions::default());
        assert_eq!(regions.len(), 2);
        let real = regions.iter().find_map(|r| r.real.as_ref()).unwrap();
        assert!(close(real.area, 12_000_000.0), "{}", real.area);
        assert!(close(real.perimeter, 14_000.0));
        assert!(close(real.outline[0], 0.0) || close(real.outline[0], 4000.0));
        let paper = regions.iter().find(|r| r.real.is_none()).unwrap();
        assert!(close(paper.paper.area, 1200.0));
    }
}
