# 0023: 編集操作で既存レイアウトの用紙サイズを変える

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

テンプレートの用紙サイズを変更するためだけに、図面全体を書き直したくない。

## 選択肢

レイアウトを作り直す、既存レイアウトを編集する方法を比較した。

## 決定

update-layout で既存の用紙サイズと用紙全体のビューポートを更新する。

## 影響

未変更の内容とハンドルを保ち、draft_into の明示的な paper からも利用する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0023-resize-existing-layouts.en.md](0023-resize-existing-layouts.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0023-resize-existing-layouts-through-an-edit-operation"></a>

[0023-resize-existing-layouts-through-an-edit-operation](0023-resize-existing-layouts.en.md#0023-resize-existing-layouts-through-an-edit-operation)

<a id="context"></a>

[context](0023-resize-existing-layouts.en.md#context)

<a id="options"></a>

[options](0023-resize-existing-layouts.en.md#options)

<a id="decision"></a>

[decision](0023-resize-existing-layouts.en.md#decision)

<a id="consequences"></a>

[consequences](0023-resize-existing-layouts.en.md#consequences)

<a id="references"></a>

[references](0023-resize-existing-layouts.en.md#references)

</details>
