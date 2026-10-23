"use client";

import type { MarkupEventMap, MarkupLayer } from "@zumenjs/viewer";
import { useEffect, useRef } from "react";

/**
 * マークアップの層のイベント（change）を受ける。listener は最新のものを呼ぶので、毎回作ってもよい。
 * layer が null の間は何もしない
 */
export function useMarkupEvent<K extends keyof MarkupEventMap>(
  layer: MarkupLayer | null | undefined,
  type: K,
  listener: (event: MarkupEventMap[K]) => void,
): void {
  const latest = useRef(listener);
  latest.current = listener;
  useEffect(() => {
    if (!layer) return;
    return layer.on(type, (event) => latest.current(event));
  }, [layer, type]);
}
