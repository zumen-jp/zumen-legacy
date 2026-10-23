import {
  type Box,
  createMarkup,
  type MarkupDocument,
  type MarkupItem,
  type MarkupPoint,
  markupFromJson,
  markupShapes,
  type Primitive,
} from "@zumenjs/core";
import { distanceToSegmentSquared, emptyBox, extendBox, insideRings, transformPoints } from "@zumenjs/core/geometry";
import { renderPrimitives, toScreen, type View } from "./render.js";
import { cssProperty, type ViewerBackground } from "./style.js";
import type { DrawingViewer, OverlayFrame } from "./viewer.js";

/** 加える項目。id・author・createdAt・updatedAt は省くと MarkupLayer の options から決める */
export type MarkupItemInput = MarkupItem extends infer T
  ? T extends MarkupItem
    ? Omit<T, "id"> & { readonly id?: string }
    : never
  : never;

/** 項目を変える値。undefined を渡したキーは消す。id と kind は変えられない */
export type MarkupPatch = MarkupItem extends infer T
  ? T extends MarkupItem
    ? { readonly [K in Exclude<keyof T, "id" | "kind">]?: T[K] | undefined }
    : never
  : never;

/** 描く項目を選ぶ。false を返した項目は描かず、itemsAt でも返さない */
export type MarkupFilter = (item: MarkupItem) => boolean;

/** 背景ごとの色。値は CSS の色 */
export interface MarkupPalette {
  /** color も分類の色もない項目 */
  color: string;
  /** 分類（category）ごとの色 */
  categories: Readonly<Record<string, string>>;
  /** 選んだ項目の枠と、点のつまみ */
  selection: string;
  /** 新しい版へ移したとき、指す図形が変わった項目の印（carried.state が changed） */
  changed: string;
  /** 指す図形がなくなった項目の印（carried.state が removed） */
  removed: string;
}

/** 見た目。CSS のカスタムプロパティ（--zumen-markup-*）から読み、setStyle で上書きする */
export interface MarkupStyle {
  dark: MarkupPalette;
  light: MarkupPalette;
}

export interface MarkupStylePatch {
  dark?: Partial<MarkupPalette>;
  light?: Partial<MarkupPalette>;
}

/** 既定の見た目。項目は赤、選択は水色と青、変わった項目は琥珀色、なくなった項目は灰色 */
export const DEFAULT_MARKUP_STYLE: MarkupStyle = {
  dark: { color: "#ff4d4d", categories: {}, selection: "#33ccff", changed: "#ffb000", removed: "#a0a0a0" },
  light: { color: "#e00000", categories: {}, selection: "#0062d6", changed: "#b86e00", removed: "#707070" },
};

/** 色の項目と CSS のカスタムプロパティの名前（--zumen-markup-<名前>-dark・-light）。color は --zumen-markup-dark・-light */
const PROPERTIES = { selection: "selection", changed: "changed", removed: "removed" } as const;

/**
 * 層の状態。変わるたびに新しいオブジェクトになる（React の useSyncExternalStore にそのまま渡せる）
 */
export interface MarkupState {
  readonly document: MarkupDocument;
  /** 選んでいる項目の id（選んだ順） */
  readonly selection: readonly string[];
  /** ポインターの下の項目の id（編集の道具が決める） */
  readonly hovered: string | null;
  readonly filter: MarkupFilter | null;
  readonly canUndo: boolean;
  readonly canRedo: boolean;
}

/** 文書が変わった。利用側はこれを受けて document を保存する */
export interface MarkupChangeEvent {
  readonly document: MarkupDocument;
  readonly added: readonly string[];
  readonly updated: readonly string[];
  readonly removed: readonly string[];
  /** edit はメソッド（道具を含む）による変更、undo と redo は取り消しとやり直し */
  readonly origin: "edit" | "undo" | "redo";
}

export interface MarkupEventMap {
  change: MarkupChangeEvent;
}

export interface MarkupLayerOptions {
  /** 最初の文書。後から setDocument で替える */
  document: MarkupDocument;
  /** 加える項目の author。省くと付けない。後から setAuthor で替える */
  author?: string;
  /** 項目の id を作る。省くと UUID */
  createId?: () => string;
  /** 今の日時を RFC 3339 で返す。省くと new Date().toISOString() */
  now?: () => string;
  /** 見た目の上書き。CSS のカスタムプロパティより優先する */
  style?: MarkupStylePatch;
  filter?: MarkupFilter | null;
  /** 取り消せる操作の数。省くと 100 */
  historyLimit?: number;
}

export interface MarkupQueryOptions {
  /** 点からの距離の上限（CSS px）。省くと 6 */
  tolerance?: number;
}

interface HistoryEntry {
  before: MarkupDocument;
  after: MarkupDocument;
  added: readonly string[];
  updated: readonly string[];
  removed: readonly string[];
}

type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Listener<K extends keyof MarkupEventMap> = (event: MarkupEventMap[K]) => void;

/** 項目を選ぶ距離（CSS px） */
const PICK_PX = 6;
/** 選んだ項目の枠の、図形からの余白と、点のつまみの大きさ（CSS px） */
const SELECTION_PADDING = 4;
const HANDLE_PX = 7;
/** 新しい版へ移した印の枠の余白（CSS px） */
const CARRIED_PADDING = 7;

function randomId(): string {
  const crypto = globalThis.crypto;
  if (typeof crypto.randomUUID === "function") return crypto.randomUUID();
  // 安全でないコンテキスト（HTTP）には randomUUID がないので、乱数から UUID の版 4 を作る（RFC 9562 の 5.4 節）
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = ((bytes[6] as number) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] as number) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/** 項目を検査して複製する。誤りは MarkupError を投げる */
function checked(document: MarkupDocument, item: object): MarkupItem {
  return markupFromJson({ ...createMarkup(document.drawing), items: [item] }).items[0] as MarkupItem;
}

/** 図形の上の点 (x, y) からの距離の 2 乗。tolerance（用紙の mm）を超えれば Infinity。塗りと文字は内側なら 0 */
function shapeDistanceSquared(shape: Primitive, x: number, y: number, tolerance: number): number {
  switch (shape.kind) {
    case "stroke": {
      const points = shape.points;
      const reach = tolerance + shape.width / 2;
      let best = Number.POSITIVE_INFINITY;
      const count = points.length / 2;
      for (let i = 0; i + 1 < count + (shape.closed ? 1 : 0); i++) {
        const j = (i + 1) % count;
        best = Math.min(
          best,
          distanceToSegmentSquared(
            x,
            y,
            points[i * 2] as number,
            points[i * 2 + 1] as number,
            points[j * 2] as number,
            points[j * 2 + 1] as number,
          ),
        );
      }
      return best <= reach * reach ? best : Number.POSITIVE_INFINITY;
    }
    case "fill":
      return insideRings(x, y, shape.rings) ? 0 : Number.POSITIVE_INFINITY;
    case "text": {
      const { boxWidth: w, boxHeight: h } = shape;
      const corners = transformPoints(shape.matrix, [0, 0, w, 0, w, h, 0, h]);
      return insideRings(x, y, [corners]) ? 0 : Number.POSITIVE_INFINITY;
    }
    default:
      return Number.POSITIVE_INFINITY;
  }
}

/** 図形の外接矩形（用紙の mm） */
function shapesBox(shapes: readonly Primitive[]): Box {
  const box = emptyBox();
  for (const shape of shapes) {
    const extend = (points: ArrayLike<number>) => {
      for (let i = 0; i + 1 < points.length; i += 2) extendBox(box, points[i] as number, points[i + 1] as number);
    };
    if (shape.kind === "stroke") extend(shape.points);
    else if (shape.kind === "fill") for (const ring of shape.rings) extend(ring);
    else if (shape.kind === "text") {
      extend(
        transformPoints(shape.matrix, [0, 0, shape.boxWidth, 0, shape.boxWidth, shape.boxHeight, 0, shape.boxHeight]),
      );
    }
  }
  return box;
}

/** 項目の点のつまみ。key と index で項目の中の点を指す */
export interface MarkupHandle {
  readonly key: "points" | "position";
  readonly index: number;
  readonly point: MarkupPoint;
}

/**
 * 項目の点のつまみ。points の各点と、points も持つ項目（範囲のある指摘）の position。位置だけの項目（メモ・ピン・写真）は
 * つまみを持たず、項目ごと動かす
 */
export function markupHandles(item: MarkupItem | MarkupItemInput): MarkupHandle[] {
  const out: MarkupHandle[] = [];
  if (!("points" in item) || !item.points) return out;
  item.points.forEach((point, index) => {
    out.push({ key: "points", index, point });
  });
  if ("position" in item) out.push({ key: "position", index: 0, point: item.position });
  return out;
}

/**
 * 図面のマークアップの層。DrawingViewer の公開された API（addOverlay・状態・問い合わせ）の上に作り、文書
 * （MarkupDocument）の項目を、図面が描いているビュー（Scene.view）のものだけ描く。状態は getState・subscribe で読み、
 * 文書が変わると change イベントを送る（利用側が保存する）。操作は add・update・remove・undo・redo などのメソッドと、
 * redlineTool などの道具が行う。色は viewer の要素の CSS のカスタムプロパティ（--zumen-markup-*）から読み、
 * setStyle で上書きする。画面の文言は持たない
 */
export class MarkupLayer {
  private state: MarkupState;
  private readonly options: MarkupLayerOptions;
  private author: string | undefined;
  private readonly undoStack: HistoryEntry[] = [];
  private readonly redoStack: HistoryEntry[] = [];
  private readonly listeners = new Set<() => void>();
  private readonly events: { [K in keyof MarkupEventMap]: Set<Listener<K>> } = { change: new Set() };
  private preview: readonly MarkupItemInput[] = [];
  private styleOverrides: MarkupStylePatch;
  private style: MarkupStyle = DEFAULT_MARKUP_STYLE;
  /** CSS から読んだ分類ごとの色（背景:分類 → 色、指定がなければ空文字） */
  private readonly categoryColors = new Map<string, string>();
  private readonly probe: CanvasRenderingContext2D | null;
  private readonly detachers: (() => void)[] = [];
  private destroyed = false;

  constructor(
    readonly viewer: DrawingViewer,
    options: MarkupLayerOptions,
  ) {
    this.options = options;
    this.author = options.author;
    this.styleOverrides = options.style ?? {};
    markupFromJson(options.document);
    this.state = {
      document: options.document,
      selection: [],
      hovered: null,
      filter: options.filter ?? null,
      canUndo: false,
      canRedo: false,
    };
    const probe = viewer.canvas.ownerDocument.createElement("canvas");
    probe.width = 1;
    probe.height = 1;
    this.probe = probe.getContext("2d");
    this.readStyle();
    this.detachers.push(viewer.addOverlay((ctx, view, frame) => this.paint(ctx, view, frame)));
    // 図面を替えたら、選択と描きかけの項目を消す
    let scene = viewer.scene;
    this.detachers.push(
      viewer.subscribe(() => {
        if (viewer.scene === scene) return;
        scene = viewer.scene;
        this.preview = [];
        this.setState({ selection: [], hovered: null });
      }),
    );
    const win = viewer.canvas.ownerDocument.defaultView;
    const scheme = win?.matchMedia?.("(prefers-color-scheme: dark)");
    if (scheme) {
      const changed = () => this.refreshStyle();
      scheme.addEventListener("change", changed);
      this.detachers.push(() => scheme.removeEventListener("change", changed));
    }
  }

  // ---- 状態

  /** 今の状態。変わらない間は同じオブジェクトを返す */
  readonly getState = (): MarkupState => this.state;

  /** 状態が変わったときに呼ぶ関数を登録し、解除する関数を返す */
  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** イベントを受ける関数を登録し、解除する関数を返す */
  on<K extends keyof MarkupEventMap>(type: K, listener: (event: MarkupEventMap[K]) => void): () => void {
    const set = this.events[type] as Set<Listener<K>>;
    set.add(listener);
    return () => {
      set.delete(listener);
    };
  }

  get document(): MarkupDocument {
    return this.state.document;
  }

  /**
   * 文書を替える（別の図面・保存した文書を開いたとき）。検査して、誤りは MarkupError を投げる。
   * 取り消しの記録と、文書にない項目の選択を消す。change イベントは送らない
   */
  setDocument(document: MarkupDocument): void {
    markupFromJson(document);
    this.undoStack.length = 0;
    this.redoStack.length = 0;
    this.preview = [];
    const ids = new Set(document.items.map((item) => item.id));
    this.setState({
      document,
      selection: this.state.selection.filter((id) => ids.has(id)),
      hovered: this.state.hovered !== null && ids.has(this.state.hovered) ? this.state.hovered : null,
      canUndo: false,
      canRedo: false,
    });
    this.viewer.invalidate();
  }

  /** 加える項目の author を替える（null で付けない） */
  setAuthor(author: string | null): void {
    this.author = author ?? undefined;
  }

  // ---- 編集

  /**
   * 項目を加え、加えた項目を返す。id・author・createdAt・updatedAt を省くと options から決める。ビューを省いた項目は
   * 図面が描いているビューに置く。検査して、誤りや、文書にある id は投げる。取り消しの 1 回分になる
   */
  add(input: MarkupItemInput | readonly MarkupItemInput[]): MarkupItem[] {
    const inputs = (Array.isArray(input) ? input : [input]) as readonly MarkupItemInput[];
    if (inputs.length === 0) return [];
    const document = this.state.document;
    const ids = new Set(document.items.map((item) => item.id));
    const now = this.now();
    const view = this.viewer.scene.view;
    const items = inputs.map((value) => {
      const id = value.id ?? (this.options.createId ?? randomId)();
      if (ids.has(id)) throw new Error(`id ${id} の項目はもうあります`);
      ids.add(id);
      const { kind, ...rest } = value;
      const item = checked(document, {
        id,
        kind,
        ...(view === undefined || "view" in value ? {} : { view }),
        ...rest,
        ...(value.author === undefined && this.author !== undefined ? { author: this.author } : {}),
        createdAt: value.createdAt ?? now,
        updatedAt: value.updatedAt ?? value.createdAt ?? now,
      });
      return item;
    });
    this.commit(
      { ...document, items: [...document.items, ...items] },
      items.map((item) => item.id),
      [],
      [],
    );
    return items;
  }

  /**
   * 項目を変える（id と、変える値の組）。updatedAt を今の日時にする。検査して、誤りや、文書にない id は投げる。
   * 取り消しの 1 回分になる
   */
  update(id: string, patch: MarkupPatch): void;
  update(changes: readonly { readonly id: string; readonly patch: MarkupPatch }[]): void;
  update(target: string | readonly { readonly id: string; readonly patch: MarkupPatch }[], patch?: MarkupPatch): void {
    const changes = typeof target === "string" ? [{ id: target, patch: patch ?? {} }] : target;
    if (changes.length === 0) return;
    const document = this.state.document;
    const patches = new Map<string, MarkupPatch[]>();
    for (const change of changes) {
      if (!document.items.some((item) => item.id === change.id)) throw new Error(`id ${change.id} の項目はありません`);
      patches.set(change.id, [...(patches.get(change.id) ?? []), change.patch]);
    }
    const now = this.now();
    const items = document.items.map((item) => {
      const list = patches.get(item.id);
      if (!list) return item;
      const next: Record<string, unknown> = { ...item };
      for (const value of list) {
        for (const [key, entry] of Object.entries(value)) {
          if (key === "id" || key === "kind") continue;
          if (entry === undefined) delete next[key];
          else next[key] = entry;
        }
      }
      if (!list.some((value) => "updatedAt" in value)) next.updatedAt = now;
      return checked(document, next);
    });
    this.commit({ ...document, items }, [], [...patches.keys()], []);
  }

  /** 項目を消す。文書にない id は飛ばす。取り消しの 1 回分になる */
  remove(ids: string | Iterable<string>): void {
    const removing = new Set(typeof ids === "string" ? [ids] : ids);
    const document = this.state.document;
    const removed = document.items.filter((item) => removing.has(item.id)).map((item) => item.id);
    if (removed.length === 0) return;
    this.commit({ ...document, items: document.items.filter((item) => !removing.has(item.id)) }, [], [], removed);
  }

  /** 最後の操作を取り消す。取り消せなければ何もしない */
  undo(): void {
    const entry = this.undoStack.pop();
    if (!entry) return;
    this.redoStack.push(entry);
    this.apply(entry.before, entry.removed, entry.updated, entry.added, "undo");
  }

  /** 取り消した操作をやり直す */
  redo(): void {
    const entry = this.redoStack.pop();
    if (!entry) return;
    this.undoStack.push(entry);
    this.apply(entry.after, entry.added, entry.updated, entry.removed, "redo");
  }

  // ---- 選択・表示

  /** 選択を置き換える（空なら外す）。文書にない id は飛ばす */
  select(ids: Iterable<string>): void {
    const known = new Set(this.state.document.items.map((item) => item.id));
    const selection = [...new Set(ids)].filter((id) => known.has(id));
    const current = this.state.selection;
    if (selection.length === current.length && selection.every((id, index) => id === current[index])) return;
    this.setState({ selection });
    this.viewer.invalidate();
  }

  /** ポインターの下の項目を決める（編集の道具が使う） */
  setHovered(id: string | null): void {
    if (id === this.state.hovered) return;
    this.setState({ hovered: id });
  }

  /** 描く項目を選ぶ（null ですべて） */
  setFilter(filter: MarkupFilter | null): void {
    if (filter === this.state.filter) return;
    this.setState({ filter });
    this.viewer.invalidate();
  }

  /**
   * 文書に入れずに描く項目（道具の描きかけ、動かしている途中の項目）。文書にある id の項目は、その項目の代わりに描く。
   * null か空で消す
   */
  setPreview(items: MarkupItemInput | readonly MarkupItemInput[] | null): void {
    this.preview = items === null ? [] : ((Array.isArray(items) ? items : [items]) as readonly MarkupItemInput[]);
    this.viewer.invalidate();
  }

  /** 描いている項目（図面のビューのもので、filter を通るもの）。描く順 */
  items(): MarkupItem[] {
    const view = this.viewer.scene.view;
    const filter = this.state.filter;
    return this.state.document.items.filter((item) => item.view === view && (!filter || filter(item)));
  }

  /** 用紙の点 (x, y) から tolerance（CSS px）以内に描いている項目。上に描いたものが先 */
  itemsAt(x: number, y: number, options: MarkupQueryOptions = {}): MarkupItem[] {
    const pixel = this.pixel();
    const tolerance = (options.tolerance ?? PICK_PX) * pixel;
    const found: MarkupItem[] = [];
    for (const item of this.items().reverse()) {
      const shapes = this.shapes(item, pixel);
      if (shapes.some((shape) => shapeDistanceSquared(shape, x, y, tolerance) < Number.POSITIVE_INFINITY)) {
        found.push(item);
      }
    }
    return found;
  }

  /** 項目を今の表示で描く図形（用紙の mm）。色は項目・分類・見た目の順に選ぶ */
  shapes(item: MarkupItem | MarkupItemInput, pixel = this.pixel()): Primitive[] {
    return markupShapes({ id: "", ...item } as MarkupItem, { color: this.itemColor(item), pixel });
  }

  // ---- 見た目

  getStyle(): MarkupStyle {
    return this.style;
  }

  /** 見た目を上書きする（渡した項目だけ）。CSS のカスタムプロパティより優先する */
  setStyle(patch: MarkupStylePatch): void {
    const palette = (a: Partial<MarkupPalette> | undefined, b: Partial<MarkupPalette> | undefined) =>
      a || b ? { ...a, ...b, categories: { ...a?.categories, ...b?.categories } } : undefined;
    const dark = palette(this.styleOverrides.dark, patch.dark);
    const light = palette(this.styleOverrides.light, patch.light);
    this.styleOverrides = { ...(dark ? { dark } : {}), ...(light ? { light } : {}) };
    this.refreshStyle();
  }

  /** CSS のカスタムプロパティを読み直して描き直す。OS の明暗の設定が変わったときは自動で読む */
  refreshStyle(): void {
    this.readStyle();
    this.viewer.invalidate();
  }

  /** 重ね描きと監視を外す。以後は使えない */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    for (const detach of this.detachers) detach();
    this.listeners.clear();
    for (const set of Object.values(this.events)) set.clear();
  }

  // ---- 内部

  private now(): string {
    return (this.options.now ?? (() => new Date().toISOString()))();
  }

  private setState(patch: Partial<MarkupState>): void {
    const next: Mutable<MarkupState> = { ...this.state, ...patch };
    this.state = next;
    for (const listener of [...this.listeners]) listener();
  }

  private emit<K extends keyof MarkupEventMap>(type: K, event: MarkupEventMap[K]): void {
    for (const listener of [...(this.events[type] as Set<Listener<K>>)]) listener(event);
  }

  /** 編集を文書に入れ、取り消しの記録に積む */
  private commit(
    document: MarkupDocument,
    added: readonly string[],
    updated: readonly string[],
    removed: readonly string[],
  ): void {
    this.undoStack.push({ before: this.state.document, after: document, added, updated, removed });
    const limit = Math.max(0, this.options.historyLimit ?? 100);
    if (this.undoStack.length > limit) this.undoStack.splice(0, this.undoStack.length - limit);
    this.redoStack.length = 0;
    this.apply(document, added, updated, removed, "edit");
  }

  private apply(
    document: MarkupDocument,
    added: readonly string[],
    updated: readonly string[],
    removed: readonly string[],
    origin: MarkupChangeEvent["origin"],
  ): void {
    const ids = new Set(document.items.map((item) => item.id));
    this.setState({
      document,
      selection: this.state.selection.filter((id) => ids.has(id)),
      hovered: this.state.hovered !== null && ids.has(this.state.hovered) ? this.state.hovered : null,
      canUndo: this.undoStack.length > 0,
      canRedo: this.redoStack.length > 0,
    });
    this.viewer.invalidate();
    this.emit("change", { document, added, updated, removed, origin });
  }

  /** 今の表示の 1 CSS px の、用紙の mm */
  private pixel(): number {
    const [x0, y0] = this.viewer.toScreen(0, 0);
    const [x1, y1] = this.viewer.toScreen(1, 0);
    const scale = Math.hypot(x1 - x0, y1 - y0);
    return scale > 0 ? 1 / scale : 1;
  }

  private palette(background: ViewerBackground = this.viewer.getState().background): MarkupPalette {
    return this.style[background];
  }

  /** 項目が color を持たないときの色。setStyle の分類の色、CSS の --zumen-markup-category-<分類>-<背景>、color の順 */
  private itemColor(item: MarkupItem | MarkupItemInput): string {
    const background = this.viewer.getState().background;
    const palette = this.palette(background);
    const category = item.category;
    if (category === undefined) return palette.color;
    const own = palette.categories[category];
    if (own) return own;
    const key = `${background}:${category}`;
    let css = this.categoryColors.get(key);
    if (css === undefined) {
      css = this.readColor(`--zumen-markup-category-${category}-${background}`) ?? "";
      this.categoryColors.set(key, css);
    }
    return css || palette.color;
  }

  /**
   * CSS のカスタムプロパティの色を #rrggbb・#rrggbbaa にする（半透明の塗りの色を作れるように）。指定がないか、
   * 色でなければ null
   */
  private readColor(name: string): string | null {
    const value = cssProperty(this.computed(), name);
    if (value === null) return null;
    const ctx = this.probe;
    if (!ctx) return value;
    if (typeof CSS !== "undefined" && !CSS.supports("color", value)) return null;
    ctx.fillStyle = "#000000";
    ctx.fillStyle = value;
    const normalized = String(ctx.fillStyle);
    if (normalized.startsWith("#")) return normalized;
    const match = /^rgba?\(\s*(\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\s*\)$/.exec(normalized);
    if (!match) return value;
    const hex = (number: number) =>
      Math.round(Math.min(Math.max(number, 0), 255))
        .toString(16)
        .padStart(2, "0");
    const alpha = match[4] === undefined ? "" : hex(Number(match[4]) * 255);
    return `#${hex(Number(match[1]))}${hex(Number(match[2]))}${hex(Number(match[3]))}${alpha}`;
  }

  private computed(): CSSStyleDeclaration {
    const win = this.viewer.canvas.ownerDocument.defaultView ?? window;
    return win.getComputedStyle(this.viewer.canvas);
  }

  private readStyle(): void {
    this.categoryColors.clear();
    const palette = (background: ViewerBackground): MarkupPalette => {
      const base = DEFAULT_MARKUP_STYLE[background];
      const result: MarkupPalette = { ...base, categories: {} };
      const color = this.readColor(`--zumen-markup-${background}`);
      if (color) result.color = color;
      for (const [key, name] of Object.entries(PROPERTIES) as [keyof typeof PROPERTIES, string][]) {
        const value = this.readColor(`--zumen-markup-${name}-${background}`);
        if (value) result[key] = value;
      }
      const over = this.styleOverrides[background];
      if (!over) return result;
      return { ...result, ...over, categories: { ...result.categories, ...over.categories } };
    };
    this.style = { dark: palette("dark"), light: palette("light") };
  }

  private paint(ctx: CanvasRenderingContext2D, view: Readonly<View>, frame: OverlayFrame): void {
    const pixel = 1 / view.scale;
    const { width, height, devicePixelRatio } = frame;
    const background = this.viewer.getPalette().background;
    const palette = this.palette();
    const replaced = new Map<string, MarkupItemInput>();
    const drafts: MarkupItemInput[] = [];
    for (const item of this.preview) {
      if (item.id !== undefined && this.state.document.items.some((other) => other.id === item.id)) {
        replaced.set(item.id, item);
      } else drafts.push(item);
    }
    const drawn = [...this.items().map((item) => replaced.get(item.id) ?? item), ...drafts];
    const shapes = drawn.map((item) => this.shapes(item, pixel));
    renderPrimitives(ctx, shapes.flat(), view, width, height, { background, devicePixelRatio });
    ctx.setTransform(devicePixelRatio, 0, 0, devicePixelRatio, 0, 0);
    const selected = new Set(this.state.selection);
    drawn.forEach((item, index) => {
      const box = shapesBox(shapes[index] as Primitive[]);
      const state = "carried" in item ? item.carried?.state : undefined;
      if (state === "changed" || state === "removed") {
        this.frame(ctx, view, box, CARRIED_PADDING, palette[state], [2, 3]);
      }
      if (item.id === undefined || !selected.has(item.id)) return;
      this.frame(ctx, view, box, SELECTION_PADDING, palette.selection, [5, 3]);
      ctx.setLineDash([]);
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = palette.selection;
      ctx.fillStyle = background;
      for (const { point } of markupHandles(item)) {
        const [px, py] = toScreen(view, point[0], point[1]);
        ctx.fillRect(px - HANDLE_PX / 2, py - HANDLE_PX / 2, HANDLE_PX, HANDLE_PX);
        ctx.strokeRect(px - HANDLE_PX / 2, py - HANDLE_PX / 2, HANDLE_PX, HANDLE_PX);
      }
    });
  }

  /** 用紙の範囲を囲む、画面の上の破線の枠 */
  private frame(
    ctx: CanvasRenderingContext2D,
    view: Readonly<View>,
    box: Box,
    padding: number,
    color: string,
    dashes: number[],
  ): void {
    if (!(box.minX <= box.maxX && box.minY <= box.maxY)) return;
    const corners = [
      toScreen(view, box.minX, box.minY),
      toScreen(view, box.maxX, box.minY),
      toScreen(view, box.maxX, box.maxY),
      toScreen(view, box.minX, box.maxY),
    ];
    const xs = corners.map(([x]) => x);
    const ys = corners.map(([, y]) => y);
    const left = Math.min(...xs) - padding;
    const top = Math.min(...ys) - padding;
    ctx.setLineDash(dashes);
    ctx.lineWidth = 1.5;
    ctx.strokeStyle = color;
    ctx.strokeRect(left, top, Math.max(...xs) + padding - left, Math.max(...ys) + padding - top);
  }
}
