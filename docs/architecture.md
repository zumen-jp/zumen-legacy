# Architecture

zumen is a pnpm workspace with one package per drawing format, a format-neutral drawing engine,
and a thin React layer. A drawing flows through the packages like this:

```text
bytes ─▶ format reader ─▶ native model ─▶ toScene ─▶ Scene ─▶ DrawingViewer (Canvas) ─▶ React UI
         (@zumenjs/sxf,     (SxfDrawing,              (@zumenjs/core)                     (@zumenjs/react)
          @zumenjs/jww,      JwwDocument,
          @zumenjs/dxf)      DxfDocument)
```

## Packages and dependency rules

| Package | Responsibility |
| --- | --- |
| `@zumenjs/core` | The `Scene` model and its types, `NoticeList`, `readDrawing` and the `DrawingReader` interface, the Canvas 2D renderer, hit testing and snapping (`HitIndex`), distance measurement (`measureDistance`, `toReal`), and `DrawingViewer`. Geometry helpers are exported from `@zumenjs/core/geometry` and text layout from `@zumenjs/core/text`. |
| `@zumenjs/sxf` | SFC and P21 parsing into the SXF feature model (`parseSfc`, `parseP21` → `SxfDrawing`), conversion to a scene (`sxfToScene`), and the `sxf` reader. |
| `@zumenjs/jww` | JWW parsing into the JWW document model (`readJww` → `JwwDocument`), conversion to a scene (`jwwToScene`), and the `jww` reader. |
| `@zumenjs/dxf` | DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parseDxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxfToScene`), and the `dxf` reader. The model and the conversion are shared with the DWG reader. |
| `@zumenjs/react` | `useDrawingViewer` (the engine in React), `CadViewer` (a complete viewer), and `useCadFile`. |

1. Format packages depend only on `@zumenjs/core`, never on each other.
   For example, `@zumenjs/jww` keeps its own copy of the SXF-compatible colors and line types.
   The one exception is the planned `@zumenjs/dwg`: DXF and DWG are two encodings of the same AutoCAD drawing database,
   so the DWG reader produces a `DxfDocument` and draws it with `dxfToScene` from `@zumenjs/dxf`
   (see [How DXF is handled](formats/dxf.md#the-autocad-drawing-model)).
2. `@zumenjs/core` has no knowledge of any format. Formats are passed to `readDrawing` as `DrawingReader`s.
3. Parsing (`parse*`, `read*` and everything below them) does not use the DOM, React or the renderer.
   It uses only the types and `NoticeList` from `@zumenjs/core`. The `*ToScene` conversions use the geometry
   and text layout helpers.
4. Interaction (zoom, pan, selection, measurement) lives in `DrawingViewer`. The React layer only reads the
   viewer state and calls its methods.

## The scene model

- Coordinates are paper millimeters with the Y axis up. Placements of sub-figures, parts and blocks are resolved.
- Each entity has drawing primitives (`stroke`, `fill`, `clipped`, `dot`, `text`), a bounding box, its layers
  (its own and those of the placements it sits in), a list of properties to show, and, inside a scaled placement,
  a `Placement` with the scale and the transform back to the original (real-world) coordinates.
- `notices` list what was not drawn or was drawn differently, by kind and count. Nothing is dropped silently.
- Colors are `#rrggbb`, or `#rrggbbaa` with opacity. On a light background, near-white colors are drawn black,
  and on a dark background near-black colors are drawn white.
- A drawing can hold several views, such as DXF model space and layouts. `Scene.views` lists them and `Scene.view` is the one drawn;
  `readDrawing(bytes, readers, { view })` passes the choice to the reader (`DrawingReader.read(bytes, format, { view })`).
  Formats with a single view leave both out.

## Adding a format

1. Create a package that depends on `@zumenjs/core`.
2. Parse the bytes into a model that keeps the format's own structure, and report unsupported content with `NoticeList`.
3. Convert the model to a `Scene` with the helpers from `@zumenjs/core/geometry` and `@zumenjs/core/text`.
4. Export a `DrawingReader` with `formats`, `detect` (look at the contents, not the file name) and `read`.
5. Add the smallest synthetic test files that pin the rules that are easy to break without anyone noticing
   (encodings, units, nesting, scales). Do not commit real drawings.

## Extension points for applications

- `viewer.addOverlay(painter)` draws on top of the drawing (markup, photo positions, annotations).
  Call `viewer.invalidate()` when the overlay content changes.
- `viewer.toPaper(px, py)`, `viewer.toScreen(x, y)` and `toReal(placement, x, y)` convert between screen pixels,
  paper millimeters and real-world coordinates.
- `viewer.getState()` and `viewer.subscribe(listener)` expose the state to any UI (React uses `useSyncExternalStore`).
- `onSelect` reports entities the user picks.

## The Rust implementation

The readers are being ported to Rust so that servers, data pipelines and Python can read drawings with the same
results as the browser. The Rust code will become the single implementation of the readers: the browser packages
switch to its WebAssembly build format by format, once the Rust reader returns the same scene as the TypeScript one
on the fixtures and on real drawings. Until then both are maintained, and the TypeScript reader is the reference.

```text
bytes ─▶ zumen-sxf, zumen-jww (and later zumen-dxf, zumen-dwg) ─▶ Scene (zumen-core) ─┬▶ JSON ─▶ sceneFromJson ─▶ DrawingViewer
                                                                                        ├▶ PNG (zumen-render)
                                                                                        └▶ Python (bindings/python), WebAssembly (bindings/wasm)
```

| Crate | Responsibility |
| --- | --- |
| `zumen-core` | The `Scene` types (serde, the same JSON as `sceneToJson`), `NoticeList`, `DrawingReader` and `read_drawing`, geometry (`geometry`), text layout (`text`), measurement, and JavaScript-compatible number handling (`js`). |
| `zumen-sxf` | SFC and P21 parsing into the SXF feature model (`parse_sfc`, `parse_p21` → `SxfDrawing`), conversion to a scene (`sxf_to_scene`), and the `SXF` reader. |
| `zumen-jww` | JWW parsing into the JWW document model (`read_jww`), conversion to a scene (`jww_to_scene`), and the `JWW` reader. |
| `zumen` | `read` and `detect` over every reader. The entry point for applications. |
| `zumen-render` | Draws a scene to PNG with the rules of `render.ts` (color inversion, line weights with a 1 px minimum, dash rules, even-odd fills, clipping, per-character text, small text as a line). |
| `zumen-cli` | The `zumen` command (`detect`, `scene`, `render`). |
| `bindings/python` | PyO3 bindings, built with maturin as an abi3 wheel for Python 3.10 and later. The GIL is released while reading and rendering. |
| `bindings/wasm` | wasm-bindgen bindings that return scene JSON (`readSceneJson`). |

The dependency rules follow the TypeScript packages: format crates depend only on `zumen-core` and never on each other
(the planned `zumen-dwg` reads into the AutoCAD model of `zumen-dxf`), and parsing does not depend on rendering.

### Keeping the two implementations equal

- A port keeps the structure and the order of operations of the TypeScript code, so that the scenes are equal value
  for value: entity ids, properties, notices and their order, and coordinates.
- Strings shown to users follow JavaScript exactly. `zumen_core::js` reproduces `toFixed`, `String(number)`,
  `Intl.NumberFormat("ja-JP")`, `Math.max`/`Math.min` with NaN and signed zeros, V8's `Math.hypot`, and the white space
  of `String.prototype.trim`. Trigonometry uses `libm`, so native and WebAssembly builds give the same values.
- Text decoding uses `encoding_rs`, the WHATWG decoders that browsers implement. Node.js decodes Shift_JIS with ICU,
  which differs from WHATWG for byte 0x80 (U+0080 in browsers and Rust, U+FFFD in Node.js), for broken two-byte
  characters (ICU drops an ASCII second byte that WHATWG keeps) and for the control characters 0x1A, 0x1C and 0x7F.
  Every two-byte character that ICU accepts decodes the same in both.
- Readers do not depend on the user's locale (no `localeCompare` or locale-sensitive formatting other than the fixed
  `ja-JP` number format), and they bound recursion and work on malformed input (nesting depth, curve subdivision,
  hatch lines), so that both implementations stop at the same place instead of overflowing the stack or hanging.
- `tests/fixtures/<format>/` holds synthetic drawings built by the TypeScript tests and the scenes the TypeScript readers
  return (`*.scene.json`). `pnpm run golden` rewrites them. The TypeScript tests check that they are current, and
  `crates/zumen/tests/golden.rs` checks that Rust returns the same scenes (numbers to a relative 1e-9).
- `pnpm run parity` compares the TypeScript and Rust readers on drawings that are not in Git (`ZUMEN_SAMPLES`),
  and `pnpm run wasm` builds the WebAssembly package and checks it against the fixtures.
- `pnpm run fuzz` reads randomly damaged copies of the fixtures (inserted escapes, quotes and brackets, removed and
  repeated ranges, changed numbers) with the built TypeScript packages and the WebAssembly build, and reports any input
  where the scenes or the error messages differ. Bytes that ICU and WHATWG decode differently are replaced first.

### Rust dependencies

Runtime dependencies are permissive and work in WebAssembly. Recorded with the reason, as for the npm packages.

| Crate | License | Why |
| --- | --- | --- |
| `serde`, `serde_json` | MIT OR Apache-2.0 | The scene JSON shared with the browser |
| `encoding_rs` | (MIT OR Apache-2.0) AND BSD-3-Clause | Shift_JIS (CP932) and UTF-16 with the WHATWG rules of `TextDecoder` |
| `unicode-normalization` | (MIT OR Apache-2.0) AND Unicode-3.0 | NFKC for font names, as `String.prototype.normalize` (about 128 KB of the WebAssembly build) |
| `libm` | MIT | The same trigonometry on every target |
| `tiny-skia` | BSD-3-Clause | CPU rasterization for PNG, with anti-aliasing, dashes, even-odd fills and masks |
| `ttf-parser` | MIT OR Apache-2.0 | Glyph outlines of the bundled font |
| `pyo3` | MIT OR Apache-2.0 | Python bindings |
| `wasm-bindgen` | MIT OR Apache-2.0 | WebAssembly bindings |

`zumen-render` bundles BIZ UDGothic Regular (SIL Open Font License 1.1, `crates/zumen-render/fonts/OFL.txt`),
a Japanese gothic close to the MS Gothic that SXF and Jw_cad drawings assume. Characters without a glyph are reported
(`RenderReport.missing_glyphs`, a Python warning).

## Development setup

- Each package's `exports` has an `@zumenjs/source` condition that points at `src`. The type checker
  (`customConditions` in `tsconfig.json`), Vitest and the Vite playground resolve it, so changes apply without a build.
  `publishConfig.exports` leaves it out of the published packages, which contain only `dist`.
- `pnpm run build` compiles the packages in dependency order (core, then sxf, jww and dxf, then react) into each `dist`.
- `pnpm run check` runs Biome, the type checker, the unit tests, `pnpm audit` and all builds, then the Rust checks
  (`check:rust`: rustfmt, Clippy, tests and the wasm32 build) and the Python checks (`check:python`: Ruff, maturin, pytest).
- The Rust toolchain is pinned in `rust-toolchain.toml`, Python in `bindings/python/.python-version` (installed by uv).
