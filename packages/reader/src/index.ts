import { type DrawingReader, type ReadDrawingOptions, type Scene, sceneFromJson } from "@zumenjs/core";
import init, { ReadFailure, readSceneJson } from "../wasm/zumen_wasm.js";
import { detectDrawing, FORMATS } from "./detect.js";
import { type Limit, LimitError, type ReadLimits } from "./limits.js";

export { detectDrawing, FORMATS, isDwg } from "./detect.js";
export { type Limit, LimitError, type ReadLimits } from "./limits.js";
export { ReaderWorker, type ReaderWorkerOptions, type WorkerReadOptions } from "./worker-client.js";

/** initReader に渡す WebAssembly の読み取り。URL・Request は取得し、バイト列と WebAssembly.Module はそのまま使う */
export type ReaderWasmSource = string | URL | Request | Response | BufferSource | WebAssembly.Module;

export interface ReadSceneOptions extends ReadDrawingOptions {
  limits?: ReadLimits;
}

let loading: Promise<void> | undefined;
let ready = false;

/**
 * 図面の読み取り（Rust の読み取りを WebAssembly にしたもの）を読み込む。読む前に 1 度だけ待てばよい。
 * 2 度目からは同じ Promise を返し（source は最初のものを使う）、読み込めなかったときは次の呼び出しで読み込み直す。
 *
 * source を省略すると、このモジュールから相対の ../wasm/zumen_wasm_bg.wasm を、ブラウザでは取得し、Node.js では
 * ファイルから読む。Vite と webpack は new URL(…, import.meta.url) の .wasm をビルドの出力に含める。
 * 別の場所に置いたときは、その URL・Response・バイト列・コンパイル済みの WebAssembly.Module を渡す
 */
export function initReader(source?: ReaderWasmSource | Promise<ReaderWasmSource>): Promise<void> {
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

/** initReader が終わり、読める状態か */
export function isReaderReady(): boolean {
  return ready;
}

async function load(source: ReaderWasmSource | Promise<ReaderWasmSource> | undefined): Promise<void> {
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
  if (!fs) throw new Error(`${url.href} を読めません。initReader に WebAssembly のバイト列を渡してください`);
  return fs.readFile(url);
}

/**
 * 図面を読み、表示用の図面にする。format を省略すると中身から判定する。読む前に initReader を待つ。
 * 読めない図面は Rust の読み取りのメッセージの Error、上限を超えたら LimitError を投げる
 */
export function readScene(input: ArrayBuffer | Uint8Array, options: ReadSceneOptions = {}): Scene {
  return sceneFromJson(readJson(input instanceof Uint8Array ? input : new Uint8Array(input), options));
}

/** readScene と同じく読み、Scene の JSON（sceneToJson と同じ形）を返す。Web Worker から JSON のまま渡すときなどに使う */
export function readJson(bytes: Uint8Array, options: ReadSceneOptions = {}): string {
  if (!ready) throw new Error("図面を読む前に、initReader() で読み取り（WebAssembly）を読み込んでください");
  const { format, view, limits } = options;
  try {
    return readSceneJson(bytes, format, view, limits?.maxBytes, limits?.maxEntities);
  } catch (error) {
    if (!(error instanceof ReadFailure)) throw error;
    const { message, limit } = error;
    error.free();
    throw limit ? new LimitError(message, limit as Limit) : new Error(message);
  }
}

/**
 * readDrawing・useDrawing に渡す読み取りを作る。limits は読むたびに Rust の読み取りに渡す上限。
 * detect は WebAssembly を使わないので、読み込む前でも形式を見分けられる。load は initReader() を呼ぶ
 */
export function createReader(options: { limits?: ReadLimits } = {}): DrawingReader {
  const limits = options.limits;
  return {
    formats: FORMATS,
    detect: detectDrawing,
    load: () => initReader(),
    read(bytes, format, readOptions) {
      if (!FORMATS.includes(format)) throw new Error(`図面の読み取りは ${format} を読めません`);
      const options: ReadSceneOptions = { format };
      if (readOptions?.view !== undefined) options.view = readOptions.view;
      if (limits) options.limits = limits;
      return readScene(bytes, options);
    },
  };
}

/**
 * readDrawing・useDrawing に渡す、どの形式（SXF の SFC・P21・P2Z・SFZ、JWW・JWC・JWS、DXF、DWG）も読む読み取り。上限はない。
 * options.view は DXF・DWG のモデル空間（Model）かレイアウトの名前
 */
export const reader: DrawingReader = createReader();
