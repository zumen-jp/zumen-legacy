// WebAssembly の読み取り（bindings/wasm）が、tests/fixtures の正解と同じ Scene を返すかを Node.js で確かめる。
//
//   wasm-pack build bindings/wasm --target web --release --out-dir pkg
//   node scripts/wasm-check.mjs [drawings…]
//
// 図面を渡すと、TS の読み取りとの比較と時間も示す（pnpm run build が必要）。
import { readdirSync, readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";
import { detect, initSync, readSceneJson } from "../bindings/wasm/pkg/zumen_wasm.js";
import { compareScenes } from "./compare-scenes.mjs";

initSync({ module: readFileSync(new URL("../bindings/wasm/pkg/zumen_wasm_bg.wasm", import.meta.url)) });

const fixtures = new URL("../tests/fixtures/", import.meta.url);
let failed = 0;
for (const format of readdirSync(fixtures, { withFileTypes: true }).filter((entry) => entry.isDirectory())) {
  const names = readdirSync(new URL(`${format.name}/`, fixtures));
  for (const name of names) {
    if (name.endsWith(".json") || name.startsWith(".")) continue;
    const bytes = readFileSync(new URL(`${format.name}/${name}`, fixtures));
    const stem = name.replace(/\.[^.]+$/, "");
    // 既定の表示は <名前>.scene.json、ほかの表示は <名前>@<表示>.scene.json
    const views = [
      undefined,
      ...names.filter((golden) => golden.startsWith(`${stem}@`)).map((golden) => golden.slice(stem.length + 1, -11)),
    ];
    for (const view of views) {
      const golden = readFileSync(
        new URL(`${format.name}/${stem}${view ? `@${view}` : ""}.scene.json`, fixtures),
        "utf8",
      );
      const actual = JSON.parse(readSceneJson(bytes, undefined, view));
      const same = detect(bytes) === format.name && compareScenes(JSON.parse(golden), actual).length === 0;
      console.log(`${same ? "same" : "DIFF"}  ${format.name}/${name}${view ? ` @${view}` : ""}`);
      if (!same) failed++;
    }
  }
}

if (process.argv.length > 2) {
  const { readDrawing, sceneToJson } = await import("@zumenjs/core");
  const { jww } = await import("@zumenjs/jww");
  for (const file of process.argv.slice(2)) {
    const bytes = readFileSync(file);
    let start = performance.now();
    const wasm = JSON.parse(readSceneJson(bytes));
    const wasmTime = performance.now() - start;
    start = performance.now();
    const ts = JSON.parse(sceneToJson(readDrawing(bytes, [jww])));
    const tsTime = performance.now() - start;
    const same = compareScenes(ts, wasm).length === 0;
    console.log(`${same ? "same" : "DIFF"}  ${file} (WASM ${wasmTime.toFixed(0)} ms, TS ${tsTime.toFixed(0)} ms)`);
    if (!same) failed++;
  }
}
process.exit(failed > 0 ? 1 : 0);
