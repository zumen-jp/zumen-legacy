import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { apply } from "@zumenjs/core/geometry";
import { describe, expect, it } from "vitest";
import { sxf } from "../src/index.js";
import { rasterFile } from "../src/scene.js";
import { SFC_RASTERS, SFC_SAMPLE, sfcBytes } from "./fixtures.js";

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

function points(item: SceneEntity, index = 0): number[] {
  const primitive = item.primitives[index];
  if (primitive?.kind !== "stroke") throw new Error("線ではありません");
  return Array.from(primitive.points, (value) => Math.round(value * 1e6) / 1e6);
}

// 用紙・レイヤ・文字・配置・線種・塗り・notices は golden.test.ts が Scene 全体で確かめる。
// ここでは計測との組み合わせと、正解ファイルにない異常入力を確かめる。
describe("SFC の読み取り", () => {
  const scene = readDrawing(sfcBytes(SFC_SAMPLE), [sxf]);

  it("部分図の縮尺を用紙の座標に解決し、計測は元の座標の長さを返す", () => {
    const line = entity(scene.entities, "#120>#80");
    expect(points(line)).toEqual([100, 50, 150, 50]);
    expect(line.layers).toEqual([1, 1]);
    expect(line.placement?.scaleX).toBeCloseTo(0.05);
    const result = measureDistance({ x: 100, y: 50, entity: line }, { x: 150, y: 50, entity: line });
    expect(result.paper).toBeCloseTo(50);
    expect(result.real).toBeCloseTo(1000);
  });

  it("入れ子が深すぎる引数のフィーチャは、読めないものとして省き、他は読む", () => {
    const nested = `${"(".repeat(70)}'1'${")".repeat(70)}`;
    const record = `/*SXF\n#900 = polyline_feature('1','1','1','1','1',${nested},'(0)')\nSXF*/\n`;
    const deep = readDrawing(sfcBytes(SFC_SAMPLE.replace("ENDSEC;\nEND-ISO", `${record}ENDSEC;\nEND-ISO`)), [sxf]);
    expect(deep.notices).toContainEqual(expect.objectContaining({ code: "unreadable", count: 1 }));
    expect(deep.entities).toHaveLength(scene.entities.length);
  });
});

describe("SXF の画像（ラスタ）", () => {
  it("名前の属性付加機構の「画像」と Ver.2.0 の $$RASTER$$ からファイルを取り出し、ほかの属性は画像としない", () => {
    expect(rasterFile("$$ATRU$$100$$地形図$$画像$$GENKYOU.TIF")).toBe("GENKYOU.TIF");
    expect(rasterFile("$$ATRU$$1$$RASTER$$画像$$202607066-00.tif")).toBe("202607066-00.tif");
    expect(rasterFile("$$RASTER$$GENKYOU.TIF")).toBe("GENKYOU.TIF");
    expect(rasterFile("$$ATRU$$2$$背景色$$色$$0_0_0")).toBeNull();
    expect(rasterFile("$$ATRU$$100$$FUJI$$等高線$$3770")).toBeNull();
    expect(rasterFile("平面図")).toBeNull();
  });

  it("範囲の折線の左下を中身の左下に、反時計回りなら 2 番目の点を右下に置く", () => {
    const scene = readDrawing(sfcBytes(SFC_RASTERS), [sxf]);
    const [clockwise, rotated] = scene.underlays ?? [];
    if (!clockwise || !rotated) throw new Error("画像の underlay がありません");
    const close = (point: [number, number], expected: [number, number]) => {
      expect(point[0]).toBeCloseTo(expected[0], 4);
      expect(point[1]).toBeCloseTo(expected[1], 4);
    };
    close(apply(clockwise.matrix, 0, clockwise.height), [20, 30]);
    close(apply(clockwise.matrix, clockwise.width, 0), [120, 80]);
    expect(clockwise.layers).toEqual([2]);
    close(apply(rotated.matrix, 0, rotated.height), [150, 30]);
    close(apply(rotated.matrix, rotated.width, rotated.height), [184.641016, 50]);
    // 中身の座標は用紙の 0.01 mm
    expect(rotated.width).toBeCloseTo(4000, 3);
  });
});
