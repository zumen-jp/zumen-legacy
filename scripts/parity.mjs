// TS の読み取り（packages/ のビルド結果）と Rust の読み取り（target/release/zumen）が、
// 手元の図面で同じ Scene を返すかを確かめる。図面は Git に入れないので、フォルダーかファイルを渡す。
//
//   pnpm run build && cargo build --release -p zumen-cli
//   node scripts/parity.mjs <folder or files…>   （省略すると ZUMEN_SAMPLES のフォルダー）
//
// 数は 1e-9 の相対誤差まで同じとみなす（三角関数の実装の差）。Rust が読めない形式は飛ばす。
// 表示が複数ある図面（DXF のレイアウトなど）は、既定の表示のほかの表示も比べる。
import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { readDrawing, sceneToJson } from "@zumenjs/core";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";
import { compareScenes } from "./compare-scenes.mjs";

const binary = new URL("../target/release/zumen", import.meta.url).pathname;
const inputs = process.argv.slice(2);
if (inputs.length === 0 && process.env.ZUMEN_SAMPLES) inputs.push(process.env.ZUMEN_SAMPLES);
if (inputs.length === 0) {
  console.error("Usage: node scripts/parity.mjs <folder or files…> (or set ZUMEN_SAMPLES)");
  process.exit(2);
}
const files = inputs.flatMap((input) =>
  statSync(input).isDirectory()
    ? readdirSync(input)
        .filter((name) => !name.startsWith("."))
        .map((name) => join(input, name))
        .filter((path) => statSync(path).isFile())
    : [input],
);

let failed = 0;
for (const file of files) {
  const bytes = readFileSync(file);
  let rust;
  try {
    execFileSync(binary, ["detect", file], { stdio: "pipe" });
    const start = performance.now();
    rust = JSON.parse(execFileSync(binary, ["scene", file], { maxBuffer: 1 << 30, stdio: ["ignore", "pipe", "pipe"] }));
    rust.time = performance.now() - start;
  } catch {
    console.log(`skip  ${file} (Rust does not read this format yet)`);
    continue;
  }
  const start = performance.now();
  const scene = readDrawing(bytes, [sxf, jww, dxf]);
  const time = performance.now() - start;
  const ts = JSON.parse(sceneToJson(scene));
  delete rust.time;
  const differences = compareScenes(ts, rust);
  const label = `${ts.entities.length} entities, TS ${time.toFixed(0)} ms`;
  if (differences.length === 0) console.log(`same  ${file} (${label})`);
  else {
    failed++;
    console.log(`DIFF  ${file} (${label})\n  ${differences.join("\n  ")}`);
  }
  for (const { id } of scene.views ?? []) {
    if (id === scene.view) continue;
    const other = JSON.parse(sceneToJson(readDrawing(bytes, [sxf, jww, dxf], { view: id })));
    const args = ["scene", file, "--view", id];
    const rustView = JSON.parse(execFileSync(binary, args, { maxBuffer: 1 << 30, stdio: ["ignore", "pipe", "pipe"] }));
    const viewDifferences = compareScenes(other, rustView);
    if (viewDifferences.length === 0) console.log(`same  ${file} @${id} (${other.entities.length} entities)`);
    else {
      failed++;
      console.log(`DIFF  ${file} @${id}\n  ${viewDifferences.join("\n  ")}`);
    }
  }
}
process.exit(failed > 0 ? 1 : 0);
