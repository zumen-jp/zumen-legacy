import { NoticeList } from "@zumenjs/core";
import { assembleDocument, type SxfDrawing } from "./document.js";
import { featureFromRecord, type SxfFeature } from "./features.js";
import { readSfc } from "./sfc-reader.js";

/** SXF のファイルの版。出力ライブラリ名の末尾（$$3.1 など）か、フィーチャの囲み（/*SXF3.1）から決める */
function sxfVersion(preprocessor: string, recordVersions: Iterable<string>): string {
  const suffix = /\$\$(\d+(?:\.\d+)?)\s*$/.exec(preprocessor)?.[1];
  if (suffix) return `SXF ${suffix}`;
  let highest = "2.0";
  for (const version of recordVersions) {
    if (version === "3.1") highest = "3.1";
    else if (version === "3" && highest !== "3.1") highest = "3.0";
  }
  return `SXF ${highest}`;
}

/** SFC ファイルのバイト列を読み、フィーチャを並び順のとおりに組み立てる */
export function parseSfc(bytes: Uint8Array): SxfDrawing {
  const notices = new NoticeList();
  const read = readSfc(bytes);
  if (read.skipped > 0) notices.add("warning", "unreadable", "書式を読めないフィーチャを省きました", read.skipped);
  const features: SxfFeature[] = [];
  const versions = new Set<string>();
  for (const record of read.records) {
    versions.add(record.version);
    try {
      const feature = featureFromRecord(record);
      if (feature) features.push(feature);
      else notices.add("warning", "unknown-feature", "対応していないフィーチャを省きました");
    } catch {
      notices.add("warning", "invalid-feature", "引数が足りない・数値でないフィーチャを省きました");
    }
  }
  if (read.records.length === 0) throw new Error("SFC のフィーチャが見つかりません");
  const document = assembleDocument(features, notices);
  return {
    format: "sfc",
    version: sxfVersion(read.header.preprocessor, versions),
    header: read.header,
    document,
    notices: notices.toArray(),
  };
}
