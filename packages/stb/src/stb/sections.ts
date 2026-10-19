import { type Profile, profileBounds, rotateProfile90, sameTopology, translateProfile } from "../geometry.js";
import type { SectionSummary } from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { bool, child, children, num, optionalNum, type StbDocument } from "./document.js";
import { rect, single, solidCircle } from "./profiles.js";

/** 材軸方向の区間。from・to は始端の基準点からの距離（mm） */
export interface Segment {
  from: number;
  to: number;
  start: Profile;
  end: Profile;
  label: string;
}

export interface RawAttributes {
  label: string;
  attributes: Record<string, string>;
}

export interface ResolvedSection {
  summary: SectionSummary | null;
  segments: Segment[];
  /** 材料の表示（「鋼材 SN490B」「コンクリート FC24（断面）」など） */
  materials: string[];
  raw: RawAttributes[];
}

export const formatNumber = (value: number): string =>
  Number.isInteger(value) ? String(value) : String(Number(value.toFixed(1)));

const SECTION_ELEMENTS: Record<string, Record<string, string>> = {
  column: { RC: "StbSecColumn_RC", S: "StbSecColumn_S", SRC: "StbSecColumn_SRC", CFT: "StbSecColumn_CFT" },
  beam: { RC: "StbSecBeam_RC", S: "StbSecBeam_S", SRC: "StbSecBeam_SRC" },
};

export function summaryOf(element: XmlElement, shapes: string[]): SectionSummary {
  return {
    element: element.name,
    id: num(element, "id"),
    name: element.attributes.name ?? "",
    shapes,
  };
}

/** 元の属性。同じ断面を参照する部材の間で共有するため複製しない */
export function rawOf(element: XmlElement, label = element.name): RawAttributes {
  return { label, attributes: element.attributes };
}

/**
 * コンクリート強度の決め方（仕様書 3.2 補足(3)）: 部材 → 断面 → 基準の節点が属する階 → StbCommon の順に、
 * 最初に定義のあるものを使う。どこから決めたかも返す。
 */
export function concreteStrength(
  doc: StbDocument,
  member: XmlElement,
  section: XmlElement | undefined,
  nodeId: number | null,
): string | null {
  const own = member.attributes.strength_concrete;
  if (own) return `${own}（部材）`;
  const fromSection = section?.attributes.strength_concrete;
  if (fromSection) return `${fromSection}（断面）`;
  const story = nodeId === null ? undefined : doc.story(doc.nodeStory.get(nodeId));
  if (story?.strengthConcrete) return `${story.strengthConcrete}（階 ${story.name}）`;
  const common = doc.common?.attributes.strength_concrete;
  if (common) return `${common}（建物全体）`;
  return null;
}

function steelMaterial(element: XmlElement): string | null {
  const main = element.attributes.strength_main ?? element.attributes.strength;
  if (!main) return null;
  const web = element.attributes.strength_web;
  return web && web !== main ? `${main}（ウェブ ${web}）` : main;
}

function unique(values: (string | null | undefined)[]): string[] {
  return [...new Set(values.filter((value): value is string => !!value))];
}

/** 鉄骨の形状名から断面を引く。見つからない・描けない形は null と、その理由を notices に記録する */
function steelProfile(doc: StbDocument, name: string | undefined, filled = false): Profile | null {
  if (!name) return null;
  const shape = doc.steel.get(name, filled);
  if (!shape.profile) {
    doc.notices.add(`steel-shape:${name}`, "warning", `鉄骨の形状「${name}」の寸法が分からないため、部材を線で示した`);
    return null;
  }
  if (!shape.element) {
    doc.notices.add("steel-shape-from-name", "info", "StbSecSteel に定義のない鉄骨の形状を、形状名の寸法から描いた");
  }
  return shape.profile;
}

function steelRaw(doc: StbDocument, names: (string | undefined)[]): RawAttributes[] {
  return unique(names).flatMap((name) => {
    const attributes = doc.steel.attributes(name);
    return attributes ? [{ label: `StbSecSteel ${name}`, attributes }] : [];
  });
}

// ---- 柱 ------------------------------------------------------------------

/** RC・SRC 柱のコンクリートの外形（6.2.1）。ふかし厚さを含める（5.2.1） */
function concreteColumnProfile(
  figure: XmlElement | undefined,
  member: XmlElement,
): { profile: Profile; label: string } | null {
  const rectangle =
    child(figure, "StbSecColumnRect") ??
    child(figure, "StbSecColumn_RC_Rect") ??
    child(figure, "StbSecColumn_SRC_Rect");
  if (rectangle) {
    const wx = num(rectangle, "width_X");
    const wy = num(rectangle, "width_Y");
    if (wx <= 0 || wy <= 0) return null;
    const sx = num(member, "thickness_add_start_X");
    const ex = num(member, "thickness_add_end_X");
    const sy = num(member, "thickness_add_start_Y");
    const ey = num(member, "thickness_add_end_Y");
    return {
      profile: single(rect(-wx / 2 - sx, -wy / 2 - sy, wx / 2 + ex, wy / 2 + ey)),
      label: `${formatNumber(wx)}×${formatNumber(wy)}`,
    };
  }
  const round =
    child(figure, "StbSecColumnCircle") ??
    child(figure, "StbSecColumn_RC_Circle") ??
    child(figure, "StbSecColumn_SRC_Circle");
  if (round) {
    const d = num(round, "D");
    if (d <= 0) return null;
    return { profile: solidCircle(d + 2 * num(member, "thickness_add_start_X")), label: `φ${formatNumber(d)}` };
  }
  return null;
}

type Position = "SAME" | "BOTTOM" | "CENTER" | "TOP";

interface ColumnPart {
  pos: Position;
  profile: Profile | null;
  label: string;
}

/**
 * 柱頭・柱脚で違う断面の切り替え位置（下端からの距離）。2.1 は steel_switch_height_*（基準点から）、
 * 2.0 は joint_*（オフセット後の端点から、5.2.1 補足(6)）。どちらもなければ等分する。
 */
function columnSwitches(doc: StbDocument, member: XmlElement, length: number, count: number): number[] {
  const bottom = optionalNum(member, "steel_switch_height_bottom") ?? optionalNum(member, "joint_bottom");
  const topFromEnd = optionalNum(member, "steel_switch_height_top") ?? optionalNum(member, "joint_top");
  const top = topFromEnd === null ? null : length - topFromEnd;
  if (count === 2) {
    const at = bottom ?? top;
    if (at === null || at <= 0 || at >= length) {
      doc.notices.add("column-switch", "info", "柱頭・柱脚で断面の違う柱に切り替え位置がないため、中央で切り替えた");
      return [length / 2];
    }
    return [at];
  }
  let first = bottom;
  let second = top;
  if (first === null || second === null || first <= 0 || second >= length || first >= second) {
    doc.notices.add(
      "column-switch",
      "info",
      "柱頭・中央・柱脚で断面の違う柱に切り替え位置がないため、3 等分して切り替えた",
    );
    first = length / 3;
    second = (length * 2) / 3;
  }
  return [first, second];
}

function partsToSegments(parts: ColumnPart[], switches: number[], length: number): Segment[] {
  const order: Position[] =
    parts.length === 1 ? ["SAME"] : parts.length === 2 ? ["BOTTOM", "TOP"] : ["BOTTOM", "CENTER", "TOP"];
  const stations = [0, ...switches, length];
  const segments: Segment[] = [];
  order.forEach((pos, i) => {
    const part = parts.find((item) => item.pos === pos) ?? parts[i];
    if (!part?.profile) return;
    segments.push({
      from: stations[i] as number,
      to: stations[i + 1] as number,
      start: part.profile,
      end: part.profile,
      label: part.label,
    });
  });
  return segments;
}

/** 柱・間柱の断面（6.2〜6.5）。断面は図心を原点にした (b, h) = (部材の X, Y) の輪郭 */
export function resolveColumnSection(
  doc: StbDocument,
  member: XmlElement,
  structure: string,
  length: number,
  topNodeId: number,
): ResolvedSection {
  const id = num(member, "id_section", Number.NaN);
  const elementName = SECTION_ELEMENTS.column?.[structure];
  const section = elementName ? doc.section(elementName, id) : undefined;
  if (!section) {
    const undefinedSection = doc.section("StbSecUndefined", id);
    if (structure !== "UNDEFINED") {
      doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    }
    return {
      summary: undefinedSection ? summaryOf(undefinedSection, []) : null,
      segments: [],
      materials: [],
      raw: undefinedSection ? [rawOf(undefinedSection)] : [],
    };
  }

  if (structure === "RC" || structure === "SRC") {
    const figure = child(section, structure === "RC" ? "StbSecFigureColumn_RC" : "StbSecFigureColumn_SRC");
    const concrete = concreteColumnProfile(figure, member);
    const steelNames = structure === "SRC" ? srcSteelNames(child(section, "StbSecSteelFigureColumn_SRC")) : [];
    const materials = unique([
      concrete ? `コンクリート ${concreteStrength(doc, member, section, topNodeId) ?? "（指定なし）"}` : null,
      ...srcSteelMaterials(child(section, "StbSecSteelFigureColumn_SRC")).map((value) => `鉄骨 ${value}`),
    ]);
    if (!concrete) doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
    const shapes = concrete ? [concrete.label, ...steelNames.map((name) => `鉄骨 ${name}`)] : steelNames;
    if (structure === "SRC")
      doc.notices.add("src-steel", "info", "SRC 部材は外形のコンクリートを描き、内部の鉄骨は描かない");
    return {
      summary: summaryOf(section, shapes),
      segments: concrete
        ? [{ from: 0, to: length, start: concrete.profile, end: concrete.profile, label: concrete.label }]
        : [],
      materials,
      raw: [rawOf(section), ...(figure?.children.map((item) => rawOf(item)) ?? []), ...steelRaw(doc, steelNames)],
    };
  }

  // S・CFT
  const cft = structure === "CFT";
  const figure = child(section, cft ? "StbSecSteelFigureColumn_CFT" : "StbSecSteelFigureColumn_S");
  const reference = bool(section, "isReferenceDirection", true);
  const items = figure?.children ?? [];
  const parts: ColumnPart[] = items.map((item) => {
    const name = item.attributes.shape;
    let profile = steelProfile(doc, name, cft);
    if (profile && !reference) profile = rotateProfile90(profile);
    const pos = (item.attributes.pos as Position | undefined) ?? "SAME";
    return { pos, profile, label: name ?? "" };
  });
  const switches = parts.length > 1 ? columnSwitches(doc, member, length, parts.length) : [];
  const segments = parts.length > 0 ? partsToSegments(parts, switches, length) : [];
  if (segments.length === 0) doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
  const materials = unique(items.map(steelMaterial)).map((value) => `${cft ? "鋼管" : "鋼材"} ${value}`);
  if (cft) {
    const strength = concreteStrength(doc, member, section, topNodeId);
    if (strength) materials.push(`充填コンクリート ${strength}`);
  }
  const labels = parts.map((part) => (part.pos === "SAME" ? part.label : `${POSITION_LABELS[part.pos]} ${part.label}`));
  return {
    summary: summaryOf(section, labels),
    segments,
    materials,
    raw: [
      rawOf(section),
      ...items.map((item) => rawOf(item)),
      ...steelRaw(
        doc,
        parts.map((part) => part.label),
      ),
    ],
  };
}

const POSITION_LABELS: Record<string, string> = {
  SAME: "",
  BOTTOM: "柱脚",
  CENTER: "中央",
  TOP: "柱頭",
  START: "始端",
  END: "終端",
  HAUNCH_S: "ハンチ（始端）",
  HAUNCH_E: "ハンチ（終端）",
};

function srcSteelNames(figure: XmlElement | undefined): string[] {
  const names: string[] = [];
  const visit = (element: XmlElement) => {
    for (const [key, value] of Object.entries(element.attributes)) if (key.startsWith("shape")) names.push(value);
    for (const item of element.children) visit(item);
  };
  if (figure) visit(figure);
  return unique(names);
}

function srcSteelMaterials(figure: XmlElement | undefined): string[] {
  const values: string[] = [];
  const visit = (element: XmlElement) => {
    for (const [key, value] of Object.entries(element.attributes)) {
      if (key === "strength_main" || key === "strength" || key.startsWith("strength_main_")) values.push(value);
    }
    for (const item of element.children) visit(item);
  };
  if (figure) visit(figure);
  return unique(values);
}

// ---- 梁 ------------------------------------------------------------------

const topCenteredCache = new WeakMap<Profile, Profile>();

/** 梁の断面を天端の幅中心が原点になるように移す（梁の基準点、5.4.1 補足(3)）。同じ断面の結果は共有する */
export function topCentered(profile: Profile): Profile {
  const cached = topCenteredCache.get(profile);
  if (cached) return cached;
  const { maxH } = profileBounds(profile);
  const moved = translateProfile(profile, 0, -maxH);
  topCenteredCache.set(profile, moved);
  return moved;
}

/** RC 梁の外形（天端の幅中心が原点）。ふかし厚さは左（+b）・右（−b）・上・下に足す（5.4.1） */
function concreteBeamProfile(member: XmlElement, width: number, depth: number, horizontal = 0, vertical = 0): Profile {
  const left = num(member, "thickness_add_left");
  const right = num(member, "thickness_add_right");
  const top = num(member, "thickness_add_top");
  const bottom = num(member, "thickness_add_bottom");
  return single(
    rect(-width / 2 - right + horizontal, -depth - bottom + vertical, width / 2 + left + horizontal, top + vertical),
  );
}

interface OrderedItem {
  order: number;
  start: Profile;
  end: Profile;
  label: string;
}

/**
 * 2.1 の区間（order）を、部材の切り替え位置（StbGirderConcreteSwitch など、5.4.1.2〜5.4.1.4）で並べる。
 * 切り替え位置の distance は、その order の区間が終わる位置。最後の区間は終端まで。
 */
function orderedSegments(doc: StbDocument, items: OrderedItem[], switches: XmlElement[], length: number): Segment[] {
  const sorted = [...items].sort((a, b) => a.order - b.order);
  const seen = new Set<number>();
  const distinct = sorted.filter((item) => {
    if (seen.has(item.order)) {
      doc.notices.add("duplicate-order", "warning", "同じ order の断面が複数あるため、最初のものを使った");
      return false;
    }
    seen.add(item.order);
    return true;
  });
  const ends = new Map<number, number>();
  for (const item of switches) ends.set(num(item, "order"), num(item, "distance"));
  const segments: Segment[] = [];
  let from = 0;
  distinct.forEach((item, i) => {
    const last = i === distinct.length - 1;
    let to = last ? length : (ends.get(item.order) ?? Number.NaN);
    if (!Number.isFinite(to) || to <= from || to > length) {
      if (!last) doc.notices.add("missing-switch", "info", "区間の切り替え位置がない梁は、残りの長さを等分して描いた");
      to = last ? length : from + (length - from) / (distinct.length - i);
    }
    segments.push({ from, to, start: item.start, end: item.end, label: item.label });
    from = to;
  });
  return segments;
}

function rcOrderedItems(figures: XmlElement[], member: XmlElement): OrderedItem[] {
  return figures.flatMap((figure) => {
    const order = num(figure, "order", 1);
    const straight = child(figure, "StbSecBeamStraight");
    if (straight) {
      const width = num(straight, "width");
      const depth = num(straight, "depth");
      const profile = concreteBeamProfile(
        member,
        width,
        depth,
        num(straight, "horizontal_offset"),
        num(straight, "vertical_offset"),
      );
      return [{ order, start: profile, end: profile, label: `${formatNumber(width)}×${formatNumber(depth)}` }];
    }
    const taper = child(figure, "StbSecBeamTaper");
    if (taper) {
      const sw = num(taper, "start_width");
      const sd = num(taper, "start_depth");
      const ew = num(taper, "end_width");
      const ed = num(taper, "end_depth");
      return [
        {
          order,
          start: concreteBeamProfile(
            member,
            sw,
            sd,
            num(taper, "start_horizontal_offset"),
            num(taper, "start_vertical_offset"),
          ),
          end: concreteBeamProfile(
            member,
            ew,
            ed,
            num(taper, "end_horizontal_offset"),
            num(taper, "end_vertical_offset"),
          ),
          label: `${formatNumber(sw)}×${formatNumber(sd)}→${formatNumber(ew)}×${formatNumber(ed)}`,
        },
      ];
    }
    return [];
  });
}

function steelOrderedItems(doc: StbDocument, shapes: XmlElement[]): OrderedItem[] {
  return shapes.flatMap((shape) => {
    const order = num(shape, "order", 1);
    const straight = child(shape, "StbSecSteelBeamStraight");
    if (straight) {
      const profile = steelProfile(doc, straight.attributes.shape);
      if (!profile) return [];
      const placed = translateProfile(
        topCentered(profile),
        num(straight, "horizontal_offset"),
        num(straight, "vertical_offset"),
      );
      return [{ order, start: placed, end: placed, label: straight.attributes.shape ?? "" }];
    }
    const taper = child(shape, "StbSecSteelBeamTaper");
    if (taper) {
      const a = steelProfile(doc, taper.attributes.start_shape);
      const b = steelProfile(doc, taper.attributes.end_shape);
      if (!a || !b) return [];
      const start = translateProfile(
        topCentered(a),
        num(taper, "start_horizontal_offset"),
        num(taper, "start_vertical_offset"),
      );
      const end = translateProfile(
        topCentered(b),
        num(taper, "end_horizontal_offset"),
        num(taper, "end_vertical_offset"),
      );
      const label = `${taper.attributes.start_shape}→${taper.attributes.end_shape}`;
      if (!sameTopology(start, end)) {
        doc.notices.add("taper-topology", "info", "形の違う鉄骨をつなぐテーパーは、始端の形で描いた");
        return [{ order, start, end: start, label }];
      }
      return [{ order, start, end, label }];
    }
    return [];
  });
}

interface Positioned {
  pos: string;
  profile: Profile;
  label: string;
}

function byPos(items: Positioned[], pos: string): Positioned | undefined {
  return items.find((item) => item.pos === pos);
}

/** 2.0 の区間を切る（5.4.1 補足(9)）。ハンチは基準点から、継手は柱の中心（節点）からの距離 */
function legacyBeamSegments(
  doc: StbDocument,
  member: XmlElement,
  items: Positioned[],
  kind: "straight" | "taper" | "joint" | "haunch" | "five",
  length: number,
  nodeShift: { start: number; end: number },
): Segment[] {
  const seg = (from: number, to: number, a: Positioned, b: Positioned = a): Segment | null =>
    to - from > 1e-6
      ? {
          from,
          to,
          start: a.profile,
          end: sameTopology(a.profile, b.profile) ? b.profile : a.profile,
          label: a === b ? a.label : `${a.label}→${b.label}`,
        }
      : null;
  const center = byPos(items, "CENTER") ?? items[0];
  if (!center) return [];
  const startItem = byPos(items, "START") ?? center;
  const endItem = byPos(items, "END") ?? center;
  const hs = optionalNum(member, "haunch_start") ?? 0;
  const he = optionalNum(member, "haunch_end") ?? 0;
  const slopeStart = (member.attributes.kind_haunch_start ?? "SLOPE") !== "DROP";
  const slopeEnd = (member.attributes.kind_haunch_end ?? "SLOPE") !== "DROP";
  const rawJs = optionalNum(member, "joint_start");
  const rawJe = optionalNum(member, "joint_end");
  const js = rawJs === null ? null : rawJs - nodeShift.start;
  const je = rawJe === null ? null : rawJe - nodeShift.end;
  const out: (Segment | null)[] = [];

  if (kind === "straight") return [seg(0, length, center)].filter((s): s is Segment => !!s);
  if (kind === "taper") return [seg(0, length, startItem, endItem)].filter((s): s is Segment => !!s);

  if (kind === "haunch") {
    if ((startItem !== center && hs <= 0) || (endItem !== center && he <= 0)) {
      doc.notices.add("missing-haunch", "info", "ハンチの位置がない梁は、中央の断面で描いた");
    }
    const a = startItem !== center && hs > 0 ? hs : 0;
    const b = endItem !== center && he > 0 ? length - he : length;
    out.push(a > 0 ? seg(0, a, startItem, slopeStart ? center : startItem) : null);
    out.push(seg(a, b, center));
    out.push(b < length ? seg(b, length, slopeEnd ? center : endItem, endItem) : null);
    return out.filter((s): s is Segment => !!s);
  }

  if (kind === "joint") {
    let a = startItem !== center ? (js ?? Number.NaN) : 0;
    let b = endItem !== center ? length - (je ?? Number.NaN) : length;
    if (!Number.isFinite(a) || !Number.isFinite(b) || a < 0 || b > length || a >= b) {
      doc.notices.add("missing-joint", "info", "継手の位置がない梁は、3 等分して断面を切り替えた");
      a = startItem !== center ? length / 3 : 0;
      b = endItem !== center ? (length * 2) / 3 : length;
    }
    out.push(seg(0, a, startItem), seg(a, b, center), seg(b, length, endItem));
    return out.filter((s): s is Segment => !!s);
  }

  // 5 種類（6.7.6）。HAUNCH_S・HAUNCH_E がないときは START・END と同じ
  const haunchS = byPos(items, "HAUNCH_S") ?? startItem;
  const haunchE = byPos(items, "HAUNCH_E") ?? endItem;
  const jointS = js ?? hs;
  const jointE = je ?? he;
  if (jointS >= hs) {
    // 5-1: 継手がハンチ位置より中央側
    out.push(seg(0, hs, startItem, slopeStart ? haunchS : startItem), seg(hs, jointS, haunchS));
  } else {
    // 5-2: 継手がハンチ位置より端点側
    out.push(seg(0, jointS, startItem), seg(jointS, hs, haunchS, slopeStart ? center : haunchS));
  }
  const left = Math.max(hs, jointS);
  const right = length - Math.max(he, jointE);
  out.push(seg(left, right, center));
  if (jointE >= he) {
    out.push(seg(right, length - he, haunchE), seg(length - he, length, slopeEnd ? haunchE : endItem, endItem));
  } else {
    out.push(seg(right, length - jointE, slopeEnd ? center : haunchE, haunchE), seg(length - jointE, length, endItem));
  }
  return out.filter((s): s is Segment => !!s);
}

/**
 * 大梁・小梁の断面（6.6〜6.8）。断面は天端の幅中心を原点にした (b, h) = (部材の Y, Z) の輪郭。
 * nodeShift は、節点の位置から基準点までの材軸方向の距離（2.0 の継手位置を基準点からの距離に直す）。
 */
export function resolveBeamSection(
  doc: StbDocument,
  member: XmlElement,
  structure: string,
  length: number,
  startNodeId: number,
  nodeShift: { start: number; end: number },
): ResolvedSection {
  const id = num(member, "id_section", Number.NaN);
  const elementName = SECTION_ELEMENTS.beam?.[structure];
  const section = elementName ? doc.section(elementName, id) : undefined;
  if (!section) {
    const undefinedSection = doc.section("StbSecUndefined", id);
    if (structure !== "UNDEFINED") doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return {
      summary: undefinedSection ? summaryOf(undefinedSection, []) : null,
      segments: [],
      materials: [],
      raw: undefinedSection ? [rawOf(undefinedSection)] : [],
    };
  }
  let segments: Segment[] = [];
  const raw: RawAttributes[] = [rawOf(section)];
  const materials: string[] = [];
  const memberEl = member.name;
  const switchName = (kind: "Concrete" | "Steel") => `${memberEl}${kind}Switch`;

  if (structure === "RC" || structure === "SRC") {
    const figureName = structure === "RC" ? "StbSecFigureBeam_RC" : "StbSecFigureBeam_SRC";
    const figures = children(section, figureName);
    for (const figure of figures) raw.push(...figure.children.map((item) => rawOf(item)));
    if (doc.is21 || figures.some((figure) => figure.attributes.order !== undefined)) {
      segments = orderedSegments(
        doc,
        rcOrderedItems(figures, member),
        children(member, switchName("Concrete")),
        length,
      );
    } else {
      const figure = figures[0];
      const prefix = structure === "RC" ? "StbSecBeam_RC_" : "StbSecBeam_SRC_";
      const items: Positioned[] = (figure?.children ?? []).map((item) => {
        const width = num(item, "width");
        const depth = num(item, "depth");
        return {
          pos: item.attributes.pos ?? "CENTER",
          profile: concreteBeamProfile(member, width, depth),
          label: `${formatNumber(width)}×${formatNumber(depth)}`,
        };
      });
      const kind = child(figure, `${prefix}Taper`) ? "taper" : child(figure, `${prefix}Haunch`) ? "haunch" : "straight";
      segments = legacyBeamSegments(doc, member, items, kind, length, nodeShift);
    }
    materials.push(`コンクリート ${concreteStrength(doc, member, section, startNodeId) ?? "（指定なし）"}`);
    if (structure === "SRC") {
      const steel = child(section, "StbSecSteelFigureBeam_SRC");
      const names = srcSteelNames(steel);
      materials.push(...srcSteelMaterials(steel).map((value) => `鉄骨 ${value}`));
      raw.push(...steelRaw(doc, names));
      doc.notices.add("src-steel", "info", "SRC 部材は外形のコンクリートを描き、内部の鉄骨は描かない");
    }
  } else {
    const figure = child(section, "StbSecSteelFigureBeam_S");
    const shapes = children(figure, "StbSecSteelBeam_S_Shape");
    const names: (string | undefined)[] = [];
    if (shapes.length > 0) {
      for (const shape of shapes) {
        for (const item of shape.children) {
          raw.push(rawOf(item));
          names.push(item.attributes.shape, item.attributes.start_shape, item.attributes.end_shape);
          const material = steelMaterial(item);
          if (material) materials.push(`鋼材 ${material}`);
        }
      }
      segments = orderedSegments(doc, steelOrderedItems(doc, shapes), children(member, switchName("Steel")), length);
      if (child(figure, "StbSecSteelBeamWidening")) {
        doc.notices.add("widening", "info", "鉄骨梁の端部の拡幅（StbSecSteelBeamWidening）は描かない");
      }
    } else {
      const items: Positioned[] = [];
      let kind: "straight" | "taper" | "joint" | "haunch" | "five" = "straight";
      for (const item of figure?.children ?? []) {
        raw.push(rawOf(item));
        names.push(item.attributes.shape);
        const material = steelMaterial(item);
        if (material) materials.push(`鋼材 ${material}`);
        const profile = steelProfile(doc, item.attributes.shape);
        if (!profile) continue;
        if (item.name.endsWith("_Taper")) kind = "taper";
        else if (item.name.endsWith("_Joint")) kind = "joint";
        else if (item.name.endsWith("_Haunch")) kind = "haunch";
        else if (item.name.endsWith("_FiveTypes")) kind = "five";
        items.push({
          pos: item.attributes.pos ?? "CENTER",
          profile: topCentered(profile),
          label: item.attributes.shape ?? "",
        });
      }
      segments = legacyBeamSegments(doc, member, items, kind, length, nodeShift);
    }
    raw.push(...steelRaw(doc, names));
  }

  // 外端・内端の読み替え（5.4.1 補足(5)）。始端が内端なら、断面の始端・終端を入れ替える
  if (bool(section, "isOutin", false)) {
    const io = member.attributes.section_io_start;
    const ioEnd = member.attributes.section_io_end;
    if (io === "IN" || ioEnd === "OUT") segments = reverseSegments(segments, length);
    else if (!io && !ioEnd) {
      doc.notices.add("outin", "info", "外端・内端の指定がない梁は、断面の始端を部材の始端に置いた");
    }
  }
  if (segments.length === 0) doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
  const labels =
    segments.length > 1
      ? segments.map((s) => `${formatNumber(s.from)}〜${formatNumber(s.to)} mm: ${s.label}`)
      : segments.map((s) => s.label);
  return { summary: summaryOf(section, labels), segments, materials: unique(materials), raw };
}

function reverseSegments(segments: Segment[], length: number): Segment[] {
  return [...segments]
    .reverse()
    .map((s) => ({ from: length - s.to, to: length - s.from, start: s.end, end: s.start, label: s.label }));
}

// ---- ブレース ------------------------------------------------------------

/** ブレースの断面（6.9）。狙い点は外形を包絡する長方形の中心（5.6.1 補足(2)）で、(b, h) = (部材の Y, Z) */
export function resolveBraceSection(
  doc: StbDocument,
  member: XmlElement,
  length: number,
  bottomAtStart: boolean,
): ResolvedSection {
  const id = num(member, "id_section", Number.NaN);
  const section = doc.section("StbSecBrace_S", id);
  if (!section) {
    doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return { summary: null, segments: [], materials: [], raw: [] };
  }
  const figure = child(section, "StbSecSteelFigureBrace_S");
  const items = figure?.children ?? [];
  const parts: ColumnPart[] = items.map((item) => ({
    pos: (item.attributes.pos as Position | undefined) ?? "SAME",
    profile: steelProfile(doc, item.attributes.shape),
    label: item.attributes.shape ?? "",
  }));
  let segments: Segment[] = [];
  if (parts.length === 1) segments = partsToSegments(parts, [], length);
  else if (parts.length > 1) {
    doc.notices.add("brace-switch", "info", "頭部・脚部で断面の違うブレースは、長さを等分して断面を切り替えた");
    const switches = parts.length === 2 ? [length / 2] : [length / 3, (length * 2) / 3];
    segments = partsToSegments(parts, switches, length);
    if (!bottomAtStart) segments = reverseSegments(segments, length);
  }
  if (segments.length === 0) doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
  return {
    summary: summaryOf(
      section,
      parts.map((part) =>
        part.pos === "SAME" ? part.label : `${POSITION_LABELS[part.pos] ?? part.pos} ${part.label}`,
      ),
    ),
    segments,
    materials: unique(items.map(steelMaterial)).map((value) => `鋼材 ${value}`),
    raw: [
      rawOf(section),
      ...items.map((item) => rawOf(item)),
      ...steelRaw(
        doc,
        parts.map((part) => part.label),
      ),
    ],
  };
}
