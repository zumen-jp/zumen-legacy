# How DWG is handled

`zumen-dwg` (`crates/zumen-dwg`) reads DWG written by AutoCAD 1.0 (MC0.0) to AutoCAD 2018 and later (AC1032) into
the AutoCAD drawing model of `zumen-dxf` (`DxfDocument`), and `dxf_to_scene` draws it as it draws DXF. It is written in
Rust only, with its own decoder that follows the Open Design Alliance's
[Open Design Specification for .dwg files](https://opendesign.com/files/guestdownloads/OpenDesign_Specification_for_.dwg_files.pdf)
5.4.1 and cites its sections in comments ([decision 0003](../decisions/0003-dwg-in-rust.md)). There is no TypeScript
DWG reader: the `zumen` command, Python and the WebAssembly build (`bindings/wasm`) read DWG, and browsers read it
through that build with `@zumenjs/dwg` ([DWG in the browser](../architecture.md#dwg-in-the-browser)).

```text
bytes ─▶ file (container, pages, decompression) ─▶ objects (bit streams) ─▶ read_dwg ─▶ DxfDocument ─▶ dxf_to_scene ─▶ Scene
```

It also writes DWG: `rewrite_dwg` writes a drawing back in its own version with every object copied as its bits,
`write_dwg` writes the AutoCAD drawing model as an AutoCAD 2013 DWG, which is how a DXF, a DWG or a drawing of another
format read into the model becomes DWG ([Writing](#writing)), and `edit_dwg` changes a drawing in place, keeping every
object that no operation names ([Editing](#editing-edit_dwg)).

Where the specification is silent or does not match the files, the behavior was checked against the same drawing
saved as DXF by AutoCAD and against the MIT implementations [ACadSharp](https://github.com/DomCR/ACadSharp) and
[ezdwg](https://github.com/monozukuri-ai/ezdwg). Those rules are marked *inferred* below. LibreDWG (GPL) is not used and
its source was not read. The specification starts at R13; drawings of Release 12 and earlier follow other descriptions
([R12 and earlier](#r12-and-earlier)).

## Versions and containers

- **Detection**: the file starts with `AC10` and two digits, or with `MC0.0`, `AC1.2`, `AC1.40`, `AC1.50` or `AC2.10`
  followed by a NUL. Unknown versions are refused with a message.
- **AC1012, AC1014, AC1015 (R13, R14, R2000)**: the section locators of the file header (3.2.1) give the header
  variables, the classes, the object map and the template section (`$MEASUREMENT`). R13 and R14 have the same container
  as R2000, without AcDb:AuxHeader ([R13 and R14](#r13-and-r14)).
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

## R13 and R14

R13 and R14 store most values in full where R2000 compresses them, have values that R2000 moved to the registry, and lack
those that R2000 added. The differences read are:

- **Objects**: the size of the object data (RL) follows the extended data, and for entities the graphic (20.1, 20.4.1);
  from R2000 on it follows the type.
- **Entities** (20.4.1, 20.4.2): the layer and, unless the line type is BYLAYER, the line type handle come before the
  links of the previous and next entity; there are no line type flags, plot style, material or line weight, so the line
  weight is BYLAYER, as DXF of these versions has none. Thickness and extrusion are a BD and a 3BD (2.8, 2.10). LINE,
  3DFACE, TEXT, ATTRIB and ATTDEF, the scale of INSERT, MINSERT and ACAD_TABLE, and the vertices of LWPOLYLINE store every
  value. MTEXT has no line spacing (DXF's defaults are used), and dimensions have no attachment point, line spacing or
  measurement (left out, as in DXF of these versions). LEADER has the R13–R14 values after the hook; R13 has no end point
  projection.
- **Header variables** (9): the values of R13 and R14 only (an unknown BS, `DIMSAV`, `BLIPMODE`, `ATTREQ`, `ATTDIA`,
  `WIREFRAME`, `DELOBJ`, `DRAGMODE`, `OSMODE`, `COORDS`, `PICKSTYLE`, and the dimension variables as bits, bytes and
  names), and none of R2000's (`PSVPSCALE`, the orthographic UCS, `$INSUNITS` and the following values, the layout and
  plot dictionaries). `$INSUNITS` is 0.
- **Tables**: a layer has four bits (frozen, off, frozen in new viewports, locked) instead of the flags; the off bit set
  means off, as in R2000 (*inferred*). It is plotted and has the default line weight, as DXF of these versions reads.
  DIMSTYLE has the R13–R14 values before `DIMSCALE`. BLOCK_HEADER has no loaded bit, insert count, description, preview,
  insert handles or layout. An R14 DICTIONARY has an unknown byte where R2000 has the cloning and hard owner flags; R13
  has neither. A dictionary with a default (ACDBDICTIONARYWDFLT, a class of R2000) has the R2000 form in R14 too, as do
  the other objects of classes that R14 does not have.
- **Viewports**: a VIEWPORT has only its center and size. Its view (target, direction, twist, height and center) is read
  from the `MVIEW` extended data of the `ACAD` application, as the DXF reader reads an R12 viewport.
- **Layouts**: AutoCAD R13 and R14 have no LAYOUT. A drawing that AutoCAD 2000 or later saved as R13 or R14 keeps its
  layouts: LAYOUT objects in the `ACAD_LAYOUT` entry of the named object dictionary, with the entities of the other
  layouts in `*Paper_Space0`, `*Paper_Space1` and so on (AutoCAD 2000 and later open them; R14 shows only
  `*Paper_Space`). They are read as in R2000. Otherwise, when paper space has entities, the drawing gets one layout,
  `Layout1`, on the paper space block with the paper space limits. Inferred: template `$MEASUREMENT = 0` makes those
  limits inches; metric limits are millimeters. The sheet and paper-space entities use the same conversion to
  millimeters.
- **Text alignment**: TEXT always stores the alignment point; it is kept only for an alignment other than left on the
  baseline, as AutoCAD leaves it out of DXF and of R2000 drawings (*inferred*).

Checked against real files: R14, with eight drawings. One of them AutoCAD saved both as R14 and as R2000; the two read the
same entity by entity, table by table and in every header variable they share, except for what R14 does not store (line
weights, plot flags, block descriptions, `$INSUNITS`, dimension measurements, MTEXT line spacing) and what AutoCAD changed
when it saved the drawing as R14 (table names in capitals, names with characters R14 does not allow, one fraction of
MTEXT, the last digits of some coordinates). ezdwg reads the same coordinates and strings from the eight drawings. R13
is covered by generated drawings checked against the equivalent R2000 model; no native R13 drawing was available to check.
MVIEW extended data also preserves frozen-layer handles, so those layers stay hidden in the viewport.

## R12 and earlier

Drawings of AutoCAD 1.0 to Release 12 have another layout, which the Open Design Specification does not cover. They are
read (`src/r12`) following the descriptions in Kaitai Struct of Michal Josef Špaček, one for each version (for example
`dwg_ac1009.ksy`, CC0-1.0), and the reading of R10 to R12 in a fork of ACadSharp
([FriendsOfCADability/ACadSharp](https://github.com/FriendsOfCADability/ACadSharp), MIT), which follows the same
descriptions.

| Version | AutoCAD | Read | Checked against real drawings |
| --- | --- | --- | --- |
| `MC0.0` | 1.0 | yes | no drawing was available |
| `AC1.2` | 1.2 | yes | no drawing was available |
| `AC1.40` | 1.4 | yes | 80 drawings |
| `AC1.50` | 2.0 | yes | no drawing was available |
| `AC2.10` | 2.1 | yes | 179 drawings |
| `AC1001` | 2.2 | yes | no drawing was available |
| `AC1002` | 2.5 | yes | no drawing was available |
| `AC1003` | 2.6 | yes | 272 drawings |
| `AC1004` | Release 9 | yes | 1 empty drawing |
| `AC1006` | Release 10 | yes | 32 drawings |
| `AC1009` | Release 11 and 12 | yes | 27 drawings, saved by Release 11 |

The checks on real drawings: every entity record ends where its size says (474 records, all but the AutoCAD 1.x
versions, whose records have no size), the CRCs of the R11 records match, and the values read are those the drawings were
made with (coordinates, scales, rotations, texts, layers, block names).

- **Layout** (AC1.50 on): the file header gives the entities section, the block entities section and the extra entities
  section, and the position, record size and count of the tables (block, layer, style, line type and view at 0x2C to
  0x5D; from R10 UCS, viewport and application; from R11 dimension style at 0x522). The header variables follow at fixed
  positions that later versions only extend (`$EXTMIN` at 0x78, `$LTSCALE` at 0x132, `$DIMSCALE` at 0x1A3; from R11
  `$DWGCODEPAGE` at 0x3F9 and the paper space values from 0x631). Table records have a fixed size; a record whose flags
  have 0x80 is deleted.
- **Entities** are records of a type (negative or 0x80 for a deleted entity), a mode, a size, a layer index and 16 bits
  that say which optional values follow, then the optional color, line type, elevation, thickness, handle (R10 on) and,
  from R11, extended data and the viewport of paper space entities, and then the values of the type. R11 records end
  with a CRC (0xC0C1 seed, as from R13). Release 11 writes the thickness before the elevation, earlier versions the
  reverse; from R10, LINE, POINT, 3DLINE and 3DFACE use the elevation bit to leave out the z of their points.
- **JUMP** records link the entities section with the extra entities section, where entities added after the first save
  are; the reader follows them, and returns after the jump when the extra section ends without one.
- **References** are indexes: of layers, line types (R11: 32767 BYLAYER and 32766 BYBLOCK; earlier 255 BYBLOCK), text
  styles, blocks and dimension styles. An anonymous block has only its prefix (`*D`), in the table and in its BLOCK, and
  gets the number of its position in the block table (`*D1`), as from R13 on.
- **Paper space** (R11 and R12): the entities that carry a viewport number are in paper space, and the drawing gets
  `Layout1`, as the DXF reader does for R12. Viewports take their view from the `MVIEW` extended data.
- **AutoCAD 1.x** (`MC0.0`, `AC1.2`, `AC1.40`): the entities follow the header variables, with a fixed record per type
  and the block definitions among them (BLOCK to ENDBLK, referenced by name). The 128 layers are numbered (their names
  are the numbers); the header holds their colors (255 for an unused layer, 0x80 for a layer that is off, 0 for white).
- **REPEAT** (up to AutoCAD 2.x) repeats the entities up to ENDREP in columns and rows; they are copied, as AutoCAD drew
  them.
- **Text**: R11 and R12 give `$DWGCODEPAGE`, numbered as from R13. Earlier drawings have no code page; as the DXF reader
  does, their strings are read as Shift_JIS when they decode as Shift_JIS and contain Japanese (`guessed-shift-jis`), and
  as Windows-1252 otherwise.
- **Ids**: an entity without a handle (R10 to R12 without handles, all earlier drawings) is `#n`, where n counts the
  entity records of the file from 1 in the order they are stored: the entities section, the block entities section, the
  extra entities section, and deleted entities and JUMP records too, so that the number does not change when more types
  are read.
- **Not read**: SHAPE and LOAD (shape files) are counted (`skipped-types`), as SHAPE is from R13 on. `rewrite_dwg` and
  `edit_dwg` refuse these drawings.

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
  dictionary, from R2000 on); multiline styles (`ACAD_MLINESTYLE`); draw order (SORTENTSTABLE); color book colors
  (DBCOLOR in `ACAD_COLOR`); the image frame setting (RASTERVARIABLES in `ACAD_IMAGE_VARS`) when the drawing has
  images.
- Entities: LINE, POINT, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, POLYLINE (2D, 3D, polyface and polygon meshes), SPLINE, TEXT,
  ATTRIB, ATTDEF, MTEXT, INSERT, MINSERT, the seven DIMENSION types, ARC_DIMENSION, LARGE_RADIAL_DIMENSION, HATCH, SOLID,
  TRACE, 3DFACE, LEADER, MULTILEADER, MLINE (with its element parameters), VIEWPORT, XLINE, RAY, IMAGE, WIPEOUT,
  PDFUNDERLAY (with the file and page of its PDFDEFINITION, drawn as from DXF: [PDF underlays](dxf.md#pdf-underlays))
  and ACAD_TABLE (its anonymous block, as for DXF).
- Not read, counted and reported: HELIX and MPOLYGON (drawn from DXF), REGION, 3DSOLID, BODY, MESH, SURFACE, TOLERANCE,
  SHAPE, DWF and DGN underlays, OLE frames, lights and proxy entities.
- External references: the bits of BLOCK_HEADER for an attached and an overlaid reference (20.4.52) become the flags 4
  and 8 of DXF, and its path the path (group 1), so references to them are counted in the `xref` notice as from DXF
  ([Entities](dxf.md#entities)). The drawings they refer to are not read.
- `$MEASUREMENT` follows the template's description (22). From R2007 the description is UTF-16 and its length counts
  characters: AutoCAD writes one NUL character there (the specification counts bytes, which would read 256 for a metric
  drawing).

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
- **IMAGE**: brightness, contrast and fade (20.4.80, RC) are DXF 281, 282 and 283, and the clip mode bit (R2010 and
  later) is 290. The IMAGEDEF (20.4.82) gives the file name and the loaded bit (DXF 280). The ACadSharp samples of R2000
  to R2018 give the same underlay from the DWG and from the DXF that AutoCAD wrote.
- **PDFUNDERLAY**: the specification does not describe it. Its data is read in the order of ACadSharp 3 (MIT,
  `DwgObjectReader.readPdfUnderlay`): the normal (3BD, DXF 210), the insertion point (3BD, 10), the rotation (BD in
  radians, 50), the three scales (BD, 41 to 43), the flags, contrast and fade (RC, 280 to 282), the number of clip
  boundary points (BL) and the points (2RD, 11), and the handle of the PDFDEFINITION, whose data is the file name and
  the page (TV, 1 and 2, `readPdfDefinition`). Scales of 0 are read as 1, as from DXF. The ACadSharp samples of R14 to
  R2018 give the same underlay from the DWG and from the DXF that AutoCAD wrote.
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

Not supported yet: the fit points of splines without end tangents, HELIX, MPOLYGON, the content of ACAD_TABLE beyond
its block, proxy graphics, and DWG files over the section limit.

## Writing

`zumen-dwg` writes DWG in Rust, following the same specification, in two ways: `rewrite_dwg` copies a drawing, and
`write_dwg` writes the AutoCAD drawing model ([New drawings](#new-drawings)), as AutoCAD 2013 or, with `write_dwg_as`,
as any version from AutoCAD R13 to 2018 ([Versions of new drawings](#versions-of-new-drawings)).

`rewrite_dwg(bytes)` reads a DWG of AutoCAD R13 to 2018 (AC1012 to AC1032, R2007 included) and writes it back
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
- **AC1012, AC1014 (R13, R14)** are laid out as R2000 without AcDb:AuxHeader: five section locators (record 4 the
  template). The eight R14 sample drawings are written back identical byte for byte, except the end of the second file
  header: the padding bits of its last byte, which AutoCAD fills with varying bits, the CRC over them and the eight
  bytes after it.
- **AC1018, AC1024, AC1027, AC1032 (R2004, 2010, 2013, 2018)**: pages from 0x100, numbered from 1 in file order; the
  section map and the page map are the last pages, and the copy of the encrypted file header follows (4.1–4.6). The
  sections keep their order in the section map and in the file, their page size, compression and encryption flag. A
  page that holds only zeros is not written, as AutoCAD does (4.5). Each page of a compressed section is padded with
  zeros to the page size and compressed on its own; an uncompressed page is stored whole. The gap after a page up to the
  next multiple of 0x20 is filled from the start of the magic sequence (4.3). The fields of the file header that point
  at pages (0x0D preview, 0x20 summary info, 0x24 VBA project, 0x2C app info, 0x30 app info history) get the new
  addresses; the other fields of the first 0x80 bytes are copied.
- **AC1021 (R2007)**: the 0x80 bytes of meta data, the file header page at 0x80 (0x400 bytes), the page map and its copy,
  the data pages in the order of the source, the section map and its copy, and the copy of the file header page at the
  end (5.1). Every page is Reed-Solomon coded (5.13) and compressed with the R2007 compression (5.10) when that makes it
  shorter; the sections keep their order, page size, encoding, encryption value and hash code. A page that holds only
  zeros is written too (AutoCAD leaves it out; the reader reads a section without pages as empty). Pages of a section
  hold the page size each, the last one the rest. The meta data fields that point at pages (0x0D preview, 0x20 summary
  info, 0x24 VBA project, 0x2C app info) get the new addresses; the other fields are copied. The random seed of the file
  header is kept, so the same drawing gives the same bytes.
- Release 12 and earlier are read only. A password-protected drawing
  (security flag 1 at 0x18, or an R2007 section with encryption 1) and a section over the section limit of reading are
  refused.

### Checks

- **CRC (2.14.1)**: the header variables and classes (from the size), every object (from the MS size), each chunk of the
  object map (big-endian, from the size) and the R2000 second file header start from 0xC0C1. The R2000 file header
  starts from 0 and is XORed with a value for the number of section locators (3.2.6), which gives the same CRC as
  starting from 0xC0C1.
- **CRC32 (2.14.2)** of the R2004 file header, from 0, over the 0x6C bytes with the CRC field set to zero.
- **Page checksums (4.2)**, which are Adler-32 with a seed: the data checksum from 0 over the stored data; the page header
  checksum seeded with the data checksum, over the 32 bytes of the header with its own field set to zero (4.6); a
  system page from 0 over its 0x14-byte header with the checksum set to zero, then over its compressed data (4.3).
- **R2007 (5.2–5.4, 5.12, 5.13)**: Reed-Solomon parity of every block of the file header page, the system pages and the
  data pages; 64-bit CRCs of the file header data (normal, seeded with UpdateSeed2 of its length) and of its compressed
  form (the same), of the check sequence (normal, UpdateSeed1), of the page map and section map before and after
  compression and of each data page as stored (mirrored, UpdateSeed1); the two CRCs of the check data at the end of the
  file header page; and the checksum of each data page (5.4.1, seed 0, over the decompressed data).
- Every one of these values stored in the 8 sample drawings of R14, the 17 of R2000 to R2018 and 8 drawings of R2007
  is computed again to the same value, and the rewritten R2007 drawings pass the same checks (`src/samples.rs`, run
  with `ZUMEN_SAMPLES`).

### Compression

The compressor finds earlier copies of three-byte sequences through hash chains and takes the longest. It uses only the
instructions that AutoCAD's files use (counted over the compressed pages of the samples): a Literal Length for the
first literals, 0x20–0x3F for matches up to 0x4000 back, 0x40–0xFF for 3 to 14 bytes up to 1,024 back, 0x10 and 0x12–0x17
for matches from 0x4001 to 0x7FFF back, and 0x11 0x00 0x00 at the end. Two forms are avoided because readers disagree on
them: a first byte above 0x11 for a short first run of literals (LZO's form; the specification has no such byte, and
ezdwg reads it as an instruction), so the first run is at least four bytes; and 0x18–0x1F, which the specification reads
as lengths and ACadSharp as an offset bit. The rewritten R2004 to 2018 samples are 3 to 11% smaller than the files AutoCAD
saved (`farmhouse.dwg`: 1.79 MB against 1.93 MB), and rewriting takes 75 ms for `farmhouse.dwg` (release build).

The R2007 compression (5.10) finds matches the same way. It uses every instruction of 5.10.2, as AutoCAD's files do:
0x30–0xFF for 3 to 15 bytes up to 512 back, 0x10–0x1F for 3 to 18 bytes up to 8,192 back, 0x00–0x0F for 19 to 50
bytes up to 4,096 back (0xF0–0xFF right after another match, where 0x00–0x0F marks literals), 0x20–0x27 for up to 255
bytes and 0x28–0x2F for longer ones up to 65,535 back. The low three bits of an instruction's last byte count the
literals that follow (1 to 7); longer runs get a literal length byte, and a first run shorter than eight bytes is
written after a zero-length 0x20 instruction, as AutoCAD writes it. Literals are stored in the byte order that
decompression undoes (5.10.1). The rewritten R2007 samples are 2 to 3% smaller than AutoCAD's files.

### Inferred rules

- **Second file header (R13 to R2000)**: the specification gives it as bytes. AutoCAD's R14 and R2000 drawings hold
  these bit-coded values: the address of this header (BL), the version string and the bytes 0x06–0x0C of the file header
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
- **R2007 Reed-Solomon (5.13)**: system pages use (255, 239) with x^8 + x^6 + x^5 + x^3 + 1 (0x169), data pages
  (255, 251) with x^8 + x^4 + x^3 + x^2 + 1 (0x11D); the generator polynomial has the roots α^1 to α^(255 − k); a block
  is its data followed by its parity, lowest degree first (the byte order of the Rockliff implementation that the
  specification names). The parity of every block of the R2007 samples is reproduced.
- **R2007 CRCs and checksums (5.12, 5.4.1)**: the mirrored table is that of the polynomial 0xAD93D23594C935A9 (Jones,
  bits reversed) and the normal one ECMA-182; the CRC seed of AutoCAD's files is 0. Which flavor and seed update each
  value uses is not stated for all of them; the choices above are those that give the stored values. The page checksum
  is computed over the decompressed data.
- **R2007 padding (5.11)**: the bytes that fill a Reed-Solomon block, follow a page or follow the encoded file header
  are the 512-byte padding of the random encoding seeded with the file header's RandomSeed, from its start (repeated for
  longer gaps). AutoCAD fills the bytes up to a multiple of eight before encoding with leftovers of earlier data; zumen
  writes zeros in pages and the padding in the file header, as the specification says, and pads the page map with the
  padding from its start too.
- **R2007 pages of encoding 1** (AcDb:SummaryInfo, AcDb:Preview, AcDb:AppInfo and others that are not compressed):
  the data padded to a multiple of eight, then the parity of each block (the last block is filled with the padding for
  the parity but the fill is not written), then padding. The page is as large as a page full of data would be (the page
  size plus four bytes per block, rounded up to 0x20). A page of encoding 4 is its interleaved blocks rounded up to 0x20.
- **R2007 maps**: the section map ends with an unnamed empty section (size 0, page size 0xF800, encoding 4), as in every
  sample. AutoCAD numbers the data pages from 3 in file order, then the section map and its copy, and two numbers later
  the page map and its copy; zumen does the same. The page size recorded for each page in the section map varies in
  AutoCAD's files and is not read; zumen records the section's page size. The page map's copy follows the page map, the
  section map's copy the section map, as in AutoCAD's smaller files.

### Notices

| Code | When | What is written |
| --- | --- | --- |
| `dwg-crc` | The CRC of the header variables or classes does not match | The CRC of the content |
| `dwg-object-crc` | The CRC of an object does not match | The object as it is |
| `dwg-broken-object` | The object map points outside the objects | Nothing for that handle |
| `dwg-section-copied` | The sentinels of the header variables or classes cannot be read | The section as it is |
| `dwg-broken-page` | A page is missing or cannot be decompressed (as reading) | Zeros |

### Verification

- Each rewritten sample (R14, 8 drawings, R2000 to R2018, 15 drawings, and 8 drawings of R2007) reads to the same
  `DxfDocument` and the same scene in every view as the original, and rewriting it again gives the same bytes
  (`tests/write.rs`); so do the test drawings and their model written as R13 and R14, and damaged copies of them are
  rewritten without panicking. Every section of a rewritten R2007 drawing holds the same
  data with the same properties as the original (`src/samples.rs`).
- Without real drawings, a small R2007 drawing built in the tests is rewritten with the same sections and the same bytes
  on a second rewrite, 300 damaged copies of it are rewritten without panicking, and sections of every kind written into
  an R2007 container read back unchanged and pass the checks above (`src/write`).
- [ezdwg](https://github.com/monozukuri-ai/ezdwg) 0.16.3 reads every rewritten sample to the same entities, layers,
  layouts and header variables as the original.
- ACadSharp 3.8.26 (`DwgReader`, failsafe off) reads every rewritten sample to the same entities and the same
  notifications as the original. It cannot read an R2007 file header that is not compressed, so zumen compresses it as
  AutoCAD does.
- ODA File Converter (audit and conversion to DXF) and DWG TrueView: not run yet.

### New drawings

`write_dwg(&DxfDocument)` writes the model as AC1027 (AutoCAD 2013) and returns the bytes and notices;
`write_dwg_as(&DxfDocument, DwgVersion)` writes it in a chosen version (below). It makes the same
choices as the DXF writer (`zumen_dxf::write_dxf`): handles of the model are kept, and a handle that is missing,
unreadable or used twice gets a new one after the largest; layers, line types, text styles, dimension styles and blocks
that entities use but the model does not define are added; what cannot be written as it is is reported with the same
notices. DWG holds every value, so what the model does not hold is written with AutoCAD's defaults (the values of
`acad.dwt` saved as AutoCAD 2013, taken from AutoCAD's own files).

Every object is built as the structure of a lossless codec (`src/codec`) and written by it. The codecs read every value
of an object and write it back; each of them gives the same bits for every object of the samples of R14 to R2018
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
  The length of a string is a BS (2.2), so a string is cut at 32,767 UTF-16 units, without splitting a surrogate pair.
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
- An IMAGE keeps its brightness, contrast and fade (RC) and, from R2010, its clip mode; its IMAGEDEF keeps the loaded
  bit. The image frame setting (RASTERVARIABLES) is not written (`image-frame`).

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

The codes of the DXF writer, with the same messages, and two more:

| Code | When | What is written |
| --- | --- | --- |
| `missing-block` | An entity uses a block that is not defined | An empty block |
| `complex-linetype` | A line type has text or shapes | Its dashes and gaps |
| `helix` | A HELIX | A SPLINE |
| `mpolygon` | An MPOLYGON | A HATCH |
| `table` | An ACAD_TABLE | An INSERT of its block |
| `attrib-alone` | An ATTRIB that no INSERT owns | A TEXT or MTEXT |
| `image` | An IMAGE whose image definition (IMAGEDEF) the model does not know | Nothing |
| `image-frame` | The drawing hides image frames (`IMAGEFRAME` 0) | Nothing: the frames show |
| `pdf-underlay` | A PDF underlay (PDFUNDERLAY), which reading did not keep before either | Nothing |
| `dimension-block` | A dimension without its block | Nothing |
| `mleader-block` | A MULTILEADER whose block content is not defined | No content |
| `multiline-attrib` | A multi-line attribute, which DWG before AutoCAD 2018 cannot hold (the embedded MTEXT is R2018) | A single-line attribute with the text on one line |
| `long-text` | A string longer than 32,767 UTF-16 units, the longest a DWG string can be | The string up to that length |

What the model does not hold is not written: entity types that the readers skip (counted in `skipped`), extended data
and extension dictionaries, the handles of vertices, the lock position of attributes, the associativity of hatches and
of leaders to their annotation (a leader keeps the hook vertex that the reader adds), visual styles and table styles.

#### Verification of new drawings

- Every DXF and DWG of `tests/fixtures`, written as DWG and read again, draws the same in every view as the model it was
  written from (`tests/write_model.rs`, as `crates/zumen/tests/convert.rs` checks the DXF writer). Models read from
  damaged copies of them are written without panicking, and the written drawings read again. The model is first
  given the values that reading a DWG gives (multi-line attributes made single-line, strings in caret notation, polygon
  boundaries closed), and splines with only fit points are not compared: the DWG reader computes their control points as
  AutoCAD does, while the DXF reader draws a curve through the points.
- The same holds for the 42 sample drawings that zumen reads (DXF and DWG, R12 to 2018) with `ZUMEN_SAMPLES`.
- ezdwg 0.16.3 reads every written drawing. For the samples that are DWG, it reads the same values for every entity as
  from the original, apart from what the model does not hold (above) and values that ezdwg reads differently from any
  R2013 file (owners of INSERTs, names of hatches).
- ACadSharp 3.8.26 (`DwgReader`, failsafe off) reads every written drawing, with no warning but one for the
  LARGE_RADIAL_DIMENSION class that it does not know. For the samples that are DWG, it finds the same entities of each
  type as in the original, apart from the types that the model does not hold.
- ODA File Converter and DWG TrueView: not run yet.

### Versions of new drawings

`write_dwg_as(&DxfDocument, DwgVersion)` writes the model as `DwgVersion::R13` (AC1012), `R14` (AC1014), `R2000`
(AC1015), `R2004` (AC1018), `R2007` (AC1021), `R2010` (AC1024), `R2013` (AC1027, what `write_dwg` writes) or `R2018`
(AC1032), so that a drawing can be handed to someone on an older AutoCAD. `DwgVersion::parse` reads `R14`, `14`,
`AC1014`, `1014`, `2000`, `R2000`, `AC1015` or `1015` (any case), as `zumen_dxf::DxfVersion::parse` does; `code()` and
`name()` give `AC1014` and `R14`, `AC1015` and `2000`. Every version holds the same objects (AutoCAD also keeps
layouts, materials, scales, multileader and visual styles in older formats when it saves to them); the codecs write the
values of each version, and the model writer gives each version what it needs:

| Version | Maintenance (0x0B, 0x12) | Application (0x11, AuxHeader) | Container |
| --- | --- | --- | --- |
| R13 | 0 | 19 | R2000 (3) without AuxHeader |
| R14 | 10 | 21 | R2000 (3) without AuxHeader |
| 2000 | 15 | 23 | R2000 (3): header, AuxHeader, preview, sections, second header (26); the Template only when the model has `$MEASUREMENT` |
| 2004 | 104 | 25 | R2004 (4) |
| 2007 | 50 | 27 | R2007 (5): the section names, hash codes, page sizes and encodings of 5.x; the random seed comes from the header and objects, so the same model gives the same bytes |
| 2010 | 226 | 29 | R2004 (4); the header and classes sections have the RL after their size |
| 2013 | 8 | 31 | R2004 (4); as above |
| 2018 | 0 | 33 | R2004 (4); as above |

The maintenance versions are those of AutoCAD's own files of each version (AutoCAD 2013's for 2013; no R13 file was
available, so 0); the application version is that of the AutoCAD that first wrote the version.

- **Sections**: AcDb:SummaryInfo has 8-bit strings before R2007 (13). AcDb:AppInfo has the R18 form for 2004 and the R21
  form from 2007 (16; its form follows the application version). AcDb:ObjFreeSpace has 32-bit values up to R2007 (21.1)
  and 64-bit values from R2010, and AcDb:AuxHeader holds the maintenance version as an RL in R2018 (129 bytes), as in
  AutoCAD's files. An R2007 drawing without `$MEASUREMENT` has an empty Template, since that container writes zero pages
  too.
- **Strings** before R2007 are in the drawing's code page, chosen as the DXF writer chooses `$DWGCODEPAGE`: the model's
  code page or encoding when it is not western, otherwise the page that can show the most characters of the drawing.
  A character that the page cannot show is written as `\U+XXXX`, which the reader turns back. A string of extended data
  is a byte length, the code page as two bytes with the high byte first (AutoCAD's files; the specification gives no
  order) and the bytes. Strings are cut at 32,767 bytes.
- **R13 to R2000**: entities are found by their links (20.4.1): each entity holds its previous and next entity (0 at the ends,
  Nolinks when they are the handles before and after, as AutoCAD writes them), BLOCK_HEADER holds its first and last
  entity, INSERT its first and last attribute and POLYLINE its first and last vertex. Vertices, attributes and their
  SEQEND are placed in the file right after their owner, as in AutoCAD's files (ezdwg finds the vertices of polyface and
  polygon meshes that way). Each VIEWPORT of `*Paper_Space` gets a VIEWPORT ENTITY HEADER (20.4.70) in its control
  (20.4.69), named `1` for the first, pointing at the next one; viewports of other layouts point at none, as in AutoCAD's
  files. LAYOUT has its class type, LTYPE its 256-byte strings area, and colors (CMC) are indexes.
- **R13 and R14** have the forms of [R13 and R14](#r13-and-r14), which the codecs write as they read them:
  - The header variables of these versions get AutoCAD's defaults (`ATTREQ`, `ATTDIA`, `DELOBJ`, `DRAGMODE` 2, `OSMODE`
    37, `COORDS` 1, `PICKSTYLE` 1, the dimension variables `DIMUNIT` 2 and `DIMFIT` 3 and empty arrow block names); the
    variables of R2000 are left out.
  - An entity with the line type BYBLOCK or CONTINUOUS points at that entry of the line type table, as these versions
    have only the BYLAYER bit. TEXT stores its alignment point as 0,0 when it is left on the baseline, as AutoCAD's R14
    files do, and its first point when it has an alignment but no second point.
  - A paper space VIEWPORT is its center and size, a VIEWPORT ENTITY HEADER, and its view in the `MVIEW` extended data
    of `ACAD`, in the order the reader reads (the order of DXF R12), with the layers frozen in it by handle.
  - LWPOLYLINE and HATCH have class types in R14 (fixed types from R2000). R13 has no LWPOLYLINE: it is written as a 2D
    POLYLINE with its vertices, widths and bulges. R13 has no HATCH: it is written with the class of R14 (`version-hatch`).
  - LAYOUT objects stay, and the entities of other layouts go into `*Paper_Space0` and so on, as AutoCAD writes them when
    it saves to R14; R13 and R14 show the `*Paper_Space` layout only (`version-layouts`).
- **SPLINE** before R2013 has scenario 2 for fit points; those versions keep no knot parameter (the reader uses chord
  length), so fit points are kept only for chord length, and control points are written otherwise.
- **MLINESTYLE** of R2018 refers to the line types of its elements by handle.
- **MULTILEADER** before R2010 holds the arrowheads of its lines in its own list (the reader takes the position as the
  line index), so arrowheads given per line are kept.
- **Multi-line attributes** are written as attributes with an embedded MTEXT in R2018, with the common values of the
  attribute.

What a version cannot hold is written as the DXF writer (`write_dxf_as`) writes it, with the same notices, and more for
DWG (the DXF writer has no R13 or R14):

| Code | Versions | What is written |
| --- | --- | --- |
| `version-lineweight` | R13, R14 | No line weights (entities BYLAYER, layers the default) |
| `version-layer-plot` | R13, R14 | Every layer plotted |
| `version-units` | R13, R14 | No `$INSUNITS` (read as a drawing without units, which zumen takes as millimeters) |
| `version-viewport-clip` | R13, R14 | A clipped viewport as its rectangle |
| `version-line-spacing` | R13, R14 | MTEXT with the default line spacing (at least, 1) (DWG only) |
| `version-layouts` | R13, R14 | Layouts other than the current one as AutoCAD 2000 and later read them; R13 and R14 show one paper space (DWG only) |
| `version-hatch` | R13 | HATCH with the class of R14, which R13 cannot show (DWG only) |
| `version-image` | R13 | Nothing: R13 has no IMAGE |
| `version-color` | R13 to 2000 | True colors as the nearest ACI (entities, layers, and the CMC colors of other objects) |
| `version-transparency` | R13 to 2000 | No transparency (entities and layers) |
| `version-gradient` | R13 to 2000 | A gradient hatch as a solid fill in its first color |
| `version-dimension` | R13 to 2000 | ARC_DIMENSION and LARGE_RADIAL_DIMENSION as INSERTs of their blocks |
| `version-background` | R13 to 2004 | No background mask of MTEXT |
| `version-mleader` | R13 to 2004 | MULTILEADER as it is (AutoCAD before 2008 shows it as a proxy) |
| `version-mleader-2010` | R13 to 2007 | A MULTILEADER without its vertical attachment, top and bottom text attachments and per-line colors (DWG only) |
| `version-image-clip` | R14 to 2007 | An image clip that removes the inside of its boundary, as one that keeps it |
| `multiline-attrib` | R13 to 2013 | A multi-line attribute as a single-line attribute (DWG only) |

R13 and R14 also leave out the dimension values of R2000 (attachment, line spacing and measurement), which the dimension
block draws, and the descriptions of blocks.

#### Verification of versions

- Every DXF and DWG of `tests/fixtures`, written in each version and read again, draws in every view as the same model
  written as DXF of that version and read again (`tests/write_model.rs`). The model is first given the values DWG of the
  version cannot hold (the multileader values of `version-mleader-2010`, multileader colors as ACI before 2004) and a
  handle for each entity without one, so that both writers number the entities alike. There is no DXF of R13 or R14;
  those drawings are compared with DXF 2000 of the model without what R13 and R14 cannot hold (line weights, layers
  that do not plot, `$INSUNITS`, viewport clipping, MTEXT line spacing). The notices of each version are checked too.
- The same holds for the 57 sample drawings with `ZUMEN_SAMPLES` in all eight versions (456 drawings), and the written
  2000 to 2018 drawings are the same bytes as before R13 and R14 were added (for the R14 samples the model changed, as
  their layouts are now read).
- For the fixtures written in every version (208 drawings), the CRCs and checksums are those the checks of
  `src/samples.rs` compute; every object read by its codec gives the same bits with nothing left over (VIEWPORT ENTITY
  HEADER and its control included); `rewrite_dwg` keeps every section; ezdwg 0.16.3 and ACadSharp 3.8.26 read every
  drawing, and ezdwg reads the same entities as from the R2013 drawing apart from the values a version cannot hold.
  `edit_dwg` edits them in every version and both read the result. ACadSharp warns that the class-based LWPOLYLINE and
  HATCH of R14 have no class definition of its own, as it does for AutoCAD's R14 files. ezdwg does not read the layer
  table of R13 (it reads it in R14 written the same way), and reads no thickness and extrusion of R14 POINTs and no
  color of R14 LWPOLYLINEs, as from AutoCAD's R14 files; the view of R13 and R14 viewports, which is extended data, is
  not read by ezdwg.
- ODA File Converter and DWG TrueView: not run yet.

## Editing (`edit_dwg`)

`zumen_dwg::edit_dwg` (through `zumen::edit`) changes a DWG drawing in place with the operations of
[decision 0012](../decisions/0012-edit-in-place-and-write-every-format.md), with the keys, values and error codes of the
DXF editor ([Editing](dxf.md#editing-edit_dxf)). It opens the drawing as `rewrite_dwg` does (R13 to R2018, R2007
included): every object that no operation changes is copied as its bits, so extended data, reactors, extension
dictionaries, objects and entities that zumen does not read stay. An object that an operation changes is read with the
lossless codec of its type (`src/codec`), changed and written again in the drawing's version, and new objects are
written in that version too. The version, the code page and the other sections stay; the drawing is written in the
container of its version. An edit without operations gives the same bytes. `new_dwg(units)` makes an empty AutoCAD 2013
drawing with `write_dwg` (`$INSUNITS` and `$MEASUREMENT` from the units), which `zumen::create` edits.

- **Handles**: new objects take handles from `$HANDSEED` (or above the largest handle), and `$HANDSEED` and the copy in
  AcDb:AuxHeader (27) become the next one. When the codec cannot read every header variable (the test drawings of
  `tests/writer` hold only those that reading uses), the bytes of the `$HANDSEED` value are written in place, which
  leaves room for new handles of the same byte length.
- **Where `add` puts an entity**: at the end of the block of its container. `model` (the default) is model space,
  `layout:<name>` the paper space block of that layout (entity mode 1 for the active paper space, otherwise the block as
  owner), `block:<name>` a block definition; block names are those of the BLOCK entities, as reading gives them (all
  paper space block records are named `*Paper_Space`). The layouts of R13 and R14 are those of `ACAD_LAYOUT`, as
  reading finds them; a drawing without LAYOUT has one paper space, which every layout name means, as in the DXF
  editor's R12. From R2004 on, the entity is added to the list of entities of its BLOCK_HEADER; in R13 to R2000 it is
  linked after the last entity of the block (previous and next entity, 20.4.1), and the block's last entity becomes it.
  A block reference is added to the list of inserts of its block.
- **What `add` writes**: LINE, CIRCLE, ARC (degrees become radians as in [Values](#values)), ELLIPSE, POINT, LWPOLYLINE
  (closed 512, constant width, elevation, bulges), TEXT, MTEXT (when the text has line breaks or `wrapWidth` is given;
  attachment bottom left, as the DXF editor writes it; from R2018 on with the redundant fields of a non-annotative
  MTEXT, as AutoCAD writes it) and INSERT, with layer, ENC color (a true color from R2004 on), line type (BYLAYER and
  BYBLOCK by the flags, others by handle; in R13 and R14 BYLAYER by its bit, BYBLOCK by the handle of its entry) and
  line weight. LWPOLYLINE has its class type in R14; when the drawing has no LWPOLYLINE class, the class is added to
  AcDb:Classes after the last one (only when the classes read back to the same bits). R13 has no LWPOLYLINE, so a
  polyline cannot be added (`unsupported-kind`, as for DXF R13). Text without a style takes `Standard`. Strings before R2007
  are in the drawing's code page, with `\U+XXXX` for characters it cannot hold and a closing NUL, as AutoCAD writes
  them. A layer that the drawing lacks is added as `add-layer` adds it (`edit-added-layer`).
- **What `update` changes**: only the values of the keys set, through the codec of the entity. Moving an aligned TEXT,
  ATTRIB or ATTDEF moves its alignment point by the same amount; `rotation` of MTEXT sets its direction; the layer of a
  POLYLINE is also set on its vertices and SEQEND; LWPOLYLINE vertices are written again with their bulges (vertex
  widths are dropped). The text of a multi-line attribute (R2018) cannot be changed (`unsupported-kind`). An entity of
  a type that the codec does not read can be deleted but not changed.
  Changing an INSERT's position, rotation or scale also transforms its ATTRIB positions, alignment points and
  glyph axes, including unequal and negative scales. Attribute values, handles and extended data stay. Tilted OCS
  and embedded MTEXT attributes refuse these placement changes (`unsupported-kind`) rather than stay at the old
  location.
- **What `delete` removes**: the entity with what follows it (vertices and SEQEND, attributes and SEQEND), its extension
  dictionary and the objects in it; the entity leaves the entity list (R2004 and later) or the chain of its block (R13 to R2000:
  the neighbours are linked to each other and the block's first or last entity moves), the draw order of the block
  (ACAD_SORTENTS), groups (GROUP) and the reactors of the objects it reacts with, and a block reference leaves the
  inserts of its block. Other references to it, such as associative dependencies, stay, as in DXF; AutoCAD's audit
  removes them. An attribute alone cannot be deleted.
- **Layers**: `add-layer` copies layer `0` without its extension dictionary, reactors and extended data (plot style and
  material stay), with color 7, `CONTINUOUS`, the default line weight and plotting on, and adds it to the layer table;
  `update-layer` sets the color, frozen (1), off (2), locked (8), plot (16) and line weight (0x3E0) of the flags, and the
  line type. An R2000 layer that is off also gets a negative color number, as DXF writes it.
- **Block definitions**: `add-block` ([decision 0017](../decisions/0017-add-annotations-and-definitions.md)) creates
  a BLOCK_HEADER (20.4.52) named `name`, with `basePoint`, no entities (R2004 and later) or null first and last
  entities (R13 to R2000), and its BLOCK and ENDBLK entities on layer `0`, and adds it to BLOCK_CONTROL. Later
  operations add entities to it with `"container": "block:<name>"` and place it with `block-reference`, as for the
  blocks the drawing has. Existing names are `already-exists`, names starting with `*` `invalid-value`.
- **Dimensions**: `kind: "dimension"` takes the keys of the DXF editor and writes DIMENSION_LINEAR (21),
  DIMENSION_ALIGNED (22), DIMENSION_ANG3PT (23), DIMENSION_RADIUS (25) or DIMENSION_DIAMETER (26) with the anonymous
  block `*D<n>` (BLOCK_HEADER named `*D`, anonymous), drawn with the same
  code as DXF (`zumen_dxf::dimension`) from the values of the DIMSTYLE object; text is MTEXT with attachment 5. The
  style is `dimensionStyle`, else `$DIMSTYLE`, else `Standard` (`missing-definition` if the drawing has none). R13 and
  R14 hold no measurement value.
- **Styles**: `add-text-style`, `add-line-type` and `add-dimension-style` take the values of the DXF editor and add a
  STYLE (with the TrueType family as `ACAD` extended data), LTYPE or DIMSTYLE (a copy of `from` with the variables
  set) to its control object.
- **Hatches**: `kind: "hatch"` takes the keys of the DXF editor and writes a HATCH (20.4.75) with polyline paths and
  the pattern lines of `zumen_core::edit::hatch` (re-exported as `zumen_dxf::hatch`); from R2000 on it is the fixed type 78, in R14 the class `HATCH` (added to
  AcDb:Classes when the drawing lacks it), and R13, which has no HATCH, refuses it (`unsupported-kind`).
- **Text placement**: `justification`, `lineSpacing` and `multiline` as in the DXF editor: TEXT gets its horizontal
  and vertical alignment and alignment point, MTEXT its attachment and line spacing factor (R13 and R14 have none,
  `edit-no-line-spacing`). `widthFactor` is the width factor of TEXT, and a `\W` code at the start of MTEXT.
- **Attributes**: `attribute-definition` and the `attributes` of `block-reference` as in the DXF editor. The ATTDEF
  sets the block header's has-attributes flag; the ATTRIBs copy their definition's values, are owned by the INSERT
  (entity mode 0), linked to each other in R13 to R2000 and listed by the INSERT from R2004 on, followed by a SEQEND.
- **Layouts and viewports** (R2000 and later): `add-layout` copies a LAYOUT object (20.4.84) of the drawing with the
  new name, tab order, paper and limits, a new BLOCK_HEADER (named `*Paper_Space`, as every paper space is, with the
  layout's handle; its BLOCK is `*Paper_Space<n>`) and the viewport of the whole sheet, the first in the block, and
  adds it to `ACAD_LAYOUT`; `margins` sets the margins of the LAYOUT as in the DXF editor. `viewport` writes a VIEWPORT (20.4.37) with the values of the DXF editor and joins the
  layout's viewport list (R2004 and later). R2000 keeps a VIEWPORT ENTITY HEADER for the viewports of the active
  paper space, so a viewport can be added there only from R2004 on; layouts added by the edit take them in R2000 too.
  New layouts use millimeter paper coordinates at 1:1, with no rotation or plot offset. Their margins default to
  zero; an explicit `margins` value overrides that default.
  `update-layout` (`name`, `set: {"paperSize": [width, height]}` in millimeters) changes an existing paper layout's
  dimensions, paper name and limits and resizes its whole-sheet viewport while preserving its handle, margins and
  other plot settings, as in the DXF editor.
- **Leaders**: `leader` writes the annotation MTEXT and a LEADER (20.4.47) that points to it, with the values of the
  DXF editor; R13 and R14 also get DIMGAP × DIMSCALE and DIMASZ × DIMSCALE in the LEADER. Reading adds the hook vertex
  that AutoCAD writes to DXF (`entities.rs`, an inferred rule).
  The hook points back toward the penultimate vertex for either annotation side. If the horizontal hook cannot fit
  inside the supplied final segment (a diagonal or short segment), `edit-leader-hook` counts the leaders whose
  paths change when read; no hook is synthesized without a text annotation.
- **Images** (R14 and later; R13 refuses them with `unsupported-kind`): `image` writes an IMAGE (20.4.80) with the
  values of the DXF editor, an IMAGEDEF (20.4.82) with `path` and the pixel counts, owned by `ACAD_IMAGE_DICT`, and an
  IMAGEDEF_REACTOR (20.4.83) owned by the IMAGE. `ACAD_IMAGE_DICT` is created in the named objects dictionary when
  the drawing has none, and the IMAGE, IMAGEDEF and IMAGEDEF_REACTOR classes are added with the editor's class
  numbers. Reading keeps the IMAGEDEF's file name (`DxfImage::path`).
  Deleting an image removes its owned IMAGEDEF_REACTOR and the reference to that reactor from IMAGEDEF, while
  retaining the image definition in the dictionary for other images that may share it.
- **Values the version cannot hold** are written as the nearest one and reported: `edit-nearest-color` (no true color
  before AutoCAD 2004) and `edit-rounded-lineweight` (the line weights of DWG). R13 and R14 have no line weights and
  plot every layer, so a line weight other than BYLAYER (entities) or the default (layers) and `plot: false` are not
  written and are reported (`edit-no-lineweight`, as the DXF editor reports it before 2000, and `edit-no-layer-plot`).
- **Not editable**: Release 12 and earlier, drawings with a password, and, in R13 to R2000, an entity whose neighbour in
  its block is of a type that the codec does not read (`unsupported-kind`, as the neighbour's links would change).

### Inferred rules of editing

- **R13 to R2000 links**: an entity with Nolinks has the previous and next handles; AutoCAD links the first entity of a block to
  a null previous entity and the last one to a null next entity, and the block header points at both. Entities are
  found by following the next entity from the block's first entity, as reading does, and the links that an edit
  changes are written as absolute soft pointers. The test drawings give paper space entities mode 2 and the first
  entity Nolinks; the editor takes a block's entities from its chain or list, not from the mode.
- **Strings**: AutoCAD's R14, R2000 and R2004 drawings end strings with a NUL that the length counts; names are compared
  without it.
- **R14 layers**: the second of the four bits is set when the layer is off (the specification calls it "on"), in the
  R14 samples as in R2000's flags.

### Verification of editing

- The conformance test of decision 0012 (`crates/zumen/tests/edit.rs`): for the two DWG fixtures and the 49 sample
  drawings (DXF and DWG, R2000 to 2018, R2007 included) with `ZUMEN_SAMPLES`, an edit without operations gives the same
  bytes, and after adding a layer, a line and a text, changing the layer of an entity and deleting another, every other
  entity reads again with the same id, type, values and shape, and no new reading notice appears.
- `src/edit/tests.rs`: every kind added in model space, paper space and a block, every key changed, deletion of block
  references with their attributes, layers added and changed, the error codes, new drawings, and 400 damaged copies
  edited without panicking. The R2000 test drawing written as R13 and R14 is edited in its version: entities, MTEXT, a
  block reference and a layer added (with the notices of what these versions cannot hold), entities changed and
  deleted, and in R14 a polyline added and changed; the LWPOLYLINE class is added to an R14 drawing that lacks it. After
  every edit, the entity lists (R2004 and later) and chains (R13 to R2000) of every block hold exactly the entities of
  the block. For the samples (the 8 R14 drawings included), the edited drawings also pass the checks of every stored
  CRC, checksum and Reed-Solomon parity (`src/samples.rs`).
- ezdwg 0.16.3 reads every edited sample with one more layer, the added line and text and without the deleted entity;
  ACadSharp 3.8.26 reads them with the same warnings as the originals. The same holds for the 8 R14 samples and for
  fixtures written as R13 and R14.
- ODA File Converter and DWG TrueView: not run yet. `cargo run --release -p zumen-dwg --example edit -- in.dwg out.dwg`
  makes the files to check.

Review constraints: R2004 and R2007 section page sizes are rejected above `MAX_SECTION_BYTES` before allocating or padding pages, including empty sections. Model writing reports `linetype-limit` and `mline-style-limit` when more than 255 dashes or MLINE style elements cannot fit.

Editing refuses a new handle when the unsigned 64-bit handle space is exhausted. Updating only LWPOLYLINE bulges or closure preserves vertex widths and IDs, including their serialization flags. Replacing the vertices resets their widths and IDs.


Legacy reader review limits: all block/entity walks together stop after one million records or 64 MiB of visited record
bytes (including repeated block addresses). Cyclic or truncated walks and malformed table records report
`dwg-broken-object`. REPEAT expansion shares a 100,000-unit budget across the drawing; each copy costs one unit plus its
vertices, attributes and one unit per 64 text bytes. Omitted copies are counted by `dwg-repeat-limit`, and application
entity/time limits also stop expansion. Generated copies have distinct ordinals beyond the original record numbers.
Polyline vertex width overrides do not change the defaults of later vertices. Legacy MVIEW frozen-layer indices resolve
to layer names, while R13/R14 EED references resolve to handles.
