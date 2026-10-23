# Drawing markup

Redlines, revision clouds, notes, review findings and photo positions are kept in a JSON document beside the drawing,
never in the drawing file. Points are paper millimeters of one view of the drawing, with the Y axis up, so markup made
on a CAD layout lines up with the PDF plotted from it at the same paper size. The document names the drawing by the
SHA-256 of its bytes, and items name the entities they are about by their ids
([decision 0021](decisions/0021-drawing-markup.md)).

`@zumenjs/core` reads, checks and writes the documents and computes the shapes of their items, `@zumenjs/viewer` draws
and edits them on a `DrawingViewer` (`MarkupLayer` and its tools), and `@zumenjs/react` has hooks for both. The Rust
crate `zumen-core` is to read the same documents and give the same shapes, checked against the fixtures in
`tests/fixtures/markup/`, for Python, the command line and the MCP server
([#93](https://github.com/zumenjs/zumen/issues/93)).

```ts
import { createMarkup, drawingHash, markupFromJson, markupToJson } from "@zumenjs/core";
import { findingTool, MarkupLayer, markupEditTool, redlineTool } from "@zumenjs/viewer";

const document = saved ? markupFromJson(saved) : createMarkup({ sha256: await drawingHash(bytes), format: scene.format });
const layer = new MarkupLayer(viewer, { document, author: user.id });
layer.on("change", (event) => save(markupToJson(event.document)));
viewer.addTool(redlineTool(layer, { arrows: "end" }));
viewer.addTool(findingTool(layer, { create: (draft) => askForText(draft) }));
viewer.addTool(markupEditTool(layer));
viewer.setTool("markup-finding");
```

## The document

```json
{
  "format": "zumen-markup",
  "version": 1,
  "drawing": { "sha256": "9f2c…", "format": "dxf" },
  "items": [
    { "id": "0b6f…", "kind": "finding", "view": "A3-平面図", "position": [212.5, 140], "status": "open", "label": "3" }
  ]
}
```

| Key | Value |
| --- | --- |
| `format` | `"zumen-markup"` |
| `version` | `1`. Readers refuse larger versions |
| `drawing.sha256` | The SHA-256 ([FIPS 180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final)) of the drawing's bytes, 64 lowercase hexadecimal digits. For P2Z and SFZ, the archive |
| `drawing.format` | `Scene.format` of the drawing (`sfc`, `jww`, `dxf`, …) |
| `items` | The items, drawn in this order |
| `data` | Optional; an object for the application, kept as it is |

Every item has these keys:

| Key | Value |
| --- | --- |
| `id` | A non-empty string, unique in the document. The viewer makes UUIDs unless the application gives its own |
| `kind` | `redline`, `cloud`, `text`, `finding` or `photo` |
| `view` | The view id of the scene (`Scene.view`) whose coordinates the points are in. Left out for formats with one view. An item is drawn only in its view |
| `category` | Optional; a key of the application in lowercase letters, digits and hyphens (`correction`, `question`…), which chooses the color |
| `color` | Optional; `#rrggbb` or `#rrggbbaa`, over the category's color |
| `lineWidth` | Optional; the width of the lines in paper millimeters, more than 0. Without it lines are drawn 2 pixels wide (below) |
| `text` | Optional; the text of a note, or of a finding |
| `entities` | Optional; the ids of the entities the item is about. They resolve only in the drawing whose bytes have `drawing.sha256` |
| `author` | Optional; the application's name or id for a person |
| `createdAt`, `updatedAt` | Optional; date and times of [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) (section 5.6, with an offset or `Z`) |
| `carried` | Optional; `{ "from": "<sha256 of the drawing it was made on>", "state": "unchanged" \| "changed" \| "removed" }`, set when the markup was carried to a new revision |
| `data` | Optional; an object for the application (assignees, due dates, replies…), kept as it is |

The kinds have their own keys:

| `kind` | Keys | Default |
| --- | --- | --- |
| `redline` | `points` (2 or more), `closed`, `arrows` (`none`, `start`, `end`, `both`) | `closed` false, `arrows` none |
| `cloud` | `points` (3 or more, the closed outline), `arcLength` (the chord of an arc, paper millimeters, more than 0) | `arcLength` 10 |
| `text` | `position` (the lower left of the first line), `text` (required), `height` (paper millimeters, more than 0), `angle` (degrees, counterclockwise) | `height` 3.5, `angle` 0 |
| `finding` | `position` (the pin), `status` (required: `open`, `resolved`, `closed`), `points` (the area, 3 or more), `label` (short text on the pin, such as its number) | |
| `photo` | `position` (where the photo was taken), `photo` (required: a reference that the application resolves, such as an id or a URL), `direction` (degrees counterclockwise from +x, the camera's heading), `takenAt` (RFC 3339) | |

A point is `[x, y]`, two finite numbers. A finding is `open` until the designer handles it, `resolved` until the reviewer
confirms it, and `closed` when confirmed or withdrawn; finer states belong to the application. The document never holds
the bytes of a photo.

### Unknown keys

A key that a reader does not know, in the document or in an item, is kept as it is when the document is read and
written again, and so is a key of another kind (a `position` on a redline). Additions that older readers may ignore keep
version 1. In TypeScript the objects returned by `markupFromJson` keep such keys although the types do not name them,
and `MarkupLayer.update` keeps them.

### Checks

`markupFromJson(json)` refuses a document with the first error met in this order, and the Rust reader is to give the
same error: the JSON, `format`, `version`, `drawing` (`sha256`, then `format`), `items`, `data`,
then each item in order: that it is an object, `id`, a duplicate id, `kind`, the common keys in the order of the
table above (`carried.from` before `carried.state`), then the keys of its kind in the order of the table of kinds. An
error has:

| Field | Value |
| --- | --- |
| `code` | `json` (not JSON), `format` (not a markup document), `version` (newer than the reader), `missing` (a required key), `type` (a value of the wrong JSON type), `value` (a value out of range, a pattern not matched, too few points), `duplicate` (an id used twice) |
| `item` | The index of the item, or null for the document's own keys |
| `id` | The id of the item, or null before it is read |
| `key` | The key: `drawing.sha256`, `carried.state`, `points`…; null when the JSON or an item itself is wrong |

`tests/fixtures/markup/errors.json` lists the cases both readers must agree on. A number beyond the range of a double
(`1e999`) is the one difference: `JSON.parse` makes it infinite, which TypeScript refuses as a `value`, and Rust refuses
the text as `json`.

## Shapes

`markupShapes(item, { color, pixel })` returns the shapes drawn for an item, in paper millimeters, as primitives of the
scene model (strokes, fills and texts), in drawing order. The viewer draws them with `renderPrimitives`, and servers
are to draw the same primitives with `zumen-render`.

- The color is the item's `color`, else `options.color` (the color of its category, which the application or the
  viewer's style chooses), else `#ff0000`. Translucent parts multiply the alpha of `#rrggbb` and `#rrggbbaa` colors
  and are written `#rrggbbaa` in lowercase; the fill color `background` is the background of the view.
- `pixel` is the paper length of one pixel of the parts drawn at a fixed size on screen: pins, photo symbols, lines
  without `lineWidth` and their arrowheads. The viewer passes one CSS pixel at the current zoom. Servers use the
  default, 25.4 / 96 mm, the size of a CSS pixel at zoom 1, so a picture drawn on a server looks like the viewer at
  100%.
- `w` is the line width: `lineWidth`, or 2 pixels.

| Kind | Shapes |
| --- | --- |
| `redline` | One stroke through `points` of width `w`, closed if `closed`. Unless closed, an arrowhead at each end that `arrows` names: a filled triangle with its tip at the end, length 6`w` and half width 2`w`, pointing from the nearest point at another position |
| `cloud` | One closed stroke of width `w`. Each edge of the outline (the last point joins the first; edges of length 0 are skipped) is divided into `n = max(1, round(length / chord))` equal chords, where `chord = max(arcLength, perimeter / 4096)`. Over each chord, an arc with a central angle of 120° bulges outward (to the right of the direction of travel when the signed area of the outline is 0 or more, to the left otherwise): its center is the middle of the chord moved inward by half the chord / tan 60°, and it is drawn as 8 segments, the start of the chord followed by 7 points each turned by 15° (counterclockwise for outlines of positive area) around the center |
| `text` | One text per line (lines split at `\r\n`, `\r` and `\n`; empty lines draw nothing but keep their place), laid out by the text layout of the scene model with the given height and angle, the lower left (anchor 1) at `position` for the first line, and each further line 1.5 `height` lower in the text's own down direction. The font is the default gothic of the scene model |
| `finding` | With `points`: the area filled at 15% opacity, then its closed outline of width `w`. The pin, a circle of radius 10 pixels at `position` drawn as a polygon of 32 points starting at +x: filled for `open`, filled with the background and outlined in the color with a line of 2 pixels for `resolved`, filled at 40% opacity for `closed`. `label` is centered on the pin, 10 pixels high, or less so that it is no wider than 16 pixels (half-width characters count as half), in the background color, or in the color for `resolved` |
| `photo` | With `direction`: a sector of radius 24 pixels from `position`, 30° to each side of the direction, as `position` followed by 9 points on the arc, filled at 30% opacity and outlined with a line of 1 pixel. Then a filled circle of radius 6 pixels and a circle of radius 2.5 pixels in the background color (the lens), both of 32 points |

`markupBox(item)` is the box of the shapes drawn with a pixel of 0: the points, the arcs of clouds and the outlines of
texts, without the parts drawn at a fixed size on screen, so a pin or a photo is the box of its position. Carrying to
a new revision compares this box with the regions that changed.

`tests/fixtures/markup/kinds.markup.json` has every kind and key, and `kinds.shapes.json` its shapes with the default
options and with `{ "color": "#336699", "pixel": 0.5 }`, written by the TypeScript side (`pnpm run golden`).

## Revisions

When `drawing.sha256` is the hash of the bytes shown, `entities` resolve in the scene of the item's view. Otherwise
the markup was made on another revision: it is drawn by its points either way, and carrying moves it to the new bytes
by the comparison of revisions (`zumen_core::compare`) view by view, as
[decision 0021](decisions/0021-drawing-markup.md#revisions) describes. A sheet drawn as CAD and the PDF plotted from
it share paper millimeters, but their views have other ids (a layout name, a page number); carrying with a map of
views moves items from one to the other.

## In the browser

`MarkupLayer` of `@zumenjs/viewer` is built on the public API of `DrawingViewer`: it draws with an overlay and
queries the viewer like any application would.

- **State.** `getState()` and `subscribe(listener)`: the document, the selection and the hovered item (ids), the filter
  and whether `undo` and `redo` can run. `items()` returns the items drawn now (the scene's view, the filter), and
  `itemsAt(x, y, { tolerance })` those under a paper point, topmost first.
- **Editing.** `add(items)` fills in the id (`createId`), `author` and the times (`now`) from the options and the
  scene's view, `update(id, patch)` or `update([{ id, patch }])` sets `updatedAt` and removes keys given as
  `undefined`, and `remove(ids)`; each call checks the items, is one step of `undo` and `redo` (`historyLimit`, 100
  by default), and sends `change` with the document and the ids added, updated and removed (`origin` is `edit`, `undo`
  or `redo`), for the application to save. `setDocument(document)` replaces the document and clears the history
  without sending `change`.
- **Tools.** Factories like the viewer's own tools, added with `viewer.addTool` and chosen with `viewer.setTool`:
  `redlineTool(layer, { mode })` (`line`, `polyline`, `freehand`, `rectangle`), `cloudTool(layer, { mode })`
  (`rectangle`, `polygon`), `textTool`, `findingTool`, `photoTool` and `markupEditTool` (select, move, move a point,
  delete with Delete or Backspace, pan where there is nothing). Each takes `name`, `category`, `color` and
  `lineWidth`, and a tool that creates an item passes the draft to `create(draft, layer)`, which returns the item with
  its text and fields (it may wait for the application's own interface) or `null` to cancel. A pending result is
  discarded after cancellation, a tool or scene change, or replacement of the markup document. The pin of a finding
  placed with a click takes the entity under it as its `entities`, and its `label` is the next number. Ctrl or ⌘ with
  Z undoes and with Shift redoes. `setPreview(items)` draws items that are not in the document, as the tools do for
  drafts and moves.
- **Style.** Colors come from CSS custom properties on the viewer's element, and `setStyle` overrides them:

  | Property | Default (dark, light) | |
  | --- | --- | --- |
  | `--zumen-markup-dark`, `-light` | `#ff4d4d`, `#e00000` | Items without a color or a category color |
  | `--zumen-markup-category-<category>-dark`, `-light` | | Items of that category |
  | `--zumen-markup-selection-dark`, `-light` | `#33ccff`, `#0062d6` | The frame and point handles of selected items |
  | `--zumen-markup-changed-dark`, `-light` | `#ffb000`, `#b86e00` | The frame of items carried as `changed` |
  | `--zumen-markup-removed-dark`, `-light` | `#a0a0a0`, `#707070` | The frame of items carried as `removed` |

  Any CSS color works; the layer turns it into `#rrggbbaa` to draw the translucent parts.
- **React.** `useMarkupLayer(viewer, { document, …options })` creates the layer while the viewer and a document exist,
  calls `setDocument` when `document` becomes another document than the layer's (passing back the document of a
  `change` event does nothing), and `setAuthor` when `author` changes. `useMarkupState(layer, selector)` and
  `useMarkupEvent(layer, "change", listener)` read it.

The playground's markup sample (`playground/markup-sample.tsx`) has tools, categories, undo and redo, a list of items,
a panel for the text and status of a finding, and saving and opening a document, with its own text and CSS.
