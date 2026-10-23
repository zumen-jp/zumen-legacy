# 0021: マークアップを別 JSON に置き、比較結果で新版へ移す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

図面への指摘や赤入れを、元ファイルを変更せず共有・継承したい。

## 選択肢

図面に直接書く、画像だけ保存する、座標付きの別文書にする方法を比較した。

## 決定

図面の SHA-256 とビューを指す JSON に、赤入れ・変更雲・注記・指摘・写真位置を用紙上の mm で保存する。compare で新版へ位置と図形参照を移す。

## 影響

未知のキーを保持し、利用側が担当者・期限などを追加できる。移行状態を unchanged・changed・removed で示し、図面との不一致を検知する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0021-drawing-markup.en.md](0021-drawing-markup.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0021-keep-drawing-markup-in-a-json-document-beside-the-drawing-in-paper-millimeters-of-a-view-and-carry-it-to-new-revisions-with-compare"></a>

[0021-keep-drawing-markup-in-a-json-document-beside-the-drawing-in-paper-millimeters-of-a-view-and-carry-it-to-new-revisions-with-compare](0021-drawing-markup.en.md#0021-keep-drawing-markup-in-a-json-document-beside-the-drawing-in-paper-millimeters-of-a-view-and-carry-it-to-new-revisions-with-compare)

<a id="context"></a>

[context](0021-drawing-markup.en.md#context)

<a id="options"></a>

[options](0021-drawing-markup.en.md#options)

<a id="decision"></a>

[decision](0021-drawing-markup.en.md#decision)

<a id="the-document"></a>

[the-document](0021-drawing-markup.en.md#the-document)

<a id="references-to-the-drawing"></a>

[references-to-the-drawing](0021-drawing-markup.en.md#references-to-the-drawing)

<a id="revisions"></a>

[revisions](0021-drawing-markup.en.md#revisions)

<a id="where-it-lives"></a>

[where-it-lives](0021-drawing-markup.en.md#where-it-lives)

<a id="python-the-command-line-and-the-mcp-server"></a>

[python-the-command-line-and-the-mcp-server](0021-drawing-markup.en.md#python-the-command-line-and-the-mcp-server)

<a id="consequences"></a>

[consequences](0021-drawing-markup.en.md#consequences)

<a id="references"></a>

[references](0021-drawing-markup.en.md#references)

</details>
