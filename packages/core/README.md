# @zumenjs/core

日本の建築・土木向けライブラリ [zumen](https://github.com/zumenjs/zumen) の共通部分です。形式に依存しない `Scene` と JSON、`DrawingReader`・`readDrawing`、幾何、文字配置、用紙上と実寸の計測、注意事項を提供します。Rust の `zumen-core` と対応し、DOM を使わないため Node.js・Web Worker でも動きます。

```ts
import { readDrawing } from "@zumenjs/core";
import { initReader, reader } from "@zumenjs/reader";

await initReader();
const scene = readDrawing(bytes, [reader]);
```

- `scene.views` はモデル空間・レイアウト・PDF ページなどの表示一覧です。`readDrawing(bytes, readers, { view })` で選びます。PDF を読む場合は `initPdfReader()` を先に呼びます。
- 画像・PDF ページの配置は `scene.underlays` に入り、利用側が渡す描画処理が表示します。
- マークアップは図面に添える JSON 文書です。`createMarkup`・`markupFromJson`・`markupToJson`・`markupShapes`・`markupBox`・`drawingHash` を使います。不正な入力は `MarkupError` で返します。
- 幾何の補助関数は `@zumenjs/core/geometry`、文字配置は `@zumenjs/core/text` から読み込みます。

表示と操作は [`@zumenjs/viewer`](../viewer)、マークアップの仕様は[マークアップ](../../docs/markup.md)、全体構成は[構成](../../docs/architecture.md)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjscore"></a>

[zumenjscore](README.en.md#zumenjscore)

</details>
