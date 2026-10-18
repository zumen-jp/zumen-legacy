import { readFileSync } from "node:fs";

// ローカル開発のポートの正本は config/local-ports.json。PORT などの明示的な上書きだけを受け付ける
const defaults = JSON.parse(readFileSync(new URL("../config/local-ports.json", import.meta.url), "utf8"));

/** @param {string} name @param {string | number | undefined} [override] */
export function port(name, override = undefined) {
  if (!Object.hasOwn(defaults, name)) throw new Error(`Unknown local port: ${name}`);
  const value = String(override ?? defaults[name]);
  if (!/^\d+$/.test(value) || Number(value) < 1024 || Number(value) > 65535) {
    throw new Error(`Use a port between 1024 and 65535: ${name}`);
  }
  return value;
}
