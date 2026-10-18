import { measureDistance, readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { cp932 } from "../../../tests/cp932.js";
import { sxf } from "../src/index.js";

/**
 * SFC は並び順でコード・所属が決まる（附属書 SFC 編 1-3）。順序の解釈を誤ると、
 * 色・レイヤ・縮尺が黙って別のものになるため、仕様の規則を 1 つの図面で確かめる。
 */
const SAMPLE = String.raw`ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');
FILE_NAME('test.sfc','2026-10-18T00:00:00',('author'),('org'),'SCADEC_API_Ver3.10$$3.1','test','');
FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
ENDSEC;
DATA;
/*SXF
#10 = user_defined_colour_feature('10','20','30')
SXF*/
/*SXF
#20 = user_defined_font_feature(\'$$SXF_custom\','2','(2.000000,1.000000)')
SXF*/
/*SXF
#30 = width_feature('0.250000')
SXF*/
/*SXF
#40 = width_feature('0.400000')
SXF*/
/*SXF
#50 = text_font_feature(\'Century\')
SXF*/
/*SXF
#60 = polyline_feature('0','0','0','0','5','(0.000000,10.000000,10.000000,0.000000,0.000000)','(0.000000,0.000000,10.000000,10.000000,0.000000)')
SXF*/
/*SXF
#70 = composite_curve_org_feature('17','17','11','1')
SXF*/
/*SXF
#80 = line_feature('1','8','1','3','0.000000','0.000000','1000.000000','0.000000')
SXF*/
/*SXF
#90 = text_string_feature('1','8','1',\'表示ソ能\\x\','0.000000','100.000000','50.000000','300.000000','0.000000','0.00000000000000','0.00000000000000','1','1')
SXF*/
/*SXF
#100 = sfig_org_feature(\'平面図\','1')
SXF*/
/*SXF
#200 = line_feature('1','8','1','1','0.000000','0.000000','10.000000','0.000000')
SXF*/
/*SXF
#210 = sfig_org_feature(\'G\','3')
SXF*/
/*SXF
#220 = line_feature('1','8','1','1','0.000000','0.000000','10.000000','0.000000')
SXF*/
/*SXF
#230 = sfig_org_feature(\'S\','2')
SXF*/
/*SXF
#110 = fill_area_style_colour_feature('2','17','1','0','()')
SXF*/
/*SXF
#120 = sfig_locate_feature('1',\'平面図\','100.000000','50.000000','0.00000000000000','0.05000000000000','0.05000000000000')
SXF*/
/*SXF
#240 = sfig_locate_feature('0',\'G\','500.000000','500.000000','30.00000000000000','2.00000000000000','2.00000000000000')
SXF*/
/*SXF
#250 = sfig_locate_feature('0',\'S\','300.000000','300.000000','0.00000000000000','1.00000000000000','1.00000000000000')
SXF*/
/*SXF
#130 = drawing_sheet_feature(\'道路\','1','1','0','0')
SXF*/
/*SXF
#140 = layer_feature(\'部分\','1')
SXF*/
/*SXF
#150 = layer_feature(\'図\','0')
SXF*/
/*SXF
#160 = line_feature('2','1','1','1','0.000000','0.000000','1.000000','1.000000')
SXF*/
ENDSEC;
END-ISO-10303-21;
`;

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

function points(item: SceneEntity, index = 0): number[] {
  const primitive = item.primitives[index];
  if (primitive?.kind !== "stroke") throw new Error("線ではありません");
  return Array.from(primitive.points, (value) => Math.round(value * 1e6) / 1e6);
}

describe("SFC の読み取り", () => {
  const scene = readDrawing(cp932(SAMPLE), [sxf]);

  it("用紙・版・出現順のレイヤコードを読む（用紙より後のレイヤも含む）", () => {
    expect(scene.format).toBe("sfc");
    expect(scene.version).toBe("SXF 3.1");
    expect(scene.sheet).toEqual({ name: "道路", size: "A1 横", width: 841, height: 594 });
    expect(scene.layers).toEqual([
      { id: 1, name: "部分", visible: true },
      { id: 2, name: "図", visible: false },
    ]);
  });

  it("2 バイト目が 0x5C の文字と、\\\\ で書いた \\ を取り違えずに復号する", () => {
    const text = entity(scene.entities, "#120>#90");
    expect(text.properties).toContainEqual({ label: "文字列", value: "表示ソ能\\x" });
  });

  it("部分図の縮尺を用紙の座標に解決し、計測は元の座標の長さを返す", () => {
    const line = entity(scene.entities, "#120>#80");
    expect(points(line)).toEqual([100, 50, 150, 50]);
    expect(line.layers).toEqual([1, 1]);
    expect(line.placement?.scaleX).toBeCloseTo(0.05);
    const result = measureDistance({ x: 100, y: 50, entity: line }, { x: 150, y: 50, entity: line });
    expect(result.paper).toBeCloseTo(50);
    expect(result.real).toBeCloseTo(1000);
  });

  it("作図グループは配置の値を使わず、測地座標系の部分図は X と Y を入れ替える", () => {
    expect(points(entity(scene.entities, "#240>#200"))).toEqual([0, 0, 10, 0]);
    expect(points(entity(scene.entities, "#250>#220"))).toEqual([300, 300, 300, 310]);
  });

  it("色・線種・線幅のユーザ定義コードを出現順に割り当て、既定義の値の線幅はコードを消費しない", () => {
    const fill = entity(scene.entities, "#110");
    expect(fill.layers).toEqual([2]);
    const [area, outline] = fill.primitives;
    expect(area).toMatchObject({ kind: "fill", color: "#0a141e" });
    expect(area?.kind === "fill" ? area.rings[0]?.length : 0).toBe(10);
    expect(outline).toMatchObject({ kind: "stroke", color: "#0a141e", width: 0.4, dashes: [2, 1], closed: true });
    expect(entity(scene.entities, "#120>#80").primitives[0]).toMatchObject({ width: 0.25 });
  });

  it("用紙より後に書かれた要素は読み落とさず、注意として件数を返す", () => {
    expect(points(entity(scene.entities, "#160"))).toEqual([0, 0, 1, 1]);
    expect(scene.notices).toContainEqual(expect.objectContaining({ code: "after-sheet", count: 1 }));
  });
});
