# SXF（SFC・P21）の扱い

国土交通省の SXF 公開仕様と実装規約に従い、SFC と P21 を同じフィーチャモデルへ読み取ります。Ver.2.0〜3.1、圧縮した SFZ・P2Z に対応します。日本の電子納品で使う図面を主要な対象にします。

## 判定と文字コード

先頭 64 KiB を確認し、`ISO-10303-21` と `/*SXF` または `feature_mode` から SFC を判定します。それ以外の P21 は、`FILE_SCHEMA` があれば先頭のスキーマが `ASSOCIATIVE_DRAUGHTING` であることを確認します。IFC や他の STEP ファイルを空の図面として読まないためです。文字列・コメントの中の宣言は判定に使いません。互換性のため、宣言がない従来の SXF は引き続き読みます。

文字は CP932 です。「表」「ソ」などの 2 バイト目の `0x5C` と、バックスラッシュのエスケープを区別します。P21 では ISO 10303-21 の文字列エスケープも戻します。

## 図形・配置・縮尺

色・線種・線幅・フォント・レイヤのコードは出現順を基本にします。定義済み線幅は固定コード 1〜9、利用者定義は 11 からです。SFC の書き出しでは、使う定義済み線幅も宣言します。Jw_cad で線が欠落した問題を再現して確認した規則です。

部分図・作図グループ・作図部品の入れ子を解決します。点記号と矢印の大きさは部分図の縮尺に依存せず、ハッチングの間隔と角度は配置に従います。スプラインは 3 次ベジェ、クロソイドは R・L = A² の積分から点列にします。

P21 は STEP/AP202 のテンプレートに従って同じフィーチャへ写し、SFC と同じ形で描きます。文字の基点補正、寸法矢印、レイヤと不可視設定の細則は詳細記録に保存しています。

## 画像

画像はグループの名前・属性からファイル名を得て、境界の折線の位置に下敷きとして配置します。外部ファイルの取得・TIFF や JPEG の復号は利用側が行います。画素数を持たないため内容の単位は用紙上の 0.01 mm です。回転を反映し、描画順の違いは `raster-order` で報告します。境界のない画像は `raster` で報告します。

SFZ・P2Z 内の関連ファイルは `embedded_files`・`readEmbeddedFiles` で取得できます。破損ファイルは報告し、展開後の総バイト数を上限に数えます。

## 圧縮と上限

SFZ・P2Z は ZIP 内の最初の `.sfc`・`.p21` を読み、格納・Deflate と CRC-32 を確認します。名前は UTF-8 フラグがなければ Shift_JIS です。暗号化 ZIP・ZIP64 は未対応です。

引数の入れ子、部分図の深さ、配置の循環、ハッチング線数、クロソイドの分割数などに上限があります。省略は `unreadable`・`sfig-depth`・`sfig-cycle`・`hatch-dense` などの注意事項で返します。

## 変換・書き出し・編集

AutoCAD モデルへの変換ではモデル空間を実寸にし、部分図を縮尺付きビューポート、用紙をレイアウト、部品をブロックに写します。逆変換も同じ対応を使います。SXF にない意味や形は描画要素へ写し、注意事項を返します。

画像の作図グループは、グループのブロックの中で境界の折線の直前に IMAGE を置き、グループが書くファイル名を指します。AutoCAD は図形の順に描くため、画像は前の図形を隠し、境界の折線は隠しません。画素数は用紙上の 0.01 mm を 1 画素とし（[決定 0031](../decisions/0031-image-units-without-a-pixel-count.md)）、外形は折線が描くので画像の外形は表示しません（IMAGEFRAME 0）。逆変換では画像を書かず（`image`）、外形は DXF が描くときだけ線で書きます。SXF の画像は描く境界の折線を必要とし、外形を表示しない画像と両立しないためです。

SFC・P21・SFZ・P2Z を Ver.2.0・3.0・3.1 で書けます。`edit_sxf` は操作対象以外の元データ・版・文字コードを保ち、寸法・ハッチング・部品・スタイルの追加やテンプレートへの作図を扱います。SFC のコメントと P21 のインスタンスでは保存方法が異なります。

## 仕様と検証

公開仕様の節をコードのコメントに記載し、仕様だけでは決められなかった挙動を詳細記録に区別しています。TypeScript と Rust の正解比較、実図面、Jw_cad での確認を組み合わせます。フィーチャ別の対応、推測、各通知コード、参照仕様は英語の補足資料に保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [sxf.en.md](sxf.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="how-sxf-is-handled"></a>

[how-sxf-is-handled](sxf.en.md#how-sxf-is-handled)

<a id="reading"></a>

[reading](sxf.en.md#reading)

<a id="images"></a>

[images](sxf.en.md#images)

<a id="converting-to-dxf-zumen-convert"></a>

[converting-to-dxf-zumen-convert](sxf.en.md#converting-to-dxf-zumen-convert)

<a id="converting-to-sxf-dxf_to_sxf"></a>

[converting-to-sxf-dxf_to_sxf](sxf.en.md#converting-to-sxf-dxf_to_sxf)

<a id="writing-sxf-files-write_sfc-write_p21-write_sxf-write_sxf_version"></a>

[writing-sxf-files-write_sfc-write_p21-write_sxf-write_sxf_version](sxf.en.md#writing-sxf-files-write_sfc-write_p21-write_sxf-write_sxf_version)

<a id="sxf-versions"></a>

[sxf-versions](sxf.en.md#sxf-versions)

<a id="editing-sxf-edit_sxf"></a>

[editing-sxf-edit_sxf](sxf.en.md#editing-sxf-edit_sxf)

<a id="editing-sfc-in-place"></a>

[editing-sfc-in-place](sxf.en.md#editing-sfc-in-place)

<a id="editing-p21-in-place"></a>

[editing-p21-in-place](sxf.en.md#editing-p21-in-place)

<a id="writing-p21-anew"></a>

[writing-p21-anew](sxf.en.md#writing-p21-anew)

<a id="writing-sfc-anew"></a>

[writing-sfc-anew](sxf.en.md#writing-sfc-anew)

<a id="compressed-sxf"></a>

[compressed-sxf](sxf.en.md#compressed-sxf)

<a id="notices-of-the-writers"></a>

[notices-of-the-writers](sxf.en.md#notices-of-the-writers)

<a id="verification"></a>

[verification](sxf.en.md#verification)

<a id="references"></a>

[references](sxf.en.md#references)

<a id="dxf-への変換zumen-convert"></a>

[dxf-への変換zumen-convert](sxf.en.md#dxf-への変換zumen-convert)

<a id="sxf-への書き出しdxf_to_sxfwrite_sfc"></a>

[sxf-への書き出しdxf_to_sxfwrite_sfc](sxf.en.md#sxf-への書き出しdxf_to_sxfwrite_sfc)

</details>
