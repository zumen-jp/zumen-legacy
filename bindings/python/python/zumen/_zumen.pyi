import os
from typing import Any, Literal, TypedDict

__version__: str

type Box = tuple[float, float, float, float]

class Placement(TypedDict):
    """用紙上の mm と元の座標の換算。縮尺と、用紙座標から元の座標への変換を持ちます。"""

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
    """点列を通る折線です。曲線も描画時の折線として返します。型検査用です。"""

    shape: Literal["polyline"]
    points: list[Point]
    closed: bool
    real: list[Point] | None

class Segments(TypedDict):
    """2 点ずつの独立した線分です。型検査用です。"""

    shape: Literal["segments"]
    segments: list[tuple[Point, Point]]
    real: list[tuple[Point, Point]] | None

class Area(TypedDict):
    """塗りまたはハッチングの領域です。外周と穴を偶奇規則で扱います。型検査用です。"""

    shape: Literal["fill", "hatch"]
    rings: list[list[Point]]
    real: list[list[Point]] | None

class Dot(TypedDict):
    """点です。型検査用です。"""

    shape: Literal["point"]
    point: Point
    real: Point | None

class TextPart(TypedDict):
    """文字の部分です。左下位置、高さ、角度（度）を持ちます。型検査用です。"""

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
    """図形の種類、レイヤ、範囲、文字、属性と、指定時は形を返します。

    kind と values のキーは形式によらない識別子です。type と属性ラベルは日本語です。
    values の数値と座標は、用紙ではなく元の形式の座標系・単位です。"""

    geometry: list[Shape]

class Text(TypedDict):
    """用紙上の mm で表す文字です。型検査用です。"""

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
    """表題欄の項目の候補です。型検査用です。"""

    key: Literal["number", "revision", "title", "project", "scale", "date"]
    value: str
    label: str
    source: Literal["attribute", "label", "inline", "sheet", "metadata"]
    entity: str | None
    label_entity: str | None
    x: float
    y: float

class TitleBlock(TypedDict):
    """表題欄の項目と、見つかった候補の一覧です。型検査用です。"""

    number: str | None
    revision: str | None
    title: str | None
    project: str | None
    scale: str | None
    date: str | None
    fields: list[TitleField]

class RegionShape(TypedDict):
    """閉じた領域の外周（反時計回り）・穴（時計回り）・面積・周長です。型検査用です。"""

    area: float
    outline_area: float
    perimeter: float
    outline: list[Point]
    holes: list[list[Point]]

class RegionText(TypedDict):
    """閉じた領域内の文字です。位置は文字の範囲の中心です。型検査用です。"""

    id: str
    text: str
    x: float
    y: float

class Region(RegionShape):
    """線で囲まれた領域です。座標は用紙上の mm、配置内の real は実寸の mm です。型検査用です。"""

    bbox: Box
    placement: Placement | None
    real: RegionShape | None
    boundary: list[str]
    texts: list[RegionText]

class TableCell(TypedDict):
    """罫線の表のセルです。左上からの行・列、結合範囲、文字を持ちます。型検査用です。"""

    row: int
    column: int
    row_span: int
    column_span: int
    bbox: Box
    text: str
    texts: list[RegionText]

class Table(TypedDict):
    """縦横の罫線で描かれた表です。座標は用紙上の mm です。型検査用です。"""

    bbox: Box
    placement: Placement | None
    row_count: int
    column_count: int
    columns: list[float]
    rows: list[float]
    cells: list[TableCell]
    grid: list[list[str]]
    boundary: list[str]

class CheckFinding(TypedDict):
    """規則に違反した値と、その値を持つ対象の件数・id・レイヤです。型検査用です。"""

    value: str
    message: str
    count: int
    entities: list[str]
    layers: list[str]

class CheckResult(TypedDict):
    """規則ごとの確認件数、確認できなかった件数、指摘です。型検査用です。"""

    id: str
    kind: str
    level: str
    reference: str | None
    description: str | None
    checked: int
    unchecked: int
    findings: list[CheckFinding]

class CheckReport(TypedDict):
    """規則集による確認結果です。error の違反がなければ passed が真になります。型検査用です。"""

    rule_set: str
    title: str | None
    passed: bool
    results: list[CheckResult]

class DrawnEntity(TypedDict):
    """Rendering に描いた図形と、そのピクセル範囲です。型検査用です。"""

    id: str
    kind: str
    type: str
    box: Box
    text: str | None

class ReadError(ValueError): ...

class LimitError(ReadError):
    """max_bytes・max_entities・timeout の上限で読み取りを停止したエラーです。"""

    limit: Literal["bytes", "entities", "time"]

class RenderError(ValueError): ...

class EditError(ValueError):
    """適用できない編集です。ファイルには何も書き込みません。"""

    code: str
    operation: int | None

class MarkupError(ValueError):
    code: str
    item: int | None
    id: str | None
    key: str | None

class CarryError(ValueError):
    code: str

class DraftError(ValueError):
    """描画できない図面仕様です。ファイルには何も書き込みません。"""

    code: str
    path: str

class Scene:
    """用紙上の mm（Y 軸は上向き）で表す図面です。ブラウザの @zumenjs/core と共通のモデルです。"""

    @property
    def format(self) -> str: ...
    @property
    def version(self) -> str: ...
    @property
    def unit(self) -> LengthUnit | None:
        """図面自体の座標の長さの単位です。values 内の点と長さに適用します。"""
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
        markup: object | None = None,
        markup_colors: dict[str, str] | None = None,
    ) -> bytes: ...
    def render_svg(
        self,
        *,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
        markup: object | None = None,
        markup_colors: dict[str, str] | None = None,
        underlay_dpi: float = 150.0,
    ) -> str: ...
    def render_pdf(
        self,
        *,
        region: Box | None = None,
        theme: Literal["light", "dark"] | None = None,
        line_weights: bool = True,
        hidden_layers: list[int] | None = None,
        sheet_frame: bool = True,
        markup: object | None = None,
        markup_colors: dict[str, str] | None = None,
        underlay_dpi: float = 150.0,
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
        markup: object | None = None,
        markup_colors: dict[str, str] | None = None,
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
        """線で囲まれた領域（室名があれば部屋）を、配置ごとに読み順で返します。"""
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
        """罫線で描かれた表（建具表・表題欄など）を読み順で返します。"""
    def check(self, rules: str | dict[str, Any] | None = None) -> CheckReport:
        """組み込み規則集（既定は mlit-cad-2025）、JSON 文字列、辞書で製図基準を確認します。"""
    def to_arrow(self, *, region: Box | None = None, hidden_layers: list[int] | None = None) -> Any:
        """図形を pyarrow.Table で返します。pyarrow が必要です。"""

class Rendering:
    """図面の PNG、ピクセルと用紙上の mm の変換、描いた図形です。"""

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
    """形と文字で図形を照合した 2 つの版の比較結果です。id では照合しません。"""

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
    """別形式に書き出した図面と、元のまま書けなかった内容の注意事項です。"""

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
    """編集・新規作成した図面、追加した図形の id、元のまま書けなかった内容の注意事項です。"""

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
def read_markup(text: str) -> str: ...
def markup_schema() -> str: ...
def markup_boxes(text: str) -> list[Box]: ...
def drawing_hash(data: bytes) -> str: ...
def carry_markup(
    markup: str,
    before: bytes,
    after: bytes,
    *,
    views: dict[str, str] | None = None,
    tolerance: float = 0.01,
    style: bool = False,
    align: bool = True,
    ignore_hash: bool = False,
    max_bytes: int | None = None,
    max_entities: int | None = None,
    timeout: float | None = None,
) -> tuple[str, int, int, int]: ...
def draft(spec: object, to: str = "dxf", *, version: str | None = None, sheet: str | None = None) -> Converted: ...
def draft_into(
    spec: object,
    data: bytes,
    *,
    sheet: str | None = None,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Converted: ...
