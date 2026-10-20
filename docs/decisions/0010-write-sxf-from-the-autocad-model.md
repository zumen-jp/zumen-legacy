# 0010: Write SXF from the AutoCAD drawing model, with viewports as sub-figures and drawn shapes for what SXF lacks

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

[Decision 0006](0006-write-through-the-autocad-model.md) writes every output from the AutoCAD drawing model
(`DxfDocument`), and [decision 0007](0007-mapping-crate-between-formats.md) puts the mappings between formats in
`zumen-convert`. [Decision 0008](0008-sheets-and-scales-in-the-autocad-model.md) maps an SXF sheet into the model with
real-size model space and a layout that has one viewport for each scale. The next output is SXF, for drawings that are
delivered to public works: a DWG or DXF received from a designer, or an SXF or JWW drawing passed through the model.

SXF is narrower than the model. It has one sheet, sub-figures placed with a position, a rotation and a scale for each
axis, drawing parts and groups, and a fixed set of features: lines, polylines, circles, arcs, ellipses, cubic Bézier
splines, clothoids, single-line texts with a text box, five kinds of dimensions, leaders and balloons, point markers and
hatches over composite curves. It has no multiline text, no text styles with alignment, no BYBLOCK, no clipping of
sub-figures, no wide polylines, no solids, no NURBS and no multileaders. Colors, line types and line widths are codes,
line type pitches are paper millimeters whatever the scale, and strings are CP932.

Two choices need a decision: how a model with layouts, viewports and blocks becomes one SXF sheet, and what to write for
entities that SXF has no feature for.

## Options

For the sheet:

1. **Model space only, at real size, on a sheet as large as the drawing.** Simple, but a drawing that opens in a layout
   (a sheet with a frame, a title block and viewports at 1:100 and 1:20) would lose its sheet, and SXF readers show the
   sheet.
2. **The reverse of decision 0008.** The layout that the drawing opens in is the sheet; each viewport becomes a
   sub-figure (部分図) with the model space entities it shows at real size, placed at the viewport's position, scale and
   twist; blocks become drawing parts (作図部品) placed like their block references. A drawing that opens in model
   space becomes one 1:1 sub-figure on a sheet the size of the drawing.

For entities without an SXF feature:

1. **Leave them out** and report them. Texts would be missing from almost every drawing (MTEXT, attributes).
2. **Map each one to the nearest SXF feature by its own rules**: MTEXT laid out line by line, dimensions as SXF
   dimensions, multileaders as leaders, solids as filled regions. It keeps the most meaning, but it repeats the text,
   dimension and leader rules of the DXF scene (word wrap, stacked fractions, dimension blocks, landing and arrows) in a
   second place, where they can drift.
3. **Write what the DXF scene draws for them.** The scene already resolves these entities into strokes, fills, dots and
   text boxes at the right place; each becomes the matching SXF feature (polyline, a filled region over invisible
   composite curves, point marker, text with the same box, rotation and slant). Entities that SXF has a feature for
   (lines, arcs, circles, ellipses, polylines with bulges, B-splines that split into Bézier curves, points, block
   references, viewports) are mapped directly, so that they stay editable as what they are.

## Decision

The sheet follows option 2, and entities without an SXF feature follow option 3. `zumen_convert::dxf_to_sxf` maps a
`DxfDocument` into an `SxfDrawing`, `zumen_sxf::write_sfc` writes it as SFC of SXF Ver.3.1, and `zumen::convert(bytes,
"sfc", …)` does both for every format.

- **Sheet and viewports**: the sheet has the paper size of the layout (an A size when it matches, else free). A
  viewport's sub-figure holds the model space entities whose extents meet its frame, except layers frozen in that
  viewport. Viewports that show the same entities share one sub-figure, as decision 0008 does the other way. A 1:1
  viewport that does not move the origin puts its entities on the sheet itself, so that an SXF drawing without
  sub-figures comes back without them. Placements of viewports are on layer 0 (no layer), because the viewport layer is
  usually off.
- **Blocks**: a block reference becomes a placement of a drawing part with the block's base point at its origin. SXF has
  no BYBLOCK, so a block whose entities inherit color, line type, line width or layer 0 from the reference gets one
  drawing part for each set of inherited values, and a block with dashed lines one for each paper scale, because SXF
  pitches are paper millimeters. Dimensions are written as the entities of their block, in the container of the
  dimension.
- **Drawn shapes**: entities without an SXF feature are drawn once, all together, by `dxf_to_scene` in a copy of the
  model where they are in model space at the drawing unit and carry the values they inherit, and their primitives are
  mapped back into the coordinates of their container. Hatches keep their pattern lines as SXF hatching
  (`fill_area_style_hatching`), with the boundary that the scene draws.
- **Codes**: ACI 7 (the foreground) and the white of the display are SXF black, colors equal to a predefined color use
  its code, and the rest are user colors. Line types keep the pitch the scene draws, in paper millimeters; a dot (a dash
  of length 0) is 0.25 mm.
- **SFC**: features are written in the order the reader assembles them (definitions, composite curves, sub-figures with
  the placed ones first, sheet members, the sheet), numbered #10, #20 …, in the comment blocks that public SFC files use.
  Characters outside CP932 become `?`.
- **P21 later**: P21 (the AP202 entities of the same features) needs a writer of its own instances and references. The
  feature model is the same, so it can be added as a second writer of `SxfDrawing` without changing the mapping.

## Consequences

- An SXF or JWW drawing converted to DXF and back to SFC, and every DXF and DWG fixture and local sample converted to SFC,
  draw the same scene as the drawing they came from, entity by entity (`crates/zumen/tests/convert_to_sxf.rs`), within
  the difference in curve flattening and the losses reported in notices.
- What SXF cannot hold is reported, not dropped: other layouts (`layouts`), model space that no viewport shows
  (`outside-viewports`), entities that a viewport frame clips, which a sub-figure cannot clip (`viewport-clip`),
  mirrored text (`text-mirror`), dots of line types, transparency, gradients, and characters outside CP932.
- Dimensions, multiline texts, leaders and multileaders are no longer dimensions, multiline texts or leaders in SXF:
  they print the same, but an SXF editor sees lines and texts. Writing SXF dimension features for linear, radial,
  angular and arc dimensions is a later improvement of the mapping.
- A block used with many different inherited values or scales becomes several drawing parts.
- The mapping depends on the DXF scene for drawn shapes, so a change in how the scene draws an MTEXT or a leader also
  changes the SXF written for it. That is intended: there is one set of drawing rules.

## References

- 国土交通省 SXF Ver.3.1 仕様書・同解説 附属書 SFC 編（1-1 the file, 1-2 the features, 1-3 the order of definitions),
  フィーチャ仕様編, 実装規約 1-4, 1-6 and 1-7 (sizes and pitches in paper millimeters)
- [How SXF is handled: Writing SXF](../formats/sxf.md)
- [Decision 0006](0006-write-through-the-autocad-model.md), [0007](0007-mapping-crate-between-formats.md),
  [0008](0008-sheets-and-scales-in-the-autocad-model.md)
