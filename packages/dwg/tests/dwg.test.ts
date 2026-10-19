import { readdirSync, readFileSync } from "node:fs";
import { readDrawing } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { dwg, initDwg, isDwg } from "../src/index.js";
import { detect } from "../wasm/zumen_wasm.js";

const fixtures = new URL("../../../tests/fixtures/", import.meta.url);
const read = (name: string) => new Uint8Array(readFileSync(new URL(name, fixtures)));
const latin1 = (text: string) => Uint8Array.from(text, (char) => char.charCodeAt(0));

// テストは書いた順に走る。最初のテストは initDwg の前で、ほかのテストは読み込んだ後
describe("@zumenjs/dwg", () => {
  // 利用者は DWG を開いたときに初めて WebAssembly を読み込める。読み込む前に読むと、黙って失敗せず、やり方を伝える
  it("WebAssembly を読み込む前でも DWG を見分け、読もうとすると initDwg を待つように伝える", () => {
    const bytes = read("dwg/r2018.dwg");
    expect(dwg.detect(bytes)).toBe("dwg");
    expect(() => readDrawing(bytes, [dwg])).toThrow(/initDwg/);
    // DWG でないファイルは、ほかの形式と同じく対応していない形式のエラーになる
    expect(() => readDrawing(read("jww/minimal-v600.jww"), [dwg])).toThrow(/対応していない形式/);
  });

  it("読み込みに失敗したら、次の initDwg で読み込み直す", async () => {
    await expect(initDwg(Uint8Array.of(0x00, 0x61, 0x73, 0x6d))).rejects.toThrow();
    const loading = initDwg();
    expect(initDwg()).toBe(loading);
    await loading;
  });

  // TypeScript の判定は、WebAssembly の detect（zumen_dwg::is_dwg）と同じ規則でなければならない
  it("isDwg は WebAssembly の detect と同じ図面を DWG とみなす", () => {
    const files = readdirSync(fixtures, { withFileTypes: true })
      .filter((entry) => entry.isDirectory())
      .flatMap((entry) => readdirSync(new URL(`${entry.name}/`, fixtures)).map((name) => `${entry.name}/${name}`))
      .filter((name) => !name.endsWith(".json"));
    const inputs = [
      ...files.map(read),
      ...["AC1032", "AC1014", "AC1099", "AC1032AC", "AC103", "AC10", "AC1A32", "ac1032", "AC2032", " AC1032"].map(
        latin1,
      ),
      new Uint8Array(),
    ];
    for (const bytes of inputs) expect(isDwg(bytes)).toBe(detect(bytes) === "dwg");
    expect(files.filter((name) => isDwg(read(name))).sort()).toEqual(["dwg/r2000.dwg", "dwg/r2018.dwg"]);
  });

  // Rust の読み取りのエラーは、そのメッセージの Error になる（R13・R14 や壊れた図面を開いた利用者に見せる）
  it("読めない DWG は Rust の読み取りのメッセージのエラーになる", () => {
    const r14 = Uint8Array.from({ length: 0x100 }, (_, index) => latin1("AC1014")[index] ?? 0);
    expect(() => readDrawing(r14, [dwg])).toThrow("対応していない DWG の版です（AC1014、AutoCAD R13・R14）");
    expect(() => readDrawing(read("dwg/r2018.dwg").subarray(0, 0x80), [dwg])).toThrow(Error);
  });

  // DrawingViewer は TypeScript の読み取りと同じ Scene を受け取る（JSON の数の配列ではなく Float64Array の点列）
  it("Scene の点列は Float64Array に戻る", () => {
    const scene = readDrawing(read("dwg/r2000.dwg"), [dwg], { view: "Layout1" });
    const primitive = scene.entities[0]?.primitives[0];
    expect(primitive?.kind === "stroke" && primitive.points).toBeInstanceOf(Float64Array);
  });
});
