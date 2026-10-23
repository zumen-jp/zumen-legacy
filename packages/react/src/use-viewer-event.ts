"use client";

import type { DrawingViewer, ViewerEventMap } from "@zumenjs/viewer";
import { useEffect, useRef } from "react";

/**
 * エンジンのイベント（move・moveend・cursor・underlay）を受ける。listener は最新のものを呼ぶので、毎回作ってもよい。
 * viewer が null の間は何もしない
 */
export function useViewerEvent<K extends keyof ViewerEventMap>(
  viewer: DrawingViewer | null | undefined,
  type: K,
  listener: (event: ViewerEventMap[K]) => void,
): void {
  const latest = useRef(listener);
  latest.current = listener;
  useEffect(() => {
    if (!viewer) return;
    return viewer.on(type, (event) => latest.current(event));
  }, [viewer, type]);
}
