# @zumenjs/core

The format-neutral part of [zumen](https://github.com/zumenjs/zumen), the TypeScript counterpart of the Rust crate
`zumen-core`: the scene model and its JSON, the `DrawingReader` interface and `readDrawing`, geometry, text layout,
distance measurement in paper and real-world units, and notices. It does not use the DOM, so it runs in Node.js and
Web Workers too. Formats plug in as `DrawingReader`s, and [`@zumenjs/viewer`](../viewer) draws the scenes.

```ts
import { measureDistance, readDrawing } from "@zumenjs/core";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";

const scene = readDrawing(bytes, [sxf, jww]);
```

Drawings with several views (DXF and DWG model space and layouts) list them in `scene.views`; pick one with
`readDrawing(bytes, readers, { view })`. A reader that needs to load first, such as the WebAssembly reader of DWG, has
`load()`; wait for it before reading (`useDrawing` of `@zumenjs/react` does).
Geometry helpers for format authors are exported from `@zumenjs/core/geometry`, and text layout from `@zumenjs/core/text`.
See the [main README](https://github.com/zumenjs/zumen#readme) and the
[architecture notes](https://github.com/zumenjs/zumen/blob/main/docs/architecture.md).
