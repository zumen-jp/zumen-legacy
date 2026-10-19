# 第三者のソフトウェア・資料

## ST-Bridge の仕様

ST-Bridge の読み取りは、一般社団法人 buildingSMART Japan 構造設計小委員会の ST-Bridge XML ファイル仕様書
（ver.2.1.1・ver.2.0.2）と XML Schema（ST-Bridge_v211.xsd・ST-Bridge_v202.xsd）に従って実装した。
ST-Bridge の権利は buildingSMART Japan に帰属する。仕様書・XML Schema は配布しない
（https://www.building-smart.or.jp/meeting/buildall/structural-design/ から入手する）。
このライブラリは buildingSMART Japan の公認・認証を受けたものではない。

## three.js

`./three` と `./react` の描画に three.js（`three`、peer dependency）を使う。`OrbitControls` と `CSS2DRenderer` は
three.js の `examples/jsm` に含まれるもの。コードは転記していない。

- 使う版: 0.186.1（https://github.com/mrdoob/three.js）
- ライセンス: MIT License

```text
The MIT License

Copyright © 2010-2026 three.js authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```
