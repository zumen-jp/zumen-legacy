import { type Profile, polygonCentroid } from "../geometry.js";
import type { SectionSummary, Vec2 } from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { child, num, type StbDocument } from "./document.js";
import { rect, single, solidCircle } from "./profiles.js";
import { concreteStrength, formatNumber, type RawAttributes, rawOf, summaryOf } from "./sections.js";

export interface FootingSection {
  summary: SectionSummary | null;
  /** 平面の外形（図心が原点、回転前） */
  outline: Vec2[];
  depth: number;
  /** 矩形テーパー（6.18.1.2）。外周の厚さ tip から中央の厚さ depth まで上面を傾ける */
  tip: number | null;
  materials: string[];
  raw: RawAttributes[];
}

function centered(points: Vec2[]): Vec2[] {
  const { centroid } = polygonCentroid(points);
  return points.map(([x, y]) => [x - centroid[0], y - centroid[1]] as const);
}

/** フーチングの断面（6.18.1）。基準点は図心（5.17.1 補足(1)） */
export function resolveFootingSection(doc: StbDocument, member: XmlElement): FootingSection | null {
  const section = doc.section("StbSecFoundation_RC", num(member, "id_section", Number.NaN));
  if (!section) {
    doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return null;
  }
  const figure = child(section, "StbSecFigureFoundation_RC");
  const shape = figure?.children[0];
  if (!shape) {
    doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
    return null;
  }
  const n = (name: string) => num(shape, name);
  let outline: Vec2[] = [];
  let depth = n("depth");
  let tip: number | null = null;
  let label = "";
  switch (shape.name) {
    case "StbSecFoundation_RC_Rect": {
      const sx = num(member, "thickness_add_start_X");
      const ex = num(member, "thickness_add_end_X");
      const sy = num(member, "thickness_add_start_Y");
      const ey = num(member, "thickness_add_end_Y");
      outline = rect(-n("width_X") / 2 - sx, -n("width_Y") / 2 - sy, n("width_X") / 2 + ex, n("width_Y") / 2 + ey);
      label = `${formatNumber(n("width_X"))}×${formatNumber(n("width_Y"))}×${formatNumber(depth)}`;
      break;
    }
    case "StbSecFoundation_RC_TaperedRect":
      outline = rect(-n("width_X") / 2, -n("width_Y") / 2, n("width_X") / 2, n("width_Y") / 2);
      depth = n("depth_base");
      tip = n("depth_tip");
      label = `${formatNumber(n("width_X"))}×${formatNumber(n("width_Y"))}×${formatNumber(depth)}（先端 ${formatNumber(tip)}）`;
      doc.notices.add(
        "tapered-footing",
        "info",
        "矩形テーパーのフーチングは、上面の平らな範囲を外形の半分として描いた",
      );
      break;
    case "StbSecFoundation_RC_Triangle": {
      // 直角三角形。直角を原点側に置き、斜辺側の 2 つの角を面取りする（6.18.1.3）
      const wx = n("width_X");
      const wy = n("width_Y");
      const cx = n("width_chamfer_X");
      const cy = n("width_chamfer_Y");
      outline = centered([
        [0, 0],
        [wx, 0],
        [wx, cy],
        [cx, wy],
        [0, wy],
      ]);
      label = `直角三角形 ${formatNumber(wx)}×${formatNumber(wy)}×${formatNumber(depth)}`;
      break;
    }
    case "StbSecFoundation_RC_EquiTriangle": {
      // 正三角形の 3 つの角を面取りした六角形。底辺幅は面取り後の辺の長さ（6.18.1.4）
      const base = n("width_base");
      const chamfer = n("width_chamfer");
      const side = base + 2 * chamfer;
      const corners: Vec2[] = [
        [-side / 2, 0],
        [side / 2, 0],
        [0, (side * Math.sqrt(3)) / 2],
      ];
      const points: Vec2[] = [];
      for (let i = 0; i < 3; i++) {
        const a = corners[i] as Vec2;
        const b = corners[(i + 1) % 3] as Vec2;
        const t = chamfer / side;
        points.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        points.push([b[0] + (a[0] - b[0]) * t, b[1] + (a[1] - b[1]) * t]);
      }
      outline = centered(points);
      label = `正三角形 底辺 ${formatNumber(base)}×${formatNumber(depth)}`;
      break;
    }
    case "StbSecFoundation_RC_Octagon": {
      // 四隅を面取りした八角形。(1) 左下・(2) 右下・(3) 右上・(4) 左上（6.18.1.5）
      const wx = n("width_X");
      const wy = n("width_Y");
      const c = (k: number, axis: "X" | "Y") => n(`width_chamfer${k}_${axis}`);
      outline = centered([
        [c(1, "X"), 0],
        [wx - c(2, "X"), 0],
        [wx, c(2, "Y")],
        [wx, wy - c(3, "Y")],
        [wx - c(3, "X"), wy],
        [c(4, "X"), wy],
        [0, wy - c(4, "Y")],
        [0, c(1, "Y")],
      ]);
      label = `八角形 ${formatNumber(wx)}×${formatNumber(wy)}×${formatNumber(depth)}`;
      break;
    }
    default:
      doc.notices.add(
        `footing-shape:${shape.name}`,
        "warning",
        `フーチングの形（${shape.name}）を描けないため線で示した`,
      );
      return null;
  }
  // 面取り 0 で重なった頂点を除く
  outline = outline.filter((p, i) => {
    const q = outline[(i + 1) % outline.length] as Vec2;
    return Math.hypot(p[0] - q[0], p[1] - q[1]) > 1e-6;
  });
  return {
    summary: summaryOf(section, [label]),
    outline,
    depth: depth + num(member, "thickness_add_top") + num(member, "thickness_add_bottom"),
    tip,
    materials: [`コンクリート ${concreteStrength(doc, member, section, num(member, "id_node")) ?? "（指定なし）"}`],
    raw: [rawOf(section), rawOf(shape)],
  };
}

/**
 * 布基礎の底板の断面（6.18.1.6）。基準点は天端の幅中心（5.18.1 補足(3)）で、(b, h) = (部材の Y, Z)。
 * 根元厚さ・先端厚さは脚部の付く側から先端へ上面を傾ける。
 */
export function resolveStripFootingSection(
  doc: StbDocument,
  member: XmlElement,
): { profile: Profile; summary: SectionSummary; materials: string[]; raw: RawAttributes[] } | null {
  const section = doc.section("StbSecFoundation_RC", num(member, "id_section", Number.NaN));
  const shape = child(child(section, "StbSecFigureFoundation_RC"), "StbSecFoundation_RC_Continuous");
  if (!section || !shape) {
    doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return null;
  }
  const w = num(shape, "width") / 2;
  const base = num(shape, "depth_base");
  const drop = Math.max(0, base - num(shape, "depth_tip"));
  const type = shape.attributes.type ?? "REVERSE_T";
  // 進行方向の右は −b。RIGHT_L は右へ、LEFT_L は左へ脚部が出る
  const outer =
    type === "RIGHT_L"
      ? [[w, -base] as const, [w, 0] as const, [-w, -drop] as const, [-w, -base] as const]
      : type === "LEFT_L"
        ? [[w, -base] as const, [w, -drop] as const, [-w, 0] as const, [-w, -base] as const]
        : [[w, -base] as const, [w, -drop] as const, [0, 0] as const, [-w, -drop] as const, [-w, -base] as const];
  return {
    profile: single(outer.map(([b, h]) => [b, h] as const)),
    summary: summaryOf(section, [`連続基礎 幅 ${formatNumber(w * 2)} 厚さ ${formatNumber(base)}（${type}）`]),
    materials: [
      `コンクリート ${concreteStrength(doc, member, section, num(member, "id_node_start")) ?? "（指定なし）"}`,
    ],
    raw: [rawOf(section), rawOf(shape)],
  };
}

/** パラペットの断面（6.22.1）。下端の中央が原点。アゴは direction の側（R は −b、L は +b）に付ける */
export function resolveParapetSection(
  doc: StbDocument,
  member: XmlElement,
): { profile: Profile; summary: SectionSummary; materials: string[]; raw: RawAttributes[] } | null {
  const section = doc.section("StbSecParapet_RC", num(member, "id_section", Number.NaN));
  const shape = child(section, "StbSecFigureParapet_RC")?.children[0];
  if (!section || !shape) {
    doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return null;
  }
  const t = num(shape, "t_T");
  const height = num(shape, "depth_H");
  const t1 = num(shape, "t_T1");
  const h1 = num(shape, "depth_H1");
  const h2 = num(shape, "depth_H2");
  const h3 = shape.name === "StbSecParapet_RC_TypeT" ? num(shape, "depth_H3") : 0;
  const side = member.attributes.direction === "L" ? 1 : -1;
  let outer: [number, number][];
  if (shape.name === "StbSecParapet_RC_TypeI" || t1 <= 0) {
    outer = [
      [-t / 2, 0],
      [t / 2, 0],
      [t / 2, height],
      [-t / 2, height],
    ];
  } else {
    // アゴを +b 側に作ってから direction で向きを決める
    const top = height - h3;
    outer = [
      [-t / 2, 0],
      [t / 2, 0],
      [t / 2, top - h2 - h1],
      [t / 2 + t1, top - h2 - h1],
      [t / 2 + t1, top - h2],
      [t / 2, top],
      [t / 2, height],
      [-t / 2, height],
    ];
    if (side < 0) outer = outer.map(([b, h]) => [-b, h] as [number, number]).reverse();
  }
  const kind = shape.name.replace("StbSecParapet_RC_Type", "");
  return {
    profile: single(outer),
    summary: summaryOf(section, [`${kind} 型 t=${formatNumber(t)} H=${formatNumber(height)}`]),
    materials: [
      `コンクリート ${concreteStrength(doc, member, section, num(member, "id_node_start")) ?? "（指定なし）"}`,
    ],
    raw: [rawOf(section), rawOf(shape)],
  };
}

/** 杭の区間。top は杭頭からの深さ（mm）、径は上端・下端 */
export interface PileSegment {
  top: number;
  length: number;
  diameterTop: number;
  diameterBottom: number;
  /** 鋼管・既製杭の肉厚（中空で描く）。0 は中実 */
  thickness: number;
  label: string;
}

const DEGREE = Math.PI / 180;

/** 杭の断面（6.19〜6.21）。場所打ち杭は杭全長 length_all、鋼管杭・既製杭は id_order 順の継ぎ杭の長さで深さを決める */
export function resolvePileSection(
  doc: StbDocument,
  member: XmlElement,
): { segments: PileSegment[]; summary: SectionSummary | null; materials: string[]; raw: RawAttributes[] } {
  const structure = member.attributes.kind_structure ?? "RC";
  const id = num(member, "id_section", Number.NaN);
  const elementName =
    structure === "S"
      ? "StbSecPile_S"
      : structure === "PC"
        ? doc.is21
          ? "StbSecPilePrecast"
          : "StbSecPileProduct"
        : "StbSecPile_RC";
  const section = doc.section(elementName, id);
  if (!section) {
    doc.notices.add("missing-section", "warning", "断面が見つからない部材を線で示した");
    return { segments: [], summary: null, materials: [], raw: [] };
  }
  const raw: RawAttributes[] = [rawOf(section)];
  const segments: PileSegment[] = [];
  if (structure === "RC") {
    const conventional = child(section, "StbSecPile_RC_Conventional");
    const figure = child(conventional, "StbSecFigurePile_RC_Conventional") ?? child(section, "StbSecFigurePile_RC");
    const shape = figure?.children[0];
    const total = num(member, "length_all");
    if (!shape || total <= 0) {
      if (child(section, "StbSecPile_RC_Certified")) {
        doc.notices.add("pile-certified", "info", "認定工法の場所打ち杭は形状を持たないため線で示した");
      } else doc.notices.add("missing-figure", "warning", "断面の形状がない部材を線で示した");
      return { segments, summary: summaryOf(section, []), materials: [], raw };
    }
    raw.push(rawOf(shape));
    const axial = num(shape, "D_axial", num(shape, "D"));
    const headD = num(shape, "D_extended_top");
    const footD = num(shape, "D_extended_foot");
    const head = headD > 0 ? Math.max(num(member, "length_head"), 0) : 0;
    const headTaper =
      headD > axial ? (headD - axial) / 2 / Math.tan(Math.max(num(shape, "angle_extended_top_taper"), 1) * DEGREE) : 0;
    const foot = footD > 0 ? num(shape, "length_extended_foot") : 0;
    const footTaper =
      footD > axial ? (footD - axial) / 2 / Math.tan(Math.max(num(shape, "angle_extended_foot_taper"), 1) * DEGREE) : 0;
    let depth = 0;
    const push = (length: number, d0: number, d1: number, label: string) => {
      if (length <= 0) return;
      segments.push({ top: depth, length, diameterTop: d0, diameterBottom: d1, thickness: 0, label });
      depth += length;
    };
    push(head, headD, headD, `拡頭 φ${formatNumber(headD)}`);
    push(headTaper, headD, axial, "拡頭テーパー");
    push(total - depth - foot - footTaper, axial, axial, `軸部 φ${formatNumber(axial)}`);
    push(footTaper, axial, footD, "拡底テーパー");
    push(
      total - depth,
      footD || axial,
      footD || axial,
      footD ? `拡底 φ${formatNumber(footD)}` : `軸部 φ${formatNumber(axial)}`,
    );
    const materials = [
      `コンクリート ${concreteStrength(doc, member, conventional ?? section, null) ?? "（指定なし）"}`,
    ];
    const label =
      segments.length === 1 ? `φ${formatNumber(axial)} L=${formatNumber(total)}` : `L=${formatNumber(total)}`;
    return { segments, summary: summaryOf(section, [label, ...segments.map((s) => s.label)]), materials, raw };
  }
  // 鋼管杭・既製杭は id_order 順に上から継ぐ
  const holder =
    structure === "S"
      ? (child(child(section, "StbSecPile_S_Conventional"), "StbSecFigurePile_S") ??
        child(section, "StbSecFigurePile_S"))
      : (child(child(section, "StbSecPilePrecastConventional"), "StbSecFigurePilePrecast") ??
        child(section, "StbSecFigurePileProduct"));
  const items = [...(holder?.children ?? [])].sort((a, b) => num(a, "id_order") - num(b, "id_order"));
  let depth = 0;
  const materials: string[] = [];
  for (const item of items) {
    raw.push(rawOf(item));
    const length = num(item, "length_pile");
    const d1 = num(item, "D1", num(item, "D"));
    const d2 = num(item, "D2", d1);
    const t = num(item, "t", num(item, "t1", num(item, "ts", 0)));
    if (length <= 0 || d1 <= 0) {
      doc.notices.add("pile-product", "info", "寸法を持たない製品指定の杭の区間は描かない");
      continue;
    }
    // 回転貫入杭・テーパーは上端 D1、下端 D2 とする
    segments.push({
      top: depth,
      length,
      diameterTop: d1,
      diameterBottom: d2,
      thickness: structure === "S" ? t : 0,
      label: `${item.name.replace(/^StbSecPile(Precast|Product)?_?/, "")} φ${formatNumber(d1)}${d2 !== d1 ? `→${formatNumber(d2)}` : ""} L=${formatNumber(length)}`,
    });
    depth += length;
    const strength = item.attributes.strength ?? item.attributes.strength_concrete;
    if (strength) materials.push(strength);
  }
  return {
    segments,
    summary: summaryOf(
      section,
      segments.map((s) => s.label),
    ),
    materials: [...new Set(materials)],
    raw,
  };
}

/** 基礎柱・根巻柱の断面（StbSecColumn_RC、5.20.1 補足(5)） */
export function foundationColumnProfile(
  doc: StbDocument,
  id: number,
): { profile: Profile; label: string; section: XmlElement } | null {
  const section = doc.section("StbSecColumn_RC", id);
  const figure = child(section, "StbSecFigureColumn_RC");
  const rectangle = child(figure, "StbSecColumnRect") ?? child(figure, "StbSecColumn_RC_Rect");
  if (section && rectangle) {
    const wx = num(rectangle, "width_X");
    const wy = num(rectangle, "width_Y");
    return {
      profile: single(rect(-wx / 2, -wy / 2, wx / 2, wy / 2)),
      label: `${formatNumber(wx)}×${formatNumber(wy)}`,
      section,
    };
  }
  const round = child(figure, "StbSecColumnCircle") ?? child(figure, "StbSecColumn_RC_Circle");
  if (section && round) {
    const d = num(round, "D");
    return { profile: solidCircle(d), label: `φ${formatNumber(d)}`, section };
  }
  return null;
}
