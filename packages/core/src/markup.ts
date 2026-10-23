import { emptyBox, extendBox, extendBoxWithPoints, IDENTITY, transformPoints } from "./geometry.js";
import { fontFamilyFor, isHalfWidth, layoutText } from "./text.js";
import {
  BACKGROUND_COLOR,
  type Box,
  type DrawingFormat,
  type FillPrimitive,
  type Primitive,
  type StrokePrimitive,
} from "./types.js";

/**
 * 図面のマークアップ（赤入れ・雲マーク・メモ・指摘・写真の位置）。図面のファイルには書かず、図面の横に置く JSON の
 * 文書にする。点はビューの用紙の mm で、Y 軸は上向き（docs/decisions/0021、docs/markup.md）。
 * Rust の zumen_core::markup が同じ JSON を読み書きし、同じ図形を返す
 */

/** 文書の format の値 */
export const MARKUP_FORMAT = "zumen-markup";
/** 読み書きできる最も新しい版 */
export const MARKUP_VERSION = 1;

export const MARKUP_KINDS = ["redline", "cloud", "text", "finding", "photo"] as const;
export type MarkupKind = (typeof MARKUP_KINDS)[number];

/** 指摘の状態。open は未対応、resolved は設計者が対応して照査者が確かめていないもの、closed は確かめたか取り下げたもの */
export const MARKUP_STATUSES = ["open", "resolved", "closed"] as const;
export type MarkupStatus = (typeof MARKUP_STATUSES)[number];

/** 赤入れの線の矢印 */
export const MARKUP_ARROWS = ["none", "start", "end", "both"] as const;
export type MarkupArrows = (typeof MARKUP_ARROWS)[number];

/** 新しい版へ移したときの印。removed は指していた図形がすべて消えた、changed は変わったか一部が消えた */
export const CARRIED_STATES = ["unchanged", "changed", "removed"] as const;
export type CarriedState = (typeof CARRIED_STATES)[number];

/** 用紙の mm の点 [x, y] */
export type MarkupPoint = readonly [number, number];

/** マークアップを付けた図面。sha256 はファイルのバイトの SHA-256（小文字の 16 進）、format は Scene.format */
export interface MarkupDrawing {
  readonly sha256: string;
  readonly format: DrawingFormat;
}

/** 新しい版へ移した記録。from は移す前の図面の sha256 */
export interface MarkupCarried {
  readonly from: string;
  readonly state: CarriedState;
}

/**
 * どの種類の項目にもあるキー。知らないキーは、読んで書き戻すときにそのまま残す（型には現れないが、オブジェクトは持つ）
 */
export interface MarkupItemBase {
  /** 文書の中で一意（既定では UUID） */
  readonly id: string;
  readonly kind: MarkupKind;
  /** 点の座標系であるビュー（Scene.view）。ビューが 1 つしかない形式では省く */
  readonly view?: string;
  /** 利用側の分類（小文字・数字・ハイフン）。色を選ぶ */
  readonly category?: string;
  /** #rrggbb か #rrggbbaa。分類の色より優先する */
  readonly color?: string;
  /** 線の太さ（用紙の mm）。省くと、画面（サーバーでは用紙）の上で決まった太さで描く */
  readonly lineWidth?: number;
  readonly text?: string;
  /** 項目が指す図形の id */
  readonly entities?: readonly string[];
  /** 利用側の、人の名前か id */
  readonly author?: string;
  /** RFC 3339 の日時 */
  readonly createdAt?: string;
  readonly updatedAt?: string;
  readonly carried?: MarkupCarried;
  /** 利用側のデータ。そのまま残す */
  readonly data?: Readonly<Record<string, unknown>>;
}

/** 線・折線・手書きの線・矩形。幅の広い半透明の色で蛍光ペンになる */
export interface RedlineItem extends MarkupItemBase {
  readonly kind: "redline";
  /** 2 点以上 */
  readonly points: readonly MarkupPoint[];
  /** 省くと false */
  readonly closed?: boolean;
  /** 省くと none。閉じた線には描かない */
  readonly arrows?: MarkupArrows;
}

/** 雲マーク。外形に沿って、外側へふくらむ弧を並べる */
export interface CloudItem extends MarkupItemBase {
  readonly kind: "cloud";
  /** 3 点以上の閉じた外形 */
  readonly points: readonly MarkupPoint[];
  /** 弧の弦の長さ（用紙の mm）。省くと 10 */
  readonly arcLength?: number;
}

/** メモ */
export interface TextItem extends MarkupItemBase {
  readonly kind: "text";
  /** 1 行目の左下 */
  readonly position: MarkupPoint;
  readonly text: string;
  /** 文字の高さ（用紙の mm）。省くと 3.5 */
  readonly height?: number;
  /** 角度（度、反時計回り）。省くと 0 */
  readonly angle?: number;
}

/** 照査の指摘。ピンと、任意で範囲。text が指摘の本文 */
export interface FindingItem extends MarkupItemBase {
  readonly kind: "finding";
  /** ピンの位置 */
  readonly position: MarkupPoint;
  readonly status: MarkupStatus;
  /** 範囲（3 点以上の閉じた外形） */
  readonly points?: readonly MarkupPoint[];
  /** ピンの上の短い文字（番号など） */
  readonly label?: string;
}

/** 写真を撮った位置 */
export interface PhotoItem extends MarkupItemBase {
  readonly kind: "photo";
  readonly position: MarkupPoint;
  /** 利用側が解決する写真の参照（id・URL など）。文書は写真のバイトを持たない */
  readonly photo: string;
  /** カメラの向き（度、+x から反時計回り） */
  readonly direction?: number;
  /** RFC 3339 の日時 */
  readonly takenAt?: string;
}

export type MarkupItem = RedlineItem | CloudItem | TextItem | FindingItem | PhotoItem;

export interface MarkupDocument {
  readonly format: typeof MARKUP_FORMAT;
  readonly version: number;
  readonly drawing: MarkupDrawing;
  readonly items: readonly MarkupItem[];
  /** 利用側のデータ。そのまま残す */
  readonly data?: Readonly<Record<string, unknown>>;
}

/** 図面に付ける、項目のない文書 */
export function createMarkup(drawing: MarkupDrawing): MarkupDocument {
  return { format: MARKUP_FORMAT, version: MARKUP_VERSION, drawing: { ...drawing }, items: [] };
}

// ---- JSON と検査

/**
 * 文書の検査で見つけた誤り。code は種類（json・format・version・missing・type・value・duplicate）、
 * item は items の何番目か（文書のキーなら null）、id はその項目の id（読めなければ null）、key はキー（drawing.sha256 など）
 */
export class MarkupError extends Error {
  override readonly name = "MarkupError";

  constructor(
    readonly code: MarkupErrorCode,
    readonly item: number | null,
    readonly id: string | null,
    readonly key: string | null,
    message: string,
  ) {
    super(message);
  }
}

export type MarkupErrorCode = "json" | "format" | "version" | "missing" | "type" | "value" | "duplicate";

type JsonObject = Record<string, unknown>;

const SHA256 = /^[0-9a-f]{64}$/;
const CATEGORY = /^[a-z0-9-]+$/;
const COLOR = /^#[0-9a-fA-F]{6}(?:[0-9a-fA-F]{2})?$/;
/** RFC 3339 の date-time（5.6 節）。T と Z は小文字も認める */
const DATE_TIME =
  /^\d{4}-(?:0[1-9]|1[0-2])-(?:0[1-9]|[12]\d|3[01])[Tt](?:[01]\d|2[0-3]):[0-5]\d:(?:[0-5]\d|60)(?:\.\d+)?(?:[Zz]|[+-](?:[01]\d|2[0-3]):[0-5]\d)$/;

function isObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** 検査の途中の場所。誤りに項目と キーを付ける */
class Checker {
  item: number | null = null;
  id: string | null = null;

  fail(code: MarkupErrorCode, key: string | null, message: string): never {
    const where = this.item === null ? "" : `items[${this.item}]${this.id === null ? "" : `（${this.id}）`} の `;
    throw new MarkupError(code, this.item, this.id, key, `${where}${key ?? "文書"}: ${message}`);
  }

  required(object: JsonObject, key: string, name = key): unknown {
    if (!(key in object)) this.fail("missing", name, "ありません");
    return object[key];
  }

  string(value: unknown, key: string, nonEmpty = false): string {
    if (typeof value !== "string") this.fail("type", key, "文字列ではありません");
    if (nonEmpty && value === "") this.fail("value", key, "空です");
    return value;
  }

  pattern(value: unknown, key: string, pattern: RegExp): string {
    const text = this.string(value, key);
    if (!pattern.test(text)) this.fail("value", key, `形が違います（${JSON.stringify(text)}）`);
    return text;
  }

  number(value: unknown, key: string, positive = false): number {
    if (typeof value !== "number") this.fail("type", key, "数ではありません");
    if (!Number.isFinite(value) || (positive && !(value > 0))) this.fail("value", key, `使えない値です（${value}）`);
    return value;
  }

  oneOf<T extends string>(value: unknown, key: string, values: readonly T[]): T {
    const text = this.string(value, key);
    if (!(values as readonly string[]).includes(text)) this.fail("value", key, `知らない値です（${text}）`);
    return text as T;
  }

  object(value: unknown, key: string): JsonObject {
    if (!isObject(value)) this.fail("type", key, "オブジェクトではありません");
    return value;
  }

  point(value: unknown, key: string): void {
    if (!Array.isArray(value) || !value.every((coordinate) => typeof coordinate === "number")) {
      this.fail("type", key, "点は数の配列 [x, y] です");
    }
    if (value.length !== 2 || !value.every((coordinate) => Number.isFinite(coordinate))) {
      this.fail("value", key, "点は有限の 2 つの数 [x, y] です");
    }
  }

  points(value: unknown, key: string, minimum: number): void {
    if (!Array.isArray(value)) this.fail("type", key, "点の配列ではありません");
    for (const point of value) this.point(point, key);
    if (value.length < minimum) this.fail("value", key, `${minimum} 点以上が要ります（${value.length} 点）`);
  }
}

/** 項目の、種類によらないキー。この順に調べる */
function checkCommon(check: Checker, item: JsonObject): void {
  if ("view" in item) check.string(item.view, "view", true);
  if ("category" in item) check.pattern(item.category, "category", CATEGORY);
  if ("color" in item) check.pattern(item.color, "color", COLOR);
  if ("lineWidth" in item) check.number(item.lineWidth, "lineWidth", true);
  if ("text" in item) check.string(item.text, "text");
  if ("entities" in item) {
    if (!Array.isArray(item.entities)) check.fail("type", "entities", "配列ではありません");
    for (const entity of item.entities) check.string(entity, "entities");
  }
  if ("author" in item) check.string(item.author, "author");
  if ("createdAt" in item) check.pattern(item.createdAt, "createdAt", DATE_TIME);
  if ("updatedAt" in item) check.pattern(item.updatedAt, "updatedAt", DATE_TIME);
  if ("carried" in item) {
    const carried = check.object(item.carried, "carried");
    check.pattern(check.required(carried, "from", "carried.from"), "carried.from", SHA256);
    check.oneOf(check.required(carried, "state", "carried.state"), "carried.state", CARRIED_STATES);
  }
  if ("data" in item) check.object(item.data, "data");
}

/** 種類ごとのキー。ほかの種類のキーは知らないキーとしてそのまま残す */
function checkKind(check: Checker, item: JsonObject, kind: MarkupKind): void {
  switch (kind) {
    case "redline":
      check.points(check.required(item, "points"), "points", 2);
      if ("closed" in item && typeof item.closed !== "boolean") check.fail("type", "closed", "真偽値ではありません");
      if ("arrows" in item) check.oneOf(item.arrows, "arrows", MARKUP_ARROWS);
      return;
    case "cloud":
      check.points(check.required(item, "points"), "points", 3);
      if ("arcLength" in item) check.number(item.arcLength, "arcLength", true);
      return;
    case "text":
      check.point(check.required(item, "position"), "position");
      check.required(item, "text");
      if ("height" in item) check.number(item.height, "height", true);
      if ("angle" in item) check.number(item.angle, "angle");
      return;
    case "finding":
      check.point(check.required(item, "position"), "position");
      check.oneOf(check.required(item, "status"), "status", MARKUP_STATUSES);
      if ("points" in item) check.points(item.points, "points", 3);
      if ("label" in item) check.string(item.label, "label");
      return;
    case "photo":
      check.point(check.required(item, "position"), "position");
      check.string(check.required(item, "photo"), "photo");
      if ("direction" in item) check.number(item.direction, "direction");
      if ("takenAt" in item) check.pattern(item.takenAt, "takenAt", DATE_TIME);
  }
}

/**
 * マークアップの文書（JSON の文字列か、読んだ値）を検査して読む。知らないキーはそのまま残す。
 * format が違う文書、version が MARKUP_VERSION より新しい文書、キーや値の誤りは MarkupError を投げる。
 * 値は複製するので、渡したオブジェクトを後から変えても結果は変わらない
 */
export function markupFromJson(json: string | object): MarkupDocument {
  // fail の never で型を絞るため、型を書く
  const check: Checker = new Checker();
  let value: unknown;
  try {
    value = JSON.parse(typeof json === "string" ? json : JSON.stringify(json));
  } catch (error) {
    check.fail("json", null, `JSON として読めません（${error instanceof Error ? error.message : String(error)}）`);
  }
  if (!isObject(value) || value.format !== MARKUP_FORMAT)
    check.fail("format", "format", `${MARKUP_FORMAT} ではありません`);
  const version = check.number(check.required(value, "version"), "version");
  if (!Number.isInteger(version) || version < 1) check.fail("value", "version", `使えない版です（${version}）`);
  if (version > MARKUP_VERSION)
    check.fail("version", "version", `版 ${version} は読めません（${MARKUP_VERSION} まで）`);
  const drawing = check.object(check.required(value, "drawing"), "drawing");
  check.pattern(check.required(drawing, "sha256", "drawing.sha256"), "drawing.sha256", SHA256);
  check.string(check.required(drawing, "format", "drawing.format"), "drawing.format", true);
  const items = check.required(value, "items");
  if (!Array.isArray(items)) check.fail("type", "items", "配列ではありません");
  if ("data" in value) check.object(value.data, "data");
  const ids = new Set<string>();
  items.forEach((item: unknown, index) => {
    check.item = index;
    check.id = null;
    if (!isObject(item)) check.fail("type", null, "項目がオブジェクトではありません");
    const object = item;
    const id = check.string(check.required(object, "id"), "id", true);
    check.id = id;
    if (ids.has(id)) check.fail("duplicate", "id", "同じ id の項目がほかにあります");
    ids.add(id);
    const kind = check.oneOf(check.required(object, "kind"), "kind", MARKUP_KINDS);
    checkCommon(check, object);
    checkKind(check, object, kind);
  });
  return value as unknown as MarkupDocument;
}

/** 文書を JSON の文字列にする。知らないキーも書く */
export function markupToJson(document: MarkupDocument, space?: number | string): string {
  return JSON.stringify(document, null, space);
}

// ---- 描く図形

/** 色を指定しない項目の、既定の色 */
export const MARKUP_COLOR = "#ff0000";
/** 雲マークの弧の弦の、既定の長さ（用紙の mm） */
export const MARKUP_ARC_LENGTH = 10;
/** メモの文字の、既定の高さ（用紙の mm） */
export const MARKUP_TEXT_HEIGHT = 3.5;
/** 画面で決まった大きさの部分の 1 px の、既定の大きさ（用紙の mm）。96 dpi の 1 CSS px で、倍率 1 の表示と同じ */
export const MARKUP_PIXEL = 25.4 / 96;

export interface MarkupShapeOptions {
  /** 項目が color を持たないときの色（分類の色など）。#rrggbb か #rrggbbaa。省くと MARKUP_COLOR */
  color?: string;
  /**
   * 画面で決まった大きさに描く部分（ピン・写真の印、lineWidth のない線の太さ、その矢印）の 1 px の大きさ（用紙の mm）。
   * ビューアは今の表示の 1 CSS px を渡す。省くと MARKUP_PIXEL
   */
  pixel?: number;
}

/** lineWidth のない線の太さ（px） */
const LINE_PX = 2;
/** 矢印の長さと、半分の幅（線の太さの倍数） */
const ARROW_LENGTH = 6;
const ARROW_HALF_WIDTH = 2;
/** 雲マークの 1 つの弧の中心角（度）と分割数、1 つの雲の弧の数の上限 */
const CLOUD_SWEEP = 120;
const CLOUD_SEGMENTS = 8;
const CLOUD_MAX_ARCS = 4096;
/** 円を描く多角形の頂点の数 */
const CIRCLE_SEGMENTS = 32;
/** メモの行の間隔（文字の高さの倍数） */
const LINE_PITCH = 1.5;
/** 指摘のピンの半径（px）。番号の文字の高さも同じ */
const PIN_RADIUS = 10;
/** ピンの番号の幅の上限（ピンの半径の倍数） */
const PIN_LABEL_WIDTH = 1.6;
/** 写真の印の半径、レンズの半径、向きの扇の半径と半分の角度（度） */
const PHOTO_RADIUS = 6;
const PHOTO_LENS = 2.5;
const PHOTO_VIEW_RADIUS = 24;
const PHOTO_VIEW_ANGLE = 30;
/** 範囲・閉じた指摘・写真の向きを塗る不透明度 */
const AREA_OPACITY = 0.15;
const CLOSED_OPACITY = 0.4;
const VIEW_OPACITY = 0.3;

/** #rrggbb・#rrggbbaa の不透明度を opacity 倍にした #rrggbbaa。ほかの書き方の色はそのまま */
function withOpacity(color: string, opacity: number): string {
  if (!COLOR.test(color)) return color;
  const alpha = color.length === 9 ? Number.parseInt(color.slice(7), 16) : 255;
  return `${color.slice(0, 7).toLowerCase()}${Math.round(alpha * opacity)
    .toString(16)
    .padStart(2, "0")}`;
}

function stroke(points: readonly number[], closed: boolean, color: string, width: number): StrokePrimitive {
  return { kind: "stroke", points: Float64Array.from(points), closed, color, width, dashes: [] };
}

function fill(ring: readonly number[], color: string): FillPrimitive {
  return { kind: "fill", color, rings: [Float64Array.from(ring)] };
}

function flatten(points: readonly MarkupPoint[]): number[] {
  return points.flat();
}

/** 中心 (x, y)、半径 radius の円の頂点（+x から反時計回り） */
function circle(x: number, y: number, radius: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < CIRCLE_SEGMENTS; i++) {
    const angle = (2 * Math.PI * i) / CIRCLE_SEGMENTS;
    out.push(x + radius * Math.cos(angle), y + radius * Math.sin(angle));
  }
  return out;
}

/** 線の端 points[end] の矢印。向きは、端と違う位置にある最も近い点から端へ */
function arrowhead(points: readonly MarkupPoint[], end: number, step: number, width: number): number[] | null {
  const [tx, ty] = points[end] as MarkupPoint;
  for (let i = end + step; i >= 0 && i < points.length; i += step) {
    const [px, py] = points[i] as MarkupPoint;
    const length = Math.hypot(tx - px, ty - py);
    if (!(length > 0)) continue;
    const dx = (tx - px) / length;
    const dy = (ty - py) / length;
    const bx = tx - dx * ARROW_LENGTH * width;
    const by = ty - dy * ARROW_LENGTH * width;
    const nx = -dy * ARROW_HALF_WIDTH * width;
    const ny = dx * ARROW_HALF_WIDTH * width;
    return [tx, ty, bx + nx, by + ny, bx - nx, by - ny];
  }
  return null;
}

/**
 * 雲マークの外形。各辺を弧の数 round(辺の長さ / 弦の長さ)（1 以上）で等分し、その弦ごとに中心角 120 度の弧を外側へ
 * ふくらませて、8 つに分けた点でつなぐ。外側は外形の向き（面積の符号）で決める。弧の数が 4096 を超えるときは弦を長くする
 */
function cloudOutline(points: readonly MarkupPoint[], arcLength: number): number[] {
  const n = points.length;
  let area = 0;
  let perimeter = 0;
  for (let i = 0; i < n; i++) {
    const [x1, y1] = points[i] as MarkupPoint;
    const [x2, y2] = points[(i + 1) % n] as MarkupPoint;
    area += x1 * y2 - x2 * y1;
    perimeter += Math.hypot(x2 - x1, y2 - y1);
  }
  const chord = Math.max(arcLength, perimeter / CLOUD_MAX_ARCS);
  // 反時計回りの外形なら弧も反時計回りに回る（中心は内側、弧は進む向きの右へふくらむ）
  const turn = ((area >= 0 ? 1 : -1) * CLOUD_SWEEP * Math.PI) / 180 / CLOUD_SEGMENTS;
  const cos = Math.cos(turn);
  const sin = Math.sin(turn);
  const half = (CLOUD_SWEEP * Math.PI) / 360;
  const out: number[] = [];
  for (let i = 0; i < n; i++) {
    const [x1, y1] = points[i] as MarkupPoint;
    const [x2, y2] = points[(i + 1) % n] as MarkupPoint;
    const length = Math.hypot(x2 - x1, y2 - y1);
    if (!(length > 0)) continue;
    const arcs = Math.max(1, Math.round(length / chord));
    // 外側への単位ベクトル。弦の中点から内側へ、弦の半分 / tan(中心角 / 2) の点が弧の中心
    const ox = (area >= 0 ? y2 - y1 : y1 - y2) / length;
    const oy = (area >= 0 ? x1 - x2 : x2 - x1) / length;
    const offset = length / arcs / 2 / Math.tan(half);
    for (let k = 0; k < arcs; k++) {
      const ax = x1 + ((x2 - x1) * k) / arcs;
      const ay = y1 + ((y2 - y1) * k) / arcs;
      const bx = x1 + ((x2 - x1) * (k + 1)) / arcs;
      const by = y1 + ((y2 - y1) * (k + 1)) / arcs;
      const cx = (ax + bx) / 2 - ox * offset;
      const cy = (ay + by) / 2 - oy * offset;
      let vx = ax - cx;
      let vy = ay - cy;
      out.push(ax, ay);
      for (let s = 1; s < CLOUD_SEGMENTS; s++) {
        const rx = vx * cos - vy * sin;
        const ry = vx * sin + vy * cos;
        vx = rx;
        vy = ry;
        out.push(cx + vx, cy + vy);
      }
    }
  }
  return out;
}

function textShapes(item: TextItem, color: string): Primitive[] {
  const height = item.height ?? MARKUP_TEXT_HEIGHT;
  const angle = item.angle ?? 0;
  const radians = (angle * Math.PI) / 180;
  const fontFamily = fontFamilyFor(undefined);
  const out: Primitive[] = [];
  item.text.split(/\r\n|\r|\n/).forEach((line, index) => {
    // 2 行目からは、文字の下の向きへ行の間隔ずつ下げる
    const down = LINE_PITCH * height * index;
    const text = layoutText(
      {
        text: line,
        x: item.position[0] + Math.sin(radians) * down,
        y: item.position[1] - Math.cos(radians) * down,
        height,
        width: 0,
        spacing: 0,
        angle,
        slant: 0,
        anchor: 1,
        vertical: false,
        color,
        fontFamily,
      },
      IDENTITY,
    );
    if (text) out.push(text);
  });
  return out;
}

function findingShapes(item: FindingItem, color: string, width: number, pixel: number): Primitive[] {
  const out: Primitive[] = [];
  if (item.points && item.points.length >= 3) {
    const ring = flatten(item.points);
    out.push(fill(ring, withOpacity(color, AREA_OPACITY)), stroke(ring, true, color, width));
  }
  const [x, y] = item.position;
  const radius = PIN_RADIUS * pixel;
  const pin = circle(x, y, radius);
  // 未対応は塗ったピン、対応済みは輪、確認済みは薄く塗ったピン
  if (item.status === "resolved") out.push(fill(pin, BACKGROUND_COLOR), stroke(pin, true, color, LINE_PX * pixel));
  else out.push(fill(pin, item.status === "closed" ? withOpacity(color, CLOSED_OPACITY) : color));
  if (item.label) {
    const units = Array.from(item.label).reduce((sum, char) => sum + (isHalfWidth(char) ? 0.5 : 1), 0);
    const label = layoutText(
      {
        text: item.label,
        x,
        y,
        height: units > PIN_LABEL_WIDTH ? (radius * PIN_LABEL_WIDTH) / units : radius,
        width: 0,
        spacing: 0,
        angle: 0,
        slant: 0,
        anchor: 5,
        vertical: false,
        color: item.status === "resolved" ? color : BACKGROUND_COLOR,
        fontFamily: fontFamilyFor(undefined),
      },
      IDENTITY,
    );
    if (label) out.push(label);
  }
  return out;
}

function photoShapes(item: PhotoItem, color: string, pixel: number): Primitive[] {
  const out: Primitive[] = [];
  const [x, y] = item.position;
  if (item.direction !== undefined) {
    // 向きの左右 30 度の扇
    const radius = PHOTO_VIEW_RADIUS * pixel;
    const ring = [x, y];
    for (let s = 0; s <= CLOUD_SEGMENTS; s++) {
      const angle = ((item.direction - PHOTO_VIEW_ANGLE + (2 * PHOTO_VIEW_ANGLE * s) / CLOUD_SEGMENTS) * Math.PI) / 180;
      ring.push(x + radius * Math.cos(angle), y + radius * Math.sin(angle));
    }
    out.push(fill(ring, withOpacity(color, VIEW_OPACITY)), stroke(ring, true, color, pixel));
  }
  out.push(fill(circle(x, y, PHOTO_RADIUS * pixel), color), fill(circle(x, y, PHOTO_LENS * pixel), BACKGROUND_COLOR));
  return out;
}

/**
 * 項目を描く図形（用紙の mm）。描く順に並ぶ。規則は docs/markup.md のとおりで、Rust の zumen_core::markup::shapes と
 * 同じ図形を返す。色は項目の color、options.color、MARKUP_COLOR の順に選ぶ
 */
export function markupShapes(item: MarkupItem, options: MarkupShapeOptions = {}): Primitive[] {
  const color = item.color ?? options.color ?? MARKUP_COLOR;
  const pixel = options.pixel ?? MARKUP_PIXEL;
  const width = item.lineWidth ?? LINE_PX * pixel;
  switch (item.kind) {
    case "redline": {
      const closed = item.closed ?? false;
      const out: Primitive[] = [stroke(flatten(item.points), closed, color, width)];
      const arrows = closed ? "none" : (item.arrows ?? "none");
      const head = (end: number, step: number) => {
        const ring = arrowhead(item.points, end, step, width);
        if (ring) out.push(fill(ring, color));
      };
      if (arrows === "start" || arrows === "both") head(0, 1);
      if (arrows === "end" || arrows === "both") head(item.points.length - 1, -1);
      return out;
    }
    case "cloud":
      return [stroke(cloudOutline(item.points, item.arcLength ?? MARKUP_ARC_LENGTH), true, color, width)];
    case "text":
      return textShapes(item, color);
    case "finding":
      return findingShapes(item, color, width, pixel);
    case "photo":
      return photoShapes(item, color, pixel);
  }
}

/** 図形の点を外接矩形に加える。文字は文字範囲の四隅 */
function extendBoxWithPrimitive(box: Box, primitive: Primitive): void {
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
      const { boxWidth: w, boxHeight: h } = primitive;
      extendBoxWithPoints(box, transformPoints(primitive.matrix, [0, 0, w, 0, w, h, 0, h]));
    }
  }
}

/**
 * 項目の外接矩形（用紙の mm）。画面で決まった大きさに描く部分（ピン・写真の印、lineWidth のない線の太さと矢印）を
 * 含まない、点・雲の弧・メモの文字範囲の範囲で、ピンと写真は位置の 1 点になる。新しい版へ移すとき、変わった場所と
 * 重なるかをこれで調べる
 */
export function markupBox(item: MarkupItem): Box {
  const box = emptyBox();
  for (const primitive of markupShapes(item, { pixel: 0 })) extendBoxWithPrimitive(box, primitive);
  return box;
}
