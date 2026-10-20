# Architecture

zumen is a pnpm workspace with one package per drawing format, a format-neutral scene model, a drawing engine that
applications embed in their own interface, and React hooks. A drawing flows through the packages like this:

```text
bytes ─▶ format reader ─▶ native model ─▶ toScene ─▶ Scene ─────────▶ DrawingViewer ───▶ hooks ─▶ your UI
         (@zumenjs/sxf,     (SxfDrawing,             (@zumenjs/core)  (@zumenjs/viewer)  (@zumenjs/react)
          @zumenjs/jww,      JwwDocument,
          @zumenjs/dxf)      DxfDocument)
```

DWG is read by the Rust reader built to WebAssembly, which returns the scene as JSON
(see [DWG in the browser](#dwg-in-the-browser)):

```text
bytes ─▶ dwg (@zumenjs/dwg) ─▶ WebAssembly: zumen-dwg ─▶ DxfDocument ─▶ dxf_to_scene ─▶ JSON ─▶ sceneFromJson ─▶ Scene
```

Structural models (ST-Bridge) are not drawings. They take a separate 3D path inside `@zumenjs/stb`:

```text
bytes ─▶ readStb ─▶ StructureModel ─▶ StructureView (three.js) ─▶ StbViewer (React)
         (@zumenjs/stb)                (@zumenjs/stb/three)        (@zumenjs/stb/react)
```

## Packages and dependency rules

| Package | Responsibility |
| --- | --- |
| `@zumenjs/core` | What `zumen-core` covers: the `Scene` model and its types and JSON (`sceneToJson`, `sceneFromJson`), `NoticeList`, `readDrawing` and the `DrawingReader` interface (with the optional `load`), layer visibility (`isEntityVisible`) and distance measurement (`measureDistance`, `toReal`). Geometry helpers are exported from `@zumenjs/core/geometry` and text layout from `@zumenjs/core/text`. No DOM. |
| `@zumenjs/viewer` | The 2D engine that applications embed: the Canvas 2D renderer (`renderScene`, `renderHighlights`), hit testing and snapping (`HitIndex`), and `DrawingViewer` with the view, the state, input handlers, tools and style. No text for people and no CSS. |
| `@zumenjs/sxf` | SFC and P21 parsing into the SXF feature model (`parseSfc`, `parseP21` → `SxfDrawing`), conversion to a scene (`sxfToScene`), and the `sxf` reader. |
| `@zumenjs/jww` | JWW parsing into the JWW document model (`readJww` → `JwwDocument`), conversion to a scene (`jwwToScene`), and the `jww` reader. |
| `@zumenjs/dxf` | DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parseDxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxfToScene`), and the `dxf` reader. Their Rust port (`zumen-dxf`) also draws DWG. |
| `@zumenjs/dwg` | The `dwg` reader, which reads DWG with the WebAssembly build of the Rust reader (`initDwg` loads it), and `isDwg`. It has no DWG reader of its own. |
| `@zumenjs/react` | Hooks only: `useDrawingViewer` (the engine on an element), `useViewerState` (a part of its state), `useViewerEvent` (its events) and `useDrawing` (read a file, waiting for readers that load). |
| `@zumenjs/stb` | ST-Bridge parsing into a 3D structural model (`isStb`, `readStb` → `StructureModel`) and member filters (`isMemberVisible`, `membersOnAxis`). `@zumenjs/stb/three` draws the model with three.js (`StructureView`, tessellation and picking), and `@zumenjs/stb/react` adds `StbViewer` and `useStbFile`. |

1. Format packages depend only on `@zumenjs/core`, never on each other.
   For example, `@zumenjs/jww` keeps its own copy of the SXF-compatible colors and line types.
   DWG has no TypeScript reader: DXF and DWG are two encodings of the same AutoCAD drawing database, and DWG is read
   only in Rust, by `zumen-dwg` into the `DxfDocument` of `zumen-dxf` (see [decision 0003](decisions/0003-dwg-in-rust.md)
   and [How DWG is handled](formats/dwg.md)). `@zumenjs/dwg` wraps the WebAssembly build and also depends only on
   `@zumenjs/core`.
2. `@zumenjs/core` has no knowledge of any format and does not use the DOM (its build has no DOM types). Formats are
   passed to `readDrawing` as `DrawingReader`s. It covers what the Rust crate `zumen-core` covers: what both
   languages have goes to `@zumenjs/core`, and what only browsers need (drawing, input, hit testing) to
   `@zumenjs/viewer`.
3. Parsing (`parse*`, `read*` and everything below them) does not use the DOM, React or the renderer.
   It uses only the types and `NoticeList` from `@zumenjs/core`. The `*ToScene` conversions use the geometry
   and text layout helpers.
4. Interaction (the view, selection, measurement, input handlers and tools) lives in the engine, `DrawingViewer` of
   `@zumenjs/viewer`, which depends only on `@zumenjs/core`. `@zumenjs/react` depends on both and has hooks only:
   they create the engine, read its state and events, and read files. The packages ship no CSS and no text for
   people; the interface belongs to the application, and the playground has a sample written only against the public
   API ([decision 0005](decisions/0005-browser-packages-for-embedding.md)).
5. `@zumenjs/stb` is a separate line for 3D structural models and depends on no other package. Inside it, parsing
   (`src/`, `src/xml/`, `src/stb/`) does not use the DOM, three.js or React, drawing lives in `src/three/` and React in
   `src/react/`, and the imports go react → three → parsing only. When plan and elevation drawings of a structure are
   added, they convert the model to a `Scene`, and `@zumenjs/stb` then depends on `@zumenjs/core` only
   (see [decision 0001](decisions/0001-st-bridge-in-zumen.md)). Its React component `StbViewer` still keeps the
   interaction state; the 3D step of decision 0005 moves it into the engine of `@zumenjs/stb/three` on the contract
   of the 2D engine, and the component into the playground.

## The scene model

- Coordinates are paper millimeters with the Y axis up. Placements of sub-figures, parts and blocks are resolved.
- Each entity has drawing primitives (`stroke`, `fill`, `clipped`, `dot`, `text`), a bounding box, its layers
  (its own and those of the placements it sits in), a list of properties to show, and, inside a scaled placement,
  a `Placement` with the scale and the transform back to the original (real-world) coordinates.
- `notices` list what was not drawn or was drawn differently, by kind and count. Nothing is dropped silently.
- Colors are `#rrggbb`, or `#rrggbbaa` with opacity. On a light background, near-white colors are drawn black,
  and on a dark background near-black colors are drawn white.
- A drawing can hold several views, such as DXF model space and layouts. `Scene.views` lists them and `Scene.view` is the one drawn;
  `readDrawing(bytes, readers, { view })` passes the choice to the reader (`DrawingReader.read(bytes, format, { view })`).
  Formats with a single view leave both out.

## The structural model

- `StructureModel` (`@zumenjs/stb`) is a 3D model in global millimeters with the Z axis up, separate from `Scene`.
- Each member has a solid (a 2D section outline extruded along a direction, or a list of 3D outlines), a list of
  properties to show and the original XML attributes. Members with the same section share the outline, and
  triangulation happens in `@zumenjs/stb/three`.
- `notices` list what was not drawn, drawn differently or inferred, by kind and count, as in a scene.
- three.js (MIT) is an optional peer dependency used only by `./three` and `./react`, for WebGL rendering with
  `OrbitControls` and `CSS2DRenderer` from its `examples/jsm`. Parsing needs no dependency, so applications that only
  read models do not install it. How the format is read is described in [How ST-Bridge is handled](formats/st-bridge.md)
  (Japanese).

## Adding a format

1. Create a package that depends on `@zumenjs/core`.
2. Parse the bytes into a model that keeps the format's own structure, and report unsupported content with `NoticeList`.
3. Convert the model to a `Scene` with the helpers from `@zumenjs/core/geometry` and `@zumenjs/core/text`.
4. Export a `DrawingReader` with `formats`, `detect` (look at the contents, not the file name) and `read`.
5. Add the smallest synthetic test files that pin the rules that are easy to break without anyone noticing
   (encodings, units, nesting, scales). Do not commit real drawings.

## Extension points for applications

`DrawingViewer` of `@zumenjs/viewer` is the engine an application embeds, and the interface around it (toolbars,
panels, text, layout) is the application's. It splits like map engines such as MapLibre GL JS and OpenLayers: the view,
the state, the input handlers and the tools are separate, and each can be used or replaced on its own
([decision 0005](decisions/0005-browser-packages-for-embedding.md)).

- **View.** `jumpTo({ center, zoom })`, `fitBounds(box, { padding })`, `fit()`, `zoomBy(factor, px, py)`,
  `panBy(dx, dy)` and `getView()`, whose center and zoom go back to `jumpTo` unchanged (saved views, two views kept
  together). `move` is sent on every change and `moveend` when it is over, with the view and the DOM event of the
  user's action. `toScreen(x, y)`, `toPaper(px, py)` and `toReal(placement, x, y)` of `@zumenjs/core` convert between
  screen pixels, paper millimeters and real-world millimeters, for HTML placed on the drawing.
- **State.** `getState()` and `subscribe(listener)`: the scene, the background, line weights, hidden layers, the
  selection (several entities), the hovered entity, named highlights (`setHighlight(name, entities)`), the active
  tool and the measurement. Every change makes a new object, which React reads with `useSyncExternalStore`. What
  changes at the pace of the pointer is sent as events instead: `move` and `cursor` (the paper point under the pointer).
- **Input handlers.** `dragPan`, `wheel` (`"always"`, `"modifier"` for Ctrl or ⌘ only, or `"never"`), `pinch`,
  `doubleClick` and `keyboard`, in the options or with `setHandlers`.
- **Tools.** A `ViewerTool` receives pointer events in paper millimeters and key presses, asks for the entities and
  the snap point under the pointer, changes the state and draws on the overlay canvas. The built-in `select`,
  `measure` and `pan` tools use the same interface; `addTool` and `setTool` add and choose an application's own
  (markup, pins, areas).
- **Style.** CSS custom properties on the element, `--zumen-<role>-dark` and `--zumen-<role>-light` for the
  background, the selection, the hovered entity, named highlights (also per name), the measurement and the sheet
  frame, and `--zumen-fit-padding`, so the drawing follows the application's theme; `setStyle` overrides them. The
  background defaults to the one the drawing assumes (`Scene.background`), and colors that cannot be told from it are
  inverted according to its lightness, with the rules of `renderScene` and `zumen-render`.
- **Queries.** `entitiesAt(x, y)`, `entitiesIn(box)` and `snapAt(x, y)` for applications that handle input
  themselves.
- **Overlays.** `addOverlay(painter)` draws on top of the drawing whatever the tool; `invalidate()` redraws it.
- **Readers that load.** A `DrawingReader` may have `load()`, as the WebAssembly reader of DWG does; `useDrawing` waits
  for the one that recognizes the file.
- **React.** `useDrawingViewer`, `useViewerState(viewer, selector)`, `useViewerEvent(viewer, type, listener)` and
  `useDrawing(file, readers, options)`.
- **Text.** The engine has none: the canvas label (`label`, `setLabel`) and every other text come from the
  application. Scenes still carry Japanese entity types, property labels and notice messages; stable keys next to them
  are a later step of decision 0005.

The playground's 2D sample (`playground/drawing-sample.tsx`, with `layer-list.tsx` and `drawing-sample.css`) is a
complete viewer, with a toolbar, a layer list, properties, notices and a status bar, written only against this API.

## The Rust implementation

The readers are being ported to Rust so that servers, data pipelines and Python can read drawings with the same
results as the browser. The Rust code will become the single implementation of the readers: the browser packages
switch to its WebAssembly build format by format, once the Rust reader returns the same scene as the TypeScript one
on the fixtures and on real drawings. Until then both are maintained, and the TypeScript reader is the reference.
DWG is the first format read only in Rust ([decision 0003](decisions/0003-dwg-in-rust.md)).

```text
bytes ─▶ zumen-sxf, zumen-jww, zumen-dxf, zumen-dwg ─▶ Scene (zumen-core) ─┬▶ JSON ─▶ sceneFromJson ─▶ DrawingViewer
                                                                             ├▶ PNG (zumen-render)
                                                                             └▶ Python (bindings/python), WebAssembly (bindings/wasm)
```

| Crate | Responsibility |
| --- | --- |
| `zumen-core` | The `Scene` types (serde, the same JSON as `sceneToJson`), `NoticeList`, `DrawingReader` and `read_drawing`, geometry (`geometry`), text layout (`text`), measurement, texts and entities for servers (`extract`), and JavaScript-compatible number handling (`js`). |
| `zumen-sxf` | SFC and P21 parsing into the SXF feature model (`parse_sfc`, `parse_p21` → `SxfDrawing`), conversion to a scene (`sxf_to_scene`), and the `SXF` reader. |
| `zumen-jww` | JWW parsing into the JWW document model (`read_jww`), conversion to a scene (`jww_to_scene`), and the `JWW` reader. |
| `zumen-dxf` | DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parse_dxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxf_to_scene`), and the `DXF` reader. |
| `zumen-dwg` | DWG decoding (AutoCAD 2000 to 2018) into the AutoCAD drawing model of `zumen-dxf` (`read_dwg` → `DxfDocument`), drawn with `dxf_to_scene`, and the `DWG` reader. |
| `zumen` | `read` and `detect` over every reader. The entry point for applications. |
| `zumen-render` | Draws a scene to PNG with the rules of `render.ts` in `@zumenjs/viewer` (color inversion, line weights with a 1 px minimum, dash rules, even-odd fills, clipping, per-character text, small text as a line), and reports which entities it drew and where (`render_image`, `drawn_entities`, `View::pixel_box`). |
| `zumen-cli` | The `zumen` command (`detect`, `scene`, `texts`, `entities`, `render`). |
| `bindings/python` | PyO3 bindings, built with maturin as an abi3 wheel for Python 3.10 and later. The GIL is released while reading and rendering. `Scene.to_arrow` needs pyarrow (Apache-2.0, `zumen[arrow]`), and the MCP server `zumen.mcp` needs the MCP SDK (MIT, `zumen[mcp]`); both are optional dependencies. |
| `bindings/wasm` | wasm-bindgen bindings that detect the format (`detect`) and return scene JSON (`readSceneJson`). `scripts/wasm.mjs` builds them into `packages/dwg/wasm` for `@zumenjs/dwg`. |

The dependency rules follow the TypeScript packages: format crates depend only on `zumen-core` and never on each other
(the exception is `zumen-dwg`, which reads into the AutoCAD model of `zumen-dxf` and draws with `dxf_to_scene`), and
parsing does not depend on rendering.

### DWG in the browser

`@zumenjs/dwg` is a `DrawingReader` around the WebAssembly build of `bindings/wasm`, as
[decision 0003](decisions/0003-dwg-in-rust.md) planned. The only format knowledge it holds is the version string that
tells DWG apart. Measurements are from [Verification records](verification.md#dwg-in-the-browser).

- **Loading is asynchronous, reading is synchronous.** `DrawingReader.read` returns a scene, so the module is loaded
  first: `initDwg()` returns one promise shared by every call, a failed load is retried by the next call, and
  `dwg.read` throws until the promise has resolved. Browsers have to fetch the file anyway, and
  `WebAssembly.instantiateStreaming` compiles it while it downloads. `initSync` needs the bytes at hand and compiles
  on the calling thread, so it is not used.
- **DWG is detected without WebAssembly.** `dwg.detect` (`isDwg`) checks the first six bytes (`AC10` and two digits)
  in TypeScript, the rule of `zumen_dwg::is_dwg`; a test compares the two on every fixture. Before the module is
  loaded, `readDrawing` still gives the usual error for files of other formats, and an application can load the module
  only when a DWG is opened. The playground does so, and other files never fetch it.
- **Where the `.wasm` is.** The package ships `wasm/zumen_wasm.js`, its types and `wasm/zumen_wasm_bg.wasm`. Without
  an argument, `initDwg` uses `new URL("../wasm/zumen_wasm_bg.wasm", import.meta.url)`, which bundlers such as Vite and
  webpack 5 follow: they copy the file and rewrite the URL (checked with Vite in development, also for an installed
  package after pre-bundling, and in a build). In Node.js, whose `fetch` does not read `file:` URLs, the file is read
  with `process.getBuiltinModule("node:fs/promises")`, which keeps `node:` imports out of browser bundles. Anywhere
  else, the application serves the file (exported as `@zumenjs/dwg/zumen_wasm_bg.wasm`) and passes a URL, a
  `Response`, the bytes or a `WebAssembly.Module`.
- **The scene comes back as JSON.** `readSceneJson` returns the JSON that the Rust readers already share with
  `sceneToJson` (checked by the golden files), and `sceneFromJson` turns the point lists back into `Float64Array`.
  Building JavaScript objects from Rust instead (`serde-wasm-bindgen`) would add code to the module and cross into
  JavaScript for every value; `sceneFromJson` takes about a quarter of the time of a read (24 of 90 ms for 9,260
  entities).
- **Views pass through.** `options.view` goes to `readSceneJson`, and the scene carries `views` and `view` from
  `dxf_to_scene`, so layouts work as in `@zumenjs/dxf`. The module keeps no document between calls, so each change of
  view decodes the file again; decoding is the smaller part of a read (natively 5.6 ms of about 35 ms for those 9,260
  entities).
- **One module for every format.** The module is the whole `bindings/wasm` build, with the SXF, JWW and DXF readers.
  A build with the DWG reader only would be 24% smaller with brotli (367,123 bytes instead of 482,539, both after
  `wasm-opt`), but when the other browser packages switch to WebAssembly, one module for all formats is downloaded
  once, while a module per format would repeat `zumen-core`, `serde_json` and the encoding tables in each.
- **No `wasm-opt`.** `scripts/wasm.mjs` runs cargo (release, LTO) and `wasm-bindgen` only. `wasm-opt -O` (Binaryen)
  makes the file 12% smaller (1,366,317 bytes instead of 1,560,473) but only 2.3% smaller with brotli (482,539 instead
  of 493,835), and it reads no faster, so the build does not need Binaryen. `wasm-bindgen-cli` must be the version of
  the `wasm-bindgen` crate in `Cargo.lock`; the script checks it and prints the command that installs it.
- **Tests.** The DWG drawings in `tests/fixtures/dwg` are written by the Rust tests (`crates/zumen-dwg/tests`, with
  `UPDATE_GOLDEN=1`), their scenes by `@zumenjs/dwg` through WebAssembly (`pnpm run golden`), and
  `crates/zumen/tests/golden.rs` checks that the native reader returns the same scenes. `pnpm run check` builds the
  module and compares it with every fixture (`pnpm run wasm`).

### Texts, entities and images for servers

Servers and vision models read a drawing from an image and from its contents together. `zumen_core::extract` selects
the entities of a scene by region and layer visibility (the rules of the renderer) and gives the text of an entity:
the text primitives on one line are joined, a line break separates lines (stacked fractions become two lines), and the
position, height, angle and box are in paper millimeters. `zumen_render::render_image` returns the PNG with the
conversion between pixels and paper millimeters, and `drawn_entities` with `View::pixel_box` gives the box of every
entity drawn, in pixels. An application can then show the entity that a model refers to, or render the part of the
sheet around it. These work on the scene, so every format gets them at once. There is no TypeScript counterpart yet.

`zumen.mcp` serves them to AI applications as MCP tools over stdio (`zumen-mcp`): open a drawing, render a region,
find texts and entities, show one entity and measure in real-world millimeters through the scale of a placement. The
tools only read, take local paths limited by `--root`, and bound file sizes, image sizes and result counts
([decision 0004](decisions/0004-mcp-server-in-the-python-package.md)).

### Keeping the two implementations equal

- A port keeps the structure and the order of operations of the TypeScript code, so that the scenes are equal value
  for value: entity ids, properties, notices and their order, and coordinates.
- Strings shown to users follow JavaScript exactly. `zumen_core::js` reproduces `toFixed`, `toPrecision`,
  `String(number)`, `Number(string)`, `parseInt`, `parseFloat`, ToInt32, `Intl.NumberFormat("ja-JP")`,
  `Math.max`/`Math.min` with NaN and signed zeros, V8's `Math.hypot`, and the white space of `String.prototype.trim`.
  Trigonometry uses `libm`, so native and WebAssembly builds give the same values.
- Sorting uses `js::sort_by`, a port of V8's `Array.prototype.sort` (TimSort with the same merges and galloping), because
  some comparisons are inconsistent on purpose or on malformed input (draw order that ignores missing handles, NaN offsets),
  and their result depends on the algorithm. Rust's `sort_by` may also panic on such comparisons.
- Tables keyed by names, which are plain objects in TypeScript (DXF `sort_handles` and `skipped`), keep JavaScript's key
  order: names that are array indexes first, in numeric order, then the others in insertion order.
- Text decoding uses `encoding_rs`, the WHATWG decoders that browsers implement. Node.js decodes Shift_JIS with ICU,
  which differs from WHATWG for byte 0x80 (U+0080 in browsers and Rust, U+FFFD in Node.js), for broken two-byte
  characters (ICU drops an ASCII second byte that WHATWG keeps) and for the control characters 0x1A, 0x1C and 0x7F.
  Every two-byte character that ICU accepts decodes the same in both.
- Text that the TypeScript readers walk by UTF-16 code units (MTEXT format codes, `\U+XXXX` escapes) is processed as
  UTF-16 in Rust too. A lone surrogate (`\U+D83D`) stays in a JavaScript string and becomes U+FFFD in a Rust string.
- Readers do not depend on the user's locale (no `localeCompare` or locale-sensitive formatting other than the fixed
  `ja-JP` number format), and they bound recursion and work on malformed input (nesting depth, entities drawn inside
  blocks, curve subdivision, hatch lines, mesh grids), so that both implementations stop at the same place instead of
  overflowing the stack, hanging or exhausting memory.
- `tests/fixtures/<format>/` holds synthetic drawings built by the TypeScript tests and the scenes the TypeScript readers
  return (`*.scene.json`). `pnpm run golden` rewrites them. The TypeScript tests check that they are current, and
  `crates/zumen/tests/golden.rs` checks that Rust returns the same scenes. Both compare numbers to a relative 1e-9:
  besides Rust's `libm`, Node.js itself gives `Math.sin` and `Math.cos` results that differ in the last bits between x64
  and arm64, so a scene written on one computer does not match another one character for character. The DWG drawings
  there come from the Rust tests instead (see [DWG in the browser](#dwg-in-the-browser)).
- `pnpm run parity` compares the TypeScript and Rust readers on drawings that are not in Git (`ZUMEN_SAMPLES`), in every
  view (DXF layouts), and `pnpm run wasm` builds the WebAssembly package and checks it against the fixtures.
- `pnpm run fuzz` reads randomly damaged copies of the fixtures (inserted escapes, quotes and brackets, DXF tags and
  MTEXT format codes, removed and repeated ranges, changed numbers) with the built TypeScript packages and the WebAssembly
  build, and reports any input where the scenes or the error messages differ. Bytes that ICU and WHATWG decode
  differently are replaced first, and lone surrogates in the TypeScript scene are replaced with U+FFFD before comparing.
  It skips the DWG fixtures, which have no TypeScript reader.
- DWG has no TypeScript reader to compare with. `pnpm run parity` compares the Rust scene of `<name>.dwg` with the
  TypeScript scene of `<name>.dxf` or `<name>_ascii.dxf`, without the format, version and metadata, and reports
  differences without failing, because AutoCAD rounds numbers and gives some entities new handles in DXF.
  `crates/zumen-dwg/examples/compare.rs` compares the two `DxfDocument`s entity by entity and every view, and
  `crates/zumen-dwg/examples/fuzz.rs` reads damaged copies of DWG files natively with overflow checks and reports panics
  and inputs that take too long.

### Rust dependencies

Runtime dependencies are permissive and work in WebAssembly. Recorded with the reason, as for the npm packages.

| Crate | License | Why |
| --- | --- | --- |
| `serde`, `serde_json` | MIT OR Apache-2.0 | The scene JSON shared with the browser |
| `encoding_rs` | (MIT OR Apache-2.0) AND BSD-3-Clause | Shift_JIS (CP932), the DXF and DWG code pages (`$DWGCODEPAGE`) and UTF-16 (DWG 2007 and later) with the WHATWG rules of `TextDecoder` |
| `unicode-normalization` | (MIT OR Apache-2.0) AND Unicode-3.0 | NFKC for font names, as `String.prototype.normalize` (about 128 KB of the WebAssembly build) |
| `libm` | MIT | The same trigonometry on every target |
| `tiny-skia` | BSD-3-Clause | CPU rasterization for PNG, with anti-aliasing, dashes, even-odd fills and masks |
| `ttf-parser` | MIT OR Apache-2.0 | Glyph outlines of the bundled font |
| `pyo3` | MIT OR Apache-2.0 | Python bindings |
| `wasm-bindgen` | MIT OR Apache-2.0 | WebAssembly bindings |

`zumen-render` bundles BIZ UDGothic Regular (SIL Open Font License 1.1, `crates/zumen-render/fonts/OFL.txt`),
a Japanese gothic close to the MS Gothic that SXF and Jw_cad drawings assume. Characters without a glyph are reported
(`RenderReport.missing_glyphs`, a Python warning).

## Development setup

- Each package's `exports` has an `@zumenjs/source` condition that points at `src`. The type checker
  (`customConditions` in `tsconfig.json`), Vitest and the Vite playground resolve it, so changes apply without a build.
  `publishConfig.exports` leaves it out of the published packages, which contain only `dist` (and `wasm` in
  `@zumenjs/dwg`).
- `scripts/wasm.mjs` builds the WebAssembly reader into `packages/dwg/wasm`, which is not in Git, with cargo and
  `wasm-bindgen-cli`. `pnpm dev` and `pnpm run check` run it first; `pnpm test`, the type checker and the builds need
  its output. When the Rust code has not changed, it takes well under a second.
- `pnpm run build` compiles the packages in dependency order (core, then viewer, sxf, jww, dxf and dwg, then react,
  then stb, which depends on none of them) into each `dist`.
- `pnpm test` runs two Vitest projects: the unit tests in Node.js, and the tests of the engine and the hooks
  (`*.browser.test.*`) in Chromium through Playwright, with synthetic pointer, wheel and key events and the pixels of
  the canvas. The Chromium headless shell of the Playwright version in `package.json` is installed with
  `pnpm exec playwright install --only-shell chromium`.
- `pnpm run check` runs Biome, builds the WebAssembly reader and compares it with the fixtures (`pnpm run wasm`), then
  runs the type checker, the unit tests, `pnpm audit` and all builds, then the Rust checks (`check:rust`: rustfmt,
  Clippy, tests and the wasm32 build) and the Python checks (`check:python`: Ruff, maturin, pytest).
- The Rust toolchain is pinned in `rust-toolchain.toml`, Python in `bindings/python/.python-version` (installed by uv).
