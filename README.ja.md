# zumen

[English](README.md) | 日本語

建築・土木の 2D 図面をブラウザで読み、表示・計測するライブラリ群。図面はサーバーへ送らず、ブラウザの中で読む。
国交省の電子納品で使う **SXF（SFC・P21）** を公開仕様から、Jw_cad の **JWW** を形式の解説から実装した。
2026-10 の調査では、SXF をブラウザで読める JS ライブラリはなく、JWW もブラウザ向けの npm と React の部品がなかった。
AutoCAD などの **DXF**（Shift_JIS の日本語、バイナリを含む）も TypeScript で読む。
次の **DWG** は同じ AutoCAD の図形モデルを使い、GPL の部品を使わず MIT のまま読むことを目指す。

> 状態: 公開前（0.x）。npm にはまだ公開していない。

## パッケージ

pnpm の workspace で、形式ごとの読み取りと、形式に依存しない図面エンジン、React の部品に分ける。
npm には未公開（0.x の公開準備中）。

| パッケージ | フォルダー | 中身 |
| --- | --- | --- |
| `@zumenjs/core` | `packages/core` | 表示用モデル（`Scene`）、Canvas の描画、`DrawingViewer`（拡大・移動・選択・計測・重ね描き）、距離の計測、`readDrawing`。幾何は `/geometry`、文字の配置は `/text` |
| `@zumenjs/sxf` | `packages/sxf` | `parseSfc`・`parseP21`（フィーチャのモデル）、`sxfToScene`、読み取りの `sxf` |
| `@zumenjs/jww` | `packages/jww` | `readJww`（JWW のままのモデル）、`jwwToScene`、読み取りの `jww` |
| `@zumenjs/dxf` | `packages/dxf` | `parseDxf`（DWG と共有する AutoCAD の図形モデル）、`dxfToScene`、読み取りの `dxf` |
| `@zumenjs/react` | `packages/react` | 完成品の画面 `CadViewer`、フック `useDrawingViewer`・`useCadFile` |

依存の向きは `@zumenjs/react` → `@zumenjs/core`、`@zumenjs/sxf`・`@zumenjs/jww`・`@zumenjs/dxf` → `@zumenjs/core` だけ
（例外は、次の DWG の読み取りが `@zumenjs/dxf` のモデルと変換を使うこと）。
`@zumenjs/core` は形式を知らず、使う形式の読み取りを `readDrawing` に渡す。形式は拡張子ではなく中身で判定する。

## できること

- 図面全体の表示、拡大・縮小（ホイール・ピンチ・ボタン・キー）、移動（ドラッグ・矢印キー）。
- レイヤごとの表示・非表示。図面の表示指定を初期状態にする。JWW はレイヤグループごとにまとめて切り替えられる。
- 図形を選んで属性を確認する。種類、レイヤ、色・線種・線幅、座標、寸法値、部分図の縮尺など。
- DXF の R12〜2018（テキスト・バイナリ）。線分・円弧・ふくらみと幅のあるポリライン・スプライン・文字・書式つきの複数行文字・
  属性と配列のあるブロック・寸法・ハッチング・引出線・ソリッド・ワイプアウトを、レイヤ・線種・線の太さ・ACI とトゥルーカラー・透過度・文字スタイルとともに描く。
- DXF のモデル空間とレイアウト（紙空間）の切り替え。ビューポートの中はその縮尺で描き、計測すると実寸を返す。
- 2 点間の距離の計測。端点・頂点、なければ線上の点に吸着し、部分図・部品（JWW はレイヤグループ、DXF は単位とビューポート）の縮尺から**実寸**も出す。
- 黒背景と白背景の切り替え。初期値は図面の想定（SXF は黒、JWW はヘッダの背景色）。
  背景と見分けにくい色（白背景の白、黒背景の黒）は反対の色で描く。
- 線の太さどおりの表示と、細線表示の切り替え。
- 図面の情報（表題欄、作成ソフト、用紙）と、省略・代替した内容の件数（読み取りの注意）。

## 使い方

React では、読む形式を `useCadFile` に渡し、`CadViewer` で表示する。

```tsx
"use client";

import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { CadViewer, useCadFile } from "@zumenjs/react";
import { sxf } from "@zumenjs/sxf";
import { useState } from "react";

const READERS = [sxf, jww, dxf];

export function DrawingView({ file }: { file: File | null }) {
  // DXF のモデル空間・レイアウト。undefined は図面の既定の表示。
  // ファイルごとに DrawingView を作り直す（key を付けるなど）と、既定の表示から始まる
  const [view, setView] = useState<string>();
  const { scene, error, loading } = useCadFile(file, READERS, { view });
  if (loading) return <p>読み込み中…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <CadViewer scene={scene} onViewChange={setView} style={{ height: 640 }} /> : null;
}
```

操作盤を自分で作る場合は `useDrawingViewer` を使う。`ref` を大きさを決めた要素に渡し、
`state`（倍率・選択・計測の距離など）を読んで、`viewer` のメソッドで操作する。

```tsx
const { ref, viewer, state } = useDrawingViewer(scene, { onSelect: (entity) => console.log(entity) });
return (
  <>
    <button type="button" onClick={() => viewer?.fit()}>全体</button>
    <div ref={ref} style={{ height: 480 }} />
    <p>{state.selected?.type}</p>
  </>
);
```

React を使わない場合は、`DrawingViewer` に要素と図面を渡す。

```ts
import { DrawingViewer, readDrawing } from "@zumenjs/core";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";

const scene = readDrawing(new Uint8Array(await file.arrayBuffer()), [sxf, jww]);
const viewer = new DrawingViewer(element, scene, { label: file.name });
viewer.subscribe(() => console.log(viewer.getState().distance));
// 図面の上に重ねて描く（書き込みなど）。view で用紙の mm と画面の px を変換する
const remove = viewer.addOverlay((ctx, view) => ctx.fillRect(view.offsetX, view.offsetY - 8, 8, 8));
```

表示せずに解析だけを使う場合は、形式のままのモデルを読む（Node.js でも動く）。

```ts
import { parseSfc, sxfToScene } from "@zumenjs/sxf";

const drawing = parseSfc(bytes); // フィーチャ、レイヤ、部分図、表題欄
const scene = sxfToScene(drawing); // 必要なら表示用モデルにする
```

`Scene` は用紙の mm・Y 軸上向きの座標で、部分図・作図部品の配置を解決済みの図形を持つ。
要素ごとに属性の一覧と、部分図の中の要素なら元の座標への変換（実寸の計測用、`toReal`）を持つ。

## 文書

- [構成と拡張の口（英語）](docs/architecture.md)
- [SXF（SFC・P21）の扱い](docs/formats/sxf.md)
- [JWW の扱い](docs/formats/jww.md)
- [DXF の扱い（英語）](docs/formats/dxf.md)
- [検証の記録（英語）](docs/verification.md)
- [開発への参加（英語）](CONTRIBUTING.md)

## 開発

```bash
pnpm install --frozen-lockfile
pnpm run check
```

`check` は Biome、型検査、単体テスト、依存監査、各パッケージ（core、sxf・jww・dxf、react の順）と試用画面のビルドを行う。
開発中は、パッケージ同士が `package.json` の exports の `@zumenjs/source` 条件で `src` を読むため、ビルドしなくても変更がすぐ反映される。
Node.js は `.node-version`、pnpm は `package.json` の `packageManager` に固定する。

試用画面は `pnpm dev` で開く（ポートは [config/local-ports.json](config/local-ports.json) の `playground`、既定 24055）。
Git に入れない図面は、置いたフォルダーを `ZUMEN_SAMPLES` に指定し、`http://127.0.0.1:24055/?sample=<ファイル名>` で開く。
ファイルの選択・ドロップでも開ける。

## ライセンス

公開の前に決める。参照した第三者のソフトウェアは [packages/jww/THIRD_PARTY_NOTICES.md](packages/jww/THIRD_PARTY_NOTICES.md) に記録した。
