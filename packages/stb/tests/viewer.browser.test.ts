import { afterEach, describe, expect, it } from "vitest";
import {
  type StructureTool,
  type StructureViewEvent,
  StructureViewer,
  type StructureViewerOptions,
} from "../src/three/index.js";
import type { StructureMember, Vec3 } from "../src/types.js";
import { structure } from "./structure-fixture.js";

// 3D の表示と操作のエンジンを Chromium（WebGL）で動かす。利用者の操作は合成したイベントで、描いた結果は Canvas の画素で確かめる

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

const frames = async (count = 2) => {
  for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
};

async function mount(options: StructureViewerOptions = {}, css = "", model = structure()) {
  const container = document.createElement("div");
  container.style.cssText = `width: 400px; height: 300px; ${css}`;
  document.body.append(container);
  const viewer = new StructureViewer(container, model, { theme: "light", ...options });
  cleanups.push(() => {
    viewer.destroy();
    container.remove();
  });
  await frames();
  return { viewer, container, model };
}

const member = (viewer: StructureViewer, key: string) =>
  viewer.model.members.find((item) => item.key === key) as StructureMember;

/** 部材の中央が見える表示領域の点 */
function screenOf(viewer: StructureViewer, point: Vec3): [number, number] {
  const p = viewer.project(point);
  if (!p) throw new Error("見えない");
  return p;
}

/** Canvas の上の点 (px, py) で合成したポインターのイベントを送る */
function pointer(
  viewer: StructureViewer,
  type: string,
  px: number,
  py: number,
  init: PointerEventInit = {},
): PointerEvent {
  const rect = viewer.canvas.getBoundingClientRect();
  const event = new PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    composed: true,
    pointerId: 1,
    pointerType: "mouse",
    isPrimary: true,
    clientX: rect.left + px,
    clientY: rect.top + py,
    button: 0,
    buttons: type === "pointerup" ? 0 : 1,
    ...init,
  });
  viewer.canvas.dispatchEvent(event);
  return event;
}

function click(viewer: StructureViewer, px: number, py: number, init: PointerEventInit = {}): PointerEvent {
  pointer(viewer, "pointerdown", px, py, init);
  return pointer(viewer, "pointerup", px, py, init);
}

function drag(viewer: StructureViewer, from: [number, number], dx: number, dy: number, init: PointerEventInit = {}) {
  pointer(viewer, "pointerdown", from[0], from[1], init);
  for (let i = 1; i <= 5; i++) pointer(viewer, "pointermove", from[0] + (dx * i) / 5, from[1] + (dy * i) / 5, init);
  pointer(viewer, "pointerup", from[0] + dx, from[1] + dy, init);
}

function key(viewer: StructureViewer, value: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key: value, code: value, bubbles: true, cancelable: true, ...init });
  viewer.canvas.dispatchEvent(event);
  return event;
}

function wheel(viewer: StructureViewer, deltaY: number, init: WheelEventInit = {}): WheelEvent {
  const rect = viewer.canvas.getBoundingClientRect();
  const event = new WheelEvent("wheel", {
    bubbles: true,
    cancelable: true,
    clientX: rect.left + 200,
    clientY: rect.top + 150,
    deltaY,
    ...init,
  });
  viewer.canvas.dispatchEvent(event);
  return event;
}

/** 今の画面を描いた画像の、表示領域の点 (px, py) の色（RGB） */
async function pixel(viewer: StructureViewer, px: number, py: number): Promise<[number, number, number]> {
  const image = new Image();
  image.src = viewer.snapshot();
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = image.width;
  canvas.height = image.height;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  ctx.drawImage(image, 0, 0);
  const scale = image.width / viewer.canvas.getBoundingClientRect().width;
  const [r = 0, g = 0, b = 0] = ctx.getImageData(Math.round(px * scale), Math.round(py * scale), 1, 1).data;
  return [r, g, b];
}

const close = (a: readonly number[], b: readonly number[], tolerance = 1e-6) =>
  a.every((value, index) => Math.abs(value - (b[index] as number)) <= tolerance);

function record(viewer: StructureViewer) {
  const moves: StructureViewEvent[] = [];
  const ends: StructureViewEvent[] = [];
  viewer.on("move", (event) => moves.push(event));
  viewer.on("moveend", (event) => ends.push(event));
  return { moves, ends };
}

describe("状態", () => {
  it("変わるたびに新しいオブジェクトになり、変わらなければ同じものを返す", async () => {
    const { viewer } = await mount();
    const initial = viewer.getState();
    expect(initial.visibleCount).toBe(4);
    expect(initial.projection).toBe("perspective");
    expect(initial.guides).toEqual({ axes: true, stories: true, labels: true, nodes: false });
    let calls = 0;
    viewer.subscribe(() => calls++);
    viewer.setColorMode("kind");
    viewer.setGuides({ axes: true });
    expect(calls).toBe(0);
    expect(viewer.getState()).toBe(initial);
    viewer.setKindsVisible(["column"], false);
    expect(calls).toBeGreaterThan(0);
    expect(viewer.getState()).not.toBe(initial);
    expect(viewer.getState().hiddenKinds).toEqual(new Set(["column"]));
    expect(viewer.getState().visibleCount).toBe(2);
  });

  it("絞り込みで隠れた部材を選択から外し、select を送る", async () => {
    const { viewer } = await mount();
    const selections: (readonly StructureMember[])[] = [];
    viewer.on("select", (event) => {
      selections.push(event.selection);
      expect(event.originalEvent).toBeNull();
    });
    viewer.select([member(viewer, "C1"), member(viewer, "G1")]);
    viewer.setHiddenKinds(["girder"]);
    expect(viewer.getState().selection.map((item) => item.key)).toEqual(["C1"]);
    expect(selections.map((list) => list.map((item) => item.key))).toEqual([["C1", "G1"], ["C1"]]);
    // 見えない部材は選べない
    viewer.select([member(viewer, "G1")]);
    expect(viewer.getState().selection).toEqual([]);
  });

  it("階に絞ると平行投影で真上から見て、通り芯の絞り込みを外す。解除すると斜めから見る", async () => {
    const { viewer } = await mount();
    viewer.focusAxis("X1");
    expect(viewer.getState().axis?.key).toBe("X1");
    viewer.focusStory(2);
    const state = viewer.getState();
    expect(state.story).toBe(2);
    expect(state.axis).toBeNull();
    expect(state.visibleCount).toBe(1);
    expect(state.projection).toBe("orthographic");
    const view = viewer.getView();
    expect(view.position[2]).toBeGreaterThan(view.target[2]);
    expect(Math.abs(view.position[0] - view.target[0])).toBeLessThan(1);
    viewer.focusStory(null);
    expect(viewer.getState().story).toBeNull();
    expect(viewer.getState().visibleCount).toBe(4);
    const iso = viewer.getView();
    expect(iso.position[0]).toBeLessThan(iso.target[0]);
    expect(iso.position[1]).toBeLessThan(iso.target[1]);
  });

  it("通り芯に絞ると通りの上の部材だけを残し、通りに直交する向きから見る", async () => {
    const { viewer } = await mount();
    viewer.focusAxis("X2");
    const state = viewer.getState();
    expect(state.axis?.name).toBe("X2");
    expect(state.visibleCount).toBe(1);
    expect(state.projection).toBe("orthographic");
    // Y 方向の通りは +X 側から見る
    const view = viewer.getView();
    expect(view.position[0]).toBeGreaterThan(view.target[0]);
    expect(Math.abs(view.position[1] - view.target[1])).toBeLessThan(1);
    // camera: false ならカメラは動かさない
    viewer.focusAxis(null, { camera: false });
    expect(viewer.getView()).toEqual(view);
    expect(viewer.getState().axis).toBeNull();
  });

  it("setModel で絞り込み・選択・投影を初期化し、色分けと補助の表示は保つ", async () => {
    const { viewer } = await mount();
    viewer.setColorMode("structure");
    viewer.setGuides({ nodes: true });
    viewer.focusStory(1);
    viewer.select([member(viewer, "C1")]);
    const next = structure();
    viewer.setModel(next);
    const state = viewer.getState();
    expect(state.model).toBe(next);
    expect(state.story).toBeNull();
    expect(state.selection).toEqual([]);
    expect(state.projection).toBe("perspective");
    expect(state.colorMode).toBe("structure");
    expect(state.guides.nodes).toBe(true);
  });
});

describe("カメラ", () => {
  it("getView の値を jumpTo に渡すと同じカメラに戻り、変わらなければイベントを送らない", async () => {
    const { viewer } = await mount();
    const saved = viewer.getView();
    viewer.viewFrom("top");
    const { moves, ends } = record(viewer);
    viewer.jumpTo(saved);
    const back = viewer.getView();
    expect(back.position).toEqual(saved.position);
    expect(back.target).toEqual(saved.target);
    expect(back.span).toBeCloseTo(saved.span, 9);
    expect(moves).toHaveLength(1);
    expect(ends).toHaveLength(1);
    expect(moves[0]?.originalEvent).toBeNull();
    viewer.jumpTo(viewer.getView());
    expect(moves).toHaveLength(1);
  });

  it("平行投影では span が見える高さになり、投影を替えても注視点は変わらない", async () => {
    const { viewer } = await mount();
    const target = viewer.getView().target;
    viewer.setProjection("orthographic");
    expect(viewer.getState().projection).toBe("orthographic");
    expect(close(viewer.getView().target, target)).toBe(true);
    viewer.jumpTo({ span: 5000 });
    expect(viewer.getView().span).toBe(5000);
    viewer.setProjection("perspective");
    expect(viewer.getView().span).toBeCloseTo(5000, 6);
  });

  it("project でモデルの点の画面の位置を求め、カメラの後ろの点は null", async () => {
    const { viewer } = await mount();
    const view = viewer.getView();
    const [px, py] = screenOf(viewer, view.target);
    expect(px).toBeCloseTo(200, 3);
    expect(py).toBeCloseTo(150, 3);
    const behind: Vec3 = [
      view.position[0] * 2 - view.target[0],
      view.position[1] * 2 - view.target[1],
      view.position[2] * 2 - view.target[2],
    ];
    expect(viewer.project(behind)).toBeNull();
  });
});

describe("操作のハンドラ", () => {
  it("ドラッグで回し、move に元のイベントを付けて送り、離したら moveend を 1 回送る。クリックにはしない", async () => {
    const { viewer } = await mount();
    const { moves, ends } = record(viewer);
    const before = viewer.getView();
    const selects: unknown[] = [];
    viewer.on("select", (event) => selects.push(event));
    drag(viewer, [200, 150], 80, 0);
    const after = viewer.getView();
    expect(close(after.position, before.position, 1)).toBe(false);
    expect(close(after.target, before.target)).toBe(true);
    expect(moves.length).toBeGreaterThan(0);
    expect(moves[0]?.originalEvent?.type).toBe("pointermove");
    expect(ends).toHaveLength(1);
    expect(ends[0]?.originalEvent?.type).toBe("pointerup");
    expect(selects).toHaveLength(0);
  });

  it("rotate を止めるとドラッグで回らない", async () => {
    const { viewer } = await mount({ handlers: { rotate: false } });
    const before = viewer.getView();
    drag(viewer, [200, 150], 80, 0);
    expect(viewer.getView()).toEqual(before);
    viewer.setHandlers({ rotate: true });
    drag(viewer, [200, 150], 80, 0);
    expect(viewer.getView()).not.toEqual(before);
  });

  it("ホイールは zoom が always なら拡大し、止まってから moveend を送る", async () => {
    const { viewer } = await mount();
    const { moves, ends } = record(viewer);
    const before = viewer.getView();
    const event = wheel(viewer, -200);
    expect(event.defaultPrevented).toBe(true);
    expect(viewer.getView().span).toBeLessThan(before.span);
    expect(moves.at(-1)?.originalEvent).toBe(event);
    expect(ends).toHaveLength(0);
    await new Promise((resolve) => setTimeout(resolve, 260));
    expect(ends).toHaveLength(1);
  });

  it("zoom が modifier なら Ctrl・⌘ のないホイールはページに任せ、never ならいつも任せる", async () => {
    const { viewer } = await mount({ handlers: { zoom: "modifier" } });
    const before = viewer.getView();
    const plain = wheel(viewer, -200);
    expect(plain.defaultPrevented).toBe(false);
    expect(viewer.getView()).toEqual(before);
    const ctrl = wheel(viewer, -200, { ctrlKey: true });
    expect(ctrl.defaultPrevented).toBe(true);
    expect(viewer.getView().span).toBeLessThan(before.span);
    viewer.setHandlers({ zoom: "never" });
    const zoomed = viewer.getView();
    expect(wheel(viewer, -200, { ctrlKey: true }).defaultPrevented).toBe(false);
    expect(viewer.getView()).toEqual(zoomed);
  });

  it("キーの操作: 0 で全体表示、Esc で選択を外す、矢印で動かす。keyboard を止めると何もしない", async () => {
    const { viewer } = await mount();
    const fitted = viewer.getView();
    wheel(viewer, -400);
    expect(key(viewer, "0").defaultPrevented).toBe(true);
    expect(close(viewer.getView().position, fitted.position, 1e-6)).toBe(true);
    viewer.select([member(viewer, "C1")]);
    const esc = key(viewer, "Escape");
    expect(viewer.getState().selection).toEqual([]);
    expect(esc.defaultPrevented).toBe(true);
    key(viewer, "ArrowLeft");
    expect(close(viewer.getView().target, fitted.target)).toBe(false);

    viewer.setHandlers({ keyboard: false });
    viewer.select([member(viewer, "C1")]);
    const moved = viewer.getView();
    key(viewer, "0");
    key(viewer, "ArrowLeft");
    key(viewer, "Escape");
    expect(viewer.getView()).toEqual(moved);
    expect(viewer.getState().selection).toHaveLength(1);
  });
});

describe("道具と問い合わせ", () => {
  it("memberAt は見えている部材のうち最も手前を返し、線だけの部材も選ぶ", async () => {
    const { viewer } = await mount();
    viewer.viewFrom("front");
    const [cx, cy] = screenOf(viewer, [0, 0, 1500]);
    expect(viewer.memberAt(cx, cy)?.key).toBe("C1");
    viewer.setKindsVisible(["column"], false);
    expect(viewer.memberAt(cx, cy)).toBeNull();
    viewer.setHiddenKinds([]);
    viewer.viewFrom("top");
    const [bx, by] = screenOf(viewer, [3000, 2000, 1500]);
    expect(viewer.memberAt(bx, by)?.key).toBe("B1");
    expect(viewer.memberAt(bx, by + 30)).toBeNull();
  });

  it("選択の道具: クリックで選び、Shift で加える・外す、何もない所で外す", async () => {
    const { viewer } = await mount();
    viewer.viewFrom("front");
    const c1 = screenOf(viewer, [0, 0, 1500]);
    const c2 = screenOf(viewer, [6000, 0, 1500]);
    const events: (Event | null)[] = [];
    viewer.on("select", (event) => events.push(event.originalEvent));
    const first = click(viewer, ...c1);
    expect(viewer.getState().selection.map((item) => item.key)).toEqual(["C1"]);
    expect(events[0]).toBe(first);
    click(viewer, ...c2, { shiftKey: true });
    expect(viewer.getState().selection.map((item) => item.key)).toEqual(["C1", "C2"]);
    click(viewer, ...c1, { shiftKey: true });
    expect(viewer.getState().selection.map((item) => item.key)).toEqual(["C2"]);
    click(viewer, 5, 5);
    expect(viewer.getState().selection).toEqual([]);
  });

  it("ポインターの下の部材を hovered にして hover を送り、hover の色があるときだけ描く", async () => {
    const { viewer } = await mount();
    viewer.viewFrom("front");
    viewer.setEdges(false);
    const [cx, cy] = screenOf(viewer, [0, 0, 1500]);
    const hovered: (string | null)[] = [];
    viewer.on("hover", (event) => hovered.push(event.member?.key ?? null));
    pointer(viewer, "pointermove", cx, cy, { buttons: 0 });
    await frames(1);
    expect(viewer.getState().hovered?.key).toBe("C1");
    expect(hovered).toEqual(["C1"]);
    const plain = await pixel(viewer, cx, cy);
    viewer.setStyle({ light: { hover: "#00ff00" } });
    const lit = await pixel(viewer, cx, cy);
    expect(lit[1]).toBeGreaterThan(plain[1] + 40);
    pointer(viewer, "pointerleave", cx, cy, { buttons: 0 });
    expect(viewer.getState().hovered).toBeNull();
  });

  it("利用側の道具はポインターを受け、orbit: false なら左ドラッグでカメラを回さない", async () => {
    const { viewer } = await mount();
    const log: string[] = [];
    const tool: StructureTool = {
      name: "pin",
      cursor: "crosshair",
      orbit: false,
      activate: () => log.push("activate"),
      deactivate: () => log.push("deactivate"),
      pointerDown: (event) => log.push(`down ${Math.round(event.px)}`),
      pointerMove: () => log.push("move"),
      pointerUp: () => log.push("up"),
      click: (event, target) => log.push(`click ${target.memberAt(event.px, event.py)?.key ?? "-"}`),
    };
    const remove = viewer.addTool(tool);
    viewer.setTool("pin");
    expect(viewer.getState().tool).toBe("pin");
    expect(viewer.canvas.style.cursor).toBe("crosshair");
    const before = viewer.getView();
    drag(viewer, [200, 150], 80, 0);
    expect(viewer.getView()).toEqual(before);
    click(viewer, 5, 5);
    expect(log).toEqual([
      "activate",
      "down 200",
      "move",
      "move",
      "move",
      "move",
      "move",
      "up",
      "down 5",
      "up",
      "click -",
    ]);
    remove();
    expect(viewer.getState().tool).toBe("select");
    expect(log.at(-1)).toBe("deactivate");
    expect(() => viewer.setTool("pin")).toThrow();
  });

  it("orbit の道具はクリックしても選ばない", async () => {
    const { viewer } = await mount({ tool: "orbit" });
    viewer.viewFrom("front");
    click(viewer, ...screenOf(viewer, [0, 0, 1500]));
    expect(viewer.getState().selection).toEqual([]);
  });
});

describe("見た目", () => {
  it("背景と部材の色を CSS のカスタムプロパティから読み、setStyle で上書きする", async () => {
    const { viewer } = await mount(
      {},
      "--zumen-background-light: rgb(10, 20, 30); --zumen-member-column: #00ff00; --zumen-edge-light: #00ff00;",
    );
    viewer.viewFrom("front");
    expect(await pixel(viewer, 2, 2)).toEqual([10, 20, 30]);
    const [cx, cy] = screenOf(viewer, [0, 0, 1500]);
    const [r, g, b] = await pixel(viewer, cx, cy);
    expect(g).toBeGreaterThan(r + 80);
    expect(g).toBeGreaterThan(b + 80);
    viewer.setStyle({ light: { background: "#ff0000" } });
    expect(await pixel(viewer, 2, 2)).toEqual([255, 0, 0]);
    expect(viewer.getPalette().background).toBe("#ff0000");
  });

  it("配色を替えると暗い背景になり、構造種別の色分けで色が変わる", async () => {
    const { viewer } = await mount();
    viewer.viewFrom("front");
    viewer.setEdges(false);
    const [cx, cy] = screenOf(viewer, [6000, 0, 1500]);
    const kind = await pixel(viewer, cx, cy);
    viewer.setColorMode("structure");
    expect(await pixel(viewer, cx, cy)).not.toEqual(kind);
    viewer.setTheme("dark");
    expect(viewer.getState().theme).toBe("dark");
    expect(await pixel(viewer, 2, 2)).toEqual([0x1b, 0x1c, 0x1f]);
  });

  it("通り芯・階の名前は外の CSS なしで読める見た目を要素に持ち、色はカスタムプロパティから読む", async () => {
    const { viewer } = await mount({}, "--zumen-axis-light: rgb(1, 2, 3);");
    await frames();
    const axis = viewer.labelLayer.querySelector<HTMLElement>('[data-zumen-label="axis"]');
    const story = viewer.labelLayer.querySelector<HTMLElement>('[data-zumen-label="story"]');
    expect(axis?.textContent).toBe("X1");
    expect(story?.textContent).toBe("1F");
    const computed = getComputedStyle(axis as HTMLElement);
    expect(computed.borderTopColor).toBe("rgb(1, 2, 3)");
    expect(computed.whiteSpace).toBe("nowrap");
    expect(computed.fontSize).toBe("11px");
    expect(getComputedStyle(viewer.labelLayer).pointerEvents).toBe("none");
    viewer.setGuides({ labels: false });
    await frames();
    expect(axis?.style.display).toBe("none");
  });

  it("読み上げの説明は利用側が決め、なければ付けない", async () => {
    const { viewer } = await mount();
    expect(viewer.canvas.hasAttribute("aria-label")).toBe(false);
    viewer.setLabel("モデル");
    expect(viewer.canvas.getAttribute("aria-label")).toBe("モデル");
  });
});
