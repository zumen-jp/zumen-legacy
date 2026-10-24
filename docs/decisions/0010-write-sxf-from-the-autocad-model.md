# 0010: AutoCAD モデルから SXF を書き、非対応の図形は描画要素へ写す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

SXF と他形式の変換に、すでに共通の AutoCAD モデルがある。

## 選択肢

Scene から書く、形式ごとの直接変換、共通モデルから書く方法を比較した。

## 決定

レイアウトを用紙、ビューポートを実寸の部分図、ブロックを作図部品に写す。SXF にない複数行文字・引出線・寸法などは描画する線・塗り・文字へ写す。

## 影響

変換後の見た目と縮尺を比較できる。元形式の意味や編集単位を保てないものは notices で報告する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0010-write-sxf-from-the-autocad-model.en.md](0010-write-sxf-from-the-autocad-model.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0010-write-sxf-from-the-autocad-drawing-model-with-viewports-as-sub-figures-and-drawn-shapes-for-what-sxf-lacks"></a>

[0010-write-sxf-from-the-autocad-drawing-model-with-viewports-as-sub-figures-and-drawn-shapes-for-what-sxf-lacks](0010-write-sxf-from-the-autocad-model.en.md#0010-write-sxf-from-the-autocad-drawing-model-with-viewports-as-sub-figures-and-drawn-shapes-for-what-sxf-lacks)

<a id="context"></a>

[context](0010-write-sxf-from-the-autocad-model.en.md#context)

<a id="options"></a>

[options](0010-write-sxf-from-the-autocad-model.en.md#options)

<a id="decision"></a>

[decision](0010-write-sxf-from-the-autocad-model.en.md#decision)

<a id="consequences"></a>

[consequences](0010-write-sxf-from-the-autocad-model.en.md#consequences)

<a id="references"></a>

[references](0010-write-sxf-from-the-autocad-model.en.md#references)

</details>
