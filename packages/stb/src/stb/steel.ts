import type { Profile } from "../geometry.js";
import type { XmlElement } from "../xml/parse.js";
import {
  angle,
  box,
  channel,
  doubleAngle,
  doubleChannel,
  doubleLipChannel,
  hShape,
  lipChannel,
  pipe,
  rectangle,
  solidCircle,
  tShape,
} from "./profiles.js";

export interface SteelShape {
  /** 形状名（StbSecSteel の子要素の name） */
  name: string;
  /** 要素名（StbSecRoll-H など）。形状名から推定したときは null */
  element: string | null;
  profile: Profile | null;
  /** 中空の形（角形鋼管・円形鋼管）。CFT は充填した外形で描く */
  hollow: boolean;
}

const number = (element: XmlElement, name: string): number => {
  const value = Number(element.attributes[name]);
  return Number.isFinite(value) ? value : 0;
};

/** StbSecSteel の子要素から断面を作る（仕様書 6.26） */
function fromElement(element: XmlElement, filled: boolean): Profile | null {
  const n = (name: string) => number(element, name);
  const backToBack = element.attributes.type !== "FACETOFACE";
  switch (element.name) {
    case "StbSecRoll-H":
    case "StbSecBuild-H":
      return hShape(n("A"), n("B"), n("t1"), n("t2"));
    case "StbSecBuild-HAsymmetric":
    case "StbSecBuild-HAssymmetric":
      return hShape(n("A"), n("B_T"), n("t1"), n("t2_T"), n("B_B"), n("t2_B"));
    case "StbSecRoll-BOX":
      return box(n("A"), n("B"), n("t"), n("t"), n("r"), filled);
    case "StbSecBuild-BOX":
      return box(n("A"), n("B"), n("t1"), n("t2"), 0, filled);
    case "StbSecPipe":
      return pipe(n("D"), n("t"), filled);
    case "StbSecRoll-T":
    case "StbSecBuild-T":
      return tShape(n("A"), n("B"), n("t1"), n("t2"));
    case "StbSecRoll-C":
      return channel(n("A"), n("B"), n("t1"), n("t2"));
    case "StbSecRoll-2C":
      return doubleChannel(n("A"), n("B"), n("t1"), n("t2"), n("gap"), backToBack);
    case "StbSecRoll-L":
      return angle(n("A"), n("B"), n("t1"), n("t2"));
    case "StbSecRoll-2L":
      return doubleAngle(n("A"), n("B"), n("t1"), n("t2"), n("gap"), backToBack);
    case "StbSecLipC":
      return lipChannel(n("H"), n("A"), n("C"), n("t"));
    case "StbSecLip2C":
      return doubleLipChannel(n("H"), n("A"), n("C"), n("t"), n("gap"), backToBack);
    case "StbSecFlatBar":
      // 幅の方向を基準方向とする（6.26.15）
      return rectangle(n("t"), n("B"));
    case "StbSecRoundBar":
      return solidCircle(n("R"));
    default:
      return null;
  }
}

const HOLLOW = new Set(["StbSecRoll-BOX", "StbSecBuild-BOX", "StbSecPipe"]);

/** 形状名の寸法（「H-400x200x8x13」の 400, 200, 8, 13）。区切りは x・X・×・* を受け付ける */
function dimensions(name: string): number[] {
  const body = name.replace(/^[^0-9.]*/, "");
  return body
    .split(/[xX×*＊]/)
    .map((part) => Number(part.replace(/[^0-9.]/g, "")))
    .filter((value) => Number.isFinite(value) && value > 0);
}

/**
 * StbSecSteel に定義のない形状名を、規格品の呼び方から読む（仕様書 6.26 補足: 規格品の呼び方で形状を特定できるときは
 * 子要素を省略してよい）。読めない名前は null。
 */
export function shapeFromName(name: string, filled: boolean): { profile: Profile; hollow: boolean } | null {
  const text = name.trim().toUpperCase();
  const d = dimensions(text);
  const [a = 0, b = 0, c = 0, e = 0] = d;
  if (/^(B?H|SH|BH)\s*[-－]/.test(text) && d.length >= 4) return { profile: hShape(a, b, c, e), hollow: false };
  if (/^(□|BOX|BCR|BCP|STKR|口|■)/.test(text) && d.length >= 3) {
    return { profile: box(a, b, c, d.length >= 4 ? e : c, 0, filled), hollow: true };
  }
  if (/^(FB|PL)\s*[-－]/.test(text) && d.length >= 2) {
    return { profile: rectangle(Math.min(a, b), Math.max(a, b)), hollow: false };
  }
  if (/^(○|〇|P|PIPE|STK|Φ|φ|Ø)/.test(text) && d.length >= 2) return { profile: pipe(a, b, filled), hollow: true };
  if (/^(L)\s*[-－]/.test(text) && d.length >= 3)
    return { profile: angle(a, b, c, d.length >= 4 ? e : c), hollow: false };
  if (/^(\[|C)\s*[-－]/.test(text) && d.length >= 4) {
    // C-100x50x20x2.3 はリップ溝形鋼の呼び方。板厚がリップ長より小さいときはリップ溝形鋼とみなす
    if (e < c) return { profile: lipChannel(a, b, c, e), hollow: false };
    return { profile: channel(a, b, c, e), hollow: false };
  }
  if (/^(C?T)\s*[-－]/.test(text) && d.length >= 4) return { profile: tShape(a, b, c, e), hollow: false };
  if (/^(RB|M|R|Φ|φ)/.test(text) && d.length === 1) return { profile: solidCircle(a), hollow: false };
  return null;
}

/** 形状名から鉄骨断面を引く。filled は CFT の外形（中を塗る） */
export class SteelCatalog {
  private readonly shapes = new Map<string, XmlElement>();
  private readonly cache = new Map<string, SteelShape>();

  constructor(steel: XmlElement | undefined) {
    for (const child of steel?.children ?? []) {
      const name = child.attributes.name;
      if (name) this.shapes.set(name, child);
    }
  }

  get(name: string, filled = false): SteelShape {
    const key = `${filled ? "F" : "H"}:${name}`;
    const cached = this.cache.get(key);
    if (cached) return cached;
    const element = this.shapes.get(name);
    let shape: SteelShape;
    if (element) {
      shape = { name, element: element.name, profile: fromElement(element, filled), hollow: HOLLOW.has(element.name) };
    } else {
      const parsed = shapeFromName(name, filled);
      shape = { name, element: null, profile: parsed?.profile ?? null, hollow: parsed?.hollow ?? false };
    }
    this.cache.set(key, shape);
    return shape;
  }

  /** 形状の要素の属性（属性の表示に使う） */
  attributes(name: string): Record<string, string> | null {
    return this.shapes.get(name)?.attributes ?? null;
  }
}
