/** 全体座標系（mm、Z が鉛直上向き）の点 */
export type Vec3 = readonly [number, number, number];

/** 平面上の点（mm） */
export type Vec2 = readonly [number, number];

export interface Box3 {
  min: Vec3;
  max: Vec3;
}

/** 部材の種類。ST-Bridge の要素との対応は `MEMBER_ELEMENTS` を参照 */
export type MemberKind =
  | "column"
  | "post"
  | "girder"
  | "beam"
  | "brace"
  | "slab"
  | "wall"
  | "footing"
  | "stripFooting"
  | "pile"
  | "foundationColumn"
  | "parapet";

export const MEMBER_KINDS: readonly MemberKind[] = [
  "column",
  "post",
  "girder",
  "beam",
  "brace",
  "slab",
  "wall",
  "footing",
  "stripFooting",
  "pile",
  "foundationColumn",
  "parapet",
];

export const MEMBER_LABELS: Record<MemberKind, string> = {
  column: "柱",
  post: "間柱",
  girder: "大梁",
  beam: "小梁",
  brace: "ブレース",
  slab: "スラブ",
  wall: "壁",
  footing: "フーチング",
  stripFooting: "布基礎",
  pile: "杭",
  foundationColumn: "基礎柱",
  parapet: "パラペット",
};

/** 断面の輪郭（閉じた多角形）。穴は外周の内側にある */
export interface SolidSection {
  outer: Vec3[];
  holes: Vec3[][];
}

/** 断面の 2D の輪郭（b, h）。同じ断面の部材の間で共有する */
export interface ProfileLoop {
  outer: Vec2[];
  holes: Vec2[][];
}

/**
 * 断面の 2D の輪郭を材軸に沿って押し出した立体（柱・梁・ブレースなど）。
 * 輪郭の点 (b, h) は start（終端は end）+ b·e1 + h·e2 に置く。変断面は始端・終端の輪郭の頂点数を揃える。
 */
export interface Extrusion {
  type: "extrusion";
  start: Vec3;
  end: Vec3;
  e1: Vec3;
  e2: Vec3;
  startLoop: ProfileLoop;
  endLoop: ProfileLoop;
  /** 側面を滑らかに陰影づけする（円形の断面） */
  smooth: boolean;
}

/**
 * 3D の輪郭を順につないだ立体（スラブ・壁・基礎）。先頭と末尾の輪郭に蓋をする。
 * 輪郭の頂点数（穴を含む）は列の間で揃え、対応する頂点を側面でつなぐ。
 */
export interface Loft {
  type: "loft";
  sections: SolidSection[];
  smooth: boolean;
}

export type Solid = Extrusion | Loft;

/** 厚さを持たない面（荷重用のスラブ・壁） */
export interface Face {
  outer: Vec3[];
  holes: Vec3[][];
}

export interface Property {
  label: string;
  value: string;
}

export interface StructureNode {
  id: number;
  position: Vec3;
  kind: string;
}

export interface Story {
  id: number;
  name: string;
  /** 代表高さ（GL からの FL、mm） */
  height: number;
  kind: string;
  levelName: string | null;
  strengthConcrete: string | null;
}

/** 直線の通り芯（平行軸・放射軸・作図用直線軸）。XY 平面の線分を、全体の範囲に合わせて伸ばした端点で持つ */
export interface LineAxis {
  kind: "parallel" | "radial" | "drawingLine";
  key: string;
  group: string;
  name: string;
  start: Vec2;
  end: Vec2;
  /** 通りに属する節点（ST-Bridge の StbNodeIdList） */
  nodeIds: number[];
}

/** 円弧の通り芯（円弧軸・作図用円弧軸） */
export interface ArcAxis {
  kind: "arc" | "drawingArc";
  key: string;
  group: string;
  name: string;
  center: Vec2;
  radius: number;
  /** 開始・終了角（度、反時計回り）。円のときは 0〜360 */
  startAngle: number;
  endAngle: number;
  nodeIds: number[];
}

/** 通り芯。平面の図形と、部材がその通りにあるかの判定に使う情報を持つ */
export type GridAxis = LineAxis | ArcAxis;

export const isArcAxis = (axis: GridAxis): axis is ArcAxis => axis.kind === "arc" || axis.kind === "drawingArc";

export interface SectionSummary {
  /** 断面の要素名（StbSecColumn_RC など） */
  element: string;
  id: number;
  name: string;
  /** 断面形状の表示（「H-400x200x8x13」「600×600」など）。区間ごとに 1 つ */
  shapes: string[];
}

export interface StructureMember {
  /** 一覧の中で一意な鍵（要素名と ID） */
  key: string;
  kind: MemberKind;
  /** ST-Bridge の要素名（StbColumn など） */
  element: string;
  id: number;
  guid: string | null;
  /** 符号（name 属性）。ないときは空文字 */
  name: string;
  /** 構造種別（kind_structure） */
  structure: string;
  storyId: number | null;
  section: SectionSummary | null;
  solids: Solid[];
  faces: Face[];
  /** 断面を描けない部材の配置基準線 */
  lines: Vec3[][];
  /** 部材を定める節点 */
  nodeIds: number[];
  bounds: Box3;
  properties: Property[];
  /** 元の XML の属性（部材と断面） */
  raw: { label: string; attributes: Record<string, string> }[];
}

export type NoticeLevel = "info" | "warning";

/** 描かない・推測で補った内容。種類ごとに件数を数える */
export interface Notice {
  code: string;
  level: NoticeLevel;
  message: string;
  count: number;
}

export interface StructureModel {
  /** ST_BRIDGE の version 属性 */
  version: string;
  encoding: string;
  nodes: StructureNode[];
  stories: Story[];
  axes: GridAxis[];
  members: StructureMember[];
  bounds: Box3;
  metadata: Property[];
  notices: Notice[];
}
