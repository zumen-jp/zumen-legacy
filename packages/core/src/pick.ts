import { distanceToSegmentSquared, insideRings } from "./geometry.js";
import type { Box, Primitive, Scene, SceneEntity } from "./types.js";

export interface SnapPoint {
  x: number;
  y: number;
  entity: SceneEntity;
  /** 端点・頂点に吸着したか、線上の最も近い点か */
  kind: "vertex" | "nearest";
}

const GRID = 96;

/** 図形の選択と計測の吸着に使う格子の索引（用紙の mm） */
export class HitIndex {
  private readonly cells = new Map<number, number[]>();
  private readonly large: number[] = [];
  private readonly box: Box;
  private readonly cellWidth: number;
  private readonly cellHeight: number;

  constructor(private readonly scene: Scene) {
    this.box = scene.bounds;
    this.cellWidth = Math.max((this.box.maxX - this.box.minX) / GRID, 1e-9);
    this.cellHeight = Math.max((this.box.maxY - this.box.minY) / GRID, 1e-9);
    scene.entities.forEach((entity, index) => {
      const [c0, r0] = this.cell(entity.bbox.minX, entity.bbox.minY);
      const [c1, r1] = this.cell(entity.bbox.maxX, entity.bbox.maxY);
      if ((c1 - c0 + 1) * (r1 - r0 + 1) > 256) {
        this.large.push(index);
        return;
      }
      for (let c = c0; c <= c1; c++) {
        for (let r = r0; r <= r1; r++) {
          const key = r * (GRID + 1) + c;
          const list = this.cells.get(key);
          if (list) list.push(index);
          else this.cells.set(key, [index]);
        }
      }
    });
  }

  private cell(x: number, y: number): [number, number] {
    const c = Math.min(Math.max(Math.floor((x - this.box.minX) / this.cellWidth), 0), GRID);
    const r = Math.min(Math.max(Math.floor((y - this.box.minY) / this.cellHeight), 0), GRID);
    return [c, r];
  }

  private candidates(x: number, y: number, tolerance: number): number[] {
    const [c0, r0] = this.cell(x - tolerance, y - tolerance);
    const [c1, r1] = this.cell(x + tolerance, y + tolerance);
    const found = new Set<number>(this.large);
    for (let c = c0; c <= c1; c++) {
      for (let r = r0; r <= r1; r++) for (const index of this.cells.get(r * (GRID + 1) + c) ?? []) found.add(index);
    }
    return [...found].sort((a, b) => a - b);
  }

  /** (x, y) に最も近い要素。線への距離を優先し、塗り・文字の内側は少し遠いものとして扱う */
  pick(x: number, y: number, tolerance: number, visible: (entity: SceneEntity) => boolean): SceneEntity | null {
    let best: SceneEntity | null = null;
    let bestDistance = tolerance * tolerance;
    for (const index of this.candidates(x, y, tolerance)) {
      const entity = this.scene.entities[index] as SceneEntity;
      if (!visible(entity) || !near(entity.bbox, x, y, tolerance)) continue;
      const distance = entityDistanceSquared(entity, x, y, tolerance);
      // 後に描いた（上に重なる）要素を、同じ距離なら優先する
      if (distance <= bestDistance) {
        best = entity;
        bestDistance = distance;
      }
    }
    return best;
  }

  /** 計測の吸着先。tolerance 以内の頂点、なければ線上の最も近い点 */
  snap(x: number, y: number, tolerance: number, visible: (entity: SceneEntity) => boolean): SnapPoint | null {
    let vertex: SnapPoint | null = null;
    let vertexDistance = tolerance * tolerance;
    let nearest: SnapPoint | null = null;
    let nearestDistance = tolerance * tolerance;
    for (const index of this.candidates(x, y, tolerance)) {
      const entity = this.scene.entities[index] as SceneEntity;
      if (!visible(entity) || !near(entity.bbox, x, y, tolerance)) continue;
      for (const primitive of entity.primitives) {
        if (primitive.kind !== "stroke" || primitive.color === "transparent") continue;
        const p = primitive.points;
        for (let i = 0; i + 1 < p.length; i += 2) {
          const dx = (p[i] as number) - x;
          const dy = (p[i + 1] as number) - y;
          const d = dx * dx + dy * dy;
          if (d <= vertexDistance) {
            vertexDistance = d;
            vertex = { x: p[i] as number, y: p[i + 1] as number, entity, kind: "vertex" };
          }
        }
        forEachSegment(primitive, (x1, y1, x2, y2) => {
          const d = distanceToSegmentSquared(x, y, x1, y1, x2, y2);
          if (d <= nearestDistance) {
            nearestDistance = d;
            const vx = x2 - x1;
            const vy = y2 - y1;
            const length = vx * vx + vy * vy;
            const t = length > 0 ? Math.max(0, Math.min(1, ((x - x1) * vx + (y - y1) * vy) / length)) : 0;
            nearest = { x: x1 + t * vx, y: y1 + t * vy, entity, kind: "nearest" };
          }
        });
      }
    }
    return vertex ?? nearest;
  }
}

function near(box: Box, x: number, y: number, tolerance: number): boolean {
  return (
    x >= box.minX - tolerance && x <= box.maxX + tolerance && y >= box.minY - tolerance && y <= box.maxY + tolerance
  );
}

function forEachSegment(
  primitive: Extract<Primitive, { kind: "stroke" }>,
  visit: (x1: number, y1: number, x2: number, y2: number) => void,
): void {
  const p = primitive.points;
  if (primitive.segments) {
    for (let i = 0; i + 3 < p.length; i += 4)
      visit(p[i] as number, p[i + 1] as number, p[i + 2] as number, p[i + 3] as number);
    return;
  }
  for (let i = 0; i + 3 < p.length; i += 2)
    visit(p[i] as number, p[i + 1] as number, p[i + 2] as number, p[i + 3] as number);
  if (primitive.closed && p.length >= 6)
    visit(p[p.length - 2] as number, p[p.length - 1] as number, p[0] as number, p[1] as number);
}

function entityDistanceSquared(entity: SceneEntity, x: number, y: number, tolerance: number): number {
  let best = Number.POSITIVE_INFINITY;
  const inside = (tolerance * 0.8) ** 2;
  for (const primitive of entity.primitives) {
    switch (primitive.kind) {
      case "stroke":
        if (primitive.segments) break;
        forEachSegment(primitive, (x1, y1, x2, y2) => {
          best = Math.min(best, distanceToSegmentSquared(x, y, x1, y1, x2, y2));
        });
        break;
      case "fill":
      case "clipped":
        if (insideRings(x, y, primitive.rings)) best = Math.min(best, inside);
        break;
      case "dot":
        best = Math.min(best, (primitive.x - x) ** 2 + (primitive.y - y) ** 2);
        break;
      case "text": {
        const m = primitive.matrix;
        const corners = new Float64Array(8);
        [
          [0, 0],
          [primitive.boxWidth, 0],
          [primitive.boxWidth, primitive.boxHeight],
          [0, primitive.boxHeight],
        ].forEach(([cx, cy], i) => {
          corners[i * 2] = m[0] * (cx as number) + m[2] * (cy as number) + m[4];
          corners[i * 2 + 1] = m[1] * (cx as number) + m[3] * (cy as number) + m[5];
        });
        if (insideRings(x, y, [corners])) best = Math.min(best, inside * 0.5);
        break;
      }
    }
  }
  return best;
}
