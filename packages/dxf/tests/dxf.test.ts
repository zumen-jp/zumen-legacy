import { measureDistance, readDrawing, type SceneEntity, type TextPrimitive } from "@zumenjs/core";
import { apply } from "@zumenjs/core/geometry";
import { describe, expect, it } from "vitest";
import {
  type DxfAttdef,
  type DxfAttrib,
  type DxfEmbeddedMText,
  type DxfImage,
  type DxfInsert,
  type DxfLine,
  type DxfMLine,
  type DxfMText,
  type DxfText,
  dxf,
  dxfToScene,
  parseDxf,
} from "../src/index.js";
import {
  ATTRIBUTES_MULTILINE,
  ATTRIBUTES_R2018,
  ascii,
  BINARY,
  BINARY_LIMITS,
  BINARY_TEXT,
  BLOCKS,
  CODEPAGE,
  DIMSTYLE_DEFAULTS,
  IDS,
  IMAGES,
  LAYOUT,
  LIMITS,
  latin1,
  MISALIGNED,
  MLEADER,
  MLINE,
  R12,
  R12_SHIFT_JIS,
  VERTICAL_TEXT,
  WESTERN,
  XREFS,
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

function texts(entity: SceneEntity | undefined): TextPrimitive[] {
  return (entity?.primitives ?? []).flatMap((item) => (item.kind === "text" ? [item] : []));
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

  it("外部参照は参照先の図面を読まないので、ブロックの中の参照も数えて、参照先とともに notices で知らせる", () => {
    const scene = readDrawing(XREFS, [dxf]);
    expect(scene.entities.map((entity) => entity.id)).toEqual(["20", "24>B1"]);
    expect(scene.notices.find((notice) => notice.code === "xref")).toEqual({
      level: "warning",
      code: "xref",
      message: "外部参照（PLAN・grid.dxf・site.dwg）は参照先の図面を読まないため、描いていません",
      count: 4,
    });
  });
});

/**
 * id はファイルから決まり、同じファイルなら版を重ねても変わらない（docs/architecture.md の「Entity ids」）。
 * ハンドルのない図形（R12 の図面など）は、ファイルの先頭から数えたレコード（グループコード 0）の位置で表す。
 * 読まない種類の図形も数えるので、読める種類が増えても id は変わらず、ブロックの中と INSERT の属性でも重ならない。
 */
describe("DXF の id と単位", () => {
  it("ハンドルのない図形は、ファイルの中のレコードの位置で表す", () => {
    const drawing = IDS;
    const doc = parseDxf(drawing);
    expect(doc.entities.map((entity) => entity.ordinal)).toEqual([3, 7, 9, 10]);
    expect((doc.entities[3] as DxfInsert).attribs.map((attribute) => attribute.ordinal)).toEqual([11, 12]);
    // SHAPE（8 番目のレコード）は読まないが数える。ブロックの中の線と属性は、どれも別の id になる
    const scene = readDrawing(drawing, [dxf]);
    expect(scene.entities.map((entity) => entity.id)).toEqual(["#7", "#9", "#10>#3", "#10>#11", "#10>#12"]);
    // ハンドルのある図形はハンドルで表し、位置を持たない
    const handled = parseDxf(BLOCKS);
    expect(handled.entities.every((entity) => entity.handle !== "" && entity.ordinal === undefined)).toBe(true);
  });

  it("$INSUNITS の単位を Scene.unit に示し、指定がなければ見なした単位にする", () => {
    expect(readDrawing(BLOCKS, [dxf]).unit).toBe("meter");
    expect(readDrawing(LAYOUT, [dxf]).unit).toBe("millimeter");
    const r12 = readDrawing(R12, [dxf]);
    expect(r12.unit).toBe("inch");
    expect(r12.notices.some((notice) => notice.code === "unit-assumed")).toBe(true);
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
 * 41 がないときは、MLINESTYLE のずれ・位置合わせ・尺度から同じ位置を求める。
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
    // 41 のないマルチライン: 切れ目はなく、位置はスタイル（±0.5、ゼロの位置合わせ、尺度 2）から求める
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

  it("模様の線の基点が遠く、線の番号に 1 を足しても変わらないハッチングは、多すぎる模様として外形を描く", () => {
    const scene = readDrawing(LIMITS, [dxf]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "hatch-dense", count: 1 }));
    expect(stroke(scene.entities.find((entity) => entity.id === "H1"))).toEqual([0, 0, 100, 0, 100, 100, 0, 100, 0, 0]);
  });

  it("模様の線の本数が数えられない（NaN の）ハッチングは、多すぎる模様として外形を描く", () => {
    const scene = readDrawing(BINARY_LIMITS, [dxf]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "hatch-dense", count: 1 }));
    expect(stroke(scene.entities[0])).toEqual([0, 0, 100, 0, 100, 100, 0, 100, 0, 0]);
  });
});

/**
 * 同じ図面の DWG と比べて見つかった、DXF の値の読み違い。AutoCAD 2018 の属性の種類、バイナリの DXF の文字列の
 * 書き方、AutoCAD が省く寸法スタイルの既定値（図面は fixtures.ts）
 */
describe("DXF の値と同じ図面の DWG", () => {
  it("AcDbAttribute の 71（属性の種類）を文字の生成フラグにせず、AcDbText の 71 だけを使う", () => {
    const [attdef, attrib] = parseDxf(ATTRIBUTES_R2018).entities as [DxfAttdef, DxfAttrib];
    expect(attdef.text_generation_flag).toBe(0);
    expect(attrib.text_generation_flag).toBe(2);
    // 複数行の属性定義のタグは上下反転せず、AcDbText で左右反転の属性は X を反転して描く
    const scene = readDrawing(ATTRIBUTES_R2018, [dxf]);
    const [tag, value] = scene.entities.map((entity) => texts(entity)[0]);
    expect([tag?.text, Math.sign(tag?.matrix[0] ?? 0), Math.sign(tag?.matrix[3] ?? 0)]).toEqual(["MULTI", 1, 1]);
    expect([value?.text, Math.sign(value?.matrix[0] ?? 0), Math.sign(value?.matrix[3] ?? 0)]).toEqual(["ABC", -1, 1]);
  });

  it("バイナリの DXF の改行・タブ・^ をテキストの DXF と同じ ^J・^I・「^ 」にし、MTEXT の改行で段落を分ける", () => {
    const [mtext, power, text] = parseDxf(BINARY_TEXT).entities as [DxfMText, DxfMText, DxfText];
    // biome-ignore format: DXF のタグ（グループコードと値）の組を 1 行に並べて読めるようにする
    const asText = parseDxf(latin1(ascii(
      0, "SECTION", 2, "ENTITIES",
      0, "MTEXT", 10, 0, 20, 0, 40, 2.5, 1, "First line^JSecond^Iline",
      0, "MTEXT", 10, 0, 20, -20, 40, 2.5, 1, "x^ 2",
      0, "TEXT", 10, 0, 20, -30, 40, 2.5, 1, "m^ 2",
      0, "ENDSEC", 0, "EOF",
    )));
    expect([mtext.raw_text, power.raw_text, text.text]).toEqual(["First line^JSecond^Iline", "x^ 2", "m^ 2"]);
    expect(
      asText.entities.map((entity) => ("raw_text" in entity ? entity.raw_text : (entity as DxfText).text)),
    ).toEqual([mtext.raw_text, power.raw_text, text.text]);
    const scene = readDrawing(BINARY_TEXT, [dxf]);
    expect(scene.entities.map((entity) => texts(entity).map((item) => item.text))).toEqual([
      ["First line", "Second    line"],
      ["x^2"],
      ["m^2"],
    ]);
  });

  it("AutoCAD が DXF で省いた DIMASZ・DIMTXT は、$MEASUREMENT によらず既定の 0.18 にする", () => {
    const doc = parseDxf(DIMSTYLE_DEFAULTS);
    expect(doc.header).toMatchObject({ measurement: 1, dimasz: 2.5, dimtxt: 2.5 });
    expect(doc.dimstyles.map(({ name, dimasz, dimtxt }) => [name, dimasz, dimtxt])).toEqual([
      ["Standard", 0.18, 0.18],
      ["ISO-25", 2.5, 2.5],
    ]);
    // 引出線の矢印の長さは、ヘッダの $DIMASZ ではなく引出線の寸法スタイルの DIMASZ × DIMSCALE
    const arrows = readDrawing(DIMSTYLE_DEFAULTS, [dxf]).entities.map((entity) => {
      const fill = entity.primitives.find((item) => item.kind === "fill");
      const [tx = 0, , ax = 0, , bx = 0] = fill?.kind === "fill" ? Array.from(fill.rings[0] ?? []) : [];
      return Math.round(((ax + bx) / 2 - tx) * 1e6) / 1e6;
    });
    expect(arrows).toEqual([0.18, 2.5]);
  });
});

/**
 * AutoCAD 2018 の複数行の属性の文字は、属性の後の埋め込みの MTEXT にだけある（1 行の文字は空）。MTEXT と同じ配置点・
 * 行の間隔で描き、非表示（70 の 1）・属性定義の規則（ブロックの外ではタグ、ブロックの中では定数だけ）は 1 行の属性と同じ
 */
describe("DXF の複数行の属性", () => {
  it("埋め込みの MTEXT の文字を、属性の色で MTEXT と同じ位置・同じ行に描く", () => {
    const doc = parseDxf(ATTRIBUTES_MULTILINE);
    const insert = doc.entities.find((entity) => entity.type === "INSERT") as DxfInsert;
    const [note, hidden] = insert.attribs as [DxfAttrib, DxfAttrib];
    expect(note).toMatchObject({ text: "", tag: "NOTE", attribute_flags: 0, color_index: 3 });
    expect(note.mtext).toMatchObject({
      raw_text: "First line\\PSecond line",
      text: "First line\nSecond line",
      insert: [100, 0, 0],
      char_height: 2.5,
      attachment_point: 1,
      line_spacing_factor: 1,
    });
    expect(hidden.mtext?.raw_text).toBe("Hidden");
    const scene = dxfToScene(doc);
    const attribute = scene.entities.find((entity) => entity.id === "50>51");
    expect(attribute).toMatchObject({ kind: "attribute", type: "属性", source: "ATTRIB" });
    expect(attribute?.properties).toContainEqual({ key: "tag", label: "タグ", value: "NOTE" });
    expect(texts(attribute).map((item) => [item.text, item.color])).toEqual([
      ["First line", "#00ff00"],
      ["Second line", "#00ff00"],
    ]);
    // 同じ値の MTEXT と同じ文字の並び
    const mtext: DxfMText = { ...note, ...(note.mtext as DxfEmbeddedMText), type: "MTEXT" };
    expect(texts(attribute)).toEqual(texts(dxfToScene({ ...doc, entities: [mtext] }).entities[0]));
    expect(scene.entities.find((entity) => entity.id === "50>52")).toBeUndefined();
    // 押し出し方向は、属性（AcDbText の 210）ではなく埋め込みの MTEXT のもの（ezdxf の virtual_mtext_entity と同じ）
    const drawn = () => texts(dxfToScene(doc).entities.find((entity) => entity.id === "50>51"));
    note.extrusion = [0, 0, -1];
    expect(drawn()).toEqual(texts(attribute));
    (note.mtext as DxfEmbeddedMText).extrusion = [0, 0, -1];
    expect(drawn().map((item) => Math.sign(item.matrix[0]))).toEqual([-1, -1]);
  });

  it("属性定義は、ブロックの外ではタグを、ブロックの中では定数の既定値だけを描く", () => {
    const scene = readDrawing(ATTRIBUTES_MULTILINE, [dxf]);
    const shown = (id: string) => texts(scene.entities.find((entity) => entity.id === id)).map((item) => item.text);
    expect(shown("54")).toEqual(["TITLE"]);
    expect(shown("50>43")).toEqual(["Fixed", "value"]);
    expect(scene.entities.find((entity) => entity.id === "50>42")).toBeUndefined();
    // タグの左上は、属性定義の埋め込みの MTEXT の配置点（0, 20）。1 行目のベースラインは高さの分だけ下
    const [title] = texts(scene.entities.find((entity) => entity.id === "54"));
    const [value] = texts(scene.entities.find((entity) => entity.id === "50>51"));
    expect((title?.matrix[5] ?? 0) - 20).toBeCloseTo((value?.matrix[5] ?? 0) - 0);
  });

  it("マルチ引出線のブロックの複数行の属性定義には、属性の値（302）を埋め込みの MTEXT の文字として描く", () => {
    const scene = readDrawing(ATTRIBUTES_MULTILINE, [dxf]);
    const leader = scene.entities.find((entity) => entity.id === "60");
    expect(texts(leader).map((item) => item.text)).toEqual(["Leader", "note", "Fixed", "value"]);
    // 値の左上は、ブロックの配置点（0, -40）に動かした属性定義の MTEXT の配置点（0, 0）
    const [first] = texts(leader);
    const [inBlock] = texts(scene.entities.find((entity) => entity.id === "50>51"));
    expect((first?.matrix[5] ?? 0) + 40).toBeCloseTo(inBlock?.matrix[5] ?? 0);
  });
});

// AutoCAD・Jw_cad の DXF の縦書き（図面は fixtures.ts の VERTICAL_TEXT）
describe("DXF の縦書き", () => {
  it("名前の先頭が @ の TrueType フォントの文字は、字を立てて、ベースラインの向き（270° なら下）へ並べる", () => {
    const [vertical, narrow, horizontal] = readDrawing(VERTICAL_TEXT, [dxf]).entities.map((entity) => texts(entity)[0]);
    const round = (value: number) => Math.round(value * 1e6) / 1e6 + 0;
    const at = (text: TextPrimitive | undefined, x: number, y: number): [number, number] => {
      const m = text?.matrix ?? [1, 0, 0, 1, 0, 0];
      return [round(m[0] * x + m[2] * y + m[4]), round(m[1] * x + m[3] * y + m[5])];
    };
    // 字の座標系は回さない（字が立つ）。文字範囲は挿入点 (10, 50) から下へ、横は em の下端から 0.12 em 右がベースライン
    const em = 3.5 / 0.88;
    expect(vertical?.matrix.slice(0, 4).map(round)).toEqual([1, 0, 0, 1]);
    expect(at(vertical, 0, 0)).toEqual([round(10 - 0.12 * em), round(50 - 4 * em)]);
    expect(at(vertical, vertical?.boxWidth ?? 0, vertical?.boxHeight ?? 0)).toEqual([round(10 + 0.88 * em), round(50)]);
    const cells = (vertical?.cells ?? []).map((cell) => at(vertical, cell.x, cell.y));
    expect(cells.map(([x]) => x)).toEqual(cells.map(() => round(10 + 0.38 * em)));
    expect(cells.map(([, y]) => y)).toEqual([...cells.map(([, y]) => y)].sort((a, b) => b - a));
    // 幅の係数はベースラインの向き（縦）に効き、斜体の角度は縦書きの字にも効く
    expect(round((narrow?.boxHeight ?? 0) * (narrow?.matrix[3] ?? 0))).toBe(round(2.5 * em * 0.8));
    expect(round(narrow?.matrix[2] ?? 0)).toBe(round(Math.tan((10 * Math.PI) / 180)));
    // 同じ図面の、@ のないフォントの文字は横に並べる
    expect(at(horizontal, 0, 0)).toEqual([10 + 40, round(50 - 0.12 * em)]);
  });
});

describe("DXF の画像", () => {
  it("大きすぎる表示フラグも ToInt32 で読み、Rust と同じく表示しない", () => {
    const doc = parseDxf(IMAGES);
    for (const entity of doc.entities) if (entity.type === "IMAGE") entity.flags = 1e100;
    expect(dxfToScene(doc).underlays).toBeUndefined();
  });

  it("IMAGEDEF のファイルの名前・読み込んだ状態と、明るさ・フェード・反転した切り抜き・IMAGEFRAME を読む", () => {
    const doc = parseDxf(IMAGES);
    const images = doc.entities.filter((entity): entity is DxfImage => entity.type === "IMAGE");
    const byHandle = new Map(images.map((image) => [image.handle, image]));
    expect(byHandle.get("100")).toMatchObject({ path: "photo.png", loaded: true, brightness: 60, fade: 25 });
    expect(byHandle.get("101")).toMatchObject({ clip_inverted: true, clipping: true });
    expect(byHandle.get("103")).toMatchObject({ path: "C:\\scans\\unloaded.tif", loaded: false });
    expect(byHandle.get("104")?.path).toBeUndefined();
    expect(byHandle.get("110")?.path).toBe("logo.jpg");
    expect(doc.image_frame).toBe(0);
  });

  it("中身の左上を画像の左上の角に、右下を右下の角に置き、外形は選ぶためだけに残す", () => {
    const scene = readDrawing(IMAGES, [dxf]);
    const underlay = scene.underlays?.find((candidate) => candidate.id === "101");
    expect(underlay).toBeDefined();
    if (!underlay) return;
    // insert (40, 0)、U (0.08, 0.06)、V (−0.03, 0.04)、100 × 50 画素: 左上は insert + V·50
    const close = (point: [number, number], expected: [number, number]) => {
      expect(point[0]).toBeCloseTo(expected[0], 9);
      expect(point[1]).toBeCloseTo(expected[1], 9);
    };
    close(apply(underlay.matrix, 0, 0), [38.5, 2]);
    close(apply(underlay.matrix, 100, 50), [48, 6]);
    close(apply(underlay.matrix, 100, 0), [46.5, 8]);
    // 内側を消す切り抜きは、画像の全体と境界の 2 つの輪
    expect(underlay.clip).toHaveLength(2);
    expect(scene.entities.find((entity) => entity.id === "101")?.drawn).toBe(false);
    // 表示しない画像・読み込みを外した画像・定義のない画像は、中身を描かない
    expect(scene.underlays?.map((candidate) => candidate.id)).toEqual(["100", "101", "105>110"]);
  });
});
