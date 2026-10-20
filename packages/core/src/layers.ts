import type { SceneEntity } from "./types.js";

/**
 * 図形を描くか。自分か配置元のレイヤが 1 つでも非表示なら描かない。
 * Rust の zumen_core::extract::is_entity_visible と同じ規則で、描画・選択・文字の取り出しが共有する
 */
export function isEntityVisible(entity: SceneEntity, hiddenLayers: ReadonlySet<number>): boolean {
  if (hiddenLayers.size === 0) return true;
  for (const layer of entity.layers) if (hiddenLayers.has(layer)) return false;
  return true;
}
