import { readFileSync } from "node:fs";
import { readDrawing, sceneToJson } from "@zumenjs/core";
import { beforeAll, describe, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { initReader, reader } from "../src/index.js";

/** tests/fixtures/dwg の図面と、既定の表示のほかに確かめる表示 */
const DRAWINGS: [string, string[]][] = [
  ["r2000", ["Layout1"]],
  ["r2018", ["Layout1"]],
];

// 図面は crates/zumen-dwg のテストが書く（TypeScript の DWG の読み取りはない）。WebAssembly で読んだ Scene を正解にし、
// Rust の読み取り（crates/zumen/tests/golden.rs）がそのまま読んでも同じ Scene になることを確かめる。
// 既定の表示は <名前>.scene.json、ほかの表示（レイアウト）は <名前>@<表示>.scene.json
describe("DWG の正解ファイル", () => {
  beforeAll(() => initReader());
  it.each(DRAWINGS)("%s と Scene", (name, views) => {
    const bytes = new Uint8Array(readFileSync(new URL(`../../../tests/fixtures/dwg/${name}.dwg`, import.meta.url)));
    matchGolden(`dwg/${name}.scene.json`, `${sceneToJson(readDrawing(bytes, [reader]), 2)}\n`);
    for (const view of views) {
      matchGolden(`dwg/${name}@${view}.scene.json`, `${sceneToJson(readDrawing(bytes, [reader], { view }), 2)}\n`);
    }
  });
});
