import type { SectionSummary } from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { child, num, type StbDocument } from "./document.js";
import { concreteStrength, formatNumber, type RawAttributes, rawOf, summaryOf } from "./sections.js";

export interface PanelSection {
  summary: SectionSummary | null;
  /** 厚さ（mm）。荷重用で厚さを持たないときは 0 */
  thickness: number;
  /** 壁のテーパー（下端・上端の厚さ） */
  taper: { bottom: number; top: number } | null;
  /** 荷重だけを受け渡す面（StbSecSlabLoad・StbSecWallLoad、kind_structure="LOAD"） */
  load: boolean;
  materials: string[];
  raw: RawAttributes[];
}

const SLAB_SECTIONS: Record<string, string> = {
  RC: "StbSecSlab_RC",
  DECK: "StbSecSlabDeck",
  PRECAST: "StbSecSlabPrecast",
  LOAD: "StbSecSlabLoad",
};

/** スラブの断面の厚さ（6.10〜6.13）。テーパー・ハンチは一様な厚さで描く */
export function resolveSlabSection(doc: StbDocument, member: XmlElement, firstNodeId: number): PanelSection {
  const structure = member.attributes.kind_structure ?? "RC";
  const id = num(member, "id_section", Number.NaN);
  const section = doc.section(SLAB_SECTIONS[structure] ?? "StbSecSlab_RC", id);
  const load = structure === "LOAD";
  if (!section) {
    if (!load) doc.notices.add("missing-section", "warning", "断面が見つからないスラブ・壁を厚さのない面で示した");
    return { summary: null, thickness: 0, taper: null, load: true, materials: [], raw: [] };
  }
  const raw: RawAttributes[] = [rawOf(section)];
  let thickness = 0;
  let label = "";
  const used = (element: XmlElement | undefined) => {
    if (element) raw.push(rawOf(element));
  };
  if (structure === "RC") {
    // 2.1: StbSecSlab_RC_Conventional / StbSecSlab_RC_Truss、2.0: StbSecFigureSlab_RC
    const conventional = child(child(section, "StbSecSlab_RC_Conventional"), "StbSecFigureSlab_RC_Conventional");
    const legacy = child(section, "StbSecFigureSlab_RC");
    const truss = child(child(section, "StbSecSlab_RC_Truss"), "StbSecSlab_RC_TrussProduct");
    const straight =
      child(conventional, "StbSecSlab_RC_ConventionalStraight") ?? child(legacy, "StbSecSlab_RC_Straight");
    used(straight ?? truss);
    if (straight) {
      thickness = num(straight, "depth");
      label = `t=${formatNumber(thickness)}`;
    } else if (truss) {
      thickness = num(truss, "thickness");
      label = `t=${formatNumber(thickness)}（トラス筋付きデッキ ${truss.attributes.product_code ?? ""}）`;
    } else {
      const shaped =
        child(conventional, "StbSecSlab_RC_ConventionalTaper") ??
        child(conventional, "StbSecSlab_RC_ConventionalHaunch");
      const legacyItems = legacy?.children ?? [];
      used(shaped);
      for (const item of legacyItems) used(item);
      const depths = shaped
        ? [num(shaped, "base_depth"), num(shaped, "tip_depth")]
        : legacyItems.map((item) => num(item, "depth")).filter((value) => value > 0);
      if (depths.length > 0) {
        thickness = Math.max(...depths);
        label = `t=${depths.map(formatNumber).join("→")}`;
        doc.notices.add("slab-taper", "info", "テーパー・ハンチのあるスラブは、最も厚い部分の厚さで一様に描いた");
      }
    }
  } else if (structure === "DECK") {
    const straight = child(child(section, "StbSecFigureSlabDeck"), "StbSecSlabDeckStraight");
    const product = child(section, "StbSecSlabDeckProduct");
    used(straight ?? product);
    used(child(section, "StbSecProductSlabDeck"));
    if (straight) {
      thickness = num(straight, "depth");
      label = `t=${formatNumber(thickness)}（デッキ）`;
    } else if (product) {
      // 2.1 はデッキ山上のトップコンクリートだけを持ち、山の高さは製品型番一覧表（非公開）による
      thickness = num(product, "top_concrete");
      label = `トップコンクリート ${formatNumber(thickness)}（${product.attributes.product_code ?? "製品不明"}）`;
      doc.notices.add(
        "deck-height",
        "info",
        "デッキ合成スラブ（2.1）は、デッキの山の高さを含めずトップコンクリートの厚さで描いた",
      );
    }
  } else if (structure === "PRECAST") {
    const straight = child(child(section, "StbSecFigureSlabPrecast"), "StbSecSlabPrecastStraight");
    used(straight);
    thickness = num(straight, "depth_top_concrete", num(straight, "depth_concrete"));
    label = `トップコンクリート ${formatNumber(thickness)}（既製スラブ）`;
    doc.notices.add("precast-slab", "info", "既製スラブは、製品の厚さを含めずトップコンクリートの厚さで描いた");
  }
  if (!load && thickness <= 0)
    doc.notices.add("missing-figure", "warning", "厚さの分からないスラブ・壁を厚さのない面で示した");
  const materials = load
    ? []
    : [`コンクリート ${concreteStrength(doc, member, section, firstNodeId) ?? "（指定なし）"}`];
  return {
    summary: summaryOf(section, label ? [label] : []),
    thickness: load ? 0 : thickness,
    taper: null,
    load: load || thickness <= 0,
    materials,
    raw,
  };
}

/** 壁の断面の厚さ（6.14・6.15）。テーパーは下端・上端の厚さを返す */
export function resolveWallSection(doc: StbDocument, member: XmlElement, lastNodeId: number): PanelSection {
  const structure = member.attributes.kind_structure ?? "RC";
  const id = num(member, "id_section", Number.NaN);
  const load = structure === "LOAD";
  const section = doc.section(load ? "StbSecWallLoad" : "StbSecWall_RC", id);
  if (!section) {
    if (!load) doc.notices.add("missing-section", "warning", "断面が見つからないスラブ・壁を厚さのない面で示した");
    return { summary: null, thickness: 0, taper: null, load: true, materials: [], raw: [] };
  }
  const figure = child(section, "StbSecFigureWall_RC");
  const straight = child(figure, "StbSecWall_RC_Straight");
  const taper = child(figure, "StbSecWall_RC_Taper");
  let thickness = 0;
  let label = "";
  let tapered: { bottom: number; top: number } | null = null;
  if (straight) {
    thickness = num(straight, "t");
    label = `t=${formatNumber(thickness)}`;
  } else if (taper) {
    tapered = { bottom: num(taper, "t_bottom"), top: num(taper, "t_top") };
    thickness = Math.max(tapered.bottom, tapered.top);
    label = `t=${formatNumber(tapered.bottom)}→${formatNumber(tapered.top)}`;
  }
  if (!load && thickness <= 0)
    doc.notices.add("missing-figure", "warning", "厚さの分からないスラブ・壁を厚さのない面で示した");
  return {
    summary: summaryOf(section, label ? [label] : []),
    thickness: load ? 0 : thickness,
    taper: tapered,
    load: load || thickness <= 0,
    materials: load ? [] : [`コンクリート ${concreteStrength(doc, member, section, lastNodeId) ?? "（指定なし）"}`],
    raw: [rawOf(section), ...(figure?.children.map((item) => rawOf(item)) ?? [])],
  };
}
