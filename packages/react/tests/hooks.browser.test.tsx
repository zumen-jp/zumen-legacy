import type { DrawingReader, Scene } from "@zumenjs/core";
import type { DrawingViewer } from "@zumenjs/viewer";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { type DrawingState, useDrawing, useDrawingViewer, useViewerState } from "../src/index.js";

// React のフックを Chromium で動かす。act で描き直しを待つ
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: { root: Root; container: HTMLElement }[] = [];
afterEach(() => {
  for (const { root, container } of roots.splice(0)) {
    act(() => root.unmount());
    container.remove();
  }
});

function render(element: React.ReactNode): Root {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  roots.push({ root, container });
  act(() => root.render(element));
  return root;
}

function scene(format = "test"): Scene {
  return {
    format,
    version: "1",
    sheet: null,
    bounds: { minX: 0, minY: 0, maxX: 10, maxY: 10 },
    layers: [{ id: 0, name: "0", visible: true }],
    entities: [
      {
        id: "a",
        type: "線分",
        source: "line",
        layers: [0],
        primitives: [
          {
            kind: "stroke",
            color: "#ffffff",
            width: 0.1,
            dashes: [],
            points: Float64Array.of(0, 0, 10, 10),
            closed: false,
          },
        ],
        bbox: { minX: 0, minY: 0, maxX: 10, maxY: 10 },
        properties: [],
      },
    ],
    notices: [],
    metadata: [],
    background: "dark",
  };
}

describe("@zumenjs/react", () => {
  // 画面の部品は状態のうち使う部分だけを読み、ほかの部分が変わっても描き直さない
  it("useViewerState は選んだ部分が変わったときだけ描き直し、useDrawingViewer は外れるとエンジンを壊す", () => {
    const drawing = scene();
    const renders: unknown[] = [];
    let current: DrawingViewer | null = null;
    const sameIds = (a: string[], b: string[]) => a.length === b.length && a.every((id, index) => id === b[index]);
    function Tool() {
      const { ref, viewer } = useDrawingViewer(drawing, { label: "図面" });
      current = viewer;
      const tool = useViewerState(viewer, (state) => state.tool);
      // 毎回新しい配列を作る selector は、isEqual で同じ値とみなせば描き直さない
      const ids = useViewerState(viewer, (state) => state.selection.map((entity) => entity.id), sameIds);
      renders.push([tool, ids]);
      return <div ref={ref} style={{ width: 200, height: 100 }} />;
    }
    const root = render(<Tool />);
    const viewer = current as DrawingViewer | null;
    if (!viewer) throw new Error("エンジンができていない");
    expect(renders.at(-1)).toEqual(["select", []]);
    expect(viewer.canvas.getAttribute("aria-label")).toBe("図面");
    const count = renders.length;
    act(() => viewer.setLineWeights(false));
    act(() => viewer.setHighlight("search", drawing.entities));
    expect(renders).toHaveLength(count);
    act(() => viewer.select(drawing.entities));
    act(() => viewer.setTool("measure"));
    expect(renders.slice(count)).toEqual([
      ["select", ["a"]],
      ["measure", ["a"]],
    ]);
    act(() => root.render(null));
    expect(viewer.canvas.isConnected).toBe(false);
  });

  // DWG の WebAssembly のように読み込みに時間がかかる読み取りは、その形式のファイルを開いたときにだけ読み込む
  it("useDrawing は中身を認めた読み取りの load だけを待ち、load の失敗はエラーにする", async () => {
    const loads: string[] = [];
    let finish = () => {};
    const slow: DrawingReader = {
      formats: ["slow"],
      detect: (bytes) => (bytes[0] === 0x53 ? "slow" : null),
      load: () => {
        loads.push("slow");
        return new Promise((resolve) => {
          finish = resolve;
        });
      },
      read: () => scene("slow"),
    };
    const broken: DrawingReader = {
      formats: ["broken"],
      detect: (bytes) => (bytes[0] === 0x42 ? "broken" : null),
      load: () => {
        loads.push("broken");
        return Promise.reject(new Error("読み取りを読み込めません"));
      },
      read: () => scene("broken"),
    };
    const readers = [slow, broken];
    let state: DrawingState | null = null;
    function Reader({ file }: { file: Blob }) {
      state = useDrawing(file, readers);
      return null;
    }
    const root = render(<Reader file={new Blob(["S"])} />);
    await vi.waitFor(() => expect(loads).toEqual(["slow"]));
    expect(state).toMatchObject({ scene: null, loading: true });
    await act(async () => finish());
    await vi.waitFor(() => expect(state?.scene?.format).toBe("slow"));
    expect(state).toMatchObject({ error: null, loading: false });

    act(() => root.render(<Reader file={new Blob(["B"])} />));
    await vi.waitFor(() => expect(state?.error?.message).toBe("読み取りを読み込めません"));
    expect(state).toMatchObject({ scene: null, loading: false });

    // どの読み取りも認めないファイルは、load を呼ばずに対応していない形式のエラーになる
    act(() => root.render(<Reader file={new Blob(["X"])} />));
    await vi.waitFor(() => expect(state?.error?.message).toMatch(/対応していない形式/));
    expect(loads).toEqual(["slow", "broken"]);
  });
});
