# @zumenjs/core

> English reference preserved when Japanese became the primary documentation. See [README.md](README.md) for the Japanese guide and current policy.

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
Raster content (images, PDF pages) is described in `scene.underlays`, with its place on the sheet and what it is, and
`UnderlayRequest` is the shape of a request to draw a region of it; [`@zumenjs/viewer`](../viewer) asks a renderer that
the application passes in to draw it. An underlay whose drawing does not record the content size (the PDF underlays of
DXF and DWG) has `width` and `height` 0 until `placeUnderlays(scene, size)` places it with the size the application
learns ([`@zumenjs/pdf`](../pdf)'s `placePdfUnderlays` takes it from pdf.js).
Drawing markup (redlines, revision clouds, notes, review findings and photo positions) is a JSON document beside the
drawing: `createMarkup`, `markupFromJson` (checked, throwing a `MarkupError`), `markupToJson`, `markupShapes` (the
shapes drawn, in paper millimeters), `markupBox` and `drawingHash` (the SHA-256 that names the drawing); see
[Drawing markup](https://github.com/zumenjs/zumen/blob/main/docs/markup.md). [`@zumenjs/viewer`](../viewer) draws and
edits it with `MarkupLayer`.
Geometry helpers for format authors are exported from `@zumenjs/core/geometry`, and text layout from `@zumenjs/core/text`.
See the [main README](https://github.com/zumenjs/zumen#readme) and the
[architecture notes](https://github.com/zumenjs/zumen/blob/main/docs/architecture.md).
