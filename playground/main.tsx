import { useDrawing } from "@zumenjs/react";
import { reader } from "@zumenjs/reader";
import { type DragEvent, StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { DrawingSample } from "./drawing-sample.js";

const ACCEPT = ".sfc,.p21,.sfz,.p2z,.jww,.dxf,.dwg";
const READERS = [reader];

/** 図面を選ぶ・ドロップする・?sample=名前 で開く試用画面 */
function App() {
  const [file, setFile] = useState<File | null>(null);
  // DXF のモデル空間・レイアウトなど、図面の中の表示。ファイルを変えたら既定の表示に戻す
  const [view, setView] = useState<string | undefined>(
    () => new URLSearchParams(window.location.search).get("view") ?? undefined,
  );
  const [over, setOver] = useState(false);
  // useDrawing が初めて図面を開いたときに reader.load で WebAssembly を読み込む。
  const drawing = useDrawing(file, READERS, { view });
  const { error, loading } = drawing;
  const open = (next: File | null) => {
    setView(undefined);
    setFile(next);
  };

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
        <h1 style={{ margin: 0, fontSize: 18 }}>図面のビューア</h1>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          図面（SFC・P21・P2Z・SFZ・JWW・DXF・DWG）
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
        {drawing.scene ? <DrawingSample scene={drawing.scene} label={file?.name} onViewChange={setView} /> : null}
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
