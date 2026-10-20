use super::*;
use crate::{JWW, JwwBlock, JwwBlockDefinition, JwwCircleSolid, JwwDimension, JwwSolid, read_jww, write_jww};
use zumen_core::{DrawingReader, ReadOptions};

fn base(group: u16, layer: u16, color: u16, style: u8) -> JwwBase {
    JwwBase { group: 0, pen_style: style, pen_color: color, pen_width: 0, layer, layer_group: group, flag: 0 }
}

fn line(base: JwwBase, x1: f64, y1: f64, x2: f64, y2: f64) -> JwwEntity {
    JwwEntity::Line(JwwLine { base, x1, y1, x2, y2 })
}

fn text(base: JwwBase, x: f64, y: f64, size: f64, content: &str) -> JwwText {
    JwwText {
        base,
        x1: x,
        y1: y,
        x2: x + 20.0,
        y2: y,
        text_type: 0,
        size_x: size,
        size_y: size,
        spacing: 0.5,
        angle: 0.0,
        font: DEFAULT_FONT.into(),
        content: content.into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn arc(
    base: JwwBase,
    cx: f64,
    cy: f64,
    radius: f64,
    start: f64,
    sweep: f64,
    tilt: f64,
    flatness: f64,
    full: bool,
) -> JwwEntity {
    JwwEntity::Arc(JwwArc { base, cx, cy, radius, start, sweep, tilt, flatness, full })
}

fn point(base: JwwBase, x: f64, y: f64, temporary: bool) -> JwwEntity {
    JwwEntity::Point(JwwPoint { base, x, y, temporary, code: 0, angle: 0.0, scale: 0.0 })
}

/// A2、グループ 0 は 1/100、グループ 1 は 1/1
fn drawing(entities: Vec<JwwEntity>) -> JwwDocument {
    let mut doc = JwwDocument::new();
    doc.paper_size = 2;
    doc.groups[0].scale = 100.0;
    doc.groups[0].name = "平面".into();
    doc.groups[0].layers[1].name = "通り芯".into();
    doc.groups[1].layers[3].state = 0;
    doc.entities = entities;
    doc
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-4 * (1.0 + a.abs().max(b.abs()))
}

/// 単精度の座標で同じ図形か
fn same(a: &JwwEntity, b: &JwwEntity) -> bool {
    match (a, b) {
        (JwwEntity::Line(p), JwwEntity::Line(q)) => {
            p.base == q.base && near(p.x1, q.x1) && near(p.y1, q.y1) && near(p.x2, q.x2) && near(p.y2, q.y2)
        }
        (JwwEntity::Arc(p), JwwEntity::Arc(q)) => {
            p.base == q.base
                && near(p.cx, q.cx)
                && near(p.cy, q.cy)
                && near(p.radius, q.radius)
                && near(p.start, q.start)
                && near(p.sweep, q.sweep)
                && near(p.tilt, q.tilt)
                && near(p.flatness, q.flatness)
                && p.full == q.full
        }
        (JwwEntity::Point(p), JwwEntity::Point(q)) => {
            p.base.layer == q.base.layer
                && p.base.layer_group == q.base.layer_group
                && near(p.x, q.x)
                && near(p.y, q.y)
                && p.temporary == q.temporary
        }
        (JwwEntity::Text(p), JwwEntity::Text(q)) => {
            p.base.pen_color == q.base.pen_color
                && p.content == q.content
                && near(p.x1, q.x1)
                && near(p.y1, q.y1)
                && near(p.x2, q.x2)
                && near(p.y2, q.y2)
                && p.size_x == q.size_x
                && p.size_y == q.size_y
                && p.spacing == q.spacing
        }
        _ => false,
    }
}

#[test]
fn writes_and_reads_what_jwc_holds() {
    let entities = vec![
        point(base(0, 0, 2, 1), 10.0, 20.0, true),
        line(base(0, 0, 2, 1), -100.0, 0.0, 0.0, 0.0),
        line(base(1, 2, 5, 7), 1.0, 2.0, 3.0, 4.0),
        arc(base(0, 1, 3, 1), 10.0, 20.0, 5.0, 0.0, TAU, 0.0, 1.0, true),
        arc(base(0, 1, 3, 2), 10.0, 20.0, 5.0, -0.5, 1.0, 0.0, 1.0, false),
        arc(base(0, 1, 4, 1), 30.0, 20.0, 8.0, 0.25, 2.0, 0.5, 0.5, false),
        JwwEntity::Text(text(base(0, 1, 2, 1), -50.0, 40.0, 3.0, "通り芯 X1")),
        JwwEntity::Text(text(base(0, 1, 6, 1), -50.0, 30.0, 7.5, "大きい文字")),
        point(base(1, 0, 9, 1), -5.0, -5.0, false),
    ];
    let doc = drawing(entities);
    let written = write_jwc(&doc);
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    assert!(is_jwc(&written.bytes));
    // 読んで書くと同じバイト列
    let file = JwcFile::read(&written.bytes).unwrap();
    assert_eq!(file.write().bytes, written.bytes);
    let read = read_jwc(&written.bytes).unwrap();
    // 仮点、線、円弧、文字、点の順
    assert_eq!(read.entities.len(), doc.entities.len());
    let mut expected = doc.entities.clone();
    // 文字種の文字は、その文字種の大きさと番号を持つ
    for entity in &mut expected {
        if let JwwEntity::Text(text) = entity {
            text.text_type = 0;
        }
    }
    for (a, b) in read.entities.iter().zip(&expected) {
        assert!(same(a, b), "{a:?}\n{b:?}");
    }
    assert_eq!((read.paper_size, read.groups[0].scale, read.groups[0].name.as_str()), (2, 100.0, "平面"));
    assert_eq!((read.groups[0].layers[1].name.as_str(), read.groups[1].layers[3].state), ("通り芯", 0));
    assert_eq!((read.groups[0].state, read.groups[0].layers[0].state), (3, 3));
    // JWW にしても同じ図形
    let jww = read_jww(&write_jww(&read).bytes).unwrap();
    assert_eq!(jww.entities, read.entities);
}

#[test]
fn breaks_down_what_jwc_cannot_hold() {
    let mut entities = vec![
        JwwEntity::Block(JwwBlock {
            base: base(0, 0, 2, 1),
            x: 50.0,
            y: 0.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: PI / 2.0,
            definition: 1,
        }),
        JwwEntity::Dimension(JwwDimension {
            base: base(0, 0, 2, 1),
            line: JwwLine { base: base(0, 0, 2, 1), x1: 0.0, y1: -10.0, x2: 100.0, y2: -10.0 },
            text: text(base(0, 0, 2, 1), 45.0, -9.0, 3.0, "10000"),
            lines: Vec::new(),
            points: Vec::new(),
        }),
        JwwEntity::Solid(JwwSolid {
            base: base(0, 0, 10, 1),
            points: [0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
            color: Some(0xff),
        }),
        JwwEntity::CircleSolid(JwwCircleSolid {
            base: base(0, 0, 2, 101),
            cx: 0.0,
            cy: 0.0,
            radius: 3.0,
            flatness: 1.0,
            tilt: 0.0,
            start: 0.0,
            sweep: TAU,
            mode: 100.0,
            color: None,
        }),
        line(base(0, 0, 104, 32), 0.0, 0.0, 10.0, 0.0),
        line(JwwBase { pen_width: 25, ..base(0, 0, 2, 1) }, 0.0, 0.0, 10.0, 0.0),
        JwwEntity::Text(JwwText { font: "@ＭＳ 明朝".into(), ..text(base(0, 0, 2, 1), 0.0, 0.0, 3.0, "縦") }),
        JwwEntity::Text(text(base(0, 0, 2, 1), 0.0, 0.0, 3.0, "^@BMzumen.bmp,100,50")),
    ];
    // 文字種（10）より多い大きさ
    for size in 1..=12 {
        entities.push(JwwEntity::Text(text(base(0, 0, 3, 1), 0.0, 0.0, f64::from(size) * 1.1, "大")));
    }
    let mut doc = drawing(entities);
    doc.memo = "メモ".into();
    doc.blocks.push(JwwBlockDefinition {
        number: 1,
        name: "部品".into(),
        entities: vec![
            line(base(0, 0, 2, 1), 0.0, 0.0, 5.0, 0.0),
            arc(base(0, 0, 2, 1), 0.0, 0.0, 1.0, 0.0, PI, 0.0, 1.0, false),
        ],
    });
    let written = write_jwc(&doc);
    let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
    for code in [
        "jwc-block",
        "jwc-dimension",
        "jwc-solid",
        "jwc-color",
        "jwc-line-type",
        "jwc-width",
        "jwc-font",
        "jwc-text-size",
        "embedded-image",
        "memo",
    ] {
        assert!(codes.contains(&code), "{code} not in {codes:?}");
    }
    let read = read_jwc(&written.bytes).unwrap();
    let lines: Vec<&JwwLine> = read
        .entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Line(line) => Some(line),
            _ => None,
        })
        .collect();
    // ブロックの線は配置（2 倍、90°）を当てた位置に
    assert!(
        lines
            .iter()
            .any(|line| near(line.x1, 50.0) && near(line.y1, 0.0) && near(line.x2, 50.0) && near(line.y2, 10.0))
    );
    // SXF 対応色 104（SXF の青）は線色 6（画面色が青）、線種 32（dashed）は線種 2
    assert!(lines.iter().any(|line| line.base.pen_color == 6 && line.base.pen_style == 2));
    // ブロックの円弧も 2 倍の半径で 90° 回す
    let arcs: Vec<&JwwArc> = read
        .entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Arc(arc) => Some(arc),
            _ => None,
        })
        .collect();
    assert!(arcs.iter().any(|arc| near(arc.radius, 2.0) && near(arc.cx, 50.0) && near(arc.tilt, PI / 2.0)));
    // 寸法の値の文字は残る
    assert!(read.entities.iter().any(|entity| matches!(entity, JwwEntity::Text(text) if text.content == "10000")));
    assert!(
        !read.entities.iter().any(|entity| matches!(entity, JwwEntity::Text(text) if text.content.starts_with("^@")))
    );
}

#[test]
fn keeps_header_fields_and_text_bytes_it_does_not_use() {
    let doc = drawing(vec![
        JwwEntity::Text(text(base(0, 0, 2, 1), 0.0, 0.0, 3.0, "最初")),
        JwwEntity::Text(text(base(0, 0, 2, 1), 0.0, 10.0, 3.0, "次")),
    ]);
    let mut bytes = write_jwc(&doc).bytes;
    // 設定の欄と識別子の後ろの値を変え、最初の文字の NUL の後ろに Jw_cad が切り詰めた残りのようなバイトを足す
    let settings = b"9.5,1.0,3.0,15.0";
    bytes[400..400 + settings.len()].copy_from_slice(settings);
    bytes[50] = 7;
    let file = JwcFile::read(&bytes).unwrap();
    let texts_at = JwcFile::header_length(true);
    let pool_at = texts_at + 2 * TEXT;
    let mut patched = bytes[..pool_at].to_vec();
    patched.extend_from_slice(&[0x8d, 0xc5, 0x8f, 0x89, 0, 0xaa, 0xbb]);
    patched.extend_from_slice(&bytes[pool_at + 5..]);
    // 2 つ目の文字の場所と、領域の終わりを 2 バイト後ろに
    let second = u32_at(&patched, texts_at + TEXT + 16) + 2;
    patched[texts_at + TEXT + 16..texts_at + TEXT + 20].copy_from_slice(&second.to_le_bytes());
    let mut fixed = patched[..texts_at].to_vec();
    let end = far_pointer(&csv_fields(&fixed, 2)[1]).unwrap() + 2;
    patch_csv(&mut fixed, 2, &[(1, far_pointer_text(end))]);
    patched[..texts_at].copy_from_slice(&fixed);
    let file2 = JwcFile::read(&patched).unwrap();
    assert_eq!(file2.texts[0].content.text, "最初");
    assert_eq!(file2.write().bytes, patched);
    assert_eq!(file.write().bytes, bytes);
    // 2 つ目の文字を変えても、最初の文字の領域はそのまま
    let mut changed = file2.clone();
    changed.texts[1].content = JwcString::new("変えた");
    let written = changed.write().bytes;
    assert!(written.windows(7).any(|window| window == [0x8d, 0xc5, 0x8f, 0x89, 0, 0xaa, 0xbb]));
    assert_eq!(read_jwc(&written).unwrap().entities[1], {
        let mut expected = read_jwc(&patched).unwrap().entities[1].clone();
        if let JwwEntity::Text(text) = &mut expected {
            text.content = "変えた".into();
        }
        expected
    });
}

#[test]
fn keeps_unused_temporary_slots_and_write_layer_bits() {
    let doc = drawing(vec![point(base(0, 0, 2, 1), 0.0, 0.0, true), point(base(0, 1, 2, 1), 10.0, 0.0, true)]);
    let mut bytes = write_jwc(&doc).bytes;
    // 使っていない仮点の番号（0 と 5）に残った値と、書込みレイヤの上位 4 ビットがグループの番号でないもの
    bytes[TEMPORARY_X..TEMPORARY_X + 4].copy_from_slice(&1.5f32.to_le_bytes());
    bytes[TEMPORARY_X + 20..TEMPORARY_X + 24].copy_from_slice(&2.5f32.to_le_bytes());
    bytes[TEMPORARY_LAYER + 5] = 0x23;
    let write_layers = JwcFile::write_layers(true);
    assert_eq!(bytes[write_layers + 3], 0x30);
    bytes[write_layers + 3] = 0x50;
    let file = JwcFile::read(&bytes).unwrap();
    assert_eq!(file.write().bytes, bytes);
    // 仮点を 1 つ減らすと、空いた番号 2 だけを 0 にする
    let mut fewer = file.clone();
    fewer.temporary_points.pop();
    let written = fewer.write().bytes;
    assert_eq!(&written[TEMPORARY_X + 8..TEMPORARY_X + 12], &[0; 4]);
    assert_eq!(&written[TEMPORARY_X..TEMPORARY_X + 4], &1.5f32.to_le_bytes());
    assert_eq!(&written[TEMPORARY_X + 20..TEMPORARY_X + 24], &2.5f32.to_le_bytes());
    assert_eq!(written[TEMPORARY_LAYER + 5], 0x23);
    assert_eq!(JwcFile::read(&written).unwrap().temporary_points, fewer.temporary_points);
    // 書込みレイヤを変えたグループだけ、上位 4 ビットをグループの番号にして書き直す
    let mut moved = file.clone();
    moved.groups[3].write_layer = 4;
    let written = moved.write().bytes;
    assert_eq!((written[write_layers + 2], written[write_layers + 3]), (0x20, 0x34));
}

#[test]
fn reads_jwc_as_a_scene() {
    let bytes = write_jwc(&drawing(vec![line(base(0, 0, 2, 1), -100.0, 0.0, 0.0, 0.0)])).bytes;
    assert_eq!(JWW.detect(&bytes), Some("jwc"));
    let scene = JWW.read(&bytes, "jwc", &ReadOptions::default()).unwrap();
    assert_eq!((scene.format.as_str(), scene.entities.len()), ("jwc", 1));
    // 実寸（単精度の座標）
    let start = scene.entities[0].properties.iter().find(|property| property.key == "start").unwrap();
    let raw = serde_json::to_value(&start.raw).unwrap();
    let (x, y) = (raw[0].as_f64().unwrap(), raw[1].as_f64().unwrap());
    assert!((x + 10000.0).abs() < 0.01 && y.abs() < 0.01, "{x} {y}");
    assert!(JwcFile::read(b"jw_cad(c)data").is_err());
    assert!(read_jwc(&bytes[..bytes.len() - 1]).is_err());
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの JWC）を、読んで書くと同じバイト列になること。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-jww --lib jwc -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn rewrites_the_jwc_samples_byte_for_byte() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut checked = 0;
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jwc")) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let file = JwcFile::read(&bytes).unwrap_or_else(|error| panic!("{}: {error:?}", path.display()));
        assert!(file.write().bytes == bytes, "{}", path.display());
        let doc = file.document();
        let written = write_jww(&doc);
        assert_eq!(read_jww(&written.bytes).unwrap().entities, doc.entities, "{}", path.display());
        checked += 1;
    }
    assert!(checked > 0, "no JWC in {folder}");
}
