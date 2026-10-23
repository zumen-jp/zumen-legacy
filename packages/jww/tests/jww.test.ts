import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { jww } from "../src/index.js";
import { build, buildSolids } from "./fixture.js";

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

function points(item: SceneEntity): number[] {
  const primitive = item.primitives[0];
  if (primitive?.kind !== "stroke") throw new Error("線ではありません");
  return Array.from(primitive.points, (value) => Math.round(value * 1e6) / 1e6);
}

// 用紙・レイヤ・線種・文字コード・円弧・ブロック・非表示の設定は golden.test.ts が各版の Scene 全体で確かめる。
// ここではソリッドの面積と、計測との組み合わせを確かめる。
describe("JWW の読み取り", () => {
  const scene = readDrawing(build(600), [jww]);

  it.each([600, 700])("版 %i の矩形・斜めの四角形・三角形のソリッドを交差させない", (version) => {
    const solids = readDrawing(buildSolids(version), [jww]).entities;
    const expectedAreas = [200, 158, 50];
    for (const [index, solid] of solids.entries()) {
      const primitive = solid.primitives[0];
      if (primitive?.kind !== "fill") throw new Error("塗りではありません");
      const ring = primitive.rings[0] as Float64Array;
      let twiceArea = 0;
      for (let i = 0; i < ring.length; i += 2) {
        const next = (i + 2) % ring.length;
        twiceArea +=
          (ring[i] as number) * (ring[next + 1] as number) - (ring[next] as number) * (ring[i + 1] as number);
      }
      expect(Math.abs(twiceArea) / 2).toBeCloseTo(expectedAreas[index] as number, 8);
      expect(primitive.color).toBe("#c0c0c0");
    }
    expect(solids).toHaveLength(3);
  });

  it("用紙の中心が原点の座標を左下原点にし、縮尺 1/100 から実寸を計測する", () => {
    expect(scene.sheet).toMatchObject({ size: "A3", width: 420, height: 297 });
    const line = entity(scene.entities, "#1");
    expect(points(line)).toEqual([110, 148.5, 210, 148.5]);
    expect(line.primitives[0]).toMatchObject({ width: 0.25, dashes: [] });
    expect(measureDistance({ x: 110, y: 148.5, entity: line }, { x: 210, y: 148.5, entity: line }).real).toBeCloseTo(
      10000,
    );
  });
});
