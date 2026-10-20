//! AutoCAD R12 の DXF に書く前に、図面のモデルを R12 にある図形だけにする（`write_dxf_as` の R12）。ezdxf の
//! r12export と同じく、R12 にない図形は R12 の図形に置き換え、置き換えたもの・省いたものを notices で知らせる。
//!
//! - LWPOLYLINE は 2D の POLYLINE にする（頂点・膨らみ・幅はそのまま）。
//! - 楕円（ELLIPSE）・スプライン（SPLINE・HELIX）・複数行の文字（MTEXT）・ハッチング（HATCH・MPOLYGON）・引出線
//!   （LEADER・MULTILEADER）・マルチライン（MLINE）は、zumen が描く形（`scene::Exploder`）を R12 の図形にする。
//!   線は LINE・POLYLINE、塗りは横の帯ごとの台形の SOLID、ハッチングの模様は境界で切った線分（LINE）、文字は run ごとの
//!   1 行の文字（TEXT）、点は POINT。1 つの図形になればその図形を、いくつかになれば無名のブロック（*U）にまとめて
//!   その参照（INSERT）を、元の図形のハンドル・レイヤ・色・線種で書く。ブロックの中の部品はレイヤ 0 に置き、図形と同じ色・
//!   線種は BYBLOCK にする（参照から継ぐ）。画面の背景色の塗り（背景のマスク）は R12 で表せないので省く。
//! - 複数行の属性は、行を空白でつないだ 1 行の属性にする。
//! - 紙空間は現在のレイアウト（*Paper_Space）だけにし（R12 は紙空間を 1 つしか持たない）、その用紙を紙空間の範囲
//!   （$PLIMMIN・$PLIMMAX）にする。
//! - XLINE・RAY・WIPEOUT・非表示の図形は省く。
//! - 長さの単位（$INSUNITS）は書けない（R12 の図面は単位を持たない）。

use crate::colors::nearest_aci;
use crate::model::*;
use crate::scene::{Exploded, Exploder, TextPiece, anchor_of, layout_sheet};
use std::collections::{HashMap, HashSet};
use zumen_core::{BACKGROUND_COLOR, NoticeLevel, NoticeList, Primitive, StrokePrimitive};

const PAPER: &str = "*PAPER_SPACE";

/// R12 にした図面と、置き換え・省いた内容
pub(crate) struct Prepared {
    pub doc: DxfDocument,
    pub notices: NoticeList,
}

pub(crate) fn prepare(doc: &DxfDocument) -> Prepared {
    let mut converter = Converter::new(doc);
    let mut entities = Vec::with_capacity(doc.entities.len());
    let mut other_layouts = 0;
    for entity in draw_sequence(doc) {
        if entity.base.space == DxfSpace::Paper && !in_current_layout(entity) {
            other_layouts += 1;
            continue;
        }
        converter.convert(entity, &mut entities);
    }
    let mut notices = std::mem::take(&mut converter.notices);
    // R12 は長さの単位（$INSUNITS・$MEASUREMENT）を持たない。読み取りは単位のない図面を mm とみなす
    if crate::scene::drawing_unit(doc) != 1.0 {
        notices.add(
            NoticeLevel::Info,
            "version-units",
            "長さの単位は、AutoCAD R12 にないため、書きませんでした（単位のない図面として読まれ、zumen は mm とみなします）",
        );
    }
    if other_layouts > 0 {
        notices.add_count(
            NoticeLevel::Info,
            "version-layouts",
            "AutoCAD R12 は紙空間を 1 つしか持たないため、現在のレイアウト（*Paper_Space）のほかのレイアウトの図形を省きました",
            other_layouts,
        );
    }
    let mut header = doc.header.clone();
    let layouts: Vec<DxfLayout> =
        doc.layouts.iter().filter(|layout| layout.model || layout.block.eq_ignore_ascii_case(PAPER)).cloned().collect();
    // R12 は用紙の大きさを持たない。読み取りは紙空間の範囲を用紙とする（paper_unit 1）ので、その範囲を用紙にする
    if let Some(layout) = layouts.iter().find(|layout| !layout.model && layout.paper_width > 0.0) {
        let sheet = layout_sheet(layout);
        if sheet.paper_unit > 0.0 && sheet.width > 0.0 && sheet.height > 0.0 {
            let [ox, oy] = sheet.origin;
            let unit = sheet.paper_unit;
            header.plimmin = Some([-ox / unit, -oy / unit]);
            header.plimmax = Some([(sheet.width - ox) / unit, (sheet.height - oy) / unit]);
        }
    }
    let mut blocks = doc.blocks.clone();
    blocks.append(&mut converter.blocks);
    let mut styles = doc.styles.clone();
    styles.append(&mut converter.styles);
    Prepared {
        doc: DxfDocument {
            format: doc.format,
            version: doc.version.clone(),
            binary: doc.binary,
            encoding: doc.encoding,
            encoding_source: doc.encoding_source,
            header,
            layers: doc.layers.clone(),
            linetypes: doc.linetypes.clone(),
            styles,
            dimstyles: doc.dimstyles.clone(),
            mline_styles: doc.mline_styles.clone(),
            blocks,
            layouts,
            entities,
            sort_handles: doc.sort_handles.clone(),
            skipped: doc.skipped.clone(),
            notices: doc.notices.clone(),
        },
        notices,
    }
}

/// 図形を、空間・ブロックごとに表示が描く順に並べる（R12 は描画順の表 SORTENTSTABLE を持たないので、描く順に書く）。
/// 空間・ブロックの並びは、モデル空間、ブロックの定義の順、定義のないブロックの名前の順
fn draw_sequence(doc: &DxfDocument) -> Vec<&DxfEntity> {
    let order = crate::scene::draw_order(doc);
    let mut names: Vec<String> = doc.blocks.iter().map(|block| block.name.to_uppercase()).collect();
    let mut rest: Vec<String> = order.blocks.keys().filter(|name| !names.contains(name)).cloned().collect();
    rest.sort();
    names.extend(rest);
    let mut seen = HashSet::new();
    let mut sequence = order.model;
    for name in names {
        if seen.insert(name.clone())
            && let Some(entities) = order.blocks.get(&name)
        {
            sequence.extend(entities);
        }
    }
    sequence
}

/// 紙空間の図形のうち、現在のレイアウト（*Paper_Space）のもの
fn in_current_layout(entity: &DxfEntity) -> bool {
    entity.base.block.as_deref().is_none_or(|block| block.eq_ignore_ascii_case(PAPER))
}

/// 部品の色。図形と同じなら、図形の色（ブロックの中では BYBLOCK）
#[derive(Clone, Copy, PartialEq)]
enum PartColor {
    Same,
    Aci(f64),
}

/// 部品の線種。図形と同じなら、図形の線種（ブロックの中では BYBLOCK）。破線の図形の実線の部品は CONTINUOUS
#[derive(Clone, Copy, PartialEq)]
enum PartLinetype {
    Same,
    Continuous,
}

struct Part {
    kind: &'static str,
    data: DxfEntityData,
    color: PartColor,
    linetype: PartLinetype,
}

struct Converter<'a> {
    exploder: Exploder<'a>,
    /// ブロックの名前（大文字）→ ブロックが配置される最大の尺度
    scales: HashMap<String, f64>,
    notices: NoticeList,
    /// 崩した図形をまとめる無名のブロック
    blocks: Vec<DxfBlock>,
    next_block: u64,
    block_names: HashSet<String>,
    /// 書式コードの書体のために足す文字スタイル
    styles: Vec<DxfTextStyle>,
    doc: &'a DxfDocument,
}

impl<'a> Converter<'a> {
    fn new(doc: &'a DxfDocument) -> Self {
        // 無名のブロックの番号は、図面の無名のブロック（*D3・*U12 など）の最大の次から
        let next_block = doc
            .blocks
            .iter()
            .filter_map(|block| {
                let name = block.name.strip_prefix('*')?;
                let digits = name.trim_start_matches(|c: char| c.is_ascii_alphabetic());
                (digits.len() < name.len()).then(|| digits.parse::<u64>().ok()).flatten()
            })
            .max()
            .and_then(|max| max.checked_add(1))
            .unwrap_or(1);
        Self {
            exploder: Exploder::new(doc),
            scales: block_scales(doc),
            notices: NoticeList::new(),
            blocks: Vec::new(),
            next_block,
            block_names: doc.blocks.iter().map(|block| block.name.to_uppercase()).collect(),
            styles: Vec::new(),
            doc,
        }
    }

    fn convert(&mut self, entity: &DxfEntity, out: &mut Vec<DxfEntity>) {
        let base = &entity.base;
        if base.invisible {
            self.notices.add(
                NoticeLevel::Info,
                "version-invisible",
                "非表示の図形は、AutoCAD R12 で非表示にできないため、省きました",
            );
            return;
        }
        let kind = base.kind.to_ascii_uppercase();
        match &entity.data {
            DxfEntityData::LwPolyline(polyline) => {
                let mut vertices = polyline.vertices.clone();
                let elevation = polyline.elevation.unwrap_or(0.0);
                for point in &mut vertices.points {
                    point[2] = elevation;
                }
                if let Some(width) = polyline.const_width {
                    vertices.widths = vec![[width, width]; vertices.points.len()];
                }
                // 線種を頂点をまたいで続ける（128）は LWPOLYLINE と POLYLINE で同じ
                vertices.flags = f64::from(js_int(vertices.flags) as i32 & 128);
                out.push(DxfEntity {
                    base: DxfEntityBase { kind: "POLYLINE".into(), ..base.clone() },
                    data: DxfEntityData::Polyline(DxfPolyline {
                        vertices,
                        vertex_flags: Vec::new(),
                        faces: None,
                        m_count: None,
                        n_count: None,
                    }),
                });
            }
            DxfEntityData::Ellipse { .. }
            | DxfEntityData::Spline(_)
            | DxfEntityData::MText(_)
            | DxfEntityData::Hatch(_)
            | DxfEntityData::Leader(_)
            | DxfEntityData::MLeader(_)
            | DxfEntityData::MLine(_) => {
                self.notice_for(&kind);
                self.explode(entity, out);
            }
            DxfEntityData::XLine { .. } => self.notices.add(
                NoticeLevel::Info,
                "version-xline",
                "構築線（XLINE・RAY）は、AutoCAD R12 にないため、省きました",
            ),
            DxfEntityData::Image(_) if kind == "WIPEOUT" => self.notices.add(
                NoticeLevel::Info,
                "version-wipeout",
                "ワイプアウト（WIPEOUT）は、AutoCAD R12 にないため、省きました",
            ),
            DxfEntityData::Attrib { .. } | DxfEntityData::Attdef { .. } => out.push(self.single_line(entity)),
            DxfEntityData::Insert(insert) => {
                let invisible = insert.attribs.iter().filter(|attrib| attrib.base.invisible).count() as u64;
                if invisible > 0 {
                    self.notices.add_count(
                        NoticeLevel::Info,
                        "version-invisible",
                        "非表示の図形は、AutoCAD R12 で非表示にできないため、省きました",
                        invisible,
                    );
                }
                let attribs = insert
                    .attribs
                    .iter()
                    .filter(|attrib| !attrib.base.invisible)
                    .map(|attrib| self.single_line(attrib))
                    .collect();
                out.push(DxfEntity {
                    base: base.clone(),
                    data: DxfEntityData::Insert(DxfInsert { attribs, ..insert.clone() }),
                });
            }
            _ => out.push(entity.clone()),
        }
    }

    fn notice_for(&mut self, kind: &str) {
        let (code, message) = match kind {
            "ELLIPSE" => ("version-ellipse", "楕円（ELLIPSE）は、AutoCAD R12 にないため、折線（POLYLINE）にしました"),
            "SPLINE" | "HELIX" => {
                ("version-spline", "スプライン（SPLINE・HELIX）は、AutoCAD R12 にないため、折線（POLYLINE）にしました")
            }
            "MTEXT" => (
                "version-mtext",
                "複数行の文字（MTEXT）は、AutoCAD R12 にないため、1 行の文字（TEXT）にしました。いくつかになるものは無名のブロックにまとめました",
            ),
            "HATCH" | "MPOLYGON" => (
                "version-hatch",
                "ハッチング（HATCH）は、AutoCAD R12 にないため、模様の線（LINE）・塗り（SOLID）の無名のブロックにしました",
            ),
            "MLINE" => (
                "version-mline",
                "マルチライン（MLINE）は、AutoCAD R12 にないため、線（LINE・POLYLINE）の無名のブロックにしました",
            ),
            _ => (
                "version-leader",
                "引出線（LEADER・MULTILEADER）は、AutoCAD R12 にないため、線・塗り・文字の無名のブロックにしました",
            ),
        };
        self.notices.add(NoticeLevel::Info, code, message);
    }

    /// 複数行の属性は、行を空白でつないだ 1 行の属性にする（AutoCAD は 1 行の文字を空にしている）
    fn single_line(&mut self, entity: &DxfEntity) -> DxfEntity {
        let (DxfEntityData::Attrib { text, mtext: Some(embedded), .. }
        | DxfEntityData::Attdef { text, mtext: Some(embedded), .. }) = &entity.data
        else {
            return entity.clone();
        };
        self.notices.add(
            NoticeLevel::Info,
            "version-multiline-attribute",
            "複数行の属性は、AutoCAD R12 にないため、行を空白でつないだ 1 行の属性にしました",
        );
        let value = embedded.content.text.lines().collect::<Vec<_>>().join(" ");
        let text = DxfTextBase { text: text_code(&value), ..text.clone() };
        let data = match &entity.data {
            DxfEntityData::Attrib { tag, attribute_flags, .. } => {
                DxfEntityData::Attrib { text, tag: tag.clone(), attribute_flags: *attribute_flags, mtext: None }
            }
            DxfEntityData::Attdef { tag, prompt, attribute_flags, .. } => DxfEntityData::Attdef {
                text,
                tag: tag.clone(),
                prompt: prompt.clone(),
                attribute_flags: *attribute_flags,
                mtext: None,
            },
            _ => unreachable!("属性だけを渡す"),
        };
        DxfEntity { base: entity.base.clone(), data }
    }

    /// zumen が描く形を R12 の図形にする。1 つになればその図形を、いくつかになれば無名のブロックの参照を書く
    fn explode(&mut self, entity: &DxfEntity, out: &mut Vec<DxfEntity>) {
        let scale = match entity.base.space {
            DxfSpace::Block => entity
                .base
                .block
                .as_deref()
                .and_then(|name| self.scales.get(&name.to_uppercase()))
                .copied()
                .unwrap_or(1.0),
            _ => 1.0,
        };
        let exploded = self.exploder.explode(entity, scale);
        let parts = self.parts(&exploded);
        let base = &entity.base;
        // 部品は図形の座標系（WCS）にあるので、元の押し出し方向・厚さは使わない
        let plain = DxfEntityBase { extrusion: None, thickness: None, ..base.clone() };
        match <[Part; 1]>::try_from(parts) {
            Ok([part]) => {
                let mut base = DxfEntityBase { kind: part.kind.into(), ..plain };
                if let PartColor::Aci(color) = part.color {
                    base.color_index = color;
                    base.true_color = None;
                }
                if part.linetype == PartLinetype::Continuous {
                    base.linetype = "CONTINUOUS".into();
                }
                out.push(DxfEntity { base, data: part.data });
            }
            Err(parts) if parts.is_empty() => {}
            Err(parts) => {
                let name = loop {
                    let name = format!("*U{}", self.next_block);
                    self.next_block = self.next_block.checked_add(1).unwrap_or(1);
                    if self.block_names.insert(name.clone()) {
                        break name;
                    }
                };
                // ブロックの基点と参照の位置は、元の図形の代表点（部品は元の座標のまま。表示が原点を寄せる位置も変わらない）
                let [x, y] = anchor_of(entity).filter(|[x, y]| x.is_finite() && y.is_finite()).unwrap_or([0.0, 0.0]);
                self.blocks.push(DxfBlock {
                    handle: String::new(),
                    name: name.clone(),
                    base_point: [x, y, 0.0],
                    flags: 1.0,
                    anonymous: true,
                    layout: false,
                    block_record_handle: None,
                    xref_path: None,
                    description: None,
                });
                for part in parts {
                    out.push(DxfEntity {
                        base: DxfEntityBase {
                            kind: part.kind.into(),
                            handle: String::new(),
                            ordinal: 0,
                            owner: None,
                            space: DxfSpace::Block,
                            block: Some(name.clone()),
                            layer: "0".into(),
                            linetype: match part.linetype {
                                PartLinetype::Same => "BYBLOCK".into(),
                                PartLinetype::Continuous => "CONTINUOUS".into(),
                            },
                            color_index: match part.color {
                                PartColor::Same => 0.0,
                                PartColor::Aci(color) => color,
                            },
                            true_color: None,
                            transparency: None,
                            lineweight: -1.0,
                            linetype_scale: 1.0,
                            invisible: false,
                            thickness: None,
                            extrusion: None,
                        },
                        data: part.data,
                    });
                }
                out.push(DxfEntity {
                    base: DxfEntityBase { kind: "INSERT".into(), ..plain },
                    data: DxfEntityData::Insert(DxfInsert {
                        name,
                        insert: [x, y, 0.0],
                        xscale: 1.0,
                        yscale: 1.0,
                        zscale: 1.0,
                        rotation: 0.0,
                        column_count: 1.0,
                        row_count: 1.0,
                        column_spacing: 0.0,
                        row_spacing: 0.0,
                        attribs: Vec::new(),
                    }),
                });
            }
        }
    }

    /// 描く要素を R12 の図形にする。文字は texts の run を順に使う
    fn parts(&mut self, exploded: &Exploded) -> Vec<Part> {
        let color = |value: &str| {
            if value == exploded.color { PartColor::Same } else { PartColor::Aci(aci_of(value)) }
        };
        let mut texts = exploded.texts.iter();
        let mut parts = Vec::new();
        let mut masks = 0;
        for primitive in &exploded.primitives {
            match primitive {
                Primitive::Stroke(stroke) => {
                    let linetype = if stroke.dashes.is_empty() && exploded.dashed {
                        PartLinetype::Continuous
                    } else {
                        PartLinetype::Same
                    };
                    for data in stroke_parts(stroke) {
                        parts.push(Part { kind: kind_of(&data), data, color: color(&stroke.color), linetype });
                    }
                }
                Primitive::Fill(fill) => {
                    if fill.color == BACKGROUND_COLOR {
                        masks += 1;
                        continue;
                    }
                    for [p0, p1, p2, p3] in trapezoids(&fill.rings) {
                        parts.push(Part {
                            kind: "SOLID",
                            // SOLID は 1 → 2 → 4 → 3 の順に囲む
                            data: DxfEntityData::Solid {
                                vtx0: [p0[0], p0[1], 0.0],
                                vtx1: [p1[0], p1[1], 0.0],
                                vtx2: [p3[0], p3[1], 0.0],
                                vtx3: [p2[0], p2[1], 0.0],
                                invisible_edges: None,
                            },
                            color: color(&fill.color),
                            linetype: PartLinetype::Same,
                        });
                    }
                }
                Primitive::Clipped(clipped) => {
                    for stroke in &clipped.strokes {
                        for [start, end] in clip_pattern(stroke, &clipped.rings, &mut self.notices) {
                            parts.push(Part {
                                kind: "LINE",
                                data: DxfEntityData::Line {
                                    start: [start[0], start[1], 0.0],
                                    end: [end[0], end[1], 0.0],
                                },
                                color: color(&stroke.color),
                                linetype: PartLinetype::Continuous,
                            });
                        }
                    }
                }
                Primitive::Dot(dot) => parts.push(Part {
                    kind: "POINT",
                    data: DxfEntityData::Point { location: [dot.x, dot.y, 0.0], x_axis_angle: 0.0 },
                    color: color(&dot.color),
                    linetype: PartLinetype::Same,
                }),
                Primitive::Text(_) => {
                    let Some(piece) = texts.next() else { continue };
                    let data = DxfEntityData::Text(self.text(piece));
                    parts.push(Part { kind: "TEXT", data, color: color(&piece.color), linetype: PartLinetype::Same });
                }
            }
        }
        if masks > 0 {
            self.notices.add(
                NoticeLevel::Info,
                "version-mask",
                "画面の背景色で塗る背景（背景のマスク）は、AutoCAD R12 で表せないため、書きませんでした",
            );
        }
        parts
    }

    /// run を、同じに描ける 1 行の文字にする。書式コードの書体は、その書体の文字スタイルを足して使う
    fn text(&mut self, piece: &TextPiece) -> DxfTextBase {
        let [a, b, c, d, e, f] = piece.matrix;
        let (sx, sy) = (a.hypot(b), c.hypot(d));
        let mirrored = a * d - b * c < 0.0;
        // 左右反転（71 の 2）は、文字の X 軸を逆向きにする
        let rotation = if mirrored { (-b).atan2(-a) } else { b.atan2(a) }.to_degrees();
        let style = match &piece.font {
            Some(font) => self.font_style(&font.name, font.truetype),
            None => piece.style.clone().unwrap_or_else(|| "Standard".into()),
        };
        DxfTextBase {
            text: text_code(&piece.text),
            insert: [e, f, 0.0],
            align_point: None,
            height: piece.height * sy,
            rotation,
            oblique: piece.oblique,
            width: if sy > 0.0 { piece.width_factor * sx / sy } else { piece.width_factor },
            style,
            halign: 0.0,
            valign: 0.0,
            text_generation_flag: if mirrored { 2.0 } else { 0.0 },
        }
    }

    /// 書体の文字スタイルの名前。図面か足したものに同じ書体のスタイルがあればそれ、なければ書体の名前で足す
    fn font_style(&mut self, name: &str, truetype: bool) -> String {
        let has_extension = name.rsplit_once('.').is_some_and(|(_, extension)| extension.len() <= 3);
        let file = if truetype && !has_extension { format!("{name}.ttf") } else { name.to_string() };
        let existing = self.doc.styles.iter().chain(&self.styles).find(|style| style.font.eq_ignore_ascii_case(&file));
        if let Some(style) = existing {
            return style.name.clone();
        }
        let taken: HashSet<String> =
            self.doc.styles.iter().chain(&self.styles).map(|style| style.name.to_uppercase()).collect();
        let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let mut style_name = stem.to_string();
        let mut index = 2;
        while taken.contains(&style_name.to_uppercase()) {
            style_name = format!("{stem}_{index}");
            index += 1;
        }
        self.styles.push(DxfTextStyle {
            handle: String::new(),
            name: style_name.clone(),
            font: file,
            bigfont: String::new(),
            height: 0.0,
            width: 1.0,
            oblique: 0.0,
            flags: 0.0,
            generation_flags: 0.0,
            font_family: None,
        });
        style_name
    }
}

/// ブロックが配置される最大の尺度（入れ子の配置の尺度を掛ける）。崩す曲線の細かさを、いちばん大きく見える配置に
/// 合わせる。寸法・表のブロックは寸法・表と同じ尺度
fn block_scales(doc: &DxfDocument) -> HashMap<String, f64> {
    let mut references: Vec<(String, String, f64)> = Vec::new();
    for entity in &doc.entities {
        let container = match entity.base.space {
            DxfSpace::Block => entity.base.block.as_deref().unwrap_or("").to_uppercase(),
            _ => String::new(),
        };
        let (name, scale) = match &entity.data {
            DxfEntityData::Insert(insert) => (&insert.name, insert.xscale.abs().max(insert.yscale.abs())),
            DxfEntityData::Table(table) => (&table.name, table.xscale.abs().max(table.yscale.abs())),
            DxfEntityData::Dimension(dimension) => (&dimension.anonymous_block_name, 1.0),
            _ => continue,
        };
        if scale.is_finite() && scale > 0.0 {
            references.push((container, name.to_uppercase(), scale));
        }
    }
    let mut scales: HashMap<String, f64> = HashMap::new();
    // 入れ子の深さだけ繰り返す（循環する参照は 64 段で止める）
    for _ in 0..64 {
        let mut changed = false;
        for (container, name, scale) in &references {
            let outer = if container.is_empty() { 1.0 } else { scales.get(container).copied().unwrap_or(0.0) };
            let value = (outer * scale).min(1e6);
            if value > scales.get(name).copied().unwrap_or(0.0) {
                scales.insert(name.clone(), value);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    scales
}

fn js_int(value: f64) -> i64 {
    if value.is_finite() { value as i64 } else { 0 }
}

fn kind_of(data: &DxfEntityData) -> &'static str {
    match data {
        DxfEntityData::Line { .. } => "LINE",
        _ => "POLYLINE",
    }
}

/// #rrggbb（#rrggbbaa の透過は除く）に最も近い ACI。画面の背景色は前景色の 7
fn aci_of(value: &str) -> f64 {
    let hex = value.trim_start_matches('#');
    match hex.get(..6).and_then(|hex| u32::from_str_radix(hex, 16).ok()) {
        Some(rgb) => nearest_aci(f64::from(rgb)),
        None => 7.0,
    }
}

/// 文字列を 1 行の文字（TEXT）の書き方にする。^ は「^ 」、% が続くところは %%037（% の文字コード）にする
fn text_code(text: &str) -> String {
    let text = text.replace('^', "^ ");
    if text.contains("%%") { text.replace('%', "%%037") } else { text }
}

/// 線を LINE（2 点）か POLYLINE にする。segments の線は 2 点ずつの LINE
fn stroke_parts(stroke: &StrokePrimitive) -> Vec<DxfEntityData> {
    let point = |index: usize| [stroke.points[index * 2], stroke.points[index * 2 + 1], 0.0];
    let count = stroke.points.len() / 2;
    let line = |start: usize| DxfEntityData::Line { start: point(start), end: point(start + 1) };
    if stroke.segments == Some(true) {
        return (0..count / 2).map(|index| line(index * 2)).collect();
    }
    match count {
        0 | 1 => Vec::new(),
        2 if !stroke.closed => vec![line(0)],
        _ => vec![DxfEntityData::Polyline(DxfPolyline {
            vertices: DxfVertexList {
                points: (0..count).map(point).collect(),
                bulges: Vec::new(),
                widths: Vec::new(),
                closed: stroke.closed,
                flags: f64::from(u8::from(stroke.closed)),
            },
            vertex_flags: Vec::new(),
            faces: None,
            m_count: None,
            n_count: None,
        })],
    }
}

type Point = [f64; 2];

/// 偶奇規則で塗る輪の並びを、頂点と辺の交点の高さで区切った横の帯ごとの台形（下の左・下の右・上の右・上の左）に分ける
fn trapezoids(rings: &[Vec<f64>]) -> Vec<[Point; 4]> {
    // 辺（下の点、上の点）。水平な辺は帯の境にしかならない
    let mut edges: Vec<(Point, Point)> = Vec::new();
    let mut heights: Vec<f64> = Vec::new();
    for ring in rings {
        let count = ring.len() / 2;
        if count < 3 {
            continue;
        }
        for index in 0..count {
            let next = (index + 1) % count;
            let a = [ring[index * 2], ring[index * 2 + 1]];
            let b = [ring[next * 2], ring[next * 2 + 1]];
            if !(a[0].is_finite() && a[1].is_finite() && b[0].is_finite() && b[1].is_finite()) {
                continue;
            }
            heights.push(a[1]);
            if a[1] != b[1] {
                edges.push(if a[1] < b[1] { (a, b) } else { (b, a) });
            }
        }
    }
    heights.sort_by(f64::total_cmp);
    heights.dedup();
    edges.sort_by(|x, y| x.0[1].total_cmp(&y.0[1]));
    let x_at = |edge: &(Point, Point), y: f64| {
        let ((x0, y0), (x1, y1)) = ((edge.0[0], edge.0[1]), (edge.1[0], edge.1[1]));
        x0 + (x1 - x0) * (y - y0) / (y1 - y0)
    };
    let mut out = Vec::new();
    let mut next = 0;
    let mut active: Vec<(Point, Point)> = Vec::new();
    for band in heights.windows(2) {
        let (bottom, top) = (band[0], band[1]);
        while next < edges.len() && edges[next].0[1] <= bottom {
            active.push(edges[next]);
            next += 1;
        }
        active.retain(|edge| edge.1[1] > bottom);
        let crossing: Vec<&(Point, Point)> =
            active.iter().filter(|edge| edge.0[1] <= bottom && edge.1[1] >= top).collect();
        // 帯の中で交わる辺（下と上で左右が入れ替わる）の交点の高さで、帯をさらに分ける
        let mut cuts = vec![bottom, top];
        for (index, a) in crossing.iter().enumerate() {
            for b in &crossing[index + 1..] {
                let below = x_at(a, bottom) - x_at(b, bottom);
                let above = x_at(a, top) - x_at(b, top);
                if below * above < 0.0 {
                    cuts.push(bottom + (top - bottom) * below / (below - above));
                }
            }
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        for part in cuts.windows(2) {
            let (low, high) = (part[0], part[1]);
            if low >= high {
                continue;
            }
            let middle = (low + high) / 2.0;
            let mut sorted = crossing.clone();
            sorted.sort_by(|x, y| x_at(x, middle).total_cmp(&x_at(y, middle)));
            for [left, right] in sorted.as_chunks::<2>().0 {
                out.push([
                    [x_at(left, low), low],
                    [x_at(right, low), low],
                    [x_at(right, high), high],
                    [x_at(left, high), high],
                ]);
            }
        }
    }
    out
}

/// ハッチングの模様の線（2 点ずつの線分。破線は各線分の始点から）を、偶奇規則の境界の中の線・点の線分にする
fn clip_pattern(stroke: &StrokePrimitive, rings: &[Vec<f64>], notices: &mut NoticeList) -> Vec<[Point; 2]> {
    let mut out = Vec::new();
    let period: f64 = stroke.dashes.iter().sum();
    let mut remaining = 100_000usize;
    let mut simplified = false;
    for [x0, y0, x1, y1] in stroke.points.as_chunks::<4>().0 {
        let (start, end) = ([*x0, *y0], [*x1, *y1]);
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        if length.is_nan() || length <= 0.0 {
            continue;
        }
        let direction = [(end[0] - start[0]) / length, (end[1] - start[1]) / length];
        let at = |u: f64| [start[0] + direction[0] * u, start[1] + direction[1] * u];
        for (u0, u1) in inside(start, direction, rings) {
            let (u0, u1) = (u0.max(0.0), u1.min(length));
            if u0 > u1 {
                continue;
            }
            if stroke.dashes.is_empty() || period.is_nan() || period <= 0.0 {
                out.push([at(u0), at(u1)]);
                continue;
            }
            // 極端に密な模様や、加算しても進まない座標は実線にし、処理量を制限する。
            let steps = ((u1 - u0) / period + 2.0) * stroke.dashes.len() as f64;
            if !period.is_finite()
                || steps > remaining as f64
                || u0 + period <= u0
                || stroke.dashes.iter().any(|dash| !dash.is_finite() || *dash < 0.0)
            {
                out.push([at(u0), at(u1)]);
                simplified = true;
                continue;
            }
            // 破線の線（偶数番目）の区間と重なるところ。長さ 0 の線は点
            let mut u = (u0 / period).floor() * period;
            'periods: while u <= u1 {
                let previous = u;
                for (index, dash) in stroke.dashes.iter().enumerate() {
                    if remaining == 0 {
                        out.push([at(u.max(u0)), at(u1)]);
                        simplified = true;
                        break 'periods;
                    }
                    remaining -= 1;
                    if index % 2 == 0 {
                        let (a, b) = (u.max(u0), (u + dash).min(u1));
                        if a <= b && (u + dash >= u0) && u <= u1 {
                            out.push([at(a), at(b)]);
                        }
                    }
                    u += dash;
                    if u > u1 {
                        break 'periods;
                    }
                }
                if u <= previous {
                    out.push([at(u.max(u0)), at(u1)]);
                    simplified = true;
                    break;
                }
            }
        }
    }
    if simplified {
        notices.add(
            NoticeLevel::Warning,
            "version-hatch-density",
            "極端に密なハッチングの破線を、処理の上限を超えないよう実線にしました",
        );
    }
    out
}

/// 点 start から direction の向きの直線が、偶奇規則の境界の中にある区間（start からの長さ）
fn inside(start: Point, direction: Point, rings: &[Vec<f64>]) -> Vec<(f64, f64)> {
    // 直線の左右の側（直線上は右とみなす）。側が変わる辺と交わる
    let side = |p: Point| (p[0] - start[0]) * direction[1] - (p[1] - start[1]) * direction[0] > 0.0;
    let mut crossings: Vec<f64> = Vec::new();
    for ring in rings {
        let count = ring.len() / 2;
        for index in 0..count {
            let next = (index + 1) % count;
            let a = [ring[index * 2], ring[index * 2 + 1]];
            let b = [ring[next * 2], ring[next * 2 + 1]];
            if side(a) == side(b) {
                continue;
            }
            // 交点の、直線に沿った長さ
            let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
            let denominator = direction[0] * ey - direction[1] * ex;
            if denominator == 0.0 {
                continue;
            }
            let u = ((a[0] - start[0]) * ey - (a[1] - start[1]) * ex) / denominator;
            crossings.push(u);
        }
    }
    crossings.sort_by(f64::total_cmp);
    crossings.as_chunks::<2>().0.iter().map(|[a, b]| (*a, *b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(trapezoids: &[[Point; 4]]) -> f64 {
        trapezoids.iter().map(|[p0, p1, p2, p3]| ((p1[0] - p0[0]) + (p2[0] - p3[0])) / 2.0 * (p3[1] - p0[1])).sum()
    }

    #[test]
    fn excessively_dense_hatch_dashes_are_bounded_and_reported() {
        let stroke = StrokePrimitive {
            points: vec![-1.0, 0.5, 2.0, 0.5],
            closed: false,
            segments: Some(true),
            color: "#ffffff".into(),
            width: 0.0,
            dashes: vec![1e-30, 1e-30],
        };
        let mut notices = NoticeList::new();
        let pieces = clip_pattern(&stroke, &[vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0]], &mut notices);
        assert_eq!(pieces, vec![[[0.0, 0.5], [1.0, 0.5]]]);
        assert_eq!(notices.into_vec()[0].code, "version-hatch-density");
    }

    #[test]
    fn maximum_anonymous_block_number_does_not_overflow() {
        let doc = crate::parse_dxf(b"0\nSECTION\n2\nBLOCKS\n0\nBLOCK\n2\n*U18446744073709551615\n0\nENDBLK\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nMTEXT\n10\n0\n20\n0\n40\n1\n1\none\\Ptwo\n0\nENDSEC\n0\nEOF\n");
        let prepared = prepare(&doc);
        let names: HashSet<_> = prepared.doc.blocks.iter().map(|block| block.name.to_uppercase()).collect();
        assert_eq!(names.len(), prepared.doc.blocks.len());
        assert!(prepared.doc.blocks.len() > doc.blocks.len());
    }

    #[test]
    fn splits_fills_with_holes_into_trapezoids() {
        // 10 × 10 の四角から 4 × 4 の穴を抜く（偶奇規則）
        let outer = vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
        let hole = vec![3.0, 3.0, 7.0, 3.0, 7.0, 7.0, 3.0, 7.0];
        let pieces = trapezoids(&[outer.clone(), hole]);
        assert!((area(&pieces) - 84.0).abs() < 1e-9, "{pieces:?}");
        // 三角形と、傾いた四角
        assert!((area(&trapezoids(&[vec![0.0, 0.0, 4.0, 0.0, 0.0, 3.0]])) - 6.0).abs() < 1e-9);
        let diamond = vec![0.0, 5.0, 5.0, 0.0, 10.0, 5.0, 5.0, 10.0];
        let pieces = trapezoids(&[diamond]);
        assert_eq!(pieces.len(), 2);
        assert!((area(&pieces) - 50.0).abs() < 1e-9);
        // 重なる 2 つの四角（偶奇規則で重なりは塗らない）と、自分と交わる輪（蝶ネクタイ）
        let a = vec![0.0, 0.0, 4.0, 0.0, 4.0, 4.0, 0.0, 4.0];
        let b = vec![2.0, 2.0, 6.0, 2.0, 6.0, 6.0, 2.0, 6.0];
        assert!((area(&trapezoids(&[a, b])) - 24.0).abs() < 1e-9);
        let bowtie = vec![0.0, 0.0, 4.0, 4.0, 4.0, 0.0, 0.0, 4.0];
        assert!((area(&trapezoids(&[bowtie])) - 8.0).abs() < 1e-9);
        // 点の足りない輪・空
        assert!(trapezoids(&[vec![0.0, 0.0, 1.0, 1.0]]).is_empty());
        assert!(trapezoids(&[]).is_empty());
    }

    #[test]
    fn clips_pattern_lines_to_the_boundary_with_their_dashes() {
        let outer = vec![0.0, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
        let hole = vec![3.0, 3.0, 7.0, 3.0, 7.0, 7.0, 3.0, 7.0];
        let line = |points: Vec<f64>, dashes: Vec<f64>| StrokePrimitive {
            points,
            closed: false,
            segments: Some(true),
            color: "#ffffff".into(),
            width: 0.0,
            dashes,
        };
        // 実線は穴の前後の 2 本になる
        let pieces = clip_pattern(
            &line(vec![-5.0, 5.0, 15.0, 5.0], Vec::new()),
            &[outer.clone(), hole.clone()],
            &mut NoticeList::new(),
        );
        let close = |a: Point, b: Point| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert!(close(pieces[0][0], [0.0, 5.0]) && close(pieces[0][1], [3.0, 5.0]));
        assert!(close(pieces[1][0], [7.0, 5.0]) && close(pieces[1][1], [10.0, 5.0]));
        // 頂点を通る線（対角線）も、境界の中だけになる
        let pieces = clip_pattern(
            &line(vec![-1.0, -1.0, 11.0, 11.0], Vec::new()),
            &[outer.clone(), hole],
            &mut NoticeList::new(),
        );
        assert_eq!(pieces.len(), 2, "{pieces:?}");
        assert!(close(pieces[0][0], [0.0, 0.0]) && close(pieces[0][1], [3.0, 3.0]));
        // 破線（長さ 2 の線、1 の空白）は線分の始点から数え、点（長さ 0）も残す
        let pieces =
            clip_pattern(&line(vec![-1.0, 1.0, 11.0, 1.0], vec![2.0, 1.0, 0.0, 1.0]), &[outer], &mut NoticeList::new());
        let starts: Vec<f64> = pieces.iter().map(|piece| piece[0][0]).collect();
        // 区間 [-1, 11) の線 [-1, 1)・点 2・線 [3, 5)・点 6・線 [7, 9)・点 10 のうち、境界 [0, 10] の中
        assert_eq!(starts, [0.0, 2.0, 3.0, 6.0, 7.0, 10.0], "{pieces:?}");
        assert!(close(pieces[0][1], [1.0, 1.0]) && close(pieces[1][1], [2.0, 1.0]));
    }

    #[test]
    fn writes_texts_with_percent_and_caret_codes() {
        assert_eq!(text_code("a^b"), "a^ b");
        assert_eq!(text_code("50%"), "50%");
        assert_eq!(text_code("%%d"), "%%037%%037d");
        let runs = crate::parse_text_codes(&text_code("%%c ^ 50%"), 1.0, 1.0, 0.0);
        let text: String = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(text, "%%c ^ 50%");
    }
}
