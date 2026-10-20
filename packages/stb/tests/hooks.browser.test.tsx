import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import { useStructureEvent, useStructureState, useStructureViewer } from "../src/react/index.js";
import type { StructureViewer } from "../src/three/index.js";
import { structure } from "./structure-fixture.js";

// @zumenjs/stb/react のフックを Chromium（WebGL）で動かす。act で描き直しを待つ
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

describe("@zumenjs/stb/react", () => {
  it("useStructureState は選んだ部分が変わったときだけ描き直し、useStructureViewer は外れるとエンジンを壊す", () => {
    const model = structure();
    const renders: unknown[] = [];
    const selected: string[][] = [];
    let current: StructureViewer | null = null;
    const sameKeys = (a: string[], b: string[]) => a.length === b.length && a.every((key, index) => key === b[index]);
    function Structure({ label }: { label: string }) {
      const { ref, viewer } = useStructureViewer(model, { label, theme: "light" });
      current = viewer;
      const story = useStructureState(viewer, (state) => state.story);
      // 毎回新しい配列を作る selector は、isEqual で同じ値とみなせば描き直さない
      const keys = useStructureState(viewer, (state) => state.selection.map((member) => member.key), sameKeys);
      useStructureEvent(viewer, "select", (event) => selected.push(event.selection.map((member) => member.key)));
      renders.push([story, keys]);
      return <div ref={ref} style={{ width: 200, height: 100 }} />;
    }
    const root = render(<Structure label="モデル" />);
    const viewer = current as StructureViewer | null;
    if (!viewer) throw new Error("エンジンができていない");
    expect(renders.at(-1)).toEqual([null, []]);
    expect(viewer.canvas.getAttribute("aria-label")).toBe("モデル");
    const count = renders.length;
    act(() => viewer.setColorMode("structure"));
    act(() => viewer.setGuides({ nodes: true }));
    act(() => viewer.jumpTo({ span: 1000 }));
    expect(renders).toHaveLength(count);
    act(() => viewer.select(model.members.slice(0, 1)));
    act(() => viewer.focusStory(1));
    expect(renders.slice(count)).toEqual([
      [null, ["C1"]],
      [1, ["C1"]],
    ]);
    expect(selected).toEqual([["C1"]]);
    act(() => root.render(<Structure label="別の名前" />));
    expect(viewer.canvas.getAttribute("aria-label")).toBe("別の名前");
    act(() => root.render(null));
    expect(viewer.canvas.isConnected).toBe(false);
  });

  it("useStructureViewer はモデルが変わると setModel で替える", () => {
    let current: StructureViewer | null = null;
    function Structure({ model }: { model: ReturnType<typeof structure> }) {
      const { ref, viewer } = useStructureViewer(model, { theme: "light" });
      current = viewer;
      return <div ref={ref} style={{ width: 200, height: 100 }} />;
    }
    const first = structure();
    const root = render(<Structure model={first} />);
    const viewer = current as StructureViewer | null;
    if (!viewer) throw new Error("エンジンができていない");
    act(() => viewer.focusStory(2));
    const second = structure();
    act(() => root.render(<Structure model={second} />));
    expect(current).toBe(viewer);
    expect(viewer.getState().model).toBe(second);
    expect(viewer.getState().story).toBeNull();
  });
});
