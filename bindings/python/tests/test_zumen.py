"""Smallest checks that the bindings pass data through correctly. The readers are tested in Rust."""

import json
import pathlib

import pytest
import zumen

FIXTURES = pathlib.Path(__file__).parents[3] / "tests" / "fixtures"


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
    assert json.loads(scene.to_json()) == golden


def test_renders_png_and_reports_unreadable_input():
    scene = zumen.read((FIXTURES / "jww" / "minimal-v600.jww").read_bytes())
    png = scene.render_png(dpi=50)
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    assert zumen.detect(b"not a drawing") is None
    with pytest.raises(zumen.ReadError):
        zumen.read(b"not a drawing")
    with pytest.raises(zumen.RenderError):
        scene.render_png(dpi=0)
