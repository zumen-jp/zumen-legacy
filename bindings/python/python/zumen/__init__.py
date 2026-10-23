"""zumen: 日本の建築・土木の 2D 図面を読み取り、描画・解析・編集します。

形式はファイル名ではなく中身で判定します。Scene は用紙上の mm（Y 軸は上向き）で、
ブラウザの @zumenjs/core と共通です。図面の読み取り・描画・書き出し中は GIL を解放します。"""

from __future__ import annotations

import json
import os

from ._markup import (
    CarriedState,
    CloudItem,
    FindingItem,
    MarkupArrows,
    MarkupCarried,
    MarkupCarry,
    MarkupDocument,
    MarkupDrawing,
    MarkupItem,
    MarkupKind,
    MarkupPoint,
    MarkupStatus,
    PhotoItem,
    RedlineItem,
    TextItem,
)
from ._zumen import (
    CarryError,
    Comparison,
    Converted,
    DraftError,
    Edited,
    EditError,
    LimitError,
    MarkupError,
    ReadError,
    RenderError,
    Rendering,
    Scene,
    __version__,
    compare,
    create,
    detect,
)
from ._zumen import carry_markup as _carry_markup
from ._zumen import convert as _convert
from ._zumen import draft as _draft
from ._zumen import draft_into as _draft_into
from ._zumen import draft_schema as _draft_schema
from ._zumen import drawing_hash as _drawing_hash
from ._zumen import edit as _edit
from ._zumen import markup_schema as _markup_schema
from ._zumen import read as _read
from ._zumen import read_markup as _read_markup

__all__ = [
    "CarriedState",
    "CarryError",
    "CloudItem",
    "Comparison",
    "Converted",
    "DraftError",
    "EditError",
    "Edited",
    "FindingItem",
    "LimitError",
    "MarkupArrows",
    "MarkupCarried",
    "MarkupCarry",
    "MarkupDocument",
    "MarkupDrawing",
    "MarkupError",
    "MarkupItem",
    "MarkupKind",
    "MarkupPoint",
    "MarkupStatus",
    "PhotoItem",
    "ReadError",
    "RedlineItem",
    "RenderError",
    "Rendering",
    "Scene",
    "TextItem",
    "__version__",
    "carry_markup",
    "compare",
    "convert",
    "create",
    "detect",
    "draft",
    "draft_into",
    "draft_schema",
    "drawing_hash",
    "edit",
    "markup_schema",
    "read",
    "read_markup",
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
    """バイト列またはファイルパスから図面を Scene に読み取ります。

    format で sfc・p21・p2z・sfz・jww・jwc・jws・dxf・dwg・pdf を指定できます。
    view はレイアウト名・PDF ページなどです。max_bytes・max_entities を超えると LimitError を返します。"""
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
    """図面を読み取り、AutoCAD の共通モデルを経て別形式に書き出します。

    to は dxf（既定 2013・UTF-8）・dwg（2013）・jww（Jw_cad 8）・sfc・p21・sfz・p2z（既定 SXF 3.1）です。
    version で DXF R12〜2018・SXF 2.0〜3.1 を選べます。R12 にない図形は分解します。
    2007 より前の DXF はコードページを使い、元のまま書けない内容は notices に返します。PDF は対象外です。"""
    return _convert(_load(source), to, format=format, version=version)


def edit(
    source: bytes | bytearray | memoryview | str | os.PathLike[str],
    operations: object,
    *,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Edited:
    """操作を適用し、対象外の内容・形式・版・文字コードを保って図面を編集します。

    operations は辞書のリストまたは同じ内容の JSON 文字列です。add・update・delete・add-layer・update-layer と
    部品・文字スタイル・線種・寸法スタイルの追加を使えます。座標は元の図面の単位です。
    DXF・DWG・JWW・SXF が対象です。すべて検証してから書き、失敗時は EditError の index・code・message で返します。"""
    return _edit(_load(source), operations, format=format, max_bytes=max_bytes)


def draft(
    spec: object,
    to: str = "dxf",
    *,
    version: str | None = None,
    sheet: str | None = None,
) -> Converted:
    """図面仕様から to（dxf・dwg・jww・sfc・p21・sfz・p2z）に作図します。

    spec は辞書・JSON 文字列・JSON ファイルのパスです。version で DXF・SXF の版を指定できます。
    JWW・SXF は sheet で 1 枚を選び、省略時は先頭です。誤りは DraftError の path・code で返します。"""
    return _draft(_spec(spec), to, version=version, sheet=sheet)


def draft_into(
    spec: object,
    template: bytes | bytearray | memoryview | str | os.PathLike[str],
    *,
    sheet: str | None = None,
    format: str | None = None,
    max_bytes: int | None = None,
) -> Converted:
    """DXF・DWG・JWW・SXF のテンプレートに、既存内容・ハンドル・拡張データ・形式・版を保って作図します。

    DXF・DWG は同名のレイアウトに描き、なければ追加します。fields は表題欄の属性を埋めます。
    JWW・SXF は sheet で選んだ 1 枚（省略時は先頭）を既存用紙に描き、ビューの縮尺をレイヤグループ・部分図にします。"""
    return _draft_into(_spec(spec), _load(template), sheet=sheet, format=format, max_bytes=max_bytes)


def _spec(spec: object) -> object:
    """辞書・JSON 文字列の図面仕様を返します。パスまたは JSON でない文字列はファイルから読みます。"""
    if isinstance(spec, os.PathLike) or (isinstance(spec, str) and not spec.lstrip().startswith("{")):
        with open(spec, encoding="utf-8") as file:
            return file.read()
    return spec


def draft_schema() -> dict[str, object]:
    """draft が受け取る図面仕様の JSON Schema（draft 2020-12）です。"""
    return json.loads(_draft_schema())


def _markup_json(markup: object) -> str:
    """辞書・JSON 文字列・ファイルパスからマークアップの JSON 文字列を得ます。"""
    if isinstance(markup, os.PathLike) or (isinstance(markup, str) and not markup.lstrip().startswith(("{", "["))):
        with open(markup, encoding="utf-8") as file:
            return file.read()
    if isinstance(markup, (bytes, bytearray, memoryview)):
        return bytes(markup).decode("utf-8")
    return markup if isinstance(markup, str) else json.dumps(markup, ensure_ascii=False)


def read_markup(source: object) -> MarkupDocument:
    """辞書・JSON 文字列・バイト列・ファイルパスからマークアップを読み取り、検証します。
    未知のキーは保ちます。誤りは MarkupError の code・path・message で返します。"""
    return json.loads(_read_markup(_markup_json(source)))


def markup_schema() -> dict[str, object]:
    """マークアップ文書の JSON Schema（draft 2020-12）です。"""
    return json.loads(_markup_schema())


def drawing_hash(source: bytes | bytearray | memoryview | str | os.PathLike[str]) -> str:
    """図面のバイト列またはファイルパスから SHA-256 を小文字の 16 進数で返します。drawing.sha256 に使います。"""
    return _drawing_hash(_load(source))


def carry_markup(
    markup: object,
    before: bytes | bytearray | memoryview | str | os.PathLike[str],
    after: bytes | bytearray | memoryview | str | os.PathLike[str],
    *,
    views: dict[str, str] | None = None,
    tolerance: float = 0.01,
    style: bool = False,
    align: bool = True,
    ignore_hash: bool = False,
    max_bytes: int | None = None,
    max_entities: int | None = None,
    timeout: float | None = None,
) -> MarkupCarry:
    """before に付けたマークアップを新版 after に移します。入力はバイト列またはファイルパスです。

    全レイヤで版を比較し、項目の座標・対象図形 id を移し、unchanged・changed・removed を記録します。
    views でビューを対応付けられます。元の図面のハッシュ不一致は ignore_hash 指定時以外は拒否します。"""
    text, unchanged, changed, removed = _carry_markup(
        _markup_json(markup),
        _load(before),
        _load(after),
        views=views,
        tolerance=tolerance,
        style=style,
        align=align,
        ignore_hash=ignore_hash,
        max_bytes=max_bytes,
        max_entities=max_entities,
        timeout=timeout,
    )
    return {"markup": json.loads(text), "unchanged": unchanged, "changed": changed, "removed": removed}
