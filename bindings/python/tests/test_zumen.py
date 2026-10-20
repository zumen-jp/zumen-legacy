"""Smallest checks that the bindings pass data through correctly. The readers are tested in Rust."""

import json
import math
import pathlib

import pytest
import zumen

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"


def same_scene(expected, actual) -> bool:
    """Numbers equal to a relative 1e-9, as in crates/zumen/tests/golden.rs (the golden may come from another CPU)."""
    if isinstance(expected, float | int) and isinstance(actual, float | int):
        return math.isclose(expected, actual, rel_tol=1e-9, abs_tol=1e-9)
    if isinstance(expected, list) and isinstance(actual, list):
        return len(expected) == len(actual) and all(map(same_scene, expected, actual))
    if isinstance(expected, dict) and isinstance(actual, dict):
        return expected.keys() == actual.keys() and all(same_scene(expected[key], actual[key]) for key in expected)
    return expected == actual


def test_reads_a_drawing_from_a_path_and_returns_the_typescript_scene():
    scene = zumen.read(FIXTURES / "jww" / "minimal-v700.jww")
    assert (scene.format, scene.version, scene.background) == ("jww", "JWW 700", "light")
    assert scene.sheet == {"name": "", "size": "A3", "width": 420.0, "height": 297.0}
    assert {"id": 1, "name": "0-0 通り芯", "visible": True, "group": "平面"} in scene.layers
    golden = json.loads((FIXTURES / "jww" / "minimal-v700.scene.json").read_text(encoding="utf-8"))
    assert json.loads(scene.to_json()) == golden


def test_reads_sxf_and_detects_sfc_and_p21():
    sfc = (FIXTURES / "sfc" / "sample.sfc").read_bytes()
    assert (zumen.detect(sfc), zumen.detect((FIXTURES / "p21" / "sample.p21").read_bytes())) == ("sfc", "p21")
    scene = zumen.read(sfc)
    assert (scene.format, scene.version, len(scene)) == ("sfc", "SXF 3.1", 6)
    assert scene.sheet == {"name": "道路", "size": "A1 横", "width": 841.0, "height": 594.0}


def test_reads_a_dxf_layout_as_a_view():
    dxf = (FIXTURES / "dxf" / "layout.dxf").read_bytes()
    assert zumen.detect(dxf) == "dxf"
    scene = zumen.read(dxf, view="Layout1")
    assert (scene.format, scene.view, scene.views) == ("dxf", "Layout1", [("Model", "モデル"), ("Layout1", "Layout1")])
    golden = json.loads((FIXTURES / "dxf" / "layout@Layout1.scene.json").read_text(encoding="utf-8"))
    assert same_scene(golden, json.loads(scene.to_json()))


def test_renders_png_and_reports_unreadable_input():
    scene = zumen.read((FIXTURES / "jww" / "minimal-v600.jww").read_bytes())
    png = scene.render_png(dpi=50)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert zumen.detect(b"not a drawing") is None
    with pytest.raises(zumen.ReadError):
        zumen.read(b"not a drawing")
    with pytest.raises(zumen.RenderError):
        scene.render_png(dpi=0)


def test_extracts_texts_and_entities_in_paper_millimeters():
    scene = zumen.read(FIXTURES / "sfc" / "features.sfc")
    texts = {text["id"]: text for text in scene.texts()}
    assert texts["#700"] == {
        "id": "#700",
        "type": "文字",
        "text": "平面図",
        "x": 10.0,
        "y": 220.0,
        "height": 5.0,
        "angle": 0.0,
        "bbox": (10.0, 220.0, 25.0, 225.0),
        "layers": ["寸法"],
    }
    assert math.isclose(texts["#710"]["angle"], 30.0)
    assert [text["id"] for text in scene.texts(region=(0, 215, 30, 230))] == ["#700"]

    entity = scene.entity("#700")
    assert entity is not None
    assert (entity["type"], entity["text"], entity["layers"], entity["placement"]) == ("文字", "平面図", ["寸法"], None)
    assert ("文字列", "平面図") in entity["properties"]
    assert scene.entity("no such id") is None
    every = scene.entities(hidden_layers=[])
    assert len(every) == len(scene) and every[0] == scene.entity(every[0]["id"])

    # Multiline text breaks between lines, and entities inside blocks carry the scale of their placement
    dxf = zumen.read(FIXTURES / "dxf" / "features.dxf")
    lines = next(text["text"] for text in dxf.texts() if text["id"] == "151").split("\n")
    assert lines[:2] == ["長い文章を折り返す", "folding long"]
    inside = next(entity for entity in zumen.read(FIXTURES / "dxf" / "blocks.dxf").entities() if entity["placement"])
    assert set(inside["placement"]) == {"name", "scale_x", "scale_y", "inverse"}
    assert len(inside["placement"]["inverse"]) == 6


def test_lists_entities_as_an_arrow_table():
    pa = pytest.importorskip("pyarrow")
    scene = zumen.read(FIXTURES / "sfc" / "features.sfc")
    table = scene.to_arrow()
    assert table.num_rows == len(scene.entities())
    assert table.schema.field("layers").type == pa.list_(pa.string())
    assert table.schema.field("properties").type == pa.list_(
        pa.struct([("label", pa.string()), ("value", pa.string())])
    )
    row = next(row for row in table.to_pylist() if row["id"] == "#700")
    assert (row["text"], row["min_x"], row["max_y"], row["placement"]) == ("平面図", 10.0, 225.0, None)
    assert {"label": "文字列", "value": "平面図"} in row["properties"]
    assert scene.to_arrow(region=(0, 215, 30, 230)).column("id").to_pylist() == ["#700"]


def test_renders_with_the_pixel_box_of_each_entity():
    scene = zumen.read(FIXTURES / "sfc" / "features.sfc")
    image = scene.render(size=(800, 600), margin=10)
    assert isinstance(image, zumen.Rendering)
    assert image.png.startswith(b"\x89PNG\r\n\x1a\n") and image._repr_png_() == image.png
    assert (image.width, image.height) == (800, 600)
    x, y = image.to_paper(*image.to_pixel(12.5, 34.5))
    assert math.isclose(x, 12.5) and math.isclose(y, 34.5)

    drawn = {entity["id"]: entity for entity in image.entities}
    text = drawn["#700"]
    left, top = image.to_pixel(10.0, 225.0)
    right, bottom = image.to_pixel(25.0, 220.0)
    assert text["text"] == "平面図"
    assert all(math.isclose(a, b) for a, b in zip(text["box"], (left, top, right, bottom), strict=True))
    hits = [entity["id"] for entity in image.entities_at((left + right) / 2, (top + bottom) / 2)]
    assert "#700" in hits

    # With a region, only the entities drawn in that region
    part = scene.render(dpi=100, region=(0, 215, 30, 230))
    assert part.scale == pytest.approx(100 / 25.4)
    assert part.region[0] == pytest.approx(0) and part.region[3] == pytest.approx(230)
    assert "#700" in {entity["id"] for entity in part.entities}
    assert "#750" not in {entity["id"] for entity in part.entities}


def test_converts_dxf_and_dwg_drawings_to_dxf(tmp_path):
    converted = zumen.convert(FIXTURES / "dwg" / "r2018.dwg")
    assert isinstance(converted, zumen.Converted)
    assert (converted.format, len(converted), converted.notices) == ("dxf", len(converted.data), [])
    assert converted.data.startswith(b"  0\r\nSECTION\r\n  2\r\nHEADER\r\n")
    assert len(zumen.read(converted.data)) == len(zumen.read(FIXTURES / "dwg" / "r2018.dwg"))
    path = tmp_path / "layout.dxf"
    zumen.convert((FIXTURES / "dxf" / "layout.dxf").read_bytes(), "dxf").save(path)
    assert zumen.read(path, view="Layout1").view == "Layout1"

    # What could not be written as it was comes back as notices, like the readers'
    notices = zumen.convert(FIXTURES / "dxf" / "features.dxf").notices
    image = next(notice for notice in notices if notice["code"] == "image")
    assert (image["level"], image["count"]) == ("warning", 1)
    with pytest.raises(zumen.ReadError):
        zumen.convert(FIXTURES / "jww" / "minimal-v700.jww")
    with pytest.raises(zumen.ReadError):
        zumen.convert(FIXTURES / "dxf" / "r12.dxf", "sfc")


def test_converted_dxf_reads_in_ezdxf_and_passes_its_audit(tmp_path):
    """ezdxf, another implementation, reads every converted fixture strictly, and its audit finds only the malformed
    input that some fixtures pin on purpose (most fixtures are too small for ezdxf to read before conversion)."""
    ezdxf = pytest.importorskip("ezdxf")
    from ezdxf.audit import AuditError

    pinned = {
        "features.dxf": {
            AuditError.INVALID_BLOCK_REFERENCE_CYCLE,
            AuditError.INVALID_COLOR_INDEX,
            AuditError.INVALID_TRANSPARENCY,
            AuditError.INVALID_VERTEX_COUNT,
        },
        "limits.dxf": {AuditError.INVALID_BLOCK_REFERENCE_CYCLE},
    }
    sources = sorted((FIXTURES / "dxf").glob("*.dxf")) + sorted((FIXTURES / "dwg").glob("*.dwg"))
    for source in sources:
        path = tmp_path / f"{source.name}.dxf"
        zumen.convert(source).save(path)
        auditor = ezdxf.readfile(path).audit()
        found = {entry.code for entry in [*auditor.errors, *auditor.fixes]}
        assert found == pinned.get(source.name, set()), source.name
