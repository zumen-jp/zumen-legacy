# @zumenjs/dxf

Reads DXF drawings (AutoCAD R12 to 2018, ASCII and binary) for [zumen](https://github.com/zumenjs/zumen).
Written in TypeScript without WebAssembly; works in browsers and Node.js.

```ts
import { readDrawing } from "@zumenjs/core";
import { dxf, dxfToScene, parseDxf } from "@zumenjs/dxf";

const scene = readDrawing(bytes, [dxf]); // model space, or the layout the drawing was saved in
const layout = readDrawing(bytes, [dxf], { view: "Layout1" }); // a layout; scene.views lists them

const doc = parseDxf(bytes); // the AutoCAD drawing model: entities, layers, blocks, layouts
const model = dxfToScene(doc, { layout: "Model" });
```

- Text in `$DWGCODEPAGE` (including Japanese `ANSI_932`), UTF-8 for 2007 and later, and Shift_JIS detected in old files
  without a code page.
- Lines, arcs, polylines with bulges and widths, splines, text and formatted MTEXT, blocks with attributes and arrays,
  dimensions, hatches, leaders and multileaders, multilines, solids and wipeouts, with layers, line types, line weights,
  ACI and true colors and transparency.
- Layout viewports are drawn at their scale, and measurements inside them return real-world units.
- SHX fonts are not bundled: text is drawn with a substitute font and the count is reported in `scene.notices`.

`DxfDocument` uses the attribute names of [ezdxf](https://ezdxf.mozman.at/) and [ezdwg](https://github.com/monozukuri-ai/ezdwg).
Its Rust port (`zumen-dxf`) shares the model and the conversion with the Rust DWG reader (`zumen-dwg`).
How the format is handled is described in
[docs/formats/dxf.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/dxf.md); see also THIRD_PARTY_NOTICES.md.
