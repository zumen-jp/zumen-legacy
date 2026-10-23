//! 形式に依存しない表示用の図面モデル（`@zumenjs/core` の types.ts と同じ JSON になる）。
//! 座標の単位は用紙上の mm で、Y 軸は上向き。部分図や部品の配置は解決済みの値を持つ。

use serde::{Serialize, Serializer};
use std::sync::Arc;

/// 2 次元のアフィン変換 [a, b, c, d, e, f]。x' = a x + c y + e、y' = b x + d y + f
pub type Matrix = [f64; 6];

/// 描画時に背景色で塗る色（白抜きの矢印など）
pub const BACKGROUND_COLOR: &str = "background";

/// 外接矩形。図形がない場合は min_x > max_x になる（JSON では無限大が null になる）
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BBox {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

/// 折線。points は x, y を交互に並べる。segments なら 2 点ずつ別の線分（ハッチング線など）。
/// 太さ・破線の長さは用紙上の mm。色は #rrggbb か、不透明度つきの #rrggbbaa
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StrokePrimitive {
    pub points: Vec<f64>,
    pub closed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<bool>,
    pub color: String,
    pub width: f64,
    /// 線分・空白の長さの繰り返し。実線は空
    pub dashes: Vec<f64>,
}

/// 塗り。rings は外形と中抜きで、偶奇規則で塗る
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FillPrimitive {
    pub color: String,
    pub rings: Vec<Vec<f64>>,
}

/// 領域で切り取って描く線（ハッチング線・パターン）
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ClippedPrimitive {
    pub rings: Vec<Vec<f64>>,
    #[serde(serialize_with = "tagged_strokes")]
    pub strokes: Vec<StrokePrimitive>,
}

/// 画面上で常に同じ大きさの点（SXF の点マーカ dot）
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DotPrimitive {
    pub x: f64,
    pub y: f64,
    pub color: String,
}

/// 1 文字分の配置。文字の座標系での中心位置
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GlyphCell {
    #[serde(rename = "char")]
    pub character: String,
    pub x: f64,
    pub y: f64,
}

/// 文字列。matrix は文字の座標系（右が文字の進む向き、上が文字の上、単位は文字の座標の mm）から
/// 用紙の座標への変換
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPrimitive {
    pub text: String,
    pub color: String,
    /// CSS の font-family
    pub font_family: String,
    /// 文字の座標系での 1 文字の高さ
    pub height: f64,
    /// 元の図面が定義する文字の高さ（文字の座標系）。省略時は height。
    /// 字形の em と文字の規定の高さが異なる場合も、解析はこの値を使う。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_height: Option<f64>,
    /// 字形の横方向の倍率（文字範囲幅に合わせる）
    pub scale_x: f64,
    pub cells: Vec<GlyphCell>,
    /// 文字範囲の幅と高さ（文字の座標系）。左下が原点
    pub box_width: f64,
    pub box_height: f64,
    pub matrix: Matrix,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Primitive {
    Stroke(StrokePrimitive),
    Fill(FillPrimitive),
    Clipped(ClippedPrimitive),
    Dot(DotPrimitive),
    Text(TextPrimitive),
}

/// 切り取りの中の線も、TypeScript と同じく kind: "stroke" を持たせる
fn tagged_strokes<S: Serializer>(strokes: &[StrokePrimitive], serializer: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Tagged<'a> {
        kind: &'static str,
        #[serde(flatten)]
        stroke: &'a StrokePrimitive,
    }
    serializer.collect_seq(strokes.iter().map(|stroke| Tagged { kind: "stroke", stroke }))
}

/// 図形の種類の、形式によらない英語のキー（`@zumenjs/core` の EntityKind と同じ）。どの形式の図形がどれになるかは
/// docs/architecture.md の「Stable keys」の表のとおり
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntityKind {
    Line,
    ConstructionLine,
    Polyline,
    Multiline,
    Mesh,
    Arc,
    Circle,
    Ellipse,
    EllipseArc,
    Spline,
    Clothoid,
    Helix,
    Point,
    Text,
    Attribute,
    AttributeDefinition,
    Dimension,
    Leader,
    Balloon,
    Hatch,
    Solid,
    Symbol,
    Image,
    Wipeout,
    Viewport,
}

impl EntityKind {
    pub const ALL: [EntityKind; 25] = [
        EntityKind::Line,
        EntityKind::ConstructionLine,
        EntityKind::Polyline,
        EntityKind::Multiline,
        EntityKind::Mesh,
        EntityKind::Arc,
        EntityKind::Circle,
        EntityKind::Ellipse,
        EntityKind::EllipseArc,
        EntityKind::Spline,
        EntityKind::Clothoid,
        EntityKind::Helix,
        EntityKind::Point,
        EntityKind::Text,
        EntityKind::Attribute,
        EntityKind::AttributeDefinition,
        EntityKind::Dimension,
        EntityKind::Leader,
        EntityKind::Balloon,
        EntityKind::Hatch,
        EntityKind::Solid,
        EntityKind::Symbol,
        EntityKind::Image,
        EntityKind::Wipeout,
        EntityKind::Viewport,
    ];

    /// JSON での名前（line、construction-line など）
    pub fn name(self) -> &'static str {
        match self {
            EntityKind::Line => "line",
            EntityKind::ConstructionLine => "construction-line",
            EntityKind::Polyline => "polyline",
            EntityKind::Multiline => "multiline",
            EntityKind::Mesh => "mesh",
            EntityKind::Arc => "arc",
            EntityKind::Circle => "circle",
            EntityKind::Ellipse => "ellipse",
            EntityKind::EllipseArc => "ellipse-arc",
            EntityKind::Spline => "spline",
            EntityKind::Clothoid => "clothoid",
            EntityKind::Helix => "helix",
            EntityKind::Point => "point",
            EntityKind::Text => "text",
            EntityKind::Attribute => "attribute",
            EntityKind::AttributeDefinition => "attribute-definition",
            EntityKind::Dimension => "dimension",
            EntityKind::Leader => "leader",
            EntityKind::Balloon => "balloon",
            EntityKind::Hatch => "hatch",
            EntityKind::Solid => "solid",
            EntityKind::Symbol => "symbol",
            EntityKind::Image => "image",
            EntityKind::Wipeout => "wipeout",
            EntityKind::Viewport => "viewport",
        }
    }
}

/// 長さの単位の、形式によらない英語のキー（`@zumenjs/core` の LengthUnit と同じ）。図面の座標（属性の raw の点・長さ）の
/// 単位を `Scene::unit` で示す。DXF・DWG の $INSUNITS の単位をすべて持つ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LengthUnit {
    Millimeter,
    Centimeter,
    Decimeter,
    Meter,
    Decameter,
    Hectometer,
    Kilometer,
    Gigameter,
    Micrometer,
    Nanometer,
    Angstrom,
    Inch,
    Foot,
    Yard,
    Mile,
    Microinch,
    Mil,
    UsSurveyFoot,
    AstronomicalUnit,
    LightYear,
    Parsec,
}

impl LengthUnit {
    pub const ALL: [LengthUnit; 21] = [
        LengthUnit::Millimeter,
        LengthUnit::Centimeter,
        LengthUnit::Decimeter,
        LengthUnit::Meter,
        LengthUnit::Decameter,
        LengthUnit::Hectometer,
        LengthUnit::Kilometer,
        LengthUnit::Gigameter,
        LengthUnit::Micrometer,
        LengthUnit::Nanometer,
        LengthUnit::Angstrom,
        LengthUnit::Inch,
        LengthUnit::Foot,
        LengthUnit::Yard,
        LengthUnit::Mile,
        LengthUnit::Microinch,
        LengthUnit::Mil,
        LengthUnit::UsSurveyFoot,
        LengthUnit::AstronomicalUnit,
        LengthUnit::LightYear,
        LengthUnit::Parsec,
    ];

    /// JSON での名前（millimeter、us-survey-foot など）
    pub fn name(self) -> &'static str {
        match self {
            LengthUnit::Millimeter => "millimeter",
            LengthUnit::Centimeter => "centimeter",
            LengthUnit::Decimeter => "decimeter",
            LengthUnit::Meter => "meter",
            LengthUnit::Decameter => "decameter",
            LengthUnit::Hectometer => "hectometer",
            LengthUnit::Kilometer => "kilometer",
            LengthUnit::Gigameter => "gigameter",
            LengthUnit::Micrometer => "micrometer",
            LengthUnit::Nanometer => "nanometer",
            LengthUnit::Angstrom => "angstrom",
            LengthUnit::Inch => "inch",
            LengthUnit::Foot => "foot",
            LengthUnit::Yard => "yard",
            LengthUnit::Mile => "mile",
            LengthUnit::Microinch => "microinch",
            LengthUnit::Mil => "mil",
            LengthUnit::UsSurveyFoot => "us-survey-foot",
            LengthUnit::AstronomicalUnit => "astronomical-unit",
            LengthUnit::LightYear => "light-year",
            LengthUnit::Parsec => "parsec",
        }
    }
}

/// 整形する前の属性の値。座標は [x, y]、範囲は [始め, 終わり] のような数の並び
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PropertyRaw {
    Number(f64),
    Text(String),
    Bool(bool),
    Numbers(Vec<f64>),
}

impl From<f64> for PropertyRaw {
    fn from(value: f64) -> Self {
        PropertyRaw::Number(value)
    }
}

impl From<String> for PropertyRaw {
    fn from(value: String) -> Self {
        PropertyRaw::Text(value)
    }
}

impl From<&str> for PropertyRaw {
    fn from(value: &str) -> Self {
        PropertyRaw::Text(value.to_string())
    }
}

impl From<bool> for PropertyRaw {
    fn from(value: bool) -> Self {
        PropertyRaw::Bool(value)
    }
}

impl From<Vec<f64>> for PropertyRaw {
    fn from(value: Vec<f64>) -> Self {
        PropertyRaw::Numbers(value)
    }
}

impl<const N: usize> From<[f64; N]> for PropertyRaw {
    fn from(value: [f64; N]) -> Self {
        PropertyRaw::Numbers(value.to_vec())
    }
}

/// 図形・図面の属性。label と value は人に見せる日本語と整形した文字列で、アプリケーションは key で属性を選ぶ。
/// key の一覧と、raw の形は docs/architecture.md の「Stable keys」の表のとおり
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Property {
    /// 形式によらない英語のキー（layer、color、start など）。1 つの図形の中では重ならない
    pub key: &'static str,
    pub label: String,
    pub value: String,
    /// 整形する前の値（数・座標・形式の番号など）。value が文字列そのものなら省く
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<PropertyRaw>,
}

impl Property {
    pub fn new(key: &'static str, label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { key, label: label.into(), value: value.into(), raw: None }
    }

    /// 整形する前の値を足す
    pub fn raw(mut self, raw: impl Into<PropertyRaw>) -> Self {
        self.raw = Some(raw.into());
        self
    }
}

/// 部分図・部品の配置。scale_x・scale_y は元の座標 1 mm が用紙で何 mm になるか
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub name: String,
    pub scale_x: f64,
    pub scale_y: f64,
    /// 用紙の座標から元の座標への変換
    pub inverse: Matrix,
}

/// 選択・属性表示の単位。primitives は描く順に並ぶ
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEntity {
    /// 形式内での識別子（SXF はインスタンス番号 #nn、配置の中では配置の番号を前に付ける）
    pub id: String,
    /// 形式によらない種類（line、text、dimension など）。アプリケーションはこれで図形を選ぶ
    pub kind: EntityKind,
    /// 日本語の種類名（線分、文字、直線寸法など）。人に見せる
    #[serde(rename = "type")]
    pub type_name: String,
    /// 元の形式での名前（line_feature など）
    pub source: String,
    /// 自分と、配置元の部分図・部品のレイヤ。すべて表示のときだけ見える
    pub layers: Vec<u32>,
    pub primitives: Vec<Primitive>,
    pub bbox: BBox,
    pub properties: Vec<Property>,
    /// 部分図・部品の中の図形は、元の座標から用紙への縮尺を持つ（実寸の計測に使う）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement: Option<Arc<Placement>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneLayer {
    pub id: u32,
    pub name: String,
    pub visible: bool,
    /// JWW のレイヤグループ名など、レイヤの上位の区分
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NoticeLevel {
    Info,
    Warning,
}

/// 読み取り・描画で省略・代替した内容。count は該当件数
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub level: NoticeLevel,
    pub code: String,
    pub message: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sheet {
    pub name: String,
    /// 用紙サイズの名前（A1、自由など）
    pub size: String,
    pub width: f64,
    pub height: f64,
}

/// 同じ図面の中の別の表示（DXF・DWG のモデル空間とレイアウトなど）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneView {
    pub id: String,
    pub name: String,
}

/// 図面の作成時に想定された背景
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Background {
    Dark,
    Light,
}

/// underlay の中身の種類の、形式によらない英語のキー（`@zumenjs/core` の UnderlayKind と同じ）。Image は画素の画像
/// （PNG・JPEG・TIFF など。中身の座標の単位は画素）、Pdf は PDF の 1 ページ（単位はページの pt、ページの回転を済ませた向き）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UnderlayKind {
    Image,
    Pdf,
}

impl UnderlayKind {
    pub const ALL: [UnderlayKind; 2] = [UnderlayKind::Image, UnderlayKind::Pdf];

    /// JSON での名前（image、pdf）
    pub fn name(self) -> &'static str {
        match self {
            UnderlayKind::Image => "image",
            UnderlayKind::Pdf => "pdf",
        }
    }
}

/// 図形の下に描くラスタの中身（画像、PDF のページ。`@zumenjs/core` の Underlay と同じ JSON になる）。中身のバイトは
/// 図面が持たず、描き手が中身の一部の範囲をある縮尺で描く（docs/decisions/0022）。中身の座標は、中身の左上を原点に
/// 右が x、下が y で、範囲は 0〜width × 0〜height。underlay は `Scene::view` の表示に属し、配置は matrix と clip に解決済み
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Underlay {
    /// 図面の中で underlay を指す識別子。同じバイトからは同じ id になる（DXF・DWG は図形のハンドルなど）
    pub id: String,
    pub kind: UnderlayKind,
    /// 図面に書かれた、中身のファイルの名前（パスを含むことがある）。図面のファイルそのものの中身なら None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// PDF のページ番号（1 から）。kind が Pdf のときだけ持つ
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    /// 中身の幅と高さ（中身の座標）
    pub width: f64,
    pub height: f64,
    /// 中身の座標から用紙の mm への変換
    pub matrix: Matrix,
    /// 描く範囲を切り取る、用紙の mm の多角形。偶奇規則で、外形と中抜き。None なら中身の全体を描く
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<Vec<Vec<f64>>>,
    /// 不透明度（0〜1）。None なら 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
    /// 自分と、配置元のレイヤ。`SceneEntity::layers` と同じく、すべて表示のときだけ描く
    pub layers: Vec<u32>,
    /// 中身の中で表示を切り替えるレイヤ（PDF の optional content group など）。None なら、中身はレイヤで変わらない
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_layers: Option<Vec<u32>>,
    /// 描く範囲（clip で切り取った後）の外接矩形（用紙の mm）。`Scene::bounds` はこれも含む
    pub bbox: BBox,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    /// 形式の名前（sfc、p21、jww、dxf など）
    pub format: String,
    /// 形式の版（SXF 3.1、JWW 700 など）
    pub version: String,
    /// 図面の座標（属性の raw の点・長さ）の長さの単位。SXF・JWW は millimeter、DXF・DWG は $INSUNITS
    /// （指定がなければ、notices の unit-assumed とともに見なした単位）。DXF・DWG のレイアウトでは、紙空間の図形
    /// （配置のない図形）の座標はレイアウトの用紙の単位になる。読み取りを経ずに作った図面では None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<LengthUnit>,
    /// 選べる表示。1 つしかない形式では省略する
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<SceneView>>,
    /// この図面が描いている表示の id（views の 1 つ）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    pub sheet: Option<Sheet>,
    /// すべての図形と underlay の外接矩形（用紙の mm）
    pub bounds: BBox,
    pub layers: Vec<SceneLayer>,
    pub entities: Vec<SceneEntity>,
    /// 図形の下に、この順に描くラスタの中身。空なら JSON では省く
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub underlays: Vec<Underlay>,
    pub notices: Vec<Notice>,
    /// 表題欄・ファイルの情報
    pub metadata: Vec<Property>,
    pub background: Background,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_units_have_the_names_of_their_json() {
        let mut names: Vec<&str> = LengthUnit::ALL.iter().map(|unit| unit.name()).collect();
        for unit in LengthUnit::ALL {
            assert_eq!(serde_json::to_value(unit).unwrap(), unit.name());
        }
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LengthUnit::ALL.len());
    }

    /// packages/core/tests/underlay.test.ts も同じファイルと比べ、TypeScript と同じ JSON になることを確かめる
    #[test]
    fn underlays_have_the_json_of_typescript() {
        let layer = |id: u32, name: &str| SceneLayer { id, name: name.into(), visible: true, group: None };
        let pt = 25.4 / 72.0;
        let scene = Scene {
            format: "test".into(),
            version: "1".into(),
            unit: None,
            views: None,
            view: None,
            sheet: Some(Sheet { name: "A4".into(), size: "A4".into(), width: 297.0, height: 210.0 }),
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 297.0, max_y: 210.0 },
            layers: vec![layer(0, "0"), layer(1, "写真")],
            entities: Vec::new(),
            underlays: vec![
                Underlay {
                    id: "page-1".into(),
                    kind: UnderlayKind::Pdf,
                    file: None,
                    page: Some(1),
                    width: 841.89,
                    height: 595.28,
                    matrix: [pt, 0.0, 0.0, -pt, 0.0, 210.0],
                    clip: None,
                    opacity: None,
                    layers: Vec::new(),
                    content_layers: Some(vec![0]),
                    bbox: BBox { min_x: 0.0, min_y: 0.0, max_x: 297.0, max_y: 210.0 },
                },
                Underlay {
                    id: "1F".into(),
                    kind: UnderlayKind::Image,
                    file: Some("photos\\site.jpg".into()),
                    page: None,
                    width: 400.0,
                    height: 300.0,
                    matrix: [0.25, 0.0, 0.0, -0.25, 100.0, 175.0],
                    clip: Some(vec![vec![100.0, 100.0, 200.0, 100.0, 200.0, 175.0, 100.0, 175.0]]),
                    opacity: Some(0.5),
                    layers: vec![1],
                    content_layers: None,
                    bbox: BBox { min_x: 100.0, min_y: 100.0, max_x: 200.0, max_y: 175.0 },
                },
            ],
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        };
        // JSON の 297 と 297.0 を同じ数とみなす
        fn floats(value: serde_json::Value) -> serde_json::Value {
            use serde_json::Value;
            match value {
                Value::Number(n) => {
                    serde_json::Number::from_f64(n.as_f64().unwrap()).map_or(Value::Null, Value::Number)
                }
                Value::Array(items) => Value::Array(items.into_iter().map(floats).collect()),
                Value::Object(map) => Value::Object(map.into_iter().map(|(key, item)| (key, floats(item))).collect()),
                other => other,
            }
        }
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("../../../packages/core/tests/underlay.scene.json")).unwrap();
        assert_eq!(floats(serde_json::to_value(&scene).unwrap()), floats(expected));
        // underlay のない図面の JSON には、underlays を書かない
        let plain = Scene { underlays: Vec::new(), ..scene };
        assert!(serde_json::to_value(&plain).unwrap().get("underlays").is_none());
        for kind in UnderlayKind::ALL {
            assert_eq!(serde_json::to_value(kind).unwrap(), kind.name());
        }
    }
}
