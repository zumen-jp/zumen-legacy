import type { Notice, NoticeList } from "@zumenjs/core";
import { FEATURE_LABELS, type FeatureKind, type SxfFeature } from "./features.js";
import { predefinedWidthCode } from "./tables.js";

type FeatureOf<K extends FeatureKind> = Extract<SxfFeature, { kind: K }>;

export interface CompositeCurve {
  feature: FeatureOf<"composite_curve_org">;
  members: SxfFeature[];
}

export interface SubFigure {
  feature: FeatureOf<"sfig_org">;
  members: SxfFeature[];
}

/**
 * フィーチャを並び順のとおりに組み立てた SXF の図面。
 * 複合図形定義・複合曲線定義は、その直前の定義以降に書かれた要素を構成要素にし、
 * 用紙は残りの要素を持つ（附属書 SFC 編 1-3）。コードは出現順に割り当てる。
 */
export interface SxfDocument {
  sheet: FeatureOf<"drawing_sheet"> | null;
  title: FeatureOf<"drawing_attribute"> | null;
  /** レイヤコード 1 から */
  layers: FeatureOf<"layer">[];
  /** 色コード 17 から */
  userColors: FeatureOf<"user_defined_colour">[];
  /** 線種コード 17 から */
  userLineTypes: FeatureOf<"user_defined_font">[];
  /** 線幅コード 11 から（既定義の値の線幅は 1〜9 の固定コード） */
  userWidths: FeatureOf<"width">[];
  /** 文字フォントコード 1 から */
  fonts: FeatureOf<"text_font">[];
  /** 複合曲線の番号 1 から */
  compositeCurves: CompositeCurve[];
  subFigures: Map<string, SubFigure>;
  sheetMembers: SxfFeature[];
  counts: Map<FeatureKind, number>;
}

/** ファイルのヘッダ（FILE_DESCRIPTION・FILE_NAME）の文字列 */
export interface SxfHeader {
  description: string;
  fileName: string;
  preprocessor: string;
  originatingSystem: string;
}

/** SFC・P21 を読んだ結果。P21 も SFC と同じフィーチャに変換して持つ */
export interface SxfDrawing {
  format: "sfc" | "p21";
  /** SXF の版（SXF 3.1 など） */
  version: string;
  header: SxfHeader;
  document: SxfDocument;
  /** 読み取りで省略・代替した内容 */
  notices: Notice[];
}

const STRUCTURE_KINDS = new Set<FeatureKind>([
  "drawing_attribute",
  "drawing_sheet",
  "layer",
  "pre_defined_font",
  "user_defined_font",
  "pre_defined_colour",
  "user_defined_colour",
  "width",
  "text_font",
  "sfig_org",
  "composite_curve_org",
]);

/** 複合曲線の構成要素にできる曲線（仕様は円弧・楕円弧・折線・スプライン。線分なども受け付ける） */
const CURVE_KINDS = new Set<FeatureKind>(["arc", "ellipse_arc", "polyline", "spline", "line", "clothoid"]);

export function assembleDocument(features: readonly SxfFeature[], notices: NoticeList): SxfDocument {
  const doc: SxfDocument = {
    sheet: null,
    title: null,
    layers: [],
    userColors: [],
    userLineTypes: [],
    userWidths: [],
    fonts: [],
    compositeCurves: [],
    subFigures: new Map(),
    sheetMembers: [],
    counts: new Map(),
  };
  let pending: SxfFeature[] = [];
  let afterSheet = 0;
  for (const feature of features) {
    doc.counts.set(feature.kind, (doc.counts.get(feature.kind) ?? 0) + 1);
    switch (feature.kind) {
      case "layer":
        doc.layers.push(feature);
        continue;
      case "user_defined_colour":
        doc.userColors.push(feature);
        continue;
      case "user_defined_font":
        doc.userLineTypes.push(feature);
        continue;
      case "width":
        if (predefinedWidthCode(feature.width) === 0) doc.userWidths.push(feature);
        continue;
      case "text_font":
        doc.fonts.push(feature);
        continue;
      case "drawing_attribute":
        doc.title = feature;
        continue;
      case "pre_defined_font":
      case "pre_defined_colour":
        continue;
      case "composite_curve_org": {
        const members = pending.filter((member) => CURVE_KINDS.has(member.kind));
        const rest = pending.length - members.length;
        if (rest > 0) {
          notices.add("warning", "composite-member", "複合曲線に使えない要素を構成要素から除きました", rest);
        }
        doc.compositeCurves.push({ feature, members });
        pending = [];
        continue;
      }
      case "sfig_org":
        if (doc.subFigures.has(feature.name)) {
          notices.add("warning", "duplicate-sfig", "同じ名前の複合図形定義は後の定義を使いました");
        }
        doc.subFigures.set(feature.name, { feature, members: pending });
        pending = [];
        continue;
      case "drawing_sheet":
        if (doc.sheet) {
          notices.add("warning", "duplicate-sheet", "2 つ目以降の用紙を無視しました");
          continue;
        }
        doc.sheet = feature;
        doc.sheetMembers = pending;
        pending = [];
        continue;
      default:
        if (STRUCTURE_KINDS.has(feature.kind)) continue;
        if (doc.sheet) afterSheet++;
        pending.push(feature);
    }
  }
  if (!doc.sheet) {
    notices.add("warning", "no-sheet", "用紙フィーチャがないため、すべての要素を用紙に置きました");
    doc.sheetMembers = pending;
  } else if (pending.length > 0) {
    // 仕様では用紙より後の要素は無効。読み落としを避けるため用紙に加え、件数を知らせる
    doc.sheetMembers = [...doc.sheetMembers, ...pending];
    notices.add("warning", "after-sheet", "用紙より後に書かれた要素を用紙に加えました", afterSheet);
  }
  return doc;
}

export function featureLabel(kind: FeatureKind): string {
  return FEATURE_LABELS[kind];
}
