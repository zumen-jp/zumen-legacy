# 0004: Python パッケージに MCP サーバーを置く

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

AI アプリケーションから既存の読み取り・描画・計測を使えるようにしたい。

## 選択肢

別パッケージ、Rust・TypeScript のサーバー、Python の任意モジュールを比較した。

## 決定

公式 Python SDK を任意依存として zumen.mcp に置き、zumen-mcp で stdio 通信する。ツールの処理は直接呼べる関数に分ける。

## 影響

MCP なしでも利用できる。許可パス・入力サイズ・返却件数・画像サイズに上限を設け、図面の文字を指示として扱わない。後続の機能追加は各記録に従う。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0004-mcp-server-in-the-python-package.en.md](0004-mcp-server-in-the-python-package.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0004-serve-drawings-to-ai-applications-through-an-mcp-server-in-the-python-package"></a>

[0004-serve-drawings-to-ai-applications-through-an-mcp-server-in-the-python-package](0004-mcp-server-in-the-python-package.en.md#0004-serve-drawings-to-ai-applications-through-an-mcp-server-in-the-python-package)

<a id="context"></a>

[context](0004-mcp-server-in-the-python-package.en.md#context)

<a id="options"></a>

[options](0004-mcp-server-in-the-python-package.en.md#options)

<a id="decision"></a>

[decision](0004-mcp-server-in-the-python-package.en.md#decision)

<a id="consequences"></a>

[consequences](0004-mcp-server-in-the-python-package.en.md#consequences)

<a id="references"></a>

[references](0004-mcp-server-in-the-python-package.en.md#references)

</details>
