# PDF 図面の扱い

PDF 図面の線・文字・画像を、選択・吸着・計測・解析に使う共通モデルへ読み取ります。ページの見た目は PDF エンジンが描き、読み取った図形を二重には描きません。汎用の PDF 文書の閲覧とは[用途を分けます](../decisions/0002-pdf-and-parapdf.md)。

## 読み取りと表示

Rust の `zumen-pdf` が hayro-interpret を使い、ページを `Scene.views` にします。ブラウザでは PDF 専用の WebAssembly を初めて PDF を開くときに読み込みます。`view: "2"` は 2 ページ目です。

ページの線・文字・画像は `drawn: false` の図形として保持し、選択・吸着・計測・文字検索・領域・表・製図基準・版の比較に使います。図形の id は読み取りの呼び出しに基づくため、依存の版と処理順を固定します。

## ページの描画

ブラウザでは利用側が開いた pdf.js の文書を `@zumenjs/pdf` に渡します。サーバーでは hayro がページを描きます。SVG・PDF 出力ではページ画像を埋め込み、その解像度を指定できます。

日本語の文字の読み取りにはフォント・文字マップを使い、同梱データとライセンスを記録します。選択用の形と実際の描画結果が同じ役割ではないことに注意します。

## 縮尺と座標

ページ座標を用紙上の mm、Y 軸上向きへ写します。計測辞書を持つビューポートは、その縮尺から実寸を求めます。縮尺が得られない図面を、実寸であるかのようには扱いません。

## 対応範囲と上限

PDF は読み取り・描画・解析専用です。`convert`・`edit`・`draft` の CAD 入力には使えません。解釈できない内容や代替は notices に返します。ページ・図形・バイト数・処理時間などの上限を適用します。

自前で出力した PDF の読み直し、ページ画像、pdf.js との比較で確かめます。判定・図形 id・省略対象・計測辞書・安全上の上限・参照仕様の細則は英語の補足資料に保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [pdf.en.md](pdf.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="how-pdf-is-handled"></a>

[how-pdf-is-handled](pdf.en.md#how-pdf-is-handled)

<a id="detection-pages-and-views"></a>

[detection-pages-and-views](pdf.en.md#detection-pages-and-views)

<a id="entities-and-ids"></a>

[entities-and-ids](pdf.en.md#entities-and-ids)

<a id="drawing-the-pages"></a>

[drawing-the-pages](pdf.en.md#drawing-the-pages)

<a id="not-read-and-reported"></a>

[not-read-and-reported](pdf.en.md#not-read-and-reported)

<a id="scales-viewports-and-measure-dictionaries"></a>

[scales-viewports-and-measure-dictionaries](pdf.en.md#scales-viewports-and-measure-dictionaries)

<a id="limits-and-safety"></a>

[limits-and-safety](pdf.en.md#limits-and-safety)

<a id="tests"></a>

[tests](pdf.en.md#tests)

</details>
