import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "vitest";

/**
 * tests/fixtures の正解ファイルと比べる。UPDATE_GOLDEN=1（pnpm run golden）のときは書き直す。
 * 正解は Rust の読み取り（crates/）のテストでも使い、TS と Rust が同じ Scene を返すことを確かめる。
 */
export function matchGolden(name: string, actual: Uint8Array | string): void {
  const path = fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));
  if (process.env.UPDATE_GOLDEN === "1") {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, actual);
    return;
  }
  const expected = readFileSync(path);
  if (typeof actual === "string") expect(actual).toBe(expected.toString("utf8"));
  else expect(Buffer.from(actual).equals(expected)).toBe(true);
}
