import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { jww } from "../src/index.js";
import { build } from "./fixture.js";

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
