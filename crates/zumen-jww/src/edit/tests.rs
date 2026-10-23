use super::*;
use crate::{JwwCircleSolid, jww_to_scene, read_jww};
use std::f64::consts::PI;
use zumen_core::edit::parse_operations;

/// tests/fixtures/jww の図面。グループ 0 は 1/100 の「平面」（レイヤ 0 は「通り芯」）、ほかは 1/1、レイヤ 2 は非表示。
/// 図形は #1 線、#2 線（レイヤ 2）、#3 文字（レイヤ 1）、#4 円弧、#5 ブロック「部品」の配置、#6 Jw_cad の設定の文字
fn fixture(version: u32) -> Vec<u8> {
    let path = format!("{}/../../tests/fixtures/jww/minimal-v{version}.jww", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(path).unwrap()
}

fn edit(bytes: &[u8], operations: &str) -> Result<Edited, EditError> {
    edit_jww(bytes, &parse_operations(operations).expect("operations"))
}

fn codes(edited: &Edited) -> Vec<&str> {
    edited.notices.iter().map(|notice| notice.code.as_str()).collect()
}

fn property<'a>(scene: &'a zumen_core::Scene, id: &str, key: &str) -> &'a zumen_core::Property {
    let entity = scene.entities.iter().find(|entity| entity.id == id).unwrap_or_else(|| panic!("no entity {id}"));
    entity.properties.iter().find(|property| property.key == key).unwrap_or_else(|| panic!("no {key} in {id}"))
}

fn raw(scene: &zumen_core::Scene, id: &str, key: &str) -> serde_json::Value {
    serde_json::to_value(&property(scene, id, key).raw).unwrap()
}

#[test]
fn keeps_every_byte_without_operations() {
    for version in [600, 700] {
        let bytes = fixture(version);
        let edited = edit_jww(&bytes, &[]).unwrap();
        assert!(edited.bytes == bytes && edited.created.is_empty() && edited.notices.is_empty(), "{version}");
    }
}

#[test]
fn adds_entities_in_real_millimeters_of_their_layer_group() {
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "add", "kind": "line", "start": [0, 0], "end": [1000, 500]},
            {"op": "add", "kind": "circle", "layer": "1-3", "center": [10, 20], "radius": 5, "color": 4},
            {"op": "add", "kind": "arc", "center": [0, 0], "radius": 100, "angle": [350, 10], "lineType": 2},
            {"op": "add", "kind": "ellipse-arc", "container": "group:0", "center": [0, 0], "majorAxis": [0, 200], "ratio": 0.5, "parameter": [0, 3]},
            {"op": "add", "kind": "point", "position": [100, 100]},
            {"op": "add", "kind": "text", "position": [100, -100], "text": "確認 OK", "height": 3, "rotation": 90},
            {"op": "add", "kind": "block-reference", "block": "部品", "position": [200, 0], "scale": 2, "rotation": 90}
        ]"##,
    )
    .unwrap();
    assert_eq!(edited.created, ["#7", "#8", "#9", "#10", "#11", "#12", "#13"]);
    assert!(edited.notices.is_empty(), "{:?}", edited.notices);
    let doc = read_jww(&edited.bytes).unwrap();
    assert_eq!(doc.entities.len(), 13);
    // 書込みレイヤグループ 0（1/100）の書込みレイヤ 0 に、用紙の mm（実寸 / 100）で
    let JwwEntity::Line(line) = &doc.entities[6] else { panic!("line") };
    assert_eq!((line.x1, line.y1, line.x2, line.y2), (0.0, 0.0, 10.0, 5.0));
    assert_eq!((line.base.layer_group, line.base.layer, line.base.pen_color, line.base.pen_style), (0, 0, 2, 1));
    let JwwEntity::Arc(circle) = &doc.entities[7] else { panic!("circle") };
    assert_eq!((circle.base.layer_group, circle.base.layer, circle.base.pen_color), (1, 3, 4));
    assert_eq!((circle.cx, circle.cy, circle.radius, circle.full), (10.0, 20.0, 5.0, true));
    let JwwEntity::Arc(arc) = &doc.entities[8] else { panic!("arc") };
    assert_eq!((arc.radius, arc.base.pen_style, arc.full), (1.0, 2, false));
    assert!((arc.start - 350f64.to_radians()).abs() < 1e-12 && (arc.sweep - 20f64.to_radians()).abs() < 1e-12);
    let JwwEntity::Arc(ellipse) = &doc.entities[9] else { panic!("ellipse") };
    assert_eq!(
        (ellipse.radius, ellipse.flatness, ellipse.tilt, ellipse.start, ellipse.sweep),
        (2.0, 0.5, PI / 2.0, 0.0, 3.0)
    );
    let JwwEntity::Text(text) = &doc.entities[11] else { panic!("text") };
    // 全角 2 文字と半角 3 文字（3.5 文字分）× 幅 3 mm を、90° の向きに
    assert_eq!((text.x1, text.y1, text.size_x, text.size_y, text.angle), (1.0, -1.0, 3.0, 3.0, 90.0));
    assert!((text.x2 - 1.0).abs() < 1e-12 && (text.y2 - 9.5).abs() < 1e-12, "{} {}", text.x2, text.y2);
    assert_eq!((text.font.as_str(), text.content.as_str(), text.text_type), ("ＭＳ ゴシック", "確認 OK", 0));
    let JwwEntity::Block(block) = &doc.entities[12] else { panic!("block") };
    assert_eq!(
        (block.x, block.y, block.scale_x, block.scale_y, block.rotation, block.definition),
        (2.0, 0.0, 2.0, 2.0, PI / 2.0, 7)
    );
    // 表示用の図面の値は、操作の座標（実寸）と同じ
    let scene = jww_to_scene(&doc);
    assert_eq!(raw(&scene, "#7", "end"), serde_json::json!([1000.0, 500.0]));
    assert_eq!(raw(&scene, "#8", "center"), serde_json::json!([10.0, 20.0]));
    assert_eq!(raw(&scene, "#9", "radius"), serde_json::json!(100.0));
    assert_eq!(raw(&scene, "#11", "position"), serde_json::json!([100.0, 100.0]));
    assert!(scene.entities.iter().any(|entity| entity.id == "#13>#1"));
    // 読んで書くと同じ
    assert_eq!(JwwFile::read(&edited.bytes).unwrap().write().bytes, edited.bytes);
}

#[test]
fn updates_values_and_keeps_real_sizes_across_layer_groups() {
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "update", "id": "#1", "set": {"end": [500, 0], "color": 5, "lineWidth": "default"}},
            {"op": "update", "id": "#2", "set": {"color": "#123456"}},
            {"op": "update", "id": "#3", "set": {"text": "変更した文字"}},
            {"op": "update", "id": "#4", "set": {"layer": "1-0"}},
            {"op": "update", "id": "#5", "set": {"position": [1000, 1000], "scale": [3, 4]}}
        ]"##,
    )
    .unwrap();
    assert_eq!(codes(&edited), ["edit-rescaled"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let JwwEntity::Line(line) = &doc.entities[0] else { panic!("line") };
    assert_eq!((line.x2, line.y2, line.base.pen_color), (5.0, 0.0, 5));
    // #123456 は SXF 対応色のユーザー定義の色（線色 117）として足す
    let JwwEntity::Line(line) = &doc.entities[1] else { panic!("line") };
    assert_eq!(line.base.pen_color, 117);
    assert_eq!(doc.extended_colors.as_ref().unwrap()[17], "#123456");
    // 文字を変えると、終点を文字の幅で求め直す（全角 6 文字 × 幅 5 mm）
    let JwwEntity::Text(text) = &doc.entities[2] else { panic!("text") };
    assert_eq!((text.content.as_str(), text.x2, text.y2), ("変更した文字", 30.0, 10.0));
    // 1/100 のグループから 1/1 のグループに移すと、実寸（半径 1000 mm）が変わらない
    let JwwEntity::Arc(arc) = &doc.entities[3] else { panic!("arc") };
    assert_eq!((arc.base.layer_group, arc.radius), (1, 1000.0));
    let JwwEntity::Block(block) = &doc.entities[4] else { panic!("block") };
    assert_eq!((block.x, block.y, block.scale_x, block.scale_y), (10.0, 10.0, 3.0, 4.0));
    let scene = jww_to_scene(&doc);
    assert_eq!(raw(&scene, "#4", "radius"), serde_json::json!(1000.0));
    assert_eq!(raw(&scene, "#1", "end"), serde_json::json!([500.0, 0.0]));
}

#[test]
fn chooses_entities_by_their_ids_as_read() {
    let original = read_jww(&fixture(700)).unwrap();
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "delete", "id": "#1"},
            {"op": "update", "id": "#3", "set": {"text": "残る"}},
            {"op": "add", "kind": "line", "start": [0, 0], "end": [100, 0]},
            {"op": "delete", "id": "#2"}
        ]"##,
    )
    .unwrap();
    // 足した線は、2 つ消した後の 5 番目
    assert_eq!(edited.created, ["#5"]);
    let doc = read_jww(&edited.bytes).unwrap();
    assert_eq!(doc.entities.len(), 5);
    let JwwEntity::Text(text) = &doc.entities[0] else { panic!("text") };
    assert_eq!(text.content, "残る");
    assert_eq!(doc.entities[1..4], original.entities[3..6]);
    assert_eq!(doc.blocks, original.blocks);
    // 消した図形は選べない
    let error = edit(&fixture(700), r##"[{"op": "delete", "id": "#1"}, {"op": "delete", "id": "#1"}]"##).unwrap_err();
    assert_eq!((error.code, error.operation), ("not-found", Some(1)));
}

#[test]
fn rewrites_only_the_records_that_change() {
    let bytes = fixture(700);
    let edited = edit(&bytes, r##"[{"op": "update", "id": "#3", "set": {"color": 3}}]"##).unwrap();
    // 違うのは文字のレコードの線色の 1 バイトだけ
    let changed: Vec<usize> =
        bytes.iter().zip(&edited.bytes).enumerate().filter(|(_, (a, b))| a != b).map(|(at, _)| at).collect();
    assert_eq!((changed.len(), edited.bytes.len()), (1, bytes.len()));
    assert_eq!((bytes[changed[0]], edited.bytes[changed[0]]), (2, 3));
    // レイヤの名前を変えるとヘッダだけが変わり、図形の一覧は同じバイト列
    let edited = edit(&bytes, r##"[{"op": "update-layer", "name": "通り芯", "set": {"name": "基準線"}}]"##).unwrap();
    let tail = bytes.len() - 600;
    assert_eq!(edited.bytes[edited.bytes.len() - 600..], bytes[tail..]);
    let doc = read_jww(&edited.bytes).unwrap();
    assert_eq!(doc.groups[0].layers[0].name, "基準線");
}

#[test]
fn adds_and_changes_layers_of_the_fixed_slots() {
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "add-layer", "name": "設備", "color": 1},
            {"op": "add-layer", "name": "外構", "group": 2, "visible": false},
            {"op": "add-layer", "name": "詳細", "group": "5", "scale": 50},
            {"op": "update-layer", "name": "設備", "set": {"name": "設備 2", "locked": true}},
            {"op": "update-layer", "name": "0-2", "set": {"visible": true}},
            {"op": "add", "kind": "line", "layer": "新しいレイヤ", "start": [0, 0], "end": [100, 0]},
            {"op": "add", "kind": "line", "layer": "詳細", "start": [0, 0], "end": [500, 0]}
        ]"##,
    )
    .unwrap();
    assert_eq!(&edited.created[..3], ["0-3 設備", "2-1 外構", "5-0 詳細"]);
    assert_eq!(codes(&edited), ["edit-layer-attributes", "edit-added-layer"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let layer = |group: usize, layer: usize| {
        (doc.groups[group].layers[layer].name.as_str(), doc.groups[group].layers[layer].state)
    };
    // レイヤグループ 0 の名前も図形もない最初のレイヤ（3）。グループ 2 は書込みレイヤ 0 を非表示にできないので次の 1
    assert_eq!(layer(0, 3), ("設備 2", 1));
    assert_eq!(layer(2, 1), ("外構", 0));
    assert_eq!(layer(0, 2).1, 2);
    assert_eq!(layer(0, 4).0, "新しいレイヤ");
    assert_eq!(doc.groups[5].scale, 50.0);
    let JwwEntity::Line(line) = &doc.entities[7] else { panic!("line") };
    assert_eq!((line.base.layer_group, line.base.layer, line.x2), (5, 0, 10.0));
    // 図形のあるレイヤグループの縮尺と、書込みレイヤの非表示は変えられない
    let error =
        edit(&fixture(700), r##"[{"op": "update-layer", "name": "通り芯", "set": {"scale": 200}}]"##).unwrap_err();
    assert_eq!(error.code, "invalid-value");
    let error =
        edit(&fixture(700), r##"[{"op": "update-layer", "name": "0-0", "set": {"visible": false}}]"##).unwrap_err();
    assert_eq!(error.code, "invalid-value");
    let error = edit(&fixture(700), r##"[{"op": "add-layer", "name": "通り芯"}]"##).unwrap_err();
    assert_eq!(error.code, "already-exists");
    let error = edit(&fixture(700), r##"[{"op": "add-layer", "name": "0-1"}]"##).unwrap_err();
    assert_eq!(error.code, "invalid-value");
}

#[test]
fn keeps_layer_names_per_layer_group() {
    // Jw_cad と同じく、レイヤの名前はレイヤグループごと。ほかのグループにある名前も付けられ、container のレイヤ
    // グループがあれば、その中で探す（なければその中に足す）
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "add-layer", "name": "通り芯", "group": 2},
            {"op": "add", "kind": "line", "container": "group:2", "layer": "通り芯", "start": [0, 0], "end": [100, 0]},
            {"op": "add", "kind": "line", "container": "group:3", "layer": "通り芯", "start": [0, 0], "end": [100, 0]},
            {"op": "update-layer", "name": "3-0", "set": {"name": "基準線"}},
            {"op": "update-layer", "name": "2-1", "set": {"name": "基準線"}}
        ]"##,
    )
    .unwrap();
    assert_eq!(edited.created[0], "2-0 通り芯");
    assert_eq!(codes(&edited), ["edit-added-layer"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let names = |group: usize| -> Vec<&str> { doc.groups[group].layers[..2].iter().map(|l| l.name.as_str()).collect() };
    assert_eq!(
        (doc.groups[0].layers[0].name.as_str(), names(2), names(3)),
        ("通り芯", vec!["通り芯", "基準線"], vec!["基準線", "3-1"])
    );
    let lines: Vec<(u16, u16)> = doc.entities[6..]
        .iter()
        .map(|entity| match entity {
            JwwEntity::Line(line) => (line.base.layer_group, line.base.layer),
            _ => panic!("line"),
        })
        .collect();
    assert_eq!(lines, [(2, 0), (3, 0)]);
    // 同じグループの中では重ならず、グループを決めない名前は 2 つあれば番号で選ぶ
    let two = r##"{"op": "add-layer", "name": "通り芯", "group": 2}"##;
    let error = |operation: &str| edit(&fixture(700), &format!("[{two}, {operation}]")).unwrap_err();
    assert_eq!(error(two).code, "already-exists");
    assert_eq!(error(r##"{"op": "update-layer", "name": "2-1", "set": {"name": "通り芯"}}"##).code, "already-exists");
    let ambiguous = error(r##"{"op": "add", "kind": "line", "layer": "通り芯", "start": [0, 0], "end": [1, 0]}"##);
    assert_eq!(ambiguous.code, "invalid-value");
}

#[test]
fn explains_what_cannot_be_edited() {
    let bytes = fixture(700);
    let error = |operations: &str| edit(&bytes, operations).unwrap_err();
    let cases = [
        (r##"[{"op": "update", "id": "#5>#1", "set": {"color": 3}}]"##, "inside-block"),
        (r##"[{"op": "delete", "id": "#99"}]"##, "not-found"),
        (r##"[{"op": "delete", "id": "3F"}]"##, "not-found"),
        (r##"[{"op": "add", "kind": "polyline", "vertices": [[0, 0], [1, 1]]}]"##, "unsupported-kind"),
        (r##"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "majorAxis": [1, 0]}]"##, "unknown-key"),
        (
            r##"[{"op": "add", "kind": "text", "position": [0, 0], "text": "a", "height": 3, "lineType": 2}]"##,
            "unknown-key",
        ),
        (r##"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "color": 10}]"##, "invalid-value"),
        (r##"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "color": "bylayer"}]"##, "invalid-value"),
        (
            r##"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "lineType": "dashed"}]"##,
            "missing-definition",
        ),
        (r##"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "lineType": 10}]"##, "invalid-value"),
        (r##"[{"op": "add", "kind": "block-reference", "block": "なし", "position": [0, 0]}]"##, "missing-definition"),
        (r##"[{"op": "add", "kind": "text", "position": [0, 0], "text": "1\n2", "height": 3}]"##, "invalid-value"),
        (
            r##"[{"op": "add", "kind": "line", "container": "layout:Layout1", "start": [0, 0], "end": [1, 0]}]"##,
            "invalid-value",
        ),
        (
            r##"[{"op": "add", "kind": "line", "container": "group:3", "layer": "0-1", "start": [0, 0], "end": [1, 0]}]"##,
            "invalid-value",
        ),
        (r##"[{"op": "update", "id": "#4", "set": {"majorAxis": [1, 0]}}]"##, "unknown-key"),
    ];
    for (operations, code) in cases {
        assert_eq!(error(operations).code, code, "{operations}");
    }
    // 1 つでも通らなければ何も書かない（操作の番号を返す）
    let failure =
        error(r##"[{"op": "delete", "id": "#1"}, {"op": "add", "kind": "polyline", "vertices": [[0, 0], [1, 1]]}]"##);
    assert_eq!(failure.operation, Some(1));
    assert_eq!(edit_jww(b"not a drawing", &[]).unwrap_err().code, "unreadable");
}

#[test]
fn writes_new_text_in_shift_jis_in_older_versions() {
    let edited =
        edit(&fixture(600), r##"[{"op": "add", "kind": "text", "position": [0, 0], "text": "𠮷野家", "height": 3}]"##)
            .unwrap();
    assert_eq!(codes(&edited), ["edit-text-encoding"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let Some(JwwEntity::Text(text)) = doc.entities.last() else { panic!("text") };
    assert_eq!(text.content, "?野家");
    // 版 420 より前の色の指定は、最も近い線色（SXF 対応色がない）。ここでは版 600 なので SXF 対応色
    let edited = edit(&fixture(600), r##"[{"op": "update", "id": "#1", "set": {"color": "#ff0000"}}]"##).unwrap();
    let doc = read_jww(&edited.bytes).unwrap();
    assert_eq!(doc.entities[0].base().pen_color, 117);
}

#[test]
fn picks_the_nearest_pen_color_without_sxf_colors() {
    // SXF 対応色の欄のない図面（版 420 より前）は、最も近い線色 1〜9
    let mut doc = JwwDocument::new();
    doc.version = 351;
    doc.entities.push(JwwEntity::Line(JwwLine {
        base: JwwBase { pen_style: 1, pen_color: 2, ..JwwBase::default() },
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
    }));
    let bytes = write_jww(&doc).bytes;
    let edited = edit(&bytes, r##"[{"op": "update", "id": "#1", "set": {"color": "#0000f0"}}]"##).unwrap();
    assert_eq!(codes(&edited), ["edit-nearest-color"]);
    // 線色 6 の画面色は青（#0000ff）
    assert_eq!(read_jww(&edited.bytes).unwrap().entities[0].base().pen_color, 6);
}

#[test]
fn changes_the_layer_and_color_of_every_part_of_solids_and_dimensions() {
    let mut doc = JwwDocument::new();
    doc.groups[0].scale = 100.0;
    let base = JwwBase { pen_style: 101, pen_color: 2, ..JwwBase::default() };
    doc.entities.push(JwwEntity::CircleSolid(JwwCircleSolid {
        base,
        cx: 0.0,
        cy: 0.0,
        radius: 5.0,
        flatness: 1.0,
        tilt: 0.0,
        start: 0.0,
        sweep: 2.0 * PI,
        mode: 100.0,
        color: None,
    }));
    let bytes = write_jww(&doc).bytes;
    let edited = edit(&bytes, r##"[{"op": "update", "id": "#1", "set": {"layer": "0-4", "color": 7}}]"##).unwrap();
    let read = read_jww(&edited.bytes).unwrap();
    let JwwEntity::CircleSolid(solid) = &read.entities[0] else { panic!("circle solid") };
    assert_eq!((solid.base.layer, solid.base.pen_color, solid.base.pen_style), (4, 7, 101));
    let error = edit(&bytes, r##"[{"op": "update", "id": "#1", "set": {"lineType": 2}}]"##).unwrap_err();
    assert_eq!(error.code, "unknown-key");
    // ソリッドの #rrggbb は任意色（線色 10）
    let edited = edit(&bytes, r##"[{"op": "update", "id": "#1", "set": {"color": "#123456"}}]"##).unwrap();
    let read = read_jww(&edited.bytes).unwrap();
    let JwwEntity::CircleSolid(solid) = &read.entities[0] else { panic!("circle solid") };
    assert_eq!((solid.base.pen_color, solid.color), (10, Some(0x563412)));
}

#[test]
fn creates_drawings_from_the_empty_one() {
    let edited = edit(
        &new_jww(),
        r##"[
            {"op": "add-layer", "name": "外構", "group": 0, "scale": 100},
            {"op": "add", "kind": "line", "layer": "外構", "start": [0, 0], "end": [10000, 0]},
            {"op": "add", "kind": "text", "layer": "外構", "position": [500, 500], "text": "敷地", "height": 5}
        ]"##,
    )
    .unwrap();
    assert_eq!(edited.created, ["0-0 外構", "#1", "#2"]);
    let doc = read_jww(&edited.bytes).unwrap();
    assert_eq!((doc.version, doc.groups[0].scale, doc.groups[0].layers[0].name.as_str()), (700, 100.0, "外構"));
    let scene = jww_to_scene(&doc);
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert_eq!(ids, ["#1", "#2"]);
    assert_eq!(raw(&scene, "#1", "end"), serde_json::json!([10000.0, 0.0]));
}

#[test]
fn writes_line_widths_in_the_unit_of_the_drawing() {
    // fixtures は線幅を 1/100 mm 単位とする図面（線描画の最大幅が負）
    let edited = edit(&fixture(700), r##"[{"op": "update", "id": "#1", "set": {"lineWidth": 0.25}}]"##).unwrap();
    assert!(edited.notices.is_empty(), "{:?}", edited.notices);
    assert_eq!(read_jww(&edited.bytes).unwrap().entities[0].base().pen_width, 25);
    // 新しい図面は印刷のドット（300 dpi）。0.25 mm は 2.95 ドットなので 3 ドットにして知らせる
    let mut doc = JwwDocument::new();
    doc.entities.push(JwwEntity::Line(JwwLine {
        base: JwwBase { pen_style: 1, pen_color: 2, ..JwwBase::default() },
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 0.0,
    }));
    let edited =
        edit(&write_jww(&doc).bytes, r##"[{"op": "update", "id": "#1", "set": {"lineWidth": 0.25}}]"##).unwrap();
    assert_eq!(codes(&edited), ["edit-rounded-lineweight"]);
    assert_eq!(read_jww(&edited.bytes).unwrap().entities[0].base().pen_width, 3);
    let error =
        edit(&fixture(700), r##"[{"op": "update", "id": "#1", "set": {"lineWidth": "bylayer"}}]"##).unwrap_err();
    assert_eq!(error.code, "invalid-value");
}

#[test]
fn turns_arcs_and_moves_text_with_its_end() {
    let edited = edit(
        &fixture(700),
        r##"[
            {"op": "update", "id": "#4", "set": {"angle": [90, 180], "center": [100, 100]}},
            {"op": "update", "id": "#3", "set": {"position": [1000, 2000]}}
        ]"##,
    )
    .unwrap();
    let doc = read_jww(&edited.bytes).unwrap();
    let JwwEntity::Arc(arc) = &doc.entities[3] else { panic!("arc") };
    assert_eq!((arc.cx, arc.cy), (1.0, 1.0));
    assert!((arc.start - PI / 2.0).abs() < 1e-12 && (arc.sweep - PI / 2.0).abs() < 1e-12);
    // 文字を動かすと、終点も同じだけ動く（文字の幅は変えない）
    let JwwEntity::Text(text) = &doc.entities[2] else { panic!("text") };
    assert_eq!((text.x1, text.y1, text.x2, text.y2), (10.0, 20.0, 30.0, 20.0));
}

#[test]
fn writes_new_strings_in_the_encoding_of_the_file() {
    // Jw_cad 9 までの版 700 の図面は、文字列を Shift_JIS で書く（版 600 の fixtures の版を 700 にしたもの）
    let mut bytes = fixture(600);
    bytes[8..12].copy_from_slice(&700u32.to_le_bytes());
    assert_eq!(edit_jww(&bytes, &[]).unwrap().bytes, bytes);
    let edited =
        edit(&bytes, r##"[{"op": "add", "kind": "text", "position": [0, 0], "text": "追加", "height": 3}]"##).unwrap();
    let shift_jis = [4u8, 0x92, 0xc7, 0x89, 0xc1];
    assert!(edited.bytes.windows(5).any(|window| window == shift_jis), "the new text is not in Shift_JIS");
    let Some(JwwEntity::Text(text)) = read_jww(&edited.bytes).unwrap().entities.last().cloned() else { panic!("text") };
    assert_eq!(text.content, "追加");
    // Jw_cad 10 の版 700 の図面（UTF-16LE）には UTF-16LE で書く
    let edited =
        edit(&fixture(700), r##"[{"op": "add", "kind": "text", "position": [0, 0], "text": "追加", "height": 3}]"##)
            .unwrap();
    let utf16 = [0xff, 0xfe, 0xff, 2, 0xfd, 0x8f, 0xa0, 0x52];
    assert!(edited.bytes.windows(8).any(|window| window == utf16));
}

#[test]
fn a_hidden_layer_can_be_shown_and_locked_together() {
    let edited =
        edit(&fixture(700), r#"[{"op":"update-layer","name":"0-2","set":{"visible":true,"locked":true}}]"#).unwrap();
    assert_eq!(read_jww(&edited.bytes).unwrap().groups[0].layers[2].state, 1);
}

#[test]
fn nonfinite_scalar_block_scales_are_refused() {
    for scale in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let operation = EditOperation::Update {
            id: "#5".into(),
            set: EditValues::from([("scale".into(), EditValue::Number(scale))]),
        };
        let Err(error) = edit_jww(&fixture(700), &[operation]) else { panic!("invalid scale was accepted") };
        assert_eq!(error.code, "invalid-value");
        assert_eq!(error.operation, Some(0));
    }
}

/// ブロック定義を足して中に図形を足し、配置する（docs/decisions/0017、0025）。中の図形の座標から基点を引き、
/// 配置は基点を配置の点に置く
#[test]
fn adds_block_definitions_and_draws_them_through_references() {
    for version in [200, 600, 700] {
        let edited = edit(
            &fixture(version),
            r##"[
                {"op": "add-block", "name": "扉", "basePoint": [1000, 0]},
                {"op": "add", "kind": "line", "container": "block:扉", "start": [1000, 0], "end": [1900, 0]},
                {"op": "add", "kind": "arc", "container": "block:扉", "center": [1000, 0], "radius": 900, "angle": [0, 90]},
                {"op": "add", "kind": "block-reference", "block": "扉", "position": [5000, 5000], "rotation": 90},
                {"op": "add", "kind": "block-reference", "container": "block:扉", "block": "部品", "position": [1000, 0]}
            ]"##,
        )
        .unwrap();
        assert_eq!(edited.created, ["扉", "block:扉>#1", "block:扉>#2", "#7", "block:扉>#3"], "{version}");
        assert!(edited.notices.is_empty(), "{version}: {:?}", edited.notices);
        let doc = read_jww(&edited.bytes).unwrap();
        let block = doc.blocks.last().unwrap();
        // Jw_cad と同じく、版 4.10 からは作図部品の印を名前の後ろに付ける
        let name = if version >= 410 { "扉@@SfigorgFlag@@4" } else { "扉" };
        assert_eq!((block.name.as_str(), block.number, block.entities.len()), (name, 8, 3), "{version}");
        let JwwEntity::Line(line) = &block.entities[0] else { panic!("line") };
        assert_eq!((line.x1, line.y1, line.x2, line.y2), (0.0, 0.0, 9.0, 0.0));
        let scene = jww_to_scene(&doc);
        let inside: Vec<&str> =
            scene.entities.iter().map(|entity| entity.id.as_str()).filter(|id| id.starts_with("#7>")).collect();
        assert_eq!(inside, ["#7>#1", "#7>#2", "#7>#3>#1"], "{version}");
        // 基点は配置の点に、900 の線は 90° 回って上へ
        let end = raw(&scene, "#7>#1", "end");
        assert!((end[0].as_f64().unwrap() - 5000.0).abs() < 1e-9 && (end[1].as_f64().unwrap() - 5900.0).abs() < 1e-9);
        let read = crate::JwwFile::read(&edited.bytes).unwrap();
        assert!(read.blocks.last().unwrap().referenced && read.blocks[0].referenced);
    }
    let bytes = fixture(700);
    let error = |operations: &str| edit(&bytes, operations).unwrap_err();
    assert_eq!(error(r##"[{"op": "add-block", "name": "部品"}]"##).code, "already-exists");
    assert_eq!(error(r##"[{"op": "add-block", "name": "a@@SfigorgFlag@@4"}]"##).code, "invalid-value");
    assert_eq!(error(r##"[{"op": "add-block", "name": "a", "origin": [0, 0]}]"##).code, "unknown-key");
    assert_eq!(
        error(r##"[{"op": "add", "kind": "line", "container": "block:なし", "start": [0, 0], "end": [1, 0]}]"##).code,
        "missing-definition"
    );
    // 配置の循環
    let cycle = error(
        r##"[{"op": "add-block", "name": "A"}, {"op": "add", "kind": "block-reference", "container": "block:A", "block": "A", "position": [0, 0]}]"##,
    );
    assert_eq!((cycle.code, cycle.operation), ("invalid-value", Some(1)));
    let cycle = error(
        r##"[{"op": "add-block", "name": "A"},
            {"op": "add", "kind": "block-reference", "container": "block:A", "block": "部品", "position": [0, 0]},
            {"op": "add", "kind": "block-reference", "container": "block:部品", "block": "A", "position": [0, 0]}]"##,
    );
    assert_eq!((cycle.code, cycle.operation), ("invalid-value", Some(2)));
}

/// 新しい 1/100 の図面に寸法を足す
fn dimensions(operations: &str) -> (Edited, JwwDocument) {
    let all = format!(r#"[{{"op": "add-layer", "name": "寸法", "group": 0, "scale": 100}}, {}]"#, operations);
    let edited = edit(&new_jww(), &all).unwrap();
    let doc = read_jww(&edited.bytes).unwrap();
    (edited, doc)
}

/// 寸法は寸法図形（CDataSunpou）。大きさと寸法値は図面の寸法の設定（ここは Jw_cad の既定: 文字種 2、離れ 0.5 mm、
/// 突出 0、小数点以下 2 桁、カンマ）に従う（docs/decisions/0025）
#[test]
fn adds_dimensions_as_dimension_figures() {
    let (edited, doc) = dimensions(
        r#"{"op": "add", "kind": "dimension", "layer": "寸法", "start": [0, 0], "end": [3700, 0], "position": [0, -800]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionType": "aligned", "start": [0, 0], "end": [0, 3000], "position": [-500, 0]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionType": "radius", "center": [0, 0], "point": [600, 0]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionType": "diameter", "center": [0, 0], "point": [0, 500], "color": 4, "text": "<>（内法）"}"#,
    );
    assert_eq!(edited.created, ["0-0 寸法", "#1", "#2", "#3", "#4"]);
    assert!(edited.notices.is_empty(), "{:?}", edited.notices);
    let JwwEntity::Dimension(linear) = &doc.entities[0] else { panic!("dimension") };
    assert_eq!(linear.sxf_mode, 1);
    assert_eq!((linear.line.x1, linear.line.y1, linear.line.x2, linear.line.y2), (0.0, -8.0, 37.0, -8.0));
    assert_eq!((linear.lines[0].x1, linear.lines[0].y1, linear.lines[0].x2, linear.lines[0].y2), (0.0, 0.0, 0.0, -8.0));
    let points: Vec<(f64, f64, bool)> = linear.points.iter().map(|point| (point.x, point.y, point.temporary)).collect();
    assert_eq!(points, [(0.0, -8.0, false), (37.0, -8.0, false), (0.0, 0.0, true), (37.0, 0.0, true)]);
    let text = &linear.text;
    assert_eq!((text.content.as_str(), text.text_type, text.size_y, text.angle), ("3,700", 2, 2.5, 0.0));
    assert_eq!((text.base.flag, text.base.pen_style, text.base.pen_width & 0x3042), (0x4010, 2, 0x2042));
    // 寸法線の中央に、5 文字（半角）× 1.25 mm の幅で、離れ 0.5 mm の上
    assert_eq!((text.x1, text.y1, text.x2), (18.5 - 3.125, -7.5, 18.5 + 3.125));
    assert_eq!((linear.base.flag, linear.line.base.flag, linear.points[0].base.flag), (0x2000, 0x2000, 0x0040));
    // 縦の寸法の値は下から上へ読み、寸法線の左に置く
    let JwwEntity::Dimension(aligned) = &doc.entities[1] else { panic!("dimension") };
    assert_eq!((aligned.text.angle, aligned.text.content.as_str()), (90.0, "3,000"));
    assert!((aligned.text.x1 + 5.5).abs() < 1e-12 && (aligned.text.y1 - (15.0 - 3.125)).abs() < 1e-12);
    let JwwEntity::Dimension(radius) = &doc.entities[2] else { panic!("dimension") };
    assert_eq!((radius.text.content.as_str(), radius.text.base.flag), ("R600", 0x4110));
    assert_eq!((radius.points[0].temporary, radius.points[1].temporary, radius.points[1].x), (true, false, 6.0));
    let JwwEntity::Dimension(diameter) = &doc.entities[3] else { panic!("dimension") };
    assert_eq!((diameter.text.content.as_str(), diameter.text.base.flag), ("φ1,000（内法）", 0x4210));
    assert_eq!((diameter.line.y1, diameter.line.y2, diameter.points[0].temporary), (-5.0, 5.0, false));
    let colors: Vec<u16> = [&diameter.line.base, &diameter.text.base, &diameter.points[1].base]
        .iter()
        .map(|base| base.pen_color)
        .collect();
    assert_eq!(colors, [4, 4, 4]);
    let scene = jww_to_scene(&doc);
    assert!(scene.entities.iter().all(|entity| entity.kind.name() == "dimension" && !entity.primitives.is_empty()));
    assert_eq!(property(&scene, "#1", "dimensionText").value, "3,700");
}

#[test]
fn draws_dimensions_with_the_settings_of_the_drawing() {
    let bytes = {
        let mut file = crate::JwwFile::read(&fixture(700)).unwrap();
        // 寸法線の色 3、小数点以下 0 桁、端部は矢印、文字種 3、離れ 1 mm、突出 2 mm
        file.header.settings.as_mut().unwrap().general.dimension = [3_100_333, 200_010, 50_150_030, 11_000_000, 4_100];
        file.write().bytes
    };
    let edited = edit(
        &bytes,
        r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [1234.5, 0], "position": [0, 500], "dimensionStyle": "Standard"}]"#,
    )
    .unwrap();
    assert_eq!(codes(&edited), ["edit-dimension-style", "edit-dimension-end"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let JwwEntity::Dimension(dimension) = doc.entities.last().unwrap() else { panic!("dimension") };
    // 1/100 のグループ 0 に。寸法線は y = 5 mm、補助線は起点から寸法線を 2 mm 越える
    assert_eq!((dimension.line.base.pen_color, dimension.text.text_type, dimension.text.size_y), (3, 3, 3.0));
    assert_eq!(dimension.text.content, "1,235");
    assert_eq!((dimension.lines[0].y1, dimension.lines[0].y2, dimension.text.y1), (0.0, 7.0, 6.0));
    // 角度寸法は寸法図形にできない
    let error = edit(
        &bytes,
        r#"[{"op": "add", "kind": "dimension", "dimensionType": "angular-3-point", "center": [0, 0], "start": [1, 0], "end": [0, 1], "position": [1, 1]}]"#,
    )
    .unwrap_err();
    assert_eq!(error.code, "unsupported-kind");
    let error =
        edit(&bytes, r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [0, 0], "position": [0, 5]}]"#)
            .unwrap_err();
    assert_eq!(error.code, "invalid-value");
    // 版 420 より前の寸法図形は補助線と点を持たない
    let edited = edit(
        &fixture(200),
        r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [1000, 0], "position": [0, -500]}]"#,
    )
    .unwrap();
    assert_eq!(codes(&edited), ["edit-dimension-parts"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let JwwEntity::Dimension(dimension) = doc.entities.last().unwrap() else { panic!("dimension") };
    assert!(dimension.lines.is_empty() && dimension.text.content == "1,000");
}

/// この編集で足した寸法スタイルの値で寸法図形を描く（docs/decisions/0026）。尺度のないスタイルの長さは用紙の mm、
/// 尺度のあるスタイルは尺度を掛けた実寸を、レイヤグループの縮尺（ここは 1/100）で用紙の mm にする
#[test]
fn draws_dimensions_with_styles_added_in_the_edit() {
    let (edited, doc) = dimensions(
        r#"{"op": "add-text-style", "name": "明朝", "font": "ＭＳ 明朝"},
           {"op": "add-dimension-style", "name": "DIM-100", "scale": 100, "textHeight": 3.5, "extensionOffset": 1, "extensionExtension": 2, "textGap": 1, "decimals": 1, "suppressTrailingZeros": false, "textStyle": "明朝"},
           {"op": "add-dimension-style", "name": "DIM-50", "from": "dim-100", "scale": 50},
           {"op": "add-dimension-style", "name": "中央", "textHeight": 3, "textAbove": false, "linearFactor": 0.001},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionStyle": "dim-100", "start": [0, 0], "end": [3700, 0], "position": [0, -800]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionStyle": "DIM-50", "start": [0, 0], "end": [3700, 0], "position": [0, -800]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionStyle": "中央", "dimensionType": "radius", "center": [0, 0], "point": [600, 0]},
           {"op": "add", "kind": "text", "layer": "寸法", "position": [0, 0], "text": "注", "height": 3, "textStyle": "明朝"}"#,
    );
    assert_eq!(edited.created, ["0-0 寸法", "明朝", "DIM-100", "DIM-50", "中央", "#1", "#2", "#3", "#4"]);
    assert_eq!(codes(&edited), ["edit-dimension-text-position"]);
    let JwwEntity::Dimension(scaled) = &doc.entities[0] else { panic!("dimension") };
    // 尺度 100 の 1/100 のグループなので、長さは用紙の mm のまま。文字種 2（2.5 mm 角）の比で 3.5 mm 角、任意サイズ
    let text = &scaled.text;
    assert_eq!((text.size_x, text.size_y, text.spacing, text.text_type), (3.5, 3.5, 0.0, 0));
    assert_eq!((text.content.as_str(), text.font.as_str()), ("3,700.0", "ＭＳ 明朝"));
    // 小数点以下 1 桁（0x1000）、後ろの 0 を残す（0x0080）、カンマ（0x0040）
    assert_eq!(text.base.pen_width & 0x30c2, 0x10c2);
    assert_eq!(text.y1, -8.0 + 1.0);
    // 補助線は起点から 1 mm 離して始め、寸法線を 2 mm 越える
    let extension = &scaled.lines[0];
    assert_eq!((extension.x1, extension.y1, extension.x2, extension.y2), (0.0, -1.0, 0.0, -10.0));
    assert_eq!((scaled.points[2].x, scaled.points[2].y, scaled.points[2].temporary), (0.0, 0.0, true));
    // 尺度 50 は 1/100 のグループで半分の大きさ
    let JwwEntity::Dimension(half) = &doc.entities[1] else { panic!("dimension") };
    assert_eq!((half.text.size_y, half.text.y1, half.lines[0].y2), (1.75, -7.5, -9.0));
    // 寸法線の上に置かない寸法値は、寸法線に高さの中央を重ねる。倍率 0.001 で 0.6（小数点以下 2 桁、後ろの 0 は省く）
    let JwwEntity::Dimension(centered) = &doc.entities[2] else { panic!("dimension") };
    assert_eq!((centered.text.content.as_str(), centered.text.size_y, centered.text.y1), ("R0.6", 3.0, -1.5));
    let JwwEntity::Text(note) = &doc.entities[3] else { panic!("text") };
    assert_eq!(note.font, "ＭＳ 明朝");
    let scene = jww_to_scene(&doc);
    assert!(scene.entities.iter().all(|entity| !entity.primitives.is_empty()));
    assert_eq!(property(&scene, "#1", "dimensionText").value, "3,700.0");
}

#[test]
fn reports_what_dimension_styles_cannot_be_in_jww() {
    let (edited, doc) = dimensions(
        r#"{"op": "add-dimension-style", "name": "A", "from": "Standard", "arrowSize": 2.5, "tickSize": 1, "decimals": 5},
           {"op": "add-text-style", "name": "JP", "font": "MS Gothic", "widthFactor": 0.8},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionStyle": "A", "start": [0, 0], "end": [1234.56789, 0], "position": [0, -800]},
           {"op": "add", "kind": "dimension", "layer": "寸法", "dimensionStyle": "なし", "start": [0, 0], "end": [1000, 0], "position": [0, -800]}"#,
    );
    assert_eq!(
        codes(&edited),
        ["edit-dimension-style", "edit-dimension-decimals", "edit-dimension-end", "edit-style-values"]
    );
    // 小数点以下は寸法値設定のフラグに書ける 3 桁まで
    let JwwEntity::Dimension(dimension) = &doc.entities[0] else { panic!("dimension") };
    assert_eq!((dimension.text.content.as_str(), dimension.text.base.pen_width & 0x3000), ("1,234.568", 0x3000));
    let JwwEntity::Dimension(plain) = &doc.entities[1] else { panic!("dimension") };
    assert_eq!((plain.text.content.as_str(), plain.text.text_type), ("1,000", 2));
    let bytes = new_jww();
    let error = |operations: &str| edit(&bytes, operations).unwrap_err();
    let cases = [
        (
            r#"[{"op": "add-dimension-style", "name": "A"}, {"op": "add-dimension-style", "name": "a"}]"#,
            "already-exists",
        ),
        (r#"[{"op": "add-dimension-style", "name": "A", "textHeight": 0}]"#, "invalid-value"),
        (r#"[{"op": "add-dimension-style", "name": "A", "dimscale": 1}]"#, "unknown-key"),
        (r#"[{"op": "add-text-style", "name": " "}]"#, "invalid-value"),
        (
            r#"[{"op": "add", "kind": "text", "position": [0, 0], "text": "a", "height": 3, "font": "a", "textStyle": "b"}]"#,
            "invalid-value",
        ),
    ];
    for (operations, code) in cases {
        assert_eq!(error(operations).code, code, "{operations}");
    }
}

/// 線種は SXF 対応線種のユーザー定義（47〜62）の空いた欄に、名前と用紙の mm の長さで足す（docs/decisions/0026）
#[test]
fn adds_line_types_to_the_free_sxf_line_types() {
    let edited = edit(
        &new_jww(),
        r#"[
            {"op": "add-line-type", "name": "破線2", "description": "__ __", "pattern": [6, -3]},
            {"op": "add-line-type", "name": "一点鎖線2", "pattern": [12, -1.5, 0, -1.5]},
            {"op": "add-line-type", "name": "実線2", "pattern": []},
            {"op": "add", "kind": "line", "lineType": "破線2", "start": [0, 0], "end": [100, 0]},
            {"op": "add", "kind": "line", "lineType": "一点鎖線2", "start": [0, 10], "end": [100, 10]},
            {"op": "add", "kind": "line", "lineType": "実線2", "start": [0, 20], "end": [100, 20]}
        ]"#,
    )
    .unwrap();
    assert_eq!(edited.created, ["破線2", "一点鎖線2", "実線2", "#1", "#2", "#3"]);
    assert_eq!(codes(&edited), ["edit-style-values", "edit-line-type-pattern"]);
    let doc = read_jww(&edited.bytes).unwrap();
    let styles: Vec<u8> = doc.entities.iter().map(|entity| entity.base().pen_style).collect();
    assert_eq!(styles, [47, 48, 1]);
    // 点は 0.25 mm の線にし、続く空白をその分だけ短くする
    assert_eq!(doc.line_types[&47], [6.0, 3.0]);
    assert_eq!(doc.line_types[&48], [12.0, 1.5, 0.25, 1.25]);
    let file = crate::JwwFile::read(&edited.bytes).unwrap();
    let sxf = file.header.settings.as_ref().unwrap().sxf.as_ref().unwrap();
    assert_eq!((sxf.user_line_types[17].name.as_str(), sxf.user_line_types[17].segments), ("破線2", 2));
    // 図形が使う欄と名前のある欄は使わない
    let error = |bytes: &[u8], operations: &str| edit(bytes, operations).unwrap_err();
    assert_eq!(error(&new_jww(), r#"[{"op": "add-line-type", "name": "Dashed"}]"#).code, "already-exists");
    assert_eq!(error(&new_jww(), r#"[{"op": "add-line-type", "name": "47"}]"#).code, "invalid-value");
    assert_eq!(
        error(&fixture(200), r#"[{"op": "add-line-type", "name": "a", "pattern": [1, -1]}]"#).code,
        "unsupported-kind"
    );
    let all: Vec<String> =
        (0..17).map(|n| format!(r#"{{"op": "add-line-type", "name": "線種{n}", "pattern": [1, -1]}}"#)).collect();
    let exhausted = error(&new_jww(), &format!("[{}]", all.join(",")));
    assert_eq!((exhausted.code, exhausted.operation), ("name-exhausted", Some(16)));
    let used = {
        let mut file = crate::JwwFile::read(&new_jww()).unwrap();
        file.entities.push(crate::JwwRecord::new(JwwEntity::Line(JwwLine {
            base: JwwBase { pen_style: 47, pen_color: 2, ..JwwBase::default() },
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 0.0,
        })));
        file.write().bytes
    };
    let edited = edit(&used, r#"[{"op": "add-line-type", "name": "a", "pattern": [1, -1]}]"#).unwrap();
    let file = crate::JwwFile::read(&edited.bytes).unwrap();
    let sxf = file.header.settings.as_ref().unwrap().sxf.as_ref().unwrap();
    assert_eq!((sxf.user_line_types[17].name.as_str(), sxf.user_line_types[18].name.as_str()), ("", "a"));
}

/// JWW にハッチングの図形はないので、塗りは台形のソリッド、模様はハッチの線と点（docs/decisions/0025）
#[test]
fn adds_hatches_as_solids_and_hatch_lines() {
    let (edited, doc) = dimensions(
        r##"{"op": "add", "kind": "hatch", "layer": "寸法", "boundaries": [[0, 0], [1000, 0], [1000, 1000], [0, 1000]], "color": "#ff0000"},
            {"op": "add", "kind": "hatch", "layer": "寸法", "boundaries": [[[0, 2000], [3000, 2000], [3000, 5000], [0, 5000]], [[1000, 3000], [2000, 3000], [2000, 4000], [1000, 4000]]], "color": 3},
            {"op": "add", "kind": "hatch", "layer": "寸法", "pattern": "ANSI31", "patternScale": 100, "boundaries": [[5000, 0], [8000, 0], [8000, 3000], [5000, 3000]]},
            {"op": "add", "kind": "line", "layer": "寸法", "start": [0, 0], "end": [1, 0]}"##,
    );
    let solids = doc.entities.iter().filter(|entity| matches!(entity, JwwEntity::Solid(_))).count();
    let lines: Vec<&JwwLine> = doc
        .entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Line(line) if line.base.flag == HATCH_FLAG => Some(line),
            _ => None,
        })
        .collect();
    // 1 つ目は 1 つのソリッド、2 つ目は穴の上下と左右の 4 つ
    assert_eq!(solids, 1 + 4);
    assert_eq!(edited.created[1..4], ["#1", "#2", "#6"]);
    assert_eq!(edited.created[4], format!("#{}", 6 + lines.len()));
    assert_eq!(codes(&edited), ["edit-fill-solids", "edit-hatch-lines"]);
    let JwwEntity::Solid(solid) = &doc.entities[0] else { panic!("solid") };
    assert_eq!(
        (solid.points, solid.base.pen_color, solid.color),
        ([0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0], 10, Some(0xff))
    );
    // 30 mm 角に 45° で 3.175 mm 間隔: 13 本ほど
    assert!((12..=14).contains(&lines.len()), "{}", lines.len());
    assert!(lines.iter().all(|line| [line.x1, line.x2].iter().all(|x| (50.0 - 1e-9..=80.0 + 1e-9).contains(x))));
    let scene = jww_to_scene(&doc);
    assert_eq!(scene.entities.len(), doc.entities.len());
    let bytes = new_jww();
    let error = |operations: &str| edit(&bytes, operations).unwrap_err();
    assert_eq!(
        error(r#"[{"op": "add", "kind": "hatch", "boundaries": [[0, 0], [1, 0], [2, 0]]}]"#).code,
        "invalid-value"
    );
    assert_eq!(
        error(r#"[{"op": "add", "kind": "hatch", "pattern": "LINE", "patternScale": 0.001, "boundaries": [[0, 0], [1000, 0], [1000, 1000]]}]"#)
            .code,
        "invalid-value"
    );
    assert_eq!(
        error(r#"[{"op": "add", "kind": "hatch", "pattern": "BRICK", "boundaries": [[0, 0], [1, 0], [1, 1]]}]"#).code,
        "missing-definition"
    );
}
