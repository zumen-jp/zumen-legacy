//! DXF・DWG・SXF・JWW から SXF（SFC）への変換: 図面を AutoCAD の図面のモデルから SXF に写して SFC に書き、zumen の SXF の
//! 読み取りで読み直すと、元の図面と同じに描ける（比べ方は common/mod.rs）。
//!
//! - SXF → DXF → SXF: 元の SXF の表示と、DXF を経て書いた SFC の表示
//! - DXF・DWG → SXF: DXF の表示（開いたときのレイアウトかモデル空間）と、書いた SFC の表示

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use zumen::dxf::DxfDocument;
use zumen::{ReadDrawingOptions, Scene};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// AutoCAD の図面のモデルを SFC に書いて読み直した表示と、SXF のインスタンス番号 → 元の表示の id の部分
fn to_sxf(doc: &DxfDocument) -> (Scene, HashMap<String, String>, Vec<zumen::Notice>) {
    let mapped = zumen::mapping::dxf_to_sxf(doc);
    let (written, sources) = mapped.write();
    let again = zumen::sxf::parse_sfc(&written.bytes).expect("書いた SFC を読める");
    assert!(again.notices.is_empty(), "{:?}", again.notices);
    (zumen::sxf::sxf_to_scene(&again), sources, mapped.notices)
}

/// SXF・JWW を DXF に写し、書いた DXF を読み直したモデル。ハンドル → 元の図形の id
fn through_dxf(bytes: &[u8]) -> (DxfDocument, HashMap<String, String>) {
    let format = zumen::detect(bytes).expect("読める図面");
    let mapped = match format {
        "jww" => zumen::mapping::jww_to_dxf(&zumen::jww::read_jww(bytes).unwrap()),
        _ => zumen::mapping::sxf_to_dxf(&zumen::sxf::parse_sxf(bytes, format).unwrap()),
    };
    let written = zumen::dxf::write_dxf(&mapped.document);
    (zumen::dxf::parse_dxf(&written.bytes), mapped.sources)
}

/// SXF の元の id の部分を、DXF を経る前の図形の id にする
fn compose(sxf: &HashMap<String, String>, dxf: &HashMap<String, String>) -> HashMap<String, String> {
    sxf.iter()
        .map(|(id, part)| {
            let handles = part.split([':', '>']).filter(|handle| !handle.is_empty());
            let original: Vec<&str> = handles
                .map(|handle| dxf.get(handle.split('[').next().unwrap_or(handle)).map_or("?", String::as_str))
                .filter(|source| !source.is_empty())
                .collect();
            (id.clone(), original.join(">"))
        })
        .collect()
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

/// SXF・JWW を DXF を経て SXF に書き直し、元と比べる
fn check_round_trip(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let original = zumen::read(bytes, &ReadDrawingOptions::default()).unwrap();
    let (doc, dxf_sources) = through_dxf(bytes);
    let (again, sxf_sources, _) = to_sxf(&doc);
    let sources = compose(&sxf_sources, &dxf_sources);
    let differences = common::differences(&original, &again, &sources);
    (original.entities.len(), differences.into_iter().map(|line| format!("{name}: {line}")).collect())
}

#[test]
fn sxf_written_through_the_autocad_model_draws_the_same() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for format in ["sfc", "p21", "jww"] {
        for file in files(&fixtures().join(format), &[format]) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (entities, differences) = check_round_trip(&name, &fs::read(&file).unwrap());
            checked += entities;
            failures.extend(differences);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked >= 100, "checked only {checked} entities");
}

/// 表示を平行移動する（モデル空間を表示する図面は、図形の範囲の左下を用紙の原点にして書く）
fn translate(scene: &mut Scene, dx: f64, dy: f64) {
    let shift = |points: &mut Vec<f64>| {
        for (index, value) in points.iter_mut().enumerate() {
            *value += if index % 2 == 0 { dx } else { dy };
        }
    };
    for entity in &mut scene.entities {
        entity.bbox = zumen::BBox {
            min_x: entity.bbox.min_x + dx,
            min_y: entity.bbox.min_y + dy,
            max_x: entity.bbox.max_x + dx,
            max_y: entity.bbox.max_y + dy,
        };
        for primitive in &mut entity.primitives {
            match primitive {
                zumen::Primitive::Stroke(stroke) => shift(&mut stroke.points),
                zumen::Primitive::Fill(fill) => fill.rings.iter_mut().for_each(shift),
                zumen::Primitive::Clipped(clipped) => {
                    clipped.rings.iter_mut().for_each(shift);
                    clipped.strokes.iter_mut().for_each(|stroke| shift(&mut stroke.points));
                }
                zumen::Primitive::Dot(dot) => {
                    dot.x += dx;
                    dot.y += dy;
                }
                zumen::Primitive::Text(text) => {
                    text.matrix[4] += dx;
                    text.matrix[5] += dy;
                }
            }
        }
    }
}

/// SFC に書けない文字（CP932 にない文字、制御文字）を含む文字か
fn unwritable(text: &str) -> bool {
    text.chars().any(|c| {
        let mut buffer = [0u8; 4];
        c.is_control() || encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buffer)).2
    })
}

/// 線種の点（長さ 0 の線分）は、SXF には 0.25 mm の線分にして書く（zumen-convert の pitch と同じ）
fn sxf_dashes(dashes: &mut [f64]) {
    for i in (0..dashes.len()).step_by(2) {
        if dashes[i] <= 0.0 {
            dashes[i] = 0.25;
            if let Some(gap) = dashes.get_mut(i + 1)
                && *gap > 0.5
            {
                *gap -= 0.25;
            }
        }
    }
}

/// R12 の図面のようにハンドルのない図形に、zumen-convert と同じハンドルを振る
fn with_handles(doc: &DxfDocument) -> DxfDocument {
    let mut out = doc.clone();
    let mut seen = std::collections::HashSet::new();
    let mut next = 0;
    let mut assign = |base: &mut zumen::dxf::DxfEntityBase| {
        if base.handle.is_empty() || !seen.insert(base.handle.clone()) {
            next += 1;
            base.handle = format!("ZH{next}");
            seen.insert(base.handle.clone());
        }
    };
    for entity in &mut out.entities {
        assign(&mut entity.base);
        if let zumen::dxf::DxfEntityData::Insert(insert) = &mut entity.data {
            for attribute in &mut insert.attribs {
                assign(&mut attribute.base);
            }
        }
    }
    out
}

/// DXF・DWG を SXF に書き、DXF の表示と比べる。SXF に書けないもの（CP932 にない文字、ビューポートの枠での
/// 切り取り）による違いと、1 つの線を折線と円弧に分けたことによる線の数の違いは除く
fn check_dxf(name: &str, doc: &DxfDocument) -> (usize, Vec<String>) {
    let doc = &with_handles(doc);
    let mut original = zumen::dxf::dxf_to_scene(doc, None);
    let (again, sources, _) = to_sxf(doc);
    if original.view.as_deref() == Some("Model") {
        let (dx, dy) = (-original.bounds.min_x, -original.bounds.min_y);
        translate(&mut original, dx, dy);
    }
    let hidden: std::collections::HashSet<u32> =
        original.layers.iter().filter(|layer| !layer.visible).map(|layer| layer.id).collect();
    // 表示しないレイヤのビューポートの枠は書かない
    original
        .entities
        .retain(|entity| entity.source != "VIEWPORT" || !entity.layers.iter().any(|id| hidden.contains(id)));
    let mut skip = std::collections::HashSet::new();
    for entity in &mut original.entities {
        for primitive in &mut entity.primitives {
            match primitive {
                zumen::Primitive::Text(text)
                    if unwritable(&text.text)
                        || text.matrix[0] * text.matrix[3] - text.matrix[1] * text.matrix[2] < 0.0 =>
                {
                    // CP932 にない文字と、裏返しの文字（text-mirror）は SXF に同じ形で書けない
                    skip.insert(entity.id.clone());
                }
                zumen::Primitive::Stroke(stroke) => sxf_dashes(&mut stroke.dashes),
                _ => {}
            }
        }
    }
    let groups = common::group(&again, &sources);
    for entity in &original.entities {
        let Some(parts) = groups.get(&entity.id) else { continue };
        let mut bbox = zumen::geometry::empty_box();
        for part in parts {
            zumen::geometry::union_box(&mut bbox, &part.bbox);
        }
        let b = &entity.bbox;
        let contains = bbox.min_x <= b.min_x + 1e-6
            && bbox.min_y <= b.min_y + 1e-6
            && bbox.max_x + 1e-6 >= b.max_x
            && bbox.max_y + 1e-6 >= b.max_y;
        if entity.id.contains(':') && contains {
            // ビューポートの枠で切り取った図形は、SXF の部分図では切り取らずに書く（viewport-clip）
            let clipped = (bbox.max_x - bbox.min_x) > (b.max_x - b.min_x) + 0.1
                || (bbox.max_y - bbox.min_y) > (b.max_y - b.min_y) + 0.1;
            if clipped {
                skip.insert(entity.id.clone());
            }
        }
    }
    // 自分のブロックを配置するブロック参照
    let recursive: std::collections::HashSet<&str> = doc
        .entities
        .iter()
        .filter(|entity| match &entity.data {
            zumen::dxf::DxfEntityData::Insert(insert) => {
                entity.base.block.as_deref().is_some_and(|block| block.eq_ignore_ascii_case(&insert.name))
            }
            _ => false,
        })
        .map(|entity| entity.base.handle.as_str())
        .collect();
    let strokes_only = |list: &str| list.split(',').all(|kind| kind == "stroke");
    // 塗りの輪郭は、重ならないものを別の塗りにする（幅のあるポリラインの区間ごとの塗り）
    let collapsed = |list: &str| -> Vec<String> {
        let mut kinds: Vec<String> = list.split(',').map(str::to_string).collect();
        kinds.dedup_by(|a, b| a == b && *a == "fill");
        kinds
    };
    let differences = common::differences(&original, &again, &sources)
        .into_iter()
        .filter(|line| {
            let id = line.split(' ').next().unwrap_or("");
            if skip.contains(id) {
                return false;
            }
            // 自分を配置するブロック（表示は 32 段まで描く）は、SXF では循環を断って 1 段だけ書く（block-depth）
            if id.split(['>', ':']).any(|part| recursive.contains(part)) {
                return false;
            }
            // DEFPOINTS の寸法の定義点は、表示では寸法の図形にまとめ、SXF では DEFPOINTS（非表示）のレイヤに書く
            if line.contains("layer DEFPOINTS not in") {
                return false;
            }
            // 折線を折線と円弧に分けた（線の数の違い。形は外接矩形と、ほかの図形で確かめる）
            if let Some((before, after)) = line.split_once("): [").and_then(|(_, rest)| rest.split_once("] → ["))
                && ((strokes_only(before) && strokes_only(after.trim_end_matches(']')))
                    || collapsed(before) == collapsed(after.trim_end_matches(']')))
            {
                return false;
            }
            true
        })
        .collect::<Vec<_>>();
    (original.entities.len(), differences.into_iter().map(|line| format!("{name}: {line}")).collect())
}

#[test]
fn dxf_and_dwg_written_as_sxf_draw_the_same() {
    let mut failures = Vec::new();
    for file in files(&fixtures().join("dxf"), &["dxf"]).into_iter().chain(files(&fixtures().join("dwg"), &["dwg"])) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&file).unwrap();
        let doc =
            if name.ends_with(".dwg") { zumen::dwg::read_dwg(&bytes).unwrap() } else { zumen::dxf::parse_dxf(&bytes) };
        if name.contains("limits") {
            // 上限を確かめる図面（巨大なメッシュ・遠すぎるハッチングの基点）は、書き終えることだけを確かめる
            to_sxf(&doc);
            continue;
        }
        let (entities, differences) = check_dxf(&name, &doc);
        println!("{name}: {entities} entities, {} differences", differences.len());
        failures.extend(differences.into_iter().take(40));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
