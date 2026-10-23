//! 図面のマークアップ（赤入れ・雲マーク・メモ・指摘・写真の位置。`@zumenjs/core` の markup.ts と同じ JSON・検査・図形）。
//! 図面のファイルには書かず、図面の横に置く JSON の文書にする。点はビューの用紙の mm で、Y 軸は上向き
//! （docs/markup.md、docs/decisions/0021）。新しい版へは比べた結果で移す（`carry_view`）

use crate::compare::SceneDiff;
use crate::geometry::{IDENTITY, empty_box, extend_box, extend_box_with_points, transform_points};
use crate::js;
use crate::text::{TextLayoutInput, font_family_for, is_half_width, layout_text};
use crate::types::{BACKGROUND_COLOR, BBox, EntityKind, FillPrimitive, Primitive, Scene, SceneEntity, StrokePrimitive};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::fmt;

/// 文書の format の値
pub const MARKUP_FORMAT: &str = "zumen-markup";
/// 読み書きできる最も新しい版
pub const MARKUP_VERSION: u64 = 1;
/// 色を指定しない項目の、既定の色
pub const MARKUP_COLOR: &str = "#ff0000";
/// 雲マークの弧の弦の、既定の長さ（用紙の mm）
pub const MARKUP_ARC_LENGTH: f64 = 10.0;
/// メモの文字の、既定の高さ（用紙の mm）
pub const MARKUP_TEXT_HEIGHT: f64 = 3.5;
/// 画面で決まった大きさの部分の 1 px の、既定の大きさ（用紙の mm）。96 dpi の 1 CSS px
pub const MARKUP_PIXEL: f64 = 25.4 / 96.0;

/// 文書の JSON Schema（draft 2020-12）
pub const SCHEMA: &str = include_str!("../schema/markup.schema.json");

/// 用紙の mm の点 [x, y]
pub type MarkupPoint = [f64; 2];

macro_rules! keyword {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Option<Self> {
                match text {
                    $($text => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }
    };
}

keyword!(
    /// 項目の種類
    MarkupKind { Redline = "redline", Cloud = "cloud", Text = "text", Finding = "finding", Photo = "photo" }
);
keyword!(
    /// 指摘の状態。open は未対応、resolved は設計者が対応して照査者が確かめていないもの、closed は確かめたか取り下げたもの
    MarkupStatus { Open = "open", Resolved = "resolved", Closed = "closed" }
);
keyword!(
    /// 赤入れの線の矢印
    MarkupArrows { None = "none", Start = "start", End = "end", Both = "both" }
);
keyword!(
    /// 新しい版へ移したときの印。removed は指していた図形がすべて消えた、changed は変わったか一部が消えた
    CarriedState { Unchanged = "unchanged", Changed = "changed", Removed = "removed" }
);

/// マークアップを付けた図面。sha256 はファイルのバイトの SHA-256（小文字の 16 進）、format は Scene.format
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarkupDrawing {
    pub sha256: String,
    pub format: String,
    /// 知らないキー
    pub extra: Map<String, Value>,
}

/// 新しい版へ移した記録。from は移す前の図面の sha256
#[derive(Debug, Clone, PartialEq)]
pub struct MarkupCarried {
    pub from: String,
    pub state: CarriedState,
    /// 知らないキー
    pub extra: Map<String, Value>,
}

/// 種類ごとのキー
#[derive(Debug, Clone, PartialEq)]
pub enum MarkupBody {
    /// 線・折線・手書きの線・矩形。closed を省くと false、arrows を省くと none
    Redline { points: Vec<MarkupPoint>, closed: Option<bool>, arrows: Option<MarkupArrows> },
    /// 雲マーク。points は 3 点以上の閉じた外形、arc_length（弧の弦の長さ）を省くと 10 mm
    Cloud { points: Vec<MarkupPoint>, arc_length: Option<f64> },
    /// メモ。position は 1 行目の左下、本文は MarkupItem::text。height を省くと 3.5 mm、angle（度）を省くと 0
    Text { position: MarkupPoint, height: Option<f64>, angle: Option<f64> },
    /// 照査の指摘。position はピン、points は範囲、label はピンの上の短い文字
    Finding { position: MarkupPoint, status: MarkupStatus, points: Option<Vec<MarkupPoint>>, label: Option<String> },
    /// 写真を撮った位置。photo は利用側が解決する参照、direction はカメラの向き（度、+x から反時計回り）
    Photo { position: MarkupPoint, photo: String, direction: Option<f64>, taken_at: Option<String> },
}

/// 文書の項目
#[derive(Debug, Clone, PartialEq)]
pub struct MarkupItem {
    /// 文書の中で一意
    pub id: String,
    pub body: MarkupBody,
    /// 点の座標系であるビュー（Scene.view）。ビューが 1 つしかない形式では None
    pub view: Option<String>,
    /// 利用側の分類（小文字・数字・ハイフン）。色を選ぶ
    pub category: Option<String>,
    /// #rrggbb か #rrggbbaa。分類の色より優先する
    pub color: Option<String>,
    /// 線の太さ（用紙の mm）
    pub line_width: Option<f64>,
    pub text: Option<String>,
    /// 項目が指す図形の id
    pub entities: Option<Vec<String>>,
    pub author: Option<String>,
    /// RFC 3339 の日時
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub carried: Option<MarkupCarried>,
    /// 利用側のデータ
    pub data: Option<Map<String, Value>>,
    /// 知らないキー（ほかの種類のキーを含む）
    pub extra: Map<String, Value>,
}

impl MarkupItem {
    pub fn kind(&self) -> MarkupKind {
        match self.body {
            MarkupBody::Redline { .. } => MarkupKind::Redline,
            MarkupBody::Cloud { .. } => MarkupKind::Cloud,
            MarkupBody::Text { .. } => MarkupKind::Text,
            MarkupBody::Finding { .. } => MarkupKind::Finding,
            MarkupBody::Photo { .. } => MarkupKind::Photo,
        }
    }

    /// 位置（メモ・指摘・写真）
    pub fn position(&self) -> Option<MarkupPoint> {
        match &self.body {
            MarkupBody::Text { position, .. }
            | MarkupBody::Finding { position, .. }
            | MarkupBody::Photo { position, .. } => Some(*position),
            _ => None,
        }
    }

    /// 点の並び（赤入れ・雲マーク・指摘の範囲）
    pub fn points(&self) -> Option<&[MarkupPoint]> {
        match &self.body {
            MarkupBody::Redline { points, .. } | MarkupBody::Cloud { points, .. } => Some(points),
            MarkupBody::Finding { points, .. } => points.as_deref(),
            _ => None,
        }
    }

    /// 指摘の状態
    pub fn status(&self) -> Option<MarkupStatus> {
        match self.body {
            MarkupBody::Finding { status, .. } => Some(status),
            _ => None,
        }
    }

    /// すべての点を (dx, dy) だけ動かす
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let shift = |point: &mut MarkupPoint| {
            point[0] += dx;
            point[1] += dy;
        };
        match &mut self.body {
            MarkupBody::Redline { points, .. } | MarkupBody::Cloud { points, .. } => points.iter_mut().for_each(shift),
            MarkupBody::Text { position, .. } | MarkupBody::Photo { position, .. } => shift(position),
            MarkupBody::Finding { position, points, .. } => {
                shift(position);
                if let Some(points) = points {
                    points.iter_mut().for_each(shift);
                }
            }
        }
    }
}

/// マークアップの文書
#[derive(Debug, Clone, PartialEq)]
pub struct MarkupDocument {
    pub version: u64,
    pub drawing: MarkupDrawing,
    pub items: Vec<MarkupItem>,
    /// 利用側のデータ
    pub data: Option<Map<String, Value>>,
    /// 知らないキー
    pub extra: Map<String, Value>,
}

impl MarkupDocument {
    /// 図面に付ける、項目のない文書
    pub fn new(drawing: MarkupDrawing) -> Self {
        Self { version: MARKUP_VERSION, drawing, items: Vec::new(), data: None, extra: Map::new() }
    }

    /// JSON の文字列を検査して読む。知らないキーは extra に残す
    pub fn from_json(text: &str) -> Result<Self, MarkupError> {
        let value: Value = serde_json::from_str(text).map_err(|error| MarkupError {
            code: "json",
            item: None,
            id: None,
            key: None,
            message: format!("文書: JSON として読めません（{error}）"),
        })?;
        Self::from_value(&value)
    }

    /// 読んだ JSON の値を検査して読む。誤りの順と code は TypeScript の markupFromJson と同じ
    pub fn from_value(value: &Value) -> Result<Self, MarkupError> {
        read_document(value)
    }

    /// JSON の値にする
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("マークアップの文書は JSON にできる")
    }

    /// JSON の文字列にする。pretty なら 2 文字の字下げ
    pub fn to_json(&self, pretty: bool) -> String {
        if pretty {
            serde_json::to_string_pretty(self).expect("マークアップの文書は JSON にできる")
        } else {
            serde_json::to_string(self).expect("マークアップの文書は JSON にできる")
        }
    }

    /// ビュー view（None はビューを持たない項目）の項目
    pub fn items_in_view<'a>(&'a self, view: Option<&'a str>) -> impl Iterator<Item = &'a MarkupItem> + 'a {
        self.items.iter().filter(move |item| item.view.as_deref() == view)
    }
}

/// 図面のバイトの SHA-256 を、小文字の 16 進で返す（FIPS 180-4）。文書の drawing.sha256 に入れる
pub fn drawing_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let mut out = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

// ---- 誤り

/// 文書の検査で見つけた誤り。code は種類（json・format・version・missing・type・value・duplicate）、item は items の
/// 何番目か（文書のキーなら None）、id はその項目の id（読めなければ None）、key はキー（drawing.sha256 など）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkupError {
    pub code: &'static str,
    pub item: Option<usize>,
    pub id: Option<String>,
    pub key: Option<String>,
    pub message: String,
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for MarkupError {}

// ---- 検査

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_category(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_color(text: &str) -> bool {
    text.strip_prefix('#')
        .is_some_and(|hex| (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// RFC 3339 の date-time（5.6 節）。T と Z は小文字も認める（TypeScript の正規表現と同じ）
fn is_date_time(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |range: std::ops::Range<usize>| b.get(range).is_some_and(|part| part.iter().all(u8::is_ascii_digit));
    let two = |at: usize| -> Option<u32> {
        digits(at..at + 2).then(|| u32::from(b[at] - b'0') * 10 + u32::from(b[at + 1] - b'0'))
    };
    if b.len() < 20 || !digits(0..4) || b[4] != b'-' || b[7] != b'-' || b[13] != b':' || b[16] != b':' {
        return false;
    }
    let (Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (two(5), two(8), two(11), two(14), two(17))
    else {
        return false;
    };
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !matches!(b[10], b'T' | b't')
        || hour > 23
        || minute > 59
        || second > 60
    {
        return false;
    }
    let mut at = 19;
    if b[at] == b'.' {
        let start = at + 1;
        at = start;
        while at < b.len() && b[at].is_ascii_digit() {
            at += 1;
        }
        if at == start {
            return false;
        }
    }
    match &b[at..] {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', ..] if b.len() == at + 6 && b[at + 3] == b':' => {
            two(at + 1).is_some_and(|hours| hours <= 23) && two(at + 4).is_some_and(|minutes| minutes <= 59)
        }
        _ => false,
    }
}

/// 検査の途中の場所。誤りに項目とキーを付ける
struct Checker {
    item: Option<usize>,
    id: Option<String>,
}

type Checked<T> = Result<T, MarkupError>;

impl Checker {
    fn fail(&self, code: &'static str, key: Option<&str>, message: &str) -> MarkupError {
        let place = match (self.item, &self.id) {
            (Some(index), Some(id)) => format!("items[{index}]（{id}）の "),
            (Some(index), None) => format!("items[{index}] の "),
            _ => String::new(),
        };
        MarkupError {
            code,
            item: self.item,
            id: self.id.clone(),
            key: key.map(str::to_string),
            message: format!("{place}{}: {message}", key.unwrap_or("文書")),
        }
    }

    fn required<'a>(&self, object: &'a Map<String, Value>, key: &str, name: &str) -> Checked<&'a Value> {
        object.get(key).ok_or_else(|| self.fail("missing", Some(name), "ありません"))
    }

    fn string<'a>(&self, value: &'a Value, key: &str, non_empty: bool) -> Checked<&'a str> {
        let Value::String(text) = value else {
            return Err(self.fail("type", Some(key), "文字列ではありません"));
        };
        if non_empty && text.is_empty() {
            return Err(self.fail("value", Some(key), "空です"));
        }
        Ok(text)
    }

    fn pattern<'a>(&self, value: &'a Value, key: &str, valid: fn(&str) -> bool) -> Checked<&'a str> {
        let text = self.string(value, key, false)?;
        if !valid(text) {
            return Err(self.fail("value", Some(key), &format!("形が違います（{}）", Value::from(text))));
        }
        Ok(text)
    }

    fn number(&self, value: &Value, key: &str, positive: bool) -> Checked<f64> {
        let Some(number) = value.as_f64().filter(|_| value.is_number()) else {
            return Err(self.fail("type", Some(key), "数ではありません"));
        };
        if !number.is_finite() || (positive && number.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)) {
            return Err(self.fail("value", Some(key), &format!("使えない値です（{}）", js::number_to_string(number))));
        }
        Ok(number)
    }

    fn one_of<T: Copy>(&self, value: &Value, key: &str, parse: fn(&str) -> Option<T>) -> Checked<T> {
        let text = self.string(value, key, false)?;
        parse(text).ok_or_else(|| self.fail("value", Some(key), &format!("知らない値です（{text}）")))
    }

    fn object<'a>(&self, value: &'a Value, key: &str) -> Checked<&'a Map<String, Value>> {
        value.as_object().ok_or_else(|| self.fail("type", Some(key), "オブジェクトではありません"))
    }

    fn point(&self, value: &Value, key: &str) -> Checked<MarkupPoint> {
        let Some(list) = value.as_array().filter(|list| list.iter().all(Value::is_number)) else {
            return Err(self.fail("type", Some(key), "点は数の配列 [x, y] です"));
        };
        let numbers: Vec<f64> = list.iter().filter_map(Value::as_f64).collect();
        match numbers[..] {
            [x, y] if x.is_finite() && y.is_finite() => Ok([x, y]),
            _ => Err(self.fail("value", Some(key), "点は有限の 2 つの数 [x, y] です")),
        }
    }

    fn points(&self, value: &Value, key: &str, minimum: usize) -> Checked<Vec<MarkupPoint>> {
        let Some(list) = value.as_array() else {
            return Err(self.fail("type", Some(key), "点の配列ではありません"));
        };
        let points = list.iter().map(|point| self.point(point, key)).collect::<Checked<Vec<_>>>()?;
        if points.len() < minimum {
            return Err(self.fail("value", Some(key), &format!("{minimum} 点以上が要ります（{} 点）", points.len())));
        }
        Ok(points)
    }

    fn optional<'a, T>(
        &self,
        object: &'a Map<String, Value>,
        key: &str,
        read: impl FnOnce(&'a Value) -> Checked<T>,
    ) -> Checked<Option<T>> {
        object.get(key).map(read).transpose()
    }
}

/// 知らないキー（known にないキー）
fn unknown(object: &Map<String, Value>, known: &[&str]) -> Map<String, Value> {
    object
        .iter()
        .filter(|(key, _)| !known.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

const DOCUMENT_KEYS: &[&str] = &["format", "version", "drawing", "items", "data"];
const DRAWING_KEYS: &[&str] = &["sha256", "format"];
const CARRIED_KEYS: &[&str] = &["from", "state"];
const COMMON_KEYS: &[&str] = &[
    "id",
    "kind",
    "view",
    "category",
    "color",
    "lineWidth",
    "text",
    "entities",
    "author",
    "createdAt",
    "updatedAt",
    "carried",
    "data",
];

fn kind_keys(kind: MarkupKind) -> &'static [&'static str] {
    match kind {
        MarkupKind::Redline => &["points", "closed", "arrows"],
        MarkupKind::Cloud => &["points", "arcLength"],
        MarkupKind::Text => &["position", "height", "angle"],
        MarkupKind::Finding => &["position", "status", "points", "label"],
        MarkupKind::Photo => &["position", "photo", "direction", "takenAt"],
    }
}

fn read_document(value: &Value) -> Checked<MarkupDocument> {
    let mut check = Checker { item: None, id: None };
    let document = value
        .as_object()
        .filter(|object| object.get("format").and_then(Value::as_str) == Some(MARKUP_FORMAT))
        .ok_or_else(|| check.fail("format", Some("format"), &format!("{MARKUP_FORMAT} ではありません")))?;
    let version = check.number(check.required(document, "version", "version")?, "version", false)?;
    if version.fract() != 0.0 || version < 1.0 {
        return Err(check.fail(
            "value",
            Some("version"),
            &format!("使えない版です（{}）", js::number_to_string(version)),
        ));
    }
    if version > MARKUP_VERSION as f64 {
        return Err(check.fail(
            "version",
            Some("version"),
            &format!("版 {} は読めません（{MARKUP_VERSION} まで）", js::number_to_string(version)),
        ));
    }
    let drawing = check.object(check.required(document, "drawing", "drawing")?, "drawing")?;
    let sha256 = check.pattern(check.required(drawing, "sha256", "drawing.sha256")?, "drawing.sha256", is_sha256)?;
    let format = check.string(check.required(drawing, "format", "drawing.format")?, "drawing.format", true)?;
    let Some(items) = check.required(document, "items", "items")?.as_array() else {
        return Err(check.fail("type", Some("items"), "配列ではありません"));
    };
    let data = check.optional(document, "data", |value| check.object(value, "data"))?.cloned();
    let mut ids = HashSet::new();
    let mut read = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        check.item = Some(index);
        check.id = None;
        let Some(object) = item.as_object() else {
            return Err(check.fail("type", None, "項目がオブジェクトではありません"));
        };
        let id = check.string(check.required(object, "id", "id")?, "id", true)?;
        check.id = Some(id.to_string());
        if !ids.insert(id) {
            return Err(check.fail("duplicate", Some("id"), "同じ id の項目がほかにあります"));
        }
        let kind = check.one_of(check.required(object, "kind", "kind")?, "kind", MarkupKind::parse)?;
        read.push(read_item(&check, object, id, kind)?);
    }
    Ok(MarkupDocument {
        version: version as u64,
        drawing: MarkupDrawing {
            sha256: sha256.to_string(),
            format: format.to_string(),
            extra: unknown(drawing, DRAWING_KEYS),
        },
        items: read,
        data,
        extra: unknown(document, DOCUMENT_KEYS),
    })
}

/// 項目を読む。種類によらないキー、種類のキーの順に調べる
fn read_item(check: &Checker, item: &Map<String, Value>, id: &str, kind: MarkupKind) -> Checked<MarkupItem> {
    let text_of = |key: &'static str, non_empty: bool| {
        check.optional(item, key, move |value| check.string(value, key, non_empty).map(str::to_string))
    };
    let view = text_of("view", true)?;
    let category = check.optional(item, "category", |value| check.pattern(value, "category", is_category))?;
    let color = check.optional(item, "color", |value| check.pattern(value, "color", is_color))?;
    let line_width = check.optional(item, "lineWidth", |value| check.number(value, "lineWidth", true))?;
    let text = text_of("text", false)?;
    let entities = check.optional(item, "entities", |value| {
        let Some(list) = value.as_array() else {
            return Err(check.fail("type", Some("entities"), "配列ではありません"));
        };
        list.iter().map(|entity| check.string(entity, "entities", false).map(str::to_string)).collect()
    })?;
    let author = text_of("author", false)?;
    let created_at = check.optional(item, "createdAt", |value| check.pattern(value, "createdAt", is_date_time))?;
    let updated_at = check.optional(item, "updatedAt", |value| check.pattern(value, "updatedAt", is_date_time))?;
    let carried = check.optional(item, "carried", |value| {
        let carried = check.object(value, "carried")?;
        let from = check.pattern(check.required(carried, "from", "carried.from")?, "carried.from", is_sha256)?;
        let state =
            check.one_of(check.required(carried, "state", "carried.state")?, "carried.state", CarriedState::parse)?;
        Ok(MarkupCarried { from: from.to_string(), state, extra: unknown(carried, CARRIED_KEYS) })
    })?;
    let data = check.optional(item, "data", |value| check.object(value, "data"))?.cloned();
    let body = match kind {
        MarkupKind::Redline => {
            let points = check.points(check.required(item, "points", "points")?, "points", 2)?;
            let closed = check.optional(item, "closed", |value| {
                value.as_bool().ok_or_else(|| check.fail("type", Some("closed"), "真偽値ではありません"))
            })?;
            let arrows = check.optional(item, "arrows", |value| check.one_of(value, "arrows", MarkupArrows::parse))?;
            MarkupBody::Redline { points, closed, arrows }
        }
        MarkupKind::Cloud => {
            let points = check.points(check.required(item, "points", "points")?, "points", 3)?;
            let arc_length = check.optional(item, "arcLength", |value| check.number(value, "arcLength", true))?;
            MarkupBody::Cloud { points, arc_length }
        }
        MarkupKind::Text => {
            let position = check.point(check.required(item, "position", "position")?, "position")?;
            check.required(item, "text", "text")?;
            let height = check.optional(item, "height", |value| check.number(value, "height", true))?;
            let angle = check.optional(item, "angle", |value| check.number(value, "angle", false))?;
            MarkupBody::Text { position, height, angle }
        }
        MarkupKind::Finding => {
            let position = check.point(check.required(item, "position", "position")?, "position")?;
            let status = check.one_of(check.required(item, "status", "status")?, "status", MarkupStatus::parse)?;
            let points = check.optional(item, "points", |value| check.points(value, "points", 3))?;
            let label = text_of("label", false)?;
            MarkupBody::Finding { position, status, points, label }
        }
        MarkupKind::Photo => {
            let position = check.point(check.required(item, "position", "position")?, "position")?;
            let photo = check.string(check.required(item, "photo", "photo")?, "photo", false)?.to_string();
            let direction = check.optional(item, "direction", |value| check.number(value, "direction", false))?;
            let taken_at = check.optional(item, "takenAt", |value| check.pattern(value, "takenAt", is_date_time))?;
            MarkupBody::Photo { position, photo, direction, taken_at: taken_at.map(str::to_string) }
        }
    };
    let known: Vec<&str> = COMMON_KEYS.iter().chain(kind_keys(kind)).copied().collect();
    Ok(MarkupItem {
        id: id.to_string(),
        body,
        view,
        category: category.map(str::to_string),
        color: color.map(str::to_string),
        line_width,
        text,
        entities,
        author,
        created_at: created_at.map(str::to_string),
        updated_at: updated_at.map(str::to_string),
        carried,
        data,
        extra: unknown(item, &known),
    })
}

// ---- JSON に書く

/// 数。整数の値は整数で書く（JavaScript の JSON.stringify と同じく 20.0 を 20 と書く）
struct Num(f64);

impl Serialize for Num {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let value = self.0;
        if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
            serializer.serialize_i64(value as i64)
        } else {
            serializer.serialize_f64(value)
        }
    }
}

struct Point<'a>(&'a MarkupPoint);

impl Serialize for Point<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&Num(self.0[0]))?;
        seq.serialize_element(&Num(self.0[1]))?;
        seq.end()
    }
}

struct Points<'a>(&'a [MarkupPoint]);

impl Serialize for Points<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter().map(Point))
    }
}

fn extra_entries<M: SerializeMap>(map: &mut M, extra: &Map<String, Value>) -> Result<(), M::Error> {
    for (key, value) in extra {
        map.serialize_entry(key, value)?;
    }
    Ok(())
}

impl Serialize for MarkupDrawing {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("sha256", &self.sha256)?;
        map.serialize_entry("format", &self.format)?;
        extra_entries(&mut map, &self.extra)?;
        map.end()
    }
}

impl Serialize for MarkupCarried {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("from", &self.from)?;
        map.serialize_entry("state", &self.state)?;
        extra_entries(&mut map, &self.extra)?;
        map.end()
    }
}

impl Serialize for MarkupItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("kind", &self.kind())?;
        macro_rules! entry {
            ($key:literal, $value:expr) => {
                if let Some(value) = $value {
                    map.serialize_entry($key, value)?;
                }
            };
        }
        entry!("view", &self.view);
        match &self.body {
            MarkupBody::Redline { points, closed, arrows } => {
                map.serialize_entry("points", &Points(points))?;
                entry!("closed", closed);
                entry!("arrows", arrows);
            }
            MarkupBody::Cloud { points, arc_length } => {
                map.serialize_entry("points", &Points(points))?;
                entry!("arcLength", &arc_length.map(Num));
            }
            MarkupBody::Text { position, height, angle } => {
                map.serialize_entry("position", &Point(position))?;
                entry!("text", &self.text);
                entry!("height", &height.map(Num));
                entry!("angle", &angle.map(Num));
            }
            MarkupBody::Finding { position, status, points, label } => {
                map.serialize_entry("position", &Point(position))?;
                map.serialize_entry("status", status)?;
                entry!("points", &points.as_deref().map(Points));
                entry!("label", label);
            }
            MarkupBody::Photo { position, photo, direction, taken_at } => {
                map.serialize_entry("position", &Point(position))?;
                map.serialize_entry("photo", photo)?;
                entry!("direction", &direction.map(Num));
                entry!("takenAt", taken_at);
            }
        }
        entry!("category", &self.category);
        entry!("color", &self.color);
        entry!("lineWidth", &self.line_width.map(Num));
        if self.kind() != MarkupKind::Text {
            entry!("text", &self.text);
        }
        entry!("entities", &self.entities);
        entry!("author", &self.author);
        entry!("createdAt", &self.created_at);
        entry!("updatedAt", &self.updated_at);
        entry!("carried", &self.carried);
        entry!("data", &self.data);
        extra_entries(&mut map, &self.extra)?;
        map.end()
    }
}

impl Serialize for MarkupDocument {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("format", MARKUP_FORMAT)?;
        map.serialize_entry("version", &self.version)?;
        map.serialize_entry("drawing", &self.drawing)?;
        map.serialize_entry("items", &self.items)?;
        if let Some(data) = &self.data {
            map.serialize_entry("data", data)?;
        }
        extra_entries(&mut map, &self.extra)?;
        map.end()
    }
}

// ---- 描く図形（TypeScript の markupShapes と同じ規則。docs/markup.md）

/// 図形の作り方
#[derive(Debug, Clone, Copy, Default)]
pub struct ShapeOptions<'a> {
    /// 項目が color を持たないときの色（分類の色など）。#rrggbb か #rrggbbaa。None なら MARKUP_COLOR
    pub color: Option<&'a str>,
    /// 画面で決まった大きさに描く部分の 1 px の大きさ（用紙の mm）。None なら MARKUP_PIXEL
    pub pixel: Option<f64>,
}

/// lineWidth のない線の太さ（px）
const LINE_PX: f64 = 2.0;
/// 矢印の長さと、半分の幅（線の太さの倍数）
const ARROW_LENGTH: f64 = 6.0;
const ARROW_HALF_WIDTH: f64 = 2.0;
/// 雲マークの 1 つの弧の中心角（度）と分割数、1 つの雲の弧の数の上限
const CLOUD_SWEEP: f64 = 120.0;
const CLOUD_SEGMENTS: usize = 8;
const CLOUD_MAX_ARCS: f64 = 4096.0;
/// 円を描く多角形の頂点の数
const CIRCLE_SEGMENTS: usize = 32;
/// メモの行の間隔（文字の高さの倍数）
const LINE_PITCH: f64 = 1.5;
/// 指摘のピンの半径（px）と、番号の幅の上限（ピンの半径の倍数）
const PIN_RADIUS: f64 = 10.0;
const PIN_LABEL_WIDTH: f64 = 1.6;
/// 写真の印の半径、レンズの半径、向きの扇の半径と半分の角度（度）
const PHOTO_RADIUS: f64 = 6.0;
const PHOTO_LENS: f64 = 2.5;
const PHOTO_VIEW_RADIUS: f64 = 24.0;
const PHOTO_VIEW_ANGLE: f64 = 30.0;
/// 範囲・閉じた指摘・写真の向きを塗る不透明度
const AREA_OPACITY: f64 = 0.15;
const CLOSED_OPACITY: f64 = 0.4;
const VIEW_OPACITY: f64 = 0.3;

/// #rrggbb・#rrggbbaa の不透明度を opacity 倍にした #rrggbbaa。ほかの書き方の色はそのまま
fn with_opacity(color: &str, opacity: f64) -> String {
    if !is_color(color) {
        return color.to_string();
    }
    let alpha = if color.len() == 9 { u8::from_str_radix(&color[7..9], 16).unwrap_or(255) } else { 255 };
    format!("{}{:02x}", color[..7].to_lowercase(), js::round(f64::from(alpha) * opacity) as u32)
}

fn stroke(points: Vec<f64>, closed: bool, color: &str, width: f64) -> Primitive {
    Primitive::Stroke(StrokePrimitive {
        points,
        closed,
        segments: None,
        color: color.to_string(),
        width,
        dashes: vec![],
    })
}

fn fill(ring: Vec<f64>, color: &str) -> Primitive {
    Primitive::Fill(FillPrimitive { color: color.to_string(), rings: vec![ring] })
}

fn flatten(points: &[MarkupPoint]) -> Vec<f64> {
    points.iter().flat_map(|point| *point).collect()
}

/// 中心 (x, y)、半径 radius の円の頂点（+x から反時計回り）
fn circle(x: f64, y: f64, radius: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(CIRCLE_SEGMENTS * 2);
    for i in 0..CIRCLE_SEGMENTS {
        let angle = (2.0 * PI * i as f64) / CIRCLE_SEGMENTS as f64;
        out.push(x + radius * js::cos(angle));
        out.push(y + radius * js::sin(angle));
    }
    out
}

/// 線の端 points[end] の矢印。向きは、端と違う位置にある最も近い点から端へ
fn arrowhead(points: &[MarkupPoint], end: usize, step: isize, width: f64) -> Option<Vec<f64>> {
    let [tx, ty] = points[end];
    let mut i = end as isize + step;
    while i >= 0 && (i as usize) < points.len() {
        let [px, py] = points[i as usize];
        i += step;
        let length = js::hypot2(tx - px, ty - py);
        if length.is_nan() || length <= 0.0 {
            continue;
        }
        let dx = (tx - px) / length;
        let dy = (ty - py) / length;
        let bx = tx - dx * ARROW_LENGTH * width;
        let by = ty - dy * ARROW_LENGTH * width;
        let nx = -dy * ARROW_HALF_WIDTH * width;
        let ny = dx * ARROW_HALF_WIDTH * width;
        return Some(vec![tx, ty, bx + nx, by + ny, bx - nx, by - ny]);
    }
    None
}

/// 雲マークの外形。各辺を弧の数 round(辺の長さ / 弦の長さ)（1 以上）で等分し、その弦ごとに中心角 120 度の弧を外側へ
/// ふくらませて、8 つに分けた点でつなぐ。外側は外形の向き（面積の符号）で決める。弧の数が 4096 を超えるときは弦を長くする
fn cloud_outline(points: &[MarkupPoint], arc_length: f64) -> Vec<f64> {
    let n = points.len();
    let mut area = 0.0;
    let mut perimeter = 0.0;
    for i in 0..n {
        let [x1, y1] = points[i];
        let [x2, y2] = points[(i + 1) % n];
        area += x1 * y2 - x2 * y1;
        perimeter += js::hypot2(x2 - x1, y2 - y1);
    }
    let chord = js::max(arc_length, perimeter / CLOUD_MAX_ARCS);
    let ccw = area >= 0.0;
    // 反時計回りの外形なら弧も反時計回りに回る（中心は内側、弧は進む向きの右へふくらむ）
    let turn = ((if ccw { 1.0 } else { -1.0 }) * CLOUD_SWEEP * PI) / 180.0 / CLOUD_SEGMENTS as f64;
    let cos = js::cos(turn);
    let sin = js::sin(turn);
    let half = (CLOUD_SWEEP * PI) / 360.0;
    let mut out = Vec::new();
    for i in 0..n {
        let [x1, y1] = points[i];
        let [x2, y2] = points[(i + 1) % n];
        let length = js::hypot2(x2 - x1, y2 - y1);
        if length.is_nan() || length <= 0.0 {
            continue;
        }
        let arcs = js::max(1.0, js::round(length / chord));
        // 外側への単位ベクトル。弦の中点から内側へ、弦の半分 / tan(中心角 / 2) の点が弧の中心
        let ox = (if ccw { y2 - y1 } else { y1 - y2 }) / length;
        let oy = (if ccw { x1 - x2 } else { x2 - x1 }) / length;
        let offset = length / arcs / 2.0 / js::tan(half);
        let mut k = 0.0;
        while k < arcs {
            let ax = x1 + ((x2 - x1) * k) / arcs;
            let ay = y1 + ((y2 - y1) * k) / arcs;
            let bx = x1 + ((x2 - x1) * (k + 1.0)) / arcs;
            let by = y1 + ((y2 - y1) * (k + 1.0)) / arcs;
            let cx = (ax + bx) / 2.0 - ox * offset;
            let cy = (ay + by) / 2.0 - oy * offset;
            let mut vx = ax - cx;
            let mut vy = ay - cy;
            out.push(ax);
            out.push(ay);
            for _ in 1..CLOUD_SEGMENTS {
                let rx = vx * cos - vy * sin;
                let ry = vx * sin + vy * cos;
                vx = rx;
                vy = ry;
                out.push(cx + vx);
                out.push(cy + vy);
            }
            k += 1.0;
        }
    }
    out
}

/// 改行（\r\n・\r・\n）で分けた行
fn lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') { 2 } else { 1 };
                start = i;
            }
            b'\n' => {
                out.push(&text[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push(&text[start..]);
    out
}

fn text_input<'a>(
    text: &'a str,
    x: f64,
    y: f64,
    height: f64,
    angle: f64,
    anchor: f64,
    color: &str,
) -> TextLayoutInput<'a> {
    TextLayoutInput {
        text,
        x,
        y,
        height,
        width: 0.0,
        spacing: 0.0,
        angle,
        slant: 0.0,
        anchor,
        vertical: false,
        color: color.to_string(),
        font_family: font_family_for("").to_string(),
    }
}

/// 項目を描く図形（用紙の mm）。描く順に並ぶ。色は項目の color、options.color、MARKUP_COLOR の順に選ぶ
pub fn shapes(item: &MarkupItem, options: &ShapeOptions<'_>) -> Vec<Primitive> {
    let color = item.color.as_deref().or(options.color).unwrap_or(MARKUP_COLOR);
    let pixel = options.pixel.unwrap_or(MARKUP_PIXEL);
    let width = item.line_width.unwrap_or(LINE_PX * pixel);
    let mut out = Vec::new();
    match &item.body {
        MarkupBody::Redline { points, closed, arrows } => {
            let closed = closed.unwrap_or(false);
            out.push(stroke(flatten(points), closed, color, width));
            let arrows = if closed { MarkupArrows::None } else { arrows.unwrap_or(MarkupArrows::None) };
            if matches!(arrows, MarkupArrows::Start | MarkupArrows::Both)
                && !points.is_empty()
                && let Some(ring) = arrowhead(points, 0, 1, width)
            {
                out.push(fill(ring, color));
            }
            if matches!(arrows, MarkupArrows::End | MarkupArrows::Both)
                && !points.is_empty()
                && let Some(ring) = arrowhead(points, points.len() - 1, -1, width)
            {
                out.push(fill(ring, color));
            }
        }
        MarkupBody::Cloud { points, arc_length } => {
            out.push(stroke(cloud_outline(points, arc_length.unwrap_or(MARKUP_ARC_LENGTH)), true, color, width));
        }
        MarkupBody::Text { position, height, angle } => {
            let height = height.unwrap_or(MARKUP_TEXT_HEIGHT);
            let angle = angle.unwrap_or(0.0);
            let radians = (angle * PI) / 180.0;
            for (index, line) in lines(item.text.as_deref().unwrap_or("")).into_iter().enumerate() {
                // 2 行目からは、文字の下の向きへ行の間隔ずつ下げる
                let down = LINE_PITCH * height * index as f64;
                let x = position[0] + js::sin(radians) * down;
                let y = position[1] - js::cos(radians) * down;
                if let Some(text) = layout_text(text_input(line, x, y, height, angle, 1.0, color), &IDENTITY) {
                    out.push(Primitive::Text(text));
                }
            }
        }
        MarkupBody::Finding { position, status, points, label } => {
            if let Some(points) = points.as_deref().filter(|points| points.len() >= 3) {
                let ring = flatten(points);
                out.push(fill(ring.clone(), &with_opacity(color, AREA_OPACITY)));
                out.push(stroke(ring, true, color, width));
            }
            let [x, y] = *position;
            let radius = PIN_RADIUS * pixel;
            let pin = circle(x, y, radius);
            // 未対応は塗ったピン、対応済みは輪、確認済みは薄く塗ったピン
            match status {
                MarkupStatus::Resolved => {
                    out.push(fill(pin.clone(), BACKGROUND_COLOR));
                    out.push(stroke(pin, true, color, LINE_PX * pixel));
                }
                MarkupStatus::Closed => out.push(fill(pin, &with_opacity(color, CLOSED_OPACITY))),
                MarkupStatus::Open => out.push(fill(pin, color)),
            }
            if let Some(label) = label.as_deref().filter(|label| !label.is_empty()) {
                let units = label.chars().fold(0.0, |sum, c| sum + if is_half_width(c) { 0.5 } else { 1.0 });
                let height = if units > PIN_LABEL_WIDTH { (radius * PIN_LABEL_WIDTH) / units } else { radius };
                let label_color = if *status == MarkupStatus::Resolved { color } else { BACKGROUND_COLOR };
                if let Some(text) = layout_text(text_input(label, x, y, height, 0.0, 5.0, label_color), &IDENTITY) {
                    out.push(Primitive::Text(text));
                }
            }
        }
        MarkupBody::Photo { position, direction, .. } => {
            let [x, y] = *position;
            if let Some(direction) = *direction {
                // 向きの左右 30 度の扇
                let radius = PHOTO_VIEW_RADIUS * pixel;
                let mut ring = vec![x, y];
                for s in 0..=CLOUD_SEGMENTS {
                    let angle = ((direction - PHOTO_VIEW_ANGLE
                        + (2.0 * PHOTO_VIEW_ANGLE * s as f64) / CLOUD_SEGMENTS as f64)
                        * PI)
                        / 180.0;
                    ring.push(x + radius * js::cos(angle));
                    ring.push(y + radius * js::sin(angle));
                }
                out.push(fill(ring.clone(), &with_opacity(color, VIEW_OPACITY)));
                out.push(stroke(ring, true, color, pixel));
            }
            out.push(fill(circle(x, y, PHOTO_RADIUS * pixel), color));
            out.push(fill(circle(x, y, PHOTO_LENS * pixel), BACKGROUND_COLOR));
        }
    }
    out
}

/// 図形の点を外接矩形に加える。文字は文字範囲の四隅
fn extend_box_with_primitive(b: &mut BBox, primitive: &Primitive) {
    match primitive {
        Primitive::Stroke(stroke) => extend_box_with_points(b, &stroke.points),
        Primitive::Fill(fill) => fill.rings.iter().for_each(|ring| extend_box_with_points(b, ring)),
        Primitive::Clipped(clipped) => clipped.rings.iter().for_each(|ring| extend_box_with_points(b, ring)),
        Primitive::Dot(dot) => extend_box(b, dot.x, dot.y),
        Primitive::Text(text) => {
            let (w, h) = (text.box_width, text.box_height);
            extend_box_with_points(b, &transform_points(&text.matrix, &[0.0, 0.0, w, 0.0, w, h, 0.0, h]));
        }
    }
}

fn primitives_box(primitives: &[Primitive]) -> BBox {
    let mut b = empty_box();
    for primitive in primitives {
        extend_box_with_primitive(&mut b, primitive);
    }
    b
}

/// 項目の外接矩形（用紙の mm）。画面で決まった大きさに描く部分を含まない、点・雲の弧・メモの文字範囲の範囲で、ピンと
/// 写真は位置の 1 点になる。新しい版へ移すとき、変わった場所と重なるかをこれで調べる
pub fn markup_box(item: &MarkupItem) -> BBox {
    primitives_box(&shapes(item, &ShapeOptions { color: None, pixel: Some(0.0) }))
}

/// 描画（zumen-render）でマークアップを描く指定
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarkupStyle {
    /// 分類ごとの色（#rrggbb か #rrggbbaa）
    pub categories: HashMap<String, String>,
    /// 色も分類の色もない項目の色。None なら MARKUP_COLOR
    pub color: Option<String>,
}

/// 項目を描く図形を、図面に重ねて描く図形にする。id は `markup:<項目の id>`、レイヤはなく（いつも描く）、source は markup。
/// pixel は画面で決まった大きさに描く部分の 1 px（用紙の mm）
pub fn markup_entities<'a>(
    items: impl IntoIterator<Item = &'a MarkupItem>,
    style: &MarkupStyle,
    pixel: f64,
) -> Vec<SceneEntity> {
    items
        .into_iter()
        .map(|item| {
            let color = item
                .category
                .as_ref()
                .and_then(|category| style.categories.get(category))
                .map(String::as_str)
                .or(style.color.as_deref());
            let primitives = shapes(item, &ShapeOptions { color, pixel: Some(pixel) });
            let (kind, type_name) = match item.kind() {
                MarkupKind::Redline => (EntityKind::Polyline, "赤線"),
                MarkupKind::Cloud => (EntityKind::Polyline, "雲マーク"),
                MarkupKind::Text => (EntityKind::Text, "メモ"),
                MarkupKind::Finding => (EntityKind::Symbol, "指摘"),
                MarkupKind::Photo => (EntityKind::Symbol, "写真"),
            };
            SceneEntity {
                id: format!("markup:{}", item.id),
                kind,
                type_name: type_name.to_string(),
                source: "markup".to_string(),
                layers: Vec::new(),
                bbox: primitives_box(&primitives),
                primitives,
                properties: Vec::new(),
                placement: None,
                drawn: true,
            }
        })
        .collect()
}

// ---- 新しい版へ移す

fn overlaps(a: &BBox, b: &BBox) -> bool {
    a.min_x <= b.max_x && b.min_x <= a.max_x && a.min_y <= b.max_y && b.min_y <= a.max_y
}

/// 1 つのビューの項目を、前の版 before と新しい版 after を比べた結果 diff（`compare::compare_scenes`）で新しい版へ移す
/// （docs/decisions/0021 の Revisions）。項目を比べたずれの分だけ動かし、entities を新しい版の対応する図形（変わらない
/// 図形と変わった図形）の id に替えて、消えた図形と比べなかった図形の id を除き、carried を `{ from, state }` にする。
/// state は、指す図形がすべて消えたら removed、どれかが変わったか消えたか、外接矩形が変わった場所と重なれば changed、
/// それ以外は unchanged。ビューの id は替えない
pub fn carry_view(
    items: &[MarkupItem],
    before: &Scene,
    after: &Scene,
    diff: &SceneDiff,
    from: &str,
) -> Vec<MarkupItem> {
    let positions: HashMap<&str, usize> =
        before.entities.iter().enumerate().map(|(index, entity)| (entity.id.as_str(), index)).collect();
    let mut matches: HashMap<usize, (usize, bool)> = HashMap::new();
    for &(a, b) in &diff.unchanged {
        matches.insert(a, (b, false));
    }
    for &(a, b) in &diff.changed {
        matches.insert(a, (b, true));
    }
    items
        .iter()
        .map(|item| {
            let mut carried = item.clone();
            let mut kept = Vec::new();
            let mut changed = false;
            let mut lost = 0;
            for id in item.entities.iter().flatten() {
                match positions.get(id.as_str()).and_then(|index| matches.get(index)) {
                    Some(&(other, is_changed)) => {
                        kept.push(after.entities[other].id.clone());
                        changed |= is_changed;
                    }
                    None => lost += 1,
                }
            }
            let named = item.entities.as_ref().is_some_and(|entities| !entities.is_empty());
            let b = markup_box(item);
            let in_region = diff.regions.iter().any(|region| overlaps(&b, &region.bbox));
            let state = if named && kept.is_empty() {
                CarriedState::Removed
            } else if changed || lost > 0 || in_region {
                CarriedState::Changed
            } else {
                CarriedState::Unchanged
            };
            carried.translate(diff.offset.0, diff.offset.1);
            if item.entities.is_some() {
                carried.entities = Some(kept);
            }
            carried.carried = Some(MarkupCarried { from: from.to_string(), state, extra: Map::new() });
            carried
        })
        .collect()
}

/// 新しい版にビューがない項目。動かさず、carried を removed にする
pub fn carry_removed(items: &[MarkupItem], from: &str) -> Vec<MarkupItem> {
    items
        .iter()
        .map(|item| {
            let mut carried = item.clone();
            carried.carried =
                Some(MarkupCarried { from: from.to_string(), state: CarriedState::Removed, extra: Map::new() });
            carried
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::{CompareOptions, compare_scenes};
    use crate::types::{Background, SceneLayer};

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/../../tests/fixtures/markup/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// 数は 1e-9 の相対誤差まで同じとみなす（crates/zumen/tests/golden.rs と同じ規則）
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
            (Value::Array(e), Value::Array(a)) if e.len() == a.len() => {
                for (index, (e, a)) in e.iter().zip(a).enumerate() {
                    compare(&format!("{path}[{index}]"), e, a, out);
                }
            }
            (Value::Object(e), Value::Object(a)) => {
                let keys: std::collections::BTreeSet<&String> = e.keys().chain(a.keys()).collect();
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
    fn reads_every_kind_and_writes_unknown_keys_back() {
        let text = fixture("kinds.markup.json");
        let document = MarkupDocument::from_json(&text).unwrap();
        assert_eq!(document.items.len(), 15);
        let expected: Value = serde_json::from_str(&text).unwrap();
        let mut differences = Vec::new();
        compare("document", &expected, &document.to_value(), &mut differences);
        assert!(differences.is_empty(), "{differences:#?}");
        // 書いた文書を読み直しても同じ
        assert_eq!(MarkupDocument::from_json(&document.to_json(true)).unwrap(), document);
        assert!(document.to_json(false).contains("\"points\":[[20,20],[60,20],[60,20],[80,45]]"));
    }

    #[test]
    fn shapes_match_the_typescript_side() {
        let document = MarkupDocument::from_json(&fixture("kinds.markup.json")).unwrap();
        let expected: Vec<Value> = serde_json::from_str(&fixture("kinds.shapes.json")).unwrap();
        assert_eq!(expected.len(), 2);
        for run in &expected {
            let options = &run["options"];
            let color = options.get("color").and_then(Value::as_str);
            let pixel = options.get("pixel").and_then(Value::as_f64);
            let actual: Vec<Value> = document
                .items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "id": item.id,
                        "box": markup_box(item),
                        "shapes": shapes(item, &ShapeOptions { color, pixel }),
                    })
                })
                .collect();
            let mut differences = Vec::new();
            compare("items", &run["items"], &Value::Array(actual), &mut differences);
            assert!(differences.is_empty(), "{options}: {differences:#?}");
        }
    }

    #[test]
    fn refuses_documents_like_the_typescript_side() {
        let cases: Vec<Value> = serde_json::from_str(&fixture("errors.json")).unwrap();
        assert!(cases.len() > 40);
        for case in &cases {
            let name = case["name"].as_str().unwrap();
            let result = match case.get("text").and_then(Value::as_str) {
                Some(text) => MarkupDocument::from_json(text),
                None => MarkupDocument::from_value(&case["document"]),
            };
            let error = result.expect_err(name);
            assert_eq!(error.code, case["code"].as_str().unwrap(), "{name}: {error}");
            assert_eq!(error.item.map(|index| index as u64), case["item"].as_u64(), "{name}");
            assert_eq!(error.id.as_deref(), case["id"].as_str(), "{name}");
            assert_eq!(error.key.as_deref(), case["key"].as_str(), "{name}");
            assert!(!error.message.is_empty());
        }
        // double を超える数は、serde_json が JSON として読まない（TypeScript では点の値の誤り）
        let text = format!(
            "{{\"format\":\"zumen-markup\",\"version\":1,\"drawing\":{{\"sha256\":\"{}\",\"format\":\"sfc\"}},\"items\":[{{\"id\":\"a\",\"kind\":\"redline\",\"points\":[[0,0],[1e999,1]]}}]}}",
            "a".repeat(64)
        );
        assert_eq!(MarkupDocument::from_json(&text).unwrap_err().code, "json");
    }

    #[test]
    fn hashes_bytes_with_sha_256() {
        // FIPS 180-4 の例（"abc"）と、空のバイト列。@zumenjs/core の drawingHash と同じ
        assert_eq!(drawing_hash(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(drawing_hash(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn checks_rfc_3339_date_times() {
        for valid in
            ["2026-10-23T09:12:00Z", "2026-10-23t09:12:00.5z", "2026-12-31T23:59:60+09:00", "2026-01-01T00:00:00-00:00"]
        {
            assert!(is_date_time(valid), "{valid}");
        }
        for invalid in [
            "2026-10-23",
            "2026-10-23T09:12:00",
            "2026-10-23 09:12:00Z",
            "2026-00-01T00:00:00Z",
            "2026-01-32T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:00:00.Z",
            "2026-01-01T00:00:00+9:00",
            "2026-01-01T00:00:00+24:00",
            "2026-01-01T00:00:00Z ",
        ] {
            assert!(!is_date_time(invalid), "{invalid}");
        }
    }

    fn line(id: &str, points: [f64; 4]) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: EntityKind::Line,
            type_name: "線分".into(),
            source: "line".into(),
            layers: vec![0],
            primitives: vec![stroke(points.to_vec(), false, "#ffffff", 0.25)],
            bbox: BBox {
                min_x: points[0].min(points[2]),
                min_y: points[1].min(points[3]),
                max_x: points[0].max(points[2]),
                max_y: points[1].max(points[3]),
            },
            properties: vec![],
            placement: None,
            drawn: true,
        }
    }

    /// 文字 text の図形（比べるときのずれは、両方の版にある同じ文字の位置から求める）
    fn label(id: &str, text: &str, x: f64, y: f64) -> SceneEntity {
        let primitive = layout_text(text_input(text, x, y, 2.5, 0.0, 1.0, "#ffffff"), &IDENTITY).unwrap();
        let primitives = vec![Primitive::Text(primitive)];
        SceneEntity {
            id: id.into(),
            kind: EntityKind::Text,
            type_name: "文字".into(),
            source: "text".into(),
            layers: vec![0],
            bbox: primitives_box(&primitives),
            primitives,
            properties: vec![],
            placement: None,
            drawn: true,
        }
    }

    fn scene(entities: Vec<SceneEntity>) -> Scene {
        let mut bounds = empty_box();
        for entity in &entities {
            crate::geometry::union_box(&mut bounds, &entity.bbox);
        }
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: None,
            bounds,
            layers: vec![SceneLayer { id: 0, name: "0".into(), visible: true, group: None }],
            entities,
            underlays: vec![],
            notices: vec![],
            metadata: vec![],
            background: Background::Dark,
        }
    }

    fn item(json: Value) -> MarkupItem {
        let document = serde_json::json!({
            "format": "zumen-markup",
            "version": 1,
            "drawing": { "sha256": "a".repeat(64), "format": "test" },
            "items": [json],
        });
        MarkupDocument::from_value(&document).unwrap().items.remove(0)
    }

    #[test]
    fn carries_items_by_the_offset_and_marks_what_changed() {
        let walls = [
            line("w1", [0.0, 0.0, 100.0, 0.0]),
            line("w2", [100.0, 0.0, 100.0, 50.0]),
            line("w3", [0.0, 50.0, 100.0, 50.0]),
            line("w4", [200.0, 0.0, 200.0, 50.0]),
            line("note-a", [300.0, 0.0, 300.0, 10.0]),
        ];
        let labels = |dx: f64, dy: f64, prefix: &str| {
            [("A", 10.0, 20.0), ("B", 40.0, 30.0), ("C", 70.0, 10.0)]
                .map(|(text, x, y)| label(&format!("{prefix}{text}"), text, x + dx, y + dy))
        };
        let before = scene([walls.to_vec(), labels(0.0, 0.0, "t").to_vec()].concat());
        // 新しい版は全体が (5, 3) ずれ、w2 が 10 mm 延び、w4 が消え、id はすべて替わった
        let shift = |entity: &SceneEntity, id: &str, longer: f64| {
            let SceneEntity { primitives, .. } = entity;
            let Primitive::Stroke(stroke) = &primitives[0] else { unreachable!() };
            let p = &stroke.points;
            line(id, [p[0] + 5.0, p[1] + 3.0, p[2] + 5.0, p[3] + 3.0 + longer])
        };
        let after = scene(
            [
                vec![
                    shift(&walls[0], "n1", 0.0),
                    shift(&walls[1], "n2", 10.0),
                    shift(&walls[2], "n3", 0.0),
                    shift(&walls[4], "n5", 0.0),
                ],
                labels(5.0, 3.0, "u").to_vec(),
            ]
            .concat(),
        );
        let diff = compare_scenes(&before, &after, &CompareOptions { all_layers: true, ..CompareOptions::default() });
        assert_eq!(diff.offset, (5.0, 3.0));
        let items = vec![
            item(
                serde_json::json!({ "id": "same", "kind": "finding", "position": [50, 0], "status": "open", "entities": ["w1", "w3"] }),
            ),
            item(
                serde_json::json!({ "id": "moved", "kind": "finding", "position": [100, 25], "status": "open", "entities": ["w2"] }),
            ),
            item(
                serde_json::json!({ "id": "gone", "kind": "finding", "position": [200, 25], "status": "open", "entities": ["w4"] }),
            ),
            item(
                serde_json::json!({ "id": "half", "kind": "cloud", "points": [[190, -5], [210, -5], [210, 55]], "entities": ["w4", "w1"] }),
            ),
            item(serde_json::json!({ "id": "free", "kind": "redline", "points": [[290, 20], [310, 20]] })),
            item(serde_json::json!({ "id": "over", "kind": "photo", "position": [200, 25], "photo": "p" })),
        ];
        let carried = carry_view(&items, &before, &after, &diff, &"a".repeat(64));
        let states: Vec<(&str, &str)> =
            carried.iter().map(|item| (item.id.as_str(), item.carried.as_ref().unwrap().state.as_str())).collect();
        assert_eq!(
            states,
            [
                ("same", "unchanged"),
                ("moved", "changed"),
                ("gone", "removed"),
                ("half", "changed"),
                ("free", "unchanged"),
                ("over", "changed"),
            ]
        );
        assert_eq!(carried[0].entities.as_deref(), Some(&["n1".to_string(), "n3".to_string()][..]));
        assert_eq!(carried[0].position(), Some([55.0, 3.0]));
        assert_eq!(carried[1].entities.as_deref(), Some(&["n2".to_string()][..]));
        assert_eq!(carried[2].entities.as_deref(), Some(&[][..]));
        assert_eq!(carried[3].entities.as_deref(), Some(&["n1".to_string()][..]));
        assert_eq!(carried[4].points(), Some(&[[295.0, 23.0], [315.0, 23.0]][..]));
        assert_eq!(carried[4].entities, None);
        assert_eq!(carried[0].carried.as_ref().unwrap().from, "a".repeat(64));
        let removed = carry_removed(&items[..1], &"b".repeat(64));
        assert_eq!(removed[0].carried.as_ref().unwrap().state, CarriedState::Removed);
        assert_eq!(removed[0].position(), Some([50.0, 0.0]));
    }

    #[test]
    fn draws_items_as_scene_entities_in_category_colors() {
        let document = MarkupDocument::from_json(&fixture("kinds.markup.json")).unwrap();
        let style = MarkupStyle {
            categories: HashMap::from([("question".to_string(), "#0000ff".to_string())]),
            color: Some("#00ff00".into()),
        };
        let entities = markup_entities(document.items_in_view(Some("Layout1")), &style, 0.5);
        assert_eq!(entities.len(), 8);
        let color = |entity: &SceneEntity| match &entity.primitives[0] {
            Primitive::Stroke(stroke) => stroke.color.clone(),
            Primitive::Fill(fill) => fill.color.clone(),
            Primitive::Text(text) => text.color.clone(),
            _ => String::new(),
        };
        let by_id = |id: &str| entities.iter().find(|entity| entity.id == format!("markup:{id}")).unwrap();
        assert_eq!(color(by_id("cloud-clockwise")), "#0000ff");
        assert_eq!(color(by_id("redline-rectangle")), "#0055FF");
        assert_eq!(color(by_id("redline-arrows")), "#00ff00");
        assert_eq!(by_id("text-note").kind, EntityKind::Text);
        assert!(entities.iter().all(|entity| entity.layers.is_empty() && entity.drawn && entity.source == "markup"));
        assert_eq!(document.items_in_view(None).count(), 1);
    }
}
