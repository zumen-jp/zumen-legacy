# 開発エージェント向けの規則

## 日本語ファースト

- 日本の建築・土木の実務を主な対象とする。SXF による電子納品、Jw_cad、日本語の図面、国土交通省の CAD 製図基準を優先する。
- 説明はわかりやすく簡潔な日本語にする。README・利用ガイド・公開 API の説明・エラー・注意事項・CLI・MCP・サンプル画面・Issue・PR の本文は日本語を標準にする。設計判断も日本語で記録する。
- 日本語の文書を正本にし、英語は補足として提供する。既存の英語による詳細な仕様や検証履歴は `.en.md` に保存する。第三者のライセンスと仕様の引用は原文を保つ。
- 公開 API の関数名・型名、コマンド名・オプション、JSON キー、エラーコードなどの識別子は互換性を保つ。図面に記録された名前・文字・単位は勝手に翻訳・変更しない。
- 詳細は [言語と対象利用者の方針](docs/language-policy.md)を参照する。

## 検証

- 検証の入口は `pnpm run check`。Biome、WebAssembly の読み取り（`pnpm run wasm`。`Cargo.lock` と同じ版の wasm-bindgen-cli が必要）、型検査、単体テスト、`pnpm audit`、全パッケージと試用画面のビルド、`check:rust`、`check:scripts`、`check:python` を実行する。
- 表示に影響する変更は `pnpm dev` の試用画面で実際の図面を開いて確認する。`*.browser.test.*` は Chromium・Firefox・WebKit で実行する。準備は `pnpm exec playwright install --only-shell chromium firefox webkit`。`ZUMEN_BROWSERS=chromium` で対象を絞れる。
- 各形式の公開仕様に従い、コメントに節番号を記載する。推測した挙動は `docs/formats/` に記録する。
- 描かないもの・異なる形で描くものは、種類と件数を `notices` に報告する。黙って捨てない。
- 受領図面・公開サンプルを Git に入れない。リポジトリ外の `ZUMEN_SAMPLES` フォルダーから開く。テストは規則を確かめる最小の図面を各形式の `tests/` で生成する。
- クライアント名・クライアントの図面・非公開リポジトリの情報を入れない。

## 構成と依存関係

- 実行時依存を追加する前に、ライセンス（GPL・AGPL は不可）とブラウザでの動作を確認し、理由を `docs/architecture.md` に記録する。Rust の依存も同じ規則に従い、WebAssembly で動くことを確認する。
- 依存の向きは `@zumenjs/react` → `@zumenjs/viewer` → `@zumenjs/core`、`@zumenjs/pdf` → `@zumenjs/viewer` → `@zumenjs/core`、各形式 → `@zumenjs/core` のみ。
- `@zumenjs/pdf` は PDF のページを描くだけで、図形を読み取らない。pdf.js（`pdfjs-dist`、peer dependency）の型と利用側が開いた文書を使い、`@zumenjs/reader` に依存しない。
- `@zumenjs/core` に DOM と形式の知識を入れない。形式パッケージ同士を依存させない。Rust の `zumen-core` にもある機能は `@zumenjs/core`、描画・入力・当たり判定などブラウザ専用の機能は `@zumenjs/viewer` に置く。
- DWG は Rust の `zumen-dwg` だけで読み、`zumen-dxf` の AutoCAD 図面モデルに変換して `dxf_to_scene` で描く。これは形式クレート同士の依存の例外。複数形式の変換を受け持つ `zumen-convert` は形式クレートではない。
- PDF も TypeScript の読み取りを持たない。`zumen-pdf` は hayro-interpret で読み、呼び出し順が id に影響するため版を固定する。`convert`・`edit`・`draft` は PDF を受け付けない。
- ブラウザは `@zumenjs/reader` から `bindings/wasm` と PDF 専用の `bindings/wasm-pdf` を使う。PDF モジュールは初めて PDF を読むときに読み込む（[決定 0030](docs/decisions/0030-pdf-in-a-webassembly-module-of-its-own.md)）。生成先 `packages/reader/wasm` は Git に入れない。
- `detectDrawing` はモジュール読み込み前に形式を判定するため Rust の判定規則を再実装し、テストで比較する。`zumen` クレートの PDF は `pdf` feature で、`bindings/wasm` は無効にする。
- TypeScript の SXF・JWW・DXF 読み取りは正解ファイルを生成する参照として残し、公開しない。
- 解析（`parse*`・`read*` とその下位処理）は DOM・React・描画に依存せず、`@zumenjs/core` の型と `NoticeList` だけを使う。`*ToScene` は幾何・文字配置の補助関数を使える。
- 表示範囲・選択・計測・入力・ツールは `DrawingViewer` に置く。React はエンジンの作成、状態・イベントの取得、ファイル読み取りのフックだけを持つ。
- パッケージに画面のラベル・ボタン・パネル・CSS を同梱しない。文言は利用側、配色は CSS カスタムプロパティ（`--zumen-*`）が持つ。エラー・属性ラベル・注意事項は日本語で返す。完成した画面は公開 API だけを使う試用サンプルに置き、不足する API はエンジンに追加する。
- 公開に第三者の許可を要する形式・部品（buildingSMART Japan が権利を持つ ST-Bridge など）、寛容でないライセンス（MPL・LGPL・GPL など）の部品は、先に `docs/decisions/` に判断を記録する（[決定 0013](docs/decisions/0013-st-bridge-out-of-zumen.md)）。

## 読み取りの互換性

- Rust は TypeScript の構造・処理順と同じにし、同じ `Scene` を返す。DWG は同じ図面を AutoCAD で DXF にした結果と比較する。PDF は各テスト図面を `render_pdf` で書いたものと比較し、`pnpm run parity` では pdf.js との差も報告する。
- 数値の表記と並べ替えは `zumen_core::js` を使う（`js::sort_by` は V8 と同順）。壊れた入力に対する再帰・ループは両言語で共通の上限を設ける。図形追加は `zumen_core::limits::admit`、入力レコードを巡回するループは `limits::tick` を使う。
- TypeScript の読み取りを変えたら `pnpm run golden` を実行し、`cargo test -p zumen` が通るまで Rust も修正する。正解ファイルを手で書き換えない。
- 図形 id は契約。同じバイト列は将来の版でも同じ id を返す。`crates/zumen/tests/ids.rs` は `ids.json` と比較し、`pnpm run golden` はその記録を更新しない。テスト用データの追加・変更は id を確認して `ZUMEN_RECORD_IDS=1 cargo test -p zumen --test ids` で記録する。同じ入力の id を変える場合は先に設計判断が必要。
- Python 拡張・WebAssembly にリンクする Rust クレートと同梱データ（hayro のフォント・CMap）のライセンスは 2 つの `THIRD_PARTY_LICENSES.txt` に保存する。`Cargo.lock` の依存が増減・更新されたら `pnpm run notices` で更新し、コミットする。`check:rust` が一致を検査する。

## 開発手順

- 最新の `origin/main` からブランチを作り、すべての変更を PR にする。`main` へ直接 push しない。push 前に `pnpm run check` を実行し、レビューで止める依頼がなければ CI 成功後に squash merge してブランチを削除する。
- コミットと PR タイトルは Conventional Commits。種別・スコープはその形式を保ち、説明は日本語を基本にする。
- 選択肢と理由を伴う設計判断は `docs/decisions/README.md` の形式で日本語で記録する。
- 開発ポートは `config/local-ports.json`（試用画面は 24100）。zumenjs の各リポジトリは 24100〜24199 を使う。
- 手動確認の図面はリポジトリ外に置き、`ZUMEN_SAMPLES` で指定する。`.claude/launch.json` など端末固有のファイルは `.git/info/exclude` で Git 対象から外す。
