# zumen

English | [日本語](README.ja.md)

Read, view and measure architectural and civil engineering 2D drawings in the browser, and view structural models in 3D.
**zumen** (図面, "drawing" in Japanese) parses the drawing files themselves, so nothing is uploaded to a server,
and gives you a framework-agnostic drawing engine and React components to build viewers on top of.

> **Status:** pre-release (0.x). The packages are not published to npm yet.

## Why

- Japanese public works deliver drawings in **SXF** (SFC and P21), and many design offices use Jw_cad's **JWW**.
  In our survey (October 2026) we found no JavaScript library that reads SXF in the browser,
  and no browser package or React component for JWW.
- **DXF** from AutoCAD and other CAD software is read in TypeScript, including Japanese drawings in Shift_JIS and binary DXF.
  **DWG** comes next, sharing the same AutoCAD drawing model; it aims to stay MIT licensed, without GPL components.
- Structural design data in Japan is exchanged as **ST-Bridge** (`.stb`). We found no browser library for it either,
  so `@zumenjs/stb` reads versions 2.0 and 2.1 and shows the members in 3D.

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
- Structural models: ST-Bridge 2.0 and 2.1 in 3D, with members extruded from their sections, stories and grid axes,
  plans and elevations by filtering, and member properties (section, materials, story and the original attributes).
- No runtime dependencies; the 3D view uses three.js as an optional peer dependency. Parsing also runs in Node.js.

## Packages

| Package | Contents |
| --- | --- |
| [`@zumenjs/core`](packages/core) | Format-neutral scene model, Canvas 2D renderer, `DrawingViewer` (zoom, pan, select, measure, overlays), `readDrawing` |
| [`@zumenjs/sxf`](packages/sxf) | `parseSfc` and `parseP21` (the SXF feature model), `sxfToScene`, the `sxf` reader |
| [`@zumenjs/jww`](packages/jww) | `readJww` (the JWW document model), `jwwToScene`, the `jww` reader |
| [`@zumenjs/dxf`](packages/dxf) | `parseDxf` (the AutoCAD drawing model, shared with DWG), `dxfToScene`, the `dxf` reader |
| [`@zumenjs/react`](packages/react) | `CadViewer` (a complete viewer), `useDrawingViewer` and `useCadFile` hooks |
| [`@zumenjs/stb`](packages/stb) | `readStb` and `isStb` (the 3D structural model), `StructureView` (three.js) and `StbViewer` (React) |

Format packages depend only on `@zumenjs/core`, and `@zumenjs/core` knows nothing about formats.
`@zumenjs/stb` is a separate 3D line and depends on no other package.
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

Structural models (ST-Bridge): `useStbFile` reads the file and `StbViewer` shows it in 3D. `readStb` alone works in Node.js.

```tsx
import { StbViewer, useStbFile } from "@zumenjs/stb/react";

export function StructureView({ file }: { file: File | null }) {
  const { model, error } = useStbFile(file);
  if (error) return <p role="alert">{error.message}</p>;
  return model ? <StbViewer model={model} style={{ height: 640 }} /> : null;
}
```

A `StructureModel` uses global millimeters with the Z axis up. Each member keeps its solid, its properties and the
original XML attributes. See [packages/stb](packages/stb/README.md).

## Rust and Python

The readers are being ported to Rust (`crates/`), with Python bindings (`bindings/python`) and a WebAssembly build
(`bindings/wasm`), so servers and data pipelines can read the same drawings. The Rust readers return the same `Scene`
as the TypeScript ones: both are compared on the drawings in `tests/fixtures`, and on local samples with
`pnpm run parity`. JWW, SXF and DXF are ported; DWG follows. The browser packages will switch to the WebAssembly
readers format by format, once each one matches.

```python
import zumen

scene = zumen.read("plan.jww")  # the format is detected from the contents
png = scene.render_png(dpi=150)  # thumbnails, or images for vision models
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))  # a part, in paper millimeters
json_text = scene.to_json()  # sceneFromJson() in @zumenjs/core turns it back into a Scene
```

```bash
cargo run --release -p zumen-cli -- render plan.jww -o plan.png --dpi 150
```

| Crate | Contents |
| --- | --- |
| [`zumen-core`](crates/zumen-core) | The scene model (the same JSON as `@zumenjs/core`), geometry and text layout |
| [`zumen-sxf`](crates/zumen-sxf) | `parse_sfc` and `parse_p21` (the SXF feature model), `sxf_to_scene`, the `SXF` reader |
| [`zumen-jww`](crates/zumen-jww) | `read_jww` (the JWW document model), `jww_to_scene`, the `JWW` reader |
| [`zumen-dxf`](crates/zumen-dxf) | `parse_dxf` (the AutoCAD drawing model, shared with DWG), `dxf_to_scene`, the `DXF` reader |
| [`zumen`](crates/zumen) | `read` and `detect` over every reader |
| [`zumen-render`](crates/zumen-render) | PNG rendering with the rules of the browser renderer; BIZ UDGothic is bundled for Japanese text |
| [`zumen-cli`](crates/zumen-cli) | The `zumen` command: `detect`, `scene` (JSON) and `render` (PNG) |

Reading and rendering release the GIL in Python.

## Privacy

Drawings are read from memory in the browser. The libraries make no network requests,
so confidential drawings can be viewed without sending them anywhere.

## Supported environments

- Browsers with Canvas 2D. Verified in Chromium; Firefox and Safari are expected to work but are not tested yet.
- WebGL and three.js 0.186 for the 3D view of `@zumenjs/stb`.
- React 19.3 for `@zumenjs/react` and `@zumenjs/stb/react`.
- Node.js 24 for parsing and for development.

## Roadmap

1. Split into readers, the drawing engine and React (done).
2. DXF, with an AutoCAD object model shared with DWG (done, including MULTILEADER and MLINE).
3. Rust readers with Python and WebAssembly bindings, and PNG rendering on servers (JWW, SXF and DXF done).
4. DWG, read into the same AutoCAD object model.
5. Writing and converting drawings, analysis (tables, rooms and areas, revision comparison), markup and PDF drawings.
6. Plan and elevation drawings of ST-Bridge models as scenes, to view, measure and render them like drawings.

## Documentation

- [Architecture and extension points](docs/architecture.md)
- [How SXF is handled](docs/formats/sxf.md) (Japanese)
- [How JWW is handled](docs/formats/jww.md) (Japanese)
- [How DXF is handled](docs/formats/dxf.md)
- [How ST-Bridge is handled](docs/formats/st-bridge.md) (Japanese)
- [Verification records](docs/verification.md)
- [Design decisions](docs/decisions/README.md)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Please never attach confidential drawings to issues or pull requests.
Report security issues as described in [SECURITY.md](SECURITY.md).

## License

To be decided before the first public release.
Third-party notices: [packages/jww/THIRD_PARTY_NOTICES.md](packages/jww/THIRD_PARTY_NOTICES.md),
[packages/dxf/THIRD_PARTY_NOTICES.md](packages/dxf/THIRD_PARTY_NOTICES.md),
[packages/stb/THIRD_PARTY_NOTICES.md](packages/stb/THIRD_PARTY_NOTICES.md).
The bundled font of `zumen-render` is BIZ UDGothic under the SIL Open Font License 1.1
([crates/zumen-render/fonts/OFL.txt](crates/zumen-render/fonts/OFL.txt)).

## Acknowledgments

- The JWW reader follows the file layout documented in [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT).
- The SXF reader follows the SXF Ver.3.1 specifications published by the Ministry of Land, Infrastructure,
  Transport and Tourism of Japan, and the display requirements published by NILIM.
- The DXF reader follows the Autodesk DXF Reference and was checked against [ezdxf](https://ezdxf.mozman.at/) (MIT);
  the code page table and some interpretations follow ezdxf.
- The ST-Bridge reader follows the ST-Bridge XML specifications (ver.2.1.1 and 2.0.2) and XML Schemas published by
  buildingSMART Japan. It is not certified by buildingSMART Japan.
