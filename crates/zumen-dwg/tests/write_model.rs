//! モデルからの DWG の書き出し: tests/fixtures の DXF・DWG を AutoCAD 2013 の DWG に書いて読み直すと、すべての表示で
//! 元と同じに描ける（crates/zumen/tests/convert.rs の DXF の書き出しと同じ確かめ方）。
//! 比べるのは図形ごとの種類・レイヤの名前・外接矩形・描く要素（数は 1e-9 の相対誤差まで同じとみなす）。
//! 書けない図形（画像の定義が分からない IMAGE）は省き、形を変えて書く図形（HELIX・MPOLYGON・ブロック参照に属さない ATTRIB）は種類を
//! 比べない（どちらも notices で知らせる）。
//!
//! 元の表示は、DWG を読むと変わる値を DWG の読み取りと同じ形にしたモデルから作る:
//! - AutoCAD 2013 の DWG は複数行の属性を持てない（埋め込みの MTEXT は R2018 から）ので、1 行の属性にする
//!   （書き出しが notices で知らせる）
//! - DWG の文字列は文字のまま持つので、読み取りはキャレット表記の正しい形（制御文字は ^ と文字、^ は「^ 」）にする
//! - WIPEOUT の多角形の境界は、読み取りが DXF と同じく最初の点で閉じる
//!
//! 当てはめ点だけのスプラインは、DWG の読み取りが AutoCAD と同じく制御点を求めて描く（DXF の読み取りは点を通る曲線で
//! 近似する）ので、比べない

use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use zumen_core::Scene;
use zumen_dwg::DwgVersion;
use zumen_dxf::{DxfDocument, DxfEntity, DxfEntityData, DxfSpace, DxfVersion, dxf_to_scene, parse_dxf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn model(bytes: &[u8]) -> Option<DxfDocument> {
    if zumen_dwg::is_dwg(bytes) { zumen_dwg::read_dwg(bytes).ok() } else { Some(parse_dxf(bytes)) }
}

fn close(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0)
        }
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| close(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(key, value)| y.get(key).is_some_and(|other| close(value, other)))
        }
        _ => a == b,
    }
}

/// 比べる値。形を変えて書く図形（changed）は種類を比べない
fn shape(scene: &Scene, entity: &Value, changed: bool) -> Value {
    let layers: Vec<String> = entity["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| {
            let id = id.as_u64().unwrap();
            scene
                .layers
                .iter()
                .find(|layer| u64::from(layer.id) == id)
                .map_or(String::new(), |layer| layer.name.clone())
        })
        .collect();
    json!({
        "type": if changed { Value::Null } else { entity["type"].clone() },
        "layers": layers,
        "bbox": entity["bbox"],
        "primitives": entity["primitives"],
    })
}

/// テキストの DXF のキャレット表記を文字に戻す（書き出しと同じ）
fn uncaret(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek().copied()) {
            ('^', Some(' ')) => {
                out.push('^');
                chars.next();
            }
            ('^', Some(next)) if ('@'..='_').contains(&next) => {
                out.push(char::from(next as u8 - 0x40));
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// 文字列をキャレット表記にする（DWG の読み取りと同じ）
fn caret(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c < ' ' {
            out.push('^');
            out.push(char::from(c as u8 + 0x40));
        } else if c == '^' {
            out.push_str("^ ");
        } else {
            out.push(c);
        }
    }
    out
}

/// DWG を読むと変わる値を、DWG の読み取りと同じ形にしたモデル（AutoCAD 2013 の DWG）
fn expected(doc: &DxfDocument) -> DxfDocument {
    expected_as(doc, DwgVersion::R2013)
}

/// DWG を読むと変わる値を、DWG の読み取りと同じ形にしたモデル。複数行の属性は 2018 より前の版では 1 行にする
fn expected_as(doc: &DxfDocument, version: DwgVersion) -> DxfDocument {
    let mut doc = doc.clone();
    let normal = |text: &mut String| *text = caret(&uncaret(text));
    let flatten = version < DwgVersion::R2018;
    fn entity(item: &mut DxfEntity, normal: &dyn Fn(&mut String), flatten: bool) {
        match &mut item.data {
            DxfEntityData::Text(text) => normal(&mut text.text),
            DxfEntityData::Attrib { text, tag, mtext, .. } => {
                if flatten
                    && let Some(embedded) = mtext.take()
                    && text.text.is_empty()
                {
                    text.text = embedded.content.text.replace(['\n', '\r'], " ");
                }
                normal(&mut text.text);
                normal(tag);
            }
            DxfEntityData::Attdef { text, tag, prompt, mtext, .. } => {
                if flatten
                    && let Some(embedded) = mtext.take()
                    && text.text.is_empty()
                {
                    text.text = embedded.content.text.replace(['\n', '\r'], " ");
                }
                normal(&mut text.text);
                normal(tag);
                normal(prompt);
            }
            DxfEntityData::MText(mtext) => normal(&mut mtext.raw_text),
            DxfEntityData::Dimension(dimension) => normal(&mut dimension.text),
            DxfEntityData::MLeader(mleader) => {
                if let Some(mtext) = &mut mleader.context.mtext {
                    normal(&mut mtext.default_content);
                }
                for attrib in &mut mleader.block_attribs {
                    normal(&mut attrib.text);
                }
            }
            DxfEntityData::Image(image) => {
                if image.boundary.len() > 2 && image.boundary.first() != image.boundary.last() {
                    let first = image.boundary[0];
                    image.boundary.push(first);
                }
            }
            DxfEntityData::Insert(insert) => {
                for attrib in &mut insert.attribs {
                    entity(attrib, normal, flatten);
                }
            }
            _ => {}
        }
    }
    for item in &mut doc.entities {
        entity(item, &normal, flatten);
    }
    doc
}

/// 当てはめ点だけのスプラインのハンドル（比べない）
fn fit_splines(doc: &DxfDocument) -> HashSet<String> {
    doc.entities
        .iter()
        .filter(|entity| {
            matches!(&entity.data, DxfEntityData::Spline(spline) if spline.control_points.len() < 2 && !spline.fit_points.is_empty())
        })
        .map(|entity| entity.base.handle.clone())
        .collect()
}

/// 元の図形ごとに、書き直した図面の同じ図形（同じ id、なければ同じ順の図形）と比べる。画像の中身（underlay）は
/// ファイル・大きさ・置き方・切り抜き・不透明度を順に比べる
fn differences(original: &Scene, again: &Scene, skipped: &HashSet<String>) -> Vec<String> {
    let entities = |scene: &Scene| serde_json::to_value(&scene.entities).unwrap().as_array().unwrap().clone();
    let (before, after) = (entities(original), entities(again));
    let mut out = Vec::new();
    let underlays = |scene: &Scene| {
        let values: Vec<Value> = scene
            .underlays
            .iter()
            .map(|underlay| {
                json!({
                    "file": underlay.file,
                    "size": [underlay.width, underlay.height],
                    "matrix": underlay.matrix,
                    "clip": underlay.clip,
                    "opacity": underlay.opacity,
                })
            })
            .collect();
        Value::Array(values)
    };
    let (images, images_again) = (underlays(original), underlays(again));
    if !close(&images, &images_again) {
        out.push(format!("underlays differ\n  {images}\n  {images_again}"));
    }
    for (index, entity) in before.iter().enumerate() {
        let id = entity["id"].as_str().unwrap();
        let source = entity["source"].as_str().unwrap_or("");
        let same_id = after.iter().find(|other| other["id"] == entity["id"]);
        // 画像のファイル（IMAGEDEF）が分からない画像は書かない
        let file = entity["properties"].as_array().is_some_and(|items| items.iter().any(|item| item["key"] == "file"));
        if source == "IMAGE" && !file {
            if same_id.is_some() {
                out.push(format!("{id}: an image was written"));
            }
            continue;
        }
        let last = id.rsplit('>').next().unwrap_or(id);
        if skipped.contains(last) {
            continue;
        }
        let changed = matches!(source, "HELIX" | "MPOLYGON" | "ATTRIB");
        match same_id.or_else(|| after.get(index)) {
            None => out.push(format!("{id}: missing")),
            Some(other) if !close(&shape(original, entity, changed), &shape(again, other, changed)) => {
                out.push(format!(
                    "{id} ({}): differs\n  {}\n  {}",
                    entity["type"],
                    shape(original, entity, changed),
                    shape(again, other, changed)
                ))
            }
            _ => {}
        }
    }
    out
}

/// 図面を DWG に書き、すべての表示で元と比べる。比べた表示の数と、違いを返す
fn check(name: &str, bytes: &[u8]) -> (usize, Vec<String>) {
    let Some(doc) = model(bytes) else { return (0, Vec::new()) };
    let written = zumen_dwg::write_dwg(&doc);
    let again = match zumen_dwg::read_dwg(&written.bytes) {
        Ok(again) => again,
        Err(error) => return (0, vec![format!("{name}: rereading: {error}")]),
    };
    let skipped = fit_splines(&doc);
    let doc = expected(&doc);
    let original = dxf_to_scene(&doc, None);
    let views: Vec<Option<String>> =
        std::iter::once(None).chain(original.views.iter().flatten().map(|view| Some(view.id.clone()))).collect();
    let mut out = Vec::new();
    for view in &views {
        let before = dxf_to_scene(&doc, view.as_deref());
        let after = dxf_to_scene(&again, view.as_deref());
        out.extend(differences(&before, &after, &skipped).into_iter().map(|line| format!("{name} ({view:?}) {line}")));
    }
    (views.len(), out)
}

/// 色の生の値の BYBLOCK（0xC1000000）
const BY_BLOCK_RAW: f64 = -1_056_964_608.0;

/// 色の生の値（MULTILEADER の 91 など）のトゥルーカラー（0xC2RRGGBB）を、最も近い ACI（0xC30000NN）にする
fn nearest_raw(raw: &mut f64) {
    let value = *raw as i64 as u32;
    if value >> 24 != 0xc2 {
        return;
    }
    let channels = |value: u32| [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff].map(|c| c as i32);
    let target = channels(value);
    let mut best = (7u32, i32::MAX);
    for (index, hex) in zumen_dxf::ACI.iter().enumerate().skip(1) {
        let color = channels(u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap());
        let distance: i32 = (0..3).map(|i| (color[i] - target[i]).pow(2)).sum();
        if distance < best.1 {
            best = (index as u32, distance);
        }
    }
    *raw = f64::from((0xc300_0000u32 | best.0) as i32);
}

/// DWG のその版が持てない値（DXF の書き出しは落とさない値）を、DWG の書き出しと同じく落とした図面（書き出しが notices で
/// 知らせる）。2010 より前の版のマルチ引出線は、垂直の付け方・上下の文字の付け方・引出線ごとの色を持たず、2004 より前の
/// 版の色（CMC）は色の番号だけ
fn reduced(doc: &DxfDocument, version: DwgVersion) -> DxfDocument {
    let mut doc = doc.clone();
    // R13 は IMAGE を持たない（書き出しが version-image で知らせる）
    if version == DwgVersion::R13 {
        doc.entities.retain(|entity| !entity.base.kind.eq_ignore_ascii_case("IMAGE"));
    }
    for entity in &mut doc.entities {
        let DxfEntityData::MLeader(mleader) = &mut entity.data else { continue };
        if version < DwgVersion::R2010 {
            mleader.text_top_attachment_type = 9.0;
            mleader.text_bottom_attachment_type = 9.0;
            for leader in &mut mleader.context.leaders {
                leader.attachment_direction = 0.0;
                for line in &mut leader.lines {
                    line.color = BY_BLOCK_RAW;
                }
            }
        }
        if version < DwgVersion::R2004 {
            nearest_raw(&mut mleader.leader_line_color);
            if let Some(mtext) = &mut mleader.context.mtext {
                nearest_raw(&mut mtext.color);
                nearest_raw(&mut mtext.bg_color);
            }
            if let Some(block) = &mut mleader.context.block {
                nearest_raw(&mut block.color);
            }
        }
    }
    doc
}

/// R13・R14 が持てない値を、DWG の書き出しと同じく落とした図面（書き出しが notices で知らせる）。線の太さ（読み取りは
/// 図形を BYLAYER、レイヤを既定とする）、レイヤを印刷しない設定、長さの単位、ビューポートの非矩形の切り抜き、複数行の
/// 文字の行間
fn reduced_r14(doc: &DxfDocument) -> DxfDocument {
    let mut doc = doc.clone();
    doc.header.insunits = 0.0;
    for layer in &mut doc.layers {
        layer.plot = true;
        layer.lineweight = -3.0;
    }
    fn entity(item: &mut DxfEntity) {
        item.base.lineweight = -1.0;
        match &mut item.data {
            DxfEntityData::Viewport(viewport) => {
                viewport.clipping_boundary_handle = None;
                viewport.status_flags = f64::from(viewport.status_flags as i32 & !0x10000);
            }
            DxfEntityData::MText(mtext) => {
                mtext.line_spacing_style = 1.0;
                mtext.line_spacing_factor = 1.0;
            }
            DxfEntityData::Insert(insert) => insert.attribs.iter_mut().for_each(entity),
            _ => {}
        }
    }
    doc.entities.iter_mut().for_each(entity);
    doc
}

/// ハンドルのない図形に、元の最大より後のハンドルを振った図面（DXF と DWG の書き出しが別々に振るハンドルで、比べる
/// 図形がずれないように）
fn with_handles(doc: &DxfDocument) -> DxfDocument {
    let mut doc = doc.clone();
    let handle = |entity: &DxfEntity| u64::from_str_radix(&entity.base.handle, 16).ok().filter(|value| *value > 0);
    let mut used: HashSet<u64> = HashSet::new();
    for entity in &doc.entities {
        used.extend(handle(entity));
        if let DxfEntityData::Insert(insert) = &entity.data {
            used.extend(insert.attribs.iter().filter_map(handle));
        }
    }
    let mut next = used.iter().max().copied().unwrap_or(0).max(0x1000) + 1;
    let mut fill = |entity: &mut DxfEntity| {
        let known = u64::from_str_radix(&entity.base.handle, 16).ok().filter(|value| *value > 0);
        if known.is_none() {
            entity.base.handle = format!("{next:X}");
            next += 1;
        }
    };
    for entity in &mut doc.entities {
        fill(entity);
        if let DxfEntityData::Insert(insert) = &mut entity.data {
            insert.attribs.iter_mut().for_each(&mut fill);
        }
    }
    doc
}

/// 図面をそれぞれの版の DWG に書いて読み直し、同じ版の DXF に書いて読み直した図面（`write_dxf_as` は版が持てない値を
/// DWG の書き出しと同じ形にする）と、すべての表示で比べる。R13・R14 の DXF は書けないので、R13・R14 が持てない値を
/// 落とした図面を 2000 の DXF に書いて読み直した図面と比べる
fn check_as(name: &str, bytes: &[u8], version: DwgVersion) -> (usize, Vec<String>) {
    let Some(doc) = model(bytes) else { return (0, Vec::new()) };
    let doc = with_handles(&doc);
    let written = zumen_dwg::write_dwg_as(&doc, version);
    let again = match zumen_dwg::read_dwg(&written.bytes) {
        Ok(again) => again,
        Err(error) => return (0, vec![format!("{name} {}: rereading: {error}", version.name())]),
    };
    assert_eq!(again.version, version.code(), "{name}");
    let reference = match DxfVersion::parse(version.name()) {
        Some(dxf) => parse_dxf(&zumen_dxf::write_dxf_as(&reduced(&doc, version), dxf).bytes),
        None => parse_dxf(&zumen_dxf::write_dxf_as(&reduced_r14(&reduced(&doc, version)), DxfVersion::R2000).bytes),
    };
    let skipped = fit_splines(&doc);
    let reference = expected_as(&reference, version);
    let original = dxf_to_scene(&reference, None);
    let views: Vec<Option<String>> =
        std::iter::once(None).chain(original.views.iter().flatten().map(|view| Some(view.id.clone()))).collect();
    let mut out = Vec::new();
    for view in &views {
        let before = dxf_to_scene(&reference, view.as_deref());
        let after = dxf_to_scene(&again, view.as_deref());
        out.extend(
            differences(&before, &after, &skipped)
                .into_iter()
                .map(|line| format!("{name} {} ({view:?}) {line}", version.name())),
        );
    }
    (views.len(), out)
}

fn drawings(folder: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "dxf" | "dwg"))
        })
        .collect();
    files.sort();
    files
}

#[test]
fn written_drawings_draw_the_same() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for format in ["dxf", "dwg"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let (views, differences) = check(&name, &fs::read(&file).unwrap());
            failures.extend(differences);
            checked += views;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked >= 20, "checked only {checked} views");
}

/// どの版に書いても、同じ版の DXF と同じに描ける（2013 は written_drawings_draw_the_same が元の図面と比べる）
#[test]
fn drawings_written_in_every_version_draw_like_dxf_of_that_version() {
    let mut checked = 0;
    let mut failures = Vec::new();
    for format in ["dxf", "dwg"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let bytes = fs::read(&file).unwrap();
            for version in DwgVersion::ALL.into_iter().filter(|version| *version != DwgVersion::R2013) {
                let (views, differences) = check_as(&name, &bytes, version);
                failures.extend(differences);
                checked += views;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(checked >= 100, "checked only {checked} views");
}

/// Git に入れない図面で、どの版に書いても同じ版の DXF と同じに描けることを確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-dwg --release --test write_model -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn samples_written_in_every_version_draw_like_dxf_of_that_version() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in drawings(Path::new(&folder)) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(&file).unwrap();
        for version in DwgVersion::ALL {
            let (views, differences) = check_as(&name, &bytes, version);
            println!("{name} {}: {views} views, {} differences", version.name(), differences.len());
            failures.extend(differences.into_iter().take(5));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの DXF・DWG）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-dwg --release --test write_model -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn written_samples_draw_the_same() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut failures = Vec::new();
    for file in drawings(Path::new(&folder)) {
        let name = file.file_name().unwrap().to_string_lossy().to_string();
        let (views, differences) = check(&name, &fs::read(&file).unwrap());
        println!("{name}: {views} views, {} differences", differences.len());
        failures.extend(differences.into_iter().take(5));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn oversized_linetype_patterns_are_reported() {
    let mut doc = parse_dxf(b"0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLTYPE\n0\nLTYPE\n2\nLONG\n73\n1\n40\n1\n49\n1\n0\nENDTAB\n0\nENDSEC\n0\nEOF\n");
    doc.linetypes[0].dashes = vec![1.0; 256];
    doc.linetypes[0].pattern_length = 256.0;
    let written = zumen_dwg::write_dwg(&doc);
    assert!(written.notices.iter().any(|notice| notice.code == "linetype-limit" && notice.count == 1));
    let again = zumen_dwg::read_dwg(&written.bytes).unwrap();
    assert_eq!(again.linetypes.iter().find(|line| line.name == "LONG").unwrap().dashes.len(), 255);
}

/// 壊れた・途中で切れた図面から読んだモデルでも panic せずすぐに書け、書いた図面は読み取りが開ける
#[test]
fn models_of_damaged_drawings_are_written() {
    let mut seed: u64 = 0x5eed_0003;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    for format in ["dxf", "dwg"] {
        for file in drawings(&fixtures().join(format)) {
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            let original = fs::read(&file).unwrap();
            for round in 0..30 {
                let mut bytes = original.clone();
                match round % 3 {
                    0 => bytes.truncate(random(bytes.len())),
                    1 => {
                        for _ in 0..1 + random(8) {
                            let at = random(bytes.len());
                            bytes[at] = random(256) as u8;
                        }
                    }
                    _ => {
                        let at = random(bytes.len());
                        let length = random(64).min(bytes.len() - at);
                        bytes.drain(at..at + length);
                    }
                }
                let Some(doc) = model(&bytes) else { continue };
                let start = std::time::Instant::now();
                let written = zumen_dwg::write_dwg(&doc);
                assert!(
                    zumen_dwg::read_dwg(&written.bytes).is_ok(),
                    "{name} round {round}: the written drawing does not read"
                );
                assert!(start.elapsed().as_secs() < 5, "{name} round {round} took too long");
            }
        }
    }
}

/// DWG の文字列は 32,767 単位（UTF-16）までなので、長い文字列はサロゲートの対を分けずにそこまでで切り、notices で知らせる
#[test]
fn long_strings_are_cut_with_a_notice() {
    let mut doc = parse_dxf(&fs::read(fixtures().join("dxf/features.dxf")).unwrap());
    let mtext = |entity: &DxfEntity| match &entity.data {
        DxfEntityData::MText(mtext) => Some(mtext.raw_text.clone()),
        _ => None,
    };
    let handle = doc.entities.iter().find(|entity| mtext(entity).is_some()).unwrap().base.handle.clone();
    for entity in &mut doc.entities {
        if entity.base.handle == handle
            && let DxfEntityData::MText(mtext) = &mut entity.data
        {
            mtext.raw_text = "\u{2000b}".repeat(20_000);
        }
    }
    let written = zumen_dwg::write_dwg(&doc);
    let notice = written.notices.iter().find(|notice| notice.code == "long-text").expect("no long-text notice");
    assert_eq!(notice.count, 1);
    let again = zumen_dwg::read_dwg(&written.bytes).unwrap();
    let text = again.entities.iter().find(|entity| entity.base.handle == handle).and_then(mtext).unwrap();
    assert_eq!(text, "\u{2000b}".repeat(16_383));
}

#[test]
fn reports_what_could_not_be_written_as_it_was() {
    let bytes = fs::read(fixtures().join("dxf/features.dxf")).unwrap();
    let written = zumen_dwg::write_dwg(&parse_dxf(&bytes));
    let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
    for code in ["image", "helix", "mpolygon", "table", "attrib-alone", "complex-linetype", "missing-block"] {
        assert!(codes.contains(&code), "{code} not in {codes:?}");
    }
}

/// 古い版が持てない値は、DXF の書き出し（write_dxf_as）と同じ notices で知らせる
#[test]
fn reports_what_older_versions_cannot_hold() {
    let doc = parse_dxf(&fs::read(fixtures().join("dxf/features.dxf")).unwrap());
    let codes = |version: DwgVersion| -> Vec<String> {
        zumen_dwg::write_dwg_as(&doc, version).notices.into_iter().map(|notice| notice.code).collect()
    };
    // R13・R14 は線の太さ・レイヤを印刷しない設定・長さの単位・複数行の文字の行間を持たず、R13 は HATCH を持たない
    let before_2000 = ["version-lineweight", "version-layer-plot", "version-units", "version-line-spacing"];
    let before_2004 = ["version-color", "version-transparency", "version-gradient", "version-dimension"];
    let before_2007 = ["version-background", "version-mleader"];
    let before_2010 = ["version-mleader-2010"];
    for version in DwgVersion::ALL {
        let codes = codes(version);
        let expected = |list: &[&str], since: DwgVersion| {
            for code in list {
                assert_eq!(codes.iter().any(|known| known == code), version < since, "{code} in {}", version.name());
            }
        };
        expected(&["version-hatch"], DwgVersion::R14);
        expected(&before_2000, DwgVersion::R2000);
        expected(&before_2004, DwgVersion::R2004);
        expected(&before_2007, DwgVersion::R2007);
        expected(&before_2010, DwgVersion::R2010);
    }
    // R13・R14 が表示する紙空間は 1 つで、ビューポートの切り抜きは矩形だけ
    let mut doc = parse_dxf(&fs::read(fixtures().join("dxf/viewports.dxf")).unwrap());
    let boundary = doc
        .entities
        .iter()
        .find(|entity| entity.base.space == DxfSpace::Paper && !matches!(entity.data, DxfEntityData::Viewport(_)))
        .map(|entity| entity.base.handle.clone())
        .unwrap();
    let viewport = doc.entities.iter_mut().rev().find_map(|entity| match &mut entity.data {
        DxfEntityData::Viewport(viewport) if entity.base.space == DxfSpace::Paper => Some(viewport),
        _ => None,
    });
    viewport.unwrap().clipping_boundary_handle = Some(boundary);
    for version in DwgVersion::ALL {
        let written = zumen_dwg::write_dwg_as(&doc, version);
        for code in ["version-layouts", "version-viewport-clip"] {
            let found = written.notices.iter().any(|notice| notice.code == code);
            assert_eq!(found, version < DwgVersion::R2000, "{code} in {}", version.name());
        }
        let again = zumen_dwg::read_dwg(&written.bytes).unwrap();
        assert_eq!(again.layouts.len(), doc.layouts.len(), "{}", version.name());
    }
    // 複数行の属性は 2018 から（それより前は 1 行の属性にして知らせる）
    let doc = parse_dxf(&fs::read(fixtures().join("dxf/attributes-multiline.dxf")).unwrap());
    for version in DwgVersion::ALL {
        let written = zumen_dwg::write_dwg_as(&doc, version);
        let flattened = written.notices.iter().any(|notice| notice.code == "multiline-attrib");
        assert_eq!(flattened, version < DwgVersion::R2018, "{}", version.name());
        let again = zumen_dwg::read_dwg(&written.bytes).unwrap();
        let multiline = again.entities.iter().any(|entity| match &entity.data {
            DxfEntityData::Attdef { mtext, .. } => mtext.is_some(),
            DxfEntityData::Insert(insert) => {
                insert.attribs.iter().any(|attrib| matches!(&attrib.data, DxfEntityData::Attrib { mtext: Some(_), .. }))
            }
            _ => false,
        });
        assert_eq!(multiline, version >= DwgVersion::R2018, "{}", version.name());
    }
}

/// 外部参照（BLOCK_HEADER の参照とオーバーレイのビット、参照先のパス、20.4.52）は DXF の 70 の 4・8 と 1 として読み、
/// DXF と同じく参照先の図面を読まないことを notices で知らせる
#[test]
fn external_references_are_reported_as_from_dxf() {
    let doc = parse_dxf(&fs::read(fixtures().join("dxf/xrefs.dxf")).unwrap());
    let expected = dxf_to_scene(&doc, None).notices.into_iter().find(|notice| notice.code == "xref");
    assert_eq!(expected.as_ref().map(|notice| notice.count), Some(4));
    for version in DwgVersion::ALL {
        let again = zumen_dwg::read_dwg(&zumen_dwg::write_dwg_as(&doc, version).bytes).unwrap();
        let notice = dxf_to_scene(&again, None).notices.into_iter().find(|notice| notice.code == "xref");
        assert_eq!(notice, expected, "{}", version.name());
    }
}

/// 画像の定義のファイルが分かる画像は、定義・リアクタ・辞書・クラスとともに書く。DXF・DWG のどちらから読んだ画像も
/// 書け、同じ定義を指していた画像は 1 つの定義を共有する。R13 は省いて知らせる
#[test]
fn writes_images_with_their_definitions() {
    let operations = zumen_core::edit::parse_operations(
        r#"[
            {"op": "add", "kind": "image", "path": "maps/site.png", "pixels": [200, 100], "position": [10, 20], "width": 50},
            {"op": "add", "kind": "image", "path": "地図/案内図.jpg", "pixels": [20, 10], "position": [0, 0], "rotation": 30}
        ]"#,
    )
    .unwrap();
    let images = |doc: &DxfDocument| -> Vec<(Option<String>, [f64; 2], [f64; 3])> {
        doc.entities
            .iter()
            .filter_map(|entity| match &entity.data {
                DxfEntityData::Image(image) if entity.base.kind == "IMAGE" => {
                    Some((image.path.clone(), image.image_size, image.u_pixel))
                }
                _ => None,
            })
            .collect()
    };
    let from_dwg =
        zumen_dwg::read_dwg(&zumen_dwg::edit_dwg(&zumen_dwg::new_dwg("mm").unwrap(), &operations).unwrap().bytes)
            .unwrap();
    let from_dxf = parse_dxf(&zumen_dxf::edit_dxf(&zumen_dxf::new_dxf("mm").unwrap(), &operations).unwrap().bytes);
    for (source, mut doc) in [("dwg", from_dwg), ("dxf", from_dxf)] {
        // 1 つ目の画像を、同じ定義を指す画像として複製する
        let mut shared = doc.entities.iter().find(|entity| entity.base.kind == "IMAGE").unwrap().clone();
        shared.base.handle = "FFFF".into();
        doc.entities.push(shared);
        assert_eq!(images(&doc).len(), 3, "{source}");
        for version in DwgVersion::ALL {
            let written = zumen_dwg::write_dwg_as(&doc, version);
            let codes: Vec<&str> = written.notices.iter().map(|notice| notice.code.as_str()).collect();
            assert!(!codes.contains(&"image"), "{source} {version:?}: {codes:?}");
            let again =
                zumen_dwg::read_dwg(&written.bytes).unwrap_or_else(|error| panic!("{source} {version:?}: {error}"));
            if version == DwgVersion::R13 {
                assert!(codes.contains(&"version-image") && images(&again).is_empty(), "{source}: {codes:?}");
                continue;
            }
            assert_eq!(images(&again), images(&doc), "{source} {version:?}");
            let definitions: HashSet<&str> = again
                .entities
                .iter()
                .filter_map(|entity| match &entity.data {
                    DxfEntityData::Image(image) => image.definition_handle.as_deref(),
                    _ => None,
                })
                .collect();
            assert_eq!(definitions.len(), 2, "{source} {version:?}: the copy shares its definition");
        }
    }
}
