# 0013: 許可が必要な形式と寛容でない依存を除き、ST-Bridge を外す

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

ST-Bridge の公開条件は権利者への確認が必要で、自由に導入できるライブラリの方針に合わない。

## 選択肢

確認前に含める、機能を分離する、zumen から外す方法を比較した。

## 決定

ST-Bridge を zumen の配布・公開 API から外す。許可が必要な形式や MPL・LGPL・GPL などの依存の追加は、先に設計判断を要する。

## 影響

決定 0001 を置き換える。将来の再導入には buildingSMART Japan の同意など条件の確認が必要で、日本向けの方針だけを理由に復活させない。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0013-st-bridge-out-of-zumen.en.md](0013-st-bridge-out-of-zumen.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0013-keep-zumen-free-of-formats-that-need-permission-and-of-non-permissive-dependencies-and-move-st-bridge-out"></a>

[0013-keep-zumen-free-of-formats-that-need-permission-and-of-non-permissive-dependencies-and-move-st-bridge-out](0013-st-bridge-out-of-zumen.en.md#0013-keep-zumen-free-of-formats-that-need-permission-and-of-non-permissive-dependencies-and-move-st-bridge-out)

<a id="context"></a>

[context](0013-st-bridge-out-of-zumen.en.md#context)

<a id="options"></a>

[options](0013-st-bridge-out-of-zumen.en.md#options)

<a id="decision"></a>

[decision](0013-st-bridge-out-of-zumen.en.md#decision)

<a id="consequences"></a>

[consequences](0013-st-bridge-out-of-zumen.en.md#consequences)

<a id="references"></a>

[references](0013-st-bridge-out-of-zumen.en.md#references)

</details>
