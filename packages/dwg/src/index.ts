import { type DrawingReader, sceneFromJson } from "@zumenjs/core";
import init, { readSceneJson } from "../wasm/zumen_wasm.js";

/**
 * 中身が DWG か。ファイルの先頭の 6 バイトが版の文字列（AC1015 など、AC10 と 2 桁の数字）なら DWG とみなす
 * （Open Design Specification for .dwg files 5.4.1 の 3.2.1）。Rust の zumen_dwg::is_dwg と同じ規則で、
 * WebAssembly を読み込まずに判定できる。読めない版（R13・R14 など）も DWG とみなし、読むときにエラーで伝える
 */
export function isDwg(bytes: Uint8Array): boolean {
  return bytes.length >= 6 && /^AC10[0-9]{2}$/.test(String.fromCharCode(...bytes.subarray(0, 6)));
}

/** initDwg に渡す WebAssembly の読み取り。URL・Request は取得し、バイト列と WebAssembly.Module はそのまま使う */
export type DwgWasmSource = string | URL | Request | Response | BufferSource | WebAssembly.Module;

let loading: Promise<void> | undefined;
let ready = false;

/**
 * DWG の読み取り（Rust の zumen-dwg を WebAssembly にしたもの）を読み込む。dwg で読む前に 1 度だけ待てばよい。
 * 2 度目からは同じ Promise を返し（source は最初のものを使う）、読み込めなかったときは次の呼び出しで読み込み直す。
 *
 * source を省略すると、このモジュールから相対の ../wasm/zumen_wasm_bg.wasm を、ブラウザでは取得し、Node.js では
 * ファイルから読む。Vite と webpack は new URL(…, import.meta.url) の .wasm をビルドの出力に含める。
 * 別の場所に置いたときは、その URL・Response・バイト列・コンパイル済みの WebAssembly.Module を渡す
 */
export function initDwg(source?: DwgWasmSource | Promise<DwgWasmSource>): Promise<void> {
  loading ??= load(source).then(
    () => {
      ready = true;
    },
    (error: unknown) => {
      loading = undefined;
      throw error;
    },
  );
  return loading;
}

async function load(source: DwgWasmSource | Promise<DwgWasmSource> | undefined): Promise<void> {
  const input = (await source) ?? new URL("../wasm/zumen_wasm_bg.wasm", import.meta.url);
  // Node.js の fetch は file: の URL を読めないので、ファイルを読んで渡す
  await init({ module_or_path: input instanceof URL && input.protocol === "file:" ? await readFile(input) : input });
}

/** Node.js の node:fs/promises の一部。import しないので、ブラウザ向けのバンドルに node: のモジュールが入らない */
interface NodeProcess {
  getBuiltinModule?(id: "node:fs/promises"): { readFile(path: URL): Promise<Uint8Array> };
}

async function readFile(url: URL): Promise<Uint8Array> {
  const fs = (globalThis as { process?: NodeProcess }).process?.getBuiltinModule?.("node:fs/promises");
  if (!fs) throw new Error(`${url.href} を読めません。initDwg に WebAssembly のバイト列を渡してください`);
  return fs.readFile(url);
}

/**
 * readDrawing に渡す DWG（AutoCAD 2000〜2018）の読み取り。読む前に initDwg を待つ。detect は WebAssembly を使わない
 * ので、読み込む前でも DWG かを見分けられる。options.view はモデル空間（Model）かレイアウトの名前
 */
export const dwg: DrawingReader = {
  formats: ["dwg"],
  detect: (bytes) => (isDwg(bytes) ? "dwg" : null),
  read(bytes, format, options) {
    if (format !== "dwg") throw new Error(`DWG の読み取りは ${format} を読めません`);
    if (!ready) throw new Error("DWG を読む前に、initDwg() で読み取り（WebAssembly）を読み込んでください");
    // Rust の読み取りは sceneToJson と同じ形の JSON を返す。点列を Float64Array にした Scene に戻す
    return sceneFromJson(readSceneJson(bytes, "dwg", options?.view));
  },
};
