# Changelog

All packages share one version during 0.x.

## Unreleased

- Rust, Python, the command line and the MCP server: drawing markup on servers
  ([decision 0021](docs/decisions/0021-drawing-markup.md), [Drawing markup](docs/markup.md)). `zumen_core::markup`
  reads and writes the documents with the same checks and JSON as `@zumenjs/core` (unknown keys kept) and gives the
  same shapes, checked against `tests/fixtures/markup`; `drawing_hash` uses the `sha2` crate. `zumen::carry_markup`
  carries a document to a new revision view by view through `compare_scenes`: items move by the offset, `entities`
  point at the matching entities, and `carried` marks them `unchanged`, `changed` or `removed`, with a map of views
  (a DXF layout to a PDF page). `zumen_render::RenderOptions::markup` draws items over PNG, SVG and PDF. Python has
  `read_markup` (with `TypedDict` classes), `markup_schema`, `drawing_hash`, `carry_markup`, `MarkupError`,
  `CarryError` and `markup=` on the `render_*` methods; the command line `zumen markup carry` and
  `zumen render --markup`; `zumen.tools` `list_markup`, `write_markup` and `carry_markup_revision`; and the MCP server
  `markup` on `render_drawing`, `read_markup`, `write_markup` and `carry_markup`.
- Browsers, Rust, Python and the command line: files that start with `ISO-10303-21` but whose `FILE_SCHEMA` does not
  begin with SXF's schema `ASSOCIATIVE_DRAUGHTING`, such as IFC (`IFC2X3`, `IFC4`, `IFC4X3_ADD2`) and STEP AP203, AP214
  and AP242 files, are no longer detected as SXF P21. Reading them gives the error for a format no reader knows instead
  of an empty drawing with the `no-sheet` notice. SXF files without `FILE_SCHEMA` are still detected
  ([How SXF is handled](docs/formats/sxf.md#reading)).
- Rust, Python and the command line: SXF user-defined line types written by `edit` and by converting to SFC hold at
  most 8 dash and gap lengths, as SXF holds 2, 4, 6 or 8 (the rule ezsxf checks). The dashes of a hatch pattern line
  keep the first 8, unless they are the lengths of a predefined line type, and the base point moves by whole periods
  of those (`edit-hatch-dash-count`); a longer DXF or DWG line type converted to SFC keeps the first 8
  (`linetype-long`). `zumen_core::edit::style::cut_pitches` cuts lengths by pairs for `line_type_pitches` and the SXF
  hatches.
- Browsers: drawing markup, kept in a JSON document beside the drawing in paper millimeters of a view
  ([decision 0021](docs/decisions/0021-drawing-markup.md), [Drawing markup](docs/markup.md)). `@zumenjs/core` has the
  model (`MarkupDocument`, `MarkupItem` and its kinds `redline`, `cloud`, `text`, `finding` and `photo`),
  `createMarkup`, `markupFromJson` (checked, keeping unknown keys, throwing `MarkupError` with a code, the item and the
  key), `markupToJson`, `markupShapes` and `markupBox`, and `drawingHash` (SHA-256 with Web Crypto). `@zumenjs/viewer`
  has `MarkupLayer` (a store with a `change` event, undo and redo, a filter and `itemsAt`) built on the public API of
  `DrawingViewer`, the tools `redlineTool`, `cloudTool`, `textTool`, `findingTool`, `photoTool` and `markupEditTool`,
  and colors from `--zumen-markup-*`; overlay painters receive the canvas size, `renderPrimitives` draws primitives in
  their own colors, and a tool with `doubleClickZoom: false` receives double clicks. `@zumenjs/react` has
  `useMarkupLayer`, `useMarkupState` and `useMarkupEvent`, and the playground a markup sample.
- Browsers: a new package, `@zumenjs/pdf`, draws PDF pages under the drawing with pdf.js. `pdfUnderlayRenderer` is an
  `UnderlayRenderer` for underlays of kind `pdf` (the pages of PDF drawings, and the PDF underlays of DXF drawings):
  it renders the requested region of a page at the requested scale, cancels the pdf.js render when the request is
  aborted, and turns optional content groups on and off as content layers are shown and hidden (`optionalContent`).
  The application opens the documents with its own pdf.js (`pdfjs-dist` 5 or 6, a peer dependency) and passes them in
  ([decision 0029](docs/decisions/0029-pdf-js-owned-by-the-application.md)). It reads no entities; those come from
  the reader.
- Browsers, Rust and Python: SXF raster images and JWW images draw as underlays of `Scene.underlays`. SXF images are
  the drawing groups named `$$ATRU$$…$$画像$$<file>` or `$$RASTER$$<file>`, drawn over the polyline of the group;
  JWW images are the `^@BM<file>,<width>,<height>` texts, turned with the text, and keep an entity of the kind `image`
  for selection only (`drawn: false`). Neither format records the pixel count, so their content is measured in
  hundredths of a sheet millimeter ([decision 0031](docs/decisions/0031-image-units-without-a-pixel-count.md)).
  SXF images placed over earlier entities, image groups without a polyline, JWW trimming values and unreadable
  placements are reported (`raster-order`, `raster`, `image-trim`, `image`).
- Browsers, Rust and Python: DXF and DWG images (IMAGE) draw their picture as an underlay of `Scene.underlays`,
  named by the file of their IMAGEDEF, placed by their pixel vectors and clipped by their boundary (also inverted),
  by viewports and with the fade as opacity ([docs/formats/dxf.md](docs/formats/dxf.md#images)). The application
  loads the files (`imageUnderlayRenderer`). Images that the drawing hides, unloads or does not define keep their
  frame only; `IMAGEFRAME` 0 keeps the frame for selection only (`drawn: false`); brightness and contrast and
  images drawn over earlier entities are reported (`image-adjust`, `image-order`). The model keeps the brightness,
  contrast, fade, clip mode and loaded state of images, which the DXF and DWG writers write (versions before 2010
  report an inverted clip, `version-image-clip`), and the image frame setting, which they report (`image-frame`).
  PDF, DWF and DGN underlays are still not read: the drawing does not hold the page size. The playground draws the
  images of a drawing from image files chosen next to it.
- Rust, Python and the command line: `edit` and `create` take `add-block` (with `"container": "block:<name>"` and
  `block-reference`), `"kind": "dimension"` and `"kind": "hatch"` for JWW and SXF too, as each format's own entities
  ([decision 0025](docs/decisions/0025-annotations-in-jww-and-sxf-edits.md)). JWW: a block definition named with
  Jw_cad's drawing part suffix, dimensions as one dimension figure (CDataSunpou) in SXF mode drawn with the drawing's
  dimension settings and text style (linear, aligned, radius and diameter; angular dimensions are refused), and hatches
  as trapezoid solids (`SOLID`) or hatch lines and points cut to the boundary (patterns). SXF: a drawing part
  (composite figure of kind 4), the dimension features (linear, angular, radius and diameter, with ISO-25 sizes on the
  sheet, divided by the placement's scale inside sub-figures), and fills and user-defined hatching over invisible
  composite curves, with pattern dashes as a user-defined line type. The reading of dimension and hatch values and the
  built-in hatch patterns move to `zumen_core::edit::dimension` and `zumen_core::edit::hatch` (re-exported by
  zumen-dxf; `HatchInput::pattern_type` becomes `custom`, and a dimension's DXF flags and display block are the
  `DxfDimension` trait), and `JwwDimension` keeps its SXF mode (`sxf_mode`).
- Rust, Python and the command line: JWW and SXF edits take `add-dimension-style`, `add-text-style` and
  `add-line-type` too ([decision 0026](docs/decisions/0026-styles-in-jww-and-sxf-edits.md)). Neither format has named
  dimension or text styles, so their names last for the edit: a dimension style sizes and formats each dimension that
  names it (the value's height and font, its gap from the line, the extension lines, decimals, trailing zeros and the
  linear factor; in SXF also the arrows, or slashes for `tickSize`), with lengths in paper millimeters or, with
  `scale`, times `scale` in the drawing's own coordinates, as DIMSCALE in DXF. A text style is a font (an SXF font,
  a JWW font name; JWW `text` gains `textStyle`), and a line type becomes an SXF user-defined line type or a free
  JWW SXF line type (47–62), with its pattern in paper millimeters. What a format cannot hold is written the nearest
  way and reported (`edit-dimension-decimals`, `edit-dimension-text-position`, `edit-dimension-end`,
  `edit-style-values`, `edit-line-type-pattern`). Every editor reads the values of the three ops with
  `zumen_core::edit::style`, which also holds the conversion of dashes to SXF line lengths; DXF, DWG and SXF hatch
  output is unchanged.
- Browsers, Rust and Python: entities can be kept for interaction only, with `drawn: false` (`SceneEntity.drawn` in
  `zumen-core`), for the content that an underlay already shows such as the paths and texts of a PDF page
  ([decision 0022](docs/decisions/0022-underlays-drawn-by-a-region-renderer.md)). `renderScene` and `zumen-render`
  (PNG, SVG, PDF and `drawn_entities`) skip them; hit testing, snapping, measurement, `texts`, `entities`, regions,
  tables, checks and comparison use them, the viewer's selection and highlights draw them, and `diff_scene` draws
  them. No reader produces them yet.
- Browsers, Rust and Python: scenes describe raster content drawn under the entities, `Scene.underlays` (images and
  PDF pages: where they sit in paper millimeters, clip rings, opacity and layers, and the file and page they come
  from), with the same JSON in `@zumenjs/core` and `zumen-core`
  ([decision 0022](docs/decisions/0022-underlays-drawn-by-a-region-renderer.md)). `DrawingViewer` draws them through
  an `UnderlayRenderer` that the application passes in (`underlayRenderer`, `setUnderlayRenderer`,
  `refreshUnderlays`, the `underlay` event): it asks for the whole content coarsely and for the visible region sharply
  once the view settles, and aborts the requests the view no longer needs. `imageUnderlayRenderer` draws images, and
  `renderScene` draws rendered regions passed in `underlays`. `zumen-render` does not draw underlays and reports them
  as `underlay-not-drawn` in the new `RenderReport.notices` (printed by the command line, a warning in Python);
  `render_svg` returns the report too. No reader produces underlays yet.
- Fix ten issues found in the library review: keep block attributes with moved, rotated and scaled references;
  resolve escaped Unicode names when editing older DXF files; preserve literal multiline drafting text; reject
  empty or reserved sheet names; apply explicit paper sizes to existing template layouts; keep narrow ruled-table
  cells valid; check dimension text heights independently of other texts; clear React drawing state when the file
  and view are reset together; keep the pinch anchor in place; and draw visible thick strokes at the screen edge.
  Text primitives carry an optional declared `characterHeight`, and DXF/DWG editing adds `update-layout` with
  `set: {"paperSize": [width, height]}` ([decision 0023](docs/decisions/0023-resize-existing-layouts.md),
  [decision 0024](docs/decisions/0024-declared-text-height.md)).
- Rust, Python, the command line and the MCP server: check a drawing against the rules of a CAD standard (layer
  names, colors, line types, line widths, text heights on the sheet, machine-dependent characters, the sheet size and
  orientation, and scales), with the CAD drafting standard of MLIT (CAD製図基準, December 2025) built in as
  `mlit-cad-2025` and rule sets of one's own as JSON ([Checks](docs/checks.md),
  [decision 0015](docs/decisions/0015-analysis-in-zumen-core.md)). Each rule reports what it checked, what the drawing
  does not give, and the values that break it with the ids and layers of their entities. `zumen_core::checks`,
  `Scene.check` and `zumen.tools.check_drawing` in Python, `zumen check` (which fails when a rule of level error is
  broken), and the `check_drawing` MCP tool. The ratio between the text height of DXF and DWG and the glyphs drawn
  moves to `zumen_core::text::em_for`.
- Rust, Python, the command line and the MCP server: read the tables drawn with rules (schedules, area tables,
  drawing lists, title blocks) as rows and columns, with merged cells and the texts of each cell
  ([decision 0015](docs/decisions/0015-analysis-in-zumen-core.md)). Only horizontal and vertical lines are rules, so
  a slash across an empty cell does not split it, and the drawing area beside a title block is left out.
  `zumen_core::tables::ruled_tables`, `Scene.tables` and `zumen.tools.find_tables` in Python, `zumen tables`
  (JSON lines, or CSV with `--csv`), and the `find_tables` MCP tool.
- Rust, Python, the command line and the MCP server: draft whole drawings from a JSON drawing spec
  ([decision 0018](docs/decisions/0018-drafting-kit.md), [Drafting](docs/drafting.md)). Sheets become layouts with
  a border and a title block (a strip on the right or a table at the bottom right), views are viewports at their
  scales onto real-size model space, and items include text (with alignment, wrapping and fitting), linear and aligned
  dimensions and dimension chains, hatches (solid, built-in patterns or pattern lines), tables with merged cells,
  blocks with attributes, grid lines with bubbles and dimensions, north arrows, scale bars, level marks, spot levels,
  leaders, and side lengths and areas of a boundary. Sizes are paper millimeters at any scale. The drawing is written
  as DXF R12 to 2018, DWG, JWW or SXF; a mistake in the spec names its place and a stable code.
  `zumen::draft`, the new crate `zumen-draft` with its JSON Schema, `zumen.draft` and `zumen.draft_schema` in
  Python, `zumen draft`, and the `draft_drawing` MCP tool.
- Rust, Python, the command line and the MCP server: draw a drawing spec into a DXF or DWG template in place
  (`zumen::draft_into`, `zumen.draft_into`, `zumen draft --into template.dwg`, `into` of `draft_drawing`). The drawing
  becomes edit operations of decision 0017 (`zumen_draft::plan`), so the template's entities, handles and data stay;
  sheets named after its layouts are drawn into them, and the new `fields` of a sheet fill the attributes of its
  title block ([Drawing into a template](docs/drafting.md#drawing-into-a-template)).
- Rust, Python and the command line: `edit` and `create` take `{"op": "add-block", "name": ..., "basePoint": [x, y]}`
  for DXF (R12 to 2018) and DWG (R13 to 2018), which creates an empty block definition; later operations of the same
  edit fill it with `"container": "block:<name>"` and place it with `block-reference` (JWW and SXF too, see above).
  First step of [decision 0017](docs/decisions/0017-add-annotations-and-definitions.md).
- Rust, Python and the command line: `add` takes `kind: "dimension"` (linear and aligned) for DXF and DWG, written as
  a DIMENSION with its `*D` block computed from the dimension style, so that zumen and other readers draw it.
- Rust, Python and the command line: `kind: "dimension"` also takes `dimensionType` `angular-3-point` (`center`,
  `start`, `end`, `position`), `radius` and `diameter` (`center`, `point`) in DXF (R12 to 2018) and DWG (R13 to 2018),
  with the arc, arrowheads and the `°`, `R` or `Ø` text in their blocks.
- Rust, Python, the command line and the MCP server: the drawing spec's `dimension` takes `kind` `angular` (`center`,
  `from`, `to`, and `radius` or `at`), `radius` and `diameter` (`center`, `radius`, `angle`), drawn in every format and
  through `draft_into` ([Drafting](docs/drafting.md)).
- Rust, Python and the command line: `add-text-style`, `add-line-type` and `add-dimension-style` for DXF and DWG, so
  that a new drawing gets a Japanese text style and dimensions sized for its units and scale.
- Rust, Python and the command line: `add` takes `kind: "hatch"` for DXF (R13 to 2018) and DWG (R14
  to 2018): closed polyline boundaries with holes, `SOLID`, built-in patterns (`ANSI31`, `ANSI32`, `ANSI37`, `LINE`,
  `NET`, `DOTS`) or the lines of a PAT pattern.
- Rust, Python and the command line: `text` takes `justification` (`top-left` … `bottom-right`, and `left`,
  `center`, `right` on the baseline), `lineSpacing`, `multiline` and `widthFactor` in DXF and DWG, for text placed in
  table cells.
- Rust, Python and the command line: block attributes in DXF and DWG: `kind: "attribute-definition"` in a block and
  `attributes: {"TAG": "value"}` on `block-reference`, which writes the ATTRIBs, for title blocks.
- Rust, Python and the command line: sheets in DXF and DWG (AutoCAD 2000 and later): `add-layout` with a paper size
  and `kind: "viewport"` in it, showing model space at a scale such as 1:100. `margins` sets the printer margins.
- Rust, Python and the command line: `kind: "leader"` in DXF and DWG, a LEADER with an MTEXT note sized from the
  dimension style.
- Rust, Python and the command line: `kind: "image"` in DXF and DWG (R14 and later), an IMAGE with its IMAGEDEF in
  `ACAD_IMAGE_DICT` that keeps `path`, for raster location maps. Reading keeps the image's file name
  (`DxfImage::path`), and converting to DXF and DWG now writes IMAGE with its IMAGEDEF instead of leaving it out
  (`version-image` in R12 DXF and R13 DWG; `image` only when the image definition is unknown).
- Readers: DXF and DWG report the block references to external drawings (xrefs, blocks whose flags have 4 or 8) in
  the new `xref` notice, with their number and the paths they refer to, instead of drawing nothing without a word. The
  drawings they refer to are still not read ([#108](https://github.com/zumenjs/zumen/issues/108)).

## 0.1.1 — 2026-10-21

- Rust: `zumen_dwg::write_dwg_as` writes AutoCAD R13, R14, 2000, 2004, 2007, 2010, 2013 or 2018 DWG, with
  notices for values an older version cannot hold. `write_dwg` and facade conversion still default to 2013.
  R13 and R14 drawings can also be rewritten and edited in their own version, including through Rust and Python
  `edit`. See [DWG writing](docs/formats/dwg.md).
- Rust, Python, the command line and the MCP server: find the regions that drawn lines close, such as rooms, with
  their areas ([decision 0015](docs/decisions/0015-analysis-in-zumen-core.md)). Lines are split where they cross or
  meet and joined where their ends are within a tolerance, lines that end nowhere close nothing, and closed shapes
  inside that touch no other line are holes. Each region has its outline, holes, area and perimeter on the sheet and,
  inside a placement, in real-world millimeters, the entities that bound it and the texts inside.
  `zumen_core::regions::closed_regions`, `Scene.regions` and `zumen.tools.find_regions` in Python (areas in square
  meters, filters by layer, text and area), `zumen regions` and the `find_regions` MCP tool.
- Reader: `ReaderWorker` gets the scene from the worker as JSON and parses it on the page, instead of structured
  cloning the scene with its point arrays. A 16.6 MB SFC with 227,139 entities now reads in 2.9 s instead of 14.5 s in
  Chromium (0.9 s of it on the page).

- Rust, Python and the command line: `zumen::convert` writes every format as DWG (AutoCAD 2013), JWW and SXF (SFC,
  P21, SFZ and P2Z, with `version="2.0"`, `"3.0"` or `"3.1"`) besides DXF, and `zumen::edit` and `zumen::create`
  handle DWG, JWW and SXF besides DXF. JWC and JWS drawings are read and converted like JWW. New JWW and SXF drawings
  are in millimeters, so `create` takes only `units="mm"` for them.
- Viewer: rotate the drawing on screen. `rotateBy(degrees)` turns it clockwise around the center of the view and refits
  a fitted box, `jumpTo({ rotation })` sets the angle and `getView().rotation` reads it. `toPaper`, `toScreen`, hit
  testing, snapping, measurement, dragging and pinching follow the rotation, `View` (of overlay painters) gains
  `rotation`, and `bounds` is the paper box around the visible area. The playground sample has buttons to turn the
  drawing by 90 degrees.

## 0.1.0 — 2026-10-20

- First public release under the MIT license, with third-party license notices, four npm packages
  (`@zumenjs/core`, `@zumenjs/viewer`, `@zumenjs/reader`, `@zumenjs/react`) and Python wheels and an sdist.

- `@zumenjs/reader` replaces `@zumenjs/dwg`: every format (SFC, P21, P2Z, SFZ, JWW, DXF and DWG) is read in browsers
  and Node.js by the Rust readers built to WebAssembly. `initReader` loads the module, `reader` is the
  `DrawingReader` for `readDrawing` and `useDrawing`, `detectDrawing` tells the format before the module loads, and
  `createReader({ limits })` and `readScene` stop at `maxBytes` and `maxEntities` with a `LimitError` that names the
  limit. `ReaderWorker` reads in a Web Worker (`@zumenjs/reader/worker`), stops it when `limits.timeout` passes or an
  `AbortSignal` fires, and transfers the scene's point lists to the page. The playground reads with it. The TypeScript
  readers `@zumenjs/sxf`, `@zumenjs/jww` and `@zumenjs/dxf` stay as the reference of the tests and will not be
  published.
- Rust, Python and the command line: edit drawings in place and create new ones
  ([decision 0012](docs/decisions/0012-edit-in-place-and-write-every-format.md)). The operations are the same for every
  format: `add`, `update` and `delete` entities, and `add-layer` and `update-layer`, with the kinds, ids and property
  keys of the scene and coordinates in the drawing's own units (`zumen_core::edit`). An edit keeps the format, version,
  text encoding and everything the operations do not name, and fails as a whole with a code and the number of the
  operation. DXF is done (R12 to 2018, ASCII and binary): `zumen_dxf::edit_dxf` keeps every tag that no operation
  changes byte for byte, so extended data, objects and unread entities stay. `zumen::edit` and `zumen::create`,
  `zumen.edit`, `zumen.create`, `Edited` and `EditError` in Python, and `zumen edit` and `zumen create`.
- Rust, Python and the command line: write DXF R12 (AC1009), with `--target-version R12` or `version="R12"`. As
  ezdxf's `r12export` does, what R12 lacks is reduced to R12 entities and reported in the notices: LWPOLYLINE becomes
  POLYLINE, and ellipses, splines, MTEXT, hatches, leaders, multileaders and multilines become the lines, solids,
  one-line texts and points that zumen draws for them (in an anonymous block when there are several). Only the current
  layout is kept, and R12 has no length unit, line weights or TrueType font names.
- DXF (TypeScript and Rust): the layers frozen in an R12 viewport (their names in the `MVIEW` extended data) are not
  drawn in it.
- Rust, Python and the command line: write DXF in a chosen version, AutoCAD 2000, 2004, 2007, 2010, 2013 or 2018
  (`zumen_dxf::write_dxf_as`, `zumen::convert_with` with `WriteOptions`, `zumen.convert(..., version=...)`,
  `zumen convert --target-version`). Versions before 2007 are written in the drawing's code page, or in the one that
  holds most of its texts, with `\U+XXXX` for what it cannot hold. True colors, transparency, gradients, MTEXT
  backgrounds and the newer dimension types that a version lacks are written in another form and reported in the
  notices.
- ST-Bridge left zumen: `@zumenjs/stb` and its notes moved to a private repository, because publishing an
  implementation of ST-Bridge needs a license from buildingSMART Japan. zumen keeps to formats that need no third
  party's permission and to dependencies with permissive licenses
  ([decision 0013](docs/decisions/0013-st-bridge-out-of-zumen.md)). The playground opens drawings only.

- Python: `zumen` requires Python 3.12 or newer (`requires-python`, and `abi3-py312` for the wheels). The types of
  `zumen.tools` are `typing.TypedDict`, which pydantic, and so the agent frameworks that build schemas with it, accepts
  only from 3.12; 3.10 and 3.11 are not supported.
- Python: wheels and an sdist of `zumen` from `.github/workflows/python-wheels.yml`, so that an application installs it
  without a Rust toolchain. One abi3 wheel serves every Python from 3.12, for manylinux_2_28 (x86_64 and aarch64) and
  macOS (arm64). Pull requests and `main` upload them as workflow artifacts after the tests of `bindings/python` ran
  against each installed wheel on Python 3.12 and 3.14, and a pushed tag `v*` attaches them to a GitHub Release
  ([docs/releasing.md](docs/releasing.md)). PyPI uploads use a trusted publisher and an explicit repository gate. The wheels hold the
  license text of the bundled BIZ UDGothic (SIL OFL 1.1) in `dist-info/licenses`, which they lacked.
- The length unit of a drawing: `Scene.unit`, one of `LENGTH_UNITS` (`LengthUnit` in Rust), is the unit of the
  drawing's own coordinates (the `raw` points and lengths of properties): `millimeter` in SXF and JWW, and in DXF and
  DWG the unit of `$INSUNITS` (`inch`, `meter`, …), or the one assumed with the `unit-assumed` notice. `Scene.unit` in
  Python, and `unit` in `zumen.tools.summary`.
- Entity ids are a contract: the same bytes give the same ids in every version
  ([Entity ids](docs/architecture.md#entity-ids)), and `crates/zumen/tests/ids.rs` checks them on every fixture. DXF
  entities without a handle (files written without handles, such as many R12 files) all had the id `?`; they are now
  `#n`, the number of their record in the file, unique like a handle.
- Python: `zumen.tools`, the functions behind the MCP tools for applications that give an AI agent tools of their
  own (`summary`, `render`, `find_texts`, `find_entities`, `get_entity`, `measure`, `compare_revisions`, `Drawings`).
  Their results are JSON with keys described by `TypedDict` classes that are part of the API, bounded by `limit`,
  `max_points`, `max_text_length` (strings from the drawing, with `truncated`) and the image size, and they take
  regions in paper millimeters or as fractions of the frame (`frame_region`, the scene's bounds with the origin at the
  top left). `zumen.mcp` wraps them, and its tools gain `frame_region`
  ([decision 0011](docs/decisions/0011-tool-functions-for-agents.md)).
- Rust, Python and the command line: write every format as SXF (SFC of SXF Ver.3.1) through the AutoCAD drawing model
  ([decision 0010](docs/decisions/0010-write-sxf-from-the-autocad-model.md)). `zumen::convert(bytes, "sfc", …)`,
  `zumen.convert(source, "sfc")` and `zumen convert plan.dwg -o plan.sfc`. The layout that the drawing opens in is the
  sheet, each viewport a sub-figure at real size placed at its scale and twist, and each block a drawing part (one for
  each set of BYBLOCK values it inherits). Entities that SXF has no feature for (multiline texts, attributes, leaders,
  dimensions, solids) are written as the lines, fills and texts that zumen draws for them, and hatch patterns as SXF
  hatching. `zumen_convert::dxf_to_sxf` maps the model, and `zumen_sxf::write_sfc` writes any SXF drawing as SFC.
- Rust, Python and MCP: the shapes that an entity draws, for AI clients and data pipelines to compute with
  ([decision 0009](docs/decisions/0009-geometry-for-ai-clients.md)). `zumen_core::extract::entity_shapes` gives
  polylines (curves as drawn), segments, fills, hatched areas, points and text parts in paper millimeters, and
  `real_shapes` the same through the entity's placement, in the coordinates of the drawing itself (real-world
  millimeters). `Scene.entity(id, geometry=True)` and `Scene.entities(..., geometry=True)` in Python, and
  `zumen entities --geometry`. In the MCP server, `get_entity` returns the geometry, `find_entities` returns it with
  `geometry` (pages bounded by `max_points`, with `next_offset`), and `open_drawing` lists the placements drawn at a
  scale with their box and the conversion from paper to real-world millimeters.
- Stable keys in the scene model, the same in TypeScript, Rust and every format: `SceneEntity.kind` (`line`, `arc`,
  `text`, `dimension`, … listed in `ENTITY_KINDS`) next to the Japanese `type`, and `Property.key` (`layer`, `color`,
  `start`, `radius`, `dimensionType`, …) next to the Japanese `label`, with the unformatted value in `Property.raw`
  (numbers, `[x, y]`, format codes). The title block and the comparison read properties by key. Python entities have
  `kind` and `values` (key to value), scenes `metadata_values`, the Arrow table a `kind` column and property keys, and
  the MCP server returns kinds and keys and finds entities by kind, and the JSON lines of `zumen entities` have `kind`
  and `values`. The kinds and keys are listed in
  [the architecture notes](docs/architecture.md#stable-keys).
- Rust and Python: compare two revisions of a drawing. `zumen_core::compare` matches entities by shape and text, not
  by id, so revisions saved again or in another format compare too. It reports the removed, added and changed entities
  and the regions that changed, aligns a revision that moved as a whole by its texts, and draws the result as a scene
  (red before, blue after, gray unchanged). `zumen.compare` and `Comparison` in Python, `zumen compare` in the command
  line, and `compare_drawings` in the MCP server.
- Rust and Python: the fields of the title block (drawing number, revision, title, project, scale and date), guessed
  from attribute tags (`DWGNO`, `REV`), from labels such as 図面番号 or 図　面　名 and the text to their right or
  below, from a label and value in one text (`縮尺 1/100`), from the sheet name and from the metadata (the title
  block attributes of SXF). `zumen_core::title`, `Scene.title_block` in Python, `zumen title`, and `open_drawing` in the
  MCP server.
- Rust and Python: SVG and PDF output of a scene, the sheet (or a region) at its real size, with the colors, line
  weights, fills, clipping and text placement of the PNG renderer. PDF embeds the glyphs of the bundled BIZ UDGothic
  that the page uses, so its text can be searched and copied, and SVG keeps text as `<text>`.
  `zumen_render::render_svg` and `render_pdf`, `Scene.render_svg` and `Scene.render_pdf` in Python, and
  `zumen render -o plan.svg` or `plan.pdf`.
- Rust and Python: limits for drawings received from others. `ReadDrawingOptions::limits` (`max_bytes`,
  `max_entities`, `max_duration`) stops reading part way and returns an error that names the limit
  (`ReadError::exceeded`; `zumen.LimitError` with `limit` in Python). The readers count the entities they add and check
  the clock in their heavy loops, and do nothing without limits, so results stay those of TypeScript.
  `zumen.read(..., max_bytes=, max_entities=, timeout=)`, `--max-bytes`, `--max-entities` and `--timeout` on every
  command that reads, and `zumen-mcp` now stops at 2,000,000 entities or 60 seconds by default.
- Rust: compressed SXF (P2Z and SFZ, a ZIP archive with one P21 or SFC file) is unpacked and read, up to `max_bytes`
  after unpacking, or 2 GiB without it, against ZIP bombs. The scene is that of the P21 or SFC, with the archive in
  its metadata.
- Rust and Python: convert SXF (SFC and P21, and compressed P2Z and SFZ) and JWW drawings to DXF through the AutoCAD drawing model. The new crate
  `zumen-convert` maps them into a `DxfDocument` (`sxf_to_dxf`, `jww_to_dxf`, re-exported as `zumen::mapping`), and
  `zumen::convert`, `zumen convert <file> -o <out.dxf>` and `zumen.convert` in Python write it
  ([decisions 0007](docs/decisions/0007-mapping-crate-between-formats.md) and
  [0008](docs/decisions/0008-sheets-and-scales-in-the-autocad-model.md)). Model space is at real size: each SXF
  sub-figure and each JWW layer group scale is shown through a viewport of the sheet's layout at its scale and
  rotation, and what is drawn on the sheet itself is in paper space; the DXF opens in that layout. Layers (with the
  states of JWW layer groups), colors (ACI where the color is one, true color otherwise), line types, line weights,
  fonts and vertical text, arcs, ellipses and splines, hatches and fills, blocks (SXF drawing parts and groups, JWW
  blocks) and dimensions (DIMENSION with their drawn block) become their AutoCAD counterparts. What cannot be mapped as
  it was (character spacing, clothoids as polylines, leaders as separate lines, arrows and text, pattern hatches as
  their outline, title blocks, temporary points …) is reported in `notices`. The layout of the written DXF draws the
  same scene as the source, entity by entity, on the fixtures and local samples, and ezdxf reads and audits the files.
- `@zumenjs/dxf` and `zumen-dxf`: TEXT and MTEXT in a vertical font (a TrueType font whose name starts with `@`, such as
  `@ＭＳ 明朝`) are drawn as a column of upright characters, as AutoCAD draws the vertical Japanese text that it and
  Jw_cad write. New fixture `tests/fixtures/dxf/vertical-text.dxf`.
- `zumen-sxf` and `zumen-jww`: the code tables and the rules that turn codes into colors, line types, widths, layer
  names and text layout are public (`SxfDocument::color`, `dashes`, `width`, `font`, `zumen_sxf::tables`,
  `zumen_sxf::parse_sxf`; `JwwDocument::pen_color`, `line_dashes`, `line_width`, `group_scale`, `layer_name`,
  `zumen_jww::paper_size`, `text_layout_input`), and the scenes use them. The scenes do not change.
- Rust and Python: write DXF and DWG drawings as DXF of AutoCAD 2013 in UTF-8 through the AutoCAD drawing model
  (`zumen::convert` and `zumen_dxf::write_dxf`, `zumen convert <file> -o <out.dxf>`, `zumen.convert` and `Converted` in
  Python; [decision 0006](docs/decisions/0006-write-through-the-autocad-model.md)). Handles, tables, blocks, layouts and
  every entity that the model holds are written, including MULTILEADER, WIPEOUT and gradients, so the written file
  draws the same scenes with the same ids. What cannot be written as it was (IMAGE, HELIX as SPLINE, MPOLYGON as HATCH,
  ACAD_TABLE as the INSERT of its block, line types with texts or shapes, dimensions without their block) is reported
  in `notices`, and missing definitions are added. ezdxf reads the written files in the Python tests.
- `zumen-dxf` and `zumen-dwg`: hatches keep the angle, shift, one-color flag and tint of their gradient.
- `@zumenjs/viewer`: the 2D engine that applications embed in their own interface
  ([decision 0005](docs/decisions/0005-browser-packages-for-embedding.md)). The Canvas renderer, hit testing and
  `DrawingViewer` move here from `@zumenjs/core`. `DrawingViewer` sets and reads the view (`jumpTo`, `fitBounds`,
  `zoomBy`, `panBy`, `getView`) and sends `move`, `moveend` and `cursor` events; its state holds several selected
  entities, the hovered entity, named highlights, the active tool and the measurement; dragging, the wheel (always,
  with Ctrl or ⌘ only, or never), pinching, double click and keys can each be turned off; tools (`select`, `measure`,
  `pan` and the application's own) receive pointer events in paper millimeters and draw on the overlay; colors come
  from `--zumen-*` CSS custom properties and `setStyle`; and `entitiesAt`, `entitiesIn` and `snapAt` query the drawing.
  It has no text, so the canvas gets no default label. Drawings look and respond as before: in the playground, the
  same wheel, drag, key, selection, measurement, background, line weight and layer steps on SXF, JWW, DXF and DWG
  drawings give the same canvas pixels.
- `@zumenjs/core` keeps what `zumen-core` covers (the model and its JSON, geometry, text layout, measurement, notices,
  `isEntityVisible` and `readDrawing`) and builds without the DOM types. `DrawingReader` gains an optional `load`, and
  `dwg.load()` of `@zumenjs/dwg` calls `initDwg()`.
- `@zumenjs/react` has hooks only: `useDrawingViewer`, `useViewerState(viewer, selector)`, `useViewerEvent` and
  `useDrawing`, which replaces `useCadFile` and waits for the `load` of the reader that recognizes the file.
  `CadViewer`, its layer list and its CSS become the 2D sample of the playground, with the Japanese text.
- The engine and the hooks are tested in Chromium (Vitest browser mode with Playwright, `*.browser.test.*`); CI installs
  the Chromium headless shell.
- `@zumenjs/dwg`: read DWG in browsers and Node.js with the WebAssembly build of `zumen-dwg`. `initDwg` loads it, `dwg`
  is the reader for `readDrawing` (model space and layouts through `view`), and `isDwg` tells DWG apart without it. The
  playground opens DWG and loads the reader (1.6 MB, 494 KB with brotli) when the first DWG is opened.
- `pnpm run wasm` builds the WebAssembly reader into `packages/dwg/wasm` with cargo and wasm-bindgen-cli (the version of
  wasm-bindgen in `Cargo.lock`) instead of wasm-pack, without wasm-opt, and `pnpm run check` and `pnpm dev` run it.
  `tests/fixtures/dwg` holds two DWG drawings written by the Rust tests and their scenes.
- Rust: `zumen-dwg` reads DWG from AutoCAD 2000 to 2018 (AC1015 to AC1032) with its own decoder, which follows the Open
  Design Specification for .dwg files 5.4.1, into the AutoCAD drawing model of `zumen-dxf`, and draws it with
  `dxf_to_scene`: header variables, layers, line types, text and dimension styles, blocks, layouts, draw order,
  MLINESTYLE and every entity that DXF draws except HELIX and MPOLYGON. What is not read is reported. `zumen.read`, the
  `zumen` command, the Python bindings, the MCP server and the WebAssembly build read DWG. There is no TypeScript DWG
  reader ([decision 0003](docs/decisions/0003-dwg-in-rust.md)).
- Rust: `zumen-dwg` also reads DWG of AutoCAD R13 and R14 (AC1012, AC1014), which have the container of R2000 and
  forms of their own ([R13 and R14](docs/formats/dwg.md#r13-and-r14)), and of AutoCAD 1.0 to Release 12 (`MC0.0` to
  `AC1009`), which the Open Design Specification does not cover
  ([R12 and earlier](docs/formats/dwg.md#r12-and-earlier)). Their paper space becomes one layout, `Layout1`, and
  entities without a handle (DWG before R10) get the ids `#n` ([Entity ids](docs/architecture.md#entity-ids)).
- `@zumenjs/dxf` and `zumen-dxf`: a scene of a DWG document is labeled DWG (format `dwg`, version `DWG AC1032`,
  `DWG AutoCAD 2018（AC1032）`), and UTF-16 text `UTF-16`.
- `@zumenjs/dxf`: a hatch whose pattern lines are numbered beyond 2⁵³ (a base point far from the boundary, where adding 1
  no longer changes the number) is treated as too dense and drawn as its outline (`hatch-dense`); it hung the reader.
  Found by fuzzing DWG.
- `pnpm run parity` compares each DWG with the DXF of the same drawing (`<name>.dxf` or `<name>_ascii.dxf`) and reports
  the differences without failing.
- Python: an MCP server, `zumen-mcp` (`pip install "zumen[mcp]"`), with read-only tools for AI applications:
  `open_drawing`, `render_drawing`, `find_texts`, `find_entities`, `get_entity` and `measure_distance`. It serves over
  stdio, reads only below `--root` when given, and limits file sizes (`--max-bytes`), image sizes and result counts.
  The placement of an entity now includes `inverse`, the conversion from paper to real-world millimeters.
- Rust and Python: the texts and entities of a scene (`zumen_core::extract`; `Scene.texts`, `Scene.entities`,
  `Scene.entity` and `Scene.to_arrow` in Python; `zumen texts` and `zumen entities` as JSON lines), and rendering that
  returns the conversion between pixels and paper millimeters and the box of every entity drawn
  (`zumen_render::render_image`, `Scene.render` and `Rendering`, `zumen render --map`), so that an application can show
  the entity that a vision model refers to. `zumen render` takes `--region` and `--all-layers`.
- `@zumenjs/stb`: read ST-Bridge 2.0 and 2.1 structural models (UTF-8 and Shift_JIS, including mismatched declarations)
  into a 3D model of members extruded from their sections, with stories, grid axes, materials and the original XML
  attributes. `@zumenjs/stb/three` draws it with three.js (filters by kind, story and axis, picking), and
  `@zumenjs/stb/react` adds `StbViewer` and `useStbFile`. The playground opens ST-Bridge files next to drawings.
- `@zumenjs/sxf`: read SXF Ver.2.0–3.1 SFC (all 34 features) and P21 (the STEP AP202 subset) into the SXF feature
  model, and convert it to a scene.
- `@zumenjs/jww`: read Jw_cad JWW versions 600 and 700 (lines, arcs, ellipses, points, text, solids, blocks,
  dimensions) into the JWW document model, and convert it to a scene.
- `@zumenjs/dxf`: read DXF R12 to 2018 (ASCII and binary, `$DWGCODEPAGE` including Japanese `ANSI_932`) into the AutoCAD
  drawing model shared with DWG, and convert model space or a layout to a scene. Layout viewports keep their scale for
  real-world measurement; SHX fonts are replaced and reported.
- `@zumenjs/dxf`: read and draw MULTILEADER (text or block content, straight and spline leaders, arrow blocks, doglegs,
  frames and underlines) and MLINE with its MLINESTYLE (element offsets and MLEDIT cuts, fill, caps, inner arcs and miters).
  The model adds `DxfMLeader`, `DxfMLine` and `mline_styles`; an MLINE without element parameters, as read from DWG, is drawn from
  its style.
- `@zumenjs/core`: `sceneToJson` and `sceneFromJson` turn a scene into JSON and back, the format shared with the Rust readers.
- Rust: `zumen-sxf`, a port of the SXF reader (SFC and P21) that returns the same scenes as `@zumenjs/sxf` on the fixtures
  and on the public SFC and P21 samples. `zumen.read`, the `zumen` command and the WebAssembly build read SXF.
- `@zumenjs/sxf`: P21 strings written in 8-bit Shift_JIS are decoded (bytes were read as windows-1252, and second bytes from
  0x40 to 0x7E were split from their first byte); dimension projection lines are ordered by name without the user's
  locale; argument nesting deeper than 64 levels is skipped as unreadable, P21 sub-figure definitions nested deeper than
  64 levels are reported (`sfig-depth`), and layer codes above 4,294,967,295 are read as no layer (`layer-range`).
- `@zumenjs/core`: `clothoidPoints` stops the Simpson integration at 4,096 steps; a clothoid 10¹¹ mm long hung the reader.
- `pnpm run fuzz` compares the TypeScript and WebAssembly readers on randomly damaged copies of the fixtures.
- Rust: `zumen-dxf`, a port of the DXF reader (ASCII and binary, all code pages, layouts and viewports, MULTILEADER and
  MLINE) that returns the same scenes as `@zumenjs/dxf` on the fixtures, every layout included, and on 26 public DXF
  samples. `zumen.read`, the `zumen` command, the Python bindings and the WebAssembly build read DXF.
- `@zumenjs/dxf`: limits so that damaged or hostile drawings finish. A dimension whose block refers to itself stops at the
  depth limit of block references (it overflowed the stack); a polygon mesh visits only the grid positions that have
  vertices (an M × N of 10⁸ × 10⁸ hung the reader); a hatch pattern whose line count is not a number is treated as too
  dense (a NaN base point turned the 5,000-line limit off); and at most 1,000,000 entities are drawn inside blocks
  (`block-limit`), since a block that contains itself twice grows to 2³² entities before the depth limit.
- `@zumenjs/dxf`: a line that is not a group code is skipped and reading resumes on the next line in CRLF drawings too
  (the value was read one character off), and block names or entity types such as `__proto__` are counted and sorted
  like any other name.
- `@zumenjs/dxf` and `zumen-dxf`: three values are read as the DWG of the same drawing has them. ATTRIB and ATTDEF take
  their text generation flags (71) from `AcDbText` only, since AutoCAD 2018 writes the attribute type to the 71 of
  `AcDbAttribute` (multi-line attribute definitions were drawn upside down). Strings of binary DXF use the caret
  notation of ASCII DXF (`^J`, `^I`, `^ `), so the line feeds of MTEXT in binary DXF of AutoCAD 2004 and later break
  paragraphs, and TEXT draws `^ ` as `^` and `^I` as a tab. A dimension style whose DIMASZ or DIMTXT AutoCAD left out
  of the DXF gets 0.18 instead of 0, so a leader no longer takes its arrow size from `$DIMASZ`.
- `@zumenjs/dxf`, `zumen-dxf` and `zumen-dwg`: the multi-line attributes of AutoCAD 2018 are drawn. Their text is in an
  embedded MTEXT, which no reader read, and AutoCAD leaves the attribute's own text empty, so these attributes were
  neither drawn nor reported. `DxfAttrib` and `DxfAttdef` keep it in `mtext` (`DxfEmbeddedMText`), drawn as MTEXT with
  the attribute's color, layer and line type. `zumen-dwg` reads the "Is NOT annotative" bit of R2018 MTEXT the right
  way round: the values after an embedded MTEXT were out of step, which gave these attributes wrong flags.
- `zumen-core`: `js::sort_by` follows V8's `Array.prototype.sort` (TimSort), so orders match TypeScript even when a
  comparison is inconsistent (NaN); `js::string_to_number`, `parse_int` and `to_precision` follow `Number`, `parseInt`
  and `toPrecision`; and `js::to_fixed` computes `toFixed` with 128-bit integers, which builds scenes faster.
- `pnpm run parity` also compares every other view (DXF layouts). `pnpm run fuzz` damages DXF tags and MTEXT format
  codes too, and with `ZUMEN_FUZZ_TRACE=1` writes each input before reading it, to find inputs that hang or crash.
- Rust (`crates/`): `zumen-core` (the scene model with the same JSON, geometry, text layout and JavaScript-compatible
  number formatting), `zumen-jww` (a port of the JWW reader that returns the same scenes on the fixtures and on 205 public
  samples), `zumen` (`read` and `detect`), `zumen-render` (PNG rendering with the bundled BIZ UDGothic) and the `zumen`
  command. Python bindings (`zumen.read`, `Scene.render_png`, `Scene.to_json`) and a WebAssembly build (`readSceneJson`).
- `@zumenjs/core`: the scene model, the Canvas 2D renderer, `readDrawing`, and `DrawingViewer` with zoom, pan,
  selection, distance measurement in paper and real-world units, overlays and coordinate conversion.
  Scenes can list several views (`Scene.views`, chosen with `readDrawing(…, { view })`), colors can carry opacity
  (`#rrggbbaa`), and only near-black colors are drawn white on a dark background (pure blue stays blue).
- `@zumenjs/react`: `CadViewer`, `useDrawingViewer` and `useCadFile`. `useCadFile` takes a `view` and `CadViewer` shows
  a view selector with `onViewChange`.
