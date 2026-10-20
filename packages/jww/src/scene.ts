import {
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
  arcSegments,
  ellipseArcPoints,
  emptyBox,
  extendBox,
  extendBoxWithPoints,
  isEmptyBox,
  type Matrix,
  meanScale,
  multiply,
  sweepRadians,
  transformPoints,
  unionBox,
} from "@zumenjs/core/geometry";
import { fontFamilyFor, layoutText } from "@zumenjs/core/text";
import type { JwwBase, JwwDocument, JwwEntity } from "./reader.js";
import { SXF_COLORS, SXF_LINE_PITCHES } from "./sxf-tables.js";

/** 用紙サイズの番号と寸法（横長、mm） */
const PAPER_SIZES: Readonly<Record<number, { label: string; width: number; height: number }>> = {
  0: { label: "A0", width: 1189, height: 841 },
  1: { label: "A1", width: 841, height: 594 },
  2: { label: "A2", width: 594, height: 420 },
  3: { label: "A3", width: 420, height: 297 },
  4: { label: "A4", width: 297, height: 210 },
  8: { label: "2A", width: 1682, height: 1189 },
  9: { label: "3A", width: 2378, height: 1682 },
  10: { label: "4A", width: 3364, height: 2378 },
  11: { label: "5A", width: 4756, height: 3364 },
};

/** ヘッダの色が読めないときの、白背景の画面色（線色 1〜9） */
const DEFAULT_PEN_COLORS = [
  "#ffffff",
  "#00c0c0",
  "#000000",
  "#00c000",
  "#c0c000",
  "#c000c0",
  "#0000ff",
  "#008080",
  "#ff0080",
  "#c0c0c0",
];
/** ヘッダの線種が読めないときの破線（線種 2〜9、mm） */
const DEFAULT_LINE_TYPES: Readonly<Record<number, number[]>> = {
  2: [1, 1],
  3: [2, 1],
  4: [4, 1],
  5: [6, 1, 1, 1],
  6: [10, 1, 1, 1],
  7: [6, 1, 1, 1, 1, 1],
  8: [10, 1, 1, 1, 1, 1],
  9: [0.5, 0.5],
};

const MAX_DEPTH = 16;
const TOLERANCE = 0.02;

function hex(value: number): string {
  const r = value & 0xff;
  const g = (value >> 8) & 0xff;
  const b = (value >> 16) & 0xff;
  return `#${[r, g, b].map((part) => part.toString(16).padStart(2, "0")).join("")}`;
}

function number(value: number, digits = 3): string {
  if (!Number.isFinite(value)) return String(value);
  return formatter.format(Number(value.toFixed(digits)));
}

const formatter = new Intl.NumberFormat("ja-JP", { maximumFractionDigits: 3 });

function groupName(doc: JwwDocument, group: number): string {
  return doc.groups[group]?.name.trim() || `グループ ${group.toString(16).toUpperCase()}`;
}

/** Jw_cad は既定のレイヤ名を「0-A」のように番号のまま保存するため、番号と同じ名前は重ねて示さない */
function layerName(doc: JwwDocument, group: number, layer: number): string {
  const code = `${group.toString(16).toUpperCase()}-${layer.toString(16).toUpperCase()}`;
  const name = doc.groups[group]?.layers[layer]?.name.trim() ?? "";
  return name && name.toUpperCase() !== code ? `${code} ${name}` : code;
}

interface Context {
  m: Matrix;
  layers: readonly number[];
  path: string;
  depth: number;
}

class Builder {
  readonly entities: SceneEntity[] = [];
  /** 文字として保存された Jw_cad の設定（Printer_Orientation = 0 など） */
  readonly settings: string[] = [];
  readonly usedLayers = new Map<number, number>();
  private readonly blocks: Map<number, JwwDocument["blocks"][number]>;
  private readonly placements = new Map<number, Placement>();

  constructor(
    private readonly doc: JwwDocument,
    private readonly origin: [number, number],
    private readonly notices: NoticeList,
  ) {
    this.blocks = new Map(doc.blocks.map((block) => [block.number, block]));
  }

  layerId(base: JwwBase): number {
    return (base.layerGroup & 0x0f) * 16 + (base.layer & 0x0f) + 1;
  }

  color(base: JwwBase, custom: number | null = null): string {
    const n = base.penColor;
    if (n === 10 && custom !== null) return hex(custom);
    if (n >= 1 && n <= 9) return this.doc.penColors?.[n] ?? DEFAULT_PEN_COLORS[n] ?? "#000000";
    if (n >= 100 && n <= 356) {
      const extended = this.doc.extendedColors?.[n - 100];
      if (extended) return extended;
      const sxf = SXF_COLORS[n - 101];
      if (sxf) return `#${sxf.map((part) => part.toString(16).padStart(2, "0")).join("")}`;
    }
    this.notices.add("warning", "unknown-color", "定義のない線色を黒で表示しました");
    return "#000000";
  }

  dashes(style: number): number[] {
    if (style <= 1 || (style >= 11 && style <= 15)) return [];
    const fromHeader = this.doc.lineTypes.get(style)?.segments;
    if (fromHeader && fromHeader.length >= 2 && fromHeader.every((value) => value > 0)) {
      return fromHeader.length % 2 === 0 ? fromHeader : [...fromHeader, ...fromHeader];
    }
    if (style >= 31 && style <= 46) return [...(SXF_LINE_PITCHES[style - 31] ?? [])];
    return DEFAULT_LINE_TYPES[style] ?? [];
  }

  width(base: JwwBase): number {
    if (base.penWidth > 0 && this.doc.widthUnit.hundredth) return base.penWidth / 100;
    if (base.penWidth > 0 && this.doc.widthUnit.dpi) return (base.penWidth * 25.4) / this.doc.widthUnit.dpi;
    const n = base.penColor;
    return (n >= 1 && n <= 9 ? this.doc.penWidths?.[n] : null) ?? 0.18;
  }

  /** レイヤグループの縮尺。用紙の座標から実寸の座標へ戻す変換を持つ */
  placement(group: number): Placement {
    const index = group & 0x0f;
    let placement = this.placements.get(index);
    if (!placement) {
      const info = this.doc.groups[index];
      const scale = info && info.scale > 0 ? info.scale : 1;
      placement = {
        name: `${groupName(this.doc, index)}（1/${number(scale)}）`,
        scaleX: 1 / scale,
        scaleY: 1 / scale,
        inverse: [scale, 0, 0, scale, -this.origin[0] * scale, -this.origin[1] * scale],
      };
      this.placements.set(index, placement);
    }
    return placement;
  }

  private stroke(ctx: Context, base: JwwBase, points: number[], closed: boolean): StrokePrimitive {
    return {
      kind: "stroke",
      points: transformPoints(ctx.m, points),
      closed,
      color: this.color(base),
      width: this.width(base),
      dashes: this.dashes(base.penStyle),
    };
  }

  /** 実寸の座標の属性 */
  private real(key: string, label: string, base: JwwBase, x: number, y: number, ctx: Context): Property {
    const [px, py] = apply(ctx.m, x, y);
    const [rx, ry] = apply(this.placement(base.layerGroup).inverse, px, py);
    return { key, label, value: `${number(rx)}, ${number(ry)}`, raw: [rx, ry] };
  }

  private add(
    ctx: Context,
    base: JwwBase,
    id: number,
    kind: EntityKind,
    type: string,
    source: string,
    primitives: Primitive[],
    properties: Property[],
  ): void {
    if (primitives.length === 0) return;
    const bbox = emptyBox();
    for (const primitive of primitives) extendPrimitiveBox(bbox, primitive);
    if (isEmptyBox(bbox)) return;
    const layer = this.layerId(base);
    const layers = [...ctx.layers, layer];
    for (const code of layers) this.usedLayers.set(code, (this.usedLayers.get(code) ?? 0) + 1);
    const group = this.doc.groups[base.layerGroup & 0x0f];
    const placement = this.placement(base.layerGroup);
    const scale = group && group.scale > 0 ? group.scale : 1;
    this.entities.push({
      id: `${ctx.path}#${id}`,
      kind,
      type,
      source,
      layers,
      primitives,
      bbox,
      placement,
      properties: [
        { key: "type", label: "種類", value: type },
        { key: "id", label: "番号", value: `${ctx.path}#${id}` },
        {
          key: "layer",
          label: "レイヤ",
          value: `${groupName(this.doc, base.layerGroup & 0x0f)} / ${layerName(this.doc, base.layerGroup & 0x0f, base.layer & 0x0f)}`,
        },
        { key: "scale", label: "縮尺", value: `1/${number(scale)}`, raw: scale },
        { key: "color", label: "線色", value: String(base.penColor), raw: base.penColor },
        { key: "lineType", label: "線種", value: String(base.penStyle), raw: base.penStyle },
        ...properties,
      ],
    });
  }

  emit(entity: JwwEntity, id: number, ctx: Context): void {
    switch (entity.kind) {
      case "line":
        this.add(
          ctx,
          entity,
          id,
          "line",
          "線",
          "CDataSen",
          [this.stroke(ctx, entity, [entity.x1, entity.y1, entity.x2, entity.y2], false)],
          [
            this.real("start", "始点（実寸）", entity, entity.x1, entity.y1, ctx),
            this.real("end", "終点（実寸）", entity, entity.x2, entity.y2, ctx),
          ],
        );
        return;
      case "arc": {
        const points = this.arcPoints(
          ctx,
          entity.cx,
          entity.cy,
          entity.radius,
          entity.flatness,
          entity.tilt,
          entity.start,
          entity.sweep,
          entity.full,
        );
        const ellipse = entity.flatness !== 1 && Math.abs(entity.flatness - 1) > 1e-9;
        const type = ellipse ? "楕円" : entity.full ? "円" : "円弧";
        const kind = ellipse ? (entity.full ? "ellipse" : "ellipse-arc") : entity.full ? "circle" : "arc";
        const radius = entity.radius * (this.doc.groups[entity.layerGroup & 0x0f]?.scale || 1);
        this.add(
          ctx,
          entity,
          id,
          kind,
          type,
          "CDataEnko",
          [this.stroke(ctx, entity, points, entity.full)],
          [
            this.real("center", "中心（実寸）", entity, entity.cx, entity.cy, ctx),
            { key: "radius", label: "半径（実寸）", value: number(radius), raw: radius },
          ],
        );
        return;
      }
      case "point": {
        if (entity.temporary) {
          this.notices.add("info", "temporary-point", "仮点は印刷されないため表示していません");
          return;
        }
        const [x, y] = apply(ctx.m, entity.x, entity.y);
        this.add(
          ctx,
          entity,
          id,
          "point",
          "点",
          "CDataTen",
          [{ kind: "dot", x, y, color: this.color(entity) }],
          [this.real("position", "位置（実寸）", entity, entity.x, entity.y, ctx)],
        );
        return;
      }
      case "text": {
        // Jw_cad 8 以降は、印刷・表示の設定を (0, -1000) の「名前 = 値」の文字として保存する。図形ではないので描かない
        if (
          ctx.depth === 0 &&
          entity.x1 === 0 &&
          entity.y1 === -1000 &&
          /^[A-Za-z][A-Za-z0-9_]* = /.test(entity.content)
        ) {
          this.settings.push(entity.content);
          return;
        }
        const text = this.text(ctx, entity);
        if (!text) return;
        this.add(
          ctx,
          entity,
          id,
          "text",
          "文字",
          "CDataMoji",
          [text],
          [
            { key: "text", label: "文字列", value: entity.content },
            { key: "font", label: "フォント", value: entity.font },
            {
              key: "textSize",
              label: "文字の大きさ",
              value: `幅 ${number(entity.sizeX)}・高さ ${number(entity.sizeY)}・間隔 ${number(entity.spacing)}（mm）`,
              raw: [entity.sizeX, entity.sizeY, entity.spacing],
            },
          ],
        );
        return;
      }
      case "solid": {
        const ring = transformPoints(ctx.m, entity.points);
        this.add(
          ctx,
          entity,
          id,
          "solid",
          "ソリッド",
          "CDataSolid",
          [{ kind: "fill", color: this.color(entity, entity.color), rings: [ring] }],
          [],
        );
        return;
      }
      case "circle-solid": {
        const full = entity.mode === 100 || Math.abs(Math.abs(entity.sweep) - Math.PI * 2) < 1e-6;
        let points = this.arcPoints(
          ctx,
          entity.cx,
          entity.cy,
          Math.abs(entity.radius),
          entity.flatness || 1,
          entity.tilt,
          full ? 0 : entity.start,
          full ? Math.PI * 2 : entity.sweep,
          full,
        );
        if (!full && entity.mode !== -1 && entity.mode !== 5) points = [entity.cx, entity.cy, ...points];
        const ring = transformPoints(ctx.m, points);
        this.add(
          ctx,
          entity,
          id,
          "solid",
          "円ソリッド",
          "CDataSolid",
          [{ kind: "fill", color: this.color(entity, entity.color), rings: [ring] }],
          [],
        );
        return;
      }
      case "block": {
        const block = this.blocks.get(entity.definition);
        if (!block) {
          this.notices.add("warning", "missing-block", "定義のないブロックの配置を省きました");
          return;
        }
        if (ctx.depth >= MAX_DEPTH) {
          this.notices.add("warning", "block-depth", "入れ子が深すぎるブロックを省きました");
          return;
        }
        const cos = Math.cos(entity.rotation);
        const sin = Math.sin(entity.rotation);
        const local: Matrix = [
          cos * entity.scaleX,
          sin * entity.scaleX,
          -sin * entity.scaleY,
          cos * entity.scaleY,
          entity.x,
          entity.y,
        ];
        const child: Context = {
          m: multiply(ctx.m, local),
          layers: [...ctx.layers, this.layerId(entity)],
          path: `${ctx.path}#${id}>`,
          depth: ctx.depth + 1,
        };
        for (const [index, member] of block.entities.entries()) this.emit(member, index + 1, child);
        return;
      }
      case "dimension": {
        const primitives: Primitive[] = [
          this.stroke(ctx, entity.line, [entity.line.x1, entity.line.y1, entity.line.x2, entity.line.y2], false),
        ];
        for (const line of entity.lines) {
          if (line.x1 === line.x2 && line.y1 === line.y2) continue;
          primitives.push(this.stroke(ctx, line, [line.x1, line.y1, line.x2, line.y2], false));
        }
        for (const point of entity.points) {
          if (point.temporary) continue;
          const [x, y] = apply(ctx.m, point.x, point.y);
          primitives.push({ kind: "dot", x, y, color: this.color(point) });
        }
        const text = this.text(ctx, entity.text);
        if (text) primitives.push(text);
        this.add(ctx, entity, id, "dimension", "寸法", "CDataSunpou", primitives, [
          { key: "dimensionText", label: "寸法値", value: entity.text.content },
        ]);
        return;
      }
    }
  }

  private arcPoints(
    ctx: Context,
    cx: number,
    cy: number,
    radius: number,
    flatness: number,
    tilt: number,
    start: number,
    sweep: number,
    full: boolean,
  ): number[] {
    const deg = 180 / Math.PI;
    const rx = radius;
    const ry = radius * (flatness > 0 ? flatness : 1);
    const begin = full ? 0 : start * deg;
    const end = full ? 360 : (start + sweep) * deg;
    const ccw = full || sweep >= 0;
    const span = full ? Math.PI * 2 : Math.abs(sweepRadians(begin, end, ccw));
    const tolerance = TOLERANCE / Math.max(meanScale(ctx.m), 1e-12);
    const points = ellipseArcPoints(
      cx,
      cy,
      rx,
      ry,
      tilt * deg,
      begin,
      end,
      ccw,
      arcSegments(Math.max(rx, ry), span, tolerance),
    );
    return full ? points.slice(0, -2) : points;
  }

  private text(ctx: Context, text: Extract<JwwEntity, { kind: "text" }>): ReturnType<typeof layoutText> {
    if (!text.content) return null;
    if (text.content.startsWith("^@")) {
      this.notices.add("info", "embedded-image", "文字に埋め込んだ画像（^@BM など）は表示していません");
      return null;
    }
    return layoutText(
      {
        text: text.content,
        x: text.x1,
        y: text.y1,
        height: text.sizeY > 0 ? text.sizeY : 2.5,
        width: Math.hypot(text.x2 - text.x1, text.y2 - text.y1),
        spacing: text.spacing,
        angle: text.angle,
        slant: 0,
        anchor: 1,
        vertical: text.font.startsWith("@"),
        color: this.color(text),
        fontFamily: fontFamilyFor(text.font.replace(/^@/, "")),
      },
      ctx.m,
    );
  }
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

function luminance(color: string): number {
  const value = Number.parseInt(color.slice(1), 16);
  return ((value >> 16) * 0.299 + ((value >> 8) & 0xff) * 0.587 + (value & 0xff) * 0.114) / 255;
}

/** JWW の図面を、用紙の座標（左下が原点の mm）に解決した表示用の図面にする */
export function jwwToScene(doc: JwwDocument): Scene {
  const notices = new NoticeList();
  if (doc.truncated)
    notices.add("warning", "truncated", `途中から読めなかったため、読めた図形だけを表示しました（${doc.truncated}）`);
  if (!doc.penColors)
    notices.add("info", "default-palette", "ヘッダの線色を読めないため、Jw_cad の既定の色で表示しました");
  const paper = PAPER_SIZES[doc.paperSize];
  const sheet: Sheet | null = paper ? { name: "", size: paper.label, width: paper.width, height: paper.height } : null;
  // JWW の座標は用紙の中心が原点。用紙の左下を原点にする
  const origin: [number, number] = sheet ? [sheet.width / 2, sheet.height / 2] : [0, 0];
  const builder = new Builder(doc, origin, notices);
  const root: Context = { m: [1, 0, 0, 1, origin[0], origin[1]], layers: [], path: "", depth: 0 };
  for (const [index, entity] of doc.entities.entries()) builder.emit(entity, index + 1, root);
  const bounds = emptyBox();
  for (const entity of builder.entities) unionBox(bounds, entity.bbox);
  if (sheet) {
    extendBox(bounds, 0, 0);
    extendBox(bounds, sheet.width, sheet.height);
  }
  const layers: SceneLayer[] = [];
  for (const id of [...builder.usedLayers.keys()].sort((a, b) => a - b)) {
    const groupIndex = Math.floor((id - 1) / 16);
    const layerIndex = (id - 1) % 16;
    const group = doc.groups[groupIndex];
    const layer = group?.layers[layerIndex];
    layers.push({
      id,
      name: layerName(doc, groupIndex, layerIndex),
      visible: (group?.state ?? 1) !== 0 && (layer?.state ?? 1) !== 0,
      group: groupName(doc, groupIndex),
    });
  }
  const background = luminance(doc.penColors?.[0] ?? "#ffffff") < 0.5 ? "dark" : "light";
  const metadata: Property[] = [{ key: "format", label: "形式", value: `JWW（版 ${doc.version}）` }];
  if (sheet) metadata.push({ key: "sheet", label: "用紙", value: `${sheet.size}（${sheet.width}×${sheet.height}mm）` });
  // 図形のあるレイヤグループの縮尺だけを示す
  const usedGroups = new Set([...builder.usedLayers.keys()].map((id) => Math.floor((id - 1) / 16)));
  const scales = [
    ...new Set(
      doc.groups.filter((group, index) => usedGroups.has(index) && group.scale > 0).map((group) => group.scale),
    ),
  ];
  metadata.push({
    key: "scale",
    label: "縮尺",
    value: scales.map((scale) => `1/${number(scale)}`).join("・"),
    raw: scales,
  });
  if (doc.memo.trim()) metadata.push({ key: "memo", label: "メモ", value: doc.memo.trim() });
  if (doc.blocks.length > 0)
    metadata.push({ key: "blockCount", label: "ブロック", value: `${doc.blocks.length} 件`, raw: doc.blocks.length });
  if (builder.settings.length > 0)
    metadata.push({ key: "settings", label: "Jw_cad の設定", value: builder.settings.join("、") });
  return {
    format: "jww",
    version: `JWW ${doc.version}`,
    sheet,
    bounds,
    layers,
    entities: builder.entities,
    notices: notices.toArray(),
    metadata,
    background,
  };
}
