import type { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { type MemberFilter, membersOnAxis } from "../filter.js";
import type { GridAxis, MemberKind, StructureMember, StructureModel, Vec3 } from "../types.js";
import { type ColorMode, type Projection, type RenderColors, StructureRenderer, type Viewpoint } from "./renderer.js";
import {
  mergeStructurePatch,
  mergeStructureStyle,
  readStructureStyle,
  rgb,
  type StructurePalette,
  type StructureStyle,
  type StructureStylePatch,
  type StructureTheme,
} from "./style.js";
import { orbitTool, type StructureTool, type StructureToolEvent, selectTool } from "./tools.js";

/** 通り芯・階などの補助の表示 */
export interface StructureGuides {
  /** 通り芯の線 */
  readonly axes: boolean;
  /** 階の線 */
  readonly stories: boolean;
  /** 通り芯・階の名前 */
  readonly labels: boolean;
  /** 節点 */
  readonly nodes: boolean;
}

/**
 * 表示と操作の状態。変わるたびに新しいオブジェクトになる（React の useSyncExternalStore にそのまま渡せる）。
 * カメラは持たず、イベント（move・moveend）で伝える
 */
export interface StructureViewerState {
  readonly model: StructureModel;
  /** 配色。setTheme(null)（既定）なら OS の明暗の設定に従う */
  readonly theme: StructureTheme;
  /** 隠している部材の種類 */
  readonly hiddenKinds: ReadonlySet<MemberKind>;
  /** 絞り込んだ階（StructureModel.stories の id）。null はすべて */
  readonly story: number | null;
  /** 絞り込んだ通り芯。null はすべて */
  readonly axis: GridAxis | null;
  /** 絞り込みの後に見えている部材の数 */
  readonly visibleCount: number;
  /** 選んでいる部材（選んだ順） */
  readonly selection: readonly StructureMember[];
  /** ポインターの下の部材（選択の道具が決める） */
  readonly hovered: StructureMember | null;
  readonly projection: Projection;
  readonly colorMode: ColorMode;
  /** 部材の稜線 */
  readonly edges: boolean;
  /** スラブ・壁を半透明にする */
  readonly translucent: boolean;
  readonly guides: StructureGuides;
  /** 使用中の道具の名前 */
  readonly tool: string;
}

/** カメラ。jumpTo に渡し、getView で読む。座標はモデルの座標（mm、Z が上） */
export interface StructureCamera {
  /** カメラの位置 */
  readonly position: Vec3;
  /** 注視点（回転の中心） */
  readonly target: Vec3;
  /** 注視点で見える高さ（mm）。透視投影では注視点までの距離で決まる */
  readonly span: number;
}

/** 今のカメラ */
export interface StructureView extends StructureCamera {
  readonly projection: Projection;
  /** 表示領域の大きさ（CSS px） */
  readonly width: number;
  readonly height: number;
}

/** 操作のハンドラ。1 つずつ止められる */
export interface StructureHandlers {
  /** 左ボタン（タッチは 1 本指）のドラッグで回す。道具が許すとき（StructureTool.orbit） */
  rotate: boolean;
  /** 右ボタンのドラッグ（タッチは 2 本指）と矢印キーで動かす */
  pan: boolean;
  /**
   * ホイール・中ボタンのドラッグ・2 本指のピンチで拡大・縮小する。"modifier" はホイールについて、Ctrl・⌘ を押しているとき
   * （トラックパッドのピンチを含む）だけ拡大し、それ以外はページをスクロールさせる
   */
  zoom: "always" | "modifier" | "never";
  /** キーの操作。矢印で移動、0 と f で全体表示、Esc で選択を外す */
  keyboard: boolean;
}

/** カメラのイベント。originalEvent は利用者の操作による変化のときの元のイベント（メソッドで変えたときは null） */
export interface StructureViewEvent {
  readonly view: StructureView;
  readonly originalEvent: Event | null;
}

/** 選択のイベント。originalEvent はクリック・キーで選んだときの元のイベント */
export interface StructureSelectEvent {
  readonly selection: readonly StructureMember[];
  readonly originalEvent: Event | null;
}

export interface StructureHoverEvent {
  readonly member: StructureMember | null;
}

export interface StructureViewerEventMap {
  /** カメラが変わるたび（ドラッグ・ホイールの間は何度も） */
  move: StructureViewEvent;
  /** カメラの変化が終わったとき（ドラッグを離した、ホイールが止まった、メソッドで変えた） */
  moveend: StructureViewEvent;
  /** 選択が変わったとき（絞り込みで隠れた部材が外れたときも） */
  select: StructureSelectEvent;
  /** ポインターの下の部材が変わったとき */
  hover: StructureHoverEvent;
}

export interface StructureViewerOptions {
  /** 配色。省略すると OS の明暗の設定に従う */
  theme?: StructureTheme;
  /** 操作を受ける Canvas の読み上げ用の説明（aria-label）。文言は利用側が決める */
  label?: string;
  /** 操作のハンドラ。省略したものは有効（zoom は "always"） */
  handlers?: Partial<StructureHandlers>;
  /** 見た目の上書き。CSS のカスタムプロパティより優先する */
  style?: StructureStylePatch;
  /** 利用側の道具。組み込みの select・orbit に加える（同じ名前なら置き換える） */
  tools?: readonly StructureTool[];
  /** 最初に使う道具の名前。省略すると "select" */
  tool?: string;
  /** 色分け。省略すると "kind"（部材の種類） */
  colorMode?: ColorMode;
  /** 稜線。省略すると描く */
  edges?: boolean;
  /** スラブ・壁を半透明にする。省略すると半透明 */
  translucent?: boolean;
  /** 補助の表示。省略すると節点のほかは描く */
  guides?: Partial<StructureGuides>;
}

/** 部材を選ぶ距離などの問い合わせの条件 */
export interface MemberQueryOptions {
  /** 線だけの部材を選ぶ距離（CSS px、既定 6） */
  tolerance?: number;
}

/** 視点を動かす操作の条件 */
export interface FocusOptions {
  /** カメラも動かすか。省略すると動かす（階は平行投影で真上から、通り芯は直交する向きから、解除は斜めから） */
  camera?: boolean;
}

type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Listener<K extends keyof StructureViewerEventMap> = (event: StructureViewerEventMap[K]) => void;

const BUILT_IN_TOOLS: Readonly<Record<string, () => StructureTool>> = {
  select: selectTool,
  orbit: orbitTool,
};
const DEFAULT_HANDLERS: StructureHandlers = { rotate: true, pan: true, zoom: "always", keyboard: true };
const DEFAULT_GUIDES: StructureGuides = { axes: true, stories: true, labels: true, nodes: false };
/** これより動いたらクリックでなくドラッグとみなす（CSS px） */
const DRAG_PX = 4;
/** ホイール・キーで動かした後、moveend を送るまで（ms） */
const END_MS = 200;
const NO_KINDS: ReadonlySet<MemberKind> = new Set();

function sameMembers(a: readonly StructureMember[], b: readonly StructureMember[]): boolean {
  return a.length === b.length && a.every((member, index) => member === b[index]);
}

/**
 * ST-Bridge の構造モデルの 3D の表示と操作のエンジン（three.js）。container の中に WebGL の Canvas と名前の層を作る。
 * カメラは jumpTo・fit・viewFrom・viewAxis・setProjection で変え、getView と move・moveend のイベントで読む。
 * 状態（モデル、隠す種類、階・通り芯の絞り込み、選択、投影、色分け、補助の表示、道具）は getState・subscribe で読む。
 * 利用者の操作は、操作のハンドラ（回転・移動・拡大・キー）と、使用中の道具が受ける。
 * 色は container の CSS のカスタムプロパティ（--zumen-*）から読み、setStyle で上書きする。画面の文言は持たない
 */
export class StructureViewer {
  /** 操作を受ける WebGL の Canvas。フォーカスと読み上げの対象 */
  readonly canvas: HTMLCanvasElement;
  /** 通り芯・階の名前を置く層（Canvas の上、ポインターのイベントは受けない） */
  readonly labelLayer: HTMLElement;
  private readonly renderer: StructureRenderer;
  private readonly win: Window;
  private state: StructureViewerState;
  private handlers: StructureHandlers;
  private themeOverride: StructureTheme | null;
  private styleOverrides: StructureStylePatch;
  private style: StructureStyle;
  private readonly probe: CanvasRenderingContext2D | null;
  private readonly scheme: MediaQueryList | null;
  private readonly tools = new Map<string, StructureTool>();
  private tool: StructureTool;
  private destroyed = false;
  /** メソッドでカメラを動かしている間（OrbitControls の change をイベントにしない） */
  private programmatic = false;
  /** 利用者がカメラを動かしている間（OrbitControls の start から end まで） */
  private interacting = false;
  /** move を送って、まだ moveend を送っていない */
  private pendingEnd = false;
  private endTimer: ReturnType<typeof setTimeout> | undefined;
  /** 最後に受けた利用者の操作のイベント */
  private inputEvent: Event | null = null;
  private drag: { px: number; py: number; moved: boolean; camera: boolean } | null = null;
  private readonly listeners = new Set<() => void>();
  private readonly events: { [K in keyof StructureViewerEventMap]: Set<Listener<K>> } = {
    move: new Set(),
    moveend: new Set(),
    select: new Set(),
    hover: new Set(),
  };
  private readonly detachers: (() => void)[] = [];
  private readonly observer: ResizeObserver;

  constructor(
    private readonly container: HTMLElement,
    model: StructureModel,
    options: StructureViewerOptions = {},
  ) {
    const doc = container.ownerDocument;
    this.win = doc.defaultView ?? window;
    this.handlers = { ...DEFAULT_HANDLERS, ...options.handlers };
    this.themeOverride = options.theme ?? null;
    this.styleOverrides = options.style ?? {};
    for (const create of Object.values(BUILT_IN_TOOLS)) {
      const tool = create();
      this.tools.set(tool.name, tool);
    }
    for (const tool of options.tools ?? []) this.tools.set(tool.name, tool);
    const tool = this.tools.get(options.tool ?? "select");
    if (!tool) throw new Error(`道具 ${options.tool} がありません`);
    this.tool = tool;
    this.scheme = this.win.matchMedia?.("(prefers-color-scheme: dark)") ?? null;
    const probe = doc.createElement("canvas");
    probe.width = 1;
    probe.height = 1;
    this.probe = probe.getContext("2d", { willReadFrequently: true });
    if (this.win.getComputedStyle(container).position === "static") container.style.position = "relative";
    this.style = this.readStyle();
    this.state = {
      model,
      theme: this.resolveTheme(),
      hiddenKinds: NO_KINDS,
      story: null,
      axis: null,
      visibleCount: model.members.length,
      selection: [],
      hovered: null,
      projection: "perspective",
      colorMode: options.colorMode ?? "kind",
      edges: options.edges ?? true,
      translucent: options.translucent ?? true,
      guides: { ...DEFAULT_GUIDES, ...options.guides },
      tool: tool.name,
    };
    this.renderer = new StructureRenderer(container, this.renderOptions(), this.renderColors());
    this.renderer.onControls = (controls) => this.configureControls(controls);
    this.configureControls(this.renderer.getControls());
    this.renderer.onChange = () => this.cameraChanged();
    this.renderer.onStart = () => {
      this.interacting = true;
      clearTimeout(this.endTimer);
    };
    this.renderer.onEnd = () => {
      this.interacting = false;
      if (!this.pendingEnd) return;
      if (this.inputEvent?.type === "wheel") this.scheduleEnd();
      else this.flushEnd();
    };
    this.canvas = this.renderer.canvas;
    this.labelLayer = this.renderer.labelLayer;
    this.canvas.tabIndex = 0;
    this.canvas.setAttribute("role", "img");
    if (options.label !== undefined) this.canvas.setAttribute("aria-label", options.label);
    this.renderer.resize();
    this.renderer.setModel(model);
    this.updateCursor();
    this.listen();
    this.tool.activate?.(this);
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(container);
  }

  // ---- 状態

  /** 今の状態。変わらない間は同じオブジェクトを返す */
  readonly getState = (): StructureViewerState => this.state;

  /** 状態が変わったときに呼ぶ関数を登録し、解除する関数を返す */
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  get model(): StructureModel {
    return this.state.model;
  }

  /** 別のモデルに替える。絞り込み・選択を初期化し、透視投影で全体を斜めから見る。色分け・補助の表示・道具はそのまま */
  setModel(model: StructureModel): void {
    this.tool.cancel?.(this);
    const hadSelection = this.state.selection.length > 0;
    this.update({
      model,
      hiddenKinds: NO_KINDS,
      story: null,
      axis: null,
      visibleCount: model.members.length,
      selection: [],
      hovered: null,
      projection: "perspective",
    });
    this.moveCamera(() => {
      this.renderer.setProjection("perspective");
      this.renderer.setModel(model);
    });
    if (hadSelection) this.emit("select", { selection: [], originalEvent: null });
  }

  /** 配色を決める。null なら OS の明暗の設定に従う */
  setTheme(theme: StructureTheme | null): void {
    this.themeOverride = theme;
    this.applyTheme();
  }

  /** 隠す部材の種類を置き換える（空ならすべて表示） */
  setHiddenKinds(kinds: Iterable<MemberKind>): void {
    this.update({ hiddenKinds: new Set(kinds) });
    this.applyFilter();
  }

  /** 部材の種類を表示する・隠す */
  setKindsVisible(kinds: Iterable<MemberKind>, visible: boolean): void {
    const next = new Set(this.state.hiddenKinds);
    for (const kind of kinds) {
      if (visible) next.delete(kind);
      else next.add(kind);
    }
    this.setHiddenKinds(next);
  }

  /**
   * 階に絞る（null で解除）。通り芯の絞り込みは外す。カメラは、階なら平行投影で真上から、解除なら斜めから見る
   */
  focusStory(id: number | null, options: FocusOptions = {}): void {
    const story = id !== null && this.state.model.stories.some((item) => item.id === id) ? id : null;
    this.update({ story, axis: null });
    this.applyFilter();
    if (options.camera === false) return;
    this.moveCamera(() => {
      if (story === null) this.renderer.viewFrom("iso");
      else {
        this.renderer.setProjection("orthographic");
        this.renderer.viewFrom("top");
      }
    });
  }

  /**
   * 通り芯の上の部材に絞る（GridAxis.key、null で解除）。階の絞り込みは外す。カメラは、通り芯なら平行投影で
   * 通り芯に直交する向きから、解除なら斜めから見る
   */
  focusAxis(key: string | null, options: FocusOptions = {}): void {
    const axis = key === null ? null : (this.state.model.axes.find((item) => item.key === key) ?? null);
    this.update({ axis, story: null });
    this.applyFilter();
    if (options.camera === false) return;
    this.moveCamera(() => {
      if (!axis) this.renderer.viewFrom("iso");
      else {
        this.renderer.setProjection("orthographic");
        this.renderer.viewAxis(axis);
      }
    });
  }

  /** 選択を置き換える（空なら選択を外す）。見えていない部材は選ばない。originalEvent は select のイベントに渡す */
  select(members: Iterable<StructureMember>, originalEvent: Event | null = null): void {
    const selection = [...new Set(members)].filter((member) => this.renderer.isVisible(member.key));
    if (sameMembers(selection, this.state.selection)) return;
    this.update({ selection });
    this.renderer.setSelection(selection.map((member) => member.key));
    this.emit("select", { selection, originalEvent });
  }

  /** ポインターの下の部材を決める。選択の道具が使い、見た目の hover の色があれば描く */
  setHovered(member: StructureMember | null): void {
    const hovered = member && this.renderer.isVisible(member.key) ? member : null;
    if (hovered === this.state.hovered) return;
    this.update({ hovered });
    this.renderer.setHover(hovered?.key ?? null);
    this.emit("hover", { member: hovered });
  }

  setProjection(projection: Projection): void {
    if (projection === this.state.projection) return;
    this.moveCamera(() => this.renderer.setProjection(projection));
  }

  setColorMode(colorMode: ColorMode): void {
    if (colorMode === this.state.colorMode) return;
    this.update({ colorMode });
    this.renderer.setOptions({ colorMode });
  }

  setEdges(edges: boolean): void {
    if (edges === this.state.edges) return;
    this.update({ edges });
    this.renderer.setOptions({ edges });
  }

  setTranslucent(translucent: boolean): void {
    if (translucent === this.state.translucent) return;
    this.update({ translucent });
    this.renderer.setOptions({ translucent });
  }

  /** 補助の表示を変える（渡したものだけ） */
  setGuides(patch: Partial<StructureGuides>): void {
    const guides = { ...this.state.guides, ...patch };
    const current = this.state.guides;
    if ((Object.keys(guides) as (keyof StructureGuides)[]).every((key) => guides[key] === current[key])) return;
    this.update({ guides });
    this.renderer.setOptions(guides);
  }

  // ---- 道具

  /** 道具を替える。組み込みは "select"・"orbit"、ほかは addTool で加えたもの */
  setTool(name: string): void {
    const next = this.tools.get(name);
    if (!next) throw new Error(`道具 ${name} がありません`);
    if (next !== this.tool) this.swapTool(next);
  }

  /** 道具を加え、外す関数を返す。同じ名前の道具は置き換える。使用中の道具を外すと "select" に戻る */
  addTool(tool: StructureTool): () => void {
    const replaced = this.tools.get(tool.name);
    this.tools.set(tool.name, tool);
    if (replaced === this.tool) this.swapTool(tool);
    return () => {
      if (this.tools.get(tool.name) !== tool) return;
      // 組み込みの道具を置き換えていたら、組み込みに戻す
      const builtIn = BUILT_IN_TOOLS[tool.name];
      if (builtIn) this.tools.set(tool.name, builtIn());
      else this.tools.delete(tool.name);
      if (this.tool === tool) this.swapTool(this.tools.get(builtIn ? tool.name : "select") as StructureTool);
    };
  }

  // ---- カメラ

  /** 今のカメラ */
  getView(): StructureView {
    const camera = this.renderer.getCamera();
    const { width, height } = this.renderer.size;
    return { ...camera, projection: this.state.projection, width, height };
  }

  /**
   * カメラを置く（省略したものは今のまま）。getView の値を渡せば同じカメラに戻る。
   * 透視投影では position を渡せば span は使わず、渡さなければ注視点からの距離を span に合わせる
   */
  jumpTo(camera: Partial<StructureCamera>): void {
    this.moveCamera(() => this.renderer.setCamera(camera));
  }

  /** 見えている部材の全体を、今の向きのまま収める */
  fit(): void {
    this.moveCamera(() => this.renderer.fit());
  }

  /** 決まった向き（斜め・真上・正面・側面）から、見えている部材の全体を見る */
  viewFrom(viewpoint: Viewpoint): void {
    this.moveCamera(() => this.renderer.viewFrom(viewpoint));
  }

  /** 通り芯に直交する向きから見る（軸組の確認）。X 方向の通りは −Y 側、Y 方向の通りは +X 側から見る */
  viewAxis(axis: GridAxis): void {
    this.moveCamera(() => this.renderer.viewAxis(axis));
  }

  /** モデルの点（mm）が表示領域のどこに見えるか（左上からの CSS px）。カメラの後ろなら null。HTML を重ねるときに使う */
  project(point: Vec3): [number, number] | null {
    return this.renderer.project(point);
  }

  // ---- 問い合わせ（入力を自分で扱う利用側と道具のため）

  /** 表示領域の点（左上からの CSS px）にある、見えている部材のうち最も手前のもの */
  memberAt(px: number, py: number, options: MemberQueryOptions = {}): StructureMember | null {
    return this.renderer.pick(px, py, options.tolerance);
  }

  // ---- イベント

  /** イベントを受ける関数を登録し、解除する関数を返す */
  on<K extends keyof StructureViewerEventMap>(
    type: K,
    listener: (event: StructureViewerEventMap[K]) => void,
  ): () => void {
    const set = this.events[type] as Set<Listener<K>>;
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  }

  // ---- 操作のハンドラ

  getHandlers(): StructureHandlers {
    return { ...this.handlers };
  }

  /** 操作のハンドラを変える（渡したものだけ） */
  setHandlers(patch: Partial<StructureHandlers>): void {
    this.handlers = { ...this.handlers, ...patch };
    this.configureControls(this.renderer.getControls());
  }

  // ---- 見た目

  /** 今の見た目（CSS のカスタムプロパティに setStyle の上書きを重ねたもの） */
  getStyle(): StructureStyle {
    return this.style;
  }

  /** 今の配色の色の組 */
  getPalette(): StructurePalette {
    return this.style[this.state.theme];
  }

  /** 見た目を上書きする（渡した項目だけ）。CSS のカスタムプロパティより優先する */
  setStyle(patch: StructureStylePatch): void {
    this.styleOverrides = mergeStructurePatch(this.styleOverrides, patch);
    this.refreshStyle();
  }

  /**
   * CSS のカスタムプロパティを読み直して描き直す。作ったとき、大きさが変わったとき、OS の明暗の設定が変わったときは
   * 自動で読む。利用側がクラスなどで配色を替えたときに呼ぶ
   */
  refreshStyle(): void {
    if (this.destroyed) return;
    this.style = this.readStyle();
    this.renderer.setColors(this.renderColors());
  }

  // ---- その他

  /** 操作を受ける Canvas の読み上げ用の説明。null で外す */
  setLabel(label: string | null): void {
    if (label === null) this.canvas.removeAttribute("aria-label");
    else this.canvas.setAttribute("aria-label", label);
  }

  /** 今の画面の画像（PNG の data URL） */
  snapshot(): string {
    return this.renderer.snapshot();
  }

  /** Canvas・名前の層と監視を外す。以後は使えない */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    this.listeners.clear();
    for (const set of Object.values(this.events)) set.clear();
    this.tool.deactivate?.(this);
    clearTimeout(this.endTimer);
    this.observer.disconnect();
    for (const detach of this.detachers) detach();
    this.renderer.dispose();
  }

  // ---- 内部

  private update(patch: Partial<StructureViewerState>): void {
    const next: Mutable<StructureViewerState> = { ...this.state, ...patch };
    this.state = next;
    for (const listener of [...this.listeners]) listener();
  }

  private emit<K extends keyof StructureViewerEventMap>(type: K, event: StructureViewerEventMap[K]): void {
    for (const listener of [...(this.events[type] as Set<Listener<K>>)]) listener(event);
  }

  private resolveTheme(): StructureTheme {
    return this.themeOverride ?? (this.scheme?.matches ? "dark" : "light");
  }

  private applyTheme(): void {
    const theme = this.resolveTheme();
    if (theme === this.state.theme) return;
    this.update({ theme });
    this.renderer.setColors(this.renderColors());
  }

  private readStyle(): StructureStyle {
    return mergeStructureStyle(readStructureStyle(this.win.getComputedStyle(this.container)), this.styleOverrides);
  }

  private renderOptions() {
    const { colorMode, edges, translucent, guides } = this.state;
    return { colorMode, edges, translucent, ...guides };
  }

  private renderColors(): RenderColors {
    const palette = this.style[this.state.theme];
    const color = (value: string) => rgb(this.probe, value);
    const kinds: Record<string, number> = {};
    for (const [key, value] of Object.entries(this.style.kinds)) kinds[key] = color(value);
    const structures: Record<string, number> = {};
    for (const [key, value] of Object.entries(this.style.structures)) structures[key] = color(value);
    return {
      palette,
      background: color(palette.background),
      selection: color(palette.selection),
      hover: palette.hover === null ? null : color(palette.hover),
      edge: color(palette.edge),
      line: color(palette.line),
      axis: color(palette.axis),
      story: color(palette.story),
      node: color(palette.node),
      kinds,
      structures,
      structureFallback: color(this.style.structureFallback),
    };
  }

  /** 絞り込みを描画に渡し、見えなくなった部材を選択とポインターの下から外す */
  private applyFilter(): void {
    const { model, hiddenKinds, story, axis } = this.state;
    const filter: MemberFilter = {
      hiddenKinds,
      stories: story === null ? null : new Set([story]),
      only: axis ? membersOnAxis(model, axis) : null,
    };
    this.renderer.setFilter(filter, axis);
    let visibleCount = 0;
    for (const member of model.members) if (this.renderer.isVisible(member.key)) visibleCount++;
    if (visibleCount !== this.state.visibleCount) this.update({ visibleCount });
    const selection = this.state.selection.filter((member) => this.renderer.isVisible(member.key));
    if (selection.length !== this.state.selection.length) {
      this.update({ selection });
      this.renderer.setSelection(selection.map((member) => member.key));
      this.emit("select", { selection, originalEvent: null });
    }
    const hovered = this.state.hovered;
    if (hovered && !this.renderer.isVisible(hovered.key)) this.setHovered(null);
  }

  /** メソッドでカメラを動かす。変わったら move と moveend を 1 回ずつ送る */
  private moveCamera(change: () => void): void {
    const before = this.getView();
    this.programmatic = true;
    try {
      change();
    } finally {
      this.programmatic = false;
    }
    const projection = this.renderer.projection;
    if (projection !== this.state.projection) this.update({ projection });
    const view = this.getView();
    if (
      view.projection === before.projection &&
      view.span === before.span &&
      view.position.every((value, index) => value === before.position[index]) &&
      view.target.every((value, index) => value === before.target[index])
    )
      return;
    this.emit("move", { view, originalEvent: null });
    this.emit("moveend", { view, originalEvent: null });
  }

  /** OrbitControls がカメラを動かしたとき */
  private cameraChanged(): void {
    if (this.programmatic || this.destroyed) return;
    this.pendingEnd = true;
    this.emit("move", { view: this.getView(), originalEvent: this.inputEvent });
    // ホイール・キーは start・end を挟まないか、1 回ごとに挟むので、止まってから moveend を送る
    if (!this.interacting) this.scheduleEnd();
  }

  private scheduleEnd(): void {
    clearTimeout(this.endTimer);
    this.endTimer = setTimeout(() => this.flushEnd(), END_MS);
  }

  private flushEnd(): void {
    clearTimeout(this.endTimer);
    if (!this.pendingEnd || this.destroyed) return;
    this.pendingEnd = false;
    this.emit("moveend", { view: this.getView(), originalEvent: this.inputEvent });
  }

  private resize(): void {
    if (this.destroyed) return;
    this.refreshStyle();
    if (!this.renderer.resize()) return;
    const view = this.getView();
    this.emit("move", { view, originalEvent: null });
    this.emit("moveend", { view, originalEvent: null });
  }

  /** 操作のハンドラと使用中の道具を、OrbitControls に移す */
  private configureControls(controls: OrbitControls): void {
    const h = this.handlers;
    controls.enableRotate = h.rotate && this.tool.orbit !== false;
    controls.enablePan = h.pan;
    controls.enableZoom = h.zoom !== "never";
    controls.stopListenToKeyEvents();
    if (h.keyboard) controls.listenToKeyEvents(this.renderer.canvas);
  }

  private swapTool(next: StructureTool): void {
    this.tool.deactivate?.(this);
    this.tool = next;
    this.update({ tool: next.name });
    this.configureControls(this.renderer.getControls());
    this.updateCursor();
    next.activate?.(this);
  }

  private updateCursor(): void {
    this.canvas.style.cursor = this.drag?.moved && this.drag.camera ? "grabbing" : (this.tool.cursor ?? "grab");
  }

  private local(event: MouseEvent): [number, number] {
    const rect = this.canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }

  private toolEvent(event: PointerEvent): StructureToolEvent {
    const [px, py] = this.local(event);
    return { px, py, event };
  }

  private listen(): void {
    const canvas = this.canvas;
    const on = <K extends keyof HTMLElementEventMap>(
      target: HTMLElement,
      type: K,
      handler: (event: HTMLElementEventMap[K]) => void,
      options?: AddEventListenerOptions,
    ) => {
      target.addEventListener(type, handler, options);
      this.detachers.push(() => target.removeEventListener(type, handler, options));
    };
    // 利用者の操作のイベントを、OrbitControls より先に受けて覚える（move の originalEvent）
    for (const type of ["pointerdown", "pointermove", "pointerup", "keydown"] as const) {
      on(
        this.container,
        type,
        (event) => {
          if (event.target === canvas) this.inputEvent = event;
        },
        { capture: true },
      );
    }
    // ホイールは、拡大しないときに OrbitControls へ渡さない（"modifier" で修飾キーがなければページをスクロールさせる）
    on(
      this.container,
      "wheel",
      (event) => {
        if (event.target !== canvas) return;
        const mode = this.handlers.zoom;
        if (mode === "never" || (mode === "modifier" && !event.ctrlKey && !event.metaKey)) {
          event.stopPropagation();
          return;
        }
        this.inputEvent = event;
      },
      { capture: true },
    );
    on(canvas, "pointerdown", (event) => this.pointerDown(event));
    on(canvas, "pointermove", (event) => this.pointerMove(event));
    on(canvas, "pointerup", (event) => this.pointerUp(event));
    on(canvas, "pointercancel", () => this.pointerCancel());
    on(canvas, "pointerleave", () => this.tool.pointerLeave?.(this));
    on(canvas, "contextmenu", (event) => event.preventDefault());
    on(canvas, "keydown", (event) => this.keyDown(event));
    // OS の明暗の設定が変わったら、配色と CSS のカスタムプロパティ（prefers-color-scheme で替えるもの）を読み直す
    const scheme = this.scheme;
    if (scheme) {
      const changed = () => {
        this.applyTheme();
        this.refreshStyle();
      };
      scheme.addEventListener("change", changed);
      this.detachers.push(() => scheme.removeEventListener("change", changed));
    }
  }

  private pointerDown(event: PointerEvent): void {
    const [px, py] = this.local(event);
    const h = this.handlers;
    const camera =
      event.button === 0
        ? h.rotate && this.tool.orbit !== false
        : event.button === 2
          ? h.pan
          : event.button === 1 && h.zoom !== "never";
    this.drag = { px, py, moved: false, camera };
    this.tool.pointerDown?.(this.toolEvent(event), this);
  }

  private pointerMove(event: PointerEvent): void {
    const drag = this.drag;
    if (drag) {
      const [px, py] = this.local(event);
      if (!drag.moved && Math.hypot(px - drag.px, py - drag.py) > DRAG_PX) {
        drag.moved = true;
        this.updateCursor();
      }
      if (drag.moved && drag.camera) return;
    }
    this.tool.pointerMove?.(this.toolEvent(event), this);
  }

  private pointerUp(event: PointerEvent): void {
    const drag = this.drag;
    this.drag = null;
    if (drag?.moved) this.updateCursor();
    if (!drag) return;
    const toolEvent = this.toolEvent(event);
    this.tool.pointerUp?.(toolEvent, this);
    if (!drag.moved) this.tool.click?.(toolEvent, this);
  }

  /** スクロールなどで取り消されたポインターは、クリックとして扱わない */
  private pointerCancel(): void {
    const drag = this.drag;
    this.drag = null;
    if (!drag) return;
    this.updateCursor();
    this.tool.cancel?.(this);
  }

  private keyDown(event: KeyboardEvent): void {
    if (this.tool.keyDown?.(event, this) === true) {
      event.preventDefault();
      return;
    }
    // ブラウザのショートカットは妨げない
    if (!this.handlers.keyboard || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Escape") {
      this.tool.cancel?.(this);
      this.select([], event);
    } else if (event.key === "0" || event.key === "f") {
      this.moveCamera(() => this.renderer.fit());
    } else return;
    event.preventDefault();
  }
}
