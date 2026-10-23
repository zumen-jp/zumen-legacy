# 0026: Let JWW and SXF edits add dimension styles, text styles and line types, named for the edit and written into what each format stores

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0025](0025-annotations-in-jww-and-sxf-edits.md) let JWW and SXF edits add dimensions, but left the style
  ops of [decision 0017](0017-add-annotations-and-definitions.md) (`add-dimension-style`, `add-text-style`,
  `add-line-type`) refused with `unsupported-kind`. A JWW dimension is sized and formatted by the drawing's dimension
  settings, an SXF dimension by fixed ISO-25 sizes, and `dimensionStyle` is only reported. An application cannot make
  the values of a 1:50 plan larger, choose the decimals or end the dimension lines with ticks, and a script that runs
  on DXF (such as the operations `draft_into` makes) fails on JWW and SXF at its first style.
- In DXF and DWG, `add-dimension-style` writes a DIMSTYLE that later dimensions name. Its lengths are multiplied by
  DIMSCALE (`scale`) in drawing units; AutoCAD reads a DIMSCALE of 0 as "scale to the viewport".
- **JWW** ([jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt), Jw_cad 7.02):
  - A drawing has no named dimension styles: the header holds one set of dimension settings (`m_lnSunpou1`–`5`), which
    Jw_cad reads from the file only when its environment setting allows it (S_COMM_8), and text styles 1–10 (width,
    height, spacing and color, no font). Each dimension figure stores its own line, value text (size, spacing, font,
    text style number, value flags), extension lines and points, so two figures of one drawing can differ in size.
    It has no arrows: its ends are points, and the point codes of Jw_cad's arrows are not described.
  - A text has its own font name; there is no font table.
  - From version 4.20 the header holds 33 SXF line types (30–62): 31–45 are SXF's predefined ones and 46 is
    `undefined`; 47–62 are free slots with a name, a segment count and up to 10 lengths in millimeters (and a bit
    pattern for the screen). Every sample at hand leaves 47–62 empty, and zumen-convert writes user line types there.
- **SXF** (Ver.3.1, 附属書 SFC 編 1-2):
  - There are no dimension styles either; each dimension feature stores its extension lines, arrows (code, direction,
    position and a scale applied to the arrow's size on the sheet) and value text (font, height, box width, anchor).
    The arrow codes include filled arrows (6) and slashes (10), 7.5 mm and 2.5 mm long at scale 1 (図 19).
  - Fonts are a table of names (`text_font_feature`), which texts refer to by code.
  - Up to 16 user-defined line types (`user_defined_font_feature`, codes 17–32) have a name and 2, 4, 6 or 8 lengths
    in sheet millimeters, alternating line and gap (the rule [ezsxf](https://github.com/monozukuri-ai/ezsxf) (MIT)
    checks as well).

## Options

**Where a dimension style lives**

1. **In the drawing's settings**: JWW's header settings and a text style. JWW holds one set without a name, so a second
   style replaces the first; Jw_cad reads it only with some environments and uses it for its own next dimensions; SXF
   has nowhere to put it.
2. **Named for the edit**: the editor keeps the style until the end of the edit and writes its sizes and format into
   each dimension that names it, which both formats store per dimension anyway. The file keeps no style: a later edit
   adds it again, as an application keeps its own settings.
3. **Keep refusing** the ops.

**What `scale` means**

- **A magnifier of paper millimeters**: the same operations would draw dimensions 100 times larger in JWW and SXF
  than in DXF when `scale` is 100.
- **DIMSCALE**: with `scale`, the lengths times `scale` are in the drawing's own coordinates, as in DXF; without it,
  they are paper millimeters, converted to the layer group or the sub-figure as decision 0025 does (AutoCAD's DIMSCALE
  0).

**Values a format cannot hold**: refuse the style, or write the nearest value and report it.

**Text styles and line types**: refuse them, or map them onto what the formats have: SXF's fonts and user-defined line
types, JWW's font names and free SXF line types.

## Decision

Option 2 for dimension styles, DIMSCALE for `scale`, the nearest values with notices, and text styles and line types
mapped onto each format's own. The values of the three ops are read by `zumen_core::edit::style`
(`DimensionStyleInput`, `TextStyleInput`, `LineTypeInput`), with the keys and errors of DXF; the DXF and DWG editors
read them there too, and write what they wrote before.

**Names**: styles and line types added in an edit are compared without case, as in DXF. A name added twice, or the
name of a font (SXF) or line type (both) the drawing has, is `already-exists`; empty names and control characters are
`invalid-value`. `created` gives the name.

**Dimension styles** (both formats):

1. `from` names a dimension style added earlier in the same edit. Without `from`, a style starts from the format's
   base: the drawing's dimension settings in JWW (Jw_cad's defaults when it stores none) and the ISO-25 values of
   decision 0025 in SXF. Another name, such as `Standard`, also starts from the base and is reported
   (`edit-dimension-style`), as `dimensionStyle` was in decision 0025.
2. `dimensionStyle` of a dimension names a style added earlier in the same edit, which draws it without a notice;
   other names draw it with the base and are reported (`edit-dimension-style`), as before.
3. The lengths (`textHeight`, `arrowSize`, `extensionOffset`, `extensionExtension`, `textGap`, `tickSize`) are paper
   millimeters while `scale` is not given or 0, and then follow the scale of the dimension's layer group (JWW) or of
   the figure's placement (SXF), as in decision 0025. With `scale`, the lengths times `scale` are in the drawing's own
   coordinates: real millimeters of the layer group (JWW) or the coordinates of the figure the dimension is added to
   (SXF). A dimension with `scale` 100 in a 1:100 layer group or sub-figure looks as it does without `scale`.

| Key | JWW (dimension figure) | SXF (dimension feature) |
| --- | --- | --- |
| `textHeight` | Height of the value (above 0: a text of height 0 cannot be written); its width and spacing are those of the settings' text style, scaled in proportion; the text style number becomes 0 (any size) unless the size is the style's | Height of the value (above 0); the box width follows from the characters |
| `arrowSize` | Not drawn: the ends are points (`edit-dimension-end`) | Filled arrows (code 6) that long, scale `arrowSize` / 7.5 on the sheet; 0 is no arrow |
| `tickSize` | Not drawn (`edit-dimension-end`) | Above 0, slashes (code 10) instead of arrows, scale `tickSize` × √2 / 2.5, the oblique stroke of DXF |
| `extensionOffset` | The extension lines start that far from their origins (0 in the base); the reference points stay at the origins | Gap between the origin and the extension line |
| `extensionExtension` | Overshoot of the extension lines (突出長さ) | Length of the extension lines past the dimension line |
| `textGap` | Gap between the value and the line (離れ) | Gap between the value and the line |
| `textAbove` | `false`: the value centered on the dimension line, which is not broken (`edit-dimension-text-position`) | Same |
| `decimals` | 0–3, in the value flags (0x1000 × decimals); 4–8 are written as 3 (`edit-dimension-decimals`) | 0–8 |
| `suppressTrailingZeros` | `true` removes trailing zeros, `false` keeps them and sets 0x0080 (少数点以下の０表示) on every value; not given, Jw_cad's rule (zeros only for radius values when the settings ask) | Removes or keeps trailing zeros (removed in the base) |
| `linearFactor` | The measurement is multiplied by its absolute value before it is formatted | Same, for lengths, radii and diameters; not for angles, as DIMLFAC |
| `textStyle` | Font of the value: the font of a text style added in the edit, or else a font of that name | Font of the value: the font of a text style added in the edit, or else a font of that name (added to the table when missing), as `textStyle` of `text` |

Angles keep whole degrees, as DIMADEC and DIMAZIN are not keys of decision 0017. Values formatted by a style are text
in both formats: Jw_cad and SXF programs do not keep the factor or the decimals as a style.

**Text styles**: SXF adds the font `font` (the name when not given) to the font table unless the drawing has it, and
JWW writes nothing, since its texts carry font names. In both, the name stands for that font in later `textStyle`
values of the same edit: of dimension styles, and of `text` (SXF's `textStyle` was already a font name; JWW's `text`
gains `textStyle`, which cannot be given with `font`). `bigFont`, a `height` other than 0, a `widthFactor` other than 1
and an `oblique` other than 0 have nowhere to go, as zumen's DXF texts do not take them from their style either, and are
reported (`edit-style-values`).

**Line types**: the lengths of `pattern` are paper millimeters, as both formats keep them. They become line and gap
lengths starting with a line, by the rules of decision 0025's hatch dashes and zumen-convert: dots are 0.25 mm lines
with the gap after them shorter by as much, a leading gap moves to the end and a trailing line joins the first.
SXF keeps at most 8 lengths and JWW 10; longer patterns are cut to whole pairs. A changed pattern is reported
(`edit-line-type-pattern`), and `description` has nowhere to go (`edit-style-values`).

- SXF adds a user-defined line type (code 17–32); when the 16 are taken, `name-exhausted`. A predefined name is
  `already-exists`.
- JWW (version 4.20 and later; earlier versions refuse the op with `unsupported-kind`) takes the first of 47–62 that
  has no name, no lengths and no entity using it, and writes the name, the segment count and the lengths, as
  zumen-convert does; when none is free, `name-exhausted`. Names that are numbers are `invalid-value`, since a number
  in `lineType` is a line type number.
- A pattern without gaps (empty or only lines) is a continuous line: the name stands for the continuous line type
  (SXF 1, JWW 1) in the same edit and nothing is written.

## Consequences

- The style ops of decision 0017 work in DXF, DWG, JWW and SXF, so one list of operations (from `draft_into` or a
  script) draws sized dimensions, fonts and line types in each format.
- A JWW or SXF drawing keeps no dimension or text style names: reading it again gives dimensions with their sizes, and
  an application that edits it again adds its styles again. Line types and SXF fonts stay in the file with their names.
- JWW dimensions drawn with a style no longer follow the drawing's settings for the keys the style sets. Jw_cad may lay
  out a dimension figure again from its settings when it is edited there; that, the handling of value flag 0x0080 on
  lengths, and of line types written without a screen pattern, are inferred and not yet checked in Jw_cad, nor the
  SXF results with the SXF browser's checking function ([decision 0014](0014-verify-on-native-windows.md)).
- `zumen_dxf::dimension::STYLE_KEYS` and `style_variables` read their values from `zumen_core::edit::style`, and the
  hatch dashes of SXF use its conversion to lengths; DXF, DWG and SXF hatch output is unchanged.

## References

- Jw_cad, [jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt) (Jw_cad 7.02): 寸法関係の設定, SXF対応拡張線種定義,
  文字種1から10, CDataMoji (寸法値設定のフラグ), CDataSunpou
- MLIT, SXF Ver.3.1 仕様書・同解説: 附属書 SFC 編 1-2 (linear_dim_feature, angular_dim_feature, radius_dim_feature,
  diameter_dim_feature, text_font_feature, user_defined_font_feature) and 図 19 (arrows)
- [ezsxf](https://github.com/monozukuri-ai/ezsxf) (MIT), `docs/sfc-writer-mvp.md`: user-defined line types and fonts
- AutoCAD System Variables: DIMSCALE, DIMTXT, DIMASZ, DIMTSZ, DIMEXO, DIMEXE, DIMGAP, DIMTAD, DIMDEC, DIMZIN, DIMLFAC,
  DIMTXSTY
- [Decision 0017](0017-add-annotations-and-definitions.md), [decision 0025](0025-annotations-in-jww-and-sxf-edits.md)
