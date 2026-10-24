# 0002: PDF は用途で分担し、図面は zumen、文書は ParaPDF に置く

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-19 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

CAD と PDF の図面を同じ操作で計測・選択したい一方、汎用文書の閲覧とは要求が違う。

## 選択肢

自前の PDF 描画、PDF を対象外にする、用途で役割を分ける、を比較した。

## 決定

図面は zumen、仕様書・報告書・契約書などの文書は ParaPDF とする。PDF の見た目は PDF エンジンが描き、zumen は選択・吸着・計測・レイヤ・マークアップを重ねる。

## 影響

図面か文書かは利用側が判断する。下敷きと描かない図形を共通モデルに追加する。PDF の図形読み取りは別の判断で具体化する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0002-pdf-and-parapdf.en.md](0002-pdf-and-parapdf.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0002-divide-pdf-work-with-parapdf-by-use-drawings-in-zumen-documents-in-parapdf"></a>

[0002-divide-pdf-work-with-parapdf-by-use-drawings-in-zumen-documents-in-parapdf](0002-pdf-and-parapdf.en.md#0002-divide-pdf-work-with-parapdf-by-use-drawings-in-zumen-documents-in-parapdf)

<a id="context"></a>

[context](0002-pdf-and-parapdf.en.md#context)

<a id="options"></a>

[options](0002-pdf-and-parapdf.en.md#options)

<a id="decision"></a>

[decision](0002-pdf-and-parapdf.en.md#decision)

<a id="consequences"></a>

[consequences](0002-pdf-and-parapdf.en.md#consequences)

<a id="references"></a>

[references](0002-pdf-and-parapdf.en.md#references)

</details>
