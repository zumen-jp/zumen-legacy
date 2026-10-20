//! AutoCAD の図面のモデルから JWW への写し（dxf_to_jww）: 写した JWW を書いて zumen の JWW の読み取りで読み直すと、
//! 元の図面と同じに描ける（比べ方は common/mod.rs）。
//!
//! - DXF・DWG → JWW: DXF の表示（開いたときのレイアウトかモデル空間）と、書いた JWW の表示
//! - JWW → DXF → JWW: 元の JWW の表示と、DXF を経て書いた JWW の表示。レイヤグループ・レイヤ・縮尺も元と同じ
//! - SXF → DXF → JWW: 元の SXF の表示と、DXF を経て書いた JWW の表示

mod common;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use zumen_convert::{MappedJww, dxf_to_jww};
use zumen_core::{Notice, Primitive, Scene};
use zumen_dxf::{DxfDocument, DxfEntityData, dxf_to_scene, parse_dxf, write_dxf};
use zumen_jww::{JwwDocument, JwwEntity, jww_to_scene, paper_size, read_jww};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn files(folder: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extensions.contains(&extension.to_ascii_lowercase().as_str()))
        })
        .collect();
    files.sort();
    files
}

/// 比べ方の規則。ユーザー定義の線種に入らない線種を近い線種にした図面は、破線を比べない
fn rules(mapped: &MappedJww, width: fn(f64) -> f64) -> common::Rules {
    let any_dashes =
        mapped.notices.iter().any(|notice| matches!(notice.code.as_str(), "linetype-count" | "linetype-long"));
    common::Rules { width, any_dashes }
}

/// AutoCAD の図面のモデルを JWW に書いて読み直した図面と表示
fn to_jww(doc: &DxfDocument) -> (MappedJww, JwwDocument, Scene) {
    let mapped = dxf_to_jww(doc);
    let written = mapped.write();
    let again = read_jww(&written.bytes).expect("書いた JWW を読める");
    assert!(again.truncated.is_none(), "{:?}", again.truncated);
    assert_eq!(again.entities.len(), mapped.document.entities.len());
    let scene = jww_to_scene(&again);
    (mapped, again, scene)
}

/// R12 の図面のようにハンドルのない図形に、zumen-convert と同じハンドルを振る
fn with_handles(doc: &DxfDocument) -> DxfDocument {
    let mut out = doc.clone();
    let mut seen = HashSet::new();
    let mut next = 0;
    let mut assign = |base: &mut zumen_dxf::DxfEntityBase| {
        if base.handle.is_empty() || !seen.insert(base.handle.clone()) {
            next += 1;
            base.handle = format!("ZH{next}");
            seen.insert(base.handle.clone());
        }
    };
    for entity in &mut out.entities {
        assign(&mut entity.base);
        if let DxfEntityData::Insert(insert) = &mut entity.data {
            for attribute in &mut insert.attribs {
                assign(&mut attribute.base);
            }
        }
    }
    out
}

fn translate(scene: &mut Scene, dx: f64, dy: f64) {
    common::transform_scene(scene, &[1.0, 0.0, 0.0, 1.0, dx, dy], 1.0);
}

/// 元の表示（DXF の表示 dxf_view と同じ座標）を、写した JWW の表示の座標に移す。レイアウトは用紙の中心を JWW の
/// 用紙の中心に置き、モデル空間は縮尺 1/S のレイヤグループに、範囲を用紙の中心へ寄せて置く
fn align(original: &mut Scene, dxf_view: &Scene, again: &JwwDocument, scene: &Scene) {
    match (&dxf_view.view, &dxf_view.sheet) {
        (Some(view), Some(sheet)) if view != "Model" => {
            let (_, width, height) = paper_size(again.paper_size).unwrap();
            translate(original, (width - sheet.width) / 2.0, (height - sheet.height) / 2.0);
        }
        _ => {
            let scale = scene.entities.first().and_then(|entity| entity.placement.as_ref()).map_or(1.0, |p| p.scale_x);
            common::transform_scene(original, &[scale, 0.0, 0.0, scale, 0.0, 0.0], scale);
            let (a, b) = (common::entity_bounds(original), common::entity_bounds(scene));
            if a.min_x <= a.max_x && b.min_x <= b.max_x {
                translate(original, b.min_x - a.min_x, b.min_y - a.min_y);
            }
        }
    }
}

/// JWW の表示のレイヤの名前から「0-A 」を除いた名前
fn jww_layer_name(scene: &Scene, id: u32) -> String {
    let name = scene.layers.iter().find(|layer| layer.id == id).map_or("", |layer| layer.name.as_str());
    match name.split_once(' ') {
        Some((code, rest)) if code.len() == 3 && code.as_bytes()[1] == b'-' => rest.to_string(),
        _ => name.to_string(),
    }
}

/// 写した図形のレイヤが、元の図形のレイヤ（配置のレイヤを含む）にあること
fn layer_differences(original: &Scene, again: &Scene, source: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    let names: HashMap<&str, Vec<String>> = original
        .entities
        .iter()
        .map(|entity| {
            let names = entity
                .layers
                .iter()
                .filter_map(|id| original.layers.iter().find(|layer| layer.id == *id))
                .map(|layer| layer.name.clone())
                .collect();
            (entity.id.as_str(), names)
        })
        .collect();
    let mut out = Vec::new();
    for entity in &again.entities {
        let (Some(id), Some(own)) = (source(&entity.id), entity.layers.last()) else { continue };
        let Some(expected) = names.get(id.as_str()) else { continue };
        let name = jww_layer_name(again, *own);
        let found = expected.iter().any(|candidate| {
            *candidate == name || candidate.split_once(' ').is_some_and(|(code, rest)| code.len() == 3 && rest == name)
        });
        if !found {
            out.push(format!("{id}: layer {name} not in {expected:?}"));
        }
    }
    out
}

/// 斜体・裏返しの文字を持つ図形（JWW には立てて・裏返さずに書く）
fn slanted_texts(scene: &Scene) -> HashSet<String> {
    scene
        .entities
        .iter()
        .filter(|entity| {
            entity.primitives.iter().any(|primitive| match primitive {
                Primitive::Text(text) => {
                    let m = &text.matrix;
                    let (x, y) = (m[0].hypot(m[1]), m[2].hypot(m[3]));
                    m[0] * m[3] - m[1] * m[2] < 0.0 || (m[0] * m[2] + m[1] * m[3]).abs() > 1e-9 * x * y
                }
                _ => false,
            })
        })
        .map(|entity| entity.id.clone())
        .collect()
}

/// DXF・DWG を JWW に書き、DXF の表示と比べる。JWW に書けないもの（ビューポートの枠での切り取り、斜体・裏返しの
/// 文字、自分を配置するブロック）による違いは除く
fn check_dxf(name: &str, doc: &DxfDocument) -> (usize, Vec<String>) {
    let doc = &with_handles(doc);
    let mut original = dxf_to_scene(doc, None);
    let (mapped, again, scene) = to_jww(doc);
    let shown = original.clone();
    align(&mut original, &shown, &again, &scene);
    let hidden: HashSet<u32> = original.layers.iter().filter(|layer| !layer.visible).map(|layer| layer.id).collect();
    // 表示しないレイヤのビューポートの枠は書かない
    original
        .entities
        .retain(|entity| entity.source != "VIEWPORT" || !entity.layers.iter().any(|id| hidden.contains(id)));
    let mut skip = slanted_texts(&original);
    let source = |id: &str| common::jww_source(id, &again, &mapped.sources);
    let groups = common::group(&scene, &source);
    for entity in &original.entities {
        let Some(parts) = groups.get(&entity.id) else { continue };
        let mut bbox = zumen_core::geometry::empty_box();
        for part in parts {
            zumen_core::geometry::union_box(&mut bbox, &part.bbox);
        }
        let b = &entity.bbox;
        // ビューポートの枠で切り取った図形は、JWW では切り取らずに書く（viewport-clip）
        if entity.id.contains(':')
            && ((bbox.max_x - bbox.min_x) > (b.max_x - b.min_x) + 0.1
                || (bbox.max_y - bbox.min_y) > (b.max_y - b.min_y) + 0.1)
        {
            skip.insert(entity.id.clone());
        }
    }
    // 自分のブロックを配置するブロック参照
    let recursive: HashSet<&str> = doc
        .entities
        .iter()
        .filter(|entity| match &entity.data {
            DxfEntityData::Insert(insert) => {
                entity.base.block.as_deref().is_some_and(|block| block.eq_ignore_ascii_case(&insert.name))
            }
            _ => false,
        })
        .map(|entity| entity.base.handle.as_str())
        .collect();
    let mut differences = common::differences(&original, &scene, &source, rules(&mapped, common::jww_width));
    differences.extend(layer_differences(&original, &scene, &source));
    let differences = differences
        .into_iter()
        .filter(|line| {
            let id = line.split(' ').next().unwrap_or("").trim_end_matches(':');
            // 寸法の定義点（DEFPOINTS）は、表示では寸法の図形にまとめ、JWW では DEFPOINTS（非表示）のレイヤに書く
            let defpoints = line.to_uppercase().contains(": LAYER DEFPOINTS NOT IN");
            !skip.contains(id) && !defpoints && !id.split(['>', ':']).any(|part| recursive.contains(part))
        })
        .map(|line| format!("{name}: {line}"))
        .collect();
    (original.entities.len(), differences)
}

/// JWW を DXF に写して書き、読み直したモデル（ハンドル → 元の図形の id）
fn jww_through_dxf(doc: &JwwDocument) -> (DxfDocument, HashMap<String, String>) {
    let mapped = zumen_convert::jww_to_dxf(doc);
    (parse_dxf(&write_dxf(&mapped.document).bytes), mapped.sources)
}

/// JWW を DXF を経て JWW に書き直し、元と比べる。レイヤグループ・レイヤと縮尺も元と同じ
fn check_round_trip(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let original_doc = read_jww(bytes).unwrap();
    let original = jww_to_scene(&original_doc);
    let (dxf, dxf_sources) = jww_through_dxf(&original_doc);
    let (mapped, again, scene) = to_jww(&dxf);
    let mut out = Vec::new();
    if again.paper_size != original_doc.paper_size {
        out.push(format!("paper {} → {}", original_doc.paper_size, again.paper_size));
    }
    let source = |id: &str| {
        common::jww_source(id, &again, &mapped.sources).map(|dxf_id| common::through_handles(&dxf_id, &dxf_sources))
    };
    out.extend(common::differences(&original, &scene, &source, rules(&mapped, common::width_through_dxf)));
    // 図形のレイヤ（レイヤグループ × 16 + レイヤ + 1）と縮尺は元と同じ
    let layers: HashMap<&str, (Option<u32>, Option<f64>)> = original
        .entities
        .iter()
        .map(|entity| {
            (entity.id.as_str(), (entity.layers.last().copied(), entity.placement.as_ref().map(|p| p.scale_x)))
        })
        .collect();
    for entity in &scene.entities {
        let Some(id) = source(&entity.id) else { continue };
        let Some((layer, scale)) = layers.get(id.as_str()) else { continue };
        if entity.layers.last() != layer.as_ref() {
            out.push(format!("{id}: layer {:?} → {:?}", layer, entity.layers.last()));
        }
        let after = entity.placement.as_ref().map(|p| p.scale_x);
        if let (Some(before), Some(after)) = (scale, after)
            && (before - after).abs() > 1e-9 * before.abs()
        {
            out.push(format!("{id}: scale {before} → {after}"));
        }
    }
    (original.entities.len(), out.into_iter().map(|line| format!("{name}: {line}")).collect())
}

/// SXF を DXF を経て JWW に書き、SXF の表示と比べる
fn check_sxf(name: &str, bytes: &[u8], format: &str) -> (usize, Vec<String>) {
    let drawing = zumen_sxf::parse_sxf(bytes, format).unwrap();
    let original_scene = zumen_sxf::sxf_to_scene(&drawing);
    let mapped_dxf = zumen_convert::sxf_to_dxf(&drawing);
    let dxf = parse_dxf(&write_dxf(&mapped_dxf.document).bytes);
    let (mapped, again, scene) = to_jww(&dxf);
    let mut original = original_scene;
    // SXF の用紙（用紙のない図面は図形の範囲）は、DXF のレイアウトの用紙
    align(&mut original, &dxf_to_scene(&dxf, None), &again, &scene);
    let skip = slanted_texts(&original);
    let source = |id: &str| {
        common::jww_source(id, &again, &mapped.sources)
            .map(|dxf_id| common::through_handles(&dxf_id, &mapped_dxf.sources))
    };
    let differences = common::differences(&original, &scene, &source, rules(&mapped, common::width_through_dxf))
        .into_iter()
        .filter(|line| !skip.contains(line.split(' ').next().unwrap_or("")))
        .map(|line| format!("{name}: {line}"))
        .collect();
    (original.entities.len(), differences)
}

fn read_dxf_or_dwg(path: &Path) -> Option<DxfDocument> {
    let bytes = fs::read(path).unwrap();
    if zumen_dxf::is_dxf(&bytes) {
        return Some(parse_dxf(&bytes));
    }
    zumen_dwg::read_dwg(&bytes).ok()
}

#[test]
fn dxf_and_dwg_written_as_jww_draw_the_same() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for file in files(&fixtures().join("dxf"), &["dxf"]).into_iter().chain(files(&fixtures().join("dwg"), &["dwg"])) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let doc = read_dxf_or_dwg(&file).expect("読める図面");
        if name.contains("limits") {
            // 上限を確かめる図面（巨大なメッシュ・遠すぎるハッチングの基点）は、書き終えることだけを確かめる
            to_jww(&doc);
            continue;
        }
        let (entities, differences) = check_dxf(&name, &doc);
        checked += entities;
        println!("{name}: {entities} entities, {} differences", differences.len());
        failures.extend(differences.into_iter().take(40));
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 100, "checked only {checked} entities");
}

#[test]
fn jww_written_through_the_autocad_model_draws_the_same() {
    let mut failures = Vec::new();
    for file in files(&fixtures().join("jww"), &["jww"]) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let (entities, differences) = check_round_trip(&name, &fs::read(&file).unwrap());
        println!("{name}: {entities} entities, {} differences", differences.len());
        failures.extend(differences);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn sxf_written_as_jww_through_the_autocad_model_draws_the_same() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for format in ["sfc", "p21"] {
        for file in files(&fixtures().join(format), &[format]) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (entities, differences) = check_sxf(&name, &fs::read(&file).unwrap(), format);
            checked += entities;
            println!("{name}: {entities} entities, {} differences", differences.len());
            failures.extend(differences.into_iter().take(40));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 100, "checked only {checked} entities");
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの DXF・DWG・JWW・SFC・P21）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-convert --release --test to_jww -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn samples_written_as_jww_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let shown = std::env::var("ZUMEN_DIFFERENCES").ok().and_then(|value| value.parse().ok()).unwrap_or(10);
    let mut failures = Vec::new();
    for file in files(Path::new(&folder), &["dxf", "dwg", "jww", "sfc", "p21"]) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&file).unwrap();
        let extension = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let started = std::time::Instant::now();
        let (entities, differences) = match extension.as_str() {
            "jww" => check_round_trip(&name, &bytes),
            "sfc" | "p21" => check_sxf(&name, &bytes, &extension),
            _ => match read_dxf_or_dwg(&file) {
                Some(doc) => check_dxf(&name, &doc),
                None => {
                    println!("{name}: not read");
                    continue;
                }
            },
        };
        println!("{name}: {entities} entities, {} differences, {:?}", differences.len(), started.elapsed());
        failures.extend(differences.into_iter().take(shown));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// --- 写し方の規則（小さな図面） ----------------------------------------------------------------------------

/// グループコードと値の組を、テキストの DXF の行にする
fn tags(pairs: &[(i32, &str)]) -> String {
    pairs.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect()
}

/// ブロックと図形だけの DXF（表とレイアウトのない図面はモデル空間を開く）。単位は mm
fn dxf(tables: &str, blocks: &str, entities: &str) -> DxfDocument {
    let text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n{tables}0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    );
    parse_dxf(text.as_bytes())
}

fn codes(notices: &[Notice]) -> Vec<&str> {
    notices.iter().map(|notice| notice.code.as_str()).collect()
}

fn line(handle: &str, layer: &str, extra: &[(i32, &str)], from: (f64, f64), to: (f64, f64)) -> String {
    let (x1, y1, x2, y2) = (from.0.to_string(), from.1.to_string(), to.0.to_string(), to.1.to_string());
    let mut pairs = vec![(0, "LINE"), (5, handle), (8, layer)];
    pairs.extend_from_slice(extra);
    pairs.extend_from_slice(&[(10, x1.as_str()), (20, y1.as_str()), (11, x2.as_str()), (21, y2.as_str())]);
    tags(&pairs)
}

fn lines_of(doc: &JwwDocument) -> Vec<&zumen_jww::JwwLine> {
    doc.entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Line(line) => Some(line),
            _ => None,
        })
        .collect()
}

#[test]
fn writes_model_space_at_a_scale_that_fits_an_a3_sheet() {
    // 建物の大きさ（20 m × 10 m）は 1/100。原点が用紙に入るので、実寸の座標はそのまま
    let doc = dxf("", "", &line("A1", "WALL", &[(62, "1")], (-10000.0, -5000.0), (10000.0, 5000.0)));
    let (mapped, again, _) = to_jww(&doc);
    assert_eq!(again.paper_size, 3, "A3");
    let written = lines_of(&again)[0];
    assert_eq!((written.x1, written.y1, written.x2, written.y2), (-100.0, -50.0, 100.0, 50.0));
    let group = usize::from(written.base.layer_group);
    assert_eq!(again.groups[group].scale, 100.0);
    assert_eq!(again.groups[group].layers[usize::from(written.base.layer)].name, "WALL");
    // ACI 1（赤）は SXF の既定義色の赤（102）、線の太さの既定 0.25 mm は 25（1/100 mm）
    assert_eq!((written.base.pen_color, written.base.pen_width), (102, 25));
    assert!(again.width_unit.hundredth);
    assert!(codes(&mapped.notices).contains(&"model-sheet"));
    assert!(!codes(&mapped.notices).contains(&"model-moved"));
    assert_eq!(mapped.sources["#1"], "A1");
    // 原点から遠い図形は、範囲の中心（用紙の 10 mm 刻み）を用紙の中心へ寄せる
    let doc = dxf("", "", &line("A1", "0", &[], (100.0, 100.0), (300.0, 200.0)));
    let (mapped, again, _) = to_jww(&doc);
    let written = lines_of(&again)[0];
    assert_eq!(again.groups[usize::from(written.base.layer_group)].scale, 1.0);
    assert_eq!((written.x1, written.y1, written.x2, written.y2), (-100.0, -50.0, 100.0, 50.0));
    assert!(codes(&mapped.notices).contains(&"model-moved"));
}

#[test]
fn writes_blocks_as_jww_blocks_and_a_copy_for_each_inherited_color() {
    let block = tags(&[(0, "BLOCK"), (2, "MARK"), (70, "0"), (10, "5"), (20, "0")])
        + &tags(&[(0, "LINE"), (5, "B1"), (8, "0"), (62, "0"), (10, "5"), (20, "0"), (11, "15"), (21, "0")])
        + &tags(&[(0, "ENDBLK")]);
    let insert = |handle: &str, color: &str, x: &str| {
        tags(&[(0, "INSERT"), (5, handle), (8, "SYMBOL"), (62, color), (2, "MARK"), (10, x), (20, "0")])
    };
    let rotated = tags(&[
        (0, "INSERT"),
        (5, "I3"),
        (8, "SYMBOL"),
        (62, "1"),
        (2, "MARK"),
        (10, "0"),
        (20, "50"),
        (41, "2"),
        (42, "2"),
        (50, "90"),
    ]);
    let doc = dxf("", &block, &(insert("I1", "1", "0") + &insert("I2", "5", "40") + &rotated));
    let (mapped, again, scene) = to_jww(&doc);
    let placements: Vec<(u32, f64, f64, f64)> = again
        .entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Block(block) => {
                Some((block.definition, block.scale_x, block.rotation.to_degrees().round(), block.y))
            }
            _ => None,
        })
        .collect();
    // BYBLOCK の色を赤で継ぐ配置と青で継ぐ配置は別の定義。同じ色で継ぐ配置（回転・2 倍）は同じ定義
    assert_eq!(placements.len(), 3);
    assert_eq!((placements[0].0, placements[1].0, placements[2].0), (1, 2, 1));
    assert_eq!((placements[2].1, placements[2].2), (2.0, 90.0));
    let names: Vec<&str> = again.blocks.iter().map(|block| block.name.as_str()).collect();
    assert_eq!(names, ["MARK", "MARK (2)"]);
    let member = |number: u32| match &again.blocks.iter().find(|block| block.number == number).unwrap().entities[0] {
        JwwEntity::Line(line) => line.clone(),
        other => panic!("{other:?}"),
    };
    // 定義の座標は基点を原点にする。レイヤ 0 の図形は配置のレイヤ、BYBLOCK の色は配置の色（赤 102・青は線色 6）
    let red = member(1);
    assert_eq!((red.x1, red.x2, red.base.pen_color), (0.0, 10.0, 102));
    assert_eq!(member(2).base.pen_color, 6);
    let layer = &again.groups[usize::from(red.base.layer_group)].layers[usize::from(red.base.layer)];
    assert_eq!(layer.name, "SYMBOL");
    // 元の図形の id: 配置の中の図形は、配置の id から順につなぐ
    let ids: Vec<String> =
        scene.entities.iter().map(|entity| common::jww_source(&entity.id, &again, &mapped.sources).unwrap()).collect();
    assert_eq!(ids, ["I1>B1", "I2>B1", "I3>B1"]);
}

#[test]
fn writes_colors_line_types_and_widths_as_jww_pens() {
    let tables = tags(&[(0, "TABLE"), (2, "LTYPE"), (70, "1")])
        + &tags(&[
            (0, "LTYPE"),
            (2, "DASHDOT"),
            (70, "0"),
            (3, "Dash dot"),
            (72, "65"),
            (73, "4"),
            (40, "10"),
            (49, "5"),
            (49, "-2"),
            (49, "0"),
            (49, "-3"),
        ])
        + &tags(&[(0, "ENDTAB")]);
    let entities = line("L1", "0", &[(420, "1193046"), (370, "50")], (0.0, 0.0), (50.0, 0.0))
        + &line("L2", "0", &[(420, "49344")], (0.0, 10.0), (50.0, 10.0))
        + &line("L3", "0", &[(62, "3"), (6, "DASHDOT")], (0.0, 20.0), (50.0, 20.0))
        + &line("L4", "0", &[(420, "1193046")], (0.0, 30.0), (50.0, 30.0));
    let doc = dxf(&tables, "", &entities);
    let (mapped, again, _) = to_jww(&doc);
    let lines = lines_of(&again);
    // トゥルーカラーは SXF 対応色のユーザー定義（117 から）、0.5 mm は 50（1/100 mm）
    assert_eq!((lines[0].base.pen_color, lines[0].base.pen_width), (117, 50));
    assert_eq!(again.extended_colors.as_ref().unwrap()[17], "#123456");
    // Jw_cad の線色 1（#00c0c0）と同じ色は線色 1、同じ色は同じ線色
    assert_eq!(lines[1].base.pen_color, 1);
    assert_eq!(lines[3].base.pen_color, 117);
    // ACI 3（緑）は SXF の既定義色の緑。点を含む線種はユーザー定義の線種（点は 0.25 mm の線分）
    assert_eq!((lines[2].base.pen_color, lines[2].base.pen_style), (103, 47));
    assert_eq!(again.line_types[&47], vec![5.0, 2.0, 0.25, 2.75]);
    assert!(codes(&mapped.notices).contains(&"linetype-dot"));
}

#[test]
fn writes_text_with_the_size_of_one_character_and_fills_as_solids() {
    let entities = tags(&[(0, "TEXT"), (5, "T1"), (8, "0"), (10, "0"), (20, "0"), (40, "5"), (1, "AB 図面")])
        + &tags(&[
            (0, "SOLID"),
            (5, "S1"),
            (8, "0"),
            (62, "5"),
            (10, "0"),
            (20, "20"),
            (11, "10"),
            (21, "20"),
            (12, "0"),
            (22, "30"),
            (13, "10"),
            (23, "30"),
        ])
        + &tags(&[
            (0, "HATCH"),
            (5, "H1"),
            (8, "0"),
            (62, "1"),
            (10, "0"),
            (20, "0"),
            (30, "0"),
            (2, "SOLID"),
            (70, "1"),
            (71, "0"),
            (91, "1"),
            (92, "1"),
            (93, "1"),
            (72, "2"),
            (10, "50"),
            (20, "50"),
            (40, "10"),
            (50, "0"),
            (51, "360"),
            (73, "1"),
            (97, "0"),
            (75, "0"),
            (76, "1"),
            (98, "0"),
        ]);
    let doc = dxf("", "", &entities);
    let (mapped, again, _) = to_jww(&doc);
    let text = again
        .entities
        .iter()
        .find_map(|entity| match entity {
            JwwEntity::Text(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.content, "AB 図面");
    // 文字の幅は、全角 1・半角 0.5 で割り振った 1 字の幅。終点までの長さは文字範囲の幅
    let width = (text.x2 - text.x1).hypot(text.y2 - text.y1);
    assert!((text.size_x * 3.5 - width).abs() < 1e-9, "{text:?}");
    assert_eq!((text.text_type, text.spacing, text.font.as_str()), (0, 0.0, "ＭＳ ゴシック"));
    // SOLID は 1 つのソリッド（1 → 2 → 4 → 3 の順）
    let solids: Vec<&zumen_jww::JwwSolid> = again
        .entities
        .iter()
        .filter_map(|entity| match entity {
            JwwEntity::Solid(solid) => Some(solid),
            _ => None,
        })
        .collect();
    assert_eq!(solids[0].points, [0.0, 20.0, 10.0, 20.0, 10.0, 30.0, 0.0, 30.0]);
    // 青（ACI 5）は Jw_cad の線色 6、赤の塗り（ACI 1）は任意色（線色 10 の COLORREF）
    assert_eq!((solids[0].base.pen_color, solids[0].color), (6, None));
    let circle: Vec<&&zumen_jww::JwwSolid> = solids.iter().skip(1).collect();
    assert!(circle.len() > 4);
    assert!(circle.iter().all(|solid| solid.base.pen_color == 10 && solid.color == Some(0x0000ff)));
    // 円の塗りは、台形のソリッドに分けても面積が同じ
    let area: f64 = circle
        .iter()
        .map(|solid| {
            let p = &solid.points;
            ((p[2] - p[0]) + (p[4] - p[6])) / 2.0 * (p[5] - p[1])
        })
        .sum();
    assert!((area - std::f64::consts::PI * 100.0).abs() < 1.0, "{area}");
    assert!(codes(&mapped.notices).contains(&"fill-solids"));
}

#[test]
fn puts_layers_named_from_jww_back_and_merges_layers_beyond_256() {
    let tables = tags(&[(0, "TABLE"), (2, "LAYER"), (70, "2")])
        + &tags(&[(0, "LAYER"), (2, "3-A 通り芯"), (70, "0"), (62, "7"), (6, "CONTINUOUS")])
        + &tags(&[(0, "LAYER"), (2, "HIDDEN"), (70, "0"), (62, "-7"), (6, "CONTINUOUS")])
        + &tags(&[(0, "ENDTAB")]);
    let mut entities = line("A1", "3-A 通り芯", &[], (0.0, 0.0), (10.0, 0.0));
    entities += &line("A2", "HIDDEN", &[], (0.0, 5.0), (10.0, 5.0));
    for index in 0..300 {
        entities += &line(&format!("L{index}"), &format!("L{index}"), &[], (0.0, 10.0), (10.0, 10.0));
    }
    let doc = dxf(&tables, "", &entities);
    let (mapped, again, _) = to_jww(&doc);
    let lines = lines_of(&again);
    assert_eq!((lines[0].base.layer_group, lines[0].base.layer), (3, 10));
    assert_eq!(again.groups[3].layers[10].name, "通り芯");
    // 非表示のレイヤは状態 0
    let hidden = &again.groups[usize::from(lines[1].base.layer_group)].layers[usize::from(lines[1].base.layer)];
    assert_eq!((hidden.name.as_str(), hidden.state), ("HIDDEN", 0));
    // 256 枚に入らないレイヤの図形は、同じ縮尺の最後のレイヤにまとめる
    assert!(codes(&mapped.notices).contains(&"layer-merged"));
    let used: HashSet<(u16, u16)> = lines.iter().map(|line| (line.base.layer_group, line.base.layer)).collect();
    assert_eq!(used.len(), 256);
}

#[test]
fn writes_viewports_as_layer_groups_of_their_scale_on_the_sheet() {
    let doc = parse_dxf(&fs::read(fixtures().join("dxf/viewports.dxf")).unwrap());
    let (mapped, again, scene) = to_jww(&doc);
    let shown = dxf_to_scene(&doc, None).sheet.unwrap();
    // A4 縦の用紙は JWW にないので、それを含む A3（横）の中心に置く
    assert_eq!((shown.width, shown.height), (210.0, 297.0));
    assert_eq!(again.paper_size, 3);
    assert!(codes(&mapped.notices).contains(&"paper-size"));
    // ビューポートの縮尺（実寸の mm → 用紙の mm）ごとのレイヤグループ
    let mut expected: Vec<f64> = doc
        .entities
        .iter()
        .filter_map(|entity| match &entity.data {
            DxfEntityData::Viewport(viewport)
                if viewport.id != 1.0
                    && viewport.status > 0.0
                    && (viewport.status_flags as i32) & 0x20000 == 0
                    && viewport.view_direction[0] == 0.0
                    && viewport.view_direction[1] == 0.0 =>
            {
                Some(zumen_dxf::drawing_unit(&doc) * viewport.view_height / viewport.height)
            }
            _ => None,
        })
        .collect();
    expected.sort_by(f64::total_cmp);
    expected.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs());
    let mut scales: Vec<f64> = scene
        .entities
        .iter()
        .filter(|entity| {
            entity.id.split('>').next().is_some_and(|top| {
                common::jww_source(top, &again, &mapped.sources).is_some_and(|source| source.contains(':'))
            })
        })
        .filter_map(|entity| entity.placement.as_ref().map(|placement| 1.0 / placement.scale_x))
        .collect();
    scales.sort_by(f64::total_cmp);
    scales.dedup_by(|a, b| (*a - *b).abs() <= 1e-9 * a.abs());
    assert_eq!(scales.len(), expected.len(), "{scales:?} {expected:?}");
    for (a, b) in scales.iter().zip(&expected) {
        assert!((a - b).abs() <= 1e-9 * b, "{scales:?} {expected:?}");
    }
}
