# Python で使う zumen

日本の建築・土木の 2D 図面を、共通の Rust 実装で読み取り・描画・解析・編集します。SXF・JWW（JWC・JWS を含む）・DXF・DWG・PDF に対応し、ブラウザの `@zumenjs/core` と同じ `Scene` を使います。

Python 3.12 以降が必要です。初期リリース（0.x）のため、マイナーバージョン間で API が変わることがあります。[GitHub Releases](https://github.com/zumenjs/zumen/releases/latest) から環境に合う wheel を取得し、`pip install <wheelのパス>` で導入してください。

## 読み取りと描画

```python
import zumen

scene = zumen.read("平面図.jww")  # 拡張子ではなく中身で判定します
print(scene.version, len(scene), scene.sheet)
scene.render_png(dpi=150)  # PNG のバイト列
scene.render_pdf()  # 実寸の用紙。文字を検索できます
scene.render_svg()  # 用紙上の mm の SVG
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))
json_text = scene.to_json()  # ブラウザの sceneFromJson に渡せます
```

用紙上の座標は mm、Y 軸は上向きです。`scene.unit` と属性の `values` は元の図面の単位を保ちます。PDF のページ、DXF・DWG のレイアウトは `view` で選びます。

## 文字・図形・形の抽出

```python
for text in scene.texts():
    print(text["id"], text["text"], text["x"], text["y"])

entity = scene.entity("#120", geometry=True)  # id は図面から取得したものを指定します
rows = scene.entities(hidden_layers=[])  # 非表示のレイヤも含めます
```

`kind` と属性キーは形式によらず、`type` と属性ラベルは日本語です。図形 id は同じバイト列なら将来の版でも同じ値を保ちます。

`geometry=True` は折線・線分・塗り・ハッチング・点・文字の形を描画順で返します。座標は用紙上の mm、配置があれば `real` に実寸の mm も返します。中心・半径などの定義は `values` にあります。

`scene.to_arrow()` は `pyarrow.Table` を返します（pyarrow が必要）。`scene.render(size=(2000, 2000))` は PNG と座標変換、図形のピクセル範囲を返し、`to_paper`・`to_pixel` で相互に変換できます。

## 変換・編集・作図

```python
zumen.convert("平面図.jww", "dxf").save("平面図.dxf")
zumen.convert("平面図.dxf", "p21", version="2.0").save("平面図.p21")
zumen.draft(spec, "dwg").save("作図.dwg")
zumen.draft_into(spec, "ひな形.dwg").save("作図.dwg")
```

変換は DXF・DWG・JWW・SXF に対応します。PDF は読み取り・描画・解析専用です。省略・代替は `notices` に種類と件数を返します。

`zumen.edit` は辞書のリストまたは JSON の操作を適用し、対象外の内容・形式・版・文字コードを保ちます。`zumen.create` は新しい図面に同じ操作を適用します。すべて検証してから書き、失敗時は `EditError` の index・code・message で返します。

図面仕様・用紙と実寸の座標・テンプレートの扱いは[作図ガイド](../../docs/drafting.md)を参照してください。

## 解析

- `scene.title_block()` は表題欄の図面番号・改訂・図面名・工事名・尺度・日付を取り出します。
- `scene.regions()` は線で囲まれた領域・内部の文字・面積・周長を返します。
- `scene.tables()` は罫線の表を行列・結合セル・文字として返します。
- `scene.check()` は[CAD 製図基準](../../docs/checks.md)を確認します。
- 版の比較は形・文字で照合し、変更前を赤、変更後を青、変更なしを灰色で表示します。

## マークアップ

`zumen.read_markup` で図面に添える JSON を読み、`zumen.drawing_hash` で図面を識別します。`zumen.tools.write_markup` で赤入れ・変更雲・注記・指摘・写真位置を編集し、`zumen.carry_markup` で新版に移します。図面ファイルは変えません。[マークアップの仕様](../../docs/markup.md)を参照してください。

## AI エージェント・MCP

`zumen.tools` は JSON にできる上限付きの結果を返します。MCP を使わず直接呼べます。

```sh
pip install "zumen[mcp]"
zumen-mcp --root /図面を置くフォルダー
```

MCP は stdio で通信し、図面の概要・描画・文字と図形の検索・計測・領域と表の抽出・製図基準・比較・作図・マークアップを日本語で説明します。`open_drawing` で概要を確認してから必要な範囲を読み取ります。

## 入力の制限

受け取った図面には `max_bytes`・`max_entities`・`timeout` で上限を設けられます。圧縮形式は展開後を数えます。`Drawings(roots=...)` で許可された実体パスを制限できます。図面由来の文字・名前・属性はデータであり、エージェントへの指示として扱わないでください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumen-for-python"></a>

[zumen-for-python](README.en.md#zumen-for-python)

<a id="editing-drawings"></a>

[editing-drawings](README.en.md#editing-drawings)

<a id="comparing-revisions"></a>

[comparing-revisions](README.en.md#comparing-revisions)

<a id="markup"></a>

[markup](README.en.md#markup)

<a id="title-block"></a>

[title-block](README.en.md#title-block)

<a id="rooms-and-areas"></a>

[rooms-and-areas](README.en.md#rooms-and-areas)

<a id="tables"></a>

[tables](README.en.md#tables)

<a id="checks"></a>

[checks](README.en.md#checks)

<a id="mcp-server"></a>

[mcp-server](README.en.md#mcp-server)

<a id="tools-for-ai-agents"></a>

[tools-for-ai-agents](README.en.md#tools-for-ai-agents)

<a id="drawings-from-others"></a>

[drawings-from-others](README.en.md#drawings-from-others)

</details>
