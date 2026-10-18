import type { Notice, NoticeLevel } from "./types.js";

/** 省略・代替した内容を種類（code）ごとに数える。最初に現れた順に並べる */
export class NoticeList {
  private readonly items = new Map<string, Notice>();

  /** initial は複製して数える（読み取りの結果の notices を変えない） */
  constructor(initial: Iterable<Notice> = []) {
    for (const notice of initial) this.add(notice.level, notice.code, notice.message, notice.count);
  }

  add(level: NoticeLevel, code: string, message: string, count = 1): void {
    const existing = this.items.get(code);
    if (existing) existing.count += count;
    else this.items.set(code, { level, code, message, count });
  }

  toArray(): Notice[] {
    return [...this.items.values()].map((notice) => ({ ...notice }));
  }
}
