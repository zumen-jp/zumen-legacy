//! SXF（SFC・P21、圧縮した SFZ・P2Z）のその場の編集（docs/decisions/0012、docs/formats/sxf.md「Editing SXF」）。
//! 読んだ図面（`SxfDrawing`）に操作を当て、読んだ形式で書く。書き出し（`write_sxf`）は元のファイルを書き換えるので、
//! 操作した要素のフィーチャ・インスタンスだけが書き直され、ほかは元の文字列のまま残る。圧縮した SXF は同じ書庫に戻る。
//!
//! - id は表示の id（用紙の要素は `#n`、部分図の要素は配置から辿った `#配置>#n`）。部分図（数学・測地座標系）は
//!   DXF のビューポートと同じく、配置を通して見える要素そのものを選ぶ。作図部品・作図グループの中の要素は、
//!   変えると定義とそのすべての配置が変わるので、DXF のブロックの中と同じく選べない（inside-block）。
//! - container は `sheet`（省略したとき）と `figure:<複合図形の名前>`。
//! - 座標は図面自体の座標（用紙の要素は用紙の mm、部分図の要素は部分図の座標）で、読み取りの property の raw と同じ数。
//!   角度は度（楕円弧の parameter だけは DXF と同じくラジアンで受け、SXF の度にする）。
//! - SXF にない指定（レイヤから継ぐ色・線幅、レイヤの色・線種・線幅）は、いちばん近い値にして知らせる。

use crate::document::{SubFigure, SxfDrawing};
use crate::features::{
    Annotation, DrawingSheet, LayerDefinition, Styled, SxfFeature, TextFont, UserColour, WidthDefinition,
};
use crate::tables::{PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, predefined_width_code};
use crate::{SxfHeader, parse_sxf, write_sxf};
use std::f64::consts::PI;
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValues, Edited, ValueReader,
};
use zumen_core::{NoticeLevel, NoticeList, js};

/// 足せる要素の種類
const ADDABLE: [&str; 9] =
    ["line", "polyline", "arc", "circle", "ellipse", "ellipse-arc", "point", "text", BLOCK_REFERENCE];

/// 要素に共通する値のキー
const COMMON: [&str; 4] = ["layer", "color", "lineType", "lineWidth"];

/// 新しい要素の既定の色（黒）・線種（実線）・線幅（0.25 mm、コード 3）
const DEFAULT_COLOR: f64 = 1.0;
const DEFAULT_LINE_TYPE: f64 = 1.0;
const DEFAULT_WIDTH_CODE: f64 = 3.0;

/// 要素のありか（用紙か、複合図形の名前）と、並びの中の位置
#[derive(Debug, Clone, PartialEq)]
struct Place {
    figure: Option<String>,
    index: usize,
}

struct Editor {
    drawing: SxfDrawing,
    notices: NoticeList,
    /// 足した要素の仮の番号（負の数）。書いた後に書いた番号にする
    next: f64,
    /// 足したもの（要素の仮の番号、またはレイヤの名前）を操作の順に
    created: Vec<Created>,
}

enum Created {
    Feature(f64),
    Layer(String),
}

/// SXF を編集する。すべての操作を確かめてから書き、1 つでも通らなければエラーを返して何も書かない。
/// format は sfc・p21・sfz・p2z
pub fn edit_sxf(bytes: &[u8], format: &str, operations: &[EditOperation]) -> Result<Edited, EditError> {
    if !matches!(format, "sfc" | "p21" | "sfz" | "p2z") {
        return Err(EditError::new("unsupported-format", format!("SXF の編集は {format} を扱えません")));
    }
    let drawing = parse_sxf(bytes, format)
        .map_err(|error| EditError::new("unreadable", format!("SXF として読めません: {error}")))?;
    if operations.is_empty() {
        return Ok(Edited { bytes: bytes.to_vec(), created: Vec::new(), notices: Vec::new() });
    }
    let mut editor = Editor { drawing, notices: NoticeList::new(), next: -1.0, created: Vec::new() };
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            EditOperation::Add { kind, container, values } => editor.add(index, kind, container.as_deref(), values)?,
            EditOperation::Update { id, set } => editor.update(index, id, set)?,
            EditOperation::Delete { id } => editor.delete(index, id)?,
            EditOperation::AddLayer { name, values } => editor.add_layer(index, name, values)?,
            EditOperation::UpdateLayer { name, set } => editor.update_layer(index, name, set)?,
        }
    }
    let written =
        write_sxf(&editor.drawing, format).map_err(|error| EditError::new("unreadable", error.to_string()))?;
    // 書き手の知らせも、編集の知らせとして edit- を付ける（DXF・JWW の編集と同じ）
    let mut notices = editor.notices;
    for notice in &written.notices {
        let code = if notice.code.starts_with("edit-") { notice.code.clone() } else { format!("edit-{}", notice.code) };
        notices.add_count(notice.level, &code, notice.message.clone(), notice.count);
    }
    let created = editor
        .created
        .iter()
        .map(|created| match created {
            Created::Layer(name) => name.clone(),
            Created::Feature(temporary) => {
                let id = written.ids.iter().find(|(id, _)| id == temporary).map_or(*temporary, |(_, new)| *new);
                format!("#{}", js::number_to_string(id))
            }
        })
        .collect();
    Ok(Edited { bytes: written.bytes, created, notices: notices.into_vec() })
}

/// 新しい図面（A3 横の用紙だけの SXF）を format（sfc・p21・sfz・p2z）で書く。`edit_sxf` で要素を足す
pub fn new_sxf(format: &str) -> Result<Vec<u8>, EditError> {
    let drawing = SxfDrawing {
        format: if matches!(format, "p21" | "p2z") { "p21" } else { "sfc" },
        version: "SXF 3.1".into(),
        header: SxfHeader {
            file_name: format!("drawing.{}", if matches!(format, "p21" | "p2z") { "p21" } else { "sfc" }),
            ..SxfHeader::default()
        },
        document: crate::SxfDocument {
            sheet: Some(DrawingSheet { id: -1.0, name: String::new(), sheet_type: 3.0, orient: 1.0, x: 0.0, y: 0.0 }),
            ..crate::SxfDocument::default()
        },
        notices: Vec::new(),
        source: None,
    };
    write_sxf(&drawing, format)
        .map(|written| written.bytes)
        .map_err(|error| EditError::new("unsupported-format", error.to_string()))
}

impl Editor {
    fn temporary(&mut self) -> f64 {
        let id = self.next;
        self.next -= 1.0;
        id
    }

    /// 要素の並び（用紙か複合図形）
    fn members(&self, figure: &Option<String>) -> &Vec<SxfFeature> {
        match figure {
            None => &self.drawing.document.sheet_members,
            Some(name) => &self.drawing.document.sub_figures.get(name).expect("ある複合図形").members,
        }
    }

    fn members_mut(&mut self, figure: &Option<String>, change: impl FnOnce(&mut Vec<SxfFeature>)) {
        match figure {
            None => change(&mut self.drawing.document.sheet_members),
            Some(name) => {
                let mut sub_figure: SubFigure =
                    self.drawing.document.sub_figures.get(name).expect("ある複合図形").clone();
                change(&mut sub_figure.members);
                self.drawing.document.sub_figures.set(name.clone(), sub_figure);
            }
        }
    }

    /// 表示の id から要素を探す。部分図の配置の中は辿り、作図部品・作図グループの中は選べない
    fn resolve(&self, index: usize, id: &str) -> Result<Place, EditError> {
        let not_found = || EditError::at(index, "not-found", format!("{id} の要素は図面にありません"));
        let numbers: Vec<f64> = id
            .split('>')
            .map(|part| part.strip_prefix('#').and_then(|number| number.parse::<f64>().ok()))
            .collect::<Option<_>>()
            .ok_or_else(not_found)?;
        let mut figure: Option<String> = None;
        for (depth, number) in numbers.iter().enumerate() {
            let members = self.members(&figure);
            let position = members.iter().position(|feature| feature.id() == *number).ok_or_else(not_found)?;
            if depth + 1 == numbers.len() {
                return Ok(Place { figure, index: position });
            }
            let SxfFeature::SfigLocate { name, .. } = &members[position] else {
                return Err(not_found());
            };
            let sub_figure = self.drawing.document.sub_figures.get(name).ok_or_else(not_found)?;
            if !matches!(sub_figure.feature.flag as i64, 1 | 2) {
                let kind = if sub_figure.feature.flag == 3.0 { "作図グループ" } else { "作図部品" };
                return Err(EditError::at(
                    index,
                    "inside-block",
                    format!(
                        "{id} は{kind}「{name}」の中の要素で、変えると定義とそのすべての配置が変わります。配置は {} です",
                        numbers[..=depth]
                            .iter()
                            .map(|n| format!("#{}", js::number_to_string(*n)))
                            .collect::<Vec<_>>()
                            .join(">")
                    ),
                ));
            }
            figure = Some(name.clone());
        }
        Err(not_found())
    }

    fn container(&self, index: usize, container: Option<&str>) -> Result<Option<String>, EditError> {
        match container {
            None | Some("sheet") => Ok(None),
            Some(container) => match container.strip_prefix("figure:") {
                Some(name) if self.drawing.document.sub_figures.contains(name) => Ok(Some(name.to_string())),
                Some(name) => {
                    Err(EditError::at(index, "missing-definition", format!("複合図形 {name} は図面にありません")))
                }
                None => Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("container の {container} は使えません（sheet、figure:<複合図形の名前>）"),
                )),
            },
        }
    }

    // ---- 共通の値 ----

    /// レイヤの名前からコード。図面にないレイヤは足して知らせる
    fn layer_code(&mut self, name: &str) -> f64 {
        if let Some(position) = self.drawing.document.layers.iter().position(|layer| layer.name == name) {
            return (position + 1) as f64;
        }
        let id = self.temporary();
        self.drawing.document.layers.push(LayerDefinition { id, name: name.to_string(), visible: true });
        self.notices.add(NoticeLevel::Info, "edit-added-layer", "図面にないレイヤを足しました");
        self.drawing.document.layers.len() as f64
    }

    fn color_code(&mut self, index: usize, color: EditColor) -> Result<f64, EditError> {
        let doc = &self.drawing.document;
        Ok(match color {
            EditColor::Index(code) => {
                let code = f64::from(code);
                if doc.color(code).is_none() {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        format!("色の番号 {code} は SXF にありません（既定義色 1〜16、ユーザ定義色 17 から）"),
                    ));
                }
                code
            }
            EditColor::Rgb(value) => {
                let rgb = [(value >> 16) as u8, (value >> 8) as u8, value as u8];
                if let Some(position) = PREDEFINED_COLORS.iter().position(|color| color.rgb == rgb) {
                    return Ok((position + 1) as f64);
                }
                let [r, g, b] = rgb.map(f64::from);
                if let Some(position) =
                    doc.user_colors.iter().position(|color| [color.r, color.g, color.b] == [r, g, b])
                {
                    return Ok((position + 17) as f64);
                }
                let id = self.temporary();
                self.drawing.document.user_colors.push(UserColour { id, r, g, b });
                (self.drawing.document.user_colors.len() + 16) as f64
            }
            EditColor::ByLayer | EditColor::ByBlock => {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-color-by-layer",
                    "SXF の色はレイヤ・配置から継がないため、黒にしました",
                );
                DEFAULT_COLOR
            }
        })
    }

    fn line_type_code(&self, index: usize, name: &str) -> Result<f64, EditError> {
        let lower = js::trim(name).to_lowercase();
        if let Some(position) = PREDEFINED_LINE_TYPES.iter().position(|line_type| line_type.name == lower) {
            return Ok((position + 1) as f64);
        }
        let doc = &self.drawing.document;
        doc.user_line_types
            .iter()
            .position(|line_type| line_type.name == name || line_type.name.trim_start_matches("$$SXF_") == name)
            .map(|position| (position + 17) as f64)
            .ok_or_else(|| EditError::at(index, "missing-definition", format!("線種 {name} は図面にありません")))
    }

    fn width_code(&mut self, width: EditLineWidth) -> f64 {
        match width {
            EditLineWidth::Millimeters(width) => {
                let predefined = predefined_width_code(width);
                if predefined > 0 {
                    return predefined as f64;
                }
                let doc = &self.drawing.document;
                if let Some(position) = doc.user_widths.iter().position(|user| (user.width - width).abs() < 1e-9) {
                    return (position + 11) as f64;
                }
                let id = self.temporary();
                self.drawing.document.user_widths.push(WidthDefinition { id, width });
                (self.drawing.document.user_widths.len() + 10) as f64
            }
            EditLineWidth::Default => DEFAULT_WIDTH_CODE,
            EditLineWidth::ByLayer | EditLineWidth::ByBlock => {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-width-by-layer",
                    "SXF の線幅はレイヤ・配置から継がないため、0.25 mm にしました",
                );
                DEFAULT_WIDTH_CODE
            }
        }
    }

    /// 共通の値（レイヤ・色・線種・線幅）を当てる。指定のないものは style のまま
    fn apply_common(&mut self, index: usize, reader: &ValueReader<'_>, style: &mut Styled) -> Result<(), EditError> {
        if let Some(name) = reader.name("layer")? {
            style.layer = self.layer_code(name);
        }
        if let Some(color) = reader.color("color")? {
            style.color = self.color_code(index, color)?;
        }
        if let Some(name) = reader.name("lineType")? {
            style.line_type = self.line_type_code(index, name)?;
        }
        if let Some(width) = reader.line_width("lineWidth")? {
            style.line_width = self.width_code(width);
        }
        Ok(())
    }

    /// 文字フォントの名前からコード。図面にないフォントは足す
    fn font_code(&mut self, name: &str) -> f64 {
        if let Some(position) = self.drawing.document.fonts.iter().position(|font| font.name == name) {
            return (position + 1) as f64;
        }
        let id = self.temporary();
        self.drawing.document.fonts.push(TextFont { id, name: name.to_string() });
        self.drawing.document.fonts.len() as f64
    }

    // ---- 操作 ----

    fn add(&mut self, index: usize, kind: &str, container: Option<&str>, values: &EditValues) -> Result<(), EditError> {
        if !ADDABLE.contains(&kind) {
            return Err(EditError::at(
                index,
                "unsupported-kind",
                format!("SXF の編集では {kind} を足せません（足せるのは {}）", ADDABLE.join("、")),
            ));
        }
        let figure = self.container(index, container)?;
        let reader = ValueReader::new(values, index);
        let keys = |own: &[&'static str]| -> Vec<&'static str> { COMMON.iter().chain(own).copied().collect() };
        let id = self.temporary();
        let mut style = Styled {
            id,
            layer: 0.0,
            color: DEFAULT_COLOR,
            line_type: DEFAULT_LINE_TYPE,
            line_width: DEFAULT_WIDTH_CODE,
        };
        let feature = match kind {
            "line" => {
                reader.allow(&keys(&["start", "end"]), "線分")?;
                let start = reader.required("start", reader.point("start")?)?;
                let end = reader.required("end", reader.point("end")?)?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Line { style, x1: start[0], y1: start[1], x2: end[0], y2: end[1] }
            }
            "polyline" => {
                reader.allow(&keys(&["vertices", "closed", "bulges", "width"]), "折線")?;
                let vertices = reader.required("vertices", reader.points("vertices", 2)?)?;
                no_bulges_or_width(index, &reader, vertices.len())?;
                let closed = reader.bool("closed")?.unwrap_or(false);
                self.apply_common(index, &reader, &mut style)?;
                let (xs, ys) = polyline_points(&vertices, closed);
                SxfFeature::Polyline { style, xs, ys }
            }
            "circle" => {
                reader.allow(&keys(&["center", "radius"]), "円")?;
                let center = reader.required("center", reader.point("center")?)?;
                let r = reader.required("radius", reader.positive("radius")?)?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Circle { style, cx: center[0], cy: center[1], r }
            }
            "arc" => {
                reader.allow(&keys(&["center", "radius", "angle"]), "円弧")?;
                let center = reader.required("center", reader.point("center")?)?;
                let r = reader.required("radius", reader.positive("radius")?)?;
                let angle = reader.required("angle", reader.numbers("angle", 2)?)?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Arc { style, cx: center[0], cy: center[1], r, ccw: true, start: angle[0], end: angle[1] }
            }
            "ellipse" | "ellipse-arc" => {
                reader.allow(&keys(&["center", "majorAxis", "ratio", "parameter"]), "楕円")?;
                let center = reader.required("center", reader.point("center")?)?;
                let (rx, ry, rotation) = ellipse_axes(index, &reader)?;
                let parameter = match reader.numbers("parameter", 2)? {
                    Some(parameter) => Some(parameter),
                    None if kind == "ellipse" => None,
                    None => return Err(EditError::at(index, "invalid-value", "parameter がありません")),
                };
                self.apply_common(index, &reader, &mut style)?;
                match parameter {
                    None => SxfFeature::Ellipse { style, cx: center[0], cy: center[1], rx, ry, rotation },
                    Some(parameter) => SxfFeature::EllipseArc {
                        style,
                        cx: center[0],
                        cy: center[1],
                        rx,
                        ry,
                        ccw: true,
                        rotation,
                        start: parameter[0] * 180.0 / PI,
                        end: parameter[1] * 180.0 / PI,
                    },
                }
            }
            "point" => {
                reader.allow(&keys(&["position"]), "点")?;
                let position = reader.required("position", reader.point("position")?)?;
                self.apply_common(index, &reader, &mut style)?;
                // 点マーカは色だけを持つ（線種・線幅は持たない）。形は点（dot、コード 3）
                SxfFeature::PointMarker {
                    id,
                    layer: style.layer,
                    color: style.color,
                    x: position[0],
                    y: position[1],
                    code: 3.0,
                    angle: 0.0,
                    scale: 1.0,
                }
            }
            "text" => {
                reader.allow(&keys(&["position", "text", "height", "rotation", "textStyle", "width"]), "文字")?;
                let position = reader.required("position", reader.point("position")?)?;
                let text = reader.required("text", reader.text("text")?)?;
                if text.contains(['\n', '\r']) {
                    return Err(EditError::at(
                        index,
                        "unsupported-kind",
                        "SXF の文字は 1 行です（複数行の文字はありません）",
                    ));
                }
                let height = reader.required("height", reader.positive("height")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let font = match reader.name("textStyle")? {
                    Some(name) => self.font_code(name),
                    None => 0.0,
                };
                let width = reader.positive("width")?.unwrap_or_else(|| natural_width(text, height));
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::TextString {
                    id,
                    layer: style.layer,
                    color: style.color,
                    text: Annotation {
                        font,
                        text: text.to_string(),
                        x: position[0],
                        y: position[1],
                        height,
                        width,
                        spacing: 0.0,
                        angle: rotation,
                        slant: 0.0,
                        anchor: 1.0,
                        direction: 1.0,
                    },
                }
            }
            _ => {
                reader.allow(&keys(&["block", "position", "rotation", "scale"]), "複合図形の配置")?;
                let name = reader.required("block", reader.name("block")?)?;
                if !self.drawing.document.sub_figures.contains(name) {
                    return Err(EditError::at(
                        index,
                        "missing-definition",
                        format!("複合図形 {name} は図面にありません"),
                    ));
                }
                let position = reader.required("position", reader.point("position")?)?;
                let rotation = reader.number("rotation")?.unwrap_or(0.0);
                let (ratio_x, ratio_y) = scale(index, &reader)?.unwrap_or((1.0, 1.0));
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::SfigLocate {
                    id,
                    layer: style.layer,
                    name: name.to_string(),
                    x: position[0],
                    y: position[1],
                    angle: rotation,
                    ratio_x,
                    ratio_y,
                }
            }
        };
        let unused = match kind {
            "point" | "text" => reader.has("lineType") || reader.has("lineWidth"),
            BLOCK_REFERENCE => reader.has("color") || reader.has("lineType") || reader.has("lineWidth"),
            _ => false,
        };
        if unused {
            self.notices.add(
                NoticeLevel::Info,
                "edit-unused-attributes",
                "点・文字の線種と線幅、または複合図形配置の色・線種・線幅は SXF にないため、指定を使っていません",
            );
        }
        self.members_mut(&figure, |members| members.push(feature));
        self.created.push(Created::Feature(id));
        Ok(())
    }

    fn update(&mut self, index: usize, id: &str, set: &EditValues) -> Result<(), EditError> {
        let place = self.resolve(index, id)?;
        let mut feature = self.members(&place.figure)[place.index].clone();
        let reader = ValueReader::new(set, index);
        let keys = |own: &[&'static str]| -> Vec<&'static str> { COMMON.iter().chain(own).copied().collect() };
        match &mut feature {
            SxfFeature::Line { style, x1, y1, x2, y2 } => {
                reader.allow(&keys(&["start", "end"]), "線分")?;
                if let Some([x, y, _]) = reader.point("start")? {
                    (*x1, *y1) = (x, y);
                }
                if let Some([x, y, _]) = reader.point("end")? {
                    (*x2, *y2) = (x, y);
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::Polyline { style, xs, ys } => {
                reader.allow(&keys(&["vertices", "closed", "bulges", "width"]), "折線")?;
                let closed = reader.bool("closed")?;
                let vertices = reader.points("vertices", 2)?;
                if vertices.is_some() || closed.is_some() {
                    let current: Vec<[f64; 3]> = xs.iter().zip(ys.iter()).map(|(x, y)| [*x, *y, 0.0]).collect();
                    let was_closed = current.len() > 2 && current.first() == current.last();
                    let mut vertices = vertices.unwrap_or_else(|| {
                        if was_closed { current[..current.len() - 1].to_vec() } else { current.clone() }
                    });
                    no_bulges_or_width(index, &reader, vertices.len())?;
                    let closed = closed.unwrap_or(was_closed);
                    if closed && vertices.len() > 2 && vertices.first() == vertices.last() {
                        vertices.pop();
                    }
                    (*xs, *ys) = polyline_points(&vertices, closed);
                } else {
                    no_bulges_or_width(index, &reader, xs.len())?;
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::Circle { style, cx, cy, r } => {
                reader.allow(&keys(&["center", "radius"]), "円")?;
                if let Some([x, y, _]) = reader.point("center")? {
                    (*cx, *cy) = (x, y);
                }
                if let Some(radius) = reader.positive("radius")? {
                    *r = radius;
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::Arc { style, cx, cy, r, ccw, start, end } => {
                reader.allow(&keys(&["center", "radius", "angle"]), "円弧")?;
                if let Some([x, y, _]) = reader.point("center")? {
                    (*cx, *cy) = (x, y);
                }
                if let Some(radius) = reader.positive("radius")? {
                    *r = radius;
                }
                // 表示の angle は反時計回りの始角・終角（時計回りの円弧は入れ替えた値）
                if let Some(angle) = reader.numbers("angle", 2)? {
                    (*ccw, *start, *end) = (true, angle[0], angle[1]);
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::Ellipse { style, cx, cy, rx, ry, rotation } => {
                reader.allow(&keys(&["center", "majorAxis", "ratio"]), "楕円")?;
                if let Some([x, y, _]) = reader.point("center")? {
                    (*cx, *cy) = (x, y);
                }
                if reader.has("majorAxis") || reader.has("ratio") {
                    (*rx, *ry, *rotation) = updated_axes(index, &reader, *rx, *ry, *rotation)?;
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::EllipseArc { style, cx, cy, rx, ry, ccw, rotation, start, end } => {
                reader.allow(&keys(&["center", "majorAxis", "ratio", "parameter"]), "楕円弧")?;
                if let Some([x, y, _]) = reader.point("center")? {
                    (*cx, *cy) = (x, y);
                }
                if reader.has("majorAxis") || reader.has("ratio") {
                    (*rx, *ry, *rotation) = updated_axes(index, &reader, *rx, *ry, *rotation)?;
                }
                if let Some(parameter) = reader.numbers("parameter", 2)? {
                    (*ccw, *start, *end) = (true, parameter[0] * 180.0 / PI, parameter[1] * 180.0 / PI);
                }
                self.apply_common(index, &reader, style)?;
            }
            SxfFeature::PointMarker { layer, color, x, y, .. } => {
                reader.allow(&["layer", "color", "position"], "点マーカ")?;
                if let Some([px, py, _]) = reader.point("position")? {
                    (*x, *y) = (px, py);
                }
                let mut style = Styled { id: 0.0, layer: *layer, color: *color, line_type: 1.0, line_width: 1.0 };
                self.apply_common(index, &reader, &mut style)?;
                (*layer, *color) = (style.layer, style.color);
            }
            SxfFeature::TextString { layer, color, text, .. } => {
                reader.allow(
                    &["layer", "color", "position", "text", "height", "rotation", "textStyle", "width"],
                    "文字",
                )?;
                if let Some([x, y, _]) = reader.point("position")? {
                    (text.x, text.y) = (x, y);
                }
                if let Some(value) = reader.text("text")? {
                    if value.contains(['\n', '\r']) {
                        return Err(EditError::at(
                            index,
                            "unsupported-kind",
                            "SXF の文字は 1 行です（複数行の文字はありません）",
                        ));
                    }
                    text.text = value.to_string();
                }
                if let Some(height) = reader.positive("height")? {
                    // 文字範囲の幅は、高さと同じ割合で変える（幅の指定があればそれ）
                    text.width *= height / text.height;
                    text.height = height;
                }
                if let Some(width) = reader.positive("width")? {
                    text.width = width;
                }
                if let Some(rotation) = reader.number("rotation")? {
                    text.angle = rotation;
                }
                if let Some(name) = reader.name("textStyle")? {
                    text.font = self.font_code(name);
                }
                let mut style = Styled { id: 0.0, layer: *layer, color: *color, line_type: 1.0, line_width: 1.0 };
                self.apply_common(index, &reader, &mut style)?;
                (*layer, *color) = (style.layer, style.color);
            }
            SxfFeature::SfigLocate { layer, name, x, y, angle, ratio_x, ratio_y, .. } => {
                reader.allow(&["layer", "block", "position", "rotation", "scale"], "複合図形の配置")?;
                if let Some(block) = reader.name("block")? {
                    if !self.drawing.document.sub_figures.contains(block) {
                        return Err(EditError::at(
                            index,
                            "missing-definition",
                            format!("複合図形 {block} は図面にありません"),
                        ));
                    }
                    *name = block.to_string();
                }
                if let Some([px, py, _]) = reader.point("position")? {
                    (*x, *y) = (px, py);
                }
                if let Some(rotation) = reader.number("rotation")? {
                    *angle = rotation;
                }
                if let Some((sx, sy)) = scale(index, &reader)? {
                    (*ratio_x, *ratio_y) = (sx, sy);
                }
                if let Some(layer_name) = reader.name("layer")? {
                    *layer = self.layer_code(layer_name);
                }
            }
            other => {
                // 形を変えられない要素（寸法・引出し線・ハッチング・スプライン・クロソイドなど）は、共通の値だけ
                reader.allow(&COMMON, "この要素")?;
                apply_style(self, index, &reader, other)?;
            }
        }
        self.members_mut(&place.figure, |members| members[place.index] = feature);
        Ok(())
    }

    fn delete(&mut self, index: usize, id: &str) -> Result<(), EditError> {
        let place = self.resolve(index, id)?;
        self.members_mut(&place.figure, |members| {
            members.remove(place.index);
        });
        Ok(())
    }

    fn add_layer(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        if self.drawing.document.layers.iter().any(|layer| layer.name == name) {
            return Err(EditError::at(index, "already-exists", format!("レイヤ {name} はすでにあります")));
        }
        let reader = ValueReader::new(values, index);
        reader.allow(&["visible", "color", "lineType", "lineWidth"], "レイヤ")?;
        layer_style(self, &reader)?;
        let visible = reader.bool("visible")?.unwrap_or(true);
        let id = self.temporary();
        self.drawing.document.layers.push(LayerDefinition { id, name: name.to_string(), visible });
        self.created.push(Created::Layer(name.to_string()));
        Ok(())
    }

    fn update_layer(&mut self, index: usize, name: &str, set: &EditValues) -> Result<(), EditError> {
        let Some(position) = self.drawing.document.layers.iter().position(|layer| layer.name == name) else {
            return Err(EditError::at(index, "not-found", format!("レイヤ {name} は図面にありません")));
        };
        let reader = ValueReader::new(set, index);
        reader.allow(&["visible", "color", "lineType", "lineWidth"], "レイヤ")?;
        layer_style(self, &reader)?;
        if let Some(visible) = reader.bool("visible")? {
            self.drawing.document.layers[position].visible = visible;
        }
        Ok(())
    }
}

/// SXF のレイヤは色・線種・線幅を持たない。指定があれば確かめてから、使わずに知らせる
fn layer_style(editor: &mut Editor, reader: &ValueReader<'_>) -> Result<(), EditError> {
    let color = reader.color("color")?;
    let line_type = reader.name("lineType")?;
    let width = reader.line_width("lineWidth")?;
    if color.is_some() || line_type.is_some() || width.is_some() {
        editor.notices.add(
            NoticeLevel::Info,
            "edit-layer-attributes",
            "SXF のレイヤは色・線種・線幅を持たないため、レイヤの色・線種・線幅の指定は使っていません",
        );
    }
    Ok(())
}

/// 形を変えられない要素に、共通の値（レイヤ・色・線種・線幅）を当てる
fn apply_style(
    editor: &mut Editor,
    index: usize,
    reader: &ValueReader<'_>,
    feature: &mut SxfFeature,
) -> Result<(), EditError> {
    let styled = match feature {
        SxfFeature::Spline { style, .. }
        | SxfFeature::Clothoid { style, .. }
        | SxfFeature::LinearDim { style, .. }
        | SxfFeature::RadiusDim { style, .. }
        | SxfFeature::DiameterDim { style, .. }
        | SxfFeature::Label { style, .. }
        | SxfFeature::Balloon { style, .. } => Some(style),
        SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => Some(&mut dimension.style),
        _ => None,
    };
    if let Some(style) = styled {
        return editor.apply_common(index, reader, style);
    }
    // レイヤ（と色）だけを持つ要素
    let (layer, color) = match feature {
        SxfFeature::ExternallyDefinedSymbol { layer, .. }
        | SxfFeature::ExternallyDefinedHatch { layer, .. }
        | SxfFeature::FillAreaStyleHatching { layer, .. } => (layer, None),
        SxfFeature::FillAreaStyleColour { layer, color, .. } | SxfFeature::FillAreaStyleTiles { layer, color, .. } => {
            (layer, Some(color))
        }
        _ => return Err(EditError::at(index, "unsupported-kind", "この要素は変えられません")),
    };
    if reader.has("lineType") || reader.has("lineWidth") || (color.is_none() && reader.has("color")) {
        return Err(EditError::at(index, "unknown-key", "この要素に指定できるのは layer（と塗りの color）です"));
    }
    let mut style = Styled {
        id: 0.0,
        layer: *layer,
        color: color.as_deref().copied().unwrap_or(1.0),
        line_type: 1.0,
        line_width: 1.0,
    };
    editor.apply_common(index, reader, &mut style)?;
    *layer = style.layer;
    if let Some(color) = color {
        *color = style.color;
    }
    Ok(())
}

/// 折線の弧の膨らみと幅は SXF にない（0 だけを受け付ける）
fn no_bulges_or_width(index: usize, reader: &ValueReader<'_>, count: usize) -> Result<(), EditError> {
    if let Some(bulges) = reader.numbers("bulges", count).ok().flatten()
        && bulges.iter().any(|bulge| *bulge != 0.0)
    {
        return Err(EditError::at(index, "unsupported-kind", "SXF の折線は弧の膨らみ（bulges）を持てません"));
    }
    if reader.has("bulges") && reader.numbers("bulges", count).is_err() {
        return Err(EditError::at(index, "invalid-value", "bulges は頂点と同じ数にしてください"));
    }
    if reader.number("width")?.is_some_and(|width| width != 0.0) {
        return Err(EditError::at(index, "unsupported-kind", "SXF の折線は幅（width）を持てません"));
    }
    Ok(())
}

/// 折線の頂点。閉じた折線は最初の点を最後にも置く（SXF の折線は閉じる印を持たない）
fn polyline_points(vertices: &[[f64; 3]], closed: bool) -> (Vec<f64>, Vec<f64>) {
    let mut xs: Vec<f64> = vertices.iter().map(|point| point[0]).collect();
    let mut ys: Vec<f64> = vertices.iter().map(|point| point[1]).collect();
    if closed && vertices.first() != vertices.last() {
        xs.push(vertices[0][0]);
        ys.push(vertices[0][1]);
    }
    (xs, ys)
}

/// 楕円の長軸（中心からのベクトル）と短軸の比から、X・Y の半径と傾き（度）
fn ellipse_axes(index: usize, reader: &ValueReader<'_>) -> Result<(f64, f64, f64), EditError> {
    let axis = reader.required("majorAxis", reader.point("majorAxis")?)?;
    let ratio = reader.required("ratio", reader.positive("ratio")?)?;
    axes(index, axis, ratio)
}

fn axes(index: usize, axis: [f64; 3], ratio: f64) -> Result<(f64, f64, f64), EditError> {
    let length = js::hypot2(axis[0], axis[1]);
    if ratio > 1.0 || length == 0.0 {
        return Err(EditError::at(
            index,
            "invalid-value",
            "ratio は 0 より大きく 1 以下、majorAxis は 0 でない長さにしてください",
        ));
    }
    Ok((length, length * ratio, js::atan2(axis[1], axis[0]) * 180.0 / PI))
}

/// 楕円の長軸・比のどちらかを変える
fn updated_axes(
    index: usize,
    reader: &ValueReader<'_>,
    rx: f64,
    ry: f64,
    rotation: f64,
) -> Result<(f64, f64, f64), EditError> {
    let theta = rotation * PI / 180.0;
    let axis = reader.point("majorAxis")?.unwrap_or([rx * js::cos(theta), rx * js::sin(theta), 0.0]);
    let ratio = reader.positive("ratio")?.unwrap_or(if rx == 0.0 { 1.0 } else { ry / rx });
    axes(index, axis, ratio)
}

/// 配置の尺度（数か [X, Y]）
fn scale(index: usize, reader: &ValueReader<'_>) -> Result<Option<(f64, f64)>, EditError> {
    if let Ok(Some(value)) = reader.number("scale") {
        return Ok(Some((value, value)));
    }
    match reader.numbers("scale", 2) {
        Ok(Some(values)) => Ok(Some((values[0], values[1]))),
        Ok(None) => Ok(None),
        Err(_) => match reader.numbers("scale", 3) {
            Ok(Some(values)) => Ok(Some((values[0], values[1]))),
            _ => Err(EditError::at(index, "invalid-value", "scale は数か [X, Y] で指定してください")),
        },
    }
}

/// 文字範囲の幅を指定しないとき、全角は高さと同じ、半角は高さの半分の幅で並べた長さ
fn natural_width(text: &str, height: f64) -> f64 {
    let units: f64 = text
        .chars()
        .map(|c| {
            let mut buffer = [0u8; 4];
            let (bytes, _, unmappable) = encoding_rs::SHIFT_JIS.encode(c.encode_utf8(&mut buffer));
            if unmappable || bytes.len() > 1 { 1.0 } else { 0.5 }
        })
        .sum();
    (units * height).max(height * 0.5)
}
