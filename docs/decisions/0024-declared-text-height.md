# 0024: Keep the declared text height with each text primitive

| Item | Value |
| --- | --- |
| Date | 2026-10-23 |
| Status | Accepted |

## Context

The height used to draw a glyph is not always the height a drawing declares. DXF and DWG declare capital heights,
while the renderer uses a larger em. The standards check inferred this convention from an entity's `height`
property, as recorded in decision 0015. A scene containing only dimensions loses that property when their parts
are merged, so a valid 2.5 mm dimension text was incorrectly reported as 3.47 mm. Adding an unrelated text changed
the check result.

## Options

1. Infer the convention from the scene format or from other entities. This puts format knowledge in core or makes
   one entity's result depend on another.
2. Add a declared height to every entity's properties. A dimension or formatted text may contain several heights,
   so an entity-level value cannot describe all of its parts.
3. Keep the declared height with each text primitive, separate from its drawing height.

## Decision

Use option 3. `TextPrimitive.characterHeight` (`character_height` in Rust) is optional and uses the same local
coordinates as `height`. When absent, checks use `height`. DXF and DWG scene conversion sets it from each text run's
declared height, including dimension parts and stacked text. Renderers continue to use `height` and do not change.

This replaces only the text-height inference described in decision 0015. Core checks remain format-neutral.

## Consequences

The scene JSON gains an optional field. Existing application-created scenes retain their previous height
convention, and entity ids and drawing bytes do not change. TypeScript reference scenes and Rust scenes carry the
same value. Golden scenes are regenerated from the TypeScript readers and stable ids are verified separately.

## References

- [Decision 0015](0015-analysis-in-zumen-core.md)
- [Checks against CAD standards](../checks.md)
- DXF Reference, TEXT group code 40 and MTEXT formatting codes (`\\H`)
