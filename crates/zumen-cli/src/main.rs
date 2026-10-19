//! zumen のコマンド。図面の形式の判定、表示用モデル（Scene）の JSON の出力、文字と図形の取り出し、PNG への描画を行う。
#![forbid(unsafe_code)]

use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::io::{BufWriter, Write};
use std::process::ExitCode;
use zumen::extract::{EntityFilter, EntityText, entity_text, layer_names, scene_texts, select_entities};
use zumen::{BBox, Background, ReadDrawingOptions, Scene, SceneEntity};
use zumen_render::{ImageSize, RenderOptions, RenderedImage};

const USAGE: &str = "\
Usage:
  zumen detect <file>
  zumen scene <file> [--format <name>] [--view <id>] [--pretty]
  zumen texts <file> [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--format <name>] [--view <id>]
  zumen entities <file> [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--format <name>] [--view <id>]
  zumen render <file> -o <out.png|out.svg|out.pdf> [--dpi <n> | --size <w>x<h> [--margin <px>]] [--dpr <n>]
               [--region <x0>,<y0>,<x1>,<y1>] [--all-layers] [--map <out.json>]
               [--theme light|dark] [--thin] [--format <name>] [--view <id>]

Reads SXF, JWW, DXF and DWG drawings into zumen's scene model.
texts and entities write one JSON object per line, in paper millimeters (Y up) rounded to 0.001.
render writes SVG or PDF when the output ends in .svg or .pdf: the sheet (or --region) at its real size,
with text that can be searched. --dpi, --size, --dpr and --map are for PNG only.
--region selects a part of the sheet in paper millimeters, and --all-layers includes the layers
that the drawing hides. --map writes the size of the image,
the conversion between pixels and paper millimeters, and the pixel box of every entity drawn.";

struct Args {
    command: String,
    file: String,
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
    map: Option<String>,
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
        map: None,
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
            "--map" => parsed.map = Some(value("--map")?),
            _ if arg.starts_with('-') => return Err(format!("unknown option: {arg}\n\n{USAGE}")),
            _ if parsed.file.is_empty() => parsed.file = arg,
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
        "type": entity.kind,
        "text": text.text,
        "x": mm(text.x),
        "y": mm(text.y),
        "height": mm(text.height),
        "angle": round(text.angle, 3),
        "bbox": bbox_json(&text.bbox),
        "layers": layer_names(scene, entity),
    })
}

fn entity_json(scene: &Scene, entity: &SceneEntity) -> Value {
    let mut object = Map::new();
    object.insert("id".into(), json!(entity.id));
    object.insert("type".into(), json!(entity.kind));
    object.insert("source".into(), json!(entity.source));
    object.insert("layers".into(), json!(layer_names(scene, entity)));
    object.insert("bbox".into(), bbox_json(&entity.bbox));
    if let Some(text) = entity_text(entity) {
        object.insert("text".into(), json!(text.text));
    }
    let properties: Vec<Value> =
        entity.properties.iter().map(|property| json!([property.label, property.value])).collect();
    object.insert("properties".into(), Value::Array(properties));
    if let Some(placement) = &entity.placement {
        object.insert(
            "placement".into(),
            json!({ "name": placement.name, "scaleX": placement.scale_x, "scaleY": placement.scale_y }),
        );
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
            object.insert("type".into(), json!(entity.kind));
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

fn write_lines(lines: impl Iterator<Item = Value>) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for line in lines {
        serde_json::to_writer(&mut out, &line).map_err(|error| error.to_string())?;
        out.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    out.flush().map_err(|error| error.to_string())
}

fn run(args: Args) -> Result<(), String> {
    let bytes = std::fs::read(&args.file).map_err(|error| format!("{}: {error}", args.file))?;
    if args.command == "detect" {
        let format = zumen::detect(&bytes).ok_or("unknown format")?;
        println!("{format}");
        return Ok(());
    }
    let options = ReadDrawingOptions { format: args.format.clone(), view: args.view.clone() };
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
        "entities" => {
            write_lines(select_entities(&scene, &filter).into_iter().map(|entity| entity_json(&scene, entity)))?
        }
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
    for notice in &scene.notices {
        eprintln!("notice ({:?}, {}): {} × {}", notice.level, notice.code, notice.message, notice.count);
    }
    Ok(())
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
