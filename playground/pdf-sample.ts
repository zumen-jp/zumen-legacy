import type { Scene } from "@zumenjs/core";
import { pdfUnderlayRenderer, placePdfUnderlays } from "@zumenjs/pdf";
import { detectDrawing } from "@zumenjs/reader";
import type { UnderlayRenderer } from "@zumenjs/viewer";
import type { PDFDocumentLoadingTask } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { useEffect, useMemo, useState } from "react";

/**
 * pdf.js は試用画面が持つ（docs/decisions/0029）。PDF を開くときに初めて読み込み、CAD の図面だけを開くときは
 * 読み込まない。ワーカーは Vite が配るファイルの URL から起こす
 */
async function loadPdfjs() {
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;
  return pdfjs;
}

/**
 * pdf.js が URL から読むファイル。vite.config.ts の pdfjsFiles が、pdfjs-dist の cmaps（図面が埋め込まない日本語の
 * フォントの文字）と wasm（スキャンした図面の JPEG 2000・JBIG2 の画像）を pdfjs/ に置く
 */
const CMAP_URL = new URL("pdfjs/cmaps/", document.baseURI).href;
const WASM_URL = new URL("pdfjs/wasm/", document.baseURI).href;

/**
 * PDF の図面のページを pdf.js で描く UnderlayRenderer。file が PDF のときだけ、そのバイトを pdf.js で開き、file が
 * 替わるか画面を閉じたら閉じる。ページの図形（当たり判定・吸着・計測に使う）は @zumenjs/reader が同じファイルから
 * 読み、pdf.js はページの画素だけを描く。PDF でなければ undefined
 */
export function usePdfUnderlayRenderer(file: File | null): UnderlayRenderer | undefined {
  const [task, setTask] = useState<PDFDocumentLoadingTask | null>(null);
  useEffect(() => {
    setTask(null);
    if (!file) return;
    let closed = false;
    let opened: PDFDocumentLoadingTask | null = null;
    file
      .arrayBuffer()
      .then(async (buffer) => {
        // 形式は中身で見る（@zumenjs/reader の読み取りと同じ規則）
        if (closed || detectDrawing(new Uint8Array(buffer)) !== "pdf") return;
        const { getDocument } = await loadPdfjs();
        if (closed) return;
        // pdf.js はバイトをワーカーへ移す。arrayBuffer() は呼ぶたびに新しいバイトを返すので、図面の読み取りの
        // バイトとは分かれている
        opened = getDocument({ data: new Uint8Array(buffer), cMapUrl: CMAP_URL, wasmUrl: WASM_URL });
        // 開けない PDF（パスワードなど）は、描き手が underlay の error で知らせる
        opened.promise.catch(() => undefined);
        setTask(opened);
      })
      .catch((reason: unknown) => console.error(reason));
    return () => {
      closed = true;
      void opened?.destroy();
    };
  }, [file]);
  return useMemo(
    () =>
      task
        ? // PDF の図面のページは、図面のファイルそのもの（file なし）を指す
          pdfUnderlayRenderer({ document: (name) => (name === undefined ? task.promise : null) })
        : undefined,
    [task],
  );
}

/**
 * DXF・DWG の PDF の underlay（docs/decisions/0034）。図面が書くファイルの名前を key で引いた、利用者が選んだ PDF を
 * pdf.js で開き、ページの大きさで置いた Scene（placePdfUnderlays）と、そのページを描く UnderlayRenderer を返す。
 * 図面はページの大きさを持たないので、PDF を開くまで underlay は置かれず、描かれない
 */
export function usePdfUnderlayFiles(
  scene: Scene | null,
  files: ReadonlyMap<string, File>,
  key: (file: string) => string,
): { scene: Scene | null; renderer: UnderlayRenderer | undefined } {
  const [tasks, setTasks] = useState<ReadonlyMap<string, PDFDocumentLoadingTask>>(new Map());
  useEffect(() => {
    setTasks(new Map());
    const pdfs = [...files].filter(([, file]) => /\.pdf$/i.test(file.name));
    if (pdfs.length === 0) return;
    let closed = false;
    const opened: PDFDocumentLoadingTask[] = [];
    loadPdfjs()
      .then(async ({ getDocument }) => {
        const next = new Map<string, PDFDocumentLoadingTask>();
        for (const [name, file] of pdfs) {
          const data = new Uint8Array(await file.arrayBuffer());
          if (closed) return;
          const task = getDocument({ data, cMapUrl: CMAP_URL, wasmUrl: WASM_URL });
          // 開けない PDF の underlay は置かれずに残る
          task.promise.catch(() => undefined);
          opened.push(task);
          next.set(name, task);
        }
        setTasks(next);
      })
      .catch((reason: unknown) => console.error(reason));
    return () => {
      closed = true;
      for (const task of opened) void task.destroy();
    };
  }, [files]);
  const options = useMemo(
    () => ({
      document: (file: string | undefined) => (file === undefined ? null : (tasks.get(key(file))?.promise ?? null)),
    }),
    [tasks, key],
  );
  const [placed, setPlaced] = useState<{ from: Scene; scene: Scene } | null>(null);
  useEffect(() => {
    if (!scene || tasks.size === 0) return;
    let current = true;
    placePdfUnderlays(scene, options)
      .then((next) => {
        if (current) setPlaced({ from: scene, scene: next });
      })
      .catch((reason: unknown) => console.error(reason));
    return () => {
      current = false;
    };
  }, [scene, tasks, options]);
  const renderer = useMemo(() => (tasks.size > 0 ? pdfUnderlayRenderer(options) : undefined), [tasks, options]);
  return { scene: placed && placed.from === scene && tasks.size > 0 ? placed.scene : scene, renderer };
}
