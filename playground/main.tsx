import { useDrawing } from "@zumenjs/react";
import { reader } from "@zumenjs/reader";
import { isStb } from "@zumenjs/stb";
import { type DragEvent, lazy, StrictMode, Suspense, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { DrawingSample } from "./drawing-sample.js";
import type { StructureStatus } from "./structure-view.js";

const ACCEPT = ".sfc,.p21,.sfz,.p2z,.jww,.dxf,.dwg,.stb";
const READERS = [reader];
const StructureView = lazy(() => import("./structure-view.js").then((module) => ({ default: module.StructureView })));

/** 図面・構造モデルを選ぶ・ドロップする・?sample=名前 で開く試用画面 */
function App() {
  const [file, setFile] = useState<File | null>(null);
  // ST-Bridge の構造モデルか図面かは、拡張子ではなく中身で決める。今のファイルについて決まるまでは null
  const [detected, setDetected] = useState<{ file: File; structure: boolean } | null>(null);
  const structure = file && detected?.file === file ? detected.structure : null;
  const [structureStatus, setStructureStatus] = useState<StructureStatus>({ loading: false, error: null });
  // DXF のモデル空間・レイアウトなど、図面の中の表示。ファイルを変えたら既定の表示に戻す
  const [view, setView] = useState<string | undefined>(
    () => new URLSearchParams(window.location.search).get("view") ?? undefined,
  );
  const [over, setOver] = useState(false);
  // 図面の読み取り（Rust の読み取りの WebAssembly、brotli で約 510 KB）は、useDrawing が初めて図面を開いたときに
  // reader.load で読み込む
  const drawing = useDrawing(structure === false ? file : null, READERS, { view });
  const { error, loading } = structure
    ? structureStatus
    : { error: drawing.error, loading: drawing.loading || (file !== null && structure === null) };
  const open = (next: File | null) => {
    setView(undefined);
    setFile(next);
  };

  useEffect(() => {
    if (!file) return;
    let current = true;
    // 頭を読めないファイルは図面として渡し、useDrawing がエラーを伝える
    file
      .slice(0, 4096)
      .arrayBuffer()
      .catch(() => null)
      .then((head) => {
        if (current) setDetected({ file, structure: head !== null && isStb(head) });
      });
    return () => {
      current = false;
    };
  }, [file]);

  useEffect(() => {
    const sample = new URLSearchParams(window.location.search).get("sample");
    if (!sample) return;
    fetch(`/samples/${encodeURIComponent(sample)}`)
      .then((response) => {
        if (!response.ok) throw new Error(`${sample} を取得できません（${response.status}）`);
        return response.blob();
      })
      .then((blob) => setFile(new File([blob], sample)))
      .catch((reason: unknown) => console.error(reason));
  }, []);

  const onDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    setOver(false);
    const dropped = event.dataTransfer.files[0];
    if (dropped) open(dropped);
  };

  return (
    <main
      style={{
        display: "grid",
        gridTemplateRows: "auto minmax(0, 1fr)",
        height: "100%",
        gap: 8,
        padding: 8,
        boxSizing: "border-box",
      }}
      onDragOver={(event) => {
        event.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <header style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 12 }}>
        <h1 style={{ margin: 0, fontSize: 18 }}>図面・構造モデルのビューア</h1>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          図面（SFC・P21・P2Z・SFZ・JWW・DXF・DWG）・構造モデル（ST-Bridge）
          <input type="file" accept={ACCEPT} onChange={(event) => open(event.currentTarget.files?.[0] ?? null)} />
        </label>
        <span aria-live="polite">
          {loading
            ? "読み込み中…"
            : error
              ? `読み込めませんでした: ${error.message}`
              : file
                ? file.name
                : "ファイルを選ぶか、ここへドロップしてください"}
        </span>
      </header>
      <div style={{ minHeight: 0, outline: over ? "3px dashed #0031d8" : undefined }}>
        {structure && file ? (
          <Suspense fallback={null}>
            <StructureView file={file} onStatus={setStructureStatus} />
          </Suspense>
        ) : drawing.scene ? (
          <DrawingSample scene={drawing.scene} label={file?.name} onViewChange={setView} />
        ) : null}
      </div>
    </main>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
