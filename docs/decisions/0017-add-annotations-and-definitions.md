# 0017: Let edits add block definitions, dimensions, styles, hatches, text options, attributes, sheets and leaders to DXF and DWG

| Item | Value |
| --- | --- |
| Date | 2026-10-22 |
| Status | Proposed |

## Context

- [Decision 0012](0012-edit-in-place-and-write-every-format.md) made `zumen::edit` and `zumen::create` add, change and
  delete entities and layers. `add` takes nine kinds in DXF and DWG: `line`, `polyline`, `arc`, `circle`, `ellipse`,
  `ellipse-arc`, `point`, `text` (MTEXT when the text has line breaks or `wrapWidth`) and `block-reference`.
- That is enough to mark up a drawing, but not to draw the general drawings of a building from nothing: a site plan,
  floor plans and elevations, with the summary and finish schedules beside them. Those drawings need what the nine
  kinds lack:
  - **Dimensions**, which architects read as values and AutoCAD keeps associated with their points. Drawn as lines
    and text, they lose both.
  - **Hatches**: solid fills of walls and columns, patterns for sections, ground and materials.
  - **Text placed in a cell**: aligned to a corner or the middle, with a line spacing, for schedules and title blocks.
  - **Block definitions**: a door, a window, a north arrow or a fixture drawn once and placed many times. `add` places
    blocks, but only blocks that the drawing already has.
  - **Styles**: a text style with a Japanese font, a dimension style whose text and arrows are the right size for the
    drawing's units and scale, line types, and sheets (layouts with viewports).
- A dimension is two things in DXF and DWG: the DIMENSION entity, with its definition points and style, and an
  anonymous block (`*D<n>`) holding the lines, arrowheads and text that are drawn. Readers that do not compute
  dimensions, zumen among them, draw the block; a dimension without it is dropped (`dimension-block`). AutoCAD
  draws the block too, and computes it again only when the dimension is edited.
- A hatch in DXF and DWG carries its boundary paths and, for a pattern, the pattern's definition lines; it does not
  name a pattern file. A pattern given only by its name is drawn by no one who lacks that file.

## Options

**Order of work**

1. **What the drawings need most first**: dimensions, then hatches. But a dimension needs an anonymous block, and
   the editors cannot create blocks yet.
2. **Block definitions first, then dimensions and their styles, hatches, text options, attributes, sheets and
   leaders.** Creating a block is small on its
   own, is needed by dimensions, and is useful by itself.

**How a dimension is drawn**

- **Only the DIMENSION entity**, leaving the block to the program that opens the file: AutoCAD would draw it after a
  regeneration, but zumen, other readers and AutoCAD before that would draw nothing.
- **The entity and its block, computed from the dimension style**: everyone draws it; AutoCAD recomputes it from the
  same values when it is edited, so the two agree as long as zumen follows the style's variables.

**How a hatch gets its pattern**

- **A name only**: drawn by nobody without the pattern file.
- **A name and a small built-in set of definitions**, plus definition lines given by the caller for anything else.

**The shape of the operations**

- **New kinds of `add` for entities, new ops for definitions**, with the keys that the scene already reads for these
  entities (`dimensionType`, `dimensionStyle`, `pattern`, `patternScale`, `patternAngle`, `justification`).
- **One generic op that writes group codes**: complete, but it would give agents a DXF tag list to get right, would
  differ between DXF and DWG, and could not be checked.

## Decision

Option 2 for the order, a computed block for dimensions, built-in pattern definitions for hatches, and new kinds
and ops with the scene's keys. Each step is its own pull request, for DXF and DWG together, and passes the
conformance test (`crates/zumen/tests/edit.rs`): the edited drawing is read again by zumen and draws what was added.

1. **Block definitions**: a new op.

   ```json
   {"op": "add-block", "name": "DOOR-900", "basePoint": [0, 0]}
   ```

   It creates an empty definition (BLOCK_RECORD, BLOCK and ENDBLK; BLOCK and ENDBLK in R12; BLOCK_HEADER, BLOCK and
   ENDBLK in DWG). Later operations of the same edit fill it with `"container": "block:DOOR-900"` and place it with
   `block-reference`. A name that exists is `already-exists`; a name that is not a valid symbol name, or starts with
   `*` (reserved for anonymous blocks), is `invalid-value`. `created` gives the name.
2. **Dimensions**: `kind: "dimension"`, linear (rotated) and aligned first.

   ```json
   {"op": "add", "kind": "dimension", "dimensionType": "linear", "start": [0, 0], "end": [5000, 0],
    "position": [2500, -800], "rotation": 0, "dimensionStyle": "Standard", "text": "<>"}
   ```

   `start` and `end` are the origins of the extension lines (13 and 14), `position` a point on the dimension line
   (10), `rotation` the angle of a linear dimension's line in degrees (50; aligned dimensions follow `start`–`end`).
   `dimensionStyle` names a style the drawing has (`missing-definition` otherwise), by default the drawing's
   `$DIMSTYLE`, else `Standard`. `text` is the text, with `<>` for the measured value, which is the default. The
   editor writes the DIMENSION and an anonymous block `*D<n>` with the dimension line, the extension lines, closed
   filled arrowheads and the text, sized from the style's DIMSCALE, DIMTXT, DIMASZ, DIMEXO, DIMEXE, DIMGAP, DIMTAD,
   DIMDEC and DIMZIN; other variables (other arrowheads, tolerances, alternate units) are not drawn and are reported
   as a notice. Angular, radial and diameter dimensions follow in the same way.
3. **Styles**, as ops, because a dimension on a new drawing needs a style sized for its units and scale:

   ```json
   [{"op": "add-text-style", "name": "JP", "font": "MS Gothic", "bigFont": null, "height": 0, "widthFactor": 1},
    {"op": "add-dimension-style", "name": "DIM-100", "from": "Standard", "scale": 100, "textHeight": 2.5,
     "arrowSize": 2.5, "extensionOffset": 0.625, "extensionExtension": 1.25, "textGap": 0.625, "textAbove": true,
     "decimals": 0, "suppressTrailingZeros": true, "tickSize": 0, "linearFactor": 1, "textStyle": "JP"},
    {"op": "add-line-type", "name": "DASHED2", "description": "__ __", "pattern": [6, -3]}]
   ```

   `add-dimension-style` copies `from` (by default `Standard`) and sets the variables of step 2 by readable names
   (`scale` DIMSCALE, `textHeight` DIMTXT, `arrowSize` DIMASZ, `extensionOffset` DIMEXO, `extensionExtension`
   DIMEXE, `textGap` DIMGAP, `textAbove` DIMTAD, `decimals` DIMDEC, `suppressTrailingZeros` DIMZIN 8, `tickSize`
   DIMTSZ, `linearFactor` DIMLFAC, `textStyle` DIMTXSTY), so that AutoCAD draws the dimension the same way when it
   recomputes it. `font` is a TrueType family name or an SHX file; `pattern` is dashes (positive), gaps (negative)
   and dots (0) in drawing units.
4. **Hatches**: `kind: "hatch"`.

   ```json
   {"op": "add", "kind": "hatch", "boundaries": [[[0, 0], [5000, 0], [5000, 3000], [0, 3000]]],
    "pattern": "ANSI31", "patternScale": 20, "patternAngle": 0}
   ```

   `boundaries` are closed polylines (the first is the outer one, the others holes, drawn with the normal style);
   the edit values gain arrays of point arrays for them. `pattern` is `SOLID` or a built-in pattern (`ANSI31`,
   `ANSI37`, `LINE`, `NET`, `DOTS` and a few more that the documentation lists, with definitions written in zumen),
   or any name together with `patternLines` (`[[angle, x, y, dx, dy, dash…], …]`, the lines of a PAT file).
   Hatches are not associative. R12 has no HATCH and refuses it (`unsupported-kind`).
5. **Text options**: `kind: "text"` takes `justification` (`top-left` … `bottom-right`, also `left`, `center`,
   `right` on the baseline for single-line text), `lineSpacing` (a factor, MTEXT) and `multiline: true` to write
   MTEXT without a line break. MTEXT stays refused in R12.
6. **Block attributes**, for title blocks: `kind: "attribute-definition"` in a block (`container: "block:<name>"`)
   with `tag`, `prompt`, `text` (the default value) and the keys of `text`; `block-reference` takes `attributes`, an
   object from tag to value, and writes an ATTRIB for each attribute definition of the block (the default value when
   the object does not name it). The edit values gain objects of strings for it.
7. **Sheets**: `add-layout` and viewports in it.

   ```json
   [{"op": "add-layout", "name": "A3-平面図", "paperSize": [420, 297]},
    {"op": "add", "kind": "viewport", "container": "layout:A3-平面図", "center": [210, 148.5],
     "width": 380, "height": 257, "viewCenter": [12000, 8000], "scale": 100}]
   ```

   `add-layout` creates a LAYOUT with its paper space block (`*Paper_Space<n>`), the paper size in millimeters and
   the paper's own viewport; `viewport` adds a VIEWPORT whose `center`, `width` and `height` are in paper
   millimeters, `viewCenter` in model coordinates, and `scale` the denominator (1:100 is `100`, so the view height
   is `height × scale`). R12 has one paper space and refuses `add-layout`; R13 and R14 DWG keep layouts in the named
   objects dictionary, as reading finds them.
8. **Leaders**: `kind: "leader"` with `vertices` (the first is the arrowhead's tip), `text`, `height` and
   `dimensionStyle`, written as LEADER with an MTEXT annotation (the arrowhead from the style, as in step 2). R12 has
   no LEADER and refuses it.
9. **Raster images**, if time allows: `kind: "image"` with `path` (kept as the IMAGEDEF's file name, which the
   reader keeps for writing too), `pixels` (`[width, height]`), `position` (lower left), `width` in drawing units
   and `rotation`. IMAGE exists from R14 on.

The same operations go to SXF and JWW editors when they gain these entities; until then those formats refuse the
new kinds and ops with `unsupported-kind`.

## Consequences

- An application can draw the general drawings of a building, with dimensions, fills, schedules, sheets and title
  blocks, from an empty `create` or into a drawing it received, in DXF R12 to 2018 and DWG R13 to 2018.
- Dimensions added by zumen look as AutoCAD would draw them only as far as the variables listed in step 2 go. A
  style that uses other arrowheads or tolerances is drawn with closed filled arrowheads and no tolerance, and the
  notice says so; AutoCAD redraws the dimension when someone edits it.
- Built-in hatch patterns are definitions written for zumen, not copies of a CAD program's pattern file. Their names
  match the common ones so that CAD programs show the expected name.
- The edit values gain arrays of point arrays (hatch boundaries) and objects of strings (attribute values).
- `EditOperation` gains variants for the new ops, so every format's editor names them; formats without them refuse
  them, as the edit contract already requires.

## References

- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm)
  (AutoCAD 2026): BLOCK_RECORD, BLOCK, ENDBLK, ATTDEF, ATTRIB, LAYOUT, VIEWPORT, LEADER, IMAGE, IMAGEDEF, DIMENSION (Common Dimension Group Codes, Aligned and Linear Dimension
  Group Codes), DIMSTYLE, HATCH (Boundary Path Data, Pattern Data), MTEXT
- Open Design Alliance, *Open Design Specification for .dwg files* 5.4.1: 20.4.22–20.4.30 (dimensions), 20.4.52
  (BLOCK_HEADER), 20.4.68 (DIMSTYLE), 20.4.75 (HATCH)
- [Decision 0012](0012-edit-in-place-and-write-every-format.md), [How DXF is handled: Editing](../formats/dxf.md#editing-edit_dxf)
