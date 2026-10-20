"use client";

import { useEffect, useRef, useState } from "react";
import { StructureViewer, type StructureViewerOptions } from "../three/viewer.js";
import type { StructureModel } from "../types.js";

export interface StructureViewerHandle {
  /** モデルを描く要素に渡す ref。要素の大きさは利用側で決める */
  ref: (element: HTMLElement | null) => void;
  /** 表示と操作のエンジン。要素が付くまでは null */
  viewer: StructureViewer | null;
}

/**
 * StructureViewer を、ref を渡した要素の上に作り、要素が外れたら壊す。
 * 状態は useStructureState、イベントは useStructureEvent で読み、操作は viewer のメソッドで行う。
 * model が変わったら setModel で替え（絞り込み・選択を初期化して全体を斜めから見る）、label が変わったら付け替える。
 * ほかの options は作るときにだけ使う。後から変えるときは viewer の setHandlers・setStyle・setTool などを呼ぶ
 */
export function useStructureViewer(model: StructureModel, options: StructureViewerOptions = {}): StructureViewerHandle {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [viewer, setViewer] = useState<StructureViewer | null>(null);
  // 作るときの値。作り直さずに最新の値を使う
  const latest = useRef({ model, options });
  latest.current = { model, options };
  const applied = useRef<StructureModel | null>(null);

  useEffect(() => {
    if (!element) return;
    const initial = latest.current;
    const instance = new StructureViewer(element, initial.model, initial.options);
    applied.current = initial.model;
    setViewer(instance);
    return () => {
      instance.destroy();
      setViewer(null);
    };
  }, [element]);

  useEffect(() => {
    if (!viewer || applied.current === model) return;
    applied.current = model;
    viewer.setModel(model);
  }, [viewer, model]);

  const { label } = options;
  useEffect(() => {
    viewer?.setLabel(label ?? null);
  }, [viewer, label]);

  return { ref: setElement, viewer };
}
