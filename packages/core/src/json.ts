import type { Box, Primitive, Scene, StrokePrimitive } from "./types.js";

/**
 * Scene を JSON の文字列にする。点列（Float64Array）は数の配列にし、空の外接矩形の無限大は null になる。
 * Rust の読み取り（crates/）が返す JSON と同じ形で、sceneFromJson で Scene に戻せる。
 */
export function sceneToJson(scene: Scene, space?: number): string {
  return JSON.stringify(scene, (_key, value) => (value instanceof Float64Array ? Array.from(value) : value), space);
}

type Json<T> = T extends Float64Array
  ? number[]
  : T extends readonly (infer U)[]
    ? Json<U>[]
    : T extends object
      ? { [K in keyof T]: Json<T[K]> }
      : T;

/** sceneToJson の結果（または Rust の読み取りの JSON）を Scene に戻す */
export function sceneFromJson(json: string): Scene {
  const data = JSON.parse(json) as Json<Scene>;
  return {
    ...data,
    bounds: box(data.bounds),
    entities: data.entities.map((entity) => ({
      ...entity,
      bbox: box(entity.bbox),
      primitives: entity.primitives.map(primitive),
    })),
  } as Scene;
}

/** JSON で null になった無限大を、空の外接矩形の値に戻す */
function box(value: Json<Box>): Box {
  return {
    minX: value.minX ?? Number.POSITIVE_INFINITY,
    minY: value.minY ?? Number.POSITIVE_INFINITY,
    maxX: value.maxX ?? Number.NEGATIVE_INFINITY,
    maxY: value.maxY ?? Number.NEGATIVE_INFINITY,
  };
}

function stroke(value: Json<StrokePrimitive>): StrokePrimitive {
  return { ...value, points: Float64Array.from(value.points) };
}

function primitive(value: Json<Primitive>): Primitive {
  switch (value.kind) {
    case "stroke":
      return stroke(value);
    case "fill":
      return { ...value, rings: value.rings.map((ring) => Float64Array.from(ring)) };
    case "clipped":
      return {
        ...value,
        rings: value.rings.map((ring) => Float64Array.from(ring)),
        strokes: value.strokes.map(stroke),
      };
    default:
      return value as Primitive;
  }
}
