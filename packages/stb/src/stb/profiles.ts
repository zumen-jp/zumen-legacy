import { circle, type Profile, type ProfilePart } from "../geometry.js";
import type { Vec2 } from "../types.js";

/**
 * 断面の 2D 輪郭。座標は (b, h) で、h が「成」の方向（鉄骨断面の基準方向、仕様書 6.26）、b が幅の方向。
 * 原点は外形を包絡する長方形の中心。梁は呼び出し側で天端の中心へ移す。
 * 溝形鋼・山形鋼などの開いた側は −b とする（梁では進行方向に向かって右、仕様書 6.26.9 補足(2)）。
 */

/** b0〜b1、h0〜h1 の長方形の頂点 */
export const rect = (b0: number, h0: number, b1: number, h1: number): Vec2[] => [
  [b0, h0],
  [b1, h0],
  [b1, h1],
  [b0, h1],
];

export const single = (outer: Vec2[], holes: Vec2[][] = []): Profile => ({ parts: [{ outer, holes }], smooth: false });

/** 長方形（幅 b × 成 h） */
export function rectangle(width: number, depth: number): Profile {
  return single(rect(-width / 2, -depth / 2, width / 2, depth / 2));
}

export function solidCircle(diameter: number): Profile {
  return { parts: [{ outer: circle(diameter / 2), holes: [] }], smooth: true };
}

/** 円形鋼管（仕様書 6.26.6）。filled は CFT のようにコンクリートを充填した外形 */
export function pipe(diameter: number, thickness: number, filled = false): Profile {
  const inner = diameter / 2 - thickness;
  const holes = !filled && inner > 0 ? [circle(inner)] : [];
  return { parts: [{ outer: circle(diameter / 2), holes }], smooth: true };
}

/** H 形鋼（6.26.1〜6.26.3）。上下のフランジ幅・厚さが違う組立 H 形鋼も扱う */
export function hShape(
  depth: number,
  widthTop: number,
  web: number,
  flangeTop: number,
  widthBottom = widthTop,
  flangeBottom = flangeTop,
): Profile {
  const h = depth / 2;
  const bt = widthTop / 2;
  const bb = widthBottom / 2;
  const w = web / 2;
  return single([
    [-bb, -h],
    [bb, -h],
    [bb, -h + flangeBottom],
    [w, -h + flangeBottom],
    [w, h - flangeTop],
    [bt, h - flangeTop],
    [bt, h],
    [-bt, h],
    [-bt, h - flangeTop],
    [-w, h - flangeTop],
    [-w, -h + flangeBottom],
    [-bb, -h + flangeBottom],
  ]);
}

function roundedRect(width: number, depth: number, radius: number): Vec2[] {
  const b = width / 2;
  const h = depth / 2;
  const r = Math.max(0, Math.min(radius, b, h));
  if (r < 1e-6) return rect(-b, -h, b, h);
  const points: Vec2[] = [];
  const steps = 2;
  const corners: [number, number, number][] = [
    [b - r, -h + r, -90],
    [b - r, h - r, 0],
    [-b + r, h - r, 90],
    [-b + r, -h + r, 180],
  ];
  for (const [cx, cy, start] of corners) {
    for (let i = 0; i <= steps; i++) {
      const a = ((start + (90 * i) / steps) * Math.PI) / 180;
      points.push([cx + r * Math.cos(a), cy + r * Math.sin(a)]);
    }
  }
  return points;
}

/**
 * 角形鋼管（6.26.4・6.26.5）。成 A を h、幅 B を b にとる。
 * 冷間成形はコーナー半径 r の外形と、r − t の内側の穴にする。組立は t1 を成方向の板（側面）、t2 を幅方向の板とする。
 */
export function box(depth: number, width: number, side: number, top: number, radius = 0, filled = false): Profile {
  const outer = roundedRect(width, depth, radius);
  const innerWidth = width - 2 * side;
  const innerDepth = depth - 2 * top;
  if (filled || innerWidth <= 0 || innerDepth <= 0) return single(outer);
  return single(outer, [roundedRect(innerWidth, innerDepth, Math.max(0, radius - Math.max(side, top)))]);
}

/** T 形鋼（6.26.7・6.26.8）。フランジを上（+h）にする */
export function tShape(depth: number, width: number, web: number, flange: number): Profile {
  const h = depth / 2;
  const b = width / 2;
  const w = web / 2;
  return single([
    [-w, -h],
    [w, -h],
    [w, h - flange],
    [b, h - flange],
    [b, h],
    [-b, h],
    [-b, h - flange],
    [-w, h - flange],
  ]);
}

/** 溝形鋼（6.26.9）。ウェブを +b 側に置き、−b 側を開く。right = true で向きを反転する */
function channelPoints(depth: number, width: number, web: number, flange: number, mirror: boolean): Vec2[] {
  const h = depth / 2;
  const b = width / 2;
  const points: Vec2[] = [
    [b, -h],
    [b, h],
    [-b, h],
    [-b, h - flange],
    [b - web, h - flange],
    [b - web, -h + flange],
    [-b, -h + flange],
    [-b, -h],
  ];
  return mirror ? points.map(([x, y]) => [-x, y] as const).reverse() : points;
}

export function channel(depth: number, width: number, web: number, flange: number): Profile {
  return single(channelPoints(depth, width, web, flange, false));
}

/** 山形鋼（6.26.11）。成方向の脚を +b 側、幅方向の脚を上に置く（Γ 形）。 */
function anglePoints(depth: number, width: number, leg: number, flange: number, mirror: boolean): Vec2[] {
  const h = depth / 2;
  const b = width / 2;
  const points: Vec2[] = [
    [b, -h],
    [b, h],
    [-b, h],
    [-b, h - flange],
    [b - leg, h - flange],
    [b - leg, -h],
  ];
  return mirror ? points.map(([x, y]) => [-x, y] as const).reverse() : points;
}

export function angle(depth: number, width: number, leg: number, flange: number): Profile {
  return single(anglePoints(depth, width, leg, flange, false));
}

/** リップ溝形鋼（6.26.13）。H: 成、A: 幅、C: リップ長、t: 板厚 */
function lipChannelPoints(depth: number, width: number, lip: number, t: number, mirror: boolean): Vec2[] {
  const h = depth / 2;
  const b = width / 2;
  const points: Vec2[] = [
    [b, -h],
    [b, h],
    [-b, h],
    [-b, h - lip],
    [-b + t, h - lip],
    [-b + t, h - t],
    [b - t, h - t],
    [b - t, -h + t],
    [-b + t, -h + t],
    [-b + t, -h + lip],
    [-b, -h + lip],
    [-b, -h],
  ];
  return mirror ? points.map(([x, y]) => [-x, y] as const).reverse() : points;
}

export function lipChannel(depth: number, width: number, lip: number, t: number): Profile {
  return single(lipChannelPoints(depth, width, lip, t, false));
}

/**
 * 2 丁合わせ（6.26.10・6.26.12・6.26.14）。背中合わせは背を中心に向け、表合わせは開いた側を中心に向ける。
 * gap は 2 丁の間の距離。単体の輪郭は幅 width の中心が原点。
 */
function double(points: (mirror: boolean) => Vec2[], width: number, gap: number, backToBack: boolean): Profile {
  const offset = width / 2 + gap / 2;
  // +b 側に背（ウェブ）がある単体を、背中合わせなら左に、反転したものを右に置く
  const left = points(!backToBack).map(([b, h]) => [b - offset, h] as const);
  const right = points(backToBack).map(([b, h]) => [b + offset, h] as const);
  const parts: ProfilePart[] = [
    { outer: left, holes: [] },
    { outer: right, holes: [] },
  ];
  return { parts, smooth: false };
}

export function doubleChannel(
  depth: number,
  width: number,
  web: number,
  flange: number,
  gap: number,
  backToBack: boolean,
): Profile {
  return double((mirror) => channelPoints(depth, width, web, flange, mirror), width, gap, backToBack);
}

export function doubleAngle(
  depth: number,
  width: number,
  leg: number,
  flange: number,
  gap: number,
  backToBack: boolean,
): Profile {
  return double((mirror) => anglePoints(depth, width, leg, flange, mirror), width, gap, backToBack);
}

export function doubleLipChannel(
  depth: number,
  width: number,
  lip: number,
  t: number,
  gap: number,
  backToBack: boolean,
): Profile {
  return double((mirror) => lipChannelPoints(depth, width, lip, t, mirror), width, gap, backToBack);
}
