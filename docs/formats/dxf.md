# How DXF is handled

`@zumenjs/dxf` reads DXF written by AutoCAD R12 (AC1009) to AutoCAD 2018 and later (AC1032), in ASCII and binary form,
following the [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm)
(AutoCAD 2026, checked 2026-10-18). It is written in TypeScript without dependencies other than `@zumenjs/core`.
`zumen-dxf` (`crates/zumen-dxf`) is its Rust port and returns the same scenes
(see [verification.md](../verification.md#dxf-in-rust)).

```text
bytes ─▶ readTags (ASCII or binary, decoded) ─▶ parseDxf ─▶ DxfDocument ─▶ dxfToScene(doc, { layout }) ─▶ Scene
                                                                      └─▶ write_dxf (Rust) ─▶ DXF (AutoCAD 2013)
```

## The AutoCAD drawing model

`DxfDocument` is the AutoCAD drawing database as plain data. It is shared with the DWG reader: `zumen-dwg` reads a DWG
file into the same `DxfDocument` of `zumen-dxf`, drawn by the same `dxf_to_scene` (see [How DWG is handled](dwg.md)).

- Attribute names follow [ezdxf](https://ezdxf.mozman.at/)'s DXF attribute names (`start`, `center`, `start_angle`, `insert`,
  `major_axis`, `ratio`, `char_height`, `attachment_point`, `defpoint2`…). Where ezdwg's high-level API uses a different name than ezdxf,
  the ezdwg name is used (`color_index`, `linetype_scale`, `raw_text`, `view_center`, `view_target`, `status_flags`,
  `frozen_layer_handles`, `anonymous_block_name`).
- Coordinates stay in drawing units (`$INSUNITS`) and angles in degrees, as in DXF. Points are `[x, y, z]` arrays.
- All entities of model space, paper space and block definitions are in one list, told apart by `space`
  (`"model"`, `"paper"`, `"block"`) and `block` (the block or paper space block name), like ezdwg's documents.
  Attributes of an `INSERT` are in its `attribs`.
- `TEXT` keeps control codes such as `%%c`; `MTEXT` has the plain `text` and the formatted `raw_text`. A multi-line
  `ATTRIB` or `ATTDEF` of AutoCAD 2018 keeps its embedded MTEXT in `mtext` (`DxfEmbeddedMText`: the values of an MTEXT
  and its extrusion).
- `MULTILEADER` (and its other name `MLEADER`) follows ezdxf's `MultiLeader`: the common values (`leader_type`,
  `leader_line_color`, `arrow_head_handle`, `block_attribs`…) and `context` with the content (`mtext` or `block`) and
  `leaders`, each with its `lines`. Colors are kept as the raw 32-bit values (`0xC0…` BYLAYER, `0xC1…` BYBLOCK, `0xC2RRGGBB`, `0xC3…` ACI),
  and line types, text styles and blocks as handles, as the entity stores them.
- `MLINE` uses ezdwg's names (`points`, `vertex_directions`, `miter_directions`, `scale`, `justification`, `base_point`,
  `mlinestyle_handle`) plus ezdxf's `style_name`, `line_params` and `fill_params` (groups 41 and 42 per vertex and element).
- Tables: `layers`, `linetypes`, `styles` (text styles), `dimstyles`, `mline_styles` (MLINESTYLE, optional), `blocks`, `layouts`;
  `header` has the variables used for drawing plus every raw variable in `header.variables`;
  `sort_handles` keeps SORTENTSTABLE (draw order); `skipped` counts entity types that are not read.

`zumen-dwg` fills the model with the values AutoCAD writes to the DXF of the same drawing: handles as upper-case
hexadecimal strings, POLYLINE and DIMENSION types as DXF flags, angles in degrees, block names as in DXF. Its rules and
the differences that remain are in [How DWG is handled](dwg.md).

## Reading

- **Detection** looks at the content: the binary sentinel (`AutoCAD Binary DXF\r\n\x1a\0`), or a first tag `0`/`SECTION`
  after optional `999` comments.
- **Binary DXF**: group codes are 2 bytes (1 byte and the `255` escape in R12); values are read by the type of the group code range
  ([Group Code Value Types Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm)).
  Strings are read into the form that ASCII DXF uses
  ([About ASCII Control Characters in DXF Files](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-77D34F88-73CC-495D-B48C-37628EE61D73.htm)):
  a control character becomes `^` and the character 64 above it (`^J` for a line feed, `^I` for a tab) and `^` becomes `^ `.
  Binary DXF keeps them as they are, so without this the line feeds that AutoCAD 2004 and later write in MTEXT (R2000
  writes `\P`) would not break paragraphs. The model therefore holds the same strings for ASCII and binary DXF, and for
  DWG. ezdxf writes `^` without the space in ASCII DXF as well; MTEXT stacks (`\S+0.1^-0.2;`) are drawn the same either way.
- **Text encoding**: AC1021 (2007) and later are UTF-8. Earlier files use `$DWGCODEPAGE` (`ANSI_932` is Japanese Shift_JIS/CP932;
  the table follows ezdxf). When the code page is missing, or is Western (`ANSI_1252`) but the bytes are valid UTF-8, UTF-8 is used.
  When it is missing and the bytes are valid Shift_JIS with kana or at least two kanji, Shift_JIS is used.
  Half-width katakana alone is not taken as evidence, because Western `°` and `Ø` are half-width katakana in Shift_JIS.
  A guessed encoding is reported in `notices`. Strings are decoded before MTEXT codes are read, so the CP932 second byte `0x5C`
  (as in 表, ソ, 能) is never taken for a backslash.
- `\U+XXXX` (CIF) and `\M+nXXXX` (MIF) escapes are decoded in all strings, except the MTEXT content, where the MTEXT reader decodes them.
- 102 groups (`{ACAD_REACTORS`…), extended data (1001 and later) and embedded objects (101) are skipped when reading entity attributes,
  except the embedded MTEXT of multi-line attributes below.
  Common attributes are taken from the `AcDbEntity` subclass only, so table cell colors of `ACAD_TABLE` are not mistaken for its color.
- `POLYLINE`/`VERTEX`/`SEQEND` and `INSERT`/`ATTRIB`/`SEQEND` sequences are assembled into one entity.
- ATTRIB and ATTDEF take the text generation flags (71) from the `AcDbText` subclass only. The
  [ATTRIB table](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-7DD8B495-C3F8-48CD-A766-14F9D7D0DD9B.htm)
  lists them in `AcDbAttribute`, but AutoCAD 2018 writes the attribute type there (1 single line, 2 multi-line
  attribute, 4 multi-line attribute definition), as ezdxf (`attribute_type`) and ACadSharp (`AttributeType`) read it;
  taken as generation flags, it drew multi-line attribute definitions upside down.
- **Multi-line attributes** (AutoCAD 2018): the text is in an embedded MTEXT, and AutoCAD leaves the attribute's own
  text (1 in `AcDbText`) empty. After `AcDbAttribute` or `AcDbAttributeDefinition` come `101`/`Embedded Object` and the
  MTEXT group codes from the insertion point (10) on, as the
  [Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf)
  5.4.1 says for the DWG form (20.4.4). The tags up to the extended data (1001) or the next embedded object are read as
  an MTEXT into `mtext` (`DxfEmbeddedMText`), with its own extrusion (a 210 there, as for MTEXT; *inferred*, as no
  sample attribute is tilted and none has one).
- **Dimension styles**: AutoCAD (R2000 and later in the samples; R12 writes every value) leaves out of DIMSTYLE the
  values equal to its defaults. A missing DIMSCALE is 1, and a missing DIMASZ or DIMTXT is 0.18, which the DWG of the
  same drawing stores, whatever `$MEASUREMENT` says: the imperial ACadSharp sample writes the 2.5 of its ISO-25 style
  and leaves out the 0.18 of the others, and ACadSharp and netDxf read missing values as 0.18 too (*inferred* for metric
  drawings, as no AutoCAD DXF of a metric drawing was at hand).
- R12 files without `LAYOUT` objects get one `Layout1` when they have paper space entities. R12 viewport settings are read
  from the `ACAD` `MVIEW` extended data, as ezdxf does, with the names of the layers frozen in the viewport (1003).

## Drawing (`dxfToScene`)

### Spaces and scales

- **Model space** is converted from `$INSUNITS` to millimeters. Without units, millimeters are assumed (inches for imperial drawings,
  `$MEASUREMENT` 0) and a notice says so. Every entity carries a `Placement` named after model space, so measured distances are
  real millimeters. Drawings whose center is more than 100 m from the origin (public coordinates in surveys) are shifted near the origin,
  because Canvas computes in single precision; the `Placement` maps back to the real coordinates.
- **Layouts** are drawn in paper millimeters: paper space units times the plot scale (142/143) and plot paper units (72),
  with the paper's lower-left corner at the origin (margins 40–43 and plot origin 46/47). Without a paper size (R12),
  the layout limits are the sheet.
- **Viewports** draw model space with the scale `height / view_height` (groups 41/45), the view twist (51), the view center (12)
  and target (17), the same transformation as ezdxf's `Viewport.get_transformation_matrix`. Lines and fills are clipped to the viewport
  rectangle; text across the border is kept whole and reported. Each entity inside a viewport has a `Placement` with the viewport scale,
  so distances measured on the sheet are converted back to real model millimeters. The overall paper space viewport (ID 1) is not
  a view, viewports switched off (68 ≤ 0 or flag `0x20000`) are skipped, layers frozen in a viewport (331, or 1003 in R12) are not drawn in it,
  and non-rectangular clip boundaries use their bounding rectangle. Viewports that do not look straight down are skipped.
- The default view is the space the drawing was saved in: the active layout when `$TILEMODE` is 0 and it has content, otherwise
  model space, otherwise the first layout with content. `Scene.views` lists model space and the layouts; pass a view to
  `dxfToScene(doc, { layout })`, or `readDrawing(bytes, [dxf], { view })`. The `dxf` reader keeps the parsed document per byte array,
  so switching views does not parse again.

### Properties

- **Colors**: true colors (420), the ACI (62), `BYLAYER`, `BYBLOCK` (the nearest block reference's color), layer colors.
  Transparency (440, and layer transparency from the `AcCmTransparency` extended data) becomes the alpha of `#rrggbbaa`.
  ACI 7 is white and turns black on a light background.
- **ACI table**: Autodesk does not publish the RGB values of ACI 10–255. Three tables circulate, all equal for 1–9:

  | Table | Shades of 10–249 (ACI 12) | Used by |
  | --- | --- | --- |
  | A | 255, 165, 127, 76, 38 (`#a50000`) | ezdxf (measured from the AutoCAD 2020 screen, with background corrections) |
  | B | 255, 204, 153, 127, 76 (`#cc0000`) | dxf-parser, dxf-viewer, dxf-render, @mlightcad/data-model — **used here** |
  | C | 255, 189, 129, 104, 79 (`#bd0000`) | the widely copied "AutoCAD Color Index RGB Equivalents" table, ezdwg |

  Table B is computed in `colors.ts` and matches the dxf-parser table for all 255 entries. It has not been compared with AutoCAD itself.
- **Layer 0** inside a block takes the layer of the block reference, for visibility and for `BYLAYER` properties.
  Layers that are off or frozen start hidden and can be shown.
- **Line types** use the LTYPE dash lengths times `$LTSCALE` and the entity scale (48), scaled with the block. In viewports,
  `$PSLTSCALE` 1 keeps the dash lengths in paper units. Shapes and text in complex line types are left out and reported.
- **Line weights** (370) are in millimeters with `BYLAYER`, `BYBLOCK` and the default (0.25 mm). In model space they are drawn
  at 1:1, so they show as hairlines until zoomed in.
- **Draw order** follows SORTENTSTABLE when the block has one, otherwise the file order.

### Entities

| Entity | Drawn as |
| --- | --- |
| LINE, POINT, CIRCLE, ARC, ELLIPSE | Lines and dots, arcs and ellipses flattened within 0.02 mm (or 1/10,000 of the radius), OCS applied |
| LWPOLYLINE, POLYLINE | Bulges as arcs; widths as filled bands; curve- and spline-fit vertices; 3D polylines, polygon and polyface meshes as wireframes |
| SPLINE, HELIX | NURBS with knots and weights (de Boor). Splines with only fit points are approximated through the points and reported |
| TEXT, ATTRIB, ATTDEF | Horizontal and vertical justification (72/73, 74 for attributes), aligned and fit text, backward and upside-down, oblique, `%%c` `%%d` `%%p` `%%nnn`, `%%u` `%%o` `%%k`, and `^ ` (a caret) and `^I` (a tab, as four spaces) of ASCII DXF. Multi-line attributes are drawn from their embedded MTEXT as MTEXT is, with the attribute's color, layer and line type and the MTEXT's own extrusion, as ezdxf's `virtual_mtext_entity` does. Hidden attributes (70 bit 1) are not drawn; an attribute definition shows its tag outside blocks (where its MTEXT is) and, inside blocks, only a constant one with its value, as for single-line attributes |
| MTEXT | Attachment point, direction or rotation, line spacing, word wrap at the reference width (at spaces, or between full-width characters), paragraph alignment, background mask, and the codes `\P` `\H` `\W` `\Q` `\C` `\c` `\F` `\f` `\L` `\O` `\K` `\S` `\~` `\U+` `\M+` and `{}` |
| INSERT (MINSERT), ACAD_TABLE | Nested blocks with base point, scale, rotation, OCS and arrays; constant attribute definitions; the table's anonymous block |
| DIMENSION, ARC_DIMENSION, LARGE_RADIAL_DIMENSION | The anonymous block (`*D…`) in the dimension's OCS, moved to group 12, merged into one selectable entity |
| HATCH, MPOLYGON | Polyline and edge boundaries (lines, arcs, ellipses, splines; clockwise edges as ezdxf reads them). Solid fills, pattern lines (with dashes) clipped to the boundary, gradients as one color |
| SOLID, TRACE, 3DFACE | Filled quadrilaterals; visible 3DFACE edges |
| LEADER | The path (straight or spline-fit) and a closed arrowhead sized by `DIMASZ × DIMSCALE` of its dimension style |
| MULTILEADER (MLEADER) | Text content (MTEXT at the stored position, with background, frame and underline or overline) or block content with its attribute values; straight or spline leaders from the arrowhead to the landing point and the dogleg, with dimension breaks; the closed filled arrowhead or the arrow block (`_Dot`, `_ArchTick`…) scaled by the arrow size. Merged into one selectable entity |
| MLINE | The elements of the MLINESTYLE at their miter offsets, without the parts cut by MLEDIT, with their colors and line types; fill, square and round caps, inner arcs and miters |
| WIPEOUT | Filled with the background color |
| IMAGE | The frame only (images are not loaded) |
| XLINE, RAY | Clipped to the extents of the other entities (or the sheet) |
| VIEWPORT | Its frame on the layout, and the model space inside |

Not read, counted in `skipped` and reported: TOLERANCE, SHAPE, 3DSOLID, REGION, BODY, MESH, SURFACE,
underlays (PDF, DWF, DGN), OLE frames, lights and proxy entities.

### Multileaders and multilines

The DXF Reference lists the MULTILEADER group codes without saying how to draw them
([Common MLeader Group Codes](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-69B9139A-48B4-48A5-B3CF-A3233ABFBE49.htm),
[Context Data](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-EC56D0DE-026D-46AB-87B1-9692393B0C22.htm),
[Leader Node](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-8648B8F7-5BD3-445B-A1B2-6F65EC4ECB3E.htm),
[Leader Line](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-B2E6436A-F17D-4F59-9DE8-DBDB61AD36C6.htm)).
The rules below follow the graphics AutoCAD stores with each MULTILEADER (proxy graphics, read with ezdxf), ezdxf 1.4.5's
`render/mleader.py` and [How to render DXF leaders](https://atlight.github.io/formats/dxf-leader.html) (cited by ezdxf);
those marked *inferred* are not stated by Autodesk.

- The same group code means different things inside `CONTEXT_DATA{` (300–301), `LEADER{` (302–303), `LEADER_LINE{` (304–305) and outside,
  so the sections are tracked while reading. Values in the entity are drawn as they are; the MLEADERSTYLE is not consulted (*inferred*,
  as ezdxf's `IGNORE_OVERRIDE_FLAGS`), and context values win over the common ones (text position, color, height, arrow size, dogleg length).
- BYBLOCK in the parts (text, leaders, block content) means the MULTILEADER's own color, line type and line weight (*inferred*).
  A leader line color (92) of BYBLOCK means the leader color (91).
- Leaders run from the arrowhead to the landing point (LEADER 10) and continue as the dogleg along LEADER 11 for LEADER 40.
  The dogleg is drawn only for straight leaders with 291 set and a non-zero length and direction; otherwise the leader lines run to the
  dogleg end. Leaders attached at the top or bottom (271 = 1) end at the landing point. Leaders whose 290 is 0 are not drawn, as in AutoCAD
  (according to ezdxf), and are reported. Spline leaders are drawn through the points, leaving in the arrow direction and arriving along
  the dogleg, and reported as approximated.
- The arrowhead is drawn when the first segment is longer than twice the arrow size (as ezdxf notes about AutoCAD). Without an arrow block,
  it is the closed filled arrow: as long as the arrow size and a third of it wide at the base, as in AutoCAD's graphics. Arrow blocks point
  to −X from their insertion point and are rotated to the leader and scaled by the arrow size; the leader starts at the back of the arrow,
  except for `_ArchTick`, `_Oblique`, `_DotSmall`, `_Small`, `_Integral` and `_None` (ezdxf's list). Arrows are drawn with solid lines.
- The text frame (292) is the drawn text extents grown by the landing gap (145) and stretched to the leaders' connection points, which
  AutoCAD's graphics place on the frame. Connections 5, 6 and 8 underline the text from the connection point; top or bottom connection 10
  draws a line over or under the text. Their lengths follow the substitute font, so they differ from AutoCAD by the text width.
- Block content is inserted at context 15 (in WCS) with its scale and rotation (radians), and each `block_attribs` value is drawn at its
  ATTDEF in the block (as the text of its embedded MTEXT for a multi-line ATTDEF). Tolerance content and text columns
  are not drawn as such and are reported; text flowing vertically (172 = 3) is laid out horizontally like vertical
  MTEXT.
- MLINE elements ([MLINE](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-590E8AE3-C6D9-4641-8485-D7B3693E432C.htm),
  [MLINESTYLE](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-3EC12E5B-F5F6-484D-880F-D69EBE186D79.htm))
  are at the vertex plus the miter direction (13) times the first value of group 41. The following values are positions
  along the segment where the element starts, stops and restarts (MLEDIT cuts). The open tee in the ACadSharp sample (the gap is exactly
  the width of the crossing multiline) shows that they are positions, not dash lengths (*inferred*). They are taken from the miter point;
  whether a cut in an element that starts after the miter point is measured from there or from the element's start is not confirmed,
  as every cut element in the samples starts at the miter point.
- Without group 41, the offset is the MLINESTYLE offset, shifted by the justification (top: the largest offset on the reference
  line, bottom: the smallest) and multiplied by the scale, divided by the sine between the miter and the segment.
- The fill is the area between the outermost elements, including round caps; fill breaks (group 42) are not drawn and reported.
  Caps follow MLINESTYLE flags 16/32/64 and 256/512/1024 unless the MLINE suppresses them (71 flags 4 and 8, which ezdxf ignores); inner arcs
  join every inner pair, as AutoCAD's [Multiline Style dialog](https://help.autodesk.com/cloudhelp/2015/ENU/AutoCAD-Core/files/GUID-9AE65983-DFCA-4140-889E-227B8A31D047.htm)
  describes (2 and 5, 3 and 4 of six elements; ezdxf joins only the first pair). Miters (flag 2) are drawn at inner vertices.
- Element colors and line types of BYBLOCK take the MLINE's own, but the MLINE's true color is not used (*inferred*, as ezdxf notes);
  caps use the MLINE's line type. An MLINE whose style is missing is drawn with the two lines of STANDARD and reported.

### Limits

Damaged or hostile drawings must not hang the reader or exhaust memory, and the TypeScript and Rust readers stop at the
same place. What is left out is reported.

| Limit | Value | Notice |
| --- | --- | --- |
| Nesting of block references, dimension blocks and multileader blocks | 32 levels | `block-depth` |
| Entities drawn inside blocks and dimensions, per view (a block that contains itself twice grows to 2³² entities first) | 1,000,000 | `block-limit` |
| Copies of an MINSERT array | 10,000 (only the first copy is drawn beyond) | `minsert-large` |
| Pattern lines of one hatch, also when the count is not a number (NaN in binary DXF) or the line numbers pass 2⁵³ (a base point so far from the boundary that adding 1 no longer changes the number) | 5,000 (the outline is drawn instead) | `hatch-dense` |
| Polygon mesh grid (71 × 72) | Only the positions that have vertices are visited | |
| Segments of an arc or ellipse, steps of a spline | 4,096 | |

### Text and fonts

SHX fonts are not bundled. Text in SHX styles (or without a font) is drawn with a substitute font chosen by `fontFamilyFor`
(Japanese Gothic, or Mincho for serif names), and the number of such entities is reported with the font names.
TrueType styles use their font name (`ＭＳ ゴシック`, `MS Mincho`…). Character widths are estimated (full width 1 em, half width 0.5 em),
so text lengths differ from AutoCAD. The text height (40) is taken as the capital height for Latin text and the kanji height
for text with full-width characters.

TrueType fonts whose name starts with `@` (`@ＭＳ 明朝`, `@MS Gothic`) are the vertical forms of Windows fonts: their
glyphs are turned 90° counter-clockwise against the baseline, so text rotated by 270° reads top to bottom with upright
characters. This is how AutoCAD and Jw_cad write vertical Japanese text to DXF (AutoCAD's vertical style flag works for
SHX fonts only). TEXT and MTEXT in such a font are laid out as a vertical column: along the baseline, each character
takes its width (full width 1 em, half width 0.5 em, times the width factor), and across it the column is 1 em wide,
from 0.12 em below the baseline, as for horizontal text (*inferred*: the position of the column against the baseline is
not documented; it is the convention that the SXF and JWW conversion writes, and close to how Windows places the
glyphs). The fixture `tests/fixtures/dxf/vertical-text.dxf` pins it.

## Writing (`write_dxf`)

`zumen-dxf` writes a `DxfDocument` back as DXF, in Rust only: ASCII DXF of AutoCAD 2013 (AC1027) in UTF-8 with CRLF
line ends, with the group codes of the DXF Reference and, where it leaves the order open, the order of AutoCAD and
ezdxf 1.4.5. `zumen::convert` reads DXF and DWG drawings into the model and writes them
([decision 0006](../decisions/0006-write-through-the-autocad-model.md)). A written drawing, read again, draws the same
scene in every view, with the same entity ids.

SXF and JWW drawings are mapped into the model by `zumen-convert` first ([decision 0007](../decisions/0007-mapping-crate-between-formats.md)):
model space at real size, the sheet as a layout with a viewport for each scale, and what is drawn on the sheet itself in
paper space ([decision 0008](../decisions/0008-sheets-and-scales-in-the-autocad-model.md)). The rules are in
[How SXF is handled](sxf.md#dxf-への変換zumen-convert) and [How JWW is handled](jww.md#dxf-への変換zumen-convert). The
mapping writes what zumen's reader draws the same way as the source scene, and what AutoCAD users expect:

- `$TILEMODE` 0, so the drawing opens in the layout of the sheet; `$INSUNITS` 4 (millimeters), `$MEASUREMENT` 1 and
  `$PSLTSCALE` 1, so line type dashes keep their paper length in every viewport.
- The overall paper space viewport (ID 1) of the layout, then a viewport for each scale, locked (90 has 16384), on the
  layer `VIEWPORTS` that is off and not plotted (ezdxf's name). The viewport rectangle is the extents of what the source
  draws there with a margin of 1 mm (or 0.5% of the size), so nothing is clipped.
- TEXT is left-aligned on the baseline, with the height, width factor and oblique angle that draw the source's text box:
  the box height is the em of the glyphs (the text height is 0.88 or 0.72 of it), and the insertion point is 0.12 em
  above the bottom of the box. Vertical text uses the `@` form of the font and a rotation of 270°.
- Dimensions are DIMENSION entities whose anonymous block (`*D1`, `*D2` …) holds the drawn lines, arrows and text;
  the definition points come from the source's dimension line and extension lines.

- **Sections**: HEADER (`$ACADVER`, `$HANDSEED`, units, scales, extents, limits and the current layer and styles),
  CLASSES, TABLES (VPORT, LTYPE, LAYER, STYLE, VIEW, UCS, APPID, DIMSTYLE, BLOCK_RECORD), BLOCKS, ENTITIES and OBJECTS
  (the root dictionary with ACAD_GROUP, ACAD_LAYOUT, ACAD_MLEADERSTYLE, ACAD_MLINESTYLE, ACAD_PLOTSETTINGS and
  ACAD_PLOTSTYLENAME, the layouts and the multiline styles).
- **Numbers** are written in the shortest form that reads back to the same value (`1.0`, `0.30000000000000004`, `1e20`),
  so the model reads back value for value.
- **Handles** of the model are kept. A handle that is missing, not hexadecimal, zero or used twice gets a new one above
  the largest handle of the drawing. References (owners, styles, blocks, arrow blocks, attribute definitions, frozen
  layers, clipping boundaries) are rewritten to the handles written, and a reference to something that is not written
  is left out.
- **Definitions that entities refer to but the drawing lacks** are added: layers in the order in which the scene
  numbers them (so layer ids stay the same), with the line type `CONTINUOUS` that the scene assumes, then layer `0`;
  text styles, dimension styles and line types; and empty blocks for block references without a definition.
  `*Model_Space`, `*Paper_Space` and one layout for each paper space block are always written, and layouts keep their
  plot settings, including a paper size of 0 (the sheet then comes from the paper space limits).
- **Strings** are written as the model holds them, in the notation of ASCII DXF. A line feed or carriage return, which a
  line cannot hold, is written as `^J` or `^M`. MTEXT is split into parts of 250 characters (group 3) and the last part
  (group 1).
- **Multileaders** are written in ezdxf's order with every override flag (90) set, so the entity's own values are used,
  and point at one `Standard` MLEADERSTYLE with ezdxf's defaults; their text style, line type, arrow blocks and attribute
  definitions point at the records written. **Wipeouts** are written without an image definition, as AutoCAD writes
  them. **Gradients** keep their name, colors, angle, shift and tint (450 to 470, in AutoCAD's order).
- **Multi-line attributes** (ATTRIB and ATTDEF with `mtext`) are written as AutoCAD 2018 writes them: the attribute
  type (71: 2 for an attribute, 4 for a definition) at the end of `AcDbAttribute` or `AcDbAttributeDefinition`, then
  `101`/`Embedded Object` and the values of the MTEXT, with its own extrusion. The attribute's own text (1 in
  `AcDbText`) is written as the model holds it.
- Only the values that the drawing has are written where the reader tells a missing value apart (the alignment point
  of TEXT, the background scale of MTEXT, the definition points and angle of dimensions). Values that the model does
  not keep are written as AutoCAD writes them for a new drawing (dictionaries, plot settings, viewport UCS,
  `$DWGCODEPAGE` `ANSI_1252`, which AutoCAD ignores in UTF-8 files). Hatches are written without associativity, as the
  links to their boundary objects are not kept.

What cannot be written as it was is reported in `notices`:

| Code | What is written |
| --- | --- |
| `image` | IMAGE is left out: the model does not keep the image file (IMAGEDEF) |
| `helix` | HELIX is written as SPLINE, its curve, without the helix parameters |
| `mpolygon` | MPOLYGON is written as HATCH |
| `table` | ACAD_TABLE is written as the INSERT of its block (`*T…`) |
| `complex-linetype` | A line type with texts or shapes keeps its dashes only, as it is drawn |
| `dimension-block` | A dimension without its block (AutoCAD's dimensional constraints) is left out, as it is not drawn |
| `attrib-alone` | An ATTRIB outside a block reference is written as TEXT, or as MTEXT when it has multiple lines |
| `missing-block` | A block reference without a definition gets an empty block |
| `mleader-block` | A multileader whose content block is missing is written without its content |

### Versions (`write_dxf_as`)

`write_dxf_as(doc, version)` writes AutoCAD R12 (AC1009), 2000 (AC1015), 2004 (AC1018), 2007 (AC1021), 2010 (AC1024),
2013 (AC1027, the default of `write_dxf`) or 2018 (AC1032). `zumen::convert_with` takes the version in `WriteOptions`,
`zumen.convert(..., version="2000")` in Python and `zumen convert --target-version 2000` on the command line.
`DxfVersion::parse` reads `2000`, `R2000` and `AC1015` alike. `$ACADVER` and `$ACADMAINTVER` are the values that ezdxf
1.4.5 writes for each version.

- **Text encoding**: 2007 and later are UTF-8. Earlier versions are written in a code page (DXF Reference "Storage of
  String Values"): the drawing's `$DWGCODEPAGE` when it is not western, else the code page of the encoding it was read
  in when that is not western, else the ANSI code page that holds the most characters of its texts. On a tie the
  single-byte code pages come first (ANSI_1252, 1250, 1251, 1253, 1254, 1257, 1258, 1255, 1256 and 874), then
  ANSI_932, 936, 949 and 950: kana and kanji give ANSI_932, simplified Chinese ANSI_936, Hangul ANSI_949, and
  Cyrillic ANSI_1251 although Shift_JIS has it too. A character the code page cannot hold is written as `\U+XXXX`,
  which AutoCAD and zumen read back as the character; one outside the Basic Multilingual Plane is written as the two
  escapes of its UTF-16 surrogates, which zumen reads back as one character.
- **What a version cannot hold** is left out or written in another form, following the versions of ezdxf's attributes,
  and reported:

| Code | Versions | What is written |
| --- | --- | --- |
| `version-color` | 2000 | True colors (420) as the nearest ACI (62), for entities and layers |
| `version-transparency` | 2000 | Transparency (440, and the `AcCmTransparency` data of layers) is left out |
| `version-gradient` | 2000 | A gradient hatch is filled with its first color |
| `version-dimension` | 2000 | ARC_DIMENSION and LARGE_RADIAL_DIMENSION are written as the INSERT of their block |
| `version-background` | 2000, 2004 | The background fill of MTEXT (90, 63, 421, 45, 441) is left out, also in blocks: the text fill of a dimension (DIMTFILL, 2007 and later) is the MTEXT in its block |
| `version-mleader` | 2000, 2004 | MULTILEADER is written as it is, as ezdxf writes it; AutoCAD before 2008 does not know it |

#### R12 (`r12::prepare`)

R12 has no subclass markers, owner handles, classes, objects, block records or layouts, so these are not written;
entities keep their handles (`$HANDLING` 1) and table entries have none. The drawing is first reduced to what R12 holds,
as ezdxf's `r12export` does, and the rest is reported:

- LWPOLYLINE becomes a 2D POLYLINE with the same vertices, bulges and widths.
- ELLIPSE, SPLINE (and HELIX), MTEXT, HATCH (and MPOLYGON), LEADER, MULTILEADER and MLINE are exploded into what zumen
  draws for them (`scene::Exploder`, the same curves as the model space view): lines as LINE and POLYLINE, fills as
  SOLIDs, one for each trapezoid of a horizontal band (even-odd rule), hatch patterns as the LINEs of their dashes cut at
  the boundary, text as one TEXT for each run, with a text style added for a font that a format code sets, and points
  as POINT. One resulting entity takes the place and handle of the original. Several go into an anonymous block (`*U`)
  whose INSERT, at the original's anchor, takes its handle, layer, color and line type; the parts are on layer `0`,
  and those with the original's color and line type use BYBLOCK. A fill with the background color (a background mask)
  cannot be drawn in R12 and is left out.
- A multiline attribute becomes a one-line attribute with its lines joined by spaces.
- Only the current layout (`*Paper_Space`) is kept, as R12 has one paper space; its sheet becomes the paper space
  limits (`$PLIMMIN`, `$PLIMMAX`), which is what a reader of R12 takes as the sheet. Viewports keep their view in the
  extended data `ACAD`/`MVIEW`, with the layers frozen in them by name (1003), and a viewport that is off has status 0.
- Entities are written in their draw order, as R12 has no SORTENTSTABLE.

| Code | What is written |
| --- | --- |
| `version-ellipse`, `version-spline` | ELLIPSE, SPLINE and HELIX as POLYLINE |
| `version-mtext` | MTEXT as TEXT, several in an anonymous block |
| `version-hatch`, `version-leader`, `version-mline` | HATCH, LEADER, MULTILEADER and MLINE as an anonymous block of LINE, POLYLINE, SOLID and TEXT |
| `version-multiline-attribute` | Multiline attributes as one line |
| `version-mask` | Fills with the background color (background masks) are left out |
| `version-xline`, `version-wipeout`, `version-invisible` | XLINE, RAY, WIPEOUT and invisible entities are left out |
| `version-layouts` | Entities of the layouts other than the current one are left out |
| `version-units` | The length unit (`$INSUNITS`) is left out; zumen reads an R12 drawing as millimeters |
| `version-lineweight`, `version-linetype-scale` | Line weights and the line type scale of entities are left out |
| `version-font-family` | The TrueType font name of a text style (extended data, which R12 tables cannot have) is left out, so a vertical `@` font is written as its file |
| `version-color`, `version-transparency`, `version-dimension` | As in 2000 |

## Editing (`edit_dxf`)

`zumen_dxf::edit_dxf` (through `zumen::edit`) changes a DXF drawing in place with the operations of
[decision 0012](../decisions/0012-edit-in-place-and-write-every-format.md). It does not go through the model: it splits
the file into tags as the reader does, keeps the bytes of every tag, and writes back every tag that no operation
changes as it was. Extended data, reactors, extension dictionaries, objects and the entities that zumen does not read
stay, and an edit without operations gives the same bytes.

- **Version and form**: the version, ASCII or binary, the line ends of the file (CRLF, LF or CR) and the text encoding
  stay. AutoCAD 2007 (AC1021) and later write UTF-8; earlier versions write the encoding that the reader chose (the
  `$DWGCODEPAGE`, or what it detected), and a character that encoding cannot hold is written as `\U+XXXX`, which
  AutoCAD and zumen read back as the character (DXF Reference "Storage of String Values"). Control characters and the
  caret are written in caret notation (`^J`, `^ `) in ASCII DXF.
- **Handles**: new records take handles from `$HANDSEED` (or above the largest handle, if the seed is lower), and
  `$HANDSEED` is set to the next one. R12 drawings without `$HANDLING` 1 get no handles: their entities are chosen
  by their place in the file (`#12`, the id of the scene), which an edit that adds or removes records before them
  changes, and `created` gives the places of the new entities in the edited file.
- **Where `add` puts an entity**: model space at the end of ENTITIES; `layout:<name>` in the paper space of that
  layout (the active one in ENTITIES with 67 = 1, the others at the end of their `*Paper_Space…` block);
  `block:<name>` at the end of the block definition, before ENDBLK. From R13 on, the owner (330) is the block record.
- **What `add` writes** (R13 and later, with the subclass markers of AutoCAD):

  | `kind` | Entity | Keys |
  | --- | --- | --- |
  | `line` | LINE | `start`, `end` |
  | `circle`, `arc` | CIRCLE, ARC | `center`, `radius`, `angle` (`[from, to]` in degrees, for arcs) |
  | `ellipse`, `ellipse-arc` | ELLIPSE (not in R12) | `center`, `majorAxis` (from the center), `ratio`, `parameter` (`[start, end]` in radians) |
  | `point` | POINT | `position` |
  | `polyline` | LWPOLYLINE (POLYLINE, VERTEX and SEQEND in R12) | `vertices`, `bulges`, `closed`, `width` |
  | `text` | TEXT, or MTEXT when the text has line breaks or `wrapWidth` is given (not in R12) | `position`, `text`, `height`, `rotation`, `textStyle`, `wrapWidth` |
  | `block-reference` | INSERT | `block`, `position`, `scale` (a number or `[x, y]`), `rotation` |

  Every kind also takes `layer` (8), `color` (62 and, from AutoCAD 2004 on, 420), `lineType` (6) and `lineWidth`
  (370, from AutoCAD 2000 on). A layer that the drawing lacks is added with color 7 and the line type `CONTINUOUS`
  (`edit-added-layer`); a drawing without a LAYER table gets one when a layer is added. Line types, text styles and
  blocks must exist (`missing-definition`).
- **What `update` changes**: only the group codes of the keys set; the order and the other tags stay. Moving a TEXT,
  ATTRIB or ATTDEF that is aligned (72 or 73, 74 for attributes, not 0) moves the alignment point (11) by the same
  amount, as it is the point that places the text. Setting `rotation` of MTEXT removes its direction vector (11),
  which would override it. Changing the layer of a POLYLINE also changes its VERTEX and SEQEND. LWPOLYLINE vertices are
  written again with their bulges; changing only bulges preserves per-vertex widths (40/41) and identifiers (91).
  Other entities, such as hatches and dimensions, take the common keys only.
- **What `delete` removes**: the entity with what follows it (VERTEX and SEQEND, ATTRIB and SEQEND), the objects it
  owns (330), such as its extension dictionary, and its handle in SORTENTSTABLE (with its sort handle), GROUP (340) and
  reactors (330 in `{ACAD_REACTORS`). An attribute alone cannot be deleted.
- **Layers**: `add-layer` copies layer `0` without its handle, extension dictionary and extended data, so the plot
  style and material references stay valid; `update-layer` sets the color (a negative 62 turns a layer off), frozen
  (70 bit 1), locked (70 bit 4), plot (290), line type (6) and line weight (370). The count of the table (70) follows.
- **Block definitions**: `add-block` ([decision 0017](../decisions/0017-add-annotations-and-definitions.md)) takes
  `name` and `basePoint` and writes an empty definition before the ENDSEC of BLOCKS: from R13 on a BLOCK_RECORD (with
  layout 340 = 0, and 70, 280 and 281 from AutoCAD 2004 on) at the end of the BLOCK_RECORD table, whose count (70)
  follows, and BLOCK and ENDBLK owned by it; in R12, BLOCK and ENDBLK only. A drawing without a BLOCKS section gets
  one before ENTITIES, which moves the ids of handle-less entities (`#12`) after it. Later operations of the same
  edit add entities with `"container": "block:<name>"` and place the block with `block-reference`. Names that exist
  are `already-exists`; names starting with `*` (anonymous blocks) are `invalid-value`. `created` gives the name, and
  the entities added to the new block their handles (or, without handles, their places in the file).
- **Values the version cannot hold** are written as the nearest one and reported: `edit-nearest-color` (no true color
  before AutoCAD 2004), `edit-rounded-lineweight` (the line weights of the DXF Reference), `edit-no-lineweight` (no
  line weights before AutoCAD 2000).

## Verification

See [verification.md](../verification.md#dxf), [DXF writing](../verification.md#dxf-writing) and
[DXF versions](../verification.md#dxf-versions). The parser was compared with ezdxf 1.4.5 on public Japanese drawings
(Yokosuka City and Imizu City), the ACadSharp samples (R12 to 2018, ASCII and binary) and generated drawings.

R12 export limits dashed hatch expansion to 100,000 dash steps per source stroke. Denser patterns or floating-point steps that cannot advance are written as continuous clipped lines and reported as `version-hatch-density`. Anonymous block names wrap to unused positive numbers when the source contains the maximum integer suffix.
