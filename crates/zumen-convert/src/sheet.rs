//! 用紙と縮尺（決定 0008）。縮尺のある図形（SXF の部分図、JWW のレイヤグループ）はモデル空間に実寸で置き、
//! 用紙のレイアウトのビューポートで、元の用紙と同じ位置・縮尺・向きに見せる。
//! 縮尺の違うまとまりがモデル空間で重なれば、ビューポートに別のまとまりが映らないよう、後のまとまりをずらす。

use crate::builder::{Builder, LINEWEIGHT_BY_LAYER, Look, Space, VIEWPORT_LAYER};
use zumen_core::geometry::{empty_box, extend_box, is_empty_box, union_box};
use zumen_core::{BBox, Scene};
use zumen_dxf::{DxfEntityData, DxfViewport};

/// ビューポートが図形を切り取らないよう、描いた範囲の外に取る余白（用紙の mm）
const MARGIN: f64 = 1.0;

/// ビューポートの 90（DXF Reference「VIEWPORT」）: 32768 は常に立つ値、16384 は表示の固定（縮尺を変えない）
const VIEWPORT_FLAGS: f64 = 49152.0;

/// 1 つのビューポート。モデルの座標（ずらす前）から用紙へ: 用紙 = origin + scale · R(angle) · モデル
#[derive(Debug, Clone)]
pub(crate) struct View {
    /// モデル空間の図形のまとまりの番号（同じ部分図を 2 か所に置けば、2 つのビューポートが同じまとまりを見る）
    pub content: usize,
    pub scale: f64,
    /// 回転（度、反時計回り）。VIEWPORT のねじれの角度
    pub angle: f64,
    pub origin: [f64; 2],
    /// 用紙の上で描く範囲（元の図面で描いた図形の外接矩形に余白を足したもの）
    pub rect: BBox,
}

/// モデル空間の図形のまとまり
#[derive(Debug, Clone, Default)]
pub(crate) struct Content {
    /// 元の図面で描いた図形の数。重なるときは、多いまとまりを元の座標に残す
    pub weight: usize,
    /// ずらした量（モデルの座標）
    pub offset: [f64; 2],
}

fn rotate(angle: f64, [x, y]: [f64; 2]) -> [f64; 2] {
    let (sin, cos) = angle.to_radians().sin_cos();
    [cos * x - sin * y, sin * x + cos * y]
}

/// ビューポートに映るモデルの範囲（ずらした後）
pub(crate) fn region(view: &View, offset: [f64; 2]) -> BBox {
    let mut out = empty_box();
    let r = &view.rect;
    for [x, y] in [[r.min_x, r.min_y], [r.max_x, r.min_y], [r.max_x, r.max_y], [r.min_x, r.max_y]] {
        let [mx, my] = rotate(-view.angle, [(x - view.origin[0]) / view.scale, (y - view.origin[1]) / view.scale]);
        extend_box(&mut out, mx + offset[0], my + offset[1]);
    }
    out
}

fn overlaps(a: &BBox, b: &BBox) -> bool {
    a.min_x <= b.max_x && b.min_x <= a.max_x && a.min_y <= b.max_y && b.min_y <= a.max_y
}

/// きりのよい刻み（10 の累乗）
fn step(size: f64) -> f64 {
    if size.is_finite() && size > 1.0 { 10f64.powf(size.log10().floor()) } else { 1.0 }
}

/// 図形の外接矩形を、余白を足したビューポートの範囲にする
pub(crate) fn rect_of(bbox: &BBox) -> Option<BBox> {
    if is_empty_box(bbox) {
        return None;
    }
    let margin = MARGIN.max((bbox.max_x - bbox.min_x).max(bbox.max_y - bbox.min_y) * 0.005);
    Some(BBox {
        min_x: bbox.min_x - margin,
        min_y: bbox.min_y - margin,
        max_x: bbox.max_x + margin,
        max_y: bbox.max_y + margin,
    })
}

/// 元の図面の図形の外接矩形を、id の先頭（# から > まで）ごとにまとめる
pub(crate) fn boxes_by_top_id(scene: &Scene) -> std::collections::HashMap<String, (BBox, usize)> {
    let mut out: std::collections::HashMap<String, (BBox, usize)> = std::collections::HashMap::new();
    for entity in &scene.entities {
        let top = entity.id.split('>').next().unwrap_or("").to_string();
        let slot = out.entry(top).or_insert_with(|| (empty_box(), 0));
        union_box(&mut slot.0, &entity.bbox);
        slot.1 += 1;
    }
    out
}

/// まとまりがモデル空間で重ならないよう、ずらす量を決める。重なったまとまりは、それまでのまとまりの右へ、
/// 下端をそろえて置く。ずらしたまとまりの数を返す
pub(crate) fn separate(views: &[View], contents: &mut [Content]) -> u64 {
    let mut order: Vec<usize> = (0..contents.len()).collect();
    order.sort_by_key(|index| std::cmp::Reverse(contents[*index].weight));
    let mut placed: Vec<BBox> = Vec::new();
    let mut moved = 0;
    for index in order {
        let mut area = empty_box();
        for view in views.iter().filter(|view| view.content == index) {
            union_box(&mut area, &region(view, [0.0, 0.0]));
        }
        if is_empty_box(&area) {
            continue;
        }
        if placed.iter().any(|other| overlaps(other, &area)) {
            let mut all = empty_box();
            for other in &placed {
                union_box(&mut all, other);
            }
            let size = (area.max_x - area.min_x).max(area.max_y - area.min_y);
            let unit = step(size * 0.1);
            let dx = ((all.max_x + size * 0.1 - area.min_x) / unit).ceil() * unit;
            let dy = ((all.min_y - area.min_y) / unit).round() * unit;
            contents[index].offset = [dx, dy];
            area = BBox { min_x: area.min_x + dx, min_y: area.min_y + dy, max_x: area.max_x + dx, max_y: area.max_y + dy };
            moved += 1;
        }
        placed.push(area);
    }
    moved
}

/// VIEWPORT の値。ezdxf の Viewport.get_transformation_matrix（zumen-dxf の表示）と同じく、
/// 用紙 = 中心 − 縮尺 × 表示の中心 + 縮尺 × R(ねじれ) × (モデル − 目標点)
pub(crate) fn viewport(view: &View, offset: [f64; 2], id: f64) -> DxfViewport {
    let r = &view.rect;
    let center = [(r.min_x + r.max_x) / 2.0, (r.min_y + r.max_y) / 2.0];
    let width = r.max_x - r.min_x;
    let height = r.max_y - r.min_y;
    // 用紙 = origin + scale · R · (モデル − offset) なので、表示の中心（DCS）= (中心 − origin) / scale + R · offset
    let turned = rotate(view.angle, offset);
    let view_center =
        [(center[0] - view.origin[0]) / view.scale + turned[0], (center[1] - view.origin[1]) / view.scale + turned[1]];
    DxfViewport {
        center: [center[0], center[1], 0.0],
        width,
        height,
        status: id,
        id,
        view_center,
        view_target: [0.0; 3],
        view_direction: [0.0, 0.0, 1.0],
        view_height: height / view.scale,
        view_twist_angle: crate::text::normalize(view.angle),
        status_flags: VIEWPORT_FLAGS,
        frozen_layer_handles: Vec::new(),
        clipping_boundary_handle: None,
    }
}

/// 紙空間の全体のビューポート（ID 1）。AutoCAD はレイアウトごとに 1 つ持ち、表示はモデルの表示に使わない
pub(crate) fn overall_viewport(builder: &mut Builder, width: f64, height: f64) {
    let look = Look {
        layer: VIEWPORT_LAYER.into(),
        color: crate::builder::BY_LAYER,
        linetype: "ByLayer".into(),
        lineweight: LINEWEIGHT_BY_LAYER,
        linetype_scale: 1.0,
    };
    let data = DxfEntityData::Viewport(DxfViewport {
        center: [width / 2.0, height / 2.0, 0.0],
        width,
        height,
        status: 1.0,
        id: 1.0,
        view_center: [width / 2.0, height / 2.0],
        view_target: [0.0; 3],
        view_direction: [0.0, 0.0, 1.0],
        view_height: height,
        view_twist_angle: 0.0,
        status_flags: 32768.0,
        frozen_layer_handles: Vec::new(),
        clipping_boundary_handle: None,
    });
    builder.add(&Space::Paper, &look, "VIEWPORT", data, "");
}
