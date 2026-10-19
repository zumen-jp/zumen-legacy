import { NoticeList } from "@zumenjs/core";
import type { SxfDocument, SxfDrawing } from "./document.js";
import type { Annotation, Arrow, ExtensionLine, HatchPattern, Styled, SxfFeature } from "./features.js";
import { type P21Entity, type P21File, type P21Value, readP21 } from "./p21-reader.js";
import {
  ARROW_NAMES,
  MARKER_NAMES,
  predefinedColorCode,
  predefinedLineTypeCode,
  predefinedWidthCode,
} from "./tables.js";

/**
 * SXF の P21（STEP AP202 のサブセット）を、SFC と同じフィーチャに変換する。
 * 対応は SXF Ver.3.1 仕様書・同解説 STEP/AP202 サブセット編の 2 章（テンプレート）と 3 章（P21 の見本）に従う。
 */

type FeatureOf<K extends SxfFeature["kind"]> = Extract<SxfFeature, { kind: K }>;

const DEG = 180 / Math.PI;
const ARROW_CODES = new Map(Object.entries(ARROW_NAMES).map(([code, name]) => [name, Number(code)]));
/** P21 の点マーカの名前（ISO の pre_defined_point_marker_symbol）と SXF のコード */
const MARKER_CODES = new Map<string, number>([
  ...Object.entries(MARKER_NAMES).map(([code, name]) => [name.toLowerCase(), Number(code)] as [string, number]),
  ["x", 7],
]);
/** 子図の定義の入れ子の上限（描くのは 32 段まで）。深すぎる定義は読まずに件数を知らせる */
const MAX_DEFINITION_DEPTH = 64;
const SUBFIGURE_PREFIXES: [string, number][] = [
  ["$$SXF_FM_", 1],
  ["$$SXF_FG_", 2],
  ["$$SXF_G_", 3],
  ["$$SXF_P_", 4],
];
/** 文字の配置基点（$$SXF_<行> <列>）を SFC の b_pnt にする */
const ANCHOR_ROWS: Record<string, number> = { baseline: 0, middleline: 1, topline: 2 };
const ANCHOR_COLUMNS: Record<string, number> = { left: 1, centre: 2, center: 2, right: 3 };

interface CurveStyle {
  color: number;
  lineType: number;
  lineWidth: number;
}

class Model {
  readonly entities: Map<number, P21Entity>;
  private readonly colors = new Map<number, number>();
  private readonly lineTypes = new Map<number, number>();
  private readonly widths = new Map<string, number>();
  private readonly fonts = new Map<number, number>();
  /** 要素（表記の occurrence）からレイヤコードへ */
  readonly layerOf = new Map<number, number>();
  readonly invisible = new Set<number>();
  readonly doc: SxfDocument;
  private readonly compositeCodes = new Map<number, number>();
  /** 複合曲線を描く ANNOTATION_CURVE_OCCURRENCE */
  private readonly curveOccurrenceOf = new Map<number, number>();

  constructor(
    file: P21File,
    private readonly notices: NoticeList,
  ) {
    this.entities = file.entities;
    this.doc = {
      sheet: null,
      title: null,
      layers: [],
      userColors: [],
      userLineTypes: [],
      userWidths: [],
      fonts: [],
      compositeCurves: [],
      subFigures: new Map(),
      sheetMembers: [],
      counts: new Map(),
    };
    this.prepareTables();
  }

  get(id: number | null | undefined): P21Entity | undefined {
    return id === null || id === undefined ? undefined : this.entities.get(id);
  }

  args(entity: P21Entity | undefined, type: string): P21Value[] | undefined {
    return entity?.parts.get(type);
  }

  has(entity: P21Entity | undefined, type: string): boolean {
    return entity?.parts.has(type) ?? false;
  }

  /** 単純インスタンスの型名 */
  typeOf(entity: P21Entity | undefined): string | undefined {
    if (entity?.parts.size !== 1) return undefined;
    return entity.parts.keys().next().value;
  }

  ref(value: P21Value | undefined): number | null {
    if (value && typeof value === "object" && "ref" in value) return value.ref;
    return null;
  }

  refs(value: P21Value | undefined): number[] {
    if (!Array.isArray(value)) return [];
    return value.map((item) => this.ref(item)).filter((item): item is number => item !== null);
  }

  num(value: P21Value | undefined): number {
    if (typeof value === "number") return value;
    if (Array.isArray(value)) return this.num(value[0]);
    if (value && typeof value === "object" && "typed" in value) return this.num(value.value);
    return Number.NaN;
  }

  str(value: P21Value | undefined): string {
    if (typeof value === "string") return value;
    if (value && typeof value === "object" && "typed" in value) return this.str(value.value);
    return "";
  }

  point(id: number | null): [number, number] {
    const values = this.args(this.get(id), "CARTESIAN_POINT")?.[1];
    return Array.isArray(values) ? [this.num(values[0]), this.num(values[1])] : [Number.NaN, Number.NaN];
  }

  direction(id: number | null): [number, number] {
    const values = this.args(this.get(id), "DIRECTION")?.[1];
    return Array.isArray(values) ? [this.num(values[0]), this.num(values[1])] : [1, 0];
  }

  /** AXIS2_PLACEMENT_2D の原点と X 軸の向き（度） */
  placement(id: number | null): { x: number; y: number; angle: number; dx: number; dy: number } {
    const args = this.args(this.get(id), "AXIS2_PLACEMENT_2D");
    const [x, y] = this.point(this.ref(args?.[1]));
    const directionId = this.ref(args?.[2]);
    const [dx, dy] = directionId === null ? [1, 0] : this.direction(directionId);
    const length = Math.hypot(dx, dy) || 1;
    return { x, y, angle: Math.atan2(dy, dx) * DEG, dx: dx / length, dy: dy / length };
  }

  private sortedIds(type: string): number[] {
    return [...this.entities.values()]
      .filter((entity) => entity.parts.has(type))
      .map((entity) => entity.id)
      .sort((a, b) => a - b);
  }

  /** 色・線種・線幅・フォント・レイヤのコードを、インスタンス番号の順に割り当てる */
  private prepareTables(): void {
    for (const id of this.sortedIds("COLOUR_RGB")) {
      const args = this.args(this.get(id), "COLOUR_RGB") ?? [];
      const name = this.str(args[0]).trim();
      const predefined = name.startsWith("$$SXF_") ? predefinedColorCode(name.slice(6)) : 0;
      if (predefined > 0) {
        this.colors.set(id, predefined);
        continue;
      }
      const [r, g, b] = [1, 2, 3].map((index) => Math.round(this.num(args[index]) * 255));
      this.doc.userColors.push({ id, kind: "user_defined_colour", r: r ?? 0, g: g ?? 0, b: b ?? 0 });
      this.colors.set(id, 16 + this.doc.userColors.length);
    }
    for (const id of this.sortedIds("DRAUGHTING_PRE_DEFINED_COLOUR")) {
      const name = this.str(this.args(this.get(id), "DRAUGHTING_PRE_DEFINED_COLOUR")?.[0]);
      this.colors.set(id, predefinedColorCode(name));
    }
    for (const id of this.sortedIds("DRAUGHTING_PRE_DEFINED_CURVE_FONT")) {
      const name = this.str(this.args(this.get(id), "DRAUGHTING_PRE_DEFINED_CURVE_FONT")?.[0]);
      this.lineTypes.set(id, predefinedLineTypeCode(name) || 1);
    }
    for (const id of this.sortedIds("CURVE_STYLE_FONT")) {
      const args = this.args(this.get(id), "CURVE_STYLE_FONT") ?? [];
      const pitch: number[] = [];
      for (const patternId of this.refs(args[1])) {
        const pattern = this.args(this.get(patternId), "CURVE_STYLE_FONT_PATTERN") ?? [];
        pitch.push(this.num(pattern[0]), this.num(pattern[1]));
      }
      this.doc.userLineTypes.push({ id, kind: "user_defined_font", name: this.str(args[0]), pitch });
      this.lineTypes.set(id, 16 + this.doc.userLineTypes.length);
    }
    for (const id of this.sortedIds("EXTERNALLY_DEFINED_TEXT_FONT")) {
      const name = this.str(this.args(this.get(id), "EXTERNALLY_DEFINED_TEXT_FONT")?.[0]);
      this.doc.fonts.push({ id, kind: "text_font", name });
      this.fonts.set(id, this.doc.fonts.length);
    }
    for (const id of this.sortedIds("PRESENTATION_LAYER_ASSIGNMENT")) {
      const args = this.args(this.get(id), "PRESENTATION_LAYER_ASSIGNMENT") ?? [];
      const name = this.str(args[0]);
      // 子図を入れるための仮のレイヤ（$$SXF_dummy_layer_for_subfigure）は、レイヤとして扱わない
      if (name.startsWith("$$SXF_dummy")) continue;
      this.doc.layers.push({ id, kind: "layer", name, visible: true });
      const code = this.doc.layers.length;
      for (const item of this.refs(args[2])) this.layerOf.set(item, code);
    }
    for (const id of this.sortedIds("INVISIBILITY")) {
      for (const item of this.refs(this.args(this.get(id), "INVISIBILITY")?.[0])) {
        const layer = this.doc.layers.find((candidate) => candidate.id === item);
        if (layer) layer.visible = false;
        else this.invisible.add(item);
      }
    }
    for (const entity of this.entities.values()) {
      if (!entity.parts.has("ANNOTATION_CURVE_OCCURRENCE")) continue;
      const item = this.ref(this.args(entity, "STYLED_ITEM")?.[1]);
      if (item !== null && this.has(this.get(item), "COMPOSITE_CURVE")) this.curveOccurrenceOf.set(item, entity.id);
    }
  }

  color(id: number | null): number {
    if (id === null) return 0;
    const code = this.colors.get(id);
    if (code !== undefined) return code;
    this.notices.add("warning", "unknown-color", "定義のない色を白で表示しました");
    return 8;
  }

  lineType(id: number | null): number {
    return id === null ? 1 : (this.lineTypes.get(id) ?? 1);
  }

  /** 線幅（LENGTH_MEASURE_WITH_UNIT か数値）を、既定義なら 1〜9、それ以外は 11 からのコードにする */
  lineWidth(value: P21Value | undefined): number {
    const id = this.ref(value);
    const width = id === null ? this.num(value) : this.num(this.args(this.get(id), "LENGTH_MEASURE_WITH_UNIT")?.[0]);
    if (!Number.isFinite(width)) return 1;
    const predefined = predefinedWidthCode(width);
    if (predefined > 0) return predefined;
    const key = width.toFixed(6);
    let code = this.widths.get(key);
    if (code === undefined) {
      this.doc.userWidths.push({ id: id ?? 0, kind: "width", width });
      code = 10 + this.doc.userWidths.length;
      this.widths.set(key, code);
    }
    return code;
  }

  font(id: number | null): number {
    return id === null ? 0 : (this.fonts.get(id) ?? 0);
  }

  /** PRESENTATION_STYLE_ASSIGNMENT の中の CURVE_STYLE */
  curveStyle(styleAssignment: number | null): CurveStyle {
    for (const styleId of this.refs(this.args(this.get(styleAssignment), "PRESENTATION_STYLE_ASSIGNMENT")?.[0])) {
      const args = this.args(this.get(styleId), "CURVE_STYLE");
      if (!args) continue;
      return {
        lineType: this.lineType(this.ref(args[1])),
        lineWidth: this.lineWidth(args[2]),
        color: this.color(this.ref(args[3])),
      };
    }
    return { color: 8, lineType: 1, lineWidth: 1 };
  }

  /** STYLED_ITEM の (スタイル, 対象)。複合インスタンスでは name を REPRESENTATION_ITEM が持つ */
  styled(entity: P21Entity | undefined): { style: number | null; item: number | null } {
    const args = this.args(entity, "STYLED_ITEM") ?? [];
    const offset = args.length >= 3 ? 1 : 0;
    return { style: this.refs(args[offset])[0] ?? null, item: this.ref(args[offset + 1]) };
  }

  compositeCode(curveId: number): number {
    const existing = this.compositeCodes.get(curveId);
    if (existing !== undefined) return existing;
    const occurrence = this.get(this.curveOccurrenceOf.get(curveId));
    const style = occurrence ? this.curveStyle(this.styled(occurrence).style) : { color: 8, lineType: 1, lineWidth: 1 };
    const visible = occurrence !== undefined && !this.invisible.has(occurrence.id);
    const members: SxfFeature[] = [];
    const segments = this.refs(this.args(this.get(curveId), "COMPOSITE_CURVE")?.[1]);
    for (const segmentId of segments) {
      const segment = this.args(this.get(segmentId), "COMPOSITE_CURVE_SEGMENT");
      const parent = this.ref(segment?.[2]);
      const feature = this.curve(parent, { id: parent ?? 0, layer: 0, ...style });
      if (feature) members.push(feature);
    }
    this.doc.compositeCurves.push({
      feature: { id: curveId, kind: "composite_curve_org", ...style, visible },
      members,
    });
    const code = this.doc.compositeCurves.length;
    this.compositeCodes.set(curveId, code);
    return code;
  }

  /** 曲線の幾何を、SFC のフィーチャ（線分・円弧・折線など）にする */
  curve(id: number | null, style: Styled): SxfFeature | null {
    const entity = this.get(id);
    if (!entity) return null;
    const trimmed = this.args(entity, "TRIMMED_CURVE");
    if (trimmed) return this.trimmed(trimmed, style);
    const polyline = this.args(entity, "POLYLINE");
    if (polyline) {
      const points = this.refs(polyline[1]).map((pointId) => this.point(pointId));
      return { ...style, kind: "polyline", xs: points.map((p) => p[0]), ys: points.map((p) => p[1]) };
    }
    const circle = this.args(entity, "CIRCLE");
    if (circle) {
      const p = this.placement(this.ref(circle[1]));
      return { ...style, kind: "circle", cx: p.x, cy: p.y, r: this.num(circle[2]) };
    }
    const ellipse = this.args(entity, "ELLIPSE");
    if (ellipse) {
      const p = this.placement(this.ref(ellipse[1]));
      return {
        ...style,
        kind: "ellipse",
        cx: p.x,
        cy: p.y,
        rx: this.num(ellipse[2]),
        ry: this.num(ellipse[3]),
        rotation: p.angle,
      };
    }
    const bezier = this.args(entity, "BEZIER_CURVE");
    if (bezier) {
      const points = this.refs(bezier[2]).map((pointId) => this.point(pointId));
      const closed = (bezier[4] as { enum?: string } | null)?.enum === "T";
      return { ...style, kind: "spline", closed, xs: points.map((p) => p[0]), ys: points.map((p) => p[1]) };
    }
    return null;
  }

  /** トリム値。PARAMETER_VALUE（ラジアン・曲線長）か CARTESIAN_POINT */
  private trim(value: P21Value | undefined): { parameter: number | null; point: [number, number] | null } {
    const items = Array.isArray(value) ? value : [value];
    let parameter: number | null = null;
    let point: [number, number] | null = null;
    for (const item of items) {
      if (item && typeof item === "object" && "typed" in item && item.typed === "PARAMETER_VALUE") {
        parameter = this.num(item.value);
      } else {
        const id = this.ref(item);
        if (id !== null && this.has(this.get(id), "CARTESIAN_POINT")) point = this.point(id);
      }
    }
    return { parameter, point };
  }

  private trimmed(args: P21Value[], style: Styled): SxfFeature | null {
    const basis = this.get(this.ref(args[1]));
    const t1 = this.trim(args[2]);
    const t2 = this.trim(args[3]);
    const sense = (args[4] as { enum?: string } | null)?.enum !== "F";
    const line = this.args(basis, "LINE");
    if (line) {
      const origin = this.point(this.ref(line[1]));
      const vector = this.args(this.get(this.ref(line[2])), "VECTOR");
      const [dx, dy] = this.direction(this.ref(vector?.[1]));
      const length = Math.hypot(dx, dy) || 1;
      const magnitude = this.num(vector?.[2]) || 1;
      const at = (trim: typeof t1): [number, number] =>
        trim.point ??
        (trim.parameter !== null
          ? [
              origin[0] + (dx / length) * magnitude * trim.parameter,
              origin[1] + (dy / length) * magnitude * trim.parameter,
            ]
          : origin);
      const [x1, y1] = at(t1);
      const [x2, y2] = at(t2);
      return { ...style, kind: "line", x1, y1, x2, y2 };
    }
    const circle = this.args(basis, "CIRCLE");
    const ellipse = this.args(basis, "ELLIPSE");
    if (circle || ellipse) {
      const p = this.placement(this.ref((circle ?? ellipse)?.[1]));
      const angleOf = (trim: typeof t1): number => {
        if (trim.parameter !== null) return trim.parameter * DEG;
        if (trim.point) return Math.atan2(trim.point[1] - p.y, trim.point[0] - p.x) * DEG - p.angle;
        return 0;
      };
      const start = angleOf(t1);
      const end = angleOf(t2);
      if (circle) {
        return {
          ...style,
          kind: "arc",
          cx: p.x,
          cy: p.y,
          r: this.num(circle[2]),
          ccw: sense,
          start: start + p.angle,
          end: end + p.angle,
        };
      }
      return {
        ...style,
        kind: "ellipse_arc",
        cx: p.x,
        cy: p.y,
        rx: this.num(ellipse?.[2]),
        ry: this.num(ellipse?.[3]),
        ccw: sense,
        rotation: p.angle,
        start,
        end,
      };
    }
    const clothoid = this.args(basis, "CLOTHOID");
    if (clothoid) {
      const p = this.placement(this.ref(clothoid[1]));
      return {
        ...style,
        kind: "clothoid",
        x: p.x,
        y: p.y,
        parameter: this.num(clothoid[2]),
        // 向きフラグは CLOTHOID の name に書かれる（0: 反時計回り、1: 時計回り）
        ccw: this.str(clothoid[0]).trim() !== "1",
        angle: p.angle,
        startLength: t1.parameter ?? 0,
        endLength: t2.parameter ?? 0,
      };
    }
    return null;
  }
}

/** 文字の occurrence を SFC の文字の値にする。配置点はベースラインからの差を戻して、SXF の配置基点にする */
function annotationOf(model: Model, occurrence: P21Entity | undefined): (Annotation & { color: number }) | null {
  if (!occurrence) return null;
  const { style, item } = model.styled(occurrence);
  const literal = model.args(model.get(item), "TEXT_LITERAL_WITH_EXTENT");
  if (!literal) return null;
  const [rowName, columnName] = model
    .str(literal[0])
    .replace(/^\$\$SXF_/, "")
    .trim()
    .split(/\s+/);
  const row = ANCHOR_ROWS[rowName ?? ""] ?? 0;
  const column = ANCHOR_COLUMNS[columnName ?? ""] ?? 1;
  const placement = model.placement(model.ref(literal[2]));
  const extent = model.args(model.get(model.ref(literal[6])), "PLANAR_EXTENT");
  const width = model.num(extent?.[1]);
  const height = model.num(extent?.[2]);
  let color = 8;
  let slant = 0;
  let spacing = 0;
  for (const styleId of model.refs(model.args(model.get(style), "PRESENTATION_STYLE_ASSIGNMENT")?.[0])) {
    const styleEntity = model.get(styleId);
    const textStyle = model.args(styleEntity, "TEXT_STYLE");
    const definedFont = model.args(model.get(model.ref(textStyle?.[1])), "TEXT_STYLE_FOR_DEFINED_FONT");
    if (definedFont) color = model.color(model.ref(definedFont[0]));
    const box = model.args(styleEntity, "TEXT_STYLE_WITH_BOX_CHARACTERISTICS")?.[0];
    if (Array.isArray(box)) {
      for (const characteristic of box) {
        if (characteristic && typeof characteristic === "object" && "typed" in characteristic) {
          if (characteristic.typed === "BOX_SLANT_ANGLE") slant = model.num(characteristic.value) * DEG;
        }
      }
    }
    const spacingValue = model.args(styleEntity, "TEXT_STYLE_WITH_SPACING")?.[0];
    if (spacingValue !== undefined) spacing = model.num(spacingValue);
  }
  // 出力ライブラリは配置基点を、下・中・上の行ごとに高さの 1/2・-1/2・-1 だけ文字の上方向へずらして書く
  const offset = row === 0 ? height / 2 : row === 1 ? -height / 2 : -height;
  const x = placement.x + placement.dy * offset;
  const y = placement.y - placement.dx * offset;
  const path = (literal[4] as { enum?: string } | null)?.enum;
  return {
    font: model.font(model.ref(literal[5])),
    text: model.str(literal[1]),
    x,
    y,
    height,
    width,
    spacing,
    angle: placement.angle,
    slant,
    anchor: row * 3 + column,
    direction: path === "DOWN" ? 2 : 1,
    color,
  };
}

/** 寸法線・引出し線の端末記号（矢印） */
function terminatorOf(
  model: Model,
  occurrence: P21Entity,
): { code: number; x: number; y: number; dx: number; dy: number; scale: number } | null {
  const symbol = model.args(model.get(model.styled(occurrence).item), "DEFINED_SYMBOL");
  if (!symbol) return null;
  const name = model.str(model.args(model.get(model.ref(symbol[1])), "PRE_DEFINED_TERMINATOR_SYMBOL")?.[0]);
  const target = model.args(model.get(model.ref(symbol[2])), "SYMBOL_TARGET");
  const placement = model.placement(model.ref(target?.[1]));
  return {
    code: ARROW_CODES.get(name.trim().toLowerCase()) ?? 9,
    x: placement.x,
    y: placement.y,
    dx: placement.dx,
    dy: placement.dy,
    scale: model.num(target?.[2]) || 1,
  };
}

class Converter {
  constructor(
    private readonly model: Model,
    private readonly notices: NoticeList,
  ) {}

  private layer(id: number): number {
    return this.model.layerOf.get(id) ?? 0;
  }

  /** 用紙・子図の items の 1 件を、フィーチャにする（対象外は空） */
  item(id: number): SxfFeature[] {
    const model = this.model;
    const entity = model.get(id);
    if (!entity) return [];
    if (model.has(entity, "DRAUGHTING_CALLOUT")) return this.callout(entity);
    if (model.has(entity, "ANNOTATION_SUBFIGURE_OCCURRENCE")) return this.subfigure(entity);
    if (model.has(entity, "ANNOTATION_FILL_AREA_OCCURRENCE")) return this.fill(entity);
    if (model.has(entity, "ANNOTATION_TEXT_OCCURRENCE")) {
      const text = annotationOf(model, entity);
      return text ? [{ id, kind: "text_string", layer: this.layer(id), ...text }] : [];
    }
    if (model.has(entity, "ANNOTATION_SYMBOL_OCCURRENCE")) return this.symbol(entity);
    if (model.has(entity, "ANNOTATION_CURVE_OCCURRENCE")) {
      const { style, item } = model.styled(entity);
      if (item !== null && model.has(model.get(item), "COMPOSITE_CURVE")) return [];
      const curve = model.curve(item, { id, layer: this.layer(id), ...model.curveStyle(style) });
      if (!curve) this.notices.add("warning", "p21-curve", "対応していない曲線を省きました");
      return curve ? [curve] : [];
    }
    const styledTarget = model.styled(entity).item;
    if (styledTarget !== null && model.has(model.get(styledTarget), "DRAUGHTING_CALLOUT")) {
      return this.callout(model.get(styledTarget) as P21Entity);
    }
    return [];
  }

  private symbol(entity: P21Entity): SxfFeature[] {
    const model = this.model;
    const { style, item } = model.styled(entity);
    const symbol = model.args(model.get(item), "DEFINED_SYMBOL");
    if (!symbol) return [];
    const target = model.args(model.get(model.ref(symbol[2])), "SYMBOL_TARGET");
    const placement = model.placement(model.ref(target?.[1]));
    let color: number | null = null;
    for (const styleId of model.refs(model.args(model.get(style), "PRESENTATION_STYLE_ASSIGNMENT")?.[0])) {
      const symbolStyle = model.args(model.get(styleId), "SYMBOL_STYLE");
      const symbolColour = model.args(model.get(model.ref(symbolStyle?.[1])), "SYMBOL_COLOUR");
      if (symbolColour) color = model.color(model.ref(symbolColour[0]));
    }
    const definition = model.get(model.ref(symbol[1]));
    const marker = model.args(definition, "PRE_DEFINED_POINT_MARKER_SYMBOL");
    if (marker) {
      return [
        {
          id: entity.id,
          kind: "point_marker",
          layer: this.layer(entity.id),
          color: color ?? 8,
          x: placement.x,
          y: placement.y,
          code: MARKER_CODES.get(model.str(marker[0]).trim().toLowerCase()) ?? 3,
          angle: placement.angle,
          scale: model.num(target?.[2]) || 1,
        },
      ];
    }
    const external = model.args(definition, "EXTERNALLY_DEFINED_SYMBOL");
    if (external) {
      return [
        {
          id: entity.id,
          kind: "externally_defined_symbol",
          layer: this.layer(entity.id),
          color,
          name: model.str(external[0]),
          x: placement.x,
          y: placement.y,
          angle: placement.angle,
          scale: model.num(target?.[2]) || 1,
        },
      ];
    }
    return [];
  }

  private subfigure(entity: P21Entity): SxfFeature[] {
    const model = this.model;
    const mapped = model.args(model.get(model.styled(entity).item), "MAPPED_ITEM");
    const map = model.args(model.get(model.ref(mapped?.[0])), "SYMBOL_REPRESENTATION_MAP");
    const representation = model.ref(map?.[1]);
    const target = model.args(model.get(model.ref(mapped?.[1])), "SYMBOL_TARGET");
    if (representation === null || !target) return [];
    const definition = this.definition(representation);
    if (!definition) return [];
    const placement = model.placement(model.ref(target[1]));
    return [
      {
        id: entity.id,
        kind: "sfig_locate",
        layer: this.layer(entity.id),
        name: definition,
        x: placement.x,
        y: placement.y,
        angle: placement.angle,
        ratioX: model.num(target[2]),
        ratioY: model.num(target[3]),
      },
    ];
  }

  private readonly definitions = new Map<number, string>();
  private depth = 0;

  /** DRAUGHTING_SUBFIGURE_REPRESENTATION を複合図形定義にして、名前を返す */
  private definition(id: number): string | null {
    const known = this.definitions.get(id);
    if (known !== undefined) return known;
    const args = this.model.args(this.model.get(id), "DRAUGHTING_SUBFIGURE_REPRESENTATION");
    if (!args) return null;
    if (this.depth >= MAX_DEFINITION_DEPTH) {
      this.notices.add("warning", "sfig-depth", "入れ子が深すぎる子図の定義を省きました");
      return null;
    }
    const fullName = this.model.str(args[0]);
    const prefix = SUBFIGURE_PREFIXES.find(([candidate]) => fullName.startsWith(candidate));
    const name = prefix ? fullName.slice(prefix[0].length) : fullName;
    this.definitions.set(id, name);
    const members: SxfFeature[] = [];
    this.depth++;
    for (const item of this.model.refs(args[1])) members.push(...this.item(item));
    this.depth--;
    this.model.doc.subFigures.set(name, {
      feature: { id, kind: "sfig_org", name, flag: prefix?.[1] ?? 3 },
      members,
    });
    return name;
  }

  private fill(entity: P21Entity): SxfFeature[] {
    const model = this.model;
    const { style, item } = model.styled(entity);
    const area = model.args(model.get(item), "ANNOTATION_FILL_AREA");
    const boundaries = model.refs(area?.[1]).filter((id) => model.has(model.get(id), "COMPOSITE_CURVE"));
    if (boundaries.length === 0) {
      this.notices.add("warning", "hatch-boundary", "外形の複合曲線がないハッチングを省きました");
      return [];
    }
    const [outer, ...holes] = boundaries.map((id) => model.compositeCode(id));
    const region = { outId: outer as number, inIds: holes };
    const layer = this.layer(entity.id);
    const patterns: HatchPattern[] = [];
    for (const styleId of model.refs(model.args(model.get(style), "PRESENTATION_STYLE_ASSIGNMENT")?.[0])) {
      for (const fillStyleId of model.refs(model.args(model.get(styleId), "FILL_AREA_STYLE")?.[1])) {
        const fillStyle = model.get(fillStyleId);
        const colour = model.args(fillStyle, "FILL_AREA_STYLE_COLOUR");
        if (colour) {
          return [
            {
              id: entity.id,
              kind: "fill_area_style_colour",
              layer,
              color: model.color(model.ref(colour[1])),
              ...region,
            },
          ];
        }
        const hatching = model.args(fillStyle, "FILL_AREA_STYLE_HATCHING");
        if (hatching) {
          const curveStyle = model.args(model.get(model.ref(hatching[1])), "CURVE_STYLE");
          const repeat = model.args(model.get(model.ref(hatching[2])), "ONE_DIRECTION_REPEAT_FACTOR");
          const vector = model.args(model.get(model.ref(repeat?.[1])), "VECTOR");
          const [x, y] = model.point(model.ref(hatching[3]));
          patterns.push({
            color: model.color(model.ref(curveStyle?.[3])),
            lineType: model.lineType(model.ref(curveStyle?.[1])),
            lineWidth: model.lineWidth(curveStyle?.[2]),
            x,
            y,
            spacing: model.num(vector?.[2]),
            angle: model.num(hatching[5]) * DEG,
          });
          continue;
        }
        const external = model.args(fillStyle, "EXTERNALLY_DEFINED_HATCH_STYLE");
        if (external) {
          return [{ id: entity.id, kind: "externally_defined_hatch", layer, name: model.str(external[0]), ...region }];
        }
        if (model.has(fillStyle, "FILL_AREA_STYLE_TILES")) {
          return [
            {
              id: entity.id,
              kind: "fill_area_style_tiles",
              layer,
              name: "",
              color: 8,
              x: 0,
              y: 0,
              vector1: 0,
              vector1Angle: 0,
              vector2: 0,
              vector2Angle: 0,
              scaleX: 1,
              scaleY: 1,
              angle: 0,
              ...region,
            },
          ];
        }
      }
    }
    if (patterns.length > 0) return [{ id: entity.id, kind: "fill_area_style_hatching", layer, patterns, ...region }];
    return [];
  }

  private callout(entity: P21Entity): SxfFeature[] {
    const model = this.model;
    if (model.has(entity, "STRUCTURED_DIMENSION_CALLOUT")) return [];
    const contents = model.refs(model.args(entity, "DRAUGHTING_CALLOUT")?.[0]).map((id) => model.get(id));
    const find = (type: string) => contents.find((item) => model.has(item, type));
    const text = annotationOf(
      model,
      contents.find((item) => model.has(item, "ANNOTATION_TEXT_OCCURRENCE")),
    );
    const annotation: Annotation | null = text ? { ...text } : null;
    if (model.has(entity, "LEADER_DIRECTED_CALLOUT")) return this.leader(entity, contents, annotation);
    const dimensionCurve = find("DIMENSION_CURVE");
    if (!dimensionCurve) return [];
    const { style, item } = model.styled(dimensionCurve);
    const curveStyle = model.curveStyle(style);
    const styled: Styled = { id: entity.id, layer: this.layer(dimensionCurve.id), ...curveStyle };
    const curve = model.curve(item, styled);
    // 寸法補助線は名前（$$SXF_prj_1・$$SXF_prj_2）の順。利用者のロケールで結果が変わらないよう、符号単位で比べる
    const projectionName = (item: P21Entity | undefined) => model.str(model.args(item, "REPRESENTATION_ITEM")?.[0]);
    const projections = contents
      .filter((item) => model.has(item, "PROJECTION_CURVE"))
      .sort((a, b) => {
        const nameA = projectionName(a);
        const nameB = projectionName(b);
        return nameA < nameB ? -1 : nameA > nameB ? 1 : 0;
      });
    const extension = (occurrence: P21Entity | undefined): ExtensionLine | null => {
      if (!occurrence) return null;
      const trimmed = model.args(model.get(model.styled(occurrence).item), "TRIMMED_CURVE");
      const line = model.args(model.get(model.ref(trimmed?.[1])), "LINE");
      const line2 = model.curve(model.styled(occurrence).item, styled);
      if (line2?.kind !== "line") return null;
      const [x0, y0] = model.point(model.ref(line?.[1]));
      return { x0, y0, x1: line2.x1, y1: line2.y1, x2: line2.x2, y2: line2.y2 };
    };
    const terminators = contents
      .filter((item): item is P21Entity => model.has(item, "DIMENSION_CURVE_TERMINATOR"))
      .map((item) => ({
        role: (model.args(item, "DIMENSION_CURVE_TERMINATOR")?.[0] as { enum?: string })?.enum,
        symbol: terminatorOf(model, item),
      }));
    const ends: [number, number, number, number] | null =
      curve?.kind === "line" ? [curve.x1, curve.y1, curve.x2, curve.y2] : null;
    /** P21 の矢印の向き（X 軸が矢印の指す向き）を、SFC の外向き・内向きにする */
    const arrow = (role: string): Arrow | null => {
      const symbol = terminators.find((item) => item.role === role)?.symbol;
      if (!symbol) return null;
      let outward: [number, number] = [symbol.dx, symbol.dy];
      if (ends) {
        const [x1, y1, x2, y2] = ends;
        const nearStart = Math.hypot(symbol.x - x1, symbol.y - y1) <= Math.hypot(symbol.x - x2, symbol.y - y2);
        outward = nearStart ? [x1 - x2, y1 - y2] : [x2 - x1, y2 - y1];
      } else if (curve?.kind === "arc") {
        const angle = Math.atan2(symbol.y - curve.cy, symbol.x - curve.cx);
        outward = role === "ORIGIN" ? [Math.sin(angle), -Math.cos(angle)] : [-Math.sin(angle), Math.cos(angle)];
      }
      const direction = outward[0] * symbol.dx + outward[1] * symbol.dy >= 0 ? 1 : 2;
      return { code: symbol.code, direction, x: symbol.x, y: symbol.y, scale: symbol.scale };
    };
    const base = { ...styled, text: annotation };
    if (model.has(entity, "LINEAR_DIMENSION") && ends) {
      return [
        {
          ...base,
          kind: "linear_dim",
          line: ends,
          ext1: extension(projections[0]),
          ext2: extension(projections[1]),
          arrow1: arrow("ORIGIN"),
          arrow2: arrow("TARGET"),
        },
      ];
    }
    if ((model.has(entity, "CURVE_DIMENSION") || model.has(entity, "ANGULAR_DIMENSION")) && curve?.kind === "arc") {
      // 寸法の円弧は、始角から終角へ反時計回り（SFC 編 1-2-24・1-2-25）
      return [
        {
          ...base,
          kind: model.has(entity, "CURVE_DIMENSION") ? "curve_dim" : "angular_dim",
          cx: curve.cx,
          cy: curve.cy,
          radius: curve.r,
          angle0: curve.start,
          angle1: curve.end,
          ext1: extension(projections[0]),
          ext2: extension(projections[1]),
          arrow1: arrow("ORIGIN"),
          arrow2: arrow("TARGET"),
        },
      ];
    }
    if (model.has(entity, "RADIUS_DIMENSION") && ends) {
      return [{ ...base, kind: "radius_dim", line: ends, arrow: arrow("ORIGIN") ?? arrow("TARGET") }];
    }
    if (model.has(entity, "DIAMETER_DIMENSION") && ends) {
      return [{ ...base, kind: "diameter_dim", line: ends, arrow1: arrow("ORIGIN"), arrow2: arrow("TARGET") }];
    }
    this.notices.add("warning", "p21-callout", "対応していない寸法の表記を省きました");
    return [];
  }

  private leader(entity: P21Entity, contents: (P21Entity | undefined)[], annotation: Annotation | null): SxfFeature[] {
    const model = this.model;
    const leaderCurve = contents.find((item) => model.has(item, "LEADER_CURVE"));
    if (!leaderCurve) return [];
    const { style, item } = model.styled(leaderCurve);
    const styled: Styled = { id: entity.id, layer: this.layer(leaderCurve.id), ...model.curveStyle(style) };
    const polyline = model.curve(item, styled);
    if (polyline?.kind !== "polyline") return [];
    const terminator = contents.find((candidate) => model.has(candidate, "LEADER_TERMINATOR"));
    const symbol = terminator ? terminatorOf(model, terminator) : null;
    const balloon = contents
      .filter(
        (candidate) => model.has(candidate, "ANNOTATION_CURVE_OCCURRENCE") && !model.has(candidate, "LEADER_CURVE"),
      )
      .map((candidate) => model.curve(model.styled(candidate).item, styled))
      .find((curve): curve is FeatureOf<"circle"> => curve?.kind === "circle");
    const common = {
      ...styled,
      xs: polyline.xs,
      ys: polyline.ys,
      arrowCode: symbol?.code ?? 0,
      arrowScale: symbol?.scale ?? 1,
      text: annotation,
    };
    if (balloon) return [{ ...common, kind: "balloon", cx: balloon.cx, cy: balloon.cy, radius: balloon.r }];
    return [{ ...common, kind: "label" }];
  }
}

const SHEET_NAMES: Record<string, number> = { A0: 0, A1: 1, A2: 2, A3: 3, A4: 4 };

function sheetOf(model: Model): { sheet: FeatureOf<"drawing_sheet"> | null; items: number[] } {
  const sheetEntity = [...model.entities.values()].find((entity) => entity.parts.has("DRAWING_SHEET_REVISION"));
  if (!sheetEntity) return { sheet: null, items: [] };
  const args = model.args(sheetEntity, "DRAWING_SHEET_REVISION") ?? [];
  const sizeName = model.str(args[0]);
  const items = model.refs(args[1]);
  let x = 0;
  let y = 0;
  for (const entity of model.entities.values()) {
    const size = model.args(entity, "PRESENTATION_SIZE");
    if (size && model.ref(size[0]) === sheetEntity.id) {
      const box = model.args(model.get(model.ref(size[1])), "PLANAR_BOX");
      x = model.num(box?.[1]);
      y = model.num(box?.[2]);
    }
  }
  const title = [...model.entities.values()]
    .map((entity) => model.args(entity, "DRAUGHTING_TITLE"))
    .find((value) => value !== undefined);
  const match = /^(A[0-4])_(horizontal|vertical)$/i.exec(sizeName.trim());
  const type = match ? (SHEET_NAMES[(match[1] as string).toUpperCase()] ?? 9) : 9;
  const orient = match ? ((match[2] as string).toLowerCase() === "horizontal" ? 1 : 0) : 1;
  return {
    sheet: {
      id: sheetEntity.id,
      kind: "drawing_sheet",
      name: model.str(title?.[2]).trim(),
      type,
      orient,
      x: Number.isFinite(x) ? x : 0,
      y: Number.isFinite(y) ? y : 0,
    },
    items,
  };
}

/** P21 ファイルのバイト列を読み、SFC と同じフィーチャに変換して組み立てる */
export function parseP21(bytes: Uint8Array): SxfDrawing {
  const notices = new NoticeList();
  const file = readP21(bytes);
  if (file.skipped > 0) notices.add("warning", "unreadable", "書式を読めないインスタンスを省きました", file.skipped);
  const model = new Model(file, notices);
  const converter = new Converter(model, notices);
  const { sheet, items } = sheetOf(model);
  if (!sheet) notices.add("warning", "no-sheet", "用紙（drawing_sheet_revision）がありません");
  model.doc.sheet = sheet;
  for (const item of items) model.doc.sheetMembers.push(...converter.item(item));
  const counts = model.doc.counts;
  const countAll = (features: readonly SxfFeature[]) => {
    for (const feature of features) counts.set(feature.kind, (counts.get(feature.kind) ?? 0) + 1);
  };
  countAll(model.doc.sheetMembers);
  for (const figure of model.doc.subFigures.values()) countAll(figure.members);
  const preprocessor = file.header.preprocessor;
  const version = /\$\$(\d+(?:\.\d+)?)\s*$/.exec(preprocessor)?.[1];
  return {
    format: "p21",
    version: version ? `SXF ${version}` : "SXF",
    header: file.header,
    document: model.doc,
    notices: notices.toArray(),
  };
}
