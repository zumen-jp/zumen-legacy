/**
 * ISO 10303-21（STEP の物理ファイル）の DATA 節を読む汎用の字句・構文解析。
 * 複合インスタンス（#n = (A() B() …);）も型ごとの引数に分けて持つ。
 */

export type P21Value =
  | null
  | number
  | string
  | { ref: number }
  | { enum: string }
  | { typed: string; value: P21Value }
  | P21Value[];

export interface P21Entity {
  id: number;
  /** 型名（大文字）ごとの引数。単純インスタンスは 1 つ */
  parts: Map<string, P21Value[]>;
}

export interface P21File {
  entities: Map<number, P21Entity>;
  header: { description: string; fileName: string; preprocessor: string; originatingSystem: string };
  /** 読めなかったインスタンスの数 */
  skipped: number;
}

const sjis = new TextDecoder("shift_jis");

/** Shift_JIS の 2 バイト文字の先頭バイト */
function isLeadByte(code: number): boolean {
  return (code >= 0x81 && code <= 0x9f) || (code >= 0xe0 && code <= 0xfc);
}

/**
 * バイトをそのまま U+0000〜U+00FF の文字にする（ISO 8859-1）。TextDecoder の "latin1" は windows-1252 で、
 * 0x80〜0x9F を別の文字（0x82 なら U+201A）にするため、Shift_JIS のバイトに戻せなくなる。
 */
export function latin1(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 0x8000) out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return out;
}

/** STEP の文字列のエスケープ（'' と \X2\…\X0\ など）を戻す。規格外の 8 ビットは Shift_JIS として読む */
export function decodeStepString(raw: string): string {
  if (!/['\\\x80-\xff]/.test(raw)) return raw;
  let out = "";
  for (let i = 0; i < raw.length; ) {
    const char = raw[i] as string;
    if (char === "'" && raw[i + 1] === "'") {
      out += "'";
      i += 2;
      continue;
    }
    if (char === "\\") {
      const rest = raw.slice(i);
      const x2 = /^\\X2\\((?:[0-9A-Fa-f]{4})*)\\X0\\/.exec(rest);
      if (x2) {
        const hex = x2[1] as string;
        for (let k = 0; k < hex.length; k += 4) out += String.fromCharCode(Number.parseInt(hex.slice(k, k + 4), 16));
        i += x2[0].length;
        continue;
      }
      const x4 = /^\\X4\\((?:[0-9A-Fa-f]{8})*)\\X0\\/.exec(rest);
      if (x4) {
        const hex = x4[1] as string;
        for (let k = 0; k < hex.length; k += 8) out += String.fromCodePoint(Number.parseInt(hex.slice(k, k + 8), 16));
        i += x4[0].length;
        continue;
      }
      const x = /^\\X\\([0-9A-Fa-f]{2})/.exec(rest);
      if (x) {
        out += String.fromCharCode(Number.parseInt(x[1] as string, 16));
        i += x[0].length;
        continue;
      }
      const s = /^\\S\\(.)/s.exec(rest);
      if (s) {
        out += String.fromCharCode(((s[1] as string).charCodeAt(0) + 128) & 0xff);
        i += s[0].length;
        continue;
      }
      const page = /^\\P[A-I]\\/.exec(rest);
      if (page) {
        i += page[0].length;
        continue;
      }
      if (raw[i + 1] === "\\") {
        out += "\\";
        i += 2;
        continue;
      }
    }
    // 規格では 8 ビットの文字は使えないが、Shift_JIS のまま書く出力ソフトもある。
    // 2 バイト文字の 2 バイト目は 0x40〜0x7E（\ を含む）のこともあるので、先頭バイトの次の 1 バイトも含めて復号する
    if (char.charCodeAt(0) >= 0x80) {
      let j = i;
      while (j < raw.length && raw.charCodeAt(j) >= 0x80)
        j += isLeadByte(raw.charCodeAt(j)) && j + 1 < raw.length ? 2 : 1;
      out += sjis.decode(Uint8Array.from(raw.slice(i, j), (c) => c.charCodeAt(0) & 0xff));
      i = j;
      continue;
    }
    out += char;
    i++;
  }
  return out;
}

class StepSyntaxError extends Error {}

/** 引数の集合・型付きの値の入れ子の上限。深すぎるインスタンスは読めないものとして省く */
const MAX_NESTING = 64;

const SPACE = 0x20;
const TAB = 0x09;
const LF = 0x0a;
const CR = 0x0d;

function isDigit(code: number): boolean {
  return code >= 0x30 && code <= 0x39;
}

function isWord(code: number): boolean {
  return (
    (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a) || isDigit(code) || code === 0x5f || code === 0x21
  );
}

function isNumberChar(code: number): boolean {
  return isDigit(code) || code === 0x2d || code === 0x2b || code === 0x2e || code === 0x45 || code === 0x65;
}

/** 1 文字ずつ読む字句解析。text は latin1 で 1 バイトを 1 文字にした文字列 */
class Lexer {
  pos: number;
  constructor(
    private readonly text: string,
    start: number,
    private readonly end: number,
  ) {
    this.pos = start;
  }

  skip(): void {
    const text = this.text;
    for (;;) {
      let code = text.charCodeAt(this.pos);
      while (this.pos < this.end && (code === SPACE || code === LF || code === CR || code === TAB)) {
        this.pos++;
        code = text.charCodeAt(this.pos);
      }
      if (code === 0x2f && text.charCodeAt(this.pos + 1) === 0x2a) {
        const close = text.indexOf("*/", this.pos + 2);
        this.pos = close < 0 ? this.end : close + 2;
        continue;
      }
      return;
    }
  }

  peek(): string {
    this.skip();
    return this.pos < this.end ? (this.text[this.pos] as string) : "";
  }

  expect(char: string): void {
    if (this.peek() !== char) throw new StepSyntaxError(`${char} がありません`);
    this.pos++;
  }

  keyword(): string {
    this.skip();
    const start = this.pos;
    while (this.pos < this.end && isWord(this.text.charCodeAt(this.pos))) this.pos++;
    if (this.pos === start) throw new StepSyntaxError("型名がありません");
    return this.text.slice(start, this.pos).toUpperCase();
  }

  /** "(" の直後から ")" までの引数 */
  list(depth = 0): P21Value[] {
    if (depth > MAX_NESTING) throw new StepSyntaxError("引数の入れ子が深すぎます");
    const values: P21Value[] = [];
    if (this.peek() === ")") {
      this.pos++;
      return values;
    }
    for (;;) {
      values.push(this.value(depth));
      const next = this.peek();
      this.pos++;
      if (next === ",") continue;
      if (next === ")") return values;
      throw new StepSyntaxError("引数の区切りがありません");
    }
  }

  value(depth: number): P21Value {
    const char = this.peek();
    const text = this.text;
    switch (char) {
      case "$":
      case "*":
        this.pos++;
        return null;
      case "'":
        return this.string();
      case "#": {
        this.pos++;
        let id = 0;
        while (this.pos < this.end && isDigit(text.charCodeAt(this.pos))) {
          id = id * 10 + (text.charCodeAt(this.pos) - 0x30);
          this.pos++;
        }
        return { ref: id };
      }
      case ".": {
        const close = text.indexOf(".", this.pos + 1);
        const value = text.slice(this.pos + 1, close);
        this.pos = close + 1;
        return { enum: value.toUpperCase() };
      }
      case "(":
        this.pos++;
        return this.list(depth + 1);
      case '"': {
        const close = text.indexOf('"', this.pos + 1);
        const value = text.slice(this.pos + 1, close);
        this.pos = close + 1;
        return value;
      }
    }
    const code = text.charCodeAt(this.pos);
    if ((code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a)) {
      const typed = this.keyword();
      this.expect("(");
      const inner = this.list(depth + 1);
      return { typed, value: inner.length === 1 ? (inner[0] as P21Value) : inner };
    }
    const start = this.pos;
    while (this.pos < this.end && isNumberChar(text.charCodeAt(this.pos))) this.pos++;
    const number = Number(text.slice(start, this.pos));
    if (this.pos === start || !Number.isFinite(number)) throw new StepSyntaxError("値を読めません");
    return number;
  }

  private string(): string {
    let i = this.pos + 1;
    for (;;) {
      const close = this.text.indexOf("'", i);
      if (close < 0 || close >= this.end) throw new StepSyntaxError("文字列が閉じていません");
      if (this.text.charCodeAt(close + 1) === 0x27) {
        i = close + 2;
        continue;
      }
      let raw = this.text.slice(this.pos + 1, close);
      if (raw.includes("\n") || raw.includes("\r")) raw = raw.replace(/[\r\n]/g, "");
      this.pos = close + 1;
      return decodeStepString(raw);
    }
  }
}

function headerStrings(text: string): P21File["header"] {
  const end = text.indexOf("DATA;");
  const head = text.slice(0, end > 0 ? end : 4096);
  const strings = (entity: string): string[] => {
    const match = new RegExp(`${entity}\\s*\\(([\\s\\S]*?)\\)\\s*;`).exec(head);
    if (!match?.[1]) return [];
    return Array.from(match[1].matchAll(/'((?:[^']|'')*)'/g), (item) => decodeStepString(item[1] ?? ""));
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

export function readP21(bytes: Uint8Array): P21File {
  const text = latin1(bytes);
  const entities = new Map<number, P21Entity>();
  let skipped = 0;
  const dataStart = text.indexOf("DATA;");
  if (dataStart < 0) throw new Error("P21 の DATA 節がありません");
  const dataEnd = text.indexOf("ENDSEC;", dataStart);
  const end = dataEnd < 0 ? text.length : dataEnd;
  const lexer = new Lexer(text, dataStart + 5, end);
  while (lexer.peek() === "#") {
    const start = lexer.pos;
    try {
      lexer.pos++;
      let id = 0;
      while (isDigit(text.charCodeAt(lexer.pos))) {
        id = id * 10 + (text.charCodeAt(lexer.pos) - 0x30);
        lexer.pos++;
      }
      lexer.expect("=");
      const parts = new Map<string, P21Value[]>();
      if (lexer.peek() === "(") {
        lexer.pos++;
        while (lexer.peek() !== ")") {
          const name = lexer.keyword();
          lexer.expect("(");
          parts.set(name, lexer.list());
        }
        lexer.pos++;
      } else {
        const name = lexer.keyword();
        lexer.expect("(");
        parts.set(name, lexer.list());
      }
      lexer.expect(";");
      entities.set(id, { id, parts });
    } catch (error) {
      if (!(error instanceof StepSyntaxError)) throw error;
      skipped++;
      // 次のインスタンスまで読み飛ばす
      const next = text.indexOf(";", start);
      lexer.pos = next < 0 ? end : next + 1;
    }
  }
  return { entities, header: headerStrings(text), skipped };
}
