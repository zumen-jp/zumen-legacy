# Changelog

All packages share one version during 0.x.

## Unreleased

- Browsers: a new package, `@zumenjs/pdf`, draws PDF pages under the drawing with pdf.js. `pdfUnderlayRenderer` is an
  `UnderlayRenderer` for underlays of kind `pdf` (the pages of PDF drawings, and the PDF underlays of DXF drawings):
  it renders the requested region of a page at the requested scale, cancels the pdf.js render when the request is
  aborted, and turns optional content groups on and off as content layers are shown and hidden (`optionalContent`).
  The application opens the documents with its own pdf.js (`pdfjs-dist` 5 or 6, a peer dependency) and passes them in
  ([decision 0029](docs/decisions/0029-pdf-js-owned-by-the-application.md)). It reads no entities; those come from
  the reader.
