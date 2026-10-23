# 0014: Windows ネイティブのビルドと CAD 連携を確認する

| 項目 | 内容 |
| --- | --- |
| 日付 | 2026-10-20 |
| 状態 | 採用 |

このページは既存判断の日本語の要約です。詳細な条件と当時の議論は英語の補足資料に保持しています。

## 背景

日本の実務では Windows と Jw_cad の利用が多く、macOS・Linux の結果だけでは足りない。

## 選択肢

WSL だけで確認する、Windows ネイティブでも検証する方法を比較した。

## 決定

PowerShell・MSVC・Windows の Python で CI と配布物を確認し、Jw_cad などでの読み直しは別の手動検証として記録する。

## 影響

日本語・空白を含むパスや wheel を検査する。フォント・実際の CAD の表示など、自動テストでは確認できない範囲を区別する。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [0014-verify-on-native-windows.en.md](0014-verify-on-native-windows.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="0014-verify-native-windows-builds-and-cad-interoperability"></a>

[0014-verify-native-windows-builds-and-cad-interoperability](0014-verify-on-native-windows.en.md#0014-verify-native-windows-builds-and-cad-interoperability)

<a id="context"></a>

[context](0014-verify-on-native-windows.en.md#context)

<a id="options"></a>

[options](0014-verify-on-native-windows.en.md#options)

<a id="decision"></a>

[decision](0014-verify-on-native-windows.en.md#decision)

<a id="consequences"></a>

[consequences](0014-verify-on-native-windows.en.md#consequences)

<a id="references"></a>

[references](0014-verify-on-native-windows.en.md#references)

</details>
