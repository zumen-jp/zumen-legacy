# 0026: JWW・SXF のスタイル操作を、各形式が保持できる値へ写す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

名前付きスタイルの仕組みがない形式でも、共通の作図操作から文字・寸法・線種を指定したい。

## 選択肢

図面全体の設定を書き換える、編集内だけ名前を持つ、操作を拒否する方法を比較した。

## 決定

入力解釈を共通化し、寸法スタイルは編集内で保持して各図形の値に展開する。文字と線種は形式の定義へ写し、近似は notices で報告する。

## 影響

再読込で名前付きスタイルが戻らない場合がある。scale は DXF の DIMSCALE と整合させ、実際の CAD で未確認の規則を明記する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0026-styles-in-jww-and-sxf-edits.en.md](0026-styles-in-jww-and-sxf-edits.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0026-let-jww-and-sxf-edits-add-dimension-styles-text-styles-and-line-types-named-for-the-edit-and-written-into-what-each-format-stores"></a>

[0026-let-jww-and-sxf-edits-add-dimension-styles-text-styles-and-line-types-named-for-the-edit-and-written-into-what-each-format-stores](0026-styles-in-jww-and-sxf-edits.en.md#0026-let-jww-and-sxf-edits-add-dimension-styles-text-styles-and-line-types-named-for-the-edit-and-written-into-what-each-format-stores)

<a id="context"></a>

[context](0026-styles-in-jww-and-sxf-edits.en.md#context)

<a id="options"></a>

[options](0026-styles-in-jww-and-sxf-edits.en.md#options)

<a id="decision"></a>

[decision](0026-styles-in-jww-and-sxf-edits.en.md#decision)

<a id="consequences"></a>

[consequences](0026-styles-in-jww-and-sxf-edits.en.md#consequences)

<a id="references"></a>

[references](0026-styles-in-jww-and-sxf-edits.en.md#references)

</details>
