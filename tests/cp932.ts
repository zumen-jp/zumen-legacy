/**
 * テスト用の SFC を CP932 のバイト列にする。使う文字だけを対応表に持つ。
 * 「表」「ソ」「能」は 2 バイト目が 0x5C（\）になる文字で、区切り・エスケープと取り違えないことを確かめる。
 */
const TABLE: Readonly<Record<string, readonly number[]>> = {
  表: [0x95, 0x5c],
  示: [0x8e, 0xa6],
  ソ: [0x83, 0x5c],
  能: [0x94, 0x5c],
  部: [0x95, 0x94],
  分: [0x95, 0xaa],
  図: [0x90, 0x7d],
  平: [0x95, 0xbd],
  面: [0x96, 0xca],
  道: [0x93, 0xb9],
  路: [0x98, 0x48],
  メ: [0x83, 0x81],
  モ: [0x83, 0x82],
  通: [0x92, 0xca],
  り: [0x82, 0xe8],
  芯: [0x90, 0x63],
  品: [0x95, 0x69],
};

export function cp932(text: string): Uint8Array {
  const bytes: number[] = [];
  for (const char of text) {
    const mapped = TABLE[char];
    if (mapped) bytes.push(...mapped);
    else {
      const code = char.charCodeAt(0);
      if (code > 0x7f) throw new Error(`対応表にない文字: ${char}`);
      bytes.push(code);
    }
  }
  return Uint8Array.from(bytes);
}
