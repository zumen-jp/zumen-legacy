# @zumenjs/stb

Reads ST-Bridge structural models (`.stb`, the Japanese standard XML format for exchanging structural design data,
versions 2.0 and 2.1) for [zumen](https://github.com/zumenjs/zumen), and shows their members in 3D with three.js and React.
Parsing is written in TypeScript without dependencies and works in browsers and Node.js. three.js and React are optional
peer dependencies, needed only for the views.

```tsx
"use client";

import { StbViewer, useStbFile } from "@zumenjs/stb/react";

export function StructureView({ file }: { file: File | null }) {
  const { model, error, loading } = useStbFile(file);
  if (loading) return <p>Loading…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return model ? <StbViewer model={model} style={{ height: 640 }} /> : null;
}
```

Without React, read a model with `readStb` and draw it with `StructureView` from `@zumenjs/stb/three`.
Parsing alone does not need three.js. TypeScript users of `./three` also install `@types/three`.

```ts
import { isStb, readStb } from "@zumenjs/stb";
import { StructureView } from "@zumenjs/stb/three";

const bytes = new Uint8Array(await file.arrayBuffer());
if (isStb(bytes)) {
  // isStb looks at the contents (an ST_BRIDGE root element), not the file name
  const model = readStb(bytes);
  console.log(model.version, model.members.length, model.notices);

  const view = new StructureView(document.querySelector("#stage") as HTMLElement);
  view.setModel(model);
  view.canvas.addEventListener("click", (event) => console.log(view.pick(event.clientX, event.clientY)?.properties));
}
```

- Columns, posts, girders, beams, braces, slabs, walls, footings, strip footings, piles, foundation columns and parapets
  are extruded from their cross sections: H, box, pipe, T, channel, angle and lipped channel steel (including doubled
  sections), flat and round bars, rectangular and circular RC and SRC, CFT, tapered and haunched beams, section changes
  at joints, and openings in walls and slabs.
- Stories and grid axes (parallel, arc, radial and drawing axes) are drawn as named lines. Filtering by a story gives a
  plan seen from above, and filtering by an axis gives an elevation.
- Picking a member shows its kind, name, section, materials (steel grade, and concrete strength with where it came from),
  story, nodes, length, rotation, offsets and the original XML attributes.
- `StructureModel` uses global coordinates in millimeters with Z up. Each member keeps its solid (a 2D section outline
  with an extrusion direction, or a list of 3D outlines), its properties and its original attributes. Members with the
  same section share the outline.
- Content that is not drawn (reinforcement, joints and welds, beam penetrations, seismic isolation and damping devices)
  or that was inferred is reported in `model.notices` with its kind and count. `readStb` throws for ST-Bridge 1.x.

How the format is handled, including the rules inferred from the specification's figures, is described in
[docs/formats/st-bridge.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/st-bridge.md) (Japanese).
This package is not certified by buildingSMART Japan and does not replace structural calculation or code compliance
checks. See THIRD_PARTY_NOTICES.md.
