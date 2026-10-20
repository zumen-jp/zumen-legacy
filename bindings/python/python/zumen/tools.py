"""Functions for applications that give an AI agent tools to read drawings, with or without MCP.

Each function takes a ``Scene`` (``zumen.read``) and returns plain data that ``json.dumps`` writes as it is: dicts,
lists, strings, numbers, booleans and ``None``. The keys are described by the ``TypedDict`` classes of this module and
are part of the API: a later version may add keys, but does not remove or rename them or change what they mean. The
functions only read and report what the drawing holds; interpreting it is left to the agent. ``zumen.mcp`` serves
the same functions as MCP tools.

Coordinates
    Paper millimeters with the Y axis up, as ``Scene`` has them: boxes are ``[min_x, min_y, max_x, max_y]``, points
    ``[x, y]``, rounded to 0.001. The *frame* of a drawing is its ``bounds``, the box around the sheet and every entity;
    a render without a region shows the frame. A region can also be given as fractions of the frame,
    ``frame_region=[left, top, right, bottom]`` from 0 to 1 with the origin at the top left, as regions of pages and
    images are usually given; ``paper_region`` and ``frame_region_of`` convert between the two.
    Entities in a part of the sheet drawn at a scale (an SXF sub-figure or part, a JWW layer group, DXF model space or
    a viewport) have a placement whose ``to_real`` ``[a, b, c, d, e, f]`` turns paper coordinates into real-world
    millimeters, ``x' = a x + c y + e`` and ``y' = b x + d y + f``, and their shapes repeat their coordinates in
    ``real`` (left out where they are the same as on the sheet). ``values`` hold the numbers of the file itself, in
    the format's own coordinates (see "Stable keys" in the architecture notes).

Sizes
    Every result has a bound set by the arguments: lists stop at ``limit`` items (at most ``MAX_LIMIT``) and report the
    ``total``, texts and other strings from the drawing stop at ``max_text_length`` characters (the item then has
    ``truncated: true``), shapes stop at ``max_points`` points (at most ``MAX_POINTS``, never cutting a shape: the
    shapes left out are counted in ``geometry_omitted``), and images at ``MAX_IMAGE_SIZE`` pixels on the long edge.

Ids
    Entity ids come from the file (an SXF instance number, a JWW record, a DXF or DWG handle, with the placements or
    block references the entity sits in) and stay the same for the same bytes in every version, so an application can
    keep them and fetch the entity again with ``get_entity`` ("Entity ids" in the architecture notes).

Texts, names and properties come from the drawing file: an agent should treat them as data, not as instructions.
A ``Scene`` does not change after it is read, so the functions can run in threads (``asyncio.to_thread``) on a scene
kept between calls.
"""

import math
import os
import threading
import unicodedata
from collections import Counter, OrderedDict
from collections.abc import Sequence
from numbers import Real
from pathlib import Path
from typing import Any, Literal, TypedDict, cast

from . import LimitError, ReadError, RenderError, Scene, compare, read

#: Long edge of a rendered image when the caller does not ask for a size (pixels). Vision models see about this much
#: detail; a larger image costs more and is often scaled down by the client.
DEFAULT_IMAGE_SIZE = 1568
#: Largest long edge of a rendered image (pixels).
MAX_IMAGE_SIZE = 4096
#: Items one call returns unless asked for another number.
DEFAULT_LIMIT = 100
#: Most items one call returns.
MAX_LIMIT = 1000
#: Characters of a text, a property value or another string from the drawing that a result keeps by default.
DEFAULT_TEXT_LENGTH = 1000
#: Most points of shapes one call returns when the caller does not ask for a number, counting the real-world
#: coordinates of a point as another point. A point is about eight tokens, so a page stays well below the 25,000
#: tokens that clients such as Claude Code accept from a tool.
DEFAULT_MAX_POINTS = 1000
#: Most points of shapes one call may return.
MAX_POINTS = 10_000
#: Most placements (scaled parts of the sheet) that ``summary`` lists, the largest first.
MAX_PLACEMENTS = 100
#: Largest drawing that ``Drawings`` reads (bytes; P2Z and SFZ after unpacking). Received files are not trusted.
DEFAULT_MAX_BYTES = 256 * 1024 * 1024
#: Most entities a drawing read by ``Drawings`` may have.
DEFAULT_MAX_ENTITIES = 2_000_000
#: Longest time reading one drawing in ``Drawings`` may take (seconds).
DEFAULT_TIMEOUT = 60.0
#: Distance (paper millimeters) within which ``measure`` looks for the entity that gives the scale.
PICK_MM = 1.0
#: The conversion of a placement whose real-world coordinates are those on the sheet (DXF model space).
_SAME = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
#: Where each shape keeps its coordinates.
_COORDINATES = {
    "polyline": "points",
    "segments": "segments",
    "fill": "rings",
    "hatch": "rings",
    "point": "point",
    "text": "point",
}

Box = tuple[float, float, float, float]
#: An unformatted property value: a number, a string, a boolean or a list of numbers (``[x, y]``, ``[from, to]`` …).
Value = float | str | bool | list[float] | None


class DrawingError(Exception):
    """A request that cannot be served (a path outside the roots, a missing entity, an unreadable drawing, a bad
    argument). The message says why, in English, and can be shown to the agent."""


# --- The schema of the results ----------------------------------------------------------------------------------


class ViewInfo(TypedDict):
    """A view of the drawing, such as DXF model space or a layout. ``id`` is the ``view`` to pass to ``read``."""

    id: str
    name: str


class SheetInfo(TypedDict):
    """The sheet. ``size`` is its name (A1, A3 縦 …); ``width`` and ``height`` are millimeters."""

    name: str
    size: str
    width: float
    height: float


class LayerInfo(TypedDict):
    """A layer, with the number of entities on it (also when the drawing hides it)."""

    id: int
    name: str
    visible: bool
    group: str | None
    entities: int


class PlacementInfo(TypedDict):
    """The part of the sheet drawn at a scale that an entity sits in. ``scale_x`` and ``scale_y`` are paper
    millimeters per real-world millimeter, ``scale`` the same as ``1:100`` when they are equal, and ``to_real``
    ``[a, b, c, d, e, f]`` turns paper coordinates into real-world millimeters: ``x' = a x + c y + e``,
    ``y' = b x + d y + f``."""

    name: str
    scale_x: float
    scale_y: float
    scale: str | None
    to_real: list[float]


class PlacedPartInfo(PlacementInfo):
    """A placement of ``summary``, with the number of its entities and the box around them (paper millimeters)."""

    entities: int
    bbox: list[float]


class NoticeInfo(TypedDict):
    """Content that was not drawn or was drawn differently: a stable ``code``, a Japanese ``message`` and the count."""

    level: Literal["info", "warning"]
    code: str
    message: str
    count: int


class PropertyInfo(TypedDict):
    """A property as people read it: the stable ``key``, the Japanese ``label`` and the formatted ``value``."""

    key: str
    label: str
    value: str


class TitleBlockInfo(TypedDict):
    """Fields guessed from the title block (labels such as 図面番号, attribute tags); ``None`` when not found."""

    number: str | None
    revision: str | None
    title: str | None
    project: str | None
    scale: str | None
    date: str | None


class Summary(TypedDict):
    """What an agent needs to plan its next calls (``summary``).

    ``unit`` is the length unit of the drawing's own coordinates, those of ``values`` (``millimeter`` in SXF and JWW,
    ``$INSUNITS`` in DXF and DWG: ``inch``, ``meter`` …), ``None`` when unknown. ``bounds`` is the frame, ``None`` for a
    drawing without entities. ``types`` counts entities by their Japanese type
    and ``kinds`` by their stable kind (``line``, ``text``, ``dimension`` …); both include the layers that the drawing
    hides, and ``layers`` tells which are visible. ``layers`` stops at ``layer_limit`` of ``layers_total``, and
    ``placements`` at ``MAX_PLACEMENTS`` of ``placements_total``, the largest first. ``metadata_values`` maps each
    metadata key to its unformatted value."""

    format: str
    version: str
    unit: str | None
    view: str | None
    views: list[ViewInfo] | None
    sheet: SheetInfo | None
    bounds: list[float] | None
    background: Literal["dark", "light"]
    entities: int
    types: dict[str, int]
    kinds: dict[str, int]
    texts: int
    layers_total: int
    layers: list[LayerInfo]
    placements: list[PlacedPartInfo]
    placements_total: int
    notices: list[NoticeInfo]
    metadata: list[PropertyInfo]
    metadata_values: dict[str, Value]
    title_block: TitleBlockInfo


class _DrawnTextInfo(TypedDict):
    id: str
    kind: str
    type: str
    text: str
    box: list[float]


class DrawnTextInfo(_DrawnTextInfo, total=False):
    """A text drawn in an image, with its box in pixels ``[left, top, right, bottom]`` (origin at the top left)."""

    truncated: bool


class RenderInfo(TypedDict):
    """How the pixels of an image map to the drawing (``render``).

    ``region`` is the part of the sheet shown (paper millimeters) and ``frame_region`` the same as fractions of the
    frame (``None`` for a drawing without a frame); ``scale`` is pixels per paper millimeter. ``texts`` stops at
    ``text_limit`` of ``texts_drawn``."""

    width: int
    height: int
    region: list[float]
    frame_region: list[float] | None
    scale: float
    pixels: str
    entities_drawn: int
    texts_drawn: int
    texts: list[DrawnTextInfo]


class _TextInfo(TypedDict):
    id: str
    kind: str
    type: str
    text: str
    x: float
    y: float
    height: float
    angle: float
    bbox: list[float]
    layers: list[str]


class TextInfo(_TextInfo, total=False):
    """A text: lines joined with a line break, ``x`` and ``y`` the lower left of its first part, ``height`` the text
    height and ``bbox`` its box in paper millimeters, ``angle`` its direction in degrees counterclockwise."""

    truncated: bool


class TextPage(TypedDict):
    """A page of texts in drawing order (``find_texts``); ``next_offset`` is the ``offset`` of the next page."""

    total: int
    offset: int
    next_offset: int | None
    texts: list[TextInfo]


class _PolylineShape(TypedDict):
    shape: Literal["polyline"]
    points: list[list[float]]
    closed: bool


class PolylineShape(_PolylineShape, total=False):
    """Lines through ``points``; ``closed`` joins the last point to the first. Arcs, circles, ellipses and splines come
    as the polyline drawn; their center and radius are in ``values``."""

    real: list[list[float]]


class _SegmentsShape(TypedDict):
    shape: Literal["segments"]
    segments: list[list[list[float]]]


class SegmentsShape(_SegmentsShape, total=False):
    """Separate segments, each a pair of points (the edges of a mesh, for example)."""

    real: list[list[list[float]]]


class _AreaShape(TypedDict):
    shape: Literal["fill", "hatch"]
    rings: list[list[list[float]]]


class AreaShape(_AreaShape, total=False):
    """A filled or hatched area: an outline and its holes (even-odd rule). The lines of a hatch are its pattern and are
    left out."""

    real: list[list[list[float]]]


class _PointShape(TypedDict):
    shape: Literal["point"]
    point: list[float]


class PointShape(_PointShape, total=False):
    """A point."""

    real: list[float]


class _TextShape(TypedDict):
    shape: Literal["text"]
    text: str
    point: list[float]
    height: float
    angle: float


class TextShape(_TextShape, total=False):
    """A part of a text on one line: ``point`` its lower left, ``height`` the text height on the sheet, ``angle`` its
    direction in degrees counterclockwise."""

    real: list[float]
    truncated: bool


Shape = PolylineShape | SegmentsShape | AreaShape | PointShape | TextShape


class PlacementName(TypedDict):
    """The name and scale of an entity's placement, as ``find_entities`` gives them with ``geometry``."""

    name: str
    scale: str | None


class _EntityInfo(TypedDict):
    id: str
    kind: str
    type: str
    layers: list[str]
    bbox: list[float]


class EntityInfo(_EntityInfo, total=False):
    """An entity in a list: its text when it has one, and with ``geometry`` its shapes and placement."""

    text: str
    truncated: bool
    placement: PlacementName | None
    geometry: list[Shape]
    geometry_omitted: int


class EntityPage(TypedDict):
    """A page of entities in drawing order (``find_entities``), with the counts of every entity found by Japanese
    type and by kind. With ``geometry`` a page can end before ``limit``; ``next_offset`` is where the next one
    starts."""

    total: int
    offset: int
    next_offset: int | None
    types: dict[str, int]
    kinds: dict[str, int]
    entities: list[EntityInfo]


class _EntityDetail(TypedDict):
    id: str
    kind: str
    type: str
    source: str
    layers: list[str]
    bbox: list[float]
    text: str | None
    properties: list[PropertyInfo]
    values: dict[str, Value]
    placement: PlacementInfo | None
    geometry: list[Shape]


class EntityDetail(_EntityDetail, total=False):
    """One entity (``get_entity``). ``source`` is its name in the format (``LINE``, ``line_feature``, ``CDataSen``).
    ``properties`` are what the format gives, as the viewer's property panel shows them, and ``values`` maps each of
    their keys to the unformatted value (numbers, ``[x, y]`` in the format's own coordinates), or to the formatted
    text when there is none. ``geometry_omitted`` counts the shapes past ``max_points``."""

    geometry_omitted: int
    truncated: bool


class _MeasurementInfo(TypedDict):
    paper: float
    real: float | None
    placement: PlacementInfo | None
    entities: list[str | None]


class MeasurementInfo(_MeasurementInfo, total=False):
    """A distance (``measure``): ``paper`` millimeters on the sheet, and ``real`` millimeters through the scale of the
    placement the points sit in, with the two points in real-world millimeters. ``entities`` are the entities that gave
    the scale; ``reason`` tells why there is no real length."""

    start_real: list[float]
    end_real: list[float]
    reason: str


class ChangeRegionInfo(TypedDict):
    """Nearby changes grouped into a box (paper millimeters of the earlier revision)."""

    bbox: list[float]
    removed: int
    added: int
    changed: int


class _ChangeInfo(TypedDict):
    change: Literal["removed", "added", "changed"]
    kind: str
    type: str
    bbox: list[float]


class ChangeInfo(_ChangeInfo, total=False):
    """A removed or added entity (``id``, ``text``), or a changed one (``before_id``, ``after_id``, ``before_text``,
    ``after_text``). Boxes are in the earlier revision's paper millimeters."""

    id: str
    text: str | None
    before_id: str
    after_id: str
    before_text: str | None
    after_text: str | None
    truncated: bool


class ComparisonImageInfo(TypedDict):
    """How the pixels of the comparison image map to the earlier revision's paper millimeters."""

    width: int
    height: int
    region: list[float]
    frame_region: list[float] | None
    scale: float
    pixels: str


class ComparisonInfo(TypedDict):
    """Two revisions compared (``compare_revisions``). ``offset`` is what was taken off the later revision's
    coordinates to align it; ``regions`` and ``changes`` stop at ``limit``."""

    unchanged: int
    removed: int
    added: int
    changed: int
    offset: list[float]
    regions: list[ChangeRegionInfo]
    changes_total: int
    changes: list[ChangeInfo]
    image: ComparisonImageInfo
    colors: str


# --- Helpers ----------------------------------------------------------------------------------------------------


def _mm(value: float) -> float:
    """Round paper millimeters to 0.001 for the client. -0.0 becomes 0.0."""
    rounded = round(value, 3)
    return rounded if rounded != 0 else 0.0


def _px(value: float) -> float:
    rounded = round(value, 1)
    return rounded if rounded != 0 else 0.0


def _box(box: Sequence[float]) -> list[float]:
    return [_mm(value) for value in box]


def _number(value: float) -> float:
    """A factor of a conversion, without the noise of floating point (12 significant digits)."""
    return float(f"{value:.12g}") or 0.0


def _fraction(value: float) -> float:
    rounded = round(value, 6)
    return rounded if rounded != 0 else 0.0


def _value(value: Any) -> Value:
    """A property value as JSON: tuples become lists, and numbers that are not finite become ``None``."""
    if isinstance(value, tuple):
        return [item if math.isfinite(item) else None for item in value]
    if isinstance(value, float) and not math.isfinite(value):
        return None
    return value


def _clip(text: str, length: int | None) -> tuple[str, bool]:
    """``text`` cut to ``length`` characters, the last one an ellipsis, and whether it was cut."""
    if length is None or len(text) <= length:
        return text, False
    return text[: length - 1] + "…", True


def _text_length(length: int | None) -> int | None:
    if length is not None and length < 1:
        raise DrawingError("max_text_length must be at least 1")
    return length


def _page(limit: int, offset: int) -> tuple[int, int]:
    if limit < 1 or offset < 0:
        raise DrawingError("limit must be at least 1 and offset at least 0")
    return min(limit, MAX_LIMIT), offset


def _max_points(max_points: int) -> int:
    if not 1 <= max_points <= MAX_POINTS:
        raise DrawingError(f"max_points must be between 1 and {MAX_POINTS:,}")
    return max_points


def normalize(text: str) -> str:
    """Fold text for searching: NFKC (full-width and half-width forms), case, and no white space."""
    return "".join(unicodedata.normalize("NFKC", text).casefold().split())


def _finite(values: Any, count: int) -> bool:
    """Whether ``values`` are ``count`` finite numbers (a list, a tuple or an array)."""
    try:
        return len(values) == count and all(isinstance(value, Real) and math.isfinite(value) for value in values)
    except TypeError:
        return False


def _four(values: Sequence[float] | None, name: str, what: str) -> Box | None:
    if values is None:
        return None
    if not _finite(values, 4):
        raise DrawingError(f"{name} needs four finite numbers {what}")
    x0, y0, x1, y1 = (float(value) for value in values)
    return (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def frame(scene: Scene) -> list[float] | None:
    """The frame of a drawing: its bounds, the box around the sheet and every entity (paper millimeters), or ``None``
    when the drawing has nothing in it."""
    x0, y0, x1, y1 = scene.bounds
    if not (math.isfinite(x0 + y0 + x1 + y1) and x0 <= x1 and y0 <= y1):
        return None
    return [x0, y0, x1, y1]


def _frame(scene: Scene) -> list[float]:
    box = frame(scene)
    if box is None:
        raise DrawingError("the drawing has nothing in it, so it has no frame")
    return box


def paper_region(scene: Scene, frame_region: Sequence[float]) -> list[float]:
    """The region ``[min_x, min_y, max_x, max_y]`` in paper millimeters for fractions of the frame
    ``[left, top, right, bottom]`` (0 to 1, origin at the top left; the order within each pair does not matter)."""
    left, top, right, bottom = cast(Box, _four(frame_region, "frame_region", "[left, top, right, bottom] from 0 to 1"))
    x0, y0, x1, y1 = _frame(scene)
    width, height = x1 - x0, y1 - y0
    return [x0 + left * width, y1 - bottom * height, x0 + right * width, y1 - top * height]


def frame_region_of(scene: Scene, region: Sequence[float]) -> list[float]:
    """The fractions of the frame ``[left, top, right, bottom]`` (origin at the top left) of a region in paper
    millimeters, rounded to 0.000001: the inverse of ``paper_region``."""
    x0, y0, x1, y1 = cast(Box, _four(region, "region", "[min_x, min_y, max_x, max_y] in paper millimeters"))
    fx0, fy0, fx1, fy1 = _frame(scene)
    width, height = fx1 - fx0, fy1 - fy0

    def along(value: float, length: float) -> float:
        return _fraction(value / length) if length > 0 else 0.0

    return [along(x0 - fx0, width), along(fy1 - y1, height), along(x1 - fx0, width), along(fy1 - y0, height)]


def _region(scene: Scene, region: Sequence[float] | None, frame_region: Sequence[float] | None) -> Box | None:
    if region is not None and frame_region is not None:
        raise DrawingError("give region or frame_region, not both")
    if frame_region is not None:
        x0, y0, x1, y1 = paper_region(scene, frame_region)
        return (x0, y0, x1, y1)
    return _four(region, "region", "[min_x, min_y, max_x, max_y] in paper millimeters")


def _point(point: Sequence[float]) -> list[float]:
    return [_mm(point[0]), _mm(point[1])]


def _round_coordinates(key: str, value: Any) -> Any:
    """Round the coordinates of a shape: a list of points, of segments (pairs of points), of rings, or one point."""
    if key == "points":
        return [_point(point) for point in value]
    if key == "segments":
        return [[_point(start), _point(end)] for start, end in value]
    if key == "rings":
        return [[_point(point) for point in ring] for ring in value]
    return _point(value)


def _shape(shape: dict[str, Any], real: bool, length: int | None) -> Shape:
    """A shape of ``Scene.entities(geometry=True)`` rounded for the client, with ``real`` only when asked."""
    key = _COORDINATES[shape["shape"]]
    rounded: dict[str, Any] = {"shape": shape["shape"]}
    cut = False
    if shape["shape"] == "text":
        rounded["text"], cut = _clip(shape["text"], length)
    rounded[key] = _round_coordinates(key, shape[key])
    if shape["shape"] == "polyline":
        rounded["closed"] = shape["closed"]
    if shape["shape"] == "text":
        rounded["height"] = _mm(shape["height"])
        rounded["angle"] = _mm(shape["angle"])
    if real:
        rounded["real"] = _round_coordinates(key, shape["real"])
    if cut:
        rounded["truncated"] = True
    return cast(Shape, rounded)


def _points(shape: dict[str, Any]) -> int:
    """How many points a shape has (a segment has two)."""
    key = _COORDINATES[shape["shape"]]
    if key == "points":
        return len(shape[key])
    if key == "segments":
        return 2 * len(shape[key])
    if key == "rings":
        return sum(len(ring) for ring in shape[key])
    return 1


def _geometry(entity: dict[str, Any], budget: int, length: int | None) -> tuple[list[Shape], int, int]:
    """The shapes of an entity that fit in ``budget`` points, in drawing order, with the points they use and how many
    shapes were left out. Shapes are never cut, so one larger than the budget is left out with all after it.

    ``real`` comes only when it differs from the coordinates on the sheet, and its points count too."""
    placement = entity["placement"]
    real = placement is not None and tuple(placement["inverse"]) != _SAME
    shapes = entity["geometry"]
    kept: list[Shape] = []
    used = 0
    for index, shape in enumerate(shapes):
        count = _points(shape) * (2 if real else 1)
        if used + count > budget:
            return kept, used, len(shapes) - index
        kept.append(_shape(shape, real, length))
        used += count
    return kept, used, 0


def _scale_label(scale: float) -> str | None:
    """1:100 for 0.01 paper millimeters per real millimeter, when the ratio is close to a whole number."""
    if not scale > 0:
        return None
    if scale >= 1:
        ratio = round(scale, 6)
        return f"{ratio:g}:1"
    inverse = 1 / scale
    if abs(inverse - round(inverse)) < 1e-6 * inverse:
        return f"1:{round(inverse)}"
    return f"1:{inverse:.6g}"


def _placement(placement: dict[str, Any]) -> PlacementInfo:
    """A placement of ``Scene.entities``: its name, scale (paper millimeters per real-world millimeter, and as 1:100)
    and ``to_real``, the conversion [a, b, c, d, e, f] from paper to real-world millimeters."""
    scale = placement["scale_x"] if placement["scale_x"] == placement["scale_y"] else None
    return {
        "name": placement["name"],
        "scale_x": placement["scale_x"],
        "scale_y": placement["scale_y"],
        "scale": _scale_label(scale) if scale is not None else None,
        "to_real": [_number(value) for value in placement["inverse"]],
    }


# --- Reading files ----------------------------------------------------------------------------------------------


class Drawings:
    """Reads drawing files from the allowed directories, within limits, and keeps the scenes read last.

    ``roots`` limits the directories (symbolic links are resolved first; empty for any readable file), and
    ``max_bytes``, ``max_entities`` and ``timeout`` limit each drawing. A scene is read again when its file changes.
    Several threads may use one ``Drawings``."""

    def __init__(
        self,
        roots: Sequence[str | os.PathLike[str]] = (),
        *,
        max_bytes: int = DEFAULT_MAX_BYTES,
        max_entities: int | None = DEFAULT_MAX_ENTITIES,
        timeout: float | None = DEFAULT_TIMEOUT,
        cache_size: int = 8,
    ) -> None:
        self.roots = [Path(root).expanduser().resolve() for root in roots]
        self.max_bytes = max_bytes
        self.max_entities = max_entities
        self.timeout = timeout
        self.cache_size = cache_size
        self._scenes: OrderedDict[tuple[Path, int, int, str | None], Scene] = OrderedDict()
        self._lock = threading.Lock()

    def resolve(self, path: str | os.PathLike[str]) -> Path:
        """The real path of a drawing file, after checking the roots and the size."""
        resolved = Path(path).expanduser().resolve()
        if self.roots and not any(resolved.is_relative_to(root) for root in self.roots):
            raise DrawingError(f"{path} is outside the directories this server may read")
        if not resolved.is_file():
            raise DrawingError(f"{path} is not a file")
        size = resolved.stat().st_size
        if size > self.max_bytes:
            raise DrawingError(f"{path} is {size:,} bytes, more than the limit of {self.max_bytes:,} bytes")
        return resolved

    def scene(self, path: str | os.PathLike[str], view: str | None = None) -> Scene:
        """Read a drawing, or return the scene read before if the file has not changed."""
        resolved = self.resolve(path)
        stat = resolved.stat()
        key = (resolved, stat.st_mtime_ns, stat.st_size, view)
        with self._lock:
            scene = self._scenes.get(key)
            if scene is not None:
                self._scenes.move_to_end(key)
                return scene
        # Read outside the lock, so that other drawings are read at the same time (reading releases the GIL)
        try:
            scene = read(
                resolved, view=view, max_bytes=self.max_bytes, max_entities=self.max_entities, timeout=self.timeout
            )
        except LimitError as error:
            raise DrawingError(f"stopped reading {path} at the {error.limit} limit: {error}") from error
        except ReadError as error:
            raise DrawingError(f"cannot read {path}: {error}") from error
        with self._lock:
            self._scenes[key] = scene
            while len(self._scenes) > self.cache_size:
                self._scenes.popitem(last=False)
        return scene


# --- The tools --------------------------------------------------------------------------------------------------


def _placements(entities: list[dict[str, Any]]) -> tuple[list[PlacedPartInfo], int]:
    """The placements of the entities (scaled parts of the sheet), the ones with the most entities first, with how
    many there are. A placement is told apart by its name and its conversion."""
    found: dict[tuple[Any, ...], dict[str, Any]] = {}
    for entity in entities:
        placement = entity["placement"]
        if placement is None:
            continue
        key = (placement["name"], placement["inverse"])
        item = found.get(key)
        x0, y0, x1, y1 = entity["bbox"]
        if item is None:
            found[key] = {**_placement(placement), "entities": 1, "bbox": [x0, y0, x1, y1]}
            continue
        item["entities"] += 1
        box = item["bbox"]
        box[0], box[1], box[2], box[3] = min(box[0], x0), min(box[1], y0), max(box[2], x1), max(box[3], y1)
    placements = sorted(found.values(), key=lambda item: -item["entities"])[:MAX_PLACEMENTS]
    for item in placements:
        item["bbox"] = _box(item["bbox"])
    return cast(list[PlacedPartInfo], placements), len(found)


def summary(
    scene: Scene, *, layer_limit: int = MAX_LIMIT, max_text_length: int | None = DEFAULT_TEXT_LENGTH
) -> Summary:
    """What an agent needs to plan: the format, sheet, frame, views, layers, entity types and kinds, notices, metadata,
    the fields of the title block (drawing number, revision, title, project, scale and date, guessed from the texts),
    and the placements: the parts of the sheet drawn at a scale (sub-figures, layer groups, viewports), with the
    conversion from paper to real-world millimeters.

    The counts include the layers that the drawing hides; ``layers`` tells which ones are visible."""
    if layer_limit < 0:
        raise DrawingError("layer_limit must be at least 0")
    length = _text_length(max_text_length)
    entities = scene.entities(hidden_layers=[])
    types = Counter(entity["type"] for entity in entities)
    kinds = Counter(entity["kind"] for entity in entities)
    per_layer: Counter[str] = Counter()
    for entity in entities:
        per_layer.update(set(entity["layers"]))
    placements, placements_total = _placements(entities)
    layers = scene.layers
    sheet = scene.sheet
    bounds = frame(scene)
    block = scene.title_block()
    return {
        "format": scene.format,
        "version": scene.version,
        "unit": scene.unit,
        "view": scene.view,
        "views": [{"id": id, "name": name} for id, name in scene.views] if scene.views else None,
        "sheet": {
            "name": str(sheet["name"]),
            "size": str(sheet["size"]),
            "width": float(sheet["width"]),
            "height": float(sheet["height"]),
        }
        if sheet
        else None,
        "bounds": _box(bounds) if bounds else None,
        "background": scene.background,
        "entities": len(entities),
        "types": dict(types.most_common()),
        "kinds": dict(kinds.most_common()),
        "texts": len(scene.texts(hidden_layers=[])),
        "layers_total": len(layers),
        "layers": [
            {
                "id": int(layer["id"]),
                "name": str(layer["name"]),
                "visible": bool(layer["visible"]),
                "group": None if layer["group"] is None else str(layer["group"]),
                "entities": per_layer[str(layer["name"])],
            }
            for layer in layers[:layer_limit]
        ],
        "placements": placements,
        "placements_total": placements_total,
        "notices": [
            {
                "level": "warning" if notice["level"] == "warning" else "info",
                "code": str(notice["code"]),
                "message": str(notice["message"]),
                "count": int(notice["count"]),
            }
            for notice in scene.notices
        ],
        "metadata": [
            {"key": key, "label": label, "value": _clip(value, length)[0]}
            for key, (label, value) in zip(scene.metadata_values, scene.metadata, strict=True)
        ],
        "metadata_values": {
            key: _clip(value, length)[0] if isinstance(value, str) else _value(value)
            for key, value in scene.metadata_values.items()
        },
        "title_block": {
            "number": block["number"],
            "revision": block["revision"],
            "title": block["title"],
            "project": block["project"],
            "scale": block["scale"],
            "date": block["date"],
        },
    }


def render(
    scene: Scene,
    *,
    region: Sequence[float] | None = None,
    frame_region: Sequence[float] | None = None,
    size: int = DEFAULT_IMAGE_SIZE,
    all_layers: bool = False,
    theme: str | None = None,
    text_limit: int = 200,
    max_text_length: int | None = DEFAULT_TEXT_LENGTH,
) -> tuple[bytes, RenderInfo]:
    """A PNG of the frame or a region (``region`` in paper millimeters or ``frame_region`` as fractions of the frame),
    ``size`` pixels on the long edge, and how its pixels map to the drawing: the region shown, the scale and the texts
    drawn with their pixel boxes (at most ``text_limit``, itself at most ``MAX_LIMIT``). ``theme`` is ``light`` or
    ``dark``, the drawing's own background when omitted."""
    if not 16 <= size <= MAX_IMAGE_SIZE:
        raise DrawingError(f"size must be between 16 and {MAX_IMAGE_SIZE} pixels")
    if theme not in (None, "light", "dark"):
        raise DrawingError("theme must be light or dark")
    if text_limit < 0:
        raise DrawingError("text_limit must be at least 0")
    length = _text_length(max_text_length)
    box = _region(scene, region, frame_region) or frame(scene)
    if box is None:
        raise DrawingError("the drawing has nothing to render")
    width, height = box[2] - box[0], box[3] - box[1]
    # A hair under size, so that rounding up does not add a pixel to the long edge
    dpi = size * (1 - 1e-12) * 25.4 / max(width, height, 1e-6)
    try:
        image = scene.render(dpi=dpi, region=tuple(box), theme=theme, hidden_layers=[] if all_layers else None)
    except RenderError as error:
        raise DrawingError(f"cannot render: {error}") from error
    drawn = image.entities
    texts = [entity for entity in drawn if entity["text"] is not None]
    shown = image.region
    items: list[DrawnTextInfo] = []
    for entity in texts[: min(text_limit, MAX_LIMIT)]:
        text, cut = _clip(entity["text"], length)
        item: DrawnTextInfo = {
            "id": entity["id"],
            "kind": entity["kind"],
            "type": entity["type"],
            "text": text,
            "box": [_px(value) for value in entity["box"]],
        }
        if cut:
            item["truncated"] = True
        items.append(item)
    info: RenderInfo = {
        "width": image.width,
        "height": image.height,
        "region": _box(shown),
        "frame_region": frame_region_of(scene, shown) if frame(scene) else None,
        "scale": image.scale,
        "pixels": "px = (x - region[0]) * scale, py = (region[3] - y) * scale; x and y in paper millimeters",
        "entities_drawn": len(drawn),
        "texts_drawn": len(texts),
        "texts": items,
    }
    return image.png, info


def find_texts(
    scene: Scene,
    *,
    query: str | None = None,
    region: Sequence[float] | None = None,
    frame_region: Sequence[float] | None = None,
    all_layers: bool = False,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
    max_text_length: int | None = DEFAULT_TEXT_LENGTH,
) -> TextPage:
    """Texts (text, dimension values, attributes, leaders) that contain ``query``, ignoring full-width and half-width
    forms, case and spaces (``normalize``), whose box meets the region, in drawing order. Without a query, all."""
    limit, offset = _page(limit, offset)
    length = _text_length(max_text_length)
    texts = scene.texts(region=_region(scene, region, frame_region), hidden_layers=[] if all_layers else None)
    if query:
        needle = normalize(query)
        texts = [text for text in texts if needle in normalize(text["text"])]
    items: list[TextInfo] = []
    for text in texts[offset : offset + limit]:
        content, cut = _clip(text["text"], length)
        item: TextInfo = {
            "id": text["id"],
            "kind": text["kind"],
            "type": text["type"],
            "text": content,
            "x": _mm(text["x"]),
            "y": _mm(text["y"]),
            "height": _mm(text["height"]),
            "angle": _mm(text["angle"]),
            "bbox": _box(text["bbox"]),
            "layers": text["layers"],
        }
        if cut:
            item["truncated"] = True
        items.append(item)
    following = offset + len(items)
    return {
        "total": len(texts),
        "offset": offset,
        "next_offset": following if following < len(texts) else None,
        "texts": items,
    }


def find_entities(
    scene: Scene,
    *,
    region: Sequence[float] | None = None,
    frame_region: Sequence[float] | None = None,
    type: str | None = None,
    kind: str | None = None,
    layer: str | None = None,
    all_layers: bool = False,
    limit: int = DEFAULT_LIMIT,
    offset: int = 0,
    geometry: bool = False,
    max_points: int = DEFAULT_MAX_POINTS,
    max_text_length: int | None = DEFAULT_TEXT_LENGTH,
) -> EntityPage:
    """Entities whose box meets the region, filtered by their stable ``kind`` (``line``, ``text``, ``dimension`` …),
    a part of their Japanese ``type`` or of a layer name, in drawing order.

    With ``geometry``, each entity also has its shapes, as ``get_entity`` gives them, and the name and scale of its
    placement. A page then ends early when the next entity's shapes would pass ``max_points``; ``next_offset`` tells
    where to go on."""
    limit, offset = _page(limit, offset)
    budget = _max_points(max_points)
    length = _text_length(max_text_length)
    entities = scene.entities(
        region=_region(scene, region, frame_region), hidden_layers=[] if all_layers else None, geometry=geometry
    )
    if kind:
        entities = [entity for entity in entities if entity["kind"] == kind]
    if type:
        wanted = normalize(type)
        entities = [entity for entity in entities if wanted in normalize(entity["type"])]
    if layer:
        wanted = normalize(layer)
        entities = [entity for entity in entities if any(wanted in normalize(name) for name in entity["layers"])]
    page: list[EntityInfo] = []
    for entity in entities[offset : offset + limit]:
        item: EntityInfo = {
            "id": entity["id"],
            "kind": entity["kind"],
            "type": entity["type"],
            "layers": entity["layers"],
            "bbox": _box(entity["bbox"]),
        }
        if entity["text"] is not None:
            item["text"], cut = _clip(entity["text"], length)
            if cut:
                item["truncated"] = True
        if geometry:
            shapes, used, omitted = _geometry(entity, budget, length)
            # The first entity of a page always comes, with the shapes that fit, so that paging goes on
            if omitted and page:
                break
            placement = _placement(entity["placement"]) if entity["placement"] else None
            item["placement"] = {"name": placement["name"], "scale": placement["scale"]} if placement else None
            item["geometry"] = shapes
            if omitted:
                item["geometry_omitted"] = omitted
            budget -= used
        page.append(item)
    following = offset + len(page)
    return {
        "total": len(entities),
        "offset": offset,
        "next_offset": following if following < len(entities) else None,
        "types": dict(Counter(entity["type"] for entity in entities).most_common()),
        "kinds": dict(Counter(entity["kind"] for entity in entities).most_common()),
        "entities": page,
    }


def get_entity(
    scene: Scene,
    id: str,
    *,
    max_points: int = DEFAULT_MAX_POINTS,
    max_text_length: int | None = DEFAULT_TEXT_LENGTH,
) -> EntityDetail:
    """One entity with everything the drawing holds about it: kind, type, layers, box, text, the properties that the
    format gives (key, Japanese label and formatted value), the unformatted values by key, the placement with its
    scale and ``to_real``, and its geometry.

    The geometry is the shapes drawn, in drawing order and paper millimeters: polylines (``points``, curves as the
    polyline drawn), segments (pairs of points), fills and hatches (``rings``), points and text parts (``point``, the
    lower left of the text). Inside a placement, ``real`` repeats the coordinates in real-world millimeters, unless
    they are the same as on the sheet. Shapes past ``max_points`` (where a point with ``real`` counts twice) are left
    out whole and counted in ``geometry_omitted``."""
    budget = _max_points(max_points)
    length = _text_length(max_text_length)
    entity = scene.entity(id, geometry=True)
    if entity is None:
        raise DrawingError(f"no entity {id!r} in this drawing")
    shapes, _, omitted = _geometry(entity, budget, length)
    cut = any(shape.get("truncated") for shape in shapes)

    def clip(value: str) -> str:
        nonlocal cut
        text, clipped = _clip(value, length)
        cut = cut or clipped
        return text

    # values has one key per property, in the same order (the keys of an entity are distinct)
    result: EntityDetail = {
        "id": entity["id"],
        "kind": entity["kind"],
        "type": entity["type"],
        "source": entity["source"],
        "layers": entity["layers"],
        "bbox": _box(entity["bbox"]),
        "text": clip(entity["text"]) if entity["text"] is not None else None,
        "properties": [
            {"key": key, "label": label, "value": clip(value)}
            for key, (label, value) in zip(entity["values"], entity["properties"], strict=True)
        ],
        "values": {
            key: clip(value) if isinstance(value, str) else _value(value) for key, value in entity["values"].items()
        },
        "placement": _placement(entity["placement"]) if entity["placement"] else None,
        "geometry": shapes,
    }
    if omitted:
        result["geometry_omitted"] = omitted
    if cut:
        result["truncated"] = True
    return result


def _picked(scene: Scene, x: float, y: float) -> dict[str, Any] | None:
    """The smallest entity with a placement whose box is within ``PICK_MM`` of the point."""
    near = scene.entities(region=(x - PICK_MM, y - PICK_MM, x + PICK_MM, y + PICK_MM))
    placed = [entity for entity in near if entity["placement"]]

    def area(entity: dict[str, Any]) -> float:
        x0, y0, x1, y1 = entity["bbox"]
        return (x1 - x0) * (y1 - y0)

    return min(placed, key=area, default=None)


def _to_real(placement: dict[str, Any], x: float, y: float) -> tuple[float, float]:
    a, b, c, d, e, f = placement["inverse"]
    return (a * x + c * y + e, b * x + d * y + f)


def measure(
    scene: Scene,
    start: Sequence[float],
    end: Sequence[float],
    *,
    entity: str | None = None,
) -> MeasurementInfo:
    """The distance between two points on the sheet (paper millimeters), and in real-world millimeters when a
    placement gives the scale.

    Like the viewer's measurement: the scale comes from the entity given, or else from the entities nearest to the
    two points. When the two points sit in placements of different scales, there is no real-world length.
    """
    if not (_finite(start, 2) and _finite(end, 2)):
        raise DrawingError("start and end need two finite numbers [x, y] in paper millimeters")
    (x0, y0), (x1, y1) = start, end
    paper = math.hypot(x1 - x0, y1 - y0)
    if entity is not None:
        chosen = scene.entity(entity)
        if chosen is None:
            raise DrawingError(f"no entity {entity!r} in this drawing")
        if not chosen["placement"]:
            raise DrawingError(f"entity {entity!r} has no placement, so it gives no scale")
        picks = [chosen, chosen]
    else:
        picks = [_picked(scene, x0, y0), _picked(scene, x1, y1)]
    result: MeasurementInfo = {
        "paper": _mm(paper),
        "real": None,
        "placement": None,
        "entities": [pick["id"] if pick else None for pick in picks],
    }
    placements = [pick["placement"] for pick in picks if pick]
    if not placements:
        result["reason"] = "no entity with a scale near the points; the length is on the sheet only"
        return result
    first = placements[0]
    if any(
        abs(other["scale_x"] - first["scale_x"]) > 1e-12 or abs(other["scale_y"] - first["scale_y"]) > 1e-12
        for other in placements[1:]
    ):
        result["reason"] = "the points sit in placements of different scales"
        return result
    (rx0, ry0), (rx1, ry1) = _to_real(first, x0, y0), _to_real(first, x1, y1)
    result["real"] = _mm(math.hypot(rx1 - rx0, ry1 - ry0))
    result["placement"] = _placement(first)
    result["start_real"] = [_mm(rx0), _mm(ry0)]
    result["end_real"] = [_mm(rx1), _mm(ry1)]
    return result


def _shifted(box: Sequence[float], offset: tuple[float, float]) -> list[float]:
    dx, dy = offset
    x0, y0, x1, y1 = box
    return _box((x0 - dx, y0 - dy, x1 - dx, y1 - dy))


def compare_revisions(
    before: Scene,
    after: Scene,
    *,
    region: Sequence[float] | None = None,
    frame_region: Sequence[float] | None = None,
    size: int = DEFAULT_IMAGE_SIZE,
    tolerance: float = 0.01,
    style: bool = False,
    all_layers: bool = False,
    limit: int = DEFAULT_LIMIT,
    max_text_length: int | None = DEFAULT_TEXT_LENGTH,
) -> tuple[bytes, ComparisonInfo]:
    """Compare two revisions: an image of the changes over the unchanged entities, the regions that changed and the
    changed entities. Entities are matched by shape and text, not by id. Boxes are in the earlier revision's paper
    millimeters; the later one is moved by ``offset``. ``frame_region`` is a part of the frame of the two revisions
    drawn together."""
    if not (math.isfinite(tolerance) and tolerance > 0):
        raise DrawingError("tolerance must be a positive number of millimeters")
    limit, _ = _page(limit, 0)
    length = _text_length(max_text_length)
    comparison = compare(before, after, tolerance=tolerance, style=style, all_layers=all_layers)
    offset = comparison.offset

    def clip(text: str | None, change: ChangeInfo) -> str | None:
        if text is None:
            return None
        clipped, cut = _clip(text, length)
        if cut:
            change["truncated"] = True
        return clipped

    changes: list[ChangeInfo] = []
    for pair in comparison.changed:
        old, new = pair["before"], pair["after"]
        change: ChangeInfo = {
            "change": "changed",
            "kind": old["kind"],
            "type": old["type"],
            "bbox": _box(old["bbox"]),
            "before_id": old["id"],
            "after_id": new["id"],
        }
        if old["text"] is not None or new["text"] is not None:
            change["before_text"] = clip(old["text"], change)
            change["after_text"] = clip(new["text"], change)
        changes.append(change)
    for entity in comparison.removed:
        change = {"change": "removed", "kind": entity["kind"], "type": entity["type"], "bbox": _box(entity["bbox"])}
        change["id"] = entity["id"]
        if entity["text"] is not None:
            change["text"] = clip(entity["text"], change)
        changes.append(change)
    for entity in comparison.added:
        change = {
            "change": "added",
            "kind": entity["kind"],
            "type": entity["type"],
            "bbox": _shifted(entity["bbox"], offset),
        }
        change["id"] = entity["id"]
        if entity["text"] is not None:
            change["text"] = clip(entity["text"], change)
        changes.append(change)
    png, image = render(comparison.scene, region=region, frame_region=frame_region, size=size, text_limit=0)
    info: ComparisonInfo = {
        "unchanged": comparison.unchanged,
        "removed": len(comparison.removed),
        "added": len(comparison.added),
        "changed": len(comparison.changed),
        "offset": [_mm(offset[0]), _mm(offset[1])],
        "regions": [
            {
                "bbox": _box(region["bbox"]),
                "removed": region["removed"],
                "added": region["added"],
                "changed": region["changed"],
            }
            for region in comparison.regions[:limit]
        ],
        "changes_total": len(changes),
        "changes": changes[:limit],
        "image": {
            "width": image["width"],
            "height": image["height"],
            "region": image["region"],
            "frame_region": image["frame_region"],
            "scale": image["scale"],
            "pixels": image["pixels"],
        },
        "colors": "red: removed or before a change; blue: added or after a change; gray: unchanged",
    }
    return png, info
