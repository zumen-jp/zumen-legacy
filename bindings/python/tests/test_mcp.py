"""The MCP server: the tools through an in-memory MCP client (the functions behind them are in test_tools.py)."""

import base64
import json
import pathlib

import pytest
import zumen
from zumen import mcp, tools

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"
SFC = str(FIXTURES / "sfc" / "features.sfc")


def test_wraps_the_functions_of_zumen_tools():
    # The functions behind the tools keep their names in zumen.mcp
    for name in ["summary", "render", "find_texts", "find_entities", "get_entity", "measure", "compare_revisions"]:
        assert getattr(mcp, name) is getattr(tools, name)
    assert (mcp.Drawings, mcp.DrawingError, mcp.normalize) == (tools.Drawings, tools.DrawingError, tools.normalize)
    assert (mcp.MAX_LIMIT, mcp.MAX_POINTS) == (tools.MAX_LIMIT, tools.MAX_POINTS)


def test_serves_the_tools_over_mcp():
    sdk = pytest.importorskip("mcp")
    import anyio

    fractions = tools.frame_region_of(zumen.read(SFC), [0, 215, 30, 230])

    async def session():
        server = mcp.create_server(mcp.Drawings([FIXTURES]))
        async with sdk.Client(server) as client:
            listed = {tool.name: tool for tool in (await client.list_tools()).tools}
            assert set(listed) == {
                "open_drawing",
                "render_drawing",
                "find_texts",
                "find_entities",
                "get_entity",
                "measure_distance",
                "compare_drawings",
            }
            assert all(tool.annotations.read_only_hint for tool in listed.values())

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
            near = await client.call_tool("find_texts", {"path": SFC, "frame_region": fractions})
            assert [text["id"] for text in near.structured_content["texts"]] == ["#700"]
            part = await client.call_tool("render_drawing", {"path": SFC, "frame_region": fractions, "size": 300})
            assert json.loads(part.content[1].text)["texts_drawn"] == 1

            entity = await client.call_tool("get_entity", {"path": SFC, "id": "#810>#290", "max_points": 8})
            assert [shape["shape"] for shape in entity.structured_content["geometry"]] == ["polyline", "polyline"]
            listed = await client.call_tool("find_entities", {"path": SFC, "geometry": True, "limit": 5})
            assert all("geometry" in item for item in listed.structured_content["entities"])
            too_many = await client.call_tool("get_entity", {"path": SFC, "id": "#600", "max_points": 10**6})
            assert too_many.is_error

            missing = await client.call_tool("get_entity", {"path": SFC, "id": "no such id"})
            assert missing.is_error and "no such id" in missing.content[0].text
            outside = await client.call_tool("open_drawing", {"path": "/etc/hosts"})
            assert outside.is_error and "outside" in outside.content[0].text

    anyio.run(session)


def test_prints_its_version():
    with pytest.raises(SystemExit) as exit:
        mcp.main(["--version"])
    assert exit.value.code == 0


def test_compares_revisions_over_mcp(tmp_path):
    sdk = pytest.importorskip("mcp")
    import anyio

    revised = tmp_path / "features-rev2.dxf"
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes().replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    revised.write_bytes(data)

    async def session():
        server = mcp.create_server(mcp.Drawings([FIXTURES, tmp_path]))
        async with sdk.Client(server) as client:
            result = await client.call_tool(
                "compare_drawings", {"before": str(FIXTURES / "dxf" / "features.dxf"), "after": str(revised)}
            )
            image, text = result.content
            assert image.type == "image" and json.loads(text.text)["changed"] == 1

    anyio.run(session)
