//! 手元の実物の図面（ZUMEN_SAMPLES の SFC・P21）のその場の書き直し

mod common;

use common::compare_entities;
use std::collections::HashMap;
use zumen_sxf::{Styled, SxfDrawing, SxfFeature, parse_sxf, sxf_to_scene, write_p21, write_sfc};

fn samples() -> Vec<std::path::PathBuf> {
    let Ok(folder) = std::env::var("ZUMEN_SAMPLES") else {
        return Vec::new();
    };
    let mut paths: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.retain(|path| {
        path.extension().is_some_and(|ext| matches!(ext.to_string_lossy().to_lowercase().as_str(), "sfc" | "p21"))
    });
    paths.sort();
    paths
}

fn write(drawing: &SxfDrawing) -> zumen_sxf::SxfWritten {
    if drawing.format == "sfc" { write_sfc(drawing) } else { write_p21(drawing) }
}

/// 最初の部分図の最初の線分を動かし、最初の寸法を消し、用紙に線分を 1 本足す
fn edit(drawing: &mut SxfDrawing) {
    let doc = &mut drawing.document;
    let names: Vec<String> = doc.sub_figures.values().map(|figure| figure.feature.name.clone()).collect();
    'outer: for name in &names {
        let mut figure = doc.sub_figures.get(name).unwrap().clone();
        for member in &mut figure.members {
            if let SxfFeature::Line { x2, y2, .. } = member {
                *x2 += 100.0;
                *y2 -= 50.0;
                doc.sub_figures.set(name.clone(), figure);
                break 'outer;
            }
        }
    }
    for name in &names {
        let mut figure = doc.sub_figures.get(name).unwrap().clone();
        if let Some(position) = figure.members.iter().position(|member| matches!(member, SxfFeature::LinearDim { .. }))
        {
            figure.members.remove(position);
            doc.sub_figures.set(name.clone(), figure);
            break;
        }
    }
    doc.sheet_members.push(SxfFeature::Line {
        style: Styled { id: 9_000_001.0, layer: 1.0, color: 2.0, line_type: 1.0, line_width: 3.0 },
        x1: 10.0,
        y1: 10.0,
        x2: 200.0,
        y2: 150.0,
    });
}

#[test]
#[ignore = "ZUMEN_SAMPLES の図面を使う"]
fn local_samples_are_edited_in_place() {
    for path in samples() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        let format = path.extension().unwrap().to_string_lossy().to_lowercase();
        let drawing = parse_sxf(&bytes, &format).unwrap();
        assert_eq!(write(&drawing).bytes, bytes, "{name}: 変えずに書くと同じバイト列");
        let mut edited = drawing.clone();
        edit(&mut edited);
        let started = std::time::Instant::now();
        let written = write(&edited);
        let elapsed = started.elapsed();
        if let Ok(out) = std::env::var("SCRATCH_OUT") {
            std::fs::write(format!("{out}/edited-{name}"), &written.bytes).unwrap();
        }
        let again = parse_sxf(&written.bytes, &format).unwrap();
        let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
        compare_entities(&sxf_to_scene(&edited), &sxf_to_scene(&again), Some(&ids), 1e-9)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let changed = bytes.len().abs_diff(written.bytes.len());
        eprintln!(
            "{name}: {} → {} bytes ({changed} 差), {elapsed:?}, {:?}",
            bytes.len(),
            written.bytes.len(),
            written.notices
        );
    }
}
