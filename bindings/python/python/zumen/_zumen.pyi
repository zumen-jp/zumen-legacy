from typing import Any, Literal, TypeAlias, TypedDict

__version__: str

Box: TypeAlias = tuple[float, float, float, float]

class Placement(TypedDict):
    """Paper millimeters per original millimeter, and the conversion from paper to original coordinates."""

    name: str
    scale_x: float
    scale_y: float
    inverse: tuple[float, float, float, float, float, float]

class Entity(TypedDict):
    """An entity of ``Scene.entities`` and ``Scene.entity`` (type checking only)."""

    id: str
    type: str
    source: str
    layers: list[str]
    bbox: Box
    text: str | None
    properties: list[tuple[str, str]]
    placement: Placement | None

class Text(TypedDict):
    """A text of ``Scene.texts`` in paper millimeters (type checking only)."""

    id: str
    type: str
    text: str
    x: float
    y: float
    height: float
    angle: float
    bbox: Box
    layers: list[str]

class DrawnEntity(TypedDict):
    """An entity drawn in a ``Rendering``, with its box in pixels (type checking only)."""

    id: str
    type: str
    box: Box
    text: str | None

class ReadError(ValueError): ...
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
    def entities(self, *, region: Box | None = None, hidden_layers: list[int] | None = None) -> list[Entity]: ...
    def entity(self, id: str) -> Entity | None: ...
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
def detect(data: bytes) -> str | None: ...
def read(data: bytes, *, format: str | None = None, view: str | None = None) -> Scene: ...
