# 開発への参加

大きな変更は着手前に Issue で相談してください。Issue・PR・説明・コメントは日本語を基本にします。参加者には[行動規範](CODE_OF_CONDUCT.md)が適用されます。

## 図面の取り扱い

図面はクライアントの所有物であることがあります。機密図面を Issue・PR に添付したり、Git に入れたりしないでください。不具合の報告には最小の再現用図面を作るか、形式・版・作成ソフト・表示された注意事項を記載してください。

## 開発環境

- Node.js は `.node-version`、pnpm は `package.json` の `packageManager` に合わせます。
- Rust は `rust-toolchain.toml`、Python は uv と `bindings/python/.python-version` に合わせます。
- `node scripts/install-wasm-bindgen.mjs` で `Cargo.lock` と同じ版の wasm-bindgen-cli を入れます。

```bash
pnpm install --frozen-lockfile
node scripts/install-wasm-bindgen.mjs
pnpm exec playwright install --only-shell chromium firefox webkit
pnpm run check
```

`pnpm dev` は WebAssembly を作って試用画面を起動します。ポートは `config/local-ports.json` の `playground`（既定 24100）で、`PORT` で変更できます。
図面を Git に入れずに開くには、リポジトリ外のフォルダーを `ZUMEN_SAMPLES` に指定し、`/?sample=<ファイル名>` にアクセスします。画面は公開 API だけで作る日本語のサンプルです。

## Windows

PowerShell 7 と MSVC の Rust、C++ ビルドツール、Windows SDK を使います。Developer PowerShell から実行し、Node.js・Rust・Python は Windows 側に置いてください。既存の Python を使う場合は `UV_PYTHON_DOWNLOADS` を `never` にできます。

```powershell
pnpm install --frozen-lockfile
node scripts/install-wasm-bindgen.mjs
pnpm exec playwright install --only-shell chromium
$env:ZUMEN_BROWSERS = "chromium"
pnpm run check
$env:ZUMEN_SAMPLES = "D:\zumen-samples"
pnpm dev
```

環境変数を指定するコマンドは `$env:ZUMEN_RECORD_IDS = "1"` のように設定し、実行後に `Remove-Item Env:ZUMEN_RECORD_IDS` で解除します。`UPDATE_GOLDEN` も同様です。CI は Linux と Windows で全検査を行い、Windows のブラウザテストは Chromium に絞ります。日本語・空白を含むパスと、インストール済み wheel も検査します。

## 実装と検証

[構成](docs/architecture.md)と [AGENTS.md](AGENTS.md)に依存方向・入力上限・id の互換性を定めています。公開仕様の節をコメントに書き、推測は形式別の資料に記録してください。省略・代替は `notices` で報告します。

`pnpm run check` が検証の入口です。表示を変えたら実際の図面でも確認します。テストは文字コード・座標系・縮尺・入れ子・重大な不具合の再発防止など、誤りを検出するものを追加し、実装や文言をそのまま写すテストは追加しません。

TypeScript の読み取りを変えたら `pnpm run golden` を実行し、Rust も同じ結果にします。id の互換性記録は別で、正解ファイルを作り直しただけでは更新されません。実際の図面での比較は `pnpm run parity <フォルダー>`、破損入力の比較は `pnpm run fuzz` を使います。

実行時依存を追加する前に、寛容なライセンスとブラウザ・WebAssembly 対応を確認して構成資料に記録します。`Cargo.lock` を変えたら `pnpm run notices` も実行します。

## 書式と PR

Biome・rustfmt・Ruff を使い、行の長さは原則 120 文字です。利用者向けの説明・新しいコメント・設計判断は日本語を標準にします。識別子と規格名、第三者の原文は保ちます。

PR は 1 つの目的に絞り、必要な文書と検証結果を含めます。コミットと PR タイトルは `feat(sxf): …`、`fix(core): …` など Conventional Commits に従い、説明は日本語を基本にします。CI 成功後に squash merge します。

## 投稿のライセンス

zumen は [MIT ライセンス](LICENSE)です。PR を送ることで、投稿した変更も同じ条件で公開することに同意したものとします。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [CONTRIBUTING.en.md](CONTRIBUTING.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="contributing-to-zumen"></a>

[contributing-to-zumen](CONTRIBUTING.en.md#contributing-to-zumen)

<a id="never-share-confidential-drawings"></a>

[never-share-confidential-drawings](CONTRIBUTING.en.md#never-share-confidential-drawings)

<a id="setup"></a>

[setup](CONTRIBUTING.en.md#setup)

<a id="windows"></a>

[windows](CONTRIBUTING.en.md#windows)

<a id="how-the-code-is-organized"></a>

[how-the-code-is-organized](CONTRIBUTING.en.md#how-the-code-is-organized)

<a id="tests"></a>

[tests](CONTRIBUTING.en.md#tests)

<a id="style"></a>

[style](CONTRIBUTING.en.md#style)

<a id="pull-requests"></a>

[pull-requests](CONTRIBUTING.en.md#pull-requests)

<a id="license-of-contributions"></a>

[license-of-contributions](CONTRIBUTING.en.md#license-of-contributions)

</details>
