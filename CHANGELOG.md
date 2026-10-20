# Changelog

All packages share one version during 0.x.

## Unreleased (0.1.0)

- Rust, Python and the command line: edit drawings in place and create new ones
  ([decision 0010](docs/decisions/0010-edit-in-place-and-write-every-format.md)). The operations are the same for every
  format: `add`, `update` and `delete` entities, and `add-layer` and `update-layer`, with the kinds, ids and property
  keys of the scene and coordinates in the drawing's own units (`zumen_core::edit`). An edit keeps the format, version,
  text encoding and everything the operations do not name, and fails as a whole with a code and the number of the
  operation. DXF is done (R12 to 2018, ASCII and binary): `zumen_dxf::edit_dxf` keeps every tag that no operation
  changes byte for byte, so extended data, objects and unread entities stay. `zumen::edit` and `zumen::create`,
  `zumen.edit`, `zumen.create`, `Edited` and `EditError` in Python, and `zumen edit` and `zumen create`.

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
