import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { cp932 } from "../../../tests/cp932.js";
import { jww } from "../src/index.js";

/**
 * JWW は MFC の CArchive で、クラス番号を図形の一覧・ブロック定義の間で共有する。
 * 番号の数え方、用紙の中心が原点の座標、レイヤグループの縮尺を読み違えると、
 * 図形が黙って欠けたり、位置・実寸がずれたりするため、最小の図面を組み立てて確かめる。
 */
class Writer {
  readonly bytes: number[] = [];
  constructor(private readonly unicode: boolean) {}
  u8(value: number): void {
    this.bytes.push(value & 0xff);
  }
  u16(value: number): void {
    this.u8(value);
    this.u8(value >> 8);
  }
  u32(value: number): void {
    const view = new DataView(new ArrayBuffer(4));
    view.setUint32(0, value >>> 0, true);
    this.bytes.push(...new Uint8Array(view.buffer));
  }
  i32(value: number): void {
    this.u32(value >>> 0);
  }
  f64(value: number): void {
    const view = new DataView(new ArrayBuffer(8));
    view.setFloat64(0, value, true);
    this.bytes.push(...new Uint8Array(view.buffer));
  }
  zeros(count: number): void {
    for (let i = 0; i < count; i++) this.u8(0);
  }
  ascii(text: string): void {
    for (const char of text) this.u8(char.charCodeAt(0));
  }
  /** CString。版 700 は FF FE FF の後に文字数と UTF-16LE、それより前は CP932 */
  cstring(text: string): void {
    if (this.unicode) {
      this.u8(0xff);
      this.u16(0xfffe);
      this.u8(text.length);
      for (const char of text) this.u16(char.charCodeAt(0));
      return;
    }
    const encoded = cp932(text);
    this.u8(encoded.length);
    this.bytes.push(...encoded);
  }
  newClass(name: string, version: number): void {
    this.u16(0xffff);
    this.u16(version);
    this.u16(name.length);
    this.ascii(name);
  }
  base(version: number, values: { style?: number; color?: number; layer?: number } = {}): void {
    this.u32(0);
    this.u8(values.style ?? 1);
    this.u16(values.color ?? 2);
    if (version >= 351) this.u16(0);
    this.u16(values.layer ?? 0);
    this.u16(0);
    this.u16(0);
  }
}

function build(version: number): Uint8Array {
  const w = new Writer(version >= 700);
  w.ascii("JwwData.");
  w.u32(version);
  w.cstring("メモ");
  w.u32(3); // A3
  w.u32(0);
  for (let g = 0; g < 16; g++) {
    w.u32(2);
    w.u32(0);
    w.f64(g === 0 ? 100 : 1);
    w.u32(0);
    for (let l = 0; l < 16; l++) {
      w.u32(l === 2 ? 0 : 2); // レイヤ 2 は非表示
      w.u32(0);
    }
  }
  w.zeros(80);
  w.i32(-101); // 線幅を 1/100 mm 単位とする
  w.zeros(72);
  for (let i = 0; i < 256; i++) w.cstring(i === 0 ? "通り芯" : "");
  for (let g = 0; g < 16; g++) w.cstring(g === 0 ? "平面" : "");
  w.zeros(464);
  const screen = [0xffffff, 0xc0c000, 0x000000, 0x00c000, 0x00c0c0, 0xc000c0, 0xff0000, 0x808000, 0x8000ff, 0xc0c0c0];
  for (const color of screen) {
    w.u32(color);
    w.u32(1);
  }
  for (let i = 0; i < 10; i++) {
    w.u32(0);
    w.u32(i === 2 ? 25 : 18);
    w.f64(0.5);
  }
  // 線種 2: 下位 4 ビット 0011 → 2 ビットの線と 2 ビットの空白。印刷ピッチ 32 で 1 ビット 1 mm
  w.u32(0b1100);
  w.u32(4);
  w.u32(1);
  w.u32(32);
  w.zeros(7 * 16 + 5 * 20 + 4 * 16);
  w.zeros(11 * 4 + 20 + 40 + 32 + 8);
  for (let i = 0; i < 257; i++) w.zeros(8);
  for (let i = 0; i < 257; i++) {
    w.cstring("");
    w.zeros(16);
  }
  w.zeros(33 * 16);
  for (let i = 0; i < 33; i++) {
    w.cstring("");
    w.u32(0);
    w.zeros(80);
  }
  // 図形の一覧（クラス番号: CDataSen 1、CDataMoji 4、CDataEnko 6、CDataBlock 8、CDataList 11）
  w.u16(6);
  w.newClass("CDataSen", version);
  w.base(version);
  for (const value of [-100, 0, 0, 0]) w.f64(value);
  w.u16(0x8000 | 1);
  w.base(version, { style: 2, layer: 2 });
  for (const value of [0, 20, 10, 20]) w.f64(value);
  w.newClass("CDataMoji", version);
  w.base(version, { layer: 1 });
  for (const value of [0, 10, 20, 10]) w.f64(value);
  w.u32(0);
  for (const value of [5, 5, 0, 0]) w.f64(value);
  w.cstring("Century");
  w.cstring("表示ソ能");
  w.newClass("CDataEnko", version);
  w.base(version);
  for (const value of [0, 0, 10, 0, -Math.PI / 2, 0, 1]) w.f64(value);
  w.u32(0);
  w.newClass("CDataBlock", version);
  w.base(version);
  for (const value of [50, 0, 2, 2, 0]) w.f64(value);
  w.u32(7);
  // Jw_cad 8 以降が (0, -1000) に文字として書く設定
  w.u16(0x8000 | 4);
  w.base(version);
  for (const value of [0, -1000, 30, -1000]) w.f64(value);
  w.u32(0);
  for (const value of [3, 3, 0, 0]) w.f64(value);
  w.cstring("Century");
  w.cstring("View_Direct2d = 1");
  // ブロック定義の一覧。中の線は、最初の一覧で登録した CDataSen をクラス番号で参照する
  w.u16(1);
  w.newClass("CDataList", version);
  w.base(version);
  w.u32(7);
  w.u32(1);
  w.u32(0);
  w.cstring("部品");
  w.u16(1);
  w.u16(0x8000 | 1);
  w.base(version);
  for (const value of [0, 0, 1, 0]) w.f64(value);
  w.u32(0); // 画像の件数
  return Uint8Array.from(w.bytes);
}

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

function points(item: SceneEntity): number[] {
  const primitive = item.primitives[0];
  if (primitive?.kind !== "stroke") throw new Error("線ではありません");
  return Array.from(primitive.points, (value) => Math.round(value * 1e6) / 1e6);
}

describe("JWW の読み取り", () => {
  const scene = readDrawing(build(600), [jww]);

  it("用紙の中心が原点の座標を左下原点にし、縮尺 1/100 から実寸を計測する", () => {
    expect(scene.sheet).toMatchObject({ size: "A3", width: 420, height: 297 });
    const line = entity(scene.entities, "#1");
    expect(points(line)).toEqual([110, 148.5, 210, 148.5]);
    expect(line.primitives[0]).toMatchObject({ width: 0.25, dashes: [] });
    expect(measureDistance({ x: 110, y: 148.5, entity: line }, { x: 210, y: 148.5, entity: line }).real).toBeCloseTo(
      10000,
    );
  });

  it("レイヤの名前と表示状態、線種のビット列を読む", () => {
    expect(scene.layers).toContainEqual({ id: 1, name: "0-0 通り芯", visible: true, group: "平面" });
    expect(scene.layers).toContainEqual(expect.objectContaining({ id: 3, visible: false }));
    expect(entity(scene.entities, "#2").primitives[0]).toMatchObject({ dashes: [2, 2] });
  });

  it("Jw_cad が文字として保存した設定は描かず、全体表示の範囲を用紙から広げない", () => {
    expect(scene.entities.map((item) => item.id)).not.toContain("#6");
    expect(scene.bounds.minY).toBeGreaterThanOrEqual(0);
  });

  it("円弧角が負なら時計回りに描き、ブロックの中の図形を共有のクラス番号で読む", () => {
    const arc = entity(scene.entities, "#4");
    expect(arc.bbox.minY).toBeCloseTo(138.5, 6);
    expect(arc.bbox.maxY).toBeCloseTo(148.5, 6);
    expect(points(entity(scene.entities, "#5>#1"))).toEqual([260, 148.5, 262, 148.5]);
  });

  it.each([600, 700])("版 %i の文字列（CP932・UTF-16）を復号する", (version) => {
    const text = entity(readDrawing(build(version), [jww]).entities, "#3");
    expect(text.properties).toContainEqual({ label: "文字列", value: "表示ソ能" });
  });
});
