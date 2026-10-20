"use client";

import { useRef, useSyncExternalStore } from "react";

/** getState と subscribe を持つ状態（StructureViewer など） */
export interface StructureStore<S> {
  getState(): S;
  subscribe(listener: () => void): () => void;
}

const subscribeNothing = () => () => {};

/**
 * エンジンの状態の一部を読む。selector の結果が isEqual（既定は Object.is）で変わったときだけ描き直す。
 * viewer が null（要素が付く前）なら undefined を返す。
 * selector が毎回新しいオブジェクトを返すときは、isEqual に浅い比較などを渡す
 */
export function useStructureState<S, T>(
  viewer: StructureStore<S> | null | undefined,
  selector: (state: S) => T,
  isEqual: (a: T, b: T) => boolean = Object.is,
): T | undefined {
  const memo = useRef<{ viewer: StructureStore<S>; state: S; selector: (state: S) => T; value: T } | null>(null);
  const getSnapshot = (): T | undefined => {
    if (!viewer) return undefined;
    const state = viewer.getState();
    const last = memo.current;
    if (last && last.viewer === viewer && last.state === state && last.selector === selector) return last.value;
    const next = selector(state);
    // 同じ値なら前のオブジェクトを返し、描き直さない
    const value = last && last.viewer === viewer && isEqual(last.value, next) ? last.value : next;
    memo.current = { viewer, state, selector, value };
    return value;
  };
  // subscribe は StructureViewer のプロパティで、同じ関数のまま変わらない
  return useSyncExternalStore(viewer ? viewer.subscribe : subscribeNothing, getSnapshot, getSnapshot);
}
