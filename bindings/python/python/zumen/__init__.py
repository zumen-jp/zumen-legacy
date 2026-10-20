"""zumen: read 2D drawings of Japanese construction and render them.

Drawings are detected from their contents, not their file names. The result is a
format-neutral ``Scene`` in paper millimeters (Y up), the same model as the
``@zumenjs/core`` package in the browser, so a scene can be sent to the viewer as JSON.

>>> import zumen
>>> scene = zumen.read("plan.jww")
>>> png = scene.render_png(dpi=150)
>>> texts = scene.texts()  # the texts with their positions, in paper millimeters
>>> image = scene.render(size=(2000, 2000))  # a PNG and the pixel box of every entity drawn
>>> changes = zumen.compare(zumen.read("plan-rev1.jww"), scene)  # removed, added and changed entities
>>> zumen.convert("plan.jww", "dxf").save("plan.dxf")  # every format can be written as DXF
>>> zumen.edit("plan.dxf", [{"op": "add", "kind": "line", "start": [0, 0], "end": [1000, 0]}]).save("plan.dxf")
"""

from __future__ import annotations

import os

from ._zumen import (
    Comparison,
    Converted,
    Edited,
    EditError,
    LimitError,
    ReadError,
    RenderError,
    Rendering,
    Scene,
    __version__,
    compare,
    create,
    detect,
)
from ._zumen import convert as _convert
from ._zumen import edit as _edit
from ._zumen import read as _read

__all__ = [
    "Comparison",
    "Converted",
    "EditError",
    "Edited",
    "LimitError",
    "ReadError",
    "RenderError",
    "Rendering",
    "Scene",
    "__version__",
    "compare",
    "convert",
    "create",
    "detect",
    "edit",
    "read",
]


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
    max_bytes: int | None = None,
    max_entities: int | None = None,
    timeout: float | None = None,
) -> Scene:
    """Read a drawing from bytes or a file path into a ``Scene``.

    ``format`` forces a format (``"sfc"``, ``"p21"``, ``"p2z"``, ``"sfz"``, ``"jww"``, ``"dxf"`` or ``"dwg"``) instead
    of detecting it from the contents. ``view`` chooses a view such as a DXF layout (see ``Scene.views``).
    Raises ``ReadError`` when the drawing cannot be read.

    For drawings received from others, ``max_bytes`` (the size after unpacking P2Z and SFZ), ``max_entities`` and
    ``timeout`` (seconds) stop reading part way and raise ``LimitError``, whose ``limit`` is ``"bytes"``,
    ``"entities"`` or ``"time"``. A file larger than ``max_bytes`` is not read into memory.
    """
    if isinstance(source, (str, os.PathLike)):
        if max_bytes is not None and os.path.getsize(source) > max_bytes:
            error = LimitError(f"図面が上限の {max_bytes:,} バイトより大きいため、読みませんでした")
            error.limit = "bytes"
            raise error
        with open(source, "rb") as file:
            data = file.read()
    else:
        data = bytes(source)
    return _read(data, format=format, view=view, max_bytes=max_bytes, max_entities=max_entities, timeout=timeout)


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


def edit(
    source: bytes | bytearray | memoryview | str | os.PathLike[str],
    operations: object,
    *,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Edited:
    """Edit a drawing in place: apply ``operations`` and keep everything else as it was.

    ``operations`` is a list of dicts (or the same as a JSON string): ``{"op": "add", "kind": "line", ...}``,
    ``{"op": "update", "id": ..., "set": {...}}``, ``{"op": "delete", "id": ...}``, ``{"op": "add-layer", "name":
    ...}`` and ``{"op": "update-layer", "name": ..., "set": {...}}``. Kinds and keys are the ones that ``Scene``
    reports (``kind`` and the property keys), and coordinates are the drawing's own (the ``raw`` of properties), not
    paper millimeters. The format, its version, its text encoding and everything that no operation names stay as they
    were, including what zumen cannot read. DXF can be edited so far.

    ``Edited.data`` holds the bytes, ``Edited.created`` the ids of the entities and the names of the layers added (in
    the order of the operations), and ``Edited.notices`` what was written differently, such as a color that the format
    cannot hold. If any operation cannot be applied, nothing is written and ``EditError`` is raised, with ``code``
    (``"not-found"``, ``"unknown-key"`` and so on) and ``operation``, the number of the operation (from 0).
    """
    return _edit(_load(source), operations, format=format, max_bytes=max_bytes)
