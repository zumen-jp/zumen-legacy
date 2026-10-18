"""zumen: read 2D drawings of Japanese construction and render them.

Drawings are detected from their contents, not their file names. The result is a
format-neutral ``Scene`` in paper millimeters (Y up), the same model as the
``@zumenjs/core`` package in the browser, so a scene can be sent to the viewer as JSON.

>>> import zumen
>>> scene = zumen.read("plan.jww")
>>> png = scene.render_png(dpi=150)
"""

from __future__ import annotations

import os

from ._zumen import ReadError, RenderError, Scene, __version__, detect
from ._zumen import read as _read

__all__ = ["ReadError", "RenderError", "Scene", "__version__", "detect", "read"]


def read(
    source: bytes | bytearray | memoryview | str | os.PathLike[str],
    *,
    format: str | None = None,
    view: str | None = None,
) -> Scene:
    """Read a drawing from bytes or a file path into a ``Scene``.

    ``format`` forces a format (for example ``"jww"``) instead of detecting it from the contents.
    ``view`` chooses a view such as a DXF layout (see ``Scene.views``).
    Raises ``ReadError`` when the drawing cannot be read.
    """
    if isinstance(source, (str, os.PathLike)):
        with open(source, "rb") as file:
            data = file.read()
    else:
        data = bytes(source)
    return _read(data, format=format, view=view)
