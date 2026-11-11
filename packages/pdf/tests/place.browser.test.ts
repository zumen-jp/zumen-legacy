/// <reference types="vite/client" />

import type { Scene, Underlay } from "@zumenjs/core";
import { apply } from "@zumenjs/core/geometry";
import { GlobalWorkerOptions, getDocument, type PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { placePdfUnderlays } from "../src/index.js";
import { drawingPdf } from "./pdf.js";

GlobalWorkerOptions.workerSrc = workerUrl;

let pdf: PDFDocumentProxy;
beforeAll(async () => {
  pdf = await getDocument({ data: drawingPdf() }).promise;
});
afterAll(async () => {
  await pdf.loadingTask.destroy();
});

/** DXF の PDF の underlay のように、大きさが分からず、挿入点（ページの左下の角）が (10, 20) にある underlay */
function unplaced(id: string, file: string, page: number): Underlay {
  return {
    id,
    kind: "pdf",
    file,
    page,
    width: 0,
    height: 0,
    matrix: [0.5, 0, 0, -0.5, 10, 20],
    layers: [],
    bbox: { minX: 10, minY: 20, maxX: 10, maxY: 20 },
  };
}

function scene(underlays: Underlay[]): Scene {
  return {
    format: "dxf",
    version: "AC1032",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 20, maxY: 20 },
    layers: [],
    entities: [],
    underlays,
    notices: [],
    metadata: [],
    background: "dark",
  };
}

describe("placePdfUnderlays", () => {
  it("places each page at the size pdf.js draws it, opening each file once", async () => {
    const opened: (string | undefined)[] = [];
    const before = scene([
      unplaced("first", "plan.pdf", 1),
      unplaced("turned", "plan.pdf", 2),
      unplaced("missing-page", "plan.pdf", 9),
      unplaced("missing-file", "other.pdf", 1),
    ]);
    const placed = await placePdfUnderlays(before, {
      document: (file) => {
        opened.push(file);
        return file === "plan.pdf" ? pdf : null;
      },
    });
    expect(opened).toEqual(["plan.pdf", "other.pdf"]);
    const byId = new Map((placed.underlays ?? []).map((underlay) => [underlay.id, underlay]));
    expect(byId.get("first")).toMatchObject({ width: 200, height: 100 });
    // /Rotate 90 のページは回した向きの大きさで、左下の角が挿入点に来る
    const turned = byId.get("turned") as Underlay;
    expect(turned).toMatchObject({ width: 100, height: 200 });
    expect(apply(turned.matrix, 0, 200)).toEqual([10, 20]);
    expect(turned.bbox).toEqual({ minX: 10, minY: 20, maxX: 60, maxY: 120 });
    // 開けないページとファイルは置かない
    expect(byId.get("missing-page")?.width).toBe(0);
    expect(byId.get("missing-file")).toBe(before.underlays?.[3]);
    expect(placed.bounds).toEqual({ minX: 0, minY: 0, maxX: 110, maxY: 120 });
  });

  it("returns the same scene when no underlay waits for a size", async () => {
    const before = scene([{ ...unplaced("placed", "plan.pdf", 1), width: 200, height: 100 }]);
    expect(await placePdfUnderlays(before, { document: () => pdf })).toBe(before);
  });
});
