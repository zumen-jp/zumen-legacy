"""The MCP server: the tool functions on the fixtures, and the tools through an in-memory MCP client."""

import base64
import json
import pathlib

import pytest
import zumen
from zumen import mcp

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"
SFC = str(FIXTURES / "sfc" / "features.sfc")


def test_summarizes_a_drawing():
    info = mcp.summary(zumen.read(SFC))
    assert (info["format"], info["version"], info["views"]) == ("sfc", "SXF 3.1", None)
    assert info["sheet"] == {"name": "平面図", "size": "A3 縦", "width": 297.0, "height": 420.0}
    assert info["entities"] == sum(info["types"].values()) and info["types"]["文字"] >= 1
    layer = next(layer for layer in info["layers"] if layer["name"] == "寸法")
    assert layer["entities"] > 0


def test_finds_texts_ignoring_width_case_and_spaces():
    scene = zumen.read(SFC)
    assert mcp.normalize("ｶﾀｶﾅ ＡＢＣ") == mcp.normalize("カタカナabc") == "カタカナabc"
    for query in ["ｶﾀｶﾅ", "カタカナ", "abc", "平 面"]:
        assert mcp.find_texts(scene, query=query)["total"] == 1, query
    page = mcp.find_texts(scene, limit=2, offset=1)
    assert page["total"] == len(scene.texts()) and len(page["texts"]) == 2
    assert page["texts"][0]["id"] == scene.texts()[1]["id"]
    with pytest.raises(mcp.DrawingError):
        mcp.find_texts(scene, limit=0)


def test_finds_entities_by_type_and_layer():
    scene = zumen.read(SFC)
    found = mcp.find_entities(scene, type="寸法")
    assert found["total"] == sum(found["types"].values()) > 0
    assert all("寸法" in entity["type"] for entity in found["entities"])
    assert mcp.find_entities(scene, layer="寸法", region=(0, 215, 30, 230))["entities"][0]["id"] == "#700"
    entity = mcp.get_entity(scene, "#700")
    assert {"label": "文字列", "value": "平面図"} in entity["properties"] and entity["placement"] is None
    with pytest.raises(mcp.DrawingError):
        mcp.get_entity(scene, "no such id")


def test_measures_real_lengths_through_the_scale_of_a_placement():
    scene = zumen.read(SFC)
    # A line in a part drawn at 1:10, where 1 mm on the sheet is 10 mm in reality
    entity = scene.entity("#810>#260>#220")
    assert entity is not None and entity["placement"]["scale_x"] == pytest.approx(0.1)
    x0, y0, x1, y1 = entity["bbox"]
    measured = mcp.measure(scene, [x0, y0], [x1, y1])
    assert measured["real"] == pytest.approx(measured["paper"] * 10, abs=1e-3)
    assert measured["placement"]["scale"] == "1:10"
    assert mcp.measure(scene, [x0, y0], [x1, y1], entity="#810>#260>#220")["real"] == measured["real"]
    # Without a scaled entity near the points, the length is on the sheet only
    alone = mcp.measure(scene, [1, 1], [2, 1])
    assert (alone["paper"], alone["real"]) == (1.0, None)
    with pytest.raises(mcp.DrawingError):
        mcp.measure(scene, [0, 0], [1], entity=None)


def test_renders_a_region_with_its_long_edge_in_pixels():
    scene = zumen.read(SFC)
    png, info = mcp.render(scene, size=800)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert max(info["width"], info["height"]) == 800
    text = next(text for text in info["texts"] if text["id"] == "#700")
    left, top, right, bottom = text["box"]
    x0, _, _, y1 = info["region"]
    assert left == pytest.approx((10.0 - x0) * info["scale"], abs=0.1)
    assert top == pytest.approx((y1 - 225.0) * info["scale"], abs=0.1)
    assert right > left and bottom > top
    _, part = mcp.render(scene, region=[0, 215, 30, 230], size=300)
    assert (part["width"], part["texts_drawn"]) == (300, 1)
    with pytest.raises(mcp.DrawingError):
        mcp.render(scene, size=10_000)


def test_reads_only_from_its_roots_and_up_to_its_size_limit(tmp_path):
    drawings = mcp.Drawings([FIXTURES / "sfc"])
    assert drawings.scene(SFC) is drawings.scene(SFC)  # read once while the file is unchanged
    with pytest.raises(mcp.DrawingError, match="outside"):
        drawings.scene(str(FIXTURES / "jww" / "minimal-v700.jww"))
    link = tmp_path / "link.sfc"
    link.symlink_to(SFC)
    with pytest.raises(mcp.DrawingError, match="outside"):
        mcp.Drawings([tmp_path]).scene(str(link))  # the real path is outside the root
    with pytest.raises(mcp.DrawingError, match="limit"):
        mcp.Drawings(max_bytes=10).scene(SFC)
    with pytest.raises(mcp.DrawingError, match="cannot read"):
        mcp.Drawings().scene(__file__)
    with pytest.raises(mcp.DrawingError, match="entities limit"):
        mcp.Drawings(max_entities=5).scene(SFC)


def test_serves_the_tools_over_mcp():
    sdk = pytest.importorskip("mcp")
    import anyio

    async def session():
        server = mcp.create_server(mcp.Drawings([FIXTURES]))
        async with sdk.Client(server) as client:
            tools = {tool.name: tool for tool in (await client.list_tools()).tools}
            assert set(tools) == {
                "open_drawing",
                "render_drawing",
                "find_texts",
                "find_entities",
                "get_entity",
                "measure_distance",
                "compare_drawings",
            }
            assert all(tool.annotations.read_only_hint for tool in tools.values())

            opened = await client.call_tool("open_drawing", {"path": SFC})
            assert not opened.is_error
            assert json.loads(opened.content[0].text) == opened.structured_content
            assert opened.structured_content["format"] == "sfc"

            rendered = await client.call_tool("render_drawing", {"path": SFC, "size": 400})
            image, text = rendered.content
            assert (image.type, image.mime_type) == ("image", "image/png")
            assert base64.b64decode(image.data).startswith(b"\x89PNG")
            assert json.loads(text.text)["width"] <= 400

            found = await client.call_tool("find_texts", {"path": SFC, "query": "平面図"})
            assert [text["id"] for text in found.structured_content["texts"]] == ["#700"]

            missing = await client.call_tool("get_entity", {"path": SFC, "id": "no such id"})
            assert missing.is_error and "no such id" in missing.content[0].text
            outside = await client.call_tool("open_drawing", {"path": "/etc/hosts"})
            assert outside.is_error and "outside" in outside.content[0].text

    anyio.run(session)


def test_prints_its_version():
    with pytest.raises(SystemExit) as exit:
        mcp.main(["--version"])
    assert exit.value.code == 0


def test_compares_revisions_with_an_image_of_the_changes(tmp_path):
    revised = tmp_path / "features-rev2.dxf"
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes().replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    revised.write_bytes(data)
    before, after = zumen.read(FIXTURES / "dxf" / "features.dxf"), zumen.read(revised)
    png, info = mcp.compare_revisions(before, after, size=600)
    assert png.startswith(b"\x89PNG") and max(info["image"]["width"], info["image"]["height"]) == 600
    assert (info["removed"], info["added"], info["changed"], info["changes_total"]) == (0, 0, 1, 1)
    [change] = info["changes"]
    assert (change["change"], change["before_text"], change["after_text"]) == ("changed", "揃え00", "揃え99")
    assert len(info["regions"]) == 1 and info["regions"][0]["changed"] == 1

    sdk = pytest.importorskip("mcp")
    import anyio

    async def session():
        server = mcp.create_server(mcp.Drawings([FIXTURES, tmp_path]))
        async with sdk.Client(server) as client:
            result = await client.call_tool(
                "compare_drawings", {"before": str(FIXTURES / "dxf" / "features.dxf"), "after": str(revised)}
            )
            image, text = result.content
            assert image.type == "image" and json.loads(text.text)["changed"] == 1

    anyio.run(session)
