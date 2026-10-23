# 0022: Draw raster underlays through a region renderer that the application passes in, and keep the entities they show undrawn

> English reference preserved when Japanese became the primary documentation. See [0022-underlays-drawn-by-a-region-renderer.md](0022-underlays-drawn-by-a-region-renderer.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

[Decision 0002](0002-pdf-and-parapdf.md) gives `@zumenjs/core` an underlay before `@zumenjs/pdf`: raster content that
a renderer passed in draws under the scene for the current view, through region rendering with cancellation. It is
not specific to PDF. The DXF IMAGE and PDF underlays, SXF raster images and JWW images in text use it next
([issue #82](https://github.com/zumenjs/zumen/issues/82)), then the pages of PDF drawings that pdf.js draws
([issue #83](https://github.com/zumenjs/zumen/issues/83)). The same record says that `Scene` needs entities that are hit
and snapped to without being drawn, for the content that an underlay already shows
([issue #81](https://github.com/zumenjs/zumen/issues/81)): the vector paths and texts of a PDF page
([decision 0020](0020-pdf-drawings-in-rust.md)).

What constrains the design:

- `@zumenjs/core` has no DOM types, so it can describe an underlay and the shape of a request, but not a canvas or
  the cancellation of a drawing.
- `Scene` is the same JSON in TypeScript and in `zumen-core`, and the golden files and entity ids of drawings without
  underlays must not change.
- The bytes of the content are not in the drawing: a DXF IMAGE names a file, a PDF drawing's page is drawn by pdf.js,
  and the application decides where files come from. The reader worker sends scenes as JSON
  ([decision 0016](0016-send-worker-scenes-as-json.md)), so large binary content in the scene would cost twice.
- `zumen-render` draws scenes to PNG, SVG and PDF on servers, where there is no PDF engine and no image decoder yet.
- A PDF page at the zoom of a detail is far larger than any canvas, so the content cannot be drawn once and scaled.

## Options

Where the underlay lives in the scene:

1. A primitive of an entity, drawn in the entity's place in the drawing order. Every renderer would meet content it
   cannot draw synchronously inside its loop over primitives, and hit testing would meet primitives without geometry.
2. A list of its own, `Scene.underlays`, drawn in order under all entities.

How it is placed: an insertion point, a size and a rotation, or a matrix from the content's coordinates to paper
millimeters with optional clip rings. The matrix also covers the U and V vectors of a DXF IMAGE (pixels that are not
square, or not at right angles), viewports, and the pages of a PDF, and is the convention of `TextPrimitive.matrix`.

How the content is drawn:

1. The renderer returns the whole content once as an image. Simple, but too large or too coarse for a PDF page.
2. The viewer asks for a region of the content at a scale, into a canvas it gives, and cancels requests that the view
   no longer needs. Cancellation through the platform's `AbortSignal`, or through a handle with `cancel()` as pdf.js
   returns.
3. A pyramid of fixed tiles at powers of two. Good for caching, but more requests, seams between tiles, and a choice
   that [decision 0002](0002-pdf-and-parapdf.md) leaves to measurements of large drawings.

What `zumen-render` does: draw underlays, which needs their bytes and decoders (image crates, a PDF rasterizer), or
report that it skipped them.

How the entities that an underlay shows are kept from being drawn:

1. A flag on the entity, `drawn: false`. Renderers skip it; hit testing, snapping, measurement, extraction and the
   analyses see it like any other entity.
2. A reference to the underlay that shows it, drawn only while that underlay is not. The vectors would stand in for a
   page without a PDF engine, but they are a subset of what the page shows (no images or shadings, text in other
   fonts), so the stand-in would look complete while missing parts, and every renderer would need to know which
   underlays it drew.
3. A list of its own next to `Scene.entities`. Every consumer of entities (hit testing, `texts`, `entities`, regions,
   tables, checks, comparison) would have to read two lists, and ids and order would be split between them.

## Decision

- `Scene.underlays` (`Vec<Underlay>` in Rust, left out of the JSON when empty) lists the underlays drawn in order
  under every entity. An `Underlay` has an `id` (unique in the scene, the same for the same bytes), a `kind` of
  `UNDERLAY_KINDS` (`image`, whose content units are pixels, or `pdf`, whose units are the points of the page as
  displayed), the `file` named in the drawing and the PDF `page` when they apply, its content `width` and `height`, a
  `matrix` from content coordinates (origin at the top left, y down) to paper millimeters, optional `clip` rings in
  paper millimeters (even-odd), an `opacity`, its `layers` with the rule of entities, the `contentLayers` that change
  what the content shows (optional content groups of a PDF), and its `bbox`. It belongs to `Scene.view`: readers
  resolve placements and viewports into the matrix and the clip, and `Scene.bounds` includes it.
- `UnderlayRequest` in `@zumenjs/core` asks for a `region` of the content at a `scale` (pixels per content unit, the
  same in both directions) into an image of `width` × `height` pixels, with the `hiddenLayers` among `contentLayers`.
  It carries the `Underlay`, so a renderer chooses the content by `kind`, `file`, `page` or `id`.
- `@zumenjs/viewer` draws: an `UnderlayRenderer(request, { canvas, context, signal })` draws into the canvas the viewer
  gives, or returns an image of its own (an `ImageBitmap` drawn in a worker), and stops when the `AbortSignal` is
  aborted. A signal rather than a handle with `cancel()`: it is the platform's own, the same signal also cancels the
  `fetch` of an image, and the pdf.js adapter connects it to `RenderTask.cancel()` in one line. The application passes it in
  `DrawingViewerOptions.underlayRenderer` or `setUnderlayRenderer`, calls `refreshUnderlays(ids)` when content
  arrives or changes, and hears about finished requests through the `underlay` event. `imageUnderlayRenderer(load)`
  draws images that the application loads. `renderScene` draws given images through `RenderOptions.underlays`.
- When the view settles (120 ms without a change), the viewer asks for the whole content at up to twice the current
  resolution, within 2048 × 2048 pixels, and, when that cannot be sharp enough, for the visible region with a margin
  at the current device resolution. While the view moves it stretches what it has. A request for a visible region is
  aborted as soon as the view no longer fits it, and every request when the scene, the renderer or the content layers
  change, or the viewer is destroyed. `image` content is not asked for at more than one pixel per pixel. A renderer
  that rejects is not asked again for that underlay until `refreshUnderlays`, `setUnderlayRenderer` or `setScene`.
- `zumen-render` does not draw underlays. `RenderReport.notices` reports `underlay-not-drawn` with the number of
  underlays on visible layers in the rendered region, for PNG, SVG and PDF (`render_svg` returns the report too); the
  command line prints it and Python raises it as a warning. `zumen_core::compare` reports `underlay-not-compared`.
- Entities that an underlay shows have `drawn: false` (left out of the JSON when drawn, `SceneEntity.drawn` in Rust).
  `renderScene` and `zumen-render` skip them, and `drawn_entities` leaves them out; the highlights of the viewer
  (selection, hover, named highlights) still draw them, so that what is selected can be seen over the underlay.
  `HitIndex`, snapping and measurement, `texts`, `entities`, regions, tables, checks and titles use them.
  `diff_scene` draws them, since its scene has no underlays.

## Consequences

- Scenes without underlays or undrawn entities have the same JSON as before, so the golden files and
  `crates/zumen/tests/ids.json` do not change. `packages/core/tests/underlay.scene.json` pins the JSON of both implementations.
- Underlays are always under the entities. A reader whose format draws an image above some entities (the draw order
  of a DXF IMAGE) reports the difference with a notice instead of drawing it in place.
- Readers of [issue #82](https://github.com/zumenjs/zumen/issues/82) give the pixel size of an image from what the
  drawing records (the IMAGEDEF); `imageUnderlayRenderer` stretches an image of another size over the content.
  Kinds that zumen cannot have drawn (DWF and DGN underlays) stay notices until a kind is added to `UNDERLAY_KINDS`.
- `@zumenjs/pdf` of [issue #83](https://github.com/zumenjs/zumen/issues/83) passes an `UnderlayRenderer` that renders
  `page.getViewport({ scale: request.scale })` with the transform `[1, 0, 0, 1, -region.x × scale, -region.y × scale]`,
  and turns `hiddenLayers` into the optional content configuration of pdf.js.
- PNG, SVG and PDF from servers lack underlays until `zumen-render` gets their content: the pages of PDF drawings
  through hayro ([decision 0020](0020-pdf-drawings-in-rust.md),
  [issue #91](https://github.com/zumenjs/zumen/issues/91)), and images once the caller can pass their bytes.
- Whether the viewer moves to tiles or workers is still decided by measurement; a renderer can already draw in a
  worker and return an `ImageBitmap`.

## References

- [Decision 0002: PDF work with ParaPDF](0002-pdf-and-parapdf.md)
- [Issue #80: an underlay for raster content](https://github.com/zumenjs/zumen/issues/80)
- [Issue #81: entities that are hit and snapped to but not drawn](https://github.com/zumenjs/zumen/issues/81)
- [Architecture: the scene model](../architecture.md#the-scene-model)
- [pdf.js `PDFPageProxy.render`](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib-PDFPageProxy.html)
- [DOM Standard: AbortSignal](https://dom.spec.whatwg.org/#interface-AbortSignal)
