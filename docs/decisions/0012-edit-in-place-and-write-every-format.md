# 0012: 操作対象以外を保って編集し、対応する CAD 形式へ書き出す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

変換で書き直すだけでは、未対応の内容・拡張データ・元の版を失う。

## 選択肢

共通モデルで全面再出力する、編集できる部分だけ元データを書き換える方法を比較した。

## 決定

add・update・delete・レイヤ操作などを形式共通の操作にし、元の形式・版・文字コードと名指ししない内容を保つ。全操作を検証してから書き込む。

## 影響

操作の失敗は番号・code・説明で返し、部分的に書かない。変換・新規作成と既存図面の編集を区別する。PDF は後の方針で読み取り専用とする。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0012-edit-in-place-and-write-every-format.en.md](0012-edit-in-place-and-write-every-format.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0012-edit-drawings-in-place-keeping-everything-an-operation-does-not-name-and-write-every-format-zumen-reads"></a>

[0012-edit-drawings-in-place-keeping-everything-an-operation-does-not-name-and-write-every-format-zumen-reads](0012-edit-in-place-and-write-every-format.en.md#0012-edit-drawings-in-place-keeping-everything-an-operation-does-not-name-and-write-every-format-zumen-reads)

<a id="context"></a>

[context](0012-edit-in-place-and-write-every-format.en.md#context)

<a id="options"></a>

[options](0012-edit-in-place-and-write-every-format.en.md#options)

<a id="decision"></a>

[decision](0012-edit-in-place-and-write-every-format.en.md#decision)

<a id="consequences"></a>

[consequences](0012-edit-in-place-and-write-every-format.en.md#consequences)

<a id="references"></a>

[references](0012-edit-in-place-and-write-every-format.en.md#references)

</details>
