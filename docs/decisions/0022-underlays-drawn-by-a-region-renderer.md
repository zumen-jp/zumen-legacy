# 0022: 利用側の範囲描画処理で下敷きを描き、同内容の図形は描かない

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-23 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

画像・PDF ページを図形の下に表示し、大きな画像も表示範囲に応じて描きたい。

## 選択肢

画像全体を常に描く、形式の読み取りに描画を含める、共通の下敷きと描画依頼に分ける方法を比較した。

## 決定

Scene は下敷きの種類・配置・範囲・参照を持ち、利用側の UnderlayRenderer に必要範囲を依頼する。既に下敷きで見える図形は drawn: false とする。

## 影響

外部ファイルの取得と復号は利用側が管理する。図形の選択・吸着・解析と、見た目の描画を両立する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0022-underlays-drawn-by-a-region-renderer.en.md](0022-underlays-drawn-by-a-region-renderer.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0022-draw-raster-underlays-through-a-region-renderer-that-the-application-passes-in-and-keep-the-entities-they-show-undrawn"></a>

[0022-draw-raster-underlays-through-a-region-renderer-that-the-application-passes-in-and-keep-the-entities-they-show-undrawn](0022-underlays-drawn-by-a-region-renderer.en.md#0022-draw-raster-underlays-through-a-region-renderer-that-the-application-passes-in-and-keep-the-entities-they-show-undrawn)

<a id="context"></a>

[context](0022-underlays-drawn-by-a-region-renderer.en.md#context)

<a id="options"></a>

[options](0022-underlays-drawn-by-a-region-renderer.en.md#options)

<a id="decision"></a>

[decision](0022-underlays-drawn-by-a-region-renderer.en.md#decision)

<a id="consequences"></a>

[consequences](0022-underlays-drawn-by-a-region-renderer.en.md#consequences)

<a id="references"></a>

[references](0022-underlays-drawn-by-a-region-renderer.en.md#references)

</details>
