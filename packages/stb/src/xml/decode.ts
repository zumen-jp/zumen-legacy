/** バイト列を XML の文字列にした結果 */
export interface DecodedXml {
  text: string;
  /** 実際に使った文字コード（WHATWG の名前） */
  encoding: string;
  /** XML 宣言の文字コード。宣言がなければ null */
  declared: string | null;
  /** 宣言と違う文字コードで読み直したか */
  fallback: boolean;
}

/** XML 宣言の encoding を TextDecoder の名前にする。ST-Bridge は UTF-8 と Shift_JIS を対象とする（仕様書 1.2） */
function normalizeEncoding(label: string): string {
  const name = label.trim().toLowerCase();
  if (["shift_jis", "shift-jis", "sjis", "x-sjis", "windows-31j", "cp932", "ms932", "ms_kanji"].includes(name)) {
    return "shift_jis";
  }
  if (name === "utf8") return "utf-8";
  return name;
}

function tryDecode(bytes: Uint8Array, encoding: string, fatal: boolean): string | null {
  try {
    return new TextDecoder(encoding, { fatal }).decode(bytes);
  } catch {
    return null;
  }
}

/**
 * 先頭の BOM と XML 宣言から文字コードを決めて文字列にする。宣言と中身が食い違うファイルがあるため、
 * UTF-8 と宣言して UTF-8 として正しくない場合は Shift_JIS で、Shift_JIS と宣言して UTF-8 の多バイト文字として
 * 正しい並びの場合は UTF-8 で読み直し、その旨を返す（Shift_JIS の日本語が UTF-8 として正しい並びになることはまずない）。
 */
export function decodeXml(bytes: Uint8Array): DecodedXml {
  if (bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf) {
    return {
      text: new TextDecoder("utf-8").decode(bytes.subarray(3)),
      encoding: "utf-8",
      declared: null,
      fallback: false,
    };
  }
  if (bytes[0] === 0xff && bytes[1] === 0xfe) {
    return {
      text: new TextDecoder("utf-16le").decode(bytes.subarray(2)),
      encoding: "utf-16le",
      declared: null,
      fallback: false,
    };
  }
  if (bytes[0] === 0xfe && bytes[1] === 0xff) {
    return {
      text: new TextDecoder("utf-16be").decode(bytes.subarray(2)),
      encoding: "utf-16be",
      declared: null,
      fallback: false,
    };
  }
  const head = new TextDecoder("latin1").decode(bytes.subarray(0, Math.min(bytes.length, 512)));
  const match = /<\?xml[^>]*?encoding\s*=\s*["']([^"']+)["']/i.exec(head);
  const declared = match?.[1] ?? null;
  const encoding = declared ? normalizeEncoding(declared) : "utf-8";
  if (encoding === "utf-8") {
    const text = tryDecode(bytes, "utf-8", true);
    if (text !== null) return { text, encoding, declared, fallback: false };
    const sjis = tryDecode(bytes, "shift_jis", true);
    if (sjis !== null) return { text: sjis, encoding: "shift_jis", declared, fallback: true };
    return { text: new TextDecoder("utf-8").decode(bytes), encoding, declared, fallback: false };
  }
  if (encoding === "shift_jis" && bytes.some((byte) => byte >= 0x80)) {
    const utf8 = tryDecode(bytes, "utf-8", true);
    if (utf8 !== null) return { text: utf8, encoding: "utf-8", declared, fallback: true };
  }
  const text = tryDecode(bytes, encoding, false);
  if (text === null) throw new Error(`文字コード ${declared} の XML は読めません`);
  return { text, encoding, declared, fallback: false };
}
