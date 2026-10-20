# @zumenjs/viewer

The 2D drawing engine of [zumen](https://github.com/zumenjs/zumen), to embed in an interface you build yourself. It
draws a `Scene` on Canvas and handles the view, selection, measurement and tools. It has no text for people and ships
no CSS: buttons, panels and labels come from your application, and colors from CSS custom properties.

```ts
import { readDrawing } from "@zumenjs/core";
import { jww } from "@zumenjs/jww";
import { DrawingViewer } from "@zumenjs/viewer";

const scene = readDrawing(bytes, [jww]);
const viewer = new DrawingViewer(element, scene, { label: "Site plan", handlers: { wheel: "modifier" } });
viewer.on("move", ({ view }) => console.log(view.center, view.zoom));
viewer.subscribe(() => console.log(viewer.getState().selection));
```

The element needs a size; the viewer fills it with two canvases (the drawing, and the tools and overlays). In React,
use the hooks of [`@zumenjs/react`](../react). The playground's
[2D sample](https://github.com/zumenjs/zumen/blob/main/playground/drawing-sample.tsx) builds a toolbar, a layer list,
properties and a status bar with this API only.

## View

- `jumpTo({ center, zoom })` sets the paper point (mm) at the center and the zoom (1 shows 1 mm of paper as 1 mm on
  screen at 96 dpi); `getView()` returns them with the visible paper box and the size in CSS pixels. The values go back
  and forth unchanged, so a saved view is restored exactly and two views can follow each other.
- `fitBounds(box, { padding })` fits a paper box (an entity's `bbox`, say) and keeps fitting it when the element is
  resized, until the view is changed. `fit()` fits the whole drawing. `zoomBy(factor, px, py)` zooms around a point in
  CSS pixels and `panBy(dx, dy)` moves the drawing.
- `move` is sent whenever the view changes (many times during a drag, a pinch or wheel zooming) and `moveend` when the
  change is over, both with the view and the DOM event of the user's action (`null` for method calls). Use them to keep
  HTML placed on the drawing in place: `viewer.toScreen(x, y)` gives its position, and `viewer.toPaper(px, py)` goes the
  other way. `toReal(entity.placement, x, y)` of `@zumenjs/core` converts paper to real-world millimeters.
- `cursor` is sent with the paper point under the pointer, and `null` when it leaves.

## State

`getState()` returns an object that is replaced on every change, and `subscribe(listener)` reports the changes. It holds
the scene, the background (`"dark"` or `"light"`, by default the one the drawing assumes), line weights, hidden
layers, the selection (several entities, in the order they were selected), the hovered entity, named highlights, the
active tool and the measurement with its distance in paper and real-world millimeters. What changes at the pace of the
pointer is sent as events instead.

`setScene`, `setBackground`, `setLineWeights`, `setLayersVisible`, `setHiddenLayers`, `select(entities)`,
`setHovered(entity)`, `setHighlight(name, entities)` (search results, for example; an empty list removes it),
`setTool(name)` and `setMeasurement` change it.

## Input handlers

Each can be turned off in the options or with `setHandlers`:

| Handler | Default | What it does |
| --- | --- | --- |
| `dragPan` | `true` | Dragging pans: the middle and right buttons always, the left button and one finger when the tool allows it |
| `wheel` | `"always"` | Wheel zoom: `"always"`, `"modifier"` (only with Ctrl or ⌘, which includes trackpad pinching, so the page scrolls otherwise) or `"never"` |
| `pinch` | `true` | Two fingers zoom and pan |
| `doubleClick` | `true` | Double click zooms in by a factor of two |
| `keyboard` | `true` | `+` and `-` zoom, `0` fits, the arrow keys pan and Escape clears the selection and the measurement |

## Tools

The active tool receives pointer events with paper coordinates (`pointerDown`, `pointerMove`, `pointerUp`, `click`,
`pointerLeave`) and key presses, asks the viewer for the entities and the snap point under the pointer, changes the
state, and draws on the overlay canvas. The built-in tools use the same interface: `select` (click to select, Shift,
Ctrl or ⌘ to add or remove, drag to pan), `measure` (two points snapped to vertices and lines) and `pan`.

```ts
const pins: [number, number][] = [];
viewer.addTool({
  name: "pin",
  cursor: "copy",
  click(event, viewer) {
    pins.push([event.x, event.y]);
    viewer.invalidate();
  },
  draw(ctx, viewer) {
    for (const [x, y] of pins) {
      const [px, py] = viewer.toScreen(x, y);
      ctx.fillRect(px - 4, py - 4, 8, 8);
    }
  },
});
viewer.setTool("pin");
```

A tool with `dragPan: false` receives left-button drags instead of panning (markup, areas). `cancel` is called on
Escape, when a second finger starts a pinch and when the scene changes.

## Style

Colors are read from CSS custom properties on the element, for the dark and the light background, and `setStyle`
overrides them in JavaScript. Without them, the viewer draws as zumen always has.

| Property | Default (dark, light) | |
| --- | --- | --- |
| `--zumen-background-dark`, `-light` | `#000000`, `#ffffff` | Background. On a light color near-white lines are drawn black, on a dark one near-black lines white |
| `--zumen-selection-dark`, `-light` | `#ffb000`, `#d16d00` | Selected entities |
| `--zumen-hover-dark`, `-light` | `none` | The hovered entity |
| `--zumen-highlight-dark`, `-light` | `#ff4fd8`, `#b0008a` | Named highlights |
| `--zumen-highlight-<name>-dark`, `-light` | | The highlight of that name |
| `--zumen-measure-dark`, `-light` | `#5ad2ff`, `#0031d8` | The measurement and the snap marker |
| `--zumen-sheet-frame-dark`, `-light` | 35% white, 35% black | The sheet frame, or `none` |
| `--zumen-fit-padding` | `24` | The padding of `fit` and `fitBounds`, in CSS pixels |

The properties are read when the viewer is created, when the element is resized and when the system color scheme
changes. Call `refreshStyle()` after switching the theme of your application in another way.

## Queries and overlays

- `entitiesAt(x, y, { tolerance })` returns the visible entities near a paper point, nearest first,
  `entitiesIn(box)` those whose box overlaps a paper box, in drawing order, and `snapAt(x, y)` the snap point. Tools use
  them, and so can applications that handle input themselves.
- `addOverlay((ctx, view) => …)` draws on top of the drawing whatever the tool; call `invalidate()` when it changes.

The renderer itself is exported for thumbnails and images: `renderScene(ctx, scene, view, width, height, options)`
draws with the same rules as the Rust renderer (`zumen-render`), and `renderHighlights` draws entities in a highlight
color on top. `HitIndex` is the hit testing and snapping behind the queries.
