"""read_markup が返すマークアップ文書の型です（決定 0021、docs/markup.md）。

点は項目のビューの用紙上の mm、[x, y] で Y 軸は上向きです。未知のキーは読み書きで保ちます。"""

from __future__ import annotations

from typing import Any, Literal, TypedDict

type MarkupPoint = list[float]
type MarkupKind = Literal["redline", "cloud", "text", "finding", "photo"]
type MarkupStatus = Literal["open", "resolved", "closed"]
type MarkupArrows = Literal["none", "start", "end", "both"]
type CarriedState = Literal["unchanged", "changed", "removed"]


class MarkupDrawing(TypedDict):
    """マークアップを付けた図面です。バイト列の SHA-256（小文字の 16 進数）と Scene.format を持ちます。"""

    sha256: str
    format: str


MarkupCarried = TypedDict("MarkupCarried", {"from": str, "state": CarriedState})
"""新版へ移した項目に付ける、移行元の SHA-256 と状態。"""


class _MarkupItemKeys(TypedDict, total=False):
    view: str
    """ビュー id（``Scene.view``）。単一ビューの形式では省略します。"""
    category: str
    """色を選ぶ利用側のキー。小文字・数字・ハイフンを使います。"""
    color: str
    """``#rrggbb`` または ``#rrggbbaa``。カテゴリの色より優先します。"""
    lineWidth: float
    """用紙上の mm。省略時の線幅は 2 ピクセルです。"""
    text: str
    entities: list[str]
    """項目の対象となる図形の id。"""
    author: str
    createdAt: str
    """RFC 3339."""
    updatedAt: str
    carried: MarkupCarried
    data: dict[str, Any]
    """担当者・期限・返信など利用側の情報。そのまま保持します。"""


class _RedlineItem(TypedDict):
    id: str
    kind: Literal["redline"]
    points: list[MarkupPoint]


class RedlineItem(_RedlineItem, _MarkupItemKeys, total=False):
    """線・折線・手書き・長方形の赤入れです。太く半透明にすると蛍光ペンとして使えます。"""

    closed: bool
    arrows: MarkupArrows


class _CloudItem(TypedDict):
    id: str
    kind: Literal["cloud"]
    points: list[MarkupPoint]


class CloudItem(_CloudItem, _MarkupItemKeys, total=False):
    """変更雲です。閉じた外周 points に沿って外向きにふくらむ円弧を描きます。"""

    arcLength: float
    """円弧の弦の長さ。用紙上の mm、既定は 10 です。"""


class _TextItem(TypedDict):
    id: str
    kind: Literal["text"]
    position: MarkupPoint
    text: str


class TextItem(_TextItem, _MarkupItemKeys, total=False):
    """注記です。position は最初の行の左下です。"""

    height: float
    """用紙上の mm、既定は 3.5 です。"""
    angle: float
    """反時計回りの度、既定は 0 です。"""


class _FindingItem(TypedDict):
    id: str
    kind: Literal["finding"]
    position: MarkupPoint
    status: MarkupStatus


class FindingItem(_FindingItem, _MarkupItemKeys, total=False):
    """指摘です。position にピンを置き、任意の範囲を付けられます。text は指摘内容です。"""

    points: list[MarkupPoint]
    label: str
    """番号など、ピン上の短い文字。"""


class _PhotoItem(TypedDict):
    id: str
    kind: Literal["photo"]
    position: MarkupPoint
    photo: str


class PhotoItem(_PhotoItem, _MarkupItemKeys, total=False):
    """写真の撮影位置です。photo は利用側が解決する id・URL などの参照です。"""

    direction: float
    """カメラの方向。+X 軸から反時計回りの度です。"""
    takenAt: str


type MarkupItem = RedlineItem | CloudItem | TextItem | FindingItem | PhotoItem


class _MarkupDocument(TypedDict):
    format: Literal["zumen-markup"]
    version: int
    drawing: MarkupDrawing
    items: list[MarkupItem]


class MarkupDocument(_MarkupDocument, total=False):
    """図面に添えるマークアップ文書です。図面ファイルは変更しません。"""

    data: dict[str, Any]


class MarkupCarry(TypedDict):
    """carry_markup の戻り値です。新版の文書と、状態ごとの項目数を持ちます。"""

    markup: MarkupDocument
    unchanged: int
    changed: int
    removed: int
