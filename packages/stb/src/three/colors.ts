import type { MemberKind } from "../types.js";

/** 部材の種類ごとの色。明るい背景・暗い背景のどちらでも見分けやすい中間の明るさにする */
export const KIND_COLORS: Record<MemberKind, number> = {
  column: 0xd2603d,
  post: 0xe39552,
  girder: 0x2f6fd6,
  beam: 0x37a2c4,
  brace: 0x3a9a5b,
  slab: 0x9aa3ad,
  wall: 0xc4ad86,
  footing: 0x8a6d4e,
  stripFooting: 0x9c7c5a,
  pile: 0x76629a,
  foundationColumn: 0xb07a52,
  parapet: 0xb59f78,
};

/** 構造種別ごとの色（kind_structure） */
export const STRUCTURE_COLORS: Record<string, number> = {
  RC: 0x8f969e,
  S: 0x3b6fb6,
  SRC: 0x8d5fb3,
  CFT: 0x2f9c96,
  DECK: 0xa5adb5,
  PRECAST: 0xa99f93,
  LOAD: 0xc9c9c9,
  PC: 0x7d6a9b,
  UNDEFINED: 0xc45c8f,
};

export const STRUCTURE_FALLBACK = 0xa0a0a0;

export interface ThemeColors {
  background: number;
  edge: number;
  axis: number;
  story: number;
  node: number;
  highlight: number;
  line: number;
}

export const THEMES: Record<"light" | "dark", ThemeColors> = {
  light: {
    background: 0xf4f5f7,
    edge: 0x2b2f36,
    axis: 0x7a2e2e,
    story: 0x5b6470,
    node: 0x1a1a1c,
    highlight: 0xffb000,
    line: 0xc0187a,
  },
  dark: {
    background: 0x1b1c1f,
    edge: 0x0c0d0f,
    axis: 0xe08a8a,
    story: 0x9aa3ad,
    node: 0xf1f2f4,
    highlight: 0xffc94a,
    line: 0xff6fbf,
  },
};

export function hex(color: number): string {
  return `#${color.toString(16).padStart(6, "0")}`;
}
