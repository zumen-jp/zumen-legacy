//! DXF への書き出し: tests/fixtures の DXF・DWG を DXF に書き直して読み直すと、すべての表示で元と同じに描ける。
//! 比べるのは図形ごとの種類・レイヤの名前・外接矩形・描く要素（数は 1e-9 の相対誤差まで同じとみなす）。
//! 書けない図形（IMAGE）は省き、形を変えて書く図形（HELIX・MPOLYGON・ブロック参照に属さない ATTRIB）は
//! 種類を比べない。どちらも notices で知らせる。

use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use zumen::{ReadDrawingOptions, Scene, UnderlayKind};

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

/// 元の図形ごとに、書き直した図面の同じ図形（同じ id、なければ同じ順の図形）と比べる。version は書いた版（None は既定の
/// 2013）。版にない値は notices で知らせて書かないので、比べない: 2004 より前の色（トゥルーカラー・透過度）、2007 より前の
/// 文字の背景の塗り（複数行の文字と、寸法などのブロックの中の複数行の文字）、2004 より前の円弧の長さ・大きい半径の寸法
/// （寸法のブロックの参照として、その中の図形が描かれる）
fn differences_with(original: &Scene, again: &Scene, version: Option<&str>) -> Vec<String> {
    let before_2004 = matches!(version, Some("2000"));
    let before_2007 = matches!(version, Some("2000" | "2004"));
    let colors = !before_2004;
    let entities = |scene: &Scene| serde_json::to_value(&scene.entities).unwrap().as_array().unwrap().clone();
    let (before, after) = (entities(original), entities(again));
    let mut out = Vec::new();
    // 画像の中身（underlay）のファイル・大きさ・置き方・切り抜き・不透明度。2010 より前の版は切り抜きの反転を持たない
    // （画像の全体と境界の 2 つの輪が、境界だけになる）。PDF の underlay は書かないので比べない（notices で知らせる）
    let before_2010 = matches!(version, Some("2000" | "2004" | "2007"));
    let underlays = |scene: &Scene, written: bool| {
        let values: Vec<Value> = scene
            .underlays
            .iter()
            .filter(|underlay| underlay.kind != UnderlayKind::Pdf)
            .map(|underlay| {
                let clip = match &underlay.clip {
                    Some(rings) if before_2010 && !written && rings.len() == 2 => Some(rings[1..].to_vec()),
                    clip => clip.clone(),
                };
                json!({
                    "file": underlay.file,
                    "size": [underlay.width, underlay.height],
                    "matrix": underlay.matrix,
                    "clip": clip,
                    "opacity": underlay.opacity,
                })
            })
            .collect();
        Value::Array(values)
    };
    let (images, images_again) = (underlays(original, false), underlays(again, true));
    if !close(&images, &images_again) {
        out.push(format!("underlays differ\n  {images}\n  {images_again}"));
    }
    for (index, entity) in before.iter().enumerate() {
        let id = entity["id"].as_str().unwrap();
        let source = entity["source"].as_str().unwrap_or("");
        let same_id = after.iter().find(|other| other["id"] == entity["id"]);
        // 画像のファイル（IMAGEDEF）が分からない画像は書かない
        let file = entity["properties"].as_array().is_some_and(|items| items.iter().any(|item| item["key"] == "file"));
        if source == "IMAGE" && !file {
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
        // 読めないハンドル（ない、16 進でない、重なる）は書き直すときに振り直す
        let Some(other) = same_id.or_else(|| after.get(index)) else {
            out.push(format!("{id}: missing"));
            continue;
        };
        let (mut entity, mut other) = (entity.clone(), other.clone());
        if before_2007 {
            without_backgrounds(&mut entity, &mut other);
        }
        let changed = matches!(source, "HELIX" | "MPOLYGON" | "ATTRIB");
        if !close(&shape_with(original, &entity, changed, colors), &shape_with(again, &other, changed, colors)) {
            out.push(format!("{id} ({}): differs", entity["type"]));
        }
    }
    out
}

/// 2007 より前の版は複数行の文字の背景を書かない。元の描く要素から、文字のすぐ前の塗り（ビューポート越しには切り取られた
/// 四角）のうち書き直した図形にないものを除き、背景を含む外接矩形は比べない
fn without_backgrounds(entity: &mut Value, other: &mut Value) {
    let before = entity["primitives"].as_array().cloned().unwrap_or_default();
    let after = other["primitives"].as_array().cloned().unwrap_or_default();
    let mut kept = Vec::new();
    for (index, primitive) in before.iter().enumerate() {
        let background =
            primitive["kind"] == "fill" && before.get(index + 1).is_some_and(|next| next["kind"] == "text");
        if background && after.get(kept.len()).is_none_or(|next| next["kind"] != "fill") {
            continue;
        }
        kept.push(primitive.clone());
    }
    if kept.len() < before.len() {
        entity["primitives"] = Value::Array(kept);
        entity["bbox"] = Value::Null;
        other["bbox"] = Value::Null;
    }
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
    for (version, code) in
        [("R12", "AC1009"), ("2000", "AC1015"), ("R2004", "AC1018"), ("AC1021", "AC1021"), ("2018", "AC1032")]
    {
        let write = zumen::WriteOptions { version: Some(version.into()) };
        let converted = zumen::convert_with(&bytes, "dxf", &options, &write).unwrap();
        let text = String::from_utf8_lossy(&converted.bytes).into_owned();
        assert!(text.contains(&format!("$ACADVER\r\n  1\r\n{code}")), "{version}");
        let old = matches!(code, "AC1009" | "AC1015" | "AC1018");
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
        if code == "AC1009" {
            // R12 は複数行の文字を行ごとの 1 行の文字にする
            let joined = |scene: &Scene| texts(scene).concat().replace('\n', "");
            assert_eq!(joined(&original), joined(&again), "{version}");
        } else {
            assert_eq!(texts(&original), texts(&again), "{version}");
        }
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
    let error = zumen::convert(&dxf, "jwc", &ReadDrawingOptions::default()).unwrap_err();
    assert!(error.message().contains("JWC"), "{}", error.message());
    let write = zumen::WriteOptions { version: Some("2013".into()) };
    let error = zumen::convert_with(&dxf, "dwg", &ReadDrawingOptions::default(), &write).unwrap_err();
    assert!(error.message().contains("DWG"), "{}", error.message());
    let write = zumen::WriteOptions { version: Some("4.0".into()) };
    let error = zumen::convert_with(&dxf, "sfc", &ReadDrawingOptions::default(), &write).unwrap_err();
    assert!(error.message().contains("4.0"), "{}", error.message());
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

/// どの形式の図面も、書き出せるどの形式にも書け、書いた図面はその形式として読める
#[test]
fn converts_every_format_to_every_writable_format() {
    let paths = ["dxf/features.dxf", "dwg/r2018.dwg", "jww/solids-v700.jww", "sfc/features.sfc", "p21/features.p21"];
    for path in paths {
        let bytes = fs::read(fixtures().join(path)).unwrap();
        let original = zumen::read(&bytes, &ReadDrawingOptions::default()).unwrap();
        for to in zumen::WRITABLE_FORMATS {
            let converted = zumen::convert(&bytes, to, &ReadDrawingOptions::default())
                .unwrap_or_else(|error| panic!("{path} → {to}: {error}"));
            assert_eq!(zumen::detect(&converted.bytes), Some(to), "{path} → {to}");
            let again = zumen::read(&converted.bytes, &ReadDrawingOptions::default())
                .unwrap_or_else(|error| panic!("{path} → {to}, read again: {error}"));
            assert!(!again.entities.is_empty() || original.entities.is_empty(), "{path} → {to}: no entities");
        }
    }
}

#[test]
fn writes_the_sxf_version_asked_for() {
    let bytes = fs::read(fixtures().join("dxf/features.dxf")).unwrap();
    for version in ["2.0", "3.0", "3.1"] {
        let write = zumen::WriteOptions { version: Some(version.into()) };
        let converted = zumen::convert_with(&bytes, "sfc", &ReadDrawingOptions::default(), &write).unwrap();
        // 出力ライブラリ名の末尾が SXF の版
        let text = String::from_utf8_lossy(&converted.bytes);
        assert!(text.contains(&format!("zumen$${version}")), "{version}");
    }
}
