# 構成と設計

zumen は日本の建築・土木向けの図面ライブラリです。読み取りは Rust を共通実装にし、Python・CLI・ブラウザから同じ図面モデルを使います。画面・説明・注意事項は[日本語を標準](language-policy.md)にします。

```text
図面のバイト列 → Rust の読み取り → Scene（共通モデル）
                                ├→ Rust・Python・CLI・MCP の解析と描画
                                └→ WebAssembly → JSON → ブラウザの DrawingViewer → 利用側の画面
```

## パッケージと依存方向

| パッケージ | 役割 |
| --- | --- |
| `@zumenjs/core` | 形式に依存しない Scene・JSON・読み取りの共通 API・幾何・文字配置・計測・マークアップ。DOM は使わない |
| `@zumenjs/viewer` | Canvas 描画・当たり判定・吸着・表示範囲・入力・選択・計測・ツール・MarkupLayer |
| `@zumenjs/reader` | Rust の WebAssembly 読み取り、形式判定、入力上限、同梱ファイル、Web Worker |
| `@zumenjs/pdf` | 利用側の pdf.js 文書を使って PDF ページを下に描く。図形は読み取らない |
| `@zumenjs/react` | エンジン・状態・イベント・ファイル読み取り・マークアップのフック |
| `@zumenjs/sxf`・`@zumenjs/jww`・`@zumenjs/dxf` | テストの正解を作る TypeScript の参照実装。公開しない |

React・PDF → viewer → core、reader と各形式 → core の方向だけに依存します。形式同士は依存しません。例外として Rust の DWG は DXF と同じ AutoCAD モデルを使います。複数形式を写す `zumen-convert` は形式クレートではありません。

画面のラベル・ボタン・パネル・CSS は利用側が持ちます。サンプル画面も公開 API だけで作り、必要な機能が足りなければエンジンに追加します。エラー・属性ラベル・注意事項は日本語です。

## 表示用モデル

- `Scene` の座標は用紙上の mm、Y 軸は上向きです。部分図・部品・ブロックなどの配置を解決します。
- 図形は描画要素、外接矩形、所属・配置元のレイヤ、属性、必要なら縮尺と元の実寸座標へ戻す `Placement` を持ちます。
- 線・塗り・切り取り・点・文字を描画要素として表します。文字の `height` は描画用の em、`characterHeight` は図面が宣言する文字高です。製図基準の確認には宣言値を使います。
- `scene.underlays` は画像・PDF ページの配置を持ち、実体のバイト列は持ちません。利用側の描画処理が必要な範囲を描きます。
- `drawn: false` の図形は下敷きですでに表示される内容です。選択・計測・解析には使い、二重に描きません。
- 複数表示は `scene.views` に列挙し、`scene.view` が現在の表示です。DXF・DWG のモデル空間・レイアウト、PDF のページに使います。
- `scene.unit` と属性の未整形値は元の図面の単位を保ちます。実寸計測は配置を通して mm に換算します。
- 色は `#rrggbb` または `#rrggbbaa`。背景に近い白・黒は見分けられる反対色で描きます。
- 省略・代替は必ず `notices` に種類と件数を返します。

## 図形 id と属性キー

同じバイト列からは将来の版でも同じ図形 id を返します。SXF のインスタンス番号、JWW のレコード、DXF・DWG のハンドルと、配置元の情報から作ります。id の変更は事前の設計判断が必要です。独立した `crates/zumen/tests/ids.json` で検査します。

`kind`、属性の `key`、メタデータのキーは形式によらない識別子です。`type` と属性の `label` は日本語です。値は表示用の文字列と、未整形の `raw`・`values` を分けます。座標・長さの未整形値は元の形式の単位であり、表示用座標とは異なります。全キーの対応表は[英語の詳細表](architecture.en.md#stable-keys)に残しています。

<a id="dwg-in-the-browser"></a>

## ブラウザの読み取り

`@zumenjs/reader` は `bindings/wasm` を使います。PDF だけは大きな読み取り・フォント・CMap を分離した `bindings/wasm-pdf` を必要時に読み込みます。`detectDrawing` は読み込み前に Rust と同じ規則で形式を判定し、`reader.load(format)` が必要なモジュールを準備します。

Web Worker は図面を JSON で返し、画面側の `sceneFromJson` が復元します。大量の小さな型付き配列を構造化複製するより速かったためです。中断・時間超過で Worker を停止し、次の要求で作り直します。

PDF の図形・id は Rust が読み、ページの見た目はブラウザで pdf.js、サーバーで hayro が描きます。利用側が pdf.js の文書・Worker・配信データを管理し、zumen はその文書のメソッドを呼びます。

## サーバーでの利用

Rust の `zumen` が読み取り・変換・編集・作図をまとめ、`zumen-render` が PNG・SVG・PDF に描きます。Python は同じ拡張を使い、`zumen.tools` は JSON にできる上限付きの結果を返します。MCP は同じ関数を公開します。

文字・図形・表題欄・形の抽出、領域と面積、罫線の表、製図基準の確認、版の比較が共通モデルを読みます。画像にはピクセルと用紙上の mm の変換、描画した図形の範囲を付けられます。

## 入力上限

バイト数・図形数・時間を利用側が指定します。圧縮形式は展開後の大きさを数えます。再帰・ループにも共通の上限を設け、壊れた入力で処理が終わらないことを防ぎます。WebAssembly の時間制限は Worker の停止で実現します。

## 書き出し・編集・作図

変換は AutoCAD のモデルを経て DXF・DWG・JWW・SXF に書きます。モデル空間は実寸、用紙のレイアウトに縮尺ごとのビューポートを作ります。SXF の部分図・JWW のレイヤグループへ戻す際も縮尺を保ちます。

編集は操作対象以外の内容・形式・版・文字コードを保ちます。すべての操作を検証してから書き、失敗時は何も書きません。新規作成、定義の追加、テンプレートへの作図も同じ操作を使います。PDF は読み取り・描画・解析専用です。

## 依存と検証

実行時依存は寛容なライセンスで、ブラウザ・WebAssembly に対応するものを選びます。追加理由とライセンスの一覧は[依存の詳細記録](architecture.en.md#rust-dependencies)に保存しています。変更時はこの日本語資料にも追記します。

TypeScript と Rust の共通形式は同じ図面モデルを比較します。DWG は対応する DXF、PDF は自前で出力した PDF と比較します。正解ファイルの更新と id の互換性記録は分けます。

開発環境・検証コマンドは[開発への参加](../CONTRIBUTING.md)、形式固有の判断は `docs/formats/`、選択肢と理由は[設計判断](decisions/README.md)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [architecture.en.md](architecture.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="architecture"></a>

[architecture](architecture.en.md#architecture)

<a id="packages-and-dependency-rules"></a>

[packages-and-dependency-rules](architecture.en.md#packages-and-dependency-rules)

<a id="npm-dependencies"></a>

[npm-dependencies](architecture.en.md#npm-dependencies)

<a id="the-scene-model"></a>

[the-scene-model](architecture.en.md#the-scene-model)

<a id="entity-ids"></a>

[entity-ids](architecture.en.md#entity-ids)

<a id="stable-keys"></a>

[stable-keys](architecture.en.md#stable-keys)

<a id="adding-a-format"></a>

[adding-a-format](architecture.en.md#adding-a-format)

<a id="extension-points-for-applications"></a>

[extension-points-for-applications](architecture.en.md#extension-points-for-applications)

<a id="the-rust-implementation"></a>

[the-rust-implementation](architecture.en.md#the-rust-implementation)

<a id="the-readers-in-the-browser"></a>

[the-readers-in-the-browser](architecture.en.md#the-readers-in-the-browser)

<a id="texts-entities-and-images-for-servers"></a>

[texts-entities-and-images-for-servers](architecture.en.md#texts-entities-and-images-for-servers)

<a id="limits-for-untrusted-drawings"></a>

[limits-for-untrusted-drawings](architecture.en.md#limits-for-untrusted-drawings)

<a id="writing-and-converting"></a>

[writing-and-converting](architecture.en.md#writing-and-converting)

<a id="editing-in-place"></a>

[editing-in-place](architecture.en.md#editing-in-place)

<a id="keeping-the-two-implementations-equal"></a>

[keeping-the-two-implementations-equal](architecture.en.md#keeping-the-two-implementations-equal)

<a id="rust-dependencies"></a>

[rust-dependencies](architecture.en.md#rust-dependencies)

<a id="development-setup"></a>

[development-setup](architecture.en.md#development-setup)

</details>
