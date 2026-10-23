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
//! - ブロック定義は作図部品（複合図形定義の種別 4）で、container の block:<名前> で中に要素を足せる。寸法は寸法の
//!   フィーチャ、ハッチングは見えない複合曲線の上の塗り・ハッチング（ユーザ定義）にする（docs/decisions/0025）。
//! - 寸法スタイル・文字スタイルは編集の中だけの名前で、後の操作が名前で使う。寸法スタイルの大きさと書き方は寸法の
//!   フィーチャごとに書き、文字スタイルは文字フォントを足す。線種はユーザ定義線種にする（docs/decisions/0026）。

mod dimension;
mod hatch;

use crate::document::{CompositeCurve, SubFigure, SxfDrawing};
use crate::features::{
    Annotation, CompositeCurveOrg, DrawingSheet, HatchPattern, LayerDefinition, Region, SfigOrg, Styled, SxfFeature,
    TextFont, UserColour, UserLineType, WidthDefinition,
};
use crate::tables::{PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, predefined_width_code};
use crate::{SxfHeader, parse_sxf, write_sxf};
use std::collections::HashMap;
use std::f64::consts::PI;
use zumen_core::edit::dimension::{DimensionGeometry, DimensionInput};
use zumen_core::edit::hatch::PatternLine;
use zumen_core::edit::style::{DimensionStyleInput, LineTypeInput, TextStyleInput, line_type_pitches};
use zumen_core::edit::{
    BLOCK_REFERENCE, EditColor, EditError, EditLineWidth, EditOperation, EditValues, Edited, StyleKind, ValueReader,
};
use zumen_core::{NoticeLevel, NoticeList, js};

/// 足せる要素の種類
const ADDABLE: [&str; 11] = [
    "line",
    "polyline",
    "arc",
    "circle",
    "ellipse",
    "ellipse-arc",
    "point",
    "text",
    "dimension",
    "hatch",
    BLOCK_REFERENCE,
];

/// 入れ子の複合図形の配置を辿る深さの上限（読み取りの複合図形の配置と同じ）
const MAX_DEPTH: usize = 32;

/// ユーザ定義線種の数の上限（線種コード 17〜32）
const MAX_USER_LINE_TYPES: usize = 16;

/// ユーザ定義線種の線と空白の長さの数の上限（2・4・6・8 個。ezsxf の検証と同じ）
const MAX_LINE_TYPE_SEGMENTS: usize = 8;

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
    /// 足したもの（要素の仮の番号、またはレイヤ・作図部品の名前）を操作の順に
    created: Vec<Created>,
    /// この編集で足した作図部品の基点（名前から、部品の座標）
    bases: HashMap<String, [f64; 2]>,
    /// この編集で足した寸法スタイル（大文字の名前から。SXF には書かない。docs/decisions/0026）
    dimension_styles: HashMap<String, dimension::Style>,
    /// この編集で足した文字スタイルの文字フォントのコード（大文字の名前から）
    text_styles: HashMap<String, f64>,
    /// この編集で足した、空白のない（実線の）線種の線種コード（大文字の名前から。SXF には書かない）
    line_type_aliases: HashMap<String, f64>,
}

enum Created {
    Feature(f64),
    Name(String),
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
    let mut editor = Editor {
        drawing,
        notices: NoticeList::new(),
        next: -1.0,
        created: Vec::new(),
        bases: HashMap::new(),
        dimension_styles: HashMap::new(),
        text_styles: HashMap::new(),
        line_type_aliases: HashMap::new(),
    };
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            EditOperation::Add { kind, container, values } => editor.add(index, kind, container.as_deref(), values)?,
            EditOperation::Update { id, set } => editor.update(index, id, set)?,
            EditOperation::Delete { id } => editor.delete(index, id)?,
            EditOperation::AddLayer { name, values } => editor.add_layer(index, name, values)?,
            EditOperation::UpdateLayer { name, set } => editor.update_layer(index, name, set)?,
            EditOperation::AddBlock { name, values } => editor.add_block(index, name, values)?,
            EditOperation::AddLayout { .. } | EditOperation::UpdateLayout { .. } => {
                return Err(EditError::at(index, "unsupported-kind", "SXF の編集ではレイアウトを扱えません"));
            }
            EditOperation::AddStyle { kind, name, values } => editor.add_style(index, *kind, name, values)?,
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
            Created::Name(name) => name.clone(),
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

    /// 足す先。sheet（省略したとき）は用紙、figure:<名前> と block:<名前> はその複合図形
    fn container(&self, index: usize, container: Option<&str>) -> Result<Option<String>, EditError> {
        match container {
            None | Some("sheet") => Ok(None),
            Some(container) => match container.strip_prefix("figure:").or_else(|| container.strip_prefix("block:")) {
                Some(name) if self.drawing.document.sub_figures.contains(name) => Ok(Some(name.to_string())),
                Some(name) => {
                    Err(EditError::at(index, "missing-definition", format!("複合図形 {name} は図面にありません")))
                }
                None => Err(EditError::at(
                    index,
                    "invalid-value",
                    format!(
                        "container の {container} は使えません（sheet、figure:<複合図形の名前>、block:<作図部品の名前>）"
                    ),
                )),
            },
        }
    }

    /// 入れ物の座標の 1 が用紙の何 mm か。用紙は 1、複合図形は用紙から辿った最初の配置の尺度（入れ子は掛ける）。
    /// 配置のない複合図形は 1、作図グループは配置の値を使わない（フィーチャ仕様編 3-5 補足）
    fn figure_scale(&self, figure: &Option<String>) -> f64 {
        fn search(
            doc: &crate::SxfDocument,
            members: &[SxfFeature],
            target: &str,
            scale: f64,
            depth: usize,
        ) -> Option<f64> {
            if depth > MAX_DEPTH {
                return None;
            }
            for member in members {
                let SxfFeature::SfigLocate { name, ratio_x, ratio_y, .. } = member else { continue };
                let Some(figure) = doc.sub_figures.get(name) else { continue };
                let ratio = if figure.feature.flag == 3.0 { 1.0 } else { (ratio_x.abs() * ratio_y.abs()).sqrt() };
                if !(ratio.is_finite() && ratio > 0.0) {
                    continue;
                }
                if name == target {
                    return Some(scale * ratio);
                }
                if let Some(found) = search(doc, &figure.members, target, scale * ratio, depth + 1) {
                    return Some(found);
                }
            }
            None
        }
        let Some(name) = figure else { return 1.0 };
        let doc = &self.drawing.document;
        search(doc, &doc.sheet_members, name, 1.0, 0).unwrap_or(1.0)
    }

    /// 複合図形 figure の中（入れ子も）に、複合図形 target の配置があるか
    fn places(&self, figure: &str, target: &str, depth: usize) -> bool {
        if depth > MAX_DEPTH {
            return true;
        }
        let Some(figure) = self.drawing.document.sub_figures.get(figure) else { return false };
        figure.members.iter().any(|member| match member {
            SxfFeature::SfigLocate { name, .. } => name == target || self.places(name, target, depth + 1),
            _ => false,
        })
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
        // この編集で足した実線の線種
        if let Some(code) = self.line_type_aliases.get(&js::trim(name).to_uppercase()) {
            return Ok(*code);
        }
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
        // この編集で足した作図部品の中は、部品の基点を原点にする（SXF の複合図形定義は基点を持たない）
        let origin = figure.as_ref().and_then(|name| self.bases.get(name)).copied().unwrap_or([0.0; 2]);
        let shift = |point: [f64; 3]| [point[0] - origin[0], point[1] - origin[1], point[2]];
        let at = |key: &str| -> Result<[f64; 3], EditError> { Ok(shift(reader.required(key, reader.point(key)?)?)) };
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
                let start = at("start")?;
                let end = at("end")?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Line { style, x1: start[0], y1: start[1], x2: end[0], y2: end[1] }
            }
            "polyline" => {
                reader.allow(&keys(&["vertices", "closed", "bulges", "width"]), "折線")?;
                let vertices: Vec<[f64; 3]> =
                    reader.required("vertices", reader.points("vertices", 2)?)?.into_iter().map(shift).collect();
                no_bulges_or_width(index, &reader, vertices.len())?;
                let closed = reader.bool("closed")?.unwrap_or(false);
                self.apply_common(index, &reader, &mut style)?;
                let (xs, ys) = polyline_points(&vertices, closed);
                SxfFeature::Polyline { style, xs, ys }
            }
            "circle" => {
                reader.allow(&keys(&["center", "radius"]), "円")?;
                let center = at("center")?;
                let r = reader.required("radius", reader.positive("radius")?)?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Circle { style, cx: center[0], cy: center[1], r }
            }
            "arc" => {
                reader.allow(&keys(&["center", "radius", "angle"]), "円弧")?;
                let center = at("center")?;
                let r = reader.required("radius", reader.positive("radius")?)?;
                let angle = reader.required("angle", reader.numbers("angle", 2)?)?;
                self.apply_common(index, &reader, &mut style)?;
                SxfFeature::Arc { style, cx: center[0], cy: center[1], r, ccw: true, start: angle[0], end: angle[1] }
            }
            "ellipse" | "ellipse-arc" => {
                reader.allow(&keys(&["center", "majorAxis", "ratio", "parameter"]), "楕円")?;
                let center = at("center")?;
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
                let position = at("position")?;
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
                let position = at("position")?;
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
                    Some(name) => self.text_font(name),
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
            "dimension" => {
                reader.allow(&keys(&zumen_core::edit::dimension::KEYS), "寸法")?;
                let input = DimensionInput::read(&reader, index)?;
                let text = reader.text("text")?.unwrap_or("<>");
                if text.contains(['\n', '\r']) {
                    return Err(EditError::at(index, "invalid-value", "SXF の寸法値は 1 行です"));
                }
                let look = match reader.name("dimensionStyle")? {
                    Some(name) => self.dimension_style(name),
                    None => dimension::Style::default(),
                };
                let geometry = input.geometry().map_err(|message| EditError::at(index, "invalid-value", message))?;
                let geometry = shifted(geometry, origin);
                self.apply_common(index, &reader, &mut style)?;
                let container = dimension::Container { scale: self.figure_scale(&figure) };
                dimension::feature(&geometry, text, style, &look, container, natural_width)
            }
            "hatch" => {
                reader.allow(&keys(&zumen_core::edit::hatch::KEYS), "ハッチング")?;
                self.hatch(index, &reader, &figure, origin, style)?
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
                // 複合図形の中に、その複合図形を含む複合図形は置けない（配置が循環する）
                if let Some(container) = &figure
                    && (name == container || self.places(name, container, 0))
                {
                    return Err(EditError::at(
                        index,
                        "invalid-value",
                        format!("複合図形 {name} は {container} を含むため、その中に置けません"),
                    ));
                }
                let position = at("position")?;
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

    /// ハッチング。境界ごとに見えない複合曲線を作り、塗り（fill_area_style_colour）か、模様の線ごとのハッチング線を
    /// 持つハッチング（ユーザ定義、fill_area_style_hatching）をその上に置く（docs/decisions/0025）
    fn hatch(
        &mut self,
        index: usize,
        reader: &ValueReader<'_>,
        figure: &Option<String>,
        origin: [f64; 2],
        mut style: Styled,
    ) -> Result<SxfFeature, EditError> {
        let input = zumen_core::edit::hatch::read(reader, index)?;
        self.apply_common(index, reader, &mut style)?;
        let mut numbers = Vec::with_capacity(input.boundaries.len());
        for boundary in &input.boundaries {
            // 閉じた折線は最初の点を最後にも置く（SXF の折線は閉じる印を持たない）
            let mut xs: Vec<f64> = boundary.iter().map(|point| point[0] - origin[0]).collect();
            let mut ys: Vec<f64> = boundary.iter().map(|point| point[1] - origin[1]).collect();
            xs.push(xs[0]);
            ys.push(ys[0]);
            let outline =
                Styled { id: self.temporary(), layer: style.layer, color: 1.0, line_type: 1.0, line_width: 1.0 };
            let id = self.temporary();
            self.drawing.document.composite_curves.push(CompositeCurve {
                feature: CompositeCurveOrg { id, color: 1.0, line_type: 1.0, line_width: 1.0, visible: false },
                members: vec![SxfFeature::Polyline { style: outline, xs, ys }],
            });
            numbers.push(self.drawing.document.composite_curves.len() as f64);
        }
        let region = Region { out_id: numbers[0], in_ids: numbers[1..].to_vec() };
        if input.solid {
            if reader.has("lineType") || reader.has("lineWidth") {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-unused-attributes",
                    "SXF の塗りは線種・線幅を持たないため、塗りのハッチングの線種・線幅の指定を使っていません",
                );
            }
            return Ok(SxfFeature::FillAreaStyleColour {
                id: style.id,
                layer: style.layer,
                color: style.color,
                region,
            });
        }
        let scale = self.figure_scale(figure);
        let near = [input.boundaries[0][0][0] - origin[0], input.boundaries[0][0][1] - origin[1]];
        let mut patterns = Vec::with_capacity(input.lines.len());
        for line in &input.lines {
            let line = PatternLine { base: [line.base[0] - origin[0], line.base[1] - origin[1]], ..line.clone() };
            let hatch = hatch::hatch_line(&line, scale, near).ok_or_else(|| {
                EditError::at(index, "invalid-value", "模様の線の間隔（dx・dy の、線に直交する成分）が 0 です")
            })?;
            if hatch.shifted {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-hatch-shift",
                    "SXF のハッチング線は線ごとの破線のずれを持たないため、模様の破線を線ごとにずらさずに書きました",
                );
            }
            if hatch.cut {
                self.notices.add(
                    NoticeLevel::Info,
                    "edit-hatch-dash-count",
                    "線と空白が 8 を超える模様の破線は、SXF のユーザ定義線種（2・4・6・8 個）に書けないため、初めの 8 だけにしました",
                );
            }
            let line_type =
                if hatch.pitch.is_empty() { style.line_type } else { self.line_type_for(&input.pattern, hatch.pitch) };
            patterns.push(HatchPattern {
                color: style.color,
                line_type,
                line_width: style.line_width,
                x: hatch.x,
                y: hatch.y,
                spacing: hatch.spacing,
                angle: hatch.angle,
            });
        }
        Ok(SxfFeature::FillAreaStyleHatching { id: style.id, layer: style.layer, patterns, region })
    }

    /// 破線のピッチ（用紙の mm。既定義線種でなければ 8 個まで）の線種コード。同じピッチの既定義線種・ユーザ定義線種、
    /// なければユーザ定義線種を足す。ユーザ定義線種の数が上限（16）なら実線にして知らせる
    fn line_type_for(&mut self, name: &str, pitch: Vec<f64>) -> f64 {
        if let Some(code) = predefined_line_type(&pitch) {
            return code;
        }
        let doc = &self.drawing.document;
        if let Some(position) = doc.user_line_types.iter().position(|line_type| same_pitch(&line_type.pitch, &pitch)) {
            return (position + 17) as f64;
        }
        if doc.user_line_types.len() >= MAX_USER_LINE_TYPES {
            self.notices.add(
                NoticeLevel::Warning,
                "edit-hatch-dashes",
                "ユーザ定義線種が上限（16）に達しているため、模様の破線を実線にしました",
            );
            return DEFAULT_LINE_TYPE;
        }
        let id = self.temporary();
        self.drawing.document.user_line_types.push(UserLineType { id, name: name.to_string(), pitch });
        (self.drawing.document.user_line_types.len() + 16) as f64
    }

    /// 寸法スタイル・文字スタイル・線種を足す（docs/decisions/0026）。名前は大文字・小文字を区別せずに比べる
    fn add_style(&mut self, index: usize, kind: StyleKind, name: &str, values: &EditValues) -> Result<(), EditError> {
        if name.trim().is_empty() || name.chars().any(char::is_control) {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("名前 {name} は使えません（空、制御文字を含む名前は使えません）"),
            ));
        }
        let reader = ValueReader::new(values, index);
        match kind {
            StyleKind::Dimension => self.add_dimension_style(index, name, &reader)?,
            StyleKind::Text => self.add_text_style(index, name, &reader)?,
            StyleKind::LineType => self.add_line_type(index, name, &reader)?,
        }
        self.created.push(Created::Name(name.to_string()));
        Ok(())
    }

    /// 寸法スタイル。SXF は寸法スタイルを持たないので、編集の中だけに持ち、dimensionStyle で名前を指す寸法の
    /// フィーチャにその大きさと書き方を書く。from（この編集で足した寸法スタイル）か ISO-25 の値から始める
    fn add_dimension_style(&mut self, index: usize, name: &str, reader: &ValueReader<'_>) -> Result<(), EditError> {
        let input = DimensionStyleInput::read(reader, index)?;
        let key = js::trim(name).to_uppercase();
        if self.dimension_styles.contains_key(&key) {
            return Err(EditError::at(index, "already-exists", format!("寸法スタイル {name} はすでにあります")));
        }
        if input.text_height == Some(0.0) {
            return Err(EditError::at(
                index,
                "invalid-value",
                "textHeight は 0 より大きい数にしてください（SXF は高さ 0 の寸法値を書けません）",
            ));
        }
        let mut style = match input.from.as_deref() {
            Some(from) => self.dimension_style(from),
            None => dimension::Style::default(),
        };
        let font = input.text_style.as_deref().map(|style| self.text_font(style));
        style.apply(&input, font);
        if input.text_above == Some(false) {
            self.notices.add(
                NoticeLevel::Info,
                "edit-dimension-text-position",
                "寸法のフィーチャの寸法線は 1 本で寸法値のところで切れないため、寸法線の上に置かない寸法値は寸法線に重ねて中央に置きました",
            );
        }
        self.dimension_styles.insert(key, style);
        Ok(())
    }

    /// 名前の寸法スタイル（この編集で足したもの）。ほかの名前は既定の大きさにして知らせる
    fn dimension_style(&mut self, name: &str) -> dimension::Style {
        if let Some(style) = self.dimension_styles.get(&js::trim(name).to_uppercase()) {
            return style.clone();
        }
        self.notices.add(
            NoticeLevel::Info,
            "edit-dimension-style",
            "SXF は寸法スタイルを持たないため、この編集で足していない寸法スタイルは既定の大きさ（ISO-25）にしました",
        );
        dimension::Style::default()
    }

    /// 文字スタイル。SXF の文字フォントは名前だけを持つので、font（なければスタイルの名前）の文字フォントを足し、
    /// 後の操作の textStyle はそのフォントになる。フォントのほかの値は持てないので知らせる
    fn add_text_style(&mut self, index: usize, name: &str, reader: &ValueReader<'_>) -> Result<(), EditError> {
        let input = TextStyleInput::read(reader, index)?;
        let key = js::trim(name).to_uppercase();
        if self.text_styles.contains_key(&key)
            || self.drawing.document.fonts.iter().any(|font| font.name.to_uppercase() == key)
        {
            return Err(EditError::at(
                index,
                "already-exists",
                format!("文字スタイル・文字フォント {name} はすでにあります"),
            ));
        }
        if input.has_more_than_font() {
            self.notices.add(
                NoticeLevel::Info,
                "edit-style-values",
                "SXF の文字フォントは名前だけ、線種は名前と線・空白の長さだけを持つため、ほかの値（bigFont・height・widthFactor・oblique・description）を書きませんでした",
            );
        }
        let code = self.font_code(input.font.as_deref().unwrap_or(name));
        self.text_styles.insert(key, code);
        Ok(())
    }

    /// 文字スタイルの名前の文字フォントのコード。この編集で足した文字スタイルでなければ、その名前の文字フォント
    /// （図面になければ足す）
    fn text_font(&mut self, name: &str) -> f64 {
        match self.text_styles.get(&js::trim(name).to_uppercase()) {
            Some(code) => *code,
            None => self.font_code(name),
        }
    }

    /// 線種をユーザ定義線種（線種コード 17〜32）として足す。長さは用紙の mm で、線から始まる線と空白の長さ（点は
    /// 0.25 mm の線）にし、8 を超える分は切る。空白のない模様は実線で、名前は編集の中で実線（1）を指す
    fn add_line_type(&mut self, index: usize, name: &str, reader: &ValueReader<'_>) -> Result<(), EditError> {
        let input = LineTypeInput::read(reader, index)?;
        let key = js::trim(name).to_uppercase();
        let doc = &self.drawing.document;
        let exists = PREDEFINED_LINE_TYPES.iter().any(|line_type| line_type.name.to_uppercase() == key)
            || doc.user_line_types.iter().any(|line_type| {
                line_type.name.to_uppercase() == key
                    || line_type.name.trim_start_matches("$$SXF_").to_uppercase() == key
            })
            || self.line_type_aliases.contains_key(&key);
        if exists {
            return Err(EditError::at(index, "already-exists", format!("線種 {name} はすでにあります")));
        }
        let (pitch, changed) = line_type_pitches(&input.pattern, MAX_LINE_TYPE_SEGMENTS);
        if pitch.is_empty() {
            self.line_type_aliases.insert(key, DEFAULT_LINE_TYPE);
        } else {
            if doc.user_line_types.len() >= MAX_USER_LINE_TYPES {
                return Err(EditError::at(
                    index,
                    "name-exhausted",
                    format!("ユーザ定義線種が上限（{MAX_USER_LINE_TYPES}）に達しているため、線種を足せません"),
                ));
            }
            let id = self.temporary();
            self.drawing.document.user_line_types.push(UserLineType { id, name: name.to_string(), pitch });
        }
        if changed {
            self.notices.add(
                NoticeLevel::Info,
                "edit-line-type-pattern",
                "SXF のユーザ定義線種は線から始まる線と空白の長さ（2・4・6・8 個）なので、線種の模様をいちばん近い長さにしました（点は 0.25 mm の線）",
            );
        }
        if !input.description.is_empty() {
            self.notices.add(
                NoticeLevel::Info,
                "edit-style-values",
                "SXF の文字フォントは名前だけ、線種は名前と線・空白の長さだけを持つため、ほかの値（bigFont・height・widthFactor・oblique・description）を書きませんでした",
            );
        }
        Ok(())
    }

    /// 空のブロック定義を作図部品（複合図形定義の種別 4）として足す（docs/decisions/0017）。複合図形定義は基点を
    /// 持たない（部品の座標の原点を配置の点に置く）ので、basePoint は後の操作で中に足す要素の座標から引く
    fn add_block(&mut self, index: usize, name: &str, values: &EditValues) -> Result<(), EditError> {
        let reader = ValueReader::new(values, index);
        reader.allow(&["basePoint"], "作図部品")?;
        if name.trim().is_empty() || name.chars().any(char::is_control) || name.starts_with("$$") {
            return Err(EditError::at(
                index,
                "invalid-value",
                format!("作図部品の名前 {name} は使えません（空、制御文字、$$ で始まる名前は使えません）"),
            ));
        }
        if self.drawing.document.sub_figures.contains(name) {
            return Err(EditError::at(index, "already-exists", format!("複合図形 {name} はすでにあります")));
        }
        let base = reader.point("basePoint")?.unwrap_or([0.0; 3]);
        let id = self.temporary();
        let feature = SfigOrg { id, name: name.to_string(), flag: 4.0 };
        self.drawing.document.sub_figures.set(name.to_string(), SubFigure { feature, members: Vec::new() });
        self.bases.insert(name.to_string(), [base[0], base[1]]);
        self.created.push(Created::Name(name.to_string()));
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
                    text.font = self.text_font(name);
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
        self.created.push(Created::Name(name.to_string()));
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

/// 線と空白の長さ（用紙の mm）が同じ既定義線種のコード（既定義線種は 10 個の長さの三点二短鎖線まで）
fn predefined_line_type(pitch: &[f64]) -> Option<f64> {
    PREDEFINED_LINE_TYPES
        .iter()
        .position(|line_type| same_pitch(line_type.pitch, pitch))
        .map(|position| (position + 1) as f64)
}

fn same_pitch(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
}

/// 寸法の形を、作図部品の基点を原点にした座標にする
fn shifted(geometry: DimensionGeometry, origin: [f64; 2]) -> DimensionGeometry {
    let at = |point: [f64; 2]| [point[0] - origin[0], point[1] - origin[1]];
    match geometry {
        DimensionGeometry::Linear { origins, feet, direction, measurement } => {
            DimensionGeometry::Linear { origins: origins.map(at), feet: feet.map(at), direction, measurement }
        }
        DimensionGeometry::Angular { center, radius, from, span, origins } => {
            DimensionGeometry::Angular { center: at(center), radius, from, span, origins: origins.map(at) }
        }
        DimensionGeometry::Radial { center, from, to, measurement, diameter } => {
            DimensionGeometry::Radial { center: at(center), from: at(from), to: at(to), measurement, diameter }
        }
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
