import { type DrawingReader, type Notice, type ReadDrawingOptions, type Scene, sceneFromJson } from "@zumenjs/core";
import init, { embeddedFiles, ReadFailure, readSceneJson } from "../wasm/zumen_wasm.js";
import initPdf, { ReadFailure as PdfReadFailure, readSceneJson as readPdfSceneJson } from "../wasm/zumen_wasm_pdf.js";
import { detectDrawing, FORMATS } from "./detect.js";
import { type Limit, LimitError, type ReadLimits } from "./limits.js";

export { detectDrawing, FORMATS, isDwg, isPdf } from "./detect.js";
export { type Limit, LimitError, type ReadLimits } from "./limits.js";
export { ReaderWorker, type ReaderWorkerOptions, type WorkerReadOptions } from "./worker-client.js";

/** initReader に渡す WebAssembly の読み取り。URL・Request は取得し、バイト列と WebAssembly.Module はそのまま使う */
export type ReaderWasmSource = string | URL | Request | Response | BufferSource | WebAssembly.Module;

export interface ReadSceneOptions extends ReadDrawingOptions {
  limits?: ReadLimits;
}

/**
 * WebAssembly のモジュールの読み込み。PDF の読み取りは大きいので、ほかの形式のモジュールとは別に読み込む（決定 0030）。
 * 読み込めなかったときは、次の呼び出しで読み込み直す
 */
class Module {
  #loading: Promise<void> | undefined;
  ready = false;

  constructor(
    readonly init: (input: { module_or_path: ReaderWasmSource }) => Promise<unknown>,
    readonly file: () => URL,
  ) {}

  load(source: ReaderWasmSource | Promise<ReaderWasmSource> | undefined): Promise<void> {
    this.#loading ??= this.#load(source).then(
      () => {
        this.ready = true;
      },
      (error: unknown) => {
        this.#loading = undefined;
        throw error;
      },
    );
    return this.#loading;
  }

  async #load(source: ReaderWasmSource | Promise<ReaderWasmSource> | undefined): Promise<void> {
    const input = (await source) ?? this.file();
    // Node.js の fetch は file: の URL を読めないので、ファイルを読んで渡す
    await this.init({
      module_or_path: input instanceof URL && input.protocol === "file:" ? await readFile(input) : input,
    });
  }
}

// new URL(…, import.meta.url) はバンドラーがビルドの出力に含める形のまま書く（関数の中で、読み込むときに作る）
const cad = new Module(init, () => new URL("../wasm/zumen_wasm_bg.wasm", import.meta.url));
const pdf = new Module(initPdf, () => new URL("../wasm/zumen_wasm_pdf_bg.wasm", import.meta.url));

/**
 * 図面の読み取り（Rust の読み取りを WebAssembly にしたもの）のうち、PDF 以外の形式のものを読み込む。読む前に 1 度だけ
 * 待てばよい。2 度目からは同じ Promise を返し（source は最初のものを使う）、読み込めなかったときは次の呼び出しで
 * 読み込み直す。PDF は initPdfReader で読み込む。
 *
 * source を省略すると、このモジュールから相対の ../wasm/zumen_wasm_bg.wasm を、ブラウザでは取得し、Node.js では
 * ファイルから読む。Vite と webpack は new URL(…, import.meta.url) の .wasm をビルドの出力に含める。
 * 別の場所に置いたときは、その URL・Response・バイト列・コンパイル済みの WebAssembly.Module を渡す
 */
export function initReader(source?: ReaderWasmSource | Promise<ReaderWasmSource>): Promise<void> {
  return cad.load(source);
}

/** initReader が終わり、PDF 以外の形式を読める状態か */
export function isReaderReady(): boolean {
  return cad.ready;
}

/**
 * PDF の読み取り（zumen-pdf の WebAssembly）を読み込む。initReader と同じく、読む前に 1 度だけ待てばよい。
 * source を省略すると ../wasm/zumen_wasm_pdf_bg.wasm（@zumenjs/reader/zumen_wasm_pdf_bg.wasm）。PDF の図面を初めて
 * 開くときにだけ呼べば、PDF を開かないアプリケーションはこのファイルを取得しない
 */
export function initPdfReader(source?: ReaderWasmSource | Promise<ReaderWasmSource>): Promise<void> {
  return pdf.load(source);
}

/** initPdfReader が終わり、PDF を読める状態か */
export function isPdfReaderReady(): boolean {
  return pdf.ready;
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
 * 図面を読み、表示用の図面にする。format を省略すると中身から判定する。読む前に initReader（PDF は initPdfReader）を
 * 待つ。読めない図面は Rust の読み取りのメッセージの Error、上限を超えたら LimitError を投げる
 */
export function readScene(input: ArrayBuffer | Uint8Array, options: ReadSceneOptions = {}): Scene {
  return sceneFromJson(readJson(input instanceof Uint8Array ? input : new Uint8Array(input), options));
}

/** readScene と同じく読み、Scene の JSON（sceneToJson と同じ形）を返す。Web Worker から JSON のまま渡すときなどに使う */
export function readJson(bytes: Uint8Array, options: ReadSceneOptions = {}): string {
  const { format, view, limits } = options;
  const detected = format ?? detectDrawing(bytes);
  // どの形式でもない図面は、どちらのモジュールにも渡さずに断る（Rust の読み取りと同じ文で、PDF も含めた形式を挙げる）。
  // 大きさの上限を超えた図面は、Rust の読み取りと同じく上限のエラーを先にする
  if (detected === null && !(limits?.maxBytes !== undefined && bytes.length > limits.maxBytes)) {
    const list = FORMATS.map((name) => name.toUpperCase()).join("・");
    throw new Error(`対応していない形式のファイルです（読める形式: ${list}）`);
  }
  const isPdf = detected === "pdf";
  if (isPdf && !pdf.ready) {
    throw new Error("PDF を読む前に、initPdfReader() で PDF の読み取り（WebAssembly）を読み込んでください");
  }
  if (!isPdf && !cad.ready) {
    throw new Error("図面を読む前に、initReader() で読み取り（WebAssembly）を読み込んでください");
  }
  try {
    const read = isPdf ? readPdfSceneJson : readSceneJson;
    return read(bytes, format, view, limits?.maxBytes, limits?.maxEntities);
  } catch (error) {
    if (!(error instanceof ReadFailure || error instanceof PdfReadFailure)) throw error;
    const { message, limit } = error;
    error.free();
    throw limit ? new LimitError(message, limit as Limit) : new Error(message);
  }
}

/** 図面のファイルが中に持つファイル。name は、それを指す画像の underlay の file と同じ */
export interface EmbeddedFile {
  name: string;
  /** 中身（それだけの ArrayBuffer に写したもの。Blob や createImageBitmap にそのまま渡せる） */
  data: Uint8Array<ArrayBuffer>;
}

export interface EmbeddedFiles {
  files: EmbeddedFile[];
  /** 取り出せなかったファイル（壊れたもの、途中で終わる一覧） */
  notices: Notice[];
}

export interface ReadEmbeddedFilesOptions {
  /** 分かっている場合の形式。省略すると中身から判定する */
  format?: string;
  /** maxBytes は図面と、展開した中身の合計の上限 */
  limits?: Pick<ReadLimits, "maxBytes">;
}

/**
 * 図面のファイルが中に持つファイルを、画像の underlay の file と同じ名前で返す。JWW（版 700 以降）は同梱画像
 * （%temp%<名前>、展開したもの）、圧縮した SXF（P2Z・SFZ）は書庫の SXF でないファイル（書庫の中の名前）、ほかの形式は
 * 空。アプリケーションは imageUnderlayRenderer（@zumenjs/viewer）の load で、underlay.file の名前のファイルを返す。
 * 読む前に initReader を待つ。呼んだスレッドで読む
 */
export function readEmbeddedFiles(
  input: ArrayBuffer | Uint8Array,
  options: ReadEmbeddedFilesOptions = {},
): EmbeddedFiles {
  if (!cad.ready) throw new Error("図面を読む前に、initReader() で読み取り（WebAssembly）を読み込んでください");
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  let result: ReturnType<typeof embeddedFiles>;
  try {
    result = embeddedFiles(bytes, options.format ?? detectDrawing(bytes) ?? undefined, options.limits?.maxBytes);
  } catch (error) {
    if (!(error instanceof ReadFailure)) throw error;
    const { message, limit } = error;
    error.free();
    throw limit ? new LimitError(message, limit as Limit) : new Error(message);
  }
  try {
    const files: EmbeddedFile[] = [];
    for (let i = 0; i < result.length; i++) {
      const name = result.name(i);
      const data = result.data(i);
      // wasm-bindgen は中身を新しい ArrayBuffer に写して返す
      if (name !== undefined && data !== undefined) files.push({ name, data: data as Uint8Array<ArrayBuffer> });
    }
    return { files, notices: JSON.parse(result.noticesJson) as Notice[] };
  } finally {
    result.free();
  }
}

/**
 * readDrawing・useDrawing に渡す読み取りを作る。limits は読むたびに Rust の読み取りに渡す上限。
 * detect は WebAssembly を使わないので、読み込む前でも形式を見分けられる。load(format) は、PDF なら initPdfReader()、
 * ほかの形式（と形式を渡さないとき）は initReader() を呼ぶ
 */
export function createReader(options: { limits?: ReadLimits } = {}): DrawingReader {
  const limits = options.limits;
  return {
    formats: FORMATS,
    detect: detectDrawing,
    load: (format) => (format === "pdf" ? initPdfReader() : initReader()),
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
 * readDrawing・useDrawing に渡す、どの形式（SXF の SFC・P21・P2Z・SFZ、JWW・JWC・JWS、DXF、DWG、PDF）も読む読み取り。
 * 上限はない。options.view は DXF・DWG のモデル空間（Model）かレイアウトの名前、PDF のページの番号（"1" から）
 */
export const reader: DrawingReader = createReader();
