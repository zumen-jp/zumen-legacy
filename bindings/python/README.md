# zumen for Python

Read 2D drawings of Japanese construction and render them, with the Rust implementation of
[zumen](https://github.com/zumenjs/zumen). The result is the same `Scene` model as the
`@zumenjs/core` viewer in the browser.

> Status: before the first release. SXF (SFC and P21, also compressed as P2Z and SFZ), Jw_cad JWW, DXF and DWG
> (AutoCAD 2000 to 2018) are supported.

```python
import zumen

scene = zumen.read("plan.jww")  # the format is detected from the contents
print(scene.version, len(scene), scene.sheet)
png = scene.render_png(dpi=150)  # for thumbnails or as an image for vision models
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

## MCP server

`zumen-mcp` lets AI applications that speak the Model Context Protocol open, render, search and measure drawings
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

`--root DIR` (repeatable) limits the directories the server may read. `--max-bytes` (256 MiB by default, after
unpacking P2Z and SFZ), `--max-entities` (2,000,000) and `--timeout` (60 seconds) limit each drawing it reads. Texts, layer names and properties come from the drawing file; clients should treat them as
data, not as instructions. The functions behind the tools (`zumen.mcp.summary`, `render`, `find_texts`,
`find_entities`, `get_entity`, `measure`) can be called without MCP.

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
