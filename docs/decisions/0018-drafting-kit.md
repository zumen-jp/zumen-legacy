# 0018: Draw complete sheets from a declarative drawing spec, in a drafting kit that builds the AutoCAD drawing model

| Item | Value |
| --- | --- |
| Date | 2026-10-22 |
| Status | Proposed |

## Context

- zumen reads, writes and edits DXF, DWG, JWW and SXF. [Decision 0012](0012-edit-in-place-and-write-every-format.md)
  lets applications add entities to a drawing, and [decision 0017](0017-add-annotations-and-definitions.md) proposes
  dimensions, hatches, formatted text, block definitions and styles for those edits.
- Edits are the right tool to change a drawing that someone sent. They are the wrong level for an application, or an
  AI agent, that has to draw a whole sheet: a site plan at 1:250 with its boundary, survey points, dimensions and a
  north arrow, a floor plan at 1:100 with grid lines, walls and dimension chains, and the sheet around them with its
  border, title block and schedules. Drawn with edit operations, every caller would compute again:
  - **sizes on paper**: text 2.5 mm high on paper is 625 mm in model space at 1:250, and so are arrowheads, the radius
    of a grid bubble and the spacing of a hatch pattern;
  - **dimension geometry**: extension lines, the dimension line, arrowheads or ticks and the measured text, which
    every reader draws from the dimension's block;
  - **tables**: column widths, merged cells, alignment and text that has to fit a cell, for the summary, finish and
    area schedules that architectural sheets carry;
  - **sheets**: the paper size, the layout, a viewport for each view at its scale, the view titles.
- [Decision 0008](0008-sheets-and-scales-in-the-autocad-model.md) already fixed how zumen keeps scales: model space
  at real size, and the sheet as a layout with a viewport for each scale. `zumen-convert` builds documents that way for
  SXF and JWW, and every writer (DXF R12 to 2018, DWG, JWW, SXF) writes such a document.
- AI agents are good at deciding what a drawing should say and bad at computing coordinates of text baselines and
  arrowheads. They work best with a declarative description that a program checks and draws, and an image of the
  result to look at (`zumen render`).

This record also sets the goal for the base that drawing needs: what zumen has to be able to write so that the
general drawings of a building (summary sheets, site and survey plans, floor plans, elevations and sections, and
schedules) can be drawn from data, in every format zumen writes.

## Options

**Where sheets are built**

1. **Edit operations only.** Callers send `add` operations (0012, 0017). Every caller computes sizes, dimension
   geometry and table layout itself.
2. **A kit that turns a spec into edit operations.** The kit computes the geometry and emits the operations of 0012
   and 0017. It works on any drawing an editor can open, but it can only draw what the editors can add, which today
   is nine kinds and no layouts, and each editor gains kinds at its own pace.
3. **A kit that builds the AutoCAD drawing model (`DxfDocument`) directly**, with the builder that `zumen-convert`
   uses, and writes it with the existing writers. Everything the model holds is available now: layouts and viewports,
   blocks with attributes, dimensions with their blocks, hatches with pattern lines, MTEXT. It starts from an empty
   drawing, or from a drawing that zumen reads into the model (a template with a border and a title block), which is
   then written again rather than edited in place.

**The shape of the spec**

- **Imperative calls** (Rust and Python functions per symbol): easy to start, but an agent then writes programs.
- **One JSON document** for a drawing: layers, blocks, sheets, views and items, where every item is a typed object
  (`line`, `text`, `dimension`, `table`, `grid`…) in either model or paper coordinates. It can be validated, stored,
  diffed and patched, and the same document works from Rust, Python, the command line and the MCP server.

## Decision

Option 3, with one JSON document per drawing. The kit is a new crate, `zumen-draft`, which depends on `zumen-core`,
`zumen-dxf` and `zumen-convert` (whose builder becomes public for it), and the facade gains `zumen::draft`. Option 2
follows once decision 0017's operations exist, for drawing into a received file in place (`zumen::draft_into`); both
paths share the geometry code. (Done: `zumen_draft::plan` turns the drawn model into 0017's operations, and
`zumen::draft_into` applies them; see [Drawing into a template](../drafting.md#drawing-into-a-template).)

### The spec

```jsonc
{
  "units": "mm",
  "font": "ＭＳ ゴシック",
  "layers": [{ "name": "A-ANNO", "color": 7, "lineWidth": 0.18 }],
  "lineTypes": [{ "name": "CENTER", "pattern": [12, -3, 3, -3] }],      // paper millimeters
  "blocks": [{ "name": "WC", "basePoint": [0, 0], "items": [ ... ], "attributes": [ ... ] }],
  "sheets": [{
    "name": "A05-1", "paper": "A3", "orientation": "landscape",
    "frame": { "margins": [20, 10, 10, 10], "titleBlock": { ... } },
    "views": [{
      "name": "現況図", "scale": 250, "viewport": [30, 20, 300, 280],      // on paper, mm
      "center": [12000, 9000],                                              // model point at the viewport's center
      "title": { "number": "1", "text": "現況図" },
      "items": [ ... ]                                                      // model space, real millimeters
    }],
    "items": [ ... ]                                                        // paper space, paper millimeters
  }]
}
```

- **Coordinates**: view items are model coordinates at real size (decision 0008); sheet items are paper millimeters.
  Sizes of annotations (`height` of text, `arrowSize`, `bubble`, hatch `spacing`) are always paper millimeters and
  are multiplied by the view's scale.
- **Items**: `line`, `polyline`, `rectangle`, `circle`, `arc`, `text` (one line or MTEXT, with `align`, `width` for
  wrapping and `fit` for shrinking into a width), `hatch` (`SOLID`, built-in patterns written for zumen, or pattern
  lines), `dimension` (`linear`, `aligned`), `dimensionChain` (consecutive dimensions along points, with an overall
  dimension), `insert` (a block with attribute values), `table`, `grid` (axis lines with bubbles and a dimension
  chain), `northArrow`, `scaleBar`, `levelMark`, `spotLevel`, `leader`, `polygonLabel` (side lengths and area of a
  boundary), and `group` (items with a shared layer and style). Every item takes `layer`, `color`, `lineType` and
  `lineWidth`.
- **Validation**: an error names the path of the item (`sheets[0].views[1].items[4]`), a stable code
  (`invalid-spec`, `unknown-type`, `unknown-key`, `invalid-value`, `missing-definition`) and a message.
- **Output**: `zumen::draft(spec, to, options)` writes the drawing in any format that `convert` writes (DXF R12 to
  2018, DWG, JWW, SFC, P21, SFZ, P2Z), with the writers' notices. The created entities carry the item path as their
  source, so a caller can map handles back to the items it sent.

### The goal for the base

zumen is complete for drawing when the following hold, each checked by a test that drafts a sheet, writes it, reads
it back with zumen and compares what is drawn:

1. **Entities**: lines, polylines with arcs, circles, arcs, ellipses, points, single-line and multi-line text with all
   nine alignments, linear and aligned dimensions (then angular, radial and diameter), hatches (solid and pattern),
   block definitions with attribute definitions, block references with attributes, leaders.
2. **Definitions**: layers with color, line type, line width and plot flag; line types; text styles with Japanese
   fonts; dimension styles.
3. **Sheets**: several layouts in one drawing, each with its paper size and viewports at their scales.
4. **Formats**: the same spec written as DXF 2018 and R12 (with the documented losses of R12), DWG 2018 and R2000,
   JWW and SFC. A spec that a format cannot hold completely is written with notices, never silently.
5. **Interoperability**: the DXF passes the ezdxf audit, and the DXF and DWG open in AutoCAD and the JWW in Jw_cad
   without errors ([decision 0014](0014-verify-on-native-windows.md), recorded in `docs/verification.md`).
6. **For agents**: the spec has a JSON Schema, the Python package and the MCP server draft and render a sheet in one
   call, and a sample spec draws a complete A3 sheet set (summary table sheet, site plan, floor plan, elevation).

The edit operations of decision 0017 are the in-place half of the same goal (items 1 to 3 for drawings that zumen
edits rather than builds).

## Consequences

- An application describes a sheet and gets a drawing in any format, with sizes, dimensions and tables computed the
  same way every time. Changing a value and drafting again gives the same drawing with only that change.
- Drafting from a template reads the template into the model and writes it again. What the model does not hold
  (extension data, objects that zumen does not read) is lost and reported by the writer; drawing into a received file
  without such loss is `zumen::draft_into` (option 2).
- Dimensions are drawn by zumen with closed filled arrowheads, ticks or dots, the text above the line, and the
  measured value rounded to the dimension's precision. AutoCAD recomputes a dimension when someone edits it, from its
  style, and may then draw it differently.
- Built-in hatch patterns and the north arrow are drawn by zumen, not copied from a CAD program's pattern file or
  symbol library.
- The width of text is estimated (full-width characters as wide as they are high, half-width characters half as wide,
  times the width factor), so `fit` and table cells leave a margin for fonts that differ from the estimate.
- `zumen-convert` exposes its builder to `zumen-draft`; it remains the only crate that maps between formats.

## References

- [Decision 0008](0008-sheets-and-scales-in-the-autocad-model.md), [decision 0012](0012-edit-in-place-and-write-every-format.md),
  [decision 0017](0017-add-annotations-and-definitions.md)
- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm):
  DIMENSION, HATCH (Pattern Data), LAYOUT, VIEWPORT, ATTDEF, ATTRIB, MTEXT
- JIS Z 8310 (製図総則), JIS Z 8311 (製図用紙のサイズ及び図面の様式), JIS Z 8317 (寸法記入方法), JIS A 0150
  (建築製図通則): paper sizes, borders and title blocks, dimensioning and the conventions of architectural drawings
