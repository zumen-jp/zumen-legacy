# How JWW is handled

`@zumenjs/jww` reads JWW, the drawing format of Jw_cad, in TypeScript without WebAssembly. `zumen-jww`
(`crates/zumen-jww`) is its Rust port and returns the same scenes; in Rust only, it also writes and edits JWW and reads
and writes [JWC](#jwc-jw_cad-for-dos), the drawing file of Jw_cad for DOS, and [JWS](#jws-registered-figures), the
file of a registered figure.

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
  and the line widths (in 1/100 mm, or printer dots converted with the dpi). Versions before 3.00 are read the same
  way: the fields they lack before the pen colors (the sky diagram, mark jumps 5–8 and the layer group of each jump,
  and the text drawing state) are skipped as the notes of jwdatafmt.txt describe. A header that cannot be read is drawn
  with Jw_cad's default colors and line types, with a notice.
- The entity list is found from the registration of the first entity class (`CData…`). Class and object numbers are
  shared by the entity list, the block definitions and the entities in them.
- A negative arc angle runs clockwise; flatness and tilt make ellipses and elliptical arcs. Blocks are placed, nested,
  by the number of their definition.
- Solid triangles and quadrilaterals use the stored `CDataSolid::Serialize` order (start, end, DPoint2, DPoint3)
  as their boundary order. Reordering by the names of vertices in the format description (1, 4, 2, 3) makes
  rectangles cross themselves. This interpretation was checked in Jw_cad 10.3.6 on Windows with its sample plan;
  generated rectangles, oblique quadrilaterals and triangles cover versions 600 and 700. Only conversion to DXF
  SOLID changes to the DXF vertex order (1, 2, 4, 3).
- Temporary points (not printed) and control texts starting with `^@` other than images are not drawn and are counted
  in notices. Images are drawn ([Images](#images)). Random (hand-drawn style) lines are drawn solid.
- Settings that Jw_cad 8 and later store as text at (0, −1000), such as `Printer_Orientation = 0`, are not drawn as
  entities but shown in the drawing's metadata (drawing them would stretch the extents far beyond the sheet).

### Images

Jw_cad places an image with a text `^@BM<file>,<width>,<height>,<trimming and transparency>` (jwdatafmt.txt, 画像データ
文字列; [ezjww](https://github.com/monozukuri-ai/ezjww/blob/main/docs/JWW_WRITE.md#images)). The width and height are
sheet millimeters and the text's start point is the image's lower left corner.

- The image becomes an underlay of `Scene.underlays`, a rectangle turned by the text's angle, with `file` as the text
  gives it: a path (`C:\images\site.jpg`) or, for images embedded in the drawing (version 7.00 and later),
  `%temp%<name>`, which Jw_cad resolves by extracting the files stored after the block definitions to its temporary
  folder. zumen does not open either.
- The text becomes an entity of the kind `image` kept for selection only (`drawn: false`, its outline), with the file
  and the text as properties; the underlay has its id and layers. Jw_cad draws no frame around images.
- JWW does not record the pixel count, so the content is measured in hundredths of a sheet millimeter
  ([decision 0031](../decisions/0031-image-units-without-a-pixel-count.md)).
- The values after the height (Jw_cad writes `0,0,1,0,255,255,255`) trim the image and make a color transparent;
  their meaning is not documented, and other values are reported (`image-trim`). A placement whose file, width or
  height cannot be read is not drawn (`image`).
- Underlays are drawn under every entity. In which order Jw_cad draws images and lines is not known, so it is not
  reported.

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
| Text | TEXT: the start point is the lower left of the text box, the length to the end point is its width and the height is the size of the text (as for SXF, [decision 0019](../decisions/0019-text-heights-between-formats.md)). Vertical text (font names starting with `@`) turns that font's text by 270° |
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

## Converting to JWW (zumen-convert)

`zumen_convert::dxf_to_jww` maps the AutoCAD drawing model to a JWW drawing, which `write_jww` writes (version 700);
`zumen::convert(bytes, "jww", …)` takes a drawing of any format this way. It is the reverse of the mapping above and walks the drawing like the DXF scene, so the JWW draws the same shapes at
the same places as the layout (or model space) the DXF opens in.

| DXF | JWW |
| --- | --- |
| Layout opened by the scene | The sheet of the same size (A0–A4, 2A–5A, landscape), or the smallest one that contains it (`paper-size`). The layout's sheet is centered on the JWW sheet |
| Paper space | A 1/1 layer group |
| Viewport | A layer group of the viewport's scale. Its model space entities sit where the viewport shows them (also turned by the twist); their real coordinates equal the model coordinates when the model origin shows at the sheet center without twist (`viewport-moved` otherwise). JWW cannot clip, so entities crossing the frame are written whole (`viewport-clip`) |
| Model space, for drawings that open it | A3 and the layer group scale (1, 2, 5 × 10ⁿ) that fits the extents into 90% of the sheet. Extents that do not fit around the origin are moved to the sheet center by a multiple of 10 mm on the sheet (`model-moved`) |
| Layer | The 16 layers of the layer groups of each scale, in order of use. Layers named `0-A name` (written by the mapping above) go back to that group and layer when the scale matches. Off, frozen and not plotted layers and DEFPOINTS are hidden, locked layers display-only. Layers beyond 256 share the last layer of their scale (`layer-merged`) |
| Color | The foreground color (ACI 7) is pen 2 (black); colors equal to Jw_cad's screen colors of pens 1–9 use those pens, SXF predefined colors the SXF colors 101–116, and other colors the user SXF colors 117–355 (the nearest color when they run out, `color-count`). Fills use pen 10 (any color) for colors other than pens 1–9 |
| Line type | Jw_cad's line types 2–9 or the SXF line types 32–45 when the dash and gap lengths (mm on the sheet) are equal, otherwise the user SXF line types 47–62: dots become 0.25 mm dashes (`linetype-dot`), at most 10 lengths (`linetype-long`), the nearest type when the 16 run out (`linetype-count`). Patterns shorter than 0.05 mm on the sheet are solid |
| Lineweight | The entity's width in 1/100 mm |
| LINE, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, POLYLINE | Lines and arcs; bulges become arcs, and circles and ellipses stay arcs through any affine transform. Arcs run counterclockwise |
| POINT, SOLID, TRACE | Points and solids |
| INSERT, TABLE | Block definitions and placements. A definition holds the block in mm on the sheet at its layer group's scale, so most placements have scale 1. A block is defined again for each different inherited BYBLOCK value or layer-0 layer, each scale, and each sheet scale of dashed content. Skewed placements are written as the block's entities (`skewed`) |
| Text, MTEXT, attributes, dimensions, leaders, multileaders, multilines, hatches, splines, wide polylines, images, wipeouts | The lines, fills and text the scene draws: lines; fills split into horizontal trapezoid solids (`fill-solids`); hatch pattern lines cut to the hatch area (`hatch-lines`); text whose height is the TEXT height (the box the scene draws shrunk by 0.88 or 0.72 about the start of the baseline, [decision 0019](../decisions/0019-text-heights-between-formats.md)), start point the lower left of that box, end point at its width, and character width the width divided by the characters (full width 1, half width 0.5). SHX fonts become `ＭＳ ゴシック`; mirrored and oblique text is written upright (`text-mirror`, `text-oblique`) |

Other notices: `layouts`, `outside-viewports`, `viewport-view`, `scale-merged`, `complex-linetype`, `transparency`,
`dimension`, `drawn-shapes`, `spline`, `polyline-width`, `fill-complex`, `hatch-dense`, `gradient`, `image`, `wipeout`,
`xline`, `minsert`, `missing-block`, `block-depth`, `dimension-block`, `text-skew`.

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
  entered sizes, the solid color, the SXF colors and line types (version 4.20 and later) and the text styles. The fields
  a version lacks follow the notes of jwdatafmt.txt: before 3.00 there is no sky diagram, no mark jumps 5–8 and no
  layer group of a jump (4 jumps of 24 bytes), and no text drawing state; before 2.30 no solid color, before 2.25 no
  last entered sizes, before 2.23 no drawing time and 2.5D views. Those fields are read and written only from their
  version, so the header model is the same for every version. The settings read are written again at once and must
  give back the same bytes; when they do not (a layout that differs from the notes, a string written differently),
  the settings stay as the bytes up to the entity list (`rest`), as do the parts that do not follow the order at all
  (the fixtures end before the text styles). Every file at hand was of version 6.00 or 7.00, so the older versions
  follow the notes alone.
- **Records** (`JwwRecord`, `JwwBlockRecord`): each entity keeps its values (`JwwEntity`, the same as `read_jww`) and the
  bytes of its body. A record whose entity has not changed is written from its bytes; a changed or added one is written
  from its values, in the order of the class's `Serialize` in jwdatafmt.txt. The SXF mode of a dimension (version 4.20
  and later) is one of its values (`JwwDimension::sxf_mode`). A block definition keeps its attributes, number, "referenced" flag, creation time (`CTime`,
  4 bytes) and name; block references point to definitions by that number, not by position.
- **Class tags** are numbered again on every write, as MFC's `CArchive::WriteObject` does: classes and objects share
  one counter from 1 across the entity list and the block definitions; the first object of a class registers it
  (`FF FF`, the schema, the name), later ones refer to it (`0x8000 | number`, or `7F FF` and a 4-byte tag from number
  0x7FFF); each object takes a number before its body. A tag that registered a class is written the same way again, so
  an unchanged file keeps its bytes, and after records are added or deleted every reference stays valid. The schema is
  the version, as Jw_cad writes it.
- **Strings** are CStrings: a length of 1 byte, or `FF` and 2 bytes, or `FF FF FF` and 4 bytes, then the text in
  Shift_JIS, or `FF FE FF` before the length and the text in UTF-16LE. Jw_cad 10.01 (2025) and later write UTF-16LE;
  earlier versions write Shift_JIS, also in data version 700. A file read keeps the encoding of its memo for the
  strings that are added or changed; a new drawing uses UTF-16LE from version 700. **Counts** are 2 bytes, or `FF FF`
  and 4 bytes.
- **After the block definitions** (the count and files of embedded images in version 7.00 and later) the bytes are kept
  as they are. Written in a version before 7.00, they are left out (`embedded-image`), and written in 7.00 or later
  without them, the count 0 is added.
- Null objects in a list hold nothing and are left out. A body that cannot be read to its end (an unknown class) is an
  error: `JwwFile` never writes part of a file, unlike `read_jww`, which draws what it could read.

### New drawings (`write_jww`)

`write_jww` writes a `JwwDocument` with the settings of a new Jw_cad drawing. `JwwDocument::new()` is an empty drawing:
version 700, A3, all layer groups 1/1, layer 0 of group 0 for writing, Jw_cad's default colors, widths and line types,
and the same names `read_jww` gives unnamed groups and layers. Entity coordinates are paper millimeters from the center
of the sheet; `JwwDocument::paper_factor(group)` turns the real coordinates of a layer group into them (1 / scale).

- **Version**: `doc.version` from 200 to 700, otherwise 700 (`JWW_VERSION`, notice `version`). Version 700 is the
  data version of Jw_cad 7.02 and later, so Jw_cad 8 and later open it; it ends with an image count of 0, and zumen
  writes its text in UTF-16LE as Jw_cad 10.01 and later do. The version decides what is written, as in jwdatafmt.txt:
  the header fields of each version (above), point symbols from 2.52, circle solids from 2.30, block names with the
  kind of composite figure from 4.10, line widths of entities from 3.51, SXF colors and line types and the extension
  lines and points of dimensions from 4.20, the dpi of printer widths from 6.00. What an older version cannot hold is
  written the nearest way it can and reported: a point symbol becomes a plain point, a circle solid becomes triangular
  solids (its outline as drawn, from the center for sectors), a custom solid color (pen color 10, from 2.30) becomes
  the pen color with the nearest screen color, the kind of composite figure leaves a block name.
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
| `text-encoding` | Characters that Shift_JIS cannot hold (versions before 7.00, or a file read that writes Shift_JIS) became `?` |
| `sxf-attributes` | Versions before 4.20 have no SXF colors or line types, so entities that use them get Jw_cad's defaults |
| `line-type` | Dash and gap lengths that a JWW line type cannot hold kept Jw_cad's default line type |
| `pen-width` | Pen widths in millimeters without a unit to write them in (printer dots of a version before 6.00) kept Jw_cad's default widths |
| `invalid-scale` | A layer group scale that is not a positive number became 1/1 |
| `invalid-color` | Colors that are not `#rrggbb` kept the default color |
| `dimension-parts` | Extension lines and points of dimensions beyond two and four, or in versions before 4.20, are not written |
| `solid-style` | The line type of a solid was changed to match its kind (circle solids from 101) |
| `entity-width` | Versions before 3.51 have no line widths of entities; they draw with the width of their pen color |
| `point-symbol` | Versions before 2.52 have no point symbols (arrows, markers); they became plain points |
| `circle-solid` | Versions before 2.30 have no circle solids; they became triangular solids |
| `solid-color` | Versions before 2.30 have no custom solid colors; those solids took the pen color with the nearest screen color |
| `block-name` | Versions before 4.10 have no kind of composite figure after a block name; it was left out |
| `embedded-image` | The files of embedded images are not in the drawing content (and versions before 7.00 have none); only the text that places an image was written |

## Editing (`edit_jww`)

`zumen_jww::edit_jww` changes a JWW drawing in place with the operations of
[decision 0012](../decisions/0012-edit-in-place-and-write-every-format.md). It reads the file as a `JwwFile`, changes,
adds and deletes records of the entity list, and writes every record that no operation changes from its bytes. The
version, the header settings, the block definitions and the embedded images stay, and an edit without operations gives
the same bytes. `new_jww()` is the empty drawing that `zumen::create` adds to (`JwwDocument::new()`: version 700, A3,
all layer groups 1/1, lengths in millimeters).

- **Ids** are the ids of the scene: the place of an entity in the entity list (`#5`), counting every record, also those
  the scene does not draw (temporary points, Jw_cad's settings at (0, −1000)). Every operation chooses entities by their
  ids in the drawing as read. Deleting records before an entity, or adding records, changes its id in the edited file;
  `created` gives the ids of the new entities there (they are added at the end of the entity list). Entities inside a
  block (`#5>#1`) are refused (`inside-block`), because changing them would change the definition and every placement
  of it.
- **Coordinates** are real-world millimeters of the entity's layer group, the numbers of the scene's `raw`: paper
  millimeters from the center of the sheet × the scale of the group. Radii and axes are real lengths too. Text sizes
  (`height`, `width`, `spacing`) are paper millimeters, as Jw_cad and the scene's `textSize` give them. Angles are
  degrees, `parameter` radians.
- **Where `add` puts an entity**: at the end of the entity list, on `layer`, or else on the write layer of the write
  layer group (Jw_cad's current layer). `container` `group:<number or name>` (0–F, or the name of the group) chooses the
  layer group; `model` is the same as none. `block:<name>` adds the entity at the end of that block definition (see
  Block definitions below).
- **What `add` writes**:

  | `kind` | Record | Keys |
  | --- | --- | --- |
  | `line` | CDataSen | `start`, `end` |
  | `circle`, `arc` | CDataEnko with flatness 1 | `center`, `radius`, `angle` (`[from, to]` in degrees counterclockwise, for arcs) |
  | `ellipse`, `ellipse-arc` | CDataEnko | `center`, `majorAxis` (from the center), `ratio` (the flatness, up to 1), `parameter` (`[start, end]` in radians, for arcs) |
  | `point` | CDataTen | `position` |
  | `text` | CDataMoji | `position` (the start of the baseline), `text` (one line), `height`, `width` (the height if left out), `spacing`, `rotation`, `font` (ＭＳ ゴシック if left out) |
  | `block-reference` | CDataBlock | `block` (the name of a definition, with or without `@@SfigorgFlag@@n`), `position`, `scale` (a number or `[x, y]`), `rotation` |
  | `dimension` | CDataSunpou | `dimensionType` (`linear`, `aligned`, `radius`, `diameter`), `start`, `end`, `position`, `rotation`, `center`, `point`, `text` (`<>` is the value), `dimensionStyle` (reported, not used) |
  | `hatch` | CDataSolid, or CDataSen and CDataTen | `boundaries`, `pattern`, `patternScale`, `patternAngle`, `patternLines` |

  Every kind takes `layer`. Lines, circles, arcs, ellipses, dimensions and hatches also take `color`, `lineType` and
  `lineWidth`, points `color` and `lineWidth`, and text `color`: in JWW the line type of a text is its anchor and its
  line width holds the flags of dimension values, and the attributes of a block placement are not used. The end point
  of a new text is its start plus the width of its characters (a full-width character is `width` wide, a half-width
  one half of it, with `spacing` between them), as Jw_cad computes it. JWW has no polylines (`unsupported-kind`; add
  lines instead).
- **Block definitions** ([decision 0025](../decisions/0025-annotations-in-jww-and-sxf-edits.md)): `add-block` takes
  `name` and `basePoint` and adds an empty definition (CDataList) numbered one above the largest number (0 in a drawing
  without blocks, as Jw_cad numbers them), with the attributes Jw_cad writes and the time 0. From version 4.10 the name
  is written with `@@SfigorgFlag@@4` (a drawing part), as Jw_cad saves the blocks it makes; names with
  `@@SfigorgFlag@@`, control characters or nothing are `invalid-value`, and a name the drawing has (with or without the
  suffix) is `already-exists`. JWW definitions have no base point (the origin of their coordinates is placed at the
  block reference), so `basePoint` is subtracted from the coordinates of the entities added with
  `"container": "block:<name>"` in the same edit; their coordinates are otherwise real millimeters of their layer
  group, as at the top level, and `block-reference` places the block with its base point at `position`. `block:<name>`
  takes any definition of the drawing (entities added to an existing definition change all its placements). A block
  reference that would place a block inside itself, directly or through other blocks, is `invalid-value`. `created`
  gives the name of a new block, and `block:<name>>#<n>` for an entity added into a block (`#<n>`, its place in the
  definition, is the part after `>` in the ids of the scene, `#<reference>>#<n>`); the definition becomes referenced
  when a block reference uses it.
- **Dimensions** are one dimension figure (CDataSunpou) in SXF mode (1): the dimension line, the value text, the two
  extension lines, the points at the ends of the dimension line and the two reference points. Its look follows the
  drawing's dimension settings (`m_lnSunpou1`–`5` of the header, jwdatafmt.txt 寸法関係の設定), or Jw_cad's defaults
  (`2002111, 5, 50150030, 11000000, 4100`) when the header stores none (all 0):
  - colors: the dimension line color, the extension line (引出線) color and the point color; the value has the color of
    its text style. `color` given in the operation applies to every part, `lineType` and `lineWidth` to the dimension
    line and the extension lines (the line type of the value is its base point position and its line width its flags).
  - the value: the text style (文字種) of the settings, with its width, height and spacing from the header's text
    styles, in ＭＳ ゴシック, centered on the dimension line the settings' gap above it (the lower left of the text box
    is the middle less half its width), turned to read from the left or from below unless the settings turn that off.
    Its attribute flag is 0x4010 (0x4110 for radius values, 0x4210 for diameter values), its base point position (the
    line type of a text) 2 (bottom center) and its line width the dimension value flags: 0x0002, the decimals × 0x1000,
    0x0008 full-width, 0x0010 m, 0x0020 unit shown, 0x0040 commas, 0x0080 zeros of radius values, 0x0100 or 0x0200
    rounding down or up, 0x0400 or 0x0800 `R` and `φ` before or after (the meaning of 0x0010 and 0x0020 is inferred
    from their names in jwdatafmt.txt).
  - the text of the value: `<>` in `text` (the default) is the measured length in millimeters, or meters when the
    settings say so, rounded to the settings' decimals (0–3) as the settings say (round, down or up), without trailing
    zeros, with a comma every three digits (a space or a full-width comma when the settings say so; radius values only
    when the settings ask), with full-width digits, a full-width point and the unit (`ｍｍ`, `ｍ`, inferred from a sample's
    `7.00ｍ`) when the settings ask, and `R` or `φ` before or after radius and diameter values, or none.
  - the extension lines run from `start` and `end` to the dimension line and pass it by the settings' length (0 by
    Jw_cad's defaults); an origin on the dimension line gives a line of length 0, which is not drawn. The points at the
    ends of the dimension line are real points (attribute flag 0x0040); the center end of a radius dimension is a
    temporary point. The reference points are temporary points at `start` and `end` (the line's ends for radius and
    diameter dimensions).
  - The dimension line and the extension lines have the attribute flag 0x2000, as Jw_cad's dimension lines. Settings
    that ask for arrows or reverse arrows at the ends are written as points (a dimension figure holds points, and the
    point codes of arrows are not described), and reported (`edit-dimension-end`). JWW has no named dimension styles:
    `dimensionStyle` is reported (`edit-dimension-style`). Before version 4.20 a dimension figure holds only the line
    and the value (`edit-dimension-parts`). Angular dimensions cannot be a dimension figure, which has no arc
    (`unsupported-kind`).
- **Hatches**: JWW has no hatch entity, so a hatch is written as Jw_cad writes fills and hatching. `SOLID` becomes
  solids: the boundaries (the first the outside, the others holes) are split by the even-odd rule into horizontal
  trapezoids (one solid for a convex quadrilateral), in the operation's color (`#rrggbb` is the solid's own color,
  pen color 10). A pattern becomes hatch lines (CDataSen with the attribute flag 0x0020) cut to the boundaries, with
  the pattern's dashes applied from its base point (a dash of length 0 is a point, CDataTen with the same flag), in the
  operation's color, line type and line width. Pattern lengths are scaled by `patternScale` in real millimeters of the
  layer group, as in DXF. `created` gives the first solid or line; the others follow it in the entity list
  (`edit-fill-solids`, `edit-hatch-lines`). More than 20,000 pieces, a pattern line without spacing, a boundary without
  area or a pattern with no line inside the boundary is `invalid-value`.
- **Values**:
  - `layer`: a layer number `0-A` (the group and the layer in hexadecimal, as the scene names layers, followed by
    anything), the name of one layer, or the scene's raw layer number (group × 16 + layer + 1). A name the drawing lacks
    is added as `add-layer` does (`edit-added-layer`).
  - `color`: pen colors 1–9, the SXF colors 100–356 (version 4.20 and later), or `#rrggbb`: a pen color of that screen
    color, else an SXF user color (117–355) that has no name and that no entity uses, set to that color, else the
    nearest pen color (`edit-nearest-color`, for example before version 4.20). A solid takes `#rrggbb` as its custom
    color (pen color 10). JWW has no `bylayer` or `byblock`.
  - `lineType`: 1–9, 11–15 (random lines), 16–19 (long line types), 30–62 (SXF line types, version 4.20 and later), or
    the name of an SXF line type (`dashed` …).
  - `lineWidth`: millimeters, written in the drawing's unit of entity widths: 1/100 mm, or printer dots for the dpi of
    the header (`edit-rounded-lineweight` when it is not exact, `edit-no-lineweight` when the drawing has no such unit
    or is older than version 3.51). `default` is 0, the width of the pen color.
- **What `update` changes**: only the values set, in the record of the entity. A text whose content, size, spacing or
  angle changes gets a new end point; moving it moves both points. Moving an entity to a layer group of another scale
  keeps its real coordinates and sizes (`edit-rescaled`): paper coordinates, radii and block scales are multiplied by
  the ratio of the scales, and text sizes stay. Solids and dimensions take `layer` and `color` (for every part of a
  dimension).
- **What `delete` removes**: the record. Block definitions stay, even when no placement uses them any more.
- **Layers**: JWW has 16 layer groups of 16 layers, so a layer cannot be created. `add-layer` names the first layer
  (0 to F) of a layer group that has no name and no entities — of the write layer group, or of the group in `group` (a
  number or a name) — and gives `0-3 name` in `created`. `visible: false` hides a layer (state 0) and `locked: true`
  makes it display-only (state 1); the write layer of a group cannot be hidden or display-only, so a layer added that
  way takes another one. `scale` sets the scale of the layer group while the group has no entities. `update-layer`
  takes the same values and `name`, which renames the layer. JWW layers have no color, line type, line width, frozen or
  plot setting (`edit-layer-attributes`).
- **Notices of the writer** come with the prefix `edit-`: `edit-text-encoding` for characters that Shift_JIS cannot
  hold, in a file that writes Shift_JIS (versions before 7.00, and version 700 from Jw_cad before 10.01), and
  `edit-dimension-parts` for the extension lines and points of dimensions before version 4.20.
- **Notices of the editor**: `edit-dimension-style`, `edit-dimension-end`, `edit-fill-solids`, `edit-hatch-lines`,
  `edit-unused-attributes` (the line type and width of a solid fill), besides those of the values above.
- **Errors**: `unreadable` (not JWW, or a file that `JwwFile` cannot read to its end), `unsupported-kind`,
  `unknown-key`, `invalid-value`, `not-found`, `inside-block`, `missing-definition`, `already-exists`,
  `name-exhausted` (no block number left).

The shared conformance test (`crates/zumen/tests/edit.rs`) passes for the fixtures and the 9 local sample drawings
when JWW is added to it, with the updated entity's layer compared by the end of the scene's layer value (`グループ 0 /
0-3 ZUMEN-EDIT-CHECK`). It also adds a block, its entities, a reference and a dimension, and hatches, to every
fixture, and reads them back drawn. In `Ａマンション平面例.jww` a linear dimension added under one of Jw_cad's puts its
value at the same place as Jw_cad's. Written and edited drawings have not yet been opened in Jw_cad; that check is
pending.

The AutoCAD-to-JWW mapper reports each omitted invisible entity as `invisible`, each array reduced to its first
placement at the 10,000-placement bound as `block-array`, and each nonrectangular viewport treated as its bounding
rectangle as `viewport-boundary`. Frozen layers named by R12 viewports are resolved alongside frozen layer handles.

## JWC (Jw_cad for DOS)

`zumen-jww` also reads and writes JWC, the drawing file of Jw_cad for DOS, with its own reader `JWC` (format `jwc`,
detected by the signature `jw_cad(c)data`). Jw_cad for Windows still opens and saves JWC.

JWC is read in Rust only, as DWG is ([decision 0003](../decisions/0003-dwg-in-rust.md)): the Rust code is to become
the single implementation of the readers, with the browser packages moving to its WebAssembly build format by format
([The Rust implementation](../architecture.md#the-rust-implementation)), and JWC is a new format with no TypeScript
reader to keep equal. A TypeScript reader would have to be written and kept equal value for value, for the files of a
program that new drawings no longer use. Browsers would read JWC through the WebAssembly build, as they read DWG.

No official description of JWC was found: the layout follows the findings of
[ezjww](https://github.com/monozukuri-ai/ezjww) (MIT), which compared JWC files that Jw_cad 10.02.1 saved with their
known contents. zumen was checked against those 80 files saved by Jw_cad 10 (kept outside the repository): every one is
written back byte for byte, and reading them gives the same entities as Jw_cad's own JWW saves of them, except one text
longer than 256 bytes that Jw_cad lays out again. No JWC saved by Jw_cad for DOS itself was at hand.

- **Fixed header**: a 40-byte signature `jw_cad(c)data.......a.f.m...............` (byte 22 `f`: layer group scales as
  f32 and a header of 2,421 bytes; `.`: as u16 and 2,389 bytes); three CSV blocks of 199 bytes at 200, 400 and 600
  (ASCII ended by NUL and padded with spaces, LF at 199, 399, 599 and 799). The first block holds the counts of lines,
  arcs, texts, points and temporary points, the scale of the write layer group (field 9), the write layer (10, group ×
  16 + layer), the sheet (11, A0–A4) and the sheet in coordinates (27, 28 and 30: 259, 259 × height / width and 518);
  the third starts with the DOS addresses (segment:offset, lower-case hexadecimal) of the start and end of the string
  pool. Then come 100 temporary points (x, y and layer), text styles 1–10 (pen color, width, height and spacing in
  0.1 mm), the scales of the 16 layer groups, the edit and visible flags of the groups and the 256 layers, and the
  write layer of each group.
- **Records** after the header: lines (22 bytes: start and end as f32, line type, pen color, layer, a spare byte and
  flags with curve markers), arcs (32 bytes: center and radius as f32, flatness × 10000, start, end and tilt angles ×
  65536 degrees, then the same attributes; equal start and end angles are a full circle, and the angles are measured in
  the tilted frame), texts (24 bytes: start, end, the address of the string, text style, layer and flags), the string
  pool, points (12 bytes: position, layer, pen color and flags) and the names (8 bytes for each layer, 16 for each
  group).
- **Coordinates** are single precision, with the lower left corner of the sheet as the origin; 1 is the sheet width /
  518 mm.

`JwcFile::read` keeps the fixed header as it was and writes again only the fields it models. The bytes of each text up
to the next text stay too (Jw_cad cuts a text over 256 bytes with a NUL and leaves the rest), and so does each name slot
(the bytes after its NUL are left from memory), and so do the temporary point slots past the count and the upper bits
of each group's write layer. `read_jwc` maps a JWC to the JWW drawing content (`JwwDocument`, version 700), and the
`JWC` reader draws it with `jwc_to_scene`, so that the scene and the conversion to DXF follow the JWW rules:
paper millimeters from the center of the sheet, arcs with their start angle in −180°–180° as Jw_cad saves them, texts
with the size, spacing and pen color of their style and Jw_cad's default font, and temporary points. Entities come in
the order temporary points, lines, arcs, texts and points, which gives their ids. JWC stores no palette, so the scene
uses Jw_cad's default colors (`default-palette`).

`write_jwc` writes a JWW drawing content as a new JWC with the header Jw_cad 10 writes (f32 scales): the sheet (A0–A4,
otherwise A0), the scales, states and names of layer groups and layers, lines, arcs, circles and ellipses (flatness up to
1, counterclockwise), points, temporary points (up to 100) and texts in up to 10 text styles. What JWC cannot hold is
written the nearest way it can and reported:

| code | Meaning |
| --- | --- |
| `jwc-paper` | JWC sheets are A0–A4; another sheet became A0 |
| `jwc-block` | JWC has no blocks; placements were exploded with their scale and rotation (arcs of a placement scaled differently in x and y became lines) |
| `jwc-arc` | Arcs under nonuniform scaling or shear became 64 line segments; equal column lengths alone do not make the composed placement a similarity transform |
| `jwc-dimension` | JWC has no dimensions; they became lines, points and their text |
| `jwc-solid` | JWC has no solids; they became their outlines |
| `jwc-color` | JWC pen colors are 1–9; SXF and other colors became the pen color with the nearest screen color |
| `jwc-line-type` | JWC line types are 1–9; random, long and SXF line types became the nearest Jw_cad line type |
| `jwc-width` | JWC has no line widths of entities |
| `jwc-font` | JWC has no fonts (nor vertical text); texts use Jw_cad's default font |
| `jwc-text-size` | Texts of more sizes than the 10 text styles took the nearest style |
| `jwc-name` | Names longer than the slots (7 bytes for layers, 15 for groups) were shortened |
| `jwc-text-pool` | Texts beyond the 64 KB string pool were left out |
| `jwc-temporary-points` | Temporary points beyond 100 were left out |
| `jwc-limit` | Records beyond a million of a kind, or beyond four million entities visited while exploding blocks, were left out |
| `point-symbol`, `embedded-image`, `memo`, `missing-block`, `text-encoding` | As for JWW |

Written JWC has not yet been opened in Jw_cad either.

## JWS (registered figures)

`zumen-jww` reads and writes JWS, the file of a figure registered in Jw_cad (図形登録), with its own reader `JWS`
(format `jws`, detected by the signature `JwsData.`), in Rust only for the reasons given for JWC. JWS has no official
description either: the header follows a
[description posted to Jw_cad's forum](https://www.jwcad.net/log/consult2/consult2-0610-12/thread17752.htm) (for
version 2.30, with a dump of version 4.20) and the findings of [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT),
which compared figures that Jw_cad 10.02.1 registered with the drawings they came from. zumen was checked against
those 7 figures (versions 600 and 700, kept outside the repository): each is written back byte for byte; its entities
are those of the drawing it came from, apart from the selection flags and two texts that Jw_cad lays out again when it
registers them (the end of a text and the placement text of an image); and `write_jws` records the same extent as
Jw_cad for 5 of them, and for the other 2 an extent at most 0.007 mm larger. No figure of an older version was at hand.

- **Header** (452 bytes): the signature `JwsData.` and 192 bytes of `.`, the version (u32 at byte 200), the base point
  for placing the figure (x, y), the scales of the 16 layer groups, 6 reserved integers and 6 reserved reals (Jw_cad
  10 writes 1 to 6), and the extent Jw_cad recorded (minimum x and y, maximum x and y).
- **Body**: the entity list and the block definitions of JWW, in the layout of that version, with no image section.
  The old-format save of Jw_cad 10 writes header version 600 with class schemas 700; the schema is not checked.
- Coordinates are paper millimeters, as in JWW. A JWS holds no sheet, pen colors, line types or layer names: a figure
  takes those of the drawing it is placed in.

`JwsFile::read` and `write` give back the same bytes: the reserved bytes are kept, and strings added to a file read
use its encoding. `read_jws` returns a `JwsDocument` (the base point, the recorded extent, and the JWW drawing content
with the version, the group scales, the entities and the block definitions) and reads up to where it can
(`truncated`), as `read_jww` does. The scene (`jws_to_scene`, format `jws`) has no sheet, keeps paper millimeters and
shows the base point in the metadata; it uses Jw_cad's default colors (`default-palette`). `write_jws` writes the
entities as the JWW writer does (versions 200 to 700, with the same notices), computes the extent as Jw_cad does
(lines and curves to their ends, texts by their start and end points, blocks by the entities they place; curves to
their exact ends, where Jw_cad uses the points of the drawn curve), and reports `jws-settings` when the content has
group or layer names, hidden or display-only layers, pen colors, widths, line types or a memo that a JWS cannot
hold.

## Other files of Jw_cad

- **JWK** (figures of Jw_cad for DOS): no public description or sample was found. Not read.
- **JWF** (environment settings), `JWC_TEMP.TXT` (the text exchanged with external programs) and `.BAK` (a copy of a
  JWW) are not drawings and are not read.

When appending JWC text would overflow the original 32-bit DOS pool address, the writer relocates the pool and all text references together. An unchanged pool keeps its original address and bytes.

JWS also reports `jws-settings` when custom SXF colors or a printer width unit cannot be stored.
