//! 書いた SXF を読み直した図面と、元の図面を比べる

#![allow(dead_code)]

use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use zumen_core::{Scene, SceneEntity};

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

/// tests/fixtures の SXF（フォルダ、拡張子、名前）
pub const FIXTURES: [(&str, &str); 6] = [
    ("sfc", "features"),
    ("sfc", "nosheet"),
    ("sfc", "sample"),
    ("p21", "features"),
    ("p21", "nosheet"),
    ("p21", "sample"),
];

pub fn read_fixture(format: &str, name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(format).join(format!("{name}.{format}"))).unwrap()
}

/// 数の差が tolerance（大きさに比例）以内なら同じとみなして、JSON を比べる。違えば最初の違いの場所を返す
pub fn close_json(a: &Value, b: &Value, tolerance: f64, path: &str) -> Result<(), String> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0) {
                Ok(())
            } else {
                Err(format!("{path}: {x} → {y}"))
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Err(format!("{path}: {} 個 → {} 個", x.len(), y.len()));
            }
            x.iter().zip(y).enumerate().try_for_each(|(i, (x, y))| close_json(x, y, tolerance, &format!("{path}[{i}]")))
        }
        (Value::Object(x), Value::Object(y)) => {
            let keys: Vec<&String> = x.keys().chain(y.keys().filter(|key| !x.contains_key(*key))).collect();
            keys.into_iter().try_for_each(|key| match (x.get(key), y.get(key)) {
                (Some(x), Some(y)) => close_json(x, y, tolerance, &format!("{path}.{key}")),
                _ => Err(format!("{path}.{key}: 片方だけにある")),
            })
        }
        _ if a == b => Ok(()),
        _ => Err(format!("{path}: {a} → {b}")),
    }
}

/// 表示の id（#n>#m …）を、書いたインスタンス番号にする
pub fn renumbered(id: &str, ids: &HashMap<u64, f64>) -> String {
    id.split('>')
        .map(|part| match part.strip_prefix('#').and_then(|number| number.parse::<f64>().ok()) {
            Some(number) => format!("#{}", ids.get(&number.to_bits()).copied().unwrap_or(f64::NAN)),
            None => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join(">")
}

/// 図形のレイヤの名前。定義のないレイヤコード（「未定義のレイヤ n」）は P21 に書けない（レイヤは名前を持つ割り当て）ので除く
fn layer_names(scene: &Scene, entity: &SceneEntity) -> Vec<String> {
    entity
        .layers
        .iter()
        .map(|id| {
            scene.layers.iter().find(|layer| layer.id == *id).map_or_else(String::new, |layer| layer.name.clone())
        })
        .filter(|name| !name.starts_with("未定義のレイヤ"))
        .collect()
}

/// 図形ごとに、種類・形（描く要素と外接矩形）・レイヤの名前を比べる。ids があれば id も書いた番号にして比べる
pub fn compare_entities(
    before: &Scene,
    after: &Scene,
    ids: Option<&HashMap<u64, f64>>,
    tolerance: f64,
) -> Result<(), String> {
    if before.entities.len() != after.entities.len() {
        return Err(format!("図形の数 {} → {}", before.entities.len(), after.entities.len()));
    }
    for (a, b) in before.entities.iter().zip(&after.entities) {
        if let Some(ids) = ids {
            let id = renumbered(&a.id, ids);
            if id != b.id {
                return Err(format!("id {} ({id}) → {}", a.id, b.id));
            }
        }
        if a.kind != b.kind || a.type_name != b.type_name {
            return Err(format!("{}: 種類 {:?} → {:?}", a.id, a.type_name, b.type_name));
        }
        let (pa, pb) = (serde_json::to_value(&a.primitives).unwrap(), serde_json::to_value(&b.primitives).unwrap());
        close_json(&pa, &pb, tolerance, &format!("{} primitives", a.id))?;
        let (ba, bb) = (serde_json::to_value(a.bbox).unwrap(), serde_json::to_value(b.bbox).unwrap());
        close_json(&ba, &bb, tolerance, &format!("{} bbox", a.id))?;
        let (la, lb) = (layer_names(before, a), layer_names(after, b));
        if la != lb {
            return Err(format!("{}: レイヤ {la:?} → {lb:?}", a.id));
        }
    }
    Ok(())
}
