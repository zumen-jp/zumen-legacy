//! その場の編集の適合テスト（docs/decisions/0010）。編集できるすべての形式（`zumen::EDITABLE_FORMATS`）の fixtures で、
//!
//! - 操作なしの編集は、元と同じバイト列になる
//! - 足した図形は読み直した図面に足した id で現れ、変えた図形は変えた値になり、消した図形はなくなる
//! - ほかの図形は、読み直しても id・種類・属性・外接矩形・描く要素がすべて同じ
//!
//! を確かめる。形式ごとの fixtures のフォルダーは `FOLDERS`。

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use zumen::edit::{EditOperation, parse_operations};
use zumen::extract::{Shape, entity_shapes, real_shapes};
use zumen::{ReadDrawingOptions, Scene};

/// 形式と、その fixtures のフォルダー・拡張子
const FOLDERS: [(&str, &str, &[&str]); 1] = [("dxf", "dxf", &["dxf"])];

/// 消して確かめる図形の種類（中にほかの図形を持たないもの）
const SIMPLE: [&str; 7] = ["line", "circle", "arc", "polyline", "point", "text", "ellipse"];

/// 足すレイヤ
const LAYER: &str = "ZUMEN-EDIT-CHECK";

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn drawings(folder: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase().as_str()))
        })
        .collect();
    files.sort();
    files
}

/// 図面のモデル空間（DXF・DWG。ほかの形式は既定の表示）を読む。足した図形はモデル空間に現れる
fn read(bytes: &[u8]) -> Scene {
    let scene = zumen::read(bytes, &ReadDrawingOptions::default()).expect("read");
    if scene.view.as_deref().is_some_and(|view| view != "Model")
        && scene.views.as_ref().is_some_and(|views| views.iter().any(|view| view.id == "Model"))
    {
        let options = ReadDrawingOptions { view: Some("Model".into()), ..ReadDrawingOptions::default() };
        return zumen::read(bytes, &options).expect("read the model space");
    }
    scene
}

/// 形を 1e-6 に丸めた JSON（図面自体の座標の形は、用紙への配置を戻す計算の誤差を持つ）
fn shapes_json(shapes: &[Shape]) -> Value {
    let round = |value: &f64| (value * 1e6).round() / 1e6;
    let points = |points: &[f64]| points.iter().map(round).collect::<Vec<_>>();
    Value::Array(
        shapes
            .iter()
            .map(|shape| match shape {
                Shape::Polyline { points: p, closed } => json!(["polyline", points(p), closed]),
                Shape::Segments { points: p } => json!(["segments", points(p)]),
                Shape::Fill { rings } => json!(["fill", rings.iter().map(|ring| points(ring)).collect::<Vec<_>>()]),
                Shape::Hatch { rings } => json!(["hatch", rings.iter().map(|ring| points(ring)).collect::<Vec<_>>()]),
                Shape::Point { x, y } => json!(["point", round(x), round(y)]),
                Shape::Text { text, x, y, height, angle } => {
                    json!(["text", text, round(x), round(y), round(height), round(angle)])
                }
            })
            .collect(),
    )
}

/// 図形の id（同じ id が続くときは何番目か）→ 比べる値（種類・属性と、図面自体の座標の形）。用紙の上の位置は
/// 図面の範囲で決まり、範囲の端の図形を足したり消したりすると全体が動くので、配置を戻した座標で比べる。
/// 無限の線（construction-line）は図面の範囲で描く長さが変わるので、属性だけを比べる
fn entities(scene: &Scene) -> BTreeMap<String, Value> {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    scene
        .entities
        .iter()
        .map(|entity| {
            let count = seen.entry(entity.id.as_str()).or_insert(0);
            *count += 1;
            let key = if *count == 1 { entity.id.clone() } else { format!("{}#{count}", entity.id) };
            let mut value = json!({ "kind": entity.kind, "source": entity.source, "properties": entity.properties });
            if entity.kind.name() != "construction-line" {
                let shapes = real_shapes(entity).unwrap_or_else(|| entity_shapes(entity, None));
                value["shapes"] = shapes_json(&shapes);
            }
            (key, value)
        })
        .collect()
}

/// 選べる図形（ブロック参照やビューポートの中ではなく、ハンドルのある図形）
fn top_level(scene: &Scene) -> Vec<&zumen::SceneEntity> {
    scene
        .entities
        .iter()
        .filter(|entity| !entity.id.contains(['>', ':']) && entity.id != "?" && !entity.id.is_empty())
        .collect()
}

fn layer_of(scene: &Scene, id: &str) -> Option<String> {
    let entity = scene.entities.iter().find(|entity| entity.id == id)?;
    entity.properties.iter().find(|property| property.key == "layer").map(|property| property.value.clone())
}

/// 1 つの図面を編集して確かめる。違いを文で返す
fn check(name: &str, bytes: &[u8]) -> Vec<String> {
    let mut problems = Vec::new();
    let options = ReadDrawingOptions::default();
    let same = zumen::edit(bytes, &[], &options).expect("edit without operations");
    if same.bytes != bytes {
        problems.push(format!("{name}: editing without operations changed the bytes"));
    }
    let before = read(bytes);
    let candidates = top_level(&before);
    let updated = candidates.first().map(|entity| entity.id.clone());
    let deleted =
        candidates.iter().skip(1).find(|entity| SIMPLE.contains(&entity.kind.name())).map(|entity| entity.id.clone());
    let mut operations = vec![
        json!({"op": "add-layer", "name": LAYER, "color": 1}),
        json!({"op": "add", "kind": "line", "layer": LAYER, "start": [0, 0], "end": [1000, 500]}),
        json!({"op": "add", "kind": "text", "layer": LAYER, "position": [0, -100], "text": "編集の確認", "height": 25}),
    ];
    if let Some(id) = &updated {
        operations.push(json!({"op": "update", "id": id, "set": {"layer": LAYER}}));
    }
    if let Some(id) = &deleted {
        operations.push(json!({"op": "delete", "id": id}));
    }
    let operations: Vec<EditOperation> = parse_operations(&Value::Array(operations).to_string()).unwrap();
    let edited = match zumen::edit(bytes, &operations, &options) {
        Ok(edited) => edited,
        Err(error) => return vec![format!("{name}: {error}")],
    };
    let after = read(&edited.bytes);
    let added: Vec<&String> = edited.created.iter().skip(1).collect();
    let (before_entities, after_entities) = (entities(&before), entities(&after));
    for id in added.iter().filter(|id| !id.is_empty()) {
        if !after_entities.contains_key(id.as_str()) {
            problems.push(format!("{name}: added {id} is not in the drawing read again"));
        }
    }
    if let Some(id) = &deleted
        && after_entities.contains_key(id)
    {
        problems.push(format!("{name}: deleted {id} is still there"));
    }
    if let Some(id) = &updated
        && layer_of(&after, id).as_deref() != Some(LAYER)
    {
        problems.push(format!("{name}: the layer of {id} is {:?}", layer_of(&after, id)));
    }
    for (id, entity) in &before_entities {
        if Some(id) == updated.as_ref() || Some(id) == deleted.as_ref() {
            continue;
        }
        match after_entities.get(id) {
            None => problems.push(format!("{name}: {id} disappeared")),
            // 足したレイヤで、レイヤの id が変わることがある
            Some(other) if strip_layer_ids(other) != strip_layer_ids(entity) => {
                problems.push(format!("{name}: {id} changed"));
            }
            _ => {}
        }
    }
    let count =
        before_entities.len() + added.iter().filter(|id| !id.is_empty()).count() - usize::from(deleted.is_some());
    if added.iter().all(|id| !id.is_empty()) && after_entities.len() != count {
        problems.push(format!("{name}: {} entities after the edit, expected {count}", after_entities.len()));
    }
    // 読み直しても、元にない読み取りの注意が増えない（足した文字の文字スタイルのフォントの注意は除く）
    for notice in &after.notices {
        if notice.code != "shx-font" && !before.notices.iter().any(|old| old.code == notice.code) {
            problems.push(format!("{name}: new notice {} ({})", notice.code, notice.message));
        }
    }
    problems
}

/// レイヤの番号（layer の raw）を除いた値
fn strip_layer_ids(entity: &Value) -> Value {
    let mut entity = entity.clone();
    if let Some(Value::Array(properties)) = entity.get_mut("properties") {
        for property in properties.iter_mut() {
            if property.get("key") == Some(&json!("layer"))
                && let Some(property) = property.as_object_mut()
            {
                property.remove("raw");
            }
        }
    }
    entity
}

#[test]
fn edits_change_only_what_they_name() {
    let mut checked = 0;
    let mut problems = Vec::new();
    for (format, folder, extensions) in FOLDERS {
        assert!(zumen::EDITABLE_FORMATS.contains(&format));
        for file in drawings(&fixtures().join(folder), extensions) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            problems.extend(check(&name, &fs::read(&file).unwrap()));
            checked += 1;
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(checked >= 20, "checked only {checked} drawings");
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダー）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test edit -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn edits_of_samples_change_only_what_they_name() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut problems = Vec::new();
    for (_, _, extensions) in FOLDERS {
        for file in drawings(Path::new(&folder), extensions) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let found = check(&name, &fs::read(&file).unwrap());
            println!("{name}: {} problems", found.len());
            problems.extend(found.into_iter().take(5));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn creates_new_drawings_from_operations() {
    let operations = parse_operations(
        r#"[
            {"op": "add-layer", "name": "外構", "color": 3},
            {"op": "add", "kind": "polyline", "layer": "外構", "vertices": [[0, 0], [10000, 0], [10000, 8000], [0, 8000]], "closed": true},
            {"op": "add", "kind": "text", "layer": "外構", "position": [500, 500], "text": "敷地", "height": 350}
        ]"#,
    )
    .unwrap();
    let created = zumen::create("dxf", "mm", &operations).unwrap();
    let scene = read(&created.bytes);
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert_eq!(ids, &created.created[1..]);
    assert!(scene.metadata.iter().any(|property| property.key == "units" && property.value.contains("mm")));
    let error = zumen::create("dxf", "furlong", &[]).unwrap_err();
    assert_eq!(error.code, "invalid-value");
}

#[test]
fn refuses_formats_it_cannot_edit_yet() {
    for (path, format) in [("sfc/sample.sfc", "SFC"), ("jww/minimal-v700.jww", "JWW"), ("dwg/r2018.dwg", "DWG")] {
        let bytes = fs::read(fixtures().join(path)).unwrap();
        if zumen::EDITABLE_FORMATS.contains(&format.to_ascii_lowercase().as_str()) {
            continue;
        }
        let error = zumen::edit(&bytes, &[], &ReadDrawingOptions::default()).unwrap_err();
        assert_eq!(error.code, "unsupported-format");
        assert!(error.message.contains(format), "{}", error.message);
    }
    let error = zumen::create("jww", "mm", &[]).unwrap_err();
    assert_eq!(error.code, "unsupported-format");
}
