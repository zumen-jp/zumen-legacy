# @zumenjs/viewer

利用側の画面に組み込む 2D 図面エンジンです。Canvas の描画、当たり判定、吸着、表示範囲、選択、計測、操作ツールを受け持ちます。ラベル・ボタン・パネル・CSS は利用側が用意します。

```ts
import { DrawingViewer } from "@zumenjs/viewer";

const viewer = new DrawingViewer(element, scene, { label: "図面" });
viewer.fit();
// 画面を閉じるときに破棄します
viewer.destroy();
```

## 表示範囲とイベント

`fit` で全体を表示し、`fitBounds` で指定範囲を表示します。`zoomBy`・`panBy`・`rotateBy` で拡大・移動・回転します。`getView` で保存した表示範囲は `jumpTo` で復元できます。

`toScreen` と `toPaper` で画面のピクセル座標と用紙の mm を変換します。`move`・`cursor` などのイベントを `on` で受け、図面に重ねた HTML の位置を合わせられます。戻り値の解除関数で購読を終えます。

## 状態・選択・レイヤ

`getState()` で状態を読み、`subscribe()` で変更を受け取ります。選択やレイヤの表示状態はエンジンにまとめます。`setHighlight` は検索結果などを名前付きの集合として強調します。

計測は用紙上の距離と、配置の縮尺から求めた実寸の距離を返します。異なる縮尺の配置をまたぐ場合は実寸を決められないことがあります。

## 操作ツール

選択・計測・移動もツールです。`addTool` で独自ツールを追加し、`setTool` で切り替えます。ツールが受け取る座標は用紙上の mm です。描画は図面の上に重ねます。

入力ハンドラは個別に停止できます。`handlers: { wheel: "modifier" }` は Ctrl または ⌘ を押している間だけホイールで拡大・縮小します。

## 配色

色は要素の CSS カスタムプロパティで指定します。パッケージは CSS を同梱しません。

```css
.drawing {
  --zumen-background-dark: #1f2023;
  --zumen-selection-dark: #ffd43d;
  --zumen-highlight-search-dark: #5ad2ff;
  --zumen-fit-padding: 16;
}
```

## 画像・PDF の下敷き

`Scene.underlays` は配置を表します。`setUnderlayRenderer` に描画処理を渡すと、必要な範囲を描きます。画像には `imageUnderlayRenderer`、PDF には [`@zumenjs/pdf`](../pdf) を使えます。外部ファイルの解決と取得は利用側の責任です。

## マークアップ

`MarkupLayer` を図面に重ね、赤入れ・変更雲・注記・指摘・写真位置を表示・編集できます。図面ファイルを書き換えず、別の JSON 文書として保存します。

具体的な操作の組み合わせは[試用画面](../../playground/drawing-sample.tsx)と[マークアップのサンプル](../../playground/markup-sample.tsx)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjsviewer"></a>

[zumenjsviewer](README.en.md#zumenjsviewer)

<a id="view"></a>

[view](README.en.md#view)

<a id="state"></a>

[state](README.en.md#state)

<a id="input-handlers"></a>

[input-handlers](README.en.md#input-handlers)

<a id="tools"></a>

[tools](README.en.md#tools)

<a id="style"></a>

[style](README.en.md#style)

<a id="underlays"></a>

[underlays](README.en.md#underlays)

<a id="markup"></a>

[markup](README.en.md#markup)

<a id="queries-and-overlays"></a>

[queries-and-overlays](README.en.md#queries-and-overlays)

</details>
