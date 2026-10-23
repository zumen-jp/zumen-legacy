# 0023: Resize existing layouts through an edit operation

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Accepted |

## Context

The drawing spec of [decision 0018](0018-drafting-kit.md) lets `draft_into` explicitly choose a sheet's paper size.
When the sheet names an existing layout, the drafting kit used that size for the frame and other items but left the
template's actual paper size unchanged. The drawing could therefore extend beyond its printable sheet.

## Options

1. Ignore the requested size, or reject it when it differs from the template. This would prevent callers from
   resizing a sheet with the same spec they use to draw it.
2. Rebuild the template with the new size. This would discard information that in-place editing promises to keep.
3. Add a narrow layout edit and use it when the spec explicitly supplies `paper`.

## Decision

Choose option 3. `update-layout` takes a layout `name` and `set: {"paperSize": [width, height]}` in millimeters.
DXF and DWG editors update the paper layout's dimensions, paper name, limits and whole-sheet viewport, preserving
its handle, margins and unrelated plot settings. Model space cannot be resized this way. JWW and SXF reject the
operation because they have no AutoCAD layouts.

`draft_into` emits this operation only for an existing sheet whose spec explicitly supplies `paper`. An omitted
`paper` continues to inherit the template's size without changing the layout.

## Consequences

The existing edit transaction still checks every operation before returning bytes. A refused resize is mapped to
the spec's `sheets[n].paper` path. Callers can also resize an existing DXF or DWG layout directly through `edit`.

## References

- [Decision 0012: In-place editing](0012-edit-in-place-and-write-every-format.md)
- [Drawing into a template](../drafting.md#drawing-into-a-template)
- DXF Reference, `LAYOUT` and `VIEWPORT` group codes.
- Open Design Specification for .dwg files, sections 20.4.84 (LAYOUT) and 20.4.37 (VIEWPORT).
