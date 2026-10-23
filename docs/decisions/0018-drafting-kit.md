# 0018: 宣言的な図面仕様から、図枠と縮尺付きビューを作図する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-22 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

線や文字を個別に追加するだけでは、図面一式を一貫して作りにくい。

## 選択肢

利用側がすべての操作を書く、描画画像を作る、JSON の図面仕様を専用クレートで処理する方法を比較した。

## 決定

zumen-draft が図枠・表題欄・ビュー・寸法・表・通り芯・記号を AutoCAD モデルへ描く。用紙上の大きさと実寸の座標を分ける。

## 影響

Rust・Python・CLI・MCP から同じ仕様を使える。誤りは場所と code で返し、出力を描画して確認する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0018-drafting-kit.en.md](0018-drafting-kit.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0018-draw-complete-sheets-from-a-declarative-drawing-spec-in-a-drafting-kit-that-builds-the-autocad-drawing-model"></a>

[0018-draw-complete-sheets-from-a-declarative-drawing-spec-in-a-drafting-kit-that-builds-the-autocad-drawing-model](0018-drafting-kit.en.md#0018-draw-complete-sheets-from-a-declarative-drawing-spec-in-a-drafting-kit-that-builds-the-autocad-drawing-model)

<a id="context"></a>

[context](0018-drafting-kit.en.md#context)

<a id="options"></a>

[options](0018-drafting-kit.en.md#options)

<a id="decision"></a>

[decision](0018-drafting-kit.en.md#decision)

<a id="the-spec"></a>

[the-spec](0018-drafting-kit.en.md#the-spec)

<a id="the-goal-for-the-base"></a>

[the-goal-for-the-base](0018-drafting-kit.en.md#the-goal-for-the-base)

<a id="consequences"></a>

[consequences](0018-drafting-kit.en.md#consequences)

<a id="references"></a>

[references](0018-drafting-kit.en.md#references)

</details>
