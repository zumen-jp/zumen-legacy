# 0025: Add block definitions, dimensions and hatches to JWW and SXF edits as each format's own entities

> English reference preserved when Japanese became the primary documentation. See [0025-annotations-in-jww-and-sxf-edits.md](0025-annotations-in-jww-and-sxf-edits.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0017](0017-add-annotations-and-definitions.md) let DXF and DWG edits add block definitions (`add-block`,
  `"container": "block:<name>"`), dimensions (`linear`, `aligned`, `angular-3-point`, `radius`, `diameter`) and
  hatches (`SOLID`, built-in patterns, `patternLines`), and said that SXF and JWW would take the same operations when
  their editors gain these entities, refusing them with `unsupported-kind` until then. A script that draws a plan with
  dimensions and fills could not be run on the formats most Japanese drawings are exchanged in.
- **JWW** ([jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt), Jw_cad 7.02):
  - A dimension figure (`CDataSunpou`) holds a dimension line, a value text and, from version 4.20, an SXF mode, two
    extension lines (補助線), two arrow points and two reference points (基準点). It has no arc, so it cannot hold an
    angular dimension. Jw_cad's own dimensions drawn without dimension figures are separate lines (attribute flag
    0x2000), real points at the ends (0x0040) and a value text (0x4010) — as in `Ａマンション平面例.jww`.
  - The header stores the drawing's dimension settings (`m_lnSunpou1`–`5`: the colors of the dimension line, the
    extension lines and the points, decimals, mm or m, dot or arrow ends, the text style, the gap between the value and
    the line, how far extension lines pass the line, the rounding and the marks of radius values) and the text styles
    1–10 (width, height, spacing, color). Every sample at hand stores them, mostly Jw_cad's defaults
    (`2002111, 5, 50150030, 11000000, 4100`).
  - A block definition (`CDataList`) has a number, a name and its entities in paper millimeters, with the base point at
    its origin; there is no base point value. Jw_cad numbers definitions from 0 and, from version 4.10, saves the names
    of its blocks with `@@SfigorgFlag@@4` (a drawing part, 作図部品) appended.
  - There is no hatch entity. Jw_cad's ハッチ command draws lines with the attribute flag 0x0020, and its fills are
    solids (`CDataSolid`, quadrilaterals). zumen-convert writes DXF hatches into JWW the same way: fills as horizontal
    trapezoid solids and pattern lines cut to the hatch area.
- **SXF** (Ver.3.1, 附属書 SFC 編 1-2 and 実装規約):
  - The dimension features (`linear_dim_feature`, `angular_dim_feature`, `radius_dim_feature`,
    `diameter_dim_feature`) keep what is drawn: the dimension line or arc, each extension line with its origin, start
    and end, the arrows (code, inward or outward, tip and scale; their size does not follow the placement's scale) and
    the value text with its box and anchor. There are no dimension styles.
  - A composite figure definition of kind 4 (`sfig_org`) is a drawing part, placed by `sfig_locate`; it has no base
    point either.
  - Fills (`fill_area_style_colour`) and user-defined hatching (`fill_area_style_hatching`) refer to composite curves
    for their outline and holes. A hatching line has a color, line type, width, base point, spacing and angle, but no
    shift along the line between successive lines, and its line type is in sheet millimeters.
- Format crates do not depend on each other, and the reading of the dimension and hatch values and the built-in
  patterns were in zumen-dxf.

## Options

**Where the shared reading lives**

1. **Copy it** into zumen-jww and zumen-sxf: three readings of the same keys that can drift apart.
2. **Move it into `zumen_core::edit`** (`dimension`, `hatch`), which already holds the operations and checks their
   values for every format. Formats only map the result into their entities.

**JWW dimensions**

- **Separate lines, points and text**, as Jw_cad draws dimensions without dimension figures: one operation would add
  several entities, and the drawing read again would have no dimension.
- **One dimension figure in SXF mode**, with its extension lines, end points and reference points: one entity, read
  again as a dimension, and the parts Jw_cad keeps in a dimension figure for SXF exchange.
- **Sizes and colors from fixed values**, or **from the drawing's dimension settings and text style**, as Jw_cad uses
  them for a new dimension.
- **Arrow ends**: the settings can ask for arrows or reverse arrows, but a dimension figure holds points, and the point
  codes of Jw_cad's arrows are not described in jwdatafmt.txt.

**JWW angular dimensions**: refuse them, or write an arc, points and a text as separate entities (not a dimension when
read again).

**JWW hatches**

- **Refuse them**: no fills or hatching in JWW from any script.
- **SOLID as solids** and patterns refused.
- **SOLID as solids and patterns as hatch lines and points cut to the boundary**, what Jw_cad and zumen-convert write:
  one operation adds several entities, and `created` names only one of them.

**SXF dimensions**: fixed sizes or sizes from somewhere else; arrows as filled arrows (like the closed filled arrowheads
of decision 0017) or another code.

**SXF pattern dashes**: drop them, or write them as a user-defined line type.

## Decision

Option 2 for the shared reading: `zumen_core::edit::dimension` reads the dimension values (`DimensionInput`) and
gives the format-neutral shape (`geometry`: where the dimension line or arc runs, the feet of the extension lines,
what is measured), and `zumen_core::edit::hatch` reads the hatch values, holds the built-in patterns and scales and
turns pattern lines. zumen-dxf re-exports both; its 70 flags and display blocks stay in zumen-dxf.

**JWW** (`edit_jww`):

1. **`add-block`** adds an empty definition numbered after the largest number (0 in a drawing without blocks), named
   with `@@SfigorgFlag@@4` appended from version 4.10 as Jw_cad saves it, and not yet referenced. Its `basePoint` is
   kept for the edit and subtracted from the coordinates of the entities added with `"container": "block:<name>"`, so
   that a block reference puts the base point at its position. `block:<name>` takes any definition of the drawing; a
   block reference that would place a block inside itself is `invalid-value`. `created` gives the name for the block
   and `block:<name>>#<n>` (the place in the definition) for an entity added into it.
2. **Dimensions** (`linear`, `aligned`, `radius`, `diameter`) are one dimension figure in SXF mode (1). Its colors,
   value text style and size, gap and extension past the line come from the drawing's dimension settings, or Jw_cad's
   defaults when the header stores none, and its value is formatted by them (decimals, rounding, mm or m, a comma every
   three digits, full-width options, the unit, `R` and `φ` before or after). The value is centered on the dimension
   line, readable (unless the settings turn that off) and the settings' gap above it, with the text style's size, the
   anchor 2 (bottom center) and the flags Jw_cad gives dimension values. Extension lines run from their origins to the
   line and past it by the settings' length; the end points are real points (the center of a radius is a temporary
   point) and the reference points temporary points at the origins. Arrow ends are written as points and reported
   (`edit-dimension-end`); `dimensionStyle` is reported (`edit-dimension-style`), since JWW names no styles. Before
   version 4.20 the figure holds only the line and the value (`edit-dimension-parts`).
3. **Angular dimensions** are refused (`unsupported-kind`).
4. **Hatches**: `SOLID` becomes trapezoid solids by the even-odd rule (one solid for a convex quadrilateral), with
   `#rrggbb` as the solid's own color; patterns become hatch lines and points (flag 0x0020) cut to the boundary, with
   the pattern's dashes applied from its base point. `created` gives the first entity, and the others follow it in the
   entity list (`edit-fill-solids`, `edit-hatch-lines`). More than 20,000 pieces, or no piece inside the boundary, is
   `invalid-value`.

**SXF** (`edit_sxf`):

1. **`add-block`** adds a drawing part (composite figure of kind 4). Its `basePoint` is subtracted from the
   coordinates of what is added with `"container": "block:<name>"` (`block:` takes any composite figure, as
   `figure:` does), and a placement that would make a cycle is `invalid-value`. Names starting with `$$` are refused:
   they belong to the attribute mechanism and the P21 prefixes.
2. **Dimensions** become the dimension features, with the style (layer, color, line type, width) of the operation:
   extension lines from 0.625 mm beyond their origins to 1.25 mm past the line, filled arrows (code 6) 2.5 mm long
   pointing outward, and the value 2.5 mm high, anchored at its bottom center 0.625 mm above the middle of the line or
   arc, readable, with no decimals (`R` and `φ` before radius and diameter values, `°` after angles). These are
   AutoCAD's metric (ISO-25) values, so a dimension looks the same as one added to a metric DXF. Sizes are sheet
   millimeters divided by the scale of the first placement of the figure from the sheet (nested placements multiply,
   drawing groups and unplaced figures count as 1), and the arrows' scale is not divided, as SXF draws them.
   An angular dimension draws extension lines only from origins inside its arc, as in DXF. `dimensionStyle` is
   reported (`edit-dimension-style`).
3. **Hatches** get one invisible composite curve per boundary (a closed polyline), the first the outline and the others
   holes. `SOLID` is a fill with the operation's color; patterns are user-defined hatching with one hatching line per
   pattern line (base point moved near the boundary on the same line, spacing across the lines, angle). Dashes become a
   user-defined line type in sheet millimeters (dots as 0.25 mm dashes, as zumen-convert writes them), shared with a
   line type of the same lengths, or solid when the 16 user-defined line types are taken (`edit-hatch-dashes`). The
   shift of dashes from one line to the next cannot be written and is reported (`edit-hatch-shift`).

## Consequences

- The operations of decision 0017 for blocks, dimensions and hatches work in DXF, DWG, SXF and JWW, from `create` or
  into a received drawing, with two exceptions: JWW has no angular dimensions, and JWW and SXF still refuse the style
  ops (`add-text-style`, `add-dimension-style`, `add-line-type`), so their dimensions are sized by the drawing's
  settings (JWW) or the values above (SXF).
- A JWW hatch is several entities, as in Jw_cad: changing or deleting it means each of them. The ids after the one in
  `created` are the hatch's, in the edited drawing.
- JWW dimensions follow the settings of the drawing they are added to, so the same operation draws differently sized
  dimensions in two JWW drawings, as Jw_cad would. Their values have commas and two decimals by Jw_cad's defaults,
  unlike the default values of SXF and DXF.
- `JwwDimension` gains `sxf_mode`, which the reader keeps and the writer writes, instead of taking it from the original
  bytes. `zumen_dxf::dimension::DimensionInput` and `zumen_dxf::hatch` are re-exports; `HatchInput::pattern_type`
  becomes `custom`, and the 70 flags and the display block of a dimension are the `DxfDimension` trait.
- Neither format's result has been opened in Jw_cad or checked with the SXF browser's checking function yet; that is
  part of the checks on a Windows host ([decision 0014](0014-verify-on-native-windows.md)). Jw_cad's handling of
  dimension figures in SXF mode written by others, and the meaning of the dimension value flags that jwdatafmt.txt
  leaves open, are inferred and written down in [How JWW is handled](../formats/jww.md#editing-edit_jww).

## References

- Jw_cad, [jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt) (Jw_cad 7.02): 寸法関係の設定, CDataSunpou, CDataMoji
  (寸法値設定のフラグ), CDataTen, CDataSolid, CDataBlock, CDataList, 属性フラグ
- MLIT, SXF Ver.3.1 仕様書・同解説: 附属書 SFC 編 1-2 (linear_dim_feature, angular_dim_feature, radius_dim_feature,
  diameter_dim_feature, sfig_org_feature, fill_area_style_colour_feature, fill_area_style_hatching_feature,
  composite_curve_org_feature, user_defined_font_feature) and 図 19 (arrows), 実装規約 1-4, 1-6 and 1-7
- [Decision 0017](0017-add-annotations-and-definitions.md), [decision 0012](0012-edit-in-place-and-write-every-format.md)
- [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT), `docs/WRITER_EXTENSIONS.md`: the members of `CDataSunpou` as
  that writer fills them
