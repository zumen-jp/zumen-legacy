import {
  BACKGROUND_COLOR,
  type Box,
  isEntityVisible,
  type Primitive,
  type Scene,
  type SceneEntity,
  type StrokePrimitive,
  type Underlay,
  type UnderlayRegion,
} from "@zumenjs/core";

/**
 * 用紙の mm から CSS の px への変換。rotation が 0 なら x_px = offsetX + x × scale、y_px = offsetY − y × scale。
 * rotation は画面の上で図面を時計回りに回す角度（度）で、用紙の点を原点の周りに回してから同じ式で写す
 */
export interface View {
  scale: number;
  offsetX: number;
  offsetY: number;
  /** 画面の上で時計回りに回す角度（度）。省略は 0 */
  rotation?: number;
}

/** 用紙の点を回す向きの cos・sin（用紙の座標で反時計回りの角度 = −rotation） */
function axes(rotation: number | undefined): [number, number] {
  if (!rotation) return [1, 0];
  // 90 度の倍数は誤差のない値にする（回した図面の直線が斜めにならないように）
  const turns = (((rotation % 360) + 360) % 360) / 90;
  if (Number.isInteger(turns))
    return (
      [
        [1, 0],
        [0, -1],
        [-1, 0],
        [0, 1],
      ] as const
    )[turns] as [number, number];
  const radians = (-rotation * Math.PI) / 180;
  return [Math.cos(radians), Math.sin(radians)];
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
  /** 用紙の枠の色。省略すると、明るい背景では黒、暗い背景では白の 35% */
  sheetFrameColor?: string;
  devicePixelRatio: number;
  /** 図面の上に強調して重ね描きする図形の組。前の組から描き、後の組が上に重なる */
  highlights?: readonly HighlightLayer[];
  /** 背景の上、図形の下に、この順に描く underlay の中身の画像。レイヤが非表示の underlay の画像は描かない */
  underlays?: readonly UnderlayImage[];
}

/**
 * 描き手（UnderlayRenderer）が描いた、underlay の中身の範囲の画像。画像の左上が中身の (region.x, region.y) で、
 * 画像の 1 px が中身の座標の 1 / scale。underlay の中身の外と clip の外は描かない
 */
export interface UnderlayImage {
  readonly underlay: Underlay;
  readonly region: Readonly<UnderlayRegion>;
  readonly scale: number;
  readonly image: CanvasImageSource;
}

/** 強調して重ね描きする図形と、その色。塗りは描かず、線を 3 デバイス px 以上の太さで描く */
export interface HighlightLayer {
  readonly entities: Iterable<SceneEntity>;
  readonly color: string;
}

const REFERENCE_FONT_SIZE = 64;
/** これより小さい文字は字形を描かず、文字の範囲を線で示す（CSS px） */
const MIN_GLYPH_PX = 2.5;

/** 範囲を、rotation の向きで、余白を残して width × height（CSS px）に収める */
export function fitView(bounds: Box, width: number, height: number, margin = 24, rotation = 0): View {
  const [cos, sin] = axes(rotation);
  const bw = Math.max(bounds.maxX - bounds.minX, 1e-6);
  const bh = Math.max(bounds.maxY - bounds.minY, 1e-6);
  // 回した範囲が画面で占める幅と高さ
  const w = Math.abs(bw * cos) + Math.abs(bh * sin);
  const h = Math.abs(bw * sin) + Math.abs(bh * cos);
  const scale = Math.max(Math.min((width - margin * 2) / w, (height - margin * 2) / h), 1e-9);
  return centeredView((bounds.minX + bounds.maxX) / 2, (bounds.minY + bounds.maxY) / 2, scale, width, height, rotation);
}

/** 用紙の点 (cx, cy) を表示領域の中央に置く変換 */
export function centeredView(cx: number, cy: number, scale: number, width: number, height: number, rotation = 0): View {
  const [cos, sin] = axes(rotation);
  const view: View = {
    scale,
    offsetX: width / 2 - (cx * cos - cy * sin) * scale,
    offsetY: height / 2 + (cx * sin + cy * cos) * scale,
  };
  if (rotation) view.rotation = rotation;
  return view;
}

export function toPaper(view: View, px: number, py: number): [number, number] {
  const u = (px - view.offsetX) / view.scale;
  const v = (view.offsetY - py) / view.scale;
  if (!view.rotation) return [u, v];
  const [cos, sin] = axes(view.rotation);
  return [u * cos + v * sin, v * cos - u * sin];
}

export function toScreen(view: View, x: number, y: number): [number, number] {
  if (!view.rotation) return [view.offsetX + x * view.scale, view.offsetY - y * view.scale];
  const [cos, sin] = axes(view.rotation);
  return [view.offsetX + (x * cos - y * sin) * view.scale, view.offsetY - (x * sin + y * cos) * view.scale];
}

/** 表示領域（CSS px）に見えている用紙の範囲。回しているときは、見えている四角形を囲む範囲 */
export function visibleBounds(view: View, width: number, height: number): Box {
  const corners = [
    toPaper(view, 0, 0),
    toPaper(view, width, 0),
    toPaper(view, 0, height),
    toPaper(view, width, height),
  ];
  return {
    minX: Math.min(...corners.map((point) => point[0])),
    minY: Math.min(...corners.map((point) => point[1])),
    maxX: Math.max(...corners.map((point) => point[0])),
    maxY: Math.max(...corners.map((point) => point[1])),
  };
}

/** 画面上の点 (px, py) を中心に factor 倍する */
export function zoomAt(view: View, px: number, py: number, factor: number): View {
  const scale = Math.min(Math.max(view.scale * factor, 1e-6), 1e6);
  const applied = scale / view.scale;
  return { ...view, scale, offsetX: px - (px - view.offsetX) * applied, offsetY: py - (py - view.offsetY) * applied };
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
    const [cos, sin] = axes(view.rotation);
    const s = view.scale * dpr;
    this.base = [s * cos, -s * sin, -s * sin, -s * cos, view.offsetX * dpr, view.offsetY * dpr];
    // 1 デバイス px が用紙の何 mm か
    this.pixel = 1 / (view.scale * dpr);
    this.visible = visibleBounds(view, width, height);
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

  intersectsEntity(entity: SceneEntity, boost = 1): boolean {
    // bbox は線の中心までの範囲。丸い線端・角も含め、描く太さの半分だけ広げて画面と比べる。
    let padding = 0;
    for (const primitive of entity.primitives) {
      if (primitive.kind === "stroke") padding = Math.max(padding, this.lineWidth(primitive.width, boost) / 2);
    }
    return this.intersects(entity.bbox, padding);
  }

  private intersects(box: Box, padding = 0): boolean {
    return !(
      box.maxX + padding < this.visible.minX ||
      box.minX - padding > this.visible.maxX ||
      box.maxY + padding < this.visible.minY ||
      box.minY - padding > this.visible.maxY
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

  underlay({ underlay, region, scale, image }: UnderlayImage): void {
    if (!(scale > 0) || !this.intersects(underlay.bbox)) return;
    this.flush();
    const ctx = this.ctx;
    const m = underlay.matrix;
    const b = this.base;
    ctx.save();
    this.resetTransform();
    if (underlay.clip) ctx.clip(this.ringsPath(underlay.clip), "evenodd");
    // 中身の座標 → デバイス px。中身の範囲の外（画像の端の切り上げた分）は描かない
    ctx.setTransform(
      b[0] * m[0] + b[2] * m[1],
      b[1] * m[0] + b[3] * m[1],
      b[0] * m[2] + b[2] * m[3],
      b[1] * m[2] + b[3] * m[3],
      b[0] * m[4] + b[2] * m[5] + b[4],
      b[1] * m[4] + b[3] * m[5] + b[5],
    );
    ctx.beginPath();
    ctx.rect(0, 0, underlay.width, underlay.height);
    ctx.clip();
    ctx.transform(1 / scale, 0, 0, 1 / scale, region.x, region.y);
    ctx.globalAlpha = underlay.opacity ?? 1;
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(image, 0, 0);
    ctx.restore();
  }

  highlights(layers: readonly HighlightLayer[]): void {
    this.resetTransform();
    for (const layer of layers) {
      const override = { color: layer.color, boost: 3 };
      for (const entity of layer.entities) {
        if (!this.intersectsEntity(entity, override.boost)) continue;
        for (const primitive of entity.primitives) if (primitive.kind !== "fill") this.primitive(primitive, override);
      }
      this.finish();
    }
  }

  sheet(scene: Scene): void {
    if (!scene.sheet) return;
    this.flush();
    const ctx = this.ctx;
    ctx.save();
    ctx.setLineDash([]);
    ctx.lineWidth = this.pixel;
    ctx.strokeStyle =
      this.options.sheetFrameColor ?? (this.options.invertLight ? "rgba(0,0,0,0.35)" : "rgba(255,255,255,0.35)");
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
  for (const image of options.underlays ?? []) {
    if (isEntityVisible(image.underlay, options.hiddenLayers)) painter.underlay(image);
  }
  if (options.sheetFrame) painter.sheet(scene);
  for (const entity of scene.entities) {
    // underlay が見せている図形（drawn が false）は描かない。強調の重ね描きでは描く
    if (entity.drawn === false || !painter.intersectsEntity(entity) || !isEntityVisible(entity, options.hiddenLayers)) {
      continue;
    }
    for (const primitive of entity.primitives) painter.primitive(primitive);
  }
  painter.finish();
  if (options.highlights) painter.highlights(options.highlights);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}

/**
 * 図形（用紙の mm）を、それぞれの色で Canvas の今の内容の上に描く（マークアップなど）。線は太さどおりに描き、
 * 色 "background" は背景色で塗る。width・height は CSS px で、描き終えると変換を単位行列に戻す
 */
export function renderPrimitives(
  ctx: CanvasRenderingContext2D,
  primitives: Iterable<Primitive>,
  view: View,
  width: number,
  height: number,
  options: Pick<RenderOptions, "background" | "devicePixelRatio">,
): void {
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const painter = new Painter(
    ctx,
    view,
    { ...options, invertLight: false, lineWeights: true, hiddenLayers: new Set(), sheetFrame: false },
    width,
    height,
  );
  painter.resetTransform();
  for (const primitive of primitives) painter.primitive(primitive);
  painter.finish();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}

/**
 * 図形を強調の色で、Canvas の今の内容の上に重ね描きする（選択・名前付きの強調・ポインターの下の図形）。
 * レイヤの表示によらず描き、画面の外の図形は飛ばす。width・height は CSS px
 */
export function renderHighlights(
  ctx: CanvasRenderingContext2D,
  layers: readonly HighlightLayer[],
  view: View,
  width: number,
  height: number,
  options: Pick<RenderOptions, "background" | "lineWeights" | "devicePixelRatio">,
): void {
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  const painter = new Painter(
    ctx,
    view,
    { ...options, invertLight: false, hiddenLayers: new Set(), sheetFrame: false },
    width,
    height,
  );
  painter.highlights(layers);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
}
