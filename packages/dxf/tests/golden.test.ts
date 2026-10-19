import { readDrawing, sceneToJson } from "@zumenjs/core";
import { describe, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { dxf } from "../src/index.js";
import { DRAWINGS } from "./fixtures.js";

// Rust の読み取り（crates/zumen-dxf）も同じ図面を読み、同じ Scene の JSON になることを確かめる。
// 既定の表示は <名前>.scene.json、ほかの表示（レイアウト）は <名前>@<表示>.scene.json
describe("DXF の正解ファイル", () => {
  it.each(DRAWINGS)("%s と Scene", (name, bytes, views) => {
    matchGolden(`dxf/${name}.dxf`, bytes);
    matchGolden(`dxf/${name}.scene.json`, `${sceneToJson(readDrawing(bytes, [dxf]), 2)}\n`);
    for (const view of views) {
      matchGolden(`dxf/${name}@${view}.scene.json`, `${sceneToJson(readDrawing(bytes, [dxf], { view }), 2)}\n`);
    }
  });
});
