# Changelog

All packages share one version during 0.x.

## Unreleased (0.1.0)

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
