# zumen

[English](README.md) | 日本語

建築・土木の 2D 図面をブラウザで読み、表示・計測するライブラリ群。図面はサーバーへ送らず、ブラウザの中で読む。
国交省の電子納品で使う **SXF（SFC・P21）** を公開仕様から、Jw_cad の **JWW** を形式の解説から実装した。
2026-10 の調査では、SXF をブラウザで読める JS ライブラリはなく、JWW もブラウザ向けの npm と React の部品がなかった。
AutoCAD などの **DXF**（Shift_JIS の日本語、バイナリを含む）も TypeScript で読む。
**DWG**（AutoCAD 2000〜2018）は、GPL の部品を使わない自前の読み取りで、Rust から同じ AutoCAD の図形モデルに読む
（ブラウザでは、その WebAssembly の build を包む `@zumenjs/dwg` で読む）。
読み取りを Rust に移し、Python（サーバーやデータ処理）と WebAssembly（ブラウザ）からも同じ結果で使えるようにしている。

> 状態: 公開前（0.x）。npm にはまだ公開していない。

## パッケージ

pnpm の workspace で、形式ごとの読み取りと、形式に依存しないモデル、利用側の画面に組み込む図面エンジン、React のフックに分ける。
npm には未公開（0.x の公開準備中）。

| パッケージ | フォルダー | 中身 |
| --- | --- | --- |
| `@zumenjs/core` | `packages/core` | 表示用モデル（`Scene`）と JSON、`readDrawing` と読み取りの口（`DrawingReader`）、距離の計測、読み取りの注意。DOM を使わない。幾何は `/geometry`、文字の配置は `/text` |
| `@zumenjs/viewer` | `packages/viewer` | 画面に組み込む 2D のエンジン。Canvas の描画、当たり判定と吸着、`DrawingViewer`（表示範囲・状態・操作のハンドラ・道具・見た目） |
| `@zumenjs/sxf` | `packages/sxf` | `parseSfc`・`parseP21`（フィーチャのモデル）、`sxfToScene`、読み取りの `sxf` |
| `@zumenjs/jww` | `packages/jww` | `readJww`（JWW のままのモデル）、`jwwToScene`、読み取りの `jww` |
| `@zumenjs/dxf` | `packages/dxf` | `parseDxf`（DWG と共有する AutoCAD の図形モデル）、`dxfToScene`、読み取りの `dxf` |
| `@zumenjs/dwg` | `packages/dwg` | 読み取りの `dwg`（Rust の DWG の読み取りを WebAssembly にしたもの。`initDwg` か `dwg.load` で読み込む）、`isDwg` |
| `@zumenjs/react` | `packages/react` | フックだけ。`useDrawingViewer`・`useViewerState`・`useViewerEvent`・`useDrawing` |

依存の向きは `@zumenjs/react` → `@zumenjs/viewer` → `@zumenjs/core`、`@zumenjs/sxf`・`@zumenjs/jww`・`@zumenjs/dxf`・
`@zumenjs/dwg` → `@zumenjs/core` だけ（DWG は Rust だけで読み、`zumen-dwg` が `zumen-dxf` のモデルと変換を使う。
`@zumenjs/dwg` はその WebAssembly の build を包む）。パッケージは画面の文言と CSS を持たず、画面は利用側が作る
（試用画面にサンプルがある）。
`@zumenjs/core` は形式を知らず、使う形式の読み取りを `readDrawing` に渡す。形式は拡張子ではなく中身で判定する。

## できること

- 図面全体の表示、拡大・縮小（ホイール・ピンチ・ボタン・キー）、移動（ドラッグ・矢印キー）。
- 利用側の画面への組み込み。表示範囲を指定・保存・復元し、変化をイベントで受ける（図面の上に置いた HTML を追従させる）。
  複数の図形の選択、検索結果などの名前付きの強調、利用側の道具（書き込み・ピン・範囲など）、操作のハンドラごとの停止
  （ホイールは Ctrl・⌘ を押したときだけ拡大、なども選べる）、CSS のカスタムプロパティによる配色。
- レイヤごとの表示・非表示。図面の表示指定を初期状態にする。JWW はレイヤグループごとにまとめて切り替えられる。
- 図形を選んで属性を確認する。種類、レイヤ、色・線種・線幅、座標、寸法値、部分図の縮尺など。
- DXF の R12〜2018（テキスト・バイナリ）。線分・円弧・ふくらみと幅のあるポリライン・スプライン・文字・書式つきの複数行文字・
  属性と配列のあるブロック・寸法・ハッチング・引出線・ソリッド・ワイプアウトを、レイヤ・線種・線の太さ・ACI とトゥルーカラー・透過度・文字スタイルとともに描く。
- DWG の AutoCAD 2000〜2018。Rust で読み（ブラウザでは WebAssembly）、DXF と同じく描く（[DWG の扱い（英語）](docs/formats/dwg.md)）。
- DXF・DWG のモデル空間とレイアウト（紙空間）の切り替え。ビューポートの中はその縮尺で描き、計測すると実寸を返す。
- 2 点間の距離の計測。端点・頂点、なければ線上の点に吸着し、部分図・部品（JWW はレイヤグループ、DXF は単位とビューポート）の縮尺から**実寸**も出す。
- 黒背景と白背景の切り替え。初期値は図面の想定（SXF は黒、JWW はヘッダの背景色）。
  背景と見分けにくい色（白背景の白、黒背景の黒）は反対の色で描く。
- 線の太さどおりの表示と、細線表示の切り替え。
- 図面の情報（表題欄、作成ソフト、用紙）と、省略・代替した内容の件数（読み取りの注意）。

## 使い方

React では、`useDrawing` で渡した読み取りを使ってファイルを読み、`useDrawingViewer` で画面の要素の上にエンジンを作り、
`useViewerState` で状態のうち部品が使う部分を読む。

```tsx
"use client";

import type { Scene } from "@zumenjs/core";
import { dwg } from "@zumenjs/dwg";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { useDrawing, useDrawingViewer, useViewerState } from "@zumenjs/react";
import { sxf } from "@zumenjs/sxf";

const READERS = [sxf, jww, dxf, dwg];

export function DrawingView({ file }: { file: File | null }) {
  // DWG の読み取りは、初めて DWG を開いたときに WebAssembly（brotli で 494 KB）を読み込む。
  // DXF・DWG のレイアウトは { view } で選ぶ
  const { scene, error, loading } = useDrawing(file, READERS);
  if (loading) return <p>読み込み中…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <Drawing scene={scene} label={file?.name ?? "図面"} /> : null;
}

function Drawing({ scene, label }: { scene: Scene; label: string }) {
  const { ref, viewer } = useDrawingViewer(scene, { label, handlers: { wheel: "modifier" } });
  const tool = useViewerState(viewer, (state) => state.tool);
  const distance = useViewerState(viewer, (state) => state.distance);
  return (
    <>
      <button type="button" onClick={() => viewer?.fit()}>全体</button>
      <button type="button" aria-pressed={tool === "measure"} onClick={() => viewer?.setTool("measure")}>
        計測
      </button>
      <div ref={ref} style={{ height: 480 }} />
      {distance ? <p>用紙 {distance.paper.toFixed(1)} mm、実寸 {distance.real?.toFixed(0) ?? "-"} mm</p> : null}
    </>
  );
}
```

試用画面の [2D のサンプル](playground/drawing-sample.tsx) は、この形で操作盤・レイヤの一覧・属性・読み取りの注意・
状態の欄を作っている。

React を使わない場合は、`DrawingViewer` に要素と図面を渡す。`load` を持つ読み取り（DWG）は、読む前にそれを待つ。

```ts
import { readDrawing } from "@zumenjs/core";
import { dwg, initDwg, isDwg } from "@zumenjs/dwg";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { sxf } from "@zumenjs/sxf";
import { DrawingViewer } from "@zumenjs/viewer";

const bytes = new Uint8Array(await file.arrayBuffer());
if (isDwg(bytes)) await initDwg(); // 最初の DWG で読み取りを読み込む。2 度目からはすぐに終わる
const scene = readDrawing(bytes, [sxf, jww, dxf, dwg]);
const viewer = new DrawingViewer(element, scene, { label: file.name });

viewer.fitBounds(scene.entities[0].bbox, { padding: 40 }); // 図形へ移る
const { center, zoom } = viewer.getView(); // viewer.jumpTo({ center, zoom }) でこの表示範囲に戻る
viewer.on("move", () => {
  // 用紙の点 (120, 80) に置いた HTML を、表示範囲の変化に追従させる
  const [px, py] = viewer.toScreen(120, 80);
  marker.style.transform = `translate(${px}px, ${py}px)`;
});
viewer.on("cursor", ({ point }) => console.log(point)); // ポインターの下の用紙の座標（mm）
viewer.setHighlight("search", found); // 検索で見つけた図形を、その名前の色で強調する
viewer.subscribe(() => console.log(viewer.getState().selection));
```

道具は用紙の mm でポインターのイベントを受け、図面の上に描く。組み込みの選択・計測・移動も同じ口の道具である。
色は要素の CSS のカスタムプロパティ（黒背景用と白背景用）から読むので、利用側の配色に合わせられる。

```ts
const pins: [number, number][] = [];
viewer.addTool({
  name: "pin",
  click(event, viewer) {
    pins.push([event.x, event.y]);
    viewer.invalidate();
  },
  draw(ctx, viewer) {
    for (const [x, y] of pins) {
      const [px, py] = viewer.toScreen(x, y);
      ctx.fillRect(px - 4, py - 4, 8, 8);
    }
  },
});
viewer.setTool("pin");
```

```css
.drawing {
  --zumen-background-dark: #1f2023; /* 暗い色なので、黒に近い線は白で描く */
  --zumen-selection-dark: #ffd43d;
  --zumen-highlight-search-dark: #5ad2ff;
  --zumen-fit-padding: 16;
}
```

表示範囲・状態・操作のハンドラ・道具・見た目の詳細は [packages/viewer](packages/viewer/README.md)（英語）を参照。

表示せずに解析だけを使う場合は、形式のままのモデルを読む（Node.js でも動く）。

```ts
import { parseSfc, sxfToScene } from "@zumenjs/sxf";

const drawing = parseSfc(bytes); // フィーチャ、レイヤ、部分図、表題欄
const scene = sxfToScene(drawing); // 必要なら表示用モデルにする
```

`Scene` は用紙の mm・Y 軸上向きの座標で、部分図・作図部品の配置を解決済みの図形を持つ。
要素ごとに属性の一覧と、部分図の中の要素なら元の座標への変換（実寸の計測用、`toReal`）を持つ。
図形と属性は、どの形式でも同じ英語のキーで選び、人には日本語の名前を見せる。

```ts
const circles = scene.entities.filter((entity) => entity.kind === "circle"); // entity.type は「円」
const radius = circles[0]?.properties.find((property) => property.key === "radius")?.raw; // label は「半径」
```

## Rust と Python

読み取りを Rust（`crates/`）に移し、Python の結合部（`bindings/python`）と WebAssembly（`bindings/wasm`）を作っている。
Rust の読み取りは TypeScript と同じ `Scene` を返す。`tests/fixtures` の図面で両方の結果を比べ、手元の図面は
`pnpm run parity` で比べる。JWW・SXF・DXF は移植済み。DWG は Rust だけで読み、AutoCAD が同じ図面を書き出した
DXF と比べて確かめ、ブラウザでは `@zumenjs/dwg` で読む。ほかのブラウザのパッケージは、形式ごとに結果が一致したものから
WebAssembly の読み取りに切り替える。

```python
import zumen

scene = zumen.read("plan.jww")  # 形式は中身で判定する
received = zumen.read("D0001.P2Z", max_bytes=200_000_000, max_entities=1_000_000, timeout=30)  # 受け取った図面
png = scene.render_png(dpi=150)  # サムネイルや、画像を読む AI への添付
pdf = scene.render_pdf()  # 用紙を実寸で、検索できる文字つきで（render_svg も同じ）
part = scene.render_png(dpi=300, region=(100, 50, 200, 120))  # 用紙の mm で範囲を指定して拡大
json_text = scene.to_json()  # @zumenjs/core の sceneFromJson で Scene に戻せる

texts = scene.texts()  # 文字列・位置・高さ・向き・レイヤ（用紙の mm）
scene.title_block()  # 表題欄の図面番号・改訂・図面名・工事名・縮尺・作成年月日
door = scene.entity("#120")  # 1 つの図形の種類・レイヤ・外接矩形・文字・属性
table = scene.to_arrow()  # すべての図形を pyarrow.Table に（pip install "zumen[arrow]"）
image = scene.render(size=(2000, 2000))  # PNG と、画像の px と用紙の mm の変換
changes = zumen.compare(zumen.read("plan-rev1.jww"), scene)  # 削除・追加・変更した図形
changes.scene.render_png(dpi=150)  # 変更前は赤、変更後は青、変わらない図形は灰
for entity in image.entities_at(640, 480):  # その画素に描いた図形（外接矩形の小さい順）
    print(entity["id"], entity["box"], entity["text"])

zumen.convert("plan.jww", "dxf").save("plan.dxf")  # どの形式も AutoCAD 2013 の DXF に。変えた内容は notices で分かる
zumen.convert("plan.dwg", "sfc").save("plan.sfc")  # SXF（SXF Ver.3.1 の SFC）にも
```

AI のアプリは、Python のパッケージの MCP サーバー（`pip install "zumen[mcp]"` の後に `zumen-mcp --root <フォルダ>`）で
図面を読める。開く、範囲を描く、文字と図形を探す、図形の形を用紙の mm と実寸の mm で読む、実寸で測る、版を比べる、の道具がある。
[bindings/python](bindings/python/README.md#mcp-server) を参照。エージェントに自前の道具を持たせるアプリは、同じ関数を
`zumen.tools` から呼ぶ。結果は形の決まった JSON で、大きさに上限がある
（[Tools for AI agents](bindings/python/README.md#tools-for-ai-agents)）。

```bash
cargo run --release -p zumen-cli -- render plan.jww -o plan.png --dpi 150 --map plan.json
cargo run --release -p zumen-cli -- render plan.jww -o plan.pdf  # plan.svg でも
cargo run --release -p zumen-cli -- texts plan.jww --region 100,50,200,120  # 1 行に 1 つの JSON
cargo run --release -p zumen-cli -- title plan.jww  # 表題欄の項目
cargo run --release -p zumen-cli -- convert plan.dwg -o plan.dxf
cargo run --release -p zumen-cli -- convert plan.sfc -o plan.dxf
cargo run --release -p zumen-cli -- convert plan.dxf -o plan.sfc
```

`convert` は、どの形式の図面も AutoCAD の図面のモデルを経て AutoCAD 2013 の DXF に書く。DXF・DWG はハンドル・表・
ブロック・レイアウトを保つ。SXF・JWW はモデル空間を実寸にして写す。SXF の部分図・JWW のレイヤグループの縮尺ごとに、
用紙のレイアウトのビューポートがその縮尺で見せ、用紙にじかに描いたもの（図枠・表題欄）は紙空間に置く。レイヤ・色・線種・
線の太さ・フォント・縦書き・ブロック・寸法・ハッチングを AutoCAD の図形にし、レイアウトは元の図面と同じに描ける
（[決定 0008（英語）](docs/decisions/0008-sheets-and-scales-in-the-autocad-model.md)）。
同じモデルから SXF（SXF Ver.3.1 の SFC）にも書く。図面を開くレイアウトが用紙、ビューポートはそれぞれ実寸の部分図をその縮尺で
配置したもの、ブロックは作図部品になる。SXF にない図形（複数行文字・引出線・寸法・塗り）は zumen が描く線・塗り・文字にするので、
SFC は元の図面と同じに描ける（[決定 0010（英語）](docs/decisions/0010-write-sxf-from-the-autocad-model.md)）。

| クレート | 中身 |
| --- | --- |
| `zumen-core` | 表示用モデル（`@zumenjs/core` と同じ JSON）、幾何、文字の配置、図面の文字と図形と表題欄の項目の取り出し、2 つの版の比較 |
| `zumen-sxf` | `parse_sfc`・`parse_p21`（SXF のフィーチャのモデル）、`sxf_to_scene`、読み取りの `SXF`、`write_sfc`（フィーチャのモデルを SFC に書く） |
| `zumen-jww` | `read_jww`（JWW のままのモデル）、`jww_to_scene`、読み取りの `JWW` |
| `zumen-dxf` | `parse_dxf`（DWG と共有する AutoCAD の図形モデル）、`dxf_to_scene`、読み取りの `DXF`、`write_dxf`（モデルを AutoCAD 2013 の DXF に書く） |
| `zumen-dwg` | `read_dwg`（DWG 2000〜2018 を `zumen-dxf` の AutoCAD の図形モデルにする）、読み取りの `DWG` |
| `zumen-convert` | `sxf_to_dxf`・`jww_to_dxf`（SXF・JWW を AutoCAD の図形モデルに写し、DXF に書けるようにする）、`dxf_to_sxf`（モデルを SXF の図面に写し、SFC に書けるようにする） |
| `zumen` | すべての読み取りをまとめた `read`・`detect` と、`convert`（どの形式も DXF・SFC に書き直す）。受け取った図面のためのバイト数・図形の数・時間の上限 |
| `zumen-render` | ブラウザの描画と同じ規則で PNG・SVG・PDF に描き、描いた図形の画像の上の位置を返す。日本語の文字は同梱の BIZ UDゴシックで描く |
| `zumen-cli` | `zumen` コマンド（`detect`、`scene` で JSON、`texts`・`entities` で 1 行 1 つの JSON、`title` で表題欄、`render` で PNG、`compare` で版の比較、`convert` で DXF・SFC） |

Python では、読み取り・描画・書き出しの間 GIL を外す。

## 文書

- [構成と拡張の口（英語）](docs/architecture.md)
- [SXF（SFC・P21）の扱い](docs/formats/sxf.md)
- [JWW の扱い](docs/formats/jww.md)
- [DXF の扱い（英語）](docs/formats/dxf.md)
- [DWG の扱い（英語）](docs/formats/dwg.md)
- [検証の記録（英語）](docs/verification.md)
- [設計の判断の記録（英語）](docs/decisions/README.md)
- [開発への参加（英語）](CONTRIBUTING.md)

## 開発

```bash
pnpm install --frozen-lockfile
pnpm exec playwright install --only-shell chromium
pnpm run check
```

`check` は Biome、WebAssembly の読み取りのビルドと正解との比較（`pnpm run wasm`）、型検査、単体テスト、依存監査、
各パッケージ（core、viewer、sxf・jww・dxf・dwg、react の順）と試用画面のビルドに加え、
Rust（rustfmt・Clippy・テスト・wasm32 のビルド）と Python の結合部（Ruff・maturin・pytest）を検査する。
WebAssembly の読み取りは `scripts/wasm.mjs` が cargo と wasm-bindgen-cli（`Cargo.lock` の wasm-bindgen と同じ版）で
`packages/dwg/wasm` に作る。`pnpm dev` と `check` は最初にこれを走らせる。
Rust は `rust-toolchain.toml`、Python は uv と `bindings/python/.python-version` に固定する。
TS の読み取りを変えたら `pnpm run golden` で `tests/fixtures` の正解を作り直し、Rust の読み取りも同じ結果に直す。
エンジンと React のフックのテスト（`*.browser.test.*`）は Canvas とポインターのイベントを使うので、Playwright の
Chromium（headless shell）で動かす。
開発中は、パッケージ同士が `package.json` の exports の `@zumenjs/source` 条件で `src` を読むため、ビルドしなくても変更がすぐ反映される。
Node.js は `.node-version`、pnpm は `package.json` の `packageManager` に固定する。

試用画面は `pnpm dev` で開く（ポートは [config/local-ports.json](config/local-ports.json) の `playground`、既定 24100）。
Git に入れない図面は、置いたフォルダーを `ZUMEN_SAMPLES` に指定し、`http://127.0.0.1:24100/?sample=<ファイル名>` で開く。
ファイルの選択・ドロップでも開ける。図面は公開 API だけで作った 2D のサンプル（`playground/drawing-sample.tsx`）で表示する。
DWG も中身で見分け、初めて開いたときに WebAssembly の読み取りを読み込む。

## ライセンス

公開の前に決める。参照した第三者のソフトウェア・資料は [packages/jww/THIRD_PARTY_NOTICES.md](packages/jww/THIRD_PARTY_NOTICES.md)、
[packages/dxf/THIRD_PARTY_NOTICES.md](packages/dxf/THIRD_PARTY_NOTICES.md) に記録した。
`zumen-render` に同梱するフォントは BIZ UDゴシック（SIL Open Font License 1.1、[crates/zumen-render/fonts/OFL.txt](crates/zumen-render/fonts/OFL.txt)）。
