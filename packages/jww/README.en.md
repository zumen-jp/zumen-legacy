# @zumenjs/jww

> English reference preserved when Japanese became the primary documentation. See [README.md](README.md) for the Japanese guide and current policy.

Reads Jw_cad JWW drawings for [zumen](https://github.com/zumenjs/zumen): every data version, with the fields of each
version as Jw_cad's format description notes them (checked on drawings that Jw_cad saved in versions 6.00 and 7.00).
Written in TypeScript without WebAssembly; works in browsers and Node.js.

```ts
import { readDrawing } from "@zumenjs/core";
import { jww, jwwToScene, readJww } from "@zumenjs/jww";

const scene = readDrawing(bytes, [jww]);
const doc = readJww(bytes); // or keep the JWW document model
```

How the format is handled is described in
[docs/formats/jww.md](https://github.com/zumenjs/zumen/blob/main/docs/formats/jww.md).
The file layout follows [ezjww](https://github.com/monozukuri-ai/ezjww) (MIT); see THIRD_PARTY_NOTICES.md.
