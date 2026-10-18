import { readDrawing, type SceneEntity } from "@zumenjs/core";
import { describe, expect, it } from "vitest";
import { sxf } from "../src/index.js";

/**
 * P21 は SFC と同じ図面を AP202 の参照の網で表す。文字の配置点のずれ、子図の縮尺、円弧の向き、
 * 矢印の向き、非表示の指定を読み違えると、表示が黙って SFC 版と食い違うため、その対応を確かめる。
 */
const OCCURRENCE = "ANNOTATION_OCCURRENCE() DRAUGHTING_ANNOTATION_OCCURRENCE() GEOMETRIC_REPRESENTATION_ITEM()";
const SAMPLE = String.raw`ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1');
FILE_NAME('test.p21','2026-10-18T00:00:00',(''),(''),'SCADEC_API_Ver3.10$$3.1','test','');
FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
ENDSEC;
DATA;
#5=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));
#7=(GEOMETRIC_REPRESENTATION_CONTEXT(2) GLOBAL_UNIT_ASSIGNED_CONTEXT((#5)) REPRESENTATION_CONTEXT('ID1','2D'));
#10=DRAUGHTING_PRE_DEFINED_COLOUR('white');
#11=COLOUR_RGB('$$SXF_deeppink',0.750000,0.000000,0.500000);
#12=COLOUR_RGB(' ',0.000000,0.800000,1.000000);
#20=DRAUGHTING_PRE_DEFINED_CURVE_FONT('continuous');
#21=LENGTH_MEASURE_WITH_UNIT(POSITIVE_LENGTH_MEASURE(0.250000),#5);
#30=CURVE_STYLE(' ',#20,#21,#10);
#31=PRESENTATION_STYLE_ASSIGNMENT((#30));
#40=CARTESIAN_POINT(' ',(0.000000,0.000000));
#41=CARTESIAN_POINT(' ',(1000.000000,0.000000));
#42=DIRECTION(' ',(1.000000,0.000000));
#43=VECTOR(' ',#42,1.000000);
#44=LINE(' ',#40,#43);
#45=TRIMMED_CURVE(' ',#44,(#40),(#41),.T.,.CARTESIAN.);
#46=(ANNOTATION_CURVE_OCCURRENCE() ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#31),#45));
#50=AXIS2_PLACEMENT_2D(' ',#40,$);
#51=CIRCLE(' ',#50,10.000000);
#52=TRIMMED_CURVE(' ',#51,(PARAMETER_VALUE(0.00000000000000)),(PARAMETER_VALUE(1.57079632679490)),.F.,.PARAMETER.);
#53=(ANNOTATION_CURVE_OCCURRENCE() ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#31),#52));
#60=PLANAR_EXTENT(' ',300.000000,50.000000);
#61=CARTESIAN_POINT(' ',(100.000000,150.000000));
#62=AXIS2_PLACEMENT_2D(' ',#61,#42);
#63=EXTERNAL_SOURCE(IDENTIFIER('scadec'));
#64=EXTERNALLY_DEFINED_TEXT_FONT(IDENTIFIER('\X2\FF2DFF33\X0\ \X2\30B430B730C330AF\X0\'),#63);
#65=TEXT_LITERAL_WITH_EXTENT('$$SXF_topline right','\X2\8868793A\X0\',#62,'baseline right',.RIGHT.,#64,#60);
#66=TEXT_STYLE_FOR_DEFINED_FONT(#12);
#67=(TEXT_STYLE(' ',#66) TEXT_STYLE_WITH_BOX_CHARACTERISTICS((BOX_HEIGHT(50.000000),BOX_WIDTH(40.000000),BOX_SLANT_ANGLE(0.0),BOX_ROTATE_ANGLE(0.0))) TEXT_STYLE_WITH_SPACING(LENGTH_MEASURE(0.000000)));
#68=PRESENTATION_STYLE_ASSIGNMENT((#67));
#69=(${OCCURRENCE} ANNOTATION_TEXT_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#68),#65));
#70=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_FM_\X2\5E73976256F3\X0\',(#46,#53,#69,#50),#7);
#71=SYMBOL_REPRESENTATION_MAP(#50,#70);
#72=CARTESIAN_POINT(' ',(100.000000,50.000000));
#73=AXIS2_PLACEMENT_2D(' ',#72,#42);
#74=SYMBOL_TARGET(' ',#73,0.050000,0.050000);
#75=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#71,#74) REPRESENTATION_ITEM(' '));
#76=PRESENTATION_STYLE_ASSIGNMENT((NULL_STYLE(.NULL.)));
#77=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#76),#75));
#81=CARTESIAN_POINT(' ',(60.000000,0.000000));
#83=TRIMMED_CURVE(' ',#44,(#40),(#81),.T.,.CARTESIAN.);
#84=(ANNOTATION_CURVE_OCCURRENCE() ${OCCURRENCE} DIMENSION_CURVE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#31),#83));
#85=DIRECTION(' ',(-1.000000,0.000000));
#86=AXIS2_PLACEMENT_2D(' ',#40,#85);
#87=SYMBOL_TARGET(' ',#86,0.500000,0.500000);
#88=PRE_DEFINED_TERMINATOR_SYMBOL('filled arrow');
#89=DEFINED_SYMBOL(' ',#88,#87);
#90=SYMBOL_COLOUR(#10);
#91=SYMBOL_STYLE(' ',#90);
#92=PRESENTATION_STYLE_ASSIGNMENT((#91));
#93=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() DIMENSION_CURVE_TERMINATOR(.ORIGIN.) REPRESENTATION_ITEM('$$SXF_arw_1') STYLED_ITEM((#92),#89) TERMINATOR_SYMBOL(#84));
#94=AXIS2_PLACEMENT_2D(' ',#81,#85);
#95=SYMBOL_TARGET(' ',#94,0.500000,0.500000);
#96=DEFINED_SYMBOL(' ',#88,#95);
#97=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() DIMENSION_CURVE_TERMINATOR(.TARGET.) REPRESENTATION_ITEM('$$SXF_arw_2') STYLED_ITEM((#92),#96) TERMINATOR_SYMBOL(#84));
#98=(DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#84,#93,#97)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LINEAR_DIMENSION() REPRESENTATION_ITEM(' '));
#101=CARTESIAN_POINT(' ',(10.000000,0.000000));
#102=CARTESIAN_POINT(' ',(10.000000,10.000000));
#103=CARTESIAN_POINT(' ',(0.000000,10.000000));
#105=POLYLINE(' ',(#40,#101,#102,#103,#40));
#106=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#105);
#107=COMPOSITE_CURVE(' ',(#106),.F.);
#108=(ANNOTATION_CURVE_OCCURRENCE() ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#31),#107));
#109=ANNOTATION_FILL_AREA(' ',(#107));
#110=FILL_AREA_STYLE_COLOUR(' ',#11);
#111=FILL_AREA_STYLE(' ',(#110));
#112=PRESENTATION_STYLE_ASSIGNMENT((#111));
#113=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#112),#109));
#120=PRESENTATION_LAYER_ASSIGNMENT('$$SXF_dummy_layer_for_subfigure',' ',(#77));
#121=PRESENTATION_LAYER_ASSIGNMENT('\X2\7DDA\X0\',' ',(#46,#53,#69));
#122=PRESENTATION_LAYER_ASSIGNMENT('DIM',' ',(#84,#113));
#123=INVISIBILITY((#122,#108));
#130=DRAWING_DEFINITION('01',$);
#131=DRAUGHTING_DRAWING_REVISION('01',#130,$);
#132=DRAUGHTING_TITLE((#131),'JAPANESE','\X2\8868793A\X0\');
#133=PLANAR_BOX(' ',841.000000,594.000000,#50);
#134=DRAWING_SHEET_REVISION('A1_horizontal',(#77,#98,#113,#133),#7,'01');
#135=PRESENTATION_SIZE(#134,#133);
ENDSEC;
END-ISO-10303-21;
`;

function entity(entities: SceneEntity[], id: string): SceneEntity {
  const found = entities.find((item) => item.id === id);
  if (!found) throw new Error(`${id} がありません`);
  return found;
}

describe("P21 の読み取り", () => {
  const scene = readDrawing(new TextEncoder().encode(SAMPLE), [sxf]);

  it("用紙・\\X2\\ の文字列・レイヤと非表示の指定を読む（子図用の仮レイヤは除く）", () => {
    expect(scene.format).toBe("p21");
    expect(scene.version).toBe("SXF 3.1");
    expect(scene.sheet).toEqual({ name: "表示", size: "A1 横", width: 841, height: 594 });
    expect(scene.layers).toEqual([
      { id: 1, name: "線", visible: true },
      { id: 2, name: "DIM", visible: false },
    ]);
  });

  it("子図の縮尺を解決し、時計回りの円弧（.F.）を逆回りに描かない", () => {
    const line = entity(scene.entities, "#77>#46");
    expect(Array.from((line.primitives[0] as { points: Float64Array }).points)).toEqual([100, 50, 150, 50]);
    expect(line.placement?.name).toContain("平面図");
    expect(line.placement?.scaleX).toBeCloseTo(0.05);
    // 0° から時計回りに 90° まで回る円弧は、中心の真下（270°）を通る
    expect(entity(scene.entities, "#77>#53").bbox.minY).toBeCloseTo(49.5, 6);
  });

  it("文字の配置点をベースラインから SXF の配置基点（右上）へ戻す", () => {
    const text = entity(scene.entities, "#77>#69");
    expect(text.properties).toContainEqual({ label: "文字列", value: "表示" });
    expect(text.bbox.minX).toBeCloseTo(90, 6);
    expect(text.bbox.maxX).toBeCloseTo(105, 6);
    expect(text.bbox.minY).toBeCloseTo(57.5, 6);
    expect(text.bbox.maxY).toBeCloseTo(60, 6);
  });

  it("矢印の向きを外向き・内向きにし、寸法と塗りのレイヤ・外形の非表示を反映する", () => {
    const dimension = entity(scene.entities, "#98");
    expect(dimension.layers).toEqual([2]);
    expect(dimension.properties).toContainEqual({
      label: "矢印",
      value: "filled arrow（外向き）・filled arrow（内向き）",
    });
    const fill = entity(scene.entities, "#113");
    expect(fill.layers).toEqual([2]);
    expect(fill.primitives.map((primitive) => primitive.kind)).toEqual(["fill"]);
    expect(fill.primitives[0]).toMatchObject({ color: "#c00080" });
  });
});
