//! 読んで書くと元と同じバイト列になること（tests/fixtures/jww と、ZUMEN_SAMPLES の図面）

use std::path::Path;
use zumen_jww::{JwwEntity, JwwFile, read_jww};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../tests/fixtures/jww/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// 最初に違うバイトの位置
fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

fn assert_same_bytes(name: &str, bytes: &[u8]) {
    let file = JwwFile::read(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
    // 図形の一覧とブロック定義は、表示用の読み取りと同じ
    let doc = read_jww(bytes).unwrap();
    assert_eq!(doc.truncated, None, "{name}");
    let entities: Vec<&JwwEntity> = file.entities.iter().map(|record| &record.entity).collect();
    assert_eq!(entities, doc.entities.iter().collect::<Vec<_>>(), "{name}");
    assert_eq!(file.blocks.len(), doc.blocks.len(), "{name}");
    for (block, definition) in file.blocks.iter().zip(&doc.blocks) {
        let entities: Vec<&JwwEntity> = block.entities.iter().map(|record| &record.entity).collect();
        assert_eq!((block.number, &block.name), (definition.number, &definition.name), "{name}");
        assert_eq!(entities, definition.entities.iter().collect::<Vec<_>>(), "{name}");
    }
    let written = file.write();
    assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
    assert_eq!(
        first_difference(&written.bytes, bytes),
        None,
        "{name}: {} / {} bytes",
        written.bytes.len(),
        bytes.len()
    );
}

#[test]
fn rewrites_the_fixtures_byte_for_byte() {
    for name in ["minimal-v600.jww", "minimal-v700.jww"] {
        assert_same_bytes(name, &fixture(name));
        // fixtures は文字の設定の欄を持たない（その前までは型つき）
        let header = JwwFile::read(&fixture(name)).unwrap().header;
        let settings = header.settings.as_ref().unwrap();
        assert!(settings.sxf.is_some() && settings.text.is_none() && header.rest.is_empty(), "{name}");
    }
}

/// Git に入れない図面（ZUMEN_SAMPLES のフォルダーの JWW）で確かめる。
/// `ZUMEN_SAMPLES=<folder> cargo test -p zumen-jww --test roundtrip -- --ignored`
#[test]
#[ignore = "needs drawings in ZUMEN_SAMPLES"]
fn rewrites_the_samples_byte_for_byte() {
    let folder = std::env::var("ZUMEN_SAMPLES").expect("set ZUMEN_SAMPLES to a folder of drawings");
    let mut checked = 0;
    for entry in std::fs::read_dir(Path::new(&folder)).unwrap() {
        let path = entry.unwrap().path();
        if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("jww")) {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&path).unwrap();
        assert_same_bytes(&name, &bytes);
        // Jw_cad が書いたヘッダは、すべての欄を型つきで読める
        let header = JwwFile::read(&bytes).unwrap().header;
        let settings = header.settings.as_ref().unwrap_or_else(|| panic!("{name}: no settings"));
        assert!(settings.display.is_some() && settings.text.is_some() && header.rest.is_empty(), "{name}");
        checked += 1;
    }
    assert!(checked > 0, "no JWW in {folder}");
}
