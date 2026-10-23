# 0009: AI 向けに描いた形と座標を返し、意味の判断は利用側に任せる

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

属性や画像だけでは、線や領域の幾何を利用側で計算しにくい。

## 選択肢

形式固有の定義をすべて公開する、画像だけ返す、描画形状を共通化して返す方法を比較した。

## 決定

図形の描画順の形を、用紙上の mm と配置内の実寸の mm で返す。曲線は描画時の折線とし、中心・半径などの定義は属性に置く。

## 影響

形式によらず計算できる。形から壁・建具などの意味を判断する責任は利用側に残し、返却点数に上限を設ける。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0009-geometry-for-ai-clients.en.md](0009-geometry-for-ai-clients.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0009-give-ai-clients-the-shapes-drawn-in-paper-and-real-world-millimeters-and-leave-their-meaning-to-them"></a>

[0009-give-ai-clients-the-shapes-drawn-in-paper-and-real-world-millimeters-and-leave-their-meaning-to-them](0009-geometry-for-ai-clients.en.md#0009-give-ai-clients-the-shapes-drawn-in-paper-and-real-world-millimeters-and-leave-their-meaning-to-them)

<a id="context"></a>

[context](0009-geometry-for-ai-clients.en.md#context)

<a id="options"></a>

[options](0009-geometry-for-ai-clients.en.md#options)

<a id="decision"></a>

[decision](0009-geometry-for-ai-clients.en.md#decision)

<a id="consequences"></a>

[consequences](0009-geometry-for-ai-clients.en.md#consequences)

<a id="references"></a>

[references](0009-geometry-for-ai-clients.en.md#references)

</details>
