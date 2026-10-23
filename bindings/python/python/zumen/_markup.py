"""The types of drawing markup documents (decision 0021, docs/markup.md), as ``read_markup`` returns them.

Points are paper millimeters of the item's view, ``[x, y]`` with the Y axis up. Keys that zumen does not know are
kept as they are when a document is read and written again, although these types do not name them.
"""

from __future__ import annotations

from typing import Any, Literal, TypedDict

type MarkupPoint = list[float]
type MarkupKind = Literal["redline", "cloud", "text", "finding", "photo"]
type MarkupStatus = Literal["open", "resolved", "closed"]
type MarkupArrows = Literal["none", "start", "end", "both"]
type CarriedState = Literal["unchanged", "changed", "removed"]


class MarkupDrawing(TypedDict):
    """The drawing the markup was made on: the SHA-256 of its bytes (lowercase hexadecimal) and ``Scene.format``."""

    sha256: str
    format: str


MarkupCarried = TypedDict("MarkupCarried", {"from": str, "state": CarriedState})
"""Set when an item was carried to a new revision: the SHA-256 of the drawing it came from, and its state."""


class _MarkupItemKeys(TypedDict, total=False):
    view: str
    """The view id of the scene (``Scene.view``); left out for formats with one view."""
    category: str
    """A key of the application in lowercase letters, digits and hyphens, which chooses the color."""
    color: str
    """``#rrggbb`` or ``#rrggbbaa``, over the category's color."""
    lineWidth: float
    """Paper millimeters. Without it lines are drawn 2 pixels wide."""
    text: str
    entities: list[str]
    """The ids of the entities the item is about."""
    author: str
    createdAt: str
    """RFC 3339."""
    updatedAt: str
    carried: MarkupCarried
    data: dict[str, Any]
    """For the application (assignees, due dates, replies), kept as it is."""


class _RedlineItem(TypedDict):
    id: str
    kind: Literal["redline"]
    points: list[MarkupPoint]


class RedlineItem(_RedlineItem, _MarkupItemKeys, total=False):
    """Lines, polylines, freehand strokes and rectangles; a wide, translucent color makes a highlighter."""

    closed: bool
    arrows: MarkupArrows


class _CloudItem(TypedDict):
    id: str
    kind: Literal["cloud"]
    points: list[MarkupPoint]


class CloudItem(_CloudItem, _MarkupItemKeys, total=False):
    """A revision cloud: arcs bulging outward along the closed outline ``points``."""

    arcLength: float
    """The chord of an arc in paper millimeters, 10 by default."""


class _TextItem(TypedDict):
    id: str
    kind: Literal["text"]
    position: MarkupPoint
    text: str


class TextItem(_TextItem, _MarkupItemKeys, total=False):
    """A note. ``position`` is the lower left of the first line."""

    height: float
    """Paper millimeters, 3.5 by default."""
    angle: float
    """Degrees counterclockwise, 0 by default."""


class _FindingItem(TypedDict):
    id: str
    kind: Literal["finding"]
    position: MarkupPoint
    status: MarkupStatus


class FindingItem(_FindingItem, _MarkupItemKeys, total=False):
    """A review finding: a pin at ``position``, with an optional area; ``text`` is the finding."""

    points: list[MarkupPoint]
    label: str
    """Short text on the pin, such as its number."""


class _PhotoItem(TypedDict):
    id: str
    kind: Literal["photo"]
    position: MarkupPoint
    photo: str


class PhotoItem(_PhotoItem, _MarkupItemKeys, total=False):
    """Where a photo was taken. ``photo`` is a reference that the application resolves, such as an id or a URL."""

    direction: float
    """The camera's heading, degrees counterclockwise from +x."""
    takenAt: str


type MarkupItem = RedlineItem | CloudItem | TextItem | FindingItem | PhotoItem


class _MarkupDocument(TypedDict):
    format: Literal["zumen-markup"]
    version: int
    drawing: MarkupDrawing
    items: list[MarkupItem]


class MarkupDocument(_MarkupDocument, total=False):
    """A markup document. It lives beside the drawing and never changes the drawing file."""

    data: dict[str, Any]


class MarkupCarry(TypedDict):
    """What ``carry_markup`` returns: the document for the new revision and the number of items in each state."""

    markup: MarkupDocument
    unchanged: int
    changed: int
    removed: int
