import os
from typing import Any, Literal, TypedDict

__version__: str

type Box = tuple[float, float, float, float]

class Placement(TypedDict):
    """Paper millimeters per original millimeter, and the conversion from paper to original coordinates."""

    name: str
    scale_x: float
    scale_y: float
    inverse: tuple[float, float, float, float, float, float]

type Point = tuple[float, float]
type PropertyValue = float | str | bool | tuple[float, ...]
type LengthUnit = Literal[
    "millimeter",
    "centimeter",
    "decimeter",
    "meter",
    "decameter",
    "hectometer",
    "kilometer",
    "gigameter",
    "micrometer",
    "nanometer",
    "angstrom",
    "inch",
    "foot",
    "yard",
    "mile",
    "microinch",
    "mil",
    "us-survey-foot",
    "astronomical-unit",
    "light-year",
    "parsec",
]

class Polyline(TypedDict):
    """A line through points; curves come as the polyline drawn (type checking only)."""

    shape: Literal["polyline"]
    points: list[Point]
    closed: bool
    real: list[Point] | None

class Segments(TypedDict):
    """Separate lines, two points each (type checking only)."""

    shape: Literal["segments"]
    segments: list[tuple[Point, Point]]
    real: list[tuple[Point, Point]] | None

class Area(TypedDict):
    """A filled area, or one filled with hatch lines: outer and inner rings, even-odd (type checking only)."""

    shape: Literal["fill", "hatch"]
    rings: list[list[Point]]
    real: list[list[Point]] | None

class Dot(TypedDict):
    """A point (type checking only)."""

    shape: Literal["point"]
    point: Point
    real: Point | None

class TextPart(TypedDict):
    """A part of a text: the lower left of the text, its height and angle in degrees (type checking only)."""

    shape: Literal["text"]
    text: str
    point: Point
    height: float
    angle: float
    real: Point | None

#: A shape drawn, in paper millimeters; ``real`` holds the same coordinates in real-world millimeters when the
#: entity sits in a placement, and is None otherwise.
type Shape = Polyline | Segments | Area | Dot | TextPart

class _Entity(TypedDict):
    id: str
    kind: str
    type: str
    source: str
    layers: list[str]
    bbox: Box
    text: str | None
    properties: list[tuple[str, str]]
    values: dict[str, PropertyValue]
    placement: Placement | None

class Entity(_Entity, total=False):
    """An entity of ``Scene.entities`` and ``Scene.entity``, with ``geometry`` when asked (type checking only).

    ``kind`` and the keys of ``values`` are the same for every format (see "Stable keys" in docs/architecture.md);
    ``type`` and the labels of ``properties`` are Japanese, for people. ``values`` maps each property's key to its
    unformatted value (numbers, ``(x, y)``), or to the formatted text when it has none."""

    geometry: list[Shape]

class Text(TypedDict):
    """A text of ``Scene.texts`` in paper millimeters (type checking only)."""

    id: str
    kind: str
    type: str
    text: str
    x: float
    y: float
    height: float
    angle: float
    bbox: Box
    layers: list[str]

class TitleField(TypedDict):
    """A candidate for a title block field (type checking only)."""

    key: Literal["number", "revision", "title", "project", "scale", "date"]
    value: str
    label: str
    source: Literal["attribute", "label", "inline", "sheet", "metadata"]
    entity: str | None
    label_entity: str | None
    x: float
    y: float

class TitleBlock(TypedDict):
    """The fields of a title block, and every candidate found (type checking only)."""

    number: str | None
    revision: str | None
    title: str | None
    project: str | None
    scale: str | None
    date: str | None
    fields: list[TitleField]

class RegionShape(TypedDict):
    """The outline (counterclockwise), holes (clockwise), area and perimeter of a closed region (type checking only)."""

    area: float
    outline_area: float
    perimeter: float
    outline: list[Point]
    holes: list[list[Point]]

class RegionText(TypedDict):
    """A text inside a closed region, at the center of its box (type checking only)."""

    id: str
    text: str
    x: float
    y: float

class Region(RegionShape):
    """A region that drawn lines close, in paper millimeters, with ``real`` in real-world millimeters inside a
    placement (type checking only)."""

    bbox: Box
    placement: Placement | None
    real: RegionShape | None
    boundary: list[str]
    texts: list[RegionText]

class TableCell(TypedDict):
    """A cell of a ruled table: its row and column from the top left, the rows and columns it spans, and its text
    (type checking only)."""

    row: int
    column: int
    row_span: int
    column_span: int
    bbox: Box
    text: str
    texts: list[RegionText]

class Table(TypedDict):
    """A table drawn with horizontal and vertical rules, in paper millimeters (type checking only)."""

    bbox: Box
    placement: Placement | None
    row_count: int
    column_count: int
    columns: list[float]
    rows: list[float]
    cells: list[TableCell]
    grid: list[list[str]]
    boundary: list[str]

class DrawnEntity(TypedDict):
    """An entity drawn in a ``Rendering``, with its box in pixels (type checking only)."""

    id: str
    kind: str
    type: str
    box: Box
    text: str | None

class ReadError(ValueError): ...

class LimitError(ReadError):
    """Reading stopped at ``max_bytes``, ``max_entities`` or ``timeout``."""

    limit: Literal["bytes", "entities", "time"]

class RenderError(ValueError): ...

class EditError(ValueError):
    """An edit that could not be applied; nothing was written."""

    code: str
    operation: int | None

class DraftError(ValueError):
    """A drawing spec that could not be drawn; nothing was written."""

    code: str
    path: str

class Scene:
    """A drawing in paper millimeters (Y up), the same model as ``@zumenjs/core``."""

    @property
    def format(self) -> str: ...
    @property
    def version(self) -> str: ...
    @property
    def unit(self) -> LengthUnit | None:
        """The length unit of the drawing's own coordinates (the points and lengths in ``values``)."""
    @property
    def background(self) -> Literal["dark", "light"]: ...
    @property
    def bounds(self) -> tuple[float, float, float, float]: ...
    @property
    def sheet(self) -> dict[str, str | float] | None: ...
    @property
    def layers(self) -> list[dict[str, int | str | bool | None]]: ...
    @property
    def notices(self) -> list[dict[str, str | int]]: ...
    @property
    def metadata(self) -> list[tuple[str, str]]: ...
    @property
    def metadata_values(self) -> dict[str, PropertyValue]: ...
    @property
    def views(self) -> list[tuple[str, str]] | None: ...
    @property
    def view(self) -> str | None: ...
    def __len__(self) -> int: ...
    def to_json(self, indent: int | None = None) -> str: ...
    def render_png(
        self,
        *,
        dpi: float = 150.0,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
    ) -> bytes: ...
    def render_svg(
        self,
        *,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
    ) -> str: ...
    def render_pdf(
        self,
        *,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
    ) -> bytes: ...
    def render(
        self,
        *,
        dpi: float = 150.0,
        size: tuple[int, int] | None = None,
        margin: float = 0.0,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
    ) -> Rendering: ...
    def texts(self, *, region: Box | None = None, hidden_layers: list[int] | None = None) -> list[Text]: ...
    def entities(
        self, *, region: Box | None = None, hidden_layers: list[int] | None = None, geometry: bool = False
    ) -> list[Entity]: ...
    def entity(self, id: str, *, geometry: bool = False) -> Entity | None: ...
    def title_block(self) -> TitleBlock: ...
    def regions(
        self,
        *,
        region: Box | None = None,
        hidden_layers: list[int] | None = None,
        layers: list[int] | None = None,
        kinds: list[str] | None = None,
        tolerance: float = 0.01,
        min_area: float = 0.0,
    ) -> list[Region]:
        """The regions that drawn lines close (rooms, when a text names them), in reading order by placement."""
    def tables(
        self,
        *,
        region: Box | None = None,
        hidden_layers: list[int] | None = None,
        layers: list[int] | None = None,
        tolerance: float = 0.01,
        min_cells: int = 2,
        empty: bool = False,
    ) -> list[Table]:
        """The tables drawn with horizontal and vertical rules (schedules, title blocks), in reading order."""
    def to_arrow(self, *, region: Box | None = None, hidden_layers: list[int] | None = None) -> Any:
        """A ``pyarrow.Table`` of the entities (needs pyarrow)."""

class Rendering:
    """A PNG of a scene, the conversion between pixels and paper millimeters, and the entities drawn."""

    @property
    def png(self) -> bytes: ...
    @property
    def width(self) -> int: ...
    @property
    def height(self) -> int: ...
    @property
    def scale(self) -> float: ...
    @property
    def region(self) -> Box: ...
    @property
    def entities(self) -> list[DrawnEntity]: ...
    def to_pixel(self, x: float, y: float) -> tuple[float, float]: ...
    def to_paper(self, px: float, py: float) -> tuple[float, float]: ...
    def entities_at(self, px: float, py: float, *, tolerance: float = 3.0) -> list[DrawnEntity]: ...
    def _repr_png_(self) -> bytes: ...

class ChangedPair(TypedDict):
    before: Entity
    after: Entity

class ChangeRegion(TypedDict):
    bbox: Box
    removed: int
    added: int
    changed: int

class Comparison:
    """Two revisions compared: entities matched by shape and text, not by id."""

    @property
    def removed(self) -> list[Entity]: ...
    @property
    def added(self) -> list[Entity]: ...
    @property
    def changed(self) -> list[ChangedPair]: ...
    @property
    def unchanged(self) -> int: ...
    @property
    def offset(self) -> tuple[float, float]: ...
    @property
    def regions(self) -> list[ChangeRegion]: ...
    @property
    def scene(self) -> Scene: ...
    def __len__(self) -> int: ...

def compare(
    before: Scene,
    after: Scene,
    *,
    tolerance: float = 0.01,
    style: bool = False,
    all_layers: bool = False,
    align: bool = True,
) -> Comparison: ...

class Converted:
    """A drawing written in another format, and the notices about what could not be written as it was."""

    @property
    def data(self) -> bytes: ...
    @property
    def format(self) -> str: ...
    @property
    def notices(self) -> list[dict[str, str | int]]: ...
    def save(self, path: str | os.PathLike[str]) -> None: ...
    def __len__(self) -> int: ...

def detect(data: bytes) -> str | None: ...
def read(
    data: bytes,
    *,
    format: str | None = None,
    view: str | None = None,
    max_bytes: int | None = None,
    max_entities: int | None = None,
    timeout: float | None = None,
) -> Scene: ...
def convert(data: bytes, to: str, *, format: str | None = None, version: str | None = None) -> Converted: ...

class Edited:
    """An edited or new drawing, the ids of what was added and the notices about what was written differently."""

    @property
    def data(self) -> bytes: ...
    @property
    def created(self) -> list[str]: ...
    @property
    def notices(self) -> list[dict[str, str | int]]: ...
    def save(self, path: str | os.PathLike[str]) -> None: ...
    def __len__(self) -> int: ...

def edit(data: bytes, operations: object, *, format: str | None = None, max_bytes: int | None = None) -> Edited: ...
def create(to: str, operations: object = None, *, units: str = "mm") -> Edited: ...
def draft_schema() -> str: ...
def draft(spec: object, to: str = "dxf", *, version: str | None = None, sheet: str | None = None) -> Converted: ...
def draft_into(
    spec: object,
    data: bytes,
    *,
    sheet: str | None = None,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Converted: ...
