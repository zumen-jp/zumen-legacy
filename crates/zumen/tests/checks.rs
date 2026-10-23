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
fn leaves_the_line_types_of_jww_unchecked() {
    let report = check("jww", "A01-1");
    let types = result(&report, "line-type");
    assert_eq!((types.checked, types.findings.len()), (0, 0));
    assert!(types.unchecked > 0);
    // 図形の尺度はレイヤグループの縮尺（見本は 1:1）
    let scales = result(&report, "scale");
    assert!(scales.checked > 0 && scales.findings.is_empty());
}
