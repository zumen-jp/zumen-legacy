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
