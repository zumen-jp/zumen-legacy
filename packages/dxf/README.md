# @zumenjs/dxf

テキスト・バイナリの DXF（R12〜2018）を読み取ります。Shift_JIS の日本語とモデル空間・レイアウトを扱います。AutoCAD の図面モデルは Rust の DWG と共通です。

この TypeScript パッケージは、テストの正解を作るための参照実装としてリポジトリに残しています。npm には公開しません。アプリケーションからは、全形式で同じ Rust の読み取りを使う [`@zumenjs/reader`](../reader) を利用してください。

形式の細則は [DXF の扱い](../../docs/formats/dxf.md)、全体の使い方は [README](../../README.md) を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjsdxf"></a>

[zumenjsdxf](README.en.md#zumenjsdxf)

</details>
