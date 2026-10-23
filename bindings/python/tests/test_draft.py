"""Drafting from a drawing spec (decision 0018): zumen.draft, the tool function behind draft_drawing, the MCP tool,
and ezdxf, another implementation, accepting what was drafted."""

import json
import pathlib

import pytest
import zumen
from zumen import mcp, tools

SPEC_PATH = pathlib.Path(__file__).parents[3] / "crates" / "zumen-draft" / "examples" / "sheets.json"
SPEC = json.loads(SPEC_PATH.read_text(encoding="utf-8"))
SHEETS = ["A01-1", "A05-1", "A10-1", "A20-1"]


def test_drafts_a_spec_given_as_a_dict_a_string_or_a_path():
    drafted = zumen.draft(SPEC, "dwg")
    assert drafted.format == "dwg" and drafted.notices == []
    assert zumen.draft(json.dumps(SPEC), "dwg").data == drafted.data
    assert zumen.draft(SPEC_PATH, "dwg").data == drafted.data
    scene = zumen.read(drafted.data)
    assert [view_id for view_id, _ in scene.views] == ["Model", *SHEETS]
    plan = zumen.read(drafted.data, view="A10-1")
    assert {"事務室", "21,600", "X4"} <= {text["text"] for text in plan.texts()}


def test_writes_one_sheet_of_jww():
    drafted = zumen.draft(SPEC, "jww", sheet="A20-1")
    assert "GL±0" in {text["text"] for text in zumen.read(drafted.data).texts()}


def test_raises_draft_errors_with_the_place_and_code():
    spec = {"sheets": [{"name": "S", "items": [{"type": "line", "from": [0, 0], "to": [1, 1], "weight": 2}]}]}
    with pytest.raises(zumen.DraftError) as raised:
        zumen.draft(spec)
    assert (raised.value.code, raised.value.path) == ("unknown-key", "sheets[0].items[0]")


def test_drafted_dxf_passes_the_audit_of_ezdxf(tmp_path):
    ezdxf = pytest.importorskip("ezdxf")
    for version in ["2018", "2000", "R12"]:
        path = tmp_path / f"sheets-{version}.dxf"
        zumen.draft(SPEC, "dxf", version=version).save(path)
        doc = ezdxf.readfile(path)
        auditor = doc.audit()
        found = [(entry.code, entry.message) for entry in [*auditor.errors, *auditor.fixes]]
        assert found == [], (version, found)
        if version != "R12":
            assert [layout.name for layout in doc.layouts if layout.name != "Model"] == SHEETS
            dimensions = doc.layout("Model").query("DIMENSION")
            assert len(dimensions) == 18  # 6 on the site plan, 7 on the floor plan and 5 on the elevation
            assert all(doc.blocks.get(dimension.dxf.geometry) is not None for dimension in dimensions)


def test_draft_drawing_writes_inside_the_roots(tmp_path):
    drawings = tools.Drawings([tmp_path])
    info = tools.draft_drawing(drawings, SPEC, tmp_path / "sheets.dxf")
    assert (info["format"], info["sheets"], info["notices"]) == ("dxf", SHEETS, [])
    assert (tmp_path / "sheets.dxf").stat().st_size == info["bytes"]
    one = tools.draft_drawing(drawings, SPEC, tmp_path / "plan.sfc", sheet="A10-1")
    assert one["sheets"] == ["A10-1"]
    with pytest.raises(tools.DrawingError, match="outside"):
        tools.draft_drawing(drawings, SPEC, tmp_path.parent / "elsewhere.dxf")
    with pytest.raises(tools.DrawingError, match="cannot write"):
        tools.draft_drawing(drawings, SPEC, tmp_path / "sheets.pdf")
    bad = {"sheets": [{"name": "S", "items": [{"type": "cloud"}]}]}
    with pytest.raises(tools.DrawingError, match=r"sheets\[0\]\.items\[0\].*unknown-type"):
        tools.draft_drawing(drawings, bad, tmp_path / "bad.dxf")


def test_drafts_and_renders_over_mcp(tmp_path):
    sdk = pytest.importorskip("mcp")
    import anyio

    async def session():
        server = mcp.create_server(mcp.Drawings([tmp_path]))
        async with sdk.Client(server) as client:
            output = str(tmp_path / "sheets.dwg")
            drafted = await client.call_tool("draft_drawing", {"spec": SPEC, "output": output})
            assert not drafted.is_error, drafted.content
            assert drafted.structured_content["sheets"] == SHEETS
            rendered = await client.call_tool("render_drawing", {"path": output, "view": "A05-1", "size": 400})
            assert not rendered.is_error
            wrong = await client.call_tool(
                "draft_drawing", {"spec": {"sheets": [{"name": "S", "items": [{"type": "cloud"}]}]}, "output": output}
            )
            assert wrong.is_error and "unknown-type" in wrong.content[0].text

    anyio.run(session)


def test_the_sample_follows_the_schema_and_mistakes_do_not():
    jsonschema = pytest.importorskip("jsonschema")
    schema = zumen.draft_schema()
    jsonschema.Draft202012Validator.check_schema(schema)
    validator = jsonschema.Draft202012Validator(schema)
    assert list(validator.iter_errors(SPEC)) == []
    item = {"type": "line", "from": [0, 0], "to": [1, 1], "weight": 2}
    assert list(validator.iter_errors({"sheets": [{"name": "S", "items": [item]}]}))


def _template(to: str) -> bytes:
    """A template: a title block with attributes on the layout A3."""
    return zumen.create(
        to,
        [
            {"op": "add-block", "name": "TITLE"},
            {
                "op": "add",
                "kind": "attribute-definition",
                "container": "block:TITLE",
                "tag": "DWGNAME",
                "text": "未定",
                "position": [5, 5],
                "height": 3,
            },
            {"op": "add-layout", "name": "A3", "paperSize": [420, 297]},
            {
                "op": "add",
                "kind": "block-reference",
                "container": "layout:A3",
                "block": "TITLE",
                "position": [300, 10],
                "attributes": {"DWGNAME": "-"},
            },
        ],
    ).data


INTO = {
    "sheets": [
        {
            "name": "A3",
            "fields": {"DWGNAME": "配置図"},
            "views": [
                {
                    "viewport": [20, 20, 280, 280],
                    "scale": 100,
                    "items": [
                        {"type": "rectangle", "from": [0, 0], "to": [10000, 8000]},
                        {"type": "dimension", "from": [0, 0], "to": [10000, 0], "offset": -10},
                    ],
                }
            ],
        }
    ]
}


def test_drafts_into_a_template_and_fills_its_title_block(tmp_path):
    for to in ["dxf", "dwg"]:
        template = _template(to)
        drafted = zumen.draft_into(INTO, template)
        assert drafted.format == to
        sheet = zumen.read(drafted.data, view="A3")
        texts = {text["text"] for text in sheet.texts()}
        assert {"配置図", "10,000"} <= texts and "-" not in texts, texts
        path = tmp_path / f"template.{to}"
        path.write_bytes(template)
        assert zumen.draft_into(INTO, path).data == drafted.data
    # JWW and SXF templates take one sheet on their paper; they have no attributes for the fields
    for to in ["jww", "sfc"]:
        drafted = zumen.draft_into(INTO, zumen.create(to).data)
        assert drafted.format == to
        texts = {text["text"] for text in zumen.read(drafted.data).texts()}
        assert "10,000" in texts, texts
        assert "unused-field" in {notice["code"] for notice in drafted.notices}


def test_drafted_into_dxf_passes_the_audit_of_ezdxf(tmp_path):
    ezdxf = pytest.importorskip("ezdxf")
    path = tmp_path / "into.dxf"
    zumen.draft_into(SPEC, _template("dxf")).save(path)
    doc = ezdxf.readfile(path)
    auditor = doc.audit()
    found = [(entry.code, entry.message) for entry in [*auditor.errors, *auditor.fixes]]
    assert found == [], found
    assert [layout.name for layout in doc.layouts if layout.name != "Model"][-4:] == SHEETS


def test_draft_drawing_draws_into_a_template(tmp_path):
    drawings = tools.Drawings([tmp_path])
    template = tmp_path / "template.dwg"
    template.write_bytes(_template("dwg"))
    info = tools.draft_drawing(drawings, INTO, tmp_path / "out.dwg", into=template)
    assert (info["format"], info["sheets"]) == ("dwg", ["A3"])
    with pytest.raises(tools.DrawingError, match="DWG"):
        tools.draft_drawing(drawings, INTO, tmp_path / "out.dxf", into=template)
    with pytest.raises(tools.DrawingError, match="outside"):
        tools.draft_drawing(drawings, INTO, tmp_path / "out2.dwg", into=tmp_path.parent / "elsewhere.dwg")
