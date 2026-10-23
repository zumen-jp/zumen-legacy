# 0005: Make the browser packages an engine to embed, with reading in Rust and the UI as a sample

> English reference preserved when Japanese became the primary documentation. See [0005-browser-packages-for-embedding.md](0005-browser-packages-for-embedding.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Proposed |

## Context

Applications that show drawings build their own interface, with their own design system, layout and language, and
put the drawing view inside it. Besides zoom, pan, selection and measurement, they need to set the view (go to an
entity, save and restore the view, keep two views together), keep HTML placed on the drawing in place while it moves,
add their own tools (markup, pins, areas), draw in the colors of their theme, read entity attributes as data, search
texts, show the title block, compare revisions and limit what a received file may cost.

Checked on main on 2026-10-20:

- `@zumenjs/core` (1,899 lines) holds the scene model, the geometry and text helpers, the Canvas renderer, hit testing,
  measurement and `DrawingViewer` (602 lines). `DrawingViewer` is one class with the canvases, every input handler,
  two modes (`select` and `measure`), the state store and fixed colors (a black or white background, and the highlight
  and measurement colors as constants). It always draws the sheet frame and labels the canvas `図面` unless told
  otherwise. The view can be read but not set, and panning does not notify subscribers, so HTML placed over the drawing
  drifts while it moves. The wheel, double click, keys and click selection cannot be turned off, and one entity at a
  time can be selected and highlighted. Besides calling its methods, applications can only add Canvas overlays,
  subscribe to the state and listen to selections.
- `@zumenjs/react` (690 lines) ships `CadViewer`, a complete interface with its own CSS and Japanese text, next to the
  hooks `useDrawingViewer` and `useCadFile`. `CadViewer` uses only `useDrawingViewer`, so the hook already reaches
  everything it does. `useCadFile` reads on the main thread.
- In `@zumenjs/stb` the roles are the other way round: the React component `StbViewer` keeps the interaction state
  (members hidden by kind, the story and axis in focus, the selection, the projection and the color mode) and handles
  picking and keys, while `StructureView` draws and moves the camera. Its labels need CSS that only `StbViewer` has.
- The readers exist twice, in TypeScript (`@zumenjs/sxf`, `@zumenjs/jww` and `@zumenjs/dxf`, 11,776 lines) and in Rust
  (19,534 lines with DWG), kept equal by the golden files. The architecture notes plan Rust as the single
  implementation, with browsers switching to WebAssembly format by format. The WebAssembly module already contains
  every reader (1.56 MB, 494 KB with brotli), but `bindings/wasm` exposes only `detect` and `readSceneJson`, and only
  `@zumenjs/dwg` uses it.
- What servers can learn from a drawing beyond the scene is in Rust only: texts and entities by region
  (`zumen_core::extract`), where each entity was drawn in an image (`zumen-render`) and, in open pull requests, the
  comparison of revisions (#10), the title block fields (#12), SVG and PDF output (#13) and limits on bytes, entities
  and time (#15). Browsers have measurement and hit testing.
- The scene model carries text for people: `SceneEntity.type`, `Property.label` and `Notice.message` are Japanese, and
  property values are formatted strings. Notices have a `code`, but entities and properties have no stable key, so an
  application can pick out an attribute only by matching its Japanese label.
- `@zumenjs/core` and `@zumenjs/react` have no tests; the unit tests cover the readers.

## Options

- **Options on the current classes**: colors, a way to set the view and switches for the input handlers. The least
  work, but `DrawingViewer` keeps growing as one class, `CadViewer` and `StbViewer` stay public interfaces to maintain,
  the 3D view keeps its state in React, and browsers still read with the TypeScript readers and lack the analysis.
- **An engine to embed, with the readers in TypeScript**: the engine split below, with the analysis ported to
  TypeScript. Browsers read without loading WebAssembly, but every analysis feature is written twice and kept equal,
  against the plan of the architecture notes.
- **An engine to embed, with reading and analysis in Rust**: browsers read and analyze through one WebAssembly
  package, the TypeScript packages draw and handle input, and the interface becomes a sample.

Within the last option:

- **Where the engine lives**: a subpath of `@zumenjs/core` (`@zumenjs/core/viewer`) or a package of its own. A package
  of its own lets `@zumenjs/core` cover what `zumen-core` covers, so the place of new code follows one rule: what both
  languages have goes to core, and what only browsers need goes to the viewer.
- **How applications set colors**: JavaScript options only, or CSS custom properties on the container with JavaScript
  overrides. Custom properties follow the application's theme and dark mode without code.
- **Where the interface lives**: a maintained component in `@zumenjs/react`, a separate example package, or samples in
  the playground.

## Decision

The last option, with a package of its own for the engine, colors from CSS custom properties and the interface as
samples in the playground.

| Package | Contents | Depends on |
| --- | --- | --- |
| `@zumenjs/core` | What `zumen-core` covers: the scene model and its JSON, geometry and text layout, measurement and `toReal`, notices and the `DrawingReader` interface. No DOM | Nothing |
| `@zumenjs/viewer` (new) | The 2D engine: the Canvas renderer, hit testing and snapping, and `DrawingViewer` as described below | `@zumenjs/core` |
| `@zumenjs/reader` (new, replaces `@zumenjs/dwg`) | Every format through the WebAssembly build of the Rust readers | `@zumenjs/core` |
| `@zumenjs/react` | Hooks only | `@zumenjs/viewer`, `@zumenjs/core` |
| `@zumenjs/stb` | Parsing (`.`), the 3D engine (`./three`) and hooks (`./react`), on the contract of the 2D engine | Nothing (`@zumenjs/core` once plan and elevation drawings come, as in 0001) |

The TypeScript readers (`@zumenjs/sxf`, `@zumenjs/jww` and `@zumenjs/dxf`) stay in the workspace as the reference that
writes the golden files until their format switches to WebAssembly. They are not published.

The 2D engine, `DrawingViewer` in `@zumenjs/viewer`, follows the split of map engines such as MapLibre GL JS and
OpenLayers, where the map, its input handlers and the controls around it are separate:

- **View**: the center and the scale, set with `jumpTo` and `fitBounds(box, { padding })` as well as `zoomBy` and
  `panBy`, read with `getView`, and announced by `move` and `moveend` events, with the conversions between screen
  pixels, paper millimeters and real-world millimeters.
- **State**: the scene, layer visibility, a selection of several entities, states per entity (selected, hovered, and
  named highlights such as search results) and the active tool, in a store read with `getState` and `subscribe` that
  makes a new object for every change. What changes at the pace of the pointer (the view, the paper position under the
  pointer) is sent as events, not kept in the store.
- **Input handlers**: dragging to pan, the wheel (always, only with a modifier key, or never), pinching, double click
  and keys, each of which can be turned off.
- **Tools**: one interface for the built-in tools (selection, measurement, panning) and for the tools of applications
  (markup, pins, areas). The active tool receives pointer and key events with paper coordinates, can ask for the
  entities and the snap point under the pointer, and can draw on the overlay.
- **Style**: the background, highlight and measurement colors of the dark and light backgrounds, the sheet frame and the
  fit padding come from CSS custom properties on the container (`--zumen-*`) and can be overridden in JavaScript. The
  background still defaults to the one the drawing assumes (`Scene.background`), and colors that cannot be told from
  the background are inverted according to its lightness. The drawing rules of `renderScene` and `zumen-render` do not
  change.
- **Text**: none. The label of the canvas and any other text come from the application.
- **Queries**: the entities at a point or in a box and the snap point, for applications that handle input themselves.

`@zumenjs/react` has hooks only: `useDrawingViewer` creates the engine on an element and destroys it,
`useViewerState(viewer, selector)` re-renders only when the selected part of the state changes, `useViewerEvent`
subscribes to an event, and `useDrawing(file, readers, options)` reads a file, waiting for readers that load
asynchronously (the `DrawingReader` interface gains an optional `load`). The packages ship no CSS.

`@zumenjs/stb/three` gets the same contract in a `StructureViewer`: a state store (members hidden by kind, the story
and axis in focus, the selection, the projection, the color mode and the guides shown), events (selection, hover,
camera), input handlers that can be turned off, tools, style from CSS custom properties, and a label layer that needs no
outside CSS. The state and the picking that `StbViewer` holds move into it, and `@zumenjs/stb/react` has hooks only.

`@zumenjs/reader` loads the WebAssembly module once (`initReader`, as `initDwg` does now) and then reads every format
synchronously as a `DrawingReader`, so `readDrawing` keeps working. It can also read in a web worker and hand the
scene to the page (a scene is plain data and `Float64Array`s, which structured cloning keeps), and it passes the read
limits to Rust once they are on main. Texts and entities by region follow as functions of the same package, and the
title block and the comparison of revisions once they are on main.

The interface leaves the packages. `CadViewer` with its layer list and `StbViewer` become samples in the playground,
written only against the public API and kept as plain as a sample can be. They show how to embed the engines and check
that the public API is enough.

The scene model gets stable keys next to the text for people: `SceneEntity.kind` (such as `line`, `arc`, `text` or
`dimension`) and `Property.key`, with the unformatted value where there is one. `type`, `label` and `message` stay,
for people and for AI applications through the MCP server. This changes the TypeScript readers, the Rust readers and
the golden files, so it is a change of its own.

## Consequences

- Nothing is published, so the packages change without a transition period: `DrawingViewer` moves to
  `@zumenjs/viewer` with the new API, `CadViewer` and `StbViewer` leave the packages, `@zumenjs/dwg` becomes
  `@zumenjs/reader` and `useCadFile` becomes `useDrawing`. The README examples change with each step.
- A browser waits for the WebAssembly module before its first read (494 KB with brotli for every format; compiling it
  took 3.6 ms in Chromium, see the verification records), and an application that opens a single format downloads the
  readers of all of them, as the architecture notes chose so that the module is downloaded once.
- Without published TypeScript readers, JavaScript applications lose the formats' own models (`SxfDrawing`,
  `JwwDocument`, `DxfDocument`) unless the WebAssembly package exposes them.
- The engines get tests of the view, the state, the input handlers and the tools.
- `AGENTS.md` and the architecture notes change with the first step: the dependency directions, interaction in the
  engines for 2D and 3D alike, hooks only in the React packages, and no CSS or text for people in the packages.
- The work goes in steps, each a pull request of its own:
  1. `@zumenjs/viewer`, the hooks and the 2D sample.
  2. The 3D engine on the same contract, and its sample.
  3. `@zumenjs/reader` with every format, the web worker and the read limits (after #15).
  4. Stable keys in the scene model.
  5. Analysis through `@zumenjs/reader` (after #10 and #12).

  Steps 1 to 4 touch different files and can go in parallel.
- Open questions, to settle in the step that needs them: the package names; how analysis reaches a scene that already
  lives in JavaScript (a handle that keeps the scene inside WebAssembly, or the scene JSON passed back, to be measured);
  whether the WebAssembly package exposes the formats' own models; the entity kinds and property keys shared by every
  format; and whether HTML markers belong in the engine or in applications.

## References

- [The architecture notes](../architecture.md), in particular
  [The Rust implementation](../architecture.md#the-rust-implementation),
  [DWG in the browser](../architecture.md#dwg-in-the-browser) and
  [Extension points for applications](../architecture.md#extension-points-for-applications)
- [Verification records](../verification.md#dwg-in-the-browser), for the size and the loading time of the WebAssembly
  module
- [0001: Keep ST-Bridge structural models in zumen as a TypeScript package](0001-st-bridge-in-zumen.md) and
  [0003: Read DWG in Rust only](0003-dwg-in-rust.md)
- MapLibre GL JS, [`Map`](https://maplibre.org/maplibre-gl-js/docs/API/classes/Map/): `jumpTo`, `fitBounds`,
  `setFeatureState`, `queryRenderedFeatures`, the `scrollZoom`, `dragPan`, `doubleClickZoom`, `keyboard` and
  `touchZoomRotate` handlers and the `move` and `moveend` events, read on 2026-10-20
- OpenLayers 10.11.0,
  [`ol/interaction/Interaction`](https://openlayers.org/en/latest/apidoc/module-ol_interaction_Interaction-Interaction.html):
  interactions that change the state of the map, turned on and off with `setActive`, read on 2026-10-20
- PDF.js, [Getting started](https://mozilla.github.io/pdf.js/getting_started/): the core, display and viewer layers,
  with the viewer built on the display layer, read on 2026-10-20
