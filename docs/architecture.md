# Architecture

zumen is a pnpm workspace with one package per drawing format, a format-neutral scene model, a drawing engine that
applications embed in their own interface, and React hooks. A drawing flows through the packages like this:

```text
bytes ─▶ reader ──────────▶ WebAssembly: Rust readers ─▶ JSON ─▶ sceneFromJson ─▶ Scene ─────────▶ DrawingViewer ───▶ hooks ─▶ your UI
         (@zumenjs/reader)   (bindings/wasm)                                     (@zumenjs/core)  (@zumenjs/viewer)  (@zumenjs/react)
```

Browsers read every format with the Rust readers built to WebAssembly, on the page or in a Web Worker
(see [The readers in the browser](#the-readers-in-the-browser)). The TypeScript readers, which parse into the formats'
own models and convert them to a scene, stay in the workspace as the reference that writes the expected scenes of the
tests:

```text
bytes ─▶ format reader ─▶ native model ─▶ toScene ─▶ Scene
         (@zumenjs/sxf,     (SxfDrawing,
          @zumenjs/jww,      JwwDocument,
          @zumenjs/dxf)      DxfDocument)
```

## Packages and dependency rules

| Package | Responsibility |
| --- | --- |
| `@zumenjs/core` | What `zumen-core` covers: the `Scene` model and its types and JSON (`sceneToJson`, `sceneFromJson`), `NoticeList`, `readDrawing` and the `DrawingReader` interface (with the optional `load`), layer visibility (`isEntityVisible`) and distance measurement (`measureDistance`, `toReal`). Geometry helpers are exported from `@zumenjs/core/geometry` and text layout from `@zumenjs/core/text`. No DOM. |
| `@zumenjs/viewer` | The 2D engine that applications embed: the Canvas 2D renderer (`renderScene`, `renderHighlights`), hit testing and snapping (`HitIndex`), and `DrawingViewer` with the view, the state, input handlers, tools and style. No text for people and no CSS. |
| `@zumenjs/reader` | Every format through the WebAssembly build of the Rust readers: `initReader` loads the module, `reader` and `createReader({ limits })` are `DrawingReader`s, `readScene` reads with limits (`LimitError`), `detectDrawing` tells the format without the module, and `ReaderWorker` reads in a Web Worker (`@zumenjs/reader/worker`). |
| `@zumenjs/sxf` | Not published; the reference for the tests. SFC and P21 parsing into the SXF feature model (`parseSfc`, `parseP21` → `SxfDrawing`), conversion to a scene (`sxfToScene`), and the `sxf` reader. |
| `@zumenjs/jww` | Not published; the reference for the tests. JWW parsing into the JWW document model (`readJww` → `JwwDocument`), conversion to a scene (`jwwToScene`), and the `jww` reader. |
| `@zumenjs/dxf` | Not published; the reference for the tests. DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parseDxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxfToScene`), and the `dxf` reader. Their Rust port (`zumen-dxf`) also draws DWG. |
| `@zumenjs/react` | Hooks only: `useDrawingViewer` (the engine on an element), `useViewerState` (a part of its state), `useViewerEvent` (its events) and `useDrawing` (read a file, waiting for readers that load). |

1. Format packages depend only on `@zumenjs/core`, never on each other.
   For example, `@zumenjs/jww` keeps its own copy of the SXF-compatible colors and line types.
   DWG has no TypeScript reader: DXF and DWG are two encodings of the same AutoCAD drawing database, and DWG is read
   only in Rust, by `zumen-dwg` into the `DxfDocument` of `zumen-dxf` (see [decision 0003](decisions/0003-dwg-in-rust.md)
   and [How DWG is handled](formats/dwg.md)). `@zumenjs/reader` wraps the WebAssembly build of every reader and also
   depends only on `@zumenjs/core`.
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
- Entities and properties carry text for people next to stable keys for applications, as described below.
- `Scene.unit` is the length unit of the drawing's own coordinates, those of the `raw` points and lengths of the
  properties: `millimeter` in SXF and JWW, and in DXF and DWG the unit of `$INSUNITS`, or the one assumed with the
  `unit-assumed` notice when the file gives none. It is one of `LENGTH_UNITS` (`LengthUnit` in Rust), every unit that
  `$INSUNITS` can name (`millimeter`, `meter`, `inch`, `foot`, `us-survey-foot` …). In a DXF or DWG layout, the
  entities drawn on the sheet itself (paper space, without a placement) have coordinates in the paper units of the
  layout. Placements turn paper coordinates into real-world millimeters whatever the unit.
- Entity ids come from the file and are kept for the same bytes, as described in [Entity ids](#entity-ids).

### Entity ids

`SceneEntity.id` names an entity by what the file gives it, so that an application can keep an id (in a citation, a
link or a database) and find the entity again by reading the same file:

- **SXF**: the instance number of the feature (`#120`). Inside a sub-figure or part, the instance numbers of the
  placements it sits in come first, each followed by `>` (`#810>#260>#220`).
- **JWW**: the position of the record in the file's list of entities, from 1 (`#5`). Inside a block, the position of
  the block reference comes first (`#12>#3`, the third entity of the block).
- **DXF and DWG**: the handle (`1F`, in upper case). Inside a block reference, the handles of the references come first,
  each followed by `>` (`2A>1F`), with the row and column of a multiple insert (`2A[0,1]>1F`); the attributes of a
  reference follow its handle in the same way. Inside a viewport of a layout, the viewport's handle and a colon come
  first (`4C:2A>1F`). The parts of a dimension or a multileader are merged into it and have its id. An entity without a
  handle (DXF files written without handles and DWG before R10) is `#n`, where n counts the records of the file
  (group code 0: sections, table entries, blocks, entities, vertices) up to the entity's own, from 1. Every record is
  counted, also those of entity types that zumen does not read, so the number does not change when a later version
  reads more types, and it is unique in the file like a handle.

The same bytes give the same ids in every version of zumen and in every language (TypeScript, Rust, Python,
WebAssembly), for each view. `crates/zumen/tests/ids.rs` checks the ids of every fixture and view against a record,
`crates/zumen/tests/ids.json`, that `pnpm run golden` does not rewrite; a change of the readers that gives the same
bytes other ids is a breaking change and needs a decision. Ids are unique within a scene when the file's handles or
instance numbers are (`packages/core/tests/keys.test.ts` checks every golden file), and `Scene.entity(id)` in Python
returns the first entity with an id. An id does not survive saving the drawing again in another program, which may
renumber handles and records; `zumen_core::compare` matches revisions by shape and text for that reason.

### Stable keys

Applications pick entities and attributes by keys that are the same in every format and do not change with the
wording, and show the Japanese text to people ([decision 0005](decisions/0005-browser-packages-for-embedding.md)).
`SceneEntity.kind` is one of `ENTITY_KINDS` (`EntityKind` in Rust) and `SceneEntity.type` the Japanese name;
`SceneEntity.source` keeps the name in the format. Each `Property` (of an entity and of `Scene.metadata`) has a `key`,
the Japanese `label`, the formatted `value` and, where the value was formatted from numbers or a code, the unformatted
`raw`: a number, a string, or numbers such as `[x, y]`. Keys are camelCase and distinct within an entity, and a key
has the same shape of `raw` in every format (`packages/core/tests/keys.test.ts` checks both on every golden file).
`zumen.tools` and the MCP server return `kind`, `key` and the values by key next to the Japanese text.

| `kind` | SXF | JWW | DXF and DWG |
| --- | --- | --- | --- |
| `line` | `line_feature` | 線 (`CDataSen`) | `LINE` |
| `construction-line` | | | `XLINE`, `RAY` |
| `polyline` | `polyline_feature` | | `LWPOLYLINE`, `POLYLINE` (2D and 3D) |
| `multiline` | | | `MLINE` |
| `mesh` | | | `POLYLINE` (polyface and polygon meshes), `3DFACE` |
| `arc`, `circle` | `arc_feature`, `circle_feature` | 円弧, 円 (`CDataEnko`) | `ARC`, `CIRCLE` |
| `ellipse`, `ellipse-arc` | `ellipse_feature`, `ellipse_arc_feature` | 楕円 (`CDataEnko`, full or not) | `ELLIPSE` (full or not) |
| `spline` | `spline_feature` | | `SPLINE` |
| `clothoid` | `clothoid_feature` | | |
| `helix` | | | `HELIX` |
| `point` | `point_marker_feature` | 点 (`CDataTen`) | `POINT` |
| `text` | `text_string_feature` | 文字 (`CDataMoji`) | `TEXT`, `MTEXT` |
| `attribute`, `attribute-definition` | | | `ATTRIB`, `ATTDEF` |
| `dimension` | `linear_dim_feature`, `curve_dim_feature`, `angular_dim_feature`, `radius_dim_feature`, `diameter_dim_feature` | 寸法 (`CDataSunpou`) | `DIMENSION`, `ARC_DIMENSION`, `LARGE_RADIAL_DIMENSION` |
| `leader` | `label_feature` | | `LEADER`, `MULTILEADER` |
| `balloon` | `balloon_feature` | | |
| `hatch` | `fill_area_style_colour_feature`, `fill_area_style_hatching_feature`, `fill_area_style_tiles_feature`, `externally_defined_hatch_feature` | | `HATCH`, `MPOLYGON` |
| `solid` | | ソリッド, 円ソリッド (`CDataSolid`) | `SOLID`, `TRACE` |
| `symbol` | `externally_defined_symbol_feature` | | |
| `image`, `wipeout`, `viewport` | | | `IMAGE`, `WIPEOUT`, `VIEWPORT` (the frame in a layout) |

Placements of sub-figures, parts and blocks are resolved, so there is no kind for them: the entities inside carry the
`placement` (SXF) or `block` (DXF and DWG) property and `SceneEntity.placement`.

The keys that every format uses for the same meaning:

| `key` | `raw` | Meaning |
| --- | --- | --- |
| `type` | | The Japanese type, the first property of every entity |
| `id` | | The entity id (SXF and JWW number, DXF and DWG handle), as `SceneEntity.id` |
| `layer` | number | The layer name; `raw` is the `SceneLayer.id` |
| `color` | number | The color; `raw` is the format's color number (SXF color code, JWW pen color, DXF ACI) when there is one |
| `lineType` | number | The line type; `raw` is the code in SXF and JWW |
| `lineWidth` | number | The line width in millimeters (SXF 線幅, DXF and DWG 線の太さ when given) |
| `start`, `end`, `center`, `position` | `[x, y]` | Points in the drawing's own coordinates (JWW: real-world) |
| `length`, `radius` | number | |
| `angle` | `[from, to]` | The angles of an arc in degrees, counterclockwise |
| `text`, `font` | | The text of an entity and its font |
| `vertexCount`, `controlPointCount` | number | |
| `dimensionType` | string | `linear`, `aligned`, `angular`, `angular-3-point`, `radius`, `diameter`, `ordinate`, `arc-length` or `jogged-radius` (not in JWW) |
| `dimensionText` | number | The dimension text; `raw` is the measurement when the file stores it (DXF and DWG) |
| `pattern` | string | The hatch pattern; `raw` is the pattern name in DXF and DWG (`SOLID` for a fill) |
| `scale` | number | The `N` of a scale 1/N: JWW layer groups and DXF and DWG viewports |

Keys of one format: `placement`, `rotation`, `textBox`, `marker`, `name`, `radii`, `parameter`, `curveLength`,
`dimensionLine`, `dimensionArc`, `arrows`, `holeCount` and `hatchLines` in SXF; `textSize` (`[width, height,
spacing]`) in JWW; `block`, `height`, `textStyle`, `tag`, `wrapWidth`, `majorRadius`, `ratio`, `degree`, `width`,
`faceCount`, `meshSize`, `dimensionStyle`, `contentBlock`, `leaderCount`, `style`, `widthScale`, `justification`,
`patternScale`, `patternAngle`, `imageSize`, `viewCenter`, `viewHeight` and `twistAngle` in DXF and DWG; and
`comparison` (`unchanged`, `removed`, `changed-before`, `added`, `changed-after`) in the scene of
`zumen_core::compare`.

`Scene.metadata` uses `format`, `sheet` and `blockCount` in every format that has them; `software`, `library`,
`figureCount`, `archive` and the title block attributes of SXF (`businessName`, `constructionName`, `contractType`,
`drawingName`, `drawingNumber`, `drawingType`, `scale`, `year`, `month`, `day`, `contractor`, `client`); `scale`
(`raw` the list of N), `memo` and `settings` in JWW; `encoding`, `units`, `layouts`, `view` (`raw` the view id),
`viewOrigin` and `viewportScales` in DXF and DWG.

A new key is added where a format first needs it, with a name that another format can reuse for the same meaning.

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
- **Readers that load.** A `DrawingReader` may have `load()`, as the WebAssembly reader of `@zumenjs/reader` does; `useDrawing` waits
  for the one that recognizes the file.
- **React.** `useDrawingViewer`, `useViewerState(viewer, selector)`, `useViewerEvent(viewer, type, listener)` and
  `useDrawing(file, readers, options)`.
- **Text.** The engine has none: the canvas label (`label`, `setLabel`) and every other text come from the
  application. Scenes carry Japanese entity types, property labels and notice messages for people, and next to them
  the stable keys of [Stable keys](#stable-keys) (`kind`, `key`, `raw` and `Notice.code`) for applications.

The playground's 2D sample (`playground/drawing-sample.tsx`, with `layer-list.tsx` and `drawing-sample.css`) is a
complete viewer, with a toolbar, a layer list, properties, notices and a status bar, written only against this API.

## The Rust implementation

The readers are written in Rust so that servers, data pipelines, Python and browsers read drawings with the same
results. Browsers use the WebAssembly build through `@zumenjs/reader`
([decision 0005](decisions/0005-browser-packages-for-embedding.md)). JWW, SXF and DXF were written in TypeScript first
and ported; the TypeScript readers stay in the workspace as the reference that writes the golden files, and both are
compared on the fixtures and on real drawings. DWG is read only in Rust ([decision 0003](decisions/0003-dwg-in-rust.md)).

```text
bytes ─▶ zumen-sxf, zumen-jww, zumen-dxf, zumen-dwg ─▶ Scene (zumen-core) ─┬▶ JSON ─▶ sceneFromJson ─▶ DrawingViewer
                                                                             ├▶ PNG (zumen-render)
                                                                             └▶ Python (bindings/python), WebAssembly (bindings/wasm)
```

| Crate | Responsibility |
| --- | --- |
| `zumen-core` | The `Scene` types (serde, the same JSON as `sceneToJson`), `NoticeList`, `DrawingReader` and `read_drawing`, geometry (`geometry`), text layout (`text`), measurement, texts and entities for servers (`extract`), the fields of the title block (`title`), the comparison of two revisions (`compare`), read limits (`limits`), and JavaScript-compatible number handling (`js`). |
| `zumen-sxf` | SFC and P21 parsing into the SXF feature model (`parse_sfc`, `parse_p21` → `SxfDrawing`), conversion to a scene (`sxf_to_scene`), the `SXF` reader, and writing the feature model as SFC (`write_sfc`). The code tables and the rules that turn codes into colors, line types, widths and text (`SxfDocument::color` …) are public, for the conversion. |
| `zumen-jww` | JWW parsing into the JWW document model (`read_jww`), conversion to a scene (`jww_to_scene`), and the `JWW` reader. The rules for pen colors, line types, widths, layer names and text (`JwwDocument::pen_color` …) are public, for the conversion. |
| `zumen-dxf` | DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parse_dxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxf_to_scene`), the `DXF` reader, and writing the model as DXF (`write_dxf`, and `write_dxf_as` for R12 to 2018). The draw order of each space (`draw_order`) and the drawing unit (`drawing_unit`) are public, for the conversion to SXF. |
| `zumen-dwg` | DWG decoding (AutoCAD 2000 to 2018) into the AutoCAD drawing model of `zumen-dxf` (`read_dwg` → `DxfDocument`), drawn with `dxf_to_scene`, and the `DWG` reader. |
| `zumen-convert` | Maps SXF and JWW into the AutoCAD drawing model (`sxf_to_dxf`, `jww_to_dxf` → `Mapped`: the `DxfDocument`, the notices, and the source of each entity), so that they can be written as DXF ([decision 0007](decisions/0007-mapping-crate-between-formats.md)), and maps the AutoCAD drawing model into the SXF feature model (`dxf_to_sxf` → `MappedSxf`), so that it can be written as SXF ([decision 0010](decisions/0010-write-sxf-from-the-autocad-model.md)), and into the JWW document model (`dxf_to_jww` → `MappedJww`). |
| `zumen-draft` | Draws a drawing from a JSON drawing spec into the AutoCAD drawing model with the builder of `zumen-convert` (`draft` → `Drafted`: the `DxfDocument`, the notices and the place in the spec of each entity): one layout per sheet, a viewport for each view at its scale, sizes in paper millimeters, dimensions with their blocks, hatches with pattern lines, tables, title blocks, blocks with attributes ([decision 0018](decisions/0018-drafting-kit.md), [Drafting](drafting.md)). |
| `zumen` | `read` and `detect` over every reader, with limits for untrusted drawings (`ReadDrawingOptions::limits`), `convert` (every format to DXF, DWG, JWW and SXF through the AutoCAD model; `mapping` re-exports `zumen-convert`), `edit` and `create` (DXF, DWG, JWW and SXF), and `draft` (a drawing spec written in any of those formats; `draft` re-exports `zumen-draft`). The entry point for applications. |
| `zumen-render` | Draws a scene to PNG with the rules of `render.ts` in `@zumenjs/viewer` (color inversion, line weights with a 1 px minimum, dash rules, even-odd fills, clipping, per-character text, small text as a line), and reports which entities it drew and where (`render_image`, `drawn_entities`, `View::pixel_box`). It also writes SVG (`render_svg`) and one-page PDF (`render_pdf`) with the same colors, fills, clipping and text placement. |
| `zumen-cli` | The `zumen` command (`detect`, `scene`, `texts`, `entities`, `title`, `render`, `compare`, `convert`). |
| `bindings/python` | PyO3 bindings, built with maturin as an abi3 wheel for Python 3.12 and later. The GIL is released while reading, rendering and converting. `Scene.to_arrow` needs pyarrow (Apache-2.0, `zumen[arrow]`), and the MCP server `zumen.mcp` needs the MCP SDK (MIT, `zumen[mcp]`); both are optional dependencies. `zumen.tools`, the functions behind the MCP tools, needs neither. |
| `bindings/wasm` | wasm-bindgen bindings that detect the format (`detect`) and return scene JSON (`readSceneJson`, with the limits on bytes and entities; a failure throws a `ReadFailure` with the message and the exceeded limit). `scripts/wasm.mjs` builds them into `packages/reader/wasm` for `@zumenjs/reader`. |

The dependency rules follow the TypeScript packages: format crates depend only on `zumen-core` and never on each other
(the exception is `zumen-dwg`, which reads into the AutoCAD model of `zumen-dxf` and draws with `dxf_to_scene`), and
parsing does not depend on rendering. `zumen-convert` is not a format crate: it is the one place that knows two formats
at once, and depends on `zumen-dxf`, `zumen-sxf` and `zumen-jww` ([decision 0007](decisions/0007-mapping-crate-between-formats.md)).
`zumen-draft` builds the AutoCAD model only, with the public `builder` and `sheet` modules of `zumen-convert`, and
leaves writing to the facade, so it knows no format but the model ([decision 0018](decisions/0018-drafting-kit.md)).

### The readers in the browser

`@zumenjs/reader` is a `DrawingReader` around the WebAssembly build of `bindings/wasm`, as
[decision 0005](decisions/0005-browser-packages-for-embedding.md) chose, and replaced `@zumenjs/dwg`, which read DWG
only. Measurements are from [Verification records](verification.md#the-readers-in-the-browser) and
[DWG in the browser](verification.md#dwg-in-the-browser).

- **Loading is asynchronous, reading is synchronous.** `DrawingReader.read` returns a scene, so the module is loaded
  first: `initReader()` returns one promise shared by every call, a failed load is retried by the next call, and
  `reader.read` throws until the promise has resolved. Browsers have to fetch the file anyway, and
  `WebAssembly.instantiateStreaming` compiles it while it downloads. `initSync` needs the bytes at hand and compiles
  on the calling thread, so it is not used.
- **Formats are detected without WebAssembly.** `detectDrawing` (the reader's `detect`) repeats in TypeScript the
  rules of `zumen::detect`: the SXF header and its comments, the central directory of a P2Z or SFZ archive, the JWW
  signature, the first tags of ASCII and binary DXF, and the DWG version string. A test compares the two on every
  fixture and on edge cases. Before the module is loaded, `readDrawing` still gives the usual error for files no
  reader knows, and an application can load the module only when a drawing is opened, as `useDrawing` does.
- **Limits.** `createReader({ limits })` and `readScene(bytes, { limits })` pass `maxBytes` and `maxEntities` to
  `zumen::read`, which stops as soon as one is exceeded. The binding throws a `ReadFailure` with the message and the
  name of the limit, and the package turns it into a `LimitError` (`limit` is `bytes` or `entities`) or an `Error`.
  WebAssembly in browsers has no clock that Rust can read while it runs, so the time limit is not passed to Rust.
- **Web Workers.** `ReaderWorker` reads in a worker created by a function of the application (a file that imports
  `@zumenjs/reader/worker`, so that its bundler builds a worker). The worker reads with `readJson` and sends the JSON
  string, and the page turns it into a scene with `sceneFromJson`. Sending the scene itself meant structured cloning
  one `Float64Array` per primitive: in Chromium, a 16.6 MB SFC with 227,139 entities took 14.5 s through the worker
  against 2.9 s for reading on the page, of which cloning was most; sending the string takes 2.9 s in all, 0.9 s of
  it on the page (parsing). A string is copied, not transferred, but avoids cloning the scene's many typed arrays.
  The alternatives and the page-side decoding cost are recorded in
  [decision 0016](decisions/0016-send-worker-scenes-as-json.md). Reads run one
  at a time; `limits.timeout` starts when the worker is sent the drawing, and when it passes, the worker is terminated,
  the read is rejected with a `LimitError` (`time`), and the next read creates a new worker. An `AbortSignal`
  terminates it the same way. This is the only way to stop a read that is running, because WebAssembly cannot be
  interrupted from outside.
- **Where the `.wasm` is.** The package ships `wasm/zumen_wasm.js`, its types and `wasm/zumen_wasm_bg.wasm`. Without
  an argument, `initReader` uses `new URL("../wasm/zumen_wasm_bg.wasm", import.meta.url)`, which bundlers such as Vite
  and webpack 5 follow: they copy the file and rewrite the URL, in the worker's bundle as well (checked with a Vite 8
  build). In Node.js, whose `fetch` does not read `file:` URLs, the file is read with
  `process.getBuiltinModule("node:fs/promises")`, which keeps `node:` imports out of browser bundles. Anywhere else,
  the application serves the file (exported as `@zumenjs/reader/zumen_wasm_bg.wasm`) and passes a URL, a `Response`,
  the bytes or a `WebAssembly.Module` (`ReaderWorker` takes a URL or a `WebAssembly.Module`, which can be sent to a
  worker).
- **The scene comes back as JSON.** `readSceneJson` returns the JSON that the Rust readers already share with
  `sceneToJson` (checked by the golden files), and `sceneFromJson` turns the point lists back into `Float64Array`.
  Building JavaScript objects from Rust instead (`serde-wasm-bindgen`) would add code to the module and cross into
  JavaScript for every value; `sceneFromJson` takes about a third of the time of a read.
- **Speed.** Reading through WebAssembly takes up to twice as long as the TypeScript readers on JWW and DXF, and less
  on P21 (14 ms instead of 6 ms for a JWW drawing of 1,754 entities, 31 ms instead of 62 ms for a P21 drawing of
  1,201). For drawings that take long, `ReaderWorker` keeps the page responsive.
- **Views pass through.** `options.view` goes to `readSceneJson`, and the scene carries `views` and `view`, so
  layouts work as in `@zumenjs/dxf`. The module keeps no document between calls, so each change of view decodes the
  file again.
- **One module for every format.** The module is the whole `bindings/wasm` build. An application that opens a single
  format downloads the readers of all of them, but the module is downloaded once, while a module per format would
  repeat `zumen-core`, `serde_json` and the encoding tables in each. A build with the DWG reader only was 24% smaller
  with brotli.
- **No `wasm-opt`.** `scripts/wasm.mjs` runs cargo (release, LTO) and `wasm-bindgen` only. `wasm-opt -O` (Binaryen)
  made the file 12% smaller but only 2.3% smaller with brotli, and it reads no faster, so the build does not need
  Binaryen. `wasm-bindgen-cli` must be the version of the `wasm-bindgen` crate in `Cargo.lock`; the script checks it
  and prints the command that installs it.
- **Tests.** `packages/reader/tests` reads every fixture through WebAssembly and compares the scene with the golden
  file the TypeScript reader wrote, and `worker.browser.test.ts` runs `ReaderWorker` in Chromium (time limits, abort,
  a module that fails to load). The DWG drawings in `tests/fixtures/dwg` are written by the Rust tests
  (`crates/zumen-dwg/tests`, with `UPDATE_GOLDEN=1`), their scenes by `@zumenjs/reader` through WebAssembly
  (`pnpm run golden`), and `crates/zumen/tests/golden.rs` checks that the native reader returns the same scenes.
  `pnpm run check` builds the module and compares it with every fixture (`pnpm run wasm`).

### Texts, entities and images for servers

Servers and vision models read a drawing from an image and from its contents together. `zumen_core::extract` selects
the entities of a scene by region and layer visibility (the rules of the renderer) and gives the text of an entity:
the text primitives on one line are joined, a line break separates lines (stacked fractions become two lines), and the
position, height, angle and box are in paper millimeters. `zumen_render::render_image` returns the PNG with the
conversion between pixels and paper millimeters, and `drawn_entities` with `View::pixel_box` gives the box of every
entity drawn, in pixels. An application can then show the entity that a model refers to, or render the part of the
sheet around it. These work on the scene, so every format gets them at once. There is no TypeScript counterpart yet.

`extract::entity_shapes` gives the shapes that an entity draws, from its primitives: polylines (curves as the
polyline drawn), segments, fills, hatched areas (their rings, without the hatch lines), points and text parts.
`real_shapes` gives the same shapes through the inverse of the entity's placement, in the coordinates of the drawing
itself (real-world millimeters), so that a client can compute lengths, areas and distances between entities without
reading every format's properties ([decision 0009](decisions/0009-geometry-for-ai-clients.md)).

`zumen_core::compare` compares two revisions of a drawing. Entity ids change when some formats are saved again, so
entities are matched by type, shape and text instead, in three passes: shapes equal after rounding coordinates to the
tolerance (0.01 mm by default; the direction of a line and the start of a ring do not matter), shapes split by the
rounding (coordinates within twice the tolerance), and then entities of the same type that sit in the same place,
which count as changed (a text at the same position, or boxes that overlap by half). Colors, line weights, line types
and layers count only when asked. When a revision moved as a whole (DXF model space shifted by different extents),
the most common offset between texts that occur once in both revisions moves it back. Changes near each other are
grouped into regions, and `diff_scene` draws the result as a scene with three layers: unchanged entities in gray,
removed ones and the old state of changed ones in red, added ones and the new state in blue.

`zumen_core::title` guesses the fields of the title block, so that drawings can be filed by drawing number and
revision: the drawing number, revision, title, project, scale and date. In order of trust, it reads attribute tags
(DXF and DWG `ATTRIB`, such as `DWGNO` or `REV`), labels such as 図面番号, 図　面　名 or `DWG. NO.` with the nearest text to
their right in the same row or else below them (in the label's own direction, so rotated title blocks work too),
labels with their value in the same text (`縮尺 1/100`), the sheet name, and the metadata (the title block attributes
that SXF files carry, with the year, month and day joined into one date). Labels are compared after NFKC, case
folding and removing spaces and punctuation; a bare 番号 or `NO` is not taken for a drawing number. When a drawing
has several title blocks, the one nearest the bottom right of the sheet comes first. Every candidate is kept with
its source, label and entities, so an application can show where a value came from.

`zumen_core::regions` finds the regions that drawn lines close, such as rooms, with their areas
([decision 0015](decisions/0015-analysis-in-zumen-core.md)). The lines of lines, polylines, arcs, circles, splines
and solids (not dimensions, texts, leaders or hatches) are split where they cross and where an end lies on another
line, ends closer than the tolerance (0.01 paper millimeters by default) become one point, and lines that end
nowhere are removed. Every bounded face of what is left is a region, found by walking the half-edges around each
point in angle order; a closed shape that touches no other line is a hole of the smallest face around it. Lines of
placements at different scales are never joined, and a region inside a placement also has its outline, area and
perimeter in real-world millimeters. Each region names the entities that bound it and the texts whose box center lies
inside it. Which regions are rooms is left to the caller, with filters by layer, text and area.

`zumen_core::tables` reads the tables drawn with rules (schedules, area tables, drawing lists, title blocks) on the
same faces: only the horizontal and vertical segments of the boundary lines are kept, so that a slash across an empty
cell or a symbol does not split a cell, and every rectangular face is a cell. Cells of one placement that share an
edge longer than the tolerance are joined, and a group of cells that fills a rectangle is a table, after leaving out
a cell that covers more than half of it and is at least ten times the next largest (the drawing area beside a title
block strip). The rules between columns and rows are the cell edges merged within twice the tolerance; each cell has
its row, column and spans, and the texts whose box center lies in it, joined from the top line down. `RuledTable::grid`
gives the text by row and column, with a merged cell's text in its top left place.

`zumen_core::checks` checks a scene against a rule set, JSON data with rules for layer names, colors, line types, line
widths, text heights, characters, the sheet and scales ([Checks](checks.md)). It reads only the stable keys, the
shapes drawn and the sheet, counts what it could not check because the scene lacks the value, and groups the items
that break a rule by their value. The CAD drafting standard of MLIT (`mlit-cad-2025`) is built in.

`zumen.tools` gives them to AI agents as functions with JSON results: summarize a drawing (with its placements and
their conversion to real-world millimeters), render a region, find texts and entities, show one entity with its
shapes, measure in real-world millimeters through the scale of a placement, find the closed regions with their
real-world areas, read the ruled tables as rows and columns, check a drawing against a CAD standard and compare two
revisions. The results
follow `TypedDict` classes that are part of the API, are bounded by the arguments (items, characters of a text, points
of shapes, pixels), and take regions in paper millimeters or as fractions of the frame (the scene's `bounds`, origin at
the top left) ([decision 0011](decisions/0011-tool-functions-for-agents.md)). `zumen.mcp` serves the same functions as
MCP tools over stdio (`zumen-mcp`), reading local paths limited by `--root` with limits on file sizes
([decision 0004](decisions/0004-mcp-server-in-the-python-package.md)).

### Limits for untrusted drawings

Applications that read drawings from others set limits on bytes, entities and time (`ReadLimits`), and reading stops
part way with an error that names the limit (`ReadError::exceeded`). `zumen::read` checks the size first, then runs
the reader inside `zumen_core::limits::with_limits`, a budget kept in thread-local cells. Readers ask it whether they
may add each entity (`admit`, at the one place where a scene builder pushes an entity; `release` gives back the parts
that a DXF dimension merges into one) and call `tick` in loops over input records and at the start of each entity
they draw, including entities inside blocks; `tick` reads the clock every 256 calls. Once a limit is reached, every
later `admit` and `tick` says stop, loops end, and `zumen::read` returns the limit error instead of the partial scene.
Outside `with_limits` these calls do nothing, so a read without limits returns what the TypeScript reader returns.
The time limit is not available in WebAssembly, where there is no clock. P2Z and SFZ (ZIP archives of P21 and SFC)
are unpacked up to `max_bytes`, or 2 GiB without it, so that a small archive cannot unpack to fill the memory.

### Writing and converting

Drawings are written through the AutoCAD drawing model ([decision 0006](decisions/0006-write-through-the-autocad-model.md)).
`zumen::convert` reads a DXF or DWG drawing into a `DxfDocument`, or maps an SXF or JWW drawing into one with
`zumen-convert`, and `zumen_dxf::write_dxf` writes it as DXF of AutoCAD 2013 in UTF-8 (`write_dxf_as` writes R12 to
2018; R12 first reduces the model to what R12 holds, `r12::prepare`). For SFC, `dxf_to_sxf` maps the
model into the SXF feature model (`SxfDrawing`) and `zumen_sxf::write_sxf_version` writes it as SFC, P21, SFZ or P2Z
([decision 0010](decisions/0010-write-sxf-from-the-autocad-model.md)). For JWW, `dxf_to_jww` maps the model into the
JWW document model and `zumen_jww::write_jww` writes it, and for DWG, `zumen_dwg::write_dwg` writes the model as
AutoCAD 2013.

```text
DXF, DWG ──────────────────────────▶ DxfDocument (zumen-dxf) ─┬▶ write_dxf ─▶ DXF (AutoCAD 2013, UTF-8)
SXF, JWW ─▶ sxf_to_dxf, jww_to_dxf ─▶      ▲                  └▶ dxf_to_sxf ─▶ SxfDrawing ─▶ write_sfc ─▶ SFC
            (zumen-convert)                │                     (zumen-convert)            (zumen-sxf)
                                           └ read again, each output draws the same scene ┘
```

SXF and JWW are sheets in paper millimeters with scales for parts of the sheet. The mapping puts every scale at real
size in model space and shows it through a viewport of the sheet's layout, and keeps what is drawn on the sheet itself
in paper space ([decision 0008](decisions/0008-sheets-and-scales-in-the-autocad-model.md)). It follows the scene of the
source value for value (the code tables and rules the scenes use are public for this), so the layout of the written
DXF draws the source's scene entity by entity (`crates/zumen/tests/convert_sxf_jww.rs`, which matches the entities
through the source of each written entity and compares boxes and drawn primitives within the difference in curve
flattening).

The writer is tested by reading what it writes: in every view, the scene of the written DXF has the same entities
(types, layers, boxes and drawn primitives) as the scene of the drawing it came from, and the handles of the written
file are unique and resolve (`crates/zumen/tests/convert.rs`, `write::tests`). ezdxf, a separate implementation, reads
the written fixtures in the Python tests; it is a development dependency only (MIT), not a dependency of the package.
What the writer leaves out or writes differently is reported in `notices`, with the same `Notice` type as the readers.
The writer has no TypeScript counterpart.

The SXF writer is tested the same way, through the SXF reader: an SXF or JWW drawing converted to DXF and then to SFC,
and every DXF and DWG fixture converted to SFC, draw the scene of the drawing they came from entity by entity
(`crates/zumen/tests/convert_to_sxf.rs`), within the difference in curve flattening and the losses reported in
`notices`. Writing a read SFC again gives the same drawing (`crates/zumen-sxf/tests/write.rs`).

### Editing in place

Editing does not go through the model, which would lose what it does not hold
([decision 0012](decisions/0012-edit-in-place-and-write-every-format.md)). `zumen_core::edit` has the operations, the
same for every format (`parse_operations` reads them from JSON, `ValueReader` checks their keys and values), and each
format edits its own file: `zumen_dxf::edit_dxf` keeps the tags of a DXF with their bytes and changes, adds and removes
only what the operations name ([How DXF is handled: Editing](formats/dxf.md#editing-edit_dxf)), and `edit_dwg`,
`edit_jww` and `edit_sxf` do the same for the objects of a DWG, the records of a JWW and the instances of an SXF.
`zumen::edit` detects
the format and calls its editor, and `zumen::create` writes an empty drawing of the format and edits it, so the code
that adds entities is the same for new and existing drawings.

```text
bytes ─▶ editor of the format (tags, objects, instances, records) ─▶ bytes: what no operation names, byte for byte
            ▲ operations (zumen_core::edit): add, update, delete, add-layer, update-layer, add-block,
              add-text-style, add-line-type, add-dimension-style
```

`crates/zumen/tests/edit.rs` is the conformance test of every editor: without operations the bytes stay the same, and
after adding, changing and deleting, the drawing read again differs from the original exactly in those entities. It
compares the drawing's own coordinates (`real_shapes`), because the place on the sheet moves when the extents change.
ezdxf audits edited DXF in the Python tests. Editors for DWG, SXF and JWW follow in their crates, with the same test.

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
  there come from the Rust tests instead (see [The readers in the browser](#the-readers-in-the-browser)).
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
| `serde`, `serde_json` | MIT OR Apache-2.0 | The scene JSON shared with the browser, and the edit operations read from JSON (`zumen_core::edit`) |
| `encoding_rs` | (MIT OR Apache-2.0) AND BSD-3-Clause | Shift_JIS (CP932), the DXF and DWG code pages (`$DWGCODEPAGE`) and UTF-16 (DWG 2007 and later) with the WHATWG rules of `TextDecoder` |
| `unicode-normalization` | (MIT OR Apache-2.0) AND Unicode-3.0 | NFKC for font names, as `String.prototype.normalize` (about 128 KB of the WebAssembly build) |
| `libm` | MIT | The same trigonometry on every target |
| `tiny-skia` | BSD-3-Clause | CPU rasterization for PNG, with anti-aliasing, dashes, even-odd fills and masks |
| `pdf-writer` | MIT OR Apache-2.0 | PDF objects and content streams for `render_pdf` (no dependencies beyond number formatting) |
| `subsetter` | MIT OR Apache-2.0 | The glyphs of the bundled font that a PDF uses, embedded as a subset; default features off (no variable fonts) |
| `miniz_oxide` | MIT OR Zlib OR Apache-2.0 | Flate compression of PDF streams; already compiled for PNG encoding |
| `ttf-parser` | MIT OR Apache-2.0 | Glyph outlines of the bundled font |
| `pyo3` | MIT OR Apache-2.0 | Python bindings |
| `wasm-bindgen` | MIT OR Apache-2.0 | WebAssembly bindings |

SVG and PDF draw the sheet, or a region given in paper millimeters, at its real size; without a sheet, the bounds of
the drawing. Coordinates stay paper millimeters, moved so that the page starts at zero before they are written as
32-bit numbers. Lines keep their weights: in SVG no thinner than 1 CSS pixel at 100% (0.2646 mm), as the viewer
draws them at zoom 1, and in PDF exactly as drawn, with weight 0 as the thinnest line the device draws. Dash lengths
stay in millimeters, since vector output is zoomed and printed rather than drawn at one scale. SVG writes text as
`<text>` in the scene's CSS fonts, one character per cell, so it can be searched and selected. PDF embeds the glyphs
of BIZ UDGothic that the page uses (a CID font with Identity-H and a ToUnicode map, so text can be searched and
copied) and places each glyph like the PNG renderer: the middle of its advance and of the em box at the center of
its cell. A page that is the sheet itself has no sheet frame.

`zumen-render` bundles BIZ UDGothic Regular (SIL Open Font License 1.1, `crates/zumen-render/fonts/OFL.txt`),
a Japanese gothic close to the MS Gothic that SXF and Jw_cad drawings assume. Characters without a glyph are reported
(`RenderReport.missing_glyphs`, a Python warning).

Release packages include source files for source maps. `scripts/notices.mjs` records the license texts of normal Rust
dependencies linked into the Python extension and WebAssembly reader; `pnpm run check` verifies these files against
Cargo.lock.

## Development setup

- Each package's `exports` has an `@zumenjs/source` condition that points at `src`. The type checker
  (`customConditions` in `tsconfig.json`), Vitest and the Vite playground resolve it, so changes apply without a build.
  `publishConfig.exports` leaves it out of the published packages, which contain `dist` and `src` (and `wasm` in
  `@zumenjs/reader`).
- `scripts/wasm.mjs` builds the WebAssembly reader into `packages/reader/wasm`, which is not in Git, with cargo and
  `wasm-bindgen-cli`. `pnpm dev` and `pnpm run check` run it first; `pnpm test`, the type checker and the builds need
  its output. When the Rust code has not changed, it takes well under a second.
- `pnpm run build` compiles the packages in dependency order (core, then viewer, sxf, jww, dxf and reader, then react)
  into each `dist`.
- `pnpm test` runs two Vitest projects: the unit tests in Node.js, and the tests of the engine and the hooks
  (`*.browser.test.*`) in Chromium through Playwright, with synthetic pointer, wheel and key events and the pixels of
  the canvas. The Chromium headless shell of the Playwright version in `package.json` is installed with
  `pnpm exec playwright install --only-shell chromium`.
- `pnpm run check` runs Biome, builds the WebAssembly reader and compares it with the fixtures (`pnpm run wasm`), then
  runs the type checker, the unit tests, `pnpm audit` and all builds, then the Rust checks (`check:rust`: rustfmt,
  Clippy, tests and the wasm32 build) and the Python checks (`check:python`: Ruff, maturin, pytest).
- The Rust toolchain is pinned in `rust-toolchain.toml`, Python in `bindings/python/.python-version` (installed by uv).

Legacy DWG REPEAT copies have generated `#n` ids after the original record ordinals, so each copy can be selected independently.
