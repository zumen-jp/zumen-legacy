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
use zumen_render::{ImageSize, MarkupStyle, RenderMarkup, RenderOptions, RenderedImage, UnderlayContent};

const USAGE: &str = "\
使い方:
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
              [--into <template.dxf|.dwg|.jww|.sfc|.p21|.sfz|.p2z>]
  zumen markup carry <markup.json> <before> <after> [-o <out.json>] [--view-map <before view>=<after view>]...
                     [--tolerance <mm>] [--style] [--no-align] [--ignore-hash]

読み取りを行うコマンドには --max-bytes <n>、--max-entities <n>、--timeout <秒> を指定できます。
大きすぎる図面（P2Z・SFZ は展開後の大きさ）、図形が多すぎる図面、時間がかかりすぎる図面の読み取りを停止します。

SXF・JWW（JWC・JWS を含む）・DXF・DWG・PDF を共通の図面モデルに読み取ります。
PDF のページはビューです（2 ページ目は --view 2）。線・文字・画像を解析や選択に使い、render では
ページ自体を下に描きます。SVG・PDF に埋め込むページ画像の解像度は --dpi（既定 150）です。
PDF は convert・edit・draft の入力には使えません。

texts・entities は 1 行に 1 つの JSON を出力します。座標は用紙上の mm（Y 軸は上向き）、小数点以下 3 桁です。
--geometry は折線・線分・塗り・ハッチング・点・文字の形も返します。曲線は描画時の折線です。
縮尺付きの配置内にある図形には、同じ座標を実寸の mm にした real も返します。
title は表題欄の図面番号・改訂・図面名・工事名・尺度・日付を取り出します。

regions は線で囲まれた領域（室名があれば部屋）を 1 行に 1 つの JSON で返します。
外形・穴・用紙上と実寸の面積（mm²）と周長・境界の図形・領域内の文字を含みます。
--layer は名前に指定文字を含むレイヤの線だけを使い、--tolerance（既定は用紙上 0.01 mm）は
近い端点をつなぎ、--min-area は小さな領域を除きます。
tables は縦横の罫線で描かれた表（建具表・面積表・図面一覧・表題欄）を 1 行に 1 つの JSON で返します。
行と列の区切り、各セルの位置・結合範囲・文字を含みます。--csv は表を CSV にし、結合セルの文字を
左上に置き、表の間に空行を入れます。--empty は文字のない表も含めます。
--layer・--tolerance・--region は regions と同じです。

check は CAD 製図基準に照らし、規則ごとの確認件数・値がなく確認できない件数・違反した値と件数・
図形の id・レイヤを JSON で返します。--rules は組み込みの規則集（既定は国土交通省の CAD 製図基準
mlit-cad-2025）、または規則集の JSON ファイル（docs/checks.md）です。error の違反があれば失敗します。

render は出力先の拡張子が .svg・.pdf なら、用紙（または --region）を実寸で出力し、文字を検索できます。
--size・--dpr・--map は PNG 用です。--dpi は SVG・PDF に埋め込む PDF ページの解像度にも使います。
--region は用紙上の mm で範囲を指定し、--all-layers は非表示レイヤも含めます。
--map は画像サイズ、ピクセルと用紙上の mm の変換、描画した図形のピクセル範囲を出力します。
--markup は同じビューのマークアップ（docs/markup.md）を重ねます。色は項目の指定、カテゴリの指定
（--markup-color）、赤の順で選びます。ピン・写真記号は PNG ではピクセル単位の固定サイズ、SVG・PDF では
CSS ピクセルに相当する用紙上のサイズです。元の図面とバイト列が違う場合は警告し、指定位置に描きます。
compare は形と文字で図形を照合し、削除・追加・変更された図形と変更領域を JSON で返します。
-o は変更前を赤、変更後を青、変わらない部分を灰色で重ねた画像を出力します。

convert は出力先の拡張子に応じて DXF・DWG・JWW・SXF に変換し、書き出せない内容を報告します。
DXF は AutoCAD 2013・UTF-8 が既定で、--target-version は R12・2000・2004・2007・2010・2013・2018 です。
2007 より前は ANSI_932 などのコードページを使い、R12 にない MTEXT・ハッチングなどは分解します。
DWG は AutoCAD 2013、JWW は Jw_cad 8、SXF は Ver.3.1 の SFC・P21・SFZ・P2Z です。
SXF の --target-version は 2.0・3.0・3.1 です。SXF・JWW は AutoCAD のモデルを経由します。
モデル空間は実寸、用紙はレイアウト、縮尺ごとのビューポートは SXF の部分図・JWW のレイヤグループに対応し、
SXF の作図部品はブロックに対応します。

edit は JSON 配列の操作（add・update・delete・add-layer・update-layer・add-block・add-text-style・
add-line-type・add-dimension-style）を適用します。操作対象以外は形式・版を含めて保ちます。
create は出力形式で図面を新規作成して操作を適用します。JWW・SXF の長さの単位は mm です。
両コマンドは追加した図形の id と注意事項を JSON で返し、失敗した操作の番号をエラーに含めます。
draft は JSON の図面仕様（レイヤ・部品・用紙・図枠・表題欄・縮尺付きビュー・文字・寸法・ハッチング・
表・通り芯。決定 0018）から作図し、出力先の形式で書き出します。
JWW・SXF は 1 枚の用紙を保持します（--sheet で選択、省略時は先頭）。仕様の誤りは場所と code で返します。
--into は DXF・DWG・JWW・SXF のテンプレートに edit の操作を通して描き、元の図形・ハンドル・データを保ちます。
DXF・DWG では同名のレイアウトに描き、なければ追加します。用紙の fields は表題欄の属性を埋めます。
JWW・SXF は用紙を 1 枚選び（--sheet、省略時は先頭）、縮尺付きのレイヤグループ・部分図にビューを描きます。
出力の形式と版はテンプレートのままです。

markup carry は <before> に付けたマークアップを新版 <after> に移します（決定 0021）。
項目のあるビューごとに全レイヤで比較し、版の移動量で項目を移動し、対応する図形の id に付け替え、
carried に unchanged・changed・removed を記録します。--view-map は DXF のレイアウトから PDF のページへの
移行など、ビューの対応を指定します。--ignore-hash を指定しなければ drawing.sha256 が <before> と一致する
必要があります。文書を -o（省略時は標準出力）に、状態ごとの項目数を JSON で出力します。";

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
    let (key, value) =
        text.split_once('=').ok_or_else(|| format!("{name} は <名前>=<値> で指定してください: {text}"))?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_region(text: &str) -> Result<BBox, String> {
    let values: Vec<f64> = text
        .split(',')
        .map(|value| value.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| format!("--region は 4 つの数値 <x0>,<y0>,<x1>,<y1> で指定してください: {text}"))?;
    let [x0, y0, x1, y1] = values[..] else {
        return Err(format!("--region は 4 つの数値 <x0>,<y0>,<x1>,<y1> で指定してください: {text}"));
    };
    if ![x0, y0, x1, y1].iter().all(|value| value.is_finite()) {
        return Err(format!("--region には有限の数値を指定してください: {text}"));
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
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} の値を指定してください"));
        match arg.as_str() {
            "--format" => parsed.format = Some(value("--format")?),
            "--view" => parsed.view = Some(value("--view")?),
            "--pretty" => parsed.pretty = true,
            "-o" | "--output" => parsed.output = Some(value("--output")?),
            "--dpi" => parsed.dpi = value("--dpi")?.parse().map_err(|_| "--dpi には数値を指定してください")?,
            "--theme" => {
                parsed.theme = Some(match value("--theme")?.as_str() {
                    "light" => Background::Light,
                    "dark" => Background::Dark,
                    other => return Err(format!("未対応のテーマです: {other}")),
                })
            }
            "--thin" => parsed.thin = true,
            "--size" => {
                let text = value("--size")?;
                let (w, h) = text.split_once('x').ok_or("--size は <幅>x<高さ> で指定してください")?;
                parsed.size = Some((
                    w.parse().map_err(|_| "--size の値が正しくありません")?,
                    h.parse().map_err(|_| "--size の値が正しくありません")?,
                ));
            }
            "--margin" => {
                parsed.margin = value("--margin")?.parse().map_err(|_| "--margin には数値を指定してください")?
            }
            "--dpr" => parsed.dpr = value("--dpr")?.parse().map_err(|_| "--dpr には数値を指定してください")?,
            "--region" => parsed.region = Some(parse_region(&value("--region")?)?),
            "--all-layers" => parsed.all_layers = true,
            "--geometry" => parsed.geometry = true,
            "--map" => parsed.map = Some(value("--map")?),
            "--tolerance" => {
                parsed.tolerance =
                    value("--tolerance")?.parse().map_err(|_| "--tolerance には数値を指定してください")?
            }
            "--layer" => parsed.layers.push(value("--layer")?),
            "--min-area" => {
                parsed.min_area = value("--min-area")?.parse().map_err(|_| "--min-area には数値を指定してください")?
            }
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
                parsed.limits.max_bytes =
                    Some(value("--max-bytes")?.parse().map_err(|_| "--max-bytes には件数を指定してください")?)
            }
            "--max-entities" => {
                parsed.limits.max_entities =
                    Some(value("--max-entities")?.parse().map_err(|_| "--max-entities には件数を指定してください")?)
            }
            "--timeout" => {
                let seconds: f64 = value("--timeout")?.parse().map_err(|_| "--timeout には秒数を指定してください")?;
                let duration = std::time::Duration::try_from_secs_f64(seconds)
                    .map_err(|_| "--timeout には秒数を指定してください")?;
                parsed.limits.max_duration = Some(duration);
            }
            _ if arg.starts_with('-') => return Err(format!("未対応のオプションです: {arg}\n\n{USAGE}")),
            _ if parsed.command == "markup" => parsed.positionals.push(arg),
            _ if parsed.file.is_empty() => parsed.file = arg,
            _ if matches!(parsed.command.as_str(), "compare" | "edit") && parsed.other.is_none() => {
                parsed.other = Some(arg)
            }
            _ => return Err(format!("想定していない引数です: {arg}")),
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
    let other = args.other.as_ref().ok_or("compare には 2 つのファイル <変更前> <変更後> を指定してください")?;
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
        Some(index) => format!("{} (操作 {index}, {})", error.message, error.code),
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
        let output = args
            .output
            .as_deref()
            .ok_or("draft には -o <出力.dxf|出力.dwg|出力.jww|出力.sfc|...> を指定してください")?;
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
                return Err(format!(
                    "{output}: --into の出力はテンプレートと同じ {} 形式にしてください",
                    format.to_uppercase()
                ));
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
        let output = args
            .output
            .as_deref()
            .ok_or("create には -o <出力.dxf|出力.dwg|出力.jww|出力.sfc|...> を指定してください")?;
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
        let format = zumen::detect(&bytes).ok_or("対応していない形式です")?;
        println!("{format}");
        return Ok(());
    }
    if args.command == "edit" {
        let output = args.output.as_deref().ok_or("edit には -o <出力先> を指定してください")?;
        let path =
            args.other.as_deref().ok_or("edit には図面と操作のファイルを <図面> <操作.json> の順で指定してください")?;
        let options = ReadDrawingOptions { format: args.format.clone(), view: None, limits: args.limits };
        let edited = zumen::edit(&bytes, &operations(path)?, &options).map_err(edit_error)?;
        return write_edited(&edited, output);
    }
    if args.command == "convert" {
        let output =
            args.output.ok_or("convert には -o <出力.dxf|出力.dwg|出力.jww|出力.sfc|...> を指定してください")?;
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
                return Err(format!("error の規則に違反する値が {errors} 件あります"));
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
            let output = args.output.ok_or("render には -o <出力.png> を指定してください")?;
            let markup = match &args.markup {
                Some(path) => {
                    let document = read_markup(path)?;
                    if document.drawing.sha256 != drawing_hash(&bytes) {
                        eprintln!(
                            "警告: {path} は別の版（SHA-256 {}）に付けたマークアップです。指定位置に描画します",
                            document.drawing.sha256
                        );
                    }
                    let items = document.items_in_view(scene.view.as_deref()).cloned().collect();
                    let style = MarkupStyle { categories: args.markup_colors.iter().cloned().collect(), color: None };
                    Some(Arc::new(RenderMarkup { items, style }))
                }
                None => None,
            };
            // PDF の図面のページ（file のない underlay）は、図面のバイト列から描く
            let underlays = if scene.underlays.iter().any(|underlay| underlay.file.is_none()) {
                UnderlayContent::drawing(bytes)
            } else {
                UnderlayContent::default()
            };
            let options = RenderOptions {
                theme: args.theme,
                line_weights: !args.thin,
                device_pixel_ratio: args.dpr,
                hidden_layers: filter.hidden_layers.clone(),
                markup,
                underlays,
                underlay_dpi: args.dpi,
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
        other => return Err(format!("未対応のコマンドです: {other}\n\n{USAGE}")),
    }
    print_notices(&scene.notices);
    Ok(())
}

/// 描けなかった文字と内容（underlay など）
fn print_report(report: &zumen_render::RenderReport) {
    if !report.missing_glyphs.is_empty() {
        let chars: String = report.missing_glyphs.iter().collect();
        eprintln!("警告: 同梱フォントに次の文字の字形がありません: {chars}");
    }
    print_notices(&report.notices);
}

fn print_notices(notices: &[zumen::Notice]) {
    for notice in notices {
        eprintln!("注意 ({:?}, {}): {} × {}", notice.level, notice.code, notice.message, notice.count);
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
