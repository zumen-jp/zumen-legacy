# 0001: Divide PDF work with ParaPDF by use: drawings in zumen, documents in ParaPDF

| Item | Value |
| --- | --- |
| Date | 2026-10-19 |
| Status | Accepted |

## Context

- zumen reads CAD drawings (SXF, JWW, DXF, and DWG next) into a format-neutral `Scene` and adds what drawings need on
  top of the picture: layers, scales and real-world lengths, snapping, measurement and notices.
  Item 5 of the roadmap includes PDF drawings.
- Drawings also arrive as PDF: plotted sheets, scans of paper drawings, and PDF underlays that DXF drawings reference.
- No reader draws raster content yet. DXF draws only the frame of an IMAGE and skips PDF, DWF and DGN underlays,
  SXF does not show raster images, and JWW does not show images embedded in text.
- ParaPDF is a React PDF viewer by the same author, not published yet. It aims at speed: rendering off the main thread
  and keeping React out of scrolling and zooming. It wraps existing PDF engines, pdf.js first, instead of writing one,
  and shows a document as a scrolling list of pages with text selection, search, links and accessibility.
- Both are browser viewers that zoom, and a PDF can be either a drawing or a document. Without a rule, any feature
  that touches PDF could land in either project.

## Options

1. zumen converts PDF into a `Scene` and draws it with its own renderer. Embedded fonts, transparency groups, shadings
   and image codecs (JBIG2, CCITT, JPEG 2000) would make zumen a second PDF renderer next to pdf.js.
2. zumen leaves PDF alone. ParaPDF gains a single-sheet mode, and applications build measurement on its overlays.
   Applications would then show CAD and PDF drawings in two viewers with different measurement and markup, and DXF
   underlays would still need a PDF renderer inside zumen's viewer.
3. Divide by use instead of by file format. In zumen's viewer a PDF engine draws the pixels of a PDF drawing and zumen
   adds the drawing semantics, while ParaPDF stays a document viewer without knowledge of drawings.

## Decision

Option 3.

- Drawings belong in zumen: they are opened one sheet at a time, measured, shown layer by layer, marked up and
  compared between revisions. Documents belong in ParaPDF: specifications, reports and contracts are scrolled,
  searched, selected and copied, followed through links and read aloud. The application chooses, because it knows
  which one a file is; in electronic delivery for public works, for example, drawings and reports sit in different
  folders.
- To place a new feature, ask whether people who never treat a PDF as a drawing need it. If they do, it belongs in
  ParaPDF; if not, in zumen.
- zumen does not write a PDF renderer. From a PDF drawing it reads what interaction needs: pages as `Scene.views`, like
  DXF layouts; vector paths and text for snapping and selection, which are not drawn; optional content groups as
  layers; and the scale, from a two-point calibration, the title block or the measure dictionaries of the PDF.
- `@zumenjs/core` gets an underlay: raster content that a renderer passed in draws under the scene for the current
  view. The contract is region rendering with cancellation: draw a region of a page or an image at a scale into a
  canvas, or cancel the request. It is not specific to PDF; images and underlays in DXF, raster images in SXF and
  images embedded in JWW use it too.
- `@zumenjs/pdf` uses pdf.js directly at first. It may move to the engine layer of ParaPDF once ParaPDF publishes that
  layer as a public API. It never depends on the React viewer of ParaPDF.
- Dependencies point one way. zumen may depend on a PDF engine. ParaPDF never depends on zumen and carries no drawing
  vocabulary: no scales, real-world lengths, measuring tools or CAD layers.
- Features that both could claim:
  - Measurement and snapping belong in zumen.
  - Markup is split. PDF annotations read and written as PDF (highlights, ink, comments, forms) belong in ParaPDF.
    Drawing markup (redlines, revision clouds, review findings, photo positions) belongs in zumen and is kept in paper
    millimeters, so CAD and PDF drawings share it.
  - Turning optional content groups on and off is a PDF feature, since PDFs that are not drawings have them too.
    ParaPDF implements it, and zumen asks its PDF engine for it.
  - The viewer runtime is not shared. ParaPDF scrolls a list of pages and zumen pans and zooms one sheet, so a shared
    package would fit neither. Techniques move between the projects instead: a low-resolution base under a
    full-resolution render of the visible area, sharp renders once the view settles, rendering in workers, and no
    React commit per frame of scrolling, zooming or pointer movement.
  - No Rust crate is shared, though both projects may depend on the same crates.
  - Writing drawings as PDF, for printing or conversion, belongs in zumen. ParaPDF does not produce PDF.

## Consequences

- "PDF drawings" in item 5 of the roadmap means drawings that a PDF engine draws, with the snapping, measurement,
  layers and markup of zumen on top.
- The underlay in `@zumenjs/core` comes before `@zumenjs/pdf`. It also lets the DXF, SXF and JWW readers draw the
  raster content they skip today.
- `Scene` needs entities that take part in hit testing and snapping without being drawn, for content that an underlay
  already shows.
- A Rust reader that collects the vectors of a PDF without rendering it, for example through the `Device` trait of
  hayro-interpret (Apache-2.0 OR MIT), would bring PDF drawings to Python and servers. It waits until that need
  appears, after DWG. It would be written in Rust first, against the rule that the TypeScript reader is the reference,
  so it needs a record of its own.
- Whether `DrawingViewer` renders in workers or in tiles is decided by measuring large DWG drawings, not by sharing
  the runtime of ParaPDF.
- When ParaPDF publishes its engine layer, a later record decides whether `@zumenjs/pdf` moves to it.

## References

- [How DXF is handled](../formats/dxf.md): IMAGE and underlays
- [How SXF is handled](../formats/sxf.md) (Japanese): raster images
- [How JWW is handled](../formats/jww.md) (Japanese): images embedded in text
- [Architecture and extension points](../architecture.md)
- ISO 32000-1:2008, 12.9 Measurement properties
- [pdf.js](https://github.com/mozilla/pdf.js)
- [hayro](https://github.com/LaurenzV/hayro)
