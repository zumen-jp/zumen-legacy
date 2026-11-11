# @zumenjs/pdf

PDF 図面のページや、CAD 図面に参照された PDF を、図形の下に描くための部品です。利用側が開いた pdf.js の文書を使います。図形の読み取りは [`@zumenjs/reader`](../reader) が行います。

```bash
npm install @zumenjs/core @zumenjs/viewer @zumenjs/pdf pdfjs-dist
```

`pdfUnderlayRenderer` は `UnderlayRenderer` を作ります。利用側の解決関数から pdf.js の文書を返し、`DrawingViewer` に渡します。文書の作成・Worker の設定・外部ファイルの取得は利用側で行います。

- 必要な範囲を必要な解像度で描画します。用紙と PDF の座標変換は `Underlay` の行列に従います。
- PDF の表示グループ（optional content）は、下敷きの内容レイヤの表示・非表示に合わせます。
- 図形の読み取りや id の生成には pdf.js を使いません。Rust・Python と共通の読み取り結果を保ちます。
- `pdfjs-dist` 5 または 6 が peer dependency です。zumen のパッケージに pdf.js の実装は含みません。
- pdf.js が必要とする CMap・フォント・画像の復号器の配信設定と、それぞれのライセンスは利用側で確認します。

DXF・DWG の PDF の下敷きは、図面がページの大きさを持たないため、読み取り直後は置かれていません（`width`・`height` が 0）。`underlay.file` のファイルを pdf.js で開き、`placePdfUnderlays(scene, { document })` で pdf.js のページの大きさに置いてから表示します。`document` は `pdfUnderlayRenderer` と同じ関数を渡します。開けない文書・ページの下敷きは置かれずに残ります（[決定 0034](../../docs/decisions/0034-place-pdf-underlays-after-reading.md)）。

```ts
import { placePdfUnderlays, pdfUnderlayRenderer } from "@zumenjs/pdf";

// file は図面に書かれた名前（..\plans\plan.pdf など、図面からの相対のことが多い）
const options = { document: (file: string | undefined) => (file === undefined ? null : openPdf(file)) };
viewer.setScene(await placePdfUnderlays(scene, options));
viewer.setUnderlayRenderer(pdfUnderlayRenderer(options));
```

具体的な初期化は[PDF のサンプル](../../playground/pdf-sample.ts)、設計理由は[決定 0029](../../docs/decisions/0029-pdf-js-owned-by-the-application.md)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjspdf"></a>

[zumenjspdf](README.en.md#zumenjspdf)

<a id="what-it-draws"></a>

[what-it-draws](README.en.md#what-it-draws)

<a id="options"></a>

[options](README.en.md#options)

<a id="layers-from-optional-content"></a>

[layers-from-optional-content](README.en.md#layers-from-optional-content)

<a id="images-and-pdf-pages-together"></a>

[images-and-pdf-pages-together](README.en.md#images-and-pdf-pages-together)

<a id="pdfjs-and-bundlers"></a>

[pdfjs-and-bundlers](README.en.md#pdfjs-and-bundlers)

<a id="network"></a>

[network](README.en.md#network)

</details>
