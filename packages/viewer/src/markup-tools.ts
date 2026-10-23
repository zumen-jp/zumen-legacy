import type {
  CloudItem,
  FindingItem,
  MarkupArrows,
  MarkupDocument,
  MarkupItem,
  MarkupKind,
  MarkupPoint,
  MarkupStatus,
  PhotoItem,
  RedlineItem,
  TextItem,
} from "@zumenjs/core";
import { type MarkupItemInput, type MarkupLayer, type MarkupPatch, markupHandles } from "./markup.js";
import type { ToolPointerEvent, ViewerTool } from "./tools.js";
import type { DrawingViewer } from "./viewer.js";

/** 種類 K の、加える前の項目（下書き） */
export type MarkupDraft<K extends MarkupKind = MarkupKind> = Extract<MarkupItemInput, { readonly kind: K }>;

/**
 * 下書きを受け、本文などを入れた項目を返す（利用側の画面で入力してから解決してよい）。null で取りやめる。
 * 返した項目は MarkupLayer.add で加わり、id・author・日時は層の options で決まる
 */
export type MarkupCreate<K extends MarkupKind = MarkupKind> = (
  draft: MarkupDraft<K>,
  layer: MarkupLayer,
) => MarkupItemInput | null | Promise<MarkupItemInput | null>;

/** 項目を描く道具の options */
export interface MarkupToolOptions<K extends MarkupKind> {
  /** 道具の名前（setTool で選ぶ）。省くと markup-<種類> */
  name?: string;
  /** 下書きの分類・色・線の太さ（用紙の mm） */
  category?: string;
  color?: string;
  lineWidth?: number;
  /** 下書きを項目にする。省くと下書きのまま加える */
  create?: MarkupCreate<K>;
}

export interface RedlineToolOptions extends MarkupToolOptions<"redline"> {
  /**
   * line はドラッグか 2 回のクリックで線分、polyline はクリックで点を加えて最後の点をもう一度クリックするか Enter で
   * 終える折線（最初の点をクリックすると閉じる）、freehand はドラッグした跡、rectangle はドラッグか 2 回のクリックで
   * 対角を決める矩形。省くと line
   */
  mode?: "line" | "polyline" | "freehand" | "rectangle";
  arrows?: MarkupArrows;
}

export interface CloudToolOptions extends MarkupToolOptions<"cloud"> {
  /** rectangle はドラッグか 2 回のクリックで対角を決める矩形、polygon はクリックで点を加える多角形。省くと rectangle */
  mode?: "rectangle" | "polygon";
  /** 弧の弦の長さ（用紙の mm）。省くと文書の既定（10 mm） */
  arcLength?: number;
}

export interface TextToolOptions extends MarkupToolOptions<"text"> {
  /** 文字の高さ（用紙の mm）と角度（度）。省くと文書の既定（3.5 mm、0 度） */
  height?: number;
  angle?: number;
}

export interface FindingToolOptions extends MarkupToolOptions<"finding"> {
  /** 下書きの状態。省くと open */
  status?: MarkupStatus;
  /** 下書きの label を、文書の指摘の番号（数だけの label）の最大 + 1 にする。省くと true */
  numbered?: boolean;
}

export type PhotoToolOptions = MarkupToolOptions<"photo">;

export interface MarkupEditToolOptions {
  /** 道具の名前。省くと markup-edit */
  name?: string;
}

/** これより動いたらクリックでなくドラッグとみなす（CSS px）。DrawingViewer と同じ */
const DRAG_PX = 4;
/** 折線の最初・最後の点をクリックしたとみなす距離、点のつまみをつかむ距離（CSS px） */
const CLOSE_PX = 8;
/** 手書きの線で、前の点からこれより離れたら点を加える（CSS px） */
const FREEHAND_PX = 2;

/** 用紙の mm を 1 µm に丸める（文書の数を短くする） */
function round(value: number): number {
  return Math.round(value * 1000) / 1000;
}

function point(event: ToolPointerEvent): MarkupPoint {
  return [round(event.x), round(event.y)];
}

function screenDistance(viewer: DrawingViewer, a: MarkupPoint, px: number, py: number): number {
  const [x, y] = viewer.toScreen(a[0], a[1]);
  return Math.hypot(x - px, y - py);
}

/** Ctrl・⌘ と Z で取り消し、Shift も押すか Ctrl と Y でやり直す */
function historyKey(event: KeyboardEvent, layer: MarkupLayer): boolean {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return false;
  const key = event.key.toLowerCase();
  if (key === "z") {
    if (event.shiftKey) layer.redo();
    else layer.undo();
    return true;
  }
  if (key === "y" && event.ctrlKey) {
    layer.redo();
    return true;
  }
  return false;
}

/** 下書きの、種類によらないキー */
function base<K extends MarkupKind>(
  kind: K,
  options: Pick<MarkupToolOptions<K>, "category" | "color" | "lineWidth">,
  viewer: DrawingViewer,
) {
  const view = viewer.scene.view;
  return {
    kind,
    ...(view === undefined ? {} : { view }),
    ...(options.category === undefined ? {} : { category: options.category }),
    ...(options.color === undefined ? {} : { color: options.color }),
    ...(options.lineWidth === undefined ? {} : { lineWidth: options.lineWidth }),
  };
}

/** 下書きを create に渡して加える。create が終わるまで下書きを描き、道具は入力を受けない */
class Creator<K extends MarkupKind> {
  pending = false;

  constructor(
    private readonly layer: MarkupLayer,
    private readonly create: MarkupCreate<K> | undefined,
  ) {}

  finish(draft: MarkupDraft<K>): void {
    const result = this.create ? this.create(draft, this.layer) : draft;
    if (!(result instanceof Promise)) {
      this.layer.setPreview(null);
      if (result) this.layer.add(result);
      return;
    }
    this.pending = true;
    this.layer.setPreview(draft);
    result
      .then((item) => {
        if (item) this.layer.add(item);
      })
      .finally(() => {
        this.pending = false;
        this.layer.setPreview(null);
      });
  }
}

/** 2 点で決める形（線分・矩形）。ドラッグか、2 回のクリックで決める */
function twoPointTool<K extends MarkupKind>(
  layer: MarkupLayer,
  name: string,
  create: MarkupCreate<K> | undefined,
  draft: (start: MarkupPoint, end: MarkupPoint, viewer: DrawingViewer) => MarkupDraft<K>,
): ViewerTool {
  const creator = new Creator(layer, create);
  let start: MarkupPoint | null = null;
  let down: [number, number] | null = null;
  let waiting = false;
  const reset = () => {
    start = null;
    down = null;
    waiting = false;
    if (!creator.pending) layer.setPreview(null);
  };
  return {
    name,
    cursor: "crosshair",
    dragPan: false,
    doubleClickZoom: false,
    deactivate: reset,
    cancel: reset,
    pointerDown(event) {
      if (event.event.button !== 0 || creator.pending) return;
      start ??= point(event);
      down = [event.px, event.py];
    },
    pointerMove(event, viewer) {
      if (start) layer.setPreview(draft(start, point(event), viewer));
    },
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || !start || !down) return;
      const moved = Math.hypot(event.px - down[0], event.py - down[1]) > DRAG_PX;
      if (!moved && !waiting) {
        waiting = true;
        return;
      }
      const from = start;
      reset();
      if (screenDistance(viewer, from, event.px, event.py) <= DRAG_PX) return;
      creator.finish(draft(from, point(event), viewer));
    },
    keyDown(event) {
      return !start && historyKey(event, layer);
    },
  };
}

/** クリックで点を加える形（折線・多角形）。最後の点をもう一度クリックするか Enter で終え、最初の点のクリックで閉じる */
function pointsTool<K extends MarkupKind>(
  layer: MarkupLayer,
  name: string,
  create: MarkupCreate<K> | undefined,
  minimum: number,
  draft: (points: readonly MarkupPoint[], closed: boolean, viewer: DrawingViewer) => MarkupDraft<K>,
  sketch: (points: readonly MarkupPoint[], viewer: DrawingViewer) => MarkupItemInput = (points, viewer) =>
    draft(points, false, viewer),
): ViewerTool {
  const creator = new Creator(layer, create);
  let points: MarkupPoint[] = [];
  const reset = () => {
    points = [];
    if (!creator.pending) layer.setPreview(null);
  };
  const finish = (closed: boolean, viewer: DrawingViewer) => {
    const done = points;
    reset();
    if (done.length >= minimum) creator.finish(draft(done, closed, viewer));
  };
  return {
    name,
    cursor: "crosshair",
    dragPan: false,
    doubleClickZoom: false,
    deactivate: reset,
    cancel: reset,
    pointerMove(event, viewer) {
      if (points.length > 0) layer.setPreview(sketch([...points, point(event)], viewer));
    },
    // 指で押すと離すまでに少し動くので、離した位置で点を決める
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || creator.pending) return;
      const first = points[0];
      const last = points[points.length - 1];
      if (first && points.length >= minimum && screenDistance(viewer, first, event.px, event.py) <= CLOSE_PX) {
        finish(true, viewer);
        return;
      }
      if (last && screenDistance(viewer, last, event.px, event.py) <= CLOSE_PX) {
        finish(false, viewer);
        return;
      }
      points = [...points, point(event)];
      layer.setPreview(sketch(points, viewer));
    },
    keyDown(event, viewer) {
      if (points.length === 0) return historyKey(event, layer);
      if (event.key === "Enter") {
        finish(false, viewer);
        return true;
      }
      if (event.key === "Backspace") {
        points = points.slice(0, -1);
        if (points.length === 0) reset();
        else layer.setPreview(sketch(points, viewer));
        return true;
      }
      return false;
    },
  };
}

/**
 * 赤入れの道具（線分・折線・手書きの線・矩形）。描き終えた下書きを options.create に渡して加える。
 * Ctrl・⌘ と Z で取り消し、Shift も押すとやり直す
 */
export function redlineTool(layer: MarkupLayer, options: RedlineToolOptions = {}): ViewerTool {
  const mode = options.mode ?? "line";
  const name = options.name ?? "markup-redline";
  const arrows = options.arrows === undefined || options.arrows === "none" ? {} : { arrows: options.arrows };
  const redline = (points: readonly MarkupPoint[], closed: boolean, viewer: DrawingViewer): MarkupDraft<"redline"> =>
    ({ ...base("redline", options, viewer), points, ...(closed ? { closed: true } : arrows) }) as RedlineItem;
  switch (mode) {
    case "line":
      return twoPointTool(layer, name, options.create, (a, b, viewer) => redline([a, b], false, viewer));
    case "rectangle":
      return twoPointTool(layer, name, options.create, (a, b, viewer) =>
        redline(
          [
            [a[0], a[1]],
            [b[0], a[1]],
            [b[0], b[1]],
            [a[0], b[1]],
          ],
          true,
          viewer,
        ),
      );
    case "polyline":
      return pointsTool(layer, name, options.create, 2, redline);
    case "freehand": {
      const creator = new Creator(layer, options.create);
      let points: MarkupPoint[] | null = null;
      const reset = () => {
        points = null;
        if (!creator.pending) layer.setPreview(null);
      };
      return {
        name,
        cursor: "crosshair",
        dragPan: false,
        deactivate: reset,
        cancel: reset,
        pointerDown(event, viewer) {
          if (event.event.button !== 0 || creator.pending) return;
          points = [point(event)];
          layer.setPreview(redline(points, false, viewer));
        },
        pointerMove(event, viewer) {
          const last = points?.[points.length - 1];
          if (!points || !last || screenDistance(viewer, last, event.px, event.py) < FREEHAND_PX) return;
          points = [...points, point(event)];
          layer.setPreview(redline(points, false, viewer));
        },
        pointerUp(event, viewer) {
          if (event.event.button !== 0 || !points) return;
          const last = points[points.length - 1];
          const done =
            last && (last[0] !== round(event.x) || last[1] !== round(event.y)) ? [...points, point(event)] : points;
          reset();
          if (done.length >= 2) creator.finish(redline(done, false, viewer));
        },
        keyDown(event) {
          return !points && historyKey(event, layer);
        },
      };
    }
  }
}

/** 雲マークの道具（矩形・多角形） */
export function cloudTool(layer: MarkupLayer, options: CloudToolOptions = {}): ViewerTool {
  const name = options.name ?? "markup-cloud";
  const arcLength = options.arcLength === undefined ? {} : { arcLength: options.arcLength };
  const cloud = (points: readonly MarkupPoint[], viewer: DrawingViewer): MarkupDraft<"cloud"> =>
    ({ ...base("cloud", options, viewer), points, ...arcLength }) as CloudItem;
  if (options.mode === "polygon") {
    // 描きかけの 2 点までは線で示す（雲マークは 3 点から）
    return pointsTool(
      layer,
      name,
      options.create,
      3,
      (points, _closed, viewer) => cloud(points, viewer),
      (points, viewer) =>
        points.length >= 3 ? cloud(points, viewer) : ({ ...base("redline", options, viewer), points } as RedlineItem),
    );
  }
  return twoPointTool(layer, name, options.create, (a, b, viewer) =>
    cloud(
      [
        [a[0], a[1]],
        [b[0], a[1]],
        [b[0], b[1]],
        [a[0], b[1]],
      ],
      viewer,
    ),
  );
}

/** クリックした位置に置く項目の道具 */
function placeTool<K extends MarkupKind>(
  layer: MarkupLayer,
  name: string,
  create: MarkupCreate<K> | undefined,
  draft: (event: ToolPointerEvent, viewer: DrawingViewer) => MarkupDraft<K>,
): ViewerTool {
  const creator = new Creator(layer, create);
  return {
    name,
    cursor: "crosshair",
    // 指で押すと離すまでに少し動くので、離した位置に置く
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || creator.pending) return;
      creator.finish(draft(event, viewer));
    },
    keyDown(event) {
      return historyKey(event, layer);
    },
  };
}

/** メモの道具。クリックした位置を 1 行目の左下にし、本文は options.create で入れる */
export function textTool(layer: MarkupLayer, options: TextToolOptions = {}): ViewerTool {
  return placeTool(
    layer,
    options.name ?? "markup-text",
    options.create,
    (event, viewer) =>
      ({
        ...base("text", options, viewer),
        position: point(event),
        text: "",
        ...(options.height === undefined ? {} : { height: options.height }),
        ...(options.angle === undefined ? {} : { angle: options.angle }),
      }) as TextItem,
  );
}

/** 文書の指摘の、数だけの label の最大 + 1 */
function nextLabel(document: MarkupDocument): string {
  let max = 0;
  for (const item of document.items) {
    if (item.kind === "finding" && item.label && /^\d+$/.test(item.label)) max = Math.max(max, Number(item.label));
  }
  return String(max + 1);
}

/**
 * 指摘の道具。クリックでピンを置き、その下の図形を entities にする。ドラッグで範囲（矩形）を決め、ピンを範囲の
 * 右上（用紙の座標）に置く。本文は options.create で入れる
 */
export function findingTool(layer: MarkupLayer, options: FindingToolOptions = {}): ViewerTool {
  const creator = new Creator(layer, options.create);
  let start: { point: MarkupPoint; px: number; py: number } | null = null;
  let area: readonly MarkupPoint[] | null = null;
  const reset = () => {
    start = null;
    area = null;
    if (!creator.pending) layer.setPreview(null);
  };
  const finding = (position: MarkupPoint, viewer: DrawingViewer, extra: object): MarkupDraft<"finding"> =>
    ({
      ...base("finding", options, viewer),
      position,
      status: options.status ?? "open",
      ...(options.numbered === false ? {} : { label: nextLabel(layer.document) }),
      ...extra,
    }) as FindingItem;
  const rectangle = (a: MarkupPoint, b: MarkupPoint): MarkupPoint[] => [
    [Math.min(a[0], b[0]), Math.min(a[1], b[1])],
    [Math.max(a[0], b[0]), Math.min(a[1], b[1])],
    [Math.max(a[0], b[0]), Math.max(a[1], b[1])],
    [Math.min(a[0], b[0]), Math.max(a[1], b[1])],
  ];
  return {
    name: options.name ?? "markup-finding",
    cursor: "crosshair",
    dragPan: false,
    deactivate: reset,
    cancel: reset,
    pointerDown(event) {
      if (event.event.button !== 0 || creator.pending) return;
      start = { point: point(event), px: event.px, py: event.py };
    },
    pointerMove(event, viewer) {
      if (!start) return;
      if (!area && Math.hypot(event.px - start.px, event.py - start.py) <= DRAG_PX) return;
      area = rectangle(start.point, point(event));
      layer.setPreview(finding(area[2] as MarkupPoint, viewer, { points: area }));
    },
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || !start) return;
      const points = area;
      const position = point(event);
      reset();
      if (points) {
        creator.finish(finding(points[2] as MarkupPoint, viewer, { points }));
        return;
      }
      const anchor = viewer.entitiesAt(event.x, event.y)[0];
      creator.finish(finding(position, viewer, anchor ? { entities: [anchor.id] } : {}));
    },
    keyDown(event) {
      return !start && historyKey(event, layer);
    },
  };
}

/** 写真の位置の道具。押した位置に置き、ドラッグした向きをカメラの向きにする。写真の参照は options.create で入れる */
export function photoTool(layer: MarkupLayer, options: PhotoToolOptions = {}): ViewerTool {
  const creator = new Creator(layer, options.create);
  let start: { point: MarkupPoint; px: number; py: number } | null = null;
  let direction: number | null = null;
  const reset = () => {
    start = null;
    direction = null;
    if (!creator.pending) layer.setPreview(null);
  };
  const photo = (position: MarkupPoint, viewer: DrawingViewer): MarkupDraft<"photo"> =>
    ({
      ...base("photo", options, viewer),
      position,
      photo: "",
      ...(direction === null ? {} : { direction }),
    }) as PhotoItem;
  return {
    name: options.name ?? "markup-photo",
    cursor: "crosshair",
    dragPan: false,
    deactivate: reset,
    cancel: reset,
    pointerDown(event, viewer) {
      if (event.event.button !== 0 || creator.pending) return;
      start = { point: point(event), px: event.px, py: event.py };
      layer.setPreview(photo(start.point, viewer));
    },
    pointerMove(event, viewer) {
      if (!start || Math.hypot(event.px - start.px, event.py - start.py) <= DRAG_PX) return;
      const angle = (Math.atan2(event.y - start.point[1], event.x - start.point[0]) * 180) / Math.PI;
      direction = Math.round((((angle % 360) + 360) % 360) * 10) / 10;
      layer.setPreview(photo(start.point, viewer));
    },
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || !start) return;
      const draft = photo(start.point, viewer);
      reset();
      creator.finish(draft);
    },
    keyDown(event) {
      return !start && historyKey(event, layer);
    },
  };
}

/** 項目の点を (dx, dy) だけ動かす値 */
function moved(item: MarkupItem, dx: number, dy: number): MarkupPatch {
  const shift = ([x, y]: MarkupPoint): MarkupPoint => [round(x + dx), round(y + dy)];
  const patch: Record<string, unknown> = {};
  if ("points" in item && item.points) patch.points = item.points.map(shift);
  if ("position" in item) patch.position = shift(item.position);
  return patch as MarkupPatch;
}

/** 項目の 1 つの点を置き換える値 */
function movedPoint(item: MarkupItem, key: "points" | "position", index: number, to: MarkupPoint): MarkupPatch {
  if (key === "position") return { position: to } as MarkupPatch;
  const points = "points" in item && item.points ? [...item.points] : [];
  points[index] = to;
  return { points } as MarkupPatch;
}

/**
 * マークアップの編集の道具。クリックで項目を選び（Shift・Ctrl・⌘ で加える・外す）、ドラッグで選んだ項目を動かし、
 * 選んだ項目の点のつまみをドラッグして点を動かす。何もない所のドラッグは図面を動かす。Delete・Backspace で選んだ項目を
 * 消し、Esc で選択を外し、Ctrl・⌘ と Z で取り消す。動かしている間は文書を変えず、離したときに 1 回の変更にする
 */
export function markupEditTool(layer: MarkupLayer, options: MarkupEditToolOptions = {}): ViewerTool {
  type Drag =
    | { kind: "move"; items: MarkupItem[]; from: MarkupPoint }
    | { kind: "point"; item: MarkupItem; key: "points" | "position"; index: number }
    | { kind: "pan"; px: number; py: number }
    | { kind: "toggle"; id: string };
  let drag: Drag | null = null;
  let down: { px: number; py: number; moved: boolean } | null = null;
  const reset = (viewer: DrawingViewer) => {
    drag = null;
    down = null;
    layer.setPreview(null);
    viewer.canvas.style.cursor = "";
  };
  const handleAt = (event: ToolPointerEvent, viewer: DrawingViewer) => {
    const selected = new Set(layer.getState().selection);
    for (const item of layer.items().reverse()) {
      if (!selected.has(item.id)) continue;
      for (const handle of markupHandles(item)) {
        if (screenDistance(viewer, handle.point, event.px, event.py) <= CLOSE_PX) return { item, ...handle };
      }
    }
    return null;
  };
  const preview = (event: ToolPointerEvent) => {
    if (!drag) return;
    if (drag.kind === "move") {
      const dx = event.x - drag.from[0];
      const dy = event.y - drag.from[1];
      layer.setPreview(drag.items.map((item) => ({ ...item, ...moved(item, dx, dy) }) as MarkupItem));
    } else if (drag.kind === "point") {
      layer.setPreview({ ...drag.item, ...movedPoint(drag.item, drag.key, drag.index, point(event)) } as MarkupItem);
    }
  };
  return {
    name: options.name ?? "markup-edit",
    dragPan: false,
    activate(viewer) {
      reset(viewer);
    },
    deactivate(viewer) {
      reset(viewer);
      layer.setHovered(null);
    },
    cancel(viewer) {
      reset(viewer);
    },
    pointerDown(event, viewer) {
      if (event.event.button !== 0) return;
      down = { px: event.px, py: event.py, moved: false };
      const handle = handleAt(event, viewer);
      if (handle) {
        drag = { kind: "point", item: handle.item, key: handle.key, index: handle.index };
        return;
      }
      const item = layer.itemsAt(event.x, event.y)[0];
      if (!item) {
        drag = { kind: "pan", px: event.px, py: event.py };
        return;
      }
      const { selection } = layer.getState();
      const additive = event.event.shiftKey || event.event.ctrlKey || event.event.metaKey;
      if (!selection.includes(item.id)) layer.select(additive ? [...selection, item.id] : [item.id]);
      else if (additive) {
        // 選んである項目の Shift のクリックは、動かさずに離したときに外す
        drag = { kind: "toggle", id: item.id };
        return;
      }
      const ids = new Set(layer.getState().selection);
      drag = { kind: "move", items: layer.items().filter((other) => ids.has(other.id)), from: [event.x, event.y] };
    },
    pointerMove(event, viewer) {
      if (!drag || !down) {
        const handle = handleAt(event, viewer);
        const item = handle ? handle.item : layer.itemsAt(event.x, event.y)[0];
        layer.setHovered(item?.id ?? null);
        viewer.canvas.style.cursor = handle ? "crosshair" : item ? "move" : "";
        return;
      }
      if (!down.moved && Math.hypot(event.px - down.px, event.py - down.py) <= DRAG_PX) return;
      down.moved = true;
      if (drag.kind === "pan") {
        viewer.panBy(event.px - drag.px, event.py - drag.py);
        drag = { kind: "pan", px: event.px, py: event.py };
        return;
      }
      if (drag.kind === "toggle") {
        const ids = new Set(layer.getState().selection);
        drag = { kind: "move", items: layer.items().filter((item) => ids.has(item.id)), from: [event.x, event.y] };
      }
      preview(event);
    },
    pointerUp(event, viewer) {
      if (event.event.button !== 0 || !drag || !down) return;
      const current = drag;
      const wasMoved = down.moved;
      reset(viewer);
      if (!wasMoved) {
        if (current.kind === "pan") layer.select([]);
        else if (current.kind === "toggle") layer.select(layer.getState().selection.filter((id) => id !== current.id));
        return;
      }
      if (current.kind === "move") {
        const dx = event.x - current.from[0];
        const dy = event.y - current.from[1];
        layer.update(current.items.map((item) => ({ id: item.id, patch: moved(item, dx, dy) })));
      } else if (current.kind === "point") {
        layer.update(current.item.id, movedPoint(current.item, current.key, current.index, point(event)));
      }
    },
    pointerLeave() {
      if (!drag) layer.setHovered(null);
    },
    keyDown(event, viewer) {
      if (historyKey(event, layer)) {
        reset(viewer);
        return true;
      }
      const { selection } = layer.getState();
      if ((event.key === "Delete" || event.key === "Backspace") && selection.length > 0) {
        reset(viewer);
        layer.remove(selection);
        return true;
      }
      if (event.key === "Escape") {
        reset(viewer);
        layer.select([]);
      }
      return false;
    },
  };
}
