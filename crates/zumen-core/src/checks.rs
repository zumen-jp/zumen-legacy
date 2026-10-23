//! 図面が CAD 製図の規則（レイヤ名、色、線種、線の太さ、文字の高さ、使えない文字、用紙の大きさと向き、尺度）に
//! 合うかを、表示用モデルから調べる。
//!
//! 規則はデータ（JSON の規則集）で持ち、組み込みの規則集（`BUILT_IN`）も組織の規則集も同じ仕組みで調べる。
//! 結果は規則ごとに、調べた数、値がなく調べられなかった数と、規則に合わない値ごとの指摘（数と図形の id）を持つ。
//! どの形式でも、形式によらないキーの属性と、描く形（色・文字の高さ）だけを見る
//! （[決定 0015](../../../docs/decisions/0015-analysis-in-zumen-core.md)、[Checks](../../../docs/checks.md)）。

use crate::extract::layer_names;
use crate::geometry::apply_vector;
use crate::js;
use crate::types::{Primitive, PropertyRaw, Scene, SceneEntity, TextPrimitive};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 組み込みの規則集の名前
pub const BUILT_IN: [&str; 1] = ["mlit-cad-2025"];

/// 国土交通省 CAD製図基準（令和7年12月）
const MLIT_CAD_2025: &str = include_str!("../rules/mlit-cad-2025.json");

/// 線の太さ・文字の高さ・用紙の大きさを同じとみなす差（mm）の既定
const WIDTH_TOLERANCE: f64 = 0.001;
const HEIGHT_TOLERANCE: f64 = 0.01;
const SHEET_TOLERANCE: f64 = 1.0;

/// 指摘の重さ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckLevel {
    Error,
    Warning,
    Info,
}

impl CheckLevel {
    /// JSON での名前
    pub fn name(self) -> &'static str {
        match self {
            CheckLevel::Error => "error",
            CheckLevel::Warning => "warning",
            CheckLevel::Info => "info",
        }
    }
}

/// 規則の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckKind {
    LayerName,
    Color,
    LineType,
    LineWidth,
    TextHeight,
    Characters,
    SheetSize,
    Orientation,
    Scale,
}

impl CheckKind {
    /// JSON での名前
    pub fn name(self) -> &'static str {
        match self {
            CheckKind::LayerName => "layer-name",
            CheckKind::Color => "color",
            CheckKind::LineType => "line-type",
            CheckKind::LineWidth => "line-width",
            CheckKind::TextHeight => "text-height",
            CheckKind::Characters => "characters",
            CheckKind::SheetSize => "sheet-size",
            CheckKind::Orientation => "orientation",
            CheckKind::Scale => "scale",
        }
    }
}

/// 用紙の向き
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Landscape,
    Portrait,
}

/// 規則集の用紙の大きさ（mm）。向きは問わない
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetSize {
    pub name: String,
    pub width: f64,
    pub height: f64,
}

/// 規則集の 1 つの規則。kind ごとに使う項目が決まっている（`RuleSet::from_json` が確かめる）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    /// 規則集の中で重ならない名前
    pub id: String,
    pub kind: CheckKind,
    /// 指摘の重さ。省略すると warning
    #[serde(default = "warning")]
    pub level: CheckLevel,
    /// 基準の節の番号など
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    /// 規則の説明（人に見せる）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 図形の規則（色・線種・線の太さ・文字の高さ・使えない文字）を、この名前の型に合うレイヤの図形だけに使う
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<String>>,
    /// layer-name: レイヤ名の型（* は 0 文字以上、? は 1 文字、[SDCM] はどれか 1 文字、[!0-9] はそれ以外の 1 文字）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patterns: Option<Vec<String>>,
    /// layer-name: レイヤ名の長さの上限（半角 1、全角 2 で数える）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_width: Option<usize>,
    /// color: 使える色（#rrggbb）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<Vec<String>>,
    /// line-type: 使える線種の名前（大文字と小文字は区別しない）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    /// line-width: 使える線の太さ（mm）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widths: Option<Vec<f64>>,
    /// text-height: 使える文字の高さ（用紙の mm）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heights: Option<Vec<f64>>,
    /// characters: 使えない文字の符号位置の範囲（16 進。2116 か FF61-FF9F）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranges: Option<Vec<String>>,
    /// sheet-size: 使える用紙の大きさ
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<SheetSize>>,
    /// orientation: 用紙の向き
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,
    /// scale: 使える尺度 1:N の N
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scales: Option<Vec<f64>>,
    /// scale: N = m × 10 の整数乗として使える m
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mantissas: Option<Vec<f64>>,
    /// line-width・text-height・sheet-size: 同じとみなす差（mm）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance: Option<f64>,
}

fn warning() -> CheckLevel {
    CheckLevel::Warning
}

/// 規則集
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuleSet {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// 規則の出典（URL など）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// 組み込みの規則集。ない名前なら None
    pub fn built_in(name: &str) -> Option<RuleSet> {
        let json = match name {
            "mlit-cad-2025" => MLIT_CAD_2025,
            _ => return None,
        };
        Some(RuleSet::from_json(json).expect("組み込みの規則集は正しい"))
    }

    /// JSON の規則集を読み、規則の項目を確かめる
    pub fn from_json(json: &str) -> Result<RuleSet, String> {
        let rules: RuleSet = serde_json::from_str(json).map_err(|error| format!("規則集を読めません: {error}"))?;
        rules.validate()?;
        Ok(rules)
    }

    /// 組み込みの規則集の名前か JSON の規則集（{ で始まる）
    pub fn named_or_json(text: &str) -> Result<RuleSet, String> {
        if text.trim_start().starts_with('{') {
            return RuleSet::from_json(text);
        }
        RuleSet::built_in(text.trim())
            .ok_or_else(|| format!("規則集 {text} はありません（組み込みの規則集: {}）", BUILT_IN.join(", ")))
    }

    fn validate(&self) -> Result<(), String> {
        let mut ids = HashSet::new();
        for rule in &self.rules {
            if !ids.insert(rule.id.as_str()) {
                return Err(format!("規則 {} が重なっています", rule.id));
            }
            rule.validate().map_err(|error| format!("規則 {}: {error}", rule.id))?;
        }
        Ok(())
    }
}

impl Rule {
    /// kind が使う項目があり、ほかの kind の項目がないか
    fn validate(&self) -> Result<(), String> {
        let present = [
            ("patterns", self.patterns.is_some()),
            ("maxWidth", self.max_width.is_some()),
            ("colors", self.colors.is_some()),
            ("names", self.names.is_some()),
            ("widths", self.widths.is_some()),
            ("heights", self.heights.is_some()),
            ("ranges", self.ranges.is_some()),
            ("sizes", self.sizes.is_some()),
            ("orientation", self.orientation.is_some()),
            ("scales", self.scales.is_some()),
            ("mantissas", self.mantissas.is_some()),
            ("tolerance", self.tolerance.is_some()),
            ("layers", self.layers.is_some()),
        ];
        let (allowed, required): (&[&str], &[&str]) = match self.kind {
            CheckKind::LayerName => (&["patterns", "maxWidth"], &[]),
            CheckKind::Color => (&["colors", "layers"], &["colors"]),
            CheckKind::LineType => (&["names", "layers"], &["names"]),
            CheckKind::LineWidth => (&["widths", "tolerance", "layers"], &["widths"]),
            CheckKind::TextHeight => (&["heights", "tolerance", "layers"], &["heights"]),
            CheckKind::Characters => (&["ranges", "layers"], &["ranges"]),
            CheckKind::SheetSize => (&["sizes", "tolerance"], &["sizes"]),
            CheckKind::Orientation => (&["orientation"], &["orientation"]),
            CheckKind::Scale => (&["scales", "mantissas"], &[]),
        };
        for (key, has) in present {
            if has && !allowed.contains(&key) {
                return Err(format!("{key} は {} の規則では使いません", self.kind.name()));
            }
        }
        for key in required {
            if !present.iter().any(|(name, has)| name == key && *has) {
                return Err(format!("{} の規則には {key} が要ります", self.kind.name()));
            }
        }
        match self.kind {
            CheckKind::LayerName if self.patterns.is_none() && self.max_width.is_none() => {
                return Err("layer-name の規則には patterns か maxWidth が要ります".into());
            }
            CheckKind::Scale if self.scales.is_none() && self.mantissas.is_none() => {
                return Err("scale の規則には scales か mantissas が要ります".into());
            }
            _ => {}
        }
        for pattern in self.patterns.iter().chain(self.layers.iter()).flatten() {
            Glob::new(pattern)?;
        }
        for color in self.colors.iter().flatten() {
            if parse_color(color).is_none() {
                return Err(format!("色 {color} は #rrggbb ではありません"));
            }
        }
        for range in self.ranges.iter().flatten() {
            parse_range(range)?;
        }
        let numbers = [&self.widths, &self.heights, &self.scales, &self.mantissas];
        let sizes = self.sizes.iter().flatten().flat_map(|size| [size.width, size.height]);
        if numbers
            .into_iter()
            .flatten()
            .flatten()
            .copied()
            .chain(sizes)
            .chain(self.tolerance)
            .any(|n| !n.is_finite() || n < 0.0)
        {
            return Err("数は 0 以上にしてください".into());
        }
        if self.scales.iter().chain(self.mantissas.iter()).flatten().any(|n| *n == 0.0) {
            return Err("尺度は正の数にしてください".into());
        }
        Ok(())
    }
}

/// 規則に合わない 1 つの値と、それを持つもの
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    /// 合わない値（レイヤ名、#rrggbb、線種、mm の数、文字、用紙の大きさ、1:N）
    pub value: String,
    pub message: String,
    /// この値を持つ図形の数（レイヤ名と文字の規則は、そのレイヤの図形やその文字を含む文字の図形とレイヤの数。
    /// 用紙と尺度の規則は、図面や表題欄も 1 つに数える）
    pub count: usize,
    /// この値を持つ図形の id（図面に現れた順）
    pub entities: Vec<String>,
    /// この値を持つ図形のレイヤの名前と、名前がこの値のレイヤ
    pub layers: Vec<String>,
}

/// 1 つの規則の結果
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleResult {
    pub id: String,
    pub kind: CheckKind,
    pub level: CheckLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 調べたもの（図形、レイヤ、用紙、尺度）の数
    pub checked: usize,
    /// 値が図面になく調べられなかった数（DXF のレイヤの線の太さ、JWW の番号だけの線種、用紙のない図面の文字など）
    pub unchecked: usize,
    pub findings: Vec<Finding>,
}

/// 規則集で図面を調べた結果
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub rule_set: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub results: Vec<RuleResult>,
}

impl CheckReport {
    /// この重さの指摘の数（値の数）
    pub fn count(&self, level: CheckLevel) -> usize {
        self.results.iter().filter(|result| result.level == level).map(|result| result.findings.len()).sum()
    }

    /// error の指摘がないか
    pub fn passed(&self) -> bool {
        self.count(CheckLevel::Error) == 0
    }
}

/// 規則集で図面を調べる。規則の順に結果を返す
pub fn check_scene(scene: &Scene, rules: &RuleSet) -> CheckReport {
    let results = rules
        .rules
        .iter()
        .map(|rule| {
            let mut result = RuleResult {
                id: rule.id.clone(),
                kind: rule.kind,
                level: rule.level,
                reference: rule.reference.clone(),
                description: rule.description.clone(),
                checked: 0,
                unchecked: 0,
                findings: Vec::new(),
            };
            let mut findings = Findings::default();
            match rule.kind {
                CheckKind::LayerName => check_layer_names(scene, rule, &mut result, &mut findings),
                CheckKind::Color => check_colors(scene, rule, &mut result, &mut findings),
                CheckKind::LineType => check_line_types(scene, rule, &mut result, &mut findings),
                CheckKind::LineWidth => check_line_widths(scene, rule, &mut result, &mut findings),
                CheckKind::TextHeight => check_text_heights(scene, rule, &mut result, &mut findings),
                CheckKind::Characters => check_characters(scene, rule, &mut result, &mut findings),
                CheckKind::SheetSize => check_sheet_size(scene, rule, &mut result, &mut findings),
                CheckKind::Orientation => check_orientation(scene, rule, &mut result, &mut findings),
                CheckKind::Scale => check_scales(scene, rule, &mut result, &mut findings),
            }
            result.findings = findings.items;
            result
        })
        .collect();
    CheckReport { rule_set: rules.name.clone(), title: rules.title.clone(), results }
}

/// 値ごとの指摘を、最初に現れた順に集める
#[derive(Default)]
struct Findings {
    items: Vec<Finding>,
    index: HashMap<String, usize>,
}

impl Findings {
    fn get(&mut self, value: &str, message: impl FnOnce() -> String) -> &mut Finding {
        let index = *self.index.entry(value.to_string()).or_insert_with(|| {
            self.items.push(Finding {
                value: value.to_string(),
                message: message(),
                count: 0,
                entities: Vec::new(),
                layers: Vec::new(),
            });
            self.items.len() - 1
        });
        &mut self.items[index]
    }

    /// 図形 1 つを指摘に足す
    fn entity(&mut self, scene: &Scene, entity: &SceneEntity, value: &str, message: impl FnOnce() -> String) {
        let finding = self.get(value, message);
        finding.count += 1;
        finding.entities.push(entity.id.clone());
        for name in layer_names(scene, entity) {
            if !finding.layers.iter().any(|layer| layer == name) {
                finding.layers.push(name.to_string());
            }
        }
    }
}

/// 規則の layers に合うレイヤの図形
fn entities<'a>(scene: &'a Scene, rule: &Rule) -> Vec<&'a SceneEntity> {
    let Some(patterns) = &rule.layers else {
        return scene.entities.iter().collect();
    };
    let globs: Vec<Glob> = patterns.iter().filter_map(|pattern| Glob::new(pattern).ok()).collect();
    scene
        .entities
        .iter()
        .filter(|entity| layer_names(scene, entity).iter().any(|name| globs.iter().any(|glob| glob.matches(name))))
        .collect()
}

/// 図形を持つレイヤ（scene.layers の順）と、その図形の数
fn used_layers(scene: &Scene) -> Vec<(&str, usize)> {
    let mut counts: HashMap<u32, usize> = HashMap::new();
    for entity in &scene.entities {
        let own: HashSet<u32> = entity.layers.iter().copied().collect();
        for id in own {
            *counts.entry(id).or_default() += 1;
        }
    }
    scene.layers.iter().filter_map(|layer| counts.get(&layer.id).map(|count| (layer.name.as_str(), *count))).collect()
}

fn check_layer_names(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let globs: Vec<Glob> = rule.patterns.iter().flatten().filter_map(|pattern| Glob::new(pattern).ok()).collect();
    for (name, count) in used_layers(scene) {
        result.checked += 1;
        let width = text_width(name);
        let message = if rule.patterns.is_some() && !globs.iter().any(|glob| glob.matches(name)) {
            format!("レイヤ名 {name} は規則の名前の型に合いません")
        } else if let Some(max) = rule.max_width.filter(|max| width > *max) {
            format!("レイヤ名 {name} は {width} 文字（半角で数える）で、{max} 文字を超えます")
        } else {
            continue;
        };
        let finding = findings.get(name, || message);
        finding.count += count;
        finding.layers.push(name.to_string());
    }
}

fn check_colors(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let allowed: HashSet<[u8; 3]> = rule.colors.iter().flatten().filter_map(|color| parse_color(color)).collect();
    for entity in entities(scene, rule) {
        let mut colors: Vec<[u8; 3]> = Vec::new();
        visit_colors(&entity.primitives, &mut |color| {
            if let Some(rgb) = parse_color(color)
                && !colors.contains(&rgb)
            {
                colors.push(rgb);
            }
        });
        if colors.is_empty() {
            continue;
        }
        result.checked += 1;
        for rgb in colors.into_iter().filter(|rgb| !allowed.contains(rgb)) {
            let value = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            findings.entity(scene, entity, &value, || format!("色 {value} は規則の色にありません"));
        }
    }
}

/// 描く形の色（背景色で描く部分は除く）
fn visit_colors(primitives: &[Primitive], visit: &mut impl FnMut(&str)) {
    for primitive in primitives {
        match primitive {
            Primitive::Stroke(stroke) => visit(&stroke.color),
            Primitive::Fill(fill) => visit(&fill.color),
            Primitive::Dot(dot) => visit(&dot.color),
            Primitive::Text(text) => visit(&text.color),
            Primitive::Clipped(clipped) => {
                for stroke in &clipped.strokes {
                    visit(&stroke.color);
                }
            }
        }
    }
}

fn check_line_types(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let allowed: HashSet<String> = rule.names.iter().flatten().map(|name| name.to_lowercase()).collect();
    for entity in entities(scene, rule) {
        let Some(property) = entity.properties.iter().find(|property| property.key == "lineType") else {
            continue;
        };
        let value = property.value.trim();
        // 番号だけの線種（JWW）は名前がないので調べない
        if value.is_empty() || value.parse::<f64>().is_ok() {
            result.unchecked += 1;
            continue;
        }
        result.checked += 1;
        if !line_type_names(value).iter().any(|name| allowed.contains(&name.to_lowercase())) {
            findings.entity(scene, entity, value, || format!("線種 {value} は規則の線種にありません"));
        }
    }
}

/// 線種の属性の値が示す名前。「実線（1）」（SXF の名前と番号）や「BYLAYER（DASHED）」（DXF のレイヤから継ぐ線種）
/// のように括弧があれば、値そのもの、括弧の前、括弧の中
fn line_type_names(value: &str) -> Vec<&str> {
    let mut names = vec![value];
    if let Some(inner) = value.strip_suffix('）')
        && let Some((before, inside)) = inner.rsplit_once('（')
    {
        names.push(before.trim());
        names.push(inside.trim());
    }
    names
}

fn check_line_widths(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let widths = rule.widths.as_deref().unwrap_or_default();
    let tolerance = rule.tolerance.unwrap_or(WIDTH_TOLERANCE);
    for entity in entities(scene, rule) {
        let Some(property) = entity.properties.iter().find(|property| property.key == "lineWidth") else {
            continue;
        };
        // 太さの数がない（DXF のレイヤ・ブロックから継ぐ太さ、既定の太さ）
        let Some(PropertyRaw::Number(width)) = property.raw else {
            result.unchecked += 1;
            continue;
        };
        result.checked += 1;
        if !widths.iter().any(|allowed| (allowed - width).abs() <= tolerance) {
            let value = js::format_ja(width, 3);
            findings.entity(scene, entity, &value, || format!("線の太さ {value} mm は規則の太さにありません"));
        }
    }
}

/// 文字の部分の、用紙の上での高さ（mm）
fn paper_height(text: &TextPrimitive) -> f64 {
    let (x, y) = apply_vector(&text.matrix, 0.0, 1.0);
    text.character_height.unwrap_or(text.height) * js::hypot2(x, y)
}

fn text_parts(entity: &SceneEntity) -> impl Iterator<Item = &TextPrimitive> {
    entity.primitives.iter().filter_map(|primitive| match primitive {
        Primitive::Text(text) if !text.text.trim().is_empty() => Some(text),
        _ => None,
    })
}

fn check_text_heights(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let heights = rule.heights.as_deref().unwrap_or_default();
    let tolerance = rule.tolerance.unwrap_or(HEIGHT_TOLERANCE);
    for entity in entities(scene, rule) {
        let mut parts = text_parts(entity).peekable();
        if parts.peek().is_none() {
            continue;
        }
        // 用紙のない図面（DXF・DWG のモデル空間）では、文字の用紙の上での高さがわからない
        if scene.sheet.is_none() {
            result.unchecked += 1;
            continue;
        }
        result.checked += 1;
        let mut seen: Vec<String> = Vec::new();
        for part in parts {
            let height = paper_height(part);
            if heights.iter().any(|allowed| (allowed - height).abs() <= tolerance) {
                continue;
            }
            let value = js::format_ja(height, 2);
            if !seen.contains(&value) {
                findings.entity(scene, entity, &value, || format!("文字の高さ {value} mm は規則の高さにありません"));
                seen.push(value);
            }
        }
    }
}

fn check_characters(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let ranges: Vec<(u32, u32)> = rule.ranges.iter().flatten().filter_map(|range| parse_range(range).ok()).collect();
    let forbidden = |c: &char| ranges.iter().any(|(from, to)| (*from..=*to).contains(&u32::from(*c)));
    let message = |c: char| format!("文字 {c}（U+{:04X}）は使えない文字です", u32::from(c));
    for entity in entities(scene, rule) {
        let mut found: Vec<char> = Vec::new();
        let mut has_text = false;
        for part in text_parts(entity) {
            has_text = true;
            for c in part.text.chars().filter(forbidden) {
                if !found.contains(&c) {
                    found.push(c);
                }
            }
        }
        if !has_text {
            continue;
        }
        result.checked += 1;
        for c in found {
            findings.entity(scene, entity, &c.to_string(), || message(c));
        }
    }
    if rule.layers.is_some() {
        return;
    }
    for (name, _) in used_layers(scene) {
        result.checked += 1;
        let mut found: Vec<char> = name.chars().filter(forbidden).collect();
        found.dedup();
        for c in found {
            let finding = findings.get(&c.to_string(), || message(c));
            if !finding.layers.iter().any(|layer| layer == name) {
                finding.count += 1;
                finding.layers.push(name.to_string());
            }
        }
    }
}

fn check_sheet_size(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let Some(sheet) = &scene.sheet else {
        result.unchecked += 1;
        return;
    };
    result.checked += 1;
    let tolerance = rule.tolerance.unwrap_or(SHEET_TOLERANCE);
    let (long, short) = (sheet.width.max(sheet.height), sheet.width.min(sheet.height));
    let fits = rule.sizes.iter().flatten().any(|size| {
        let (a, b) = (size.width.max(size.height), size.width.min(size.height));
        (a - long).abs() <= tolerance && (b - short).abs() <= tolerance
    });
    if !fits {
        let value = format!("{} × {}", js::format_ja(sheet.width, 1), js::format_ja(sheet.height, 1));
        let finding = findings.get(&value, || format!("用紙の大きさ {value} mm は規則の大きさにありません"));
        finding.count += 1;
    }
}

fn check_orientation(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let Some(sheet) = &scene.sheet else {
        result.unchecked += 1;
        return;
    };
    result.checked += 1;
    let actual = if sheet.width >= sheet.height { Orientation::Landscape } else { Orientation::Portrait };
    if Some(actual) != rule.orientation {
        let (value, message) = match actual {
            Orientation::Landscape => ("landscape", "用紙が横向きです"),
            Orientation::Portrait => ("portrait", "用紙が縦向きです"),
        };
        findings.get(value, || message.to_string()).count += 1;
    }
}

fn check_scales(scene: &Scene, rule: &Rule, result: &mut RuleResult, findings: &mut Findings) {
    let allowed = |n: f64| {
        rule.scales.iter().flatten().any(|scale| (scale - n).abs() <= 1e-9 * n)
            || rule.mantissas.iter().flatten().any(|m| {
                let ratio = n / m;
                let power = 10f64.powf(ratio.log10().round());
                (ratio - power).abs() <= 1e-9 * ratio
            })
    };
    let value = |n: f64| format!("1:{}", js::format_ja(n, 3));
    let message = |text: &str| format!("尺度 {text} は規則の尺度にありません");
    // 図形の尺度（JWW のレイヤグループ、DXF・DWG のビューポート）
    for entity in &scene.entities {
        let Some(property) = entity.properties.iter().find(|property| property.key == "scale") else {
            continue;
        };
        let Some(PropertyRaw::Number(n)) = property.raw else {
            result.unchecked += 1;
            continue;
        };
        result.checked += 1;
        if !(n.is_finite() && n > 0.0 && allowed(n)) {
            let text = value(n);
            findings.entity(scene, entity, &text, || message(&text));
        }
    }
    // 表題欄の尺度（SXF の図面表題欄の属性。図形が尺度を持つ JWW の数の並びは除く）
    for property in scene.metadata.iter().filter(|property| property.key == "scale") {
        let numbers = match &property.raw {
            Some(PropertyRaw::Number(n)) => vec![*n],
            Some(PropertyRaw::Numbers(_)) => continue,
            _ => written_scales(&property.value),
        };
        if numbers.is_empty() {
            result.unchecked += 1;
        }
        for n in numbers {
            result.checked += 1;
            if !(n.is_finite() && n > 0.0 && allowed(n)) {
                let text = value(n);
                findings.get(&text, || message(&text)).count += 1;
            }
        }
    }
}

/// 書いた尺度（1:100、1/50、S=1：200 など）の N
fn written_scales(text: &str) -> Vec<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut scales = Vec::new();
    for (index, c) in chars.iter().enumerate() {
        if !matches!(c, ':' | '/' | '：' | '／') || index == 0 || !matches!(chars[index - 1], '1' | '１') {
            continue;
        }
        if index >= 2 && (chars[index - 2].is_ascii_digit() || chars[index - 2] == '.') {
            continue;
        }
        let digits: String = chars[index + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',') || ('０'..='９').contains(c))
            .filter(|c| **c != ',')
            .map(|c| {
                if ('０'..='９').contains(c) {
                    char::from(b'0' + (u32::from(*c) - u32::from('０')) as u8)
                } else {
                    *c
                }
            })
            .collect();
        if let Ok(n) = digits.parse::<f64>() {
            scales.push(n);
        }
    }
    scales
}

/// 半角 1、全角 2 で数えた長さ（ASCII と半角カナが半角）
fn text_width(text: &str) -> usize {
    text.chars().map(|c| if c.is_ascii() || ('\u{FF61}'..='\u{FF9F}').contains(&c) { 1 } else { 2 }).sum()
}

/// #rrggbb か #rrggbbaa の色（不透明度は見ない）
fn parse_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#')?;
    if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
        return None;
    }
    let byte = |index: usize| u8::from_str_radix(&hex[index..index + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// 2116 や FF61-FF9F の符号位置の範囲
fn parse_range(text: &str) -> Result<(u32, u32), String> {
    let point = |part: &str| {
        let part = part.trim();
        let digits = part.strip_prefix("U+").unwrap_or(part);
        u32::from_str_radix(digits, 16).map_err(|_| format!("文字の範囲 {text} は 16 進の符号位置ではありません"))
    };
    let (from, to) = match text.split_once('-') {
        Some((from, to)) => (point(from)?, point(to)?),
        None => {
            let only = point(text)?;
            (only, only)
        }
    };
    if from > to {
        return Err(format!("文字の範囲 {text} は始めが終わりより後ろです"));
    }
    Ok((from, to))
}

/// レイヤ名の型。* は 0 文字以上、? は 1 文字、[...] は中のどれか 1 文字（a-z の範囲、先頭の ! で否定）
#[derive(Debug, Clone)]
struct Glob {
    tokens: Vec<Token>,
}

#[derive(Debug, Clone)]
enum Token {
    Char(char),
    Any,
    Star,
    Class { negated: bool, ranges: Vec<(char, char)> },
}

impl Glob {
    fn new(pattern: &str) -> Result<Glob, String> {
        let mut tokens = Vec::new();
        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            tokens.push(match c {
                '*' => Token::Star,
                '?' => Token::Any,
                '[' => {
                    let negated = chars.next_if_eq(&'!').is_some();
                    let mut ranges = Vec::new();
                    loop {
                        let Some(first) = chars.next() else {
                            return Err(format!("レイヤ名の型 {pattern} の [ が閉じていません"));
                        };
                        if first == ']' && !ranges.is_empty() {
                            break;
                        }
                        if chars.peek() == Some(&'-') {
                            chars.next();
                            match chars.next() {
                                Some(']') => {
                                    ranges.push((first, first));
                                    ranges.push(('-', '-'));
                                    break;
                                }
                                Some(last) => ranges.push((first, last)),
                                None => return Err(format!("レイヤ名の型 {pattern} の [ が閉じていません")),
                            }
                        } else {
                            ranges.push((first, first));
                        }
                    }
                    Token::Class { negated, ranges }
                }
                other => Token::Char(other),
            });
        }
        Ok(Glob { tokens })
    }

    fn matches(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        // 位置 i までの型で、文字 j まで合うか（動的計画法）
        let mut current = vec![false; chars.len() + 1];
        current[0] = true;
        for token in &self.tokens {
            let mut next = vec![false; chars.len() + 1];
            for j in 0..=chars.len() {
                match token {
                    Token::Star => next[j] = current[j] || (j > 0 && next[j - 1]),
                    _ if j == 0 => {}
                    Token::Char(c) => next[j] = current[j - 1] && chars[j - 1] == *c,
                    Token::Any => next[j] = current[j - 1],
                    Token::Class { negated, ranges } => {
                        let c = chars[j - 1];
                        let inside = ranges.iter().any(|(from, to)| (*from..=*to).contains(&c));
                        next[j] = current[j - 1] && inside != *negated;
                    }
                }
            }
            current = next;
        }
        current[chars.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::placement;
    use crate::types::{BBox, Background, EntityKind, Property, SceneLayer, Sheet, StrokePrimitive};

    fn entity(id: &str, layer: u32, color: &str, properties: Vec<Property>) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: EntityKind::Line,
            type_name: "線分".into(),
            source: "test".into(),
            layers: vec![layer],
            primitives: vec![Primitive::Stroke(StrokePrimitive {
                points: vec![0.0, 0.0, 10.0, 0.0],
                closed: false,
                segments: None,
                color: color.into(),
                width: 0.25,
                dashes: Vec::new(),
            })],
            bbox: BBox { min_x: 0.0, min_y: 0.0, max_x: 10.0, max_y: 0.0 },
            properties,
            placement: None,
            drawn: true,
        }
    }

    /// 文字の座標系で高さ height の文字を、倍率 scale で用紙に置く
    fn text(id: &str, layer: u32, content: &str, height: f64, scale: f64) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: EntityKind::Text,
            type_name: "文字".into(),
            source: "test".into(),
            layers: vec![layer],
            primitives: vec![Primitive::Text(TextPrimitive {
                text: content.into(),
                color: "#000000".into(),
                font_family: "sans-serif".into(),
                height,
                character_height: None,
                scale_x: 1.0,
                cells: Vec::new(),
                box_width: height * content.chars().count() as f64,
                box_height: height,
                matrix: placement(0.0, 0.0, 30.0, scale, scale),
            })],
            bbox: BBox { min_x: 0.0, min_y: 0.0, max_x: 10.0, max_y: 10.0 },
            properties: Vec::new(),
            placement: None,
            drawn: true,
        }
    }

    fn scene(layers: &[&str], entities: Vec<SceneEntity>, sheet: Option<(f64, f64)>) -> Scene {
        Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: sheet.map(|(width, height)| Sheet { name: "1".into(), size: "A".into(), width, height }),
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 420.0, max_y: 297.0 },
            layers: layers
                .iter()
                .enumerate()
                .map(|(id, name)| SceneLayer { id: id as u32, name: (*name).into(), visible: true, group: None })
                .collect(),
            entities,
            underlays: Vec::new(),
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    fn mlit() -> RuleSet {
        RuleSet::built_in("mlit-cad-2025").unwrap()
    }

    fn result<'a>(report: &'a CheckReport, id: &str) -> &'a RuleResult {
        report.results.iter().find(|result| result.id == id).unwrap()
    }

    fn values(result: &RuleResult) -> Vec<&str> {
        result.findings.iter().map(|finding| finding.value.as_str()).collect()
    }

    #[test]
    fn every_built_in_rule_set_reads() {
        for name in BUILT_IN {
            let rules = RuleSet::built_in(name).unwrap();
            assert_eq!(rules.name, name);
        }
        assert!(RuleSet::built_in("none").is_none());
        assert!(RuleSet::named_or_json("none").unwrap_err().contains("mlit-cad-2025"));
    }

    #[test]
    fn checks_the_layer_names_of_the_mlit_standard() {
        let layers = [
            "D-STR",
            "D-STR-A1橋台",
            "D-BMK-道路中心線",
            "C-ORD",
            "D-ORD",
            "WALL",
            "D-XYZ",
            "d-str",
            "D-TTL-",
            "unused",
        ];
        let entities = (0..9).map(|layer| entity(&format!("#{layer}"), layer, "#000000", Vec::new())).collect();
        let report = check_scene(&scene(&layers, entities, None), &mlit());
        let names = result(&report, "layer-name");
        assert_eq!(names.checked, 9);
        // 発注用レイヤの責任主体は C だけ。図面オブジェクトは表 1-4 の 10 項目、ユーザ定義領域は空にしない
        assert_eq!(values(names), ["D-ORD", "WALL", "D-XYZ", "d-str", "D-TTL-"]);
        assert_eq!(names.findings[1].layers, ["WALL"]);
        assert_eq!(names.findings[1].count, 1);
        assert_eq!(names.level, CheckLevel::Error);
        assert!(!report.passed());

        let long = format!("D-STR-{}", "橋".repeat(126));
        let report = check_scene(&scene(&[&long], vec![entity("#1", 0, "#000000", Vec::new())], None), &mlit());
        assert!(result(&report, "layer-name").findings[0].message.contains("258 文字"));
    }

    #[test]
    fn checks_colors_line_types_and_line_widths() {
        // 太さの数がなければ、DXF のレイヤから継ぐ太さのように raw を持たない
        let style = |line_type: &str, width: Option<f64>| {
            let mut width_property = Property::new("lineWidth", "線幅", "");
            width_property.raw = width.map(PropertyRaw::Number);
            vec![Property::new("lineType", "線種", line_type), width_property]
        };
        let entities = vec![
            entity("a", 0, "#c00080", style("実線（1）", Some(0.25))),
            entity("b", 0, "#123456cc", style("BYLAYER（DASHED）", Some(0.3))),
            entity("c", 0, "#123456", style("CENTER", Some(0.13))),
            entity("d", 0, "background", style("2", None)),
            entity("e", 0, "#ffffff", style("ユーザ線種（17）", Some(2.0))),
        ];
        let report = check_scene(&scene(&["D-STR"], entities, None), &mlit());
        let colors = result(&report, "color");
        assert_eq!((colors.checked, values(colors)), (4, vec!["#123456"]));
        assert_eq!(colors.findings[0].entities, ["b", "c"]);
        let types = result(&report, "line-type");
        // 番号だけの線種（JWW）は調べられない
        assert_eq!((types.checked, types.unchecked), (4, 1));
        assert_eq!(values(types), ["CENTER", "ユーザ線種（17）"]);
        let widths = result(&report, "line-width");
        assert_eq!((widths.checked, widths.unchecked, values(widths)), (4, 1, vec!["0.3"]));
        assert!(report.passed());
        assert_eq!(report.count(CheckLevel::Warning), 4);
    }

    #[test]
    fn checks_text_heights_on_the_sheet_and_characters() {
        let entities = vec![
            text("ok", 0, "平面図", 2.5, 1.0),
            text("scaled", 0, "A1橋台", 250.0, 0.01),
            text("odd", 0, "面積 100㎡", 3.0, 1.0),
            text("kana", 0, "ｶﾅ①", 3.0, 1.0),
        ];
        let report = check_scene(&scene(&["D-STR", "D-ＳＴＲ"], entities.clone(), Some((420.0, 297.0))), &mlit());
        let heights = result(&report, "text-height");
        assert_eq!((heights.checked, values(heights)), (4, vec!["3"]));
        assert_eq!(heights.findings[0].entities, ["odd", "kana"]);
        let characters = result(&report, "characters");
        assert_eq!(values(characters), ["㎡", "ｶ", "ﾅ", "①"]);
        assert_eq!(characters.checked, 5);

        // 描画用の em と別に規定の高さがあれば、各文字のその高さを調べる。
        let mut drawn = entities.clone();
        for entity in &mut drawn {
            if let Primitive::Text(part) = &mut entity.primitives[0] {
                part.character_height = Some(part.height / crate::text::em_for(&part.text));
            }
        }
        let report = check_scene(&scene(&["D-STR"], drawn, Some((420.0, 297.0))), &mlit());
        assert_eq!(values(result(&report, "text-height")), ["2.2", "2.64"]);

        // 用紙のない図面（モデル空間）では、文字の用紙の上での高さがわからない
        let report = check_scene(&scene(&["D-STR"], entities, None), &mlit());
        let heights = result(&report, "text-height");
        assert_eq!((heights.checked, heights.unchecked), (0, 4));
    }

    #[test]
    fn checks_each_text_height_without_inference_from_other_entities() {
        let mut dimension = text("dimension", 0, "100", 2.5 * crate::text::EM_LATIN, 1.0);
        dimension.kind = EntityKind::Dimension;
        let Primitive::Text(part) = &mut dimension.primitives[0] else { unreachable!() };
        part.character_height = Some(2.5);
        let frame = text("frame", 0, "注記", 2.5, 1.0);
        for entities in [vec![dimension.clone()], vec![dimension.clone(), frame.clone()]] {
            let report = check_scene(&scene(&["D-STR"], entities, Some((420.0, 297.0))), &mlit());
            assert!(result(&report, "text-height").findings.is_empty());
        }
        // 別の図形に height の属性があっても、規定の高さを省略した文字は height を使う。
        dimension.properties.push(Property::new("height", "高さ", "2.5").raw(2.5));
        let report = check_scene(&scene(&["D-STR"], vec![dimension, frame], Some((420.0, 297.0))), &mlit());
        assert!(result(&report, "text-height").findings.is_empty());
    }

    #[test]
    fn checks_the_sheet_and_the_scales() {
        let viewport = |id: &str, n: f64| {
            let mut viewport = entity(id, 0, "#000000", vec![Property::new("scale", "縮尺", "").raw(n)]);
            viewport.kind = EntityKind::Viewport;
            viewport
        };
        let mut drawing = scene(
            &["D-TTL"],
            vec![viewport("1", 100.0), viewport("2", 250.0), viewport("3", 70.0)],
            Some((297.0, 420.0)),
        );
        drawing.metadata.push(Property::new("scale", "尺度", "1:600、S=1／35"));
        let report = check_scene(&drawing, &mlit());
        assert!(result(&report, "sheet-size").findings.is_empty());
        assert_eq!(values(result(&report, "orientation")), ["portrait"]);
        let scales = result(&report, "scale");
        assert_eq!((scales.checked, values(scales)), (5, vec!["1:70", "1:35"]));
        assert_eq!(scales.findings[0].entities, ["3"]);

        drawing.sheet = Some(Sheet { name: "1".into(), size: "自由".into(), width: 500.0, height: 300.0 });
        let report = check_scene(&drawing, &mlit());
        assert_eq!(values(result(&report, "sheet-size")), ["500 × 300"]);
        drawing.sheet = None;
        let report = check_scene(&drawing, &mlit());
        assert_eq!(result(&report, "sheet-size").unchecked, 1);
    }

    #[test]
    fn rules_of_an_organization_apply_to_the_layers_they_name() {
        let rules = RuleSet::from_json(
            r##"{"name": "office", "rules": [
                {"id": "frame-color", "kind": "color", "layers": ["*-TTL*"], "colors": ["#ffffff"]},
                {"id": "names", "kind": "layer-name", "patterns": ["A-*", "*-TTL"], "level": "error"}
            ]}"##,
        )
        .unwrap();
        let entities = vec![entity("1", 0, "#ff0000", Vec::new()), entity("2", 1, "#ff0000", Vec::new())];
        let report = check_scene(&scene(&["D-TTL", "A-WALL"], entities, None), &rules);
        assert_eq!(result(&report, "frame-color").findings[0].entities, ["1"]);
        assert!(report.passed());
        assert_eq!(result(&report, "frame-color").level, CheckLevel::Warning);
    }

    #[test]
    fn rejects_rules_with_missing_or_foreign_fields() {
        let error = |rule: &str| RuleSet::from_json(&format!(r#"{{"name": "x", "rules": [{rule}]}}"#)).unwrap_err();
        assert!(error(r#"{"id": "a", "kind": "color"}"#).contains("colors が要ります"));
        assert!(error(r##"{"id": "a", "kind": "color", "colors": ["#000"]}"##).contains("#rrggbb"));
        assert!(error(r#"{"id": "a", "kind": "color", "colors": [], "widths": [1]}"#).contains("widths は color"));
        assert!(error(r#"{"id": "a", "kind": "layer-name", "patterns": ["[AB"]}"#).contains("閉じていません"));
        assert!(error(r#"{"id": "a", "kind": "scale"}"#).contains("scales か mantissas"));
        assert!(error(r#"{"id": "a", "kind": "characters", "ranges": ["FF9F-FF61"]}"#).contains("後ろ"));
        assert!(error(r#"{"id": "a", "kind": "colour", "colors": []}"#).contains("読めません"));
        assert!(error(r#"{"id": "a", "kind": "color", "colors": [], "extra": 1}"#).contains("読めません"));
        let twice = r#"{"id": "a", "kind": "orientation", "orientation": "landscape"}"#;
        assert!(error(&format!("{twice}, {twice}")).contains("重なっています"));
    }

    #[test]
    fn globs_match_like_shell_patterns() {
        let matches = |pattern: &str, text: &str| Glob::new(pattern).unwrap().matches(text);
        assert!(matches("[SDCM]-STR-?*", "D-STR-A1橋台"));
        assert!(!matches("[SDCM]-STR-?*", "D-STR-"));
        assert!(matches("*", ""));
        assert!(matches("A*B*C", "AxxBC"));
        assert!(!matches("A*B*C", "AxxCB"));
        assert!(matches("[!0-9]x", "ax"));
        assert!(!matches("[!0-9]x", "5x"));
        assert!(matches("[a-]", "-"));
        assert!(matches("[]]", "]"));
    }

    #[test]
    fn reads_written_scales() {
        assert_eq!(written_scales("1:100"), [100.0]);
        assert_eq!(written_scales("1/50（A3）、S=1：２００"), [50.0, 200.0]);
        assert_eq!(written_scales("1:1,000"), [1000.0]);
        assert_eq!(written_scales("図示"), Vec::<f64>::new());
        assert_eq!(written_scales("11:20"), Vec::<f64>::new());
    }
}
