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

/// AutoCAD の図面のモデルを SFC に書いて読み直した表示と、SXF のインスタンス番号 → 元の表示の id の部分、写して書いた
/// ときの知らせ
fn to_sxf(doc: &DxfDocument) -> (Scene, HashMap<String, String>, Vec<zumen::Notice>) {
    let mapped = zumen::mapping::dxf_to_sxf(doc);
    let (written, sources) = mapped.write();
    let again = zumen::sxf::parse_sfc(&written.bytes).expect("書いた SFC を読める");
    assert!(again.notices.is_empty(), "{:?}", again.notices);
    (zumen::sxf::sxf_to_scene(&again), sources, [mapped.notices, written.notices].concat())
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

/// SXF・JWW を DXF を経て SXF に書き直し、元と比べる。SFC は CP932 にない文字を ? にして encoding で知らせ、制御文字は
/// 書かないので、元の文字列もそうしてから比べ、? にした文字があれば知らせたかを確かめる
fn check_round_trip(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let mut original = zumen::read(bytes, &ReadDrawingOptions::default()).unwrap();
    let mut replaced = std::collections::HashSet::new();
    let mut unmapped = false;
    for entity in &mut original.entities {
        for primitive in &mut entity.primitives {
            if let zumen::Primitive::Text(text) = primitive
                && unwritable(&text.text)
            {
                unmapped |= text.text.chars().any(|c| !c.is_control() && unwritable(&c.to_string()));
                text.text = cp932(&text.text);
                replaced.insert(entity.id.clone());
            }
        }
    }
    let (doc, dxf_sources) = through_dxf(bytes);
    let (mut again, sxf_sources, notices) = to_sxf(&doc);
    let sources = compose(&sxf_sources, &dxf_sources);
    // ? は半角なので、全角とする文字（Ø など）を ? にした文字列は、字の並びが変わる。その図形の文字は 1 文字ずつの
    // 位置を比べない
    let written = again
        .entities
        .iter_mut()
        .filter(|entity| common::source_id(&entity.id, &sources).is_some_and(|id| replaced.contains(&id)));
    for entity in original.entities.iter_mut().filter(|entity| replaced.contains(&entity.id)).chain(written) {
        for primitive in &mut entity.primitives {
            if let zumen::Primitive::Text(text) = primitive {
                text.cells.clear();
            }
        }
    }
    let mut differences = common::differences(&original, &again, &sources);
    if unmapped && !codes(&notices).contains(&"encoding") {
        differences.push("characters outside CP932 written as ? without the encoding notice".into());
    }
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

/// CP932 にない文字（Ø）を含む JWW の文字は、DXF を経た SFC に ? にして書き、encoding で知らせる。その文字と
/// 1 文字ずつの位置のほかは、元と同じに描ける
#[test]
fn writes_jww_text_outside_cp932_with_question_marks() {
    let operations = zumen::edit::parse_operations(
        r#"[{"op": "add", "kind": "text", "position": [0, 0], "text": "Ø45,6", "height": 2.5}]"#,
    )
    .unwrap();
    let bytes = zumen::create("jww", "mm", &operations).unwrap().bytes;
    let scene = zumen::read(&bytes, &ReadDrawingOptions::default()).unwrap();
    assert!(matches!(&scene.entities[0].primitives[0], zumen::Primitive::Text(text) if text.text == "Ø45,6"));
    let (entities, differences) = check_round_trip("Ø.jww", &bytes);
    assert_eq!((entities, differences), (1, Vec::<String>::new()));
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

/// CP932 にない文字を ? にした文字列（SFC の書き出しと同じ）
fn cp932(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).map(|c| if unwritable(&c.to_string()) { '?' } else { c }).collect()
}

/// 「box [a, b, c, d] → [e, f, g, h]」の違いのいちばん大きい値
fn box_moved(line: &str) -> Option<f64> {
    let numbers = |part: &str| -> Option<Vec<f64>> {
        let inner = part.split_once('[')?.1.split_once(']')?.0;
        inner.split(',').map(|value| value.trim().parse().ok()).collect()
    };
    let (before, after) = line.split_once(" → ")?;
    let (a, b) = (numbers(before.rsplit_once("box ")?.1)?, numbers(after)?);
    (a.len() == 4 && b.len() == 4).then(|| a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0, f64::max))
}

/// 線種の点（長さ 0 の線分）は、SXF には 0.25 mm の線分にして書き、線分・空白が 8 を超える線種は初めの 8 だけを書く
/// （zumen-convert の pitch・line_type_code と同じ。既定義線種と同じ名前・10 個の長さの線種は切らないが、
/// 試験の図面にはない）
fn sxf_dashes(dashes: &mut Vec<f64>) {
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
    dashes.truncate(8);
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
    // SXF の文字範囲の高さは TEXT の高さ（decision 0019）
    let (mut original, distorted) = common::scale_texts(&zumen::dxf::dxf_to_scene(doc, None), false);
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
                zumen::Primitive::Clipped(clipped) => {
                    clipped.strokes.iter_mut().for_each(|stroke| sxf_dashes(&mut stroke.dashes));
                }
                _ => {}
            }
        }
    }
    let sizes: HashMap<String, f64> = original
        .entities
        .iter()
        .map(|entity| {
            let b = &entity.bbox;
            (entity.id.clone(), (b.max_x - b.min_x).max(b.max_y - b.min_y))
        })
        .collect();
    // 円・円弧の半径（表示の点列の始め・中・終わりを通る円）。弦の誤差の許容に使う
    let radii: HashMap<String, f64> = original
        .entities
        .iter()
        .filter_map(|entity| {
            let zumen::Primitive::Stroke(stroke) = entity.primitives.first()? else { return None };
            let n = stroke.points.len() / 2;
            if n < 3 {
                return Some((entity.id.clone(), 0.0));
            }
            let point = |i: usize| (stroke.points[i * 2], stroke.points[i * 2 + 1]);
            let ((ax, ay), (bx, by), (cx, cy)) = (point(0), point(n / 2), point(n - 1));
            let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
            let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
            let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
            let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
            let radius = (ax - ux).hypot(ay - uy);
            Some((entity.id.clone(), if radius.is_finite() { radius } else { 0.0 }))
        })
        .collect();
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
        kinds.dedup_by(|a, b| a == b && (*a == "fill" || *a == "clipped"));
        kinds
    };
    let differences = common::differences(&original, &again, &sources)
        .into_iter()
        .filter(|line| {
            let id = line.split(' ').next().unwrap_or("");
            if skip.contains(id) || common::is_text_place(line, &distorted) {
                return false;
            }
            // 自分を配置するブロック（表示は 32 段まで描く）は、SXF では循環を断って 1 段だけ書く（block-depth）
            if id.split(['>', ':']).any(|part| recursive.contains(part)) {
                return false;
            }
            // DEFPOINTS の寸法の定義点は、表示では寸法の図形にまとめ、SXF では DEFPOINTS（非表示）のレイヤに書く
            if line.to_uppercase().contains("LAYER DEFPOINTS NOT IN") {
                return false;
            }
            // 円・円弧・楕円は同じ形で書く。表示が折線にする細かさ（弦の誤差は 0.02 mm か半径の 1/10,000 の大きい方）
            // が DXF と SXF で違うので、その 2 倍まで
            if let Some(moved) = line.split("stroke moved by ").nth(1).and_then(|rest| rest.split(' ').next())
                && ["(円)", "(円弧)", "(楕円)", "(楕円弧)"].iter().any(|kind| line.contains(kind))
                && let (Some(radius), Some(size)) = (radii.get(id), sizes.get(id))
                && moved.parse::<f64>().is_ok_and(|moved| {
                    // 点が 2 つ（弦 1 本）の円弧は、弦の誤差が弧の大きさの 1/100 まで
                    moved <= radius * 3e-4 + 0.05 || (*radius == 0.0 && moved <= size * 0.01)
                })
            {
                return false;
            }
            if line.contains(": box [")
                && ["(円)", "(円弧)", "(楕円)", "(楕円弧)", "(スプライン)"].iter().any(|kind| line.contains(kind))
                && let Some(size) = sizes.get(id)
                && box_moved(line).is_some_and(|moved| moved <= size * 0.01 + 0.05)
            {
                return false;
            }
            // CP932 にないレイヤ名の文字は ? にして書く
            if let Some(name) = line.split("): layer ").nth(1).and_then(|rest| rest.split(" not in ").next())
                && original.layers.iter().any(|layer| cp932(&layer.name) == name)
            {
                return false;
            }
            // スプラインは、SXF には同じ曲線の 3 次ベジェ曲線で書く（shapes.rs の単体テストで、分けたベジェ曲線が
            // B スプラインと同じ点を通ることを確かめている）。DXF の表示は媒介変数を曲線全体で等分した点
            // （16 × 制御点の数）を結ぶので、節点の間隔が不揃いな曲線では短い区間の点が足りない。大きさの 5/100 まで
            if let Some(moved) = line.split("stroke moved by ").nth(1).and_then(|rest| rest.split(' ').next())
                && line.contains("(スプライン)")
                && let Some(size) = sizes.get(id)
                && moved.parse::<f64>().is_ok_and(|moved| moved <= size * 0.05)
            {
                return false;
            }
            // 座標の大きい図面の模様の線の位置（10 万 mm を超える座標の桁落ち。間隔の 1/10,000 まで）
            if let Some((before, after)) = line.split("hatch lines ").nth(1).and_then(|rest| rest.split_once(" → ")) {
                let parse = |text: &str| -> Vec<f64> {
                    text.trim_end_matches('"').split('/').filter_map(|value| value.parse().ok()).collect()
                };
                let (a, b) = (parse(before), parse(after));
                if a.len() == 3 && b.len() == 3 && a[0] == b[0] && a[1] == b[1] && (a[2] - b[2]).abs() <= a[1] * 1e-4 {
                    return false;
                }
            }
            // 境界にちょうど重なるハッチング線（線の数の数え方の端数で、引く・引かないが分かれる）
            if line.contains("hatch lines") && line.contains("/0.000000/") {
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

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの SFC・P21・JWW・DXF・DWG）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen --release --test convert_to_sxf -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn samples_written_as_sxf_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in files(Path::new(&folder), &["sfc", "p21", "jww", "dxf", "dwg"]) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&file).unwrap();
        let (entities, differences) = match zumen::detect(&bytes) {
            Some("dxf") => check_dxf(&name, &zumen::dxf::parse_dxf(&bytes)),
            Some("dwg") => match zumen::dwg::read_dwg(&bytes) {
                Ok(doc) => check_dxf(&name, &doc),
                Err(error) => {
                    println!("{name}: not read ({error})");
                    continue;
                }
            },
            Some(_) => check_round_trip(&name, &bytes),
            None => continue,
        };
        println!("{name}: {entities} entities, {} differences", differences.len());
        let shown = std::env::var("ZUMEN_DIFFERENCES").ok().and_then(|value| value.parse().ok()).unwrap_or(10);
        failures.extend(differences.into_iter().take(shown));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// --- 写し方の規則（小さな図面） ----------------------------------------------------------------------------

/// グループコードと値の組を、テキストの DXF の行にする
fn tags(pairs: &[(i32, &str)]) -> String {
    pairs.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect()
}

/// 表とブロックと図形だけの DXF（レイアウトのない図面はモデル空間を開く）
fn dxf(tables: &str, blocks: &str, entities: &str) -> DxfDocument {
    let text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n{tables}0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n"
    );
    zumen::dxf::parse_dxf(text.as_bytes())
}

fn written_sxf(doc: &DxfDocument) -> (zumen::sxf::SxfDrawing, zumen::mapping::MappedSxf, Vec<u8>) {
    let mapped = zumen::mapping::dxf_to_sxf(doc);
    let (written, _) = mapped.write();
    (zumen::sxf::parse_sfc(&written.bytes).unwrap(), mapped, written.bytes)
}

fn codes(notices: &[zumen::Notice]) -> Vec<&str> {
    notices.iter().map(|notice| notice.code.as_str()).collect()
}

#[test]
fn writes_model_space_as_a_one_to_one_sub_figure_on_a_sheet_of_its_size() {
    let doc = dxf(
        "",
        "",
        &tags(&[(0, "LINE"), (5, "A1"), (8, "WALL"), (62, "1"), (10, "100"), (20, "100"), (11, "300"), (21, "200")]),
    );
    let (drawing, mapped, _) = written_sxf(&doc);
    let document = &drawing.document;
    // 用紙は図形の範囲の大きさ（自由）
    let sheet = document.sheet.as_ref().unwrap();
    assert_eq!((sheet.sheet_type, sheet.x, sheet.y), (9.0, 200.0, 100.0));
    // モデル空間は 1:1 の部分図。図形は実寸のまま、配置で用紙の原点へ寄せる
    let figure = document.sub_figures.values().next().unwrap();
    assert_eq!((figure.feature.name.as_str(), figure.feature.flag), ("モデル空間", 1.0));
    let zumen::sxf::SxfFeature::Line { style, x1, y1, .. } = &figure.members[0] else { panic!("line") };
    assert_eq!((*x1, *y1), (100.0, 100.0));
    assert_eq!(style.color, 2.0, "ACI 1 は既定義色の赤");
    assert_eq!(document.layers[style.layer as usize - 1].name, "WALL");
    let zumen::sxf::SxfFeature::SfigLocate { x, y, ratio_x, ratio_y, layer, .. } = &document.sheet_members[0] else {
        panic!("locate")
    };
    assert_eq!((*x, *y, *ratio_x, *ratio_y, *layer), (-100.0, -100.0, 1.0, 1.0, 0.0));
    assert!(codes(&mapped.notices).contains(&"model-sheet"));
}

#[test]
fn writes_blocks_as_drawing_parts_and_a_copy_for_each_inherited_color() {
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
    let (drawing, mapped, _) = written_sxf(&doc);
    let document = &drawing.document;
    let model = document.sub_figures.get("モデル空間").unwrap();
    let locates: Vec<(&str, f64, f64, f64)> = model
        .members
        .iter()
        .filter_map(|member| match member {
            zumen::sxf::SxfFeature::SfigLocate { name, x, angle, ratio_x, .. } => {
                Some((name.as_str(), *x, *angle, *ratio_x))
            }
            _ => None,
        })
        .collect();
    // BYBLOCK の色を赤で継ぐ配置と青で継ぐ配置は、別の作図部品。同じ色で継ぐ配置（回転・2 倍）は同じ作図部品
    assert_eq!(locates, [("MARK", 0.0, 0.0, 1.0), ("MARK (2)", 40.0, 0.0, 1.0), ("MARK", 0.0, 90.0, 2.0)]);
    let part = document.sub_figures.get("MARK").unwrap();
    assert_eq!(part.feature.flag, 4.0, "作図部品");
    let zumen::sxf::SxfFeature::Line { style, x1, x2, .. } = &part.members[0] else { panic!("line") };
    // 作図部品の座標は基点を原点にする。レイヤ 0 の図形は配置のレイヤ、BYBLOCK の色は配置の色
    assert_eq!((*x1, *x2), (0.0, 10.0));
    assert_eq!((style.color, document.layers[style.layer as usize - 1].name.as_str()), (2.0, "SYMBOL"));
    let blue = document.sub_figures.get("MARK (2)").unwrap();
    let zumen::sxf::SxfFeature::Line { style, .. } = &blue.members[0] else { panic!("line") };
    assert_eq!(style.color, 4.0);
    // 元の図形の id: 配置の中の要素は、配置の id から順につなぐ
    let (_, sources) = mapped.write();
    let again = zumen::sxf::sxf_to_scene(&drawing);
    let ids: Vec<String> = again
        .entities
        .iter()
        .map(|entity| {
            let mut id = String::new();
            for part in entity.id.split('>') {
                let source = &sources[part];
                if !id.is_empty() && !id.ends_with(':') && !id.ends_with('>') {
                    id.push('>');
                }
                id.push_str(source);
            }
            id
        })
        .collect();
    assert_eq!(ids, ["I1>B1", "I2>B1", "I3>B1"]);
}

#[test]
fn writes_codes_and_text_in_cp932_and_reports_what_it_replaced() {
    let doc = dxf(
        "",
        "",
        &(tags(&[(0, "TEXT"), (5, "T1"), (8, "0"), (10, "0"), (20, "0"), (40, "2.5"), (1, "直径 Ø50")])
            + &tags(&[
                (0, "LINE"),
                (5, "L1"),
                (8, "0"),
                (420, "1193046"),
                (370, "50"),
                (10, "0"),
                (20, "10"),
                (11, "50"),
                (21, "10"),
            ])),
    );
    let (drawing, mapped, bytes) = written_sxf(&doc);
    let members = &drawing.document.sub_figures.get("モデル空間").unwrap().members;
    let zumen::sxf::SxfFeature::TextString { color, text, .. } = &members[0] else { panic!("text") };
    // ACI 7（前景色）は SXF の黒。CP932 にない文字は ?
    assert_eq!((*color, text.text.as_str()), (1.0, "直径 ?50"));
    let zumen::sxf::SxfFeature::Line { style, .. } = &members[1] else { panic!("line") };
    // トゥルーカラーはユーザ定義色（17 から）、0.5 mm は既定義線幅 5
    assert_eq!((style.color, style.line_width), (17.0, 5.0));
    assert_eq!(drawing.document.user_colors[0].r, 18.0);
    let (written, _) = mapped.write();
    assert!(codes(&written.notices).contains(&"encoding"));
    // SFC のヘッダと、フィーチャの書き方（附属書 SFC 編 1-1）
    let text = encoding_rs::SHIFT_JIS.decode(&bytes).0.into_owned();
    assert!(
        text.starts_with("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');"),
        "{text}"
    );
    assert!(text.contains("'zumen$$3.1'"));
    assert!(text.contains("/*SXF\n#10 = pre_defined_colour_feature(\\'black\\')\nSXF*/\n"), "{text}");
    assert_eq!(drawing.version, "SXF 3.1");
}

/// 線分・空白が 8 を超える線種は、SXF のユーザ定義線種（2・4・6・8 個）に初めの 8 だけを書いて知らせる。読み直すと、
/// その線種で描ける
#[test]
fn writes_line_types_of_more_than_eight_lengths_with_the_first_eight() {
    let tables = tags(&[(0, "TABLE"), (2, "LTYPE"), (70, "1")])
        + &tags(&[
            (0, "LTYPE"),
            (2, "STEPS"),
            (70, "0"),
            (3, "Steps"),
            (72, "65"),
            (73, "10"),
            (40, "20"),
            (49, "5"),
            (49, "-1"),
            (49, "4"),
            (49, "-1"),
            (49, "3"),
            (49, "-1"),
            (49, "2"),
            (49, "-1"),
            (49, "1"),
            (49, "-1"),
        ])
        + &tags(&[(0, "ENDTAB")]);
    let line = tags(&[(0, "LINE"), (5, "L1"), (8, "0"), (6, "STEPS"), (10, "0"), (20, "0"), (11, "100"), (21, "0")]);
    let doc = dxf(&tables, "", &line);
    let (drawing, mapped, _) = written_sxf(&doc);
    assert!(codes(&mapped.notices).contains(&"linetype-long"), "{:?}", codes(&mapped.notices));
    let line_type = &drawing.document.user_line_types[0];
    assert_eq!(line_type.name, "STEPS");
    assert_eq!(line_type.pitch, [5.0, 1.0, 4.0, 1.0, 3.0, 1.0, 2.0, 1.0]);
    let (scene, _, _) = to_sxf(&doc);
    let dashes: Vec<&[f64]> = scene
        .entities
        .iter()
        .flat_map(|entity| &entity.primitives)
        .filter_map(|primitive| match primitive {
            zumen::Primitive::Stroke(stroke) => Some(stroke.dashes.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(dashes, [line_type.pitch.as_slice()]);
}

#[test]
fn writes_viewports_as_sub_figures_placed_at_their_scale_and_twist() {
    let doc = zumen::dxf::parse_dxf(&fs::read(fixtures().join("dxf/viewports.dxf")).unwrap());
    let (drawing, _, _) = written_sxf(&doc);
    let document = &drawing.document;
    let sheet = document.sheet.as_ref().unwrap();
    // 用紙は表示と同じ大きさ（このレイアウトは 90° 回して印刷するので A4 縦）
    let shown = zumen::dxf::dxf_to_scene(&doc, None).sheet.unwrap();
    assert_eq!((sheet.sheet_type, sheet.orient, shown.width, shown.height), (4.0, 0.0, 210.0, 297.0));
    let mut scales: Vec<(f64, f64)> = document
        .sheet_members
        .iter()
        .filter_map(|member| match member {
            zumen::sxf::SxfFeature::SfigLocate { name, angle, ratio_x, ratio_y, .. } => {
                assert_eq!(document.sub_figures.get(name).unwrap().feature.flag, 1.0, "部分図");
                // Decomposing a rotated viewport uses hypot: MSVC and other platforms may differ by one ULP.
                assert!((ratio_x - ratio_y).abs() <= ratio_x.abs().max(ratio_y.abs()) * 1e-12);
                Some((*ratio_x, *angle))
            }
            _ => None,
        })
        .collect();
    scales.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut expected: Vec<(f64, f64)> = doc
        .entities
        .iter()
        .filter_map(|entity| match &entity.data {
            zumen::dxf::DxfEntityData::Viewport(viewport)
                // 上から見たビューポート（3D の表示は書かない）
                if viewport.id != 1.0
                    && viewport.status > 0.0
                    && (viewport.status_flags as i32) & 0x20000 == 0
                    && viewport.view_direction[0] == 0.0
                    && viewport.view_direction[1] == 0.0 =>
            {
                // 部分図は実寸の mm（この図面の単位は m）、用紙は mm
                Some((viewport.height / viewport.view_height / zumen::dxf::drawing_unit(&doc), viewport.view_twist_angle))
            }
            _ => None,
        })
        .collect();
    expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(scales.len(), expected.len());
    for ((scale, angle), (expected_scale, expected_angle)) in scales.iter().zip(&expected) {
        assert!(
            (scale - expected_scale).abs() < 1e-15 && (angle - expected_angle).abs() < 1e-9,
            "{scales:?} {expected:?}"
        );
    }
}

#[test]
fn converts_every_format_to_sfc() {
    for path in ["sfc/features.sfc", "p21/features.p21", "jww/minimal-v700.jww", "dxf/features.dxf", "dwg/r2018.dwg"] {
        let bytes = fs::read(fixtures().join(path)).unwrap();
        let converted = zumen::convert(&bytes, "sfc", &ReadDrawingOptions::default()).unwrap();
        assert_eq!(zumen::detect(&converted.bytes), Some("sfc"), "{path}");
        let scene = zumen::read(&converted.bytes, &ReadDrawingOptions::default()).unwrap();
        assert!(!scene.entities.is_empty(), "{path}");
        assert!(
            scene.notices.iter().all(|notice| notice.level != zumen::NoticeLevel::Warning),
            "{path}: {:?}",
            scene.notices
        );
    }
    // 写すときの知らせ（寸法を図形にした、など）を返す
    let bytes = fs::read(fixtures().join("dxf/features.dxf")).unwrap();
    let converted = zumen::convert(&bytes, "sfc", &ReadDrawingOptions::default()).unwrap();
    for code in ["dimension", "drawn-shapes", "image", "text-mirror"] {
        assert!(codes(&converted.notices).contains(&code), "{code} not in {:?}", codes(&converted.notices));
    }
}
