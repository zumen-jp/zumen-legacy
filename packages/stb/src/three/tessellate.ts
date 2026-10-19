import { ShapeUtils, Vector2 } from "three";
import { extrusionLoop } from "../geometry.js";
import type { Face, Solid, SolidSection, Vec3 } from "../types.js";

/** 伸びる Float32Array */
export class FloatList {
  data: Float32Array;
  length = 0;

  constructor(capacity = 1024) {
    this.data = new Float32Array(capacity);
  }

  push3(x: number, y: number, z: number): void {
    if (this.length + 3 > this.data.length) this.grow(this.length + 3);
    this.data[this.length++] = x;
    this.data[this.length++] = y;
    this.data[this.length++] = z;
  }

  private grow(min: number): void {
    const next = new Float32Array(Math.max(min, this.data.length * 2));
    next.set(this.data.subarray(0, this.length));
    this.data = next;
  }

  toArray(): Float32Array {
    return this.data.slice(0, this.length);
  }
}

export class IndexList {
  data: Uint32Array;
  length = 0;

  constructor(capacity = 1024) {
    this.data = new Uint32Array(capacity);
  }

  push3(a: number, b: number, c: number): void {
    if (this.length + 3 > this.data.length) {
      const next = new Uint32Array(Math.max(this.length + 3, this.data.length * 2));
      next.set(this.data.subarray(0, this.length));
      this.data = next;
    }
    this.data[this.length++] = a;
    this.data[this.length++] = b;
    this.data[this.length++] = c;
  }

  toArray(): Uint32Array {
    return this.data.slice(0, this.length);
  }
}

/** 三角形と線分を書き込む先。座標は origin を引いた値で持つ（大きな座標で float32 の精度を落とさない） */
export interface MeshSink {
  positions: FloatList;
  normals: FloatList;
  indices: IndexList;
  /** 稜線（線分の両端の座標） */
  edges: FloatList;
  origin: Vec3;
}

export function createSink(origin: Vec3): MeshSink {
  return {
    positions: new FloatList(),
    normals: new FloatList(),
    indices: new IndexList(),
    edges: new FloatList(),
    origin,
  };
}

type V = [number, number, number];
const sub = (a: Vec3, b: Vec3): V => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): V => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const norm = (a: Vec3): V => {
  const l = Math.hypot(a[0], a[1], a[2]);
  return l > 0 ? [a[0] / l, a[1] / l, a[2] / l] : [0, 0, 0];
};

function newell(points: readonly Vec3[]): V {
  let x = 0;
  let y = 0;
  let z = 0;
  for (let i = 0; i < points.length; i++) {
    const a = points[i] as Vec3;
    const b = points[(i + 1) % points.length] as Vec3;
    x += (a[1] - b[1]) * (a[2] + b[2]);
    y += (a[2] - b[2]) * (a[0] + b[0]);
    z += (a[0] - b[0]) * (a[1] + b[1]);
  }
  return [x, y, z];
}

function centroid(points: readonly Vec3[]): V {
  const c: V = [0, 0, 0];
  for (const p of points) {
    c[0] += p[0];
    c[1] += p[1];
    c[2] += p[2];
  }
  const n = Math.max(points.length, 1);
  return [c[0] / n, c[1] / n, c[2] / n];
}

function vertex(sink: MeshSink, p: Vec3, n: Vec3): number {
  const index = sink.positions.length / 3;
  sink.positions.push3(p[0] - sink.origin[0], p[1] - sink.origin[1], p[2] - sink.origin[2]);
  sink.normals.push3(n[0], n[1], n[2]);
  return index;
}

function edge(sink: MeshSink, a: Vec3, b: Vec3): void {
  const o = sink.origin;
  sink.edges.push3(a[0] - o[0], a[1] - o[1], a[2] - o[2]);
  sink.edges.push3(b[0] - o[0], b[1] - o[1], b[2] - o[2]);
}

function ringEdges(sink: MeshSink, ring: readonly Vec3[]): void {
  for (let i = 0; i < ring.length; i++) edge(sink, ring[i] as Vec3, ring[(i + 1) % ring.length] as Vec3);
}

/** 蓋（穴あきの多角形）を三角形にする。normal の向きを表にする */
function cap(sink: MeshSink, section: SolidSection, normal: Vec3): void {
  const all = [section.outer, ...section.holes];
  const n = norm(normal);
  if (n[0] === 0 && n[1] === 0 && n[2] === 0) return;
  const helper: Vec3 = Math.abs(n[2]) < 0.9 ? [0, 0, 1] : [1, 0, 0];
  const u = norm(cross(helper, n));
  const v = cross(n, u);
  const to2 = (p: Vec3) => new Vector2(dot(p, u), dot(p, v));
  const contour = section.outer.map(to2);
  const holes = section.holes.map((hole) => hole.map(to2));
  let triangles: number[][];
  try {
    triangles = ShapeUtils.triangulateShape(contour, holes);
  } catch {
    return;
  }
  const flat = all.flat();
  const base = flat.map((p) => vertex(sink, p, n));
  for (const [i, j, k] of triangles as [number, number, number][]) {
    const a = flat[i] as Vec3;
    const b = flat[j] as Vec3;
    const c = flat[k] as Vec3;
    const facing = dot(cross(sub(b, a), sub(c, a)), n);
    if (facing >= 0) sink.indices.push3(base[i] as number, base[j] as number, base[k] as number);
    else sink.indices.push3(base[i] as number, base[k] as number, base[j] as number);
  }
}

/** 2 つの輪郭の間の側面。外向きは輪郭の向きと押し出しの向きから決める（凹んだ断面でも正しく向く） */
function sides(
  sink: MeshSink,
  a: readonly Vec3[],
  b: readonly Vec3[],
  axis: Vec3,
  hole: boolean,
  smooth: boolean,
  longitudinal: boolean,
): void {
  const m = Math.min(a.length, b.length);
  if (m < 2) return;
  const ccw = dot(newell(a), axis) > 0;
  // 反時計回りの外周では「辺 × 軸」が外向き。穴は内側（穴の中心）が外向き
  const sign = (ccw ? 1 : -1) * (hole ? -1 : 1);
  if (smooth) {
    const center = centroid(a);
    const normals: V[] = [];
    for (let j = 0; j < m; j++) {
      const p = a[j] as Vec3;
      const radial = sub(p, center);
      const along = dot(radial, norm(axis));
      const n = norm([radial[0] - along * axis[0], radial[1] - along * axis[1], radial[2] - along * axis[2]]);
      normals.push(hole ? [-n[0], -n[1], -n[2]] : n);
    }
    const startA = sink.positions.length / 3;
    for (let j = 0; j < m; j++) vertex(sink, a[j] as Vec3, normals[j] as Vec3);
    const startB = sink.positions.length / 3;
    for (let j = 0; j < m; j++) vertex(sink, b[j] as Vec3, normals[j] as Vec3);
    for (let j = 0; j < m; j++) {
      const k = (j + 1) % m;
      const p0 = a[j] as Vec3;
      const p1 = a[k] as Vec3;
      const q0 = b[j] as Vec3;
      const out = sign > 0 ? cross(sub(p1, p0), axis) : cross(axis, sub(p1, p0));
      const facing = dot(cross(sub(p1, p0), sub(q0, p0)), out);
      if (facing >= 0) {
        sink.indices.push3(startA + j, startA + k, startB + k);
        sink.indices.push3(startA + j, startB + k, startB + j);
      } else {
        sink.indices.push3(startA + j, startB + k, startA + k);
        sink.indices.push3(startA + j, startB + j, startB + k);
      }
    }
    return;
  }
  for (let j = 0; j < m; j++) {
    const k = (j + 1) % m;
    const p0 = a[j] as Vec3;
    const p1 = a[k] as Vec3;
    const q1 = b[k] as Vec3;
    const q0 = b[j] as Vec3;
    const e = sub(p1, p0);
    const out = sign > 0 ? cross(e, axis) : cross(axis, e);
    let n = norm(cross(sub(p1, p0), sub(q0, p0)));
    if (n[0] === 0 && n[1] === 0 && n[2] === 0) n = norm(cross(sub(q1, q0), sub(q0, p0)));
    const flip = dot(n, out) < 0;
    const normal: V = flip ? [-n[0], -n[1], -n[2]] : n;
    const i0 = vertex(sink, p0, normal);
    const i1 = vertex(sink, p1, normal);
    const i2 = vertex(sink, q1, normal);
    const i3 = vertex(sink, q0, normal);
    if (!flip) {
      sink.indices.push3(i0, i1, i2);
      sink.indices.push3(i0, i2, i3);
    } else {
      sink.indices.push3(i0, i2, i1);
      sink.indices.push3(i0, i3, i2);
    }
    if (longitudinal) edge(sink, p0, q0);
  }
}

/** 立体を三角形と稜線にする。押し出しは両端の輪郭を 3D の点にしてからつなぐ */
export function addSolid(sink: MeshSink, solid: Solid): void {
  const sections = solid.type === "loft" ? solid.sections : [extrusionLoop(solid, false), extrusionLoop(solid, true)];
  if (sections.length < 2) return;
  for (let i = 0; i + 1 < sections.length; i++) {
    const a = sections[i] as SolidSection;
    const b = sections[i + 1] as SolidSection;
    const axis = norm(sub(centroid(b.outer), centroid(a.outer)));
    const direction: Vec3 = axis[0] === 0 && axis[1] === 0 && axis[2] === 0 ? norm(newell(a.outer)) : axis;
    sides(sink, a.outer, b.outer, direction, false, solid.smooth, !solid.smooth);
    a.holes.forEach((hole, k) => {
      sides(sink, hole, b.holes[k] ?? hole, direction, true, solid.smooth, !solid.smooth);
    });
  }
  const first = sections[0] as SolidSection;
  const last = sections[sections.length - 1] as SolidSection;
  const forward = norm(sub(centroid((sections[1] as SolidSection).outer), centroid(first.outer)));
  const backward = norm(sub(centroid(last.outer), centroid((sections[sections.length - 2] as SolidSection).outer)));
  cap(sink, first, [-forward[0], -forward[1], -forward[2]]);
  cap(sink, last, backward);
  for (const section of sections) {
    ringEdges(sink, section.outer);
    for (const hole of section.holes) ringEdges(sink, hole);
  }
}

/** 厚さのない面（両面から見える材料で描く） */
export function addFace(sink: MeshSink, face: Face): void {
  const normal = norm(newell(face.outer));
  cap(sink, { outer: face.outer, holes: face.holes }, normal);
  ringEdges(sink, face.outer);
  for (const hole of face.holes) ringEdges(sink, hole);
}

export function addLine(sink: MeshSink, line: readonly Vec3[]): void {
  for (let i = 0; i + 1 < line.length; i++) edge(sink, line[i] as Vec3, line[i + 1] as Vec3);
}
