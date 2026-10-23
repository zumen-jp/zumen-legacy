"use client";

import type { MarkupDocument } from "@zumenjs/core";
import { type DrawingViewer, MarkupLayer, type MarkupLayerOptions } from "@zumenjs/viewer";
import { useEffect, useRef, useState } from "react";

export interface UseMarkupLayerOptions extends Omit<MarkupLayerOptions, "document"> {
  /** 層の文書。null の間は層を作らない（図面の SHA-256 を計算している間など） */
  document: MarkupDocument | null;
}

/**
 * @zumenjs/viewer の MarkupLayer を viewer の上に作り、viewer が替わるか外れたら壊す。document が null の間は作らず、
 * null になったら壊す。document が層の今の文書と違うものに変わったら setDocument で替え（取り消しの記録を消す）、
 * change イベントの文書を戻したときは何もしない。author が変わったら setAuthor で替える。ほかの options は
 * 作るときにだけ使う。状態は useMarkupState、イベントは useMarkupEvent で読む
 */
export function useMarkupLayer(
  viewer: DrawingViewer | null | undefined,
  options: UseMarkupLayerOptions,
): MarkupLayer | null {
  const [layer, setLayer] = useState<MarkupLayer | null>(null);
  const latest = useRef(options);
  latest.current = options;
  const ready = options.document !== null;

  useEffect(() => {
    if (!viewer || !ready) return;
    const { document, ...rest } = latest.current;
    if (!document) return;
    const instance = new MarkupLayer(viewer, { ...rest, document });
    setLayer(instance);
    return () => {
      instance.destroy();
      setLayer(null);
    };
  }, [viewer, ready]);

  const { document, author } = options;
  useEffect(() => {
    if (layer && document && layer.getState().document !== document) layer.setDocument(document);
  }, [layer, document]);
  useEffect(() => {
    layer?.setAuthor(author ?? null);
  }, [layer, author]);

  return layer;
}
