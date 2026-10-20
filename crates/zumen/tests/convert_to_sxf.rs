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
