# 0009: Give AI agents the functions behind the MCP tools as `zumen.tools`, with a documented schema

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Proposed |

## Context

- Applications that build an AI agent give it tools as functions of their own (agent frameworks call Python
  functions; MCP is one way among others). [Decision 0004](0004-mcp-server-in-the-python-package.md) made the
  functions behind the MCP tools importable from `zumen.mcp`, but did not promise their results: their keys could
  change with the server, and they lived in a module named after a protocol the application does not use.
- An agent should be able to fetch every fact a drawing holds, as numbers with names that do not change, and do the
  interpreting itself. The scene has the shapes of every entity as drawn (`primitives`, paper millimeters), the numbers
  of the file by key (`Property.raw`, see "Stable keys" in the architecture notes) and the scale of each placement, but
  the Python results had no shapes.
- Agent frameworks limit what one tool call may return, often to a few tens of thousands of characters, so every
  result has to be bounded by its arguments, including texts from the drawing (notes can be thousands of characters)
  and shapes (a hatch can have thousands of lines).
- Applications give regions of PDF pages and images as fractions of the page, `[left, top, right, bottom]` from the
  top left. Drawings are in paper millimeters with the Y axis up, and DXF model space has no sheet.

## Options

- **Where the functions live**: keep them in `zumen.mcp` and document their results there, or a module of their own
  that `zumen.mcp` wraps.
- **How the results are described**: a JSON Schema file, pydantic models (a runtime dependency, and objects rather than
  plain data), or `TypedDict` classes (no dependency; type checkers check the callers, and on Python 3.12 and later
  pydantic's `TypeAdapter` derives a JSON Schema from them when an application needs one).
- **The frame for fractions**: the sheet, which not every drawing has and which entities may stick out of, or the
  scene's `bounds`, the box around the sheet and every entity, which every drawing with content has and which a render
  without a region already shows.

## Decision

- `zumen.tools` holds `summary`, `render`, `find_texts`, `find_entities`, `get_entity`, `measure`,
  `compare_revisions` and `Drawings` (reading files from allowed directories within limits). `zumen.mcp` imports them
  and keeps their names there.
- Results are dicts, lists, strings, numbers, booleans and `None`, which `json.dumps` writes as they are, described by
  `TypedDict` classes of the module. A later version may add keys, but does not remove or rename them or change what
  they mean; the tests check every result against the classes.
- Every result is bounded by its arguments: lists stop at `limit` (at most 1,000) with the `total`, strings from the
  drawing at `max_text_length` characters (1,000 by default) with `truncated: true`, geometry at `max_points` points
  for the call (1,000 by default, about 20,000 characters, and at most 100,000) with the full count, and images at
  4,096 pixels on the long edge.
- The frame is the scene's `bounds`. Every function that takes `region` (paper millimeters) also takes
  `frame_region` (fractions of the frame, origin at the top left), and `paper_region` and `frame_region_of` convert.
- `get_entity`, and `find_entities` when asked, return the geometry as drawn, in paper millimeters: lines through
  points, filled areas, clipped hatch lines, point markers, and text runs with their position, height, length and
  angle instead of the matrix and glyph cells of the scene. The numbers of the file stay in `values`, and the
  placement's `inverse` is returned so that paper coordinates can be turned into real-world millimeters.
- The functions add no analysis: they report what the drawing holds.

## Consequences

- Removing or renaming a key of a result is a breaking change, like changing the Python API.
- Curves are drawn as many short lines, so their exact radii and angles come from `values`, where the format gives
  them. Keys that the scene does not have yet (the vertices and bulges of polylines, the insertion point and rotation
  of DXF texts, block insertions, the definition points of dimensions) are not in the results until the readers add
  them.
- Agents receive more data per entity than before; the default caps keep one call to about 25,000 characters for
  usual entities and pages.
- MCP clients get `frame_region` on the tools that take a region, `geometry` and `max_points` on `find_entities`, and
  `max_points` on `get_entity`.

## References

- [0004: Serve drawings to AI applications through an MCP server in the Python package](0004-mcp-server-in-the-python-package.md)
- [The architecture notes](../architecture.md), "Stable keys" and "Texts, entities and images for servers"
- [Python bindings](../../bindings/python/README.md#tools-for-ai-agents)
- pydantic, [TypeAdapter](https://docs.pydantic.dev/latest/concepts/type_adapter/), which builds a JSON Schema from a
  `TypedDict`
