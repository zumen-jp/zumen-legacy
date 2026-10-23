# 0028: IFC を自前の Rust 読み取りで扱い、STEP 構文を SXF と共有する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

BIM と図面の確認を同じ道具で扱いたいが、既存依存のライセンスと ST-Bridge の公開条件に制約がある。

## 選択肢

モデルを対象外にする、既存ライブラリを使う、zumen 内または別リポジトリに自前で実装する方法を比較した。

## 決定

zumen-step に ISO 10303-21 構文を切り出し、zumen-ifc を Rust で実装する案を選ぶ。IFC-SPF の IFC2X3・IFC4・IFC4X3 を対象とし、ST-Bridge は許可が得られるまで含めない。

## 影響

これは計画で、IFC 対応済みを意味しない。階の平面を Scene にして既存の計測・解析を使い、WebAssembly の容量は計測して分離を判断する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0028-ifc-in-rust.en.md](0028-ifc-in-rust.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0028-read-ifc-building-models-with-our-own-reader-in-rust-on-the-iso-10303-21-syntax-shared-with-sxf-and-keep-st-bridge-out-until-buildingsmart-japan-agrees"></a>

[0028-read-ifc-building-models-with-our-own-reader-in-rust-on-the-iso-10303-21-syntax-shared-with-sxf-and-keep-st-bridge-out-until-buildingsmart-japan-agrees](0028-ifc-in-rust.en.md#0028-read-ifc-building-models-with-our-own-reader-in-rust-on-the-iso-10303-21-syntax-shared-with-sxf-and-keep-st-bridge-out-until-buildingsmart-japan-agrees)

<a id="context"></a>

[context](0028-ifc-in-rust.en.md#context)

<a id="options"></a>

[options](0028-ifc-in-rust.en.md#options)

<a id="decision"></a>

[decision](0028-ifc-in-rust.en.md#decision)

<a id="consequences"></a>

[consequences](0028-ifc-in-rust.en.md#consequences)

<a id="references"></a>

[references](0028-ifc-in-rust.en.md#references)

</details>
