# 0005: ブラウザ向けは組み込みエンジンとフックに分ける

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 提案 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

完成した UI を配る形では、利用側の状態管理・見た目・操作に合わせにくい。

## 選択肢

既存クラスへのオプション追加、TypeScript の読み取りを残す構成、Rust と共通の読み取りを使うエンジン構成を比較した。

## 決定

core に共通モデル、viewer に描画と操作、reader に WebAssembly、react にフックを置く。画面と CSS はサンプルにし、配色は CSS カスタムプロパティから受け取る。

## 影響

利用側が UI を作れる。ブラウザは初回読み取り前に WebAssembly を待つ。TypeScript の形式読み取りはテストの参照として残す。ST-Bridge の扱いは後の決定 0013 を優先する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0005-browser-packages-for-embedding.en.md](0005-browser-packages-for-embedding.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0005-make-the-browser-packages-an-engine-to-embed-with-reading-in-rust-and-the-ui-as-a-sample"></a>

[0005-make-the-browser-packages-an-engine-to-embed-with-reading-in-rust-and-the-ui-as-a-sample](0005-browser-packages-for-embedding.en.md#0005-make-the-browser-packages-an-engine-to-embed-with-reading-in-rust-and-the-ui-as-a-sample)

<a id="context"></a>

[context](0005-browser-packages-for-embedding.en.md#context)

<a id="options"></a>

[options](0005-browser-packages-for-embedding.en.md#options)

<a id="decision"></a>

[decision](0005-browser-packages-for-embedding.en.md#decision)

<a id="consequences"></a>

[consequences](0005-browser-packages-for-embedding.en.md#consequences)

<a id="references"></a>

[references](0005-browser-packages-for-embedding.en.md#references)

</details>
