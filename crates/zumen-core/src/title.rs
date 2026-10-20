//! 表題欄の項目（図面番号・改訂・図面名・工事名・縮尺・日付）を、図面の文字から推定して取り出す。
//! 形式に依らず、次の手がかりを確からしい順に使う:
//!
//! 1. 属性（DXF・DWG の ATTRIB）のタグ（DWGNO、REV、TITLE、図番 など）
//! 2. 項目名の文字（「図面番号」「図　面　名」「DWG. NO.」など）と、その右か下にある文字
//! 3. 項目名と値が 1 つの文字に入っているもの（「縮尺 1/100」「図面番号：A-101」）
//! 4. 用紙の名前（SXF の図面名）と、図面の情報（`Scene::metadata`）の同じ項目名（JWW の縮尺など）
//!
//! 同じ手がかりの候補が複数あれば（1 つの図面に表題欄が 2 つあるときなど）、用紙の右下に近いものを先にする。

use crate::extract::{EntityText, entity_text, hidden_layers, is_entity_visible};
use crate::types::{Scene, SceneEntity};
use unicode_normalization::UnicodeNormalization;

/// 表題欄の項目
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TitleKey {
    /// 図面番号（図番）
    Number,
    /// 改訂（版）
    Revision,
    /// 図面名
    Title,
    /// 工事名・業務名・件名
    Project,
    /// 縮尺
    Scale,
    /// 作成年月日
    Date,
}

impl TitleKey {
    pub const ALL: [TitleKey; 6] =
        [TitleKey::Number, TitleKey::Revision, TitleKey::Title, TitleKey::Project, TitleKey::Scale, TitleKey::Date];

    /// 英語の名前（number、revision、title、project、scale、date）
    pub fn name(self) -> &'static str {
        match self {
            TitleKey::Number => "number",
            TitleKey::Revision => "revision",
            TitleKey::Title => "title",
            TitleKey::Project => "project",
            TitleKey::Scale => "scale",
            TitleKey::Date => "date",
        }
    }
}

/// 値を見つけた手がかり
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TitleSource {
    /// 属性のタグ
    Attribute,
    /// 項目名の文字の右か下の文字
    Label,
    /// 項目名と値が 1 つの文字に入っているもの
    Inline,
    /// 用紙の名前
    Sheet,
    /// 図面の情報（`Scene::metadata`）
    Metadata,
}

impl TitleSource {
    pub fn name(self) -> &'static str {
        match self {
            TitleSource::Attribute => "attribute",
            TitleSource::Label => "label",
            TitleSource::Inline => "inline",
            TitleSource::Sheet => "sheet",
            TitleSource::Metadata => "metadata",
        }
    }
}

/// 表題欄の項目の値
#[derive(Debug, Clone, PartialEq)]
pub struct TitleField {
    pub key: TitleKey,
    pub value: String,
    /// 手がかりにした属性のタグか項目名の文字（用紙の名前なら「用紙」、図面の情報ならその項目名）
    pub label: String,
    pub source: TitleSource,
    /// 値の図形の id（用紙の名前と図面の情報なら None）
    pub entity: Option<String>,
    /// 項目名の図形の id（項目名を使ったときだけ）
    pub label_entity: Option<String>,
    /// 値の文字の基準点（用紙の mm）
    pub x: f64,
    pub y: f64,
}

/// 項目名と属性のタグ。照合する前に `fold` で揃える（全角・半角、大文字・小文字、空白と . : _ - ・ を除く）
const LABELS: &[(TitleKey, &[&str])] = &[
    (
        TitleKey::Number,
        &[
            "図面番号",
            "図番",
            "図面no",
            "図no",
            "dwgno",
            "dwgnum",
            "dwgnumber",
            "drawingno",
            "drawingnumber",
            "drawno",
            "zuban",
            "sheetno",
            "sheetnumber",
        ],
    ),
    (TitleKey::Revision, &["改訂", "改訂番号", "改訂記号", "改訂no", "rev", "revno", "revision", "版", "版数"]),
    (TitleKey::Title, &["図面名", "図名", "図面名称", "表題", "title", "dwgtitle", "drawingtitle", "sheettitle"]),
    (TitleKey::Project, &["工事名", "工事件名", "件名", "事業名", "業務名", "project", "projectname"]),
    (TitleKey::Scale, &["縮尺", "尺度", "scale"]),
    (TitleKey::Date, &["作成年月日", "年月日", "日付", "作成日", "date"]),
];

/// 照合のために揃えた文字
fn fold(text: &str) -> String {
    text.nfkc().filter(|c| !c.is_whitespace() && !is_mark(*c)).flat_map(char::to_lowercase).collect()
}

/// 項目名の中や後ろに置かれる記号
fn is_mark(c: char) -> bool {
    matches!(c, '.' | ':' | '_' | '-' | '・' | '･' | '=')
}

/// 揃えた文字が項目名か
fn label_key(folded: &str) -> Option<TitleKey> {
    LABELS.iter().find(|(_, names)| names.contains(&folded)).map(|(key, _)| *key)
}

/// 項目名の確からしさの順（`LABELS` で先に書いたものほど小さい）。同じ位置の候補を並べるのに使う
fn label_rank(key: TitleKey, label: &str) -> usize {
    let folded = fold(label);
    LABELS
        .iter()
        .find(|(k, _)| *k == key)
        .and_then(|(_, names)| names.iter().position(|name| *name == folded))
        .unwrap_or(usize::MAX)
}

/// SXF の作成年・作成月・作成日を 1 つの日付にする。年が 0 のもの（未設定）は使わない
fn sxf_date(scene: &Scene) -> Option<String> {
    let part = |key: &str| -> Option<u32> {
        scene.metadata.iter().find(|property| property.key == key)?.value.trim().parse().ok()
    };
    let (year, month, day) = (part("year")?, part("month")?, part("day")?);
    (year > 0 && (1..=12).contains(&month) && (1..=31).contains(&day)).then(|| format!("{year}年{month}月{day}日"))
}

/// 1 つの文字に項目名と値があるときに、空白ではなく記号の区切りを求める英語の項目名。
/// 「PROJECT INFORMATION」「TITLE SHEET」のような見出しを値と読まないため
const WORD_LABELS: &[&str] = &["project", "title", "date", "scale", "rev", "revision"];

/// 「縮尺 1/100」「DWG. NO. A-101」のように、項目名で始まり値が続く文字を、項目名と値に分ける。
/// 項目名の後には空白か記号が要る（「版下」を版と「下」に分けない）。ただし 2 文字以上の項目名に
/// 英数字が続くときは区切りがなくてもよい（「縮尺1/100」「図番A-101」）。英語の 1 語の項目名
/// （`WORD_LABELS`）には、空白ではなく記号の区切りが要る（「SCALE: 1/4"」は読み、「PROJECT INFORMATION」は読まない）
fn split_inline(text: &str) -> Option<(TitleKey, String, String)> {
    let mut folded = String::new();
    let mut best: Option<(TitleKey, usize, usize)> = None;
    for (at, c) in text.char_indices() {
        let next = at + c.len_utf8();
        let piece: String = c.to_string().nfkc().collect();
        let before = folded.len();
        for c in piece.chars() {
            if !c.is_whitespace() && !is_mark(c) {
                folded.extend(c.to_lowercase());
            }
        }
        // 項目名は、それを終える文字までにする（後ろの空白や記号は区切りとして残す）
        if folded.len() > before
            && let Some(key) = label_key(&folded)
        {
            best = Some((key, next, folded.chars().count()));
        }
        if folded.chars().count() > 16 {
            break;
        }
    }
    let (key, end, length) = best?;
    let rest = &text[end..];
    let first = rest.chars().next()?;
    let label = fold(&text[..end]);
    if WORD_LABELS.contains(&label.as_str()) {
        let after = rest.trim_start();
        if !after.starts_with(|c: char| is_mark(c) || matches!(c, '：' | '＝')) {
            return None;
        }
    }
    let separated = first.is_whitespace() || is_mark(first) || matches!(first, '：' | '＝');
    let alphanumeric = first.to_string().nfkc().next().is_some_and(|c| c.is_ascii_alphanumeric());
    if !separated && !(length >= 2 && alphanumeric) {
        return None;
    }
    let value = rest.trim_start_matches(|c: char| c.is_whitespace() || is_mark(c) || matches!(c, '：' | '＝')).trim();
    if value.is_empty() {
        return None;
    }
    Some((key, text[..end].trim().to_string(), value.to_string()))
}

/// 値の候補になる文字
struct Text<'a> {
    entity: &'a SceneEntity,
    text: EntityText,
    /// 揃えた文字が項目名なら、その項目
    label: Option<TitleKey>,
}

/// 文字の向きに沿った幅（外接矩形を文字の向きに投影した長さ）
fn width_along(text: &EntityText) -> f64 {
    let (dx, dy) = (text.angle.to_radians().cos(), text.angle.to_radians().sin());
    let b = &text.bbox;
    let projections = [(b.min_x, b.min_y), (b.max_x, b.min_y), (b.max_x, b.max_y), (b.min_x, b.max_y)]
        .map(|(x, y)| (x - text.x) * dx + (y - text.y) * dy);
    projections.iter().fold(0.0_f64, |m, &p| m.max(p)) - projections.iter().fold(0.0_f64, |m, &p| m.min(p))
}

/// 項目名の文字の右（同じ行）か下にある、項目名でない文字。右を先に、近い順に選ぶ
fn value_of<'a>(label: &Text<'_>, texts: &'a [Text<'a>]) -> Option<&'a Text<'a>> {
    let l = &label.text;
    let (cos, sin) = (l.angle.to_radians().cos(), l.angle.to_radians().sin());
    let width = width_along(l);
    let height = l.height.max(1e-9);
    let mut right: Option<(f64, &Text)> = None;
    let mut below: Option<(f64, &Text)> = None;
    for candidate in texts {
        if candidate.label.is_some() || std::ptr::eq(candidate.entity, label.entity) {
            continue;
        }
        let c = &candidate.text;
        let turn = ((c.angle - l.angle + 540.0) % 360.0 - 180.0).abs();
        if turn > 1.0 {
            continue;
        }
        // 項目名の文字の座標（右が文字の進む向き）に置き直す
        let (dx, dy) = (c.x - l.x, c.y - l.y);
        let u = dx * cos + dy * sin;
        let v = -dx * sin + dy * cos;
        let tall = height.max(c.height);
        if v.abs() <= 0.6 * tall && u >= width - 0.5 * height && u - width <= 25.0 * height {
            let gap = u - width;
            if right.is_none_or(|(best, _)| gap < best) {
                right = Some((gap, candidate));
            }
        } else if v <= -0.5 * height && v >= -3.5 * tall && u >= -width && u <= width {
            let gap = -v;
            if below.is_none_or(|(best, _)| gap < best) {
                below = Some((gap, candidate));
            }
        }
    }
    right.or(below).map(|(_, candidate)| candidate)
}

/// 表題欄の項目の候補。項目の順（図面番号、改訂、図面名、工事名、縮尺、日付）に、確からしい順に並べる
pub fn title_fields(scene: &Scene) -> Vec<TitleField> {
    let hidden = hidden_layers(scene, None);
    let texts: Vec<Text> = scene
        .entities
        .iter()
        .filter(|entity| is_entity_visible(entity, &hidden))
        .filter_map(|entity| {
            let text = entity_text(entity)?;
            let label = label_key(&fold(&text.text));
            Some(Text { entity, text, label })
        })
        .collect();
    let mut fields = Vec::new();
    for text in &texts {
        // 1. 属性のタグ
        if let Some(tag) = text.entity.properties.iter().find(|property| property.key == "tag")
            && let Some(key) = label_key(&fold(&tag.value))
        {
            fields.push(TitleField {
                key,
                value: text.text.text.trim().to_string(),
                label: tag.value.clone(),
                source: TitleSource::Attribute,
                entity: Some(text.entity.id.clone()),
                label_entity: None,
                x: text.text.x,
                y: text.text.y,
            });
            continue;
        }
        // 2. 項目名の文字と、その右か下の文字
        if let Some(key) = text.label {
            if let Some(value) = value_of(text, &texts) {
                fields.push(TitleField {
                    key,
                    value: value.text.text.trim().to_string(),
                    label: text.text.text.trim().to_string(),
                    source: TitleSource::Label,
                    entity: Some(value.entity.id.clone()),
                    label_entity: Some(text.entity.id.clone()),
                    x: value.text.x,
                    y: value.text.y,
                });
            }
            continue;
        }
        // 3. 項目名と値が 1 つの文字にあるもの（1 行目だけを見る）
        let first_line = text.text.text.lines().next().unwrap_or("");
        if let Some((key, label, value)) = split_inline(first_line) {
            fields.push(TitleField {
                key,
                value,
                label,
                source: TitleSource::Inline,
                entity: Some(text.entity.id.clone()),
                label_entity: None,
                x: text.text.x,
                y: text.text.y,
            });
        }
    }
    // 4. 用紙の名前
    if let Some(sheet) = &scene.sheet
        && !sheet.name.trim().is_empty()
    {
        fields.push(TitleField {
            key: TitleKey::Title,
            value: sheet.name.trim().to_string(),
            label: "用紙".into(),
            source: TitleSource::Sheet,
            entity: None,
            label_entity: None,
            x: 0.0,
            y: 0.0,
        });
    }
    // 4. 図面の情報の同じ項目名。SXF の作成年・作成月・作成日は 1 つの日付にまとめる
    if let Some(date) = sxf_date(scene) {
        fields.push(TitleField {
            key: TitleKey::Date,
            value: date,
            label: "作成年月日".into(),
            source: TitleSource::Metadata,
            entity: None,
            label_entity: None,
            x: 0.0,
            y: 0.0,
        });
    }
    for property in &scene.metadata {
        if matches!(property.key, "year" | "month" | "day") {
            continue;
        }
        if let Some(key) = label_key(&fold(&property.label))
            && !property.value.trim().is_empty()
        {
            fields.push(TitleField {
                key,
                value: property.value.trim().to_string(),
                label: property.label.clone(),
                source: TitleSource::Metadata,
                entity: None,
                label_entity: None,
                x: 0.0,
                y: 0.0,
            });
        }
    }
    // 用紙（なければ図面の範囲）の右下からの距離
    let (corner_x, corner_y) = match &scene.sheet {
        Some(sheet) => (sheet.width, 0.0),
        None => (scene.bounds.max_x, scene.bounds.min_y),
    };
    let distance = |field: &TitleField| (field.x - corner_x).hypot(field.y - corner_y);
    fields.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then(a.source.cmp(&b.source))
            .then(distance(a).total_cmp(&distance(b)))
            .then(label_rank(a.key, &a.label).cmp(&label_rank(b.key, &b.label)))
    });
    fields
}

/// 項目ごとに最も確からしい値（見つかった項目だけ、項目の順）
pub fn title_block(scene: &Scene) -> Vec<TitleField> {
    let mut best: Vec<TitleField> = Vec::new();
    for field in title_fields(scene) {
        if best.last().is_none_or(|last| last.key != field.key) {
            best.push(field);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::placement;
    use crate::types::{BBox, Background, Primitive, Property, SceneLayer, Sheet, TextPrimitive};

    fn text(id: &str, content: &str, x: f64, y: f64, angle: f64) -> SceneEntity {
        let height = 3.5;
        let width = content.chars().count() as f64 * height;
        let matrix = placement(x, y, angle, 1.0, 1.0);
        let mut bbox =
            BBox { min_x: f64::INFINITY, min_y: f64::INFINITY, max_x: f64::NEG_INFINITY, max_y: f64::NEG_INFINITY };
        for (u, v) in [(0.0, 0.0), (width, 0.0), (width, height), (0.0, height)] {
            let (px, py) = crate::geometry::apply(&matrix, u, v);
            crate::geometry::extend_box(&mut bbox, px, py);
        }
        SceneEntity {
            id: id.into(),
            kind: crate::EntityKind::Text,
            type_name: "文字".into(),
            source: "test".into(),
            layers: vec![0],
            primitives: vec![Primitive::Text(TextPrimitive {
                text: content.into(),
                color: "#000000".into(),
                font_family: "sans-serif".into(),
                height,
                scale_x: 1.0,
                cells: Vec::new(),
                box_width: width,
                box_height: height,
                matrix,
            })],
            bbox,
            properties: Vec::new(),
            placement: None,
        }
    }

    fn scene(entities: Vec<SceneEntity>, sheet_name: &str) -> Scene {
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: Some(Sheet { name: sheet_name.into(), size: "A1".into(), width: 841.0, height: 594.0 }),
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 841.0, max_y: 594.0 },
            layers: vec![SceneLayer { id: 0, name: "D-TTL-TXT".into(), visible: true, group: None }],
            entities,
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    fn value(fields: &[TitleField], key: TitleKey) -> Option<&str> {
        fields.iter().find(|field| field.key == key).map(|field| field.value.as_str())
    }

    #[test]
    fn reads_values_to_the_right_of_or_below_their_labels() {
        // 公共工事の表題欄の並び（項目名の右に値）
        let s = scene(
            vec![
                text("1", "工事名", 726.5, 73.3, 0.0),
                text("2", "○○道路詳細設計業務", 763.9, 73.2, 0.0),
                text("3", "図　面　名", 726.5, 63.3, 0.0),
                text("4", "小構造物図", 772.6, 63.2, 0.0),
                text("5", "縮尺", 728.2, 43.3, 0.0),
                text("6", "図示", 751.1, 43.2, 0.0),
                text("7", "図面番号", 774.7, 43.1, 0.0),
                text("8", "4 / 4", 801.6, 43.3, 0.0),
                // 項目名の下に値
                text("9", "改訂", 820.0, 30.0, 0.0),
                text("10", "B", 821.0, 25.0, 0.0),
            ],
            "図面-1",
        );
        let block = title_block(&s);
        assert_eq!(value(&block, TitleKey::Project), Some("○○道路詳細設計業務"));
        assert_eq!(value(&block, TitleKey::Title), Some("小構造物図"));
        assert_eq!(value(&block, TitleKey::Scale), Some("図示"));
        assert_eq!(value(&block, TitleKey::Number), Some("4 / 4"));
        assert_eq!(value(&block, TitleKey::Revision), Some("B"));
        assert_eq!(value(&block, TitleKey::Date), None);
        let number = block.iter().find(|field| field.key == TitleKey::Number).unwrap();
        assert_eq!(
            (number.source, number.entity.as_deref(), number.label_entity.as_deref()),
            (TitleSource::Label, Some("8"), Some("7"))
        );
        // 用紙の名前は、項目名から見つけた図面名より後の候補
        let titles: Vec<String> =
            title_fields(&s).into_iter().filter(|f| f.key == TitleKey::Title).map(|f| f.value).collect();
        assert_eq!(titles, ["小構造物図", "図面-1"]);
    }

    #[test]
    fn reads_attribute_tags_and_inline_labels() {
        let mut number = text("20>21", "A-101", 700.0, 20.0, 0.0);
        number.properties.push(Property::new("tag", "タグ", "DWG_NO"));
        let mut revision = text("20>22", "3", 760.0, 20.0, 0.0);
        revision.properties.push(Property::new("tag", "タグ", "REV"));
        // 部品番号のタグ（NO）は図面番号ではない
        let mut part = text("30>31", "D-2", 100.0, 100.0, 0.0);
        part.properties.push(Property::new("tag", "タグ", "NO"));
        let s = scene(
            vec![
                number,
                revision,
                part,
                text("40", "縮尺 1/100", 600.0, 10.0, 0.0),
                text("41", "DWG. NO. X-9", 100.0, 500.0, 0.0),
                text("42", "版下の注記", 100.0, 400.0, 0.0),
                text("43", "日付2026.10.20", 650.0, 10.0, 0.0),
            ],
            "",
        );
        let fields = title_fields(&s);
        let block = title_block(&s);
        assert_eq!(value(&block, TitleKey::Number), Some("A-101"));
        assert_eq!(value(&block, TitleKey::Revision), Some("3"));
        assert_eq!(value(&block, TitleKey::Scale), Some("1/100"));
        assert_eq!(value(&block, TitleKey::Date), Some("2026.10.20"));
        // 属性の図面番号が先、文字の中の図面番号は後の候補
        let numbers: Vec<(&str, TitleSource)> =
            fields.iter().filter(|f| f.key == TitleKey::Number).map(|f| (f.value.as_str(), f.source)).collect();
        assert_eq!(numbers, [("A-101", TitleSource::Attribute), ("X-9", TitleSource::Inline)]);
        assert!(fields.iter().all(|f| f.value != "D-2" && !f.value.contains("下")));
    }

    #[test]
    fn follows_rotated_title_blocks_and_prefers_the_one_at_the_bottom_right() {
        // 90 度回した表題欄（値は項目名の「右」= 上）と、左上にある別の表題欄
        let s = scene(
            vec![
                text("1", "図面番号", 800.0, 50.0, 90.0),
                text("2", "S-12", 800.0, 70.0, 90.0),
                text("3", "図面番号", 50.0, 550.0, 0.0),
                text("4", "S-01", 70.0, 550.0, 0.0),
            ],
            "",
        );
        let numbers: Vec<String> =
            title_fields(&s).into_iter().filter(|f| f.key == TitleKey::Number).map(|f| f.value).collect();
        assert_eq!(numbers, ["S-12", "S-01"]);
        assert_eq!(fold("Ｄｗｇ．Ｎｏ．"), "dwgno");
        assert_eq!(split_inline("縮尺"), None);
        // 英語の 1 語の項目名は、記号の区切りがあるときだけ値と読む
        assert_eq!(
            split_inline("SCALE: 1/4\" = 1'-0\""),
            Some((TitleKey::Scale, "SCALE".into(), "1/4\" = 1'-0\"".into()))
        );
        assert_eq!(split_inline("PROJECT INFORMATION"), None);
        assert_eq!(split_inline("TITLE SHEET"), None);
        assert_eq!(split_inline("縮尺 1/100"), Some((TitleKey::Scale, "縮尺".into(), "1/100".into())));
    }

    #[test]
    fn falls_back_on_the_metadata_of_the_drawing() {
        let mut s = scene(Vec::new(), "");
        s.metadata = vec![Property::new("format", "形式", "JWW（版 600）"), Property::new("scale", "縮尺", "1/100")];
        let block = title_block(&s);
        assert_eq!(block.len(), 1);
        assert_eq!(
            (block[0].key, block[0].value.as_str(), block[0].source),
            (TitleKey::Scale, "1/100", TitleSource::Metadata)
        );
        // SXF の表題欄の属性。工事名を事業名より先にし、作成年・作成月・作成日は 1 つの日付にする
        s.metadata = [
            ("businessName", "事業名"),
            ("constructionName", "工事名"),
            ("drawingNumber", "図面番号"),
            ("scale", "尺度"),
            ("year", "作成年"),
            ("month", "作成月"),
            ("day", "作成日"),
        ]
        .into_iter()
        .zip(["道路改良事業", "工事", "No.1", "1:500", "2026", "10", "18"])
        .map(|((key, label), value)| Property::new(key, label, value))
        .collect();
        let block = title_block(&s);
        assert_eq!(value(&block, TitleKey::Project), Some("工事"));
        assert_eq!(value(&block, TitleKey::Number), Some("No.1"));
        assert_eq!(value(&block, TitleKey::Scale), Some("1:500"));
        assert_eq!(value(&block, TitleKey::Date), Some("2026年10月18日"));
        // 年が 0 なら日付はない
        s.metadata[4] = Property::new("year", "作成年", "0");
        assert_eq!(value(&title_block(&s), TitleKey::Date), None);
    }
}
