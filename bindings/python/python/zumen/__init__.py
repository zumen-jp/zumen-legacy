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
>>> zumen.draft("sheets.json", "dwg").save("sheets.dwg")  # draw sheets from a drawing spec (docs/drafting.md)
"""

from __future__ import annotations

import json
import os

from ._zumen import (
    Comparison,
    Converted,
    DraftError,
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
from ._zumen import draft as _draft
from ._zumen import draft_into as _draft_into
from ._zumen import draft_schema as _draft_schema
from ._zumen import edit as _edit
from ._zumen import read as _read

__all__ = [
    "Comparison",
    "Converted",
    "DraftError",
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
    "draft",
    "draft_into",
    "draft_schema",
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

    ``format`` forces a format (``"sfc"``, ``"p21"``, ``"p2z"``, ``"sfz"``, ``"jww"``, ``"jwc"``, ``"jws"``, ``"dxf"``,
    ``"dwg"`` or ``"pdf"``) instead of detecting it from the contents. ``view`` chooses a view such as a DXF layout or a
    PDF page (``"1"``, ``"2"`` …; see ``Scene.views``). The paths, texts and images of a PDF page are entities that are
    not drawn (``drawn`` is false): the page itself is the underlay that shows them.
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
    version: str | None = None,
) -> Converted:
    """Read a drawing from bytes or a file path and write it in another format.

    Drawings of every format can be written as DXF (``to="dxf"``: ASCII, AutoCAD 2013, UTF-8), DWG (``"dwg"``:
    AutoCAD 2013), JWW (``"jww"``: Jw_cad 8) and SXF (``"sfc"``, ``"p21"``, ``"sfz"`` or ``"p2z"``: SXF Ver.3.1),
    through the AutoCAD drawing model. SXF and JWW drawings are mapped into
    it with model space at real size, and the sheet as a layout whose viewports show each scale where the drawing had
    it. Written as SFC, the layout becomes the sheet, each viewport a sub-figure at its scale and each block a drawing
    part; what SXF has no feature for (multiline texts, leaders, dimensions) is written as the lines, fills and texts
    that zumen draws for it. Written as JWW, the layout becomes the sheet and the model space in each viewport a layer
    group at the viewport's scale.
    ``version`` chooses the SXF version (``"2.0"``, ``"3.0"`` or ``"3.1"``, the default) or the DXF version: ``"R12"``, ``"2000"``, ``"2004"``, ``"2007"``, ``"2010"``, ``"2013"`` (the
    default) or ``"2018"``. Versions before 2007 are written in the drawing's code page (``$DWGCODEPAGE``), or in the one
    that holds its texts (such as ANSI_932 for Japanese), and what a version cannot hold (true colors, transparency,
    gradients and arc and large radial dimensions before 2004, backgrounds of multiline text before 2007) is written in
    another form and reported in the notices. R12 also explodes what it lacks (multiline text, hatches, leaders,
    ellipses, splines) into lines, solids and one-line texts, keeps only the current layout and has no length unit.
    ``Converted.data`` holds the bytes and ``Converted.save(path)`` writes them to a file.
    ``Converted.notices`` tells what could not be written as it was, such as images (IMAGE) that are left out.
    Raises ``ReadError`` when the drawing cannot be read or written in that format.
    """
    return _convert(_load(source), to, format=format, version=version)


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
    ...}``, ``{"op": "update-layer", "name": ..., "set": {...}}`` and ``{"op": "add-block", "name": ...,
    "basePoint": [x, y]}`` (an empty block definition that later operations fill with ``"container":
    "block:<name>"``; a drawing part in SXF). Every format takes ``"kind": "dimension"`` (``dimensionType``
    ``"linear"`` or ``"aligned"`` with ``start``, ``end``, ``position`` and ``rotation``, ``"angular-3-point"`` with
    ``center``, ``start``, ``end`` and ``position`` (not in JWW), ``"radius"`` or ``"diameter"`` with ``center`` and
    ``point``; ``text``, ``dimensionStyle``) and ``"kind": "hatch"`` (``boundaries``, ``pattern``, ``patternScale``,
    ``patternAngle``, ``patternLines``; JWW writes solids and hatch lines, and ``created`` names the first), and the
    ops ``add-text-style``, ``add-line-type`` and ``add-dimension-style`` (in JWW and SXF, which have no named
    dimension or text styles, their names last for the edit: a dimension style sizes and formats the dimensions that
    name it, a text style is a font, and a line type keeps its name). DXF and DWG ``text`` takes
    ``justification``, ``lineSpacing``,
    ``multiline`` and ``widthFactor``; ``"kind": "attribute-definition"`` goes in a block and ``block-reference`` takes ``attributes``
    (``{"TAG": "value"}``); ``add-layout`` (``paperSize`` and ``margins`` in mm) and ``"kind": "viewport"`` in ``"container":
    "layout:<name>"`` (``center``, ``width``, ``height``, ``viewCenter``, ``scale``) make sheets;
    ``update-layout`` (``name``, ``set: {"paperSize": [width, height]}`` in mm) resizes an existing sheet.
    ``"kind": "leader"`` (``vertices``, ``text``, ``height``,
    ``dimensionStyle``) adds a leader with its note; ``"kind": "image"`` (``path``, ``pixels``,
    ``position``, ``width``, ``rotation``) adds a raster image that refers to ``path``. Kinds and keys are the ones that ``Scene``
    reports (``kind`` and the property keys), and coordinates are the drawing's own (the ``raw`` of properties), not
    paper millimeters. The format, its version, its text encoding and everything that no operation names stay as they
    were, including what zumen cannot read. DXF, DWG, JWW and SXF can be edited.

    ``Edited.data`` holds the bytes, ``Edited.created`` the ids of the entities and the names of the layers and blocks
    added (in the order of the operations; an entity added into a JWW block is ``block:<name>>#<n>``, its place in the
    block), and ``Edited.notices`` what was written differently, such as a color that the format
    cannot hold. If any operation cannot be applied, nothing is written and ``EditError`` is raised, with ``code``
    (``"not-found"``, ``"unknown-key"`` and so on) and ``operation``, the number of the operation (from 0).
    """
    return _edit(_load(source), operations, format=format, max_bytes=max_bytes)


def draft(
    spec: object,
    to: str = "dxf",
    *,
    version: str | None = None,
    sheet: str | None = None,
) -> Converted:
    """Draw a drawing from a drawing spec and write it as ``to`` (dxf, dwg, jww, sfc, p21, sfz or p2z).

    ``spec`` is a dict (the JSON document of docs/drafting.md), a JSON string, or the path of a JSON file.
    ``version`` is the version to write (DXF R12 to 2018, SXF 2.0 to 3.1). JWW and SXF hold one sheet, so
    ``sheet`` names the sheet to write (the first one by default). A mistake in the spec raises
    ``DraftError`` with ``code`` and ``path`` (such as ``sheets[0].views[1].items[4]``).
    """
    return _draft(_spec(spec), to, version=version, sheet=sheet)


def draft_into(
    spec: object,
    template: bytes | bytearray | memoryview | str | os.PathLike[str],
    *,
    sheet: str | None = None,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Converted:
    """Draw a drawing spec into a DXF or DWG drawing (a template) in place.

    The drawing is changed through the operations of ``edit``, so its entities, handles, extended data and what zumen
    cannot read stay, and the format and version stay too. A sheet named after a layout of the template is drawn into
    that layout, with its paper size when the sheet gives none; other sheets become new layouts. The ``fields`` of a
    sheet (``{"TAG": "value"}``) fill the attributes of the block references on it, such as the template's title
    block. ``template`` is bytes or a file path, ``sheet`` draws one sheet only. A mistake in the spec, or an operation
    that the drawing refuses, raises ``DraftError`` with ``code`` and ``path``.
    """
    return _draft_into(_spec(spec), _load(template), sheet=sheet, format=format, max_bytes=max_bytes)


def _spec(spec: object) -> object:
    """The spec as a dict or JSON string. A path (or a string that is not JSON) is read from the file."""
    if isinstance(spec, os.PathLike) or (isinstance(spec, str) and not spec.lstrip().startswith("{")):
        with open(spec, encoding="utf-8") as file:
            return file.read()
    return spec


def draft_schema() -> dict[str, object]:
    """The JSON Schema (draft 2020-12) of the drawing spec that ``draft`` takes."""
    return json.loads(_draft_schema())
