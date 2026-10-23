import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { type Scene, type SceneEntity, sceneFromJson, sceneToJson, UNDERLAY_KINDS } from "../src/index.js";

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

  it("keeps drawn: false on the entities that an underlay shows, and leaves it out on the others", () => {
    const [shown, drawn] = JSON.parse(sceneToJson(scene())).entities;
    expect(shown.drawn).toBe(false);
    expect(drawn).not.toHaveProperty("drawn");
    expect(sceneFromJson(JSON_TEXT).entities.map((entity) => entity.drawn)).toEqual([false, undefined]);
  });

  it("lists the kinds that zumen-core has", () => {
    expect(UNDERLAY_KINDS).toEqual(["image", "pdf"]);
  });
});
