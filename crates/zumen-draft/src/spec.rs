//! 図面の仕様（決定 0018）。JSON の 1 つの文書で、レイヤ・線種・ブロック・用紙・ビュー・図形を持つ。
//!
//! - ビューの図形はモデル空間の実寸（mm）、用紙の図形は用紙の mm の座標。
//! - 大きさ（文字の高さ、矢印、記号、表の幅、ハッチングの間隔）はいつも用紙の mm で、ビューの縮尺を掛けて描く。
//! - 図形は `type` を持つオブジェクト。どの図形も `layer`・`color`・`lineType`・`lineWidth` を持てる。
//!
//! 読み違いは、図形の場所（`sheets[0].views[1].items[4]`）と、変わらない英語の code で返す。

use crate::DraftError;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub type Point = [f64; 2];

/// 図面全体
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Spec {
    /// 長さの単位。いまは mm だけ
    #[serde(default)]
    pub units: Option<String>,
    /// 文字の既定のフォント
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default)]
    pub layers: Vec<LayerSpec>,
    #[serde(default)]
    pub line_types: Vec<LineTypeSpec>,
    #[serde(default)]
    pub blocks: Vec<BlockSpec>,
    #[serde(default)]
    pub sheets: Vec<SheetSpec>,
}

/// 図形の既定の値
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Defaults {
    pub layer: Option<String>,
    /// 文字の高さ（用紙の mm）。既定は 2.5
    pub text_height: Option<f64>,
    /// 寸法の値の高さ（用紙の mm）。既定は文字の高さ
    pub dimension_text_height: Option<f64>,
    /// 寸法の端の記号（arrow・tick・dot・none）。既定は arrow
    pub arrow: Option<String>,
    /// 寸法の端の記号の大きさ（用紙の mm）。既定は 2.5
    pub arrow_size: Option<f64>,
    /// 寸法の値の小数の桁数。既定は 0
    pub precision: Option<u32>,
    /// 寸法の値に 3 桁ごとの区切り（,）を入れる。既定は true
    pub thousands: Option<bool>,
    /// 寸法を置くレイヤ
    pub dimension_layer: Option<String>,
    /// 文字を置くレイヤ
    pub text_layer: Option<String>,
    /// ハッチングを置くレイヤ
    pub hatch_layer: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayerSpec {
    pub name: String,
    #[serde(default)]
    pub color: Option<Value>,
    #[serde(default)]
    pub line_type: Option<String>,
    /// 線の太さ（mm）
    #[serde(default)]
    pub line_width: Option<f64>,
    /// 印刷する（既定は true）
    #[serde(default)]
    pub plot: Option<bool>,
    #[serde(default)]
    pub off: bool,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineTypeSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// 線分・空白の長さを交互に（用紙の mm）。0 の線分は点
    pub pattern: Vec<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlockSpec {
    pub name: String,
    #[serde(default)]
    pub base_point: Option<Point>,
    #[serde(default)]
    pub items: Vec<Value>,
    #[serde(default)]
    pub attributes: Vec<AttributeSpec>,
}

/// ブロックの属性の定義（ATTDEF）
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttributeSpec {
    pub tag: String,
    #[serde(default)]
    pub prompt: Option<String>,
    /// 既定の値
    #[serde(default)]
    pub value: Option<String>,
    pub at: Point,
    #[serde(default)]
    pub height: Option<f64>,
    #[serde(default)]
    pub align: Option<String>,
    #[serde(default)]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub invisible: bool,
}

/// 用紙 1 枚（レイアウト）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SheetSpec {
    pub name: String,
    /// A0〜A4・B1〜B5 か、[幅, 高さ]（mm）
    #[serde(default)]
    pub paper: Option<Value>,
    /// landscape（既定）か portrait
    #[serde(default)]
    pub orientation: Option<String>,
    #[serde(default)]
    pub frame: Option<FrameSpec>,
    #[serde(default)]
    pub views: Vec<ViewSpec>,
    #[serde(default)]
    pub items: Vec<Value>,
}

/// 図面の枠と表題欄
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FrameSpec {
    /// 用紙の端から枠までの余白（左・下・右・上、mm）。既定は [20, 10, 10, 10]（JIS Z 8311 の綴じ代あり）
    #[serde(default)]
    pub margins: Option<[f64; 4]>,
    /// 枠の線の太さ（mm）。既定は 0.5
    #[serde(default)]
    pub line_width: Option<f64>,
    #[serde(default)]
    pub layer: Option<String>,
    #[serde(default)]
    pub title_block: Option<TitleBlockSpec>,
}

/// 表題欄。right は枠の右端の縦の帯、bottom-right は右下の表
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleBlockSpec {
    #[serde(default)]
    pub placement: Option<String>,
    /// 幅（mm）。既定は right で 40、bottom-right で 120
    #[serde(default)]
    pub width: Option<f64>,
    pub rows: Vec<TitleRowSpec>,
    #[serde(default)]
    pub text_height: Option<f64>,
}

/// 表題欄の 1 行。label は欄の左上の小さな見出し、value は欄の値
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TitleRowSpec {
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    /// 行の高さ（mm）。省略した行は残りの高さを分け合う（right のとき）
    #[serde(default)]
    pub height: Option<f64>,
    /// 値の文字の高さ（mm）
    #[serde(default)]
    pub value_height: Option<f64>,
    /// 値の位置（既定は middle-center）
    #[serde(default)]
    pub align: Option<String>,
}

/// 縮尺のあるビュー（紙空間のビューポート）
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewSpec {
    #[serde(default)]
    pub name: Option<String>,
    /// 縮尺の分母（1:250 なら 250）
    pub scale: f64,
    /// 用紙の上のビューポートの範囲 [x0, y0, x1, y1]（mm）
    pub viewport: [f64; 4],
    /// ビューポートの中心に映すモデルの点。省略すると図形の範囲の中心
    #[serde(default)]
    pub center: Option<Point>,
    #[serde(default)]
    pub title: Option<ViewTitleSpec>,
    /// ビューポートの枠を用紙に描く
    #[serde(default)]
    pub border: bool,
    #[serde(default)]
    pub items: Vec<Value>,
}

/// ビューの見出し（番号の丸・名前・縮尺）
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewTitleSpec {
    #[serde(default)]
    pub number: Option<String>,
    pub text: String,
    /// 縮尺の文字。省略すると S=1:<縮尺>
    #[serde(default)]
    pub scale_text: Option<String>,
    /// 用紙の上の位置（番号の丸の中心の高さの、左端）。省略するとビューポートの左下の内側
    #[serde(default)]
    pub at: Option<Point>,
    #[serde(default)]
    pub height: Option<f64>,
}

/// 図形の見た目（どの図形も持てる）
#[derive(Debug, Clone, Default)]
pub struct StyleSpec {
    pub layer: Option<String>,
    pub color: Option<Value>,
    pub line_type: Option<String>,
    pub line_width: Option<f64>,
}

/// 図形 1 つと、その場所
#[derive(Debug, Clone)]
pub struct Item {
    pub path: String,
    pub style: StyleSpec,
    pub shape: Shape,
}

#[derive(Debug, Clone)]
pub enum Shape {
    Line(LineSpec),
    Polyline(PolylineSpec),
    Rectangle(RectangleSpec),
    Circle(CircleSpec),
    Arc(ArcSpec),
    Ellipse(EllipseSpec),
    Point(PointSpec),
    Text(TextSpec),
    Hatch(HatchSpec),
    Dimension(DimensionSpec),
    DimensionChain(DimensionChainSpec),
    Insert(InsertSpec),
    Table(TableSpec),
    Grid(GridSpec),
    NorthArrow(NorthArrowSpec),
    ScaleBar(ScaleBarSpec),
    LevelMark(LevelMarkSpec),
    SpotLevel(SpotLevelSpec),
    Leader(LeaderSpec),
    PolygonLabel(PolygonLabelSpec),
    ViewTitle(ViewTitleSpec),
    Group(Vec<Item>),
}

/// 図形の種類の名前（エラーの案内に使う）
pub const TYPES: [&str; 22] = [
    "line",
    "polyline",
    "rectangle",
    "circle",
    "arc",
    "ellipse",
    "point",
    "text",
    "hatch",
    "dimension",
    "dimensionChain",
    "insert",
    "table",
    "grid",
    "northArrow",
    "scaleBar",
    "levelMark",
    "spotLevel",
    "leader",
    "polygonLabel",
    "viewTitle",
    "group",
];

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineSpec {
    pub from: Point,
    pub to: Point,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolylineSpec {
    pub points: Vec<Point>,
    #[serde(default)]
    pub closed: bool,
    /// 頂点から次の頂点への弧の膨らみ（tan(中心角/4)）
    #[serde(default)]
    pub bulges: Option<Vec<f64>>,
    /// 線の幅（用紙の mm）。塗った太い線
    #[serde(default)]
    pub width: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RectangleSpec {
    /// 向かい合う 2 つの角
    pub from: Point,
    pub to: Point,
    /// 中を塗る（SOLID のハッチング）
    #[serde(default)]
    pub fill: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CircleSpec {
    pub center: Point,
    pub radius: f64,
    #[serde(default)]
    pub fill: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArcSpec {
    pub center: Point,
    pub radius: f64,
    /// 始まりと終わりの角度（度、X 軸から反時計回り）
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EllipseSpec {
    pub center: Point,
    /// 長軸の端点（中心からの相対）
    pub major_axis: Point,
    /// 短軸 / 長軸
    pub ratio: f64,
    /// 楕円弧の始まりと終わり（度、長軸から）
    #[serde(default)]
    pub start: Option<f64>,
    #[serde(default)]
    pub end: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PointSpec {
    pub at: Point,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextSpec {
    pub at: Point,
    pub text: String,
    /// 用紙の mm
    #[serde(default)]
    pub height: Option<f64>,
    /// bottom-left（既定）・top-center など 9 つと、ベースラインの left・center・right
    #[serde(default)]
    pub align: Option<String>,
    #[serde(default)]
    pub rotation: Option<f64>,
    /// 折り返す幅（用紙の mm）
    #[serde(default)]
    pub width: Option<f64>,
    /// この幅（用紙の mm）に収まらなければ、文字の幅を縮める
    #[serde(default)]
    pub fit: Option<f64>,
    /// 文字の幅の係数
    #[serde(default)]
    pub width_factor: Option<f64>,
    /// 行の間隔の係数（複数行）
    #[serde(default)]
    pub line_spacing: Option<f64>,
    #[serde(default)]
    pub font: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HatchSpec {
    /// 閉じた境界。1 つ目が外側、ほかは穴
    pub boundaries: Vec<Vec<Point>>,
    /// SOLID（既定）、組み込みの模様の名前、または patternLines と組む任意の名前
    #[serde(default)]
    pub pattern: Option<String>,
    /// 模様の尺度（組み込みの模様は用紙の mm で描くので、1 で用紙の上の既定の間隔）
    #[serde(default)]
    pub scale: Option<f64>,
    /// 模様の角度（度）
    #[serde(default)]
    pub angle: Option<f64>,
    /// 模様の線 [角度, x, y, dx, dy, 線分, 空白…]（PAT の書き方、用紙の mm）
    #[serde(default)]
    pub pattern_lines: Option<Vec<Vec<f64>>>,
    /// 境界の線も描く
    #[serde(default)]
    pub outline: bool,
}

/// 寸法の見た目（寸法・寸法の並び・通り芯の寸法が持つ）
#[derive(Debug, Clone, Default)]
pub struct DimensionLook {
    pub arrow: Option<String>,
    pub arrow_size: Option<f64>,
    pub text_height: Option<f64>,
    pub precision: Option<u32>,
    pub thousands: Option<bool>,
    /// 測る点から寸法補助線の始まりまでの隙間（用紙の mm）。既定は 1
    pub extension_gap: Option<f64>,
    /// 寸法線から先に延ばす寸法補助線の長さ（用紙の mm）。既定は 2
    pub extension_beyond: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionSpec {
    /// linear（既定、水平・垂直か angle の向き）か aligned（2 点の向き）
    #[serde(default)]
    pub kind: Option<String>,
    pub from: Point,
    pub to: Point,
    /// 寸法線の位置: 測る点からの距離（用紙の mm、正は from → to の左側）
    #[serde(default)]
    pub offset: Option<f64>,
    /// 寸法線の位置: 寸法線が通る点（offset の代わり）
    #[serde(default)]
    pub at: Option<Point>,
    /// linear の寸法線の角度（度）
    #[serde(default)]
    pub angle: Option<f64>,
    /// 寸法の文字。<> は測った値
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub arrow: Option<String>,
    #[serde(default)]
    pub arrow_size: Option<f64>,
    #[serde(default)]
    pub text_height: Option<f64>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub thousands: Option<bool>,
    #[serde(default)]
    pub extension_gap: Option<f64>,
    #[serde(default)]
    pub extension_beyond: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionChainSpec {
    pub points: Vec<Point>,
    /// 寸法線の角度（度）。省略すると最初と最後の点の向きを水平・垂直にそろえた向き
    #[serde(default)]
    pub angle: Option<f64>,
    #[serde(default)]
    pub offset: Option<f64>,
    #[serde(default)]
    pub at: Option<Point>,
    /// 全体の寸法を、さらに外（用紙の mm、既定は 8）に足す
    #[serde(default)]
    pub total: Option<bool>,
    #[serde(default)]
    pub total_offset: Option<f64>,
    #[serde(default)]
    pub arrow: Option<String>,
    #[serde(default)]
    pub arrow_size: Option<f64>,
    #[serde(default)]
    pub text_height: Option<f64>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub thousands: Option<bool>,
    #[serde(default)]
    pub extension_gap: Option<f64>,
    #[serde(default)]
    pub extension_beyond: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InsertSpec {
    pub block: String,
    pub at: Point,
    /// 数か [x, y]
    #[serde(default)]
    pub scale: Option<Value>,
    #[serde(default)]
    pub rotation: Option<f64>,
    /// 属性の値（タグ → 値）
    #[serde(default)]
    pub attributes: Option<Map<String, Value>>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSpec {
    /// 表の左上
    pub at: Point,
    /// 列の幅（用紙の mm）
    pub columns: Vec<f64>,
    /// 行の高さ（用紙の mm）。省略すると rowHeight
    #[serde(default)]
    pub rows: Option<Vec<f64>>,
    #[serde(default)]
    pub row_height: Option<f64>,
    /// 行ごとのセル。文字列・数・null か、{text, colspan, rowspan, align, height}
    pub cells: Vec<Vec<Value>>,
    #[serde(default)]
    pub text_height: Option<f64>,
    /// セルの中の余白（用紙の mm）。既定は 1
    #[serde(default)]
    pub padding: Option<f64>,
    /// セルの既定の位置。既定は middle-left
    #[serde(default)]
    pub align: Option<String>,
    /// 列ごとの位置
    #[serde(default)]
    pub column_align: Option<Vec<Option<String>>>,
    /// shrink（既定、幅を縮める）・wrap（折り返す）・none
    #[serde(default)]
    pub fit: Option<String>,
    /// 外枠と内側の線の太さ（mm）
    #[serde(default)]
    pub outer_width: Option<f64>,
    #[serde(default)]
    pub inner_width: Option<f64>,
}

/// 表のセル
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellSpec {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub colspan: Option<usize>,
    #[serde(default)]
    pub rowspan: Option<usize>,
    #[serde(default)]
    pub align: Option<String>,
    #[serde(default)]
    pub height: Option<f64>,
    /// セルの対角線（斜線で「該当なし」を表す）
    #[serde(default)]
    pub diagonal: bool,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisSpec {
    pub label: String,
    pub at: f64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridSpec {
    /// 縦の通り芯（x の位置）
    #[serde(default)]
    pub x: Vec<AxisSpec>,
    /// 横の通り芯（y の位置）
    #[serde(default)]
    pub y: Vec<AxisSpec>,
    /// 通り芯を引く範囲 [x0, y0, x1, y1]（モデル）。省略すると通り芯の範囲
    #[serde(default)]
    pub extent: Option<[f64; 4]>,
    /// 範囲から外へ延ばす長さ（用紙の mm）。既定は 20
    #[serde(default)]
    pub overhang: Option<f64>,
    /// 丸の直径（用紙の mm）。既定は 8
    #[serde(default)]
    pub bubble: Option<f64>,
    /// 丸を付ける端: start（既定、下と左）・end・both
    #[serde(default)]
    pub bubbles: Option<String>,
    /// 通り芯の間の寸法を描く（既定は true）
    #[serde(default)]
    pub dimensions: Option<bool>,
    #[serde(default)]
    pub arrow: Option<String>,
    #[serde(default)]
    pub arrow_size: Option<f64>,
    #[serde(default)]
    pub text_height: Option<f64>,
    #[serde(default)]
    pub precision: Option<u32>,
    #[serde(default)]
    pub thousands: Option<bool>,
    #[serde(default)]
    pub extension_gap: Option<f64>,
    #[serde(default)]
    pub extension_beyond: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NorthArrowSpec {
    pub at: Point,
    /// 直径（用紙の mm）。既定は 15
    #[serde(default)]
    pub size: Option<f64>,
    /// 北の向き（度、上から反時計回り）
    #[serde(default)]
    pub angle: Option<f64>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScaleBarSpec {
    /// 左下
    pub at: Point,
    /// モデルの長さ
    pub length: f64,
    #[serde(default)]
    pub divisions: Option<u32>,
    /// 長さの表示の単位（m・mm）。既定は m
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub height: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LevelMarkSpec {
    /// 三角の先
    pub at: Point,
    pub text: String,
    /// 三角の大きさ（用紙の mm）。既定は 3
    #[serde(default)]
    pub size: Option<f64>,
    /// 三角の先を通る水平の線の長さ（用紙の mm）
    #[serde(default)]
    pub line: Option<f64>,
    #[serde(default)]
    pub text_height: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpotLevelSpec {
    pub at: Point,
    pub text: String,
    #[serde(default)]
    pub text_height: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaderSpec {
    /// 指す点から文字の側へ
    pub points: Vec<Point>,
    pub text: String,
    #[serde(default)]
    pub arrow: Option<String>,
    #[serde(default)]
    pub arrow_size: Option<f64>,
    #[serde(default)]
    pub text_height: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PolygonLabelSpec {
    pub points: Vec<Point>,
    /// 辺の長さを書く（既定は true）
    #[serde(default)]
    pub lengths: Option<bool>,
    /// 長さの単位（m・mm）と桁数。既定は m の 3 桁
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub precision: Option<u32>,
    /// 面積を中に書く。{} が ㎡ の値になる書式（例「敷地面積 {}㎡」）か true
    #[serde(default)]
    pub area: Option<Value>,
    #[serde(default)]
    pub area_precision: Option<u32>,
    /// 面積の文字の位置。省略すると多角形の重心
    #[serde(default)]
    pub area_at: Option<Point>,
    /// 境界の線も描く
    #[serde(default)]
    pub outline: bool,
    #[serde(default)]
    pub text_height: Option<f64>,
}

fn invalid(path: &str, message: impl Into<String>) -> DraftError {
    DraftError::new("invalid-spec", path, message)
}

/// serde の読み違いを、場所と code のあるエラーにする
fn from_value<T: DeserializeOwned>(value: Value, path: &str) -> Result<T, DraftError> {
    serde_json::from_value(value).map_err(|error| {
        let message = error.to_string();
        let code = if message.contains("unknown field") {
            "unknown-key"
        } else if message.contains("missing field") {
            "invalid-spec"
        } else {
            "invalid-value"
        };
        DraftError::new(code, path, message)
    })
}

/// 仕様の JSON を読む
pub fn parse(value: &Value) -> Result<Spec, DraftError> {
    if !value.is_object() {
        return Err(invalid("", "仕様はオブジェクトで指定してください"));
    }
    from_value(value.clone(), "")
}

const STYLE_KEYS: [&str; 4] = ["layer", "color", "lineType", "lineWidth"];

/// 図形の並びを読む
pub fn parse_items(values: &[Value], path: &str) -> Result<Vec<Item>, DraftError> {
    values.iter().enumerate().map(|(index, value)| parse_item(value, &format!("{path}[{index}]"))).collect()
}

fn parse_item(value: &Value, path: &str) -> Result<Item, DraftError> {
    let Value::Object(object) = value else {
        return Err(invalid(path, "図形はオブジェクトで指定してください"));
    };
    let kind = match object.get("type") {
        Some(Value::String(kind)) => kind.as_str(),
        _ => return Err(invalid(path, format!("type がありません（{}）", TYPES.join("・")))),
    };
    let mut style = StyleSpec::default();
    let mut rest = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "type" => {}
            "layer" => style.layer = Some(string(value, path, key)?),
            "color" => style.color = Some(value.clone()),
            "lineType" => style.line_type = Some(string(value, path, key)?),
            "lineWidth" => {
                style.line_width =
                    Some(value.as_f64().ok_or_else(|| invalid(path, "lineWidth は数（mm）で指定してください"))?)
            }
            _ => {
                rest.insert(key.clone(), value.clone());
            }
        }
    }
    let rest = Value::Object(rest);
    let shape = match kind {
        "line" => Shape::Line(from_value(rest, path)?),
        "polyline" => Shape::Polyline(from_value(rest, path)?),
        "rectangle" => Shape::Rectangle(from_value(rest, path)?),
        "circle" => Shape::Circle(from_value(rest, path)?),
        "arc" => Shape::Arc(from_value(rest, path)?),
        "ellipse" => Shape::Ellipse(from_value(rest, path)?),
        "point" => Shape::Point(from_value(rest, path)?),
        "text" => Shape::Text(from_value(rest, path)?),
        "hatch" => Shape::Hatch(from_value(rest, path)?),
        "dimension" => Shape::Dimension(from_value(rest, path)?),
        "dimensionChain" => Shape::DimensionChain(from_value(rest, path)?),
        "insert" => Shape::Insert(from_value(rest, path)?),
        "table" => Shape::Table(from_value(rest, path)?),
        "grid" => Shape::Grid(from_value(rest, path)?),
        "northArrow" => Shape::NorthArrow(from_value(rest, path)?),
        "scaleBar" => Shape::ScaleBar(from_value(rest, path)?),
        "levelMark" => Shape::LevelMark(from_value(rest, path)?),
        "spotLevel" => Shape::SpotLevel(from_value(rest, path)?),
        "leader" => Shape::Leader(from_value(rest, path)?),
        "polygonLabel" => Shape::PolygonLabel(from_value(rest, path)?),
        "viewTitle" => Shape::ViewTitle(from_value(rest, path)?),
        "group" => {
            let Value::Object(rest) = rest else { unreachable!() };
            if let Some(key) = rest.keys().find(|key| key.as_str() != "items") {
                return Err(DraftError::new(
                    "unknown-key",
                    path,
                    format!("group に {key} は指定できません（items と {}）", STYLE_KEYS.join("・")),
                ));
            }
            let items = match rest.get("items") {
                Some(Value::Array(items)) => items.clone(),
                _ => return Err(invalid(path, "group の items は配列で指定してください")),
            };
            Shape::Group(parse_items(&items, &format!("{path}.items"))?)
        }
        other => {
            return Err(DraftError::new(
                "unknown-type",
                path,
                format!("図形の種類 {other} はありません（{}）", TYPES.join("・")),
            ));
        }
    };
    Ok(Item { path: path.to_string(), style, shape })
}

fn string(value: &Value, path: &str, key: &str) -> Result<String, DraftError> {
    match value {
        Value::String(text) if !text.is_empty() => Ok(text.clone()),
        _ => Err(invalid(path, format!("{key} は空でない文字列で指定してください"))),
    }
}

/// 表のセルを読む
pub fn parse_cell(value: &Value, path: &str) -> Result<CellSpec, DraftError> {
    Ok(match value {
        Value::Null => CellSpec::default(),
        Value::String(text) => CellSpec { text: Some(text.clone()), ..CellSpec::default() },
        Value::Number(number) => CellSpec { text: Some(number.to_string()), ..CellSpec::default() },
        Value::Bool(value) => CellSpec { text: Some(value.to_string()), ..CellSpec::default() },
        Value::Object(_) => from_value(value.clone(), path)?,
        Value::Array(_) => return Err(invalid(path, "セルは文字列・数・null かオブジェクトで指定してください")),
    })
}

macro_rules! dimension_look {
    ($($name:ident),*) => {
        $(impl $name {
            pub fn look(&self) -> DimensionLook {
                DimensionLook {
                    arrow: self.arrow.clone(),
                    arrow_size: self.arrow_size,
                    text_height: self.text_height,
                    precision: self.precision,
                    thousands: self.thousands,
                    extension_gap: self.extension_gap,
                    extension_beyond: self.extension_beyond,
                }
            }
        })*
    };
}

dimension_look!(DimensionSpec, DimensionChainSpec, GridSpec);
