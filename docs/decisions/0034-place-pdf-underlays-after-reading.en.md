# 0034: Read the PDF underlays of DXF and DWG unplaced, and place them with the page sizes of the PDF engine that draws them

| Item | Value |
| --- | --- |
| Date | 2026-10-24 |
| Status | Proposed |

## Context

A PDF underlay of DXF and DWG (PDFUNDERLAY,
[UNDERLAY](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-3EC8FBCC-A85A-4B0B-93CD-C6C785959077.htm))
stores where the page goes: its insertion point (the lower left corner of the page), scale, rotation, normal, display
flags and clip boundary. Its definition (PDFDEFINITION) names the file (1) and the page (2). Neither stores the size of
the page. [Decision 0022](0022-underlays-drawn-by-a-region-renderer.md) puts the origin of an underlay's content at its
top left corner, so the matrix needs the page height, and the content size, the extents, `bbox` and `Scene.bounds`
need its width and height. An inverted clip (flag 16 of 280) shows the page outside the boundary, so it needs the
page outline as well. Until now the readers counted PDF underlays in `skipped`
([issue #82](https://github.com/zumenjs/zumen/issues/82)).

What constrains the design:

- Only the PDF file has the size, as it is displayed: after `/Rotate`, within the crop box (pdf.js uses its
  intersection with the media box) and multiplied by `/UserUnit`.
- The application owns pdf.js and opens the documents that `pdfUnderlayRenderer` draws
  ([decision 0029](0029-pdf-js-owned-by-the-application.md)). Readers do not open other files: browsers have no file
  system, the libraries make no network requests, and a format crate depends on `zumen-core` only.
- In browsers the readers run in a worker and send scene JSON ([decision 0016](0016-send-worker-scenes-as-json.md));
  reading again costs as much as the first read, seconds for a large DWG.
- The scene of a drawing is the same in TypeScript and Rust, and the same bytes give the same ids.

## Options

1. A reader option with the page sizes (`file`, `page`, `width`, `height`), read again once the application has
   opened the PDFs. The reader places everything itself (a frame entity, the inverted clip, `Scene.bounds`), but the
   application still needs a first read to learn which files and pages the drawing names, every drawing with a PDF
   underlay is read twice, and the option has to pass through `ReadOptions`, `zumen::read`, the WebAssembly binding,
   the worker protocol and Python.
2. The readers read the page size from PDF bytes that the application passes. They would need a PDF parser
   (`zumen-pdf`, which a format crate may not depend on, and its WebAssembly module in browsers), and would compute
   rotation, boxes and user units a second time, apart from the pdf.js that draws the page.
3. The readers return the underlay unplaced, and a function places it once the application knows the page size. One
   read, and the size comes from the same pdf.js document that draws the page. The reader's output depends only on
   the drawing.
4. The viewer asks the renderer for the content size (a new method of `UnderlayRenderer`) and places the underlays
   itself. No application code, but the viewer would change the geometry of the application's scene (`bbox`,
   `Scene.bounds`), and analyses and servers would see another scene than the viewer.
5. Assume a page size. The page would be misplaced without anything telling the application.

## Decision

Option 3.

- The DXF and DWG readers turn a PDFUNDERLAY that is on (flag 2 of 280) into an underlay of `kind: "pdf"` with the
  `file` of its PDFDEFINITION (1, as the drawing writes it), the `page` (2, from 1), and `width` and `height` 0: an
  underlay whose size is not known yet. Its `matrix` is that of a page of height 0: content point (x, y) of a page of
  height h is at `matrix` · (x, y − h), so the lower left corner of the page, the insertion point, is at
  `matrix` · (0, 0). The content unit is the point of the page as displayed, and one point is 1/72 drawing unit at
  scale 1 (one PDF inch per drawing unit, *inferred*, see [DXF: PDF underlays](../formats/dxf.md#pdf-underlays)).
- Its `clip` rings are in paper millimeters as for images: the clip boundary, viewports. An empty ring stands for the
  outline of the page, which is not known yet: the inverted clip outside viewports is the empty ring and the boundary,
  and a page without a boundary is clipped by the rectangle of the viewport it is seen through. Its `bbox` is the box
  of the clip when the clip bounds the page (a boundary, or a viewport), and the insertion point otherwise. It has no
  frame entity, as the frame of a page whose size is not known cannot be drawn.
- `placeUnderlays(scene, size)` of `@zumenjs/core` places the underlays whose `width` and `height` are 0 and for which
  `size(underlay)` returns a size: it sets `width` and `height`, moves the matrix by the height along the content's
  y axis, puts the outline of the content in the empty rings of `clip`, sets `bbox` to the box of the content within
  the box of the clip, and extends `Scene.bounds`; an underlay whose content lies outside its clip is left out. It
  returns a new scene that keeps the other underlays as they are, or the same scene when it placed nothing. It knows nothing of DXF: any underlay anchored at the lower left corner of
  its content can use it.
- `placePdfUnderlays(scene, { document })` of `@zumenjs/pdf` takes each page size from pdf.js
  (`page.getViewport({ scale: 1 })`), through the same `document` function as `pdfUnderlayRenderer`, so that the
  placement and the drawing agree on rotation, boxes and user units. Underlays whose document or page cannot be
  opened stay unplaced.
- Renderers draw nothing for underlays without a size: the viewer already skips them, and `zumen-render` counts them
  in `underlay-not-drawn`.

## Consequences

- Drawings without PDF underlays keep their scenes, golden files and ids. Drawings with PDF underlays gain an underlay,
  `skipped` no longer counts PDFUNDERLAY, and no entity is added, so the ids of their entities do not change.
- An application that draws PDF underlays opens the files that `underlay.file` names, calls `placePdfUnderlays` after
  reading and gives the placed scene to the viewer. Until then the page is not drawn and `Scene.bounds` does not
  contain it. The playground does this with PDF files that the user picks next to the drawing.
- The frame (PDFFRAME), the underlay layers (optional content that the drawing turns on and off) and the display
  values that AutoCAD applies (contrast, monochrome, colors adjusted for the background) are not drawn and are
  reported. Selecting a PDF underlay needs an entity, which can follow once a frame can be placed with its page.
- Python, the command line and `zumen-render` see unplaced underlays. When `zumen-render` takes the bytes of the PDF
  files that a drawing names, it gets the page size from hayro and places them with the same rule, in `zumen-core`.
- The writers from the drawing model (DXF, DWG, and the conversion to SXF and JWW) do not write PDF underlays, which
  they lost at reading before, and report them. Edits in place keep them, as they keep every entity they do not touch.

## References

- [Decision 0022: Draw raster underlays through a region renderer](0022-underlays-drawn-by-a-region-renderer.md)
- [Decision 0029: Let the application own pdf.js](0029-pdf-js-owned-by-the-application.md)
- [Issue #82: draw the raster images that the readers skip](https://github.com/zumenjs/zumen/issues/82)
- [DXF: PDF underlays](../formats/dxf.md#pdf-underlays)
- [AutoCAD DXF Reference: UNDERLAY](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-3EC8FBCC-A85A-4B0B-93CD-C6C785959077.htm),
  [UNDERLAYDEFINITION](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-A4FF15D3-F745-4E1F-94D4-1DC3DF297B0F.htm)
- [pdf.js `PDFPageProxy.getViewport`](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib-PDFPageProxy.html)
