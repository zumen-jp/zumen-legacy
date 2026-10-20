"use client";

import { useEffect, useRef } from "react";
import type { StructureViewer, StructureViewerEventMap } from "../three/viewer.js";

/**
 * エンジンのイベント（move・moveend・select・hover）を受ける。listener は最新のものを呼ぶので、毎回作ってもよい。
 * viewer が null の間は何もしない
 */
export function useStructureEvent<K extends keyof StructureViewerEventMap>(
  viewer: StructureViewer | null | undefined,
  type: K,
  listener: (event: StructureViewerEventMap[K]) => void,
): void {
  const latest = useRef(listener);
  latest.current = listener;
  useEffect(() => {
    if (!viewer) return;
    return viewer.on(type, (event) => latest.current(event));
  }, [viewer, type]);
}
