import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "vitest";
import { compareScenes } from "../scripts/compare-scenes.mjs";

/**
 * tests/fixtures の正解ファイルと比べる。UPDATE_GOLDEN=1（pnpm run golden）のときは書き直す。
 * 正解は Rust の読み取り（crates/）のテストでも使い、TS と Rust が同じ Scene を返すことを確かめる。
 * Scene の JSON の数は、相対誤差 1e-9 まで同じとみなす（crates/zumen/tests/golden.rs と同じ規則）。
 * Node.js の Math.sin・Math.cos などは CPU（x64 と arm64）で最後の桁が違うことがあり、正解を作った計算機と
 * 違う計算機（CI など）では、文字列としては一致しない。数でない値は同じでなければならない
 */
export function matchGolden(name: string, actual: Uint8Array | string): void {
  const path = fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));
  if (process.env.UPDATE_GOLDEN === "1") {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, actual);
    return;
  }
  const expected = readFileSync(path);
  if (typeof actual !== "string") expect(Buffer.from(actual).equals(expected)).toBe(true);
  else if (name.endsWith(".scene.json")) {
    expect(compareScenes(JSON.parse(expected.toString("utf8")), JSON.parse(actual), 20)).toEqual([]);
  } else expect(actual).toBe(expected.toString("utf8"));
}
