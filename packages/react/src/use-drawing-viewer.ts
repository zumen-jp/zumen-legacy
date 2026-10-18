"use client";

import {
  DrawingViewer,
  initialViewerState,
  type Scene,
  type SceneEntity,
  type ViewerState,
  type ViewerTheme,
} from "@zumenjs/core";
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

export interface UseDrawingViewerOptions {
  /** 背景。省略時は図面の想定。変えると、図面を開き直したときと同じく表示状態を初期化する */
  theme?: ViewerTheme;
  /** 操作を受ける Canvas の読み上げ用の説明 */
  label?: string;
  /** 利用者が要素を選んだとき・選択を外したとき */
  onSelect?: (entity: SceneEntity | null) => void;
}

export interface DrawingViewerHandle {
  /** 図面を描く要素に渡す ref。要素の大きさは利用側で決める */
  ref: (element: HTMLElement | null) => void;
  /** 表示と操作。要素が付くまでは null */
  viewer: DrawingViewer | null;
  /** 表示と操作の状態。要素が付くまでは、図面を開いた直後の状態 */
  state: ViewerState;
}

const subscribeNothing = () => () => {};

/**
 * @zumenjs/core の DrawingViewer を React で使う。画面の部品は state を読み、viewer のメソッドで操作する。
 * 図面（scene）か背景（theme）が変わったら、レイヤの表示・選択・計測を初期化して全体表示にする。
 */
export function useDrawingViewer(scene: Scene, options: UseDrawingViewerOptions = {}): DrawingViewerHandle {
  const { theme, label, onSelect } = options;
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [viewer, setViewer] = useState<DrawingViewer | null>(null);
  // 作るときの値と最新の onSelect。作り直さずに最新の値を使う
  const latest = useRef({ scene, theme, label, onSelect });
  latest.current = { scene, theme, label, onSelect };
  const applied = useRef<{ scene: Scene; theme: ViewerTheme | undefined } | null>(null);

  useEffect(() => {
    if (!element) return;
    const initial = latest.current;
    const instance = new DrawingViewer(element, initial.scene, {
      theme: initial.theme,
      label: initial.label,
      onSelect: (entity) => latest.current.onSelect?.(entity),
    });
    applied.current = { scene: initial.scene, theme: initial.theme };
    setViewer(instance);
    return () => {
      instance.destroy();
      setViewer(null);
    };
  }, [element]);

  useEffect(() => {
    if (!viewer) return;
    const last = applied.current;
    if (last && last.scene === scene && last.theme === theme) return;
    applied.current = { scene, theme };
    viewer.setScene(scene, theme);
  }, [viewer, scene, theme]);

  useEffect(() => {
    if (viewer && label !== undefined) viewer.setLabel(label);
  }, [viewer, label]);

  const fallback = useMemo(() => initialViewerState(scene, theme), [scene, theme]);
  const state = useSyncExternalStore(
    viewer ? viewer.subscribe : subscribeNothing,
    viewer ? viewer.getState : () => fallback,
    () => fallback,
  );
  return { ref: setElement, viewer, state };
}
