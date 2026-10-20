# @zumenjs/dwg

Reads DWG drawings (AutoCAD 2000 to 2018, AC1015 to AC1032) for [zumen](https://github.com/zumenjs/zumen) in browsers
and Node.js. The reader is zumen's own Rust decoder (`zumen-dwg`) built to WebAssembly; there is no TypeScript DWG
reader. A DWG is read into the same AutoCAD drawing model as DXF and drawn by the same conversion: model space and
layouts, viewports at their scale, and measurements in real-world units.

```ts
import { readDrawing } from "@zumenjs/core";
import { dxf } from "@zumenjs/dxf";
import { dwg, initDwg } from "@zumenjs/dwg";

await initDwg(); // loads the WebAssembly reader once
const scene = readDrawing(bytes, [dxf, dwg]); // model space, or the layout the drawing was saved in
const layout = readDrawing(bytes, [dxf, dwg], { view: "Layout1" }); // a layout; scene.views lists them
```

- `initDwg()` loads `wasm/zumen_wasm_bg.wasm` (1.6 MB, 494 KB with brotli) relative to the module: browsers fetch it
  and Node.js reads the file. Bundlers that follow `new URL(…, import.meta.url)`, such as Vite and webpack 5, copy it
  into the build (checked with Vite 8, in development and in a build). Elsewhere, serve
  `@zumenjs/dwg/zumen_wasm_bg.wasm` yourself and pass its URL, a `Response`, its bytes or a compiled
  `WebAssembly.Module` to `initDwg`. Every call returns the same promise, and a failed load is retried by the next call.
- `dwg.detect` and `isDwg` look at the version string at the start of the file without the WebAssembly reader, so an
  application can load it only when a DWG is opened. Reading before `initDwg` has resolved throws an error that says so.
- `dwg.load()` calls `initDwg()` without an argument. `useDrawing` of `@zumenjs/react` waits for the `load` of the
  reader that recognizes the file, so it loads the WebAssembly reader when the first DWG is opened.
- Versions that are not read (R13 and R14) and damaged files throw the reader's message. Content that is not drawn is
  counted in `scene.notices`, as for the other formats.
- The drawing stays in the browser; the only request is for the WebAssembly file. Under a Content Security Policy,
  `script-src` needs `'wasm-unsafe-eval'`.
- Reading is synchronous, like the other readers: a 0.4 MB drawing with 9,260 entities takes about 90 ms in Chromium.
  Large drawings can be read in a Web Worker.

How DWG is read is described in [docs/formats/dwg.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/dwg.md),
why it is read in Rust only in [decision 0003](https://github.com/zumenjs/zumen/blob/main/docs/decisions/0003-dwg-in-rust.md),
and how this package loads the reader in the
[architecture notes](https://github.com/zumenjs/zumen/blob/main/docs/architecture.md#dwg-in-the-browser).
