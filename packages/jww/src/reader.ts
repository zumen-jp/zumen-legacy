/**
 * Jw_cad の JWW ファイル（MFC の CArchive で書いたバイナリ）を読む。
 * ヘッダの並びとクラスごとの値の並びは、Jw_cad の形式説明（jwdatafmt.txt）を読み解いた
 * ezjww（MIT、https://github.com/monozukuri-ai/ezjww）の実装を参照した。
 */

export interface JwwBase {
  group: number;
  /** 線種番号（点では 100 のとき記号の点） */
  penStyle: number;
  /** 線色番号（1〜9、SXF 対応色は 100〜356、ソリッドの任意色は 10） */
  penColor: number;
  /** 線幅（0 なら線色ごとの線幅） */
  penWidth: number;
  layer: number;
  layerGroup: number;
  flag: number;
}

export type JwwEntity =
  | ({ kind: "line"; x1: number; y1: number; x2: number; y2: number } & JwwBase)
  | ({
      kind: "arc";
      cx: number;
      cy: number;
      radius: number;
      /** 始角・円弧角・傾き（ラジアン）。円弧角が負なら時計回り */
      start: number;
      sweep: number;
      tilt: number;
      /** 扁平率（楕円の短径 / 長径） */
      flatness: number;
      full: boolean;
    } & JwwBase)
  | ({ kind: "point"; x: number; y: number; temporary: boolean; code: number; angle: number; scale: number } & JwwBase)
  | ({
      kind: "text";
      x1: number;
      y1: number;
      x2: number;
      y2: number;
      textType: number;
      sizeX: number;
      sizeY: number;
      spacing: number;
      /** 回転角（度） */
      angle: number;
      font: string;
      content: string;
    } & JwwBase)
  | ({
      kind: "solid";
      points: [number, number, number, number, number, number, number, number];
      color: number | null;
    } & JwwBase)
  | ({
      kind: "circle-solid";
      cx: number;
      cy: number;
      radius: number;
      flatness: number;
      tilt: number;
      start: number;
      sweep: number;
      mode: number;
      color: number | null;
    } & JwwBase)
  | ({
      kind: "block";
      x: number;
      y: number;
      scaleX: number;
      scaleY: number;
      /** 回転角（ラジアン） */
      rotation: number;
      definition: number;
    } & JwwBase)
  | ({
      kind: "dimension";
      line: Extract<JwwEntity, { kind: "line" }>;
      text: Extract<JwwEntity, { kind: "text" }>;
      lines: Extract<JwwEntity, { kind: "line" }>[];
      points: Extract<JwwEntity, { kind: "point" }>[];
    } & JwwBase);

export interface JwwBlockDefinition {
  number: number;
  name: string;
  entities: JwwEntity[];
}

export interface JwwLayerGroup {
  name: string;
  /** 0: 非表示、1: 表示のみ、2: 編集可能、3: 書込み */
  state: number;
  /** 縮尺の分母（1/100 なら 100） */
  scale: number;
  layers: { name: string; state: number }[];
}

export interface JwwLineType {
  /** 用紙上の線分・空白の長さ（mm） */
  segments: number[];
}

export interface JwwDocument {
  version: number;
  memo: string;
  paperSize: number;
  groups: JwwLayerGroup[];
  /** 線色 0〜9 の画面色（0 は背景）。読めなければ null */
  penColors: string[] | null;
  /** 線色 100〜356 の画面色 */
  extendedColors: string[] | null;
  /** 線色 0〜9 の印刷線幅（mm）。単位が分からなければ null */
  penWidths: (number | null)[] | null;
  /** 図形ごとの線幅の単位（1/100 mm か、印刷のドット） */
  widthUnit: { hundredth: boolean; dpi: number | null };
  /** 線種番号ごとの破線の長さ（mm） */
  lineTypes: Map<number, JwwLineType>;
  entities: JwwEntity[];
  blocks: JwwBlockDefinition[];
  /** 読み取りを途中で止めた理由（読めた分は entities に残る） */
  truncated: string | null;
}

const sjis = new TextDecoder("shift_jis");
const utf16 = new TextDecoder("utf-16le");

class EndOfData extends Error {}

class Reader {
  private readonly view: DataView;
  pos: number;

  constructor(
    private readonly bytes: Uint8Array,
    start = 0,
  ) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.pos = start;
  }

  get remaining(): number {
    return this.bytes.length - this.pos;
  }

  private need(length: number): void {
    if (this.pos + length > this.bytes.length) throw new EndOfData("データが途中で終わっています");
  }

  skip(length: number): void {
    this.need(length);
    this.pos += length;
  }

  u8(): number {
    this.need(1);
    return this.view.getUint8(this.pos++);
  }

  u16(): number {
    this.need(2);
    const value = this.view.getUint16(this.pos, true);
    this.pos += 2;
    return value;
  }

  u32(): number {
    this.need(4);
    const value = this.view.getUint32(this.pos, true);
    this.pos += 4;
    return value;
  }

  i32(): number {
    this.need(4);
    const value = this.view.getInt32(this.pos, true);
    this.pos += 4;
    return value;
  }

  f64(): number {
    this.need(8);
    const value = this.view.getFloat64(this.pos, true);
    this.pos += 8;
    return value;
  }

  /**
   * MFC の CString。長さは BYTE、0xFF なら WORD、0xFFFF なら DWORD。
   * WORD の 0xFFFE は Unicode の印で、その後に長さを改めて書き、本文は UTF-16LE（JWW 700）。
   */
  cstring(): string {
    let unicode = false;
    let length = this.u8();
    if (length === 0xff) {
      length = this.u16();
      if (length === 0xfffe) {
        unicode = true;
        length = this.u8();
        if (length === 0xff) {
          length = this.u16();
          if (length === 0xffff) length = this.u32();
        }
      } else if (length === 0xffff) {
        length = this.u32();
      }
    }
    const size = unicode ? length * 2 : length;
    this.need(size);
    const slice = this.bytes.subarray(this.pos, this.pos + size);
    this.pos += size;
    return (unicode ? utf16 : sjis).decode(slice).replace(/\0+$/, "");
  }
}

/** COLORREF（0x00BBGGRR）を #rrggbb にする */
function colorref(value: number): string {
  const r = value & 0xff;
  const g = (value >> 8) & 0xff;
  const b = (value >> 16) & 0xff;
  return `#${[r, g, b].map((part) => part.toString(16).padStart(2, "0")).join("")}`;
}

/** 線種のビット列（下位 unit ビットが 1 周期）を、線分・空白の長さ（mm）にする */
function patternSegments(pattern: number, unit: number, printerPitch: number): number[] {
  if (!(unit >= 1 && unit <= 32)) return [];
  const bit = (index: number) => ((pattern >>> (index % unit)) & 1) === 1;
  let start = -1;
  for (let i = 0; i < unit; i++) {
    if (bit(i) && !bit(i + unit - 1)) {
      start = i;
      break;
    }
  }
  if (start < 0) return [];
  const runs: number[] = [];
  let current = true;
  let length = 0;
  for (let offset = 0; offset < unit; offset++) {
    const value = bit(start + offset);
    if (value === current) length++;
    else {
      runs.push(length);
      current = value;
      length = 1;
    }
  }
  runs.push(length);
  // 1 ビットは印刷ピッチ × 1/32 mm
  const scale = printerPitch / 32;
  return runs.map((run) => run * scale);
}

function readBase(reader: Reader, version: number): JwwBase {
  return {
    group: reader.u32(),
    penStyle: reader.u8(),
    penColor: reader.u16(),
    penWidth: version >= 351 ? reader.u16() : 0,
    layer: reader.u16(),
    layerGroup: reader.u16(),
    flag: reader.u16(),
  };
}

function readLine(reader: Reader, version: number): Extract<JwwEntity, { kind: "line" }> {
  const base = readBase(reader, version);
  return { kind: "line", ...base, x1: reader.f64(), y1: reader.f64(), x2: reader.f64(), y2: reader.f64() };
}

function readPoint(reader: Reader, version: number): Extract<JwwEntity, { kind: "point" }> {
  const base = readBase(reader, version);
  const x = reader.f64();
  const y = reader.f64();
  const temporary = reader.u32() !== 0;
  const symbol = base.penStyle === 100;
  return {
    kind: "point",
    ...base,
    x,
    y,
    temporary,
    code: symbol ? reader.u32() : 0,
    angle: symbol ? reader.f64() : 0,
    scale: symbol ? reader.f64() : 0,
  };
}

function readText(reader: Reader, version: number): Extract<JwwEntity, { kind: "text" }> {
  const base = readBase(reader, version);
  return {
    kind: "text",
    ...base,
    x1: reader.f64(),
    y1: reader.f64(),
    x2: reader.f64(),
    y2: reader.f64(),
    textType: reader.u32(),
    sizeX: reader.f64(),
    sizeY: reader.f64(),
    spacing: reader.f64(),
    angle: reader.f64(),
    font: reader.cstring(),
    content: reader.cstring(),
  };
}

function readEntityBody(reader: Reader, className: string, version: number): JwwEntity {
  switch (className) {
    case "CDataSen":
      return readLine(reader, version);
    case "CDataEnko": {
      const base = readBase(reader, version);
      return {
        kind: "arc",
        ...base,
        cx: reader.f64(),
        cy: reader.f64(),
        radius: reader.f64(),
        start: reader.f64(),
        sweep: reader.f64(),
        tilt: reader.f64(),
        flatness: reader.f64(),
        full: reader.u32() !== 0,
      };
    }
    case "CDataTen":
      return readPoint(reader, version);
    case "CDataMoji":
      return readText(reader, version);
    case "CDataSolid": {
      const base = readBase(reader, version);
      const values = Array.from({ length: 8 }, () => reader.f64()) as [
        number,
        number,
        number,
        number,
        number,
        number,
        number,
        number,
      ];
      const color = base.penColor === 10 ? reader.u32() : null;
      if (base.penStyle >= 101) {
        const [cx, cy, radius, flatness, tilt, start, sweep, mode] = values;
        return { kind: "circle-solid", ...base, cx, cy, radius, flatness, tilt, start, sweep, mode, color };
      }
      // JWW 形式説明「ソリッドデータ」CDataSolid::Serialize の格納順で輪郭をたどる。
      // メンバー名の点番号で並べ替えると矩形が交差する（Jw_cad で照合。docs/formats/jww.md）。
      return { kind: "solid", ...base, points: values, color };
    }
    case "CDataBlock": {
      const base = readBase(reader, version);
      return {
        kind: "block",
        ...base,
        x: reader.f64(),
        y: reader.f64(),
        scaleX: reader.f64(),
        scaleY: reader.f64(),
        rotation: reader.f64(),
        definition: reader.u32(),
      };
    }
    case "CDataSunpou": {
      const base = readBase(reader, version);
      const line = readLine(reader, version);
      const text = readText(reader, version);
      const lines: Extract<JwwEntity, { kind: "line" }>[] = [];
      const points: Extract<JwwEntity, { kind: "point" }>[] = [];
      if (version >= 420) {
        reader.u16();
        for (let i = 0; i < 2; i++) lines.push(readLine(reader, version));
        for (let i = 0; i < 4; i++) points.push(readPoint(reader, version));
      }
      return { kind: "dimension", ...base, line, text, lines, points };
    }
    default:
      throw new Error(`対応していないクラス ${className}`);
  }
}

/**
 * CArchive のオブジェクトの表。クラスとオブジェクトは同じ通し番号（1 から）を使い、
 * 図形の一覧・ブロック定義・ブロックの中の一覧で共有する。
 */
class ArchiveTable {
  private readonly classes = new Map<number, string>();
  private next = 1;

  register(name: string): void {
    this.classes.set(this.next++, name);
  }

  resolve(pid: number): string | undefined {
    return this.classes.get(pid);
  }

  reserve(): void {
    this.next++;
  }
}

/** オブジェクトの先頭のタグ。新しいクラスならクラス名、既知のクラスならその名前、空なら null */
function readClass(reader: Reader, table: ArchiveTable): string | null {
  const word = reader.u16();
  if (word === 0xffff) {
    reader.u16(); // スキーマ（版）
    const length = reader.u16();
    let name = "";
    for (let i = 0; i < length; i++) name += String.fromCharCode(reader.u8());
    table.register(name);
    return name;
  }
  const tag = word === 0x7fff ? reader.u32() : ((word & 0x8000) << 16) | (word & 0x7fff);
  if ((tag & 0x80000000) !== 0) {
    const pid = tag & 0x7fffffff;
    if (pid === 0) return null;
    const name = table.resolve(pid);
    if (!name) throw new Error(`未登録のクラス番号 ${pid}`);
    return name;
  }
  if (tag === 0) return null;
  throw new Error("既に読んだオブジェクトへの参照には対応していません");
}

function readCount(reader: Reader): number {
  const word = reader.u16();
  return word === 0xffff ? reader.u32() : word;
}

function readEntityList(reader: Reader, version: number, table: ArchiveTable, out: JwwEntity[]): void {
  const count = readCount(reader);
  for (let i = 0; i < count; i++) {
    const className = readClass(reader, table);
    if (className === null) continue;
    out.push(readEntityBody(reader, className, version));
    table.reserve();
  }
}

/** 最初の図形クラス（FF FF・版・名前の長さ・"CData"）の直前にある件数の位置 */
function findEntityList(bytes: Uint8Array): number {
  for (let i = 100; i + 12 < bytes.length; i++) {
    if (bytes[i] !== 0xff || bytes[i + 1] !== 0xff) continue;
    const length = (bytes[i + 4] as number) | ((bytes[i + 5] as number) << 8);
    if (length < 8 || length > 32) continue;
    if (
      bytes[i + 6] !== 0x43 ||
      bytes[i + 7] !== 0x44 ||
      bytes[i + 8] !== 0x61 ||
      bytes[i + 9] !== 0x74 ||
      bytes[i + 10] !== 0x61
    ) {
      continue;
    }
    // 65535 件以上は WORD の 0xFFFF と DWORD の件数
    if (i >= 6 && bytes[i - 6] === 0xff && bytes[i - 5] === 0xff) {
      const count = new DataView(bytes.buffer, bytes.byteOffset + i - 4, 4).getUint32(0, true);
      if (count >= 0xffff && count * 20 <= bytes.length) return i - 6;
    }
    return i - 2;
  }
  return -1;
}

interface DisplaySettings {
  penColors: string[];
  extendedColors: string[] | null;
  penWidths: (number | null)[] | null;
  lineTypes: Map<number, JwwLineType>;
  widthUnit: JwwDocument["widthUnit"];
}

/**
 * レイヤ名の後の、画面色・線幅・線種の設定。版 300 より前は天空図（16 バイト）、マークジャンプ 5〜8 とジャンプの
 * レイヤグループ（ジャンプは 24 バイトが 4 つ）、文字の描画状態（56 バイト）がない（jwdatafmt.txt の注記）
 */
function readDisplaySettings(reader: Reader, version: number, maxDrawWidth: number): DisplaySettings {
  // 日影、天空図、2.5D の単位、画面と範囲記憶の倍率・原点、マークジャンプ、文字の描画状態、複線の間隔と留線出
  const [sky, jumps, text]: [number, number, number] = version >= 300 ? [16, 8 * 28, 56] : [0, 4 * 24, 0];
  reader.skip(28 + sky + 4 + 24 + 24 + jumps + text + 80 + 8);
  const penColors: string[] = [];
  for (let i = 0; i < 10; i++) {
    penColors.push(colorref(reader.u32()));
    reader.u32();
  }
  const printerWidths: number[] = [];
  for (let i = 0; i < 10; i++) {
    reader.u32();
    printerWidths.push(reader.u32());
    reader.f64();
  }
  const lineTypes = new Map<number, JwwLineType>();
  for (let i = 0; i < 8; i++) {
    const pattern = reader.u32();
    const unit = reader.u32();
    reader.u32();
    const printerPitch = reader.u32();
    lineTypes.set(2 + i, { segments: patternSegments(pattern, unit, printerPitch) });
  }
  reader.skip(5 * 20); // ランダム線
  for (let i = 0; i < 4; i++) {
    const pattern = reader.u32();
    const unit = reader.u32();
    reader.u32();
    const printerPitch = reader.u32();
    lineTypes.set(16 + i, { segments: patternSegments(pattern, unit, printerPitch) });
  }
  // 線幅の単位：m_nMaxDrawWid が負なら 1/100 mm、そうでなければ印刷のドット
  let dpi: number | null = null;
  let extendedColors: string[] | null = null;
  if (version >= 420) {
    const flags = Array.from({ length: 11 }, () => reader.u32());
    const last = flags[10] ?? 0;
    dpi = last >= 20 && last < 30 ? 600 : last >= 10 && last < 20 ? 300 : null;
    reader.skip(20 + 40 + 32 + 8);
    extendedColors = [];
    for (let i = 0; i < 257; i++) {
      extendedColors.push(colorref(reader.u32()));
      reader.u32();
    }
    try {
      for (let i = 0; i < 257; i++) {
        reader.cstring();
        reader.skip(16);
      }
      reader.skip(33 * 16);
      for (let i = 0; i < 33; i++) {
        reader.cstring();
        const count = reader.u32();
        const lengths = Array.from({ length: 10 }, () => reader.f64());
        if (count <= 10 && lengths.every((value) => Number.isFinite(value) && value >= 0)) {
          lineTypes.set(30 + i, { segments: lengths.slice(0, count) });
        }
      }
    } catch {
      // SXF 対応線種の欄が読めなくても、色と標準の線種は使う
    }
  }
  const hundredth = maxDrawWidth < 0;
  const penWidths = printerWidths.map((width) => (hundredth ? width / 100 : dpi ? (width * 25.4) / dpi : null));
  return { penColors, extendedColors, penWidths, lineTypes, widthUnit: { hundredth, dpi } };
}

export function isJww(bytes: Uint8Array): boolean {
  const signature = "JwwData.";
  for (let i = 0; i < signature.length; i++) if (bytes[i] !== signature.charCodeAt(i)) return false;
  return true;
}

export function readJww(bytes: Uint8Array): JwwDocument {
  if (!isJww(bytes)) throw new Error("JWW ファイルではありません");
  const reader = new Reader(bytes, 8);
  const version = reader.u32();
  const memo = reader.cstring();
  const paperSize = reader.u32();
  reader.u32(); // 書込みレイヤグループ
  const groups: JwwLayerGroup[] = [];
  for (let g = 0; g < 16; g++) {
    const state = reader.u32();
    reader.u32();
    const scale = reader.f64();
    reader.u32();
    const layers: JwwLayerGroup["layers"] = [];
    for (let l = 0; l < 16; l++) {
      const layerState = reader.u32();
      reader.u32();
      layers.push({ name: "", state: layerState });
    }
    groups.push({ name: "", state, scale, layers });
  }
  let settings: DisplaySettings | null = null;
  try {
    reader.skip((14 + 5 + 1) * 4);
    const maxDrawWidth = reader.i32();
    reader.skip(16 + 8 + 4 + 4 + 8 + 16 + 16);
    for (const group of groups) for (const layer of group.layers) layer.name = reader.cstring();
    for (const group of groups) group.name = reader.cstring();
    settings = readDisplaySettings(reader, version, maxDrawWidth);
  } catch {
    settings = null;
  }
  groups.forEach((group, g) => {
    if (!group.name) group.name = `グループ ${g.toString(16).toUpperCase()}`;
    group.layers.forEach((layer, l) => {
      if (!layer.name) layer.name = `${g.toString(16).toUpperCase()}-${l.toString(16).toUpperCase()}`;
    });
  });

  const document: JwwDocument = {
    version,
    memo,
    paperSize,
    groups,
    penColors: settings?.penColors ?? null,
    extendedColors: settings?.extendedColors ?? null,
    penWidths: settings?.penWidths ?? null,
    widthUnit: settings?.widthUnit ?? { hundredth: false, dpi: null },
    lineTypes: settings?.lineTypes ?? new Map(),
    entities: [],
    blocks: [],
    truncated: null,
  };
  const start = findEntityList(bytes);
  if (start < 0) return document;
  const body = new Reader(bytes, start);
  const table = new ArchiveTable();
  try {
    readEntityList(body, version, table, document.entities);
  } catch (error) {
    document.truncated = error instanceof Error ? error.message : String(error);
    return document;
  }
  // ブロック定義の一覧（CDataList）。中に図形の一覧を持つ
  try {
    if (body.remaining >= 2) {
      const count = readCount(body);
      for (let i = 0; i < count && i < 10_000; i++) {
        const className = readClass(body, table);
        if (className === null) continue;
        table.reserve();
        readBase(body, version);
        const number = body.u32();
        body.u32(); // 参照されているか
        body.skip(4); // CTime
        const name = body.cstring();
        const entities: JwwEntity[] = [];
        readEntityList(body, version, table, entities);
        document.blocks.push({ number, name, entities });
      }
    }
  } catch (error) {
    document.truncated = `ブロック定義: ${error instanceof Error ? error.message : String(error)}`;
  }
  return document;
}
