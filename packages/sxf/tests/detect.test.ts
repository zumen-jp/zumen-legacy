import { readDrawing } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { detectSxf, sxf } from "../src/index.js";
import { P21_SAMPLE, p21Bytes, SFC_SAMPLE, sfcBytes } from "./fixtures.js";

// IFC や STEP の AP203・AP214・AP242 も ISO-10303-21 で始まる。FILE_SCHEMA の 1 つ目のスキーマが SXF の
// ASSOCIATIVE_DRAUGHTING でないものは SXF ではない。Rust の detect_sxf と同じ規則で、ほかの場合は
// crates/zumen-sxf/tests/detect.rs と packages/reader/tests/reader.test.ts が確かめる
describe("SXF の判定", () => {
  const line = "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));";
  const p21 = (schema: string) => p21Bytes(P21_SAMPLE.replace(line, schema));
  const sfc = (schema: string) => sfcBytes(SFC_SAMPLE.replace(line, schema));

  it("FILE_SCHEMA がほかのスキーマのファイルは SXF とみなさず、対応していない形式として読まない", () => {
    for (const schema of ["IFC2X3", "IFC4", "IFC4X3_ADD2", "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }", "ifc4"]) {
      expect(detectSxf(p21(`FILE_SCHEMA(('${schema}'));`))).toBeNull();
      expect(detectSxf(sfc(`FILE_SCHEMA(('${schema}'));`))).toBeNull();
    }
    expect(() => readDrawing(p21("FILE_SCHEMA(('IFC4'));"), [sxf])).toThrow(
      "対応していない形式のファイルです（読める形式: SFC・P21）",
    );
  });

  it("FILE_SCHEMA のないものと、大文字と小文字・空白・オブジェクト識別子の違う SXF のスキーマは SXF とみなす", () => {
    for (const schema of [
      line,
      "",
      "file_schema (( ' associative_draughting { 1 0 10303 202 1 1 1 }' ));",
      `/* FILE_SCHEMA(('IFC4')); */\n${line}`,
      `FILE_NAME('FILE_SCHEMA((''IFC4''))');\n${line}`,
    ]) {
      expect(detectSxf(p21(schema))).toBe("p21");
      expect(detectSxf(sfc(schema))).toBe("sfc");
    }
  });
});
