# 0020: PDF 図面の線と文字を Rust で読み、ページ描画は PDF エンジンに任せる

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

PDF の見た目を保ちながら選択・吸着・計測・解析に使う図形が必要。

## 選択肢

ブラウザだけで抽出、自前の PDF 描画、Rust の共通読み取りと専用描画の分担を比較した。

## 決定

hayro-interpret の呼び出しから線・文字・画像を Scene に読み、ページは下敷きとして描く。図形は drawn: false にして二重描画を避ける。

## 影響

ブラウザ・サーバーで図形と id を共通にする。フォント・CMap・依存の版とライセンスを管理し、PDF は CAD 形式への編集・変換の入力にしない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0020-pdf-drawings-in-rust.en.md](0020-pdf-drawings-in-rust.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0020-read-the-paths-and-texts-of-pdf-drawings-in-rust-only-with-hayro-interpret-and-leave-the-pixels-to-a-pdf-engine"></a>

[0020-read-the-paths-and-texts-of-pdf-drawings-in-rust-only-with-hayro-interpret-and-leave-the-pixels-to-a-pdf-engine](0020-pdf-drawings-in-rust.en.md#0020-read-the-paths-and-texts-of-pdf-drawings-in-rust-only-with-hayro-interpret-and-leave-the-pixels-to-a-pdf-engine)

<a id="context"></a>

[context](0020-pdf-drawings-in-rust.en.md#context)

<a id="options"></a>

[options](0020-pdf-drawings-in-rust.en.md#options)

<a id="decision"></a>

[decision](0020-pdf-drawings-in-rust.en.md#decision)

<a id="consequences"></a>

[consequences](0020-pdf-drawings-in-rust.en.md#consequences)

<a id="references"></a>

[references](0020-pdf-drawings-in-rust.en.md#references)

</details>
