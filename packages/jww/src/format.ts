import type { DrawingReader } from "@zumenjs/core";
import { isJww, readJww } from "./reader.js";
import { jwwToScene } from "./scene.js";

/** readDrawing に渡す JWW の読み取り */
export const jww: DrawingReader = {
  formats: ["jww"],
  detect: (bytes) => (isJww(bytes) ? "jww" : null),
  read(bytes, format) {
    if (format !== "jww") throw new Error(`JWW の読み取りは ${format} を読めません`);
    return jwwToScene(readJww(bytes));
  },
};
