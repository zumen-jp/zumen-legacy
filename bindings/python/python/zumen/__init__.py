"""zumen: read 2D drawings of Japanese construction and render them.

Drawings are detected from their contents, not their file names. The result is a
format-neutral ``Scene`` in paper millimeters (Y up), the same model as the
``@zumenjs/core`` package in the browser, so a scene can be sent to the viewer as JSON.

>>> import zumen
>>> scene = zumen.read("plan.jww")
>>> png = scene.render_png(dpi=150)
>>> texts = scene.texts()  # the texts with their positions, in paper millimeters
>>> image = scene.render(size=(2000, 2000))  # a PNG and the pixel box of every entity drawn
>>> zumen.convert("plan.jww", "dxf").save("plan.dxf")  # every format can be written as DXF
"""

from __future__ import annotations

import os

from ._zumen import Converted, ReadError, RenderError, Rendering, Scene, __version__, detect
from ._zumen import convert as _convert
from ._zumen import read as _read

__all__ = ["Converted", "ReadError", "RenderError", "Rendering", "Scene", "__version__", "convert", "detect", "read"]


def _load(source: bytes | bytearray | memoryview | str | os.PathLike[str]) -> bytes:
    if isinstance(source, (str, os.PathLike)):
        with open(source, "rb") as file:
            return file.read()
    return bytes(source)


def read(
    source: bytes | bytearray | memoryview | str | os.PathLike[str],
    *,
    format: str | None = None,
    view: str | None = None,
) -> Scene:
    """Read a drawing from bytes or a file path into a ``Scene``.

    ``format`` forces a format (``"sfc"``, ``"p21"``, ``"jww"`` or ``"dxf"``) instead of detecting it from the contents.
    ``view`` chooses a view such as a DXF layout (see ``Scene.views``).
    Raises ``ReadError`` when the drawing cannot be read.
    """
    return _read(_load(source), format=format, view=view)


def convert(
    source: bytes | bytearray | memoryview | str | os.PathLike[str],
    to: str = "dxf",
    *,
    format: str | None = None,
) -> Converted:
    """Read a drawing from bytes or a file path and write it in another format.

    Drawings of every format can be written as DXF (``to="dxf"``: ASCII, AutoCAD 2013, UTF-8) so far. SXF and JWW
    drawings are mapped into the AutoCAD drawing model: model space at real size, and the sheet as a layout whose
    viewports show each scale where the drawing had it.
    ``Converted.data`` holds the bytes and ``Converted.save(path)`` writes them to a file.
    ``Converted.notices`` tells what could not be written as it was, such as images (IMAGE) that are left out.
    Raises ``ReadError`` when the drawing cannot be read or written in that format.
    """
    return _convert(_load(source), to, format=format)
