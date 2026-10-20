//! Jw_cad の JWW ファイルを読み、zumen の表示用モデル（`Scene`）にする。`@zumenjs/jww` の Rust 版。
#![forbid(unsafe_code)]

mod reader;
mod scene;
mod style;
mod sxf_tables;

pub use reader::{
    JwwArc, JwwBase, JwwBlock, JwwBlockDefinition, JwwCircleSolid, JwwDimension, JwwDocument, JwwEntity, JwwLayer,
    JwwLayerGroup, JwwLine, JwwPoint, JwwSolid, JwwText, WidthUnit, is_jww, read_jww,
};
pub use scene::jww_to_scene;
pub use style::{
    DEFAULT_PEN_WIDTH, is_embedded_image, is_random_line, is_setting, paper_size, text_layout_input,
};
pub use sxf_tables::{SXF_COLORS, SXF_LINE_PITCHES};

use zumen_core::{DrawingReader, ReadError, ReadOptions, Scene};

/// read_drawing に渡す JWW の読み取り
pub struct Jww;

/// JWW の読み取り（`@zumenjs/jww` の `jww`）
pub const JWW: Jww = Jww;

impl DrawingReader for Jww {
    fn formats(&self) -> &'static [&'static str] {
        &["jww"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_jww(bytes).then_some("jww")
    }

    fn read(&self, bytes: &[u8], format: &str, _options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "jww" {
            return Err(ReadError::new(format!("JWW の読み取りは {format} を読めません")));
        }
        Ok(jww_to_scene(&read_jww(bytes)?))
    }
}
