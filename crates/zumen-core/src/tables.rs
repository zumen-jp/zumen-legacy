//! 罫線で描いた表（面積表・建具表・図面リスト・表題欄など）を、描いた線と文字から求める。
//!
//! 表の罫線は水平か垂直なので、境界にする図形の線のうち水平・垂直の線分だけで閉じた領域
//! （`regions` と同じ面）を求め、長方形の面をます目とする。辺を共有するます目をつなぎ、つないだます目が
//! 隙間なく長方形を埋めるものを 1 つの表にする。行と列はます目の辺の位置から決め、複数の行・列にまたがる
//! ます目（結合したます目）はその数を持つ。文字は、同じ配置で文字範囲の中心を含むます目に入れる。
//!
//! 斜線（空欄を示す斜めの線）や丸などの記号は罫線にしないので、ます目を分けない。用紙に対して傾いた表は
//! 求めない。図枠の内側と、それに接する表題欄のように、1 つのます目が表の大半を占めるときは、そのます目を
//! 表から除く（[決定 0015](../../../docs/decisions/0015-analysis-in-zumen-core.md)）。

use crate::extract::{EntityFilter, EntityText, entity_text, select_entities};
use crate::geometry::{empty_box, extend_box};
use crate::regions::{BOUNDARY_KINDS, BoxGrid, ClosedRegion, RegionOptions, RegionText, region_faces, same_placement};
use crate::types::{BBox, EntityKind, Placement, Scene, SceneEntity};
use std::collections::HashSet;
use std::sync::Arc;

/// ます目に入れる文字の図形の種類
const TEXT_KINDS: [EntityKind; 2] = [EntityKind::Text, EntityKind::Attribute];

/// 表の大半を占めるます目: 表の面積のこの割合より広く、ほかのどのます目よりこの倍数より広い
const DOMINANT_SHARE: f64 = 0.5;
const DOMINANT_RATIO: f64 = 10.0;

/// 表を求める条件
#[derive(Debug, Clone, PartialEq)]
pub struct TableOptions {
    /// 罫線にする図形と文字を選ぶ範囲と、描かないレイヤ（`extract::select_entities` と同じ）
    pub filter: EntityFilter,
    /// 罫線にする図形のレイヤの id。図形か配置元のレイヤが 1 つでも入っていれば使う。None ならすべて
    pub layers: Option<HashSet<u32>>,
    /// この距離（用紙の mm）より近い点は同じ点とし、罫線の位置の違いもこの距離までは同じ位置とみなす
    pub tolerance: f64,
    /// これより少ないます目の表は返さない
    pub min_cells: usize,
    /// 文字のない表も返すか
    pub empty: bool,
}

impl Default for TableOptions {
    fn default() -> Self {
        TableOptions { filter: EntityFilter::default(), layers: None, tolerance: 0.01, min_cells: 2, empty: false }
    }
}

/// 表のます目
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    /// 上から数えた行と、左から数えた列（0 から）
    pub row: usize,
    pub column: usize,
    /// またがる行と列の数（結合していなければ 1）
    pub row_span: usize,
    pub column_span: usize,
    /// ます目の範囲（用紙の mm）
    pub bbox: BBox,
    /// ます目の文字を、上の行から、同じ行は左から並べたもの。同じ行の文字は空白で、行は改行でつなぐ
    pub text: String,
    /// ます目の中の文字（並びは `text` と同じ）
    pub texts: Vec<RegionText>,
}

/// 罫線で描いた表
#[derive(Debug, Clone, PartialEq)]
pub struct RuledTable {
    /// 表の範囲（用紙の mm）
    pub bbox: BBox,
    /// 罫線を描いた配置。配置の外なら None
    pub placement: Option<Arc<Placement>>,
    /// 列の境の罫線の x（左から。列の数 + 1 個）
    pub columns: Vec<f64>,
    /// 行の境の罫線の y（上から。行の数 + 1 個）
    pub rows: Vec<f64>,
    /// ます目。行の順、同じ行は列の順
    pub cells: Vec<TableCell>,
    /// 罫線を描いた図形の id（重ならないように、最初に現れた順）
    pub boundary: Vec<String>,
}

impl RuledTable {
    /// 行と列の数
    pub fn size(&self) -> (usize, usize) {
        (self.rows.len().saturating_sub(1), self.columns.len().saturating_sub(1))
    }

    /// 行ごとの文字。結合したます目の文字は左上の位置に入れ、ほかの位置とます目のない位置は空にする
    pub fn grid(&self) -> Vec<Vec<String>> {
        let (rows, columns) = self.size();
        let mut grid = vec![vec![String::new(); columns]; rows];
        for cell in &self.cells {
            grid[cell.row][cell.column].clone_from(&cell.text);
        }
        grid
    }
}

/// 罫線で描いた表を、配置ごとに、上から下・左から右の順に返す
pub fn ruled_tables(scene: &Scene, options: &TableOptions) -> Vec<RuledTable> {
    let tolerance = if options.tolerance.is_finite() && options.tolerance > 0.0 { options.tolerance } else { 0.01 };
    let selected = select_entities(scene, &options.filter);
    let region_options = RegionOptions {
        filter: options.filter.clone(),
        kinds: Some(BOUNDARY_KINDS.to_vec()),
        layers: options.layers.clone(),
        tolerance,
        min_area: 0.0,
    };
    // 水平か垂直の線分だけを罫線にする
    let faces = region_faces(&selected, &region_options, |s| {
        (s.x2 - s.x1).abs() <= tolerance || (s.y2 - s.y1).abs() <= tolerance
    });

    // 長方形の面がます目になる
    let cells: Vec<(usize, ClosedRegion)> = faces
        .into_iter()
        .filter(|(_, face)| {
            let b = &face.bbox;
            let perimeter = 2.0 * ((b.max_x - b.min_x) + (b.max_y - b.min_y));
            ((b.max_x - b.min_x) * (b.max_y - b.min_y) - face.paper.outline_area).abs() <= tolerance * perimeter
        })
        .collect();

    // 辺を共有するます目をつなぐ
    let grid = BoxGrid::new(cells.iter().enumerate().map(|(index, (_, cell))| (index, cell.bbox)).collect());
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    for (index, (group, cell)) in cells.iter().enumerate() {
        let b = cell.bbox;
        let area = BBox {
            min_x: b.min_x - tolerance,
            min_y: b.min_y - tolerance,
            max_x: b.max_x + tolerance,
            max_y: b.max_y + tolerance,
        };
        for other in grid.overlapping(area) {
            if other <= index || cells[other].0 != *group || !share_edge(&b, &cells[other].1.bbox, tolerance) {
                continue;
            }
            let (a, c) = (find(&mut parent, index), find(&mut parent, other));
            if a != c {
                parent[a.max(c)] = a.min(c);
            }
        }
    }
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    let mut cluster_of = vec![usize::MAX; cells.len()];
    for index in 0..cells.len() {
        let root = find(&mut parent, index);
        if cluster_of[root] == usize::MAX {
            cluster_of[root] = clusters.len();
            clusters.push(Vec::new());
        }
        clusters[cluster_of[root]].push(index);
    }

    // 長方形を埋めるものを表にする。大半を占めるます目は除き、残りを分け直して調べる
    let mut tables: Vec<(usize, Vec<usize>)> = Vec::new();
    while let Some(members) = clusters.pop() {
        if members.len() < options.min_cells.max(1) {
            continue;
        }
        let boxes: Vec<BBox> = members.iter().map(|&m| cells[m].1.bbox).collect();
        let total = union_box(&boxes);
        let mut areas: Vec<(f64, usize)> = boxes.iter().enumerate().map(|(i, b)| (box_area(b), i)).collect();
        areas.sort_by(|a, b| b.0.total_cmp(&a.0));
        let whole = box_area(&total);
        if areas.len() > 1 && areas[0].0 > whole * DOMINANT_SHARE && areas[0].0 >= areas[1].0 * DOMINANT_RATIO {
            let rest: Vec<usize> =
                members.iter().enumerate().filter(|&(i, _)| i != areas[0].1).map(|(_, &m)| m).collect();
            clusters.extend(split_connected(&rest, &cells, tolerance));
            continue;
        }
        let perimeters: f64 = boxes.iter().map(|b| 2.0 * ((b.max_x - b.min_x) + (b.max_y - b.min_y))).sum();
        let filled: f64 = areas.iter().map(|(area, _)| area).sum();
        if (whole - filled).abs() > tolerance * perimeters {
            continue;
        }
        tables.push((cells[members[0]].0, members));
    }

    let texts: Vec<(&SceneEntity, EntityText)> = selected
        .iter()
        .filter(|entity| TEXT_KINDS.contains(&entity.kind))
        .filter_map(|entity| Some((*entity, entity_text(entity)?)))
        .collect();

    let mut result: Vec<(usize, RuledTable)> = tables
        .into_iter()
        .filter_map(|(group, members)| Some((group, build_table(&members, &cells, tolerance)?)))
        .collect();

    // 文字は、同じ配置でその中心を含むます目に入れる。ます目の中の島（記号の枠など）の文字もそのます目に入れる
    let mut spots: Vec<(usize, usize)> = Vec::new();
    for (t, (_, table)) in result.iter().enumerate() {
        for c in 0..table.cells.len() {
            spots.push((t, c));
        }
    }
    let grid = BoxGrid::new(spots.iter().enumerate().map(|(i, &(t, c))| (i, result[t].1.cells[c].bbox)).collect());
    let mut placed: Vec<Vec<Vec<(&EntityText, &SceneEntity)>>> =
        result.iter().map(|(_, table)| vec![Vec::new(); table.cells.len()]).collect();
    for (entity, text) in &texts {
        let x = (text.bbox.min_x + text.bbox.max_x) / 2.0;
        let y = (text.bbox.min_y + text.bbox.max_y) / 2.0;
        let found = grid
            .at(x, y)
            .filter(|&i| {
                let (t, _) = spots[i];
                same_placement(result[t].1.placement.as_deref(), entity.placement.as_deref())
            })
            .min_by(|&a, &b| {
                let area = |i: usize| box_area(&result[spots[i].0].1.cells[spots[i].1].bbox);
                area(a).total_cmp(&area(b)).then(a.cmp(&b))
            });
        if let Some(i) = found {
            let (t, c) = spots[i];
            placed[t][c].push((text, entity));
        }
    }
    for ((_, table), cells) in result.iter_mut().zip(placed) {
        for (cell, mut texts) in table.cells.iter_mut().zip(cells) {
            fill_cell(cell, &mut texts);
        }
    }

    result.retain(|(_, table)| options.empty || table.cells.iter().any(|cell| !cell.texts.is_empty()));
    result.sort_by(|(a_group, a), (b_group, b)| {
        a_group.cmp(b_group).then(b.bbox.max_y.total_cmp(&a.bbox.max_y)).then(a.bbox.min_x.total_cmp(&b.bbox.min_x))
    });
    result.into_iter().map(|(_, table)| table).collect()
}

fn find(parent: &mut [usize], mut x: usize) -> usize {
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn box_area(b: &BBox) -> f64 {
    (b.max_x - b.min_x) * (b.max_y - b.min_y)
}

fn union_box(boxes: &[BBox]) -> BBox {
    let mut total = empty_box();
    for b in boxes {
        extend_box(&mut total, b.min_x, b.min_y);
        extend_box(&mut total, b.max_x, b.max_y);
    }
    total
}

/// 2 つの長方形が、許容差より長い辺を共有するか
fn share_edge(a: &BBox, b: &BBox, tolerance: f64) -> bool {
    let overlap = |a0: f64, a1: f64, b0: f64, b1: f64| a1.min(b1) - a0.max(b0);
    let beside = (a.max_x - b.min_x).abs() <= tolerance || (b.max_x - a.min_x).abs() <= tolerance;
    let stacked = (a.max_y - b.min_y).abs() <= tolerance || (b.max_y - a.min_y).abs() <= tolerance;
    (beside && overlap(a.min_y, a.max_y, b.min_y, b.max_y) > tolerance)
        || (stacked && overlap(a.min_x, a.max_x, b.min_x, b.max_x) > tolerance)
}

/// ます目を、辺を共有するものどうしのまとまりに分ける
fn split_connected(members: &[usize], cells: &[(usize, ClosedRegion)], tolerance: f64) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..members.len()).collect();
    for i in 0..members.len() {
        for j in i + 1..members.len() {
            if share_edge(&cells[members[i]].1.bbox, &cells[members[j]].1.bbox, tolerance) {
                let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_of = vec![usize::MAX; members.len()];
    for (i, &member) in members.iter().enumerate() {
        let root = find(&mut parent, i);
        if group_of[root] == usize::MAX {
            group_of[root] = groups.len();
            groups.push(Vec::new());
        }
        groups[group_of[root]].push(member);
    }
    groups
}

/// 位置を許容差の 2 倍までまとめるが、同じます目の向かい合う辺は別の罫線にする。
/// 各ます目の両端が属する罫線も返す（近い罫線への再割り当てで細います目をつぶさない）。
fn lines(boxes: &[BBox], axis: fn(&BBox) -> [f64; 2], tolerance: f64) -> (Vec<f64>, Vec<[usize; 2]>) {
    let mut values: Vec<(f64, usize, usize)> = boxes
        .iter()
        .enumerate()
        .flat_map(|(cell, bbox)| axis(bbox).into_iter().enumerate().map(move |(side, value)| (value, cell, side)))
        .collect();
    values.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut lines: Vec<f64> = Vec::new();
    let mut edges = vec![[0; 2]; boxes.len()];
    let mut members = HashSet::new();
    let mut index = 0;
    while index < values.len() {
        let value = values[index].0;
        // 交点の計算で同じ辺の座標の末尾だけ違うことがある。幾何の許容差より十分小さい誤差は同じ位置。
        let rounding = (f64::EPSILON * 8.0 * value.abs().max(1.0)).min(tolerance * 0.001);
        let end = index + values[index..].partition_point(|edge| edge.0 - value <= rounding);
        // 同じ位置の辺はまとめて扱う。細います目の反対側の辺があれば、この位置から新しい罫線にする。
        if lines.last().is_none_or(|last| value - last > tolerance * 2.0)
            || values[index..end].iter().any(|(_, cell, _)| members.contains(cell))
        {
            lines.push(value);
            members.clear();
        }
        for &(_, cell, side) in &values[index..end] {
            edges[cell][side] = lines.len() - 1;
            members.insert(cell);
        }
        index = end;
    }
    (lines, edges)
}

fn build_table(members: &[usize], cells: &[(usize, ClosedRegion)], tolerance: f64) -> Option<RuledTable> {
    let boxes: Vec<BBox> = members.iter().map(|&m| cells[m].1.bbox).collect();
    let (columns, x_edges) = lines(&boxes, |b| [b.min_x, b.max_x], tolerance);
    let (mut rows, y_edges) = lines(&boxes, |b| [b.min_y, b.max_y], tolerance);
    rows.reverse();
    let mut boundary: Vec<String> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut table_cells: Vec<TableCell> = members
        .iter()
        .enumerate()
        .map(|(index, &m)| {
            let region = &cells[m].1;
            for id in &region.boundary {
                if seen.insert(id) {
                    boundary.push(id.clone());
                }
            }
            let b = region.bbox;
            let [left, right] = x_edges[index];
            let (top, bottom) = (rows.len() - 1 - y_edges[index][1], rows.len() - 1 - y_edges[index][0]);
            TableCell {
                row: top,
                column: left,
                row_span: bottom.saturating_sub(top).max(1),
                column_span: right.saturating_sub(left).max(1),
                bbox: b,
                text: String::new(),
                texts: Vec::new(),
            }
        })
        .collect();
    table_cells.sort_by_key(|cell| (cell.row, cell.column));
    if table_cells
        .iter()
        .any(|cell| cell.row + cell.row_span >= rows.len() || cell.column + cell.column_span >= columns.len())
    {
        return None;
    }
    Some(RuledTable {
        bbox: union_box(&boxes),
        placement: cells[members[0]].1.placement.clone(),
        columns,
        rows,
        cells: table_cells,
        boundary,
    })
}

/// ます目の文字を、上の行から、同じ行は左から並べる。中心の高さの差が文字の高さの半分までなら同じ行
fn fill_cell(cell: &mut TableCell, texts: &mut [(&EntityText, &SceneEntity)]) {
    let center =
        |text: &EntityText| ((text.bbox.min_x + text.bbox.max_x) / 2.0, (text.bbox.min_y + text.bbox.max_y) / 2.0);
    texts.sort_by(|(a, _), (b, _)| center(b).1.total_cmp(&center(a).1));
    let mut lines: Vec<Vec<(&EntityText, &SceneEntity)>> = Vec::new();
    for &(text, entity) in texts.iter() {
        let same = lines
            .last()
            .and_then(|line| line.first())
            .is_some_and(|(first, _)| (center(first).1 - center(text).1).abs() <= first.height.max(text.height) / 2.0);
        if same {
            lines.last_mut().unwrap().push((text, entity));
        } else {
            lines.push(vec![(text, entity)]);
        }
    }
    let mut joined: Vec<String> = Vec::new();
    for line in &mut lines {
        line.sort_by(|(a, _), (b, _)| a.bbox.min_x.total_cmp(&b.bbox.min_x));
        joined.push(line.iter().map(|(text, _)| text.text.as_str()).collect::<Vec<_>>().join(" "));
        for (text, entity) in line.iter() {
            let (x, y) = center(text);
            cell.texts.push(RegionText { entity: entity.id.clone(), text: text.text.clone(), x, y });
        }
    }
    cell.text = joined.join("\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::placement;
    use crate::types::{Background, Primitive, SceneLayer, StrokePrimitive, TextPrimitive};

    fn line(id: &str, points: &[f64], closed: bool) -> SceneEntity {
        let mut bbox = empty_box();
        for [x, y] in points.as_chunks::<2>().0 {
            extend_box(&mut bbox, *x, *y);
        }
        SceneEntity {
            id: id.into(),
            kind: if closed { EntityKind::Polyline } else { EntityKind::Line },
            type_name: "線".into(),
            source: "test".into(),
            layers: vec![0],
            primitives: vec![Primitive::Stroke(StrokePrimitive {
                points: points.to_vec(),
                closed,
                segments: None,
                color: "#000000".into(),
                width: 0.0,
                dashes: Vec::new(),
            })],
            bbox,
            properties: Vec::new(),
            placement: None,
            drawn: true,
        }
    }

    /// 左下が (x, y) の、高さ 2 mm の文字
    fn label(id: &str, text: &str, x: f64, y: f64) -> SceneEntity {
        let width = 2.0 * text.chars().count() as f64;
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
                character_height: None,
                scale_x: 1.0,
                cells: Vec::new(),
                box_width: width,
                box_height: 2.0,
                matrix: placement(x, y, 0.0, 1.0, 1.0),
            })],
            bbox: BBox { min_x: x, min_y: y, max_x: x + width, max_y: y + 2.0 },
            properties: Vec::new(),
            placement: None,
            drawn: true,
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
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 420.0, max_y: 297.0 },
            layers: vec![SceneLayer { id: 0, name: "0".into(), visible: true, group: None }],
            entities,
            underlays: Vec::new(),
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    /// 幅 30 + 50、高さ 10 × 3 の表。見出しの行は 2 列を結合し、空欄に斜線を引く
    fn schedule() -> Vec<SceneEntity> {
        vec![
            line("frame", &[0.0, 0.0, 80.0, 0.0, 80.0, 30.0, 0.0, 30.0], true),
            line("h1", &[0.0, 20.0, 80.0, 20.0], false),
            line("h2", &[0.0, 10.0, 80.0, 10.0], false),
            line("v", &[30.0, 0.0, 30.0, 20.0], false),
            line("slash", &[30.0, 0.0, 80.0, 10.0], false),
            label("title", "面積表", 2.0, 24.0),
            label("k1", "敷地面積", 2.0, 14.0),
            label("v1", "1,029.18", 32.0, 14.0),
            label("v1u", "㎡", 52.0, 14.0),
            label("k2", "建築面積", 2.0, 4.0),
        ]
    }

    #[test]
    fn finds_rows_columns_merged_cells_and_texts() {
        let tables = ruled_tables(&scene(schedule()), &TableOptions::default());
        assert_eq!(tables.len(), 1, "{tables:#?}");
        let table = &tables[0];
        assert_eq!(table.size(), (3, 2));
        assert_eq!(table.columns, [0.0, 30.0, 80.0]);
        assert_eq!(table.rows, [30.0, 20.0, 10.0, 0.0]);
        assert_eq!(table.cells.len(), 5);
        let header = &table.cells[0];
        assert_eq!((header.row, header.column, header.row_span, header.column_span), (0, 0, 1, 2));
        assert_eq!(header.text, "面積表");
        assert_eq!(
            table.grid(),
            [vec!["面積表", ""], vec!["敷地面積", "1,029.18 ㎡"], vec!["建築面積", ""]]
                .map(|row| row.into_iter().map(String::from).collect::<Vec<_>>())
        );
        // 斜線は罫線にしない
        assert!(!table.boundary.contains(&"slash".to_string()));
        assert_eq!(table.cells[2].texts.iter().map(|t| t.entity.as_str()).collect::<Vec<_>>(), ["v1", "v1u"]);
    }

    #[test]
    fn leaves_out_a_dominant_cell_such_as_the_drawing_area_beside_a_title_block() {
        // 図枠（400 × 277）の右に幅 40 の表題欄があり、3 つの欄に分かれる
        let mut entities = vec![
            line("frame", &[10.0, 10.0, 410.0, 10.0, 410.0, 287.0, 10.0, 287.0], true),
            line("edge", &[370.0, 10.0, 370.0, 287.0], false),
            line("r1", &[370.0, 40.0, 410.0, 40.0], false),
            line("r2", &[370.0, 60.0, 410.0, 60.0], false),
            label("number", "A-101", 375.0, 20.0),
            label("note", "注記", 100.0, 100.0),
        ];
        let tables = ruled_tables(&scene(entities.clone()), &TableOptions::default());
        assert_eq!(tables.len(), 1, "{tables:#?}");
        assert_eq!(tables[0].size(), (3, 1));
        assert_eq!(tables[0].grid()[2], ["A-101"]);

        // 文字のない表は既定で返さない
        entities.retain(|entity| entity.kind != EntityKind::Text);
        assert!(ruled_tables(&scene(entities.clone()), &TableOptions::default()).is_empty());
        let empty = TableOptions { empty: true, ..TableOptions::default() };
        assert_eq!(ruled_tables(&scene(entities), &empty).len(), 1);
    }

    #[test]
    fn keeps_both_edges_of_cells_narrower_than_twice_the_tolerance() {
        for tolerance in [0.01, 1.0] {
            for (width, height) in [(20.0, 1.5), (1.5, 20.0), (1.5, 1.5)] {
                let (w, h) = (width * tolerance, height * tolerance);
                let drawing = scene(vec![
                    line("frame", &[0.0, 0.0, w * 2.0, 0.0, w * 2.0, h * 2.0, 0.0, h * 2.0], true),
                    line("vertical", &[w, 0.0, w, h * 2.0], false),
                    line("horizontal", &[0.0, h, w * 2.0, h], false),
                ]);
                let options = TableOptions { tolerance, empty: true, ..TableOptions::default() };
                let tables = ruled_tables(&drawing, &options);
                assert_eq!(tables.len(), 1, "{tolerance} {width} {height}");
                assert_eq!(tables[0].size(), (2, 2));
                assert_eq!(tables[0].grid(), vec![vec!["", ""], vec!["", ""]]);
                assert!(tables[0].cells.iter().all(|cell| cell.row_span == 1 && cell.column_span == 1));
            }
        }
    }

    #[test]
    fn merges_offset_edges_without_collapsing_a_narrow_cell() {
        for tolerance in [0.01, 1.0] {
            let t = tolerance;
            let drawing = scene(vec![
                line("frame", &[0.0, 0.0, 20.0 * t, 0.0, 20.0 * t, 20.0 * t, 0.0, 20.0 * t], true),
                line("horizontal", &[0.0, 10.0 * t, 20.0 * t, 10.0 * t], false),
                line("upper", &[1.5 * t, 10.0 * t, 1.5 * t, 20.0 * t], false),
                line("lower", &[2.3 * t, 0.0, 2.3 * t, 10.0 * t], false),
            ]);
            let tables = ruled_tables(&drawing, &TableOptions { tolerance, empty: true, ..TableOptions::default() });
            assert_eq!(tables.len(), 1);
            assert_eq!(tables[0].size(), (2, 2));
            let cells: Vec<_> = tables[0].cells.iter().map(|c| (c.row, c.column, c.row_span, c.column_span)).collect();
            assert_eq!(cells, [(0, 0, 1, 1), (0, 1, 1, 1), (1, 0, 1, 1), (1, 1, 1, 1)], "{tables:#?}");
            assert_eq!(tables[0].grid(), vec![vec!["", ""], vec!["", ""]]);
        }
    }

    #[test]
    fn rejects_cells_that_do_not_fill_a_rectangle() {
        // L 字に並ぶ 3 つのます目は表にしない
        let s = scene(vec![
            line("a", &[0.0, 0.0, 20.0, 0.0, 20.0, 10.0, 0.0, 10.0], true),
            line("b", &[20.0, 0.0, 40.0, 0.0, 40.0, 10.0, 20.0, 10.0], true),
            line("c", &[0.0, 10.0, 20.0, 10.0, 20.0, 20.0, 0.0, 20.0], true),
            label("t", "x", 2.0, 2.0),
        ]);
        assert!(ruled_tables(&s, &TableOptions::default()).is_empty());
        let many = TableOptions { min_cells: 6, ..TableOptions::default() };
        assert!(ruled_tables(&scene(schedule()), &many).is_empty());
    }
}
