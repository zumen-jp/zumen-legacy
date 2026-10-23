# 0006: AutoCAD の図面モデルを経由して書き出す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

表示用 Scene は曲線・部品・寸法を展開しており、そのまま書くと編集しにくい図面になる。

## 選択肢

Scene から書く、形式の組み合わせごとに変換する、共通の図面モデルを介す方法を比較した。

## 決定

DxfDocument を中間モデルにし、最初は AutoCAD 2013・UTF-8 の DXF を出力する。保持できない内容は notices で返す。

## 影響

形式ごとに中間モデルとの変換を用意できる。モデルに保存しない内容は変換では保持できず、バイト列の完全保存とは区別する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0006-write-through-the-autocad-model.en.md](0006-write-through-the-autocad-model.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0006-write-drawings-through-the-autocad-drawing-model-starting-with-autocad-2013-dxf"></a>

[0006-write-drawings-through-the-autocad-drawing-model-starting-with-autocad-2013-dxf](0006-write-through-the-autocad-model.en.md#0006-write-drawings-through-the-autocad-drawing-model-starting-with-autocad-2013-dxf)

<a id="context"></a>

[context](0006-write-through-the-autocad-model.en.md#context)

<a id="options"></a>

[options](0006-write-through-the-autocad-model.en.md#options)

<a id="decision"></a>

[decision](0006-write-through-the-autocad-model.en.md#decision)

<a id="consequences"></a>

[consequences](0006-write-through-the-autocad-model.en.md#consequences)

<a id="references"></a>

[references](0006-write-through-the-autocad-model.en.md#references)

</details>
