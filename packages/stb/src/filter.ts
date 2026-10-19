import { type GridAxis, isArcAxis, type StructureMember, type StructureModel, type Vec3 } from "./types.js";

/** 通り芯の上にあるとみなす距離（mm）。ST-Bridge の節点は軸の代表距離と一致しなくてよい（仕様書 4.2.1 補足(4)）ので、軸の節点の一覧も使う */
export const AXIS_TOLERANCE = 5;

function distanceToAxis(axis: GridAxis, p: Vec3): number {
  if (isArcAxis(axis)) {
    return Math.abs(Math.hypot(p[0] - axis.center[0], p[1] - axis.center[1]) - axis.radius);
  }
  const dx = axis.end[0] - axis.start[0];
  const dy = axis.end[1] - axis.start[1];
  const length = Math.hypot(dx, dy) || 1;
  const cross = ((p[0] - axis.start[0]) * dy - (p[1] - axis.start[1]) * dx) / length;
  if (axis.kind === "radial") {
    const along = ((p[0] - axis.start[0]) * dx + (p[1] - axis.start[1]) * dy) / length;
    if (along < -AXIS_TOLERANCE) return Number.POSITIVE_INFINITY;
  }
  return Math.abs(cross);
}

/** 通り芯の上にある部材（部材を定める節点がすべて通り芯の上にあるもの）の鍵 */
export function membersOnAxis(model: StructureModel, axis: GridAxis): Set<string> {
  const listed = new Set(axis.nodeIds);
  const positions = new Map(model.nodes.map((node) => [node.id, node.position]));
  const onAxis = (id: number) => {
    if (listed.has(id)) return true;
    const p = positions.get(id);
    return !!p && distanceToAxis(axis, p) <= AXIS_TOLERANCE;
  };
  const keys = new Set<string>();
  for (const member of model.members) {
    if (member.nodeIds.length > 0 && member.nodeIds.every(onAxis)) keys.add(member.key);
  }
  return keys;
}

/** 部材の一覧を、種類・階・通り芯の条件で絞る */
export interface MemberFilter {
  hiddenKinds?: ReadonlySet<string>;
  /** 表示する階（null はすべて） */
  stories?: ReadonlySet<number> | null;
  /** 表示する部材の鍵（通り芯で絞ったとき）。null はすべて */
  only?: ReadonlySet<string> | null;
}

export function isMemberVisible(member: StructureMember, filter: MemberFilter): boolean {
  if (filter.hiddenKinds?.has(member.kind)) return false;
  if (filter.stories && (member.storyId === null || !filter.stories.has(member.storyId))) return false;
  if (filter.only && !filter.only.has(member.key)) return false;
  return true;
}
