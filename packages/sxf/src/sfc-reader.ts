/**
 * SFC ファイル（SXF のフィーチャコメントファイル）のバイト列を、フィーチャのレコードに分ける。
 * 構造は ASCII で、文字列だけが CP932（Shift_JIS）なので、バイト単位で読んでから文字列を復号する。
 * 仕様: SXF Ver.3.1 仕様書・同解説 附属書 SFC 編（第 2 版）1-1。
 */

/** 引数は、引用符で囲んだ値（数値・集合の文字列）、文字列、括弧の集合のいずれか */
export type SfcValue = string | SfcValue[];

export interface SfcRecord {
  /** インスタンス番号（#nn の nn） */
  id: number;
  /** フィーチャ名（小文字） */
  name: string;
  /** /*SXF、/*SXF3、/*SXF3.1 のどれで囲まれていたか */
  version: "2" | "3" | "3.1";
  params: SfcValue[];
  /** 文字列の引数（\'…\' で囲まれたもの）の位置 */
  stringParams: ReadonlySet<number>;
}

export interface SfcReadResult {
  header: { description: string; fileName: string; preprocessor: string; originatingSystem: string };
  records: SfcRecord[];
  /** 読めなかったフィーチャの数 */
  skipped: number;
}

const decoder = new TextDecoder("shift_jis");
const BACKSLASH = 0x5c;
const QUOTE = 0x27;
const COMMA = 0x2c;
const OPEN = 0x28;
const CLOSE = 0x29;

function isSpace(byte: number | undefined): boolean {
  return byte === 0x20 || byte === 0x09 || byte === 0x0a || byte === 0x0d;
}

/** Shift_JIS の 2 バイト文字の先頭バイト */
function isLeadByte(byte: number): boolean {
  return (byte >= 0x81 && byte <= 0x9f) || (byte >= 0xe0 && byte <= 0xfc);
}

function startsWith(bytes: Uint8Array, at: number, ascii: string): boolean {
  if (at + ascii.length > bytes.length) return false;
  for (let i = 0; i < ascii.length; i++) if (bytes[at + i] !== ascii.charCodeAt(i)) return false;
  return true;
}

function indexOfAscii(bytes: Uint8Array, ascii: string, from: number): number {
  const first = ascii.charCodeAt(0);
  for (let i = bytes.indexOf(first, from); i >= 0; i = bytes.indexOf(first, i + 1)) {
    if (startsWith(bytes, i, ascii)) return i;
  }
  return -1;
}

class ParseError extends Error {}

/** レコードの引数を読む小さな字句解析器 */
class ParamReader {
  pos: number;
  constructor(
    private readonly bytes: Uint8Array,
    start: number,
    private readonly end: number,
  ) {
    this.pos = start;
  }

  private skipSpace(): void {
    while (this.pos < this.end && isSpace(this.bytes[this.pos])) this.pos++;
  }

  peek(): number | undefined {
    this.skipSpace();
    return this.pos < this.end ? this.bytes[this.pos] : undefined;
  }

  /** "(" の直後から、対応する ")" までの引数の並びを読む */
  readList(stringParams?: Set<number>): SfcValue[] {
    const values: SfcValue[] = [];
    if (this.peek() === CLOSE) {
      this.pos++;
      return values;
    }
    for (;;) {
      if (this.peek() === BACKSLASH && this.bytes[this.pos + 1] === QUOTE) stringParams?.add(values.length);
      values.push(this.readValue());
      const next = this.peek();
      this.pos++;
      if (next === COMMA) continue;
      if (next === CLOSE) return values;
      throw new ParseError("引数の区切りがありません");
    }
  }

  private readValue(): SfcValue {
    const byte = this.peek();
    if (byte === undefined) throw new ParseError("引数が途中で終わっています");
    if (byte === BACKSLASH && this.bytes[this.pos + 1] === QUOTE) return this.readText();
    if (byte === QUOTE) return this.readQuoted();
    if (byte === OPEN) {
      this.pos++;
      return this.readList();
    }
    const start = this.pos;
    while (this.pos < this.end && this.bytes[this.pos] !== COMMA && this.bytes[this.pos] !== CLOSE) this.pos++;
    return decoder.decode(this.bytes.subarray(start, this.pos)).trim();
  }

  /** '…'。数値や '(1,2,3)' の集合。中身は ASCII */
  private readQuoted(): SfcValue {
    this.pos++;
    const start = this.pos;
    const close = this.bytes.indexOf(QUOTE, start);
    if (close < 0 || close >= this.end) throw new ParseError("引用符が閉じていません");
    this.pos = close + 1;
    const raw = decoder.decode(this.bytes.subarray(start, close)).replace(/[\r\n]/g, "");
    const trimmed = raw.trim();
    if (trimmed.startsWith("(") && trimmed.endsWith(")")) {
      const inner = trimmed.slice(1, -1).trim();
      return inner === "" ? [] : inner.split(",").map((item) => item.trim());
    }
    return trimmed;
  }

  /** \'…\'。終わりは \' の後に , か ) が続く位置。\\ は \ 1 文字。2 バイト文字の 2 バイト目は区切らない */
  private readText(): string {
    this.pos += 2;
    const out: number[] = [];
    while (this.pos < this.end) {
      const byte = this.bytes[this.pos] as number;
      if (isLeadByte(byte) && this.pos + 1 < this.end) {
        out.push(byte, this.bytes[this.pos + 1] as number);
        this.pos += 2;
        continue;
      }
      if (byte === BACKSLASH) {
        const next = this.bytes[this.pos + 1];
        if (next === BACKSLASH) {
          out.push(BACKSLASH);
          this.pos += 2;
          continue;
        }
        if (next === QUOTE) {
          let after = this.pos + 2;
          while (after < this.end && isSpace(this.bytes[after])) after++;
          const terminator = this.bytes[after];
          if (terminator === COMMA || terminator === CLOSE || after >= this.end) {
            this.pos += 2;
            return decoder.decode(new Uint8Array(out)).replace(/[\r\n]/g, "");
          }
        }
      }
      if (byte !== 0x0a && byte !== 0x0d) out.push(byte);
      this.pos++;
    }
    throw new ParseError("文字列が閉じていません");
  }
}

const OPENERS = [
  { marker: "/*SXF3.1", closer: "SXF3.1*/", version: "3.1" },
  { marker: "/*SXF3", closer: "SXF3*/", version: "3" },
  { marker: "/*SXF", closer: "SXF*/", version: "2" },
] as const;

function readHeaderStrings(bytes: Uint8Array): SfcReadResult["header"] {
  const end = indexOfAscii(bytes, "DATA;", 0);
  const text = decoder.decode(bytes.subarray(0, end > 0 ? end : Math.min(bytes.length, 4096)));
  const strings = (entity: string): string[] => {
    const match = new RegExp(`${entity}\\s*\\(([\\s\\S]*?)\\)\\s*;`).exec(text);
    if (!match?.[1]) return [];
    return Array.from(match[1].matchAll(/'((?:[^']|'')*)'/g), (item) => (item[1] ?? "").replace(/''/g, "'"));
  };
  const description = strings("FILE_DESCRIPTION");
  const name = strings("FILE_NAME");
  return {
    description: description[0] ?? "",
    fileName: name[0] ?? "",
    preprocessor: name[4] ?? "",
    originatingSystem: name[5] ?? "",
  };
}

export function readSfc(bytes: Uint8Array): SfcReadResult {
  const records: SfcRecord[] = [];
  let skipped = 0;
  let pos = 0;
  for (;;) {
    const start = indexOfAscii(bytes, "/*SXF", pos);
    if (start < 0) break;
    const opener = OPENERS.find((candidate) => startsWith(bytes, start, candidate.marker)) ?? OPENERS[2];
    const close = indexOfAscii(bytes, opener.closer, start + opener.marker.length);
    if (close < 0) {
      skipped++;
      break;
    }
    pos = close + opener.closer.length;
    try {
      const record = readRecord(bytes, start + opener.marker.length, close, opener.version);
      if (record) records.push(record);
      else skipped++;
    } catch (error) {
      if (!(error instanceof ParseError)) throw error;
      skipped++;
    }
  }
  return { header: readHeaderStrings(bytes), records, skipped };
}

function readRecord(bytes: Uint8Array, from: number, to: number, version: SfcRecord["version"]): SfcRecord | null {
  let i = from;
  while (i < to && isSpace(bytes[i])) i++;
  if (bytes[i] !== 0x23) return null;
  i++;
  let id = 0;
  let digits = 0;
  while (i < to && (bytes[i] as number) >= 0x30 && (bytes[i] as number) <= 0x39) {
    id = id * 10 + ((bytes[i] as number) - 0x30);
    i++;
    digits++;
  }
  if (digits === 0) return null;
  while (i < to && isSpace(bytes[i])) i++;
  if (bytes[i] !== 0x3d) return null;
  i++;
  while (i < to && isSpace(bytes[i])) i++;
  const nameStart = i;
  while (i < to && /[A-Za-z0-9_]/.test(String.fromCharCode(bytes[i] as number))) i++;
  const name = decoder.decode(bytes.subarray(nameStart, i)).toLowerCase();
  if (!name) return null;
  while (i < to && isSpace(bytes[i])) i++;
  if (bytes[i] !== OPEN) return null;
  const reader = new ParamReader(bytes, i + 1, to);
  const stringParams = new Set<number>();
  const params = reader.readList(stringParams);
  return { id, name, version, params, stringParams };
}
