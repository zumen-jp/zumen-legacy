import type { DrawingReader } from "@zumenjs/core";
import { parseP21 } from "./p21.js";
import { latin1 } from "./p21-reader.js";
import { sxfToScene } from "./scene.js";
import { parseSfc } from "./sfc.js";

function startsWithAscii(bytes: Uint8Array, text: string, from = 0): boolean {
  if (bytes.length < from + text.length) return false;
  for (let i = 0; i < text.length; i++) if (bytes[from + i] !== text.charCodeAt(i)) return false;
  return true;
}

/**
 * 中身が SXF なら形式（sfc・p21）を返す。SFC と P21 はどちらも ISO-10303-21 で始まり、
 * SFC はフィーチャを /*SXF のコメントに書く（FILE_DESCRIPTION が feature_mode のものも SFC）。
 */
export function detectSxf(bytes: Uint8Array): "sfc" | "p21" | null {
  let start = 0;
  while (start < bytes.length && (bytes[start] === 0x20 || bytes[start] === 0x0d || bytes[start] === 0x0a)) start++;
  if (!startsWithAscii(bytes, "ISO-10303-21", start)) return null;
  const head = latin1(bytes.subarray(0, Math.min(bytes.length, 64 * 1024)));
  if (head.includes("/*SXF")) return "sfc";
  if (/FILE_DESCRIPTION\s*\(\s*\(\s*'SCADEC level\d feature_mode'/.test(head)) return "sfc";
  return "p21";
}

/** readDrawing に渡す SXF（SFC・P21）の読み取り */
export const sxf: DrawingReader = {
  formats: ["sfc", "p21"],
  detect: detectSxf,
  read(bytes, format) {
    if (format !== "sfc" && format !== "p21") throw new Error(`SXF の読み取りは ${format} を読めません`);
    return sxfToScene(format === "sfc" ? parseSfc(bytes) : parseP21(bytes));
  },
};
