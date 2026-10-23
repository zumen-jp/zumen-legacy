# How PDF is handled

`zumen-pdf` (`crates/zumen-pdf`) reads PDF drawings into a scene in Rust, with
[hayro-interpret](https://github.com/LaurenzV/hayro) 0.8.0 interpreting the pages
([decision 0020](../decisions/0020-pdf-drawings-in-rust.md)). It reads what interaction and analysis need: the pages as
views, the painted paths, glyph runs and images as entities, and the scales of the measured viewports. It does not draw:
each page is the underlay of its view, drawn by a PDF engine (pdf.js in browsers through `@zumenjs/pdf`), and every
entity has `drawn: false`. Native rendering of PDF underlays is tracked separately in
[issue #91](https://github.com/zumenjs/zumen/issues/91); a server renderer without a PDF engine reports the undrawn
underlay in its notices. Sections are those of ISO 32000-1:2008.

There is no TypeScript PDF reader. The `zumen` command, Python, the MCP server and the PDF module of `@zumenjs/reader`
(`bindings/wasm-pdf`, [decision 0030](../decisions/0030-pdf-in-a-webassembly-module-of-its-own.md)) read PDF with this
crate, so a PDF gives the same scene and ids everywhere. PDF is read only: `convert`, `edit` and `draft_into` refuse
it, and drawings are written as PDF by `render_pdf`.

```text
bytes ─▶ hayro-syntax (objects, streams, pages) ─▶ hayro-interpret (graphics and text state, fonts) ─▶ Device ─▶ Scene
```

Rules marked *inferred* below are not in the specification and were chosen for drawings.

## Detection, pages and views

- **Detection**: the header `%PDF-` in the first 1,024 bytes (7.5.2; Acrobat and pdf.js accept up to 1,024 bytes
  before it). `detectDrawing` of `@zumenjs/reader` repeats the rule.
- **Pages are views**: the id is the page number from 1 (`"1"`, `"2"` …), the name its page label (12.4.2: the
  numbering style `D`, `R`, `r`, `A` or `a`, the prefix `P` and the start `St` of the range in the number tree
  `PageLabels`), or the number when the page has no label. The first page is the default view, and an unknown view is
  reported (`unknown-page`) and shows the first page. Each view reads only its page.
- **The sheet** is the visible region of the page, the intersection of the crop box and the media box (7.7.3.3,
  14.11.2), turned by `Rotate` (clockwise, in multiples of 90 degrees) and scaled by `UserUnit` (table 30). A region
  without area is A4, and a side shorter than 1 point is 1 point, as hayro does. Its size is named A0 to A4 portrait
  or landscape within 1 mm, else 自由.
- **Coordinates** are paper millimeters with the Y axis up and the origin at the lower left of the sheet as displayed:
  a point of default user space `(x, y)` is first moved to the crop box's lower left `(u, v) = (x − x0, y − y0)`, then
  turned (90°: `(v, w − u)`, 180°: `(w − u, h − v)`, 270°: `(h − v, u)`, where `w × h` is the unturned size), then
  scaled by `25.4 / 72 × UserUnit`. `Scene.unit` is `millimeter`, and the `raw` values of properties are these
  coordinates.
- **The underlay** of a view is `{ id: "page-<n>", kind: "pdf", page: n }` without `file` (the drawing's own bytes). Its
  content coordinates are the points (1/72 inch) of the page as displayed, turned and scaled by `UserUnit`, with the
  origin at the top left and Y down: those of pdf.js's `page.getViewport({ scale: 1 })`, which applies `UserUnit` too.
  Its `width` and `height` are that size, and its `matrix` `[25.4 / 72, 0, 0, −25.4 / 72, 0, height in mm]` maps them to
  paper millimeters. Its layer is the page's layer, and `Scene.bounds` includes it.
- **Layers**: one layer per page, `ページ` (id 0), until hayro-interpret tells which optional content group (8.11)
  painted content belongs to. hayro-interpret applies the default configuration (`/D`) itself, so content hidden by
  default is not read, and a file with optional content groups is reported (`optional-content`, the number of groups).

## Entities and ids

hayro-interpret calls the device for every painted path (`draw_path`, `draw_rect`), glyph run (`draw_glyph_run`, one
per `Tj`, `TJ`, `'` or `"`) and image (`draw_image`), in paint order, also inside form XObjects (8.10) and annotation
appearances (not read, see below). The device turns them into entities:

- **Ids** are `#n`, n counting the paints of the page from 1. hayro-interpret gives a path that is filled and stroked
  (`B`, `b`, text rendering mode 2) as a fill and then a stroke; a stroke right after a fill of the same path or glyph
  run in the same clip joins it, as one entity with one number (*inferred*). Paints that become no entity (shadings,
  pattern fills, content outside the clip, glyph runs of spaces only) still take their number, so that reading more of them later
  does not rename the entities after them. The numbering follows the calls of hayro-interpret, so hayro is pinned to
  one version (`hayro-interpret = "=0.8.0"`), and `crates/zumen/tests/ids.rs` checks the ids of `tests/fixtures/pdf`.
- **Paths** (8.5): the path in paper millimeters, curves flattened within 0.01 mm. A stroked path is a `line` (one
  subpath of one straight segment), a `polyline` (straight segments only) or a `spline` (with Bézier curves); a path
  that is only filled is a `solid`. Properties: `color` (the stroke, or the fill of a solid), `fillColor` (a path both
  filled and stroked), `lineWidth` (the line width times the mean scale of the transformation matrix, mm), `lineType`
  (the dash array in mm, when dashed), `start`, `end` and `length` (line), `vertexCount` and `length` (polyline),
  `length` (spline). A path of subpaths that are each a single segment (hatching lines) is one stroke of `segments`.
  hayro gives a rectangular path to `draw_rect` as a rectangle that starts at its lower left corner, so the points of
  a rectangle do not keep the order of the file.
- **Glyph runs** (9.4) are `text`. The text is the Unicode of each glyph as hayro-interpret gives it (9.10: `ToUnicode`,
  then the encoding and the glyph names for simple fonts, and for CID fonts that are not embedded and have no
  `ToUnicode`, the UCS-2 CMap of their character collection); a glyph without Unicode is U+FFFD, counted in
  `glyph-without-unicode`. A glyph that maps to
  several characters (a ligature) is one cell. Properties: `text`, `font` (the `BaseFont` without the subset tag, read
  as UTF-8 or else Shift_JIS; none for Type 3 fonts), `height` (the font size times the text and transformation
  matrices, mm: the em, which is what PDF declares), `position` (the start of the baseline), `rotation` (degrees) and
  `visibility` for text rendering mode 3 (invisible text, such as the OCR layer of a scan).
  - The text primitive's coordinates have the baseline as the x axis; the box spans the font's `Descent` to `Ascent`
    (`FontDescriptor`, of the descendant font for Type 0; −200 and 800 when absent or implausible, *inferred*). Each
    glyph's cell center is the middle of its advance and of that box, mapped through the glyph's own transform, so
    horizontal scaling (`Tz`), rise and slanted matrices keep their places. The advance is the font's width; for Type 3
    glyphs, the distance to the next glyph, or half an em for the last. `render_pdf` places glyphs by the same rule,
    which the round trip test relies on.
  - The font name and the em box come from the font dictionaries of the page's resources (and of the form XObjects and
    Type 3 fonts in them), matched to the glyphs by the key hayro-interpret derives from the dictionary's bytes.
- **Images** (8.9) are `image` entities with their frame: the unit square of image space through the transformation
  matrix (`position` is its lower left corner, `imageSize` the width and height in samples). The pixels are not read.
  Stencil masks are `image` with the source `image-mask`.
- **Clipping**: the device keeps the bounding box of the clip paths (and of the sheet). An entity whose bounding box
  does not meet it is not read (`clipped-out`), so that content outside a viewport's clip or outside the page is not
  selected. An entity that the clip cuts keeps its whole shape (*inferred*).
- **Colors** are those of hayro-interpret converted to sRGB (`#rrggbb`, with alpha `#rrggbbaa`).

## Not read, and reported

| Code | What |
| --- | --- |
| `shading-not-read` | Fills with a shading (`sh`, and paths painted with a shading pattern, 8.7.4.3) |
| `pattern-not-read` | Paths painted with a tiling pattern (8.7.3): the pattern cells are not read |
| `clipped-out` | Content outside the clip or the page |
| `glyph-without-unicode` | Glyphs whose Unicode is unknown, read as U+FFFD |
| `unsupported-font` | Glyphs of fonts that hayro-interpret cannot read (CID fonts with an encoding it does not support) |
| `annotation-not-read` | Annotations with an appearance stream that is not hidden (12.5). The underlay draws them (pdf.js and hayro draw appearances), but they belong to ParaPDF ([decision 0002](../decisions/0002-pdf-and-parapdf.md)) |
| `optional-content` | The file has optional content groups: the page is one layer, and content hidden by default is not read |
| `measure-not-read` | Viewports with a geospatial measure dictionary (`GEO`) |
| `measure-unit-unknown` | Viewports whose number format names a unit that zumen does not know |
| `unknown-page` | The view asked for is not a page; the first page is shown |

## Scales: viewports and measure dictionaries

A page's `VP` array (table 30) lists viewports, each a rectangle of the page (`BBox`) with a measure dictionary
(12.9.2, table 261). For a rectilinear one (`Subtype /RL`), the first number format of `X` (table 262) gives the factor
`C` from a unit of default user space to its unit `U`, and `Y`, when present, the same for the y axis. The placement's
scale is `25.4 / 72 × UserUnit / (C × U in mm)`, the paper millimeters of a real millimeter, and its inverse maps paper
millimeters to real millimeters from the origin `O` (default user space; the lower left corner of `BBox` when absent),
in the directions of the sheet as displayed (*inferred* for turned pages). `U` is a text for people: `mm`, `cm`, `m`,
`km`, `in`, `ft`, `yd`, `mi` and `pt`, with their English and Japanese names, are read. The placement's name is the
viewport's `Name` (or `ビューポート n`) with the ratio `R` (or `1/N`).

Entities whose bounding box lies inside a viewport get its placement, so `measure_distance`, the geometry for AI clients
and the analyses see real-world sizes; where viewports overlap, the last one in the array wins (12.9.2). A page
without viewports has no placement, and the application calibrates with two points.

## Limits and safety

- `limits::tick` is called for every path, glyph run and image, and for the nodes of the page label tree and the
  resources walked for font names; `limits::admit` for every entity. `max_bytes` bounds the input (`zumen::read`).
- Work inside hayro between two calls (decoding a stream, parsing a font) is not interrupted by the limits, and
  decompressed streams are not bounded by `max_bytes`. The fuzz example (`crates/zumen-pdf/examples/fuzz.rs`) damages
  PDFs and reports panics and slow reads, to be taken to hayro.
- Encrypted PDFs are read when the empty user password opens them; others are refused with a message.

## Tests

- `crates/zumen-pdf/tests/pdf.rs` writes the smallest PDFs that pin each rule with `pdf-writer` (detection, views and
  labels, the transformation matrix, path kinds and joined fill and stroke, the text matrix and font size, Type 0 fonts
  with `ToUnicode`, Type 3 glyphs without Unicode, `Rotate` and the crop box, `UserUnit`, form XObjects, images, the
  content not read, viewports with measure dictionaries, limits), and `tests/fixtures/pdf/drawing.pdf`.
- `crates/zumen/tests/pdf.rs` draws every fixture of the other formats with `render_pdf` and reads it back: the lines,
  fills and glyph centers are the ones drawn.
- `packages/reader/tests/golden.test.ts` writes the scenes of `tests/fixtures/pdf` with the PDF module, and
  `crates/zumen/tests/golden.rs` checks that the native reader returns the same.
- `pnpm run parity` compares the texts of PDF files with pdf.js's `getTextContent` page by page and reports the
  differences without failing ([Verification records](../verification.md#pdf-in-rust)).
