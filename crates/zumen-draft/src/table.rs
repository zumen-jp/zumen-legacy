//! 表。列の幅・行の高さ・結合したセル・セルの中の文字の位置から、罫線（外枠は太く）と文字を描く。
//! 設計概要書・仕上表・面積表・表題欄に使う。
//!
//! セルに収まらない文字は、fit が shrink（既定）なら幅の係数を縮め、wrap なら折り返し、どちらも高さに収まらなければ
//! 文字を低くする。罫線は、隣り合うセルが違うところにだけ引き、同じ線の上で続く罫線は 1 本の線にする。

use crate::spec::{CellSpec, Point, StyleSpec, TableSpec, parse_cell};
use crate::text::{self, Align, MIDDLE_LEFT, Placed};
use crate::{Ctx, DraftError, Drafter, MAX_PARTS, number, point, positive};
use zumen_convert::builder::{Look, lineweight, lwpolyline, xyz};
use zumen_dxf::DxfEntityData;

/// 行の高さの既定（用紙の mm）
const ROW_HEIGHT: f64 = 7.0;
/// MTEXT の行の送り（文字の高さの 5/3、DXF Reference「MTEXT」の 44）
const LINE_PITCH: f64 = 5.0 / 3.0;

/// 置いたセル
struct Placement {
    row: usize,
    column: usize,
    rows: usize,
    columns: usize,
    cell: CellSpec,
    path: String,
}

/// 文字を収め方
#[derive(Clone, Copy, PartialEq)]
enum Fit {
    Shrink,
    Wrap,
    None,
}

fn sizes(values: &[f64], key: &str, path: &str) -> Result<Vec<f64>, DraftError> {
    values.iter().map(|value| positive(*value, key, path)).collect()
}

/// 格子の各位置に置いたセルの番号（空きは None）
type Owners = Vec<Vec<Option<usize>>>;

/// セルを格子に置く。owner[行][列] は置いたセルの番号（空きは None）
fn place(spec: &TableSpec, columns: usize, path: &str) -> Result<(Vec<Placement>, Owners), DraftError> {
    let rows = spec.cells.len();
    let mut owner: Owners = vec![vec![None; columns]; rows];
    let mut placements = Vec::new();
    for (row, cells) in spec.cells.iter().enumerate() {
        let mut column = 0;
        for (index, value) in cells.iter().enumerate() {
            let cell_path = format!("{path}.cells[{row}][{index}]");
            while column < columns && owner[row][column].is_some() {
                column += 1;
            }
            let cell = parse_cell(value, &cell_path)?;
            let (span_columns, span_rows) = (cell.colspan.unwrap_or(1).max(1), cell.rowspan.unwrap_or(1).max(1));
            if column + span_columns > columns {
                return Err(DraftError::new(
                    "invalid-value",
                    &cell_path,
                    format!("セルが列の数（{columns}）を超えます"),
                ));
            }
            if row + span_rows > rows {
                return Err(DraftError::new("invalid-value", &cell_path, format!("セルが行の数（{rows}）を超えます")));
            }
            for cells in owner.iter_mut().skip(row).take(span_rows) {
                for slot in cells.iter_mut().skip(column).take(span_columns) {
                    if slot.is_some() {
                        return Err(DraftError::new("invalid-value", &cell_path, "結合したセルが重なります"));
                    }
                    *slot = Some(placements.len());
                }
            }
            placements.push(Placement { row, column, rows: span_rows, columns: span_columns, cell, path: cell_path });
            column += span_columns;
        }
    }
    Ok((placements, owner))
}

/// 境界の上の、続く区間をまとめる。edges[k] は k 番目の区間に線を引くか
fn runs(edges: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (index, edge) in edges.iter().chain(std::iter::once(&false)).enumerate() {
        match (edge, start) {
            (true, None) => start = Some(index),
            (false, Some(begin)) => {
                out.push((begin, index));
                start = None;
            }
            _ => {}
        }
    }
    out
}

pub(crate) fn draw(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &TableSpec,
    path: &str,
) -> Result<(), DraftError> {
    let at = point(spec.at, path)?;
    if spec.columns.is_empty() || spec.cells.is_empty() {
        return Err(DraftError::new("invalid-value", path, "columns と cells は 1 つ以上にしてください"));
    }
    if spec.columns.len() * spec.cells.len() > MAX_PARTS {
        return Err(DraftError::new("invalid-value", path, "表のセルが多すぎます"));
    }
    let s = ctx.scale;
    let widths: Vec<f64> = sizes(&spec.columns, "columns", path)?.iter().map(|w| w * s).collect();
    let row_height = positive(spec.row_height.unwrap_or(ROW_HEIGHT), "rowHeight", path)?;
    let heights: Vec<f64> = match &spec.rows {
        Some(rows) if rows.len() != spec.cells.len() => {
            return Err(DraftError::new("invalid-value", path, "rows は cells の行と同じ数にしてください"));
        }
        Some(rows) => sizes(rows, "rows", path)?.iter().map(|h| h * s).collect(),
        None => vec![row_height * s; spec.cells.len()],
    };
    let (columns, rows) = (widths.len(), heights.len());
    let (placements, owner) = place(spec, columns, path)?;
    // 罫線の位置
    let mut xs = vec![at[0]];
    for width in &widths {
        xs.push(xs[xs.len() - 1] + width);
    }
    let mut ys = vec![at[1]];
    for height in &heights {
        ys.push(ys[ys.len() - 1] - height);
    }
    let base = drafter.look(ctx, style, None, path)?;
    let outer_width = spec.outer_width.map_or(Ok(0.35), |w| number(w, "outerWidth", path))?;
    let inner_width = spec.inner_width.map_or(Ok(0.13), |w| number(w, "innerWidth", path))?;
    let with_width =
        |look: &Look, width: f64| Look { lineweight: lineweight(width.clamp(0.0, 2.11)).0, ..look.clone() };
    let (outer, inner) = if style.line_width.is_some() || ctx.style.line_width.is_some() {
        (base.clone(), base.clone())
    } else {
        (with_width(&base, outer_width), with_width(&base, inner_width))
    };
    let same = |a: Option<usize>, b: Option<usize>| a.is_some() && a == b;
    // 横の罫線（内側）
    for k in 1..rows {
        let edges: Vec<bool> = (0..columns).map(|c| !same(owner[k - 1][c], owner[k][c])).collect();
        for (from, to) in runs(&edges) {
            let data = DxfEntityData::Line { start: xyz([xs[from], ys[k]]), end: xyz([xs[to], ys[k]]) };
            drafter.add(&ctx.space, &inner, "LINE", data, path);
        }
    }
    // 縦の罫線（内側）
    for k in 1..columns {
        let edges: Vec<bool> = (0..rows).map(|r| !same(owner[r][k - 1], owner[r][k])).collect();
        for (from, to) in runs(&edges) {
            let data = DxfEntityData::Line { start: xyz([xs[k], ys[from]]), end: xyz([xs[k], ys[to]]) };
            drafter.add(&ctx.space, &inner, "LINE", data, path);
        }
    }
    // 外枠
    let corners = vec![[xs[0], ys[0]], [xs[columns], ys[0]], [xs[columns], ys[rows]], [xs[0], ys[rows]]];
    drafter.add(&ctx.space, &outer, "LWPOLYLINE", lwpolyline(&corners, None, true), path);
    // 文字
    let default_align = Align::parse(spec.align.as_deref(), MIDDLE_LEFT, path)?;
    let fit = match spec.fit.as_deref().unwrap_or("shrink") {
        "shrink" => Fit::Shrink,
        "wrap" => Fit::Wrap,
        "none" => Fit::None,
        other => {
            return Err(DraftError::new(
                "invalid-value",
                path,
                format!("fit の {other} はありません（shrink・wrap・none）"),
            ));
        }
    };
    let padding = number(spec.padding.unwrap_or(1.0), "padding", path)?.max(0.0) * s;
    let text_height = positive(spec.text_height.unwrap_or_else(|| drafter.text_height()), "textHeight", path)?;
    let text_layer = drafter.defaults.text_layer.clone();
    let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
    let text_style = drafter.text_style(None);
    for placement in &placements {
        let (x0, x1) = (xs[placement.column], xs[placement.column + placement.columns]);
        let (top, bottom) = (ys[placement.row], ys[placement.row + placement.rows]);
        if placement.cell.diagonal {
            let data = DxfEntityData::Line { start: xyz([x0, top]), end: xyz([x1, bottom]) };
            drafter.add(&ctx.space, &inner, "LINE", data, &placement.path);
        }
        let Some(content) = placement.cell.text.as_deref().filter(|text| !text.is_empty()) else { continue };
        let column_align =
            spec.column_align.as_ref().and_then(|aligns| aligns.get(placement.column).cloned().flatten());
        let align_name = placement.cell.align.clone().or(column_align);
        let align = Align::parse(align_name.as_deref(), default_align, &placement.path)?;
        let max_width = (x1 - x0 - 2.0 * padding).max(0.0);
        let max_height = (top - bottom - 2.0 * padding).max(0.0);
        let mut height = placement.cell.height.map_or(Ok(text_height), |h| positive(h, "height", &placement.path))? * s;
        let mut factor = 1.0;
        let lines: Vec<String> = match fit {
            Fit::Wrap => text::wrap(content, max_width, height, factor),
            _ => content.split('\n').map(str::to_string).collect(),
        };
        if fit == Fit::Shrink {
            (height, factor) = text::fit(&lines, max_width, height, factor);
        }
        if fit != Fit::None {
            // 行の送りの合計が高さに収まるよう、文字を低くする
            let needed = height * (1.0 + (lines.len() as f64 - 1.0) * LINE_PITCH);
            if needed > max_height && needed > 0.0 {
                height *= max_height / needed;
            }
        }
        let x = match align.h {
            0 => x0 + padding,
            1 => (x0 + x1) / 2.0,
            _ => x1 - padding,
        };
        let y = match align.v {
            3 => top - padding,
            2 => (top + bottom) / 2.0,
            _ => bottom + padding,
        };
        // ベースラインの合わせ方は、セルの下の余白の上にそろえる（下と同じ）
        let align = Align { v: align.v.max(1), ..align };
        let placed: Placed<'_> = Placed {
            at: [x, y] as Point,
            lines: &lines,
            height,
            align,
            rotation: 0.0,
            width_factor: factor,
            line_spacing: 1.0,
            style: text_style.clone(),
        };
        text::put(drafter, &ctx.space, &text_look, placed, &placement.path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_runs_of_edges() {
        assert_eq!(runs(&[true, true, false, true]), vec![(0, 2), (3, 4)]);
        assert_eq!(runs(&[false, false]), Vec::<(usize, usize)>::new());
    }
}
