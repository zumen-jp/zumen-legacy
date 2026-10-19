import { buildModel } from "./stb/model.js";
import type { StructureModel } from "./types.js";
import { decodeXml } from "./xml/decode.js";
import { parseXml } from "./xml/parse.js";

/** 中身が ST-Bridge（ルート要素が ST_BRIDGE の XML）か。拡張子は使わない */
export function isStb(input: ArrayBuffer | Uint8Array): boolean {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  const head = new TextDecoder("latin1").decode(bytes.subarray(0, Math.min(bytes.length, 4096)));
  return /<\s*(?:[A-Za-z_][\w.-]*:)?ST_BRIDGE[\s>]/.test(head);
}

/** ST-Bridge のバイト列を読み、表示用のモデルにする。文字コード（UTF-8・Shift_JIS）は XML 宣言から決める */
export function readStb(input: ArrayBuffer | Uint8Array): StructureModel {
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  const decoded = decodeXml(bytes);
  const root = parseXml(decoded.text);
  return buildModel(root, { encoding: decoded.encoding, declared: decoded.declared, fallback: decoded.fallback });
}
