//! P21 の書き出し: tests/fixtures の SFC・P21 を読んで P21 に書き、読み直すと同じ図面になる

mod common;

use common::{FIXTURES, compare_entities, read_fixture};
use std::collections::HashMap;
use zumen_sxf::{parse_p21, parse_sxf, sxf_to_scene, write_p21};

#[test]
fn written_p21_reads_back_as_the_same_drawing() {
    for (format, name) in FIXTURES {
        let drawing = parse_sxf(&read_fixture(format, name), format).unwrap();
        let written = write_p21(&drawing);
        let again = parse_p21(&written.bytes).unwrap();
        let (before, after) = (sxf_to_scene(&drawing), sxf_to_scene(&again));
        let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
        // P21 から読んだフィーチャは同じインスタンス番号を持つことがあるので、id は SFC だけで比べる
        let ids = (format == "sfc").then_some(&ids);
        compare_entities(&before, &after, ids, 1e-9).unwrap_or_else(|error| panic!("{name}.{format}: {error}"));
        if before.sheet.is_some() {
            assert_eq!(before.sheet, after.sheet, "{name}.{format}");
        }
        // 書いた P21 を書き直しても変わらない
        let twice = write_p21(&again).bytes;
        assert_eq!(write_p21(&parse_p21(&twice).unwrap()).bytes, twice, "{name}.{format}");
    }
}

/// 手元の実物の図面（ZUMEN_SAMPLES の SFC・P21）でも同じ図面になる
#[test]
#[ignore = "ZUMEN_SAMPLES の図面を使う"]
fn written_p21_of_local_samples_reads_back_as_the_same_drawing() {
    let Ok(folder) = std::env::var("ZUMEN_SAMPLES") else {
        return;
    };
    let mut entries: Vec<_> = std::fs::read_dir(&folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        let Some(format) = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) else {
            continue;
        };
        if !matches!(format.as_str(), "sfc" | "p21" | "sfz" | "p2z") {
            continue;
        }
        let drawing = parse_sxf(&std::fs::read(&path).unwrap(), &format).unwrap();
        let written = write_p21(&drawing);
        let again = parse_p21(&written.bytes).unwrap();
        let (before, after) = (sxf_to_scene(&drawing), sxf_to_scene(&again));
        let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
        let ids = (drawing.format == "sfc").then_some(&ids);
        let name = path.file_name().unwrap().to_string_lossy();
        compare_entities(&before, &after, ids, 1e-9).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(before.sheet, after.sheet, "{name}");
        eprintln!("{name}: {} entities, {} bytes, {:?}", before.entities.len(), written.bytes.len(), written.notices);
    }
}
