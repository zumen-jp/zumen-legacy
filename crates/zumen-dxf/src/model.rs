//! AutoCAD の図面のモデル（`@zumenjs/dxf` の model.ts）。DXF の読み取り（parse_dxf）と、これから作る DWG の読み取りで
//! 共有し、dxf_to_scene で表示用の図面にする。属性の名前は ezdxf の DXF 属性名と ezdwg に合わせた。
//! 座標は図面の単位（$INSUNITS）のままで、角度は度。数は JavaScript と同じく f64 で持つ。

use std::collections::HashMap;
use zumen_core::Notice;

pub type Vec2 = [f64; 2];
pub type Vec3 = [f64; 3];

/// model: モデル空間、paper: 紙空間（レイアウト）、block: ブロック定義の中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DxfSpace {
    Model,
    Paper,
    Block,
}

/// すべての図形に共通する属性（DXF Reference「Common Group Codes for Entities」）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfEntityBase {
    /// 図形の種類（グループコード 0）
    pub kind: String,
    /// ハンドル（5、16 進の大文字）。ハンドルを持たない R12 の図面では空文字
    pub handle: String,
    /// 所有者（330）。ブロック定義・レイアウトの BLOCK_RECORD のハンドル
    pub owner: Option<String>,
    pub space: DxfSpace,
    /// 属するブロック定義・紙空間のブロックの名前（space が block・paper のとき）
    pub block: Option<String>,
    pub layer: String,
    /// 線種の名前（6）。BYLAYER・BYBLOCK・CONTINUOUS を含む
    pub linetype: String,
    /// AutoCAD Color Index（62）。0 は BYBLOCK、256 は BYLAYER
    pub color_index: f64,
    /// トゥルーカラー（420、0xRRGGBB）
    pub true_color: Option<f64>,
    /// 透過度（440、0x020000TT は 255 − TT が透明さ）
    pub transparency: Option<f64>,
    /// 線の太さ（370、1/100 mm）。-1 は BYLAYER、-2 は BYBLOCK、-3 は既定
    pub lineweight: f64,
    /// 線種の尺度（48）
    pub linetype_scale: f64,
    /// 非表示（60）
    pub invisible: bool,
    /// 厚さ（39）
    pub thickness: Option<f64>,
    /// 押し出し方向（210）。OCS の Z 軸。省略は (0, 0, 1)
    pub extrusion: Option<Vec3>,
}

/// LWPOLYLINE と POLYLINE に共通する頂点の並び。bulges・widths は頂点と同じ数
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DxfVertexList {
    /// LWPOLYLINE・2D の POLYLINE は OCS、3D の POLYLINE は WCS
    pub points: Vec<Vec3>,
    /// 頂点から次の頂点への弧の膨らみ（tan(中心角 / 4)、負なら時計回り）
    pub bulges: Vec<f64>,
    /// 頂点ごとの始めと終わりの幅
    pub widths: Vec<Vec2>,
    pub closed: bool,
    pub flags: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfLwPolyline {
    pub vertices: DxfVertexList,
    /// すべての線分の幅（43）
    pub const_width: Option<f64>,
    pub elevation: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfPolyline {
    pub vertices: DxfVertexList,
    /// 頂点の種類（VERTEX の 70）
    pub vertex_flags: Vec<f64>,
    /// ポリフェースメッシュの面（頂点の番号、1 から。負は見えない辺）
    pub faces: Option<Vec<Vec<f64>>>,
    /// ポリゴンメッシュの M・N の数
    pub m_count: Option<f64>,
    pub n_count: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfSpline {
    pub degree: f64,
    pub flags: f64,
    pub closed: bool,
    pub periodic: bool,
    pub rational: bool,
    pub knots: Vec<f64>,
    /// WCS
    pub control_points: Vec<Vec3>,
    pub weights: Vec<f64>,
    pub fit_points: Vec<Vec3>,
    pub start_tangent: Option<Vec3>,
    pub end_tangent: Option<Vec3>,
}

/// TEXT・ATTRIB・ATTDEF に共通する 1 行の文字
#[derive(Debug, Clone, PartialEq)]
pub struct DxfTextBase {
    /// 文字列。%%c などの制御コードを含んだまま
    pub text: String,
    /// 第 1 整列点（OCS）
    pub insert: Vec3,
    /// 第 2 整列点（OCS）。左寄せ・ベースライン以外のとき
    pub align_point: Option<Vec3>,
    pub height: f64,
    pub rotation: f64,
    /// 斜体の角度（度）
    pub oblique: f64,
    /// 幅の係数
    pub width: f64,
    pub style: String,
    /// 0 左、1 中央、2 右、3 両端揃え、4 中心、5 フィット
    pub halign: f64,
    /// 0 ベースライン、1 下、2 中、3 上
    pub valign: f64,
    /// 2 は左右反転、4 は上下反転
    pub text_generation_flag: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfMText {
    /// 書式コードを除いた文字列（\P は改行）
    pub text: String,
    /// 書式コードを含む元の文字列
    pub raw_text: String,
    /// WCS
    pub insert: Vec3,
    pub char_height: f64,
    /// 折り返しの幅（41）。0 なら折り返さない
    pub rect_width: f64,
    /// 1〜9（左上・中上・右上・左中…右下）
    pub attachment_point: f64,
    pub drawing_direction: f64,
    /// 文字の進む向き（11、WCS）。ないときは rotation
    pub text_direction: Option<Vec3>,
    pub rotation: f64,
    pub style: String,
    pub line_spacing_style: f64,
    pub line_spacing_factor: f64,
    /// 背景の塗り（90）。1 は色で、2 は画面の背景色で塗る
    pub background_flags: f64,
    pub background_color_index: Option<f64>,
    pub background_true_color: Option<f64>,
    /// 背景の枠の倍率（45）
    pub background_scale: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfInsert {
    /// ブロックの名前
    pub name: String,
    /// OCS
    pub insert: Vec3,
    pub xscale: f64,
    pub yscale: f64,
    pub zscale: f64,
    pub rotation: f64,
    /// 配列（MINSERT）の列・行の数と間隔
    pub column_count: f64,
    pub row_count: f64,
    pub column_spacing: f64,
    pub row_spacing: f64,
    /// 配置に続く属性（ATTRIB）。位置はブロックの中ではなく、配置と同じ座標系
    pub attribs: Vec<DxfEntity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfDimension {
    /// 表示の図形を持つブロック（2、*D1 など）
    pub anonymous_block_name: String,
    /// 寸法の種類（70）。下位 3 ビットが 0 回転・1 平行・2 角度・3 直径・4 半径・5 3 点角度・6 座標
    pub dimtype: f64,
    /// 定義点（10、WCS）
    pub defpoint: Vec3,
    /// 13・14・15・16 の定義点（WCS）
    pub defpoint2: Option<Vec3>,
    pub defpoint3: Option<Vec3>,
    pub defpoint4: Option<Vec3>,
    pub defpoint5: Option<Vec3>,
    /// 寸法値の中点（11、OCS）
    pub text_midpoint: Vec3,
    /// ブロックを置く位置（12、OCS）
    pub insert: Option<Vec3>,
    /// 利用者の文字（1）。空や <> は計測値
    pub text: String,
    pub actual_measurement: Option<f64>,
    pub attachment_point: Option<f64>,
    pub text_rotation: Option<f64>,
    pub horizontal_direction: Option<f64>,
    /// 寸法線の回転（50、回転寸法）
    pub angle: Option<f64>,
    /// 寸法補助線の角度（52）
    pub oblique_angle: Option<f64>,
    pub dimstyle: String,
}

/// HATCH の辺（72 の 1〜4）
#[derive(Debug, Clone, PartialEq)]
pub enum DxfHatchEdge {
    Line {
        start: Vec2,
        end: Vec2,
    },
    Arc {
        center: Vec2,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        ccw: bool,
    },
    Ellipse {
        center: Vec2,
        /// 長軸の端点（中心からの相対）
        major_axis: Vec2,
        /// 短軸 / 長軸
        ratio: f64,
        /// 長軸からの角度（度）
        start_angle: f64,
        end_angle: f64,
        ccw: bool,
    },
    Spline {
        degree: f64,
        rational: bool,
        periodic: bool,
        knots: Vec<f64>,
        control_points: Vec<Vec2>,
        weights: Vec<f64>,
        fit_points: Vec<Vec2>,
    },
}

/// HATCH の境界（DXF Reference「Boundary Path Data」）。座標は OCS
#[derive(Debug, Clone, PartialEq)]
pub struct DxfHatchPath {
    /// 92 の値。1 外側、2 ポリライン、16 最も外側
    pub flags: f64,
    pub closed: bool,
    /// ポリラインの境界の頂点。辺の境界では空
    pub points: Vec<Vec3>,
    pub bulges: Option<Vec<f64>>,
    /// 線・弧・楕円弧・スプラインの辺の境界
    pub edges: Option<Vec<DxfHatchEdge>>,
}

/// 模様の線（DXF Reference「Pattern Data」）。角度・尺度を適用した後の値
#[derive(Debug, Clone, PartialEq)]
pub struct DxfPatternLine {
    pub angle: f64,
    pub base: Vec2,
    pub offset: Vec2,
    /// 正は線、負は空白、0 は点
    pub dashes: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfHatch {
    pub pattern_name: String,
    pub solid_fill: bool,
    pub associative: bool,
    /// 0 交互（奇偶）、1 最も外側、2 無視
    pub hatch_style: f64,
    pub pattern_type: f64,
    pub pattern_angle: f64,
    pub pattern_scale: f64,
    pub pattern_double: bool,
    /// OCS の Z
    pub elevation: f64,
    pub paths: Vec<DxfHatchPath>,
    pub pattern_lines: Vec<DxfPatternLine>,
    /// グラデーションの名前（470）。グラデーションでなければ None
    pub gradient: Option<String>,
    /// グラデーションの色（421 のトゥルーカラー）
    pub gradient_colors: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfLeader {
    /// WCS
    pub points: Vec<Vec3>,
    /// 0 直線、1 スプライン
    pub path_type: f64,
    pub annotation_type: f64,
    pub has_arrowhead: bool,
    pub dimstyle: String,
}

/// MULTILEADER の線の切れ目（寸法の切断）。始点と終点の間を描かない
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderBreak {
    pub start: Vec3,
    pub end: Vec3,
}

/// MULTILEADER の引出線の 1 本（DXF Reference「MLeader Leader Line Group Codes」）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderLine {
    /// 頂点（10、WCS）。矢印の先から順に、着地点の手前まで
    pub vertices: Vec<Vec3>,
    /// 線の切れ目（11・12）
    pub breaks: Vec<DxfMLeaderBreak>,
    /// 引出線の番号（91）。arrow_heads の index と対応する
    pub index: f64,
    /// 色（92、生の値）。BYBLOCK（0xC1000000）は MULTILEADER の引出線の色
    pub color: f64,
}

/// MULTILEADER の引出線のまとまり（「MLeader Leader Node Group Codes」）。同じ着地点に集まる線を持つ
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderLeader {
    pub lines: Vec<DxfMLeaderLine>,
    /// 着地点を持つ（290）。AutoCAD は 0 の引出線を描かない
    pub has_last_leader_line: bool,
    /// 折れ線の向きを持つ（291）
    pub has_dogleg_vector: bool,
    /// 着地点（10、WCS）。折れ線（ドッグレッグ）の始まり
    pub last_leader_point: Vec3,
    /// 折れ線の向き（11、WCS）。内容に向かう
    pub dogleg_vector: Vec3,
    /// 折れ線の長さ（40、尺度を掛けた値）
    pub dogleg_length: f64,
    /// 番号（90）
    pub index: f64,
    /// 内容への接続（271、R2010 以降）。0 は左右（水平）、1 は上下（垂直）
    pub attachment_direction: f64,
    /// 折れ線の切れ目（12・13）
    pub breaks: Vec<DxfMLeaderBreak>,
}

/// MULTILEADER の文字の内容（CONTEXT_DATA の 290 が 1 のとき）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderMText {
    /// 書式コードを含む文字列（304）。MTEXT の raw_text と同じ
    pub default_content: String,
    /// 文字の面の法線（11）
    pub extrusion: Vec3,
    /// 文字スタイル（STYLE）のハンドル（340）
    pub style_handle: String,
    /// 配置点（12、WCS）
    pub insert: Vec3,
    /// 文字の進む向き（13、WCS）
    pub text_direction: Vec3,
    /// 折り返しの幅（43、全体の尺度を掛ける前）。0 なら折り返さない
    pub width: f64,
    pub line_spacing_factor: f64,
    pub line_spacing_style: f64,
    /// 色（90、生の値）
    pub color: f64,
    /// 配置点の位置（171）。MTEXT の 71 と同じ 1〜9
    pub alignment: f64,
    /// 文字の並ぶ向き（172）
    pub flow_direction: f64,
    /// 背景の色（91、生の値）
    pub bg_color: f64,
    /// 背景の枠の倍率（141）
    pub bg_scale_factor: f64,
    /// 画面の背景色で塗る（291）
    pub use_window_bg_color: bool,
    /// 背景を塗る（292）
    pub has_bg_fill: bool,
    /// 段組みの種類（173）。0 は段組みなし
    pub column_type: f64,
}

/// MULTILEADER のブロックの内容（CONTEXT_DATA の 296 が 1 のとき）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderBlock {
    /// ブロック（BLOCK_RECORD）のハンドル（341）
    pub block_record_handle: String,
    /// 法線（14）
    pub extrusion: Vec3,
    /// 配置点（15、WCS）
    pub insert: Vec3,
    /// 尺度（16）
    pub scale: Vec3,
    /// 回転（46、ラジアン）
    pub rotation: f64,
    /// 色（93、生の値）
    pub color: f64,
}

/// MULTILEADER の表示の値（「MLeader Context Data Group Codes」）。大きさは全体の尺度を掛けた後の値
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderContext {
    /// 全体の尺度（40）
    pub scale: f64,
    /// 内容の基点（10、WCS）
    pub base_point: Vec3,
    /// 文字の高さ（41）
    pub char_height: f64,
    /// 矢印の大きさ（140）
    pub arrow_head_size: f64,
    /// 着地の隙間（145）
    pub landing_gap_size: f64,
    pub mtext: Option<DxfMLeaderMText>,
    pub block: Option<DxfMLeaderBlock>,
    pub leaders: Vec<DxfMLeaderLeader>,
}

/// 引出線ごとの矢印（94 の引出線の番号と、345 のブロックのハンドル）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderArrowHead {
    pub index: f64,
    pub handle: String,
}

/// ブロックの属性の値（330 の ATTDEF のハンドル、177 の番号、44 の幅、302 の値）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeaderAttrib {
    pub handle: String,
    pub index: f64,
    pub width: f64,
    pub text: String,
}

/// マルチ引出線（DXF Reference「Common MLeader Group Codes」）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLeader {
    /// MLEADERSTYLE のハンドル（340）
    pub style_handle: String,
    /// 引出線の種類（170）。0 なし、1 直線、2 スプライン
    pub leader_type: f64,
    /// 引出線の色（91、生の値）
    pub leader_line_color: f64,
    /// 引出線の線種（LTYPE）のハンドル（341）
    pub leader_linetype_handle: String,
    /// 引出線の太さ（171）
    pub leader_lineweight: f64,
    /// 折れ線（ドッグレッグ）を描く（291）
    pub has_dogleg: bool,
    /// 矢印のブロック（BLOCK_RECORD）のハンドル（342）。空なら塗りつぶしの矢印
    pub arrow_head_handle: String,
    /// 内容の種類（172）。0 なし、1 ブロック、2 文字、3 公差
    pub content_type: f64,
    /// 文字を枠で囲む（292）
    pub has_text_frame: bool,
    /// 左・右に接続する位置（173・95）。5・6・8 は下線を引く
    pub text_left_attachment_type: f64,
    pub text_right_attachment_type: f64,
    /// 上・下に接続する位置（273・272）。10 は線を引く
    pub text_top_attachment_type: f64,
    pub text_bottom_attachment_type: f64,
    pub arrow_heads: Vec<DxfMLeaderArrowHead>,
    pub block_attribs: Vec<DxfMLeaderAttrib>,
    pub context: DxfMLeaderContext,
}

/// マルチライン（DXF Reference「MLINE」）。座標と向きは WCS
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLine {
    /// MLINESTYLE の名前（2）
    pub style_name: String,
    /// MLINESTYLE のハンドル（340）
    pub mlinestyle_handle: Option<String>,
    /// 尺度（40）
    pub scale: f64,
    /// 位置合わせ（70）。0 上、1 ゼロ（中心）、2 下
    pub justification: f64,
    /// 71。1 頂点を持つ、2 閉じる、4 始点の端を描かない、8 終点の端を描かない
    pub flags: f64,
    pub closed: bool,
    /// 要素の数（73）
    pub line_count: f64,
    /// 始点（10）
    pub base_point: Vec3,
    /// 頂点（11）
    pub points: Vec<Vec3>,
    /// 頂点から始まる線分の向き（12）
    pub vertex_directions: Vec<Vec3>,
    /// 頂点の留め継ぎの向き（13）
    pub miter_directions: Vec<Vec3>,
    /// 頂点ごと・要素ごとの線のパラメータ（74・41）。DWG（ezdwg）は持たない
    pub line_params: Option<Vec<Vec<Vec<f64>>>>,
    /// 頂点ごと・要素ごとの塗りのパラメータ（75・42）
    pub fill_params: Option<Vec<Vec<Vec<f64>>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfViewport {
    /// 紙空間の中心
    pub center: Vec3,
    /// 紙空間の幅・高さ
    pub width: f64,
    pub height: f64,
    /// 68。0 は非表示、1 以上は重なりの順
    pub status: f64,
    /// 69。レイアウトの全体を表すビューポートは 1
    pub id: f64,
    /// 表示の中心（12、DCS）
    pub view_center: Vec2,
    pub view_target: Vec3,
    pub view_direction: Vec3,
    /// 表示の高さ（45、モデルの単位）。縮尺は height / view_height
    pub view_height: f64,
    pub view_twist_angle: f64,
    /// 90
    pub status_flags: f64,
    /// このビューポートだけで凍結したレイヤ（331）
    pub frozen_layer_handles: Vec<String>,
    /// 矩形でない切り抜きの図形（340）
    pub clipping_boundary_handle: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfImage {
    /// WCS
    pub insert: Vec3,
    /// 画素 1 つ分の U・V のベクトル
    pub u_pixel: Vec3,
    pub v_pixel: Vec3,
    /// 画像の画素数（WIPEOUT は 1 × 1）
    pub image_size: Vec2,
    /// 切り抜きの境界（画素の座標。2 点なら矩形）
    pub boundary: Vec<Vec2>,
    /// 70。1 表示、2 回転しないとき表示、4 切り抜き
    pub flags: f64,
    pub clipping: bool,
}

/// 表（ACAD_TABLE）。表示の図形は INSERT と同じくブロック（*T…）で持つ
#[derive(Debug, Clone, PartialEq)]
pub struct DxfTable {
    pub name: String,
    pub insert: Vec3,
    pub xscale: f64,
    pub yscale: f64,
    pub zscale: f64,
    pub rotation: f64,
}

/// 図形の種類ごとの値
#[derive(Debug, Clone, PartialEq)]
pub enum DxfEntityData {
    Line {
        start: Vec3,
        end: Vec3,
    },
    /// OCS
    Circle {
        center: Vec3,
        radius: f64,
    },
    /// OCS。角度は OCS の X 軸から反時計回りの度
    Arc {
        center: Vec3,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
    },
    /// WCS。長軸の端点は中心からの相対、媒介変数はラジアン
    Ellipse {
        center: Vec3,
        major_axis: Vec3,
        ratio: f64,
        start_param: f64,
        end_param: f64,
    },
    Point {
        location: Vec3,
        x_axis_angle: f64,
    },
    LwPolyline(DxfLwPolyline),
    Polyline(DxfPolyline),
    /// SPLINE・HELIX
    Spline(DxfSpline),
    Text(DxfTextBase),
    Attrib {
        text: DxfTextBase,
        tag: String,
        attribute_flags: f64,
    },
    Attdef {
        text: DxfTextBase,
        tag: String,
        prompt: String,
        attribute_flags: f64,
    },
    MText(DxfMText),
    Insert(DxfInsert),
    /// DIMENSION・ARC_DIMENSION・LARGE_RADIAL_DIMENSION
    Dimension(DxfDimension),
    /// HATCH・MPOLYGON
    Hatch(DxfHatch),
    /// SOLID・TRACE・3DFACE。SOLID・TRACE は OCS（面は vtx0 → vtx1 → vtx3 → vtx2）、3DFACE は WCS
    Solid {
        vtx0: Vec3,
        vtx1: Vec3,
        vtx2: Vec3,
        vtx3: Vec3,
        invisible_edges: Option<f64>,
    },
    Leader(DxfLeader),
    /// MULTILEADER・MLEADER
    MLeader(Box<DxfMLeader>),
    MLine(DxfMLine),
    Viewport(DxfViewport),
    /// XLINE・RAY
    XLine {
        start: Vec3,
        unit_vector: Vec3,
    },
    /// IMAGE・WIPEOUT
    Image(DxfImage),
    Table(DxfTable),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfEntity {
    pub base: DxfEntityBase,
    pub data: DxfEntityData,
}

impl DxfEntity {
    pub fn kind(&self) -> &str {
        &self.base.kind
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfLayer {
    pub handle: String,
    pub name: String,
    /// レイヤの色（ACI、正の値）。非表示は off で持つ
    pub color_index: f64,
    pub true_color: Option<f64>,
    /// 凍結（70 の 1）
    pub frozen: bool,
    /// 非表示（色が負）
    pub off: bool,
    pub locked: bool,
    /// 印刷する（290）
    pub plot: bool,
    pub linetype: String,
    pub lineweight: f64,
    pub transparency: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfLinetype {
    pub handle: String,
    pub name: String,
    pub description: String,
    /// 1 周期の長さ（40）
    pub pattern_length: f64,
    /// 正は線、負は空白、0 は点（49）
    pub dashes: Vec<f64>,
    /// 文字・形の要素を持つ（74 が 0 以外）
    pub complex: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfTextStyle {
    pub handle: String,
    pub name: String,
    /// 主のフォントのファイル（3）
    pub font: String,
    /// ビッグフォントのファイル（4）
    pub bigfont: String,
    /// 固定の高さ（40）。0 なら図形の高さ
    pub height: f64,
    pub width: f64,
    pub oblique: f64,
    pub flags: f64,
    pub generation_flags: f64,
    /// TrueType のフォント名（拡張データ ACAD の 1000）
    pub font_family: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfDimStyle {
    pub handle: String,
    pub name: String,
    /// 全体の尺度（DIMSCALE）
    pub dimscale: f64,
    /// 矢印の大きさ（DIMASZ）
    pub dimasz: f64,
    /// 寸法値の高さ（DIMTXT）
    pub dimtxt: f64,
}

/// MLINE の要素（MLINESTYLE の 49・62・6）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLineStyleElement {
    /// 中心線からのずれ（尺度を掛ける前）
    pub offset: f64,
    /// ACI。0 は BYBLOCK、256 は BYLAYER
    pub color: f64,
    pub linetype: String,
}

/// マルチラインのスタイル（OBJECTS の MLINESTYLE）
#[derive(Debug, Clone, PartialEq)]
pub struct DxfMLineStyle {
    pub handle: String,
    pub name: String,
    /// 70。1 塗り、2 留め継ぎ、16・32・64 始点の線・内側の弧・外側の弧、256・512・1024 終点の線・内側の弧・外側の弧
    pub flags: f64,
    pub description: String,
    /// 塗りの色（62、ACI）
    pub fill_color: f64,
    /// 始点・終点の端の角度（51・52、度）
    pub start_angle: f64,
    pub end_angle: f64,
    pub elements: Vec<DxfMLineStyleElement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfBlock {
    pub handle: String,
    pub name: String,
    pub base_point: Vec3,
    /// 70。1 無名、2 属性を持つ、4 外部参照
    pub flags: f64,
    pub anonymous: bool,
    /// モデル空間・紙空間のブロック
    pub layout: bool,
    /// BLOCK_RECORD のハンドル
    pub block_record_handle: Option<String>,
    pub xref_path: Option<String>,
    pub description: Option<String>,
}

/// レイアウト（LAYOUT と PLOTSETTINGS）。R12 の紙空間は Layout1 として持つ
#[derive(Debug, Clone, PartialEq)]
pub struct DxfLayout {
    pub handle: String,
    pub name: String,
    pub tab_order: f64,
    pub model: bool,
    /// 現在の紙空間（*Paper_Space）か
    pub active: bool,
    /// レイアウトのブロックの名前（*Model_Space、*Paper_Space、*Paper_Space0 …）
    pub block: String,
    pub block_record_handle: Option<String>,
    /// 用紙の大きさ（mm）
    pub paper_width: f64,
    pub paper_height: f64,
    /// 余白（mm）。左・下・右・上
    pub margins: [f64; 4],
    /// 用紙の名前（4）
    pub paper_size: String,
    /// 印刷の単位（72）。0 インチ、1 mm
    pub paper_units: f64,
    /// 印刷の回転（73）。0〜3 で 90 度ずつ
    pub plot_rotation: f64,
    /// 印刷の尺度（142・143）。用紙の単位 / 図面の単位
    pub plot_scale: Vec2,
    /// 印刷の原点のずれ（46・47、mm）
    pub plot_origin: Vec2,
    /// 紙空間の範囲（10・11）
    pub limmin: Vec2,
    pub limmax: Vec2,
}

/// ヘッダ変数の値。座標は数の並び
#[derive(Debug, Clone, PartialEq)]
pub enum DxfHeaderValue {
    Text(String),
    Number(f64),
    Point(Vec<f64>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfHeader {
    /// 長さの単位（$INSUNITS）。0 単位なし、1 インチ、4 mm、6 m など
    pub insunits: f64,
    /// 0 ヤード・ポンド法、1 メートル法（$MEASUREMENT）
    pub measurement: Option<f64>,
    /// 文字コード（$DWGCODEPAGE、ANSI_932 など）
    pub codepage: Option<String>,
    pub ltscale: f64,
    /// 紙空間で線種の尺度をビューポートの縮尺によらずに揃えるか（$PSLTSCALE）
    pub psltscale: f64,
    pub textsize: Option<f64>,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimtxt: f64,
    /// 1 ならモデル空間が開いた状態で保存された（$TILEMODE）
    pub tilemode: f64,
    pub extmin: Option<Vec3>,
    pub extmax: Option<Vec3>,
    pub limmin: Option<Vec2>,
    pub limmax: Option<Vec2>,
    pub plimmin: Option<Vec2>,
    pub plimmax: Option<Vec2>,
    /// すべてのヘッダ変数（名前は $ を付けたまま）。最初に現れた順
    pub variables: Vec<(String, DxfHeaderValue)>,
}

impl DxfHeader {
    pub fn variable(&self, name: &str) -> Option<&DxfHeaderValue> {
        self.variables.iter().find(|(key, _)| key == name).map(|(_, value)| value)
    }
}

/// 文字列の復号の決め方
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingSource {
    Version,
    Codepage,
    Detected,
    Default,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfDocument {
    /// どの形式から読んだか（dxf・dwg）
    pub format: &'static str,
    /// 図面の版（$ACADVER、AC1009〜AC1032）
    pub version: String,
    pub binary: bool,
    /// 文字列の復号に使った文字コード（TextDecoder の名前）と、その決め方
    pub encoding: &'static str,
    pub encoding_source: EncodingSource,
    pub header: DxfHeader,
    pub layers: Vec<DxfLayer>,
    pub linetypes: Vec<DxfLinetype>,
    pub styles: Vec<DxfTextStyle>,
    pub dimstyles: Vec<DxfDimStyle>,
    /// マルチラインのスタイル。DWG（ezdwg）は読まないので None にできる
    pub mline_styles: Option<Vec<DxfMLineStyle>>,
    pub blocks: Vec<DxfBlock>,
    pub layouts: Vec<DxfLayout>,
    /// モデル空間・紙空間・ブロック定義のすべての図形
    pub entities: Vec<DxfEntity>,
    /// 描画順（SORTENTSTABLE）。ブロックの名前ごとに、図形のハンドル → 並べ替えのハンドル。
    /// 名前は JavaScript のオブジェクトのキーと同じ順（整数の名前が先）で引く
    pub sort_handles: Vec<(String, HashMap<String, String>)>,
    /// 読まなかった図形の種類と件数（最初に現れた順）
    pub skipped: Vec<(String, u64)>,
    /// 読み取りで省略・代替した内容
    pub notices: Vec<Notice>,
}
