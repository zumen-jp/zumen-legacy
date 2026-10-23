# リリース手順

公開する README・説明・変更履歴は日本語を標準にします。公開パッケージの識別子とバージョン規則は維持します。

## 公開前の確認

1. `pnpm run check` を実行し、CI の必須検査を通します。
2. README・CHANGELOG・形式ごとの対応範囲を更新します。
3. npm と Python のバージョン、およびワークスペースの参照を揃えます。
4. ライセンス・同梱フォント・第三者の通知を確認します。Rust の依存変更時は `pnpm run notices` を実行します。
5. GitHub のリリースワークフローを確認し、対象のタグを作ります。

npm は公開対象パッケージだけをビルドして公開します。TypeScript の SXF・JWW・DXF は参照実装のため公開しません。認証情報・OIDC・権限などの公開先設定が必要です。初回公開と追加パッケージの設定を含む具体的な操作は、既存の[詳細手順](releasing.en.md)と `.github/workflows/npm-release.yml` を参照してください。

## バージョン

初期リリースは 0.x です。利用側への影響を変更履歴に記載し、互換性に関わる変更は設計判断と合わせます。コミット・PR は Conventional Commits に従います。

## Python wheel

`.github/workflows/python-wheels.yml` が環境ごとの wheel とソース配布物を作ります。Python の最小対応版と配布先は `bindings/python/pyproject.toml` とワークフローを正本にします。

```sh
uv run --project bindings/python --frozen maturin build --release -m bindings/python/Cargo.toml --out target/wheels
node scripts/check-wheel.mjs
```

wheel の検査はチェックアウト外の新しい仮想環境で行います。`target/wheels` は対象の wheel を 1 つにし、インポート、日本語のファイルパス、読み取り、PNG 描画まで確認します。

公開する版の変更履歴には対応形式、API の変更、省略・代替、検証済みの範囲を日本語で記載します。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [releasing.en.md](releasing.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="releasing"></a>

[releasing](releasing.en.md#releasing)

<a id="before-the-first-package-release"></a>

[before-the-first-package-release](releasing.en.md#before-the-first-package-release)

<a id="what-the-workflows-do"></a>

[what-the-workflows-do](releasing.en.md#what-the-workflows-do)

<a id="versions"></a>

[versions](releasing.en.md#versions)

<a id="first-release"></a>

[first-release](releasing.en.md#first-release)

<a id="a-package-added-after-the-first-release"></a>

[a-package-added-after-the-first-release](releasing.en.md#a-package-added-after-the-first-release)

<a id="later-releases"></a>

[later-releases](releasing.en.md#later-releases)

<a id="python-wheels"></a>

[python-wheels](releasing.en.md#python-wheels)

</details>
