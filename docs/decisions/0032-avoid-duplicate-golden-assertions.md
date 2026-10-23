# 0032: 正解ファイルで確認済みの値の重複テストを除く

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-24 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

同じテスト図面と Scene の値を、正解ファイルと単体テストで繰り返し確認していた。

## 選択肢

重複する期待値を維持する、正解ファイルに任せて独立した目的のテストを残す方法を比較した。

## 決定

値を重複確認するテストだけを除く。計測・幾何の不変条件・解析モデル・変更入力・結合部・ブラウザ・言語間の一致・id の独立記録は残す。

## 影響

読み取りと期待値の保守を減らす。正解の再生成だけで正しさは保証されず、幾何と notices の確認は引き続き必要。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0032-avoid-duplicate-golden-assertions.en.md](0032-avoid-duplicate-golden-assertions.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0032-let-golden-tests-own-fixture-scene-values"></a>

[0032-let-golden-tests-own-fixture-scene-values](0032-avoid-duplicate-golden-assertions.en.md#0032-let-golden-tests-own-fixture-scene-values)

<a id="context"></a>

[context](0032-avoid-duplicate-golden-assertions.en.md#context)

<a id="options"></a>

[options](0032-avoid-duplicate-golden-assertions.en.md#options)

<a id="decision"></a>

[decision](0032-avoid-duplicate-golden-assertions.en.md#decision)

<a id="consequences"></a>

[consequences](0032-avoid-duplicate-golden-assertions.en.md#consequences)

<a id="references"></a>

[references](0032-avoid-duplicate-golden-assertions.en.md#references)

</details>
