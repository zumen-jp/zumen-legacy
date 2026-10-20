/**
 * 形式に依存しない表示用の図面モデル。
 * 座標の単位は用紙上の mm で、Y 軸は上向き。部分図や部品の配置は解決済みの値を持つ。
 */

/** 形式の名前（sfc、p21、jww など）。形式ごとの読み取りが決める */
export type DrawingFormat = string;

/** 描画時に背景色で塗る色（白抜きの矢印など） */
export const BACKGROUND_COLOR = "background";

/** 外接矩形。図形がない場合は minX > maxX になる。 */
export interface Box {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/**
 * 線の描き方。太さ・破線の長さは用紙上の mm。
 * 色は CSS の色で、#rrggbb か、不透明度つきの #rrggbbaa を使う（背景に合わせた色の反転はこの 2 つだけを見る）。
 */
export interface StrokeStyle {
  color: string;
  width: number;
  /** 線分・空白の長さの繰り返し。実線は空配列 */
  dashes: readonly number[];
}

/** 折線。points は x, y を交互に並べる。segments なら 2 点ずつ別の線分（ハッチング線など） */
export interface StrokePrimitive extends StrokeStyle {
  kind: "stroke";
  points: Float64Array;
  closed: boolean;
  segments?: boolean;
}

/** 塗り。rings は外形と中抜きで、偶奇規則で塗る */
export interface FillPrimitive {
  kind: "fill";
  color: string;
  rings: Float64Array[];
}

/** 領域で切り取って描く線（ハッチング線・パターン） */
export interface ClippedPrimitive {
  kind: "clipped";
  rings: Float64Array[];
  strokes: StrokePrimitive[];
}

/** 画面上で常に同じ大きさの点（SXF の点マーカ dot） */
export interface DotPrimitive {
  kind: "dot";
  x: number;
  y: number;
  color: string;
}

/** 1 文字分の配置。文字の座標系での中心位置 */
export interface GlyphCell {
  char: string;
  x: number;
  y: number;
}

/**
 * 文字列。matrix は文字の座標系（右が文字の進む向き、上が文字の上、単位は文字の座標の mm）から
 * 用紙の座標への変換 [a, b, c, d, e, f]（x' = a x + c y + e, y' = b x + d y + f）。
 */
export interface TextPrimitive {
  kind: "text";
  text: string;
  color: string;
  /** CSS の font-family */
  fontFamily: string;
  /** 文字の座標系での 1 文字の高さ */
  height: number;
  /** 字形の横方向の倍率（文字範囲幅に合わせる） */
  scaleX: number;
  cells: GlyphCell[];
  /** 文字範囲の幅と高さ（文字の座標系）。左下が原点 */
  boxWidth: number;
  boxHeight: number;
  matrix: readonly [number, number, number, number, number, number];
}

export type Primitive = StrokePrimitive | FillPrimitive | ClippedPrimitive | DotPrimitive | TextPrimitive;

/**
 * 図形の種類の、形式によらない英語のキー。どの形式の図形がどれになるかは docs/architecture.md の
 * 「Stable keys」の表のとおり。形式での名前は SceneEntity.source、日本語の名前は SceneEntity.type にある
 */
export const ENTITY_KINDS = [
  "line",
  "construction-line",
  "polyline",
  "multiline",
  "mesh",
  "arc",
  "circle",
  "ellipse",
  "ellipse-arc",
  "spline",
  "clothoid",
  "helix",
  "point",
  "text",
  "attribute",
  "attribute-definition",
  "dimension",
  "leader",
  "balloon",
  "hatch",
  "solid",
  "symbol",
  "image",
  "wipeout",
  "viewport",
] as const;

export type EntityKind = (typeof ENTITY_KINDS)[number];

/** 整形する前の属性の値。座標は [x, y]、範囲は [始め, 終わり] のような数の並び */
export type PropertyRaw = number | string | boolean | readonly number[];

/**
 * 図形・図面の属性。label と value は人に見せる日本語と整形した文字列で、アプリケーションは key で属性を選ぶ。
 * key の一覧と、raw の形は docs/architecture.md の「Stable keys」の表のとおり
 */
export interface Property {
  /** 形式によらない英語のキー（layer、color、start など）。1 つの図形の中では重ならない */
  key: string;
  label: string;
  value: string;
  /** 整形する前の値（数・座標・形式の番号など）。value が文字列そのものなら省く */
  raw?: PropertyRaw;
}

/** 選択・属性表示の単位。primitives は描く順に並ぶ */
export interface SceneEntity {
  /** 形式内での識別子（SXF はインスタンス番号 #nn、配置の中では配置の番号を前に付ける） */
  id: string;
  /** 形式によらない種類（line、text、dimension など）。アプリケーションはこれで図形を選ぶ */
  kind: EntityKind;
  /** 日本語の種類名（線分、文字、直線寸法など）。人に見せる */
  type: string;
  /** 元の形式での名前（line_feature など） */
  source: string;
  /** 自分と、配置元の部分図・部品のレイヤ。すべて表示のときだけ見える */
  layers: readonly number[];
  primitives: Primitive[];
  bbox: Box;
  properties: Property[];
  /** 部分図・部品の中の図形は、元の座標から用紙への縮尺を持つ（実寸の計測に使う） */
  placement?: Placement;
}

/** 部分図・部品の配置。scaleX・scaleY は元の座標 1 mm が用紙で何 mm になるか */
export interface Placement {
  name: string;
  scaleX: number;
  scaleY: number;
  /** 用紙の座標から元の座標への変換 */
  inverse: readonly [number, number, number, number, number, number];
}

export interface SceneLayer {
  id: number;
  name: string;
  visible: boolean;
  /** JWW のレイヤグループ名など、レイヤの上位の区分 */
  group?: string;
}

export type NoticeLevel = "info" | "warning";

/** 読み取り・描画で省略・代替した内容。count は該当件数 */
export interface Notice {
  level: NoticeLevel;
  code: string;
  message: string;
  count: number;
}

export interface Sheet {
  name: string;
  /** 用紙サイズの名前（A1、自由など） */
  size: string;
  width: number;
  height: number;
}

/** 同じ図面の中の別の表示（DXF・DWG のモデル空間とレイアウトなど）。readDrawing の options.view で選ぶ */
export interface SceneView {
  id: string;
  name: string;
}

export interface Scene {
  format: DrawingFormat;
  /** 形式の版（SXF 3.1、JWW 700 など） */
  version: string;
  /** 選べる表示。1 つしかない形式では省略する */
  views?: SceneView[];
  /** この図面が描いている表示の id（views の 1 つ） */
  view?: string;
  sheet: Sheet | null;
  /** すべての図形の外接矩形（用紙の mm） */
  bounds: Box;
  layers: SceneLayer[];
  entities: SceneEntity[];
  notices: Notice[];
  /** 表題欄・ファイルの情報 */
  metadata: Property[];
  /** 図面の作成時に想定された背景 */
  background: "dark" | "light";
}
