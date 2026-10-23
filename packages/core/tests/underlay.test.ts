import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { type Scene, sceneFromJson, sceneToJson, UNDERLAY_KINDS } from "../src/index.js";

// Rust の zumen-core（types.rs のテスト）も同じファイルと比べ、2 つの実装の JSON がそろっていることを確かめる
const JSON_TEXT = readFileSync(new URL("./underlay.scene.json", import.meta.url), "utf8");

/** underlay.scene.json と同じ図面。PDF の 1 ページ全体と、レイヤ 1 の切り取った画像 */
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
    entities: [],
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

  it("lists the kinds that zumen-core has", () => {
    expect(UNDERLAY_KINDS).toEqual(["image", "pdf"]);
  });
});
