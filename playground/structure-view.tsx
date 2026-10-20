import { useStbFile } from "@zumenjs/stb/react";
import { useEffect } from "react";
import { StructureSample } from "./structure-sample.js";

export interface StructureStatus {
  loading: boolean;
  error: Error | null;
}

/**
 * ST-Bridge の構造モデルを 3D で表示する。three.js を含むので、試用画面は構造モデルを開いたときだけ読み込む。
 * 読み込みの状態は onStatus で見出しへ返す
 */
export function StructureView({ file, onStatus }: { file: File; onStatus: (status: StructureStatus) => void }) {
  const { model, error, loading } = useStbFile(file);
  useEffect(() => onStatus({ loading, error }), [loading, error, onStatus]);
  return model ? <StructureSample model={model} label={file.name} /> : null;
}
