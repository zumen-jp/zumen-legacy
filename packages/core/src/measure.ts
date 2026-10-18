import { apply } from "./geometry.js";
import type { Placement, SceneEntity } from "./types.js";

/** 計測の端点（用紙の mm）。吸着した要素があれば entity に持ち、実寸の換算に使う */
export interface MeasurePoint {
  x: number;
  y: number;
  entity?: SceneEntity;
}

export interface Distance {
  /** 用紙上の距離（mm） */
  paper: number;
  /** 部分図・部品・レイヤグループの縮尺から換算した実寸（mm）。決められないときは null */
  real: number | null;
  /** 実寸の換算に使った配置の名前 */
  placement: string | null;
}

/** 用紙の座標を、配置の元の座標（実寸）に戻す */
export function toReal(placement: Placement, x: number, y: number): [number, number] {
  return apply(placement.inverse, x, y);
}

/**
 * 2 点の距離。部分図・部品の要素に吸着していれば、その配置の元の座標（実寸）の距離も返す。
 * 両端が縮尺の違う配置に吸着したときは、実寸を決められないので用紙上の距離だけを返す。
 */
export function measureDistance(a: MeasurePoint, b: MeasurePoint): Distance {
  const paper = Math.hypot(b.x - a.x, b.y - a.y);
  const pa = a.entity?.placement;
  const pb = b.entity?.placement;
  if (pa && pb && (Math.abs(pa.scaleX - pb.scaleX) > 1e-12 || Math.abs(pa.scaleY - pb.scaleY) > 1e-12)) {
    return { paper, real: null, placement: null };
  }
  const placement = pa ?? pb;
  if (!placement) return { paper, real: null, placement: null };
  const [ax, ay] = toReal(placement, a.x, a.y);
  const [bx, by] = toReal(placement, b.x, b.y);
  return { paper, real: Math.hypot(bx - ax, by - ay), placement: placement.name };
}
