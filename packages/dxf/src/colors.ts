/**
 * AutoCAD Color Index（ACI）の 1〜255 の RGB。Autodesk は表を公開しておらず、出回る表は 3 通りある
 * （docs/formats/dxf.md）。ブラウザの DXF の表示部品（dxf-parser・dxf-viewer・dxf-render・mlightcad）と同じ表を計算で作る。
 * 10〜249 は 15 度ずつの色相 24 色 × 明るさ 5 段（255・204・153・127・76）× 濃・淡、250〜255 は灰色。
 * 7 は前景色（白）で、表示部品が白背景では黒にする。
 */
const LEVELS = [255, 204, 153, 127, 76] as const;
const GRAYS = [51, 91, 132, 173, 214, 255] as const;
const BASE: readonly (readonly [number, number, number])[] = [
  [0, 0, 0],
  [255, 0, 0],
  [255, 255, 0],
  [0, 255, 0],
  [0, 255, 255],
  [0, 0, 255],
  [255, 0, 255],
  [255, 255, 255],
  [128, 128, 128],
  [192, 192, 192],
];

function toHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((c) => Math.max(0, Math.min(255, c)).toString(16).padStart(2, "0")).join("")}`;
}

function build(): readonly string[] {
  const table: string[] = BASE.map(([r, g, b]) => toHex(r, g, b));
  for (let index = 10; index < 250; index++) {
    const hue = Math.floor((index - 10) / 10) * 15;
    const shade = (index - 10) % 10;
    const level = LEVELS[Math.floor(shade / 2)] as number;
    const pale = shade % 2 === 1;
    const x = 1 - Math.abs(((hue / 60) % 2) - 1);
    const sector = Math.floor(hue / 60) % 6;
    const unit = (
      [
        [1, x, 0],
        [x, 1, 0],
        [0, 1, x],
        [0, x, 1],
        [x, 0, 1],
        [1, 0, x],
      ] as const
    )[sector] as readonly [number, number, number];
    const channel = (c: number) => {
      const value = Math.floor(level * (pale ? c + (1 - c) / 2 : c));
      // AutoCAD の表は明るさ 153 の段で、色相 60 度以上の 153 を 152 にしている（dxf-parser などの表も同じ）
      return level === 153 && hue >= 60 && value === 153 ? 152 : value;
    };
    table.push(toHex(channel(unit[0]), channel(unit[1]), channel(unit[2])));
  }
  for (const gray of GRAYS) table.push(toHex(gray, gray, gray));
  return table;
}

/** ACI の色（#rrggbb）。0 は使わない */
export const ACI: readonly string[] = build();

export function aciColor(index: number): string {
  return ACI[Math.abs(Math.trunc(index))] ?? (ACI[7] as string);
}

export function trueColor(value: number): string {
  const rgb = value & 0xffffff;
  return toHex((rgb >> 16) & 0xff, (rgb >> 8) & 0xff, rgb & 0xff);
}

/** 透過度（440）。0x020000TT の TT が不透明さ（255 で不透明）。BYLAYER・BYBLOCK は不透明として扱う */
export function isTransparent(value: number | undefined): boolean {
  if (value === undefined) return false;
  return (value & 0xff000000) === 0x02000000 && (value & 0xff) < 255;
}
