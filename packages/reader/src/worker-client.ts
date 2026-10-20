import { type Scene, sceneFromJson } from "@zumenjs/core";
import type { ReadSceneOptions } from "./index.js";
import { type Limit, LimitError } from "./limits.js";
import type { FromWorker, ToWorker } from "./protocol.js";

export interface ReaderWorkerOptions {
  /**
   * Web Worker が読み込む WebAssembly（URL かコンパイル済みの WebAssembly.Module）。省略すると、worker のモジュールから
   * 相対の ../wasm/zumen_wasm_bg.wasm（initReader の既定と同じ）
   */
  wasm?: string | URL | WebAssembly.Module;
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
 * 読み取りは 1 つずつ順に行い、時間は Web Worker が読み始めたときから測る（WebAssembly の読み込みは含めない）
 */
export class ReaderWorker {
  readonly #createWorker: () => Worker;
  readonly #wasm: string | WebAssembly.Module | undefined;
  #worker: Worker | undefined;
  #ready: Promise<Worker> | undefined;
  readonly #queue: Job[] = [];
  #current: Job | undefined;
  #nextId = 0;
  #closed = false;

  constructor(createWorker: () => Worker, options: ReaderWorkerOptions = {}) {
    this.#createWorker = createWorker;
    this.#wasm = options.wasm instanceof URL ? options.wasm.href : options.wasm;
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
      const worker = await this.#start();
      if (this.#current !== job) return;
      const { signal: _signal, ...options } = job.options;
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

  /** Web Worker を作り、WebAssembly を読み込ませる。読み込めなかったら、次の読み取りで作り直す */
  #start(): Promise<Worker> {
    if (this.#ready) return this.#ready;
    const worker = this.#createWorker();
    this.#worker = worker;
    const ready = new Promise<Worker>((resolve, reject) => {
      worker.onmessage = (event: MessageEvent<FromWorker>) => {
        const message = event.data;
        if (message.type === "ready") resolve(worker);
        else if (message.type === "init-error") {
          this.#kill();
          reject(new Error(message.message));
        } else this.#reply(message);
      };
      // モジュールを読み込めない、読んでいる途中で落ちたなど。読んでいる図面を拒否し、次は作り直す
      worker.onerror = (event) => {
        event.preventDefault();
        this.#kill();
        const error = new Error(event.message || "図面を読む Web Worker が止まりました");
        reject(error);
        if (this.#current) this.#finish(this.#current, error);
      };
    });
    this.#ready = ready;
    const init: ToWorker = this.#wasm === undefined ? { type: "init" } : { type: "init", wasm: this.#wasm };
    worker.postMessage(init);
    return ready;
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
    this.#ready = undefined;
  }
}
