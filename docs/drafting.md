# Drafting sheets from a drawing spec

`zumen::draft` draws a complete drawing from one JSON document, the *drawing spec*: sheets with a border and a title
block, views at their scales, and the items on them (text, dimensions, hatches, tables, grid lines, symbols). It
writes the drawing in every format that `convert` writes: DXF R12 to 2018, DWG, JWW and SXF (SFC, P21, SFZ, P2Z).
The reasons are in [decision 0018](decisions/0018-drafting-kit.md); the JSON Schema is
[`crates/zumen-draft/schema/drawing-spec.schema.json`](../crates/zumen-draft/schema/drawing-spec.schema.json), and
[`crates/zumen-draft/examples/sheets.json`](../crates/zumen-draft/examples/sheets.json) is a four-sheet A3 set
(summary sheet with tables, site plan, floor plan, elevation).

```sh
zumen draft sheets.json -o sheets.dwg                  # every sheet as a layout
zumen draft sheets.json -o plan.jww --sheet A10-1      # JWW and SXF hold one sheet
zumen render sheets.dwg -o site.png --view A05-1       # look at a sheet
```

```python
import zumen
zumen.draft(spec, "dxf", version="2018").save("sheets.dxf")   # spec: a dict, a JSON string or a path
zumen.draft_schema()                                          # the JSON Schema
```

The MCP server has the same as the `draft_drawing` tool; `render_drawing` with `view` set to a sheet name shows the
result, so an agent can draft, look and correct in a loop.

## Coordinates and sizes

- **Model space is real size** ([decision 0008](decisions/0008-sheets-and-scales-in-the-autocad-model.md)): the items
  of a view are in real millimeters, and the view shows them through a viewport at `1:scale`.
- **Sheet items are paper millimeters**, with the origin at the bottom left of the paper and Y up.
- **Sizes are always paper millimeters**: text heights, arrowheads, symbols, table column widths and row heights,
  grid bubbles and hatch spacing. A 2.5 mm text in a 1:250 view is 625 mm high in model space and prints at 2.5 mm.
- Angles are degrees, counterclockwise from the X axis.
- Two views whose model regions overlap would show each other's items; the notice `view-overlap` says so. Keep the
  views apart in model space (the sample puts the site plan near 0, the floor plan near 100 m and the elevation near
  200 m).

## The document

| Key | Contents |
| --- | --- |
| `units` | `"mm"` (the only unit for now) |
| `font` | Default font, `ＭＳ ゴシック` unless given |
| `defaults` | `layer`, `textHeight` (2.5), `dimensionTextHeight`, `arrow` (`arrow`, `tick`, `dot`, `none`), `arrowSize` (2.5), `precision` (0), `thousands` (true: `6,400`), `dimensionLayer`, `textLayer`, `hatchLayer` |
| `layers` | `{name, color, lineType, lineWidth, plot, off, locked}`. Layers used by items but not listed are created with color 7 |
| `lineTypes` | `{name, description, pattern}`: dash and gap lengths in turn, paper millimeters, 0 for a dot. Built in: `DASHED`, `HIDDEN`, `CENTER`, `PHANTOM`, `DOT` (and `Continuous`, `ByLayer`, `ByBlock`) |
| `blocks` | `{name, basePoint, items, attributes}`. Items are in block units (paper millimeters when the block is placed at scale 1 on paper). `attributes` are attribute definitions `{tag, prompt, value, at, height, align, rotation, invisible}` |
| `sheets` | One layout each: `{name, fields, paper, orientation, frame, views, items}` |

A **sheet** has `paper` (`A0`–`A4`, `B1`–`B5` or `[width, height]`, A3 by default), `orientation` (`landscape` by
default), and a `name` that contains a non-space character and differs from `Model` (case insensitive, ignoring
surrounding spaces), which is reserved for model space. Its other keys are:

- `frame`: `{margins: [left, bottom, right, top], lineWidth, layer, titleBlock}`. Margins default to `[20, 10, 10, 10]`
  (JIS Z 8311, with binding). `titleBlock` is `{placement, width, textHeight, rows}`: `right` (default) is a vertical
  strip along the right border whose rows `{label, value, height, valueHeight, align}` run from the top, rows without
  a height sharing what is left; `bottom-right` is a two-column table of labels and values at the bottom right.
- `views`: `{name, scale, viewport: [x0, y0, x1, y1] on paper, center, title, border, items}`. `center` is the model
  point at the middle of the viewport (the middle of the items by default). `title` is `{number, text, scaleText, at,
  height}`: a numbered circle, the name and `S=1:<scale>` with a line under them, inside the bottom left of the viewport
  unless `at` is given.
- `items`: items on paper.
- `fields`: attribute values by tag, `{"DWGNO": "A-101"}`, written into the block references on the sheet (see
  [Drawing into a template](#drawing-into-a-template)).

## Items

Every item is an object with a `type`, and may have `layer`, `color` (ACI 1–255, `#rrggbb`, `bylayer`, `byblock`),
`lineType` and `lineWidth` (mm). A `group` passes these to its items.

| Type | Keys | Notes |
| --- | --- | --- |
| `line` | `from`, `to` | |
| `polyline` | `points`, `closed`, `bulges`, `width` | `bulges` are per vertex (tan of a quarter of the arc angle); `width` is a filled width in paper mm |
| `rectangle` | `from`, `to`, `fill` | Opposite corners; `fill` adds a solid hatch |
| `circle`, `arc`, `ellipse` | `center`, `radius` (`start`, `end` in degrees for arcs); `majorAxis`, `ratio`, `start`, `end` for ellipses | `circle` takes `fill` |
| `point` | `at` | |
| `text` | `at`, `text`, `height`, `align`, `rotation`, `width`, `fit`, `widthFactor`, `lineSpacing`, `font` | `align`: `top-left` … `bottom-right` (9) or `left`, `center`, `right` on the baseline; `bottom-left` by default. Line breaks or `width` (wrap) give MTEXT; `fit` shrinks the width factor to a width, then the height |
| `hatch` | `boundaries`, `pattern`, `scale`, `angle`, `patternLines`, `outline` | The first boundary is the outside, the others holes. `SOLID` by default; built-in patterns `LINE`, `ANSI31`, `ANSI32`, `ANSI37`, `NET`, `DOTS`, `DASH`, `GROUND`, drawn at their paper spacing × `scale`; any other name with `patternLines` `[angle, x, y, dx, dy, dash, gap…]` as in a PAT file, in paper mm |
| `dimension` | `kind` (`linear`, `aligned`, `angular`, `radius`, `diameter`), `from`, `to`, `offset` or `at`, `angle`, `center`, `radius`, `text` | `offset` is the paper distance of the dimension line from the points, positive on the left of `from → to`; `at` is a point on the dimension line. A linear dimension is horizontal or vertical by default. `angular` measures at `center` the angle from `from` counterclockwise to `to` on an arc of `radius` (model length; by default half the distance to the nearer point), or, with `at` (a point on the arc) instead of `radius`, the angle between the two sides that holds `at`; the value is in degrees with `precision` decimals. `radius` and `diameter` measure the circle of `center` and `radius` along the direction `angle` (45° by default), with `R` or `Ø` before the value. `text` replaces `<>` with the measured value |
| `dimensionChain` | `points`, `angle`, `offset` or `at`, `total`, `totalOffset` | One dimension between each pair of points; `total` adds the overall dimension 8 mm further out |
| `insert` | `block`, `at`, `scale` (number or `[x, y]`), `rotation`, `attributes` | `attributes` maps tags to values; tags the block lacks are an error |
| `table` | `at` (top left), `columns`, `rows` or `rowHeight` (7), `cells`, `textHeight`, `padding` (1), `align` (`middle-left`), `columnAlign`, `fit` (`shrink`, `wrap`, `none`), `outerWidth` (0.35), `innerWidth` (0.13) | A cell is a string, a number, `null` or `{text, colspan, rowspan, align, height, diagonal}`. Rows list the cells not covered by a span from above. Rules are drawn only between different cells; text that does not fit is condensed (`shrink`) or wrapped (`wrap`), and lowered if the lines do not fit the height |
| `grid` | `x`, `y` (`[{label, at}]`), `extent`, `overhang` (20), `bubble` (8), `bubbles` (`start`, `end`, `both`, `none`), `dimensions` (true) | Axis lines in `CENTER` with numbered bubbles, and dimension chains between the axes |
| `northArrow` | `at`, `size` (15), `angle`, `label` (`N`) | `angle` turns north counterclockwise from up |
| `scaleBar` | `at`, `length` (model), `divisions` (5), `unit` (`m`, `mm`), `height` | For a view: the length is real |
| `levelMark` | `at`, `text`, `size` (3), `line`, `textHeight` | ▽ with its tip at `at` and the text to the right; `line` draws the level line |
| `spotLevel` | `at`, `text`, `textHeight` | A dot with the height beside it, as survey drawings show spot heights |
| `leader` | `points` (tip first), `text`, `arrow`, `arrowSize`, `textHeight` | Ends with a horizontal shelf under the text |
| `polygonLabel` | `points`, `lengths` (true), `unit` (`m`), `precision`, `area`, `areaPrecision` (2), `areaAt`, `outline` | Writes the length of each side outside it and, with `area` (true or a format such as `"敷地面積 {}㎡"`), the area in m² |
| `viewTitle` | `number`, `text`, `scaleText`, `at`, `height` | The same title as a view's `title`, anywhere |
| `group` | `items` | Shares `layer`, `color`, `lineType`, `lineWidth` with its items |

## Mistakes

A spec that cannot be drawn raises an error with the place of the mistake and a code that does not change:

| Code | Meaning |
| --- | --- |
| `invalid-spec` | The document's shape is wrong (not an object, no `sheets`, a name used twice, a required key missing) |
| `unknown-type` | An item's `type` is not one of the types above |
| `unknown-key` | An item or object has a key it does not take |
| `invalid-value` | A value is out of range or of the wrong shape (a negative radius, a cell spanning past the table, an alignment that does not exist) |
| `missing-definition` | A block, line type or hatch pattern that is not defined |
| `unsupported-format`, `unwritable` | The output format or version cannot be written |

The place is a path such as `sheets[0].views[1].items[4]` or `sheets[0].items[2].cells[3][1]`. Nothing is written
when there is a mistake.

## What each format keeps

- **DXF 2000–2018 and DWG**: everything, with one layout per sheet, dimensions as DIMENSION entities with their
  blocks (each sheet scale gets a dimension style `ZUMEN-1-<scale>`), hatches with their pattern lines, MTEXT,
  attributes. ezdxf audits the result without findings.
- **DXF R12**: one paper space, so only the first sheet; hatches become lines and solids, MTEXT becomes TEXT, line
  weights are dropped. The notices list each.
- **JWW and SXF**: one sheet per file (`--sheet` / `sheet=`); dimensions become the lines and text they draw, viewports
  become layer groups or sub-figures at their scales. The notices list what changed. SXF writes text in CP932, which
  has no `Ø`: diameter values carry `?` there and the writer reports it (`encoding`).

Text widths are estimated (full-width characters as wide as the text is high divided by 0.88, half-width ones half
that), so `fit`, wrapping and table cells leave room for fonts that differ from the estimate. Dimensions are drawn by
zumen; AutoCAD draws a dimension again from its style when someone edits it.

Text and attribute values are literal: `%`, `^`, braces and backslashes are preserved, including in wrapped or
multiline text. To show a diameter symbol, give `Ø` rather than a CAD control code such as `%%c`.

## Drawing into a template

`zumen::draft_into` draws the spec into a drawing received from someone else, a company template with its sheet and
title block, without rewriting it: DXF, DWG, JWW and SXF (SFC, P21, SFZ, P2Z). The drawing is drawn the same way as by
`draft`, then turned into the edit operations of [decision 0017](decisions/0017-add-annotations-and-definitions.md)
(`zumen_draft::plan`, `plan_jww` and `plan_sxf`) and applied with `zumen::edit`: the template's entities keep their
handles and bytes, extended data and objects that zumen does not read stay, and the format and version are the
template's.

```sh
zumen draft plan.json --into template.dwg -o A-101.dwg
zumen draft plan.json --into template.jww -o A-101.jww --sheet A-101
```

```python
zumen.draft_into(spec, "template.dwg").save("A-101.dwg")      # template: bytes or a path
```

The MCP tool `draft_drawing` takes `into` for the same. The rest of this section is about DXF and DWG templates;
[JWW and SXF templates](#jww-and-sxf-templates) follows.

- **Sheets**: a sheet named after a layout of the template is drawn into that layout, with that layout's paper size
  unless the sheet gives `paper`; an explicit `paper` updates the existing layout's size and its whole-sheet viewport.
  Its coordinates are the layout's own (AutoCAD places them from the corner of the
  printable area, inside the printer margins). Other sheets become new layouts (`add-layout`, with no margins, as
  `draft` writes them). Leave out `frame` when the template has its border.
- **Title block**: `fields` (`{"TAG": "value"}`) of a sheet fill the attributes with those tags of every block
  reference on the sheet, the template's and the spec's. A tag that no block reference has is reported
  (`unused-field`). `draft` fills the spec's block references the same way.
- **Names**: layers, line types, blocks and dimension styles that the template has are used as the template defines
  them (a block of the same name is reported, `draft-into-block-exists`); a text style is reused when its font file,
  TrueType family, bigfont, width, fixed height, oblique angle and flags match. Otherwise a style named after the
  font is added, preserving its TrueType family. The others are added.
- **Dimensions** become DIMENSION operations, and the editor draws their blocks from a dimension style added for each
  scale and look. Dot and empty end marks have no dimension style variable and become arrowheads
  (`draft-into-end-mark`); values with thousands separators, and angles with decimals (the added style keeps
  DIMADEC at 0), are given as the dimension's text. Angular dimensions become `angular-3-point`, radius and diameter
  dimensions `radius` and `diameter`.
- **Solids** (arrowheads, filled marks) become solid hatches.
- The template must be in millimeters (`$INSUNITS` 4 or unitless), and layouts need AutoCAD 2000 or later. An
  operation that the drawing refuses is reported with the place in the spec it came from.

### JWW and SXF templates

[Decision 0027](decisions/0027-draft-into-jww-and-sxf.md) draws into JWW and SXF drawings through their own edits
(`zumen_draft::plan_jww`, `zumen_draft::plan_sxf`), so dimensions, hatches and blocks become the format's dimension
figures and features, drawing parts and fills. What these formats cannot hold is drawn as close as they allow and
reported in the notices, with these codes:

- **One sheet.** A JWW or SXF drawing has one sheet, so the first sheet of the spec, or the one `sheet` names, is drawn;
  the others are not (`draft-into-sheets`). The sheet is drawn on the template's paper (an SXF drawing without a sheet
  takes the spec's). No operation changes the paper of these formats, so a `paper` or `orientation` that differs is a
  warning (`draft-into-paper`) and the frame follows the template's paper. Sheet coordinates are the same as in DXF:
  millimeters from the lower left corner of the paper.
- **Views.** In JWW each view goes into a layer group whose scale is the view's: the write layer group if its scale
  matches, else the first other group of that scale, else the first group without entities, whose scale is set (a hidden
  or display-only group is not used, and views of one scale share a group). Sheet items go into a group of scale 1 the
  same way. Coordinates are moved so that the view's `center` lands at the middle of its viewport on the JWW sheet; text
  sizes are divided by the group's scale, since JWW keeps them in paper millimeters. A template without such a group is
  an error at the view (`invalid-value`). In SXF each view becomes a sub-figure (部分図) named after the view, placed at
  its viewport at 1:`scale` with the new `viewport` edit, and its items are drawn into it at real size. Layer groups and
  sub-figures do not clip, so a view whose items pass its viewport is reported (`draft-into-view-clip`); since each view
  has its own group or sub-figure, `view-overlap` does not apply.
- **Layers** of the spec become layers of the item's group in JWW (layer names belong to their layer group, as in
  Jw_cad) and layers of the drawing in SXF, reusing a layer of the same name; layer `0` is a layer named `0`, not the
  template's current layer. A new JWW layer is the first layer of the group without a name and entities, named by the
  planner. Layers take `off` (and `locked` in JWW); `plot: false`, and `locked` in SXF, are reported
  (`draft-into-layer-attributes`). JWW and SXF have no BYLAYER or BYBLOCK, so every item carries its color (`#rrggbb`,
  ACI 7 as black), line type and line width.
- **Line types** cannot be added to these drawings yet, so a line type of the spec becomes the drawing's line type with
  the same dash and gap lengths, or the nearest one (`draft-into-line-type`): Jw_cad's line types 2–9 and, from version
  4.20, the SXF line types 31–62 in JWW; the predefined SXF line types and the drawing's own in SXF.
- **Text** is one line placed by the lower left of its box: each line of a text becomes a text whose box (the text
  height, and characters as wide as the height times the width factor, half-width ones half of that) is placed by the
  text's alignment, 0.12 of the height below the baseline, as
  [decision 0019](decisions/0019-text-heights-between-formats.md) keeps text heights between formats. JWW and SXF draw
  glyphs to the box, so they are narrower than the glyphs zumen draws for DXF.
- **Polylines** become lines and arcs in JWW, and polylines and arcs in SXF; a polyline with a width becomes a solid
  fill for each segment (a filled circle is two half disks).
- **Blocks**: only the blocks that the drawn sheet places are added (the template holds that sheet only). A block's
  entities go on the layers of the group of its first reference, which also gives the values that layer `0` and BYBLOCK
  take; references that would give others are reported (`draft-into-block-inherit`). A block the template has is used as
  it is defined there (`draft-into-block-exists`).
- **Attributes**: the values of block references are drawn as text where they would be, and attribute definitions are
  not written (`draft-into-attributes`). `fields` fill the spec's block references; the template's title block is text
  in these formats, so its fields are reported (`unused-field`).
- **Dimensions** carry their dimension style's name, which the editors report (`edit-dimension-style`). JWW dimensions
  follow the template's dimension settings, value format included, as Jw_cad's do; SXF dimensions take the value as the
  spec formats it, with `φ` before diameters (CP932 has no `Ø`). JWW dimension figures cannot be angular, so angular
  dimensions are drawn as the lines, arcs, fills and text of their block (`draft-into-angular-dimension`).
