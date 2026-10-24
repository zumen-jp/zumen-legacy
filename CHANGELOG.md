# 変更履歴

利用者向けの変更履歴は日本語で記載します。過去の個別の修正・対応形式・検証範囲は [CHANGELOG.en.md](CHANGELOG.en.md) に保存しています。

## 未リリース

- 日本の建築・土木向け、日本語ファーストの方針に変更しました。
- 既定の README、利用ガイド、設計判断、開発手順、パッケージの説明を日本語中心にしました。英語の詳細資料は補足として残しています。
- Python API・CLI・MCP の説明と、ライブラリが返す利用者向けエラーを日本語にしました。識別子・JSON キー・エラーコードと図面由来の文字は維持します。
- PDF 図面の読み取りとページ描画、図面の画像・参照、マークアップ、製図基準の確認、編集・作図など、従来の未リリース変更の詳細は英語の履歴を参照してください。
- DXF・DWG の PDF の下敷き（PDFUNDERLAY）を、`skipped` に数えずに `kind: "pdf"` の下敷きとして読むようにしました（[決定 0034](docs/decisions/0034-place-pdf-underlays-after-reading.md)、[PDF の下敷き](docs/formats/dxf.md#pdf-underlays)）。図面はページの大きさを持たないため、利用側が `@zumenjs/pdf` の `placePdfUnderlays`（または `@zumenjs/core` の `placeUnderlays`）で置くまで描きません。縮尺 1 で PDF の 1 インチを図面の 1 単位とします（推測）。書き出しは PDF の下敷きを省いて報告し、試用画面は図面の隣の PDF を選ぶと置いて描きます。

## 0.1.1 — 2026-10-21

読み取り・描画・配布に関する修正です。個別の修正内容は[当時の記録](CHANGELOG.en.md#011--2026-10-21)を参照してください。

## 0.1.0 — 2026-10-20

初回リリースです。SXF・JWW・DXF・DWG の読み取りと図面モデル、ブラウザでの表示・計測、Rust・Python・CLI などを公開しました。[詳細](CHANGELOG.en.md#010--2026-10-20)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [CHANGELOG.en.md](CHANGELOG.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="changelog"></a>

[changelog](CHANGELOG.en.md#changelog)

<a id="unreleased"></a>

[unreleased](CHANGELOG.en.md#unreleased)

<a id="011--2026-10-21"></a>

[011--2026-10-21](CHANGELOG.en.md#011--2026-10-21)

<a id="010--2026-10-20"></a>

[010--2026-10-20](CHANGELOG.en.md#010--2026-10-20)

</details>
