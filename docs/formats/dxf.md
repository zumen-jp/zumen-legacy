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
- `TEXT` keeps control codes such as `%%c`; `MTEXT` has the plain `text` and the formatted `raw_text`.
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
- 102 groups (`{ACAD_REACTORS`…), extended data (1001 and later) and embedded objects (101) are skipped when reading entity attributes.
  Common attributes are taken from the `AcDbEntity` subclass only, so table cell colors of `ACAD_TABLE` are not mistaken for its color.
- `POLYLINE`/`VERTEX`/`SEQEND` and `INSERT`/`ATTRIB`/`SEQEND` sequences are assembled into one entity.
- ATTRIB and ATTDEF take the text generation flags (71) from the `AcDbText` subclass only. The
  [ATTRIB table](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-7DD8B495-C3F8-48CD-A766-14F9D7D0DD9B.htm)
  lists them in `AcDbAttribute`, but AutoCAD 2018 writes the attribute type there (1 single line, 2 multi-line
  attribute, 4 multi-line attribute definition), as ezdxf (`attribute_type`) and ACadSharp (`AttributeType`) read it;
  taken as generation flags, it drew multi-line attribute definitions upside down. The text of a multi-line attribute is
  in its embedded MTEXT (101), which is not read yet: AutoCAD 2018 leaves the attribute's own text empty, so it is not
  drawn.
- **Dimension styles**: AutoCAD (R2000 and later in the samples; R12 writes every value) leaves out of DIMSTYLE the
  values equal to its defaults. A missing DIMSCALE is 1, and a missing DIMASZ or DIMTXT is 0.18, which the DWG of the
  same drawing stores, whatever `$MEASUREMENT` says: the imperial ACadSharp sample writes the 2.5 of its ISO-25 style
  and leaves out the 0.18 of the others, and ACadSharp and netDxf read missing values as 0.18 too (*inferred* for metric
  drawings, as no AutoCAD DXF of a metric drawing was at hand).
- R12 files without `LAYOUT` objects get one `Layout1` when they have paper space entities. R12 viewport settings are read
  from the `ACAD` `MVIEW` extended data, as ezdxf does.

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
  a view, viewports switched off (68 ≤ 0 or flag `0x20000`) are skipped, layers frozen in a viewport (331) are not drawn in it,
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
| TEXT, ATTRIB, ATTDEF | Horizontal and vertical justification (72/73, 74 for attributes), aligned and fit text, backward and upside-down, oblique, `%%c` `%%d` `%%p` `%%nnn`, `%%u` `%%o` `%%k`, and `^ ` (a caret) and `^I` (a tab, as four spaces) of ASCII DXF |
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
  ATTDEF in the block. Tolerance content and text columns are not drawn as such and are reported; text flowing vertically (172 = 3)
  is laid out horizontally like vertical MTEXT.
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

## Writing (`write_dxf`)

`zumen-dxf` writes a `DxfDocument` back as DXF, in Rust only: ASCII DXF of AutoCAD 2013 (AC1027) in UTF-8 with CRLF
line ends, with the group codes of the DXF Reference and, where it leaves the order open, the order of AutoCAD and
ezdxf 1.4.5. `zumen::convert` reads DXF and DWG drawings into the model and writes them
([decision 0006](../decisions/0006-write-through-the-autocad-model.md)). A written drawing, read again, draws the same
scene in every view, with the same entity ids.

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
| `attrib-alone` | An ATTRIB outside a block reference is written as TEXT |
| `missing-block` | A block reference without a definition gets an empty block |
| `mleader-block` | A multileader whose content block is missing is written without its content |

## Verification

See [verification.md](../verification.md#dxf) and [DXF writing](../verification.md#dxf-writing). The parser was compared with ezdxf 1.4.5 on public Japanese drawings
(Yokosuka City and Imizu City), the ACadSharp samples (R12 to 2018, ASCII and binary) and generated drawings.
