# 0011: Give AI agents the functions behind the MCP tools as `zumen.tools`, with a documented schema

> English reference preserved when Japanese became the primary documentation. See [0011-tool-functions-for-agents.md](0011-tool-functions-for-agents.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

- Applications that build an AI agent give it tools as functions of their own: agent frameworks call Python
  functions, and MCP is one way among others. [Decision 0004](0004-mcp-server-in-the-python-package.md) made the
  functions behind the MCP tools importable from `zumen.mcp`, but did not promise their results: their keys could
  change with the server, and they lived in a module named after a protocol the application may not use.
- [Decision 0009](0009-geometry-for-ai-clients.md) gives the shapes of entities in paper and real-world millimeters,
  so that an agent can fetch the facts of a drawing and interpret them itself. An application that builds its tools on
  these functions needs to know their results exactly: which keys, which are optional, and how large a result can get.
- Agent frameworks limit what one tool call may return, often to a few tens of thousands of characters. Shapes are
  bounded by `max_points` and lists by `limit`, but texts were not: a note on a drawing can be thousands of characters
  and appears in a text, a property and a value at once.
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
  `compare_revisions` and `Drawings` (reading files from allowed directories within limits; several threads may share
  one). `zumen.mcp` imports them and keeps their names.
- Results are dicts, lists, strings, numbers, booleans and `None`, which `json.dumps` writes as they are (values that
  were tuples come as lists), described by `TypedDict` classes of the module. A later version may add keys, but does
  not remove or rename them or change what they mean; the tests check every result of every fixture format against
  the classes.
- Every result is bounded by its arguments: lists by `limit` (at most 1,000) with the `total` and `next_offset`, shapes
  by `max_points` as decision 0009 set, images by 4,096 pixels on the long edge, the layers of `summary` by
  `layer_limit`, and every string from the drawing (texts, property values, values, text shapes, metadata) by
  `max_text_length` characters, 1,000 by default, with `truncated: true` on the item that was cut.
- The frame is the scene's `bounds`. Every function that takes `region` (paper millimeters) also takes
  `frame_region` (fractions of the frame, origin at the top left), `render` returns the region shown both ways, and
  `paper_region` and `frame_region_of` convert.
- The functions add no analysis: they report what the drawing holds.

## Consequences

- Removing or renaming a key of a result is a breaking change, like changing the Python API.
- Results gain keys next to those of the MCP server before: `kinds` and `next_offset` on every page, `layers_total`
  and `metadata_values` in `summary`, `frame_region` in `render`, and `truncated` where a string was cut. Values that
  were tuples are lists.
- MCP clients get `frame_region` on the tools that take a region.

## References

- [0004: Serve drawings to AI applications through an MCP server in the Python package](0004-mcp-server-in-the-python-package.md)
- [0009: Give AI clients the shapes drawn, in paper and real-world millimeters](0009-geometry-for-ai-clients.md)
- [The architecture notes](../architecture.md), "Texts, entities and images for servers"
- [Python bindings](../../bindings/python/README.md#tools-for-ai-agents)
- pydantic, [TypeAdapter](https://docs.pydantic.dev/latest/concepts/type_adapter/), which builds a JSON Schema from a
  `TypedDict`
