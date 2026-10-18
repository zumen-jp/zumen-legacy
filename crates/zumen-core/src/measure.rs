use crate::geometry::apply;
use crate::js;
use crate::types::{Placement, SceneEntity};

/// 計測の端点（用紙の mm）。吸着した要素があれば entity に持ち、実寸の換算に使う
#[derive(Debug, Clone, Copy)]
pub struct MeasurePoint<'a> {
    pub x: f64,
    pub y: f64,
    pub entity: Option<&'a SceneEntity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Distance {
    /// 用紙上の距離（mm）
    pub paper: f64,
    /// 部分図・部品・レイヤグループの縮尺から換算した実寸（mm）。決められないときは None
    pub real: Option<f64>,
    /// 実寸の換算に使った配置の名前
    pub placement: Option<String>,
}

/// 用紙の座標を、配置の元の座標（実寸）に戻す
pub fn to_real(placement: &Placement, x: f64, y: f64) -> (f64, f64) {
    apply(&placement.inverse, x, y)
}

/// 2 点の距離。部分図・部品の要素に吸着していれば、その配置の元の座標（実寸）の距離も返す。
/// 両端が縮尺の違う配置に吸着したときは、実寸を決められないので用紙上の距離だけを返す
pub fn measure_distance(a: MeasurePoint<'_>, b: MeasurePoint<'_>) -> Distance {
    let paper = js::hypot2(b.x - a.x, b.y - a.y);
    let pa = a.entity.and_then(|entity| entity.placement.as_deref());
    let pb = b.entity.and_then(|entity| entity.placement.as_deref());
    if let (Some(pa), Some(pb)) = (pa, pb)
        && ((pa.scale_x - pb.scale_x).abs() > 1e-12 || (pa.scale_y - pb.scale_y).abs() > 1e-12)
    {
        return Distance { paper, real: None, placement: None };
    }
    let Some(placement) = pa.or(pb) else {
        return Distance { paper, real: None, placement: None };
    };
    let (ax, ay) = to_real(placement, a.x, a.y);
    let (bx, by) = to_real(placement, b.x, b.y);
    Distance { paper, real: Some(js::hypot2(bx - ax, by - ay)), placement: Some(placement.name.clone()) }
}
