import type { Scene, SceneEntity, Underlay, UnderlayRequest } from "@zumenjs/core";
import { afterEach, describe, expect, it } from "vitest";
import {
  DrawingViewer,
  type DrawingViewerOptions,
  imageUnderlayRenderer,
  renderScene,
  type UnderlayEvent,
  type UnderlayRenderer,
  type UnderlayTarget,
} from "../src/index.js";

// underlay を描き手に求め、図形の下に描くことを Chromium で確かめる。描いた結果は Canvas の画素で見る

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

/** 太さ 2 mm の白い水平線 */
function line(id: string, y: number): SceneEntity {
  return {
    id,
    kind: "line",
    type: "線分",
    source: "line",
    layers: [0],
    primitives: [
      {
        kind: "stroke",
        color: "#ffffff",
        width: 2,
        dashes: [],
        points: Float64Array.from([0, y, 100, y]),
        closed: false,
      },
    ],
    bbox: { minX: 0, minY: y, maxX: 100, maxY: y },
    properties: [],
  };
}

/** 用紙の (0, 0)〜(100, 50) を覆う underlay。中身は width × height で、上下を反転して用紙に写す */
function underlay(overrides: Partial<Underlay> = {}): Underlay {
  const width = overrides.width ?? 100;
  const height = overrides.height ?? 50;
  return {
    id: "u1",
    kind: "image",
    width,
    height,
    matrix: [100 / width, 0, 0, -50 / height, 0, 50],
    layers: [1],
    bbox: { minX: 0, minY: 0, maxX: 100, maxY: 50 },
    ...overrides,
  };
}

function drawing(underlays: Underlay[] | undefined, overrides: Partial<Scene> = {}): Scene {
  return {
    format: "test",
    version: "1",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 100, maxY: 50 },
    layers: [
      { id: 0, name: "0", visible: true },
      { id: 1, name: "画像", visible: true },
    ],
    entities: [line("top", 40)],
    ...(underlays ? { underlays } : {}),
    notices: [],
    metadata: [],
    background: "dark",
    ...overrides,
  };
}

const frames = async (count = 2) => {
  for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
};

async function mount(scene: Scene, options: DrawingViewerOptions = {}) {
  const container = document.createElement("div");
  container.style.cssText = "width: 400px; height: 300px;";
  document.body.append(container);
  const viewer = new DrawingViewer(container, scene, options);
  const events: UnderlayEvent[] = [];
  viewer.on("underlay", (event) => events.push(event));
  cleanups.push(() => {
    viewer.destroy();
    container.remove();
  });
  await frames();
  return { viewer, events, base: container.querySelector("canvas") as HTMLCanvasElement };
}

/** 条件が満たされるまで待つ（表示が止まってから求める要求と、描いた結果の再描画） */
async function until(condition: () => boolean, timeout = 2000): Promise<void> {
  const start = performance.now();
  while (!condition()) {
    if (performance.now() - start > timeout) throw new Error("待ちきれません");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await frames();
}

/** Canvas の点（CSS px）の色 */
function pixel(canvas: HTMLCanvasElement, px: number, py: number): number[] {
  const ratio = canvas.width / canvas.getBoundingClientRect().width;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  return [...ctx.getImageData(Math.floor(px * ratio), Math.floor(py * ratio), 1, 1).data];
}

/** 要求の範囲を 1 色で塗る描き手 */
function solid(color: string, requests: UnderlayRequest[] = []): UnderlayRenderer {
  return (request, { context }) => {
    requests.push(request);
    context.fillStyle = color;
    context.fillRect(0, 0, request.width, request.height);
  };
}

/** 呼ばれた要求を持ち、resolve を呼ぶまで描き終えない描き手 */
function deferred() {
  const calls: { request: UnderlayRequest; target: UnderlayTarget; resolve: () => void }[] = [];
  const renderer: UnderlayRenderer = (request, target) =>
    new Promise<void>((resolve) => {
      calls.push({
        request,
        target,
        resolve: () => {
          target.context.fillStyle = "#00ff00";
          target.context.fillRect(0, 0, request.width, request.height);
          resolve();
        },
      });
    });
  return { calls, renderer };
}

describe("underlay", () => {
  it("描き手が描いた中身を、背景の上、図形の下に描く", async () => {
    const requests: UnderlayRequest[] = [];
    const { viewer, events, base } = await mount(drawing([underlay()]), {
      underlayRenderer: solid("#ff0000", requests),
    });
    await until(() => events.length > 0);
    expect(events[0]?.error).toBeNull();
    // 画像の中身は画素より細かく描かない。全体を一度に描く
    expect(requests).toHaveLength(1);
    expect(requests[0]?.region).toEqual({ x: 0, y: 0, width: 100, height: 50 });
    expect(requests[0]?.scale).toBe(1);
    expect([requests[0]?.width, requests[0]?.height]).toEqual([100, 50]);
    expect(pixel(base, ...viewer.toScreen(50, 20))).toEqual([255, 0, 0, 255]);
    // 線は underlay の上に描く
    expect(pixel(base, ...viewer.toScreen(50, 40))).toEqual([255, 255, 255, 255]);
    // underlay の外は背景
    expect(pixel(base, ...viewer.toScreen(50, 60))).toEqual([0, 0, 0, 255]);
  });

  it("不透明度と切り取りを守り、非表示のレイヤの underlay は描かず、求めもしない", async () => {
    const clipped = underlay({
      opacity: 0.5,
      clip: [Float64Array.from([0, 0, 50, 0, 50, 50, 0, 50])],
      bbox: { minX: 0, minY: 0, maxX: 50, maxY: 50 },
    });
    const requests: UnderlayRequest[] = [];
    const { viewer, events, base } = await mount(drawing([clipped]), { underlayRenderer: solid("#ff0000", requests) });
    await until(() => events.length > 0);
    const [r, g, b] = pixel(base, ...viewer.toScreen(25, 20));
    expect(Math.abs((r as number) - 128)).toBeLessThan(3);
    expect([g, b]).toEqual([0, 0]);
    expect(pixel(base, ...viewer.toScreen(75, 20))).toEqual([0, 0, 0, 255]);
    viewer.setLayersVisible([1], false);
    await frames();
    expect(pixel(base, ...viewer.toScreen(25, 20))).toEqual([0, 0, 0, 255]);

    const hidden = await mount(drawing([underlay()], { layers: [{ id: 1, name: "画像", visible: false }] }), {
      underlayRenderer: solid("#ff0000", requests),
    });
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(requests).toHaveLength(1);
    expect(hidden.events).toEqual([]);
  });

  it("全体では粗すぎる表示では見えている範囲を求め、表示が動いて合わなくなった要求は取り消す", async () => {
    // 中身 10000 × 5000（1 が用紙の 0.01 mm）。拡大すると、全体を一度に描くには細かすぎる
    const page = underlay({ id: "page", kind: "pdf", page: 1, width: 10000, height: 5000, layers: [] });
    const { calls, renderer } = deferred();
    const { viewer, events, base } = await mount(drawing([page]), { underlayRenderer: renderer });
    const isWhole = (call: (typeof calls)[number]) => call.request.region.width === 10000;
    await until(() => calls.length === 1);
    const first = calls[0] as (typeof calls)[number];
    expect(isWhole(first)).toBe(true);
    // 全体の画像は、今の表示の 2 倍の細かさで描いておく
    expect(first.request.scale).toBeCloseTo(viewer.getView().zoom * (96 / 25.4) * devicePixelRatio * 0.01 * 2, 6);
    first.resolve();
    await until(() => events.length === 1);
    expect(pixel(base, ...viewer.toScreen(50, 20))).toEqual([0, 255, 0, 255]);

    // 拡大すると、見えている範囲を今の細かさで求め、全体の画像も一度に描ける細かさで求め直す
    viewer.jumpTo({ center: [25, 25], zoom: 100 });
    await until(() => calls.length === 3);
    const part = calls[1] as (typeof calls)[number];
    const upgrade = calls[2] as (typeof calls)[number];
    expect(isWhole(part)).toBe(false);
    expect(isWhole(upgrade)).toBe(true);
    // 全体を一度に描く画素数の上限は 2048 × 2048
    expect(upgrade.request.scale).toBeCloseTo(Math.sqrt((2048 * 2048) / (10000 * 5000)), 9);
    const view = viewer.getView();
    expect(part.request.scale).toBeCloseTo(view.zoom * (96 / 25.4) * devicePixelRatio * 0.01, 6);
    const { region } = part.request;
    expect(region.x).toBeLessThanOrEqual((view.bounds.minX / 100) * 10000);
    expect(region.x + region.width).toBeGreaterThanOrEqual((view.bounds.maxX / 100) * 10000);
    expect(part.target.signal.aborted).toBe(false);

    // 描き終わる前に遠くへ動かすと、見えている範囲の古い要求を取り消し、止まってから新しい範囲を求める。
    // 全体の要求は動かしても古くならないので残す
    viewer.jumpTo({ center: [75, 25] });
    expect(part.target.signal.aborted).toBe(true);
    expect(upgrade.target.signal.aborted).toBe(false);
    await until(() => calls.length === 4);
    const next = calls[3] as (typeof calls)[number];
    expect(isWhole(next)).toBe(false);
    expect(next.request.region.x).toBeGreaterThan(region.x + region.width);
    next.resolve();
    part.resolve();
    await until(() => events.length === 2);
    expect(events[1]?.request).toBe(next.request);

    // 図面を替えると、描き途中の要求を取り消す
    viewer.setScene(drawing(undefined));
    expect(upgrade.target.signal.aborted).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(calls).toHaveLength(4);
  });

  it("中身の中のレイヤの表示が変わると、非表示のレイヤを渡して描き直しを求める", async () => {
    const page = underlay({ id: "page", kind: "pdf", page: 1, layers: [], contentLayers: [0, 1] });
    const requests: UnderlayRequest[] = [];
    const { viewer, events } = await mount(drawing([page]), { underlayRenderer: solid("#ff0000", requests) });
    await until(() => events.length === 1);
    expect(requests[0]?.hiddenLayers).toEqual([]);
    viewer.setLayersVisible([1], false);
    await until(() => events.length === 2);
    expect(requests[1]?.hiddenLayers).toEqual([1]);
  });

  it("描き手が拒否した underlay は知らせて求め直さず、refreshUnderlays でもう一度求める", async () => {
    let fail = true;
    let count = 0;
    const renderer: UnderlayRenderer = (request, { context }) => {
      count++;
      if (fail) throw new Error("ファイルがありません");
      context.fillStyle = "#0000ff";
      context.fillRect(0, 0, request.width, request.height);
    };
    const { viewer, events, base } = await mount(drawing([underlay()]), { underlayRenderer: renderer });
    await until(() => events.length === 1);
    expect(events[0]?.error).toEqual(new Error("ファイルがありません"));
    viewer.zoomBy(1.5);
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(count).toBe(1);
    fail = false;
    viewer.refreshUnderlays(["u1"]);
    await until(() => events.length === 2);
    expect(events[1]?.error).toBeNull();
    expect(pixel(base, ...viewer.toScreen(50, 20))).toEqual([0, 0, 255, 255]);
    // 描き手を外すと描かない
    viewer.setUnderlayRenderer(null);
    await frames();
    expect(pixel(base, ...viewer.toScreen(50, 20))).toEqual([0, 0, 0, 255]);
  });

  it("underlay のない図面では描き手を呼ばない", async () => {
    const requests: UnderlayRequest[] = [];
    await mount(drawing(undefined), { underlayRenderer: solid("#ff0000", requests) });
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(requests).toEqual([]);
  });

  it("imageUnderlayRenderer は画像の範囲を、画像の大きさに合わせて描く", async () => {
    // 左半分が赤、右半分が青の 4 × 2 px の画像を、中身 100 × 50 の underlay に描く
    const image = document.createElement("canvas");
    image.width = 4;
    image.height = 2;
    const ictx = image.getContext("2d") as CanvasRenderingContext2D;
    ictx.fillStyle = "#ff0000";
    ictx.fillRect(0, 0, 2, 2);
    ictx.fillStyle = "#0000ff";
    ictx.fillRect(2, 0, 2, 2);
    const loaded: Underlay[] = [];
    const renderer = imageUnderlayRenderer(async (target) => {
      loaded.push(target);
      return image;
    });
    const { viewer, events, base } = await mount(drawing([underlay()]), { underlayRenderer: renderer });
    await until(() => events.length === 1);
    expect(pixel(base, ...viewer.toScreen(10, 20))).toEqual([255, 0, 0, 255]);
    expect(pixel(base, ...viewer.toScreen(90, 20))).toEqual([0, 0, 255, 255]);
    viewer.refreshUnderlays();
    await until(() => events.length === 2);
    expect(loaded).toHaveLength(1);
  });

  it("renderScene は渡した画像を、範囲と縮尺に合わせて描く", () => {
    const canvas = document.createElement("canvas");
    canvas.width = 100;
    canvas.height = 50;
    const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
    const image = document.createElement("canvas");
    image.width = 10;
    image.height = 10;
    const ictx = image.getContext("2d") as CanvasRenderingContext2D;
    ictx.fillStyle = "#ffff00";
    ictx.fillRect(0, 0, 10, 10);
    // 中身の (50, 0)〜(100, 50) を 0.2 px / 単位で描いた画像は、用紙の右半分を覆う
    renderScene(ctx, drawing([underlay()], { entities: [] }), { scale: 1, offsetX: 0, offsetY: 50 }, 100, 50, {
      background: "#000000",
      invertLight: false,
      hiddenLayers: new Set(),
      lineWeights: true,
      sheetFrame: false,
      devicePixelRatio: 1,
      underlays: [{ underlay: underlay(), region: { x: 50, y: 0, width: 50, height: 50 }, scale: 0.2, image }],
    });
    const at = (x: number, y: number) => [...ctx.getImageData(x, y, 1, 1).data];
    expect(at(75, 25)).toEqual([255, 255, 0, 255]);
    expect(at(25, 25)).toEqual([0, 0, 0, 255]);
  });
});
