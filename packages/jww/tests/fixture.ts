import { cp932 } from "../../../tests/cp932.js";

/**
 * JWW は MFC の CArchive で、クラス番号を図形の一覧・ブロック定義の間で共有する。
 * 番号の数え方、用紙の中心が原点の座標、レイヤグループの縮尺を確かめるための最小の図面を組み立てる。
 * 同じ図面を tests/fixtures/jww に書き出し、Rust の読み取りと結果を比べる（golden.test.ts）。
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

function header(version: number): Writer {
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
  // 日影から複線まで。版 300 より前は天空図、マークジャンプ 5〜8 とジャンプのレイヤグループ、文字の描画状態がない
  w.zeros(version >= 300 ? 464 : 264);
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
  // 印刷の設定。作図時間と 2.5D の視点は版 223、最終値は 225、ソリッドの任意色は 230、SXF 対応の色と線種は 420 から
  w.zeros(11 * 4);
  if (version >= 223) w.zeros(20 + 40);
  if (version >= 225) w.zeros(32);
  if (version >= 230) w.zeros(8);
  if (version >= 420) {
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
  }
  return w;
}

export function build(version: number): Uint8Array {
  const w = header(version);
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

/** CDataSolid serialises its boundary in storage order, despite the point numbers in its member names. */
export function buildSolids(version: number): Uint8Array {
  const w = header(version);
  const boundaries = [
    [0, 0, 0, 10, 20, 10, 20, 0],
    [30, 0, 34, 10, 49, 8, 45, -2],
    [60, 0, 65, 10, 70, 0, 70, 0],
  ];
  w.u16(boundaries.length);
  for (const [index, points] of boundaries.entries()) {
    if (index === 0) w.newClass("CDataSolid", version);
    else w.u16(0x8001);
    w.base(version, { color: 10 });
    for (const value of points) w.f64(value);
    w.u32(0xc0c0c0);
  }
  w.u16(0); // No block definitions.
  w.u32(0); // No images.
  return Uint8Array.from(w.bytes);
}

/**
 * 画像の配置（^@BM ファイル名,幅,高さ,トリミングと透過の値）の文字。文字の始点が画像の左下で、文字の向きに回る。
 * 読み違えると、画像が黙ってずれたり回ったりする
 */
export function buildImages(version: number): Uint8Array {
  const w = header(version);
  const text = (x: number, y: number, angle: number, content: string) => {
    w.base(version, { layer: 1 });
    for (const value of [x, y, x + 10, y]) w.f64(value);
    w.u32(0);
    for (const value of [3, 3, 0, angle]) w.f64(value);
    w.cstring("Century");
    w.cstring(content);
  };
  // クラス番号: CDataSen 1、CDataMoji 3、CDataBlock 8、CDataList 10
  w.u16(6);
  w.newClass("CDataSen", version);
  w.base(version);
  for (const value of [-120, -20, 40, -20]) w.f64(value);
  w.newClass("CDataMoji", version);
  // 同梱した画像（%temp%）を 40 × 20 mm、既定のトリミング・透過の値で
  text(-50, -30, 0, "^@BM%temp%photo.bmp,40,20,0,0,1,0,255,255,255");
  // 30° 回した、別のファイルの画像。トリミングの値が既定でない
  w.u16(0x8000 | 3);
  text(20, 10, 30, "^@BMC:\\images\\site.jpg,30,15,0.1,0,1,0,255,255,255");
  // 幅のない画像と、画像でない制御の文字
  w.u16(0x8000 | 3);
  text(0, 50, 0, "^@BMbroken.bmp,0,10");
  w.u16(0x8000 | 3);
  text(0, 60, 0, "^@XX");
  // ブロックの中の画像を、2 倍・90° 回して置く
  w.newClass("CDataBlock", version);
  w.base(version);
  for (const value of [-100, 50, 2, 2, Math.PI / 2]) w.f64(value);
  w.u32(7);
  w.u16(1);
  w.newClass("CDataList", version);
  w.base(version);
  w.u32(7);
  w.u32(1);
  w.u32(0);
  w.cstring("logo");
  w.u16(1);
  w.u16(0x8000 | 3);
  text(0, 0, 0, "^@BMlogo.png,10,5");
  w.u32(0); // 画像の件数
  return Uint8Array.from(w.bytes);
}
