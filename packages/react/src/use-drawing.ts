"use client";

import { type DrawingReader, detectFormat, type ReadDrawingOptions, readDrawing, type Scene } from "@zumenjs/core";
import { useEffect, useRef, useState } from "react";

export interface DrawingState {
  scene: Scene | null;
  error: Error | null;
  loading: boolean;
}

export interface UseDrawingOptions {
  /** 描く表示（Scene.views の id）。DXF のレイアウトなど。省略すると形式ごとの既定の表示 */
  view?: string;
}

function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

/**
 * File・Blob を、渡した形式の読み取り（@zumenjs/sxf の sxf、@zumenjs/dwg の dwg など）で読み、表示用の図面にする。
 * 中身を認めた読み取りに load があれば（DWG の WebAssembly など）、それを待ってから読む。ほかの読み取りの load は呼ばない。
 * 読み込み中に別のファイルへ変えたら、前の結果は捨てる。readers は読み始めたときの値を使う。
 * options.view を変えると、ファイルを読み直さずに、その表示の図面にする
 */
export function useDrawing(
  file: Blob | null,
  readers: readonly DrawingReader[],
  options: UseDrawingOptions = {},
): DrawingState {
  const { view } = options;
  const [loaded, setLoaded] = useState<{ file: Blob; bytes: Uint8Array | null; error: Error | null } | null>(null);
  const [state, setState] = useState<DrawingState>({ scene: null, error: null, loading: false });
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
    const bytes = loaded.bytes;
    if (!bytes) {
      setState({ scene: null, error: loaded.error, loading: false });
      return;
    }
    let cancelled = false;
    const readers = readersRef.current;
    const format = detectFormat(bytes, readers);
    const reader = format === null ? undefined : readers.find((candidate) => candidate.formats.includes(format));
    const read = () => {
      if (cancelled) return;
      const options: ReadDrawingOptions = {};
      if (format !== null) options.format = format;
      if (view !== undefined) options.view = view;
      try {
        setState({ scene: readDrawing(bytes, readers, options), error: null, loading: false });
      } catch (error) {
        setState({ scene: null, error: toError(error), loading: false });
      }
    };
    if (!reader?.load) read();
    else {
      reader.load().then(read, (error: unknown) => {
        if (!cancelled) setState({ scene: null, error: toError(error), loading: false });
      });
    }
    return () => {
      cancelled = true;
    };
  }, [loaded, view]);
  return state;
}
