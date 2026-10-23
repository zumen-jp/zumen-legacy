# 0015: 共通モデルから領域・表・製図基準を解析する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-21 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

部屋の面積、建具表、電子納品の製図規則を形式共通で扱いたい。

## 選択肢

形式ごとに解析する、AI にすべて任せる、共通の Scene から幾何と規則で解析する方法を比較した。

## 決定

zumen-core に閉領域、罫線の表、JSON 規則集による製図基準の確認を置く。意味の推定は限定し、結果と未確認の対象を区別する。

## 影響

Rust・Python・CLI・MCP で同じ結果を使える。規則の違反なしと、全項目を確認できたことを区別する。文字高の推定部分は決定 0024 に置き換える。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0015-analysis-in-zumen-core.en.md](0015-analysis-in-zumen-core.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0015-analyze-drawings-in-zumen-core-from-the-scene-closed-regions-first-then-tables-then-checks-against-cad-standards"></a>

[0015-analyze-drawings-in-zumen-core-from-the-scene-closed-regions-first-then-tables-then-checks-against-cad-standards](0015-analysis-in-zumen-core.en.md#0015-analyze-drawings-in-zumen-core-from-the-scene-closed-regions-first-then-tables-then-checks-against-cad-standards)

<a id="context"></a>

[context](0015-analysis-in-zumen-core.en.md#context)

<a id="options"></a>

[options](0015-analysis-in-zumen-core.en.md#options)

<a id="decision"></a>

[decision](0015-analysis-in-zumen-core.en.md#decision)

<a id="consequences"></a>

[consequences](0015-analysis-in-zumen-core.en.md#consequences)

<a id="references"></a>

[references](0015-analysis-in-zumen-core.en.md#references)

</details>
