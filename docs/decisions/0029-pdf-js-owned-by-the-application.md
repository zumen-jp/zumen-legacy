# 0029: pdf.js の文書を利用側で開き、描画部品へ渡す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

Worker・バンドラー・文書の寿命・別の PDF ビューアと共有する条件はアプリによって違う。

## 選択肢

パッケージが文書を開く、モジュールとバイト列を受ける、開いた文書を受ける方法を比較した。

## 決定

pdfUnderlayRenderer は利用側の PDFDocumentProxy を受けて描く。pdfjs-dist は型だけの peer dependency とし、文書の作成・破棄・Worker・配信データは利用側が管理する。

## 影響

pdf.js の重複を避け、配信ファイルのライセンスも利用側が選べる。新しいメジャー版の対応は描画テスト後に追加する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0029-pdf-js-owned-by-the-application.en.md](0029-pdf-js-owned-by-the-application.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0029-let-the-application-own-pdfjs-and-draw-the-pdfjs-documents-it-opens"></a>

[0029-let-the-application-own-pdfjs-and-draw-the-pdfjs-documents-it-opens](0029-pdf-js-owned-by-the-application.en.md#0029-let-the-application-own-pdfjs-and-draw-the-pdfjs-documents-it-opens)

<a id="context"></a>

[context](0029-pdf-js-owned-by-the-application.en.md#context)

<a id="options"></a>

[options](0029-pdf-js-owned-by-the-application.en.md#options)

<a id="decision"></a>

[decision](0029-pdf-js-owned-by-the-application.en.md#decision)

<a id="consequences"></a>

[consequences](0029-pdf-js-owned-by-the-application.en.md#consequences)

<a id="references"></a>

[references](0029-pdf-js-owned-by-the-application.en.md#references)

</details>
