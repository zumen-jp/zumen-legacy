//! SXF の図面（`SxfDrawing`）を SFC ファイルに書く。フィーチャの引数の並びは読み取り（features.rs）と同じく
//! 附属書 SFC 編 1-2、フィーチャの囲み（/*SXF … SXF*/）とヘッダは 1-1 に従う。
//!
//! - 書く順は、既定義の色・線種の宣言、ユーザ定義の色・線種・線幅、文字フォント、レイヤ、複合曲線（構成要素の後に
//!   複合曲線定義）、複合図形（構成要素の後に複合図形定義。配置される複合図形を先に）、用紙の要素、図面表題欄、用紙。
//!   複合曲線定義・複合図形定義・用紙は、直前の定義以降の要素を構成要素にする（1-3）ので、この順なら構成要素が混ざらない。
//! - インスタンス番号は書く順に 10 から 10 ずつ振り直す（公開されている SFC と同じ振り方）。元の番号との対応を返す。
//! - 文字列は CP932。CP932 にない文字は ? にして知らせる。\ は \\ にし、改行と制御文字は書かない。
//! - SXF Ver.3.0 で加わった図面表題欄とクロソイドは /*SXF3 … SXF3*/ で囲む（公開されている SFC と同じ）。

use crate::document::{SxfDocument, SxfDrawing};
use crate::features::{
    Annotation, Arrow, CompositeCurveOrg, DrawingAttribute, DrawingSheet, ExtensionLine, LayerDefinition, Region,
    SfigOrg, Styled, SxfFeature, TextFont, UserColour, UserLineType, WidthDefinition,
};
use crate::tables::{PREDEFINED_COLORS, PREDEFINED_LINE_TYPES, PREDEFINED_WIDTHS};
use crate::version::{SxfVersion, downgrade};
use encoding_rs::SHIFT_JIS;
use std::collections::{HashMap, HashSet};
use zumen_core::{Notice, NoticeLevel, NoticeList};

/// 書いた SFC と、書き方を変えた内容。ids は（図面のインスタンス番号, 書いたインスタンス番号）を書いた順に並べたもの
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SxfWritten {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
    pub ids: Vec<(f64, f64)>,
}

/// フィーチャの引数
pub(crate) enum Param {
    /// '…' の数値
    Num(f64),
    /// \'…\' の文字列
    Text(String),
    /// '(…)' の数値の集合
    List(Vec<f64>),
    /// '…' の文字列（図面表題欄の作成年月日）
    Quoted(String),
}

use Param::{List, Num, Text};

struct Writer {
    out: Vec<u8>,
    notices: NoticeList,
    next: f64,
    ids: Vec<(f64, f64)>,
}

/// 数を SFC の数値にする。Rust の f64 の表示は指数を使わず、読み直すと同じ値になる最短の桁
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    format!("{value}")
}

/// 文字列を CP932 にする。CP932 にない文字は ? にして知らせる。\ は \\ にし、改行と制御文字は書かない
pub(crate) fn encode_text(value: &str, notices: &mut NoticeList) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    let mut replaced = false;
    for c in value.chars() {
        if c == '\r' || c == '\n' || c.is_control() {
            continue;
        }
        if c == '\\' {
            out.extend_from_slice(b"\\\\");
            continue;
        }
        let mut buffer = [0u8; 4];
        let (bytes, _, unmappable) = SHIFT_JIS.encode(c.encode_utf8(&mut buffer));
        if unmappable {
            out.push(b'?');
            replaced = true;
        } else {
            out.extend_from_slice(&bytes);
        }
    }
    if replaced {
        notices.add(NoticeLevel::Warning, "encoding", "CP932（Shift_JIS）にない文字を ? にしました");
    }
    out
}

/// フィーチャを 1 つ、囲み（/*SXF{version} … SXF{version}*/）ごと書く。最後の改行を含む
pub(crate) fn record(id: f64, name: &str, version: &str, params: Vec<Param>, notices: &mut NoticeList) -> Vec<u8> {
    let mut line = format!("/*SXF{version}\n#{} = {name}(", number(id)).into_bytes();
    let mut width = line.len() - 6 - version.len();
    for (index, param) in params.into_iter().enumerate() {
        let value = match param {
            Num(value) => format!("'{}'", number(value)).into_bytes(),
            List(values) => {
                format!("'({})'", values.iter().map(|value| number(*value)).collect::<Vec<_>>().join(",")).into_bytes()
            }
            Text(value) => {
                let mut bytes = b"\\'".to_vec();
                bytes.extend(encode_text(&value, notices));
                bytes.extend_from_slice(b"\\'");
                bytes
            }
            Param::Quoted(value) => {
                let mut bytes = b"'".to_vec();
                bytes.extend(encode_text(&value.replace('\'', ""), notices));
                bytes.push(b'\'');
                bytes
            }
        };
        if index > 0 {
            line.push(b',');
            width += 1;
            // 長い行は引数の区切りで折り返す（読み取りは引数の間の改行を空白とみなす）
            if width + value.len() > 80 {
                line.push(b'\n');
                width = 0;
            }
        }
        width += value.len();
        line.extend(value);
    }
    line.extend_from_slice(format!(")\nSXF{version}*/\n").as_bytes());
    line
}

impl Writer {
    /// フィーチャを 1 つ書き、書いたインスタンス番号を返す
    fn feature(&mut self, id: f64, name: &str, version: &str, params: Vec<Param>) -> f64 {
        let written = self.next;
        self.next += 10.0;
        self.ids.push((id, written));
        let bytes = record(written, name, version, params, &mut self.notices);
        self.out.extend(bytes);
        written
    }

    fn text(&mut self, value: &str) -> Vec<u8> {
        encode_text(value, &mut self.notices)
    }
}

fn styled(style: &Styled) -> Vec<Param> {
    vec![Num(style.layer), Num(style.color), Num(style.line_type), Num(style.line_width)]
}

fn extension(params: &mut Vec<Param>, ext: &Option<ExtensionLine>) {
    match ext {
        Some(ext) => {
            params.extend([Num(1.0), Num(ext.x0), Num(ext.y0), Num(ext.x1), Num(ext.y1), Num(ext.x2), Num(ext.y2)])
        }
        None => params.extend((0..7).map(|_| Num(0.0))),
    }
}

fn arrow(params: &mut Vec<Param>, arrow: &Option<Arrow>) {
    match arrow {
        Some(arrow) => {
            params.extend([Num(arrow.code), Num(arrow.direction), Num(arrow.x), Num(arrow.y), Num(arrow.scale)])
        }
        None => params.extend((0..5).map(|_| Num(0.0))),
    }
}

fn annotation_params(text: &Annotation) -> [Param; 11] {
    [
        Num(text.font),
        Text(text.text.clone()),
        Num(text.x),
        Num(text.y),
        Num(text.height),
        Num(text.width),
        Num(text.spacing),
        Num(text.angle),
        Num(text.slant),
        Num(text.anchor),
        Num(text.direction),
    ]
}

/// 寸法値・引出し線の文字（有無のフラグと 11 の引数）
fn annotation(params: &mut Vec<Param>, text: &Option<Annotation>) {
    match text {
        Some(text) => {
            params.push(Num(1.0));
            params.extend(annotation_params(text));
        }
        None => {
            params.push(Num(0.0));
            params.extend([
                Num(0.0),
                Text(String::new()),
                Num(0.0),
                Num(0.0),
                Num(0.0),
                Num(0.0),
                Num(0.0),
                Num(0.0),
                Num(0.0),
                Num(1.0),
                Num(1.0),
            ]);
        }
    }
}

fn region(params: &mut Vec<Param>, region: &Region) {
    params.extend([Num(region.out_id), Num(region.in_ids.len() as f64), List(region.in_ids.clone())]);
}

fn points(params: &mut Vec<Param>, xs: &[f64], ys: &[f64]) {
    params.extend([Num(xs.len() as f64), List(xs.to_vec()), List(ys.to_vec())]);
}

/// 定義のフィーチャ（表の項目、複合曲線定義・複合図形定義、図面表題欄、用紙）
#[derive(Clone, Copy)]
pub(crate) enum Definition<'a> {
    Layer(&'a LayerDefinition),
    Colour(&'a UserColour),
    LineType(&'a UserLineType),
    Width(&'a WidthDefinition),
    Font(&'a TextFont),
    Composite(&'a CompositeCurveOrg),
    SubFigure(&'a SfigOrg),
    Title(&'a DrawingAttribute),
    Sheet(&'a DrawingSheet),
}

/// 定義のフィーチャのフィーチャ名と引数（附属書 SFC 編 1-2）
pub(crate) fn definition(item: Definition<'_>) -> (&'static str, &'static str, Vec<Param>) {
    let flag = |value: bool| Num(if value { 1.0 } else { 0.0 });
    match item {
        Definition::Layer(layer) => ("layer_feature", "", vec![Text(layer.name.clone()), flag(layer.visible)]),
        Definition::Colour(color) => {
            ("user_defined_colour_feature", "", vec![Num(color.r), Num(color.g), Num(color.b)])
        }
        Definition::LineType(line_type) => (
            "user_defined_font_feature",
            "",
            vec![Text(line_type.name.clone()), Num(line_type.pitch.len() as f64), List(line_type.pitch.clone())],
        ),
        Definition::Width(width) => ("width_feature", "", vec![Num(width.width)]),
        Definition::Font(font) => ("text_font_feature", "", vec![Text(font.name.clone())]),
        Definition::Composite(feature) => (
            "composite_curve_org_feature",
            "",
            vec![Num(feature.color), Num(feature.line_type), Num(feature.line_width), flag(feature.visible)],
        ),
        Definition::SubFigure(figure) => ("sfig_org_feature", "", vec![Text(figure.name.clone()), Num(figure.flag)]),
        Definition::Title(title) => {
            // 作成年・月・日（8〜10 番目）は数値、ほかは文字列（附属書 SFC 編 1-2 の drawing_attribute_feature）
            let params = title
                .values
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    if (7..=9).contains(&index) {
                        Param::Quoted(if value.value.is_empty() { "0".into() } else { value.value.clone() })
                    } else {
                        Text(value.value.clone())
                    }
                })
                .collect();
            // 図面表題欄は Ver.3.0 で加わった
            ("drawing_attribute_feature", "3", params)
        }
        Definition::Sheet(sheet) => (
            "drawing_sheet_feature",
            "",
            vec![Text(sheet.name.clone()), Num(sheet.sheet_type), Num(sheet.orient), Num(sheet.x), Num(sheet.y)],
        ),
    }
}

/// 要素のフィーチャ名と引数（附属書 SFC 編 1-2）。定義のフィーチャ（レイヤなど）は None
pub(crate) fn drawable(feature: &SxfFeature) -> Option<(&'static str, &'static str, Vec<Param>)> {
    let bool_flag = |value: bool| Num(if value { 0.0 } else { 1.0 });
    Some(match feature {
        SxfFeature::PointMarker { layer, color, x, y, code, angle, scale, .. } => (
            "point_marker_feature",
            "",
            vec![Num(*layer), Num(*color), Num(*x), Num(*y), Num(*code), Num(*angle), Num(*scale)],
        ),
        SxfFeature::Line { style, x1, y1, x2, y2 } => {
            let mut params = styled(style);
            params.extend([Num(*x1), Num(*y1), Num(*x2), Num(*y2)]);
            ("line_feature", "", params)
        }
        SxfFeature::Polyline { style, xs, ys } => {
            let mut params = styled(style);
            points(&mut params, xs, ys);
            ("polyline_feature", "", params)
        }
        SxfFeature::Circle { style, cx, cy, r } => {
            let mut params = styled(style);
            params.extend([Num(*cx), Num(*cy), Num(*r)]);
            ("circle_feature", "", params)
        }
        SxfFeature::Arc { style, cx, cy, r, ccw, start, end } => {
            let mut params = styled(style);
            params.extend([Num(*cx), Num(*cy), Num(*r), bool_flag(*ccw), Num(*start), Num(*end)]);
            ("arc_feature", "", params)
        }
        SxfFeature::Ellipse { style, cx, cy, rx, ry, rotation } => {
            let mut params = styled(style);
            params.extend([Num(*cx), Num(*cy), Num(*rx), Num(*ry), Num(*rotation)]);
            ("ellipse_feature", "", params)
        }
        SxfFeature::EllipseArc { style, cx, cy, rx, ry, ccw, rotation, start, end } => {
            let mut params = styled(style);
            params.extend([
                Num(*cx),
                Num(*cy),
                Num(*rx),
                Num(*ry),
                bool_flag(*ccw),
                Num(*rotation),
                Num(*start),
                Num(*end),
            ]);
            ("ellipse_arc_feature", "", params)
        }
        SxfFeature::TextString { layer, color, text, .. } => {
            let mut params = vec![Num(*layer), Num(*color)];
            params.extend(annotation_params(text));
            ("text_string_feature", "", params)
        }
        SxfFeature::Spline { style, closed, xs, ys } => {
            let mut params = styled(style);
            params.push(bool_flag(*closed));
            points(&mut params, xs, ys);
            ("spline_feature", "", params)
        }
        SxfFeature::Clothoid { style, x, y, parameter, ccw, angle, start_length, end_length } => {
            let mut params = styled(style);
            params.extend([
                Num(*x),
                Num(*y),
                Num(*parameter),
                bool_flag(*ccw),
                Num(*angle),
                Num(*start_length),
                Num(*end_length),
            ]);
            // クロソイド曲線は Ver.3.1 で加わった
            ("clothoid_feature", "3.1", params)
        }
        SxfFeature::SfigLocate { layer, name, x, y, angle, ratio_x, ratio_y, .. } => (
            "sfig_locate_feature",
            "",
            vec![Num(*layer), Text(name.clone()), Num(*x), Num(*y), Num(*angle), Num(*ratio_x), Num(*ratio_y)],
        ),
        SxfFeature::ExternallyDefinedSymbol { layer, color, name, x, y, angle, scale, .. } => (
            "externally_defined_symbol_feature",
            "",
            vec![
                Num(*layer),
                Num(if color.is_some() { 1.0 } else { 0.0 }),
                Num(color.unwrap_or(0.0)),
                Text(name.clone()),
                Num(*x),
                Num(*y),
                Num(*angle),
                Num(*scale),
            ],
        ),
        SxfFeature::LinearDim { style, line, ext1, ext2, arrow1, arrow2, text } => {
            let mut params = styled(style);
            params.extend(line.iter().map(|value| Num(*value)));
            extension(&mut params, ext1);
            extension(&mut params, ext2);
            arrow(&mut params, arrow1);
            arrow(&mut params, arrow2);
            annotation(&mut params, text);
            ("linear_dim_feature", "", params)
        }
        SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => {
            let mut params = styled(&dimension.style);
            params.extend([
                Num(dimension.cx),
                Num(dimension.cy),
                Num(dimension.radius),
                Num(dimension.angle0),
                Num(dimension.angle1),
            ]);
            extension(&mut params, &dimension.ext1);
            extension(&mut params, &dimension.ext2);
            arrow(&mut params, &dimension.arrow1);
            arrow(&mut params, &dimension.arrow2);
            annotation(&mut params, &dimension.text);
            // 弧長寸法は Ver.3.1 で加わった
            match feature {
                SxfFeature::CurveDim(_) => ("curve_dim_feature", "3.1", params),
                _ => ("angular_dim_feature", "", params),
            }
        }
        SxfFeature::RadiusDim { style, line, arrow: tip, text } => {
            let mut params = styled(style);
            params.extend(line.iter().map(|value| Num(*value)));
            arrow(&mut params, tip);
            annotation(&mut params, text);
            ("radius_dim_feature", "", params)
        }
        SxfFeature::DiameterDim { style, line, arrow1, arrow2, text } => {
            let mut params = styled(style);
            params.extend(line.iter().map(|value| Num(*value)));
            arrow(&mut params, arrow1);
            arrow(&mut params, arrow2);
            annotation(&mut params, text);
            ("diameter_dim_feature", "", params)
        }
        SxfFeature::Label { style, xs, ys, arrow_code, arrow_scale, text } => {
            let mut params = styled(style);
            points(&mut params, xs, ys);
            params.extend([Num(*arrow_code), Num(*arrow_scale)]);
            annotation(&mut params, text);
            ("label_feature", "", params)
        }
        SxfFeature::Balloon { style, xs, ys, cx, cy, radius, arrow_code, arrow_scale, text } => {
            let mut params = styled(style);
            points(&mut params, xs, ys);
            params.extend([Num(*cx), Num(*cy), Num(*radius), Num(*arrow_code), Num(*arrow_scale)]);
            annotation(&mut params, text);
            ("balloon_feature", "", params)
        }
        SxfFeature::ExternallyDefinedHatch { layer, name, region: area, .. } => {
            let mut params = vec![Num(*layer), Text(name.clone())];
            region(&mut params, area);
            ("externally_defined_hatch_feature", "", params)
        }
        SxfFeature::FillAreaStyleColour { layer, color, region: area, .. } => {
            let mut params = vec![Num(*layer), Num(*color)];
            region(&mut params, area);
            ("fill_area_style_colour_feature", "", params)
        }
        SxfFeature::FillAreaStyleHatching { layer, patterns, region: area, .. } => {
            let mut params = vec![Num(*layer), Num(patterns.len() as f64)];
            for pattern in patterns {
                params.push(List(vec![
                    pattern.color,
                    pattern.line_type,
                    pattern.line_width,
                    pattern.x,
                    pattern.y,
                    pattern.spacing,
                    pattern.angle,
                ]));
            }
            region(&mut params, area);
            ("fill_area_style_hatching_feature", "", params)
        }
        SxfFeature::FillAreaStyleTiles {
            layer,
            name,
            color,
            x,
            y,
            vector1,
            vector1_angle,
            vector2,
            vector2_angle,
            scale_x,
            scale_y,
            angle,
            region: area,
            ..
        } => {
            let mut params = vec![
                Num(*layer),
                Text(name.clone()),
                Num(*color),
                Num(*x),
                Num(*y),
                Num(*vector1),
                Num(*vector1_angle),
                Num(*vector2),
                Num(*vector2_angle),
                Num(*scale_x),
                Num(*scale_y),
                Num(*angle),
            ];
            region(&mut params, area);
            ("fill_area_style_tiles_feature", "", params)
        }
        _ => return None,
    })
}

/// 要素が使う色・線種のコード
fn codes(feature: &SxfFeature, colors: &mut HashSet<u64>, line_types: &mut HashSet<u64>, widths: &mut HashSet<u64>) {
    let mut style = |style: &Styled| {
        colors.insert(style.color.to_bits());
        line_types.insert(style.line_type.to_bits());
        widths.insert(style.line_width.to_bits());
    };
    match feature {
        SxfFeature::Line { style: s, .. }
        | SxfFeature::Polyline { style: s, .. }
        | SxfFeature::Circle { style: s, .. }
        | SxfFeature::Arc { style: s, .. }
        | SxfFeature::Ellipse { style: s, .. }
        | SxfFeature::EllipseArc { style: s, .. }
        | SxfFeature::Spline { style: s, .. }
        | SxfFeature::Clothoid { style: s, .. }
        | SxfFeature::LinearDim { style: s, .. }
        | SxfFeature::RadiusDim { style: s, .. }
        | SxfFeature::DiameterDim { style: s, .. }
        | SxfFeature::Label { style: s, .. }
        | SxfFeature::Balloon { style: s, .. } => style(s),
        SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => style(&dimension.style),
        SxfFeature::PointMarker { color, .. }
        | SxfFeature::TextString { color, .. }
        | SxfFeature::FillAreaStyleColour { color, .. }
        | SxfFeature::FillAreaStyleTiles { color, .. } => {
            colors.insert(color.to_bits());
        }
        SxfFeature::ExternallyDefinedSymbol { color: Some(color), .. } => {
            colors.insert(color.to_bits());
        }
        SxfFeature::FillAreaStyleHatching { patterns, .. } => {
            for pattern in patterns {
                colors.insert(pattern.color.to_bits());
                line_types.insert(pattern.line_type.to_bits());
                widths.insert(pattern.line_width.to_bits());
            }
        }
        _ => {}
    }
}

/// 複合図形を、配置される複合図形が先になる順に並べる（循環する配置は、最初に見つけた所で切る）
pub(crate) fn sub_figure_order(doc: &SxfDocument) -> Vec<String> {
    fn visit(
        doc: &SxfDocument,
        name: &str,
        done: &mut HashSet<String>,
        active: &mut Vec<String>,
        out: &mut Vec<String>,
    ) {
        if done.contains(name) || active.iter().any(|candidate| candidate == name) {
            return;
        }
        let Some(figure) = doc.sub_figures.get(name) else {
            return;
        };
        active.push(name.to_string());
        for member in &figure.members {
            if let SxfFeature::SfigLocate { name: child, .. } = member {
                visit(doc, child, done, active, out);
            }
        }
        active.pop();
        done.insert(name.to_string());
        out.push(name.to_string());
    }
    let mut done = HashSet::new();
    let mut out = Vec::new();
    // 循環する配置の並びが定義の順で変わらないよう、名前の順に辿り始める
    let mut names: Vec<&str> = doc.sub_figures.values().map(|figure| figure.feature.name.as_str()).collect();
    names.sort_unstable();
    for name in names {
        visit(doc, name, &mut done, &mut Vec::new(), &mut out);
    }
    out
}

/// SXF の図面を SFC ファイルの版 version に新しく書く（番号を 10 から振り直す）。その版にないフィーチャは近い形に
/// する（version.rs）
pub(crate) fn write_sfc_new(drawing: &SxfDrawing, version: SxfVersion) -> SxfWritten {
    let mut writer = Writer { out: Vec::new(), notices: NoticeList::new(), next: 10.0, ids: Vec::new() };
    let document = downgrade(&drawing.document, version, &mut writer.notices);
    let doc = document.as_ref();
    let header = &drawing.header;
    let file_name = if header.file_name.is_empty() { "drawing.sfc" } else { header.file_name.as_str() };
    let quote = |writer: &mut Writer, value: &str| {
        let mut bytes = b"'".to_vec();
        bytes.extend(writer.text(&value.replace('\'', "''")));
        bytes.push(b'\'');
        bytes
    };
    let mut head =
        b"ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('SCADEC level2 feature_mode'),'2;1');\nFILE_NAME(".to_vec();
    let name = quote(&mut writer, file_name);
    head.extend(name);
    head.extend_from_slice(b",'',(''),(''),");
    // 出力ライブラリ名の末尾の $$3.1 などが SXF の版（読み取りの sxf_version）
    let preprocessor = quote(&mut writer, &format!("zumen$${}", version.as_str()));
    head.extend(preprocessor);
    head.push(b',');
    let system = if header.originating_system.is_empty() { "zumen" } else { header.originating_system.as_str() };
    let system = quote(&mut writer, system);
    head.extend(system);
    head.extend_from_slice(b",'');\nFILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));\nENDSEC;\nDATA;\n");
    writer.out.extend(head);

    // 既定義の色・線種のうち使うもの（コード 1〜16 は宣言の順によらない）
    let mut colors = HashSet::new();
    let mut line_types = HashSet::new();
    let mut widths = HashSet::new();
    let all_members = doc
        .sheet_members
        .iter()
        .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
        .chain(doc.composite_curves.iter().flat_map(|curve| curve.members.iter()));
    for feature in all_members {
        codes(feature, &mut colors, &mut line_types, &mut widths);
    }
    for curve in &doc.composite_curves {
        colors.insert(curve.feature.color.to_bits());
        line_types.insert(curve.feature.line_type.to_bits());
        widths.insert(curve.feature.line_width.to_bits());
    }
    for (index, color) in PREDEFINED_COLORS.iter().enumerate() {
        if colors.contains(&((index + 1) as f64).to_bits()) {
            writer.feature(0.0, "pre_defined_colour_feature", "", vec![Text(color.name.into())]);
        }
    }
    for (index, line_type) in PREDEFINED_LINE_TYPES.iter().enumerate() {
        if line_types.contains(&((index + 1) as f64).to_bits()) {
            writer.feature(0.0, "pre_defined_font_feature", "", vec![Text(line_type.name.into())]);
        }
    }
    let define = |writer: &mut Writer, id: f64, item: Definition<'_>| {
        let (name, version, params) = definition(item);
        writer.feature(id, name, version, params);
    };
    for color in &doc.user_colors {
        define(&mut writer, color.id, Definition::Colour(color));
    }
    for line_type in &doc.user_line_types {
        define(&mut writer, line_type.id, Definition::LineType(line_type));
    }
    // 附属書 SFC 編 1-3-17（線幅）: 既定義コードも width_feature で宣言する。
    // Jw_cad 10.3.6 は宣言のない線幅の線・ハッチング境界を読み落とす（docs/formats/sxf.md）。
    for (index, width) in PREDEFINED_WIDTHS.iter().enumerate() {
        if widths.contains(&((index + 1) as f64).to_bits()) {
            writer.feature(0.0, "width_feature", "", vec![Num(*width)]);
        }
    }
    for width in &doc.user_widths {
        define(&mut writer, width.id, Definition::Width(width));
    }
    for font in &doc.fonts {
        define(&mut writer, font.id, Definition::Font(font));
    }
    for layer in &doc.layers {
        define(&mut writer, layer.id, Definition::Layer(layer));
    }
    let mut skipped = 0;
    let mut members = |writer: &mut Writer, list: &[SxfFeature]| {
        for feature in list {
            match drawable(feature) {
                Some((name, version, params)) => {
                    writer.feature(feature.id(), name, version, params);
                }
                None => skipped += 1,
            }
        }
    };
    for curve in &doc.composite_curves {
        members(&mut writer, &curve.members);
        define(&mut writer, curve.feature.id, Definition::Composite(&curve.feature));
    }
    for name in sub_figure_order(doc) {
        let figure = doc.sub_figures.get(&name).expect("並べた複合図形");
        members(&mut writer, &figure.members);
        define(&mut writer, figure.feature.id, Definition::SubFigure(&figure.feature));
    }
    members(&mut writer, &doc.sheet_members);
    if skipped > 0 {
        writer.notices.add_count(
            NoticeLevel::Warning,
            "not-drawable",
            "要素の並びにある定義のフィーチャ（レイヤ・用紙など）は書いていません",
            skipped,
        );
    }
    if let Some(title) = &doc.title {
        define(&mut writer, title.id, Definition::Title(title));
    }
    if let Some(sheet) = &doc.sheet {
        define(&mut writer, sheet.id, Definition::Sheet(sheet));
    }
    writer.out.extend_from_slice(b"ENDSEC;\nEND-ISO-10303-21;\n");
    check_limits(doc, &mut writer.notices);
    SxfWritten { bytes: writer.out, notices: writer.notices.into_vec(), ids: writer.ids }
}

/// SXF の数の上限（レイヤ 256、色コード 256、線種コード 32、線幅コード 16、文字フォント 1024、文字列 256 バイト。
/// ezsxf の検証と同じ値）を超える図面を知らせる。値は変えずに書く（SXF のソフトが読めないことがある）。
/// 件数は上限を超えた項目の数（表の項目の多い分と、長すぎる文字列の数）
pub(crate) fn check_limits(doc: &SxfDocument, notices: &mut NoticeList) {
    let over = |count: usize, limit: usize| count.saturating_sub(limit) as u64;
    let mut found: Vec<(&str, u64)> = vec![
        ("レイヤ", over(doc.layers.len(), 256)),
        ("ユーザ定義色", over(doc.user_colors.len(), 240)),
        ("ユーザ定義線種", over(doc.user_line_types.len(), 16)),
        ("ユーザ定義線幅", over(doc.user_widths.len(), 6)),
        ("文字フォント", over(doc.fonts.len(), 1024)),
    ];
    // 文字列は CP932 で 256 バイトまで
    let long = |text: &str| SHIFT_JIS.encode(text).0.len() > 256;
    let mut strings = 0u64;
    let mut count_feature = |feature: &SxfFeature| {
        let texts: Vec<&str> = match feature {
            SxfFeature::TextString { text, .. } => vec![text.text.as_str()],
            SxfFeature::LinearDim { text, .. }
            | SxfFeature::RadiusDim { text, .. }
            | SxfFeature::DiameterDim { text, .. }
            | SxfFeature::Label { text, .. }
            | SxfFeature::Balloon { text, .. } => text.iter().map(|text| text.text.as_str()).collect(),
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => {
                dimension.text.iter().map(|text| text.text.as_str()).collect()
            }
            SxfFeature::SfigLocate { name, .. }
            | SxfFeature::ExternallyDefinedSymbol { name, .. }
            | SxfFeature::ExternallyDefinedHatch { name, .. }
            | SxfFeature::FillAreaStyleTiles { name, .. } => vec![name.as_str()],
            _ => Vec::new(),
        };
        strings += texts.into_iter().filter(|text| long(text)).count() as u64;
    };
    for feature in doc
        .sheet_members
        .iter()
        .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
        .chain(doc.composite_curves.iter().flat_map(|curve| curve.members.iter()))
    {
        count_feature(feature);
    }
    let names = doc
        .layers
        .iter()
        .map(|layer| layer.name.as_str())
        .chain(doc.fonts.iter().map(|font| font.name.as_str()))
        .chain(doc.user_line_types.iter().map(|line_type| line_type.name.as_str()))
        .chain(doc.sub_figures.values().map(|figure| figure.feature.name.as_str()))
        .chain(doc.sheet.iter().map(|sheet| sheet.name.as_str()));
    strings += names.filter(|name| long(name)).count() as u64;
    found.push(("CP932 で 256 バイトを超える文字列", strings));
    found.retain(|(_, count)| *count > 0);
    if found.is_empty() {
        return;
    }
    let total = found.iter().map(|(_, count)| count).sum();
    let detail: Vec<String> = found.iter().map(|(what, count)| format!("{what} {count}")).collect();
    notices.add_count(
        NoticeLevel::Warning,
        "sxf-limit",
        format!(
            "SXF の上限（レイヤ 256・色 256・線種 32・線幅 16・文字フォント 1024・文字列 256 バイト）を超えています（{}）。\
             SXF のソフトが読めないことがあります",
            detail.join("、")
        ),
        total,
    );
}

/// 書いたインスタンス番号の対応（図面のインスタンス番号 → 書いた番号）。同じ番号の要素が複数あれば最初のもの
pub fn written_ids(written: &SxfWritten) -> HashMap<u64, f64> {
    let mut out = HashMap::new();
    for (id, new) in &written.ids {
        out.entry(id.to_bits()).or_insert(*new);
    }
    out
}
