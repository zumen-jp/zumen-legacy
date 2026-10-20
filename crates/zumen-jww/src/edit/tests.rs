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
