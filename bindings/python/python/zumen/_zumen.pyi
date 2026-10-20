import os
from typing import Any, Literal, TypeAlias, TypedDict

from typing_extensions import NotRequired

__version__: str

Box: TypeAlias = tuple[float, float, float, float]

class Placement(TypedDict):
    """Paper millimeters per original millimeter, and the conversion from paper to original coordinates."""

    name: str
    scale_x: float
    scale_y: float
    inverse: tuple[float, float, float, float, float, float]

PropertyValue: TypeAlias = float | str | bool | tuple[float, ...]

class StrokePrimitive(TypedDict):
    """A line through ``points`` in paper millimeters; ``closed`` joins the last point to the first, and with
    ``segments`` each two points are a separate segment. ``width`` and ``dashes`` are paper millimeters."""

    kind: Literal["stroke"]
    points: list[tuple[float, float]]
    closed: bool
    segments: bool
    color: str
    width: float
    dashes: list[float]

class FillPrimitive(TypedDict):
    """An area filled by the even-odd rule: an outline and its holes, in paper millimeters."""

    kind: Literal["fill"]
    color: str
    rings: list[list[tuple[float, float]]]

class ClippedPrimitive(TypedDict):
    """Lines drawn only inside ``rings`` (hatch patterns)."""

    kind: Literal["clipped"]
    rings: list[list[tuple[float, float]]]
    strokes: list[StrokePrimitive]

class DotPrimitive(TypedDict):
    """A point marker drawn at the same size at every zoom."""

    kind: Literal["dot"]
    x: float
    y: float
    color: str

class GlyphCell(TypedDict):
    """The center of one character, in the text's own coordinates."""

    char: str
    x: float
    y: float

class TextPrimitive(TypedDict):
    """A run of text. ``matrix`` (a, b, c, d, e, f) maps the text's own coordinates (x along the line, y up, the box
    from (0, 0) to (box_width, box_height)) to paper millimeters: x' = a x + c y + e, y' = b x + d y + f."""

    kind: Literal["text"]
    text: str
    color: str
    font_family: str
    height: float
    scale_x: float
    box_width: float
    box_height: float
    matrix: tuple[float, float, float, float, float, float]
    cells: list[GlyphCell]

Primitive: TypeAlias = StrokePrimitive | FillPrimitive | ClippedPrimitive | DotPrimitive | TextPrimitive

class Entity(TypedDict):
    """An entity of ``Scene.entities`` and ``Scene.entity`` (type checking only).

    ``kind`` and the keys of ``values`` are the same for every format (see "Stable keys" in docs/architecture.md);
    ``type`` and the labels of ``properties`` are Japanese, for people. ``values`` maps each property's key to its
    unformatted value (numbers, ``(x, y)``), or to the formatted text when it has none. ``primitives`` are what is
    drawn, in drawing order and paper millimeters: always from ``Scene.entity``, and from ``Scene.entities`` with
    ``geometry=True``."""

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
    primitives: NotRequired[list[Primitive]]

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

class Scene:
    """A drawing in paper millimeters (Y up), the same model as ``@zumenjs/core``."""

    @property
    def format(self) -> str: ...
    @property
    def version(self) -> str: ...
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
    def entity(self, id: str) -> Entity | None: ...
    def title_block(self) -> TitleBlock: ...
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
def convert(data: bytes, to: str, *, format: str | None = None) -> Converted: ...
