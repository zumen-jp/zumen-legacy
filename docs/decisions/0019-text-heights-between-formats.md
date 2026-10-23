# 0019: 形式間で文字高の数値を保ち、字形の描き方は各形式に任せる

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

DXF・DWG の文字高と SXF・JWW の文字枠高は、同じ数値でも字形の見え方が違う。

## 選択肢

見た目の高さを揃える、各形式の数値を保つ方法を比較した。

## 決定

変換で文字高の数値を保ち、em との換算は各形式の描画規則で扱う。

## 影響

再変換で数値が変わらず、CAD の属性と整合する。見た目には形式の規則による差があることを記録する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0019-text-heights-between-formats.en.md](0019-text-heights-between-formats.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0019-keep-the-number-of-a-text-height-between-dxf-dwg-sxf-and-jww-and-let-each-format-draw-its-glyphs"></a>

[0019-keep-the-number-of-a-text-height-between-dxf-dwg-sxf-and-jww-and-let-each-format-draw-its-glyphs](0019-text-heights-between-formats.en.md#0019-keep-the-number-of-a-text-height-between-dxf-dwg-sxf-and-jww-and-let-each-format-draw-its-glyphs)

<a id="context"></a>

[context](0019-text-heights-between-formats.en.md#context)

<a id="options"></a>

[options](0019-text-heights-between-formats.en.md#options)

<a id="decision"></a>

[decision](0019-text-heights-between-formats.en.md#decision)

<a id="consequences"></a>

[consequences](0019-text-heights-between-formats.en.md#consequences)

<a id="references"></a>

[references](0019-text-heights-between-formats.en.md#references)

</details>
