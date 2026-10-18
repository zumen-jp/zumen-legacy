/** SXF Ver.3.1 共通既定義要素編の既定義色（コード 1〜16） */
export const PREDEFINED_COLORS: readonly { name: string; label: string; rgb: [number, number, number] }[] = [
  { name: "black", label: "黒", rgb: [0, 0, 0] },
  { name: "red", label: "赤", rgb: [255, 0, 0] },
  { name: "green", label: "緑", rgb: [0, 255, 0] },
  { name: "blue", label: "青", rgb: [0, 0, 255] },
  { name: "yellow", label: "黄色", rgb: [255, 255, 0] },
  { name: "magenta", label: "マジェンタ", rgb: [255, 0, 255] },
  { name: "cyan", label: "シアン", rgb: [0, 255, 255] },
  { name: "white", label: "白", rgb: [255, 255, 255] },
  { name: "deeppink", label: "牡丹", rgb: [192, 0, 128] },
  { name: "brown", label: "茶", rgb: [192, 128, 64] },
  { name: "orange", label: "橙", rgb: [255, 128, 0] },
  { name: "lightgreen", label: "薄緑", rgb: [128, 192, 128] },
  { name: "lightblue", label: "明青", rgb: [0, 128, 255] },
  { name: "lavender", label: "青紫", rgb: [128, 64, 255] },
  { name: "lightgray", label: "明灰", rgb: [192, 192, 192] },
  { name: "darkgray", label: "暗灰", rgb: [128, 128, 128] },
];

/** 既定義線種（コード 1〜15）。ピッチは線幅 0.5mm 相当の参考値（用紙の mm） */
export const PREDEFINED_LINE_TYPES: readonly { name: string; label: string; pitch: readonly number[] }[] = [
  { name: "continuous", label: "実線", pitch: [] },
  { name: "dashed", label: "破線", pitch: [6, 1.5] },
  { name: "dashed spaced", label: "跳び破線", pitch: [6, 6] },
  { name: "long dashed dotted", label: "一点長鎖線", pitch: [12, 1.5, 0.25, 1.5] },
  { name: "long dashed double-dotted", label: "二点長鎖線", pitch: [12, 1.5, 0.25, 1.5, 0.25, 1.5] },
  {
    name: "long dashed triplicate-dotted",
    label: "三点長鎖線",
    pitch: [12, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5],
  },
  { name: "dotted", label: "点線", pitch: [0.25, 1.5] },
  { name: "chain", label: "一点鎖線", pitch: [12, 1.5, 3.5, 1.5] },
  { name: "chain double dash", label: "二点鎖線", pitch: [12, 1.5, 3.5, 1.5, 3.5, 1.5] },
  { name: "dashed dotted", label: "一点短鎖線", pitch: [6, 1.5, 0.25, 1.5] },
  { name: "double-dashed dotted", label: "一点二短鎖線", pitch: [6, 1.5, 6, 1.5, 0.25, 1.5] },
  { name: "dashed double-dotted", label: "二点短鎖線", pitch: [6, 1.5, 0.25, 1.5, 0.25, 1.5] },
  {
    name: "double-dashed double-dotted",
    label: "二点二短鎖線",
    pitch: [6, 1.5, 6, 1.5, 0.25, 1.5, 0.25, 1.5],
  },
  { name: "dashed triplicate-dotted", label: "三点短鎖線", pitch: [6, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5] },
  {
    name: "double-dashed triplicate-dotted",
    label: "三点二短鎖線",
    pitch: [6, 1.5, 6, 1.5, 0.25, 1.5, 0.25, 1.5, 0.25, 1.5],
  },
];

/** 既定義線幅（コード 1〜9、mm） */
export const PREDEFINED_WIDTHS: readonly number[] = [0.13, 0.18, 0.25, 0.35, 0.5, 0.7, 1, 1.4, 2];

/** 用紙サイズ種別（JIS A 列の縦長の寸法、mm） */
export const SHEET_SIZES: Readonly<Record<number, { label: string; width: number; height: number }>> = {
  0: { label: "A0", width: 841, height: 1189 },
  1: { label: "A1", width: 594, height: 841 },
  2: { label: "A2", width: 420, height: 594 },
  3: { label: "A3", width: 297, height: 420 },
  4: { label: "A4", width: 210, height: 297 },
};

export const SFIG_KINDS: Readonly<Record<number, string>> = {
  1: "部分図（数学座標系）",
  2: "部分図（測地座標系）",
  3: "作図グループ",
  4: "作図部品",
};

/** 寸法線・引出し線の矢印コード（1〜11） */
export const ARROW_NAMES: Readonly<Record<number, string>> = {
  1: "blanked arrow",
  2: "blanked box",
  3: "blanked dot",
  4: "dimension origin",
  5: "filled box",
  6: "filled arrow",
  7: "filled dot",
  8: "integral symbol",
  9: "open arrow",
  10: "slash",
  11: "unfilled arrow",
};

/** 点マーカのコード（1〜7） */
export const MARKER_NAMES: Readonly<Record<number, string>> = {
  1: "asterisk",
  2: "circle",
  3: "dot",
  4: "plus",
  5: "square",
  6: "triangle",
  7: "X",
};

export function predefinedColorCode(name: string): number {
  const normalized = name.trim().toLowerCase();
  const index = PREDEFINED_COLORS.findIndex((color) => color.name === normalized);
  return index >= 0 ? index + 1 : 0;
}

export function predefinedLineTypeCode(name: string): number {
  const normalized = name.trim().toLowerCase();
  const index = PREDEFINED_LINE_TYPES.findIndex((type) => type.name === normalized);
  return index >= 0 ? index + 1 : 0;
}

/** 既定義線幅と一致すれば 1〜9、一致しなければ 0 */
export function predefinedWidthCode(width: number): number {
  const index = PREDEFINED_WIDTHS.findIndex((value) => Math.abs(value - width) < 1e-6);
  return index >= 0 ? index + 1 : 0;
}
