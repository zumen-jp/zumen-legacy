# 0016: Worker から Scene の JSON を送り、画面側で復元する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-21 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

大量の小さな型付き配列を構造化複製すると、読み取りより転送が遅くなる。

## 選択肢

Scene の複製、配列の転送、JSON の転送と復元を比較した。

## 決定

読み取り Worker は JSON を返し、画面側が sceneFromJson で復元する。

## 影響

既存の Scene と JSON の契約を使い、大きな図面での受け渡しを速くする。計測結果をもとに変更し、意味や id は変えない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0016-send-worker-scenes-as-json.en.md](0016-send-worker-scenes-as-json.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0016-send-scene-json-from-the-reader-worker-and-decode-it-on-the-page"></a>

[0016-send-scene-json-from-the-reader-worker-and-decode-it-on-the-page](0016-send-worker-scenes-as-json.en.md#0016-send-scene-json-from-the-reader-worker-and-decode-it-on-the-page)

<a id="context"></a>

[context](0016-send-worker-scenes-as-json.en.md#context)

<a id="options"></a>

[options](0016-send-worker-scenes-as-json.en.md#options)

<a id="decision"></a>

[decision](0016-send-worker-scenes-as-json.en.md#decision)

<a id="consequences"></a>

[consequences](0016-send-worker-scenes-as-json.en.md#consequences)

<a id="references"></a>

[references](0016-send-worker-scenes-as-json.en.md#references)

</details>
