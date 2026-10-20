# zumen for Python

Read 2D drawings of Japanese construction and render them, with the Rust implementation of
[zumen](https://github.com/zumenjs/zumen). The result is the same `Scene` model as the
`@zumenjs/core` viewer in the browser.

> Status: before the first release. SXF (SFC and P21), Jw_cad JWW, DXF and DWG (AutoCAD 2000 to 2018) are supported.

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
entity = scene.entity("#120")  # id, type, source, layers, bbox, text, properties, placement
rows = scene.entities(hidden_layers=[])  # [] includes the layers that the drawing hides
table = scene.to_arrow()  # a pyarrow.Table, for pandas, Polars or DuckDB (pip install "zumen[arrow]")
```

`texts` joins the parts of a text that share a line and puts a line break between lines.
`properties` are the label and value pairs that the format gives, as in the viewer's property panel.

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

## MCP server

`zumen-mcp` lets AI applications that speak the Model Context Protocol open, render, search, measure and compare drawings
on this computer. It serves over stdio and only reads.

```bash
pip install "zumen[mcp]"
claude mcp add zumen -- zumen-mcp --root ~/drawings  # Claude Code; other clients take the same command
```

| Tool | What it returns |
| --- | --- |
| `open_drawing` | Format, sheet, views, layers with entity counts, entity types, notices and the title block |
| `render_drawing` | A PNG of the sheet or a region (1,568 pixels on the long edge by default), the scale to convert pixels to paper millimeters, and the texts drawn with their pixel boxes |
| `find_texts` | Texts that contain a query, ignoring full-width and half-width forms, case and spaces |
| `find_entities` | Entities in a region, by type or layer, with counts by type |
| `get_entity` | One entity with its properties and the scale of its placement |
| `measure_distance` | The distance between two points on the sheet and in real-world millimeters |
| `compare_drawings` | What changed between two revisions: an image of the changes, the regions that changed and the changed entities with their texts |

`--root DIR` (repeatable) limits the directories the server may read, and `--max-bytes` the file size
(256 MiB by default). Texts, layer names and properties come from the drawing file; clients should treat them as
data, not as instructions. The functions behind the tools (`zumen.mcp.summary`, `render`, `find_texts`,
`find_entities`, `get_entity`, `measure`, `compare_revisions`) can be called without MCP.

Reading and rendering release the GIL. Japanese text is drawn with the bundled
BIZ UDGothic (SIL Open Font License 1.1).
