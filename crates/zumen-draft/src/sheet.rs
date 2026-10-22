//! 用紙（レイアウト）・ビュー（ビューポート）・枠と表題欄・ブロックと属性（決定 0008・0018）。
//!
//! - 1 枚目の用紙は *Paper_Space、2 枚目からは *Paper_Space0、*Paper_Space1 … のレイアウトにする。
//! - ビューの図形はモデル空間に実寸で描き、ビューポートで縮尺 1/scale に見せる。2 つのビューのモデルの範囲が
//!   重なれば、ビューポートにもう一方の図形が映るので、知らせる（`view-overlap`）。
//! - 表題欄は、枠の右端の縦の帯（right）か右下の表（bottom-right）。

use crate::spec::{
    AttributeSpec, InsertSpec, Point, SheetSpec, Spec, StyleSpec, TableSpec, TitleBlockSpec, ViewTitleSpec, parse_items,
};
use crate::text::{self, Align, BOTTOM_LEFT, MIDDLE_CENTER, Placed};
use crate::{BlockInfo, Ctx, DraftError, Drafter, number, point, positive, symbols};
use serde_json::Value;
use zumen_convert::builder::{BY_LAYER, LINEWEIGHT_BY_LAYER, Look, Space, VIEWPORT_LAYER, lineweight, lwpolyline, xyz};
use zumen_convert::sheet::{View, region, viewport};
use zumen_core::BBox;
use zumen_dxf::{DxfEntity, DxfEntityBase, DxfEntityData, DxfInsert, DxfLayout, DxfSpace, DxfTextBase, DxfViewport};

/// JIS Z 8311 の A 列と B 列（横置きの幅・高さ、mm）
const PAPERS: [(&str, f64, f64); 10] = [
    ("A0", 1189.0, 841.0),
    ("A1", 841.0, 594.0),
    ("A2", 594.0, 420.0),
    ("A3", 420.0, 297.0),
    ("A4", 297.0, 210.0),
    ("B1", 1030.0, 728.0),
    ("B2", 728.0, 515.0),
    ("B3", 515.0, 364.0),
    ("B4", 364.0, 257.0),
    ("B5", 257.0, 182.0),
];

/// 用紙の名前と大きさ（幅, 高さ）
fn paper(sheet: &SheetSpec, path: &str) -> Result<(String, f64, f64), DraftError> {
    let (name, width, height) = match &sheet.paper {
        None => ("A3".to_string(), 420.0, 297.0),
        Some(Value::String(name)) => {
            let upper = name.to_uppercase();
            let Some((_, w, h)) = PAPERS.iter().find(|(paper, _, _)| *paper == upper) else {
                let names: Vec<&str> = PAPERS.iter().map(|(name, _, _)| *name).collect();
                return Err(DraftError::new(
                    "invalid-value",
                    path,
                    format!("paper の {name} はありません（{} か [幅, 高さ]）", names.join("・")),
                ));
            };
            (upper, *w, *h)
        }
        Some(Value::Array(size)) if size.len() == 2 => {
            let w = size[0].as_f64().unwrap_or(f64::NAN);
            let h = size[1].as_f64().unwrap_or(f64::NAN);
            let (w, h) = (positive(w, "paper", path)?, positive(h, "paper", path)?);
            (format!("{w}x{h}"), w.max(h), w.min(h))
        }
        Some(_) => {
            return Err(DraftError::new(
                "invalid-value",
                path,
                "paper は A3 などの名前か [幅, 高さ] で指定してください",
            ));
        }
    };
    match sheet.orientation.as_deref().unwrap_or("landscape") {
        "landscape" => Ok((name, width, height)),
        "portrait" => Ok((name, height, width)),
        other => Err(DraftError::new(
            "invalid-value",
            path,
            format!("orientation の {other} はありません（landscape・portrait）"),
        )),
    }
}

fn layout(
    drafter: &mut Drafter,
    name: &str,
    block: &str,
    model: bool,
    order: usize,
    size: (&str, f64, f64),
) -> DxfLayout {
    let record = drafter
        .builder
        .doc
        .blocks
        .iter()
        .find(|candidate| candidate.name == block)
        .and_then(|candidate| candidate.block_record_handle.clone());
    let (paper, width, height) = size;
    DxfLayout {
        handle: drafter.builder.handle(),
        name: name.to_string(),
        tab_order: order as f64,
        model,
        active: block == "*Paper_Space",
        block: block.to_string(),
        block_record_handle: record,
        paper_width: if model { 0.0 } else { width },
        paper_height: if model { 0.0 } else { height },
        margins: [0.0; 4],
        paper_size: if model { String::new() } else { paper.to_string() },
        paper_units: 1.0,
        plot_rotation: 0.0,
        plot_scale: [1.0, 1.0],
        plot_origin: [0.0, 0.0],
        limmin: [0.0, 0.0],
        limmax: if model { [0.0, 0.0] } else { [width, height] },
    }
}

fn viewport_look() -> Look {
    Look {
        layer: VIEWPORT_LAYER.into(),
        color: BY_LAYER,
        linetype: "ByLayer".into(),
        lineweight: LINEWEIGHT_BY_LAYER,
        linetype_scale: 1.0,
    }
}

/// 図面全体を描く
pub(crate) fn draw(drafter: &mut Drafter, spec: &Spec) -> Result<(), DraftError> {
    blocks(drafter, spec)?;
    if spec.sheets.is_empty() {
        return Err(DraftError::new("invalid-spec", "sheets", "用紙（sheets）を 1 枚以上指定してください"));
    }
    let mut layouts = vec![layout(drafter, "Model", "*Model_Space", true, 0, ("", 0.0, 0.0))];
    let mut names: Vec<String> = Vec::new();
    // All layouts view the same model space, including views on different sheets.
    let mut regions: Vec<(String, BBox)> = Vec::new();
    for (index, sheet) in spec.sheets.iter().enumerate() {
        let path = format!("sheets[{index}]");
        if names.iter().any(|name| name.eq_ignore_ascii_case(&sheet.name)) {
            return Err(DraftError::new("invalid-spec", &path, format!("用紙の名前 {} が 2 度あります", sheet.name)));
        }
        names.push(sheet.name.clone());
        let (paper_name, width, height) = paper(sheet, &path)?;
        let (space, block) = if index == 0 {
            (Space::Paper, "*Paper_Space".to_string())
        } else {
            let block = drafter.builder.layout_block();
            (Space::Layout(block.clone()), block)
        };
        layouts.push(layout(drafter, &sheet.name, &block, false, index + 1, (&paper_name, width, height)));
        draw_sheet(drafter, sheet, &space, (width, height), &path, &mut regions)?;
    }
    drafter.builder.doc.layouts = layouts;
    // レイアウトを開いた状態にする（決定 0008）
    drafter.builder.doc.header.tilemode = 0.0;
    Ok(())
}

/// ビューポートの id（ID 1 は用紙ごとの全体のビューポート）
fn draw_sheet(
    drafter: &mut Drafter,
    sheet: &SheetSpec,
    space: &Space,
    size: (f64, f64),
    path: &str,
    regions: &mut Vec<(String, BBox)>,
) -> Result<(), DraftError> {
    let (width, height) = size;
    let paper_ctx = Ctx { space: space.clone(), scale: 1.0, style: StyleSpec::default() };
    // 全体のビューポート（AutoCAD はレイアウトごとに ID 1 を持つ）
    let overall = DxfEntityData::Viewport(DxfViewport {
        center: [width / 2.0, height / 2.0, 0.0],
        width,
        height,
        status: 1.0,
        id: 1.0,
        view_center: [width / 2.0, height / 2.0],
        view_target: [0.0; 3],
        view_direction: [0.0, 0.0, 1.0],
        view_height: height,
        view_twist_angle: 0.0,
        status_flags: 32768.0,
        frozen_layer_handles: Vec::new(),
        frozen_layer_names: Vec::new(),
        clipping_boundary_handle: None,
    });
    drafter.builder.add(space, &viewport_look(), "VIEWPORT", overall, "");
    if let Some(frame) = &sheet.frame {
        draw_frame(drafter, &paper_ctx, frame, size, &format!("{path}.frame"))?;
    }
    for (index, view) in sheet.views.iter().enumerate() {
        let view_path = format!("{path}.views[{index}]");
        let scale = positive(view.scale, "scale", &view_path)?;
        let [a, b, c, d] = view.viewport;
        let rect = BBox {
            min_x: number(a.min(c), "viewport", &view_path)?,
            min_y: number(b.min(d), "viewport", &view_path)?,
            max_x: number(a.max(c), "viewport", &view_path)?,
            max_y: number(b.max(d), "viewport", &view_path)?,
        };
        if rect.max_x - rect.min_x <= 0.0 || rect.max_y - rect.min_y <= 0.0 {
            return Err(DraftError::new("invalid-value", &view_path, "viewport の幅と高さを 0 より大きくしてください"));
        }
        let items = parse_items(&view.items, &format!("{view_path}.items"))?;
        let ctx = Ctx { space: Space::Model, scale, style: StyleSpec::default() };
        drafter.extent = None;
        drafter.items(&items, &ctx)?;
        let center = match view.center {
            Some(center) => point(center, &view_path)?,
            None => drafter.extent.map_or([0.0, 0.0], |[x0, y0, x1, y1]| [(x0 + x1) / 2.0, (y0 + y1) / 2.0]),
        };
        let paper_center = [(rect.min_x + rect.max_x) / 2.0, (rect.min_y + rect.max_y) / 2.0];
        let view_model = View {
            content: index,
            scale: 1.0 / scale,
            angle: 0.0,
            origin: [paper_center[0] - center[0] / scale, paper_center[1] - center[1] / scale],
            rect,
        };
        let model_region = region(&view_model, [0.0, 0.0]);
        for (other, other_region) in regions.iter() {
            let overlaps = model_region.min_x < other_region.max_x
                && other_region.min_x < model_region.max_x
                && model_region.min_y < other_region.max_y
                && other_region.min_y < model_region.max_y;
            if overlaps {
                drafter.notice(
                    "view-overlap",
                    format!(
                        "{view_path} と {other} のモデルの範囲が重なり、ビューポートにもう一方の図形が映ります。モデルの座標をずらしてください"
                    ),
                );
            }
        }
        let label = match &view.name {
            Some(name) => format!("{view_path}（{name}）"),
            None => view_path.clone(),
        };
        regions.push((label, model_region));
        let data = DxfEntityData::Viewport(viewport(&view_model, [0.0, 0.0], (index + 2) as f64));
        drafter.builder.add(space, &viewport_look(), "VIEWPORT", data, &view_path);
        if view.border {
            let look = drafter.look(&paper_ctx, &StyleSpec::default(), None, &view_path)?;
            let corners = vec![
                [rect.min_x, rect.min_y],
                [rect.max_x, rect.min_y],
                [rect.max_x, rect.max_y],
                [rect.min_x, rect.max_y],
            ];
            drafter.add(space, &look, "LWPOLYLINE", lwpolyline(&corners, None, true), &view_path);
        }
        if let Some(title) = &view.title {
            let title =
                ViewTitleSpec { at: Some(title.at.unwrap_or([rect.min_x + 5.0, rect.min_y + 7.0])), ..title.clone() };
            symbols::view_title(
                drafter,
                &paper_ctx,
                &StyleSpec::default(),
                &title,
                Some(scale),
                &format!("{view_path}.title"),
            )?;
        }
    }
    let items = parse_items(&sheet.items, &format!("{path}.items"))?;
    drafter.items(&items, &paper_ctx)
}

fn draw_frame(
    drafter: &mut Drafter,
    ctx: &Ctx,
    frame: &crate::spec::FrameSpec,
    size: (f64, f64),
    path: &str,
) -> Result<(), DraftError> {
    let (width, height) = size;
    let [left, bottom, right, top] = frame.margins.unwrap_or([20.0, 10.0, 10.0, 10.0]);
    let (x0, y0, x1, y1) = (left, bottom, width - right, height - top);
    if x1 - x0 <= 0.0 || y1 - y0 <= 0.0 || [left, bottom, right, top].iter().any(|m| !m.is_finite() || *m < 0.0) {
        return Err(DraftError::new("invalid-value", path, "margins が用紙に収まりません"));
    }
    let style = StyleSpec { layer: frame.layer.clone(), ..StyleSpec::default() };
    let base = drafter.look(ctx, &style, None, path)?;
    let border = Look { lineweight: lineweight(frame.line_width.unwrap_or(0.5).clamp(0.0, 2.11)).0, ..base.clone() };
    let corners = vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    drafter.add(&ctx.space, &border, "LWPOLYLINE", lwpolyline(&corners, None, true), path);
    if let Some(title_block) = &frame.title_block {
        title(drafter, ctx, &style, title_block, [x0, y0, x1, y1], &format!("{path}.titleBlock"))?;
    }
    Ok(())
}

/// 表題欄
fn title(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &TitleBlockSpec,
    border: [f64; 4],
    path: &str,
) -> Result<(), DraftError> {
    let [_, y0, x1, y1] = border;
    let text_height = positive(spec.text_height.unwrap_or(2.5), "textHeight", path)?;
    match spec.placement.as_deref().unwrap_or("right") {
        "bottom-right" => {
            // 右下の 2 列の表（見出し・値）
            let width = positive(spec.width.unwrap_or(120.0), "width", path)?;
            let label_width = (width * 0.25).min(30.0);
            let rows: Vec<f64> = spec.rows.iter().map(|row| row.height.unwrap_or(8.0)).collect();
            let total: f64 = rows.iter().sum();
            let cells: Vec<Vec<Value>> = spec
                .rows
                .iter()
                .map(|row| {
                    let value = serde_json::json!({
                        "text": row.value.clone().unwrap_or_default(),
                        "height": row.value_height,
                        "align": row.align.clone().unwrap_or_else(|| "middle-left".into()),
                    });
                    vec![Value::String(row.label.clone().unwrap_or_default()), value]
                })
                .collect();
            let table = TableSpec {
                at: [x1 - width, y0 + total],
                columns: vec![label_width, width - label_width],
                rows: Some(rows),
                row_height: None,
                cells,
                text_height: Some(text_height),
                padding: None,
                align: None,
                column_align: None,
                fit: None,
                outer_width: Some(0.35),
                inner_width: None,
            };
            crate::table::draw(drafter, ctx, style, &table, path)
        }
        "right" => {
            // 枠の右端の縦の帯。高さのない行は、残りの高さを分け合う
            let width = positive(spec.width.unwrap_or(40.0), "width", path)?;
            let left = x1 - width;
            let fixed: f64 = spec.rows.iter().filter_map(|row| row.height).sum();
            let flexible = spec.rows.iter().filter(|row| row.height.is_none()).count();
            let rest = (y1 - y0 - fixed).max(0.0);
            if flexible == 0 && fixed > y1 - y0 + 1e-9 {
                return Err(DraftError::new("invalid-value", path, "表題欄の行の高さの合計が枠の高さを超えます"));
            }
            let base = drafter.look(ctx, style, None, path)?;
            let thin = Look { lineweight: lineweight(0.13).0, ..base.clone() };
            let thick = Look { lineweight: lineweight(0.35).0, ..base };
            drafter.add(
                &ctx.space,
                &thick,
                "LINE",
                DxfEntityData::Line { start: xyz([left, y0]), end: xyz([left, y1]) },
                path,
            );
            let text_layer = drafter.defaults.text_layer.clone();
            let text_look = drafter.look(ctx, style, text_layer.as_deref(), path)?;
            let text_style = drafter.text_style(None);
            let mut top = y1;
            for (index, row) in spec.rows.iter().enumerate() {
                let row_path = format!("{path}.rows[{index}]");
                let row_height = match row.height {
                    Some(h) => positive(h, "height", &row_path)?,
                    None => rest / flexible as f64,
                };
                let bottom = top - row_height;
                if index + 1 < spec.rows.len() {
                    let data = DxfEntityData::Line { start: xyz([left, bottom]), end: xyz([x1, bottom]) };
                    drafter.add(&ctx.space, &thin, "LINE", data, &row_path);
                }
                let mut label_room = 0.0;
                if let Some(label) = row.label.as_deref().filter(|label| !label.is_empty()) {
                    let small = text_height * 0.7;
                    let lines = [label.to_string()];
                    let (h, factor) = text::fit(&lines, width - 2.0, small, 1.0);
                    let placed = Placed {
                        at: [left + 1.0, top - 1.0],
                        lines: &lines,
                        height: h,
                        align: Align { h: 0, v: 3 },
                        rotation: 0.0,
                        width_factor: factor,
                        line_spacing: 1.0,
                        style: text_style.clone(),
                    };
                    text::put(drafter, &ctx.space, &text_look, placed, &row_path);
                    label_room = small + 1.5;
                }
                if let Some(value) = row.value.as_deref().filter(|value| !value.is_empty()) {
                    let lines: Vec<String> = value.split('\n').map(str::to_string).collect();
                    let wanted = positive(row.value_height.unwrap_or(text_height), "valueHeight", &row_path)?;
                    let (mut h, factor) = text::fit(&lines, width - 3.0, wanted, 1.0);
                    let room = (row_height - label_room - 1.0).max(0.0);
                    let needed = h * (1.0 + (lines.len() as f64 - 1.0) * 5.0 / 3.0);
                    if needed > room && needed > 0.0 {
                        h *= room / needed;
                    }
                    let align = Align::parse(row.align.as_deref(), MIDDLE_CENTER, &row_path)?;
                    let x = match align.h {
                        0 => left + 1.5,
                        1 => left + width / 2.0,
                        _ => x1 - 1.5,
                    };
                    let area_top = top - label_room;
                    let y = match align.v {
                        3 => area_top - 0.5,
                        2 => (area_top + bottom) / 2.0,
                        _ => bottom + 1.0,
                    };
                    let placed = Placed {
                        at: [x, y],
                        lines: &lines,
                        height: h,
                        align: Align { v: align.v.max(1), ..align },
                        rotation: 0.0,
                        width_factor: factor,
                        line_spacing: 1.0,
                        style: text_style.clone(),
                    };
                    text::put(drafter, &ctx.space, &text_look, placed, &row_path);
                }
                top = bottom;
            }
            Ok(())
        }
        other => Err(DraftError::new(
            "invalid-value",
            path,
            format!("placement の {other} はありません（right・bottom-right）"),
        )),
    }
}

/// ブロックを定義する。先に名前をすべて登録してから中身を描くので、ブロックの中で別のブロックを置ける
fn blocks(drafter: &mut Drafter, spec: &Spec) -> Result<(), DraftError> {
    let mut names = Vec::new();
    for (index, block) in spec.blocks.iter().enumerate() {
        let path = format!("blocks[{index}]");
        let key = block.name.to_uppercase();
        if block.name.trim().is_empty() || block.name.starts_with('*') {
            return Err(DraftError::new(
                "invalid-value",
                &path,
                "ブロックの名前は空でなく、* で始まらない名前にしてください",
            ));
        }
        if drafter.blocks.contains_key(&key) {
            return Err(DraftError::new("invalid-spec", &path, format!("ブロック {} が 2 度あります", block.name)));
        }
        let name = drafter.builder.block(&block.name);
        let base = match block.base_point {
            Some(base) => point(base, &path)?,
            None => [0.0, 0.0],
        };
        if let Some(entry) = drafter.builder.doc.blocks.iter_mut().find(|entry| entry.name == name) {
            entry.base_point = xyz(base);
            if !block.attributes.is_empty() {
                entry.flags = 2.0;
            }
        }
        drafter.blocks.insert(key, BlockInfo { name: name.clone(), attributes: block.attributes.clone(), base });
        names.push(name);
    }
    for (index, (block, name)) in spec.blocks.iter().zip(&names).enumerate() {
        let path = format!("blocks[{index}]");
        let items = parse_items(&block.items, &format!("{path}.items"))?;
        if refers_to(&items, &block.name) {
            return Err(DraftError::new(
                "invalid-spec",
                &path,
                format!("ブロック {} が自身を置いています", block.name),
            ));
        }
        let ctx = Ctx { space: Space::Block(name.clone()), scale: 1.0, style: StyleSpec::default() };
        drafter.items(&items, &ctx)?;
        for (attribute_index, attribute) in block.attributes.iter().enumerate() {
            let attribute_path = format!("{path}.attributes[{attribute_index}]");
            let look = drafter.look(&ctx, &StyleSpec::default(), None, &attribute_path)?;
            let text = attribute_text(
                drafter,
                attribute,
                attribute.value.as_deref().unwrap_or(""),
                [0.0, 0.0],
                [1.0, 1.0],
                0.0,
                &attribute_path,
            )?;
            let data = DxfEntityData::Attdef {
                text,
                tag: attribute.tag.clone(),
                prompt: attribute.prompt.clone().unwrap_or_else(|| attribute.tag.clone()),
                attribute_flags: if attribute.invisible { 1.0 } else { 0.0 },
                mtext: None,
            };
            drafter.add(&ctx.space, &look, "ATTDEF", data, &attribute_path);
        }
    }
    Ok(())
}

fn refers_to(items: &[crate::spec::Item], name: &str) -> bool {
    items.iter().any(|item| match &item.shape {
        crate::spec::Shape::Insert(insert) => insert.block.eq_ignore_ascii_case(name),
        crate::spec::Shape::Group(inner) => refers_to(inner, name),
        _ => false,
    })
}

/// 属性の文字。at・scale・rotation は配置（ブロックの中の定義なら原点・1・0）
fn attribute_text(
    drafter: &mut Drafter,
    attribute: &AttributeSpec,
    value: &str,
    at: Point,
    scale: [f64; 2],
    rotation: f64,
    path: &str,
) -> Result<DxfTextBase, DraftError> {
    let align = Align::parse(attribute.align.as_deref(), BOTTOM_LEFT, path)?;
    let local = point(attribute.at, path)?;
    let (sin, cos) = rotation.to_radians().sin_cos();
    let scaled = [local[0] * scale[0], local[1] * scale[1]];
    let position = [at[0] + scaled[0] * cos - scaled[1] * sin, at[1] + scaled[0] * sin + scaled[1] * cos];
    let height = positive(attribute.height.unwrap_or_else(|| drafter.text_height()), "height", path)? * scale[1].abs();
    let style = drafter.text_style(None);
    Ok(DxfTextBase {
        text: value.to_string(),
        insert: xyz(position),
        align_point: (align.h != 0 || align.v != 0).then(|| xyz(position)),
        height,
        rotation: (attribute.rotation.unwrap_or(0.0) + rotation).rem_euclid(360.0),
        oblique: 0.0,
        width: (scale[0] / scale[1]).abs(),
        style,
        halign: f64::from(align.h),
        valign: f64::from(align.v),
        text_generation_flag: 0.0,
    })
}

/// ブロックを置く。属性の値は、ブロックの属性の定義の既定の値を、attributes で上書きする
pub(crate) fn insert(
    drafter: &mut Drafter,
    ctx: &Ctx,
    style: &StyleSpec,
    spec: &InsertSpec,
    path: &str,
) -> Result<(), DraftError> {
    let Some(info) = drafter.blocks.get(&spec.block.to_uppercase()) else {
        return Err(DraftError::new(
            "missing-definition",
            path,
            format!("ブロック {} がありません（blocks で定義してください）", spec.block),
        ));
    };
    let (name, attributes, base) = (info.name.clone(), info.attributes.clone(), info.base);
    let look = drafter.look(ctx, style, None, path)?;
    let at = point(spec.at, path)?;
    let scale = match &spec.scale {
        None => [1.0, 1.0],
        Some(Value::Number(n)) => {
            let k = n.as_f64().unwrap_or(f64::NAN);
            [number(k, "scale", path)?, number(k, "scale", path)?]
        }
        Some(Value::Array(pair)) if pair.len() == 2 => [
            number(pair[0].as_f64().unwrap_or(f64::NAN), "scale", path)?,
            number(pair[1].as_f64().unwrap_or(f64::NAN), "scale", path)?,
        ],
        Some(_) => return Err(DraftError::new("invalid-value", path, "scale は数か [x, y] で指定してください")),
    };
    if scale[0] == 0.0 || scale[1] == 0.0 {
        return Err(DraftError::new("invalid-value", path, "scale に 0 は使えません"));
    }
    let rotation = number(spec.rotation.unwrap_or(0.0), "rotation", path)?;
    let values = spec.attributes.clone().unwrap_or_default();
    if let Some(tag) =
        values.keys().find(|tag| !attributes.iter().any(|attribute| attribute.tag.eq_ignore_ascii_case(tag)))
    {
        let known: Vec<&str> = attributes.iter().map(|attribute| attribute.tag.as_str()).collect();
        return Err(DraftError::new(
            "invalid-value",
            path,
            format!("ブロック {} に属性 {tag} はありません（{}）", spec.block, known.join("・")),
        ));
    }
    let mut attribs = Vec::new();
    for attribute in &attributes {
        let value = values
            .iter()
            .find(|(tag, _)| tag.eq_ignore_ascii_case(&attribute.tag))
            .map(|(_, value)| match value {
                Value::String(text) => text.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            })
            .or_else(|| attribute.value.clone())
            .unwrap_or_default();
        // 属性の位置は、ブロックの基点からの位置を配置の座標系に写したもの
        let relative =
            AttributeSpec { at: [attribute.at[0] - base[0], attribute.at[1] - base[1]], ..attribute.clone() };
        let text = attribute_text(drafter, &relative, &value, at, scale, rotation, path)?;
        let data = DxfEntityData::Attrib {
            text,
            tag: attribute.tag.clone(),
            attribute_flags: if attribute.invisible { 1.0 } else { 0.0 },
            mtext: None,
        };
        attribs.push(entity(drafter, &ctx.space, &look, "ATTRIB", data));
    }
    let data = DxfEntityData::Insert(DxfInsert {
        name,
        insert: xyz(at),
        xscale: scale[0],
        yscale: scale[1],
        zscale: 1.0,
        rotation: rotation.rem_euclid(360.0),
        column_count: 1.0,
        row_count: 1.0,
        column_spacing: 0.0,
        row_spacing: 0.0,
        attribs,
    });
    drafter.add(&ctx.space, &look, "INSERT", data, path);
    Ok(())
}

/// 配置に続く属性の図形（モデルの図形の並びには加えない）
fn entity(drafter: &mut Drafter, space: &Space, look: &Look, kind: &str, data: DxfEntityData) -> DxfEntity {
    let (space_kind, block) = match space {
        Space::Model => (DxfSpace::Model, None),
        Space::Paper => (DxfSpace::Paper, Some("*Paper_Space".to_string())),
        Space::Layout(name) => (DxfSpace::Paper, Some(name.clone())),
        Space::Block(name) => (DxfSpace::Block, Some(name.clone())),
    };
    DxfEntity {
        base: DxfEntityBase {
            kind: kind.to_string(),
            handle: drafter.builder.handle(),
            ordinal: 0,
            owner: None,
            space: space_kind,
            block,
            layer: look.layer.clone(),
            linetype: look.linetype.clone(),
            color_index: look.color.index,
            true_color: look.color.true_color,
            transparency: None,
            lineweight: look.lineweight,
            linetype_scale: look.linetype_scale,
            invisible: false,
            thickness: None,
            extrusion: None,
        },
        data,
    }
}
