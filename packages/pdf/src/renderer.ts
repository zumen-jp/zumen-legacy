import type { Underlay } from "@zumenjs/core";
import type { UnderlayRenderer } from "@zumenjs/viewer";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";

type OptionalContentConfig = Awaited<ReturnType<PDFDocumentProxy["getOptionalContentConfig"]>>;

/**
 * pdf.js の AnnotationMode の値。pdf.js は型だけを読み、実行時には読まない（アプリケーションが開いた文書の
 * メソッドだけを呼ぶ）ので、公開された定数の値を写す
 */
const ANNOTATION_MODE_DISABLE = 0;
const ANNOTATION_MODE_ENABLE = 1;

/** 描く意図。表示に向けた optional content の既定（8.11.4.3 の /Usage /View）と、注釈の表示に従う */
const INTENT = "display";

export interface PdfUnderlayOptions {
  /**
   * underlay のページを持つ pdf.js の文書を返す。file は underlay.file で、PDF の図面のページなら undefined
   * （図面のファイルそのもの）、DXF の PDF underlay なら図面に書かれたファイルの名前。file ごとに一度だけ呼び、
   * 拒否するか null を返したときは、次の要求でまた呼ぶ。文書はアプリケーションが pdf.js の getDocument で開き、
   * 要らなくなったら閉じる（このパッケージは閉じない）
   */
  readonly document: (
    file: string | undefined,
    underlay: Underlay,
  ) => PDFDocumentProxy | null | PromiseLike<PDFDocumentProxy | null>;
  /**
   * underlay.contentLayers のレイヤが表示を切り替える optional content group の id（pdf.js の
   * OptionalContentConfig の id、"12R" など）。省略するか、group のないレイヤは、文書の既定の表示のまま描く
   */
  readonly optionalContent?: (layer: number, underlay: Underlay) => string | readonly string[] | null | undefined;
  /** 注釈の外観（appearance stream）を描くか。省略すると描く（pdf.js の既定） */
  readonly annotations?: boolean;
  /** ページの背景（Canvas の fillStyle）。省略すると白（pdf.js の既定） */
  readonly background?: string;
}

/**
 * PDF のページ（kind が "pdf" の underlay）を pdf.js で描く UnderlayRenderer を作る。要求の範囲だけを、要求の
 * 細かさで、viewer が渡す Canvas に描く。要求の signal が中止されたら pdf.js の描画を取り消す。
 * 中身の座標は、ページの /Rotate を済ませた向きの、左上を原点とする pt（pdf.js の getViewport({ scale: 1 })）。
 * ページの大きさが underlay の width・height と違っても、中身の全体に合わせて描く
 */
export function pdfUnderlayRenderer(options: PdfUnderlayOptions): UnderlayRenderer {
  const documents = new Map<string | undefined, Promise<PDFDocumentProxy | null>>();
  const open = async (underlay: Underlay): Promise<PDFDocumentProxy> => {
    const file = underlay.file;
    let document = documents.get(file);
    if (!document) {
      document = Promise.resolve().then(() => options.document(file, underlay));
      documents.set(file, document);
    }
    const pdf = await document.catch((error: unknown) => {
      if (documents.get(file) === document) documents.delete(file);
      throw error;
    });
    if (!pdf) {
      if (documents.get(file) === document) documents.delete(file);
      throw new Error(`underlay ${underlay.id} の PDF がありません`);
    }
    return pdf;
  };

  return async (request, { canvas, context, signal }) => {
    const { underlay, region, scale } = request;
    if (underlay.kind !== "pdf") throw new Error(`underlay ${underlay.id} は PDF ではありません（${underlay.kind}）`);
    const pdf = await open(underlay);
    if (signal.aborted) return;
    // page は 1 から。kind が "pdf" の underlay は必ず持つが、持たなければ最初のページとする
    const page = await pdf.getPage(underlay.page ?? 1);
    if (signal.aborted) return;
    const config = await contentConfig(pdf, request.underlay, request.hiddenLayers, options.optionalContent);
    if (signal.aborted) return;
    // 中身の座標 (x, y) を、画像の ((x − region.x) × scale, (y − region.y) × scale) に写す。ページの pt
    // （scale 1 の viewport）から中身の座標へは、underlay の大きさに合わせて kx・ky 倍する。縦の倍率は
    // viewport に、横の縦との違いと、範囲の左上へのずらしは transform に入れる（pdf.js は transform を
    // viewport の変換の前に掛ける）。ページと underlay の大きさが同じなら kx = ky = 1 で、0022 の描き方になる
    const base = page.getViewport({ scale: 1 });
    const kx = underlay.width / base.width;
    const ky = underlay.height / base.height;
    if (!(kx > 0 && ky > 0 && Number.isFinite(kx) && Number.isFinite(ky))) {
      throw new Error(`underlay ${underlay.id} のページの大きさが 0 です`);
    }
    const viewport = page.getViewport({ scale: scale * ky });
    const task: RenderTask = page.render({
      // pdf.js 5.4 より前は canvasContext だけを読み、5.4 からは canvas から同じ context を得る
      canvas,
      canvasContext: context,
      viewport,
      transform: [kx / ky, 0, 0, 1, -region.x * scale, -region.y * scale],
      intent: INTENT,
      annotationMode: options.annotations === false ? ANNOTATION_MODE_DISABLE : ANNOTATION_MODE_ENABLE,
      ...(options.background === undefined ? {} : { background: options.background }),
      ...(config ? { optionalContentConfigPromise: Promise.resolve(config) } : {}),
    });
    const cancel = () => task.cancel();
    signal.addEventListener("abort", cancel, { once: true });
    // render を呼んでいる間に中止されたときは、聞き始める前に中止の知らせが過ぎている
    if (signal.aborted) cancel();
    try {
      await task.promise;
    } catch (error) {
      // 取り消した描画は RenderingCancelledException で終わる。viewer はその結果を使わない
      if (signal.aborted) return;
      throw error;
    } finally {
      signal.removeEventListener("abort", cancel);
    }
  };
}

/**
 * 中身のレイヤの表示を、optional content group の表示にした設定（ISO 32000-1:2008, 8.11.4.3）。
 * 切り替えるものがなければ undefined で、pdf.js は文書の既定の設定で描く
 */
async function contentConfig(
  pdf: PDFDocumentProxy,
  underlay: Underlay,
  hiddenLayers: readonly number[],
  groupsOf: PdfUnderlayOptions["optionalContent"],
): Promise<OptionalContentConfig | undefined> {
  const layers = underlay.contentLayers ?? [];
  if (!groupsOf || layers.length === 0) return undefined;
  const states: [string, boolean][] = [];
  const hidden = new Set(hiddenLayers);
  for (const layer of layers) {
    const groups = groupsOf(layer, underlay);
    for (const id of typeof groups === "string" ? [groups] : (groups ?? [])) states.push([id, !hidden.has(layer)]);
  }
  if (states.length === 0) return undefined;
  // getOptionalContentConfig は呼ぶたびに既定の表示の新しい設定を返す
  const config = await pdf.getOptionalContentConfig({ intent: INTENT });
  // レイヤの表示をそのまま写す。ラジオボタンの組（/RBGroups）で、ほかの group を消すことはしない
  for (const [id, visible] of states) config.setVisibility(id, visible, false);
  return config;
}
