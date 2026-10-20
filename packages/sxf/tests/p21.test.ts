import { readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { sxf } from "../src/index.js";
import { P21_FEATURES, P21_SAMPLE, p21Bytes } from "./fixtures.js";

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

// AP202 の参照の網から SFC と同じ図面になることを、1 つの図面（fixtures.ts の P21_SAMPLE）で確かめる
describe("P21 の読み取り", () => {
  const scene = readDrawing(p21Bytes(P21_SAMPLE), [sxf]);

  it("用紙・\\X2\\ の文字列・レイヤと非表示の指定を読む（子図用の仮レイヤは除く）", () => {
    expect(scene.format).toBe("p21");
    expect(scene.version).toBe("SXF 3.1");
    expect(scene.sheet).toEqual({ name: "表示", size: "A1 横", width: 841, height: 594 });
    expect(scene.layers).toEqual([
      { id: 1, name: "線", visible: true },
      { id: 2, name: "DIM", visible: false },
    ]);
  });

  it("8 ビットのまま書いた Shift_JIS を、2 バイト目が 0x5C（\\）や 0x40〜0x7E の文字も分けずに読む", () => {
    // 表 = 95 5C、ハ = 83 6E。0x82〜0x9F の先頭バイトは windows-1252 として読むと別の文字になる
    const shiftJis = readDrawing(p21Bytes(P21_SAMPLE.replace("'DIM'", "'表ハッチ'")), [sxf]);
    expect(shiftJis.layers.map((layer) => layer.name)).toEqual(["線", "表ハッチ"]);
  });

  it("入れ子が深すぎる値のインスタンスと、64 段より深い子図の定義は読まずに件数を知らせる", () => {
    const features = readDrawing(p21Bytes(P21_FEATURES), [sxf]);
    expect(features.notices).toContainEqual(expect.objectContaining({ code: "unreadable", count: 4 }));
    expect(features.notices).toContainEqual(expect.objectContaining({ code: "sfig-depth", count: 1 }));
  });

  it("子図の縮尺を解決し、時計回りの円弧（.F.）を逆回りに描かない", () => {
    const line = entity(scene.entities, "#77>#46");
    expect(Array.from((line.primitives[0] as { points: Float64Array }).points)).toEqual([100, 50, 150, 50]);
    expect(line.placement?.name).toContain("平面図");
    expect(line.placement?.scaleX).toBeCloseTo(0.05);
    // 0° から時計回りに 90° まで回る円弧は、中心の真下（270°）を通る
    expect(entity(scene.entities, "#77>#53").bbox.minY).toBeCloseTo(49.5, 6);
  });

  it("文字の配置点をベースラインから SXF の配置基点（右上）へ戻す", () => {
    const text = entity(scene.entities, "#77>#69");
    expect(text.properties).toContainEqual({ key: "text", label: "文字列", value: "表示" });
    expect(text.bbox.minX).toBeCloseTo(90, 6);
    expect(text.bbox.maxX).toBeCloseTo(105, 6);
    expect(text.bbox.minY).toBeCloseTo(57.5, 6);
    expect(text.bbox.maxY).toBeCloseTo(60, 6);
  });

  it("矢印の向きを外向き・内向きにし、寸法と塗りのレイヤ・外形の非表示を反映する", () => {
    const dimension = entity(scene.entities, "#98");
    expect(dimension.layers).toEqual([2]);
    expect(dimension.properties).toContainEqual({
      key: "arrows",
      label: "矢印",
      value: "filled arrow（外向き）・filled arrow（内向き）",
    });
    const fill = entity(scene.entities, "#113");
    expect(fill.layers).toEqual([2]);
    expect(fill.primitives.map((primitive) => primitive.kind)).toEqual(["fill"]);
    expect(fill.primitives[0]).toMatchObject({ color: "#c00080" });
  });
});
