import { placeUnderlays, type Scene, type Underlay, type UnderlaySize } from "@zumenjs/core";
import type { PDFDocumentProxy } from "pdfjs-dist";
import type { PdfUnderlayOptions } from "./renderer.js";

/**
 * 図面がページの大きさを持たない PDF の underlay（DXF・DWG の PDF の underlay。width と height が 0）を、pdf.js が
 * 開いたページの大きさで置く（docs/decisions/0034）。大きさは描くときと同じ page.getViewport({ scale: 1 })
 * （/Rotate・クロップボックス・/UserUnit を済ませた pt）で、placeUnderlays（@zumenjs/core）で置いた Scene を返す。
 * document は pdfUnderlayRenderer と同じもので、file ごとに一度だけ呼ぶ。開けない文書・ページの underlay は置かずに
 * 残す（width と height が 0 のまま）。置くものがなければ同じ Scene を返す
 */
export async function placePdfUnderlays(scene: Scene, options: Pick<PdfUnderlayOptions, "document">): Promise<Scene> {
  const unplaced = (scene.underlays ?? []).filter(
    (underlay) => underlay.kind === "pdf" && underlay.width === 0 && underlay.height === 0,
  );
  if (unplaced.length === 0) return scene;
  const documents = new Map<string | undefined, Promise<PDFDocumentProxy | null>>();
  const sizes = new Map<Underlay, UnderlaySize>();
  await Promise.all(
    unplaced.map(async (underlay) => {
      let document = documents.get(underlay.file);
      if (!document) {
        document = Promise.resolve()
          .then(() => options.document(underlay.file, underlay))
          .catch(() => null);
        documents.set(underlay.file, document);
      }
      const pdf = await document;
      if (!pdf) return;
      try {
        const { width, height } = (await pdf.getPage(underlay.page ?? 1)).getViewport({ scale: 1 });
        sizes.set(underlay, { width, height });
      } catch {
        // ページのない文書は置かない
      }
    }),
  );
  return placeUnderlays(scene, (underlay) => sizes.get(underlay));
}
