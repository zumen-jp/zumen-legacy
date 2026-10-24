# 図面のマークアップ

赤入れ・変更雲・注記・指摘・写真の撮影位置を、図面に添える JSON 文書として扱います。図面ファイル自体は変えません。ブラウザで表示・編集でき、Rust・Python・CLI・MCP からも読み書きと新版への移行ができます。

## 文書

```json
{
  "format": "zumen-markup",
  "version": 1,
  "drawing": {"sha256": "0000000000000000000000000000000000000000000000000000000000000000", "format": "jww"},
  "items": [
    {"id": "note-1", "kind": "text", "position": [100, 80], "text": "寸法を確認", "height": 3.5}
  ]
}
```

`drawing.sha256` は対象図面のバイト列の SHA-256（小文字の 16 進数）です。上の値は書式例で、実際には `drawingHash`・`zumen.drawing_hash` の結果を使います。

座標は項目の `view` の用紙上の mm、Y 軸は上向きです。複数ビューのある図面はビュー id を指定し、単一ビューでは省略できます。文書と項目の未知のキーは読み書きで保持します。

| kind | 内容 |
| --- | --- |
| `redline` | 線・折線・手書き・長方形。`points`、閉じる指定、矢印 |
| `cloud` | `points` の閉じた外周に沿う変更雲 |
| `text` | `position` からの注記。高さ・回転を指定可能 |
| `finding` | 指摘のピン、任意の範囲、内容、ラベル、状態 |
| `photo` | 撮影位置、写真の参照、撮影方向 |

項目には一意の `id` が必要です。`category` は色の分類、`color` は個別色、`lineWidth` は用紙上の mm です。`entities` は対象図形の id、`data` は担当者・期限・返信など利用側の情報を保持します。

[JSON Schema](../crates/zumen-core/schema/markup.schema.json)で文書の形を確認できます。不正な入力は code・path・日本語の説明で返します。

## 描画と編集

ブラウザでは `createMarkup`・`markupFromJson`・`markupToJson` と `MarkupLayer` を使います。道具を選んで入力し、変更イベントで文書を保存します。画面は[サンプル](../playground/markup-sample.tsx)を参照してください。

サーバーでは `zumen.read_markup`、`zumen.tools.list_markup`・`write_markup` を使います。描画時に markup を渡すと同じビューの項目を重ねます。ピン・写真記号の大きさは画面のピクセルを基準にします。カテゴリ色より個別色を優先し、未指定は赤です。

## 新版への移行

`carry_markup` は旧版と新版の全レイヤを形・文字で比較します。版の移動量で項目の座標を動かし、対応する図形 id に付け替え、状態を記録します。

| carried.state | 意味 |
| --- | --- |
| `unchanged` | 対象の内容・位置に関する変更なし |
| `changed` | 対象の内容または項目付近の図面に変更あり |
| `removed` | 対象図形またはビューがなくなった |

ビューの対応を指定すると、DXF のレイアウトから PDF のページなどへ移せます。元の文書が旧版の SHA-256 を指すことを確認し、明示的な無視指定がなければ不一致を拒否します。移行結果には新しい図面のハッシュと状態ごとの件数を返します。

```sh
zumen markup carry markup.json before.dwg after.dwg -o carried.json
zumen render after.dwg -o review.png --markup carried.json
```

写真の参照の解決、担当者・期限・返信の運用、保存先は利用側で決めます。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [markup.en.md](markup.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="drawing-markup"></a>

[drawing-markup](markup.en.md#drawing-markup)

<a id="the-document"></a>

[the-document](markup.en.md#the-document)

<a id="unknown-keys"></a>

[unknown-keys](markup.en.md#unknown-keys)

<a id="checks"></a>

[checks](markup.en.md#checks)

<a id="shapes"></a>

[shapes](markup.en.md#shapes)

<a id="revisions"></a>

[revisions](markup.en.md#revisions)

<a id="carrying-in-detail"></a>

[carrying-in-detail](markup.en.md#carrying-in-detail)

<a id="on-servers"></a>

[on-servers](markup.en.md#on-servers)

<a id="in-the-browser"></a>

[in-the-browser](markup.en.md#in-the-browser)

</details>
