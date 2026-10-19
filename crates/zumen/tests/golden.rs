//! tests/fixtures の図面を読み、TypeScript の読み取りが書いた正解（*.scene.json）と比べる。
//! 既定の表示は <名前>.scene.json、ほかの表示（DXF のレイアウトなど）は <名前>@<表示>.scene.json。
//! 正解は `pnpm run golden` で作り直す。数は 1e-9 の相対誤差まで同じとみなす（三角関数の実装の差）。
//! DWG は TypeScript では読まないので、tests/fixtures/dwg の正解は @zumenjs/dwg（この読み取りの WebAssembly）が書く。

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn compare(path: &str, expected: &Value, actual: &Value, out: &mut Vec<String>) {
    if out.len() >= 20 {
        return;
    }
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => {
            let (e, a) = (e.as_f64().unwrap(), a.as_f64().unwrap());
            if (e - a).abs() > 1e-9 * e.abs().max(a.abs()).max(1.0) {
                out.push(format!("{path}: {e} ≠ {a}"));
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                out.push(format!("{path}: length {} ≠ {}", e.len(), a.len()));
                return;
            }
            for (index, (e, a)) in e.iter().zip(a).enumerate() {
                compare(&format!("{path}[{index}]"), e, a, out);
            }
        }
        (Value::Object(e), Value::Object(a)) => {
            let mut keys: Vec<&String> = e.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                match (e.get(key), a.get(key)) {
                    (Some(e), Some(a)) => compare(&format!("{path}.{key}"), e, a, out),
                    (e, a) => out.push(format!("{path}.{key}: {e:?} ≠ {a:?}")),
                }
            }
        }
        (e, a) if e == a => {}
        (e, a) => out.push(format!("{path}: {e} ≠ {a}")),
    }
}

#[test]
fn scenes_match_the_typescript_readers() {
    let mut checked = 0;
    for format in fs::read_dir(fixtures()).expect("tests/fixtures") {
        let format = format.unwrap().path();
        if !format.is_dir() {
            continue;
        }
        let names: Vec<String> = fs::read_dir(&format)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        for name in &names {
            if name.ends_with(".json") || name.starts_with('.') {
                continue;
            }
            let file = format.join(name);
            let stem = file.file_stem().unwrap().to_string_lossy().to_string();
            let mut goldens = vec![(None, format!("{stem}.scene.json"))];
            for golden in &names {
                if let Some(view) =
                    golden.strip_prefix(&format!("{stem}@")).and_then(|rest| rest.strip_suffix(".scene.json"))
                {
                    goldens.push((Some(view.to_string()), golden.clone()));
                }
            }
            let bytes = fs::read(&file).unwrap();
            for (view, golden) in goldens {
                let expected: Value = serde_json::from_str(&fs::read_to_string(format.join(&golden)).unwrap()).unwrap();
                let options = zumen::ReadDrawingOptions { format: None, view };
                let scene = zumen::read(&bytes, &options).unwrap_or_else(|error| panic!("{golden}: {error}"));
                let actual = serde_json::to_value(&scene).unwrap();
                let mut differences = Vec::new();
                compare("scene", &expected, &actual, &mut differences);
                assert!(differences.is_empty(), "{golden}:\n{}", differences.join("\n"));
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "no fixtures");
}
