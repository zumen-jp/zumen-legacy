# 0004: Serve drawings to AI applications through an MCP server in the Python package

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

- AI applications such as coding agents and desktop assistants reach local tools through the Model Context Protocol
  (MCP). They cannot read SXF, JWW or DXF themselves, and a whole sheet as one image is too small to read: an A1 sheet
  in 1,568 pixels shows 2.5 mm text at about 5 pixels.
- zumen reads these formats in Rust, with Python bindings, and can render a region of a sheet at a chosen size, list
  the texts and entities of a drawing with their positions, and tell which entity is drawn where in an image. These are
  the parts an AI application needs: look at the sheet, zoom into a region, search the texts, inspect an entity and
  measure in real-world units.
- The official MCP SDK for Python (`mcp`, MIT) reached 2.0 in July 2026; 2.3.0 is current. Its 1.x API (`FastMCP`)
  was renamed (`MCPServer`) and its transport options moved, so a server pins the major version. FastMCP by Prefect is
  a separate framework with server composition, proxies and OpenAPI import.

## Options

- **A separate repository or package** for the server: stars, issues and releases would be split, and the server
  could not use zumen before zumen is published.
- **A server in Rust or TypeScript**: Rust would need its own MCP implementation and Python bindings already exist;
  a TypeScript server would use the TypeScript readers, while DWG will exist only in Rust.
- **A module of the Python package** (`zumen.mcp`) on the official SDK, as an optional dependency, with the plain
  functions behind the tools importable by applications that call them without MCP.

## Decision

The third option, with these rules:

- `pip install "zumen[mcp]"` adds `mcp>=2.3,<3`; `zumen-mcp` (or `python -m zumen.mcp`) serves over stdio. Reading
  drawings does not need the SDK.
- The tools only read: `open_drawing`, `render_drawing`, `find_texts`, `find_entities`, `get_entity` and
  `measure_distance`. They take a file path and an optional view, and keep the scenes of the last eight files read.
- Results are paper millimeters with the Y axis up, as `Scene` is, rounded to 0.001, sent as compact JSON with the same
  data as structured content. `render_drawing` also sends the PNG and the conversion between pixels and millimeters.
- Received drawings are not trusted. `--root` limits the directories that may be read (symbolic links are resolved
  first), `--max-bytes` limits the file size (256 MiB by default), images are at most 4,096 pixels on the long edge,
  and one call returns at most 1,000 items. The instructions and tool descriptions tell clients that texts, names and
  properties come from the file and are data, not instructions.
- The independent FastMCP framework is not used: the server needs only tools over stdio.
- No HTTP transport. A server shared over a network needs authentication and per-user permissions, which belong to the
  application that hosts it.

## Consequences

- Applications that do not use MCP call `zumen.mcp.summary`, `render`, `find_texts`, `find_entities`, `get_entity` and
  `measure` directly and get the same results.
- New features become tools as they land, starting with the comparison of two revisions.
- Reading time is bounded only by the readers' own limits on nesting and entity counts. A deadline per call would need
  support inside the readers.
- A new major version of the SDK needs a review of the server before the upper bound moves.

## References

- [Model Context Protocol specification](https://modelcontextprotocol.io/specification/2026-07-28)
- [MCP Python SDK](https://github.com/modelcontextprotocol/python-sdk), version 2.3.0, and its
  [migration guide](https://py.sdk.modelcontextprotocol.io/migration/)
- [Texts, entities and images for servers](../architecture.md#texts-entities-and-images-for-servers)
