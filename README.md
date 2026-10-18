# zumen

English | [日本語](README.ja.md)

Read, view and measure architectural and civil engineering 2D drawings in the browser.
**zumen** (図面, "drawing" in Japanese) parses the drawing files themselves, so nothing is uploaded to a server,
and gives you a framework-agnostic drawing engine and React components to build viewers on top of.

> **Status:** pre-release (0.x). The packages are not published to npm yet.

## Why

- Japanese public works deliver drawings in **SXF** (SFC and P21), and many design offices use Jw_cad's **JWW**.
  In our survey (October 2026) we found no JavaScript library that reads SXF in the browser,
  and no browser package or React component for JWW.
- **DXF** from AutoCAD and other CAD software is read in TypeScript, including Japanese drawings in Shift_JIS and binary DXF.
  **DWG** comes next, sharing the same AutoCAD drawing model; it aims to stay MIT licensed, without GPL components.

## Features

- Formats: SXF Ver.2.0–3.1 SFC (all 34 features) and P21 (the STEP AP202 subset);
  JWW versions 600 and 700 (lines, arcs, ellipses, points, text, solids, blocks, dimensions);
  DXF R12 to 2018 in ASCII and binary (lines, arcs, polylines with bulges and widths, splines, text and formatted MTEXT,
  blocks with attributes and arrays, dimensions, hatches, leaders, solids, wipeouts), with layers, line types,
  line weights, ACI and true colors, transparency and text styles.
- DXF model space and layouts: viewports draw model space at their scale, and measurements inside them return real-world units.
- Fit, zoom (wheel, pinch, buttons, keyboard) and pan (drag, arrow keys).
- Layer visibility. JWW layer groups can be toggled at once.
- Pick an entity to see its properties: type, layer, color, line type, width, coordinates, dimension values, scale.
- Measure distances with snapping to vertices and lines, in paper millimeters and in **real-world units**
  from the scale of SXF sub-figures, JWW layer groups, DXF units and DXF layout viewports.
- Dark and light backgrounds; true line weights or hairlines.
- Drawing metadata (title block, authoring software, sheet) and a list of everything that was skipped or substituted.
- No runtime dependencies. Parsing also runs in Node.js.

## Packages

| Package | Contents |
| --- | --- |
| [`@zumenjs/core`](packages/core) | Format-neutral scene model, Canvas 2D renderer, `DrawingViewer` (zoom, pan, select, measure, overlays), `readDrawing` |
| [`@zumenjs/sxf`](packages/sxf) | `parseSfc` and `parseP21` (the SXF feature model), `sxfToScene`, the `sxf` reader |
| [`@zumenjs/jww`](packages/jww) | `readJww` (the JWW document model), `jwwToScene`, the `jww` reader |
| [`@zumenjs/dxf`](packages/dxf) | `parseDxf` (the AutoCAD drawing model, shared with DWG), `dxfToScene`, the `dxf` reader |
| [`@zumenjs/react`](packages/react) | `CadViewer` (a complete viewer), `useDrawingViewer` and `useCadFile` hooks |

Format packages depend only on `@zumenjs/core`, and `@zumenjs/core` knows nothing about formats.
Formats are detected from the file contents, not from the file name. See [docs/architecture.md](docs/architecture.md).

## Quick start

React: pass the readers you need to `useCadFile` and show the result with `CadViewer`.

```tsx
"use client";

import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { CadViewer, useCadFile } from "@zumenjs/react";
import { sxf } from "@zumenjs/sxf";
import { useState } from "react";

const READERS = [sxf, jww, dxf];

export function DrawingView({ file }: { file: File | null }) {
  // DXF model space or a layout; undefined is the drawing's default view.
  // Give each file its own DrawingView (for example with a key) so that the view starts from the default.
  const [view, setView] = useState<string>();
  const { scene, error, loading } = useCadFile(file, READERS, { view });
  if (loading) return <p>Loading…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <CadViewer scene={scene} onViewChange={setView} style={{ height: 640 }} /> : null;
}
```

Your own UI: `useDrawingViewer` returns a `ref` for a sized element, the viewer state, and the viewer to control.

```tsx
const { ref, viewer, state } = useDrawingViewer(scene, { onSelect: (entity) => console.log(entity) });
return (
  <>
    <button type="button" onClick={() => viewer?.fit()}>Fit</button>
    <div ref={ref} style={{ height: 480 }} />
    <p>{state.selected?.type}</p>
  </>
);
```

Without React: give `DrawingViewer` an element and a scene.

```ts
import { DrawingViewer, readDrawing } from "@zumenjs/core";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";

const scene = readDrawing(new Uint8Array(await file.arrayBuffer()), [sxf, jww]);
const viewer = new DrawingViewer(element, scene, { label: file.name });
viewer.subscribe(() => console.log(viewer.getState().distance));
// Draw on top of the drawing (markup, for example). `view` converts paper millimeters to screen pixels.
const remove = viewer.addOverlay((ctx, view) => ctx.fillRect(view.offsetX, view.offsetY - 8, 8, 8));
```

Parsing only (works in Node.js):

```ts
import { dxfToScene, parseDxf } from "@zumenjs/dxf";
import { parseSfc, sxfToScene } from "@zumenjs/sxf";

const drawing = parseSfc(bytes); // features, layers, sub-figures, title block
const scene = sxfToScene(drawing); // convert to the scene model when you need it

const doc = parseDxf(dxfBytes); // entities with ezdxf attribute names, tables, blocks, layouts
const sheet = dxfToScene(doc, { layout: "Layout1" }); // or the default view (model space or the saved layout)
```

A `Scene` uses paper millimeters with the Y axis up. Placements of sub-figures and blocks are resolved,
and each entity keeps its properties and, inside a scaled placement, the transform back to real-world coordinates.

## Privacy

Drawings are read from memory in the browser. The libraries make no network requests,
so confidential drawings can be viewed without sending them anywhere.

## Supported environments

- Browsers with Canvas 2D. Verified in Chromium; Firefox and Safari are expected to work but are not tested yet.
- React 19.3 for `@zumenjs/react`.
- Node.js 24 for parsing and for development.

## Roadmap

1. Split into readers, the drawing engine and React (done).
2. DXF, with an AutoCAD object model shared with DWG (done; MULTILEADER and MLINE are next).
3. DWG, starting with WebAssembly.
4. Markup, georeferencing, and measuring PDF drawings.

## Documentation

- [Architecture and extension points](docs/architecture.md)
- [How SXF is handled](docs/formats/sxf.md) (Japanese)
- [How JWW is handled](docs/formats/jww.md) (Japanese)
- [How DXF is handled](docs/formats/dxf.md)
- [Verification records](docs/verification.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Please never attach confidential drawings to issues or pull requests.
Report security issues as described in [SECURITY.md](SECURITY.md).

## License

To be decided before the first public release.
Third-party notices: [packages/jww/THIRD_PARTY_NOTICES.md](packages/jww/THIRD_PARTY_NOTICES.md),
[packages/dxf/THIRD_PARTY_NOTICES.md](packages/dxf/THIRD_PARTY_NOTICES.md).

## Acknowledgments

- The JWW reader follows the file layout documented in [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT).
- The SXF reader follows the SXF Ver.3.1 specifications published by the Ministry of Land, Infrastructure,
  Transport and Tourism of Japan, and the display requirements published by NILIM.
- The DXF reader follows the Autodesk DXF Reference and was checked against [ezdxf](https://ezdxf.mozman.at/) (MIT);
  the code page table and some interpretations follow ezdxf.
