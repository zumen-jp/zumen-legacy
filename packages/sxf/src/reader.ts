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
 * FILE_SCHEMA の 1 つ目のスキーマ名（ISO 10303-21 の 8.2.3 file_schema）。キーワードの大文字と小文字は区別しない。
 * 1 バイトを 1 文字にした文字列なので、i は ASCII の大文字と小文字だけを同じとみなす
 */
const FILE_SCHEMA = /FILE_SCHEMA\s*\(\s*\(\s*'([^']*)'/i;

/**
 * SXF のスキーマ名。SXF の P21 は STEP AP202 のサブセット（SXF Ver.3.1 仕様書・同解説 STEP/AP202 サブセット編）なので、
 * AP202 の ASSOCIATIVE_DRAUGHTING（SFC も同じヘッダを書く）。前の空白と、後ろのスキーマのオブジェクト識別子（{ … }）を
 * 除き、大文字と小文字を区別せずに比べる（EXPRESS の名前は大文字と小文字を区別しない）
 */
const SXF_SCHEMA = /^\s*ASSOCIATIVE_DRAUGHTING(?![^\s{])/i;

/**
 * 中身が SXF なら形式（sfc・p21）を返す。SFC と P21 はどちらも ISO-10303-21 で始まり、
 * SFC はフィーチャを /*SXF のコメントに書く（FILE_DESCRIPTION が feature_mode のものも SFC）。
 * FILE_SCHEMA の 1 つ目のスキーマが ASSOCIATIVE_DRAUGHTING でないもの（IFC、STEP の AP203・AP214・AP242 など）は
 * SXF ではない。FILE_SCHEMA が先頭の 64 KiB に見つからないものは、これまでどおり SXF とみなす（読み取りは
 * FILE_SCHEMA を使わないので、それを書かない図面も読めなくしない）。
 */
export function detectSxf(bytes: Uint8Array): "sfc" | "p21" | null {
  let start = 0;
  while (start < bytes.length && (bytes[start] === 0x20 || bytes[start] === 0x0d || bytes[start] === 0x0a)) start++;
  if (!startsWithAscii(bytes, "ISO-10303-21", start)) return null;
  const head = latin1(bytes.subarray(0, Math.min(bytes.length, 64 * 1024)));
  const schema = FILE_SCHEMA.exec(head)?.[1];
  if (schema !== undefined && !SXF_SCHEMA.test(schema)) return null;
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
