"use client";

import { useEffect, useState } from "react";
import { readStb } from "../read.js";
import type { StructureModel } from "../types.js";

export interface StbFileState {
  model: StructureModel | null;
  error: Error | null;
  loading: boolean;
}

/** File・Blob を読み、表示用のモデルにする。読み込み中に別のファイルへ変えたら、前の結果は捨てる */
export function useStbFile(file: Blob | null): StbFileState {
  const [state, setState] = useState<StbFileState>({ model: null, error: null, loading: false });
  useEffect(() => {
    if (!file) {
      setState({ model: null, error: null, loading: false });
      return;
    }
    let cancelled = false;
    setState({ model: null, error: null, loading: true });
    file
      .arrayBuffer()
      .then((buffer) => {
        if (cancelled) return;
        setState({ model: readStb(buffer), error: null, loading: false });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setState({ model: null, error: error instanceof Error ? error : new Error(String(error)), loading: false });
      });
    return () => {
      cancelled = true;
    };
  }, [file]);
  return state;
}
