# zumen for Python

Read 2D drawings of Japanese construction and render them, with the Rust implementation of
[zumen](https://github.com/zumenjs/zumen). The result is the same `Scene` model as the
`@zumenjs/core` viewer in the browser.

> Status: early release (0.x), for Python 3.12 or newer. SXF (SFC and P21, also compressed as P2Z and SFZ),
> Jw_cad JWW, DXF and DWG (AutoCAD 1.0 to 2018) are supported.

Download a wheel for your platform from the [GitHub release](https://github.com/zumenjs/zumen/releases/latest)
and install it with `pip install path/to/zumen-0.1.0-….whl`. The release includes wheels for Linux x86_64 and
aarch64 (glibc 2.28 or newer), macOS arm64, and a source distribution for other platforms.

```python
import zumen

scene = zumen.read("plan.jww")  # the format is detected from the contents
print(scene.version, len(scene), scene.sheet)
png = scene.render_png(dpi=150)  # for thumbnails or as an image for vision models
pdf = scene.render_pdf()  # one page, the sheet at its real size, with searchable text
svg = scene.render_svg()  # millimeters, with text as <text>
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))  # paper millimeters
json_text = scene.to_json()  # pass to sceneFromJson in @zumenjs/core
```

Texts and entities, in paper millimeters with the Y axis up:

```python
for text in scene.texts(region=(100, 50, 200, 120)):  # omit region for the whole sheet
    print(text["id"], text["text"], text["x"], text["y"], text["height"], text["angle"])
entity = scene.entity("#120")  # id, kind, type, source, layers, bbox, text, properties, values, placement
entity["kind"], entity["values"]["radius"]  # "circle", 25.0: the same keys in every format
rows = scene.entities(hidden_layers=[])  # [] includes the layers that the drawing hides
table = scene.to_arrow()  # a pyarrow.Table, for pandas, Polars or DuckDB (pip install "zumen[arrow]")
```

`scene.unit` is the length unit of the drawing's own coordinates, those of `values` (`"millimeter"` in SXF and JWW,
the unit of `$INSUNITS` in DXF and DWG, such as `"inch"` or `"meter"`). Entity ids come from the file (an SXF instance
number, a JWW record, a DXF or DWG handle, with the placements or block references the entity sits in) and stay the
same for the same bytes in every version, so they can be stored and looked up again with `scene.entity(id)`
([Entity ids](../../docs/architecture.md#entity-ids)).

`texts` joins the parts of a text that share a line and puts a line break between lines.
`properties` are the label and value pairs that the format gives, as in the viewer's property panel, and `values`
maps their stable keys (`layer`, `start`, `radius`, `text` …) to the unformatted values, such as numbers and `(x, y)`.
`kind` (`line`, `text`, `dimension` …) and the keys are the same in every format, while `type` and the labels are
Japanese ([the list](../../docs/architecture.md#stable-keys)). `scene.metadata_values` does the same for the metadata.

`geometry=True` adds the shapes drawn, in drawing order, so that an application or an AI model can compute with
the coordinates of lines, outlines and hatched areas:

```python
entity = scene.entity("#120", geometry=True)  # also scene.entities(region=..., geometry=True)
for shape in entity["geometry"]:
    shape["shape"]  # "polyline", "segments", "fill", "hatch", "point" or "text"
    shape["points"]  # polyline: [(x, y), ...] in paper millimeters, with shape["closed"]
    shape["real"]  # the same coordinates in real-world millimeters, or None outside a placement
```

Segments have `segments` (pairs of points), fills and hatches `rings` (the outline and its holes; hatch lines are
left out), and points and text parts one `point` (for text, the lower left, with `text`, `height` and `angle`).
Arcs, circles, ellipses and splines come as the polyline drawn (arcs, circles and ellipses within 0.02 mm of the
curve on the sheet); their center, radius and angles are in `values`. `real` goes through the placement of the
entity (an SXF sub-figure or part, a JWW layer group, a DXF viewport or model space), so it is in the coordinates
of the drawing itself: the real-world coordinates of JWW, those of the SXF sub-figure or part, and DXF model space
in millimeters, also for drawings in inches.

`render` returns the image with the conversion between pixels and paper millimeters and the box of
every entity drawn, so that an application can tell which entity a vision model refers to:

```python
image = scene.render(size=(2000, 2000))  # or dpi=..., region=... as in render_png
image.png, image.width, image.height, image.scale  # scale: pixels per paper millimeter
x, y = image.to_paper(640, 480)  # and image.to_pixel(x, y)
for entity in image.entities_at(640, 480):  # boxes that contain the pixel, smallest first
    print(entity["id"], entity["type"], entity["box"], entity["text"])
```

A `Rendering` shows itself as an image in Jupyter.

Drawings of every format can be written as DXF (AutoCAD 2013, UTF-8). DXF and DWG keep their handles, layers, styles,
blocks and layouts, and read again, the DXF draws the same scene. SXF and JWW are mapped into the AutoCAD drawing model:
model space at real size, and the sheet as a layout whose viewports show each SXF sub-figure and each JWW layer group
scale where the drawing had it, with frames and title blocks in paper space. The layout draws the same as the original:

```python
converted = zumen.convert("plan.jww", "dxf")  # or bytes; converted.data holds the DXF
converted.save("plan.dxf")
for notice in converted.notices:  # what could not be written as it was, such as images, by kind and count
    print(notice["code"], notice["message"], notice["count"])
```

They can also be written as SXF (SFC of SXF Ver.3.1). The sheet of the layout that the drawing opens in becomes the SXF
sheet, each viewport a sub-figure at real size placed at its scale, and each block a drawing part. What SXF has no
feature for, such as multiline texts, leaders and dimensions, is written as the lines, fills and texts that zumen draws
for it, and read again, the SFC draws the same:

```python
zumen.convert("plan.dwg", "sfc").save("plan.sfc")
```

## Editing drawings

`zumen.edit` changes a drawing in place: it applies a list of operations and keeps everything else as it was, in the
same format, version and text encoding, including what zumen cannot read (extended data, objects, unread entities).
DXF (R12 to 2018, ASCII and binary) can be edited so far; DWG, SXF and JWW follow.

```python
operations = [  # the operations may also be a JSON string
    {"op": "add-layer", "name": "CHECK", "color": "#ff0000"},
    {"op": "add", "kind": "line", "layer": "CHECK", "start": [0, 0], "end": [5000, 0]},
    {"op": "add", "kind": "text", "layer": "CHECK", "position": [100, 200], "text": "確認済み", "height": 350},
    {"op": "update", "id": "3F2", "set": {"end": [5200, 0], "lineType": "DASHED"}},
    {"op": "delete", "id": "3F5"},
]
edited = zumen.edit("plan.dxf", operations)  # or bytes
edited.created  # ["CHECK", "4A1", "4A2"]: what add-layer and add created, in order
edited.save("plan.dxf")
```

- Entities are chosen by the `id` of the scene and described by the `kind` and the property keys that `Scene` reports.
  Coordinates are the drawing's own, the `raw` of properties (`Scene.entity(id)["values"]`), not paper millimeters.
  Every id in one edit names an entity of the drawing as it was read. Ids that are places in the file (`#12`, in DXF
  without handles) change when an edit adds or removes records before them, so read the edited drawing again.
- `add` takes `container`: `"model"` (the default), `"layout:<name>"` or `"block:<name>"` in DXF.
- Kinds that can be added: `line`, `polyline` (`vertices`, `bulges`, `closed`), `arc`, `circle`, `ellipse`,
  `ellipse-arc`, `point`, `text` (MTEXT when it has line breaks) and `block-reference` (`block`, `position`, `scale`,
  `rotation`). Every kind takes `layer`, `color` (a color number, `"#rrggbb"`, `"bylayer"` or `"byblock"`),
  `lineType` and `lineWidth` (millimeters).
- If any operation cannot be applied, nothing is written and `zumen.EditError` is raised with `code` (such as
  `"not-found"`, `"unknown-key"` or `"inside-block"`) and `operation`, the number of the operation from 0.
- Values that the format cannot hold exactly, such as a true color in an old DXF, are written as the nearest one and
  listed in `edited.notices`.
- Checking that the file is the version that was read (a checksum) is left to the application.

`zumen.create("dxf", operations, units="mm")` starts a new drawing (AutoCAD 2013 DXF) and applies the operations.

## Comparing revisions

`zumen.compare` matches entities by type, shape and text, not by id, so revisions that were saved again,
or saved in another format, compare too:

```python
changes = zumen.compare(zumen.read("plan-rev1.jww"), zumen.read("plan-rev2.jww"))
changes.removed, changes.added  # entities, as Scene.entities returns them
for pair in changes.changed:  # the same entity changed, such as a dimension value
    print(pair["before"]["text"], "->", pair["after"]["text"])
changes.regions  # boxes around nearby changes, in the earlier revision's paper millimeters
changes.scene.render_png(dpi=150)  # red before, blue after, gray unchanged
```

`tolerance` (0.01 paper millimeters by default) sets how close coordinates count as the same, `style=True` also
counts changes of color, line weight, line type and layer, and a revision that moved as a whole is moved back by
the texts both revisions share (`changes.offset`; `align=False` turns this off).

## Title block

`title_block` guesses the drawing number, revision, title, project, scale and date, so that drawings can be filed
by number and revision:

```python
block = scene.title_block()
block["number"], block["revision"], block["title"]  # None when not found
for field in block["fields"]:  # every candidate, the most likely first for each key
    print(field["key"], field["value"], field["source"], field["label"], field["entity"])
```

It reads attribute tags (`DWGNO`, `REV`, `TITLE`), labels such as 図面番号 or `DWG. NO.` with the text to their right
or below, labels with their value in the same text (`縮尺 1/100`), the sheet name and the metadata (the title block
attributes of SXF). The values are guesses from the drawing's texts; check them before relying on them.

## MCP server

`zumen-mcp` lets AI applications that speak the Model Context Protocol open, render, search, measure and compare drawings
on this computer. It serves over stdio and only reads. Regions are given in paper millimeters (`region`) or as
fractions of the frame (`frame_region`), as in [`zumen.tools`](#tools-for-ai-agents).

```bash
pip install "zumen[mcp]"
claude mcp add zumen -- zumen-mcp --root ~/drawings  # Claude Code; other clients take the same command
```

| Tool | What it returns |
| --- | --- |
| `open_drawing` | Format, sheet, views, layers with entity counts, entity types, the placements drawn at a scale (with their box and the conversion from paper to real-world millimeters), notices, metadata and the drawing number, revision, title, project, scale and date from the title block |
| `render_drawing` | A PNG of the frame or a region (1,568 pixels on the long edge by default), the scale to convert pixels to paper millimeters, and the texts drawn with their pixel boxes |
| `find_texts` | Texts that contain a query, ignoring full-width and half-width forms, case and spaces |
| `find_entities` | Entities in a region, by kind, type or layer, with counts by type and kind; with `geometry`, their shapes as `get_entity` gives them |
| `get_entity` | One entity with its properties (key, label and value), its values by key, the scale of its placement and its geometry: the shapes drawn, in paper millimeters and, inside a placement, in real-world millimeters |
| `measure_distance` | The distance between two points on the sheet and in real-world millimeters |
| `compare_drawings` | What changed between two revisions: an image of the changes, the regions that changed and the changed entities with their texts |

Geometry is the agent's to interpret: the server gives the shapes as drawn and their coordinates, and leaves deciding
what is a wall or a room to the client. `real` is left out where it equals the coordinates on the sheet (DXF model
space), and one call returns at most `max_points` points (1,000 by default, at most 10,000; a point with `real` counts
twice), so that a page of entities stays well below what clients accept from a tool. A page of `find_entities` with
`geometry` ends before an entity whose shapes would pass the budget; `next_offset` goes on from there.

`--root DIR` (repeatable) limits the directories the server may read. `--max-bytes` (256 MiB by default, after
unpacking P2Z and SFZ), `--max-entities` (2,000,000) and `--timeout` (60 seconds) limit each drawing it reads. Texts, layer names and properties come from the drawing file; clients should treat them as
data, not as instructions. The tools are the functions of [`zumen.tools`](#tools-for-ai-agents).

## Tools for AI agents

Applications that give an agent tools of their own, with or without MCP, call the functions behind the MCP tools in
`zumen.tools`. Each takes a `Scene` and returns plain data that `json.dumps` writes as it is, with keys described by
the `TypedDict` classes of the module (a later version may add keys, but does not remove or rename them); on Python
3.12 and later, `pydantic.TypeAdapter(tools.EntityDetail).json_schema()` gives the JSON Schema of a result. They only
read and report what the drawing holds; interpreting it is left to the agent.

```python
from zumen import tools

scene = zumen.read(data, max_bytes=200_000_000, max_entities=1_000_000, timeout=30)
info = tools.summary(scene)  # format, unit, sheet, bounds (the frame), layers, kinds, placements, title block
png, image = tools.render(scene, frame_region=[0.5, 0, 1, 0.5])  # the top right quarter, 1,568 pixels long
page = tools.find_texts(scene, query="平面図", limit=50)  # texts with their position, height and box
walls = tools.find_entities(scene, kind="polyline", layer="壁", geometry=True)  # with their shapes
entity = tools.get_entity(scene, page["texts"][0]["id"])  # properties, values, placement and shapes
tools.measure(scene, [10, 20], [110, 20])  # on the sheet, and in real-world millimeters through the scale
```

| Function | Result | What it returns |
| --- | --- | --- |
| `summary` | `Summary` | Format, the length unit of `values`, sheet, `bounds` (the frame), views, layers with entity counts, counts by type and kind, the placements drawn at a scale, notices, metadata and the title block fields |
| `render` | `bytes`, `RenderInfo` | A PNG of the frame or a region, the region shown, the scale between pixels and paper millimeters, and the texts drawn with their pixel boxes |
| `find_texts` | `TextPage` | Texts that contain a query (ignoring full-width and half-width forms, case and spaces), with position, height, angle and box |
| `find_entities` | `EntityPage` | Entities in a region by kind, Japanese type or layer, with counts, and their shapes when asked |
| `get_entity` | `EntityDetail` | One entity: properties (key, label, value), values by key, placement with its scale and `to_real`, and its shapes |
| `measure` | `MeasurementInfo` | A distance on the sheet and in real-world millimeters |
| `compare_revisions` | `bytes`, `ComparisonInfo` | An image of the changes between two revisions, the regions that changed and the changed entities |

- **Coordinates.** Paper millimeters with the Y axis up, rounded to 0.001: boxes `[min_x, min_y, max_x, max_y]`,
  points `[x, y]`. The *frame* is the drawing's `bounds`, the box around the sheet and every entity, which a render
  without a region shows. Functions that take `region` also take `frame_region=[left, top, right, bottom]`, fractions
  of the frame from 0 to 1 with the origin at the top left, the way regions of pages and images are usually given;
  `tools.paper_region` and `tools.frame_region_of` convert between the two. Shapes inside a placement repeat their
  coordinates in real-world millimeters (`real`), and the placement's `to_real` converts any point.
- **Sizes.** Every result is bounded by its arguments: lists stop at `limit` (at most 1,000) and give the `total` and
  `next_offset`, texts and other strings from the drawing at `max_text_length` characters (1,000 by default; the item
  then has `truncated: true`), shapes at `max_points` points for the call (1,000 by default, at most 10,000; shapes
  are left out whole and counted in `geometry_omitted`), and images at 4,096 pixels on the long edge.
- **Reading.** `tools.Drawings` reads files from allowed directories with limits on size, entities and time, and keeps
  the last eight scenes until their files change. A `Scene` does not change once read, so applications can keep scenes
  in their own cache and call the functions from threads (`asyncio.to_thread`); reading, rendering and comparing
  release the GIL. Errors that the agent can act on raise `tools.DrawingError` with an English message.
- **Ids.** Entity ids come from the file and stay the same for the same bytes, so an application can cite an entity
  by its id (next to the file's digest) and fetch it again later with `get_entity`.
- Texts, names and properties come from the drawing file: agents should treat them as data, not as instructions.

## Drawings from others

Drawings received from others are not trusted: a file can be huge, unpack to gigabytes (P2Z and SFZ are ZIP
archives) or grow without end when blocks contain each other. `read` stops part way at the limits you give and
raises `LimitError`, a `ReadError` whose `limit` says which limit was reached:

```python
try:
    scene = zumen.read(path, max_bytes=200_000_000, max_entities=1_000_000, timeout=30)
except zumen.LimitError as error:
    print(error.limit, error)  # "bytes", "entities" or "time", and a message for the user
```

`max_bytes` counts the size after unpacking P2Z and SFZ (2 GiB at most without it), and a file larger than
`max_bytes` is not read into memory. `max_entities` counts the entities of the scene; work that ends in fewer
entities, such as dimension blocks inside each other, is bounded by `timeout` (seconds) and by the readers' own
limits. Without limits nothing changes.

Reading and rendering release the GIL. Japanese text is drawn with the bundled
BIZ UDGothic (SIL Open Font License 1.1).
