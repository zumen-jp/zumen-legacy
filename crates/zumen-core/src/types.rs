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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Property {
    pub label: String,
    pub value: String,
}

impl Property {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self { label: label.into(), value: value.into() }
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
    /// 日本語の種類名（線分、文字、直線寸法など）
    #[serde(rename = "type")]
    pub kind: String,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Scene {
    /// 形式の名前（sfc、p21、jww など）
    pub format: String,
    /// 形式の版（SXF 3.1、JWW 700 など）
    pub version: String,
    /// 選べる表示。1 つしかない形式では省略する
    #[serde(skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<SceneView>>,
    /// この図面が描いている表示の id（views の 1 つ）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    pub sheet: Option<Sheet>,
    /// すべての図形の外接矩形（用紙の mm）
    pub bounds: BBox,
    pub layers: Vec<SceneLayer>,
    pub entities: Vec<SceneEntity>,
    pub notices: Vec<Notice>,
    /// 表題欄・ファイルの情報
    pub metadata: Vec<Property>,
    pub background: Background,
}
