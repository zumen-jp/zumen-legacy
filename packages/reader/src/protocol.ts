import type { ReadSceneOptions } from "./index.js";

// ReaderWorker（ページ）と worker.ts（Web Worker）の間のメッセージ

/** WebAssembly のモジュール。cad は PDF 以外の形式、pdf は PDF（決定 0030） */
export type ReaderModule = "cad" | "pdf";

/** ページから Web Worker へ。load はモジュールごとに、その形式を初めて読む前に 1 度だけ送る */
export type ToWorker =
  | { type: "load"; module: ReaderModule; wasm?: string | WebAssembly.Module }
  | { type: "read"; id: number; bytes: Uint8Array; options: ReadSceneOptions };

/** Web Worker からページへ */
export type FromWorker =
  | { type: "loaded"; module: ReaderModule }
  | { type: "load-error"; module: ReaderModule; message: string }
  /** 読んだ図面の JSON（sceneToJson と同じ形）。ページが sceneFromJson で Scene にする */
  | { type: "scene"; id: number; json: string }
  | { type: "error"; id: number; message: string; limit?: string };
