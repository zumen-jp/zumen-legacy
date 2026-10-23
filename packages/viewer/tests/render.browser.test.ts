import type { Scene, SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { renderHighlights, renderScene } from "../src/index.js";

function line(y: number, width: number): SceneEntity {
  return {
    id: "edge",
    kind: "line",
    type: "線分",
    source: "test",
    layers: [0],
    primitives: [
      {
        kind: "stroke",
        points: Float64Array.of(-4, y, 4, y),
        closed: false,
        color: "#ffffff",
        width,
        dashes: [],
      },
    ],
    bbox: { minX: -4, minY: y, maxX: 4, maxY: y },
    properties: [],
  };
}

function scene(entity: SceneEntity): Scene {
  return {
    format: "test",
    version: "1",
    background: "dark",
    sheet: null,
    bounds: entity.bbox,
    layers: [{ id: 0, name: "0", visible: true }],
    entities: [entity],
    notices: [],
    metadata: [],
  };
}

describe("画面の縁の描画", () => {
  it.each([1, 2])("中心線が画面の外でも、太線の見える部分を描く（DPR %s）", (dpr) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 100 * dpr;
    const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
    // 中心線は画面の y = 101、線幅は 10 CSS px。画面内の y = 96〜99 にも線がある。
    renderScene(ctx, scene(line(-5.1, 1)), { scale: 10, offsetX: 50, offsetY: 50 }, 100, 100, {
      background: "#000000",
      invertLight: false,
      hiddenLayers: new Set(),
      lineWeights: true,
      sheetFrame: false,
      devicePixelRatio: dpr,
    });
    expect([...ctx.getImageData(50 * dpr, 98 * dpr, 1, 1).data]).toEqual([255, 255, 255, 255]);
  });

  it("中心線が画面の外でも、強調した細線の見える部分を描く", () => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 100;
    const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
    // 通常は 1 px の細線も、強調すると 3 px の線になる。
    renderHighlights(
      ctx,
      [{ entities: [line(-5.025, 0)], color: "#ff0000" }],
      { scale: 10, offsetX: 50, offsetY: 50 },
      100,
      100,
      {
        background: "#000000",
        lineWeights: false,
        devicePixelRatio: 1,
      },
    );
    expect([...ctx.getImageData(50, 99, 1, 1).data]).toEqual([255, 0, 0, 255]);
  });
});
