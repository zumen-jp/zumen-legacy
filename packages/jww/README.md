# @zumenjs/jww

Reads Jw_cad JWW drawings (versions 600 and 700) for [zumen](https://github.com/zumenjs/zumen).
Written in TypeScript without WebAssembly; works in browsers and Node.js.

```ts
import { readDrawing } from "@zumenjs/core";
import { jww, jwwToScene, readJww } from "@zumenjs/jww";

const scene = readDrawing(bytes, [jww]);
const doc = readJww(bytes); // or keep the JWW document model
```

How the format is handled is described in
[docs/formats/jww.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/jww.md) (Japanese).
The file layout follows [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT); see THIRD_PARTY_NOTICES.md.
