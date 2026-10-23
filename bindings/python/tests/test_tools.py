"""zumen.tools: the functions behind the MCP tools, their documented schema, their caps and regions of the frame."""

import json
import pathlib
import subprocess
import sys
import types
import typing
from concurrent.futures import ThreadPoolExecutor

import pytest
import zumen
from zumen import tools

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"
SFC = str(FIXTURES / "sfc" / "features.sfc")
ROOMS = FIXTURES / "dxf" / "rooms.dxf"
SHEETS = pathlib.Path(__file__).parents[3] / "crates" / "zumen-draft" / "examples" / "sheets.json"


def conforms(value: typing.Any, hint: typing.Any, path: str = "result") -> None:
    """Fail unless value has the shape of the type hint: the keys of a TypedDict (required ones present, no others),
    the items of lists and dicts, literals and plain types."""
    origin = typing.get_origin(hint)
    if hint is typing.Any:
        return
    if typing.is_typeddict(hint):
        assert isinstance(value, dict), f"{path} is not a dict: {value!r}"
        hints = typing.get_type_hints(hint)
        missing = hint.__required_keys__ - value.keys()
        unknown = value.keys() - hints.keys()
        assert not missing and not unknown, f"{path}: missing {missing}, unknown {unknown} for {hint.__name__}"
        for key, item in value.items():
            conforms(item, hints[key], f"{path}.{key}")
        return
    if origin in (typing.Union, types.UnionType):
        for option in typing.get_args(hint):
            try:
                conforms(value, option, path)
                return
            except AssertionError:
                pass
        raise AssertionError(f"{path}: {value!r} is none of {hint}")
    if origin is typing.Literal:
        assert value in typing.get_args(hint), f"{path}: {value!r} is not one of {typing.get_args(hint)}"
        return
    if origin is list:
        assert isinstance(value, list), f"{path} is not a list: {value!r}"
        (item,) = typing.get_args(hint)
        for index, element in enumerate(value):
            conforms(element, item, f"{path}[{index}]")
        return
    if origin is dict:
        assert isinstance(value, dict), f"{path} is not a dict: {value!r}"
        key_hint, item_hint = typing.get_args(hint)
        for key, item in value.items():
            conforms(key, key_hint, path)
            conforms(item, item_hint, f"{path}.{key}")
        return
    if hint is type(None):
        assert value is None, f"{path} is not None: {value!r}"
    elif hint is float:
        assert isinstance(value, int | float) and not isinstance(value, bool), f"{path} is not a number: {value!r}"
    elif hint is int:
        assert isinstance(value, int) and not isinstance(value, bool), f"{path} is not an integer: {value!r}"
    else:
        assert isinstance(value, hint), f"{path} is not {hint}: {value!r}"


def check(result: typing.Any, hint: typing.Any) -> None:
    """The result has the documented shape, and JSON keeps it as it is (no tuples, no NaN)."""
    conforms(result, hint)
    assert json.loads(json.dumps(result, ensure_ascii=False, allow_nan=False)) == result


def revised_dxf(tmp_path: pathlib.Path) -> pathlib.Path:
    """features.dxf with one text changed."""
    revised = tmp_path / "features-rev2.dxf"
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes().replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    revised.write_bytes(data)
    return revised


def test_every_result_follows_the_documented_schema(tmp_path):
    for path in [
        SFC,
        FIXTURES / "p21" / "features.p21",
        FIXTURES / "dxf" / "features.dxf",
        FIXTURES / "dxf" / "blocks.dxf",
        FIXTURES / "dwg" / "r2018.dwg",
        FIXTURES / "jww" / "minimal-v700.jww",
    ]:
        scene = zumen.read(path)
        check(tools.summary(scene), tools.Summary)
        png, info = tools.render(scene, size=400)
        assert png.startswith(b"\x89PNG")
        check(info, tools.RenderInfo)
        check(tools.find_texts(scene, all_layers=True), tools.TextPage)
        page = tools.find_entities(scene, all_layers=True, limit=1000, geometry=True, max_points=tools.MAX_POINTS)
        check(page, tools.EntityPage)
        for entity in scene.entities(hidden_layers=[]):
            check(tools.get_entity(scene, entity["id"], max_points=tools.MAX_POINTS), tools.EntityDetail)
        check(tools.find_regions(scene, all_layers=True, max_points=tools.MAX_POINTS), tools.RegionPage)
    check(tools.find_regions(zumen.read(ROOMS)), tools.RegionPage)
    summary_sheet = zumen.read(zumen.draft(SHEETS, "dxf", sheet="A01-1").data)
    check(tools.find_tables(summary_sheet), tools.TablePage)
    check(tools.find_tables(summary_sheet, max_cells=10), tools.TablePage)
    scene = zumen.read(SFC)
    line = scene.entity("#810>#260>#220")
    x0, y0, x1, y1 = line["bbox"]
    check(tools.measure(scene, [x0, y0], [x1, y1]), tools.MeasurementInfo)
    check(tools.measure(scene, [1, 1], [2, 1]), tools.MeasurementInfo)
    before, after = zumen.read(FIXTURES / "dxf" / "features.dxf"), zumen.read(revised_dxf(tmp_path))
    _, comparison = tools.compare_revisions(before, after)
    check(comparison, tools.ComparisonInfo)


def test_the_schema_converts_to_json_schema():
    pydantic = pytest.importorskip("pydantic")
    for hint in [
        tools.Summary,
        tools.RenderInfo,
        tools.TextPage,
        tools.EntityPage,
        tools.EntityDetail,
        tools.RegionPage,
    ]:
        assert pydantic.TypeAdapter(hint).json_schema()["type"] == "object"
    detail = tools.get_entity(zumen.read(SFC), "#810>#290")
    assert pydantic.TypeAdapter(tools.EntityDetail).validate_python(detail) == detail


def test_summarizes_a_drawing():
    info = tools.summary(zumen.read(SFC))
    assert (info["format"], info["version"], info["unit"], info["views"]) == ("sfc", "SXF 3.1", "millimeter", None)
    assert info["sheet"] == {"name": "平面図", "size": "A3 縦", "width": 297.0, "height": 420.0}
    assert info["bounds"] == [0.0, 0.0, 297.0, 420.0]  # the frame: the sheet and every entity
    assert info["entities"] == sum(info["types"].values()) == sum(info["kinds"].values())
    assert info["types"]["文字"] >= 1 and info["kinds"]["text"] >= 1
    layer = next(layer for layer in info["layers"] if layer["name"] == "寸法")
    assert layer["entities"] > 0 and info["layers_total"] == len(info["layers"])
    assert (info["title_block"]["number"], info["title_block"]["title"]) == ("No.1", "平面図")
    assert "fields" not in info["title_block"]
    assert info["metadata_values"]["format"] == info["metadata"][0]["value"]
    assert tools.summary(zumen.read(SFC), layer_limit=2)["layers"] == info["layers"][:2]


def test_lists_the_placements_drawn_at_a_scale():
    info = tools.summary(zumen.read(SFC))
    assert info["placements_total"] == len(info["placements"]) > 1
    counts = [placement["entities"] for placement in info["placements"]]
    assert counts == sorted(counts, reverse=True)
    part = next(placement for placement in info["placements"] if placement["scale"] == "1:20")
    assert part["name"] == "部分図（数学座標系） 部分図"
    assert part["to_real"] == [20.0, 0.0, 0.0, 20.0, -400.0, -2000.0]
    x0, y0, x1, y1 = part["bbox"]
    assert x0 < x1 and y0 < y1
    # A drawing whose entities sit on the sheet itself has no placement
    assert tools.summary(zumen.read(FIXTURES / "sfc" / "nosheet.sfc"))["placements"] == []


def test_a_drawing_without_entities_has_no_frame():
    empty = zumen.read(b"0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n")
    info = tools.summary(empty)
    check(info, tools.Summary)
    assert (info["bounds"], info["entities"], tools.frame(empty)) == (None, 0, None)
    with pytest.raises(tools.DrawingError, match="nothing"):
        tools.render(empty)
    with pytest.raises(tools.DrawingError, match="no frame"):
        tools.find_texts(empty, frame_region=[0, 0, 1, 1])
    _, image = tools.render(empty, region=[0, 0, 10, 10], size=100)
    assert (image["width"], image["frame_region"]) == (100, None)


def test_converts_between_paper_millimeters_and_fractions_of_the_frame():
    scene = zumen.read(SFC)  # frame [0, 0, 297, 420]
    assert tools.frame(scene) == [0.0, 0.0, 297.0, 420.0]
    assert tools.paper_region(scene, [0, 0, 1, 1]) == [0.0, 0.0, 297.0, 420.0]
    # The top right quarter: fractions start at the top left, paper millimeters at the bottom left
    assert tools.paper_region(scene, [0.5, 0, 1, 0.5]) == [148.5, 210.0, 297.0, 420.0]
    assert tools.paper_region(scene, [1, 0.5, 0.5, 0]) == [148.5, 210.0, 297.0, 420.0]
    assert tools.frame_region_of(scene, [148.5, 210.0, 297.0, 420.0]) == [0.5, 0.0, 1.0, 0.5]
    fractions = tools.frame_region_of(scene, [10, 220, 25, 225])
    assert tools.paper_region(scene, fractions) == pytest.approx([10, 220, 25, 225], abs=1e-3)
    with pytest.raises(tools.DrawingError):
        tools.paper_region(scene, [0, 0, 1])
    with pytest.raises(tools.DrawingError, match="not both"):
        tools.find_texts(scene, region=[0, 0, 1, 1], frame_region=[0, 0, 1, 1])


def test_takes_regions_as_fractions_of_the_frame():
    scene = zumen.read(SFC)
    fractions = tools.frame_region_of(scene, [0, 215, 30, 230])
    assert [text["id"] for text in tools.find_texts(scene, frame_region=fractions)["texts"]] == ["#700"]
    assert tools.find_entities(scene, layer="寸法", frame_region=fractions)["entities"][0]["id"] == "#700"
    _, part = tools.render(scene, frame_region=fractions, size=300)
    assert (part["width"], part["texts_drawn"]) == (300, 1)
    assert part["frame_region"] == pytest.approx(fractions, abs=1e-3)
    _, whole = tools.render(scene, size=420)
    assert whole["region"] == pytest.approx([0, 0, 297, 420], abs=1)
    assert whole["frame_region"] == pytest.approx([0, 0, 1, 1], abs=1e-2)


def test_finds_texts_ignoring_width_case_and_spaces():
    scene = zumen.read(SFC)
    assert tools.normalize("ｶﾀｶﾅ ＡＢＣ") == tools.normalize("カタカナabc") == "カタカナabc"
    for query in ["ｶﾀｶﾅ", "カタカナ", "abc", "平 面"]:
        assert tools.find_texts(scene, query=query)["total"] == 1, query
    page = tools.find_texts(scene, limit=2, offset=1)
    assert page["total"] == len(scene.texts()) and len(page["texts"]) == 2
    assert page["texts"][0]["id"] == scene.texts()[1]["id"] and page["next_offset"] == 3
    assert tools.find_texts(scene, limit=1000)["next_offset"] is None
    with pytest.raises(tools.DrawingError):
        tools.find_texts(scene, limit=0)


def test_cuts_long_texts_and_says_so():
    scene = zumen.read(SFC)
    [text] = tools.find_texts(scene, query="平面図", max_text_length=2)["texts"]
    assert (text["text"], text["truncated"]) == ("平…", True)
    assert "truncated" not in tools.find_texts(scene, query="平面図")["texts"][0]
    entity = tools.get_entity(scene, "#700", max_text_length=2)
    assert (entity["text"], entity["values"]["text"], entity["truncated"]) == ("平…", "平…", True)
    assert {"key": "text", "label": "文字列", "value": "平…"} in entity["properties"]
    [shape] = entity["geometry"]
    assert (shape["text"], shape["truncated"]) == ("平…", True)
    assert "truncated" not in tools.get_entity(scene, "#700")
    with pytest.raises(tools.DrawingError):
        tools.find_texts(scene, max_text_length=0)


def test_finds_entities_by_kind_type_and_layer():
    scene = zumen.read(SFC)
    found = tools.find_entities(scene, type="寸法")
    assert found["total"] == sum(found["types"].values()) == sum(found["kinds"].values()) > 0
    assert all("寸法" in entity["type"] for entity in found["entities"])
    assert tools.find_entities(scene, layer="寸法", region=(0, 215, 30, 230))["entities"][0]["id"] == "#700"
    entity = tools.get_entity(scene, "#700")
    assert {"key": "text", "label": "文字列", "value": "平面図"} in entity["properties"] and entity["placement"] is None
    assert (entity["kind"], entity["values"]["position"]) == ("text", [10.0, 220.0])
    dimensions = tools.find_entities(scene, kind="dimension")
    assert dimensions["total"] == found["total"]
    assert {entity["kind"] for entity in dimensions["entities"]} == {"dimension"}
    with pytest.raises(tools.DrawingError):
        tools.get_entity(scene, "no such id")


def test_gives_the_geometry_of_an_entity_in_paper_and_real_millimeters():
    scene = zumen.read(SFC)
    line = tools.get_entity(scene, "#600")
    assert line["geometry"] == [{"shape": "polyline", "points": [[10.0, 250.0], [100.0, 250.0]], "closed": False}]
    dimension = tools.get_entity(scene, "#810>#290")
    assert dimension["placement"]["scale"] == "1:20" and len(dimension["placement"]["to_real"]) == 6
    first, *_, text = dimension["geometry"]
    assert first == {
        "shape": "polyline",
        "points": [[20.0, 110.0], [70.0, 110.0]],
        "closed": False,
        "real": [[0.0, 200.0], [1000.0, 200.0]],
    }
    assert text == {
        "shape": "text",
        "text": "1,000",
        "point": [42.5, 110.5],
        "height": 0.125,
        "angle": 0.0,
        "real": [450.0, 210.0],
    }
    # Shapes past max_points are left out whole; a point with real counts twice
    short = tools.get_entity(scene, "#810>#290", max_points=8)
    assert [shape["shape"] for shape in short["geometry"]] == ["polyline", "polyline"]
    assert short["geometry_omitted"] == 4
    with pytest.raises(tools.DrawingError):
        tools.get_entity(scene, "#600", max_points=0)
    # In DXF model space the real-world coordinates are those on the sheet, so they are not repeated
    point = tools.get_entity(zumen.read(FIXTURES / "dxf" / "features.dxf"), "107")
    assert point["geometry"] == [{"shape": "point", "point": [110.0, 5.0]}]


def test_pages_through_the_geometry_of_entities_within_a_number_of_points():
    scene = zumen.read(SFC)
    plain = tools.find_entities(scene, limit=1000)
    assert "geometry" not in plain["entities"][0] and plain["next_offset"] is None
    whole = tools.find_entities(scene, geometry=True, limit=1000, max_points=tools.MAX_POINTS)
    assert [entity["id"] for entity in whole["entities"]] == [entity["id"] for entity in plain["entities"]]
    assert all("geometry" in entity and "placement" in entity for entity in whole["entities"])
    dimension = next(entity for entity in whole["entities"] if entity["id"] == "#810>#290")
    assert dimension["placement"] == {"name": "部分図（数学座標系） 部分図", "scale": "1:20"}
    assert dimension["geometry"] == tools.get_entity(scene, "#810>#290")["geometry"]

    # Small pages end before the points pass the budget, and next_offset goes on from there
    ids: list[str] = []
    offset: int | None = 0
    while offset is not None:
        page = tools.find_entities(scene, geometry=True, offset=offset, max_points=40)
        assert page["entities"], offset
        ids += [entity["id"] for entity in page["entities"]]
        offset = page["next_offset"]
    assert ids == [entity["id"] for entity in plain["entities"]]
    # An entity larger than the budget still comes first on its page, with the shapes that fit
    big = tools.find_entities(scene, geometry=True, region=(165, 240, 175, 250), type="円", max_points=4)
    assert big["entities"][0]["geometry"] == [] and big["entities"][0]["geometry_omitted"] == 1


def test_measures_real_lengths_through_the_scale_of_a_placement():
    scene = zumen.read(SFC)
    # A line in a part drawn at 1:10, where 1 mm on the sheet is 10 mm in reality
    entity = scene.entity("#810>#260>#220")
    assert entity is not None and entity["placement"]["scale_x"] == pytest.approx(0.1)
    x0, y0, x1, y1 = entity["bbox"]
    measured = tools.measure(scene, [x0, y0], [x1, y1])
    assert measured["real"] == pytest.approx(measured["paper"] * 10, abs=1e-3)
    assert measured["placement"]["scale"] == "1:10"
    assert tools.measure(scene, [x0, y0], [x1, y1], entity="#810>#260>#220")["real"] == measured["real"]
    # Without a scaled entity near the points, the length is on the sheet only
    alone = tools.measure(scene, [1, 1], [2, 1])
    assert (alone["paper"], alone["real"]) == (1.0, None)
    with pytest.raises(tools.DrawingError):
        tools.measure(scene, [0, 0], [1], entity=None)
    with pytest.raises(tools.DrawingError):
        tools.measure(scene, [0, 0], ["1", 2])


def test_finds_the_rooms_of_a_floor_plan_with_their_real_areas():
    scene = zumen.read(ROOMS)
    page = tools.find_regions(scene, layer="wall")
    assert page["total"] == 5 and page["next_offset"] is None
    rooms = tools.find_regions(scene, layer="WALL", named=True)["regions"]
    assert [[text["text"] for text in room["texts"]] for room in rooms] == [["LDK"], ["寝室"]]
    living, bedroom = rooms
    # 4.85 m × 5.8 m, less the column of radius 0.2 m drawn as a polygon
    assert living["real_outline_area_m2"] == 28.13
    assert 28.0 < living["real_area_m2"] < 28.01
    assert len(living["holes"]) == 1 and living["boundary"] == ["21", "22", "25"]
    assert (bedroom["real_area_m2"], bedroom["real_perimeter"]) == (16.53, 17300.0)
    assert bedroom["outline"] == [[5050.0, 100.0], [7900.0, 100.0], [7900.0, 5900.0], [5050.0, 5900.0]]
    # Model space in millimeters has the same coordinates on the sheet and in the real world
    assert "real_outline" not in bedroom and bedroom["placement"] == {"name": "モデル空間（mm）", "scale": "1:1"}
    # The furniture on another layer is a hole of the bedroom when every layer is used
    every = tools.find_regions(scene, named=True)["regions"]
    assert every[1]["real_area_m2"] == 15.53
    assert tools.find_regions(scene, min_area_m2=20)["total"] == 1
    # A page ends before its points pass max_points, and the first region comes without them when they do not fit
    small = tools.find_regions(scene, layer="wall", max_points=10)
    assert small["regions"][0]["outline"] and small["next_offset"] == 1
    first = tools.find_regions(scene, layer="wall", offset=1, max_points=10)
    assert first["regions"][0]["outline_omitted"] and "outline" not in first["regions"][0]
    with pytest.raises(tools.DrawingError):
        tools.find_regions(scene, tolerance=0)


def test_reads_the_tables_of_a_drafted_sheet_as_rows_and_columns():
    scene = zumen.read(zumen.draft(SHEETS, "dxf", sheet="A01-1").data)
    page = tools.find_tables(scene)
    assert page["total"] == 3 and page["next_offset"] is None
    title, summary, finish = page["tables"]
    assert (title["row_count"], title["column_count"]) == (6, 1)
    assert title["grid"][5] == ["図面番号\nA01-1"]
    assert summary["grid"][8] == ["建築面積", "", "616.12 ㎡", "0.00 ㎡", "616.12 ㎡"]
    assert summary["columns"] == [25.0, 55.0, 115.0, 145.0, 175.0, 205.0]
    heading = summary["cells"][0]
    assert (heading["row"], heading["column"], heading["column_span"]) == (0, 0, 5) and "row_span" not in heading
    assert scene.entity(heading["ids"][0])["kind"] == "text"
    assert finish["grid"][1] == ["記号", "部位", "材料・規格・寸法"]
    # contains finds a table by a text in one of its cells
    found = tools.find_tables(scene, contains="外部仕上")
    assert found["total"] == 1 and found["tables"][0]["grid"][0][0] == "■外部仕上表"
    # A page ends before its cells pass max_cells, and the first table comes with the rows that fit
    small = tools.find_tables(scene, offset=1, max_cells=20)
    assert len(small["tables"]) == 1 and len(small["tables"][0]["grid"]) == 4
    assert small["tables"][0]["rows_omitted"] == 9 and small["next_offset"] == 2
    assert all(cell["row"] < 4 for cell in small["tables"][0]["cells"])
    clipped = tools.find_tables(scene, contains="備考", max_text_length=5)["tables"][0]
    assert clipped["truncated"] and clipped["grid"][12][1] == "この表は…"
    # The Scene method gives every cell with its box
    tables = scene.tables()
    assert [len(table["cells"]) for table in tables] == [6, 31, 19]
    with pytest.raises(tools.DrawingError):
        tools.find_tables(scene, max_cells=0)


def test_gives_real_areas_through_the_scale_of_a_viewport():
    scene = zumen.read(FIXTURES / "dxf" / "viewports.dxf")
    region = next(item for item in tools.find_regions(scene)["regions"] if item["placement"]["scale"] == "1:500")
    assert region["real_area_m2"] == round(region["paper_area"] * 0.25, 4)
    assert region["real_outline"] != region["outline"]


def test_renders_a_region_with_its_long_edge_in_pixels():
    scene = zumen.read(SFC)
    png, info = tools.render(scene, size=800)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert max(info["width"], info["height"]) == 800
    text = next(text for text in info["texts"] if text["id"] == "#700")
    left, top, right, bottom = text["box"]
    x0, _, _, y1 = info["region"]
    assert left == pytest.approx((10.0 - x0) * info["scale"], abs=0.1)
    assert top == pytest.approx((y1 - 225.0) * info["scale"], abs=0.1)
    assert right > left and bottom > top
    _, part = tools.render(scene, region=[0, 215, 30, 230], size=300)
    assert (part["width"], part["texts_drawn"]) == (300, 1)
    assert tools.render(scene, size=800, text_limit=1)[1]["texts"] == info["texts"][:1]
    with pytest.raises(tools.DrawingError):
        tools.render(scene, size=10_000)


def test_reads_only_from_its_roots_and_up_to_its_size_limit():
    drawings = tools.Drawings([FIXTURES / "sfc"])
    assert drawings.scene(SFC) is drawings.scene(SFC)  # read once while the file is unchanged
    with pytest.raises(tools.DrawingError, match="outside"):
        drawings.scene(str(FIXTURES / "jww" / "minimal-v700.jww"))
    with pytest.raises(tools.DrawingError, match="limit"):
        tools.Drawings(max_bytes=10).scene(SFC)
    with pytest.raises(tools.DrawingError, match="cannot read"):
        tools.Drawings().scene(__file__)
    with pytest.raises(tools.DrawingError, match="entities limit"):
        tools.Drawings(max_entities=5).scene(SFC)


def test_rejects_symlinks_outside_its_roots(tmp_path):
    link = tmp_path / "link.sfc"
    try:
        link.symlink_to(SFC)
    except OSError as error:
        if getattr(error, "winerror", None) == 1314:
            pytest.skip("Windows symlinks require Developer Mode or the create-symlink privilege")
        raise
    with pytest.raises(tools.DrawingError, match="outside"):
        tools.Drawings([tmp_path]).scene(str(link))


@pytest.mark.skipif(sys.platform != "win32", reason="Windows directory junctions")
def test_rejects_junctions_outside_its_roots(tmp_path):
    # Junctions do not require Developer Mode. Exercise real path resolution on ordinary Windows accounts too.
    link = tmp_path / "図面 link"
    subprocess.run(["cmd", "/c", "mklink", "/J", str(link), str(FIXTURES / "sfc")], check=True, capture_output=True)
    try:
        with pytest.raises(tools.DrawingError, match="outside"):
            tools.Drawings([tmp_path]).scene(str(link / "features.sfc"))
    finally:
        link.rmdir()  # Remove the junction itself, never its target.


def test_reads_and_answers_from_threads():
    drawings = tools.Drawings([FIXTURES], cache_size=2)
    paths = [SFC, str(FIXTURES / "dxf" / "features.dxf"), str(FIXTURES / "jww" / "minimal-v700.jww")] * 8

    def call(path: str) -> int:
        return tools.find_entities(drawings.scene(path), limit=1000, geometry=True)["total"]

    with ThreadPoolExecutor(max_workers=8) as pool:
        totals = list(pool.map(call, paths))
    assert totals == [call(path) for path in paths]
    assert len(drawings._scenes) <= 2


def test_compares_revisions_with_an_image_of_the_changes(tmp_path):
    before, after = zumen.read(FIXTURES / "dxf" / "features.dxf"), zumen.read(revised_dxf(tmp_path))
    png, info = tools.compare_revisions(before, after, size=600)
    assert png.startswith(b"\x89PNG") and max(info["image"]["width"], info["image"]["height"]) == 600
    assert (info["removed"], info["added"], info["changed"], info["changes_total"]) == (0, 0, 1, 1)
    [change] = info["changes"]
    assert (change["change"], change["before_text"], change["after_text"]) == ("changed", "揃え00", "揃え99")
    assert len(info["regions"]) == 1 and info["regions"][0]["changed"] == 1
    _, cut = tools.compare_revisions(before, after, size=600, max_text_length=2)
    assert (cut["changes"][0]["before_text"], cut["changes"][0]["truncated"]) == ("揃…", True)
