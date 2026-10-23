# 0024: 図面が宣言する文字高を描画要素ごとに保持する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

em から文字枠高を推定すると、寸法内や書式付き文字の高さを正しく確認できない。

## 選択肢

形式ごとの逆算、属性からの復元、描画要素に宣言値を持たせる方法を比較した。

## 決定

TextPrimitive.characterHeight を任意項目として追加し、未指定時だけ height を使う。描画には従来どおり height を使う。

## 影響

製図基準の確認が形式に依存しない。既存の Scene の扱いと id は変えず、決定 0015 の文字高推定だけを置き換える。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0024-declared-text-height.en.md](0024-declared-text-height.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0024-keep-the-declared-text-height-with-each-text-primitive"></a>

[0024-keep-the-declared-text-height-with-each-text-primitive](0024-declared-text-height.en.md#0024-keep-the-declared-text-height-with-each-text-primitive)

<a id="context"></a>

[context](0024-declared-text-height.en.md#context)

<a id="options"></a>

[options](0024-declared-text-height.en.md#options)

<a id="decision"></a>

[decision](0024-declared-text-height.en.md#decision)

<a id="consequences"></a>

[consequences](0024-declared-text-height.en.md#consequences)

<a id="references"></a>

[references](0024-declared-text-height.en.md#references)

</details>
