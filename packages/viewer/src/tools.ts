import type { MeasurePoint } from "@zumenjs/core";
import type { SnapPoint } from "./pick.js";
import type { DrawingViewer } from "./viewer.js";

/** 道具に渡すポインターのイベント */
export interface ToolPointerEvent {
  /** ポインターの下の用紙の座標（mm） */
  readonly x: number;
  readonly y: number;
  /** 表示領域の左上からの位置（CSS px） */
  readonly px: number;
  readonly py: number;
  /** 元のイベント（ボタン・修飾キー・ポインターの種類） */
  readonly event: PointerEvent;
}

/**
 * 図面の上の操作（選択・計測・移動、利用側の書き込み・ピン・範囲など）。DrawingViewer は使用中の道具に
 * ポインターとキーのイベントを用紙の座標で渡す。道具は viewer の entitiesAt・snapAt で下にある図形と吸着点を調べ、
 * 状態を変え（select・setMeasurement など）、draw で重ね描きの Canvas に描く。中身を変えたら viewer.invalidate() を呼ぶ
 */
export interface ViewerTool {
  /** 道具の名前。setTool で選び、ViewerState.tool に入る */
  readonly name: string;
  /** 図面の上のポインターの形（CSS の cursor） */
  readonly cursor?: string;
  /** 左ボタン（タッチは 1 本指）のドラッグで図面を動かすか。省略すると動かす。false なら道具がドラッグを受ける */
  readonly dragPan?: boolean;
  /** 道具を選んだとき */
  activate?(viewer: DrawingViewer): void;
  /** ほかの道具に替えるとき */
  deactivate?(viewer: DrawingViewer): void;
  pointerDown?(event: ToolPointerEvent, viewer: DrawingViewer): void;
  /** ポインターが動いたとき。図面を動かしている間は呼ばない */
  pointerMove?(event: ToolPointerEvent, viewer: DrawingViewer): void;
  /** pointerDown を受けたボタンを離したとき。図面を動かした後も呼ぶ */
  pointerUp?(event: ToolPointerEvent, viewer: DrawingViewer): void;
  /** 押して、ほとんど動かさずに離したとき（pointerUp の後） */
  click?(event: ToolPointerEvent, viewer: DrawingViewer): void;
  /** ポインターが図面の外へ出たとき */
  pointerLeave?(viewer: DrawingViewer): void;
  /** キーを押したとき。true を返すと、キーの既定の操作（拡大・移動・Esc で消す）をしない */
  keyDown?(event: KeyboardEvent, viewer: DrawingViewer): boolean | undefined;
  /** 途中の操作をやめる（Esc、2 本指の操作、図面の差し替え） */
  cancel?(viewer: DrawingViewer): void;
  /** 重ね描きの Canvas に描く。ctx は CSS px の座標 */
  draw?(ctx: CanvasRenderingContext2D, viewer: DrawingViewer): void;
}

/** 選択。クリックで図形を選び、Shift・Ctrl・⌘ を押したクリックで選択に加える・外す。ドラッグで図面を動かす */
export function selectTool(): ViewerTool {
  return {
    name: "select",
    pointerMove(event, viewer) {
      viewer.setHovered(viewer.entitiesAt(event.x, event.y)[0] ?? null);
    },
    pointerLeave(viewer) {
      viewer.setHovered(null);
    },
    deactivate(viewer) {
      viewer.setHovered(null);
    },
    click(event, viewer) {
      if (event.event.button !== 0) return;
      const entity = viewer.entitiesAt(event.x, event.y)[0] ?? null;
      const { selection } = viewer.getState();
      if (!(event.event.shiftKey || event.event.ctrlKey || event.event.metaKey)) {
        viewer.select(entity ? [entity] : []);
        return;
      }
      if (!entity) return;
      viewer.select(selection.includes(entity) ? selection.filter((item) => item !== entity) : [...selection, entity]);
    },
  };
}

/**
 * 距離の計測。クリックで始点、次のクリックで終点を決め、その次のクリックで測り直す。
 * 端点・頂点、なければ線上の最も近い点に吸着する。図面は中ボタン・右ボタンのドラッグで動かす
 */
export function measureTool(): ViewerTool {
  /** ポインターの下の吸着点 */
  let hover: SnapPoint | null = null;
  const setHover = (next: SnapPoint | null, viewer: DrawingViewer) => {
    hover = next;
    viewer.invalidate();
  };
  return {
    name: "measure",
    cursor: "crosshair",
    dragPan: false,
    activate(viewer) {
      hover = null;
      viewer.setMeasurement(null);
    },
    deactivate(viewer) {
      setHover(null, viewer);
    },
    cancel(viewer) {
      setHover(null, viewer);
    },
    pointerMove(event, viewer) {
      setHover(viewer.snapAt(event.x, event.y), viewer);
    },
    pointerLeave(viewer) {
      setHover(null, viewer);
    },
    // 指で押すと離すまでに少し動くので、クリックでなく、離した位置で点を決める
    pointerUp(event, viewer) {
      if (event.event.button !== 0) return;
      const snapped = viewer.snapAt(event.x, event.y);
      const point: MeasurePoint = snapped
        ? { x: snapped.x, y: snapped.y, entity: snapped.entity }
        : { x: event.x, y: event.y };
      const current = viewer.getState().measurement;
      viewer.setMeasurement(!current || current.end ? { start: point, end: null } : { ...current, end: point });
    },
    draw(ctx, viewer) {
      const { measurement } = viewer.getState();
      if (!hover) return;
      const [hx, hy] = viewer.toScreen(hover.x, hover.y);
      ctx.strokeStyle = viewer.getPalette().measure;
      ctx.fillStyle = viewer.getPalette().measure;
      ctx.lineWidth = 2;
      if (measurement && !measurement.end) {
        // 終点を選ぶ前は、始点から吸着点までを示す
        const [sx, sy] = viewer.toScreen(measurement.start.x, measurement.start.y);
        ctx.setLineDash([6, 4]);
        ctx.beginPath();
        ctx.moveTo(sx, sy);
        ctx.lineTo(hx, hy);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.beginPath();
        ctx.arc(hx, hy, 4, 0, Math.PI * 2);
        ctx.fill();
        return;
      }
      // 吸着点の印。頂点は四角、線上の点は ×
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
    },
  };
}

/** 移動。ドラッグで図面を動かすだけで、クリックしても選ばない */
export function panTool(): ViewerTool {
  return { name: "pan", cursor: "grab" };
}
