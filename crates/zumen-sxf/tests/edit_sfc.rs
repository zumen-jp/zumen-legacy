//! SFC のその場の書き直し: 読んだ SFC を変えずに書くと同じバイト列になり、変えたフィーチャだけが書き直される

mod common;

use common::read_fixture;
use std::collections::HashMap;
use zumen_sxf::{
    DrawingSheet, LayerDefinition, SfigOrg, Styled, SubFigure, SxfDocument, SxfDrawing, SxfFeature, parse_sfc,
    sxf_to_scene, write_sfc,
};

/// SFC のフィーチャの囲み（/*SXF … SXF*/）を、インスタンス番号ごとに取り出す
fn records(bytes: &[u8]) -> HashMap<String, String> {
    let text = encoding_free(bytes);
    let mut out = HashMap::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("/*SXF") {
        let after = &rest[start..];
        let Some(end) = after.find("*/") else { break };
        let record = &after[..end + 2];
        if let Some(id) = record.split('#').nth(1).and_then(|tail| tail.split(|c: char| !c.is_ascii_digit()).next()) {
            out.insert(id.to_string(), record.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

/// バイト列を 1 バイト 1 文字の文字列にする（CP932 の文字列もバイトのまま比べる）
fn encoding_free(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// 変えたフィーチャ（番号）以外の囲みが、元と同じ文字列のまま残っている
fn assert_unchanged_except(before: &[u8], after: &[u8], changed: &[&str]) {
    let (before, after) = (records(before), records(after));
    for (id, record) in &before {
        if changed.contains(&id.as_str()) {
            continue;
        }
        assert_eq!(after.get(id), Some(record), "#{id} は変えていない");
    }
}

/// 書いた SFC を読み直した図面が、書いた図面と同じ（新しいフィーチャの番号は書いた番号に読み替える）
fn assert_reads_back(drawing: &SxfDrawing, written: &zumen_sxf::SxfWritten) {
    let again = parse_sfc(&written.bytes).unwrap();
    let mut expected = drawing.document.clone();
    renumber(&mut expected, written);
    let mut actual = again.document.clone();
    expected.counts = Vec::new();
    actual.counts = Vec::new();
    assert_eq!(actual, expected);
}

/// 図面の新しいフィーチャ（番号が 0 のもの）に、書いた番号を振る
fn renumber(doc: &mut SxfDocument, written: &zumen_sxf::SxfWritten) {
    let mut fresh = written.ids.iter().filter(|(old, new)| old != new).map(|(_, new)| *new);
    let mut set = |id: &mut f64| {
        if *id == 0.0 {
            *id = fresh.next().expect("新しい番号");
        }
    };
    for layer in &mut doc.layers {
        set(&mut layer.id);
    }
    for curve in &mut doc.composite_curves {
        for member in &mut curve.members {
            set_feature(member, &mut set);
        }
        set(&mut curve.feature.id);
    }
    let names: Vec<String> = doc.sub_figures.values().map(|figure| figure.feature.name.clone()).collect();
    for name in names {
        let mut figure = doc.sub_figures.get(&name).unwrap().clone();
        for member in &mut figure.members {
            set_feature(member, &mut set);
        }
        set(&mut figure.feature.id);
        doc.sub_figures.set(name, figure);
    }
    for member in &mut doc.sheet_members {
        set_feature(member, &mut set);
    }
    if let Some(sheet) = &mut doc.sheet {
        set(&mut sheet.id);
    }
}

fn set_feature(feature: &mut SxfFeature, set: &mut impl FnMut(&mut f64)) {
    match feature {
        SxfFeature::Line { style, .. }
        | SxfFeature::Circle { style, .. }
        | SxfFeature::Polyline { style, .. }
        | SxfFeature::Arc { style, .. } => set(&mut style.id),
        SxfFeature::TextString { id, .. }
        | SxfFeature::SfigLocate { id, .. }
        | SxfFeature::FillAreaStyleColour { id, .. } => set(id),
        _ => {}
    }
}

fn line(id: f64, layer: f64, x2: f64) -> SxfFeature {
    SxfFeature::Line {
        style: Styled { id, layer, color: 1.0, line_type: 1.0, line_width: 1.0 },
        x1: 0.0,
        y1: 0.0,
        x2,
        y2: 50.0,
    }
}

#[test]
fn unchanged_drawings_are_written_as_the_same_bytes() {
    for name in ["features", "nosheet", "sample"] {
        let bytes = read_fixture("sfc", name);
        let written = write_sfc(&parse_sfc(&bytes).unwrap());
        assert_eq!(written.bytes, bytes, "{name}.sfc");
        assert!(written.notices.is_empty(), "{name}.sfc {:?}", written.notices);
    }
}

#[test]
fn a_changed_feature_is_rewritten_in_place_with_its_number() {
    let bytes = read_fixture("sfc", "sample");
    let mut drawing = parse_sfc(&bytes).unwrap();
    // 部分図「平面図」の線分 #80 の終点を動かす
    let mut figure = drawing.document.sub_figures.get("平面図").unwrap().clone();
    let SxfFeature::Line { x2, .. } = &mut figure.members[0] else { panic!("線分 #80") };
    *x2 = 1234.5;
    drawing.document.sub_figures.set("平面図".into(), figure);
    let written = write_sfc(&drawing);
    assert_unchanged_except(&bytes, &written.bytes, &["80"]);
    let text = encoding_free(&written.bytes);
    assert!(text.contains("#80 = line_feature('1','8','1','3','0','0','1234.5','0')"), "{text}");
    assert_reads_back(&drawing, &written);
}

#[test]
fn deleted_features_and_sub_figures_are_removed() {
    let bytes = read_fixture("sfc", "sample");
    let mut drawing = parse_sfc(&bytes).unwrap();
    let doc = &mut drawing.document;
    // 部分図「平面図」の文字 #90 と、複合図形 G（#210、構成要素 #200）とその配置 #240 を消す
    doc.sheet_members.retain(|feature| feature.id() != 240.0);
    let mut figures = zumen_sxf::SubFigures::default();
    for figure in doc.sub_figures.values().filter(|figure| figure.feature.name != "G") {
        let mut figure = figure.clone();
        figure.members.retain(|feature| feature.id() != 90.0);
        figures.set(figure.feature.name.clone(), figure);
    }
    doc.sub_figures = figures;
    let written = write_sfc(&drawing);
    let after = records(&written.bytes);
    for id in ["90", "200", "210", "240"] {
        assert!(!after.contains_key(id), "#{id} を消す");
    }
    assert_unchanged_except(&bytes, &written.bytes, &["90", "200", "210", "240"]);
    assert_reads_back(&drawing, &written);
}

#[test]
fn new_features_get_fresh_numbers_in_their_definitions() {
    let bytes = read_fixture("sfc", "sample");
    let mut drawing = parse_sfc(&bytes).unwrap();
    let doc = &mut drawing.document;
    // 用紙に線分、新しいレイヤ、新しい複合図形とその配置、新しい複合曲線と塗り
    doc.layers.push(LayerDefinition { id: 0.0, name: "追加".into(), visible: true });
    doc.sheet_members.push(line(0.0, 3.0, 300.0));
    doc.sub_figures.set(
        "新しい部品".into(),
        SubFigure {
            feature: SfigOrg { id: 0.0, name: "新しい部品".into(), flag: 4.0 },
            members: vec![line(0.0, 1.0, 10.0)],
        },
    );
    doc.sheet_members.push(SxfFeature::SfigLocate {
        id: 0.0,
        layer: 1.0,
        name: "新しい部品".into(),
        x: 50.0,
        y: 60.0,
        angle: 0.0,
        ratio_x: 1.0,
        ratio_y: 1.0,
    });
    let written = write_sfc(&drawing);
    assert_unchanged_except(&bytes, &written.bytes, &[]);
    let after = records(&written.bytes);
    // 元の最大の番号（#250）より大きい 10 の倍数を順に振る
    for id in ["260", "270", "280", "290", "300"] {
        assert!(after.contains_key(id), "#{id} {:?}", written.ids);
    }
    assert_reads_back(&drawing, &written);
    // 書いた SFC は表示もできる（新しいレイヤのコード 3 は最後のレイヤ）
    let scene = sxf_to_scene(&parse_sfc(&written.bytes).unwrap());
    assert!(scene.layers.iter().any(|layer| layer.id == 3 && layer.name == "追加"));
}

#[test]
fn the_sheet_and_tables_are_rewritten_when_changed() {
    let bytes = read_fixture("sfc", "features");
    let mut drawing = parse_sfc(&bytes).unwrap();
    let doc = &mut drawing.document;
    doc.sheet = Some(DrawingSheet { name: "改訂".into(), ..doc.sheet.clone().unwrap() });
    doc.layers[2].visible = true;
    doc.user_colors[0].g = 64.0;
    if let Some(title) = &mut doc.title {
        title.values[4].value = "No.2".into();
    }
    let written = write_sfc(&drawing);
    assert_unchanged_except(&bytes, &written.bytes, &["1130", "1160", "20", "1120"]);
    assert_reads_back(&drawing, &written);
}

#[test]
fn a_source_cut_short_by_the_read_limits_is_written_anew() {
    let bytes = read_fixture("sfc", "sample");
    let drawing = parse_sfc(&bytes).unwrap();
    let limits = zumen_core::ReadLimits { max_duration: Some(std::time::Duration::ZERO), ..Default::default() };
    let (written, _) = zumen_core::limits::with_limits(&limits, || write_sfc(&drawing));
    assert_eq!(written.notices.first().map(|notice| notice.code.as_str()), Some("rewrite"));
    assert_ne!(written.bytes, bytes);
    let again = parse_sfc(&written.bytes).unwrap();
    assert_eq!(again.document.sheet_members.len(), drawing.document.sheet_members.len());
}

#[test]
fn new_numbers_do_not_repeat_numbers_kept_from_the_drawing() {
    let bytes = read_fixture("sfc", "sample");
    let mut drawing = parse_sfc(&bytes).unwrap();
    // 1 本目は図面の番号（元のファイルにない #260）のまま、2 本目は振り直す番号が #260 と重ならない
    drawing.document.sheet_members.push(line(260.0, 1.0, 10.0));
    drawing.document.sheet_members.push(line(0.0, 1.0, 20.0));
    let written = write_sfc(&drawing);
    let ids: Vec<f64> =
        written.ids.iter().filter(|(old, new)| old != new || *old == 260.0).map(|(_, new)| *new).collect();
    assert_eq!(&ids[ids.len() - 2..], &[260.0, 270.0]);
    assert_reads_back(&drawing, &written);
}
