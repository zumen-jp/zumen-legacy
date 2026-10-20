# 0007: Map SXF and JWW into the AutoCAD drawing model in a crate of its own, zumen-convert

| Item | Value |
| --- | --- |
| Date | 2026-10-20 |
| Status | Accepted |

## Context

[Decision 0006](0006-write-through-the-autocad-model.md) writes every output from the AutoCAD drawing model
(`DxfDocument` of `zumen-dxf`). DXF and DWG already read into it. SXF and JWW have their own models (`SxfDrawing`,
`JwwDocument`) and have to be mapped into it before they can be written as DXF, and later writers of SFC, P21 and JWW
will map the other way. The mapping needs to know both sides: the SXF and JWW tables (predefined colors, line types and
widths, pen colors, layer groups) and the rules of their scenes, and the AutoCAD model.

The dependency rules ([architecture](../architecture.md#packages-and-dependency-rules)) keep format crates independent:
each depends on `zumen-core` only, as the TypeScript packages do. The one exception is `zumen-dwg`, which reads DWG
into the model of `zumen-dxf` because DWG and DXF are two encodings of the same database.

## Options

1. **In the `zumen` crate**, next to `convert`. No new crate, and `zumen` already depends on every format crate. But
   the facade crate, which so far only dispatches (`read`, `detect`, `convert`), would hold a few thousand lines of
   format knowledge, and the writers of SFC, P21 and JWW would follow it there.
2. **In each format crate**: `zumen-sxf` and `zumen-jww` depend on `zumen-dxf` and map their own model, as `zumen-dwg`
   does. The SXF and JWW knowledge stays private to its crate. But two more format crates would depend on another
   format crate, so reading SXF alone would build the DXF parser, scene and writer, and the rule that format crates
   stand alone would no longer hold for most of them.
3. **A new crate, `zumen-convert`**, that depends on `zumen-core`, `zumen-dxf`, `zumen-sxf` and `zumen-jww` and holds
   the mappings (`sxf_to_dxf`, `jww_to_dxf`). Format crates stay readers of one format each. The SXF and JWW crates
   expose what the mapping needs: their code tables, and the rules that turn codes into colors, line types, widths and
   text layout, which their scenes use as well.
4. **Move the AutoCAD model into a crate of its own** that every format crate may depend on. It would be the cleanest
   hub, but it splits `zumen-dxf` (the model, the parser and the scene share helpers and the TypeScript port mirrors
   `@zumenjs/dxf`), which is a large change for the first mapping.

## Decision

Option 3. `zumen-convert` maps SXF and JWW into the AutoCAD model, and `zumen::convert` reads the drawing, maps it when
it is SXF or JWW, and writes it with `zumen_dxf::write_dxf`. `zumen` re-exports the crate as `zumen::mapping`.

- The mapping follows the scene of each format, value for value, so that the written DXF draws what the SXF or JWW
  scene draws. To keep one rule for both, the code tables and the rules that the scenes used privately are now public
  in the format crates and used by the scenes: `SxfDocument::color`, `dashes`, `width` and `font`,
  `Annotation::layout_input`, `zumen_sxf::tables`, `arrow_primitives` and `marker_primitives`; `JwwDocument::pen_color`,
  `line_dashes`, `line_width`, `group_scale`, `layer_name`, `paper_size` and `text_layout_input` in `zumen-jww`.
- The mapping returns the model, the notices of what it could not map as it was, and the source of each entity it
  wrote (handle to the SXF instance number or the JWW entity number), so that tests and applications can trace a
  written entity back.

## Consequences

- Format crates still read one format each and do not depend on each other (except `zumen-dwg`). `zumen-convert` is
  the one place that knows two formats at once, and the writers of SFC, P21 and JWW from the model will go there too.
- The SXF and JWW crates have a larger public surface: the rules above are now API. Changing them changes both the
  scene and the conversion, which is the point.
- The mapping has no TypeScript counterpart, like the DXF writer. The browser packages do not convert.

## References

- [Decision 0006: Write drawings through the AutoCAD drawing model](0006-write-through-the-autocad-model.md)
- [Decision 0008: Real-size model space, and the sheet as a layout with a viewport for each scale](0008-sheets-and-scales-in-the-autocad-model.md)
