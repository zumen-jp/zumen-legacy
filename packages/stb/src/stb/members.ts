import {
  add,
  cross,
  distance,
  dot,
  emptyBox,
  expandBox,
  expandBoxBySolid,
  extrude,
  type Frame,
  length as lengthOf,
  memberFrame,
  normalize,
  type Profile,
  polygonNormal,
  rotateFrame,
  sameTopology,
  scale,
  sub,
  X,
  Y,
  Z,
} from "../geometry.js";
import {
  type Face,
  MEMBER_LABELS,
  type MemberKind,
  type Property,
  type SectionSummary,
  type Solid,
  type StructureMember,
  type Vec2,
  type Vec3,
} from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { bool, child, children, idList, num, optionalNum, type StbDocument } from "./document.js";
import {
  foundationColumnProfile,
  resolveFootingSection,
  resolveParapetSection,
  resolvePileSection,
  resolveStripFootingSection,
} from "./foundations.js";
import { resolveSlabSection, resolveWallSection } from "./panels.js";
import { pipe, solidCircle } from "./profiles.js";
import {
  formatNumber,
  type RawAttributes,
  rawOf,
  resolveBeamSection,
  resolveBraceSection,
  resolveColumnSection,
  type Segment,
} from "./sections.js";

const STRUCTURE_LABELS: Record<string, string> = {
  RC: "RC（鉄筋コンクリート）",
  S: "S（鉄骨）",
  SRC: "SRC（鉄骨鉄筋コンクリート）",
  CFT: "CFT（コンクリート充填鋼管）",
  DECK: "DECK（デッキ合成スラブ）",
  PRECAST: "PRECAST（既製スラブ）",
  LOAD: "LOAD（荷重のみ）",
  PC: "PC（既製コンクリート杭）",
  UNDEFINED: "UNDEFINED（構造種別なし）",
};

const formatPoint = (p: Vec3) => `(${p.map((v) => formatNumber(Math.round(v * 10) / 10)).join(", ")})`;

interface Built {
  solids: Solid[];
  faces: Face[];
  lines: Vec3[][];
}

function interpolateProfile(a: Profile, b: Profile, t: number): Profile {
  if (t <= 0 || a === b || !sameTopology(a, b)) return a;
  if (t >= 1) return b;
  const mix = (p: Vec2[], q: Vec2[]) =>
    p.map(([x, y], i) => [x + ((q[i]?.[0] ?? x) - x) * t, y + ((q[i]?.[1] ?? y) - y) * t] as const);
  return {
    smooth: a.smooth,
    parts: a.parts.map((part, k) => {
      const other = b.parts[k] ?? part;
      return {
        outer: mix(part.outer, other.outer),
        holes: part.holes.map((hole, h) => mix(hole, other.holes[h] ?? hole)),
      };
    }),
  };
}

/**
 * 折れ線の配置基準線に沿って、区間ごとの断面を押し出す。区間の位置は始端からの線に沿った距離。
 * frameOf は線分ごとの部材座標系（中間節点で折れる柱・梁、5.2.1.1・5.4.1.1）。
 */
function sweep(path: Vec3[], frameOf: (axis: Vec3) => Frame, segments: Segment[]): Solid[] {
  const solids: Solid[] = [];
  const cumulative = [0];
  for (let i = 1; i < path.length; i++)
    cumulative.push((cumulative[i - 1] as number) + distance(path[i - 1] as Vec3, path[i] as Vec3));
  for (let k = 0; k + 1 < path.length; k++) {
    const p0 = path[k] as Vec3;
    const p1 = path[k + 1] as Vec3;
    const c0 = cumulative[k] as number;
    const c1 = cumulative[k + 1] as number;
    if (c1 - c0 < 1e-6) continue;
    const axis = normalize(sub(p1, p0));
    const frame = frameOf(axis);
    for (const segment of segments) {
      const a = Math.max(segment.from, c0);
      const b = Math.min(segment.to, c1);
      if (b - a < 1e-6) continue;
      const span = segment.to - segment.from || 1;
      const pa = interpolateProfile(segment.start, segment.end, (a - segment.from) / span);
      const pb = interpolateProfile(segment.start, segment.end, (b - segment.from) / span);
      solids.push(...extrude(add(p0, scale(axis, a - c0)), add(p0, scale(axis, b - c0)), frame, pa, pb));
    }
  }
  return solids;
}

function pathLength(path: Vec3[]): number {
  let total = 0;
  for (let i = 1; i < path.length; i++) total += distance(path[i - 1] as Vec3, path[i] as Vec3);
  return total;
}

/** 部材を一覧の形にする。外接箱は描く形（なければ基準線）から求める */
function finish(
  doc: StbDocument,
  kind: MemberKind,
  element: XmlElement,
  built: Built,
  nodeIds: number[],
  storyNodeId: number | null,
  section: SectionSummary | null,
  properties: Property[],
  raw: RawAttributes[],
): StructureMember {
  const box = emptyBox();
  for (const solid of built.solids) expandBoxBySolid(box, solid);
  for (const face of built.faces) for (const p of face.outer) expandBox(box, p);
  for (const line of built.lines) for (const p of line) expandBox(box, p);
  const id = num(element, "id");
  const storyId = storyNodeId === null ? null : (doc.nodeStory.get(storyNodeId) ?? null);
  const story = doc.story(storyId);
  const structure = element.attributes.kind_structure ?? "";
  const head: Property[] = [
    { label: "部材", value: `${MEMBER_LABELS[kind]}（${element.name}）` },
    { label: "符号", value: element.attributes.name || "（なし）" },
    { label: "ID", value: String(id) },
  ];
  if (element.attributes.guid) head.push({ label: "GUID", value: element.attributes.guid });
  if (structure) head.push({ label: "構造種別", value: STRUCTURE_LABELS[structure] ?? structure });
  if (section) {
    head.push({ label: "断面", value: `${section.name || "（名称なし）"}（${section.element} id=${section.id}）` });
    if (section.shapes.length > 0) head.push({ label: "断面形状", value: section.shapes.join(" / ") });
  }
  if (story) head.push({ label: "階", value: story.levelName ? `${story.name}（${story.levelName}）` : story.name });
  return {
    key: `${element.name}#${id}`,
    kind,
    element: element.name,
    id,
    guid: element.attributes.guid ?? null,
    name: element.attributes.name ?? "",
    structure,
    storyId,
    section,
    solids: built.solids,
    faces: built.faces,
    lines: built.lines,
    nodeIds,
    bounds: { min: box.min, max: box.max },
    properties: [...head, ...properties],
    raw: [rawOf(element), ...raw],
  };
}

const offsetOf = (element: XmlElement, prefix: string): Vec3 => [
  num(element, `${prefix}_X`),
  num(element, `${prefix}_Y`),
  num(element, `${prefix}_Z`),
];

function missingNode(doc: StbDocument): null {
  doc.notices.add("missing-node", "warning", "存在しない節点を参照する部材は描かない");
  return null;
}

/** 中間節点（StbColumnViaNode など）を含む配置基準線 */
function viaPath(doc: StbDocument, element: XmlElement, viaName: string, start: Vec3, end: Vec3): Vec3[] {
  const via = child(element, viaName);
  if (!via) return [start, end];
  const offsets = new Map<number, Vec3>();
  for (const item of children(via, "StbMemberOffsetList")) {
    offsets.set(num(item, "id_node"), [num(item, "offset_X"), num(item, "offset_Y"), num(item, "offset_Z")]);
  }
  const points: Vec3[] = [start];
  for (const id of idList(child(via, "StbNodeIdOrder"))) {
    const node = doc.node(id);
    if (node) points.push(add(node.position, offsets.get(id) ?? [0, 0, 0]));
  }
  points.push(end);
  return points;
}

function lengthProperty(path: Vec3[]): Property {
  return { label: "長さ", value: `${formatNumber(Math.round(pathLength(path)))} mm（基準点の間）` };
}

function offsetProperty(label: string, value: Vec3): Property | null {
  return value.some((v) => v !== 0) ? { label, value: formatPoint(value) } : null;
}

const present = <T>(items: (T | null | undefined)[]): T[] => items.filter((item): item is T => !!item);

// ---- 柱・間柱 ----------------------------------------------------------------

function buildColumn(doc: StbDocument, element: XmlElement, kind: "column" | "post"): StructureMember | null {
  const bottomId = num(element, "id_node_bottom", Number.NaN);
  const topId = num(element, "id_node_top", Number.NaN);
  const bottom = doc.node(bottomId);
  const top = doc.node(topId);
  if (!bottom || !top) return missingNode(doc);
  const ob = offsetOf(element, "offset_bottom");
  const ot = offsetOf(element, "offset_top");
  // 柱の基準点は図心（5.2.1 補足(2)）
  const path = viaPath(
    doc,
    element,
    kind === "column" ? "StbColumnViaNode" : "StbPostViaNode",
    add(bottom.position, ob),
    add(top.position, ot),
  );
  const structure = element.attributes.kind_structure ?? "";
  const resolved = resolveColumnSection(doc, element, structure, pathLength(path), topId);
  const rotate = num(element, "rotate");
  const frameOf = (axis: Vec3) => rotateFrame(memberFrame(axis, Y, X), rotate);
  const solids = sweep(path, frameOf, resolved.segments);
  const built: Built = { solids, faces: [], lines: solids.length > 0 ? [] : [path] };
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    {
      label: "節点",
      value: `柱脚 ${bottomId} ${formatPoint(bottom.position)} / 柱頭 ${topId} ${formatPoint(top.position)}`,
    },
    lengthProperty(path),
    rotate ? { label: "回転", value: `${formatNumber(rotate)}°` } : null,
    offsetProperty("柱脚のオフセット", ob),
    offsetProperty("柱頭のオフセット", ot),
  ]);
  return finish(doc, kind, element, built, [bottomId, topId], topId, resolved.summary, properties, resolved.raw);
}

// ---- 大梁・小梁 --------------------------------------------------------------

function buildBeam(doc: StbDocument, element: XmlElement, kind: "girder" | "beam"): StructureMember | null {
  const startId = num(element, "id_node_start", Number.NaN);
  const endId = num(element, "id_node_end", Number.NaN);
  const start = doc.node(startId);
  const end = doc.node(endId);
  if (!start || !end) return missingNode(doc);
  const os = offsetOf(element, "offset_start");
  const oe = offsetOf(element, "offset_end");
  // 梁の基準点は天端の幅中心（5.4.1 補足(3)）
  const ps = add(start.position, os);
  const pe = add(end.position, oe);
  const path = viaPath(doc, element, kind === "girder" ? "StbGirderViaNode" : "StbBeamViaNode", ps, pe);
  const total = pathLength(path);
  const firstAxis = normalize(sub(path[1] as Vec3, ps));
  const lastAxis = normalize(sub(pe, path[path.length - 2] as Vec3));
  // 2.0 の継手位置は柱の中心（節点）から測るので、基準点までの材軸方向の距離を返す
  const nodeShift = { start: dot(os, firstAxis), end: -dot(oe, lastAxis) };
  const structure = element.attributes.kind_structure ?? "";
  const resolved = resolveBeamSection(doc, element, structure, total, startId, nodeShift);
  const rotate = num(element, "rotate");
  const frameOf = (axis: Vec3) => rotateFrame(memberFrame(axis, Z, Y), rotate);
  const solids = sweep(path, frameOf, resolved.segments);
  const built: Built = { solids, faces: [], lines: solids.length > 0 ? [] : [path] };
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    {
      label: "節点",
      value: `始端 ${startId} ${formatPoint(start.position)} / 終端 ${endId} ${formatPoint(end.position)}`,
    },
    lengthProperty(path),
    bool(element, "isFoundation", false) ? { label: "基礎梁", value: "はい" } : null,
    rotate ? { label: "回転", value: `${formatNumber(rotate)}°` } : null,
    offsetProperty("始端のオフセット", os),
    offsetProperty("終端のオフセット", oe),
    element.attributes.haunch_start
      ? { label: "ハンチ位置（始端）", value: `${element.attributes.haunch_start} mm` }
      : null,
    element.attributes.haunch_end
      ? { label: "ハンチ位置（終端）", value: `${element.attributes.haunch_end} mm` }
      : null,
    element.attributes.joint_start
      ? { label: "継手位置（始端）", value: `${element.attributes.joint_start} mm` }
      : null,
    element.attributes.joint_end ? { label: "継手位置（終端）", value: `${element.attributes.joint_end} mm` } : null,
  ]);
  return finish(doc, kind, element, built, [startId, endId], startId, resolved.summary, properties, resolved.raw);
}

// ---- ブレース ----------------------------------------------------------------

function buildBrace(doc: StbDocument, element: XmlElement): StructureMember | null {
  const startId = num(element, "id_node_start", Number.NaN);
  const endId = num(element, "id_node_end", Number.NaN);
  const start = doc.node(startId);
  const end = doc.node(endId);
  if (!start || !end) return missingNode(doc);
  // 2.1 は狙い点オフセット（aim_offset_*）、2.0 は offset_*（5.6.1）
  const os =
    element.attributes.aim_offset_start_X !== undefined || element.attributes.aim_offset_start_Z !== undefined
      ? offsetOf(element, "aim_offset_start")
      : offsetOf(element, "offset_start");
  const oe =
    element.attributes.aim_offset_end_X !== undefined || element.attributes.aim_offset_end_Z !== undefined
      ? offsetOf(element, "aim_offset_end")
      : offsetOf(element, "offset_end");
  let ps = add(start.position, os);
  let pe = add(end.position, oe);
  const axis = normalize(sub(pe, ps));
  const cutStart = num(element, "cutback_start");
  const cutEnd = num(element, "cutback_end");
  const full: Vec3[] = [ps, pe];
  if (cutStart > 0 || cutEnd > 0) {
    ps = add(ps, scale(axis, cutStart));
    pe = sub(pe, scale(axis, cutEnd));
  }
  const total = distance(ps, pe);
  const resolved = resolveBraceSection(doc, element, total, ps[2] <= pe[2]);
  const rotate = num(element, "rotate");
  const solids = sweep([ps, pe], (a) => rotateFrame(memberFrame(a, Z, Y), rotate), resolved.segments);
  const built: Built = { solids, faces: [], lines: solids.length > 0 ? [] : [full] };
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    {
      label: "節点",
      value: `始端 ${startId} ${formatPoint(start.position)} / 終端 ${endId} ${formatPoint(end.position)}`,
    },
    lengthProperty(full),
    {
      label: "ブレース特性",
      value: element.attributes.feature_brace === "TENSIONANDCOMPRESSION" ? "引張り・圧縮" : "引張り",
    },
    rotate ? { label: "回転", value: `${formatNumber(rotate)}°` } : null,
    offsetProperty("始端の狙い点オフセット", os),
    offsetProperty("終端の狙い点オフセット", oe),
  ]);
  // ブレースの階は、柱と同じく上側の節点で決める
  const upper = start.position[2] >= end.position[2] ? startId : endId;
  return finish(doc, "brace", element, built, [startId, endId], upper, resolved.summary, properties, resolved.raw);
}

// ---- 開口 --------------------------------------------------------------------

interface Opening {
  x: number;
  y: number;
  width: number;
  height: number;
  rotate: number;
  name: string;
}

/** 壁・スラブの開口。2.1 は StbOpenArrangement（5.22）、2.0 は StbOpenIdList と StbOpens（5.14） */
export class Openings {
  private readonly byMember = new Map<string, Opening[]>();
  private readonly legacy = new Map<number, XmlElement>();
  readonly used = new Set<string>();
  total = 0;

  constructor(doc: StbDocument) {
    for (const open of doc.members("StbOpens", "StbOpen")) this.legacy.set(num(open, "id"), open);
    for (const item of doc.members("StbOpenArrangements", "StbOpenArrangement")) {
      this.total++;
      const section = doc.section("StbSecOpen_RC", num(item, "id_section", Number.NaN));
      const width = num(section, "length_X");
      const height = num(section, "length_Y");
      if (width <= 0 || height <= 0) {
        doc.notices.add("open-size", "warning", "大きさの分からない開口は描かない");
        continue;
      }
      const key = `${item.attributes.kind_member === "SLAB" ? "StbSlab" : "StbWall"}#${num(item, "id_member")}`;
      const list = this.byMember.get(key) ?? [];
      list.push({
        x: num(item, "position_X"),
        y: num(item, "position_Y"),
        width,
        height,
        rotate: num(item, "rotate"),
        name: item.attributes.name ?? "",
      });
      this.byMember.set(key, list);
    }
    this.total += this.legacy.size;
  }

  of(element: XmlElement): Opening[] {
    const key = `${element.name}#${num(element, "id")}`;
    const list = [...(this.byMember.get(key) ?? [])];
    if (list.length > 0) this.used.add(key);
    for (const ref of children(child(element, "StbOpenIdList"), "StbOpenId")) {
      const open = this.legacy.get(num(ref, "id"));
      if (!open) continue;
      this.used.add(`StbOpen#${num(open, "id")}`);
      list.push({
        x: num(open, "position_X"),
        y: num(open, "position_Y"),
        width: num(open, "length_X"),
        height: num(open, "length_Y"),
        rotate: num(open, "rotate"),
        name: open.attributes.name ?? "",
      });
    }
    return list;
  }

  /** 対応する壁・スラブのない開口の数 */
  unused(): number {
    let count = 0;
    for (const [key, list] of this.byMember) if (!this.used.has(key)) count += list.length;
    for (const id of this.legacy.keys()) if (!this.used.has(`StbOpen#${id}`)) count++;
    return count;
  }
}

/**
 * 開口の 4 隅。位置・寸法は第 1 基準点から、1 点目から 2 点目の向きを X、それを反時計回りに 90 度回した向きを Y とする
 * （5.22.1 補足(1)）。ex・ey は面内の X・Y の単位ベクトル。
 */
function openingCorners(origin: Vec3, ex: Vec3, ey: Vec3, open: Opening): Vec3[] {
  const c = Math.cos((open.rotate * Math.PI) / 180);
  const s = Math.sin((open.rotate * Math.PI) / 180);
  const u = add(scale(ex, c), scale(ey, s));
  const v = add(scale(ex, -s), scale(ey, c));
  const o = add(origin, add(scale(ex, open.x), scale(ey, open.y)));
  return [
    o,
    add(o, scale(u, open.width)),
    add(add(o, scale(u, open.width)), scale(v, open.height)),
    add(o, scale(v, open.height)),
  ];
}

function panelRing(
  doc: StbDocument,
  element: XmlElement,
  offsetList: string,
  offsetItem: string,
): { ids: number[]; ring: Vec3[] } | null {
  const ids = idList(child(element, "StbNodeIdOrder"));
  const offsets = new Map<number, Vec3>();
  for (const item of children(child(element, offsetList), offsetItem)) {
    offsets.set(num(item, "id_node"), [num(item, "offset_X"), num(item, "offset_Y"), num(item, "offset_Z")]);
  }
  const ring: Vec3[] = [];
  for (const id of ids) {
    const node = doc.node(id);
    if (!node) return null;
    ring.push(add(node.position, offsets.get(id) ?? [0, 0, 0]));
  }
  // 閉じた多角形の終点が始点と重なる書き方を許す
  if (ring.length > 3 && distance(ring[0] as Vec3, ring[ring.length - 1] as Vec3) < 1e-6) ring.pop();
  return ring.length >= 3 ? { ids, ring } : null;
}

// ---- スラブ ------------------------------------------------------------------

function buildSlab(doc: StbDocument, element: XmlElement, openings: Openings): StructureMember | null {
  const panel = panelRing(doc, element, "StbSlabOffsetList", "StbSlabOffset");
  if (!panel) {
    doc.notices.add("slab-nodes", "warning", "節点が 3 つ未満、または存在しない節点を参照するスラブは描かない");
    return null;
  }
  const { ids, ring } = panel;
  const resolved = resolveSlabSection(doc, element, ids[0] as number);
  // 節点の並びは下から見て時計回り（5.7.1 補足(5)）。上面は節点（オフセット後）
  const winding = polygonNormal(ring);
  let up = winding;
  if (dot(up, Z) < 0) up = scale(up, -1);
  if (lengthOf(up) < 0.5) up = Z;
  const ex = normalize(sub(ring[1] as Vec3, ring[0] as Vec3));
  const ey = normalize(cross(winding, ex));
  const holes = openings.of(element).map((open) => openingCorners(ring[0] as Vec3, ex, ey, open));
  const addTop = num(element, "thickness_add_top");
  const addBottom = num(element, "thickness_add_bottom");
  const built: Built = { solids: [], faces: [], lines: [] };
  if (resolved.load) {
    built.faces.push({ outer: ring, holes });
  } else {
    const t = resolved.thickness + addBottom;
    const shift = (points: Vec3[], k: number) => points.map((p) => add(p, scale(up, k)));
    built.solids.push({
      type: "loft",
      sections: [
        { outer: shift(ring, addTop), holes: holes.map((hole) => shift(hole, addTop)) },
        { outer: shift(ring, -t), holes: holes.map((hole) => shift(hole, -t)) },
      ],
      smooth: false,
    });
  }
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    { label: "スラブ種類", value: element.attributes.kind_slab === "CANTI" ? "片持ち（CANTI）" : "一般（NORMAL）" },
    { label: "節点", value: ids.join(" ") },
    bool(element, "isFoundation", false) ? { label: "基礎スラブ", value: "はい" } : null,
    holes.length > 0 ? { label: "開口", value: `${holes.length} か所` } : null,
    element.attributes.direction_load ? { label: "荷重伝達方向", value: element.attributes.direction_load } : null,
  ]);
  return finish(doc, "slab", element, built, ids, ids[0] ?? null, resolved.summary, properties, resolved.raw);
}

// ---- 壁 ----------------------------------------------------------------------

function buildWall(doc: StbDocument, element: XmlElement, openings: Openings): StructureMember | null {
  const panel = panelRing(doc, element, "StbWallOffsetList", "StbWallOffset");
  if (!panel) {
    doc.notices.add("wall-nodes", "warning", "節点が 3 つ未満、または存在しない節点を参照する壁は描かない");
    return null;
  }
  const { ids, ring } = panel;
  const resolved = resolveWallSection(doc, element, ids[ids.length - 1] as number);
  // 壁の部材座標系は 1 点目から 2 点目が X、鉛直上向きが Y（5.8.1 補足(3)）。厚さは Z 方向（右が正）
  const ex = normalize(sub(ring[1] as Vec3, ring[0] as Vec3));
  let normal = normalize(cross(ex, Z));
  const winding = polygonNormal(ring);
  if (lengthOf(normal) < 0.5) normal = winding;
  const ey = normalize(cross(winding, ex));
  const holes = openings.of(element).map((open) => openingCorners(ring[0] as Vec3, ex, ey, open));
  const built: Built = { solids: [], faces: [], lines: [] };
  if (resolved.load) {
    built.faces.push({ outer: ring, holes });
  } else {
    const right = num(element, "thickness_add_right");
    const left = num(element, "thickness_add_left");
    const zs = ring.map((p) => p[2]);
    const z0 = Math.min(...zs);
    const z1 = Math.max(...zs);
    const half = (p: Vec3) => {
      if (!resolved.taper || z1 - z0 < 1e-6) return resolved.thickness / 2;
      const t = (p[2] - z0) / (z1 - z0);
      return (resolved.taper.bottom + (resolved.taper.top - resolved.taper.bottom) * t) / 2;
    };
    const side = (points: Vec3[], sign: 1 | -1) =>
      points.map((p) => add(p, scale(normal, sign * (half(p) + (sign > 0 ? right : left)))));
    built.solids.push({
      type: "loft",
      sections: [
        { outer: side(ring, 1), holes: holes.map((hole) => side(hole, 1)) },
        { outer: side(ring, -1), holes: holes.map((hole) => side(hole, -1)) },
      ],
      smooth: false,
    });
  }
  const layout: Record<string, string> = { ON_GIRDER: "大梁上", ON_BEAM: "小梁上", ON_SLAB: "スラブ上" };
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    { label: "耐力区分", value: element.attributes.kind_wall === "WALL_SHEAR" ? "耐力壁" : "一般壁" },
    element.attributes.kind_layout
      ? { label: "配置", value: layout[element.attributes.kind_layout] ?? element.attributes.kind_layout }
      : null,
    { label: "節点", value: ids.join(" ") },
    holes.length > 0 ? { label: "開口", value: `${holes.length} か所` } : null,
  ]);
  return finish(
    doc,
    "wall",
    element,
    built,
    ids,
    ids[ids.length - 1] ?? null,
    resolved.summary,
    properties,
    resolved.raw,
  );
}

// ---- 基礎・杭・パラペット -----------------------------------------------------

function rotate2(points: Vec2[], degrees: number): Vec2[] {
  if (!degrees) return points;
  const c = Math.cos((degrees * Math.PI) / 180);
  const s = Math.sin((degrees * Math.PI) / 180);
  return points.map(([x, y]) => [x * c - y * s, x * s + y * c] as const);
}

function buildFooting(doc: StbDocument, element: XmlElement): StructureMember | null {
  const nodeId = num(element, "id_node", Number.NaN);
  const node = doc.node(nodeId);
  if (!node) return missingNode(doc);
  const resolved = resolveFootingSection(doc, element);
  const [nx, ny, nz] = node.position;
  // 基準点は図心、オフセットとレベルの省略時は下端の基準点が節点（5.17.1 補足(1)(2)）
  const cx = nx + num(element, "offset_X");
  const cy = ny + num(element, "offset_Y");
  const bottom = nz + num(element, "level_bottom") - num(element, "thickness_add_bottom");
  const built: Built = { solids: [], faces: [], lines: [] };
  if (resolved) {
    const outline = rotate2(resolved.outline, num(element, "rotate"));
    const at = (points: Vec2[], z: number, k = 1): Vec3[] =>
      points.map(([x, y]) => [cx + x * k, cy + y * k, z] as const);
    const sections = [{ outer: at(outline, bottom), holes: [] as Vec3[][] }];
    if (resolved.tip !== null && resolved.tip > 0 && resolved.tip < resolved.depth) {
      sections.push({ outer: at(outline, bottom + resolved.tip), holes: [] });
      sections.push({ outer: at(outline, bottom + resolved.depth, 0.5), holes: [] });
    } else {
      sections.push({ outer: at(outline, bottom + resolved.depth), holes: [] });
    }
    built.solids.push({ type: "loft", sections, smooth: false });
  } else {
    built.lines.push([node.position, [cx, cy, bottom]]);
  }
  const properties = present<Property>([
    resolved?.materials.length ? { label: "材料", value: resolved.materials.join("、") } : null,
    { label: "節点", value: `${nodeId} ${formatPoint(node.position)}` },
    element.attributes.level_bottom ? { label: "レベル（下）", value: `${element.attributes.level_bottom} mm` } : null,
    num(element, "rotate") ? { label: "回転", value: `${formatNumber(num(element, "rotate"))}°` } : null,
  ]);
  return finish(
    doc,
    "footing",
    element,
    built,
    [nodeId],
    nodeId,
    resolved?.summary ?? null,
    properties,
    resolved?.raw ?? [],
  );
}

function buildStripFooting(doc: StbDocument, element: XmlElement): StructureMember | null {
  const startId = num(element, "id_node_start", Number.NaN);
  const endId = num(element, "id_node_end", Number.NaN);
  const start = doc.node(startId);
  const end = doc.node(endId);
  if (!start || !end) return missingNode(doc);
  const resolved = resolveStripFootingSection(doc, element);
  const axis = normalize(sub(end.position, start.position));
  const frame = memberFrame(axis, Z, Y);
  // オフセットは始端から終端に向かって左が正、余長は外側が正（5.18.1 補足(4)(5)）
  const shift = add(scale(frame.e1, num(element, "offset")), [0, 0, num(element, "level")]);
  const ps = sub(add(start.position, shift), scale(axis, num(element, "length_ex_start")));
  const pe = add(add(end.position, shift), scale(axis, num(element, "length_ex_end")));
  const built: Built = { solids: [], faces: [], lines: [] };
  if (resolved) built.solids.push(...extrude(ps, pe, frame, resolved.profile));
  else built.lines.push([ps, pe]);
  const properties = present<Property>([
    resolved?.materials.length ? { label: "材料", value: resolved.materials.join("、") } : null,
    {
      label: "節点",
      value: `始端 ${startId} ${formatPoint(start.position)} / 終端 ${endId} ${formatPoint(end.position)}`,
    },
    lengthProperty([ps, pe]),
  ]);
  return finish(
    doc,
    "stripFooting",
    element,
    built,
    [startId, endId],
    startId,
    resolved?.summary ?? null,
    properties,
    resolved?.raw ?? [],
  );
}

function buildPile(doc: StbDocument, element: XmlElement): StructureMember | null {
  const nodeId = num(element, "id_node", Number.NaN);
  const node = doc.node(nodeId);
  if (!node) return missingNode(doc);
  const resolved = resolvePileSection(doc, element);
  // 杭の基準点は図心、オフセットとレベルの省略時は杭頭の基準点が節点（5.19.1 補足(1)(2)）
  const top: Vec3 = [
    node.position[0] + num(element, "offset_X"),
    node.position[1] + num(element, "offset_Y"),
    node.position[2] + num(element, "level_top"),
  ];
  const down: Vec3 = [0, 0, -1];
  const frame = memberFrame(down, Y, X);
  const built: Built = { solids: [], faces: [], lines: [] };
  for (const segment of resolved.segments) {
    const make = (d: number) => (segment.thickness > 0 ? pipe(d, segment.thickness) : solidCircle(d));
    built.solids.push(
      ...extrude(
        add(top, scale(down, segment.top)),
        add(top, scale(down, segment.top + segment.length)),
        frame,
        make(segment.diameterTop),
        make(segment.diameterBottom),
      ),
    );
  }
  const depth = resolved.segments.reduce((sum, s) => Math.max(sum, s.top + s.length), 0);
  if (built.solids.length === 0) built.lines.push([top, add(top, [0, 0, -Math.max(num(element, "length_all"), 1000)])]);
  const properties = present<Property>([
    resolved.materials.length > 0 ? { label: "材料", value: resolved.materials.join("、") } : null,
    { label: "節点", value: `${nodeId} ${formatPoint(node.position)}` },
    { label: "杭頭", value: formatPoint(top) },
    depth > 0 ? { label: "杭長", value: `${formatNumber(depth)} mm` } : null,
  ]);
  return finish(doc, "pile", element, built, [nodeId], nodeId, resolved.summary, properties, resolved.raw);
}

function buildFoundationColumn(doc: StbDocument, element: XmlElement): StructureMember | null {
  const nodeId = num(element, "id_node", Number.NaN);
  const node = doc.node(nodeId);
  if (!node) return missingNode(doc);
  const rotate = num(element, "rotate");
  const frame = rotateFrame(memberFrame(Z, Y, X), rotate);
  const base = node.position[2] + num(element, "offset_Z");
  const built: Built = { solids: [], faces: [], lines: [] };
  const shapes: string[] = [];
  const raw: RawAttributes[] = [];
  let summary: SectionSummary | null = null;
  // 基礎柱は基準点から下へ、根巻柱は上へ（5.20.1 補足(8)）
  for (const [part, sign, label] of [
    ["FD", -1, "基礎柱"],
    ["WR", 1, "根巻柱"],
  ] as const) {
    const id = optionalNum(element, `id_section_${part}`);
    const length = num(element, `length_${part}`);
    if (id === null || length === 0) continue;
    const resolved = foundationColumnProfile(doc, id);
    const center: Vec3 = [
      node.position[0] + num(element, `offset_${part}_X`),
      node.position[1] + num(element, `offset_${part}_Y`),
      base,
    ];
    if (!resolved) {
      doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
      built.lines.push([center, add(center, [0, 0, sign * length])]);
      continue;
    }
    const a = center;
    const b = add(center, [0, 0, sign * length]);
    built.solids.push(...extrude(sign > 0 ? a : b, sign > 0 ? b : a, frame, resolved.profile));
    shapes.push(`${label} ${resolved.label}`);
    raw.push(rawOf(resolved.section));
    summary ??= { element: "StbSecColumn_RC", id, name: resolved.section.attributes.name ?? "", shapes };
  }
  if (summary) summary.shapes = shapes;
  const properties = present<Property>([
    { label: "節点", value: `${nodeId} ${formatPoint(node.position)}（S 柱の柱脚）` },
    element.attributes.length_FD ? { label: "基礎柱高さ", value: `${element.attributes.length_FD} mm` } : null,
    element.attributes.length_WR ? { label: "根巻柱高さ", value: `${element.attributes.length_WR} mm` } : null,
  ]);
  return finish(doc, "foundationColumn", element, built, [nodeId], nodeId, summary, properties, raw);
}

function buildParapet(doc: StbDocument, element: XmlElement): StructureMember | null {
  const startId = num(element, "id_node_start", Number.NaN);
  const endId = num(element, "id_node_end", Number.NaN);
  const start = doc.node(startId);
  const end = doc.node(endId);
  if (!start || !end) return missingNode(doc);
  const resolved = resolveParapetSection(doc, element);
  const axis = normalize(sub(end.position, start.position));
  const frame = memberFrame(axis, Z, Y);
  // オフセットは左が正、レベルは下端の位置（5.21.1 補足(1)(2)）
  const shift = add(scale(frame.e1, num(element, "offset")), [0, 0, num(element, "level")]);
  const ps = add(start.position, shift);
  const pe = add(end.position, shift);
  const built: Built = { solids: [], faces: [], lines: [] };
  if (resolved) built.solids.push(...extrude(ps, pe, frame, resolved.profile));
  else built.lines.push([ps, pe]);
  const properties = present<Property>([
    resolved?.materials.length ? { label: "材料", value: resolved.materials.join("、") } : null,
    {
      label: "節点",
      value: `始端 ${startId} ${formatPoint(start.position)} / 終端 ${endId} ${formatPoint(end.position)}`,
    },
    lengthProperty([ps, pe]),
    element.attributes.direction
      ? { label: "アゴの向き", value: element.attributes.direction === "L" ? "左" : "右" }
      : null,
  ]);
  return finish(
    doc,
    "parapet",
    element,
    built,
    [startId, endId],
    startId,
    resolved?.summary ?? null,
    properties,
    resolved?.raw ?? [],
  );
}

/** 部材の要素（StbMembers の子の子）と作り方 */
export const MEMBER_ELEMENTS: { group: string; element: string; kind: MemberKind }[] = [
  { group: "StbColumns", element: "StbColumn", kind: "column" },
  { group: "StbPosts", element: "StbPost", kind: "post" },
  { group: "StbGirders", element: "StbGirder", kind: "girder" },
  { group: "StbBeams", element: "StbBeam", kind: "beam" },
  { group: "StbBraces", element: "StbBrace", kind: "brace" },
  { group: "StbSlabs", element: "StbSlab", kind: "slab" },
  { group: "StbWalls", element: "StbWall", kind: "wall" },
  { group: "StbFootings", element: "StbFooting", kind: "footing" },
  { group: "StbStripFootings", element: "StbStripFooting", kind: "stripFooting" },
  { group: "StbPiles", element: "StbPile", kind: "pile" },
  { group: "StbFoundationColumns", element: "StbFoundationColumn", kind: "foundationColumn" },
  { group: "StbParapets", element: "StbParapet", kind: "parapet" },
];

export function buildMember(
  doc: StbDocument,
  kind: MemberKind,
  element: XmlElement,
  openings: Openings,
): StructureMember | null {
  switch (kind) {
    case "column":
    case "post":
      return buildColumn(doc, element, kind);
    case "girder":
    case "beam":
      return buildBeam(doc, element, kind);
    case "brace":
      return buildBrace(doc, element);
    case "slab":
      return buildSlab(doc, element, openings);
    case "wall":
      return buildWall(doc, element, openings);
    case "footing":
      return buildFooting(doc, element);
    case "stripFooting":
      return buildStripFooting(doc, element);
    case "pile":
      return buildPile(doc, element);
    case "foundationColumn":
      return buildFoundationColumn(doc, element);
    case "parapet":
      return buildParapet(doc, element);
  }
}
