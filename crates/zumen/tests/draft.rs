//! 作図キット（決定 0018）: 見本の仕様をどの形式にも書き、zumen で読み直して、用紙の上に描いた文字・寸法・表が
//! 仕様のとおりの位置と大きさにあることを確かめる。仕様の読み違いは、場所と code で返す。

use zumen::extract::{EntityFilter, scene_texts};
use zumen::{DraftOptions, ReadDrawingOptions, Scene, WriteOptions};

const SPEC: &str = include_str!("../../zumen-draft/examples/sheets.json");

fn draft(to: &str, version: Option<&str>, sheet: Option<&str>) -> zumen::Converted {
    let options =
        DraftOptions { write: WriteOptions { version: version.map(str::to_string) }, sheet: sheet.map(str::to_string) };
    zumen::draft(SPEC, to, &options).unwrap_or_else(|error| panic!("{to}: {error}"))
}

fn read(bytes: &[u8], view: Option<&str>) -> Scene {
    let options = ReadDrawingOptions { view: view.map(str::to_string), ..ReadDrawingOptions::default() };
    zumen::read(bytes, &options).unwrap()
}

/// 用紙の上の文字（文字列 → 基準点と高さ）
fn texts(scene: &Scene) -> Vec<(String, f64, f64, f64)> {
    scene_texts(scene, &EntityFilter::default())
        .into_iter()
        .map(|(_, text)| (text.text, text.bbox.min_x, text.bbox.min_y, text.height))
        .collect()
}

fn find<'a>(texts: &'a [(String, f64, f64, f64)], wanted: &str) -> &'a (String, f64, f64, f64) {
    texts.iter().find(|(text, ..)| text == wanted).unwrap_or_else(|| {
        let all: Vec<&str> = texts.iter().map(|(text, ..)| text.as_str()).collect();
        panic!("{wanted} がありません: {all:?}")
    })
}

/// 読み取りの文字の高さは字形の em（TEXT の高さの 1/0.88 か 1/0.72 倍）なので、用紙の 2.5 mm の文字は 2.5〜3.5 mm
fn is_paper_2_5(height: f64) -> bool {
    (2.5..3.6).contains(&height)
}

/// シートごとに、どの形式でも同じ文字が同じ大きさで用紙の中にある
fn check_sheets(scenes: &[(&str, Scene)], label: &str) {
    for (sheet, scene) in scenes {
        let texts = texts(scene);
        match *sheet {
            "A01-1" => {
                find(&texts, "設計概要書");
                find(&texts, "1,029.18 ㎡");
                find(&texts, "(仮称)サンプル計画 新築工事");
                let number = find(&texts, "A01-1");
                assert!(number.1 > 370.0 && number.2 < 30.0, "{label}: 図面番号は表題欄の下 {number:?}");
            }
            "A05-1" => {
                let area = find(&texts, "敷地面積 2,096.00㎡");
                assert!(is_paper_2_5(area.3), "{label}: 1:250 のビューでも文字は用紙の 2.5 mm {area:?}");
                let dimension = find(&texts, "12,000");
                assert!(dimension.1 > 25.0 && dimension.1 < 365.0, "{label}: 寸法はビューポートの中 {dimension:?}");
                find(&texts, "48,000");
                find(&texts, "S=1:250");
                find(&texts, "23.087");
            }
            "A10-1" => {
                let span = find(&texts, "21,600");
                assert!(is_paper_2_5(span.3), "{label}: 寸法の値は用紙の 2.5 mm {span:?}");
                assert_eq!(texts.iter().filter(|(text, ..)| text == "7,200").count(), 5, "{label}");
                find(&texts, "X4");
                find(&texts, "事務室");
                find(&texts, "205.9㎡");
            }
            "A20-1" => {
                find(&texts, "GL±0");
                find(&texts, "16,600");
                find(&texts, "S=1:200");
            }
            _ => unreachable!(),
        }
    }
}

const SHEETS: [&str; 4] = ["A01-1", "A05-1", "A10-1", "A20-1"];

#[test]
fn drafts_every_sheet_as_dxf_and_dwg_with_layouts() {
    for (to, version) in [("dxf", None), ("dxf", Some("2000")), ("dxf", Some("2018")), ("dwg", None)] {
        let label = format!("{to} {version:?}");
        let drafted = draft(to, version, None);
        let scene = read(&drafted.bytes, None);
        let views: Vec<&str> = scene.views.iter().flatten().map(|view| view.id.as_str()).collect();
        assert_eq!(views, ["Model", "A01-1", "A05-1", "A10-1", "A20-1"], "{label}");
        let scenes: Vec<(&str, Scene)> =
            SHEETS.iter().map(|sheet| (*sheet, read(&drafted.bytes, Some(sheet)))).collect();
        check_sheets(&scenes, &label);
        assert!(drafted.notices.is_empty(), "{label}: {:?}", drafted.notices);
        // 寸法は DIMENSION のまま（通り芯の間の 3 + 1 と 2 + 1）
        let plan = &scenes[2].1;
        assert_eq!(plan.entities.iter().filter(|entity| entity.kind.name() == "dimension").count(), 7, "{label}");
    }
}

#[test]
fn drafts_one_sheet_at_a_time_as_jww_and_sxf() {
    for to in ["jww", "sfc", "p21"] {
        let scenes: Vec<(&str, Scene)> =
            SHEETS.iter().map(|sheet| (*sheet, read(&draft(to, None, Some(sheet)).bytes, None))).collect();
        check_sheets(&scenes, to);
    }
}

#[test]
fn drafts_r12_with_the_documented_losses() {
    let drafted = draft("dxf", Some("R12"), Some("A05-1"));
    let codes: Vec<&str> = drafted.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert!(codes.contains(&"version-hatch"), "{codes:?}");
    check_sheets(&[("A05-1", read(&drafted.bytes, None))], "R12");
}

#[test]
fn drafts_the_same_bytes_every_time() {
    assert_eq!(draft("dxf", None, None).bytes, draft("dxf", None, None).bytes);
    assert_eq!(draft("dwg", None, None).bytes, draft("dwg", None, None).bytes);
}

fn error(spec: &str) -> zumen::draft::DraftError {
    zumen::draft(spec, "dxf", &DraftOptions::default()).expect_err("仕様の誤りを返す")
}

#[test]
fn names_the_place_and_code_of_mistakes() {
    let sheet = |items: &str| format!(r#"{{"sheets": [{{"name": "S", "items": [{items}]}}]}}"#);
    let unknown = error(&sheet(r#"{"type": "cloud"}"#));
    assert_eq!((unknown.code, unknown.path.as_str()), ("unknown-type", "sheets[0].items[0]"));
    let key = error(&sheet(r#"{"type": "line", "from": [0, 0], "to": [1, 1], "weight": 2}"#));
    assert_eq!((key.code, key.path.as_str()), ("unknown-key", "sheets[0].items[0]"));
    let missing = error(&sheet(r#"{"type": "text", "at": [0, 0]}"#));
    assert_eq!(missing.code, "invalid-spec");
    let block = error(&sheet(r#"{"type": "insert", "block": "DOOR", "at": [0, 0]}"#));
    assert_eq!(block.code, "missing-definition");
    let line_type = error(&sheet(r#"{"type": "line", "from": [0, 0], "to": [1, 1], "lineType": "WAVY"}"#));
    assert_eq!(line_type.code, "missing-definition");
    let pattern = error(&sheet(r#"{"type": "hatch", "boundaries": [[[0, 0], [1, 0], [1, 1]]], "pattern": "BRICKS"}"#));
    assert_eq!(pattern.code, "missing-definition");
    let span =
        error(&sheet(r#"{"type": "table", "at": [0, 0], "columns": [10], "cells": [[{"text": "a", "colspan": 2}]]}"#));
    assert_eq!((span.code, span.path.as_str()), ("invalid-value", "sheets[0].items[0].cells[0][0]"));
    let nested = error(&sheet(r#"{"type": "group", "items": [{"type": "circle", "center": [0, 0], "radius": -1}]}"#));
    assert_eq!((nested.code, nested.path.as_str()), ("invalid-value", "sheets[0].items[0].items[0]"));
    let twice = error(r#"{"sheets": [{"name": "S"}, {"name": "s"}]}"#);
    assert_eq!(twice.code, "invalid-spec");
    assert_eq!(error(r#"{"sheets": []}"#).code, "invalid-spec");
    assert_eq!(error(r#"{"units": "inch", "sheets": [{"name": "S"}]}"#).code, "invalid-value");
    let format = zumen::draft(&sheet(""), "pdf", &DraftOptions::default()).unwrap_err();
    assert_eq!(format.code, "unsupported-format");
}

#[test]
fn reports_views_whose_model_regions_overlap() {
    let spec = r#"{"sheets": [{"name": "S", "views": [
        {"scale": 100, "viewport": [10, 10, 100, 100], "center": [0, 0], "items": [{"type": "circle", "center": [0, 0], "radius": 1000}]},
        {"scale": 100, "viewport": [110, 10, 200, 100], "center": [2000, 0], "items": []}
    ]}]}"#;
    let drafted = zumen::draft(spec, "dxf", &DraftOptions::default()).unwrap();
    assert!(drafted.notices.iter().any(|notice| notice.code == "view-overlap"), "{:?}", drafted.notices);
}
