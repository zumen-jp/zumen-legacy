import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  placeUnderlays,
  type Scene,
  type SceneEntity,
  sceneFromJson,
  sceneToJson,
  type Underlay,
} from "../src/index.js";

// Rust の zumen-core（types.rs のテスト）も同じファイルと比べ、2 つの実装の JSON がそろっていることを確かめる
const JSON_TEXT = readFileSync(new URL("./underlay.scene.json", import.meta.url), "utf8");

function line(id: string, y: number, color: string, width: number, drawn: boolean): SceneEntity {
  return {
    id,
    kind: "line",
    type: "線分",
    source: drawn ? "line" : "path",
    layers: [0],
    primitives: [
      { kind: "stroke", points: Float64Array.from([10, y, 100, y]), closed: false, color, width, dashes: [] },
    ],
    bbox: { minX: 10, minY: y, maxX: 100, maxY: y },
    properties: [],
    ...(drawn ? {} : { drawn: false as const }),
  };
}

/**
 * underlay.scene.json と同じ図面。PDF の 1 ページ全体と、レイヤ 1 の切り取った画像。ページが見せている線（描かない図形）と、
 * 描く線
 */
function scene(): Scene {
  const pt = 25.4 / 72;
  return {
    format: "test",
    version: "1",
    sheet: { name: "A4", size: "A4", width: 297, height: 210 },
    bounds: { minX: 0, minY: 0, maxX: 297, maxY: 210 },
    layers: [
      { id: 0, name: "0", visible: true },
      { id: 1, name: "写真", visible: true },
    ],
    entities: [line("1", 20, "#000000", 0.25, false), line("2", 30, "#ff0000", 0.5, true)],
    underlays: [
      {
        id: "page-1",
        kind: "pdf",
        page: 1,
        width: 841.89,
        height: 595.28,
        matrix: [pt, 0, 0, -pt, 0, 210],
        layers: [],
        contentLayers: [0],
        bbox: { minX: 0, minY: 0, maxX: 297, maxY: 210 },
      },
      {
        id: "1F",
        kind: "image",
        file: "photos\\site.jpg",
        width: 400,
        height: 300,
        matrix: [0.25, 0, 0, -0.25, 100, 175],
        clip: [Float64Array.from([100, 100, 200, 100, 200, 175, 100, 175])],
        opacity: 0.5,
        layers: [1],
        bbox: { minX: 100, minY: 100, maxX: 200, maxY: 175 },
      },
    ],
    notices: [],
    metadata: [],
    background: "light",
  };
}

describe("underlay", () => {
  it("has the same JSON as zumen-core", () => {
    expect(JSON.parse(sceneToJson(scene()))).toEqual(JSON.parse(JSON_TEXT));
  });

  it("comes back from JSON with typed clip rings", () => {
    const back = sceneFromJson(JSON_TEXT);
    const image = back.underlays?.[1];
    expect(image?.clip?.[0]).toBeInstanceOf(Float64Array);
    expect(back).toEqual(scene());
    expect(sceneFromJson(sceneToJson(back))).toEqual(back);
  });

  it("is left out of the JSON of a scene without underlays", () => {
    const { underlays: _, ...plain } = scene();
    const json = sceneToJson(plain);
    expect(JSON.parse(json)).not.toHaveProperty("underlays");
    expect(sceneFromJson(json)).not.toHaveProperty("underlays");
  });
});

/** 大きさの分からない PDF の underlay。1 pt = 0.5 mm、挿入点（ページの左下の角）は (10, 20) */
function unplaced(id: string, clip?: Float64Array[]): Underlay {
  const underlay: Underlay = {
    id,
    kind: "pdf",
    file: "plan.pdf",
    page: 2,
    width: 0,
    height: 0,
    matrix: [0.5, 0, 0, -0.5, 10, 20],
    layers: [0],
    bbox: { minX: 10, minY: 20, maxX: 10, maxY: 20 },
  };
  if (clip) {
    underlay.clip = clip;
    if (clip.every((ring) => ring.length > 0)) {
      const xs = clip.flatMap((ring) => [...ring].filter((_, i) => i % 2 === 0));
      const ys = clip.flatMap((ring) => [...ring].filter((_, i) => i % 2 === 1));
      underlay.bbox = { minX: Math.min(...xs), minY: Math.min(...ys), maxX: Math.max(...xs), maxY: Math.max(...ys) };
    }
  }
  return underlay;
}

function withUnderlays(...underlays: Underlay[]): Scene {
  return { ...scene(), bounds: { minX: 0, minY: 0, maxX: 50, maxY: 50 }, underlays };
}

const A4 = { width: 595, height: 842 };

describe("placeUnderlays", () => {
  it("moves the lower left corner of the page to the insertion point and grows the bounds", () => {
    const image = scene().underlays?.[1] as Underlay;
    const before = withUnderlays(unplaced("p"), image);
    const placed = placeUnderlays(before, (underlay) => (underlay.file === "plan.pdf" ? A4 : null));
    const page = placed.underlays?.[0] as Underlay;
    expect(page.width).toBe(595);
    expect(page.height).toBe(842);
    // 中身の左上 (0, 0) は高さの分だけ上、左下 (0, 842) が挿入点
    expect(page.matrix).toEqual([0.5, 0, 0, -0.5, 10, 441]);
    expect(page.bbox).toEqual({ minX: 10, minY: 20, maxX: 307.5, maxY: 441 });
    expect(placed.bounds).toEqual({ minX: 0, minY: 0, maxX: 307.5, maxY: 441 });
    expect(placed.underlays?.[1]).toBe(image);
    expect(before.underlays?.[0]?.width).toBe(0);
  });

  it("keeps the box of the clip and puts the outline of the page in its empty rings", () => {
    const window = Float64Array.from([0, 0, 100, 0, 100, 100, 0, 100]);
    const placed = placeUnderlays(
      withUnderlays(unplaced("inside", [window]), unplaced("outside", [new Float64Array(0), window])),
      () => A4,
    );
    const [inside, outside] = placed.underlays as [Underlay, Underlay];
    expect(inside.clip).toEqual([window]);
    expect(inside.bbox).toEqual({ minX: 10, minY: 20, maxX: 100, maxY: 100 });
    expect(outside.clip?.[0]).toEqual(Float64Array.from([10, 441, 307.5, 441, 307.5, 20, 10, 20]));
    expect(outside.clip?.[1]).toBe(window);
    expect(outside.bbox).toEqual({ minX: 10, minY: 20, maxX: 307.5, maxY: 441 });
  });

  it("leaves out an underlay whose page is outside its clip", () => {
    const away = Float64Array.from([-100, -100, -50, -100, -50, -50, -100, -50]);
    const placed = placeUnderlays(withUnderlays(unplaced("away", [away]), unplaced("p")), () => A4);
    expect(placed.underlays?.map((underlay) => underlay.id)).toEqual(["p"]);
  });

  it("returns the same scene when it places nothing", () => {
    const before = withUnderlays(unplaced("p"), scene().underlays?.[0] as Underlay);
    expect(placeUnderlays(before, () => null)).toBe(before);
    expect(placeUnderlays(before, () => ({ width: 0, height: 842 }))).toBe(before);
    expect(placeUnderlays(before, () => ({ width: Number.NaN, height: 842 }))).toBe(before);
    expect(placeUnderlays(scene(), () => A4)).toEqual(scene());
  });

  it("keeps the empty ring through JSON", () => {
    const json = sceneToJson(
      withUnderlays(unplaced("outside", [new Float64Array(0), Float64Array.of(0, 0, 1, 0, 1, 1)])),
    );
    const ring = sceneFromJson(json).underlays?.[0]?.clip?.[0];
    expect(ring).toBeInstanceOf(Float64Array);
    expect(ring?.length).toBe(0);
  });
});
