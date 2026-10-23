# 0029: Let the application own pdf.js, and draw the pdf.js documents it opens

> English reference preserved when Japanese became the primary documentation. See [0029-pdf-js-owned-by-the-application.md](0029-pdf-js-owned-by-the-application.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0020](0020-pdf-drawings-in-rust.md) makes `@zumenjs/pdf` a renderer: it draws the pages of PDF drawings
  with pdf.js through the `UnderlayRenderer` of [decision 0022](0022-underlays-drawn-by-a-region-renderer.md), and the
  entities of a page come from the Rust reader through `@zumenjs/reader`. The application gives the same bytes to
  both.
- pdf.js parses in a Web Worker. Its API (`pdfjs-dist`) and its worker must be the same version, and the API finds the
  worker through `GlobalWorkerOptions` of the copy that the application imports, or through the `worker` option of
  `getDocument`.
- Bundlers load workers in their own ways. Vite gives the URL of a file with `?url`, and webpack 5 builds
  `new Worker(new URL(…, import.meta.url))`, for which pdf.js ships `pdfjs-dist/webpack.mjs`. Other bundlers, and pages
  without one, serve the worker file and set its URL.
- `getDocument` takes what a drawing needs from the application: the URLs of the CMaps (text in CJK fonts that the
  PDF does not embed, common in Japanese drawings), the standard fonts and the image decoders, a password, and
  `isEvalSupported` under a Content Security Policy. Its data files carry their own licenses, among them Liberation
  Sans under GPL-2.0 with the font exception.
- pdf.js changes its API between major versions: 6 removed `PDFDocumentProxy.destroy`, and 5.4 added the `canvas`
  parameter of `render`.
- The packages make no network requests except for their own files, and ship no text for people.

## Options

1. **`@zumenjs/pdf` depends on `pdfjs-dist` and opens the bytes itself**, starting the worker from its own module.
   The worker would load only where the application's bundler follows `new URL(…, import.meta.url)` into a
   dependency, and the application could not choose the legacy build or a worker of its own. An application that also
   uses pdf.js (a document viewer next to the drawing) would have two copies, and setting
   `GlobalWorkerOptions` on its own copy would not reach ours. Every `getDocument` option would have to be passed
   through, and the package would decide when documents are destroyed.
2. **The application passes the pdf.js module and the bytes**, and the package calls `getDocument`. One copy of pdf.js,
   but the options, the lifetime of the documents and errors such as passwords stay with the package.
3. **The application opens the documents and passes them in.** The package calls only methods of a
   `PDFDocumentProxy` (`getPage`, `getOptionalContentConfig`) and of its pages (`getViewport`, `render`), and imports
   only the types of pdf.js.

How content layers reach optional content groups (8.11): the scene has zumen layer ids, and the renderer does not see
the scene's layers. Matching by position fails because a page uses a subset of the groups of the document, and
matching by name needs the scene and fails on groups that share a name. The reader does not tell the group of a layer
yet: [decision 0020](0020-pdf-drawings-in-rust.md) waits for hayro-interpret to report groups.

## Decision

Option 3.

- `pdfUnderlayRenderer({ document, optionalContent, annotations, background })` returns an `UnderlayRenderer`.
  `document(file, underlay)` returns the `PDFDocumentProxy` that holds the page, `file` being undefined for the pages
  of the drawing itself. It is called once per file, and again after it returns `null` or rejects, as
  `imageUnderlayRenderer` does for images. The application closes the documents.
- `pdfjs-dist` is a peer dependency (`^5.0.0 || ^6.0.0`, the majors the tests draw with) used for types only. The
  renderer passes both `canvas` and `canvasContext` to `render`, which every version from 5.0 reads, and the
  `AnnotationMode` values as numbers, so that it does not import pdf.js at run time.
- The worker and the data files are set up by the application, as pdf.js documents them. The README shows Vite (`?url`,
  as the tests do), webpack 5 (`pdfjs-dist/webpack.mjs`) and serving the worker file.
- Each request renders `page.getViewport({ scale })` with the transform `[1, 0, 0, 1, −x × scale, −y × scale]`, as
  [decision 0022](0022-underlays-drawn-by-a-region-renderer.md) describes. A page whose size differs from the
  underlay's `width` × `height` is stretched over it: the vertical factor goes into the viewport and the horizontal
  one, relative to it, into the transform. The request's `signal` cancels the `RenderTask`, also when it is aborted
  while `render` is being called.
- `optionalContent(layer, underlay)` gives the group ids of a content layer. The renderer starts from the document's
  default configuration and sets every mapped group to the visibility of its layer, so groups hidden by default appear
  when their layer is shown; radio-button groups (`/RBGroups`) do not switch each other off, since zumen's layers are
  independent. Without it, pages are drawn with the default visibility.
- Annotations with appearance streams are drawn (pdf.js's default, `annotations: false` turns it off), and the page
  background is white unless `background` says otherwise. The intent is `display`.
- Moving and settling follow [decision 0002](0002-pdf-and-parapdf.md) through the viewer: it asks for the whole page
  coarsely and the visible region sharply once the view settles, and stretches what it has while the view moves. The
  renderer adds no cache of its own beyond the documents; pdf.js keeps the operator lists of the pages.

## Consequences

- An application writes three lines to set up pdf.js and one to open a document, with the options its drawings need,
  and can share the documents with a PDF viewer of its own. A drawing and its PDF underlays can be drawn by one renderer
  that chooses by `kind` between `pdfUnderlayRenderer` and `imageUnderlayRenderer`.
- `@zumenjs/pdf` ships no pdf.js code and fetches nothing. pdf.js fetches its worker and data files from the URLs the
  application sets, and the application decides whether to serve the files under other licenses.
- A new major version of pdf.js is added to the peer range once the tests pass with it.
- Layers of PDF drawings are switched only when the application maps them, until the reader tells which group a layer
  stands for. When it does, the reader can record the group's reference with the layer, and `@zumenjs/pdf` can map
  them without the application; that changes the scene and needs its own change of both implementations.
- When ParaPDF publishes its engine layer, [decision 0002](0002-pdf-and-parapdf.md) asks for a record on whether
  `@zumenjs/pdf` moves to it; with option 3, that is a question of which documents the application passes in.

## References

- ISO 32000-1:2008: 7.7.3.3 (page objects: `CropBox`, `Rotate`, `UserUnit`), 8.11 (optional content), 12.5
  (annotations)
- [pdf.js](https://github.com/mozilla/pdf.js) 6.4.299: `PDFPageProxy.render`, `PDFDocumentProxy.getOptionalContentConfig`,
  `OptionalContentConfig.setVisibility`, `pdfjs-dist/webpack.mjs`
- [Vite: static asset handling](https://vite.dev/guide/assets) (`?url`, `new URL(…, import.meta.url)`)
- [Decision 0002](0002-pdf-and-parapdf.md), [decision 0020](0020-pdf-drawings-in-rust.md),
  [decision 0022](0022-underlays-drawn-by-a-region-renderer.md)
