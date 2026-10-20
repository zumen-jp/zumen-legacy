import type { Scene } from "@zumenjs/core";

/** 図面を描く背景。色は ViewerStyle の同じ名前の組で決める */
export type ViewerBackground = Scene["background"];

/** 背景ごとの色。値は CSS の色 */
export interface ViewerPalette {
  /** 図面の背景 */
  background: string;
  /** 選んだ図形 */
  selection: string;
  /** ポインターの下の図形（ViewerState.hovered）。null なら描かない */
  hover: string | null;
  /** 名前付きの強調（setHighlight）の色。名前ごとの色がないときに使う */
  highlight: string;
  /** 名前付きの強調の、名前ごとの色 */
  highlights: Readonly<Record<string, string>>;
  /** 計測の線と吸着点 */
  measure: string;
  /** 用紙の枠。null なら描かない */
  sheetFrame: string | null;
}

/** 見た目。CSS のカスタムプロパティ（--zumen-*）から読み、setStyle で上書きする */
export interface ViewerStyle {
  dark: ViewerPalette;
  light: ViewerPalette;
  /** fit と fitBounds の既定の余白（CSS px） */
  fitPadding: number;
}

/** setStyle に渡す上書き。渡した項目だけを変える */
export interface ViewerStylePatch {
  dark?: Partial<ViewerPalette>;
  light?: Partial<ViewerPalette>;
  fitPadding?: number;
}

/** 既定の見た目。背景は黒と白、選択は琥珀色、計測は水色と青 */
export const DEFAULT_STYLE: ViewerStyle = {
  dark: {
    background: "#000000",
    selection: "#ffb000",
    hover: null,
    highlight: "#ff4fd8",
    highlights: {},
    measure: "#5ad2ff",
    sheetFrame: "rgba(255,255,255,0.35)",
  },
  light: {
    background: "#ffffff",
    selection: "#d16d00",
    hover: null,
    highlight: "#b0008a",
    highlights: {},
    measure: "#0031d8",
    sheetFrame: "rgba(0,0,0,0.35)",
  },
  fitPadding: 24,
};

/** 色の項目と CSS のカスタムプロパティの名前（--zumen-<名前>-dark・-light） */
const PROPERTIES = {
  background: "background",
  selection: "selection",
  hover: "hover",
  highlight: "highlight",
  measure: "measure",
  sheetFrame: "sheet-frame",
} as const;

/** none で描かないことにできる項目 */
const OPTIONAL = new Set<keyof ViewerPalette>(["hover", "sheetFrame"]);

/** 要素に指定したカスタムプロパティの値。指定がなければ null */
export function cssProperty(computed: CSSStyleDeclaration, name: string): string | null {
  const value = computed.getPropertyValue(name).trim();
  return value === "" ? null : value;
}

/** 要素のカスタムプロパティから見た目を読む。指定のない項目は既定の値 */
export function readStyle(computed: CSSStyleDeclaration): ViewerStyle {
  const palette = (background: ViewerBackground): ViewerPalette => {
    const result: ViewerPalette = { ...DEFAULT_STYLE[background] };
    for (const [key, name] of Object.entries(PROPERTIES) as [keyof typeof PROPERTIES, string][]) {
      const value = cssProperty(computed, `--zumen-${name}-${background}`);
      if (value === null) continue;
      if (value === "none") {
        if (OPTIONAL.has(key)) Object.assign(result, { [key]: null });
        continue;
      }
      Object.assign(result, { [key]: value });
    }
    return result;
  };
  const padding = Number.parseFloat(cssProperty(computed, "--zumen-fit-padding") ?? "");
  return {
    dark: palette("dark"),
    light: palette("light"),
    fitPadding: Number.isFinite(padding) && padding >= 0 ? padding : DEFAULT_STYLE.fitPadding,
  };
}

/** 見た目に上書きを重ねる。名前ごとの強調の色は名前ごとに重ねる */
export function mergeStyle(style: ViewerStyle, patch: ViewerStylePatch): ViewerStyle {
  const palette = (base: ViewerPalette, over: Partial<ViewerPalette> | undefined): ViewerPalette => {
    if (!over) return base;
    const result: ViewerPalette = { ...base, highlights: { ...base.highlights, ...over.highlights } };
    for (const key of Object.keys(PROPERTIES) as (keyof typeof PROPERTIES)[]) {
      const value = over[key];
      if (value !== undefined) Object.assign(result, { [key]: value });
    }
    return result;
  };
  return {
    dark: palette(style.dark, patch.dark),
    light: palette(style.light, patch.light),
    fitPadding: patch.fitPadding ?? style.fitPadding,
  };
}

/** 上書き同士を重ねる（後のものが優先） */
export function mergePatch(base: ViewerStylePatch, patch: ViewerStylePatch): ViewerStylePatch {
  const palette = (a: Partial<ViewerPalette> | undefined, b: Partial<ViewerPalette> | undefined) =>
    a || b ? { ...a, ...b, highlights: { ...a?.highlights, ...b?.highlights } } : undefined;
  const result: ViewerStylePatch = {};
  const dark = palette(base.dark, patch.dark);
  const light = palette(base.light, patch.light);
  if (dark) result.dark = dark;
  if (light) result.light = light;
  const fitPadding = patch.fitPadding ?? base.fitPadding;
  if (fitPadding !== undefined) result.fitPadding = fitPadding;
  return result;
}

/**
 * CSS の色の明るさ（0〜1）。どの書き方の色も、1 px の Canvas に塗って sRGB の値を読む。
 * 読めない色は黒とみなす
 */
export function lightness(ctx: CanvasRenderingContext2D, color: string): number {
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillStyle = "#000000";
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0] = ctx.getImageData(0, 0, 1, 1).data;
  return (r * 0.299 + g * 0.587 + b * 0.114) / 255;
}
