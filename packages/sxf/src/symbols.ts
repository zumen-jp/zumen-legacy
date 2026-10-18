import type { Primitive, StrokePrimitive } from "@zumenjs/core";

/**
 * 矢印・点マーカの形。大きさは用紙上の mm で、部分図の縮尺や部品の倍率の影響を受けない
 * （SXF 実装規約 1-4・1-6）。矢印は線種によらず実線で描く。
 */

function stroke(points: number[], closed: boolean, color: string, width: number): StrokePrimitive {
  return { kind: "stroke", points: Float64Array.from(points), closed, color, width, dashes: [] };
}

function circlePoints(cx: number, cy: number, r: number, segments = 24): number[] {
  const out: number[] = [];
  for (let i = 0; i < segments; i++) {
    const t = (i / segments) * Math.PI * 2;
    out.push(cx + r * Math.cos(t), cy + r * Math.sin(t));
  }
  return out;
}

/** 局所座標（x: 矢印の向き、y: 左）で作った点列を、先端 (x, y)・向き (ux, uy) に置く */
function place(points: number[], x: number, y: number, ux: number, uy: number): number[] {
  const out: number[] = [];
  for (let i = 0; i + 1 < points.length; i += 2) {
    const px = points[i] as number;
    const py = points[i + 1] as number;
    out.push(x + px * ux - py * uy, y + px * uy + py * ux);
  }
  return out;
}

/**
 * 矢印を作る。先端 (x, y)、矢印が指す向き (ux, uy)（単位ベクトル）、倍率 scale。
 * 基本の大きさは長さ 7.5mm・幅 2.5mm、箱と点は 2.5mm（附属書 SFC 編 図 19）。
 */
export function arrowPrimitives(
  code: number,
  x: number,
  y: number,
  ux: number,
  uy: number,
  scale: number,
  color: string,
  width: number,
  background: string,
): Primitive[] {
  const s = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const length = 7.5 * s;
  const half = 1.25 * s;
  const size = 2.5 * s;
  // 先端から後ろへ伸びる三角形（局所座標では先端が原点で、本体は -x 側）
  const triangle = place([0, 0, -length, half, -length, -half], x, y, ux, uy);
  const box = place([0, -size / 2, -size, -size / 2, -size, size / 2, 0, size / 2], x, y, ux, uy);
  switch (code) {
    case 1:
      return [
        { kind: "fill", color: background, rings: [Float64Array.from(triangle)] },
        stroke(triangle, true, color, width),
      ];
    case 2:
      return [{ kind: "fill", color: background, rings: [Float64Array.from(box)] }, stroke(box, true, color, width)];
    case 3: {
      const dot = circlePoints(x, y, size / 2);
      return [{ kind: "fill", color: background, rings: [Float64Array.from(dot)] }, stroke(dot, true, color, width)];
    }
    case 4:
      return [stroke(circlePoints(x, y, size / 2), true, color, width)];
    case 5:
      return [{ kind: "fill", color, rings: [Float64Array.from(box)] }];
    case 6:
      return [{ kind: "fill", color, rings: [Float64Array.from(triangle)] }];
    case 7:
      return [{ kind: "fill", color, rings: [Float64Array.from(circlePoints(x, y, size / 2))] }];
    case 8: {
      // 積分記号：先端を中心に 45 度傾けた S 字
      const curve: number[] = [];
      for (let i = 0; i <= 16; i++) {
        const t = -1 + (2 * i) / 16;
        curve.push(t * size * 0.35 + Math.sin(t * Math.PI) * size * 0.15, t * size * 0.5);
      }
      return [stroke(place(curve, x, y, ux, uy), false, color, width)];
    }
    case 9:
      return [stroke(place([-length, half, 0, 0, -length, -half], x, y, ux, uy), false, color, width)];
    case 10: {
      const d = (size / 2) * Math.SQRT1_2;
      return [stroke(place([-d, -d, d, d], x, y, ux, uy), false, color, width)];
    }
    case 11:
      return [stroke(triangle, true, color, width)];
    default:
      return [];
  }
}

/** 点マーカ。dot（3）は画面上で一定の大きさ。他は 2.5mm × 倍率を、用紙に対する角度で描く */
export function markerPrimitives(
  code: number,
  x: number,
  y: number,
  angleDeg: number,
  scale: number,
  color: string,
  width: number,
): Primitive[] {
  if (code === 3) return [{ kind: "dot", x, y, color }];
  const s = (Number.isFinite(scale) && scale > 0 ? scale : 1) * 2.5;
  const h = s / 2;
  const r = (angleDeg * Math.PI) / 180;
  const ux = Math.cos(r);
  const uy = Math.sin(r);
  const at = (points: number[]) => place(points, x, y, ux, uy);
  switch (code) {
    case 1:
      return [0, 60, 120].map((deg) => {
        const t = (deg * Math.PI) / 180;
        return stroke(at([-h * Math.cos(t), -h * Math.sin(t), h * Math.cos(t), h * Math.sin(t)]), false, color, width);
      });
    case 2:
      return [stroke(circlePoints(x, y, h), true, color, width)];
    case 4:
      return [stroke(at([-h, 0, h, 0]), false, color, width), stroke(at([0, -h, 0, h]), false, color, width)];
    case 5:
      return [stroke(at([-h, -h, h, -h, h, h, -h, h]), true, color, width)];
    case 6: {
      const top = (s * Math.sqrt(3)) / 3;
      const bottom = -(s * Math.sqrt(3)) / 6;
      return [stroke(at([0, top, -h, bottom, h, bottom]), true, color, width)];
    }
    case 7:
      return [stroke(at([-h, -h, h, h]), false, color, width), stroke(at([-h, h, h, -h]), false, color, width)];
    default:
      return [{ kind: "dot", x, y, color }];
  }
}
