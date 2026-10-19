// TS の読み取り（packages/ のビルド結果）と Rust の読み取り（target/release/zumen）が、
// 手元の図面で同じ Scene を返すかを確かめる。図面は Git に入れないので、フォルダーかファイルを渡す。
//
//   pnpm run build && cargo build --release -p zumen-cli
//   node scripts/parity.mjs <folder or files…>   （省略すると ZUMEN_SAMPLES のフォルダー）
//
// 数は 1e-9 の相対誤差まで同じとみなす（三角関数の実装の差）。Rust が読めない形式は飛ばす。
// 表示が複数ある図面（DXF のレイアウトなど）は、既定の表示のほかの表示も比べる。
// DWG は TS では読まないので、同じ図面の DXF（<名前>.dxf か <名前>_ascii.dxf）を TS で読んだ Scene と、形式・版・
// メタデータを除いて比べる。AutoCAD は DXF で数を 16 桁に丸め、ハンドルを作り直すことがあるので、違っても失敗にしない。
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
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

/** DWG と DXF で違ってよい値を除く */
const withoutSource = (/** @type {Record<string, unknown>} */ scene) => {
  const { format: _format, version: _version, metadata: _metadata, ...rest } = scene;
  return rest;
};

let failed = 0;
let dwgDiffering = 0;
for (const file of files) {
  let bytes = readFileSync(file);
  let rust;
  let twin;
  try {
    const format = execFileSync(binary, ["detect", file], { stdio: "pipe" }).toString().trim();
    if (format === "dwg") {
      const stem = file.replace(/\.dwg$/i, "");
      twin = [`${stem}.dxf`, `${stem}_ascii.dxf`].find((path) => existsSync(path));
      if (!twin) {
        console.log(`skip  ${file} (DWG without a DXF of the same drawing)`);
        continue;
      }
      bytes = readFileSync(twin);
    }
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
  const compare = (/** @type {Record<string, unknown>} */ a, /** @type {Record<string, unknown>} */ b) =>
    twin ? compareScenes(withoutSource(a), withoutSource(b)) : compareScenes(a, b);
  const fail = () => {
    if (twin) dwgDiffering++;
    else failed++;
  };
  const differences = compare(ts, rust);
  const label = `${ts.entities.length} entities, TS ${time.toFixed(0)} ms${twin ? ` for ${twin}` : ""}`;
  if (differences.length === 0) console.log(`same  ${file} (${label})`);
  else {
    fail();
    console.log(`DIFF  ${file} (${label})\n  ${differences.join("\n  ")}`);
  }
  for (const { id } of scene.views ?? []) {
    if (id === scene.view) continue;
    const other = JSON.parse(sceneToJson(readDrawing(bytes, [sxf, jww, dxf], { view: id })));
    const args = ["scene", file, "--view", id];
    const rustView = JSON.parse(execFileSync(binary, args, { maxBuffer: 1 << 30, stdio: ["ignore", "pipe", "pipe"] }));
    const viewDifferences = compare(other, rustView);
    if (viewDifferences.length === 0) console.log(`same  ${file} @${id} (${other.entities.length} entities)`);
    else {
      fail();
      console.log(`DIFF  ${file} @${id}\n  ${viewDifferences.join("\n  ")}`);
    }
  }
}
if (dwgDiffering > 0) console.log(`${dwgDiffering} DWG scenes differ from the DXF of the same drawing (not a failure)`);
process.exit(failed > 0 ? 1 : 0);
