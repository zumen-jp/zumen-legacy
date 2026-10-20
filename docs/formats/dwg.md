# How DWG is handled

`zumen-dwg` (`crates/zumen-dwg`) reads DWG written by AutoCAD 2000 (AC1015) to AutoCAD 2018 and later (AC1032) into the
AutoCAD drawing model of `zumen-dxf` (`DxfDocument`), and `dxf_to_scene` draws it as it draws DXF. It is written in Rust
only, with its own decoder that follows the Open Design Alliance's
[Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf)
5.4.1 and cites its sections in comments ([decision 0003](../decisions/0003-dwg-in-rust.md)). There is no TypeScript
DWG reader: the `zumen` command, Python and the WebAssembly build (`bindings/wasm`) read DWG, and browsers read it
through that build with `@zumenjs/dwg` ([DWG in the browser](../architecture.md#dwg-in-the-browser)).

```text
bytes ─▶ file (container, pages, decompression) ─▶ objects (bit streams) ─▶ read_dwg ─▶ DxfDocument ─▶ dxf_to_scene ─▶ Scene
```

It also writes DWG: `rewrite_dwg` writes a drawing back in its own version with every object copied as its bits, the
base for editing a drawing in place, and `write_dwg` writes the AutoCAD drawing model as an AutoCAD 2013 DWG, which is
how a DXF, a DWG or a drawing of another format read into the model becomes DWG ([Writing](#writing)).

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
- R2018 MTEXT ends with the bit that the specification calls "Is NOT annotative" (20.4.46). When it is set, as in every
  MTEXT of the samples, the redundant fields and the column data follow, and they are read so that the values after an
  embedded MTEXT stay in step.
- **Multi-line attributes** (R2018): an ATTRIB or ATTDEF of attribute type 2 or 4 holds an embedded MTEXT from the
  entity mode (20.4.4), read like an MTEXT into `mtext` with its own extrusion, then the annotative data and the tag,
  field length and flags of a single-line attribute. Its own layer, color and line type are not used; the attribute's
  are, as ezdxf draws it from DXF. The attribute's own text is empty.

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
- **MULTILEADER arrows**: from R2010 on, each leader line has its own arrow; the arrow of a line whose override flags have
  32 (arrow symbol) is that line's arrow head (DXF 94 and 345, by the line's index). Before R2010 the arrow heads are a
  list of their own.
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
- MULTILEADER background colors in R2000 are indexes only.
- AutoCAD writes some layers, anonymous blocks and their entities with new handles in DXF, so entity ids differ.

The DXF reader used to differ in three more places, now read as DWG has them (see [How DXF is handled](dxf.md#reading)):
the dimension style values that DXF leaves out at their defaults, the attribute type of R2018 multi-line attributes
(71 in `AcDbAttribute`), and the line feeds of MTEXT in binary DXF of R2004 and later. The attribute flags (70) of R2018
multi-line attributes differed too (28 and 235 against 0 in the ACadSharp sample), because the DWG reader took the
"Is NOT annotative" bit of MTEXT the wrong way round and read the values after the embedded MTEXT out of step.

Not supported yet: R13 and R14, the fit points of splines without end tangents, HELIX, MPOLYGON, the content of
ACAD_TABLE beyond its block, proxy graphics, and DWG files over the section limit.

## Writing

`zumen-dwg` writes DWG in Rust, following the same specification, in two ways: `rewrite_dwg` copies a drawing, and
`write_dwg` writes the AutoCAD drawing model ([New drawings](#new-drawings)).

`rewrite_dwg(bytes)` reads a DWG and writes it back
in the same version: every object is copied as its bits, without decoding and encoding it again, into a new objects
section in the order of the source, and the object map is written for the new positions. The header variables and the
classes are copied with their CRCs computed again, and every other section (AcDb:AuxHeader, AcDb:SummaryInfo,
AcDb:Preview, AcDb:AppInfo, AcDb:AppInfoHistory, AcDb:FileDepList, AcDb:RevHistory, AcDb:ObjFreeSpace, AcDb:Template,
AcDb:AcDsPrototype_1b and any other) as it is. It returns the bytes and notices (`DwgWritten`), or a message for a file
it cannot write. The writer (`src/write`) is shared by the tests, which build the smallest drawing that pins a rule with
it (`tests/writer`).

### Containers

- **AC1015 (R2000)** is laid out as AutoCAD lays it out: the file header with six section locators (3.2.6, record 3
  empty), AcDb:AuxHeader (record 5), the preview, the header variables, the classes, the RL 0x0DCA and the objects, the
  object map, the second file header (26) and the template. A drawing that AutoCAD saved as R2000 is written back
  identical byte for byte, except eight bytes at the end of the second file header that AutoCAD fills with varying
  values (zumen writes zeros).
- **AC1018, AC1024, AC1027, AC1032 (R2004, 2010, 2013, 2018)**: pages from 0x100, numbered from 1 in file order; the
  section map and the page map are the last pages, and the copy of the encrypted file header follows (4.1–4.6). The
  sections keep their order in the section map and in the file, their page size, compression and encryption flag. A
  page that holds only zeros is not written, as AutoCAD does (4.5). Each page of a compressed section is padded with
  zeros to the page size and compressed on its own; an uncompressed page is stored whole. The gap after a page up to the
  next multiple of 0x20 is filled from the start of the magic sequence (4.3). The fields of the file header that point
  at pages (0x0D preview, 0x20 summary info, 0x24 VBA project, 0x2C app info, 0x30 app info history) get the new
  addresses; the other fields of the first 0x80 bytes are copied.
- AC1021 (R2007) is not written yet, and R13 and R14 are not supported. A password-protected drawing (security flag 1 at
  0x18) and a section over the section limit of reading are refused.

### Checks

- **CRC (2.14.1)**: the header variables and classes (from the size), every object (from the MS size), each chunk of the
  object map (big-endian, from the size) and the R2000 second file header start from 0xC0C1. The R2000 file header
  starts from 0 and is XORed with a value for the number of section locators (3.2.6), which gives the same CRC as
  starting from 0xC0C1.
- **CRC32 (2.14.2)** of the R2004 file header, from 0, over the 0x6C bytes with the CRC field set to zero.
- **Page checksums (4.2)**, which are Adler-32 with a seed: the data checksum from 0 over the stored data; the page header
  checksum seeded with the data checksum, over the 32 bytes of the header with its own field set to zero (4.6); a
  system page from 0 over its 0x14-byte header with the checksum set to zero, then over its compressed data (4.3).
- Every one of these values stored in the 17 sample drawings is computed again to the same value
  (`src/samples.rs`, run with `ZUMEN_SAMPLES`).

### Compression

The compressor finds earlier copies of three-byte sequences through hash chains and takes the longest. It uses only the
instructions that AutoCAD's files use (counted over the compressed pages of the samples): a Literal Length for the
first literals, 0x20–0x3F for matches up to 0x4000 back, 0x40–0xFF for 3 to 14 bytes up to 1,024 back, 0x10 and 0x12–0x17
for matches from 0x4001 to 0x7FFF back, and 0x11 0x00 0x00 at the end. Two forms are avoided because readers disagree on
them: a first byte above 0x11 for a short first run of literals (LZO's form; the specification has no such byte, and
ezdwg reads it as an instruction), so the first run is at least four bytes; and 0x18–0x1F, which the specification reads
as lengths and ACadSharp as an offset bit. The rewritten R2004 to 2018 samples are 3 to 11% smaller than the files AutoCAD
saved (`farmhouse.dwg`: 1.79 MB against 1.93 MB), and rewriting takes 75 ms for `farmhouse.dwg` (release build).

### Inferred rules

- **Second file header (R2000)**: the layout of the specification is that of R13 and R14. AutoCAD writes these
  bit-coded values: the address of this header (BL), the version string and the bytes 0x06–0x0C of the file header
  (RC), the bytes 0x11–0x14 of the file header (BL), the number of section locators (BL) and each locator (RC number, BL
  address, BL size; record 4 is zero although the file header points at the template), the number of handle records
  (BS 14) and each record (RC byte count, RC index, the handle's bytes high first), zero bits to the next byte, the CRC,
  and eight more bytes. The size counts from the size field to the end sentinel. The handle records are HANDSEED and
  the controls of blocks, layers, text styles, line types, views, UCSs, viewports, applications, dimension styles and
  viewport entity headers, the named object dictionary, the multiline style dictionary and the group dictionary.
- **Classes**: from R2004 on, the version and maintenance version of a class are BL, not BS as the specification says
  (a class with a maintenance version over 255 reads wrong as BS). Before R2007, AutoCAD ends every class string with a
  NUL byte that the length counts; other writers do not. The classes of every sample are written back to the same bits.
- **Framed sections**: the RL after the size of the header variables and classes (R2010 and R2013 with a maintenance
  version over 3, R2018) is zero in every sample, and from R2004 on the classes end with eight zero bytes after the end
  sentinel. Some files keep bytes of an earlier save after the end sentinel of the header variables; they are not
  copied. AutoCAD pads the last byte of these sections with varying bits.
- **Objects**: in AutoCAD's files the objects follow each other without gaps in the order of their handles, and each
  chunk of the object map holds at most 2,032 bytes; the map written for them is identical to AutoCAD's. Handles that
  point at the same position keep pointing at one copy.
- **Preview**: the positions of the header and image data in the preview (14.2) are file addresses in every version,
  so they are moved with the preview.
- **Page map**: AutoCAD records a size for the page map's own page that can differ from what it writes (an allocation
  from an earlier save) and puts the copy of the file header right after the page map's trailer. zumen records the
  written page rounded up to 0x20 and puts the copy at the same place. The pages are numbered without the gaps that
  AutoCAD leaves before the section map.

### Notices

| Code | When | What is written |
| --- | --- | --- |
| `dwg-crc` | The CRC of the header variables or classes does not match | The CRC of the content |
| `dwg-object-crc` | The CRC of an object does not match | The object as it is |
| `dwg-broken-object` | The object map points outside the objects | Nothing for that handle |
| `dwg-section-copied` | The sentinels of the header variables or classes cannot be read | The section as it is |
| `dwg-broken-page` | A page is missing or cannot be decompressed (as reading) | Zeros |

### Verification

- Each rewritten sample (R2000 to R2018, 15 drawings) reads to the same `DxfDocument` and the same scene in every view
  as the original, and rewriting it again gives the same bytes (`tests/write.rs`); damaged copies of the test drawings
  are rewritten without panicking.
- [ezdwg](https://github.com/monozukuri-ai/ezdwg) 0.16.3 reads every rewritten sample to the same entities, layers,
  layouts and header variables as the original.
- ODA File Converter (audit and conversion to DXF) and DWG TrueView: not run yet.

### New drawings

`write_dwg(&DxfDocument)` writes the model as AC1027 (AutoCAD 2013) and returns the bytes and notices. It makes the same
choices as the DXF writer (`zumen_dxf::write_dxf`): handles of the model are kept, and a handle that is missing,
unreadable or used twice gets a new one after the largest; layers, line types, text styles, dimension styles and blocks
that entities use but the model does not define are added; what cannot be written as it is is reported with the same
notices. DWG holds every value, so what the model does not hold is written with AutoCAD's defaults (the values of
`acad.dwt` saved as AutoCAD 2013, taken from AutoCAD's own files).

Every object is built as the structure of a lossless codec (`src/codec`) and written by it. The codecs read every value
of an object and write it back; each of them gives the same bits for every object of the samples of R2000 to R2018
(`src/samples.rs`), which is how the order and the forms of their values were checked.

- **Sections**: AcDb:Header, AcDb:AuxHeader, AcDb:Classes, AcDb:Handles, AcDb:Template, AcDb:ObjFreeSpace,
  AcDb:AcDbObjects, AcDb:RevHistory, AcDb:SummaryInfo, AcDb:Preview (no image), AcDb:AppInfo and AcDb:FileDepList, in
  AutoCAD's order and page sizes. AcDb:AuxHeader (27), AcDb:ObjFreeSpace (21) and AcDb:RevHistory (18) have the layout
  and constant values of AutoCAD's files; AcDb:AppInfo (16.2) names zumen and its version; the dates are `$TDCREATE` and
  `$TDUPDATE` of the model (zero when it has none, since the library has no clock). The Template keeps its zero page when
  the model has `$MEASUREMENT`, so that the reader finds it.
- **Header variables**: all of them (9), with AutoCAD's defaults, replaced by the model's variables that have the same
  name and a compatible type (all of a DXF's header; the ones the DWG reader keeps), then by the values that drawing uses
  (`$INSUNITS`, `$LTSCALE`, `$PSLTSCALE`, `$TEXTSIZE`, `$DIMSCALE`, `$DIMASZ`, `$DIMTXT`, `$TILEMODE`, extents and
  limits) and the handles of the tables and dictionaries. Julian dates become a day and milliseconds. The fingerprint and
  version GUIDs are the model's, or are made from the written objects, so the same model gives the same bytes.
- **Tables**: the controls with their entries, LTYPE (ByBlock, ByLayer and Continuous, kept at the end of the control
  as AutoCAD does), LAYER, STYLE, VPORT `*Active`, APPID, DIMSTYLE (the model's scale, arrow size and text height, the
  defaults otherwise), empty VIEW and UCS, and BLOCK_HEADER with its BLOCK and ENDBLK, owned entities and the handles of
  the INSERTs that use it.
- **Objects**: the named object dictionary with ACAD_COLOR, ACAD_GROUP, ACAD_LAYOUT, ACAD_MATERIAL (ByBlock, ByLayer,
  Global), ACAD_MLEADERSTYLE (Standard), ACAD_MLINESTYLE, ACAD_PLOTSETTINGS, ACAD_PLOTSTYLENAME (the dictionary with
  default `Normal` and its placeholder), ACAD_SCALELIST (`1:1`), ACAD_TABLESTYLE and ACAD_VISUALSTYLE (empty) and
  AcDbVariableDictionary (`CANNOSCALE`, `CMLEADERSTYLE`); a LAYOUT for the model and for each paper space block, with the
  model's paper settings; the multiline styles; and a SORTENTSTABLE for each block with a draw order.
- **Entities**: every kind the model holds: LINE, CIRCLE, ARC, ELLIPSE, POINT, LWPOLYLINE, POLYLINE with its vertices and
  faces, SPLINE, TEXT, MTEXT, INSERT and MINSERT with their attributes, ATTDEF, the dimension types, ARC_DIMENSION,
  LARGE_RADIAL_DIMENSION, HATCH, SOLID, TRACE, 3DFACE, LEADER, MULTILEADER, MLINE, VIEWPORT, XLINE, RAY and WIPEOUT.
  Entities of model space and of `*Paper_Space` have no owner handle; other entities, vertices, attributes and SEQEND
  are owned by their block, POLYLINE or INSERT, as in AutoCAD's files.

#### Values

- **Strings** are stored as UTF-16. The model holds the caret notation of DXF (`^J`, `^ `), which the DWG reader also
  produces, so it is turned back into characters; reading gives the same string, except for a string that is not in that
  notation (a control character in a DXF, or `^` followed by another character), which reading gives in the notation.
- **Angles**: degrees become radians chosen so that the reader's `r × 180 / π` gives the same degrees when such a value
  exists (the double rounding leaves some degrees out). The start and end of an arc are chosen together so that their
  difference is the same as in the model: the scene divides an arc by its sweep, and one unit in the last place over
  exactly 180° changes the division.
- **Colors** become `ENC` colors: an index (BYLAYER 256, BYBLOCK 0), or `0x8000` with `0xC2RRGGBB` for a true color, and
  `0x2000` with the DXF value for a transparency. A color book color, which the model holds as its true color, is
  written as that true color. Line weights take the nearest value of AutoCAD's list; line types BYLAYER, BYBLOCK and
  CONTINUOUS use the flags, others the handle.
- TEXT leaves out the values at their defaults with the data flags, as AutoCAD does, and a text with an alignment but
  no second point gets its first point, as the DXF writer does. MTEXT without a direction gets one from its rotation in
  its OCS.
- A SPLINE is written with its control points and knots (R2013 scenario 1, custom knot parameter), or with its fit points
  and tangents when it has no control points or when its control points are those the reader computes from the fit
  points (a spline read from a DWG keeps its fit points).
- A WIPEOUT boundary that ends at its first point is written without that point, which the reader adds back.

#### Inferred rules

- **Header variables**: from R2004 on, the four BS after the handle stream (R2007 and later) or the data are followed by
  BL 0, BL 0 and a bit (every sample ends this way; ACadSharp writes them too).
- **Dimensions**: AutoCAD sets bit 8 of Flags 1 in every dimension of the samples; it is set.
- **MLINE**: the BS that the specification calls open (1) or closed (3) holds the flags of DXF's 71, with 4 and 8 for
  caps that are not drawn.
- **SPLINE** (R2013 and later): AutoCAD writes scenario 1 for every spline; fit data is read when the knot parameter is
  not custom (15) and flag 8 (use knot parameter) is set, as the reader and ACadSharp read it.
- **BLOCK_HEADER**: the owned entity count is left out for external references, as ACadSharp reads and writes it (the
  specification has it in every version).
- **Classes**: the values of AutoCAD's own files (proxy flags, application, version and maintenance version), also those
  that AutoCAD 2018 writes into older formats (33 and 329 for MLEADERSTYLE, MULTILEADER and WIPEOUT).
- **Layers** that are off have bit 2 of their flags set; the color keeps its index (every sample).

#### Notices of new drawings

The codes of the DXF writer, with the same messages, and one more:

| Code | When | What is written |
| --- | --- | --- |
| `missing-block` | An entity uses a block that is not defined | An empty block |
| `complex-linetype` | A line type has text or shapes | Its dashes and gaps |
| `helix` | A HELIX | A SPLINE |
| `mpolygon` | An MPOLYGON | A HATCH |
| `table` | An ACAD_TABLE | An INSERT of its block |
| `attrib-alone` | An ATTRIB that no INSERT owns | A TEXT or MTEXT |
| `image` | An IMAGE (the model does not keep the image file) | Nothing |
| `dimension-block` | A dimension without its block | Nothing |
| `mleader-block` | A MULTILEADER whose block content is not defined | No content |
| `multiline-attrib` | A multi-line attribute, which AutoCAD 2013 DWG cannot hold (the embedded MTEXT is R2018) | A single-line attribute with the text on one line |

What the model does not hold is not written: entity types that the readers skip (counted in `skipped`), extended data
and extension dictionaries, the handles of vertices, the lock position of attributes, the associativity of hatches and
of leaders to their annotation (a leader keeps the hook vertex that the reader adds), visual styles and table styles.

#### Verification of new drawings

- Every DXF and DWG of `tests/fixtures`, written as DWG and read again, draws the same in every view as the model it was
  written from (`tests/write_model.rs`, as `crates/zumen/tests/convert.rs` checks the DXF writer). The model is first
  given the values that reading a DWG gives (multi-line attributes made single-line, strings in caret notation, polygon
  boundaries closed), and splines with only fit points are not compared: the DWG reader computes their control points as
  AutoCAD does, while the DXF reader draws a curve through the points.
- The same holds for the 42 sample drawings that zumen reads (DXF and DWG, R12 to 2018) with `ZUMEN_SAMPLES`.
- ezdwg 0.16.3 reads every written drawing. For the samples that are DWG, it reads the same values for every entity as
  from the original, apart from what the model does not hold (above) and values that ezdwg reads differently from any
  R2013 file (owners of INSERTs, names of hatches).
- ODA File Converter and DWG TrueView: not run yet.
