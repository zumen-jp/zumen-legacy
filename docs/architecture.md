# Architecture

zumen is a pnpm workspace with one package per drawing format, a format-neutral drawing engine,
and a thin React layer. A drawing flows through the packages like this:

```text
bytes ─▶ format reader ─▶ native model ─▶ toScene ─▶ Scene ─▶ DrawingViewer (Canvas) ─▶ React UI
         (@zumenjs/sxf,     (SxfDrawing,              (@zumenjs/core)                     (@zumenjs/react)
          @zumenjs/jww,      JwwDocument,
          @zumenjs/dxf)      DxfDocument)
```

## Packages and dependency rules

| Package | Responsibility |
| --- | --- |
| `@zumenjs/core` | The `Scene` model and its types, `NoticeList`, `readDrawing` and the `DrawingReader` interface, the Canvas 2D renderer, hit testing and snapping (`HitIndex`), distance measurement (`measureDistance`, `toReal`), and `DrawingViewer`. Geometry helpers are exported from `@zumenjs/core/geometry` and text layout from `@zumenjs/core/text`. |
| `@zumenjs/sxf` | SFC and P21 parsing into the SXF feature model (`parseSfc`, `parseP21` → `SxfDrawing`), conversion to a scene (`sxfToScene`), and the `sxf` reader. |
| `@zumenjs/jww` | JWW parsing into the JWW document model (`readJww` → `JwwDocument`), conversion to a scene (`jwwToScene`), and the `jww` reader. |
| `@zumenjs/dxf` | DXF parsing (ASCII and binary) into the AutoCAD drawing model (`parseDxf` → `DxfDocument`), conversion of model space or a layout to a scene (`dxfToScene`), and the `dxf` reader. The model and the conversion are shared with the DWG reader. |
| `@zumenjs/react` | `useDrawingViewer` (the engine in React), `CadViewer` (a complete viewer), and `useCadFile`. |

1. Format packages depend only on `@zumenjs/core`, never on each other.
   For example, `@zumenjs/jww` keeps its own copy of the SXF-compatible colors and line types.
   The one exception is the planned `@zumenjs/dwg`: DXF and DWG are two encodings of the same AutoCAD drawing database,
   so the DWG reader produces a `DxfDocument` and draws it with `dxfToScene` from `@zumenjs/dxf`
   (see [How DXF is handled](formats/dxf.md#the-autocad-drawing-model)).
2. `@zumenjs/core` has no knowledge of any format. Formats are passed to `readDrawing` as `DrawingReader`s.
3. Parsing (`parse*`, `read*` and everything below them) does not use the DOM, React or the renderer.
   It uses only the types and `NoticeList` from `@zumenjs/core`. The `*ToScene` conversions use the geometry
   and text layout helpers.
4. Interaction (zoom, pan, selection, measurement) lives in `DrawingViewer`. The React layer only reads the
   viewer state and calls its methods.

## The scene model

- Coordinates are paper millimeters with the Y axis up. Placements of sub-figures, parts and blocks are resolved.
- Each entity has drawing primitives (`stroke`, `fill`, `clipped`, `dot`, `text`), a bounding box, its layers
  (its own and those of the placements it sits in), a list of properties to show, and, inside a scaled placement,
  a `Placement` with the scale and the transform back to the original (real-world) coordinates.
- `notices` list what was not drawn or was drawn differently, by kind and count. Nothing is dropped silently.
- Colors are `#rrggbb`, or `#rrggbbaa` with opacity. On a light background, near-white colors are drawn black,
  and on a dark background near-black colors are drawn white.
- A drawing can hold several views, such as DXF model space and layouts. `Scene.views` lists them and `Scene.view` is the one drawn;
  `readDrawing(bytes, readers, { view })` passes the choice to the reader (`DrawingReader.read(bytes, format, { view })`).
  Formats with a single view leave both out.

## Adding a format

1. Create a package that depends on `@zumenjs/core`.
2. Parse the bytes into a model that keeps the format's own structure, and report unsupported content with `NoticeList`.
3. Convert the model to a `Scene` with the helpers from `@zumenjs/core/geometry` and `@zumenjs/core/text`.
4. Export a `DrawingReader` with `formats`, `detect` (look at the contents, not the file name) and `read`.
5. Add the smallest synthetic test files that pin the rules that are easy to break without anyone noticing
   (encodings, units, nesting, scales). Do not commit real drawings.

## Extension points for applications

- `viewer.addOverlay(painter)` draws on top of the drawing (markup, photo positions, annotations).
  Call `viewer.invalidate()` when the overlay content changes.
- `viewer.toPaper(px, py)`, `viewer.toScreen(x, y)` and `toReal(placement, x, y)` convert between screen pixels,
  paper millimeters and real-world coordinates.
- `viewer.getState()` and `viewer.subscribe(listener)` expose the state to any UI (React uses `useSyncExternalStore`).
- `onSelect` reports entities the user picks.

## Development setup

- Each package's `exports` has an `@zumenjs/source` condition that points at `src`. The type checker
  (`customConditions` in `tsconfig.json`), Vitest and the Vite playground resolve it, so changes apply without a build.
  `publishConfig.exports` leaves it out of the published packages, which contain only `dist`.
- `pnpm run build` compiles the packages in dependency order (core, then sxf, jww and dxf, then react) into each `dist`.
- `pnpm run check` runs Biome, the type checker, the unit tests, `pnpm audit` and all builds.
