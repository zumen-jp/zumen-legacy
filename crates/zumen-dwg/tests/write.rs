//! DWG の書き出しのテスト。書き直した図面は、元の図面と同じ AutoCAD の図面のモデルと表示用の図面になる。
//!
//! 図面は tests/fixtures/dwg（tests/writer で書いたテストの図面）と、ZUMEN_SAMPLES のフォルダの実際の図面
//! （リポジトリには入れない。フォルダがなければ飛ばす）

use std::path::{Path, PathBuf};
use zumen_dwg::{read_dwg, rewrite_dwg};
use zumen_dxf::dxf_to_scene;

fn fixtures() -> Vec<(String, Vec<u8>)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/dwg");
    ["r2000.dwg", "r2018.dwg"]
        .iter()
        .map(|name| (name.to_string(), std::fs::read(folder.join(name)).unwrap()))
        .collect()
}

fn samples() -> Vec<(String, Vec<u8>)> {
    let Some(folder) = std::env::var_os("ZUMEN_SAMPLES") else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(PathBuf::from(folder)) else { return Vec::new() };
    let mut out: Vec<(String, Vec<u8>)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("dwg")))
        .filter_map(|path| Some((path.file_name()?.to_string_lossy().into_owned(), std::fs::read(&path).ok()?)))
        .collect();
    out.sort();
    out
}

/// 書き直せる版（R2000、R2004、R2010〜R2018）
fn rewritable(bytes: &[u8]) -> bool {
    matches!(&bytes[..6], b"AC1015" | b"AC1018" | b"AC1024" | b"AC1027" | b"AC1032")
}

/// 書き直した図面は、読むと元の図面と同じモデル（notices を含む）と、どの表示でも同じ図面になる
#[test]
fn rewritten_drawings_read_the_same() {
    for (name, bytes) in fixtures().into_iter().chain(samples()) {
        if !rewritable(&bytes) {
            continue;
        }
        let written = rewrite_dwg(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(written.notices.is_empty(), "{name}: {:?}", written.notices);
        let before = read_dwg(&bytes).unwrap();
        let after = read_dwg(&written.bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(before == after, "{name}: the model differs");
        let views: Vec<Option<&str>> =
            std::iter::once(None).chain(before.layouts.iter().map(|layout| Some(layout.name.as_str()))).collect();
        for view in views {
            assert!(dxf_to_scene(&before, view) == dxf_to_scene(&after, view), "{name} {view:?}: the scene differs");
        }
        // 書き直した図面をもう一度書き直しても同じバイト列
        assert_eq!(rewrite_dwg(&written.bytes).unwrap().bytes, written.bytes, "{name}");
    }
}

/// 書き直せない版と DWG でないファイルは、利用者に見せるメッセージを返す
#[test]
fn unsupported_files_are_refused() {
    let mut r2007 = fixtures()[1].1.clone();
    r2007[..6].copy_from_slice(b"AC1021");
    let mut r14 = fixtures()[0].1.clone();
    r14[..6].copy_from_slice(b"AC1014");
    for bytes in [r2007, r14, b"AC1015".to_vec(), b"not a drawing".to_vec(), Vec::new()] {
        assert!(rewrite_dwg(&bytes).is_err());
    }
}

/// 壊れた・途中で切れた DWG でも panic せず、すぐに終わる（書けないものは Err か notices にする）
#[test]
fn damaged_drawings_rewrite_without_panicking() {
    let mut seed: u64 = 0x5eed_0002;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    for (name, original) in fixtures() {
        for round in 0..300 {
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
            let start = std::time::Instant::now();
            if let Ok(written) = rewrite_dwg(&bytes) {
                // 書いた図面は読み取りが開ける（中身が壊れていれば notices になる）
                let _ = read_dwg(&written.bytes);
            }
            assert!(start.elapsed().as_secs() < 5, "{name} round {round} took too long");
        }
    }
}
