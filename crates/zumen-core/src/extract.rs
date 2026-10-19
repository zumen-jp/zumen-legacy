//! 図面（`Scene`）から図形と文字を取り出す。サーバーや AI が、描いた画像と一緒に図面の中身を読むために使う。
//! 座標はすべて用紙の mm（Y 軸は上向き）。

use crate::geometry::{apply, apply_vector, empty_box, extend_box, is_empty_box, rotation_deg};
use crate::js;
use crate::types::{BBox, Primitive, Scene, SceneEntity, TextPrimitive};
use std::collections::HashSet;

/// 取り出す図形の条件
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntityFilter {
    /// 外接矩形がこの範囲（用紙の mm）と重なる図形だけを選ぶ。None なら図面全体
    pub region: Option<BBox>,
    /// 描かないレイヤの id。None なら図面の表示指定（`SceneLayer::visible`）に従う
    pub hidden_layers: Option<HashSet<u32>>,
}

/// TypeScript の isEntityVisible と同じ: 自分か配置元のレイヤが 1 つでも非表示なら描かない
pub fn is_entity_visible(entity: &SceneEntity, hidden: &HashSet<u32>) -> bool {
    hidden.is_empty() || !entity.layers.iter().any(|layer| hidden.contains(layer))
}

/// 描かないレイヤ。hidden が None なら図面の表示指定に従う
pub fn hidden_layers(scene: &Scene, hidden: Option<&HashSet<u32>>) -> HashSet<u32> {
    match hidden {
        Some(hidden) => hidden.clone(),
        None => scene.layers.iter().filter(|layer| !layer.visible).map(|layer| layer.id).collect(),
    }
}

/// 2 つの矩形が重なるか。辺が接するだけでも重なるとみなす
pub fn intersects(a: &BBox, b: &BBox) -> bool {
    !(a.max_x < b.min_x || a.min_x > b.max_x || a.max_y < b.min_y || a.min_y > b.max_y)
}

/// 条件に合う図形を、描く順に返す
pub fn select_entities<'a>(scene: &'a Scene, filter: &EntityFilter) -> Vec<&'a SceneEntity> {
    let hidden = hidden_layers(scene, filter.hidden_layers.as_ref());
    scene
        .entities
        .iter()
        .filter(|entity| is_entity_visible(entity, &hidden))
        .filter(|entity| filter.region.as_ref().is_none_or(|region| intersects(&entity.bbox, region)))
        .collect()
}

/// 図形のレイヤの名前。自分のレイヤと配置元のレイヤを、図形が持つ順に並べる
pub fn layer_names<'a>(scene: &'a Scene, entity: &SceneEntity) -> Vec<&'a str> {
    entity
        .layers
        .iter()
        .filter_map(|id| scene.layers.iter().find(|layer| layer.id == *id))
        .map(|layer| layer.name.as_str())
        .collect()
}

/// 図形の文字
#[derive(Debug, Clone, PartialEq)]
pub struct EntityText {
    /// 文字列。同じ行に並ぶ部分（書式の違う部分など）はつなげ、行が変わるところに改行を入れる
    pub text: String,
    /// 最初の部分の文字範囲の左下（文字の基準点）
    pub x: f64,
    pub y: f64,
    /// 文字の高さ（用紙の mm）。高さの違う部分があれば最も高いもの
    pub height: f64,
    /// 最初の部分の文字の向き（度。X 軸から反時計回りで、-180 より大きく 180 以下）
    pub angle: f64,
    /// 文字範囲の外接矩形（用紙の mm）
    pub bbox: BBox,
}

/// 図形の文字。文字を持たない図形と、空白だけの図形は None
pub fn entity_text(entity: &SceneEntity) -> Option<EntityText> {
    let parts: Vec<&TextPrimitive> = entity
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            Primitive::Text(text) if !text.text.trim().is_empty() => Some(text),
            _ => None,
        })
        .collect();
    let first = *parts.first()?;
    let mut text = String::new();
    let mut height: f64 = 0.0;
    let mut bbox = empty_box();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 && !same_line(parts[index - 1], part) {
            text.push('\n');
        }
        text.push_str(&part.text);
        let (cx, cy) = apply_vector(&part.matrix, 0.0, 1.0);
        height = height.max(part.height * js::hypot2(cx, cy));
        for (x, y) in [(0.0, 0.0), (part.box_width, 0.0), (part.box_width, part.box_height), (0.0, part.box_height)] {
            let (px, py) = apply(&part.matrix, x, y);
            extend_box(&mut bbox, px, py);
        }
    }
    if is_empty_box(&bbox) {
        let (x, y) = apply(&first.matrix, 0.0, 0.0);
        bbox = BBox { min_x: x, min_y: y, max_x: x, max_y: y };
    }
    let (x, y) = apply(&first.matrix, 0.0, 0.0);
    Some(EntityText { text, x, y, height, angle: rotation_deg(&first.matrix), bbox })
}

/// b が a と同じ行に並ぶか。向きが同じで、b の基準点が a の基準線から a の文字の高さの半分以内にあれば同じ行
fn same_line(a: &TextPrimitive, b: &TextPrimitive) -> bool {
    let m = &a.matrix;
    let det = m[0] * m[3] - m[1] * m[2];
    if !det.is_finite() || det.abs() < 1e-300 {
        return false;
    }
    let ax = js::hypot2(m[0], m[1]);
    let bx = js::hypot2(b.matrix[0], b.matrix[1]);
    if ax == 0.0 || bx == 0.0 {
        return false;
    }
    // 文字の進む向きのずれ（外積）が小さいか
    let cross = (m[0] * b.matrix[1] - m[1] * b.matrix[0]) / (ax * bx);
    let dot = m[0] * b.matrix[0] + m[1] * b.matrix[1];
    if cross.abs() > 1e-6 || dot <= 0.0 {
        return false;
    }
    // b の基準点を a の文字の座標に戻し、上下のずれを見る
    let dx = b.matrix[4] - m[4];
    let dy = b.matrix[5] - m[5];
    let v = (-m[1] * dx + m[0] * dy) / det;
    v.abs() <= a.height * 0.5
}

/// 文字を持つ図形と、その文字（描く順）
pub fn scene_texts<'a>(scene: &'a Scene, filter: &EntityFilter) -> Vec<(&'a SceneEntity, EntityText)> {
    select_entities(scene, filter).into_iter().filter_map(|entity| Some((entity, entity_text(entity)?))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::placement;
    use crate::types::{Background, SceneLayer, StrokePrimitive};

    fn text(content: &str, matrix: [f64; 6], height: f64) -> Primitive {
        Primitive::Text(TextPrimitive {
            text: content.into(),
            color: "#000000".into(),
            font_family: "sans-serif".into(),
            height,
            scale_x: 1.0,
            cells: Vec::new(),
            box_width: content.chars().count() as f64 * height,
            box_height: height,
            matrix,
        })
    }

    fn entity(id: &str, layers: Vec<u32>, primitives: Vec<Primitive>, bbox: BBox) -> SceneEntity {
        SceneEntity {
            id: id.into(),
            kind: "文字".into(),
            source: "test".into(),
            layers,
            primitives,
            bbox,
            properties: Vec::new(),
            placement: None,
        }
    }

    fn scene(entities: Vec<SceneEntity>) -> Scene {
        Scene {
            format: "test".into(),
            version: "1".into(),
            views: None,
            view: None,
            sheet: None,
            bounds: BBox { min_x: 0.0, min_y: 0.0, max_x: 100.0, max_y: 100.0 },
            layers: vec![
                SceneLayer { id: 0, name: "表示".into(), visible: true, group: None },
                SceneLayer { id: 1, name: "非表示".into(), visible: false, group: None },
            ],
            entities,
            notices: Vec::new(),
            metadata: Vec::new(),
            background: Background::Light,
        }
    }

    fn square(x: f64, y: f64, size: f64) -> BBox {
        BBox { min_x: x, min_y: y, max_x: x + size, max_y: y + size }
    }

    #[test]
    fn joins_parts_on_one_line_and_breaks_between_lines() {
        let e = entity(
            "1",
            vec![0],
            vec![
                text("赤い", placement(10.0, 50.0, 0.0, 1.0, 1.0), 5.0),
                text("文字", placement(20.0, 50.0, 0.0, 1.0, 1.0), 5.0),
                text("次の行", placement(10.0, 42.0, 0.0, 1.0, 1.0), 5.0),
                text(" ", placement(10.0, 34.0, 0.0, 1.0, 1.0), 5.0),
            ],
            square(10.0, 42.0, 20.0),
        );
        let found = entity_text(&e).unwrap();
        assert_eq!(found.text, "赤い文字\n次の行");
        assert_eq!((found.x, found.y, found.height, found.angle), (10.0, 50.0, 5.0, 0.0));
        assert_eq!(found.bbox, BBox { min_x: 10.0, min_y: 42.0, max_x: 30.0, max_y: 55.0 });
    }

    #[test]
    fn reads_the_height_and_angle_of_rotated_and_scaled_text() {
        // 文字の座標の 1 mm が用紙の 0.5 mm になる、90 度回した文字
        let e =
            entity("1", vec![0], vec![text("AB", placement(5.0, 5.0, 90.0, 0.5, 0.5), 4.0)], square(0.0, 0.0, 10.0));
        let found = entity_text(&e).unwrap();
        assert_eq!(found.text, "AB");
        assert!((found.height - 2.0).abs() < 1e-12);
        assert!((found.angle - 90.0).abs() < 1e-12);
        assert!((found.bbox.min_x - 3.0).abs() < 1e-12 && (found.bbox.max_x - 5.0).abs() < 1e-12);
        assert!((found.bbox.min_y - 5.0).abs() < 1e-12 && (found.bbox.max_y - 9.0).abs() < 1e-12);
        // 向きの違う部分は別の行
        let two = entity(
            "2",
            vec![0],
            vec![
                text("A", placement(0.0, 0.0, 0.0, 1.0, 1.0), 4.0),
                text("B", placement(4.0, 0.0, 90.0, 1.0, 1.0), 4.0),
            ],
            square(0.0, 0.0, 10.0),
        );
        assert_eq!(entity_text(&two).unwrap().text, "A\nB");
    }

    #[test]
    fn skips_entities_without_text() {
        let line = entity(
            "1",
            vec![0],
            vec![Primitive::Stroke(StrokePrimitive {
                points: vec![0.0, 0.0, 1.0, 1.0],
                closed: false,
                segments: None,
                color: "#000000".into(),
                width: 0.0,
                dashes: Vec::new(),
            })],
            square(0.0, 0.0, 1.0),
        );
        assert_eq!(entity_text(&line), None);
        let blank =
            entity("2", vec![0], vec![text("　 ", placement(0.0, 0.0, 0.0, 1.0, 1.0), 4.0)], square(0.0, 0.0, 1.0));
        assert_eq!(entity_text(&blank), None);
    }

    #[test]
    fn selects_by_region_and_layer_visibility() {
        let s = scene(vec![
            entity("a", vec![0], vec![text("a", placement(0.0, 0.0, 0.0, 1.0, 1.0), 1.0)], square(0.0, 0.0, 1.0)),
            entity("b", vec![0, 1], vec![text("b", placement(5.0, 5.0, 0.0, 1.0, 1.0), 1.0)], square(5.0, 5.0, 1.0)),
            entity("c", vec![0], vec![text("c", placement(9.0, 9.0, 0.0, 1.0, 1.0), 1.0)], square(9.0, 9.0, 1.0)),
        ]);
        let ids = |filter: &EntityFilter| -> Vec<String> {
            scene_texts(&s, filter).into_iter().map(|(entity, _)| entity.id.clone()).collect()
        };
        assert_eq!(ids(&EntityFilter::default()), ["a", "c"]);
        let all = EntityFilter { hidden_layers: Some(HashSet::new()), ..EntityFilter::default() };
        assert_eq!(ids(&all), ["a", "b", "c"]);
        let region = EntityFilter { region: Some(square(1.0, 1.0, 8.0)), hidden_layers: Some(HashSet::new()) };
        assert_eq!(ids(&region), ["a", "b", "c"]);
        let inside = EntityFilter { region: Some(square(2.0, 2.0, 5.0)), hidden_layers: Some(HashSet::new()) };
        assert_eq!(ids(&inside), ["b"]);
        assert_eq!(layer_names(&s, &s.entities[1]), ["表示", "非表示"]);
    }
}
