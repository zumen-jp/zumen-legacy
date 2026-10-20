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
import { detectFormat, readDrawing, sceneToJson } from "@zumenjs/core";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";
import { cliPath } from "./cli-path.mjs";
import { compareScenes } from "./compare-scenes.mjs";

const inputs = process.argv.slice(2);
if (inputs.length === 0 && process.env.ZUMEN_SAMPLES) inputs.push(process.env.ZUMEN_SAMPLES);
if (inputs.length === 0) {
  console.error("Usage: node scripts/parity.mjs <folder or files…> (or set ZUMEN_SAMPLES)");
  process.exit(2);
}
const binary = process.env.ZUMEN_BINARY || cliPath();
if (!existsSync(binary)) {
  console.error(`Rust CLI not found: ${binary}\nRun cargo build --release -p zumen-cli (or set ZUMEN_BINARY).`);
  process.exit(1);
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
let compared = 0;
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
    rust = JSON.parse(execFileSync(binary, ["scene", file], { maxBuffer: 1 << 30, stdio: ["ignore", "pipe", "pipe"] }));
  } catch (error) {
    // Only an explicit unknown format may be skipped, and never if TypeScript recognises the drawing.
    // Spawn errors, crashes, parse failures and invalid JSON must fail verification.
    if (
      error.status === 1 &&
      error.stderr?.toString().trim() === "unknown format" &&
      !detectFormat(bytes, [sxf, jww, dxf])
    ) {
      console.log(`skip  ${file} (unknown format)`);
    } else {
      console.error(`ERROR ${file}: ${error.stderr?.toString().trim() || error.message}`);
      failed++;
    }
    continue;
  }
  const start = performance.now();
  const scene = readDrawing(bytes, [sxf, jww, dxf]);
  const time = performance.now() - start;
  const ts = JSON.parse(sceneToJson(scene));
  compared++;
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
if (compared === 0) console.error("No drawings were compared.");
process.exit(failed > 0 || compared === 0 ? 1 : 0);
