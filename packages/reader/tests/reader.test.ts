import { readdirSync, readFileSync } from "node:fs";
import { readDrawing, sceneToJson } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { compareScenes } from "../../../scripts/compare-scenes.mjs";
import {
  createReader,
  detectDrawing,
  initPdfReader,
  initReader,
  isDwg,
  isPdf,
  isPdfReaderReady,
  isReaderReady,
  LimitError,
  reader,
  readScene,
} from "../src/index.js";
import { detect } from "../wasm/zumen_wasm.js";
import { detect as detectPdf } from "../wasm/zumen_wasm_pdf.js";

const fixtures = new URL("../../../tests/fixtures/", import.meta.url);
const read = (name: string) => new Uint8Array(readFileSync(new URL(name, fixtures)));
const latin1 = (text: string) => Uint8Array.from(text, (char) => char.charCodeAt(0));
const drawings = readdirSync(fixtures, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .flatMap((entry) => readdirSync(new URL(`${entry.name}/`, fixtures)).map((name) => `${entry.name}/${name}`))
  .filter((name) => !name.endsWith(".json"))
  .sort();

/** 書庫に入れる 1 つのファイル。名前と、UTF-8 の名前か */
interface Entry {
  name: string;
  bytes: Uint8Array;
  utf8?: boolean;
}

/** 圧縮しない（stored）ZIP の書庫。圧縮した SXF（P2Z・SFZ）を試すのに使う */
function zip(entries: Entry[], comment = ""): Uint8Array {
  const parts: number[] = [];
  const central: number[] = [];
  const u16 = (out: number[], value: number) => out.push(value & 0xff, (value >> 8) & 0xff);
  const u32 = (out: number[], value: number) => {
    u16(out, value & 0xffff);
    u16(out, value >>> 16);
  };
  for (const entry of entries) {
    const name = entry.utf8 ? new TextEncoder().encode(entry.name) : latin1(entry.name);
    const flags = entry.utf8 ? 0x800 : 0;
    const local = parts.length;
    const crc = crc32(entry.bytes);
    u32(parts, 0x04034b50);
    for (const value of [20, flags, 0, 0, 0]) u16(parts, value);
    for (const value of [crc, entry.bytes.length, entry.bytes.length]) u32(parts, value);
    u16(parts, name.length);
    u16(parts, 0);
    parts.push(...name, ...entry.bytes);
    u32(central, 0x02014b50);
    for (const value of [20, 20, flags, 0, 0, 0]) u16(central, value);
    for (const value of [crc, entry.bytes.length, entry.bytes.length]) u32(central, value);
    for (const value of [name.length, 0, 0, 0, 0]) u16(central, value);
    u32(central, 0);
    u32(central, local);
    central.push(...name);
  }
  const offset = parts.length;
  parts.push(...central);
  u32(parts, 0x06054b50);
  for (const value of [0, 0, entries.length, entries.length]) u16(parts, value);
  u32(parts, central.length);
  u32(parts, offset);
  u16(parts, comment.length);
  parts.push(...latin1(comment));
  return Uint8Array.from(parts);
}

function crc32(bytes: Uint8Array): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = crc & 1 ? (crc >>> 1) ^ 0xedb88320 : crc >>> 1;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

// テストは書いた順に走る。最初のテストは initReader の前で、ほかのテストは読み込んだ後
describe("@zumenjs/reader", () => {
  // 利用者は図面を開いたときに初めて WebAssembly を読み込める。読み込む前に読むと、黙って失敗せず、やり方を伝える
  it("WebAssembly を読み込む前でも形式を見分け、読もうとすると initReader を待つように伝える", () => {
    expect(isReaderReady()).toBe(false);
    expect(reader.detect(read("dwg/r2018.dwg"))).toBe("dwg");
    expect(reader.detect(read("jww/minimal-v600.jww"))).toBe("jww");
    expect(() => readDrawing(read("dxf/r12.dxf"), [reader])).toThrow(/initReader/);
    expect(() => readDrawing(latin1("not a drawing"), [reader])).toThrow(/対応していない形式/);
    // PDF は別のモジュール（決定 0030）。形式は読み込む前でも分かる
    expect(reader.detect(read("pdf/drawing.pdf"))).toBe("pdf");
    expect(() => readDrawing(read("pdf/drawing.pdf"), [reader])).toThrow(/initPdfReader/);
  });

  it("読み込みに失敗したら、次の initReader で読み込み直す", async () => {
    await expect(initReader(Uint8Array.of(0x00, 0x61, 0x73, 0x6d))).rejects.toThrow();
    const loading = initReader();
    expect(initReader()).toBe(loading);
    await loading;
    expect(isReaderReady()).toBe(true);
    // PDF のモジュールは、PDF を読むときにだけ読み込む
    expect(isPdfReaderReady()).toBe(false);
    expect(() => readScene(read("pdf/drawing.pdf"))).toThrow(/initPdfReader/);
    await reader.load?.("dxf");
    expect(isPdfReaderReady()).toBe(false);
    await reader.load?.("pdf");
    expect(isPdfReaderReady()).toBe(true);
    expect(initPdfReader()).toBe(initPdfReader());
  });

  // TypeScript の判定は、WebAssembly の detect（zumen::detect）と同じ規則でなければならない
  it("detectDrawing は WebAssembly の detect と同じ形式を返す", () => {
    const p21 = read("p21/sample.p21");
    const sfc = read("sfc/sample.sfc");
    const inputs = [
      ...drawings.map(read),
      ...["MC0.0", "AC1.2", "AC1.40", "AC1.50", "AC2.10"].flatMap((magic) =>
        [magic, `${magic}\0`, `${magic}x`].map(latin1),
      ),
      ...["AC1032", "AC1014", "AC1099", "AC1032AC", "AC103", "AC10", "AC1A32", "ac1032", "AC2032", " AC1032"].map(
        latin1,
      ),
      ...["JwwData.", "JwwData", "999\ncomment\n  0\nSECTION\n", "ï»¿0\r\nSECTION", "0\nSECTIONS"].map(latin1),
      ...["jw_cad(c)data", "jw_cad(c)dat", "JW_CAD(c)data", "JwsData.", "JwsData", "jwsData."].map(latin1),
      latin1("AutoCAD Binary DXF\r\n\x1a\x00"),
      latin1(" \r\nISO-10303-21;\nFILE_DESCRIPTION ( ( 'SCADEC level2 feature_mode'"),
      latin1("\tISO-10303-21;"),
      zip([{ name: "a/B.P21", bytes: p21 }]),
      zip([
        { name: "readme.txt", bytes: latin1("x") },
        { name: "図面.sfc", bytes: sfc, utf8: true },
      ]),
      zip([{ name: "readme.txt", bytes: latin1("x") }]),
      zip([{ name: "x.sfc", bytes: sfc }], "comment"),
      zip([{ name: "x.sfc", bytes: sfc }]).subarray(0, 60),
      latin1("PK\x03\x04"),
      new Uint8Array(),
      ...["%PDF-1.7", "%PDF", "%pdf-1.7", `${" ".repeat(1019)}%PDF-`, `${" ".repeat(1020)}%PDF-`].map(latin1),
      latin1("999\n%PDF-1.4\n  0\nSECTION\n"),
    ];
    // 形式はどちらかのモジュールだけが認める
    for (const bytes of inputs) expect(detectDrawing(bytes)).toBe(detect(bytes) ?? detectPdf(bytes) ?? null);
    expect(inputs.map(detectDrawing).slice(-13, -6)).toEqual(["p2z", "sfz", null, "sfz", null, null, null]);
    expect(inputs.map(detectDrawing).slice(-6)).toEqual(["pdf", null, null, "pdf", null, "dxf"]);
    expect(drawings.filter((name) => isDwg(read(name)))).toEqual(["dwg/r2000.dwg", "dwg/r2018.dwg"]);
    expect(drawings.filter((name) => isPdf(read(name)))).toEqual(["pdf/drawing.pdf"]);
  });

  // IFC や STEP の AP203・AP214・AP242 も ISO-10303-21 で始まる。FILE_SCHEMA の 1 つ目のスキーマが SXF の
  // ASSOCIATIVE_DRAUGHTING でないものは SXF ではなく、FILE_SCHEMA のないものは SXF とみなす。入力は
  // crates/zumen-sxf/tests/detect.rs と同じで、WebAssembly の detect も同じ形式を返す
  it("FILE_SCHEMA がほかのスキーマの STEP のファイルを SXF とみなさず、対応していない形式として読まない", () => {
    const step = (schema: string) =>
      latin1(
        "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION((''),'2;1');\n" +
          `FILE_NAME('x','2026-10-23T00:00:00',(''),(''),'','','');\n${schema}\n` +
          "ENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n",
      );
    const others = [
      "FILE_SCHEMA(('IFC2X3'));",
      "FILE_SCHEMA(('IFC4'));",
      "FILE_SCHEMA(('IFC4X3_ADD2'));",
      // AP203、AP214（スキーマのオブジェクト識別子つき）、AP242
      "FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));",
      "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));",
      "FILE_SCHEMA(('AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }'));",
      // 大文字と小文字、空白（\s の VT・FF・NBSP も）
      "FILE_SCHEMA(('ifc4'));",
      "file_schema(('IFC4'));",
      "FILE_SCHEMA ( ( 'IFC4' ) ) ;",
      "FILE_SCHEMA\r\n\t(\t(\r\n'IFC4'));",
      "FILE_SCHEMA\x0b\x0c\xa0(('IFC4'));",
      // 名前の一部が同じもの、\s でない U+0085 が続くもの、ß（toUpperCase では SS）を含むもの
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING_X'));",
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING\x85'));",
      "FILE_SCHEMA(('A\xdfOCIATIVE_DRAUGHTING'));",
      // 1 つ目がほかのスキーマのもの、空の名前、FILE_SCHEMA という文字列を含む FILE_NAME の後
      "FILE_SCHEMA(('IFC4','ASSOCIATIVE_DRAUGHTING'));",
      "FILE_SCHEMA((''));",
      "FILE_NAME('FILE_SCHEMA');\nFILE_SCHEMA(('IFC4'));",
    ];
    const sxfs = [
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));",
      "FILE_SCHEMA(('associative_draughting'));",
      "file_schema (( ' Associative_Draughting\t' ));",
      "FILE_SCHEMA\xa0(('\xa0ASSOCIATIVE_DRAUGHTING\xa0'));",
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING { 1 0 10303 202 1 1 1 }'));",
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING{1 0 10303 202 1 1 1}'));",
      "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING','IFC4'));",
      // FILE_SCHEMA がない、スキーマ名の文字列がないものは、これまでどおり SXF
      "",
      "FILE_SCHEMA(());",
      "FILE_SCHEMA($);",
    ];
    // tests/fixtures の SXF から FILE_SCHEMA を除いたものは SXF のまま、IFC4 にしたものは SXF でない
    const line = "FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));\n";
    const fixtures = drawings
      .filter((name) => /^(sfc|p21)\//.test(name))
      .flatMap((name): [Uint8Array, string | null][] => {
        const text = Buffer.from(read(name)).toString("latin1");
        expect(text).toContain(line);
        return [
          [latin1(text.replace(line, "")), name.slice(0, 3)],
          [latin1(text.replace(line, "FILE_SCHEMA(('IFC4'));\n")), null],
        ];
      });
    // 先頭の 64 KiB だけを見る。FILE_SCHEMA の文字列の閉じる引用符が 64 KiB の最後のバイトなら読み、次のバイトなら読まない
    const quoteAt = (at: number) => {
      const [before, after] = ["ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('", "'),'2;1');\nFILE_SCHEMA(('IFC4"];
      const padding = "x".repeat(at - before.length - after.length);
      const bytes = latin1(`${before}${padding}${after}'));\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n`);
      expect(bytes[at]).toBe(0x27);
      return bytes;
    };
    const cases: [Uint8Array, string | null][] = [
      ...others.map((schema): [Uint8Array, null] => [step(schema), null]),
      ...sxfs.map((schema): [Uint8Array, string] => [step(schema), "p21"]),
      // ISO-10303-21 の前の空白と改行は、これまでどおり飛ばす
      [Uint8Array.from([...latin1(" \r\n"), ...step("FILE_SCHEMA(('IFC4'));")]), null],
      [step("/* FILE_SCHEMA(('IFC4')); */\nFILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));"), "p21"],
      [step("/* FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING')); */\nFILE_SCHEMA(('IFC4'));"), null],
      [
        latin1(
          Buffer.from(step("FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));"))
            .toString("latin1")
            .replace("FILE_NAME('x'", "FILE_NAME('FILE_SCHEMA((''IFC4''))'"),
        ),
        "p21",
      ],
      [step("FILE_SCHEMA/* metadata */( /* metadata */ ('IFC4'));"), null],
      [step("FILE_SCHEMA/* metadata */(('ASSOCIATIVE_DRAUGHTING'));"), "p21"],
      ...fixtures,
      [quoteAt(64 * 1024 - 1), null],
      [quoteAt(64 * 1024), "p21"],
    ];
    for (const [bytes, format] of cases) {
      // 違えば、ヘッダの FILE_SCHEMA の辺りを示す
      const header = Buffer.from(bytes.subarray(0, 200)).toString("latin1");
      expect(detectDrawing(bytes), header).toBe(format);
      expect(detect(bytes) ?? null, header).toBe(format);
    }
    // TypeScript の readDrawing も Rust の読み取り（readScene）も、どの読み取りも読めない形式のエラーにする
    const message = "対応していない形式のファイルです（読める形式: SFC・P21・SFZ・P2Z・JWW・JWC・JWS・DXF・DWG・PDF）";
    const ifc = step("FILE_SCHEMA(('IFC4'));");
    expect(() => readDrawing(ifc, [reader])).toThrow(message);
    expect(() => readScene(ifc)).toThrow(message);
  });

  it("JWC と JWS を WebAssembly 読み込み前の判定と reader の対応形式に含める", () => {
    for (const [signature, format] of [
      ["jw_cad(c)data", "jwc"],
      ["JwsData.", "jws"],
    ] as const) {
      expect(reader.detect(latin1(signature))).toBe(format);
      expect(reader.formats).toContain(format);
    }
  });

  // どの形式も Rust の読み取りで読み、TypeScript の読み取りが書いた正解ファイル（DWG と PDF は golden.test.ts）と同じ Scene になる
  it.each(drawings)("%s を正解と同じ Scene に読む", (name) => {
    const stem = name.replace(/\.[^.]+$/, "");
    const golden = JSON.parse(readFileSync(new URL(`${stem}.scene.json`, fixtures), "utf8"));
    const scene = readDrawing(read(name), [reader]);
    expect(compareScenes(golden, JSON.parse(sceneToJson(scene)))).toEqual([]);
  });

  it("圧縮した SXF を、中の SXF と同じ Scene に読み、書庫と中のファイルの名前を示す", () => {
    const bytes = read("p21/sample.p21");
    const scene = readScene(zip([{ name: "図面.P21", bytes, utf8: true }]));
    const plain = readScene(bytes);
    expect(scene.format).toBe("p21");
    expect(scene.metadata).toContainEqual({ key: "archive", label: "圧縮", value: "P2Z（図面.P21）" });
    expect(scene.entities).toEqual(plain.entities);
  });

  // DrawingViewer は TypeScript の読み取りと同じ Scene を受け取る（JSON の数の配列ではなく Float64Array の点列）
  it("Scene の点列は Float64Array に戻る", () => {
    const scene = readDrawing(read("dwg/r2000.dwg"), [reader], { view: "Layout1" });
    const primitive = scene.entities[0]?.primitives[0];
    expect(primitive?.kind === "stroke" && primitive.points).toBeInstanceOf(Float64Array);
  });

  // PDF のページはビュー。図形は下敷きのページが描くので drawn: false、上限は PDF のモジュールにも効く
  it("PDF をページごとに読み、上限を超えたら LimitError になる", () => {
    const bytes = read("pdf/drawing.pdf");
    const scene = readScene(bytes, { view: "2" });
    expect(scene.format).toBe("pdf");
    expect(scene.views?.map((view) => view.name)).toEqual(["1", "詳細-1"]);
    expect(scene.underlays?.[0]).toMatchObject({ kind: "pdf", page: 2 });
    expect(scene.entities.every((entity) => entity.drawn === false)).toBe(true);
    expect(() => readScene(bytes, { limits: { maxEntities: 1 } })).toThrow(
      expect.objectContaining({ limit: "entities" }),
    );
    expect(() => readScene(latin1("%PDF-1.7\nnot a document"))).toThrow(/PDF/);
  });

  // Rust の読み取りのエラーは、そのメッセージの Error になる（未対応の版や壊れた図面を開いた利用者に見せる）
  it("読めない図面は Rust の読み取りのメッセージのエラーになる", () => {
    const unsupported = Uint8Array.from({ length: 0x100 }, (_, index) => latin1("AC1099")[index] ?? 0);
    expect(() => readDrawing(unsupported, [reader])).toThrow("対応していない DWG の版です（AC1099）");
    expect(() => readScene(read("dwg/r2018.dwg").subarray(0, 0x80))).toThrow(Error);
    expect(() => readScene(read("dxf/r12.dxf"), { format: "stl" })).toThrow(/STL/);
  });

  // 受け取った図面に、かけてよい大きさを決められる。超えたらどの上限かが分かる
  it("上限を超えたら、上限の種類の LimitError になる", () => {
    const bytes = read("dxf/r12.dxf");
    const bytesError = (() => {
      try {
        readScene(bytes, { limits: { maxBytes: 100 } });
      } catch (error) {
        return error;
      }
    })();
    expect(bytesError).toBeInstanceOf(LimitError);
    expect(bytesError).toMatchObject({
      limit: "bytes",
      message: "図面が上限の 100 バイトより大きいため、読みませんでした",
    });
    const limited = createReader({ limits: { maxEntities: 1 } });
    expect(() => readDrawing(bytes, [limited])).toThrow(LimitError);
    expect(() => readDrawing(bytes, [limited])).toThrow(expect.objectContaining({ limit: "entities" }));
    // 上限の中なら、上限のない読み取りと同じ
    const scene = readDrawing(bytes, [createReader({ limits: { maxBytes: bytes.length, maxEntities: 1e6 } })]);
    expect(sceneToJson(scene)).toBe(sceneToJson(readDrawing(bytes, [reader])));
  });
});
