import type { Placement, Scene, SceneEntity } from "@zumenjs/core";
import { apply, emptyBox, extendBox, IDENTITY } from "@zumenjs/core/geometry";
import { fontFamilyFor, layoutText } from "@zumenjs/core/text";
import { afterEach, describe, expect, it } from "vitest";
import { DrawingViewer, type DrawingViewerOptions, type ViewEvent, type ViewerTool } from "../src/index.js";

// 表示と操作のエンジンを Chromium で動かす。利用者の操作は合成したイベントで、描いた結果は Canvas の画素で確かめる

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

/** 太さ 2 mm の折線。layers の最後が自分のレイヤ */
function line(
  id: string,
  points: number[],
  options: { layers?: number[]; color?: string; placement?: Placement } = {},
): SceneEntity {
  const xs = points.filter((_, index) => index % 2 === 0);
  const ys = points.filter((_, index) => index % 2 === 1);
  return {
    id,
    kind: "line",
    type: "線分",
    source: "line",
    layers: options.layers ?? [0],
    primitives: [
      {
        kind: "stroke",
        color: options.color ?? "#ffffff",
        width: 2,
        dashes: [],
        points: Float64Array.from(points),
        closed: false,
      },
    ],
    bbox: { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) },
    properties: [],
    ...(options.placement ? { placement: options.placement } : {}),
  };
}

/** 2 本の水平線（y = 0 と y = 50、x = 0〜100）と、レイヤ 1 の縦線の図面 */
function drawing(overrides: Partial<Scene> = {}): Scene {
  const entities = [
    line("bottom", [0, 0, 100, 0]),
    line("top", [0, 50, 100, 50]),
    line("hidden", [50, 0, 50, 50], { layers: [1] }),
  ];
  return {
    format: "test",
    version: "1",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 100, maxY: 50 },
    layers: [
      { id: 0, name: "0", visible: true },
      { id: 1, name: "1", visible: true },
    ],
    entities,
    notices: [],
    metadata: [],
    background: "dark",
    ...overrides,
  };
}

const frames = async (count = 2) => {
  for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
};

async function mount(scene: Scene = drawing(), options: DrawingViewerOptions = {}, css = "") {
  const container = document.createElement("div");
  container.style.cssText = `width: 400px; height: 300px; ${css}`;
  document.body.append(container);
  const viewer = new DrawingViewer(container, scene, options);
  cleanups.push(() => {
    viewer.destroy();
    container.remove();
  });
  await frames();
  return { viewer, container, base: container.querySelector("canvas") as HTMLCanvasElement };
}

/** 表示領域の点（CSS px）にポインターのイベントを送る */
function pointer(viewer: DrawingViewer, type: string, px: number, py: number, init: PointerEventInit = {}) {
  const rect = viewer.canvas.getBoundingClientRect();
  const event = new PointerEvent(type, {
    bubbles: true,
    cancelable: true,
    pointerId: 1,
    pointerType: "mouse",
    isPrimary: true,
    button: 0,
    clientX: rect.left + px,
    clientY: rect.top + py,
    ...init,
  });
  viewer.canvas.dispatchEvent(event);
  return event;
}

/** 動かさずに押して離す */
function click(viewer: DrawingViewer, px: number, py: number, init: PointerEventInit = {}) {
  pointer(viewer, "pointermove", px, py, { buttons: 0, ...init });
  pointer(viewer, "pointerdown", px, py, { buttons: 1, ...init });
  pointer(viewer, "pointerup", px, py, { buttons: 0, ...init });
}

function drag(viewer: DrawingViewer, from: [number, number], to: [number, number], init: PointerEventInit = {}) {
  pointer(viewer, "pointerdown", from[0], from[1], { buttons: 1, ...init });
  for (let step = 1; step <= 4; step++) {
    const t = step / 4;
    pointer(viewer, "pointermove", from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t, {
      buttons: 1,
      ...init,
    });
  }
  pointer(viewer, "pointerup", to[0], to[1], { buttons: 0, ...init });
}

function wheel(viewer: DrawingViewer, px: number, py: number, init: WheelEventInit = {}) {
  const rect = viewer.canvas.getBoundingClientRect();
  const event = new WheelEvent("wheel", {
    bubbles: true,
    cancelable: true,
    clientX: rect.left + px,
    clientY: rect.top + py,
    deltaY: -100,
    ...init,
  });
  viewer.canvas.dispatchEvent(event);
  return event;
}

function key(viewer: DrawingViewer, value: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: value, ...init });
  viewer.canvas.dispatchEvent(event);
  return event;
}

/** Canvas の点（CSS px）の色 */
function pixel(canvas: HTMLCanvasElement, px: number, py: number): number[] {
  const ratio = canvas.width / canvas.getBoundingClientRect().width;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  return [...ctx.getImageData(Math.floor(px * ratio), Math.floor(py * ratio), 1, 1).data];
}

function record(viewer: DrawingViewer) {
  const events: { type: string; event: ViewEvent }[] = [];
  viewer.on("move", (event) => events.push({ type: "move", event }));
  viewer.on("moveend", (event) => events.push({ type: "moveend", event }));
  return events;
}

describe("日本語フォント", () => {
  it.each(["ＭＳ ゴシック", "ＭＳ 明朝"])("%s の横書き・縦書き・回転文字を描き、同じ位置で選択できる", async (font) => {
    for (const [vertical, angle] of [
      [false, 0],
      [true, 0],
      [false, 30],
    ] as const) {
      const primitive = layoutText(
        {
          text: "図面ｶﾅ。、",
          x: 0,
          y: 0,
          height: vertical ? 50 : 10,
          width: vertical ? 10 : 50,
          spacing: 0,
          angle,
          slant: 0,
          anchor: 1,
          vertical,
          color: "#000000",
          fontFamily: fontFamilyFor(font),
        },
        IDENTITY,
      );
      if (!primitive) throw new Error("文字がありません");
      await document.fonts.load(`64px ${primitive.fontFamily}`, primitive.text);
      const bbox = emptyBox();
      for (const [x, y] of [
        [0, 0],
        [primitive.boxWidth, 0],
        [primitive.boxWidth, primitive.boxHeight],
        [0, primitive.boxHeight],
      ]) {
        extendBox(bbox, ...apply(primitive.matrix, x as number, y as number));
      }
      const entity: SceneEntity = {
        id: "text",
        kind: "text",
        type: "文字",
        source: "test",
        layers: [0],
        primitives: [primitive],
        bbox,
        properties: [],
      };
      const { viewer, base } = await mount(drawing({ entities: [entity], bounds: bbox, background: "light" }));
      const ctx = base.getContext("2d") as CanvasRenderingContext2D;
      const dpr = base.width / base.getBoundingClientRect().width;
      for (const cell of primitive.cells) {
        const paper = apply(primitive.matrix, cell.x, cell.y);
        expect(viewer.entitiesAt(...paper, { tolerance: 0 }).map((item) => item.id)).toEqual(["text"]);
        // Check each glyph's cell has ink, including half-width kana and punctuation. No OS-specific pixel golden.
        const [x, y] = viewer.toScreen(...paper);
        const [edgeX, edgeY] = viewer.toScreen(...apply(primitive.matrix, cell.x + primitive.height / 2, cell.y));
        const radius = Math.ceil(Math.hypot(edgeX - x, edgeY - y) * dpr);
        const pixels = ctx.getImageData(
          Math.floor(x * dpr) - radius,
          Math.floor(y * dpr) - radius,
          radius * 2,
          radius * 2,
        ).data;
        expect(pixels.some((value, index) => index % 4 !== 3 && value < 100)).toBe(true);
      }
    }
  });
});

describe("表示範囲", () => {
  // 保存した表示範囲を戻す、2 つの表示をそろえる、といった使い方は、値がそのまま往復することに頼る
  it("jumpTo と getView は同じ値を往復し、表示範囲が変わったときだけ move と moveend を送る", async () => {
    const { viewer } = await mount();
    const events = record(viewer);
    viewer.jumpTo({ center: [12.3456789, -7.1], zoom: 1.7320508 });
    expect(viewer.getView()).toMatchObject({ center: [12.3456789, -7.1], zoom: 1.7320508, width: 400, height: 300 });
    expect(events.map((entry) => entry.type)).toEqual(["move", "moveend"]);
    expect(events[0]?.event.originalEvent).toBeNull();
    viewer.jumpTo(viewer.getView());
    viewer.zoomBy(1);
    expect(events).toHaveLength(2);

    // 互いの move で表示範囲をそろえても、往復は止まる
    const other = (await mount()).viewer;
    viewer.on("move", ({ view }) => other.jumpTo(view));
    other.on("move", ({ view }) => viewer.jumpTo(view));
    viewer.zoomBy(1.3, 50, 60);
    other.panBy(25, -10);
    expect(other.getView().center).toEqual(viewer.getView().center);
    expect(other.getView().zoom).toBe(viewer.getView().zoom);
  });

  it("fitBounds は余白を残して範囲を収め、利用者が動かすまでは大きさが変わっても収め直す", async () => {
    const { viewer, container } = await mount();
    const box = { minX: 20, minY: 10, maxX: 60, maxY: 30 };
    viewer.fitBounds(box, { padding: 40 });
    const check = (width: number, height: number) => {
      const [left, top] = viewer.toScreen(box.minX, box.maxY);
      const [right, bottom] = viewer.toScreen(box.maxX, box.minY);
      // 範囲は余白の内側にあり、縦か横の一方は余白まで届く
      expect(Math.min(left, top)).toBeGreaterThanOrEqual(40 - 1e-9);
      expect(Math.max(right - width, bottom - height)).toBeLessThanOrEqual(-40 + 1e-9);
      expect(Math.min(Math.abs(left - 40), Math.abs(top - 40))).toBeLessThan(1e-9);
    };
    check(400, 300);
    container.style.width = "600px";
    await frames();
    expect(viewer.getView().width).toBe(600);
    check(600, 300);
    // 動かした後は、大きさが変わっても中央を保つ
    viewer.panBy(30, 0);
    const center = viewer.getView().center;
    container.style.width = "500px";
    await frames();
    expect(viewer.getView().center).toEqual(center);
  });

  // ホイール・ダブルクリック・ピンチの拡大は、ポインターの下の図面が動かないことで、狙った場所を拡大できる
  it("zoomBy とホイールは、指定した点の下の用紙の点を動かさない", async () => {
    const { viewer } = await mount();
    const [x, y] = viewer.toPaper(120, 90);
    viewer.zoomBy(2.5, 120, 90);
    const [px, py] = viewer.toScreen(x, y);
    expect(px).toBeCloseTo(120, 9);
    expect(py).toBeCloseTo(90, 9);
    const [wx, wy] = viewer.toPaper(300, 40);
    wheel(viewer, 300, 40);
    expect(viewer.toScreen(wx, wy)[0]).toBeCloseTo(300, 9);
    expect(viewer.toScreen(wx, wy)[1]).toBeCloseTo(40, 9);
  });
});

describe("回転", () => {
  // 回した表示でも、画面の点と用紙の点の変換・収め方・ドラッグ・選択・描画がそろっていないと、狙った図形を扱えない
  it("rotateBy は収めた範囲を回した向きで収め直し、ドラッグ・選択・描画は回した画面の座標で動く", async () => {
    const { viewer, base } = await mount(drawing(), { background: "dark" });
    const events = record(viewer);
    viewer.rotateBy(90);
    const view = viewer.getView();
    expect(view.rotation).toBe(90);
    expect(events.map((entry) => entry.type)).toEqual(["move", "moveend"]);
    // 時計回りに 90 度: 用紙の右（+x）が画面の下、用紙の上（+y）が画面の右
    const [ox, oy] = viewer.toScreen(0, 0);
    const [rx, ry] = viewer.toScreen(100, 0);
    const [ux, uy] = viewer.toScreen(0, 50);
    expect(rx).toBeCloseTo(ox, 9);
    expect(ry).toBeGreaterThan(oy);
    expect(ux).toBeGreaterThan(ox);
    expect(uy).toBeCloseTo(oy, 9);
    const [x, y] = viewer.toPaper(123, 45);
    expect(viewer.toScreen(x, y)[0]).toBeCloseTo(123, 9);
    expect(viewer.toScreen(x, y)[1]).toBeCloseTo(45, 9);
    // 100 × 50 の範囲は、回すと縦長になり、高さで収まる
    expect(ry - oy).toBeGreaterThan(ux - ox);
    expect(Math.min(oy, 300 - ry)).toBeGreaterThan(0);
    // 見えている範囲は、回した表示領域を囲む
    expect(view.bounds.maxX - view.bounds.minX).toBeCloseTo((300 / view.zoom) * (25.4 / 96), 6);

    await frames();
    const [tx, ty] = viewer.toScreen(75, 50);
    // y = 50 の線は回すと縦になり、その上の点は線の色、線のない用紙の点は背景の色
    expect(pixel(base, tx, ty)).toEqual([255, 255, 255, 255]);
    expect(pixel(base, ...viewer.toScreen(25, 25))).toEqual([0, 0, 0, 255]);
    click(viewer, tx, ty);
    expect(viewer.getState().selection.map((entity) => entity.id)).toEqual(["top"]);

    const [px, py] = viewer.toPaper(150, 150);
    drag(viewer, [150, 150], [200, 120]);
    expect(viewer.toScreen(px, py)[0]).toBeCloseTo(200, 9);
    expect(viewer.toScreen(px, py)[1]).toBeCloseTo(120, 9);

    // 動かした後は、中央を保って回す。角度は 0 以上 360 未満にそろえる
    const center = viewer.getView().center;
    viewer.rotateBy(-450);
    expect(viewer.getView().rotation).toBe(0);
    expect(viewer.getView().center[0]).toBeCloseTo(center[0], 9);
    expect(viewer.getView().center[1]).toBeCloseTo(center[1], 9);
    viewer.jumpTo({ rotation: 45 });
    expect(viewer.getView().rotation).toBe(45);
    viewer.jumpTo({ center: [10, 20] });
    expect(viewer.getView()).toMatchObject({ center: [10, 20], rotation: 45 });
  });
});

describe("操作のハンドラ", () => {
  it("ホイールは wheel の設定に従い、拡大しないときはページのスクロールを止めない。止まると moveend を送る", async () => {
    const { viewer } = await mount();
    const events = record(viewer);
    const zoom = viewer.getView().zoom;
    expect(wheel(viewer, 200, 150).defaultPrevented).toBe(true);
    expect(viewer.getView().zoom).toBeGreaterThan(zoom);
    expect(events.map((entry) => entry.type)).toEqual(["move"]);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(events.map((entry) => entry.type)).toEqual(["move", "moveend"]);
    expect(events[1]?.event.originalEvent).toBeInstanceOf(WheelEvent);

    viewer.setHandlers({ wheel: "modifier" });
    const before = viewer.getView().zoom;
    expect(wheel(viewer, 200, 150).defaultPrevented).toBe(false);
    expect(viewer.getView().zoom).toBe(before);
    // トラックパッドのピンチは Ctrl つきのホイールとして届く
    expect(wheel(viewer, 200, 150, { ctrlKey: true }).defaultPrevented).toBe(true);
    expect(viewer.getView().zoom).toBeGreaterThan(before);

    viewer.setHandlers({ wheel: "never" });
    const after = viewer.getView().zoom;
    expect(wheel(viewer, 200, 150, { ctrlKey: true }).defaultPrevented).toBe(false);
    expect(viewer.getView().zoom).toBe(after);
  });

  it("ドラッグで図面を動かし、離したときに moveend を送る。dragPan を止めると動かさず、選択もしない", async () => {
    const { viewer } = await mount();
    const events = record(viewer);
    const [x, y] = viewer.toPaper(100, 100);
    drag(viewer, [100, 100], [160, 130]);
    const [px, py] = viewer.toScreen(x, y);
    expect(px).toBeCloseTo(160, 9);
    expect(py).toBeCloseTo(130, 9);
    expect(events.filter((entry) => entry.type === "move")).toHaveLength(4);
    expect(events.at(-1)?.type).toBe("moveend");
    expect(events.at(-1)?.event.originalEvent).toBeInstanceOf(PointerEvent);

    viewer.setHandlers({ dragPan: false });
    const center = viewer.getView().center;
    // y = 50 の線の上から始めても、ドラッグはクリックではない
    const [sx, sy] = viewer.toScreen(30, 50);
    drag(viewer, [sx, sy], [sx + 80, sy + 40]);
    expect(viewer.getView().center).toEqual(center);
    expect(viewer.getState().selection).toEqual([]);
  });

  it("2 本の指を広げると拡大し、pinch を止めると何もしない", async () => {
    const { viewer } = await mount();
    const touch = (type: string, id: number, px: number, py: number) =>
      pointer(viewer, type, px, py, { pointerId: id, pointerType: "touch", isPrimary: id === 1 });
    const pinch = () => {
      touch("pointerdown", 1, 150, 150);
      touch("pointerdown", 2, 250, 150);
      touch("pointermove", 1, 100, 150);
      touch("pointermove", 2, 300, 150);
      touch("pointerup", 1, 100, 150);
      touch("pointerup", 2, 300, 150);
    };
    const zoom = viewer.getView().zoom;
    pinch();
    // 指の間の距離は 100 px から 200 px
    expect(viewer.getView().zoom).toBeCloseTo(zoom * 2, 9);

    viewer.setHandlers({ pinch: false });
    const view = viewer.getView();
    pinch();
    expect(viewer.getView().zoom).toBe(view.zoom);
    expect(viewer.getView().center).toEqual(view.center);
  });

  it("ダブルクリックとキーは 1 つずつ止められ、止めたキーはページに任せる", async () => {
    const { viewer } = await mount();
    const zoom = viewer.getView().zoom;
    viewer.canvas.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 10, clientY: 10 }));
    expect(viewer.getView().zoom).toBeCloseTo(zoom * 2, 9);
    expect(key(viewer, "+").defaultPrevented).toBe(true);
    expect(viewer.getView().zoom).toBeCloseTo(zoom * 2.5, 9);
    expect(key(viewer, "0").defaultPrevented).toBe(true);
    expect(viewer.getView().zoom).toBe(zoom);

    viewer.setHandlers({ doubleClick: false, keyboard: false });
    viewer.canvas.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, clientX: 10, clientY: 10 }));
    expect(key(viewer, "+").defaultPrevented).toBe(false);
    expect(key(viewer, "ArrowLeft").defaultPrevented).toBe(false);
    expect(viewer.getView().zoom).toBe(zoom);
  });
});

describe("状態と道具", () => {
  it("選択の道具はクリックで選び、Shift で加え・外し、Esc で外す。変わらない操作では状態を作り直さない", async () => {
    const { viewer } = await mount();
    const [bottom, top] = viewer.getState().scene.entities as [SceneEntity, SceneEntity];
    const at = (x: number, y: number) => viewer.toScreen(x, y);
    const states: unknown[] = [];
    viewer.subscribe(() => states.push(viewer.getState()));

    click(viewer, ...at(20, 0));
    expect(viewer.getState().selection).toEqual([bottom]);
    click(viewer, ...at(20, 50), { shiftKey: true });
    expect(viewer.getState().selection).toEqual([bottom, top]);
    click(viewer, ...at(20, 0), { shiftKey: true });
    expect(viewer.getState().selection).toEqual([top]);
    // ポインターの下の図形も状態に入る
    expect(viewer.getState().hovered).toBe(bottom);
    const count = states.length;
    viewer.select([top]);
    expect(states).toHaveLength(count);
    expect(key(viewer, "Escape").defaultPrevented).toBe(true);
    expect(viewer.getState().selection).toEqual([]);
    expect(new Set(states).size).toBe(states.length);
  });

  it("問い合わせは見えている図形だけを返し、点では近い順、範囲では描く順に並べる", async () => {
    const { viewer } = await mount();
    const [bottom, top, hidden] = viewer.getState().scene.entities as [SceneEntity, SceneEntity, SceneEntity];
    // (50, 0) は下の線と縦の線の両方に重なり、縦の線は後に描いたので先に来る
    expect(viewer.entitiesAt(50, 0)).toEqual([hidden, bottom]);
    expect(viewer.entitiesIn({ minX: 40, minY: 45, maxX: 60, maxY: 60 })).toEqual([top, hidden]);
    expect(viewer.snapAt(99, 1)).toMatchObject({ x: 100, y: 0, kind: "vertex", entity: bottom });
    viewer.setLayersVisible([1], false);
    expect(viewer.entitiesAt(50, 0)).toEqual([bottom]);
    expect(viewer.entitiesIn({ minX: 40, minY: 45, maxX: 60, maxY: 60 })).toEqual([top]);
  });

  it("計測の道具は頂点に吸着し、部分図の縮尺から実寸を出す。道具を選び直すと計測を消す", async () => {
    // 1/100 の部分図の中の線: 用紙の 1 mm が元の 100 mm
    const placement: Placement = { name: "平面図", scaleX: 0.01, scaleY: 0.01, inverse: [100, 0, 0, 100, 0, 0] };
    const { viewer } = await mount(drawing({ entities: [line("scaled", [0, 0, 100, 50], { placement })] }));
    viewer.setTool("measure");
    expect(viewer.canvas.style.cursor).toBe("crosshair");
    const [sx, sy] = viewer.toScreen(0, 0);
    const [ex, ey] = viewer.toScreen(100, 50);
    click(viewer, sx + 3, sy - 2);
    expect(viewer.getState().measurement?.end).toBeNull();
    // 計測の道具では、左ボタンのドラッグで図面を動かさない
    const center = viewer.getView().center;
    drag(viewer, [200, 200], [ex + 2, ey + 3]);
    expect(viewer.getView().center).toEqual(center);
    const { distance } = viewer.getState();
    expect(distance?.paper).toBeCloseTo(Math.hypot(100, 50), 9);
    expect(distance?.real).toBeCloseTo(Math.hypot(10000, 5000), 6);
    expect(distance?.placement).toBe("平面図");
    viewer.setTool("select");
    expect(viewer.getState().measurement).not.toBeNull();
    viewer.setTool("measure");
    expect(viewer.getState().measurement).toBeNull();
  });

  it("利用側の道具は用紙の座標でクリックとドラッグを受け、重ね描きに描き、外すと選択の道具に戻る", async () => {
    const { viewer } = await mount();
    const pins: [number, number][] = [];
    const strokes: string[] = [];
    const tool: ViewerTool = {
      name: "pin",
      dragPan: false,
      cursor: "copy",
      pointerDown: () => strokes.push("down"),
      pointerMove: (event) => {
        if (event.event.buttons) strokes.push("move");
      },
      pointerUp: () => strokes.push("up"),
      click: (event, target) => {
        pins.push([event.x, event.y]);
        target.invalidate();
      },
      draw: (ctx, target) => {
        ctx.fillStyle = "#00ff00";
        for (const [x, y] of pins) {
          const [px, py] = target.toScreen(x, y);
          ctx.fillRect(px - 4, py - 4, 8, 8);
        }
      },
    };
    const remove = viewer.addTool(tool);
    viewer.setTool("pin");
    expect(viewer.getState().tool).toBe("pin");
    expect(viewer.canvas.style.cursor).toBe("copy");
    click(viewer, 123, 77);
    const [x, y] = viewer.toPaper(123, 77);
    expect(pins).toEqual([[x, y]]);
    await frames();
    expect(pixel(viewer.canvas, 123, 77)).toEqual([0, 255, 0, 255]);
    // dragPan が false の道具はドラッグを受け、図面は動かない
    const center = viewer.getView().center;
    strokes.length = 0;
    drag(viewer, [200, 200], [260, 230]);
    expect(strokes).toEqual(["down", "move", "move", "move", "move", "up"]);
    expect(viewer.getView().center).toEqual(center);
    expect(pins).toHaveLength(1);
    remove();
    expect(viewer.getState().tool).toBe("select");
  });
});

describe("見た目", () => {
  // 図面の色と背景は利用側の配色に合わせて変えられ、図面の線は背景と見分けられるように描く
  it("背景・選択・名前付きの強調の色は CSS のカスタムプロパティから読み、setStyle で上書きする", async () => {
    const css = "--zumen-background-dark: #102030; --zumen-highlight-search-dark: #00ff00; --zumen-fit-padding: 0";
    const { viewer, base } = await mount(drawing(), {}, css);
    expect(pixel(base, 2, 2)).toEqual([16, 32, 48, 255]);
    // 余白 0 で、範囲の左端が表示領域の左端に来る
    expect(viewer.toScreen(0, 25)[0]).toBeCloseTo(0, 9);
    const [bottom, top] = viewer.getState().scene.entities as [SceneEntity, SceneEntity];
    viewer.setHighlight("search", [bottom]);
    viewer.select([top]);
    await frames();
    expect(pixel(base, ...viewer.toScreen(25, 0))).toEqual([0, 255, 0, 255]);
    expect(pixel(base, ...viewer.toScreen(25, 50))).toEqual([255, 176, 0, 255]);
    viewer.setStyle({ dark: { background: "#405060", selection: "#ff0000" } });
    await frames();
    expect(pixel(base, 2, 2)).toEqual([64, 80, 96, 255]);
    expect(pixel(base, ...viewer.toScreen(25, 50))).toEqual([255, 0, 0, 255]);
    // ポインターの下の図形は、hover の色を決めたときだけ重ね描きの Canvas に描く
    pointer(viewer, "pointermove", ...viewer.toScreen(25, 0), { buttons: 0 });
    expect(viewer.getState().hovered).toBe(bottom);
    await frames();
    expect(pixel(viewer.canvas, ...viewer.toScreen(25, 0))).toEqual([0, 0, 0, 0]);
    viewer.setStyle({ dark: { hover: "#00ffff" } });
    await frames();
    expect(pixel(viewer.canvas, ...viewer.toScreen(25, 0))).toEqual([0, 255, 255, 255]);
  });

  it("背景の既定は図面の想定で、背景と見分けにくい色は背景の明るさで反転する", async () => {
    const white = drawing({
      entities: [line("white", [0, 0, 100, 0]), line("black", [0, 50, 100, 50], { color: "#000000" })],
    });
    const light = await mount({ ...white, background: "light" });
    expect(pixel(light.base, 2, 2)).toEqual([255, 255, 255, 255]);
    expect(pixel(light.base, ...light.viewer.toScreen(25, 0))).toEqual([0, 0, 0, 255]);
    // 暗い背景の組でも、色が明るければ明るい背景として反転する
    const pale = await mount(white, {}, "--zumen-background-dark: #f0f0f0");
    expect(pale.viewer.getState().background).toBe("dark");
    expect(pixel(pale.base, ...pale.viewer.toScreen(25, 0))).toEqual([0, 0, 0, 255]);
    expect(pixel(pale.base, ...pale.viewer.toScreen(25, 50))).toEqual([0, 0, 0, 255]);
    const dark = await mount(white);
    expect(pixel(dark.base, ...dark.viewer.toScreen(25, 50))).toEqual([255, 255, 255, 255]);
  });
});
