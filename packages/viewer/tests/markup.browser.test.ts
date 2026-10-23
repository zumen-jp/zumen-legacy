import {
  createMarkup,
  type MarkupDocument,
  MarkupError,
  type MarkupItem,
  type Scene,
  type SceneEntity,
} from "@zumenjs/core";
import { afterEach, describe, expect, it } from "vitest";
import {
  cloudTool,
  DrawingViewer,
  findingTool,
  type MarkupChangeEvent,
  type MarkupDraft,
  MarkupLayer,
  type MarkupLayerOptions,
  markupEditTool,
  photoTool,
  redlineTool,
  textTool,
} from "../src/index.js";

// マークアップの層と道具を、合成したポインターとキーのイベントで動かし、描いた結果を重ね描きの Canvas の画素で確かめる

const HASH = "9f2c6d1a4b7e8f0312a5c6d7e8f9a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b";

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

function line(id: string, points: number[]): SceneEntity {
  const xs = points.filter((_, index) => index % 2 === 0);
  const ys = points.filter((_, index) => index % 2 === 1);
  return {
    id,
    kind: "line",
    type: "線分",
    source: "line",
    layers: [0],
    primitives: [
      { kind: "stroke", color: "#ffffff", width: 0.5, dashes: [], points: Float64Array.from(points), closed: false },
    ],
    bbox: { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) },
    properties: [],
  };
}

/** 用紙の (0, 0)〜(100, 50) の図面。ビューは v1 と v2 で、v1 を描く */
function drawing(overrides: Partial<Scene> = {}): Scene {
  return {
    format: "test",
    version: "1",
    views: [
      { id: "v1", name: "1" },
      { id: "v2", name: "2" },
    ],
    view: "v1",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 100, maxY: 50 },
    layers: [{ id: 0, name: "0", visible: true }],
    entities: [line("bottom", [0, 0, 100, 0]), line("top", [0, 50, 100, 50])],
    notices: [],
    metadata: [],
    background: "dark",
    ...overrides,
  };
}

function markup(items: unknown[] = []): MarkupDocument {
  return { ...createMarkup({ sha256: HASH, format: "test" }), items: items as MarkupItem[] };
}

const frames = async (count = 2) => {
  for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
};

async function mount(
  options: Partial<MarkupLayerOptions> = {},
  scene: Scene = drawing(),
  css = "",
): Promise<{ viewer: DrawingViewer; layer: MarkupLayer; events: MarkupChangeEvent[] }> {
  const container = document.createElement("div");
  // 余白 0 で、用紙の 1 mm を 4 px にする（整数の用紙の点が整数の画素に来る）
  container.style.cssText = `width: 400px; height: 300px; --zumen-fit-padding: 0; ${css}`;
  document.body.append(container);
  const viewer = new DrawingViewer(container, scene);
  let counter = 0;
  const layer = new MarkupLayer(viewer, {
    document: markup(),
    createId: () => `m${++counter}`,
    now: () => "2026-10-23T09:00:00Z",
    ...options,
  });
  const events: MarkupChangeEvent[] = [];
  layer.on("change", (event) => events.push(event));
  cleanups.push(() => {
    layer.destroy();
    viewer.destroy();
    container.remove();
  });
  await frames();
  return { viewer, layer, events };
}

function pointer(viewer: DrawingViewer, type: string, px: number, py: number, init: PointerEventInit = {}) {
  const rect = viewer.canvas.getBoundingClientRect();
  viewer.canvas.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      clientX: rect.left + px,
      clientY: rect.top + py,
      ...init,
    }),
  );
}

/** 用紙の点 (x, y) を、動かさずに押して離す */
function click(viewer: DrawingViewer, x: number, y: number, init: PointerEventInit = {}) {
  const [px, py] = viewer.toScreen(x, y);
  pointer(viewer, "pointermove", px, py, { buttons: 0, ...init });
  pointer(viewer, "pointerdown", px, py, { buttons: 1, ...init });
  pointer(viewer, "pointerup", px, py, { buttons: 0, ...init });
}

/** 用紙の点から点へドラッグする */
function drag(viewer: DrawingViewer, from: [number, number], to: [number, number], init: PointerEventInit = {}) {
  const [fx, fy] = viewer.toScreen(...from);
  const [tx, ty] = viewer.toScreen(...to);
  pointer(viewer, "pointerdown", fx, fy, { buttons: 1, ...init });
  for (let step = 1; step <= 4; step++) {
    const t = step / 4;
    pointer(viewer, "pointermove", fx + (tx - fx) * t, fy + (ty - fy) * t, { buttons: 1, ...init });
  }
  pointer(viewer, "pointerup", tx, ty, { buttons: 0, ...init });
}

function key(viewer: DrawingViewer, value: string, init: KeyboardEventInit = {}) {
  viewer.canvas.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, cancelable: true, key: value, ...init }));
}

/** 重ね描きの Canvas の、用紙の点の画素 */
function pixel(viewer: DrawingViewer, x: number, y: number): number[] {
  const [px, py] = viewer.toScreen(x, y);
  const canvas = viewer.canvas;
  const ratio = canvas.width / canvas.getBoundingClientRect().width;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  return [...ctx.getImageData(Math.floor(px * ratio), Math.floor(py * ratio), 1, 1).data];
}

const RED = [255, 77, 77, 255];
const NONE = [0, 0, 0, 0];

describe("描画", () => {
  it("図面が描いているビューの項目を、分類の色・既定の色で描き、filter で選ぶ", async () => {
    const css = "--zumen-markup-dark: rgb(0, 255, 0); --zumen-markup-category-question-dark: #0000ff";
    const { viewer, layer } = await mount(
      {
        document: markup([
          { id: "a", kind: "finding", view: "v1", position: [20, 25], status: "open" },
          { id: "b", kind: "finding", view: "v1", position: [50, 25], status: "open", category: "question" },
          { id: "c", kind: "finding", view: "v2", position: [80, 25], status: "open" },
          { id: "d", kind: "finding", view: "v1", position: [80, 10], status: "open", color: "#ff00ff" },
        ]),
      },
      drawing(),
      css,
    );
    await frames();
    expect(layer.items().map((item) => item.id)).toEqual(["a", "b", "d"]);
    expect(pixel(viewer, 20, 25)).toEqual([0, 255, 0, 255]);
    expect(pixel(viewer, 50, 25)).toEqual([0, 0, 255, 255]);
    expect(pixel(viewer, 80, 25)).toEqual(NONE);
    expect(pixel(viewer, 80, 10)).toEqual([255, 0, 255, 255]);
    layer.setStyle({ dark: { categories: { question: "#ffff00" } } });
    layer.setFilter((item) => item.id !== "a");
    await frames();
    expect(pixel(viewer, 20, 25)).toEqual(NONE);
    expect(pixel(viewer, 50, 25)).toEqual([255, 255, 0, 255]);
    expect(layer.itemsAt(20, 25)).toEqual([]);
  });

  it("ピンは画面で決まった大きさに描き、線は lineWidth の用紙の太さで描く", async () => {
    const { viewer, layer } = await mount({
      document: markup([
        { id: "pin", kind: "finding", view: "v1", position: [40, 25], status: "open" },
        {
          id: "wide",
          kind: "redline",
          view: "v1",
          points: [
            [55, 10],
            [70, 10],
          ],
          lineWidth: 4,
        },
      ]),
    });
    const radius = (x: number, y: number) => {
      // 中心から右へ、塗られた画素の続く長さ（CSS px）
      const [px, py] = viewer.toScreen(x, y);
      let r = 0;
      while (r < 100 && pixel(viewer, ...viewer.toPaper(px + r, py))[3] === 255) r++;
      return r;
    };
    expect(radius(40, 25)).toBeGreaterThanOrEqual(9);
    expect(radius(40, 25)).toBeLessThanOrEqual(11);
    viewer.zoomBy(2);
    await frames();
    expect(radius(40, 25)).toBeGreaterThanOrEqual(9);
    expect(radius(40, 25)).toBeLessThanOrEqual(11);
    // 太さ 4 mm の線は、拡大すると画面で太くなる。中心から 1.8 mm の所は線の中
    expect(pixel(viewer, 62, 11.8)).toEqual(RED);
    expect(layer.itemsAt(62, 11.8).map((item) => item.id)).toEqual(["wide"]);
    expect(layer.itemsAt(62, 13).map((item) => item.id)).toEqual([]);
  });

  it("同じ用紙の CAD のレイアウトと PDF のページで、用紙の mm の項目が重なる", async () => {
    // DXF のレイアウトも PDF のページも、用紙の左下を原点にする（docs/decisions/0021）
    const sheet = { name: "A3", size: "A3", width: 420, height: 297 };
    const bounds = { minX: 0, minY: 0, maxX: 420, maxY: 297 };
    const cad = drawing({ format: "dxf", views: [{ id: "Layout1", name: "Layout1" }], view: "Layout1", sheet, bounds });
    const pt = 25.4 / 72;
    const pdf = drawing({
      format: "pdf",
      views: [{ id: "1", name: "1" }],
      view: "1",
      sheet,
      bounds,
      entities: [{ ...line("p1", [0, 0, 420, 0]), drawn: false }],
      underlays: [
        {
          id: "page-1",
          kind: "pdf",
          page: 1,
          width: 420 / pt,
          height: 297 / pt,
          matrix: [pt, 0, 0, -pt, 0, 297],
          layers: [],
          bbox: bounds,
        },
      ],
    });
    // 同じ点の 2 つの項目。PDF の側は、ビューの対応表で移した項目（#93 の carry と同じ形）
    const finding = { kind: "finding", position: [100, 80], status: "open" };
    const cloud = {
      kind: "cloud",
      points: [
        [200, 150],
        [300, 150],
        [300, 220],
        [200, 220],
      ],
    };
    const { viewer, layer } = await mount(
      {
        document: markup([
          { id: "cad-pin", view: "Layout1", ...finding },
          { id: "cad-cloud", view: "Layout1", ...cloud },
          { id: "pdf-pin", view: "1", ...finding },
          { id: "pdf-cloud", view: "1", ...cloud },
        ]),
      },
      cad,
    );
    await frames();
    const read = () => {
      const ratio = viewer.canvas.width / viewer.canvas.getBoundingClientRect().width;
      const ctx = viewer.canvas.getContext("2d") as CanvasRenderingContext2D;
      return [...ctx.getImageData(0, 0, Math.round(400 * ratio), Math.round(300 * ratio)).data];
    };
    const onCad = { items: layer.items().map((item) => item.id), screen: viewer.toScreen(100, 80), image: read() };
    viewer.setScene(pdf);
    await frames();
    expect(onCad.items).toEqual(["cad-pin", "cad-cloud"]);
    expect(layer.items().map((item) => item.id)).toEqual(["pdf-pin", "pdf-cloud"]);
    expect(viewer.toScreen(100, 80)).toEqual(onCad.screen);
    expect(pixel(viewer, 100, 80)).toEqual(RED);
    // 同じ画素に描く
    const image = read();
    let differ = 0;
    for (let i = 0; i < image.length; i++) if (image[i] !== onCad.image[i]) differ++;
    expect(differ).toBe(0);
  });
});

describe("編集と取り消し", () => {
  it("add は id・author・日時とビューを入れ、update・remove とともに 1 回ずつ取り消せる", async () => {
    const { viewer, layer, events } = await mount({ author: "u-1" });
    const [added] = layer.add({
      kind: "redline",
      points: [
        [10, 10],
        [40, 10],
      ],
      arrows: "end",
    });
    expect(added).toEqual({
      id: "m1",
      kind: "redline",
      view: "v1",
      points: [
        [10, 10],
        [40, 10],
      ],
      arrows: "end",
      author: "u-1",
      createdAt: "2026-10-23T09:00:00Z",
      updatedAt: "2026-10-23T09:00:00Z",
    });
    layer.update("m1", {
      points: [
        [10, 20],
        [40, 20],
      ],
      arrows: undefined,
      text: "直す",
    });
    const updated = layer.document.items[0];
    expect(updated).toMatchObject({
      points: [
        [10, 20],
        [40, 20],
      ],
      text: "直す",
    });
    expect(updated && "arrows" in updated).toBe(false);
    layer.select(["m1", "missing"]);
    expect(layer.getState().selection).toEqual(["m1"]);
    layer.remove("m1");
    expect(layer.getState()).toMatchObject({ selection: [], canUndo: true, canRedo: false });
    expect(events.map(({ added, updated, removed, origin }) => [origin, added, updated, removed])).toEqual([
      ["edit", ["m1"], [], []],
      ["edit", [], ["m1"], []],
      ["edit", [], [], ["m1"]],
    ]);
    layer.undo();
    expect(layer.document.items[0]).toBe(updated);
    layer.undo();
    expect(layer.document.items[0]).toBe(added);
    layer.redo();
    expect(layer.document.items[0]).toBe(updated);
    expect(events.slice(3).map(({ added, updated, removed, origin }) => [origin, added, updated, removed])).toEqual([
      ["undo", ["m1"], [], []],
      ["undo", [], ["m1"], []],
      ["redo", [], ["m1"], []],
    ]);
    expect(events.at(-1)?.document).toBe(layer.document);
    // 新しい編集はやり直しを消す
    layer.add({ kind: "photo", position: [1, 1], photo: "p" });
    expect(layer.getState().canRedo).toBe(false);
    // 誤った項目と、同じ id は投げ、文書を変えない
    const before = layer.document;
    expect(() =>
      layer.add({
        kind: "cloud",
        points: [
          [0, 0],
          [1, 1],
        ],
      }),
    ).toThrow(MarkupError);
    expect(() => layer.add({ id: "m1", kind: "photo", position: [1, 1], photo: "p" })).toThrow();
    expect(() =>
      layer.update("m1", {
        points: [
          [Number.NaN, 0],
          [1, 1],
        ],
      }),
    ).toThrow(MarkupError);
    expect(layer.document).toBe(before);
    // setDocument は取り消しの記録を消し、change を送らない
    const count = events.length;
    layer.setDocument(markup());
    expect(layer.getState()).toMatchObject({ canUndo: false, canRedo: false });
    expect(events).toHaveLength(count);
    expect(() => layer.setDocument({ ...markup(), version: 2 })).toThrow(MarkupError);
    expect(viewer.getState().tool).toBe("select");
  });

  it("取り消せる操作の数は historyLimit まで", async () => {
    const { layer } = await mount({ historyLimit: 2 });
    for (let i = 0; i < 4; i++) layer.add({ kind: "photo", position: [i, i], photo: "p" });
    layer.undo();
    layer.undo();
    layer.undo();
    expect(layer.document.items.map((item) => item.id)).toEqual(["m1", "m2"]);
  });
});

describe("道具", () => {
  it("赤入れの道具は、線分をドラッグか 2 回のクリックで、矩形と手書きの線を描き、Esc でやめる", async () => {
    const { viewer, layer } = await mount();
    viewer.addTool(redlineTool(layer, { arrows: "end", category: "correction" }));
    viewer.addTool(redlineTool(layer, { name: "box", mode: "rectangle", lineWidth: 0.5 }));
    viewer.addTool(redlineTool(layer, { name: "pen", mode: "freehand" }));
    viewer.setTool("markup-redline");
    const center = viewer.getView().center;
    drag(viewer, [10, 10], [40, 30]);
    // ドラッグで図面を動かさない
    expect(viewer.getView().center).toEqual(center);
    click(viewer, 50, 10);
    click(viewer, 70, 10);
    // 1 回のクリックの後に Esc でやめる
    click(viewer, 80, 40);
    key(viewer, "Escape");
    click(viewer, 90, 40);
    key(viewer, "Escape");
    viewer.setTool("box");
    drag(viewer, [60, 20], [80, 40]);
    viewer.setTool("pen");
    drag(viewer, [10, 40], [30, 45]);
    expect(layer.document.items.map(({ id, createdAt, updatedAt, ...rest }) => rest)).toEqual([
      {
        kind: "redline",
        view: "v1",
        category: "correction",
        points: [
          [10, 10],
          [40, 30],
        ],
        arrows: "end",
      },
      {
        kind: "redline",
        view: "v1",
        category: "correction",
        points: [
          [50, 10],
          [70, 10],
        ],
        arrows: "end",
      },
      {
        kind: "redline",
        view: "v1",
        lineWidth: 0.5,
        points: [
          [60, 20],
          [80, 20],
          [80, 40],
          [60, 40],
        ],
        closed: true,
      },
      expect.objectContaining({ kind: "redline", view: "v1" }),
    ]);
    const freehand = layer.document.items[3];
    expect(freehand?.kind === "redline" ? freehand.points.length : 0).toBe(5);
  });

  it("折線は最後の点のクリック（ダブルクリック）で終え、最初の点のクリックで閉じ、図面を拡大しない", async () => {
    const { viewer, layer } = await mount();
    viewer.addTool(redlineTool(layer, { mode: "polyline" }));
    viewer.addTool(cloudTool(layer, { mode: "polygon", arcLength: 5 }));
    viewer.setTool("markup-redline");
    const zoom = viewer.getView().zoom;
    click(viewer, 10, 10);
    click(viewer, 30, 10);
    click(viewer, 30, 30);
    click(viewer, 30, 30);
    viewer.canvas.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, cancelable: true }));
    expect(viewer.getView().zoom).toBe(zoom);
    click(viewer, 50, 10);
    click(viewer, 70, 10);
    click(viewer, 70, 30);
    click(viewer, 50, 10);
    // Enter で終え、Backspace で最後の点を消す
    click(viewer, 80, 10);
    click(viewer, 90, 10);
    click(viewer, 90, 40);
    key(viewer, "Backspace");
    key(viewer, "Enter");
    viewer.setTool("markup-cloud");
    click(viewer, 10, 40);
    click(viewer, 20, 40);
    click(viewer, 20, 48);
    click(viewer, 10, 40);
    expect(
      layer.document.items.map((item) => ("points" in item ? [item.kind, item.points, "closed" in item] : [])),
    ).toEqual([
      [
        "redline",
        [
          [10, 10],
          [30, 10],
          [30, 30],
        ],
        false,
      ],
      [
        "redline",
        [
          [50, 10],
          [70, 10],
          [70, 30],
        ],
        true,
      ],
      [
        "redline",
        [
          [80, 10],
          [90, 10],
        ],
        false,
      ],
      [
        "cloud",
        [
          [10, 40],
          [20, 40],
          [20, 48],
        ],
        false,
      ],
    ]);
    expect(layer.document.items[3]).toMatchObject({ arcLength: 5 });
  });

  it("メモ・指摘・写真の道具は下書きを create に渡し、返した項目を加える。null で取りやめる", async () => {
    const { viewer, layer } = await mount();
    const drafts: MarkupDraft[] = [];
    let resolve: (value: { text: string } | null) => void = () => {};
    viewer.addTool(
      textTool(layer, {
        height: 5,
        create: async (draft) => {
          drafts.push(draft);
          const answer = await new Promise<{ text: string } | null>((done) => {
            resolve = done;
          });
          return answer ? { ...draft, text: answer.text } : null;
        },
      }),
    );
    viewer.addTool(findingTool(layer, { create: (draft) => ({ ...draft, text: "壁厚" }) }));
    viewer.addTool(photoTool(layer, { create: (draft) => ({ ...draft, photo: "IMG_1.jpg" }) }));
    viewer.setTool("markup-text");
    click(viewer, 20, 20);
    // create が終わるまでは次の下書きを作らない
    click(viewer, 30, 30);
    expect(drafts).toEqual([{ kind: "text", view: "v1", position: [20, 20], text: "", height: 5 }]);
    resolve({ text: "確認" });
    await frames();
    click(viewer, 40, 20);
    resolve(null);
    await frames();
    viewer.setTool("markup-finding");
    // 線の上のピンは、その線を entities にする
    click(viewer, 50, 50);
    drag(viewer, [60, 10], [80, 30]);
    viewer.setTool("markup-photo");
    drag(viewer, [90, 20], [90, 40]);
    expect(layer.document.items.map(({ id, createdAt, updatedAt, ...rest }) => rest)).toEqual([
      { kind: "text", view: "v1", position: [20, 20], text: "確認", height: 5 },
      {
        kind: "finding",
        view: "v1",
        position: [50, 50],
        status: "open",
        label: "1",
        entities: ["top"],
        text: "壁厚",
      },
      {
        kind: "finding",
        view: "v1",
        position: [80, 30],
        status: "open",
        label: "2",
        points: [
          [60, 10],
          [80, 10],
          [80, 30],
          [60, 30],
        ],
        text: "壁厚",
      },
      { kind: "photo", view: "v1", position: [90, 20], photo: "IMG_1.jpg", direction: 90 },
    ]);
  });

  it("入力待ちの下書きは、取消・道具・図面・文書の切替後に加えない", async () => {
    const { viewer, layer } = await mount();
    let finish = () => {};
    viewer.addTool(
      textTool(layer, {
        create: (draft) =>
          new Promise((resolve) => {
            finish = () => resolve({ ...draft, text: "前の図面へのメモ" });
          }),
      }),
    );
    for (const cancel of [
      () => key(viewer, "Escape"),
      () => viewer.setTool("select"),
      () => viewer.setScene(drawing({ view: "v2" })),
      () => layer.setDocument(markup()),
    ]) {
      viewer.setTool("markup-text");
      click(viewer, 20, 20);
      cancel();
      finish();
      await frames();
      expect(layer.document.items).toEqual([]);
    }
  });

  it("編集の道具は選び、動かし、点を動かし、消す。動かした結果は 1 回で取り消せる", async () => {
    const { viewer, layer, events } = await mount({
      document: markup([
        {
          id: "line",
          kind: "redline",
          view: "v1",
          points: [
            [10, 10],
            [40, 10],
          ],
        },
        { id: "pin", kind: "finding", view: "v1", position: [70, 30], status: "open" },
      ]),
    });
    viewer.addTool(markupEditTool(layer));
    viewer.setTool("markup-edit");
    pointer(viewer, "pointermove", ...viewer.toScreen(25, 10), { buttons: 0 });
    expect(layer.getState().hovered).toBe("line");
    expect(viewer.canvas.style.cursor).toBe("move");
    click(viewer, 25, 10);
    expect(layer.getState().selection).toEqual(["line"]);
    click(viewer, 70, 30, { shiftKey: true });
    expect(layer.getState().selection).toEqual(["line", "pin"]);
    drag(viewer, [70, 30], [75, 35]);
    expect(layer.document.items.map((item) => ("points" in item ? item.points : item.position))).toEqual([
      [
        [15, 15],
        [45, 15],
      ],
      [75, 35],
    ]);
    expect(events).toHaveLength(1);
    // 選んである項目の点のつまみをドラッグして、1 点だけ動かす
    drag(viewer, [45, 15], [50, 20]);
    expect(layer.document.items[0]).toMatchObject({
      points: [
        [15, 15],
        [50, 20],
      ],
    });
    // Shift のクリックで選択から外す
    click(viewer, 75, 35, { shiftKey: true });
    expect(layer.getState().selection).toEqual(["line"]);
    key(viewer, "z", { ctrlKey: true });
    expect(layer.document.items[0]).toMatchObject({
      points: [
        [15, 15],
        [45, 15],
      ],
    });
    key(viewer, "z", { metaKey: true });
    expect(layer.document.items[0]).toMatchObject({
      points: [
        [10, 10],
        [40, 10],
      ],
    });
    key(viewer, "z", { ctrlKey: true, shiftKey: true });
    key(viewer, "Delete");
    expect(layer.document.items.map((item) => item.id)).toEqual(["pin"]);
    // 何もない所のドラッグは図面を動かし、クリックは選択を外す
    const center = viewer.getView().center;
    drag(viewer, [20, 40], [30, 40]);
    expect(viewer.getView().center[0]).toBeCloseTo(center[0] - 10, 6);
    layer.select(["pin"]);
    click(viewer, 20, 40);
    expect(layer.getState().selection).toEqual([]);
  });
});
