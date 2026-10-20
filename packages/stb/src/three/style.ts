import { MEMBER_KINDS, type MemberKind } from "../types.js";

/** 配色の組。省略すると OS の明暗の設定に合わせる */
export type StructureTheme = "light" | "dark";

/** 配色ごとの色。値は CSS の色 */
export interface StructurePalette {
  /** 背景 */
  background: string;
  /** 選んだ部材 */
  selection: string;
  /** ポインターの下の部材（StructureViewerState.hovered）。null なら描かない */
  hover: string | null;
  /** 部材の稜線 */
  edge: string;
  /** 線だけの部材（断面のない部材） */
  line: string;
  /** 通り芯の線と、名前の枠 */
  axis: string;
  /** 階の線 */
  story: string;
  /** 節点 */
  node: string;
  /** 通り芯・階の名前の文字 */
  labelText: string;
  /** 通り芯の名前の地。階の名前は 85% の濃さで敷く */
  labelBackground: string;
  /** 階の名前の文字 */
  storyLabelText: string;
}

/** 見た目。CSS のカスタムプロパティ（--zumen-*）から読み、setStyle で上書きする */
export interface StructureStyle {
  dark: StructurePalette;
  light: StructurePalette;
  /** 部材の種類ごとの色（colorMode が "kind" のとき） */
  kinds: Readonly<Record<MemberKind, string>>;
  /** 構造種別（RC・S など）ごとの色（colorMode が "structure" のとき） */
  structures: Readonly<Record<string, string>>;
  /** 色の決まっていない構造種別の色 */
  structureFallback: string;
}

/** setStyle に渡す上書き。渡した項目だけを変える */
export interface StructureStylePatch {
  dark?: Partial<StructurePalette>;
  light?: Partial<StructurePalette>;
  kinds?: Partial<Record<MemberKind, string>>;
  structures?: Readonly<Record<string, string>>;
  structureFallback?: string;
}

/** 既定の見た目。部材の色は明るい背景・暗い背景のどちらでも見分けやすい中間の明るさにする */
export const DEFAULT_STRUCTURE_STYLE: StructureStyle = {
  light: {
    background: "#f4f5f7",
    selection: "#ffb000",
    hover: null,
    edge: "#2b2f36",
    line: "#c0187a",
    axis: "#7a2e2e",
    story: "#5b6470",
    node: "#1a1a1c",
    labelText: "#1a1a1c",
    labelBackground: "#ffffff",
    storyLabelText: "#55585e",
  },
  dark: {
    background: "#1b1c1f",
    selection: "#ffc94a",
    hover: null,
    edge: "#0c0d0f",
    line: "#ff6fbf",
    axis: "#e08a8a",
    story: "#9aa3ad",
    node: "#f1f2f4",
    labelText: "#f1f2f4",
    labelBackground: "#1f2023",
    storyLabelText: "#b6b9bf",
  },
  kinds: {
    column: "#d2603d",
    post: "#e39552",
    girder: "#2f6fd6",
    beam: "#37a2c4",
    brace: "#3a9a5b",
    slab: "#9aa3ad",
    wall: "#c4ad86",
    footing: "#8a6d4e",
    stripFooting: "#9c7c5a",
    pile: "#76629a",
    foundationColumn: "#b07a52",
    parapet: "#b59f78",
  },
  structures: {
    RC: "#8f969e",
    S: "#3b6fb6",
    SRC: "#8d5fb3",
    CFT: "#2f9c96",
    DECK: "#a5adb5",
    PRECAST: "#a99f93",
    LOAD: "#c9c9c9",
    PC: "#7d6a9b",
    UNDEFINED: "#c45c8f",
  },
  structureFallback: "#a0a0a0",
};

/** 色の項目と CSS のカスタムプロパティの名前（--zumen-<名前>-dark・-light） */
const PROPERTIES = {
  background: "background",
  selection: "selection",
  hover: "hover",
  edge: "edge",
  line: "member-line",
  axis: "axis",
  story: "story",
  node: "node",
  labelText: "label-text",
  labelBackground: "label-background",
  storyLabelText: "story-label-text",
} as const;

/** none で描かないことにできる項目 */
const OPTIONAL = new Set<keyof StructurePalette>(["hover"]);

/** stripFooting → strip-footing */
function kebab(name: string): string {
  return name.replace(/[A-Z]/g, (letter) => `-${letter.toLowerCase()}`);
}

function cssProperty(computed: CSSStyleDeclaration, name: string): string | null {
  const value = computed.getPropertyValue(name).trim();
  return value === "" ? null : value;
}

/**
 * 要素のカスタムプロパティから見た目を読む。指定のない項目は既定の値。
 * 部材の色は --zumen-member-<種類>（strip-footing など）、構造種別の色は --zumen-structure-<種別>（rc・s など、小文字）
 */
export function readStructureStyle(computed: CSSStyleDeclaration): StructureStyle {
  const palette = (theme: StructureTheme): StructurePalette => {
    const result: StructurePalette = { ...DEFAULT_STRUCTURE_STYLE[theme] };
    for (const [key, name] of Object.entries(PROPERTIES) as [keyof typeof PROPERTIES, string][]) {
      const value = cssProperty(computed, `--zumen-${name}-${theme}`);
      if (value === null) continue;
      if (value === "none") {
        if (OPTIONAL.has(key)) Object.assign(result, { [key]: null });
        continue;
      }
      Object.assign(result, { [key]: value });
    }
    return result;
  };
  const kinds = { ...DEFAULT_STRUCTURE_STYLE.kinds };
  for (const kind of MEMBER_KINDS) {
    const value = cssProperty(computed, `--zumen-member-${kebab(kind)}`);
    if (value !== null) kinds[kind] = value;
  }
  const structures: Record<string, string> = { ...DEFAULT_STRUCTURE_STYLE.structures };
  for (const key of Object.keys(structures)) {
    const value = cssProperty(computed, `--zumen-structure-${key.toLowerCase()}`);
    if (value !== null) structures[key] = value;
  }
  return {
    dark: palette("dark"),
    light: palette("light"),
    kinds,
    structures,
    structureFallback: cssProperty(computed, "--zumen-structure-fallback") ?? DEFAULT_STRUCTURE_STYLE.structureFallback,
  };
}

/** 見た目に上書きを重ねる */
export function mergeStructureStyle(style: StructureStyle, patch: StructureStylePatch): StructureStyle {
  const palette = (base: StructurePalette, over: Partial<StructurePalette> | undefined): StructurePalette => {
    if (!over) return base;
    const result: StructurePalette = { ...base };
    for (const key of Object.keys(PROPERTIES) as (keyof typeof PROPERTIES)[]) {
      const value = over[key];
      if (value !== undefined) Object.assign(result, { [key]: value });
    }
    return result;
  };
  return {
    dark: palette(style.dark, patch.dark),
    light: palette(style.light, patch.light),
    kinds: { ...style.kinds, ...patch.kinds } as Record<MemberKind, string>,
    structures: { ...style.structures, ...patch.structures },
    structureFallback: patch.structureFallback ?? style.structureFallback,
  };
}

/** 上書き同士を重ねる（後のものが優先） */
export function mergeStructurePatch(base: StructureStylePatch, patch: StructureStylePatch): StructureStylePatch {
  const result: StructureStylePatch = {};
  if (base.dark || patch.dark) result.dark = { ...base.dark, ...patch.dark };
  if (base.light || patch.light) result.light = { ...base.light, ...patch.light };
  if (base.kinds || patch.kinds) result.kinds = { ...base.kinds, ...patch.kinds };
  if (base.structures || patch.structures) result.structures = { ...base.structures, ...patch.structures };
  const fallback = patch.structureFallback ?? base.structureFallback;
  if (fallback !== undefined) result.structureFallback = fallback;
  return result;
}

/**
 * CSS の色を 0xRRGGBB にする。どの書き方の色も、1 px の Canvas に塗って sRGB の値を読む。読めない色は黒とみなす
 */
export function rgb(ctx: CanvasRenderingContext2D | null, color: string): number {
  if (!ctx) return 0;
  ctx.clearRect(0, 0, 1, 1);
  ctx.fillStyle = "#000000";
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, 1, 1);
  const [r = 0, g = 0, b = 0] = ctx.getImageData(0, 0, 1, 1).data;
  return (r << 16) | (g << 8) | b;
}
