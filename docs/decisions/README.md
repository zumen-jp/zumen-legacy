# 設計判断の記録

選択肢、選んだ理由、影響を 1 件ずつ記録します。ファイル名は `NNNN-<主題>.md`、番号は連番です。決定後は内容を上書きせず、変更する判断から元の記録へリンクします。リンク修正と今回の日本語の案内・要約の追加は、判断自体の変更と区別します。

日本語を標準にします。従来の詳細な議論は隣の `.en.md` に保存し、日付と状態を元のまま保持しています。「提案」は実装・採用済みを意味しません。

## テンプレート

```markdown
# NNNN: 決定を表す短い文

| 項目 | 内容 |
| --- | --- |
| 日付 | YYYY-MM-DD |
| 状態 | 提案 / 採用 / NNNN に置換 |

## 背景

## 選択肢

## 決定

## 影響

## 参照
```

## 一覧

- [0001: ST-Bridge を TypeScript パッケージとして扱う](0001-st-bridge-in-zumen.md)（2026-10-19、Superseded by [0013](0013-st-bridge-out-of-zumen.md)）
- [0002: PDF は用途で分担し、図面は zumen、文書は ParaPDF に置く](0002-pdf-and-parapdf.md)（2026-10-19、採用）
- [0003: DWG を自前の Rust 実装で AutoCAD の共通モデルに読む](0003-dwg-in-rust.md)（2026-10-19、採用）
- [0004: Python パッケージに MCP サーバーを置く](0004-mcp-server-in-the-python-package.md)（2026-10-20、採用）
- [0005: ブラウザ向けは組み込みエンジンとフックに分ける](0005-browser-packages-for-embedding.md)（2026-10-20、提案）
- [0006: AutoCAD の図面モデルを経由して書き出す](0006-write-through-the-autocad-model.md)（2026-10-20、採用）
- [0007: 形式間の変換を専用クレートに置く](0007-mapping-crate-between-formats.md)（2026-10-20、採用）
- [0008: モデル空間を実寸にし、用紙は縮尺ごとのビューポートで表示する](0008-sheets-and-scales-in-the-autocad-model.md)（2026-10-20、採用）
- [0009: AI 向けに描いた形と座標を返し、意味の判断は利用側に任せる](0009-geometry-for-ai-clients.md)（2026-10-20、採用）
- [0010: AutoCAD モデルから SXF を書き、非対応の図形は描画要素へ写す](0010-write-sxf-from-the-autocad-model.md)（2026-10-20、採用）
- [0011: MCP の処理を zumen.tools として直接呼べるようにする](0011-tool-functions-for-agents.md)（2026-10-20、採用）
- [0012: 操作対象以外を保って編集し、対応する CAD 形式へ書き出す](0012-edit-in-place-and-write-every-format.md)（2026-10-20、採用）
- [0013: 許可が必要な形式と寛容でない依存を除き、ST-Bridge を外す](0013-st-bridge-out-of-zumen.md)（2026-10-20、採用）
- [0014: Windows ネイティブのビルドと CAD 連携を確認する](0014-verify-on-native-windows.md)（2026-10-20、採用）
- [0015: 共通モデルから領域・表・製図基準を解析する](0015-analysis-in-zumen-core.md)（2026-10-21、採用）
- [0016: Worker から Scene の JSON を送り、画面側で復元する](0016-send-worker-scenes-as-json.md)（2026-10-21、採用）
- [0017: DXF・DWG の編集に注記と定義の追加を広げる](0017-add-annotations-and-definitions.md)（2026-10-22、採用）
- [0018: 宣言的な図面仕様から、図枠と縮尺付きビューを作図する](0018-drafting-kit.md)（2026-10-22、提案）
- [0019: 形式間で文字高の数値を保ち、字形の描き方は各形式に任せる](0019-text-heights-between-formats.md)（2026-10-23、採用）
- [0020: PDF 図面の線と文字を Rust で読み、ページ描画は PDF エンジンに任せる](0020-pdf-drawings-in-rust.md)（2026-10-23、提案）
- [0021: マークアップを別 JSON に置き、比較結果で新版へ移す](0021-drawing-markup.md)（2026-10-23、提案）
- [0022: 利用側の範囲描画処理で下敷きを描き、同内容の図形は描かない](0022-underlays-drawn-by-a-region-renderer.md)（2026-10-23、提案）
- [0023: 編集操作で既存レイアウトの用紙サイズを変える](0023-resize-existing-layouts.md)（2026-10-23、採用）
- [0024: 図面が宣言する文字高を描画要素ごとに保持する](0024-declared-text-height.md)（2026-10-23、採用）
- [0025: JWW・SXF の編集で部品・寸法・ハッチングを各形式の図形として追加する](0025-annotations-in-jww-and-sxf-edits.md)（2026-10-23、提案）
- [0026: JWW・SXF のスタイル操作を、各形式が保持できる値へ写す](0026-styles-in-jww-and-sxf-edits.md)（2026-10-23、提案）
- [0027: JWW・SXF テンプレートへ編集操作を通して作図する](0027-draft-into-jww-and-sxf.md)（2026-10-23、提案）
- [0028: IFC を自前の Rust 読み取りで扱い、STEP 構文を SXF と共有する](0028-ifc-in-rust.md)（2026-10-23、提案）
- [0029: pdf.js の文書を利用側で開き、描画部品へ渡す](0029-pdf-js-owned-by-the-application.md)（2026-10-23、提案）
- [0030: PDF の WebAssembly を分離し、PDF を開いたときだけ読む](0030-pdf-in-a-webassembly-module-of-its-own.md)（2026-10-23、提案）
- [0031: 画素数のない画像は用紙上の 0.01 mm を内容単位にする](0031-image-units-without-a-pixel-count.md)（2026-10-23、提案）
- [0032: 正解ファイルで確認済みの値の重複テストを除く](0032-avoid-duplicate-golden-assertions.md)（2026-10-24、採用）
- [0033: 日本の実務と日本語を標準にする](0033-japanese-first.md)（2026-10-24、採用）

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="design-decisions"></a>

[design-decisions](README.en.md#design-decisions)

<a id="template"></a>

[template](README.en.md#template)

<a id="records"></a>

[records](README.en.md#records)

</details>
