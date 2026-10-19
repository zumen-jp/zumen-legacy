# How DWG is handled

`zumen-dwg` (`crates/zumen-dwg`) reads DWG written by AutoCAD 2000 (AC1015) to AutoCAD 2018 and later (AC1032) into the
AutoCAD drawing model of `zumen-dxf` (`DxfDocument`), and `dxf_to_scene` draws it as it draws DXF. It is written in Rust
only, with its own decoder that follows the Open Design Alliance's
[Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf)
5.4.1 and cites its sections in comments ([decision 0003](../decisions/0003-dwg-in-rust.md)). There is no TypeScript
DWG reader: the `zumen` command, Python and the WebAssembly build (`bindings/wasm`) read DWG.

```text
bytes ─▶ file (container, pages, decompression) ─▶ objects (bit streams) ─▶ read_dwg ─▶ DxfDocument ─▶ dxf_to_scene ─▶ Scene
```

Where the specification is silent or does not match the files, the behavior was checked against the same drawing
saved as DXF by AutoCAD and against the MIT implementations [ACadSharp](https://github.com/DomCR/ACadSharp) and
[ezdwg](https://github.com/monozukuri-ai/ezdwg). Those rules are marked *inferred* below. LibreDWG (GPL) is not used and
its source was not read.

## Versions and containers

- **Detection**: the file starts with `AC10` and two digits. AC1012 and AC1014 (R13 and R14) and unknown versions are
  refused with a message. R13 and R14 use the R2000 container and can be added later.
- **AC1015 (R2000)**: the section locators of the file header (3.2.1) give the header variables, the classes, the object
  map and the template section (`$MEASUREMENT`).
- **AC1018, AC1024, AC1027, AC1032 (R2004, 2010, 2013, 2018)**: the file header at 0x80 is masked with a pseudo-random
  sequence (4.3); the page map and the section map give the pages of each section (4.4, 4.5); each data page header is
  masked with `0x4164536b` XOR the page position (4.6), and the data is LZ77-compressed (4.7).
- **AC1021 (R2007)**: the file header at 0x80 is Reed-Solomon coded (255, 239) and interleaved by three, pages start at
  0x480, data pages are Reed-Solomon coded (255, 251), and the compression differs (5.2–5.4, 5.10, 5.13). The length of
  a section name is a byte count, not a character count as the specification says (*inferred*, as ACadSharp reads it).
- A section is assembled page by page. A page that is missing or cannot be decompressed is filled with zeros and
  reported (`dwg-broken-page`). The decompressors stop at the page size, so a damaged page cannot grow without bound.

## Objects

- Values are read with the bit codes of section 2 (B, BB, BS, BL, BLL, BD, DD, RC, RS, RL, RD, MC, MS, H, TV, BE, BT,
  OT, CMC, ENC). Objects are found through the object map (23); class numbers from 500 take their DXF names from the
  classes section (10).
- From R2007 on, an object has three streams: data, strings (UTF-16) and handles. The string stream is located by the
  flag before the handle stream and the 15- or 30-bit size before that flag. From R2010 on, the object type is an `OT`
  and the size of the handle stream is counted from the end of the object.
- Only R2000 entities have the "no links" bit and the handles of the previous and next entity, as ACadSharp and ezdwg
  read them (*inferred*: reading the bit in later versions shifts every following value of the samples). Entities are
  followed through their block: by those links in R2000, by the block's list of owned entities later.
- An object that cannot be read is left out and counted by type (`dwg-broken-object`). Entity types that are not read
  are counted by their DXF name in `skipped` and reported (`skipped-types`), as for DXF.

## Text

- From R2007 on, strings are UTF-16 (the scene says `UTF-16（AutoCAD 2007 以降の DWG）`). Earlier strings use the code
  page of the file header, whose number (3.2.5) is turned into the `$DWGCODEPAGE` name (`ANSI_932` for Japanese) and
  decoded as DXF decodes that name. An unknown code page is read as Windows-1252 and reported (`unknown-codepage`).
- Control characters are written in caret notation, as AutoCAD writes them in DXF (`^J` for a line feed, `^ ` for a
  caret), so that TEXT and MTEXT read the same as from the DXF of the drawing (*inferred*).

## What is read

- Header variables used for drawing (`$INSUNITS`, `$LTSCALE`, `$PSLTSCALE`, `$TILEMODE`, `$TEXTSIZE`, `$DIMSCALE`,
  `$DIMASZ`, `$DIMTXT`, extents and limits) and `$MEASUREMENT` from the template section; the tables of layers, line
  types, text styles, dimension styles and block records with their BLOCK entities; layouts (the `ACAD_LAYOUT`
  dictionary); multiline styles (`ACAD_MLINESTYLE`); draw order (SORTENTSTABLE); color book colors (DBCOLOR in
  `ACAD_COLOR`).
- Entities: LINE, POINT, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, POLYLINE (2D, 3D, polyface and polygon meshes), SPLINE, TEXT,
  ATTRIB, ATTDEF, MTEXT, INSERT, MINSERT, the seven DIMENSION types, ARC_DIMENSION, LARGE_RADIAL_DIMENSION, HATCH, SOLID,
  TRACE, 3DFACE, LEADER, MULTILEADER, MLINE (with its element parameters), VIEWPORT, XLINE, RAY, IMAGE, WIPEOUT and
  ACAD_TABLE (its anonymous block, as for DXF).
- Not read, counted and reported: HELIX and MPOLYGON (drawn from DXF), REGION, 3DSOLID, BODY, MESH, SURFACE, TOLERANCE,
  SHAPE, underlays, OLE frames, lights and proxy entities.

## The same values as the DXF of the drawing

A DWG and the DXF that AutoCAD writes for it hold the same drawing in different forms. These rules make `read_dwg`
return what `parse_dxf` returns for that DXF. All are *inferred* from the samples unless the specification is cited.

- Angles are stored in radians and become degrees (`r × 180 / π`).
- **Block names**: the BLOCK_HEADER of an anonymous block has only the prefix (`*D`, `*U`, `*X`) and the BLOCK entity has
  the full name (`*D7`), which is used, as for `*Model_Space` and `*Paper_Space`. When the BLOCK has another name (a
  dynamic block saved as R2000 keeps the name of its definition), the block is named with the prefix and its position in
  the block table (`*U17`), as AutoCAD names it in DXF.
- **Flags**: the layer bit that the specification calls "on" means off (hidden). In text style flags, the shape file bit
  comes first. MLINESTYLE flags use other bits than DXF (1, 32 and 512 in DWG are 2, 64 and 1024 in DXF, and the
  reverse), and its line types are handles from R2018 on and table positions before (32767 BYLAYER, 32766 BYBLOCK).
- **Values that DXF leaves out**: the MTEXT direction (1, 0, 0), the dimension clone point (12) at the origin and the
  dimension angles 50, 51, 52 and 53 when they are zero are left out, as AutoCAD leaves them out of DXF.
- **Polygons**: the boundary of a polygonal WIPEOUT or IMAGE is closed with its first point, as DXF repeats it.
- **LEADER**: AutoCAD writes the hook of a leader with an annotation (annotation type below 3) as an extra vertex in DXF;
  DWG does not store it. It is added before the last vertex, at the arrow size times `DIMSCALE` along the leader's
  horizontal direction, backwards unless the hook is on the X direction.
- **VIEWPORT**: the ID (69) is the position in its layout, and the status (68) is the ID, or 0 for a viewport that is
  off (flag `0x20000`).
- **Attributes**: the owner of an ATTRIB is its INSERT.
- **Colors**: color book colors (DBCOLOR) give the color name and book; the index (62) is the low byte of the `ENC`
  color. Transparency types 0, 1 and 3 become none, BYBLOCK (`0x01000000`) and `0x020000TT`. MULTILEADER colors are kept
  as raw 32-bit values as in DXF; R2000 stores only an index, which becomes `0xC0…` (BYLAYER), `0xC1…` (BYBLOCK) or
  `0xC3…` (ACI).
- **Splines with fit points**: from R2013 on, a spline made from fit points (scenario 2) is stored with its fit points and
  end tangents only, while AutoCAD writes control points to DXF. For a degree 3 open spline with end tangents, the control
  points are computed by cubic interpolation with the end derivatives and chord-length parameters. With two fit points
  this gives AutoCAD's control points exactly (ACadSharp sample); with more, it is the standard construction, not
  confirmed against AutoCAD. A spline without tangents keeps its fit points and is drawn through them (`spline-fit`).

## Limits

The drawing limits of DXF apply ([How DXF is handled](dxf.md#limits)). Reading adds these:

| Limit | Value | Notice |
| --- | --- | --- |
| Size of one section after decompression | 256 MB (the rest is not read) | `dwg-section-large` |
| Entities followed through one block | 10,000,000 | |
| Pages that are missing or cannot be decompressed | Filled with zeros | `dwg-broken-page` |
| Objects that cannot be read | Left out | `dwg-broken-object` |

## Known differences from the DXF of the drawing

Compared with the DXF that AutoCAD wrote for the same drawing (see [verification.md](../verification.md#dwg-in-rust)):

- SPLINE flags (70): the planar bit and others that DWG does not store are missing.
- True colors: DXF also writes the nearest index of a true color (62 for layers, 63 for an MTEXT background), which
  DWG does not store; a layer with a true color gets index 7. The true color is drawn in both.
- Dimension styles: DXF leaves out values at their defaults (DIMASZ and DIMTXT 0.18 in an imperial drawing), which the
  DXF reader takes as 0, so the drawing uses the header's `$DIMASZ`; DWG stores every value.
- MULTILEADER background colors in R2000 are indexes only.
- AutoCAD writes some layers, anonymous blocks and their entities with new handles in DXF, so entity ids differ.
- Two differences come from the DXF reader: in R2018 files it reads the attribute type of multi-line ATTRIB and ATTDEF as
  their generation flags (71), and in binary DXF of R2004 and later it keeps the line feeds of MTEXT instead of breaking
  paragraphs.

Not supported yet: R13 and R14, the fit points of splines without end tangents, HELIX, MPOLYGON, the content of
ACAD_TABLE beyond its block, proxy graphics, and DWG files over the section limit.
