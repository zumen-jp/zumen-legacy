# Changelog

All packages share one version during 0.x.

## Unreleased (0.1.0)

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
- `@zumenjs/core`: the scene model, the Canvas 2D renderer, `readDrawing`, and `DrawingViewer` with zoom, pan,
  selection, distance measurement in paper and real-world units, overlays and coordinate conversion.
  Scenes can list several views (`Scene.views`, chosen with `readDrawing(…, { view })`), colors can carry opacity
  (`#rrggbbaa`), and only near-black colors are drawn white on a dark background (pure blue stays blue).
- `@zumenjs/react`: `CadViewer`, `useDrawingViewer` and `useCadFile`. `useCadFile` takes a `view` and `CadViewer` shows
  a view selector with `onViewChange`.
