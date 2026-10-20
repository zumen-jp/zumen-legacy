"""zumen.tools: the results follow the documented schema, are JSON, and stay within their caps."""

import json
import pathlib
import types
import typing

import pytest
import zumen
from zumen import tools

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"
SFC = FIXTURES / "sfc" / "features.sfc"


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
    """The result has the documented shape and JSON keeps it unchanged (no tuples, no NaN)."""
    conforms(result, hint)
    assert json.loads(json.dumps(result, ensure_ascii=False, allow_nan=False)) == result


def test_every_result_follows_the_documented_schema(tmp_path):
    scene = zumen.read(SFC)
    check(tools.summary(scene), tools.Summary)
    png, info = tools.render(scene, size=400)
    assert png.startswith(b"\x89PNG")
    check(info, tools.RenderInfo)
    check(tools.find_texts(scene), tools.TextPage)
    check(tools.find_entities(scene, all_layers=True, geometry=True, max_points=10_000), tools.EntityPage)
    for entity in scene.entities(hidden_layers=[]):
        check(tools.get_entity(scene, entity["id"]), tools.EntityDetail)
    line = scene.entity("#810>#260>#220")
    x0, y0, x1, y1 = line["bbox"]
    check(tools.measure(scene, [x0, y0], [x1, y1]), tools.MeasurementInfo)
    check(tools.measure(scene, [1, 1], [2, 1]), tools.MeasurementInfo)

    for path in [
        FIXTURES / "dxf" / "features.dxf",
        FIXTURES / "dxf" / "blocks.dxf",
        FIXTURES / "jww" / "minimal-v700.jww",
    ]:
        other = zumen.read(path)
        check(tools.summary(other), tools.Summary)
        check(tools.find_entities(other, all_layers=True, geometry=True, limit=1000, max_points=0), tools.EntityPage)
        for entity in other.entities(hidden_layers=[]):
            check(tools.get_entity(other, entity["id"], max_points=200), tools.EntityDetail)

    revised = tmp_path / "features-rev2.dxf"
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes().replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    revised.write_bytes(data)
    _, comparison = tools.compare_revisions(zumen.read(FIXTURES / "dxf" / "features.dxf"), zumen.read(revised))
    check(comparison, tools.ComparisonInfo)


def test_the_schema_converts_to_json_schema():
    pydantic = pytest.importorskip("pydantic")
    scene = zumen.read(SFC)
    for hint in [tools.Summary, tools.RenderInfo, tools.TextPage, tools.EntityPage, tools.EntityDetail]:
        assert pydantic.TypeAdapter(hint).json_schema()["type"] == "object"
    detail = tools.get_entity(scene, "#640")
    assert pydantic.TypeAdapter(tools.EntityDetail).validate_python(detail) == detail


def test_summarizes_a_drawing():
    info = tools.summary(zumen.read(SFC))
    assert (info["format"], info["version"], info["views"]) == ("sfc", "SXF 3.1", None)
    assert info["sheet"] == {"name": "平面図", "size": "A3 縦", "width": 297.0, "height": 420.0}
    assert info["bounds"] == [0.0, 0.0, 297.0, 420.0]  # the frame: the sheet and every entity
    assert info["entities"] == sum(info["types"].values()) == sum(info["kinds"].values())
    assert info["types"]["文字"] >= 1 and info["kinds"]["text"] >= 1
    layer = next(layer for layer in info["layers"] if layer["name"] == "寸法")
    assert layer["entities"] > 0 and info["layers_total"] == len(info["layers"])
    assert (info["title_block"]["number"], info["title_block"]["title"]) == ("No.1", "平面図")
    assert info["metadata_values"]["format"] == info["metadata"][0]["value"]
    assert tools.summary(zumen.read(SFC), layer_limit=2)["layers"] == info["layers"][:2]


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
    assert whole["region"] == pytest.approx([0, 0, 297, 420], abs=1) and whole["frame_region"] == pytest.approx(
        [0, 0, 1, 1], abs=1e-2
    )


def test_finds_texts_ignoring_width_case_and_spaces():
    scene = zumen.read(SFC)
    assert tools.normalize("ｶﾀｶﾅ ＡＢＣ") == tools.normalize("カタカナabc") == "カタカナabc"
    for query in ["ｶﾀｶﾅ", "カタカナ", "abc", "平 面"]:
        assert tools.find_texts(scene, query=query)["total"] == 1, query
    page = tools.find_texts(scene, limit=2, offset=1)
    assert page["total"] == len(scene.texts()) and len(page["texts"]) == 2
    assert page["texts"][0]["id"] == scene.texts()[1]["id"]
    with pytest.raises(tools.DrawingError):
        tools.find_texts(scene, limit=0)
    assert len(tools.find_texts(scene, limit=10_000)["texts"]) <= tools.MAX_LIMIT


def test_cuts_long_texts_and_says_so():
    scene = zumen.read(SFC)
    [text] = tools.find_texts(scene, query="平面図", max_text_length=2)["texts"]
    assert (text["text"], text["truncated"]) == ("平…", True)
    assert "truncated" not in tools.find_texts(scene, query="平面図")["texts"][0]
    entity = tools.get_entity(scene, "#700", max_text_length=2)
    assert (entity["text"], entity["values"]["text"], entity["truncated"]) == ("平…", "平…", True)
    assert {"key": "text", "label": "文字列", "value": "平…"} in entity["properties"]
    assert "truncated" not in tools.get_entity(scene, "#700")


def test_finds_entities_by_kind_type_and_layer():
    scene = zumen.read(SFC)
    found = tools.find_entities(scene, type="寸法")
    assert found["total"] == sum(found["types"].values()) == sum(found["kinds"].values()) > 0
    assert all("寸法" in entity["type"] for entity in found["entities"])
    assert tools.find_entities(scene, layer="寸法", region=(0, 215, 30, 230))["entities"][0]["id"] == "#700"
    dimensions = tools.find_entities(scene, kind="dimension")
    assert dimensions["total"] == found["total"]
    assert {entity["kind"] for entity in dimensions["entities"]} == {"dimension"}
    assert "geometry" not in dimensions["entities"][0]


def test_returns_geometry_as_drawn_within_a_budget_of_points():
    scene = zumen.read(SFC)
    polyline = tools.get_entity(scene, "#640")
    [stroke] = polyline["geometry"]["primitives"]
    assert stroke == {
        "kind": "stroke",
        "points": [[110.0, 240.0], [130.0, 250.0], [150.0, 240.0]],
        "closed": False,
        "color": "#ff00ff",
        "width": 0.5,
        "dashes": [12.0, 1.5, 0.25, 1.5],
    }
    assert (polyline["geometry"]["points"], polyline["geometry"]["truncated"]) == (3, False)
    cut = tools.get_entity(scene, "#640", max_points=2)["geometry"]
    assert (cut["primitives"][0]["points"], cut["points"], cut["truncated"]) == (
        [[110.0, 240.0], [130.0, 250.0]],
        3,
        True,
    )
    assert tools.get_entity(scene, "#640", max_points=0)["geometry"] == {
        "primitives": [],
        "points": 3,
        "truncated": True,
    }

    # A text run: the bottom left of its box, its height, length and direction on the sheet
    [run] = tools.get_entity(scene, "#700")["geometry"]["primitives"]
    assert run == {
        "kind": "text",
        "text": "平面図",
        "position": [10.0, 220.0],
        "height": 5.0,
        "width": 15.0,
        "angle": 0.0,
        "color": "#000000",
    }
    assert tools.get_entity(scene, "#710")["geometry"]["primitives"][0]["angle"] == pytest.approx(30.0)

    # The points of a page are shared by its entities; the first ones get theirs
    page = tools.find_entities(scene, all_layers=True, geometry=True, limit=1000, max_points=20)
    used = [entity["geometry"] for entity in page["entities"]]
    kept = sum(
        len(primitive.get("points", [])) if primitive["kind"] == "stroke" else 1
        for geometry in used
        for primitive in geometry["primitives"]
        if primitive["kind"] in ("stroke", "dot", "text")
    )
    assert kept <= 20 and any(geometry["truncated"] for geometry in used)
    assert all(geometry["primitives"] == [] for geometry in used[-3:])
    with pytest.raises(tools.DrawingError):
        tools.get_entity(scene, "#640", max_points=-1)


def test_shows_one_entity_with_its_placement():
    scene = zumen.read(SFC)
    entity = tools.get_entity(scene, "#700")
    assert {"key": "text", "label": "文字列", "value": "平面図"} in entity["properties"] and entity["placement"] is None
    assert (entity["kind"], entity["values"]["position"]) == ("text", [10.0, 220.0])
    # A line in a part drawn at 1:10, whose inverse gives real-world millimeters
    line = tools.get_entity(scene, "#810>#260>#220")
    placement = line["placement"]
    assert (placement["scale"], placement["scale_x"]) == ("1:10", pytest.approx(0.1))
    a, b, c, d, e, f = placement["inverse"]
    [stroke] = line["geometry"]["primitives"]
    (x0, y0), (x1, y1) = stroke["points"]
    real = [(a * x + c * y + e, b * x + d * y + f) for x, y in [(x0, y0), (x1, y1)]]
    assert real == [pytest.approx(line["values"]["start"], abs=0.02), pytest.approx(line["values"]["end"], abs=0.02)]
    with pytest.raises(tools.DrawingError):
        tools.get_entity(scene, "no such id")


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
        tools.measure(scene, [0, 0], [1])
    with pytest.raises(tools.DrawingError):
        tools.measure(scene, [0, 0], ["1", 2])


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


def test_reads_only_from_its_roots_and_up_to_its_size_limit(tmp_path):
    drawings = tools.Drawings([FIXTURES / "sfc"])
    assert drawings.scene(SFC) is drawings.scene(str(SFC))  # read once while the file is unchanged
    with pytest.raises(tools.DrawingError, match="outside"):
        drawings.scene(str(FIXTURES / "jww" / "minimal-v700.jww"))
    link = tmp_path / "link.sfc"
    link.symlink_to(SFC)
    with pytest.raises(tools.DrawingError, match="outside"):
        tools.Drawings([tmp_path]).scene(str(link))  # the real path is outside the root
    with pytest.raises(tools.DrawingError, match="limit"):
        tools.Drawings(max_bytes=10).scene(SFC)
    with pytest.raises(tools.DrawingError, match="cannot read"):
        tools.Drawings().scene(__file__)
    with pytest.raises(tools.DrawingError, match="entities limit"):
        tools.Drawings(max_entities=5).scene(SFC)


def test_compares_revisions_with_an_image_of_the_changes(tmp_path):
    revised = tmp_path / "features-rev2.dxf"
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes().replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    revised.write_bytes(data)
    before, after = zumen.read(FIXTURES / "dxf" / "features.dxf"), zumen.read(revised)
    png, info = tools.compare_revisions(before, after, size=600)
    assert png.startswith(b"\x89PNG") and max(info["image"]["width"], info["image"]["height"]) == 600
    assert (info["removed"], info["added"], info["changed"], info["changes_total"]) == (0, 0, 1, 1)
    [change] = info["changes"]
    assert (change["change"], change["before_text"], change["after_text"]) == ("changed", "揃え00", "揃え99")
    assert len(info["regions"]) == 1 and info["regions"][0]["changed"] == 1
    _, cut = tools.compare_revisions(before, after, size=600, max_text_length=2)
    assert (cut["changes"][0]["before_text"], cut["changes"][0]["truncated"]) == ("揃…", True)
