# @zumenjs/react

React hooks for embedding the drawing engine of [zumen](https://github.com/zumenjs/zumen),
[`@zumenjs/viewer`](../viewer), in an interface you build yourself. There are no components and no CSS: the hooks
create the engine, read its state and events, and read files.

```tsx
"use client";

import type { Scene } from "@zumenjs/core";
import { useDrawing, useDrawingViewer, useViewerEvent, useViewerState } from "@zumenjs/react";
import { reader } from "@zumenjs/reader";
import { useState } from "react";

const READERS = [reader];

export function DrawingView({ file }: { file: File | null }) {
  const { scene, error } = useDrawing(file, READERS);
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <Drawing scene={scene} /> : null;
}

function Drawing({ scene }: { scene: Scene }) {
  const { ref, viewer } = useDrawingViewer(scene, { label: "Drawing" });
  const selection = useViewerState(viewer, (state) => state.selection);
  const [zoom, setZoom] = useState(0);
  useViewerEvent(viewer, "move", ({ view }) => setZoom(view.zoom));
  return (
    <>
      <button type="button" onClick={() => viewer?.fit()}>Fit</button>
      <div ref={ref} style={{ height: 480 }} />
      <p>{Math.round(zoom * 100)}% {selection?.[0]?.type}</p>
    </>
  );
}
```

- `useDrawingViewer(scene, options)` creates a `DrawingViewer` on the element that receives `ref` and destroys it when
  the element goes. A new `scene` replaces the drawing and a new `label` the canvas label; the other options are used
  when the viewer is created, and the viewer's methods (`setHandlers`, `setStyle`, `setTool`…) change them later.
- `useViewerState(viewer, selector, isEqual?)` re-renders only when the selected part of the state changes, and returns
  `undefined` while the viewer does not exist. Pass `isEqual` when the selector builds a new object.
- `useViewerEvent(viewer, type, listener)` subscribes to `move`, `moveend` or `cursor`, which change at the pace of
  the pointer and are not kept in the state.
- `useDrawing(file, readers, { view })` reads a `File` or `Blob`. When the reader that recognizes the file has `load`
  (the reader of `@zumenjs/reader` loads its WebAssembly module), it waits for it, so the module is fetched when the
  first drawing is opened. `view` picks a DXF or DWG layout without reading the file again. To read in a Web Worker,
  use `ReaderWorker` of `@zumenjs/reader` in an effect of your own and pass the scene to `useDrawingViewer`.

The playground's [2D sample](https://github.com/zumenjs/zumen/blob/main/playground/drawing-sample.tsx) is a complete
viewer built with these hooks: toolbar, layer list, properties and status bar, with its own CSS and Japanese text.

Requires React 19.3 or a later 19.x release.
