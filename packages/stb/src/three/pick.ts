/** 描画用の座標（モデルの中心を引いた値）での、部材の三角形・線分の範囲と外接箱 */
export interface PickTarget {
  index: number;
  /** 三角形の頂点の座標（xyz の並び）と、三角形の頂点番号 */
  positions: Float32Array | null;
  indices: Uint32Array | null;
  indexStart: number;
  indexCount: number;
  /** 線だけの部材の線分（xyz の両端の並び） */
  lines: Float32Array | null;
  lineStart: number;
  lineCount: number;
  min: [number, number, number];
  max: [number, number, number];
}

export interface Ray {
  origin: [number, number, number];
  direction: [number, number, number];
}

/** 光線と箱の交わる最小の距離。交わらなければ Infinity */
export function rayBox(ray: Ray, min: readonly number[], max: readonly number[], pad = 0): number {
  let t0 = 0;
  let t1 = Number.POSITIVE_INFINITY;
  for (let k = 0; k < 3; k++) {
    const o = ray.origin[k] as number;
    const d = ray.direction[k] as number;
    const lo = (min[k] as number) - pad;
    const hi = (max[k] as number) + pad;
    if (Math.abs(d) < 1e-12) {
      if (o < lo || o > hi) return Number.POSITIVE_INFINITY;
      continue;
    }
    let a = (lo - o) / d;
    let b = (hi - o) / d;
    if (a > b) [a, b] = [b, a];
    t0 = Math.max(t0, a);
    t1 = Math.min(t1, b);
    if (t0 > t1) return Number.POSITIVE_INFINITY;
  }
  return t0;
}

/** Möller–Trumbore の交差判定。交われば光線上の距離、なければ Infinity */
function rayTriangle(ray: Ray, p: Float32Array, a: number, b: number, c: number): number {
  const ax = p[a * 3] as number;
  const ay = p[a * 3 + 1] as number;
  const az = p[a * 3 + 2] as number;
  const e1x = (p[b * 3] as number) - ax;
  const e1y = (p[b * 3 + 1] as number) - ay;
  const e1z = (p[b * 3 + 2] as number) - az;
  const e2x = (p[c * 3] as number) - ax;
  const e2y = (p[c * 3 + 1] as number) - ay;
  const e2z = (p[c * 3 + 2] as number) - az;
  const [dx, dy, dz] = ray.direction;
  const px = dy * e2z - dz * e2y;
  const py = dz * e2x - dx * e2z;
  const pz = dx * e2y - dy * e2x;
  const det = e1x * px + e1y * py + e1z * pz;
  if (Math.abs(det) < 1e-12) return Number.POSITIVE_INFINITY;
  const inv = 1 / det;
  const tx = ray.origin[0] - ax;
  const ty = ray.origin[1] - ay;
  const tz = ray.origin[2] - az;
  const u = (tx * px + ty * py + tz * pz) * inv;
  if (u < 0 || u > 1) return Number.POSITIVE_INFINITY;
  const qx = ty * e1z - tz * e1y;
  const qy = tz * e1x - tx * e1z;
  const qz = tx * e1y - ty * e1x;
  const v = (dx * qx + dy * qy + dz * qz) * inv;
  if (v < 0 || u + v > 1) return Number.POSITIVE_INFINITY;
  const t = (e2x * qx + e2y * qy + e2z * qz) * inv;
  return t > 0 ? t : Number.POSITIVE_INFINITY;
}

/** 光線と線分の最短距離と、そのときの光線上の距離 */
function rayLine(ray: Ray, l: Float32Array, i: number): { distance: number; t: number } {
  const ax = l[i] as number;
  const ay = l[i + 1] as number;
  const az = l[i + 2] as number;
  const ux = (l[i + 3] as number) - ax;
  const uy = (l[i + 4] as number) - ay;
  const uz = (l[i + 5] as number) - az;
  const [dx, dy, dz] = ray.direction;
  const wx = ray.origin[0] - ax;
  const wy = ray.origin[1] - ay;
  const wz = ray.origin[2] - az;
  const a = dx * dx + dy * dy + dz * dz;
  const b = dx * ux + dy * uy + dz * uz;
  const c = ux * ux + uy * uy + uz * uz;
  const d = dx * wx + dy * wy + dz * wz;
  const e = ux * wx + uy * wy + uz * wz;
  // 光線 O + sD と線分 A + rU の最近接（r を線分の範囲に収めてから s を求める）
  const denominator = a * c - b * b;
  const r = Math.min(1, Math.max(0, denominator > 1e-12 ? (a * e - b * d) / denominator : c > 0 ? e / c : 0));
  const s = Math.max(0, (b * r - d) / a);
  const cx = ray.origin[0] + dx * s - (ax + ux * r);
  const cy = ray.origin[1] + dy * s - (ay + uy * r);
  const cz = ray.origin[2] + dz * s - (az + uz * r);
  return { distance: Math.hypot(cx, cy, cz), t: s };
}

/**
 * 光線に最も近い部材を返す。外接箱で候補を絞ってから三角形を調べる。
 * 線だけの部材は、光線からの距離が tolerance(t)（光線上の距離 t での許容距離）以内なら選ぶ。
 */
export function pickTarget(
  ray: Ray,
  targets: readonly PickTarget[],
  tolerance: (t: number) => number,
): { index: number; t: number } | null {
  const candidates: { target: PickTarget; t: number }[] = [];
  for (const target of targets) {
    const pad = target.lines ? tolerance(1e6) : 0;
    const t = rayBox(ray, target.min, target.max, pad);
    if (Number.isFinite(t)) candidates.push({ target, t });
  }
  candidates.sort((a, b) => a.t - b.t);
  let best: { index: number; t: number } | null = null;
  for (const { target, t: entry } of candidates) {
    if (best && entry > best.t) break;
    if (target.positions && target.indices) {
      const end = target.indexStart + target.indexCount;
      for (let i = target.indexStart; i < end; i += 3) {
        const t = rayTriangle(
          ray,
          target.positions,
          target.indices[i] as number,
          target.indices[i + 1] as number,
          target.indices[i + 2] as number,
        );
        if (t < (best?.t ?? Number.POSITIVE_INFINITY)) best = { index: target.index, t };
      }
    }
    if (target.lines) {
      const end = (target.lineStart + target.lineCount) * 3;
      for (let i = target.lineStart * 3; i < end; i += 6) {
        const hit = rayLine(ray, target.lines, i);
        if (hit.distance <= tolerance(hit.t) && hit.t < (best?.t ?? Number.POSITIVE_INFINITY))
          best = { index: target.index, t: hit.t };
      }
    }
  }
  return best;
}
