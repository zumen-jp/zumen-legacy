"""Editing drawings in place and creating new ones (docs/decisions/0012). The editors are tested in Rust; these check
that operations, results and errors pass through, and that ezdxf, another implementation, accepts what was edited."""

import json
import pathlib

import pytest
import zumen

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"


def test_edits_a_drawing_with_operations_given_as_python_values(tmp_path):
    source = FIXTURES / "dxf" / "survey.dxf"
    edited = zumen.edit(
        source,
        [
            {"op": "add-layer", "name": "確認", "color": "#ff0000"},
            {"op": "add", "kind": "line", "layer": "確認", "start": [51234.5, 120456.25], "end": [51300, 120500]},
            {"op": "update", "id": "21", "set": {"layer": "確認", "radius": 2.5}},
            {"op": "delete", "id": "20"},
        ],
    )
    assert isinstance(edited, zumen.Edited)
    assert edited.created[0] == "確認" and len(edited.created) == 2
    assert len(edited) == len(edited.data)
    scene = zumen.read(edited.data)
    ids = {entity["id"] for entity in scene.entities(hidden_layers=[])}
    assert edited.created[1] in ids and "20" not in ids
    circle = scene.entity("21")
    assert ("レイヤ", "確認") in circle["properties"] and ("半径", "2.5") in circle["properties"]
    # The same operations as JSON text
    again = zumen.edit(source.read_bytes(), json.dumps([{"op": "delete", "id": "20"}]))
    assert "20" not in {entity["id"] for entity in zumen.read(again.data).entities()}
    path = tmp_path / "edited.dxf"
    edited.save(path)
    assert path.read_bytes() == edited.data
    # No operations: the same bytes
    assert zumen.edit(source, []).data == source.read_bytes()


def test_reports_the_operation_that_cannot_be_applied():
    source = FIXTURES / "dxf" / "survey.dxf"
    with pytest.raises(zumen.EditError) as raised:
        zumen.edit(source, [{"op": "delete", "id": "20"}, {"op": "update", "id": "21", "set": {"wall": 1}}])
    assert (raised.value.code, raised.value.operation) == ("unknown-key", 1)
    assert str(raised.value).startswith("操作 1: ")
    with pytest.raises(zumen.EditError) as raised:
        zumen.edit(source, [], format="jwc")
    assert (raised.value.code, raised.value.operation) == ("unsupported-format", None)
    with pytest.raises(zumen.EditError) as raised:
        zumen.edit(source, [{"op": "add", "kind": "line"}], max_bytes=10)
    assert raised.value.code == "too-large"


def test_creates_a_new_drawing():
    created = zumen.create(
        "dxf",
        [
            {"op": "add-layer", "name": "敷地", "color": 3},
            {
                "op": "add",
                "kind": "polyline",
                "layer": "敷地",
                "vertices": [[0, 0], [8000, 0], [8000, 6000]],
                "closed": True,
            },
        ],
        units="m",
    )
    scene = zumen.read(created.data)
    assert [entity["id"] for entity in scene.entities()] == created.created[1:]
    assert ("単位", "m") in scene.metadata
    with pytest.raises(zumen.EditError):
        zumen.create("dxf", units="furlong")


def test_edits_and_creates_every_editable_format():
    line = [{"op": "add", "kind": "line", "start": [0, 0], "end": [1000, 500]}]
    for path in ["dwg/r2018.dwg", "jww/minimal-v700.jww", "sfc/features.sfc", "p21/features.p21"]:
        source = (FIXTURES / path).read_bytes()
        assert zumen.edit(source, []).data == source, path
        edited = zumen.edit(source, line)
        assert len(zumen.read(edited.data, view="Model")) == len(zumen.read(source, view="Model")) + 1, path
    for to in ["dwg", "jww", "sfc", "p21", "sfz", "p2z"]:
        created = zumen.create(to, line)
        assert zumen.detect(created.data) == to
        assert len(zumen.read(created.data)) == 1, to
    with pytest.raises(zumen.EditError) as raised:
        zumen.create("jww", units="m")
    assert raised.value.code == "invalid-value"


def test_edited_dxf_passes_the_audit_of_ezdxf(tmp_path):
    """Drawings that ezdxf audits without findings (every fixture converted to AutoCAD 2013 DXF, and a new drawing)
    still have none after adding, changing and deleting entities and layers."""
    ezdxf = pytest.importorskip("ezdxf")
    sources = [
        *sorted((FIXTURES / "dxf").glob("*.dxf")),
        *sorted((FIXTURES / "dwg").glob("*.dwg")),
        *sorted((FIXTURES / "sfc").glob("*.sfc")),
        *sorted((FIXTURES / "jww").glob("*.jww")),
    ]
    pinned = {"features.dxf", "limits.dxf"}  # malformed on purpose (test_converted_dxf_reads_in_ezdxf...)
    drawings = [(source.name, zumen.convert(source).data) for source in sources if source.name not in pinned]
    drawings.append(("new", zumen.create("dxf").data))
    for name, data in drawings:
        scene = zumen.read(data, view="Model")
        entities = [entity for entity in scene.entities(hidden_layers=[]) if ">" not in entity["id"]]
        operations = [
            {"op": "add-layer", "name": "CHECK", "color": 1, "lineWidth": 0.35},
            {"op": "add", "kind": "line", "layer": "CHECK", "start": [0, 0], "end": [10, 5]},
            {"op": "add", "kind": "circle", "center": [0, 0], "radius": 3, "color": "#00ff00"},
            {"op": "add", "kind": "arc", "center": [0, 0], "radius": 4, "angle": [0, 90]},
            {"op": "add", "kind": "ellipse", "center": [0, 0], "majorAxis": [5, 0], "ratio": 0.5},
            {"op": "add", "kind": "point", "position": [1, 1]},
            {"op": "add", "kind": "polyline", "vertices": [[0, 0], [5, 0], [5, 5]], "bulges": [0, 0.5, 0]},
            {"op": "add", "kind": "text", "position": [0, -5], "text": "確認済み", "height": 2.5, "rotation": 15},
            {"op": "add", "kind": "text", "position": [0, -10], "text": "一行目\n二行目", "height": 2},
            {"op": "add", "kind": "line", "container": "layout:Layout1", "start": [0, 0], "end": [1, 1]},
        ]
        if entities:
            operations.append({"op": "update", "id": entities[0]["id"], "set": {"layer": "CHECK", "color": 5}})
        if len(entities) > 1:
            operations.append({"op": "delete", "id": entities[-1]["id"]})
        layouts = {view_id for view_id, _ in scene.views or []}
        if "Layout1" not in layouts:
            operations = [operation for operation in operations if operation.get("container") != "layout:Layout1"]
        operations += [
            {"op": "add-block", "name": "CHECK-BLOCK", "basePoint": [1, 1]},
            {"op": "add", "kind": "circle", "container": "block:CHECK-BLOCK", "center": [1, 1], "radius": 1},
            {"op": "add", "kind": "block-reference", "block": "CHECK-BLOCK", "position": [20, 20]},
            {"op": "add", "kind": "dimension", "start": [0, 0], "end": [40, 0], "position": [0, -10]},
        ]
        edited = zumen.edit(data, operations)
        path = tmp_path / f"{name}.edited.dxf"
        edited.save(path)
        doc = ezdxf.readfile(path)
        auditor = doc.audit()
        found = [(entry.code, entry.message) for entry in [*auditor.errors, *auditor.fixes]]
        assert found == [], (name, found)
        added = [doc.entitydb[handle] for handle in edited.created[1:-4]]
        assert [entity.dxftype() for entity in added[:9]] == [
            "LINE",
            "CIRCLE",
            "ARC",
            "ELLIPSE",
            "POINT",
            "LWPOLYLINE",
            "TEXT",
            "MTEXT",
            "LINE",
        ][: len(added[:9])], name
        assert added[6].dxf.text == "確認済み"
        assert added[7].plain_text() == "一行目\n二行目"
        assert doc.layers.get("CHECK").dxf.lineweight == 35
        block = doc.blocks.get("CHECK-BLOCK")
        assert [entity.dxftype() for entity in block] == ["CIRCLE"], name
        assert doc.entitydb[edited.created[-2]].dxf.name == "CHECK-BLOCK", name
        dimension = doc.entitydb[edited.created[-1]]
        assert dimension.dxftype() == "DIMENSION" and dimension.get_measurement() == 40, name
        assert len(doc.blocks.get(dimension.dxf.geometry)) >= 7, name
