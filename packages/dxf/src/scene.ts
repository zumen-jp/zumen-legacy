import {
  BACKGROUND_COLOR,
  type Box,
  NoticeList,
  type Placement,
  type Primitive,
  type Property,
  type Scene,
  type SceneEntity,
  type SceneLayer,
  type SceneView,
  type Sheet,
  type StrokePrimitive,
} from "@zumenjs/core";
import {
  apply,
  applyVector,
  emptyBox,
  extendBox,
  extendBoxWithPoints,
  invert,
  isEmptyBox,
  type Matrix,
  meanScale,
  multiply,
  transformPoints,
  unionBox,
} from "@zumenjs/core/geometry";
import { fontFamilyFor, isHalfWidth, layoutText } from "@zumenjs/core/text";
import { aciColor, isTransparent, trueColor } from "./colors.js";
import {
  arcPoints,
  bulgePoints,
  clipPolygon,
  clipPolyline,
  ellipsePoints,
  minorAxis,
  nurbsPoints,
  ocsMatrix,
  throughPoints,
} from "./curves.js";
import type {
  DxfAttdef,
  DxfAttrib,
  DxfBlock,
  DxfDimension,
  DxfDimStyle,
  DxfDocument,
  DxfEntity,
  DxfEntityBase,
  DxfHatch,
  DxfHatchPath,
  DxfImage,
  DxfInsert,
  DxfLayer,
  DxfLayout,
  DxfLeader,
  DxfLinetype,
  DxfLwPolyline,
  DxfMText,
  DxfPolyline,
  DxfTable,
  DxfText,
  DxfTextStyle,
  DxfViewport,
  DxfXLine,
  Vec2,
  Vec3,
} from "./model.js";
import { type ParagraphAlign, parseMText, parseTextCodes, type TextRun } from "./mtext.js";

export interface DxfSceneOptions {
  /** 描く空間。"Model"（モデル空間）かレイアウトの名前。省略すると図面を保存したときの空間 */
  layout?: string;
}

/** $INSUNITS の単位の mm（DXF Reference「HEADER Section Group Codes」の $INSUNITS） */
const UNIT_TO_MM: Readonly<Record<number, number>> = {
  1: 25.4,
  2: 304.8,
  3: 1_609_344,
  4: 1,
  5: 10,
  6: 1000,
  7: 1_000_000,
  8: 0.0000254,
  9: 0.0254,
  10: 914.4,
  11: 1e-7,
  12: 1e-6,
  13: 0.001,
  14: 100,
  15: 10_000,
  16: 100_000,
  17: 1e12,
  18: 1.495978707e14,
  19: 9.4607304725808e18,
  20: 3.0856775814914e19,
  21: 1_200_000 / 3937,
};

const UNIT_NAMES: Readonly<Record<number, string>> = {
  0: "単位なし",
  1: "インチ",
  2: "フィート",
  3: "マイル",
  4: "mm",
  5: "cm",
  6: "m",
  7: "km",
  9: "ミル",
  10: "ヤード",
  13: "μm",
  14: "dm",
  21: "US 測量フィート",
};

const RELEASES: Readonly<Record<string, string>> = {
  AC1009: "R12",
  AC1012: "R13",
  AC1014: "R14",
  AC1015: "2000",
  AC1018: "2004",
  AC1021: "2007",
  AC1024: "2010",
  AC1027: "2013",
  AC1032: "2018",
};

const TYPE_NAMES: Readonly<Record<string, string>> = {
  LINE: "線分",
  CIRCLE: "円",
  ARC: "円弧",
  ELLIPSE: "楕円",
  POINT: "点",
  LWPOLYLINE: "ポリライン",
  POLYLINE: "ポリライン",
  SPLINE: "スプライン",
  HELIX: "らせん",
  TEXT: "文字",
  MTEXT: "複数行文字",
  ATTRIB: "属性",
  ATTDEF: "属性定義",
  DIMENSION: "寸法",
  ARC_DIMENSION: "弧長寸法",
  LARGE_RADIAL_DIMENSION: "折り曲げ半径寸法",
  HATCH: "ハッチング",
  MPOLYGON: "塗りの多角形",
  SOLID: "塗り（ソリッド）",
  TRACE: "太線（トレース）",
  "3DFACE": "3D 面",
  LEADER: "引出線",
  VIEWPORT: "ビューポート",
  XLINE: "構築線",
  RAY: "放射線",
  IMAGE: "画像",
  WIPEOUT: "ワイプアウト",
};

const DIMENSION_KINDS = ["長さ寸法", "平行寸法", "角度寸法", "直径寸法", "半径寸法", "3 点角度寸法", "座標寸法"];

/** 弦と弧のずれの許容（表示用の図面の mm）。大きい円は半径に比例して粗くする */
const TOLERANCE = 0.02;
const MAX_DEPTH = 32;
const MAX_PATTERN_LINES = 5000;
const DEFAULT_LINEWEIGHT = 0.25;
/** 文字の高さ（大文字の高さ）に対する字形の em の大きさ。全角を含む文字は漢字の高さを合わせる */
const EM_LATIN = 1 / 0.72;
const EM_CJK = 1 / 0.88;
/** em の下端からベースラインまで（em に対する比） */
const DESCENT = 0.12;
/** MTEXT の既定の行の間隔（文字の高さの 5/3、DXF Reference「MTEXT」の 44） */
const LINE_SPACING = 5 / 3;

interface Ctx {
  /** 今の座標系（ブロックの中など） → 表示用の図面（mm） */
  m: Matrix;
  /** 今の座標系 → その空間の WCS（属性に示す座標） */
  local: Matrix;
  layers: readonly number[];
  /** ブロック参照のレイヤ。ブロックの中のレイヤ 0 の図形が継ぐ。最上位では null */
  parentLayer: string | null;
  /** BYBLOCK の色・線の太さ（mm）・線種 */
  color: string;
  lineweight: number;
  linetype: string;
  path: string;
  blocks: readonly string[];
  depth: number;
  placement: Placement | undefined;
  /** 線種の長さの補正（紙空間の線種の尺度を揃えるビューポート） */
  dashFactor: number;
  /** ビューポートの切り抜き（表示用の図面の座標） */
  clip: Box | null;
  /** ビューポートで凍結したレイヤ（大文字） */
  frozen: ReadonlySet<string> | null;
}

function fmt(value: number): string {
  if (!Number.isFinite(value)) return String(value);
  return formatter.format(Number(value.toFixed(4)));
}

const formatter = new Intl.NumberFormat("ja-JP", { maximumFractionDigits: 4 });

function point2(x: number, y: number): string {
  return `${fmt(x)}, ${fmt(y)}`;
}

function translate(x: number, y: number): Matrix {
  return [1, 0, 0, 1, x, y];
}

function rotate(deg: number): Matrix {
  const r = (deg * Math.PI) / 180;
  const c = Math.cos(r);
  const s = Math.sin(r);
  return [c, s, -s, c, 0, 0];
}

function scale(sx: number, sy: number): Matrix {
  return [sx, 0, 0, sy, 0, 0];
}

function extendPrimitiveBox(box: Box, primitive: Primitive): void {
  switch (primitive.kind) {
    case "stroke":
      extendBoxWithPoints(box, primitive.points);
      return;
    case "fill":
    case "clipped":
      for (const ring of primitive.rings) extendBoxWithPoints(box, ring);
      return;
    case "dot":
      extendBox(box, primitive.x, primitive.y);
      return;
    case "text": {
      const m = primitive.matrix;
      for (const [x, y] of [
        [0, 0],
        [primitive.boxWidth, 0],
        [primitive.boxWidth, primitive.boxHeight],
        [0, primitive.boxHeight],
      ] as const) {
        extendBox(box, m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
      }
    }
  }
}

function inside(inner: Box, outer: Box): boolean {
  return inner.minX >= outer.minX && inner.maxX <= outer.maxX && inner.minY >= outer.minY && inner.maxY <= outer.maxY;
}

function intersects(a: Box, b: Box): boolean {
  return !(a.maxX < b.minX || a.minX > b.maxX || a.maxY < b.minY || a.minY > b.maxY);
}

/** 線種（49）を Canvas の線・空白の並びにする。正は線、負は空白、0 は点 */
export function dashArray(pattern: readonly number[], factor: number): number[] {
  const out: number[] = [];
  if (!(factor > 0)) return out;
  for (const value of pattern) {
    const length = Math.abs(value) * factor;
    const on = value >= 0;
    const expectOn = out.length % 2 === 0;
    if (on === expectOn) out.push(length);
    else if (out.length === 0) out.push(0, length);
    else out[out.length - 1] = (out[out.length - 1] as number) + length;
  }
  // 線で終わる並びは、次の周期の初めの線とつながる
  if (out.length % 2 === 1) {
    const tail = out.pop() as number;
    if (out.length > 0) out[0] = (out[0] as number) + tail;
  }
  if (out.length === 0 || out.every((value, index) => index % 2 === 1 || value === 0)) {
    // 空白だけ・点だけの並びも描けるように残す。空白がない並びは実線
    return out.some((value, index) => index % 2 === 1 && value > 0) ? out : [];
  }
  return out;
}

function units(text: string): number {
  let total = 0;
  for (const char of text) total += isHalfWidth(char) ? 0.5 : 1;
  return total;
}

function emFor(text: string): number {
  for (const char of text) if (!isHalfWidth(char) && char.trim() !== "") return EM_CJK;
  return EM_LATIN;
}

interface BlockContents {
  block: DxfBlock | undefined;
  entities: DxfEntity[];
}

/** ハンドルの大小（16 進の文字列） */
function compareHandles(a: string, b: string): number {
  if (a.length !== b.length) return a.length - b.length;
  return a < b ? -1 : a > b ? 1 : 0;
}

class Builder {
  readonly entities: SceneEntity[] = [];
  readonly usedLayers = new Map<number, number>();
  readonly notices: NoticeList;
  private readonly layerIds = new Map<string, number>();
  private readonly layerList: DxfLayer[] = [];
  private readonly linetypes = new Map<string, DxfLinetype>();
  private readonly styles = new Map<string, DxfTextStyle>();
  private readonly dimstyles = new Map<string, DxfDimStyle>();
  private readonly blocks = new Map<string, BlockContents>();
  readonly model: DxfEntity[] = [];
  private readonly shxFonts = new Set<string>();
  private shxCount = 0;
  private readonly infinite: { entity: DxfXLine; ctx: Ctx }[] = [];

  constructor(readonly doc: DxfDocument) {
    this.notices = new NoticeList(doc.notices);
    for (const layer of doc.layers) this.layer(layer.name, layer);
    for (const linetype of doc.linetypes) this.linetypes.set(linetype.name.toUpperCase(), linetype);
    for (const style of doc.styles) this.styles.set(style.name.toUpperCase(), style);
    for (const style of doc.dimstyles) this.dimstyles.set(style.name.toUpperCase(), style);
    for (const block of doc.blocks) this.blocks.set(block.name.toUpperCase(), { block, entities: [] });
    for (const entity of doc.entities) {
      if (entity.space === "model") {
        this.model.push(entity);
        continue;
      }
      const name = (entity.block ?? "*Paper_Space").toUpperCase();
      let contents = this.blocks.get(name);
      if (!contents) {
        contents = { block: undefined, entities: [] };
        this.blocks.set(name, contents);
      }
      contents.entities.push(entity);
    }
    this.sortByDrawOrder("*MODEL_SPACE", this.model);
    for (const [name, contents] of this.blocks) this.sortByDrawOrder(name, contents.entities);
  }

  /** SORTENTSTABLE の並べ替えのハンドルの順に描く（表にない図形は自分のハンドル） */
  private sortByDrawOrder(blockName: string, list: DxfEntity[]): void {
    const table = Object.entries(this.doc.sort_handles).find(([name]) => name.toUpperCase() === blockName)?.[1];
    if (!table) return;
    const keyed = list.map((entity, index) => ({ entity, index, key: table[entity.handle] ?? entity.handle }));
    keyed.sort((a, b) => (a.key && b.key ? compareHandles(a.key, b.key) : 0) || a.index - b.index);
    list.splice(0, list.length, ...keyed.map((item) => item.entity));
  }

  blockEntities(name: string): DxfEntity[] {
    return this.blocks.get(name.toUpperCase())?.entities ?? [];
  }

  layers(): SceneLayer[] {
    const out: SceneLayer[] = [];
    this.layerList.forEach((layer, index) => {
      const id = index + 1;
      if (!this.usedLayers.has(id)) return;
      out.push({ id, name: layer.name, visible: !layer.off && !layer.frozen });
    });
    return out;
  }

  private layer(name: string, definition?: DxfLayer): number {
    const key = name.toUpperCase();
    let id = this.layerIds.get(key);
    if (id === undefined) {
      this.layerList.push(
        definition ?? {
          handle: "",
          name,
          color_index: 7,
          frozen: false,
          off: false,
          locked: false,
          plot: true,
          linetype: "CONTINUOUS",
          lineweight: -3,
        },
      );
      id = this.layerList.length;
      this.layerIds.set(key, id);
    }
    return id;
  }

  private layerOf(name: string): DxfLayer {
    return this.layerList[this.layer(name) - 1] as DxfLayer;
  }

  /** ブロックの中のレイヤ 0 の図形は、ブロック参照のレイヤに属する */
  effectiveLayer(entity: DxfEntityBase, ctx: Ctx): string {
    return entity.layer === "0" && ctx.parentLayer !== null ? ctx.parentLayer : entity.layer;
  }

  /** 色（#rrggbb）。透過度があれば不透明度を足して #rrggbbaa にする */
  color(entity: DxfEntityBase, ctx: Ctx): string {
    const color = this.baseColor(entity, ctx);
    // 透過度（440）がなければレイヤの透過度。0x020000TT の TT が不透明さ（DXF Reference「Common Group Codes for Entities」）
    const transparency =
      entity.transparency ?? this.layerOf(this.effectiveLayer(entity, ctx)).transparency ?? undefined;
    if (!isTransparent(transparency) || color.length !== 7) return color;
    return `${color}${((transparency as number) & 0xff).toString(16).padStart(2, "0")}`;
  }

  private baseColor(entity: DxfEntityBase, ctx: Ctx): string {
    if (entity.true_color !== undefined) return trueColor(entity.true_color);
    const index = entity.color_index;
    if (index === 0) return ctx.color.slice(0, 7);
    if (index === 256 || index === 257 || index < 0 || index > 257) {
      const layer = this.layerOf(this.effectiveLayer(entity, ctx));
      return layer.true_color !== undefined ? trueColor(layer.true_color) : aciColor(layer.color_index);
    }
    return aciColor(index);
  }

  /** 線の太さ（mm） */
  lineweight(entity: DxfEntityBase, ctx: Ctx): number {
    const value = entity.lineweight;
    if (value === -2) return ctx.lineweight;
    if (value === -1) {
      const layer = this.layerOf(this.effectiveLayer(entity, ctx));
      return layer.lineweight >= 0 ? layer.lineweight / 100 : DEFAULT_LINEWEIGHT;
    }
    return value >= 0 ? value / 100 : DEFAULT_LINEWEIGHT;
  }

  linetypeName(entity: DxfEntityBase, ctx: Ctx): string {
    const name = entity.linetype || "BYLAYER";
    const upper = name.toUpperCase();
    if (upper === "BYBLOCK") return ctx.linetype;
    if (upper === "BYLAYER") return this.layerOf(this.effectiveLayer(entity, ctx)).linetype || "CONTINUOUS";
    return name;
  }

  dashes(entity: DxfEntityBase, ctx: Ctx, m: Matrix): number[] {
    const name = this.linetypeName(entity, ctx);
    const upper = name.toUpperCase();
    if (upper === "CONTINUOUS" || upper === "BYLAYER" || upper === "BYBLOCK") return [];
    const linetype = this.linetypes.get(upper);
    if (!linetype || linetype.dashes.length === 0) return [];
    if (linetype.complex) {
      this.notices.add("info", "complex-linetype", "文字・形を含む線種は、線と空白だけで描きました");
    }
    const factor = this.doc.header.ltscale * entity.linetype_scale * meanScale(m) * ctx.dashFactor;
    return dashArray(linetype.dashes, factor);
  }

  stroke(entity: DxfEntityBase, ctx: Ctx, m: Matrix, points: ArrayLike<number>, closed: boolean): StrokePrimitive {
    return {
      kind: "stroke",
      points: transformPoints(m, points),
      closed,
      color: this.color(entity, ctx),
      width: this.lineweight(entity, ctx),
      dashes: this.dashes(entity, ctx, m),
    };
  }

  /** 弦と弧のずれの許容（m で表示用の図面に写す前の座標） */
  tolerance(m: Matrix, radius: number): number {
    const s = Math.max(meanScale(m), 1e-12);
    return Math.max(TOLERANCE, radius * s * 1e-4) / s;
  }

  /** 子の文脈（ブロック参照・寸法の中）。BYBLOCK の値とレイヤ 0 の継ぎ先を entity から決める */
  child(entity: DxfEntityBase, ctx: Ctx, local: Matrix, path: string, block: string): Ctx {
    const layer = this.effectiveLayer(entity, ctx);
    const id = this.layer(layer);
    return {
      ...ctx,
      m: multiply(ctx.m, local),
      local: multiply(ctx.local, local),
      layers: ctx.layers.includes(id) ? ctx.layers : [...ctx.layers, id],
      parentLayer: layer,
      color: this.color(entity, ctx),
      lineweight: this.lineweight(entity, ctx),
      linetype: this.linetypeName(entity, ctx),
      path,
      blocks: [...ctx.blocks, block],
      depth: ctx.depth + 1,
    };
  }

  private colorLabel(entity: DxfEntityBase): string {
    if (entity.true_color !== undefined) return trueColor(entity.true_color);
    if (entity.color_index === 0) return "BYBLOCK";
    if (entity.color_index === 256) return "BYLAYER";
    return `ACI ${entity.color_index}`;
  }

  private lineweightLabel(value: number): string {
    if (value === -1) return "BYLAYER";
    if (value === -2) return "BYBLOCK";
    if (value === -3) return "既定";
    return `${fmt(value / 100)} mm`;
  }

  /** 表示用の図面の要素を加える。ビューポートの中では、切り抜きの矩形で線・塗りを切る */
  add(
    entity: DxfEntityBase,
    ctx: Ctx,
    primitives: Primitive[],
    properties: Property[],
    type = TYPE_NAMES[entity.type] ?? entity.type,
  ): void {
    if (primitives.length === 0) return;
    let parts = primitives;
    let bbox = emptyBox();
    for (const primitive of parts) extendPrimitiveBox(bbox, primitive);
    if (isEmptyBox(bbox)) return;
    if (ctx.clip) {
      if (!intersects(bbox, ctx.clip)) return;
      if (!inside(bbox, ctx.clip)) {
        parts = this.clip(parts, ctx.clip);
        if (parts.length === 0) return;
        bbox = emptyBox();
        for (const primitive of parts) extendPrimitiveBox(bbox, primitive);
        if (isEmptyBox(bbox)) return;
      }
    }
    const layer = this.effectiveLayer(entity, ctx);
    const own = this.layer(layer);
    const layers = ctx.layers.includes(own) ? ctx.layers : [...ctx.layers, own];
    for (const id of layers) this.usedLayers.set(id, (this.usedLayers.get(id) ?? 0) + 1);
    const id = `${ctx.path}${entity.handle || "?"}`;
    const linetype = entity.linetype || "BYLAYER";
    const resolved = this.linetypeName(entity, ctx);
    const common: Property[] = [
      { label: "種類", value: `${type}（${entity.type}）` },
      { label: "ハンドル", value: id },
      { label: "レイヤ", value: layer },
    ];
    if (ctx.blocks.length > 0) common.push({ label: "ブロック", value: ctx.blocks.join(" › ") });
    common.push(
      { label: "色", value: this.colorLabel(entity) },
      {
        label: "線種",
        value: linetype.toUpperCase() === resolved.toUpperCase() ? linetype : `${linetype}（${resolved}）`,
      },
      { label: "線の太さ", value: this.lineweightLabel(entity.lineweight) },
    );
    const sceneEntity: SceneEntity = {
      id,
      type,
      source: entity.type,
      layers,
      primitives: parts,
      bbox,
      properties: [...common, ...properties],
    };
    if (ctx.placement) sceneEntity.placement = ctx.placement;
    this.entities.push(sceneEntity);
  }

  private clip(primitives: Primitive[], box: Box): Primitive[] {
    const out: Primitive[] = [];
    let partialText = false;
    for (const primitive of primitives) {
      switch (primitive.kind) {
        case "stroke":
          if (primitive.segments) {
            const kept: number[] = [];
            for (let i = 0; i + 3 < primitive.points.length; i += 4) {
              for (const piece of clipPolyline(primitive.points.subarray(i, i + 4), false, box)) kept.push(...piece);
            }
            if (kept.length >= 4) out.push({ ...primitive, points: Float64Array.from(kept) });
          } else {
            for (const piece of clipPolyline(primitive.points, primitive.closed, box)) {
              out.push({ ...primitive, points: piece, closed: false });
            }
          }
          break;
        case "fill": {
          const rings = primitive.rings.map((ring) => clipPolygon(ring, box)).filter((ring) => ring.length >= 6);
          if (rings.length > 0) out.push({ ...primitive, rings });
          break;
        }
        case "clipped": {
          const rings = primitive.rings.map((ring) => clipPolygon(ring, box)).filter((ring) => ring.length >= 6);
          if (rings.length > 0) out.push({ ...primitive, rings });
          break;
        }
        case "dot":
          if (
            primitive.x >= box.minX &&
            primitive.x <= box.maxX &&
            primitive.y >= box.minY &&
            primitive.y <= box.maxY
          ) {
            out.push(primitive);
          }
          break;
        case "text": {
          const bbox = emptyBox();
          extendPrimitiveBox(bbox, primitive);
          if (!intersects(bbox, box)) break;
          if (!inside(bbox, box)) partialText = true;
          out.push(primitive);
        }
      }
    }
    if (partialText)
      this.notices.add("info", "viewport-text", "ビューポートの境界にかかる文字は切り取らずに描きました");
    return out;
  }

  // --- 図形 -----------------------------------------------------------------

  emit(entity: DxfEntity, ctx: Ctx): void {
    if (entity.invisible) return;
    if (ctx.frozen?.has(this.effectiveLayer(entity, ctx).toUpperCase())) return;
    switch (entity.type) {
      case "LINE": {
        const [x1, y1] = apply(ctx.local, entity.start[0], entity.start[1]);
        const [x2, y2] = apply(ctx.local, entity.end[0], entity.end[1]);
        this.add(
          entity,
          ctx,
          [this.stroke(entity, ctx, ctx.m, [entity.start[0], entity.start[1], entity.end[0], entity.end[1]], false)],
          [
            { label: "始点", value: point2(x1, y1) },
            { label: "終点", value: point2(x2, y2) },
            { label: "長さ", value: fmt(Math.hypot(x2 - x1, y2 - y1)) },
          ],
        );
        return;
      }
      case "CIRCLE":
      case "ARC": {
        const ocs = ocsMatrix(entity.extrusion, entity.center[2]);
        const m = multiply(ctx.m, ocs);
        const [cx, cy] = [entity.center[0], entity.center[1]];
        const full = entity.type === "CIRCLE";
        let points = arcPoints(
          cx,
          cy,
          entity.radius,
          full ? 0 : entity.start_angle,
          full ? 360 : entity.end_angle,
          this.tolerance(m, entity.radius),
        );
        if (full) points = points.slice(0, -2);
        const [wx, wy] = apply(multiply(ctx.local, ocs), cx, cy);
        const properties: Property[] = [
          { label: "中心", value: point2(wx, wy) },
          { label: "半径", value: fmt(entity.radius * meanScale(ctx.local)) },
        ];
        if (entity.type === "ARC") {
          properties.push({ label: "角度", value: `${fmt(entity.start_angle)}° → ${fmt(entity.end_angle)}°` });
        }
        this.add(entity, ctx, [this.stroke(entity, ctx, m, points, full)], properties);
        return;
      }
      case "ELLIPSE": {
        const minor = minorAxis(entity.major_axis, entity.ratio, entity.extrusion);
        const tau = Math.PI * 2;
        const full =
          Math.abs(entity.end_param - entity.start_param) < 1e-9 ||
          Math.abs(Math.abs(entity.end_param - entity.start_param) - tau) < 1e-9;
        const radius = Math.hypot(...entity.major_axis);
        let points = ellipsePoints(
          entity.center,
          entity.major_axis,
          minor,
          full ? 0 : entity.start_param,
          full ? tau : entity.end_param,
          this.tolerance(ctx.m, radius),
        );
        if (full) points = points.slice(0, -2);
        const [wx, wy] = apply(ctx.local, entity.center[0], entity.center[1]);
        this.add(
          entity,
          ctx,
          [this.stroke(entity, ctx, ctx.m, points, full)],
          [
            { label: "中心", value: point2(wx, wy) },
            { label: "長軸の半径", value: fmt(radius * meanScale(ctx.local)) },
            { label: "短軸 / 長軸", value: fmt(entity.ratio) },
          ],
          full ? "楕円" : "楕円弧",
        );
        return;
      }
      case "POINT": {
        const [x, y] = apply(ctx.m, entity.location[0], entity.location[1]);
        const [wx, wy] = apply(ctx.local, entity.location[0], entity.location[1]);
        this.add(
          entity,
          ctx,
          [{ kind: "dot", x, y, color: this.color(entity, ctx) }],
          [{ label: "位置", value: point2(wx, wy) }],
        );
        return;
      }
      case "LWPOLYLINE":
        this.polyline(entity, ctx, multiply(ctx.m, ocsMatrix(entity.extrusion, entity.elevation ?? 0)));
        return;
      case "POLYLINE":
        this.polylineEntity(entity, ctx);
        return;
      case "SPLINE":
      case "HELIX": {
        let points: number[];
        if (entity.control_points.length >= 2) {
          const steps = Math.min(4096, Math.max(32, (entity.control_points.length - entity.degree) * 16));
          points = nurbsPoints(entity.degree, entity.control_points, entity.knots, entity.weights, steps);
        } else {
          points = throughPoints(entity.fit_points, 16);
          if (entity.fit_points.length >= 3) {
            this.notices.add("info", "spline-fit", "当てはめ点だけのスプラインは、点を通る曲線で近似しました");
          }
        }
        this.add(
          entity,
          ctx,
          [this.stroke(entity, ctx, ctx.m, points, false)],
          [
            { label: "次数", value: String(entity.degree) },
            { label: "制御点", value: `${entity.control_points.length} 点` },
          ],
        );
        return;
      }
      case "TEXT":
      case "ATTRIB":
      case "ATTDEF":
        this.text(entity, ctx);
        return;
      case "MTEXT":
        this.mtext(entity, ctx);
        return;
      case "INSERT":
      case "ACAD_TABLE":
        this.insert(entity, ctx);
        return;
      case "DIMENSION":
      case "ARC_DIMENSION":
      case "LARGE_RADIAL_DIMENSION":
        this.dimension(entity, ctx);
        return;
      case "HATCH":
      case "MPOLYGON":
        this.hatch(entity, ctx);
        return;
      case "SOLID":
      case "TRACE": {
        const m = multiply(ctx.m, ocsMatrix(entity.extrusion, entity.vtx0[2]));
        const v = [entity.vtx0, entity.vtx1, entity.vtx3, entity.vtx2];
        const same = entity.vtx2[0] === entity.vtx3[0] && entity.vtx2[1] === entity.vtx3[1];
        const ring = transformPoints(
          m,
          (same ? [entity.vtx0, entity.vtx1, entity.vtx2] : v).flatMap((p) => [p[0], p[1]]),
        );
        this.add(entity, ctx, [{ kind: "fill", color: this.color(entity, ctx), rings: [ring] }], []);
        return;
      }
      case "3DFACE": {
        const v = [entity.vtx0, entity.vtx1, entity.vtx2, entity.vtx3];
        const segments: number[] = [];
        const hidden = entity.invisible_edges ?? 0;
        for (let i = 0; i < 4; i++) {
          if (hidden & (1 << i)) continue;
          const a = v[i] as Vec3;
          const b = v[(i + 1) % 4] as Vec3;
          if (a[0] === b[0] && a[1] === b[1]) continue;
          segments.push(a[0], a[1], b[0], b[1]);
        }
        if (segments.length === 0) return;
        const stroke = this.stroke(entity, ctx, ctx.m, segments, false);
        stroke.segments = true;
        this.add(entity, ctx, [stroke], []);
        return;
      }
      case "LEADER":
        this.leader(entity, ctx);
        return;
      case "XLINE":
      case "RAY":
        this.infinite.push({ entity, ctx });
        return;
      case "IMAGE":
      case "WIPEOUT":
        this.image(entity, ctx);
        return;
      case "VIEWPORT":
        return;
    }
  }

  /** LWPOLYLINE と 2D の POLYLINE。幅のある線分は塗りの帯で描く */
  private polyline(entity: DxfLwPolyline | DxfPolyline, ctx: Ctx, m: Matrix, keep?: readonly number[]): void {
    const indices = keep ?? entity.points.map((_, index) => index);
    const points = indices.map((index) => entity.points[index] as Vec3);
    const bulges = indices.map((index) => entity.bulges[index] ?? 0);
    if (points.length === 0) return;
    const closed = entity.closed;
    const tolerance = (radius: number) => this.tolerance(m, radius);
    const line = bulgePoints(points, bulges, closed, tolerance);
    const primitives: Primitive[] = [];
    const constant = entity.type === "LWPOLYLINE" ? (entity.const_width ?? 0) : 0;
    const widths = indices.map((index) => entity.widths[index] ?? ([0, 0] as Vec2));
    const wide = constant > 0 || widths.some(([start, end]) => start > 0 || end > 0);
    if (wide) {
      const color = this.color(entity, ctx);
      const rings: Float64Array[] = [];
      const count = closed ? points.length : points.length - 1;
      for (let i = 0; i < count; i++) {
        const a = points[i] as Vec3;
        const b = points[(i + 1) % points.length] as Vec3;
        const segment = bulgePoints([a, b], [bulges[i] ?? 0], false, tolerance);
        const [w0, w1] = constant > 0 ? [constant, constant] : (widths[i] as Vec2);
        const ring = this.band(segment, w0, w1);
        if (ring) rings.push(transformPoints(m, ring));
      }
      if (rings.length > 0) primitives.push({ kind: "fill", color, rings });
    }
    const stroke = this.stroke(entity, ctx, m, line.length > 2 && closed ? line.slice(0, -2) : line, closed);
    if (wide) stroke.width = 0;
    primitives.push(stroke);
    this.add(entity, ctx, primitives, [
      { label: "頂点", value: `${points.length} 点${closed ? "（閉じる）" : ""}` },
      ...(wide ? [{ label: "幅", value: constant > 0 ? fmt(constant) : "頂点ごと" }] : []),
    ]);
  }

  /** 幅が w0 から w1 に変わる帯の外形（中心線の左右に半分ずつ） */
  private band(line: number[], w0: number, w1: number): number[] | null {
    const n = Math.floor(line.length / 2);
    if (n < 2) return null;
    let total = 0;
    const lengths = [0];
    for (let i = 1; i < n; i++) {
      total += Math.hypot(
        (line[i * 2] as number) - (line[i * 2 - 2] as number),
        (line[i * 2 + 1] as number) - (line[i * 2 - 1] as number),
      );
      lengths.push(total);
    }
    if (!(total > 0)) return null;
    const left: number[] = [];
    const right: number[] = [];
    for (let i = 0; i < n; i++) {
      const j = Math.min(i + 1, n - 1);
      const k = Math.max(i - 1, 0);
      const dx = (line[j * 2] as number) - (line[k * 2] as number);
      const dy = (line[j * 2 + 1] as number) - (line[k * 2 + 1] as number);
      const length = Math.hypot(dx, dy) || 1;
      const half = (w0 + ((w1 - w0) * (lengths[i] as number)) / total) / 2;
      const nx = (-dy / length) * half;
      const ny = (dx / length) * half;
      left.push((line[i * 2] as number) + nx, (line[i * 2 + 1] as number) + ny);
      right.push((line[i * 2] as number) - nx, (line[i * 2 + 1] as number) - ny);
    }
    const ring = [...left];
    for (let i = n - 1; i >= 0; i--) ring.push(right[i * 2] as number, right[i * 2 + 1] as number);
    return ring;
  }

  private polylineEntity(entity: DxfPolyline, ctx: Ctx): void {
    const flags = entity.flags;
    if (flags & 64) {
      // ポリフェースメッシュ: 面の見える辺（番号が正）
      const segments: number[] = [];
      for (const face of entity.faces ?? []) {
        for (let i = 0; i < face.length; i++) {
          const a = face[i] as number;
          const b = face[(i + 1) % face.length] as number;
          if (a <= 0) continue;
          const p = entity.points[a - 1];
          const q = entity.points[Math.abs(b) - 1];
          if (p && q) segments.push(p[0], p[1], q[0], q[1]);
        }
      }
      if (segments.length > 0) {
        const stroke = this.stroke(entity, ctx, ctx.m, segments, false);
        stroke.segments = true;
        this.add(
          entity,
          ctx,
          [stroke],
          [{ label: "面", value: `${entity.faces?.length ?? 0} 面` }],
          "ポリフェースメッシュ",
        );
      }
      return;
    }
    if (flags & 16) {
      // ポリゴンメッシュ: M × N の頂点の格子
      const mCount = entity.m_count ?? 0;
      const nCount = entity.n_count ?? 0;
      const segments: number[] = [];
      const at = (i: number, j: number) => entity.points[i * nCount + j];
      for (let i = 0; i < mCount; i++) {
        for (let j = 0; j < nCount; j++) {
          const p = at(i, j);
          const right = j + 1 < nCount ? at(i, j + 1) : flags & 32 ? at(i, 0) : undefined;
          const down = i + 1 < mCount ? at(i + 1, j) : flags & 1 ? at(0, j) : undefined;
          if (p && right) segments.push(p[0], p[1], right[0], right[1]);
          if (p && down) segments.push(p[0], p[1], down[0], down[1]);
        }
      }
      if (segments.length > 0) {
        const stroke = this.stroke(entity, ctx, ctx.m, segments, false);
        stroke.segments = true;
        this.add(entity, ctx, [stroke], [{ label: "頂点", value: `${mCount} × ${nCount}` }], "ポリゴンメッシュ");
      }
      return;
    }
    // スプラインで当てはめたポリラインは、当てはめた頂点（8）を描き、制御点（16）を描かない
    const keep = entity.points
      .map((_, index) => index)
      .filter((index) => {
        const vertexFlags = entity.vertex_flags[index] ?? 0;
        return flags & 4 ? (vertexFlags & 16) === 0 : true;
      });
    if (flags & 8) {
      const points = keep.flatMap((index) => [(entity.points[index] as Vec3)[0], (entity.points[index] as Vec3)[1]]);
      this.add(
        entity,
        ctx,
        [this.stroke(entity, ctx, ctx.m, points, entity.closed)],
        [{ label: "頂点", value: `${keep.length} 点${entity.closed ? "（閉じる）" : ""}` }],
        "3D ポリライン",
      );
      return;
    }
    const elevation = entity.points[0]?.[2] ?? 0;
    this.polyline(entity, ctx, multiply(ctx.m, ocsMatrix(entity.extrusion, elevation)), keep);
  }

  // --- 文字 -----------------------------------------------------------------

  private style(name: string): DxfTextStyle | undefined {
    return this.styles.get(name.toUpperCase()) ?? this.styles.get("STANDARD");
  }

  /** フォントの CSS の名前。SHX は同梱しないので代わりのフォントで描き、数を知らせる */
  private fontFamily(style: DxfTextStyle | undefined, override?: TextRun["font"]): { family: string; shx: boolean } {
    const file = override?.name ?? style?.font ?? "";
    const truetype =
      override?.truetype ||
      /\.(ttf|ttc|otf)$/i.test(file) ||
      (!override && Boolean(style?.font_family)) ||
      (file !== "" && !/\.shx$/i.test(file) && /[^\x20-\x7e]/.test(file));
    const name = (truetype ? (override ? file : style?.font_family || file) : file).replace(
      /\.(ttf|ttc|otf|shx)$/i,
      "",
    );
    if (!truetype) this.shxFonts.add((file || "txt").replace(/\.shx$/i, "").toLowerCase());
    return { family: fontFamilyFor(name || undefined), shx: !truetype };
  }

  /**
   * 1 行の run を、ベースラインの左端を原点とする文字の座標系に並べて描く。
   * transform は文字の座標系から表示用の図面への変換。width は行の幅
   */
  private drawLine(
    runs: readonly TextRun[],
    transform: Matrix,
    baseColor: string,
    style: DxfTextStyle | undefined,
    out: Primitive[],
  ): { width: number; shx: boolean } {
    let x = 0;
    let shx = false;
    for (const run of runs) {
      const color =
        run.color === undefined ? baseColor : typeof run.color === "number" ? aciColor(run.color) : run.color;
      const face = this.fontFamily(style, run.font);
      if (face.shx) shx = true;
      if (run.stack) {
        x += this.drawStack(run, x, transform, color, face.family, out);
        continue;
      }
      const text = run.text.replace(/\t/g, "    ");
      if (!text) continue;
      const em = emFor(text) * run.height;
      const width = units(text) * em * run.widthFactor;
      if (text.trim() !== "") {
        const primitive = layoutText(
          {
            text,
            x: 0,
            y: 0,
            height: em,
            width,
            spacing: 0,
            angle: 0,
            slant: run.oblique,
            anchor: 1,
            vertical: false,
            color,
            fontFamily: face.family,
          },
          multiply(transform, translate(x, -DESCENT * em)),
        );
        if (primitive) out.push(primitive);
      }
      for (const [on, y] of [
        [run.underline, -0.2 * run.height],
        [run.overline, 1.2 * run.height],
        [run.strike, 0.45 * run.height],
      ] as const) {
        if (!on) continue;
        out.push({
          kind: "stroke",
          points: transformPoints(transform, [x, y, x + width, y]),
          closed: false,
          color,
          width: 0,
          dashes: [],
        });
      }
      x += width;
    }
    return { width: x, shx };
  }

  /** 積み重ね（\S）。上と下を 70% の高さで重ね、/ は横線を引く */
  private drawStack(
    run: TextRun,
    x: number,
    transform: Matrix,
    color: string,
    family: string,
    out: Primitive[],
  ): number {
    const stack = run.stack as NonNullable<TextRun["stack"]>;
    const h = run.height * 0.7;
    const parts = [stack.upper, stack.lower];
    const widths = parts.map((part) => units(part) * emFor(part) * h * run.widthFactor);
    const width = stack.kind === "#" ? (widths[0] as number) + (widths[1] as number) : Math.max(...widths);
    const place = (text: string, px: number, baseline: number, w: number) => {
      if (!text) return;
      const em = emFor(text) * h;
      const primitive = layoutText(
        {
          text,
          x: 0,
          y: 0,
          height: em,
          width: w,
          spacing: 0,
          angle: 0,
          slant: run.oblique,
          anchor: 1,
          vertical: false,
          color,
          fontFamily: family,
        },
        multiply(transform, translate(px, baseline - DESCENT * em)),
      );
      if (primitive) out.push(primitive);
    };
    if (stack.kind === "#") {
      place(stack.upper, x, run.height * 0.45, widths[0] as number);
      place(stack.lower, x + (widths[0] as number), -run.height * 0.1, widths[1] as number);
    } else {
      place(stack.upper, x + (width - (widths[0] as number)) / 2, run.height * 0.55, widths[0] as number);
      place(stack.lower, x + (width - (widths[1] as number)) / 2, -run.height * 0.35, widths[1] as number);
      if (stack.kind === "/") {
        out.push({
          kind: "stroke",
          points: transformPoints(transform, [x, run.height * 0.42, x + width, run.height * 0.42]),
          closed: false,
          color,
          width: 0,
          dashes: [],
        });
      }
    }
    this.notices.add("info", "mtext-stack", "積み重ねた文字（分数など）は小さく上下に並べました");
    return width;
  }

  private countShx(shx: boolean): void {
    if (shx) this.shxCount++;
  }

  /** TEXT・ATTRIB・ATTDEF（DXF Reference「TEXT」の 72・73 の表） */
  private text(entity: DxfText | DxfAttrib | DxfAttdef, ctx: Ctx): void {
    if (entity.type === "ATTRIB" && entity.attribute_flags & 1) return;
    if (entity.type === "ATTDEF" && entity.attribute_flags & 1 && ctx.depth === 0) return;
    const content = entity.type === "ATTDEF" && ctx.depth === 0 ? entity.tag : entity.text;
    if (!content) return;
    const style = this.style(entity.style);
    const height =
      entity.height > 0 ? entity.height : style && style.height > 0 ? style.height : (this.doc.header.textsize ?? 2.5);
    const runs = parseTextCodes(content, height, entity.width || 1, entity.oblique);
    const width = runs.reduce((sum, run) => sum + units(run.text) * emFor(run.text) * run.height * run.widthFactor, 0);
    const ocs = ocsMatrix(entity.extrusion, entity.insert[2]);
    let origin: Matrix;
    const align = entity.align_point ?? entity.insert;
    const h = entity.halign;
    const v = entity.valign;
    if ((h === 3 || h === 5) && entity.align_point && width > 0) {
      // 両端揃え（3）は高さも、フィット（5）は幅だけを、2 点の間に合わせる
      const dx = align[0] - entity.insert[0];
      const dy = align[1] - entity.insert[1];
      const length = Math.hypot(dx, dy);
      const factor = length > 0 ? length / width : 1;
      const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
      origin = multiply(
        multiply(translate(entity.insert[0], entity.insert[1]), rotate(length > 0 ? angle : entity.rotation)),
        scale(factor, h === 3 ? factor : 1),
      );
    } else {
      const point = h === 0 && v === 0 ? entity.insert : align;
      const ax = h === 1 || h === 4 ? width / 2 : h === 2 ? width : 0;
      const ay =
        h === 4 ? height / 2 : v === 1 ? -DESCENT * EM_LATIN * height : v === 2 ? height / 2 : v === 3 ? height : 0;
      const mirror = scale(entity.text_generation_flag & 2 ? -1 : 1, entity.text_generation_flag & 4 ? -1 : 1);
      origin = multiply(
        multiply(multiply(translate(point[0], point[1]), rotate(entity.rotation)), mirror),
        translate(-ax, -ay),
      );
    }
    const transform = multiply(multiply(ctx.m, ocs), origin);
    const primitives: Primitive[] = [];
    const { shx } = this.drawLine(runs, transform, this.color(entity, ctx), style, primitives);
    if (primitives.length === 0) return;
    this.countShx(shx);
    const plain = runs.map((run) => run.text).join("");
    const properties: Property[] = [{ label: "文字列", value: plain }];
    if (entity.type !== "TEXT") properties.push({ label: "タグ", value: entity.tag });
    properties.push(
      { label: "高さ", value: fmt(height * meanScale(ctx.local)) },
      {
        label: "文字スタイル",
        value: style ? `${style.name}（${style.font_family || style.font || "txt"}）` : entity.style,
      },
    );
    this.add(entity, ctx, primitives, properties);
  }

  /** MTEXT（DXF Reference「MTEXT」）。折り返しの幅があれば、語・全角の文字の境目で折り返す */
  private mtext(entity: DxfMText, ctx: Ctx): void {
    const style = this.style(entity.style);
    const base =
      entity.char_height > 0
        ? entity.char_height
        : style && style.height > 0
          ? style.height
          : (this.doc.header.textsize ?? 2.5);
    const paragraphs = parseMText(entity.raw_text, base, style?.width || 1, style?.oblique ?? 0);
    const wrap = entity.rect_width > 0 ? entity.rect_width : 0;
    interface Line {
      runs: TextRun[];
      width: number;
      height: number;
      align?: ParagraphAlign;
    }
    const runWidth = (run: TextRun) =>
      run.stack
        ? Math.max(units(run.stack.upper), units(run.stack.lower)) * EM_LATIN * run.height * 0.7 * run.widthFactor
        : units(run.text) * emFor(run.text) * run.height * run.widthFactor;
    const lines: Line[] = [];
    for (const paragraph of paragraphs) {
      let line: Line = { runs: [], width: 0, height: 0, align: paragraph.align };
      const push = (run: TextRun) => {
        line.runs.push(run);
        line.width += runWidth(run);
        line.height = Math.max(line.height, run.height);
      };
      const finish = () => {
        if (line.height === 0) line.height = paragraph.runs[0]?.height ?? base;
        lines.push(line);
        line = { runs: [], width: 0, height: 0, align: paragraph.align };
      };
      for (const run of paragraph.runs) {
        if (!wrap || run.stack) {
          if (wrap && line.runs.length > 0 && line.width + runWidth(run) > wrap) finish();
          push(run);
          continue;
        }
        // 1 文字ずつ足し、幅を超えたら直前の空白か全角の文字の境目で折り返す
        let piece = "";
        for (const char of run.text) {
          const candidate = piece + char;
          const width = runWidth({ ...run, text: candidate });
          if (line.width + width <= wrap || (line.runs.length === 0 && piece === "")) {
            piece = candidate;
            continue;
          }
          // 半角の語の途中では折り返さない。直前の空白か、全角の文字の直後で折り返す
          let head = piece;
          let tail = char;
          if (isHalfWidth(char) && char !== " ") {
            const chars = Array.from(piece);
            let at = chars.length;
            while (at > 0 && chars[at - 1] !== " " && isHalfWidth(chars[at - 1] as string)) at--;
            if (at > 0) {
              head = chars.slice(0, at).join("").replace(/ +$/, "");
              tail = chars.slice(at).join("") + char;
            }
          }
          if (head) push({ ...run, text: head });
          finish();
          piece = tail.replace(/^ +/, "");
        }
        if (piece) push({ ...run, text: piece });
      }
      finish();
    }
    while (lines.length > 1 && (lines[lines.length - 1] as Line).runs.length === 0) lines.pop();
    if (!lines.some((line) => line.runs.some((run) => run.stack || run.text.trim() !== ""))) return;
    const factor = entity.line_spacing_factor > 0 ? entity.line_spacing_factor : 1;
    const baselines: number[] = [];
    let y = -(lines[0] as Line).height;
    lines.forEach((line, index) => {
      if (index > 0) y -= line.height * LINE_SPACING * factor;
      baselines.push(y);
    });
    const top = 0;
    const bottom =
      (baselines[baselines.length - 1] as number) - DESCENT * EM_LATIN * (lines[lines.length - 1] as Line).height;
    const attachment = entity.attachment_point >= 1 && entity.attachment_point <= 9 ? entity.attachment_point : 1;
    const column = (attachment - 1) % 3;
    const row = Math.floor((attachment - 1) / 3);
    const blockWidth = wrap || Math.max(...lines.map((line) => line.width));
    const x0 = column === 0 ? 0 : column === 1 ? -blockWidth / 2 : -blockWidth;
    const dy = row === 0 ? 0 : row === 1 ? (top - bottom) / 2 : top - bottom;
    // 文字の向き: 11（WCS のベクトル）があればそれを、なければ OCS の回転角
    let frame: Matrix;
    if (entity.text_direction && Math.hypot(entity.text_direction[0], entity.text_direction[1]) > 1e-12) {
      const [dx0, dy0] = entity.text_direction;
      const length = Math.hypot(dx0, dy0);
      const ux = dx0 / length;
      const uy = dy0 / length;
      const flip = (entity.extrusion?.[2] ?? 1) < 0 ? -1 : 1;
      frame = [ux, uy, -uy * flip, ux * flip, entity.insert[0], entity.insert[1]];
    } else {
      const ocs = ocsMatrix(entity.extrusion);
      const [ix, iy] = [entity.insert[0], entity.insert[1]];
      frame = multiply(translate(ix, iy), multiply([ocs[0], ocs[1], ocs[2], ocs[3], 0, 0], rotate(entity.rotation)));
    }
    if (entity.drawing_direction === 3) {
      this.notices.add("info", "mtext-vertical", "縦書きの複数行文字は横書きで並べました");
    }
    const transform = multiply(ctx.m, frame);
    const color = this.color(entity, ctx);
    const primitives: Primitive[] = [];
    if (entity.background_flags & 3) {
      const margin = ((entity.background_scale ?? 1.5) - 1) * base;
      const fill =
        entity.background_flags & 2
          ? BACKGROUND_COLOR
          : entity.background_true_color !== undefined
            ? trueColor(entity.background_true_color)
            : entity.background_color_index !== undefined
              ? aciColor(entity.background_color_index)
              : BACKGROUND_COLOR;
      const x1 = x0 - margin;
      const x2 = x0 + blockWidth + margin;
      const y1 = top + dy + margin;
      const y2 = bottom + dy - margin;
      primitives.push({
        kind: "fill",
        color: fill,
        rings: [transformPoints(transform, [x1, y1, x2, y1, x2, y2, x1, y2])],
      });
    }
    let shx = false;
    lines.forEach((line, index) => {
      const align = line.align ?? (column === 0 ? "left" : column === 1 ? "center" : "right");
      const offset =
        align === "center" ? (blockWidth - line.width) / 2 : align === "right" ? blockWidth - line.width : 0;
      const lineTransform = multiply(transform, translate(x0 + offset, (baselines[index] as number) + dy));
      const result = this.drawLine(line.runs, lineTransform, color, style, primitives);
      if (result.shx) shx = true;
    });
    this.countShx(shx);
    this.add(entity, ctx, primitives, [
      { label: "文字列", value: entity.text },
      { label: "高さ", value: fmt(base * meanScale(ctx.local)) },
      {
        label: "文字スタイル",
        value: style ? `${style.name}（${style.font_family || style.font || "txt"}）` : entity.style,
      },
      ...(wrap ? [{ label: "折り返しの幅", value: fmt(wrap * meanScale(ctx.local)) }] : []),
    ]);
  }

  // --- ブロック・寸法 -------------------------------------------------------

  private insert(entity: DxfInsert | DxfTable, ctx: Ctx): void {
    const name = entity.name;
    const contents = this.blocks.get(name.toUpperCase());
    if (!contents) {
      this.notices.add("warning", "missing-block", "定義の見つからないブロック参照を省きました");
      return;
    }
    if (ctx.depth >= MAX_DEPTH) {
      this.notices.add("warning", "block-depth", "入れ子が深すぎるブロック参照を省きました");
      return;
    }
    const base = contents.block?.base_point ?? [0, 0, 0];
    const ocs = ocsMatrix(entity.extrusion, entity.insert[2]);
    const columns = entity.type === "INSERT" ? entity.column_count : 1;
    const rows = entity.type === "INSERT" ? entity.row_count : 1;
    const many = columns * rows > 1;
    if (many && columns * rows > 10_000) {
      this.notices.add("warning", "minsert-large", "配列が多すぎるブロック参照は、先頭の 1 つだけを描きました");
    }
    for (let r = 0; r < (columns * rows > 10_000 ? 1 : rows); r++) {
      for (let c = 0; c < (columns * rows > 10_000 ? 1 : columns); c++) {
        const offset =
          entity.type === "INSERT" ? translate(c * entity.column_spacing, r * entity.row_spacing) : translate(0, 0);
        const local = multiply(
          multiply(
            multiply(multiply(ocs, translate(entity.insert[0], entity.insert[1])), rotate(entity.rotation)),
            offset,
          ),
          multiply(scale(entity.xscale, entity.yscale), translate(-base[0], -base[1])),
        );
        const path = `${ctx.path}${entity.handle || "?"}${many ? `[${r},${c}]` : ""}>`;
        const child = this.child(entity, ctx, local, path, name);
        for (const member of contents.entities) {
          // 属性定義は、定数（2）のものだけを描く。値は ATTRIB が持つ
          if (member.type === "ATTDEF" && (member.attribute_flags & 2) === 0) continue;
          this.emit(member, child);
        }
      }
    }
    if (entity.type === "INSERT" && entity.attribs.length > 0) {
      const attribCtx = this.child(entity, ctx, [1, 0, 0, 1, 0, 0], `${ctx.path}${entity.handle || "?"}>`, name);
      for (const attrib of entity.attribs) this.emit(attrib, attribCtx);
    }
  }

  /** 寸法は表示のブロック（*D…）の図形を、寸法の 1 つの要素にまとめる（DXF Reference「Common Dimension Group Codes」の 2） */
  private dimension(entity: DxfDimension, ctx: Ctx): void {
    const contents = entity.anonymous_block_name
      ? this.blocks.get(entity.anonymous_block_name.toUpperCase())
      : undefined;
    if (!contents) {
      this.notices.add("warning", "dimension-block", "表示のブロックがない寸法を省きました");
      return;
    }
    // ブロックの中の図形は寸法の OCS にあり、12 の位置へ動かす（ezdxf の Dimension.__virtual_entities__ と同じ）
    const ocs = ocsMatrix(entity.extrusion);
    const insert = entity.insert ?? [0, 0, 0];
    const local = multiply(translate(...(apply(ocs, insert[0], insert[1]) as [number, number])), ocs);
    const child = this.child(entity, ctx, local, `${ctx.path}${entity.handle || "?"}>`, entity.anonymous_block_name);
    const start = this.entities.length;
    for (const member of contents.entities) this.emit(member, child);
    const parts = this.entities.splice(start);
    const primitives = parts.flatMap((part) => part.primitives);
    const kind =
      entity.type === "DIMENSION" ? (DIMENSION_KINDS[entity.dimtype & 7] ?? "寸法") : TYPE_NAMES[entity.type];
    const value =
      entity.text && entity.text !== "<>" && !entity.text.includes("<>")
        ? entity.text
        : entity.actual_measurement !== undefined
          ? fmt(entity.actual_measurement)
          : "";
    // まとめる前に add が数えたレイヤの使用を戻す
    for (const part of parts) {
      for (const id of part.layers) {
        const count = (this.usedLayers.get(id) ?? 0) - 1;
        if (count > 0) this.usedLayers.set(id, count);
        else this.usedLayers.delete(id);
      }
    }
    this.add(
      entity,
      ctx,
      primitives,
      [
        { label: "寸法の種類", value: kind ?? "寸法" },
        { label: "寸法値", value },
        { label: "寸法スタイル", value: entity.dimstyle },
      ],
      kind ?? "寸法",
    );
  }

  private leader(entity: DxfLeader, ctx: Ctx): void {
    if (entity.points.length < 2) return;
    const points =
      entity.path_type === 1 && entity.points.length > 2
        ? throughPoints(entity.points, 12)
        : entity.points.flatMap((p) => [p[0], p[1]]);
    const primitives: Primitive[] = [this.stroke(entity, ctx, ctx.m, points, false)];
    if (entity.has_arrowhead) {
      const style = this.dimstyles.get(entity.dimstyle.toUpperCase());
      const size = (style?.dimasz || this.doc.header.dimasz) * (style?.dimscale || this.doc.header.dimscale || 1);
      const [tx, ty] = [entity.points[0]?.[0] ?? 0, entity.points[0]?.[1] ?? 0];
      const [nx, ny] = [entity.points[1]?.[0] ?? 0, entity.points[1]?.[1] ?? 0];
      const length = Math.hypot(nx - tx, ny - ty);
      if (length > 0 && size > 0) {
        const ux = (nx - tx) / length;
        const uy = (ny - ty) / length;
        const bx = tx + ux * size;
        const by = ty + uy * size;
        const w = size / 6;
        primitives.unshift({
          kind: "fill",
          color: this.color(entity, ctx),
          rings: [transformPoints(ctx.m, [tx, ty, bx - uy * w, by + ux * w, bx + uy * w, by - ux * w])],
        });
      }
    }
    this.add(entity, ctx, primitives, [{ label: "頂点", value: `${entity.points.length} 点` }]);
  }

  // --- ハッチング -----------------------------------------------------------

  /** 境界を点列（OCS）にする。辺の境界は、つながる向きに辺を並べ直す */
  private boundary(path: DxfHatchPath, m: Matrix): number[] {
    const tolerance = (radius: number) => this.tolerance(m, radius);
    if (!path.edges) return bulgePoints(path.points, path.bulges, true, tolerance);
    const out: number[] = [];
    for (const edge of path.edges) {
      let points: number[] = [];
      switch (edge.kind) {
        case "line":
          points = [...(edge.start ?? [0, 0]), ...(edge.end ?? [0, 0])];
          break;
        case "arc": {
          const [cx, cy] = edge.center ?? [0, 0];
          const radius = edge.radius ?? 0;
          // 時計回りの弧は、360 から引いた角度を入れ替えて書く（ezdxf の ArcEdge.load_tags と同じ）
          const [start, end] = edge.ccw
            ? [edge.start_angle ?? 0, edge.end_angle ?? 360]
            : [360 - (edge.end_angle ?? 360), 360 - (edge.start_angle ?? 0)];
          points = arcPoints(cx, cy, radius, start, end, tolerance(radius));
          if (!edge.ccw) points = reversePairs(points);
          break;
        }
        case "ellipse": {
          const [cx, cy] = edge.center ?? [0, 0];
          const [mx, my] = edge.major_axis ?? [1, 0];
          const ratio = edge.ratio ?? 1;
          const [startAngle, endAngle] = edge.ccw
            ? [edge.start_angle ?? 0, edge.end_angle ?? 360]
            : [360 - (edge.end_angle ?? 360), 360 - (edge.start_angle ?? 0)];
          // 楕円の辺の角度は長軸からの実際の角度。媒介変数に直す（ezdxf の angle_to_param）
          const param = (deg: number) => {
            const a = (deg * Math.PI) / 180;
            return Math.atan2(Math.sin(a) / (ratio || 1), Math.cos(a));
          };
          const p0 = param(startAngle);
          let p1 = param(endAngle);
          if (Math.abs(endAngle - startAngle) >= 360 - 1e-9) p1 = p0 + Math.PI * 2;
          if (p1 <= p0) p1 += Math.PI * 2;
          points = ellipsePoints(
            [cx, cy, 0],
            [mx, my, 0],
            [-my * ratio, mx * ratio, 0],
            p0,
            p1,
            tolerance(Math.hypot(mx, my)),
          );
          if (!edge.ccw) points = reversePairs(points);
          break;
        }
        case "spline":
          points =
            (edge.control_points?.length ?? 0) >= 2
              ? nurbsPoints(edge.degree ?? 3, edge.control_points ?? [], edge.knots ?? [], edge.weights ?? [], 64)
              : throughPoints(edge.fit_points ?? [], 16);
          break;
      }
      if (points.length < 2) continue;
      if (out.length >= 2) {
        const lx = out[out.length - 2] as number;
        const ly = out[out.length - 1] as number;
        const dStart = Math.hypot((points[0] as number) - lx, (points[1] as number) - ly);
        const dEnd = Math.hypot((points[points.length - 2] as number) - lx, (points[points.length - 1] as number) - ly);
        if (dEnd < dStart) points = reversePairs(points);
      }
      out.push(...points);
    }
    return out;
  }

  private hatch(entity: DxfHatch, ctx: Ctx): void {
    const m = multiply(ctx.m, ocsMatrix(entity.extrusion, entity.elevation));
    let paths = entity.paths;
    if (entity.hatch_style === 2) {
      const outer = paths.filter((path) => path.flags & 17);
      if (outer.length > 0) paths = outer;
    }
    const local = paths.map((path) => this.boundary(path, m)).filter((ring) => ring.length >= 6);
    if (local.length === 0) return;
    const rings = local.map((ring) => transformPoints(m, ring));
    const color = this.color(entity, ctx);
    const properties: Property[] = [
      { label: "模様", value: entity.solid_fill ? "塗りつぶし" : entity.pattern_name || "（名前なし）" },
    ];
    if (entity.gradient !== undefined) {
      this.notices.add("info", "hatch-gradient", "グラデーションのハッチングは 1 色で塗りました");
      const first = entity.gradient_colors?.[0];
      this.add(
        entity,
        ctx,
        [{ kind: "fill", color: first !== undefined ? trueColor(first) : color, rings }],
        properties,
      );
      return;
    }
    if (entity.solid_fill) {
      this.add(entity, ctx, [{ kind: "fill", color, rings }], properties);
      return;
    }
    properties.push(
      { label: "模様の尺度", value: fmt(entity.pattern_scale) },
      { label: "模様の角度", value: `${fmt(entity.pattern_angle)}°` },
    );
    const box = emptyBox();
    for (const ring of local) extendBoxWithPoints(box, ring);
    const strokes = this.pattern(entity, box, m, color);
    if (!strokes) {
      this.notices.add("info", "hatch-dense", "線の多すぎるハッチングの模様は、外形だけを描きました");
      this.add(
        entity,
        ctx,
        rings.map((ring) => ({ kind: "stroke", points: ring, closed: true, color, width: 0, dashes: [] })),
        properties,
      );
      return;
    }
    if (strokes.length === 0) {
      this.notices.add("info", "hatch-pattern", "模様の定義がないハッチングは、外形だけを描きました");
      this.add(
        entity,
        ctx,
        rings.map((ring) => ({ kind: "stroke", points: ring, closed: true, color, width: 0, dashes: [] })),
        properties,
      );
      return;
    }
    this.add(entity, ctx, [{ kind: "clipped", rings, strokes }], properties);
  }

  /** 模様の線（OCS）を境界の外接矩形いっぱいに引く。境界での切り抜きは描画が行う。多すぎれば null */
  private pattern(entity: DxfHatch, box: Box, m: Matrix, color: string): StrokePrimitive[] | null {
    const strokes: StrokePrimitive[] = [];
    let count = 0;
    const corners: Vec2[] = [
      [box.minX, box.minY],
      [box.maxX, box.minY],
      [box.maxX, box.maxY],
      [box.minX, box.maxY],
    ];
    for (const line of entity.pattern_lines) {
      const a = (line.angle * Math.PI) / 180;
      const d: Vec2 = [Math.cos(a), Math.sin(a)];
      const n: Vec2 = [-d[1], d[0]];
      const spacing = line.offset[0] * n[0] + line.offset[1] * n[1];
      if (!(Math.abs(spacing) > 1e-9)) continue;
      const ts = corners.map((c) => ((c[0] - line.base[0]) * n[0] + (c[1] - line.base[1]) * n[1]) / spacing);
      const k0 = Math.floor(Math.min(...ts));
      const k1 = Math.ceil(Math.max(...ts));
      count += k1 - k0 + 1;
      if (count > MAX_PATTERN_LINES) return null;
      const period = line.dashes.reduce((sum, value) => sum + Math.abs(value), 0);
      const segments: number[] = [];
      for (let k = k0; k <= k1; k++) {
        const px = line.base[0] + k * line.offset[0];
        const py = line.base[1] + k * line.offset[1];
        const us = corners.map((c) => (c[0] - px) * d[0] + (c[1] - py) * d[1]);
        let u0 = Math.min(...us);
        const u1 = Math.max(...us);
        // 破線の位相は各線の原点（基点 + k × 間隔）から数える
        if (period > 0) u0 = Math.floor(u0 / period) * period;
        segments.push(px + u0 * d[0], py + u0 * d[1], px + u1 * d[0], py + u1 * d[1]);
      }
      if (segments.length === 0) continue;
      strokes.push({
        kind: "stroke",
        points: transformPoints(m, segments),
        closed: false,
        segments: true,
        color,
        width: 0,
        dashes: dashArray(line.dashes, meanScale(m)),
      });
    }
    return strokes;
  }

  private image(entity: DxfImage, ctx: Ctx): void {
    const [w, h] = entity.image_size;
    let pixels = entity.boundary;
    if (pixels.length === 0) {
      pixels = [
        [-0.5, -0.5],
        [w - 0.5, h - 0.5],
      ];
    }
    if (pixels.length === 2) {
      const [a, b] = pixels as [Vec2, Vec2];
      pixels = [a, [b[0], a[1]], b, [a[0], b[1]]];
    }
    // 画素の座標は左上が原点で、Y は下向き（ezdxf の Image.boundary_path_wcs と同じ）
    const ring: number[] = [];
    for (const [px, py] of pixels) {
      const u = px + 0.5;
      const v = h - py - 0.5;
      ring.push(
        entity.insert[0] + entity.u_pixel[0] * u + entity.v_pixel[0] * v,
        entity.insert[1] + entity.u_pixel[1] * u + entity.v_pixel[1] * v,
      );
    }
    const points = transformPoints(ctx.m, ring);
    if (entity.type === "WIPEOUT") {
      this.add(entity, ctx, [{ kind: "fill", color: BACKGROUND_COLOR, rings: [points] }], []);
      return;
    }
    this.notices.add("info", "image", "画像（IMAGE）は読み込まず、外形だけを描きました");
    this.add(
      entity,
      ctx,
      [{ kind: "stroke", points, closed: true, color: this.color(entity, ctx), width: 0, dashes: [] }],
      [{ label: "画素", value: `${fmt(w)} × ${fmt(h)}` }],
    );
  }

  /** 構築線・放射線を、ほかの図形の範囲（limit）で切って描く */
  finishInfinite(limit: Box): void {
    if (this.infinite.length === 0) return;
    const box = isEmptyBox(limit) ? null : limit;
    for (const { entity, ctx } of this.infinite) {
      const [px, py] = apply(ctx.m, entity.start[0], entity.start[1]);
      const [dx, dy] = applyVector(ctx.m, entity.unit_vector[0], entity.unit_vector[1]);
      const length = Math.hypot(dx, dy);
      if (!(length > 0)) continue;
      const ux = dx / length;
      const uy = dy / length;
      const reach = box
        ? Math.hypot(box.maxX - box.minX, box.maxY - box.minY) * 2 + Math.hypot(px - box.minX, py - box.minY)
        : 1000;
      const start = entity.type === "XLINE" ? -reach : 0;
      let points: Float64Array = Float64Array.of(px + ux * start, py + uy * start, px + ux * reach, py + uy * reach);
      if (box) {
        const clipped = clipPolyline(points, false, box)[0];
        if (!clipped) continue;
        points = clipped;
      }
      const stroke: StrokePrimitive = {
        kind: "stroke",
        points,
        closed: false,
        color: this.color(entity, ctx),
        width: this.lineweight(entity, ctx),
        dashes: this.dashes(entity, ctx, ctx.m),
      };
      this.notices.add("info", "xline", "構築線・放射線は、ほかの図形の範囲で切って描きました");
      this.add(entity, { ...ctx, m: [1, 0, 0, 1, 0, 0] }, [stroke], []);
    }
  }

  finishNotices(): void {
    if (this.shxCount > 0) {
      this.notices.add(
        "info",
        "shx-font",
        `SHX フォント（${[...this.shxFonts].sort().join("・")}）は同梱していないため、代わりのフォントで描きました`,
        this.shxCount,
      );
    }
  }
}

function reversePairs(points: number[]): number[] {
  const out: number[] = [];
  for (let i = points.length - 2; i >= 0; i -= 2) out.push(points[i] as number, points[i + 1] as number);
  return out;
}

function unitInfo(doc: DxfDocument, notices: NoticeList): { unit: number; name: string } {
  const code = doc.header.insunits;
  const unit = UNIT_TO_MM[code];
  if (unit !== undefined) return { unit, name: UNIT_NAMES[code] ?? `$INSUNITS ${code}` };
  if (code === 0 && doc.header.measurement === 0) {
    notices.add("info", "unit-assumed", "長さの単位の指定がなく、ヤード・ポンド法の図面なのでインチとみなしました");
    return { unit: 25.4, name: "インチ（指定なし）" };
  }
  notices.add("info", "unit-assumed", "長さの単位の指定がないため、mm とみなしました");
  return { unit: 1, name: "mm（指定なし）" };
}

/** 図形の代表の点（原点を寄せる位置を決める） */
function anchorOf(entity: DxfEntity): Vec2 | null {
  switch (entity.type) {
    case "LINE":
      return [entity.start[0], entity.start[1]];
    case "CIRCLE":
    case "ARC":
    case "ELLIPSE":
      return [entity.center[0], entity.center[1]];
    case "POINT":
      return [entity.location[0], entity.location[1]];
    case "LWPOLYLINE":
    case "POLYLINE":
      return entity.points[0] ? [entity.points[0][0], entity.points[0][1]] : null;
    case "SPLINE":
    case "HELIX": {
      const p = entity.control_points[0] ?? entity.fit_points[0];
      return p ? [p[0], p[1]] : null;
    }
    case "TEXT":
    case "ATTRIB":
    case "ATTDEF":
    case "MTEXT":
    case "INSERT":
    case "ACAD_TABLE":
    case "IMAGE":
    case "WIPEOUT":
      return [entity.insert[0], entity.insert[1]];
    case "DIMENSION":
    case "ARC_DIMENSION":
    case "LARGE_RADIAL_DIMENSION":
      return [entity.defpoint[0], entity.defpoint[1]];
    case "SOLID":
    case "TRACE":
    case "3DFACE":
      return [entity.vtx0[0], entity.vtx0[1]];
    case "LEADER":
      return entity.points[0] ? [entity.points[0][0], entity.points[0][1]] : null;
    case "HATCH":
    case "MPOLYGON": {
      const path = entity.paths[0];
      const p = path?.points[0] ?? path?.edges?.[0]?.start ?? path?.edges?.[0]?.center;
      return p ? [p[0] as number, p[1] as number] : null;
    }
  }
  return null;
}

/**
 * 座標の大きい図面（公共座標の測量図など）は、Canvas の単精度の計算で線が揺れないよう、
 * 図形の中央の近くを原点に寄せる。寄せた分は Placement の inverse で実寸の座標に戻す
 */
function originShift(entities: readonly DxfEntity[], unit: number): Vec2 {
  const xs: number[] = [];
  const ys: number[] = [];
  for (const entity of entities) {
    const p = anchorOf(entity);
    if (p && Number.isFinite(p[0]) && Number.isFinite(p[1])) {
      xs.push(p[0]);
      ys.push(p[1]);
    }
  }
  if (xs.length === 0) return [0, 0];
  const median = (values: number[]) => values.sort((a, b) => a - b)[Math.floor(values.length / 2)] as number;
  const cx = median(xs);
  const cy = median(ys);
  if (Math.hypot(cx, cy) * unit < 100_000) return [0, 0];
  const step = 1000 / unit;
  return [Math.round(cx / step) * step, Math.round(cy / step) * step];
}

function scaleLabel(paperPerReal: number): string {
  if (!(paperPerReal > 0)) return "-";
  const denominator = 1 / paperPerReal;
  if (denominator >= 1) return `1:${fmt(Number(denominator.toPrecision(6)))}`;
  return `${fmt(Number(paperPerReal.toPrecision(6)))}:1`;
}

function defaultView(doc: DxfDocument, builder: Builder): string {
  const paperLayouts = doc.layouts.filter((layout) => !layout.model);
  const active = paperLayouts.find((layout) => layout.active);
  const hasContent = (layout: DxfLayout) => builder.blockEntities(layout.block).length > 0;
  if (doc.header.tilemode === 0 && active && hasContent(active)) return active.name;
  if (builder.model.length > 0) return "Model";
  return paperLayouts.find(hasContent)?.name ?? "Model";
}

function metadataBase(doc: DxfDocument, unitName: string): Property[] {
  const release = RELEASES[doc.version];
  const encodingLabels: Record<string, string> = {
    "utf-8": "UTF-8",
    shift_jis: "Shift_JIS（CP932）",
    "windows-1252": "西欧（Windows-1252）",
  };
  const encoding = encodingLabels[doc.encoding] ?? doc.encoding;
  const source =
    doc.encoding_source === "version"
      ? "AutoCAD 2007 以降の DXF"
      : doc.encoding_source === "codepage"
        ? `$DWGCODEPAGE ${doc.header.codepage ?? ""}`.trim()
        : doc.encoding_source === "detected"
          ? "中身から判断"
          : "指定なし";
  const blocks = doc.blocks.filter((block) => !block.layout && !block.anonymous).length;
  const metadata: Property[] = [
    {
      label: "形式",
      value: `DXF ${release ? `AutoCAD ${release}` : doc.version}（${doc.version}、${doc.binary ? "バイナリ" : "テキスト"}）`,
    },
    { label: "文字コード", value: `${encoding}（${source}）` },
    { label: "単位", value: unitName },
  ];
  if (blocks > 0) metadata.push({ label: "ブロック", value: `${blocks} 件` });
  const layouts = doc.layouts.filter((layout) => !layout.model);
  if (layouts.length > 0)
    metadata.push({ label: "レイアウト", value: layouts.map((layout) => layout.name).join("・") });
  return metadata;
}

function rootCtx(m: Matrix, local: Matrix, placement: Placement | undefined): Ctx {
  return {
    m,
    local,
    layers: [],
    parentLayer: null,
    color: aciColor(7),
    lineweight: DEFAULT_LINEWEIGHT,
    linetype: "CONTINUOUS",
    path: "",
    blocks: [],
    depth: 0,
    placement,
    dashFactor: 1,
    clip: null,
    frozen: null,
  };
}

function views(doc: DxfDocument): SceneView[] {
  return [
    { id: "Model", name: "モデル" },
    ...doc.layouts.filter((layout) => !layout.model).map((layout) => ({ id: layout.name, name: layout.name })),
  ];
}

/** AutoCAD の図面（DXF・DWG から読んだモデル）を、モデル空間か 1 つのレイアウトの表示用の図面にする */
export function dxfToScene(doc: DxfDocument, options: DxfSceneOptions = {}): Scene {
  const builder = new Builder(doc);
  const notices = builder.notices;
  const { unit, name: unitName } = unitInfo(doc, notices);
  let view = options.layout ?? defaultView(doc, builder);
  const layout =
    view === "Model" ? undefined : doc.layouts.find((candidate) => !candidate.model && candidate.name === view);
  if (view !== "Model" && !layout) {
    notices.add("warning", "unknown-layout", `レイアウト ${view} が見つからないため、モデル空間を表示しました`);
    view = "Model";
  }
  const metadata = metadataBase(doc, unitName);
  let sheet: Sheet | null = null;
  let background: Scene["background"] = "dark";
  if (!layout) {
    const [ox, oy] = originShift(builder.model, unit);
    const placement: Placement = {
      name: `モデル空間（${unitName}）`,
      scaleX: 1,
      scaleY: 1,
      inverse: [1, 0, 0, 1, ox * unit, oy * unit],
    };
    const root = rootCtx([unit, 0, 0, unit, -ox * unit, -oy * unit], [1, 0, 0, 1, 0, 0], placement);
    for (const entity of builder.model) builder.emit(entity, root);
    metadata.push({ label: "表示", value: "モデル空間" });
    if (ox !== 0 || oy !== 0) {
      metadata.push({ label: "表示の原点", value: `${point2(ox, oy)}（座標の大きい図面のため寄せた）` });
    }
    const paper = doc.entities.filter((entity) => entity.space === "paper" && entity.type !== "VIEWPORT").length;
    if (paper > 0) {
      notices.add(
        "info",
        "paper-space",
        "レイアウト（紙空間）の図形はモデル空間の表示に含めていません。表示を切り替えると見られます",
        paper,
      );
    }
  } else {
    background = "light";
    const paperUnit =
      (layout.paper_units === 0 ? 25.4 : 1) *
      (layout.plot_scale[1] > 0 && layout.plot_scale[0] > 0 ? layout.plot_scale[0] / layout.plot_scale[1] : 1);
    const rotated = layout.plot_rotation === 1 || layout.plot_rotation === 3;
    let [left, bottom] = [layout.margins[0], layout.margins[1]];
    if (rotated) [left, bottom] = [bottom, left];
    let width = rotated ? layout.paper_height : layout.paper_width;
    let height = rotated ? layout.paper_width : layout.paper_height;
    let ox = left + layout.plot_origin[0];
    let oy = bottom + layout.plot_origin[1];
    if (!(width > 0 && height > 0)) {
      // 用紙の大きさがない（R12 など）ときは、紙空間の範囲を用紙とする
      const [x0, y0] = layout.limmin;
      const [x1, y1] = layout.limmax;
      width = (x1 - x0) * paperUnit;
      height = (y1 - y0) * paperUnit;
      ox = -x0 * paperUnit;
      oy = -y0 * paperUnit;
    }
    const paperMatrix: Matrix = [paperUnit, 0, 0, paperUnit, ox, oy];
    if (width > 0 && height > 0) {
      sheet = { name: layout.name, size: layout.paper_size.replace(/_/g, " ") || "レイアウト", width, height };
    }
    const root = rootCtx(paperMatrix, [1, 0, 0, 1, 0, 0], undefined);
    const content = builder.blockEntities(layout.block);
    const viewports = content.filter((entity): entity is DxfViewport => entity.type === "VIEWPORT");
    // レイアウトの全体を表すビューポート（ID 1、なければ最初のもの）は、モデルの表示ではない
    const overall = viewports.find((viewport) => viewport.id === 1) ?? viewports[0];
    const scales: string[] = [];
    for (const entity of content) {
      if (entity.type !== "VIEWPORT") {
        builder.emit(entity, root);
        continue;
      }
      if (entity === overall || entity.status <= 0 || entity.status_flags & 0x20000) continue;
      const label = drawViewport(builder, doc, entity, root, paperMatrix, unit, paperUnit);
      if (label) scales.push(label);
    }
    metadata.push({ label: "表示", value: `レイアウト ${layout.name}` });
    if (sheet) metadata.push({ label: "用紙", value: `${sheet.size}（${fmt(sheet.width)}×${fmt(sheet.height)} mm）` });
    if (scales.length > 0) metadata.push({ label: "ビューポートの縮尺", value: [...new Set(scales)].join("・") });
  }
  const bounds = emptyBox();
  for (const entity of builder.entities) unionBox(bounds, entity.bbox);
  if (sheet) {
    extendBox(bounds, 0, 0);
    extendBox(bounds, sheet.width, sheet.height);
  }
  builder.finishInfinite(sheet ? { minX: 0, minY: 0, maxX: sheet.width, maxY: sheet.height } : { ...bounds });
  for (const entity of builder.entities) unionBox(bounds, entity.bbox);
  builder.finishNotices();
  return {
    format: "dxf",
    version: `DXF ${doc.version}`,
    views: views(doc),
    view,
    sheet,
    bounds,
    layers: builder.layers(),
    entities: builder.entities,
    notices: notices.toArray(),
    metadata,
    background,
  };
}

/**
 * ビューポートの中にモデル空間を描く。モデルから紙空間への変換は ezdxf の Viewport.get_transformation_matrix と同じ
 * （表示の目標点を原点にし、縮尺 = 高さ / 表示の高さ、ねじれの角度で回し、中心をビューポートの中心へ）。
 */
function drawViewport(
  builder: Builder,
  doc: DxfDocument,
  viewport: DxfViewport,
  root: Ctx,
  paperMatrix: Matrix,
  unit: number,
  paperUnit: number,
): string | null {
  const notices = builder.notices;
  const [vx, vy, vz] = viewport.view_direction;
  const length = Math.hypot(vx, vy, vz);
  if (length > 0 && (Math.abs(vx) > 1e-9 * length || Math.abs(vy) > 1e-9 * length || vz < 0)) {
    notices.add("info", "viewport-view", "上から見た向きでないビューポート（3D の表示）は描いていません");
    return null;
  }
  if (!(viewport.view_height > 0) || !(viewport.height > 0)) return null;
  const s = viewport.height / viewport.view_height;
  const [cx, cy] = [viewport.center[0], viewport.center[1]];
  const model: Matrix = multiply(
    multiply(
      translate(cx - viewport.view_center[0] * s, cy - viewport.view_center[1] * s),
      rotate(viewport.view_twist_angle),
    ),
    multiply(scale(s, s), translate(-viewport.view_target[0], -viewport.view_target[1])),
  );
  const m = multiply(paperMatrix, model);
  // 紙空間の mm → モデルの実寸の mm
  const inverse = multiply(scale(unit, unit), invert(m));
  const paperPerReal = (s * paperUnit) / unit;
  const label = scaleLabel(paperPerReal);
  const placement: Placement = {
    name: `ビューポート（${label}）`,
    scaleX: paperPerReal,
    scaleY: paperPerReal,
    inverse,
  };
  let clip: Box = {
    minX: cx - viewport.width / 2,
    minY: cy - viewport.height / 2,
    maxX: cx + viewport.width / 2,
    maxY: cy + viewport.height / 2,
  };
  if (viewport.clipping_boundary_handle) {
    notices.add("info", "viewport-clip", "矩形でないビューポートは、外接矩形で切り取りました");
  }
  const [x0, y0] = apply(paperMatrix, clip.minX, clip.minY);
  const [x1, y1] = apply(paperMatrix, clip.maxX, clip.maxY);
  clip = { minX: Math.min(x0, x1), minY: Math.min(y0, y1), maxX: Math.max(x0, x1), maxY: Math.max(y0, y1) };
  const frozen = new Set<string>();
  for (const handle of viewport.frozen_layer_handles) {
    const layer = doc.layers.find((candidate) => candidate.handle === handle);
    if (layer) frozen.add(layer.name.toUpperCase());
  }
  const ctx: Ctx = {
    ...root,
    m,
    local: [1, 0, 0, 1, 0, 0],
    placement,
    // $PSLTSCALE が 1 なら、線種の長さは紙空間の単位（ビューポートの縮尺で割り戻す）
    dashFactor: doc.header.psltscale === 1 ? 1 / s : 1,
    clip,
    frozen,
    path: `${viewport.handle || "VP"}:`,
  };
  for (const entity of builder.model) builder.emit(entity, ctx);
  // ビューポートの枠
  builder.add(
    viewport,
    root,
    [
      builder.stroke(
        viewport,
        root,
        root.m,
        [
          cx - viewport.width / 2,
          cy - viewport.height / 2,
          cx + viewport.width / 2,
          cy - viewport.height / 2,
          cx + viewport.width / 2,
          cy + viewport.height / 2,
          cx - viewport.width / 2,
          cy + viewport.height / 2,
        ],
        true,
      ),
    ],
    [
      { label: "縮尺", value: label },
      {
        label: "表示の中心（モデル）",
        value: point2(
          viewport.view_target[0] + viewport.view_center[0],
          viewport.view_target[1] + viewport.view_center[1],
        ),
      },
      { label: "表示の高さ（モデル）", value: fmt(viewport.view_height) },
      ...(viewport.view_twist_angle ? [{ label: "ねじれの角度", value: `${fmt(viewport.view_twist_angle)}°` }] : []),
    ],
  );
  return label;
}
