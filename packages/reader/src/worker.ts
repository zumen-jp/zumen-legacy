// Web Worker で図面を読む（@zumenjs/reader/worker）。ReaderWorker がこのモジュールの Worker を作り、メッセージを送る。
// 読んだ図面は JSON の文字列のまま送り、ページが sceneFromJson で Scene にする。Scene を送ると、図形ごとの点列の
// Float64Array を構造化複製・転送することになり、図形が多い図面ではそれが読み取りの何倍もかかる
// （SFC の 22 万図形で、文字列は 2.9 秒、Scene の転送は 14.5 秒。docs/architecture.md）
import { initPdfReader, initReader, LimitError, readJson } from "./index.js";
import type { FromWorker, ToWorker } from "./protocol.js";

interface WorkerScope {
  onmessage: ((event: MessageEvent<ToWorker>) => void) | null;
  postMessage(message: FromWorker): void;
}

const scope = globalThis as unknown as WorkerScope;

scope.onmessage = (event) => {
  const message = event.data;
  if (message.type === "load") {
    const { module, wasm } = message;
    (module === "pdf" ? initPdfReader(wasm) : initReader(wasm)).then(
      () => scope.postMessage({ type: "loaded", module }),
      (error: unknown) => scope.postMessage({ type: "load-error", module, message: messageOf(error) }),
    );
    return;
  }
  const { id, bytes, options } = message;
  try {
    scope.postMessage({ type: "scene", id, json: readJson(bytes, options) });
  } catch (error) {
    const reply: FromWorker = { type: "error", id, message: messageOf(error) };
    if (error instanceof LimitError) reply.limit = error.limit;
    scope.postMessage(reply);
  }
};

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
