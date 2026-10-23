# 0011: MCP の処理を zumen.tools として直接呼べるようにする

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

MCP 以外のエージェント実装からも同じ道具と結果の型を使いたい。

## 選択肢

MCP を経由する、各アプリで同じ処理を書く、共通関数を公開する方法を比較した。

## 決定

zumen.tools に Scene を受け取る関数と TypedDict の結果を置く。MCP はその薄い入口とし、JSON にできる値を返す。

## 影響

SDK に依存せずアプリから使える。座標・返却上限・省略の契約を文書化し、道具の処理を二重に持たない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0011-tool-functions-for-agents.en.md](0011-tool-functions-for-agents.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0011-give-ai-agents-the-functions-behind-the-mcp-tools-as-zumentools-with-a-documented-schema"></a>

[0011-give-ai-agents-the-functions-behind-the-mcp-tools-as-zumentools-with-a-documented-schema](0011-tool-functions-for-agents.en.md#0011-give-ai-agents-the-functions-behind-the-mcp-tools-as-zumentools-with-a-documented-schema)

<a id="context"></a>

[context](0011-tool-functions-for-agents.en.md#context)

<a id="options"></a>

[options](0011-tool-functions-for-agents.en.md#options)

<a id="decision"></a>

[decision](0011-tool-functions-for-agents.en.md#decision)

<a id="consequences"></a>

[consequences](0011-tool-functions-for-agents.en.md#consequences)

<a id="references"></a>

[references](0011-tool-functions-for-agents.en.md#references)

</details>
