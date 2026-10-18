import type { Box } from "./types.js";

/** 2 次元のアフィン変換 [a, b, c, d, e, f]。x' = a x + c y + e、y' = b x + d y + f */
export type Matrix = readonly [number, number, number, number, number, number];

export const IDENTITY: Matrix = [1, 0, 0, 1, 0, 0];

/** m · n（n を先に適用する） */
export function multiply(m: Matrix, n: Matrix): Matrix {
  return [
    m[0] * n[0] + m[2] * n[1],
    m[1] * n[0] + m[3] * n[1],
    m[0] * n[2] + m[2] * n[3],
    m[1] * n[2] + m[3] * n[3],
    m[0] * n[4] + m[2] * n[5] + m[4],
    m[1] * n[4] + m[3] * n[5] + m[5],
  ];
}

export function invert(m: Matrix): Matrix {
  const det = m[0] * m[3] - m[1] * m[2];
  if (!Number.isFinite(det) || Math.abs(det) < 1e-300) return IDENTITY;
  const a = m[3] / det;
  const b = -m[1] / det;
  const c = -m[2] / det;
  const d = m[0] / det;
  return [a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])];
}

/** 平行移動・回転（度、反時計回り）・拡大の順に合成した配置 */
export function placement(x: number, y: number, angleDeg: number, scaleX: number, scaleY: number): Matrix {
  const r = (angleDeg * Math.PI) / 180;
  const cos = Math.cos(r);
  const sin = Math.sin(r);
  return [cos * scaleX, sin * scaleX, -sin * scaleY, cos * scaleY, x, y];
}

export function apply(m: Matrix, x: number, y: number): [number, number] {
  return [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
}

/** 変換の向き（方向ベクトルだけを変換する） */
export function applyVector(m: Matrix, x: number, y: number): [number, number] {
  return [m[0] * x + m[2] * y, m[1] * x + m[3] * y];
}

/** 変換が平均して長さを何倍にするか */
export function meanScale(m: Matrix): number {
  return Math.sqrt(Math.abs(m[0] * m[3] - m[1] * m[2]));
}

/** 変換の回転角（度）。X 軸の向きから求める */
export function rotationDeg(m: Matrix): number {
  return (Math.atan2(m[1], m[0]) * 180) / Math.PI;
}

/** 変換が向きを反転するか（測地座標系の部分図など） */
export function isMirrored(m: Matrix): boolean {
  return m[0] * m[3] - m[1] * m[2] < 0;
}

export function emptyBox(): Box {
  return { minX: Number.POSITIVE_INFINITY, minY: Number.POSITIVE_INFINITY, maxX: -Infinity, maxY: -Infinity };
}

export function isEmptyBox(box: Box): boolean {
  return !(box.minX <= box.maxX && box.minY <= box.maxY);
}

export function extendBox(box: Box, x: number, y: number): void {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return;
  if (x < box.minX) box.minX = x;
  if (y < box.minY) box.minY = y;
  if (x > box.maxX) box.maxX = x;
  if (y > box.maxY) box.maxY = y;
}

export function extendBoxWithPoints(box: Box, points: ArrayLike<number>): void {
  for (let i = 0; i + 1 < points.length; i += 2) extendBox(box, points[i] as number, points[i + 1] as number);
}

export function unionBox(box: Box, other: Box): void {
  if (isEmptyBox(other)) return;
  extendBox(box, other.minX, other.minY);
  extendBox(box, other.maxX, other.maxY);
}

/** 変換後の点列を Float64Array で返す */
export function transformPoints(m: Matrix, points: ArrayLike<number>): Float64Array {
  const out = new Float64Array(points.length);
  for (let i = 0; i + 1 < points.length; i += 2) {
    const x = points[i] as number;
    const y = points[i + 1] as number;
    out[i] = m[0] * x + m[2] * y + m[4];
    out[i + 1] = m[1] * x + m[3] * y + m[5];
  }
  return out;
}

/** 弦と弧のずれが tolerance 以下になる分割数。radius と tolerance は同じ座標（用紙の mm）で渡す */
export function arcSegments(radius: number, sweepRadians: number, tolerance: number): number {
  const r = Math.abs(radius);
  const sweep = Math.abs(sweepRadians);
  if (!(r > 0) || !(sweep > 0)) return 1;
  const step = r <= tolerance ? Math.PI / 2 : 2 * Math.acos(Math.max(-1, 1 - tolerance / r));
  const n = Math.ceil(sweep / Math.max(step, 1e-4));
  return Math.min(Math.max(n, Math.ceil((sweep / (2 * Math.PI)) * 16), 1), 4096);
}

/**
 * 楕円弧を点列にする（元の座標）。角度は度、ccw が false なら時計回り。
 * 楕円の角度は媒介変数の角度（x = rx cos t、y = ry sin t を rotation だけ回す）。
 */
export function ellipseArcPoints(
  cx: number,
  cy: number,
  rx: number,
  ry: number,
  rotationDeg: number,
  startDeg: number,
  endDeg: number,
  ccw: boolean,
  segments: number,
): number[] {
  const rot = (rotationDeg * Math.PI) / 180;
  const cosR = Math.cos(rot);
  const sinR = Math.sin(rot);
  const start = (startDeg * Math.PI) / 180;
  const sweep = sweepRadians(startDeg, endDeg, ccw);
  const out: number[] = [];
  for (let i = 0; i <= segments; i++) {
    const t = start + (sweep * i) / segments;
    const x = rx * Math.cos(t);
    const y = ry * Math.sin(t);
    out.push(cx + x * cosR - y * sinR, cy + x * sinR + y * cosR);
  }
  return out;
}

/** 始角から終角まで、指定の向きに回る角度（ラジアン、符号付き）。同じ角度は全周とみなす */
export function sweepRadians(startDeg: number, endDeg: number, ccw: boolean): number {
  const raw = ccw ? endDeg - startDeg : startDeg - endDeg;
  let d = ((raw % 360) + 360) % 360;
  if (d < 1e-9) d = 360;
  const sweep = (d * Math.PI) / 180;
  return ccw ? sweep : -sweep;
}

/** 3 次ベジェ曲線（制御点 3n+1 個）を点列にする。余りの点は折線としてつなぐ */
export function bezierPoints(points: ArrayLike<number>, stepsPerSegment: number): number[] {
  const count = Math.floor(points.length / 2);
  const out: number[] = [];
  if (count === 0) return out;
  out.push(points[0] as number, points[1] as number);
  let i = 0;
  for (; i + 3 < count; i += 3) {
    const x0 = points[i * 2] as number;
    const y0 = points[i * 2 + 1] as number;
    const x1 = points[i * 2 + 2] as number;
    const y1 = points[i * 2 + 3] as number;
    const x2 = points[i * 2 + 4] as number;
    const y2 = points[i * 2 + 5] as number;
    const x3 = points[i * 2 + 6] as number;
    const y3 = points[i * 2 + 7] as number;
    for (let s = 1; s <= stepsPerSegment; s++) {
      const t = s / stepsPerSegment;
      const u = 1 - t;
      const b0 = u * u * u;
      const b1 = 3 * t * u * u;
      const b2 = 3 * t * t * u;
      const b3 = t * t * t;
      out.push(b0 * x0 + b1 * x1 + b2 * x2 + b3 * x3, b0 * y0 + b1 * y1 + b2 * y2 + b3 * y3);
    }
  }
  for (i += 1; i < count; i++) out.push(points[i * 2] as number, points[i * 2 + 1] as number);
  return out;
}

/**
 * クロソイド（曲率が曲線長に比例する曲線）を点列にする。
 * 基点で接線の向き angleDeg、パラメータ a（R・L = A²）、基点からの曲線長 s0 から s1 まで。
 * ccw なら接線の左側へ曲がる。
 */
export function clothoidPoints(
  x: number,
  y: number,
  a: number,
  ccw: boolean,
  angleDeg: number,
  s0: number,
  s1: number,
  segments: number,
): number[] {
  const theta = (angleDeg * Math.PI) / 180;
  const cos = Math.cos(theta);
  const sin = Math.sin(theta);
  const side = ccw ? 1 : -1;
  const a2 = 2 * a * a;
  // 接線角 s²/(2A²) をシンプソン則で積分して、基点からの局所座標を求める
  const local = (s: number): [number, number] => {
    const n = Math.max(8, Math.ceil(Math.abs(s) / Math.max(a, 1e-9)) * 16);
    const h = s / n;
    let sx = 0;
    let sy = 0;
    for (let k = 0; k <= n; k++) {
      const t = k * h;
      const w = k === 0 || k === n ? 1 : k % 2 === 1 ? 4 : 2;
      const phi = (t * t) / a2;
      sx += w * Math.cos(phi);
      sy += w * Math.sin(phi);
    }
    return [(sx * h) / 3, (side * sy * h) / 3];
  };
  const out: number[] = [];
  for (let i = 0; i <= segments; i++) {
    const s = s0 + ((s1 - s0) * i) / segments;
    const [lx, ly] = local(s);
    out.push(x + lx * cos - ly * sin, y + lx * sin + ly * cos);
  }
  return out;
}

/** 点 (px, py) から線分までの距離の 2 乗 */
export function distanceToSegmentSquared(
  px: number,
  py: number,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): number {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const lengthSquared = dx * dx + dy * dy;
  let t = lengthSquared > 0 ? ((px - x1) * dx + (py - y1) * dy) / lengthSquared : 0;
  t = Math.max(0, Math.min(1, t));
  const qx = x1 + t * dx - px;
  const qy = y1 + t * dy - py;
  return qx * qx + qy * qy;
}

/** 偶奇規則で点が多角形の内側にあるか */
export function insideRings(px: number, py: number, rings: readonly Float64Array[]): boolean {
  let inside = false;
  for (const ring of rings) {
    const n = Math.floor(ring.length / 2);
    for (let i = 0, j = n - 1; i < n; j = i++) {
      const xi = ring[i * 2] as number;
      const yi = ring[i * 2 + 1] as number;
      const xj = ring[j * 2] as number;
      const yj = ring[j * 2 + 1] as number;
      if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) inside = !inside;
    }
  }
  return inside;
}
