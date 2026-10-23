# 0020: Read the paths and texts of PDF drawings in Rust only, with hayro-interpret, and leave the pixels to a PDF engine

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0002](0002-pdf-and-parapdf.md) divides PDF work with ParaPDF: in zumen's viewer a PDF engine draws the
  pixels of a PDF drawing, and zumen reads what interaction needs (pages as views, vector paths and texts that are hit
  and snapped to but not drawn, optional content groups as layers, the scale). It left a Rust reader that collects the
  vectors without rendering "until that need appears, after DWG", and asked for a record of its own because it would be
  written in Rust first. DWG is done ([decision 0003](0003-dwg-in-rust.md)).
- Drawings travel as PDF more than in any CAD format: sheets plotted for review, approval and filing, and drawings from
  parties that do not share their CAD files. Most are vector PDFs written by CAD plotters (lines, fills and, depending
  on the font, real text or text drawn as strokes); scans are images only.
- What zumen gives servers and AI agents works on the scene and exists only in Rust: texts and entities with their
  shapes ([decision 0009](0009-geometry-for-ai-clients.md)), closed regions, ruled tables and checks against CAD
  standards ([decision 0015](0015-analysis-in-zumen-core.md)), the title block and the comparison of revisions. Python,
  the `zumen` command and the MCP server reach them through `zumen::read` ([decisions
  0004](0004-mcp-server-in-the-python-package.md) and [0011](0011-tool-functions-for-agents.md)). The WebAssembly module
  has `detect` and `readSceneJson` only, and none of the analyses has a TypeScript counterpart.
- [Entity ids](../architecture.md#entity-ids) are a contract: the same bytes give the same ids in every language. An
  application that keeps an id in the browser (a selection, the anchor of a markup) and looks it up on a server needs
  both to have read the PDF the same way.
- The readers of JWW, SXF and DXF were written in TypeScript first and stay the reference that writes the golden files.
  DWG is read only in Rust, and checked against the DXF that AutoCAD writes for the same drawing.
- Components checked on 2026-10-23:

| Component | License | Notes |
| --- | --- | --- |
| [pdf.js](https://github.com/mozilla/pdf.js) (`pdfjs-dist`) | Apache-2.0 | Renders pages in browsers, in a worker. `getOperatorList` and `getTextContent` expose the operators and the text positions. The engine of `@zumenjs/pdf` ([#83](https://github.com/zumenjs/zumen/issues/83)) |
| [hayro-syntax, hayro-interpret](https://github.com/LaurenzV/hayro) 0.8.0 | Apache-2.0 OR MIT | Pure Rust, no unsafe code in the interpreter, MSRV 1.92. Interprets a page into a `Device`: `draw_path` (a kurbo `BezPath` with the stroke or fill), `draw_glyph_run` (positioned glyphs, with their Unicode from ToUnicode, the encoding or the glyph names), `draw_image`, clip paths, transparency groups and marked content. Tested on more than 1,400 PDFs, largely from the pdf.js and PDFBox suites; its demo runs in WebAssembly. It applies the default visibility of optional content itself and does not tell the device which group content belongs to. It always enables the image decoders of hayro-syntax (JPEG, JPEG 2000, JBIG2, CCITT), and depends on skrifa, moxcms, kurbo and phf (MIT, Apache-2.0, BSD-3-Clause, Zlib and Unicode-3.0) |
| [hayro](https://crates.io/crates/hayro) 0.8.0 | Apache-2.0 OR MIT | Renders pages into bitmaps on the CPU (vello_cpu, Apache-2.0 OR MIT), on hayro-interpret |
| [lopdf](https://crates.io/crates/lopdf) 0.46, [pdf](https://crates.io/crates/pdf) 0.10 | MIT | Objects, streams and content operators, without the graphics state, text state or fonts |

## Options

1. **No reader outside the browser.** `@zumenjs/pdf` reads paths and texts with pdf.js in TypeScript. Python, the
   command line and the MCP server get nothing, the analyses never see a PDF drawing, and ids exist only in browsers.
2. **pdf.js in browsers and a Rust reader on servers.** Two interpreters of a format as large as PDF cannot be kept
   equal value for value: they split text into runs, flatten curves, resolve fonts and recover from broken files in
   their own ways. The same PDF would have other entities and ids in the browser than in Python.
3. **A TypeScript reader as the reference, ported to Rust**, as for JWW, SXF and DXF. pdf.js is a dependency, not code
   to port value for value, so the reference would be a PDF interpreter of our own (filters, fonts, CMaps, the graphics
   and text state), written twice.
4. **One reader in Rust on hayro-interpret, for every runtime.** Browsers read through the WebAssembly module of
   `@zumenjs/reader`, and pdf.js draws the pixels only.
5. **One reader in Rust on hayro-syntax or lopdf with our own interpreter.** Full control over ids and optional content,
   but the text state, fonts (widths, CID fonts, Type 3), CMaps and the graphics state would be ours to write and
   maintain, which is what hayro-interpret is.

## Decision

Option 4. Option 5 stays the fallback if hayro-interpret cannot give what the reader needs, after asking upstream.

- **A format crate, `zumen-pdf`**, depending on `zumen-core` only, reads a PDF with hayro-syntax and hayro-interpret
  into a scene (`read_pdf`, the `PDF` reader of `zumen::read` and `zumen::detect`). It is detected by the `%PDF-` header
  in the first 1,024 bytes (ISO 32000-1:2008, 7.5.2), and `detectDrawing` of `@zumenjs/reader` repeats the rule, with
  the test that compares the two.
- **Pages are views**, like DXF layouts: the view id is the page number from 1 (`"1"`, `"2"` …), the name its page
  label (12.4.2) when it has one, and the first page is the default view.
- **Coordinates are paper millimeters** (a point is 25.4 / 72 mm, times `UserUnit`), with the origin at the lower left
  of the crop box and the page turned by `Rotate` (7.7.3.3). The crop box is the sheet. `Scene.unit` is `millimeter`,
  and the `raw` values of properties are millimeters on the sheet.
- **Entities**, in the order the interpreter paints them (8.5, 8.10, 9.4):
  - A painted path is one entity. Its kind follows its segments: `line` for one straight segment, `polyline` for
    straight segments, `spline` when it has curves, and `solid` when it is filled without a stroke. So regions and ruled
    tables read PDF lines as they read CAD lines.
  - A glyph run is a `text` entity with the Unicode of its glyphs (9.10), its position, height and angle from the text
    matrix and the font size, and the font name. Glyphs without Unicode become U+FFFD, counted in a notice.
  - A painted image is an `image` entity with its frame, the unit square through the current transformation matrix, like
    a DXF IMAGE without its pixels.
  - Content that is not read is reported by kind and count: shadings, tiling patterns, annotations (their appearance
    streams; PDF annotations belong to ParaPDF), content hidden by optional content, glyphs without Unicode.
- **Ids** are `#n`, where n counts the paths, glyph runs and images of the page in that order from 1, including those of
  form XObjects. The numbering follows the calls of hayro-interpret, so hayro is pinned to one version, and an update is
  a reader change like any other: `crates/zumen/tests/ids.rs` checks it, and an update that renames the entities of
  unchanged bytes needs a decision first.
- **The page is an underlay and the entities are not drawn.** Each view has the page as its underlay
  ([#80](https://github.com/zumenjs/zumen/issues/80)), and its entities are hit, snapped to, measured and extracted but
  not drawn ([#81](https://github.com/zumenjs/zumen/issues/81)). A PDF engine draws the underlay: pdf.js in browsers
  (`@zumenjs/pdf`, [#83](https://github.com/zumenjs/zumen/issues/83)) and hayro in `zumen-render` on servers
  ([#91](https://github.com/zumenjs/zumen/issues/91)), so `render_drawing` of the MCP server shows scans too. A renderer
  without an engine for the underlay reports it through `notices`.
- **Layers from optional content groups (8.11)** need to know the group of every entity, which hayro-interpret 0.8 does
  not tell the device. zumen proposes it upstream (the group with `begin_marked_content`, and a setting that shows every
  group), and until then a page has one layer, content hidden by default is not read, and the notice says that the file
  has optional content.
- **The scale** comes from the viewports of the page and their measure dictionaries (12.9): each viewport whose measure
  dictionary is rectilinear becomes a placement with its scale, like a DXF viewport. A page without them has no
  placement, and the application calibrates with two points in the viewer
  ([#83](https://github.com/zumenjs/zumen/issues/83)). The scale written in the title block (`title`) is a guess the
  application may apply; the reader does not.
- **Browsers read PDF through `@zumenjs/reader`** like every other format (`readSceneJson`, `ReaderWorker`), so the
  scene and its ids are the same in the browser and on servers. `@zumenjs/pdf` is a renderer, not a reader: it draws the
  page underlays with pdf.js and, once layers exist, turns optional content on and off through pdf.js as layers are
  hidden. It does not read entities (no `getOperatorList` or `getTextContent` for them) and does not depend on
  `@zumenjs/reader`; the application gives the same bytes to both. It depends on pdf.js and on the package that defines
  the underlay renderer ([#80](https://github.com/zumenjs/zumen/issues/80)), and the dependency directions in
  `AGENTS.md` gain it when it lands.
- **There is no TypeScript PDF reader**, as for DWG. In place of golden files written by TypeScript, `zumen-pdf` is
  checked by:
  - the smallest PDFs that pin a rule, written in its tests with `pdf-writer` (already a dependency of `zumen-render`):
    the transformation matrix, the text matrix and font size, Type 0 fonts with ToUnicode, `Rotate`, the crop box,
    `UserUnit`, form XObjects, viewports and measure dictionaries;
  - reading back what `render_pdf` writes: the scene of every fixture, drawn as PDF and read again, gives the same
    shapes and texts within the precision of the numbers written, a test whose answer is known;
  - `pnpm run parity` on the PDF files in `ZUMEN_SAMPLES`, comparing the texts and their positions with pdf.js
    `getTextContent` and reporting differences without failing, as for DWG against DXF;
  - a native fuzz example on damaged copies, like `crates/zumen-dwg/examples/fuzz.rs`.
- **Limits.** The device calls `limits::admit` for every entity and `limits::tick` for every path, glyph run and image,
  and `max_bytes` bounds the input. Work inside hayro between two calls (decoding a stream, parsing a font) cannot be
  stopped by the limits; what the fuzz example finds goes upstream.
- **PDF is read only.** `convert`, `edit` and `draft` refuse it; drawings are written as PDF by `render_pdf` ([decision
  0002](0002-pdf-and-parapdf.md)).

## Consequences

- Python (`zumen.read`), the `zumen` command and the MCP tools read PDF drawings with no other change, the view choosing
  the page: texts, entities and their shapes, closed regions, ruled tables (schedules and title blocks drawn with
  rules), the title block, checks (what a PDF does not have, such as layers and line types, counts as unchecked) and the
  comparison of two revisions of a PDF.
- Text that a plotter drew as strokes (SHX fonts) is paths, and a scan is one image: such pages have no text entities.
  zumen does no OCR; a vision model reads the rendered region.
- The WebAssembly module grows by the interpreter, the font parser, color management and the image decoders that
  hayro-interpret enables although the reader decodes no image. The reader issue measures the growth with brotli and
  records it in [Verification records](../verification.md). PDF stays in the one module of every format unless it more
  than doubles the module; then a record decides whether it gets a module of its own.
- `zumen-render` depends on hayro and vello_cpu for the underlay on servers. The WebAssembly module does not include
  them, since pdf.js draws pages in browsers.
- The reader depends on a 0.x crate under active development. Updates are pinned, and checked by the tests above and
  `ids.rs`; what zumen needs from it (optional content groups, limits) is proposed upstream.
- `AGENTS.md`, [the architecture notes](../architecture.md) (crates, Rust dependencies and their reasons) and the
  `THIRD_PARTY_LICENSES.txt` files change with the reader.
- The work is in milestone 7: the reader ([#90](https://github.com/zumenjs/zumen/issues/90)), drawing PDF underlays in
  `zumen-render` ([#91](https://github.com/zumenjs/zumen/issues/91)), and `@zumenjs/pdf`
  ([#83](https://github.com/zumenjs/zumen/issues/83)), which needs [#80](https://github.com/zumenjs/zumen/issues/80),
  [#81](https://github.com/zumenjs/zumen/issues/81) and [#90](https://github.com/zumenjs/zumen/issues/90).

## References

- ISO 32000-1:2008: 7.5.2 (file header), 7.7.3.3 (page objects: `CropBox`, `Rotate`, `UserUnit`), 8.5 (path construction
  and painting), 8.10 (form XObjects), 8.11 (optional content), 9.4 (text objects), 9.10 (extraction of text content),
  12.4.2 (page labels), 12.9 (measurement properties)
- [hayro](https://github.com/LaurenzV/hayro): README, `hayro-interpret` 0.8.0 (`Device`, `font::Glyph::as_unicode`,
  `examples/extract_html.rs`, `src/ocg.rs`), read on 2026-10-23
- [pdf.js](https://github.com/mozilla/pdf.js)
- [Decision 0002](0002-pdf-and-parapdf.md), [decision 0003](0003-dwg-in-rust.md), [Keeping the two implementations
  equal](../architecture.md#keeping-the-two-implementations-equal)
