import { emptyBox, expandBox, isEmptyBox } from "../geometry.js";
import type { GridAxis, Property, StructureMember, StructureModel, Vec2 } from "../types.js";
import { MEMBER_KINDS, MEMBER_LABELS } from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { child, children, num, StbDocument } from "./document.js";
import { buildMember, MEMBER_ELEMENTS, Openings } from "./members.js";

/** 描かない要素と、その説明（読み取りの注意に件数を出す） */
const NOT_DRAWN: Record<string, string> = {
  StbIsolatingDevices: "免震装置",
  StbDampingDevices: "制振装置",
  StbFrameDampingDevices: "制振装置（フレーム）",
  StbPenetrationArrangements: "梁貫通孔",
  StbJointArrangements: "継手の配置",
  StbPanelZoneArrangements: "柱梁接合部",
  StbConnectionArrangements: "接合部の配置",
};

const DRAWN_GROUPS = new Set([...MEMBER_ELEMENTS.map((item) => item.group), "StbOpens", "StbOpenArrangements"]);

const DEGREE = Math.PI / 180;

/** 平行軸・放射軸の線分を、モデルの範囲（XY）に余裕を足して作る（仕様書 4.3） */
function gridAxes(doc: StbDocument, extent: { min: Vec2; max: Vec2 }): GridAxis[] {
  const axes: GridAxis[] = [];
  const axesElement = child(doc.model, "StbAxes");
  const size = Math.max(extent.max[0] - extent.min[0], extent.max[1] - extent.min[1], 1);
  const margin = Math.max(1000, size * 0.08);
  const corners: Vec2[] = [
    [extent.min[0], extent.min[1]],
    [extent.max[0], extent.min[1]],
    [extent.max[0], extent.max[1]],
    [extent.min[0], extent.max[1]],
  ];
  const nodeIds = (element: XmlElement) =>
    children(child(element, "StbNodeIdList"), "StbNodeId").map((item) => num(item, "id"));

  for (const group of children(axesElement, "StbParallelAxes")) {
    // 基準座標点を中心に angle だけ回した座標系の Y への距離に軸がある（4.3.1.1 補足(2)）
    const angle = num(group, "angle") * DEGREE;
    const base: Vec2 = [num(group, "X"), num(group, "Y")];
    const d: Vec2 = [Math.cos(angle), Math.sin(angle)];
    const n: Vec2 = [-Math.sin(angle), Math.cos(angle)];
    const along = corners.map(([x, y]) => (x - base[0]) * d[0] + (y - base[1]) * d[1]);
    const t0 = Math.min(...along) - margin;
    const t1 = Math.max(...along) + margin;
    for (const axis of children(group, "StbParallelAxis")) {
      const offset = num(axis, "distance");
      const origin: Vec2 = [base[0] + n[0] * offset, base[1] + n[1] * offset];
      axes.push({
        kind: "parallel",
        key: `parallel:${group.attributes.group_name ?? ""}:${num(axis, "id")}`,
        group: group.attributes.group_name ?? "",
        name: axis.attributes.name ?? "",
        start: [origin[0] + d[0] * t0, origin[1] + d[1] * t0],
        end: [origin[0] + d[0] * t1, origin[1] + d[1] * t1],
        nodeIds: nodeIds(axis),
      });
    }
  }
  for (const group of children(axesElement, "StbArcAxes")) {
    const full = group.attributes.start_angle === undefined || group.attributes.end_angle === undefined;
    for (const axis of children(group, "StbArcAxis")) {
      axes.push({
        kind: "arc",
        key: `arc:${group.attributes.group_name ?? ""}:${num(axis, "id")}`,
        group: group.attributes.group_name ?? "",
        name: axis.attributes.name ?? "",
        center: [num(group, "X"), num(group, "Y")],
        radius: num(axis, "radius"),
        startAngle: full ? 0 : num(group, "start_angle"),
        endAngle: full ? 360 : num(group, "end_angle"),
        nodeIds: nodeIds(axis),
      });
    }
  }
  for (const group of children(axesElement, "StbRadialAxes")) {
    const center: Vec2 = [num(group, "X"), num(group, "Y")];
    const reach = Math.max(...corners.map(([x, y]) => Math.hypot(x - center[0], y - center[1]))) + margin;
    for (const axis of children(group, "StbRadialAxis")) {
      const angle = num(axis, "angle") * DEGREE;
      axes.push({
        kind: "radial",
        key: `radial:${group.attributes.group_name ?? ""}:${num(axis, "id")}`,
        group: group.attributes.group_name ?? "",
        name: axis.attributes.name ?? "",
        start: center,
        end: [center[0] + Math.cos(angle) * reach, center[1] + Math.sin(angle) * reach],
        nodeIds: nodeIds(axis),
      });
    }
  }
  const drawing = child(axesElement, "StbDrawingAxes");
  children(drawing, "StbDrawingLineAxis").forEach((axis, i) => {
    axes.push({
      kind: "drawingLine",
      key: `drawing-line:${i}`,
      group: axis.attributes.group_name ?? "作図用軸",
      name: axis.attributes.name ?? "",
      start: [num(axis, "start_X"), num(axis, "start_Y")],
      end: [num(axis, "end_X"), num(axis, "end_Y")],
      nodeIds: [],
    });
  });
  children(drawing, "StbDrawingArcAxis").forEach((axis, i) => {
    const full = axis.attributes.start_angle === undefined || axis.attributes.end_angle === undefined;
    axes.push({
      kind: "drawingArc",
      key: `drawing-arc:${i}`,
      group: axis.attributes.group_name ?? "作図用軸",
      name: axis.attributes.name ?? "",
      center: [num(axis, "X"), num(axis, "Y")],
      radius: num(axis, "radius"),
      startAngle: full ? 0 : num(axis, "start_angle"),
      endAngle: full ? 360 : num(axis, "end_angle"),
      nodeIds: [],
    });
  });
  return axes;
}

export interface ModelSource {
  encoding: string;
  /** XML 宣言の文字コード */
  declared: string | null;
  /** 宣言と違う文字コードで読み直した */
  fallback: boolean;
}

const encodingName = (encoding: string) => (encoding === "shift_jis" ? "Shift_JIS" : encoding.toUpperCase());

/** ST_BRIDGE の要素の木から表示用のモデルを作る */
export function buildModel(root: XmlElement, source: ModelSource): StructureModel {
  if (root.name !== "ST_BRIDGE") throw new Error("ST-Bridge のファイルではありません（ルート要素が ST_BRIDGE でない）");
  const version = root.attributes.version ?? "";
  const major = Number(version.split(".")[0]);
  if (major < 2) {
    throw new Error(
      `ST-Bridge ${version} は読めません。ver.2.0 以降（2.0.x・2.1.x）で書き出したファイルを使ってください`,
    );
  }
  const doc = new StbDocument(root);
  if (!doc.model) throw new Error("StbModel がありません");
  if (source.fallback) {
    doc.notices.add(
      "encoding",
      "warning",
      `XML 宣言の文字コード（${source.declared ?? "なし"}）と中身が合わないため、${encodingName(source.encoding)} として読んだ`,
    );
  }
  if (!["2.0", "2.0.0", "2.0.1", "2.0.2", "2.1.0", "2.1.1"].includes(version)) {
    doc.notices.add(
      "version",
      "info",
      `確認していない版（${version}）のため、${doc.is21 ? "2.1" : "2.0"} 系として読んだ`,
    );
  }

  const openings = new Openings(doc);
  const members: StructureMember[] = [];
  for (const { group, element, kind } of MEMBER_ELEMENTS) {
    for (const item of doc.members(group, element)) {
      const member = buildMember(doc, kind, item, openings);
      if (member) members.push(member);
    }
  }
  const unused = openings.unused();
  if (unused > 0) doc.notices.add("open-unused", "warning", "対応する壁・スラブのない開口は描かない", unused);

  const membersElement = child(doc.model, "StbMembers");
  for (const group of membersElement?.children ?? []) {
    if (DRAWN_GROUPS.has(group.name)) continue;
    const label = NOT_DRAWN[group.name];
    doc.notices.add(
      `not-drawn:${group.name}`,
      "info",
      `${label ?? group.name}は描かない`,
      Math.max(group.children.length, 1),
    );
  }
  const extras: [XmlElement | undefined, string][] = [
    [child(doc.model, "StbJoints"), "継手の詳細"],
    [child(doc.model, "StbConnections"), "接合部の詳細"],
    [child(doc.model, "StbWeld"), "溶接の情報"],
    [child(root, "StbCalData"), "計算データ（StbCalData）"],
    [child(root, "StbAnaModels"), "解析モデル（StbAnaModels）"],
    [child(root, "StbExtensions"), "拡張情報（StbExtensions）"],
  ];
  for (const [element, label] of extras) {
    if (element && element.children.length > 0)
      doc.notices.add(`extra:${element.name}`, "info", `${label}は表示しない`, element.children.length);
  }
  const hasBars = (element: XmlElement, depth: number): boolean =>
    element.children.some(
      (item) => item.name.startsWith("StbSecBarArrangement") || (depth > 0 && hasBars(item, depth - 1)),
    );
  const reinforced = (child(doc.model, "StbSections")?.children ?? []).filter((section) => hasBars(section, 2)).length;
  doc.notices.add("rebar", "info", "断面の配筋は描かない", reinforced);

  const box = emptyBox();
  for (const member of members) {
    if (isEmptyBox(member.bounds)) continue;
    expandBox(box, member.bounds.min);
    expandBox(box, member.bounds.max);
  }
  for (const node of doc.nodes.values()) expandBox(box, node.position);
  const bounds = isEmptyBox(box) ? { min: [0, 0, 0] as const, max: [0, 0, 0] as const } : box;

  const counts = new Map<string, number>();
  for (const member of members) counts.set(member.kind, (counts.get(member.kind) ?? 0) + 1);
  const common = doc.common?.attributes ?? {};
  const metadata: Property[] = [
    { label: "ST-Bridge", value: `ver.${version}` },
    {
      label: "文字コード",
      value: source.fallback
        ? `${encodingName(source.encoding)}（宣言は ${source.declared}）`
        : encodingName(source.encoding),
    },
  ];
  if (common.project_name) metadata.push({ label: "プロジェクト", value: common.project_name });
  if (common.app_name)
    metadata.push({ label: "作成ソフト", value: [common.app_name, common.app_version].filter(Boolean).join(" ") });
  if (common.convert_app_name) {
    metadata.push({
      label: "変換プログラム",
      value: [common.convert_app_name, common.convert_app_version].filter(Boolean).join(" "),
    });
  }
  if (common.strength_concrete)
    metadata.push({ label: "コンクリート", value: `${common.strength_concrete}（建物全体）` });
  metadata.push({ label: "節点", value: `${doc.nodes.size.toLocaleString("ja-JP")} 点` });
  metadata.push({ label: "階", value: `${doc.stories.length} 階` });
  const memberSummary = MEMBER_KINDS.filter((kind) => counts.has(kind))
    .map((kind) => `${MEMBER_LABELS[kind]} ${(counts.get(kind) ?? 0).toLocaleString("ja-JP")}`)
    .join("、");
  metadata.push({ label: "部材", value: memberSummary || "なし" });
  if (common.global_offset_X || common.global_offset_Y || common.global_offset_Z || common.global_rotation) {
    metadata.push({
      label: "グローバル座標との関係",
      value: `ΔX ${common.global_offset_X ?? 0} ΔY ${common.global_offset_Y ?? 0} ΔZ ${common.global_offset_Z ?? 0} θ ${common.global_rotation ?? 0}°（表示には使わない）`,
    });
  }

  return {
    version,
    encoding: source.encoding,
    nodes: [...doc.nodes.values()].map((node) => ({ id: node.id, position: node.position, kind: node.kind })),
    stories: doc.stories,
    axes: gridAxes(doc, { min: [bounds.min[0], bounds.min[1]], max: [bounds.max[0], bounds.max[1]] }),
    members,
    bounds,
    metadata,
    notices: doc.notices.list(),
  };
}
