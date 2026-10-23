import { pdfUnderlayRenderer } from "@zumenjs/pdf";
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
