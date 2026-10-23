//! SXF・JWW から DXF への変換: tests/fixtures の SXF（SFC・P21）と JWW を DXF に書き直して読み直すと、
//! DXF を開いたときの表示（用紙のレイアウト）に、元の図面と同じに描ける（比べ方は common/mod.rs）。

mod common;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use zumen::dxf::{DxfDocument, DxfEntity, DxfEntityData, DxfSpace, DxfViewport};
use zumen::jww::{
    JwwArc, JwwBase, JwwBlock, JwwBlockDefinition, JwwCircleSolid, JwwDimension, JwwDocument, JwwEntity, JwwLayer,
    JwwLayerGroup, JwwLine, JwwPoint, JwwSolid, JwwText, WidthUnit,
};
use zumen::mapping::Mapped;
use zumen::{ReadDrawingOptions, Scene};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// 図面を AutoCAD の図面のモデルに写して DXF に書き、読み直した表示と、ハンドル → 元の図形の id
fn convert(bytes: &[u8]) -> (Scene, HashMap<String, String>, Vec<zumen::Notice>) {
    let format = zumen::detect(bytes).expect("読める図面");
    let mapped = match format {
        "jww" => zumen::mapping::jww_to_dxf(&zumen::jww::read_jww(bytes).unwrap()),
        _ => zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sxf(bytes, format).unwrap()),
    };
    let written = zumen::dxf::write_dxf(&mapped.document);
    let again = zumen::dxf::parse_dxf(&written.bytes);
    (zumen::dxf::dxf_to_scene(&again, None), mapped.sources, mapped.notices)
}

/// 図面を DXF に書き直し、元と比べる。比べた図形の数と、違いを返す
fn check(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let (original, distorted) = common::scale_texts(&zumen::read(bytes, &ReadDrawingOptions::default()).unwrap(), true);
    let (again, sources, _) = convert(bytes);
    let differences = common::differences(&original, &again, &sources)
        .into_iter()
        .filter(|line| !common::is_text_place(line, &distorted))
        .map(|line| format!("{name}: {line}"))
        .collect();
    (original.entities.len(), differences)
}

fn drawings(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "sfc" | "p21" | "jww"))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn converted_sxf_and_jww_draw_the_same() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for format in ["sfc", "p21", "jww"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (entities, differences) = check(&name, &fs::read(&file).unwrap());
            checked += entities;
            failures.extend(differences);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 100, "checked only {checked} entities");
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの SFC・P21・JWW）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test convert_sxf_jww -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn converted_sxf_and_jww_samples_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in drawings(Path::new(&folder)) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let (entities, differences) = check(&name, &fs::read(&file).unwrap());
        println!("{name}: {entities} entities, {} differences", differences.len());
        failures.extend(differences.into_iter().take(10));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// --- 写し方の規則（合成した小さな図面） --------------------------------------------------------------------

/// SFC の図面（フィーチャを 1 つずつ /*SXF … SXF*/ に書く。文字列は CP932）
fn sfc(features: &[&str]) -> Vec<u8> {
    let mut text = String::from(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');\n\
         FILE_NAME('test.sfc','2026-10-20T00:00:00',('author'),('org'),'SCADEC_API_Ver3.10$$3.1','test','');\n\
         FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));\nENDSEC;\nDATA;\n",
    );
    for feature in features {
        text.push_str("/*SXF\n");
        text.push_str(feature);
        text.push_str("\nSXF*/\n");
    }
    text.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    encoding_rs::SHIFT_JIS.encode(&text).0.into_owned()
}

/// 写して DXF に書き、読み直した図面のモデル
fn written(mapped: &Mapped) -> DxfDocument {
    zumen::dxf::parse_dxf(&zumen::dxf::write_dxf(&mapped.document).bytes)
}

/// モデル空間・紙空間の、元の図形 source から書いた図形（ブロックの中の図形は、ブロックの中の id を持つので除く）
fn handle_of<'a>(doc: &'a DxfDocument, mapped: &Mapped, source: &str) -> Vec<&'a DxfEntity> {
    doc.entities
        .iter()
        .filter(|entity| entity.base.space != DxfSpace::Block)
        .filter(|entity| mapped.sources.get(&entity.base.handle).is_some_and(|id| id == source))
        .collect()
}

fn viewports(doc: &DxfDocument) -> Vec<&DxfViewport> {
    doc.entities
        .iter()
        .filter_map(|entity| match &entity.data {
            DxfEntityData::Viewport(viewport) if viewport.id != 1.0 => Some(viewport),
            _ => None,
        })
        .collect()
}

fn codes(notices: &[zumen::Notice]) -> Vec<&str> {
    notices.iter().map(|notice| notice.code.as_str()).collect()
}

fn assert_same_drawing(bytes: &[u8]) {
    let (_, differences) = check("drawing", bytes);
    assert!(differences.is_empty(), "{differences:#?}");
}

#[test]
fn places_sub_figures_in_real_size_model_space_and_shows_them_through_viewports() {
    let bytes = sfc(&[
        r"#10 = layer_feature(\'FRAME\','1')",
        r"#20 = layer_feature(\'PLAN\','0')",
        r"#30 = text_font_feature(\'ＭＳ 明朝\')",
        // 部分図の中は実寸（mm）。線・円・縦書きの文字
        "#100 = line_feature('2','1','1','1','0.0','0.0','1000.0','0.0')",
        "#110 = circle_feature('2','2','1','1','500.0','0.0','100.0')",
        r"#120 = text_string_feature('2','1','1',\'縦書き\','100.0','100.0','300.0','100.0','0.0','0.0','0.0','1','2')",
        r"#200 = sfig_org_feature(\'PLAN\','1')",
        // 用紙にじかに描いた図枠と、1/100・30° の部分図の配置
        "#300 = line_feature('1','1','1','1','10.0','10.0','410.0','10.0')",
        r"#310 = sfig_locate_feature('2',\'PLAN\','100.0','50.0','30.0','0.01','0.01')",
        r"#400 = drawing_sheet_feature(\'S\','3','1','0','0')",
    ]);
    assert_same_drawing(&bytes);
    let mapped = zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sfc(&bytes).unwrap());
    let doc = written(&mapped);
    // 部分図の線はモデル空間に実寸で、図枠は紙空間に用紙の mm で
    let line = handle_of(&doc, &mapped, "#100")[0];
    assert_eq!(line.base.space, DxfSpace::Model);
    assert_eq!(line.data, DxfEntityData::Line { start: [0.0, 0.0, 0.0], end: [1000.0, 0.0, 0.0] });
    assert_eq!(line.base.layer, "PLAN");
    let frame = handle_of(&doc, &mapped, "#300")[0];
    assert_eq!((frame.base.space, frame.base.layer.as_str()), (DxfSpace::Paper, "FRAME"));
    // 部分図の配置は、縮尺 1/100・ねじれ 30° のビューポート（元の配置の番号を持つ）
    let [viewport] = viewports(&doc)[..] else { panic!("one viewport") };
    assert!((viewport.height / viewport.view_height - 0.01).abs() < 1e-12);
    assert!((viewport.view_twist_angle - 30.0).abs() < 1e-9);
    assert_eq!(handle_of(&doc, &mapped, "#310").len(), 1);
    // 非表示のレイヤ、用紙、開く空間（レイアウト）
    assert!(doc.layers.iter().any(|layer| layer.name == "PLAN" && layer.off));
    let layout = doc.layouts.iter().find(|layout| !layout.model).unwrap();
    assert_eq!((layout.name.as_str(), layout.paper_width, layout.paper_height), ("S", 420.0, 297.0));
    assert_eq!(doc.header.tilemode, 0.0);
    // 縦書きの文字は、縦書きのフォント（@）の文字を 270° 回して書く
    let DxfEntityData::Text(text) = &handle_of(&doc, &mapped, "#120")[0].data else { panic!("TEXT") };
    assert_eq!(text.rotation, 270.0);
    let style = doc.styles.iter().find(|style| style.name == text.style).unwrap();
    assert_eq!(style.font_family.as_deref(), Some("@ＭＳ 明朝"));
    // モデル空間の表示は実寸
    let model = zumen::dxf::dxf_to_scene(&doc, Some("Model"));
    let circle = model.entities.iter().find(|entity| entity.kind == zumen::EntityKind::Circle).unwrap();
    assert!((circle.bbox.max_x - circle.bbox.min_x - 200.0).abs() < 0.1);
}

#[test]
fn swaps_geodetic_coordinates_and_moves_sub_figures_that_overlap_in_model_space() {
    let bytes = sfc(&[
        r"#10 = layer_feature(\'A\','1')",
        // 測地座標系の部分図（X が北、Y が東）。表示の向き（X が東）にする
        "#100 = line_feature('1','1','1','1','0.0','0.0','100.0','0.0')",
        r"#110 = sfig_org_feature(\'GEO\','2')",
        // 同じ範囲の、縮尺の違う部分図
        "#200 = line_feature('1','2','1','1','0.0','0.0','50.0','50.0')",
        r"#210 = sfig_org_feature(\'DETAIL\','1')",
        r"#300 = sfig_locate_feature('1',\'GEO\','20.0','20.0','0.0','0.5','0.5')",
        r"#310 = sfig_locate_feature('1',\'DETAIL\','200.0','20.0','0.0','1.0','1.0')",
        r"#400 = drawing_sheet_feature(\'S\','3','1','0','0')",
    ]);
    assert_same_drawing(&bytes);
    let mapped = zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sfc(&bytes).unwrap());
    let doc = written(&mapped);
    let geodetic = handle_of(&doc, &mapped, "#100")[0];
    assert_eq!(geodetic.data, DxfEntityData::Line { start: [0.0, 0.0, 0.0], end: [0.0, 100.0, 0.0] });
    // 2 つ目の部分図は、1 つ目のビューポートに映る範囲（x は -2〜2）と重ならないよう右へずらし、知らせる
    let DxfEntityData::Line { start, .. } = &handle_of(&doc, &mapped, "#200")[0].data else { panic!("LINE") };
    assert_eq!(start, &[9.0, -1.0, 0.0]);
    assert!(codes(&mapped.notices).contains(&"view-moved"));
}

#[test]
fn writes_sub_figures_with_different_horizontal_and_vertical_scales_as_block_references() {
    let bytes = sfc(&[
        "#100 = line_feature('0','1','1','1','0.0','0.0','1000.0','100.0')",
        r"#110 = sfig_org_feature(\'PROFILE\','1')",
        r"#200 = sfig_locate_feature('0',\'PROFILE\','10.0','10.0','0.0','0.001','0.01')",
        r"#300 = drawing_sheet_feature(\'S\','4','0','0','0')",
    ]);
    assert_same_drawing(&bytes);
    let mapped = zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sfc(&bytes).unwrap());
    let doc = written(&mapped);
    let DxfEntityData::Insert(insert) = &handle_of(&doc, &mapped, "#200")[0].data else { panic!("INSERT") };
    assert_eq!((insert.name.as_str(), insert.xscale, insert.yscale), ("PROFILE", 0.001, 0.01));
    assert!(codes(&mapped.notices).contains(&"sfig-insert"));
    // 部分図のない図面は、用紙の座標のままモデル空間に置き、1:1 のビューポートで見せる
    let plain = sfc(&[
        "#100 = line_feature('0','1','1','1','10.0','10.0','100.0','10.0')",
        r"#300 = drawing_sheet_feature(\'S\','4','0','0','0')",
    ]);
    assert_same_drawing(&plain);
    let mapped = zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sfc(&plain).unwrap());
    let doc = written(&mapped);
    assert_eq!(handle_of(&doc, &mapped, "#100")[0].base.space, DxfSpace::Model);
    let [viewport] = viewports(&doc)[..] else { panic!("one viewport") };
    assert_eq!(viewport.height, viewport.view_height);
}

#[test]
fn maps_colors_line_types_widths_dimensions_and_text_codes() {
    let bytes = sfc(&[
        "#10 = user_defined_colour_feature('255','128','0')",
        r"#20 = user_defined_font_feature(\'$$SXF_custom\','4','(3.0,1.0,0.5,1.0)')",
        "#30 = width_feature('0.47')",
        "#100 = line_feature('0','2','2','5','0.0','0.0','100.0','0.0')",
        "#110 = line_feature('0','1','17','11','0.0','10.0','100.0','10.0')",
        "#120 = line_feature('0','17','1','1','0.0','20.0','100.0','20.0')",
        r"#130 = text_string_feature('0','1','0',\'100%%c ^2\','0.0','30.0','2.5','0.0','0.0','0.0','0.0','1','1')",
        r"#140 = linear_dim_feature('0','1','1','1','0.0','50.0','100.0','50.0','1','0.0','40.0','0.0','42.0','0.0','52.0','1','100.0','40.0','100.0','42.0','100.0','52.0','6','1','0.0','50.0','1.0','6','1','100.0','50.0','1.0','1','1',\'100\','45.0','51.0','2.5','10.0','0.5','0.0','0.0','1','1')",
        r"#300 = drawing_sheet_feature(\'S\','4','0','0','0')",
    ]);
    assert_same_drawing(&bytes);
    let mapped = zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sfc(&bytes).unwrap());
    let doc = written(&mapped);
    // 既定義色の赤は ACI 1、黒は前景色 ACI 7、ユーザ定義色はトゥルーカラー
    let base = |id: &str| handle_of(&doc, &mapped, id)[0].base.clone();
    assert_eq!((base("#100").color_index, base("#100").true_color), (1.0, None));
    assert_eq!((base("#110").color_index, base("#110").true_color), (7.0, None));
    assert_eq!(base("#120").true_color, Some(f64::from(0xff8000)));
    // 既定義線種は SXF の名前の線種、線幅 0.5 mm は 50、0.47 mm はいちばん近い 0.5 mm にして知らせる
    assert_eq!((base("#100").linetype.as_str(), base("#100").lineweight), ("dashed", 50.0));
    assert_eq!((base("#110").linetype.as_str(), base("#110").lineweight), ("$$SXF_custom", 50.0));
    let dashed = doc.linetypes.iter().find(|linetype| linetype.name == "dashed").unwrap();
    assert_eq!(dashed.dashes, [6.0, -1.5]);
    assert!(codes(&mapped.notices).contains(&"lineweight"));
    // %% と ^ は TEXT の制御コードにならないよう書く
    let DxfEntityData::Text(text) = &handle_of(&doc, &mapped, "#130")[0].data else { panic!("TEXT") };
    assert_eq!(text.text, "100%%%%c ^ 2");
    // 直線寸法は、寸法線・補助線・矢印・寸法値を表示のブロックに入れた回転寸法
    let DxfEntityData::Dimension(dimension) = &handle_of(&doc, &mapped, "#140")[0].data else { panic!("DIMENSION") };
    assert_eq!((dimension.dimtype, dimension.text.as_str()), (32.0, "100"));
    assert_eq!((dimension.defpoint2, dimension.defpoint3), (Some([0.0, 40.0, 0.0]), Some([100.0, 40.0, 0.0])));
    assert_eq!(dimension.actual_measurement, Some(100.0));
    let parts =
        doc.entities.iter().filter(|entity| entity.base.block.as_deref() == Some(&dimension.anonymous_block_name));
    let kinds: Vec<&str> = parts.map(|entity| entity.base.kind.as_str()).collect();
    assert_eq!(kinds, ["LINE", "LINE", "LINE", "SOLID", "SOLID", "TEXT"]);
}

fn jww_base(group: u16, layer: u16, color: u16, style: u8) -> JwwBase {
    JwwBase { group: 0, pen_style: style, pen_color: color, pen_width: 0, layer, layer_group: group, flag: 0 }
}

fn jww_line(base: JwwBase, x1: f64, y1: f64, x2: f64, y2: f64) -> JwwLine {
    JwwLine { base, x1, y1, x2, y2 }
}

/// A3 の JWW。グループ 0 は 1/100、1 は 1/1（図枠）、2 は 1/50
fn jww(entities: Vec<JwwEntity>, blocks: Vec<JwwBlockDefinition>) -> JwwDocument {
    let groups = (0..16)
        .map(|group| JwwLayerGroup {
            name: if group == 0 { "平面".into() } else { format!("グループ {group:X}") },
            state: if group == 3 { 1 } else { 2 },
            scale: match group {
                0 => 100.0,
                2 => 50.0,
                _ => 1.0,
            },
            layers: (0..16)
                .map(|layer| JwwLayer {
                    name: if group == 0 && layer == 0 { "通り芯".into() } else { format!("{group:X}-{layer:X}") },
                    state: if layer == 2 { 0 } else { 2 },
                })
                .collect(),
        })
        .collect();
    let colors =
        ["#ffffff", "#00c0c0", "#000000", "#00c000", "#c0c000", "#c000c0", "#0000ff", "#008080", "#ff0080", "#c0c0c0"];
    JwwDocument {
        version: 700,
        memo: String::new(),
        paper_size: 3,
        groups,
        pen_colors: Some(colors.iter().map(|color| color.to_string()).collect()),
        extended_colors: None,
        pen_widths: Some(vec![Some(0.18); 10]),
        width_unit: WidthUnit { hundredth: true, dpi: None },
        line_types: BTreeMap::from([(2, vec![1.0, 1.0])]),
        entities,
        blocks,
        truncated: None,
    }
}

fn jww_check(doc: &JwwDocument) -> Mapped {
    let (original, _) = common::scale_texts(&zumen::jww::jww_to_scene(doc), true);
    let mapped = zumen::mapping::jww_to_dxf(doc);
    let again = zumen::dxf::dxf_to_scene(&written(&mapped), None);
    let differences = common::differences(&original, &again, &mapped.sources);
    assert!(differences.is_empty(), "{differences:#?}");
    mapped
}

#[test]
fn places_jww_layer_groups_by_scale_in_model_space_and_one_to_one_groups_on_paper() {
    let text = |base: JwwBase, x: f64, y: f64, content: &str, font: &str| {
        JwwEntity::Text(JwwText {
            base,
            x1: x,
            y1: y,
            x2: x + 20.0,
            y2: y,
            text_type: 0,
            size_x: 5.0,
            size_y: 5.0,
            spacing: 0.0,
            angle: 0.0,
            font: font.into(),
            content: content.into(),
        })
    };
    let entities = vec![
        // 1/100: 線（実寸 10 m）、文字、縦書き、ソリッド、円ソリッド、点、ブロック、寸法
        JwwEntity::Line(jww_line(jww_base(0, 0, 2, 2), -100.0, 0.0, 0.0, 0.0)),
        text(jww_base(0, 1, 3, 1), -100.0, 10.0, "通り芯", "ＭＳ ゴシック"),
        text(jww_base(0, 1, 3, 1), -110.0, 40.0, "縦書き", "@ＭＳ 明朝"),
        JwwEntity::Solid(JwwSolid {
            base: jww_base(0, 1, 10, 1),
            points: [-90.0, 20.0, -80.0, 20.0, -80.0, 30.0, -90.0, 30.0],
            color: Some(0x0000ff),
        }),
        JwwEntity::CircleSolid(JwwCircleSolid {
            base: jww_base(0, 1, 5, 101),
            cx: -60.0,
            cy: 25.0,
            radius: 5.0,
            flatness: 1.0,
            tilt: 0.0,
            start: 0.0,
            sweep: std::f64::consts::FRAC_PI_2,
            mode: 0.0,
            color: None,
        }),
        JwwEntity::Point(JwwPoint {
            base: jww_base(0, 1, 6, 1),
            x: -40.0,
            y: 25.0,
            temporary: false,
            code: 0,
            angle: 0.0,
            scale: 0.0,
        }),
        JwwEntity::Block(JwwBlock {
            base: jww_base(0, 1, 2, 1),
            x: -20.0,
            y: 25.0,
            scale_x: 2.0,
            scale_y: 2.0,
            rotation: 0.0,
            definition: 7,
        }),
        JwwEntity::Dimension(JwwDimension {
            base: jww_base(0, 1, 2, 1),
            line: jww_line(jww_base(0, 1, 2, 1), -100.0, -10.0, 0.0, -10.0),
            text: match text(jww_base(0, 1, 2, 1), -55.0, -9.0, "10000", "ＭＳ ゴシック") {
                JwwEntity::Text(text) => text,
                _ => unreachable!(),
            },
            lines: vec![
                jww_line(jww_base(0, 1, 2, 1), -100.0, -1.0, -100.0, -12.0),
                jww_line(jww_base(0, 1, 2, 1), 0.0, -1.0, 0.0, -12.0),
            ],
            sxf_mode: 0,
            points: Vec::new(),
        }),
        // 1/1: 図枠（紙空間）
        JwwEntity::Line(jww_line(jww_base(1, 0, 2, 1), -200.0, -140.0, 200.0, -140.0)),
        // 1/50: 1/100 の図形と実寸の範囲が重なる円
        JwwEntity::Arc(JwwArc {
            base: jww_base(2, 0, 4, 1),
            cx: -50.0,
            cy: 0.0,
            radius: 10.0,
            start: 0.0,
            sweep: 0.0,
            tilt: 0.0,
            flatness: 1.0,
            full: true,
        }),
        // 表示のみ（編集できない）のレイヤグループ 3（1/1）の線
        JwwEntity::Line(jww_line(jww_base(3, 0, 2, 1), -200.0, 140.0, 200.0, 140.0)),
    ];
    let blocks = vec![JwwBlockDefinition {
        number: 7,
        name: "部品".into(),
        entities: vec![JwwEntity::Line(jww_line(jww_base(0, 0, 2, 2), 0.0, 0.0, 5.0, 0.0))],
    }];
    let mapped = jww_check(&jww(entities, blocks));
    let doc = written(&mapped);
    // 1/100 の線はモデル空間に実寸（座標 × 100、用紙の中心が原点）
    let line = handle_of(&doc, &mapped, "#1")[0];
    assert_eq!(line.base.space, DxfSpace::Model);
    assert_eq!(line.data, DxfEntityData::Line { start: [-10000.0, 0.0, 0.0], end: [0.0, 0.0, 0.0] });
    assert_eq!(line.base.layer, "0-0 通り芯");
    // 1/1 の図枠は紙空間に、用紙の左下を原点とする mm で
    let frame = handle_of(&doc, &mapped, "#9")[0];
    assert_eq!(frame.base.space, DxfSpace::Paper);
    assert_eq!(frame.data, DxfEntityData::Line { start: [10.0, 8.5, 0.0], end: [410.0, 8.5, 0.0] });
    // 縮尺ごとのビューポート（1/100 と 1/50）。重なる 1/50 の円はずらす
    let mut scales: Vec<f64> = viewports(&doc).iter().map(|viewport| viewport.view_height / viewport.height).collect();
    scales.sort_by(f64::total_cmp);
    assert_eq!(scales.iter().map(|scale| scale.round()).collect::<Vec<_>>(), [50.0, 100.0]);
    assert!(codes(&mapped.notices).contains(&"view-moved"));
    // ブロックの配置は縮尺を掛けた倍率、ブロックの中の線種の尺度はその逆数
    let DxfEntityData::Insert(insert) = &handle_of(&doc, &mapped, "#7")[0].data else { panic!("INSERT") };
    assert_eq!((insert.name.as_str(), insert.xscale, insert.yscale), ("部品", 200.0, 200.0));
    let member = doc.entities.iter().find(|entity| entity.base.block.as_deref() == Some("部品")).unwrap();
    assert_eq!(member.base.linetype_scale, 1.0 / 200.0);
    // レイヤ: 非表示のレイヤは非表示、表示のみのグループはロック
    let layer = |name: &str| doc.layers.iter().find(|layer| layer.name == name);
    assert!(layer("0-1").is_some_and(|layer| !layer.off && !layer.locked));
    assert!(layer("3-0").is_some_and(|layer| !layer.off && layer.locked));
    // 寸法は回転寸法、ソリッドは SOLID、円ソリッドは塗りつぶしの HATCH
    assert!(matches!(&handle_of(&doc, &mapped, "#8")[0].data, DxfEntityData::Dimension(d) if d.text == "10000"));
    assert_eq!(handle_of(&doc, &mapped, "#4")[0].base.kind, "SOLID");
    assert!(matches!(&handle_of(&doc, &mapped, "#5")[0].data, DxfEntityData::Hatch(hatch) if hatch.solid_fill));
}

#[test]
fn keeps_one_scale_jww_drawings_in_model_space_and_reports_what_is_left_out() {
    let settings = JwwEntity::Text(JwwText {
        base: jww_base(0, 0, 2, 1),
        x1: 0.0,
        y1: -1000.0,
        x2: 30.0,
        y2: -1000.0,
        text_type: 0,
        size_x: 3.0,
        size_y: 3.0,
        spacing: 0.0,
        angle: 0.0,
        font: "Century".into(),
        content: "View_Direct2d = 1".into(),
    });
    let entities = vec![
        JwwEntity::Line(jww_line(jww_base(0, 0, 2, 1), -100.0, 0.0, 0.0, 0.0)),
        JwwEntity::Line(jww_line(jww_base(0, 2, 2, 12), 0.0, 10.0, 50.0, 10.0)),
        JwwEntity::Point(JwwPoint {
            base: jww_base(0, 0, 2, 1),
            x: 0.0,
            y: 0.0,
            temporary: true,
            code: 0,
            angle: 0.0,
            scale: 0.0,
        }),
        settings,
    ];
    let mapped = jww_check(&jww(entities, Vec::new()));
    let doc = written(&mapped);
    assert!(
        doc.entities
            .iter()
            .filter(|entity| entity.base.kind != "VIEWPORT")
            .all(|entity| entity.base.space == DxfSpace::Model)
    );
    let [viewport] = viewports(&doc)[..] else { panic!("one viewport") };
    assert!((viewport.view_height / viewport.height - 100.0).abs() < 1e-9);
    assert!(doc.layers.iter().any(|layer| layer.name == "0-2" && layer.off));
    let codes = codes(&mapped.notices);
    for code in ["temporary-point", "jw-settings", "random-line", "layer-group"] {
        assert!(codes.contains(&code), "{code} not in {codes:?}");
    }
}
