import {
  type Box,
  type Distance,
  isEntityVisible,
  type MeasurePoint,
  measureDistance,
  type Scene,
  type SceneEntity,
} from "@zumenjs/core";
import { HitIndex, type SnapPoint } from "./pick.js";
import {
  centeredView,
  fitView,
  type HighlightLayer,
  renderHighlights,
  renderScene,
  toPaper,
  toScreen,
  type View,
  visibleBounds,
  zoomAt,
} from "./render.js";
import {
  cssProperty,
  DEFAULT_STYLE,
  lightness,
  mergePatch,
  mergeStyle,
  readStyle,
  type ViewerBackground,
  type ViewerPalette,
  type ViewerStyle,
  type ViewerStylePatch,
} from "./style.js";
import { measureTool, panTool, selectTool, type ToolPointerEvent, type ViewerTool } from "./tools.js";

export interface Measurement {
  readonly start: MeasurePoint;
  /** 終点を選ぶ前は null */
  readonly end: MeasurePoint | null;
}

/**
 * 表示と操作の状態。変わるたびに新しいオブジェクトになる（React の useSyncExternalStore にそのまま渡せる）。
 * ポインターの速さで変わるもの（表示範囲、ポインターの下の用紙の座標）は持たず、イベント（move・cursor）で伝える
 */
export interface ViewerState {
  readonly scene: Scene;
  /** 背景。色は見た目（ViewerStyle）の同じ名前の組 */
  readonly background: ViewerBackground;
  /** 線幅を太さどおりに描くか。false なら細線 */
  readonly lineWeights: boolean;
  readonly hiddenLayers: ReadonlySet<number>;
  /** 選んでいる図形（選んだ順） */
  readonly selection: readonly SceneEntity[];
  /** ポインターの下の図形（選択の道具が決める） */
  readonly hovered: SceneEntity | null;
  /** 名前付きの強調（検索結果など）。名前を加えた順 */
  readonly highlights: ReadonlyMap<string, readonly SceneEntity[]>;
  /** 使用中の道具の名前 */
  readonly tool: string;
  readonly measurement: Measurement | null;
  /** 計測の 2 点がそろったときの距離 */
  readonly distance: Distance | null;
}

/** 表示範囲。jumpTo に渡し、getView で読む */
export interface Camera {
  /** 表示領域の中央に来る用紙の点（mm） */
  readonly center: readonly [number, number];
  /** 倍率。1 は用紙の 1 mm を画面の 1 mm（96 dpi 換算）で表示する */
  readonly zoom: number;
  /** 画面の上で図面を時計回りに回す角度（度、0 以上 360 未満）。省略すると 0（jumpTo では今のまま） */
  readonly rotation?: number;
}

/** 今の表示範囲 */
export interface ViewerView extends Camera {
  readonly rotation: number;
  /** 見えている用紙の範囲（mm）。回しているときは、見えている四角形を囲む範囲 */
  readonly bounds: Box;
  /** 表示領域の大きさ（CSS px）。要素に大きさが付く前は 0 */
  readonly width: number;
  readonly height: number;
}

export interface FitOptions {
  /** 表示領域の縁からの余白（CSS px）。省略すると見た目の fitPadding */
  padding?: number;
}

export interface QueryOptions {
  /** 点からの距離の上限（CSS px） */
  tolerance?: number;
}

/** 操作のハンドラ。1 つずつ止められる */
export interface ViewerHandlers {
  /** ドラッグで図面を動かす。中ボタン・右ボタンはいつも、左ボタンとタッチは道具が許すとき（ViewerTool.dragPan） */
  dragPan: boolean;
  /**
   * ホイールで拡大・縮小する。"modifier" は Ctrl・⌘ を押しているとき（トラックパッドのピンチを含む）だけ拡大し、
   * それ以外はページをスクロールさせる
   */
  wheel: "always" | "modifier" | "never";
  /** 2 本の指で拡大・縮小し、動かす */
  pinch: boolean;
  /** ダブルクリックで 2 倍にする */
  doubleClick: boolean;
  /** キーの操作。+ と - で拡大・縮小、0 で全体表示、矢印で移動、Esc で選択と計測を消す */
  keyboard: boolean;
}

/** 表示範囲のイベント。originalEvent は利用者の操作による変化のときの元のイベント（メソッドで変えたときは null） */
export interface ViewEvent {
  readonly view: ViewerView;
  readonly originalEvent: Event | null;
}

/** ポインターの下の用紙の座標（mm）。図面の外へ出たら null */
export interface CursorEvent {
  readonly point: readonly [number, number] | null;
}

export interface ViewerEventMap {
  /** 表示範囲が変わるたび（ドラッグ・ホイール・ピンチの間は何度も） */
  move: ViewEvent;
  /** 表示範囲の変化が終わったとき（ドラッグを離した、ホイールが止まった、メソッドで変えた） */
  moveend: ViewEvent;
  /** ポインターが動いたとき。図面を動かしている間は送らない（ポインターの下の点は変わらない） */
  cursor: CursorEvent;
}

export interface DrawingViewerOptions {
  /** 背景。省略すると図面の想定（Scene.background）。setScene で図面を替えたときもこの値を使う */
  background?: ViewerBackground;
  /** 操作を受ける Canvas の読み上げ用の説明（aria-label）。文言は利用側が決める */
  label?: string;
  /** 操作のハンドラ。省略したものは有効（ホイールは "always"） */
  handlers?: Partial<ViewerHandlers>;
  /** 見た目の上書き。CSS のカスタムプロパティより優先する */
  style?: ViewerStylePatch;
  /** 利用側の道具。組み込みの select・measure・pan に加える（同じ名前なら置き換える） */
  tools?: readonly ViewerTool[];
  /** 最初に使う道具の名前。省略すると "select" */
  tool?: string;
}

/** 図面の上に重ねて描く処理（書き込みなど）。ctx は CSS px の座標で、view で用紙の mm と変換する */
export type OverlayPainter = (ctx: CanvasRenderingContext2D, view: Readonly<View>) => void;

type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Listener<K extends keyof ViewerEventMap> = (event: ViewerEventMap[K]) => void;
interface InternalCamera {
  cx: number;
  cy: number;
  zoom: number;
  /** 時計回りの角度（度、0 以上 360 未満） */
  rotation: number;
}

/** 角度を 0 以上 360 未満にする */
function normalizeRotation(degrees: number): number {
  const value = ((degrees % 360) + 360) % 360;
  return Math.abs(value - 360) < 1e-9 ? 0 : value;
}

const BUILT_IN_TOOLS: Readonly<Record<string, () => ViewerTool>> = {
  select: selectTool,
  measure: measureTool,
  pan: panTool,
};
const DEFAULT_HANDLERS: ViewerHandlers = {
  dragPan: true,
  wheel: "always",
  pinch: true,
  doubleClick: true,
  keyboard: true,
};
/** 図形を選ぶ距離（CSS px） */
const PICK_PX = 6;
/** 計測で吸着する距離（CSS px） */
const SNAP_PX = 10;
/** これより動いたらクリックでなくドラッグとみなす（CSS px） */
const DRAG_PX = 4;
const KEY_PAN_PX = 48;
/** ホイールが止まってから moveend を送るまで（ms） */
const WHEEL_END_MS = 200;
/** 用紙の 1 mm を画面の 1 mm にする倍率（CSS px / mm） */
const PX_PER_MM = 96 / 25.4;
/** 拡大の範囲（CSS px / mm）。描画の zoomAt と同じ */
const MIN_SCALE = 1e-6;
const MAX_SCALE = 1e6;

function hiddenByDefault(scene: Scene): ReadonlySet<number> {
  return new Set(scene.layers.filter((layer) => !layer.visible).map((layer) => layer.id));
}

function isEmpty(box: Box): boolean {
  return !(box.minX <= box.maxX && box.minY <= box.maxY && Number.isFinite(box.maxX - box.minX + box.maxY - box.minY));
}

function sameEntities(a: readonly SceneEntity[], b: readonly SceneEntity[]): boolean {
  return a.length === b.length && a.every((entity, index) => entity === b[index]);
}

/**
 * 2D の図面の表示と操作のエンジン。container の中に Canvas を 2 枚（図面と、道具・重ね描き）作る。
 * 表示範囲は jumpTo・fitBounds・zoomBy・panBy で変え、getView と move・moveend のイベントで読む。
 * 状態（図面、レイヤ、選択、強調、道具、計測）は getState・subscribe で読む。
 * 利用者の操作は、操作のハンドラ（ドラッグ・ホイール・ピンチ・ダブルクリック・キー）と、使用中の道具が受ける。
 * 色は container の CSS のカスタムプロパティ（--zumen-*）から読み、setStyle で上書きする。画面の文言は持たない
 */
export class DrawingViewer {
  /** 操作を受ける Canvas。フォーカスと読み上げの対象 */
  readonly canvas: HTMLCanvasElement;
  private readonly base: HTMLCanvasElement;
  private readonly win: Window;
  private readonly options: DrawingViewerOptions;
  private state: ViewerState;
  private index: HitIndex;
  private camera: InternalCamera;
  private size = { width: 0, height: 0, dpr: 1 };
  /** 大きさが変わるたびに合わせる範囲。利用者やメソッドが表示範囲を変えるまで続く */
  private fitTarget: { box: Box; padding: number | undefined } | null = null;
  private handlers: ViewerHandlers;
  private styleOverrides: ViewerStylePatch;
  private style: ViewerStyle = DEFAULT_STYLE;
  /** 背景ごとの明るさ（0〜1）。背景と見分けにくい色の反転を決める */
  private lightness = { dark: 0, light: 1 };
  /** CSS から読んだ、名前付きの強調ごとの色（背景:名前 → 色、指定がなければ空文字） */
  private readonly highlightColors = new Map<string, string>();
  private readonly probe: CanvasRenderingContext2D | null;
  private readonly tools = new Map<string, ViewerTool>();
  private tool: ViewerTool;
  private frame = 0;
  private baseDirty = true;
  private destroyed = false;
  private panning = false;
  private wheelTimer: ReturnType<typeof setTimeout> | undefined;
  private drag: {
    x: number;
    y: number;
    camera: InternalCamera;
    moved: boolean;
    pan: boolean;
  } | null = null;
  private pinch: { distance: number; x: number; y: number; moved: boolean } | null = null;
  private readonly pointers = new Map<number, { x: number; y: number }>();
  private readonly listeners = new Set<() => void>();
  private readonly events: { [K in keyof ViewerEventMap]: Set<Listener<K>> } = {
    move: new Set(),
    moveend: new Set(),
    cursor: new Set(),
  };
  private readonly overlays = new Set<OverlayPainter>();
  private readonly detachers: (() => void)[] = [];
  private readonly observer: ResizeObserver;

  constructor(
    private readonly container: HTMLElement,
    scene: Scene,
    options: DrawingViewerOptions = {},
  ) {
    const doc = container.ownerDocument;
    this.win = doc.defaultView ?? window;
    this.options = options;
    this.handlers = { ...DEFAULT_HANDLERS, ...options.handlers };
    this.styleOverrides = options.style ?? {};
    for (const create of Object.values(BUILT_IN_TOOLS)) {
      const tool = create();
      this.tools.set(tool.name, tool);
    }
    for (const tool of options.tools ?? []) this.tools.set(tool.name, tool);
    const tool = this.tools.get(options.tool ?? "select");
    if (!tool) throw new Error(`道具 ${options.tool} がありません`);
    this.tool = tool;
    this.state = {
      scene,
      background: options.background ?? scene.background,
      lineWeights: true,
      hiddenLayers: hiddenByDefault(scene),
      selection: [],
      hovered: null,
      highlights: new Map(),
      tool: tool.name,
      measurement: null,
      distance: null,
    };
    this.index = new HitIndex(scene);
    this.fitTarget = isEmpty(scene.bounds) ? null : { box: scene.bounds, padding: undefined };
    this.camera = this.initialCamera(scene.bounds);
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
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("role", "img");
    if (options.label !== undefined) this.canvas.setAttribute("aria-label", options.label);
    const probe = doc.createElement("canvas");
    probe.width = 1;
    probe.height = 1;
    this.probe = probe.getContext("2d", { willReadFrequently: true });
    container.append(this.base, this.canvas);
    this.readStyle();
    this.updateCursor();
    this.listen();
    this.tool.activate?.(this);
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(container);
  }

  // ---- 状態

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

  /** 別の図面に替える。レイヤの表示・選択・強調・計測を初期化し、全体を表示する。道具はそのまま */
  setScene(scene: Scene): void {
    this.tool.cancel?.(this);
    this.index = new HitIndex(scene);
    this.update({
      scene,
      background: this.options.background ?? scene.background,
      hiddenLayers: hiddenByDefault(scene),
      selection: [],
      hovered: null,
      highlights: new Map(),
      measurement: null,
    });
    this.schedule(true);
    if (isEmpty(scene.bounds)) {
      this.fitTarget = null;
      this.setCamera(this.initialCamera(scene.bounds, this.camera.rotation), true, null);
    } else this.fitTo(scene.bounds, undefined, true, null);
  }

  setBackground(background: ViewerBackground): void {
    if (background === this.state.background) return;
    this.update({ background });
    this.schedule(true);
  }

  setLineWeights(lineWeights: boolean): void {
    if (lineWeights === this.state.lineWeights) return;
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

  /** 選択を置き換える（空なら選択を外す） */
  select(entities: Iterable<SceneEntity>): void {
    const selection = [...new Set(entities)];
    if (sameEntities(selection, this.state.selection)) return;
    this.update({ selection });
    this.schedule(true);
  }

  /** ポインターの下の図形を決める。選択の道具が使い、見た目の hover の色があれば重ね描きする */
  setHovered(entity: SceneEntity | null): void {
    if (entity === this.state.hovered) return;
    this.update({ hovered: entity });
    this.schedule(false);
  }

  /** 名前付きの強調（検索結果など）を置き換える。空なら外す。色は見た目の highlights[名前]、なければ highlight */
  setHighlight(name: string, entities: Iterable<SceneEntity>): void {
    const list = [...new Set(entities)];
    const current = this.state.highlights.get(name);
    if (current ? sameEntities(current, list) : list.length === 0) return;
    const highlights = new Map(this.state.highlights);
    if (list.length === 0) highlights.delete(name);
    else highlights.set(name, list);
    this.update({ highlights });
    this.schedule(true);
  }

  /** 計測を置き換える（null で消す）。距離（distance）も計算し直す。計測の道具が使う */
  setMeasurement(measurement: Measurement | null): void {
    if (measurement === this.state.measurement) return;
    this.update({ measurement });
    this.schedule(false);
  }

  // ---- 道具

  /** 道具を替える。組み込みは "select"・"measure"・"pan"、ほかは addTool で加えたもの */
  setTool(name: string): void {
    const next = this.tools.get(name);
    if (!next) throw new Error(`道具 ${name} がありません`);
    if (next !== this.tool) this.swapTool(next);
  }

  /** 道具を加え、外す関数を返す。同じ名前の道具は置き換える。使用中の道具を外すと "select" に戻る */
  addTool(tool: ViewerTool): () => void {
    const replaced = this.tools.get(tool.name);
    this.tools.set(tool.name, tool);
    if (replaced === this.tool) this.swapTool(tool);
    return () => {
      if (this.tools.get(tool.name) !== tool) return;
      // 組み込みの道具を置き換えていたら、組み込みに戻す
      const builtIn = BUILT_IN_TOOLS[tool.name];
      if (builtIn) this.tools.set(tool.name, builtIn());
      else this.tools.delete(tool.name);
      if (this.tool === tool) this.swapTool(this.tools.get(builtIn ? tool.name : "select") as ViewerTool);
    };
  }

  // ---- 表示範囲

  /** 今の表示範囲 */
  getView(): ViewerView {
    const { cx, cy, zoom, rotation } = this.camera;
    const { width, height } = this.size;
    return { center: [cx, cy], zoom, rotation, bounds: visibleBounds(this.transform(), width, height), width, height };
  }

  /** 中央の点・倍率・角度を変える（省略したものは今のまま） */
  jumpTo(camera: Partial<Camera>): void {
    this.fitTarget = null;
    const [cx, cy] = camera.center ?? [this.camera.cx, this.camera.cy];
    this.setCamera(
      {
        cx,
        cy,
        zoom: camera.zoom ?? this.camera.zoom,
        rotation: normalizeRotation(camera.rotation ?? this.camera.rotation),
      },
      true,
      null,
    );
  }

  /**
   * 図面を表示領域の中央の周りに、画面の上で時計回りに degrees 度回す（負なら反時計回り）。
   * fit・fitBounds で収めた範囲を表示しているときは、回した向きで収め直す
   */
  rotateBy(degrees: number): void {
    const rotation = normalizeRotation(this.camera.rotation + degrees);
    if (!Number.isFinite(rotation)) return;
    if (this.fitTarget) {
      // 元の角度を残して setCamera に渡す。倍率が変わらない回転でも、再描画と変更の通知が必要。
      this.setCamera(this.fitCamera(rotation) ?? { ...this.camera, rotation }, true, null);
      return;
    }
    this.setCamera({ ...this.camera, rotation }, true, null);
  }

  /**
   * 用紙の範囲（mm）を、余白を残して表示領域に収める。表示領域の大きさが変わっても、利用者やメソッドが
   * 表示範囲を変えるまでは同じ範囲を収め直す。空の範囲は何もしない
   */
  fitBounds(box: Box, options: FitOptions = {}): void {
    this.fitTo(box, options.padding, true, null);
  }

  /** 図面全体を表示する */
  fit(): void {
    this.fitTo(this.state.scene.bounds, undefined, true, null);
  }

  /** 表示領域の点（CSS px、省略時は中央）を中心に factor 倍する */
  zoomBy(factor: number, px?: number, py?: number): void {
    this.zoomAround(factor, px, py, true, null);
  }

  /** 図面を表示領域の上で dx・dy（CSS px）だけ動かす */
  panBy(dx: number, dy: number): void {
    this.panFrom(this.camera, dx, dy, true, null);
  }

  /** 表示領域の点（CSS px）を用紙の mm にする */
  toPaper(px: number, py: number): [number, number] {
    return toPaper(this.transform(), px, py);
  }

  /** 用紙の mm を表示領域の点（CSS px）にする */
  toScreen(x: number, y: number): [number, number] {
    return toScreen(this.transform(), x, y);
  }

  // ---- 問い合わせ（入力を自分で扱う利用側と道具のため）

  /** 用紙の点 (x, y) から tolerance（CSS px、既定 6）以内の見えている図形。近い順で、同じ距離なら上に描いたものが先 */
  entitiesAt(x: number, y: number, options: QueryOptions = {}): SceneEntity[] {
    return this.index.pickAll(x, y, (options.tolerance ?? PICK_PX) / this.transform().scale, this.visible);
  }

  /** 外接矩形が用紙の範囲 box（mm）と重なる、見えている図形（描く順） */
  entitiesIn(box: Box): SceneEntity[] {
    return this.index.query(box, this.visible);
  }

  /** 用紙の点 (x, y) から tolerance（CSS px、既定 10）以内の吸着点。端点・頂点を優先し、なければ線上の最も近い点 */
  snapAt(x: number, y: number, options: QueryOptions = {}): SnapPoint | null {
    return this.index.snap(x, y, (options.tolerance ?? SNAP_PX) / this.transform().scale, this.visible);
  }

  // ---- イベント

  /** イベントを受ける関数を登録し、解除する関数を返す */
  on<K extends keyof ViewerEventMap>(type: K, listener: (event: ViewerEventMap[K]) => void): () => void {
    const set = this.events[type] as Set<Listener<K>>;
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  }

  // ---- 操作のハンドラ

  getHandlers(): ViewerHandlers {
    return { ...this.handlers };
  }

  /** 操作のハンドラを変える（渡したものだけ） */
  setHandlers(patch: Partial<ViewerHandlers>): void {
    this.handlers = { ...this.handlers, ...patch };
    this.updateCursor();
  }

  // ---- 見た目

  /** 今の見た目（CSS のカスタムプロパティに setStyle の上書きを重ねたもの） */
  getStyle(): ViewerStyle {
    return this.style;
  }

  /** 今の背景の色の組 */
  getPalette(): ViewerPalette {
    return this.style[this.state.background];
  }

  /** 見た目を上書きする（渡した項目だけ）。CSS のカスタムプロパティより優先する */
  setStyle(patch: ViewerStylePatch): void {
    this.styleOverrides = mergePatch(this.styleOverrides, patch);
    this.refreshStyle();
  }

  /**
   * CSS のカスタムプロパティを読み直して描き直す。作ったとき、大きさが変わったとき、OS の明暗の設定が変わったときは
   * 自動で読む。利用側がクラスなどで配色を替えたときに呼ぶ
   */
  refreshStyle(): void {
    this.readStyle();
    this.schedule(true);
    // 余白（fitPadding）が変わったら、fitBounds の範囲を収め直す
    const camera = this.fitCamera();
    if (camera) this.setCamera(camera, true, null);
  }

  // ---- 重ね描き・その他

  /** 操作を受ける Canvas の読み上げ用の説明。null で外す */
  setLabel(label: string | null): void {
    if (label === null) this.canvas.removeAttribute("aria-label");
    else this.canvas.setAttribute("aria-label", label);
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

  /** 重ね描き（道具・addOverlay）を描き直す */
  invalidate(): void {
    this.schedule(false);
  }

  /** Canvas と監視を外す。以後は使えない */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.listeners.clear();
    for (const set of Object.values(this.events)) set.clear();
    this.tool.deactivate?.(this);
    if (this.frame) this.win.cancelAnimationFrame(this.frame);
    clearTimeout(this.wheelTimer);
    this.observer.disconnect();
    for (const detach of this.detachers) detach();
    this.base.remove();
    this.canvas.remove();
    this.overlays.clear();
  }

  // ---- 内部

  private update(patch: Partial<ViewerState>): void {
    const next: Mutable<ViewerState> = { ...this.state, ...patch };
    if ("measurement" in patch) {
      const measurement = patch.measurement;
      next.distance = measurement?.end ? measureDistance(measurement.start, measurement.end) : null;
    }
    this.state = next;
    for (const listener of [...this.listeners]) listener();
  }

  private emit<K extends keyof ViewerEventMap>(type: K, event: ViewerEventMap[K]): void {
    for (const listener of [...(this.events[type] as Set<Listener<K>>)]) listener(event);
  }

  private readonly visible = (entity: SceneEntity): boolean => isEntityVisible(entity, this.state.hiddenLayers);

  /** 用紙の mm から CSS px への変換 */
  private transform(camera: InternalCamera = this.camera): View {
    const { width, height } = this.size;
    return centeredView(camera.cx, camera.cy, camera.zoom * PX_PER_MM, width, height, camera.rotation);
  }

  private fromTransform(view: View): InternalCamera {
    const [cx, cy] = toPaper(view, this.size.width / 2, this.size.height / 2);
    return { cx, cy, zoom: view.scale / PX_PER_MM, rotation: normalizeRotation(view.rotation ?? 0) };
  }

  /** 大きさが分かる前の表示範囲。範囲の中央を倍率 1 で示す */
  private initialCamera(box: Box, rotation = 0): InternalCamera {
    return isEmpty(box)
      ? { cx: 0, cy: 0, zoom: 1, rotation }
      : { cx: (box.minX + box.maxX) / 2, cy: (box.minY + box.maxY) / 2, zoom: 1, rotation };
  }

  /** 表示範囲を変え、変わったら move（end なら moveend も）を送る */
  private setCamera(next: InternalCamera, end: boolean, originalEvent: Event | null): void {
    const scale = Math.min(Math.max(next.zoom * PX_PER_MM, MIN_SCALE), MAX_SCALE);
    const camera = {
      cx: next.cx,
      cy: next.cy,
      zoom: scale === next.zoom * PX_PER_MM ? next.zoom : scale / PX_PER_MM,
      rotation: next.rotation,
    };
    if (
      !Number.isFinite(camera.cx) ||
      !Number.isFinite(camera.cy) ||
      !Number.isFinite(camera.zoom) ||
      !Number.isFinite(camera.rotation)
    )
      return;
    const current = this.camera;
    if (
      camera.cx === current.cx &&
      camera.cy === current.cy &&
      camera.zoom === current.zoom &&
      camera.rotation === current.rotation
    )
      return;
    this.camera = camera;
    this.schedule(true);
    if (this.size.width === 0 || this.size.height === 0) return;
    const view = this.getView();
    this.emit("move", { view, originalEvent });
    if (end) this.emit("moveend", { view, originalEvent });
  }

  private emitMoveEnd(originalEvent: Event | null): void {
    this.emit("moveend", { view: this.getView(), originalEvent });
  }

  /** 範囲を収める。大きさが分かる前は、分かったときに収める */
  private fitTo(box: Box, padding: number | undefined, end: boolean, originalEvent: Event | null): void {
    if (isEmpty(box)) return;
    this.fitTarget = { box, padding };
    const camera = this.fitCamera();
    if (camera) this.setCamera(camera, end, originalEvent);
  }

  /** fitBounds の範囲を今の大きさに収める表示範囲。範囲がないか、大きさが分からなければ null */
  private fitCamera(rotation = this.camera.rotation): InternalCamera | null {
    const target = this.fitTarget;
    const { width, height } = this.size;
    if (!target || width === 0 || height === 0) return null;
    return this.fromTransform(fitView(target.box, width, height, target.padding ?? this.style.fitPadding, rotation));
  }

  private swapTool(next: ViewerTool): void {
    this.tool.deactivate?.(this);
    this.tool = next;
    this.update({ tool: next.name });
    this.updateCursor();
    next.activate?.(this);
    this.schedule(false);
  }

  private zoomAround(
    factor: number,
    px: number | undefined,
    py: number | undefined,
    end: boolean,
    originalEvent: Event | null,
  ): void {
    const { width, height } = this.size;
    this.fitTarget = null;
    const view = zoomAt(this.transform(), px ?? width / 2, py ?? height / 2, factor);
    this.setCamera(this.fromTransform(view), end, originalEvent);
  }

  private panFrom(from: InternalCamera, dx: number, dy: number, end: boolean, originalEvent: Event | null): void {
    this.fitTarget = null;
    // 画面で dx・dy 動かすと、中央に来る用紙の点は表示領域の (中央 − dx, 中央 − dy) の点になる
    const [cx, cy] = toPaper(this.transform(from), this.size.width / 2 - dx, this.size.height / 2 - dy);
    this.setCamera({ ...from, cx, cy }, end, originalEvent);
  }

  private readStyle(): void {
    const computed = this.win.getComputedStyle(this.container);
    this.style = mergeStyle(readStyle(computed), this.styleOverrides);
    this.highlightColors.clear();
    if (this.probe) {
      this.lightness = {
        dark: lightness(this.probe, this.style.dark.background),
        light: lightness(this.probe, this.style.light.background),
      };
    }
  }

  /** 名前付きの強調の色。setStyle の上書き、CSS の --zumen-highlight-<名前>-<背景>、highlight の順に探す */
  private highlightColor(name: string): string {
    const background = this.state.background;
    const palette = this.style[background];
    const own = palette.highlights[name];
    if (own) return own;
    const key = `${background}:${name}`;
    let css = this.highlightColors.get(key);
    if (css === undefined) {
      css = cssProperty(this.win.getComputedStyle(this.container), `--zumen-highlight-${name}-${background}`) ?? "";
      this.highlightColors.set(key, css);
    }
    return css || palette.highlight;
  }

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
  }

  /** 表示領域の大きさに合わせて Canvas の解像度を変える。fitBounds の範囲があれば収め直し、なければ中央を保つ */
  private resize(): void {
    const rect = this.container.getBoundingClientRect();
    const dpr = this.win.devicePixelRatio || 1;
    const previous = this.size;
    this.size = { width: rect.width, height: rect.height, dpr };
    for (const canvas of [this.base, this.canvas]) {
      canvas.width = Math.max(1, Math.round(rect.width * dpr));
      canvas.height = Math.max(1, Math.round(rect.height * dpr));
    }
    this.readStyle();
    const before = this.camera;
    this.camera = this.fitCamera() ?? before;
    // 解像度を変えると Canvas が消えるので、次のフレームを待たずに描き直す
    if (this.frame) this.win.cancelAnimationFrame(this.frame);
    this.frame = 0;
    this.baseDirty = true;
    this.paint();
    const changed =
      before.cx !== this.camera.cx ||
      before.cy !== this.camera.cy ||
      before.zoom !== this.camera.zoom ||
      before.rotation !== this.camera.rotation ||
      previous.width !== rect.width ||
      previous.height !== rect.height;
    if (changed && rect.width > 0 && rect.height > 0) {
      const view = this.getView();
      this.emit("move", { view, originalEvent: null });
      this.emit("moveend", { view, originalEvent: null });
    }
  }

  private drawBase(): void {
    const ctx = this.base.getContext("2d");
    const { width, height, dpr } = this.size;
    if (!ctx || width === 0 || height === 0) return;
    const { scene, background, hiddenLayers, lineWeights, selection, highlights } = this.state;
    const palette = this.style[background];
    const invertLight = this.lightness[background] > 0.5;
    const layers: HighlightLayer[] = [];
    for (const [name, entities] of highlights) layers.push({ entities, color: this.highlightColor(name) });
    if (selection.length > 0) layers.push({ entities: selection, color: palette.selection });
    renderScene(ctx, scene, this.transform(), width, height, {
      background: palette.background,
      invertLight,
      invertDark: !invertLight,
      hiddenLayers,
      lineWeights,
      sheetFrame: palette.sheetFrame !== null,
      ...(palette.sheetFrame === null ? {} : { sheetFrameColor: palette.sheetFrame }),
      devicePixelRatio: dpr,
      highlights: layers,
    });
  }

  private drawOverlay(): void {
    const ctx = this.canvas.getContext("2d");
    const { width, height, dpr } = this.size;
    if (!ctx) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, width * dpr, height * dpr);
    const view = this.transform();
    const { hovered, measurement, background, lineWeights } = this.state;
    const palette = this.style[background];
    if (hovered && palette.hover) {
      renderHighlights(ctx, [{ entities: [hovered], color: palette.hover }], view, width, height, {
        background: palette.background,
        lineWeights,
        devicePixelRatio: dpr,
      });
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (measurement) {
      const [sx, sy] = toScreen(view, measurement.start.x, measurement.start.y);
      ctx.strokeStyle = palette.measure;
      ctx.fillStyle = palette.measure;
      ctx.lineWidth = 2;
      if (measurement.end) {
        const [ex, ey] = toScreen(view, measurement.end.x, measurement.end.y);
        ctx.setLineDash([6, 4]);
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
    if (this.tool.draw) {
      ctx.save();
      this.tool.draw(ctx, this);
      ctx.restore();
    }
    for (const painter of this.overlays) {
      ctx.save();
      painter(ctx, view);
      ctx.restore();
    }
  }

  private updateCursor(): void {
    this.canvas.style.cursor = this.panning ? "grabbing" : (this.tool.cursor ?? "");
    // ページのスクロール・拡大をブラウザに任せるのは、図面を指で動かさず、道具もドラッグを受けないとき
    const takesDrag = this.handlers.dragPan || this.tool.dragPan === false;
    this.canvas.style.touchAction = takesDrag ? "none" : this.handlers.pinch ? "pan-x pan-y" : "auto";
  }

  private local(event: MouseEvent): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }

  private toolEvent(event: PointerEvent, px: number, py: number): ToolPointerEvent {
    const [x, y] = toPaper(this.transform(), px, py);
    return { x, y, px, py, event };
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
    // ホイールの既定のスクロールを止めることがあるので、passive でない登録にする
    on("wheel", (event) => this.wheel(event), { passive: false });
    on("dblclick", (event) => {
      if (!this.handlers.doubleClick) return;
      const [px, py] = this.local(event);
      this.zoomAround(2, px, py, true, event);
    });
    on("contextmenu", (event) => event.preventDefault());
    on("keydown", (event) => this.keyDown(event));
    // OS の明暗の設定が変わったら、CSS のカスタムプロパティ（prefers-color-scheme で替えるもの）を読み直す
    const scheme = this.win.matchMedia?.("(prefers-color-scheme: dark)");
    if (scheme) {
      const changed = () => this.refreshStyle();
      scheme.addEventListener("change", changed);
      this.detachers.push(() => scheme.removeEventListener("change", changed));
    }
  }

  private pointerDown(event: PointerEvent): void {
    try {
      this.canvas.setPointerCapture(event.pointerId);
    } catch {
      // 有効でないポインター（合成したイベントなど）は捕捉しない
    }
    const [px, py] = this.local(event);
    this.pointers.set(event.pointerId, { x: px, y: py });
    if (this.pointers.size >= 2) {
      // 2 本目の指が触れたら、1 本目の操作をやめて 2 本指の操作にする
      if (this.drag) {
        this.drag = null;
        this.endPan(event);
        this.tool.cancel?.(this);
      }
      if (this.pointers.size === 2 && this.handlers.pinch) {
        const [a, b] = [...this.pointers.values()] as [{ x: number; y: number }, { x: number; y: number }];
        this.pinch = {
          distance: Math.hypot(a.x - b.x, a.y - b.y),
          x: (a.x + b.x) / 2,
          y: (a.y + b.y) / 2,
          moved: false,
        };
      }
      return;
    }
    const pan =
      this.handlers.dragPan &&
      (event.button === 1 || event.button === 2 || (event.button === 0 && this.tool.dragPan !== false));
    this.drag = { x: px, y: py, camera: this.camera, moved: false, pan };
    this.tool.pointerDown?.(this.toolEvent(event, px, py), this);
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
      const zoomed = zoomAt(this.transform(), mx, my, distance / Math.max(pinch.distance, 1));
      this.pinch = { distance, x: mx, y: my, moved: true };
      this.fitTarget = null;
      this.setCamera(
        this.fromTransform({
          ...zoomed,
          offsetX: zoomed.offsetX + mx - pinch.x,
          offsetY: zoomed.offsetY + my - pinch.y,
        }),
        false,
        event,
      );
      return;
    }
    if (this.pointers.size >= 2) return;
    const drag = this.drag;
    if (drag) {
      const dx = px - drag.x;
      const dy = py - drag.y;
      if (!drag.moved && Math.hypot(dx, dy) > DRAG_PX) {
        drag.moved = true;
        if (drag.pan) {
          this.panning = true;
          this.updateCursor();
        }
      }
      if (drag.moved && drag.pan) {
        this.panFrom(drag.camera, dx, dy, false, event);
        return;
      }
    }
    const toolEvent = this.toolEvent(event, px, py);
    this.emit("cursor", { point: [toolEvent.x, toolEvent.y] });
    this.tool.pointerMove?.(toolEvent, this);
  }

  /** ドラッグで図面を動かしていたら、やめて moveend を送る */
  private endPan(event: Event): void {
    if (!this.panning) return;
    this.panning = false;
    this.updateCursor();
    this.emitMoveEnd(event);
  }

  /** ポインターを外す。2 本指の操作が終わったら moveend を送る */
  private releasePointer(event: PointerEvent): void {
    this.pointers.delete(event.pointerId);
    if (this.pointers.size < 2 && this.pinch) {
      const moved = this.pinch.moved;
      this.pinch = null;
      if (moved) this.emitMoveEnd(event);
    }
  }

  private pointerUp(event: PointerEvent): void {
    this.releasePointer(event);
    const drag = this.drag;
    this.drag = null;
    this.endPan(event);
    if (!drag) return;
    const [px, py] = this.local(event);
    const toolEvent = this.toolEvent(event, px, py);
    this.tool.pointerUp?.(toolEvent, this);
    if (!drag.moved) this.tool.click?.(toolEvent, this);
  }

  /** スクロールなどで取り消されたポインターは、クリックとして扱わない */
  private pointerCancel(event: PointerEvent): void {
    this.releasePointer(event);
    const drag = this.drag;
    this.drag = null;
    this.endPan(event);
    if (drag) this.tool.cancel?.(this);
  }

  private pointerLeave(): void {
    this.emit("cursor", { point: null });
    this.tool.pointerLeave?.(this);
  }

  private wheel(event: WheelEvent): void {
    const mode = this.handlers.wheel;
    if (mode === "never" || (mode === "modifier" && !event.ctrlKey && !event.metaKey)) return;
    event.preventDefault();
    const [px, py] = this.local(event);
    const delta = event.deltaMode === 1 ? event.deltaY * 16 : event.deltaY;
    this.zoomAround(Math.exp(-delta * (event.ctrlKey ? 0.01 : 0.0015)), px, py, false, event);
    clearTimeout(this.wheelTimer);
    this.wheelTimer = setTimeout(() => {
      if (!this.destroyed) this.emitMoveEnd(event);
    }, WHEEL_END_MS);
  }

  private keyDown(event: KeyboardEvent): void {
    if (this.tool.keyDown?.(event, this) === true) {
      event.preventDefault();
      return;
    }
    // ブラウザの拡大（Ctrl・⌘ と +）などのショートカットは妨げない
    if (!this.handlers.keyboard || event.ctrlKey || event.metaKey || event.altKey) return;
    switch (event.key) {
      case "+":
      case "=":
        this.zoomAround(1.25, undefined, undefined, true, event);
        break;
      case "-":
        this.zoomAround(0.8, undefined, undefined, true, event);
        break;
      case "0":
        this.fitTo(this.state.scene.bounds, undefined, true, event);
        break;
      case "ArrowLeft":
        this.panFrom(this.camera, KEY_PAN_PX, 0, true, event);
        break;
      case "ArrowRight":
        this.panFrom(this.camera, -KEY_PAN_PX, 0, true, event);
        break;
      case "ArrowUp":
        this.panFrom(this.camera, 0, KEY_PAN_PX, true, event);
        break;
      case "ArrowDown":
        this.panFrom(this.camera, 0, -KEY_PAN_PX, true, event);
        break;
      case "Escape":
        this.tool.cancel?.(this);
        this.setMeasurement(null);
        this.select([]);
        break;
      default:
        return;
    }
    event.preventDefault();
  }
}
