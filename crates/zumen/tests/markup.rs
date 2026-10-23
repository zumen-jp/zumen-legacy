//! マークアップを図面の新しい版へ移す（carry_markup）。図面は create と edit で作り、前の版と新しい版の違いを決める

use serde_json::{Value, json};
use zumen::markup::{CarriedState, MarkupDocument, drawing_hash};
use zumen::{CarryOptions, ReadDrawingOptions};

/// 矩形の 4 辺と 3 つの文字。すべてを (dx, dy) だけずらす
fn drawing(dx: f64, dy: f64) -> zumen::edit::Edited {
    let at = |x: f64, y: f64| json!([x + dx, y + dy]);
    let operations = json!([
        { "op": "add", "kind": "line", "start": at(0.0, 0.0), "end": at(200.0, 0.0) },
        { "op": "add", "kind": "line", "start": at(200.0, 0.0), "end": at(200.0, 100.0) },
        { "op": "add", "kind": "line", "start": at(0.0, 100.0), "end": at(200.0, 100.0) },
        { "op": "add", "kind": "line", "start": at(0.0, 0.0), "end": at(0.0, 100.0) },
        { "op": "add", "kind": "text", "position": at(20.0, 20.0), "text": "A", "height": 5 },
        { "op": "add", "kind": "text", "position": at(80.0, 50.0), "text": "B", "height": 5 },
        { "op": "add", "kind": "text", "position": at(150.0, 80.0), "text": "C", "height": 5 },
    ]);
    zumen::create("dxf", "mm", &zumen::edit::operations_from_json(&operations).unwrap()).unwrap()
}

fn markup(bytes: &[u8], ids: &[String]) -> MarkupDocument {
    MarkupDocument::from_value(&json!({
        "format": "zumen-markup",
        "version": 1,
        "drawing": { "sha256": drawing_hash(bytes), "format": "dxf" },
        "items": [
            { "id": "bottom", "kind": "finding", "view": "Model", "position": [100, 0], "status": "open", "entities": [ids[0]] },
            { "id": "right", "kind": "finding", "view": "Model", "position": [200, 50], "status": "open", "entities": [ids[1]] },
            { "id": "left", "kind": "finding", "view": "Model", "position": [0, 50], "status": "resolved", "entities": [ids[3]] },
            { "id": "free", "kind": "cloud", "view": "Model", "points": [[60, 30], [120, 30], [120, 70], [60, 70]], "arcLength": 5 },
            { "id": "sheet", "kind": "text", "view": "Layout9", "position": [10, 10], "text": "用紙のメモ" },
        ],
        "data": { "round": 2 },
    }))
    .unwrap()
}

fn states(document: &MarkupDocument) -> Vec<(&str, CarriedState)> {
    document.items.iter().map(|item| (item.id.as_str(), item.carried.as_ref().unwrap().state)).collect()
}

#[test]
fn carries_markup_to_an_edited_revision() {
    let before = drawing(0.0, 0.0);
    let ids = &before.created;
    assert_eq!(ids.len(), 7);
    // 新しい版は左の辺を消し、右の辺を延ばした
    let edits = json!([
        { "op": "delete", "id": ids[3] },
        { "op": "update", "id": ids[1], "set": { "end": [200, 140] } },
    ]);
    let after =
        zumen::edit(&before.bytes, &zumen::edit::operations_from_json(&edits).unwrap(), &ReadDrawingOptions::default())
            .unwrap();
    let document = markup(&before.bytes, ids);
    let carried = zumen::carry_markup(&document, &before.bytes, &after.bytes, &CarryOptions::default()).unwrap();
    assert_eq!(
        states(&carried.document),
        [
            ("bottom", CarriedState::Unchanged),
            ("right", CarriedState::Changed),
            ("left", CarriedState::Removed),
            ("free", CarriedState::Unchanged),
            ("sheet", CarriedState::Removed),
        ]
    );
    assert_eq!((carried.unchanged, carried.changed, carried.removed), (2, 1, 2));
    assert_eq!(carried.document.drawing.sha256, drawing_hash(&after.bytes));
    assert_eq!(carried.document.items[0].carried.as_ref().unwrap().from, drawing_hash(&before.bytes));
    // DXF の編集はハンドルを保つので、対応する図形の id は同じ
    assert_eq!(carried.document.items[0].entities.as_deref(), Some(&ids[..1]));
    assert_eq!(carried.document.items[1].entities.as_deref(), Some(&ids[1..2]));
    assert_eq!(carried.document.items[2].entities.as_deref(), Some(&[][..]));
    // ずれのない版では動かさず、ビューのない項目は元のビューのまま
    assert_eq!(carried.document.items[0].position(), Some([100.0, 0.0]));
    assert_eq!(carried.document.items[4].view.as_deref(), Some("Layout9"));
    let value = carried.document.to_value();
    assert_eq!(value["data"], json!({ "round": 2 }));
    assert_eq!(value["items"][1]["carried"]["state"], Value::from("changed"));
}

#[test]
fn moves_markup_by_the_offset_of_a_shifted_revision() {
    let before = drawing(0.0, 0.0);
    let after = drawing(100.0, 50.0);
    let document = markup(&before.bytes, &before.created);
    let carried = zumen::carry_markup(&document, &before.bytes, &after.bytes, &CarryOptions::default()).unwrap();
    assert_eq!(carried.document.items[0].position(), Some([200.0, 50.0]));
    assert_eq!(carried.document.items[3].points().unwrap()[0], [160.0, 80.0]);
    assert_eq!(
        &states(&carried.document)[..4].iter().map(|(_, state)| *state).collect::<Vec<_>>(),
        &[CarriedState::Unchanged, CarriedState::Unchanged, CarriedState::Unchanged, CarriedState::Unchanged,]
    );
    // ずらすと比べずに、図形の位置で比べる（ずれを見つけない）と、どの辺も消えて別の辺ができたことになる
    let fixed = CarryOptions {
        compare: zumen::compare::CompareOptions { align: false, all_layers: true, ..Default::default() },
        ..CarryOptions::default()
    };
    let unaligned = zumen::carry_markup(&document, &before.bytes, &after.bytes, &fixed).unwrap();
    assert_eq!(unaligned.document.items[0].position(), Some([100.0, 0.0]));
    assert_eq!(states(&unaligned.document)[0].1, CarriedState::Removed);
}

#[test]
fn maps_views_and_refuses_markup_of_another_drawing() {
    let before = drawing(0.0, 0.0);
    let after = drawing(0.0, 0.0);
    let document = markup(&before.bytes, &before.created);
    // 対応表で、新しい版のほかのビューへ移す。ないビューへ移す項目は removed
    let to = |view: &str| CarryOptions {
        views: [("Model".to_string(), view.to_string())].into_iter().collect(),
        ..CarryOptions::default()
    };
    let carried = zumen::carry_markup(&document, &before.bytes, &after.bytes, &to("Layout1")).unwrap();
    assert_eq!(carried.document.items[0].view.as_deref(), Some("Layout1"));
    let carried = zumen::carry_markup(&document, &before.bytes, &after.bytes, &to("Nope")).unwrap();
    assert_eq!(carried.removed, 5);
    assert_eq!(carried.document.items[0].view.as_deref(), Some("Model"));
    let other = drawing(1.0, 0.0);
    let error = zumen::carry_markup(&document, &other.bytes, &after.bytes, &CarryOptions::default()).unwrap_err();
    assert_eq!(error.code, "drawing-mismatch");
    let ignored = CarryOptions { ignore_hash: true, ..CarryOptions::default() };
    assert!(zumen::carry_markup(&document, &other.bytes, &after.bytes, &ignored).is_ok());
    let error = zumen::carry_markup(&document, &before.bytes, b"not a drawing", &CarryOptions::default()).unwrap_err();
    assert_eq!(error.code, "unreadable");
}
