//! DWG の編集のテスト。tests/fixtures/dwg の R2000 と R2018 のテストの図面（tests/writer で書いたもの）と、R2000 の
//! 図面のモデルを R13・R14 に書いた図面を編集し、読み直した値と、ブロックの図形の並び・つながりを確かめる

use super::*;
use crate::codec::object::open_raw;
use zumen_core::edit::parse_operations;
use zumen_dxf::{DxfDocument, DxfEntity, DxfEntityData};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../tests/fixtures/dwg/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

const FIXTURES: [&str; 2] = ["r2000.dwg", "r2018.dwg"];

fn edit(bytes: &[u8], json: &str) -> Result<Edited, EditError> {
    edit_dwg(bytes, &parse_operations(json).unwrap())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

fn entity<'a>(doc: &'a DxfDocument, handle: &str) -> Option<&'a DxfEntity> {
    doc.entities.iter().find(|entity| entity.base.handle == handle)
}

/// ブロックの図形の並び（R2004 以降）とつながり（R2000）: どのブロックの並び・つながりもたどれ、図面の図形を指し、
/// ブロックに属する図形（BLOCK・ENDBLK と、図形が持つ頂点・属性・SEQEND を除く）はちょうど 1 つのブロックにある。
/// 持ち主のハンドルを持つ図形（モード 0）はそのブロックにある。前の図形を書いた図形は、たどった順の前の図形を指す
/// （テストの図面は紙空間の図形をモード 2 で書き、最初の図形も Nolinks にするので、読み取りと同じくつながりを正とする）
fn check_structure(name: &str, bytes: &[u8]) {
    let mut notices = NoticeList::new();
    let source = Source::open(bytes, &mut notices).unwrap();
    let editor = Editor::new(source, NoticeList::new()).unwrap();
    let blocks: HashSet<u64> = editor.blocks.values().copied().collect();
    let mut member_of: HashMap<u64, u64> = HashMap::new();
    for &block in &blocks {
        let header = editor.read(block, |s, _| BlockHeader::read(s)).unwrap().value;
        let members: Vec<u64> = if editor.version >= Version::R2004 {
            header.entities.iter().map(|entity| entity.resolve(block)).collect()
        } else {
            let Some((first, last)) = header.first_last else { continue };
            let walked = editor
                .chain(first.resolve(block), last.resolve(block))
                .unwrap_or_else(|| panic!("{name}: chain of block {block:X}"));
            for (at, &current) in walked.iter().enumerate() {
                let common = editor.entity_common(current).unwrap();
                let previous = if at == 0 { 0 } else { walked[at - 1] };
                if common.links.is_some() || at > 0 {
                    assert_eq!(linked_previous(&common, current), previous, "{name}: link before {current:X}");
                }
            }
            walked
        };
        for member in members {
            assert!(editor.entity_common(member).is_some(), "{name}: block {block:X} has {member:X}");
            assert!(member_of.insert(member, block).is_none(), "{name}: {member:X} in two blocks");
        }
    }
    for &handle in editor.index.keys() {
        let Some(mut raw) = editor.bytes(handle).and_then(|bytes| open_raw(bytes, 0, editor.version).ok()) else {
            continue;
        };
        if matches!(raw.kind, 4 | 5) || !editor.kind_of(raw.kind).is_some_and(|(_, entity)| entity) {
            continue;
        }
        let Ok(common) = EntityCommon::read(&mut raw.input) else { continue };
        let owner = editor.owner_block(handle, &common);
        if common.mode == 0 && !blocks.contains(&owner) {
            continue;
        }
        let block = member_of.get(&handle).unwrap_or_else(|| panic!("{name}: {handle:X} is in no block"));
        if common.mode == 0 {
            assert_eq!(*block, owner, "{name}: {handle:X} in block {block:X}, owned by {owner:X}");
        }
    }
}

/// R2000 のテストの図面のモデルを、その版の DWG に書いたもの（図形のハンドルは元のまま）
fn written_as(version: crate::DwgVersion) -> Vec<u8> {
    crate::write_dwg_as(&crate::read_dwg(&fixture("r2000.dwg")).unwrap(), version).bytes
}

/// R13・R14 の図面も、その版のまま編集する。トゥルーカラーは近い色の番号に、線の太さとレイヤを印刷しない設定は
/// 書かずに知らせ、BYBLOCK の線種は線種の表の項目を指す
#[test]
fn r13_and_r14_drawings_are_edited_in_their_version() {
    for version in [crate::DwgVersion::R13, crate::DwgVersion::R14] {
        let name = version.name();
        let bytes = written_as(version);
        assert_eq!(edit_dwg(&bytes, &[]).unwrap().bytes, bytes, "{name}");
        check_structure(name, &bytes);
        let edited = edit(
            &bytes,
            r##"[
                {"op": "add", "kind": "line", "start": [1, 2], "end": [3, 4, 5], "color": "#ff8000", "lineType": "byblock", "lineWidth": 0.35},
                {"op": "add", "kind": "circle", "center": [10, 10], "radius": 5, "layer": "平面図"},
                {"op": "add", "kind": "text", "position": [1, 1], "text": "敷地 ^ 境界", "height": 2.5, "rotation": 90},
                {"op": "add", "kind": "text", "position": [1, 5], "text": "1 行目\n2 行目", "height": 2, "wrapWidth": 30},
                {"op": "add", "kind": "block-reference", "block": "BOX", "position": [20, 20], "scale": [2, 3], "rotation": 45},
                {"op": "add", "kind": "point", "container": "layout:Layout1", "position": [1, 1]},
                {"op": "add-layer", "name": "確認", "color": 1, "lineWidth": 0.5, "plot": false},
                {"op": "update", "id": "44", "set": {"radius": 3, "layer": "確認", "lineWidth": "bylayer"}},
                {"op": "delete", "id": "43"},
                {"op": "delete", "id": "46"}
            ]"##,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(edited.created.len(), 7, "{name}");
        let codes: Vec<&str> = edited.notices.iter().map(|notice| notice.code.as_str()).collect();
        for code in ["edit-nearest-color", "edit-no-lineweight", "edit-no-layer-plot"] {
            assert!(codes.contains(&code), "{name}: {code} not in {codes:?}");
        }
        check_structure(name, &edited.bytes);
        crate::samples::check_r2000(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        assert_eq!(doc.version, version.code(), "{name}");
        let get = |index: usize| entity(&doc, &edited.created[index]).unwrap_or_else(|| panic!("{name}: {index}"));
        let line = get(0);
        assert_eq!((line.base.color_index, line.base.true_color), (f64::from(nearest_aci(0xff8000)), None), "{name}");
        // 線種の名前は、R2000 以降と同じく線種の表の BYBLOCK の項目の名前
        assert_eq!((line.base.linetype.as_str(), line.base.lineweight), ("ByBlock", -1.0), "{name}");
        let DxfEntityData::Line { start, end } = &line.data else { panic!("{name}") };
        assert_eq!((*start, *end), ([1.0, 2.0, 0.0], [3.0, 4.0, 5.0]), "{name}");
        assert_eq!(get(1).base.layer, "平面図", "{name}");
        let DxfEntityData::Text(text) = &get(2).data else { panic!("{name}") };
        assert_eq!((text.text.as_str(), text.height, text.rotation), ("敷地 ^  境界", 2.5, 90.0), "{name}");
        let DxfEntityData::MText(mtext) = &get(3).data else { panic!("{name}") };
        assert_eq!((mtext.raw_text.as_str(), mtext.rect_width), ("1 行目\\P2 行目", 30.0), "{name}");
        let DxfEntityData::Insert(insert) = &get(4).data else { panic!("{name}") };
        assert_eq!(
            (insert.name.as_str(), [insert.xscale, insert.yscale, insert.zscale], insert.rotation),
            ("BOX", [2.0, 3.0, 1.0], 45.0),
            "{name}"
        );
        assert_eq!(get(5).base.space, zumen_dxf::DxfSpace::Paper, "{name}");
        let layer = doc.layers.iter().find(|layer| layer.name == "確認").unwrap();
        assert_eq!((layer.color_index, layer.plot, layer.lineweight), (1.0, true, -3.0), "{name}");
        let arc = entity(&doc, "44").unwrap();
        assert_eq!(arc.base.layer, "確認", "{name}");
        assert!(matches!(arc.data, DxfEntityData::Arc { radius: 3.0, .. }), "{name}");
        for handle in ["43", "46"] {
            assert!(entity(&doc, handle).is_none(), "{name}: {handle}");
        }
        // ハンドルは $HANDSEED から振り、$HANDSEED は次になる
        let again = edit(&edited.bytes, r#"[{"op": "add", "kind": "point", "position": [0, 0]}]"#).unwrap();
        let next = u64::from_str_radix(&edited.created[5], 16).unwrap() + 2;
        assert_eq!(again.created, vec![format!("{next:X}")], "{name}");
        // 折線: R14 はクラスの型の LWPOLYLINE（変えることもできる）、R13 にはない
        let polyline = edit(
            &bytes,
            r#"[
                {"op": "add", "kind": "polyline", "vertices": [[0, 0], [10, 0], [10, 5]], "bulges": [0, 0.5, 0], "closed": true, "width": 0.2},
                {"op": "update", "id": "45", "set": {"vertices": [[0, 0], [1, 1], [2, 0]], "closed": true}}
            ]"#,
        );
        if version == crate::DwgVersion::R13 {
            assert_eq!(polyline.unwrap_err().code, "unsupported-kind", "{name}");
            continue;
        }
        let polyline = polyline.unwrap();
        check_structure(name, &polyline.bytes);
        let doc = crate::read_dwg(&polyline.bytes).unwrap();
        let DxfEntityData::LwPolyline(added) = &entity(&doc, &polyline.created[0]).unwrap().data else { panic!() };
        assert_eq!((added.vertices.bulges.clone(), added.const_width), (vec![0.0, 0.5, 0.0], Some(0.2)), "{name}");
        let DxfEntityData::LwPolyline(changed) = &entity(&doc, "45").unwrap().data else { panic!() };
        assert_eq!((changed.vertices.points.len(), changed.vertices.closed), (3, true), "{name}");
    }
}

/// R14 の LWPOLYLINE のクラスがない図面に折線を足すと、クラスの節にクラスを足す
#[test]
fn r14_drawings_get_the_polyline_class_when_a_polyline_is_added() {
    let empty = zumen_dxf::parse_dxf(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
    let bytes = crate::write_dwg_as(&empty, crate::DwgVersion::R14).bytes;
    let classes = |bytes: &[u8]| -> Vec<String> {
        let mut notices = NoticeList::new();
        let source = Source::open(bytes, &mut notices).unwrap();
        let format = crate::objects::Format {
            version: Version::R14,
            maintenance: source.maintenance,
            codec: crate::bits::TextCodec::Bytes(encoding_rs::WINDOWS_1252),
        };
        crate::objects::read_classes(&source.classes, format).into_iter().map(|class| class.dxf_name).collect()
    };
    assert!(!classes(&bytes).contains(&"LWPOLYLINE".to_string()));
    let json = r#"[{"op": "add", "kind": "polyline", "vertices": [[0, 0], [10, 0], [10, 5]]}]"#;
    let edited = edit(&bytes, json).unwrap();
    assert!(classes(&edited.bytes).contains(&"LWPOLYLINE".to_string()), "{:?}", classes(&edited.bytes));
    check_structure("empty R14", &edited.bytes);
    crate::samples::check_r2000("empty R14", &edited.bytes);
    // 2 つ目の折線は同じクラス
    let again = edit(&edited.bytes, json).unwrap();
    assert_eq!(classes(&again.bytes), classes(&edited.bytes));
    let doc = crate::read_dwg(&again.bytes).unwrap();
    let kinds: Vec<&str> = doc.entities.iter().map(|entity| entity.base.kind.as_str()).collect();
    assert_eq!(kinds, ["LWPOLYLINE", "LWPOLYLINE"]);
}

#[test]
fn edits_without_operations_keep_the_bytes() {
    for name in FIXTURES {
        let bytes = fixture(name);
        assert_eq!(edit_dwg(&bytes, &[]).unwrap().bytes, bytes, "{name}");
        check_structure(name, &bytes);
    }
}

#[test]
fn adds_every_kind_and_reads_it_back() {
    for name in FIXTURES {
        let bytes = fixture(name);
        let edited = edit(
            &bytes,
            r##"[
                {"op": "add", "kind": "line", "start": [1, 2], "end": [3, 4, 5], "color": 3, "lineType": "DASHED"},
                {"op": "add", "kind": "circle", "center": [10, 10], "radius": 5, "layer": "平面図"},
                {"op": "add", "kind": "arc", "center": [0, 0], "radius": 2, "angle": [30, 180], "lineWidth": 0.35},
                {"op": "add", "kind": "ellipse", "center": [5, 5], "majorAxis": [4, 0], "ratio": 0.5},
                {"op": "add", "kind": "ellipse-arc", "center": [5, 5], "majorAxis": [0, 4], "ratio": 0.25, "parameter": [0, 3.14]},
                {"op": "add", "kind": "point", "position": [7, 8, 9]},
                {"op": "add", "kind": "polyline", "vertices": [[0, 0], [10, 0], [10, 5]], "bulges": [0, 0.5, 0], "closed": true, "width": 0.2},
                {"op": "add", "kind": "text", "position": [1, 1], "text": "敷地 ^ 境界", "height": 2.5, "rotation": 90},
                {"op": "add", "kind": "text", "position": [1, 5], "text": "1 行目\n2 行目 {x}", "height": 2, "wrapWidth": 30},
                {"op": "add", "kind": "block-reference", "block": "BOX", "position": [20, 20], "scale": [2, 3], "rotation": 45}
            ]"##,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(edited.created.len(), 10, "{name}");
        check_structure(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        let get = |index: usize| entity(&doc, &edited.created[index]).unwrap_or_else(|| panic!("{name}: {index}"));
        let line = get(0);
        assert_eq!((line.base.kind.as_str(), line.base.layer.as_str()), ("LINE", "0"), "{name}");
        assert_eq!((line.base.color_index, line.base.linetype.as_str()), (3.0, "DASHED"), "{name}");
        let DxfEntityData::Line { start, end } = &line.data else { panic!("{name}") };
        assert_eq!((*start, *end), ([1.0, 2.0, 0.0], [3.0, 4.0, 5.0]), "{name}");
        assert_eq!(get(1).base.layer, "平面図", "{name}");
        let arc = get(2);
        let DxfEntityData::Arc { start_angle, end_angle, .. } = &arc.data else { panic!("{name}") };
        // DWG はラジアンで持つので、度に戻すと最後の桁が違うことがある
        assert!(close(*start_angle, 30.0) && close(*end_angle, 180.0), "{name}: {start_angle} {end_angle}");
        assert_eq!(arc.base.lineweight, 35.0, "{name}");
        assert_eq!(get(3).base.kind, "ELLIPSE", "{name}");
        assert_eq!(get(5).base.kind, "POINT", "{name}");
        let DxfEntityData::LwPolyline(polyline) = &get(6).data else { panic!("{name}") };
        assert!(polyline.vertices.closed, "{name}");
        assert_eq!(polyline.vertices.bulges, vec![0.0, 0.5, 0.0], "{name}");
        assert_eq!(polyline.const_width, Some(0.2), "{name}");
        let DxfEntityData::Text(text) = &get(7).data else { panic!("{name}") };
        assert_eq!((text.text.as_str(), text.height, text.rotation), ("敷地 ^  境界", 2.5, 90.0), "{name}");
        let DxfEntityData::MText(mtext) = &get(8).data else { panic!("{name}") };
        assert_eq!(mtext.raw_text, "1 行目\\P2 行目 \\{x\\}", "{name}");
        assert_eq!(mtext.rect_width, 30.0, "{name}");
        let DxfEntityData::Insert(insert) = &get(9).data else { panic!("{name}") };
        assert_eq!(
            (insert.name.as_str(), [insert.xscale, insert.yscale, insert.zscale], insert.rotation),
            ("BOX", [2.0, 3.0, 1.0], 45.0),
            "{name}"
        );
        // ハンドルは $HANDSEED から振り、$HANDSEED は次になる
        let again = edit(&edited.bytes, r#"[{"op": "add", "kind": "point", "position": [0, 0]}]"#).unwrap();
        let next = u64::from_str_radix(&edited.created[9], 16).unwrap() + 1;
        assert_eq!(again.created, vec![format!("{next:X}")], "{name}");
    }
}

#[test]
fn adds_to_layouts_and_blocks() {
    for name in FIXTURES {
        let bytes = fixture(name);
        let edited = edit(
            &bytes,
            r#"[
                {"op": "add", "kind": "line", "container": "layout:Layout1", "start": [0, 0], "end": [1, 1]},
                {"op": "add", "kind": "circle", "container": "block:BOX", "center": [0, 0], "radius": 1},
                {"op": "add", "kind": "point", "container": "layout:Model", "position": [0, 0]}
            ]"#,
        )
        .unwrap();
        check_structure(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        let paper = entity(&doc, &edited.created[0]).unwrap();
        assert_eq!(paper.base.space, zumen_dxf::DxfSpace::Paper, "{name}");
        let block = entity(&doc, &edited.created[1]).unwrap();
        assert_eq!(block.base.block.as_deref(), Some("BOX"), "{name}");
        assert_eq!(entity(&doc, &edited.created[2]).unwrap().base.space, zumen_dxf::DxfSpace::Model, "{name}");
    }
}

#[test]
fn updates_only_the_values_it_names() {
    for name in FIXTURES {
        let bytes = fixture(name);
        let before = crate::read_dwg(&bytes).unwrap();
        let edited = edit(
            &bytes,
            r##"[
                {"op": "update", "id": "40", "set": {"start": [5, 5], "color": "#ff8000", "lineWidth": "byblock"}},
                {"op": "update", "id": "41", "set": {"text": "変更", "height": 7, "position": [3, 3], "textStyle": "Standard"}},
                {"op": "update", "id": "42", "set": {"text": "a\nb", "rotation": 30, "wrapWidth": 50}},
                {"op": "update", "id": "43", "set": {"position": [9, 9], "scale": 2, "rotation": 10}},
                {"op": "update", "id": "44", "set": {"radius": 3, "angle": [10, 350], "layer": "新しいレイヤ"}},
                {"op": "update", "id": "45", "set": {"vertices": [[0, 0], [1, 1], [2, 0]], "closed": true}},
                {"op": "update", "id": "47", "set": {"color": "byblock"}},
                {"op": "update", "id": "50", "set": {"lineType": "DASHED"}}
            ]"##,
        )
        .unwrap();
        assert!(edited.notices.iter().any(|notice| notice.code == "edit-added-layer"), "{name}");
        check_structure(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        let get = |handle: &str| entity(&doc, handle).unwrap();
        let DxfEntityData::Line { start, end } = &get("40").data else { panic!() };
        let DxfEntityData::Line { end: old_end, .. } = &entity(&before, "40").unwrap().data else { panic!() };
        assert_eq!((*start, end), ([5.0, 5.0, 0.0], old_end), "{name}");
        if name == "r2000.dwg" {
            // R2000 にトゥルーカラーはない
            assert_eq!(get("40").base.true_color, None, "{name}");
            assert!(edited.notices.iter().any(|notice| notice.code == "edit-nearest-color"), "{name}");
        } else {
            assert_eq!(get("40").base.true_color, Some(f64::from(0xff8000)), "{name}");
        }
        assert_eq!(get("40").base.lineweight, -2.0, "{name}");
        let DxfEntityData::Text(text) = &get("41").data else { panic!() };
        assert_eq!((text.text.as_str(), text.height, text.insert[0]), ("変更", 7.0, 3.0), "{name}");
        let DxfEntityData::MText(mtext) = &get("42").data else { panic!() };
        assert_eq!((mtext.raw_text.as_str(), mtext.rect_width), ("a\\Pb", 50.0), "{name}");
        assert!((mtext.text_direction.unwrap()[1] - 0.5).abs() < 1e-12, "{name}");
        let DxfEntityData::Insert(insert) = &get("43").data else { panic!() };
        assert_eq!(
            (insert.insert, [insert.xscale, insert.yscale, insert.zscale], insert.rotation),
            ([9.0, 9.0, 0.0], [2.0; 3], 10.0),
            "{name}"
        );
        let arc = get("44");
        assert_eq!(arc.base.layer, "新しいレイヤ", "{name}");
        let DxfEntityData::Arc { radius, start_angle, end_angle, .. } = &arc.data else { panic!() };
        assert_eq!(*radius, 3.0, "{name}");
        assert!(close(*start_angle, 10.0) && close(*end_angle, 350.0), "{name}: {start_angle} {end_angle}");
        let DxfEntityData::LwPolyline(polyline) = &get("45").data else { panic!() };
        assert_eq!((polyline.vertices.points.len(), polyline.vertices.closed), (3, true), "{name}");
        assert_eq!(get("47").base.color_index, 0.0, "{name}");
        assert_eq!(get("50").base.linetype, "DASHED", "{name}");
        // 触れていない図形はそのまま
        assert_eq!(get("46").data, entity(&before, "46").unwrap().data, "{name}");
        assert!(doc.layers.iter().any(|layer| layer.name == "新しいレイヤ"), "{name}");
    }
}

#[test]
fn deletes_entities_with_what_they_own() {
    for name in FIXTURES {
        let bytes = fixture(name);
        let edited = edit(
            &bytes,
            r#"[{"op": "delete", "id": "43"}, {"op": "delete", "id": "50"}, {"op": "delete", "id": "46"}]"#,
        )
        .unwrap();
        check_structure(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        for handle in ["43", "46", "50"] {
            assert!(entity(&doc, handle).is_none(), "{name}: {handle}");
        }
        assert_eq!(doc.entities.len(), crate::read_dwg(&bytes).unwrap().entities.len() - 3, "{name}");
        // 消した図形は図面のオブジェクトの表からもなくなる
        let mut notices = NoticeList::new();
        let source = Source::open(&edited.bytes, &mut notices).unwrap();
        assert!(!source.objects.handles.iter().any(|(handle, _)| matches!(handle, 0x43 | 0x46 | 0x50)), "{name}");
        // 消した図形の後ろに足すことも、前の図形を消すこともできる
        let again = edit(
            &edited.bytes,
            r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 0]}, {"op": "delete", "id": "45"}, {"op": "delete", "id": "40"}]"#,
        )
        .unwrap();
        check_structure(name, &again.bytes);
    }
}

#[test]
fn adds_and_changes_layers() {
    for name in FIXTURES {
        let bytes = fixture(name);
        let edited = edit(
            &bytes,
            r##"[
                {"op": "add-layer", "name": "確認", "color": 1, "lineType": "DASHED", "lineWidth": 0.5, "plot": false},
                {"op": "update-layer", "name": "確認", "set": {"visible": false, "locked": true}},
                {"op": "update-layer", "name": "平面図", "set": {"frozen": true, "color": 5}},
                {"op": "add", "kind": "line", "layer": "確認", "start": [0, 0], "end": [1, 1]}
            ]"##,
        )
        .unwrap();
        assert_eq!(edited.created[0], "確認", "{name}");
        let doc = crate::read_dwg(&edited.bytes).unwrap();
        let layer = doc.layers.iter().find(|layer| layer.name == "確認").unwrap();
        assert_eq!((layer.color_index, layer.linetype.as_str(), layer.lineweight), (1.0, "DASHED", 50.0), "{name}");
        assert_eq!((layer.off, layer.locked, layer.plot, layer.frozen), (true, true, false, false), "{name}");
        let plan = doc.layers.iter().find(|layer| layer.name == "平面図").unwrap();
        assert_eq!((plan.frozen, plan.color_index), (true, 5.0), "{name}");
        assert_eq!(entity(&doc, &edited.created[1]).unwrap().base.layer, "確認", "{name}");
    }
}

#[test]
fn reports_why_an_operation_fails() {
    let bytes = fixture("r2018.dwg");
    let code = |json: &str| edit(&bytes, json).unwrap_err();
    let cases = [
        (r#"[{"op": "add", "kind": "spline"}]"#, "unsupported-kind"),
        (r#"[{"op": "delete", "id": "FFFF"}]"#, "not-found"),
        (r#"[{"op": "delete", "id": "zz"}]"#, "not-found"),
        (r#"[{"op": "delete", "id": "10"}]"#, "not-found"),
        (r#"[{"op": "update", "id": "43>33", "set": {"color": 1}}]"#, "inside-block"),
        (
            r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 1], "lineType": "NOPE"}]"#,
            "missing-definition",
        ),
        (r#"[{"op": "add", "kind": "block-reference", "block": "NOPE", "position": [0, 0]}]"#, "missing-definition"),
        (
            r#"[{"op": "add", "kind": "line", "container": "layout:NOPE", "start": [0, 0], "end": [1, 1]}]"#,
            "missing-definition",
        ),
        (r#"[{"op": "add-layer", "name": "平面図"}]"#, "already-exists"),
        (r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 1], "color": 300}]"#, "invalid-value"),
        (r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1, 1], "radius": 1}]"#, "unknown-key"),
        (r#"[{"op": "update-layer", "name": "NOPE", "set": {}}]"#, "not-found"),
        (r#"[{"op": "add-layer", "name": "a<b"}]"#, "invalid-value"),
    ];
    for (json, expected) in cases {
        let error = code(json);
        assert_eq!(error.code, expected, "{json}: {error}");
        assert_eq!(error.operation, Some(0), "{json}");
    }
    // 1 つでも通らなければ何も書かない（2 つ目の操作の番号）
    let error = code(r#"[{"op": "add-layer", "name": "X"}, {"op": "delete", "id": "FFFF"}]"#);
    assert_eq!(error.operation, Some(1));
    assert_eq!(edit_dwg(b"not a drawing", &[]).unwrap_err().code, "unreadable");
}

#[test]
fn new_drawings_take_the_units_and_operations() {
    let bytes = new_dwg("mm").unwrap();
    let edited = edit(&bytes, r#"[{"op": "add", "kind": "line", "start": [0, 0], "end": [1000, 0]}]"#).unwrap();
    check_structure("new", &edited.bytes);
    let doc = crate::read_dwg(&edited.bytes).unwrap();
    assert_eq!(doc.header.insunits, 4.0);
    assert_eq!(doc.entities.len(), 1);
    assert_eq!(new_dwg("furlong").unwrap_err().code, "invalid-value");
}

/// 壊れた図面を編集しても panic せず、すぐに終わる
#[test]
fn damaged_drawings_edit_without_panicking() {
    let operations = parse_operations(
        r#"[{"op": "add-layer", "name": "X"}, {"op": "add", "kind": "line", "start": [0, 0], "end": [1, 1]}, {"op": "update", "id": "40", "set": {"layer": "X"}}, {"op": "delete", "id": "41"}]"#,
    )
    .unwrap();
    let mut seed: u64 = 0x5eed_0e17;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    for name in FIXTURES {
        let original = fixture(name);
        for round in 0..200 {
            let mut bytes = original.clone();
            match round % 3 {
                0 => bytes.truncate(random(bytes.len())),
                1 => {
                    for _ in 0..1 + random(8) {
                        let at = random(bytes.len());
                        bytes[at] = random(256) as u8;
                    }
                }
                _ => {
                    let at = random(bytes.len());
                    let length = random(64).min(bytes.len() - at);
                    bytes.drain(at..at + length);
                }
            }
            let start = std::time::Instant::now();
            let _ = edit_dwg(&bytes, &operations);
            assert!(start.elapsed().as_secs() < 5, "{name} round {round} took too long");
        }
    }
}

/// 実物の図面（ZUMEN_SAMPLES）: 元の図面も編集した図面もブロックの並び・つながりが合い、編集した図面の検査の値
/// （CRC・ページの検査の値・Reed-Solomon）がすべて合い、読み直すと足した図形があり、消した図形がない
#[test]
fn edits_of_real_drawings_keep_their_structure() {
    for (name, bytes) in crate::samples::samples() {
        let Some(version) = Version::from_magic(&bytes[..6]) else { continue };
        check_structure(&name, &bytes);
        let doc = crate::read_dwg(&bytes).unwrap();
        let model: Vec<&DxfEntity> =
            doc.entities.iter().filter(|entity| entity.base.space == zumen_dxf::DxfSpace::Model).collect();
        let simple = ["LINE", "CIRCLE", "ARC", "LWPOLYLINE", "POINT", "TEXT", "ELLIPSE"];
        let mut json = vec![
            serde_json::json!({"op": "add-layer", "name": "ZUMEN-EDIT-CHECK", "color": 1}),
            serde_json::json!({"op": "add", "kind": "line", "layer": "ZUMEN-EDIT-CHECK", "start": [0, 0], "end": [1000, 500]}),
            serde_json::json!({"op": "add", "kind": "text", "position": [0, -100], "text": "編集の確認\n2 行目", "height": 25}),
            serde_json::json!({"op": "add", "kind": "point", "container": "layout:Layout1", "position": [1, 1]}),
        ];
        if !doc.layouts.iter().any(|layout| layout.name.eq_ignore_ascii_case("Layout1")) {
            json.pop();
        }
        if let Some(first) = model.first() {
            json.push(serde_json::json!({"op": "update", "id": first.base.handle, "set": {"layer": "ZUMEN-EDIT-CHECK", "color": "#123456"}}));
        }
        let mut deleted: Vec<String> = model
            .iter()
            .skip(1)
            .filter(|entity| simple.contains(&entity.base.kind.as_str()))
            .take(3)
            .map(|entity| entity.base.handle.clone())
            .collect();
        // 属性のある配置も消す（属性と SEQEND も消える）
        if let Some(insert) = model
            .iter()
            .skip(1)
            .find(|entity| matches!(&entity.data, DxfEntityData::Insert(insert) if !insert.attribs.is_empty()))
        {
            deleted.push(insert.base.handle.clone());
        }
        for handle in &deleted {
            json.push(serde_json::json!({"op": "delete", "id": handle}));
        }
        let edited =
            edit(&bytes, &serde_json::Value::Array(json).to_string()).unwrap_or_else(|error| panic!("{name}: {error}"));
        check_structure(&name, &edited.bytes);
        match version {
            Version::R13 | Version::R14 | Version::R2000 => crate::samples::check_r2000(&name, &edited.bytes),
            Version::R2007 => {
                crate::samples::check_r2007(&name, &edited.bytes);
            }
            _ => crate::samples::check_r2004(&name, &edited.bytes, version),
        }
        let again = crate::read_dwg(&edited.bytes).unwrap();
        for created in &edited.created[1..] {
            assert!(entity(&again, created).is_some(), "{name}: {created}");
        }
        for handle in &deleted {
            assert!(entity(&again, handle).is_none(), "{name}: {handle}");
        }
        assert_eq!(again.entities.len(), doc.entities.len() + edited.created.len() - 1 - deleted.len(), "{name}");
    }
}

#[test]
fn exhausted_handles_fail_without_overflowing_or_reusing_an_id() {
    let bytes = new_dwg("mm").unwrap();
    let mut source = Source::open(&bytes, &mut NoticeList::new()).unwrap();
    source.objects.handles.push((u64::MAX, 0));
    let mut editor = Editor::new(source, NoticeList::new()).unwrap();
    let error = editor.next_handle(0).unwrap_err();
    assert_eq!(error.code, "handle-exhausted");
    assert_eq!(editor.seed, u64::MAX);
    assert!(!editor.seed_used);
}

#[test]
fn changing_bulges_or_closed_keeps_vertex_widths_and_ids() {
    for json in [
        r#"[{"op":"update","id":"1","set":{"bulges":[0.5,0]}}]"#,
        r#"[{"op":"update","id":"1","set":{"closed":true}}]"#,
    ] {
        let Body::LwPolyline(mut polyline) = build::lwpolyline(&[[0.0; 3], [1.0, 1.0, 0.0]], &[0.0; 2], false, None)
        else {
            panic!()
        };
        polyline.widths = vec![[1.0, 2.0], [3.0, 4.0]];
        polyline.ids = vec![10, 20];
        polyline.flags |= 32 | 1024;
        let operations = parse_operations(json).unwrap();
        let EditOperation::Update { set, .. } = &operations[0] else { panic!() };
        build::update_lwpolyline(0, &mut polyline, &ValueReader::new(set, 0)).unwrap();
        assert_eq!(polyline.widths, [[1.0, 2.0], [3.0, 4.0]]);
        assert_eq!(polyline.ids, [10, 20]);
        assert_eq!(polyline.flags & (32 | 1024), 32 | 1024);
    }
}

/// ブロックの定義を足し、中に図形を足して配置する（docs/decisions/0017）。どの版の図面でも、読み直したブロックに
/// 図形があり、配置を通して描ける
#[test]
fn adds_block_definitions_in_every_version() {
    for version in crate::DwgVersion::ALL {
        let name = version.name();
        let bytes = written_as(version);
        let edited = edit(
            &bytes,
            r#"[
                {"op": "add-block", "name": "窓-1", "basePoint": [5, 0]},
                {"op": "add", "kind": "line", "container": "block:窓-1", "start": [0, 0], "end": [10, 0]},
                {"op": "add", "kind": "circle", "container": "block:窓-1", "center": [5, 0], "radius": 1},
                {"op": "add", "kind": "block-reference", "block": "窓-1", "position": [100, 100]},
                {"op": "add", "kind": "block-reference", "block": "窓-1", "position": [200, 100], "container": "layout:Layout1"}
            ]"#,
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(edited.created[0], "窓-1", "{name}");
        check_structure(name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let block = doc.blocks.iter().find(|block| block.name == "窓-1").unwrap_or_else(|| panic!("{name}"));
        assert_eq!(block.base_point, [5.0, 0.0, 0.0], "{name}");
        for created in &edited.created[1..3] {
            assert_eq!(entity(&doc, created).unwrap().base.block.as_deref(), Some("窓-1"), "{name}");
        }
        let scene = zumen_dxf::dxf_to_scene(&doc, None);
        let reference = &edited.created[3];
        let drawn = scene.entities.iter().filter(|entity| entity.id.starts_with(&format!("{reference}>"))).count();
        assert_eq!(drawn, 2, "{name}");
        // 同じ名前は足せない
        let again = edit(&edited.bytes, r#"[{"op": "add-block", "name": "窓-1"}]"#).unwrap_err();
        assert_eq!(again.code, "already-exists", "{name}");
    }
    let empty = new_dwg("mm").unwrap();
    let edited = edit(
        &empty,
        r#"[{"op": "add-block", "name": "N"}, {"op": "add", "kind": "point", "container": "block:N", "position": [0, 0]}]"#,
    )
    .unwrap();
    check_structure("new", &edited.bytes);
    assert!(crate::read_dwg(&edited.bytes).unwrap().blocks.iter().any(|block| block.name == "N"));
    assert_eq!(edit(&empty, r#"[{"op": "add-block", "name": "*D1"}]"#).unwrap_err().code, "invalid-value");
}

/// 寸法を足す（docs/decisions/0017）。どの版の図面でも、寸法とその表示のブロック（*D…）を読み直して描け、寸法値は
/// 計測値
#[test]
fn adds_dimensions_in_every_version() {
    let operations = r#"[
        {"op": "add", "kind": "dimension", "start": [0, 0], "end": [50, 5], "position": [0, -20]},
        {"op": "add", "kind": "dimension", "dimensionType": "aligned", "start": [0, 0], "end": [30, 40], "position": [-10, 10], "text": "A=<>"},
        {"op": "add", "kind": "dimension", "start": [0, 0], "end": [0, 27], "position": [-9, 0], "rotation": 90, "container": "block:BOX"}
    ]"#;
    let mut versions: Vec<(String, Vec<u8>)> =
        crate::DwgVersion::ALL.iter().map(|version| (version.name().to_string(), written_as(*version))).collect();
    versions.push(("new".into(), new_dwg("mm").unwrap()));
    for (name, bytes) in versions {
        let operations =
            if name == "new" { operations.replace(r#", "container": "block:BOX""#, "") } else { operations.into() };
        let edited = edit(&bytes, &operations).unwrap_or_else(|error| panic!("{name}: {error}"));
        check_structure(&name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let measurements: Vec<Option<f64>> = edited
            .created
            .iter()
            .map(|handle| {
                let DxfEntityData::Dimension(dimension) =
                    &entity(&doc, handle).unwrap_or_else(|| panic!("{name}")).data
                else {
                    panic!("{name}: {handle} is not a dimension")
                };
                let block = doc
                    .entities
                    .iter()
                    .filter(|entity| entity.base.block.as_deref() == Some(dimension.anonymous_block_name.as_str()));
                assert!(block.count() >= 7, "{name}: {} has too few entities", dimension.anonymous_block_name);
                dimension.actual_measurement
            })
            .collect();
        // 計測値（42）は R2000 から
        let expected =
            if matches!(name.as_str(), "R13" | "R14") { [None; 3] } else { [Some(50.0), Some(50.0), Some(27.0)] };
        assert_eq!(measurements, expected, "{name}");
        let scene = zumen_dxf::dxf_to_scene(&doc, None);
        for handle in &edited.created[..2] {
            let drawn = scene.entities.iter().find(|entity| &entity.id == handle).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(drawn.kind.name(), "dimension", "{name}");
            assert!(!drawn.primitives.is_empty(), "{name}: {handle} draws nothing");
        }
        let mtext = doc
            .entities
            .iter()
            .find_map(|entity| match &entity.data {
                DxfEntityData::MText(text)
                    if entity.base.block.as_deref().is_some_and(|block| block.starts_with("*D")) =>
                {
                    Some(text.raw_text.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}"));
        assert!(mtext.starts_with("50"), "{name}: {mtext}");
    }
}

/// 文字スタイル・線種・寸法スタイルを足し、同じ編集の図形が使う（docs/decisions/0017）
#[test]
fn adds_styles_in_every_version() {
    let operations = r#"[
        {"op": "add-text-style", "name": "JP", "font": "MS Gothic", "widthFactor": 0.8},
        {"op": "add-line-type", "name": "DASH2", "description": "__ __", "pattern": [6, -3]},
        {"op": "add-dimension-style", "name": "DIM-100", "scale": 100, "textHeight": 2.5, "arrowSize": 2.5,
         "textAbove": true, "decimals": 0, "suppressTrailingZeros": true, "textStyle": "JP"},
        {"op": "add", "kind": "line", "lineType": "DASH2", "start": [0, 0], "end": [10, 0]},
        {"op": "add", "kind": "text", "textStyle": "JP", "position": [0, 0], "text": "案内", "height": 3},
        {"op": "add", "kind": "dimension", "dimensionStyle": "DIM-100", "start": [0, 0], "end": [5000, 0], "position": [0, -800]}
    ]"#;
    let mut versions: Vec<(String, Vec<u8>)> =
        crate::DwgVersion::ALL.iter().map(|version| (version.name().to_string(), written_as(*version))).collect();
    versions.push(("new".into(), new_dwg("mm").unwrap()));
    for (name, bytes) in versions {
        let edited = edit(&bytes, operations).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(&edited.created[..3], ["JP", "DASH2", "DIM-100"], "{name}");
        check_structure(&name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let style = doc.styles.iter().find(|style| style.name == "JP").unwrap_or_else(|| panic!("{name}"));
        assert_eq!(
            (style.font.as_str(), style.font_family.as_deref(), style.width),
            ("msgothic.ttc", Some("MS Gothic"), 0.8),
            "{name}"
        );
        let linetype =
            doc.linetypes.iter().find(|linetype| linetype.name == "DASH2").unwrap_or_else(|| panic!("{name}"));
        assert_eq!(linetype.dashes, [6.0, -3.0], "{name}");
        assert_eq!(entity(&doc, &edited.created[3]).unwrap().base.linetype, "DASH2", "{name}");
        let dimstyle = doc.dimstyles.iter().find(|style| style.name == "DIM-100").unwrap_or_else(|| panic!("{name}"));
        assert_eq!((dimstyle.dimscale, dimstyle.dimtxt), (100.0, 2.5), "{name}");
        let label = doc
            .entities
            .iter()
            .find_map(|entity| match &entity.data {
                DxfEntityData::MText(text)
                    if entity.base.block.as_deref().is_some_and(|block| block.starts_with("*D")) =>
                {
                    Some(text.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}"));
        assert_eq!((label.raw_text.as_str(), label.char_height, label.style.as_str()), ("5000", 250.0, "JP"), "{name}");
    }
}

/// ハッチングを足す（docs/decisions/0017）。R14 から（R14 はクラスの型）、R13 は受け付けない
#[test]
fn adds_hatches_from_r14_on() {
    let operations = r#"[
        {"op": "add", "kind": "hatch", "boundaries": [[[0, 0], [100, 0], [100, 20], [0, 20]]]},
        {"op": "add", "kind": "hatch", "pattern": "ANSI37", "patternScale": 2, "patternAngle": 30,
         "boundaries": [[[0, 0], [50, 0], [50, 40], [0, 40]], [[10, 10], [20, 10], [20, 20], [10, 20]]]}
    ]"#;
    let mut versions: Vec<(String, Vec<u8>)> =
        crate::DwgVersion::ALL.iter().map(|version| (version.name().to_string(), written_as(*version))).collect();
    versions.push(("new".into(), new_dwg("mm").unwrap()));
    for (name, bytes) in versions {
        let edited = edit(&bytes, operations);
        if name == "R13" {
            assert_eq!(edited.unwrap_err().code, "unsupported-kind");
            continue;
        }
        let edited = edited.unwrap_or_else(|error| panic!("{name}: {error}"));
        check_structure(&name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let get = |handle: &str| match &entity(&doc, handle).unwrap_or_else(|| panic!("{name}: {handle}")).data {
            DxfEntityData::Hatch(hatch) => hatch.clone(),
            other => panic!("{name}: {other:?}"),
        };
        let (solid, patterned) = (get(&edited.created[0]), get(&edited.created[1]));
        assert!(solid.solid_fill, "{name}");
        assert_eq!((patterned.pattern_name.as_str(), patterned.paths.len()), ("ANSI37", 2), "{name}");
        assert!((patterned.pattern_lines[1].angle - 165.0).abs() < 1e-9, "{name}");
        let scene = zumen_dxf::dxf_to_scene(&doc, None);
        for handle in &edited.created {
            let drawn = scene.entities.iter().find(|entity| &entity.id == handle).unwrap_or_else(|| panic!("{name}"));
            assert!(!drawn.primitives.is_empty(), "{name}: {handle} draws nothing");
        }
    }
}

/// 文字の位置合わせと行間（docs/decisions/0017）
#[test]
fn places_text_by_justification_in_every_version() {
    let operations = r#"[
        {"op": "add", "kind": "text", "position": [100, 50], "text": "中央", "height": 5, "justification": "middle-center"},
        {"op": "add", "kind": "text", "position": [10, 90], "text": "一行", "height": 3, "multiline": true, "justification": "top-right", "lineSpacing": 1.5}
    ]"#;
    let mut versions: Vec<(String, Vec<u8>)> =
        crate::DwgVersion::ALL.iter().map(|version| (version.name().to_string(), written_as(*version))).collect();
    versions.push(("new".into(), new_dwg("mm").unwrap()));
    for (name, bytes) in versions {
        let edited = edit(&bytes, operations).unwrap_or_else(|error| panic!("{name}: {error}"));
        check_structure(&name, &edited.bytes);
        let doc = crate::read_dwg(&edited.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        let DxfEntityData::Text(middle) = &entity(&doc, &edited.created[0]).unwrap().data else { panic!("{name}") };
        assert_eq!((middle.halign, middle.valign), (1.0, 2.0), "{name}");
        assert_eq!(middle.align_point.map(|point| [point[0], point[1]]), Some([100.0, 50.0]), "{name}");
        let DxfEntityData::MText(top) = &entity(&doc, &edited.created[1]).unwrap().data else { panic!("{name}") };
        // MTEXT の行間は R2000 から
        let old = matches!(name.as_str(), "R13" | "R14");
        assert_eq!((top.attachment_point, top.line_spacing_factor), (3.0, if old { 1.0 } else { 1.5 }), "{name}");
        assert_eq!(edited.notices.iter().any(|notice| notice.code == "edit-no-line-spacing"), old, "{name}");
    }
}

/// DWG 2.13: DIMSTYLE の参照が相対ハンドルでも、写したスタイルは元と同じ文字スタイルを指す。
#[test]
fn cloned_dimension_style_keeps_relative_references() {
    for version in crate::DwgVersion::ALL {
        let name = version.name();
        let bytes = written_as(version);
        let mut notices = NoticeList::new();
        let source = Source::open(&bytes, &mut notices).unwrap();
        let mut editor = Editor::new(source, notices).unwrap();
        let template = editor.dimstyles["STANDARD"];
        let mut style = editor.read(template, |s, _| DimStyle::read(s)).unwrap();
        let expected: Vec<_> = style.value.handles.iter().map(|handle| handle.resolve(template)).collect();
        let text_style = expected[0];
        style.value.handles[0] = if text_style < template {
            HandleRef { code: 0xC, value: template - text_style }
        } else {
            HandleRef { code: 0xA, value: text_style - template }
        };
        let kind = style.kind;
        editor.store(template, kind, &style.eed, |out| style.value.write(out));
        let relative = editor.finish().bytes;
        let edited = edit(&relative, r#"[{"op":"add-dimension-style","name":"CLONE"}]"#).unwrap();
        let mut notices = NoticeList::new();
        let source = Source::open(&edited.bytes, &mut notices).unwrap();
        let editor = Editor::new(source, notices).unwrap();
        let cloned = editor.dimstyles["CLONE"];
        let style = editor.read(cloned, |s, _| DimStyle::read(s)).unwrap();
        let actual: Vec<_> = style.value.handles.iter().map(|handle| handle.resolve(cloned)).collect();
        assert_eq!(actual, expected, "{name}: cloned DIMSTYLE references");
    }
}

#[test]
fn reports_approximated_jis_dimension_text_position() {
    let bytes = new_dwg("mm").unwrap();
    let mut notices = NoticeList::new();
    let source = Source::open(&bytes, &mut notices).unwrap();
    let mut editor = Editor::new(source, notices).unwrap();
    let handle = editor.dimstyles["STANDARD"];
    let mut style = editor.read(handle, |s, _| DimStyle::read(s)).unwrap();
    let value = style.value.values.iter_mut().find(|(key, _)| *key == "DIMTAD").unwrap();
    value.1 = DimValue::Short(3);
    let kind = style.kind;
    editor.store(handle, kind, &style.eed, |out| style.value.write(out));
    let bytes = editor.finish().bytes;
    let edited =
        edit(&bytes, r#"[{"op":"add","kind":"dimension","start":[0,0],"end":[1000,0],"position":[0,-100]}]"#).unwrap();
    assert!(edited.notices.iter().any(|notice| notice.code == "edit-dimension-text-position" && notice.count == 1));
}

#[test]
fn exhausted_dimension_block_names_fail_without_overflowing() {
    let bytes = new_dwg("mm").unwrap();
    let mut notices = NoticeList::new();
    let source = Source::open(&bytes, &mut notices).unwrap();
    let mut editor = Editor::new(source, notices).unwrap();
    editor.create_block(0, "*D18446744073709551615", [0.0; 3], true).unwrap();
    let bytes = editor.finish().bytes;
    let error =
        edit(&bytes, r#"[{"op":"add","kind":"dimension","start":[0,0],"end":[10,0],"position":[0,5]}]"#).unwrap_err();
    assert_eq!((error.code, error.operation), ("name-exhausted", Some(0)));
}
