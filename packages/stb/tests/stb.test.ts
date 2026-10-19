import { describe, expect, test } from "vitest";
import { membersOnAxis, readStb } from "../src/index.js";
import type { StructureMember, StructureModel, Vec3 } from "../src/types.js";

/**
 * テストの ST-Bridge は、仕様の規則を確かめる最小のものを自作する（公開の見本は Git に入れない）。
 * 確かめるのは、表示が崩れて見えないまま誤る点だけ: 文字コード、部材の基準点と断面の向き、版による要素名の違いと
 * 変断面の位置、材料の決め方、描かない要素の件数。
 */
const encoder = new TextEncoder();

function stb(version: string, model: string, common = ""): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<ST_BRIDGE xmlns="https://www.building-smart.or.jp/dl" version="${version}">
  <StbCommon project_name="test" app_name="test" app_version="1" ${common}/>
  <StbModel>${model}</StbModel>
</ST_BRIDGE>`;
}

const read = (text: string) => readStb(encoder.encode(text));

function member(model: StructureModel, key: string): StructureMember {
  const found = model.members.find((item) => item.key === key);
  if (!found) throw new Error(`${key} がない`);
  return found;
}

const round = (p: Vec3) => p.map((v) => Math.round(v * 10) / 10);
const property = (m: StructureMember, label: string) => m.properties.find((item) => item.label === label)?.value;

/** 部材の外接箱の大きさ（X・Y・Z） */
const size = (m: StructureMember) => m.bounds.max.map((v, i) => Math.round(v - (m.bounds.min[i] as number)));

describe("文字コード", () => {
  test("Shift_JIS の符号を読み、宣言と中身が違うファイルは中身の文字コードで読み直して知らせる", () => {
    // 研修・階柱を CP932 で書いた最小のファイル
    const sjis = Uint8Array.from([
      ...encoder.encode(
        '<?xml version="1.0" encoding="Shift_JIS"?><ST_BRIDGE version="2.0.2"><StbCommon project_name="',
      ),
      0x8c,
      0xa4,
      0x8f,
      0x43,
      ...encoder.encode('" app_name="a" app_version="1"/><StbModel><StbNodes>'),
      ...encoder.encode(
        '<StbNode id="1" X="0" Y="0" Z="0" kind="ON_GRID"/><StbNode id="2" X="0" Y="0" Z="3000" kind="ON_GRID"/>',
      ),
      ...encoder.encode('</StbNodes><StbMembers><StbColumns><StbColumn id="1" name="1'),
      0x8a,
      0x4b,
      0x92,
      0x8c,
      ...encoder.encode('" id_node_bottom="1" id_node_top="2" id_section="1" kind_structure="RC"/>'),
      ...encoder.encode("</StbColumns></StbMembers></StbModel></ST_BRIDGE>"),
    ]);
    const model = readStb(sjis);
    expect(model.encoding).toBe("shift_jis");
    expect(model.members[0]?.name).toBe("1階柱");
    expect(model.metadata.find((item) => item.label === "プロジェクト")?.value).toBe("研修");
    expect(model.notices.some((notice) => notice.code === "encoding")).toBe(false);

    // SS7 の出力を UTF-8 で保存し直したものなど、宣言が Shift_JIS のまま中身が UTF-8 のファイル
    const mislabeled = encoder.encode(
      '<?xml version="1.0" encoding="Shift_JIS"?><ST_BRIDGE version="2.0.2"><StbCommon project_name="研修" app_name="a" app_version="1"/><StbModel/></ST_BRIDGE>',
    );
    const relabeled = readStb(mislabeled);
    expect(relabeled.encoding).toBe("utf-8");
    expect(relabeled.metadata.find((item) => item.label === "プロジェクト")?.value).toBe("研修");
    expect(relabeled.notices.find((notice) => notice.code === "encoding")?.level).toBe("warning");
  });

  test("ver.1.x は読めないことを示す", () => {
    expect(() => read('<?xml version="1.0"?><ST_BRIDGE version="1.4.00"><StbCommon/><StbModel/></ST_BRIDGE>')).toThrow(
      /1\.4\.00 は読めません/,
    );
  });
});

describe("部材の配置と断面の向き", () => {
  const nodes = `<StbNodes>
    <StbNode id="1" X="0" Y="0" Z="0" kind="ON_GRID"/>
    <StbNode id="2" X="0" Y="0" Z="4000" kind="ON_GRID"/>
    <StbNode id="3" X="6000" Y="0" Z="4000" kind="ON_GRID"/>
    <StbNode id="4" X="6000" Y="0" Z="0" kind="ON_GRID"/>
  </StbNodes>`;
  const steel = `<StbSecSteel>
    <StbSecRoll-H name="H-400x200x8x13" type="H" A="400" B="200" t1="8" t2="13" r="16"/>
  </StbSecSteel>`;

  test("H 形鋼の柱は成を Y に向け、isReferenceDirection=false と回転で向きを変える", () => {
    const model = read(
      stb(
        "2.1.1",
        `${nodes}
        <StbMembers><StbColumns>
          <StbColumn id="1" id_node_bottom="1" id_node_top="2" id_section="1" kind_structure="S"/>
          <StbColumn id="2" id_node_bottom="4" id_node_top="3" id_section="2" kind_structure="S"/>
          <StbColumn id="3" id_node_bottom="1" id_node_top="2" id_section="1" kind_structure="S" rotate="90" offset_bottom_X="100" offset_top_X="100"/>
        </StbColumns></StbMembers>
        <StbSections>
          <StbSecColumn_S id="1" name="C1"><StbSecSteelFigureColumn_S><StbSecSteelColumn_S_Same shape="H-400x200x8x13" strength_main="SN490B"/></StbSecSteelFigureColumn_S></StbSecColumn_S>
          <StbSecColumn_S id="2" name="C2" isReferenceDirection="false"><StbSecSteelFigureColumn_S><StbSecSteelColumn_S_Same shape="H-400x200x8x13" strength_main="SN490B"/></StbSecSteelFigureColumn_S></StbSecColumn_S>
          ${steel}
        </StbSections>`,
      ),
    );
    // 鉄骨断面の基準方向（成）を部材の Y に向ける（仕様書 6.3 補足(4)）
    expect(size(member(model, "StbColumn#1"))).toEqual([200, 400, 4000]);
    expect(size(member(model, "StbColumn#2"))).toEqual([400, 200, 4000]);
    // 回転は X を 0 度とし、断面を回してからオフセットを足す（5.2.1 補足(2)(3)）
    const rotated = member(model, "StbColumn#3");
    expect(size(rotated)).toEqual([400, 200, 4000]);
    expect(round(rotated.bounds.min)).toEqual([-100, -100, 0]);
    expect(property(rotated, "材料")).toBe("鋼材 SN490B");
  });

  test("梁は天端の幅中心が基準点で、断面は節点から下に付く。RC は 2.0 と 2.1 の要素名を同じように読む", () => {
    const beam = (version: string, figure: string) =>
      read(
        stb(
          version,
          `${nodes}
          <StbMembers><StbGirders>
            <StbGirder id="1" id_node_start="2" id_node_end="3" id_section="1" kind_structure="RC" isFoundation="false" offset_start_Z="-50" offset_end_Z="-50"/>
          </StbGirders></StbMembers>
          <StbSections><StbSecBeam_RC id="1" name="G1">${figure}</StbSecBeam_RC></StbSections>`,
        ),
      );
    for (const model of [
      beam("2.0.2", '<StbSecFigureBeam_RC><StbSecBeam_RC_Straight width="400" depth="700"/></StbSecFigureBeam_RC>'),
      beam(
        "2.1.1",
        '<StbSecFigureBeam_RC order="1"><StbSecBeamStraight width="400" depth="700"/></StbSecFigureBeam_RC>',
      ),
    ]) {
      const girder = member(model, "StbGirder#1");
      expect(round(girder.bounds.min)).toEqual([0, -200, 3250]);
      expect(round(girder.bounds.max)).toEqual([6000, 200, 3950]);
      expect(girder.section?.shapes).toEqual(["400×700"]);
    }
  });
});

describe("変断面と材料", () => {
  const nodes = `<StbNodes>
    <StbNode id="1" X="0" Y="0" Z="0" kind="ON_GRID"/>
    <StbNode id="2" X="8000" Y="0" Z="0" kind="ON_GRID"/>
  </StbNodes>
  <StbStories><StbStory id="1" name="2FL" height="0" kind="GENERAL" strength_concrete="FC30"><StbNodeIdList><StbNodeId id="1"/><StbNodeId id="2"/></StbNodeIdList></StbStory></StbStories>`;

  test("2.1 の区間は切り替え位置で、2.0 のハンチはハンチ位置で断面を変える", () => {
    const v21 = read(
      stb(
        "2.1.1",
        `${nodes}
        <StbMembers><StbGirders>
          <StbGirder id="1" id_node_start="1" id_node_end="2" id_section="1" kind_structure="RC" isFoundation="false">
            <StbGirderConcreteSwitch order="1" distance="1000"/>
            <StbGirderConcreteSwitch order="2" distance="7000"/>
          </StbGirder>
        </StbGirders></StbMembers>
        <StbSections><StbSecBeam_RC id="1" name="G1">
          <StbSecFigureBeam_RC order="2"><StbSecBeamStraight width="400" depth="700"/></StbSecFigureBeam_RC>
          <StbSecFigureBeam_RC order="1"><StbSecBeamTaper start_width="600" start_depth="1000" end_width="400" end_depth="700"/></StbSecFigureBeam_RC>
          <StbSecFigureBeam_RC order="3"><StbSecBeamStraight width="400" depth="900"/></StbSecFigureBeam_RC>
        </StbSecBeam_RC></StbSections>`,
      ),
    );
    const g21 = member(v21, "StbGirder#1");
    expect(g21.section?.shapes).toEqual([
      "0〜1000 mm: 600×1000→400×700",
      "1000〜7000 mm: 400×700",
      "7000〜8000 mm: 400×900",
    ]);
    expect(g21.solids).toHaveLength(3);
    expect(round(g21.bounds.min)).toEqual([0, -300, -1000]);
    // 部材 → 断面 → 階 → 全体の順に決める（3.2 補足(3)）。ここは始端の節点が属する階
    expect(property(g21, "材料")).toBe("コンクリート FC30（階 2FL）");

    const v20 = read(
      stb(
        "2.0.2",
        `${nodes}
        <StbMembers><StbGirders>
          <StbGirder id="1" name="G" id_node_start="1" id_node_end="2" id_section="1" kind_structure="RC" isFoundation="false" haunch_start="1500" kind_haunch_start="DROP" strength_concrete="FC36"/>
        </StbGirders></StbMembers>
        <StbSections><StbSecBeam_RC id="1" name="G1" strength_concrete="FC24"><StbSecFigureBeam_RC>
          <StbSecBeam_RC_Haunch pos="START" width="500" depth="900"/>
          <StbSecBeam_RC_Haunch pos="CENTER" width="400" depth="700"/>
        </StbSecFigureBeam_RC></StbSecBeam_RC></StbSections>`,
        'strength_concrete="FC21"',
      ),
    );
    const g20 = member(v20, "StbGirder#1");
    expect(g20.section?.shapes).toEqual(["0〜1500 mm: 500×900", "1500〜8000 mm: 400×700"]);
    expect(property(g20, "材料")).toBe("コンクリート FC36（部材）");
  });
});

describe("壁・通り芯・描かない要素", () => {
  test("開口は壁の第 1 点から測った位置に穴を開け、描かない要素と不正な参照は件数を知らせる", () => {
    const model = read(
      stb(
        "2.1.1",
        `<StbNodes>
          <StbNode id="1" X="0" Y="0" Z="0" kind="ON_GRID"/>
          <StbNode id="2" X="6000" Y="0" Z="0" kind="ON_GRID"/>
          <StbNode id="3" X="6000" Y="0" Z="3000" kind="ON_GRID"/>
          <StbNode id="4" X="0" Y="0" Z="3000" kind="ON_GRID"/>
        </StbNodes>
        <StbAxes>
          <StbParallelAxes group_name="Y" X="0" Y="0" angle="0"><StbParallelAxis id="1" name="Y1" distance="0"/></StbParallelAxes>
          <StbParallelAxes group_name="X" X="0" Y="0" angle="270"><StbParallelAxis id="2" name="X2" distance="6000"/></StbParallelAxes>
        </StbAxes>
        <StbMembers>
          <StbColumns><StbColumn id="9" id_node_bottom="1" id_node_top="99" id_section="1" kind_structure="RC"/></StbColumns>
          <StbWalls><StbWall id="1" id_section="1" kind_structure="RC" kind_layout="ON_GIRDER"><StbNodeIdOrder>1 2 3 4</StbNodeIdOrder></StbWall></StbWalls>
          <StbIsolatingDevices><StbIsolatingDevice id="1" id_node="1" id_section="1"/><StbIsolatingDevice id="2" id_node="2" id_section="1"/></StbIsolatingDevices>
          <StbOpenArrangements><StbOpenArrangement id="1" id_section="2" kind_member="WALL" id_member="1" position_X="1000" position_Y="500" rotate="0"/></StbOpenArrangements>
        </StbMembers>
        <StbSections>
          <StbSecWall_RC id="1" name="W18"><StbSecFigureWall_RC><StbSecWall_RC_Straight t="180"/></StbSecFigureWall_RC></StbSecWall_RC>
          <StbSecOpen_RC id="2" name="AW" length_X="1800" length_Y="2000"/>
        </StbSections>`,
      ),
    );
    const wall = member(model, "StbWall#1");
    const solid = wall.solids[0];
    if (solid?.type !== "loft") throw new Error("壁は輪郭の列で描く");
    // 厚さは壁の中心面から ±t/2（5.8.1 補足(8)）
    expect(size(wall)).toEqual([6000, 180, 3000]);
    const hole = solid.sections[0]?.holes[0]?.map(round);
    expect(hole?.map(([x, , z]) => [x, z])).toEqual([
      [1000, 500],
      [2800, 500],
      [2800, 2500],
      [1000, 2500],
    ]);
    const notices = new Map(model.notices.map((notice) => [notice.code, notice.count]));
    expect(notices.get("not-drawn:StbIsolatingDevices")).toBe(2);
    expect(notices.get("missing-node")).toBe(1);
    // 通り芯の上の部材（軸組の表示）
    const y1 = model.axes.find((axis) => axis.name === "Y1");
    const x2 = model.axes.find((axis) => axis.name === "X2");
    if (!y1 || !x2) throw new Error("通り芯がない");
    expect([...membersOnAxis(model, y1)]).toEqual(["StbWall#1"]);
    expect([...membersOnAxis(model, x2)]).toEqual([]);
  });
});
