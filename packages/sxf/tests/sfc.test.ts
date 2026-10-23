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

// 並び順で決まるコード・所属の規則を、1 つの図面（fixtures.ts の SFC_SAMPLE）で確かめる
describe("SFC の読み取り", () => {
  const scene = readDrawing(sfcBytes(SFC_SAMPLE), [sxf]);

  it("用紙・版・出現順のレイヤコードを読む（用紙より後のレイヤも含む）", () => {
    expect(scene.format).toBe("sfc");
    expect(scene.version).toBe("SXF 3.1");
    expect(scene.sheet).toEqual({ name: "道路", size: "A1 横", width: 841, height: 594 });
    expect(scene.layers).toEqual([
      { id: 1, name: "部分", visible: true },
      { id: 2, name: "図", visible: false },
    ]);
  });

  it("2 バイト目が 0x5C の文字と、\\\\ で書いた \\ を取り違えずに復号する", () => {
    const text = entity(scene.entities, "#120>#90");
    expect(text.properties).toContainEqual({ key: "text", label: "文字列", value: "表示ソ能\\x" });
  });

  it("部分図の縮尺を用紙の座標に解決し、計測は元の座標の長さを返す", () => {
    const line = entity(scene.entities, "#120>#80");
    expect(points(line)).toEqual([100, 50, 150, 50]);
    expect(line.layers).toEqual([1, 1]);
    expect(line.placement?.scaleX).toBeCloseTo(0.05);
    const result = measureDistance({ x: 100, y: 50, entity: line }, { x: 150, y: 50, entity: line });
    expect(result.paper).toBeCloseTo(50);
    expect(result.real).toBeCloseTo(1000);
  });

  it("作図グループは配置の値を使わず、測地座標系の部分図は X と Y を入れ替える", () => {
    expect(points(entity(scene.entities, "#240>#200"))).toEqual([0, 0, 10, 0]);
    expect(points(entity(scene.entities, "#250>#220"))).toEqual([300, 300, 300, 310]);
  });

  it("色・線種・線幅のユーザ定義コードを出現順に割り当て、既定義の値の線幅はコードを消費しない", () => {
    const fill = entity(scene.entities, "#110");
    expect(fill.layers).toEqual([2]);
    const [area, outline] = fill.primitives;
    expect(area).toMatchObject({ kind: "fill", color: "#0a141e" });
    expect(area?.kind === "fill" ? area.rings[0]?.length : 0).toBe(10);
    expect(outline).toMatchObject({ kind: "stroke", color: "#0a141e", width: 0.4, dashes: [2, 1], closed: true });
    expect(entity(scene.entities, "#120>#80").primitives[0]).toMatchObject({ width: 0.25 });
  });

  it("入れ子が深すぎる引数のフィーチャは、読めないものとして省き、他は読む", () => {
    const nested = `${"(".repeat(70)}'1'${")".repeat(70)}`;
    const record = `/*SXF\n#900 = polyline_feature('1','1','1','1','1',${nested},'(0)')\nSXF*/\n`;
    const deep = readDrawing(sfcBytes(SFC_SAMPLE.replace("ENDSEC;\nEND-ISO", `${record}ENDSEC;\nEND-ISO`)), [sxf]);
    expect(deep.notices).toContainEqual(expect.objectContaining({ code: "unreadable", count: 1 }));
    expect(deep.entities).toHaveLength(scene.entities.length);
  });

  it("用紙より後に書かれた要素は読み落とさず、注意として件数を返す", () => {
    expect(points(entity(scene.entities, "#160"))).toEqual([0, 0, 1, 1]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "after-sheet", count: 1 }));
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
