//! AutoCAD の DXF（R12〜2018、テキストとバイナリ）を読み、AutoCAD の図面のモデル（`DxfDocument`）と
//! zumen の表示用モデル（`Scene`）にする。`@zumenjs/dxf` の Rust 版で、同じ図面から同じ `Scene` を返す。
//! モデルを DXF に書き戻す（`write_dxf` は AutoCAD 2013、`write_dxf_as` は R12〜2018 の版を選ぶ）。
#![forbid(unsafe_code)]

mod colors;
mod curves;
mod edit;
mod model;
mod mtext;
mod parse;
mod r12;
mod scene;
mod tags;
mod write;

pub use colors::{ACI, aci_color, is_transparent, true_color};
pub use curves::{minor_axis, ocs_matrix};
pub use edit::{attributes, dimension, edit_dxf, font_file, hatch, new_dxf, paper_name, placement, view_height};
pub use model::*;
pub use mtext::{
    ParagraphAlign, RunColor, RunFont, RunStack, TextParagraph, TextRun, mtext_plain_text, parse_mtext,
    parse_text_codes,
};
pub use parse::parse_dxf;
pub use scene::{
    DESCENT as TEXT_DESCENT, DrawOrder, dash_array, draw_order, drawing_unit, dxf_to_scene, em_for as text_em,
};
pub use tags::{codepage_encoding, decode_escapes, is_ascii_dxf, is_binary_dxf};
pub use write::{DxfVersion, DxfWritten, write_dxf, write_dxf_as};

use zumen_core::{DrawingReader, ReadError, ReadOptions, Scene};

/// 中身がテキストかバイナリの DXF か
pub fn is_dxf(bytes: &[u8]) -> bool {
    is_binary_dxf(bytes) || is_ascii_dxf(bytes)
}

/// read_drawing に渡す DXF の読み取り。options.view はモデル空間（Model）かレイアウトの名前
pub struct Dxf;

/// DXF の読み取り（`@zumenjs/dxf` の `dxf`）
pub const DXF: Dxf = Dxf;

impl DrawingReader for Dxf {
    fn formats(&self) -> &'static [&'static str] {
        &["dxf"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_dxf(bytes).then_some("dxf")
    }

    fn read(&self, bytes: &[u8], format: &str, options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "dxf" {
            return Err(ReadError::new(format!("DXF の読み取りは {format} を読めません")));
        }
        Ok(dxf_to_scene(&parse_dxf(bytes), options.view.as_deref()))
    }
}
