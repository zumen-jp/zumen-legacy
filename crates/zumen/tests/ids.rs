//! 図形の id の約束（docs/architecture.md の「Entity ids」）: 同じバイト列の図面は、どの版でも同じ id になる。
//! tests/fixtures のすべての図面と表示の id を ids.json に記録しておき、読み取りの変更で id が変わったら失敗する。
//! `pnpm run golden` はこの記録を書き換えない。図面のバイト列が変わったとき（図面を作るテストを変えたとき）と、
//! 図面を足したときは、id を確かめてから `ZUMEN_RECORD_IDS=1 cargo test -p zumen --test ids` で記録し直す。
//! 同じバイト列の図面の id を変える読み取りの変更は約束を破るので、記録し直す前に決定（docs/decisions）に残す。

use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn record() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/ids.json")
}

/// バイト列の FNV-1a（64 ビット）。図面が変わったかどうかだけを見る
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn ids(bytes: &[u8], view: Option<String>) -> (Vec<Value>, Option<Vec<String>>) {
    let options = zumen::ReadDrawingOptions { view, ..zumen::ReadDrawingOptions::default() };
    let scene = zumen::read(bytes, &options).expect("a fixture that reads");
    let views = scene.views.as_ref().map(|views| views.iter().map(|view| view.id.clone()).collect());
    (scene.entities.iter().map(|entity| Value::String(entity.id.clone())).collect(), views)
}

/// 図面ごとに、バイト列の指紋と、表示ごとの id（表示が 1 つしかない形式は ""）
fn current() -> Map<String, Value> {
    let mut out = Map::new();
    let mut formats: Vec<PathBuf> = fs::read_dir(fixtures())
        .expect("tests/fixtures")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    formats.sort();
    for format in formats {
        let mut files: Vec<PathBuf> = fs::read_dir(&format)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                let name = path.file_name().unwrap().to_string_lossy();
                !name.ends_with(".json") && !name.starts_with('.')
            })
            .collect();
        files.sort();
        for file in files {
            let bytes = fs::read(&file).unwrap();
            let (default, views) = ids(&bytes, None);
            let mut by_view = Map::new();
            match views {
                None => {
                    by_view.insert(String::new(), Value::Array(default));
                }
                Some(views) => {
                    for view in views {
                        by_view.insert(view.clone(), Value::Array(ids(&bytes, Some(view)).0));
                    }
                }
            }
            let name = format!(
                "{}/{}",
                format.file_name().unwrap().to_string_lossy(),
                file.file_name().unwrap().to_string_lossy()
            );
            out.insert(name, json!({ "bytes": fingerprint(&bytes), "views": by_view }));
        }
    }
    out
}

#[test]
fn ids_stay_the_same_for_the_same_bytes() {
    let now = current();
    assert!(!now.is_empty(), "no fixtures");
    if std::env::var_os("ZUMEN_RECORD_IDS").is_some() {
        fs::write(record(), format!("{}\n", serde_json::to_string_pretty(&Value::Object(now)).unwrap())).unwrap();
        return;
    }
    let recorded: Map<String, Value> =
        serde_json::from_str(&fs::read_to_string(record()).expect("crates/zumen/tests/ids.json")).unwrap();
    let again = "check the ids, then record them with ZUMEN_RECORD_IDS=1 cargo test -p zumen --test ids";
    let mut problems = Vec::new();
    for (name, entry) in &now {
        let Some(before) = recorded.get(name) else {
            problems.push(format!("{name}: a new fixture whose ids are not recorded; {again}"));
            continue;
        };
        if before["bytes"] != entry["bytes"] {
            problems.push(format!("{name}: the bytes of the fixture changed; {again}"));
            continue;
        }
        let (Some(was), Some(is)) = (before["views"].as_object(), entry["views"].as_object()) else {
            problems.push(format!("{name}: the record has no views"));
            continue;
        };
        for (view, ids) in is {
            let Some(old) = was.get(view) else {
                problems.push(format!("{name} @{view}: a view whose ids are not recorded"));
                continue;
            };
            if old != ids {
                let (old, new) = (old.as_array().unwrap(), ids.as_array().unwrap());
                let first = old.iter().zip(new).position(|(a, b)| a != b).unwrap_or(old.len().min(new.len()));
                problems.push(format!(
                    "{name} @{view}: entity ids changed for the same bytes (entity {first} in drawing order: {:?} → \
                     {:?}, {} → {} entities). Ids are a contract; keep them, or record the change in a decision",
                    old.get(first),
                    new.get(first),
                    old.len(),
                    new.len()
                ));
            }
        }
        for view in was.keys().filter(|view| !is.contains_key(*view)) {
            problems.push(format!("{name} @{view}: the view is gone"));
        }
    }
    for name in recorded.keys().filter(|name| !now.contains_key(*name)) {
        problems.push(format!("{name}: the fixture is gone; {again}"));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
