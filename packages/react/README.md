# @zumenjs/react

図面エンジンを React の画面に組み込むフックです。画面のラベル・ボタン・パネル・CSS は利用側で用意します。サンプル画面は日本語です。

| フック | 用途 |
| --- | --- |
| `useDrawingViewer` | 要素に `DrawingViewer` を作成し、破棄まで管理する |
| `useViewerState` | エンジンの状態から必要な値を取り出す |
| `useViewerEvent` | 表示範囲・カーソルなどのイベントを受け取る |
| `useDrawing` | ファイルを読み取る。読み取りモジュールの初期化も待つ |
| `useMarkupLayer` | 図面にマークアップの層を重ねる |
| `useMarkupState`・`useMarkupEvent` | マークアップの状態・イベントを受け取る |

```tsx
"use client";

import { useDrawing, useDrawingViewer } from "@zumenjs/react";
import { reader } from "@zumenjs/reader";
import type { Scene } from "@zumenjs/core";

const READERS = [reader];

export function DrawingView({ file }: { file: File | null }) {
  const { scene, loading, error } = useDrawing(file, READERS);
  if (loading) return <p>読み込み中…</p>;
  if (error) return <p role="alert">{error.message}</p>;
  return scene ? <Drawing scene={scene} /> : null;
}

function Drawing({ scene }: { scene: Scene }) {
  const { ref, viewer } = useDrawingViewer(scene, { label: "図面" });
  return <><button type="button" onClick={() => viewer?.fit()}>全体表示</button><div ref={ref} style={{ height: 480 }} /></>;
}
```

完成した画面例は[試用画面](../../playground/drawing-sample.tsx)、操作 API は[ビューア](../viewer/README.md)を参照してください。

## 英語の補足資料

対応表・実装上の細則・従来のコード例は [README.en.md](README.en.md) にも保存しています。

<details>
<summary>従来の見出しから詳細を探す</summary>

<a id="zumenjsreact"></a>

[zumenjsreact](README.en.md#zumenjsreact)

</details>
