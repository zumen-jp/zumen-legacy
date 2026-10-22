//! 足す引出線（docs/decisions/0017）。DXF と DWG の編集が、引出線（LEADER）の頂点と寸法スタイルから、注釈の文字
//! （MTEXT）の位置・配置・高さを決める。文字は最後の頂点から DIMGAP だけ離し、最後の線分が右へ向かうなら左端を、
//! 左へ向かうなら右端を合わせ、縦は中央にそろえる（AutoCAD の引出線の注釈と同じ）。

use super::dimension::DimStyleValues;
use zumen_core::edit::{EditError, ValueReader};

/// leader の値のキー
pub const KEYS: [&str; 5] = ["vertices", "text", "height", "dimensionStyle", "textStyle"];

/// 引出線の注釈の文字
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub position: [f64; 3],
    /// MTEXT の 71（4 左中、6 右中）
    pub attachment: i64,
    pub height: f64,
}

/// 頂点（最初が矢印の先）と、文字の高さ（省略は DIMTXT × DIMSCALE）を読む
pub fn read(reader: &ValueReader<'_>) -> Result<(Vec<[f64; 3]>, Option<f64>), EditError> {
    let vertices = reader.required("vertices", reader.points("vertices", 2)?)?;
    Ok((vertices, reader.positive("height")?))
}

/// 注釈の文字の位置と配置
pub fn annotation(vertices: &[[f64; 3]], height: Option<f64>, style: &DimStyleValues) -> Annotation {
    let scale = if style.scale > 0.0 { style.scale } else { 1.0 };
    let last = vertices[vertices.len() - 1];
    let before = vertices[vertices.len() - 2];
    let gap = style.gap.abs() * scale;
    let right = last[0] >= before[0];
    Annotation {
        position: [last[0] + if right { gap } else { -gap }, last[1], last[2]],
        attachment: if right { 4 } else { 6 },
        height: height.unwrap_or(style.text_height * scale),
    }
}
