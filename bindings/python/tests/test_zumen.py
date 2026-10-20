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


def test_stops_reading_at_the_limits():
    path = FIXTURES / "sfc" / "features.sfc"
    total = len(zumen.read(path))
    roomy = zumen.read(path, max_bytes=path.stat().st_size, max_entities=total, timeout=60)
    assert roomy.to_json() == zumen.read(path).to_json()
    for limits, limit in [
        ({"max_entities": total - 1}, "entities"),
        ({"timeout": 0}, "time"),
        ({"max_bytes": 10}, "bytes"),
    ]:
        with pytest.raises(zumen.LimitError) as error:
            zumen.read(path, **limits)
        assert error.value.limit == limit and isinstance(error.value, zumen.ReadError)
    with pytest.raises(zumen.LimitError) as error:
        zumen.read(path.read_bytes(), max_bytes=10)
    assert error.value.limit == "bytes"
    with pytest.raises(ValueError):
        zumen.read(path, timeout=-1)


def test_reads_compressed_sxf_and_stops_unpacking_at_the_limit(tmp_path):
    import zipfile

    p21 = FIXTURES / "p21" / "sample.p21"
    archive = tmp_path / "sample.p2z"
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
        zipped.write(p21, "DRAWING/SAMPLE.P21")
    assert zumen.detect(archive.read_bytes()) == "p2z"
    scene = zumen.read(archive)
    assert json.loads(scene.to_json())["entities"] == json.loads(zumen.read(p21).to_json())["entities"]
    assert ("圧縮", "P2Z（DRAWING/SAMPLE.P21）") in scene.metadata

    bomb = tmp_path / "bomb.sfz"
    with zipfile.ZipFile(bomb, "w", compression=zipfile.ZIP_DEFLATED) as zipped:
        zipped.writestr("bomb.sfc", b" " * (1 << 22))
    assert bomb.stat().st_size < 10_000
    with pytest.raises(zumen.LimitError) as error:
        zumen.read(bomb, max_bytes=1 << 20)
    assert error.value.limit == "bytes"


def revised_features() -> bytes:
    """features.dxf with one text changed and one line removed, as a later revision."""
    data = (FIXTURES / "dxf" / "features.dxf").read_bytes()
    data = data.replace("揃え00".encode("cp932"), "揃え99".encode("cp932"))
    start = data.index(b"\r\n0\r\nLINE\r\n5\r\n101\r\n")
    end = data.index(b"\r\n0\r\n", start + 5)
    return data[:start] + data[end:]


def test_compares_two_revisions_by_shape_and_text():
    before = zumen.read(FIXTURES / "dxf" / "features.dxf")
    same = zumen.compare(before, zumen.read(FIXTURES / "dxf" / "features.dxf"))
    assert (len(same), same.unchanged, same.regions) == (0, len(before.entities()), [])

    after = zumen.read(revised_features())
    comparison = zumen.compare(before, after)
    assert [entity["id"] for entity in comparison.removed] == ["101"]
    assert comparison.added == []
    [pair] = comparison.changed
    assert (pair["before"]["text"], pair["after"]["text"]) == ("揃え00", "揃え99")
    assert comparison.unchanged == len(before.entities()) - 2
    assert comparison.offset == (0.0, 0.0)
    assert sum(region["removed"] + region["added"] + region["changed"] for region in comparison.regions) == 2
    assert repr(comparison).startswith("<zumen.Comparison unchanged")

    overlay = comparison.scene
    assert overlay is comparison.scene or overlay.to_json() == comparison.scene.to_json()
    assert (overlay.format, [layer["name"] for layer in overlay.layers]) == (
        "diff",
        ["変更なし", "削除・変更前", "追加・変更後"],
    )
    removed = overlay.entity("A:101")
    assert removed is not None and ("比較", "削除") in removed["properties"]
    assert overlay.render_png(dpi=20).startswith(b"\x89PNG")
    with pytest.raises(ValueError):
        zumen.compare(before, after, tolerance=0)
