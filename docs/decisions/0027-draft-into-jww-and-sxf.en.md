# 0027: Draw a drawing spec into JWW and SXF templates through their edits, with views as layer groups and sub-figures

> English reference preserved when Japanese became the primary documentation. See [0027-draft-into-jww-and-sxf.md](0027-draft-into-jww-and-sxf.md) for the Japanese guide and current policy.

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Proposed |

## Context

- `zumen::draft_into` ([decision 0018](0018-drafting-kit.md)) draws a drawing spec into a drawing received from
  someone else: the drafting kit draws the spec into the AutoCAD drawing model, `zumen_draft::plan` turns what it drew
  into the operations of [decision 0017](0017-add-annotations-and-definitions.md), and `zumen::edit` applies them, so
  the template keeps its bytes, entities and everything zumen does not read. Only DXF and DWG templates were accepted,
  although most drawings exchanged in Japan are JWW and SXF.
- [Decision 0025](0025-annotations-in-jww-and-sxf-edits.md) let JWW and SXF edits add block definitions, dimensions
  (no angular ones in JWW) and hatches. Both still refuse the style operations (`add-text-style`,
  `add-dimension-style`, `add-line-type`), layouts, attributes, leaders and images.
- What the drafted model holds and these formats lack:
  - **One sheet** per file. Its paper is the header's paper size in JWW (A0–A4 and 2A–5A, landscape) and the
    drawing_sheet_feature in SXF (A0–A4 upright or across, or a free size). No operation changes either.
  - **Scales.** A JWW drawing has 16 layer groups, each with a scale; coordinates are paper millimeters from the
    center of the sheet times the scale of the entity's group, while text sizes and the scales of block references
    are paper values ([How JWW is handled](../formats/jww.md#editing-edit_jww)). An SXF sub-figure (部分図) holds its
    elements at real size and is placed on the sheet with a position and a scale; it is what a viewport is to model
    space ([decision 0008](0008-sheets-and-scales-in-the-autocad-model.md),
    [decision 0010](0010-write-sxf-from-the-autocad-model.md)). The SXF editor adds elements into an existing
    composite figure (`figure:<name>`) but has no operation that adds a sub-figure.
  - **Layers.** JWW has 16 layers in each group. A layer's name belongs to its group, and Jw_cad templates repeat
    names across groups, but the JWW editor refuses a name that any group has. SXF layers are names only.
  - **No BYLAYER or BYBLOCK** (JWW refuses them, SXF writes black), and no line types can be added, so the kit's
    line types (`DASHED`, `CENTER`…) have to become ones the drawing has: Jw_cad's 2–9 and, from version 4.20, the
    SXF line types 31–62 in JWW; the predefined SXF line types and the drawing's own in SXF.
  - **Text** is one line placed by the lower left of its box, without alignment. Both formats draw the glyph's em at
    the height of the box, and the height number is kept between formats
    ([decision 0019](0019-text-heights-between-formats.md)).
  - **No polylines** in JWW, no arcs or widths in SXF polylines, and **no attributes** in either.

## Options

**How the drafted model reaches the template**

1. **Map it with `dxf_to_jww` or `dxf_to_sxf`** (what `draft` writes) and copy the result into the template. The
   result would match `draft`, but dimensions and hatches would arrive as the lines, solids and texts that these
   mappings draw, and the copy would bypass the editors, their checks and their in-place writers.
2. **Turn it into edit operations**, as for DXF and DWG, and adapt what each format cannot hold in the planner.

**JWW views**

- Draw every view into one group, scaled down to paper: real coordinates are lost.
- **A layer group for each scale**: the views' items at real size, in a group whose scale is the view's. Either always
  a group without entities, or first a group that already has the scale.

**SXF views**

- On the sheet, scaled down: real coordinates are lost.
- **Drawing parts** (作図部品, kind 4) placed at 1/scale: real coordinates stay, but a view is not a part, and SXF
  readers show the scale of sub-figures, not of parts.
- **Sub-figures** (部分図, kind 1), as `dxf_to_sxf` writes viewports, through a new operation.

**JWW layer names**: leave the layers added for other groups unnamed, give them other names, or let the editor keep
names per group.

**Line types**: draw them solid, refuse them, or use the nearest line type of the drawing.

**Paper**: add an operation that changes it, refuse a spec whose `paper` differs, or keep the template's and report it.

**Attributes**: leave them out, or write their values as text.

## Decision

Option 2. `zumen_draft::plan_jww` and `zumen_draft::plan_sxf` turn the drafted model into operations for a JWW or SXF
template, and `zumen::draft_into` (the CLI's `--into`, Python's `draft_into` and the MCP tool's `into`) accepts JWW
and SXF (SFC, P21, SFZ, P2Z) templates. JWC and JWS are still refused: they cannot be edited.

1. **Sheets.** The template has one sheet, so one sheet of the spec is drawn: the one named by `sheet`, or the first.
   The others are reported (`draft-into-sheets`). The sheet is drawn on the template's paper (an SXF template without
   a sheet takes the spec's). A `paper` or `orientation` that differs from it is reported as a warning
   (`draft-into-paper`) and not applied; nothing can change the paper of a JWW or SXF drawing yet.
2. **JWW views.** Sheet items go into a layer group of scale 1 and each view into a group with the view's scale: the
   write layer group when its scale matches, else the first other group with that scale, else the first group without
   entities, whose scale an `update-layer` sets. Groups that are hidden or display-only are not used. Views at the same
   scale share a group, and a sheet or view with nothing to draw takes none. Coordinates are moved so that the sheet's
   lower left corner is where it is on the JWW sheet, and a view's model center appears at the center of its viewport;
   text heights and widths are divided by the group's scale, as JWW keeps them in paper millimeters. No group left is an
   error (`invalid-value`, with the place of the view).
3. **SXF views.** The SXF editor gains `add` of kind `viewport` (on the sheet only): it adds an empty sub-figure in
   mathematical coordinates named `name` (`部分図 <n>` when left out) and places it so that `viewCenter` appears at
   `center` at 1:`scale`, the keys of a DXF viewport. `width` and `height` are taken for the same JSON, but SXF does not
   clip sub-figures. The planner adds one for each view with items, named after the view (made unique), and draws the
   view's items into it with `"container": "figure:<name>"` at real size.
4. **Viewports do not clip** in either format. A view whose drawn extent passes its viewport is reported
   (`draft-into-view-clip`). Since each view has its own group or sub-figure, views do not show each other's items,
   and `view-overlap` is not reported.
5. **Layers.** Each layer of the spec used by an item becomes a layer of the item's group (JWW) or a layer of the
   drawing (SXF), reusing one of the same name (layer `0` too, not the template's current layer). A new JWW layer is
   the first layer of the group that has no name (or its own number as its name) and no entities, named with
   `update-layer`; the JWW editor now keeps layer names unique within a layer group, as Jw_cad does, and a name in
   an `add` with `"container": "group:<g>"` is looked up in that group first. New layers take `visible` and, in JWW,
   `locked`; values the format's layers lack (`plot`, and `locked` in SXF) are reported
   (`draft-into-layer-attributes`). Colors, line types and line widths are written on each item, resolved through
   BYLAYER and BYBLOCK: ACI 7 is black, other colors are `#rrggbb`, which the editors match or add.
6. **Line types** become the drawing's line type with the same dash and gap lengths in paper millimeters, else the
   one whose period, share of dashes and number of parts are nearest (the measure `dxf_to_jww` uses when its
   user-defined line types run out), reported once for each line type (`draft-into-line-type`). JWW looks at line
   types 2–9 and, from version 4.20, 31–62; SXF at its predefined line types and the drawing's.
7. **Text.** A text becomes one text per line, whose box (the TEXT height, and characters as wide as the height times
   the width factor, half-width ones half of that) is placed by the spec's alignment, with its lower edge 0.12 of the
   height below the baseline, as `dxf_to_jww` places boxes (which keeps instead the left end of the wider glyphs that
   zumen draws for AutoCAD text, so a centered text drifts left). Multiline text is split at its lines with the scene's
   line spacing. Values are the displayed strings, without DXF control codes.
8. **Other items.** Polylines become lines and arcs in JWW, and polylines and arcs in SXF; a polyline with a width
   becomes a solid fill per segment (a filled circle is two half disks). Solids and hatches become `hatch`, the block
   definitions that the sheet places `add-block` (the template holds this sheet only, so the blocks of other sheets
   are left out), block references `block-reference`. A block's entities go on the layers of the group of
   its first reference, which also gives BYBLOCK values and layer `0`; references that would give other values are
   reported (`draft-into-block-inherit`). A JWW block reference's scale is multiplied by the ratio of the scales of
   those groups. A block the template has is used as defined there (`draft-into-block-exists`, as for DXF).
9. **Attributes.** The values of block references are drawn as text where the scene draws them, and attribute
   definitions are not written (`draft-into-attributes`). `fields` fill the spec's own block references, and fields
   that no reference has are reported (`unused-field`).
10. **Dimensions** pass `dimensionStyle`, so the editors report that the format has no dimension styles
    (`edit-dimension-style`). JWW dimensions follow the template's dimension settings, value format included, as
    Jw_cad's own do. SXF has no such settings, so SXF dimensions get the value as the spec formats it (decimals,
    thousands separators, `R`, `φ` before diameters, which CP932 can write, and `°`). JWW angular dimensions, which a
    dimension figure cannot hold, are drawn as the lines, arcs, solids and text of their block
    (`draft-into-angular-dimension`).

## Consequences

- A spec can be drawn into JWW and SXF drawings received from others without rewriting them; the template's records
  and features keep their bytes, and the result reads back with views, dimensions, hatches and blocks.
- JWW and SXF results differ from DXF ones in what the formats decide: JWW dimensions are sized by the template's
  settings and SXF ones by 0025's fixed values, line types are the nearest existing ones until the editors can add
  line types, the paper stays the template's, and nothing is clipped. When the style operations come to these
  editors, the planner can add the spec's line types and dimension looks instead.
- JWW drafting uses up to one layer group for each scale and a layer for each layer name in each group. A template
  without free groups or layers cannot take the spec (an error, not a partial drawing).
- Two edit behaviors change: the JWW editor allows a layer name that another group has (it refused it before), and
  the SXF editor adds viewports as sub-figures. Both only accept what was refused before.
- `zumen-draft` depends on `zumen-jww` and `zumen-sxf`, to read the layer groups, layers, line types and composite
  figures of a template. It still writes nothing itself: the operations go through the editors.
- JWW and SXF texts are placed by their box, so a centered text is centered in Jw_cad and SXF readers, which do not
  use the AutoCAD glyph widths that `draft` keeps when it maps texts with `dxf_to_jww` and `dxf_to_sxf`.
- The results have not been opened in Jw_cad or checked with an SXF browser yet; that belongs to the checks on a
  Windows host ([decision 0014](0014-verify-on-native-windows.md)).

## References

- Jw_cad, [jwdatafmt.txt](https://www.jwcad.net/jwdatafmt.txt) (Jw_cad 7.02): レイヤグループ (縮尺, 状態,
  書込みレイヤ), レイヤ名, 用紙サイズ, 線種, CDataMoji, CDataBlock, CDataList
- MLIT, SXF Ver.3.1 仕様書・同解説: 附属書 SFC 編 1-2 (drawing_sheet_feature, sfig_org_feature,
  sfig_locate_feature, text_string_feature), フィーチャ仕様編 3-5 (複合図形の種別と配置)
- [Decision 0008](0008-sheets-and-scales-in-the-autocad-model.md),
  [decision 0010](0010-write-sxf-from-the-autocad-model.md),
  [decision 0012](0012-edit-in-place-and-write-every-format.md),
  [decision 0017](0017-add-annotations-and-definitions.md), [decision 0018](0018-drafting-kit.md),
  [decision 0019](0019-text-heights-between-formats.md), [decision 0023](0023-resize-existing-layouts.md),
  [decision 0025](0025-annotations-in-jww-and-sxf-edits.md)
- [Drawing into a template](../drafting.md#drawing-into-a-template)
