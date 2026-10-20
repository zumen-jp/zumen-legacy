"use client";

import type { Scene } from "@zumenjs/core";
import { DrawingViewer, type DrawingViewerOptions } from "@zumenjs/viewer";
import { useEffect, useRef, useState } from "react";

export interface DrawingViewerHandle {
  /** 図面を描く要素に渡す ref。要素の大きさは利用側で決める */
  ref: (element: HTMLElement | null) => void;
  /** 表示と操作のエンジン。要素が付くまでは null */
  viewer: DrawingViewer | null;
}

/**
 * @zumenjs/viewer の DrawingViewer を、ref を渡した要素の上に作り、要素が外れたら壊す。
 * 状態は useViewerState、イベントは useViewerEvent で読み、操作は viewer のメソッドで行う。
 * scene が変わったら setScene で替え（レイヤの表示・選択・計測を初期化して全体表示）、label が変わったら付け替える。
 * ほかの options は作るときにだけ使う。後から変えるときは viewer の setHandlers・setStyle・setTool などを呼ぶ
 */
export function useDrawingViewer(scene: Scene, options: DrawingViewerOptions = {}): DrawingViewerHandle {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [viewer, setViewer] = useState<DrawingViewer | null>(null);
  // 作るときの値。作り直さずに最新の値を使う
  const latest = useRef({ scene, options });
  latest.current = { scene, options };
  const applied = useRef<Scene | null>(null);

  useEffect(() => {
    if (!element) return;
    const initial = latest.current;
    const instance = new DrawingViewer(element, initial.scene, initial.options);
    applied.current = initial.scene;
    setViewer(instance);
    return () => {
      instance.destroy();
      setViewer(null);
    };
  }, [element]);

  useEffect(() => {
    if (!viewer || applied.current === scene) return;
    applied.current = scene;
    viewer.setScene(scene);
  }, [viewer, scene]);

  const { label } = options;
  useEffect(() => {
    viewer?.setLabel(label ?? null);
  }, [viewer, label]);

  return { ref: setElement, viewer };
}
