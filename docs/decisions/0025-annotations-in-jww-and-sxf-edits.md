# 0025: JWW・SXF の編集で部品・寸法・ハッチングを各形式の図形として追加する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

DXF・DWG に追加した編集操作を、日本の JWW・SXF でも使いたい。

## 選択肢

すべてを線と文字に分解する、各形式固有の部品・寸法・ハッチングとして書く方法を比較した。

## 決定

寸法とハッチングの入力解釈を zumen-core に共通化し、形式側で対応する図形へ写す。JWW は設定に従う寸法図形、SXF は対応フィーチャを使う。

## 影響

JWW の角度寸法など形式にないものは明示的に拒否する。複数図形になるハッチングや、編集時の基点・番号などの細則は原記録を参照する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0025-annotations-in-jww-and-sxf-edits.en.md](0025-annotations-in-jww-and-sxf-edits.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0025-add-block-definitions-dimensions-and-hatches-to-jww-and-sxf-edits-as-each-formats-own-entities"></a>

[0025-add-block-definitions-dimensions-and-hatches-to-jww-and-sxf-edits-as-each-formats-own-entities](0025-annotations-in-jww-and-sxf-edits.en.md#0025-add-block-definitions-dimensions-and-hatches-to-jww-and-sxf-edits-as-each-formats-own-entities)

<a id="context"></a>

[context](0025-annotations-in-jww-and-sxf-edits.en.md#context)

<a id="options"></a>

[options](0025-annotations-in-jww-and-sxf-edits.en.md#options)

<a id="decision"></a>

[decision](0025-annotations-in-jww-and-sxf-edits.en.md#decision)

<a id="consequences"></a>

[consequences](0025-annotations-in-jww-and-sxf-edits.en.md#consequences)

<a id="references"></a>

[references](0025-annotations-in-jww-and-sxf-edits.en.md#references)

</details>
