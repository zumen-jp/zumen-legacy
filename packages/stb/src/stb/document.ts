import type { Notice, NoticeLevel, Story, Vec3 } from "../types.js";
import type { XmlElement } from "../xml/parse.js";
import { SteelCatalog } from "./steel.js";

export function child(element: XmlElement | undefined, name: string): XmlElement | undefined {
  return element?.children.find((item) => item.name === name);
}

export function children(element: XmlElement | undefined, name: string): XmlElement[] {
  return element?.children.filter((item) => item.name === name) ?? [];
}

/** 数値の属性。省略・数値でない値は fallback（仕様書 1.5: 型に合わない値は認めない） */
export function num(element: XmlElement | undefined, name: string, fallback = 0): number {
  const raw = element?.attributes[name];
  if (raw === undefined || raw.trim() === "") return fallback;
  const value = Number(raw);
  return Number.isFinite(value) ? value : fallback;
}

export function optionalNum(element: XmlElement | undefined, name: string): number | null {
  const raw = element?.attributes[name];
  if (raw === undefined || raw.trim() === "") return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

/** boolean 属性。"true"・"false" 以外（大文字の TRUE を含む）も読み、省略は fallback */
export function bool(element: XmlElement | undefined, name: string, fallback: boolean): boolean {
  const raw = element?.attributes[name]?.trim().toLowerCase();
  if (raw === "true" || raw === "1") return true;
  if (raw === "false" || raw === "0") return false;
  return fallback;
}

/** monolist（空白区切りの整数の並び、仕様書 1.8） */
export function idList(element: XmlElement | undefined): number[] {
  if (!element?.text) return [];
  return element.text
    .trim()
    .split(/\s+/)
    .map(Number)
    .filter((value) => Number.isInteger(value));
}

export interface StbNodeRecord {
  id: number;
  position: Vec3;
  kind: string;
  guid: string | null;
}

/** 描かない・推測で補った内容を種類ごとに数える */
export class Notices {
  private readonly items = new Map<string, Notice>();

  add(code: string, level: NoticeLevel, message: string, count = 1): void {
    if (count <= 0) return;
    const current = this.items.get(code);
    if (current) current.count += count;
    else this.items.set(code, { code, level, message, count });
  }

  list(): Notice[] {
    return [...this.items.values()].sort((a, b) =>
      a.level === b.level ? a.code.localeCompare(b.code) : a.level === "warning" ? -1 : 1,
    );
  }
}

/** ST-Bridge の要素を ID で引けるようにしたもの */
export class StbDocument {
  readonly version: string;
  /** 2.1 系（断面の要素名・梁の変断面の表し方が 2.0 系と違う） */
  readonly is21: boolean;
  readonly common: XmlElement | undefined;
  readonly model: XmlElement | undefined;
  readonly nodes = new Map<number, StbNodeRecord>();
  readonly stories: Story[] = [];
  /** 節点が属する階（StbStory の StbNodeIdList。なければ高さの近い階） */
  readonly nodeStory = new Map<number, number>();
  private readonly sections = new Map<string, Map<number, XmlElement>>();
  readonly steel: SteelCatalog;
  readonly notices = new Notices();

  constructor(readonly root: XmlElement) {
    this.version = root.attributes.version ?? "";
    const [major = 0, minor = 0] = this.version.split(".").map(Number);
    this.is21 = major > 2 || (major === 2 && minor >= 1);
    this.common = child(root, "StbCommon");
    this.model = child(root, "StbModel");

    for (const node of children(child(this.model, "StbNodes"), "StbNode")) {
      const id = num(node, "id", Number.NaN);
      if (!Number.isInteger(id)) continue;
      this.nodes.set(id, {
        id,
        position: [num(node, "X"), num(node, "Y"), num(node, "Z")],
        kind: node.attributes.kind ?? "",
        guid: node.attributes.guid ?? null,
      });
    }

    for (const story of children(child(this.model, "StbStories"), "StbStory")) {
      const id = num(story, "id", Number.NaN);
      if (!Number.isInteger(id)) continue;
      this.stories.push({
        id,
        name: story.attributes.name ?? String(id),
        height: num(story, "height"),
        kind: story.attributes.kind ?? "",
        levelName: story.attributes.level_name ?? null,
        strengthConcrete: story.attributes.strength_concrete ?? null,
      });
      for (const item of children(child(story, "StbNodeIdList"), "StbNodeId")) {
        const nodeId = num(item, "id", Number.NaN);
        if (Number.isInteger(nodeId) && !this.nodeStory.has(nodeId)) this.nodeStory.set(nodeId, id);
      }
    }
    this.stories.sort((a, b) => a.height - b.height);
    // 階の節点の一覧がない節点は、代表高さが最も近い階に属するとみなす
    if (this.stories.length > 0) {
      for (const node of this.nodes.values()) {
        if (this.nodeStory.has(node.id)) continue;
        let best = this.stories[0] as Story;
        for (const story of this.stories) {
          if (Math.abs(story.height - node.position[2]) < Math.abs(best.height - node.position[2])) best = story;
        }
        this.nodeStory.set(node.id, best.id);
      }
    }

    const sections = child(this.model, "StbSections");
    for (const section of sections?.children ?? []) {
      if (section.name === "StbSecSteel") continue;
      const id = num(section, "id", Number.NaN);
      if (!Number.isInteger(id)) continue;
      let table = this.sections.get(section.name);
      if (!table) {
        table = new Map();
        this.sections.set(section.name, table);
      }
      table.set(id, section);
    }
    this.steel = new SteelCatalog(child(sections, "StbSecSteel"));
  }

  node(id: number): StbNodeRecord | undefined {
    return this.nodes.get(id);
  }

  section(element: string, id: number): XmlElement | undefined {
    return this.sections.get(element)?.get(id);
  }

  story(id: number | null | undefined): Story | undefined {
    return id === null || id === undefined ? undefined : this.stories.find((story) => story.id === id);
  }

  members(group: string, element: string): XmlElement[] {
    return children(child(child(this.model, "StbMembers"), group), element);
  }
}
