/// <reference types="vite/client" />
import type { Scene, SceneEntity, Underlay, UnderlayRequest } from "@zumenjs/core";
import { DrawingViewer, type UnderlayEvent, type UnderlayRenderer } from "@zumenjs/viewer";
import { GlobalWorkerOptions, getDocument, type PDFDocumentProxy, type RenderTask } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pdfUnderlayRenderer } from "../src/index.js";
import { drawingPdf } from "./pdf.js";

// テストで作った PDF のページを pdf.js で描き、描いた Canvas の画素を見る。pdf.js はワーカーで PDF を読む
GlobalWorkerOptions.workerSrc = workerUrl;

const RED = [255, 0, 0, 255];
const GREEN = [0, 255, 0, 255];
const BLUE = [0, 0, 255, 255];
const YELLOW = [255, 255, 0, 255];
const CYAN = [0, 255, 255, 255];
const MAGENTA = [255, 0, 255, 255];

/** 1 pt の用紙の mm */
const PT = 25.4 / 72;

let pdf: PDFDocumentProxy;
beforeAll(async () => {
  pdf = await getDocument({ data: drawingPdf() }).promise;
});
afterAll(async () => {
  await pdf.loadingTask.destroy();
});

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const cleanup of cleanups.splice(0)) cleanup();
});

/** PDF の図面のページの underlay。用紙の (0, 0) を左下に、ページの pt を mm にして置く */
function page(number: number, overrides: Partial<Underlay> = {}): Underlay {
  const width = overrides.width ?? (number === 2 ? 100 : 200);
  const height = overrides.height ?? (number === 2 ? 200 : 100);
  return {
    id: `page-${number}`,
    kind: "pdf",
    page: number,
    width,
    height,
    matrix: [PT, 0, 0, -PT, 0, height * PT],
    layers: [],
    bbox: { minX: 0, minY: 0, maxX: width * PT, maxY: height * PT },
    ...overrides,
  };
}

/** 描き手を直に呼び、描いた Canvas を返す */
async function draw(
  renderer: UnderlayRenderer,
  underlay: Underlay,
  region: UnderlayRequest["region"],
  scale: number,
  hiddenLayers: readonly number[] = [],
  controller = new AbortController(),
): Promise<HTMLCanvasElement> {
  const request: UnderlayRequest = {
    underlay,
    region,
    scale,
    width: Math.ceil(region.width * scale),
    height: Math.ceil(region.height * scale),
    hiddenLayers,
  };
  const canvas = document.createElement("canvas");
  canvas.width = request.width;
  canvas.height = request.height;
  const context = canvas.getContext("2d") as CanvasRenderingContext2D;
  const result = await renderer(request, { canvas, context, signal: controller.signal });
  expect(result).toBeUndefined();
  return canvas;
}

/** Canvas の画素の色 */
function pixel(canvas: HTMLCanvasElement, x: number, y: number): number[] {
  const context = canvas.getContext("2d") as CanvasRenderingContext2D;
  return [...context.getImageData(Math.floor(x), Math.floor(y), 1, 1).data];
}

const whole = (underlay: Underlay) => ({ x: 0, y: 0, width: underlay.width, height: underlay.height });

describe("pdfUnderlayRenderer", () => {
  it("要求の範囲だけを、要求の細かさで描く", async () => {
    const renderer = pdfUnderlayRenderer({ document: () => pdf });
    // 赤と青の境目のまわり（中身の x 90〜110、y 30〜70）を 1 pt あたり 2 px で
    const middle = await draw(renderer, page(1), { x: 90, y: 30, width: 20, height: 40 }, 2);
    expect([middle.width, middle.height]).toEqual([40, 80]);
    expect(pixel(middle, 10, 40)).toEqual(RED);
    expect(pixel(middle, 30, 40)).toEqual(BLUE);
    // y は下向き。ページの下の緑の帯は中身の y 90〜100
    const bottom = await draw(renderer, page(1), { x: 0, y: 80, width: 200, height: 20 }, 1);
    expect(pixel(bottom, 50, 5)).toEqual(RED);
    expect(pixel(bottom, 150, 5)).toEqual(BLUE);
    expect(pixel(bottom, 50, 15)).toEqual(GREEN);
    expect(pixel(bottom, 150, 15)).toEqual(GREEN);
  });

  it("/Rotate を済ませた向きの座標で描く", async () => {
    const renderer = pdfUnderlayRenderer({ document: () => pdf });
    const rotated = page(2);
    const canvas = await draw(renderer, rotated, whole(rotated), 1);
    expect([canvas.width, canvas.height]).toEqual([100, 200]);
    expect(pixel(canvas, 50, 50)).toEqual(RED);
    expect(pixel(canvas, 50, 150)).toEqual(BLUE);
    expect(pixel(canvas, 5, 50)).toEqual(GREEN);
    expect(pixel(canvas, 5, 150)).toEqual(GREEN);
  });

  it("ページの大きさが underlay の width・height と違っても、中身の全体に合わせて描く", async () => {
    const renderer = pdfUnderlayRenderer({ document: () => pdf });
    // 中身の 1 がページの 0.5 pt（横）と 0.25 pt（縦）
    const stretched = page(1, { width: 400, height: 400 });
    const canvas = await draw(renderer, stretched, whole(stretched), 0.5);
    expect([canvas.width, canvas.height]).toEqual([200, 200]);
    expect(pixel(canvas, 50, 50)).toEqual(RED);
    expect(pixel(canvas, 150, 50)).toEqual(BLUE);
    expect(pixel(canvas, 50, 190)).toEqual(GREEN);
    expect(pixel(canvas, 50, 170)).toEqual(RED);
  });

  it("中身のレイヤの表示を optional content group の表示にする", async () => {
    const layered = page(1, { contentLayers: [3, 4] });
    const groups = new Map([
      [3, "6R"],
      [4, "7R"],
    ]);
    const asked: [number, string][] = [];
    const renderer = pdfUnderlayRenderer({
      document: () => pdf,
      optionalContent: (layer, underlay) => {
        asked.push([layer, underlay.id]);
        return groups.get(layer);
      },
    });
    // すべて表示: 既定で非表示の B も、レイヤが表示なら描く
    const shown = await draw(renderer, layered, whole(layered), 1);
    expect(pixel(shown, 30, 50)).toEqual(YELLOW);
    expect(pixel(shown, 170, 50)).toEqual(CYAN);
    expect(asked).toEqual([
      [3, "page-1"],
      [4, "page-1"],
    ]);
    // A のレイヤを隠す
    const hidden = await draw(renderer, layered, whole(layered), 1, [3]);
    expect(pixel(hidden, 30, 50)).toEqual(RED);
    expect(pixel(hidden, 170, 50)).toEqual(CYAN);
    // 1 つのレイヤが複数の group を切り替える
    const both = pdfUnderlayRenderer({ document: () => pdf, optionalContent: () => ["6R", "7R"] });
    const none = await draw(both, page(1, { contentLayers: [3] }), whole(layered), 1, [3]);
    expect(pixel(none, 30, 50)).toEqual(RED);
    expect(pixel(none, 170, 50)).toEqual(BLUE);
    // group を知らなければ文書の既定の表示（A は表示、B は非表示）で描く
    const plain = pdfUnderlayRenderer({ document: () => pdf });
    const defaults = await draw(plain, layered, whole(layered), 1, [3, 4]);
    expect(pixel(defaults, 30, 50)).toEqual(YELLOW);
    expect(pixel(defaults, 170, 50)).toEqual(BLUE);
  });

  it("注釈の外観を描き、annotations が false なら描かない", async () => {
    const withAnnotations = await draw(pdfUnderlayRenderer({ document: () => pdf }), page(1), whole(page(1)), 1);
    expect(pixel(withAnnotations, 70, 30)).toEqual(MAGENTA);
    const without = await draw(
      pdfUnderlayRenderer({ document: () => pdf, annotations: false }),
      page(1),
      whole(page(1)),
      1,
    );
    expect(pixel(without, 70, 30)).toEqual(RED);
  });

  it("ページの背景を background で替える", async () => {
    // 3 ページ目は左下の赤い正方形のほかに何も描かない
    const empty = page(3, { width: 100, height: 100 });
    const white = await draw(pdfUnderlayRenderer({ document: () => pdf }), empty, whole(empty), 1);
    expect(pixel(white, 5, 95)).toEqual(RED);
    expect(pixel(white, 50, 50)).toEqual([255, 255, 255, 255]);
    const clear = await draw(
      pdfUnderlayRenderer({ document: () => pdf, background: "rgba(0, 0, 0, 0)" }),
      empty,
      whole(empty),
      1,
    );
    expect(pixel(clear, 5, 95)).toEqual(RED);
    expect(pixel(clear, 50, 50)[3]).toBe(0);
  });

  it("signal が中止されたら pdf.js の描画を取り消し、何も描かない", async () => {
    const tasks: RenderTask[] = [];
    let onRender: () => void = () => {};
    // 描き手が使う pdf.js のメソッドだけを持つ文書。描画の要求を横で見る
    const watched = {
      getPage: async (number: number) => {
        const original = await pdf.getPage(number);
        return {
          getViewport: (parameters: Parameters<typeof original.getViewport>[0]) => original.getViewport(parameters),
          render: (parameters: Parameters<typeof original.render>[0]) => {
            const task = original.render(parameters);
            tasks.push(task);
            onRender();
            return task;
          },
        };
      },
      getOptionalContentConfig: (parameters: Parameters<typeof pdf.getOptionalContentConfig>[0]) =>
        pdf.getOptionalContentConfig(parameters),
    } as unknown as PDFDocumentProxy;
    const renderer = pdfUnderlayRenderer({ document: () => watched });

    // render を呼んでいる間に中止された
    const during = new AbortController();
    onRender = () => during.abort();
    const first = await draw(renderer, page(1), whole(page(1)), 1, [], during);
    expect(tasks).toHaveLength(1);
    await expect(tasks[0]?.promise).rejects.toMatchObject({ name: "RenderingCancelledException" });
    expect(pixel(first, 50, 50)[3]).toBe(0);

    // 描いている途中で中止された
    const after = new AbortController();
    onRender = () => queueMicrotask(() => after.abort());
    const second = await draw(renderer, page(1), whole(page(1)), 1, [], after);
    expect(tasks).toHaveLength(2);
    await expect(tasks[1]?.promise).rejects.toMatchObject({ name: "RenderingCancelledException" });
    expect(pixel(second, 50, 50)[3]).toBe(0);

    // 始める前に中止されていれば、描画を求めない
    const before = new AbortController();
    before.abort();
    await draw(renderer, page(1), whole(page(1)), 1, [], before);
    expect(tasks).toHaveLength(2);
  });

  it("文書はファイルごとに一度だけ求め、得られなければ次の要求でまた求める", async () => {
    const asked: (string | undefined)[] = [];
    let missing = true;
    const renderer = pdfUnderlayRenderer({
      document: (file) => {
        asked.push(file);
        if (file === undefined) return Promise.resolve(pdf);
        return missing ? null : pdf;
      },
    });
    await draw(renderer, page(1), whole(page(1)), 0.5);
    await draw(renderer, page(2), whole(page(2)), 0.5);
    expect(asked).toEqual([undefined]);

    const external = page(1, { id: "xref", file: "plan.pdf" });
    await expect(draw(renderer, external, whole(external), 0.5)).rejects.toThrow("underlay xref の PDF がありません");
    missing = false;
    const canvas = await draw(renderer, external, whole(external), 0.5);
    expect(pixel(canvas, 25, 25)).toEqual(RED);
    await draw(renderer, external, whole(external), 0.5);
    expect(asked).toEqual([undefined, "plan.pdf", "plan.pdf"]);

    const failing = pdfUnderlayRenderer({
      document: () => {
        throw new Error("開けません");
      },
    });
    await expect(draw(failing, page(1), whole(page(1)), 1)).rejects.toThrow("開けません");
  });

  it("PDF でない underlay と、ないページは拒否する", async () => {
    const renderer = pdfUnderlayRenderer({ document: () => pdf });
    const image = page(1, { id: "image", kind: "image" });
    await expect(draw(renderer, image, whole(image), 1)).rejects.toThrow(
      "underlay image は PDF ではありません（image）",
    );
    const absent = page(4);
    await expect(draw(renderer, absent, whole(absent), 1)).rejects.toThrow();
  });
});

/** 太さ 1 mm の白い水平線 */
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
        width: 1,
        dashes: [],
        points: Float64Array.from([0, y, 200 * PT, y]),
        closed: false,
      },
    ],
    bbox: { minX: 0, minY: y, maxX: 200 * PT, maxY: y },
    properties: [],
  };
}

/** 1 ページ目を underlay にした、PDF の図面のような表示 */
function sheet(): Scene {
  return {
    format: "test",
    version: "1",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 200 * PT, maxY: 100 * PT },
    layers: [
      { id: 0, name: "0", visible: true },
      { id: 3, name: "A", visible: true },
      { id: 4, name: "B", visible: false },
    ],
    entities: [line("line", 25 * PT)],
    underlays: [page(1, { contentLayers: [3, 4] })],
    notices: [],
    metadata: [],
    background: "dark",
  };
}

const frames = async (count = 2) => {
  for (let i = 0; i < count; i++) await new Promise((resolve) => requestAnimationFrame(resolve));
};

/** 条件が満たされるまで待つ（表示が止まってから求める要求と、描いた結果の再描画） */
async function until(condition: () => boolean, timeout = 5000): Promise<void> {
  const start = performance.now();
  while (!condition()) {
    if (performance.now() - start > timeout) throw new Error("待ちきれません");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  await frames();
}

describe("DrawingViewer と pdfUnderlayRenderer", () => {
  it("ページを図形の下に描き、拡大すると見えている範囲を鮮明に描き直し、レイヤを切り替える", async () => {
    const container = document.createElement("div");
    container.style.cssText = "width: 400px; height: 300px;";
    document.body.append(container);
    const groups = new Map([
      [3, "6R"],
      [4, "7R"],
    ]);
    const viewer = new DrawingViewer(container, sheet(), {
      underlayRenderer: pdfUnderlayRenderer({ document: () => pdf, optionalContent: (layer) => groups.get(layer) }),
    });
    const events: UnderlayEvent[] = [];
    viewer.on("underlay", (event) => events.push(event));
    cleanups.push(() => {
      viewer.destroy();
      container.remove();
    });
    const base = container.querySelector("canvas") as HTMLCanvasElement;
    /** 用紙の点（ページの pt で、左上が原点）の画素 */
    const at = (x: number, y: number) => {
      const [px, py] = viewer.toScreen(x * PT, (100 - y) * PT);
      const ratio = base.width / base.getBoundingClientRect().width;
      return pixel(base, px * ratio, py * ratio);
    };

    await until(() => events.length > 0);
    expect(events.every((event) => event.error === null)).toBe(true);
    expect(at(50, 50)).toEqual(RED);
    expect(at(150, 50)).toEqual(BLUE);
    // A は表示、B は非表示のレイヤ
    expect(at(30, 50)).toEqual(YELLOW);
    expect(at(170, 50)).toEqual(BLUE);
    // 線は underlay の上に描く
    expect(at(150, 75)).toEqual([255, 255, 255, 255]);

    // 全体の画像では粗すぎるほど拡大すると、見えている範囲を今の細かさで求める
    const count = events.length;
    viewer.jumpTo({ center: [100 * PT, 50 * PT], zoom: 200 });
    await until(() => events.length > count && events.some((event) => event.request.region.width < 200));
    const part = events.find((event) => event.request.region.width < 200)?.request as UnderlayRequest;
    expect(part.region.x).toBeLessThan(100);
    expect(part.region.x + part.region.width).toBeGreaterThan(100);
    expect(part.scale).toBeGreaterThan(Math.sqrt((2048 * 2048) / (200 * 100)));
    expect(at(99.8, 50)).toEqual(RED);
    expect(at(100.2, 50)).toEqual(BLUE);

    // 中身のレイヤを切り替えると、描き直しを求める
    viewer.fit();
    const before = events.length;
    viewer.setLayersVisible([3], false);
    viewer.setLayersVisible([4], true);
    await until(() => events.length > before);
    await until(() => at(30, 50)[1] === 0);
    expect(at(30, 50)).toEqual(RED);
    expect(at(170, 50)).toEqual(CYAN);
  });
});
