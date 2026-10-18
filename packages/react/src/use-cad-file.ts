"use client";

import { type DrawingReader, readDrawing, type Scene } from "@zumenjs/core";
import { useEffect, useRef, useState } from "react";

export interface CadFileState {
  scene: Scene | null;
  error: Error | null;
  loading: boolean;
}

export interface CadFileOptions {
  /** 描く表示（Scene.views の id）。DXF のレイアウトなど。省略すると形式ごとの既定の表示 */
  view?: string;
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

/**
 * File・Blob を、渡した形式の読み取り（@zumenjs/sxf の sxf、@zumenjs/jww の jww など）で読み、表示用の図面にする。
 * 読み込み中に別のファイルへ変えたら、前の結果は捨てる。readers は読み始めたときの値を使う。
 * options.view を変えると、ファイルを読み直さずに、その表示の図面にする。
 */
export function useCadFile(
  file: Blob | null,
  readers: readonly DrawingReader[],
  options: CadFileOptions = {},
): CadFileState {
  const { view } = options;
  const [loaded, setLoaded] = useState<{ file: Blob; bytes: Uint8Array | null; error: Error | null } | null>(null);
  const [state, setState] = useState<CadFileState>({ scene: null, error: null, loading: false });
  const readersRef = useRef(readers);
  readersRef.current = readers;
  useEffect(() => {
    if (!file) {
      setLoaded(null);
      setState({ scene: null, error: null, loading: false });
      return;
    }
    let cancelled = false;
    setLoaded(null);
    setState({ scene: null, error: null, loading: true });
    file
      .arrayBuffer()
      .then((buffer) => {
        if (!cancelled) setLoaded({ file, bytes: new Uint8Array(buffer), error: null });
      })
      .catch((error: unknown) => {
        if (!cancelled) setLoaded({ file, bytes: null, error: toError(error) });
      });
    return () => {
      cancelled = true;
    };
  }, [file]);
  useEffect(() => {
    if (!loaded) return;
    if (!loaded.bytes) {
      setState({ scene: null, error: loaded.error, loading: false });
      return;
    }
    try {
      const scene = readDrawing(loaded.bytes, readersRef.current, view === undefined ? {} : { view });
      setState({ scene, error: null, loading: false });
    } catch (error) {
      setState({ scene: null, error: toError(error), loading: false });
    }
  }, [loaded, view]);
  return state;
}
