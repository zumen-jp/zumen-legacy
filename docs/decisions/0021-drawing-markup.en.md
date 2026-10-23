# 0021: Keep drawing markup in a JSON document beside the drawing, in paper millimeters of a view, and carry it to new revisions with compare

> English reference preserved when Japanese became the primary documentation. See [0021-drawing-markup.md](0021-drawing-markup.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- [Decision 0002](0002-pdf-and-parapdf.md) put drawing markup (redlines, revision clouds, review findings, photo
  positions) in zumen, kept in paper millimeters so that CAD and PDF drawings share it, and PDF annotations read and
  written as PDF in ParaPDF. Nothing decides how markup works yet.
- [Decision 0017](0017-add-annotations-and-definitions.md) writes annotations into the drawing file: they become part of
  the drawing. Markup is a layer on top of a drawing that is often someone else's: a reviewer's findings (照査),
  corrections returned to the designer (赤入れ), clouds around what a revision changed, photos of the site placed on a
  plan. The drawing stays as it was received.
- Every scene with a sheet has the sheet's lower left at the origin, in paper millimeters with the Y axis up (the viewer
  draws the frame from `[0, 0]` to `[width, height]`), and a PDF page puts its crop box there too ([decision
  0020](0020-pdf-drawings-in-rust.md)). A drawing with several views (DXF and DWG layouts, PDF pages) has a coordinate
  system per view. DXF and DWG model space has no sheet, and its coordinates are moved by `viewOrigin` when they are
  large.
- [Entity ids](../architecture.md#entity-ids) stay the same for the same bytes, but not when another program saves the
  drawing again. `zumen_core::compare` matches two revisions by shape and text instead, and gives the pairs of unchanged
  and changed entities, the removed and the added ones, the offset of a revision that moved as a whole and the regions
  that changed. It exists in Rust only.
- `DrawingViewer` takes the tools of applications (`addTool`, `setTool`), overlays (`addOverlay`) and colors from CSS
  custom properties, and the packages have no text for people ([decision 0005](0005-browser-packages-for-embedding.md)).
- The owner answered four questions on 2026-10-23: markup is stored as JSON only, and is not written back into drawing
  files; one person saves a document at a time, with no simultaneous editing; a finding needs its text and status only,
  and the application keeps assignees, due dates and replies, linked by the item's id; and no existing markup format
  needs to be read or written.

## Options

**Where markup is stored**

1. **In the drawing** (a layer of the DXF, DWG, JWW or SXF file): every format's editor would write it, PDF would need
   PDF annotations (ParaPDF's), the bytes, the hash and the ids of the drawing would change, and a reviewer often may
   not change the file.
2. **A JSON document beside the drawing**: one format for every drawing format, PDF included, and the drawing stays byte
   for byte.
3. **Both**, the JSON as the source and an export into the drawing: not needed now (the owner's answer).

**How it refers to the drawing**

- **The file name**: changes, and is the application's.
- **A hash of the bytes**: tells for certain whether the markup was made on these bytes, but every revision has another.
- **Entity ids**: name what an item is about, valid while the bytes are the same.
- **The view**: tells which coordinate system the points are in.

**How it follows a new revision**

- **It does not**: the application shows old markup with the old file only.
- **By coordinates**: the items stay where they were, which is wrong when the revision moved as a whole, and says
  nothing about whether what they point at changed.
- **Through compare**: move the items by the offset of the revision, point their entity ids at the matching entities,
  and mark the items whose entities changed or were removed, or that lie in a region that changed.

**Where the code lives**

- **The model in TypeScript only**: servers and agents could not read, draw or carry markup.
- **The model in both languages**, as `@zumenjs/core` and `zumen-core` share the scene, with editing in the engine.
- **Editing in `DrawingViewer` itself**, or **a layer on the engine's public API**, as drawing tools of map engines are
  built on the map.

## Decision

Option 2, referring to the drawing by the hash of its bytes, the view and entity ids, and carried to new revisions
through compare. The model is in both languages, editing is a layer on the public API of `DrawingViewer`, and Python,
the command line and the MCP server read, draw and carry markup.

### The document

```json
{
  "format": "zumen-markup",
  "version": 1,
  "drawing": { "sha256": "9f2c…", "format": "dxf" },
  "items": [
    {
      "id": "0b6f6f1e-5d8a-4c8e-9a52-3f1d2c7e4b10",
      "kind": "finding",
      "view": "A3-平面図",
      "position": [212.5, 140],
      "points": [[200, 130], [225, 130], [225, 150], [200, 150]],
      "label": "3",
      "text": "壁厚が構造図と合わない",
      "status": "open",
      "category": "correction",
      "entities": ["4C:2A>1F"],
      "author": "u-128",
      "createdAt": "2026-10-23T09:12:00Z",
      "updatedAt": "2026-10-23T09:12:00Z"
    }
  ]
}
```

- `format` is `"zumen-markup"` and `version` is 1. Readers refuse other formats and larger versions. A key they do not
  know is kept as it is when a document is read and written again, so additions that older readers may ignore keep
  version 1.
- `drawing` names the file the markup was made on: `sha256`, the SHA-256 of its bytes in lowercase hex (for P2Z and SFZ,
  the archive), and `format`, the `Scene.format`. The document may have `data`, an object for the application.
- Every item has:
  - `id`, unique in the document (a UUID unless the application chooses), and `kind`;
  - `view`, the view id of the scene (left out for formats with one view), and points in paper millimeters of that view,
    `[x, y]` with the Y axis up, the coordinates of the scene;
  - optionally `category` (a key of the application in lowercase letters, digits and hyphens, such as `correction`,
    `question` or `checked`, which chooses the color), `color` (`#rrggbb` or `#rrggbbaa`, over the category's),
    `lineWidth` (paper millimeters), `text`, `entities` (the ids of the entities the item is about), `author` (the
    application's name or id for a person), `createdAt` and `updatedAt` (RFC 3339), `carried` (below) and `data` (an
    object for the application, kept as is).
- The kinds, with their own keys:

  | `kind` | Keys | Drawn as |
  | --- | --- | --- |
  | `redline` | `points` (2 or more), `closed`, `arrows` (`none`, `start`, `end`, `both`) | Lines, polylines, freehand strokes and rectangles, with arrowheads; a wide, translucent color makes a highlighter |
  | `cloud` | `points` (3 or more, closed), `arcLength` (paper millimeters) | A revision cloud: arcs of that chord bulging outward along the outline |
  | `text` | `position` (lower left), `text`, `height` (paper millimeters), `angle` (degrees) | A note |
  | `finding` | `position` (the pin), `status` (`open`, `resolved`, `closed`), optionally `points` (the area, closed) and `label` (short text on the pin, such as its number) | A pin with its area; `text` is the finding |
  | `photo` | `position` (where the photo was taken), `photo` (a reference the application resolves, such as an id or a URL), optionally `direction` (degrees counterclockwise from +x, the camera's heading) and `takenAt` | A camera symbol pointing in its direction |

  `open` is a finding not handled yet, `resolved` one the designer handled and the reviewer has not confirmed, and
  `closed` one confirmed or withdrawn. Finer states, assignees, due dates and replies are the application's, in its own
  system or in `data`. The document never holds the bytes of a photo.
- The shapes of an item (the arcs of a cloud, arrowheads, the outline of a text) are computed by one rule in both
  languages, so the viewer and `zumen-render` draw the same lines. Pins and photo symbols, and lines without
  `lineWidth`, are drawn at a fixed size on the screen in the viewer, and on servers at a fixed number of pixels in PNG
  (whose drawings may be fitted at any scale) and at the paper size of a CSS pixel in SVG and PDF (the sheet at its
  real size). The rules and the defaults of the optional keys are in [Drawing markup](../markup.md).

### References to the drawing

- When `drawing.sha256` is the hash of the bytes shown, `entities` resolve in the scene of the item's view. Otherwise
  the application carries the markup to these bytes (below) or shows it as made on another revision; items are drawn by
  their points either way.
- Items are drawn in the view they name. A sheet drawn as CAD and the PDF plotted from it at the same paper size line
  up, but their views have other ids (a layout name, a page number): carrying with a map of views (`{"A3-平面図": "1"}`)
  moves the items from one to the other, and aligns a plot that moved the sheet.
- One person saves a document at a time. `id`, `author`, `createdAt` and `updatedAt` let an application merge two
  documents item by item if it wants, but the format has no log of operations and no records of deletions.

### Revisions

`carry` takes a document, the drawing it was made on and a new revision, and returns the document for the new revision.
For each view that has items, it compares the scene of that view with the scene of the same view of the new revision, or
of the view that the caller's map of views names (`compare_scenes`, with the options of the caller), and:

1. moves every item of the view by the offset of the comparison;
2. replaces each id in `entities` by the id of its match in the new revision (unchanged or changed), and drops the ids
   of removed entities;
3. sets `carried` to `{ "from": "<sha256 of the old bytes>", "state": … }`, where `state` is `removed` when every entity
   the item named was removed (or its view is gone), `changed` when one of them changed or was removed or the item's box
   overlaps a region that changed, and `unchanged` otherwise.

`drawing` then names the new bytes. Items are never moved with the entities that changed under them; that would be a
guess. The application shows `changed` and `removed` items for someone to look at.

### Where it lives

| Package or crate | What |
| --- | --- |
| `@zumenjs/core` | The types (`MarkupDocument`, `MarkupItem` as a union by `kind`, `MarkupKind`, `MarkupStatus`, `MarkupArrows`, `CarriedState`), `createMarkup(drawing)`, `markupFromJson` (checks keys and values, keeps unknown keys, and throws a `MarkupError` with a `code` and naming the item and the key) and `markupToJson`, `markupShapes(item, { color, pixel })` (the shapes drawn, in paper millimeters, as primitives of the scene model; `pixel` is the paper size of a pixel of the parts drawn at a fixed size on screen), `markupBox(item)`, and `drawingHash(bytes)` (SHA-256 with Web Crypto, typed without the DOM library) |
| `zumen-core` (`markup`) | The same types with serde and the same JSON, the checks, `shapes`, `carry_view` (one view, given the two scenes and their comparison), and the scene entities of a view's items for `zumen-render` |
| `zumen` | `carry_markup(document, before, after, options)` over the bytes of both revisions, every view with items; the options are those of the comparison and the map of views |
| `@zumenjs/viewer` | `MarkupLayer`, built only on the public API of `DrawingViewer` (what it lacks is added to that API): the document and the selected and hovered items in a store (`getState`, `subscribe`), a `change` event with the document and the ids added, updated and removed, so that the application saves it, `add`, `update`, `remove`, `select`, `undo`, `redo`, `setDocument`, `setFilter` and `itemsAt`. The tools are factories like `selectTool`: `redlineTool` (`line`, `polyline`, `freehand`, `rectangle`), `cloudTool` (`rectangle`, `polygon`), `textTool`, `findingTool`, `photoTool` and `markupEditTool` (select, move, move a point, delete). A tool that creates an item passes the draft to the application's `create` callback, which fills in the text and fields with its own interface and returns the item, or `null` to cancel; ids, `author` and the times come from the layer's options. The entity under a new finding becomes its anchor. Colors come from `--zumen-markup-dark` and `-light`, per category from `--zumen-markup-category-<category>-dark` and `-light`, and for the selection and the carried states from their own properties (`--zumen-markup-selection-*`, `-changed-*` and `-removed-*`, which the infix `category-` keeps apart from any category); `setStyle` overrides them |
| `@zumenjs/react` | `useMarkupLayer(viewer, options)`, `useMarkupState(layer, selector)` and `useMarkupEvent(layer, type, listener)` |
| Playground | A markup sample, written only against the public API: tools, a list of items, a panel for a finding's text and status, saving and opening a document |

Browsers get `carry` through `@zumenjs/reader` when the comparison of revisions does (step 5 of [decision
0005](0005-browser-packages-for-embedding.md)), not with the viewer.

### Python, the command line and the MCP server

- Python: `zumen.read_markup(source)` (checked, returning `TypedDict` classes that are part of the API),
  `zumen.markup_schema()` (the JSON Schema, like `draft_schema`), `zumen.drawing_hash(source)`,
  `zumen.carry_markup(markup, before, after, ...)`, and `markup=` on the `render_*` methods of `Scene`, with the colors
  of categories passed in (red by default).
- The command line: `zumen markup carry <markup> <before> <after>` writes the carried document, and `zumen render
  --markup <file>` draws it over the drawing.
- The MCP server: `render_drawing` takes `markup`, a path; `read_markup` (read only) lists the items of a view with
  their kind, box, text, status, carried state and entities, and whether the hash matches the drawing; `write_markup`
  adds, updates and removes items in a document under the roots, creating it for a drawing, so that an agent can leave
  its findings on the drawing for people to see; and `carry_markup` writes the carried document to a new path and
  returns the number of items in each state. The two that write follow `draft_drawing`.

## Consequences

- Review results travel as one small JSON file per drawing, for every format zumen reads, PDF included, and the drawing
  is never changed. People who open the drawing in a CAD program do not see the markup; they see it in an application
  built on zumen, or in the PNG, SVG and PDF that `zumen render --markup` writes.
- Writing markup into drawing files or reading other markup formats (PDF annotations, BCF) needs a decision of its own.
- An item drawn in a scaled part of the sheet is measured in real-world millimeters through the placement of the scene,
  like any point; markup stores paper millimeters only.
- Model space without a sheet is a coordinate system of the bytes (the shift of `viewOrigin` depends on the extents).
  Markup made there is carried like any other; its points mean nothing to another file without carrying.
- The format has a schema of its own, described in `docs/markup.md` with the rules of the shapes. The TypeScript side
  writes the shapes of the fixtures in `tests/fixtures/markup/` (`pnpm run golden`), and the Rust side reads the same
  documents and gives the same shapes, as for scenes. SHA-256 in Rust needs a crate (`sha2`, MIT OR Apache-2.0), to be
  recorded in the architecture notes.
- The work is in milestone 8: the model in `@zumenjs/core`, `MarkupLayer` with its tools, the hooks, the sample and
  `docs/markup.md` ([#86](https://github.com/zumenjs/zumen/issues/86)); then the model in `zumen-core`, carrying,
  drawing on servers, Python, the command line and the MCP server ([#93](https://github.com/zumenjs/zumen/issues/93)).

## References

- [Decision 0002](0002-pdf-and-parapdf.md), [decision 0005](0005-browser-packages-for-embedding.md), [decision
  0017](0017-add-annotations-and-definitions.md), [decision 0020](0020-pdf-drawings-in-rust.md)
- [Entity ids](../architecture.md#entity-ids), [Extension points for
  applications](../architecture.md#extension-points-for-applications) and the comparison of revisions in [Texts,
  entities and images for servers](../architecture.md#texts-entities-and-images-for-servers)
- [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) (dates and times), [FIPS
  180-4](https://csrc.nist.gov/pubs/fips/180-4/upd1/final) (SHA-256), [Web Crypto
  API](https://www.w3.org/TR/WebCryptoAPI/) (`SubtleCrypto.digest`)
