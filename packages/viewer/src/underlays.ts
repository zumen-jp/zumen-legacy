import { isEntityVisible, type Scene, type Underlay, type UnderlayRegion, type UnderlayRequest } from "@zumenjs/core";
import { apply, invert } from "@zumenjs/core/geometry";
import { toPaper, type UnderlayImage, type View, visibleBounds } from "./render.js";

/** 描き手が描く先 */
export interface UnderlayTarget {
  /** request.width × request.height px の、透明な Canvas */
  readonly canvas: HTMLCanvasElement;
  readonly context: CanvasRenderingContext2D;
  /** 要求が古くなったとき（表示が動いた、レイヤ・図面・描き手が替わった、壊した）に中止される */
  readonly signal: AbortSignal;
}

/**
 * underlay の中身を描く処理。アプリケーションが渡す（画像なら imageUnderlayRenderer、PDF なら PDF の描画エンジン）。
 * 要求の範囲を target.canvas に描き終えたら解決し、描けなければ拒否する。target.canvas の代わりに、描いた画像
 * （ワーカーで描いた ImageBitmap など、request.width × request.height px）を返してもよい。返した画像は viewer が
 * 要らなくなるまで持ち、close しない。signal が中止されたら描くのをやめてよく、その結果は使わない
 */
export type UnderlayRenderer = (
  request: UnderlayRequest,
  target: UnderlayTarget,
  // biome-ignore lint/suspicious/noConfusingVoidType: target.canvas に描いて何も返さない描き手を書けるように
) => Promise<CanvasImageSource | void> | CanvasImageSource | void;

/** underlay の要求が終わった。取り消した要求では送らない */
export interface UnderlayEvent {
  readonly underlay: Underlay;
  readonly request: UnderlayRequest;
  /** 描き手が拒否した理由。描けたら null。拒否した underlay は、refreshUnderlays などで描き直すまで求めない */
  readonly error: unknown;
}

/**
 * 画像（HTMLImageElement・ImageBitmap・Canvas など）を描く UnderlayRenderer を作る。load は underlay の画像を返す
 * （underlay ごとに一度だけ呼び、拒否するか null を返したときは、次の要求でまた呼ぶ）。画像の大きさが underlay の
 * width・height と違っても、中身の全体に合わせて描く
 */
export function imageUnderlayRenderer(
  load: (underlay: Underlay) => CanvasImageSource | null | Promise<CanvasImageSource | null>,
): UnderlayRenderer {
  const images = new WeakMap<Underlay, Promise<CanvasImageSource | null>>();
  return async (request, { context, signal }) => {
    const { underlay, region, scale } = request;
    let image = images.get(underlay);
    if (!image) {
      image = Promise.resolve().then(() => load(underlay));
      images.set(underlay, image);
    }
    const source = await image.catch((error: unknown) => {
      images.delete(underlay);
      throw error;
    });
    if (!source) {
      images.delete(underlay);
      throw new Error(`underlay ${underlay.id} の画像がありません`);
    }
    if (signal.aborted) return;
    const [width, height] = sourceSize(source) ?? [underlay.width, underlay.height];
    const sx = width / underlay.width;
    const sy = height / underlay.height;
    context.imageSmoothingQuality = "high";
    context.drawImage(
      source,
      region.x * sx,
      region.y * sy,
      region.width * sx,
      region.height * sy,
      0,
      0,
      region.width * scale,
      region.height * scale,
    );
  };
}

/** 画像の画素の大きさ。分からなければ null */
function sourceSize(source: CanvasImageSource): [number, number] | null {
  if ("naturalWidth" in source) return [source.naturalWidth, source.naturalHeight];
  if ("videoWidth" in source) return [source.videoWidth, source.videoHeight];
  if ("displayWidth" in source) return [source.displayWidth, source.displayHeight];
  if (typeof source.width === "number" && typeof source.height === "number") return [source.width, source.height];
  return null;
}

/** 中身の全体を一度に描く画素数の上限。これより細かく描くときは、見えている範囲だけを描く */
const WHOLE_PIXELS = 2048 * 2048;
/** 全体の画像は、今の表示の何倍の細かさまで描いておくか（少し拡大しても描き直さないように） */
const WHOLE_HEADROOM = 2;
/** 見えている範囲の画像の画素数の上限（回した表示で、範囲を囲む矩形が大きくなるときの歯止め） */
const PART_PIXELS = 4096 * 4096;
/** 見えている範囲の周りに、範囲の大きさのこの割合だけ足して描く（少し動かしても粗い全体が見えないように） */
const PART_MARGIN = 0.1;
/** 描いた画像の細かさが、今の表示に要る細かさのこの割合より粗くなったら描き直す */
const SHARPNESS = 0.8;

/** 要求を決めるための今の表示 */
export interface UnderlayContext {
  readonly scene: Scene;
  readonly view: View;
  /** 表示領域の大きさ（CSS px） */
  readonly width: number;
  readonly height: number;
  readonly dpr: number;
  readonly hiddenLayers: ReadonlySet<number>;
}

/** 今の表示に要る、underlay の中身の範囲と細かさ */
interface Need {
  /** 見えている中身の範囲（中身の座標） */
  readonly visible: UnderlayRegion;
  /** 中身の座標の 1 が何デバイス px か（finest まで） */
  readonly scale: number;
  /** これより細かく描いても変わらない縮尺。画像は画素の 1、PDF は上限なし */
  readonly finest: number;
  /** 非表示の、中身の中のレイヤ */
  readonly hiddenLayers: readonly number[];
  readonly layerKey: string;
}

interface Raster {
  readonly request: UnderlayRequest;
  /** 求めた細かさ。画素数の上限で request.scale を粗くしても、これで今の表示と比べる */
  readonly wanted: number;
  readonly image: CanvasImageSource;
  readonly layerKey: string;
}

interface Pending {
  readonly request: UnderlayRequest;
  readonly wanted: number;
  readonly controller: AbortController;
  readonly layerKey: string;
}

/** underlay ごとの、中身の全体の画像と見えている範囲の画像、その描き途中の要求 */
interface Slot {
  whole: Raster | null;
  part: Raster | null;
  pendingWhole: Pending | null;
  pendingPart: Pending | null;
  failed: boolean;
}

function needOf(underlay: Underlay, context: UnderlayContext): Need | null {
  const { view, width, height, dpr } = context;
  if (width <= 0 || height <= 0 || !isEntityVisible(underlay, context.hiddenLayers)) return null;
  if (!(underlay.width > 0 && underlay.height > 0)) return null;
  const screen = visibleBounds(view, width, height);
  const box = underlay.bbox;
  if (box.maxX < screen.minX || box.minX > screen.maxX || box.maxY < screen.minY || box.minY > screen.maxY) return null;
  const m = underlay.matrix;
  if (!(Math.abs(m[0] * m[3] - m[1] * m[2]) > 0)) return null;
  // 表示領域の 4 隅を、用紙の mm を経て中身の座標にし、中身の範囲と重なる部分を見えている範囲とする
  const inverse = invert(m);
  const corners = [
    [0, 0],
    [width, 0],
    [0, height],
    [width, height],
  ].map(([px, py]) => apply(inverse, ...toPaper(view, px as number, py as number)));
  const xs = corners.map((point) => point[0]);
  const ys = corners.map((point) => point[1]);
  const minX = Math.max(0, Math.min(...xs));
  const minY = Math.max(0, Math.min(...ys));
  const maxX = Math.min(underlay.width, Math.max(...xs));
  const maxY = Math.min(underlay.height, Math.max(...ys));
  if (!(minX < maxX && minY < maxY)) return null;
  const unit = Math.max(Math.hypot(m[0], m[1]), Math.hypot(m[2], m[3]));
  // 画像の中身の座標は画素なので、画素より細かく描いても変わらない
  const finest = underlay.kind === "image" ? 1 : Number.POSITIVE_INFINITY;
  const scale = Math.min(view.scale * dpr * unit, finest);
  if (!(scale > 0 && Number.isFinite(scale))) return null;
  const inside = new Set(underlay.contentLayers ?? []);
  const hiddenLayers = [...context.hiddenLayers].filter((layer) => inside.has(layer)).sort((a, b) => a - b);
  return {
    visible: { x: minX, y: minY, width: maxX - minX, height: maxY - minY },
    scale,
    finest,
    hiddenLayers,
    layerKey: hiddenLayers.join(","),
  };
}

/** 描いた（描いている）画像が、今の表示に要る細かさを持つか */
function sharp(raster: Raster | Pending | null, need: Need, scale = need.scale): boolean {
  return raster !== null && raster.layerKey === need.layerKey && raster.wanted >= scale * SHARPNESS;
}

/** 見えている範囲の画像が、今の表示の見えている範囲を覆い、要る細かさを持つか */
function fits(raster: Raster | Pending | null, need: Need): boolean {
  if (!raster || !sharp(raster, need)) return false;
  const region = raster.request.region;
  const visible = need.visible;
  const epsilon = 1e-9 * Math.max(region.width, region.height);
  return (
    region.x <= visible.x + epsilon &&
    region.y <= visible.y + epsilon &&
    region.x + region.width >= visible.x + visible.width - epsilon &&
    region.y + region.height >= visible.y + visible.height - epsilon
  );
}

/**
 * underlay の中身の画像を描き手に求め、描いた画像を持つ。表示が止まるたびに、中身の全体を粗く描いた画像と、
 * それでは粗すぎるときは見えている範囲を今の細かさで描いた画像を求める。動いている間は持っている画像を
 * 引き伸ばして描き、今の表示に合わなくなった要求は取り消す
 */
export class UnderlayCache {
  private renderer: UnderlayRenderer | null = null;
  private readonly slots = new Map<Underlay, Slot>();

  constructor(
    private readonly doc: Document,
    /** 画像が届いたとき */
    private readonly changed: () => void,
    private readonly finished: (event: UnderlayEvent) => void,
  ) {}

  get hasRenderer(): boolean {
    return this.renderer !== null;
  }

  setRenderer(renderer: UnderlayRenderer | null): void {
    this.clear();
    this.renderer = renderer;
  }

  /** 要求を取り消し、描いた画像を捨てる。ids を渡せば、その id の underlay だけ */
  clear(ids?: ReadonlySet<string>): void {
    for (const [underlay, slot] of this.slots) {
      if (ids && !ids.has(underlay.id)) continue;
      slot.pendingWhole?.controller.abort();
      slot.pendingPart?.controller.abort();
      this.slots.delete(underlay);
    }
  }

  /** 描く画像。scene.underlays の順に、全体の画像と、その上に見えている範囲の画像 */
  images(scene: Scene): UnderlayImage[] {
    const images: UnderlayImage[] = [];
    if (this.slots.size === 0) return images;
    for (const underlay of scene.underlays ?? []) {
      const slot = this.slots.get(underlay);
      if (!slot) continue;
      for (const raster of [slot.whole, slot.part]) {
        if (raster) {
          images.push({ underlay, region: raster.request.region, scale: raster.request.scale, image: raster.image });
        }
      }
    }
    return images;
  }

  /** 表示が動いた。今の表示に合わなくなった、見えている範囲の要求を取り消す */
  moved(context: UnderlayContext): void {
    for (const [underlay, slot] of this.slots) {
      const pending = slot.pendingPart;
      if (!pending) continue;
      const need = needOf(underlay, context);
      if (need && fits(pending, need)) continue;
      pending.controller.abort();
      slot.pendingPart = null;
    }
  }

  /** 表示が止まった。今の表示に足りない画像を求める */
  plan(context: UnderlayContext): void {
    if (!this.renderer) return;
    for (const underlay of context.scene.underlays ?? []) {
      const need = needOf(underlay, context);
      let slot = this.slots.get(underlay);
      if (!need) {
        // 見えない underlay の、見えている範囲の画像は捨てる（全体の画像は残す）
        if (slot) {
          slot.pendingPart?.controller.abort();
          slot.pendingPart = null;
          slot.part = null;
        }
        continue;
      }
      if (!slot) {
        slot = { whole: null, part: null, pendingWhole: null, pendingPart: null, failed: false };
        this.slots.set(underlay, slot);
      }
      if (slot.failed) continue;
      // 中身の全体を一度に描ける最も細かい縮尺
      const limit = Math.min(Math.sqrt(WHOLE_PIXELS / (underlay.width * underlay.height)), need.finest);
      const whole = { x: 0, y: 0, width: underlay.width, height: underlay.height };
      if (need.scale <= limit) {
        // 全体の画像で足りる
        slot.pendingPart?.controller.abort();
        slot.pendingPart = null;
        if (sharp(slot.whole, need)) slot.part = null;
        else if (!sharp(slot.pendingWhole, need)) {
          slot.pendingWhole?.controller.abort();
          slot.pendingWhole = this.start(
            underlay,
            slot,
            "whole",
            whole,
            Math.min(need.scale * WHOLE_HEADROOM, limit),
            need,
          );
        }
        continue;
      }
      // 見えている範囲を今の細かさで描き、全体の画像は動かしたときのために一度に描ける細かさにしておく
      if (!fits(slot.part, need) && !fits(slot.pendingPart, need)) {
        slot.pendingPart?.controller.abort();
        slot.pendingPart = this.start(underlay, slot, "part", partRegion(underlay, need), need.scale, need);
      }
      if (!sharp(slot.whole, need, limit) && !sharp(slot.pendingWhole, need, limit)) {
        slot.pendingWhole?.controller.abort();
        slot.pendingWhole = this.start(underlay, slot, "whole", whole, limit, need);
      }
    }
  }

  private start(
    underlay: Underlay,
    slot: Slot,
    kind: "whole" | "part",
    region: UnderlayRegion,
    wanted: number,
    need: Need,
  ): Pending | null {
    const renderer = this.renderer;
    if (!renderer) return null;
    // 画素数の上限を超えないように粗くする
    const pixels = region.width * region.height * wanted * wanted;
    const scale = kind === "part" && pixels > PART_PIXELS ? wanted * Math.sqrt(PART_PIXELS / pixels) : wanted;
    const request: UnderlayRequest = {
      underlay,
      region,
      scale,
      width: Math.max(1, Math.ceil(region.width * scale - 1e-9)),
      height: Math.max(1, Math.ceil(region.height * scale - 1e-9)),
      hiddenLayers: need.hiddenLayers,
    };
    const controller = new AbortController();
    const pending: Pending = { request, wanted, controller, layerKey: need.layerKey };
    const canvas = this.doc.createElement("canvas");
    canvas.width = request.width;
    canvas.height = request.height;
    const run = async () => {
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Canvas の 2D の描画を使えません");
      return await renderer(request, { canvas, context, signal: controller.signal });
    };
    run().then(
      (result) => {
        if (controller.signal.aborted) return;
        const raster: Raster = { request, wanted, image: result ?? canvas, layerKey: pending.layerKey };
        if (kind === "whole") {
          slot.pendingWhole = null;
          slot.whole = raster;
        } else {
          slot.pendingPart = null;
          slot.part = raster;
        }
        this.changed();
        this.finished({ underlay, request, error: null });
      },
      (error: unknown) => {
        if (controller.signal.aborted) return;
        slot.failed = true;
        slot.pendingWhole?.controller.abort();
        slot.pendingPart?.controller.abort();
        slot.pendingWhole = null;
        slot.pendingPart = null;
        this.finished({ underlay, request, error: error ?? new Error(`underlay ${underlay.id} を描けません`) });
      },
    );
    return pending;
  }
}

/** 見えている範囲に周りを足し、中身の範囲に収めた範囲 */
function partRegion(underlay: Underlay, need: Need): UnderlayRegion {
  const { x, y, width, height } = need.visible;
  const dx = width * PART_MARGIN;
  const dy = height * PART_MARGIN;
  const minX = Math.max(0, x - dx);
  const minY = Math.max(0, y - dy);
  const maxX = Math.min(underlay.width, x + width + dx);
  const maxY = Math.min(underlay.height, y + height + dy);
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
}
