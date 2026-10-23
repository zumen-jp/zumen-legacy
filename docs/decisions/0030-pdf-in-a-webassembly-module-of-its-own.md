# 0030: PDF の WebAssembly を分離し、PDF を開いたときだけ読む

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

PDF の読み取り・フォント・CMap を含めると、CAD だけの利用でも転送量が大きくなる。

## 選択肢

単一モジュール、フォントの削減、全形式入りの別モジュール、PDF 専用モジュールを比較した。

## 決定

bindings/wasm-pdf を追加し、reader.load(format) が PDF のときだけ読み込む。CAD のモジュールは既存のままにする。

## 影響

2 つの wasm と初期化関数を提供し、Worker は pdfWasm の配置も受け取る。両方を使う場合は共通コードの重複を許容する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0030-pdf-in-a-webassembly-module-of-its-own.en.md](0030-pdf-in-a-webassembly-module-of-its-own.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0030-read-pdf-in-browsers-with-a-webassembly-module-of-its-own-loaded-only-for-pdf-drawings"></a>

[0030-read-pdf-in-browsers-with-a-webassembly-module-of-its-own-loaded-only-for-pdf-drawings](0030-pdf-in-a-webassembly-module-of-its-own.en.md#0030-read-pdf-in-browsers-with-a-webassembly-module-of-its-own-loaded-only-for-pdf-drawings)

<a id="context"></a>

[context](0030-pdf-in-a-webassembly-module-of-its-own.en.md#context)

<a id="options"></a>

[options](0030-pdf-in-a-webassembly-module-of-its-own.en.md#options)

<a id="decision"></a>

[decision](0030-pdf-in-a-webassembly-module-of-its-own.en.md#decision)

<a id="consequences"></a>

[consequences](0030-pdf-in-a-webassembly-module-of-its-own.en.md#consequences)

<a id="references"></a>

[references](0030-pdf-in-a-webassembly-module-of-its-own.en.md#references)

</details>
