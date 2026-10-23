import type { Scene, Underlay } from "@zumenjs/core";
import { useDrawing } from "@zumenjs/react";
import { reader } from "@zumenjs/reader";
import { imageUnderlayRenderer } from "@zumenjs/viewer";
import { type DragEvent, StrictMode, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { DrawingSample } from "./drawing-sample.js";

const ACCEPT = ".sfc,.p21,.sfz,.p2z,.jww,.jwc,.jws,.dxf,.dwg";
const READERS = [reader];
const UNDERLAY_ID = "playground-image";

/** 下敷きにする画像 */
interface UnderlayImage {
  name: string;
  bitmap: ImageBitmap;
}

/** 図面が書くファイルの名前（.\image.JPG、C:\scans\a.tif など）から、フォルダを除いた名前を小文字で */
function baseName(path: string): string {
  return (path.split(/[\\/]/).pop() ?? path).toLowerCase();
}

/** 画像を、用紙（なければ図面の範囲）の中央に縦横比を保って収める underlay にし、図面の下に敷く */
function withUnderlay(scene: Scene, image: UnderlayImage): Scene {
  const box = scene.sheet ? { minX: 0, minY: 0, maxX: scene.sheet.width, maxY: scene.sheet.height } : scene.bounds;
  const { width, height } = image.bitmap;
  if (!(box.minX < box.maxX && box.minY < box.maxY && width > 0 && height > 0)) return scene;
  const scale = Math.min((box.maxX - box.minX) / width, (box.maxY - box.minY) / height);
  const x = (box.minX + box.maxX - width * scale) / 2;
  const y = (box.minY + box.maxY - height * scale) / 2;
  const bbox = { minX: x, minY: y, maxX: x + width * scale, maxY: y + height * scale };
  // 中身の座標は左上が原点で下向きなので、上下を反転して用紙に写す
  const underlay: Underlay = {
    id: UNDERLAY_ID,
    kind: "image",
    file: image.name,
    width,
    height,
    matrix: [scale, 0, 0, -scale, x, bbox.maxY],
    opacity: 0.6,
    layers: [],
    bbox,
  };
  const bounds = {
    minX: Math.min(scene.bounds.minX, bbox.minX),
    minY: Math.min(scene.bounds.minY, bbox.minY),
    maxX: Math.max(scene.bounds.maxX, bbox.maxX),
    maxY: Math.max(scene.bounds.maxY, bbox.maxY),
  };
  return { ...scene, bounds, underlays: [...(scene.underlays ?? []), underlay] };
}

/** 図面を選ぶ・ドロップする・?sample=名前 で開く試用画面 */
function App() {
  const [file, setFile] = useState<File | null>(null);
  // DXF のモデル空間・レイアウトなど、図面の中の表示。ファイルを変えたら既定の表示に戻す
  const [view, setView] = useState<string | undefined>(
    () => new URLSearchParams(window.location.search).get("view") ?? undefined,
  );
  const [over, setOver] = useState(false);
  // useDrawing が初めて図面を開いたときに reader.load で WebAssembly を読み込む。
  const drawing = useDrawing(file, READERS, { view });
  const { error, loading } = drawing;
  const open = (next: File | null) => {
    setView(undefined);
    setFile(next);
  };
  // 図面の下に敷く画像と、図面の画像（DXF の IMAGE など）が指すファイル（フォルダを除いた名前で引く）
  const [image, setImage] = useState<UnderlayImage | null>(null);
  const [imageError, setImageError] = useState<string | null>(null);
  const [imageFiles, setImageFiles] = useState<ReadonlyMap<string, File>>(new Map());
  const scene = useMemo(
    () => (drawing.scene && image ? withUnderlay(drawing.scene, image) : drawing.scene),
    [drawing.scene, image],
  );
  // 図面の画像が指すファイルの名前（重なりを除く）
  const wanted = useMemo(() => {
    const names = new Set<string>();
    for (const underlay of drawing.scene?.underlays ?? []) if (underlay.file) names.add(underlay.file);
    return [...names];
  }, [drawing.scene]);
  const underlayRenderer = useMemo(
    () =>
      image || wanted.length > 0
        ? imageUnderlayRenderer((underlay) => {
            if (underlay.id === UNDERLAY_ID) return image?.bitmap ?? null;
            const found = underlay.file === undefined ? undefined : imageFiles.get(baseName(underlay.file));
            return found ? createImageBitmap(found) : null;
          })
        : undefined,
    [image, imageFiles, wanted.length],
  );
  const found = wanted.filter((name) => imageFiles.has(baseName(name))).length;
  const openImage = (next: File | null) => {
    setImageError(null);
    if (!next) {
      setImage(null);
      return;
    }
    createImageBitmap(next)
      .then((bitmap) => setImage({ name: next.name, bitmap }))
      .catch(() => setImageError(`${next.name} を画像として読めません`));
  };

  useEffect(() => {
    const sample = new URLSearchParams(window.location.search).get("sample");
    if (!sample) return;
    fetch(`/samples/${encodeURIComponent(sample)}`)
      .then((response) => {
        if (!response.ok) throw new Error(`${sample} を取得できません（${response.status}）`);
        return response.blob();
      })
      .then((blob) => setFile(new File([blob], sample)))
      .catch((reason: unknown) => console.error(reason));
  }, []);

  const onDrop = (event: DragEvent<HTMLElement>) => {
    event.preventDefault();
    setOver(false);
    const dropped = event.dataTransfer.files[0];
    if (dropped) open(dropped);
  };

  return (
    <main
      style={{
        display: "grid",
        gridTemplateRows: "auto minmax(0, 1fr)",
        height: "100%",
        gap: 8,
        padding: 8,
        boxSizing: "border-box",
      }}
      onDragOver={(event) => {
        event.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={onDrop}
    >
      <header style={{ display: "flex", flexWrap: "wrap", alignItems: "center", gap: 12 }}>
        <h1 style={{ margin: 0, fontSize: 18 }}>図面のビューア</h1>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          図面（SFC・P21・P2Z・SFZ・JWW・JWC・JWS・DXF・DWG）
          <input type="file" accept={ACCEPT} onChange={(event) => open(event.currentTarget.files?.[0] ?? null)} />
        </label>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          下敷きの画像（PNG・JPEG）
          <input type="file" accept="image/*" onChange={(event) => openImage(event.currentTarget.files?.[0] ?? null)} />
        </label>
        {wanted.length > 0 ? (
          <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
            図面の画像のファイル（{found.toLocaleString("ja-JP")} / {wanted.length.toLocaleString("ja-JP")} 件）
            <input
              type="file"
              accept="image/*"
              multiple
              title={wanted.join("\n")}
              onChange={(event) =>
                setImageFiles(
                  new Map([...(event.currentTarget.files ?? [])].map((file) => [baseName(file.name), file])),
                )
              }
            />
          </label>
        ) : null}
        {imageError ? <span role="alert">{imageError}</span> : null}
        <span aria-live="polite">
          {loading
            ? "読み込み中…"
            : error
              ? `読み込めませんでした: ${error.message}`
              : file
                ? file.name
                : "ファイルを選ぶか、ここへドロップしてください"}
        </span>
      </header>
      <div style={{ minHeight: 0, outline: over ? "3px dashed #0031d8" : undefined }}>
        {scene ? (
          <DrawingSample scene={scene} label={file?.name} onViewChange={setView} underlayRenderer={underlayRenderer} />
        ) : null}
      </div>
    </main>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
