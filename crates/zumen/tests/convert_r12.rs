//! AutoCAD R12 の DXF への書き出し: tests/fixtures の DXF・DWG を R12 に書いて読み直すと、モデル空間と現在の
//! レイアウト（*Paper_Space）が元と同じに描ける。R12 にない図形は R12 の図形に崩す（zumen_dxf の r12.rs）ので、
//! 元の図形ごとに、同じ id の図形か、崩した部品の図形（id が「元の id>」で始まる、無名のブロックの中の図形）と、
//! 描く要素の種類ごとに比べる。ハンドルのない図形（R12 の図面）・使えないハンドルの図形は、書くときにハンドルを振り直す
//! ので、描く順で対応させる:
//! - 線・文字・点は、同じ順に同じ座標（1e-9 の相対誤差まで）。楕円・スプラインは、折線の細かさがビューポートの縮尺で
//!   変わるので、線の外接矩形で比べる
//! - 塗りは、横の帯ごとの台形（SOLID）に分けるので、偶奇規則の面積と外接矩形で比べる。画面の背景色の塗り（背景の
//!   マスク）は書かない
//! - ハッチングの模様（境界で切り取って描く線）は、境界で切った線分にするので、線分が元の外接矩形の中にあること
//! - 複数行の属性は 1 行に、縦書きのフォント（@ で始まる TrueType の名前）の文字は横書きになる（R12 の文字スタイルは
//!   フォントの名前を持たない）ので、文字列だけを比べる
//!
//! 色（R12 は ACI だけ）・線の太さ（R12 にない）・破線（図形ごとの線種の尺度は R12 にない）は比べない。
//! R12 は長さの単位を持たないので、読み直す前に元の $INSUNITS を足して同じ単位で比べる。紙空間は R12 が持つ 1 つの
//! レイアウト（Layout1）と比べ、用紙の単位がインチのレイアウトは用紙の幅の比で縮尺を合わせる。

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use zumen::{ReadDrawingOptions, Scene};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn drawings(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "dxf" | "dwg"))
        })
        .collect();
    files.sort();
    files
}

/// R12 にあり、そのまま書く図形
const NATIVE: [&str; 13] = [
    "LINE",
    "CIRCLE",
    "ARC",
    "POINT",
    "TEXT",
    "ATTRIB",
    "ATTDEF",
    "POLYLINE",
    "SOLID",
    "TRACE",
    "3DFACE",
    "DIMENSION",
    "SHAPE",
];

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn numbers(value: &Value) -> Vec<f64> {
    match value {
        Value::Number(number) => vec![number.as_f64().unwrap()],
        Value::Array(items) => items.iter().flat_map(numbers).collect(),
        _ => Vec::new(),
    }
}

fn all_close(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| close(*a, *b))
}

/// 外接矩形（min_x, min_y, max_x, max_y）
type Bounds = [f64; 4];

fn bounds_of(points: &[f64], bounds: &mut Option<Bounds>) {
    for pair in points.as_chunks::<2>().0 {
        let b = bounds.get_or_insert([pair[0], pair[1], pair[0], pair[1]]);
        b[0] = b[0].min(pair[0]);
        b[1] = b[1].min(pair[1]);
        b[2] = b[2].max(pair[0]);
        b[3] = b[3].max(pair[1]);
    }
}

/// 外接矩形が、大きさの tolerance 倍まで同じか
fn bounds_close(a: Option<Bounds>, b: Option<Bounds>, tolerance: f64) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let size = (a[2] - a[0]).hypot(a[3] - a[1]).max(1e-9);
            a.iter().zip(&b).all(|(a, b)| (a - b).abs() <= size * tolerance + 1e-9 * a.abs().max(1.0))
        }
        _ => false,
    }
}

/// 偶奇規則の面積。頂点と辺の交点の高さで区切った横の帯ごとに、帯の中ほどで塗る区間の長さを足す
/// （R12 の書き出しの台形とは別の数え方で、交わる輪・重なる輪も正しく数える）
fn even_odd_area(rings: &[Vec<f64>]) -> f64 {
    let mut edges: Vec<[f64; 4]> = Vec::new();
    let mut heights: Vec<f64> = Vec::new();
    for ring in rings {
        let count = ring.len() / 2;
        for index in 0..count {
            let next = (index + 1) % count;
            let edge = [ring[index * 2], ring[index * 2 + 1], ring[next * 2], ring[next * 2 + 1]];
            heights.push(edge[1]);
            if edge[1] != edge[3] {
                edges.push(edge);
            }
        }
    }
    for (index, a) in edges.iter().enumerate() {
        for b in &edges[index + 1..] {
            let d = (a[0] - a[2]) * (b[1] - b[3]) - (a[1] - a[3]) * (b[0] - b[2]);
            if d == 0.0 {
                continue;
            }
            let t = ((a[0] - b[0]) * (b[1] - b[3]) - (a[1] - b[1]) * (b[0] - b[2])) / d;
            let u = -((a[0] - a[2]) * (a[1] - b[1]) - (a[1] - a[3]) * (a[0] - b[0])) / d;
            if t > 0.0 && t < 1.0 && u > 0.0 && u < 1.0 {
                heights.push(a[1] + t * (a[3] - a[1]));
            }
        }
    }
    heights.sort_by(f64::total_cmp);
    heights.dedup();
    let mut total = 0.0;
    for band in heights.windows(2) {
        let middle = (band[0] + band[1]) / 2.0;
        let mut xs: Vec<f64> = edges
            .iter()
            .filter(|e| e[1].min(e[3]) <= middle && middle < e[1].max(e[3]))
            .map(|e| e[0] + (e[2] - e[0]) * (middle - e[1]) / (e[3] - e[1]))
            .collect();
        xs.sort_by(f64::total_cmp);
        total += xs.as_chunks::<2>().0.iter().map(|[left, right]| right - left).sum::<f64>() * (band[1] - band[0]);
    }
    total
}

/// 描く要素の種類ごとの並び。座標は scale 倍する（紙空間の単位を合わせる）
#[derive(Default)]
struct Shapes {
    strokes: Vec<(Vec<f64>, bool)>,
    texts: Vec<(String, Vec<f64>)>,
    dots: Vec<f64>,
    fills: Option<Bounds>,
    /// 塗りの面積（偶奇規則）の和
    area: f64,
    clipped: Option<Bounds>,
}

fn shapes(entities: &[&Value], scale: f64) -> Shapes {
    let mut out = Shapes::default();
    let scaled = |value: &Value| numbers(value).into_iter().map(|number| number * scale).collect::<Vec<f64>>();
    for entity in entities {
        for primitive in entity["primitives"].as_array().unwrap() {
            match primitive["kind"].as_str().unwrap() {
                "stroke" => {
                    // 2 点ずつの線分（segments）は、1 本ずつの線にする（R12 の LINE と同じに）
                    let points = scaled(&primitive["points"]);
                    if primitive["segments"] == Value::Bool(true) {
                        out.strokes.extend(points.as_chunks::<4>().0.iter().map(|segment| (segment.to_vec(), false)));
                    } else {
                        out.strokes.push((points, primitive["closed"].as_bool().unwrap()));
                    }
                }
                "text" => {
                    // 用紙の上の値で比べる（文字の高さを行列で拡大するか、高さそのものにするかによらない）:
                    // 高さ・字形の横の倍率・文字範囲の大きさ、文字の向き、字の中心の位置
                    let m = scaled(&primitive["matrix"]);
                    let (sx, sy) = (m[0].hypot(m[1]), m[2].hypot(m[3]));
                    let number = |key: &str| primitive[key].as_f64().unwrap();
                    let mut values = vec![
                        number("height") * sy,
                        number("scaleX") * sx / sy,
                        number("boxWidth") * sx,
                        number("boxHeight") * sy,
                        m[0] / sx,
                        m[1] / sx,
                        m[2] / sy,
                        m[3] / sy,
                    ];
                    for cell in primitive["cells"].as_array().unwrap() {
                        let (x, y) = (cell["x"].as_f64().unwrap(), cell["y"].as_f64().unwrap());
                        values.extend([m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]);
                    }
                    let text = primitive["text"].as_str().unwrap().to_string();
                    out.texts.push((text, values));
                }
                "dot" => out
                    .dots
                    .extend([primitive["x"].as_f64().unwrap(), primitive["y"].as_f64().unwrap()].map(|v| v * scale)),
                "fill" => {
                    // 画面の背景色の塗り（背景のマスク）は R12 に書かない
                    if primitive["color"] == "background" {
                        continue;
                    }
                    let rings: Vec<Vec<f64>> = primitive["rings"].as_array().unwrap().iter().map(&scaled).collect();
                    out.area += even_odd_area(&rings);
                    for ring in &rings {
                        bounds_of(ring, &mut out.fills);
                    }
                }
                "clipped" => bounds_of(&scaled(&primitive["rings"]), &mut out.clipped),
                kind => panic!("{kind}"),
            }
        }
    }
    out
}

/// 元の図形と、R12 で描いた図形（同じ id の図形か、崩した部品）の違い。text_only なら文字は文字列だけを比べる
/// （1 行にした属性はつないだ文字列）
fn compare(entity: &Value, parts: &[&Value], scale: f64, text_only: bool) -> Option<String> {
    let source = entity["source"].as_str().unwrap_or("");
    let a = shapes(&[entity], 1.0);
    let b = shapes(parts, scale);
    let texts = |shapes: &Shapes| shapes.texts.iter().map(|text| text.0.clone()).collect::<Vec<_>>();
    let same_texts = if text_only {
        texts(&a).join(" ") == texts(&b).join(" ")
    } else {
        a.texts.len() == b.texts.len() && a.texts.iter().zip(&b.texts).all(|(x, y)| x.0 == y.0 && all_close(&x.1, &y.1))
    };
    if !same_texts {
        return Some(format!("texts {:?} → {:?}", texts(&a), texts(&b)));
    }
    if !all_close(&a.dots, &b.dots) {
        return Some(format!("dots {:?} → {:?}", a.dots, b.dots));
    }
    let size = a.fills.map_or(0.0, |b| (b[2] - b[0]) * (b[3] - b[1]));
    let inside = match (a.fills, b.fills) {
        (_, None) => true,
        (Some(x), Some(y)) => {
            bounds_close(Some([x[0].min(y[0]), x[1].min(y[1]), x[2].max(y[2]), x[3].max(y[3])]), Some(x), 1e-9)
        }
        (None, Some(_)) => false,
    };
    // 曲線の細かさは、ブロックの配置・ビューポートの縮尺で変わる。用紙の単位を R12 で mm とみなすレイアウト（scale が
    // 1 でない）は、ビューポートの縮尺が小さくなり、曲線が粗くなる
    let loose = if scale == 1.0 { 1.0 } else { 10.0 };
    if (a.area - b.area).abs() > 1e-2 * loose * a.area.abs().max(b.area.abs()) + 1e-9 * size.max(1.0) || !inside {
        return Some(format!("fills {} {:?} → {} {:?}", a.area, a.fills, b.area, b.fills));
    }
    if let Some(boundary) = a.clipped {
        // 模様の線分は、境界の外接矩形の中にある
        let mut lines = None;
        for (points, _) in &b.strokes {
            bounds_of(points, &mut lines);
        }
        let inside = lines.is_some_and(|lines| {
            let size = (boundary[2] - boundary[0]).hypot(boundary[3] - boundary[1]) * 1e-9 + 1e-9;
            lines[0] >= boundary[0] - size
                && lines[1] >= boundary[1] - size
                && lines[2] <= boundary[2] + size
                && lines[3] <= boundary[3] + size
        });
        return (!inside).then(|| format!("pattern lines {lines:?} outside {boundary:?}"));
    }
    // 楕円・スプラインは R12 で折線にする。用紙の単位を R12 で mm とみなすレイアウトでは、ビューポートの縮尺が変わり、
    // 曲線の細かさも変わる
    if matches!(source, "ELLIPSE" | "SPLINE" | "HELIX") || scale != 1.0 {
        let (mut x, mut y) = (None, None);
        for (points, _) in &a.strokes {
            bounds_of(points, &mut x);
        }
        for (points, _) in &b.strokes {
            bounds_of(points, &mut y);
        }
        return (!bounds_close(x, y, 1e-3 * loose * loose)).then(|| format!("curve {x:?} → {y:?}"));
    }
    if a.strokes.len() != b.strokes.len()
        || !a.strokes.iter().zip(&b.strokes).all(|(x, y)| x.1 == y.1 && all_close(&x.0, &y.0))
    {
        let counts = |shapes: &Shapes| shapes.strokes.iter().map(|stroke| stroke.0.len() / 2).collect::<Vec<_>>();
        return Some(format!("strokes {:?} → {:?}", counts(&a), counts(&b)));
    }
    None
}

fn layer_names(scene: &Scene, entity: &Value) -> Vec<String> {
    entity["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let id = id.as_u64().unwrap();
            scene
                .layers
                .iter()
                .find(|layer| u64::from(layer.id) == id)
                .map_or(String::new(), |layer| layer.name.clone())
        })
        .collect()
}

fn id_of(entity: &Value) -> &str {
    entity["id"].as_str().unwrap()
}

/// id の入れ子の深さ（ブロック参照の中は「>」でつなぐ）と、親の部分
fn depth(id: &str) -> usize {
    id.matches('>').count()
}

fn parent(id: &str) -> &str {
    id.rsplit_once('>').map_or("", |(head, _)| head)
}

/// 元の図形ごとに、R12 で描いた図形と比べる。scale は R12 の座標を元の座標にする倍率（紙空間の単位）。
/// lenient は文字列だけを比べる図形のハンドル（複数行の属性）
fn differences_r12(original: &Scene, again: &Scene, scale: f64, lenient: &HashSet<String>) -> Vec<String> {
    let entities = |scene: &Scene| serde_json::to_value(&scene.entities).unwrap().as_array().unwrap().clone();
    let (before, after) = (entities(original), entities(again));
    let index_of: HashMap<&str, usize> =
        after.iter().enumerate().map(|(index, entity)| (id_of(entity), index)).collect();
    let mut used = vec![false; after.len()];
    let mut out = Vec::new();
    // 次に対応させる R12 の図形（描く順）
    let mut next = 0;
    for entity in &before {
        let id = id_of(entity);
        let source = entity["source"].as_str().unwrap_or("");
        if matches!(source, "IMAGE" | "XLINE" | "RAY" | "WIPEOUT") {
            if index_of.contains_key(id) {
                out.push(format!("{id} ({source}): written"));
            }
            continue;
        }
        // 同じ id の図形、id が「id>」で始まる部品、なければ描く順の次の図形（同じ深さ）か部品（1 つ深い）
        let prefix = format!("{id}>");
        let group = |start: usize| {
            let first = id_of(&after[start]);
            after[start..]
                .iter()
                .take_while(|other| depth(id_of(other)) == depth(first) && parent(id_of(other)) == parent(first))
                .count()
        };
        let (start, count) = if let Some(&index) = index_of.get(id) {
            (index, 1)
        } else if let Some(index) = after.iter().position(|other| id_of(other).starts_with(&prefix)) {
            (index, group(index))
        } else if next < after.len() && depth(id_of(&after[next])) == depth(id) {
            (next, 1)
        } else if next < after.len() && depth(id_of(&after[next])) == depth(id) + 1 {
            (next, group(next))
        } else {
            out.push(format!("{id} ({source}): missing"));
            continue;
        };
        next = start + count;
        let parts: Vec<&Value> = after[start..start + count].iter().collect();
        used[start..start + count].iter_mut().for_each(|used| *used = true);
        if matches!(source, "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION") {
            // 寸法のブロックの参照として書く（中の図形が描かれる）
            continue;
        }
        // ブロック参照に属さない属性は文字として書く（ほかの版と同じ）
        if let [other] = parts.as_slice()
            && NATIVE.contains(&source)
            && !matches!(source, "ATTRIB" | "ATTDEF")
            && other["type"] != entity["type"]
        {
            out.push(format!("{id}: type {} → {}", entity["type"], other["type"]));
        }
        let layers = layer_names(original, entity);
        if let Some(part) = parts.iter().find(|part| layer_names(again, part) != layers) {
            out.push(format!("{id}: layers {layers:?} → {:?}", layer_names(again, part)));
        }
        // 縦書きのフォントの文字は横書きになる（R12 の文字スタイルはフォントの名前を持たない）
        let vertical = entity["properties"].as_array().unwrap().iter().any(|property| {
            property["key"] == "textStyle" && property["value"].as_str().is_some_and(|value| value.contains("（@"))
        });
        let handle = id.rsplit(['>', ':']).next().unwrap_or(id);
        if let Some(difference) = compare(entity, &parts, scale, vertical || lenient.contains(handle)) {
            out.push(format!("{id} ({source}): {difference}"));
        }
    }
    // R12 の図形は、どれも元の図形か、その部品。用紙の単位を R12 で mm とみなすレイアウト（scale が 1 でない）では、
    // ビューポートの切り抜きの境に乗る図形の内外が、座標の丸めで入れ替わることがあるので数えない
    if scale == 1.0 {
        for (entity, used) in after.iter().zip(&used) {
            if !used {
                out.push(format!("{}: added", id_of(entity)));
            }
        }
    }
    out
}

/// R12 は長さの単位を持たない。元の単位で読むため、$ACADVER の後に元の $INSUNITS（と $MEASUREMENT）を足す
fn with_units(bytes: &[u8], header: &zumen::dxf::DxfHeader) -> Vec<u8> {
    let marker = b"$ACADVER\r\n  1\r\nAC1009\r\n";
    let at = bytes.windows(marker.len()).position(|window| window == marker).expect("R12 の DXF") + marker.len();
    let mut units = format!("  9\r\n$INSUNITS\r\n 70\r\n{}\r\n", header.insunits);
    if let Some(measurement) = header.measurement {
        units += &format!("  9\r\n$MEASUREMENT\r\n 70\r\n{measurement}\r\n");
    }
    [&bytes[..at], units.as_bytes(), &bytes[at..]].concat()
}

/// R12 に書いて、モデル空間と現在のレイアウトを比べる。比べた表示の数と違いを返す（読めない図面は比べない）
fn check_r12(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let options = ReadDrawingOptions::default();
    if zumen::read(bytes, &options).is_err() {
        return (0, Vec::new());
    }
    let doc = match zumen::detect(bytes) {
        Some("dwg") => match zumen::dwg::read_dwg(bytes) {
            Ok(doc) => doc,
            Err(_) => return (0, Vec::new()),
        },
        _ => zumen::dxf::parse_dxf(bytes),
    };
    let write = zumen::WriteOptions { version: Some("R12".into()) };
    let converted = match zumen::convert_with(bytes, "dxf", &options, &write) {
        Ok(converted) => converted,
        Err(error) => return (0, vec![format!("{name}: {error}")]),
    };
    let r12 = with_units(&converted.bytes, &doc.header);
    // 複数行の属性（1 行にする）のハンドル
    let lenient: HashSet<String> = doc
        .entities
        .iter()
        .flat_map(|entity| match &entity.data {
            zumen::dxf::DxfEntityData::Insert(insert) => insert.attribs.iter().collect(),
            _ => vec![entity],
        })
        .filter(|entity| {
            matches!(
                &entity.data,
                zumen::dxf::DxfEntityData::Attrib { mtext: Some(_), .. }
                    | zumen::dxf::DxfEntityData::Attdef { mtext: Some(_), .. }
            )
        })
        .map(|entity| entity.base.handle.clone())
        .collect();
    let mut views = vec![("Model".to_string(), "Model")];
    if let Some(layout) =
        doc.layouts.iter().find(|layout| !layout.model && layout.block.eq_ignore_ascii_case("*Paper_Space"))
    {
        views.push((layout.name.clone(), "Layout1"));
    }
    let mut out = Vec::new();
    for (view, again_view) in &views {
        let read = |bytes: &[u8], view: &str| {
            zumen::read(bytes, &ReadDrawingOptions { view: Some(view.to_string()), ..ReadDrawingOptions::default() })
        };
        let original = read(bytes, view).unwrap();
        if original.entities.is_empty() {
            continue;
        }
        let again = match read(&r12, again_view) {
            Ok(again) => again,
            Err(error) => {
                out.push(format!("{name} ({view}): rereading: {error}"));
                continue;
            }
        };
        let scale = match (&original.sheet, &again.sheet) {
            (Some(a), Some(b)) if *view != "Model" && b.width > 0.0 => a.width / b.width,
            _ => 1.0,
        };
        out.extend(
            differences_r12(&original, &again, scale, &lenient)
                .into_iter()
                .map(|line| format!("{name} ({view}) {line}")),
        );
    }
    (views.len(), out)
}

#[test]
fn drawings_written_as_r12_draw_the_same() {
    let mut problems = Vec::new();
    let mut checked = 0;
    for format in ["dxf", "dwg"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (views, differences) = check_r12(&name, &fs::read(&file).unwrap());
            checked += views;
            problems.extend(differences);
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(checked >= 26, "checked only {checked} views");
}

/// Git に入れない図面で、R12 に書いても同じに描けることを確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test convert_r12 -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn samples_written_as_r12_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in drawings(Path::new(&folder)) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let (views, differences) = check_r12(&name, &fs::read(&file).unwrap());
        println!("{name}: {views} views, {} differences", differences.len());
        failures.extend(differences.into_iter().take(5));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// R12 に書けない・形を変えて書いた内容を notices で知らせる
#[test]
fn reports_what_r12_cannot_hold() {
    let codes = |name: &str| {
        let bytes = fs::read(fixtures().join("dxf").join(name)).unwrap();
        let write = zumen::WriteOptions { version: Some("R12".into()) };
        let converted = zumen::convert_with(&bytes, "dxf", &ReadDrawingOptions::default(), &write).unwrap();
        converted.notices.iter().map(|notice| notice.code.clone()).collect::<Vec<_>>()
    };
    let features = codes("features.dxf");
    for code in [
        "version-ellipse",
        "version-spline",
        "version-mtext",
        "version-hatch",
        "version-leader",
        "version-mline",
        "version-mask",
        "version-xline",
        "version-wipeout",
        "version-invisible",
        "version-lineweight",
        "version-linetype-scale",
        "version-color",
        "version-dimension",
    ] {
        assert!(features.contains(&code.to_string()), "{code} not in {features:?}");
    }
    for (name, code) in [
        ("viewports.dxf", "version-layouts"),
        ("r12.dxf", "version-units"),
        ("attributes-multiline.dxf", "version-multiline-attribute"),
        ("vertical-text.dxf", "version-font-family"),
    ] {
        let found = codes(name);
        assert!(found.contains(&code.to_string()), "{name}: {code} not in {found:?}");
    }
    // 単位が mm の図面は、R12 にしても単位を失わない（読み取りは単位のない図面を mm とみなす）
    assert!(!codes("features.dxf").contains(&"version-units".to_string()));
}
