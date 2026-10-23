import { type Scene, sceneFromJson } from "@zumenjs/core";
import { detectDrawing } from "./detect.js";
import type { ReadSceneOptions } from "./index.js";
import { type Limit, LimitError } from "./limits.js";
import type { FromWorker, ReaderModule, ToWorker } from "./protocol.js";

export interface ReaderWorkerOptions {
  /**
   * Web Worker が読み込む WebAssembly（URL かコンパイル済みの WebAssembly.Module）。省略すると、worker のモジュールから
   * 相対の ../wasm/zumen_wasm_bg.wasm（initReader の既定と同じ）
   */
  wasm?: string | URL | WebAssembly.Module;
  /**
   * PDF を読む WebAssembly。省略すると ../wasm/zumen_wasm_pdf_bg.wasm（initPdfReader の既定と同じ）。PDF を初めて
   * 読むときに読み込む（決定 0030）
   */
  pdfWasm?: string | URL | WebAssembly.Module;
}

/** 読み込み中か読み込んだモジュール */
interface Load {
  promise: Promise<void>;
  resolve(): void;
  reject(error: unknown): void;
}

export interface WorkerReadOptions extends ReadSceneOptions {
  /** 読むのをやめる合図。読んでいる途中なら Web Worker を止め、signal.reason で拒否する */
  signal?: AbortSignal;
}

interface Job {
  id: number;
  bytes: Uint8Array;
  options: WorkerReadOptions;
  resolve(scene: Scene): void;
  reject(error: unknown): void;
  timer?: ReturnType<typeof setTimeout>;
  abort?: () => void;
}

/**
 * Web Worker で図面を読む。読んでいる間もページは動き、時間の上限（limits.timeout）を超えた読み取りは Web Worker ごと
 * 止められる（WebAssembly は途中で止まらないので、受け取った図面を読むときはこちらを使う）。
 *
 * createWorker は @zumenjs/reader/worker のモジュールで Worker を作る関数。止めたあとに作り直すので、関数で渡す。
 * 読み取りは 1 つずつ順に行い、時間は Web Worker が読み始めたときから測る（WebAssembly の読み込みは含めない）。
 * WebAssembly は図面の形式のモジュール（PDF とそれ以外）を、その形式を初めて読むときに読み込む
 */
export class ReaderWorker {
  readonly #createWorker: () => Worker;
  readonly #wasm: Record<ReaderModule, string | WebAssembly.Module | undefined>;
  #worker: Worker | undefined;
  #loads = new Map<ReaderModule, Load>();
  readonly #queue: Job[] = [];
  #current: Job | undefined;
  #nextId = 0;
  #closed = false;

  constructor(createWorker: () => Worker, options: ReaderWorkerOptions = {}) {
    this.#createWorker = createWorker;
    const location = (wasm: string | URL | WebAssembly.Module | undefined) => (wasm instanceof URL ? wasm.href : wasm);
    this.#wasm = { cad: location(options.wasm), pdf: location(options.pdfWasm) };
  }

  /** 図面を読み、表示用の図面を返す。読めない図面は Error、上限を超えたら LimitError、やめたら signal.reason で拒否する */
  read(input: ArrayBuffer | Uint8Array, options: WorkerReadOptions = {}): Promise<Scene> {
    if (this.#closed) return Promise.reject(new Error("ReaderWorker は terminate の後は読めません"));
    const { signal } = options;
    if (signal?.aborted) return Promise.reject(signal.reason);
    return new Promise<Scene>((resolve, reject) => {
      const job: Job = {
        id: this.#nextId++,
        bytes: input instanceof Uint8Array ? input : new Uint8Array(input),
        options,
        resolve,
        reject,
      };
      if (signal) {
        job.abort = () => this.#cancel(job, signal.reason);
        signal.addEventListener("abort", job.abort, { once: true });
      }
      this.#queue.push(job);
      this.#pump();
    });
  }

  /** Web Worker を止める。読んでいる途中と待っている読み取りは拒否する */
  terminate(): void {
    this.#closed = true;
    const error = new Error("ReaderWorker を止めました");
    if (this.#current) this.#settle(this.#current, error);
    for (const job of this.#queue.splice(0)) this.#settle(job, error);
    this.#kill();
  }

  #pump(): void {
    if (this.#current || this.#closed) return;
    const job = this.#queue.shift();
    if (!job) return;
    this.#current = job;
    void this.#run(job);
  }

  async #run(job: Job): Promise<void> {
    try {
      const worker = this.#start();
      const { signal: _signal, ...options } = job.options;
      await this.#load(worker, (options.format ?? detectDrawing(job.bytes)) === "pdf" ? "pdf" : "cad");
      if (this.#current !== job) return;
      const message: ToWorker = { type: "read", id: job.id, bytes: job.bytes, options };
      worker.postMessage(message);
      const timeout = options.limits?.timeout;
      if (timeout !== undefined) {
        job.timer = setTimeout(() => {
          this.#kill();
          const seconds = String(timeout / 1000);
          this.#finish(job, new LimitError(`読み取りが上限の ${seconds} 秒を超えたため、途中でやめました`, "time"));
        }, timeout);
      }
    } catch (error) {
      // Worker の生成・postMessage も同期的に失敗する。必ず今の読み取りを終え、待っている次を進める。
      if (this.#current === job) {
        this.#kill();
        this.#finish(job, error);
      }
    }
  }

  /** Web Worker を作る。WebAssembly は #load で読み込ませる */
  #start(): Worker {
    if (this.#worker) return this.#worker;
    const worker = this.#createWorker();
    this.#worker = worker;
    worker.onmessage = (event: MessageEvent<FromWorker>) => {
      const message = event.data;
      if (message.type === "loaded") this.#loads.get(message.module)?.resolve();
      else if (message.type === "load-error") {
        // 読み込めなかったら Web Worker ごと捨て、次の読み取りで作り直す
        this.#loads.get(message.module)?.reject(new Error(message.message));
        this.#kill();
      } else this.#reply(message);
    };
    // モジュールを読み込めない、読んでいる途中で落ちたなど。読んでいる図面を拒否し、次は作り直す
    worker.onerror = (event) => {
      event.preventDefault();
      const error = new Error(event.message || "図面を読む Web Worker が止まりました");
      for (const load of this.#loads.values()) load.reject(error);
      this.#kill();
      if (this.#current) this.#finish(this.#current, error);
    };
    return worker;
  }

  /** Web Worker にモジュールを読み込ませる。読み込み中・読み込んだものは同じ Promise を返す */
  #load(worker: Worker, module: ReaderModule): Promise<void> {
    const existing = this.#loads.get(module);
    if (existing) return existing.promise;
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<void>((onResolve, onReject) => {
      resolve = onResolve;
      reject = onReject;
    });
    this.#loads.set(module, { promise, resolve, reject });
    const wasm = this.#wasm[module];
    const message: ToWorker = wasm === undefined ? { type: "load", module } : { type: "load", module, wasm };
    worker.postMessage(message);
    return promise;
  }

  #reply(message: Extract<FromWorker, { id: number }>): void {
    const job = this.#current;
    if (!job || job.id !== message.id) return;
    if (message.type === "scene") {
      let scene: Scene;
      try {
        scene = sceneFromJson(message.json);
      } catch (error) {
        this.#finish(job, error);
        return;
      }
      this.#finish(job, undefined, scene);
    } else {
      const error = message.limit
        ? new LimitError(message.message, message.limit as Limit)
        : new Error(message.message);
      this.#finish(job, error);
    }
  }

  #cancel(job: Job, reason: unknown): void {
    const index = this.#queue.indexOf(job);
    if (index >= 0) {
      this.#queue.splice(index, 1);
      this.#settle(job, reason);
    } else if (this.#current === job) {
      this.#kill();
      this.#finish(job, reason);
    }
  }

  /** 読んでいた図面を終え、次を読む */
  #finish(job: Job, error: unknown, scene?: Scene): void {
    this.#current = undefined;
    this.#settle(job, error, scene);
    this.#pump();
  }

  #settle(job: Job, error: unknown, scene?: Scene): void {
    clearTimeout(job.timer);
    if (job.abort) job.options.signal?.removeEventListener("abort", job.abort);
    if (scene) job.resolve(scene);
    else job.reject(error);
  }

  #kill(): void {
    if (this.#worker) {
      this.#worker.onmessage = null;
      this.#worker.onerror = null;
      this.#worker.terminate();
    }
    this.#worker = undefined;
    // 読み込みを待っている読み取りを終える（その読み取りがまだ今のものなら、呼び出した側が別の理由で終えている）
    const stopped = new Error("ReaderWorker の Web Worker を止めました");
    for (const load of this.#loads.values()) load.reject(stopped);
    this.#loads = new Map();
  }
}
