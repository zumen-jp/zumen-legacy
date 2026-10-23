# 0008: モデル空間を実寸にし、用紙は縮尺ごとのビューポートで表示する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

用紙の見た目だけをモデル空間に写すと、計測や寸法が実寸にならない。

## 選択肢

用紙をモデル空間にする、主縮尺だけ実寸にする、すべて実寸にして紙空間で表示する方法を比較した。

## 決定

各縮尺の図形をモデル空間の実寸で置き、用紙のレイアウトにビューポートを作る。図枠・表題欄は紙空間に置く。SXF の異なる縦横縮尺は紙空間のブロック配置とする。

## 影響

変換後も実寸で計測できる。モデル空間しか表示しないアプリでは図枠が見えない。重なる配置の移動など、元の座標を変える処理は報告する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0008-sheets-and-scales-in-the-autocad-model.en.md](0008-sheets-and-scales-in-the-autocad-model.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0008-real-size-model-space-and-the-sheet-as-a-layout-with-a-viewport-for-each-scale"></a>

[0008-real-size-model-space-and-the-sheet-as-a-layout-with-a-viewport-for-each-scale](0008-sheets-and-scales-in-the-autocad-model.en.md#0008-real-size-model-space-and-the-sheet-as-a-layout-with-a-viewport-for-each-scale)

<a id="context"></a>

[context](0008-sheets-and-scales-in-the-autocad-model.en.md#context)

<a id="options"></a>

[options](0008-sheets-and-scales-in-the-autocad-model.en.md#options)

<a id="decision"></a>

[decision](0008-sheets-and-scales-in-the-autocad-model.en.md#decision)

<a id="consequences"></a>

[consequences](0008-sheets-and-scales-in-the-autocad-model.en.md#consequences)

<a id="references"></a>

[references](0008-sheets-and-scales-in-the-autocad-model.en.md#references)

</details>
