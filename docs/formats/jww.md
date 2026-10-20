# How JWW is handled

`@zumenjs/jww` reads JWW, the drawing format of Jw_cad, in TypeScript without WebAssembly. `zumen-jww`
(`crates/zumen-jww`) is its Rust port and returns the same scenes; it also writes JWW (in Rust only).

A JWW file is written by MFC's `CArchive`. The layout of the header and of each entity class follows Jw_cad's format
description, [jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt) (Jw_cad 7.02, data version 700, checked 2026-10-20).
The reader was first written after [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT), which reads that description
([THIRD_PARTY_NOTICES.md](../../packages/jww/THIRD_PARTY_NOTICES.md)). How `CArchive` stores strings, counts and objects
follows MFC (Technical Note 2, *Persistent Object Data Format*).

## Reading

- Coordinates are paper millimeters with the origin at the center of the sheet. The scene moves the origin to the lower
  left corner, and real coordinates and distances come from the scale of the layer group (1/100 and so on).
- The header gives the names and visibility of layer groups and layers, the screen colors of the pen colors (1–9, and
  the SXF colors 100–356), the line types (bit patterns with the printer pitch, and the lengths of the SXF line types)
  and the line widths (in 1/100 mm, or printer dots converted with the dpi). Older versions whose header cannot be read
  are drawn with Jw_cad's default colors and line types, with a notice.
- The entity list is found from the registration of the first entity class (`CData…`). Class and object numbers are
  shared by the entity list, the block definitions and the entities in them.
- A negative arc angle runs clockwise; flatness and tilt make ellipses and elliptical arcs. Blocks are placed, nested,
  by the number of their definition.
- Solid triangles and quadrilaterals use the stored `CDataSolid::Serialize` order (start, end, DPoint2, DPoint3)
  as their boundary order. Reordering by the names of vertices in the format description (1, 4, 2, 3) makes
  rectangles cross themselves. This interpretation was checked in Jw_cad 10.3.6 on Windows with its sample plan;
  generated rectangles, oblique quadrilaterals and triangles cover versions 600 and 700. Only conversion to DXF
  SOLID changes to the DXF vertex order (1, 2, 4, 3).
- Temporary points (not printed) and images embedded in text (`^@BM` …) are not drawn and are counted in notices.
  Random (hand-drawn style) lines are drawn solid.
- Settings that Jw_cad 8 and later store as text at (0, −1000), such as `Printer_Orientation = 0`, are not drawn as
  entities but shown in the drawing's metadata (drawing them would stretch the extents far beyond the sheet).

<a id="dxf-への変換zumen-convert"></a>

## Converting to DXF (zumen-convert)

`zumen::convert` maps a JWW drawing into the AutoCAD drawing model (`DxfDocument`) and writes AutoCAD 2013 DXF
([decision 0007](../decisions/0007-mapping-crate-between-formats.md),
[0008](../decisions/0008-sheets-and-scales-in-the-autocad-model.md)). The mapping follows the drawing rules above, and the
layout of the written DXF draws the same shapes at the same places as the JWW.

| JWW | DXF |
| --- | --- |
| Sheet (A0–A4, 2A–5A) | A paper space layout of the sheet's size. The DXF opens in this layout (`$TILEMODE` 0) |
| Scale of a layer group | Entities go to model space at real size (coordinates × scale, origin at the sheet center), shown at the same place on the sheet by one viewport per scale. A drawing with one scale stays in model space, even at 1/1 |
| A 1/1 layer group of a drawing with two or more scales | Paper space (millimeters from the lower left corner of the sheet), as the frame and title block drawn on the sheet |
| Layer | A layer named "group-layer name" (`0-0 通り芯` …). Hidden groups and layers are off; display-only ones are locked. DXF has no place for layer group names |
| Pen color | The screen color of the header: the ACI number when it equals ACI 1–9, the foreground color (ACI 7) for black and white, and a true color otherwise. Custom solid colors (pen color 10) are true colors too |
| Line type | A line type `Jw_cad 線種 n` with the dash and gap lengths (mm) of the header's bit pattern; SXF line types (31–45) use the SXF names. Random lines are solid |
| Line width | The entity's width (1/100 mm or printer dots) or the pen color's width, as the nearest DXF lineweight |
| Line, circle, arc, ellipse | LINE, CIRCLE, ARC (the tilt is added to the start angle; clockwise arcs swap the start and end angles) and ELLIPSE (flatness other than 1) |
| Point | POINT. Temporary points are not written |
| Text | TEXT: the start point is the lower left of the text box, the length to the end point is its width and the height is the em (as for SXF). Vertical text (font names starting with `@`) turns that font's text by 270° |
| Solid, circle solid | SOLID, and a solid-fill HATCH (circles, sectors and segments with arc bulges, ellipses with the same points as the scene) |
| Block | Block definitions and INSERTs; the insertion scale is multiplied by the scale of the layer group |
| Dimension | A rotated DIMENSION whose display block (`*D`) holds the dimension line, extension lines, end points and text. The definition points are the ends of the dimension line and the ends of the extension lines away from it |

What cannot be mapped, or is mapped differently, is returned in notices (kind and count).

| code | Meaning |
| --- | --- |
| `view-moved` | Layer groups of different scales would overlap in model space, so the smaller one was moved (the real coordinates of its entities change) |
| `temporary-point` | Temporary points (not printed) are not written |
| `jw-settings` | Jw_cad's settings stored as text at (0, −1000) are not entities and are not written |
| `embedded-image` | Images embedded in text (`^@BM` …) are not written |
| `random-line` | Random (hand-drawn style) lines became solid |
| `text-spacing` | Character spacing cannot be written, so the text is widened to the same box |
| `layer-group` | Layer group names cannot be written to DXF; only the number in the layer name shows the group |
| `memo` | The drawing memo is not written |
| `lineweight` | Widths that DXF does not have became the nearest lineweight |
| `block-scale` | Line type pitches inside blocks placed at different scales follow the first placement |
| `missing-block`, `block-depth`, `unknown-color`, `truncated`, `default-palette`, `degenerate` | Left out or replaced as in the scene |

## Writing

`zumen-jww` writes JWW in two ways: it writes a file it has read back with only the changed records rewritten
(`JwwFile`), and it writes a new drawing from the drawing content (`write_jww`). The TypeScript package does not write.

```text
bytes ─▶ JwwFile::read ─▶ JwwFile (header, records with their bytes, trailer) ─▶ JwwFile::write ─▶ bytes
JwwDocument (read_jww, or built) ─▶ write_jww ─▶ bytes (a new drawing, version 700 by default)
```

### The file as stored (`JwwFile`)

`JwwFile::read` keeps everything, including the values `read_jww` does not use, so that `JwwFile::write` gives back the
same bytes. The fixtures and every local sample drawing (versions 600 and 700) are rewritten byte for byte, both from the
kept bytes and from the typed values alone.

- **Header** (`JwwHeader`): every value of jwdatafmt.txt in its order and type — the memo, sheet size, write layer
  group, the state, write layer, scale and protection of the 16 layer groups and 256 layers, the dimension, printer and
  grid settings, layer and group names, shadow and sky settings, the view and mark jumps, parallel line distances, the
  screen and printer pens, line types 2–9, random lines and the long line types, print and 2.5D settings, the last
  entered sizes, the solid color, the SXF colors and line types (version 4.20 and later) and the text styles. Versions
  before 3.00, and anything that does not follow this order (the fixtures, which end before the text styles), are kept
  as the bytes up to the entity list (`rest`).
- **Records** (`JwwRecord`, `JwwBlockRecord`): each entity keeps its values (`JwwEntity`, the same as `read_jww`) and the
  bytes of its body. A record whose entity has not changed is written from its bytes; a changed or added one is written
  from its values, in the order of the class's `Serialize` in jwdatafmt.txt. A changed dimension keeps its SXF mode
  from the original bytes. A block definition keeps its attributes, number, "referenced" flag, creation time (`CTime`,
  4 bytes) and name; block references point to definitions by that number, not by position.
- **Class tags** are numbered again on every write, as MFC's `CArchive::WriteObject` does: classes and objects share
  one counter from 1 across the entity list and the block definitions; the first object of a class registers it
  (`FF FF`, the schema, the name), later ones refer to it (`0x8000 | number`, or `7F FF` and a 4-byte tag from number
  0x7FFF); each object takes a number before its body. A tag that registered a class is written the same way again, so
  an unchanged file keeps its bytes, and after records are added or deleted every reference stays valid. The schema is
  the version, as Jw_cad writes it.
- **Strings** are CStrings: a length of 1 byte, or `FF` and 2 bytes, or `FF FF FF` and 4 bytes, then the text. Version
  7.00 and later put `FF FE FF` before the length and write UTF-16LE; older versions write Shift_JIS.
  **Counts** are 2 bytes, or `FF FF` and 4 bytes.
- **After the block definitions** (the count and files of embedded images in version 7.00 and later) the bytes are kept
  as they are.
- Null objects in a list hold nothing and are left out. A body that cannot be read to its end (an unknown class) is an
  error: `JwwFile` never writes part of a file, unlike `read_jww`, which draws what it could read.

### New drawings (`write_jww`)

`write_jww` writes a `JwwDocument` with the settings of a new Jw_cad drawing. `JwwDocument::new()` is an empty drawing:
version 700, A3, all layer groups 1/1, layer 0 of group 0 for writing, Jw_cad's default colors, widths and line types,
and the same names `read_jww` gives unnamed groups and layers. Entity coordinates are paper millimeters from the center
of the sheet; `JwwDocument::paper_factor(group)` turns the real coordinates of a layer group into them (1 / scale).

- **Version**: `doc.version` from 300 to 700, otherwise 700 (`JWW_VERSION`, notice `version`). Version 700 is what
  Jw_cad 7.02 and later write, so Jw_cad 8 and later open it; it stores text in UTF-16LE and ends with an image count
  of 0. The version decides what is written, as in jwdatafmt.txt: line widths of entities from 3.51, SXF colors and line
  types and the extension lines and points of dimensions from 4.20, the dpi of printer widths from 6.00.
- **Settings the document does not hold** take the values of a new Jw_cad drawing: dimension settings, printer origin
  and scale, grid, shadow (latitude 36°), view, parallel line distances, printer pens, random lines, 2.5D views, text
  styles 1–10 (2 mm to 10 mm), and the SXF tables (the 16 SXF colors and 15 line types with their names).
- **From the document**: the memo, the sheet size, the state and scale of layer groups, the state of layers and their
  names. Names that `read_jww` makes up for unnamed ones (`グループ 0`, `0-0`) are written empty. As in Jw_cad, one
  layer group (the first in state 3, else the first editable one) is the write group in state 3, and one layer of each
  group is its write layer; other states 3 become 2.
- **Pens**: the screen colors of pens 0–9, the SXF colors 100–356 (screen and printer), and the printer widths in
  1/100 mm or in printer dots for the dpi of `width_unit` (`max_draw_width` −300, or −101 before 6.00, for 1/100 mm and
  100 for dots; the dpi flag 10 for 300 dpi and 20 for 600 dpi).
- **Line types**: lengths that differ from the defaults are written as a bit pattern for types 2–9 and 16–19 (one bit
  is the printer pitch / 32 mm; the smallest pitch that gives back exactly the same lengths, up to 32 bits) and as
  lengths for the SXF line types 30–62 (up to 10).
- **Entities** keep their values. A circle solid has a line type of 101 or more and a solid less (Jw_cad tells them
  apart by it). A dimension of version 4.20 or later without its two extension lines and four points gets
  zero-length lines and temporary points, which are not drawn.
- **Block definitions** get the attributes Jw_cad writes (line type 1, pen color 2), the time 0, and "referenced" when
  a block reference (also one inside a block) uses their number.

Notices when writing:

| code | Meaning |
| --- | --- |
| `version` | A version that cannot be written was written as 700 |
| `text-encoding` | Characters that Shift_JIS cannot hold (versions before 7.00) became `?` |
| `sxf-attributes` | Versions before 4.20 have no SXF colors or line types, so entities that use them get Jw_cad's defaults |
| `line-type` | Dash and gap lengths that a JWW line type cannot hold kept Jw_cad's default line type |
| `pen-width` | Pen widths in millimeters without a unit to write them in (printer dots of a version before 6.00) kept Jw_cad's default widths |
| `invalid-scale` | A layer group scale that is not a positive number became 1/1 |
| `invalid-color` | Colors that are not `#rrggbb` kept the default color |
| `dimension-parts` | Extension lines and points of dimensions beyond two and four, or in versions before 4.20, are not written |
| `solid-style` | The line type of a solid was changed to match its kind (circle solids from 101) |

Written drawings have not yet been opened in Jw_cad; that check is pending.
