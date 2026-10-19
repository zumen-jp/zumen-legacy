# @zumenjs/core

The format-neutral part of [zumen](https://github.com/zumenjs/zumen): the scene model, the Canvas 2D renderer,
and `DrawingViewer` for zoom, pan, selection, measurement and overlays. Formats plug in as `DrawingReader`s.

```ts
import { DrawingViewer, readDrawing } from "@zumenjs/core";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";

const scene = readDrawing(bytes, [sxf, jww]);
const viewer = new DrawingViewer(element, scene);
```

Drawings with several views (DXF and DWG model space and layouts) list them in `scene.views`; pick one with
`readDrawing(bytes, readers, { view })`.
Geometry helpers for format authors are exported from `@zumenjs/core/geometry`, and text layout from `@zumenjs/core/text`.
See the [main README](https://github.com/zumenjs/zumen#readme) and the
[architecture notes](https://github.com/zumenjs/zumen/blob/main/docs/architecture.md).
