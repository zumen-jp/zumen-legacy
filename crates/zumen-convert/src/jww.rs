//! JWW の図面（`JwwDocument`）を AutoCAD の図面のモデルに写す（docs/formats/jww.md「DXF への変換」）。
//!
//! JWW の座標は用紙の中心を原点とする用紙の mm で、レイヤグループの縮尺を掛けると実寸になる。
//! - 縮尺ごとに、そのレイヤグループの図形をモデル空間に実寸（座標 × 縮尺）で置き、用紙のレイアウトに
//!   その縮尺のビューポートを置く。図面の縮尺が 1 つなら、1/1 でもモデル空間に置く。
//! - 縮尺が 2 つ以上あるときは、1/1 のレイヤグループ（図枠・表題など）を紙空間に置く。
//! - レイヤは「グループの番号-レイヤの番号 名前」、非表示のグループ・レイヤは非表示、表示のみはロックにする。
//! - 線色は画面色（ヘッダ）、線種はヘッダのビット列の長さ、線幅は印刷の線幅を DXF の線の太さにする。
//! - ブロックはブロック定義とその配置（INSERT）、寸法は寸法線・補助線・端点・寸法値を表示のブロックに入れた
//!   DIMENSION、ソリッドは SOLID、円ソリッドは塗りつぶしの HATCH にする。
//!
//! 表示（zumen-jww の scene.rs）と同じ順・同じ規則で辿り、同じ所に同じ形を描く DXF にする。

use crate::Mapped;
use crate::builder::{
    BY_LAYER, Builder, Color, DEFAULT_FONT, FOREGROUND, LINEWEIGHT_BY_LAYER, Look, Space, VIEWPORT_LAYER, lineweight,
    polyline_path, scale_of, solid_hatch, xyz,
};
use crate::sheet::{Content, View, overall_viewport, rect_of, region, separate, viewport};
use crate::text::{normalize, text_entity};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::PI;
use zumen_core::geometry::{
    apply, arc_segments, ellipse_arc_points, empty_box, invert, mean_scale, multiply, sweep_radians, union_box,
};
use zumen_core::text::layout_text;
use zumen_core::{BBox, Matrix, NoticeLevel, js};
use zumen_dxf::{DxfDimension, DxfEntityData, DxfInsert, Vec2};
use zumen_jww::{
    JwwBase, JwwBlockDefinition, JwwDocument, JwwEntity, JwwText, is_embedded_image, is_random_line, is_setting,
    jww_to_scene, paper_size, text_layout_input,
};

/// ブロックの入れ子の上限（表示と同じ）
const MAX_DEPTH: usize = 16;
/// 表示が円弧を折線にする許容誤差（用紙の mm）
const TOLERANCE: f64 = 0.02;

/// JWW の図面を AutoCAD の図面のモデルに写す
pub fn jww_to_dxf(doc: &JwwDocument) -> Mapped {
    Mapper::new(doc).run()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Top,
    Block(u32),
}

/// 入れ物の座標
#[derive(Debug, Clone)]
struct Frame {
    key: Key,
    space: Space,
    /// JWW の座標（用紙の中心が原点の用紙の mm、ブロックの中はブロックの座標）→ 入れ物の座標
    to_container: Matrix,
    /// 入れ物の座標 → 用紙（左下が原点）
    to_paper: Matrix,
    linetype_scale: f64,
    depth: usize,
}

impl Frame {
    fn point(&self, x: f64, y: f64) -> Vec2 {
        let (px, py) = apply(&self.to_container, x, y);
        [px, py]
    }

    fn scale(&self) -> f64 {
        scale_of(&self.to_container)
    }
}

/// ブロックの最初の配置
#[derive(Debug, Clone)]
struct BlockUse {
    name: String,
    /// ブロックの座標 → 用紙
    to_paper: Matrix,
    /// 配置の縮尺の積（モデル空間・紙空間から）
    scale: f64,
}

struct Mapper<'a> {
    doc: &'a JwwDocument,
    out: Builder,
    /// 用紙の中心（JWW の原点）の、用紙の左下からの位置
    origin: (f64, f64),
    /// 元の図面で描いた図形の外接矩形と数（最上位の図形の順番 #n ごと）
    boxes: HashMap<String, (BBox, usize)>,
    definitions: HashMap<u32, &'a JwwBlockDefinition>,
    blocks: HashMap<u32, BlockUse>,
    block_order: Vec<u32>,
    layers: BTreeMap<u32, String>,
    cuts: HashSet<(Key, usize)>,
}

/// 図形のレイヤの番号（グループ × 16 + レイヤ + 1、表示と同じ）
fn layer_id(base: &JwwBase) -> u32 {
    u32::from(base.layer_group & 0x0f) * 16 + u32::from(base.layer & 0x0f) + 1
}

fn boxes_by_top_id(doc: &JwwDocument) -> HashMap<String, (BBox, usize)> {
    crate::sheet::boxes_by_top_id(&jww_to_scene(doc))
}

impl<'a> Mapper<'a> {
    fn new(doc: &'a JwwDocument) -> Self {
        let origin = paper_size(doc.paper_size).map_or((0.0, 0.0), |(_, width, height)| (width / 2.0, height / 2.0));
        Mapper {
            doc,
            out: Builder::new(),
            origin,
            boxes: boxes_by_top_id(doc),
            definitions: doc.blocks.iter().map(|block| (block.number, block)).collect(),
            blocks: HashMap::new(),
            block_order: Vec::new(),
            layers: BTreeMap::new(),
            cuts: HashSet::new(),
        }
    }

    fn run(mut self) -> Mapped {
        let doc = self.doc;
        self.report_document();
        // 描く最上位の図形の縮尺（レイヤグループごと）
        let drawn: Vec<(usize, f64)> = doc
            .entities
            .iter()
            .enumerate()
            .filter(|(index, _)| self.boxes.contains_key(&format!("#{}", index + 1)))
            .map(|(index, entity)| (index, doc.group_scale(entity.base().layer_group)))
            .collect();
        let mut scales: Vec<f64> = Vec::new();
        for (_, scale) in &drawn {
            if !scales.contains(scale) {
                scales.push(*scale);
            }
        }
        // 縮尺が 2 つ以上なら、1/1 のレイヤグループは用紙にじかに描いたものとして紙空間に置く
        let paper_scale = scales.len() > 1 && scales.contains(&1.0);
        let view_scales: Vec<f64> = scales.iter().copied().filter(|scale| !(paper_scale && *scale == 1.0)).collect();
        let (width, height, sheet_name, media) = self.sheet();
        let mut views: Vec<View> = Vec::new();
        let mut contents: Vec<Content> = Vec::new();
        for scale in &view_scales {
            let mut bbox = empty_box();
            let mut weight = 0;
            for (index, entity_scale) in &drawn {
                if entity_scale == scale {
                    let (entity_box, count) = self.boxes[&format!("#{}", index + 1)];
                    union_box(&mut bbox, &entity_box);
                    weight += count;
                }
            }
            let Some(rect) = rect_of(&bbox) else {
                continue;
            };
            views.push(View {
                content: contents.len(),
                scale: 1.0 / scale,
                angle: 0.0,
                origin: [self.origin.0, self.origin.1],
                rect,
            });
            contents.push(Content { weight, offset: [0.0, 0.0] });
        }
        let moved = separate(&views, &mut contents);
        if moved > 0 {
            self.out.notices.add_count(
                NoticeLevel::Info,
                "view-moved",
                "縮尺の違うレイヤグループがモデル空間で重ならないよう、ずらして置きました（ずらした図形の実寸の座標は元と変わります）",
                moved,
            );
        }
        let origin = self.origin;
        let frame_of = |scale: f64, views: &[View], contents: &[Content]| -> Frame {
            match views.iter().position(|view| (1.0 / view.scale - scale).abs() <= 1e-12 * scale.abs()) {
                Some(index) => {
                    let offset = contents[views[index].content].offset;
                    let container = [scale, 0.0, 0.0, scale, offset[0], offset[1]];
                    Frame {
                        key: Key::Top,
                        space: Space::Model,
                        to_container: container,
                        to_paper: multiply(
                            &[1.0, 0.0, 0.0, 1.0, origin.0, origin.1],
                            &invert(&[scale, 0.0, 0.0, scale, offset[0], offset[1]]),
                        ),
                        linetype_scale: 1.0,
                        depth: 0,
                    }
                }
                None => Frame {
                    key: Key::Top,
                    space: Space::Paper,
                    to_container: [1.0, 0.0, 0.0, 1.0, origin.0, origin.1],
                    to_paper: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    linetype_scale: 1.0,
                    depth: 0,
                },
            }
        };
        // ブロックの最初の配置を決める（ブロックの中の線種・ブロックの定義の循環）
        for (index, entity) in doc.entities.iter().enumerate() {
            let frame = frame_of(doc.group_scale(entity.base().layer_group), &views, &contents);
            let to_paper = multiply(&frame.to_paper, &frame.to_container);
            self.walk(entity, index, &Key::Top, &to_paper, frame.scale(), 0, &[]);
        }
        overall_viewport(&mut self.out, width, height);
        let mut extents = empty_box();
        for (index, view) in views.iter().enumerate() {
            let offset = contents[view.content].offset;
            union_box(&mut extents, &region(view, offset));
            let look = Look {
                layer: VIEWPORT_LAYER.into(),
                color: BY_LAYER,
                linetype: "ByLayer".into(),
                lineweight: LINEWEIGHT_BY_LAYER,
                linetype_scale: 1.0,
            };
            let data = DxfEntityData::Viewport(viewport(view, offset, index as f64 + 2.0));
            self.out.add(&Space::Paper, &look, "VIEWPORT", data, "");
        }
        for (index, entity) in doc.entities.iter().enumerate() {
            let frame = frame_of(doc.group_scale(entity.base().layer_group), &views, &contents);
            self.emit(entity, index, &frame);
        }
        for number in self.block_order.clone() {
            let block = self.blocks[&number].clone();
            let definition = self.definitions[&number];
            let frame = Frame {
                key: Key::Block(number),
                space: Space::Block(block.name.clone()),
                to_container: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                to_paper: block.to_paper,
                linetype_scale: 1.0 / block.scale,
                depth: 1,
            };
            for (index, member) in definition.entities.iter().enumerate() {
                self.emit(member, index, &frame);
            }
        }
        let header = &mut self.out.doc.header;
        if extents.min_x <= extents.max_x {
            header.extmin = Some([extents.min_x, extents.min_y, 0.0]);
            header.extmax = Some([extents.max_x, extents.max_y, 0.0]);
            header.limmin = Some([extents.min_x, extents.min_y]);
            header.limmax = Some([extents.max_x, extents.max_y]);
        }
        header.plimmin = Some([0.0, 0.0]);
        header.plimmax = Some([width, height]);
        self.out.report_renamed();
        crate::builder::add_layouts(&mut self.out, &sheet_name, &media, width, height);
        Mapped { document: self.out.doc, notices: self.out.notices.into_vec(), sources: self.out.sources }
    }

    /// 用紙の大きさ（mm）とレイアウト・用紙の名前
    fn sheet(&self) -> (f64, f64, String, String) {
        if let Some((label, width, height)) = paper_size(self.doc.paper_size) {
            let media = if label.starts_with('A') {
                format!("ISO_full_bleed_{label}_({width:.2}_x_{height:.2}_MM)")
            } else {
                format!("USER_({width:.2}_x_{height:.2}_MM)")
            };
            return (width, height, label.to_string(), media);
        }
        let mut bounds = empty_box();
        for (bbox, _) in self.boxes.values() {
            union_box(&mut bounds, bbox);
        }
        let width = if bounds.max_x.is_finite() { js::max(bounds.max_x, 1.0) } else { 1.0 };
        let height = if bounds.max_y.is_finite() { js::max(bounds.max_y, 1.0) } else { 1.0 };
        (width, height, "Layout1".into(), format!("USER_({width:.2}_x_{height:.2}_MM)"))
    }

    fn report_document(&mut self) {
        let doc = self.doc;
        if let Some(truncated) = &doc.truncated {
            self.out.notices.add(
                NoticeLevel::Warning,
                "truncated",
                format!("途中から読めなかったため、読めた図形だけを書きました（{truncated}）"),
            );
        }
        if doc.pen_colors.is_none() {
            self.out.notices.add(
                NoticeLevel::Info,
                "default-palette",
                "ヘッダの線色を読めないため、Jw_cad の既定の色で書きました",
            );
        }
        if !js::trim(&doc.memo).is_empty() {
            self.out.notices.add(NoticeLevel::Info, "memo", "図面のメモは、DXF に書く場所がないため書いていません");
        }
        let named = doc
            .groups
            .iter()
            .enumerate()
            .any(|(index, group)| !js::trim(&group.name).is_empty() && group.name != format!("グループ {index:X}"));
        if named {
            self.out.notices.add(
                NoticeLevel::Info,
                "layer-group",
                "レイヤグループは DXF にないため、レイヤの名前の先頭の番号（0-1 の 0）で示しました（グループの名前は書いていません）",
            );
        }
    }

    /// ブロックの最初の配置を決め、書かない配置（定義がない・深すぎる・循環する）を覚える。scale は今の座標の
    /// 1 がモデル空間・紙空間の何倍か（最上位はレイヤグループの縮尺、ブロックの中は配置の倍率の積）
    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        entity: &'a JwwEntity,
        index: usize,
        key: &Key,
        to_paper: &Matrix,
        scale: f64,
        depth: usize,
        stack: &[u32],
    ) {
        let JwwEntity::Block(e) = entity else {
            return;
        };
        let Some(definition) = self.definitions.get(&e.definition).copied() else {
            self.cuts.insert((key.clone(), index));
            self.out.notices.add(NoticeLevel::Warning, "missing-block", "定義のないブロックの配置を省きました");
            return;
        };
        if depth >= MAX_DEPTH || stack.contains(&e.definition) {
            self.cuts.insert((key.clone(), index));
            self.out.notices.add(
                NoticeLevel::Warning,
                "block-depth",
                "入れ子が深すぎる・循環するブロックの配置を省きました",
            );
            return;
        }
        let local = insert_matrix(e.x, e.y, e.scale_x, e.scale_y, e.rotation);
        let m = multiply(to_paper, &local);
        let child_scale = js::max(scale * mean_scale(&local), 1e-12);
        match self.blocks.get(&e.definition) {
            Some(used) => {
                if (used.scale - child_scale).abs() > 1e-9 * used.scale.max(child_scale) {
                    self.out.notices.add(
                        NoticeLevel::Info,
                        "block-scale",
                        "縮尺の違う配置から使われるブロックの中の線種のピッチは、最初の配置に合わせました",
                    );
                }
            }
            None => {
                let name = if definition.name.trim().is_empty() {
                    format!("ブロック {}", definition.number)
                } else {
                    definition.name.clone()
                };
                let name = self.out.block(&name);
                self.blocks.insert(e.definition, BlockUse { name, to_paper: m, scale: child_scale });
                self.block_order.push(e.definition);
                let mut stack = stack.to_vec();
                stack.push(e.definition);
                for (member_index, member) in definition.entities.iter().enumerate() {
                    self.walk(member, member_index, &Key::Block(e.definition), &m, child_scale, depth + 1, &stack);
                }
            }
        }
    }

    // --- 書く --------------------------------------------------------------------------------------

    fn layer(&mut self, base: &JwwBase) -> String {
        let id = layer_id(base);
        if let Some(name) = self.layers.get(&id) {
            return name.clone();
        }
        let group = usize::from(base.layer_group & 0x0f);
        let layer = usize::from(base.layer & 0x0f);
        let group_info = self.doc.groups.get(group);
        let layer_info = group_info.and_then(|info| info.layers.get(layer));
        let group_state = group_info.map_or(2, |info| info.state);
        let layer_state = layer_info.map_or(2, |info| info.state);
        // 状態: 0 非表示、1 表示のみ（編集できない）、2 編集可能、3 書込み
        let off = group_state == 0 || layer_state == 0;
        let locked = group_state == 1 || layer_state == 1;
        let name = self.out.layer(&self.doc.layer_name(group, layer), off, locked, true);
        self.layers.insert(id, name.clone());
        name
    }

    fn color(&mut self, base: &JwwBase, custom: Option<u32>) -> Color {
        match self.doc.pen_color(base, custom) {
            Some(hex) => Color::from_hex(&hex),
            None => {
                self.out.notices.add(
                    NoticeLevel::Warning,
                    "unknown-color",
                    "定義のない線色を黒（前景色、ACI 7）で書きました",
                );
                FOREGROUND
            }
        }
    }

    fn linetype(&mut self, style: u8) -> String {
        if is_random_line(style) {
            self.out.notices.add(NoticeLevel::Info, "random-line", "ランダム線（手書き風の線）は実線で書きました");
            return "Continuous".into();
        }
        let dashes = self.doc.line_dashes(style);
        if dashes.is_empty() {
            return "Continuous".into();
        }
        let name = match style {
            31..=45 => zumen_sxf::tables::PREDEFINED_LINE_TYPES
                .get(usize::from(style - 31))
                .map_or_else(|| format!("Jw_cad 線種 {style}"), |kind| kind.name.to_string()),
            _ => format!("Jw_cad 線種 {style}"),
        };
        self.out.linetype(&name, "", &dashes)
    }

    fn look(&mut self, base: &JwwBase, frame: &Frame, custom: Option<u32>) -> Look {
        let width = self.doc.line_width(base);
        let (weight, rounded) = lineweight(width);
        if rounded {
            self.out.notices.add(
                NoticeLevel::Info,
                "lineweight",
                "線幅を、DXF で使えるいちばん近い線の太さ（0.13・0.18・0.25・0.3 mm など）にしました",
            );
        }
        Look {
            layer: self.layer(base),
            color: self.color(base, custom),
            linetype: self.linetype(base.pen_style),
            lineweight: weight,
            linetype_scale: frame.linetype_scale,
        }
    }

    fn degenerate(&mut self) {
        self.out.notices.add(NoticeLevel::Info, "degenerate", "大きさが 0 の円などの、形にならない図形を省きました");
    }

    fn emit(&mut self, entity: &JwwEntity, index: usize, frame: &Frame) {
        let source = format!("#{}", index + 1);
        match entity {
            JwwEntity::Line(e) => {
                let look = self.look(&e.base, frame, None);
                let data =
                    DxfEntityData::Line { start: xyz(frame.point(e.x1, e.y1)), end: xyz(frame.point(e.x2, e.y2)) };
                self.out.add(&frame.space, &look, "LINE", data, &source);
            }
            JwwEntity::Arc(e) => {
                let ry_factor = if e.flatness > 0.0 { e.flatness } else { 1.0 };
                if !(e.radius.is_finite() && e.radius > 0.0) {
                    self.degenerate();
                    return;
                }
                let look = self.look(&e.base, frame, None);
                let center = xyz(frame.point(e.cx, e.cy));
                let s = frame.scale();
                let ellipse = e.flatness != 1.0 && (e.flatness - 1.0).abs() > 1e-9;
                let tilt = e.tilt.to_degrees();
                let ccw = e.full || e.sweep >= 0.0;
                let (begin, end) = (e.start.to_degrees(), (e.start + e.sweep).to_degrees());
                if ellipse {
                    let (rx, ry) = (e.radius, e.radius * ry_factor);
                    let (sin, cos) = e.tilt.sin_cos();
                    let (major, ratio, shift) = if rx >= ry {
                        ([rx * cos, rx * sin], ry / rx, 0.0)
                    } else {
                        ([-ry * sin, ry * cos], rx / ry, PI / 2.0)
                    };
                    let (start_param, end_param) = if e.full {
                        (0.0, PI * 2.0)
                    } else if ccw {
                        (begin.to_radians() - shift, end.to_radians() - shift)
                    } else {
                        (end.to_radians() - shift, begin.to_radians() - shift)
                    };
                    let data = DxfEntityData::Ellipse {
                        center,
                        major_axis: [major[0] * s, major[1] * s, 0.0],
                        ratio,
                        start_param,
                        end_param,
                    };
                    self.out.add(&frame.space, &look, "ELLIPSE", data, &source);
                } else if e.full {
                    let data = DxfEntityData::Circle { center, radius: e.radius * s };
                    self.out.add(&frame.space, &look, "CIRCLE", data, &source);
                } else {
                    // 傾きは円の始角に足す。時計回り（円弧角が負）は始角と終角を入れ替える
                    let (a, b) = if ccw { (begin + tilt, end + tilt) } else { (end + tilt, begin + tilt) };
                    let data = DxfEntityData::Arc { center, radius: e.radius * s, start_angle: a, end_angle: b };
                    self.out.add(&frame.space, &look, "ARC", data, &source);
                }
            }
            JwwEntity::Point(e) => {
                if e.temporary {
                    self.out.notices.add(NoticeLevel::Info, "temporary-point", "仮点は印刷されないため書いていません");
                    return;
                }
                let look = self.look(&e.base, frame, None);
                let data = DxfEntityData::Point { location: xyz(frame.point(e.x, e.y)), x_axis_angle: 0.0 };
                self.out.add(&frame.space, &look, "POINT", data, &source);
            }
            JwwEntity::Text(e) => {
                if frame.depth == 0 && is_setting(e) {
                    self.out.notices.add(
                        NoticeLevel::Info,
                        "jw-settings",
                        "Jw_cad の設定（Printer_Orientation = 0 などの文字）は図形でないため書いていません",
                    );
                    return;
                }
                let look = self.look(&e.base, frame, None);
                self.text(e, frame, &look, &frame.space, &source);
            }
            JwwEntity::Solid(e) => {
                let look = self.look(&e.base, frame, e.color);
                let p = |i: usize| xyz(frame.point(e.points[i * 2], e.points[i * 2 + 1]));
                // JWW の四角形は 始点 → 第 2 点 → 第 3 点 → 終点。SOLID は 1 → 2 → 4 → 3 の順に塗る
                let data =
                    DxfEntityData::Solid { vtx0: p(0), vtx1: p(1), vtx2: p(3), vtx3: p(2), invisible_edges: None };
                self.out.add(&frame.space, &look, "SOLID", data, &source);
            }
            JwwEntity::CircleSolid(e) => self.circle_solid(e, frame, &source),
            JwwEntity::Block(e) => {
                if self.cuts.contains(&(frame.key.clone(), index)) {
                    return;
                }
                let Some(block) = self.blocks.get(&e.definition).cloned() else {
                    return;
                };
                let local = insert_matrix(e.x, e.y, e.scale_x, e.scale_y, e.rotation);
                let m = multiply(&frame.to_container, &local);
                let s = frame.scale();
                let look = Look {
                    layer: self.layer(&e.base),
                    color: BY_LAYER,
                    linetype: "ByLayer".into(),
                    lineweight: LINEWEIGHT_BY_LAYER,
                    linetype_scale: 1.0,
                };
                let data = DxfEntityData::Insert(DxfInsert {
                    name: block.name,
                    insert: [m[4], m[5], 0.0],
                    xscale: e.scale_x * s,
                    yscale: e.scale_y * s,
                    zscale: 1.0,
                    rotation: normalize(e.rotation.to_degrees()),
                    column_count: 1.0,
                    row_count: 1.0,
                    column_spacing: 0.0,
                    row_spacing: 0.0,
                    attribs: Vec::new(),
                });
                self.out.add(&frame.space, &look, "INSERT", data, &source);
            }
            JwwEntity::Dimension(e) => self.dimension(e, frame, &source),
        }
    }

    /// 文字。書いた文字範囲の中心（入れ物の座標）を返す
    fn text(&mut self, e: &JwwText, frame: &Frame, look: &Look, space: &Space, source: &str) -> Option<Vec2> {
        if e.content.is_empty() {
            return None;
        }
        if is_embedded_image(e) {
            self.out.notices.add(
                NoticeLevel::Info,
                "embedded-image",
                "文字に埋め込んだ画像（^@BM など）は書いていません",
            );
            return None;
        }
        let input = text_layout_input(e, String::new());
        let vertical = input.vertical;
        if input.spacing > 0.0 && e.content.chars().count() > 1 {
            self.out.notices.add(
                NoticeLevel::Info,
                "text-spacing",
                "文字間隔は DXF の文字に書けないため、文字の幅を広げて同じ文字範囲に書きました",
            );
        }
        let primitive = layout_text(input, &frame.to_container)?;
        let font = e.font.strip_prefix('@').unwrap_or(&e.font);
        let style = self.out.style(if font.trim().is_empty() { DEFAULT_FONT } else { font }, vertical);
        let data = text_entity(&primitive, vertical, 0.0, &style)?;
        let look = Look { linetype: "Continuous".into(), lineweight: LINEWEIGHT_BY_LAYER, ..look.clone() };
        self.out.add(space, &look, "TEXT", DxfEntityData::Text(data), source);
        let m = &primitive.matrix;
        let (cx, cy) = (primitive.box_width / 2.0, primitive.box_height / 2.0);
        Some([m[0] * cx + m[2] * cy + m[4], m[1] * cx + m[3] * cy + m[5]])
    }

    /// 円ソリッド（円・扇形・弓形を塗る）は、塗りつぶしの HATCH にする。円は弧の膨らみで、楕円は表示と同じ点で書く
    fn circle_solid(&mut self, e: &zumen_jww::JwwCircleSolid, frame: &Frame, source: &str) {
        let full = e.mode == 100.0 || (e.sweep.abs() - PI * 2.0).abs() < 1e-6;
        let flatness = if e.flatness == 0.0 || e.flatness.is_nan() { 1.0 } else { e.flatness };
        let radius = e.radius.abs();
        if !(radius.is_finite() && radius > 0.0) {
            self.degenerate();
            return;
        }
        let look = self.look(&e.base, frame, e.color);
        let look = Look { linetype: "Continuous".into(), ..look };
        let sector = !full && e.mode != -1.0 && e.mode != 5.0;
        let circle = flatness == 1.0 || (flatness - 1.0).abs() <= 1e-9;
        let (points, bulges): (Vec<Vec2>, Vec<f64>) = if circle {
            let (cx, cy) = (e.cx, e.cy);
            let at = |angle: f64| frame.point(cx + radius * (angle).cos(), cy + radius * (angle).sin());
            if full {
                (vec![at(e.tilt), at(e.tilt + PI)], vec![1.0, 1.0])
            } else {
                let start = e.start + e.tilt;
                let end = start + e.sweep;
                let bulge = (e.sweep / 4.0).tan();
                if sector {
                    (vec![frame.point(cx, cy), at(start), at(end)], vec![0.0, bulge, 0.0])
                } else {
                    (vec![at(start), at(end)], vec![bulge, 0.0])
                }
            }
        } else {
            // 楕円は表示（arc_points）と同じ点で
            let deg = 180.0 / PI;
            let rx = radius;
            let ry = radius * if flatness > 0.0 { flatness } else { 1.0 };
            let begin = if full { 0.0 } else { e.start * deg };
            let end = if full { 360.0 } else { (e.start + e.sweep) * deg };
            let ccw = full || e.sweep >= 0.0;
            let span = if full { PI * 2.0 } else { sweep_radians(begin, end, ccw).abs() };
            let m = multiply(&frame.to_paper, &frame.to_container);
            let tolerance = TOLERANCE / js::max(mean_scale(&m), 1e-12);
            let segments = arc_segments(js::max(rx, ry), span, tolerance);
            let mut flat = ellipse_arc_points(e.cx, e.cy, rx, ry, e.tilt * deg, begin, end, ccw, segments);
            if full {
                flat.truncate(flat.len().saturating_sub(2));
            }
            if sector {
                flat.splice(0..0, [e.cx, e.cy]);
            }
            let points: Vec<Vec2> = flat.as_chunks::<2>().0.iter().map(|[x, y]| frame.point(*x, *y)).collect();
            let count = points.len();
            (points, vec![0.0; count])
        };
        if points.len() < 2 {
            self.degenerate();
            return;
        }
        let path = polyline_path(&points, Some(bulges), 1.0);
        self.out.add(&frame.space, &look, "HATCH", solid_hatch(vec![path]), source);
    }

    /// 寸法は、寸法線・補助線・端点・寸法値を表示のブロック（*D…）に入れた DIMENSION（回転寸法）にする
    fn dimension(&mut self, e: &zumen_jww::JwwDimension, frame: &Frame, source: &str) {
        let look = self.look(&e.base, frame, None);
        let block = self.out.dimension_block();
        let space = Space::Block(block.clone());
        let in_block = |look: Look| Look { layer: "0".into(), ..look };
        let line = &e.line;
        let (a, b) = (frame.point(line.x1, line.y1), frame.point(line.x2, line.y2));
        let line_look = in_block(self.look(&line.base, frame, None));
        self.out.add(&space, &line_look, "LINE", DxfEntityData::Line { start: xyz(a), end: xyz(b) }, "");
        let mut ends: Vec<Vec2> = Vec::new();
        for ext in &e.lines {
            if ext.x1 == ext.x2 && ext.y1 == ext.y2 {
                continue;
            }
            let ext_look = in_block(self.look(&ext.base, frame, None));
            let (p, q) = (frame.point(ext.x1, ext.y1), frame.point(ext.x2, ext.y2));
            self.out.add(&space, &ext_look, "LINE", DxfEntityData::Line { start: xyz(p), end: xyz(q) }, "");
            // 補助線の、寸法線から遠い端を測る点にする
            let distance = |point: Vec2| distance_to_line(point, a, b);
            ends.push(if distance(p) >= distance(q) { p } else { q });
        }
        for point in &e.points {
            if point.temporary {
                continue;
            }
            let point_look = in_block(self.look(&point.base, frame, None));
            let data = DxfEntityData::Point { location: xyz(frame.point(point.x, point.y)), x_axis_angle: 0.0 };
            self.out.add(&space, &point_look, "POINT", data, "");
        }
        let text_look = in_block(self.look(&e.text.base, frame, None));
        let center = self.text(&e.text, frame, &text_look, &space, "");
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let (ux, uy) = if length > 0.0 { ((b[0] - a[0]) / length, (b[1] - a[1]) / length) } else { (1.0, 0.0) };
        let first = ends.first().copied().unwrap_or(a);
        let second = ends.get(1).copied().unwrap_or(b);
        let dimension = DxfDimension {
            anonymous_block_name: block,
            dimtype: 32.0,
            defpoint: xyz(b),
            defpoint2: Some(xyz(first)),
            defpoint3: Some(xyz(second)),
            defpoint4: None,
            defpoint5: None,
            text_midpoint: xyz(center.unwrap_or([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])),
            insert: None,
            text: e.text.content.replace('\\', "\\\\").replace('{', "\\{").replace('}', "\\}"),
            actual_measurement: Some(((second[0] - first[0]) * ux + (second[1] - first[1]) * uy).abs()),
            attachment_point: Some(5.0),
            text_rotation: None,
            horizontal_direction: None,
            angle: Some(normalize(uy.atan2(ux).to_degrees())),
            oblique_angle: None,
            dimstyle: "Standard".into(),
        };
        self.out.add(&frame.space, &look, "DIMENSION", DxfEntityData::Dimension(dimension), source);
    }
}

/// ブロックの配置（JWW の倍率・回転、表示の Block と同じ）。INSERT の 挿入点・回転・X と Y の尺度と同じ
fn insert_matrix(x: f64, y: f64, scale_x: f64, scale_y: f64, rotation: f64) -> Matrix {
    let (cos, sin) = (js::cos(rotation), js::sin(rotation));
    [cos * scale_x, sin * scale_x, -sin * scale_y, cos * scale_y, x, y]
}

/// 点から、a と b を通る直線までの距離
fn distance_to_line(point: Vec2, a: Vec2, b: Vec2) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return (point[0] - a[0]).hypot(point[1] - a[1]);
    }
    ((point[0] - a[0]) * dy - (point[1] - a[1]) * dx).abs() / length
}
