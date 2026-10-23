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
        edit_sxf(&bytes, "sfc", &operations(json!([{"op": "add", "kind": "leader", "layer": "A"}]))).unwrap_err();
    assert_eq!(error.code, "unsupported-kind");
    let error =
        edit_sxf(&bytes, "sfc", &operations(json!([{"op": "add", "kind": "hatch", "layer": "A"}]))).unwrap_err();
    assert_eq!(error.code, "invalid-value");
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
    assert!(codes.contains(&"edit-layer-attributes") && codes.contains(&"edit-color-by-layer"), "{codes:?}");
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

#[test]
fn adding_non_curve_elements_reports_unused_line_attributes() {
    for format in ["sfc", "p21", "sfz", "p2z"] {
        let source = new_sxf(format).unwrap();
        let edited = edit_sxf(
            &source,
            format,
            &operations(json!([
                {"op":"add", "kind":"point", "position":[1,2], "lineType":"continuous", "lineWidth":0.5},
                {"op":"add", "kind":"text", "position":[3,4], "text":"note", "height":2, "lineWidth":0.5}
            ])),
        )
        .unwrap();
        assert!(edited.notices.iter().any(|notice| notice.code == "edit-unused-attributes" && notice.count == 2));
    }
}

/// 作図部品を足して中に要素を足し、配置する。寸法と塗り・ハッチングを足す（docs/decisions/0017、0025）。
/// SFC・P21・SFZ・P2Z のすべての fixtures で、読み直すと足したものが描け、ほかの要素はそのまま
#[test]
fn adds_parts_dimensions_and_hatches() {
    let list = operations(json!([
        {"op": "add-block", "name": "扉", "basePoint": [100, 0]},
        {"op": "add", "kind": "line", "container": "block:扉", "start": [100, 0], "end": [190, 0]},
        {"op": "add", "kind": "circle", "container": "block:扉", "center": [100, 0], "radius": 10},
        {"op": "add", "kind": "block-reference", "block": "扉", "position": [50, 50], "rotation": 90},
        {"op": "add", "kind": "dimension", "start": [0, 0], "end": [100, 0], "position": [0, -30]},
        {"op": "add", "kind": "dimension", "dimensionType": "aligned", "start": [0, 0], "end": [60, 80], "position": [-10, 10]},
        {"op": "add", "kind": "dimension", "dimensionType": "angular-3-point", "center": [0, 0], "start": [50, 0], "end": [0, 50], "position": [30, 30]},
        {"op": "add", "kind": "dimension", "dimensionType": "radius", "center": [0, 0], "point": [40, 0], "color": 2},
        {"op": "add", "kind": "dimension", "dimensionType": "diameter", "center": [0, 0], "point": [0, 40]},
        {"op": "add", "kind": "hatch", "boundaries": [[0, 0], [100, 0], [100, 50], [0, 50]], "color": "#ff0000"},
        {"op": "add", "kind": "hatch", "pattern": "ANSI31", "boundaries": [[[0, 100], [100, 100], [100, 150], [0, 150]], [[40, 110], [60, 110], [60, 140], [40, 140]]]},
        {"op": "add", "kind": "hatch", "pattern": "DOTS", "patternScale": 2, "boundaries": [[200, 0], [300, 0], [300, 50], [200, 50]]}
    ]));
    let mut problems = Vec::new();
    for (format, name) in common::FIXTURES {
        let original = read_fixture(format, name);
        let (archive, to) = compressed(&original, format);
        for (bytes, format) in [(original, format), (archive, to)] {
            let label = format!("{name}.{format}");
            let edited = match edit_sxf(&bytes, format, &list) {
                Ok(edited) => edited,
                Err(error) => {
                    problems.push(format!("{label}: {error}"));
                    continue;
                }
            };
            let (before, after) = (read(&bytes, format), read(&edited.bytes, format));
            let reference = &edited.created[3];
            let inside = after.entities.iter().filter(|entity| entity.id.starts_with(&format!("{reference}>"))).count();
            if inside != 2 {
                problems.push(format!("{label}: {inside} elements drawn through the new placement"));
            }
            for (position, kind) in [
                (4, "dimension"),
                (5, "dimension"),
                (6, "dimension"),
                (7, "dimension"),
                (8, "dimension"),
                (9, "hatch"),
                (10, "hatch"),
                (11, "hatch"),
            ] {
                let id = &edited.created[position];
                match after.entities.iter().find(|entity| &entity.id == id) {
                    Some(entity) if entity.kind.name() == kind && !entity.primitives.is_empty() => {}
                    other => problems.push(format!("{label}: {id} is {:?}", other.map(|entity| entity.kind.name()))),
                }
            }
            if after.entities.len() != before.entities.len() + 2 + 8 {
                problems.push(format!(
                    "{label}: {} elements before, {} after",
                    before.entities.len(),
                    after.entities.len()
                ));
            }
            for notice in &after.notices {
                if !before.notices.iter().any(|old| old.code == notice.code) {
                    problems.push(format!("{label}: new notice {} ({})", notice.code, notice.message));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn writes_parts_dimensions_and_hatches_as_sxf_features() {
    let bytes = read_fixture("sfc", "sample");
    let edited = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([
            {"op": "add-block", "name": "扉", "basePoint": [100, 0]},
            {"op": "add", "kind": "line", "container": "block:扉", "start": [100, 0], "end": [190, 0]},
            {"op": "add", "kind": "dimension", "start": [0, 0], "end": [100, 0], "position": [0, -30], "dimensionStyle": "Standard"},
            {"op": "add", "kind": "hatch", "boundaries": [[0, 0], [100, 0], [100, 50], [0, 50]], "color": "#ff0000", "lineType": "dashed"},
            {"op": "add", "kind": "hatch", "pattern": "DOTS", "boundaries": [[200, 0], [300, 0], [300, 50], [200, 50]]}
        ])),
    )
    .unwrap();
    assert_eq!(edited.created[0], "扉");
    let codes: Vec<&str> = edited.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert!(
        ["edit-dimension-style", "edit-unused-attributes", "edit-hatch-shift"].iter().all(|code| codes.contains(code)),
        "{codes:?}"
    );
    let before = parse_sxf(&bytes, "sfc").unwrap().document;
    let doc = parse_sxf(&edited.bytes, "sfc").unwrap().document;
    // 作図部品の座標は基点を原点に
    let part = doc.sub_figures.get("扉").unwrap();
    assert_eq!(part.feature.flag, 4.0);
    assert!(matches!(part.members[..], [SxfFeature::Line { x1: 0.0, y1: 0.0, x2: 90.0, y2: 0.0, .. }]));
    let find = |id: &str| doc.sheet_members.iter().find(|feature| format!("#{}", feature.id()) == id).unwrap();
    let SxfFeature::LinearDim { line, ext1, arrow1, text, .. } = find(&edited.created[2]) else { panic!() };
    assert_eq!(*line, [0.0, -30.0, 100.0, -30.0]);
    assert_eq!(ext1.as_ref().unwrap().y2, -31.25);
    assert_eq!(arrow1.as_ref().unwrap().code, 6.0);
    let text = text.as_ref().unwrap();
    assert_eq!((text.text.as_str(), text.height, text.x, text.y), ("100", 2.5, 50.0, -29.375));
    // 塗りは見えない複合曲線の上に
    let SxfFeature::FillAreaStyleColour { color, region, .. } = find(&edited.created[3]) else { panic!() };
    assert_eq!(doc.color(*color).as_deref(), Some("#ff0000"));
    let curve = &doc.composite_curves[region.out_id as usize - 1];
    assert!(!curve.feature.visible && region.in_ids.is_empty());
    assert_eq!(doc.composite_curves.len(), before.composite_curves.len() + 2);
    // 点の模様は 0.25 mm の線のユーザ定義線種
    let SxfFeature::FillAreaStyleHatching { patterns, .. } = find(&edited.created[4]) else { panic!() };
    assert_eq!(patterns.len(), 1);
    let line_type = &doc.user_line_types[patterns[0].line_type as usize - 17];
    assert_eq!((line_type.name.as_str(), line_type.pitch.as_slice()), ("DOTS", [0.25, 1.3375].as_slice()));
    let error = |list: Value| edit_sxf(&bytes, "sfc", &operations(list)).unwrap_err();
    assert_eq!(
        error(json!([{"op": "add-block", "name": "扉"}, {"op": "add-block", "name": "扉"}])).code,
        "already-exists"
    );
    assert_eq!(error(json!([{"op": "add-block", "name": "$$SXF_P_扉"}])).code, "invalid-value");
    let cycle = error(json!([
        {"op": "add-block", "name": "A"},
        {"op": "add-block", "name": "B"},
        {"op": "add", "kind": "block-reference", "container": "block:A", "block": "B", "position": [0, 0]},
        {"op": "add", "kind": "block-reference", "container": "block:B", "block": "A", "position": [0, 0]}
    ]));
    assert_eq!((cycle.code, cycle.operation), ("invalid-value", Some(3)));
    let degenerate =
        error(json!([{"op": "add", "kind": "dimension", "start": [0, 0], "end": [0, 0], "position": [0, -30]}]));
    assert_eq!(degenerate.code, "invalid-value");
}

/// 部分図の中の寸法は、配置の尺度で割った大きさ（用紙の上で 2.5 mm の文字）
#[test]
fn sizes_dimensions_in_sub_figures_by_their_placement() {
    let bytes = read_fixture("sfc", "features");
    let drawing = parse_sxf(&bytes, "sfc").unwrap();
    let (name, ratio) = drawing
        .document
        .sheet_members
        .iter()
        .find_map(|feature| match feature {
            SxfFeature::SfigLocate { name, ratio_x, ratio_y, .. }
                if drawing.document.sub_figures.get(name).is_some_and(|figure| figure.feature.flag == 1.0) =>
            {
                Some((name.clone(), (ratio_x.abs() * ratio_y.abs()).sqrt()))
            }
            _ => None,
        })
        .expect("a sub-figure placed on the sheet");
    let edited = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([{"op": "add", "kind": "dimension", "container": format!("figure:{name}"), "start": [0, 0], "end": [10, 0], "position": [0, -5]}])),
    )
    .unwrap();
    let doc = parse_sxf(&edited.bytes, "sfc").unwrap().document;
    let id = &edited.created[0];
    let figure = doc.sub_figures.get(&name).unwrap();
    let SxfFeature::LinearDim { text, arrow1, .. } =
        figure.members.iter().find(|feature| format!("#{}", feature.id()) == *id).unwrap()
    else {
        panic!()
    };
    assert!((text.as_ref().unwrap().height * ratio - 2.5).abs() < 1e-9, "{ratio}");
    assert!((arrow1.as_ref().unwrap().scale - 1.0 / 3.0).abs() < 1e-12);
}

/// この編集で足した寸法スタイルの値で寸法のフィーチャを描く（docs/decisions/0026）。尺度のないスタイルの長さは用紙の
/// mm（部分図の中は配置の尺度で割る）、尺度のあるスタイルは尺度を掛けた入れ物の座標。矢印は用紙の大きさで書く
#[test]
fn sizes_and_formats_dimensions_with_styles_added_in_the_edit() {
    let bytes = read_fixture("sfc", "features");
    let drawing = parse_sxf(&bytes, "sfc").unwrap();
    let (name, ratio) = drawing
        .document
        .sheet_members
        .iter()
        .find_map(|feature| match feature {
            SxfFeature::SfigLocate { name, ratio_x, ratio_y, .. }
                if drawing.document.sub_figures.get(name).is_some_and(|figure| figure.feature.flag == 1.0) =>
            {
                Some((name.clone(), (ratio_x.abs() * ratio_y.abs()).sqrt()))
            }
            _ => None,
        })
        .expect("a sub-figure placed on the sheet");
    let edited = edit_sxf(
        &bytes,
        "sfc",
        &operations(json!([
            {"op": "add-text-style", "name": "明朝", "font": "ＭＳ 明朝"},
            {"op": "add-dimension-style", "name": "斜線", "textHeight": 3.5, "tickSize": 1.5, "extensionOffset": 1, "extensionExtension": 2, "textGap": 1, "decimals": 2, "suppressTrailingZeros": false, "textStyle": "明朝"},
            {"op": "add-dimension-style", "name": "DIM-R", "from": "斜線", "scale": 2.0 / ratio, "arrowSize": 3, "tickSize": 0},
            {"op": "add-dimension-style", "name": "中央", "textAbove": false, "linearFactor": 0.001, "decimals": 3},
            {"op": "add", "kind": "dimension", "dimensionStyle": "斜線", "start": [0, 0], "end": [100, 0], "position": [0, -30]},
            {"op": "add", "kind": "dimension", "container": format!("figure:{name}"), "dimensionStyle": "dim-r", "start": [0, 0], "end": [10, 0], "position": [0, -5]},
            {"op": "add", "kind": "dimension", "dimensionStyle": "中央", "dimensionType": "diameter", "center": [0, 0], "point": [50, 0]},
            {"op": "add", "kind": "dimension", "dimensionStyle": "中央", "dimensionType": "angular-3-point", "center": [0, 0], "start": [100, 0], "end": [0, 100], "position": [50, 50]}
        ])),
    )
    .unwrap();
    assert_eq!(edited.created[..4], ["明朝", "斜線", "DIM-R", "中央"]);
    let codes: Vec<&str> = edited.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert_eq!(codes, ["edit-dimension-text-position"]);
    let doc = parse_sxf(&edited.bytes, "sfc").unwrap().document;
    let find = |id: &str| doc.sheet_members.iter().find(|feature| format!("#{}", feature.id()) == id).unwrap();
    let SxfFeature::LinearDim { ext1, arrow1, text, .. } = find(&edited.created[4]) else { panic!() };
    let text = text.as_ref().unwrap();
    assert_eq!((text.text.as_str(), text.height, text.y), ("100.00", 3.5, -29.0));
    assert_eq!(doc.font(text.font), Some("ＭＳ 明朝"));
    let ext1 = ext1.as_ref().unwrap();
    assert_eq!((ext1.y1, ext1.y2), (-1.0, -32.0));
    // 斜線（コード 10）は長さ 2.5 mm × 倍率で 45° に傾く。DXF の斜線（DIMTSZ × √2）と同じ長さ
    let arrow1 = arrow1.as_ref().unwrap();
    assert_eq!(arrow1.code, 10.0);
    assert!((arrow1.scale * 2.5 - 1.5 * std::f64::consts::SQRT_2).abs() < 1e-12);
    // 尺度 2 / 配置の尺度の部分図: 用紙の上で文字は 7 mm、矢印は 6 mm（倍率 0.8）
    let figure = doc.sub_figures.get(&name).unwrap();
    let SxfFeature::LinearDim { text, arrow1, .. } =
        figure.members.iter().find(|feature| format!("#{}", feature.id()) == edited.created[5]).unwrap()
    else {
        panic!()
    };
    assert!((text.as_ref().unwrap().height * ratio - 7.0).abs() < 1e-9, "{ratio}");
    let arrow1 = arrow1.as_ref().unwrap();
    assert!(arrow1.code == 6.0 && (arrow1.scale - 0.8).abs() < 1e-12, "{arrow1:?}");
    // 寸法線の上に置かない寸法値は、寸法線に高さの中央を重ねる。倍率 0.001 で 0.1（後ろの 0 は省く）
    let SxfFeature::DiameterDim { text, .. } = find(&edited.created[6]) else { panic!() };
    let text = text.as_ref().unwrap();
    assert_eq!((text.text.as_str(), text.y), ("φ0.1", -1.25));
    // 角度は度の整数のまま
    let SxfFeature::AngularDim(angular) = find(&edited.created[7]) else { panic!() };
    assert_eq!(angular.text.as_ref().unwrap().text, "90°");
    // 用紙の寸法は読み直して描ける（部分図の中の寸法の id は配置を通した id）
    let scene = read(&edited.bytes, "sfc");
    for id in [&edited.created[4], &edited.created[6], &edited.created[7]] {
        assert!(scene.entities.iter().any(|entity| &entity.id == id && !entity.primitives.is_empty()), "{id}");
    }
    let error = |list: Value| edit_sxf(&bytes, "sfc", &operations(list)).unwrap_err();
    assert_eq!(
        error(json!([{"op": "add-dimension-style", "name": "A"}, {"op": "add-dimension-style", "name": "a"}])).code,
        "already-exists"
    );
    assert_eq!(error(json!([{"op": "add-dimension-style", "name": "A", "textHeight": 0}])).code, "invalid-value");
}

/// 線種はユーザ定義線種、文字スタイルは文字フォント（docs/decisions/0026）。SFC・P21・SFZ・P2Z で読み直せる
#[test]
fn adds_line_types_and_fonts_with_their_names() {
    for format in ["sfc", "p21", "sfz", "p2z"] {
        let empty = new_sxf(format).unwrap();
        let edited = edit_sxf(
            &empty,
            format,
            &operations(json!([
                {"op": "add-text-style", "name": "JP", "font": "ＭＳ ゴシック", "height": 0, "widthFactor": 1},
                {"op": "add-line-type", "name": "一点鎖線2", "description": "_ . _", "pattern": [12, -1.5, 0, -1.5]},
                {"op": "add-line-type", "name": "実線2"},
                {"op": "add", "kind": "line", "lineType": "一点鎖線2", "start": [0, 0], "end": [100, 0]},
                {"op": "add", "kind": "line", "lineType": "実線2", "start": [0, 10], "end": [100, 10]},
                {"op": "add", "kind": "text", "textStyle": "jp", "position": [0, 20], "text": "注記", "height": 3.5}
            ])),
        )
        .unwrap();
        assert_eq!(edited.created[..3], ["JP", "一点鎖線2", "実線2"], "{format}");
        let codes: Vec<&str> = edited.notices.iter().map(|notice| notice.code.as_str()).collect();
        assert_eq!(codes, ["edit-line-type-pattern", "edit-style-values"], "{format}");
        let doc = parse_sxf(&edited.bytes, format).unwrap().document;
        let line_type = &doc.user_line_types[0];
        assert_eq!(line_type.name.trim_start_matches("$$SXF_"), "一点鎖線2", "{format}");
        // 点は 0.25 mm の線にし、続く空白をその分だけ短くする
        assert_eq!(line_type.pitch, [12.0, 1.5, 0.25, 1.25], "{format}");
        let styles: Vec<f64> = doc
            .sheet_members
            .iter()
            .filter_map(|feature| match feature {
                SxfFeature::Line { style, .. } => Some(style.line_type),
                _ => None,
            })
            .collect();
        assert_eq!(styles, [17.0, 1.0], "{format}");
        let text = doc.sheet_members.iter().find_map(|feature| match feature {
            SxfFeature::TextString { text, .. } => Some(text),
            _ => None,
        });
        assert_eq!(doc.font(text.unwrap().font), Some("ＭＳ ゴシック"), "{format}");
    }
    let bytes = new_sxf("sfc").unwrap();
    let error = |list: Value| edit_sxf(&bytes, "sfc", &operations(list)).unwrap_err();
    assert_eq!(error(json!([{"op": "add-line-type", "name": "Dashed", "pattern": [1, -1]}])).code, "already-exists");
    let many: Vec<Value> =
        (0..17).map(|n| json!({"op": "add-line-type", "name": format!("線種{n}"), "pattern": [1 + n, -1]})).collect();
    let exhausted = error(Value::Array(many));
    assert_eq!((exhausted.code, exhausted.operation), ("name-exhausted", Some(16)));
    assert_eq!(
        error(json!([{"op": "add-text-style", "name": "JP", "font": "ＭＳ ゴシック"}, {"op": "add-text-style", "name": "ＭＳ ゴシック"}]))
            .code,
        "already-exists"
    );
}
