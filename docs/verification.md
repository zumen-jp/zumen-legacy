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
