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
// PDF も TS では読まないので、ページごとに pdf.js の getTextContent の文字と位置を比べ、違っても失敗にしない（決定 0020。
// pdf.js は近くの字形の並びを 1 つの文字列にまとめるので、文字列のベースラインの上にある Rust の文字をつないで比べる）。
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { performance } from "node:perf_hooks";
import { fileURLToPath } from "node:url";
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
let pdfDiffering = 0;
let compared = 0;
for (const file of files) {
  let bytes = readFileSync(file);
  let rust;
  let twin;
  try {
    const format = execFileSync(binary, ["detect", file], { stdio: "pipe" }).toString().trim();
    if (format === "pdf") {
      compared++;
      if (!(await comparePdf(file, bytes))) pdfDiffering++;
      continue;
    }
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
if (pdfDiffering > 0) console.log(`${pdfDiffering} PDF files have texts that differ from pdf.js (not a failure)`);
if (compared === 0) console.error("No drawings were compared.");
process.exit(failed > 0 || compared === 0 ? 1 : 0);

/**
 * PDF のページごとに、pdf.js の getTextContent の文字列と、Rust の読み取りの文字を比べる。文字列の始めのベースラインから
 * 幅の分の上にある Rust の文字（位置は字形の並びのベースラインの始め）を、ベースラインに沿った順につなぎ、空白を除いて
 * 同じかを見る。位置は用紙の mm（Rust の下敷きの matrix で、pdf.js の表示の座標から写す）。同じなら true
 * @param {string} file
 * @param {Uint8Array} bytes
 */
async function comparePdf(file, bytes) {
  const pdfjs = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const root = new URL(".", import.meta.resolve("pdfjs-dist/package.json"));
  const task = pdfjs.getDocument({
    data: new Uint8Array(bytes),
    cMapUrl: fileURLToPath(new URL("cmaps/", root)),
    cMapPacked: true,
    standardFontDataUrl: fileURLToPath(new URL("standard_fonts/", root)),
    disableFontFace: true,
    isEvalSupported: false,
    verbosity: 0,
  });
  const document = await task.promise;
  let same = true;
  for (let number = 1; number <= document.numPages; number++) {
    const page = await document.getPage(number);
    const viewport = page.getViewport({ scale: 1 });
    const scene = JSON.parse(
      execFileSync(binary, ["scene", file, "--view", String(number)], {
        maxBuffer: 1 << 30,
        stdio: ["ignore", "pipe", "pipe"],
      }),
    );
    const underlay = scene.underlays[0];
    // pdf.js の表示の座標（UserUnit を含むことがある）→ 下敷きの中身の pt → 用紙の mm
    const unit = viewport.width / underlay.width;
    const [a, b, c, d, e, f] = underlay.matrix;
    const paper = (/** @type {number} */ x, /** @type {number} */ y) => {
      const [vx, vy] = viewport.convertToViewportPoint(x, y).map((value) => value / unit);
      return [a * vx + c * vy + e, b * vx + d * vy + f];
    };
    const texts = scene.entities
      .filter((/** @type {{ kind: string }} */ entity) => entity.kind === "text")
      .map((/** @type {{ properties: { key: string, value: string, raw?: unknown }[] }} */ entity) => {
        const property = (/** @type {string} */ key) => entity.properties.find((item) => item.key === key);
        return {
          text: property("text")?.value ?? "",
          position: /** @type {number[]} */ (property("position")?.raw),
          rotation: /** @type {number} */ (property("rotation")?.raw ?? 0),
          height: /** @type {number} */ (property("height")?.raw ?? 0),
          used: false,
        };
      });
    const content = await page.getTextContent();
    let items = 0;
    let matched = 0;
    const differences = [];
    for (const item of content.items) {
      if (!("str" in item) || item.str.trim() === "") continue;
      items++;
      const [ta, tb, tc, td, te, tf] = item.transform;
      const start = paper(te, tf);
      const end = paper(te + (ta / Math.hypot(ta, tb)) * item.width, tf + (tb / Math.hypot(ta, tb)) * item.width);
      const length = Math.hypot(end[0] - start[0], end[1] - start[1]);
      const ux = length > 0 ? (end[0] - start[0]) / length : 1;
      const uy = length > 0 ? (end[1] - start[1]) / length : 0;
      // 文字の大きさ（用紙の mm）の 1/4 まで、ベースラインから離れてよい
      const size = Math.hypot(...paper(te + tc, tf + td).map((value, index) => value - (start[index] ?? 0)));
      const slack = Math.max(0.05, size / 4);
      const along = texts
        .filter((text) => !text.used && text.position)
        .map((text) => {
          const dx = (text.position?.[0] ?? 0) - start[0];
          const dy = (text.position?.[1] ?? 0) - start[1];
          return { text, t: dx * ux + dy * uy, off: Math.abs(-dx * uy + dy * ux) };
        })
        // 向き（5 度まで）と大きさ（半分から 2 倍まで）も同じ文字だけ
        .filter(({ text, t, off }) => {
          const turn = Math.abs(((text.rotation - (Math.atan2(uy, ux) * 180) / Math.PI + 540) % 360) - 180);
          const near = off <= slack && t >= -slack && t <= length + slack;
          return near && turn <= 5 && text.height >= size / 2 && text.height <= size * 2;
        })
        .sort((x, y) => x.t - y.t);
      const rust = along.map(({ text }) => text.text).join("");
      const strip = (/** @type {string} */ text) => text.replace(/\s+/g, "");
      if (strip(rust) === strip(item.str)) {
        matched++;
        for (const { text } of along) text.used = true;
      } else
        differences.push(
          `p${number} ${JSON.stringify(item.str)} at ${start.map((v) => v.toFixed(2))}: Rust ${JSON.stringify(rust)}`,
        );
    }
    const unmatched = texts.filter((text) => !text.used && text.text.trim() !== "").length;
    if (differences.length === 0 && unmatched === 0) {
      console.log(`same  ${file} p${number} (${items} pdf.js texts, ${texts.length} Rust texts)`);
    } else {
      same = false;
      console.log(
        `DIFF  ${file} p${number} (pdf.js ${matched}/${items} texts the same, ${unmatched} Rust texts on no pdf.js text)` +
          `\n  ${differences.slice(0, 10).join("\n  ")}`,
      );
    }
  }
  await task.destroy();
  return same;
}
