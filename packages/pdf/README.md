# @zumenjs/pdf

Draws PDF pages under a [zumen](https://github.com/zumenjs/zumen) drawing with [pdf.js](https://github.com/mozilla/pdf.js):
the pages of PDF drawings, and the PDF underlays that DXF drawings name. It is an `UnderlayRenderer` for
`DrawingViewer` of [`@zumenjs/viewer`](../viewer), and draws pixels only. What is hit, snapped to, measured and listed
on a page (its paths and texts, as entities that are not drawn) comes from the reader that read the scene, so the
application gives the same bytes to both ([decision 0020](../../docs/decisions/0020-pdf-drawings-in-rust.md)).

```ts
import { readDrawing } from "@zumenjs/core";
import { pdfUnderlayRenderer } from "@zumenjs/pdf";
import { initPdfReader, reader } from "@zumenjs/reader";
import { DrawingViewer } from "@zumenjs/viewer";
import { GlobalWorkerOptions, getDocument } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url"; // Vite; see "pdf.js and bundlers"

GlobalWorkerOptions.workerSrc = workerUrl;

const bytes = new Uint8Array(await file.arrayBuffer());
await initPdfReader();
const scene = readDrawing(bytes, [reader]);
// pdf.js moves the bytes it is given to its worker, so give it a copy
const pdf = getDocument({ data: bytes.slice() });
const viewer = new DrawingViewer(element, scene, {
  // file is undefined for the pages of the drawing itself
  underlayRenderer: pdfUnderlayRenderer({ document: (file) => (file === undefined ? pdf.promise : null) }),
});

// When the drawing is closed
viewer.destroy();
await pdf.destroy();
```

The pages of PDF drawings reach the scene through `@zumenjs/reader`, which reads PDF with a WebAssembly module of its
own ([decision 0030](../../docs/decisions/0030-pdf-in-a-webassembly-module-of-its-own.md)); each page is a view of the
scene with the page as its underlay. The playground's
[PDF sample](https://github.com/zumenjs/zumen/blob/main/playground/pdf-sample.ts) puts it together: it opens the
bytes with pdf.js only when they are a PDF (`detectDrawing`), importing pdf.js at that moment, and its
`vite.config.ts` serves the CMaps and image decoders of pdf.js.

## What it draws

- Underlays of `kind: "pdf"`. `page` counts from 1. Without `file` the page belongs to the drawing's own file; with
  `file`, to the file the drawing names (a DXF PDF underlay), which the application finds and opens.
- The content coordinates are the points of the page as it is displayed: turned by `/Rotate`, with the origin at the
  top left of the crop box and y down, which is pdf.js's `page.getViewport({ scale: 1 })`. A page of another size than
  the underlay's `width` × `height` is stretched over it.
- Each request draws `request.region` at `request.scale` into the canvas the viewer gives: pdf.js renders
  `page.getViewport({ scale })` with the transform `[1, 0, 0, 1, −x × scale, −y × scale]`, so a detail at a high zoom
  never renders the whole page. When the request's `signal` is aborted, the `RenderTask` is cancelled.
- The viewer asks for the whole page coarsely (up to 2048 × 2048 pixels) and, when that is not sharp enough, for the
  visible region at the device resolution, once the view has stood still for 120 ms. While the view moves it stretches
  what it has and aborts the requests that no longer fit, so moving stays fast and the page sharpens when it stops.

## Options

| Option | Default | What it does |
| --- | --- | --- |
| `document(file, underlay)` | Required | Returns the pdf.js document (`PDFDocumentProxy`, or a promise of it) that holds the page. Called once per `file`; returning `null` or rejecting fails the request, and the next request calls it again. The application opens the documents and closes them when they are no longer drawn. |
| `optionalContent(layer, underlay)` | None | The ids of the optional content groups (pdf.js ids such as `"12R"`) that a layer of `underlay.contentLayers` turns on and off. Hidden layers turn their groups off and visible layers turn them on, including groups that the document hides by default. Without it, pages are drawn with the document's default visibility. |
| `annotations` | `true` | Draws the appearance of annotations (stamps, comments, form fields), as pdf.js does by default. `false` draws the page content only. |
| `background` | white | The fill under the page, any canvas `fillStyle`. |

### Layers from optional content

The viewer sends the hidden layers among `contentLayers` with each request and asks again when one of them is shown or
hidden. The scene does not say which optional content group a layer stands for, so `optionalContent` gives it; matching
the names of the scene's layers with the groups of the document, for example:

```ts
const opened = await pdf.promise;
const config = await opened.getOptionalContentConfig();
const groups = new Map([...config].map(([id, group]) => [group.name, id]));
const names = new Map(scene.layers.map((layer) => [layer.id, layer.name]));
const renderer = pdfUnderlayRenderer({
  document: (file) => (file === undefined ? opened : null),
  optionalContent: (layer) => groups.get(names.get(layer) ?? ""),
});
```

zumen's PDF reader gives one layer per page until it can tell which group an entity belongs to
([decision 0020](../../docs/decisions/0020-pdf-drawings-in-rust.md)); until then pages have no content layers and are
drawn with the default visibility of the document.

### Images and PDF pages together

A drawing can have both (DXF IMAGE and PDF underlays). Pass one renderer that chooses by kind:

```ts
import { imageUnderlayRenderer } from "@zumenjs/viewer";

const pages = pdfUnderlayRenderer({ document: openPdf });
const images = imageUnderlayRenderer(loadImage);
viewer.setUnderlayRenderer((request, target) => (request.underlay.kind === "pdf" ? pages : images)(request, target));
```

## pdf.js and bundlers

Install `pdfjs-dist` (5 or 6) next to this package. The application owns pdf.js: it starts the worker, opens the
documents with the options it needs and closes them. This package imports only pdf.js's types and calls the methods of
the documents it is given, so an application has one copy of pdf.js, whose worker always matches it.

- **Vite:** import the worker's URL with `?url` and set `GlobalWorkerOptions.workerSrc`, as in the example above.
- **webpack 5:** import from `pdfjs-dist/webpack.mjs` instead of `pdfjs-dist`; pdf.js ships that entry, which starts its
  worker with `new Worker(new URL(…, import.meta.url))`.
- **Elsewhere:** serve `pdfjs-dist/build/pdf.worker.min.mjs` with your files and set `GlobalWorkerOptions.workerSrc` to
  its URL.

pdf.js reads some files from URLs passed to `getDocument`: `cMapUrl` (the `cmaps` folder, for text in CJK fonts that
the PDF does not embed, common in Japanese drawings), `standardFontDataUrl` (`standard_fonts`, for the standard 14 fonts
when they are not embedded) and `wasmUrl` (`wasm`, the JPEG 2000 and JBIG2 decoders of scanned drawings). Copy those
folders of `pdfjs-dist` into your build and pass their URLs; each folder carries the licenses of its files. Under a
Content Security Policy without `'unsafe-eval'`, pass `isEvalSupported: false`.

The default build of pdf.js needs current browsers; `pdfjs-dist/legacy/build/pdf.mjs` and its worker serve older ones.
The tests draw pages with pdf.js 5.0 and 6.4 in Chromium, Firefox and WebKit.

## Network

This package makes no requests. pdf.js fetches its worker and the files above from the URLs the application sets, and
the PDF itself only when `getDocument` is given a `url` instead of bytes.
