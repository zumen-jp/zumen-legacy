import type { Notice } from "@zumenjs/core";

/**
 * AutoCAD の図面のモデル。DXF の読み取り（parseDxf）と DWG の読み取りで共有し、dxfToScene で表示用の図面にする。
 * 属性の名前は ezdxf の DXF 属性名に合わせ、ezdxf と違う名前を使う ezdwg（高水準 API と WASM 版の JSON）の属性は
 * ezdwg の名前（color_index、linetype_scale、view_center など）にした。ezdwg の図形は名前を変えずに入る。
 * 座標は図面の単位（$INSUNITS）のままで、角度は度。DXF の配列（[x, y, z]）は ezdwg の JSON と同じ形。
 */

export type Vec2 = [number, number];
export type Vec3 = [number, number, number];

/** model: モデル空間、paper: 紙空間（レイアウト）、block: ブロック定義の中 */
export type DxfSpace = "model" | "paper" | "block";

/** すべての図形に共通する属性（DXF Reference「Common Group Codes for Entities」） */
export interface DxfEntityBase {
  /** 図形の種類（グループコード 0） */
  type: string;
  /** ハンドル（5、16 進の大文字）。ハンドルを持たない R12 の図面では空文字 */
  handle: string;
  /** 所有者（330）。ブロック定義・レイアウトの BLOCK_RECORD のハンドル */
  owner?: string;
  space: DxfSpace;
  /** 属するブロック定義・紙空間のブロックの名前（space が block・paper のとき） */
  block?: string;
  layer: string;
  /** 線種の名前（6）。BYLAYER・BYBLOCK・CONTINUOUS を含む */
  linetype: string;
  /** AutoCAD Color Index（62）。0 は BYBLOCK、256 は BYLAYER */
  color_index: number;
  /** トゥルーカラー（420、0xRRGGBB） */
  true_color?: number;
  /** 透過度（440、0x020000TT は 255 − TT が透明さ） */
  transparency?: number;
  /** 線の太さ（370、1/100 mm）。-1 は BYLAYER、-2 は BYBLOCK、-3 は既定 */
  lineweight: number;
  /** 線種の尺度（48） */
  linetype_scale: number;
  /** 非表示（60） */
  invisible: boolean;
  /** 厚さ（39） */
  thickness?: number;
  /** 押し出し方向（210）。OCS の Z 軸。省略は (0, 0, 1) */
  extrusion?: Vec3;
}

export interface DxfLine extends DxfEntityBase {
  type: "LINE";
  start: Vec3;
  end: Vec3;
}

export interface DxfCircle extends DxfEntityBase {
  type: "CIRCLE";
  /** OCS */
  center: Vec3;
  radius: number;
}

export interface DxfArc extends DxfEntityBase {
  type: "ARC";
  /** OCS */
  center: Vec3;
  radius: number;
  /** OCS の X 軸から反時計回りの度 */
  start_angle: number;
  end_angle: number;
}

export interface DxfEllipse extends DxfEntityBase {
  type: "ELLIPSE";
  /** WCS */
  center: Vec3;
  /** 長軸の端点（中心からの相対、WCS） */
  major_axis: Vec3;
  /** 短軸 / 長軸 */
  ratio: number;
  /** 媒介変数（ラジアン） */
  start_param: number;
  end_param: number;
}

export interface DxfPoint extends DxfEntityBase {
  type: "POINT";
  location: Vec3;
  x_axis_angle?: number;
}

/** LWPOLYLINE と POLYLINE に共通する頂点の並び。bulges・widths は頂点と同じ数 */
export interface DxfVertexList {
  /** LWPOLYLINE・2D の POLYLINE は OCS、3D の POLYLINE は WCS */
  points: Vec3[];
  /** 頂点から次の頂点への弧の膨らみ（tan(中心角 / 4)、負なら時計回り） */
  bulges: number[];
  /** 頂点ごとの始めと終わりの幅 */
  widths: Vec2[];
  closed: boolean;
  flags: number;
}

export interface DxfLwPolyline extends DxfEntityBase, DxfVertexList {
  type: "LWPOLYLINE";
  /** すべての線分の幅（43） */
  const_width?: number;
  elevation?: number;
}

export interface DxfPolyline extends DxfEntityBase, DxfVertexList {
  type: "POLYLINE";
  /** 頂点の種類（VERTEX の 70） */
  vertex_flags: number[];
  /** ポリフェースメッシュの面（頂点の番号、1 から。負は見えない辺） */
  faces?: number[][];
  /** ポリゴンメッシュの M・N の数 */
  m_count?: number;
  n_count?: number;
}

export interface DxfSpline extends DxfEntityBase {
  type: "SPLINE" | "HELIX";
  degree: number;
  flags: number;
  closed: boolean;
  periodic: boolean;
  rational: boolean;
  knots: number[];
  /** WCS */
  control_points: Vec3[];
  weights: number[];
  fit_points: Vec3[];
  start_tangent?: Vec3;
  end_tangent?: Vec3;
}

/** TEXT・ATTRIB・ATTDEF に共通する 1 行の文字 */
export interface DxfTextBase {
  /** 文字列。%%c などの制御コードを含んだまま */
  text: string;
  /** 第 1 整列点（OCS） */
  insert: Vec3;
  /** 第 2 整列点（OCS）。左寄せ・ベースライン以外のとき */
  align_point?: Vec3;
  height: number;
  rotation: number;
  /** 斜体の角度（度） */
  oblique: number;
  /** 幅の係数 */
  width: number;
  style: string;
  /** 0 左、1 中央、2 右、3 両端揃え、4 中心、5 フィット */
  halign: number;
  /** 0 ベースライン、1 下、2 中、3 上 */
  valign: number;
  /** 2 は左右反転、4 は上下反転 */
  text_generation_flag: number;
}

export interface DxfText extends DxfEntityBase, DxfTextBase {
  type: "TEXT";
}

export interface DxfAttrib extends DxfEntityBase, DxfTextBase {
  type: "ATTRIB";
  tag: string;
  /** 1 は非表示、2 は定数、4 は確認、8 は既定値 */
  attribute_flags: number;
}

export interface DxfAttdef extends DxfEntityBase, DxfTextBase {
  type: "ATTDEF";
  tag: string;
  prompt: string;
  attribute_flags: number;
}

export interface DxfMText extends DxfEntityBase {
  type: "MTEXT";
  /** 書式コードを除いた文字列（\P は改行） */
  text: string;
  /** 書式コードを含む元の文字列 */
  raw_text: string;
  /** WCS */
  insert: Vec3;
  char_height: number;
  /** 折り返しの幅（41）。0 なら折り返さない */
  rect_width: number;
  /** 1〜9（左上・中上・右上・左中…右下） */
  attachment_point: number;
  drawing_direction: number;
  /** 文字の進む向き（11、WCS）。ないときは rotation */
  text_direction?: Vec3;
  rotation: number;
  style: string;
  line_spacing_style: number;
  line_spacing_factor: number;
  /** 背景の塗り（90）。1 は色で、2 は画面の背景色で塗る */
  background_flags: number;
  background_color_index?: number;
  background_true_color?: number;
  /** 背景の枠の倍率（45） */
  background_scale?: number;
}

export interface DxfInsert extends DxfEntityBase {
  type: "INSERT";
  /** ブロックの名前 */
  name: string;
  /** OCS */
  insert: Vec3;
  xscale: number;
  yscale: number;
  zscale: number;
  rotation: number;
  /** 配列（MINSERT）の列・行の数と間隔 */
  column_count: number;
  row_count: number;
  column_spacing: number;
  row_spacing: number;
  /** 配置に続く属性（ATTRIB）。位置はブロックの中ではなく、配置と同じ座標系 */
  attribs: DxfAttrib[];
}

export interface DxfDimension extends DxfEntityBase {
  type: "DIMENSION" | "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION";
  /** 表示の図形を持つブロック（2、*D1 など） */
  anonymous_block_name: string;
  /** 寸法の種類（70）。下位 3 ビットが 0 回転・1 平行・2 角度・3 直径・4 半径・5 3 点角度・6 座標 */
  dimtype: number;
  /** 定義点（10、WCS） */
  defpoint: Vec3;
  /** 13・14・15・16 の定義点（WCS） */
  defpoint2?: Vec3;
  defpoint3?: Vec3;
  defpoint4?: Vec3;
  defpoint5?: Vec3;
  /** 寸法値の中点（11、OCS） */
  text_midpoint: Vec3;
  /** ブロックを置く位置（12、OCS） */
  insert?: Vec3;
  /** 利用者の文字（1）。空や <> は計測値 */
  text: string;
  actual_measurement?: number;
  attachment_point?: number;
  text_rotation?: number;
  horizontal_direction?: number;
  /** 寸法線の回転（50、回転寸法） */
  angle?: number;
  /** 寸法補助線の角度（52） */
  oblique_angle?: number;
  dimstyle: string;
}

export interface DxfHatchEdge {
  /** line・arc・ellipse・spline（72 の 1〜4） */
  kind: "line" | "arc" | "ellipse" | "spline";
  start?: Vec2;
  end?: Vec2;
  center?: Vec2;
  radius?: number;
  /** 楕円の長軸の端点（中心からの相対） */
  major_axis?: Vec2;
  /** 短軸 / 長軸 */
  ratio?: number;
  /** 弧の角度（度）。楕円は媒介変数の角度 */
  start_angle?: number;
  end_angle?: number;
  ccw?: boolean;
  degree?: number;
  rational?: boolean;
  periodic?: boolean;
  knots?: number[];
  control_points?: Vec2[];
  weights?: number[];
  fit_points?: Vec2[];
}

/** HATCH の境界（DXF Reference「Boundary Path Data」）。座標は OCS */
export interface DxfHatchPath {
  /** 92 の値。1 外側、2 ポリライン、16 最も外側 */
  flags: number;
  closed: boolean;
  /** ポリラインの境界の頂点。辺の境界では空 */
  points: Vec3[];
  bulges?: number[];
  /** 線・弧・楕円弧・スプラインの辺の境界 */
  edges?: DxfHatchEdge[];
}

/** 模様の線（DXF Reference「Pattern Data」）。角度・尺度を適用した後の値 */
export interface DxfPatternLine {
  angle: number;
  base: Vec2;
  offset: Vec2;
  /** 正は線、負は空白、0 は点 */
  dashes: number[];
}

export interface DxfHatch extends DxfEntityBase {
  type: "HATCH" | "MPOLYGON";
  pattern_name: string;
  solid_fill: boolean;
  associative: boolean;
  /** 0 交互（奇偶）、1 最も外側、2 無視 */
  hatch_style: number;
  pattern_type: number;
  pattern_angle: number;
  pattern_scale: number;
  pattern_double: boolean;
  /** OCS の Z */
  elevation: number;
  paths: DxfHatchPath[];
  pattern_lines: DxfPatternLine[];
  /** グラデーションの名前（470）。グラデーションでなければ undefined */
  gradient?: string;
  /** グラデーションの色（421 のトゥルーカラー） */
  gradient_colors?: number[];
}

export interface DxfSolid extends DxfEntityBase {
  type: "SOLID" | "TRACE" | "3DFACE";
  /** SOLID・TRACE は OCS、3DFACE は WCS。SOLID・TRACE の面は vtx0 → vtx1 → vtx3 → vtx2 */
  vtx0: Vec3;
  vtx1: Vec3;
  vtx2: Vec3;
  vtx3: Vec3;
  /** 3DFACE の見えない辺（70） */
  invisible_edges?: number;
}

export interface DxfLeader extends DxfEntityBase {
  type: "LEADER";
  /** WCS */
  points: Vec3[];
  /** 0 直線、1 スプライン */
  path_type: number;
  annotation_type: number;
  has_arrowhead: boolean;
  dimstyle: string;
}

export interface DxfViewport extends DxfEntityBase {
  type: "VIEWPORT";
  /** 紙空間の中心 */
  center: Vec3;
  /** 紙空間の幅・高さ */
  width: number;
  height: number;
  /** 68。0 は非表示、1 以上は重なりの順（1 が現在のビューポート） */
  status: number;
  /** 69。レイアウトの全体を表すビューポートは 1 */
  id: number;
  /** 表示の中心（12、DCS） */
  view_center: Vec2;
  view_target: Vec3;
  view_direction: Vec3;
  /** 表示の高さ（45、モデルの単位）。縮尺は height / view_height */
  view_height: number;
  view_twist_angle: number;
  /** 90 */
  status_flags: number;
  /** このビューポートだけで凍結したレイヤ（331） */
  frozen_layer_handles: string[];
  /** 矩形でない切り抜きの図形（340） */
  clipping_boundary_handle?: string;
}

export interface DxfXLine extends DxfEntityBase {
  type: "XLINE" | "RAY";
  start: Vec3;
  unit_vector: Vec3;
}

export interface DxfImage extends DxfEntityBase {
  type: "IMAGE" | "WIPEOUT";
  /** WCS */
  insert: Vec3;
  /** 画素 1 つ分の U・V のベクトル */
  u_pixel: Vec3;
  v_pixel: Vec3;
  /** 画像の画素数（WIPEOUT は 1 × 1） */
  image_size: Vec2;
  /** 切り抜きの境界（画素の座標。2 点なら矩形） */
  boundary: Vec2[];
  /** 70。1 表示、2 回転しないとき表示、4 切り抜き */
  flags: number;
  clipping: boolean;
}

/** 表（ACAD_TABLE）。表示の図形は INSERT と同じくブロック（*T…）で持つ */
export interface DxfTable extends DxfEntityBase {
  type: "ACAD_TABLE";
  name: string;
  insert: Vec3;
  xscale: number;
  yscale: number;
  zscale: number;
  rotation: number;
}

export type DxfEntity =
  | DxfLine
  | DxfCircle
  | DxfArc
  | DxfEllipse
  | DxfPoint
  | DxfLwPolyline
  | DxfPolyline
  | DxfSpline
  | DxfText
  | DxfAttrib
  | DxfAttdef
  | DxfMText
  | DxfInsert
  | DxfDimension
  | DxfHatch
  | DxfSolid
  | DxfLeader
  | DxfViewport
  | DxfXLine
  | DxfImage
  | DxfTable;

export interface DxfLayer {
  handle: string;
  name: string;
  /** レイヤの色（ACI、正の値）。非表示は off で持つ */
  color_index: number;
  true_color?: number;
  /** 凍結（70 の 1） */
  frozen: boolean;
  /** 非表示（色が負） */
  off: boolean;
  locked: boolean;
  /** 印刷する（290） */
  plot: boolean;
  linetype: string;
  lineweight: number;
  transparency?: number;
}

export interface DxfLinetype {
  handle: string;
  name: string;
  description: string;
  /** 1 周期の長さ（40） */
  pattern_length: number;
  /** 正は線、負は空白、0 は点（49） */
  dashes: number[];
  /** 文字・形の要素を持つ（74 が 0 以外） */
  complex: boolean;
}

export interface DxfTextStyle {
  handle: string;
  name: string;
  /** 主のフォントのファイル（3）。txt.shx、arial.ttf など */
  font: string;
  /** ビッグフォントのファイル（4）。extfont.shx など */
  bigfont: string;
  /** 固定の高さ（40）。0 なら図形の高さ */
  height: number;
  width: number;
  oblique: number;
  flags: number;
  generation_flags: number;
  /** TrueType のフォント名（拡張データ ACAD の 1000） */
  font_family?: string;
}

export interface DxfDimStyle {
  handle: string;
  name: string;
  /** 全体の尺度（DIMSCALE） */
  dimscale: number;
  /** 矢印の大きさ（DIMASZ） */
  dimasz: number;
  /** 寸法値の高さ（DIMTXT） */
  dimtxt: number;
}

export interface DxfBlock {
  handle: string;
  name: string;
  base_point: Vec3;
  /** 70。1 無名、2 属性を持つ、4 外部参照 */
  flags: number;
  anonymous: boolean;
  /** モデル空間・紙空間のブロック */
  layout: boolean;
  /** BLOCK_RECORD のハンドル */
  block_record_handle?: string;
  xref_path?: string;
  description?: string;
}

/** レイアウト（LAYOUT と PLOTSETTINGS）。R12 の紙空間は Layout1 として持つ */
export interface DxfLayout {
  handle: string;
  name: string;
  tab_order: number;
  model: boolean;
  /** 現在の紙空間（*Paper_Space）か */
  active: boolean;
  /** レイアウトのブロックの名前（*Model_Space、*Paper_Space、*Paper_Space0 …） */
  block: string;
  block_record_handle?: string;
  /** 用紙の大きさ（mm） */
  paper_width: number;
  paper_height: number;
  /** 余白（mm）。左・下・右・上 */
  margins: [number, number, number, number];
  /** 用紙の名前（4） */
  paper_size: string;
  /** 印刷の単位（72）。0 インチ、1 mm */
  paper_units: number;
  /** 印刷の回転（73）。0〜3 で 90 度ずつ */
  plot_rotation: number;
  /** 印刷の尺度（142・143）。用紙の単位 / 図面の単位 */
  plot_scale: Vec2;
  /** 印刷の原点のずれ（46・47、mm） */
  plot_origin: Vec2;
  /** 紙空間の範囲（10・11） */
  limmin: Vec2;
  limmax: Vec2;
}

export interface DxfHeader {
  /** 長さの単位（$INSUNITS）。0 単位なし、1 インチ、4 mm、6 m など */
  insunits: number;
  /** 0 ヤード・ポンド法、1 メートル法（$MEASUREMENT） */
  measurement?: number;
  /** 文字コード（$DWGCODEPAGE、ANSI_932 など） */
  codepage?: string;
  ltscale: number;
  /** 紙空間で線種の尺度をビューポートの縮尺によらずに揃えるか（$PSLTSCALE） */
  psltscale: number;
  textsize?: number;
  dimscale: number;
  dimasz: number;
  dimtxt: number;
  /** 1 ならモデル空間が開いた状態で保存された（$TILEMODE） */
  tilemode: number;
  extmin?: Vec3;
  extmax?: Vec3;
  limmin?: Vec2;
  limmax?: Vec2;
  plimmin?: Vec2;
  plimmax?: Vec2;
  /** すべてのヘッダ変数（名前は $ を付けたまま）。座標は配列 */
  variables: Record<string, string | number | number[]>;
}

export interface DxfDocument {
  /** どの形式から読んだか */
  format: "dxf" | "dwg";
  /** 図面の版（$ACADVER、AC1009〜AC1032） */
  version: string;
  binary: boolean;
  /** 文字列の復号に使った文字コード（TextDecoder の名前）と、その決め方 */
  encoding: string;
  encoding_source: "version" | "codepage" | "detected" | "default";
  header: DxfHeader;
  layers: DxfLayer[];
  linetypes: DxfLinetype[];
  styles: DxfTextStyle[];
  dimstyles: DxfDimStyle[];
  blocks: DxfBlock[];
  layouts: DxfLayout[];
  /** モデル空間・紙空間・ブロック定義のすべての図形。space と block で区別する */
  entities: DxfEntity[];
  /** 描画順（SORTENTSTABLE）。ブロックの名前ごとに、図形のハンドル → 並べ替えのハンドル */
  sort_handles: Record<string, Record<string, string>>;
  /** 読まなかった図形の種類と件数 */
  skipped: Record<string, number>;
  /** 読み取りで省略・代替した内容 */
  notices: Notice[];
}
