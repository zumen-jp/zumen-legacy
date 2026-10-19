# @zumenjs/react

React components for [zumen](https://github.com/zumenjs/zumen): `CadViewer`, a complete viewer with layers,
properties and measurement, and the `useDrawingViewer` and `useCadFile` hooks for building your own UI.

```tsx
import { jww } from "@zumenjs/jww";
import { CadViewer, useCadFile } from "@zumenjs/react";
import { sxf } from "@zumenjs/sxf";

const READERS = [sxf, jww];

export function DrawingView({ file }: { file: File | null }) {
  const { scene } = useCadFile(file, READERS);
  return scene ? <CadViewer scene={scene} style={{ height: 640 }} /> : null;
}
```

For drawings with several views (DXF and DWG model space and layouts), pass `{ view }` to `useCadFile` and
`onViewChange` to `CadViewer`, which then shows a view selector. `useCadFile` reads as soon as the file is loaded, so
wait for `initDwg()` of `@zumenjs/dwg` before passing it a DWG.

Requires React 19.3 or a later 19.x release.
