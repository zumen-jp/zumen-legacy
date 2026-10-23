"""Drawing markup: reading and checking documents, drawing them over a drawing and carrying them to new revisions."""

import hashlib
import json
import pathlib

import pytest
import zumen
from zumen import tools

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"
KINDS = FIXTURES / "markup" / "kinds.markup.json"


def drawing(dx: float = 0.0, dy: float = 0.0) -> zumen.Edited:
    """A rectangle of four lines and three texts, moved by (dx, dy)."""

    def at(x: float, y: float) -> list[float]:
        return [x + dx, y + dy]

    return zumen.create(
        "dxf",
        [
            {"op": "add", "kind": "line", "start": at(0, 0), "end": at(200, 0)},
            {"op": "add", "kind": "line", "start": at(200, 0), "end": at(200, 100)},
            {"op": "add", "kind": "line", "start": at(0, 100), "end": at(200, 100)},
            {"op": "add", "kind": "line", "start": at(0, 0), "end": at(0, 100)},
            {"op": "add", "kind": "text", "position": at(20, 20), "text": "A", "height": 5},
            {"op": "add", "kind": "text", "position": at(80, 50), "text": "B", "height": 5},
            {"op": "add", "kind": "text", "position": at(150, 80), "text": "C", "height": 5},
        ],
    )


def markup_for(data: bytes, items: list[dict]) -> zumen.MarkupDocument:
    return zumen.read_markup(
        {
            "format": "zumen-markup",
            "version": 1,
            "drawing": {"sha256": zumen.drawing_hash(data), "format": "dxf"},
            "items": items,
        }
    )


def test_reads_documents_keeping_unknown_keys():
    text = KINDS.read_text(encoding="utf-8")
    for source in (KINDS, str(KINDS), text, text.encode("utf-8"), json.loads(text)):
        document = zumen.read_markup(source)
        assert document == json.loads(text)
    assert document["x-extension"] == [1, "two", {"three": 3}]  # type: ignore[typeddict-item]


@pytest.mark.parametrize("case", json.loads((FIXTURES / "markup" / "errors.json").read_text(encoding="utf-8")))
def test_refuses_documents_like_the_typescript_side(case):
    with pytest.raises(zumen.MarkupError) as error:
        zumen.read_markup(case.get("text") or json.dumps(case["document"]))
    assert (error.value.code, error.value.item, error.value.id, error.value.key) == (
        case["code"],
        case["item"],
        case["id"],
        case["key"],
    )


def test_the_fixture_follows_the_schema():
    jsonschema = pytest.importorskip("jsonschema")
    schema = zumen.markup_schema()
    jsonschema.Draft202012Validator.check_schema(schema)
    jsonschema.Draft202012Validator(schema).validate(json.loads(KINDS.read_text(encoding="utf-8")))


def test_hashes_drawings(tmp_path):
    path = tmp_path / "plan.dxf"
    data = drawing().data
    path.write_bytes(data)
    assert zumen.drawing_hash(path) == zumen.drawing_hash(data) == hashlib.sha256(data).hexdigest()


def test_draws_markup_over_the_drawing():
    created = drawing()
    scene = zumen.read(created.data)
    markup = markup_for(
        created.data,
        [
            {"id": "f", "kind": "finding", "view": "Model", "position": [100, 50], "status": "open", "label": "1"},
            {"id": "q", "kind": "cloud", "view": "Model", "points": [[20, 20], [80, 20], [80, 60]], "category": "q"},
            {"id": "other", "kind": "finding", "view": "Layout1", "position": [10, 10], "status": "open"},
        ],
    )
    plain = scene.render_svg()
    drawn = scene.render_svg(markup=markup, markup_colors={"q": "#0050c8"})
    assert "#0050c8" in drawn and "#0050c8" not in plain
    assert "#ff0000" in drawn
    # The item of another view is not drawn: the same picture without it
    assert drawn == scene.render_svg(
        markup=markup_for(created.data, markup["items"][:2]), markup_colors={"q": "#0050c8"}
    )
    assert scene.render_png(dpi=50, markup=markup) != scene.render_png(dpi=50)
    assert scene.render_pdf(markup=json.dumps(markup))[:5] == b"%PDF-"
    image = scene.render(size=(400, 300), markup=markup)
    assert image.png.startswith(b"\x89PNG")
    with pytest.raises(zumen.MarkupError):
        scene.render_png(markup={"format": "zumen-markup"})


def test_carries_markup_to_an_edited_revision(tmp_path):
    before = drawing()
    ids = before.created
    after = zumen.edit(
        before.data,
        [{"op": "delete", "id": ids[3]}, {"op": "update", "id": ids[1], "set": {"end": [200, 140]}}],
    )
    markup = markup_for(
        before.data,
        [
            {
                "id": "bottom",
                "kind": "finding",
                "view": "Model",
                "position": [100, 0],
                "status": "open",
                "entities": [ids[0]],
            },
            {
                "id": "right",
                "kind": "finding",
                "view": "Model",
                "position": [200, 50],
                "status": "open",
                "entities": [ids[1]],
            },
            {
                "id": "left",
                "kind": "finding",
                "view": "Model",
                "position": [0, 50],
                "status": "open",
                "entities": [ids[3]],
            },
        ],
    )
    carried = zumen.carry_markup(markup, before.data, after.data)
    assert (carried["unchanged"], carried["changed"], carried["removed"]) == (1, 1, 1)
    document = carried["markup"]
    assert document["drawing"]["sha256"] == zumen.drawing_hash(after.data)
    assert [item["carried"]["state"] for item in document["items"]] == ["unchanged", "changed", "removed"]
    assert document["items"][2]["entities"] == []
    # A revision moved as a whole moves the items by the offset
    moved = zumen.carry_markup(markup, before.data, drawing(100, 50).data)
    assert moved["markup"]["items"][0]["position"] == [200, 50]
    with pytest.raises(zumen.CarryError) as error:
        zumen.carry_markup(markup, after.data, before.data)
    assert error.value.code == "drawing-mismatch"
    assert zumen.carry_markup(markup, after.data, before.data, ignore_hash=True)["removed"] >= 0
    with pytest.raises(ValueError):
        zumen.carry_markup(markup, before.data, after.data, tolerance=0)


def test_tools_write_list_and_carry_markup(tmp_path):
    before = drawing()
    plan = tmp_path / "plan.dxf"
    plan.write_bytes(before.data)
    revised = tmp_path / "plan-rev2.dxf"
    revised.write_bytes(zumen.edit(before.data, [{"op": "delete", "id": before.created[0]}]).data)
    drawings = tools.Drawings([tmp_path])
    path = tmp_path / "plan.markup.json"
    with pytest.raises(tools.DrawingError, match="give the drawing"):
        tools.write_markup(drawings, path, add=[{"kind": "text", "position": [0, 0], "text": "x"}])
    written = tools.write_markup(
        drawings,
        path,
        drawing=plan,
        author="agent",
        add=[
            {
                "kind": "finding",
                "view": "Model",
                "position": [100, 0],
                "status": "open",
                "text": "壁厚",
                "entities": [before.created[0]],
            },
            {"id": "note", "kind": "text", "view": "Model", "position": [10, 90], "text": "確認"},
        ],
    )
    assert written["created"] and written["items"] == 2 and written["added"][1] == "note"
    document = zumen.read_markup(path)
    assert document["drawing"] == {"sha256": zumen.drawing_hash(before.data), "format": "dxf"}
    finding = document["items"][0]
    assert finding["author"] == "agent" and finding["createdAt"].endswith("Z")
    updated = tools.write_markup(
        drawings, path, update=[{"id": finding["id"], "set": {"status": "resolved", "text": None}}], remove=["note"]
    )
    assert (updated["created"], updated["items"], updated["removed"]) == (False, 1, ["note"])
    assert "text" not in zumen.read_markup(path)["items"][0]
    with pytest.raises(tools.DrawingError, match="points"):
        tools.write_markup(drawings, path, add=[{"kind": "cloud", "points": [[0, 0]]}])
    with pytest.raises(tools.DrawingError, match="no item"):
        tools.write_markup(drawings, path, remove=["missing"])

    page = tools.list_markup(drawings, path, drawing=plan)
    assert page["matches"] is True and page["views"] == {"Model": 1} and page["total"] == 1
    assert page["items"][0]["box"] == [100, 0, 100, 0] and page["items"][0]["status"] == "resolved"
    assert tools.list_markup(drawings, path, drawing=revised)["matches"] is False
    assert tools.list_markup(drawings, path, status="open")["total"] == 0

    output = tmp_path / "plan-rev2.markup.json"
    carried = tools.carry_markup_revision(drawings, path, plan, revised, output)
    assert (carried["unchanged"], carried["changed"], carried["removed"]) == (0, 0, 1)
    assert tools.list_markup(drawings, output, drawing=revised)["items"][0]["carried"] == "removed"

    png, info = tools.render(zumen.read(plan), markup=document, markup_colors={"x": "#00ff00"})
    assert png.startswith(b"\x89PNG") and info["markup_items"] == 2
    outside = tmp_path.parent / "elsewhere.markup.json"
    with pytest.raises(tools.DrawingError, match="outside"):
        tools.write_markup(drawings, outside, drawing=plan)
