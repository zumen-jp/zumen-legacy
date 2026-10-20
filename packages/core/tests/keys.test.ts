import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { ENTITY_KINDS, type Property, type Scene } from "../src/index.js";

const FIXTURES = fileURLToPath(new URL("../../../tests/fixtures", import.meta.url));

/** すべての形式の正解（*.scene.json）。TypeScript と Rust の読み取りが同じ Scene を返すので、両方の確認になる */
const scenes: [string, Scene][] = readdirSync(FIXTURES, { withFileTypes: true }).flatMap((entry) => {
  if (!entry.isDirectory()) return [];
  const format = entry.name;
  const folder = join(FIXTURES, format);
  return readdirSync(folder)
    .filter((name) => name.endsWith(".scene.json"))
    .map((name): [string, Scene] => [`${format}/${name}`, JSON.parse(readFileSync(join(folder, name), "utf8"))]);
});

/** raw の形。数・文字列・真偽値か、数の並びの長さ */
function shape(property: Property): string | null {
  const raw = property.raw;
  if (raw === undefined) return null;
  return Array.isArray(raw) ? `number[${raw.length}]` : typeof raw;
}

function check(where: string, properties: Property[], shapes: Map<string, Set<string>>): void {
  const keys = properties.map((property) => property.key);
  expect(new Set(keys).size, `${where}: ${keys.join(", ")}`).toBe(keys.length);
  for (const property of properties) {
    expect(property.key, where).toMatch(/^[a-z][A-Za-z]*$/);
    const kind = shape(property);
    if (kind === null) continue;
    if (Array.isArray(property.raw)) expect(property.raw.every((value) => typeof value === "number")).toBe(true);
    const seen = shapes.get(property.key) ?? new Set();
    seen.add(kind);
    shapes.set(property.key, seen);
  }
}

describe("安定したキー", () => {
  it("正解がある", () => {
    expect(scenes.length).toBeGreaterThan(0);
  });

  it("図形の kind は決めた一覧の値で、key は 1 つの図形・図面の中で重ならない", () => {
    const shapes = new Map<string, Set<string>>();
    for (const [name, scene] of scenes) {
      check(`${name} metadata`, scene.metadata, new Map());
      for (const entity of scene.entities) {
        expect(ENTITY_KINDS, `${name} ${entity.id}`).toContain(entity.kind);
        check(`${name} ${entity.id}`, entity.properties, shapes);
        expect(entity.properties[0]?.key).toBe("type");
      }
    }
    // 同じ key の raw は、どの形式でも同じ形（アプリケーションが形式で分けずに読める）
    for (const [key, seen] of shapes) expect([...seen], key).toHaveLength(1);
  });

  it("どの形式の図形も、種類・識別子・レイヤを同じ key で持つ", () => {
    for (const [name, scene] of scenes) {
      for (const entity of scene.entities) {
        const keys = entity.properties.map((property) => property.key);
        for (const key of ["type", "id", "layer"]) expect(keys, `${name} ${entity.id}`).toContain(key);
      }
    }
  });
});
