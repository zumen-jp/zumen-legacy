# @zumenjs/sxf

Reads SXF drawings (SFC and P21, SXF Ver.2.0–3.1), the CAD exchange format of Japanese public works,
for [zumen](https://github.com/zumenjs/zumen). Works in browsers and Node.js.

```ts
import { readDrawing } from "@zumenjs/core";
import { parseSfc, sxf, sxfToScene } from "@zumenjs/sxf";

const scene = readDrawing(bytes, [sxf]); // detects SFC or P21 from the contents
const drawing = parseSfc(bytes); // or keep the SXF feature model
```

How each feature is handled is described in
[docs/formats/sxf.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/sxf.md) (Japanese).
