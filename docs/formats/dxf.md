# DXF の扱い

テキスト・バイナリの DXF（R12〜2018）を AutoCAD の図面モデル `DxfDocument` に読み、`dxfToScene`・`dxf_to_scene` で共通の表示用モデルにします。同じモデルを DWG でも使います。

## 読み取りと日本語

セクション・テーブル・ブロック・図形をグループコードから読みます。R2007 以降の UTF-8、以前のコードページ、Shift_JIS の日本語、バイナリの数値と文字列を区別します。未知のコードページや未対応の図形は報告します。

線・円弧・円・楕円・折線・スプライン・文字・複数行文字・属性・部品・寸法・ハッチング・引出線・ソリッド・ワイプアウトなどを扱います。色・線種・線幅・透明度・文字スタイルを解決し、元の定義を属性に残します。

## 空間・単位・縮尺

モデル空間とレイアウトを `Scene.views` で選べます。ビューポートがモデル空間をその縮尺と回転で表示し、配置の変換から実寸を求めます。モデルの単位は `$INSUNITS` を使い、不明なときは仮定を注意事項で示します。

表示用の座標は用紙上の mm ですが、属性の未整形の座標と長さは元の図面の単位です。日本向けの方針によって inch などを勝手に mm と読み替えません。

## 文字・画像・参照

MTEXT の書式・縦書き・文字高・配置を共通の文字要素へ写します。図面が宣言する文字高と、描画用 em を分けて保持します。参照された画像・PDF は下敷きとして配置し、ファイルの取得は利用側が行います。

画像の画素数・配置・切り取り・不透明度を保持します。外部図面の参照など、単体で描画できない内容は黙って省略せず notices に返します。多重引出線・複線・配列・部品の入れ子も共通の上限の中で解決します。

## PDF の下敷き

PDFUNDERLAY は `kind: "pdf"` の下敷きにします。`file` は PDFDEFINITION の 1（図面に書かれたファイル名）、`page` は 2（ページ番号）です。図面はページの大きさを持たないため、読み取りは置く前の下敷き（`width`・`height` が 0、`matrix` は高さ 0 のページのもの）を返します。利用側が PDF を開き、`@zumenjs/pdf` の `placePdfUnderlays`（大きさが分かっていれば `@zumenjs/core` の `placeUnderlays`）で置いてから、`pdfUnderlayRenderer` で描きます。置くまでは描きません（[決定 0034](../decisions/0034-place-pdf-underlays-after-reading.md)）。

- **配置**: 内容の座標は、表示の向き（`/Rotate` 済み、pdf.js の `page.getViewport({ scale: 1 })`）のページの pt で、左上が原点です。挿入点（10、OCS）がページの左下の角で、高さ h のページの点 (x, y) は、挿入点から (x / 72, (h − y) / 72) に縮尺（41・42）と回転（50）を掛け、法線（210）で WCS に写した位置です。縮尺 0 は 1、280 がなければ 10 と読みます（ezdxf 1.4 と同じ）。
- **単位**（推測）: 縮尺 1 で PDF の 1 インチを図面の 1 単位とします（OpenCADStudio と同じ）。ACadSharp の `sample_AC1015_ascii.dxf`（`$INSUNITS` 1、インチ）は A4（595 × 842 pt）の `pdf-definition.pdf` の 1 ページ目を縮尺 1 で (16.287, 15) に置いており、この規則でページは実寸の 8.26 × 11.69 インチになります。単位のある図面では縮尺 1 で実寸にする AutoCAD の PDF 添付ダイアログとも合います。ただしインチの図面では「`$INSUNITS` で換算する」規則と区別できず、ページの位置をなぞる図形もないため、確定ではありません。ほかの単位の図面では、記録された縮尺が換算を含むと仮定します（mm の図面で実寸なら 25.4）。見本の DWG（R14〜2018）も DXF と同じ下敷きになります。
- **切り抜き**（推測）: 境界（11・21）は、挿入点を原点とする縮尺・回転の前の図面の単位です（ezdxf は変換で境界を動かさず、OpenCADStudio はページのインチとして読みます）。2 点なら矩形の対角です。280 の 1 で切り、280 の 16 は境界の内側を消します。このとき `clip` の空の輪は、まだ分からないページの外形を表します。範囲の分からないページは、レイアウトではビューポートの矩形で切ります。`placeUnderlays` は、ページが切り抜きの外にある下敷きを除きます。
- **範囲**: 置く前の `bbox` は切り抜きの外接矩形か、範囲が分からなければ挿入点です。`placeUnderlays` が、切り抜きの外接矩形の中のページの外接矩形にし、`Scene.bounds` を広げます。
- **表示の値**: 表示を切った下敷き（280 の 2 がない）は、AutoCAD と同じく描きません。フェード（282）は画像と同じく不透明度 1 − フェード / 100 にします。コントラスト（281 が 100 以外）・単色（280 の 4）・背景に合わせた色（280 の 8）は反映せず、`underlay-adjust` で報告します。枠（PDFFRAME）と、下敷きのレイヤの切り替え（`AdeskUnderlayLayerOverrideData` の拡張データ）は扱わず、PDF の下敷きごとに `pdf-underlay` で報告します。紙の背景を描くかは、利用側が `pdfUnderlayRenderer` の `background` で選びます。
- **報告**: ファイル名かページ番号（1 以上の整数）が分からない下敷きは描かず、`underlay` で報告します。描く順で前にある図形に重なる（範囲が分からなければ、重なるかもしれない）下敷きは、`underlay-order` で報告します。
- **id とレイヤ**: id は PDFUNDERLAY のハンドルに、ブロックとビューポートの前置きを付けたものです。図形は増やさないので、既存の図形 id は変わりません。
- **書き出し**: DXF（R12 を含む）・DWG の書き出しと、SXF・JWW への変換は PDF の下敷きを書かず、`pdf-underlay` で報告します。既存図面の編集は、ほかの図形と同じく元のまま残します。DWF・DGN の下敷きは読みません。

詳しい規則は英語の補足資料の [PDF underlays](dxf.en.md#pdf-underlays) にもあります。

## 入力上限

図形数・バイト数・時間に加え、部品の再帰・展開数、ハッチング線、曲線の分割、入力レコードのループに上限を設けます。TypeScript と Rust は構造と処理順、数値の表記、並べ替えを揃えます。

## 書き出し

`write_dxf` は AutoCAD 2013・UTF-8、`write_dxf_as` は R12〜2018 を扱います。2007 より前はコードページを使います。R12 にない MTEXT・ハッチングなどは分解し、紙空間・線幅など版固有の制約を注意事項で返します。

ハンドル・レイヤ・スタイル・部品・レイアウトをモデルから書き、再読込時の見た目を確認します。ただし変換で保持できるのはモデルに持つ内容で、バイト列の完全保存とは異なります。

## 既存図面の編集

`edit_dxf` は元のタグ列に操作を当て、対象外の内容・拡張データ・版・文字コードを保ちます。操作は共通の JSON 形式で、定義と図形の追加、更新、削除、レイアウトの用紙サイズ変更などを行えます。全操作を検証してから書きます。

図形別の対応表、グループコード、版ごとの制限、推測した規則、独立した実装での検証結果は英語の補足資料に保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [dxf.en.md](dxf.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="how-dxf-is-handled"></a>

[how-dxf-is-handled](dxf.en.md#how-dxf-is-handled)

<a id="the-autocad-drawing-model"></a>

[the-autocad-drawing-model](dxf.en.md#the-autocad-drawing-model)

<a id="reading"></a>

[reading](dxf.en.md#reading)

<a id="drawing-dxftoscene"></a>

[drawing-dxftoscene](dxf.en.md#drawing-dxftoscene)

<a id="spaces-and-scales"></a>

[spaces-and-scales](dxf.en.md#spaces-and-scales)

<a id="properties"></a>

[properties](dxf.en.md#properties)

<a id="entities"></a>

[entities](dxf.en.md#entities)

<a id="images"></a>

[images](dxf.en.md#images)

<a id="pdf-underlays"></a>

[pdf-underlays](dxf.en.md#pdf-underlays)

<a id="multileaders-and-multilines"></a>

[multileaders-and-multilines](dxf.en.md#multileaders-and-multilines)

<a id="limits"></a>

[limits](dxf.en.md#limits)

<a id="text-and-fonts"></a>

[text-and-fonts](dxf.en.md#text-and-fonts)

<a id="writing-write_dxf"></a>

[writing-write_dxf](dxf.en.md#writing-write_dxf)

<a id="versions-write_dxf_as"></a>

[versions-write_dxf_as](dxf.en.md#versions-write_dxf_as)

<a id="r12-r12prepare"></a>

[r12-r12prepare](dxf.en.md#r12-r12prepare)

<a id="editing-edit_dxf"></a>

[editing-edit_dxf](dxf.en.md#editing-edit_dxf)

<a id="verification"></a>

[verification](dxf.en.md#verification)

</details>
