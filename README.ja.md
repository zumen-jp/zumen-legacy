# zumen

[English](README.md) | 日本語

建築・土木の 2D 図面をブラウザで読み、表示・計測するライブラリ群。図面はサーバーへ送らず、ブラウザの中で読む。
国交省の電子納品で使う **SXF（SFC・P21）** を公開仕様から、Jw_cad の **JWW** を形式の解説から実装した。
2026-10 の調査では、SXF をブラウザで読める JS ライブラリはなく、JWW もブラウザ向けの npm と React の部品がなかった。
AutoCAD などの **DXF**（Shift_JIS の日本語、バイナリを含む）も TypeScript で読む。
**DWG**（AutoCAD 2000〜2018）は、GPL の部品を使わない自前の読み取りで、Rust から同じ AutoCAD の図形モデルに読む
（いまは `zumen` コマンド・Python・WebAssembly から使える）。
構造設計データの国内標準 **ST-Bridge**（`.stb`）も 2.0 系・2.1 系を読み、部材を 3D で表示する。
ブラウザで ST-Bridge を読める JS ライブラリも見つからなかった。
読み取りを Rust に移し、Python（サーバーやデータ処理）と WebAssembly（ブラウザ）からも同じ結果で使えるようにしている。

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
| `@zumenjs/stb` | `packages/stb` | ST-Bridge の読み取り `readStb`・`isStb`（3D の構造モデル）、three.js の表示 `StructureView`（`/three`）、React の `StbViewer`・`useStbFile`（`/react`） |

依存の向きは `@zumenjs/react` → `@zumenjs/core`、`@zumenjs/sxf`・`@zumenjs/jww`・`@zumenjs/dxf` → `@zumenjs/core` だけ
（DWG は Rust だけで読み、`zumen-dwg` が `zumen-dxf` のモデルと変換を使う）。
`@zumenjs/stb` は 3D の別の系統で、ほかのパッケージに依存しない。
`@zumenjs/core` は形式を知らず、使う形式の読み取りを `readDrawing` に渡す。形式は拡張子ではなく中身で判定する。

## できること

- 図面全体の表示、拡大・縮小（ホイール・ピンチ・ボタン・キー）、移動（ドラッグ・矢印キー）。
- レイヤごとの表示・非表示。図面の表示指定を初期状態にする。JWW はレイヤグループごとにまとめて切り替えられる。
- 図形を選んで属性を確認する。種類、レイヤ、色・線種・線幅、座標、寸法値、部分図の縮尺など。
- DXF の R12〜2018（テキスト・バイナリ）。線分・円弧・ふくらみと幅のあるポリライン・スプライン・文字・書式つきの複数行文字・
  属性と配列のあるブロック・寸法・ハッチング・引出線・ソリッド・ワイプアウトを、レイヤ・線種・線の太さ・ACI とトゥルーカラー・透過度・文字スタイルとともに描く。
- DWG の AutoCAD 2000〜2018。Rust で読み、DXF と同じく描く（[DWG の扱い（英語）](docs/formats/dwg.md)）。
- DXF のモデル空間とレイアウト（紙空間）の切り替え。ビューポートの中はその縮尺で描き、計測すると実寸を返す。
- 2 点間の距離の計測。端点・頂点、なければ線上の点に吸着し、部分図・部品（JWW はレイヤグループ、DXF は単位とビューポート）の縮尺から**実寸**も出す。
- 黒背景と白背景の切り替え。初期値は図面の想定（SXF は黒、JWW はヘッダの背景色）。
  背景と見分けにくい色（白背景の白、黒背景の黒）は反対の色で描く。
- 線の太さどおりの表示と、細線表示の切り替え。
- 図面の情報（表題欄、作成ソフト、用紙）と、省略・代替した内容の件数（読み取りの注意）。
- 構造モデル（ST-Bridge 2.0・2.1）の 3D 表示。部材を断面の形で押し出し、階と通り芯を示す。階で絞ると伏図、
  通り芯で絞ると軸組図になる。部材を選ぶと、断面・材料・階・元の XML の属性を確かめられる。

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

構造モデル（ST-Bridge）は、`useStbFile` で読み、`StbViewer` で 3D に表示する。`readStb` だけなら Node.js でも動く。

```tsx
import { StbViewer, useStbFile } from "@zumenjs/stb/react";

export function StructureView({ file }: { file: File | null }) {
  const { model, error } = useStbFile(file);
  if (error) return <p role="alert">{error.message}</p>;
  return model ? <StbViewer model={model} style={{ height: 640 }} /> : null;
}
```

`StructureModel` は全体座標系の mm・Z 軸上向きで、部材ごとに立体・属性の一覧・元の XML の属性を持つ。
詳しくは [packages/stb](packages/stb/README.md)（英語）と [ST-Bridge の扱い](docs/formats/st-bridge.md)。

## Rust と Python

読み取りを Rust（`crates/`）に移し、Python の結合部（`bindings/python`）と WebAssembly（`bindings/wasm`）を作っている。
Rust の読み取りは TypeScript と同じ `Scene` を返す。`tests/fixtures` の図面で両方の結果を比べ、手元の図面は
`pnpm run parity` で比べる。JWW・SXF・DXF は移植済み。DWG は Rust だけで読み、AutoCAD が同じ図面を書き出した
DXF と比べて確かめる。ブラウザのパッケージは、形式ごとに結果が一致したものから WebAssembly の読み取りに切り替える。

```python
import zumen

scene = zumen.read("plan.jww")  # 形式は中身で判定する
png = scene.render_png(dpi=150)  # サムネイルや、画像を読む AI への添付
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))  # 用紙の mm で範囲を指定して拡大
json_text = scene.to_json()  # @zumenjs/core の sceneFromJson で Scene に戻せる

texts = scene.texts()  # 文字列・位置・高さ・向き・レイヤ（用紙の mm）
scene.title_block()  # 表題欄の図面番号・改訂・図面名・工事名・縮尺・作成年月日
door = scene.entity("#120")  # 1 つの図形の種類・レイヤ・外接矩形・文字・属性
table = scene.to_arrow()  # すべての図形を pyarrow.Table に（pip install "zumen[arrow]"）
image = scene.render(size=(2000, 2000))  # PNG と、画像の px と用紙の mm の変換
for entity in image.entities_at(640, 480):  # その画素に描いた図形（外接矩形の小さい順）
    print(entity["id"], entity["box"], entity["text"])
```

AI のアプリは、Python のパッケージの MCP サーバー（`pip install "zumen[mcp]"` の後に `zumen-mcp --root <フォルダ>`）で
図面を読める。開く、範囲を描く、文字と図形を探す、実寸で測る、の道具がある。
[bindings/python](bindings/python/README.md#mcp-server) を参照。

```bash
cargo run --release -p zumen-cli -- render plan.jww -o plan.png --dpi 150 --map plan.json
cargo run --release -p zumen-cli -- texts plan.jww --region 100,50,200,120  # 1 行に 1 つの JSON
cargo run --release -p zumen-cli -- title plan.jww  # 表題欄の項目
```

| クレート | 中身 |
| --- | --- |
| `zumen-core` | 表示用モデル（`@zumenjs/core` と同じ JSON）、幾何、文字の配置、図面の文字と図形と表題欄の項目の取り出し |
| `zumen-sxf` | `parse_sfc`・`parse_p21`（SXF のフィーチャのモデル）、`sxf_to_scene`、読み取りの `SXF` |
| `zumen-jww` | `read_jww`（JWW のままのモデル）、`jww_to_scene`、読み取りの `JWW` |
| `zumen-dxf` | `parse_dxf`（DWG と共有する AutoCAD の図形モデル）、`dxf_to_scene`、読み取りの `DXF` |
| `zumen-dwg` | `read_dwg`（DWG 2000〜2018 を `zumen-dxf` の AutoCAD の図形モデルにする）、読み取りの `DWG` |
| `zumen` | すべての読み取りをまとめた `read`・`detect` |
| `zumen-render` | ブラウザの描画と同じ規則で PNG に描き、描いた図形の画像の上の位置を返す。日本語の文字は同梱の BIZ UDゴシックで描く |
| `zumen-cli` | `zumen` コマンド（`detect`、`scene` で JSON、`texts`・`entities` で 1 行 1 つの JSON、`title` で表題欄、`render` で PNG） |

Python では、読み取りと描画の間 GIL を外す。

## 文書

- [構成と拡張の口（英語）](docs/architecture.md)
- [SXF（SFC・P21）の扱い](docs/formats/sxf.md)
- [JWW の扱い](docs/formats/jww.md)
- [DXF の扱い（英語）](docs/formats/dxf.md)
- [DWG の扱い（英語）](docs/formats/dwg.md)
- [ST-Bridge の扱い](docs/formats/st-bridge.md)
- [検証の記録（英語）](docs/verification.md)
- [設計の判断の記録（英語）](docs/decisions/README.md)
- [開発への参加（英語）](CONTRIBUTING.md)

## 開発

```bash
pnpm install --frozen-lockfile
pnpm run check
```

`check` は Biome、型検査、単体テスト、依存監査、各パッケージ（core、sxf・jww・dxf、react、stb の順）と試用画面のビルドに加え、
Rust（rustfmt・Clippy・テスト・wasm32 のビルド）と Python の結合部（Ruff・maturin・pytest）を検査する。
Rust は `rust-toolchain.toml`、Python は uv と `bindings/python/.python-version` に固定する。
TS の読み取りを変えたら `pnpm run golden` で `tests/fixtures` の正解を作り直し、Rust の読み取りも同じ結果に直す。
開発中は、パッケージ同士が `package.json` の exports の `@zumenjs/source` 条件で `src` を読むため、ビルドしなくても変更がすぐ反映される。
Node.js は `.node-version`、pnpm は `package.json` の `packageManager` に固定する。

試用画面は `pnpm dev` で開く（ポートは [config/local-ports.json](config/local-ports.json) の `playground`、既定 24100）。
Git に入れない図面は、置いたフォルダーを `ZUMEN_SAMPLES` に指定し、`http://127.0.0.1:24100/?sample=<ファイル名>` で開く。
ファイルの選択・ドロップでも開ける。ST-Bridge は中身で見分けて 3D の表示に切り替える（three.js はそのときだけ読み込む）。

## ライセンス

公開の前に決める。参照した第三者のソフトウェア・資料は [packages/jww/THIRD_PARTY_NOTICES.md](packages/jww/THIRD_PARTY_NOTICES.md)、
[packages/dxf/THIRD_PARTY_NOTICES.md](packages/dxf/THIRD_PARTY_NOTICES.md)、
[packages/stb/THIRD_PARTY_NOTICES.md](packages/stb/THIRD_PARTY_NOTICES.md) に記録した。
`zumen-render` に同梱するフォントは BIZ UDゴシック（SIL Open Font License 1.1、[crates/zumen-render/fonts/OFL.txt](crates/zumen-render/fonts/OFL.txt)）。
