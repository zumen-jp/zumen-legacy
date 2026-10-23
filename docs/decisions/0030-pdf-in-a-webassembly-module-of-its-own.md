# 0030: Read PDF in browsers with a WebAssembly module of its own, loaded only for PDF drawings

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0020](0020-pdf-drawings-in-rust.md) reads PDF drawings in Rust with hayro-interpret, also in browsers
  through `@zumenjs/reader`, and keeps PDF "in the one module of every format unless it more than doubles the module;
  then a record decides whether it gets a module of its own". This is that record.
- [Issue #90](https://github.com/zumenjs/zumen/issues/90) measured the release build of `bindings/wasm` after
  `wasm-bindgen`, compressed with brotli at quality 11 ([Verification records](../verification.md#pdf-in-rust)):

| Module | Bytes | Brotli |
| --- | --- | --- |
| Every format but PDF, before PDF | 1,930,221 | 579,783 |
| Every format but PDF, with this change | 1,930,001 | 581,296 |
| Every format and PDF in one module | 5,726,005 | 1,782,482 |
| PDF only | 4,281,113 | 1,433,746 |
| PDF only, without the fonts that hayro-interpret embeds | 4,009,351 | 1,213,319 |
| PDF only, without those fonts and without the predefined CMaps | 3,526,423 | 904,034 |

- The code that PDF adds is the interpreter (`hayro-syntax`, `hayro-interpret`, about 750 KB before compression), font
  parsing (`skrifa`, `read-fonts`, about 570 KB), the image decoders that `hayro-syntax` always compiles in (JBIG2,
  JPEG 2000, JPEG and CCITT, about 570 KB, reachable from the stream filters although the reader decodes no image),
  and color management (`moxcms`, about 150 KB). Its data is the substitutes of the 14 standard fonts (221 KB with
  brotli) and the predefined CMaps (309 KB with brotli).
- Both data sets are needed for correct text. A font that the PDF does not embed (Helvetica, or MS Gothic in many
  Japanese drawings) is loaded from a substitute so that hayro-interpret can map its codes to glyphs; without one the
  run is dropped. Japanese PDFs that do not embed their fonts use predefined CMaps (`90ms-RKSJ-H`, `UniJIS-UCS2-H` …),
  and those CID fonts, when they have no `ToUnicode` map, get their Unicode from the `Adobe-Japan1-UCS2` CMap.
- Even the smallest PDF reader would more than double the module (581,296 + 904,034 bytes), and most applications open
  CAD drawings only. An application that shows PDF drawings also loads pdf.js for the pages
  ([issue #83](https://github.com/zumenjs/zumen/issues/83)), so it already downloads a PDF engine.
- `DrawingReader.load()` is how `useDrawing` waits for a reader's module, and it is called only for the reader whose
  `detect` recognized the file. It takes no argument, so a reader that reads several formats cannot load one module per
  format.

## Options

1. **Keep one module**, three times its size before PDF, for every application.
2. **Shrink the PDF reader to fit**, without the fonts and CMaps. It loses text, and is still 1.6 times the module.
3. **A second module with every format and PDF**, which an application loads instead of the first. An application
   that opens CAD drawings and PDF drawings loads the large module even for CAD drawings.
4. **A module with the PDF reader only**, loaded the first time a PDF drawing is read. CAD-only applications load what
   they load today; applications that read both load the two modules (2,015,042 bytes with brotli, 13% more than option
   1, since `zumen-core` and `serde_json` are in both).

## Decision

Option 4.

- `bindings/wasm-pdf` (`zumen-wasm-pdf`) builds a module with the same functions as `bindings/wasm` (`detect`,
  `readSceneJson` and `ReadFailure`) over `zumen-pdf` only. `scripts/wasm.mjs` builds both into
  `packages/reader/wasm`: `zumen_wasm_bg.wasm` and `zumen_wasm_pdf_bg.wasm`.
- The `zumen` crate reads PDF with its feature `pdf`. Its users in the workspace turn it on (the command line, Python,
  the tests of `zumen`) except `bindings/wasm`, so the CAD module does not change. The reading with limits that both
  modules need (`read_drawing_with_limits`, `limit_error`) moves to `zumen-core`.
- `@zumenjs/reader` reads every format, PDF included, through the same functions: `reader`, `createReader`,
  `readScene`, `readJson` and `ReaderWorker`. `detectDrawing` returns `pdf` for PDF files, and a read goes to the module
  of its format. `initReader()` loads the CAD module as before and `initPdfReader()` the PDF one; reading a PDF before
  `initPdfReader()` has finished throws an error that says so.
- `DrawingReader.load(format?)` receives the format that `detect` found, and `useDrawing` passes it. The reader's
  `load` loads the PDF module for `pdf` and the CAD module otherwise, so `useDrawing(file, [reader])` loads only what the
  file needs. Readers that ignore the argument keep working.
- `ReaderWorker` loads the module of each drawing's format in its worker the first time that format is read, before
  the time limit starts. `ReaderWorkerOptions.pdfWasm` gives the PDF module's location, as `wasm` does for the CAD
  module.
- Without an argument, `initPdfReader` and the worker use `new URL("../wasm/zumen_wasm_pdf_bg.wasm", import.meta.url)`,
  which bundlers copy into their output as they do for the CAD module; it is downloaded only when it is loaded.
- Python, the command line and the MCP server keep one native library with every format.
- The fonts and CMaps stay in the PDF module. Whether a font resolver that gives hayro-interpret one small font for
  every standard font keeps the text (221 KB less), and whether hayro can make its image decoders optional, are
  followed up separately.

## Consequences

- Applications that read CAD drawings only download the same module as before. Applications that read PDF drawings
  download 1.4 MB more with brotli the first time a PDF is opened.
- `@zumenjs/reader` ships two `.wasm` files and exports `./zumen_wasm_pdf_bg.wasm` next to `./zumen_wasm_bg.wasm`.
  An application that serves the files itself passes both locations (`initPdfReader(url)`,
  `ReaderWorkerOptions.pdfWasm`).
- The test that compares `detectDrawing` with Rust compares it with the `detect` of both modules.
- `DrawingReader.load` gains an optional argument in `@zumenjs/core`, and `useDrawing` passes the detected format.
- `check:rust` builds both modules for `wasm32-unknown-unknown`, and `pnpm run notices` records the crates linked into
  each.

## References

- [Decision 0005: browser packages for embedding](0005-browser-packages-for-embedding.md), [decision 0020: PDF drawings
  in Rust](0020-pdf-drawings-in-rust.md)
- [Architecture: the readers in the browser](../architecture.md#the-readers-in-the-browser)
- [hayro-interpret 0.8.0](https://crates.io/crates/hayro-interpret/0.8.0): the features `embed-fonts` and
  `embed-cmaps`, and the dependency on `hayro-syntax` with `images`
