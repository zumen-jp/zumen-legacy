//! 新しい図面を書く（write_jww）。書いた図面を表示用の読み取り（read_jww）で読むと、同じ図面の内容になること

use std::f64::consts::PI;
use zumen_jww::{
    JWW_VERSION, JwwArc, JwwBase, JwwBlock, JwwBlockDefinition, JwwCircleSolid, JwwDimension, JwwDocument, JwwEntity,
    JwwFile, JwwLine, JwwPoint, JwwSolid, JwwText, WidthUnit, read_jww, write_jww,
};

fn base(group: u16, layer: u16, color: u16, style: u8) -> JwwBase {
    JwwBase { group: 0, pen_style: style, pen_color: color, pen_width: 0, layer, layer_group: group, flag: 0 }
}

fn line(base: JwwBase, x1: f64, y1: f64, x2: f64, y2: f64) -> JwwLine {
    JwwLine { base, x1, y1, x2, y2 }
}

fn text(base: JwwBase, x: f64, y: f64, content: &str) -> JwwText {
    JwwText {
        base,
        x1: x,
        y1: y,
        x2: x + 20.0,
        y2: y,
        text_type: 2,
        size_x: 3.0,
        size_y: 3.0,
        spacing: 0.5,
        angle: 0.0,
        font: "ＭＳ ゴシック".into(),
        content: content.into(),
    }
}

fn point(base: JwwBase, x: f64, y: f64, temporary: bool) -> JwwPoint {
    JwwPoint { base, x, y, temporary, code: 0, angle: 0.0, scale: 0.0 }
}

/// どの種類の図形も持つ図面（A2、グループ 0 は 1/100 の「平面」、グループ 1 は 1/1）
fn drawing(version: u32) -> JwwDocument {
    let mut doc = JwwDocument::new();
    doc.version = version;
    doc.memo = "テストの図面\r\n2 行目".into();
    doc.paper_size = 2;
    doc.groups[0].name = "平面".into();
    doc.groups[0].scale = 100.0;
    doc.groups[0].layers[0].name = "通り芯".into();
    doc.groups[0].layers[2].state = 0;
    doc.groups[1].state = 1;
    doc.groups[2].scale = 50.0;
    let mut dimension_points =
        vec![point(base(0, 1, 2, 1), 0.0, -10.0, false), point(base(0, 1, 2, 1), 100.0, -10.0, false)];
    dimension_points.push(point(base(0, 1, 2, 1), 0.0, 0.0, true));
    dimension_points.push(point(base(0, 1, 2, 1), 100.0, 0.0, true));
    doc.entities = vec![
        JwwEntity::Line(line(base(0, 0, 2, 2), -100.0, 0.0, 0.0, 0.0)),
        JwwEntity::Arc(JwwArc {
            base: base(0, 1, 3, 1),
            cx: 10.0,
            cy: 20.0,
            radius: 5.0,
            start: 0.5,
            sweep: -PI / 2.0,
            tilt: 0.25,
            flatness: 0.5,
            full: false,
        }),
        JwwEntity::Arc(JwwArc {
            base: base(0, 1, 4, 1),
            cx: 30.0,
            cy: 20.0,
            radius: 5.0,
            start: 0.0,
            sweep: 2.0 * PI,
            tilt: 0.0,
            flatness: 1.0,
            full: true,
        }),
        JwwEntity::Point(point(base(0, 1, 5, 1), 40.0, 25.0, false)),
        JwwEntity::Point(JwwPoint { code: 3, angle: 0.5, scale: 2.0, ..point(base(0, 1, 6, 100), 45.0, 25.0, false) }),
        JwwEntity::Text(text(base(0, 1, 7, 1), -100.0, 10.0, "通り芯 X1")),
        JwwEntity::Solid(JwwSolid {
            base: base(0, 1, 10, 1),
            points: [-90.0, 20.0, -80.0, 20.0, -80.0, 30.0, -90.0, 30.0],
            color: Some(0x0080ff),
        }),
        JwwEntity::Solid(JwwSolid {
            base: base(0, 1, 2, 1),
            points: [-70.0, 20.0, -60.0, 20.0, -60.0, 30.0, -70.0, 30.0],
            color: None,
        }),
        JwwEntity::CircleSolid(JwwCircleSolid {
            base: base(0, 1, 5, 101),
            cx: -50.0,
            cy: 25.0,
            radius: 5.0,
            flatness: 1.0,
            tilt: 0.0,
            start: 0.0,
            sweep: PI / 2.0,
            mode: 0.0,
            color: None,
        }),
        JwwEntity::Block(JwwBlock {
            base: base(0, 1, 2, 1),
            x: -20.0,
            y: 25.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 0.25,
            definition: 7,
        }),
        JwwEntity::Dimension(JwwDimension {
            base: base(0, 1, 2, 1),
            line: line(base(0, 1, 2, 1), 0.0, -10.0, 100.0, -10.0),
            text: text(base(0, 1, 2, 1), 45.0, -9.0, "10000"),
            lines: vec![
                line(base(0, 1, 2, 1), 0.0, -1.0, 0.0, -12.0),
                line(base(0, 1, 2, 1), 100.0, -1.0, 100.0, -12.0),
            ],
            points: dimension_points,
        }),
        // 1/1 のグループの図枠
        JwwEntity::Line(line(base(1, 0, 2, 1), -200.0, -140.0, 200.0, -140.0)),
    ];
    doc.blocks = vec![
        JwwBlockDefinition {
            number: 7,
            name: "部品".into(),
            entities: vec![
                JwwEntity::Line(line(base(0, 0, 2, 2), 0.0, 0.0, 5.0, 0.0)),
                JwwEntity::Block(JwwBlock {
                    base: base(0, 0, 2, 1),
                    x: 1.0,
                    y: 1.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    definition: 8,
                }),
            ],
        },
        JwwBlockDefinition {
            number: 8,
            name: "小部品@@SfigorgFlag@@4".into(),
            entities: vec![JwwEntity::Text(text(base(0, 0, 2, 1), 0.0, 0.0, "記号"))],
        },
    ];
    doc
}

/// 書込みレイヤグループ（Jw_cad は状態 3）
fn normalized(mut doc: JwwDocument) -> JwwDocument {
    doc.groups[0].state = 3;
    doc
}

#[test]
fn reads_the_new_empty_drawing_as_written() {
    let doc = JwwDocument::new();
    assert_eq!((doc.version, doc.paper_size, doc.groups.len(), doc.groups[0].layers.len()), (JWW_VERSION, 3, 16, 16));
    assert_eq!((doc.groups[0].name.as_str(), doc.groups[0].layers[1].name.as_str()), ("グループ 0", "0-1"));
    assert_eq!(doc.pen_colors.as_ref().map(|colors| colors[2].as_str()), Some("#000000"));
    assert_eq!(doc.line_types.get(&2), Some(&vec![0.625, 0.625]));
    let written = write_jww(&doc);
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    assert_eq!(read_jww(&written.bytes).unwrap(), doc);
    // ヘッダはすべての欄を持ち、図形の一覧はヘッダのすぐ後ろ
    let file = JwwFile::read(&written.bytes).unwrap();
    let settings = file.header.settings.as_ref().unwrap();
    assert!(settings.display.is_some() && settings.sxf.is_some() && settings.text.is_some());
    assert!(file.header.rest.is_empty());
    // 図形の一覧・ブロック定義の一覧の件数 0 と、同梱画像の件数 0
    assert!(written.bytes.ends_with(&[0, 0, 0, 0, 0, 0, 0, 0]));
}

#[test]
fn reads_every_kind_of_entity_as_written() {
    for version in [700, 600] {
        let doc = normalized(drawing(version));
        let written = write_jww(&doc);
        assert!(written.notices.is_empty(), "{version}: {:?}", written.notices);
        let read = read_jww(&written.bytes).unwrap();
        assert_eq!(read, doc, "{version}");
        // 読んで書くと同じバイト列
        assert_eq!(JwwFile::read(&written.bytes).unwrap().write().bytes, written.bytes, "{version}");
    }
}

#[test]
fn writes_what_older_versions_hold() {
    // 版 420 より前は SXF 対応の色・線種と、寸法の補助線・点を持たない。版 351 より前は図形の線幅を持たない
    for version in [420, 351, 300] {
        let mut doc = normalized(drawing(version));
        // 版 600 より前は印刷の dpi を持たないので、印刷のドットの線幅を mm にできない
        doc.width_unit = WidthUnit::default();
        doc.pen_widths = Some(vec![None; 10]);
        if version < 420 {
            for entity in &mut doc.entities {
                if let JwwEntity::Dimension(dimension) = entity {
                    dimension.lines.clear();
                    dimension.points.clear();
                }
            }
        }
        // 版 410 より前のブロック名は複合図形の種別を持たない
        let written = write_jww(&doc);
        let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
        if version < 410 {
            assert_eq!(codes, ["block-name"], "{version}");
            doc.blocks[1].name = "小部品".into();
        } else {
            assert!(codes.is_empty(), "{version}: {codes:?}");
        }
        let read = read_jww(&written.bytes).unwrap();
        assert_eq!((read.version, &read.entities, &read.blocks), (version, &doc.entities, &doc.blocks), "{version}");
        assert_eq!(read.groups, doc.groups, "{version}");
        assert_eq!(read.extended_colors.is_some(), version >= 420);
        assert_eq!(read.line_types.contains_key(&31), version >= 420);
        assert_eq!(JwwFile::read(&written.bytes).unwrap().write().bytes, written.bytes, "{version}");
    }
}

#[test]
fn reports_sxf_attributes_that_older_versions_cannot_define() {
    let mut doc = normalized(drawing(351));
    doc.entities.push(JwwEntity::Line(line(base(0, 0, 105, 32), 0.0, 0.0, 10.0, 0.0)));
    let codes: Vec<String> = write_jww(&doc).notices.into_iter().map(|notice| notice.code).collect();
    assert!(codes.contains(&"sxf-attributes".to_string()), "{codes:?}");
}

#[test]
fn keeps_one_write_group_and_one_write_layer_per_group() {
    let mut doc = drawing(700);
    // 状態 3 が 2 つ以上なら最初のものを書込みにし、なければ最初の編集可能なもの（状態 2）を書込みにする
    doc.groups[0].state = 0;
    doc.groups[4].state = 3;
    doc.groups[5].state = 3;
    doc.groups[3].layers[0].state = 2;
    doc.groups[3].layers[5].state = 3;
    doc.groups[3].layers[9].state = 3;
    doc.groups[6].layers[0].state = 1;
    doc.groups[6].layers[1].state = 2;
    let read = read_jww(&write_jww(&doc).bytes).unwrap();
    let states: Vec<u32> = read.groups.iter().map(|group| group.state).collect();
    assert_eq!(states[..7], [0, 1, 2, 2, 3, 2, 2]);
    let layers = |group: usize| -> Vec<u32> { read.groups[group].layers.iter().map(|layer| layer.state).collect() };
    assert_eq!(layers(3)[..10], [2, 2, 2, 2, 2, 3, 2, 2, 2, 2]);
    assert_eq!(layers(6)[..3], [1, 3, 2]);
}

#[test]
fn writes_colors_widths_and_line_types_of_the_document() {
    let mut doc = normalized(drawing(700));
    let colors = doc.pen_colors.as_mut().unwrap();
    colors[1] = "#123456".into();
    let extended = doc.extended_colors.as_mut().unwrap();
    extended[17] = "#abcdef".into();
    doc.width_unit.hundredth = true;
    doc.pen_widths = Some((0..10).map(|n| Some(f64::from(n * 5) / 100.0)).collect());
    // 線種 2 を 1 mm の線分と 0.5 mm の空白に、SXF 対応線種 47 をユーザー定義の長さに
    doc.line_types.insert(2, vec![1.0, 0.5]);
    doc.line_types.insert(47, vec![3.0, 1.0, 0.5, 1.0]);
    let written = write_jww(&doc);
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    let read = read_jww(&written.bytes).unwrap();
    assert_eq!(read.pen_colors, doc.pen_colors);
    assert_eq!(read.extended_colors, doc.extended_colors);
    assert_eq!((read.width_unit, &read.pen_widths), (doc.width_unit, &doc.pen_widths));
    assert_eq!(read.line_types, doc.line_types);
}

#[test]
fn reports_what_it_cannot_write() {
    let mut doc = normalized(drawing(600));
    doc.entities.push(JwwEntity::Text(text(base(0, 0, 2, 1), 0.0, 0.0, "𠮷野家")));
    // ビット列に表せない長さ（32 ビットに収まらない）
    doc.line_types.insert(3, vec![0.01, 100.0]);
    let written = write_jww(&doc);
    let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert_eq!(codes, ["line-type", "text-encoding"]);
    let read = read_jww(&written.bytes).unwrap();
    let Some(JwwEntity::Text(last)) = read.entities.last() else { panic!("text") };
    assert_eq!(last.content, "?野家");
    // 正の数でない縮尺は 1/1
    let mut doc = normalized(drawing(700));
    doc.groups[5].scale = 0.0;
    let written = write_jww(&doc);
    assert_eq!(written.notices[0].code, "invalid-scale");
    assert_eq!(read_jww(&written.bytes).unwrap().groups[5].scale, 1.0);
    // 書けない版は 700 で書く
    let mut doc = drawing(800);
    doc.version = 800;
    let written = write_jww(&doc);
    assert_eq!(written.notices[0].code, "version");
    assert_eq!(read_jww(&written.bytes).unwrap().version, 700);
}

#[test]
fn marks_block_definitions_that_placements_use() {
    let mut doc = normalized(drawing(700));
    doc.blocks.push(JwwBlockDefinition { number: 9, name: "未使用".into(), entities: Vec::new() });
    let file = JwwFile::read(&write_jww(&doc).bytes).unwrap();
    let referenced: Vec<(u32, bool)> = file.blocks.iter().map(|block| (block.number, block.referenced)).collect();
    assert_eq!(referenced, [(7, true), (8, true), (9, false)]);
}

#[test]
fn converts_real_coordinates_of_a_group_to_paper_millimeters() {
    let doc = normalized(drawing(700));
    assert_eq!(doc.paper_factor(0), 0.01);
    assert_eq!(doc.paper_factor(1), 1.0);
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの JWW）の内容を新しい図面として書き、読むと同じ内容になること。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-jww --test write -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn writes_the_contents_of_the_samples() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut checked = 0;
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jww")) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let doc = read_jww(&std::fs::read(&path).unwrap()).unwrap();
        let written = write_jww(&doc);
        assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
        let read = read_jww(&written.bytes).unwrap();
        assert_eq!(read, doc, "{name}");
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn reads_back_a_file_with_block_definitions_only() {
    // 図形の一覧が空でブロック定義だけの図面。JwwFile はヘッダの終わりから図形の一覧を読む
    let mut doc = normalized(drawing(700));
    doc.entities.clear();
    let written = write_jww(&doc);
    let file = JwwFile::read(&written.bytes).unwrap();
    assert_eq!((file.entities.len(), file.blocks.len()), (0, 2));
    let entities: Vec<&JwwEntity> = file.blocks[0].entities.iter().map(|record| &record.entity).collect();
    assert_eq!(entities, doc.blocks[0].entities.iter().collect::<Vec<_>>());
    assert_eq!(file.write().bytes, written.bytes);
}

#[test]
fn writes_versions_before_300_with_their_header_fields() {
    let length = |version: u32| {
        let mut doc = JwwDocument::new();
        doc.version = version;
        // 版 600 より前は印刷の dpi を持たない
        doc.width_unit = WidthUnit::default();
        doc.pen_widths = Some(vec![None; 10]);
        let written = write_jww(&doc);
        assert!(written.notices.is_empty(), "{version}: {:?}", written.notices);
        let file = JwwFile::read(&written.bytes).unwrap();
        let settings = file.header.settings.as_ref().unwrap();
        assert!(settings.display.is_some() && settings.text.is_some() && file.header.rest.is_empty(), "{version}");
        assert_eq!(file.write().bytes, written.bytes, "{version}");
        written.bytes.len()
    };
    // 300 より前: 天空図（16 バイト）、マークジャンプ 5〜8 とレイヤグループ（128）、文字の描画状態（56）がない
    assert_eq!(length(300) - length(252), 200);
    // 230 より前はソリッドの任意色（8）、225 より前は最終値（32）、223 より前は作図時間と 2.5D の視点（60）がない
    assert_eq!(length(230) - length(225), 8);
    assert_eq!(length(225) - length(223), 32);
    assert_eq!(length(223) - length(200), 60);
}

#[test]
fn fits_what_older_versions_cannot_hold() {
    let mut doc = normalized(drawing(700));
    doc.version = 200;
    doc.width_unit = WidthUnit::default();
    doc.pen_widths = Some(vec![None; 10]);
    doc.entities.retain(|entity| !matches!(entity, JwwEntity::Dimension(_)));
    doc.entities.push(JwwEntity::Line(line(JwwBase { pen_width: 25, ..base(0, 0, 2, 1) }, 0.0, 0.0, 1.0, 0.0)));
    doc.entities.push(JwwEntity::Text(text(
        base(0, 0, 2, 1),
        0.0,
        0.0,
        "^@BM%temp%zumen.bmp,100,50,0,0,1,0,255,255,255",
    )));
    let written = write_jww(&doc);
    let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
    assert_eq!(codes, ["entity-width", "point-symbol", "circle-solid", "block-name", "embedded-image"]);
    let read = read_jww(&written.bytes).unwrap();
    // 円のソリッド（90° の扇形）は、中心からの三角形のソリッドに分ける
    let solids = read.entities.iter().filter(|entity| matches!(entity, JwwEntity::Solid(_))).count();
    assert_eq!(solids, 2 + 8);
    assert!(read.entities.iter().all(|entity| !matches!(entity, JwwEntity::CircleSolid(_))));
    // 記号の点はふつうの点に、ブロック名は複合図形の種別を除く
    assert!(read.entities.iter().all(|entity| entity.base().pen_style != 100));
    assert_eq!(read.blocks[1].name, "小部品");
}
