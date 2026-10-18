import { type Matrix, multiply } from "./geometry.js";
import type { GlyphCell, TextPrimitive } from "./types.js";

const SANS =
  '"MS Gothic", "Hiragino Kaku Gothic ProN", "Hiragino Sans", "Yu Gothic", "Noto Sans JP", "Noto Sans CJK JP", sans-serif';
const SERIF = '"MS Mincho", "Hiragino Mincho ProN", "Yu Mincho", "Noto Serif JP", "Noto Serif CJK JP", serif';

/**
 * 図面のフォント名から CSS の font-family を選ぶ。
 * 扱えないフォントは MS ゴシック相当で表示する（SXF 実装規約 1-3）。
 */
export function fontFamilyFor(name: string | undefined): string {
  const key = name ?? "";
  let family = families.get(key);
  if (family === undefined) {
    const normalized = key.normalize("NFKC").toLowerCase();
    family = /明朝|mincho|serif|century|times/.test(normalized) ? SERIF : SANS;
    families.set(key, family);
  }
  return family;
}

const families = new Map<string, string>();

/** CP932 で 1 バイトの文字（ASCII と半角カナ）。全角の半分の幅で並べる（SXF 実装規約 1-5） */
export function isHalfWidth(char: string): boolean {
  const code = char.codePointAt(0) ?? 0;
  return (code >= 0x20 && code <= 0x7e) || (code >= 0xff61 && code <= 0xff9f) || code === 0xa5 || code === 0x203e;
}

export interface TextLayoutInput {
  text: string;
  /** 配置基点（元の座標） */
  x: number;
  y: number;
  /** 文字範囲の高さと幅 */
  height: number;
  width: number;
  spacing: number;
  /** 文字列の回転角（度、反時計回り） */
  angle: number;
  /** スラント角度（度、時計回りが正） */
  slant: number;
  /** 配置基点の位置（1:左下 2:中下 3:右下 4:左中 5:中中 6:右中 7:左上 8:中上 9:右上） */
  anchor: number;
  vertical: boolean;
  color: string;
  fontFamily: string;
}

/**
 * 文字範囲（高さ・幅・文字間隔）に収まるように 1 文字ずつ並べた文字列を作る。
 * 全角を 1、半角を 0.5 として文字の幅（縦書きは高さ）を割り振る。
 */
export function layoutText(input: TextLayoutInput, transform: Matrix): TextPrimitive | null {
  const chars = Array.from(input.text);
  if (chars.length === 0 || !(input.height > 0)) return null;
  const units = chars.map((char) => (isHalfWidth(char) ? 0.5 : 1));
  const totalUnits = units.reduce((sum, unit) => sum + unit, 0);
  const gaps = Math.max(0, chars.length - 1) * Math.max(0, input.spacing);
  const cells: GlyphCell[] = [];
  let boxWidth: number;
  let boxHeight: number;
  let glyphHeight: number;
  let scaleX: number;
  if (input.vertical) {
    boxHeight = input.height;
    boxWidth = input.width > 0 ? input.width : input.height / Math.max(totalUnits, 1);
    const unit = totalUnits > 0 && boxHeight - gaps > 0 ? (boxHeight - gaps) / totalUnits : boxWidth;
    glyphHeight = unit;
    scaleX = unit > 0 ? boxWidth / unit : 1;
    let top = boxHeight;
    chars.forEach((char, index) => {
      const cellHeight = (units[index] as number) * unit;
      cells.push({ char, x: boxWidth / 2, y: top - cellHeight / 2 });
      top -= cellHeight + Math.max(0, input.spacing);
    });
  } else {
    boxHeight = input.height;
    boxWidth = input.width > 0 ? input.width : totalUnits * input.height + gaps;
    const unit = totalUnits > 0 && boxWidth - gaps > 0 ? (boxWidth - gaps) / totalUnits : input.height;
    glyphHeight = input.height;
    scaleX = unit / input.height;
    let left = 0;
    chars.forEach((char, index) => {
      const cellWidth = (units[index] as number) * unit;
      cells.push({ char, x: left + cellWidth / 2, y: boxHeight / 2 });
      left += cellWidth + Math.max(0, input.spacing);
    });
  }
  const anchor = Number.isInteger(input.anchor) && input.anchor >= 1 && input.anchor <= 9 ? input.anchor : 1;
  const column = (anchor - 1) % 3;
  const row = Math.floor((anchor - 1) / 3);
  const offsetX = (column * boxWidth) / 2;
  const offsetY = (row * boxHeight) / 2;
  const angle = (input.angle * Math.PI) / 180;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const shear = Math.tan((Math.max(-85, Math.min(85, input.slant)) * Math.PI) / 180);
  // 文字の座標 → ベースラインを軸にスラント → 基点を原点へ → 回転 → 配置基点へ
  const local: Matrix = multiply(
    [cos, sin, -sin, cos, input.x, input.y],
    multiply([1, 0, 0, 1, -(offsetX + offsetY * shear), -offsetY], [1, 0, shear, 1, 0, 0]),
  );
  return {
    kind: "text",
    text: input.text,
    color: input.color,
    fontFamily: input.fontFamily,
    height: glyphHeight,
    scaleX: Number.isFinite(scaleX) && scaleX > 0 ? scaleX : 1,
    cells,
    boxWidth,
    boxHeight,
    matrix: multiply(transform, local),
  };
}
