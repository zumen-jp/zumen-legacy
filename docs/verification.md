# Verification records

Sample drawings are not part of this repository, because their terms of use do not allow redistribution or have not been confirmed.

## SXF and JWW

Checked on 2026-10-18 on macOS (Apple Silicon), Node.js 24.21.0 and Chromium.

- 16 unit tests. SFC: the CP932 second byte 0x5C, the order in which codes are assigned, sub-figure scales and real-world
  measurement, drawing groups, geodetic coordinate systems, user-defined colors, line types and widths, and features after the sheet.
  P21: `\X2\` strings, layers and hidden layers, child figure scales, clockwise arcs, the text anchor offset, outward and inward
  arrows, and fills with a hidden outline. JWW: coordinates centered on the sheet and a 1/100 real-world scale, layer names and states,
  line type bit patterns, clockwise arcs, class numbers shared across block definitions, version 600 and 700 strings (CP932 and UTF-16),
  and settings stored as text.
- The real-project sample `D0LS004Z.SFC` from the public [ezsxf](https://github.com/monozukuri-ai/ezsxf) repository
  (255 KB, written by ABC-CAD 2008, SXF 3.0, A1) became 1,201 scene entities in 168 ms in Node.js. The feature counts
  (581 lines, 313 texts, 126 linear dimensions and so on) match the original, and Japanese notes and dimension values are decoded.
- The P21 version `D0LS004Z.P21` (1.4 MB, 18,150 instances) is read in 0.2–0.4 s into the same 1,201 entities as the SFC version.
  Types, layers, notice counts and the overall extents (to 0.01 mm) match.
- In the playground: fit, zoom, hatch clipping, dimension arrows and values, vertical text, selection and properties,
  real-world measurement in a 1/20 sub-figure, the light background, and no horizontal scroll at 375 px.
- Eight JWW samples (version 600: site plan, shadow diagram, wooden house plan, apartment plan and others) from the ezjww repository
  are read in 1–50 ms each. Counts by type, coordinate ranges, scales and layer group names match ezjww 0.7.2.
  On the apartment plan (1,754 entities): room names, half-width katakana, rotated text, wall solids, dimensions and real coordinates.
- Nine version 700 samples saved with Jw_cad (including nested blocks and shared class numbers) have the same entity and block
  definition counts as ezjww.
- Before and after the split into packages, the scenes of the 20 samples above (properties, coordinates and notices) were identical.
  In the playground, 15 interaction steps on the SFC sample and the apartment plan (wheel, drag, keys, selection, measurement,
  background, line weights, layers and width changes) gave the same state and the same Canvas pixels. `DrawingViewer` without React
  was checked with the built packages loaded through an import map: display, coordinate conversion, overlays and cleanup.

## DXF

Checked on 2026-10-18 on macOS (Apple M2 Pro), Node.js 24.21.0 and Chromium, against [ezdxf](https://ezdxf.mozman.at/) 1.4.5 (Python 3.14).

- Eight unit tests in `packages/dxf/tests`: `$DWGCODEPAGE` `ANSI_932` with CP932 second bytes 0x5C in TEXT and MTEXT and a `\U+` escape;
  Shift_JIS detected for an R12 file without a code page while a Western `°` stays Windows-1252; binary DXF value sizes;
  nested blocks with base points, scales and rotation, meters converted to millimeters and real-world measurement;
  layer 0 and `BYBLOCK` inheritance; a 1:100 layout viewport whose measurements return model millimeters, with the overall
  viewport ignored; a MULTILEADER whose context, leader and common sections reuse group codes 10, 41 and 42 (the leader starts at the
  back of the arrow and the dogleg follows the leader's own length); and an MLINE with a mitered corner and an MLEDIT cut, drawn at the
  same positions from the MLINESTYLE when group 41 is removed, as for DWG. Replacing the reading of the cut with dash lengths, or taking
  the arrow size from the common section, makes them fail.
- Compared with ezdxf: counts by type per space and per block, layer and layout names, the content and insertion point of model space
  TEXT and MTEXT (to 0.001), LINE end points (to 1e-6), model space extents and viewport scales.

  | Sample | Source and terms | Version | Entities | Result |
  | --- | --- | --- | ---: | --- |
  | Yokosuka City survey plan (Minami kindergarten site) | [City of Yokosuka](https://www.city.yokosuka.kanagawa.jp/2645/minamikodomoen-proposaljisshi.html); no reproduction or modification, read locally | AC1032 | 9,260 | Counts, 1,239 texts (356 Japanese) and 4,814 lines match (two differ only in the rounding of the sixth decimal); extents equal |
  | Yokosuka City profile and cross sections | as above | AC1032 | 692 and 1,191 | Counts, texts, lines and extents match |
  | Imizu City cultural hall seating section | [Imizu City Culture Promotion Foundation](https://www.imizubunka.or.jp/chubun/about/download/); redistribution not confirmed, read locally | AC1009, no code page | 638 | Shift_JIS detected; 26 texts (22 Japanese) match ezdxf read with `cp932` |
  | ACadSharp samples (14) | [ACadSharp](https://github.com/DomCR/ACadSharp) (MIT), commit 2f8b099; one drawing saved as R12 to 2018, ASCII and binary | AC1009–AC1032 | 341 (2,396 in R12) | Counts in blocks, texts and extents match; ASCII and binary give the same result; model space differs only by types not read (3 TOLERANCE, 2 3DSOLID, 2 MESH…) |
  | BFP Teaching House | Big Floor Plans (CC BY 4.0) | AC1015, inches | 376 | Counts and 49 texts match; extents differ by 0.26% (text widths) |
  | ezdwg DXF samples (25) | [ezdwg](https://github.com/monozukuri-ai/ezdwg) (MIT) `test_dwg` | AC1015–AC1027 | 1 each | Counts and texts match |
  | Generated drawings (3) | Made with ezdxf: four dimension types, hatch patterns, formatted MTEXT, attributes, arrays, nested blocks, OCS, a 1:100 viewport and a 1:50 viewport twisted by 30°, R2000 in `ANSI_932`, binary | AC1015, AC1032 | 76 | Counts and texts match; points inside both viewports equal ezdxf's `Viewport.get_transformation_matrix` |

  The remaining differences: ezdxf adds layers `0` and `Defpoints` and a `Layout1` while loading; ezdxf reads the tolerance stack
  `\S+0.1^-0.2;` as caret notation, while zumen splits it into upper and lower parts as AutoCAD does; and text widths change the extents.
- MULTILEADER and MLINE: the drawn lines and fills were compared point by point with the graphics AutoCAD stored in each MULTILEADER
  (proxy graphics, read with ezdxf's `ProxyGraphic`) and with ezdxf's `render/mleader.py` and `render/mline.py`.

  | Sample | Entities | Result |
  | --- | --- | --- |
  | ACadSharp samples, R2000 to 2018, ASCII and binary (12) | 15 MULTILEADER, 3 MLINE each | Read the same as ezdxf in every version (content, leaders, colors, arrows, MLINESTYLE). Leader lines, doglegs and arrowheads of all 15 equal AutoCAD's graphics (within 1e-12) in all 12 files. Text frames differ only at the bottom edge by 0.51 (the text descent), and the underline of connection 5 is 10 shorter with the substitute font. The 3 MULTILEADER with vertical text flow (172 = 3), which AutoCAD lays out in one row, are drawn as horizontal lines and reported. The R2000 files, where AutoCAD replaced the top/bottom connection by a zero-length dogleg, draw the same. MLINE: equal to ezdxf except the open tee made by MLEDIT, where zumen leaves the gap in the crossed line and starts the other one at it, while ezdxf draws both through |
  | Generated with ezdxf, R2000 and R2018 | 9 MULTILEADER, 6 MLINE each | MULTILEADER with three leaders on two sides, underline and over/under connections, a spline leader, dot, open, architectural tick, oblique and box arrows, 30° content with overall scale 2, block content with an attribute, no leader, one inside a rotated block, and a background fill; MLINE with 2, 3, 4 and 6 elements, fill, miters, square and round caps, inner arcs, top, zero and bottom justification, a closed one, suppressed start caps, and one inside a rotated block. Lines equal ezdxf's within 0.02 (arc flattening), except where ezdxf differs on purpose or by mistake: underline and overline lengths (text widths, and ezdxf measures R2000 text before decoding `\U+`), the spline (within 0.65), the suppressed start caps and the second pair of inner arcs, which ezdxf draws and leaves out respectively, and leader colors, which ezdxf draws white when the line color is BYBLOCK (`is not` on integers in `leader_line_attribs`) |
- Time (median of five runs after a warm-up, Node.js): the Yokosuka survey plan (2.7 MB, 9,260 entities) parses in 47 ms and converts in
  56 ms; the ACadSharp 2018 sample parses in 14 ms as ASCII (2.5 MB) and 22 ms as binary (1.3 MB). With MULTILEADER and MLINE added,
  the same runs took 54 and 64 ms, and 16 and 19 ms, within the variation between runs. The built package is 232 KB (57 KB gzip;
  184 KB and 45 KB before MULTILEADER and MLINE).
- In the playground (built-in browser of the Claude desktop app, Chromium): model space of the Yokosuka plan with Japanese layer names
  and text, 50% transparent hatches showing the lines below, selection of a TEXT (handle, layer, style `MS Gothic`), and a model space
  measurement; switching an ACadSharp drawing to the layout `MyLayout` (A4 sheet, light background, viewport 1:1.4352) and measuring
  inside the viewport (57.9 mm on paper, 83.1 mm real); the generated drawing compared with ezdxf's own rendering (matplotlib);
  the Imizu section in Shift_JIS; the floor plan of the teaching house; and both side panels closed. The MULTILEADER and MLINE samples
  were viewed next to ezdxf's rendering and AutoCAD's graphics: colored leaders with dot, oblique and architectural tick arrows, the
  dashed spline leader, framed and rotated text, the block content with its attribute, the yellow text background, filled multilines
  with round, square and inner arc caps, and the open tee of the ACadSharp sample.
- The SXF and JWW unit tests still pass after the changes to `@zumenjs/core` (views, colors with alpha, near-black inversion) and
  `@zumenjs/react` (view selection, grid columns).
- Not checked: client drawings, DXF files over 10 MB, external references, dynamic blocks, AutoCAD's own rendering of text widths and
  ACI colors, Firefox and Safari. For MULTILEADER and MLINE: AutoCAD's own rendering of spline leaders, block content and the arrow
  blocks (the ACadSharp samples only have straight leaders with closed filled arrows and text), tolerance content, text columns,
  non-rectangular text frames, dimension breaks, MLINE fill breaks, and MLEDIT cuts in elements that start after the miter point.

## DXF against the DWG of the same drawing

Checked on 2026-10-20 on macOS (Apple M2 Pro) with Node.js 24.21.0 and Rust 1.99.0. Comparing DWG with the DXF of
the same drawing (`crates/zumen-dwg/examples/compare.rs` of [zumenjs/zumen#11](https://github.com/zumenjs/zumen/pull/11),
with this change merged locally) found three values that the DXF reader read differently from AutoCAD. Both readers now
read them as [How DXF is handled](formats/dxf.md#reading) describes.

- Three synthetic drawings in `tests/fixtures/dxf` and three unit tests pin the rules: a multi-line ATTDEF as AutoCAD
  2018 writes it (attribute type 4 in `AcDbAttributeDefinition`, text in the embedded MTEXT) is drawn upright, and an
  ATTRIB whose `AcDbText` 71 is 2 is still mirrored (`attributes-r2018`); binary MTEXT with a line feed and a tab, MTEXT
  `x^2` and TEXT `m^2` read as the ASCII DXF of the same strings and draw two lines, `x^2` and `m^2` (`binary-text`); and
  in a metric drawing whose `$DIMASZ` is 2.5, a leader whose dimension style leaves DIMASZ out gets an arrow of 0.18
  (`dimstyle-defaults`). Undoing any of the four changes (the subclass of 71, the caret notation, `^ ` in TEXT, the 0.18)
  in TypeScript or in Rust makes them fail.
- The ACadSharp samples, one drawing saved by AutoCAD as DWG 2000 to 2018 and as ASCII and binary DXF:
  - The dimension styles of DXF and DWG are now equal in every version; DIMASZ and DIMTXT of four of the six styles were
    0 in DXF.
  - The R2018 multi-line ATTRIB and ATTDEF have the generation flags of DWG (0; DXF had 2 and 4). They still differ in
    their attribute flags (70: 0 in DXF, 28 and 235 in DWG), which the first difference had hidden; that is for the DWG
    reader.
  - The four MTEXT with line feeds in binary DXF of 2004 and later equal those of ASCII DXF and DWG (`^J`), so 328
    entities of 2004 to 2013 and 326 of 2018 equal DWG, as for ASCII DXF (324 and 322 before).
  - Model space of ASCII and of binary DXF now differs from DWG only in the ids of the three entities AutoCAD renumbered
    in DXF; before, binary DXF of 2004 and later also drew the four MTEXT with their line feeds inside one paragraph.
  - In TypeScript, ASCII and binary DXF of 2000 to 2018 give the same scenes in every view except those three ids.
    R12 differs only in the numbers that ASCII DXF rounds and in handles.
- The Yokosuka City survey plan as DWG 2018 and DXF: its dimension style is now the same; nothing else changed.
- The TypeScript scenes of the 26 local DXF samples in all 106 views are unchanged except the four MTEXT in model space of
  each binary ACadSharp file of 2004 and later. `pnpm run parity` gives the same scenes in TypeScript and Rust for the 37
  local drawings and their 54 other views. Parsing the binary 2000 and 2018 samples takes 22 and 15 ms (21 and 17 ms
  before, within the variation between runs).
- Not checked: an AutoCAD DXF of a metric drawing (whether AutoCAD also leaves out the imperial defaults there), DXF from
  other programs that leave dimension style values out, control characters in TEXT other than the tab, and drawing the
  text of multi-line attributes (their embedded MTEXT is not read yet).

## SXF in Rust

Checked on 2026-10-19 on macOS (Apple Silicon) with Rust 1.99.0, Node.js 24.21.0 and wasm-pack 0.15.0.

- Six synthetic SXF drawings in `tests/fixtures/sfc` and `tests/fixtures/p21` give the same scenes in Rust
  (`crates/zumen/tests/golden.rs`) and in the WebAssembly build (`pnpm run wasm`) as in TypeScript. Besides the two
  drawings of the unit tests, they use all 34 features with predefined and user-defined colors, line types and widths,
  the four kinds of composite figures nested and placed with unequal scales, cycles, duplicate and missing definitions,
  every arrow and marker code, vertical, slanted and spaced text, composite curves of every curve type, the five
  dimension types, labels and balloons, every hatch kind, title blocks, features after the sheet and unreadable,
  invalid and unknown features. The P21 drawing also has trimmed lines, circles, ellipses and clothoids by parameter and
  by point, Bézier curves, all text anchors, STEP escapes, an 8-bit Shift_JIS layer name and 70 nested sub-figures.
- The public sample `D0LS004Z` from the [ezsxf](https://github.com/monozukuri-ai/ezsxf) repository gives the same scene
  in Rust as in TypeScript as SFC (255 KB) and as P21 (1.4 MB), 1,201 entities each (`pnpm run parity`).
- `pnpm run fuzz` read 24,000 damaged copies of the SXF fixtures (8 seeds of 3,000) and 9,000 of the JWW fixtures with
  TypeScript and WebAssembly: the scenes or the error messages were the same for every input. Before that, it found
  two differences in the port (header strings that are not used were not decoded, so an invalid `\X4\` code point did
  not fail as in TypeScript; layer codes above 32 bits) and a clothoid that hung both readers.
- The changes to `@zumenjs/sxf` and `@zumenjs/core` leave the TypeScript scenes of all 54 local samples (SXF, JWW and
  DXF) unchanged, compared with the build of the previous `main`.
- Time (median of seven runs in Node.js, after a warm-up): `D0LS004Z.SFC` 11 ms in TypeScript and 11 ms in WebAssembly
  including the JSON; `D0LS004Z.P21` 67 ms and 23 ms. The native `zumen scene` command takes 10 ms and 20 ms including
  the process start and the JSON output.
- WebAssembly with the SXF and JWW readers: 742,516 bytes (gzip −9: 382,785, brotli −11: 295,198), 222,817 bytes
  (brotli 66,163) more than with JWW only.
- `zumen render` draws `D0LS004Z.SFC` (A1) at 60 dpi in 0.06 s with the notes, dimension values, hatching, tables and
  the yellow border of the original. Not compared pixel by pixel with the viewer yet.

## DXF in Rust

Checked on 2026-10-19 on macOS (Apple Silicon) with Rust 1.99.0, Node.js 24.21.0 and wasm-pack 0.15.0.

- 18 synthetic DXF drawings in `tests/fixtures/dxf` give the same scenes in Rust (`crates/zumen/tests/golden.rs`), in the
  WebAssembly build (`pnpm run wasm`) and through Python as in TypeScript, in their default view and in five other views
  (layouts, model space and a missing layout). They cover the code pages (`ANSI_932`, Shift_JIS guessed in R12, Western,
  UTF-8 declared as `ANSI_1252`, an unknown code page), binary DXF (R12 and R2000), lines out of step, nested blocks with
  attributes and arrays, layouts and viewports (scales, clipping, frozen layers, twisted and 3D views, R12 viewports),
  MULTILEADER, MLINE, a drawing with every supported entity type, and the limits of
  [How DXF is handled](formats/dxf.md#limits). Four numbers differ, in the last bit of trigonometric results (two in
  `features.dxf` and two in the model space of `r12.dxf`); everything else is equal bit for bit.
- All 26 DXF files of the local samples (the ACadSharp samples of R12 to 2018 in ASCII and binary, the public drawings of
  Yokosuka City and Imizu City, and generated MULTILEADER and MLINE drawings) give the same scenes in Rust as in TypeScript
  in their default view and in all 54 other views (`pnpm run parity`, which now compares every view).
- `pnpm run fuzz` read 60,000 damaged copies of the DXF fixtures (six seeds of 10,000) and 20,000 of all fixtures with
  TypeScript and WebAssembly: the scenes or the error messages were the same for every input. Before that, it found one difference in
  the new `js::string_to_number` (`Number("--1")` was 1 in Rust and NaN in JavaScript) and a drawing whose dimension
  block contains itself twice: it grows to 2³² entities and exhausted the memory of both readers. Porting found
  three more inputs that hang or overflow the stack in both (a dimension block that contains itself once, a polygon mesh
  of 10⁸ × 10⁸ and a hatch with a NaN pattern base point). All four are now limits that both readers share.
- The changes to `@zumenjs/dxf` leave the TypeScript scenes of every local sample unchanged, in every view (160 scenes),
  compared with the build of the previous commit.
- Time (median of seven runs in Node.js after a warm-up, `readDrawing` and `sceneToJson` against `readSceneJson`):
  `jp-yokosuka-minami-survey-plan.dxf` (2.7 MB, 9,260 entities) 140 ms in TypeScript and 79 ms in WebAssembly;
  `sample_AC1009_ascii.dxf` 35 ms and 18 ms; `sample_AC1015_binary.dxf` (1.4 MB) 24 ms and 8 ms. Natively, the first
  takes 19 ms to parse, 28 ms to build the scene and 10 ms to write the JSON. Building the scene took 73 ms before
  `js::to_fixed` computed `toFixed` with 128-bit integers instead of a full decimal expansion (checked against Node.js on
  140,196 values, ties and subnormal numbers included), and parsing the binary sample took 34 ms before binary data was
  turned into hexadecimal with a table.
- A block that contains itself twice stops at the limit of 1,000,000 entities in 0.2 s in TypeScript and 0.5 s natively
  when the blocks are empty, and in 1.3 s and 2.4 s when each copy draws a line (333,342 entities, a 290 MB scene JSON).
- WebAssembly with the SXF, JWW and DXF readers: 1,167,943 bytes (gzip −9: 545,663, brotli −11: 419,049), 425,427 bytes
  (brotli 123,851) more than with SXF and JWW. Most of it is the DXF reader and the `encoding_rs` tables of the Chinese,
  Korean and other code pages, which browsers decode with their own `TextDecoder`.
- `zumen render` draws the Yokosuka cross sections and the `Layout1` of `sample_AC1015_ascii.dxf` in 0.05 s.

## DWG in Rust

Checked on 2026-10-20 on macOS (Apple M2 Pro) with Rust 1.99.0, Node.js 24.21.0 and wasm-pack 0.15.0. There is no
TypeScript DWG reader, so each DWG is compared with the DXF that AutoCAD wrote for the same drawing
(`crates/zumen-dwg/examples/compare.rs` for the documents and every view, `pnpm run parity` for the scenes).

- Five tests in `crates/zumen-dwg/tests` write the smallest DWG that pins a rule (`tests/writer`, R2000 and R2018). A
  drawing with three layers (one off), four line types, a text style, a dimension style, a block and an anonymous
  dimension block, two layouts, and a line, a TEXT whose Shift_JIS second bytes are 0x5C (`表示ソ能`), a two-line MTEXT,
  an INSERT, an ARC, a closed LWPOLYLINE with a bulge, a linear DIMENSION and a paper space line reads the same as the
  DXF written for it, as R2000 in `ANSI_932` and as R2018 in UTF-16, and gives the same scenes in model space and in the
  layout. A spline stored by its fit points and end tangents gets AutoCAD's control points, a leader with an annotation
  gets its hook vertex, anonymous blocks are named as AutoCAD names them in DXF (`*D7`, and `*U1` for a dynamic block
  saved as R2000), and 800 damaged copies of the drawing finish without a panic, each within five seconds. Writing
  the TEXT angle one degree off in the DXF, or multiplying the byte position in `Bits::at` without saturation, makes
  them fail.
- The ACadSharp samples ([ACadSharp](https://github.com/DomCR/ACadSharp) (MIT), commit 2f8b099), one drawing saved by
  AutoCAD as DWG 2000 to 2018 and as ASCII and binary DXF:
  - 327 (2000), 328 (2004 to 2013) and 326 (2018) of the 335 entities and attributes equal those of the ASCII DXF
    value for value (numbers to a relative 1e-9). The others are the SPLINE flags that DWG does not store (2), the
    background index (63) that DXF adds to MTEXT with a true color background (2, 2004 and later), the background colors
    of R2000 MULTILEADER, which are indexes in DWG (3), the generation flags (71) that the DXF reader takes from the
    attribute type of R2018 multi-line ATTRIB and ATTDEF (2), and two INSERT and one MTEXT that AutoCAD wrote with new
    handles in DXF.
  - The tables, blocks, layouts and draw order are the same, except the handles of two layers and a block that AutoCAD
    renumbered in DXF, the index (62) that DXF adds to the three layers with a true color or a color book color (2004
    and later; DWG stores only the color), and DIMASZ and DIMTXT of the four dimension styles, which DXF leaves out at
    their default (0.18) and the DXF reader takes as 0 (the drawing then uses `$DIMASZ`, the same value, and does not
    use DIMTXT).
  - The three layouts give the same scenes in every version. Model space differs only in the ids of the three entities
    with new handles. Against binary DXF of 2004 and later, four MTEXT also differ, because the DXF reader keeps their
    line feeds instead of breaking paragraphs.
  - The AC1014 (R14) file is refused with a message.
- The Yokosuka City survey plan as DWG 2018 (0.4 MB, read locally), against the DXF of the same drawing used in
  [DXF](#dxf): all 9,260 entities equal those of the DXF value for value, including the Japanese layer names and texts,
  and so do the tables, except the index that DXF adds to two layers with a true color and the dimension style
  defaults that DXF leaves out. In the scene, 37 hatches and 31 arcs have a different number of segments, because DXF
  rounds numbers to 16 digits: a hatch bulge of 1.0000000000000002 is 1.0 in DXF, and a half circle from
  225.73767313748996° to 45.737673137489956° is 225.7376731374899° to 45.73767313748995°.
- Samples without a DXF, read without broken objects: the ezdwg samples ([ezdwg](https://github.com/monozukuri-ai/ezdwg)
  (MIT), commit d5e4336, `examples/data`) of an arc, a line, a 2D polyline, a text and an MTEXT (2000, code page
  `ANSI_1251`), an INSERT (2004, code page number 45, unknown, read as Windows-1252 and reported) and an imperial
  mechanical drawing (2010, 71 entities; its 17 dimensions have no block in the file and are reported, as from DXF);
  ACadSharp's `BLOCKPOINTPARAMETER.dwg` (a dynamic block, 2018); and `farmhouse.dwg` (2013, 1.9 MB, 25,873 entities in
  model space and 26 layouts, source not recorded), where 45 REGION are not read and 162 splines with fit points but
  no tangents are drawn through their points.
- `pnpm run parity` over the local samples: the 37 SXF, JWW and DXF drawings and their 54 other views still give the
  same scenes in TypeScript and Rust, and the 20 layouts of the 7 DWG with a DXF equal the DXF ones; their model spaces
  differ as above.
- `crates/zumen-dwg/examples/fuzz.rs`, built with overflow checks and debug assertions, read 64,040 damaged copies of
  the 17 DWG samples (1,000 of each of 16 per seed with four seeds, and 40 of `farmhouse.dwg`): 28,135 were read and
  drawn in every view, 35,905 were refused with a message, none panicked and none took more than four times as long as
  the undamaged drawing (or 2 s). Before that, it found two damaged hatches that hung both `dxf_to_scene` and
  `dxfToScene`: their pattern base points (6.6 × 10¹¹⁶ and −5.8 × 10¹⁹⁹) made the line numbers so large that adding 1
  no longer changed them. Both readers now draw such a hatch as its outline (`hatch-dense`, a new hatch in
  `tests/fixtures/dxf/limits.dxf`).
- Time (median of seven runs after a warm-up): natively, `read_dwg` takes 0.5 to 0.9 ms for the ACadSharp samples,
  5.6 ms for the Yokosuka plan (0.4 MB) and 29 ms for `farmhouse.dwg`, and the model space scene 1.7, 29 and 113 ms. In
  Node.js, `readSceneJson` (WebAssembly, with the JSON) reads the 2018 sample in 6 ms, the Yokosuka plan in 63 ms and
  `farmhouse.dwg` in 253 ms; the same drawings as DXF take 10 and 78 ms in WebAssembly and 12 and 99 ms in TypeScript.
- WebAssembly with the DWG reader: 1,366,446 bytes (gzip −9: 617,613, brotli −11: 482,377), 198,503 bytes (brotli
  63,074) more than `main` without it. It returns the scenes of all 31 fixtures (`scripts/wasm-check.mjs`).
- Not checked: DWG files written by programs other than AutoCAD, Japanese text in real DWG files before 2007 (only the
  synthetic `ANSI_932` test), files over 2 MB, external references, proxy graphics, R2007 files other than the
  ACadSharp sample, and DWG in the browser packages and the playground, which do not read it yet.

## Rust readers, rendering and bindings

Checked on 2026-10-18 on macOS (Apple Silicon) with Rust 1.99.0, Node.js 24.21.0, Python 3.15.0, wasm-pack 0.15.0
(wasm-bindgen 0.2.129).

- The synthetic JWW drawings of the TypeScript tests (versions 600 and 700, written to `tests/fixtures/jww`) give the
  same scenes in Rust as in TypeScript (`crates/zumen/tests/golden.rs`). Through Python, `json.loads(scene.to_json())`
  is equal to the TypeScript scene without any tolerance.
- All 205 JWW files of the public [ezjww](https://github.com/monozukuri-ai/ezjww) repository (MIT, commit 66af683, read
  locally and not committed) give the same scenes in Rust as in TypeScript with `pnpm run parity` (numbers to a relative
  1e-9). 196 of them are equal bit for bit; the other 9 differ only in the last bits of trigonometric results. They include the 15 drawings of `jww_samples` (an apartment floor plan with 1,754
  entities, a shadow diagram with 1,720, a sky factor table with 6,457 on A0), the block regression files, files saved by
  the JWW writer and JWW files generated from JWC and JWS samples.
- WebAssembly (`pnpm run wasm`): 519,699 bytes (gzip −9: 297,963, brotli −11: 229,035) with the JWW reader only.
  Unicode normalization (NFKC for font names) accounts for about 128 KB before `wasm-opt`. The output equals the
  fixtures and the TypeScript scenes of four real drawings.
- PNG rendering at 100 dpi with the release build of the `zumen` command (median of three runs, including reading and
  PNG encoding): the apartment floor plan on A3 (1,654 × 1,170 px) in 0.06 s, the shadow diagram (2,404 × 1,631 px) in
  0.10 s, and the sky factor table on A0 (4,682 × 3,312 px) in 0.33 s. Reading and writing the scene JSON took 0.02 s,
  0.02 s and 0.07 s.
- The apartment floor plan was drawn by Rust with the view of the playground's viewer (`--size 1484x1324 --margin 48
  --dpr 2`, the canvas of `DrawingViewer` in Chromium at device pixel ratio 2) and compared pixel by pixel with the
  viewer's canvas. Every dark pixel of the viewer (64,694) has a dark pixel of Rust within 2 px; 1.09% of the dark
  pixels of Rust (62,687) have none in the viewer, all in text, because the browser draws with Hiragino and Rust with
  the bundled BIZ UDGothic. Room names, vertical text, half-width katakana, dimension values, dashed center lines,
  solids and hatches are drawn in the same places.
- Not checked: Windows and Linux builds of the Python wheel (CI builds on Linux), free-threaded Python, fonts other
  than the bundled BIZ UDGothic (Mincho text is drawn in Gothic), and rendering of SXF and DXF scenes (not ported yet).

## ST-Bridge

Checked on 2026-10-18 on macOS (Apple Silicon), Node.js 24.21.0 and Chromium (Playwright 1.64.0, headless, WebGL through
SwiftShader), before the package moved into this repository.

- 6 unit tests: names in Shift_JIS and a declaration that does not match the contents, rejecting 1.x, the orientation of
  an H-section column (reference direction, `isReferenceDirection`, rotation and offset), the top-of-girder reference
  point and the RC section element names of 2.0 and 2.1, the segments and switch positions of 2.1, the haunch positions
  of 2.0, where the concrete strength comes from, the position of a wall opening, the counts of what is not drawn, and
  the members on a grid axis.
- Public samples, read locally under their repositories' licenses and not committed:
  - [STB-DiffChecker](https://github.com/NS-NS/STB-DiffChecker) (MIT, commit 2fb1230) `TestData/FileA.stb`, a training
    model exported by Super Build/SS7 (ver.2.0.2, 117 KB, steel, five stories: 40 columns, 75 girders, 56 beams and
    82 slabs), in 27 ms in Node.js. It declares Shift_JIS but contains UTF-8; this is reported, and the Japanese names
    are read. `Mini210_FileA.stb` (ver.2.1.0) is read as well.
  - [HoaryFox](https://github.com/hrntsm/HoaryFox) (MIT, commit f991f97) `Samples/SampleBuilding.stb` (ver.2.0.2, 83 KB:
    105 columns, 192 girders and 10 braces), in 12 ms.
  - [STBDotNet](https://github.com/hrntsm/STBDotNet) (MIT, commit 2e74268) `TestStbFiles/ver2/Sample1.stb`, an RC
    building from SS7 (ver.2.0.1, 63 KB: 17 columns, 34 girders, 11 beams, 27 slabs, 6 walls, 9 footings and
    8 parapets). `ver1/Sample1.stb` (1.3.00) is rejected with a message.
  - [stb-kit](https://github.com/taisei-oss/stb-kit) (MPL-2.0, commit ea558d8) `tests/fixtures/cases`: 16 files of
    ver.2.1.1 (S, RC, SRC and CFT columns; tapered, haunched and five-section girders; lipped channels and T sections;
    round bar braces; five footing shapes and strip footings; six kinds of piles; parapets; a wall with openings; slabs).
- In the viewer: fit, picking and properties (column, girder, wall, parapet, footing, slab), the original XML
  attributes, plans by story (3FL, RFL), an elevation by axis (X2), colors by structure type, edges, transparency,
  nodes, member kinds, Esc to clear the selection, the dark theme, and no horizontal scroll at 375 px.
- A synthetic model (25 × 25 grid axes, 20 stories, 48,020 members, 7.3 MB) is read in 0.5 s and tessellated into
  3.8 million vertices and 2.4 million triangles in 0.65 s in Node.js. The model takes about 160 MB of JavaScript heap.
  Headless Chromium with software rendering takes about 11 s to show it, about 50 ms to pick and about 170 ms to filter
  by story.

Checked again on 2026-10-19 on macOS (Apple Silicon) and Node.js 24.21.0, after the move into `packages/stb`
(the code is unchanged except the id of the style element).

- `pnpm run check` passes, with the 6 tests above among 32.
- The samples above, two samples of [SEPIKA](https://github.com/hrntsm/SEPIKA) and the synthetic model are read by the
  built package with the same member counts. The synthetic model takes 0.49 s; 1.3.00 is rejected.
- The playground opens drawings and ST-Bridge on one page and tells them apart by their contents (`isStb`). In one
  session it switched from `FileA.stb` (253 members; picking a slab shows it) to `D0LS004Z.SFC` (1,201 entities), to the
  1.3.00 file (the error is shown in the header) and to a ver.2.1.1 stb-kit sample. Opening a drawing loads no three.js
  module. The production build puts three.js and the 3D view in their own chunk (609 KB, gzip 155 KB), next to the main
  chunk (488 KB, gzip 160 KB), which includes the ST-Bridge parser used by `isStb`.
- Not checked: Firefox and Safari, files from authoring software that the samples do not cover, arc, radial and drawing
  axes, and members with intermediate nodes (none of the public samples have them).
