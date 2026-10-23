import { readFileSync } from "node:fs";
import { readDrawing, sceneToJson } from "@zumenjs/core";
import { beforeAll, describe, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { initPdfReader, initReader, reader } from "../src/index.js";

/** TypeScript の読み取りのない形式の図面（tests/fixtures/<形式>/<名前>）と、既定の表示のほかに確かめる表示 */
const DRAWINGS: [string, string, string[]][] = [
  ["dwg", "r2000", ["Layout1"]],
  ["dwg", "r2018", ["Layout1"]],
  ["pdf", "drawing", ["2"]],
];

// 図面は Rust のテストが書く（DWG は crates/zumen-dwg、PDF は crates/zumen-pdf。TypeScript の読み取りはない）。
// WebAssembly で読んだ Scene を正解にし、Rust の読み取り（crates/zumen/tests/golden.rs）がそのまま読んでも同じ Scene に
// なることを確かめる。既定の表示は <名前>.scene.json、ほかの表示（レイアウト、ページ）は <名前>@<表示>.scene.json
describe("TypeScript の読み取りのない形式の正解ファイル", () => {
  beforeAll(() => Promise.all([initReader(), initPdfReader()]));
  it.each(DRAWINGS)("%s/%s と Scene", (format, name, views) => {
    const file = new URL(`../../../tests/fixtures/${format}/${name}.${format}`, import.meta.url);
    const bytes = new Uint8Array(readFileSync(file));
    matchGolden(`${format}/${name}.scene.json`, `${sceneToJson(readDrawing(bytes, [reader]), 2)}\n`);
    for (const view of views) {
      matchGolden(
        `${format}/${name}@${view}.scene.json`,
        `${sceneToJson(readDrawing(bytes, [reader], { view }), 2)}\n`,
      );
    }
  });
});
