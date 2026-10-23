# 0008: Real-size model space, and the sheet as a layout with a viewport for each scale

> English reference preserved when Japanese became the primary documentation. See [0008-sheets-and-scales-in-the-autocad-model.md](0008-sheets-and-scales-in-the-autocad-model.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

SXF and JWW drawings are sheets. Their coordinates are paper millimeters, and parts of the sheet carry a scale:

- SXF places sub-figures (部分図, in mathematical or geodetic coordinates) on the sheet with a position, a rotation and
  a scale for each axis; inside a sub-figure the coordinates are real-world millimeters. Drawing parts (作図部品) and
  drawing groups (作図グループ) are placed the same way, and everything else is drawn on the sheet directly.
- JWW stores every entity in paper millimeters around the center of the sheet, and each of the 16 layer groups has a
  scale (1/100, 1/1 …) that turns them into real-world millimeters.

An AutoCAD drawing keeps geometry at real size in model space and composes the sheet in a paper space layout, where
viewports show model space at a scale. How the sheets and their scales map onto this decides whether a converted
drawing measures and dimensions correctly in AutoCAD, and whether it looks like the original.

## Options

1. **Model space is the sheet** (paper millimeters). Sub-figures become blocks inserted at their scale. Exact and
   simple, but nothing in model space is at real size: a 10 m wall of a 1/100 plan measures 100, and new dimensions show
   paper lengths. This is how a drawing looks after "explode to paper", not how AutoCAD drawings are made.
2. **Model space at the main scale.** The scale with the most entities is real size; the sheet frame and the other
   scales become blocks inserted at the ratio of the scales, and one viewport shows the whole sheet. Common in Japanese
   practice (a frame block scaled ×100 in model space), and a viewer that only shows model space still sees the sheet.
   But everything not at the main scale (details, frames) measures wrong in model space.
3. **Real size for every scale, one viewport per scale, and the sheet in paper space.** Each SXF sub-figure placement
   and each JWW scale becomes a viewport of the layout that shows real-size model space at that scale and rotation;
   what is drawn on the sheet itself (SXF sheet members, JWW 1/1 layer groups) goes to paper space. This is how an
   AutoCAD user lays out a sheet with several scales.

## Decision

Option 3, with these rules:

- **SXF**: a sub-figure placed on the sheet with the same scale on both axes becomes a viewport (scale, twist =
  rotation, position) onto its members at real size in model space; a sub-figure placed twice gets two viewports onto
  one copy. Sheet members go to paper space. A sub-figure with different horizontal and vertical scales (profiles,
  縦断図) cannot be a viewport and becomes a paper space INSERT of its block. Drawing parts and groups become blocks and
  INSERTs wherever they are placed. A drawing without sub-figures keeps its sheet coordinates in model space and gets
  one 1:1 viewport.
- **JWW**: entities go to model space at real size (coordinates × scale, the sheet center at the origin), with one
  viewport for each scale. When the drawing has more than one scale, the 1/1 layer groups (frames and titles) go to
  paper space instead; a drawing with one scale, 1/1 included, keeps everything in model space.
- **Separation**: viewports show what lies inside their rectangle (the extents of what the original draws there, plus
  a margin). When the regions of two scales overlap in model space, the one with fewer entities is moved to the right
  of the others, by a round amount, and the move is reported (`view-moved`), because a viewport would otherwise show
  the other scale's geometry. The first region keeps its coordinates, such as survey coordinates of a geodetic
  sub-figure.
- **Opening**: `$TILEMODE` is 0, so AutoCAD and zumen open the layout, which looks like the original sheet. Viewport
  frames are on the layer `VIEWPORTS`, which is off and not plotted (as ezdxf names it), and the viewports are locked.
  The layout has the paper size of the sheet, and line type dashes stay in paper millimeters (`$PSLTSCALE` 1).
- **Paper sizes inside model space**: arrows, point markers, hatch line dashes and line type dashes of SXF are paper
  millimeters whatever the scale (SXF 実装規約 1-4, 1-6, 1-7), and JWW dashes are too. They are divided by the scale of
  the viewport or of the block reference, so that they print at the same size.

## Consequences

- In model space, every SXF sub-figure and every JWW scale is at real size: measuring and dimensioning in AutoCAD give
  real lengths. zumen's reader gives the same: entities seen through a viewport carry its scale.
- The layout of the converted DXF draws the same scene as the original drawing, entity by entity
  (`crates/zumen/tests/convert_sxf_jww.rs` on the fixtures and on local samples), and ezdxf maps the model space through
  the written viewports to the same place on the sheet (`bindings/python/tests`).
- Programs that show model space only see no frame or title block, as with any AutoCAD drawing that keeps them in
  paper space. zumen shows the layout first.
- A moved region no longer has the original real-world coordinates. Sub-figures with different axis scales and blocks
  measure in the units of their container, and a block used at several scales keeps the arrow sizes and dash lengths
  of its first use (`block-scale`).
- The visibility of a sub-figure through its placement's layer is not kept: viewports show their contents whatever
  layer the viewport is on.

## References

- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm):
  VIEWPORT (view center, height, twist, status flags), LAYOUT and PLOTSETTINGS, `$TILEMODE` and `$PSLTSCALE`
- [How SXF is handled](../formats/sxf.md), [How JWW is handled](../formats/jww.md), [How DXF is handled](../formats/dxf.md)
- [Decision 0007: the zumen-convert crate](0007-mapping-crate-between-formats.md)
