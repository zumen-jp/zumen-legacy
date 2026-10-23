//! 図形（20.4）。モデルの図形を、読み取り（entities.rs）が同じ値に戻す DWG の値にする。DXF の度は DWG のラジアンに、
//! 名前で持つ表・スタイル・ブロックはハンドルにする。書けない図形・形を変えて書く図形は DXF の書き出しと同じ notices
//! で知らせる。その版が持てない値は DXF の書き出し（write_dxf_as）と同じ形にする。R2000 の図形は前後の図形のハンドルを
//! 持ち（20.4.1。前後がハンドル ± 1 なら Nolinks）、属性・頂点は持ち主が最初と最後を持つ

use super::tables::lineweight_index;
use super::{MODEL, PAPER, Writer, container, href, nearest_aci, ocs_axes, rad, rad_pair, upper};
use crate::bits::HandleRef;
use crate::codec::CmColor;
use crate::codec::entities::{
    AnnotContext, Arc, Attribute, Body, Circle, Content, ContextBlock, ContextText, Dimension, DimensionKind, Ellipse,
    EmbeddedMText, Entity, Face, Hatch, HatchBoundary, HatchEdge, HatchGradient, HatchPath, HatchPattern, HatchSpline,
    Image, Insert, Leader, LeaderLine, LeaderLineStyle, LeaderR14, LeaderRoot, Line, LwPolyline, MLeader, MLine,
    MLineVertex, MText, Owned, PatternLine, Point, Polyline2d, Polyline3d, PolylineMesh, PolylinePface, Ray, Solid,
    Spline, SplineControl, SplineFit, Text, Vertex2d, Viewport,
};
use crate::codec::object::{EntityColor, EntityCommon};
use crate::file::Version;
use std::collections::{HashMap, HashSet};
use zumen_core::NoticeLevel;
use zumen_dxf::{
    DxfDimension, DxfEmbeddedMText, DxfEntity, DxfEntityBase, DxfEntityData, DxfHatch, DxfHatchEdge, DxfImage,
    DxfMLeader, DxfMLine, DxfMText, DxfPolyline, DxfSpline, DxfTextBase, Vec3,
};

/// 図形の持ち主。モデル空間・紙空間（*Paper_Space）の図形は持ち主のハンドルを持たない（20.4.1 の図形のモード）
#[derive(Debug, Clone, Copy)]
pub(super) enum Owner {
    Model,
    Paper,
    Of(u64),
}

const Z: Vec3 = [0.0, 0.0, 1.0];

/// 色の生の値の BYBLOCK（0xC1000000。MULTILEADER の引出線の色の既定）
const BY_BLOCK_RAW: f64 = -1_056_964_608.0;

fn extrusion(base: &DxfEntityBase) -> Vec3 {
    base.extrusion.unwrap_or(Z)
}

fn thickness(base: &DxfEntityBase) -> f64 {
    base.thickness.unwrap_or(0.0)
}

fn xy(point: &Vec3) -> [f64; 2] {
    [point[0], point[1]]
}

/// 点がいちばん近い線分（path の i 番目から i + 1 番目）の番号（DXF の書き出しと同じ）
fn nearest_segment(path: &[Vec3], point: &Vec3) -> usize {
    let distance = |a: &Vec3, b: &Vec3| {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx * dx + dy * dy;
        let t = if length > 0.0 {
            (((point[0] - a[0]) * dx + (point[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (a[0] + t * dx - point[0]).hypot(a[1] + t * dy - point[1])
    };
    path.windows(2)
        .enumerate()
        .min_by(|(_, x), (_, y)| distance(&x[0], &x[1]).total_cmp(&distance(&y[0], &y[1])))
        .map_or(0, |(index, _)| index)
}

fn int(value: f64) -> i32 {
    zumen_core::js::to_int32(value)
}

fn short(value: f64) -> i16 {
    if value.is_finite() { value.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16 } else { 0 }
}

impl Writer<'_> {
    /// 図形のハンドル（書く前に決めたもの）
    fn handle_of(&mut self, base: &DxfEntityBase) -> u64 {
        match self.entity_handles.get(&(std::ptr::from_ref(base) as usize)) {
            Some(handle) => *handle,
            None => {
                let original = base.handle.clone();
                self.keep(&original)
            }
        }
    }

    fn layer_of(&self, name: &str) -> u64 {
        let name = if name.is_empty() { "0" } else { name };
        self.layer_handles.get(&upper(name)).or_else(|| self.layer_handles.get("0")).copied().unwrap_or(0)
    }

    fn style_of(&self, name: &str) -> HandleRef {
        let name = if name.is_empty() { "STANDARD" } else { name };
        href(
            5,
            self.style_handles.get(&upper(name)).or_else(|| self.style_handles.get("STANDARD")).copied().unwrap_or(0),
        )
    }

    fn dimstyle_of(&self, name: &str) -> HandleRef {
        let name = if name.is_empty() { "STANDARD" } else { name };
        href(
            5,
            self.dimstyle_handles
                .get(&upper(name))
                .or_else(|| self.dimstyle_handles.get("STANDARD"))
                .copied()
                .unwrap_or(0),
        )
    }

    fn block_of(&self, name: &str) -> HandleRef {
        href(5, self.block_index.get(&upper(name)).map_or(0, |index| self.blocks[*index].record))
    }

    /// 図形の共通の値（20.4.1）。色は ENC（0x8000 トゥルーカラー、0x2000 透過度）、線種は BYLAYER・BYBLOCK・
    /// CONTINUOUS の印か線種のハンドル、線の太さは番号
    pub(super) fn common(&self, base: &DxfEntityBase, owner: Owner) -> EntityCommon {
        let (mode, owner) = match owner {
            Owner::Model => (2, None),
            Owner::Paper => (1, None),
            Owner::Of(handle) => (0, Some(href(4, handle))),
        };
        let index = if base.color_index.is_finite() { base.color_index.clamp(0.0, 256.0) as u16 } else { 256 };
        let mut color = EntityColor::index(index);
        if self.before_2004() {
            // 2004 より前の版は色の番号だけ（トゥルーカラーは最も近い色の番号、透過度は書かない）
            if let Some(rgb) = base.true_color {
                color.value = u16::from(nearest_aci(int(rgb) as u32));
                self.version_colors.set(self.version_colors.get() + 1);
            }
            if base.transparency.is_some() {
                self.version_transparency.set(self.version_transparency.get() + 1);
            }
        } else {
            if let Some(rgb) = base.true_color {
                color.value = 0x8000 | (index & 0xff);
                color.rgb = Some(0xc200_0000 | (int(rgb) as u32 & 0xff_ffff));
            }
            if let Some(transparency) = base.transparency {
                color.value |= 0x2000;
                color.transparency = Some(int(transparency) as u32);
            }
        }
        let (mut ltype_flags, mut ltype) = match upper(&base.linetype).as_str() {
            "" | "BYLAYER" => (0, None),
            "BYBLOCK" => (1, None),
            "CONTINUOUS" => (2, None),
            name => match self.linetype_handles.get(name) {
                Some(handle) => (3, Some(href(5, *handle))),
                None => (0, None),
            },
        };
        if self.before_2000() {
            // R13・R14 は BYLAYER のほかの線種をハンドルで持ち（BYBLOCK・CONTINUOUS も）、線の太さを持たない
            if ltype_flags == 1 || ltype_flags == 2 {
                let key = if ltype_flags == 1 { "BYBLOCK" } else { "CONTINUOUS" };
                ltype = self.linetype_handles.get(key).map(|handle| href(5, *handle));
                ltype_flags = 3;
            }
            if base.lineweight >= 0.0 {
                self.version_lineweights.set(self.version_lineweights.get() + 1);
            }
        }
        EntityCommon {
            graphic: None,
            mode,
            owner,
            reactors: Vec::new(),
            xdictionary: None,
            ds_binary: false,
            // R2000 の前後の図形は書くときに決める（links）。前後のない図形（BLOCK・ENDBLK・SEQEND）は 0
            links: self.before_2004().then_some((href(4, 0), href(4, 0))),
            color,
            book_color: None,
            linetype_scale: base.linetype_scale,
            layer: href(5, self.layer_of(&base.layer)),
            ltype_flags,
            ltype,
            plotstyle_flags: 0,
            plotstyle: None,
            material_flags: 0,
            material: None,
            shadow: 0,
            visual_styles: [None; 3],
            invisible: i16::from(base.invisible),
            lineweight: lineweight_index(base.lineweight),
        }
    }

    /// BLOCK・ENDBLK などの、レイヤ 0・BYLAYER の共通の値
    fn plain_common(&self, owner: Owner) -> EntityCommon {
        let base = DxfEntityBase {
            kind: String::new(),
            handle: String::new(),
            ordinal: 0,
            owner: None,
            space: zumen_dxf::DxfSpace::Model,
            block: None,
            layer: "0".into(),
            linetype: "BYLAYER".into(),
            color_index: 256.0,
            true_color: None,
            transparency: None,
            lineweight: -1.0,
            linetype_scale: 1.0,
            invisible: false,
            thickness: None,
            extrusion: None,
        };
        self.common(&base, owner)
    }

    fn put(&mut self, kind: u16, handle: u64, common: EntityCommon, body: Body) {
        self.put_with(kind, handle, common, body, Vec::new());
    }

    /// 拡張データ（APPID のハンドル, 値のバイト列）を持つ図形を書く
    fn put_with(&mut self, kind: u16, handle: u64, common: EntityCommon, body: Body, eed: Vec<(u64, Vec<u8>)>) {
        let mut out = self.object(kind, handle);
        out.eed = eed;
        Entity { common, body }.write(&mut out);
        self.push(&out);
    }

    /// R14 より前・2000 より前の版で、決まった型のない図形（LWPOLYLINE・HATCH は R14 ではクラスの型。AutoCAD の図面と同じ）
    fn kind_of(&mut self, fixed: u16, class: &'static str) -> u16 {
        if self.before_2000() { self.class_number(class) } else { fixed }
    }

    /// R2000 の図形の前と次の図形（20.4.1）。前後がハンドル ± 1 なら Nolinks（AutoCAD と同じ）、ないものは 0
    fn links(&self, handle: u64, previous: u64, next: u64) -> Option<(HandleRef, HandleRef)> {
        if !self.before_2004() {
            return None;
        }
        if previous != 0 && previous.checked_add(1) == Some(handle) && handle.checked_add(1) == Some(next) {
            return None;
        }
        Some((href(4, previous), href(4, next)))
    }

    /// R2000 のつながり（持ち主の最初から最後まで）のうち index 番目の前後の図形
    fn chained(&self, chain: &[u64], index: usize) -> Option<(HandleRef, HandleRef)> {
        let previous = index.checked_sub(1).map_or(0, |at| chain[at]);
        let next = chain.get(index + 1).copied().unwrap_or(0);
        self.links(chain[index], previous, next)
    }

    /// 書く図形か（R13 の画像、画像の定義のファイルが分からない画像と、表示のブロックを持たない寸法は書かない）。
    /// R2000 の図形のつながりを先に決めるのに使う
    fn writes(&self, entity: &DxfEntity) -> bool {
        if entity.base.kind.eq_ignore_ascii_case("IMAGE") {
            return self.version >= Version::R14
                && matches!(&entity.data, DxfEntityData::Image(image) if image.path.is_some());
        }
        !matches!(&entity.data, DxfEntityData::Dimension(dimension) if dimension.anonymous_block_name.is_empty())
    }

    /// 図形をブロックごとに、モデルの順に書く（BLOCK・図形・ENDBLK）
    pub(super) fn write_entities(&mut self) {
        let doc = self.doc;
        let mut contents: HashMap<String, Vec<&DxfEntity>> = HashMap::new();
        for entity in &doc.entities {
            contents.entry(container(entity)).or_default().push(entity);
        }
        for index in 0..self.blocks.len() {
            let key = upper(&self.blocks[index].name);
            let owner = match key.as_str() {
                MODEL => Owner::Model,
                PAPER => Owner::Paper,
                _ => Owner::Of(self.blocks[index].record),
            };
            let (begin, end) = (self.blocks[index].begin, self.blocks[index].end);
            let name = self.text(&self.blocks[index].name.clone());
            self.put(4, begin, self.plain_common(owner), Body::Block(name));
            let entities: Vec<&DxfEntity> = contents.get(&key).into_iter().flatten().copied().collect();
            // R2000 の図形は前後の図形を持つので、書く図形のハンドルを先に並べる
            let written: Vec<&DxfEntity> = entities.iter().copied().filter(|entity| self.writes(entity)).collect();
            let chain: Vec<u64> = written.iter().map(|entity| self.handle_of(&entity.base)).collect();
            for entity in entities {
                let position = self.blocks[index].entities.len();
                let links = if chain.get(position).is_some() { self.chained(&chain, position) } else { None };
                if let Some(handle) = self.entity(entity, owner, links) {
                    self.blocks[index].entities.push(handle);
                }
            }
            self.put(5, end, self.plain_common(owner), Body::Empty);
        }
        self.plan_sort_tables();
    }

    /// 描画順（モデルの sort_handles）を、書いたブロック・図形のハンドルで決める
    fn plan_sort_tables(&mut self) {
        let doc = self.doc;
        for (name, handles) in &doc.sort_handles {
            let Some(&index) = self.block_index.get(&upper(name)) else { continue };
            let written: HashSet<u64> = self.blocks[index].entities.iter().copied().collect();
            let mut entries: Vec<(u64, u64)> = handles
                .iter()
                .filter_map(|(entity, sort)| {
                    let entity = self.mapped(entity)?;
                    let sort = u64::from_str_radix(sort, 16).ok()?;
                    written.contains(&entity).then_some((sort, entity))
                })
                .collect();
            if entries.is_empty() {
                continue;
            }
            entries.sort_by_key(|(_, entity)| *entity);
            let (dictionary, table) = (self.handles.fresh(), self.handles.fresh());
            self.sort_tables.push((index, dictionary, table, entries));
        }
    }

    /// 1 つの図形を書く。links は R2000 の前後の図形（Writer::links）。書いたハンドル（書かなかった図形は None）
    fn entity(&mut self, entity: &DxfEntity, owner: Owner, links: Option<(HandleRef, HandleRef)>) -> Option<u64> {
        let base = &entity.base;
        let kind = upper(&base.kind);
        if kind == "IMAGE" && !self.writes(entity) {
            if matches!(&entity.data, DxfEntityData::Image(image) if image.path.is_some()) {
                self.notices.add(
                    NoticeLevel::Info,
                    "version-image",
                    "画像（IMAGE）は、AutoCAD R13 にないため、省きました",
                );
            } else {
                self.notices.add(
                    NoticeLevel::Warning,
                    "image",
                    "画像（IMAGE）は、画像のファイル（IMAGEDEF）が分からないため省きました",
                );
            }
            return None;
        }
        if let DxfEntityData::Dimension(dimension) = &entity.data
            && dimension.anonymous_block_name.is_empty()
        {
            // 表示のブロックを持たない寸法（AutoCAD の拘束寸法など）は書かない。表示でも省いている
            self.notices.add(NoticeLevel::Warning, "dimension-block", "表示のブロックがない寸法を省きました");
            return None;
        }
        let handle = self.handle_of(base);
        let mut common = self.common(base, owner);
        common.links = links;
        match &entity.data {
            DxfEntityData::Line { start, end } => {
                let body = Line { start: *start, end: *end, thickness: thickness(base), extrusion: extrusion(base) };
                self.put(19, handle, common, Body::Line(body));
            }
            DxfEntityData::Circle { center, radius } => {
                let body =
                    Circle { center: *center, radius: *radius, thickness: thickness(base), extrusion: extrusion(base) };
                self.put(18, handle, common, Body::Circle(body));
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                let circle =
                    Circle { center: *center, radius: *radius, thickness: thickness(base), extrusion: extrusion(base) };
                let (start, end) = rad_pair(*start_angle, *end_angle);
                let body = Arc { circle, start_angle: start, end_angle: end };
                self.put(17, handle, common, Body::Arc(body));
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                let body = Ellipse {
                    center: *center,
                    major_axis: *major_axis,
                    extrusion: extrusion(base),
                    ratio: *ratio,
                    start: *start_param,
                    end: *end_param,
                };
                self.put(35, handle, common, Body::Ellipse(body));
            }
            DxfEntityData::Point { location, x_axis_angle } => {
                let body = Point {
                    location: *location,
                    thickness: thickness(base),
                    extrusion: extrusion(base),
                    x_axis_angle: rad(*x_axis_angle),
                };
                self.put(27, handle, common, Body::Point(body));
            }
            DxfEntityData::LwPolyline(polyline) if self.version < Version::R14 => {
                // R13 は LWPOLYLINE を持たない（R14 から）ので、DXF の R12 の書き出しと同じく POLYLINE（2D）にする
                let mut vertices = polyline.vertices.clone();
                let elevation = polyline.elevation.unwrap_or(0.0);
                for point in &mut vertices.points {
                    point[2] = elevation;
                }
                if let Some(width) = polyline.const_width {
                    vertices.widths = vec![[width, width]; vertices.points.len()];
                }
                vertices.flags = f64::from(int(vertices.flags) & 128);
                let converted =
                    DxfPolyline { vertices, vertex_flags: Vec::new(), faces: None, m_count: None, n_count: None };
                self.polyline(base, &converted, handle, common);
            }
            DxfEntityData::LwPolyline(polyline) => {
                let vertices = &polyline.vertices;
                let dxf_flags = int(vertices.flags);
                let closed = vertices.closed || dxf_flags & 1 != 0;
                let mut flags = 0i16;
                if closed {
                    flags |= 512;
                }
                if dxf_flags & 128 != 0 {
                    flags |= 256;
                }
                let const_width = polyline.const_width;
                if const_width.is_some() {
                    flags |= 4;
                }
                let elevation = polyline.elevation.filter(|value| *value != 0.0);
                if elevation.is_some() {
                    flags |= 8;
                }
                if thickness(base) != 0.0 {
                    flags |= 2;
                }
                if extrusion(base) != Z {
                    flags |= 1;
                }
                let count = vertices.points.len();
                let bulges: Vec<f64> =
                    (0..count).map(|index| vertices.bulges.get(index).copied().unwrap_or(0.0)).collect();
                if bulges.iter().any(|bulge| *bulge != 0.0) {
                    flags |= 16;
                }
                let widths: Vec<[f64; 2]> =
                    (0..count).map(|index| vertices.widths.get(index).copied().unwrap_or([0.0, 0.0])).collect();
                let has_widths = const_width.is_none() && widths.iter().any(|width| *width != [0.0, 0.0]);
                if has_widths {
                    flags |= 32;
                }
                let body = LwPolyline {
                    flags,
                    const_width: const_width.unwrap_or(0.0),
                    elevation: elevation.unwrap_or(0.0),
                    thickness: thickness(base),
                    extrusion: extrusion(base),
                    points: vertices.points.iter().map(xy).collect(),
                    bulges: if flags & 16 != 0 { bulges } else { Vec::new() },
                    ids: Vec::new(),
                    widths: if has_widths { widths } else { Vec::new() },
                };
                let kind = self.kind_of(77, "LWPOLYLINE");
                self.put(kind, handle, common, Body::LwPolyline(body));
            }
            DxfEntityData::Polyline(polyline) => self.polyline(base, polyline, handle, common),
            DxfEntityData::Spline(spline) => {
                if kind == "HELIX" {
                    self.notices.add(
                        NoticeLevel::Info,
                        "helix",
                        "らせん（HELIX）は、スプライン（SPLINE）として書きました",
                    );
                }
                self.put(36, handle, common, Body::Spline(spline_body(spline, self.version)));
            }
            DxfEntityData::Text(text) => {
                let body = self.text_body(base, text);
                self.put(1, handle, common, Body::Text(body));
            }
            DxfEntityData::Attrib { text, mtext, .. } => {
                // ブロックの配置に属さない属性は、DXF の書き出しと同じく文字として書く（複数行の属性は MTEXT）。
                // 配置の属性は配置（INSERT）を書くときに書く
                self.notices.add(
                    NoticeLevel::Info,
                    "attrib-alone",
                    "ブロック参照に属さない属性（ATTRIB）は、文字（TEXT・MTEXT）として書きました",
                );
                match mtext {
                    Some(embedded) => {
                        let body = self.mtext_body(&embedded.content, embedded.extrusion);
                        self.put(44, handle, common, Body::MText(Box::new(body)));
                    }
                    None => {
                        let body = self.text_body(base, text);
                        self.put(1, handle, common, Body::Text(body));
                    }
                }
            }
            DxfEntityData::Attdef { text, tag, prompt, attribute_flags, mtext } => {
                let body = self.attribute(base, text, tag, *attribute_flags, mtext.as_deref(), Some(prompt), &common);
                self.put(3, handle, common, Body::Attdef(body));
            }
            DxfEntityData::MText(mtext) => {
                let body = self.mtext_body(mtext, base.extrusion);
                self.put(44, handle, common, Body::MText(Box::new(body)));
            }
            DxfEntityData::Insert(insert) => {
                let array = insert.column_count > 1.0 || insert.row_count > 1.0;
                let scale = [insert.xscale, insert.yscale, insert.zscale];
                let mut attributes = Vec::new();
                let mut seqend = None;
                if !insert.attribs.is_empty() {
                    // R2000 の属性は属性どうしでつながり、配置が最初と最後を持つ
                    let chain: Vec<u64> = insert
                        .attribs
                        .iter()
                        .filter(|attrib| matches!(attrib.data, DxfEntityData::Attrib { .. }))
                        .map(|attrib| self.handle_of(&attrib.base))
                        .collect();
                    for attrib in &insert.attribs {
                        let attrib_handle = self.handle_of(&attrib.base);
                        if let DxfEntityData::Attrib { text, tag, attribute_flags, mtext } = &attrib.data {
                            let mut attrib_common = self.common(&attrib.base, Owner::Of(handle));
                            attrib_common.links = self.chained(&chain, attributes.len());
                            let body = self.attribute(
                                &attrib.base,
                                text,
                                tag,
                                *attribute_flags,
                                mtext.as_deref(),
                                None,
                                &attrib_common,
                            );
                            self.put(2, attrib_handle, attrib_common, Body::Attrib(body));
                            attributes.push(href(3, attrib_handle));
                        }
                    }
                    let seqend_handle = self.handles.fresh();
                    let seqend_common = self.common(base, Owner::Of(handle));
                    self.put(6, seqend_handle, seqend_common, Body::Empty);
                    seqend = Some(href(3, seqend_handle));
                    if self.before_2004() {
                        let mut list = chain;
                        list.push(seqend_handle);
                        self.followers.insert(handle, list);
                    }
                }
                let body = Insert {
                    point: insert.insert,
                    scale,
                    scale_flags: Insert::scale_flags_of(scale),
                    rotation: rad(insert.rotation),
                    extrusion: extrusion(base),
                    has_attributes: seqend.is_some(),
                    array: array.then(|| {
                        (
                            short(insert.column_count.max(1.0)),
                            short(insert.row_count.max(1.0)),
                            insert.column_spacing,
                            insert.row_spacing,
                        )
                    }),
                    block: self.block_of(&insert.name),
                    first_last: Some((
                        attributes.first().map_or(href(4, 0), |first| href(4, first.value)),
                        attributes.last().map_or(href(4, 0), |last| href(4, last.value)),
                    )),
                    attributes,
                    seqend,
                };
                if let Some(&index) = self.block_index.get(&upper(&insert.name)) {
                    self.blocks[index].inserts.push(handle);
                }
                self.put(if array { 8 } else { 7 }, handle, common, Body::Insert(body));
            }
            DxfEntityData::Dimension(dimension)
                if self.before_2004() && matches!(kind.as_str(), "ARC_DIMENSION" | "LARGE_RADIAL_DIMENSION") =>
            {
                // 円弧の長さ・大きい半径の寸法は 2004 から。前の版は寸法のブロックの参照にする（DXF の書き出しと同じ）
                self.notices.add(
                    NoticeLevel::Info,
                    "version-dimension",
                    "円弧の長さ・大きい半径の寸法は、AutoCAD 2004 より前の版にないため、寸法のブロックの参照（INSERT）として書きました",
                );
                let one = [1.0, 1.0, 1.0];
                let body = Insert {
                    point: [0.0; 3],
                    scale: one,
                    scale_flags: Insert::scale_flags_of(one),
                    rotation: 0.0,
                    extrusion: Z,
                    has_attributes: false,
                    array: None,
                    block: self.block_of(&dimension.anonymous_block_name),
                    attributes: Vec::new(),
                    first_last: None,
                    seqend: None,
                };
                if let Some(&index) = self.block_index.get(&upper(&dimension.anonymous_block_name)) {
                    self.blocks[index].inserts.push(handle);
                }
                self.put(7, handle, common, Body::Insert(body));
            }
            DxfEntityData::Dimension(dimension) => {
                let (kind_number, body) = self.dimension(&kind, base, dimension);
                self.put(kind_number, handle, common, Body::Dimension(body));
            }
            DxfEntityData::Hatch(hatch) => {
                if kind == "MPOLYGON" {
                    self.notices.add(NoticeLevel::Info, "mpolygon", "MPOLYGON は、ハッチング（HATCH）として書きました");
                }
                let body = if hatch.gradient.is_some() && self.before_2004() {
                    // グラデーションは 2004 から。前の版は 1 色目の塗りにする（DXF の書き出しと同じ）
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-gradient",
                        "グラデーションは、AutoCAD 2004 より前の版にないため、1 色目の塗りで書きました",
                    );
                    if let Some(first) = hatch.gradient_colors.as_deref().and_then(<[f64]>::first) {
                        common.color = EntityColor::index(u16::from(nearest_aci(int(*first) as u32)));
                    }
                    self.hatch(base, &DxfHatch { gradient: None, solid_fill: true, ..hatch.clone() })
                } else {
                    self.hatch(base, hatch)
                };
                if self.version < Version::R14 {
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-hatch",
                        "ハッチング（HATCH）は AutoCAD R14 からで、R13 の AutoCAD では代理オブジェクトとして扱われ表示されません",
                    );
                }
                let kind = self.kind_of(78, "HATCH");
                self.put(kind, handle, common, Body::Hatch(Box::new(body)));
            }
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges } => match kind.as_str() {
                "3DFACE" => {
                    let invisible = invisible_edges.map(short).filter(|flags| *flags != 0);
                    let body = Face { corners: [*vtx0, *vtx1, *vtx2, *vtx3], invisible };
                    self.put(28, handle, common, Body::Face(body));
                }
                _ => {
                    let body = Solid {
                        thickness: thickness(base),
                        elevation: vtx0[2],
                        corners: [xy(vtx0), xy(vtx1), xy(vtx2), xy(vtx3)],
                        extrusion: extrusion(base),
                    };
                    self.put(if kind == "TRACE" { 32 } else { 31 }, handle, common, Body::Solid(body));
                }
            },
            DxfEntityData::Leader(leader) => {
                let dimstyle = self.dimstyle_of(&leader.dimstyle);
                // R13・R14 は作ったときの DIMGAP・DIMASZ に DIMSCALE を掛けた値を持つ（20.4.47）。書いた寸法スタイルの値と、
                // ほかは AutoCAD の R14 の図面の値
                let r14 = self.before_2000().then(|| {
                    let (scale, arrow) = self
                        .dimstyles
                        .iter()
                        .find(|(_, handle, _)| *handle == dimstyle.value)
                        .and_then(|(_, _, source)| *source)
                        .map_or((1.0, 2.5), |style| (style.dimscale, style.dimasz));
                    let gap = match super::header::dim_default("DIMGAP", self.version) {
                        crate::codec::tables::DimValue::Double(gap) => gap,
                        _ => 0.0,
                    };
                    LeaderR14 {
                        dimgap: gap * scale,
                        arrowhead_type: 0,
                        dimasz: arrow * scale,
                        unknown: (false, true, 256),
                        byblock_color: 256,
                        unknown_bits: [false, false],
                    }
                });
                let body = Leader {
                    unknown_bit: false,
                    annotation_type: short(leader.annotation_type),
                    path_type: short(leader.path_type),
                    points: leader.points.clone(),
                    origin: leader.points.first().copied().unwrap_or([0.0; 3]),
                    extrusion: extrusion(base),
                    x_direction: [1.0, 0.0, 0.0],
                    block_offset: [0.0; 3],
                    end_projection: [0.0; 3],
                    box_size: None,
                    hook_on_x: false,
                    arrowhead: leader.has_arrowhead,
                    unknown_short: 0,
                    unknown_bits: [false, false],
                    r14,
                    annotation: href(5, 0),
                    dimstyle,
                };
                self.put(45, handle, common, Body::Leader(body));
            }
            DxfEntityData::MLeader(mleader) => {
                if self.version < Version::R2007 {
                    // DXF の書き出しと同じく書く（AutoCAD 2008 より前の AutoCAD は代理オブジェクトとして扱う）
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-mleader",
                        "マルチ引出線（MULTILEADER）は AutoCAD 2008 からで、それより前の版を読む CAD では表示されないことがあります",
                    );
                }
                let body = self.mleader(mleader);
                let number = self.class_number("MULTILEADER");
                self.put(number, handle, common, Body::MLeader(Box::new(body)));
            }
            DxfEntityData::MLine(mline) => {
                let body = self.mline(base, mline);
                self.put(47, handle, common, Body::MLine(body));
            }
            DxfEntityData::Viewport(viewport) => {
                let frozen = viewport
                    .frozen_layer_handles
                    .iter()
                    .filter_map(|frozen| self.mapped(frozen))
                    .filter(|handle| self.layer_handles.values().any(|known| known == handle))
                    .chain(
                        viewport
                            .frozen_layer_names
                            .iter()
                            .filter_map(|name| self.layer_handles.get(&upper(name)).copied()),
                    )
                    .map(|handle| href(5, handle))
                    .collect();
                let clip = viewport.clipping_boundary_handle.as_deref().and_then(|clip| self.mapped(clip)).unwrap_or(0);
                // R2000 は紙空間（*Paper_Space）のビューポートに VIEWPORT ENTITY HEADER を置く（ほかのレイアウトは 0。
                // AutoCAD と同じ）
                let vp_header = self.before_2004().then(|| {
                    let header = if matches!(owner, Owner::Paper) {
                        let header = self.handles.fresh();
                        self.viewport_headers.push((handle, header));
                        header
                    } else {
                        0
                    };
                    href(5, header)
                });
                let body = Viewport {
                    center: viewport.center,
                    width: viewport.width,
                    height: viewport.height,
                    view_target: viewport.view_target,
                    view_direction: viewport.view_direction,
                    twist: rad(viewport.view_twist_angle),
                    view_height: viewport.view_height,
                    lens: 50.0,
                    front_clip: 0.0,
                    back_clip: 0.0,
                    snap_angle: 0.0,
                    view_center: viewport.view_center,
                    snap_base: [0.0, 0.0],
                    snap_spacing: [10.0, 10.0],
                    grid_spacing: [10.0, 10.0],
                    circle_zoom: 1000,
                    grid_major: 5,
                    status_flags: int(viewport.status_flags),
                    style_sheet: self.empty(),
                    render_mode: 0,
                    ucs_at_origin: true,
                    ucs_per_viewport: true,
                    ucs_origin: [0.0; 3],
                    ucs_x: [1.0, 0.0, 0.0],
                    ucs_y: [0.0, 1.0, 0.0],
                    ucs_elevation: 0.0,
                    ucs_ortho: 0,
                    shade_plot_mode: 0,
                    default_lights: true,
                    lighting_type: 1,
                    brightness: 0.0,
                    contrast: 0.0,
                    ambient: Some(CmColor { index: 0, rgb: 0xc233_3333, name: None, book: None }),
                    frozen_layers: frozen,
                    clip_boundary: href(5, clip),
                    vp_header,
                    named_ucs: href(5, 0),
                    base_ucs: href(5, 0),
                    r2007_handles: Some([href(4, 0), href(5, 0), href(5, 0), href(3, 0)]),
                };
                self.viewports.insert(handle);
                let mut eed = Vec::new();
                if self.before_2000() {
                    // R13・R14 は表示の値を拡張データ ACAD の MVIEW に持つ（DXF の R12 と同じ並び）。切り抜きの境界はない
                    if clip != 0 {
                        self.notices.add(
                            NoticeLevel::Info,
                            "version-viewport-clip",
                            "ビューポートの非矩形の切り抜きは、AutoCAD 2000 より前の版にないため、矩形の表示範囲にしました",
                        );
                    }
                    let frozen: Vec<u64> = body.frozen_layers.iter().map(|layer| layer.value).collect();
                    if let Some(acad) = self.fixed.appids.iter().find(|(name, _)| *name == "ACAD").map(|(_, app)| *app)
                    {
                        eed.push((acad, self.mview(viewport, &frozen)));
                    }
                }
                self.put_with(34, handle, common, Body::Viewport(Box::new(body)), eed);
            }
            DxfEntityData::XLine { start, unit_vector } => {
                let body = Ray { point: *start, vector: *unit_vector };
                self.put(if kind == "RAY" { 40 } else { 41 }, handle, common, Body::Ray(body));
            }
            DxfEntityData::Image(image) if kind == "IMAGE" => {
                if image.clip_inverted && self.version < Version::R2010 {
                    self.notices.add(
                        NoticeLevel::Info,
                        "version-image-clip",
                        "画像の境界の内側を消す切り抜きは、AutoCAD 2010 より前の版にないため、内側を描く切り抜きにしました",
                    );
                }
                let (definition, reactor) = self.plan_image(image, handle);
                let mut body = wipeout(image);
                if body.boundary.is_empty() {
                    // 境界がなければ画像の全体の矩形（画素の座標）
                    body.boundary = vec![[-0.5, -0.5], [image.image_size[0] - 0.5, image.image_size[1] - 0.5]];
                    body.clip_type = 1;
                }
                body.imagedef = href(5, definition);
                body.reactor = href(3, reactor);
                let number = self.class_number("IMAGE");
                self.put(number, handle, common, Body::Image(Box::new(body)));
            }
            DxfEntityData::Image(image) => {
                let body = wipeout(image);
                let number = self.class_number("WIPEOUT");
                self.put(number, handle, common, Body::Image(Box::new(body)));
            }
            DxfEntityData::Table(table) => {
                // 表（ACAD_TABLE）は、表示の図形を持つブロックの配置として書く
                self.notices.add(
                    NoticeLevel::Info,
                    "table",
                    "表（ACAD_TABLE）は、表示の図形を持つブロックの参照（INSERT）として書きました",
                );
                let scale = [table.xscale, table.yscale, table.zscale];
                let body = Insert {
                    point: table.insert,
                    scale,
                    scale_flags: Insert::scale_flags_of(scale),
                    rotation: rad(table.rotation),
                    extrusion: extrusion(base),
                    has_attributes: false,
                    array: None,
                    block: self.block_of(&table.name),
                    attributes: Vec::new(),
                    first_last: None,
                    seqend: None,
                };
                if let Some(&index) = self.block_index.get(&upper(&table.name)) {
                    self.blocks[index].inserts.push(handle);
                }
                self.put(7, handle, common, Body::Insert(body));
            }
        }
        Some(handle)
    }

    /// R13・R14 のビューポートの拡張データ MVIEW（28 の形。AutoCAD の R14 の図面と同じ並び）: { 版 16、目標・向き、
    /// ねじれ（度）・高さ・中心・レンズの長さ・前と後ろの切り取り、表示の設定・円の滑らかさ・高速ズーム・UCS アイコン・
    /// スナップ・グリッド・スナップの種類と面、スナップの角度・基点・間隔、グリッドの間隔、印刷で隠す、{ 凍結したレイヤ } }
    fn mview(&self, viewport: &zumen_dxf::DxfViewport, frozen: &[u64]) -> Vec<u8> {
        let mut out = Vec::new();
        self.eed_string(&mut out, "MVIEW");
        let control = |out: &mut Vec<u8>, close: bool| out.extend_from_slice(&[2, u8::from(close)]);
        let short = |out: &mut Vec<u8>, value: i16| {
            out.push(70);
            out.extend_from_slice(&value.to_le_bytes());
        };
        let real = |out: &mut Vec<u8>, value: f64| {
            out.push(40);
            out.extend_from_slice(&value.to_le_bytes());
        };
        control(&mut out, false);
        short(&mut out, 16);
        for point in [viewport.view_target, viewport.view_direction] {
            out.push(10);
            for value in point {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        for value in [
            viewport.view_twist_angle,
            viewport.view_height,
            viewport.view_center[0],
            viewport.view_center[1],
            50.0,
            0.0,
            0.0,
        ] {
            real(&mut out, value);
        }
        for value in [0, 1000, 1, 1, 0, 0, 0, 0] {
            short(&mut out, value);
        }
        for value in [0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.5] {
            real(&mut out, value);
        }
        short(&mut out, 0);
        control(&mut out, false);
        for layer in frozen {
            out.push(3);
            out.extend_from_slice(&layer.to_be_bytes());
        }
        control(&mut out, true);
        control(&mut out, true);
        out
    }

    /// TEXT・ATTRIB・ATTDEF の 1 行の文字（20.4.3）。既定の値（0、1）の値は DataFlags で省く（AutoCAD と同じ）
    fn text_body(&self, base: &DxfEntityBase, text: &DxfTextBase) -> Text {
        let nonzero = |value: f64| (value != 0.0).then_some(value);
        // DXF の書き出しと同じく、揃えのある文字で第 2 整列点がなければ第 1 整列点を書く
        let align = match text.align_point {
            Some(point) => Some(xy(&point)),
            None if text.halign != 0.0 || text.valign != 0.0 => Some(xy(&text.insert)),
            None => None,
        };
        Text {
            elevation: nonzero(text.insert[2]),
            insert: xy(&text.insert),
            align,
            extrusion: extrusion(base),
            thickness: thickness(base),
            oblique: nonzero(rad(text.oblique)),
            rotation: nonzero(rad(text.rotation)),
            height: text.height,
            width: (text.width != 1.0 && text.width > 0.0).then_some(text.width),
            value: self.text(&text.text),
            generation: nonzero(text.text_generation_flag).map(short),
            halign: nonzero(text.halign).map(short),
            valign: nonzero(text.valign).map(short),
            style: self.style_of(&text.style),
        }
    }

    /// ATTRIB・ATTDEF（20.4.4、20.4.5）。複数行の属性は R2018 から埋め込みの MTEXT（種類 2・4）として書く。共通の値は
    /// 属性と同じ（読み取りも属性の色・レイヤを使う）。それより前の版は複数行の属性を持てないので、MTEXT の文字を 1 行に
    /// した属性として書く。common は属性の共通の値
    #[allow(clippy::too_many_arguments)]
    fn attribute(
        &mut self,
        base: &DxfEntityBase,
        text: &DxfTextBase,
        tag: &str,
        flags: f64,
        mtext: Option<&DxfEmbeddedMText>,
        prompt: Option<&str>,
        common: &EntityCommon,
    ) -> Attribute {
        let mut body = self.text_body(base, text);
        let mut attribute_type = 1;
        let mut embedded = None;
        if let Some(content) = mtext {
            if self.version >= Version::R2018 {
                attribute_type = if prompt.is_some() { 4 } else { 2 };
                let mut common = common.clone();
                common.graphic = None;
                common.links = None;
                embedded = Some(EmbeddedMText {
                    common,
                    mtext: Box::new(self.mtext_body(&content.content, content.extrusion)),
                    annotative: None,
                });
            } else {
                self.notices.add(
                    NoticeLevel::Info,
                    "multiline-attrib",
                    format!(
                        "複数行の属性は、AutoCAD {} の DWG に書けない（2018 から）ため 1 行の属性として書きました",
                        self.target.name()
                    ),
                );
                // AutoCAD は複数行の属性の 1 行の文字を空にするので、MTEXT の文字を 1 行にして使う
                if text.text.is_empty() {
                    body.value = self.text(&content.content.text.replace(['\n', '\r'], " "));
                }
            }
        }
        Attribute {
            text: body,
            version: 0,
            attribute_type,
            embedded,
            tag: self.text(tag),
            field_length: 0,
            flags: int(flags) as u8,
            lock: false,
            prompt: prompt.map(|prompt| (0, self.text(prompt))),
        }
    }

    /// MTEXT の値（20.4.46）。向きがなければ回転（度、OCS）から WCS の向きを求める。背景は DXF の書き出しと同じく 2007 から
    fn mtext_body(&mut self, mtext: &DxfMText, extrusion: Option<Vec3>) -> MText {
        let normal = extrusion.unwrap_or(Z);
        let direction = match mtext.text_direction {
            Some(direction) => direction,
            None if mtext.rotation == 0.0 => [1.0, 0.0, 0.0],
            None => {
                let (ax, ay) = ocs_axes(normal);
                let (sin, cos) = rad(mtext.rotation).sin_cos();
                [cos * ax[0] + sin * ay[0], cos * ax[1] + sin * ay[1], cos * ax[2] + sin * ay[2]]
            }
        };
        let raw = if mtext.raw_text.is_empty() { mtext.text.replace('\n', "\\P") } else { mtext.raw_text.clone() };
        let mut background_flags = int(mtext.background_flags);
        if background_flags != 0 && self.version < Version::R2007 {
            self.notices.add(
                NoticeLevel::Info,
                "version-background",
                "複数行の文字の背景は、AutoCAD 2007 より前の版にないため、書きませんでした",
            );
            background_flags = 0;
        }
        let background = (background_flags & 1 != 0).then(|| {
            (
                mtext.background_scale.unwrap_or(1.5),
                self.cmc(mtext.background_color_index.unwrap_or(256.0), mtext.background_true_color),
                0,
            )
        });
        let line_spacing_style = if mtext.line_spacing_style > 0.0 { short(mtext.line_spacing_style) } else { 1 };
        let line_spacing_factor = if mtext.line_spacing_factor > 0.0 { mtext.line_spacing_factor } else { 1.0 };
        // 行間（20.4.46 の R2000+）。R13・R14 の MTEXT は既定の行間（少なくとも、1 倍）で表示される
        if self.before_2000() && (line_spacing_style != 1 || line_spacing_factor != 1.0) {
            self.notices.add(
                NoticeLevel::Info,
                "version-line-spacing",
                "複数行の文字の行間は、AutoCAD 2000 より前の版にないため、書きませんでした（既定の行間で表示されます）",
            );
        }
        MText {
            insert: mtext.insert,
            extrusion: normal,
            direction,
            rect_width: mtext.rect_width,
            rect_height: Some(0.0),
            char_height: mtext.char_height,
            attachment: if mtext.attachment_point >= 1.0 { short(mtext.attachment_point) } else { 1 },
            drawing_direction: if mtext.drawing_direction > 0.0 { short(mtext.drawing_direction) } else { 1 },
            extents_height: 0.0,
            extents_width: 0.0,
            text: self.text(&raw),
            style: self.style_of(&mtext.style),
            line_spacing_style,
            line_spacing_factor,
            unknown_bit: false,
            background_flags,
            background,
            not_annotative: None,
            annotative_flag: None,
        }
    }

    /// POLYLINE（2D・3D・ポリフェースメッシュ・ポリゴンメッシュ）と頂点、SEQEND。頂点は POLYLINE の共通の値を持つ
    fn polyline(&mut self, base: &DxfEntityBase, polyline: &DxfPolyline, handle: u64, common: EntityCommon) {
        let vertices = &polyline.vertices;
        let flags = (int(vertices.flags) & !1) | i32::from(vertices.closed);
        let polyface = flags & 64 != 0;
        let mesh = flags & 16 != 0;
        let three_d = flags & 8 != 0;
        let sub = self.common(base, Owner::Of(handle));
        // 頂点（ポリフェースメッシュは面の記録も）のハンドル。R2000 の頂点は頂点どうしでつながり、POLYLINE が最初と最後を持つ
        let faces = if polyface { polyline.faces.as_ref().map_or(0, Vec::len) } else { 0 };
        let chain: Vec<u64> = (0..vertices.points.len() + faces).map(|_| self.handles.fresh()).collect();
        let linked = |writer: &Self, index: usize| {
            let mut common = sub.clone();
            common.links = writer.chained(&chain, index);
            common
        };
        let mut owned = Vec::new();
        for (index, point) in vertices.points.iter().enumerate() {
            let vertex = chain[index];
            owned.push(href(3, vertex));
            let default = if polyface {
                192.0
            } else if mesh {
                64.0
            } else if three_d {
                32.0
            } else {
                0.0
            };
            let vertex_flags = polyline.vertex_flags.get(index).copied().unwrap_or(default);
            let vertex_flags = int(vertex_flags) as u8;
            if polyface || mesh || three_d {
                let kind = if polyface {
                    13
                } else if mesh {
                    12
                } else {
                    11
                };
                self.put(kind, vertex, linked(self, index), Body::Vertex3d(vertex_flags, *point));
            } else {
                let [start, end] = vertices.widths.get(index).copied().unwrap_or([0.0, 0.0]);
                // 始めと終わりが同じ 0 でない幅は、始めの幅を負にして終わりを省く（20.4.11）
                let (start_width, end_width) =
                    if start == end && start != 0.0 { (-start, None) } else { (start, Some(end)) };
                let body = Vertex2d {
                    flags: vertex_flags,
                    point: *point,
                    start_width,
                    end_width,
                    bulge: vertices.bulges.get(index).copied().unwrap_or(0.0),
                    id: 0,
                    tangent: 0.0,
                };
                self.put(10, vertex, linked(self, index), Body::Vertex2d(body));
            }
        }
        if polyface && let Some(records) = &polyline.faces {
            for (number, face) in records.iter().enumerate() {
                let index = vertices.points.len() + number;
                let vertex = chain[index];
                owned.push(href(3, vertex));
                let mut indices = [0i16; 4];
                for (slot, index) in indices.iter_mut().zip(face) {
                    *slot = short(*index);
                }
                self.put(14, vertex, linked(self, index), Body::FaceRecord(indices));
            }
        }
        let seqend = self.handles.fresh();
        let first_last =
            Some((href(4, chain.first().copied().unwrap_or(0)), href(4, chain.last().copied().unwrap_or(0))));
        self.put(6, seqend, sub.clone(), Body::Empty);
        if self.before_2004() {
            let mut list = chain.clone();
            list.push(seqend);
            self.followers.insert(handle, list);
        }
        let owned = Owned { vertices: owned, first_last, seqend: href(3, seqend) };
        if polyface {
            let body = PolylinePface {
                vertex_count: short(vertices.points.len() as f64),
                face_count: short(faces as f64),
                owned,
            };
            self.put(29, handle, common, Body::PolylinePface(body));
        } else if mesh {
            let body = PolylineMesh {
                flags: flags as i16,
                curve_type: 0,
                m_count: short(polyline.m_count.unwrap_or(0.0)),
                n_count: short(polyline.n_count.unwrap_or(0.0)),
                m_density: 0,
                n_density: 0,
                owned,
            };
            self.put(30, handle, common, Body::PolylineMesh(body));
        } else if three_d {
            let body = Polyline3d {
                spline_flags: if flags & 4 != 0 { 1 } else { 0 },
                closed_flags: u8::from(flags & 1 != 0),
                owned,
            };
            self.put(16, handle, common, Body::Polyline3d(body));
        } else {
            let body = Polyline2d {
                flags: flags as i16,
                curve_type: 0,
                start_width: 0.0,
                end_width: 0.0,
                thickness: thickness(base),
                elevation: vertices.points.first().map_or(0.0, |point| point[2]),
                extrusion: extrusion(base),
                owned,
            };
            self.put(15, handle, common, Body::Polyline2d(body));
        }
    }

    /// 寸法（20.4.22〜20.4.30）。DXF の 70 の種類から型を決める。DXF の 70 の 32 は Flags 1 の 2、128 は Flags 1 の 1 の反対
    fn dimension(&mut self, kind: &str, base: &DxfEntityBase, dimension: &DxfDimension) -> (u16, Dimension) {
        let dimtype = int(dimension.dimtype);
        let point = |value: Option<Vec3>| value.unwrap_or([0.0; 3]);
        let defpoint = dimension.defpoint;
        let (number, body) = match kind {
            "ARC_DIMENSION" => (
                self.class_number("ARC_DIMENSION"),
                DimensionKind::Arc {
                    p10: defpoint,
                    p13: point(dimension.defpoint2),
                    p14: point(dimension.defpoint3),
                    p15: point(dimension.defpoint4),
                    partial: false,
                    start: 0.0,
                    end: 0.0,
                    leader: false,
                    p16: point(dimension.defpoint5),
                    p17: [0.0; 3],
                },
            ),
            "LARGE_RADIAL_DIMENSION" => (
                self.class_number("LARGE_RADIAL_DIMENSION"),
                DimensionKind::LargeRadial {
                    p10: defpoint,
                    p13: point(dimension.defpoint2),
                    unknown: 0.0,
                    p14: point(dimension.defpoint3),
                    p15: point(dimension.defpoint4),
                },
            ),
            _ => match dimtype & 7 {
                0 | 1 => (
                    if dimtype & 7 == 0 { 21 } else { 22 },
                    DimensionKind::Linear {
                        p13: point(dimension.defpoint2),
                        p14: point(dimension.defpoint3),
                        p10: defpoint,
                        oblique: rad(dimension.oblique_angle.unwrap_or(0.0)),
                        rotation: (dimtype & 7 == 0).then(|| rad(dimension.angle.unwrap_or(0.0))),
                    },
                ),
                2 => (
                    24,
                    DimensionKind::Angular2 {
                        p16: xy(&point(dimension.defpoint5)),
                        p13: point(dimension.defpoint2),
                        p14: point(dimension.defpoint3),
                        p15: point(dimension.defpoint4),
                        p10: defpoint,
                    },
                ),
                3 => (26, DimensionKind::Diameter { p15: point(dimension.defpoint4), p10: defpoint, leader: 0.0 }),
                4 => (25, DimensionKind::Radius { p10: defpoint, p15: point(dimension.defpoint4), leader: 0.0 }),
                5 => (
                    23,
                    DimensionKind::Angular3 {
                        p10: defpoint,
                        p13: point(dimension.defpoint2),
                        p14: point(dimension.defpoint3),
                        p15: point(dimension.defpoint4),
                    },
                ),
                _ => (
                    20,
                    DimensionKind::Ordinate {
                        p10: defpoint,
                        p13: point(dimension.defpoint2),
                        p14: point(dimension.defpoint3),
                        flags2: u8::from(dimtype & 64 != 0),
                    },
                ),
            },
        };
        // 8 は意味が分からないが、AutoCAD はどの寸法にも立てる
        let flags1 = 8 | u8::from(dimtype & 128 == 0) | (u8::from(dimtype & 32 != 0) << 1);
        let midpoint = dimension.text_midpoint;
        let body = Dimension {
            version: 0,
            extrusion: extrusion(base),
            text_midpoint: xy(&midpoint),
            elevation: midpoint[2],
            flags1,
            text: self.text(&dimension.text),
            text_rotation: rad(dimension.text_rotation.unwrap_or(0.0)),
            horizontal_direction: rad(dimension.horizontal_direction.unwrap_or(0.0)),
            insert_scale: [1.0, 1.0, 1.0],
            insert_rotation: 0.0,
            attachment: short(dimension.attachment_point.unwrap_or(5.0)),
            line_spacing_style: 1,
            line_spacing_factor: 1.0,
            measurement: dimension.actual_measurement.unwrap_or(0.0),
            flips: [false; 3],
            insert: dimension.insert.map_or([0.0, 0.0], |insert| xy(&insert)),
            kind: body,
            dimstyle: self.dimstyle_of(&dimension.dimstyle),
            block: self.block_of(&dimension.anonymous_block_name),
        };
        (number, body)
    }

    /// HATCH（20.4.75）。DXF の書き出しと同じく、グラデーションは塗りつぶし、境界の図形への関連は書かない
    fn hatch(&mut self, base: &DxfEntityBase, hatch: &DxfHatch) -> Hatch {
        let solid = hatch.solid_fill || hatch.gradient.is_some();
        let gradient = hatch.gradient.as_ref().map(|name| {
            let colors = hatch.gradient_colors.as_deref().unwrap_or(&[]);
            Box::new(HatchGradient {
                enabled: 1,
                reserved: 0,
                angle: hatch.gradient_angle,
                shift: hatch.gradient_shift,
                single_color: i32::from(hatch.gradient_one_color),
                tint: hatch.gradient_tint,
                colors: colors
                    .iter()
                    .enumerate()
                    .map(|(index, rgb)| (index as f64, self.cmc(0.0, Some(*rgb))))
                    .collect(),
                name: self.text(if name.is_empty() { "LINEAR" } else { name }),
            })
        });
        let name = if !hatch.pattern_name.is_empty() {
            hatch.pattern_name.as_str()
        } else if solid {
            "SOLID"
        } else {
            "ANSI31"
        };
        let paths = hatch
            .paths
            .iter()
            .map(|path| {
                let flags = int(path.flags);
                match &path.edges {
                    Some(edges) if flags & 2 == 0 => HatchPath {
                        flags,
                        boundary: HatchBoundary::Edges(edges.iter().map(hatch_edge).collect()),
                        objects: Vec::new(),
                    },
                    _ => {
                        let bulges = path.bulges.as_ref().filter(|bulges| bulges.iter().any(|bulge| *bulge != 0.0));
                        let points = path
                            .points
                            .iter()
                            .enumerate()
                            .map(|(index, point)| {
                                (xy(point), bulges.and_then(|b| b.get(index).copied()).unwrap_or(0.0))
                            })
                            .collect();
                        HatchPath {
                            flags: flags | 2,
                            boundary: HatchBoundary::Polyline {
                                has_bulges: bulges.is_some(),
                                closed: path.closed,
                                points,
                            },
                            objects: Vec::new(),
                        }
                    }
                }
            })
            .collect::<Vec<HatchPath>>();
        let pattern = (!solid).then(|| HatchPattern {
            angle: rad(hatch.pattern_angle),
            scale: if hatch.pattern_scale != 0.0 { hatch.pattern_scale } else { 1.0 },
            double: hatch.pattern_double,
            lines: hatch
                .pattern_lines
                .iter()
                .map(|line| PatternLine {
                    angle: rad(line.angle),
                    base: line.base,
                    offset: line.offset,
                    dashes: line.dashes.clone(),
                })
                .collect(),
        });
        let pixel_size = paths.iter().any(|path| path.flags & 4 != 0).then_some(0.0);
        Hatch {
            gradient,
            elevation: hatch.elevation,
            extrusion: extrusion(base),
            name: self.text(name),
            solid_fill: solid,
            associative: false,
            paths,
            style: short(hatch.hatch_style),
            pattern_type: short(hatch.pattern_type),
            pattern,
            pixel_size,
            seeds: Vec::new(),
        }
    }

    /// MLINE（20.4.50）。スタイルは名前で探し、なければ STANDARD
    fn mline(&self, base: &DxfEntityBase, mline: &DxfMLine) -> MLine {
        let style = |name: &str| {
            self.mline_styles.iter().find(|(_, style)| upper(&style.name) == upper(name)).map(|(handle, _)| *handle)
        };
        let style = style(&mline.style_name).or_else(|| style("STANDARD")).unwrap_or(0);
        let lines = int(mline.line_count).clamp(0, 255) as u8;
        let vertices = mline
            .points
            .iter()
            .enumerate()
            .map(|(index, point)| {
                let line_params = mline.line_params.as_ref().and_then(|params| params.get(index));
                let fill_params = mline.fill_params.as_ref().and_then(|params| params.get(index));
                MLineVertex {
                    point: *point,
                    direction: mline.vertex_directions.get(index).copied().unwrap_or([1.0, 0.0, 0.0]),
                    miter: mline.miter_directions.get(index).copied().unwrap_or([0.0, 1.0, 0.0]),
                    params: (0..usize::from(lines))
                        .map(|element| {
                            (
                                line_params.and_then(|lines| lines.get(element)).cloned().unwrap_or_default(),
                                fill_params.and_then(|fills| fills.get(element)).cloned().unwrap_or_default(),
                            )
                        })
                        .collect(),
                }
            })
            .collect();
        MLine {
            scale: mline.scale,
            justification: int(mline.justification).clamp(0, 255) as u8,
            base_point: mline.base_point,
            extrusion: extrusion(base),
            // DXF の 71 と同じ印（1 頂点を持つ、2 閉じる、4・8 始点・終点の端を描かない。DXF の書き出しと同じく 2 は closed から）
            open_closed: ((int(mline.flags) & !2) | (i32::from(mline.closed) << 1)) as i16,
            lines_in_style: lines,
            vertices,
            style: href(5, style),
        }
    }

    /// MULTILEADER（20.4.48、20.4.86）。DXF の書き出しと同じく、スタイルは Standard を指し、上書きの印をすべて立てて
    /// 図形の値を使わせる。R2010 より前の版は、引出線ごとの矢印を図形の終わりの矢印の並び（位置が引出線の番号）に書き、
    /// 持てない値（垂直の付け方、上下の付け方、引出線ごとの色）は書かずに数える
    fn mleader(&mut self, mleader: &DxfMLeader) -> MLeader {
        let context = &mleader.context;
        if self.version < Version::R2010 {
            let vertical = context.leaders.iter().any(|leader| leader.attachment_direction != 0.0)
                || mleader.text_top_attachment_type != 9.0
                || mleader.text_bottom_attachment_type != 9.0;
            let colored = context
                .leaders
                .iter()
                .flat_map(|leader| &leader.lines)
                .any(|line| int(line.color) != int(BY_BLOCK_RAW));
            if vertical || colored {
                self.notices.add(
                    NoticeLevel::Info,
                    "version-mleader-2010",
                    "マルチ引出線の垂直の付け方・上下の文字の付け方・引出線ごとの色は AutoCAD 2010 からで、それより前の版にないため、書きませんでした",
                );
            }
        }
        let standard = self.style_handles.get("STANDARD").copied().unwrap_or(0);
        let text_style = context.mtext.as_ref().and_then(|mtext| self.mapped(&mtext.style_handle)).unwrap_or(standard);
        let block = context
            .block
            .as_ref()
            .and_then(|block| self.mapped(&block.block_record_handle).map(|handle| (block, handle)));
        if context.block.is_some() && block.is_none() {
            self.notices.add(
                NoticeLevel::Warning,
                "mleader-block",
                "定義のないブロックを内容とするマルチ引出線は、内容を省きました",
            );
        }
        let linetype = self
            .mapped(&mleader.leader_linetype_handle)
            .or_else(|| self.linetype_handles.get("BYBLOCK").copied())
            .unwrap_or(0);
        let arrow =
            if mleader.arrow_head_handle.is_empty() { 0 } else { self.mapped(&mleader.arrow_head_handle).unwrap_or(0) };
        let arrows: Vec<(f64, u64)> = mleader
            .arrow_heads
            .iter()
            .filter_map(|head| self.mapped(&head.handle).map(|handle| (head.index, handle)))
            .collect();
        let labels = mleader
            .block_attribs
            .iter()
            .filter_map(|attrib| {
                self.mapped(&attrib.handle)
                    .map(|handle| (href(4, handle), self.text(&attrib.text), short(attrib.index), attrib.width))
            })
            .collect();
        let align = context.mtext.as_ref().map_or(0, |mtext| (int(mtext.alignment).max(1) - 1) % 3);
        let scale = if context.scale != 0.0 { context.scale } else { 1.0 };
        let by_block = self.cmc(0.0, None);
        let roots = context
            .leaders
            .iter()
            .map(|leader| {
                let lines = leader
                    .lines
                    .iter()
                    .map(|line| {
                        let mut path = line.vertices.clone();
                        path.push(leader.last_leader_point);
                        let breaks = line
                            .breaks
                            .iter()
                            .map(|gap| (nearest_segment(&path, &gap.start) as i32, vec![[gap.start, gap.end]]))
                            .collect();
                        let own = arrows.iter().find(|(index, _)| *index == line.index).map(|(_, handle)| *handle);
                        LeaderLine {
                            points: line.vertices.clone(),
                            breaks,
                            index: int(line.index),
                            style: Some(Box::new(LeaderLineStyle {
                                leader_type: short(mleader.leader_type),
                                color: self.cmc_raw(line.color),
                                linetype: href(5, linetype),
                                lineweight: int(mleader.leader_lineweight),
                                arrow_size: context.arrow_head_size,
                                arrow: href(5, own.unwrap_or(arrow)),
                                // 矢印を上書きした線は印 32（読み取りはその線の矢印を DXF の 94・345 と同じに読む）
                                overrides: if own.is_some() { 32 } else { 0 },
                            })),
                        }
                    })
                    .collect();
                LeaderRoot {
                    content_valid: leader.has_last_leader_line,
                    unknown: leader.has_dogleg_vector,
                    connection: leader.last_leader_point,
                    direction: leader.dogleg_vector,
                    breaks: leader.breaks.iter().map(|gap| [gap.start, gap.end]).collect(),
                    index: int(leader.index),
                    landing_distance: leader.dogleg_length,
                    lines,
                    attachment_direction: short(leader.attachment_direction),
                }
            })
            .collect();
        let content = match (&context.mtext, block) {
            (Some(mtext), _) => Content::Text(Box::new(ContextText {
                label: self.text(&mtext.default_content),
                normal: mtext.extrusion,
                style: href(5, text_style),
                location: mtext.insert,
                direction: mtext.text_direction,
                rotation: mtext.text_direction[1].atan2(mtext.text_direction[0]),
                width: mtext.width,
                height: 0.0,
                line_spacing_factor: mtext.line_spacing_factor,
                line_spacing_style: short(mtext.line_spacing_style),
                color: self.cmc_raw(mtext.color),
                alignment: short(mtext.alignment),
                flow: short(mtext.flow_direction),
                bg_color: self.cmc_raw(mtext.bg_color),
                bg_scale: mtext.bg_scale_factor,
                bg_transparency: 0,
                bg_fill: mtext.use_window_bg_color,
                bg_mask: mtext.has_bg_fill,
                column_type: short(mtext.column_type),
                auto_height: false,
                column_width: 0.0,
                gutter: 0.0,
                flow_reversed: false,
                column_sizes: Vec::new(),
                word_break: true,
                unknown: false,
            })),
            (None, Some((block, handle))) => Content::Block(Box::new(ContextBlock {
                block: href(4, handle),
                normal: block.extrusion,
                location: block.insert,
                scale: block.scale,
                rotation: block.rotation,
                color: self.cmc_raw(block.color),
                matrix: [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            })),
            (None, None) => Content::None,
        };
        let top = short(mleader.text_top_attachment_type);
        let bottom = short(mleader.text_bottom_attachment_type);
        let annot = AnnotContext {
            roots,
            scale: context.scale,
            base_point: context.base_point,
            text_height: context.char_height,
            arrow_size: context.arrow_head_size,
            landing_gap: context.landing_gap_size,
            attachments: [
                short(mleader.text_left_attachment_type),
                short(mleader.text_right_attachment_type),
                align as i16,
                0,
            ],
            content,
            base: [context.base_point, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normal_reversed: false,
            top_bottom: Some([top, bottom]),
        };
        let text_color = context.mtext.as_ref().map_or(by_block.clone(), |mtext| self.cmc_raw(mtext.color));
        let block_handle = block.map_or(0, |(_, handle)| handle);
        // R2010 より前の引出線ごとの矢印（既定か、ブロック）。読み取りは位置を引出線の番号とする
        let mut old_arrows = Vec::new();
        if self.version < Version::R2010 && !arrows.is_empty() {
            let last = arrows.iter().map(|(index, _)| int(*index)).max().unwrap_or(0).clamp(0, 1 << 16);
            for index in 0..=last {
                match arrows.iter().find(|(own, _)| int(*own) == index) {
                    Some((_, handle)) => old_arrows.push((false, href(5, *handle))),
                    None => old_arrows.push((true, href(5, arrow))),
                }
            }
        }
        MLeader {
            version: 2,
            context: annot,
            style: href(5, self.fixed.mleader_style),
            overrides: 0x7fff_ffff,
            leader_type: short(mleader.leader_type),
            line_color: self.cmc_raw(mleader.leader_line_color),
            linetype: href(5, linetype),
            lineweight: int(mleader.leader_lineweight),
            landing: true,
            dogleg: mleader.has_dogleg,
            landing_distance: context.leaders.first().map_or(0.0, |leader| leader.dogleg_length) / scale,
            arrow_head: href(5, arrow),
            arrow_size: context.arrow_head_size / scale,
            content_type: short(mleader.content_type),
            text_style: href(5, text_style),
            left_attachment: short(mleader.text_left_attachment_type),
            right_attachment: short(mleader.text_right_attachment_type),
            text_angle_type: 1,
            unknown: align as i16,
            text_color,
            text_frame: mleader.has_text_frame,
            block: href(5, block_handle),
            block_color: block.map_or(by_block, |(block, _)| self.cmc_raw(block.color)),
            block_scale: block.map_or([1.0, 1.0, 1.0], |(block, _)| block.scale),
            block_rotation: block.map_or(0.0, |(block, _)| block.rotation),
            block_connection: 0,
            annotative: false,
            arrow_heads: old_arrows,
            labels,
            text_direction_negative: false,
            ipe_align: 0,
            justification: align as i16 + 1,
            scale: context.scale,
            attachments: Some([
                short(context.leaders.first().map_or(0.0, |leader| leader.attachment_direction)),
                bottom,
                top,
            ]),
            extended_to_text: false,
        }
    }
}

/// HATCH の境界の辺。角度は度からラジアンに
fn hatch_edge(edge: &DxfHatchEdge) -> HatchEdge {
    match edge {
        DxfHatchEdge::Line { start, end } => HatchEdge::Line(*start, *end),
        DxfHatchEdge::Arc { center, radius, start_angle, end_angle, ccw } => {
            let (start, end) = rad_pair(*start_angle, *end_angle);
            HatchEdge::Arc { center: *center, radius: *radius, start, end, ccw: *ccw }
        }
        DxfHatchEdge::Ellipse { center, major_axis, ratio, start_angle, end_angle, ccw } => {
            let (start, end) = rad_pair(*start_angle, *end_angle);
            HatchEdge::Ellipse { center: *center, major_axis: *major_axis, ratio: *ratio, start, end, ccw: *ccw }
        }
        DxfHatchEdge::Spline { degree, rational, periodic, knots, control_points, weights, fit_points } => {
            HatchEdge::Spline(Box::new(HatchSpline {
                degree: int(*degree),
                rational: *rational,
                periodic: *periodic,
                knots: knots.clone(),
                control: control_points
                    .iter()
                    .enumerate()
                    .map(|(index, point)| (*point, weights.get(index).copied().unwrap_or(1.0)))
                    .collect(),
                fit: fit_points.clone(),
                tangents: [[0.0; 2]; 2],
            }))
        }
    }
}

/// SPLINE（20.4.40）。制御点があれば制御点と節点（R2013 以降の節点の取り方は任意 15）、なければ当てはめ点（節点の取り方
/// 弦長 0、印は当てはめ点 1 と節点の取り方を使う 8）。R2013 以降の AutoCAD と同じく scenario は 1。R2013 より前は
/// scenario（1 制御点、2 当てはめ点）で分け、節点の取り方を持たない（読み取りは弦長で求める）
fn spline_body(spline: &DxfSpline, version: Version) -> Spline {
    let closed = spline.closed || int(spline.flags) & 1 != 0;
    let r2013 = version >= Version::R2013;
    let fit = |knot_parameter: i32| Spline {
        scenario: if r2013 { 1 } else { 2 },
        flags1: 9 | if closed { 4 } else { 0 },
        knot_parameter,
        degree: int(spline.degree),
        fit: Some(SplineFit {
            tolerance: 1e-10,
            start_tangent: spline.start_tangent.unwrap_or([0.0; 3]),
            end_tangent: spline.end_tangent.unwrap_or([0.0; 3]),
            points: spline.fit_points.clone(),
        }),
        control: None,
    };
    if spline.control_points.is_empty() && !spline.fit_points.is_empty() {
        return fit(0);
    }
    // 読み取りが当てはめ点から求めた制御点（DWG の当てはめ点のスプライン）と同じなら、当てはめ点のまま書く
    if let (Some(d0), Some(dn)) = (spline.start_tangent, spline.end_tangent)
        && spline.degree == 3.0
        && !closed
    {
        let parameters: &[i32] = if r2013 { &[0, 1, 2] } else { &[0] };
        for &knot_parameter in parameters {
            if crate::entities::interpolate(&spline.fit_points, d0, dn, knot_parameter)
                .is_some_and(|(knots, controls)| knots == spline.knots && controls == spline.control_points)
            {
                return fit(knot_parameter);
            }
        }
    }
    let weighted = spline.rational && !spline.weights.is_empty();
    Spline {
        scenario: 1,
        flags1: if closed { 4 } else { 0 },
        knot_parameter: 15,
        degree: int(spline.degree),
        fit: None,
        control: Some(SplineControl {
            rational: spline.rational,
            closed,
            periodic: spline.periodic || int(spline.flags) & 2 != 0,
            knot_tolerance: 1e-10,
            control_tolerance: 1e-10,
            weighted,
            knots: spline.knots.clone(),
            points: spline.control_points.clone(),
            weights: if weighted { spline.weights.clone() } else { Vec::new() },
        }),
    }
}

impl Writer<'_> {
    /// 画像の定義（IMAGEDEF）とリアクタ（IMAGEDEF_REACTOR）のハンドル。同じ定義を指していた画像は、1 つの定義を
    /// 共有する（DXF の書き出しと同じ）
    fn plan_image(&mut self, image: &DxfImage, handle: u64) -> (u64, u64) {
        let original = image.definition_handle.as_deref().map(upper).unwrap_or_default();
        let number = match self.image_def_index.get(&original).copied().filter(|_| !original.is_empty()) {
            Some(number) => number,
            None => {
                self.image_defs.push(super::ImageDefPlan {
                    handle: self.handles.fresh(),
                    path: image.path.clone().unwrap_or_default(),
                    size: image.image_size,
                    loaded: image.loaded,
                    reactors: Vec::new(),
                });
                self.image_def_index.insert(original, self.image_defs.len() - 1);
                self.image_defs.len() - 1
            }
        };
        let reactor = self.handles.fresh();
        self.image_defs[number].reactors.push((reactor, handle));
        (self.image_defs[number].handle, reactor)
    }
}

/// 明るさ・コントラスト・フェード（0〜100 の RC）
fn percent(value: f64) -> u8 {
    // NaN は 0
    value.round().clamp(0.0, 100.0) as u8
}

/// WIPEOUT（20.4.80 と同じ形）。画像の定義は持たない。多角形の境界は閉じる点を持たない（読み取りが閉じる）
fn wipeout(image: &DxfImage) -> Image {
    let mut boundary = image.boundary.clone();
    if boundary.len() > 3 && boundary.first() == boundary.last() {
        boundary.pop();
    }
    Image {
        class_version: 0,
        insert: image.insert,
        u_vector: image.u_pixel,
        v_vector: image.v_pixel,
        size: image.image_size,
        display: short(image.flags),
        clipping: image.clipping,
        brightness: percent(image.brightness),
        contrast: percent(image.contrast),
        fade: percent(image.fade),
        clip_mode: image.clip_inverted,
        clip_type: if boundary.len() == 2 { 1 } else { 2 },
        boundary,
        imagedef: href(5, 0),
        reactor: href(3, 0),
    }
}
