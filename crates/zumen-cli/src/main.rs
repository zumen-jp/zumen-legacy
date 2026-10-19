//! zumen のコマンド。図面の形式の判定、表示用モデル（Scene）の JSON の出力、PNG への描画を行う。
#![forbid(unsafe_code)]

use std::process::ExitCode;
use zumen::{Background, ReadDrawingOptions};
use zumen_render::RenderOptions;

const USAGE: &str = "\
Usage:
  zumen detect <file>
  zumen scene <file> [--format <name>] [--view <id>] [--pretty]
  zumen render <file> -o <out.png> [--dpi <n> | --size <w>x<h> [--margin <px>]] [--dpr <n>]
               [--theme light|dark] [--thin] [--format <name>] [--view <id>]

Reads SXF, JWW, DXF and DWG drawings into zumen's scene model (SXF, JWW and DXF for now).";

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

fn run(args: Args) -> Result<(), String> {
    let bytes = std::fs::read(&args.file).map_err(|error| format!("{}: {error}", args.file))?;
    if args.command == "detect" {
        let format = zumen::detect(&bytes).ok_or("unknown format")?;
        println!("{format}");
        return Ok(());
    }
    let options = ReadDrawingOptions { format: args.format.clone(), view: args.view.clone() };
    let scene = zumen::read(&bytes, &options).map_err(|error| error.to_string())?;
    match args.command.as_str() {
        "scene" => {
            let json = if args.pretty { serde_json::to_string_pretty(&scene) } else { serde_json::to_string(&scene) };
            println!("{}", json.map_err(|error| error.to_string())?);
        }
        "render" => {
            let output = args.output.ok_or("render needs -o <out.png>")?;
            let options = RenderOptions {
                theme: args.theme,
                line_weights: !args.thin,
                device_pixel_ratio: args.dpr,
                ..RenderOptions::default()
            };
            let (png, report) = match args.size {
                Some((width, height)) => {
                    zumen_render::render_png_fit(&scene, None, width, height, args.margin, &options)
                }
                None => zumen_render::render_png(&scene, None, args.dpi, &options),
            }
            .map_err(|error| error.to_string())?;
            std::fs::write(&output, png).map_err(|error| format!("{output}: {error}"))?;
            if !report.missing_glyphs.is_empty() {
                let chars: String = report.missing_glyphs.iter().collect();
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
