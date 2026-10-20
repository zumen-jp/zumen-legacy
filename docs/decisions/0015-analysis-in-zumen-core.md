# 0015: Analyze drawings in zumen-core from the scene: closed regions first, then tables, then checks against CAD standards

| Item | Value |
| --- | --- |
| Date | 2026-10-21 |
| Status | Proposed |

## Context

- The next stage of the roadmap is analysis: rooms and areas, tables, and checks against CAD standards. Two
  analyses are already in `zumen-core` and reach every format at once: the comparison of revisions
  (`zumen_core::compare`) and the fields of the title block (`zumen_core::title`).
- [Decision 0009](0009-geometry-for-ai-clients.md) gives clients the shapes drawn and leaves their meaning to them.
  That works for questions about a few entities. It does not work for questions about how many lines fit together:
  the area of a room is the face that dozens of wall lines close, found among thousands of other lines. A client would
  need every line of the plan in its context (25,000 entities pass the budget of a tool call many times), and would
  have to split and join segments with exact arithmetic, which models do badly and pixels do not give.
- The people who use drawings ask for these facts in the same terms in every format: room areas in square meters
  for area tables and checks of floor area, the schedules drawn on a sheet as ruled lines and texts (door and window
  schedules, area tables, drawing lists), and whether a drawing follows the rules a client or a public standard sets
  for layers, colors, line types, text heights and sheets, such as the CAD drafting standards for public works in
  Japan (CAD製図基準), which SXF drawings follow.
- Python, the command line, the MCP server and the browser all read through the Rust crates, so what is written once
  in `zumen-core` reaches all of them ([decision 0005](0005-browser-packages-for-embedding.md)).

## Options

**Where the analysis runs**

- **In each binding or in the clients**: Python could compute regions from `Scene.entities(geometry=True)`, but the
  command line, WebAssembly and other Rust users would not get it, and Python would be slow on large plans.
- **In `zumen-core`, on the scene**: one implementation for every format and every binding, in the coordinates that
  boxes, images and placements already use, as `compare` and `title` do.
- **On each format's own model** (DXF entities, SXF features): exact curves and the format's own attributes, but the
  work would be repeated for every format, and placements of blocks, sub-figures and viewports would have to be resolved
  again.

**What a room is**

- **Recognize rooms**: find walls as pairs of parallel lines, doors as arcs, and close openings. Short results, but a
  guess with many rules, and it decides for the client what counts as a room.
- **The faces of the line arrangement**: split the lines where they cross or meet, join ends within a tolerance, and
  return every face they close, with its area, its outline and holes, the entities that bound it and the texts inside.
  Exact and explainable; the client (or a filter: a layer, a text inside, a smallest area) picks the rooms.
- **Leave it to the client**: not workable, for the reasons above.

**Tables**

- **Ruled tables**: cells are the faces of the table's horizontal and vertical lines, which the regions above already
  find; texts go to the cell that holds them; a cell that spans rows or columns is one face.
- **Tables without rules**: texts aligned in columns. Common in notes, but a guess about where columns are; left for
  later, behind an option.

**Checks**

- **Hard-coded rules of one standard**: quick to start, but standards differ by client and change by edition.
- **Rules as data** (a JSON document: layer name patterns, the colors, line types and line widths allowed on a
  layer, text heights, sheet sizes and scales), with built-in rule sets for public standards that drawings commonly
  follow: one engine, and an organization's own rules without a new release.

## Decision

Analyses are functions of `zumen-core` on the `Scene`, one module each, and every result names the entities it comes
from (ids), so that a client can render, cite and check it. The server computes facts that need the whole drawing;
deciding what they mean (which region is a room) stays with the client, with filters to narrow the results.

Each analysis has the same surfaces as the existing ones:

| Surface | Regions (this decision) | Tables (next) | Checks (after) |
| --- | --- | --- | --- |
| Rust | `zumen_core::regions::closed_regions` | `zumen_core::tables` | `zumen_core::checks` |
| Python | `Scene.regions` (all values), `zumen.tools.find_regions` (bounded JSON, `RegionPage`) | `Scene.tables`, `tools.find_tables` | `tools.check_drawing` |
| Command line | `zumen regions` (JSON lines) | `zumen tables` (JSON, CSV) | `zumen check` |
| MCP | `find_regions` | `find_tables` | `check_drawing` |

The browser gets them through `bindings/wasm` when an application needs them; there is no TypeScript port.

**Closed regions**, the first piece:

- The lines of lines, polylines, multilines, arcs, circles, ellipses, splines, clothoids and solids (their outlines)
  are the boundaries by default; dimensions, texts, leaders, hatches and viewport frames are not. `kinds` and
  `layers` change the choice.
- Lines are split where they cross and where an end lies on another line (T junctions, overlapping lines), and ends
  closer than `tolerance` (0.01 paper millimeters by default) are one point. Lines that end nowhere are removed, so
  they close nothing. Each bounded face is a region; a closed shape that touches no other line (a column, a fixture)
  is a hole of the face around it and a region of its own.
- Lines of different placements (an SXF sub-figure, a JWW layer group, a DXF viewport or model space) are never
  joined, because their scales differ. A region inside a placement also has its outline, holes, area and perimeter in
  the placement's real-world millimeters; the others have them on the sheet only.
- A region gives its area without its holes and the area inside its outline, its perimeter, its box, the entities
  that bound it, and the texts and attributes in the same placement whose box center lies inside it (room names,
  written areas). This keeps labels in a viewport from being assigned to an overlapping paper-space frame. Regions come
  in reading order (top to bottom, then left to right) for each placement.
- `zumen.tools.find_regions` gives real-world areas in square meters (`real_area_m2`) and lengths in millimeters, keeps
  only the lines on layers that match `layer`, the regions with a text (`named`) or of a smallest real-world area
  (`min_area_m2`), and bounds the points of outlines with `max_points` as the other tools do.

**Order**: closed regions, then ruled tables built on the same faces, then checks. Regions answer the question asked
most (areas) and stand alone; tables reuse their faces; checks need rule sets to be designed and written down from
the standards, and can then also use regions and tables (the sheet frame, the title block, a drawing list).

## Consequences

- An opening drawn as a gap in a wall (a door without a line across it) joins the room with the next one. A later
  option will close gaps up to a given width between wall ends that face each other; until then a client can see the
  joined region and its boundary entities.
- Furniture or fixtures drawn as closed shapes on the same layers become holes and lower the area; `layers` (or
  `layer` in the tools) restricts the boundaries to walls, and `real_outline_area_m2` gives the area inside the outline.
- Curves are the polylines drawn, so the area of a round room or a column differs from the exact one by the
  tolerance of the drawing (under 0.1 % for the circles of the test fixture).
- The work grows with the number of segments: about 3 seconds in a release build for 5 million segments (a grid of
  22,500 rooms with a round column in each), most of it for the circles. Clients should pass a region for very large
  sheets.
- Decision 0009 still holds: shapes stay available as they were drawn, and regions add a computed fact with the
  entities it comes from, not an interpretation.

## References

- [Texts, entities and images for servers](../architecture.md#texts-entities-and-images-for-servers)
- [Decision 0009](0009-geometry-for-ai-clients.md), [decision 0011](0011-tool-functions-for-agents.md)
- [`tests/fixtures/dxf/rooms.dxf`](../../tests/fixtures/dxf/rooms.dxf), generated by `packages/dxf/tests/fixtures.ts`
