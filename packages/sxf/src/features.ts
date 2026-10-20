import type { SfcRecord, SfcValue } from "./sfc-reader.js";

/**
 * SXF Ver.3.1 のフィーチャ（34 種）。引数の並びは 附属書 SFC 編 1-2 に従う。
 * P21 からも同じ形に変換して、図面の組み立てと描画を共通にする。
 */

interface Base {
  /** インスタンス番号 */
  id: number;
}

export interface Styled extends Base {
  layer: number;
  color: number;
  lineType: number;
  lineWidth: number;
}

export interface ExtensionLine {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface Arrow {
  code: number;
  /** 1: 外向き、2: 内向き */
  direction: number;
  x: number;
  y: number;
  scale: number;
}

export interface Annotation {
  font: number;
  text: string;
  x: number;
  y: number;
  height: number;
  width: number;
  spacing: number;
  angle: number;
  slant: number;
  anchor: number;
  /** 1: 横書き、2: 縦書き */
  direction: number;
}

export interface HatchPattern {
  color: number;
  lineType: number;
  lineWidth: number;
  x: number;
  y: number;
  spacing: number;
  angle: number;
}

export interface Region {
  /** 外形の複合曲線の番号（出現順、1 から） */
  outId: number;
  inIds: number[];
}

export type SxfFeature =
  | (Base & {
      kind: "drawing_attribute";
      values: { key: string; label: string; value: string }[];
    })
  | (Base & { kind: "drawing_sheet"; name: string; type: number; orient: number; x: number; y: number })
  | (Base & { kind: "layer"; name: string; visible: boolean })
  | (Base & { kind: "pre_defined_font"; name: string })
  | (Base & { kind: "user_defined_font"; name: string; pitch: number[] })
  | (Base & { kind: "pre_defined_colour"; name: string })
  | (Base & { kind: "user_defined_colour"; r: number; g: number; b: number })
  | (Base & { kind: "width"; width: number })
  | (Base & { kind: "text_font"; name: string })
  | (Base & {
      kind: "point_marker";
      layer: number;
      color: number;
      x: number;
      y: number;
      code: number;
      angle: number;
      scale: number;
    })
  | (Styled & { kind: "line"; x1: number; y1: number; x2: number; y2: number })
  | (Styled & { kind: "polyline"; xs: number[]; ys: number[] })
  | (Styled & { kind: "circle"; cx: number; cy: number; r: number })
  | (Styled & { kind: "arc"; cx: number; cy: number; r: number; ccw: boolean; start: number; end: number })
  | (Styled & { kind: "ellipse"; cx: number; cy: number; rx: number; ry: number; rotation: number })
  | (Styled & {
      kind: "ellipse_arc";
      cx: number;
      cy: number;
      rx: number;
      ry: number;
      ccw: boolean;
      rotation: number;
      start: number;
      end: number;
    })
  | (Base & { kind: "text_string"; layer: number; color: number } & Annotation)
  | (Styled & { kind: "spline"; closed: boolean; xs: number[]; ys: number[] })
  | (Styled & {
      kind: "clothoid";
      x: number;
      y: number;
      parameter: number;
      ccw: boolean;
      angle: number;
      startLength: number;
      endLength: number;
    })
  | (Base & { kind: "sfig_org"; name: string; flag: number })
  | (Base & {
      kind: "sfig_locate";
      layer: number;
      name: string;
      x: number;
      y: number;
      angle: number;
      ratioX: number;
      ratioY: number;
    })
  | (Base & {
      kind: "externally_defined_symbol";
      layer: number;
      color: number | null;
      name: string;
      x: number;
      y: number;
      angle: number;
      scale: number;
    })
  | (Styled & {
      kind: "linear_dim";
      line: [number, number, number, number];
      ext1: ExtensionLine | null;
      ext2: ExtensionLine | null;
      arrow1: Arrow | null;
      arrow2: Arrow | null;
      text: Annotation | null;
    })
  | (Styled & {
      kind: "curve_dim" | "angular_dim";
      cx: number;
      cy: number;
      radius: number;
      angle0: number;
      angle1: number;
      ext1: ExtensionLine | null;
      ext2: ExtensionLine | null;
      arrow1: Arrow | null;
      arrow2: Arrow | null;
      text: Annotation | null;
    })
  | (Styled & {
      kind: "radius_dim";
      line: [number, number, number, number];
      arrow: Arrow | null;
      text: Annotation | null;
    })
  | (Styled & {
      kind: "diameter_dim";
      line: [number, number, number, number];
      arrow1: Arrow | null;
      arrow2: Arrow | null;
      text: Annotation | null;
    })
  | (Styled & {
      kind: "label";
      xs: number[];
      ys: number[];
      arrowCode: number;
      arrowScale: number;
      text: Annotation | null;
    })
  | (Styled & {
      kind: "balloon";
      xs: number[];
      ys: number[];
      cx: number;
      cy: number;
      radius: number;
      arrowCode: number;
      arrowScale: number;
      text: Annotation | null;
    })
  | (Base & Region & { kind: "externally_defined_hatch"; layer: number; name: string })
  | (Base & Region & { kind: "fill_area_style_colour"; layer: number; color: number })
  | (Base & Region & { kind: "fill_area_style_hatching"; layer: number; patterns: HatchPattern[] })
  | (Base &
      Region & {
        kind: "fill_area_style_tiles";
        layer: number;
        name: string;
        color: number;
        x: number;
        y: number;
        vector1: number;
        vector1Angle: number;
        vector2: number;
        vector2Angle: number;
        scaleX: number;
        scaleY: number;
        angle: number;
      })
  | (Base & { kind: "composite_curve_org"; color: number; lineType: number; lineWidth: number; visible: boolean });

export type FeatureKind = SxfFeature["kind"];

/** フィーチャ名の別名（仕様書の表と見本で名前が違うもの）を本来の種類にそろえる */
const ALIASES: Readonly<Record<string, FeatureKind>> = {
  symbol_externally_defined_feature: "externally_defined_symbol",
  fill_area_style_tiles_hatching_feature: "fill_area_style_tiles",
  composite_curve_feature: "composite_curve_org",
};

export function featureKind(name: string): FeatureKind | null {
  const alias = ALIASES[name];
  if (alias) return alias;
  if (!name.endsWith("_feature")) return null;
  const kind = name.slice(0, -"_feature".length);
  return KINDS.has(kind) ? (kind as FeatureKind) : null;
}

const KINDS = new Set<string>([
  "drawing_attribute",
  "drawing_sheet",
  "layer",
  "pre_defined_font",
  "user_defined_font",
  "pre_defined_colour",
  "user_defined_colour",
  "width",
  "text_font",
  "point_marker",
  "line",
  "polyline",
  "circle",
  "arc",
  "ellipse",
  "ellipse_arc",
  "text_string",
  "spline",
  "clothoid",
  "sfig_org",
  "sfig_locate",
  "externally_defined_symbol",
  "linear_dim",
  "curve_dim",
  "angular_dim",
  "radius_dim",
  "diameter_dim",
  "label",
  "balloon",
  "externally_defined_hatch",
  "fill_area_style_colour",
  "fill_area_style_hatching",
  "fill_area_style_tiles",
  "composite_curve_org",
]);

/** 日本語のフィーチャ名 */
export const FEATURE_LABELS: Readonly<Record<FeatureKind, string>> = {
  drawing_attribute: "図面表題欄",
  drawing_sheet: "用紙",
  layer: "レイヤ",
  pre_defined_font: "既定義線種",
  user_defined_font: "ユーザ定義線種",
  pre_defined_colour: "既定義色",
  user_defined_colour: "ユーザ定義色",
  width: "線幅",
  text_font: "文字フォント",
  point_marker: "点マーカ",
  line: "線分",
  polyline: "折線",
  circle: "円",
  arc: "円弧",
  ellipse: "楕円",
  ellipse_arc: "楕円弧",
  text_string: "文字",
  spline: "スプライン",
  clothoid: "クロソイド",
  sfig_org: "複合図形定義",
  sfig_locate: "複合図形配置",
  externally_defined_symbol: "既定義シンボル",
  linear_dim: "直線寸法",
  curve_dim: "弧長寸法",
  angular_dim: "角度寸法",
  radius_dim: "半径寸法",
  diameter_dim: "直径寸法",
  label: "引出し線",
  balloon: "バルーン",
  externally_defined_hatch: "ハッチング（外部定義）",
  fill_area_style_colour: "ハッチング（塗り）",
  fill_area_style_hatching: "ハッチング（ユーザ定義）",
  fill_area_style_tiles: "ハッチング（パターン）",
  composite_curve_org: "複合曲線定義",
};

class InvalidFeature extends Error {}

/** レコードの引数を型に合わせて取り出す。足りない・数値でない引数は InvalidFeature */
class Args {
  constructor(private readonly values: readonly SfcValue[]) {}

  get length(): number {
    return this.values.length;
  }

  private raw(index: number): SfcValue {
    const value = this.values[index];
    if (value === undefined) throw new InvalidFeature(`引数 ${index + 1} がありません`);
    return value;
  }

  str(index: number): string {
    const value = this.raw(index);
    return typeof value === "string" ? value : value.join(",");
  }

  num(index: number): number {
    const value = Number.parseFloat(this.str(index));
    if (!Number.isFinite(value)) throw new InvalidFeature(`引数 ${index + 1} が数値ではありません`);
    return value;
  }

  int(index: number): number {
    return Math.trunc(this.num(index));
  }

  /** 省略可能な整数（読めなければ fallback） */
  intOr(index: number, fallback: number): number {
    const value = Number.parseFloat(this.values[index] === undefined ? "" : this.str(index));
    return Number.isFinite(value) ? Math.trunc(value) : fallback;
  }

  nums(index: number): number[] {
    const value = this.raw(index);
    const items = typeof value === "string" ? splitCollection(value) : value.map(String);
    return items.map((item) => {
      const parsed = Number.parseFloat(item);
      if (!Number.isFinite(parsed)) throw new InvalidFeature(`引数 ${index + 1} に数値でない値があります`);
      return parsed;
    });
  }
}

function splitCollection(value: string): string[] {
  const trimmed = value.trim().replace(/^\(/, "").replace(/\)$/, "").trim();
  return trimmed === "" ? [] : trimmed.split(",").map((item) => item.trim());
}

function styled(args: Args, id: number): Styled {
  return { id, layer: args.int(0), color: args.int(1), lineType: args.int(2), lineWidth: args.int(3) };
}

function extension(args: Args, at: number): ExtensionLine | null {
  if (args.int(at) === 0) return null;
  return {
    x0: args.num(at + 1),
    y0: args.num(at + 2),
    x1: args.num(at + 3),
    y1: args.num(at + 4),
    x2: args.num(at + 5),
    y2: args.num(at + 6),
  };
}

function arrow(args: Args, at: number): Arrow | null {
  const direction = args.int(at + 1);
  if (direction === 0) return null;
  return { code: args.int(at), direction, x: args.num(at + 2), y: args.num(at + 3), scale: args.num(at + 4) };
}

/** 寸法値・引出し線の文字。at は有無フラグの位置 */
function annotation(args: Args, at: number): Annotation | null {
  if (args.int(at) === 0) return null;
  return {
    font: args.intOr(at + 1, 0),
    text: args.str(at + 2),
    x: args.num(at + 3),
    y: args.num(at + 4),
    height: args.num(at + 5),
    width: args.num(at + 6),
    spacing: args.num(at + 7),
    angle: args.num(at + 8),
    slant: args.num(at + 9),
    anchor: args.int(at + 10),
    direction: args.intOr(at + 11, 1),
  };
}

function region(args: Args, at: number): Region {
  return { outId: args.int(at), inIds: args.nums(at + 2).map(Math.trunc) };
}

/** 図面表題欄の項目の名前と、形式によらないキー（docs/architecture.md の「Stable keys」） */
const TITLE_LABELS = [
  ["businessName", "事業名"],
  ["constructionName", "工事名"],
  ["contractType", "契約区分"],
  ["drawingName", "図面名"],
  ["drawingNumber", "図面番号"],
  ["drawingType", "図面種別"],
  ["scale", "尺度"],
  ["year", "作成年"],
  ["month", "作成月"],
  ["day", "作成日"],
  ["contractor", "受注会社名"],
  ["client", "発注事業者名"],
] as const;

/** SFC のレコードをフィーチャにする。未対応の名前は null、引数の不備は例外 */
export function featureFromRecord(record: SfcRecord): SxfFeature | null {
  const kind = featureKind(record.name);
  if (!kind) return null;
  const a = new Args(record.params);
  const id = record.id;
  try {
    return convert(kind, a, id);
  } catch (error) {
    if (error instanceof InvalidFeature) throw new Error(`${record.name} #${id}: ${error.message}`);
    throw error;
  }
}

function convert(kind: FeatureKind, a: Args, id: number): SxfFeature {
  switch (kind) {
    case "drawing_attribute":
      return {
        id,
        kind,
        values: TITLE_LABELS.map(([key, label], index) => ({
          key,
          label,
          value: index < a.length ? a.str(index).trim() : "",
        })),
      };
    case "drawing_sheet":
      return { id, kind, name: a.str(0), type: a.int(1), orient: a.intOr(2, 1), x: a.num(3), y: a.num(4) };
    case "layer":
      return { id, kind, name: a.str(0), visible: a.intOr(1, 1) !== 0 };
    case "pre_defined_font":
    case "pre_defined_colour":
    case "text_font":
      return { id, kind, name: a.str(0) };
    case "user_defined_font":
      return { id, kind, name: a.str(0), pitch: a.nums(2) };
    case "user_defined_colour":
      return { id, kind, r: a.int(0), g: a.int(1), b: a.int(2) };
    case "width":
      return { id, kind, width: a.num(0) };
    case "point_marker":
      return {
        id,
        kind,
        layer: a.int(0),
        color: a.int(1),
        x: a.num(2),
        y: a.num(3),
        code: a.int(4),
        angle: a.num(5),
        scale: a.num(6),
      };
    case "line":
      return { ...styled(a, id), kind, x1: a.num(4), y1: a.num(5), x2: a.num(6), y2: a.num(7) };
    case "polyline":
      return { ...styled(a, id), kind, xs: a.nums(5), ys: a.nums(6) };
    case "circle":
      return { ...styled(a, id), kind, cx: a.num(4), cy: a.num(5), r: a.num(6) };
    case "arc":
      return {
        ...styled(a, id),
        kind,
        cx: a.num(4),
        cy: a.num(5),
        r: a.num(6),
        ccw: a.int(7) === 0,
        start: a.num(8),
        end: a.num(9),
      };
    case "ellipse":
      return {
        ...styled(a, id),
        kind,
        cx: a.num(4),
        cy: a.num(5),
        rx: a.num(6),
        ry: a.num(7),
        rotation: a.num(8),
      };
    case "ellipse_arc":
      return {
        ...styled(a, id),
        kind,
        cx: a.num(4),
        cy: a.num(5),
        rx: a.num(6),
        ry: a.num(7),
        ccw: a.int(8) === 0,
        rotation: a.num(9),
        start: a.num(10),
        end: a.num(11),
      };
    case "text_string":
      return {
        id,
        kind,
        layer: a.int(0),
        color: a.int(1),
        font: a.int(2),
        text: a.str(3),
        x: a.num(4),
        y: a.num(5),
        height: a.num(6),
        width: a.num(7),
        spacing: a.num(8),
        angle: a.num(9),
        slant: a.num(10),
        anchor: a.int(11),
        direction: a.intOr(12, 1),
      };
    case "spline":
      return { ...styled(a, id), kind, closed: a.int(4) === 0, xs: a.nums(6), ys: a.nums(7) };
    case "clothoid":
      return {
        ...styled(a, id),
        kind,
        x: a.num(4),
        y: a.num(5),
        parameter: a.num(6),
        ccw: a.int(7) === 0,
        angle: a.num(8),
        startLength: a.num(9),
        endLength: a.num(10),
      };
    case "sfig_org":
      return { id, kind, name: a.str(0), flag: a.int(1) };
    case "sfig_locate":
      return {
        id,
        kind,
        layer: a.int(0),
        name: a.str(1),
        x: a.num(2),
        y: a.num(3),
        angle: a.num(4),
        ratioX: a.num(5),
        ratioY: a.num(6),
      };
    case "externally_defined_symbol":
      return {
        id,
        kind,
        layer: a.int(0),
        color: a.int(1) === 0 ? null : a.int(2),
        name: a.str(3),
        x: a.num(4),
        y: a.num(5),
        angle: a.num(6),
        scale: a.num(7),
      };
    case "linear_dim":
      return {
        ...styled(a, id),
        kind,
        line: [a.num(4), a.num(5), a.num(6), a.num(7)],
        ext1: extension(a, 8),
        ext2: extension(a, 15),
        arrow1: arrow(a, 22),
        arrow2: arrow(a, 27),
        text: annotation(a, 32),
      };
    case "curve_dim":
    case "angular_dim":
      return {
        ...styled(a, id),
        kind,
        cx: a.num(4),
        cy: a.num(5),
        radius: a.num(6),
        angle0: a.num(7),
        angle1: a.num(8),
        ext1: extension(a, 9),
        ext2: extension(a, 16),
        arrow1: arrow(a, 23),
        arrow2: arrow(a, 28),
        text: annotation(a, 33),
      };
    case "radius_dim":
      return {
        ...styled(a, id),
        kind,
        line: [a.num(4), a.num(5), a.num(6), a.num(7)],
        arrow: arrow(a, 8),
        text: annotation(a, 13),
      };
    case "diameter_dim":
      return {
        ...styled(a, id),
        kind,
        line: [a.num(4), a.num(5), a.num(6), a.num(7)],
        arrow1: arrow(a, 8),
        arrow2: arrow(a, 13),
        text: annotation(a, 18),
      };
    case "label":
      return {
        ...styled(a, id),
        kind,
        xs: a.nums(5),
        ys: a.nums(6),
        arrowCode: a.int(7),
        arrowScale: a.num(8),
        text: annotation(a, 9),
      };
    case "balloon":
      return {
        ...styled(a, id),
        kind,
        xs: a.nums(5),
        ys: a.nums(6),
        cx: a.num(7),
        cy: a.num(8),
        radius: a.num(9),
        arrowCode: a.int(10),
        arrowScale: a.num(11),
        text: annotation(a, 12),
      };
    case "externally_defined_hatch":
      return { id, kind, layer: a.int(0), name: a.str(1), ...region(a, 2) };
    case "fill_area_style_colour":
      return { id, kind, layer: a.int(0), color: a.int(1), ...region(a, 2) };
    case "fill_area_style_hatching": {
      const count = a.int(1);
      const patterns: HatchPattern[] = [];
      for (let i = 0; i < count; i++) {
        const [color, lineType, lineWidth, x, y, spacing, angle] = a.nums(2 + i);
        if (angle === undefined) throw new InvalidFeature("ハッチング線の値が足りません");
        patterns.push({
          color: Math.trunc(color as number),
          lineType: Math.trunc(lineType as number),
          lineWidth: Math.trunc(lineWidth as number),
          x: x as number,
          y: y as number,
          spacing: spacing as number,
          angle,
        });
      }
      return { id, kind, layer: a.int(0), patterns, ...region(a, 2 + count) };
    }
    case "fill_area_style_tiles":
      return {
        id,
        kind,
        layer: a.int(0),
        name: a.str(1),
        color: a.int(2),
        x: a.num(3),
        y: a.num(4),
        vector1: a.num(5),
        vector1Angle: a.num(6),
        vector2: a.num(7),
        vector2Angle: a.num(8),
        scaleX: a.num(9),
        scaleY: a.num(10),
        angle: a.num(11),
        ...region(a, 12),
      };
    case "composite_curve_org":
      return { id, kind, color: a.int(0), lineType: a.int(1), lineWidth: a.int(2), visible: a.intOr(3, 1) !== 0 };
  }
}
