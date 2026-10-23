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
fn refuses_formats_without_layouts_and_names_the_spec_place_of_edit_errors() {
    let jww = zumen::create("jww", "mm", &[]).unwrap().bytes;
    let error = zumen::draft_into(&jww, SPEC, &zumen::DraftIntoOptions::default()).unwrap_err();
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
