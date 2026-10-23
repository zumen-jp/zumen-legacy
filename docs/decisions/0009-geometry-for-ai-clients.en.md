# 0009: Give AI clients the shapes drawn, in paper and real-world millimeters, and leave their meaning to them

> English reference preserved when Japanese became the primary documentation. See [0009-geometry-for-ai-clients.md](0009-geometry-for-ai-clients.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

- The MCP server of [decision 0004](0004-mcp-server-in-the-python-package.md) gives texts, boxes, images and the
  properties of an entity. Since the stable keys of [decision 0005](0005-browser-packages-for-embedding.md), a
  property also has its unformatted value (a start point, a center, a radius), but in each format's own coordinates
  (a DXF block's own space, drawing units such as inches, an SXF part's own space). Many coordinates cannot be read
  at all: the vertices of a polyline, the outline of a hatch and the lines of a dimension are only counted or not
  given. A client cannot compute the area of an outline, the thickness of a wall, the distance between two lines in
  different blocks, or tell which line a dimension measures, except by guessing from pixels.
- Capable models can interpret raw geometry themselves, deciding what is a wall or a room, once they can get it.
  The server's part is to give the facts of the drawing completely and simply; interpretation in the server would be
  complex, would guess, and would decide for the client what it may ask.
- Every scene already holds what it draws as primitives in paper millimeters, the same coordinates as boxes, regions
  and images, and each entity in a scaled part of the sheet (an SXF sub-figure or part, a JWW layer group, a DXF
  viewport or model space) holds the conversion from paper to the coordinates of that part. Measuring in real-world
  millimeters already goes through it.
- MCP clients bound what a tool may return: Claude Code warns above 10,000 tokens and limits a result to 25,000 by
  default.

## Options

- **Definitions of each format only**, extending the raw values of properties (vertices, bulges, control points,
  hatch boundaries): exact, and short for curves, but every reader in TypeScript and Rust and every golden file
  would change again, and the values stay in each format's own coordinates, which a client cannot relate to each
  other or to the image without the transformations of blocks and parts.
- **The shapes drawn, from the primitives of the scene**: one implementation in `zumen-core` for every format, in
  the coordinates of boxes, regions and images, and through the placement in real-world coordinates. Curves come as
  the polylines drawn.
- **Higher-level shapes recognized by the server** (arcs from points, walls, rooms): short and convenient, but a
  guess, and the interpretation that a capable client does better with the facts in hand.

## Decision

The shapes drawn, next to the definitions that the properties keep:

- `zumen_core::extract::entity_shapes` turns the primitives of an entity into shapes: polylines (points, closed),
  segments (pairs of points), fills (rings), hatches (the rings of the area; the hatch lines are a pattern and are
  left out), points, and text parts (the lower left of the text, its height and angle). Curves are the polylines
  drawn; their center, radius and angles are in the values of the properties. `real_shapes` gives the same shapes through the
  inverse of the entity's placement: the real-world coordinates of JWW, those of the SXF sub-figure or part, and DXF
  model space in millimeters. Entities outside a placement have none.
- Python has them as `Scene.entity(id, geometry=True)` and `Scene.entities(..., geometry=True)`, and the command line
  as `zumen entities --geometry`.
- The MCP server gets no new tool. `get_entity` returns the geometry of the entity, `find_entities` returns it for a
  page with `geometry`, and `open_drawing` lists the placements with their box, scale and `to_real`, the conversion
  from paper to real-world millimeters. `real` is left out where it equals the coordinates on the sheet (DXF model
  space without a shifted origin).
- `max_points` bounds a call: 1,000 points by default and 10,000 at most, a point with `real` counting twice, which
  keeps a page of entities well below the 25,000 tokens a client accepts. Shapes are never cut: those past the
  budget are counted in `geometry_omitted`, and a page of `find_entities` ends before an entity whose shapes would
  pass the budget, with `next_offset` to go on. The first entity of a page always comes, so paging cannot stall.
- `measure_distance` stays. A client without a way to run code would otherwise apply `to_real` by hand.

## Consequences

- Clients compute lengths, areas and distances, and join lines into outlines, from the coordinates; nothing in the
  server decides what a shape means.
- A curve costs points (a full circle is 16 or more), which is the price of one representation for every shape.
  The values of `center` and `radius` give the definition as numbers, in the format's own coordinates.
- `find_entities` with `geometry` builds the shapes of every entity it selects before it pages (about half a second
  for the 25,873 entities of a DWG house plan), so clients should narrow the search with a region, a type or a layer.
- The command line and Python keep `real` even where it equals the coordinates on the sheet: data pipelines read the
  same keys for every entity, and only the MCP server trims for tokens.

## References

- [Texts, entities and images for servers](../architecture.md#texts-entities-and-images-for-servers)
- [MCP server](../../bindings/python/README.md#mcp-server) of the Python package
