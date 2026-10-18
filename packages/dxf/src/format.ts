import type { DrawingReader } from "@zumenjs/core";
import type { DxfDocument } from "./model.js";
import { parseDxf } from "./parse.js";
import { dxfToScene } from "./scene.js";
import { isAsciiDxf, isBinaryDxf } from "./tags.js";

/** 中身がテキストかバイナリの DXF か */
export function isDxf(bytes: Uint8Array): boolean {
  return isBinaryDxf(bytes) || isAsciiDxf(bytes);
}

/** レイアウトを切り替えて読み直すとき、同じバイト列を解析し直さない */
const parsed = new WeakMap<Uint8Array, DxfDocument>();

/** readDrawing に渡す DXF の読み取り。options.view はモデル空間（Model）かレイアウトの名前 */
export const dxf: DrawingReader = {
  formats: ["dxf"],
  detect: (bytes) => (isDxf(bytes) ? "dxf" : null),
  read(bytes, format, options) {
    if (format !== "dxf") throw new Error(`DXF の読み取りは ${format} を読めません`);
    let doc = parsed.get(bytes);
    if (!doc) {
      doc = parseDxf(bytes);
      parsed.set(bytes, doc);
    }
    return dxfToScene(doc, options?.view === undefined ? {} : { layout: options.view });
  },
};
