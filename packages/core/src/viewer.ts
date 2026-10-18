import { type Distance, type MeasurePoint, measureDistance } from "./measure.js";
import { HitIndex, type SnapPoint } from "./pick.js";
import { fitView, isEntityVisible, renderScene, toPaper, toScreen, type View, zoomAt } from "./render.js";
import type { Scene, SceneEntity } from "./types.js";

export type ViewerMode = "select" | "measure";
export type ViewerTheme = "dark" | "light";

export interface Measurement {
  readonly start: MeasurePoint;
  /** 終点を選ぶ前は null */
  readonly end: MeasurePoint | null;
}

/** 表示と操作の状態。変わるたびに新しいオブジェクトになる（React の useSyncExternalStore にそのまま渡せる） */
export interface ViewerState {
  readonly scene: Scene;
  readonly mode: ViewerMode;
  readonly theme: ViewerTheme;
  /** 線幅を太さどおりに描くか。false なら細線 */
  readonly lineWeights: boolean;
  readonly hiddenLayers: ReadonlySet<number>;
  readonly selected: SceneEntity | null;
  readonly measurement: Measurement | null;
  /** 計測の 2 点がそろったときの距離 */
  readonly distance: Distance | null;
  /** ポインターの下の用紙の座標（mm）。図面の外では null */
  readonly cursor: readonly [number, number] | null;
  /** 表示の倍率。1 は用紙の 1 mm を画面の 1 mm（96 dpi 換算）で表示する。1% 未満は有効数字 2 桁。表示の前は 0 */
  readonly zoom: number;
}

export interface DrawingViewerOptions {
  /** 背景。省略時は図面の想定（Scene.background） */
  theme?: ViewerTheme;
  /** 図面の読み上げ用の説明（操作を受ける Canvas の aria-label） */
  label?: string;
  /** 利用者が要素を選んだとき・選択を外したとき */
  onSelect?: (entity: SceneEntity | null) => void;
}

/** 図面の上に重ねて描く処理（書き込みなど）。ctx は CSS px の座標で、view で用紙の mm と変換する */
export type OverlayPainter = (ctx: CanvasRenderingContext2D, view: Readonly<View>) => void;

type Mutable<T> = { -readonly [K in keyof T]: T[K] };

const BACKGROUNDS = { dark: "#000000", light: "#ffffff" } as const;
const HIGHLIGHTS = { dark: "#ffb000", light: "#d16d00" } as const;
const ACCENTS = { dark: "#5ad2ff", light: "#0031d8" } as const;
const PICK_PX = 6;
const SNAP_PX = 10;
/** これより動いたらクリックでなくドラッグとみなす（CSS px） */
const DRAG_PX = 4;
const KEY_PAN_PX = 48;
/** 用紙の 1 mm を画面の 1 mm にする倍率（CSS px / mm） */
const PX_PER_MM = 96 / 25.4;

function hiddenByDefault(scene: Scene): ReadonlySet<number> {
  return new Set(scene.layers.filter((layer) => !layer.visible).map((layer) => layer.id));
}

/** 図面を開いた直後の状態。レイヤは図面の表示指定に従う */
export function initialViewerState(scene: Scene, theme?: ViewerTheme): ViewerState {
  return {
    scene,
    mode: "select",
    theme: theme ?? scene.background,
    lineWeights: true,
    hiddenLayers: hiddenByDefault(scene),
    selected: null,
    measurement: null,
    distance: null,
    cursor: null,
    zoom: 0,
  };
}

/**
 * 図面の表示と操作。container の中に Canvas を 2 枚（図面と、計測・書き込みの重ね描き）作り、
 * 全体表示、拡大・縮小（ホイール・ピンチ・キー・ダブルクリック）、移動（ドラッグ・矢印キー）、
 * 要素の選択、端点に吸着する距離の計測を行う。React などの画面は getState・subscribe で状態を読む。
 */
export class DrawingViewer {
  /** 操作を受ける Canvas。フォーカスと読み上げの対象 */
  readonly canvas: HTMLCanvasElement;
  private readonly base: HTMLCanvasElement;
  private readonly win: Window;
  private state: ViewerState;
  private index: HitIndex;
  private view: View = { scale: 1, offsetX: 0, offsetY: 0 };
  private size = { width: 0, height: 0, dpr: 1 };
  /** 利用者が拡大・移動したか。動かすまでは大きさが変わるたびに全体表示にする */
  private moved = false;
  private frame = 0;
  private baseDirty = true;
  private dragging = false;
  private destroyed = false;
  private hover: SnapPoint | null = null;
  private drag: { x: number; y: number; view: View; moved: boolean; button: number } | null = null;
  private pinch: { distance: number; x: number; y: number } | null = null;
  private readonly pointers = new Map<number, { x: number; y: number }>();
  private readonly listeners = new Set<() => void>();
  private readonly overlays = new Set<OverlayPainter>();
  private readonly detachers: (() => void)[] = [];
  private readonly observer: ResizeObserver;
  private readonly onSelect: ((entity: SceneEntity | null) => void) | undefined;

  constructor(
    private readonly container: HTMLElement,
    scene: Scene,
    options: DrawingViewerOptions = {},
  ) {
    const doc = container.ownerDocument;
    this.win = doc.defaultView ?? window;
    this.state = initialViewerState(scene, options.theme);
    this.index = new HitIndex(scene);
    this.onSelect = options.onSelect;
    if (this.win.getComputedStyle(container).position === "static") container.style.position = "relative";
    this.base = doc.createElement("canvas");
    this.canvas = doc.createElement("canvas");
    for (const canvas of [this.base, this.canvas]) {
      Object.assign(canvas.style, {
        position: "absolute",
        inset: "0",
        width: "100%",
        height: "100%",
        display: "block",
      });
    }
    this.base.setAttribute("aria-hidden", "true");
    this.canvas.style.touchAction = "none";
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("role", "img");
    this.canvas.setAttribute("aria-label", options.label ?? "図面");
    container.append(this.base, this.canvas);
    this.listen();
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(container);
  }

  /** 今の状態。変わらない間は同じオブジェクトを返す */
  readonly getState = (): ViewerState => this.state;

  /** 状態が変わったときに呼ぶ関数を登録し、解除する関数を返す */
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  get scene(): Scene {
    return this.state.scene;
  }

  /** 用紙の mm と CSS px の変換（写し） */
  get viewport(): Readonly<View> {
    return { ...this.view };
  }

  /** 別の図面に替える。レイヤの表示・選択・計測を初期化し、全体表示にする */
  setScene(scene: Scene, theme?: ViewerTheme): void {
    this.index = new HitIndex(scene);
    this.hover = null;
    this.update({
      scene,
      theme: theme ?? scene.background,
      hiddenLayers: hiddenByDefault(scene),
      selected: null,
      measurement: null,
      cursor: null,
    });
    this.moved = false;
    const { width, height } = this.size;
    if (width > 0 && height > 0) this.view = fitView(scene.bounds, width, height);
    this.schedule(true);
  }

  /** 選択と計測を切り替える。計測に切り替えると、前の計測を消す */
  setMode(mode: ViewerMode): void {
    this.hover = null;
    this.update(mode === "measure" ? { mode, measurement: null } : { mode });
    this.updateCursor();
    this.schedule(false);
  }

  setTheme(theme: ViewerTheme): void {
    this.update({ theme });
    this.schedule(true);
  }

  setLineWeights(lineWeights: boolean): void {
    this.update({ lineWeights });
    this.schedule(true);
  }

  setLayersVisible(ids: Iterable<number>, visible: boolean): void {
    const next = new Set(this.state.hiddenLayers);
    for (const id of ids) {
      if (visible) next.delete(id);
      else next.add(id);
    }
    this.update({ hiddenLayers: next });
    this.schedule(true);
  }

  /** 非表示にするレイヤを置き換える（空ならすべて表示） */
  setHiddenLayers(ids: Iterable<number>): void {
    this.update({ hiddenLayers: new Set(ids) });
    this.schedule(true);
  }

  /** 要素を選ぶ（null で選択を外す）。onSelect も呼ぶ */
  select(entity: SceneEntity | null): void {
    this.update({ selected: entity });
    this.schedule(true);
    this.onSelect?.(entity);
  }

  clearMeasurement(): void {
    this.update({ measurement: null });
    this.schedule(false);
  }

  /** 図面全体を表示する */
  fit(): void {
    const { width, height } = this.size;
    if (width === 0 || height === 0) return;
    this.view = fitView(this.state.scene.bounds, width, height);
    this.moved = false;
    this.schedule(true);
  }

  /** 画面上の点（CSS px、省略時は中央）を中心に factor 倍する */
  zoomBy(factor: number, px?: number, py?: number): void {
    const { width, height } = this.size;
    this.view = zoomAt(this.view, px ?? width / 2, py ?? height / 2, factor);
    this.moved = true;
    this.schedule(true);
  }

  /** 図面を画面上で dx・dy（CSS px）だけ動かす */
  panBy(dx: number, dy: number): void {
    this.view = { ...this.view, offsetX: this.view.offsetX + dx, offsetY: this.view.offsetY + dy };
    this.moved = true;
    this.schedule(true);
  }

  /** Canvas 上の点（CSS px）を用紙の mm にする */
  toPaper(px: number, py: number): [number, number] {
    return toPaper(this.view, px, py);
  }

  /** 用紙の mm を Canvas 上の点（CSS px）にする */
  toScreen(x: number, y: number): [number, number] {
    return toScreen(this.view, x, y);
  }

  setLabel(label: string): void {
    this.canvas.setAttribute("aria-label", label);
  }

  /** 図面の上に重ねて描く処理を加え、外す関数を返す。内容を変えたら invalidate で描き直す */
  addOverlay(painter: OverlayPainter): () => void {
    this.overlays.add(painter);
    this.schedule(false);
    return () => {
      this.overlays.delete(painter);
      this.schedule(false);
    };
  }

  /** 重ね描きを描き直す */
  invalidate(): void {
    this.schedule(false);
  }

  /** Canvas と監視を外す。以後は使えない */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    if (this.frame) this.win.cancelAnimationFrame(this.frame);
    this.observer.disconnect();
    for (const detach of this.detachers) detach();
    this.base.remove();
    this.canvas.remove();
    this.listeners.clear();
    this.overlays.clear();
  }

  private update(patch: Partial<ViewerState>): void {
    const next: Mutable<ViewerState> = { ...this.state, ...patch };
    if ("measurement" in patch) {
      const measurement = patch.measurement;
      next.distance = measurement?.end ? measureDistance(measurement.start, measurement.end) : null;
    }
    this.state = next;
    for (const listener of this.listeners) listener();
  }

  private readonly visible = (entity: SceneEntity): boolean => isEntityVisible(entity, this.state.hiddenLayers);

  private schedule(base: boolean): void {
    if (this.destroyed) return;
    if (base) this.baseDirty = true;
    if (this.frame) return;
    this.frame = this.win.requestAnimationFrame(() => this.paint());
  }

  private paint(): void {
    this.frame = 0;
    if (this.destroyed) return;
    if (this.baseDirty) {
      this.baseDirty = false;
      this.drawBase();
    }
    this.drawOverlay();
    if (this.size.width > 0) {
      // 1% 以上は 1% 単位、それより小さい倍率（実寸のモデル空間を全体表示したときなど）は有効数字 2 桁
      const raw = this.view.scale / PX_PER_MM;
      const zoom = raw >= 0.01 ? Math.round(raw * 100) / 100 : Number(raw.toPrecision(2));
      if (zoom !== this.state.zoom) this.update({ zoom });
    }
  }

  /** 表示領域の大きさに合わせて Canvas の解像度を変える。利用者が動かすまでは全体表示を保つ */
  private resize(): void {
    const rect = this.container.getBoundingClientRect();
    const dpr = this.win.devicePixelRatio || 1;
    const previous = this.size;
    this.size = { width: rect.width, height: rect.height, dpr };
    for (const canvas of [this.base, this.canvas]) {
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
    }
    if (!this.moved && rect.width > 0 && rect.height > 0) {
      this.view = fitView(this.state.scene.bounds, rect.width, rect.height);
    } else if (previous.width > 0) {
      // 中心を保ったまま大きさを変える
      this.view = {
        ...this.view,
        offsetX: this.view.offsetX + (rect.width - previous.width) / 2,
        offsetY: this.view.offsetY + (rect.height - previous.height) / 2,
      };
    }
    // 解像度を変えると Canvas が消えるので、次のフレームを待たずに描き直す
    if (this.frame) this.win.cancelAnimationFrame(this.frame);
    this.baseDirty = true;
    this.paint();
  }

  private drawBase(): void {
    const ctx = this.base.getContext("2d");
    const { width, height, dpr } = this.size;
    if (!ctx || width === 0 || height === 0) return;
    const { scene, theme, hiddenLayers, lineWeights, selected } = this.state;
    renderScene(ctx, scene, this.view, width, height, {
      background: BACKGROUNDS[theme],
      invertLight: theme === "light",
      invertDark: theme === "dark",
      hiddenLayers,
      lineWeights,
      sheetFrame: true,
      devicePixelRatio: dpr,
      highlight: selected,
      highlightColor: HIGHLIGHTS[theme],
    });
  }

  private drawOverlay(): void {
    const ctx = this.canvas.getContext("2d");
    const { width, height, dpr } = this.size;
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width * dpr, height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const { measurement, mode, theme } = this.state;
    const view = this.view;
    const hover = this.hover;
    const accent = ACCENTS[theme];
    if (measurement) {
      const end = measurement.end ?? hover ?? null;
      const [sx, sy] = toScreen(view, measurement.start.x, measurement.start.y);
      ctx.strokeStyle = accent;
      ctx.fillStyle = accent;
      ctx.lineWidth = 2;
      ctx.setLineDash([6, 4]);
      if (end) {
        const [ex, ey] = toScreen(view, end.x, end.y);
        ctx.beginPath();
        ctx.moveTo(sx, sy);
        ctx.lineTo(ex, ey);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.beginPath();
        ctx.arc(ex, ey, 4, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.arc(sx, sy, 4, 0, Math.PI * 2);
      ctx.fill();
    }
    if (mode === "measure" && hover && (!measurement || measurement.end)) {
      const [hx, hy] = toScreen(view, hover.x, hover.y);
      ctx.strokeStyle = accent;
      ctx.lineWidth = 2;
      ctx.setLineDash([]);
      if (hover.kind === "vertex") ctx.strokeRect(hx - 5, hy - 5, 10, 10);
      else {
        ctx.beginPath();
        ctx.moveTo(hx - 6, hy - 6);
        ctx.lineTo(hx + 6, hy + 6);
        ctx.moveTo(hx + 6, hy - 6);
        ctx.lineTo(hx - 6, hy + 6);
        ctx.stroke();
      }
    }
    for (const painter of this.overlays) {
      ctx.save();
      painter(ctx, view);
      ctx.restore();
    }
  }

  private updateCursor(): void {
    this.canvas.style.cursor = this.dragging ? "grabbing" : this.state.mode === "measure" ? "crosshair" : "";
  }

  private local(event: MouseEvent): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }

  private listen(): void {
    const canvas = this.canvas;
    const on = <K extends keyof HTMLElementEventMap>(
      type: K,
      handler: (event: HTMLElementEventMap[K]) => void,
      options?: AddEventListenerOptions,
    ) => {
      canvas.addEventListener(type, handler, options);
      this.detachers.push(() => canvas.removeEventListener(type, handler, options));
    };
    on("pointerdown", (event) => this.pointerDown(event));
    on("pointermove", (event) => this.pointerMove(event));
    on("pointerup", (event) => this.pointerUp(event));
    on("pointercancel", (event) => this.pointerCancel(event));
    on("pointerleave", () => this.pointerLeave());
    // ホイールの既定のスクロールを止めるため、passive でない登録にする
    on("wheel", (event) => this.wheel(event), { passive: false });
    on("dblclick", (event) => {
      const [px, py] = this.local(event);
      this.zoomBy(2, px, py);
    });
    on("contextmenu", (event) => event.preventDefault());
    on("keydown", (event) => this.keyDown(event));
  }

  private pointerDown(event: PointerEvent): void {
    try {
      this.canvas.setPointerCapture(event.pointerId);
    } catch {
      // 有効でないポインター（合成したイベントなど）は捕捉しない
    }
    const [px, py] = this.local(event);
    this.pointers.set(event.pointerId, { x: px, y: py });
    if (this.pointers.size === 2) {
      const [a, b] = [...this.pointers.values()] as [{ x: number; y: number }, { x: number; y: number }];
      this.pinch = { distance: Math.hypot(a.x - b.x, a.y - b.y), x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
      this.drag = null;
      return;
    }
    this.drag = { x: px, y: py, view: this.view, moved: false, button: event.button };
  }

  private pointerMove(event: PointerEvent): void {
    const [px, py] = this.local(event);
    if (this.pointers.has(event.pointerId)) this.pointers.set(event.pointerId, { x: px, y: py });
    const pinch = this.pinch;
    if (pinch && this.pointers.size === 2) {
      const [a, b] = [...this.pointers.values()] as [{ x: number; y: number }, { x: number; y: number }];
      const distance = Math.hypot(a.x - b.x, a.y - b.y);
      const mx = (a.x + b.x) / 2;
      const my = (a.y + b.y) / 2;
      const zoomed = zoomAt(this.view, mx, my, distance / Math.max(pinch.distance, 1));
      this.view = { ...zoomed, offsetX: zoomed.offsetX + mx - pinch.x, offsetY: zoomed.offsetY + my - pinch.y };
      this.pinch = { distance, x: mx, y: my };
      this.moved = true;
      this.schedule(true);
      return;
    }
    const drag = this.drag;
    if (drag) {
      const dx = px - drag.x;
      const dy = py - drag.y;
      // 計測中は、中ボタン・右ボタンのドラッグだけで移動する
      const panning = this.state.mode === "select" || drag.button === 1 || drag.button === 2;
      if (!drag.moved && Math.hypot(dx, dy) > DRAG_PX && panning) {
        drag.moved = true;
        this.dragging = true;
        this.updateCursor();
      }
      if (drag.moved) {
        this.view = { ...drag.view, offsetX: drag.view.offsetX + dx, offsetY: drag.view.offsetY + dy };
        this.moved = true;
        this.schedule(true);
        return;
      }
    }
    const [x, y] = toPaper(this.view, px, py);
    this.update({ cursor: [x, y] });
    if (this.state.mode === "measure") {
      this.hover = this.index.snap(x, y, SNAP_PX / this.view.scale, this.visible);
      this.schedule(false);
    }
  }

  private endPointer(event: PointerEvent): { x: number; y: number; moved: boolean; button: number } | null {
    this.pointers.delete(event.pointerId);
    if (this.pointers.size < 2) this.pinch = null;
    const drag = this.drag;
    this.drag = null;
    if (this.dragging) {
      this.dragging = false;
      this.updateCursor();
    }
    return drag;
  }

  private pointerUp(event: PointerEvent): void {
    const drag = this.endPointer(event);
    if (!drag || drag.moved || drag.button !== 0) return;
    const [px, py] = this.local(event);
    const [x, y] = toPaper(this.view, px, py);
    if (this.state.mode === "measure") {
      const snapped = this.index.snap(x, y, SNAP_PX / this.view.scale, this.visible);
      const point: MeasurePoint = snapped ? { x: snapped.x, y: snapped.y, entity: snapped.entity } : { x, y };
      const current = this.state.measurement;
      this.update({ measurement: !current || current.end ? { start: point, end: null } : { ...current, end: point } });
      this.schedule(false);
      return;
    }
    this.select(this.index.pick(x, y, PICK_PX / this.view.scale, this.visible));
  }

  /** スクロールなどで取り消されたポインターは、クリックとして扱わない */
  private pointerCancel(event: PointerEvent): void {
    this.endPointer(event);
  }

  private pointerLeave(): void {
    this.update({ cursor: null });
    if (this.state.mode === "measure") {
      this.hover = null;
      this.schedule(false);
    }
  }

  private wheel(event: WheelEvent): void {
    event.preventDefault();
    const [px, py] = this.local(event);
    const delta = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
    this.zoomBy(Math.exp(-delta * (event.ctrlKey ? 0.01 : 0.0015)), px, py);
  }

  private keyDown(event: KeyboardEvent): void {
    // ブラウザの拡大（Ctrl・Cmd と +）などのショートカットは妨げない
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    switch (event.key) {
      case "+":
      case "=":
        this.zoomBy(1.25);
        break;
      case "-":
        this.zoomBy(0.8);
        break;
      case "0":
        this.fit();
        break;
      case "ArrowLeft":
        this.panBy(KEY_PAN_PX, 0);
        break;
      case "ArrowRight":
        this.panBy(-KEY_PAN_PX, 0);
        break;
      case "ArrowUp":
        this.panBy(0, KEY_PAN_PX);
        break;
      case "ArrowDown":
        this.panBy(0, -KEY_PAN_PX);
        break;
      case "Escape":
        this.clearMeasurement();
        this.select(null);
        break;
      default:
        return;
    }
    event.preventDefault();
  }
}
