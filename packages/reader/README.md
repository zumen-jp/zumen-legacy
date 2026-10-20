# @zumenjs/reader

Reads every drawing format of [zumen](https://github.com/zumenjs/zumen) in browsers and Node.js: SXF (SFC, P21 and the
compressed P2Z and SFZ), JWW, DXF (ASCII and binary) and DWG (AutoCAD 2000 to 2018). The readers are zumen's Rust
readers built to WebAssembly, the same code that the Rust crates, the command line and the Python package run, so a
drawing reads into the same scene everywhere.

```ts
import { readDrawing } from "@zumenjs/core";
import { initReader, reader } from "@zumenjs/reader";

await initReader(); // loads the WebAssembly module once
const scene = readDrawing(bytes, [reader]); // any format, detected from the contents
const layout = readDrawing(bytes, [reader], { view: "Layout1" }); // a DXF or DWG layout; scene.views lists them
```

## Loading

- `initReader()` loads `wasm/zumen_wasm_bg.wasm` (1.6 MB, 507 KB with brotli) relative to the module: browsers fetch
  it and Node.js reads the file. Bundlers that follow `new URL(…, import.meta.url)`, such as Vite and webpack 5, copy
  it into the build. Elsewhere, serve `@zumenjs/reader/zumen_wasm_bg.wasm` yourself and pass its URL, a `Response`,
  its bytes or a compiled `WebAssembly.Module`. Every call returns the same promise, and a failed load is retried by
  the next call.
- `reader.detect` and `detectDrawing` tell the format from the contents without the WebAssembly module, with the rules
  of the Rust readers, so an application can load the module only when a drawing is opened. `reader.load()` calls
  `initReader()`, and `useDrawing` of `@zumenjs/react` waits for it. Reading before the module has loaded throws an
  error that says so.
- The drawing stays on the device; the only request is for the WebAssembly file. Under a Content Security Policy,
  `script-src` needs `'wasm-unsafe-eval'`.

## Limits for drawings you receive

```ts
import { createReader, LimitError, readScene } from "@zumenjs/reader";

const limited = createReader({ limits: { maxBytes: 50_000_000, maxEntities: 500_000 } });
try {
  const scene = readDrawing(bytes, [limited]);
} catch (error) {
  if (error instanceof LimitError) console.log(error.limit); // "bytes" or "entities"
}
// or without readDrawing
const scene = readScene(bytes, { limits: { maxEntities: 500_000 } });
```

`maxBytes` counts the drawing after decompression for P2Z and SFZ, and `maxEntities` the entities of the scene; the
Rust reader stops as soon as one is exceeded and throws a `LimitError` whose `limit` names it and whose `message` can be
shown to people. Other unreadable files throw an `Error` with the reader's message. WebAssembly cannot look at the
clock while it reads, so a time limit (`timeout`, in milliseconds) needs a Web Worker.

## Reading in a Web Worker

```ts
// reader.worker.ts
import "@zumenjs/reader/worker";
```

```ts
import { ReaderWorker } from "@zumenjs/reader";

const worker = new ReaderWorker(() => new Worker(new URL("./reader.worker.ts", import.meta.url), { type: "module" }));
const scene = await worker.read(bytes, { view: "Layout1", limits: { timeout: 10_000, maxEntities: 500_000 } });
```

- The page keeps responding while the drawing is read, and the scene comes back with its point arrays transferred,
  not copied or parsed again.
- A read that passes `timeout` stops the worker and rejects with `LimitError` (`limit: "time"`); the next read starts
  a new worker. `signal` (an `AbortSignal`) cancels a read the same way, and `terminate()` stops the worker for good.
- Reads run one at a time in the order they were asked for. The time is measured from when the worker starts reading,
  so loading the WebAssembly module does not count.
- The function passed to `ReaderWorker` creates the worker, because a stopped worker is created again. The worker is a
  file of your own that imports `@zumenjs/reader/worker`, so that the bundler builds it as a worker
  (`new Worker(new URL(…, import.meta.url), { type: "module" })`, as Vite and webpack 5 do). Pass `wasm` (a URL or a
  `WebAssembly.Module`) when the WebAssembly file is not where `initReader()` finds it.

## Formats

| Format | Read as | Notes |
| --- | --- | --- |
| SXF SFC, P21 | `sfc`, `p21` | Sheets, sub-figures at their scale, attributes. P2Z and SFZ (a ZIP archive with one `.p21` or `.sfc`) read the drawing inside. |
| JWW | `jww` | Layer groups and their scales, blocks, dimensions. |
| DXF | `dxf` | ASCII and binary; model space and layouts with viewports at their scale. |
| DWG | `dwg` | AutoCAD 2000 to 2018 (AC1015 to AC1032), read into the same model as DXF. R13 and R14 throw an error. |

How the formats are read is described in [docs/formats](https://github.com/zumenjs/zumen/tree/main/docs/formats), why
the browser reads with WebAssembly in [decision 0005](https://github.com/zumenjs/zumen/blob/main/docs/decisions/0005-browser-packages-for-embedding.md),
and how this package loads the module in the
[architecture notes](https://github.com/zumenjs/zumen/blob/main/docs/architecture.md#the-readers-in-the-browser).
