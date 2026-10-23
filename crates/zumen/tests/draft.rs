//! 作図キット（決定 0018）: 見本の仕様をどの形式にも書き、zumen で読み直して、用紙の上に描いた文字・寸法・表が
//! 仕様のとおりの位置と大きさにあることを確かめる。仕様の読み違いは、場所と code で返す。

use std::collections::BTreeSet;
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

/// 読み取りの文字の高さは字形の em。DXF・DWG は TEXT の高さの 1/0.88 か 1/0.72 倍なので、用紙の 2.5 mm の文字は
/// 2.5〜3.5 mm。SXF・JWW は文字範囲の高さが TEXT の高さ（decision 0019）なので 2.5 mm
fn is_paper_2_5(height: f64) -> bool {
    (2.5 - 1e-9..3.6).contains(&height)
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
    let dimension = |item: &str| error(&sheet(&format!(r#"{{"type": "dimension", {item}}}"#))).code;
    assert_eq!(dimension(r#""kind": "radius", "center": [0, 0], "radius": 1, "from": [0, 0]"#), "unknown-key");
    assert_eq!(dimension(r#""kind": "diameter", "center": [0, 0]"#), "invalid-spec");
    assert_eq!(dimension(r#""kind": "angular", "from": [1, 0], "to": [0, 1]"#), "invalid-spec");
    let both = r#""kind": "angular", "center": [0, 0], "from": [1, 0], "to": [0, 1], "at": [1, 1], "radius": 1"#;
    assert_eq!(dimension(both), "invalid-value");
    assert_eq!(dimension(r#""kind": "angular", "center": [0, 0], "from": [1, 0], "to": [2, 0]"#), "invalid-value");
    assert_eq!(dimension(r#""kind": "ordinate", "from": [1, 0], "to": [2, 0]"#), "invalid-value");
    let twice = error(r#"{"sheets": [{"name": "S"}, {"name": "s"}]}"#);
    assert_eq!(twice.code, "invalid-spec");
    assert_eq!(error(r#"{"sheets": []}"#).code, "invalid-spec");
    assert_eq!(error(r#"{"units": "inch", "sheets": [{"name": "S"}]}"#).code, "invalid-value");
    let format = zumen::draft(&sheet(""), "pdf", &DraftOptions::default()).unwrap_err();
    assert_eq!(format.code, "unsupported-format");
}

#[test]
fn rejects_empty_and_model_sheet_names_before_writing() {
    for name in ["", "   ", "Model", "mOdEl", " Model "] {
        let spec = serde_json::json!({"sheets": [{"name": name, "items": [
            {"type": "text", "at": [10, 10], "text": "KEEP-ME"}
        ]}]})
        .to_string();
        for to in ["dxf", "dwg"] {
            let error = zumen::draft(&spec, to, &DraftOptions::default()).unwrap_err();
            assert_eq!((error.code, error.path.as_str()), ("invalid-value", "sheets[0].name"), "{to}: {name:?}");
        }
    }
}

#[test]
fn preserves_literal_text_and_attribute_values_when_drafting_and_drafting_into() {
    let literal = "%%c %%d %%p %%u %%123 %%% ^I ^2 ^ \\P {raw}\nTAIL";
    let spec = serde_json::json!({
        "blocks": [{"name": "TITLE", "attributes": [
            {"tag": "DEFAULT", "at": [0, 0], "value": "default %%c ^I"},
            {"tag": "INSERT", "at": [0, 5]},
            {"tag": "FIELD", "at": [0, 10]}
        ]}],
        "sheets": [{"name": "S", "fields": {"FIELD": "field %%p ^ "}, "items": [
            {"type": "text", "at": [10, 10], "text": literal},
            {"type": "insert", "block": "TITLE", "at": [20, 20], "attributes": {"INSERT": "insert %%d ^2"}}
        ]}]
    })
    .to_string();
    for to in ["dxf", "dwg"] {
        for into in [false, true] {
            let drafted = if into {
                let template = zumen::create(to, "mm", &[]).unwrap();
                draft_into(&template.bytes, &spec)
            } else {
                zumen::draft(&spec, to, &DraftOptions::default()).unwrap()
            };
            let found = texts(&read(&drafted.bytes, Some("S")));
            for wanted in [literal, "default %%c ^I", "insert %%d ^2", "field %%p ^ "] {
                assert!(found.iter().any(|(text, ..)| text == wanted), "{to} into={into}: {wanted:?} in {found:?}");
            }
        }
        let spec = serde_json::json!({"sheets": [{
            "name": "A3", "fields": {"DWGNAME": "template %%c ^I ^ ", "DWGNO": "template %%d"}
        }]})
        .to_string();
        let drafted = draft_into(&template(to), &spec);
        let found = texts(&read(&drafted.bytes, Some("A3")));
        for wanted in ["template %%c ^I ^ ", "template %%d"] {
            assert!(found.iter().any(|(text, ..)| text == wanted), "{to}: {wanted:?} in {found:?}");
        }
    }
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

// ---- draft_into: 受け取った図面にその場で描く ----

use zumen::dxf::DxfDocument;

/// テンプレート: モデルの線と、属性のある表題欄のブロックを置いたレイアウト A3
fn template(to: &str) -> Vec<u8> {
    let operations = zumen::edit::parse_operations(
        r#"[
            {"op": "add", "kind": "line", "start": [0, -50000], "end": [1000, -50000]},
            {"op": "add-block", "name": "TITLE"},
            {"op": "add", "kind": "line", "container": "block:TITLE", "start": [0, 0], "end": [100, 0]},
            {"op": "add", "kind": "attribute-definition", "container": "block:TITLE", "tag": "DWGNAME", "text": "未定", "position": [5, 5], "height": 3},
            {"op": "add", "kind": "attribute-definition", "container": "block:TITLE", "tag": "DWGNO", "text": "-", "position": [5, 15], "height": 3},
            {"op": "add-layout", "name": "A3", "paperSize": [420, 297]},
            {"op": "add", "kind": "block-reference", "container": "layout:A3", "block": "TITLE", "position": [300, 10], "attributes": {"DWGNO": "-"}}
        ]"#,
    )
    .unwrap();
    zumen::create(to, "mm", &operations).unwrap_or_else(|error| panic!("{to}: {error}")).bytes
}

fn document(to: &str, bytes: &[u8]) -> DxfDocument {
    match to {
        "dxf" => zumen::dxf::parse_dxf(bytes),
        _ => zumen::dwg::read_dwg(bytes).unwrap(),
    }
}

fn draft_into(bytes: &[u8], spec: &str) -> zumen::Converted {
    zumen::draft_into(bytes, spec, &zumen::DraftIntoOptions::default()).unwrap_or_else(|error| panic!("{error}"))
}

/// テンプレートの図形はハンドルごと残り、版も同じで、描いたものと表題欄の属性を zumen で読み直せる
#[test]
fn drafts_into_a_template_layout_and_fills_its_title_block() {
    let spec = r#"{
        "layers": [{"name": "A-WALL", "color": 7, "lineWidth": 0.5}],
        "sheets": [{
            "name": "A3",
            "fields": {"DWGNAME": "1階平面図", "DWGNO": "A-101"},
            "views": [{"viewport": [20, 20, 280, 280], "scale": 100, "items": [
                {"type": "rectangle", "from": [0, 0], "to": [10000, 8000], "layer": "A-WALL"},
                {"type": "dimension", "from": [0, 0], "to": [10000, 0], "offset": -10},
                {"type": "hatch", "boundaries": [[[0, 0], [2000, 0], [2000, 2000], [0, 2000]]], "pattern": "ANSI31"},
                {"type": "text", "at": [5000, 4000], "text": "事務室", "align": "middle-center", "fit": 10, "height": 5}
            ]}],
            "items": [{"type": "text", "at": [30, 285], "text": "平面図", "height": 5}]
        }]
    }"#;
    for to in ["dxf", "dwg"] {
        let before = template(to);
        let drafted = draft_into(&before, spec);
        let (old, new) = (document(to, &before), document(to, &drafted.bytes));
        assert_eq!(old.version, new.version, "{to}");
        for entity in &old.entities {
            assert!(
                new.entities
                    .iter()
                    .any(|other| other.base.handle == entity.base.handle && other.base.kind == entity.base.kind),
                "{to}: {} {} が残っていません",
                entity.base.kind,
                entity.base.handle
            );
        }
        assert!(new.entities.len() > old.entities.len(), "{to}");
        // レイアウトは増えず、A3 に描いた
        let scene = read(&drafted.bytes, None);
        let views: Vec<&str> = scene.views.iter().flatten().map(|view| view.id.as_str()).collect();
        assert_eq!(views, ["Model", "Layout1", "A3"], "{to}");
        let sheet = read(&drafted.bytes, Some("A3"));
        let texts = texts(&sheet);
        find(&texts, "平面図");
        find(&texts, "1階平面図");
        find(&texts, "A-101");
        assert!(texts.iter().all(|(text, ..)| text != "未定"), "{to}: {texts:?}");
        let dimension = find(&texts, "10,000");
        assert!(is_paper_2_5(dimension.3), "{to}: 寸法の値は用紙の 2.5 mm {dimension:?}");
        assert!(dimension.1 > 20.0 && dimension.1 < 280.0, "{to}: 寸法はビューポートの中 {dimension:?}");
        let room = find(&texts, "事務室");
        assert!(room.1 > 20.0 && room.1 < 280.0, "{to}: {room:?}");
        let kinds: Vec<&str> = sheet.entities.iter().map(|entity| entity.kind.name()).collect();
        assert!(kinds.contains(&"dimension") && kinds.contains(&"hatch"), "{to}: {kinds:?}");
    }
}

#[test]
fn explicit_paper_resizes_an_existing_template_layout() {
    let operations = zumen::edit::parse_operations(
        r#"[
        {"op": "add-layout", "name": "S", "paperSize": [100, 100], "margins": [1, 2, 3, 4]}
    ]"#,
    )
    .unwrap();
    for to in ["dxf", "dwg"] {
        let template = zumen::create(to, "mm", &operations).unwrap();
        let before = document(to, &template.bytes);
        let old = before.layouts.iter().find(|layout| layout.name == "S").unwrap();
        for (paper, expected) in [("", (100.0, 100.0)), (r#", "paper": "A3""#, (420.0, 297.0))] {
            let spec = format!(r#"{{"sheets": [{{"name": "S"{paper}, "frame": {{}}}}]}}"#);
            let drafted = draft_into(&template.bytes, &spec);
            let doc = document(to, &drafted.bytes);
            let sheet = doc.layouts.iter().find(|layout| layout.name == "S").unwrap();
            assert_eq!((sheet.paper_width, sheet.paper_height), expected, "{to}: {paper}");
            assert_eq!(sheet.handle, old.handle, "{to}: preserve layout identity");
            assert_eq!(sheet.margins, old.margins, "{to}: preserve print margins");
            let overall = doc
                .entities
                .iter()
                .find_map(|entity| match &entity.data {
                    zumen::dxf::DxfEntityData::Viewport(viewport)
                        if entity.base.block.as_deref() == Some(&sheet.block) && viewport.id == 1.0 =>
                    {
                        Some(viewport)
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!((overall.width, overall.height), expected, "{to}: whole-sheet viewport");
        }
    }
}

/// 見本の仕様をテンプレートに描くと、用紙ごとにレイアウトが足され、draft と同じ文字が同じ場所にある
#[test]
fn drafts_the_sample_into_a_template_with_new_layouts() {
    for to in ["dxf", "dwg"] {
        let drafted = draft_into(&template(to), SPEC);
        let scene = read(&drafted.bytes, None);
        let views: Vec<&str> = scene.views.iter().flatten().map(|view| view.id.as_str()).collect();
        assert_eq!(views, ["Model", "Layout1", "A3", "A01-1", "A05-1", "A10-1", "A20-1"], "{to}");
        let scenes: Vec<(&str, Scene)> =
            SHEETS.iter().map(|sheet| (*sheet, read(&drafted.bytes, Some(sheet)))).collect();
        check_sheets(&scenes, &format!("draft_into {to}"));
        let plan = &scenes[2].1;
        assert_eq!(plan.entities.iter().filter(|entity| entity.kind.name() == "dimension").count(), 7, "{to}");
    }
}

#[test]
fn refuses_formats_that_cannot_be_edited_and_names_the_spec_place_of_edit_errors() {
    let options = zumen::DraftIntoOptions {
        read: ReadDrawingOptions { format: Some("jwc".into()), ..ReadDrawingOptions::default() },
        ..zumen::DraftIntoOptions::default()
    };
    let error = zumen::draft_into(b"jw_cad(c)data", SPEC, &options).unwrap_err();
    assert_eq!(error.code, "unsupported-format");
    // テンプレートにある同じ名前のブロックを使うので、その定義にない属性は編集が断り、仕様の場所で返す
    let spec = r#"{
        "blocks": [{"name": "TITLE", "items": [], "attributes": [{"tag": "OTHER", "at": [0, 0]}]}],
        "sheets": [{"name": "A3", "items": [{"type": "insert", "block": "TITLE", "at": [0, 0], "attributes": {"OTHER": "x"}}]}]
    }"#;
    let error = zumen::draft_into(&template("dxf"), spec, &zumen::DraftIntoOptions::default()).unwrap_err();
    assert_eq!((error.code, error.path.as_str()), ("invalid-value", "sheets[0].items[0]"), "{error}");
}

/// 同じ TTC に含まれる別の書体や、幅・高さ・傾きの異なるスタイルを再利用すると、仕様と違う文字になる。
#[test]
fn drafts_into_with_the_requested_font_family_and_plain_style() {
    for to in ["dxf", "dwg"] {
        for (font, width, height, oblique, bigfont, reuse) in [
            ("MS Gothic", 1.0, 0.0, 0.0, "", true),
            ("MS PGothic", 1.0, 0.0, 0.0, "", false),
            ("MS Gothic", 0.5, 0.0, 0.0, "", false),
            ("MS Gothic", 1.0, 8.0, 0.0, "", false),
            ("MS Gothic", 1.0, 0.0, 15.0, "", false),
            ("MS Gothic", 1.0, 0.0, 0.0, "extfont.shx", false),
        ] {
            let operations = serde_json::json!([{
                "op": "add-text-style", "name": "TEMPLATE-FONT", "font": font,
                "widthFactor": width, "height": height, "oblique": oblique, "bigFont": bigfont,
            }]);
            let operations = zumen::edit::operations_from_json(&operations).unwrap();
            let template = zumen::create(to, "mm", &operations).unwrap().bytes;
            let spec = r#"{"font":"MS Gothic","sheets":[{"name":"S","items":[{"type":"text","at":[10,10],"text":"FONT-CHECK"}]}]}"#;
            let drafted = draft_into(&template, spec);
            let doc = document(to, &drafted.bytes);
            let name = doc
                .entities
                .iter()
                .find_map(|entity| match &entity.data {
                    zumen::dxf::DxfEntityData::Text(text) if text.text == "FONT-CHECK" => Some(&text.style),
                    _ => None,
                })
                .expect("drafted text");
            let style = doc.styles.iter().find(|style| &style.name == name).expect("text style");
            assert_eq!(style.font_family.as_deref(), Some("MS Gothic"), "{to}: {font}/{width}");
            assert_eq!(
                (style.width, style.height, style.oblique, style.bigfont.as_str()),
                (1.0, 0.0, 0.0, ""),
                "{to}: {font}/{width}"
            );
            assert_eq!(style.name == "TEMPLATE-FONT", reuse, "{to}: {font}/{width}");
        }
    }
}

#[test]
fn drafts_into_with_case_variant_font_names_without_duplicate_styles() {
    let spec = r#"{"font":"MS Gothic","sheets":[{"name":"S","items":[
        {"type":"text","at":[10,10],"text":"A"},
        {"type":"text","at":[20,10],"text":"B","font":"ms gothic"}
    ]}]}"#;
    for to in ["dxf", "dwg"] {
        let bytes = zumen::create(to, "mm", &[]).unwrap().bytes;
        let drafted = draft_into(&bytes, spec);
        let doc = document(to, &drafted.bytes);
        let families: Vec<_> = doc
            .entities
            .iter()
            .filter_map(|entity| match &entity.data {
                zumen::dxf::DxfEntityData::Text(text) if matches!(text.text.as_str(), "A" | "B") => Some(
                    doc.styles
                        .iter()
                        .find(|style| style.name == text.style)
                        .unwrap()
                        .font_family
                        .as_deref()
                        .unwrap()
                        .to_ascii_uppercase(),
                ),
                _ => None,
            })
            .collect();
        assert_eq!(families, ["MS GOTHIC", "MS GOTHIC"], "{to}");
    }
}

/// 角度・半径・直径寸法（決定 0018 の目標 1）: どの形式に書いても、draft_into で描いても、読み直すと寸法として描かれ、
/// 値は度・R・直径の記号付き
#[test]
fn drafts_angular_radius_and_diameter_dimensions_in_every_format() {
    let spec = r#"{"sheets": [{"name": "S", "paper": "A3", "views": [{
        "scale": 100, "viewport": [20, 20, 400, 277], "center": [10000, 5000],
        "items": [
            {"type": "line", "from": [0, 0], "to": [6000, 0]},
            {"type": "line", "from": [0, 0], "to": [3000, 4000]},
            {"type": "dimension", "kind": "angular", "center": [0, 0], "from": [6000, 0], "to": [3000, 4000], "radius": 3000},
            {"type": "dimension", "kind": "angular", "center": [0, 0], "from": [6000, 0], "to": [3000, 4000], "at": [-2000, -1000], "precision": 1},
            {"type": "circle", "center": [12000, 5000], "radius": 1500},
            {"type": "dimension", "kind": "radius", "center": [12000, 5000], "radius": 1500},
            {"type": "circle", "center": [17000, 5000], "radius": 1500},
            {"type": "dimension", "kind": "diameter", "center": [17000, 5000], "radius": 1500, "angle": 0, "text": "<> 穴"}
        ]}]}]}"#;
    let check = |label: &str, bytes: &[u8], view: Option<&str>, dimensions: bool| {
        let scene = read(bytes, view);
        let texts = texts(&scene);
        // SXF の文字は CP932 で、直径の記号（Ø）がない（書き出しが encoding で知らせる）
        let diameter = if label == "sfc" { "?3,000 穴" } else { "Ø3,000 穴" };
        for text in ["53°", "306.9°", "R1,500", diameter] {
            let found = find(&texts, text);
            assert!(is_paper_2_5(found.3), "{label}: {found:?}");
        }
        if dimensions {
            let count = scene.entities.iter().filter(|entity| entity.kind.name() == "dimension").count();
            assert_eq!(count, 4, "{label}");
        }
    };
    for (to, version) in [("dxf", None), ("dxf", Some("R12")), ("dwg", None)] {
        let options = DraftOptions { write: WriteOptions { version: version.map(str::to_string) }, sheet: None };
        let drafted = zumen::draft(spec, to, &options).unwrap_or_else(|error| panic!("{to}: {error}"));
        check(&format!("{to} {version:?}"), &drafted.bytes, None, true);
    }
    for to in ["jww", "sfc"] {
        let drafted = zumen::draft(spec, to, &DraftOptions::default()).unwrap_or_else(|error| panic!("{to}: {error}"));
        check(to, &drafted.bytes, None, false);
        let encoding = drafted.notices.iter().any(|notice| notice.code == "encoding");
        assert_eq!(encoding, to == "sfc", "{to}: {:?}", drafted.notices);
    }
    for to in ["dxf", "dwg"] {
        let drafted = draft_into(&template(to), &spec.replace(r#""name": "S""#, r#""name": "A3""#));
        check(&format!("{to} into"), &drafted.bytes, Some("A3"), true);
    }
}

// ---- draft_into: JWW・SXF のテンプレート（決定 0027） ----

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// 用紙が A3 横か A1 横の JWW・SXF のテンプレート: fixtures と、編集で作った線と作図部品のある SFZ
fn landscape_templates() -> Vec<(&'static str, Vec<u8>)> {
    let operations = zumen::edit::parse_operations(
        r#"[
            {"op": "add", "kind": "line", "layer": "既存", "start": [10, 10], "end": [100, 10]},
            {"op": "add-block", "name": "既存の部品"},
            {"op": "add", "kind": "circle", "container": "block:既存の部品", "center": [0, 0], "radius": 5},
            {"op": "add", "kind": "block-reference", "block": "既存の部品", "position": [50, 50]}
        ]"#,
    )
    .unwrap();
    vec![
        ("jww v700", fixture("jww/minimal-v700.jww")),
        ("jww v600", fixture("jww/minimal-v600.jww")),
        ("p21 A1", fixture("p21/sample.p21")),
        ("sfc A1", fixture("sfc/sample.sfc")),
        ("sfc without a sheet", fixture("sfc/nosheet.sfc")),
        ("sfz", zumen::create("sfz", "mm", &operations).unwrap().bytes),
    ]
}

fn draft_sheet_into(bytes: &[u8], spec: &str, sheet: Option<&str>) -> zumen::Converted {
    let options = zumen::DraftIntoOptions { sheet: sheet.map(str::to_string), ..zumen::DraftIntoOptions::default() };
    zumen::draft_into(bytes, spec, &options).unwrap_or_else(|error| panic!("{error} ({})", error.code))
}

/// 図形の id・種類・範囲（0.001 mm に丸める）
fn outlines(scene: &Scene) -> Vec<(String, String, [i64; 4])> {
    let round = |value: f64| (value * 1000.0).round() as i64;
    scene
        .entities
        .iter()
        .map(|entity| {
            let b = &entity.bbox;
            (
                entity.id.clone(),
                entity.kind.name().to_string(),
                [round(b.min_x), round(b.min_y), round(b.max_x), round(b.max_y)],
            )
        })
        .collect()
}

/// 見本の 4 枚を 1 枚ずつ JWW・SXF のテンプレートに描くと、テンプレートの図形はそのままで、draft と同じ文字が同じ大きさ
/// で用紙にあり、寸法は寸法、ブロックはブロックの配置として読み直せる
#[test]
fn drafts_every_sample_sheet_into_jww_and_sxf_templates() {
    for (label, template) in landscape_templates() {
        let format = zumen::detect(&template).unwrap();
        let before = outlines(&read(&template, None));
        let mut scenes = Vec::new();
        for sheet in SHEETS {
            let drafted = draft_sheet_into(&template, SPEC, Some(sheet));
            assert_eq!(zumen::detect(&drafted.bytes), Some(format), "{label} {sheet}");
            let scene = read(&drafted.bytes, None);
            let after = outlines(&scene);
            for entity in &before {
                assert!(after.contains(entity), "{label} {sheet}: テンプレートの {entity:?} が変わりました");
            }
            assert!(after.len() > before.len(), "{label} {sheet}");
            let codes: Vec<&str> = drafted.notices.iter().map(|notice| notice.code.as_str()).collect();
            assert!(!codes.contains(&"view-overlap") && !codes.contains(&"draft-into-sheets"), "{label}: {codes:?}");
            scenes.push((sheet, scene));
        }
        check_sheets(&scenes, label);
        let plan = &scenes[2].1;
        let count = |kind: &str| plan.entities.iter().filter(|entity| entity.kind.name() == kind).count();
        // 通り芯の間の 3 + 1 と 2 + 1 の寸法は、どちらの形式でも寸法の図形（JWW の寸法図形、SXF の寸法のフィーチャ）
        let template_dimensions = before.iter().filter(|(_, kind, _)| kind == "dimension").count();
        assert_eq!(count("dimension"), 7 + template_dimensions, "{label}");
        // 扉は、足したブロックの配置
        let doors =
            plan.entities.iter().filter(|entity| entity.id.contains('>') && entity.kind.name() == "arc").count();
        assert!(doors >= 2, "{label}: 扉の円弧 {doors}");
    }
}

/// 用紙の大きさは変えられないので、仕様の用紙が違えば知らせて、テンプレートの用紙に描く。2 枚目からの用紙は描かない
#[test]
fn drafts_one_sheet_into_jww_and_sxf_on_the_template_paper() {
    let drafted = draft_into(&fixture("jww/minimal-v700.jww"), SPEC);
    let sheets = drafted.notices.iter().find(|notice| notice.code == "draft-into-sheets").expect("draft-into-sheets");
    assert_eq!(sheets.count, 3);
    let first = texts(&read(&drafted.bytes, None));
    find(&first, "設計概要書");
    assert!(first.iter().all(|(text, ..)| text != "敷地面積 2,096.00㎡"), "2 枚目は描かない");
    // features.sfc の用紙は A3 縦。仕様の A3 横は描き込む先の用紙にし、警告する
    let drafted = draft_sheet_into(&fixture("sfc/features.sfc"), SPEC, Some("A01-1"));
    let paper = drafted.notices.iter().find(|notice| notice.code == "draft-into-paper").expect("draft-into-paper");
    assert_eq!(paper.level, zumen::NoticeLevel::Warning);
    let scene = read(&drafted.bytes, None);
    let number = find(&texts(&scene), "A01-1").clone();
    assert!(number.1 < 297.0 && number.1 > 200.0, "図面番号は A3 縦の表題欄 {number:?}");
}

/// JWW: ビューは縮尺の合うレイヤグループ（なければ図形のないレイヤグループの縮尺を変える）に、用紙の図形は 1/1 の
/// レイヤグループに描き、レイヤの名前はレイヤグループごとに付ける
#[test]
fn drafts_views_into_jww_layer_groups_of_their_scales() {
    let template = fixture("jww/minimal-v700.jww");
    let original = zumen::jww::read_jww(&template).unwrap();
    for (sheet, scale) in [("A10-1", 100.0), ("A05-1", 250.0)] {
        let drafted = draft_sheet_into(&template, SPEC, Some(sheet));
        let doc = zumen::jww::read_jww(&drafted.bytes).unwrap();
        let layer = |name: &str| -> Vec<(usize, usize)> {
            (0..16)
                .flat_map(|group| (0..16).map(move |layer| (group, layer)))
                .filter(|&(group, layer)| doc.groups[group].layers[layer].name == name)
                .collect()
        };
        let walls = layer(if sheet == "A10-1" { "A-WALL" } else { "A-SITE" });
        assert_eq!(walls.len(), 1, "{sheet}");
        assert_eq!(doc.group_scale(walls[0].0 as u16), scale, "{sheet}");
        let frame = layer("A-FRAME");
        assert_eq!(doc.group_scale(frame[0].0 as u16), 1.0, "{sheet}");
        // 同じ仕様のレイヤ（A-TEXT）は、用紙とビューのレイヤグループにそれぞれある
        let groups: BTreeSet<usize> = layer("A-TEXT").iter().map(|(group, _)| *group).collect();
        assert_eq!(groups.len(), 2, "{sheet}: {groups:?}");
        // 図形のあるレイヤグループの縮尺は変えない
        for group in 0..16 {
            if original.entities.iter().any(|entity| usize::from(entity.base().layer_group) == group) {
                assert_eq!(doc.groups[group].scale, original.groups[group].scale, "{sheet}: {group}");
            }
        }
    }
    // 図形のないレイヤグループがなく、縮尺も合わなければ、ビューの場所で断る
    let lines: Vec<serde_json::Value> = (0..16)
        .map(|group| {
            serde_json::json!({"op": "add", "kind": "line", "container": format!("group:{group:X}"), "start": [0, 0], "end": [1, 0]})
        })
        .collect();
    let full = zumen::create("jww", "mm", &zumen::edit::operations_from_json(&serde_json::json!(lines)).unwrap())
        .unwrap()
        .bytes;
    let spec = r#"{"sheets": [{"name": "S", "views": [{"scale": 100, "viewport": [10, 10, 100, 100], "items": [
        {"type": "line", "from": [0, 0], "to": [1000, 0]}
    ]}]}]}"#;
    let error = zumen::draft_into(&full, spec, &zumen::DraftIntoOptions::default()).unwrap_err();
    assert_eq!((error.code, error.path.as_str()), ("invalid-value", "sheets[0].views[0]"), "{error}");
}

/// SXF: ビューはビューの名前の部分図（数学座標系）で、ビューポートの位置に 1/縮尺で置かれ、図形は実寸
#[test]
fn drafts_views_into_sxf_sub_figures() {
    let drafted = draft_sheet_into(&fixture("sfc/sample.sfc"), SPEC, Some("A10-1"));
    let drawing = zumen::sxf::parse_sxf(&drafted.bytes, "sfc").unwrap();
    let figure = drawing.document.sub_figures.get("1階平面図").expect("ビューの部分図");
    assert_eq!(figure.feature.flag, 1.0);
    assert!(!figure.members.is_empty());
    let placement = drawing.document.sheet_members.iter().find_map(|feature| match feature {
        zumen::sxf::SxfFeature::SfigLocate { name, ratio_x, ratio_y, .. } if name == "1階平面図" => {
            Some((*ratio_x, *ratio_y))
        }
        _ => None,
    });
    assert_eq!(placement, Some((0.01, 0.01)));
}

/// 角度・半径・直径寸法: SXF は寸法のフィーチャで、値は仕様の書き方（φ は CP932 で書ける）。JWW の寸法図形は角度寸法を
/// 持てないので、角度寸法は表示の線・円弧・文字で描いて知らせる
#[test]
fn drafts_angular_radius_and_diameter_dimensions_into_jww_and_sxf() {
    let spec = r#"{"sheets": [{"name": "S", "paper": "A3", "views": [{
        "scale": 100, "viewport": [20, 20, 400, 277], "center": [10000, 5000],
        "items": [
            {"type": "line", "from": [0, 0], "to": [6000, 0]},
            {"type": "line", "from": [0, 0], "to": [3000, 4000]},
            {"type": "dimension", "kind": "angular", "center": [0, 0], "from": [6000, 0], "to": [3000, 4000], "radius": 3000},
            {"type": "dimension", "kind": "angular", "center": [0, 0], "from": [6000, 0], "to": [3000, 4000], "at": [-2000, -1000], "precision": 1},
            {"type": "circle", "center": [12000, 5000], "radius": 1500},
            {"type": "dimension", "kind": "radius", "center": [12000, 5000], "radius": 1500},
            {"type": "circle", "center": [17000, 5000], "radius": 1500},
            {"type": "dimension", "kind": "diameter", "center": [17000, 5000], "radius": 1500, "angle": 0, "text": "<> 穴"}
        ]}]}]}"#;
    for to in ["jww", "sfc", "p21"] {
        let template = zumen::create(to, "mm", &[]).unwrap().bytes;
        let drafted = draft_into(&template, spec);
        let scene = read(&drafted.bytes, None);
        let texts = texts(&scene);
        for text in ["53°", "306.9°", "R1,500", "φ3,000 穴"] {
            let found = find(&texts, text);
            assert!(is_paper_2_5(found.3), "{to}: {found:?}");
        }
        let dimensions = scene.entities.iter().filter(|entity| entity.kind.name() == "dimension").count();
        let angular = drafted.notices.iter().find(|notice| notice.code == "draft-into-angular-dimension");
        if to == "jww" {
            assert_eq!((dimensions, angular.map(|notice| notice.count)), (2, Some(2)), "{to}");
        } else {
            assert_eq!((dimensions, angular), (4, None), "{to}");
            assert!(
                !drafted.notices.iter().any(|notice| notice.code == "edit-encoding"),
                "{to}: {:?}",
                drafted.notices
            );
        }
    }
}

/// 属性の値は文字で描き、線種は図面にある近い線種にし、どちらも知らせる
#[test]
fn reports_what_jww_and_sxf_templates_cannot_hold() {
    let spec = r#"{
        "layers": [{"name": "HIDDEN-LAYER", "plot": false}],
        "blocks": [{"name": "TAG", "attributes": [{"tag": "NAME", "at": [0, 0], "value": "既定"}]}],
        "sheets": [{"name": "S", "fields": {"NAME": "室名", "MISSING": "x"}, "items": [
            {"type": "insert", "block": "TAG", "at": [100, 100]},
            {"type": "line", "from": [10, 10], "to": [200, 10], "lineType": "CENTER", "layer": "HIDDEN-LAYER"},
            {"type": "polyline", "points": [[10, 20], [60, 20], [60, 40]], "bulges": [0.5, 0, 0], "width": 1}
        ]}]
    }"#;
    for to in ["jww", "sfc"] {
        let drafted = draft_into(&zumen::create(to, "mm", &[]).unwrap().bytes, spec);
        let codes: Vec<&str> = drafted.notices.iter().map(|notice| notice.code.as_str()).collect();
        for code in ["draft-into-attributes", "draft-into-line-type", "draft-into-layer-attributes", "unused-field"] {
            assert!(codes.contains(&code), "{to}: {code} in {codes:?}");
        }
        let scene = read(&drafted.bytes, None);
        find(&texts(&scene), "室名");
        // 幅のある折線は塗り（JWW はソリッド、SXF は塗り）
        let fills = scene.entities.iter().filter(|entity| matches!(entity.kind.name(), "solid" | "hatch")).count();
        assert!(fills >= 2, "{to}: {fills}");
    }
}
