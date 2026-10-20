import type { GridAxis, MemberKind, StructureMember, StructureModel, Vec3 } from "../src/types.js";

// エンジンとフックのテストに使う、最小の構造モデル（コードで作る）

/** 400 mm 角の柱・梁（start から end へ押し出した箱） */
function box(
  key: string,
  kind: MemberKind,
  start: Vec3,
  end: Vec3,
  options: { storyId?: number | null; structure?: string; nodeIds?: number[] } = {},
): StructureMember {
  const vertical = start[0] === end[0] && start[1] === end[1];
  const e1: Vec3 = vertical ? [1, 0, 0] : [0, 1, 0];
  const e2: Vec3 = vertical ? [0, 1, 0] : [0, 0, 1];
  const loop = {
    outer: [
      [-200, -200],
      [200, -200],
      [200, 200],
      [-200, 200],
    ] as [number, number][],
    holes: [],
  };
  // 梁は断面の高さの半分だけ上下に広がる
  const dz = vertical ? 0 : 200;
  const min: Vec3 = [
    Math.min(start[0], end[0]) - 200,
    Math.min(start[1], end[1]) - 200,
    Math.min(start[2], end[2]) - dz,
  ];
  const max: Vec3 = [
    Math.max(start[0], end[0]) + 200,
    Math.max(start[1], end[1]) + 200,
    Math.max(start[2], end[2]) + dz,
  ];
  return {
    key,
    kind,
    element: kind === "column" ? "StbColumn" : "StbGirder",
    id: 1,
    guid: null,
    name: key,
    structure: options.structure ?? "S",
    storyId: options.storyId ?? null,
    section: null,
    solids: [{ type: "extrusion", start, end, e1, e2, startLoop: loop, endLoop: loop, smooth: false }],
    faces: [],
    lines: [],
    nodeIds: options.nodeIds ?? [],
    bounds: { min, max },
    properties: [{ label: "符号", value: key }],
    raw: [],
  };
}

/** 2 本の柱（X1・X2 通り）と、2 階の梁、線だけのブレース。階は 1 階（0）と 2 階（3000） */
export function structure(): StructureModel {
  const axis = (key: string, name: string, x: number): GridAxis => ({
    kind: "parallel",
    key,
    group: "X",
    name,
    start: [x, -1000],
    end: [x, 1000],
    nodeIds: [],
  });
  const brace: StructureMember = {
    ...box("B1", "brace", [0, 0, 0], [6000, 0, 3000], { storyId: 1 }),
    solids: [],
    lines: [
      [
        [0, 2000, 0],
        [6000, 2000, 3000],
      ],
    ],
    bounds: { min: [0, 2000, 0], max: [6000, 2000, 3000] },
  };
  return {
    version: "2.0.2",
    encoding: "utf-8",
    nodes: [
      { id: 1, position: [0, 0, 0], kind: "ON_GRID" },
      { id: 2, position: [0, 0, 3000], kind: "ON_GRID" },
      { id: 3, position: [6000, 0, 0], kind: "ON_GRID" },
      { id: 4, position: [6000, 0, 3000], kind: "ON_GRID" },
    ],
    stories: [
      { id: 1, name: "1F", height: 0, kind: "GENERAL", levelName: null, strengthConcrete: null },
      { id: 2, name: "2F", height: 3000, kind: "GENERAL", levelName: null, strengthConcrete: null },
    ],
    axes: [axis("X1", "X1", 0), axis("X2", "X2", 6000)],
    members: [
      box("C1", "column", [0, 0, 0], [0, 0, 3000], { storyId: 1, nodeIds: [1, 2] }),
      box("C2", "column", [6000, 0, 0], [6000, 0, 3000], { storyId: 1, nodeIds: [3, 4], structure: "RC" }),
      box("G1", "girder", [0, 0, 3000], [6000, 0, 3000], { storyId: 2, nodeIds: [2, 4] }),
      brace,
    ],
    bounds: { min: [-200, -200, 0], max: [6200, 2000, 3200] },
    metadata: [],
    notices: [],
  };
}
