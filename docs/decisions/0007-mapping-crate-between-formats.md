# 0007: 形式間の変換を専用クレートに置く

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

SXF・JWW を AutoCAD モデルへ写す処理は、複数形式の知識を必要とする。

## 選択肢

zumen 本体、各形式クレート、専用の zumen-convert を比較した。

## 決定

zumen-convert に sxf_to_dxf・jww_to_dxf を置き、zumen から mapping として公開する。描画に使う色・線種・文字配置などの規則を共用する。

## 影響

形式クレートを独立に保てる。変換は Rust で提供し、ブラウザの TypeScript には別実装を作らない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0007-mapping-crate-between-formats.en.md](0007-mapping-crate-between-formats.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0007-map-sxf-and-jww-into-the-autocad-drawing-model-in-a-crate-of-its-own-zumen-convert"></a>

[0007-map-sxf-and-jww-into-the-autocad-drawing-model-in-a-crate-of-its-own-zumen-convert](0007-mapping-crate-between-formats.en.md#0007-map-sxf-and-jww-into-the-autocad-drawing-model-in-a-crate-of-its-own-zumen-convert)

<a id="context"></a>

[context](0007-mapping-crate-between-formats.en.md#context)

<a id="options"></a>

[options](0007-mapping-crate-between-formats.en.md#options)

<a id="decision"></a>

[decision](0007-mapping-crate-between-formats.en.md#decision)

<a id="consequences"></a>

[consequences](0007-mapping-crate-between-formats.en.md#consequences)

<a id="references"></a>

[references](0007-mapping-crate-between-formats.en.md#references)

</details>
