import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { apply } from "@zumenjs/core/geometry";
import { describe, expect, it } from "vitest";
import { jww } from "../src/index.js";
import { build, buildImages, buildSolids } from "./fixture.js";

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

  it("レイヤの名前と表示状態、線種のビット列を読む", () => {
    expect(scene.layers).toContainEqual({ id: 1, name: "0-0 通り芯", visible: true, group: "平面" });
    expect(scene.layers).toContainEqual(expect.objectContaining({ id: 3, visible: false }));
    expect(entity(scene.entities, "#2").primitives[0]).toMatchObject({ dashes: [2, 2] });
  });

  it("Jw_cad が文字として保存した設定は描かず、全体表示の範囲を用紙から広げない", () => {
    expect(scene.entities.map((item) => item.id)).not.toContain("#6");
    expect(scene.bounds.minY).toBeGreaterThanOrEqual(0);
  });

  it("円弧角が負なら時計回りに描き、ブロックの中の図形を共有のクラス番号で読む", () => {
    const arc = entity(scene.entities, "#4");
    expect(arc.bbox.minY).toBeCloseTo(138.5, 6);
    expect(arc.bbox.maxY).toBeCloseTo(148.5, 6);
    expect(points(entity(scene.entities, "#5>#1"))).toEqual([260, 148.5, 262, 148.5]);
  });

  it("版 300 より前の図面も、ヘッダのレイヤ名・線色・線幅・線種を読む", () => {
    const old = readDrawing(build(200), [jww]);
    expect(old.layers).toContainEqual({ id: 1, name: "0-0 通り芯", visible: true, group: "平面" });
    expect(entity(old.entities, "#1").primitives[0]).toMatchObject({ width: 0.25, dashes: [] });
    expect(entity(old.entities, "#2").primitives[0]).toMatchObject({ dashes: [2, 2] });
    expect(old.notices.map((notice) => notice.code)).not.toContain("default-palette");
  });

  it.each([600, 700])("版 %i の文字列（CP932・UTF-16）を復号する", (version) => {
    const text = entity(readDrawing(build(version), [jww]).entities, "#3");
    expect(text.properties).toContainEqual({ key: "text", label: "文字列", value: "表示ソ能" });
  });
});

describe("JWW の画像の配置（^@BM）", () => {
  it("文字の始点を画像の左下に、文字の向きに回して置き、外形は選ぶためだけに残す", () => {
    const scene = readDrawing(buildImages(700), [jww]);
    const rotated = scene.underlays?.find((underlay) => underlay.file === "C:\\images\\site.jpg");
    if (!rotated) throw new Error("回した画像の underlay がありません");
    // 用紙 A3 の中心が原点。始点 (20, 10) を 30° 回した 30 × 15 mm
    const [x0, y0] = [210 + 20, 148.5 + 10];
    const [lowerLeftX, lowerLeftY] = apply(rotated.matrix, 0, rotated.height);
    expect(lowerLeftX).toBeCloseTo(x0, 9);
    expect(lowerLeftY).toBeCloseTo(y0, 9);
    const [lowerRightX, lowerRightY] = apply(rotated.matrix, rotated.width, rotated.height);
    expect(lowerRightX).toBeCloseTo(x0 + 30 * Math.cos(Math.PI / 6), 9);
    expect(lowerRightY).toBeCloseTo(y0 + 30 * Math.sin(Math.PI / 6), 9);
    expect(entity(scene.entities, rotated.id).drawn).toBe(false);
    expect(scene.underlays?.map((underlay) => underlay.file)).toEqual([
      "%temp%photo.bmp",
      "C:\\images\\site.jpg",
      "logo.png",
    ]);
  });
});
