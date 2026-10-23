# 0027: JWW・SXF テンプレートへ編集操作を通して作図する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

会社や受領図面の用紙・既存図形を保って、新しい図形とビューを足したい。

## 選択肢

変換済みモデルのコピー、形式別の編集操作への変換を比較した。

## 決定

plan_jww・plan_sxf が作図結果を操作へ変換する。JWW は縮尺ごとのレイヤグループ、SXF は部分図へビューを置く。

## 影響

用紙は 1 枚を選び、テンプレートの大きさを保つ。空きレイヤ・グループ不足は失敗にし、部分的に書かない。JWC・JWS は編集対象外。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0027-draft-into-jww-and-sxf.en.md](0027-draft-into-jww-and-sxf.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0027-draw-a-drawing-spec-into-jww-and-sxf-templates-through-their-edits-with-views-as-layer-groups-and-sub-figures"></a>

[0027-draw-a-drawing-spec-into-jww-and-sxf-templates-through-their-edits-with-views-as-layer-groups-and-sub-figures](0027-draft-into-jww-and-sxf.en.md#0027-draw-a-drawing-spec-into-jww-and-sxf-templates-through-their-edits-with-views-as-layer-groups-and-sub-figures)

<a id="context"></a>

[context](0027-draft-into-jww-and-sxf.en.md#context)

<a id="options"></a>

[options](0027-draft-into-jww-and-sxf.en.md#options)

<a id="decision"></a>

[decision](0027-draft-into-jww-and-sxf.en.md#decision)

<a id="consequences"></a>

[consequences](0027-draft-into-jww-and-sxf.en.md#consequences)

<a id="references"></a>

[references](0027-draft-into-jww-and-sxf.en.md#references)

</details>
