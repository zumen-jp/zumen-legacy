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
| `@zumenjs/core` | What `zumen-core` covers: the `Scene` model and its types and JSON (`sceneToJson`, `sceneFromJson`), `NoticeList`, `readDrawing` and the `DrawingReader` interface (with the optional `load`), layer visibility (`isEntityVisible`), distance measurement (`measureDistance`, `toReal`), and drawing markup (`markupFromJson`, `markupToJson`, `markupShapes`, `drawingHash`, [Drawing markup](markup.md)). Geometry helpers are exported from `@zumenjs/core/geometry` and text layout from `@zumenjs/core/text`. No DOM. |
| `@zumenjs/viewer` | The 2D engine that applications embed: the Canvas 2D renderer (`renderScene`, `renderHighlights`, `renderPrimitives`), hit testing and snapping (`HitIndex`), `DrawingViewer` with the view, the state, input handlers, tools and style, and `MarkupLayer` with the markup tools, built on the public API of `DrawingViewer`. No text for people and no CSS. |
| `@zumenjs/reader` | Every format through the WebAssembly build of the Rust readers: `initReader` loads the module, `reader` and `createReader({ limits })` are `DrawingReader`s, `readScene` reads with limits (`LimitError`), `detectDrawing` tells the format without the module, and `ReaderWorker` reads in a Web Worker (`@zumenjs/reader/worker`). |
| `@zumenjs/sxf` | Not published; the reference for the tests. SFC and P21 parsing into the SXF feature model (`parseSfc`, `parseP21` → `SxfDrawing`), conversion to a scene (`sxfToScene`), and the `sxf` reader. |
| `@zumenjs/jww` | Not published; the reference for the tests. JWW parsing into the JWW document model (`readJww` → `JwwDocument`), conversion to a scene (`jwwToScene`), and the `jww` reader. |
| `@zumenjs/dxf` | Not published; the reference for the tests. DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parseDxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxfToScene`), and the `dxf` reader. Their Rust port (`zumen-dxf`) also draws DWG. |
| `@zumenjs/pdf` | Draws PDF pages under the scene with pdf.js: `pdfUnderlayRenderer`, an `UnderlayRenderer` for underlays of kind `pdf` (the pages of PDF drawings and the PDF underlays of DXF drawings) that renders the requested region of a page and turns optional content groups on and off as content layers are shown and hidden. It reads nothing: the entities of a page come from the reader. |
