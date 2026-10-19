//! DWG の読み取りのテスト。図面はテストの中で書く（tests/writer）。実際の図面はリポジトリに入れない。
//!
//! 同じ内容の図面を DWG（R2000 と R2018）と DXF で書き、DWG から読んだ AutoCAD の図面のモデルと表示用の図面が、
//! DXF から読んだものと同じになることを確かめる。

mod writer;

use serde_json::Value;
use std::f64::consts::PI;
use writer::*;
use zumen_dxf::{DxfDocument, DxfEntityData, dxf_to_scene, parse_dxf};

/// 表と図形のハンドル（DWG と DXF で同じ）
const MODEL: u64 = 0x1f;
const PAPER: u64 = 0x1b;
const BOX: u64 = 0x30;
const DIM_BLOCK: u64 = 0x34;
const LAYER_0: u64 = 0x10;
const LAYER_PLAN: u64 = 0x11;
const LAYER_HIDDEN: u64 = 0x12;
const STANDARD: u64 = 0x18;
const DIMSTYLE: u64 = 0x19;

/// 図形の置き場所。R2000 は前後の図形を持たない（Nolinks = 1 でハンドル ±1）、R2018 はブロックが図形を並べる
fn place(layer: u64, color: i16, owner: Option<u64>) -> Place {
    Place { layer, color, links: None, owner }
}

fn header(insunits: i16, mlinestyles: u64) -> Header {
    Header {
        insunits,
        ltscale: 1.0,
        tilemode: true,
        block_control: 0x1,
        layer_control: 0x2,
        style_control: 0x3,
        linetype_control: 0x5,
        appid_control: 0x9,
        dimstyle_control: 0xa,
        mlinestyle_dictionary: mlinestyles,
        named_objects: 0xc,
        layouts: 0xd,
        paper_space: PAPER,
        model_space: MODEL,
        bylayer: 0x15,
        byblock: 0x14,
        continuous: 0x16,
    }
}

/// テストの図面。単位は m、レイヤ 0・平面図・非表示（非表示は切ってある）、ブロック BOX と寸法のブロック *D1、
/// モデル空間の線・文字（CP932 の 2 バイト目が 0x5C の字）・複数行文字（改行）・配置・円弧・ポリライン・寸法と、
/// レイアウト Layout1 の線
fn drawing(version: Version) -> Vec<u8> {
    let v = version;
    let model = place(LAYER_PLAN, 256, None);
    let model_entities = [0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47];
    let objects = vec![
        (0x1, control(v, 48, 0x1, &[BOX, DIM_BLOCK], &[MODEL, PAPER])),
        (0x2, control(v, 50, 0x2, &[LAYER_0, LAYER_PLAN, LAYER_HIDDEN], &[])),
        (0x3, control(v, 52, 0x3, &[STANDARD], &[])),
        (0x5, control(v, 56, 0x5, &[0x16, 0x17], &[0x15, 0x14])),
        (0x9, control(v, 66, 0x9, &[], &[])),
        (0xa, control(v, 68, 0xa, &[DIMSTYLE], &[])),
        (0xc, dictionary(v, 0xc, 0, &[("ACAD_LAYOUT", 0xd), ("ACAD_MLINESTYLE", 0xe)])),
        (0xd, dictionary(v, 0xd, 0xc, &[("Layout1", 0x23), ("Model", 0x22)])),
        (0xe, dictionary(v, 0xe, 0xc, &[])),
        (LAYER_0, layer(v, LAYER_0, 0x2, "0", 7, false, 0x16)),
        (LAYER_PLAN, layer(v, LAYER_PLAN, 0x2, "平面図", 2, false, 0x16)),
        (LAYER_HIDDEN, layer(v, LAYER_HIDDEN, 0x2, "非表示", 3, true, 0x16)),
        (0x14, linetype(v, 0x14, 0x5, "ByBlock", "", &[])),
        (0x15, linetype(v, 0x15, 0x5, "ByLayer", "", &[])),
        (0x16, linetype(v, 0x16, 0x5, "Continuous", "Solid line", &[])),
        (0x17, linetype(v, 0x17, 0x5, "DASHED", "Dashed", &[0.5, -0.25])),
        (STANDARD, style(v, STANDARD, 0x3, "Standard", "txt")),
        (DIMSTYLE, dimstyle(v, DIMSTYLE, 0xa, "Standard", 2.5)),
        (MODEL, block_header(v, MODEL, "*Model_Space", false, 0x20, 0x21, &model_entities, 0x22)),
        (0x20, block_entity(v, 0x20, MODEL, "*Model_Space")),
        (0x21, endblk(v, 0x21, MODEL)),
        (PAPER, block_header(v, PAPER, "*Paper_Space", false, 0x1c, 0x1d, &[0x50], 0x23)),
        (0x1c, block_entity(v, 0x1c, PAPER, "*Paper_Space")),
        (0x1d, endblk(v, 0x1d, PAPER)),
        (BOX, block_header(v, BOX, "BOX", false, 0x31, 0x32, &[0x33], 0)),
        (0x31, block_entity(v, 0x31, BOX, "BOX")),
        (0x32, endblk(v, 0x32, BOX)),
        (0x33, line(v, 0x33, &place(LAYER_0, 256, Some(BOX)), [0.0, 0.0, 0.0], [1.0, 1.0, 0.0])),
        // 無名のブロックは BLOCK_HEADER が *D だけを持ち、番号は BLOCK が持つ
        (DIM_BLOCK, block_header(v, DIM_BLOCK, "*D", true, 0x35, 0x36, &[0x37], 0)),
        (0x35, block_entity(v, 0x35, DIM_BLOCK, "*D1")),
        (0x36, endblk(v, 0x36, DIM_BLOCK)),
        (0x37, line(v, 0x37, &place(LAYER_0, 256, Some(DIM_BLOCK)), [0.0, 3.0, 0.0], [4.0, 3.0, 0.0])),
        (0x22, layout(v, 0x22, 0xd, "Model", 0, MODEL, [420.0, 297.0])),
        (0x23, layout(v, 0x23, 0xd, "Layout1", 1, PAPER, [420.0, 297.0])),
        (0x40, line(v, 0x40, &model, [0.0, 0.0, 0.0], [1.0, 0.5, 0.0])),
        (0x41, text(v, 0x41, &model, "表示ソ能", [0.0, 1.0], 0.25, PI / 6.0, STANDARD)),
        (0x42, mtext(v, 0x42, &model, "一行目\n二行目", [0.0, 2.0, 0.0], 0.25, STANDARD)),
        (0x43, insert(v, 0x43, &model, BOX, [2.0, 0.0, 0.0], [2.0, 2.0, 1.0], PI / 2.0)),
        (0x44, arc(v, 0x44, &place(LAYER_0, 1, None), [5.0, 5.0, 0.0], 1.0, [0.0, PI / 2.0])),
        (
            0x45,
            lwpolyline(
                v,
                0x45,
                &place(LAYER_0, 256, None),
                &[[0.0, -2.0], [2.0, -2.0], [2.0, -1.0]],
                &[0.5, 0.0, 0.0],
                true,
            ),
        ),
        (0x46, line(v, 0x46, &place(LAYER_HIDDEN, 256, None), [0.0, -3.0, 0.0], [1.0, -3.0, 0.0])),
        (
            0x47,
            linear_dimension(
                v,
                0x47,
                &place(LAYER_0, 256, None),
                [[4.0, 3.0, 0.0], [0.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
                [2.0, 3.5],
                4.0,
                DIMSTYLE,
                DIM_BLOCK,
            ),
        ),
        (0x50, line(v, 0x50, &place(LAYER_0, 256, None), [10.0, 10.0, 0.0], [100.0, 10.0, 0.0])),
    ];
    // 文字コード 38 は ANSI_932（Shift_JIS）
    Drawing { version, codepage: 38, header: header(6, 0xe), classes: Vec::new(), objects, measurement: 1 }.bytes()
}

/// 同じ図面の DXF（R2000、ANSI_932）。グループコードと値を | で区切って並べる
fn dxf() -> Vec<u8> {
    let text = [
        "0|SECTION|2|HEADER",
        "9|$ACADVER|1|AC1015|9|$DWGCODEPAGE|3|ANSI_932|9|$INSUNITS|70|6|9|$MEASUREMENT|70|1",
        "9|$LTSCALE|40|1.0|9|$PSLTSCALE|70|1|9|$TEXTSIZE|40|2.5|9|$DIMSCALE|40|1.0|9|$DIMASZ|40|2.5",
        "9|$DIMTXT|40|2.5|9|$TILEMODE|70|1",
        "0|ENDSEC|0|SECTION|2|TABLES",
        "0|TABLE|2|LTYPE",
        "0|LTYPE|5|14|2|ByBlock|70|0|3||72|65|73|0|40|0.0",
        "0|LTYPE|5|15|2|ByLayer|70|0|3||72|65|73|0|40|0.0",
        "0|LTYPE|5|16|2|Continuous|70|0|3|Solid line|72|65|73|0|40|0.0",
        "0|LTYPE|5|17|2|DASHED|70|0|3|Dashed|72|65|73|2|40|0.75|49|0.5|74|0|49|-0.25|74|0",
        "0|ENDTAB|0|TABLE|2|LAYER",
        "0|LAYER|5|10|2|0|70|0|62|7|6|Continuous",
        "0|LAYER|5|11|2|平面図|70|0|62|2|6|Continuous",
        "0|LAYER|5|12|2|非表示|70|0|62|-3|6|Continuous",
        "0|ENDTAB|0|TABLE|2|STYLE",
        "0|STYLE|5|18|2|Standard|70|0|40|0.0|41|1.0|50|0.0|71|0|42|2.5|3|txt|4|",
        "0|ENDTAB|0|TABLE|2|DIMSTYLE",
        "0|DIMSTYLE|105|19|2|Standard|70|0|40|1.0|41|2.5|140|2.5",
        "0|ENDTAB|0|TABLE|2|BLOCK_RECORD",
        "0|BLOCK_RECORD|5|1F|2|*Model_Space|0|BLOCK_RECORD|5|1B|2|*Paper_Space",
        "0|BLOCK_RECORD|5|30|2|BOX|0|BLOCK_RECORD|5|34|2|*D1",
        "0|ENDTAB|0|ENDSEC|0|SECTION|2|BLOCKS",
        "0|BLOCK|5|20|330|1F|100|AcDbEntity|8|0|100|AcDbBlockBegin|2|*Model_Space|70|0|10|0|20|0|30|0",
        "0|ENDBLK|5|21|330|1F|100|AcDbEntity|8|0|100|AcDbBlockEnd",
        "0|BLOCK|5|1C|330|1B|100|AcDbEntity|8|0|100|AcDbBlockBegin|2|*Paper_Space|70|0|10|0|20|0|30|0",
        "0|ENDBLK|5|1D|330|1B|100|AcDbEntity|8|0|100|AcDbBlockEnd",
        "0|BLOCK|5|31|330|30|100|AcDbEntity|8|0|100|AcDbBlockBegin|2|BOX|70|0|10|0|20|0|30|0",
        "0|LINE|5|33|330|30|100|AcDbEntity|8|0|100|AcDbLine|10|0|20|0|30|0|11|1|21|1|31|0",
        "0|ENDBLK|5|32|330|30|100|AcDbEntity|8|0|100|AcDbBlockEnd",
        "0|BLOCK|5|35|330|34|100|AcDbEntity|8|0|100|AcDbBlockBegin|2|*D1|70|1|10|0|20|0|30|0",
        "0|LINE|5|37|330|34|100|AcDbEntity|8|0|100|AcDbLine|10|0|20|3|30|0|11|4|21|3|31|0",
        "0|ENDBLK|5|36|330|34|100|AcDbEntity|8|0|100|AcDbBlockEnd",
        "0|ENDSEC|0|SECTION|2|ENTITIES",
        "0|LINE|5|40|330|1F|100|AcDbEntity|8|平面図|100|AcDbLine|10|0|20|0|30|0|11|1|21|0.5|31|0",
        "0|TEXT|5|41|330|1F|100|AcDbEntity|8|平面図|100|AcDbText|10|0|20|1|30|0|40|0.25|1|表示ソ能|50|30",
        "7|Standard|100|AcDbText",
        "0|MTEXT|5|42|330|1F|100|AcDbEntity|8|平面図|100|AcDbMText|10|0|20|2|30|0|40|0.25|41|0|71|1|72|1",
        "1|一行目^J二行目|7|Standard|73|1|44|1.0",
        "0|INSERT|5|43|330|1F|100|AcDbEntity|8|平面図|100|AcDbBlockReference|2|BOX|10|2|20|0|30|0|41|2|42|2",
        "43|1|50|90",
        "0|ARC|5|44|330|1F|100|AcDbEntity|8|0|62|1|100|AcDbCircle|10|5|20|5|30|0|40|1|100|AcDbArc|50|0|51|90",
        "0|LWPOLYLINE|5|45|330|1F|100|AcDbEntity|8|0|100|AcDbPolyline|90|3|70|1|10|0|20|-2|42|0.5|10|2|20|-2",
        "10|2|20|-1",
        "0|LINE|5|46|330|1F|100|AcDbEntity|8|非表示|100|AcDbLine|10|0|20|-3|30|0|11|1|21|-3|31|0",
        "0|DIMENSION|5|47|330|1F|100|AcDbEntity|8|0|100|AcDbDimension|2|*D1|10|4|20|3|30|0|11|2|21|3.5|31|0",
        "70|0|1||71|5|42|4|3|Standard|100|AcDbAlignedDimension|13|0|23|0|33|0|14|4|24|0|34|0",
        "100|AcDbRotatedDimension",
        "0|LINE|5|50|330|1B|100|AcDbEntity|67|1|8|0|100|AcDbLine|10|10|20|10|30|0|11|100|21|10|31|0",
        "0|ENDSEC|0|SECTION|2|OBJECTS",
        "0|DICTIONARY|5|C|330|0|100|AcDbDictionary|281|1|3|ACAD_LAYOUT|350|D|3|ACAD_MLINESTYLE|350|E",
        "0|DICTIONARY|5|D|330|C|100|AcDbDictionary|281|1|3|Layout1|350|23|3|Model|350|22",
        "0|DICTIONARY|5|E|330|C|100|AcDbDictionary|281|1",
        "0|LAYOUT|5|22|330|D|100|AcDbPlotSettings|1||4|ISO_A3_(420.00_x_297.00_MM)|40|0|41|0|42|0|43|0",
        "44|420|45|297|46|0|47|0|72|1|73|0|142|1|143|1",
        "100|AcDbLayout|1|Model|70|1|71|0|10|0|20|0|11|420|21|297|330|1F",
        "0|LAYOUT|5|23|330|D|100|AcDbPlotSettings|1||4|ISO_A3_(420.00_x_297.00_MM)|40|0|41|0|42|0|43|0",
        "44|420|45|297|46|0|47|0|72|1|73|0|142|1|143|1",
        "100|AcDbLayout|1|Layout1|70|1|71|1|10|0|20|0|11|420|21|297|330|1B",
        "0|ENDSEC|0|EOF",
    ]
    .join("|")
    .replace('|', "\r\n");
    encoding_rs::SHIFT_JIS.encode(&text).0.into_owned()
}

/// 数を相対 1e-9 まで同じとみなして比べる（DWG はラジアン、DXF は度で持つ角度の丸めの差）
fn differences(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() > 1e-9 * x.abs().max(y.abs()).max(1.0) {
                out.push(format!("{path}: {x} ≠ {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(format!("{path}: length {} ≠ {}", x.len(), y.len()));
                return;
            }
            for (index, (x, y)) in x.iter().zip(y).enumerate() {
                differences(&format!("{path}[{index}]"), x, y, out);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                match (x.get(key), y.get(key)) {
                    (Some(x), Some(y)) => differences(&format!("{path}.{key}"), x, y, out),
                    (x, y) => out.push(format!("{path}.{key}: {x:?} ≠ {y:?}")),
                }
            }
        }
        _ => {
            if a != b {
                out.push(format!("{path}: {a} ≠ {b}"));
            }
        }
    }
}

/// 表示用の図面を比べる。形式・版・メタデータ（DWG か DXF か）は比べない
fn scene_differences(dwg: &DxfDocument, dxf: &DxfDocument, view: Option<&str>) -> Vec<String> {
    let mut scenes =
        [dxf_to_scene(dwg, view), dxf_to_scene(dxf, view)].map(|scene| serde_json::to_value(scene).unwrap());
    for scene in &mut scenes {
        let object = scene.as_object_mut().unwrap();
        object.remove("format");
        object.remove("version");
        object.remove("metadata");
    }
    let mut out = Vec::new();
    differences("", &scenes[0], &scenes[1], &mut out);
    out
}

/// DWG の値の形（ビット単位の値、ラジアンの角度、ハンドルで引く表・スタイル・ブロック、CP932・UTF-16 の文字列、
/// 文字列の中の改行）を取り違えると、DXF から読んだ図面と違う図面になる
#[test]
fn a_drawing_reads_the_same_as_its_dxf() {
    let dxf = parse_dxf(&dxf());
    assert!(dxf.notices.is_empty(), "{:?}", dxf.notices);
    for version in [Version::R2000, Version::R2018] {
        let dwg = zumen_dwg::read_dwg(&drawing(version)).unwrap();
        assert!(dwg.notices.is_empty(), "{version:?}: {:?}", dwg.notices);
        assert_eq!(dwg.entities.len(), dxf.entities.len(), "{version:?}");
        let text = dwg.entities.iter().find(|entity| entity.base.handle == "41").unwrap();
        let DxfEntityData::Text(text) = &text.data else { panic!("TEXT") };
        assert_eq!(text.text, "表示ソ能");
        let mtext = dwg.entities.iter().find(|entity| entity.base.handle == "42").unwrap();
        let DxfEntityData::MText(mtext) = &mtext.data else { panic!("MTEXT") };
        // 改行は DXF と同じキャレット表記（^J）で持つ
        assert_eq!(mtext.raw_text, "一行目^J二行目");
        let blocks: Vec<&str> = dwg.blocks.iter().map(|block| block.name.as_str()).collect();
        assert_eq!(blocks, ["*Model_Space", "*Paper_Space", "BOX", "*D1"]);
        assert!(dwg.layers.iter().any(|layer| layer.name == "非表示" && layer.off));
        assert_eq!(dwg.header.insunits, 6.0);
        assert_eq!(dwg.header.measurement, Some(1.0));
        for view in [None, Some("Model"), Some("Layout1")] {
            let out = scene_differences(&dwg, &dxf, view);
            assert!(out.is_empty(), "{version:?} {view:?}\n{}", out.join("\n"));
        }
    }
}

/// ブロックとモデル空間の図形だけの小さな図面（単位は mm、文字コードは ANSI_1252）
fn small(version: Version, extra: Vec<(u64, Vec<u8>)>, model_entities: &[u64], block_list: &[u64]) -> Vec<u8> {
    let v = version;
    let mut objects: Vec<(u64, Vec<u8>)> = vec![
        (0x1, control(v, 48, 0x1, block_list, &[MODEL, PAPER])),
        (0x2, control(v, 50, 0x2, &[LAYER_0], &[])),
        (0x3, control(v, 52, 0x3, &[STANDARD], &[])),
        (0x5, control(v, 56, 0x5, &[0x16], &[0x15, 0x14])),
        (0x9, control(v, 66, 0x9, &[], &[])),
        (0xa, control(v, 68, 0xa, &[DIMSTYLE], &[])),
        (0xc, dictionary(v, 0xc, 0, &[("ACAD_LAYOUT", 0xd)])),
        (0xd, dictionary(v, 0xd, 0xc, &[("Model", 0x22)])),
        (LAYER_0, layer(v, LAYER_0, 0x2, "0", 7, false, 0x16)),
        (0x14, linetype(v, 0x14, 0x5, "ByBlock", "", &[])),
        (0x15, linetype(v, 0x15, 0x5, "ByLayer", "", &[])),
        (0x16, linetype(v, 0x16, 0x5, "Continuous", "", &[])),
        (STANDARD, style(v, STANDARD, 0x3, "Standard", "txt")),
        (DIMSTYLE, dimstyle(v, DIMSTYLE, 0xa, "Standard", 2.5)),
        (MODEL, block_header(v, MODEL, "*Model_Space", false, 0x20, 0x21, model_entities, 0x22)),
        (0x20, block_entity(v, 0x20, MODEL, "*Model_Space")),
        (0x21, endblk(v, 0x21, MODEL)),
        (PAPER, block_header(v, PAPER, "*Paper_Space", false, 0x1c, 0x1d, &[], 0)),
        (0x1c, block_entity(v, 0x1c, PAPER, "*Paper_Space")),
        (0x1d, endblk(v, 0x1d, PAPER)),
        (0x22, layout(v, 0x22, 0xd, "Model", 0, MODEL, [420.0, 297.0])),
    ];
    objects.extend(extra);
    Drawing { version, codepage: 30, header: header(4, 0), classes: Vec::new(), objects, measurement: 1 }.bytes()
}

/// R2013 以降は、当てはめ点で作ったスプラインを当てはめ点と端の接線だけで保存し、AutoCAD は DXF に制御点を書く。
/// 2 点なら制御点は端の点と、接線の 1/3 だけ進んだ・戻った点（ACadSharp の見本の DWG と DXF で確かめた形）
#[test]
fn a_fit_point_spline_gets_the_control_points_autocad_writes() {
    let tangents = [[3.0, 6.0, 0.0], [6.0, -3.0, 0.0]];
    let read = |fit: &[[f64; 3]]| {
        let spline = fit_spline(Version::R2018, 0x40, &place(LAYER_0, 256, None), fit, tangents);
        let doc = zumen_dwg::read_dwg(&small(Version::R2018, vec![(0x40, spline)], &[0x40], &[])).unwrap();
        let DxfEntityData::Spline(spline) = &doc.entities[0].data else { panic!("SPLINE") };
        spline.clone()
    };
    let spline = read(&[[0.0, 0.0, 0.0], [9.0, 3.0, 0.0]]);
    assert_eq!(spline.knots, [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    assert_eq!(spline.control_points, [[0.0, 0.0, 0.0], [1.0, 2.0, 0.0], [7.0, 4.0, 0.0], [9.0, 3.0, 0.0]]);
    // 3 点以上は内側の点を通る 3 次の補間。節点は弦の長さの比、制御点は点の数 + 2
    let spline = read(&[[0.0, 0.0, 0.0], [5.0, 5.0, 0.0], [10.0, 0.0, 0.0]]);
    assert_eq!(spline.knots, [0.0, 0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 1.0]);
    assert_eq!(spline.control_points.len(), 5);
}

/// 注釈を持つ LEADER の終わりの横線（フック）は DWG に頂点がない。DXF と同じく、最後の頂点から横の向きの反対に
/// 矢印の大きさ × DIMSCALE だけ戻った点を足す
#[test]
fn a_leader_with_an_annotation_gets_its_hook_point() {
    for version in [Version::R2000, Version::R2018] {
        let points = [[0.0, 0.0, 0.0], [10.0, 10.0, 0.0]];
        let extra = vec![
            (0x40, leader(version, 0x40, &place(LAYER_0, 256, None), &points, 0x41, DIMSTYLE)),
            (0x41, mtext(version, 0x41, &place(LAYER_0, 256, None), "注記", [10.5, 10.0, 0.0], 2.5, STANDARD)),
        ];
        let doc = zumen_dwg::read_dwg(&small(version, extra, &[0x40, 0x41], &[])).unwrap();
        let DxfEntityData::Leader(leader) = &doc.entities[0].data else { panic!("LEADER") };
        assert_eq!(leader.points, [[0.0, 0.0, 0.0], [7.5, 10.0, 0.0], [10.0, 10.0, 0.0]], "{version:?}");
    }
}

/// 無名のブロックの名前は BLOCK の名前（*D7 など）。R2000 に保存した動的ブロックは BLOCK が元の名前を持つので、
/// BLOCK_HEADER の *U と表の中の順の番号にする（AutoCAD の DXF の *U17 と同じ）
#[test]
fn anonymous_blocks_are_named_like_autocad_names_them_in_dxf() {
    let v = Version::R2000;
    let extra = vec![
        (0x30, block_header(v, 0x30, "*D", true, 0x31, 0x32, &[], 0)),
        (0x31, block_entity(v, 0x31, 0x30, "*D7")),
        (0x32, endblk(v, 0x32, 0x30)),
        (0x33, block_header(v, 0x33, "*U", true, 0x34, 0x35, &[], 0)),
        (0x34, block_entity(v, 0x34, 0x33, "my-dynamic-block")),
        (0x35, endblk(v, 0x35, 0x33)),
    ];
    let doc = zumen_dwg::read_dwg(&small(v, extra, &[], &[0x30, 0x33])).unwrap();
    let names: Vec<&str> = doc.blocks.iter().map(|block| block.name.as_str()).collect();
    assert_eq!(names, ["*Model_Space", "*Paper_Space", "*D7", "*U1"]);
}

/// 壊れた・途中で切れた DWG でも panic せず、すぐに読み終える（読めないものは notices か Err にする）
#[test]
fn damaged_drawings_finish_without_panicking() {
    let mut seed: u64 = 0x5eed;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    for version in [Version::R2000, Version::R2018] {
        let original = drawing(version);
        for round in 0..400 {
            let mut bytes = original.clone();
            match round % 4 {
                0 => bytes.truncate(random(bytes.len())),
                1 => {
                    for _ in 0..1 + random(8) {
                        let at = random(bytes.len());
                        bytes[at] = random(256) as u8;
                    }
                }
                2 => {
                    let at = random(bytes.len());
                    let length = random(64).min(bytes.len() - at);
                    bytes.drain(at..at + length);
                }
                _ => {
                    let at = random(bytes.len());
                    let length = random(16).min(bytes.len() - at);
                    let copy: Vec<u8> = bytes[at..at + length].to_vec();
                    bytes.splice(at..at, copy.iter().copied().cycle().take(length * random(4)));
                }
            }
            let start = std::time::Instant::now();
            if let Ok(doc) = zumen_dwg::read_dwg(&bytes) {
                for view in [None, Some("Layout1")] {
                    dxf_to_scene(&doc, view);
                }
            }
            assert!(start.elapsed().as_secs() < 5, "{version:?} round {round} took too long");
        }
    }
}

/// tests/fixtures/dwg の図面は、このテストの図面（R2000 は ANSI_932、R2018 は UTF-16）。@zumenjs/dwg のテストが
/// WebAssembly で読んで正解の Scene を書き（pnpm run golden）、crates/zumen/tests/golden.rs が同じ Scene になることを
/// 確かめる。テストの図面を変えたら UPDATE_GOLDEN=1 cargo test -p zumen-dwg で書き直してから pnpm run golden を走らせる
#[test]
fn the_fixtures_are_the_test_drawing() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dwg");
    for (name, version) in [("r2000.dwg", Version::R2000), ("r2018.dwg", Version::R2018)] {
        let bytes = drawing(version);
        let path = fixtures.join(name);
        if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
            std::fs::create_dir_all(&fixtures).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        } else {
            assert!(
                std::fs::read(&path).is_ok_and(|written| written == bytes),
                "tests/fixtures/dwg/{name} is not the test drawing: run UPDATE_GOLDEN=1 cargo test -p zumen-dwg"
            );
        }
    }
}
