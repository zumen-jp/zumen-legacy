# @zumenjs/reader

SXF（SFC・P21・SFZ・P2Z）・JWW（JWC・JWS を含む）・DXF・DWG・PDF を、ブラウザと Node.js で読み取ります。Rust・Python・CLI と同じ Rust の読み取りを WebAssembly で動かすため、環境が違っても同じ図面モデルを返します。

## 読み取り

```ts
import { readDrawing } from "@zumenjs/core";
import { initReader, reader } from "@zumenjs/reader";

await initReader(); // 初回だけ WebAssembly を読み込みます
const scene = readDrawing(bytes, [reader]); // ファイルの中身から形式を判定します
const layout = readDrawing(bytes, [reader], { view: "Layout1" });
```

PDF は専用モジュールを必要なときだけ読み込みます。

```ts
import { initPdfReader, reader } from "@zumenjs/reader";

await initPdfReader(); // reader.load("pdf") でも同じです
const page = readDrawing(pdfBytes, [reader], { view: "2" }); // 2 ページ目
```

## WebAssembly の配置

`initReader()` はモジュールからの相対位置 `wasm/zumen_wasm_bg.wasm` を読み込みます。ブラウザは取得し、Node.js はファイルを読みます。Vite・webpack 5 などは `new URL(…, import.meta.url)` に従ってファイルをコピーします。

自動で配置されない場合は `@zumenjs/reader/zumen_wasm_bg.wasm` を配信し、その URL・`Response`・バイト列・コンパイル済み `WebAssembly.Module` を渡してください。PDF は `@zumenjs/reader/zumen_wasm_pdf_bg.wasm` を `initPdfReader()` に渡します。同じ初期化の呼び出しは同じ Promise を返し、失敗後の呼び出しは再試行します。

`detectDrawing` と `reader.detect` は WebAssembly を読み込まずに形式を判定します。`reader.load(format)` は PDF の場合だけ PDF モジュールを読み、React の `useDrawing` はこれを待ってから読み取ります。初期化前の読み取りは日本語のエラーを返します。

図面は端末に留まり、読み取りのための通信は WebAssembly の取得だけです。CSP の `script-src` には `'wasm-unsafe-eval'` が必要です。

## 読み取りの上限

```ts
import { createReader, LimitError } from "@zumenjs/reader";

const limited = createReader({ limits: { maxBytes: 50_000_000, maxEntities: 500_000 } });
try {
  const scene = readDrawing(bytes, [limited]);
} catch (error) {
  if (error instanceof LimitError) console.log(error.limit); // bytes・entities・time
}
```

`maxBytes` は P2Z・SFZ の展開後のバイト数、`maxEntities` は図形数です。超えた時点で読み取りを停止します。`LimitError.limit` は機械処理用の識別子、`message` は利用者に見せられる日本語です。時間制限 `timeout`（ミリ秒）には Web Worker が必要です。

## Web Worker

```ts
// reader.worker.ts
import "@zumenjs/reader/worker";
```

```ts
import { ReaderWorker } from "@zumenjs/reader";

const worker = new ReaderWorker(() => new Worker(new URL("./reader.worker.ts", import.meta.url), { type: "module" }));
const scene = await worker.read(bytes, { view: "Layout1", limits: { timeout: 10_000, maxEntities: 500_000 } });
```

- 読み取り中も画面を操作できます。結果は JSON として返し、`sceneFromJson` で復元します。
- `timeout` を超えるか `signal` が中断されると Worker を停止し、次の読み取りで作り直します。`terminate()` は恒久的に停止します。
- 読み取りは依頼順に 1 件ずつ行います。時間は読み取り開始から数え、モジュールの読み込み時間を含めません。
- Worker の作成関数は利用側が渡します。WebAssembly の配置を変えるときは `wasm` と、PDF 用の `pdfWasm` を指定します。

対応形式の細則は[形式別の資料](../../README.md#文書)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjsreader"></a>

[zumenjsreader](README.en.md#zumenjsreader)

<a id="loading"></a>

[loading](README.en.md#loading)

<a id="limits-for-drawings-you-receive"></a>

[limits-for-drawings-you-receive](README.en.md#limits-for-drawings-you-receive)

<a id="reading-in-a-web-worker"></a>

[reading-in-a-web-worker](README.en.md#reading-in-a-web-worker)

<a id="formats"></a>

[formats](README.en.md#formats)

</details>
