# 0031: Measure images whose drawing records no pixel count in hundredths of a sheet millimeter

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

[Decision 0022](0022-underlays-drawn-by-a-region-renderer.md) gives `image` underlays content coordinates in pixels,
as many as the drawing records (a DXF IMAGE's size), and the viewer does not ask for image content at more than one
pixel per content unit: a finer request would not add detail. `imageUnderlayRenderer` stretches the decoded image over
the content, whatever its size.

The images of SXF and JWW ([issue #82](https://github.com/zumenjs/zumen/issues/82)) record no pixel count. An SXF image
is a drawing group named after its file, placed over the polyline that bounds it (OCF implementation rules, 18
「画像」). A JWW image is a text `^@BM<file>,<width>,<height>,…` with its size in sheet millimeters. Both record only
the placed size. The image is a separate file (SXF, and JWW images linked by path) or a compressed entry after the
block definitions (JWW images embedded in version 7.00 and later). The readers do not open either.

## Options

1. Use sheet millimeters as the content unit. The viewer would ask for at most one pixel per millimeter, which blurs a
   scanned map at 400 dpi (16 pixels per millimeter) as soon as the view zooms in.
2. Use millimeters and mark the underlay as having no pixel count (a new field), so that the viewer stops limiting
   the scale. This changes the scene JSON of `@zumenjs/core` and `zumen-core`, the viewer, and what every renderer
   has to understand about `width` and `height`.
3. Use a unit fine enough never to limit the drawing: a hundredth of a sheet millimeter (2,540 dpi). Renderers need to
   know nothing new, since `imageUnderlayRenderer` already stretches the image over the content and the viewer bounds
   every request by the device resolution of the visible region.
4. Read the pixel count from the image file. The readers would need the files, which the application supplies only
   to renderers, and decoders for TIFF, BMP and JPEG headers.

## Decision

Option 3. For image underlays of formats that do not record a pixel count (SXF and JWW), `width` and `height` are the
placed size in hundredths of a millimeter of the sheet, and `matrix` maps that content to the sheet. Formats that
record a pixel count (DXF and DWG) keep pixels.

## Consequences

- The scene JSON and the viewer do not change. `width` and `height` are not pixel counts for SXF and JWW images:
  an application that shows an image's pixel size decodes the image itself.
- When zoomed in beyond the image's own resolution, the viewer asks for regions finer than its pixels. They are
  bounded by the screen like any other request, and a renderer may draw them by scaling the image up.
- A format that later records a pixel count uses it, as DXF does.

## References

- [Decision 0022: Draw raster underlays through a region renderer](0022-underlays-drawn-by-a-region-renderer.md)
- [Issue #82: draw the raster images that the readers skip](https://github.com/zumenjs/zumen/issues/82)
- [SXF: Images](../formats/sxf.md#images), [JWW: Images](../formats/jww.md#images)
