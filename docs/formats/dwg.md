# DWG の扱い

`zumen-dwg` は AutoCAD 1.0（MC0.0）〜2018 以降の AC1032 を、自前の Rust デコーダで読み取ります。公開された ODA の仕様に従い、DXF と共通の `DxfDocument` へ写し、同じ `dxf_to_scene` で描画します。

TypeScript の DWG 読み取りは持ちません。ブラウザは `@zumenjs/reader` の WebAssembly、Rust・Python・CLI は同じ Rust 実装を使います。GPL の LibreDWG とそのソースは使いません。

## 版とコンテナ

- R12 以前の形式は、それぞれのヘッダとレコードの配置に従います。
- R13・R14・R2000 はセクション位置からヘッダ変数・クラス・オブジェクト表を読みます。
- R2004・2010・2013・2018 はマスクされたヘッダ、ページ表・セクション表、LZ77 圧縮を扱います。
- R2007 は Reed-Solomon 符号化と固有の圧縮・ページ構造を扱います。

破損・欠落したページは報告し、展開をページサイズで止めます。オブジェクトの型・ビット列・文字列・ハンドルのストリームを版ごとに読み分けます。読めないオブジェクト・型は件数付きで報告します。

## 図面モデルと文字

レイヤ・線種・文字と寸法のスタイル・部品・図形・レイアウトを AutoCAD モデルに読みます。文字列は版に応じてコードページまたは UTF-16 で読み、日本語を保持します。

DXF と DWG が同じ図面を表すときは、座標・属性・配置・縮尺が一致するようにします。元の版が持たない値や、AutoCAD が保存時に変える値は違いとして記録します。R13 の実図面が手元にない検証など、未確認の範囲を区別します。

## 書き出し

`rewrite_dwg` は既存図面のオブジェクトをビット列として保持し、元の版へ書き戻します。`write_dwg` は AutoCAD モデルから新しい AutoCAD 2013 の DWG を作ります。これにより DXF・SXF・JWW から DWG へ変換できます。

コンテナ・圧縮・チェックサム・オブジェクト表を整合させ、読み直しと独立した実装で検証します。版ごとの新規書き出しの対応条件は詳細記録を参照してください。

## 既存図面の編集

`edit_dwg` は操作対象以外のオブジェクトと元の版を保ちます。既存図形の更新・削除、図形と定義の追加、用紙の変更、テンプレートへの作図を共通の操作で行います。すべて検証してから書き、失敗時に途中までの図面を返しません。

## 推測と検証

公開仕様が実ファイルと合わない箇所は、同じ図面を AutoCAD が書いた DXF、MIT の独立実装、最小の合成図面で確認し、推測と明記します。版別のビット配置・規則・既知の差分・通知コード・各テストの記録は英語の補足資料に保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [dwg.en.md](dwg.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="how-dwg-is-handled"></a>

[how-dwg-is-handled](dwg.en.md#how-dwg-is-handled)

<a id="versions-and-containers"></a>

[versions-and-containers](dwg.en.md#versions-and-containers)

<a id="objects"></a>

[objects](dwg.en.md#objects)

<a id="r13-and-r14"></a>

[r13-and-r14](dwg.en.md#r13-and-r14)

<a id="r12-and-earlier"></a>

[r12-and-earlier](dwg.en.md#r12-and-earlier)

<a id="text"></a>

[text](dwg.en.md#text)

<a id="what-is-read"></a>

[what-is-read](dwg.en.md#what-is-read)

<a id="the-same-values-as-the-dxf-of-the-drawing"></a>

[the-same-values-as-the-dxf-of-the-drawing](dwg.en.md#the-same-values-as-the-dxf-of-the-drawing)

<a id="limits"></a>

[limits](dwg.en.md#limits)

<a id="known-differences-from-the-dxf-of-the-drawing"></a>

[known-differences-from-the-dxf-of-the-drawing](dwg.en.md#known-differences-from-the-dxf-of-the-drawing)

<a id="writing"></a>

[writing](dwg.en.md#writing)

<a id="containers"></a>

[containers](dwg.en.md#containers)

<a id="checks"></a>

[checks](dwg.en.md#checks)

<a id="compression"></a>

[compression](dwg.en.md#compression)

<a id="inferred-rules"></a>

[inferred-rules](dwg.en.md#inferred-rules)

<a id="notices"></a>

[notices](dwg.en.md#notices)

<a id="verification"></a>

[verification](dwg.en.md#verification)

<a id="new-drawings"></a>

[new-drawings](dwg.en.md#new-drawings)

<a id="values"></a>

[values](dwg.en.md#values)

<a id="inferred-rules-1"></a>

[inferred-rules-1](dwg.en.md#inferred-rules-1)

<a id="notices-of-new-drawings"></a>

[notices-of-new-drawings](dwg.en.md#notices-of-new-drawings)

<a id="verification-of-new-drawings"></a>

[verification-of-new-drawings](dwg.en.md#verification-of-new-drawings)

<a id="versions-of-new-drawings"></a>

[versions-of-new-drawings](dwg.en.md#versions-of-new-drawings)

<a id="verification-of-versions"></a>

[verification-of-versions](dwg.en.md#verification-of-versions)

<a id="editing-edit_dwg"></a>

[editing-edit_dwg](dwg.en.md#editing-edit_dwg)

<a id="inferred-rules-of-editing"></a>

[inferred-rules-of-editing](dwg.en.md#inferred-rules-of-editing)

<a id="verification-of-editing"></a>

[verification-of-editing](dwg.en.md#verification-of-editing)

</details>
