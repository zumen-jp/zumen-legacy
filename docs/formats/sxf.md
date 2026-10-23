# How SXF is handled

SXF is read and written as the MLIT SXF Ver.3.1 specification (2nd edition, 2009-06) describes it: its annexes on SFC,
on features and on common predefined elements, the implementation rules (実装規約), and the NILIM requirements for display
and checking functions (表示機能及び確認機能要件書（案）, December 2025 edition). The specification is not distributed with
this repository; it can be obtained from the MLIT page listed under [References](#references).

## Reading

- **Detection** looks at the first 64 KiB, each byte read as one ISO 8859-1 character. SFC and P21 both start with
  `ISO-10303-21`; a file with `/*SXF` comments or the `feature_mode` description is SFC, any other is P21. SXF is a
  subset of STEP AP202, whose schema is `ASSOCIATIVE_DRAUGHTING`, so a file whose `FILE_SCHEMA` (ISO 10303-21, 8.2.3)
  lists another schema first is not SXF: IFC (`IFC2X3`, `IFC4`, `IFC4X3_ADD2`) and other STEP files (AP203, AP214,
  AP242) get the error for a format no reader knows instead of being read as an empty P21. The keyword and the schema
  name are compared without regard to case. The declaration is read outside strings and comments, so text in
  `FILE_NAME` or a comment cannot replace it; comments between its tokens are treated as whitespace (6.2). Spaces and an object identifier after the name
  (`{ 1 0 10303 202 1 1 1 }`) are ignored. SXF writers put `FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'))` in SFC and P21
  alike (every SXF drawing checked), but the readers do not use it, so a file without `FILE_SCHEMA` is still SXF, as
  before. P2Z and SFZ are told by the names in the archive, not by the header of the file in it.
- Strings are CP932. Characters whose second byte is 0x5C (表, ソ, 能 …) are told apart from a `\` written as `\\`.
- The codes of colors, line types, line widths, text fonts and layers are assigned in order of appearance, as the
  specification says. Line widths with predefined values have the fixed codes 1–9; only user-defined widths are numbered
  from 11.
- When writing SFC, used predefined widths also get a `width_feature` declaration (SFC annex 1-2-8 and 1-3-9).
  Jw_cad 10.3.6 on Windows omitted lines, arcs and hatch boundaries without these declarations. Adding them
  restored those strokes; user-defined width codes still start at 11.
- A composite figure definition (sfig_org) or composite curve definition takes as its members the elements written after
  the previous definition. The sheet takes the rest. Elements after the sheet are invalid in the specification, but they
  are added to the sheet with a notice so that nothing is lost.
- Sub-figures (mathematical and geodetic coordinates), drawing groups and drawing parts are placed, nested. Drawing groups
  ignore their placement values.
- The size of point markers and arrows does not depend on the scale of the sub-figure. The spacing and angle of hatch
  lines do depend on the placement.
- Splines are cubic Bézier curves; clothoids are turned into points by integrating R·L = A².

P21 is mapped onto the same features as SFC following the templates of the STEP/AP202 subset volume, and drawn the same.

- The output library writes the base point of a text shifted, for the bottom, middle and top rows, by 1/2, −1/2 and −1
  of the height; the reader shifts it back to the SXF base point. The rule was confirmed on pairs of SFC and P21 files
  of the same drawing and matches the record in
  [ezsxf](https://github.com/monozukuri-ai/ezsxf/blob/main/docs/p21-writing.md).
- For arcs, sense_agreement .T. is counterclockwise and .F. clockwise. Arcs of dimensions run counterclockwise from the
  start angle to the end angle, as in SFC.
- The direction of dimension arrows (their X axis) is turned into outward or inward relative to the dimension line.
  Leader arrows take their direction from the line.
- Layer assignments and INVISIBILITY give the visibility of layers and hide the outlines of composite curves. The
  temporary layer for sub-figures (`$$SXF_dummy_layer_for_subfigure`) is not listed as a layer.
- Strings undo the ISO 10303-21 escapes (`''`, `\X2\…\X0\`, `\X4\…\X0\`, `\X\hh`, `\S\c`, `\P?\`). Some output
  programs write 8-bit bytes, which the standard does not allow, so each byte is read as one character (ISO 8859-1) and
  bytes from 0x80 are decoded as Shift_JIS. The second byte of a double-byte character can be 0x40–0x7E (including
  `\`), so a lead byte is decoded together with the byte after it. (TextDecoder's "latin1" is windows-1252, which maps
  0x80–0x9F to other characters, so it is not used.)
- Extension lines (PROJECTION_CURVE) are ordered as extension line 1 and 2 by comparing their names (`$$SXF_prj_1`,
  `$$SXF_prj_2`) code unit by code unit, not in the user's locale, so that a drawing does not read differently from one
  locale to another.

Compressed SXF (P2Z, SFZ) is a ZIP archive that holds one P21 or SFC file. The Rust reader (`crates/zumen-sxf`)
extracts it (the TypeScript reader does not read it yet): it takes the first file of the central directory whose name
ends in .p21 or .sfc, inflates stored or deflated data and checks the CRC-32. Names are Shift_JIS unless the UTF-8 flag
is set, and the drawing information (圧縮) shows the archive format and the name of the file in it. The extracted size is
limited by the read limit (`max_bytes`), or by 2 GiB when there is no limit, against ZIP bombs (a small archive that
extracts to a huge file). Encrypted archives and ZIP64 are not supported.

The other files of the archive, such as the image files that sit next to the drawing (OCF implementation rules, 18
画像 (5)), are what `zumen::embedded_files` (and `readEmbeddedFiles` of `@zumenjs/reader`) returns, under their names in
the archive. Folders are left out, files that cannot be extracted are reported (`archive-file`), and their extracted
sizes count together against the same limit.

What is not drawn or is shown otherwise (counted in the reader's notices):

| Content | Handling |
| --- | --- |
| Patterns of pattern hatching and externally defined hatching | Only the outline is drawn |
| Predefined symbols | Their name is shown instead of the shape (実装規約 1-10) |
| Raster images (separate TIFF and JPEG files) | Drawn under every entity ([Images](#images)) |
| Area_control | A blank region; not drawn, but it can be selected |

### Images

An image (画像, a monochrome TIFF or a JPEG, called raster in SXF Ver.2.0) is a drawing group placed on the sheet. Its
name carries the file: the predefined attribute 画像 of the attribute mechanism (`$$ATRU$$<number>$$<name>$$画像$$<file>`)
from Ver.3.0, or `$$RASTER$$<file>` in Ver.2.0 (SXF 技術者リファレンスブック 2017, 3.5.4 画像). The polyline in the
group bounds the image, and the image is on that polyline's layer (OCF implementation rules, geometry, 2011, 18 画像
(3)). The image file sits next to the drawing (18 (5)); zumen does not open it.

- The image becomes an underlay of `Scene.underlays` with `file` as the name gives it, drawn over the rectangle of the
  first polyline of four points or more in the group, which stays drawn as an entity. The underlay has its id and
  layers.
- The polyline is taken to start at the image's lower left corner (*inferred*: the public drawings of Yamaguchi
  prefecture's infrastructure database, written by TOWISE CAD, go clockwise from there; the specification text was
  not at hand). The second vertex is the upper left corner when the polygon turns clockwise and the lower right corner
  otherwise, so images rotated by their placement angle are drawn rotated (18 (1)).
- SXF does not record the pixel count, so the content is measured in hundredths of a sheet millimeter
  ([decision 0031](../decisions/0031-image-units-without-a-pixel-count.md)).
- SXF draws in the order of the file (the implementation rules' display order), so an image placed after entities that
  it overlaps hides them. Underlays are drawn under every entity, and such images are reported (`raster-order`). The
  comparisons are limited as in DXF (1,000,000).
- An image group without a polyline is not drawn (`raster`). The display color and the transparency of monochrome
  rasters (18 (1)) are left to the application, which decodes the file (browsers other than Safari do not decode
  TIFF).

Broken or hostile drawings must not stop the reader, so these limits apply, with the same rules in TypeScript and Rust
(`crates/zumen-sxf`):

| Content | Handling |
| --- | --- |
| Lists of arguments or typed values nested deeper than 64 levels | The feature (SFC) or instance (P21) is left out as unreadable (`unreadable`) |
| P21 sub-figure definitions nested deeper than 64 levels | Deeper definitions are not read, with a count (`sfig-depth`) |
| Placements of composite figures nested deeper than 32 levels, or in a cycle | The placement is left out (`sfig-cycle`) |
| A clothoid longer than 256 times its parameter A | The integration (Simpson's rule) stops at 4096 divisions (a clothoid of length 10¹¹ used to hang) |
| More than 20,000 hatch lines | The pattern is left out (`hatch-dense`) |
| Layer codes above 4,294,967,295 | Treated as no layer (`layer-range`); the scene's layer ids are 32-bit |

The Rust reader (`crates/zumen-sxf`) reads with the same structure and in the same order as TypeScript and returns the
same `Scene`. This is compared on the drawings of `tests/fixtures/sfc` and `tests/fixtures/p21` (synthetic drawings with
all 34 features and the reader's notices) and on public samples, and `pnpm run fuzz` checks that broken drawings give
the same result or an error.

<a id="dxf-への変換zumen-convert"></a>

## Converting to DXF (zumen-convert)

`zumen::convert` maps an SXF drawing into the AutoCAD drawing model (`DxfDocument`) and writes AutoCAD 2013 DXF
([decision 0007](../decisions/0007-mapping-crate-between-formats.md),
[0008](../decisions/0008-sheets-and-scales-in-the-autocad-model.md)). The mapping follows the drawing rules above, and the
layout of the written DXF draws the same shapes at the same places as the SXF.

| SXF | DXF |
| --- | --- |
| Sheet (drawing_sheet) | A paper space layout with the sheet size (A0–A4 portrait or landscape, or free) and its name. The DXF opens in this layout (`$TILEMODE` 0) |
| Elements drawn on the sheet itself | Paper space (sheet millimeters) |
| A sub-figure placed on the sheet with equal horizontal and vertical scales | Its elements go to model space at real size and are shown through a viewport with the position, scale and rotation of the placement. A sub-figure placed twice gets two viewports. Viewport frames are on the layer VIEWPORTS (off, not plotted) |
| A sub-figure in geodetic coordinates | X and Y are swapped to the display orientation (X east). As in the display, the angles of arcs and the direction of texts are not swapped |
| Sub-figures with different horizontal and vertical scales or mirrored, drawing parts, drawing groups | A block (named after the composite figure, without the P21 prefix) and its placement (INSERT). Drawing groups ignore their placement values (フィーチャ仕様編 3-5, note) |
| A drawing without sub-figures | The sheet coordinates go to model space as they are, shown through a 1:1 viewport |
| Layers | Layers of the same name (unnamed ones as レイヤ n, undefined codes as 未定義のレイヤ n). Hidden layers are off. Code 0 is the DXF layer 0 |
| Colors | The predefined red, yellow, green, cyan, blue, magenta, dark gray and light gray are ACI 1–6, 8 and 9; black and white are the foreground color (ACI 7); other colors are true colors (420) with the nearest ACI |
| Line types | Predefined line types keep their SXF names (dashed, chain …, with the Japanese name as description); user-defined ones become an LTYPE of their name. Pitches are sheet millimeters (`$PSLTSCALE` 1, so they print at the same length whatever the viewport scale) |
| Line widths | The DXF lineweight (370): the predefined widths 0.13–2.0 mm as they are, other widths the nearest lineweight |
| Text fonts | A text style named after the font (TrueType; ＭＳ ゴシック and ＭＳ 明朝 with their file names). Texts without a font use Standard (ＭＳ ゴシック) |
| Texts | TEXT. The height of the text box is the TEXT height ([decision 0019](../decisions/0019-text-heights-between-formats.md): DXF draws the glyphs 1/0.88 or 1/0.72 as large), the width of a character over its height the width factor, and the left end of the baseline the insertion point (the same base point and slant). Vertical texts use the vertical font (@ＭＳ 明朝 …) rotated 270°. %% and ^ are written so that they do not become TEXT control codes |
| Lines, polylines, circles, arcs, ellipses, elliptical arcs | LINE, LWPOLYLINE, CIRCLE, ARC (clockwise arcs swap start and end angles), ELLIPSE (the longer axis is the major axis) |
| Splines | Cubic NURBS (Bézier curves with triple knots at the segment ends). The points beyond 3n+1 become an LWPOLYLINE |
| Clothoids | An LWPOLYLINE through the points the display draws (AutoCAD has no clothoid) |
| Point markers | dot is a POINT; other shapes are LINEs and LWPOLYLINEs through the points the display draws (2.5 mm × the scale on the sheet) |
| Dimensions (linear, radius, diameter, angular, arc length) | DIMENSION (rotated, radius, diameter, three-point angular; arc length is ARC_DIMENSION) with the dimension line, extension lines, arrows and value in its display block (*D). The definition points are the ends of the dimension line and the bases of the extension lines; the value is the SXF string |
| Arrows | The shapes the display draws: filled arrows and boxes as SOLID, blanked ones as a WIPEOUT filled with the background and an LWPOLYLINE outline, filled dots as a solid HATCH, line arrows as LINE and LWPOLYLINE |
| Leaders and balloons | Split into a line (LWPOLYLINE), arrow, circle and text. AutoCAD's LEADER and MULTILEADER place arrows and texts differently from SXF |
| Hatching (fill) | A solid HATCH. The boundary is the composite curve (lines and arcs as bulges; elliptical arcs, splines and clothoids as the points the display draws) |
| Hatching (user-defined) | A user-defined HATCH for each set of lines (dashed sets as a custom pattern). Spacing and angle depend on the placement (実装規約 1-7) |
| Outlines of composite curves | A closed LWPOLYLINE for those marked visible |

What cannot be mapped as it is, or is mapped in another shape, is returned as notices (kind and count).

| code | Content |
| --- | --- |
| `view-moved` | Sub-figures of different scales overlapped in model space, so the smaller one was moved (its coordinates differ from the original) |
| `sfig-insert` | A sub-figure with different horizontal and vertical scales, or mirrored (negative scale), became a block reference in paper space instead of a viewport |
| `block-scale` | A composite figure used by placements of different scales or orientations takes the arrow and marker sizes and line type pitches of its first placement |
| `text-spacing` | Character spacing cannot be written, so the width of the characters was widened to keep the text box |
| `clothoid` | A clothoid became a polyline |
| `symbol` | A predefined symbol became the text of its name, as in the display |
| `leader` | A leader or balloon was split into lines, arrows, circles and texts |
| `hatch-pattern`, `hatch-external`, `area-control` | Pattern hatching, externally defined hatching and Area_control were written as their outline only |
| `lineweight` | A width that DXF lacks became the nearest lineweight |
| `title`, `background`, `raster` | The title block, the background color and raster images were not written |
| `name` | Characters not allowed in DXF names (< > / \ " : ; ? * \| = `) became _, or a number was added to a repeated name |
| `missing-sfig`, `sfig-cycle`, `sfig-scale`, `unknown-*`, `layer-range`, `hatch-boundary`, `hatch-dense`, `degenerate` | Left out or replaced as in the display |

<a id="sxf-への書き出しdxf_to_sxfwrite_sfc"></a>

## Converting to SXF (dxf_to_sxf)

`zumen::convert(bytes, "sfc", …)` takes a drawing of any format through the AutoCAD drawing model (`DxfDocument`), maps
it into an SXF drawing (`SxfDrawing`, `zumen_convert::dxf_to_sxf`) and writes it as SFC (`zumen_sxf::write_sfc`)
([decision 0010](../decisions/0010-write-sxf-from-the-autocad-model.md)). The mapping follows the order and the rules of
the DXF display (`dxf_to_scene`), and zumen's SXF reader draws the written SFC with the same shapes at the same places as
the original drawing.

| AutoCAD drawing model | SXF |
| --- | --- |
| The layout of the space that opens (the default view of `dxf_to_scene`) | The sheet. An A0–A4 size in portrait or landscape (±0.5 mm) gets that sheet size type, anything else is free. Paper space entities are elements of the sheet (sheet millimeters) |
| Viewports | A sub-figure (mathematical coordinates) with the model space entities the viewport shows (those whose extents meet the viewport frame, without layers frozen in the viewport) at real size in millimeters, and its placement (position, scale and twist). Viewports that show the same entities place one sub-figure twice. Placements are on layer 0 (no layer) |
| A 1:1 viewport that does not move the origin | The entities go on the sheet itself (an SXF drawing without sub-figures converted to DXF) |
| Viewport frames | Polylines (frames on layers that are off or not plotted are not written) |
| A drawing that opens in model space | Model space becomes a 1:1 sub-figure on a free sheet the size of the extents, with the lower left corner of the extents at the origin |
| Block references (INSERT), tables (ACAD_TABLE) | Drawing parts (composite figure definitions with the base point at the origin) and their placements (insertion point, rotation, X and Y scale; mirroring as a negative scale). MINSERT places the part several times |
| BYBLOCK colors, line types and lineweights, and layer 0 in blocks | Written with the values inherited from the placement. Each set of inherited values gets its own drawing part (with (2) … added to the name). Blocks with dashed lines also get one for each scale on the sheet (line type pitches are sheet millimeters) |
| Layers | Layers of the same name. Layers that are off, frozen or not plotted, and DEFPOINTS, are hidden |
| Colors | ACI 7 (the foreground) and the display's white are black, colors equal to a predefined color use it, and the rest are user-defined colors |
| Line types | The dash and gap lengths the display draws (sheet millimeters). The same name and lengths as a predefined line type use it, others become a user-defined line type with the DXF name. Dots of length 0 are 0.25 mm. A user-defined line type holds 2, 4, 6 or 8 lengths (the rule ezsxf checks), so one of more than 8 keeps the first 8 |
| Lineweights | Values equal to a predefined width (0.13–2.0 mm) use its code, others become user-defined widths. The default (including the default of BYLAYER) is 0.25 mm |
| Lines, circles, arcs, ellipses, elliptical arcs | As they are (OCS mapped to WCS; mirrored arcs and elliptical arcs run clockwise) |
| Polylines | Polylines, with bulged segments as arcs. Wide polylines, meshes and curve-fit polylines become the fills and lines the display draws |
| Splines | Non-rational splines of degree 1–3 that pass through their ends are split into cubic Bézier curves (spline_feature) by knot insertion (Boehm's method); others become the polyline the display draws |
| Points | Point markers (dot) |
| Dimensions | The lines, fills and texts of their display block (not SXF dimension features) |
| Texts, attributes, MTEXT, leaders, multileaders, MLINE, fills (SOLID, TRACE), wipeouts, image frames | The lines (polylines), fills (fill_area_style_colour over invisible composite curves) and texts the display draws (text_string with the lower left corner as base point and the rotation and slant of the text box; its height is the TEXT height, so the box the display draws is shrunk by 0.88 or 0.72 about the start of the baseline, [decision 0019](../decisions/0019-text-heights-between-formats.md); vertical fonts are vertical) |
| Hatches | Solid fills are fill_area_style_colour, patterns are fill_area_style_hatching for each set of lines (spacing, angle and base in the coordinates of the container, dashes in sheet millimeters). The boundary is a composite curve with the outline the display draws, with even-odd nesting as outer and inner boundaries |
| Construction lines and rays | Line segments cut by the sheet (or the viewport frame), as in the display |

What cannot be written as it is, or is written in another shape, is returned as notices (kind and count).

| code | Content |
| --- | --- |
| `layouts` | SXF has one sheet, so layouts other than the one that opens were not written |
| `outside-viewports` | Model space entities that no viewport shows were not written |
| `viewport-clip` | Entities that cross a viewport frame cannot be clipped by a sub-figure and were written whole |
| `viewport-view` | Viewports not seen from above (3D) were not written |
| `model-sheet` | A drawing that opens in model space became a 1:1 sub-figure on a sheet the size of the extents |
| `dimension`, `drawn-shapes`, `polyline-width`, `spline`, `wipeout` | Entities and styles that SXF lacks became the lines, fills and texts the display draws |
| `text-mirror` | A mirrored text was written unmirrored in the same text box |
| `linetype-dot`, `linetype-long`, `complex-linetype`, `transparency`, `gradient` | Dots of line types became 0.25 mm dashes, line types of more than 8 dashes and gaps kept the first 8, line types with texts or shapes became dashes and gaps, transparency became opaque, gradients became solid fills |
| `hatch-dense` | Hatches without a pattern or with too many lines (more than 5,000) became their outline, as in the display |
| `image`, `xline`, `minsert` | Images became their frame, construction lines and rays became cut segments, MINSERT became several placements |
| `encoding` | Characters outside CP932 became ? (`write_sfc`) |
| `missing-block`, `block-depth`, `dimension-block`, `skewed`, `text-skew` | Undefined blocks, blocks in a cycle or nested too deep, dimensions without a display block, and skewed placements and texts were left out |

## Writing SXF files (write_sfc, write_p21, write_sxf, write_sxf_version)

`zumen_sxf::write_sfc`, `write_p21` and `write_sxf(drawing, format)` (`sfc`, `p21`, `sfz`, `p2z`) write an `SxfDrawing`.
A drawing read from a file keeps that file (`SxfDrawing::source`, the extracted file and, for P2Z and SFZ, the archive).
Written in the format it was read from, the file is **edited in place**: only what differs from the file is rewritten,
and everything else keeps its bytes. A drawing without a source (made by `dxf_to_sxf`, or with `source` set to `None`),
or written in the other format, is **written anew**. Both give an `SxfWritten` with the bytes, notices, and the pairs
(feature id in the drawing, instance number written) of the features written.

### SXF versions

SXF Ver.3.0 added the title block (`drawing_attribute_feature`), and Ver.3.1 added clothoids (`clothoid_feature`) and arc
length dimensions (`curve_dim_feature`); the other features are those of Ver.2.0. In SFC, a feature added in Ver.3.0 is
enclosed in `/*SXF3` … `SXF3*/` and one added in Ver.3.1 in `/*SXF3.1` … `SXF3.1*/`. The version of a file is the
suffix of its output library name (`zumen$$3.1`).

- Written without a version, a drawing written anew keeps its version (`SxfDrawing::version`, Ver.3.1 when unknown), or
  takes the newer version its features need, so that nothing changes. An edited file keeps its header.
- `write_sxf_version(drawing, format, SxfVersion)` chooses the version (`SxfVersion::V2_0`, `V3_0`, `V3_1`;
  `SxfVersion::parse("3.0")`). A drawing read from a file of that version is edited in place as by `write_sxf`; any other
  drawing or version is written anew, with what the version lacks written as close as it can: before Ver.3.1 clothoids
  become polylines that keep within 0.01 mm of the curve (as the scene draws them, also in geodetic sub-figures) and arc
  length dimensions become angular dimensions of the same shape (`clothoid`, `curve-dim`); before Ver.3.0 the title block
  is left out (`drawing-attribute`).

### Editing SXF (edit_sxf)

`zumen_sxf::edit_sxf(bytes, format, operations)` applies the operations of
[decision 0012](../decisions/0012-edit-in-place-and-write-every-format.md) to an SFC, P21, SFZ or P2Z file and writes it
in the same format with the in-place writers below, so only the features the operations name are rewritten, and a
compressed file goes back into its archive. All operations are checked before anything is written. `new_sxf(format)`
writes an empty drawing (an A3 landscape sheet) for `create`.

- **Ids** are the scene's: `#n` for an element of the sheet, `#P>#n` for an element seen through the placement `P`.
  Sub-figures (部分図, mathematical and geodetic coordinates) are what DXF viewports are to model space
  ([decision 0008](../decisions/0008-sheets-and-scales-in-the-autocad-model.md)), so an id through their placements
  names the element itself (a sub-figure placed twice shows the change twice). Elements of drawing parts and drawing
  groups are refused (`inside-block`): changing them changes the definition and every placement.
- **Containers** for `add`: `sheet` (the default), and `figure:<name>` or `block:<name>` (any composite figure; `block:`
  is the name of decision 0017, for the drawing parts that `add-block` adds).
- **Values** are in the drawing's own coordinates (sheet millimeters for the sheet, the sub-figure's coordinates inside
  it), the numbers that the `raw` of the properties gives; angles are degrees.

| kind | keys | SXF feature |
| --- | --- | --- |
| `line` | `start`, `end` | line_feature |
| `polyline` | `vertices`, `closed` (`bulges` only 0, no `width`) | polyline_feature; a closed one repeats its first point |
| `circle` | `center`, `radius` | circle_feature |
| `arc` | `center`, `radius`, `angle` (start and end, counterclockwise) | arc_feature |
| `ellipse`, `ellipse-arc` | `center`, `majorAxis`, `ratio`, `parameter` (radians, as in DXF) | ellipse_feature, ellipse_arc_feature (degrees) |
| `point` | `position` | point_marker_feature (dot) |
| `text` | `position`, `text` (one line), `height`, `rotation`, `textStyle` (the font of a text style added in the edit, or else a font name, added when missing), `width` (of the text box) | text_string_feature, base point at the lower left; without `width`, full-width characters are as wide as the height and half-width ones half of it |
| `block-reference` | `block` (composite figure), `position`, `rotation`, `scale` (a number or `[x, y]`) | sfig_locate_feature |
| `dimension` | `dimensionType` (`linear`, `aligned`, `angular-3-point`, `radius`, `diameter`), `start`, `end`, `position`, `rotation`, `center`, `point`, `text` (`<>` is the value, one line), `dimensionStyle` (a dimension style added in the edit; other names are reported) | linear_dim_feature, angular_dim_feature, radius_dim_feature, diameter_dim_feature |
| `hatch` | `boundaries`, `pattern`, `patternScale`, `patternAngle`, `patternLines` | composite_curve_org_feature (one invisible composite curve per boundary) with fill_area_style_colour_feature or fill_area_style_hatching_feature |

- Every kind takes `layer` (a name; a missing layer is added, `edit-added-layer`), `color` (an SXF color code, or `"#rrggbb"`:
  a predefined color when equal, else an existing or new user-defined color), `lineType` (a predefined SXF name such as
  `dashed`, or a user-defined line type of the drawing) and `lineWidth` (millimeters: a predefined width when equal, else
  a user-defined width). New elements are black, continuous and 0.25 mm. SXF has no BYLAYER or BYBLOCK, so `bylayer` and
  `byblock` become black and 0.25 mm (`edit-color-by-layer`, `edit-width-by-layer`). Other elements (dimensions, leaders,
  hatches, splines, clothoids, symbols) can change their layer and style, not their shape.
- **Drawing parts** ([decision 0025](../decisions/0025-annotations-in-jww-and-sxf-edits.md)): `add-block` takes
  `name` and `basePoint` and adds an empty composite figure definition of kind 4 (作図部品). A composite figure has no
  base point (the origin of its coordinates is placed at `sfig_locate`), so `basePoint` is subtracted from the
  coordinates of what is added with `"container": "block:<name>"` in the same edit, and `block-reference` places the
  part with its base point at `position`. A name the drawing has is `already-exists`; empty names, control characters
  and names starting with `$$` (the attribute mechanism and the P21 prefixes) are `invalid-value`. A placement that
  would put a composite figure inside itself, directly or through others, is `invalid-value`. `created` gives the name.
- **Dimensions** are the dimension features with the style of the operation (`layer`, `color`, `lineType`,
  `lineWidth`). SXF has no dimension styles, so the sizes are those of AutoCAD's metric dimensions (ISO-25), in sheet
  millimeters: extension lines start 0.625 beyond their origin and pass the dimension line by 1.25, the arrows are
  filled arrows (code 6, outward) 2.5 long (scale 1/3), and the value is 2.5 high, anchored at its bottom center
  (anchor 2) 0.625 above the middle of the dimension line or arc, turned to read from the left or from below, with the
  box width of `text` (full-width characters as wide as the height). Inside a composite figure the lengths are divided
  by the scale of its first placement found from the sheet (nested placements multiply; drawing groups and figures
  without a placement count as 1), so that they have these sizes on the sheet; arrows keep their scale, since SXF
  draws arrows at sheet size. Values are rounded to whole numbers, with `R` before radius values, `φ` before diameter
  values and `°` after angles. A linear or aligned dimension writes the dimension line between the feet of its
  extension lines, the extension lines (none for an origin on the dimension line) and an arrow at each end; an angular
  dimension the arc from `angle0` to `angle1` counterclockwise, extension lines only from origins inside the arc, as
  in DXF, and arrows at its ends; a radius dimension the line from the center to `point` with an arrow at `point`; a
  diameter dimension the line across the circle with an arrow at each end. `dimensionStyle` names a style added
  earlier in the same edit (see Styles below); other names are reported (`edit-dimension-style`) and drawn with these
  sizes.
- **Styles** ([decision 0026](../decisions/0026-styles-in-jww-and-sxf-edits.md)): SXF has no dimension styles, and its
  fonts and user-defined line types hold only a name and lengths, so `add-dimension-style` and `add-text-style` keep
  their names for the edit only, and later operations of the same edit use them. Names are compared without case; a
  name added twice is `already-exists`, an empty one or one with control characters `invalid-value`, and `created`
  gives the name.
  - `add-dimension-style` takes the keys of decision 0017 and starts from `from`, a dimension style added earlier in
    the edit, or else from the sizes above (another name in `from`, such as `Standard`, is reported with
    `edit-dimension-style`). Its lengths are sheet millimeters, divided by the scale of the figure's placement as
    above, or, with `scale` (above 0), the lengths times `scale` are coordinates of the figure the dimension is added
    to, as DIMSCALE in DXF: a style with `scale` 100 draws in a 1/100 sub-figure what one without it draws everywhere.
    `textHeight` (above 0) is the height of the value, `textGap` the gap between the value and the line,
    `extensionOffset` and `extensionExtension` where the extension lines start and how far they pass the line, and
    `arrowSize` the length of the filled arrows (scale `arrowSize` / 7.5 on the sheet; 0 draws no arrows). `tickSize`
    above 0 writes slashes (code 10, 2.5 mm long at scale 1) instead, with scale `tickSize` × √2 / 2.5, the length of
    DXF's oblique stroke.
    `textAbove: false` puts the value centered on the dimension line, which a dimension feature cannot break
    (`edit-dimension-text-position`). Values have `decimals` (0–8) and lose trailing zeros unless
    `suppressTrailingZeros` is `false`; lengths, radii and diameters are multiplied by the absolute value of
    `linearFactor`, and angles stay whole degrees. `textStyle` gives the value's font, as `textStyle` of `text` does.
  - `add-text-style` adds the font `font` (the name when it is not given) to the font table unless the drawing has it;
    the name stands for that font in `textStyle` of `text` and of dimension styles. A name that is a font of the
    drawing is `already-exists`. `bigFont`, a `height` other than 0, a `widthFactor` other than 1 and an `oblique`
    other than 0 are reported (`edit-style-values`).
  - `add-line-type` adds a user-defined line type (code 17–32) with the name and `pattern` in sheet millimeters (dashes
    positive, gaps negative, dots 0): the lengths start with a dash and alternate, dots become 0.25 mm dashes with the
    following gap shorter by as much, a leading gap moves to the end, and more than 8 lengths are cut, as SXF holds 2,
    4, 6 or 8 (`edit-line-type-pattern`). A pattern without gaps is a continuous line: the name stands for line type 1
    in the edit and nothing is written. Predefined names and names of the drawing's line types are `already-exists`;
    when the 16 user-defined line types are taken, `name-exhausted`. `description` is reported (`edit-style-values`).
- **Hatches**: each boundary becomes an invisible composite curve (a closed polyline; the first is the outline, the
  others holes) added after the composite curves of the drawing. `SOLID` is fill_area_style_colour in the operation's
  color (`lineType` and `lineWidth` are reported as unused, `edit-unused-attributes`). A pattern is
  fill_area_style_hatching with one hatching line for each line of the pattern, in the operation's color, line type and
  width: its base point moved along the normal by whole spacings (and along the line, by whole dash periods) to near
  the first boundary point, its spacing (the part of the pattern offset across the lines) and its angle. Dashes become
  a user-defined line type in sheet millimeters (lengths in the figure × the scale of its placement; dots as 0.25 mm
  dashes, the gap after them shorter by as much, as zumen-convert writes them), named after the pattern, or the
  predefined or user-defined line type with the same lengths. A user-defined line type holds 2, 4, 6 or 8 lengths, so
  more than 8 that are not the lengths of a predefined line type (double-dashed triplicate-dotted has 10) are cut to
  the first 8, and the base point moves by whole periods of those (`edit-hatch-dash-count`); when the 16 user-defined
  line types are taken, the line is solid (`edit-hatch-dashes`). The shift of the dashes from one line to the next (the
  offset along the lines) cannot be written, and is reported when it is not a whole number of dash periods
  (`edit-hatch-shift`).
- Layers take `visible`; SXF layers have no color, line type or width, so those values are checked and left unused
  (`edit-layer-attributes`). A layer without elements cannot be written to P21.
- The ids of added elements are their written instance numbers (`#n`); the ids of added layers are their names.
- Notices of the writers below come with `edit-` before their code (`edit-rewrite`, `edit-encoding`), as the notices of
  the editor itself do.

### Editing SFC in place

The reader's rules (附属書 SFC 編 1-3) decide which definition each record of the file belongs to: a composite curve
definition, a composite figure definition and the sheet take the records written after the previous definition. The
drawing and the file are matched for each definition (the tables, each composite curve, each composite figure, the
sheet) by instance number, from the front.

- A feature whose values did not change keeps its record (`/*SXF … SXF*/`) byte for byte. A changed feature is rewritten
  in its place with the same number (and the same `/*SXF3.1`-style marker, or a newer one when the feature needs it).
- A feature missing from the drawing is removed with the white space after it. Removing a composite curve or a composite
  figure removes its members.
- A new feature goes among the members of its definition: before the next member that stays, else after the previous
  one (which can be after the sheet), else before the definition. Its number is its own when the file does not use it,
  else the next multiple of 10 above the largest number in the file. Numbers of removed records are not reused.
  If that would exceed the exact-integer range of the shared drawing model, unused multiples of 10 are taken
  from the start of that range, skipping all existing numbers.
- A new composite curve or composite figure is inserted with its members right after an existing definition, where the
  reader starts collecting members again. Composite curves keep their numbers (order of appearance): a new one goes
  after the composite curve before it in the drawing.
- New layers, colors, line types, widths and fonts go after the last record of their kind (codes are in order of
  appearance). An edit also declares missing predefined widths used by the resulting drawing; an unchanged file
  retains its original bytes.
- Records the reader cannot use stay as they are: unreadable and unknown features, declarations of predefined colors,
  line types and widths, a second sheet, definitions replaced by a later one of the same name, and members that a
  composite curve cannot hold.
- New records take the line ends of the file and the white space found between its first two records. Numbers are
  written with the fewest digits that read back as the same value.

### Editing P21 in place

The reader remembers where each feature came from: the item of the sheet or sub-figure (the `items` of
DRAWING_SHEET_REVISION and DRAUGHTING_SUBFIGURE_REPRESENTATION) that made it, and the segments of each composite curve.
Features are matched by instance number within each list.

- A changed feature keeps the number of its item (the occurrence, or the callout of a dimension or leader) and gets new
  geometry and style instances. Relations that referred to the old item (DIMENSION_CALLOUT_RELATIONSHIP) are rebuilt. An
  item that wraps another one (a STYLED_ITEM around a callout) is removed and made anew.
- A feature missing from the drawing leaves its list; a new feature is made with new numbers and joins its list at its
  place, and its layer.
- Then every instance that nothing reaches any more is removed: references are followed from the instances that nothing
  referred to in the file (relations, layer usage, the title …) and from the new instances. Layer assignments
  (PRESENTATION_LAYER_ASSIGNMENT) and INVISIBILITY are weak references: they keep nothing alive, and removed instances
  are taken out of them. A layer assignment left without items is removed with its PRESENTATION_LAYER_USAGE
  (assigned_items must hold one item or more), and references to instances that never existed in the file are kept.
- A changed layer name or visibility, sheet size or name (DRAWING_SHEET_REVISION, PLANAR_BOX and DRAUGHTING_TITLE), and
  changed values of colors, line types, widths and fonts rewrite only those instances. New layers, colors and so on are
  new instances with numbers above the old ones, because the reader assigns codes in order of instance number.
  A changed composite curve is rebuilt, keeping the numbers of its COMPOSITE_CURVE and of its occurrence.
- New instances are appended at the end of the DATA section, numbered from the next multiple of 10 above the largest
  number in the section, with the line ends of the file. If that range is not exactly representable by the shared
  drawing model, numbering restarts at 10 while skipping every number already referenced in the source. Rewritten
  instances are written on one line.
- The P21 reader does not read the title block, so a title set on a drawing read from P21 is not written (`title`).
- A P21 with repeated instance numbers or without a sheet cannot be matched reliably and is written anew (`rewrite`), as
  is a file that the read limits stopped from being read again.

### Writing P21 anew

The instances of each feature follow the templates of the STEP/AP202 subset volume (chapter 2) and its P21 examples
(chapter 3). Where the specification leaves a choice, the writer does what the SXF output library does in paired SFC
and P21 files of the same drawing:

- Names are `' '`. The header is `FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1')` with the output library
  `zumen$$3.1` (the SXF version is the suffix, see [SXF versions](#sxf-versions)) and FILE_SCHEMA
  `ASSOCIATIVE_DRAUGHTING`. Instances are numbered from 10
  in steps of 10, in this order: units, colors, line types and fonts, composite curves, composite figures (placed ones
  first), the elements of the sheet, the title and the sheet, layers, INVISIBILITY.
- Reals are written with the fewest digits that read back as the same value, always with a decimal point (`1.0`,
  `1.5E-7`). Non-ASCII strings use `\X2\…\X0\` (or `\X4\…\X0\` outside the basic plane); control characters are left out.
- Each feature has its own points, directions and placements. Only colors, line types, widths, fonts and the styles made
  of them (CURVE_STYLE in a PRESENTATION_STYLE_ASSIGNMENT, symbol styles, NULL_STYLE) are shared, so that editing can
  tell which instances belong to one feature.
- The predefined colors with AP202 names (black, red, green, blue, yellow, magenta, cyan, white) are
  DRAUGHTING_PRE_DEFINED_COLOUR; the other SXF predefined colors are COLOUR_RGB named `$$SXF_<name>` with quarter
  fractions (192 as 0.75). User-defined colors, line types and fonts are written in table order, used or not, so that
  their codes stay. Widths are LENGTH_MEASURE_WITH_UNIT, one per value (the reader numbers user widths in order of use).
- Lines are a TRIMMED_CURVE of a LINE whose DIRECTION is the vector from start to end, trimmed by their end points.
  Arcs and elliptical arcs are trimmed by parameters in radians, with sense .T. for counterclockwise. Splines are one
  BEZIER_CURVE of degree 3 with all control points (3n+1 points are n cubic segments). Clothoids are a CLOTHOID named
  `0` (counterclockwise) or `1` (clockwise), trimmed by curve length.
- Texts are TEXT_LITERAL_WITH_EXTENT named `$$SXF_<row> <column>` with alignment `baseline <column>`, the base point
  shifted by 1/2, −1/2 or −1 of the height for the bottom, middle and top rows (the reader's rule), a PLANAR_EXTENT of the
  text box, and TEXT_STYLE_WITH_BOX_CHARACTERISTICS with BOX_WIDTH 0.8 times the height and the rotation also in
  BOX_ROTATE_ANGLE. Texts without a font refer to no font (`$`).
- Dimensions are a DRAUGHTING_CALLOUT of the dimension curve, the extension lines (`$$SXF_prj_1`, `$$SXF_prj_2`, a LINE
  through their base point trimmed by their ends), the value (`dimension value`) and the terminators (`$$SXF_arw_1`,
  `$$SXF_arw_2`, ORIGIN and TARGET), with a STRUCTURED_DIMENSION_CALLOUT and DIMENSION_CALLOUT_RELATIONSHIP when there is a
  value. A terminator's X axis points outward from the nearer end of the dimension line for arrow direction 1, inward
  for 2. Leaders are a LEADER_DIRECTED_CALLOUT of the line, the balloon circle, the text and a LEADER_TERMINATOR at the
  first point, pointing from the second point. Only the dimension or leader curve and its text are assigned to layers.
- Fills are an ANNOTATION_FILL_AREA of the outer and inner COMPOSITE_CURVEs with FILL_AREA_STYLE_COLOUR,
  EXTERNALLY_DEFINED_HATCH_STYLE, one FILL_AREA_STYLE_HATCHING for each pattern (its CURVE_STYLE, a repeat VECTOR at the
  angle + 90° as long as the spacing, and its start point), or FILL_AREA_STYLE_TILES. Composite curves have an
  occurrence with their style that is in no list, and in INVISIBILITY when hidden; full circles and ellipses in them are
  trimmed from 0 to 2π (segments must be bounded curves).
- Composite figures are a DRAUGHTING_SUBFIGURE_REPRESENTATION named with `$$SXF_FM_`, `$$SXF_FG_`, `$$SXF_G_` or
  `$$SXF_P_`, with their own units and context and the origin placement as the last item, and a
  SYMBOL_REPRESENTATION_MAP. Placements are a MAPPED_ITEM in an ANNOTATION_SUBFIGURE_OCCURRENCE; those without a layer are
  assigned to `$$SXF_dummy_layer_for_subfigure`.
- The sheet is a DRAWING_SHEET_REVISION named `A<n>_horizontal`, `A<n>_vertical` or `FREE`, with the PLANAR_BOX as its
  last item, a PRESENTATION_SIZE, and a DRAUGHTING_TITLE that holds the drawing name of the title block or else the
  sheet name (the reader takes the sheet name from it). A title block adds the contract, approval date and
  organizations of the SFC title block. A drawing without a sheet is put on a free sheet of size 0 (`no-sheet`).
- Every layer with items gets a PRESENTATION_LAYER_ASSIGNMENT and PRESENTATION_LAYER_USAGE; layers without items cannot
  be written (`empty-layer`), and undefined codes are left unassigned.

### Writing SFC anew

- The header is `FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1')` with the output library `zumen$$3.1` (or the
  version written).
- Each feature is enclosed in `/*SXF` … `SXF*/`; the title block, added in Ver.3.0, in `/*SXF3` … `SXF3*/`, and clothoids
  and arc length dimensions, added in Ver.3.1, in `/*SXF3.1` … `SXF3.1*/`.
  Instance numbers run from 10 in steps of 10 in writing order, and long lines break between arguments at about 80
  columns, as public SFC files do.
- The order is: declarations of the predefined colors and line types used, user-defined colors, line types and widths,
  text fonts, layers, composite curves (members before the definition), composite figures (members before the
  definition, placed figures first), the elements of the sheet, the title block, the sheet. Composite curve and composite
  figure definitions and the sheet take the elements after the previous definition (1-3), so in this order members do
  not mix.
- Strings are CP932; characters outside it become ? (`encoding`), and line breaks and control characters are left out.
  `\` is written as `\\`. Numbers are written with the fewest digits that read back as the same value.

### Compressed SXF

`write_sxf(drawing, "sfz")` and `"p2z"` write the SFC or P21 and put it in a ZIP archive. A drawing read from a
compressed file and written in the same format goes back into the same archive: only the SXF file is replaced (deflated,
with its name and date), the other files of the archive (attribute files, images) are copied byte for byte, and an
unchanged drawing gives the original archive. Other drawings get a new archive with one deflated file named after the
drawing (`.sfc` or `.p21`, upper case when the name was), dated 1980-01-01 so that the same drawing gives the same bytes.
Names outside ASCII are Shift_JIS, or UTF-8 with the UTF-8 flag when Shift_JIS lacks a character.

### Notices of the writers

| code | Content |
| --- | --- |
| `unknown-code` | Undefined color, line type, width, font, layer, marker or arrow codes were written with the value the display uses (white, continuous, 0.13 mm, no font, no layer, a dot, no arrow) |
| `not-written`, `not-drawable` | Elements that refer to an undefined composite figure or composite curve, and definitions found among the elements, were not written |
| `spline-points` | A spline whose control points are not 3n+1 was written as one BEZIER_CURVE with those points |
| `empty-layer` | A layer without items was not written to P21, or its assignment was removed by an edit |
| `no-sheet` | A drawing without a sheet was put on a free sheet of size 0 in P21 |
| `title` | A title block set on a drawing read from P21 was not written |
| `rewrite` | The original file could not be edited in place and was written anew (the reason is in the message) |
| `archive` | The original archive could not be read again, so a new archive holds only the SXF file |
| `sxf-limit` | The drawing exceeds an SXF limit (256 layers, color codes up to 256, line type codes up to 32, width codes up to 16, 1,024 fonts, strings of 256 CP932 bytes, the values ezsxf checks); it is written as it is, and SXF programs may refuse it |
| `clothoid`, `curve-dim`, `drawing-attribute` | A version chosen before Ver.3.1 got polylines for clothoids and angular dimensions for arc length dimensions; one before Ver.3.0 has no title block |
| `encoding`, `invalid-number` | Characters outside CP932 became ? (SFC), and values that are not numbers became 0 |

### Verification

- `crates/zumen-sxf/tests`: every SFC and P21 fixture written anew as P21 and as SFC reads back as the same scene
  (primitives and boxes within 1e-9, ids through the written numbers); written unchanged, SFC, P21, SFZ and P2Z give the
  same bytes; edits (moving, deleting and adding features, sub-figures, composite curves and layers, renaming layers,
  resizing the sheet, changing colors) change exactly those records and instances, remove the instances only removed
  features used, and read back as the edited drawing; a P2Z keeps its other files.
- `crates/zumen-sxf/tests/versions.rs`: a drawing with a title block, clothoids and arc length dimensions written as
  Ver.2.0 and Ver.3.0 (SFC, P21, SFZ, P2Z) has the version's header and markers and none of the newer features, and its
  scene has the same shapes (clothoids within 0.05 mm); written as its own version it keeps its bytes.
- The local samples (`ZUMEN_SAMPLES=… cargo test -p zumen-sxf --release -- --ignored`), a 1,201-entity drawing as SFC
  and as P21: written anew as P21 they read back the same; written unchanged they give the same bytes; moving a line,
  deleting a dimension and adding a line in place reads back as the edited drawing (35 ms for the 1.4 MB P21).
- `crates/zumen-sxf/tests/edit_operations.rs` and `crates/zumen/tests/edit.rs`: a drawing part with its elements and a
  placement, the five kinds of dimension, a fill and two hatchings added to every SFC, P21, SFZ and P2Z fixture read
  back drawn, with the other elements unchanged; a dimension added in a 1/20 sub-figure has a 2.5 mm value on the
  sheet. The same operations on the 1,201-entity sample draw the same in SFC and P21. Dimension styles change the
  value's height, font, decimals and trailing zeros, the arrows (slashes for `tickSize`) and the extension lines, on
  the sheet and with `scale` in a sub-figure; a text style and a line type added to every fixture are used by a text
  and a line read back drawn. A hatch whose pattern line has 10 dash and gap lengths writes a user-defined line type
  of the first 8, with its base point moved by whole periods of those 8, and one with the 10 lengths of
  double-dashed triplicate-dotted keeps that predefined line type; in SFC, P21, SFZ and P2Z both read back drawn with
  those dashes.
- Checking the written files with the SXF browser's checking function (SXF確認機能) is to come on a Windows host.

## References

- [MLIT SXF Ver.3.1 specification and the requirements for SXF display and checking functions](https://www.cals-ed.go.jp/sxf_ver3-1_specification_draft/)
- [NILIM, CAD データの作成](https://www.nilim.go.jp/lab/bcg/siryou/tnn/tnn0403pdf/ks0403012.pdf)
- [Utsunomiya City, 電子納品運用に関するガイドライン〔営繕編〕更新概要](https://www.city.utsunomiya.lg.jp/_res/projects/default_project/_page_/001/013/197/egaiyou4-1.pdf):
  what each SXF version added (Ver.3.0 the title block and attributes, Ver.3.1 clothoids and arc length dimensions)
- [ezsxf](https://github.com/monozukuri-ai/ezsxf) (MIT): a Python and Rust library that reads SXF and writes SFC and P21

Explicit SXF version selection checks feature requirements even when the source header already names the requested version. Changing the version within SFZ/P2Z replaces only the drawing member and preserves other archive members and the drawing path.
