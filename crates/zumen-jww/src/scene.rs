//! JWW の図面を表示用モデルにする（`@zumenjs/jww` の scene.ts と同じ規則・同じ順）

use crate::reader::{JwwBase, JwwBlockDefinition, JwwDocument, JwwEntity, JwwText};
use crate::style::{is_embedded_image, is_setting, paper_size, text_layout_input};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::f64::consts::PI;
use std::sync::Arc;
use zumen_core::geometry::{
    apply, arc_segments, ellipse_arc_points, empty_box, extend_box, extend_box_with_points, is_empty_box, mean_scale,
    multiply, sweep_radians, transform_points, union_box,
};
use zumen_core::text::layout_text;
use zumen_core::{
    BBox, Background, DotPrimitive, FillPrimitive, Matrix, NoticeLevel, NoticeList, Placement, Primitive, Property,
    Scene, SceneEntity, SceneLayer, Sheet, StrokePrimitive, TextPrimitive, js,
};

const MAX_DEPTH: usize = 16;
const TOLERANCE: f64 = 0.02;

fn number(value: f64) -> String {
    js::format_number(value, 3)
}

fn group_name(doc: &JwwDocument, group: usize) -> String {
    doc.group_name(group)
}

fn layer_name(doc: &JwwDocument, group: usize, layer: usize) -> String {
    doc.layer_name(group, layer)
}

struct Context {
    m: Matrix,
    layers: Vec<u32>,
    path: String,
    depth: usize,
}

struct Builder<'a> {
    doc: &'a JwwDocument,
    origin: (f64, f64),
    notices: NoticeList,
    entities: Vec<SceneEntity>,
    /// 文字として保存された Jw_cad の設定（Printer_Orientation = 0 など）
    settings: Vec<String>,
    used_layers: BTreeMap<u32, u64>,
    blocks: HashMap<u32, &'a JwwBlockDefinition>,
    placements: HashMap<u16, Arc<Placement>>,
}

impl<'a> Builder<'a> {
    fn new(doc: &'a JwwDocument, origin: (f64, f64), notices: NoticeList) -> Self {
        Self {
            doc,
            origin,
            notices,
            entities: Vec::new(),
            settings: Vec::new(),
            used_layers: BTreeMap::new(),
            blocks: doc.blocks.iter().map(|block| (block.number, block)).collect(),
            placements: HashMap::new(),
        }
    }

    fn layer_id(base: &JwwBase) -> u32 {
        u32::from(base.layer_group & 0x0f) * 16 + u32::from(base.layer & 0x0f) + 1
    }

    fn color(&mut self, base: &JwwBase, custom: Option<u32>) -> String {
        self.doc.pen_color(base, custom).unwrap_or_else(|| {
            self.notices.add(NoticeLevel::Warning, "unknown-color", "定義のない線色を黒で表示しました");
            "#000000".to_string()
        })
    }

    fn dashes(&self, style: u8) -> Vec<f64> {
        self.doc.line_dashes(style)
    }

    fn width(&self, base: &JwwBase) -> f64 {
        self.doc.line_width(base)
    }

    /// レイヤグループの縮尺。用紙の座標から実寸の座標へ戻す変換を持つ
    fn placement(&mut self, group: u16) -> Arc<Placement> {
        let index = group & 0x0f;
        if let Some(placement) = self.placements.get(&index) {
            return placement.clone();
        }
        let scale = self.doc.group_scale(index);
        let placement = Arc::new(Placement {
            name: format!("{}（1/{}）", group_name(self.doc, usize::from(index)), number(scale)),
            scale_x: 1.0 / scale,
            scale_y: 1.0 / scale,
            inverse: [scale, 0.0, 0.0, scale, -self.origin.0 * scale, -self.origin.1 * scale],
        });
        self.placements.insert(index, placement.clone());
        placement
    }

    fn stroke(&mut self, ctx: &Context, base: &JwwBase, points: &[f64], closed: bool) -> Primitive {
        Primitive::Stroke(StrokePrimitive {
            points: transform_points(&ctx.m, points),
            closed,
            segments: None,
            color: self.color(base, None),
            width: self.width(base),
            dashes: self.dashes(base.pen_style),
        })
    }

    fn real(&mut self, base: &JwwBase, x: f64, y: f64, ctx: &Context) -> String {
        let (px, py) = apply(&ctx.m, x, y);
        let (rx, ry) = apply(&self.placement(base.layer_group).inverse, px, py);
        format!("{}, {}", number(rx), number(ry))
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        ctx: &Context,
        base: &JwwBase,
        id: usize,
        kind: &str,
        source: &str,
        primitives: Vec<Primitive>,
        properties: Vec<Property>,
    ) {
        if primitives.is_empty() {
            return;
        }
        let mut bbox = empty_box();
        for primitive in &primitives {
            extend_primitive_box(&mut bbox, primitive);
        }
        if is_empty_box(&bbox) {
            return;
        }
        let layer = Self::layer_id(base);
        let mut layers = ctx.layers.clone();
        layers.push(layer);
        for code in &layers {
            *self.used_layers.entry(*code).or_insert(0) += 1;
        }
        let group_index = usize::from(base.layer_group & 0x0f);
        let scale = match self.doc.groups.get(group_index) {
            Some(group) if group.scale > 0.0 => group.scale,
            _ => 1.0,
        };
        let placement = self.placement(base.layer_group);
        let id = format!("{}#{id}", ctx.path);
        let mut all = vec![
            Property::new("種類", kind),
            Property::new("番号", id.clone()),
            Property::new(
                "レイヤ",
                format!(
                    "{} / {}",
                    group_name(self.doc, group_index),
                    layer_name(self.doc, group_index, usize::from(base.layer & 0x0f))
                ),
            ),
            Property::new("縮尺", format!("1/{}", number(scale))),
            Property::new("線色", base.pen_color.to_string()),
            Property::new("線種", base.pen_style.to_string()),
        ];
        all.extend(properties);
        self.entities.push(SceneEntity {
            id,
            kind: kind.to_string(),
            source: source.to_string(),
            layers,
            primitives,
            bbox,
            properties: all,
            placement: Some(placement),
        });
    }

    fn emit(&mut self, entity: &'a JwwEntity, id: usize, ctx: &Context) {
        match entity {
            JwwEntity::Line(e) => {
                let stroke = self.stroke(ctx, &e.base, &[e.x1, e.y1, e.x2, e.y2], false);
                let properties = vec![
                    Property::new("始点（実寸）", self.real(&e.base, e.x1, e.y1, ctx)),
                    Property::new("終点（実寸）", self.real(&e.base, e.x2, e.y2, ctx)),
                ];
                self.add(ctx, &e.base, id, "線", "CDataSen", vec![stroke], properties);
            }
            JwwEntity::Arc(e) => {
                let points = self.arc_points(ctx, e.cx, e.cy, e.radius, e.flatness, e.tilt, e.start, e.sweep, e.full);
                let kind = if e.flatness != 1.0 && (e.flatness - 1.0).abs() > 1e-9 {
                    "楕円"
                } else if e.full {
                    "円"
                } else {
                    "円弧"
                };
                let stroke = self.stroke(ctx, &e.base, &points, e.full);
                let scale = match self.doc.groups.get(usize::from(e.base.layer_group & 0x0f)).map(|group| group.scale) {
                    Some(scale) if scale != 0.0 && !scale.is_nan() => scale,
                    _ => 1.0,
                };
                let properties = vec![
                    Property::new("中心（実寸）", self.real(&e.base, e.cx, e.cy, ctx)),
                    Property::new("半径（実寸）", number(e.radius * scale)),
                ];
                self.add(ctx, &e.base, id, kind, "CDataEnko", vec![stroke], properties);
            }
            JwwEntity::Point(e) => {
                if e.temporary {
                    self.notices.add(NoticeLevel::Info, "temporary-point", "仮点は印刷されないため表示していません");
                    return;
                }
                let (x, y) = apply(&ctx.m, e.x, e.y);
                let dot = Primitive::Dot(DotPrimitive { x, y, color: self.color(&e.base, None) });
                let properties = vec![Property::new("位置（実寸）", self.real(&e.base, e.x, e.y, ctx))];
                self.add(ctx, &e.base, id, "点", "CDataTen", vec![dot], properties);
            }
            JwwEntity::Text(e) => {
                // Jw_cad 8 以降は、印刷・表示の設定を (0, -1000) の「名前 = 値」の文字として保存する。図形ではないので描かない
                if ctx.depth == 0 && is_setting(e) {
                    self.settings.push(e.content.clone());
                    return;
                }
                let Some(text) = self.text(ctx, e) else {
                    return;
                };
                let properties = vec![
                    Property::new("文字列", e.content.clone()),
                    Property::new("フォント", e.font.clone()),
                    Property::new(
                        "文字の大きさ",
                        format!("幅 {}・高さ {}・間隔 {}（mm）", number(e.size_x), number(e.size_y), number(e.spacing)),
                    ),
                ];
                self.add(ctx, &e.base, id, "文字", "CDataMoji", vec![Primitive::Text(text)], properties);
            }
            JwwEntity::Solid(e) => {
                let ring = transform_points(&ctx.m, &e.points);
                let fill = Primitive::Fill(FillPrimitive { color: self.color(&e.base, e.color), rings: vec![ring] });
                self.add(ctx, &e.base, id, "ソリッド", "CDataSolid", vec![fill], Vec::new());
            }
            JwwEntity::CircleSolid(e) => {
                let full = e.mode == 100.0 || (e.sweep.abs() - PI * 2.0).abs() < 1e-6;
                let flatness = if e.flatness == 0.0 || e.flatness.is_nan() { 1.0 } else { e.flatness };
                let mut points = self.arc_points(
                    ctx,
                    e.cx,
                    e.cy,
                    e.radius.abs(),
                    flatness,
                    e.tilt,
                    if full { 0.0 } else { e.start },
                    if full { PI * 2.0 } else { e.sweep },
                    full,
                );
                if !full && e.mode != -1.0 && e.mode != 5.0 {
                    points.splice(0..0, [e.cx, e.cy]);
                }
                let ring = transform_points(&ctx.m, &points);
                let fill = Primitive::Fill(FillPrimitive { color: self.color(&e.base, e.color), rings: vec![ring] });
                self.add(ctx, &e.base, id, "円ソリッド", "CDataSolid", vec![fill], Vec::new());
            }
            JwwEntity::Block(e) => {
                let Some(block) = self.blocks.get(&e.definition).copied() else {
                    self.notices.add(NoticeLevel::Warning, "missing-block", "定義のないブロックの配置を省きました");
                    return;
                };
                if ctx.depth >= MAX_DEPTH {
                    self.notices.add(NoticeLevel::Warning, "block-depth", "入れ子が深すぎるブロックを省きました");
                    return;
                }
                let cos = js::cos(e.rotation);
                let sin = js::sin(e.rotation);
                let local = [cos * e.scale_x, sin * e.scale_x, -sin * e.scale_y, cos * e.scale_y, e.x, e.y];
                let mut layers = ctx.layers.clone();
                layers.push(Self::layer_id(&e.base));
                let child = Context {
                    m: multiply(&ctx.m, &local),
                    layers,
                    path: format!("{}#{id}>", ctx.path),
                    depth: ctx.depth + 1,
                };
                for (index, member) in block.entities.iter().enumerate() {
                    self.emit(member, index + 1, &child);
                }
            }
            JwwEntity::Dimension(e) => {
                let line = &e.line;
                let mut primitives = vec![self.stroke(ctx, &line.base, &[line.x1, line.y1, line.x2, line.y2], false)];
                for line in &e.lines {
                    if line.x1 == line.x2 && line.y1 == line.y2 {
                        continue;
                    }
                    primitives.push(self.stroke(ctx, &line.base, &[line.x1, line.y1, line.x2, line.y2], false));
                }
                for point in &e.points {
                    if point.temporary {
                        continue;
                    }
                    let (x, y) = apply(&ctx.m, point.x, point.y);
                    primitives.push(Primitive::Dot(DotPrimitive { x, y, color: self.color(&point.base, None) }));
                }
                if let Some(text) = self.text(ctx, &e.text) {
                    primitives.push(Primitive::Text(text));
                }
                let properties = vec![Property::new("寸法値", e.text.content.clone())];
                self.add(ctx, &e.base, id, "寸法", "CDataSunpou", primitives, properties);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn arc_points(
        &self,
        ctx: &Context,
        cx: f64,
        cy: f64,
        radius: f64,
        flatness: f64,
        tilt: f64,
        start: f64,
        sweep: f64,
        full: bool,
    ) -> Vec<f64> {
        let deg = 180.0 / PI;
        let rx = radius;
        let ry = radius * (if flatness > 0.0 { flatness } else { 1.0 });
        let begin = if full { 0.0 } else { start * deg };
        let end = if full { 360.0 } else { (start + sweep) * deg };
        let ccw = full || sweep >= 0.0;
        let span = if full { PI * 2.0 } else { sweep_radians(begin, end, ccw).abs() };
        let tolerance = TOLERANCE / js::max(mean_scale(&ctx.m), 1e-12);
        let segments = arc_segments(js::max(rx, ry), span, tolerance);
        let mut points = ellipse_arc_points(cx, cy, rx, ry, tilt * deg, begin, end, ccw, segments);
        if full {
            points.truncate(points.len().saturating_sub(2));
        }
        points
    }

    fn text(&mut self, ctx: &Context, text: &JwwText) -> Option<TextPrimitive> {
        if text.content.is_empty() {
            return None;
        }
        if is_embedded_image(text) {
            self.notices.add(
                NoticeLevel::Info,
                "embedded-image",
                "文字に埋め込んだ画像（^@BM など）は表示していません",
            );
            return None;
        }
        let color = self.color(&text.base, None);
        layout_text(text_layout_input(text, color), &ctx.m)
    }
}

fn extend_primitive_box(bbox: &mut BBox, primitive: &Primitive) {
    match primitive {
        Primitive::Stroke(stroke) => extend_box_with_points(bbox, &stroke.points),
        Primitive::Fill(FillPrimitive { rings, .. }) => {
            for ring in rings {
                extend_box_with_points(bbox, ring);
            }
        }
        Primitive::Clipped(clipped) => {
            for ring in &clipped.rings {
                extend_box_with_points(bbox, ring);
            }
        }
        Primitive::Dot(dot) => extend_box(bbox, dot.x, dot.y),
        Primitive::Text(text) => {
            let m = text.matrix;
            for (x, y) in [(0.0, 0.0), (text.box_width, 0.0), (text.box_width, text.box_height), (0.0, text.box_height)]
            {
                extend_box(bbox, m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            }
        }
    }
}

fn luminance(color: &str) -> f64 {
    let Ok(value) = u32::from_str_radix(color.get(1..).unwrap_or(""), 16) else {
        return f64::NAN;
    };
    (f64::from(value >> 16) * 0.299 + f64::from((value >> 8) & 0xff) * 0.587 + f64::from(value & 0xff) * 0.114) / 255.0
}

/// JWW の図面を、用紙の座標（左下が原点の mm）に解決した表示用の図面にする
pub fn jww_to_scene(doc: &JwwDocument) -> Scene {
    let mut notices = NoticeList::new();
    if let Some(truncated) = &doc.truncated {
        notices.add(
            NoticeLevel::Warning,
            "truncated",
            format!("途中から読めなかったため、読めた図形だけを表示しました（{truncated}）"),
        );
    }
    if doc.pen_colors.is_none() {
        notices.add(
            NoticeLevel::Info,
            "default-palette",
            "ヘッダの線色を読めないため、Jw_cad の既定の色で表示しました",
        );
    }
    let sheet = paper_size(doc.paper_size).map(|(label, width, height)| Sheet {
        name: String::new(),
        size: label.to_string(),
        width,
        height,
    });
    // JWW の座標は用紙の中心が原点。用紙の左下を原点にする
    let origin = sheet.as_ref().map_or((0.0, 0.0), |sheet| (sheet.width / 2.0, sheet.height / 2.0));
    let mut builder = Builder::new(doc, origin, notices);
    let root =
        Context { m: [1.0, 0.0, 0.0, 1.0, origin.0, origin.1], layers: Vec::new(), path: String::new(), depth: 0 };
    for (index, entity) in doc.entities.iter().enumerate() {
        builder.emit(entity, index + 1, &root);
    }
    let mut bounds = empty_box();
    for entity in &builder.entities {
        union_box(&mut bounds, &entity.bbox);
    }
    if let Some(sheet) = &sheet {
        extend_box(&mut bounds, 0.0, 0.0);
        extend_box(&mut bounds, sheet.width, sheet.height);
    }
    let layers = builder
        .used_layers
        .keys()
        .map(|&id| {
            let group_index = ((id - 1) / 16) as usize;
            let layer_index = ((id - 1) % 16) as usize;
            let group = doc.groups.get(group_index);
            let layer = group.and_then(|group| group.layers.get(layer_index));
            SceneLayer {
                id,
                name: layer_name(doc, group_index, layer_index),
                visible: group.map_or(1, |group| group.state) != 0 && layer.map_or(1, |layer| layer.state) != 0,
                group: Some(group_name(doc, group_index)),
            }
        })
        .collect();
    let paper = doc.pen_colors.as_ref().and_then(|colors| colors.first()).map_or("#ffffff", String::as_str);
    let background = if luminance(paper) < 0.5 { Background::Dark } else { Background::Light };
    let mut metadata = vec![Property::new("形式", format!("JWW（版 {}）", doc.version))];
    if let Some(sheet) = &sheet {
        metadata.push(Property::new(
            "用紙",
            format!("{}（{}×{}mm）", sheet.size, js::number_to_string(sheet.width), js::number_to_string(sheet.height)),
        ));
    }
    // 図形のあるレイヤグループの縮尺だけを示す
    let used_groups: BTreeSet<usize> = builder.used_layers.keys().map(|id| ((id - 1) / 16) as usize).collect();
    let mut scales: Vec<f64> = Vec::new();
    for (index, group) in doc.groups.iter().enumerate() {
        if used_groups.contains(&index) && group.scale > 0.0 && !scales.contains(&group.scale) {
            scales.push(group.scale);
        }
    }
    metadata.push(Property::new(
        "縮尺",
        scales.iter().map(|scale| format!("1/{}", number(*scale))).collect::<Vec<_>>().join("・"),
    ));
    let memo = js::trim(&doc.memo);
    if !memo.is_empty() {
        metadata.push(Property::new("メモ", memo));
    }
    if !doc.blocks.is_empty() {
        metadata.push(Property::new("ブロック", format!("{} 件", doc.blocks.len())));
    }
    if !builder.settings.is_empty() {
        metadata.push(Property::new("Jw_cad の設定", builder.settings.join("、")));
    }
    Scene {
        format: "jww".to_string(),
        version: format!("JWW {}", doc.version),
        views: None,
        view: None,
        sheet,
        bounds,
        layers,
        entities: builder.entities,
        notices: builder.notices.into_vec(),
        metadata,
        background,
    }
}
