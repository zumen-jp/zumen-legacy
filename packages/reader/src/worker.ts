// Web Worker で図面を読む（@zumenjs/reader/worker）。ReaderWorker がこのモジュールの Worker を作り、メッセージを送る。
// 読んだ Scene は点列の Float64Array ごと移し（transfer）、ページで JSON を解析し直さない
import { initReader, LimitError, readScene } from "./index.js";
import type { FromWorker, ToWorker } from "./protocol.js";

interface WorkerScope {
  onmessage: ((event: MessageEvent<ToWorker>) => void) | null;
  postMessage(message: FromWorker, transfer?: Transferable[]): void;
}

const scope = globalThis as unknown as WorkerScope;

scope.onmessage = (event) => {
  const message = event.data;
  if (message.type === "init") {
    initReader(message.wasm).then(
      () => scope.postMessage({ type: "ready" }),
      (error: unknown) => scope.postMessage({ type: "init-error", message: messageOf(error) }),
    );
    return;
  }
  const { id, bytes, options } = message;
  try {
    const scene = readScene(bytes, options);
    scope.postMessage({ type: "scene", id, scene }, buffersOf(scene));
  } catch (error) {
    const reply: FromWorker = { type: "error", id, message: messageOf(error) };
    if (error instanceof LimitError) reply.limit = error.limit;
    scope.postMessage(reply);
  }
};

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Scene の中の Float64Array などのバッファ（同じものは 1 度だけ） */
function buffersOf(scene: object): ArrayBuffer[] {
  const buffers = new Set<ArrayBuffer>();
  const visit = (value: unknown) => {
    if (typeof value !== "object" || value === null) return;
    if (ArrayBuffer.isView(value)) {
      if (value.buffer instanceof ArrayBuffer) buffers.add(value.buffer);
      return;
    }
    for (const child of Array.isArray(value) ? value : Object.values(value)) visit(child);
  };
  visit(scene);
  return [...buffers];
}
