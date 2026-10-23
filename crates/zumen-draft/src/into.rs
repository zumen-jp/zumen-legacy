//! 受け取った図面にその場で描く（draft_into、決定 0018）。仕様から描いた図面のモデル（`DxfDocument`）を、決定 0017
//! の編集の操作（add・add-block・add-layout・add-text-style など）に写す。操作を `zumen::edit` で当てると、元の
//! 図面のバイト列・ハンドル・拡張データ・zumen が読まない内容は残り、描いたものだけが加わる。
//!
//! - 幾何は `draft` と同じ計算（描いたモデル）から写すので、2 つの経路で形は変わらない。
//! - 描き込む先にある名前（レイヤ・線種・ブロック・寸法スタイル・レイアウト）は、その定義を使う。文字スタイルは、
//!   同じフォントのものがあれば使い、なければフォントの名前で足す。
//! - 用紙の名前と同じレイアウトがあれば、そこに描く（全体のビューポートは元のもの）。なければ add-layout で足す。
//! - 寸法は DIMENSION の操作にし、表示のブロックは編集が寸法スタイルから描く。そのため、寸法スタイルを縮尺と見た目
//!   ごとに足す。黒丸・なしの端の記号は寸法スタイルで表せないので、矢印になる（知らせる）。
//! - 用紙の fields は、描き込む先のその用紙のブロック参照の属性（ATTRIB）も update で埋める。

use crate::dimension::{DimStyle, EndMark, TEXT_GAP, format_number};
use crate::{DraftError, draft_on, spec};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap};
use zumen_core::{Notice, NoticeLevel, NoticeList};
use zumen_dxf::{DxfDocument, DxfEntity, DxfEntityData, DxfSpace, DxfTextBase};

/// 描き込む操作
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// 編集の操作（JSON の配列。`zumen_core::edit::operations_from_json` で読める）
    pub operations: Value,
    /// 描けなかった・形を変えて描いた内容
    pub notices: Vec<Notice>,
    /// 操作ごとの、仕様の図形の場所（編集の誤りを仕様の場所で返すのに使う。仕様の図形でない操作は空）
    pub sources: Vec<String>,
}

/// 仕様から、template（DXF か DWG を読んだモデル）に描き込む編集の操作を作る
pub fn plan(value: &Value, template: &DxfDocument) -> Result<Planned, DraftError> {
    // 仕様の長さは mm。図面の単位が mm でなければ、座標とビューポートの縮尺が合わない
    let units = template.header.insunits;
    if units != 0.0 && units != 4.0 {
        return Err(DraftError::new(
            "invalid-value",
            "",
            format!(
                "描き込む先の図面の長さの単位（$INSUNITS = {units}）が mm ではありません。mm の図面にだけ描き込めます"
            ),
        ));
    }
    let spec = spec::parse(value)?;
    let (drafted, mut drafter) = draft_on(value, Some(template))?;
    let default_style = drafter.text_style(None);
    let mut planner = Planner::new(&drafter.dimension_looks);
    planner.notices = NoticeList::from_notices(&drafted.notices);
    planner.tables(template, &drafted.document, &default_style);
    let sources = &drafted.sources;
    let doc = &drafted.document;
    // ブロックの定義の中身（足したブロックだけ）
    for entity in &doc.entities {
        if entity.base.space != DxfSpace::Block {
            continue;
        }
        let Some(block) = entity.base.block.as_deref() else { continue };
        if !planner.new_blocks.contains(&block.to_uppercase()) {
            continue;
        }
        let container = format!("block:{}", planner.block_name(block));
        planner.entity(entity, &container, source(sources, entity));
    }
    // レイアウト（用紙）。描き込む先にない用紙は足す
    let mut containers: HashMap<String, String> = HashMap::new();
    let mut sheet_layouts: Vec<Option<String>> = Vec::new();
    let mut paper: Vec<&zumen_dxf::DxfLayout> = doc.layouts.iter().filter(|layout| !layout.model).collect();
    paper.sort_by(|a, b| a.tab_order.total_cmp(&b.tab_order));
    for (index, layout) in paper.iter().enumerate() {
        let existing = template
            .layouts
            .iter()
            .find(|candidate| !candidate.model && candidate.name.eq_ignore_ascii_case(&layout.name));
        let name = match existing {
            Some(existing) => {
                if spec.sheets[index].paper.is_some() {
                    planner.push(
                        json!({
                            "op": "update-layout", "name": existing.name,
                            "set": {"paperSize": [layout.paper_width, layout.paper_height]},
                        }),
                        &format!("sheets[{index}].paper"),
                    );
                }
                existing.name.clone()
            }
            None => {
                planner.push(
                    json!({
                        "op": "add-layout",
                        "name": layout.name,
                        "paperSize": [layout.paper_width, layout.paper_height],
                        // 用紙の座標は用紙の角から（draft と同じ）。写したレイアウトの印刷の余白を残さない
                        "margins": layout.margins,
                    }),
                    "",
                );
                layout.name.clone()
            }
        };
        containers.insert(layout.block.to_uppercase(), format!("layout:{name}"));
        sheet_layouts.push(existing.map(|layout| layout.block.clone()));
    }
    // モデル空間と用紙の図形（描いた順）
    for entity in &doc.entities {
        let container = match entity.base.space {
            DxfSpace::Block => continue,
            DxfSpace::Model => "model".to_string(),
            DxfSpace::Paper => {
                let block = entity.base.block.as_deref().unwrap_or("*Paper_Space").to_uppercase();
                match containers.get(&block) {
                    Some(container) => container.clone(),
                    None => continue,
                }
            }
        };
        planner.entity(entity, &container, source(sources, entity));
    }
    // 描き込む先の用紙の表題欄の属性
    for (index, sheet) in spec.sheets.iter().enumerate() {
        let mut filled = drafter.filled.get(index).cloned().unwrap_or_default();
        if let Some(Some(block)) = sheet_layouts.get(index) {
            planner.fill(template, block, &sheet.fields, &mut filled, &format!("sheets[{index}].fields"));
        }
        let unused: Vec<&str> =
            sheet.fields.keys().filter(|tag| !filled.contains(&tag.to_uppercase())).map(String::as_str).collect();
        if !unused.is_empty() {
            planner.notices.add(
                NoticeLevel::Info,
                "unused-field",
                format!("sheets[{index}].fields の {} の属性を持つブロック参照が用紙にありません", unused.join("・")),
            );
        }
    }
    Ok(Planned {
        operations: Value::Array(planner.operations),
        notices: planner.notices.into_vec(),
        sources: planner.sources,
    })
}

fn source(sources: &HashMap<String, String>, entity: &DxfEntity) -> String {
    sources.get(&entity.base.handle).cloned().unwrap_or_default()
}

/// 名前の大文字 → 書く名前
type Names = HashMap<String, String>;

struct Planner {
    operations: Vec<Value>,
    sources: Vec<String>,
    notices: NoticeList,
    /// 描いた名前（大文字）→ 書く名前
    layers: Names,
    line_types: Names,
    styles: Names,
    blocks: Names,
    /// 足したブロック（描いた名前の大文字）
    new_blocks: BTreeSet<String>,
    /// 寸法スタイルの名前 → 縮尺と見た目
    dimension_looks: HashMap<String, (f64, DimStyle)>,
    /// 寸法スタイルの名前（大文字）→ 書く名前
    dimension_styles: Names,
    /// 線種を足すまで待つレイヤの操作
    pending_layers: Vec<Value>,
}

impl Planner {
    fn new(looks: &HashMap<String, (f64, DimStyle)>) -> Self {
        Planner {
            operations: Vec::new(),
            sources: Vec::new(),
            notices: NoticeList::new(),
            layers: Names::new(),
            line_types: Names::new(),
            styles: Names::new(),
            blocks: Names::new(),
            new_blocks: BTreeSet::new(),
            dimension_looks: looks.clone(),
            dimension_styles: Names::new(),
            pending_layers: Vec::new(),
        }
    }

    fn push(&mut self, operation: Value, source: &str) {
        self.operations.push(operation);
        self.sources.push(source.to_string());
    }

    fn info(&mut self, code: &str, message: impl Into<String>) {
        self.notices.add(NoticeLevel::Info, code, message);
    }

    fn mapped(names: &Names, name: &str) -> String {
        names.get(&name.to_uppercase()).cloned().unwrap_or_else(|| name.to_string())
    }

    fn block_name(&self, name: &str) -> String {
        Self::mapped(&self.blocks, name)
    }

    /// レイヤ・線種・文字スタイル・寸法スタイル・ブロックの定義。描き込む先にある名前はその定義を使い、ない名前を足す
    fn tables(&mut self, template: &DxfDocument, doc: &DxfDocument, default_style: &str) {
        let find = |names: Vec<&String>, name: &str| {
            names.into_iter().find(|candidate| candidate.eq_ignore_ascii_case(name)).cloned()
        };
        for layer in &doc.layers {
            let key = layer.name.to_uppercase();
            if let Some(existing) = find(template.layers.iter().map(|layer| &layer.name).collect(), &layer.name) {
                self.layers.insert(key, existing);
                continue;
            }
            let mut operation = Map::new();
            operation.insert("op".into(), json!("add-layer"));
            operation.insert("name".into(), json!(layer.name));
            operation.insert("color".into(), color(layer.color_index, layer.true_color));
            if !layer.linetype.eq_ignore_ascii_case("Continuous") {
                operation.insert("lineType".into(), json!(Self::mapped(&self.line_types, &layer.linetype)));
            }
            if let Some(width) = line_width(layer.lineweight) {
                operation.insert("lineWidth".into(), width);
            }
            if layer.off {
                operation.insert("visible".into(), json!(false));
            }
            if layer.locked {
                operation.insert("locked".into(), json!(true));
            }
            if !layer.plot {
                operation.insert("plot".into(), json!(false));
            }
            self.layers.insert(key, layer.name.clone());
            // レイヤの線種は先に足す（下で線種を足してから、この操作を並べる）
            self.pending_layers.push(Value::Object(operation));
        }
        for line_type in &doc.linetypes {
            let key = line_type.name.to_uppercase();
            if matches!(key.as_str(), "CONTINUOUS" | "BYLAYER" | "BYBLOCK") {
                continue;
            }
            if let Some(existing) =
                find(template.linetypes.iter().map(|line_type| &line_type.name).collect(), &line_type.name)
            {
                self.line_types.insert(key, existing);
                continue;
            }
            self.push(
                json!({"op": "add-line-type", "name": line_type.name, "description": line_type.description, "pattern": line_type.dashes}),
                "",
            );
            self.line_types.insert(key, line_type.name.clone());
        }
        for operation in std::mem::take(&mut self.pending_layers) {
            let mut operation = operation;
            if let Some(Value::String(name)) = operation.get("lineType").cloned() {
                operation["lineType"] = json!(Self::mapped(&self.line_types, &name));
            }
            self.push(operation, "");
        }
        // 文字スタイル: 同じ名前で同じフォントか、同じフォントのスタイルがあれば使う。なければフォントの名前で足す
        for style in &doc.styles {
            let key = style.name.to_uppercase();
            // DXF Reference「STYLE」: 同じ TTC の別の書体や、幅・高さ・傾きの違うスタイルでは文字の見た目が変わる。
            let same_style = |candidate: &zumen_dxf::DxfTextStyle| {
                candidate.font.eq_ignore_ascii_case(&style.font)
                    && match (&candidate.font_family, &style.font_family) {
                        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                        (None, None) => true,
                        _ => false,
                    }
                    && candidate.bigfont.eq_ignore_ascii_case(&style.bigfont)
                    && candidate.height == style.height
                    && candidate.width == style.width
                    && candidate.oblique == style.oblique
                    && candidate.flags == style.flags
                    && candidate.generation_flags == style.generation_flags
            };
            let existing = template
                .styles
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(&style.name) && same_style(candidate))
                .or_else(|| template.styles.iter().find(|candidate| same_style(candidate)));
            if let Some(existing) = existing {
                self.styles.insert(key, existing.name.clone());
                continue;
            }
            let wanted = style.font_family.clone().unwrap_or_else(|| style.name.clone());
            let name = unique(&wanted, template.styles.iter().map(|style| &style.name).chain(self.styles.values()));
            self.push(
                json!({
                    "op": "add-text-style", "name": name,
                    "font": style.font_family.as_deref().unwrap_or(&style.font),
                    "bigFont": style.bigfont, "height": style.height,
                    "widthFactor": style.width, "oblique": style.oblique,
                }),
                "",
            );
            self.styles.insert(key, name);
        }
        // 寸法スタイル（縮尺と見た目ごと）
        let mut looks: Vec<(String, f64, DimStyle)> =
            self.dimension_looks.iter().map(|(name, (scale, look))| (name.clone(), *scale, look.clone())).collect();
        looks.sort_by(|a, b| a.0.cmp(&b.0));
        let text_style = Some(Self::mapped(&self.styles, default_style));
        for (name, scale, look) in looks {
            let key = name.to_uppercase();
            if let Some(existing) = find(template.dimstyles.iter().map(|style| &style.name).collect(), &name) {
                self.dimension_styles.insert(key, existing);
                continue;
            }
            let mut operation = json!({
                "op": "add-dimension-style",
                "name": name,
                "scale": scale,
                "textHeight": look.text_height,
                "arrowSize": look.arrow_size,
                "extensionOffset": look.gap,
                "extensionExtension": look.beyond,
                "textGap": TEXT_GAP,
                "textAbove": true,
                "decimals": look.precision,
                "suppressTrailingZeros": false,
            });
            match look.arrow {
                // 寸法スタイルの斜線（DIMTSZ）は長さ DIMTSZ × √2 で描かれる。作図キットの斜線は長さ arrowSize
                EndMark::Tick => operation["tickSize"] = json!(look.arrow_size / std::f64::consts::SQRT_2),
                EndMark::Arrow => {}
                EndMark::Dot | EndMark::None => self.info(
                    "draft-into-end-mark",
                    format!("寸法スタイル {name} の端の記号（黒丸・なし）は寸法スタイルで表せないため、矢印にしました"),
                ),
            }
            if let Some(style) = &text_style {
                operation["textStyle"] = json!(style);
            }
            self.push(operation, "");
            self.dimension_styles.insert(key, name);
        }
        // ブロックの定義（名前だけ先に足し、中身は後で入れる。ブロックの中で別のブロックを置けるように）
        for block in doc.blocks.iter().filter(|block| !block.anonymous && !block.layout && !block.name.starts_with('*'))
        {
            let key = block.name.to_uppercase();
            if let Some(existing) = find(template.blocks.iter().map(|block| &block.name).collect(), &block.name) {
                self.info(
                    "draft-into-block-exists",
                    format!("ブロック {} は描き込む先にあるため、その定義を使いました", block.name),
                );
                self.blocks.insert(key, existing);
                continue;
            }
            self.push(
                json!({"op": "add-block", "name": block.name, "basePoint": [block.base_point[0], block.base_point[1]]}),
                "",
            );
            self.blocks.insert(key.clone(), block.name.clone());
            self.new_blocks.insert(key);
        }
    }

    /// 図形 1 つを add の操作にする
    fn entity(&mut self, entity: &DxfEntity, container: &str, source: String) {
        let base = &entity.base;
        let mut operation = Map::new();
        operation.insert("op".into(), json!("add"));
        if container != "model" {
            operation.insert("container".into(), json!(container));
        }
        operation.insert("layer".into(), json!(Self::mapped(&self.layers, &base.layer)));
        if base.color_index != 256.0 || base.true_color.is_some() {
            operation.insert("color".into(), color(base.color_index, base.true_color));
        }
        if !base.linetype.eq_ignore_ascii_case("ByLayer") {
            let name = match base.linetype.to_uppercase().as_str() {
                "BYBLOCK" => "ByBlock".to_string(),
                "CONTINUOUS" => "Continuous".to_string(),
                _ => Self::mapped(&self.line_types, &base.linetype),
            };
            operation.insert("lineType".into(), json!(name));
        }
        if let Some(width) = line_width(base.lineweight) {
            operation.insert("lineWidth".into(), width);
        }
        let values = match self.values(entity) {
            Some(values) => values,
            None => return,
        };
        operation.extend(values);
        self.push(Value::Object(operation), &source);
    }

    /// 図形の種類と値。写せない図形は知らせて None
    fn values(&mut self, entity: &DxfEntity) -> Option<Map<String, Value>> {
        let p2 = |p: &[f64; 3]| json!([p[0], p[1]]);
        let value = match &entity.data {
            DxfEntityData::Line { start, end } => json!({"kind": "line", "start": p2(start), "end": p2(end)}),
            DxfEntityData::Circle { center, radius } => {
                json!({"kind": "circle", "center": p2(center), "radius": radius})
            }
            DxfEntityData::Arc { center, radius, start_angle, end_angle } => {
                json!({"kind": "arc", "center": p2(center), "radius": radius, "angle": [start_angle, end_angle]})
            }
            DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param } => {
                let full = (end_param - start_param - std::f64::consts::TAU).abs() < 1e-9;
                json!({
                    "kind": if full { "ellipse" } else { "ellipse-arc" },
                    "center": p2(center),
                    "majorAxis": p2(major_axis),
                    "ratio": ratio,
                    "parameter": [start_param, end_param],
                })
            }
            DxfEntityData::Point { location, .. } => json!({"kind": "point", "position": p2(location)}),
            DxfEntityData::LwPolyline(polyline) => {
                let vertices: Vec<Value> = polyline.vertices.points.iter().map(p2).collect();
                let mut value = json!({"kind": "polyline", "vertices": vertices, "closed": polyline.vertices.closed});
                if polyline.vertices.bulges.iter().any(|bulge| *bulge != 0.0) {
                    value["bulges"] = json!(polyline.vertices.bulges);
                }
                if let Some(width) = polyline.const_width.filter(|width| *width > 0.0) {
                    value["width"] = json!(width);
                }
                value
            }
            DxfEntityData::Text(text) => {
                let mut value = self.text(text);
                value["kind"] = json!("text");
                value
            }
            DxfEntityData::MText(mtext) => {
                let mut value = json!({
                    "kind": "text",
                    "position": p2(&mtext.insert),
                    "text": mtext.text,
                    "height": mtext.char_height,
                    "rotation": mtext.rotation,
                    "textStyle": Self::mapped(&self.styles, &mtext.style),
                    "multiline": true,
                    "justification": ATTACHMENTS.get((mtext.attachment_point as usize).wrapping_sub(1)).copied().unwrap_or("top-left"),
                });
                if (mtext.line_spacing_factor - 1.0).abs() > 1e-9 {
                    value["lineSpacing"] = json!(mtext.line_spacing_factor);
                }
                if let Some(factor) = mtext_width_factor(&mtext.raw_text) {
                    value["widthFactor"] = json!(factor);
                }
                if mtext.rect_width > 0.0 {
                    value["wrapWidth"] = json!(mtext.rect_width);
                }
                value
            }
            DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, .. } => {
                // SOLID の頂点の順は 0・1・3・2。編集では塗りのハッチングにする
                let mut ring: Vec<[f64; 3]> = Vec::new();
                for p in [vtx0, vtx1, vtx3, vtx2] {
                    if ring.last().is_none_or(|last| last != p) && ring.first() != Some(p) {
                        ring.push(*p);
                    }
                }
                if ring.len() < 3 {
                    return None;
                }
                let ring: Vec<Value> = ring.iter().map(p2).collect();
                json!({"kind": "hatch", "boundaries": [ring], "pattern": "SOLID"})
            }
            DxfEntityData::Hatch(hatch) => {
                let boundaries: Vec<Value> =
                    hatch.paths.iter().map(|path| Value::Array(path.points.iter().map(p2).collect())).collect();
                let mut value = json!({"kind": "hatch", "boundaries": boundaries});
                if hatch.solid_fill {
                    value["pattern"] = json!("SOLID");
                } else {
                    // 模様の線は角度・尺度を掛けた後の値なので、尺度 1・角度 0 の PAT の線に戻す（ずれは線の向きの座標系）
                    let lines: Vec<Value> = hatch
                        .pattern_lines
                        .iter()
                        .map(|line| {
                            let (sin, cos) = (-line.angle).to_radians().sin_cos();
                            let offset = [
                                line.offset[0] * cos - line.offset[1] * sin,
                                line.offset[0] * sin + line.offset[1] * cos,
                            ];
                            let mut definition = vec![line.angle, line.base[0], line.base[1], offset[0], offset[1]];
                            definition.extend(&line.dashes);
                            json!(definition)
                        })
                        .collect();
                    value["pattern"] = json!(hatch.pattern_name);
                    value["patternLines"] = Value::Array(lines);
                }
                value
            }
            DxfEntityData::Insert(insert) => {
                let mut value = json!({
                    "kind": "block-reference",
                    "block": self.block_name(&insert.name),
                    "position": p2(&insert.insert),
                    "scale": [insert.xscale, insert.yscale],
                    "rotation": insert.rotation,
                });
                if !insert.attribs.is_empty() {
                    let attributes: Map<String, Value> = insert
                        .attribs
                        .iter()
                        .filter_map(|attrib| match &attrib.data {
                            DxfEntityData::Attrib { text, tag, .. } => Some((tag.clone(), json!(unescape(&text.text)))),
                            _ => None,
                        })
                        .collect();
                    value["attributes"] = Value::Object(attributes);
                }
                value
            }
            DxfEntityData::Attdef { text, tag, prompt, attribute_flags, .. } => {
                if *attribute_flags != 0.0 {
                    self.info("draft-into-attribute-flags", format!("属性 {tag} の表示しない指定は書けませんでした"));
                }
                let mut value = self.text(text);
                let object = value.as_object_mut().expect("an object");
                object.remove("widthFactor");
                object.insert("kind".into(), json!("attribute-definition"));
                object.insert("tag".into(), json!(tag));
                object.insert("prompt".into(), json!(prompt));
                value
            }
            DxfEntityData::Dimension(dimension) => {
                let look = self.dimension_looks.get(&dimension.dimstyle).map(|(_, look)| look);
                let style = Self::mapped(&self.dimension_styles, &dimension.dimstyle);
                let measured = dimension.actual_measurement;
                // 寸法スタイルで表せない値（3 桁ごとの区切り、角度の小数）は、作図キットの値を文字で渡す
                let (mut value, shown) = match dimension.dimtype as i64 & 7 {
                    kind @ (0 | 1) => {
                        let (Some(start), Some(end)) = (dimension.defpoint2, dimension.defpoint3) else { return None };
                        let aligned = kind == 1;
                        let mut value = json!({
                            "kind": "dimension",
                            "dimensionType": if aligned { "aligned" } else { "linear" },
                            "start": p2(&start),
                            "end": p2(&end),
                            "position": p2(&dimension.defpoint),
                        });
                        if !aligned {
                            value["rotation"] = json!(dimension.angle.unwrap_or(0.0));
                        }
                        let shown = look
                            .zip(measured)
                            .map(|(look, measured)| format_number(measured, look.precision, look.thousands));
                        (value, shown.filter(|shown| shown.contains(',')))
                    }
                    5 => {
                        let (Some(start), Some(end), Some(center)) =
                            (dimension.defpoint2, dimension.defpoint3, dimension.defpoint4)
                        else {
                            return None;
                        };
                        let value = json!({
                            "kind": "dimension",
                            "dimensionType": "angular-3-point",
                            "center": p2(&center),
                            "start": p2(&start),
                            "end": p2(&end),
                            "position": p2(&dimension.defpoint),
                        });
                        // 足す寸法スタイルの角度の小数（DIMADEC）は Standard の 0
                        let shown =
                            look.zip(measured).filter(|(look, _)| look.precision > 0).map(|(look, measured)| {
                                format!("{}%%d", format_number(measured.to_degrees(), look.precision, false))
                            });
                        (value, shown)
                    }
                    kind @ (3 | 4) => {
                        let point = dimension.defpoint4?;
                        let diameter = kind == 3;
                        let center = if diameter {
                            [(dimension.defpoint[0] + point[0]) / 2.0, (dimension.defpoint[1] + point[1]) / 2.0, 0.0]
                        } else {
                            dimension.defpoint
                        };
                        let value = json!({
                            "kind": "dimension",
                            "dimensionType": if diameter { "diameter" } else { "radius" },
                            "center": p2(&center),
                            "point": p2(&point),
                        });
                        let prefix = if diameter { "%%c" } else { "R" };
                        let shown = look
                            .zip(measured)
                            .map(|(look, measured)| format_number(measured, look.precision, look.thousands));
                        (value, shown.filter(|shown| shown.contains(',')).map(|shown| format!("{prefix}{shown}")))
                    }
                    _ => return None,
                };
                value["dimensionStyle"] = json!(style);
                let text = match (dimension.text.as_str(), shown) {
                    ("", shown) => shown,
                    (text, Some(shown)) => Some(text.replace("<>", &shown)),
                    (text, None) => Some(text.to_string()),
                };
                if let Some(text) = text {
                    value["text"] = json!(text);
                }
                value
            }
            DxfEntityData::Viewport(viewport) => {
                // 全体のビューポート（ID 1）は、元のレイアウトか add-layout のものを使う
                if viewport.id == 1.0 {
                    return None;
                }
                json!({
                    "kind": "viewport",
                    "center": p2(&viewport.center),
                    "width": viewport.width,
                    "height": viewport.height,
                    "viewCenter": viewport.view_center,
                    "scale": viewport.view_height / viewport.height,
                })
            }
            _ => {
                self.info("draft-into-skipped", format!("{} は編集で足せないため、描きませんでした", entity.base.kind));
                return None;
            }
        };
        match value {
            Value::Object(map) => Some(map),
            _ => None,
        }
    }

    /// TEXT・ATTDEF の文字の値
    fn text(&self, text: &DxfTextBase) -> Value {
        let position = text.align_point.filter(|_| text.halign != 0.0 || text.valign != 0.0).unwrap_or(text.insert);
        let mut value = json!({
            "position": [position[0], position[1]],
            "text": unescape(&text.text),
            "height": text.height,
            "rotation": text.rotation,
            "textStyle": Self::mapped(&self.styles, &text.style),
        });
        if let Some(justification) = justification(text.halign as i64, text.valign as i64) {
            value["justification"] = json!(justification);
        }
        if (text.width - 1.0).abs() > 1e-9 && text.width > 0.0 {
            value["widthFactor"] = json!(text.width);
        }
        value
    }

    /// 描き込む先の用紙（紙空間のブロック block）のブロック参照の属性に、fields の値を update で書く
    fn fill(
        &mut self,
        template: &DxfDocument,
        block: &str,
        fields: &std::collections::BTreeMap<String, String>,
        filled: &mut BTreeSet<String>,
        path: &str,
    ) {
        if fields.is_empty() {
            return;
        }
        let active = template.layouts.iter().any(|layout| layout.active && layout.block.eq_ignore_ascii_case(block));
        for entity in &template.entities {
            let on_sheet = entity.base.space == DxfSpace::Paper
                && match entity.base.block.as_deref() {
                    Some(name) => name.eq_ignore_ascii_case(block),
                    None => active,
                };
            if !on_sheet {
                continue;
            }
            let DxfEntityData::Insert(insert) = &entity.data else { continue };
            for attrib in &insert.attribs {
                let DxfEntityData::Attrib { tag, .. } = &attrib.data else { continue };
                let Some((key, value)) = fields.iter().find(|(key, _)| key.eq_ignore_ascii_case(tag)) else { continue };
                let id = if attrib.base.handle.is_empty() {
                    format!("#{}", attrib.base.ordinal)
                } else {
                    attrib.base.handle.clone()
                };
                // 編集の TEXT・ATTRIB は %% の制御コードを受け取る。仕様の値は文字のまま表示する。
                // キャレット表記は編集が書くときに付けるので、ここでは戻して渡す。
                let value = unescape(&crate::text::escape_text(value));
                self.push(json!({"op": "update", "id": id, "set": {"text": value}}), path);
                filled.insert(key.to_uppercase());
            }
        }
    }
}

/// MTEXT の配置（71 の 1〜9）の名前
const ATTACHMENTS: [&str; 9] = [
    "top-left",
    "top-center",
    "top-right",
    "middle-left",
    "middle-center",
    "middle-right",
    "bottom-left",
    "bottom-center",
    "bottom-right",
];

/// TEXT の 72・73 の位置合わせの名前。左・ベースラインは None
fn justification(horizontal: i64, vertical: i64) -> Option<String> {
    let h = ["left", "center", "right"].get(horizontal as usize)?;
    match vertical {
        0 if horizontal == 0 => None,
        0 => Some(h.to_string()),
        1 => Some(format!("bottom-{h}")),
        2 => Some(format!("middle-{h}")),
        3 => Some(format!("top-{h}")),
        _ => None,
    }
}

/// 作図キットが MTEXT の頭に付けた幅の係数（\W…;）
fn mtext_width_factor(raw: &str) -> Option<f64> {
    let rest = raw.strip_prefix("\\W")?;
    let (number, _) = rest.split_once(';')?;
    number.parse().ok()
}

/// TEXT の文字の、DXF の書き方（^ の後の空白）を戻す。編集が書くときに書き方にする
fn unescape(text: &str) -> String {
    text.replace("^ ", "^")
}

/// 色の値（ACI 256 は bylayer、0 は byblock、トゥルーカラーは #rrggbb）
fn color(index: f64, true_color: Option<f64>) -> Value {
    if let Some(rgb) = true_color {
        return json!(format!("#{:06x}", rgb as u32 & 0xFF_FFFF));
    }
    match index as i64 {
        256 => json!("bylayer"),
        0 => json!("byblock"),
        index => json!(index.abs()),
    }
}

/// 線の太さ（1/100 mm）。BYLAYER は省く
fn line_width(lineweight: f64) -> Option<Value> {
    match lineweight as i64 {
        -1 => None,
        -2 => Some(json!("byblock")),
        -3 => Some(json!("default")),
        width => Some(json!(width as f64 / 100.0)),
    }
}

/// 描き込む先の名前と重ならない名前
fn unique<'a>(wanted: &str, existing: impl Iterator<Item = &'a String>) -> String {
    let taken: BTreeSet<String> = existing.map(|name| name.to_uppercase()).collect();
    if !taken.contains(&wanted.to_uppercase()) {
        return wanted.to_string();
    }
    (2..).map(|number| format!("{wanted}-{number}")).find(|name| !taken.contains(&name.to_uppercase())).expect("a name")
}
