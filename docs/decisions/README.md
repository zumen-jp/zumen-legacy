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

- [0001: Keep ST-Bridge structural models in zumen as a TypeScript package](0001-st-bridge-in-zumen.md) (2026-10-19, Accepted)
- [0002: Divide PDF work with ParaPDF by use: drawings in zumen, documents in ParaPDF](0002-pdf-and-parapdf.md) (2026-10-19, Accepted)
- [0003: Read DWG in Rust only, with our own decoder, into the AutoCAD drawing model of zumen-dxf](0003-dwg-in-rust.md) (2026-10-19, Accepted)
- [0004: Serve drawings to AI applications through an MCP server in the Python package](0004-mcp-server-in-the-python-package.md) (2026-10-20, Accepted)
- [0006: Write drawings through the AutoCAD drawing model, starting with AutoCAD 2013 DXF](0006-write-through-the-autocad-model.md) (2026-10-20, Accepted)
- [0007: Map SXF and JWW into the AutoCAD drawing model in a crate of its own, zumen-convert](0007-mapping-crate-between-formats.md) (2026-10-20, Accepted)
- [0008: Real-size model space, and the sheet as a layout with a viewport for each scale](0008-sheets-and-scales-in-the-autocad-model.md) (2026-10-20, Accepted)
