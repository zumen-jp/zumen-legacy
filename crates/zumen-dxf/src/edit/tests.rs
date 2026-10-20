use super::*;
use crate::{DxfEntityData, DxfSpace, parse_dxf, write_dxf};
use zumen_core::edit::parse_operations;

/// AutoCAD 2013 の DXF（zumen の書き出し）。ブロック DOOR と、属性つきの参照を持つ
fn drawing(entities: &str) -> Vec<u8> {
    let text = format!(
        "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLTYPE\n0\nLTYPE\n2\nDASHED\n70\n0\n3\n\n72\n65\n73\n2\n40\n10\n49\n5\n49\n-5\n0\nENDTAB\n\
         0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n0\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nLAYER\n2\nWALL\n70\n0\n62\n1\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n\
         0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\nDOOR\n70\n2\n10\n0\n20\n0\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n\
         0\nATTDEF\n8\n0\n10\n0\n20\n0\n40\n1\n1\n\n3\nNO\n2\nNO\n70\n0\n0\nENDBLK\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    );
    write_dxf(&parse_dxf(text.as_bytes())).bytes
}

/// 図面の ENTITIES の終わりに、書いたままのタグ（拡張データなど、モデルが持たない値を含む）を足す
fn with_raw(bytes: &[u8], raw: &str) -> Vec<u8> {
    let text = text(bytes);
    let end = text.rfind("ENDSEC\r\n  0\r\nSECTION\r\n  2\r\nOBJECTS").expect("ENTITIES");
    let at = text[..end].rfind("  0\r\n").expect("ENDSEC");
    format!("{}{raw}{}", &text[..at], &text[at..]).into_bytes()
}

fn edit(bytes: &[u8], operations: &str) -> Result<Edited, EditError> {
    edit_dxf(bytes, &parse_operations(operations).expect("operations"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// タグの並び（コード、値）
fn tags(bytes: &[u8]) -> Vec<(i32, String)> {
    let raw = RawDxf::parse(bytes);
    raw.tags.iter().map(|tag| (tag.code, tag.text().to_string())).collect()
}

/// ハンドルの図形のタグ
fn entity_tags(bytes: &[u8], handle: &str) -> Vec<(i32, String)> {
    let all = tags(bytes);
    let start = all
        .windows(2)
        .position(|pair| pair[0].0 == 0 && pair[1] == (5, handle.to_string()))
        .unwrap_or_else(|| panic!("no entity {handle}"));
    let end = all[start + 1..].iter().position(|tag| tag.0 == 0).map_or(all.len(), |offset| start + 1 + offset);
    all[start..end].to_vec()
}

fn handle_seed(bytes: &[u8]) -> u64 {
    let all = tags(bytes);
    let at = all.iter().position(|tag| *tag == (9, "$HANDSEED".to_string())).expect("$HANDSEED");
    u64::from_str_radix(&all[at + 1].1, 16).unwrap()
}

#[test]
fn keeps_every_byte_without_operations() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf");
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "dxf") {
            let bytes = std::fs::read(&path).unwrap();
            let edited = edit_dxf(&bytes, &[]).unwrap();
            assert!(edited.bytes == bytes, "{} changed", path.display());
        }
    }
}

#[test]
fn adds_entities_with_new_handles_in_model_space() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r##"[
            {"op": "add", "kind": "line", "layer": "WALL", "start": [0, 0], "end": [5000, 0], "color": "#ff0000"},
            {"op": "add", "kind": "circle", "center": [10, 20], "radius": 5, "lineWidth": 0.25},
            {"op": "add", "kind": "arc", "center": [0, 0], "radius": 2, "angle": [0, 90]},
            {"op": "add", "kind": "ellipse", "center": [0, 0], "majorAxis": [4, 0], "ratio": 0.5},
            {"op": "add", "kind": "point", "position": [1, 2, 3]},
            {"op": "add", "kind": "polyline", "vertices": [[0, 0], [10, 0], [10, 10]], "bulges": [0, 1, 0], "closed": true},
            {"op": "add", "kind": "text", "position": [0, -10], "text": "確認済み", "height": 3.5, "rotation": 90},
            {"op": "add", "kind": "text", "position": [0, -20], "text": "1 行目\n{2} 行目", "height": 2},
            {"op": "add", "kind": "block-reference", "block": "DOOR", "position": [100, 0], "scale": 2, "rotation": 45}
        ]"##,
    )
    .unwrap();
    assert_eq!(edited.created.len(), 9);
    let doc = parse_dxf(&edited.bytes);
    let model = doc.entities.iter().filter(|entity| entity.base.space == DxfSpace::Model).collect::<Vec<_>>();
    assert_eq!(
        model.iter().map(|entity| entity.base.kind.as_str()).collect::<Vec<_>>(),
        ["LINE", "CIRCLE", "ARC", "ELLIPSE", "POINT", "LWPOLYLINE", "TEXT", "MTEXT", "INSERT"]
    );
    assert_eq!(model.iter().map(|entity| entity.base.handle.clone()).collect::<Vec<_>>(), edited.created);
    let owner = model[0].base.owner.clone().expect("owner");
    assert!(model.iter().all(|entity| entity.base.owner.as_ref() == Some(&owner)));
    // 新しいハンドルは元の最大より後で、$HANDSEED はその次
    let before = handle_seed(&bytes);
    let handles: Vec<u64> = edited.created.iter().map(|handle| u64::from_str_radix(handle, 16).unwrap()).collect();
    assert_eq!(handles[0], before);
    assert_eq!(handle_seed(&edited.bytes), handles[8] + 1);
    let line = &model[0];
    assert_eq!(
        (line.base.layer.as_str(), line.base.color_index, line.base.true_color),
        ("WALL", 1.0, Some(f64::from(0xff0000)))
    );
    assert!(matches!(line.data, DxfEntityData::Line { end: [5000.0, 0.0, 0.0], .. }));
    assert_eq!(model[1].base.lineweight, 25.0);
    let DxfEntityData::LwPolyline(polyline) = &model[5].data else { panic!() };
    assert_eq!((polyline.vertices.points.len(), polyline.vertices.closed, polyline.vertices.bulges[1]), (3, true, 1.0));
    let DxfEntityData::Text(text) = &model[6].data else { panic!() };
    assert_eq!((text.text.as_str(), text.height, text.rotation), ("確認済み", 3.5, 90.0));
    let DxfEntityData::MText(mtext) = &model[7].data else { panic!() };
    assert_eq!(mtext.text, "1 行目\n{2} 行目");
    let DxfEntityData::Insert(insert) = &model[8].data else { panic!() };
    assert_eq!((insert.name.as_str(), insert.xscale, insert.rotation), ("DOOR", 2.0, 45.0));
}

#[test]
fn adds_to_layouts_and_block_definitions() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "line", "container": "layout:Layout1", "start": [0, 0], "end": [1, 1]},
            {"op": "add", "kind": "line", "container": "block:DOOR", "start": [0, 0], "end": [0, 1]}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let paper = doc.entities.iter().find(|entity| entity.base.handle == edited.created[0]).unwrap();
    assert_eq!(paper.base.space, DxfSpace::Paper);
    let inside = doc.entities.iter().find(|entity| entity.base.handle == edited.created[1]).unwrap();
    assert_eq!((inside.base.space, inside.base.block.as_deref()), (DxfSpace::Block, Some("DOOR")));
    let error =
        edit(&bytes, r#"[{"op": "add", "kind": "line", "container": "block:NONE", "start": [0, 0], "end": [1, 1]}]"#)
            .unwrap_err();
    assert_eq!((error.code, error.operation), ("missing-definition", Some(0)));
}

#[test]
fn updates_only_the_tags_of_the_values_set() {
    // 拡張データ（1001）は図形のタグとして残る
    let bytes = with_raw(&drawing(""), "0\nLINE\n5\nA0\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n1001\nAPP\n1000\nkeep\n");
    let handle = "A0";
    let edited = edit(
        &bytes,
        &format!(
            r#"[{{"op": "update", "id": "{handle}", "set": {{"end": [7, 8], "layer": "WALL", "color": 3, "lineType": "dashed"}}}}]"#
        ),
    )
    .unwrap();
    let tags = entity_tags(&edited.bytes, handle);
    assert!(tags.contains(&(11, "7.0".into())) && tags.contains(&(21, "8.0".into())), "{tags:?}");
    assert!(
        tags.contains(&(8, "WALL".into())) && tags.contains(&(62, "3".into())) && tags.contains(&(6, "DASHED".into()))
    );
    assert!(tags.contains(&(1000, "keep".into())), "{tags:?}");
    // 変えていない図形のタグは元のバイト列のまま
    assert_eq!(edited.bytes.len() - bytes.len(), text(&edited.bytes).len() - text(&bytes).len());
}

#[test]
fn moves_aligned_text_by_its_alignment_point() {
    let bytes = drawing("0\nTEXT\n8\n0\n10\n0\n20\n0\n40\n1\n1\nA\n72\n1\n11\n5\n21\n5\n");
    let doc = parse_dxf(&bytes);
    let handle = doc.entities.iter().find(|entity| entity.base.kind == "TEXT").unwrap().base.handle.clone();
    let edited =
        edit(&bytes, &format!(r#"[{{"op": "update", "id": "{handle}", "set": {{"position": [10, 5]}}}}]"#)).unwrap();
    let doc = parse_dxf(&edited.bytes);
    let DxfEntityData::Text(text) = &doc.entities.iter().find(|entity| entity.base.handle == handle).unwrap().data
    else {
        panic!()
    };
    assert_eq!((text.insert[0], text.align_point.map(|point| point[0])), (5.0, Some(10.0)));
}

#[test]
fn updates_polyline_vertices_and_block_references() {
    let bytes = drawing(
        "0\nLWPOLYLINE\n8\n0\n90\n2\n70\n0\n10\n0\n20\n0\n10\n1\n20\n0\n0\nINSERT\n8\n0\n2\nDOOR\n10\n0\n20\n0\n",
    );
    let doc = parse_dxf(&bytes);
    let polyline = doc.entities.iter().find(|entity| entity.base.kind == "LWPOLYLINE").unwrap().base.handle.clone();
    let insert = doc.entities.iter().find(|entity| entity.base.kind == "INSERT").unwrap().base.handle.clone();
    let edited = edit(
        &bytes,
        &format!(
            r#"[
                {{"op": "update", "id": "{polyline}", "set": {{"vertices": [[0, 0], [2, 0], [2, 2], [0, 2]], "closed": true}}}},
                {{"op": "update", "id": "{insert}", "set": {{"position": [3, 4], "scale": [2, 3], "rotation": 30}}}}
            ]"#
        ),
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let DxfEntityData::LwPolyline(lw) =
        &doc.entities.iter().find(|entity| entity.base.handle == polyline).unwrap().data
    else {
        panic!()
    };
    assert_eq!((lw.vertices.points.len(), lw.vertices.closed), (4, true));
    let DxfEntityData::Insert(reference) =
        &doc.entities.iter().find(|entity| entity.base.handle == insert).unwrap().data
    else {
        panic!()
    };
    assert_eq!(
        (reference.insert, reference.xscale, reference.yscale, reference.rotation),
        ([3.0, 4.0, 0.0], 2.0, 3.0, 30.0)
    );
}

#[test]
fn deletes_entities_with_what_follows_them() {
    let bytes = drawing(
        "0\nPOLYLINE\n8\n0\n66\n1\n70\n0\n0\nVERTEX\n8\n0\n10\n0\n20\n0\n0\nVERTEX\n8\n0\n10\n1\n20\n1\n0\nSEQEND\n8\n0\n\
         0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n",
    );
    let doc = parse_dxf(&bytes);
    let polyline = doc.entities.iter().find(|entity| entity.base.kind == "POLYLINE").unwrap().base.handle.clone();
    let edited = edit(&bytes, &format!(r#"[{{"op": "delete", "id": "{polyline}"}}]"#)).unwrap();
    let all = tags(&edited.bytes);
    assert!(!all.iter().any(|tag| tag.0 == 0 && matches!(tag.1.as_str(), "POLYLINE" | "VERTEX" | "SEQEND")), "{all:?}");
    let doc = parse_dxf(&edited.bytes);
    assert_eq!(doc.entities.iter().filter(|entity| entity.base.space == DxfSpace::Model).count(), 1);
    // 消した図形は選べない
    let error = edit(&edited.bytes, &format!(r#"[{{"op": "delete", "id": "{polyline}"}}]"#)).unwrap_err();
    assert_eq!(error.code, "not-found");
}

#[test]
fn removes_references_to_deleted_entities() {
    // SORTENTSTABLE の組（331・5）と、グループ（340）から除く
    let text = "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1027\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
                0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n5\n2\n70\n1\n0\nLAYER\n5\n10\n2\n0\n70\n0\n62\n7\n6\nCONTINUOUS\n0\nENDTAB\n0\nENDSEC\n\
                0\nSECTION\n2\nENTITIES\n0\nLINE\n5\n20\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n1\n21\n0\n31\n0\n\
                0\nLINE\n5\n21\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbLine\n10\n0\n20\n0\n30\n0\n11\n2\n21\n0\n31\n0\n0\nENDSEC\n\
                0\nSECTION\n2\nOBJECTS\n0\nSORTENTSTABLE\n5\n30\n330\n1F\n100\nAcDbSortentsTable\n330\n1F\n331\n20\n5\n50\n331\n21\n5\n51\n\
                0\nGROUP\n5\n31\n330\n1F\n100\nAcDbGroup\n340\n20\n340\n21\n0\nDICTIONARY\n5\n32\n330\n20\n100\nAcDbDictionary\n0\nENDSEC\n0\nEOF\n";
    let edited = edit(text.as_bytes(), r#"[{"op": "delete", "id": "20"}]"#).unwrap();
    let all = tags(&edited.bytes);
    assert!(!all.contains(&(331, "20".into())) && !all.contains(&(5, "50".into())));
    assert!(all.contains(&(331, "21".into())) && all.contains(&(5, "51".into())));
    assert!(!all.contains(&(340, "20".into())) && all.contains(&(340, "21".into())));
    // 図形が持つ辞書（所有者 330 が図形）も消す
    assert!(!all.contains(&(5, "32".into())), "{all:?}");
}

#[test]
fn adds_and_updates_layers() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r##"[
            {"op": "add-layer", "name": "CHECK", "color": "#00ff00", "lineType": "DASHED", "lineWidth": 0.5},
            {"op": "add", "kind": "line", "layer": "CHECK", "start": [0, 0], "end": [1, 0]},
            {"op": "update-layer", "name": "WALL", "set": {"visible": false, "frozen": true, "locked": true}},
            {"op": "add", "kind": "line", "layer": "NEW", "start": [0, 0], "end": [1, 0]}
        ]"##,
    )
    .unwrap();
    assert_eq!(edited.created[0], "CHECK");
    let doc = parse_dxf(&edited.bytes);
    let check = doc.layers.iter().find(|layer| layer.name == "CHECK").unwrap();
    assert_eq!(
        (check.true_color, check.linetype.as_str(), check.lineweight),
        (Some(f64::from(0x00ff00)), "DASHED", 50.0)
    );
    let wall = doc.layers.iter().find(|layer| layer.name == "WALL").unwrap();
    assert_eq!((wall.off, wall.frozen, wall.locked, wall.color_index), (true, true, true, 1.0));
    // ないレイヤは既定の値で足して知らせる
    assert!(doc.layers.iter().any(|layer| layer.name == "NEW"));
    assert!(edited.notices.iter().any(|notice| notice.code == "edit-added-layer"));
    // レイヤのハンドルも重ならない
    let all = tags(&edited.bytes);
    let mut handles: Vec<&str> = all.iter().filter(|tag| tag.0 == 5).map(|tag| tag.1.as_str()).collect();
    let count = handles.len();
    handles.sort_unstable();
    handles.dedup();
    assert_eq!(handles.len(), count);
}

#[test]
fn explains_what_cannot_be_edited() {
    let bytes = drawing("0\nINSERT\n8\n0\n2\nDOOR\n10\n0\n20\n0\n");
    let doc = parse_dxf(&bytes);
    let insert = doc.entities.iter().find(|entity| entity.base.kind == "INSERT").unwrap().base.handle.clone();
    let child = doc.entities.iter().find(|entity| entity.base.kind == "LINE").unwrap().base.handle.clone();
    let cases = [
        (format!(r#"[{{"op": "update", "id": "{insert}>{child}", "set": {{"layer": "WALL"}}}}]"#), "inside-block"),
        (r#"[{"op": "add", "kind": "hatch"}]"#.to_string(), "unsupported-kind"),
        (format!(r#"[{{"op": "update", "id": "{child}", "set": {{"radius": 1}}}}]"#), "unknown-key"),
        (
            r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "lineType": "NONE"}]"#.to_string(),
            "missing-definition",
        ),
        (
            r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0], "color": 300}]"#.to_string(),
            "invalid-value",
        ),
        (r#"[{"op": "add", "kind": "line", "start": [0, 0]}]"#.to_string(), "invalid-value"),
        (r#"[{"op": "add-layer", "name": "WALL"}]"#.to_string(), "already-exists"),
        (r#"[{"op": "add-layer", "name": "A/B"}]"#.to_string(), "invalid-value"),
        (r#"[{"op": "delete", "id": "FFFF"}]"#.to_string(), "not-found"),
    ];
    for (operations, code) in cases {
        let error = edit(&bytes, &operations).unwrap_err();
        assert_eq!(error.code, code, "{operations}: {error}");
    }
    // ビューポートを通した id は、その図形
    let edited =
        edit(&bytes, &format!(r#"[{{"op": "update", "id": "AB:{insert}", "set": {{"rotation": 10}}}}]"#)).unwrap();
    assert!(entity_tags(&edited.bytes, &insert).contains(&(50, "10.0".into())));
}

#[test]
fn writes_new_text_in_the_code_page_of_older_drawings() {
    // R12 の Shift_JIS の図面。ハンドルがない（$HANDLING がない）
    let mut bytes = b"0\r\nSECTION\r\n2\r\nHEADER\r\n9\r\n$ACADVER\r\n1\r\nAC1009\r\n9\r\n$DWGCODEPAGE\r\n3\r\nANSI_932\r\n0\r\nENDSEC\r\n\
        0\r\nSECTION\r\n2\r\nTABLES\r\n0\r\nTABLE\r\n2\r\nLAYER\r\n70\r\n1\r\n0\r\nLAYER\r\n2\r\n0\r\n70\r\n0\r\n62\r\n7\r\n6\r\nCONTINUOUS\r\n0\r\nENDTAB\r\n0\r\nENDSEC\r\n\
        0\r\nSECTION\r\n2\r\nENTITIES\r\n0\r\nTEXT\r\n8\r\n0\r\n10\r\n0\r\n20\r\n0\r\n40\r\n1\r\n1\r\n"
        .to_vec();
    let (old, _, _) = encoding_rs::SHIFT_JIS.encode("既存");
    bytes.extend_from_slice(&old);
    bytes.extend_from_slice(b"\r\n0\r\nENDSEC\r\n0\r\nEOF\r\n");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "text", "position": [0, 5], "text": "確認済み Ā", "height": 2},
            {"op": "add", "kind": "polyline", "vertices": [[0, 0], [1, 0]], "layer": "外構"}
        ]"#,
    )
    .unwrap();
    assert_eq!(edited.created, ["", ""]);
    let (expected, _, _) = encoding_rs::SHIFT_JIS.encode("確認済み \\U+0100");
    assert!(edited.bytes.windows(expected.len()).any(|window| window == &*expected));
    // 改行は元の図面と同じ CRLF
    assert!(!edited.bytes.windows(2).any(|window| window[1] == b'\n' && window[0] != b'\r'));
    let doc = parse_dxf(&edited.bytes);
    let texts: Vec<&str> = doc
        .entities
        .iter()
        .filter_map(|entity| match &entity.data {
            DxfEntityData::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    // 読み取りは \U+XXXX を文字に戻す
    assert_eq!(texts, ["既存", "確認済み Ā"]);
    assert!(doc.entities.iter().any(|entity| entity.base.kind == "POLYLINE" && entity.base.layer == "外構"));
    assert!(doc.layers.iter().any(|layer| layer.name == "外構"));
}

#[test]
fn edits_binary_drawings_in_binary() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dxf/binary.dxf");
    let bytes = std::fs::read(fixture).unwrap();
    let before = parse_dxf(&bytes).entities.len();
    let edited =
        edit(&bytes, r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [3, 4], "layer": "外構"}]"#).unwrap();
    assert!(crate::is_binary_dxf(&edited.bytes));
    let doc = parse_dxf(&edited.bytes);
    assert_eq!(doc.entities.len(), before + 1);
    let line = doc.entities.last().unwrap();
    assert_eq!(line.base.layer, "外構");
    assert!(matches!(line.data, DxfEntityData::Line { end: [3.0, 4.0, 0.0], .. }));
}

#[test]
fn splits_long_multiline_text_into_chunks() {
    let long = "あ".repeat(600);
    let parts = chunks(&long, 250);
    assert_eq!(parts.iter().map(|part| part.chars().count()).collect::<Vec<_>>(), [250, 250, 100]);
    assert_eq!(chunks("short", 250), ["short"]);
    assert_eq!(mtext_content("a\\b{c}\nd"), "a\\\\b\\{c\\}\\Pd");
}
