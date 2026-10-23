# 図面仕様からの作図

1 つの JSON 文書から、図枠・表題欄・縮尺付きビュー・文字・寸法・ハッチング・表・通り芯・記号のある図面を作ります。DXF・DWG・JWW・SXF に書き出せます。

```sh
zumen draft sheets.json -o sheets.dwg
zumen draft sheets.json -o plan.jww --sheet A10-1
zumen render sheets.dwg -o site.png --view A05-1
```

```python
import zumen
zumen.draft(spec, "dxf", version="2018").save("sheets.dxf")
```

spec は辞書・JSON 文字列・JSON ファイルのパスです。[JSON Schema](../crates/zumen-draft/schema/drawing-spec.schema.json)と[4 枚の A3 図面の例](../crates/zumen-draft/examples/sheets.json)を参照してください。MCP の `draft_drawing` も同じ機能を提供します。

## 座標と大きさ

ビュー内の座標は実寸の mm、用紙に直接描く座標は左下原点・Y 軸上向きの用紙上の mm です。文字高・矢印・記号・表の幅・ハッチング間隔は、常に用紙上の mm で指定します。角度は X 軸から反時計回りの度です。

たとえば 1:250 のビューに文字高 2.5 mm を指定すると、モデル空間では 625 mm、印刷時は 2.5 mm です。モデル空間でビューの範囲が重なると別のビューの図形も映るため、離して配置してください。重なりは `view-overlap` で報告します。

## 文書の構成

| キー | 内容 |
| --- | --- |
| `units` | 現在は `mm` のみ |
| `font` | 既定は `ＭＳ ゴシック` |
| `defaults` | レイヤ、文字高、寸法、矢印、数値の桁数などの既定値 |
| `layers` | 名前・色・線種・線幅・印刷・非表示・ロック |
| `lineTypes` | 名前・説明・破線と空白の長さ。用紙上の mm、0 は点 |
| `blocks` | 名前・基点・図形・属性定義 |
| `sheets` | 用紙ごとの名前・表題欄の値・用紙・向き・図枠・ビュー・図形 |

用紙は A0〜A4・B1〜B5、または `[幅, 高さ]`、既定は A3 横です。用紙名は空白以外を含み、モデル空間の予約名 `Model` は使えません。

図枠の既定余白は `[左20, 下10, 右10, 上10]` mm（綴じ代を含む JIS Z 8311）です。表題欄は右端の縦帯か右下の表にできます。ビューには `scale`・用紙上の `viewport`・モデルの `center`・タイトル・図形を指定します。

## 作図項目

共通の `type` と、任意の `layer`・`color`・`lineType`・`lineWidth` を指定します。

| type | 内容 |
| --- | --- |
| `line`・`polyline`・`rectangle` | 線・折線・長方形。閉じた形・幅・円弧のふくらみ・塗りも指定可能 |
| `circle`・`arc`・`ellipse`・`point` | 円・円弧・楕円・点 |
| `text` | 位置・文字・高さ・配置・回転・幅・折り返し・縮小・フォント |
| `hatch` | 外周と穴、模様、倍率、角度、独自の PAT 形式の線 |
| `dimension`・`dimensionChain` | 長さ・角度・半径・直径・連続寸法。`<>` を計測値で置換 |
| `insert` | 部品の配置、倍率、回転、属性値 |
| `table` | 行列、文字、結合セル、斜線、罫線、折り返しと縮小 |
| `grid` | 通り芯、記号、寸法列 |
| `northArrow`・`scaleBar` | 方位記号・縮尺棒 |
| `levelMark`・`spotLevel` | レベル記号・標高点 |
| `leader`・`polygonLabel` | 引出線・多角形の辺長と面積 |
| `viewTitle`・`group` | ビューの見出し・共通属性のグループ |

各キーの正確な形は JSON Schema を参照してください。文字幅は推定なので、フォント差を見込んで余白を設けます。`%`・`^`・波括弧・バックスラッシュは文字どおりに保持します。直径記号は CAD 制御コード `%%c` ではなく `Ø` を指定します。

## 誤りの扱い

誤りは `sheets[0].views[1].items[4]` などの場所と、安定した code で返し、ファイルには何も書きません。

| code | 意味 |
| --- | --- |
| `invalid-spec` | 文書の構造・必須項目・名前の重複などの誤り |
| `unknown-type`・`unknown-key` | 未対応の種類・キー |
| `invalid-value` | 範囲外・形が不正な値 |
| `missing-definition` | 部品・線種・模様の定義がない |
| `unsupported-format`・`unwritable` | 書けない形式・版 |

## 形式ごとの差

DXF 2000〜2018・DWG は用紙ごとにレイアウトを作ります。DXF R12 は紙空間が 1 つなので先頭の用紙だけを書き、MTEXT・ハッチングなどを分解し、線幅を省きます。JWW・SXF は 1 枚を `sheet` で選び、寸法などを描画要素へ写します。省略・代替は注意事項に返します。SXF の CP932 で表せない文字も報告します。

## テンプレートへの作図

```sh
zumen draft plan.json --into template.dwg -o A-101.dwg
zumen draft plan.json --into template.jww -o A-101.jww --sheet A-101
```

```python
zumen.draft_into(spec, "template.dwg").save("A-101.dwg")
```

編集の操作を通すため、既存の図形・ハンドル・拡張データ・未対応の内容・形式・版を保ちます。DXF・DWG では同名のレイアウトに描き、なければ追加します。明示した `paper` は既存の用紙サイズも更新し、`fields` は表題欄の属性を埋めます。

JWW・SXF は既存用紙に 1 枚を描き、ビューを縮尺付きレイヤグループ・部分図にします。空き領域や定義の再利用などの形式固有の条件は補足資料と[決定 0027](decisions/0027-draft-into-jww-and-sxf.md)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [drafting.en.md](drafting.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="drafting-sheets-from-a-drawing-spec"></a>

[drafting-sheets-from-a-drawing-spec](drafting.en.md#drafting-sheets-from-a-drawing-spec)

<a id="coordinates-and-sizes"></a>

[coordinates-and-sizes](drafting.en.md#coordinates-and-sizes)

<a id="the-document"></a>

[the-document](drafting.en.md#the-document)

<a id="items"></a>

[items](drafting.en.md#items)

<a id="mistakes"></a>

[mistakes](drafting.en.md#mistakes)

<a id="what-each-format-keeps"></a>

[what-each-format-keeps](drafting.en.md#what-each-format-keeps)

<a id="drawing-into-a-template"></a>

[drawing-into-a-template](drafting.en.md#drawing-into-a-template)

<a id="jww-and-sxf-templates"></a>

[jww-and-sxf-templates](drafting.en.md#jww-and-sxf-templates)

</details>
