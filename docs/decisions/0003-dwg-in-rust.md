# 0003: DWG を自前の Rust 実装で AutoCAD の共通モデルに読む

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-19 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

DWG は大きなバイナリ形式で、二重実装と互換性の維持が負担になる。

## 選択肢

TypeScript と Rust の二重実装、既存の寛容なライブラリ、自前の Rust 実装を比較した。

## 決定

zumen-dwg で公開仕様に従うデコーダを実装し、zumen-dxf の DxfDocument に読む。描画は dxf_to_scene を共用し、LibreDWG のソースは使わない。

## 影響

形式クレート同士の依存の例外になる。同じ図面の DXF、独立した実装、最小の合成入力で確かめ、仕様外の推測を記録する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0003-dwg-in-rust.en.md](0003-dwg-in-rust.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0003-read-dwg-in-rust-only-with-our-own-decoder-into-the-autocad-drawing-model-of-zumen-dxf"></a>

[0003-read-dwg-in-rust-only-with-our-own-decoder-into-the-autocad-drawing-model-of-zumen-dxf](0003-dwg-in-rust.en.md#0003-read-dwg-in-rust-only-with-our-own-decoder-into-the-autocad-drawing-model-of-zumen-dxf)

<a id="context"></a>

[context](0003-dwg-in-rust.en.md#context)

<a id="options"></a>

[options](0003-dwg-in-rust.en.md#options)

<a id="decision"></a>

[decision](0003-dwg-in-rust.en.md#decision)

<a id="consequences"></a>

[consequences](0003-dwg-in-rust.en.md#consequences)

<a id="references"></a>

[references](0003-dwg-in-rust.en.md#references)

<a id="implementation-update-2026-10-20"></a>

[implementation-update-2026-10-20](0003-dwg-in-rust.en.md#implementation-update-2026-10-20)

</details>
