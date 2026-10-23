import { readDrawing } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { sxf } from "../src/index.js";
import { P21_SAMPLE, p21Bytes } from "./fixtures.js";

// 用紙・レイヤ・配置・文字・寸法・塗り・深さの上限の notices は golden.test.ts が Scene 全体で確かめる。
// ここでは正解ファイルにない入力の変化を確かめる。
describe("P21 の読み取り", () => {
  it("8 ビットのまま書いた Shift_JIS を、2 バイト目が 0x5C（\\）や 0x40〜0x7E の文字も分けずに読む", () => {
    // 表 = 95 5C、ハ = 83 6E。0x82〜0x9F の先頭バイトは windows-1252 として読むと別の文字になる
    const shiftJis = readDrawing(p21Bytes(P21_SAMPLE.replace("'DIM'", "'表ハッチ'")), [sxf]);
    expect(shiftJis.layers.map((layer) => layer.name)).toEqual(["線", "表ハッチ"]);
  });
});
