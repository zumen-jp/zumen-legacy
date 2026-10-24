import { emptyBox, extendBoxWithPoints, isEmptyBox, transformPoints, unionBox } from "./geometry.js";
import type { Box, Scene, Underlay } from "./types.js";

/** underlay の中身の幅と高さ（中身の座標） */
export interface UnderlaySize {
  width: number;
  height: number;
}

/**
 * 図面が中身の大きさを持たない underlay（width と height が 0。DXF・DWG の PDF の underlay、docs/decisions/0034）を、
 * アプリケーションが知った大きさで置く。置く前の matrix は高さ 0 の中身のもの（中身の左下の角が原点）で、
 * size が大きさを返した underlay の width・height を決め、matrix を中身の高さだけ中身の Y の向きに動かし、clip の
 * 空の輪を中身の外形にし、bbox を clip の外接矩形の中の中身の外接矩形にして、Scene.bounds を広げる。中身が clip の
 * 外にある underlay は除く。ほかの underlay はそのまま（同じオブジェクト）。置いたものがなければ同じ scene を返す
 */
export function placeUnderlays(scene: Scene, size: (underlay: Underlay) => UnderlaySize | null | undefined): Scene {
  const underlays = scene.underlays;
  if (!underlays) return scene;
  let changed = false;
  const bounds = { ...scene.bounds };
  const out: Underlay[] = [];
  for (const underlay of underlays) {
    const known = underlay.width === 0 && underlay.height === 0 ? size(underlay) : null;
    if (!known || !(known.width > 0 && known.height > 0 && known.width < Infinity && known.height < Infinity)) {
      out.push(underlay);
      continue;
    }
    changed = true;
    const placed = place(underlay, known.width, known.height);
    if (!placed) continue;
    out.push(placed);
    unionBox(bounds, placed.bbox);
  }
  return changed ? { ...scene, bounds, underlays: out } : scene;
}

function place(underlay: Underlay, width: number, height: number): Underlay | null {
  const [a, b, c, d, e, f] = underlay.matrix;
  const matrix = [a, b, c, d, e - c * height, f - d * height] as const;
  const outline = transformPoints(matrix, [0, 0, width, 0, width, height, 0, height]);
  const placed: Underlay = { ...underlay, width, height, matrix, bbox: ringsBox([outline]) };
  if (!underlay.clip) return placed;
  const clip = underlay.clip.map((ring) => (ring.length === 0 ? outline : ring));
  const bbox = intersection(placed.bbox, ringsBox(clip));
  if (isEmptyBox(bbox)) return null;
  return { ...placed, clip, bbox };
}

function ringsBox(rings: readonly Float64Array[]): Box {
  const box = emptyBox();
  for (const ring of rings) extendBoxWithPoints(box, ring);
  return box;
}

function intersection(a: Box, b: Box): Box {
  return {
    minX: Math.max(a.minX, b.minX),
    minY: Math.max(a.minY, b.minY),
    maxX: Math.min(a.maxX, b.maxX),
    maxY: Math.min(a.maxY, b.maxY),
  };
}
