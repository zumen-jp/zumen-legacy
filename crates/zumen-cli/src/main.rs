//! zumen のコマンド。図面の形式の判定、表示用モデル（Scene）の JSON の出力、文字と図形の取り出し、PNG への描画、
//! 線で閉じた領域（部屋）と面積の取り出し、2 つの版の比較、DXF への書き出しを行う。
#![forbid(unsafe_code)]

use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use zumen::compare::{CompareOptions, compare_scenes, diff_scene};
use zumen::extract::{
    EntityFilter, EntityText, Shape, entity_shapes, entity_text, layer_names, real_shapes, scene_texts, select_entities,
};
use zumen::regions::{ClosedRegion, RegionOptions, RegionShape, closed_regions};
use zumen::title::{TitleKey, title_block, title_fields};
use zumen::{BBox, Background, ReadDrawingOptions, ReadLimits, Scene, SceneEntity};
use zumen_render::{ImageSize, RenderOptions, RenderedImage};

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
  zumen render <file> -o <out.png|out.svg|out.pdf> [--dpi <n> | --size <w>x<h> [--margin <px>]] [--dpr <n>]
               [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--map <out.json>]
               [--theme light|dark] [--thin] [--format <name>] [--view <id>]
  zumen compare <before> <after> [--tolerance <mm>] [--style] [--all-layers] [--no-align]
                [-o <overlay.png> [--dpi <n> | --size <w>x<h>]] [--format <name>] [--view <id>]
  zumen convert <file> -o <out.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z> [--target-version <version>] [--format <name>]
  zumen edit <file> <operations.json> -o <out> [--format <name>]
  zumen create <operations.json> -o <out.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z> [--units mm|cm|m|inch|foot|unitless]

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
render writes SVG or PDF when the output ends in .svg or .pdf: the sheet (or --region) at its real size,
with text that can be searched. --dpi, --size, --dpr and --map are for PNG only.
--region selects a part of the sheet in paper millimeters, and --all-layers includes the layers
that the drawing hides. --map writes the size of the image,
the conversion between pixels and paper millimeters, and the pixel box of every entity drawn.
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
edit changes a drawing in place: it applies the operations (a JSON array of add, update, delete, add-layer and
update-layer) and keeps everything else as it was, in the same format and version (DXF, DWG, JWW and SXF). create
starts a new drawing in the format of the output and applies the operations (JWW and SXF lengths are in mm). Both write the ids of what they added and the notices as
JSON, and on an error write the number of the operation that failed.";

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
    style: bool,
    align: bool,
    limits: ReadLimits,
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
        style: false,
        align: true,
        limits: ReadLimits::default(),
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
            "--no-align" => parsed.align = false,
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
            _ if parsed.file.is_empty() => parsed.file = arg,
            _ if matches!(parsed.command.as_str(), "compare" | "edit") && parsed.other.is_none() => {
                parsed.other = Some(arg)
            }
            _ => return Err(format!("unexpected argument: {arg}")),
        }
    }
    if parsed.file.is_empty() {
        return Err(USAGE.into());
    }
    Ok(parsed)
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

fn run(args: Args) -> Result<(), String> {
    if args.command == "compare" {
        return compare(&args);
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
            let layers = (!args.layers.is_empty()).then(|| {
                let wanted: Vec<String> = args.layers.iter().map(|name| name.to_lowercase()).collect();
                scene
                    .layers
                    .iter()
                    .filter(|layer| wanted.iter().any(|name| layer.name.to_lowercase().contains(name)))
                    .map(|layer| layer.id)
                    .collect()
            });
            let options = RegionOptions {
                filter: filter.clone(),
                layers,
                tolerance: args.tolerance,
                min_area: args.min_area,
                ..RegionOptions::default()
            };
            write_lines(closed_regions(&scene, &options).iter().map(region_json))?
        }
        "entities" => write_lines(
            select_entities(&scene, &filter).into_iter().map(|entity| entity_json(&scene, entity, args.geometry)),
        )?,
        "render" => {
            let output = args.output.ok_or("render needs -o <out.png>")?;
            let options = RenderOptions {
                theme: args.theme,
                line_weights: !args.thin,
                device_pixel_ratio: args.dpr,
                hidden_layers: filter.hidden_layers.clone(),
                ..RenderOptions::default()
            };
            let extension = output.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
            if matches!(extension.as_deref(), Some("svg" | "pdf")) {
                if args.map.is_some() {
                    return Err("--map is for PNG output".into());
                }
                let (bytes, missing) = if extension.as_deref() == Some("svg") {
                    let svg = zumen_render::render_svg(&scene, args.region, &options).map_err(|e| e.to_string())?;
                    (svg.into_bytes(), Default::default())
                } else {
                    let (pdf, report) =
                        zumen_render::render_pdf(&scene, args.region, &options).map_err(|e| e.to_string())?;
                    (pdf, report.missing_glyphs)
                };
                std::fs::write(&output, bytes).map_err(|error| format!("{output}: {error}"))?;
                if !missing.is_empty() {
                    let chars: String = missing.iter().collect();
                    eprintln!("warning: no glyph in the bundled font for: {chars}");
                }
                for notice in &scene.notices {
                    eprintln!("notice ({:?}, {}): {} × {}", notice.level, notice.code, notice.message, notice.count);
                }
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
            if !image.report.missing_glyphs.is_empty() {
                let chars: String = image.report.missing_glyphs.iter().collect();
                eprintln!("warning: no glyph in the bundled font for: {chars}");
            }
        }
        other => return Err(format!("unknown command: {other}\n\n{USAGE}")),
    }
    print_notices(&scene.notices);
    Ok(())
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
