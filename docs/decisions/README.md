# Design decisions

Each record explains one choice between alternatives: what we needed, the options we compared, what we chose and why,
and what follows from it. Records are named `NNNN-<subject>.md`, numbered in order. They are not rewritten after the
decision, except to fix links; a later record that changes a decision links to the one it replaces.

Choices made before this folder existed are described in [the architecture notes](../architecture.md).

## Template

```markdown
# NNNN: <the decision, as a short sentence>

| Item | Value |
| --- | --- |
| Date | YYYY-MM-DD |
| Status | Proposed / Accepted / Superseded by NNNN |

## Context

## Options

## Decision

## Consequences

## References
```

## Records

- [0001: Keep ST-Bridge structural models in zumen as a TypeScript package](0001-st-bridge-in-zumen.md) (2026-10-19, Superseded by 0013)
- [0002: Divide PDF work with ParaPDF by use: drawings in zumen, documents in ParaPDF](0002-pdf-and-parapdf.md) (2026-10-19, Accepted)
- [0003: Read DWG in Rust only, with our own decoder, into the AutoCAD drawing model of zumen-dxf](0003-dwg-in-rust.md) (2026-10-19, Accepted)
- [0004: Serve drawings to AI applications through an MCP server in the Python package](0004-mcp-server-in-the-python-package.md) (2026-10-20, Accepted)
- [0005: Make the browser packages an engine to embed, with reading in Rust and the UI as a sample](0005-browser-packages-for-embedding.md) (2026-10-20, Proposed)
- [0006: Write drawings through the AutoCAD drawing model, starting with AutoCAD 2013 DXF](0006-write-through-the-autocad-model.md) (2026-10-20, Accepted)
- [0007: Map SXF and JWW into the AutoCAD drawing model in a crate of its own, zumen-convert](0007-mapping-crate-between-formats.md) (2026-10-20, Accepted)
- [0008: Real-size model space, and the sheet as a layout with a viewport for each scale](0008-sheets-and-scales-in-the-autocad-model.md) (2026-10-20, Accepted)
- [0009: Give AI clients the shapes drawn, in paper and real-world millimeters, and leave their meaning to them](0009-geometry-for-ai-clients.md) (2026-10-20, Accepted)
- [0010: Write SXF from the AutoCAD drawing model, with viewports as sub-figures and drawn shapes for what SXF lacks](0010-write-sxf-from-the-autocad-model.md) (2026-10-20, Accepted)
- [0011: Give AI agents the functions behind the MCP tools as `zumen.tools`, with a documented schema](0011-tool-functions-for-agents.md) (2026-10-20, Accepted)
- [0012: Edit drawings in place, keeping everything an operation does not name, and write every format zumen reads](0012-edit-in-place-and-write-every-format.md) (2026-10-20, Accepted)
- [0013: Keep zumen free of formats that need permission and of non-permissive dependencies, and move ST-Bridge out](0013-st-bridge-out-of-zumen.md) (2026-10-20, Accepted)
- [0014: Verify native Windows builds and CAD interoperability](0014-verify-on-native-windows.md) (2026-10-20, Accepted)
- [0015: Analyze drawings in zumen-core from the scene: closed regions first, then tables, then checks against CAD standards](0015-analysis-in-zumen-core.md) (2026-10-21, Accepted)
- [0016: Send scene JSON from the reader worker and decode it on the page](0016-send-worker-scenes-as-json.md) (2026-10-21, Accepted)
- [0017: Let edits add block definitions, dimensions, styles, hatches, text options, attributes, sheets and leaders to DXF and DWG](0017-add-annotations-and-definitions.md) (2026-10-22, Accepted)
- [0018: Draw complete sheets from a declarative drawing spec, in a drafting kit that builds the AutoCAD drawing model](0018-drafting-kit.md) (2026-10-22, Proposed)
- [0019: Keep the number of a text height between DXF, DWG, SXF and JWW, and let each format draw its glyphs](0019-text-heights-between-formats.md) (2026-10-23, Accepted)
- [0020: Read the paths and texts of PDF drawings in Rust only, with hayro-interpret, and leave the pixels to a PDF engine](0020-pdf-drawings-in-rust.md) (2026-10-23, Proposed)
- [0021: Keep drawing markup in a JSON document beside the drawing, in paper millimeters of a view, and carry it to new revisions with compare](0021-drawing-markup.md) (2026-10-23, Proposed)
- [0022: Draw raster underlays through a region renderer that the application passes in, and keep the entities they show undrawn](0022-underlays-drawn-by-a-region-renderer.md) (2026-10-23, Proposed)
- [0023: Resize existing layouts through an edit operation](0023-resize-existing-layouts.md) (2026-10-23, Accepted)
- [0024: Keep the declared text height with each text primitive](0024-declared-text-height.md) (2026-10-23, Accepted)
- [0025: Add block definitions, dimensions and hatches to JWW and SXF edits as each format's own entities](0025-annotations-in-jww-and-sxf-edits.md) (2026-10-23, Proposed)
- [0028: Read IFC building models with our own reader in Rust, on the ISO 10303-21 syntax shared with SXF, and keep ST-Bridge out until buildingSMART Japan agrees](0028-ifc-in-rust.md) (2026-10-23, Proposed)
