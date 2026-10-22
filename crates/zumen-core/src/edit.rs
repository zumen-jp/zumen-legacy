//! 図面の編集の操作（docs/decisions/0012）。形式によらない操作の形と値の検証で、形式ごとのその場の編集
//! （元のファイルを保ち、操作した図形だけを書き換える）が受け取る。
//!
//! - 種類は `EntityKind` の名前（line、polyline など）と、ブロック・部分図の配置の `block-reference`。
//! - 値のキーは `Property::key` と同じ（start、center、layer など）。編集にだけ使うキー（vertices など）は
//!   docs/decisions/0012 の表のとおり。
//! - 座標は図面自体の座標（図形が属する空間・ブロック・部分図・レイヤグループの座標）で、読み取りの property の
//!   raw と同じ数。用紙の mm ではない。
//!
//! 操作は JSON の配列で受け取る（`parse_operations`）。すべての操作を確かめてから書き、1 つでも通らなければ
//! 何も書かない。エラーは操作の番号（0 から）と、変わらない英語の code を持つ。

use crate::types::{EntityKind, Notice};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::fmt;

/// 配置（DXF・DWG の INSERT、SXF の部分図・複合図形の配置、JWW のブロック）の種類の名前
pub const BLOCK_REFERENCE: &str = "block-reference";

/// 座標・長さとして受け付ける数の絶対値の上限
const MAX_NUMBER: f64 = 1e12;

/// 操作の値
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    /// 数の並び（[x, y] や [開始, 終了] など）
    Numbers(Vec<f64>),
    /// 点の並び（[[x, y], ...]）
    Points(Vec<Vec<f64>>),
}

/// キーごとの値
pub type EditValues = BTreeMap<String, EditValue>;

/// 編集の操作
#[derive(Debug, Clone, PartialEq)]
pub enum EditOperation {
    /// 図形を足す。container は足す先（形式ごとの名前。省略は図面の主な空間）
    Add { kind: String, container: Option<String>, values: EditValues },
    /// 図形の値を変える
    Update { id: String, set: EditValues },
    /// 図形を消す
    Delete { id: String },
    /// レイヤを足す
    AddLayer { name: String, values: EditValues },
    /// レイヤの値を変える
    UpdateLayer { name: String, set: EditValues },
    /// 空のブロック定義を足す（docs/decisions/0017）。中の図形は後の add が container の block:<名前> で足す
    AddBlock { name: String, values: EditValues },
}

impl EditOperation {
    /// JSON の op の名前
    pub fn name(&self) -> &'static str {
        match self {
            EditOperation::Add { .. } => "add",
            EditOperation::Update { .. } => "update",
            EditOperation::Delete { .. } => "delete",
            EditOperation::AddLayer { .. } => "add-layer",
            EditOperation::UpdateLayer { .. } => "update-layer",
            EditOperation::AddBlock { .. } => "add-block",
        }
    }
}

/// 編集した図面
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Edited {
    pub bytes: Vec<u8>,
    /// 足した図形の id（add の操作の順）
    pub created: Vec<String>,
    /// 形を変えて書いた・書かなかった内容
    pub notices: Vec<Notice>,
}

/// 編集できなかった理由。code は変わらない英語の名前、message は人に見せる文
#[derive(Debug, Clone, PartialEq)]
pub struct EditError {
    pub code: &'static str,
    pub message: String,
    /// 理由になった操作の番号（0 から）。図面全体の理由なら None
    pub operation: Option<usize>,
}

impl EditError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), operation: None }
    }

    pub fn at(operation: usize, code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), operation: Some(operation) }
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.operation {
            Some(index) => write!(f, "操作 {index}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for EditError {}

/// 色の指定
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditColor {
    ByLayer,
    ByBlock,
    /// 形式の色の番号（DXF・DWG の ACI 1〜255、SXF の既定の色の番号、JWW の線色の番号）
    Index(u32),
    /// 0xRRGGBB
    Rgb(u32),
}

/// 線の太さの指定
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditLineWidth {
    ByLayer,
    ByBlock,
    /// 形式の既定の太さ
    Default,
    /// mm
    Millimeters(f64),
}

/// JSON の配列を操作にする
pub fn parse_operations(json: &str) -> Result<Vec<EditOperation>, EditError> {
    let value: Json = serde_json::from_str(json)
        .map_err(|error| EditError::new("invalid-operations", format!("操作の JSON を読めません: {error}")))?;
    operations_from_json(&value)
}

/// JSON の値（配列）を操作にする
pub fn operations_from_json(value: &Json) -> Result<Vec<EditOperation>, EditError> {
    let Json::Array(items) = value else {
        return Err(EditError::new("invalid-operations", "操作は配列で指定してください"));
    };
    items.iter().enumerate().map(|(index, item)| operation(index, item)).collect()
}

fn operation(index: usize, item: &Json) -> Result<EditOperation, EditError> {
    let Json::Object(object) = item else {
        return Err(EditError::at(index, "invalid-operations", "操作はオブジェクトで指定してください"));
    };
    let string = |key: &str| -> Result<String, EditError> {
        match object.get(key) {
            Some(Json::String(text)) if !text.is_empty() => Ok(text.clone()),
            Some(_) => {
                Err(EditError::at(index, "invalid-operations", format!("{key} は空でない文字列で指定してください")))
            }
            None => Err(EditError::at(index, "invalid-operations", format!("{key} がありません"))),
        }
    };
    let values = |skip: &[&str]| -> Result<EditValues, EditError> {
        object
            .iter()
            .filter(|(key, _)| !skip.contains(&key.as_str()))
            .map(|(key, value)| Ok((key.clone(), edit_value(index, key, value)?)))
            .collect()
    };
    let set = || -> Result<EditValues, EditError> {
        match object.get("set") {
            Some(Json::Object(set)) => {
                set.iter().map(|(key, value)| Ok((key.clone(), edit_value(index, key, value)?))).collect()
            }
            Some(_) => Err(EditError::at(index, "invalid-operations", "set はオブジェクトで指定してください")),
            None => Err(EditError::at(index, "invalid-operations", "set がありません")),
        }
    };
    let only = |keys: &[&str]| -> Result<(), EditError> {
        match object.keys().find(|key| !keys.contains(&key.as_str())) {
            Some(key) => Err(EditError::at(
                index,
                "unknown-key",
                format!("{} の操作に {key} は指定できません（指定できるのは {}）", op_name(object), keys.join("、")),
            )),
            None => Ok(()),
        }
    };
    let op = match object.get("op") {
        Some(Json::String(op)) => op.as_str(),
        _ => {
            return Err(EditError::at(
                index,
                "invalid-operations",
                "op（add・update・delete・add-layer・update-layer・add-block）がありません",
            ));
        }
    };
    Ok(match op {
        "add" => {
            let kind = string("kind")?;
            if kind != BLOCK_REFERENCE && !EntityKind::ALL.iter().any(|known| known.name() == kind) {
                return Err(EditError::at(index, "unknown-kind", format!("図形の種類 {kind} はありません")));
            }
            let container = match object.get("container") {
                None | Some(Json::Null) => None,
                Some(Json::String(text)) if !text.is_empty() => Some(text.clone()),
                Some(_) => {
                    return Err(EditError::at(index, "invalid-operations", "container は文字列で指定してください"));
                }
            };
            EditOperation::Add { kind, container, values: values(&["op", "kind", "container"])? }
        }
        "update" => {
            only(&["op", "id", "set"])?;
            EditOperation::Update { id: string("id")?, set: set()? }
        }
        "delete" => {
            only(&["op", "id"])?;
            EditOperation::Delete { id: string("id")? }
        }
        "add-layer" => EditOperation::AddLayer { name: string("name")?, values: values(&["op", "name"])? },
        "update-layer" => {
            only(&["op", "name", "set"])?;
            EditOperation::UpdateLayer { name: string("name")?, set: set()? }
        }
        "add-block" => EditOperation::AddBlock { name: string("name")?, values: values(&["op", "name"])? },
        other => {
            return Err(EditError::at(
                index,
                "invalid-operations",
                format!("op の {other} はありません（add・update・delete・add-layer・update-layer・add-block）"),
            ));
        }
    })
}

fn op_name(object: &serde_json::Map<String, Json>) -> &str {
    object.get("op").and_then(Json::as_str).unwrap_or("")
}

fn edit_value(index: usize, key: &str, value: &Json) -> Result<EditValue, EditError> {
    let invalid = || EditError::at(index, "invalid-value", format!("{key} の値の形が違います"));
    let number = |value: &Json| -> Result<f64, EditError> {
        let number = value.as_f64().ok_or_else(invalid)?;
        if !number.is_finite() || number.abs() > MAX_NUMBER {
            return Err(EditError::at(index, "invalid-value", format!("{key} の数が大きすぎます")));
        }
        Ok(number)
    };
    Ok(match value {
        Json::Null => EditValue::Null,
        Json::Bool(value) => EditValue::Bool(*value),
        Json::Number(_) => EditValue::Number(number(value)?),
        Json::String(text) => EditValue::Text(text.clone()),
        Json::Array(items) if items.iter().all(Json::is_array) && !items.is_empty() => EditValue::Points(
            items
                .iter()
                .map(|item| item.as_array().ok_or_else(invalid)?.iter().map(number).collect::<Result<Vec<_>, _>>())
                .collect::<Result<_, _>>()?,
        ),
        Json::Array(items) => EditValue::Numbers(items.iter().map(number).collect::<Result<_, _>>()?),
        Json::Object(_) => return Err(invalid()),
    })
}

/// 操作の値を読む。形式ごとの編集が、受け付けるキーを確かめ、値を型ごとに取り出すのに使う
pub struct ValueReader<'a> {
    values: &'a EditValues,
    operation: usize,
}

impl<'a> ValueReader<'a> {
    pub fn new(values: &'a EditValues, operation: usize) -> Self {
        Self { values, operation }
    }

    fn error(&self, code: &'static str, message: String) -> EditError {
        EditError::at(self.operation, code, message)
    }

    fn invalid(&self, key: &str, expected: &str) -> EditError {
        self.error("invalid-value", format!("{key} は{expected}で指定してください"))
    }

    /// 指定できるキーのほかがあればエラーにする
    pub fn allow(&self, keys: &[&str], what: &str) -> Result<(), EditError> {
        for (key, value) in self.values {
            let valid = |number: &f64| number.is_finite() && number.abs() <= MAX_NUMBER;
            let valid = match value {
                EditValue::Number(number) => valid(number),
                EditValue::Numbers(numbers) => numbers.iter().all(valid),
                EditValue::Points(points) => points.iter().flatten().all(valid),
                _ => true,
            };
            if !valid {
                return Err(self.invalid(key, "有限で大きすぎない数"));
            }
        }
        match self.values.keys().find(|key| !keys.contains(&key.as_str())) {
            Some(key) => Err(self.error(
                "unknown-key",
                format!("{what}に {key} は指定できません（指定できるのは {}）", keys.join("、")),
            )),
            None => Ok(()),
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// 値そのもの（形で読み方が変わる値に使う）。null は指定がないのと同じ
    pub fn value(&self, key: &str) -> Option<&'a EditValue> {
        self.get(key)
    }

    /// 指定がない（null も同じ）
    fn get(&self, key: &str) -> Option<&'a EditValue> {
        self.values.get(key).filter(|value| **value != EditValue::Null)
    }

    pub fn required<T>(&self, key: &str, value: Option<T>) -> Result<T, EditError> {
        value.ok_or_else(|| self.error("invalid-value", format!("{key} がありません")))
    }

    pub fn number(&self, key: &str) -> Result<Option<f64>, EditError> {
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Number(value)) => Ok(Some(*value)),
            Some(_) => Err(self.invalid(key, "数")),
        }
    }

    /// 0 より大きい数
    pub fn positive(&self, key: &str) -> Result<Option<f64>, EditError> {
        match self.number(key)? {
            Some(value) if value <= 0.0 => Err(self.invalid(key, "0 より大きい数")),
            value => Ok(value),
        }
    }

    pub fn text(&self, key: &str) -> Result<Option<&'a str>, EditError> {
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Text(text)) => Ok(Some(text)),
            Some(_) => Err(self.invalid(key, "文字列")),
        }
    }

    /// 空でない名前（レイヤ・線種・ブロックなど）
    pub fn name(&self, key: &str) -> Result<Option<&'a str>, EditError> {
        match self.text(key)? {
            Some(text) if text.trim().is_empty() => Err(self.invalid(key, "空でない文字列")),
            value => Ok(value),
        }
    }

    pub fn bool(&self, key: &str) -> Result<Option<bool>, EditError> {
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Bool(value)) => Ok(Some(*value)),
            Some(_) => Err(self.invalid(key, "true か false")),
        }
    }

    /// 決まった数の数の並び
    pub fn numbers(&self, key: &str, count: usize) -> Result<Option<Vec<f64>>, EditError> {
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Numbers(values)) if values.len() == count => Ok(Some(values.clone())),
            Some(_) => Err(self.invalid(key, &format!("{count} 個の数の配列"))),
        }
    }

    /// 点。[x, y] か [x, y, z]（z の省略は 0）
    pub fn point(&self, key: &str) -> Result<Option<[f64; 3]>, EditError> {
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Numbers(values)) => point(values).map(Some).ok_or_else(|| self.invalid(key, "[x, y]")),
            Some(_) => Err(self.invalid(key, "[x, y]")),
        }
    }

    /// 点の並び。[[x, y], ...]
    pub fn points(&self, key: &str, minimum: usize) -> Result<Option<Vec<[f64; 3]>>, EditError> {
        let expected = format!("{minimum} 点以上の [[x, y], ...]");
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Points(points)) if points.len() >= minimum => points
                .iter()
                .map(|values| point(values).ok_or_else(|| self.invalid(key, &expected)))
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(self.invalid(key, &expected)),
        }
    }

    /// 色。番号（形式の色の番号）、"#rrggbb"、"bylayer"、"byblock"
    pub fn color(&self, key: &str) -> Result<Option<EditColor>, EditError> {
        let expected = "色の番号、\"#rrggbb\"、\"bylayer\" か \"byblock\"";
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Number(value)) if *value >= 0.0 && value.fract() == 0.0 && *value <= 65535.0 => {
                Ok(Some(EditColor::Index(*value as u32)))
            }
            Some(EditValue::Text(text)) => match text.to_ascii_lowercase().as_str() {
                "bylayer" => Ok(Some(EditColor::ByLayer)),
                "byblock" => Ok(Some(EditColor::ByBlock)),
                lower => rgb(lower).map(|value| Some(EditColor::Rgb(value))).ok_or_else(|| self.invalid(key, expected)),
            },
            Some(_) => Err(self.invalid(key, expected)),
        }
    }

    /// 線の太さ。mm の数、"bylayer"、"byblock"、"default"
    pub fn line_width(&self, key: &str) -> Result<Option<EditLineWidth>, EditError> {
        let expected = "mm の数、\"bylayer\"、\"byblock\" か \"default\"";
        match self.get(key) {
            None => Ok(None),
            Some(EditValue::Number(value)) if *value >= 0.0 => Ok(Some(EditLineWidth::Millimeters(*value))),
            Some(EditValue::Text(text)) => match text.to_ascii_lowercase().as_str() {
                "bylayer" => Ok(Some(EditLineWidth::ByLayer)),
                "byblock" => Ok(Some(EditLineWidth::ByBlock)),
                "default" => Ok(Some(EditLineWidth::Default)),
                _ => Err(self.invalid(key, expected)),
            },
            Some(_) => Err(self.invalid(key, expected)),
        }
    }
}

fn point(values: &[f64]) -> Option<[f64; 3]> {
    match values {
        [x, y] => Some([*x, *y, 0.0]),
        [x, y, z] => Some([*x, *y, *z]),
        _ => None,
    }
}

/// "#rrggbb" を 0xRRGGBB にする
fn rgb(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_operation() {
        let operations = parse_operations(
            r##"[
                {"op": "add", "kind": "line", "layer": "A", "start": [0, 0], "end": [10, 0, 5]},
                {"op": "add", "kind": "block-reference", "container": "block:DOOR", "block": "WIN", "position": [1, 2]},
                {"op": "update", "id": "3F", "set": {"color": "#FF0000", "vertices": [[0, 0], [1, 1]], "closed": true}},
                {"op": "delete", "id": "40"},
                {"op": "add-layer", "name": "CHECK", "color": 1},
                {"op": "update-layer", "name": "CHECK", "set": {"visible": false, "lineType": null}},
                {"op": "add-block", "name": "DOOR", "basePoint": [10, 0]}
            ]"##,
        )
        .unwrap();
        assert_eq!(operations.len(), 7);
        let EditOperation::Add { kind, values, container } = &operations[0] else { panic!() };
        assert_eq!((kind.as_str(), container), ("line", &None));
        let reader = ValueReader::new(values, 0);
        assert_eq!(reader.point("end").unwrap(), Some([10.0, 0.0, 5.0]));
        assert_eq!(reader.point("start").unwrap(), Some([0.0, 0.0, 0.0]));
        assert_eq!(reader.name("layer").unwrap(), Some("A"));
        let EditOperation::Add { container, .. } = &operations[1] else { panic!() };
        assert_eq!(container.as_deref(), Some("block:DOOR"));
        let EditOperation::Update { set, .. } = &operations[2] else { panic!() };
        let reader = ValueReader::new(set, 2);
        assert_eq!(reader.color("color").unwrap(), Some(EditColor::Rgb(0xff0000)));
        assert_eq!(reader.points("vertices", 2).unwrap().unwrap().len(), 2);
        assert_eq!(reader.bool("closed").unwrap(), Some(true));
        let EditOperation::UpdateLayer { set, .. } = &operations[5] else { panic!() };
        // null は指定がないのと同じ
        assert_eq!(ValueReader::new(set, 5).name("lineType").unwrap(), None);
        let EditOperation::AddBlock { name, values } = &operations[6] else { panic!() };
        assert_eq!(name, "DOOR");
        assert_eq!(ValueReader::new(values, 6).point("basePoint").unwrap(), Some([10.0, 0.0, 0.0]));
    }

    #[test]
    fn reports_the_operation_that_fails() {
        let error = parse_operations(r#"[{"op": "delete", "id": "1"}, {"op": "add", "kind": "wall"}]"#).unwrap_err();
        assert_eq!((error.code, error.operation), ("unknown-kind", Some(1)));
        assert_eq!(error.to_string(), "操作 1: 図形の種類 wall はありません");
        let error = parse_operations(r#"[{"op": "update", "id": "1", "set": {}, "extra": 1}]"#).unwrap_err();
        assert_eq!(error.code, "unknown-key");
        let error = parse_operations(r#"[{"op": "add", "kind": "line", "start": [1e300, 0]}]"#).unwrap_err();
        assert_eq!(error.code, "invalid-value");
        let error = parse_operations(r#"{"op": "delete"}"#).unwrap_err();
        assert_eq!((error.code, error.operation), ("invalid-operations", None));
    }

    #[test]
    fn checks_the_shape_of_values() {
        let operations = parse_operations(
            r#"[{"op": "add", "kind": "circle", "center": [0], "radius": -1, "color": "red", "lineWidth": "thin"}]"#,
        )
        .unwrap();
        let EditOperation::Add { values, .. } = &operations[0] else { panic!() };
        let reader = ValueReader::new(values, 0);
        assert_eq!(reader.point("center").unwrap_err().code, "invalid-value");
        assert_eq!(reader.positive("radius").unwrap_err().code, "invalid-value");
        assert_eq!(reader.color("color").unwrap_err().code, "invalid-value");
        assert_eq!(reader.line_width("lineWidth").unwrap_err().code, "invalid-value");
        let error = reader.allow(&["center", "radius"], "円").unwrap_err();
        assert_eq!(error.code, "unknown-key");
        assert!(error.message.contains("color"), "{}", error.message);
    }
}
