//! 足す図形の値（src/codec の構造体）と、図形の値の変え方。値の決め方は DXF の編集と同じで、DWG の形は
//! AutoCAD が書く図形（モデルからの書き出し write/drawing と同じ）に合わせる

use crate::bits::HandleRef;
use crate::codec::RawText;
use crate::codec::entities::{
    Arc, Body, Circle, Ellipse, Hatch, HatchBoundary, HatchPath, HatchPattern, Insert, Line, LwPolyline, MText,
    MTextRedundant, PatternLine, Point, Solid, Text,
};
use crate::file::Version;
use crate::write::drawing::{rad, rad_pair};
use zumen_core::edit::{EditError, ValueReader};

const Z: [f64; 3] = [0.0, 0.0, 1.0];

fn href(code: u8, value: u64) -> HandleRef {
    HandleRef { code, value }
}

pub(super) fn line(start: [f64; 3], end: [f64; 3]) -> Body {
    Body::Line(Line { start, end, thickness: 0.0, extrusion: Z })
}

pub(super) fn circle(center: [f64; 3], radius: f64) -> Body {
    Body::Circle(Circle { center, radius, thickness: 0.0, extrusion: Z })
}

/// 円弧。角度は度で、読み取りで同じ度に戻るラジアンにする（開きも保つ）
pub(super) fn arc(center: [f64; 3], radius: f64, angle: [f64; 2]) -> Body {
    let (start_angle, end_angle) = rad_pair(angle[0], angle[1]);
    Body::Arc(Arc { circle: Circle { center, radius, thickness: 0.0, extrusion: Z }, start_angle, end_angle })
}

pub(super) fn ellipse(center: [f64; 3], major_axis: [f64; 3], ratio: f64, parameter: [f64; 2]) -> Body {
    Body::Ellipse(Ellipse { center, major_axis, extrusion: Z, ratio, start: parameter[0], end: parameter[1] })
}

pub(super) fn point(location: [f64; 3]) -> Body {
    Body::Point(Point { location, thickness: 0.0, extrusion: Z, x_axis_angle: 0.0 })
}

/// LWPOLYLINE（20.4.85）。印は 512 閉じる、4 一定の幅、8 高さ、16 ふくらみ
pub(super) fn lwpolyline(vertices: &[[f64; 3]], bulges: &[f64], closed: bool, width: Option<f64>) -> Body {
    let mut polyline = LwPolyline {
        flags: 0,
        const_width: 0.0,
        elevation: vertices[0][2],
        thickness: 0.0,
        extrusion: Z,
        points: Vec::new(),
        bulges: Vec::new(),
        ids: Vec::new(),
        widths: Vec::new(),
    };
    set_vertices(&mut polyline, vertices.iter().map(|vertex| [vertex[0], vertex[1]]).collect(), bulges.to_vec());
    polyline.flags = flags(&polyline, closed, width.is_some());
    if let Some(width) = width {
        polyline.const_width = width;
    }
    Body::LwPolyline(polyline)
}

/// 頂点とふくらみ。頂点の幅と番号は除く
fn set_vertices(polyline: &mut LwPolyline, points: Vec<[f64; 2]>, bulges: Vec<f64>) {
    polyline.points = points;
    polyline.bulges = if bulges.iter().any(|bulge| *bulge != 0.0) { bulges } else { Vec::new() };
    polyline.widths.clear();
    polyline.ids.clear();
}

fn flags(polyline: &LwPolyline, closed: bool, const_width: bool) -> i16 {
    // 押し出しの向き・厚さ・線種の生成（1・2・256）は元のまま、ほかは値から決める
    let mut flags = polyline.flags & (1 | 2 | 256);
    if closed {
        flags |= 512;
    }
    if const_width {
        flags |= 4;
    }
    if polyline.elevation != 0.0 {
        flags |= 8;
    }
    if !polyline.bulges.is_empty() {
        flags |= 16;
    }
    // ODA 20.4.85: these flags control whether the retained arrays are written.
    if !polyline.widths.is_empty() {
        flags |= 32;
    }
    if !polyline.ids.is_empty() {
        flags |= 1024;
    }
    flags
}

/// LWPOLYLINE の値を変える（DXF の編集と同じく、頂点を変えると頂点の幅は除き、ふくらみは指定がなければ 0）
pub(super) fn update_lwpolyline(
    index: usize,
    polyline: &mut LwPolyline,
    reader: &ValueReader<'_>,
) -> Result<(), EditError> {
    let vertices = reader.points("vertices", 2)?;
    let closed = reader.bool("closed")?;
    let width = reader.number("width")?;
    let count = vertices.as_ref().map_or(polyline.points.len(), Vec::len);
    let bulges = match reader.numbers("bulges", count)? {
        Some(bulges) => Some(bulges),
        None if reader.has("bulges") => {
            return Err(EditError::at(index, "invalid-value", "bulges は頂点と同じ数にしてください"));
        }
        None => None,
    };
    if let Some(vertices) = &vertices {
        let elevation = vertices[0][2];
        if vertices.iter().any(|vertex| vertex[2] != elevation) {
            return Err(EditError::at(index, "invalid-value", "polyline の頂点の z はすべて同じにしてください"));
        }
        polyline.elevation = elevation;
    }
    if vertices.is_some() || bulges.is_some() {
        let points: Vec<[f64; 2]> = match &vertices {
            Some(vertices) => vertices.iter().map(|vertex| [vertex[0], vertex[1]]).collect(),
            None => polyline.points.clone(),
        };
        let current =
            if polyline.bulges.len() == points.len() { polyline.bulges.clone() } else { vec![0.0; points.len()] };
        let bulges = bulges.unwrap_or_else(|| if vertices.is_some() { vec![0.0; points.len()] } else { current });
        if vertices.is_some() {
            set_vertices(polyline, points, bulges);
        } else {
            polyline.bulges = if bulges.iter().any(|bulge| *bulge != 0.0) { bulges } else { Vec::new() };
        }
    }
    let closed = closed.unwrap_or(polyline.flags & 512 != 0);
    let const_width = width.is_some() || polyline.flags & 4 != 0;
    if let Some(width) = width {
        polyline.const_width = width;
    }
    polyline.flags = flags(polyline, closed, const_width);
    Ok(())
}

/// TEXT（20.4.1）。省ける値（高さ 0 の Z、回転 0、幅の倍率 1 など）はデータの印で省く
pub(super) fn text(position: [f64; 3], value: RawText, height: f64, rotation: f64, style: u64) -> Body {
    Body::Text(Text {
        elevation: (position[2] != 0.0).then_some(position[2]),
        insert: [position[0], position[1]],
        align: None,
        extrusion: Z,
        thickness: 0.0,
        oblique: None,
        rotation: (rotation != 0.0).then(|| rad(rotation)),
        height,
        width: None,
        value,
        generation: None,
        halign: None,
        valign: None,
        style: href(5, style),
    })
}

/// 1 行の文字の位置を動かす。位置合わせが左・ベースライン以外なら、配置に使われる位置合わせの点も同じだけ動かす
/// （DXF の編集と同じ。72・73 が 0 でなければ 11 が位置を決める）
pub(super) fn move_text(text: &mut Text, position: [f64; 3]) {
    let aligned = text.halign.unwrap_or(0) != 0 || text.valign.unwrap_or(0) != 0;
    let anchor = match text.align {
        Some(align) if aligned => align,
        _ => text.insert,
    };
    let delta = [position[0] - anchor[0], position[1] - anchor[1]];
    text.insert = [text.insert[0] + delta[0], text.insert[1] + delta[1]];
    if let Some(align) = &mut text.align {
        *align = [align[0] + delta[0], align[1] + delta[1]];
    }
    text.elevation = (position[2] != 0.0).then_some(position[2]);
}

/// MTEXT（20.4.46）。attachment は配置（文字は 7 の左下、寸法の文字は 5 の中央）、左から右へ（1）、行間は倍率 1
/// （DXF の編集と同じ）
#[allow(clippy::too_many_arguments)]
pub(super) fn mtext(
    version: Version,
    position: [f64; 3],
    content: RawText,
    height: f64,
    rotation: f64,
    wrap: Option<f64>,
    style: u64,
    attachment: i16,
) -> Body {
    let (sin, cos) = rad(rotation).sin_cos();
    let direction = if rotation == 0.0 { [1.0, 0.0, 0.0] } else { [cos, sin, 0.0] };
    let rect_width = wrap.unwrap_or(0.0);
    let mut mtext = MText {
        insert: position,
        extrusion: Z,
        direction,
        rect_width,
        rect_height: Some(0.0),
        char_height: height,
        attachment,
        drawing_direction: 1,
        extents_height: 0.0,
        extents_width: 0.0,
        text: content,
        style: href(5, style),
        line_spacing_style: 1,
        line_spacing_factor: 1.0,
        unknown_bit: false,
        background_flags: 0,
        background: None,
        not_annotative: None,
        annotative_flag: None,
    };
    // R2018 以降は AutoCAD と同じく注釈でない MTEXT（Is NOT annotative）として、同じ値を繰り返す
    if version >= Version::R2018 {
        mtext.not_annotative = Some(Box::new(MTextRedundant {
            version: 4,
            default_flag: true,
            registered_app: href(5, 0),
            attachment: i32::from(attachment),
            direction,
            insert: position,
            rect_width,
            rect_height: 0.0,
            extents_width: 0.0,
            extents_height: 0.0,
            column_type: 0,
            columns: None,
        }));
    }
    Body::MText(Box::new(mtext))
}

/// MTEXT の値を変える。回転は向き（X 軸の向き）にする
pub(super) fn update_mtext(
    mtext: &mut MText,
    position: Option<[f64; 3]>,
    content: Option<RawText>,
    height: Option<f64>,
    rotation: Option<f64>,
    wrap: Option<f64>,
    style: Option<u64>,
) {
    if let Some(position) = position {
        mtext.insert = position;
    }
    if let Some(content) = content {
        mtext.text = content;
    }
    if let Some(height) = height {
        mtext.char_height = height;
    }
    if let Some(rotation) = rotation {
        let (sin, cos) = rad(rotation).sin_cos();
        mtext.direction = if rotation == 0.0 { [1.0, 0.0, 0.0] } else { [cos, sin, 0.0] };
    }
    if let Some(wrap) = wrap {
        mtext.rect_width = wrap;
    }
    if let Some(style) = style {
        mtext.style = href(5, style);
    }
    if let Some(redundant) = &mut mtext.not_annotative {
        redundant.insert = mtext.insert;
        redundant.direction = mtext.direction;
        redundant.rect_width = mtext.rect_width;
    }
}

/// INSERT（20.4.9）。属性なし
pub(super) fn insert(position: [f64; 3], scale: [f64; 3], rotation: f64, block: u64) -> Body {
    Body::Insert(Insert {
        point: position,
        scale,
        scale_flags: Insert::scale_flags_of(scale),
        rotation: rad(rotation),
        extrusion: Z,
        has_attributes: false,
        array: None,
        block: href(5, block),
        attributes: Vec::new(),
        first_last: None,
        seqend: None,
    })
}

/// SOLID（20.4.35）。三角形は 4 つ目の角を 3 つ目と同じにする（DXF の SOLID と同じ）
pub(super) fn solid(corners: [[f64; 3]; 3]) -> Body {
    let xy = |point: [f64; 3]| [point[0], point[1]];
    Body::Solid(Solid {
        thickness: 0.0,
        elevation: corners[0][2],
        corners: [xy(corners[0]), xy(corners[1]), xy(corners[2]), xy(corners[2])],
        extrusion: Z,
    })
}

/// HATCH（20.4.75）。境界は閉じた折線（外側は 1 も立てる）、関連づけない（DXF の編集と同じ）。角度はラジアン
pub(super) fn hatch(input: &zumen_dxf::hatch::HatchInput, name: RawText) -> Body {
    let paths = input
        .boundaries
        .iter()
        .enumerate()
        .map(|(number, boundary)| HatchPath {
            flags: if number == 0 { 3 } else { 2 },
            boundary: HatchBoundary::Polyline {
                has_bulges: false,
                closed: true,
                points: boundary.iter().map(|point| ([point[0], point[1]], 0.0)).collect(),
            },
            objects: Vec::new(),
        })
        .collect();
    let pattern = (!input.solid).then(|| HatchPattern {
        angle: rad(input.angle),
        scale: input.scale,
        double: false,
        lines: input
            .lines
            .iter()
            .map(|line| PatternLine {
                angle: rad(line.angle),
                base: line.base,
                offset: line.offset,
                dashes: line.dashes.clone(),
            })
            .collect(),
    });
    Body::Hatch(Box::new(Hatch {
        gradient: None,
        elevation: input.boundaries[0][0][2],
        extrusion: Z,
        name,
        solid_fill: input.solid,
        associative: false,
        paths,
        style: 0,
        pattern_type: input.pattern_type as i16,
        pattern,
        pixel_size: None,
        seeds: Vec::new(),
    }))
}
