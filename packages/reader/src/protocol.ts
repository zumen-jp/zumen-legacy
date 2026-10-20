import type { Scene } from "@zumenjs/core";
import type { ReadSceneOptions } from "./index.js";

// ReaderWorker（ページ）と worker.ts（Web Worker）の間のメッセージ

/** ページから Web Worker へ。init は最初に 1 度だけ送る */
export type ToWorker =
  | { type: "init"; wasm?: string | WebAssembly.Module }
  | { type: "read"; id: number; bytes: Uint8Array; options: ReadSceneOptions };

/** Web Worker からページへ */
export type FromWorker =
  | { type: "ready" }
  | { type: "init-error"; message: string }
  | { type: "scene"; id: number; scene: Scene }
  | { type: "error"; id: number; message: string; limit?: string };
