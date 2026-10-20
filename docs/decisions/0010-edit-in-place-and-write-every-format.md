# 0010: Edit drawings in place, keeping everything an operation does not name, and write every format zumen reads

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

- Applications that hand drawings to people and to AI agents also change them: they add a line or a note, move,
  recolor or delete an entity, add a layer, or start a new drawing. zumen reads SXF, JWW, DXF and DWG, but writes only
  AutoCAD 2013 DXF from the AutoCAD drawing model ([decision 0006](0006-write-through-the-autocad-model.md)). An
  application that edits needs other libraries: ezdxf for DXF, and a proprietary converter to turn DWG into DXF and
  back.
- The AutoCAD drawing model (`DxfDocument`) is made for drawing and conversion. It does not hold extended data
  (XDATA), extension dictionaries, most dimension style variables, objects such as groups and xrecords, or the
  entities that zumen does not read (REGION, 3DSOLID, proxies). A drawing written from it loses them. A conversion
  reports this ([decision 0006](0006-write-through-the-autocad-model.md)), but an edit of a drawing received from
  someone else must not lose what the edit did not touch.
- Agents choose entities by the ids of the scene, read values by the stable keys of
  [decision 0005](0005-browser-packages-for-embedding.md) and coordinates in the drawing's own coordinates
  ([decision 0009](0009-geometry-for-ai-clients.md)). Edits need the same names and coordinates, need to fail as a
  whole with a precise reason, and need to say what they added.

## Options

1. **Edit the model and write it.** One writer per format, and edits are simple, but every edit loses what the model
   does not hold, and the version becomes the writer's.
2. **Edit the file in place.** Keep the file as it was and change only what an operation names, in the unit that each
   format is made of: DXF tags, DWG objects, SFC and P21 instances, JWW records. Each format needs its own editor, but
   everything an operation does not name stays as it was, including what zumen cannot read.
3. **Extend the model until it holds everything.** Every reader in both languages and every golden file would change,
   and what is unknown would still be lost.

## Decision

Two ways of writing:

- **In-place editing** (`zumen::edit`, `zumen.edit` in Python, `zumen edit`) is option 2. The format, the version, the
  text encoding and every part that no operation names are kept: a DXF keeps every tag that an operation does not
  change byte for byte, so its extended data, reactors, extension dictionaries, objects and unread entities survive.
  All operations are checked before anything is written; if one fails, nothing is written and the error names the
  operation.
- **New drawings and conversions** (`zumen::create`, `zumen::convert`) write from the AutoCAD drawing model
  ([decision 0006](0006-write-through-the-autocad-model.md)): AutoCAD 2013 for DXF and DWG, SXF Ver.3.1 for SFC and
  P21, and a version that Jw_cad 8 opens for JWW. `create` writes an empty drawing of the format and applies the
  operations with the in-place editor, so adding to a new drawing and to an existing one is the same code.

The operations are a JSON array, the same for every format:

```json
[
  {"op": "add", "kind": "line", "layer": "A-WALL", "start": [0, 0], "end": [5000, 0]},
  {"op": "add", "kind": "text", "container": "layout:Layout1", "position": [100, 200], "text": "確認済み", "height": 3.5},
  {"op": "update", "id": "3F2", "set": {"end": [5200, 0], "layer": "A-WALL-2", "color": "#ff0000"}},
  {"op": "delete", "id": "3F5"},
  {"op": "add-layer", "name": "CHECK", "color": 1, "lineType": "DASHED"},
  {"op": "update-layer", "name": "CHECK", "set": {"visible": false}}
]
```

- `kind` is a name of `EntityKind` ([decision 0005](0005-browser-packages-for-embedding.md)), or `block-reference`
  for a placement of a block, a sub-figure or a part, which the scene resolves and has no kind for.
- Values use the keys of the properties (`start`, `center`, `radius`, `angle`, `position`, `text`, `height`, `layer`,
  `color`, `lineType`, `lineWidth`, `textStyle`, `scale`, `block`), and a few keys of their own where the scene only
  counts (`vertices`, `bulges`, `closed`, `width`, `majorAxis`, `ratio`, `parameter`, `rotation`, `wrapWidth`).
  Coordinates are the drawing's own, the numbers that the `raw` of a property gives: a DXF entity in drawing units,
  in the space, block or object coordinate system where it is stored. They are not paper millimeters.
- `id` is the id of the scene. An id inside a block reference (`…>…`) is refused, because changing the entity would
  change the block definition and every reference to it; the error says which ids change the reference and the
  definition. An id seen through a viewport (`…:…`) is the entity itself.
- `container` says where `add` puts an entity: `model` (the default), `layout:<name>` or `block:<name>` in DXF and
  DWG. SXF and JWW define their own names when their editors come.
- `color` is a color number of the format (ACI 1 to 255 in DXF and DWG), `"#rrggbb"`, `"bylayer"` or `"byblock"`;
  `lineWidth` is millimeters, `"bylayer"`, `"byblock"` or `"default"`. A value the format cannot hold exactly is
  written as the nearest one and reported as a notice.
- A layer that an entity names but the drawing lacks is added with default values and reported as a notice.
- The result is the bytes, the ids of what `add` and `add-layer` created (in the order of the operations) and the
  notices. An error has a stable `code` (`invalid-operations`, `unknown-kind`, `unsupported-kind`, `unknown-key`,
  `invalid-value`, `not-found`, `inside-block`, `missing-definition`, `already-exists`, `unsupported-format`,
  `unreadable`, `too-large`), a message for people and the number of the operation.
- Checking that the file is the version the agent read (a checksum) is left to the application, which stores it.

DXF comes first (R12 to 2018, ASCII and binary). DWG, SXF and JWW follow in their own crates with the same operations
and the same conformance test (`crates/zumen/tests/edit.rs`), each writing its own format first.

## Consequences

- An edit keeps what zumen cannot read, so an application can edit drawings that zumen draws only in part.
- Each format has limits of its own, refused with `unsupported-kind`: R12 DXF has no ELLIPSE, MTEXT or lineweights,
  and a multi-line attribute cannot be changed.
- Deleting an entity also deletes the objects it owns (its extension dictionary) and removes it from draw orders
  (SORTENTSTABLE), groups and reactors. Other references to it, such as the boundary of an associative hatch, stay;
  AutoCAD's audit removes them.
- The conformance test runs every editable fixture: an edit without operations gives the same bytes, and an edit that
  adds, changes and deletes changes exactly those entities, compared in the drawing's own coordinates because the
  sheet moves when the extents change. ezdxf audits edited DXF in the Python tests. DWG, SXF and JWW will also be
  checked with ODA File Converter, DWG TrueView, the SXF Browser and Jw_cad.
- The browser packages do not edit yet, and the MCP server stays read-only
  ([decision 0004](0004-mcp-server-in-the-python-package.md)).

## References

- [Autodesk DXF Reference](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-235B22E0-A567-4CF6-92D3-38A2306D73F3.htm)
  (AutoCAD 2026): Common Group Codes for Entities, the entity sections, TABLES Section, Storage of String Values
- [How DXF is handled: Editing](../formats/dxf.md#editing-edit_dxf)
