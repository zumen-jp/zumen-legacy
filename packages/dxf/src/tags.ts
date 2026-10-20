import type { NoticeList } from "@zumenjs/core";

/**
 * DXF のグループコードと値の並び（タグ）。テキストとバイナリの DXF を同じ形にする。
 * 値の型はグループコードの範囲で決まる（DXF Reference「Group Code Value Types Reference」）。
 */
export interface DxfTags {
  readonly codes: Int32Array;
  readonly values: readonly (string | number)[];
  readonly length: number;
}

export interface TagReadResult {
  tags: DxfTags;
  /** $ACADVER。見つからなければ AC1009（R12） */
  version: string;
  binary: boolean;
  /** 文字列の復号に使った文字コード（TextDecoder の名前） */
  encoding: string;
  encodingSource: "version" | "codepage" | "detected" | "default";
  codepage: string | undefined;
}

const Kind = { String: 0, Float: 1, Int16: 2, Int32: 3, Int64: 4, Bool: 5, Binary: 6 } as const;
type Kind = (typeof Kind)[keyof typeof Kind];

/** グループコードの値の型（DXF Reference「Group Code Value Types Reference」の表） */
function kindOf(code: number): Kind {
  if (code <= 9) return Kind.String;
  if (code <= 59) return Kind.Float;
  if (code <= 79) return Kind.Int16;
  if (code <= 99) return Kind.Int32;
  if (code <= 109) return Kind.String;
  if (code <= 149) return Kind.Float;
  if (code <= 159) return Kind.String;
  if (code <= 169) return Kind.Int64;
  if (code <= 179) return Kind.Int16;
  if (code <= 209) return Kind.String;
  if (code <= 239) return Kind.Float;
  if (code <= 269) return Kind.String;
  if (code <= 289) return Kind.Int16;
  if (code <= 299) return Kind.Bool;
  if (code <= 309) return Kind.String;
  if (code <= 319) return Kind.Binary;
  if (code <= 369) return Kind.String;
  if (code <= 389) return Kind.Int16;
  if (code <= 399) return Kind.String;
  if (code <= 409) return Kind.Int16;
  if (code <= 419) return Kind.String;
  if (code <= 429) return Kind.Int32;
  if (code <= 439) return Kind.String;
  if (code <= 459) return Kind.Int32;
  if (code <= 469) return Kind.Float;
  if (code <= 1003) return Kind.String;
  if (code === 1004) return Kind.Binary;
  if (code <= 1009) return Kind.String;
  if (code <= 1059) return Kind.Float;
  if (code <= 1070) return Kind.Int16;
  if (code === 1071) return Kind.Int32;
  return Kind.String;
}

/** バイナリ DXF の先頭 22 バイト（DXF Reference「About Binary DXF Files」） */
const BINARY_SENTINEL = "AutoCAD Binary DXF\r\n\x1a\x00";

export function isBinaryDxf(bytes: Uint8Array): boolean {
  if (bytes.length < BINARY_SENTINEL.length) return false;
  for (let i = 0; i < BINARY_SENTINEL.length; i++) if (bytes[i] !== BINARY_SENTINEL.charCodeAt(i)) return false;
  return true;
}

/** テキストの DXF か。先頭のコメント（999）を飛ばし、最初のタグが 0 / SECTION かを見る */
export function isAsciiDxf(bytes: Uint8Array): boolean {
  const head = latin1(bytes, 0, Math.min(bytes.length, 4096)).replace(/^\u00ef\u00bb\u00bf/, "");
  const lines = head.split(/\r\n|\n|\r/);
  let i = 0;
  while (i + 1 < lines.length && lines[i]?.trim() === "999") i += 2;
  return lines[i]?.trim() === "0" && lines[i + 1]?.trim() === "SECTION";
}

function latin1(bytes: Uint8Array, start: number, end: number): string {
  let out = "";
  for (let i = start; i < end; i += 8192) {
    out += String.fromCharCode(...bytes.subarray(i, Math.min(i + 8192, end)));
  }
  return out;
}

/**
 * $DWGCODEPAGE の値から TextDecoder の名前。ANSI_932 は Windows の日本語（Shift_JIS の拡張、CP932）。
 * 名前の一覧は ezdxf（MIT）の tools/codepage.py と同じ対応。
 */
const CODEPAGES: Readonly<Record<string, string>> = {
  ANSI_874: "windows-874",
  ANSI_932: "shift_jis",
  ANSI_936: "gbk",
  ANSI_949: "euc-kr",
  ANSI_950: "big5",
  ANSI_1250: "windows-1250",
  ANSI_1251: "windows-1251",
  ANSI_1252: "windows-1252",
  ANSI_1253: "windows-1253",
  ANSI_1254: "windows-1254",
  ANSI_1255: "windows-1255",
  ANSI_1256: "windows-1256",
  ANSI_1257: "windows-1257",
  ANSI_1258: "windows-1258",
  DOS932: "shift_jis",
  BIG5: "big5",
  GB2312: "gbk",
  KSC5601: "euc-kr",
  MAC_ROMAN: "macintosh",
  ISO8859_1: "windows-1252",
  ISO8859_2: "iso-8859-2",
  ISO8859_3: "iso-8859-3",
  ISO8859_4: "iso-8859-4",
  ISO8859_5: "iso-8859-5",
  ISO8859_6: "iso-8859-6",
  ISO8859_7: "iso-8859-7",
  ISO8859_8: "iso-8859-8",
  ISO8859_9: "windows-1254",
};

/**
 * 日本語の文字列らしいか。全角のかなが 1 字か、漢字が 2 字以上あるとき。
 * 半角カナは根拠にしない（西欧の文字コードの ° や Ø は、Shift_JIS では 1 バイトの半角カナになる）
 */
function looksJapanese(text: string): boolean {
  if (/[\u3041-\u30fa]/.test(text)) return true;
  return (text.match(/[\u3400-\u4dbf\u4e00-\u9fff]/g)?.length ?? 0) >= 2;
}

function decodes(bytes: Uint8Array, label: string): string | null {
  try {
    return new TextDecoder(label, { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}

function hasMultibyteUtf8(bytes: Uint8Array): boolean {
  for (let i = 0; i < bytes.length; i++) if ((bytes[i] as number) >= 0xc2) return true;
  return false;
}

interface EncodingChoice {
  encoding: string;
  source: TagReadResult["encodingSource"];
}

/**
 * 文字列の文字コード。AutoCAD 2007（AC1021）以降の DXF は UTF-8、それより前は $DWGCODEPAGE
 * （DXF Reference「Storage of String Values」）。指定がない、または西欧（ANSI_1252）の指定で、中身が UTF-8 として
 * 正しく読めるなら UTF-8 とする。指定がなく Shift_JIS として正しく読めて日本語を含むなら Shift_JIS とする
 * （日本の古い DXF は指定がないことがある）。
 */
function chooseEncoding(
  bytes: Uint8Array | null,
  version: string,
  codepage: string | undefined,
  notices: NoticeList,
): EncodingChoice {
  if (version >= "AC1021") return { encoding: "utf-8", source: "version" };
  const declared = codepage ? CODEPAGES[codepage.trim().toUpperCase()] : undefined;
  if (codepage && !declared) {
    notices.add(
      "warning",
      "unknown-codepage",
      `対応していない文字コード ${codepage} の文字列を西欧の文字として読みました`,
    );
  }
  if (declared && declared !== "windows-1252") return { encoding: declared, source: "codepage" };
  // バイナリの DXF は数値のバイトを含むので、中身から文字コードを推測しない（bytes が null）
  if (!bytes) return { encoding: "windows-1252", source: codepage ? "codepage" : "default" };
  if (hasMultibyteUtf8(bytes) && decodes(bytes, "utf-8") !== null) {
    if (declared) notices.add("info", "utf8-codepage", "西欧の文字コードの指定と違い、UTF-8 として読みました");
    return { encoding: "utf-8", source: "detected" };
  }
  if (!codepage) {
    const sjis = decodes(bytes, "shift_jis");
    if (sjis !== null && looksJapanese(sjis)) {
      notices.add("info", "guessed-shift-jis", "文字コードの指定がないため、日本語（Shift_JIS）と判断して読みました");
      return { encoding: "shift_jis", source: "detected" };
    }
    return { encoding: "windows-1252", source: "default" };
  }
  return { encoding: "windows-1252", source: "codepage" };
}

function headerString(head: string, name: string): string | undefined {
  // $NAME の次のタグ（グループコード 1 または 3）の値
  const match = new RegExp(`\\${name}\\s*(?:\\r\\n|\\n|\\r)\\s*[0-9]+\\s*(?:\\r\\n|\\n|\\r)([^\\r\\n]*)`).exec(head);
  return match?.[1]?.trim();
}

function parseNumber(text: string): number {
  const value = Number(text);
  if (Number.isFinite(value)) return value;
  // 1.0E+20 のような値は Number で読める。読めない値（空など）は 0
  const trimmed = text.trim();
  const fallback = Number.parseFloat(trimmed);
  return Number.isFinite(fallback) ? fallback : 0;
}

function asciiTags(text: string, notices: NoticeList): DxfTags {
  const codes: number[] = [];
  const values: (string | number)[] = [];
  let pos = text.charCodeAt(0) === 0xfeff ? 1 : 0;
  const length = text.length;
  const separator = text.indexOf("\n") >= 0 ? "\n" : "\r";
  const nextLine = (): string | null => {
    if (pos >= length) return null;
    let end = text.indexOf(separator, pos);
    if (end < 0) end = length;
    let line = text.slice(pos, end);
    pos = end + 1;
    if (separator === "\n" && line.endsWith("\r")) line = line.slice(0, -1);
    return line;
  };
  let bad = 0;
  for (;;) {
    const codeLine = nextLine();
    if (codeLine === null) break;
    const valueStart = pos;
    const valueLine = nextLine();
    if (valueLine === null) {
      if (codeLine.trim() !== "") bad++;
      break;
    }
    const code = Number.parseInt(codeLine, 10);
    if (!Number.isFinite(code)) {
      bad++;
      // 1 行ずれた図面は、次の行から読み直す（行末の \r の分も戻す）
      pos = valueStart;
      continue;
    }
    if (code === 999) continue;
    codes.push(code);
    switch (kindOf(code)) {
      case Kind.Float:
        values.push(parseNumber(valueLine));
        break;
      case Kind.Int16:
      case Kind.Int32:
      case Kind.Int64:
      case Kind.Bool:
        values.push(Math.trunc(parseNumber(valueLine)));
        break;
      default:
        values.push(valueLine);
    }
  }
  if (bad > 0) notices.add("warning", "bad-tag", "グループコードとして読めない行を飛ばしました", bad);
  return { codes: Int32Array.from(codes), values, length: codes.length };
}

const HEX = Array.from({ length: 256 }, (_, value) => value.toString(16).padStart(2, "0").toUpperCase());

class BinaryTags {
  private readonly view: DataView;
  private pos = BINARY_SENTINEL.length;

  constructor(
    private readonly bytes: Uint8Array,
    private readonly decoder: TextDecoder,
    private readonly r12: boolean,
  ) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  read(notices: NoticeList): DxfTags {
    const codes: number[] = [];
    const values: (string | number)[] = [];
    const bytes = this.bytes;
    try {
      while (this.pos < bytes.length) {
        let code: number;
        if (this.r12) {
          // R12 のバイナリ DXF は 1 バイトのグループコード。255 の後の 2 バイトが拡張データのコード
          code = this.u8();
          if (code === 255) code = this.view.getUint16(this.advance(2), true);
        } else {
          code = this.view.getUint16(this.advance(2), true);
        }
        let value: string | number;
        switch (kindOf(code)) {
          case Kind.Float:
            value = this.view.getFloat64(this.advance(8), true);
            break;
          case Kind.Int16:
            value = this.view.getInt16(this.advance(2), true);
            break;
          case Kind.Int32:
            value = this.view.getInt32(this.advance(4), true);
            break;
          case Kind.Int64:
            value = Number(this.view.getBigInt64(this.advance(8), true));
            break;
          case Kind.Bool:
            value = this.u8();
            break;
          case Kind.Binary: {
            // テキストの DXF と同じく 16 進の文字列にする（サムネイル・代理図形などで、表示には使わない）
            const size = this.u8();
            const start = this.advance(size);
            let hex = "";
            for (let i = start; i < start + size; i++) hex += HEX[bytes[i] as number];
            value = hex;
            break;
          }
          default:
            value = this.string();
        }
        codes.push(code);
        values.push(value);
      }
    } catch {
      notices.add("warning", "truncated", "バイナリ DXF が途中で終わっているため、読めたところまでを使いました");
    }
    return { codes: Int32Array.from(codes), values, length: codes.length };
  }

  private advance(size: number): number {
    const start = this.pos;
    if (start + size > this.bytes.length) throw new RangeError("end of data");
    this.pos += size;
    return start;
  }

  private u8(): number {
    return this.bytes[this.advance(1)] as number;
  }

  private string(): string {
    const start = this.pos;
    let end = start;
    const bytes = this.bytes;
    let ascii = true;
    while (end < bytes.length && bytes[end] !== 0) {
      if ((bytes[end] as number) >= 0x80) ascii = false;
      end++;
    }
    if (end >= bytes.length) throw new RangeError("unterminated string");
    this.pos = end + 1;
    return caretNotation(ascii ? latin1(bytes, start, end) : this.decoder.decode(bytes.subarray(start, end)));
  }
}

/**
 * テキストの DXF の文字列の書き方（DXF Reference「About ASCII Control Characters in DXF Files」）。制御文字は ^ と
 * 0x40 を足した文字（改行は ^J）、^ は「^ 」と書く。バイナリの DXF は文字のまま書くので、テキストの DXF と同じ値にする
 */
function caretNotation(text: string): string {
  let i = 0;
  while (i < text.length && text.charCodeAt(i) >= 0x20 && text.charCodeAt(i) !== 0x5e) i++;
  if (i === text.length) return text;
  let out = text.slice(0, i);
  for (; i < text.length; i++) {
    const code = text.charCodeAt(i);
    if (code < 0x20) out += `^${String.fromCharCode(code + 0x40)}`;
    else if (code === 0x5e) out += "^ ";
    else out += text[i];
  }
  return out;
}

function binaryHeaderValue(bytes: Uint8Array, name: string): string | undefined {
  // ezdxf（MIT）の binary_tags_loader と同じく、先頭 1024 バイトから変数の名前を探し、続く文字列を読む
  const head = latin1(bytes, 0, Math.min(bytes.length, 2048));
  const at = head.indexOf(name, BINARY_SENTINEL.length);
  if (at < 0) return undefined;
  // 名前の終わりの NUL の後に値のグループコード（R13 以降は 2 バイト、R12 は 1 バイト）がある
  let start = at + name.length + 1;
  start += head.charCodeAt(start + 1) === 0 ? 2 : 1;
  const end = head.indexOf("\0", start);
  return end > start ? head.slice(start, end) : undefined;
}

/** テキスト・バイナリの DXF をタグにする。文字列は図面の文字コードで復号する */
export function readTags(bytes: Uint8Array, notices: NoticeList): TagReadResult {
  if (isBinaryDxf(bytes)) {
    const version = binaryHeaderValue(bytes, "$ACADVER") ?? "AC1009";
    const codepage = version < "AC1021" ? binaryHeaderValue(bytes, "$DWGCODEPAGE") : undefined;
    const choice = chooseEncoding(null, version, codepage, notices);
    const tags = new BinaryTags(bytes, new TextDecoder(choice.encoding), version <= "AC1009").read(notices);
    return {
      tags,
      version,
      binary: true,
      encoding: choice.encoding,
      encodingSource: choice.source,
      codepage,
    };
  }
  const head = latin1(bytes, 0, Math.min(bytes.length, 256 * 1024));
  const version = headerString(head, "$ACADVER") ?? "AC1009";
  const codepage = headerString(head, "$DWGCODEPAGE");
  const choice =
    bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf
      ? { encoding: "utf-8", source: "detected" as const }
      : chooseEncoding(bytes, version, codepage, notices);
  // CP932・UTF-8 などの 2 バイト目以降は改行（0x0A・0x0D）にならないので、復号してから行に分けてよい
  const text = new TextDecoder(choice.encoding).decode(bytes);
  return {
    tags: asciiTags(text, notices),
    version,
    binary: false,
    encoding: choice.encoding,
    encodingSource: choice.source,
    codepage,
  };
}

const UNICODE_ESCAPE = /\\U\+([0-9A-Fa-f]{4})/g;
const MIF_ESCAPE = /\\M\+([1-5])([0-9A-Fa-f]{4})/g;
/** MIF（\M+nXXXX）の n と文字コード（ezdxf の lldxf/encoding.py と同じ対応） */
const MIF_CODEPAGES: Readonly<Record<string, string>> = {
  "1": "shift_jis",
  "2": "big5",
  "3": "euc-kr",
  "5": "gbk",
};

/**
 * 文字列の \U+XXXX（CIF）と \M+nXXXX（MIF）を文字にする（DXF Reference「Storage of String Values」）。
 * AutoCAD 2004 以前の DXF は、図面の文字コードにない文字をこの形で書く。
 */
export function decodeEscapes(text: string): string {
  if (!text.includes("\\U+") && !text.includes("\\M+") && !text.includes("\\u+") && !text.includes("\\m+")) {
    return text;
  }
  return text
    .replace(UNICODE_ESCAPE, (_, hex: string) => String.fromCodePoint(Number.parseInt(hex, 16)))
    .replace(/\\u\+([0-9A-Fa-f]{4})/g, (_, hex: string) => String.fromCodePoint(Number.parseInt(hex, 16)))
    .replace(MIF_ESCAPE, (whole, page: string, hex: string) => decodeMif(page, hex) ?? whole);
}

export function decodeMif(page: string, hex: string): string | null {
  const label = MIF_CODEPAGES[page];
  if (!label) return null;
  const code = Number.parseInt(hex, 16);
  const bytes = code > 0xff ? Uint8Array.of(code >> 8, code & 0xff) : Uint8Array.of(code);
  return decodes(bytes, label);
}
