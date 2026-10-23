import type { DrawingFormat } from "@zumenjs/core";

// 中身から形式を見分ける規則。Rust の zumen::detect（zumen-sxf・zumen-jww・zumen-dxf・zumen-dwg・zumen-pdf の detect）と
// 同じで、WebAssembly を読み込まずに判定できる。どちらのモジュールを読み込むかもこれで決める（決定 0030）。
// tests/reader.test.ts がどの図面でも WebAssembly の detect と比べる

/** 読める形式。圧縮した SXF は sfz（中が SFC）と p2z（中が P21）。pdf は PDF のモジュールで読む */
export const FORMATS: readonly DrawingFormat[] = ["sfc", "p21", "sfz", "p2z", "jww", "jwc", "jws", "dxf", "dwg", "pdf"];

/** 図面の形式を中身から判定する（SXF・JWW・DXF・DWG・PDF の順に見る）。読めない形式なら null */
export function detectDrawing(bytes: Uint8Array): DrawingFormat | null {
  return (
    detectSxf(bytes) ?? detectJw(bytes) ?? (isDxf(bytes) ? "dxf" : isDwg(bytes) ? "dwg" : isPdf(bytes) ? "pdf" : null)
  );
}

/**
 * 中身が PDF か。先頭の 1,024 バイトの中に、ファイルのヘッダ %PDF- があれば PDF とみなす（ISO 32000-1:2008 の 7.5.2。
 * ヘッダの前の余計なバイトは Acrobat と pdf.js と同じく 1,024 バイトまで読む）。Rust の zumen_pdf::is_pdf と同じ
 */
export function isPdf(bytes: Uint8Array): boolean {
  return latin1(bytes, 0, Math.min(bytes.length, 1024)).includes("%PDF-");
}

/**
 * 中身が DWG か。ファイルの先頭の 6 バイトが版の文字列（AC1015 など、AC10 と 2 桁の数字）なら DWG とみなす
 * （Open Design Specification for .dwg files 5.4.1 の 3.2.1）。未知の版も DWG とみなし、読むときに
 * エラーで伝える
 */
export function isDwg(bytes: Uint8Array): boolean {
  return (
    (bytes.length >= 6 && /^AC10[0-9]{2}$/.test(latin1(bytes, 0, 6))) ||
    ["MC0.0", "AC1.2", "AC1.40", "AC1.50", "AC2.10"].some(
      (magic) => startsWith(bytes, magic, 0) && bytes[magic.length] === 0,
    )
  );
}

/** Jw_cad の識別子。Rust の is_jww・is_jwc・is_jws と同じ（docs/formats/jww.md の JWC・JWS） */
function detectJw(bytes: Uint8Array): DrawingFormat | null {
  if (startsWith(bytes, "JwwData.", 0)) return "jww";
  if (startsWith(bytes, "jw_cad(c)data", 0)) return "jwc";
  if (startsWith(bytes, "JwsData.", 0)) return "jws";
  return null;
}

/** バイナリの DXF の印で始まるか、テキストの DXF（先頭のコメント 999 を飛ばし、最初のタグが 0 / SECTION）か */
function isDxf(bytes: Uint8Array): boolean {
  if (startsWith(bytes, "AutoCAD Binary DXF\r\n\x1a\x00", 0)) return true;
  const head = latin1(bytes, 0, Math.min(bytes.length, 4096)).replace(/^ï»¿/, "");
  const lines = head.split(/\r\n|\n|\r/);
  let i = 0;
  while (i + 1 < lines.length && lines[i]?.trim() === "999") i += 2;
  return lines[i]?.trim() === "0" && lines[i + 1]?.trim() === "SECTION";
}

/**
 * SXF の形式（sfc・p21、圧縮したものは sfz・p2z）。SFC と P21 はどちらも ISO-10303-21 で始まり、SFC はフィーチャを
 * /*SXF のコメントに書く（FILE_DESCRIPTION が feature_mode のものも SFC）。圧縮したものは、名前が .p21・.sfc で終わる
 * ファイルを入れた ZIP の書庫
 */
function detectSxf(bytes: Uint8Array): DrawingFormat | null {
  if (u32(bytes, 0) === LOCAL_HEADER) {
    const inner = zippedSxf(bytes);
    return inner === null ? null : inner === "sfc" ? "sfz" : "p2z";
  }
  let start = 0;
  while (start < bytes.length && (bytes[start] === 0x20 || bytes[start] === 0x0d || bytes[start] === 0x0a)) start++;
  if (!startsWith(bytes, "ISO-10303-21", start)) return null;
  const head = latin1(bytes, 0, Math.min(bytes.length, 64 * 1024));
  if (head.includes("/*SXF")) return "sfc";
  if (/FILE_DESCRIPTION\s*\(\s*\(\s*'SCADEC level\d feature_mode'/.test(head)) return "sfc";
  return "p21";
}

const LOCAL_HEADER = 0x04034b50;
const CENTRAL_HEADER = 0x02014b50;
const END_OF_CENTRAL = 0x06054b50;

/** 書庫の中央ディレクトリで、名前が .p21・.sfc で終わる最初のファイルの形式（APPNOTE 4.3.12・4.3.16） */
function zippedSxf(bytes: Uint8Array): "sfc" | "p21" | null {
  if (bytes.length < 22) return null;
  // 中央ディレクトリの終わり（後ろの 22 バイト以降、コメントは 65535 バイトまで）
  let end = bytes.length - 22;
  const earliest = Math.max(0, bytes.length - 22 - 65_535);
  while (end >= earliest && u32(bytes, end) !== END_OF_CENTRAL) end--;
  if (end < earliest) return null;
  const count = u16(bytes, end + 10);
  let at = u32(bytes, end + 16);
  if (count === null || at === null) return null;
  for (let index = 0; index < count; index++) {
    if (u32(bytes, at) !== CENTRAL_HEADER) return null;
    const nameLength = u16(bytes, at + 28);
    const extraLength = u16(bytes, at + 30);
    const commentLength = u16(bytes, at + 32);
    // Rust は汎用フラグから先の欄をすべて読めなければ諦める。いちばん後ろの欄は at + 42 の 4 バイト
    if (nameLength === null || extraLength === null || commentLength === null || u32(bytes, at + 42) === null) {
      return null;
    }
    if (at + 46 + nameLength > bytes.length) return null;
    const name = latin1(bytes, at + 46, at + 46 + nameLength).replace(/[A-Z]/g, (char) => char.toLowerCase());
    at += 46 + nameLength + extraLength + commentLength;
    if (name.endsWith(".p21")) return "p21";
    if (name.endsWith(".sfc")) return "sfc";
  }
  return null;
}

function u16(bytes: Uint8Array, at: number): number | null {
  if (at + 2 > bytes.length) return null;
  return (bytes[at] as number) | ((bytes[at + 1] as number) << 8);
}

function u32(bytes: Uint8Array, at: number): number | null {
  if (at + 4 > bytes.length) return null;
  const low = u16(bytes, at) as number;
  const high = u16(bytes, at + 2) as number;
  return low + high * 0x10000;
}

function startsWith(bytes: Uint8Array, text: string, at: number): boolean {
  if (at + text.length > bytes.length) return false;
  for (let i = 0; i < text.length; i++) if (bytes[at + i] !== text.charCodeAt(i)) return false;
  return true;
}

/** ISO 8859-1 として 1 バイトを 1 文字にする */
function latin1(bytes: Uint8Array, start: number, end: number): string {
  let out = "";
  for (let i = start; i < end; i += 8192) out += String.fromCharCode(...bytes.subarray(i, Math.min(i + 8192, end)));
  return out;
}
