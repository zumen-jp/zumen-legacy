//! 製図の規則の確認（zumen_core::checks）: 作図キットの見本を各形式に書いて読み直し、国土交通省 CAD製図基準の
//! 規則集で調べる。

use zumen::checks::{CheckLevel, CheckReport, RuleResult, RuleSet, check_scene};
use zumen::{DraftOptions, ReadDrawingOptions, WriteOptions};

const SPEC: &str = include_str!("../../zumen-draft/examples/sheets.json");

fn check(to: &str, sheet: &str) -> CheckReport {
    let options = DraftOptions { write: WriteOptions::default(), sheet: Some(sheet.to_string()) };
    let drafted = zumen::draft(SPEC, to, &options).unwrap();
    let scene = zumen::read(&drafted.bytes, &ReadDrawingOptions::default()).unwrap();
    check_scene(&scene, &RuleSet::built_in("mlit-cad-2025").unwrap())
}

fn result<'a>(report: &'a CheckReport, id: &str) -> &'a RuleResult {
    report.results.iter().find(|result| result.id == id).unwrap()
}

fn values(result: &RuleResult) -> Vec<&str> {
    result.findings.iter().map(|finding| finding.value.as_str()).collect()
}

#[test]
fn checks_a_drafted_sheet_in_every_format() {
    for to in ["dxf", "dwg", "jww", "sfc"] {
        let report = check(to, "A01-1");
        // 見本のレイヤ名（A-FRAME、A-TEXT、0）は基準の名前の型に合わない
        let names = result(&report, "layer-name");
        assert_eq!((names.checked, names.findings.len(), names.level), (3, 3, CheckLevel::Error), "{to}");
        assert!(!report.passed(), "{to}");
        // 面積の表の ㎡ は機種依存文字
        let characters = result(&report, "characters");
        assert_eq!(values(characters), ["㎡"], "{to}");
        assert_eq!(characters.findings[0].count, 7, "{to}");
        // A3 の横置き、色は 16 色の中
        for id in ["sheet-size", "orientation", "color"] {
            assert!(result(&report, id).findings.is_empty(), "{to} {id}");
            assert!(result(&report, id).checked > 0, "{to} {id}");
        }
    }
}

#[test]
fn reads_the_text_heights_of_dxf_and_dwg_as_capital_heights() {
    for to in ["dxf", "dwg"] {
        let report = check(to, "A01-1");
        let heights = result(&report, "text-height");
        // 2.5 mm の文字、寸法の文字はそのまま。見本の縮尺の文字（1.75）、図名（6）、縮めた文字（1.87）が規則にない
        assert_eq!(values(heights), ["1.75", "6", "1.87"], "{to}");
        // DXF のレイヤから継ぐ線の太さは調べられない
        let widths = result(&report, "line-width");
        assert!(widths.unchecked > 0 && widths.findings.is_empty(), "{to}");
    }
}

#[test]
fn writes_the_text_heights_of_the_spec_as_sxf_and_jww_text_heights() {
    // SXF の文字範囲の高さと JWW の文字の大きさは、DXF の TEXT の高さと同じ値（decision 0019）
    for to in ["sfc", "jww"] {
        let report = check(to, "A01-1");
        assert_eq!(values(result(&report, "text-height")), ["1.75", "6", "1.87"], "{to}");
    }
}

#[test]
fn dimension_text_heights_do_not_depend_on_unrelated_text_entities() {
    for to in ["dxf", "dwg"] {
        for with_label in [false, true] {
            let mut items =
                vec![serde_json::json!({"type": "dimension", "from": [20, 20], "to": [120, 20], "offset": 10})];
            if with_label {
                items.push(serde_json::json!({"type": "text", "at": [30, 30], "text": "注記"}));
            }
            let spec = serde_json::json!({"sheets": [{"name": "S", "items": items}]}).to_string();
            let drafted = zumen::draft(&spec, to, &DraftOptions::default()).unwrap();
            let scene = zumen::read(
                &drafted.bytes,
                &ReadDrawingOptions { view: Some("S".into()), ..ReadDrawingOptions::default() },
            )
            .unwrap();
            let report = check_scene(&scene, &RuleSet::built_in("mlit-cad-2025").unwrap());
            let heights = result(&report, "text-height");
            assert_eq!(heights.checked, if with_label { 2 } else { 1 });
            assert!(heights.findings.is_empty(), "{to} {with_label}: {heights:?}");
        }
    }
}

#[test]
fn leaves_the_line_types_of_jww_unchecked() {
    let report = check("jww", "A01-1");
    let types = result(&report, "line-type");
    assert_eq!((types.checked, types.findings.len()), (0, 0));
    assert!(types.unchecked > 0);
    // 図形の尺度はレイヤグループの縮尺（見本は 1:1）
    let scales = result(&report, "scale");
    assert!(scales.checked > 0 && scales.findings.is_empty());
}
