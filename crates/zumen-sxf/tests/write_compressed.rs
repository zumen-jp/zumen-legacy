//! 圧縮した SXF（SFZ・P2Z）の書き出し

mod common;

use common::{compare_entities, read_fixture};
use std::collections::HashMap;
use zumen_sxf::{SxfFeature, parse_sxf, sxf_to_scene, write_sxf};

/// CRC-32（ZIP と同じ多項式）
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { 0xEDB8_8320 ^ (crc >> 1) } else { crc >> 1 };
        }
    }
    !crc
}

/// 無圧縮のファイルを並べた ZIP の書庫
fn stored_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content) in files {
        let offset = out.len() as u32;
        let mut header = Vec::new();
        header.extend(20u16.to_le_bytes());
        header.extend([0u8; 4]);
        header.extend([0x00, 0x60, 0x21, 0x5a]);
        header.extend(crc32(content).to_le_bytes());
        header.extend((content.len() as u32).to_le_bytes());
        header.extend((content.len() as u32).to_le_bytes());
        header.extend((name.len() as u16).to_le_bytes());
        header.extend(0u16.to_le_bytes());
        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(&header);
        out.extend(name.as_bytes());
        out.extend(*content);
        central.extend(0x0201_4b50u32.to_le_bytes());
        central.extend(20u16.to_le_bytes());
        central.extend(&header);
        central.extend([0u8; 10]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[test]
fn drawings_are_written_as_compressed_sxf() {
    for (format, compressed) in [("sfc", "sfz"), ("p21", "p2z"), ("sfc", "p2z"), ("p21", "sfz")] {
        for name in ["features", "sample"] {
            let drawing = parse_sxf(&read_fixture(format, name), format).unwrap();
            let written = write_sxf(&drawing, compressed).unwrap();
            let again = parse_sxf(&written.bytes, compressed).unwrap();
            let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
            // 形式が変わるとき、P21 から読んだフィーチャは同じ番号を持つことがあるので、id は SFC から書くときだけ比べる
            let ids = (format == "sfc").then_some(&ids);
            compare_entities(&sxf_to_scene(&drawing), &sxf_to_scene(&again), ids, 1e-9)
                .unwrap_or_else(|error| panic!("{name}.{format} → {compressed}: {error}"));
        }
    }
}

#[test]
fn a_compressed_drawing_keeps_its_archive_and_the_other_files() {
    let p21 = read_fixture("p21", "sample");
    let archive = stored_zip(&[("readme.txt", b"hello"), ("DRAWING/D0001.P21", &p21), ("raster.jpg", b"\xff\xd8\xff")]);
    let drawing = parse_sxf(&archive, "p2z").unwrap();
    // 変えなければ元の書庫のまま
    assert_eq!(write_sxf(&drawing, "p2z").unwrap().bytes, archive);
    // 変えた図面は、書庫の SXF だけを置き換える
    let mut edited = drawing.clone();
    edited.document.sheet_members.retain(|feature| !matches!(feature, SxfFeature::LinearDim { .. }));
    let written = write_sxf(&edited, "p2z").unwrap();
    assert!(written.bytes.windows(5).any(|window| window == b"hello"));
    assert!(written.bytes.windows(3).any(|window| window == b"\xff\xd8\xff"));
    let again = parse_sxf(&written.bytes, "p2z").unwrap();
    assert_eq!(again.source.as_ref().unwrap().archive.as_ref().unwrap().name, "DRAWING/D0001.P21");
    compare_entities(&sxf_to_scene(&edited), &sxf_to_scene(&again), None, 1e-9).unwrap();
    assert_eq!(again.document.sheet_members.len(), edited.document.sheet_members.len());
    // 書庫の中の P21 は、元の P21 を書き換えたもの（変えていないインスタンスは同じ文字列）
    let inner = again.source.as_ref().unwrap().bytes.clone();
    assert!(inner.windows(30).any(|window| window == &p21[p21.len() - 200..p21.len() - 170]));
}

#[test]
fn changing_version_preserves_other_archive_members() {
    use zumen_sxf::{SxfVersion, write_sxf_version};
    for (format, compressed) in [("sfc", "sfz"), ("p21", "p2z")] {
        let source = read_fixture(format, "features");
        let name = format!("DRAWING/source.{format}");
        let archive = stored_zip(&[("metadata.txt", b"keep-this-metadata"), (&name, &source)]);
        let drawing = parse_sxf(&archive, compressed).unwrap();
        let written = write_sxf_version(&drawing, compressed, SxfVersion::V2_0).unwrap();
        assert!(written.bytes.windows(18).any(|bytes| bytes == b"keep-this-metadata"));
        let again = parse_sxf(&written.bytes, compressed).unwrap();
        assert_eq!(SxfVersion::of(&again), Some(SxfVersion::V2_0));
        assert_eq!(again.source.unwrap().archive.unwrap().name, name);
    }
}
