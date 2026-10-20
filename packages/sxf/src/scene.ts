import {
  BACKGROUND_COLOR,
  type Box,
  type EntityKind,
  NoticeList,
  type Placement,
  type Primitive,
  type Property,
  type Scene,
  type SceneEntity,
  type SceneLayer,
  type Sheet,
  type StrokePrimitive,
} from "@zumenjs/core";
import {
  apply,
  applyVector,
  arcSegments,
  bezierPoints,
  clothoidPoints,
  ellipseArcPoints,
  emptyBox,
  extendBox,
  extendBoxWithPoints,
  IDENTITY,
  invert,
  isEmptyBox,
  type Matrix,
  meanScale,
  multiply,
  placement,
  rotationDeg,
  sweepRadians,
  transformPoints,
  unionBox,
} from "@zumenjs/core/geometry";
import { fontFamilyFor, layoutText } from "@zumenjs/core/text";
import type { SxfDocument, SxfDrawing } from "./document.js";
import {
  type Annotation,
  type Arrow,
  type ExtensionLine,
  FEATURE_LABELS,
  type FeatureKind,
  type SxfFeature,
} from "./features.js";
import { arrowPrimitives, markerPrimitives } from "./symbols.js";
import {
  ARROW_NAMES,
  MARKER_NAMES,
  PREDEFINED_COLORS,
  PREDEFINED_LINE_TYPES,
  PREDEFINED_WIDTHS,
  SFIG_KINDS,
  SHEET_SIZES,
} from "./tables.js";

const DEFAULT_COLOR = "#ffffff";
const MAX_DEPTH = 32;
/** Scene のレイヤの id の上限（Rust の読み取りと同じ 32 ビットの符号なし整数） */
const MAX_LAYER_CODE = 0xffffffff;
const MAX_HATCH_LINES = 20_000;

function hex(r: number, g: number, b: number): string {
  const clamp = (value: number) => Math.max(0, Math.min(255, Math.round(value)));
  return `#${[r, g, b].map((value) => clamp(value).toString(16).padStart(2, "0")).join("")}`;
}

const formatters = new Map<number, Intl.NumberFormat>();

/** 属性に表示する数値。要素の数だけ呼ぶので、書式は桁数ごとに使い回す */
function format(value: number, digits = 3): string {
  if (!Number.isFinite(value)) return String(value);
  let formatter = formatters.get(digits);
  if (!formatter) {
    formatter = new Intl.NumberFormat("ja-JP", { maximumFractionDigits: digits });
    formatters.set(digits, formatter);
  }
  return formatter.format(value);
}

/** 座標の属性。value は整形した「x, y」、raw は [x, y] */
function point(key: string, label: string, x: number, y: number): Property {
  return { key, label, value: `${format(x)}, ${format(y)}`, raw: [x, y] };
}

/** 数の属性 */
function number(key: string, label: string, value: number): Property {
  return { key, label, value: format(value), raw: value };
}

/** SXF のフィーチャの、形式によらない種類（docs/architecture.md の「Stable keys」） */
const ENTITY_KINDS: Partial<Record<FeatureKind, EntityKind>> = {
  point_marker: "point",
  line: "line",
  polyline: "polyline",
  circle: "circle",
  arc: "arc",
  ellipse: "ellipse",
  ellipse_arc: "ellipse-arc",
  text_string: "text",
  spline: "spline",
  clothoid: "clothoid",
  externally_defined_symbol: "symbol",
  linear_dim: "dimension",
  curve_dim: "dimension",
  angular_dim: "dimension",
  radius_dim: "dimension",
  diameter_dim: "dimension",
  label: "leader",
  balloon: "balloon",
  externally_defined_hatch: "hatch",
  fill_area_style_colour: "hatch",
  fill_area_style_hatching: "hatch",
  fill_area_style_tiles: "hatch",
};

/** 寸法の種類の、形式によらないキー（DXF・DWG の寸法と同じ値） */
const DIMENSION_TYPES = {
  linear_dim: "linear",
  curve_dim: "arc-length",
  angular_dim: "angular",
  radius_dim: "radius",
  diameter_dim: "diameter",
} as const;

/** 色・線種・線幅・フォントのコードを値にする（附属書 SFC 編 1-3-8〜1-3-18） */
class Styles {
  constructor(
    private readonly doc: SxfDocument,
    private readonly notices: NoticeList,
  ) {}

  color(code: number): string {
    const predefined = PREDEFINED_COLORS[code - 1];
    if (predefined) return hex(...predefined.rgb);
    const user = this.doc.userColors[code - 17];
    if (code >= 17 && user) return hex(user.r, user.g, user.b);
    this.notices.add("warning", "unknown-color", "定義のない色コードを白で表示しました");
    return DEFAULT_COLOR;
  }

  colorLabel(code: number): string {
    const predefined = PREDEFINED_COLORS[code - 1];
    if (predefined) return `${predefined.label}（${code}）`;
    const user = this.doc.userColors[code - 17];
    if (code >= 17 && user) return `ユーザ定義 RGB(${user.r}, ${user.g}, ${user.b})（${code}）`;
    return `未定義（${code}）`;
  }

  dashes(code: number): number[] {
    if (code >= 1 && code <= 16) return [...(PREDEFINED_LINE_TYPES[code - 1]?.pitch ?? [])];
    const user = this.doc.userLineTypes[code - 17];
    if (code >= 17 && user) {
      return user.pitch.every((value) => value > 0) && user.pitch.length % 2 === 0 ? user.pitch : [];
    }
    this.notices.add("warning", "unknown-line-type", "定義のない線種コードを実線で表示しました");
    return [];
  }

  lineTypeLabel(code: number): string {
    const predefined = PREDEFINED_LINE_TYPES[code - 1];
    if (predefined && code <= 15) return `${predefined.label}（${code}）`;
    const user = this.doc.userLineTypes[code - 17];
    if (code >= 17 && user) return `${user.name}（${code}）`;
    return `未定義（${code}）`;
  }

  width(code: number): number {
    const predefined = PREDEFINED_WIDTHS[code - 1];
    if (predefined !== undefined) return predefined;
    const user = this.doc.userWidths[code - 11];
    if (code >= 11 && user) return user.width;
    if (code !== 0) this.notices.add("warning", "unknown-width", "定義のない線幅コードを 0.13mm で表示しました");
    return 0.13;
  }

  font(code: number): string | undefined {
    return this.doc.fonts[code - 1]?.name;
  }

  layerName(code: number): string {
    if (code === 0) return "なし";
    return this.doc.layers[code - 1]?.name ?? `未定義（${code}）`;
  }
}

interface Context {
  /** 元の座標（表示の向き）から用紙への変換 */
  m: Matrix;
  /** 測地座標系の部分図の中なら、座標の X と Y を入れ替えて表示の向きにする */
  geodetic: boolean;
  layers: readonly number[];
  path: string;
  placement?: Placement;
  stack: readonly string[];
}

type SxfMeta = Pick<SxfDrawing, "format" | "version" | "header">;

export interface SxfSceneOptions {
  /** 円弧・曲線を折線にするときの許容誤差（用紙の mm） */
  tolerance?: number;
}

/** SXF の図面を、用紙の座標に解決した表示用の図面にする。読み取りの注意に、描画で省いた内容を加える */
export function sxfToScene(drawing: SxfDrawing, options: SxfSceneOptions = {}): Scene {
  const doc = drawing.document;
  const meta: SxfMeta = drawing;
  const notices = new NoticeList(drawing.notices);
  const builder = new SceneBuilder(doc, new Styles(doc, notices), notices, options.tolerance ?? 0.02);
  builder.run();
  const sheet = sheetOf(doc);
  const bounds = emptyBox();
  for (const entity of builder.entities) unionBox(bounds, entity.bbox);
  if (sheet) {
    extendBox(bounds, 0, 0);
    extendBox(bounds, sheet.width, sheet.height);
  }
  const layers: SceneLayer[] = doc.layers.map((layer, index) => ({
    id: index + 1,
    name: layer.name || `レイヤ ${index + 1}`,
    visible: layer.visible,
  }));
  for (const code of builder.layerCodes) {
    if (code > 0 && !layers.some((layer) => layer.id === code)) {
      layers.push({ id: code, name: `未定義のレイヤ ${code}`, visible: true });
      notices.add("warning", "unknown-layer", "定義のないレイヤコードを表示中のレイヤとして扱いました");
    }
  }
  layers.sort((a, b) => a.id - b.id);
  return {
    format: meta.format,
    version: meta.version,
    sheet,
    bounds,
    layers,
    entities: builder.entities,
    notices: notices.toArray(),
    metadata: metadataOf(doc, meta, sheet),
    background: builder.background,
  };
}

function sheetOf(doc: SxfDocument): Sheet | null {
  const feature = doc.sheet;
  if (!feature) return null;
  const size = SHEET_SIZES[feature.type];
  if (!size) {
    return { name: feature.name, size: "自由", width: Math.max(feature.x, 1), height: Math.max(feature.y, 1) };
  }
  const landscape = feature.orient !== 0;
  return {
    name: feature.name,
    size: `${size.label}${landscape ? " 横" : " 縦"}`,
    width: landscape ? size.height : size.width,
    height: landscape ? size.width : size.height,
  };
}

function metadataOf(doc: SxfDocument, meta: SxfMeta, sheet: Sheet | null): Property[] {
  const items: Property[] = [
    { key: "format", label: "形式", value: `${meta.format.toUpperCase()}（${meta.version}）` },
  ];
  if (sheet) {
    const name = sheet.name.trim() ? `${sheet.name}・` : "";
    items.push({ key: "sheet", label: "用紙", value: `${name}${sheet.size}（${sheet.width}×${sheet.height}mm）` });
  }
  for (const { key, label, value } of doc.title?.values ?? []) {
    if (value && value !== " ") items.push({ key, label, value });
  }
  if (meta.header.originatingSystem) {
    items.push({ key: "software", label: "作成ソフト", value: meta.header.originatingSystem });
  }
  if (meta.header.preprocessor) {
    items.push({ key: "library", label: "出力ライブラリ", value: meta.header.preprocessor });
  }
  const figures = [...doc.subFigures.values()];
  const partial = figures.filter((figure) => figure.feature.flag === 1 || figure.feature.flag === 2).length;
  if (figures.length > 0) {
    items.push({
      key: "figureCount",
      label: "複合図形",
      value: `${figures.length} 件（部分図 ${partial} 件）`,
      raw: [figures.length, partial],
    });
  }
  return items;
}

class SceneBuilder {
  readonly entities: SceneEntity[] = [];
  readonly layerCodes = new Set<number>();
  background: "dark" | "light" = "dark";

  constructor(
    private readonly doc: SxfDocument,
    private readonly styles: Styles,
    private readonly notices: NoticeList,
    private readonly tolerance: number,
  ) {}

  run(): void {
    for (const figure of this.doc.subFigures.values()) {
      const match = /\$\$背景色\$\$色\$\$(\d+)_(\d+)_(\d+)/.exec(figure.feature.name);
      if (match) {
        const luminance = Number(match[1]) * 0.299 + Number(match[2]) * 0.587 + Number(match[3]) * 0.114;
        this.background = luminance > 128 ? "light" : "dark";
      }
      if (figure.feature.name.includes("$$RASTER$$")) {
        this.notices.add("info", "raster", "ラスタ画像（別の TIFF・JPEG ファイル）は表示していません");
      }
    }
    const root: Context = { m: IDENTITY, geodetic: false, layers: [], path: "", stack: [] };
    for (const feature of this.doc.sheetMembers) this.emit(feature, root);
  }

  /** 元の座標を表示の向きにして用紙へ写す */
  private point(ctx: Context, x: number, y: number): [number, number] {
    return ctx.geodetic ? apply(ctx.m, y, x) : apply(ctx.m, x, y);
  }

  private local(ctx: Context, x: number, y: number): [number, number] {
    return ctx.geodetic ? [y, x] : [x, y];
  }

  private localPoints(ctx: Context, xs: readonly number[], ys: readonly number[]): number[] {
    const out: number[] = [];
    const count = Math.min(xs.length, ys.length);
    for (let i = 0; i < count; i++) out.push(...this.local(ctx, xs[i] as number, ys[i] as number));
    return out;
  }

  /** 元の座標での弦の許容誤差 */
  private localTolerance(ctx: Context): number {
    return this.tolerance / Math.max(meanScale(ctx.m), 1e-12);
  }

  private stroke(
    ctx: Context,
    localPoints: ArrayLike<number>,
    closed: boolean,
    color: number,
    lineType: number,
    lineWidth: number,
  ): StrokePrimitive {
    return {
      kind: "stroke",
      points: transformPoints(ctx.m, localPoints),
      closed,
      color: this.styles.color(color),
      width: this.styles.width(lineWidth),
      dashes: this.styles.dashes(lineType),
    };
  }

  private add(feature: SxfFeature, ctx: Context, layer: number, primitives: Primitive[], properties: Property[]): void {
    if (primitives.length === 0) return;
    const bbox = emptyBox();
    for (const primitive of primitives) extendPrimitiveBox(bbox, primitive);
    if (isEmptyBox(bbox)) return;
    const layers = [...ctx.layers];
    const own = this.layerCode(layer);
    if (own > 0) layers.push(own);
    for (const code of layers) this.layerCodes.add(code);
    const head: Property[] = [
      { key: "type", label: "種類", value: FEATURE_LABELS[feature.kind] },
      { key: "id", label: "番号", value: `${ctx.path}#${feature.id}` },
      { key: "layer", label: "レイヤ", value: this.styles.layerName(layer), raw: own },
    ];
    if (ctx.placement) {
      head.push({
        key: "placement",
        label: "配置",
        value: `${ctx.placement.name}（縮尺 ${scaleLabel(ctx.placement.scaleX)}${
          Math.abs(ctx.placement.scaleX - ctx.placement.scaleY) > 1e-12
            ? `・縦 ${scaleLabel(ctx.placement.scaleY)}`
            : ""
        }）`,
      });
    }
    this.entities.push({
      id: `${ctx.path}#${feature.id}`,
      kind: ENTITY_KINDS[feature.kind] as EntityKind,
      type: FEATURE_LABELS[feature.kind],
      source: `${feature.kind}_feature`,
      layers,
      primitives,
      bbox,
      properties: [...head, ...properties],
      ...(ctx.placement ? { placement: ctx.placement } : {}),
    });
  }

  /** Scene のレイヤの id にできないほど大きいレイヤコードは、レイヤなし（0）として扱う */
  private layerCode(code: number): number {
    if (code <= MAX_LAYER_CODE) return code;
    this.notices.add("warning", "layer-range", "範囲外のレイヤコードを、レイヤなしとして扱いました");
    return 0;
  }

  private colorProperty(color: number): Property {
    return { key: "color", label: "色", value: this.styles.colorLabel(color), raw: color };
  }

  private styleProperties(color: number, lineType: number, lineWidth: number): Property[] {
    const width = this.styles.width(lineWidth);
    return [
      this.colorProperty(color),
      { key: "lineType", label: "線種", value: this.styles.lineTypeLabel(lineType), raw: lineType },
      { key: "lineWidth", label: "線幅", value: `${format(width)}mm（${lineWidth}）`, raw: width },
    ];
  }

  private emit(feature: SxfFeature, ctx: Context): void {
    const tol = this.localTolerance(ctx);
    switch (feature.kind) {
      case "line": {
        const points = [...this.local(ctx, feature.x1, feature.y1), ...this.local(ctx, feature.x2, feature.y2)];
        this.add(
          feature,
          ctx,
          feature.layer,
          [this.stroke(ctx, points, false, feature.color, feature.lineType, feature.lineWidth)],
          [
            ...this.styleProperties(feature.color, feature.lineType, feature.lineWidth),
            point("start", "始点", feature.x1, feature.y1),
            point("end", "終点", feature.x2, feature.y2),
            number("length", "長さ", Math.hypot(feature.x2 - feature.x1, feature.y2 - feature.y1)),
          ],
        );
        return;
      }
      case "polyline": {
        const points = this.localPoints(ctx, feature.xs, feature.ys);
        this.add(
          feature,
          ctx,
          feature.layer,
          [this.stroke(ctx, points, false, feature.color, feature.lineType, feature.lineWidth)],
          [
            ...this.styleProperties(feature.color, feature.lineType, feature.lineWidth),
            { key: "vertexCount", label: "頂点数", value: String(points.length / 2), raw: points.length / 2 },
          ],
        );
        return;
      }
      case "circle":
      case "arc":
      case "ellipse":
      case "ellipse_arc": {
        const [cx, cy] = this.local(ctx, feature.cx, feature.cy);
        const rx = feature.kind === "circle" || feature.kind === "arc" ? feature.r : feature.rx;
        const ry = feature.kind === "circle" || feature.kind === "arc" ? feature.r : feature.ry;
        const rotation = feature.kind === "ellipse" || feature.kind === "ellipse_arc" ? feature.rotation : 0;
        const full = feature.kind === "circle" || feature.kind === "ellipse";
        const start = full ? 0 : feature.start;
        const end = full ? 360 : feature.end;
        const ccw = full ? true : feature.ccw;
        const sweep = full ? Math.PI * 2 : sweepRadians(start, end, ccw);
        const segments = arcSegments(Math.max(rx, ry), sweep, tol);
        let points = ellipseArcPoints(cx, cy, rx, ry, rotation, start, end, ccw, segments);
        if (full) points = points.slice(0, -2);
        const properties: Property[] = [
          ...this.styleProperties(feature.color, feature.lineType, feature.lineWidth),
          point("center", "中心", feature.cx, feature.cy),
          feature.kind === "circle" || feature.kind === "arc"
            ? number("radius", "半径", feature.r)
            : {
                key: "radii",
                label: "半径（X・Y）",
                value: `${format(feature.rx)}・${format(feature.ry)}`,
                raw: [feature.rx, feature.ry],
              },
        ];
        if (!full) {
          properties.push({
            key: "angle",
            label: "角度",
            value: `${format(start)}° → ${format(end)}°（${ccw ? "反時計回り" : "時計回り"}）`,
            raw: ccw ? [start, end] : [end, start],
          });
        }
        this.add(
          feature,
          ctx,
          feature.layer,
          [this.stroke(ctx, points, full, feature.color, feature.lineType, feature.lineWidth)],
          properties,
        );
        return;
      }
      case "spline": {
        const control = this.localPoints(ctx, feature.xs, feature.ys);
        if (control.length / 2 > 1 && (control.length / 2 - 1) % 3 !== 0) {
          this.notices.add("warning", "spline-points", "頂点数が 3n+1 でないスプラインの余りの点を折線でつなぎました");
        }
        const points = bezierPoints(control, 24);
        this.add(
          feature,
          ctx,
          feature.layer,
          [this.stroke(ctx, points, false, feature.color, feature.lineType, feature.lineWidth)],
          [
            ...this.styleProperties(feature.color, feature.lineType, feature.lineWidth),
            {
              key: "controlPointCount",
              label: "制御点数",
              value: String(control.length / 2),
              raw: control.length / 2,
            },
          ],
        );
        return;
      }
      case "clothoid": {
        const [x, y] = this.local(ctx, feature.x, feature.y);
        const length = Math.abs(feature.endLength - feature.startLength);
        const segments = Math.min(Math.max(Math.ceil(length / Math.max(tol * 20, 1e-6)), 16), 2048);
        const points = clothoidPoints(
          x,
          y,
          feature.parameter,
          feature.ccw,
          feature.angle,
          feature.startLength,
          feature.endLength,
          segments,
        );
        this.add(
          feature,
          ctx,
          feature.layer,
          [this.stroke(ctx, points, false, feature.color, feature.lineType, feature.lineWidth)],
          [
            ...this.styleProperties(feature.color, feature.lineType, feature.lineWidth),
            number("parameter", "パラメータ A", feature.parameter),
            {
              key: "curveLength",
              label: "曲線長",
              value: `${format(feature.startLength)} → ${format(feature.endLength)}`,
              raw: [feature.startLength, feature.endLength],
            },
          ],
        );
        return;
      }
      case "point_marker": {
        const [x, y] = this.point(ctx, feature.x, feature.y);
        const primitives = markerPrimitives(
          feature.code,
          x,
          y,
          feature.angle + rotationDeg(ctx.m),
          feature.scale,
          this.styles.color(feature.color),
          0,
        );
        this.add(feature, ctx, feature.layer, primitives, [
          this.colorProperty(feature.color),
          {
            key: "marker",
            label: "マーカ",
            value: MARKER_NAMES[feature.code] ?? String(feature.code),
            raw: feature.code,
          },
          point("position", "位置", feature.x, feature.y),
        ]);
        return;
      }
      case "text_string": {
        const text = this.text(ctx, feature, feature.color);
        if (!text) return;
        this.add(
          feature,
          ctx,
          feature.layer,
          [text],
          [
            { key: "text", label: "文字列", value: feature.text },
            this.colorProperty(feature.color),
            { key: "font", label: "フォント", value: this.styles.font(feature.font) ?? `未定義（${feature.font}）` },
            {
              key: "textBox",
              label: "文字範囲",
              value: `高さ ${format(feature.height)}・幅 ${format(feature.width)}`,
              raw: [feature.height, feature.width],
            },
            point("position", "位置", feature.x, feature.y),
            { key: "rotation", label: "回転角", value: `${format(feature.angle)}°`, raw: feature.angle },
          ],
        );
        return;
      }
      case "sfig_locate":
        this.locate(feature, ctx);
        return;
      case "externally_defined_symbol": {
        const [x, y] = this.point(ctx, feature.x, feature.y);
        const color = feature.color === null ? DEFAULT_COLOR : this.styles.color(feature.color);
        const label = layoutText(
          {
            text: feature.name || "?",
            x,
            y,
            height: 2.5 * (feature.scale > 0 ? feature.scale : 1),
            width: 0,
            spacing: 0,
            angle: feature.angle + rotationDeg(ctx.m),
            slant: 0,
            anchor: 5,
            vertical: false,
            color,
            fontFamily: fontFamilyFor(undefined),
          },
          IDENTITY,
        );
        this.notices.add("info", "symbol", "既定義シンボルは形の代わりに名前を表示しました");
        this.add(feature, ctx, feature.layer, label ? [label] : [], [
          { key: "name", label: "シンボル名", value: feature.name },
          point("position", "位置", feature.x, feature.y),
        ]);
        return;
      }
      case "linear_dim":
      case "radius_dim":
      case "diameter_dim":
      case "curve_dim":
      case "angular_dim":
        this.dimension(feature, ctx);
        return;
      case "label":
      case "balloon":
        this.leader(feature, ctx);
        return;
      case "fill_area_style_colour":
      case "fill_area_style_hatching":
      case "fill_area_style_tiles":
      case "externally_defined_hatch":
        this.hatch(feature, ctx);
        return;
      default:
        return;
    }
  }

  private text(
    ctx: Context,
    value: Annotation | (Annotation & { layer: number }),
    color: number,
  ): ReturnType<typeof layoutText> {
    const [x, y] = this.local(ctx, value.x, value.y);
    if (!value.text) return null;
    return layoutText(
      {
        text: value.text,
        x,
        y,
        height: value.height,
        width: value.width,
        spacing: value.spacing,
        angle: value.angle,
        slant: value.slant,
        anchor: value.anchor,
        vertical: value.direction === 2,
        color: this.styles.color(color),
        fontFamily: fontFamilyFor(this.styles.font(value.font)),
      },
      ctx.m,
    );
  }

  private locate(feature: Extract<SxfFeature, { kind: "sfig_locate" }>, ctx: Context): void {
    const figure = this.doc.subFigures.get(feature.name);
    if (!figure) {
      this.notices.add("warning", "missing-sfig", "定義のない複合図形の配置を省きました");
      return;
    }
    if (ctx.stack.includes(feature.name) || ctx.stack.length >= MAX_DEPTH) {
      this.notices.add("warning", "sfig-cycle", "循環または深すぎる複合図形の配置を省きました");
      return;
    }
    const flag = figure.feature.flag;
    const [x, y] = this.local(ctx, feature.x, feature.y);
    // 作図グループは配置位置 (0, 0)・回転 0・尺度 1 とみなす（フィーチャ仕様編 3-5 補足）
    const local = flag === 3 ? IDENTITY : placement(x, y, feature.angle, feature.ratioX, feature.ratioY);
    const m = multiply(ctx.m, local);
    const own = this.layerCode(feature.layer);
    const layers = own > 0 ? [...ctx.layers, own] : ctx.layers;
    for (const code of layers) this.layerCodes.add(code);
    let nextPlacement = ctx.placement;
    if (flag !== 3) {
      const scale = meanScale(local);
      nextPlacement = {
        name: `${SFIG_KINDS[flag] ?? "複合図形"} ${displayName(feature.name)}`,
        scaleX: Math.hypot(m[0], m[1]),
        scaleY: Math.hypot(m[2], m[3]),
        inverse: invert(m),
      };
      if (!(scale > 0)) return;
    }
    const child: Context = {
      m,
      geodetic: flag === 2 ? true : flag === 1 ? false : ctx.geodetic,
      layers,
      path: `${ctx.path}#${feature.id}>`,
      stack: [...ctx.stack, feature.name],
      ...(nextPlacement ? { placement: nextPlacement } : {}),
    };
    for (const member of figure.members) this.emit(member, child);
  }

  /** 矢印の先端と向き。向きは、寸法線の近い端から遠い端と反対（外向き）を基本にする */
  private arrowAt(
    ctx: Context,
    arrow: Arrow | null,
    a: [number, number],
    b: [number, number],
    color: string,
  ): Primitive[] {
    if (!arrow || arrow.code <= 0) return [];
    const [tx, ty] = this.point(ctx, arrow.x, arrow.y);
    const [ax, ay] = apply(ctx.m, ...a);
    const [bx, by] = apply(ctx.m, ...b);
    const nearA = Math.hypot(tx - ax, ty - ay) <= Math.hypot(tx - bx, ty - by);
    let ux = nearA ? ax - bx : bx - ax;
    let uy = nearA ? ay - by : by - ay;
    const length = Math.hypot(ux, uy);
    if (!(length > 0)) return [];
    ux /= length;
    uy /= length;
    if (arrow.direction === 2) {
      ux = -ux;
      uy = -uy;
    }
    return arrowPrimitives(arrow.code, tx, ty, ux, uy, arrow.scale, color, 0, BACKGROUND_COLOR);
  }

  /** 円弧の寸法線の矢印。矢印の位置での接線方向（始角側は時計回り、終角側は反時計回りが外向き） */
  private arcArrow(
    ctx: Context,
    arrow: Arrow | null,
    center: [number, number],
    startSide: boolean,
    color: string,
  ): Primitive[] {
    if (!arrow || arrow.code <= 0) return [];
    const [lx, ly] = this.local(ctx, arrow.x, arrow.y);
    const angle = Math.atan2(ly - center[1], lx - center[0]);
    let tx = startSide ? Math.sin(angle) : -Math.sin(angle);
    let ty = startSide ? -Math.cos(angle) : Math.cos(angle);
    if (arrow.direction === 2) {
      tx = -tx;
      ty = -ty;
    }
    const [ux, uy] = applyVector(ctx.m, tx, ty);
    const length = Math.hypot(ux, uy);
    if (!(length > 0)) return [];
    const [px, py] = apply(ctx.m, lx, ly);
    return arrowPrimitives(arrow.code, px, py, ux / length, uy / length, arrow.scale, color, 0, BACKGROUND_COLOR);
  }

  private extension(ctx: Context, line: ExtensionLine | null, style: [number, number, number]): Primitive[] {
    if (!line) return [];
    const points = [...this.local(ctx, line.x1, line.y1), ...this.local(ctx, line.x2, line.y2)];
    return [this.stroke(ctx, points, false, ...style)];
  }

  private dimension(
    feature: Extract<SxfFeature, { kind: "linear_dim" | "radius_dim" | "diameter_dim" | "curve_dim" | "angular_dim" }>,
    ctx: Context,
  ): void {
    const style: [number, number, number] = [feature.color, feature.lineType, feature.lineWidth];
    const color = this.styles.color(feature.color);
    const primitives: Primitive[] = [];
    const properties = this.styleProperties(...style);
    if (!("line" in feature)) {
      const center = this.local(ctx, feature.cx, feature.cy);
      const sweep = sweepRadians(feature.angle0, feature.angle1, true);
      const points = ellipseArcPoints(
        center[0],
        center[1],
        feature.radius,
        feature.radius,
        0,
        feature.angle0,
        feature.angle1,
        true,
        arcSegments(feature.radius, sweep, this.localTolerance(ctx)),
      );
      primitives.push(this.stroke(ctx, points, false, ...style));
      primitives.push(...this.extension(ctx, feature.ext1, style), ...this.extension(ctx, feature.ext2, style));
      primitives.push(...this.arcArrow(ctx, feature.arrow1, center, true, color));
      primitives.push(...this.arcArrow(ctx, feature.arrow2, center, false, color));
      properties.push({
        key: "dimensionArc",
        label: "中心・半径",
        value: `${format(feature.cx)}, ${format(feature.cy)}・${format(feature.radius)}`,
        raw: [feature.cx, feature.cy, feature.radius],
      });
    } else {
      const [x1, y1, x2, y2] = feature.line;
      const a = this.local(ctx, x1, y1);
      const b = this.local(ctx, x2, y2);
      primitives.push(this.stroke(ctx, [...a, ...b], false, ...style));
      if (feature.kind === "linear_dim") {
        primitives.push(...this.extension(ctx, feature.ext1, style), ...this.extension(ctx, feature.ext2, style));
        primitives.push(
          ...this.arrowAt(ctx, feature.arrow1, a, b, color),
          ...this.arrowAt(ctx, feature.arrow2, a, b, color),
        );
      } else if (feature.kind === "radius_dim") {
        primitives.push(...this.arrowAt(ctx, feature.arrow, a, b, color));
      } else {
        primitives.push(
          ...this.arrowAt(ctx, feature.arrow1, a, b, color),
          ...this.arrowAt(ctx, feature.arrow2, a, b, color),
        );
      }
      properties.push({
        key: "dimensionLine",
        label: "寸法線",
        value: `${format(x1)}, ${format(y1)} → ${format(x2)}, ${format(y2)}`,
        raw: [x1, y1, x2, y2],
      });
    }
    if (feature.text) {
      const text = this.text(ctx, feature.text, feature.color);
      if (text) primitives.push(text);
      properties.unshift({ key: "dimensionText", label: "寸法値", value: feature.text.text });
    }
    properties.unshift({
      key: "dimensionType",
      label: "寸法の種類",
      value: FEATURE_LABELS[feature.kind],
      raw: DIMENSION_TYPES[feature.kind],
    });
    const arrows = [
      "arrow" in feature ? feature.arrow : null,
      "arrow1" in feature ? feature.arrow1 : null,
      "arrow2" in feature ? feature.arrow2 : null,
    ].filter((item): item is Arrow => item !== null);
    if (arrows.length > 0) {
      properties.push({
        key: "arrows",
        label: "矢印",
        value: arrows
          .map((item) => `${ARROW_NAMES[item.code] ?? item.code}（${item.direction === 2 ? "内" : "外"}向き）`)
          .join("・"),
      });
    }
    this.add(feature, ctx, feature.layer, primitives, properties);
  }

  private leader(feature: Extract<SxfFeature, { kind: "label" | "balloon" }>, ctx: Context): void {
    const style: [number, number, number] = [feature.color, feature.lineType, feature.lineWidth];
    const color = this.styles.color(feature.color);
    const points = this.localPoints(ctx, feature.xs, feature.ys);
    const primitives: Primitive[] = [this.stroke(ctx, points, false, ...style)];
    if (points.length >= 4 && feature.arrowCode > 0) {
      const [tx, ty] = apply(ctx.m, points[0] as number, points[1] as number);
      const [nx, ny] = apply(ctx.m, points[2] as number, points[3] as number);
      const length = Math.hypot(tx - nx, ty - ny);
      if (length > 0) {
        primitives.push(
          ...arrowPrimitives(
            feature.arrowCode,
            tx,
            ty,
            (tx - nx) / length,
            (ty - ny) / length,
            feature.arrowScale,
            color,
            0,
            BACKGROUND_COLOR,
          ),
        );
      }
    }
    if (feature.kind === "balloon") {
      const [cx, cy] = this.local(ctx, feature.cx, feature.cy);
      const circle = ellipseArcPoints(
        cx,
        cy,
        feature.radius,
        feature.radius,
        0,
        0,
        360,
        true,
        arcSegments(feature.radius, Math.PI * 2, this.localTolerance(ctx)),
      ).slice(0, -2);
      primitives.push(this.stroke(ctx, circle, true, ...style));
    }
    const properties = this.styleProperties(...style);
    if (feature.text) {
      const text = this.text(ctx, feature.text, feature.color);
      if (text) primitives.push(text);
      properties.unshift({ key: "text", label: "文字列", value: feature.text.text });
    }
    this.add(feature, ctx, feature.layer, primitives, properties);
  }

  /** 複合曲線を閉じた点列にする（元の座標、表示の向き） */
  private ring(ctx: Context, id: number): number[] | null {
    const curve = this.doc.compositeCurves[id - 1];
    if (!curve) return null;
    const tol = this.localTolerance(ctx);
    const ring: number[] = [];
    for (const member of curve.members) {
      let points: number[];
      switch (member.kind) {
        case "polyline":
          points = this.localPoints(ctx, member.xs, member.ys);
          break;
        case "line":
          points = [...this.local(ctx, member.x1, member.y1), ...this.local(ctx, member.x2, member.y2)];
          break;
        case "arc":
        case "ellipse_arc": {
          const [cx, cy] = this.local(ctx, member.cx, member.cy);
          const rx = member.kind === "arc" ? member.r : member.rx;
          const ry = member.kind === "arc" ? member.r : member.ry;
          const rotation = member.kind === "ellipse_arc" ? member.rotation : 0;
          const sweep = sweepRadians(member.start, member.end, member.ccw);
          points = ellipseArcPoints(
            cx,
            cy,
            rx,
            ry,
            rotation,
            member.start,
            member.end,
            member.ccw,
            arcSegments(Math.max(rx, ry), sweep, tol),
          );
          break;
        }
        case "spline":
          points = bezierPoints(this.localPoints(ctx, member.xs, member.ys), 24);
          break;
        case "clothoid":
          points = clothoidPoints(
            ...this.local(ctx, member.x, member.y),
            member.parameter,
            member.ccw,
            member.angle,
            member.startLength,
            member.endLength,
            64,
          );
          break;
        default:
          continue;
      }
      appendChained(ring, points);
    }
    return ring.length >= 6 ? ring : null;
  }

  private hatch(
    feature: Extract<
      SxfFeature,
      {
        kind:
          | "fill_area_style_colour"
          | "fill_area_style_hatching"
          | "fill_area_style_tiles"
          | "externally_defined_hatch";
      }
    >,
    ctx: Context,
  ): void {
    const outer = this.ring(ctx, feature.outId);
    if (!outer) {
      this.notices.add("warning", "hatch-boundary", "外形の複合曲線がないハッチングを省きました");
      return;
    }
    const localRings = [outer, ...feature.inIds.map((id) => this.ring(ctx, id)).filter((ring) => ring !== null)];
    const rings = localRings.map((ring) => transformPoints(ctx.m, ring));
    const primitives: Primitive[] = [];
    const properties: Property[] = [
      { key: "holeCount", label: "中抜き", value: `${feature.inIds.length} 件`, raw: feature.inIds.length },
    ];
    if (feature.kind === "fill_area_style_colour") {
      primitives.push({ kind: "fill", color: this.styles.color(feature.color), rings });
      properties.unshift(this.colorProperty(feature.color));
    } else if (feature.kind === "fill_area_style_hatching") {
      const strokes = feature.patterns
        .map((pattern) => this.hatchLines(ctx, pattern, rings))
        .filter((stroke): stroke is StrokePrimitive => stroke !== null);
      primitives.push({ kind: "clipped", rings, strokes });
      properties.unshift({
        key: "hatchLines",
        label: "ハッチング線",
        value: feature.patterns
          .map(
            (pattern) =>
              `間隔 ${format(pattern.spacing)}・${format(pattern.angle)}°・${this.styles.colorLabel(pattern.color)}`,
          )
          .join(" / "),
      });
    } else if (feature.kind === "fill_area_style_tiles") {
      this.notices.add("info", "hatch-pattern", "パターンハッチングの模様は描かず、外形だけを表示しました");
      properties.unshift({ key: "pattern", label: "シンボル", value: feature.name });
    } else if (feature.name.toLowerCase() === "area_control") {
      this.notices.add("info", "area-control", "既定義ハッチング（Area_control）は空白の領域のため描いていません");
      properties.unshift({ key: "pattern", label: "名前", value: feature.name });
    } else {
      this.notices.add("info", "hatch-external", "外部定義ハッチングの模様は描かず、外形だけを表示しました");
      properties.unshift({ key: "pattern", label: "名前", value: feature.name });
    }
    // 外形・中抜きの複合曲線のうち、表示する指定のものを線で描く
    [feature.outId, ...feature.inIds].forEach((id, index) => {
      const curve = this.doc.compositeCurves[id - 1];
      const ring = localRings[index];
      if (!curve?.feature.visible || !ring) return;
      const { color, lineType, lineWidth } = curve.feature;
      primitives.push(this.stroke(ctx, ring, true, color, lineType, lineWidth));
    });
    if (primitives.length === 0) {
      // 描く形がない領域（Area_control など）も、選択できるように外形を透明な線で持つ
      primitives.push({
        kind: "stroke",
        points: rings[0] as Float64Array,
        closed: true,
        color: "transparent",
        width: 0,
        dashes: [],
      });
    }
    this.add(feature, ctx, feature.layer, primitives, properties);
  }

  /**
   * ハッチング線を、領域の外接矩形を覆う平行線として作る。描画時に領域で切り取る。
   * 間隔と角度は部分図・部品の配置の影響を受ける（SXF 実装規約 1-7）。
   */
  private hatchLines(
    ctx: Context,
    pattern: {
      color: number;
      lineType: number;
      lineWidth: number;
      x: number;
      y: number;
      spacing: number;
      angle: number;
    },
    rings: Float64Array[],
  ): StrokePrimitive | null {
    if (!(pattern.spacing > 0)) return null;
    const box = emptyBox();
    for (const ring of rings) extendBoxWithPoints(box, ring);
    const theta = (pattern.angle * Math.PI) / 180;
    const [px, py] = this.point(ctx, pattern.x, pattern.y);
    const [ux, uy] = applyVector(ctx.m, Math.cos(theta), Math.sin(theta));
    const [sx, sy] = applyVector(ctx.m, -Math.sin(theta) * pattern.spacing, Math.cos(theta) * pattern.spacing);
    const cross = (ax: number, ay: number, bx: number, by: number) => ax * by - ay * bx;
    const denominator = cross(ux, uy, sx, sy);
    if (Math.abs(denominator) < 1e-15) return null;
    const corners: [number, number][] = [
      [box.minX, box.minY],
      [box.maxX, box.minY],
      [box.maxX, box.maxY],
      [box.minX, box.maxY],
    ];
    const ks = corners.map(([cx, cy]) => cross(ux, uy, cx - px, cy - py) / denominator);
    const kMin = Math.ceil(Math.min(...ks));
    const kMax = Math.floor(Math.max(...ks));
    if (kMax - kMin + 1 > MAX_HATCH_LINES) {
      this.notices.add("warning", "hatch-dense", "間隔が細かすぎるハッチング線を省きました");
      return null;
    }
    const length = Math.hypot(ux, uy);
    const dx = ux / length;
    const dy = uy / length;
    const span = Math.hypot(box.maxX - box.minX, box.maxY - box.minY);
    const points: number[] = [];
    for (let k = kMin; k <= kMax; k++) {
      const ox = px + k * sx;
      const oy = py + k * sy;
      // 外接矩形の中心へ最も近い線上の点から、対角線の長さだけ両側へ伸ばす
      const cx = (box.minX + box.maxX) / 2;
      const cy = (box.minY + box.maxY) / 2;
      const t = (cx - ox) * dx + (cy - oy) * dy;
      const mx = ox + t * dx;
      const my = oy + t * dy;
      points.push(mx - dx * span, my - dy * span, mx + dx * span, my + dy * span);
    }
    return {
      kind: "stroke",
      points: Float64Array.from(points),
      closed: false,
      segments: true,
      color: this.styles.color(pattern.color),
      width: this.styles.width(pattern.lineWidth),
      dashes: this.styles.dashes(pattern.lineType),
    };
  }
}

function scaleLabel(scale: number): string {
  if (!(scale > 0)) return "-";
  const inverse = 1 / scale;
  return Math.abs(inverse - Math.round(inverse)) < 1e-6 ? `1/${Math.round(inverse)}` : format(scale, 6);
}

/** P21 の接頭語（$$SXF_FM_ など）と属性付加機構の記述を除いた複合図形名 */
function displayName(name: string): string {
  return name.replace(/^\$\$SXF_(FM|FG|G|P)_/, "") || name;
}

/** 曲線の点列を、向きをそろえてつなぐ */
function appendChained(ring: number[], points: number[]): void {
  if (points.length < 4) return;
  if (ring.length === 0) {
    ring.push(...points);
    return;
  }
  const lx = ring[ring.length - 2] as number;
  const ly = ring[ring.length - 1] as number;
  const n = points.length;
  const startDistance = Math.hypot((points[0] as number) - lx, (points[1] as number) - ly);
  const endDistance = Math.hypot((points[n - 2] as number) - lx, (points[n - 1] as number) - ly);
  let ordered = points;
  if (endDistance < startDistance) {
    ordered = [];
    for (let i = n - 2; i >= 0; i -= 2) ordered.push(points[i] as number, points[i + 1] as number);
  }
  ring.push(...ordered.slice(2));
}

function extendPrimitiveBox(box: Box, primitive: Primitive): void {
  switch (primitive.kind) {
    case "stroke":
      extendBoxWithPoints(box, primitive.points);
      return;
    case "fill":
      for (const ring of primitive.rings) extendBoxWithPoints(box, ring);
      return;
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
