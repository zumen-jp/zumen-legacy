import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { type DxfLine, type DxfMLine, type DxfMText, type DxfText, dxf, dxfToScene, parseDxf } from "../src/index.js";
import {
  BINARY,
  BINARY_LIMITS,
  BLOCKS,
  CODEPAGE,
  LAYOUT,
  LIMITS,
  MISALIGNED,
  MLEADER,
  MLINE,
  R12_SHIFT_JIS,
  WESTERN,
} from "./fixtures.js";

function stroke(entity: SceneEntity | undefined): number[] {
  return strokes(entity)[0] ?? [];
}

function strokes(entity: SceneEntity | undefined): number[][] {
  const out = (entity?.primitives ?? []).flatMap((item) =>
    item.kind === "stroke" ? [Array.from(item.points, (value) => Math.round(value * 1e6) / 1e6)] : [],
  );
  if (out.length === 0) throw new Error("線がありません");
  return out;
}

// 文字コードの規則と、CP932 の 2 バイト目の 0x5C を書式コードと取り違えないこと（図面は fixtures.ts）
describe("DXF の文字コード", () => {
  it("$DWGCODEPAGE の ANSI_932 で復号し、\\U+XXXX を文字にする", () => {
    const doc = parseDxf(CODEPAGE);
    expect(doc.encoding).toBe("shift_jis");
    expect(doc.layers.map((layer) => layer.name)).toEqual(["平面図"]);
    const [text, mtext] = doc.entities as [DxfText, DxfMText];
    expect(text.text).toBe("表示ソ能平");
    expect(mtext.text).toBe("表\n能ソ");
  });

  it("指定のない R12 は、日本語として正しく読めるときだけ Shift_JIS とし、西欧の ° は Shift_JIS にしない", () => {
    const japanese = parseDxf(R12_SHIFT_JIS);
    expect(japanese.encoding).toBe("shift_jis");
    expect((japanese.entities[0] as DxfText).text).toBe("表示ソ能平");
    const western = parseDxf(WESTERN);
    expect(western.encoding).toBe("windows-1252");
    expect((western.entities[0] as DxfText).text).toBe("45°");
  });

  it("バイナリの DXF を、グループコードごとの値の型（2・4・8 バイト、真偽値、文字列）で読む", () => {
    const doc = parseDxf(BINARY);
    expect(doc.binary).toBe(true);
    expect(doc.layers[0]).toMatchObject({ name: "A", color_index: 3, plot: false, lineweight: 35 });
    expect(doc.entities[0]).toMatchObject({ type: "LINE", layer: "A", start: [1, 2, 0], end: [4, 6, 0] });
  });

  it("グループコードとして読めない行があれば、CRLF の図面でも次の行から読み直す", () => {
    const doc = parseDxf(MISALIGNED);
    expect(doc.notices).toContainEqual(expect.objectContaining({ code: "bad-tag", count: 1 }));
    expect(doc.entities[0] as DxfLine).toMatchObject({ type: "LINE", start: [1, 2, 0], end: [3, 4, 0] });
  });
});

/**
 * ブロックの入れ子は、基点・尺度・回転と、レイヤ 0 の図形が配置元のレイヤに属すること、
 * BYBLOCK が一番内側の配置の色を継ぐことで結果が決まる。$INSUNITS（m）は mm に直して描く。
 */
describe("DXF の単位とブロックの入れ子", () => {
  it("基点・尺度・回転を重ね、m を mm にし、実寸を計測できる", () => {
    const scene = readDrawing(BLOCKS, [dxf]);
    const line = scene.entities.find((entity) => entity.id === "20>B2>B1");
    expect(stroke(line)).toEqual([10000, 0, 10000, 2000]);
    const distance = measureDistance({ x: 10000, y: 0, entity: line }, { x: 10000, y: 2000, entity: line });
    expect(distance.real).toBeCloseTo(2000);
  });

  it("レイヤ 0 の図形は配置元のレイヤ A に属し、BYBLOCK は内側の配置（BYLAYER のレイヤ A の赤）を継ぐ", () => {
    const scene = readDrawing(BLOCKS, [dxf]);
    const line = scene.entities.find((entity) => entity.id === "20>B2>B1");
    const layerA = scene.layers.find((layer) => layer.name === "A");
    expect(line?.layers).toEqual([layerA?.id]);
    expect(line?.primitives[0]).toMatchObject({ kind: "stroke", color: "#ff0000" });
  });
});

/**
 * レイアウトのビューポートは、高さ / 表示の高さ（DXF Reference「VIEWPORT」の 41 / 45）の縮尺で
 * モデル空間を描く。縮尺を誤ると、用紙の上では正しく見えたまま実寸の計測だけが狂う。
 */
describe("DXF のレイアウト", () => {
  it("ビューポートの縮尺で描き、実寸の計測はモデルの長さを返す", () => {
    const doc = parseDxf(LAYOUT);
    const scene = dxfToScene(doc, { layout: "Layout1" });
    expect(scene.views?.map((view) => view.id)).toEqual(["Model", "Layout1"]);
    expect(scene.sheet).toMatchObject({ width: 297, height: 210 });
    // モデルの線は、全体を表すビューポートではなく 1:100 のビューポートにだけ描く
    const lines = scene.entities.filter((entity) => entity.source === "LINE");
    expect(lines).toHaveLength(1);
    const line = lines[0] as SceneEntity;
    expect(stroke(line)).toEqual([95, 100, 105, 100]);
    expect(line.placement?.scaleX).toBeCloseTo(0.01);
    const distance = measureDistance({ x: 95, y: 100, entity: line }, { x: 105, y: 100, entity: line });
    expect(distance.paper).toBeCloseTo(10);
    expect(distance.real).toBeCloseTo(1000);
  });
});

/**
 * MULTILEADER は CONTEXT_DATA・LEADER・LEADER_LINE の区切りの中と外で、10（基点・着地点・頂点・ブロックの尺度）、
 * 40・41・42（尺度・折れ線・矢印の大きさ）などを別の意味に使う。読み分けを誤っても何かは描かれるので、
 * 引出線の位置・矢印・折れ線の長さが黙ってずれる。
 */
describe("DXF のマルチ引出線", () => {
  it("線を矢印の根元から着地点へ、折れ線を着地点から内容の側へ引き、文字を CONTEXT_DATA の位置に描く", () => {
    const scene = dxfToScene(parseDxf(MLEADER));
    expect(scene.entities).toHaveLength(1);
    const leader = scene.entities[0] as SceneEntity;
    expect(leader.source).toBe("MULTILEADER");
    // 矢印の長さは CONTEXT_DATA の 140（2）。線は先端 (0, 0) から着地点の向きへ 2 だけ離れた根元から始まる
    const [ux, uy] = [16 / Math.hypot(16, 10), 10 / Math.hypot(16, 10)];
    const [line, dogleg] = strokes(leader);
    expect(line?.map((value) => Math.round(value * 1000) / 1000)).toEqual([
      Math.round(ux * 2 * 1000) / 1000,
      Math.round(uy * 2 * 1000) / 1000,
      16,
      10,
    ]);
    expect(dogleg).toEqual([16, 10, 20, 10]);
    const arrow = leader.primitives.find((item) => item.kind === "fill");
    expect(arrow?.kind === "fill" && Array.from(arrow.rings[0] ?? []).slice(0, 2)).toEqual([0, 0]);
    const text = leader.primitives.find((item) => item.kind === "text");
    expect(text?.kind === "text" && text.text).toBe("ABC");
  });
});

/**
 * MLINE の要素の位置は、頂点から留め継ぎの向きへの距離（41 の 1 つ目）で、角では 1 / sin の分だけ長い。
 * 41 の 2 つ目以降は MLEDIT で切った区間。読み違えると壁の線が角でずれたり、T 字の継ぎ目に線が残ったりする。
 * DWG（ezdwg）は 41 を持たないので、MLINESTYLE のずれ・位置合わせ・尺度から同じ位置を求める。
 */
describe("DXF のマルチライン", () => {
  const round = (lines: number[][]) => lines.map((line) => line.map((value) => Math.round(value * 1e6) / 1e6));

  it("要素を留め継ぎの向きへ 41 の距離に引いて切れ目を除き、41 がなければスタイルから同じ位置を求める", () => {
    const doc = parseDxf(MLINE);
    expect(round(strokes(dxfToScene(doc).entities[0]))).toEqual([
      [0, 1, 3, 1],
      [5, 1, 9, 1, 9, 10],
      [0, -1, 11, -1, 11, 10],
    ]);
    // DWG と同じく 41 のないマルチライン: 切れ目はなく、位置はスタイル（±0.5、ゼロの位置合わせ、尺度 2）から求める
    delete (doc.entities[0] as DxfMLine).line_params;
    expect(round(strokes(dxfToScene(doc).entities[0]))).toEqual([
      [0, 1, 9, 1, 9, 10],
      [0, -1, 11, -1, 11, 10],
    ]);
  });
});

// 壊れた図面・大きすぎる値でも、読み取りが終わり、TypeScript と Rust が同じところで止まること
describe("DXF の読み取りの上限", () => {
  it("表示のブロックが自分を指す寸法は、ブロック参照と同じ深さで止め、自分を 2 回含むブロックは描く数で止める", () => {
    const scene = readDrawing(LIMITS, [dxf]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "block-depth" }));
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "block-limit" }));
    // 32 段の寸法の線を、一番外側の寸法の 1 つの要素にまとめる
    expect(strokes(scene.entities.find((entity) => entity.id === "D1"))).toHaveLength(32);
  });

  it("M × N が頂点の数より大きいポリゴンメッシュは、頂点のある格子の位置だけを描く", () => {
    const mesh = readDrawing(LIMITS, [dxf]).entities.find((entity) => entity.type === "ポリゴンメッシュ");
    expect(strokes(mesh)).toEqual([[0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 1, 1]]);
  });

  it("模様の線の本数が数えられない（NaN の）ハッチングは、多すぎる模様として外形を描く", () => {
    const scene = readDrawing(BINARY_LIMITS, [dxf]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "hatch-dense", count: 1 }));
    expect(stroke(scene.entities[0])).toEqual([0, 0, 100, 0, 100, 100, 0, 100, 0, 0]);
  });
});
