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
default), and:

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

## Drawing into a template

`zumen::draft_into` draws the spec into a DXF or DWG drawing received from someone else, a company template with its
layouts and title block, without rewriting it. The drawing is drawn the same way as by `draft`, then turned into the
edit operations of [decision 0017](decisions/0017-add-annotations-and-definitions.md) (`zumen_draft::plan`) and
applied with `zumen::edit`: the template's entities keep their handles and bytes, extended data and objects that zumen
does not read stay, and the format and version are the template's.

```sh
zumen draft plan.json --into template.dwg -o A-101.dwg
```

```python
zumen.draft_into(spec, "template.dwg").save("A-101.dwg")      # template: bytes or a path
```

The MCP tool `draft_drawing` takes `into` for the same.

- **Sheets**: a sheet named after a layout of the template is drawn into that layout, with that layout's paper size
  unless the sheet gives `paper`. Its coordinates are the layout's own (AutoCAD places them from the corner of the
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
