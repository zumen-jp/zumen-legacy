# 0001: ST-Bridge を TypeScript パッケージとして扱う

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-19 |
| 状態 | Superseded by [0013](0013-st-bridge-out-of-zumen.md) |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

構造モデルのビューアと、将来の平面図・立面図をどう配置するか。

## 選択肢

別リポジトリで Rust 化する、zumen 内に TypeScript として置く、公開しない、を比較した。

## 決定

当初は @zumenjs/stb を zumen 内に置き、解析・three.js・React の入口を分ける方針を選んだ。Rust への移植は行わない。

## 影響

公開条件の確認が必要だった。現在は決定 0013 に置き換えられており、採用方針として使わない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0001-st-bridge-in-zumen.en.md](0001-st-bridge-in-zumen.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0001-keep-st-bridge-structural-models-in-zumen-as-a-typescript-package"></a>

[0001-keep-st-bridge-structural-models-in-zumen-as-a-typescript-package](0001-st-bridge-in-zumen.en.md#0001-keep-st-bridge-structural-models-in-zumen-as-a-typescript-package)

<a id="context"></a>

[context](0001-st-bridge-in-zumen.en.md#context)

<a id="options"></a>

[options](0001-st-bridge-in-zumen.en.md#options)

<a id="decision"></a>

[decision](0001-st-bridge-in-zumen.en.md#decision)

<a id="consequences"></a>

[consequences](0001-st-bridge-in-zumen.en.md#consequences)

<a id="references"></a>

[references](0001-st-bridge-in-zumen.en.md#references)

</details>
