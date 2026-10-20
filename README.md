# zumen

English | [日本語](README.ja.md)

Read, view and measure architectural and civil engineering 2D drawings in the browser, and view structural models in 3D.
**zumen** (図面, "drawing" in Japanese) parses the drawing files themselves, so nothing is uploaded to a server,
and gives you a framework-agnostic drawing engine to embed in your own interface, with React hooks and a sample UI.

> **Status:** pre-release (0.x). The packages are not published to npm yet.

## Why

- Japanese public works deliver drawings in **SXF** (SFC and P21), and many design offices use Jw_cad's **JWW**.
  In our survey (October 2026) we found no JavaScript library that reads SXF in the browser,
  and no browser package or React component for JWW.
- **DXF** from AutoCAD and other CAD software is read in TypeScript, including Japanese drawings in Shift_JIS and binary DXF.
  **DWG** (AutoCAD 2000 to 2018) is read into the same AutoCAD drawing model by our own decoder in Rust, without GPL
  components, and in the browser through its WebAssembly build (`@zumenjs/dwg`).
- Structural design data in Japan is exchanged as **ST-Bridge** (`.stb`). We found no browser library for it either,
  so `@zumenjs/stb` reads versions 2.0 and 2.1 and shows the members in 3D.

## Features

- Formats: SXF Ver.2.0–3.1 SFC (all 34 features) and P21 (the STEP AP202 subset);
  JWW versions 600 and 700 (lines, arcs, ellipses, points, text, solids, blocks, dimensions);
  DXF R12 to 2018 in ASCII and binary (lines, arcs, polylines with bulges and widths, splines, text and formatted MTEXT,
  blocks with attributes and arrays, dimensions, hatches, leaders, solids, wipeouts), with layers, line types,
  line weights, ACI and true colors, transparency and text styles;
  DWG AutoCAD 2000 to 2018 in Rust, in the browser through WebAssembly, drawn like DXF
  (see [How DWG is handled](docs/formats/dwg.md)).
- DXF and DWG model space and layouts: viewports draw model space at their scale, and measurements inside them return
  real-world units.
- Fit, zoom (wheel, pinch, buttons, keyboard) and pan (drag, arrow keys).
- An engine to embed: set and read the view (and follow it with events, to keep HTML on the drawing in place), select
  several entities, highlight named sets such as search results, add your own tools, turn each input handler off
  (the wheel can zoom only with Ctrl or ⌘), and take colors from CSS custom properties. The engine has no text and the
  packages ship no CSS, so the interface around it is yours; the playground has a sample built on the public API.
- Layer visibility. JWW layer groups can be toggled at once.
- Pick an entity to see its properties: type, layer, color, line type, width, coordinates, dimension values, scale.
- Measure distances with snapping to vertices and lines, in paper millimeters and in **real-world units**
  from the scale of SXF sub-figures, JWW layer groups, DXF units and DXF layout viewports.
- Dark and light backgrounds; true line weights or hairlines.
- Drawing metadata (title block, authoring software, sheet) and a list of everything that was skipped or substituted.
- Structural models: ST-Bridge 2.0 and 2.1 in 3D, with members extruded from their sections, stories and grid axes,
  plans and elevations by filtering, and member properties (section, materials, story and the original attributes).
- No runtime dependencies; the 3D view uses three.js as an optional peer dependency, and `@zumenjs/dwg` ships its
  WebAssembly reader. Parsing also runs in Node.js.

## Packages

| Package | Contents |
| --- | --- |
| [`@zumenjs/core`](packages/core) | Format-neutral scene model, `readDrawing` and the `DrawingReader` interface, geometry, text layout and measurement (no DOM) |
| [`@zumenjs/viewer`](packages/viewer) | The 2D engine to embed: Canvas renderer, hit testing and snapping, and `DrawingViewer` (view, state, input handlers, tools, style) |
| [`@zumenjs/sxf`](packages/sxf) | `parseSfc` and `parseP21` (the SXF feature model), `sxfToScene`, the `sxf` reader |
| [`@zumenjs/jww`](packages/jww) | `readJww` (the JWW document model), `jwwToScene`, the `jww` reader |
| [`@zumenjs/dxf`](packages/dxf) | `parseDxf` (the AutoCAD drawing model, shared with DWG), `dxfToScene`, the `dxf` reader |
| [`@zumenjs/dwg`](packages/dwg) | The `dwg` reader, with the Rust DWG reader built to WebAssembly (`initDwg` or `dwg.load` loads it) |
| [`@zumenjs/react`](packages/react) | Hooks: `useDrawingViewer`, `useViewerState`, `useViewerEvent` and `useDrawing` |
| [`@zumenjs/stb`](packages/stb) | `readStb` and `isStb` (the 3D structural model), `StructureView` (three.js) and `StbViewer` (React) |

Format packages and `@zumenjs/viewer` depend only on `@zumenjs/core`, `@zumenjs/react` on `@zumenjs/viewer` and
`@zumenjs/core`, and `@zumenjs/core` knows nothing about formats and does not use the DOM.
`@zumenjs/stb` is a separate 3D line and depends on no other package.
Formats are detected from the file contents, not from the file name. See [docs/architecture.md](docs/architecture.md).

## Quick start

React: `useDrawing` reads a file with the readers you pass, `useDrawingViewer` creates the engine on an element of
your interface, and `useViewerState` reads the part of its state a component needs.

```tsx
"use client";

import type { Scene } from "@zumenjs/core";
import { dwg } from "@zumenjs/dwg";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { useDrawing, useDrawingViewer, useViewerState } from "@zumenjs/react";
import { sxf } from "@zumenjs/sxf";

const READERS = [sxf, jww, dxf, dwg];

export function DrawingView({ file }: { file: File | null }) {
  // The DWG reader loads its WebAssembly module (494 KB with brotli) when the first DWG is opened.
  // Pass { view } to pick a DXF or DWG layout.
  const { scene, error, loading } = useDrawing(file, READERS);
  if (loading) return <p>Loading…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <Drawing scene={scene} label={file?.name ?? "Drawing"} /> : null;
}

function Drawing({ scene, label }: { scene: Scene; label: string }) {
  const { ref, viewer } = useDrawingViewer(scene, { label, handlers: { wheel: "modifier" } });
  const tool = useViewerState(viewer, (state) => state.tool);
  const distance = useViewerState(viewer, (state) => state.distance);
  return (
    <>
      <button type="button" onClick={() => viewer?.fit()}>Fit</button>
      <button type="button" aria-pressed={tool === "measure"} onClick={() => viewer?.setTool("measure")}>
        Measure
      </button>
      <div ref={ref} style={{ height: 480 }} />
      {distance ? <p>{distance.paper.toFixed(1)} mm on paper, {distance.real?.toFixed(0) ?? "-"} mm real</p> : null}
    </>
  );
}
```

The playground's [2D sample](playground/drawing-sample.tsx) builds a complete viewer this way: toolbar, layer list,
properties, notices and status bar.

Without React: give `DrawingViewer` an element and a scene. Wait for `load` of a reader that has one (DWG) before
reading with it.

```ts
import { readDrawing } from "@zumenjs/core";
import { dwg, initDwg, isDwg } from "@zumenjs/dwg";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";
import { DrawingViewer } from "@zumenjs/viewer";

const bytes = new Uint8Array(await file.arrayBuffer());
if (isDwg(bytes)) await initDwg(); // the first DWG loads the reader; later calls return at once
const scene = readDrawing(bytes, [sxf, jww, dxf, dwg]);
const viewer = new DrawingViewer(element, scene, { label: file.name });

viewer.fitBounds(scene.entities[0].bbox, { padding: 40 }); // go to an entity
const { center, zoom } = viewer.getView(); // viewer.jumpTo({ center, zoom }) restores this view
viewer.on("move", () => {
  // keep an HTML marker on the paper point (120, 80) while the view moves
  const [px, py] = viewer.toScreen(120, 80);
  marker.style.transform = `translate(${px}px, ${py}px)`;
});
viewer.on("cursor", ({ point }) => console.log(point)); // paper millimeters under the pointer
viewer.setHighlight("search", found); // entities your search found, in their own color
viewer.subscribe(() => console.log(viewer.getState().selection));
```

Tools receive pointer events in paper millimeters and draw on top of the drawing; the built-in selection, measurement
and panning are tools too. Colors come from CSS custom properties on the element, for the dark and the light
background, so they follow the theme of your application:

```ts
const pins: [number, number][] = [];
viewer.addTool({
  name: "pin",
  click(event, viewer) {
    pins.push([event.x, event.y]);
    viewer.invalidate();
  },
  draw(ctx, viewer) {
    for (const [x, y] of pins) {
      const [px, py] = viewer.toScreen(x, y);
      ctx.fillRect(px - 4, py - 4, 8, 8);
    }
  },
});
viewer.setTool("pin");
```

```css
.drawing {
  --zumen-background-dark: #1f2023; /* a dark color, so near-black lines are drawn white */
  --zumen-selection-dark: #ffd43d;
  --zumen-highlight-search-dark: #5ad2ff;
  --zumen-fit-padding: 16;
}
```

See [packages/viewer](packages/viewer/README.md) for the view, the state, the input handlers, the tools and the style.

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
`pnpm run parity`. JWW, SXF and DXF are ported. DWG is read in Rust only and checked against the DXF that AutoCAD
writes for the same drawing; `@zumenjs/dwg` brings it to the browser. The other browser packages will switch to the
WebAssembly readers format by format, once each one matches.

```python
import zumen

scene = zumen.read("plan.jww")  # the format is detected from the contents
png = scene.render_png(dpi=150)  # thumbnails, or images for vision models
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))  # a part, in paper millimeters
json_text = scene.to_json()  # sceneFromJson() in @zumenjs/core turns it back into a Scene

texts = scene.texts()  # text, position, height, angle and layers, in paper millimeters
scene.title_block()  # drawing number, revision, title, project, scale and date from the title block
door = scene.entity("#120")  # the type, layers, box, text and properties of one entity
table = scene.to_arrow()  # every entity as a pyarrow.Table (pip install "zumen[arrow]")
image = scene.render(size=(2000, 2000))  # a PNG with the conversion between pixels and paper millimeters
for entity in image.entities_at(640, 480):  # the entities drawn under a pixel, smallest first
    print(entity["id"], entity["box"], entity["text"])
```

AI applications can read drawings through the MCP server of the Python package (`pip install "zumen[mcp]"`, then
`zumen-mcp --root <dir>`): open, render a region, find texts and entities, and measure in real-world units. See
[bindings/python](bindings/python/README.md#mcp-server).

```bash
cargo run --release -p zumen-cli -- render plan.jww -o plan.png --dpi 150 --map plan.json
cargo run --release -p zumen-cli -- texts plan.jww --region 100,50,200,120  # one JSON object per line
cargo run --release -p zumen-cli -- title plan.jww  # the fields of the title block
```

| Crate | Contents |
| --- | --- |
| [`zumen-core`](crates/zumen-core) | The scene model (the same JSON as `@zumenjs/core`), geometry, text layout, the texts and entities of a scene, and the fields of its title block |
| [`zumen-sxf`](crates/zumen-sxf) | `parse_sfc` and `parse_p21` (the SXF feature model), `sxf_to_scene`, the `SXF` reader |
| [`zumen-jww`](crates/zumen-jww) | `read_jww` (the JWW document model), `jww_to_scene`, the `JWW` reader |
| [`zumen-dxf`](crates/zumen-dxf) | `parse_dxf` (the AutoCAD drawing model, shared with DWG), `dxf_to_scene`, the `DXF` reader |
| [`zumen-dwg`](crates/zumen-dwg) | `read_dwg` (DWG 2000 to 2018 into the AutoCAD drawing model of `zumen-dxf`), the `DWG` reader |
| [`zumen`](crates/zumen) | `read` and `detect` over every reader |
| [`zumen-render`](crates/zumen-render) | PNG rendering with the rules of the browser renderer, and the pixel box of every entity drawn; BIZ UDGothic is bundled for Japanese text |
| [`zumen-cli`](crates/zumen-cli) | The `zumen` command: `detect`, `scene` (JSON), `texts` and `entities` (JSON lines), `title` and `render` (PNG) |

Reading and rendering release the GIL in Python.

## Privacy

Drawings are read from memory in the browser. The libraries make no network requests, except that `@zumenjs/dwg`
fetches its own WebAssembly file from where the application serves it, so confidential drawings can be viewed without
sending them anywhere.

## Supported environments

- Browsers with Canvas 2D, and WebAssembly for DWG. Verified in Chromium; Firefox and Safari are expected to work but
  are not tested yet.
- WebGL and three.js 0.186 for the 3D view of `@zumenjs/stb`.
- React 19.3 for the hooks of `@zumenjs/react` and for `@zumenjs/stb/react`.
- Node.js 24 for parsing and for development.

## Roadmap

1. Split into readers, the drawing engine and React (done).
2. DXF, with an AutoCAD object model shared with DWG (done, including MULTILEADER and MLINE).
3. Rust readers with Python and WebAssembly bindings, and PNG rendering on servers (JWW, SXF and DXF done).
4. DWG, read into the same AutoCAD object model (2000 to 2018 done in Rust and in the browser through WebAssembly;
   R13 and R14 are next).
5. Writing and converting drawings, analysis (tables, rooms and areas, revision comparison), markup and PDF drawings.
6. Plan and elevation drawings of ST-Bridge models as scenes, to view, measure and render them like drawings.

## Documentation

- [Architecture and extension points](docs/architecture.md)
- [How SXF is handled](docs/formats/sxf.md) (Japanese)
- [How JWW is handled](docs/formats/jww.md) (Japanese)
- [How DXF is handled](docs/formats/dxf.md)
- [How DWG is handled](docs/formats/dwg.md)
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
