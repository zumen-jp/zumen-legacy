//! zumen のコマンド。図面の形式の判定、表示用モデル（Scene）の JSON の出力、文字と図形の取り出し、PNG への描画、
//! 線で閉じた領域（部屋）と面積の取り出し、罫線で描いた表の取り出し、製図の規則の確認、2 つの版の比較、DXF への
//! 書き出しを行う。
#![forbid(unsafe_code)]

use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use std::sync::Arc;
use zumen::checks::{CheckLevel, RuleSet, check_scene};
use zumen::compare::{CompareOptions, compare_scenes, diff_scene};
use zumen::extract::{
    EntityFilter, EntityText, Shape, entity_shapes, entity_text, layer_names, real_shapes, scene_texts, select_entities,
};
use zumen::markup::{MarkupDocument, drawing_hash};
use zumen::regions::{ClosedRegion, RegionOptions, RegionShape, closed_regions};
use zumen::tables::{RuledTable, TableOptions, ruled_tables};
use zumen::title::{TitleKey, title_block, title_fields};
use zumen::{BBox, Background, ReadDrawingOptions, ReadLimits, Scene, SceneEntity};
use zumen_render::{ImageSize, MarkupStyle, RenderMarkup, RenderOptions, RenderedImage};

const USAGE: &str = "\
Usage:
  zumen detect <file>
  zumen scene <file> [--format <name>] [--view <id>] [--pretty]
  zumen texts <file> [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--format <name>] [--view <id>]
  zumen entities <file> [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--geometry] [--format <name>]
                 [--view <id>]
  zumen title <file> [--format <name>] [--view <id>]
  zumen regions <file> [--region <x0>,<y0>,<x1>,<y1>] [--layer <name>]... [--tolerance <mm>]
                [--min-area <mm2>] [--all-layers] [--format <name>] [--view <id>]
  zumen tables <file> [--csv] [--empty] [--region <x0>,<y0>,<x1>,<y1>] [--layer <name>]... [--tolerance <mm>]
               [--all-layers] [--format <name>] [--view <id>]
  zumen check <file> [--rules <name|rules.json>] [--format <name>] [--view <id>]
  zumen render <file> -o <out.png|out.svg|out.pdf> [--dpi <n> | --size <w>x<h> [--margin <px>]] [--dpr <n>]
               [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--map <out.json>]
               [--theme light|dark] [--thin] [--format <name>] [--view <id>]
               [--markup <markup.json> [--markup-color <category>=<#rrggbb>]...]
  zumen compare <before> <after> [--tolerance <mm>] [--style] [--all-layers] [--no-align]
                [-o <overlay.png> [--dpi <n> | --size <w>x<h>]] [--format <name>] [--view <id>]
  zumen convert <file> -o <out.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z> [--target-version <version>] [--format <name>]
  zumen edit <file> <operations.json> -o <out> [--format <name>]
  zumen create <operations.json> -o <out.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z> [--units mm|cm|m|inch|foot|unitless]
  zumen draft <spec.json> -o <out.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z> [--target-version <version>] [--sheet <name>]
              [--into <template.dxf|.dwg>]
  zumen markup carry <markup.json> <before> <after> [-o <out.json>] [--view-map <before view>=<after view>]...
                     [--tolerance <mm>] [--style] [--no-align] [--ignore-hash]

Every command that reads takes --max-bytes <n>, --max-entities <n> and --timeout <seconds>, to stop
reading an untrusted drawing that is too large (P2Z and SFZ after unpacking), too complex or too slow.

Reads SXF, JWW (also JWC and JWS), DXF and DWG drawings into zumen's scene model.
texts and entities write one JSON object per line, in paper millimeters (Y up) rounded to 0.001.
--geometry adds the shapes drawn to each entity (polylines, segments, fills, hatches, points and text
parts; curves as the polyline drawn), with real for the same coordinates in real-world millimeters
when the entity sits in a scaled placement.
title finds the drawing number, revision, title, project, scale and date in the title block.
regions writes, one JSON object per line, the regions that drawn lines close (rooms, when a text names them):
the outline and holes, the area (mm²) and perimeter on the sheet and in real-world millimeters, the entities
that bound them and the texts inside. --layer keeps the lines on layers whose name contains the text (walls),
--tolerance (default 0.01 paper mm) joins ends that nearly meet, and --min-area leaves out smaller regions.
tables writes, one JSON object per line, the tables drawn with horizontal and vertical rules (schedules, area
tables, drawing lists, title blocks): the lines between rows and columns, and each cell with its row, column, the
rows and columns it spans and its text. --csv writes each table as CSV instead (a merged cell's text in its top left
place), with an empty line between tables. --empty also writes tables without any text, and --layer, --tolerance
and --region work as for regions.
check checks a drawing against the rules of a CAD standard and writes the result of each rule as JSON: how many
layers, entities, sheets or scales it checked, how many it could not (a value the drawing does not give), and each
value that breaks it with its count, entity ids and layers. --rules names a built-in rule set (mlit-cad-2025, the CAD
drafting standard of the Ministry of Land, Infrastructure, Transport and Tourism of Japan, by default) or a JSON
file of rules (docs/checks.md). The command fails when a rule of level error is broken.
render writes SVG or PDF when the output ends in .svg or .pdf: the sheet (or --region) at its real size,
with text that can be searched. --dpi, --size, --dpr and --map are for PNG only.
--region selects a part of the sheet in paper millimeters, and --all-layers includes the layers
that the drawing hides. --map writes the size of the image,
the conversion between pixels and paper millimeters, and the pixel box of every entity drawn.
--markup draws the items of a markup document (docs/markup.md) that belong to the view drawn, over the drawing:
in the color of the item, of its category (--markup-color), or red; pins and photo symbols at a fixed size in
pixels in PNG and at the paper size of a CSS pixel in SVG and PDF. A warning tells when the markup was made on
other bytes than the drawing's (its items are drawn at their points).
compare matches entities by shape and text (not by id) and writes the removed, added and changed
entities and the regions that changed as JSON; -o draws them over the unchanged ones (red before,
blue after, gray unchanged).
convert writes a drawing as DXF (AutoCAD 2013, UTF-8, or the version of --target-version: R12, 2000, 2004, 2007, 2010,
2013 or 2018, before 2007 in a code page such as ANSI_932; R12 explodes what it lacks, such as MTEXT and hatches),
DWG (AutoCAD 2013), JWW (Jw_cad 8) or SXF (SFC, P21, SFZ or P2Z of SXF Ver.3.1, or of the version of
--target-version: 2.0, 3.0 or 3.1), by the extension of the output, and reports what it could not write. SXF and JWW are mapped into the AutoCAD model: model space at real size, the
sheet as a layout with a viewport for each scale. SXF is written from that model: the layout is the sheet, each
viewport a sub-figure at its scale and each block a drawing part. JWW is written from it the same way: the layout is
the sheet and the model space in each viewport is a layer group at the viewport's scale.
edit changes a drawing in place: it applies the operations (a JSON array of add, update, delete, add-layer,
update-layer, add-block, add-text-style, add-line-type and add-dimension-style) and keeps everything else as it was, in the same format and version (DXF, DWG, JWW and SXF). create
starts a new drawing in the format of the output and applies the operations (JWW and SXF lengths are in mm). Both write the ids of what they added and the notices as
JSON, and on an error write the number of the operation that failed.
draft draws a drawing from a drawing spec (JSON: layers, blocks, sheets with a frame and title block, views at
scales, and items such as text, dimensions, hatches, tables and grid lines, decision 0018) and writes it in the
format of the output (JWW and SXF hold one sheet: --sheet chooses it, else the first); an error names the place in the spec (sheets[0].views[1].items[4]) and a code.
--into draws the spec into a DXF or DWG template in place, through the operations of edit, so that the template's
entities, handles and data stay: a sheet named after a layout of the template is drawn into it, other sheets become
new layouts, and the fields of a sheet fill the attributes of its title block. The output keeps the template's
format and version.
markup carry moves a markup document made on <before> to the revision <after> (decision 0021): for each view
with items it compares the two revisions (every layer, like compare with --all-layers), moves the items by the
offset of the revision, points their entities at the matching entities and marks each item unchanged, changed or
removed in carried. --view-map carries the items of a view to another view of <after>, such as a DXF layout to
the page of a PDF plotted from it. The markup must have been made on <before> (its drawing.sha256), unless
--ignore-hash. It writes the document to -o (or standard output) and the number of items in each state as JSON.";

struct Args {
    command: String,
    file: String,
    other: Option<String>,
    format: Option<String>,
    view: Option<String>,
    pretty: bool,
    output: Option<String>,
    dpi: f64,
    theme: Option<Background>,
    thin: bool,
    size: Option<(u32, u32)>,
    margin: f64,
    dpr: f64,
    region: Option<BBox>,
    all_layers: bool,
    geometry: bool,
    map: Option<String>,
    tolerance: f64,
    layers: Vec<String>,
    min_area: f64,
    units: String,
    target_version: Option<String>,
    sheet: Option<String>,
    into: Option<String>,
    style: bool,
    align: bool,
    csv: bool,
    empty: bool,
    rules: Option<String>,
    limits: ReadLimits,
    markup: Option<String>,
    markup_colors: Vec<(String, String)>,
    view_map: Vec<(String, String)>,
    ignore_hash: bool,
    /// markup の、サブコマンドと 3 つのファイル
    positionals: Vec<String>,
}

/// <名前>=<値>
fn parse_pair(name: &str, text: &str) -> Result<(String, String), String> {
    let (key, value) = text.split_once('=').ok_or_else(|| format!("{name} needs <name>=<value>: {text}"))?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_region(text: &str) -> Result<BBox, String> {
    let values: Vec<f64> = text
        .split(',')
        .map(|value| value.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("--region needs four numbers <x0>,<y0>,<x1>,<y1>: {text}"))?;
    let [x0, y0, x1, y1] = values[..] else {
        return Err(format!("--region needs four numbers <x0>,<y0>,<x1>,<y1>: {text}"));
    };
    if ![x0, y0, x1, y1].iter().all(|value| value.is_finite()) {
        return Err(format!("--region needs finite numbers: {text}"));
    }
    Ok(BBox { min_x: x0.min(x1), min_y: y0.min(y1), max_x: x0.max(x1), max_y: y0.max(y1) })
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let command = args.next().ok_or(USAGE)?;
    if command == "--help" || command == "-h" {
        return Err(USAGE.into());
    }
    if command == "--version" || command == "-V" {
        return Err(format!("zumen {}", env!("CARGO_PKG_VERSION")));
    }
    let mut parsed = Args {
        command,
        file: String::new(),
        other: None,
        format: None,
        view: None,
        pretty: false,
        output: None,
        dpi: 150.0,
        theme: None,
        thin: false,
        size: None,
        margin: 0.0,
        dpr: 1.0,
        region: None,
        all_layers: false,
        geometry: false,
        map: None,
        tolerance: 0.01,
        layers: Vec::new(),
        min_area: 0.0,
        units: "mm".into(),
        target_version: None,
        sheet: None,
        into: None,
        style: false,
        align: true,
        csv: false,
        empty: false,
        rules: None,
        limits: ReadLimits::default(),
        markup: None,
        markup_colors: Vec::new(),
        view_map: Vec::new(),
        ignore_hash: false,
        positionals: Vec::new(),
    };
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--format" => parsed.format = Some(value("--format")?),
            "--view" => parsed.view = Some(value("--view")?),
            "--pretty" => parsed.pretty = true,
            "-o" | "--output" => parsed.output = Some(value("--output")?),
            "--dpi" => parsed.dpi = value("--dpi")?.parse().map_err(|_| "--dpi needs a number")?,
            "--theme" => {
                parsed.theme = Some(match value("--theme")?.as_str() {
                    "light" => Background::Light,
                    "dark" => Background::Dark,
                    other => return Err(format!("unknown theme: {other}")),
                })
            }
            "--thin" => parsed.thin = true,
            "--size" => {
                let text = value("--size")?;
                let (w, h) = text.split_once('x').ok_or("--size needs <width>x<height>")?;
                parsed.size = Some((w.parse().map_err(|_| "bad --size")?, h.parse().map_err(|_| "bad --size")?));
            }
            "--margin" => parsed.margin = value("--margin")?.parse().map_err(|_| "--margin needs a number")?,
            "--dpr" => parsed.dpr = value("--dpr")?.parse().map_err(|_| "--dpr needs a number")?,
            "--region" => parsed.region = Some(parse_region(&value("--region")?)?),
            "--all-layers" => parsed.all_layers = true,
            "--geometry" => parsed.geometry = true,
            "--map" => parsed.map = Some(value("--map")?),
            "--tolerance" => {
                parsed.tolerance = value("--tolerance")?.parse().map_err(|_| "--tolerance needs a number")?
            }
            "--layer" => parsed.layers.push(value("--layer")?),
            "--min-area" => parsed.min_area = value("--min-area")?.parse().map_err(|_| "--min-area needs a number")?,
            "--style" => parsed.style = true,
            "--units" => parsed.units = value("--units")?,
            "--target-version" => parsed.target_version = Some(value("--target-version")?),
            "--sheet" => parsed.sheet = Some(value("--sheet")?),
            "--into" => parsed.into = Some(value("--into")?),
            "--no-align" => parsed.align = false,
            "--csv" => parsed.csv = true,
            "--empty" => parsed.empty = true,
            "--rules" => parsed.rules = Some(value("--rules")?),
            "--markup" => parsed.markup = Some(value("--markup")?),
            "--markup-color" => parsed.markup_colors.push(parse_pair("--markup-color", &value("--markup-color")?)?),
            "--view-map" => parsed.view_map.push(parse_pair("--view-map", &value("--view-map")?)?),
            "--ignore-hash" => parsed.ignore_hash = true,
            "--max-bytes" => {
                parsed.limits.max_bytes = Some(value("--max-bytes")?.parse().map_err(|_| "--max-bytes needs a count")?)
            }
            "--max-entities" => {
                parsed.limits.max_entities =
                    Some(value("--max-entities")?.parse().map_err(|_| "--max-entities needs a count")?)
            }
            "--timeout" => {
                let seconds: f64 = value("--timeout")?.parse().map_err(|_| "--timeout needs seconds")?;
                let duration =
                    std::time::Duration::try_from_secs_f64(seconds).map_err(|_| "--timeout needs seconds")?;
                parsed.limits.max_duration = Some(duration);
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}\n\n{USAGE}")),
            _ if parsed.command == "markup" => parsed.positionals.push(arg),
            _ if parsed.file.is_empty() => parsed.file = arg,
            _ if matches!(parsed.command.as_str(), "compare" | "edit") && parsed.other.is_none() => {
                parsed.other = Some(arg)
            }
            _ => return Err(format!("unexpected argument: {arg}")),
        }
    }
    if parsed.command == "markup" {
        if parsed.positionals.len() != 4 || parsed.positionals[0] != "carry" {
            return Err(USAGE.into());
        }
        return Ok(parsed);
    }
    if parsed.file.is_empty() {
        return Err(USAGE.into());
    }
    Ok(parsed)
}

/// --layer の名前を含むレイヤの id。--layer がなければ None（すべてのレイヤ）
fn layer_ids(scene: &Scene, names: &[String]) -> Option<HashSet<u32>> {
    (!names.is_empty()).then(|| {
        let wanted: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
        scene
            .layers
            .iter()
            .filter(|layer| wanted.iter().any(|name| layer.name.to_lowercase().contains(name)))
            .map(|layer| layer.id)
            .collect()
    })
}

/// 出力の数を丸める（用紙の mm は 0.001 まで、px は 0.1 まで）。-0 は 0 にする
fn round(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    let rounded = (value * factor).round() / factor;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn mm(value: f64) -> f64 {
    round(value, 3)
}

fn bbox_json(b: &BBox) -> Value {
    json!([mm(b.min_x), mm(b.min_y), mm(b.max_x), mm(b.max_y)])
}

fn text_json(scene: &Scene, entity: &SceneEntity, text: &EntityText) -> Value {
    json!({
        "id": entity.id,
        "kind": entity.kind,
        "type": entity.type_name,
        "text": text.text,
        "x": mm(text.x),
        "y": mm(text.y),
        "height": mm(text.height),
        "angle": round(text.angle, 3),
        "bbox": bbox_json(&text.bbox),
        "layers": layer_names(scene, entity),
    })
}

fn point_json(x: f64, y: f64) -> Value {
    json!([mm(x), mm(y)])
}

fn points_json(points: &[f64]) -> Value {
    Value::Array(points.as_chunks::<2>().0.iter().map(|[x, y]| point_json(*x, *y)).collect())
}

/// 形の座標。polyline は点の並び、segments は 2 点の組の並び、fill と hatch は輪の並び、point と text は 1 点
fn coordinates_json(shape: &Shape) -> Value {
    match shape {
        Shape::Polyline { points, .. } => points_json(points),
        Shape::Segments { points } => Value::Array(
            points
                .as_chunks::<4>()
                .0
                .iter()
                .map(|[x0, y0, x1, y1]| json!([point_json(*x0, *y0), point_json(*x1, *y1)]))
                .collect(),
        ),
        Shape::Fill { rings } | Shape::Hatch { rings } => {
            Value::Array(rings.iter().map(|ring| points_json(ring)).collect())
        }
        Shape::Point { x, y } | Shape::Text { x, y, .. } => point_json(*x, *y),
    }
}

/// 形（shape と用紙の mm の座標、配置の中なら実寸の mm の座標の real）
fn shape_json(shape: &Shape, real: Option<&Shape>) -> Value {
    let mut object = Map::new();
    object.insert("shape".into(), json!(shape.name()));
    if let Shape::Text { text, .. } = shape {
        object.insert("text".into(), json!(text));
    }
    let key = match shape {
        Shape::Polyline { .. } => "points",
        Shape::Segments { .. } => "segments",
        Shape::Fill { .. } | Shape::Hatch { .. } => "rings",
        Shape::Point { .. } | Shape::Text { .. } => "point",
    };
    object.insert(key.into(), coordinates_json(shape));
    match shape {
        Shape::Polyline { closed, .. } => {
            object.insert("closed".into(), json!(closed));
        }
        Shape::Text { height, angle, .. } => {
            object.insert("height".into(), json!(mm(*height)));
            object.insert("angle".into(), json!(round(*angle, 3)));
        }
        _ => {}
    }
    if let Some(real) = real {
        object.insert("real".into(), coordinates_json(real));
    }
    Value::Object(object)
}

fn entity_json(scene: &Scene, entity: &SceneEntity, geometry: bool) -> Value {
    let mut object = Map::new();
    object.insert("id".into(), json!(entity.id));
    object.insert("kind".into(), json!(entity.kind));
    object.insert("type".into(), json!(entity.type_name));
    object.insert("source".into(), json!(entity.source));
    object.insert("layers".into(), json!(layer_names(scene, entity)));
    object.insert("bbox".into(), bbox_json(&entity.bbox));
    if let Some(text) = entity_text(entity) {
        object.insert("text".into(), json!(text.text));
    }
    let properties: Vec<Value> =
        entity.properties.iter().map(|property| json!([property.label, property.value])).collect();
    object.insert("properties".into(), Value::Array(properties));
    // 形式によらない key → 整形する前の値（なければ文字列の値）
    let values: Map<String, Value> = entity
        .properties
        .iter()
        .map(|property| {
            let value = property.raw.as_ref().map_or_else(|| json!(property.value), |raw| json!(raw));
            (property.key.to_string(), value)
        })
        .collect();
    object.insert("values".into(), Value::Object(values));
    if let Some(placement) = &entity.placement {
        object.insert(
            "placement".into(),
            json!({ "name": placement.name, "scaleX": placement.scale_x, "scaleY": placement.scale_y }),
        );
    }
    if geometry {
        let real = real_shapes(entity);
        let shapes: Vec<Value> = entity_shapes(entity, None)
            .iter()
            .enumerate()
            .map(|(index, shape)| shape_json(shape, real.as_ref().and_then(|real| real.get(index))))
            .collect();
        object.insert("geometry".into(), Value::Array(shapes));
    }
    Value::Object(object)
}

fn map_json(scene: &Scene, image: &RenderedImage, options: &RenderOptions) -> Value {
    let px = |value: f64| round(value, 1);
    let entities: Vec<Value> = zumen_render::drawn_entities(scene, &image.view, image.width, image.height, options)
        .into_iter()
        .filter_map(|entity| {
            let b = image.view.pixel_box(&entity.bbox, image.width, image.height)?;
            let mut object = Map::new();
            object.insert("id".into(), json!(entity.id));
            object.insert("kind".into(), json!(entity.kind));
            object.insert("type".into(), json!(entity.type_name));
            object.insert("box".into(), json!([px(b.left), px(b.top), px(b.right), px(b.bottom)]));
            if let Some(text) = entity_text(entity) {
                object.insert("text".into(), json!(text.text));
            }
            Some(Value::Object(object))
        })
        .collect();
    json!({
        "width": image.width,
        "height": image.height,
        "scale": image.view.scale,
        "offsetX": image.view.offset_x,
        "offsetY": image.view.offset_y,
        "region": bbox_json(&image.view.visible_region(image.width, image.height)),
        "entities": entities,
    })
}

fn ring_json(ring: &[f64]) -> Value {
    Value::Array(ring.as_chunks::<2>().0.iter().map(|[x, y]| json!([mm(*x), mm(*y)])).collect())
}

fn region_shape_json(shape: &RegionShape) -> Value {
    json!({
        "area": mm(shape.area),
        "outlineArea": mm(shape.outline_area),
        "perimeter": mm(shape.perimeter),
        "outline": ring_json(&shape.outline),
        "holes": shape.holes.iter().map(|hole| ring_json(hole)).collect::<Vec<_>>(),
    })
}

/// 閉じた領域。用紙の mm の形と大きさに、配置の中なら real（実寸の mm）、境界の図形と中の文字を添える
fn region_json(region: &ClosedRegion) -> Value {
    let Value::Object(mut object) = region_shape_json(&region.paper) else { unreachable!() };
    object.insert("bbox".into(), bbox_json(&region.bbox));
    object.insert(
        "placement".into(),
        json!(region.placement.as_ref().map(|placement| json!({
            "name": placement.name,
            "scaleX": placement.scale_x,
            "scaleY": placement.scale_y,
        }))),
    );
    object.insert("real".into(), json!(region.real.as_ref().map(region_shape_json)));
    object.insert("boundary".into(), json!(region.boundary));
    let texts: Vec<Value> = region
        .texts
        .iter()
        .map(|text| json!({ "id": text.entity, "text": text.text, "x": mm(text.x), "y": mm(text.y) }))
        .collect();
    object.insert("texts".into(), Value::Array(texts));
    Value::Object(object)
}

fn table_json(table: &RuledTable) -> Value {
    let cells: Vec<Value> = table
        .cells
        .iter()
        .map(|cell| {
            let texts: Vec<Value> = cell
                .texts
                .iter()
                .map(|text| json!({ "id": text.entity, "text": text.text, "x": mm(text.x), "y": mm(text.y) }))
                .collect();
            json!({
                "row": cell.row,
                "column": cell.column,
                "rowSpan": cell.row_span,
                "columnSpan": cell.column_span,
                "bbox": bbox_json(&cell.bbox),
                "text": cell.text,
                "texts": texts,
            })
        })
        .collect();
    let (rows, columns) = table.size();
    json!({
        "bbox": bbox_json(&table.bbox),
        "placement": table.placement.as_ref().map(|placement| json!({
            "name": placement.name,
            "scaleX": placement.scale_x,
            "scaleY": placement.scale_y,
        })),
        "rowCount": rows,
        "columnCount": columns,
        "columns": table.columns.iter().map(|x| mm(*x)).collect::<Vec<_>>(),
        "rows": table.rows.iter().map(|y| mm(*y)).collect::<Vec<_>>(),
        "cells": cells,
        "boundary": table.boundary,
    })
}

/// RFC 4180 の CSV。区切り・引用符・改行を含む値を引用符で囲み、引用符は 2 つ重ねる
fn table_csv(table: &RuledTable) -> String {
    let mut out = String::new();
    for row in table.grid() {
        let fields: Vec<String> = row
            .iter()
            .map(|field| {
                if field.contains([',', '"', '\n', '\r']) {
                    format!("\"{}\"", field.replace('"', "\"\""))
                } else {
                    field.clone()
                }
            })
            .collect();
        out.push_str(&fields.join(","));
        out.push_str("\r\n");
    }
    out
}

fn write_lines(lines: impl Iterator<Item = Value>) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for line in lines {
        serde_json::to_writer(&mut out, &line).map_err(|error| error.to_string())?;
        out.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    out.flush().map_err(|error| error.to_string())
}

/// 比べた図形の要約（id、種類、外接矩形、文字）
fn brief_json(entity: &SceneEntity) -> Value {
    let mut object = Map::new();
    object.insert("id".into(), json!(entity.id));
    object.insert("kind".into(), json!(entity.kind));
    object.insert("type".into(), json!(entity.type_name));
    object.insert("bbox".into(), bbox_json(&entity.bbox));
    if let Some(text) = entity_text(entity) {
        object.insert("text".into(), json!(text.text));
    }
    Value::Object(object)
}

fn compare(args: &Args) -> Result<(), String> {
    let other = args.other.as_ref().ok_or("compare needs two files: <before> <after>")?;
    let options = ReadDrawingOptions { format: args.format.clone(), view: args.view.clone(), limits: args.limits };
    let read = |path: &str| -> Result<Scene, String> {
        if let Some(max) = args.limits.max_bytes
            && std::fs::metadata(path).map_err(|error| format!("{path}: {error}"))?.len() > max
        {
            return Err(format!("{path}: {}", zumen::limit_error(zumen::Limit::Bytes, &args.limits)));
        }
        let bytes = std::fs::read(path).map_err(|error| format!("{path}: {error}"))?;
        zumen::read(&bytes, &options).map_err(|error| format!("{path}: {error}"))
    };
    let (before, after) = (read(&args.file)?, read(other)?);
    let compare_options =
        CompareOptions { tolerance: args.tolerance, style: args.style, all_layers: args.all_layers, align: args.align };
    let diff = compare_scenes(&before, &after, &compare_options);
    let regions: Vec<Value> = diff
        .regions
        .iter()
        .map(|region| {
            json!({
                "bbox": bbox_json(&region.bbox),
                "removed": region.removed,
                "added": region.added,
                "changed": region.changed,
            })
        })
        .collect();
    let summary = json!({
        "unchanged": diff.unchanged.len(),
        "removed": diff.removed.len(),
        "added": diff.added.len(),
        "changed": diff.changed.len(),
        "offset": [mm(diff.offset.0), mm(diff.offset.1)],
        "regions": regions,
        "removedEntities": diff.removed.iter().map(|&i| brief_json(&before.entities[i])).collect::<Vec<_>>(),
        "addedEntities": diff.added.iter().map(|&i| brief_json(&after.entities[i])).collect::<Vec<_>>(),
        "changedEntities": diff
            .changed
            .iter()
            .map(|&(a, b)| json!({ "before": brief_json(&before.entities[a]), "after": brief_json(&after.entities[b]) }))
            .collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?);
    if let Some(output) = &args.output {
        let overlay = diff_scene(&before, &after, &diff);
        let size = match args.size {
            Some((width, height)) => ImageSize::Fit { width, height, margin: args.margin },
            None => ImageSize::Dpi(args.dpi),
        };
        let options = RenderOptions { line_weights: !args.thin, ..RenderOptions::default() };
        let image =
            zumen_render::render_image(&overlay, args.region, size, &options).map_err(|error| error.to_string())?;
        std::fs::write(output, &image.png).map_err(|error| format!("{output}: {error}"))?;
    }
    Ok(())
}

/// 操作の JSON のファイルを読む
fn operations(path: &str) -> Result<Vec<zumen::edit::EditOperation>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    zumen::edit::parse_operations(&text).map_err(|error| format!("{path}: {error}"))
}

/// 編集の結果（足した id と notices）を書き、図面をファイルに書く
fn write_edited(edited: &zumen::edit::Edited, output: &str) -> Result<(), String> {
    std::fs::write(output, &edited.bytes).map_err(|error| format!("{output}: {error}"))?;
    let notices: Vec<Value> = edited
        .notices
        .iter()
        .map(|notice| json!({ "code": notice.code, "message": notice.message, "count": notice.count }))
        .collect();
    let summary = json!({ "created": edited.created, "notices": notices });
    println!("{}", serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?);
    Ok(())
}

/// 編集のエラー（code と操作の番号を含む）
fn edit_error(error: zumen::edit::EditError) -> String {
    match error.operation {
        Some(index) => format!("{} (operation {index}, {})", error.message, error.code),
        None => format!("{} ({})", error.message, error.code),
    }
}

/// 出力のファイルの拡張子（書く形式）
fn extension(output: &str) -> String {
    std::path::Path::new(output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// マークアップの文書のファイルを読む
fn read_markup(path: &str) -> Result<MarkupDocument, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    MarkupDocument::from_json(&text).map_err(|error| format!("{path}: {error} ({})", error.code))
}

/// markup carry: マークアップの文書を新しい版へ移し、文書と、印ごとの項目の数を書く
fn markup_command(args: &Args) -> Result<(), String> {
    let [_, markup, before, after] = &args.positionals[..] else {
        return Err(USAGE.into());
    };
    let document = read_markup(markup)?;
    let before_bytes = std::fs::read(before).map_err(|error| format!("{before}: {error}"))?;
    let after_bytes = std::fs::read(after).map_err(|error| format!("{after}: {error}"))?;
    let options = zumen::CarryOptions {
        compare: CompareOptions { tolerance: args.tolerance, style: args.style, all_layers: true, align: args.align },
        views: args.view_map.iter().cloned().collect(),
        limits: args.limits,
        ignore_hash: args.ignore_hash,
    };
    let carried = zumen::carry_markup(&document, &before_bytes, &after_bytes, &options)
        .map_err(|error| format!("{error} ({})", error.code))?;
    let json = format!("{}\n", carried.document.to_json(true));
    match &args.output {
        Some(output) => std::fs::write(output, json).map_err(|error| format!("{output}: {error}"))?,
        None => print!("{json}"),
    }
    let counts = json!({ "unchanged": carried.unchanged, "changed": carried.changed, "removed": carried.removed });
    if args.output.is_some() {
        println!("{counts}");
    } else {
        eprintln!("{counts}");
    }
    Ok(())
}

fn run(args: Args) -> Result<(), String> {
    if args.command == "compare" {
        return compare(&args);
    }
    if args.command == "markup" {
        return markup_command(&args);
    }
    if args.command == "draft" {
        let output = args.output.as_deref().ok_or("draft needs -o <out.dxf|out.dwg|out.jww|out.sfc|...>")?;
        let spec = std::fs::read_to_string(&args.file).map_err(|error| format!("{}: {error}", args.file))?;
        if let Some(template) = &args.into {
            if args.target_version.is_some() {
                return Err("--into keeps the template's version; leave out --target-version".into());
            }
            let bytes = std::fs::read(template).map_err(|error| format!("{template}: {error}"))?;
            let options = zumen::DraftIntoOptions {
                sheet: args.sheet.clone(),
                read: ReadDrawingOptions { format: args.format.clone(), view: None, limits: args.limits },
            };
            let drafted =
                zumen::draft_into(&bytes, &spec, &options).map_err(|error| format!("{error} ({})", error.code))?;
            let format = zumen::detect(&drafted.bytes).unwrap_or_default();
            if extension(output) != format {
                return Err(format!("{output}: --into writes {} like the template", format.to_uppercase()));
            }
            std::fs::write(output, &drafted.bytes).map_err(|error| format!("{output}: {error}"))?;
            print_notices(&drafted.notices);
            return Ok(());
        }
        let options = zumen::DraftOptions {
            write: zumen::WriteOptions { version: args.target_version.clone() },
            sheet: args.sheet.clone(),
        };
        let drafted =
            zumen::draft(&spec, &extension(output), &options).map_err(|error| format!("{error} ({})", error.code))?;
        std::fs::write(output, &drafted.bytes).map_err(|error| format!("{output}: {error}"))?;
        print_notices(&drafted.notices);
        return Ok(());
    }
    if args.command == "create" {
        let output = args.output.as_deref().ok_or("create needs -o <out.dxf|out.dwg|out.jww|out.sfc|...>")?;
        let created = zumen::create(&extension(output), &args.units, &operations(&args.file)?).map_err(edit_error)?;
        return write_edited(&created, output);
    }
    // 大きすぎるファイルは、メモリに読む前に断る
    if let Some(max) = args.limits.max_bytes
        && std::fs::metadata(&args.file).map_err(|error| format!("{}: {error}", args.file))?.len() > max
    {
        return Err(zumen::limit_error(zumen::Limit::Bytes, &args.limits).to_string());
    }
    let bytes = std::fs::read(&args.file).map_err(|error| format!("{}: {error}", args.file))?;
    if args.command == "detect" {
        let format = zumen::detect(&bytes).ok_or("unknown format")?;
        println!("{format}");
        return Ok(());
    }
    if args.command == "edit" {
        let output = args.output.as_deref().ok_or("edit needs -o <out>")?;
        let path = args.other.as_deref().ok_or("edit needs a file of operations: <file> <operations.json>")?;
        let options = ReadDrawingOptions { format: args.format.clone(), view: None, limits: args.limits };
        let edited = zumen::edit(&bytes, &operations(path)?, &options).map_err(edit_error)?;
        return write_edited(&edited, output);
    }
    if args.command == "convert" {
        let output = args.output.ok_or("convert needs -o <out.dxf|out.dwg|out.jww|out.sfc|...>")?;
        // 書く形式は出力のファイルの拡張子
        let to = extension(&output);
        let options = ReadDrawingOptions { format: args.format.clone(), view: None, limits: args.limits };
        let write = zumen::WriteOptions { version: args.target_version.clone() };
        let converted = zumen::convert_with(&bytes, &to, &options, &write).map_err(|error| error.to_string())?;
        std::fs::write(&output, &converted.bytes).map_err(|error| format!("{output}: {error}"))?;
        print_notices(&converted.notices);
        return Ok(());
    }
    let options = ReadDrawingOptions { format: args.format.clone(), view: args.view.clone(), limits: args.limits };
    let scene = zumen::read(&bytes, &options).map_err(|error| error.to_string())?;
    let filter = EntityFilter { region: args.region, hidden_layers: args.all_layers.then(HashSet::new) };
    match args.command.as_str() {
        "scene" => {
            let json = if args.pretty { serde_json::to_string_pretty(&scene) } else { serde_json::to_string(&scene) };
            println!("{}", json.map_err(|error| error.to_string())?);
        }
        "texts" => {
            write_lines(scene_texts(&scene, &filter).iter().map(|(entity, text)| text_json(&scene, entity, text)))?
        }
        "check" => {
            let name = args.rules.as_deref().unwrap_or("mlit-cad-2025");
            let rules = if name.ends_with(".json") {
                let json = std::fs::read_to_string(name).map_err(|error| format!("{name}: {error}"))?;
                RuleSet::from_json(&json).map_err(|error| format!("{name}: {error}"))?
            } else {
                RuleSet::named_or_json(name)?
            };
            let report = check_scene(&scene, &rules);
            println!("{}", serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?);
            let errors = report.count(CheckLevel::Error);
            if errors > 0 {
                return Err(format!("{errors} values break rules of level error"));
            }
        }
        "title" => {
            let mut object = Map::new();
            let best = title_block(&scene);
            for key in TitleKey::ALL {
                let value = best.iter().find(|field| field.key == key).map(|field| field.value.clone());
                object.insert(key.name().into(), json!(value));
            }
            let fields: Vec<Value> = title_fields(&scene)
                .into_iter()
                .map(|field| {
                    json!({
                        "key": field.key.name(),
                        "value": field.value,
                        "label": field.label,
                        "source": field.source.name(),
                        "entity": field.entity,
                        "labelEntity": field.label_entity,
                        "x": mm(field.x),
                        "y": mm(field.y),
                    })
                })
                .collect();
            object.insert("fields".into(), Value::Array(fields));
            println!("{}", serde_json::to_string_pretty(&Value::Object(object)).map_err(|error| error.to_string())?);
        }
        "regions" => {
            let options = RegionOptions {
                filter: filter.clone(),
                layers: layer_ids(&scene, &args.layers),
                tolerance: args.tolerance,
                min_area: args.min_area,
                ..RegionOptions::default()
            };
            write_lines(closed_regions(&scene, &options).iter().map(region_json))?
        }
        "tables" => {
            let options = TableOptions {
                filter: filter.clone(),
                layers: layer_ids(&scene, &args.layers),
                tolerance: args.tolerance,
                empty: args.empty,
                ..TableOptions::default()
            };
            let tables = ruled_tables(&scene, &options);
            if args.csv {
                let csv: Vec<String> = tables.iter().map(table_csv).collect();
                print!("{}", csv.join("\r\n"));
            } else {
                write_lines(tables.iter().map(table_json))?
            }
        }
        "entities" => write_lines(
            select_entities(&scene, &filter).into_iter().map(|entity| entity_json(&scene, entity, args.geometry)),
        )?,
        "render" => {
            let output = args.output.ok_or("render needs -o <out.png>")?;
            let markup = match &args.markup {
                Some(path) => {
                    let document = read_markup(path)?;
                    if document.drawing.sha256 != drawing_hash(&bytes) {
                        eprintln!(
                            "warning: {path} was made on another revision (SHA-256 {}); its items are drawn at their points",
                            document.drawing.sha256
                        );
                    }
                    let items = document.items_in_view(scene.view.as_deref()).cloned().collect();
                    let style = MarkupStyle { categories: args.markup_colors.iter().cloned().collect(), color: None };
                    Some(Arc::new(RenderMarkup { items, style }))
                }
                None => None,
            };
            let options = RenderOptions {
                theme: args.theme,
                line_weights: !args.thin,
                device_pixel_ratio: args.dpr,
                hidden_layers: filter.hidden_layers.clone(),
                markup,
                ..RenderOptions::default()
            };
            let extension = output.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
            if matches!(extension.as_deref(), Some("svg" | "pdf")) {
                if args.map.is_some() {
                    return Err("--map is for PNG output".into());
                }
                let (bytes, report) = if extension.as_deref() == Some("svg") {
                    let (svg, report) =
                        zumen_render::render_svg(&scene, args.region, &options).map_err(|e| e.to_string())?;
                    (svg.into_bytes(), report)
                } else {
                    zumen_render::render_pdf(&scene, args.region, &options).map_err(|e| e.to_string())?
                };
                std::fs::write(&output, bytes).map_err(|error| format!("{output}: {error}"))?;
                print_report(&report);
                print_notices(&scene.notices);
                return Ok(());
            }
            let size = match args.size {
                Some((width, height)) => ImageSize::Fit { width, height, margin: args.margin },
                None => ImageSize::Dpi(args.dpi),
            };
            let image =
                zumen_render::render_image(&scene, args.region, size, &options).map_err(|error| error.to_string())?;
            std::fs::write(&output, &image.png).map_err(|error| format!("{output}: {error}"))?;
            if let Some(map) = &args.map {
                let json = serde_json::to_string(&map_json(&scene, &image, &options)).map_err(|e| e.to_string())?;
                std::fs::write(map, json).map_err(|error| format!("{map}: {error}"))?;
            }
            print_report(&image.report);
        }
        other => return Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
    print_notices(&scene.notices);
    Ok(())
}

/// 描けなかった文字と内容（underlay など）
fn print_report(report: &zumen_render::RenderReport) {
    if !report.missing_glyphs.is_empty() {
        let chars: String = report.missing_glyphs.iter().collect();
        eprintln!("warning: no glyph in the bundled font for: {chars}");
    }
    print_notices(&report.notices);
}

fn print_notices(notices: &[zumen::Notice]) {
    for notice in notices {
        eprintln!("notice ({:?}, {}): {} × {}", notice.level, notice.code, notice.message, notice.count);
    }
}

fn main() -> ExitCode {
    match parse(std::env::args().skip(1)).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
