import {
  BACKGROUND_COLOR,
  type Box,
  type Primitive,
  type Scene,
  type SceneEntity,
  type StrokePrimitive,
} from "./types.js";

/** 用紙の mm から CSS の px への変換。x_px = offsetX + x × scale、y_px = offsetY − y × scale */
export interface View {
  scale: number;
  offsetX: number;
  offsetY: number;
}

export interface RenderOptions {
  /** 背景色（CSS の色） */
  background: string;
  /** 明るい背景で、白に近い線・文字を黒にする */
  invertLight: boolean;
  /** 暗い背景で、黒に近い線・文字を白にする */
  invertDark?: boolean;
  /** 非表示のレイヤ */
  hiddenLayers: ReadonlySet<number>;
  /** 線幅を太さどおりに描く。false なら細線（1 デバイス px） */
  lineWeights: boolean;
  /** 用紙の枠を描く */
  sheetFrame: boolean;
  devicePixelRatio: number;
  /** 強調して重ね描きする要素 */
  highlight?: SceneEntity | null;
  highlightColor?: string;
}

const REFERENCE_FONT_SIZE = 64;
/** これより小さい文字は字形を描かず、文字の範囲を線で示す（CSS px） */
const MIN_GLYPH_PX = 2.5;

export function fitView(bounds: Box, width: number, height: number, margin = 24): View {
  const w = Math.max(bounds.maxX - bounds.minX, 1e-6);
  const h = Math.max(bounds.maxY - bounds.minY, 1e-6);
  const scale = Math.max(Math.min((width - margin * 2) / w, (height - margin * 2) / h), 1e-9);
  return {
    scale,
    offsetX: width / 2 - ((bounds.minX + bounds.maxX) / 2) * scale,
    offsetY: height / 2 + ((bounds.minY + bounds.maxY) / 2) * scale,
  };
}

export function toPaper(view: View, px: number, py: number): [number, number] {
  return [(px - view.offsetX) / view.scale, (view.offsetY - py) / view.scale];
}

export function toScreen(view: View, x: number, y: number): [number, number] {
  return [view.offsetX + x * view.scale, view.offsetY - y * view.scale];
}

/** 画面上の点 (px, py) を中心に factor 倍する */
export function zoomAt(view: View, px: number, py: number, factor: number): View {
  const scale = Math.min(Math.max(view.scale * factor, 1e-6), 1e6);
  const applied = scale / view.scale;
  return { scale, offsetX: px - (px - view.offsetX) * applied, offsetY: py - (py - view.offsetY) * applied };
}

export function isEntityVisible(entity: SceneEntity, hiddenLayers: ReadonlySet<number>): boolean {
  if (hiddenLayers.size === 0) return true;
  for (const layer of entity.layers) if (hiddenLayers.has(layer)) return false;
  return true;
}

/** #rrggbb・#rrggbbaa の輝度（0〜1）。読めない色は null */
function luminance(color: string): number | null {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})(?:[0-9a-f]{2})?$/i.exec(color);
  if (!match) return null;
  return (
    (Number.parseInt(match[1] as string, 16) * 0.299 +
      Number.parseInt(match[2] as string, 16) * 0.587 +
      Number.parseInt(match[3] as string, 16) * 0.114) /
    255
  );
}

class Painter {
  private pathOpen = false;
  private pathKey = "";
  private font = "";
  /** 用紙の mm から デバイス px への変換 */
  private readonly base: [number, number, number, number, number, number];
  private readonly pixel: number;
  private readonly visible: Box;
  private readonly colorCache = new Map<string, string>();

  constructor(
    private readonly ctx: CanvasRenderingContext2D,
    view: View,
    private readonly options: RenderOptions,
    width: number,
    height: number,
  ) {
    const dpr = options.devicePixelRatio;
    this.base = [view.scale * dpr, 0, 0, -view.scale * dpr, view.offsetX * dpr, view.offsetY * dpr];
    // 1 デバイス px が用紙の何 mm か
    this.pixel = 1 / (view.scale * dpr);
    const [minX, maxY] = toPaper(view, 0, 0);
    const [maxX, minY] = toPaper(view, width, height);
    this.visible = { minX, minY, maxX, maxY };
  }

  /**
   * 背景と見分けにくい色（白背景の白に近い色、黒背景の黒に近い色）を反対の色にする。
   * #rrggbbaa の不透明度は保つ
   */
  color(color: string): string {
    if (color === BACKGROUND_COLOR) return this.options.background;
    if (!this.options.invertLight && !this.options.invertDark) return color;
    let mapped = this.colorCache.get(color);
    if (mapped === undefined) {
      const value = luminance(color);
      const alpha = color.length === 9 ? color.slice(7) : "";
      mapped =
        value === null
          ? color
          : this.options.invertLight && value > 0.85
            ? `#000000${alpha}`
            : this.options.invertDark && value < 0.05
              ? `#ffffff${alpha}`
              : color;
      this.colorCache.set(color, mapped);
    }
    return mapped;
  }

  intersects(box: Box): boolean {
    return !(
      box.maxX < this.visible.minX ||
      box.minX > this.visible.maxX ||
      box.maxY < this.visible.minY ||
      box.minY > this.visible.maxY
    );
  }

  resetTransform(): void {
    this.ctx.setTransform(...this.base);
  }

  private lineWidth(width: number, boost: number): number {
    const minimum = this.pixel * boost;
    return this.options.lineWeights ? Math.max(width, minimum) : minimum;
  }

  private dashes(dashes: readonly number[]): number[] {
    if (dashes.length === 0) return [];
    const total = dashes.reduce((sum, value) => sum + value, 0);
    // 1 周期が 3 デバイス px より短い破線は、実線と見分けられないので実線で描く
    if (total < this.pixel * 3) return [];
    return dashes.map((value) => Math.max(value, this.pixel));
  }

  private flush(): void {
    if (this.pathOpen) this.ctx.stroke();
    this.pathOpen = false;
    this.pathKey = "";
  }

  stroke(primitive: StrokePrimitive, override?: { color: string; boost: number }): void {
    if (primitive.color === "transparent" && !override) return;
    const color = override?.color ?? this.color(primitive.color);
    const width = this.lineWidth(primitive.width, override?.boost ?? 1);
    const key = `${color}|${width}|${primitive.dashes.join(",")}`;
    if (key !== this.pathKey) {
      this.flush();
      const ctx = this.ctx;
      ctx.strokeStyle = color;
      ctx.lineWidth = width;
      ctx.setLineDash(this.dashes(primitive.dashes));
      ctx.beginPath();
      this.pathOpen = true;
      this.pathKey = key;
    }
    const points = primitive.points;
    const ctx = this.ctx;
    if (primitive.segments) {
      for (let i = 0; i + 3 < points.length; i += 4) {
        ctx.moveTo(points[i] as number, points[i + 1] as number);
        ctx.lineTo(points[i + 2] as number, points[i + 3] as number);
      }
      return;
    }
    if (points.length < 2) return;
    ctx.moveTo(points[0] as number, points[1] as number);
    for (let i = 2; i + 1 < points.length; i += 2) ctx.lineTo(points[i] as number, points[i + 1] as number);
    if (primitive.closed) ctx.closePath();
  }

  private ringsPath(rings: readonly Float64Array[]): Path2D {
    const path = new Path2D();
    for (const ring of rings) {
      if (ring.length < 6) continue;
      path.moveTo(ring[0] as number, ring[1] as number);
      for (let i = 2; i + 1 < ring.length; i += 2) path.lineTo(ring[i] as number, ring[i + 1] as number);
      path.closePath();
    }
    return path;
  }

  primitive(primitive: Primitive, override?: { color: string; boost: number }): void {
    switch (primitive.kind) {
      case "stroke":
        this.stroke(primitive, override);
        return;
      case "fill": {
        this.flush();
        this.ctx.fillStyle = override?.color ?? this.color(primitive.color);
        this.ctx.fill(this.ringsPath(primitive.rings), "evenodd");
        return;
      }
      case "clipped": {
        this.flush();
        this.ctx.save();
        this.ctx.clip(this.ringsPath(primitive.rings), "evenodd");
        for (const stroke of primitive.strokes) this.stroke(stroke, override);
        this.flush();
        this.ctx.restore();
        return;
      }
      case "dot": {
        this.flush();
        const r = this.pixel * 1.5 * this.options.devicePixelRatio;
        this.ctx.fillStyle = override?.color ?? this.color(primitive.color);
        this.ctx.fillRect(primitive.x - r, primitive.y - r, r * 2, r * 2);
        return;
      }
      case "text":
        this.text(primitive, override?.color);
    }
  }

  private text(primitive: Extract<Primitive, { kind: "text" }>, override?: string): void {
    this.flush();
    const ctx = this.ctx;
    const m = primitive.matrix;
    const b = this.base;
    // 文字の座標 → デバイス px
    const t0 = b[0] * m[0] + b[2] * m[1];
    const t1 = b[1] * m[0] + b[3] * m[1];
    const t2 = b[0] * m[2] + b[2] * m[3];
    const t3 = b[1] * m[2] + b[3] * m[3];
    const t4 = b[0] * m[4] + b[2] * m[5] + b[4];
    const t5 = b[1] * m[4] + b[3] * m[5] + b[5];
    const color = override ?? this.color(primitive.color);
    const glyphPx = (Math.hypot(t2, t3) * primitive.height) / this.options.devicePixelRatio;
    if (glyphPx < MIN_GLYPH_PX) {
      // 小さい文字は、文字範囲の中央に線を引いて位置だけ示す
      const y = primitive.boxHeight / 2;
      ctx.setTransform(t0, t1, t2, t3, t4, t5);
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(primitive.boxWidth, y);
      ctx.strokeStyle = color;
      ctx.lineWidth = Math.max(primitive.boxHeight * 0.4, 1 / Math.max(Math.hypot(t2, t3), 1e-12));
      ctx.setLineDash([]);
      ctx.globalAlpha = 0.6;
      ctx.stroke();
      ctx.globalAlpha = 1;
      this.resetTransform();
      return;
    }
    const font = `${REFERENCE_FONT_SIZE}px ${primitive.fontFamily}`;
    if (font !== this.font) {
      ctx.font = font;
      this.font = font;
    }
    ctx.fillStyle = color;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const sx = (primitive.scaleX * primitive.height) / REFERENCE_FONT_SIZE;
    const sy = -primitive.height / REFERENCE_FONT_SIZE;
    for (const cell of primitive.cells) {
      // 文字の座標で (x, y) へ移動し、上下を反転して字形を正立させる
      ctx.setTransform(
        t0 * sx,
        t1 * sx,
        t2 * sy,
        t3 * sy,
        t0 * cell.x + t2 * cell.y + t4,
        t1 * cell.x + t3 * cell.y + t5,
      );
      ctx.fillText(cell.char, 0, 0);
    }
    this.resetTransform();
  }

  finish(): void {
    this.flush();
  }

  sheet(scene: Scene): void {
    if (!scene.sheet) return;
    this.flush();
    const ctx = this.ctx;
    ctx.save();
    ctx.setLineDash([]);
    ctx.lineWidth = this.pixel;
    ctx.strokeStyle = this.options.invertLight ? "rgba(0,0,0,0.35)" : "rgba(255,255,255,0.35)";
    ctx.strokeRect(0, 0, scene.sheet.width, scene.sheet.height);
    ctx.restore();
  }
}

/** 図面を Canvas に描く。width・height は CSS px */
export function renderScene(
  ctx: CanvasRenderingContext2D,
  scene: Scene,
  view: View,
  width: number,
  height: number,
  options: RenderOptions,
): void {
  const dpr = options.devicePixelRatio;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.fillStyle = options.background;
  ctx.fillRect(0, 0, width * dpr, height * dpr);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const painter = new Painter(ctx, view, options, width, height);
  painter.resetTransform();
  if (options.sheetFrame) painter.sheet(scene);
  for (const entity of scene.entities) {
    if (!painter.intersects(entity.bbox) || !isEntityVisible(entity, options.hiddenLayers)) continue;
    for (const primitive of entity.primitives) painter.primitive(primitive);
  }
  painter.finish();
  const highlight = options.highlight;
  if (highlight) {
    const override = { color: options.highlightColor ?? "#ffb000", boost: 3 };
    for (const primitive of highlight.primitives) {
      if (primitive.kind !== "fill") painter.primitive(primitive, override);
    }
    painter.finish();
  }
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}
