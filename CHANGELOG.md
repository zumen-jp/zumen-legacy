# Changelog

All packages share one version during 0.x.

## Unreleased (0.1.0)

- Rust and Python: compare two revisions of a drawing. `zumen_core::compare` matches entities by shape and text, not
  by id, so revisions saved again or in another format compare too. It reports the removed, added and changed entities
  and the regions that changed, aligns a revision that moved as a whole by its texts, and draws the result as a scene
  (red before, blue after, gray unchanged). `zumen.compare` and `Comparison` in Python, `zumen compare` in the command
  line, and `compare_drawings` in the MCP server.
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
