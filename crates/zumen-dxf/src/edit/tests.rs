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
        (r#"[{"op": "add", "kind": "spline"}]"#.to_string(), "unsupported-kind"),
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
            {"op": "add", "kind": "text", "position": [0, 5], "text": "確認済み Ā 😀", "height": 2},
            {"op": "add", "kind": "polyline", "vertices": [[0, 0], [1, 0]], "layer": "外構"}
        ]"#,
    )
    .unwrap();
    // ハンドルのない図面の id はファイルの中の位置（読み取りと同じ #番号）
    let scene = crate::dxf_to_scene(&parse_dxf(&edited.bytes), None);
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert!(
        edited.created.iter().all(|id| id.starts_with('#') && ids.contains(&id.as_str())),
        "{:?} {ids:?}",
        edited.created
    );
    let (expected, _, _) = encoding_rs::SHIFT_JIS.encode("確認済み \\U+0100 \\U+D83D\\U+DE00");
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
    assert_eq!(texts, ["既存", "確認済み Ā 😀"]);
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
    let error = edit(&bytes, r#"[{"op":"add","kind":"text","position":[0,0],"height":1,"text":"before\u0000after"}]"#)
        .unwrap_err();
    assert_eq!((error.code, error.operation), ("invalid-value", Some(0)));
}

#[test]
fn splits_long_multiline_text_into_chunks() {
    let long = "あ".repeat(600);
    let parts = chunks(&long, 250);
    assert_eq!(parts.iter().map(|part| part.chars().count()).collect::<Vec<_>>(), [250, 250, 100]);
    assert_eq!(chunks("short", 250), ["short"]);
    assert_eq!(mtext_content("a\\b{c}\nd"), "a\\\\b\\{c\\}\\Pd");
}

#[test]
fn chooses_entities_without_handles_by_their_place_in_the_file() {
    // R12 の図面（ハンドルなし）。id は先頭から何番目のコード 0 のレコードか
    let text = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n\
                0\nCIRCLE\n8\n0\n10\n5\n20\n5\n40\n1\n0\nENDSEC\n0\nEOF\n";
    let scene = crate::dxf_to_scene(&parse_dxf(text.as_bytes()), None);
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert_eq!(ids, ["#2", "#3"]);
    let edited = edit(
        text.as_bytes(),
        r##"[
            {"op": "update", "id": "#3", "set": {"radius": 2}},
            {"op": "delete", "id": "#2"},
            {"op": "add", "kind": "point", "position": [9, 9]}
        ]"##,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    assert!(matches!(doc.entities[0].data, DxfEntityData::Circle { radius: 2.0, .. }));
    // 線を消したので円は #2 になり、足した点は #3
    let scene = crate::dxf_to_scene(&doc, None);
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert_eq!(ids, ["#2", "#3"]);
    assert_eq!(edited.created, ["#3"]);
}

#[test]
fn reserves_handles_of_polyline_followers() {
    let bytes = with_raw(
        &drawing(""),
        "0\nPOLYLINE\n5\nE0\n8\n0\n66\n1\n70\n0\n0\nVERTEX\n5\nF0\n8\n0\n10\n0\n20\n0\n0\nSEQEND\n5\nF1\n8\n0\n",
    );
    let edited = edit(&bytes, r#"[{"op":"add","kind":"point","position":[1,2]}]"#).unwrap();
    assert_eq!(edited.created, ["F2"]);
}

#[test]
fn exhausted_handles_are_an_error_without_overflow() {
    let bytes = with_raw(&drawing(""), "0\nPOINT\n5\nFFFFFFFFFFFFFFFF\n8\n0\n10\n0\n20\n0\n");
    assert_eq!(edit_dxf(&bytes, &[]).unwrap().bytes, bytes);
    let error = edit(&bytes, r#"[{"op":"add","kind":"point","position":[1,2]}]"#).unwrap_err();
    assert_eq!((error.code, error.operation), ("handle-exhausted", Some(0)));
}

#[test]
fn changing_only_bulges_preserves_vertex_widths_and_ids() {
    let bytes = with_raw(
        &drawing(""),
        "0\r\nLWPOLYLINE\r\n5\r\nFACE\r\n100\r\nAcDbEntity\r\n8\r\n0\r\n100\r\nAcDbPolyline\r\n90\r\n2\r\n70\r\n0\r\n10\r\n0\r\n20\r\n0\r\n40\r\n1\r\n41\r\n2\r\n91\r\n10\r\n10\r\n1\r\n20\r\n1\r\n40\r\n3\r\n41\r\n4\r\n91\r\n20\r\n",
    );
    let written = edit(&bytes, r#"[{"op":"update","id":"FACE","set":{"bulges":[0.5,0]}}]"#).unwrap();
    let kept: Vec<_> =
        entity_tags(&written.bytes, "FACE").into_iter().filter(|(code, _)| [40, 41, 91].contains(code)).collect();
    let before: Vec<_> =
        entity_tags(&bytes, "FACE").into_iter().filter(|(code, _)| [40, 41, 91].contains(code)).collect();
    assert_eq!(kept, before);
}

#[test]
fn adds_block_definitions_and_places_them() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add-block", "name": "WIN-1", "basePoint": [5, 0]},
            {"op": "add", "kind": "line", "container": "block:WIN-1", "start": [0, 0], "end": [10, 0]},
            {"op": "add", "kind": "circle", "container": "block:win-1", "center": [5, 0], "radius": 1, "layer": "WALL"},
            {"op": "add", "kind": "block-reference", "block": "WIN-1", "position": [100, 100]}
        ]"#,
    )
    .unwrap();
    assert_eq!(edited.created[0], "WIN-1");
    let doc = parse_dxf(&edited.bytes);
    let block = doc.blocks.iter().find(|block| block.name == "WIN-1").expect("block");
    assert_eq!(block.base_point, [5.0, 0.0, 0.0]);
    let record = block.block_record_handle.clone().expect("block record");
    let inside: Vec<_> = doc.entities.iter().filter(|entity| entity.base.block.as_deref() == Some("WIN-1")).collect();
    assert_eq!(inside.len(), 2);
    assert!(inside.iter().all(|entity| entity.base.owner.as_deref() == Some(record.as_str())));
    // 参照を通して、ブロックの中の図形が描ける
    let scene = crate::dxf_to_scene(&doc, None);
    let reference = &edited.created[3];
    let drawn = scene.entities.iter().filter(|entity| entity.id.starts_with(&format!("{reference}>"))).count();
    assert_eq!(drawn, 2);
    let errors = [
        (r#"[{"op": "add-block", "name": "DOOR"}]"#, "already-exists"),
        (r#"[{"op": "add-block", "name": "*U1"}]"#, "invalid-value"),
        (r#"[{"op": "add-block", "name": "A", "origin": [0, 0]}]"#, "unknown-key"),
    ];
    for (operations, code) in errors {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
}

#[test]
fn adds_block_definitions_to_drawings_without_blocks() {
    // R12 の図面（ENTITIES だけ）には BLOCKS の節を ENTITIES の前に作る
    let text = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n0\n0\nENDSEC\n0\nEOF\n";
    let edited = edit(
        text.as_bytes(),
        r#"[
            {"op": "add-block", "name": "MARK"},
            {"op": "add", "kind": "point", "container": "block:MARK", "position": [0, 0]},
            {"op": "add", "kind": "block-reference", "block": "MARK", "position": [3, 4]}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    assert!(doc.blocks.iter().any(|block| block.name == "MARK"));
    let scene = crate::dxf_to_scene(&doc, None);
    // 線は BLOCKS の節（SECTION・BLOCK・POINT・ENDBLK・ENDSEC）の後ろに動く（#2 から #7）。参照は #8
    let ids: Vec<&str> = scene.entities.iter().map(|entity| entity.id.as_str()).collect();
    assert_eq!(ids, ["#7", "#8>#3"]);
    assert_eq!(edited.created, ["MARK", "#3", "#8"]);
}

#[test]
fn adds_linear_and_aligned_dimensions_with_their_blocks() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "dimension", "layer": "WALL", "start": [0, 0], "end": [5000, 200], "position": [0, -800]},
            {"op": "add", "kind": "dimension", "dimensionType": "aligned", "start": [0, 0], "end": [3000, 4000], "position": [-500, 500], "text": "斜め <>"},
            {"op": "add", "kind": "dimension", "start": [0, 0], "end": [0, 2700], "position": [-900, 0], "rotation": 90, "dimensionStyle": "standard"}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let scene = crate::dxf_to_scene(&doc, None);
    let values: Vec<(String, Option<zumen_core::PropertyRaw>)> = edited
        .created
        .iter()
        .map(|id| {
            let entity = scene.entities.iter().find(|entity| &entity.id == id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(entity.kind.name(), "dimension");
            assert!(!entity.primitives.is_empty(), "{id} draws nothing");
            let text = entity.properties.iter().find(|property| property.key == "dimensionText").unwrap();
            (text.value.clone(), text.raw.clone())
        })
        .collect();
    // 寸法値の property は計測値（文字の <> は計測値の文字になる）
    let measured = |text: &str, value: f64| (text.to_string(), Some(zumen_core::PropertyRaw::Number(value)));
    assert_eq!(values, [measured("5,000", 5000.0), measured("5,000", 5000.0), measured("2,700", 2700.0)]);
    let label = doc
        .entities
        .iter()
        .find_map(|entity| match &entity.data {
            DxfEntityData::MText(text) if entity.base.block.as_deref() == Some("*D2") => Some(text.raw_text.clone()),
            _ => None,
        })
        .unwrap();
    // zumen が書いた Standard は DIMDEC を持たないので、AutoCAD の既定の 4 桁
    assert_eq!(label, "斜め 5000.0000");
    let dimensions: Vec<_> = doc
        .entities
        .iter()
        .filter_map(|entity| match &entity.data {
            DxfEntityData::Dimension(dimension) if entity.base.block.is_none() => Some((entity, dimension)),
            _ => None,
        })
        .collect();
    assert_eq!(dimensions.len(), 3);
    let (first, dimension) = dimensions[0];
    assert_eq!(first.base.layer, "WALL");
    assert_eq!((dimension.dimtype, dimension.defpoint), (32.0, [5000.0, -800.0, 0.0]));
    assert_eq!(dimension.dimstyle, "Standard");
    let names: Vec<&str> = dimensions.iter().map(|(_, dimension)| dimension.anonymous_block_name.as_str()).collect();
    assert_eq!(names, ["*D1", "*D2", "*D3"]);
    assert_eq!(dimensions[1].1.dimtype, 33.0);
    let errors = [
        (
            r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [0, 10], "position": [5, 5]}]"#,
            "invalid-value",
        ),
        (
            r#"[{"op": "add", "kind": "dimension", "dimensionType": "radius", "start": [0, 0], "end": [1, 0], "position": [0, 1]}]"#,
            "unsupported-kind",
        ),
        (
            r#"[{"op": "add", "kind": "dimension", "dimensionStyle": "NONE", "start": [0, 0], "end": [1, 0], "position": [0, 1]}]"#,
            "missing-definition",
        ),
    ];
    for (operations, code) in errors {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
}

#[test]
fn adds_dimensions_to_r12_drawings_with_text_and_header_variables() {
    let text = "0\nSECTION\n2\nHEADER\n9\n$DIMTXT\n40\n250\n9\n$DIMASZ\n40\n200\n0\nENDSEC\n\
                0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
    let edited = edit(
        text.as_bytes(),
        r#"[{"op": "add", "kind": "dimension", "start": [0, 0], "end": [1000, 0], "position": [0, 500]}]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let block = doc.entities.iter().filter(|entity| entity.base.block.as_deref() == Some("*D1")).collect::<Vec<_>>();
    let DxfEntityData::Text(label) = &block.iter().find(|entity| entity.base.kind == "TEXT").expect("TEXT").data else {
        panic!()
    };
    assert_eq!((label.text.as_str(), label.height), ("1000.0000", 250.0));
    let scene = crate::dxf_to_scene(&doc, None);
    assert_eq!(scene.entities.len(), 1);
    assert_eq!(scene.entities[0].kind.name(), "dimension");
}

#[test]
fn adds_text_and_dimension_styles_and_line_types_that_later_operations_use() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add-text-style", "name": "JP", "font": "MS Gothic", "widthFactor": 0.8},
            {"op": "add-text-style", "name": "ROMANS", "font": "romans.shx", "bigFont": "extfont.shx"},
            {"op": "add-line-type", "name": "DASH2", "description": "__ __", "pattern": [6, -3]},
            {"op": "add-dimension-style", "name": "DIM-100", "scale": 100, "textHeight": 2.5, "arrowSize": 2.5,
             "extensionOffset": 1, "extensionExtension": 2, "textGap": 1, "textAbove": true, "decimals": 0,
             "suppressTrailingZeros": true, "textStyle": "JP"},
            {"op": "add", "kind": "line", "lineType": "DASH2", "start": [0, 0], "end": [10, 0]},
            {"op": "add", "kind": "text", "textStyle": "JP", "position": [0, 0], "text": "案内", "height": 3},
            {"op": "add", "kind": "dimension", "dimensionStyle": "DIM-100", "start": [0, 0], "end": [5000, 0], "position": [0, -800]}
        ]"#,
    )
    .unwrap();
    assert_eq!(&edited.created[..4], ["JP", "ROMANS", "DASH2", "DIM-100"]);
    let doc = parse_dxf(&edited.bytes);
    let style = doc.styles.iter().find(|style| style.name == "JP").unwrap();
    assert_eq!(
        (style.font.as_str(), style.font_family.as_deref(), style.width),
        ("msgothic.ttc", Some("MS Gothic"), 0.8)
    );
    let romans = doc.styles.iter().find(|style| style.name == "ROMANS").unwrap();
    assert_eq!(
        (romans.font.as_str(), romans.bigfont.as_str(), romans.font_family.as_deref()),
        ("romans.shx", "extfont.shx", None)
    );
    let linetype = doc.linetypes.iter().find(|linetype| linetype.name == "DASH2").unwrap();
    assert_eq!((linetype.dashes.clone(), linetype.pattern_length), (vec![6.0, -3.0], 9.0));
    let dimstyle = doc.dimstyles.iter().find(|style| style.name == "DIM-100").unwrap();
    assert_eq!((dimstyle.dimscale, dimstyle.dimtxt, dimstyle.dimasz), (100.0, 2.5, 2.5));
    // 寸法の文字は DIM-100 の値（小数なし、高さ 250、文字スタイル JP）
    let label = doc
        .entities
        .iter()
        .find_map(|entity| match &entity.data {
            DxfEntityData::MText(text) if entity.base.block.as_deref() == Some("*D1") => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!((label.raw_text.as_str(), label.char_height, label.style.as_str()), ("5000", 250.0, "JP"));
    let table = tags(&edited.bytes);
    let count = |name: &str| {
        let at =
            table.windows(2).position(|pair| pair[0] == (0, "TABLE".into()) && pair[1] == (2, name.into())).unwrap();
        table[at..].iter().find(|tag| tag.0 == 70).unwrap().1.clone()
    };
    // STYLE は Standard と 2 つ、LTYPE は ByBlock・ByLayer・Continuous・DASHED と 1 つ
    assert_eq!((count("STYLE"), count("LTYPE"), count("DIMSTYLE")), ("3".into(), "5".into(), "2".into()));
    for (operations, code) in [
        (r#"[{"op": "add-text-style", "name": "standard"}]"#, "already-exists"),
        (r#"[{"op": "add-dimension-style", "name": "X", "from": "NONE"}]"#, "missing-definition"),
        (r#"[{"op": "add-dimension-style", "name": "X", "decimals": 1.5}]"#, "invalid-value"),
        (r#"[{"op": "add-line-type", "name": "X", "dashes": [1]}]"#, "unknown-key"),
    ] {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
}

#[test]
fn adds_solid_and_patterned_hatches() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "hatch", "layer": "WALL", "boundaries": [[[0, 0], [100, 0], [100, 20], [0, 20]]]},
            {"op": "add", "kind": "hatch", "pattern": "ansi31", "patternScale": 10, "patternAngle": 15,
             "boundaries": [[[0, 0], [1000, 0], [1000, 800], [0, 800]], [[100, 100], [300, 100], [300, 300], [100, 300]]]},
            {"op": "add", "kind": "hatch", "pattern": "TILE", "patternLines": [[0, 0, 0, 0, 100], [90, 0, 0, 0, 100]],
             "boundaries": [[0, 0], [500, 0], [500, 500]]}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let hatches: Vec<_> = edited
        .created
        .iter()
        .map(|handle| match &doc.entities.iter().find(|entity| &entity.base.handle == handle).unwrap().data {
            DxfEntityData::Hatch(hatch) => hatch.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(hatches[0].solid_fill && hatches[0].pattern_name == "SOLID");
    assert_eq!(
        (hatches[1].pattern_name.as_str(), hatches[1].paths.len(), hatches[1].pattern_scale),
        ("ANSI31", 2, 10.0)
    );
    assert_eq!(hatches[1].pattern_lines[0].angle, 60.0);
    assert_eq!(
        (hatches[2].pattern_name.as_str(), hatches[2].pattern_lines.len(), hatches[2].pattern_type),
        ("TILE", 2, 2.0)
    );
    let scene = crate::dxf_to_scene(&doc, None);
    for handle in &edited.created {
        let entity = scene.entities.iter().find(|entity| &entity.id == handle).unwrap();
        assert_eq!(entity.kind.name(), "hatch");
        assert!(!entity.primitives.is_empty());
    }
    assert!(!scene.notices.iter().any(|notice| notice.code.starts_with("hatch")), "{:?}", scene.notices);
    let errors = [
        (
            r#"[{"op": "add", "kind": "hatch", "pattern": "AR-CONC", "boundaries": [[0, 0], [1, 0], [1, 1]]}]"#,
            "missing-definition",
        ),
        (r#"[{"op": "add", "kind": "hatch", "boundaries": [[0, 0], [1, 0]]}]"#, "invalid-value"),
        (r#"[{"op": "add", "kind": "hatch"}]"#, "invalid-value"),
    ];
    for (operations, code) in errors {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
    let r12 = "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
    let error = edit(r12.as_bytes(), r#"[{"op": "add", "kind": "hatch", "boundaries": [[0, 0], [1, 0], [1, 1]]}]"#);
    assert_eq!(error.unwrap_err().code, "unsupported-kind");
}

#[test]
fn places_text_by_justification_and_line_spacing() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "text", "position": [100, 50], "text": "中央", "height": 5, "justification": "middle-center"},
            {"op": "add", "kind": "text", "position": [100, 50], "text": "右", "height": 5, "justification": "right"},
            {"op": "add", "kind": "text", "position": [10, 90], "text": "一行", "height": 3, "multiline": true, "justification": "top-left"},
            {"op": "add", "kind": "text", "position": [10, 80], "text": "一\n二", "height": 3, "lineSpacing": 1.5}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let get =
        |index: usize| &doc.entities.iter().find(|entity| entity.base.handle == edited.created[index]).unwrap().data;
    let DxfEntityData::Text(middle) = get(0) else { panic!() };
    assert_eq!((middle.halign, middle.valign, middle.align_point), (1.0, 2.0, Some([100.0, 50.0, 0.0])));
    let DxfEntityData::Text(right) = get(1) else { panic!() };
    assert_eq!((right.halign, right.valign), (2.0, 0.0));
    let DxfEntityData::MText(top) = get(2) else { panic!() };
    assert_eq!(top.attachment_point, 1.0);
    let DxfEntityData::MText(spaced) = get(3) else { panic!() };
    assert_eq!((spaced.attachment_point, spaced.line_spacing_factor), (7.0, 1.5));
    // 中央に合わせた文字は、位置合わせの点をはさむ
    let scene = crate::dxf_to_scene(&doc, None);
    let centered = scene.entities.iter().find(|entity| entity.id == edited.created[0]).unwrap();
    let center = (centered.bbox.min_x + centered.bbox.max_x) / 2.0;
    // 右に合わせた文字は、同じ位置合わせの点で終わる
    let right = scene.entities.iter().find(|entity| entity.id == edited.created[1]).unwrap();
    assert!((right.bbox.max_x - center).abs() < 0.5, "{} {center}", right.bbox.max_x);
    let error = edit(
        &bytes,
        r#"[{"op": "add", "kind": "text", "position": [0, 0], "text": "x", "height": 1, "justification": "upper"}]"#,
    );
    assert_eq!(error.unwrap_err().code, "invalid-value");
}

#[test]
fn adds_attribute_definitions_and_fills_them_in_references() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add-block", "name": "TITLE", "basePoint": [0, 0]},
            {"op": "add", "kind": "polyline", "container": "block:TITLE", "vertices": [[0, 0], [170, 0], [170, 40], [0, 40]], "closed": true},
            {"op": "add", "kind": "attribute-definition", "container": "block:TITLE", "tag": "drawing", "prompt": "図面名", "text": "未定", "position": [85, 25], "height": 5, "justification": "middle-center"},
            {"op": "add", "kind": "attribute-definition", "container": "block:TITLE", "tag": "SCALE", "position": [5, 5], "height": 3},
            {"op": "add", "kind": "block-reference", "block": "TITLE", "position": [250, 10], "scale": 2, "attributes": {"DRAWING": "1階平面図"}},
            {"op": "add", "kind": "attribute-definition", "container": "block:DOOR", "tag": "WIDTH", "position": [0, 0], "height": 1, "text": "900"},
            {"op": "add", "kind": "block-reference", "block": "DOOR", "position": [0, 0], "attributes": {"WIDTH": "800"}}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let block = doc.blocks.iter().find(|block| block.name == "TITLE").unwrap();
    assert_eq!(block.flags, 2.0);
    let insert = |handle: &str| match &doc.entities.iter().find(|entity| entity.base.handle == handle).unwrap().data {
        DxfEntityData::Insert(insert) => insert.clone(),
        other => panic!("{other:?}"),
    };
    let title = insert(&edited.created[4]);
    let values: Vec<(String, String, [f64; 3])> = title
        .attribs
        .iter()
        .map(|attrib| match &attrib.data {
            DxfEntityData::Attrib { text, tag, .. } => {
                (tag.clone(), text.text.clone(), text.align_point.unwrap_or(text.insert))
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        values,
        [
            ("DRAWING".to_string(), "1階平面図".to_string(), [420.0, 60.0, 0.0]),
            ("SCALE".into(), String::new(), [260.0, 20.0, 0.0])
        ]
    );
    let door = insert(&edited.created[6]);
    assert_eq!(door.attribs.len(), 2, "the fixture's NO and the added WIDTH");
    // 参照を通して属性が描ける
    let scene = crate::dxf_to_scene(&doc, None);
    let texts: Vec<&str> = scene
        .entities
        .iter()
        .filter(|entity| entity.kind.name() == "attribute")
        .filter_map(|entity| {
            entity.properties.iter().find(|property| property.key == "text").map(|property| property.value.as_str())
        })
        .collect();
    assert!(texts.contains(&"1階平面図") && texts.contains(&"800"), "{texts:?}");
    for (operations, code) in [
        (
            r#"[{"op": "add", "kind": "attribute-definition", "tag": "A", "position": [0, 0], "height": 1}]"#,
            "invalid-value",
        ),
        (
            r#"[{"op": "add", "kind": "attribute-definition", "container": "block:DOOR", "tag": "A B", "position": [0, 0], "height": 1}]"#,
            "invalid-value",
        ),
        (
            r#"[{"op": "add", "kind": "block-reference", "block": "DOOR", "position": [0, 0], "attributes": {"NONE": "x"}}]"#,
            "invalid-value",
        ),
    ] {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
}

#[test]
fn adds_layouts_with_viewports_at_a_scale() {
    let bytes = drawing("0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10000\n21\n8000\n");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add-layout", "name": "A3-平面図", "paperSize": [420, 297]},
            {"op": "add", "kind": "viewport", "container": "layout:A3-平面図", "center": [210, 148.5], "width": 380, "height": 257, "viewCenter": [5000, 4000], "scale": 100},
            {"op": "add", "kind": "text", "container": "layout:A3-平面図", "position": [20, 10], "text": "1階平面図 S=1:100", "height": 5},
            {"op": "add-layout", "name": "A1", "paperSize": [841, 594]}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let layout = doc.layouts.iter().find(|layout| layout.name == "A3-平面図").unwrap();
    assert_eq!(
        (layout.paper_width, layout.paper_height, layout.paper_size.as_str()),
        (420.0, 297.0, "ISO_A3_(420.00_x_297.00_MM)")
    );
    let second = doc.layouts.iter().find(|layout| layout.name == "A1").unwrap();
    assert!(second.tab_order > layout.tab_order);
    let scene = crate::dxf_to_scene(&doc, Some("A3-平面図"));
    // ビューポートを通したモデルの線と、紙空間の文字
    let kinds: Vec<&str> = scene.entities.iter().map(|entity| entity.kind.name()).collect();
    assert!(kinds.contains(&"line") && kinds.contains(&"text"), "{kinds:?}");
    let scales =
        scene.metadata.iter().find(|property| property.key == "viewportScales").map(|property| property.value.clone());
    assert_eq!(scales.as_deref(), Some("1:100"));
    let sheet = scene.metadata.iter().find(|property| property.key == "sheet").unwrap();
    assert!(sheet.value.contains("420×297"), "{}", sheet.value);
    for (operations, code) in [
        (r#"[{"op": "add-layout", "name": "Layout1", "paperSize": [420, 297]}]"#, "already-exists"),
        (r#"[{"op": "add-layout", "name": "X"}]"#, "invalid-value"),
        (
            r#"[{"op": "add", "kind": "viewport", "center": [0, 0], "width": 1, "height": 1, "viewCenter": [0, 0]}]"#,
            "invalid-value",
        ),
    ] {
        assert_eq!(edit(&bytes, operations).unwrap_err().code, code, "{operations}");
    }
}

#[test]
fn adds_leaders_with_their_notes() {
    let bytes = drawing("");
    let edited = edit(
        &bytes,
        r#"[
            {"op": "add", "kind": "leader", "vertices": [[0, 0], [10, 10], [30, 10]], "text": "化粧ケイカル板 t=6"},
            {"op": "add", "kind": "leader", "vertices": [[0, 0], [-10, 5]], "text": "左", "height": 3}
        ]"#,
    )
    .unwrap();
    let doc = parse_dxf(&edited.bytes);
    let leader = |handle: &str| match &doc.entities.iter().find(|entity| entity.base.handle == handle).unwrap().data {
        DxfEntityData::Leader(leader) => leader.clone(),
        other => panic!("{other:?}"),
    };
    let first = leader(&edited.created[0]);
    assert!(first.has_arrowhead && first.points.len() == 3 && first.annotation_type == 0.0);
    let notes: Vec<(String, f64, [f64; 3], f64)> = doc
        .entities
        .iter()
        .filter_map(|entity| match &entity.data {
            DxfEntityData::MText(text) => {
                Some((text.raw_text.clone(), text.attachment_point, text.insert, text.char_height))
            }
            _ => None,
        })
        .collect();
    // Standard（DIMGAP は既定の 0.09、DIMTXT は 2.5）
    assert_eq!(notes[0], ("化粧ケイカル板 t=6".to_string(), 4.0, [30.09, 10.0, 0.0], 2.5));
    assert_eq!(notes[1], ("左".to_string(), 6.0, [-10.09, 5.0, 0.0], 3.0));
    let scene = crate::dxf_to_scene(&doc, None);
    let drawn = scene.entities.iter().find(|entity| entity.id == edited.created[0]).unwrap();
    assert_eq!(drawn.kind.name(), "leader");
    assert!(drawn.primitives.len() >= 2, "the arrowhead and the line");
    let r12 = "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n";
    let error = edit(r12.as_bytes(), r#"[{"op": "add", "kind": "leader", "vertices": [[0, 0], [1, 1]]}]"#);
    assert_eq!(error.unwrap_err().code, "unsupported-kind");
}

/// 画像と、その定義・辞書・クラスを足す（docs/decisions/0017）
#[test]
fn adds_images_with_their_definitions() {
    let operations = r#"[
        {"op": "add", "kind": "image", "path": "maps/site.png", "pixels": [200, 100], "position": [10, 20],
         "width": 50, "rotation": 90}
    ]"#;
    let source = parse_dxf(&drawing(""));
    let mut versions: Vec<(String, Vec<u8>)> = crate::write::DxfVersion::ALL
        .iter()
        .map(|version| (format!("{version:?}"), crate::write_dxf_as(&source, *version).bytes))
        .collect();
    // R14（AC1014）は R2000 の書き出しの版を変えて作る
    let r2000 = text(&versions.iter().find(|(name, _)| name == "R2000").unwrap().1);
    versions.push(("R14".into(), r2000.replacen("AC1015", "AC1014", 1).into_bytes()));
    for (name, bytes) in versions {
        let result = edit(&bytes, operations);
        if name == "R12" {
            assert_eq!(result.unwrap_err().code, "unsupported-kind");
            continue;
        }
        let edited = result.unwrap_or_else(|error| panic!("{name}: {error}"));
        // 同じ図面にもう 1 つ足すと、辞書とクラスを使い回し、項目の名前を変える
        let edited = edit(&edited.bytes, operations).unwrap_or_else(|error| panic!("{name}: {error}"));
        let doc = parse_dxf(&edited.bytes);
        let image = doc.entities.iter().find(|entity| entity.base.handle == edited.created[0]).unwrap();
        let DxfEntityData::Image(image) = &image.data else { panic!("{name}: {image:?}") };
        assert_eq!(image.path.as_deref(), Some("maps/site.png"), "{name}");
        assert_eq!(image.image_size, [200.0, 100.0], "{name}");
        assert!((image.u_pixel[1] - 0.25).abs() < 1e-12 && image.u_pixel[0].abs() < 1e-12, "{name}: {image:?}");
        assert!((image.v_pixel[0] + 0.25).abs() < 1e-12, "{name}: {image:?}");
        let all = tags(&edited.bytes);
        let count = |code: i32, value: &str| all.iter().filter(|tag| tag.0 == code && tag.1 == value).count();
        assert_eq!(count(1, "IMAGEDEF_REACTOR"), 1, "{name}: one class");
        assert_eq!(count(0, "IMAGEDEF_REACTOR"), 2, "{name}");
        assert_eq!(count(3, "ACAD_IMAGE_DICT"), 1, "{name}: one dictionary");
        assert_eq!((count(3, "site"), count(3, "site-2")), (1, 1), "{name}");
        let scene = crate::dxf_to_scene(&doc, None);
        let drawn = scene.entities.iter().find(|entity| entity.id == edited.created[0]).unwrap();
        assert_eq!(drawn.kind.name(), "image", "{name}");
    }
    let error = edit(
        &drawing(""),
        r#"[{"op": "add", "kind": "image", "path": "a.png", "pixels": [0, 1], "position": [0, 0]}]"#,
    );
    assert_eq!(error.unwrap_err().code, "invalid-value");
}
