// WebAssembly の読み取り（bindings/wasm と、PDF だけの bindings/wasm-pdf。決定 0030）を作り、@zumenjs/reader が読み込む
// packages/reader/wasm に置く。
//
//   node scripts/wasm.mjs
//
// cargo で wasm32 の release を作り、wasm-bindgen（Cargo.lock の wasm-bindgen と同じ版の wasm-bindgen-cli）で
// ブラウザと Node.js から読む JavaScript と型を書く。wasm-opt は通さない（brotli では 2% しか小さくならない。
// docs/architecture.md）。Rust を変えていなければ cargo は何もしないので、試用画面を開くたびに走らせてよい。
import { execFileSync } from "node:child_process";
import { rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { version } from "./wasm-bindgen-version.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const out = fileURLToPath(new URL("../packages/reader/wasm/", import.meta.url));

let installed = null;
try {
  installed = execFileSync("wasm-bindgen", ["--version"], { encoding: "utf8" }).trim().split(" ")[1] ?? null;
} catch {
  // 入っていない
}
if (installed !== version) {
  console.error(
    `wasm-bindgen ${version} が必要です（${installed ? `${installed} が入っています` : "見つかりません"}）。` +
      `次のコマンドで入れてください:\n  cargo install wasm-bindgen-cli --locked --version ${version}`,
  );
  process.exit(1);
}

// cargo が書いた .wasm の場所は、ビルドの結果の JSON から取る（CARGO_TARGET_DIR などに左右されない）。2 つのモジュールは
// zumen の機能 pdf を共有しないので（bindings/wasm-pdf は zumen を使わない）、1 度の cargo で作ってよい
const messages = execFileSync(
  "cargo",
  [
    "build",
    "-p",
    "zumen-wasm",
    "-p",
    "zumen-wasm-pdf",
    "--target",
    "wasm32-unknown-unknown",
    "--release",
    "--message-format=json-render-diagnostics",
  ],
  { cwd: root, encoding: "utf8", maxBuffer: 1 << 28, stdio: ["ignore", "pipe", "inherit"] },
);
const artifacts = messages
  .split("\n")
  .filter((line) => line.startsWith("{"))
  .map((line) => JSON.parse(line))
  .filter((message) => message.reason === "compiler-artifact");
const wasmOf = (/** @type {string} */ target) => {
  const wasm = artifacts
    .filter((message) => message.target?.name === target)
    .flatMap((message) => message.filenames)
    .find((name) => name.endsWith(".wasm"));
  if (!wasm) throw new Error(`cargo が ${target}.wasm を書きませんでした`);
  return wasm;
};
const modules = [wasmOf("zumen_wasm"), wasmOf("zumen_wasm_pdf")];

rmSync(out, { recursive: true, force: true });
for (const wasm of modules) {
  execFileSync("wasm-bindgen", ["--target", "web", "--out-dir", out, wasm], { cwd: root, stdio: "inherit" });
}
for (const name of ["zumen_wasm_bg.wasm", "zumen_wasm_pdf_bg.wasm"]) {
  console.log(`packages/reader/wasm/${name}: ${statSync(join(out, name)).size} bytes`);
}
