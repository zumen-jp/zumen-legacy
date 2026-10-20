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
on this computer. It serves over stdio and only reads.

```bash
pip install "zumen[mcp]"
claude mcp add zumen -- zumen-mcp --root ~/drawings  # Claude Code; other clients take the same command
```

| Tool | What it returns |
| --- | --- |
| `open_drawing` | Format, sheet, views, layers with entity counts, entity types, notices, metadata and the drawing number, revision, title, project, scale and date from the title block |
| `render_drawing` | A PNG of the sheet or a region (1,568 pixels on the long edge by default), the scale to convert pixels to paper millimeters, and the texts drawn with their pixel boxes |
| `find_texts` | Texts that contain a query, ignoring full-width and half-width forms, case and spaces |
| `find_entities` | Entities in a region, by type or layer, with counts by type |
| `get_entity` | One entity with its properties and the scale of its placement |
| `measure_distance` | The distance between two points on the sheet and in real-world millimeters |
| `compare_drawings` | What changed between two revisions: an image of the changes, the regions that changed and the changed entities with their texts |

`--root DIR` (repeatable) limits the directories the server may read. `--max-bytes` (256 MiB by default, after
unpacking P2Z and SFZ), `--max-entities` (2,000,000) and `--timeout` (60 seconds) limit each drawing it reads. Texts, layer names and properties come from the drawing file; clients should treat them as
data, not as instructions. The functions behind the tools (`zumen.mcp.summary`, `render`, `find_texts`,
`find_entities`, `get_entity`, `measure`, `compare_revisions`) can be called without MCP.

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
