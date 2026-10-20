import type { StructureViewer } from "./viewer.js";

/** 道具に渡すポインターのイベント */
export interface StructureToolEvent {
  /** 表示領域の左上からの位置（CSS px） */
  readonly px: number;
  readonly py: number;
  /** 元のイベント（ボタン・修飾キー・ポインターの種類） */
  readonly event: PointerEvent;
}

/**
 * モデルの上の操作（選択、カメラの操作だけ、利用側の書き込み・ピンなど）。StructureViewer は使用中の道具に
 * ポインターとキーのイベントを表示領域の座標で渡す。道具は viewer の memberAt で下にある部材を調べ、
 * 状態を変える（select・setHovered など）
 */
export interface StructureTool {
  /** 道具の名前。setTool で選び、StructureViewerState.tool に入る */
  readonly name: string;
  /** モデルの上のポインターの形（CSS の cursor）。省略すると grab */
  readonly cursor?: string;
  /** 左ボタン（タッチは 1 本指）のドラッグでカメラを回すか。省略すると回す。false なら道具がドラッグを受ける */
  readonly orbit?: boolean;
  /** 道具を選んだとき */
  activate?(viewer: StructureViewer): void;
  /** ほかの道具に替えるとき */
  deactivate?(viewer: StructureViewer): void;
  pointerDown?(event: StructureToolEvent, viewer: StructureViewer): void;
  /** ポインターが動いたとき。カメラを動かしている間は呼ばない */
  pointerMove?(event: StructureToolEvent, viewer: StructureViewer): void;
  /** pointerDown を受けたボタンを離したとき。カメラを動かした後も呼ぶ */
  pointerUp?(event: StructureToolEvent, viewer: StructureViewer): void;
  /** 押して、ほとんど動かさずに離したとき（pointerUp の後） */
  click?(event: StructureToolEvent, viewer: StructureViewer): void;
  /** ポインターがモデルの外へ出たとき */
  pointerLeave?(viewer: StructureViewer): void;
  /** キーを押したとき。true を返すと、キーの既定の操作（Esc で選択を外す、0 で全体表示など）をしない */
  keyDown?(event: KeyboardEvent, viewer: StructureViewer): boolean | undefined;
  /** 途中の操作をやめる（Esc、モデルの差し替え） */
  cancel?(viewer: StructureViewer): void;
}

/**
 * 選択。クリックで部材を選び、Shift・Ctrl・⌘ を押したクリックで選択に加える・外す。ポインターの下の部材を hovered にする。
 * ドラッグでカメラを回す
 */
export function selectTool(): StructureTool {
  // ポインターの下の部材は、1 フレームに 1 回だけ調べる
  let frame = 0;
  let pending: { px: number; py: number } | null = null;
  const stop = () => {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    pending = null;
  };
  return {
    name: "select",
    pointerMove(event, viewer) {
      pending = { px: event.px, py: event.py };
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        const point = pending;
        pending = null;
        if (point) viewer.setHovered(viewer.memberAt(point.px, point.py));
      });
    },
    pointerLeave(viewer) {
      stop();
      viewer.setHovered(null);
    },
    deactivate(viewer) {
      stop();
      viewer.setHovered(null);
    },
    click(event, viewer) {
      if (event.event.button !== 0) return;
      const member = viewer.memberAt(event.px, event.py);
      const { selection } = viewer.getState();
      if (!(event.event.shiftKey || event.event.ctrlKey || event.event.metaKey)) {
        viewer.select(member ? [member] : [], event.event);
        return;
      }
      if (!member) return;
      viewer.select(
        selection.includes(member) ? selection.filter((item) => item !== member) : [...selection, member],
        event.event,
      );
    },
  };
}

/** カメラの操作だけ。ドラッグで回し、クリックしても選ばない */
export function orbitTool(): StructureTool {
  return { name: "orbit" };
}
