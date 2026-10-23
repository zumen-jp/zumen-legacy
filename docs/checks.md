# CAD 製図基準の確認

レイヤ名・色・線種・線幅・文字高・使用文字・用紙サイズと向き・尺度を、JSON の規則集で確認します。国土交通省の組み込み規則と、組織独自の規則を同じ仕組みで扱います。

| 環境 | 呼び出し |
| --- | --- |
| Rust | `RuleSet::built_in("mlit-cad-2025")`、`RuleSet::from_json`、`check_scene` |
| Python | `scene.check(rules)`、上限付きの `zumen.tools.check_drawing` |
| CLI | `zumen check plan.sfc --rules mlit-cad-2025` |
| MCP | `check_drawing` |

## 結果の読み方

`checked` は確認した対象数、`unchecked` は図面に値がなく確認できなかった対象数です。`findings` は違反した値ごとに、日本語の説明・件数・図形 id・レイヤを返します。

`passed` は重要度 `error` の違反がないことを示します。すべての規則を確認できたことや、電子納品の要件をすべて満たすことを示す値ではありません。`warning`・`info` は報告しますが、CLI の失敗にはしません。

## 独自の規則集

```json
{
  "name": "office",
  "title": "社内の製図規則",
  "rules": [
    {"id": "layers", "kind": "layer-name", "level": "error", "patterns": ["A-*", "S-*"], "maxWidth": 31},
    {"id": "text", "kind": "text-height", "heights": [2.5, 3.5, 5], "tolerance": 0.05}
  ]
}
```

各規則に一意の `id` と `kind` が必要です。`level` は `error`・`warning`（既定）・`info`。`reference` は根拠の節、`description` は説明です。図形の規則は `layers` で対象レイヤを絞れます。

レイヤ名のパターンは大文字小文字を区別します。`*` は任意の文字列、`?` は 1 文字、`[SDCM]` はそのどれか、`[A-Z]` は範囲、`[!0-9]` は数字以外です。

| kind | 設定 | 確認する内容 |
| --- | --- | --- |
| `layer-name` | `patterns`・`maxWidth` | 図形を持つレイヤの名前。ASCII・半角カナを 1、その他を 2 と数える |
| `color` | `colors` | 実際に描く RGB 色。不透明度は比較しない |
| `line-type` | `names` | 線種名。括弧の外・中も比較し、単なる番号なら未確認 |
| `line-width` | `widths`・`tolerance` | mm の線幅。数値を得られない継承値などは未確認 |
| `text-height` | `heights`・`tolerance` | 用紙上の宣言文字高。用紙がないモデル空間は未確認 |
| `characters` | `ranges` | 文字とレイヤ名の Unicode 符号位置 |
| `sheet-size` | `sizes`・`tolerance` | 向きを問わない用紙サイズ。用紙なしは未確認 |
| `orientation` | `orientation` | `landscape`・`portrait` |
| `scale` | `scales`・`mantissas` | 縮尺 1:N の N、または仮数×10ⁿ |

## 国土交通省の組み込み規則

`mlit-cad-2025` は CAD製図基準・同解説（土木、令和7年12月）を対象とします。規則データは [mlit-cad-2025.json](../crates/zumen-core/rules/mlit-cad-2025.json) です。

- レイヤ名は `[SDCM]-<対象>` を基本とし、ユーザー領域を追加できます。半角換算で 256 文字以内、`ORD` は C の発注用レイヤです。
- 色は基準の 16 色、線種は JIS Z 8312 の 15 種を確認します。
- 線幅は 0.13・0.18・0.25・0.35・0.5・0.7・1・1.4・2 mm、文字高は 1.8・2.5・3.5・5・7・10・14・20 mm です。
- 半角カナ、丸数字、ローマ数字、単位記号など、規則集に列挙した機種依存文字を指摘します。
- 用紙は A0〜A4、向きと尺度は基準の推奨値を確認します。例外がある項目は warning・info として扱います。

線の用途別の太さ、図枠と余白、表題欄の寸法・項目、ファイル名、フォント、JIS X 0208 以外のすべての文字は、まだ網羅していません。根拠の節と全規則は英語の補足資料にも保存しています。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [checks.en.md](checks.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="checks-against-cad-standards"></a>

[checks-against-cad-standards](checks.en.md#checks-against-cad-standards)

<a id="results"></a>

[results](checks.en.md#results)

<a id="rule-sets"></a>

[rule-sets](checks.en.md#rule-sets)

<a id="built-in-rule-sets"></a>

[built-in-rule-sets](checks.en.md#built-in-rule-sets)

<a id="mlit-cad-2025-cad製図基準令和7年12月"></a>

[mlit-cad-2025-cad製図基準令和7年12月](checks.en.md#mlit-cad-2025-cad製図基準令和7年12月)

</details>
