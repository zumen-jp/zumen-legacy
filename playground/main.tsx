import { dwg, initDwg, isDwg } from "@zumenjs/dwg";
import { dxf } from "@zumenjs/dxf";
import { jww } from "@zumenjs/jww";
import { CadViewer, useCadFile } from "@zumenjs/react";
import { isStb } from "@zumenjs/stb";
import { sxf } from "@zumenjs/sxf";
import { type DragEvent, lazy, StrictMode, Suspense, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import type { StructureStatus } from "./structure-view.js";

const ACCEPT = ".sfc,.p21,.jww,.dxf,.dwg,.stb";
const READERS = [sxf, jww, dxf, dwg];
const StructureView = lazy(() => import("./structure-view.js").then((module) => ({ default: module.StructureView })));

/** 図面・構造モデルを選ぶ・ドロップする・?sample=名前 で開く試用画面 */
function App() {
  const [file, setFile] = useState<File | null>(null);
  // ST-Bridge の構造モデルか図面かは、拡張子ではなく中身で決める。今のファイルについて決まるまでは null。
  // DWG は読み取り（WebAssembly）を読み込んでから決まったことにし、読み込めなかったら error にする
  const [detected, setDetected] = useState<{ file: File; structure: boolean; error: Error | null } | null>(null);
  const decided = file && detected?.file === file ? detected : null;
  const structure = decided && !decided.error ? decided.structure : null;
  const [structureStatus, setStructureStatus] = useState<StructureStatus>({ loading: false, error: null });
  // DXF のモデル空間・レイアウトなど、図面の中の表示。ファイルを変えたら既定の表示に戻す
  const [view, setView] = useState<string | undefined>(
    () => new URLSearchParams(window.location.search).get("view") ?? undefined,
  );
  const [over, setOver] = useState(false);
  const drawing = useCadFile(structure === false ? file : null, READERS, { view });
  const { error, loading } = structure
    ? structureStatus
    : { error: decided?.error ?? drawing.error, loading: drawing.loading || (file !== null && decided === null) };
  const open = (next: File | null) => {
    setView(undefined);
    setFile(next);
  };

  useEffect(() => {
    if (!file) return;
    let current = true;
    const decide = async () => {
      // 頭を読めないファイルは図面として渡し、useCadFile がエラーを伝える
      const head = await file
        .slice(0, 4096)
        .arrayBuffer()
        .catch(() => null);
      if (!head || !isDwg(new Uint8Array(head))) return { structure: head !== null && isStb(head), error: null };
      // DWG の読み取り（WebAssembly、約 1.6 MB）は、初めて DWG を開いたときに読み込む
      return initDwg().then(
        () => ({ structure: false, error: null }),
        (reason: unknown) => ({
          structure: false,
          error: new Error(`DWG の読み取りを読み込めません（${reason instanceof Error ? reason.message : reason}）`),
        }),
      );
    };
    decide().then((result) => {
      if (current) setDetected({ file, ...result });
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
          図面（SFC・P21・JWW・DXF・DWG）・構造モデル（ST-Bridge）
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
          <CadViewer scene={drawing.scene} label={file?.name} style={{ height: "100%" }} onViewChange={setView} />
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
