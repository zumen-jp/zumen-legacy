"use client";

import type { MarkupLayer, MarkupState } from "@zumenjs/viewer";
import { useViewerState } from "./use-viewer-state.js";

/**
 * マークアップの層の状態の一部を読む。selector の結果が isEqual（既定は Object.is）で変わったときだけ描き直す。
 * layer が null（作る前）なら undefined を返す
 */
export function useMarkupState<T>(
  layer: MarkupLayer | null | undefined,
  selector: (state: MarkupState) => T,
  isEqual: (a: T, b: T) => boolean = Object.is,
): T | undefined {
  return useViewerState(layer, selector, isEqual);
}
