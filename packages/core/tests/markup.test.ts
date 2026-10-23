import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { insideRings } from "../src/geometry.js";
import {
  createMarkup,
  drawingHash,
  type FindingItem,
  MARKUP_PIXEL,
  type MarkupDocument,
  MarkupError,
  type MarkupItem,
  type MarkupShapeOptions,
  markupBox,
  markupFromJson,
  markupShapes,
  markupToJson,
  type Primitive,
  type RedlineItem,
} from "../src/index.js";

// tests/fixtures/markup の文書と図形は、Rust の zumen-core（markup のテスト）も読み、2 つの実装が同じ検査と同じ図形に
// なることを確かめる。図形の正解は pnpm run golden で書き直す
const fixture = (name: string) =>
  readFileSync(new URL(`../../../tests/fixtures/markup/${name}`, import.meta.url), "utf8");

const HASH = "9f2c6d1a4b7e8f0312a5c6d7e8f9a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b";

/** 図形の正解に書く、図形を作る条件 */
const SHAPE_OPTIONS: MarkupShapeOptions[] = [{}, { color: "#336699", pixel: 0.5 }];

function shapesJson(document: MarkupDocument): string {
  const runs = SHAPE_OPTIONS.map((options) => ({
    options,
    items: document.items.map((item) => ({ id: item.id, box: markupBox(item), shapes: markupShapes(item, options) })),
  }));
  return `${JSON.stringify(runs, (_key, value) => (value instanceof Float64Array ? Array.from(value) : value), 2)}\n`;
}

function doc(items: unknown[]): MarkupDocument {
  return markupFromJson({ ...createMarkup({ sha256: HASH, format: "sfc" }), items });
}

function points(primitive: Primitive | undefined): number[] {
  if (primitive?.kind !== "stroke") throw new Error("線ではありません");
  return Array.from(primitive.points);
}

describe("文書", () => {
  it("どの種類の項目も読み、知らないキーと data を残して書き戻す", () => {
    const text = fixture("kinds.markup.json");
    const document = markupFromJson(text);
    expect(document.items.map((item) => item.kind)).toEqual([
      "redline",
      "redline",
      "redline",
      "redline",
      "cloud",
      "cloud",
      "text",
      "text",
      "finding",
      "finding",
      "finding",
      "finding",
      "photo",
      "photo",
      "redline",
    ]);
    expect(JSON.parse(markupToJson(document))).toEqual(JSON.parse(text));
    expect((document as unknown as Record<string, unknown>)["x-extension"]).toEqual([1, "two", { three: 3 }]);
    // 読んだ値は複製なので、元のオブジェクトを変えても変わらない
    const source = JSON.parse(text);
    const copied = markupFromJson(source);
    source.items[0].points[0][0] = 999;
    expect(copied.items[0]).toMatchObject({
      points: [
        [20, 20],
        [60, 20],
        [60, 20],
        [80, 45],
      ],
    });
  });

  it("図形の正解と同じ図形を描く", () => {
    matchGolden("markup/kinds.shapes.json", shapesJson(markupFromJson(fixture("kinds.markup.json"))));
  });

  it.each(
    JSON.parse(fixture("errors.json")) as {
      name: string;
      text?: string;
      document?: unknown;
      code: string;
      item: number | null;
      id: string | null;
      key: string | null;
    }[],
  )("誤り: $name", ({ text, document, code, item, id, key }) => {
    let error: unknown;
    try {
      markupFromJson(text ?? (document as object));
    } catch (caught) {
      error = caught;
    }
    expect(error).toBeInstanceOf(MarkupError);
    expect(error).toMatchObject({ code, item, id, key });
    expect((error as Error).message).not.toBe("");
  });

  // JSON.parse は double を超える数を無限大にする。Rust の serde_json は JSON として読まない（code が json）ので、
  // この場合は errors.json に入れず、ここで確かめる
  it("double を超える数は、点の値の誤りにする", () => {
    const text = `{"format":"zumen-markup","version":1,"drawing":{"sha256":"${HASH}","format":"sfc"},"items":[{"id":"a","kind":"redline","points":[[0,0],[1e999,1]]}]}`;
    expect(() => markupFromJson(text)).toThrow(expect.objectContaining({ code: "value", item: 0, key: "points" }));
  });

  it("createMarkup は項目のない文書を作る", () => {
    const document = createMarkup({ sha256: HASH, format: "jww" });
    expect(document).toEqual({
      format: "zumen-markup",
      version: 1,
      drawing: { sha256: HASH, format: "jww" },
      items: [],
    });
    expect(markupFromJson(markupToJson(document))).toEqual(document);
  });

  it("drawingHash はバイトの SHA-256 を小文字の 16 進で返す", async () => {
    // FIPS 180-4 の例（"abc"）
    expect(await drawingHash(new TextEncoder().encode("abc"))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
    expect(await drawingHash(new ArrayBuffer(0))).toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
  });
});

describe("図形", () => {
  it("色は項目の color、options.color、既定の赤の順で、lineWidth のない線は 2 px の太さ", () => {
    const [own] = doc([
      {
        id: "a",
        kind: "redline",
        points: [
          [0, 0],
          [10, 0],
        ],
        color: "#00ff00",
      },
    ]).items as MarkupItem[];
    const [plain] = doc([
      {
        id: "b",
        kind: "redline",
        points: [
          [0, 0],
          [10, 0],
        ],
      },
    ]).items as MarkupItem[];
    expect(markupShapes(own as MarkupItem, { color: "#0000ff" })[0]).toMatchObject({ color: "#00ff00" });
    expect(markupShapes(plain as MarkupItem, { color: "#0000ff" })[0]).toMatchObject({ color: "#0000ff" });
    expect(markupShapes(plain as MarkupItem)[0]).toMatchObject({ color: "#ff0000", width: 2 * MARKUP_PIXEL });
    expect(markupShapes(plain as MarkupItem, { pixel: 0.1 })[0]).toMatchObject({ width: 0.2 });
  });

  it("矢印は端に先を置き、同じ位置の点を飛ばした向きに描く。閉じた線には描かない", () => {
    const [line, closed] = doc([
      {
        id: "a",
        kind: "redline",
        points: [
          [0, 0],
          [10, 0],
          [10, 0],
        ],
        arrows: "both",
        lineWidth: 1,
      },
      {
        id: "b",
        kind: "redline",
        points: [
          [0, 0],
          [10, 0],
          [10, 10],
        ],
        arrows: "both",
        closed: true,
      },
    ]).items as RedlineItem[];
    const shapes = markupShapes(line as RedlineItem);
    expect(shapes.map((shape) => shape.kind)).toEqual(["stroke", "fill", "fill"]);
    const [start, end] = shapes
      .slice(1)
      .map((shape) => (shape.kind === "fill" ? Array.from(shape.rings[0] ?? []) : []));
    expect(start).toEqual([0, 0, 6, -2, 6, 2]);
    expect(end).toEqual([10, 0, 4, 2, 4, -2]);
    expect(markupShapes(closed as RedlineItem).map((shape) => shape.kind)).toEqual(["stroke"]);
  });

  it.each([
    [
      "反時計回り",
      [
        [0, 0],
        [40, 0],
        [40, 20],
        [0, 20],
      ],
    ],
    [
      "時計回り",
      [
        [0, 0],
        [0, 20],
        [40, 20],
        [40, 0],
      ],
    ],
  ])("雲マークの弧は%sの外形でも外側へふくらみ、辺を弦の長さで等分する", (_name, outline) => {
    const [cloud] = doc([{ id: "c", kind: "cloud", points: outline, arcLength: 10 }]).items as MarkupItem[];
    const line = points(markupShapes(cloud as MarkupItem)[0]);
    // 辺の長さ 40 と 20 を 4 つと 2 つの弧に分け、弧ごとに 8 点
    expect(line.length / 2).toBe((4 + 2 + 4 + 2) * 8);
    const ring = [Float64Array.from(outline.flat())];
    for (let i = 0; i < line.length; i += 2) {
      const x = line[i] as number;
      const y = line[i + 1] as number;
      const onEdge = x === 0 || x === 40 || y === 0 || y === 20;
      expect(onEdge || !insideRings(x, y, ring)).toBe(true);
    }
    // 弧の高さは弦の半分 × tan(30°)（中心角 120 度）
    const box = markupBox(cloud as MarkupItem);
    expect(box.minY).toBeCloseTo(-5 * Math.tan(Math.PI / 6), 9);
    expect(box.maxX).toBeCloseTo(40 + 5 * Math.tan(Math.PI / 6), 9);
  });

  it("弦が短すぎる雲マークは、弧を 4096 個までにする", () => {
    const [cloud] = doc([
      {
        id: "c",
        kind: "cloud",
        points: [
          [0, 0],
          [1000, 0],
          [1000, 1000],
          [0, 1000],
        ],
        arcLength: 1e-6,
      },
    ]).items as MarkupItem[];
    expect(points(markupShapes(cloud as MarkupItem)[0]).length / 2).toBe(4096 * 8);
  });

  it("メモは 1 行目の左下を位置にし、行を文字の下の向きへ 1.5 倍の間隔で並べる", () => {
    const [note] = doc([{ id: "t", kind: "text", position: [10, 20], text: "一\n\n二", height: 4, angle: 90 }])
      .items as MarkupItem[];
    const shapes = markupShapes(note as MarkupItem);
    expect(shapes.map((shape) => (shape.kind === "text" ? shape.text : shape.kind))).toEqual(["一", "二"]);
    const second = shapes[1];
    if (second?.kind !== "text") throw new Error("文字ではありません");
    // 90 度回した文字の下は +x。3 行目（空の 2 行目の次）は 2 × 1.5 × 4 = 12 mm 先
    expect(second.matrix[4]).toBeCloseTo(22, 9);
    expect(second.matrix[5]).toBeCloseTo(20, 9);
    expect(markupBox(note as MarkupItem)).toMatchObject({ minX: expect.closeTo(6, 9), maxY: expect.closeTo(24, 9) });
  });

  it("指摘のピンは状態で描き方を変え、長い番号はピンに収まる高さにする", () => {
    const findings = doc(
      (["open", "resolved", "closed"] as const).map((status) => ({
        id: status,
        kind: "finding",
        position: [0, 0],
        status,
        label: status === "closed" ? "A-104" : "3",
      })),
    ).items as FindingItem[];
    const [open, resolved, closed] = findings.map((item) => markupShapes(item, { pixel: 1 }));
    expect(open?.map((shape) => [shape.kind, "color" in shape ? shape.color : ""])).toEqual([
      ["fill", "#ff0000"],
      ["text", "background"],
    ]);
    expect(resolved?.map((shape) => [shape.kind, "color" in shape ? shape.color : ""])).toEqual([
      ["fill", "background"],
      ["stroke", "#ff0000"],
      ["text", "#ff0000"],
    ]);
    expect(closed?.[0]).toMatchObject({ kind: "fill", color: "#ff000066" });
    const label = closed?.[1];
    // "A-104" は半角 5 文字で 2.5 文字分。ピンの半径 10 px の 1.6 倍の幅に収める
    expect(label?.kind === "text" ? label.height : 0).toBeCloseTo((10 * 1.6) / 2.5, 9);
    // 外接矩形はピンの大きさを含まない
    expect(markupBox(findings[0] as FindingItem)).toEqual({ minX: 0, minY: 0, maxX: 0, maxY: 0 });
  });
});
