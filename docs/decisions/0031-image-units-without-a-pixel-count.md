# 0031: 画素数のない画像は用紙上の 0.01 mm を内容単位にする

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

JWW・SXF は画像の配置サイズを持つが、画素数を記録しない。mm を画素とみなすと拡大時にぼける。

## 選択肢

mm 単位のまま、専用フラグの追加、細かい共通単位、外部画像を読み取る方法を比較した。

## 決定

JWW・SXF の下敷きの width・height は用紙上の 0.01 mm 単位とし、matrix で用紙へ写す。DXF・DWG は記録された画素数を使う。

## 影響

Scene の構造を増やさず、表示解像度で描画範囲を要求できる。JWW・SXF の width・height は画素数ではない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0031-image-units-without-a-pixel-count.en.md](0031-image-units-without-a-pixel-count.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0031-measure-images-whose-drawing-records-no-pixel-count-in-hundredths-of-a-sheet-millimeter"></a>

[0031-measure-images-whose-drawing-records-no-pixel-count-in-hundredths-of-a-sheet-millimeter](0031-image-units-without-a-pixel-count.en.md#0031-measure-images-whose-drawing-records-no-pixel-count-in-hundredths-of-a-sheet-millimeter)

<a id="context"></a>

[context](0031-image-units-without-a-pixel-count.en.md#context)

<a id="options"></a>

[options](0031-image-units-without-a-pixel-count.en.md#options)

<a id="decision"></a>

[decision](0031-image-units-without-a-pixel-count.en.md#decision)

<a id="consequences"></a>

[consequences](0031-image-units-without-a-pixel-count.en.md#consequences)

<a id="references"></a>

[references](0031-image-units-without-a-pixel-count.en.md#references)

</details>
