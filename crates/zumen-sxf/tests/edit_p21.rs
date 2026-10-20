//! P21 のその場の書き直し: 読んだ P21 を変えずに書くと同じバイト列になり、変えた要素のインスタンスだけが書き直される

mod common;

use common::{compare_entities, read_fixture};
use std::collections::{BTreeMap, HashMap};
use zumen_sxf::{
    Annotation, CompositeCurve, CompositeCurveOrg, LayerDefinition, Region, SfigOrg, Styled, SubFigure, SxfDrawing,
    SxfFeature, SxfWritten, parse_p21, sxf_to_scene, write_p21,
};

/// P21 のインスタンスを番号ごとに取り出す（改行と連続する空白は 1 つの空白にする）
fn instances(bytes: &[u8]) -> BTreeMap<u64, String> {
    let text: String = bytes.iter().map(|byte| char::from(*byte)).collect();
    let data = &text[text.find("DATA;").unwrap() + 5..];
    let mut out = BTreeMap::new();
    for chunk in data.split(";\n").flat_map(|chunk| chunk.split(";\r\n")) {
        let chunk = chunk.trim();
        let Some(rest) = chunk.strip_prefix('#') else { continue };
        let Some((id, body)) = rest.split_once('=') else { continue };
        if let Ok(id) = id.trim().parse::<u64>() {
            out.insert(id, body.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    out
}

/// 書いた P21 を読み直した図面の表示が、書いた図面の表示と同じ（新しい要素の番号は書いた番号に読み替える）
fn assert_reads_back(drawing: &SxfDrawing, written: &SxfWritten) {
    let again = parse_p21(&written.bytes).unwrap();
    let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
    let (before, after) = (sxf_to_scene(drawing), sxf_to_scene(&again));
    compare_entities(&before, &after, Some(&ids), 1e-9).unwrap();
    assert_eq!(before.layers, after.layers);
    assert_eq!(before.sheet, after.sheet);
}

fn line(id: f64, layer: f64, x2: f64) -> SxfFeature {
    SxfFeature::Line {
        style: Styled { id, layer, color: 2.0, line_type: 1.0, line_width: 3.0 },
        x1: 0.0,
        y1: 0.0,
        x2,
        y2: 50.0,
    }
}

#[test]
fn unchanged_drawings_are_written_as_the_same_bytes() {
    for name in ["features", "nosheet", "sample"] {
        let bytes = read_fixture("p21", name);
        let written = write_p21(&parse_p21(&bytes).unwrap());
        assert_eq!(written.bytes, bytes, "{name}.p21");
    }
}

#[test]
fn a_changed_feature_keeps_its_number_and_drops_the_instances_only_it_used() {
    let bytes = read_fixture("p21", "sample");
    let mut drawing = parse_p21(&bytes).unwrap();
    // 部分図「平面図」の線分 #46 の終点を動かす
    let mut figure = drawing.document.sub_figures.get("平面図").unwrap().clone();
    let SxfFeature::Line { x2, .. } = &mut figure.members[0] else { panic!("線分 #46") };
    *x2 = 500.0;
    drawing.document.sub_figures.set("平面図".into(), figure);
    let written = write_p21(&drawing);
    let (before, after) = (instances(&bytes), instances(&written.bytes));
    // #46 は同じ番号で新しい TRIMMED_CURVE を参照し、元の #45 は寸法線（#83）が使う #44・#40 を残して消える
    assert_ne!(before[&46], after[&46]);
    assert!(!after.contains_key(&45) && !after.contains_key(&41), "{after:?}");
    for id in [40, 44, 83] {
        assert_eq!(before[&id], after[&id], "#{id}");
    }
    for (id, body) in &before {
        if ![41, 45, 46].contains(id) {
            assert_eq!(after.get(id), Some(body), "#{id} は変えていない");
        }
    }
    assert_reads_back(&drawing, &written);
}

#[test]
fn deleted_features_leave_their_lists_and_layers() {
    let bytes = read_fixture("p21", "sample");
    let mut drawing = parse_p21(&bytes).unwrap();
    // 寸法 #98 を消す（DIMENSION_CALLOUT_RELATIONSHIP はない。寸法線のレイヤ DIM から外れる）
    drawing.document.sheet_members.retain(|feature| feature.id() != 98.0);
    let written = write_p21(&drawing);
    let after = instances(&written.bytes);
    for id in [84, 93, 97, 98, 85, 86, 87, 89, 94, 95, 96] {
        assert!(!after.contains_key(&id), "#{id} を消す {after:?}");
    }
    // 寸法が使っていた見え方（#88 の矢印の名前、#92 の記号の見え方）は、ほかに使うものがないので消え、
    // 線分と共有する #31・#40・#44 は残る
    for id in [31, 40, 44] {
        assert!(after.contains_key(&id), "#{id}");
    }
    assert!(after[&134].starts_with("DRAWING_SHEET_REVISION('A1_horizontal',(#77,#113,#133)"), "{}", after[&134]);
    assert!(after[&122].contains("(#113)"), "{}", after[&122]);
    assert_reads_back(&drawing, &written);
}

#[test]
fn new_features_get_fresh_numbers_and_join_lists_and_layers() {
    let bytes = read_fixture("p21", "sample");
    let mut drawing = parse_p21(&bytes).unwrap();
    let doc = &mut drawing.document;
    doc.layers.push(LayerDefinition { id: 900_001.0, name: "追加".into(), visible: false });
    doc.sheet_members.push(line(900_002.0, 2.0, 300.0));
    doc.sheet_members.push(line(900_003.0, 3.0, 200.0));
    doc.sheet_members.push(SxfFeature::TextString {
        id: 900_004.0,
        layer: 1.0,
        color: 4.0,
        text: Annotation {
            font: 1.0,
            text: "追記".into(),
            x: 20.0,
            y: 30.0,
            height: 5.0,
            width: 10.0,
            spacing: 0.0,
            angle: 30.0,
            slant: 0.0,
            anchor: 5.0,
            direction: 1.0,
        },
    });
    doc.sub_figures.set(
        "部品".into(),
        SubFigure {
            feature: SfigOrg { id: 900_005.0, name: "部品".into(), flag: 4.0 },
            members: vec![line(900_006.0, 1.0, 10.0)],
        },
    );
    doc.sheet_members.push(SxfFeature::SfigLocate {
        id: 900_007.0,
        layer: 0.0,
        name: "部品".into(),
        x: 50.0,
        y: 60.0,
        angle: 15.0,
        ratio_x: 2.0,
        ratio_y: 2.0,
    });
    doc.composite_curves.push(CompositeCurve {
        feature: CompositeCurveOrg { id: 900_008.0, color: 5.0, line_type: 1.0, line_width: 1.0, visible: true },
        members: vec![SxfFeature::Polyline {
            style: Styled { id: 900_009.0, layer: 0.0, color: 5.0, line_type: 1.0, line_width: 1.0 },
            xs: vec![300.0, 340.0, 340.0, 300.0],
            ys: vec![300.0, 300.0, 340.0, 300.0],
        }],
    });
    doc.sheet_members.push(SxfFeature::FillAreaStyleColour {
        id: 900_010.0,
        layer: 1.0,
        color: 3.0,
        region: Region { out_id: 2.0, in_ids: vec![] },
    });
    let written = write_p21(&drawing);
    let (before, after) = (instances(&bytes), instances(&written.bytes));
    // 元のインスタンスで変わるのは、用紙の要素の並び（#134）とレイヤの割り当て（#120・#121・#122）と非表示の指定（#123）だけ
    for (id, body) in &before {
        if ![120, 121, 122, 123, 134].contains(id) {
            assert_eq!(after.get(id), Some(body), "#{id} は変えていない");
        }
    }
    // 新しいインスタンスは、元の最大の番号（#135）より大きい 140 から
    assert!(after.keys().filter(|id| !before.contains_key(id)).all(|id| *id >= 140));
    assert_reads_back(&drawing, &written);
}

#[test]
fn layers_sheets_and_colours_are_rewritten_when_changed() {
    let bytes = read_fixture("p21", "sample");
    let mut drawing = parse_p21(&bytes).unwrap();
    let doc = &mut drawing.document;
    doc.layers[0].name = "線（改）".into();
    doc.layers[1].visible = true;
    doc.user_colors[0].g = 64.0;
    if let Some(sheet) = &mut doc.sheet {
        sheet.sheet_type = 2.0;
        sheet.orient = 0.0;
        sheet.name = "改訂".into();
    }
    let written = write_p21(&drawing);
    let (before, after) = (instances(&bytes), instances(&written.bytes));
    for (id, body) in &before {
        if ![12, 121, 123, 132, 133, 134].contains(id) {
            assert_eq!(after.get(id), Some(body), "#{id} は変えていない");
        }
    }
    assert!(after[&134].starts_with("DRAWING_SHEET_REVISION('A2_vertical'"), "{}", after[&134]);
    assert_reads_back(&drawing, &written);
}

#[test]
fn a_source_cut_short_by_the_read_limits_is_written_anew() {
    let bytes = read_fixture("p21", "sample");
    let mut drawing = parse_p21(&bytes).unwrap();
    drawing.document.sheet_members.pop();
    let limits = zumen_core::ReadLimits { max_duration: Some(std::time::Duration::ZERO), ..Default::default() };
    let (written, _) = zumen_core::limits::with_limits(&limits, || write_p21(&drawing));
    assert_eq!(written.notices.first().map(|notice| notice.code.as_str()), Some("rewrite"));
}

#[test]
fn an_emptied_layer_leaves_invisibility_even_when_listed_before_it() {
    // INVISIBILITY を レイヤの割り当てより前に置いた P21（番号の順は読み取りの結果に影響しない）
    let text = String::from_utf8(read_fixture("p21", "sample")).unwrap();
    let text = text.replace("#123=INVISIBILITY((#122,#108));\n", "").replace(
        "#120=PRESENTATION_LAYER_ASSIGNMENT",
        "#119=INVISIBILITY((#122,#108));\n#120=PRESENTATION_LAYER_ASSIGNMENT",
    );
    let mut drawing = parse_p21(text.as_bytes()).unwrap();
    // レイヤ DIM（#122）の要素（寸法 #98 の寸法線、塗り #113）を消す
    drawing.document.sheet_members.retain(|feature| !matches!(feature.id(), 98.0 | 113.0));
    let written = write_p21(&drawing);
    let after = instances(&written.bytes);
    assert!(!after.contains_key(&122), "{after:?}");
    assert_eq!(after[&119], "INVISIBILITY((#108))");
    assert!(written.notices.iter().any(|notice| notice.code == "empty-layer"));
    let again = parse_p21(&written.bytes).unwrap();
    assert_eq!(again.document.sheet_members.len(), drawing.document.sheet_members.len());
}

#[test]
fn large_unread_instance_ids_do_not_collapse_new_references() {
    let bytes = read_fixture("p21", "sample");
    let text = String::from_utf8(bytes).unwrap().replace("DATA;", "DATA;\n#999999999999999999=UNKNOWN();");
    let mut drawing = parse_p21(text.as_bytes()).unwrap();
    drawing.document.sheet_members.push(line(0.0, 1.0, 10.0));
    let written = write_p21(&drawing);
    assert_reads_back(&drawing, &written);
    assert!(String::from_utf8_lossy(&written.bytes).contains("#999999999999999999=UNKNOWN();"));
}
