import type { Box3, Extrusion, ProfileLoop, Solid, Vec2, Vec3 } from "./types.js";

export const add = (a: Vec3, b: Vec3): Vec3 => [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
export const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
export const scale = (a: Vec3, k: number): Vec3 => [a[0] * k, a[1] * k, a[2] * k];
export const dot = (a: Vec3, b: Vec3): number => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
export const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
export const length = (a: Vec3): number => Math.hypot(a[0], a[1], a[2]);
export const distance = (a: Vec3, b: Vec3): number => length(sub(a, b));

export function normalize(a: Vec3): Vec3 {
  const l = length(a);
  return l > 0 ? [a[0] / l, a[1] / l, a[2] / l] : [0, 0, 0];
}

export const X: Vec3 = [1, 0, 0];
export const Y: Vec3 = [0, 1, 0];
export const Z: Vec3 = [0, 0, 1];

const DEGREE = Math.PI / 180;

/** 断面の平面の座標（b: 幅の方向、h: 成・基準の方向）と、材軸の方向 */
export interface Frame {
  /** 断面の b 方向 */
  e1: Vec3;
  /** 断面の h 方向 */
  e2: Vec3;
  /** 材軸（始端から終端） */
  e3: Vec3;
}

/**
 * 材軸 e3 と「対応断面軸」から部材座標系を作る（仕様書 2.6）。断面軸は材軸と対応断面軸の張る面内にとる。
 * 対応断面軸が材軸と平行なときは代わりの軸を使う。e1 × e2 = e3 の右手系にする。
 */
export function memberFrame(axis: Vec3, reference: Vec3, fallback: Vec3): Frame {
  const e3 = normalize(axis);
  let along = sub(reference, scale(e3, dot(reference, e3)));
  if (length(along) < 1e-9) along = sub(fallback, scale(e3, dot(fallback, e3)));
  const e2 = normalize(along);
  return { e1: cross(e2, e3), e2, e3 };
}

/** 材軸まわりに断面を回す（右ねじ、度）。始端から終端を見て時計回りが正（仕様書 5.2.1 補足(3)） */
export function rotateFrame(frame: Frame, degrees: number): Frame {
  if (!degrees) return frame;
  const c = Math.cos(degrees * DEGREE);
  const s = Math.sin(degrees * DEGREE);
  return {
    e1: add(scale(frame.e1, c), scale(frame.e2, s)),
    e2: add(scale(frame.e1, -s), scale(frame.e2, c)),
    e3: frame.e3,
  };
}

export function place(origin: Vec3, frame: Frame, b: number, h: number): Vec3 {
  return [
    origin[0] + frame.e1[0] * b + frame.e2[0] * h,
    origin[1] + frame.e1[1] * b + frame.e2[1] * h,
    origin[2] + frame.e1[2] * b + frame.e2[2] * h,
  ];
}

/** 断面の 2D の輪郭。外周・穴の向きは問わない（描画側で揃える） */
export type ProfilePart = ProfileLoop;

export interface Profile {
  parts: ProfilePart[];
  /** 円形の断面（側面を滑らかに陰影づけする） */
  smooth: boolean;
}

export function translateProfile(profile: Profile, db: number, dh: number): Profile {
  if (!db && !dh) return profile;
  const move = (points: Vec2[]) => points.map(([b, h]) => [b + db, h + dh] as const);
  return {
    smooth: profile.smooth,
    parts: profile.parts.map((part) => ({ outer: move(part.outer), holes: part.holes.map(move) })),
  };
}

/** 断面を 90 度回す（(b, h) → (−h, b)） */
export function rotateProfile90(profile: Profile): Profile {
  const turn = (points: Vec2[]) => points.map(([b, h]) => [-h, b] as const);
  return {
    smooth: profile.smooth,
    parts: profile.parts.map((part) => ({ outer: turn(part.outer), holes: part.holes.map(turn) })),
  };
}

export function profileBounds(profile: Profile): { minB: number; maxB: number; minH: number; maxH: number } {
  let minB = Number.POSITIVE_INFINITY;
  let maxB = Number.NEGATIVE_INFINITY;
  let minH = Number.POSITIVE_INFINITY;
  let maxH = Number.NEGATIVE_INFINITY;
  for (const part of profile.parts) {
    for (const [b, h] of part.outer) {
      minB = Math.min(minB, b);
      maxB = Math.max(maxB, b);
      minH = Math.min(minH, h);
      maxH = Math.max(maxH, h);
    }
  }
  return { minB, maxB, minH, maxH };
}

/** 頂点数（穴を含む）が同じ輪郭か。変断面で両端をつなげるかの判定に使う */
export function sameTopology(a: Profile, b: Profile): boolean {
  if (a.parts.length !== b.parts.length) return false;
  return a.parts.every((part, i) => {
    const other = b.parts[i];
    if (!other || other.outer.length !== part.outer.length || other.holes.length !== part.holes.length) return false;
    return part.holes.every((hole, k) => other.holes[k]?.length === hole.length);
  });
}

/**
 * 材軸に沿って断面を押し出す。始端の断面 start と終端の断面 end は頂点の数を揃える。
 * 断面ごと（2 丁合わせの形鋼は 2 つ）に立体を作る。輪郭は複製せず共有する。
 */
export function extrude(
  startOrigin: Vec3,
  endOrigin: Vec3,
  frame: Frame,
  start: Profile,
  end: Profile = start,
): Extrusion[] {
  return start.parts.map((part, index) => ({
    type: "extrusion",
    start: startOrigin,
    end: endOrigin,
    e1: frame.e1,
    e2: frame.e2,
    startLoop: part,
    endLoop: end.parts[index] ?? part,
    smooth: start.smooth,
  }));
}

/** 押し出しの輪郭を 3D の点にする（描画・外接箱に使う） */
export function extrusionLoop(solid: Extrusion, atEnd: boolean): { outer: Vec3[]; holes: Vec3[][] } {
  const origin = atEnd ? solid.end : solid.start;
  const loop = atEnd ? solid.endLoop : solid.startLoop;
  const frame = { e1: solid.e1, e2: solid.e2, e3: [0, 0, 0] as Vec3 };
  return {
    outer: loop.outer.map(([b, h]) => place(origin, frame, b, h)),
    holes: loop.holes.map((hole) => hole.map(([b, h]) => place(origin, frame, b, h))),
  };
}

/** 立体の外接箱を広げる。押し出しは輪郭の外周の点だけを使う */
export function expandBoxBySolid(box: { min: number[]; max: number[] }, solid: Solid): void {
  if (solid.type === "loft") {
    for (const section of solid.sections) for (const p of section.outer) expandBox(box, p);
    return;
  }
  for (const atEnd of [false, true]) for (const p of extrusionLoop(solid, atEnd).outer) expandBox(box, p);
}

/** 多角形の符号付き面積と図心（XY） */
export function polygonCentroid(points: Vec2[]): { area: number; centroid: Vec2 } {
  let area = 0;
  let cx = 0;
  let cy = 0;
  for (let i = 0; i < points.length; i++) {
    const [x0, y0] = points[i] as Vec2;
    const [x1, y1] = points[(i + 1) % points.length] as Vec2;
    const cross2 = x0 * y1 - x1 * y0;
    area += cross2;
    cx += (x0 + x1) * cross2;
    cy += (y0 + y1) * cross2;
  }
  area /= 2;
  if (Math.abs(area) < 1e-12) {
    const n = Math.max(points.length, 1);
    return { area: 0, centroid: [points.reduce((s, p) => s + p[0], 0) / n, points.reduce((s, p) => s + p[1], 0) / n] };
  }
  return { area, centroid: [cx / (6 * area), cy / (6 * area)] };
}

/** 多角形の法線（Newell の方法）。面積が 0 なら [0, 0, 0] */
export function polygonNormal(points: readonly Vec3[]): Vec3 {
  let nx = 0;
  let ny = 0;
  let nz = 0;
  for (let i = 0; i < points.length; i++) {
    const a = points[i] as Vec3;
    const b = points[(i + 1) % points.length] as Vec3;
    nx += (a[1] - b[1]) * (a[2] + b[2]);
    ny += (a[2] - b[2]) * (a[0] + b[0]);
    nz += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return normalize([nx, ny, nz]);
}

export function emptyBox(): { min: [number, number, number]; max: [number, number, number] } {
  return {
    min: [Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY],
    max: [Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.NEGATIVE_INFINITY],
  };
}

export function expandBox(box: { min: number[]; max: number[] }, p: Vec3): void {
  for (let k = 0; k < 3; k++) {
    const value = p[k] as number;
    if (value < (box.min[k] as number)) box.min[k] = value;
    if (value > (box.max[k] as number)) box.max[k] = value;
  }
}

export function isEmptyBox(box: Box3): boolean {
  return !(box.min[0] <= box.max[0]);
}

/** 円を n 角形にする（中心 0、反時計回り） */
export function circle(radius: number, segments = 24): Vec2[] {
  const points: Vec2[] = [];
  for (let i = 0; i < segments; i++) {
    const a = (i / segments) * Math.PI * 2;
    points.push([radius * Math.cos(a), radius * Math.sin(a)]);
  }
  return points;
}
