//! SXF のその場の編集（edit_sxf）: crates/zumen/tests/edit.rs の適合テストと同じ確かめ方を、SFC・P21 と圧縮した
//! SFZ・P2Z の fixtures で行い、SXF に固有の規則（部分図の配置を通した id、作図部品の中、container、値の写し方）を確かめる

mod common;

use common::read_fixture;
use serde_json::{Value, json};
use zumen_core::Scene;
use zumen_core::edit::{EditOperation, parse_operations};
use zumen_core::extract::{Shape, entity_shapes, real_shapes};
use zumen_sxf::{SxfFeature, edit_sxf, new_sxf, parse_sxf, sxf_to_scene, write_sxf};

const LAYER: &str = "ZUMEN-EDIT-CHECK";
const SIMPLE: [&str; 7] = ["line", "circle", "arc", "polyline", "point", "text", "ellipse"];

fn operations(json: Value) -> Vec<EditOperation> {
    parse_operations(&json.to_string()).unwrap()
}

fn read(bytes: &[u8], format: &str) -> Scene {
    sxf_to_scene(&parse_sxf(bytes, format).unwrap())
}

/// 形を 1e-6 に丸めた JSON
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

fn entities(scene: &Scene) -> Vec<(String, Value)> {
    scene
        .entities
        .iter()
        .map(|entity| {
            let properties: Vec<&zumen_core::Property> =
                entity.properties.iter().filter(|property| property.key != "id").collect();
            let mut value = json!({ "kind": entity.kind, "source": entity.source, "properties": properties });
            let shapes = real_shapes(entity).unwrap_or_else(|| entity_shapes(entity, None));
            value["shapes"] = shapes_json(&shapes);
            (entity.id.clone(), value)
        })
        .collect()
}

fn layer_value(entity: &Value) -> Option<&str> {
    entity["properties"].as_array()?.iter().find(|property| property["key"] == "layer")?["value"].as_str()
}

/// レイヤの番号（layer の raw）を除き、ほかの raw を 1e-6 に丸めた値。P21 の読み取りは 1e-6 以内の線幅を 1 つの
/// コードにまとめ、最初に出会った値を使うので、編集で出会う順が変わると raw がその差だけ変わる（表示は同じ）
fn strip_layer_ids(entity: &Value) -> Value {
    fn round(value: &mut Value) {
        match value {
            Value::Number(number) => {
                if let Some(rounded) =
                    number.as_f64().and_then(|x| serde_json::Number::from_f64((x * 1e6).round() / 1e6))
                {
                    *value = Value::Number(rounded);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(round),
            _ => {}
        }
    }
    let mut entity = entity.clone();
    if let Some(Value::Array(properties)) = entity.get_mut("properties") {
        for property in properties.iter_mut() {
            let Some(property) = property.as_object_mut() else { continue };
            if property.get("key") == Some(&json!("layer")) {
                property.remove("raw");
            } else if let Some(raw) = property.get_mut("raw") {
                round(raw);
            }
        }
    }
    entity
}

/// 適合テストの check と同じ確かめ方
fn check(name: &str, bytes: &[u8], format: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let same = edit_sxf(bytes, format, &[]).expect("edit without operations");
    if same.bytes != bytes {
        problems.push(format!("{name}: editing without operations changed the bytes"));
    }
    let before = read(bytes, format);
    let candidates: Vec<&zumen_core::SceneEntity> =
        before.entities.iter().filter(|entity| !entity.id.contains(['>', ':']) && !entity.id.is_empty()).collect();
    let updated = candidates.first().map(|entity| entity.id.clone());
    let deleted =
        candidates.iter().skip(1).find(|entity| SIMPLE.contains(&entity.kind.name())).map(|entity| entity.id.clone());
    let mut list = vec![
        json!({"op": "add-layer", "name": LAYER, "color": 1}),
        json!({"op": "add", "kind": "line", "layer": LAYER, "start": [0, 0], "end": [1000, 500]}),
        json!({"op": "add", "kind": "text", "layer": LAYER, "position": [0, -100], "text": "編集の確認", "height": 25}),
    ];
    if let Some(id) = &updated {
        list.push(json!({"op": "update", "id": id, "set": {"layer": LAYER}}));
    }
    if let Some(id) = &deleted {
        list.push(json!({"op": "delete", "id": id}));
    }
    let edited = match edit_sxf(bytes, format, &operations(Value::Array(list))) {
        Ok(edited) => edited,
        Err(error) => return vec![format!("{name}: {error}")],
    };
    let after = read(&edited.bytes, format);
    let added: Vec<&str> = edited.created.iter().skip(1).map(String::as_str).collect();
    let (before_entities, after_entities) = (entities(&before), entities(&after));
    for id in &added {
        if !after_entities.iter().any(|(other, _)| other == id) {
            problems.push(format!("{name}: added {id} is not in the drawing read again"));
        }
    }
    let mut kept = Vec::new();
    let mut updated_at = None;
    for (id, entity) in &before_entities {
        if Some(id) == deleted.as_ref() {
            continue;
        }
        if Some(id) == updated.as_ref() {
            updated_at = Some(kept.len());
        }
        kept.push((id, entity));
    }
    let remaining: Vec<&(String, Value)> =
        after_entities.iter().filter(|(id, _)| !added.contains(&id.as_str())).collect();
    if kept.len() != remaining.len() {
        problems.push(format!("{name}: {} entities kept, {} after the edit", kept.len(), remaining.len()));
        return problems;
    }
    for (index, ((id, entity), (other_id, other))) in kept.iter().zip(&remaining).enumerate() {
        // 変えた要素は、ほかの並びと共有するインスタンス（P21）なら新しい番号になる
        if *id != other_id && Some(index) != updated_at {
            problems.push(format!("{name}: {id} became {other_id}"));
        } else if Some(index) == updated_at {
            if layer_value(other) != Some(LAYER) {
                problems.push(format!("{name}: the layer of {id} is {:?}", layer_value(other)));
            }
        } else if strip_layer_ids(entity) != strip_layer_ids(other) {
            problems.push(format!("{name}: {id} changed"));
        }
    }
    for notice in &after.notices {
        if !before.notices.iter().any(|old| old.code == notice.code) {
            problems.push(format!("{name}: new notice {} ({})", notice.code, notice.message));
        }
    }
    problems
}

/// 圧縮した SXF（書庫に入れ直したもの）
fn compressed(bytes: &[u8], format: &str) -> (Vec<u8>, &'static str) {
    let mut drawing = parse_sxf(bytes, format).unwrap();
    let to = if format == "sfc" { "sfz" } else { "p2z" };
    // 元のファイルのまま書庫に入れる（書き出しは元を書き換えるので、何も変えなければ同じバイト列）
    let written = write_sxf(&drawing, to).unwrap();
    drawing.source = None;
    (written.bytes, to)
}

#[test]
fn edits_change_only_what_they_name() {
    let mut problems = Vec::new();
    for (format, name) in common::FIXTURES {
        let bytes = read_fixture(format, name);
        if read(&bytes, format).entities.is_empty() {
            continue;
        }
        problems.extend(check(&format!("{name}.{format}"), &bytes, format));
        let (archive, to) = compressed(&bytes, format);
        problems.extend(check(&format!("{name}.{to}"), &archive, to));
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
#[ignore = "ZUMEN_SAMPLES の図面を使う"]
fn edits_of_samples_change_only_what_they_name() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("ZUMEN_SAMPLES");
    let mut problems = Vec::new();
    let mut paths: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        let Some(format) = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) else { continue };
        if matches!(format.as_str(), "sfc" | "p21") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            problems.extend(check(&name, &std::fs::read(&path).unwrap(), &format));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn ids_through_sub_figures_are_their_members_and_parts_are_refused() {
    let bytes = read_fixture("sfc", "features");
    // 部分図「部分図」（#300）を置く #810 を通して、その線分 #270 を動かす
    let edited =
        edit_sxf(&bytes, "sfc", &operations(json!([{"op": "update", "id": "#810>#270", "set": {"end": [150, 0]}}])))
            .unwrap();
    let drawing = parse_sxf(&edited.bytes, "sfc").unwrap();
    let figure = drawing.document.sub_figures.get("部分図").unwrap();
    assert!(
        figure.members.iter().any(|member| matches!(member, SxfFeature::Line { x2: 150.0, .. })),
        "{:?}",
        figure.members
    );
    // 作図部品「部品」（#250）を用紙に置く #870 の中の線分 #220 は選べない。部分図の中の配置 #810>#260 の中も同じ
    for id in ["#870>#220", "#810>#260>#220"] {
        let error = edit_sxf(&bytes, "sfc", &operations(json!([{"op": "delete", "id": id}]))).unwrap_err();
        assert_eq!((error.code, error.operation), ("inside-block", Some(0)), "{id}");
        assert!(error.message.contains("部品"), "{}", error.message);
    }
    // 部分図の中の配置そのものは選べる
    let edited = edit_sxf(&bytes, "sfc", &operations(json!([{"op": "delete", "id": "#810>#260"}]))).unwrap();
    let drawing = parse_sxf(&edited.bytes, "sfc").unwrap();
    assert!(!drawing.document.sub_figures.get("部分図").unwrap().members.iter().any(|member| member.id() == 260.0));
    let error = edit_sxf(&bytes, "sfc", &operations(json!([{"op": "delete", "id": "#99999"}]))).unwrap_err();
    assert_eq!(error.code, "not-found");
}

#[test]
fn values_map_onto_sxf_features() {
    let bytes = read_fixture("sfc", "sample");
    let edited = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([
            {"op": "add", "kind": "arc", "layer": "部分", "center": [10, 20], "radius": 5, "angle": [30, 120],
             "color": "#ff0000", "lineType": "dashed", "lineWidth": 0.35},
            {"op": "add", "kind": "ellipse-arc", "center": [0, 0], "majorAxis": [0, 10], "ratio": 0.5,
             "parameter": [0, std::f64::consts::FRAC_PI_2], "color": "#123456", "lineWidth": 0.42},
            {"op": "add", "kind": "polyline", "vertices": [[0, 0], [10, 0], [10, 10]], "closed": true},
            {"op": "add", "kind": "point", "position": [3, 4], "color": 5},
            {"op": "add", "kind": "block-reference", "block": "G", "position": [50, 60], "rotation": 15, "scale": [2, 3]},
            {"op": "add", "kind": "line", "container": "figure:S", "start": [0, 0], "end": [5, 5]},
            {"op": "add", "kind": "text", "position": [1, 2], "text": "ABC表", "height": 4, "textStyle": "ＭＳ 明朝"}
        ])),
    )
    .unwrap();
    assert_eq!(edited.created.len(), 7);
    let drawing = parse_sxf(&edited.bytes, "sfc").unwrap();
    let doc = &drawing.document;
    let find = |id: &str| {
        let number: f64 = id.trim_start_matches('#').parse().unwrap();
        doc.sheet_members
            .iter()
            .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
            .find(|feature| feature.id() == number)
            .cloned()
            .unwrap()
    };
    // 円弧: 赤（既定義色 2）、破線（既定義線種 2）、0.35 mm（既定義線幅 4）、レイヤ「部分」（1）
    let SxfFeature::Arc { style, ccw, start, end, .. } = find(&edited.created[0]) else { panic!() };
    assert_eq!((style.layer, style.color, style.line_type, style.line_width), (1.0, 2.0, 2.0, 4.0));
    assert_eq!((ccw, start, end), (true, 30.0, 120.0));
    // 楕円弧: 傾き 90°、半径 10・5、パラメータは度。ユーザ定義色・線幅を足す
    let SxfFeature::EllipseArc { style, rx, ry, rotation, start, end, .. } = find(&edited.created[1]) else { panic!() };
    assert_eq!((rx, ry, rotation, start, end), (10.0, 5.0, 90.0, 0.0, 90.0));
    assert_eq!(doc.user_colors.last().map(|color| (color.r, color.g, color.b)), Some((18.0, 52.0, 86.0)));
    assert_eq!(style.color, (16 + doc.user_colors.len()) as f64);
    assert_eq!(style.line_width, (10 + doc.user_widths.len()) as f64);
    // 閉じた折線は最初の点を最後にも置く
    let SxfFeature::Polyline { xs, .. } = find(&edited.created[2]) else { panic!() };
    assert_eq!(xs, vec![0.0, 10.0, 10.0, 0.0]);
    assert!(matches!(find(&edited.created[3]), SxfFeature::PointMarker { color: 5.0, code: 3.0, .. }));
    let SxfFeature::SfigLocate { name, angle, ratio_x, ratio_y, .. } = find(&edited.created[4]) else { panic!() };
    assert_eq!((name.as_str(), angle, ratio_x, ratio_y), ("G", 15.0, 2.0, 3.0));
    assert!(
        doc.sub_figures
            .get("S")
            .unwrap()
            .members
            .iter()
            .any(|feature| matches!(feature, SxfFeature::Line { x2: 5.0, .. }))
    );
    // 文字範囲の幅は、半角 3 文字と全角 1 文字（高さ 4）で 10
    let SxfFeature::TextString { text, .. } = find(&edited.created[6]) else { panic!() };
    assert_eq!((text.width, text.anchor), (10.0, 1.0));
    assert_eq!(doc.font(text.font), Some("ＭＳ 明朝"));
}

#[test]
fn values_sxf_cannot_hold_are_refused_or_reported() {
    let bytes = read_fixture("sfc", "sample");
    let error = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([{"op": "add", "kind": "polyline", "vertices": [[0, 0], [1, 1]], "bulges": [0.5, 0]}])),
    )
    .unwrap_err();
    assert_eq!(error.code, "unsupported-kind");
    let error =
        edit_sxf(&bytes, "sfc", &operations(json!([{"op": "add", "kind": "hatch", "layer": "A"}]))).unwrap_err();
    assert_eq!(error.code, "unsupported-kind");
    let error = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 1], "lineType": "zigzag"}])),
    )
    .unwrap_err();
    assert_eq!(error.code, "missing-definition");
    let error = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([{"op": "add", "kind": "line", "container": "figure:ない", "start": [0, 0], "end": [1, 1]}])),
    )
    .unwrap_err();
    assert_eq!(error.code, "missing-definition");
    // レイヤから継ぐ色・レイヤの色は、近い値にして知らせる
    let edited = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([
            {"op": "add-layer", "name": "新", "color": 3, "visible": false},
            {"op": "add", "kind": "line", "layer": "新", "start": [0, 0], "end": [1, 1], "color": "bylayer"}
        ])),
    )
    .unwrap();
    let codes: Vec<&str> = edited.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert!(codes.contains(&"layer-style") && codes.contains(&"color-by-layer"), "{codes:?}");
    let error = edit_sxf(&bytes, "sfc", &operations(json!([{"op": "add-layer", "name": "部分"}]))).unwrap_err();
    assert_eq!(error.code, "already-exists");
}

#[test]
fn new_drawings_are_created_by_editing_an_empty_one() {
    for format in ["sfc", "p21", "sfz", "p2z"] {
        let empty = new_sxf(format).unwrap();
        let edited = edit_sxf(
            &empty,
            format,
            &operations(json!([
                {"op": "add-layer", "name": "外構"},
                {"op": "add", "kind": "polyline", "layer": "外構", "vertices": [[0, 0], [100, 0], [100, 80], [0, 80]], "closed": true},
                {"op": "add", "kind": "text", "layer": "外構", "position": [5, 5], "text": "敷地", "height": 3.5}
            ])),
        )
        .unwrap();
        let scene = read(&edited.bytes, format);
        let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
        assert_eq!(ids, &edited.created[1..], "{format}");
        assert_eq!(scene.layers.iter().map(|layer| layer.name.as_str()).collect::<Vec<_>>(), ["外構"], "{format}");
    }
}
