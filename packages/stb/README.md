# @zumenjs/stb

Reads ST-Bridge structural models (`.stb`, the Japanese standard XML format for exchanging structural design data,
versions 2.0 and 2.1) for [zumen](https://github.com/zumenjs/zumen), and shows their members in 3D with three.js and React.
Parsing is written in TypeScript without dependencies and works in browsers and Node.js. three.js and React are optional
peer dependencies, needed only for the 3D engine and its hooks.

```ts
import { isStb, readStb } from "@zumenjs/stb";

const bytes = new Uint8Array(await file.arrayBuffer());
if (isStb(bytes)) {
  // isStb looks at the contents (an ST_BRIDGE root element), not the file name
  const model = readStb(bytes);
  console.log(model.version, model.members.length, model.notices);
}
```

Parsing alone does not need three.js. TypeScript users of `./three` also install `@types/three`.

## The 3D engine

`StructureViewer` of `@zumenjs/stb/three` is the 3D engine to embed in an interface you build yourself, on the same
contract as the 2D `DrawingViewer` of [`@zumenjs/viewer`](../viewer). It has no text for people and ships no CSS:
buttons, panels and labels come from your application, and colors from CSS custom properties.

```ts
import { StructureViewer } from "@zumenjs/stb/three";

const viewer = new StructureViewer(element, model, { label: "Structure", handlers: { zoom: "modifier" } });
viewer.on("select", ({ selection }) => console.log(selection.map((member) => member.properties)));
viewer.on("moveend", ({ view }) => console.log(view.position, view.target));
viewer.focusStory(model.stories[0]?.id ?? null); // the members of a story, from above
```

The element needs a size; the viewer fills it with a WebGL canvas and a layer for the names of axes and stories, which
are styled on the elements themselves and need no stylesheet. The playground's
[3D sample](https://github.com/zumenjs/zumen/blob/main/playground/structure-sample.tsx) builds a toolbar, the filters,
properties and a status bar with this API only.

- **State.** `getState()` returns an object that is replaced on every change, and `subscribe(listener)` reports the
  changes: the model, the theme, the member kinds hidden (`setHiddenKinds`, `setKindsVisible`), the story or axis in
  focus (`focusStory(id)`, `focusAxis(key)`, which also move the camera unless `{ camera: false }`), the number of
  visible members, the selection (several members, `select(members)`), the hovered member, the projection
  (`setProjection`), the color mode (`setColorMode("kind" | "structure")`), edges, translucent slabs and walls, the
  guides shown (`setGuides({ axes, stories, labels, nodes })`) and the active tool. `setModel` replaces the model and
  resets the filters, the selection and the camera. Members hidden by a filter leave the selection.
- **Camera.** `getView()` returns the position, the target and the visible height at the target (`span`) in model
  millimeters, with the projection and the size; `jumpTo` takes them back unchanged, so a saved view is restored.
  `fit()`, `viewFrom("iso" | "top" | "front" | "side")` and `viewAxis(axis)` place the camera around the visible
  members. `move` is sent on every change and `moveend` when it is over, with the DOM event of the user's action
  (`null` for method calls); `project(point)` gives the screen position of a model point, for HTML placed on the model.
- **Events.** `move`, `moveend`, `select` (the selection and the DOM event that changed it) and `hover`.
- **Input handlers**, each can be turned off in the options or with `setHandlers`: `rotate` (left drag, one finger),
  `pan` (right drag, two fingers, arrow keys), `zoom` (wheel, middle drag and pinch: `"always"`, `"modifier"` for the
  wheel with Ctrl or ⌘ only, so the page scrolls otherwise, or `"never"`) and `keyboard` (`0` or `f` fits, Escape
  clears the selection, the arrow keys pan).
- **Tools.** The active tool receives pointer events in CSS pixels (`pointerDown`, `pointerMove`, `pointerUp`,
  `click`, `pointerLeave`) and key presses, and asks for the member under the pointer with `memberAt(px, py)`. The
  built-in `select` (click to select, Shift, Ctrl or ⌘ to add or remove) and `orbit` (camera only) use the same
  interface; `addTool` and `setTool` add and choose your own. A tool with `orbit: false` receives left-button drags
  instead of rotating.
- **Style.** CSS custom properties on the element, overridden by `setStyle`. The theme follows the system color scheme
  unless set with the `theme` option or `setTheme`.

| Property | Default (dark, light) | |
| --- | --- | --- |
| `--zumen-background-dark`, `-light` | `#1b1c1f`, `#f4f5f7` | Background |
| `--zumen-selection-dark`, `-light` | `#ffc94a`, `#ffb000` | Selected members |
| `--zumen-hover-dark`, `-light` | `none` | The hovered member |
| `--zumen-edge-dark`, `-light` | `#0c0d0f`, `#2b2f36` | Member edges |
| `--zumen-member-line-dark`, `-light` | `#ff6fbf`, `#c0187a` | Members drawn as lines (no section) |
| `--zumen-axis-dark`, `-light` | `#e08a8a`, `#7a2e2e` | Grid axes and the border of their names |
| `--zumen-story-dark`, `-light` | `#9aa3ad`, `#5b6470` | Story outlines |
| `--zumen-node-dark`, `-light` | `#f1f2f4`, `#1a1a1c` | Nodes |
| `--zumen-label-text-dark`, `-light` | `#f1f2f4`, `#1a1a1c` | Axis names |
| `--zumen-label-background-dark`, `-light` | `#1f2023`, `#ffffff` | Behind axis and story names |
| `--zumen-story-label-text-dark`, `-light` | `#b6b9bf`, `#55585e` | Story names |
| `--zumen-member-<kind>` | | The color of a member kind (`column`, `strip-footing` …) |
| `--zumen-structure-<kind>` | | The color of a structure kind (`rc`, `s`, `src` …) |

### React

`@zumenjs/stb/react` has hooks only: `useStructureViewer(model, options)` creates the engine on the element that
receives `ref` and destroys it when the element goes (a new `model` calls `setModel`, a new `label` the canvas label),
`useStructureState(viewer, selector, isEqual?)` re-renders only when the selected part of the state changes,
`useStructureEvent(viewer, type, listener)` subscribes to an event, and `useStbFile(file)` reads a file.

```tsx
"use client";

import type { StructureModel } from "@zumenjs/stb";
import { useStbFile, useStructureState, useStructureViewer } from "@zumenjs/stb/react";

export function StructureView({ file }: { file: File | null }) {
  const { model, error } = useStbFile(file);
  if (error) return <p role="alert">{error.message}</p>;
  return model ? <Structure model={model} /> : null;
}

function Structure({ model }: { model: StructureModel }) {
  const { ref, viewer } = useStructureViewer(model, { label: "Structure" });
  const selected = useStructureState(viewer, (state) => state.selection.at(-1));
  return (
    <>
      <button type="button" onClick={() => viewer?.viewFrom("top")}>Plan</button>
      <div ref={ref} style={{ height: 640 }} />
      <p>{selected?.name}</p>
    </>
  );
}
```

## What is read

- Columns, posts, girders, beams, braces, slabs, walls, footings, strip footings, piles, foundation columns and parapets
  are extruded from their cross sections: H, box, pipe, T, channel, angle and lipped channel steel (including doubled
  sections), flat and round bars, rectangular and circular RC and SRC, CFT, tapered and haunched beams, section changes
  at joints, and openings in walls and slabs.
- Stories and grid axes (parallel, arc, radial and drawing axes) are drawn as named lines. Filtering by a story gives a
  plan seen from above, and filtering by an axis gives an elevation.
- Each member carries, for the interface to show when it is picked, its kind, name, section, materials (steel grade, and
  concrete strength with where it came from), story, nodes, length, rotation, offsets and the original XML attributes.
- `StructureModel` uses global coordinates in millimeters with Z up. Each member keeps its solid (a 2D section outline
  with an extrusion direction, or a list of 3D outlines), its properties and its original attributes. Members with the
  same section share the outline.
- Content that is not drawn (reinforcement, joints and welds, beam penetrations, seismic isolation and damping devices)
  or that was inferred is reported in `model.notices` with its kind and count. `readStb` throws for ST-Bridge 1.x.

How the format is handled, including the rules inferred from the specification's figures, is described in
[docs/formats/st-bridge.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/st-bridge.md) (Japanese).
This package is not certified by buildingSMART Japan and does not replace structural calculation or code compliance
checks. See THIRD_PARTY_NOTICES.md.
