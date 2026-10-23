/** Web Crypto の SubtleCrypto のうち使う部分。core は DOM の型を持たないので、ここで書く */
interface Digest {
  digest(algorithm: "SHA-256", data: Uint8Array): Promise<ArrayBuffer>;
}

/**
 * 図面のバイトの SHA-256 を、小文字の 16 進で返す（FIPS 180-4）。マークアップの文書の drawing.sha256 に入れる。
 * Web Crypto（crypto.subtle.digest）を使うので、ブラウザでは安全なコンテキスト（HTTPS・localhost）が要る
 */
export async function drawingHash(input: ArrayBuffer | Uint8Array): Promise<string> {
  const subtle = (globalThis as { crypto?: { subtle?: Digest } }).crypto?.subtle;
  if (!subtle) throw new Error("Web Crypto（crypto.subtle）が使えないため、SHA-256 を計算できません");
  const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
  const digest = new Uint8Array(await subtle.digest("SHA-256", bytes));
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
