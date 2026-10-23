import { readDrawing, sceneToJson } from "@zumenjs/core";
import { describe, it } from "vitest";
import { matchGolden } from "../../../tests/golden.js";
import { sxf } from "../src/index.js";
import {
  P21_FEATURES,
  P21_NO_SHEET,
  P21_SAMPLE,
  p21Bytes,
  SFC_FEATURES,
  SFC_NO_SHEET,
  SFC_RASTERS,
  SFC_SAMPLE,
  sfcBytes,
} from "./fixtures.js";

const DRAWINGS: [string, Uint8Array][] = [
  ["sfc/sample.sfc", sfcBytes(SFC_SAMPLE)],
  ["sfc/features.sfc", sfcBytes(SFC_FEATURES)],
  ["sfc/nosheet.sfc", sfcBytes(SFC_NO_SHEET)],
  ["sfc/rasters.sfc", sfcBytes(SFC_RASTERS)],
  ["p21/sample.p21", p21Bytes(P21_SAMPLE)],
  ["p21/features.p21", p21Bytes(P21_FEATURES)],
  ["p21/nosheet.p21", p21Bytes(P21_NO_SHEET)],
];

// Rust の読み取り（crates/zumen-sxf）も同じ図面を読み、同じ Scene の JSON になることを確かめる
describe("SXF の正解ファイル", () => {
  it.each(DRAWINGS)("%s と Scene", (name, bytes) => {
    matchGolden(name, bytes);
    matchGolden(name.replace(/\.[^.]+$/, ".scene.json"), `${sceneToJson(readDrawing(bytes, [sxf]), 2)}\n`);
  });
});
