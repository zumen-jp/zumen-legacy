import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { cp932 } from "../../../tests/cp932.js";
import { type DxfMText, type DxfText, dxf, dxfToScene, parseDxf } from "../src/index.js";

/** グループコードと値の組を、テキストの DXF（1 行ずつ）にする */
function ascii(...pairs: (string | number)[]): string {
  const lines: string[] = [];
  for (let i = 0; i + 1 < pairs.length; i += 2) lines.push(String(pairs[i]), String(pairs[i + 1]));
  return `${lines.join("\r\n")}\r\n`;
}

const latin1 = (text: string) => Uint8Array.from(text, (char) => char.charCodeAt(0));

function stroke(entity: SceneEntity | undefined): number[] {
  const primitive = entity?.primitives.find((item) => item.kind === "stroke");
  if (primitive?.kind !== "stroke") throw new Error("線がありません");
  return Array.from(primitive.points, (value) => Math.round(value * 1e6) / 1e6);
}

/**
 * 文字コードは AutoCAD 2007 より前の DXF では $DWGCODEPAGE で決まり、誤ると日本語が黙って文字化けする。
 * CP932 の 2 バイト目の 0x5C（表・ソ・能）を MTEXT の書式コードの \ と取り違えないことも確かめる。
 */
describe("DXF の文字コード", () => {
  // biome-ignore format: DXF のタグ（グループコードと値）の組を 1 行に並べて読めるようにする
  const body = ascii(
    0, "SECTION", 2, "TABLES", 0, "TABLE", 2, "LAYER", 0, "LAYER", 2, "平面図", 70, 0, 62, 2, 6, "CONTINUOUS",
    0, "ENDTAB", 0, "ENDSEC", 0, "SECTION", 2, "ENTITIES",
    0, "TEXT", 5, "10", 8, "平面図", 10, 0, 20, 0, 40, 2.5, 1, "表示ソ能\\U+5E73",
    0, "MTEXT", 5, "11", 8, "平面図", 10, 0, 20, -10, 40, 2.5, 1, "表\\P能ソ",
    0, "ENDSEC", 0, "EOF",
  );

  it("$DWGCODEPAGE の ANSI_932 で復号し、\\U+XXXX を文字にする", () => {
    const header = ascii(
      0,
      "SECTION",
      2,
      "HEADER",
      9,
      "$ACADVER",
      1,
      "AC1015",
      9,
      "$DWGCODEPAGE",
      3,
      "ANSI_932",
      0,
      "ENDSEC",
    );
    const doc = parseDxf(cp932(header + body));
    expect(doc.encoding).toBe("shift_jis");
    expect(doc.layers.map((layer) => layer.name)).toEqual(["平面図"]);
    const [text, mtext] = doc.entities as [DxfText, DxfMText];
    expect(text.text).toBe("表示ソ能平");
    expect(mtext.text).toBe("表\n能ソ");
  });

  it("指定のない R12 は、日本語として正しく読めるときだけ Shift_JIS とし、西欧の ° は Shift_JIS にしない", () => {
    const japanese = parseDxf(cp932(ascii(0, "SECTION", 2, "HEADER", 9, "$ACADVER", 1, "AC1009", 0, "ENDSEC") + body));
    expect(japanese.encoding).toBe("shift_jis");
    expect((japanese.entities[0] as DxfText).text).toBe("表示ソ能平");
    // 0xB0 は西欧の °、Shift_JIS では半角カナの ｰ
    const western = parseDxf(
      latin1(ascii(0, "SECTION", 2, "ENTITIES", 0, "TEXT", 10, 0, 20, 0, 40, 1, 1, "45\xb0", 0, "ENDSEC")),
    );
    expect(western.encoding).toBe("windows-1252");
    expect((western.entities[0] as DxfText).text).toBe("45°");
  });

  it("バイナリの DXF を、グループコードごとの値の型（2・4・8 バイト、真偽値、文字列）で読む", () => {
    const parts: number[] = [...latin1("AutoCAD Binary DXF\r\n\x1a\x00")];
    const push = (code: number, value: string | number, kind: "s" | "d" | "h" | "b" = "s") => {
      parts.push(code & 0xff, code >> 8);
      const view = new DataView(new ArrayBuffer(8));
      if (kind === "d") {
        view.setFloat64(0, value as number, true);
        parts.push(...new Uint8Array(view.buffer));
      } else if (kind === "h") {
        view.setInt16(0, value as number, true);
        parts.push(...new Uint8Array(view.buffer, 0, 2));
      } else if (kind === "b") parts.push(value as number);
      else parts.push(...latin1(String(value)), 0);
    };
    push(0, "SECTION");
    push(2, "HEADER");
    push(9, "$ACADVER");
    push(1, "AC1015");
    push(0, "ENDSEC");
    push(0, "SECTION");
    push(2, "TABLES");
    push(0, "LAYER");
    push(2, "A");
    push(62, 3, "h");
    push(290, 0, "b");
    push(370, 35, "h");
    push(0, "ENDSEC");
    push(0, "SECTION");
    push(2, "ENTITIES");
    push(0, "LINE");
    push(8, "A");
    for (const [code, value] of [
      [10, 1],
      [20, 2],
      [30, 0],
      [11, 4],
      [21, 6],
      [31, 0],
    ] as const) {
      push(code, value, "d");
    }
    push(0, "ENDSEC");
    push(0, "EOF");
    const doc = parseDxf(Uint8Array.from(parts));
    expect(doc.binary).toBe(true);
    expect(doc.layers[0]).toMatchObject({ name: "A", color_index: 3, plot: false, lineweight: 35 });
    expect(doc.entities[0]).toMatchObject({ type: "LINE", layer: "A", start: [1, 2, 0], end: [4, 6, 0] });
  });
});

/**
 * ブロックの入れ子は、基点・尺度・回転と、レイヤ 0 の図形が配置元のレイヤに属すること、
 * BYBLOCK が一番内側の配置の色を継ぐことで結果が決まる。$INSUNITS（m）は mm に直して描く。
 */
describe("DXF の単位とブロックの入れ子", () => {
  // biome-ignore format: DXF のタグ（グループコードと値）の組を 1 行に並べて読めるようにする
  const source = ascii(
    0, "SECTION", 2, "HEADER", 9, "$ACADVER", 1, "AC1015", 9, "$INSUNITS", 70, 6, 0, "ENDSEC",
    0, "SECTION", 2, "TABLES", 0, "TABLE", 2, "LAYER",
    0, "LAYER", 2, "0", 70, 0, 62, 7, 6, "CONTINUOUS",
    0, "LAYER", 2, "A", 70, 0, 62, 1, 6, "CONTINUOUS",
    0, "ENDTAB", 0, "ENDSEC",
    0, "SECTION", 2, "BLOCKS",
    0, "BLOCK", 8, "0", 2, "INNER", 70, 0, 10, 1, 20, 0, 30, 0,
    0, "LINE", 5, "B1", 8, "0", 62, 0, 10, 1, 20, 0, 30, 0, 11, 2, 21, 0, 31, 0,
    0, "ENDBLK",
    0, "BLOCK", 8, "0", 2, "OUTER", 70, 0, 10, 0, 20, 0, 30, 0,
    0, "INSERT", 5, "B2", 8, "0", 2, "INNER", 10, 0, 20, 0, 30, 0, 41, 2, 42, 2,
    0, "ENDBLK",
    0, "ENDSEC",
    0, "SECTION", 2, "ENTITIES",
    0, "INSERT", 5, "20", 8, "A", 62, 3, 2, "OUTER", 10, 10, 20, 0, 30, 0, 50, 90,
    0, "ENDSEC", 0, "EOF",
  );

  it("基点・尺度・回転を重ね、m を mm にし、実寸を計測できる", () => {
    const scene = readDrawing(latin1(source), [dxf]);
    const line = scene.entities.find((entity) => entity.id === "20>B2>B1");
    expect(stroke(line)).toEqual([10000, 0, 10000, 2000]);
    const distance = measureDistance({ x: 10000, y: 0, entity: line }, { x: 10000, y: 2000, entity: line });
    expect(distance.real).toBeCloseTo(2000);
  });

  it("レイヤ 0 の図形は配置元のレイヤ A に属し、BYBLOCK は内側の配置（BYLAYER のレイヤ A の赤）を継ぐ", () => {
    const scene = readDrawing(latin1(source), [dxf]);
    const line = scene.entities.find((entity) => entity.id === "20>B2>B1");
    const layerA = scene.layers.find((layer) => layer.name === "A");
    expect(line?.layers).toEqual([layerA?.id]);
    expect(line?.primitives[0]).toMatchObject({ kind: "stroke", color: "#ff0000" });
  });
});

/**
 * レイアウトのビューポートは、高さ / 表示の高さ（DXF Reference「VIEWPORT」の 41 / 45）の縮尺で
 * モデル空間を描く。縮尺を誤ると、用紙の上では正しく見えたまま実寸の計測だけが狂う。
 */
describe("DXF のレイアウト", () => {
  // biome-ignore format: DXF のタグ（グループコードと値）の組を 1 行に並べて読めるようにする
  const source = ascii(
    0, "SECTION", 2, "HEADER", 9, "$ACADVER", 1, "AC1015", 9, "$INSUNITS", 70, 4, 0, "ENDSEC",
    0, "SECTION", 2, "TABLES", 0, "TABLE", 2, "BLOCK_RECORD",
    0, "BLOCK_RECORD", 5, "1E", 2, "*Paper_Space",
    0, "BLOCK_RECORD", 5, "1F", 2, "*Model_Space",
    0, "ENDTAB", 0, "ENDSEC",
    0, "SECTION", 2, "ENTITIES",
    0, "LINE", 5, "40", 330, "1F", 100, "AcDbEntity", 8, "0", 100, "AcDbLine", 10, 0, 20, 0, 30, 0, 11, 1000, 21, 0, 31, 0,
    // レイアウトの全体を表すビューポート（ID 1）と、縮尺 1:100 のビューポート
    0, "VIEWPORT", 5, "41", 330, "1E", 100, "AcDbEntity", 67, 1, 8, "0", 100, "AcDbViewport",
    10, 148.5, 20, 105, 30, 0, 40, 297, 41, 210, 68, 1, 69, 1, 12, 148.5, 22, 105, 45, 210,
    0, "VIEWPORT", 5, "42", 330, "1E", 100, "AcDbEntity", 67, 1, 8, "0", 100, "AcDbViewport",
    10, 100, 20, 100, 30, 0, 40, 100, 41, 50, 68, 2, 69, 2, 12, 500, 22, 0, 17, 0, 27, 0, 37, 0, 45, 5000,
    0, "ENDSEC",
    0, "SECTION", 2, "OBJECTS",
    0, "LAYOUT", 5, "30", 100, "AcDbPlotSettings", 1, "", 4, "A4", 40, 0, 41, 0, 42, 0, 43, 0, 44, 297, 45, 210,
    72, 1, 73, 0, 142, 1, 143, 1, 100, "AcDbLayout", 1, "Layout1", 70, 1, 71, 1, 10, 0, 20, 0, 11, 297, 21, 210,
    330, "1E",
    0, "ENDSEC", 0, "EOF",
  );

  it("ビューポートの縮尺で描き、実寸の計測はモデルの長さを返す", () => {
    const doc = parseDxf(latin1(source));
    const scene = dxfToScene(doc, { layout: "Layout1" });
    expect(scene.views?.map((view) => view.id)).toEqual(["Model", "Layout1"]);
    expect(scene.sheet).toMatchObject({ width: 297, height: 210 });
    // モデルの線は、全体を表すビューポートではなく 1:100 のビューポートにだけ描く
    const lines = scene.entities.filter((entity) => entity.source === "LINE");
    expect(lines).toHaveLength(1);
    const line = lines[0] as SceneEntity;
    expect(stroke(line)).toEqual([95, 100, 105, 100]);
    expect(line.placement?.scaleX).toBeCloseTo(0.01);
    const distance = measureDistance({ x: 95, y: 100, entity: line }, { x: 105, y: 100, entity: line });
    expect(distance.paper).toBeCloseTo(10);
    expect(distance.real).toBeCloseTo(1000);
  });
});
