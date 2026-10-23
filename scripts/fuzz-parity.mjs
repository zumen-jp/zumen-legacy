// TS の読み取り（packages/ のビルド結果）と WebAssembly の読み取り（packages/reader/wasm）を、
// tests/fixtures の図面を無作為に壊した入力で比べる。壊れた図面でも、同じ Scene か同じエラーになることを確かめる。
//
//   pnpm run build && pnpm run wasm
//   node scripts/fuzz-parity.mjs [回数（既定 2000）] [種（既定 1）] [形式のフォルダー名…]
//
// 違いの出た入力は一時フォルダー（os.tmpdir()）に zumen-fuzz-<種>-<番号>.bin として書く。
// ZUMEN_FUZZ_TRACE=1 なら、読む前の入力を zumen-fuzz-<種>-current.bin に書く（読み取りが止まらない・落ちるときに調べる）。
//
// Node.js は Shift_JIS を ICU で復号し、ブラウザと Rust（WHATWG）と違う文字になる並び（0x80、壊れた 2 バイト文字、
// 制御文字 0x1A・0x1C・0x7F）がある。比べるのは読み取りの違いなので、壊した図面からそれらを空白に置き換える。
// DXF の \U+D83D のような対にならないサロゲートは、JavaScript の文字列には残り、Rust の文字列では U+FFFD になる。
// これも読み取りの違いではないので、TS の Scene の文字列を toWellFormed でそろえてから比べる。
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readDrawing, sceneToJson } from "@zumenjs/core";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";
import { initSync, readSceneJson } from "../packages/reader/wasm/zumen_wasm.js";
import { compareScenes } from "./compare-scenes.mjs";

initSync({ module: readFileSync(new URL("../packages/reader/wasm/zumen_wasm_bg.wasm", import.meta.url)) });

const [count = "2000", seed = "1", ...formats] = process.argv.slice(2);
const fixtures = new URL("../tests/fixtures/", import.meta.url);
// DWG は TypeScript では読まないので比べない（crates/zumen-dwg/examples/fuzz.rs が Rust の読み取りだけを壊した図面で試す）
const drawings = readdirSync(fixtures, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name !== "dwg")
  .filter((entry) => formats.length === 0 || formats.includes(entry.name))
  .flatMap((entry) =>
    readdirSync(new URL(`${entry.name}/`, fixtures))
      .filter((name) => !name.endsWith(".json") && !name.startsWith("."))
      .map((name) => ({
        name: `${entry.name}/${name}`,
        bytes: readFileSync(new URL(`${entry.name}/${name}`, fixtures)),
      })),
  );

/** mulberry32 */
function random(state) {
  state.value = (state.value + 0x6d2b79f5) | 0;
  let t = state.value;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}

const TOKENS = [
  "'",
  "''",
  "\\'",
  "\\",
  "\\\\",
  "(",
  ")",
  "()",
  ",",
  "#",
  "#1",
  "=",
  ";",
  "$",
  "*",
  ".",
  ".T.",
  ".F.",
  '"',
  "/*",
  "*/",
  "\r\n",
  " ",
  "\t",
  "　",
  "0",
  "-0",
  "1e999",
  "NaN",
  "Infinity",
  "-1",
  "0.5",
  "99999",
  "1e-7",
  "1.",
  ".5",
  "+3",
  "0x10",
  "\\X2\\",
  "\\X0\\",
  "\\X2\\3042\\X0\\",
  "\\X4\\00110000\\X0\\",
  "\\X4\\0000D83D0000DE00\\X0\\",
  "\\X\\41",
  "\\S\\a",
  "\\PA\\",
  "SXF*/",
  "/*SXF",
  "/*SXF3.1",
  "ENDSEC;",
  "DATA;",
  "_feature",
  "(((",
  ")))",
  "表",
  "ハ",
  "ｶ",
  " ",
  // DXF のタグ・セクション・書式コード
  "\r\n0\r\n",
  "\r\n999\r\n",
  "0\r\nSECTION\r\n2\r\nENTITIES\r\n",
  "0\r\nENDSEC\r\n",
  "0\r\nEOF\r\n",
  "0\r\nBLOCK\r\n2\r\nX\r\n",
  "0\r\nENDBLK\r\n",
  "0\r\nINSERT\r\n2\r\nX\r\n",
  "0\r\nSEQEND\r\n",
  "102\r\n{ACAD_REACTORS\r\n",
  "1001\r\nACAD\r\n",
  "100\r\nAcDbEntity\r\n",
  "$ACADVER",
  "AC1009",
  "AC1021",
  "ANSI_932",
  "*Paper_Space",
  "\\P",
  "\\U+3042",
  "\\U+D83D",
  "\\M+18140",
  "%%c",
  "%%u",
  "%%065",
  "{",
  "}",
  "\\S1/2;",
  "\\S1#2;",
  "\\S1^2;",
  "\\H2x;",
  "\\W0.8;",
  "\\C1;",
  "\\c255;",
  "\\fArial|b1;",
  "\\Fromans.shx;",
  "\\pqc;",
  "\\A1;",
  "\\L",
  "\\~",
  "^J",
  "^I",
  "^",
];
const BYTES = [
  0x5c, 0x27, 0x28, 0x29, 0x2c, 0x23, 0x3d, 0x3b, 0x81, 0x82, 0x83, 0x95, 0xe0, 0xfc, 0xa0, 0xff, 0x00, 0x0a,
];
const encoder = new TextEncoder();
const sjis = { 表: [0x95, 0x5c], ハ: [0x83, 0x6e], ｶ: [0xb6], "　": [0x81, 0x40], " ": [0xa0] };

function token(state) {
  const text = TOKENS[Math.floor(random(state) * TOKENS.length)];
  if (text in sjis) return Uint8Array.from(sjis[text]);
  return encoder.encode(text);
}

function mutate(bytes, state) {
  let out = Uint8Array.from(bytes);
  const steps = 1 + Math.floor(random(state) * 4);
  for (let step = 0; step < steps; step++) {
    const at = Math.floor(random(state) * (out.length + 1));
    const kind = random(state);
    if (kind < 0.25) {
      // 1 バイトを置き換える
      if (out.length > 0) out[Math.min(at, out.length - 1)] = BYTES[Math.floor(random(state) * BYTES.length)];
    } else if (kind < 0.55) {
      const insert = token(state);
      out = Uint8Array.from([...out.subarray(0, at), ...insert, ...out.subarray(at)]);
    } else if (kind < 0.75) {
      const length = Math.floor(random(state) * 40);
      out = Uint8Array.from([...out.subarray(0, at), ...out.subarray(at + length)]);
    } else if (kind < 0.85) {
      const length = Math.floor(random(state) * 200);
      out = Uint8Array.from([
        ...out.subarray(0, at + length),
        ...out.subarray(at, at + length),
        ...out.subarray(at + length),
      ]);
    } else {
      // 数字の並びを別の数にする
      let start = at;
      while (start < out.length && !(out[start] >= 0x30 && out[start] <= 0x39)) start++;
      let end = start;
      while (end < out.length && ((out[end] >= 0x30 && out[end] <= 0x39) || out[end] === 0x2e)) end++;
      const numbers = ["0", "-0", "1", "-1", "2", "3", "9", "17", "1e3", "0.000001", "123456789012", "4.5"];
      const insert = encoder.encode(numbers[Math.floor(random(state) * numbers.length)]);
      out = Uint8Array.from([...out.subarray(0, start), ...insert, ...out.subarray(end)]);
    }
  }
  return sanitize(out);
}

const strict = new TextDecoder("shift_jis", { fatal: true });

/** ICU と WHATWG で同じ文字になるバイトだけを残す（半角カナと、正しく読める 2 バイト文字） */
function sanitize(bytes) {
  const out = Uint8Array.from(bytes);
  for (let i = 0; i < out.length; i++) {
    const byte = out[i];
    if (byte === 0x1a || byte === 0x1c || byte === 0x7f) out[i] = 0x20;
    if (byte < 0x80 || (byte >= 0xa1 && byte <= 0xdf)) continue;
    const lead = (byte >= 0x81 && byte <= 0x9f) || (byte >= 0xe0 && byte <= 0xfc);
    if (lead && i + 1 < out.length && out[i + 1] >= 0x40 && out[i + 1] !== 0x7f) {
      try {
        strict.decode(out.subarray(i, i + 2));
        i++;
        continue;
      } catch {}
    }
    out[i] = 0x20;
  }
  return out;
}

/** 対にならないサロゲートを U+FFFD にする（Rust の文字列と同じ） */
function wellFormed(value) {
  if (typeof value === "string") return value.toWellFormed();
  if (Array.isArray(value)) return value.map(wellFormed);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, wellFormed(item)]));
  }
  return value;
}

function run(read) {
  try {
    return { json: read() };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

const state = { value: Number(seed) };
let failed = 0;
for (let index = 0; index < Number(count); index++) {
  const drawing = drawings[index % drawings.length];
  const bytes = mutate(drawing.bytes, state);
  if (process.env.ZUMEN_FUZZ_TRACE) writeFileSync(join(tmpdir(), `zumen-fuzz-${seed}-current.bin`), bytes);
  const ts = run(() => wellFormed(JSON.parse(sceneToJson(readDrawing(bytes, [sxf, jww, dxf])))));
  const rust = run(() => JSON.parse(readSceneJson(bytes)));
  let differences = [];
  if (ts.error !== undefined || rust.error !== undefined) {
    if (ts.error !== rust.error) differences = [`error: ${ts.error} ≠ ${rust.error}`];
  } else {
    differences = compareScenes(ts.json, rust.json, 5);
  }
  if (differences.length > 0) {
    failed++;
    // 違いの出た入力は一時フォルダーに書く（リポジトリに図面を置かない）
    const path = join(tmpdir(), `zumen-fuzz-${seed}-${index}.bin`);
    writeFileSync(path, bytes);
    console.log(`DIFF  ${drawing.name} #${index} → ${path}\n  ${differences.join("\n  ")}`);
    if (failed >= 10) break;
  }
}
console.log(`${drawings.length} 図面から ${count} 件を確認し、${failed} 件に差分あり`);
process.exit(failed > 0 ? 1 : 0);
