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

/** HEADER の宣言を読む。文字列とコメントの中の文字はトークンではない（ISO 10303-21, 6.2・8.2.3）。 */
function firstSchema(head: string): string | undefined {
  const skip = (at: number): number => {
    while (at < head.length) {
      if (/\s/.test(head[at] ?? "")) at++;
      else if (head.startsWith("/*", at)) {
        const end = head.indexOf("*/", at + 2);
        at = end < 0 ? head.length : end + 2;
      } else break;
    }
    return at;
  };
  let at = 0;
  while (at < head.length) {
    at = skip(at);
    const quote = head[at];
    if (quote === "'" || quote === '"') {
      at++;
      while (at < head.length) {
        if (head[at++] !== quote) continue;
        if (head[at] !== quote) break;
        at++;
      }
      continue;
    }
    const start = at;
    while (/[a-z0-9_]/i.test(head[at] ?? "")) at++;
    if (at === start) {
      at++;
      continue;
    }
    if (head.slice(start, at).toUpperCase() !== "FILE_SCHEMA") continue;
    let value = skip(at);
    if (head[value] !== "(") continue;
    value = skip(value + 1);
    if (head[value] !== "(") continue;
    value = skip(value + 1);
    if (head[value] !== "'") continue;
    const end = head.indexOf("'", value + 1);
    if (end >= 0) return head.slice(value + 1, end);
  }
  return undefined;
}

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
  const schema = firstSchema(head);
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
