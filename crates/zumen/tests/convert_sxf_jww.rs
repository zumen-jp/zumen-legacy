//! SXF・JWW から DXF への変換: tests/fixtures の SXF（SFC・P21）と JWW を DXF に書き直して読み直すと、
//! DXF を開いたときの表示（用紙のレイアウト）に、元の図面と同じに描ける（比べ方は common/mod.rs）。

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
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
    let original = zumen::read(bytes, &ReadDrawingOptions::default()).unwrap();
    let (again, sources, _) = convert(bytes);
    let differences = common::differences(&original, &again, &sources);
    (original.entities.len(), differences.into_iter().map(|line| format!("{name}: {line}")).collect())
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

#[test]
#[ignore]
fn debug_entity() {
    let id = std::env::var("ID").unwrap();
    let file = std::env::var("FILE").unwrap();
    let bytes = fs::read(&file).unwrap();
    let original = zumen::read(&bytes, &ReadDrawingOptions::default()).unwrap();
    let (again, sources, _) = convert(&bytes);
    let entity = original.entities.iter().find(|entity| entity.id == id).unwrap();
    println!("ORIGINAL {}", serde_json::to_string(&entity.primitives).unwrap());
    for entity in &again.entities {
        if common::source_id(&entity.id, &sources).as_deref() == Some(id.as_str()) {
            println!("CONVERTED {} {}", entity.id, serde_json::to_string(&entity.primitives).unwrap());
        }
    }
}
