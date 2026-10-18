# 第三者のソフトウェア・資料

## ezdxf

DXF の読み取りと表示は Autodesk の DXF Reference に沿って書き、次の点は ezdxf の実装を参照した（`src/tags.ts`・`src/parse.ts`・`src/scene.ts`）。
コードは転記していない。

- `$DWGCODEPAGE` の名前と文字コードの対応、MIF（`\M+nXXXX`）の文字コードの番号、バイナリ DXF の版の見つけ方
- R12 のビューポートの拡張データ（MVIEW）の並び、ビューポートのモデルから紙空間への変換
- HATCH の時計回りの弧・楕円の辺の角度、楕円の辺の角度から媒介変数への変換、IMAGE・WIPEOUT の境界の座標
- 寸法の表示のブロックを寸法の OCS に置くこと、MTEXT の真色（`\c`）の値の並び
- MULTILEADER の区切り（CONTEXT_DATA・LEADER・LEADER_LINE）ごとの値の意味、色の生の値（0xC0〜0xC3・0xC8）、
  MLEADERSTYLE より図形の値を使うこと、矢印を描かない短い線分、矢印のブロックの向きと線を矢印の先から始める矢印、
  下線・上線の接続、ブロックの内容の配置点が WCS であること（`entities/mleader.py`・`render/mleader.py`・`render/arrows.py`・`colors.py`）
- MLINE の頂点ごとの要素のパラメータ（41・42）の並び、塗り・端の線・弧・留め継ぎの描き方、MLINE の真色を使わないこと
  （`entities/mline.py`・`render/mline.py`）

- 参照した版: https://github.com/mozman/ezdxf （PyPI の ezdxf 1.4.5、2026-10-18）
- ライセンス: MIT License

```text
MIT License

Copyright (c) 2020 Manfred Moitzi

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
