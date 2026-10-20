# Verification records

Sample drawings are not part of this repository, because their terms of use do not allow redistribution or have not been confirmed.

## Windows native verification

Checked on 2026-10-20 with Windows 11, PowerShell 7, Node.js 24.21.0, Rust 1.99.0 MSVC,
wasm-bindgen 0.2.129, Python 3.14.8 and Jw_cad 10.3.6. Samples and exported drawings stayed outside the repository.

- `pnpm run check` passes, including Chromium, native Rust, WebAssembly and Python. The Python suite reports
  42 passed and one skip: this account cannot create symlinks. Its separate directory-junction containment test
  passes without enabling Developer Mode. A conversion test now compares viewport scale ratios with a relative
  tolerance of 1e-12: MSVC's `hypot` differed by one ULP for equal X and Y scales.
- The release `cp310-abi3-win_amd64` wheel installs into a fresh virtual environment in a Japanese path with spaces.
  The installed extension imports, reads a Japanese drawing filename, returns the same JSON as reading its bytes,
  and renders PNG outside the source tree.
- All 15 JWW samples bundled with Jw_cad give the same scenes in TypeScript and native Rust. Script tests also
  run a copied CLI from a Japanese path, compare every DXF view, and require missing executables, malformed drawings
  and zero comparisons to fail instead of silently passing.
- The installed MS Gothic and MS Mincho fonts are exercised in Chromium with Japanese text, half-width katakana,
  punctuation, vertical writing and 30-degree rotation. Tests check visible glyph pixels and hit areas. They use the
  normal fallback families on systems without those fonts; they do not prove identical platform font metrics.
- Jw_cad's bundled apartment plan (1,754 entities) exposed crossed rectangular wall solids in both readers.
  Preserving the JWW serialized boundary order fixes them in the playground and in the DXF opened by Jw_cad.
  Synthetic rectangles, skewed quadrilaterals and triangles in versions 600 and 700 pin the rule in both readers.
- Exporting that plan to SFC initially lost strokes using predefined widths when opened in Jw_cad. Its own SFC
  export declares those widths; adding the declarations restores the walls, arcs and fills. Re-saving the imported
  drawing as JWW preserves all 215 arcs/circles/ellipses, 56 texts and 10 points. Jw_cad expands the hatches into
  many lines (509,215 scene entities after import), so raw entity counts cannot be used as a round-trip oracle.
  The writer regression explicitly checks the declarations, since our own reader accepts the missing declarations.

Limits: Jw_cad imports the DXF model at its chosen scale (1:50 here) and maps some cyan truecolor entities to yellow;
this is not a successful paper-layout or color-fidelity check. SFC retains the sheet appearance but its hatch
expansion makes editing expensive. AutoCAD was not installed, so native DWG/DXF rendering and AutoCAD round trips
remain unverified. No claim of pixel-identical text or complete CAD interoperability follows from these checks.

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

Checked on 2026-10-20 on macOS (Apple M2 Pro) with Node.js 24.21.0 and Rust 1.99.0. Comparing DWG with the DXF of the
same drawing (`crates/zumen-dwg/examples/compare.rs`, see [DWG in Rust](#dwg-in-rust)) found three values that the DXF
reader read differently from AutoCAD. Both DXF readers now read them as [How DXF is handled](formats/dxf.md#reading)
describes.

- Three synthetic drawings in `tests/fixtures/dxf` and three unit tests pin the rules: a multi-line ATTDEF as AutoCAD
  2018 writes it (attribute type 4 in `AcDbAttributeDefinition`, text in the embedded MTEXT) is drawn upright, and an
  ATTRIB whose `AcDbText` 71 is 2 is still mirrored (`attributes-r2018`); binary MTEXT with a line feed and a tab, MTEXT
  `x^2` and TEXT `m^2` read as the ASCII DXF of the same strings and draw two lines, `x^2` and `m^2` (`binary-text`);
  and in a metric drawing whose `$DIMASZ` is 2.5, a leader whose dimension style leaves DIMASZ out gets an arrow of 0.18
  (`dimstyle-defaults`). Undoing any of the four changes (the subclass of 71, the caret notation, `^ ` in TEXT, the
  0.18) in TypeScript or in Rust makes them fail.
- The ACadSharp samples, one drawing saved by AutoCAD as DWG 2000 to 2018 and as ASCII and binary DXF:
  - The dimension styles of DXF and DWG are now equal in every version; DIMASZ and DIMTXT of four of the six styles were
    0 in DXF.
  - The R2018 multi-line ATTRIB and ATTDEF have the generation flags of DWG (0; DXF had 2 and 4). They still differ in
    their attribute flags (70: 0 in DXF, 28 and 235 in DWG), which the first difference had hidden; that is for the DWG
    reader (fixed since, see [Multi-line attributes of AutoCAD 2018](#multi-line-attributes-of-autocad-2018)).
  - The four MTEXT with line feeds in binary DXF of 2004 and later equal those of ASCII DXF and DWG (`^J`), so 328
    entities of 2004 to 2013 and 326 of 2018 equal DWG, as for ASCII DXF (324 and 322 before).
  - Model space of ASCII and of binary DXF now differs from DWG only in the ids of the three entities AutoCAD renumbered
    in DXF; before, binary DXF of 2004 and later also drew the four MTEXT with their line feeds inside one paragraph.
  - In TypeScript, ASCII and binary DXF of 2000 to 2018 give the same scenes in every view except those three ids.
    R12 differs only in the numbers that ASCII DXF rounds and in handles.
- The Yokosuka City survey plan as DWG 2018 and DXF: its dimension style is now the same; nothing else changed.
- The TypeScript scenes of the 26 local DXF samples in all 106 views are unchanged except the four MTEXT in model space
  of each binary ACadSharp file of 2004 and later. `pnpm run parity` gives the same scenes in TypeScript and Rust for
  the 37 local drawings and their 54 other views, and the 20 layouts of the 7 DWG with a DXF still equal the DXF ones.
  Parsing the binary 2000 and 2018 samples takes 22 and 15 ms (21 and 17 ms before, within the variation between runs).
- Not checked: an AutoCAD DXF of a metric drawing (whether AutoCAD also leaves out the imperial defaults there), DXF
  from other programs that leave dimension style values out, control characters in TEXT other than the tab, and drawing
  the text of multi-line attributes (their embedded MTEXT is not read yet; drawn since, see
  [Multi-line attributes of AutoCAD 2018](#multi-line-attributes-of-autocad-2018)).

## Multi-line attributes of AutoCAD 2018

Checked on 2026-10-20 on macOS (Apple M2 Pro) with Node.js 24.21.0, Rust 1.99.0 and wasm-bindgen 0.2.129. AutoCAD 2018
keeps the text of a multi-line ATTRIB or ATTDEF in an embedded MTEXT, which neither the DXF readers nor the DWG reader
read, and the DWG reader read the values after it out of step. Both are now read as
[How DXF is handled](formats/dxf.md#reading) and [How DWG is handled](formats/dwg.md#text) describe.

- `tests/fixtures/dxf/attributes-multiline` and three unit tests pin the rules in TypeScript and Rust: a two-line ATTRIB
  of an INSERT is drawn in its own color with the same text as an MTEXT of the same values; a hidden one is not drawn;
  the extrusion is the embedded MTEXT's, not the attribute's; in the block, the constant ATTDEF draws its value and the
  other ATTDEF nothing; the ATTDEF in model space draws its tag where its MTEXT is; and a MULTILEADER whose content is
  the block draws its attribute value (302) as the text of the ATTDEF's MTEXT. In `attributes-r2018`, the tag of the
  multi-line ATTDEF is now laid out from its MTEXT, one line lower than the single-line text was, because the synthetic
  drawing leaves out the alignment point that AutoCAD writes. Taking out the multi-line branch or the attribute value
  in TypeScript or in Rust makes them fail.
- `crates/zumen-dwg/tests`: the test writer now writes R2018 MTEXT as AutoCAD does (not annotative, with the redundant
  fields and no columns), attributes with an embedded MTEXT, and an INSERT with attributes. A drawing with the
  attributes above reads the same as its DXF, entity by entity and in the scene, with the flags 8, 1 and 4 that were
  written; reading the bit the old way gives 128 instead of 8. `tests/fixtures/dwg/r2018.dwg` is rewritten for the new
  MTEXT form; its scenes do not change.
- The ACadSharp samples:
  - In 2018, the ATTDEF and ATTRIB now equal those of ASCII and binary DXF, flags included (DWG had 235 and 28), so 328
    of the 335 entities and attributes equal DXF, as in 2004 to 2013 (326 before). With the bit read as the
    specification says, the data of every MTEXT and of both attributes is read to its last bit (a temporary check);
    read the other way, 299 to 581 bits were left after each MTEXT.
  - The ATTRIB (`my multi line text for the attrrib`, one line) is drawn at the same place from DWG, ASCII and binary
    DXF (24,822.95 and 679.282 mm in model space, `zumen texts`), so model space has 208 entities, as in 2000 to 2013
    (207 before). Those versions hold it as single-line text, which AutoCAD placed 4.0 mm to the left and 0.7 mm
    lower. The ATTDEF is in a block and not constant, so it is not drawn.
  - In the playground, `sample_AC1032_ascii.dxf` (TypeScript) and `sample_AC1032.dwg` (WebAssembly) draw the
    attribute in model space between `hello` and `bla bla`, and selecting it shows an ATTRIB with the tag
    `MULTI_LINE_ATT` and the height 1.7207.
  - The other versions compare as before.
- The other 16 local DWG files (two more of 2018, without MTEXT) and the other 24 local DXF files give the same scenes
  as before in their default views. `pnpm run parity` gives the same scenes in TypeScript and Rust for the 37 local
  drawings and their 54 other views, and the 20 layouts of the 7 DWG with a DXF equal the DXF ones.
- `crates/zumen-dwg/examples/fuzz.rs`, with overflow checks, read 12,000 damaged copies of the 2018 sample and
  `r2018.dwg` (3,000 of each per seed, two seeds): 3,502 were read and drawn, 8,498 were refused with a message, and
  none panicked or was slow. `scripts/fuzz-parity.mjs` compared TypeScript and WebAssembly on 12,000 damaged copies of
  the 22 DXF fixtures (three seeds of 4,000): the same scene or error everywhere, except 30 unreadable inputs whose
  messages differ in the list of formats (WebAssembly lists DWG, which the TypeScript readers do not read).
- Not checked: AutoCAD multi-line attributes with several lines, columns or a tilted MTEXT (the sample has one line),
  annotative MTEXT (none in the samples), and how AutoCAD itself displays these attributes.

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
    of R2000 MULTILEADER, which are indexes in DWG (3), the generation flags (71) that the DXF reader took from the
    attribute type of R2018 multi-line ATTRIB and ATTDEF (2; fixed since, see
    [DXF against the DWG of the same drawing](#dxf-against-the-dwg-of-the-same-drawing), and their attribute flags,
    which DWG read out of step, in [Multi-line attributes of AutoCAD 2018](#multi-line-attributes-of-autocad-2018)), and
    two INSERT and one MTEXT that AutoCAD wrote with new handles in DXF.
  - The tables, blocks, layouts and draw order are the same, except the handles of two layers and a block that AutoCAD
    renumbered in DXF, the index (62) that DXF adds to the three layers with a true color or a color book color (2004
    and later; DWG stores only the color), and DIMASZ and DIMTXT of the four dimension styles, which DXF leaves out at
    their default (0.18) and the DXF reader took as 0 (the drawing then used `$DIMASZ`, the same value, and does not
    use DIMTXT; read as 0.18 since).
  - The three layouts give the same scenes in every version. Model space differs only in the ids of the three entities
    with new handles. Against binary DXF of 2004 and later, four MTEXT also differed, because the DXF reader kept their
    line feeds instead of breaking paragraphs (fixed since).
  - The AC1014 (R14) file is refused with a message.
- The Yokosuka City survey plan as DWG 2018 (0.4 MB, read locally), against the DXF of the same drawing used in
  [DXF](#dxf): all 9,260 entities equal those of the DXF value for value, including the Japanese layer names and texts,
  and so do the tables, except the index that DXF adds to two layers with a true color and the dimension style defaults
  that DXF leaves out (read as DWG has them since). In the scene, 37 hatches and 31 arcs have a different number of
  segments, because DXF rounds numbers to 16 digits: a hatch bulge of 1.0000000000000002 is 1.0 in DXF, and a half
  circle from 225.73767313748996° to 45.737673137489956° is 225.7376731374899° to 45.73767313748995°.
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
  ACadSharp sample, and DWG in the browser, which [DWG in the browser](#dwg-in-the-browser) checks.

## DWG in the browser

Checked on 2026-10-20 on macOS (Apple M2 Pro) with Rust 1.99.0, Node.js 24.21.0, wasm-bindgen 0.2.129, Vite 8.3.4 and
Chromium 152. Times are medians of seven runs after a warm-up.

- Seven tests in `packages/dwg/tests`. The two DWG fixtures (`tests/fixtures/dwg`, the drawing of the Rust tests as
  R2000 in `ANSI_932` and as R2018 in UTF-16, 3.3 and 3.7 KB) give the same scenes in model space and in `Layout1`
  through `@zumenjs/dwg` in Node.js (the golden files), natively (`crates/zumen/tests/golden.rs`) and in
  `scripts/wasm-check.mjs` with the other 31 fixtures. Reading before `initDwg` throws an error that names it, a failed
  load is retried, `isDwg` agrees with the WebAssembly `detect` on every fixture and 11 short inputs, an R14 file
  throws the reader's message, and point lists are `Float64Array`.
- Size: the module (`packages/dwg/wasm/zumen_wasm_bg.wasm`, all four readers, no `wasm-opt`) is 1,560,473 bytes
  (gzip −9: 630,299, brotli −11: 493,835), and the JavaScript glue 8,483 bytes. `pnpm pack` of `@zumenjs/dwg` is
  643,606 bytes. Through `wasm-opt -O` (wasm-pack 0.15.0, Binaryen 117) the module would be 1,366,317 bytes (brotli
  482,539) and read the Yokosuka plan in the same 62–64 ms in Node.js; with the DWG reader only, 992,405 bytes (brotli
  367,123).
- The playground's main script grows by 4.9 kB (gzip 2.0 kB) to 494.45 kB. The module is a separate file, fetched only
  when a DWG is opened: opening a JWW first does not fetch it.
- Loading: in Node.js, `initDwg()` reads, compiles and instantiates the module in 15 ms. In Chromium, compiling it
  from the production build of the playground (`WebAssembly.compileStreaming`, the file served as `application/wasm`)
  takes 3.6 ms, because V8 compiles each function when it is first called. In that build, the Yokosuka plan appeared
  153 ms after it was dropped on the page, including the module, reading and drawing.
- Reading in Chromium (`readDrawing`; the WebAssembly part includes writing and decoding the JSON):
  `sample_AC1032.dwg` (1.1 MB, 207 entities, 0.9 MB of JSON) in 9.9 ms (7.0 ms in WebAssembly, 2.6 ms in
  `sceneFromJson`), the Yokosuka plan (0.4 MB, 9,260 entities, 8.8 MB of JSON) in 89.9 ms (66.2 and 24.2 ms), and
  `farmhouse.dwg` (1.9 MB, 25,873 entities, 39.8 MB of JSON) in 383.9 ms (290.5 and 99.3 ms). `@zumenjs/dxf` reads the
  same drawings as ASCII DXF in 16.9 and 65.2 ms. In Node.js through the built package: 10, 103 and 414 ms.
- Memory: the WebAssembly memory grows to 160 MB while reading `farmhouse.dwg` and stays there when it is read twelve
  times and other drawings after it (freed memory is reused; WebAssembly memory does not shrink).
- In the playground: `sample_AC1032.dwg` in model space and in its three layouts, from the view menu and from
  `?view=Layout1`. In the `Layout1` viewport (1:1.442, inches), a line measures 57.6 mm on paper and 83.1 mm in real
  units, the same as in `sample_AC1032_ascii.dxf` at the same points. The Yokosuka plan (UTF-16, 15 layers with
  Japanese names) in model space, where the left sheet frame measures 574.0 mm, as in its DXF. `sample_AC1014.dwg`
  shows 「対応していない DWG の版です（AC1014、AutoCAD R13・R14）。R2000 以降の DWG を読めます」. A JWW opened after a
  DWG reads as before.
- The packed `@zumenjs/core` and `@zumenjs/dwg`, installed in a separate Vite app: `initDwg()` without an argument
  loads the module in the development server (from `node_modules`, after dependency pre-bundling) and in a build
  (`assets/zumen_wasm_bg-[hash].wasm`, 5.97 kB of JavaScript), and the same installed package reads the R2018 fixture
  in Node.js.
- Not checked: Firefox and Safari, webpack and other bundlers, Web Workers, slow networks and devices, and pages with a
  Content Security Policy.

## The 2D engine to embed

Checked on 2026-10-20 on macOS (Apple M2 Pro), Node.js 24.21.0 and Chromium 156 (Playwright 1.64.0, headless), for
the first step of [decision 0005](decisions/0005-browser-packages-for-embedding.md): `@zumenjs/viewer`, the hooks of
`@zumenjs/react` and the 2D sample of the playground.

- 15 tests in Chromium (`packages/viewer/tests`, `packages/react/tests`). The view: `jumpTo` and `getView` give back
  the same center and zoom, `move` and `moveend` come only when the view changes (so two views that follow each other
  stop), `fitBounds` keeps its padding and fits again on resize until the view is moved, and `zoomBy` and the wheel
  keep the paper point under the pointer. The input handlers: the three wheel modes (the page scroll is left alone
  when the wheel does not zoom) and `moveend` after the wheel stops, dragging, pinching with two touch pointers, double
  click and keys, each also turned off. The state and the tools: selection with Shift and Escape, no new state for an
  unchanged selection, queries by point and box without hidden layers, measurement snapped to vertices with the
  real-world distance of a 1/100 placement, and a tool of the application that receives clicks and drags in paper
  millimeters and draws. The style: the background, the selection, a named highlight and the hover color from CSS
  custom properties and `setStyle`, checked on canvas pixels, and near-white lines drawn black on a light color given
  as the dark background. The hooks: `useViewerState` re-renders only when its part changes (also with a selector that
  builds arrays), `useDrawingViewer` destroys the engine, and `useDrawing` loads only the reader that recognizes the
  file and reports a failed load. Each of seven deliberate mistakes in the engine and two in the hooks made a test fail.
- The same steps in the playground before the change (`main` at 75a227c, with `CadViewer`) and after it, with synthetic
  events in headless Chromium at device pixel ratios 1 and 2 in a 1280 × 860 window: fit, wheel zoom with and without
  Ctrl, a drag, the `+` and arrow keys, a double click and `-`, `0`, three clicks to select, line weights off, the light
  background, hiding the first layer, two measurement clicks with snapping and the pointer moved on, and Escape. On
  `D0LS004Z.SFC` (1,201 entities), the Jw_cad sample `サンプル.jww` (version 600, 695 entities, light background), the
  Yokosuka survey plan as DXF (9,260 entities) and `farmhouse.dwg` (25,873 entities, read through WebAssembly), all 96
  snapshots of the drawing canvas and of the overlay canvas were the same, pixel for pixel, and the pages logged no
  errors.
- In the playground (built-in browser of the Claude desktop app): switching an ACadSharp drawing to `MyLayout` and
  measuring inside its viewport gives 57.9 mm on paper and 83.1 mm real, as before; Shift adds a second wall of the
  Jw_cad sample to the selection and both are highlighted; the layer groups, the properties and the notices show as
  before; at 375 px the panels start closed and the page does not scroll sideways; ST-Bridge models still open in 3D.
- Not checked: Firefox and Safari, real touch devices (pinching only with synthetic touch pointers), screen readers,
  and the 3D view, which keeps its current interface until the next step.

## DXF writing

Checked on 2026-10-20 on macOS 27 (Apple Silicon) with Rust 1.99.0, Python 3.14.8 and ezdxf 1.4.5.

- Each of the 23 DXF and DWG drawings in `tests/fixtures` is written as DXF, read again and drawn in its default view and
  every other view (52 scenes): every entity has the same type, layers, box and drawn primitives as in the drawing it
  came from, to a relative 1e-9 (`crates/zumen/tests/convert.rs`). The exceptions are the ones reported in `notices`:
  the IMAGE is left out, and HELIX, MPOLYGON and an attribute outside a block reference change their type only.
- The same check on the 42 local DXF and DWG samples that zumen reads (the ACadSharp samples of R12 to 2018 in ASCII and
  binary, the public drawings of Yokosuka City and Imizu City, and generated MULTILEADER and MLINE drawings), in all 198
  scenes, finds no difference (`ZUMEN_SAMPLES=… cargo test -p zumen --release --test convert -- --ignored`). Before
  `@zumenjs/dxf` read binary DXF strings in ASCII notation, the four MTEXT with line feeds of each binary sample of 2004
  and later differed; they are now written as `^J`.
- In every written file, handles are unique and below `$HANDSEED`, and every handle reference (330 to 369, 390, 1005)
  points at an object written (`write::tests` on the fixtures, and a script on the samples).
- ezdxf reads every written fixture and sample strictly (`ezdxf.readfile`, not its recover mode). Its audit finds nothing
  in the 42 samples, and in the fixtures only what `features.dxf` and `limits.dxf` pin on purpose (block reference
  cycles, a wrong color index and transparency, a LEADER with one vertex); the Python tests check this. Most fixtures are
  too small for ezdxf to read before conversion.
- Writing a written file again gives the same entities and entity handles; the new handles of objects that the model
  does not keep (tables, dictionaries) move, so the bytes differ.
- Time with the release build of the `zumen` command, including reading (median of seven runs):
  `jp-yokosuka-minami-survey-plan.dxf` (2.7 MB, 9,260 entities) 45 ms, its DWG (0.4 MB) 31 ms, both written as 2.4 MB
  of DXF; `sample_AC1015_binary.dxf` (1.4 MB) 9 ms.
- Not checked: opening the written files in AutoCAD or other CAD programs, and multileaders with block content in
  AutoCAD (ezdxf reads them).

## Editing DXF in place

Checked on 2026-10-20 on macOS 27 (Apple Silicon) with Rust 1.99.0, Python 3.14.8 and ezdxf 1.4.5
([decision 0012](decisions/0012-edit-in-place-and-write-every-format.md)).

- Each of the 23 DXF fixtures, edited without operations, gives the same bytes. Edited with a layer added, a line and a
  text added on it, the layer of the first entity changed and another entity deleted, it reads again with the added
  ids, the new layer and without the deleted entity, and every other entity has the same kind, properties and shapes
  in the drawing's own coordinates (`crates/zumen/tests/edit.rs`). The same check finds no difference in the 26 local DXF
  samples (R12 to 2018, ASCII and binary, the public drawings of Yokosuka City and Imizu City, and generated MULTILEADER
  and MLINE drawings; `ZUMEN_SAMPLES=… cargo test -p zumen --release --test edit -- --ignored`).
- The first version compared the boxes and primitives in paper millimeters. A line added far from the drawing, or the
  deletion of an entity at its edge, changes the extents, and the whole sheet moves (`survey.dxf`); an XLINE is drawn
  as long as the extents. The test now compares shapes through the placement of each entity, and XLINE by its
  properties only.
- ezdxf reads every edited file strictly. Its audit finds nothing in the fixtures converted to DXF and in a new drawing
  after every kind was added, an entity changed and one deleted (`bindings/python/tests/test_edit.py`), and in the 26
  samples it finds the same before and after the edit (nothing, except an INVALID_TABLE_HANDLE in the Imizu drawing
  that is there before).
- `$HANDSEED` of the files that zumen writes is above every handle; the editor also takes handles above the largest
  handle when a drawing's seed is lower.
- Time to edit with the release build from Python (the operations above): `jp-yokosuka-minami-survey-plan.dxf`
  (2.7 MB) 27 ms, the 2.6 MB ASCII samples 10 to 11 ms, the 1.4 MB binary samples 44 to 45 ms.
- Not checked: opening edited files in AutoCAD; the check on the Windows host with ODA File Converter and DWG TrueView
  is to come.

## SXF and JWW to DXF

Checked on 2026-10-20 on macOS 27 (Apple Silicon) with Rust 1.99.0, Node.js 24.21.0, Python 3.14.8 and ezdxf 1.4.5.

- The 8 SXF and JWW drawings in `tests/fixtures` (153 entities, including every SXF feature, broken references and
  cycles of composite figures, geodetic and non-uniform sub-figures) are mapped into the AutoCAD model, written as DXF,
  read again and drawn in the layout that the DXF opens in (`crates/zumen/tests/convert_sxf_jww.rs`). Each entity of the
  original scene is matched with the entities written from it (through the source of each written entity) and has the
  same box (within 0.05 mm plus 1/10,000 of its size, the difference in curve flattening), the same drawn primitives in
  the same order (strokes by color, line weight, dashes and distance; fills by color and outline; hatch lines by color,
  dashes, direction, spacing and position; texts by string, color, font and text box, and each character's position
  within a quarter of its size or half the character spacing), its layers, and, through a viewport, the scale of its
  sub-figure or layer group. The exceptions are invisible: the selectable outline of `Area_control` and empty text.
- Synthetic drawings in the same test pin the rules: a 1/100 sub-figure rotated by 30° in real-size model space behind a
  viewport, with the sheet frame in paper space and vertical text in `@ＭＳ 明朝` at 270°; geodetic coordinates swapped;
  overlapping sub-figures moved apart; a profile with different scales as a paper space INSERT; colors, line types,
  line weights, `%%` and `^`, and a linear dimension with its definition points and block; a JWW drawing with 1/100,
  1/50 and 1/1 layer groups (two viewports, the frame in paper space, the 1/50 group moved), blocks, solids, circle
  solids, points and dimensions; and a one-scale JWW drawing kept in model space.
- The same check on the local samples, which are not in Git: the SFC and P21 versions of `D0LS004Z` (1,201 entities each,
  two sub-figures at 1/10 and 1/20 that overlap at real size, so one is moved) and nine JWW drawings (site plans, shadow
  diagrams, house and apartment plans; 8,683 entities) draw the same entities
  (`ZUMEN_SAMPLES=… cargo test -p zumen --release --test convert_sxf_jww -- --ignored`). Rendered to PNG, the layouts of
  `D0LS004Z.SFC` and the apartment plan show the same drawing as the originals.
- ezdxf reads every converted fixture and sample strictly (`ezdxf.readfile`) and its audit finds nothing. For the SXF
  fixture `sample.sfc`, ezdxf's `Viewport.get_transformation_matrix` maps the real-size line of the 1/20 sub-figure to
  the paper millimeters where the SXF scene draws it (the Python tests).
- The 19 DXF files written from the fixtures and samples read the same in TypeScript and Rust, in the layout and in
  model space (`ZUMEN_SAMPLES=<folder of the converted files> pnpm run parity`). In the playground, the converted
  `D0LS004Z.SFC` opens in its layout `図面-1` with viewports at 1:10 and 1:20 and the hidden layer `VIEWPORTS`, and its
  model space shows the two sub-figures at real size side by side.
- `@zumenjs/dxf` and `zumen-dxf` draw `@` fonts vertically with the same scenes (`vertical-text` fixture, TypeScript
  test of the column, and the playground). None of the local DXF samples uses an `@` font, so their scenes do not
  change: `pnpm run parity` over the local samples gives the same 111 scenes as before.
- Time with the release build of the `zumen` command, including reading and writing (median of seven runs):
  `D0LS004Z.SFC` (255 KB) 17 ms, `D0LS004Z.P21` (1.4 MB) 31 ms, both written as 0.58 MB of DXF; `Test6.jww` (128 KB,
  1,962 entities) 18 ms.
- Not checked: opening the written files in AutoCAD, Jw_cad or other CAD programs (in particular how they place the
  column of `@` font text, which zumen infers), and P2Z and SFZ, which are read by another pull request.

## Writing SXF

Checked on 2026-10-20 on macOS 27 (Apple Silicon) with Rust 1.99.0 and Python 3.14.8.

- The 8 SXF and JWW drawings in `tests/fixtures` are converted to DXF, read again, mapped back into SXF and written as
  SFC, and the 25 DXF and DWG fixtures are written as SFC (`crates/zumen/tests/convert_to_sxf.rs`). Read by the SXF
  reader, each entity of the original scene (the SXF scene, or the DXF scene of the view that the drawing opens in) is
  matched with the SXF features written from it, through the source of each feature, and compared as in
  "SXF and JWW to DXF": boxes, drawn primitives in order, layers and the scale of sub-figures. The SXF → DXF → SXF round
  trip finds no difference. From DXF and DWG, the differences that remain are the ones SXF cannot hold and that are
  reported in `notices`: characters outside CP932 and mirrored text, viewport frames that clip, dots of line types
  (0.25 mm), a block that inserts itself (drawn 32 levels deep by the DXF scene, written once), and the points of
  dimensions on `DEFPOINTS`. Also left aside are differences in curve flattening: arcs within twice the chord error of
  the two scenes, and splines within 5 % of their size, because the DXF scene samples the parameter of the whole curve
  evenly (a spline of `farmhouse.dwg` with an uneven first knot span is 310 mm off in the DXF scene; the Bézier curves
  written to SXF match the B-spline to 1e-13, `shapes::tests`).
- The same check on the 57 local samples that zumen reads (the ACadSharp samples of R12 to 2018, the public drawings of
  Yokosuka City and Imizu City, `farmhouse.dwg` with 25,873 entities, the SXF sample `D0LS004Z` and 10 JWW drawings)
  finds no other difference (`ZUMEN_SAMPLES=… cargo test -p zumen --release --test convert_to_sxf -- --ignored`).
- Small drawings in the same test pin the rules: model space as a 1:1 sub-figure on a sheet the size of the drawing,
  block references as drawing parts with a copy for each inherited BYBLOCK color, layer 0 inheriting the reference's
  layer, viewports as sub-figures at their scale and twist, CP932 and the codes of colors and widths. Each SXF and P21
  fixture written as SFC and read again draws the same scene (`crates/zumen-sxf/tests/write.rs`).
- Time with the release build of the `zumen` command, including reading and writing (mean of five runs):
  `farmhouse.dwg` (25,873 entities) 332 ms as SFC (155 ms as DXF), the Yokosuka survey plan (9,260 entities) 103 ms,
  `D0LS004Z.SFC` 31 ms and `Test6.jww` 34 ms.
- Not checked: opening the written SFC in other SXF programs (the SXF browser of the 電子納品 checks, or CAD programs),
  and the limits that the specification sets on the number of layers, user colors, line types and widths, which the
  writer does not enforce yet.

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
