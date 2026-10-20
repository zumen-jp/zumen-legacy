import type { DrawingFormat, Scene } from "./types.js";

/**
 * 形式ごとの読み取り。@zumenjs/sxf の sxf、@zumenjs/jww の jww などを readDrawing に並べて渡す。
 * 形式は拡張子ではなく中身で判定する（名前を変えたファイルや、拡張子の大文字・小文字の違いに左右されない）。
 */
export interface DrawingReader {
  /** 読める形式の名前。Scene.format と同じ値 */
  readonly formats: readonly DrawingFormat[];
  /** 中身がこの読み取りの形式なら、その名前を返す */
  detect(bytes: Uint8Array): DrawingFormat | null;
  /**
   * 表示用の図面にする。format は detect の結果か、利用者が指定した形式。
   * options.view は Scene.views の id で、省略すると形式ごとの既定の表示にする
   */
  read(bytes: Uint8Array, format: DrawingFormat, options?: ReadOptions): Scene;
  /**
   * 読む前に待つ準備（WebAssembly の読み込みなど）。持たない読み取りは、すぐに読める。
   * 何度呼んでもよく、準備ができた後はすぐに終わる。@zumenjs/react の useDrawing は、中身を認めた読み取りのものだけを待つ
   */
  load?(): Promise<void>;
}

export interface ReadOptions {
  /** 描く表示（Scene.views の id）。DXF・DWG のレイアウトなど */
  view?: string;
}

export interface ReadDrawingOptions extends ReadOptions {
  /** 分かっている場合の形式。省略すると中身から判定する */
  format?: DrawingFormat;
}

function toBytes(input: ArrayBuffer | Uint8Array): Uint8Array {
  return input instanceof Uint8Array ? input : new Uint8Array(input);
}

/** 渡した読み取りのうち、最初に中身を認めたものの形式を返す */
export function detectFormat(input: ArrayBuffer | Uint8Array, readers: readonly DrawingReader[]): DrawingFormat | null {
  const bytes = toBytes(input);
  for (const reader of readers) {
    const format = reader.detect(bytes);
    if (format) return format;
  }
  return null;
}

/** 図面のバイト列を、渡した読み取りのどれかで読み、表示用の図面にする */
export function readDrawing(
  input: ArrayBuffer | Uint8Array,
  readers: readonly DrawingReader[],
  options: ReadDrawingOptions = {},
): Scene {
  const bytes = toBytes(input);
  const known = readers.flatMap((reader) => reader.formats);
  const readOptions: ReadOptions = options.view === undefined ? {} : { view: options.view };
  if (options.format) {
    const format = options.format;
    const reader = readers.find((candidate) => candidate.formats.includes(format));
    if (!reader) throw new Error(`${format.toUpperCase()} を読む読み取りが渡されていません`);
    return reader.read(bytes, format, readOptions);
  }
  for (const reader of readers) {
    const format = reader.detect(bytes);
    if (format) return reader.read(bytes, format, readOptions);
  }
  const list = known.map((format) => format.toUpperCase()).join("・");
  throw new Error(`対応していない形式のファイルです（読める形式: ${list || "なし"}）`);
}
