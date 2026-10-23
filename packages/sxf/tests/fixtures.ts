import { cp932 } from "../../../tests/cp932.js";

/**
 * テストと正解ファイル（tests/fixtures/sfc・p21）に使う SXF の図面。Rust の読み取り（crates/zumen-sxf）も
 * 同じ図面を読み、TypeScript と同じ Scene になることを確かめる。図面は仕様の規則を確かめるために合成したもので、
 * 実際の図面ではない。
 */

const SFC_HEADER = (name: string, preprocessor: string) => `ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');
FILE_NAME('${name}','2026-10-18T00:00:00',('author'),('org'),'${preprocessor}','test','');
FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
ENDSEC;
DATA;
`;

/** フィーチャを /*SXF … SXF*\/ で囲む。version は /*SXF3・/*SXF3.1 の版 */
function records(lines: readonly string[], version = ""): string {
  return lines.map((line) => `/*SXF${version}\n${line}\nSXF${version}*/\n`).join("");
}

/**
 * SFC は並び順でコード・所属が決まる（附属書 SFC 編 1-3）。順序の解釈を誤ると、
 * 色・レイヤ・縮尺が黙って別のものになるため、仕様の規則を 1 つの図面で確かめる。
 */
export const SFC_SAMPLE = String.raw`ISO-10303-21;
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

/** 寸法・引出し線の文字（有無フラグから縦横の指定まで 12 個の引数） */
function annotation(text: string, x: number, y: number, extra = "'1','1'"): string {
  return `'1','1',\\'${text}\\','${x}','${y}','2.500000','10.000000','0.500000','0.000000','0.000000',${extra}`;
}

/**
 * 34 種のフィーチャをすべて使い、読み取りの注意（未対応・引数の不備・未定義のコード・循環など）が出る書き方も含めた SFC。
 * 構造（色・線種・線幅・フォント）→ 複合曲線 → 複合図形 → 用紙の要素 → 表題欄・用紙 → 用紙より後の要素・レイヤの順に書く。
 */
export const SFC_FEATURES = `${SFC_HEADER("features.sfc", "SCADEC_API_Ver3.10$$3.1")}${records([
  String.raw`#10 = pre_defined_colour_feature(\'red\')`,
  `#20 = user_defined_colour_feature('255','128','0')`,
  `#30 = user_defined_colour_feature('250','250','250')`,
  String.raw`#40 = pre_defined_font_feature(\'dashed\')`,
  String.raw`#50 = user_defined_font_feature(\'$$SXF_custom\','4','(3.000000,1.000000,0.500000,1.000000)')`,
  String.raw`#60 = user_defined_font_feature(\'$$SXF_odd\','3','(3.000000,1.000000,0.500000)')`,
  `#70 = width_feature('0.130000')`,
  `#80 = width_feature('0.300000')`,
  `#90 = width_feature('0.050000')`,
  String.raw`#100 = text_font_feature(\'ＭＳ 明朝\')`,
  String.raw`#110 = text_font_feature(\'ＭＳ ゴシック\')`,
  // 複合曲線 1: 外形の四角形（表示）
  `#120 = polyline_feature('1','1','1','1','5','(0.000000,40.000000,40.000000,0.000000,0.000000)','(0.000000,0.000000,30.000000,30.000000,0.000000)')`,
  `#130 = composite_curve_org_feature('2','1','11','1')`,
  // 複合曲線 2: 半円の穴（非表示）。円弧と線分の向きをそろえてつなぐ
  `#140 = arc_feature('1','1','1','1','20.000000','15.000000','5.000000','0','0.00000000000000','180.00000000000000')`,
  `#150 = line_feature('1','1','1','1','15.000000','15.000000','25.000000','15.000000')`,
  `#160 = composite_curve_org_feature('3','2','1','0')`,
  // 複合曲線 3: 楕円弧・スプライン・クロソイドと、使えない円
  `#170 = ellipse_arc_feature('1','1','1','1','60.000000','10.000000','10.000000','5.000000','1','30.00000000000000','0.00000000000000','180.00000000000000')`,
  `#180 = spline_feature('1','1','1','1','1','4','(50.000000,50.000000,70.000000,70.000000)','(10.000000,30.000000,30.000000,10.000000)')`,
  `#190 = clothoid_feature('1','1','1','1','70.000000','10.000000','20.000000','1','90.00000000000000','0.000000','10.000000')`,
  `#200 = circle_feature('1','1','1','1','60.000000','20.000000','3.000000')`,
  `#210 = composite_curve_org_feature('4','17','1','1')`,
  // 複合曲線 4・5: 三角形と離れた四角形
  `#212 = polyline_feature('1','1','1','1','4','(100.000000,140.000000,120.000000,100.000000)','(0.000000,0.000000,30.000000,0.000000)')`,
  `#214 = composite_curve_org_feature('5','1','1','1')`,
  `#216 = polyline_feature('1','1','1','1','5','(200.000000,260.000000,260.000000,200.000000,200.000000)','(0.000000,0.000000,20.000000,20.000000,0.000000)')`,
  `#218 = composite_curve_org_feature('6','3','2','1')`,
  // 作図部品「部品」
  `#220 = line_feature('1','2','1','2','0.000000','0.000000','10.000000','0.000000')`,
  `#230 = circle_feature('1','3','1','2','5.000000','0.000000','2.000000')`,
  String.raw`#240 = text_string_feature('1','4','2',\'部品\','0.000000','3.000000','2.000000','4.000000','0.000000','0.00000000000000','0.00000000000000','1','1')`,
  String.raw`#250 = sfig_org_feature(\'部品\','4')`,
  // 部分図（数学座標系）。部品を入れ子で配置し、点マーカと寸法を持つ
  String.raw`#260 = sfig_locate_feature('2',\'部品\','100.000000','100.000000','45.00000000000000','2.00000000000000','2.00000000000000')`,
  `#270 = line_feature('1','1','1','1','0.000000','0.000000','1000.000000','0.000000')`,
  `#280 = point_marker_feature('1','2','500.000000','0.000000','2','0.00000000000000','1.00000000000000')`,
  `#290 = linear_dim_feature('2','1','1','1','0.000000','200.000000','1000.000000','200.000000','1','0.000000','0.000000','0.000000','10.000000','0.000000','210.000000','1','1000.000000','0.000000','1000.000000','10.000000','1000.000000','210.000000','6','1','0.000000','200.000000','1.00000000000000','6','1','1000.000000','200.000000','1.00000000000000',${annotation("1,000", 450, 210)})`,
  String.raw`#300 = sfig_org_feature(\'部分図\','1')`,
  // 部分図（測地座標系）。X と Y を入れ替えて描く
  `#310 = line_feature('1','1','1','1','0.000000','0.000000','100.000000','0.000000')`,
  `#320 = arc_feature('1','1','1','1','0.000000','0.000000','50.000000','0','0.00000000000000','90.00000000000000')`,
  String.raw`#330 = text_string_feature('1','1','1',\'測地\','10.000000','10.000000','5.000000','10.000000','0.000000','0.00000000000000','0.00000000000000','1','1')`,
  String.raw`#340 = sfig_org_feature(\'測地\','2')`,
  // 作図グループ、背景色、ラスタ画像の複合図形
  `#350 = line_feature('1','5','1','1','0.000000','0.000000','5.000000','5.000000')`,
  String.raw`#360 = sfig_org_feature(\'グループ\','3')`,
  String.raw`#370 = sfig_org_feature(\'$$背景色$$色$$255_255_255\','3')`,
  String.raw`#380 = sfig_org_feature(\'$$RASTER$$写真\','3')`,
  // 互いに配置し合う複合図形（循環）
  String.raw`#390 = sfig_locate_feature('0',\'循環B\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#400 = sfig_org_feature(\'循環A\','4')`,
  String.raw`#410 = sfig_locate_feature('0',\'循環A\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  `#415 = line_feature('1','1','1','1','0.000000','0.000000','3.000000','0.000000')`,
  String.raw`#420 = sfig_org_feature(\'循環B\','4')`,
  // 同じ名前の複合図形定義（後の定義を使う）
  `#430 = line_feature('1','1','1','1','0.000000','0.000000','1.000000','0.000000')`,
  String.raw`#440 = sfig_org_feature(\'重複\','4')`,
  `#450 = circle_feature('1','1','1','1','0.000000','0.000000','1.000000')`,
  String.raw`#460 = sfig_org_feature(\'重複\','4')`,
  // 用紙の要素: 点マーカ 1〜7 と未定義のコード
  ...[1, 2, 3, 4, 5, 6, 7, 9].map(
    (code) =>
      `#${500 + code} = point_marker_feature('1','${code}','${20 * code}.000000','280.000000','${code}','${code * 10}.00000000000000','${code === 4 ? "2.00000000000000" : "1.00000000000000"}')`,
  ),
  // 線分: ユーザ定義の色・線種・線幅、未定義のコード、白に近い色、未定義のレイヤ
  `#600 = line_feature('1','17','17','11','10.000000','250.000000','100.000000','250.000000')`,
  `#610 = line_feature('1','99','30','20','10.000000','245.000000','100.000000','245.000000')`,
  `#620 = line_feature('9','18','18','12','10.000000','240.000000','100.000000','240.000000')`,
  `#630 = line_feature('1','8','16','0','10.000000','235.000000','100.000000','235.000000')`,
  `#640 = polyline_feature('1','6','4','5','3','(110.000000,130.000000,150.000000)','(240.000000,250.000000,240.000000)')`,
  `#650 = circle_feature('1','7','1','3','170.000000','245.000000','5.000000')`,
  `#660 = arc_feature('1','2','1','3','190.000000','245.000000','5.000000','0','45.00000000000000','300.00000000000000')`,
  `#670 = arc_feature('1','3','2','3','210.000000','245.000000','5.000000','1','45.00000000000000','300.00000000000000')`,
  `#680 = ellipse_feature('1','4','1','3','230.000000','245.000000','8.000000','4.000000','30.00000000000000')`,
  `#690 = ellipse_arc_feature('1','5','1','3','250.000000','245.000000','8.000000','4.000000','1','-15.00000000000000','10.00000000000000','200.00000000000000')`,
  // 文字: 配置基点、縦書き、スラント、文字間隔、幅の省略、空文字、半角カナ、\、フォント
  String.raw`#700 = text_string_feature('2','1','1',\'平面図\','10.000000','220.000000','5.000000','15.000000','0.000000','0.00000000000000','0.00000000000000','1','1')`,
  String.raw`#710 = text_string_feature('2','2','2',\'道路\','40.000000','220.000000','5.000000','12.000000','1.000000','30.00000000000000','15.00000000000000','5','1')`,
  String.raw`#720 = text_string_feature('2','3','2',\'縦書き\','70.000000','220.000000','15.000000','5.000000','0.500000','0.00000000000000','0.00000000000000','9','2')`,
  String.raw`#730 = text_string_feature('2','4','9',\'ｶﾀｶﾅ ABC\\x\','90.000000','220.000000','4.000000','0.000000','0.000000','0.00000000000000','-10.00000000000000','3','1')`,
  String.raw`#740 = text_string_feature('2','5','1',\'\','120.000000','220.000000','4.000000','10.000000','0.000000','0.00000000000000','0.00000000000000','1','1')`,
  String.raw`#750 = text_string_feature('2','6','1',\'表示ソ能\','130.000000','220.000000','4.000000','16.000000','0.000000','90.00000000000000','0.00000000000000','7','1')`,
  // スプライン（3n+1 点、余りのある点、閉じたもの）とクロソイド
  `#760 = spline_feature('1','1','1','1','1','4','(10.000000,20.000000,30.000000,40.000000)','(190.000000,200.000000,180.000000,190.000000)')`,
  `#770 = spline_feature('1','2','1','1','1','6','(50.000000,60.000000,70.000000,80.000000,90.000000,100.000000)','(190.000000,200.000000,180.000000,190.000000,200.000000,190.000000)')`,
  `#780 = spline_feature('1','3','1','1','0','4','(110.000000,120.000000,130.000000,110.000000)','(190.000000,200.000000,180.000000,190.000000)')`,
  `#790 = clothoid_feature('1','4','1','1','150.000000','180.000000','30.000000','0','0.00000000000000','10.000000','40.000000')`,
  `#800 = clothoid_feature('1','5','1','1','200.000000','180.000000','30.000000','1','90.00000000000000','0.000000','40.000000')`,
  // A の 300 倍の長さのクロソイド（積分の分割数を打ち切る）と、32 ビットを超えるレイヤコード
  `#802 = clothoid_feature('1','6','1','1','250.000000','180.000000','0.100000','0','0.00000000000000','0.000000','30.000000')`,
  `#804 = line_feature('123456789012','1','1','1','260.000000','180.000000','280.000000','180.000000')`,
  // 複合図形の配置: 部分図（縮尺 1/20 と、縦横の縮尺が違う回転）、測地、作図グループ、循環、未定義、縮尺 0、重複
  String.raw`#810 = sfig_locate_feature('1',\'部分図\','20.000000','100.000000','0.00000000000000','0.05000000000000','0.05000000000000')`,
  String.raw`#820 = sfig_locate_feature('1',\'部分図\','100.000000','100.000000','30.00000000000000','0.10000000000000','0.05000000000000')`,
  String.raw`#830 = sfig_locate_feature('3',\'測地\','200.000000','100.000000','0.00000000000000','0.50000000000000','0.50000000000000')`,
  String.raw`#840 = sfig_locate_feature('0',\'グループ\','500.000000','500.000000','45.00000000000000','3.00000000000000','3.00000000000000')`,
  String.raw`#850 = sfig_locate_feature('0',\'循環A\','250.000000','150.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#860 = sfig_locate_feature('0',\'存在しない\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#870 = sfig_locate_feature('2',\'部品\','0.000000','0.000000','0.00000000000000','0.00000000000000','0.00000000000000')`,
  String.raw`#880 = sfig_locate_feature('0',\'重複\','260.000000','160.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  // 既定義シンボル（色の指定なし・あり、名前なし）
  String.raw`#890 = externally_defined_symbol_feature('1','0','0',\'40201050100000&&a0010010\','20.000000','160.000000','0.00000000000000','1.00000000000000')`,
  String.raw`#900 = externally_defined_symbol_feature('1','1','2',\'\','40.000000','160.000000','30.00000000000000','2.00000000000000')`,
  // 寸法: 直線（補助線・矢印・文字あり、すべてなし）、弧長、角度、半径、直径
  `#910 = linear_dim_feature('2','1','1','1','10.000000','130.000000','60.000000','130.000000','1','10.000000','120.000000','10.000000','122.000000','10.000000','132.000000','1','60.000000','120.000000','60.000000','122.000000','60.000000','132.000000','6','1','10.000000','130.000000','1.00000000000000','9','2','60.000000','130.000000','0.50000000000000',${annotation("50", 30, 131)})`,
  `#920 = linear_dim_feature('2','1','1','1','70.000000','130.000000','90.000000','130.000000','0','1','0','0','0','0','0','0','1','0','0','0','0','0','1','0','1','0','0','0','1','0','1','0','0','0','0')`,
  `#930 = curve_dim_feature('2','2','1','1','120.000000','120.000000','15.000000','0.00000000000000','90.00000000000000','1','120.000000','120.000000','125.000000','120.000000','137.000000','120.000000','1','120.000000','120.000000','120.000000','125.000000','120.000000','137.000000','1','1','135.000000','120.000000','1.00000000000000','3','2','120.000000','135.000000','1.00000000000000',${annotation("23.56", 128, 133)})`,
  `#940 = angular_dim_feature('2','3','1','1','160.000000','120.000000','15.000000','30.00000000000000','150.00000000000000','0','0','0','0','0','0','0','0','0','0','0','0','0','0','4','1','172.990381','127.500000','1.00000000000000','8','1','147.009619','127.500000','1.00000000000000',${annotation("120°", 155, 136)})`,
  `#950 = radius_dim_feature('2','4','1','1','200.000000','120.000000','210.606602','130.606602','5','1','210.606602','130.606602','1.00000000000000',${annotation("R15", 203, 128)})`,
  `#960 = diameter_dim_feature('2','5','1','1','230.000000','120.000000','260.000000','120.000000','7','2','230.000000','120.000000','1.00000000000000','10','1','260.000000','120.000000','1.00000000000000',${annotation("φ30", 240, 121)})`,
  `#965 = diameter_dim_feature('2','6','1','1','270.000000','120.000000','290.000000','120.000000','11','1','270.000000','120.000000','1.00000000000000','2','2','290.000000','120.000000','1.00000000000000','0')`,
  // 引出し線（矢印あり・なし）とバルーン
  `#970 = label_feature('2','1','1','1','3','(10.000000,20.000000,40.000000)','(90.000000,100.000000,100.000000)','2','1.00000000000000',${annotation("メモ", 22, 101)})`,
  `#980 = label_feature('2','2','1','1','2','(50.000000,60.000000)','(90.000000,100.000000)','0','1.00000000000000','0')`,
  `#990 = balloon_feature('2','3','1','1','2','(80.000000,95.000000)','(90.000000,100.000000)','100.000000','100.000000','5.000000','6','1.00000000000000',${annotation("1", 99, 99, "'5','1'")})`,
  // ハッチング: 外部定義、Area_control、塗り（穴あり）、ユーザ定義（2 本・間隔 0・細かすぎ）、パターン、外形なし
  String.raw`#1000 = externally_defined_hatch_feature('3',\'sxf_hatch_style_1\','1','0','()')`,
  String.raw`#1010 = externally_defined_hatch_feature('3',\'Area_control\','4','0','()')`,
  `#1020 = fill_area_style_colour_feature('3','17','1','1','(2)')`,
  `#1030 = fill_area_style_hatching_feature('3','3','(1,1,1,0.000000,0.000000,3.000000,45.0000000000000)','(2,2,3,0.000000,0.000000,4.000000,-30.0000000000000)','(3,1,1,0.000000,0.000000,0.000000,0.0000000000000)','4','0','()')`,
  `#1040 = fill_area_style_hatching_feature('3','1','(1,1,1,0.000000,0.000000,0.000100,0.0000000000000)','3','0','()')`,
  String.raw`#1050 = fill_area_style_tiles_feature('3',\'40201050100000&&a0010010\','2','200.000000','0.000000','10.000000','0.00000000000000','10.000000','90.00000000000000','1.00000000000000','1.00000000000000','0.00000000000000','5','0','()')`,
  `#1060 = fill_area_style_colour_feature('3','2','99','0','()')`,
  // 読めないもの: 未対応のフィーチャ、引数の不足、数値でない引数
  `#1070 = unknown_shape_feature('1','1')`,
  `#1080 = line_feature('1','1','1','1','0.000000','0.000000')`,
  `#1090 = circle_feature('1','1','1','1','abc','0.000000','5.000000')`,
  // 入れ子が深すぎる引数（読めないフィーチャ）
  `#1095 = polyline_feature('1','1','1','1','1',${"(".repeat(70)}'1'${")".repeat(70)},'(0)')`,
])}/*SXF
#1100 line_feature('1','1','1','1','0','0','1','1')
SXF*/
/*SXF
#1110 = line_feature('1','1','1','1','0','0'
SXF*/
${records(
  [
    String.raw`#1120 = drawing_attribute_feature(\'道路改良事業\',\'工事\',\'\',\'平面図\',\'No.1\',\' \',\'1:500\','2026','10','18',\'受注会社\',\'発注者\')`,
  ],
  "3",
)}${records([
  String.raw`#1130 = drawing_sheet_feature(\'平面図\','3','0','0','0')`,
  String.raw`#1140 = layer_feature(\'道路\','1')`,
  String.raw`#1150 = layer_feature(\'寸法\','1')`,
  String.raw`#1160 = layer_feature(\'非表示\','0')`,
  String.raw`#1170 = layer_feature(\'\','1')`,
  `#1180 = line_feature('2','1','1','1','0.000000','0.000000','10.000000','10.000000')`,
  String.raw`#1190 = drawing_sheet_feature(\'2 枚目\','1','1','0','0')`,
])}${records([`#1200 = line_feature('1','1','1','1','0.000000','10.000000','10.000000','0.000000')`], "3.1")}ENDSEC;
END-ISO-10303-21;
`;

/**
 * 画像（ラスタ）は作図グループで、名前が属性付加機構の「画像」（$$ATRU$$番号$$名称$$画像$$ファイル名）か
 * Ver.2.0 の $$RASTER$$ファイル名。範囲はグループの中の折線（左下の角から、時計回りでも反時計回りでも）。
 * 折線を読み違えると、画像が回ったり裏返ったりしたまま黙って描かれる
 */
export const SFC_RASTERS = `${SFC_HEADER("rasters.sfc", "SCADEC_API_Ver3.10$$3.1")}${records([
  String.raw`#2 = layer_feature(\'枠\','1')`,
  String.raw`#4 = layer_feature(\'画像\','1')`,
  // 時計回り（左下・左上・右上・右下、山口県の公開図面を書いた CAD の順）の 100 × 50 mm、レイヤ 2
  `#10 = polyline_feature('2','4','1','1','5','(20.000000,20.000000,120.000000,120.000000,20.000000)','(30.000000,80.000000,80.000000,30.000000,30.000000)')`,
  String.raw`#20 = sfig_org_feature(\'$$ATRU$$100$$地形図$$画像$$GENKYOU.TIF\','3')`,
  // 反時計回りで 30° 回した 40 × 20 mm（Ver.2.0 の名前）
  `#30 = polyline_feature('1','1','1','1','5','(150.000000,184.641016,174.641016,140.000000,150.000000)','(30.000000,50.000000,67.320508,47.320508,30.000000)')`,
  String.raw`#40 = sfig_org_feature(\'$$RASTER$$photo.jpg\','3')`,
  // 範囲の折線のない画像と、画像でない属性（背景色）
  `#50 = line_feature('1','1','1','1','200.000000','100.000000','220.000000','100.000000')`,
  String.raw`#60 = sfig_org_feature(\'$$ATRU$$101$$写真$$画像$$missing.jpg\','3')`,
  `#70 = line_feature('1','1','1','1','0.000000','0.000000','1.000000','0.000000')`,
  String.raw`#80 = sfig_org_feature(\'$$ATRU$$2$$背景色$$色$$255_255_255\','3')`,
  // 画像の前の線（画像はこの線を隠す）と、後の線
  `#130 = line_feature('1','1','1','1','10.000000','50.000000','200.000000','50.000000')`,
  String.raw`#140 = sfig_locate_feature('0',\'$$ATRU$$100$$地形図$$画像$$GENKYOU.TIF\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#150 = sfig_locate_feature('0',\'$$RASTER$$photo.jpg\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#160 = sfig_locate_feature('0',\'$$ATRU$$101$$写真$$画像$$missing.jpg\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  String.raw`#170 = sfig_locate_feature('0',\'$$ATRU$$2$$背景色$$色$$255_255_255\','0.000000','0.000000','0.00000000000000','1.00000000000000','1.00000000000000')`,
  `#180 = line_feature('1','1','1','1','10.000000','60.000000','200.000000','60.000000')`,
  String.raw`#190 = drawing_sheet_feature(\'画像\','3','1','0','0')`,
])}ENDSEC;
END-ISO-10303-21;
`;

/** 用紙のない SFC。要素はすべて用紙に置き、版はフィーチャの囲み（/*SXF3）から決める */
export const SFC_NO_SHEET = `${SFC_HEADER("nosheet.sfc", "zumen")}${records(
  [
    `#10 = line_feature('1','1','1','1','0.000000','0.000000','10.000000','0.000000')`,
    `#20 = circle_feature('1','2','1','1','5.000000','5.000000','3.000000')`,
  ],
  "3",
)}ENDSEC;
END-ISO-10303-21;
`;

const OCCURRENCE = "ANNOTATION_OCCURRENCE() DRAUGHTING_ANNOTATION_OCCURRENCE() GEOMETRIC_REPRESENTATION_ITEM()";

/**
 * P21 は SFC と同じ図面を AP202 の参照の網で表す。文字の配置点のずれ、子図の縮尺、円弧の向き、
 * 矢印の向き、非表示の指定を読み違えると、表示が黙って SFC 版と食い違うため、その対応を確かめる。
 */
export const P21_SAMPLE = String.raw`ISO-10303-21;
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

const P21_HEADER = (name: string, preprocessor: string) => `ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1');
FILE_NAME('${name}','2026-10-18T00:00:00',('author'),('org'),'${preprocessor}','zumen \\X2\\30C630B930C8\\X0\\ it''s','');
FILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));
ENDSEC;
DATA;
`;

/** 表記線（ANNOTATION_CURVE_OCCURRENCE）。extra は DIMENSION_CURVE() などの追加の型 */
function curveOccurrence(id: number, style: number, item: number, extra = "", name = " "): string {
  return `#${id}=(ANNOTATION_CURVE_OCCURRENCE() ${OCCURRENCE} ${extra} REPRESENTATION_ITEM('${name}') STYLED_ITEM((#${style}),#${item}));`;
}

/** 文字の表記一式（範囲・配置・文字列・スタイル・occurrence）。id から id+6 を使う */
function textOccurrence(
  id: number,
  options: {
    text: string;
    x: number;
    y: number;
    anchor: string;
    width?: number;
    height?: number;
    direction?: string;
    path?: string;
    font?: string;
    color?: string;
    slant?: number;
    spacing?: number;
  },
): string {
  const { text, x, y, anchor, width = 20, height = 5, direction = "#42", path = ".RIGHT.", font = "#64" } = options;
  return [
    `#${id}=PLANAR_EXTENT(' ',${width},${height});`,
    `#${id + 1}=CARTESIAN_POINT(' ',(${x},${y}));`,
    `#${id + 2}=AXIS2_PLACEMENT_2D(' ',#${id + 1},${direction});`,
    `#${id + 3}=TEXT_LITERAL_WITH_EXTENT('${anchor}','${text}',#${id + 2},'baseline left',${path},${font},#${id});`,
    `#${id + 4}=(TEXT_STYLE(' ',${options.color ?? "#66"}) TEXT_STYLE_WITH_BOX_CHARACTERISTICS((BOX_HEIGHT(${height}),BOX_WIDTH(4.0),BOX_SLANT_ANGLE(${options.slant ?? 0}),BOX_ROTATE_ANGLE(0.0))) TEXT_STYLE_WITH_SPACING(LENGTH_MEASURE(${options.spacing ?? 0})));`,
    `#${id + 5}=PRESENTATION_STYLE_ASSIGNMENT((#${id + 4}));`,
    `#${id + 6}=(${OCCURRENCE} ANNOTATION_TEXT_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#${id + 5}),#${id + 3}));`,
  ].join("\n");
}

/** 端末記号（矢印）の occurrence 一式。id から id+4 を使う */
function terminator(
  id: number,
  role: string,
  name: string,
  x: number,
  y: number,
  dx: number,
  dy: number,
  of: number,
  type = "DIMENSION_CURVE_TERMINATOR",
): string {
  const roleArgs = type === "DIMENSION_CURVE_TERMINATOR" ? `(.${role}.)` : "()";
  return [
    `#${id}=CARTESIAN_POINT(' ',(${x},${y}));`,
    `#${id + 1}=DIRECTION(' ',(${dx},${dy}));`,
    `#${id + 2}=AXIS2_PLACEMENT_2D(' ',#${id},#${id + 1});`,
    `#${id + 3}=(SYMBOL_TARGET(' ',#${id + 2},0.800000,0.800000));`,
    `#${id + 4}=DEFINED_SYMBOL(' ',#${name},#${id + 3});`,
    `#${id + 5}=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() ${type}${roleArgs} REPRESENTATION_ITEM('$$SXF_arw') STYLED_ITEM((#92),#${id + 4}) TERMINATOR_SYMBOL(#${of}));`,
  ].join("\n");
}

/** 2 点を結ぶトリム直線。id から id+5 を使い、id+5 が TRIMMED_CURVE */
function trimmedLine(id: number, x1: number, y1: number, x2: number, y2: number): string {
  return [
    `#${id}=CARTESIAN_POINT(' ',(${x1},${y1}));`,
    `#${id + 1}=CARTESIAN_POINT(' ',(${x2},${y2}));`,
    `#${id + 2}=DIRECTION(' ',(${x2 - x1},${y2 - y1}));`,
    `#${id + 3}=VECTOR(' ',#${id + 2},1.0);`,
    `#${id + 4}=LINE(' ',#${id},#${id + 3});`,
    `#${id + 5}=TRIMMED_CURVE(' ',#${id + 4},(#${id}),(#${id + 1}),.T.,.CARTESIAN.);`,
  ].join("\n");
}

/**
 * AP202 サブセット編のテンプレートの組み合わせを広く使い、読み取りの注意が出る書き方も含めた P21。
 * 曲線（トリム・パラメータ・座標でのトリム、ベジェ、クロソイド、未対応の曲線）、文字の配置基点と縦書き、
 * 点マーカと既定義シンボル、子図の 5 つの種類と入れ子・循環、塗り・ハッチング・外部定義・パターン、
 * 寸法 5 種と引出し線・バルーン、レイヤ・非表示、8 ビットの Shift_JIS と STEP のエスケープを含む。
 */
export const P21_FEATURES = `${P21_HEADER("features.p21", "SCADEC_API_Ver3.10$$3.0")}${[
  "#5=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));",
  "#6=(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.));",
  "#7=(GEOMETRIC_REPRESENTATION_CONTEXT(2) GLOBAL_UNIT_ASSIGNED_CONTEXT((#5,#6)) REPRESENTATION_CONTEXT('ID1','2D'));",
  // 色: 既定義、名前で既定義になる RGB、ユーザ定義、名前のない既定義
  "#10=DRAUGHTING_PRE_DEFINED_COLOUR('white');",
  "#11=DRAUGHTING_PRE_DEFINED_COLOUR('red');",
  "#12=COLOUR_RGB('$$SXF_brown',0.750000,0.500000,0.250000);",
  "#13=COLOUR_RGB(' ',0.100000,0.200000,0.300000);",
  "#14=COLOUR_RGB(' ',0.500000,0.500000,0.500000);",
  "#15=DRAUGHTING_PRE_DEFINED_COLOUR('purple');",
  // 線種: 既定義、名前の分からない既定義、ユーザ定義
  "#20=DRAUGHTING_PRE_DEFINED_CURVE_FONT('continuous');",
  "#21=DRAUGHTING_PRE_DEFINED_CURVE_FONT('chain');",
  "#22=DRAUGHTING_PRE_DEFINED_CURVE_FONT('wavy');",
  "#23=CURVE_STYLE_FONT_PATTERN(4.000000,1.000000);",
  "#24=CURVE_STYLE_FONT_PATTERN(0.500000,1.000000);",
  "#25=CURVE_STYLE_FONT('$$SXF_custom',(#23,#24));",
  // 線幅: 既定義、ユーザ定義（同じ値は同じコード）
  "#26=LENGTH_MEASURE_WITH_UNIT(POSITIVE_LENGTH_MEASURE(0.250000),#5);",
  "#27=LENGTH_MEASURE_WITH_UNIT(POSITIVE_LENGTH_MEASURE(0.300000),#5);",
  "#28=LENGTH_MEASURE_WITH_UNIT(POSITIVE_LENGTH_MEASURE(0.3000001),#5);",
  // 曲線のスタイル: 既定義、ユーザ定義の線種・線幅・色、数値の線幅、定義のない色、読めない線幅
  "#30=CURVE_STYLE(' ',#20,#26,#10);",
  "#31=PRESENTATION_STYLE_ASSIGNMENT((#30));",
  "#32=CURVE_STYLE(' ',#25,#27,#13);",
  "#33=PRESENTATION_STYLE_ASSIGNMENT((#32));",
  "#34=CURVE_STYLE(' ',#21,0.700000,#12);",
  "#35=PRESENTATION_STYLE_ASSIGNMENT((#34));",
  "#36=CURVE_STYLE(' ',#22,#28,#99999);",
  "#37=PRESENTATION_STYLE_ASSIGNMENT((#36));",
  "#38=CURVE_STYLE(' ',$,$,#14);",
  "#39=PRESENTATION_STYLE_ASSIGNMENT((#38));",
  "#40=CARTESIAN_POINT(' ',(0.000000,0.000000));",
  "#41=CARTESIAN_POINT(' ',(100.000000,0.000000));",
  "#42=DIRECTION(' ',(1.000000,0.000000));",
  "#43=VECTOR(' ',#42,1.000000);",
  "#44=LINE(' ',#40,#43);",
  "#45=AXIS2_PLACEMENT_2D(' ',#40,$);",
  // 文字のフォントと色
  "#60=EXTERNAL_SOURCE(IDENTIFIER('scadec'));",
  "#61=EXTERNALLY_DEFINED_TEXT_FONT(IDENTIFIER('\\X2\\FF2DFF33\\X0\\ \\X2\\660E671D\\X0\\'),#60);",
  "#64=EXTERNALLY_DEFINED_TEXT_FONT(IDENTIFIER('century'),#60);",
  "#66=TEXT_STYLE_FOR_DEFINED_FONT(#11);",
  "#67=TEXT_STYLE_FOR_DEFINED_FONT(#88888);",
  // 端末記号のスタイルと記号
  "#90=SYMBOL_COLOUR(#11);",
  "#91=SYMBOL_STYLE(' ',#90);",
  "#92=PRESENTATION_STYLE_ASSIGNMENT((#91));",
  "#93=PRE_DEFINED_TERMINATOR_SYMBOL('filled arrow');",
  "#94=PRE_DEFINED_TERMINATOR_SYMBOL('Blanked Box');",
  "#95=PRE_DEFINED_TERMINATOR_SYMBOL('unknown arrow');",
  "#96=PRESENTATION_STYLE_ASSIGNMENT((NULL_STYLE(.NULL.)));",
  // 線分（座標でトリム、パラメータでトリム）と折線・円・楕円
  "#100=TRIMMED_CURVE(' ',#44,(#40),(#41),.T.,.CARTESIAN.);",
  curveOccurrence(101, 31, 100),
  "#102=CARTESIAN_POINT(' ',(0.000000,10.000000));",
  "#103=DIRECTION(' ',(3.000000,4.000000));",
  "#104=VECTOR(' ',#103,2.000000);",
  "#105=LINE(' ',#102,#104);",
  "#106=TRIMMED_CURVE(' ',#105,(PARAMETER_VALUE(1.000000)),(PARAMETER_VALUE(5.000000)),.T.,.PARAMETER.);",
  curveOccurrence(107, 33, 106),
  "#108=CARTESIAN_POINT(' ',(20.000000,10.000000));",
  "#109=CARTESIAN_POINT(' ',(40.000000,30.000000));",
  "#110=CARTESIAN_POINT(' ',(60.000000,10.000000));",
  "#111=POLYLINE(' ',(#108,#109,#110));",
  curveOccurrence(112, 35, 111),
  "#113=CARTESIAN_POINT(' ',(80.000000,20.000000));",
  "#114=AXIS2_PLACEMENT_2D(' ',#113,$);",
  "#115=CIRCLE(' ',#114,8.000000);",
  curveOccurrence(116, 37, 115),
  "#117=DIRECTION(' ',(0.000000,2.000000));",
  "#118=CARTESIAN_POINT(' ',(110.000000,20.000000));",
  "#119=AXIS2_PLACEMENT_2D(' ',#118,#117);",
  "#120=ELLIPSE(' ',#119,10.000000,4.000000);",
  curveOccurrence(121, 39, 120),
  // 円弧（.T. と .F.、座標でのトリム）と楕円弧
  "#122=CARTESIAN_POINT(' ',(140.000000,20.000000));",
  "#123=DIRECTION(' ',(0.000000,1.000000));",
  "#124=AXIS2_PLACEMENT_2D(' ',#122,#123);",
  "#125=CIRCLE(' ',#124,6.000000);",
  "#126=TRIMMED_CURVE(' ',#125,(PARAMETER_VALUE(0.500000)),(PARAMETER_VALUE(2.000000)),.T.,.PARAMETER.);",
  curveOccurrence(127, 31, 126),
  "#128=TRIMMED_CURVE(' ',#125,(PARAMETER_VALUE(0.500000)),(PARAMETER_VALUE(2.000000)),.F.,.PARAMETER.);",
  curveOccurrence(129, 33, 128),
  "#130=CARTESIAN_POINT(' ',(146.000000,20.000000));",
  "#131=CARTESIAN_POINT(' ',(140.000000,14.000000));",
  "#132=TRIMMED_CURVE(' ',#125,(#130),(#131),.T.,.CARTESIAN.);",
  curveOccurrence(133, 35, 132),
  "#134=ELLIPSE(' ',#119,10.000000,4.000000);",
  "#135=TRIMMED_CURVE(' ',#134,(PARAMETER_VALUE(0.000000)),(PARAMETER_VALUE(3.141593)),.F.,.PARAMETER.);",
  curveOccurrence(136, 31, 135),
  "#137=CARTESIAN_POINT(' ',(120.000000,20.000000));",
  "#138=TRIMMED_CURVE(' ',#134,(#137,PARAMETER_VALUE(0.100000)),(#118),.T.,.PARAMETER.);",
  curveOccurrence(139, 31, 138),
  // ベジェ（開・閉）、クロソイド（反時計回り・時計回り）、未対応の曲線、何も指さない occurrence
  "#140=CARTESIAN_POINT(' ',(10.000000,40.000000));",
  "#141=CARTESIAN_POINT(' ',(20.000000,50.000000));",
  "#142=CARTESIAN_POINT(' ',(30.000000,30.000000));",
  "#143=CARTESIAN_POINT(' ',(40.000000,40.000000));",
  "#144=BEZIER_CURVE(' ',3,(#140,#141,#142,#143),.UNSPECIFIED.,.F.,.U.);",
  curveOccurrence(145, 31, 144),
  "#146=BEZIER_CURVE(' ',3,(#140,#141,#142,#143,#140),.UNSPECIFIED.,.T.,.U.);",
  curveOccurrence(147, 33, 146),
  "#148=CARTESIAN_POINT(' ',(60.000000,40.000000));",
  "#149=AXIS2_PLACEMENT_2D(' ',#148,#42);",
  "#150=CLOTHOID('0',#149,20.000000);",
  "#151=TRIMMED_CURVE('0.000000,30.000000',#150,(PARAMETER_VALUE((0.000000))),(PARAMETER_VALUE((30.000000))),.F.,.PARAMETER.);",
  curveOccurrence(152, 31, 151),
  "#153=CLOTHOID('1 ',#149,20.000000);",
  "#154=TRIMMED_CURVE(' ',#153,(PARAMETER_VALUE(5.000000)),(PARAMETER_VALUE(25.000000)),.F.,.PARAMETER.);",
  curveOccurrence(155, 33, 154),
  "#156=B_SPLINE_CURVE(' ',3,(#140,#141,#142,#143),.UNSPECIFIED.,.F.,.U.);",
  curveOccurrence(157, 31, 156),
  curveOccurrence(158, 31, 99990),
  // 文字: 配置基点 9 種類、縦書き、スラント、文字間隔、フォントと色
  textOccurrence(200, { text: "\\X2\\5DE64E0B\\X0\\", x: 10, y: 60, anchor: "$$SXF_baseline left" }),
  textOccurrence(210, { text: "baseline centre", x: 40, y: 60, anchor: "$$SXF_baseline centre", font: "#61" }),
  textOccurrence(220, { text: "baseline right", x: 70, y: 60, anchor: "$$SXF_baseline   right" }),
  textOccurrence(230, { text: "middle left", x: 10, y: 70, anchor: "$$SXF_middleline left", spacing: 1 }),
  textOccurrence(240, { text: "middle centre", x: 40, y: 70, anchor: "$$SXF_middleline center", slant: 0.2 }),
  textOccurrence(250, { text: "middle right", x: 70, y: 70, anchor: "$$SXF_middleline right", color: "#67" }),
  textOccurrence(260, { text: "top left", x: 10, y: 80, anchor: "$$SXF_topline left", direction: "#123" }),
  textOccurrence(270, { text: "top centre", x: 40, y: 80, anchor: "$$SXF_topline centre", font: "$" }),
  textOccurrence(280, {
    text: "\\X2\\7E2666F8304D\\X0\\",
    x: 70,
    y: 80,
    anchor: "$$SXF_topline right",
    path: ".DOWN.",
  }),
  textOccurrence(290, { text: "", x: 90, y: 80, anchor: "", width: 0 }),
  // 点マーカ（名前 7 種と不明な名前）と既定義シンボル（色あり・なし）
  ...["asterisk", "Circle", "dot", "plus", "square", "triangle", "x", "star"].map((name, index) => {
    const id = 300 + index * 10;
    return [
      `#${id}=CARTESIAN_POINT(' ',(${10 + index * 10},100.000000));`,
      `#${id + 1}=AXIS2_PLACEMENT_2D(' ',#${id},#${index % 2 === 0 ? 42 : 103});`,
      `#${id + 2}=SYMBOL_TARGET(' ',#${id + 1},${index === 3 ? 2 : 1},${index === 3 ? 2 : 1});`,
      `#${id + 3}=PRE_DEFINED_POINT_MARKER_SYMBOL('${name}');`,
      `#${id + 4}=DEFINED_SYMBOL(' ',#${id + 3},#${id + 2});`,
      `#${id + 5}=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#92),#${id + 4}));`,
    ].join("\n");
  }),
  "#380=CARTESIAN_POINT(' ',(100.000000,100.000000));",
  "#381=AXIS2_PLACEMENT_2D(' ',#380,#103);",
  "#382=SYMBOL_TARGET(' ',#381,1.500000,1.500000);",
  "#383=EXTERNALLY_DEFINED_SYMBOL(IDENTIFIER('40201050100000&&a0010010'),#60);",
  "#384=DEFINED_SYMBOL(' ',#383,#382);",
  `#385=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#92),#384));`,
  `#386=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#384));`,
  "#387=DEFINED_SYMBOL(' ',#93,#382);",
  `#388=(${OCCURRENCE} ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#92),#387));`,
  // 子図: 数学座標系（入れ子と、同じ定義の 2 回の配置）、測地座標系、作図グループ、作図部品、接頭語なし、循環
  "#400=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_P_\\X2\\90E854C1\\X0\\',(#101,#116,#45),#7);",
  "#401=SYMBOL_REPRESENTATION_MAP(#45,#400);",
  "#402=CARTESIAN_POINT(' ',(500.000000,500.000000));",
  "#403=AXIS2_PLACEMENT_2D(' ',#402,#103);",
  "#404=SYMBOL_TARGET(' ',#403,2.000000,2.000000);",
  "#405=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#401,#404) REPRESENTATION_ITEM(' '));",
  `#406=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#405));`,
  "#410=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_FM_\\X2\\5E73976256F3\\X0\\',(#406,#101,#127,#206,#305,#45),#7);",
  "#411=SYMBOL_REPRESENTATION_MAP(#45,#410);",
  "#412=CARTESIAN_POINT(' ',(150.000000,100.000000));",
  "#413=AXIS2_PLACEMENT_2D(' ',#412,#42);",
  "#414=SYMBOL_TARGET(' ',#413,0.050000,0.050000);",
  "#415=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#411,#414) REPRESENTATION_ITEM(' '));",
  `#416=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#415));`,
  "#417=CARTESIAN_POINT(' ',(150.000000,150.000000));",
  "#418=AXIS2_PLACEMENT_2D(' ',#417,#103);",
  "#419=SYMBOL_TARGET(' ',#418,0.100000,0.050000);",
  "#420=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#411,#419) REPRESENTATION_ITEM(' '));",
  `#421=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#420));`,
  "#430=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_FG_\\X2\\6E2C5730\\X0\\',(#101,#133,#45),#7);",
  "#431=SYMBOL_REPRESENTATION_MAP(#45,#430);",
  "#432=CARTESIAN_POINT(' ',(250.000000,100.000000));",
  "#433=AXIS2_PLACEMENT_2D(' ',#432,#42);",
  "#434=SYMBOL_TARGET(' ',#433,0.500000,0.500000);",
  "#435=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#431,#434) REPRESENTATION_ITEM(' '));",
  `#436=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#435));`,
  "#440=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_G_group',(#107,#45),#7);",
  "#441=SYMBOL_REPRESENTATION_MAP(#45,#440);",
  "#442=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#441,#434) REPRESENTATION_ITEM(' '));",
  `#443=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#442));`,
  "#450=DRAUGHTING_SUBFIGURE_REPRESENTATION('loop',(#456,#112,#45),#7);",
  "#451=SYMBOL_REPRESENTATION_MAP(#45,#450);",
  "#452=CARTESIAN_POINT(' ',(300.000000,100.000000));",
  "#453=AXIS2_PLACEMENT_2D(' ',#452,#42);",
  "#454=SYMBOL_TARGET(' ',#453,1.000000,1.000000);",
  "#455=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#451,#454) REPRESENTATION_ITEM(' '));",
  `#456=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#455));`,
  "#460=SYMBOL_REPRESENTATION_MAP(#45,#7);",
  "#461=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#460,#454) REPRESENTATION_ITEM(' '));",
  `#462=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#461));`,
  "#463=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_P_',(#101,#45),#7);",
  "#464=SYMBOL_REPRESENTATION_MAP(#45,#463);",
  "#465=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#464,#454) REPRESENTATION_ITEM(' '));",
  `#466=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#465));`,
  // 複合曲線: 外形（折線）、穴（トリム円・トリム直線）、ベジェを含むもの、非表示の外形
  "#500=CARTESIAN_POINT(' ',(0.000000,200.000000));",
  "#501=CARTESIAN_POINT(' ',(60.000000,200.000000));",
  "#502=CARTESIAN_POINT(' ',(60.000000,240.000000));",
  "#503=CARTESIAN_POINT(' ',(0.000000,240.000000));",
  "#504=POLYLINE(' ',(#500,#501,#502,#503,#500));",
  "#505=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#504);",
  "#506=COMPOSITE_CURVE(' ',(#505),.F.);",
  curveOccurrence(507, 33, 506),
  "#510=CARTESIAN_POINT(' ',(30.000000,220.000000));",
  "#511=AXIS2_PLACEMENT_2D(' ',#510,$);",
  "#512=CIRCLE(' ',#511,8.000000);",
  "#513=TRIMMED_CURVE(' ',#512,(PARAMETER_VALUE(0.000000)),(PARAMETER_VALUE(3.141593)),.T.,.PARAMETER.);",
  "#514=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#513);",
  trimmedLine(515, 22, 220, 38, 220),
  "#521=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#520);",
  "#522=COMPOSITE_CURVE(' ',(#514,#521),.F.);",
  curveOccurrence(523, 31, 522),
  "#530=CARTESIAN_POINT(' ',(100.000000,200.000000));",
  "#531=CARTESIAN_POINT(' ',(140.000000,200.000000));",
  "#532=CARTESIAN_POINT(' ',(120.000000,230.000000));",
  "#533=POLYLINE(' ',(#530,#531,#532,#530));",
  "#534=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#533);",
  "#535=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#144);",
  "#536=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#99991);",
  "#537=COMPOSITE_CURVE(' ',(#534,#535,#536),.F.);",
  curveOccurrence(538, 35, 537),
  "#540=CARTESIAN_POINT(' ',(200.000000,200.000000));",
  "#541=CARTESIAN_POINT(' ',(260.000000,200.000000));",
  "#542=CARTESIAN_POINT(' ',(260.000000,230.000000));",
  "#543=POLYLINE(' ',(#540,#541,#542,#540));",
  "#544=COMPOSITE_CURVE_SEGMENT(.CONTINUOUS.,.T.,#543);",
  "#545=COMPOSITE_CURVE(' ',(#544),.F.);",
  // 塗り（穴あり）、ユーザ定義ハッチング 2 本、外部定義、パターン、外形のない塗り、複合曲線の occurrence のない外形
  "#550=ANNOTATION_FILL_AREA(' ',(#506,#522));",
  "#551=FILL_AREA_STYLE_COLOUR(' ',#12);",
  "#552=FILL_AREA_STYLE(' ',(#551));",
  "#553=PRESENTATION_STYLE_ASSIGNMENT((#552));",
  `#554=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#553),#550));`,
  "#560=ANNOTATION_FILL_AREA(' ',(#537));",
  "#561=CARTESIAN_POINT(' ',(100.000000,200.000000));",
  "#562=DIRECTION(' ',(0.000000,1.000000));",
  "#563=VECTOR(' ',#562,3.000000);",
  "#564=ONE_DIRECTION_REPEAT_FACTOR(' ',#563);",
  "#565=FILL_AREA_STYLE_HATCHING(' ',#32,#564,#561,#561,0.785398);",
  "#566=FILL_AREA_STYLE_HATCHING(' ',#34,#564,#40,#40,-0.523599);",
  "#567=FILL_AREA_STYLE(' ',(#565,#566));",
  "#568=PRESENTATION_STYLE_ASSIGNMENT((#567));",
  `#569=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#568),#560));`,
  "#570=ANNOTATION_FILL_AREA(' ',(#545));",
  "#571=EXTERNALLY_DEFINED_HATCH_STYLE(IDENTIFIER('sxf_hatch_style_2'),#60);",
  "#572=FILL_AREA_STYLE(' ',(#571));",
  "#573=PRESENTATION_STYLE_ASSIGNMENT((#572));",
  `#574=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#573),#570));`,
  "#575=FILL_AREA_STYLE_TILES(' ',(#40),1.000000);",
  "#576=FILL_AREA_STYLE(' ',(#575));",
  "#577=PRESENTATION_STYLE_ASSIGNMENT((#576));",
  `#578=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#577),#570));`,
  "#579=ANNOTATION_FILL_AREA(' ',(#504));",
  `#580=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#553),#579));`,
  "#581=FILL_AREA_STYLE(' ',());",
  "#582=PRESENTATION_STYLE_ASSIGNMENT((#581));",
  `#583=(ANNOTATION_FILL_AREA_OCCURRENCE(#40) ${OCCURRENCE} REPRESENTATION_ITEM(' ') STYLED_ITEM((#582),#570));`,
  // 直線寸法（補助線は名前の逆順に並ぶ）
  trimmedLine(600, 0, 300, 50, 300),
  curveOccurrence(606, 31, 605, "DIMENSION_CURVE()"),
  trimmedLine(610, 50, 290, 50, 305),
  curveOccurrence(616, 31, 615, "PROJECTION_CURVE()", "$$SXF_prj_2"),
  trimmedLine(620, 0, 290, 0, 305),
  curveOccurrence(626, 31, 625, "PROJECTION_CURVE()", "$$SXF_prj_1"),
  textOccurrence(630, { text: "50", x: 20, y: 301, anchor: "$$SXF_baseline centre", height: 2.5 }),
  terminator(640, "ORIGIN", "93", 0, 300, -1, 0, 606),
  terminator(650, "TARGET", "94", 50, 300, -1, 0, 606),
  "#660=(DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#606,#616,#626,#636,#645,#655)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LINEAR_DIMENSION() REPRESENTATION_ITEM(' '));",
  "#661=(DRAUGHTING_CALLOUT((#636)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM(' ') STRUCTURED_DIMENSION_CALLOUT());",
  "#662=(GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#660));",
  // 弧長寸法と角度寸法（円弧の寸法線）
  "#670=CARTESIAN_POINT(' ',(100.000000,300.000000));",
  "#671=AXIS2_PLACEMENT_2D(' ',#670,$);",
  "#672=CIRCLE(' ',#671,20.000000);",
  "#673=TRIMMED_CURVE(' ',#672,(PARAMETER_VALUE(0.000000)),(PARAMETER_VALUE(1.570796)),.T.,.PARAMETER.);",
  curveOccurrence(674, 35, 673, "DIMENSION_CURVE()"),
  textOccurrence(675, { text: "31.42", x: 110, y: 315, anchor: "$$SXF_baseline left", height: 2.5 }),
  terminator(690, "ORIGIN", "93", 120, 300, 0, -1, 674),
  terminator(700, "TARGET", "95", 100, 320, 1, 0, 674),
  "#710=(CURVE_DIMENSION() DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#674,#681,#695,#705)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM(' '));",
  "#711=(ANGULAR_DIMENSION() DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#674,#695,#705)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM(' '));",
  // 半径寸法（矢印は TARGET だけ）、直径寸法、対応していない寸法、寸法線のない寸法
  trimmedLine(720, 150, 300, 160, 310),
  curveOccurrence(726, 31, 725, "DIMENSION_CURVE()"),
  terminator(730, "TARGET", "93", 160, 310, 1, 1, 726),
  "#740=(DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#726,#735)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() RADIUS_DIMENSION() REPRESENTATION_ITEM(' '));",
  trimmedLine(750, 180, 300, 220, 300),
  curveOccurrence(756, 33, 755, "DIMENSION_CURVE()"),
  terminator(760, "ORIGIN", "93", 180, 300, 1, 0, 756),
  terminator(770, "TARGET", "93", 220, 300, -1, 0, 756),
  "#780=(DIAMETER_DIMENSION() DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#756,#765,#775)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() REPRESENTATION_ITEM(' '));",
  "#781=(DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#756)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() ORDINATE_DIMENSION() REPRESENTATION_ITEM(' '));",
  "#782=(DIMENSION_CURVE_DIRECTED_CALLOUT() DRAUGHTING_CALLOUT((#636)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LINEAR_DIMENSION() REPRESENTATION_ITEM(' '));",
  // 引出し線（矢印と文字）、バルーン、引出し線のない表記
  "#800=CARTESIAN_POINT(' ',(250.000000,300.000000));",
  "#801=CARTESIAN_POINT(' ',(270.000000,320.000000));",
  "#802=CARTESIAN_POINT(' ',(290.000000,320.000000));",
  "#803=POLYLINE(' ',(#800,#801,#802));",
  curveOccurrence(804, 33, 803, "LEADER_CURVE()"),
  textOccurrence(805, { text: "\\X2\\30E130E2\\X0\\", x: 272, y: 321, anchor: "$$SXF_baseline left", height: 2.5 }),
  terminator(812, "", "94", 250, 300, -1, -1, 804, "LEADER_TERMINATOR"),
  "#820=(DRAUGHTING_CALLOUT((#804,#811,#817)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LEADER_DIRECTED_CALLOUT() REPRESENTATION_ITEM(' '));",
  "#830=CARTESIAN_POINT(' ',(310.000000,300.000000));",
  "#831=CARTESIAN_POINT(' ',(320.000000,310.000000));",
  "#832=POLYLINE(' ',(#830,#831));",
  curveOccurrence(833, 31, 832, "LEADER_CURVE()"),
  "#834=CARTESIAN_POINT(' ',(325.000000,315.000000));",
  "#835=AXIS2_PLACEMENT_2D(' ',#834,$);",
  "#836=CIRCLE(' ',#835,7.000000);",
  curveOccurrence(837, 31, 836),
  "#838=(DRAUGHTING_CALLOUT((#833,#837)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LEADER_DIRECTED_CALLOUT() REPRESENTATION_ITEM(' '));",
  "#839=(DRAUGHTING_CALLOUT((#811)) DRAUGHTING_ELEMENTS() GEOMETRIC_REPRESENTATION_ITEM() LEADER_DIRECTED_CALLOUT() REPRESENTATION_ITEM(' '));",
  // レイヤ（8 ビットの Shift_JIS の名前を含む）と非表示
  "#900=PRESENTATION_LAYER_ASSIGNMENT('$$SXF_dummy_layer_for_subfigure',' ',(#406,#416,#421,#436,#443,#456,#462,#466));",
  "#901=PRESENTATION_LAYER_ASSIGNMENT('\\X2\\7DDA\\X0\\',' ',(#101,#107,#112,#116,#121,#127,#129,#133));",
  "#902=PRESENTATION_LAYER_ASSIGNMENT('ハッチ',' ',(#554,#569,#574,#578,#580,#583));",
  "#903=PRESENTATION_LAYER_ASSIGNMENT('DIM\\X\\B0 \\S\\1 \\PA\\ \\\\',' ',(#606,#674,#726,#756,#804,#833));",
  "#904=PRESENTATION_LAYER_ASSIGNMENT('hidden',' ',(#145,#147));",
  "#905=INVISIBILITY((#904,#507,#99992));",
  // 表題と用紙
  "#950=DRAWING_DEFINITION('01',$);",
  "#951=DRAUGHTING_DRAWING_REVISION('01',#950,$);",
  "#952=DRAUGHTING_TITLE((#951),'JAPANESE',' \\X2\\5E73976256F3\\X0\\ ');",
  "#953=PLANAR_BOX(' ',420.000000,594.000000,#45);",
  "#954=DRAWING_SHEET_REVISION('a2_Vertical',(#101,#107,#112,#116,#121,#127,#129,#133,#136,#139,#145,#147,#152,#155,#157,#158,#206,#216,#226,#236,#246,#256,#266,#276,#286,#296,#305,#315,#325,#335,#345,#355,#365,#375,#385,#386,#388,#406,#416,#421,#436,#443,#456,#462,#466,#554,#569,#574,#578,#580,#583,#662,#661,#710,#711,#740,#780,#781,#782,#820,#838,#839,#2009,#99993,#953),#7,'01');",
  "#955=PRESENTATION_SIZE(#954,#953);",
  // 70 段の入れ子の子図（作図グループ）。64 段より深い定義は読まない
  ...Array.from({ length: 70 }, (_, level) => {
    const id = 2000 + level * 10;
    const inner = level < 69 ? `#${id + 19}` : "#101";
    return [
      `#${id}=DRAUGHTING_SUBFIGURE_REPRESENTATION('$$SXF_G_deep${level}',(${inner},#45),#7);`,
      `#${id + 1}=SYMBOL_REPRESENTATION_MAP(#45,#${id});`,
      `#${id + 5}=(ANNOTATION_SYMBOL() GEOMETRIC_REPRESENTATION_ITEM() MAPPED_ITEM(#${id + 1},#454) REPRESENTATION_ITEM(' '));`,
      `#${id + 9}=(${OCCURRENCE} ANNOTATION_SUBFIGURE_OCCURRENCE() ANNOTATION_SYMBOL_OCCURRENCE() REPRESENTATION_ITEM(' ') STYLED_ITEM((#96),#${id + 5}));`,
    ].join("\n");
  }),
  // 読めないインスタンス（括弧の閉じ忘れ、余計な語、型名なし、入れ子が深すぎる値）
  `#989=CARTESIAN_POINT(' ',${"(".repeat(70)}1.0${")".repeat(70)});`,
  "#990=CARTESIAN_POINT(' ',(1.0,2.0);",
  "#991=CARTESIAN_POINT(' ',(1.0,2.0)) junk;",
  "#992=?;",
].join("\n")}
ENDSEC;
END-ISO-10303-21;
`;

/** 用紙（drawing_sheet_revision）のない P21。出力ライブラリ名に版がない */
export const P21_NO_SHEET = `${P21_HEADER("nosheet.p21", "zumen")}#5=(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.));
#40=CARTESIAN_POINT(' ',(0.000000,0.000000));
#45=AXIS2_PLACEMENT_2D(' ',#40,$);
#46=CIRCLE(' ',#45,5.000000);
ENDSEC;
END-ISO-10303-21;
`;

/** SFC は CP932、P21 は ASCII（8 ビットの Shift_JIS の名前を 1 つ含む）のバイト列にする */
export function sfcBytes(text: string): Uint8Array {
  return cp932(text);
}

export function p21Bytes(text: string): Uint8Array {
  return cp932(text);
}
