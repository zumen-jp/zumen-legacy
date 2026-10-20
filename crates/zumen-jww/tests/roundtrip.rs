//! 読んで書くと元と同じバイト列になること（tests/fixtures/jww と、ZUMEN_SAMPLES の図面）

use std::path::Path;
use zumen_jww::JwwFile;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../tests/fixtures/jww/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// 最初に違うバイトの位置
fn first_difference(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

fn assert_same_bytes(name: &str, bytes: &[u8]) {
    let file = JwwFile::read(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
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
        assert_same_bytes(&name, &std::fs::read(&path).unwrap());
        checked += 1;
    }
    assert!(checked > 0, "no JWW in {folder}");
}
