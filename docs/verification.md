# 検証の記録

形式ごとの実図面の比較、ブラウザと OS の確認、回帰テストの履歴です。従来の件数・日時・個別の差分は[詳細記録](verification.en.md)に保存しています。ここでは検証方法と結果の読み方を日本語で整理します。

## 自動検証

`pnpm run check` が入口です。書式、WebAssembly、型、単体・ブラウザテスト、依存監査、ビルド、Rust、スクリプト、Python を確認します。

- TypeScript と Rust の共通形式は、同じ合成図面の `Scene` 全体を正解ファイルと比較します。
- DWG は同じ図面を AutoCAD が保存した DXF と比較し、形式内の値・図形・属性・配置を確認します。
- PDF は `render_pdf` で出力した図面を読み直し、読み取り結果とページ描画を確認します。pdf.js との比較は差分の報告にも使います。
- 図形 id の互換性は正解ファイルとは独立した記録で確認します。
- ランダムに破損させた入力を両言語で読み、結果・エラーの一致と処理が終了することを確認します。

## ブラウザ

Chromium・Firefox・WebKit で Canvas、入力イベント、WebAssembly、Web Worker、React フック、PDF ページ描画を検査します。実図面を試用画面で開いた記録もあります。

Playwright の WebKit は Safari そのものではありません。Safari、iOS・Android、実際のタッチ・トラックパッド・マウスなど、自動テストと同一視できない環境は未確認として区別します。

## Windows

ネイティブの PowerShell・MSVC・Windows の Python を使い、日本語と空白を含むパス、拡張モジュール、配布 wheel を確認します。Jw_cad での読み直し・表示・フォントの確認も、別の手動検証として記録しています。

## 形式と書き出し

SXF・JWW・DXF の読み取り、Rust への移植、DWG の版別コンテナ、同じ図面の DXF・DWG、複数行属性、変換後の縮尺・文字高・ハッチング・レイアウトを比較しています。

書き出し後に自前の読み取りだけでなく ezdxf など独立した実装や実際の CAD で確認することがあります。その実施範囲と差分は詳細記録に明記します。正解ファイルの作り直しだけでは新しい結果の正しさは保証できません。

## 新しい記録

検証した日、環境、入力の種類、実行したコマンド、比較対象、結果、残る差分を日本語で追記します。機密図面・受領図面・端末固有のパスや非公開情報を Git に入れません。

今回の日本語ファーストへの変更については、PR に実行した検査と結果を記載します。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [verification.en.md](verification.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="verification-records"></a>

[verification-records](verification.en.md#verification-records)

<a id="windows-native-verification"></a>

[windows-native-verification](verification.en.md#windows-native-verification)

<a id="sxf-and-jww"></a>

[sxf-and-jww](verification.en.md#sxf-and-jww)

<a id="dxf"></a>

[dxf](verification.en.md#dxf)

<a id="dxf-against-the-dwg-of-the-same-drawing"></a>

[dxf-against-the-dwg-of-the-same-drawing](verification.en.md#dxf-against-the-dwg-of-the-same-drawing)

<a id="multi-line-attributes-of-autocad-2018"></a>

[multi-line-attributes-of-autocad-2018](verification.en.md#multi-line-attributes-of-autocad-2018)

<a id="sxf-in-rust"></a>

[sxf-in-rust](verification.en.md#sxf-in-rust)

<a id="dxf-in-rust"></a>

[dxf-in-rust](verification.en.md#dxf-in-rust)

<a id="dwg-in-rust"></a>

[dwg-in-rust](verification.en.md#dwg-in-rust)

<a id="dwg-in-the-browser"></a>

[dwg-in-the-browser](verification.en.md#dwg-in-the-browser)

<a id="the-readers-in-the-browser"></a>

[the-readers-in-the-browser](verification.en.md#the-readers-in-the-browser)

<a id="the-2d-engine-to-embed"></a>

[the-2d-engine-to-embed](verification.en.md#the-2d-engine-to-embed)

<a id="firefox-and-webkit"></a>

[firefox-and-webkit](verification.en.md#firefox-and-webkit)

<a id="pdf-pages-with-pdfjs"></a>

[pdf-pages-with-pdfjs](verification.en.md#pdf-pages-with-pdfjs)

<a id="dxf-writing"></a>

[dxf-writing](verification.en.md#dxf-writing)

<a id="editing-dxf-in-place"></a>

[editing-dxf-in-place](verification.en.md#editing-dxf-in-place)

<a id="dxf-versions"></a>

[dxf-versions](verification.en.md#dxf-versions)

<a id="sxf-and-jww-to-dxf"></a>

[sxf-and-jww-to-dxf](verification.en.md#sxf-and-jww-to-dxf)

<a id="writing-sxf"></a>

[writing-sxf](verification.en.md#writing-sxf)

<a id="rust-readers-rendering-and-bindings"></a>

[rust-readers-rendering-and-bindings](verification.en.md#rust-readers-rendering-and-bindings)

<a id="r13r14-dwg-review-2026-10-20"></a>

[r13r14-dwg-review-2026-10-20](verification.en.md#r13r14-dwg-review-2026-10-20)

<a id="library-review-regressions-2026-10-23"></a>

[library-review-regressions-2026-10-23](verification.en.md#library-review-regressions-2026-10-23)

<a id="pdf-in-rust"></a>

[pdf-in-rust](verification.en.md#pdf-in-rust)

<a id="pdf-pages-on-servers"></a>

[pdf-pages-on-servers](verification.en.md#pdf-pages-on-servers)

</details>
