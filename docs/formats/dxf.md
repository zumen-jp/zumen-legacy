# How DXF is handled

`@zumenjs/dxf` reads DXF written by AutoCAD R12 (AC1009) to AutoCAD 2018 and later (AC1032), in ASCII and binary form,
following the [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm)
(AutoCAD 2026, checked 2026-10-18). It is written in TypeScript without dependencies other than `@zumenjs/core`.

```text
bytes ─▶ readTags (ASCII or binary, decoded) ─▶ parseDxf ─▶ DxfDocument ─▶ dxfToScene(doc, { layout }) ─▶ Scene
```

## The AutoCAD drawing model

`DxfDocument` is the AutoCAD drawing database as plain data. It is shared with the DWG reader:
a DWG file read with [ezdwg](https://github.com/monozukuri-ai/ezdwg) becomes the same `DxfDocument` and is drawn by the same `dxfToScene`.

- Attribute names follow [ezdxf](https://ezdxf.mozman.at/)'s DXF attribute names (`start`, `center`, `start_angle`, `insert`,
  `major_axis`, `ratio`, `char_height`, `attachment_point`, `defpoint2`…). Where ezdwg's high-level API uses a different name than ezdxf,
  the ezdwg name is used (`color_index`, `linetype_scale`, `raw_text`, `view_center`, `view_target`, `status_flags`,
  `frozen_layer_handles`, `anonymous_block_name`), so ezdwg entities fit without renaming.
- Coordinates stay in drawing units (`$INSUNITS`) and angles in degrees, as in DXF. Points are `[x, y, z]` arrays.
- All entities of model space, paper space and block definitions are in one list, told apart by `space`
  (`"model"`, `"paper"`, `"block"`) and `block` (the block or paper space block name), like ezdwg's documents.
  Attributes of an `INSERT` are in its `attribs`.
- `TEXT` keeps control codes such as `%%c`; `MTEXT` has the plain `text` and the formatted `raw_text`.
- Tables: `layers`, `linetypes`, `styles` (text styles), `dimstyles`, `blocks`, `layouts`;
  `header` has the variables used for drawing plus every raw variable in `header.variables`;
  `sort_handles` keeps SORTENTSTABLE (draw order); `skipped` counts entity types that are not read.

What a DWG reader changes when it maps ezdwg's JSON (`readDwgJson`) to `DxfDocument`:

| ezdwg | `DxfDocument` |
| --- | --- |
| `handle` (integer) | `handle` (upper-case hexadecimal string, as in DXF) |
| `POLYLINE_2D`, `POLYLINE_3D` | `POLYLINE` with `flags` (`8` for 3D) |
| `MINSERT` | `INSERT` with `column_count`, `row_count`, `column_spacing`, `row_spacing` |
| `dimtype` (`"LINEAR"`, `"RADIUS"`…) | `dimtype` (DXF group 70: 0 rotated, 1 aligned, 2 angular, 3 diameter, 4 radius, 5 angular 3-point, 6 ordinate) |
| `block_handle` | `block` (the name, from ezdwg's `blocks`) |
| HATCH `paths` (`{ closed, points }`) | the same, as polyline paths (`edges` are only used by DXF edge paths) |

## Reading

- **Detection** looks at the content: the binary sentinel (`AutoCAD Binary DXF\r\n\x1a\0`), or a first tag `0`/`SECTION`
  after optional `999` comments.
- **Binary DXF**: group codes are 2 bytes (1 byte and the `255` escape in R12); values are read by the type of the group code range
  ([Group Code Value Types Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-2553CF98-44F6-4828-82DD-FE3BC7448113.htm)).
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
| TEXT, ATTRIB, ATTDEF | Horizontal and vertical justification (72/73, 74 for attributes), aligned and fit text, backward and upside-down, oblique, `%%c` `%%d` `%%p` `%%nnn`, `%%u` `%%o` `%%k` |
| MTEXT | Attachment point, direction or rotation, line spacing, word wrap at the reference width (at spaces, or between full-width characters), paragraph alignment, background mask, and the codes `\P` `\H` `\W` `\Q` `\C` `\c` `\F` `\f` `\L` `\O` `\K` `\S` `\~` `\U+` `\M+` and `{}` |
| INSERT (MINSERT), ACAD_TABLE | Nested blocks with base point, scale, rotation, OCS and arrays; constant attribute definitions; the table's anonymous block |
| DIMENSION, ARC_DIMENSION, LARGE_RADIAL_DIMENSION | The anonymous block (`*D…`) in the dimension's OCS, moved to group 12, merged into one selectable entity |
| HATCH, MPOLYGON | Polyline and edge boundaries (lines, arcs, ellipses, splines; clockwise edges as ezdxf reads them). Solid fills, pattern lines (with dashes) clipped to the boundary, gradients as one color |
| SOLID, TRACE, 3DFACE | Filled quadrilaterals; visible 3DFACE edges |
| LEADER | The path (straight or spline-fit) and a closed arrowhead sized by `DIMASZ × DIMSCALE` |
| WIPEOUT | Filled with the background color |
| IMAGE | The frame only (images are not loaded) |
| XLINE, RAY | Clipped to the extents of the other entities (or the sheet) |
| VIEWPORT | Its frame on the layout, and the model space inside |

Not read, counted in `skipped` and reported: MULTILEADER, MLINE, TOLERANCE, SHAPE, 3DSOLID, REGION, BODY, MESH, SURFACE,
underlays (PDF, DWF, DGN), OLE frames, lights and proxy entities.

### Text and fonts

SHX fonts are not bundled. Text in SHX styles (or without a font) is drawn with a substitute font chosen by `fontFamilyFor`
(Japanese Gothic, or Mincho for serif names), and the number of such entities is reported with the font names.
TrueType styles use their font name (`ＭＳ ゴシック`, `MS Mincho`…). Character widths are estimated (full width 1 em, half width 0.5 em),
so text lengths differ from AutoCAD. The text height (40) is taken as the capital height for Latin text and the kanji height
for text with full-width characters.

## Verification

See [verification.md](../verification.md#dxf). The parser was compared with ezdxf 1.4.5 on public Japanese drawings
(Yokosuka City and Imizu City), the ACadSharp samples (R12 to 2018, ASCII and binary) and generated drawings.
