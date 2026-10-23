# Jw_cad の JWW・JWC・JWS の扱い

Jw_cad の図面を、レイヤグループの縮尺・日本語・線・円弧・文字・ソリッド・部品・寸法とともに読み取ります。TypeScript は JWW の参照実装、Rust は JWW の読み書き・編集と JWC・JWS の読み書きを提供します。

## 仕様と読み取り

Jw_cad の `jwdatafmt.txt` に従い、MFC の `CArchive` の文字列・件数・クラス・オブジェクトの配置を読みます。参照した実装とライセンスは[第三者の資料](../../packages/jww/THIRD_PARTY_NOTICES.md)に記録しています。

元の座標は用紙中央が原点です。表示用モデルは左下を原点に移し、実寸はレイヤグループの縮尺から求めます。レイヤ名・表示状態・ペン色・線種・線幅はヘッダを使い、読めないヘッダは既定値と注意事項で扱います。

負の円弧角は時計回りで、扁平率と傾きは楕円にします。ソリッドの頂点順は JWW の保存順を使い、DXF SOLID に変換するときだけ頂点順を変えます。印刷しない仮点と設定用の文字は図形として描かず、注意事項やメタデータに分けます。

## 日本語と画像

文字とフォント名を保ち、縦書き、全角・半角、文字幅と配置を扱います。Jw_cad 8 以降が特殊な位置に文字として保存する設定値は、図面の範囲を広げないように描画対象から外します。

`^@BM` の画像文字からファイル参照・用紙上の幅と高さ・角度を読み、下敷きにします。画像を示す図形は選択用に保持し、`drawn: false` として二重に描きません。画素数は保存されないため、内容単位は用紙上の 0.01 mm です。

7.00 以降の同梱画像はブロック定義後の一覧から取り出します。gzip の CRC・長さ・予約フラグを確認し、`embedded_files`・`readEmbeddedFiles` で返します。破損・途中終了は `embedded-broken`・`embedded-truncated`、未解明の切り取り値は `image-trim` で報告します。

## 変換

AutoCAD モデルへは図形を実寸で写し、縮尺ごとのビューポートで用紙に表示します。複数縮尺があるときの 1:1 グループは図枠・表題欄として紙空間に置きます。レイヤ・色・線種・線幅・文字・ソリッド・部品・寸法を対応させます。

`^@BM` の画像は、図面が書くファイル名を指す IMAGE にします。画素数は用紙上の 0.01 mm を 1 画素とし（[決定 0031](../decisions/0031-image-units-without-a-pixel-count.md) の内容単位と同じ）、用紙上の幅・高さの 100 倍を整数に丸めます。Jw_cad は画像に外形を描かないため、外形を表示しない図面（IMAGEFRAME 0）にします。同梱画像は `%temp%` の名前のまま指し（`embedded-file`）、既定でない切り取り値は書きません（`image-trim`）。

逆変換では用紙を JWW の用紙、ビューを縮尺付きグループにします。保持できない値は近似または描画要素に分解し、注意事項を返します。画像は、切り抜かず、ゆがまず、裏返さない矩形だけを `^@BM` にします。ほかは `image`、フェードは `image-fade` で報告し、外形は DXF が描くときだけ線で書きます。

## 書き出しと編集

`JwwFile` は元の保存内容を扱い、`write_jww` は新規図面を作ります。`edit_jww` は未変更のレコードと元の版を保って、図形・レイヤ・部品・寸法・ハッチング・文字と線の設定を編集します。

JWW の寸法は図面の設定に従います。角度寸法など未対応のものは明示的に拒否します。ハッチングが複数の図形になる場合は、編集や削除もその図形ごとです。テンプレートへの作図は空きグループ・レイヤと縮尺を確認し、不足時は部分的に書きません。

## JWC と JWS

JWC は DOS 版 Jw_cad の図面、JWS は登録図形です。Rust で読み書きできますが、JWW の編集・テンプレート作図とは対象を区別します。他の Jw_cad 関連ファイルを図面として誤って読まないように判定します。

版ごとのフィールド、推測した値、対応表、編集時の番号と基点、参照仕様は英語の補足資料に保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [jww.en.md](jww.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="how-jww-is-handled"></a>

[how-jww-is-handled](jww.en.md#how-jww-is-handled)

<a id="reading"></a>

[reading](jww.en.md#reading)

<a id="images"></a>

[images](jww.en.md#images)

<a id="converting-to-dxf-zumen-convert"></a>

[converting-to-dxf-zumen-convert](jww.en.md#converting-to-dxf-zumen-convert)

<a id="converting-to-jww-zumen-convert"></a>

[converting-to-jww-zumen-convert](jww.en.md#converting-to-jww-zumen-convert)

<a id="writing"></a>

[writing](jww.en.md#writing)

<a id="the-file-as-stored-jwwfile"></a>

[the-file-as-stored-jwwfile](jww.en.md#the-file-as-stored-jwwfile)

<a id="new-drawings-write_jww"></a>

[new-drawings-write_jww](jww.en.md#new-drawings-write_jww)

<a id="editing-edit_jww"></a>

[editing-edit_jww](jww.en.md#editing-edit_jww)

<a id="jwc-jw_cad-for-dos"></a>

[jwc-jw_cad-for-dos](jww.en.md#jwc-jw_cad-for-dos)

<a id="jws-registered-figures"></a>

[jws-registered-figures](jww.en.md#jws-registered-figures)

<a id="other-files-of-jw_cad"></a>

[other-files-of-jw_cad](jww.en.md#other-files-of-jw_cad)

<a id="dxf-への変換zumen-convert"></a>

[dxf-への変換zumen-convert](jww.en.md#dxf-への変換zumen-convert)

</details>
