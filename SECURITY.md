# セキュリティ方針

## サポートする版

zumen は 1.0 より前の段階です。修正は最新リリースを対象にします。

## 脆弱性の報告

リポジトリの Security タブにある「Report a vulnerability」から、非公開で報告してください。公開 Issue には書かないでください。機密図面は添付せず、問題を再現する最小のファイルを用意してください。

## 報告の対象

- 読み取りが停止しない、画面が落ちる、メモリや CPU を過剰に消費する入力。
- ライブラリによる意図しないネットワークアクセス。
- 図面内の文字を介したスクリプトの実行。

読み取りはメモリ上のバイト列を対象とし、配置・部品の入れ子に上限を設け、ネットワーク要求を行いません。図面の文字は Canvas または文字列として扱い、HTML として挿入しません。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [SECURITY.en.md](SECURITY.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="security-policy"></a>

[security-policy](SECURITY.en.md#security-policy)

<a id="supported-versions"></a>

[supported-versions](SECURITY.en.md#supported-versions)

<a id="reporting-a-vulnerability"></a>

[reporting-a-vulnerability](SECURITY.en.md#reporting-a-vulnerability)

<a id="what-counts"></a>

[what-counts](SECURITY.en.md#what-counts)

</details>
