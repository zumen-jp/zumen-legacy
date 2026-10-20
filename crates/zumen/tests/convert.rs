//! DXF への書き出し: tests/fixtures の DXF・DWG を DXF に書き直して読み直すと、すべての表示で元と同じに描ける。
//! 比べるのは図形ごとの種類・レイヤの名前・外接矩形・描く要素（数は 1e-9 の相対誤差まで同じとみなす）。
//! 書けない図形（IMAGE）は省き、形を変えて書く図形（HELIX・MPOLYGON・ブロック参照に属さない ATTRIB）は
//! 種類を比べない。どちらも notices で知らせる。

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use zumen::{ReadDrawingOptions, Scene};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| close(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(key, value)| y.get(key).is_some_and(|other| close(value, other)))
        }
        _ => a == b,
    }
}

/// 比べる値。形を変えて書く図形（changed）は種類を比べず、色を比べないとき（colors が false）は、描く要素の色（color）を除く
fn shape_with(scene: &Scene, entity: &Value, changed: bool, colors: bool) -> Value {
    let layers: Vec<String> = entity["layers"]
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
        .collect();
    let mut primitives = entity["primitives"].clone();
    if !colors {
        strip_colors(&mut primitives);
    }
    json!({
        "type": if changed { Value::Null } else { entity["type"].clone() },
        "layers": layers,
        "bbox": entity["bbox"],
        "primitives": primitives,
    })
}

fn strip_colors(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("color");
            for item in object.values_mut() {
                strip_colors(item);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_colors),
        _ => {}
    }
}

/// 元の図形ごとに、書き直した図面の同じ図形（同じ id、なければ同じ順の図形）と比べる
/// 元の図形ごとに、書き直した図面の同じ図形と比べる。version は書いた版（None は既定の 2013）。版にない値は notices で
/// 知らせて書かないので、比べない: 2004 より前の色（トゥルーカラー・透過度）、2007 より前の複数行の文字の背景の塗り、
/// 2004 より前の円弧の長さ・大きい半径の寸法（寸法のブロックの参照として、その中の図形が描かれる）
fn differences_with(original: &Scene, again: &Scene, version: Option<&str>) -> Vec<String> {
    let before_2004 = matches!(version, Some("2000"));
    let before_2007 = matches!(version, Some("2000" | "2004"));
    let colors = !before_2004;
    let entities = |scene: &Scene| serde_json::to_value(&scene.entities).unwrap().as_array().unwrap().clone();
    let (before, after) = (entities(original), entities(again));
    let mut out = Vec::new();
    for (index, entity) in before.iter().enumerate() {
        let id = entity["id"].as_str().unwrap();
        let source = entity["source"].as_str().unwrap_or("");
        let same_id = after.iter().find(|other| other["id"] == entity["id"]);
        if source == "IMAGE" {
            if same_id.is_some() {
                out.push(format!("{id}: an image was written"));
            }
            continue;
        }
        if before_2004 && matches!(source, "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION") {
            let prefix = format!("{id}>");
            if !after.iter().any(|other| other["id"].as_str().is_some_and(|other| other.starts_with(&prefix))) {
                out.push(format!("{id}: the block of the dimension is not drawn"));
            }
            continue;
        }
        let mut entity = entity.clone();
        let mut other_fills = None;
        if before_2007 && source == "MTEXT" {
            // 背景の塗り（fill）と、それを含む外接矩形を除いて比べる
            let without_fills = |value: &mut Value| {
                if let Some(primitives) = value["primitives"].as_array_mut() {
                    primitives.retain(|primitive| primitive["kind"] != "fill");
                }
                value["bbox"] = Value::Null;
            };
            without_fills(&mut entity);
            other_fills = Some(without_fills);
        }
        let entity = &entity;
        // 読めないハンドル（ない、16 進でない、重なる）は書き直すときに振り直す
        let changed = matches!(source, "HELIX" | "MPOLYGON" | "ATTRIB");
        let other = same_id.or_else(|| after.get(index)).map(|other| {
            let mut other = other.clone();
            if let Some(without_fills) = other_fills {
                without_fills(&mut other);
            }
            other
        });
        match other.as_ref() {
            None => out.push(format!("{id}: missing")),
            Some(other)
                if !close(
                    &shape_with(original, entity, changed, colors),
                    &shape_with(again, other, changed, colors),
                ) =>
            {
                out.push(format!("{id} ({}): differs", entity["type"]))
            }
            _ => {}
        }
    }
    out
}

/// 図面を DXF に書き直し、すべての表示で元と比べる。比べた表示の数と、違いを返す（読めない図面は比べない）
fn check(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    check_version(name, bytes, None)
}

/// 版を選んで書き直して比べる。2004 より前の版はトゥルーカラー・透過度を持たないので、色は比べない
fn check_version(name: &str, bytes: &[u8], version: Option<&str>) -> (usize, Vec<String>) {
    let Ok(original) = zumen::read(bytes, &ReadDrawingOptions::default()) else {
        return (0, Vec::new());
    };
    let write = zumen::WriteOptions { version: version.map(str::to_string) };
    let converted = match zumen::convert_with(bytes, "dxf", &ReadDrawingOptions::default(), &write) {
        Ok(converted) => converted,
        Err(error) => return (0, vec![format!("{name}: {error}")]),
    };
    if zumen::detect(&converted.bytes) != Some("dxf") {
        return (0, vec![format!("{name}: not DXF")]);
    }
    let views: Vec<Option<String>> =
        std::iter::once(None).chain(original.views.iter().flatten().map(|view| Some(view.id.clone()))).collect();
    let mut out = Vec::new();
    for view in &views {
        let options = ReadDrawingOptions { view: view.clone(), ..ReadDrawingOptions::default() };
        let original = zumen::read(bytes, &options).unwrap();
        match zumen::read(&converted.bytes, &options) {
            Ok(again) => out.extend(
                differences_with(&original, &again, version)
                    .into_iter()
                    .map(|line| format!("{name} {version:?} ({view:?}) {line}")),
            ),
            Err(error) => out.push(format!("{name} ({view:?}): rereading: {error}")),
        }
    }
    (views.len(), out)
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

#[test]
fn converted_drawings_draw_the_same() {
    let mut checked = 0;
    for format in ["dxf", "dwg"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (views, differences) = check(&name, &fs::read(&file).unwrap());
            assert!(differences.is_empty(), "{differences:?}");
            checked += views;
        }
    }
    assert!(checked >= 20, "checked only {checked} views");
}

/// AutoCAD 2000〜2018 のどの版に書いても、読み直すと元と同じに描ける（2000 は色を除く）
#[test]
fn drawings_written_in_every_version_draw_the_same() {
    let mut problems = Vec::new();
    for version in ["2000", "2004", "2007", "2010", "2018"] {
        for format in ["dxf", "dwg"] {
            for file in drawings(&fixtures().join(format)) {
                let name = file.file_name().unwrap().to_string_lossy().to_string();
                let (_, differences) = check_version(&name, &fs::read(&file).unwrap(), Some(version));
                problems.extend(differences);
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Git に入れない図面で、どの版に書いても同じに描けることを確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test convert -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn samples_written_in_every_version_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for version in ["2000", "2004", "2007", "2010", "2018"] {
        for file in drawings(Path::new(&folder)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (views, differences) = check_version(&name, &fs::read(&file).unwrap(), Some(version));
            println!("{version} {name}: {views} views, {} differences", differences.len());
            failures.extend(differences.into_iter().take(5));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn writes_the_version_asked_for_in_its_code_page() {
    let bytes = fs::read(fixtures().join("dxf/r12-shift-jis.dxf")).unwrap();
    let options = ReadDrawingOptions::default();
    for (version, code) in [("2000", "AC1015"), ("R2004", "AC1018"), ("AC1021", "AC1021"), ("2018", "AC1032")] {
        let write = zumen::WriteOptions { version: Some(version.into()) };
        let converted = zumen::convert_with(&bytes, "dxf", &options, &write).unwrap();
        let text = String::from_utf8_lossy(&converted.bytes).into_owned();
        assert!(text.contains(&format!("$ACADVER\r\n  1\r\n{code}")), "{version}");
        let old = matches!(code, "AC1015" | "AC1018");
        // 2007 より前は図面のコードページ（Shift_JIS）、それ以降は UTF-8
        assert_eq!(std::str::from_utf8(&converted.bytes).is_ok(), !old, "{version}");
        if old {
            assert!(text.contains("$DWGCODEPAGE\r\n  3\r\nANSI_932"), "{version}");
        }
        let original = zumen::read(&bytes, &options).unwrap();
        let again = zumen::read(&converted.bytes, &options).unwrap();
        let texts = |scene: &Scene| {
            zumen::extract::scene_texts(scene, &Default::default())
                .into_iter()
                .map(|(_, text)| text.text)
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&original), texts(&again), "{version}");
    }
    let error =
        zumen::convert_with(&bytes, "dxf", &options, &zumen::WriteOptions { version: Some("R13".into()) }).unwrap_err();
    assert!(error.message().contains("R13"), "{}", error.message());
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの DXF・DWG）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test convert -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn converted_samples_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in drawings(Path::new(&folder)) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let (views, differences) = check(&name, &fs::read(&file).unwrap());
        println!("{name}: {views} views, {} differences", differences.len());
        failures.extend(differences.into_iter().take(5));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn reports_what_could_not_be_written_as_it_was() {
    let bytes = fs::read(fixtures().join("dxf/features.dxf")).unwrap();
    let converted = zumen::convert(&bytes, "dxf", &ReadDrawingOptions::default()).unwrap();
    let codes: Vec<&str> = converted.notices.iter().map(|notice| notice.code.as_str()).collect();
    for code in ["image", "helix", "mpolygon", "table", "attrib-alone", "complex-linetype", "missing-block"] {
        assert!(codes.contains(&code), "{code} not in {codes:?}");
    }
    // 書き直した DXF をもう一度書き直すときは、知らせることがない（省いた図形も補った定義も、もう図面にある形）
    let again = zumen::convert(&converted.bytes, "dxf", &ReadDrawingOptions::default()).unwrap();
    assert_eq!(again.notices, []);
}

#[test]
fn refuses_formats_it_cannot_write() {
    let dxf = fs::read(fixtures().join("dxf/r12.dxf")).unwrap();
    let error = zumen::convert(&dxf, "p21", &ReadDrawingOptions::default()).unwrap_err();
    assert!(error.message().contains("P21"), "{}", error.message());
    let options = ReadDrawingOptions { format: Some("stb".into()), ..ReadDrawingOptions::default() };
    let error = zumen::convert(&dxf, "dxf", &options).unwrap_err();
    assert!(error.message().contains("STB"), "{}", error.message());
}

#[test]
fn converts_sxf_and_jww_with_the_notices_of_mapping_and_writing() {
    for path in ["sfc/features.sfc", "p21/features.p21", "jww/minimal-v700.jww"] {
        let bytes = fs::read(fixtures().join(path)).unwrap();
        let converted = zumen::convert(&bytes, "dxf", &ReadDrawingOptions::default()).unwrap();
        assert_eq!(zumen::detect(&converted.bytes), Some("dxf"), "{path}");
        let codes: Vec<&str> = converted.notices.iter().map(|notice| notice.code.as_str()).collect();
        if path.ends_with(".jww") {
            assert!(codes.contains(&"jw-settings"), "{codes:?}");
        } else {
            for code in ["sfig-insert", "sfig-cycle", "symbol", "leader", "hatch-pattern"] {
                assert!(codes.contains(&code), "{path}: {code} not in {codes:?}");
            }
        }
    }
}
