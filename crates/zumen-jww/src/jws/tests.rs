use super::*;
use crate::{JWC, JWS, JWW, JwwArc, JwwBase, JwwBlock, JwwLine, JwwPoint, JwwText};
use std::f64::consts::{PI, TAU};
use zumen_core::{DrawingReader, ReadOptions};

fn base(group: u16, layer: u16, color: u16) -> JwwBase {
    JwwBase { group: 0, pen_style: 1, pen_color: color, pen_width: 0, layer, layer_group: group, flag: 0 }
}

fn line(base: JwwBase, x1: f64, y1: f64, x2: f64, y2: f64) -> JwwEntity {
    JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 })
}

fn text(base: JwwBase, (x1, y1): (f64, f64), (x2, y2): (f64, f64), content: &str) -> JwwEntity {
    JwwEntity::Text(JwwText {
        base,
        x1,
        y1,
        x2,
        y2,
        text_type: 1,
        size_x: 3.0,
        size_y: 3.0,
        spacing: 0.0,
        angle: (y2 - y1).atan2(x2 - x1).to_degrees(),
        font: "ＭＳ ゴシック".into(),
        content: content.into(),
    })
}

/// 基準点 (10, 20) の図形。線 2 本、円（グループ 1、1/50）、文字、点、ブロックの配置（横 2 倍、定義の中に半円）
fn symbol(version: u32) -> JwsDocument {
    let mut content = JwwDocument::new();
    content.version = version;
    content.groups[1].scale = 50.0;
    content.entities = vec![
        line(base(0, 0, 1), 10.0, 20.0, 40.0, 20.0),
        line(base(0, 0, 1), 10.0, 20.0, 10.0, 40.0),
        JwwEntity::Arc(JwwArc {
            base: base(1, 2, 2),
            cx: 30.0,
            cy: 30.0,
            radius: 5.0,
            start: 0.0,
            sweep: TAU,
            tilt: 0.0,
            flatness: 1.0,
            full: true,
        }),
        text(base(0, 1, 3), (-50.0, -20.0), (-26.0, -20.0), "日本語ｶﾅ ABC"),
        JwwEntity::Point(JwwPoint {
            base: base(0, 0, 2),
            x: 0.0,
            y: 0.0,
            temporary: false,
            code: 0,
            angle: 0.0,
            scale: 0.0,
        }),
        JwwEntity::Block(JwwBlock {
            base: base(0, 0, 2),
            x: 50.0,
            y: 0.0,
            scale_x: 2.0,
            scale_y: 1.0,
            rotation: 0.0,
            definition: 1,
        }),
    ];
    content.blocks = vec![JwwBlockDefinition {
        number: 1,
        name: "部品".into(),
        entities: vec![JwwEntity::Arc(JwwArc {
            base: base(0, 0, 2),
            cx: 0.0,
            cy: 0.0,
            radius: 5.0,
            start: 0.0,
            sweep: PI,
            tilt: 0.0,
            flatness: 1.0,
            full: false,
        })],
    }];
    JwsDocument { origin: [10.0, 20.0], bounds: [0.0; 4], content }
}

fn codes(written: &JwwWritten) -> Vec<&str> {
    written.notices.iter().map(|notice| notice.code.as_str()).collect()
}

#[test]
fn writes_and_reads_back_a_symbol() {
    for version in [351, 420, 600, 700] {
        let doc = symbol(version);
        let written = write_jws(&doc);
        assert!(written.notices.is_empty(), "{version}: {:?}", written.notices);
        let file = JwsFile::read(&written.bytes).unwrap();
        assert_eq!(file.write().bytes, written.bytes, "{version}");
        assert_eq!(
            (file.header.version, file.header.origin, file.header.group_scales[1]),
            (version, [10.0, 20.0], 50.0)
        );
        let read = read_jws(&written.bytes).unwrap();
        assert_eq!((&read.content.entities, &read.content.blocks), (&doc.content.entities, &doc.content.blocks));
        assert_eq!((read.content.version, read.content.groups[1].scale, read.origin), (version, 50.0, [10.0, 20.0]));
        // Jw_cad と同じく、範囲は文字の始点・終点と、円・ブロックの中の半円（横 2 倍）の端まで
        assert_eq!(read.bounds, [-50.0, -20.0, 60.0, 40.0], "{version}");
    }
}

#[test]
fn writes_a_new_header_as_jw_cad_10_does() {
    let mut doc = symbol(700);
    doc.content.entities.clear();
    doc.content.blocks.clear();
    let bytes = write_jws(&doc).bytes;
    // 識別子の後ろは "."、予備の整数・実数は 1〜6。図形がなければ範囲は基準点
    assert!(bytes[8..200].iter().all(|&byte| byte == b'.'));
    assert_eq!(u32::from_le_bytes(bytes[348..352].try_into().unwrap()), 1);
    assert_eq!(f64::from_le_bytes(bytes[412..420].try_into().unwrap()), 6.0);
    let read = read_jws(&bytes).unwrap();
    assert_eq!(read.bounds, [10.0, 20.0, 10.0, 20.0]);
    assert!(read.content.entities.is_empty() && read.content.truncated.is_none());
}

#[test]
fn writes_new_strings_in_the_encoding_of_the_file() {
    // Jw_cad 9 までの版 700 の図形は、文字列を Shift_JIS で書く（版 600 で書いた図形の版を 700 にしたもの）
    let mut bytes = write_jws(&symbol(600)).bytes;
    bytes[VERSION_AT..VERSION_AT + 4].copy_from_slice(&700u32.to_le_bytes());
    let mut file = JwsFile::read(&bytes).unwrap();
    assert_eq!(file.write().bytes, bytes);
    file.entities.push(JwwRecord::new(text(base(0, 0, 2), (0.0, 0.0), (6.0, 0.0), "追加")));
    let shift_jis = [4u8, 0x92, 0xc7, 0x89, 0xc1];
    assert!(file.write().bytes.windows(5).any(|window| window == shift_jis), "the new text is not in Shift_JIS");
    // Jw_cad 10 の版 700 の図形（UTF-16LE）には UTF-16LE で書く。Shift_JIS にない字も書ける
    let mut doc = symbol(700);
    doc.content.entities.push(text(base(0, 0, 2), (0.0, 10.0), (6.0, 10.0), "𠮷野"));
    let written = write_jws(&doc);
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    let mut file = JwsFile::read(&written.bytes).unwrap();
    file.entities.push(JwwRecord::new(text(base(0, 0, 2), (0.0, 0.0), (6.0, 0.0), "追加")));
    let utf16 = [0xff, 0xfe, 0xff, 2, 0xfd, 0x8f, 0xa0, 0x52];
    assert!(file.write().bytes.windows(8).any(|window| window == utf16));
    let Some(JwwEntity::Text(read)) = read_jws(&written.bytes).unwrap().content.entities.last().cloned() else {
        panic!("text")
    };
    assert_eq!(read.content, "𠮷野");
}

#[test]
fn reads_jws_as_a_scene() {
    let bytes = write_jws(&symbol(700)).bytes;
    // JWS は JWW・JWC の読み取りとは別の読み取り（Rust だけ）
    assert_eq!((JWS.detect(&bytes), JWW.detect(&bytes), JWC.detect(&bytes)), (Some("jws"), None, None));
    assert!(JWW.read(&bytes, "jws", &ReadOptions::default()).is_err());
    let scene = JWS.read(&bytes, "jws", &ReadOptions::default()).unwrap();
    assert_eq!((scene.format.as_str(), scene.version.as_str()), ("jws", "JWS 700"));
    // 用紙はなく、座標は用紙の mm のまま。ブロックの中の図形も描く
    assert!(scene.sheet.is_none());
    assert_eq!((scene.entities[0].bbox.min_x, scene.entities[0].bbox.min_y), (10.0, 20.0));
    assert!(scene.entities.iter().any(|entity| entity.id == "#6>#1"));
    let metadata = |key: &str| scene.metadata.iter().find(|property| property.key == key).map(|p| p.value.clone());
    assert_eq!(metadata("format").as_deref(), Some("JWS（図形、版 700）"));
    assert_eq!(metadata("origin").as_deref(), Some("10, 20"));
    // JWS は線色を持たないので、Jw_cad の既定の色で描く
    assert!(scene.notices.iter().any(|notice| notice.code == "default-palette"));
}

#[test]
fn reads_what_it_can_of_a_broken_symbol() {
    let bytes = write_jws(&symbol(700)).bytes;
    let cut = &bytes[..bytes.len() - 10];
    assert!(JwsFile::read(cut).is_err());
    let read = read_jws(cut).unwrap();
    assert!(read.content.truncated.is_some() && read.content.entities.len() == 6);
    assert!(read_jws(&bytes[..HEADER_LENGTH - 1]).is_err());
    assert!(read_jws(b"JwwData.").is_err());
}

#[test]
fn reports_what_jws_cannot_hold() {
    // レイヤグループの名前・非表示のレイヤは、JWS が持たない
    let mut doc = symbol(700);
    doc.content.groups[0].name = "平面".into();
    doc.content.groups[0].layers[2].state = 0;
    assert_eq!(codes(&write_jws(&doc)), ["jws-settings"]);
    let mut doc = symbol(700);
    doc.content.memo = "図面のメモ".into();
    assert_eq!(codes(&write_jws(&doc)), ["jws-settings"]);
    // 書けない版は 700 で書く
    let written = write_jws(&symbol(900));
    let notice = written.notices.iter().find(|notice| notice.code == "version").expect("version");
    assert!(notice.message.contains("JWS"), "{}", notice.message);
    assert_eq!(JwsFile::read(&written.bytes).unwrap().header.version, 700);
}

#[test]
fn reports_discarded_sxf_colors_and_width_units() {
    let mut doc = symbol(700);
    doc.content.extended_colors = Some(vec!["#123456".into(); 257]);
    assert_eq!(codes(&write_jws(&doc)), ["jws-settings"]);
    let mut doc = symbol(700);
    doc.content.width_unit.hundredth = !doc.content.width_unit.hundredth;
    assert_eq!(codes(&write_jws(&doc)), ["jws-settings"]);
}

/// Git に入れない図形（ZUMEN_SAMPLES のフォルダーの JWS）を、読んで書くと同じバイト列になること。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-jww --lib jws -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn rewrites_the_jws_samples_byte_for_byte() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut checked = 0;
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jws")) {
            continue;
        }
        let name = path.display().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let file = JwsFile::read(&bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(file.write().bytes == bytes, "{name}");
        let doc = read_jws(&bytes).unwrap();
        assert!(doc.content.truncated.is_none(), "{name}");
        // 書き直しても同じ図形。範囲は Jw_cad の記録に近い（Jw_cad は描いた曲線の点で求め、こちらは曲線の端で求める）
        let again = read_jws(&write_jws(&doc).bytes).unwrap();
        assert!(again.content.entities == doc.content.entities && again.content.blocks == doc.content.blocks, "{name}");
        let size = (doc.bounds[2] - doc.bounds[0]).max(doc.bounds[3] - doc.bounds[1]);
        let near = again.bounds.iter().zip(doc.bounds).all(|(a, b)| (a - b).abs() <= 0.01 + 0.01 * size);
        assert!(near, "{name}: {:?} {:?}", again.bounds, doc.bounds);
        checked += 1;
    }
    assert!(checked > 0, "no JWS in {folder}");
}
