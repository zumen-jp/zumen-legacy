# 0006: Write drawings through the AutoCAD drawing model, starting with AutoCAD 2013 DXF

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

zumen reads SXF, JWW, DXF and DWG. Users also need to pass drawings on in another form: a DWG received from a client
as a DXF that other programs open, an R12 DXF in Shift_JIS as UTF-8, and later SXF for public works and JWW for Jw_cad.
Every output needs a model to write from. That choice decides how much of a drawing survives a conversion, and how
many converters there are to keep and test.

## Options

1. **Write from the `Scene`.** Every reader produces it, so every format could be written at once. But the scene is a
   display model: curves are divided into segments, texts are laid out character by character, blocks are expanded,
   dimensions are their drawn lines, and the tables are reduced to what the viewer needs. A DXF written from it would
   draw the same and edit like an exploded drawing.
2. **One converter per pair of formats** (DWG to DXF, SXF to DXF, DXF to SFC …), each keeping what both sides share.
   It keeps the most for each pair, but the converters grow with the square of the formats, each with its own rules.
3. **One drawing model in the middle: the AutoCAD drawing model (`DxfDocument` of `zumen-dxf`).** DXF and DWG already
   read into it ([decision 0003](0003-dwg-in-rust.md)). SXF and JWW would be mapped into it, and every output written
   from it.

For the first output, the DXF version:

- R12 (AC1009) is read by the most programs, but it has no handles, LWPOLYLINE, MTEXT, HATCH, MULTILEADER or layouts
  of today's kind, so most of a current drawing would be exploded, and its text needs a code page.
- AutoCAD 2018 (AC1032) is the newest version read, and older programs refuse it.
- AutoCAD 2013 (AC1027) is UTF-8 (since 2007), holds every entity the model holds, and is what ezdxf writes by default.

## Decision

Drawings are written through the AutoCAD drawing model (option 3). The first writer, `zumen_dxf::write_dxf`, writes a
`DxfDocument` as ASCII DXF of AutoCAD 2013 (AC1027) in UTF-8. `zumen::convert(bytes, "dxf", options)` reads a DXF or
DWG drawing into the model and writes it; SXF and JWW return an error until they are mapped into the model.

- A written DXF, read again, draws the same scene in every view as the drawing it came from, with the same entity ids
  (handles are kept). `crates/zumen/tests/convert.rs` checks this on every DXF and DWG fixture, and ezdxf reads every
  written fixture as an independent check.
- What cannot be written as it was is reported as `notices`, as the readers do, and never dropped silently: IMAGE
  (its file is not read), HELIX (as SPLINE), MPOLYGON (as HATCH), ACAD_TABLE (as the INSERT of its block), line types
  with texts or shapes (dashes only), dimensions without their block, attributes outside a block reference (as TEXT),
  and definitions that entities refer to but the drawing lacks (added).
- What the model does not keep is written as AutoCAD and ezdxf write it for a new drawing: the dictionaries, one
  `Standard` multileader style, and default plot settings for layouts that have none.

## Consequences

- One mapping per format instead of one converter per pair: SXF and JWW into the model, and SFC, P21 and JWW writers
  from it.
- The model has to hold what writing needs, not only what drawing needs. Gradient settings (angle, shift, tint) were
  added for this; the file of an image and the multileader styles are not kept yet.
- A conversion keeps only what the model holds: a DXF written from SXF or JWW will have what their mapping gives.
- Objects that the model does not keep (tables, dictionaries, `SEQEND`) get new handles, so writing a written DXF again
  gives the same drawing and the same entity handles, but not the same bytes.

## References

- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm)
  (AutoCAD 2026)
- [ezdxf](https://ezdxf.mozman.at/) 1.4.5: the order of the group codes of MULTILEADER, MLEADERSTYLE and gradients, and
  the audit used in the tests
- [How DXF is handled: Writing](../formats/dxf.md#writing-write_dxf)
