//! SFC の書き出し: tests/fixtures の SFC・P21 を読んで SFC に新しく書き（元のファイルを持たせずに書く）、読み直すと
//! 同じ図面になる。元の SFC を書き換える書き方は edit_sfc.rs

use std::path::{Path, PathBuf};
use zumen_sxf::{SxfDrawing, parse_sxf, sxf_to_scene, write_sfc, written_ids};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// 表示の id（#n>#m …）を、書いたインスタンス番号にする
fn renumbered(id: &str, ids: &std::collections::HashMap<u64, f64>) -> String {
    id.split('>')
        .map(|part| match part.strip_prefix('#').and_then(|number| number.parse::<f64>().ok()) {
            Some(number) => format!("#{}", ids.get(&number.to_bits()).copied().unwrap_or(f64::NAN)),
            None => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(">")
}

fn round_trip(drawing: &SxfDrawing) -> SxfDrawing {
    let written = write_sfc(drawing);
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    zumen_sxf::parse_sfc(&written.bytes).unwrap()
}

#[test]
fn written_sfc_reads_back_as_the_same_drawing() {
    for (folder, format) in [("sfc", "sfc"), ("p21", "p21")] {
        for name in ["features", "nosheet", "sample"] {
            let bytes = std::fs::read(fixtures().join(folder).join(format!("{name}.{format}"))).unwrap();
            let mut drawing = parse_sxf(&bytes, format).unwrap();
            drawing.source = None;
            if sxf_to_scene(&drawing).entities.is_empty() {
                // フィーチャのない図面（nosheet.p21 は円が用紙に置かれていない）は、読み直せる SFC にならない
                continue;
            }
            let written = write_sfc(&drawing);
            let ids = written_ids(&written);
            let again = round_trip(&drawing);
            let (mut before, after) = (sxf_to_scene(&drawing), sxf_to_scene(&again));
            for entity in &mut before.entities {
                entity.id = renumbered(&entity.id, &ids);
            }
            assert_eq!(before.entities.len(), after.entities.len(), "{name}.{format}");
            for (a, b) in before.entities.iter().zip(&after.entities) {
                // P21 から変換したフィーチャは同じインスタンス番号を持つことがあるので、番号は SFC だけで比べる
                if format == "sfc" {
                    assert_eq!(a.id, b.id, "{name}.{format}");
                }
                assert_eq!(a.kind, b.kind, "{name}.{format}");
                assert_eq!(&a.layers, &b.layers, "{name}.{format} {}", a.id);
                assert_eq!(&a.primitives, &b.primitives, "{name}.{format} {}", a.id);
            }
            assert_eq!(before.layers, after.layers, "{name}.{format}");
            assert_eq!(before.sheet, after.sheet, "{name}.{format}");
            // 書いた SFC を読んで新しく書き直しても変わらない（循環する配置の複合図形は、1 度目に並べ替わることがある）
            let mut again = again;
            again.source = None;
            let twice = write_sfc(&again).bytes;
            let mut third = zumen_sxf::parse_sfc(&twice).unwrap();
            third.source = None;
            assert_eq!(write_sfc(&third).bytes, twice, "{name}.{format}");
        }
    }
}

#[test]
fn declares_predefined_widths_before_geometry_without_shifting_user_width_codes() {
    // A round trip through our reader cannot detect missing predefined declarations:
    // it resolves codes 1–9 even without them. Jw_cad drops those strokes instead.
    let source = b"ISO-10303-21;\nDATA;
/*SXF
#10 = width_feature('0.42')
SXF*/
/*SXF
#20 = line_feature('0','1','1','2','0','0','10','10')
SXF*/
/*SXF
#30 = line_feature('0','1','1','11','0','10','10','0')
SXF*/
ENDSEC;\nEND-ISO-10303-21;";
    let mut drawing = zumen_sxf::parse_sfc(source).unwrap();
    drawing.source = None;
    let written = write_sfc(&drawing);
    let text = String::from_utf8(written.bytes.clone()).unwrap();
    let standard = text.find("width_feature('0.18')").expect("used predefined width must be declared");
    let custom = text.find("width_feature('0.42')").unwrap();
    assert!(standard < custom && custom < text.find("line_feature(").unwrap());
    let again = zumen_sxf::parse_sfc(&written.bytes).unwrap();
    assert_eq!(again.document.user_widths.len(), 1);
    assert_eq!(again.document.width(2.0), Some(0.18));
    assert_eq!(again.document.width(11.0), Some(0.42));
}

#[test]
fn drawings_beyond_the_sxf_limits_are_reported() {
    let mut drawing = parse_sxf(&std::fs::read(fixtures().join("sfc/sample.sfc")).unwrap(), "sfc").unwrap();
    drawing.source = None;
    for index in 0..300 {
        drawing.document.layers.push(zumen_sxf::LayerDefinition { id: 0.0, name: format!("L{index}"), visible: true });
    }
    drawing.document.sheet.as_mut().unwrap().name = "長".repeat(200);
    for written in [write_sfc(&drawing), zumen_sxf::write_p21(&drawing)] {
        let limit = written.notices.iter().find(|notice| notice.code == "sxf-limit").expect("上限の知らせ");
        // レイヤは 302 で 46 多い。用紙の名前は CP932 で 400 バイト
        assert_eq!(limit.count, 47);
        assert!(limit.message.contains("レイヤ 46") && limit.message.contains("文字列 1"), "{}", limit.message);
    }
}
