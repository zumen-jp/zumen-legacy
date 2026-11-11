# 0034: DXF・DWG の PDF の下敷きは置かずに読み、描く PDF エンジンのページの大きさで置く

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-24 |
| 状態 | 提案 |

## 背景

DXF・DWG の PDF の下敷き（PDFUNDERLAY、DXF Reference「UNDERLAY」）は、ページの置き方として挿入点（ページの左下の角）・縮尺・回転・法線・表示のフラグ・切り抜きの境界を持ち、その定義（PDFDEFINITION）はファイル（1）とページ（2）を指す。どちらもページの大きさを持たない。[決定 0022](0022-underlays-drawn-by-a-region-renderer.md) は下敷きの内容の原点を左上に置くので、matrix にはページの高さが、内容の大きさ・範囲・`bbox`・`Scene.bounds` には幅と高さが要る。境界の内側を消す切り抜き（280 の 16）はページの外形も要る。これまで読み取りは PDF の下敷きを `skipped` に数えていた（[issue #82](https://github.com/zumenjs/zumen/issues/82)）。

制約:

- ページの大きさは PDF のファイルだけが、表示する形で持つ（`/Rotate` を済ませ、クロップボックスの中で、pdf.js はメディアボックスとの共通部分を使い、`/UserUnit` を掛ける）。
- pdf.js は利用側が持ち、`pdfUnderlayRenderer` が描く文書を開く（[決定 0029](0029-pdf-js-owned-by-the-application.md)）。読み取りはほかのファイルを開かない。ブラウザにはファイルシステムがなく、ライブラリは通信せず、形式クレートは `zumen-core` にだけ依存する。
- ブラウザでは読み取りがワーカーで動き、Scene の JSON を送る（[決定 0016](0016-send-worker-scenes-as-json.md)）。読み直すと最初の読み取りと同じだけかかり、大きな DWG では数秒になる。
- Scene は TypeScript と Rust で同じで、同じバイト列は同じ id になる。

## 選択肢

1. ページの大きさ（`file`・`page`・`width`・`height`）を読み取りの引数で渡し、利用側が PDF を開いた後に読み直す。読み取りがすべて（枠の図形、内側を消す切り抜き、`Scene.bounds`）を置けるが、どのファイルとページが要るかを知るために一度読む必要があり、PDF の下敷きを持つ図面は毎回 2 回読む。引数は `ReadOptions`・`zumen::read`・WebAssembly・ワーカー・Python まで通す必要がある。
2. 利用側が渡す PDF のバイト列から、読み取りがページの大きさを読む。PDF の解析（形式クレートが依存できない `zumen-pdf` と、ブラウザではその WebAssembly）が要り、回転・ボックス・UserUnit を描く pdf.js とは別にもう一度計算する。
3. 読み取りは置く前の下敷きを返し、利用側がページの大きさを知ってから関数で置く。読むのは 1 回で、大きさはページを描く pdf.js の文書から得る。読み取りの結果は図面だけで決まる。
4. viewer が描画部品に内容の大きさを尋ね（`UnderlayRenderer` の新しいメソッド）、自分で置く。利用側のコードは要らないが、利用側の Scene の形（`bbox`・`Scene.bounds`）を viewer が裏で変え、解析やサーバーは viewer と違う Scene を見る。
5. ページの大きさを仮定する。利用側に何も知らせずに位置がずれる。

## 決定

3 つ目を選ぶ。

- DXF・DWG の読み取りは、表示が入っている（280 の 2）PDFUNDERLAY を `kind: "pdf"` の下敷きにする。`file` は PDFDEFINITION の 1（図面に書かれたまま）、`page` は 2（1 から）で、`width`・`height` は 0（大きさがまだ分からない下敷き）にする。`matrix` は高さ 0 のページのものにする。高さ h のページの内容の点 (x, y) は `matrix` · (x, y − h) にあり、ページの左下の角（挿入点）は `matrix` · (0, 0) にある。内容の単位は表示するページの pt で、縮尺 1 で 1 pt を図面の 1/72 単位にする（PDF の 1 インチが図面の 1 単位。推測。[DXF の PDF の下敷き](../formats/dxf.md#pdf-underlays)）。
- `clip` は画像と同じく用紙の mm の輪（切り抜きの境界、ビューポート）。空の輪は、まだ分からないページの外形を表す。ビューポートの外で境界の内側を消す切り抜きは、空の輪と境界にする。境界のないページは、それを見せるビューポートの矩形で切る。`bbox` は、切り抜きがページを囲むとき（境界かビューポート）はその外接矩形、そうでなければ挿入点にする。大きさの分からないページの枠は描けないので、枠の図形は作らない。
- `@zumenjs/core` の `placeUnderlays(scene, size)` は、`width`・`height` が 0 で `size(underlay)` が大きさを返す下敷きを置く。`width`・`height` を決め、matrix を内容の Y の向きに高さだけ動かし、`clip` の空の輪を内容の外形にし、`bbox` を切り抜きの外接矩形の中の内容の外接矩形にして、`Scene.bounds` を広げる。内容が切り抜きの外にある下敷きは除く。ほかの下敷きはそのままの新しい Scene を返し、置くものがなければ同じ Scene を返す。DXF の知識は持たず、内容の左下の角で置く下敷きなら何にでも使える。
- `@zumenjs/pdf` の `placePdfUnderlays(scene, { document })` は、`pdfUnderlayRenderer` と同じ `document` 関数で開いた pdf.js の文書から、ページの大きさ（`page.getViewport({ scale: 1 })`）を得る。置き方と描き方が回転・ボックス・UserUnit で食い違わない。文書やページを開けない下敷きは置かずに残す。
- 描画は大きさのない下敷きを描かない。viewer はもともと飛ばし、`zumen-render` は `underlay-not-drawn` に数える。

## 影響

- PDF の下敷きを持たない図面の Scene・正解ファイル・id は変わらない。PDF の下敷きを持つ図面は下敷きが増え、`skipped` から PDFUNDERLAY が消える。図形は増えないので、図形の id は変わらない。
- PDF の下敷きを描く利用側は、`underlay.file` のファイルを開き、読み取りの後に `placePdfUnderlays` を呼んで、置いた Scene を viewer に渡す。それまではページを描かず、`Scene.bounds` もページを含まない。試用画面は、利用者が図面の隣で選んだ PDF でこれを行う。
- 枠（PDFFRAME）、下敷きのレイヤ（図面が切り替える optional content）、AutoCAD が反映する表示の値（コントラスト・単色・背景に合わせた色）は描かず、報告する。PDF の下敷きを選ぶには図形が要り、ページとともに枠を置けるようになったら追加できる。
- Python・CLI・`zumen-render` は置く前の下敷きを見る。`zumen-render` が図面の指す PDF のバイト列を受け取るようになったら、hayro からページの大きさを得て、同じ規則で `zumen-core` で置く。
- 図面モデルからの書き出し（DXF・DWG と、SXF・JWW への変換）は PDF の下敷きを書かず、報告する。これまでも読み取りの時点で失っていた。既存図面の編集は、触らないほかの図形と同じく残す。

## 参照

- [決定 0022: 下敷きを範囲を描く描画部品で描く](0022-underlays-drawn-by-a-region-renderer.md)
- [決定 0029: pdf.js の文書を利用側で開く](0029-pdf-js-owned-by-the-application.md)
- [issue #82: 読み取りが飛ばしている画像を描く](https://github.com/zumenjs/zumen/issues/82)
- [DXF の PDF の下敷き](../formats/dxf.md#pdf-underlays)
- [AutoCAD DXF Reference: UNDERLAY](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-3EC8FBCC-A85A-4B0B-93CD-C6C785959077.htm)、[UNDERLAYDEFINITION](https://help.autodesk.com/cloudhelp/2026/ENU/AutoCAD-DXF/files/GUID-A4FF15D3-F745-4E1F-94D4-1DC3DF297B0F.htm)
- [pdf.js `PDFPageProxy.getViewport`](https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib-PDFPageProxy.html)

## 英語の補足資料

同じ内容の英語版を [0034-place-pdf-underlays-after-reading.en.md](0034-place-pdf-underlays-after-reading.en.md) に置いています。
