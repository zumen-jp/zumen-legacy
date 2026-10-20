import { readFileSync } from "node:fs";

/** Cargo.lock may have CRLF after a Windows checkout. */
export function wasmBindgenVersion(lock) {
  const version = /^name = "wasm-bindgen"\r?\nversion = "([^"]+)"/m.exec(lock)?.[1];
  if (!version) throw new Error("Cargo.lock に wasm-bindgen がありません");
  return version;
}

export const version = wasmBindgenVersion(readFileSync(new URL("../Cargo.lock", import.meta.url), "utf8"));
