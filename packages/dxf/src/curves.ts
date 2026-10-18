import type { Box } from "@zumenjs/core";
import { arcSegments, type Matrix } from "@zumenjs/core/geometry";
import type { Vec2, Vec3 } from "./model.js";

type V3 = readonly [number, number, number];

function cross(a: V3, b: V3): Vec3 {
  return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
}

function normalize(v: V3): Vec3 {
  const length = Math.hypot(v[0], v[1], v[2]);
  return length > 0 ? [v[0] / length, v[1] / length, v[2] / length] : [0, 0, 1];
}

/**
 * 任意軸アルゴリズム（DXF Reference「About Arbitrary Axis Algorithm」）。押し出し方向から OCS の X・Y・Z 軸を求める。
 * 押し出し方向が WCS の Z なら null（OCS と WCS が同じ）
 */
export function ocsAxes(extrusion: V3 | undefined): [Vec3, Vec3, Vec3] | null {
  if (!extrusion) return null;
  const az = normalize(extrusion);
  if (Math.abs(az[0]) < 1e-12 && Math.abs(az[1]) < 1e-12 && az[2] > 0) return null;
  const ax = normalize(
    Math.abs(az[0]) < 1 / 64 && Math.abs(az[1]) < 1 / 64 ? cross([0, 1, 0], az) : cross([0, 0, 1], az),
  );
  const ay = normalize(cross(az, ax));
  return [ax, ay, az];
}

/** OCS の (x, y) と標高 elevation を、WCS を上から見た (x, y) にする変換 */
export function ocsMatrix(extrusion: V3 | undefined, elevation = 0): Matrix {
  const axes = ocsAxes(extrusion);
  if (!axes) return [1, 0, 0, 1, 0, 0];
  const [ax, ay, az] = axes;
  return [ax[0], ax[1], ay[0], ay[1], az[0] * elevation, az[1] * elevation];
}

/** 円弧を点列にする（角度は度、反時計回り）。始点を含む */
export function arcPoints(
  cx: number,
  cy: number,
  radius: number,
  startDeg: number,
  endDeg: number,
  tolerance: number,
): number[] {
  let sweep = endDeg - startDeg;
  sweep = ((sweep % 360) + 360) % 360;
  if (sweep < 1e-9) sweep = 360;
  const start = (startDeg * Math.PI) / 180;
  const radians = (sweep * Math.PI) / 180;
  const n = arcSegments(radius, radians, tolerance);
  const out: number[] = [];
  for (let i = 0; i <= n; i++) {
    const a = start + (radians * i) / n;
    out.push(cx + radius * Math.cos(a), cy + radius * Math.sin(a));
  }
  return out;
}

/**
 * 膨らみ（bulge = tan(中心角 / 4)）つきの頂点列を点列にする（DXF Reference「LWPOLYLINE」の 42）。
 * 正の膨らみは反時計回りの弧
 */
export function bulgePoints(
  points: readonly (readonly number[])[],
  bulges: readonly number[] | undefined,
  closed: boolean,
  tolerance: (radius: number) => number,
): number[] {
  const n = points.length;
  const out: number[] = [];
  if (n === 0) return out;
  const first = points[0] as readonly number[];
  out.push(first[0] as number, first[1] as number);
  const last = closed ? n : n - 1;
  for (let i = 0; i < last; i++) {
    const a = points[i] as readonly number[];
    const b = points[(i + 1) % n] as readonly number[];
    const x1 = a[0] as number;
    const y1 = a[1] as number;
    const x2 = b[0] as number;
    const y2 = b[1] as number;
    const bulge = bulges?.[i] ?? 0;
    if (Math.abs(bulge) < 1e-12 || (x1 === x2 && y1 === y2)) {
      out.push(x2, y2);
      continue;
    }
    const theta = 4 * Math.atan(bulge);
    const chord = Math.hypot(x2 - x1, y2 - y1);
    const radius = chord / (2 * Math.abs(Math.sin(theta / 2)));
    // 弦の中点から、弦に垂直に中心へ
    const k = (1 - bulge * bulge) / (4 * bulge);
    const cx = (x1 + x2) / 2 - (y2 - y1) * k;
    const cy = (y1 + y2) / 2 + (x2 - x1) * k;
    const a1 = Math.atan2(y1 - cy, x1 - cx);
    const segments = arcSegments(radius, Math.abs(theta), tolerance(radius));
    for (let s = 1; s < segments; s++) {
      const angle = a1 + (theta * s) / segments;
      out.push(cx + radius * Math.cos(angle), cy + radius * Math.sin(angle));
    }
    out.push(x2, y2);
  }
  return out;
}

/** 端を固定した一様なノット列（ノットがない・数が合わないスプライン） */
function clampedKnots(count: number, degree: number): number[] {
  const knots: number[] = [];
  const spans = count - degree;
  for (let i = 0; i < count + degree + 1; i++) {
    if (i <= degree) knots.push(0);
    else if (i >= count) knots.push(spans);
    else knots.push(i - degree);
  }
  return knots;
}

/** NURBS を de Boor の算法で点列にする。control は [x, y, ...] の並び */
export function nurbsPoints(
  degree: number,
  control: readonly (readonly number[])[],
  knotValues: readonly number[],
  weightValues: readonly number[],
  steps: number,
): number[] {
  const n = control.length;
  const p = Math.max(1, Math.min(Math.trunc(degree), n - 1));
  if (n < 2) return control.flatMap((point) => [point[0] as number, point[1] as number]);
  const knots = knotValues.length === n + p + 1 ? knotValues : clampedKnots(n, p);
  const weights = weightValues.length === n ? weightValues : null;
  const lo = knots[p] as number;
  const hi = knots[n] as number;
  if (!(hi > lo)) return control.flatMap((point) => [point[0] as number, point[1] as number]);
  const out: number[] = [];
  const d: number[][] = Array.from({ length: p + 1 }, () => [0, 0, 0]);
  for (let s = 0; s <= steps; s++) {
    const u = s === steps ? hi : lo + ((hi - lo) * s) / steps;
    let k = p;
    while (k < n - 1 && u >= (knots[k + 1] as number)) k++;
    for (let j = 0; j <= p; j++) {
      const point = control[k - p + j] as readonly number[];
      const w = weights ? (weights[k - p + j] as number) || 1 : 1;
      const target = d[j] as number[];
      target[0] = (point[0] as number) * w;
      target[1] = (point[1] as number) * w;
      target[2] = w;
    }
    for (let r = 1; r <= p; r++) {
      for (let j = p; j >= r; j--) {
        const i = k - p + j;
        const left = knots[i] as number;
        const denominator = (knots[i + p - r + 1] as number) - left;
        const alpha = denominator === 0 ? 0 : (u - left) / denominator;
        const a = d[j - 1] as number[];
        const b = d[j] as number[];
        b[0] = (1 - alpha) * (a[0] as number) + alpha * (b[0] as number);
        b[1] = (1 - alpha) * (a[1] as number) + alpha * (b[1] as number);
        b[2] = (1 - alpha) * (a[2] as number) + alpha * (b[2] as number);
      }
    }
    const result = d[p] as number[];
    const w = (result[2] as number) || 1;
    out.push((result[0] as number) / w, (result[1] as number) / w);
  }
  return out;
}

/**
 * 当てはめ点だけのスプラインを、点を通る 3 次の曲線（Catmull-Rom）で近似する。
 * startTangent・endTangent（向き）を渡すと、両端をその向きで出入りさせる。
 * AutoCAD の当てはめの計算（接線・ノットの決め方）とは一致しない
 */
export function throughPoints(
  points: readonly (readonly number[])[],
  stepsPerSpan: number,
  startTangent?: readonly number[],
  endTangent?: readonly number[],
): number[] {
  const n = points.length;
  const out: number[] = [];
  if (n === 0) return out;
  // 端の外に、Catmull-Rom の端の接線がその向き（長さは隣の弦）になる仮の点を置く
  const phantom = (tangent: readonly number[] | undefined, end: number, next: number, sign: number) => {
    const p = points[end] as readonly number[];
    const q = points[next] as readonly number[];
    const length = Math.hypot((tangent?.[0] as number) ?? 0, (tangent?.[1] as number) ?? 0);
    if (!tangent || !(length > 0) || n < 2) return p;
    const chord = Math.hypot((q[0] as number) - (p[0] as number), (q[1] as number) - (p[1] as number));
    const scale = (2 * chord * sign) / length;
    return [(q[0] as number) - (tangent[0] as number) * scale, (q[1] as number) - (tangent[1] as number) * scale];
  };
  const before = phantom(startTangent, 0, 1, 1);
  const after = phantom(endTangent, n - 1, n - 2, -1);
  const at = (i: number) =>
    i < 0 ? before : i >= n ? after : (points[Math.max(0, Math.min(n - 1, i))] as readonly number[]);
  out.push(at(0)[0] as number, at(0)[1] as number);
  for (let i = 0; i < n - 1; i++) {
    const p0 = at(i - 1);
    const p1 = at(i);
    const p2 = at(i + 1);
    const p3 = at(i + 2);
    for (let s = 1; s <= stepsPerSpan; s++) {
      const t = s / stepsPerSpan;
      const t2 = t * t;
      const t3 = t2 * t;
      const f = (c: number) =>
        0.5 *
        (2 * (p1[c] as number) +
          (-(p0[c] as number) + (p2[c] as number)) * t +
          (2 * (p0[c] as number) - 5 * (p1[c] as number) + 4 * (p2[c] as number) - (p3[c] as number)) * t2 +
          (-(p0[c] as number) + 3 * (p1[c] as number) - 3 * (p2[c] as number) + (p3[c] as number)) * t3);
      out.push(f(0), f(1));
    }
  }
  return out;
}

/**
 * 折線（[x, y, ...]）から、切れ目（始点と終点の組）の間を除いた部分ごとの点列にする。
 * 切れ目の点は折線の最も近い位置に寄せる（MULTILEADER の寸法の切断）
 */
export function cutPolyline(points: readonly number[], gaps: readonly (readonly number[])[]): number[][] {
  const n = Math.floor(points.length / 2);
  if (gaps.length === 0 || n < 2) return [points.slice()];
  const x = (i: number) => points[i * 2] as number;
  const y = (i: number) => points[i * 2 + 1] as number;
  const lengths = [0];
  for (let i = 1; i < n; i++) lengths.push((lengths[i - 1] as number) + Math.hypot(x(i) - x(i - 1), y(i) - y(i - 1)));
  const total = lengths[n - 1] as number;
  /** 点に最も近い折線の位置（始点からの長さ） */
  const project = (px: number, py: number): number => {
    let best = Number.POSITIVE_INFINITY;
    let at = 0;
    for (let i = 0; i + 1 < n; i++) {
      const dx = x(i + 1) - x(i);
      const dy = y(i + 1) - y(i);
      const length2 = dx * dx + dy * dy;
      const t = length2 > 0 ? Math.max(0, Math.min(1, ((px - x(i)) * dx + (py - y(i)) * dy) / length2)) : 0;
      const distance = Math.hypot(x(i) + dx * t - px, y(i) + dy * t - py);
      if (distance < best) {
        best = distance;
        at = (lengths[i] as number) + Math.sqrt(length2) * t;
      }
    }
    return at;
  };
  const pointAt = (s: number): [number, number] => {
    let i = 0;
    while (i + 2 < n && (lengths[i + 1] as number) < s) i++;
    const length = (lengths[i + 1] as number) - (lengths[i] as number);
    const t = length > 0 ? (s - (lengths[i] as number)) / length : 0;
    return [x(i) + (x(i + 1) - x(i)) * t, y(i) + (y(i + 1) - y(i)) * t];
  };
  const intervals = gaps
    .map((gap) => {
      const a = project(gap[0] as number, gap[1] as number);
      const b = project(gap[2] as number, gap[3] as number);
      return [Math.min(a, b), Math.max(a, b)] as const;
    })
    .sort((p, q) => p[0] - q[0]);
  const out: number[][] = [];
  const piece = (s0: number, s1: number) => {
    if (!(s1 - s0 > 1e-9)) return;
    const part = [...pointAt(s0)];
    for (let i = 1; i < n - 1; i++)
      if ((lengths[i] as number) > s0 && (lengths[i] as number) < s1) part.push(x(i), y(i));
    part.push(...pointAt(s1));
    out.push(part);
  };
  let cursor = 0;
  for (const [a, b] of intervals) {
    piece(cursor, a);
    cursor = Math.max(cursor, b);
  }
  piece(cursor, total);
  return out;
}

/** 楕円（WCS の中心・長軸・短軸の向き）を、媒介変数 start から end まで点列にする（ラジアン、反時計回り） */
export function ellipsePoints(
  center: V3,
  major: V3,
  minor: V3,
  start: number,
  end: number,
  tolerance: number,
): number[] {
  let sweep = end - start;
  const tau = Math.PI * 2;
  sweep = ((sweep % tau) + tau) % tau;
  if (sweep < 1e-9) sweep = tau;
  const radius = Math.max(Math.hypot(major[0], major[1], major[2]), Math.hypot(minor[0], minor[1], minor[2]));
  const n = arcSegments(radius, sweep, tolerance);
  const out: number[] = [];
  for (let i = 0; i <= n; i++) {
    const t = start + (sweep * i) / n;
    const c = Math.cos(t);
    const s = Math.sin(t);
    out.push(center[0] + major[0] * c + minor[0] * s, center[1] + major[1] * c + minor[1] * s);
  }
  return out;
}

/** 楕円の短軸（押し出し方向 × 長軸、長さは比 × 長軸） */
export function minorAxis(major: V3, ratio: number, extrusion: V3 | undefined): Vec3 {
  const normal = normalize(extrusion ?? [0, 0, 1]);
  const axis = cross(normal, major);
  return [axis[0] * ratio, axis[1] * ratio, axis[2] * ratio];
}

/** 線分を矩形で切る（Liang–Barsky）。外なら null */
function clipSegment(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
  box: Box,
): [number, number, number, number] | null {
  let t0 = 0;
  let t1 = 1;
  const dx = x2 - x1;
  const dy = y2 - y1;
  const checks: [number, number][] = [
    [-dx, x1 - box.minX],
    [dx, box.maxX - x1],
    [-dy, y1 - box.minY],
    [dy, box.maxY - y1],
  ];
  for (const [p, q] of checks) {
    if (p === 0) {
      if (q < 0) return null;
      continue;
    }
    const r = q / p;
    if (p < 0) {
      if (r > t1) return null;
      if (r > t0) t0 = r;
    } else {
      if (r < t0) return null;
      if (r < t1) t1 = r;
    }
  }
  return [x1 + t0 * dx, y1 + t0 * dy, x1 + t1 * dx, y1 + t1 * dy];
}

/** 折線を矩形で切り、内側の部分ごとの点列にする */
export function clipPolyline(points: ArrayLike<number>, closed: boolean, box: Box): Float64Array[] {
  const out: Float64Array[] = [];
  let current: number[] = [];
  const count = Math.floor(points.length / 2);
  const segments = closed && count > 2 ? count : count - 1;
  for (let i = 0; i < segments; i++) {
    const j = (i + 1) % count;
    const clipped = clipSegment(
      points[i * 2] as number,
      points[i * 2 + 1] as number,
      points[j * 2] as number,
      points[j * 2 + 1] as number,
      box,
    );
    if (!clipped) {
      if (current.length >= 4) out.push(Float64Array.from(current));
      current = [];
      continue;
    }
    const [ax, ay, bx, by] = clipped;
    const lastX = current[current.length - 2];
    const lastY = current[current.length - 1];
    if (current.length === 0 || lastX !== ax || lastY !== ay) {
      if (current.length >= 4) out.push(Float64Array.from(current));
      current = [ax, ay];
    }
    current.push(bx, by);
  }
  if (current.length >= 4) out.push(Float64Array.from(current));
  return out;
}

/** 多角形を矩形で切る（Sutherland–Hodgman）。切り抜く側が凸なので、塗りの多角形はどの形でもよい */
export function clipPolygon(ring: ArrayLike<number>, box: Box): Float64Array {
  let input: number[] = Array.from(ring);
  const edges: [(x: number, y: number) => boolean, (ax: number, ay: number, bx: number, by: number) => Vec2][] = [
    [(x) => x >= box.minX, (ax, ay, bx, by) => [box.minX, ay + ((by - ay) * (box.minX - ax)) / (bx - ax)]],
    [(x) => x <= box.maxX, (ax, ay, bx, by) => [box.maxX, ay + ((by - ay) * (box.maxX - ax)) / (bx - ax)]],
    [(_, y) => y >= box.minY, (ax, ay, bx, by) => [ax + ((bx - ax) * (box.minY - ay)) / (by - ay), box.minY]],
    [(_, y) => y <= box.maxY, (ax, ay, bx, by) => [ax + ((bx - ax) * (box.maxY - ay)) / (by - ay), box.maxY]],
  ];
  for (const [inside, intersect] of edges) {
    const output: number[] = [];
    const n = Math.floor(input.length / 2);
    for (let i = 0; i < n; i++) {
      const cx = input[i * 2] as number;
      const cy = input[i * 2 + 1] as number;
      const px = input[((i + n - 1) % n) * 2] as number;
      const py = input[((i + n - 1) % n) * 2 + 1] as number;
      const currentInside = inside(cx, cy);
      if (currentInside) {
        if (!inside(px, py)) output.push(...intersect(px, py, cx, cy));
        output.push(cx, cy);
      } else if (inside(px, py)) {
        output.push(...intersect(px, py, cx, cy));
      }
    }
    input = output;
    if (input.length < 6) return new Float64Array(0);
  }
  return Float64Array.from(input);
}
