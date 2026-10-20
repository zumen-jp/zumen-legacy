//! Jw_cad の JWW ファイルを読み、zumen の表示用モデル（`Scene`）にする。`@zumenjs/jww` の Rust 版。
//! JWW を書く（`JwwFile` は読んだファイルを元と同じバイト列に書き戻し、`write_jww` は新しい図面を書く）。
//! JWW をその場で編集する（`edit_jww`。docs/decisions/0012）。Jw_cad for DOS の JWC も読み書きする（`JwcFile`、
//! `read_jwc`、`write_jwc`）。
#![forbid(unsafe_code)]

mod archive;
mod create;
mod edit;
mod file;
mod header;
mod jwc;
mod reader;
mod scene;
mod style;
mod sxf_tables;

pub use create::{JWW_VERSION, write_jww};
pub use edit::{edit_jww, new_jww};
pub use file::{JwwBlockRecord, JwwFile, JwwRecord, JwwWritten};
pub use header::{
    JwwDisplaySettings, JwwGeneralSettings, JwwGroupState, JwwHeader, JwwLayerState, JwwLinePattern, JwwMarkJump,
    JwwPen, JwwPrinterPen, JwwRandomLine, JwwSettings, JwwSxfLineType, JwwSxfPrinterPen, JwwSxfSettings,
    JwwTextSettings, JwwTextStyle,
};
pub use jwc::{
    JWC_SIGNATURE, JwcArc, JwcAttributes, JwcFile, JwcGroup, JwcLayer, JwcLine, JwcPoint, JwcString, JwcTemporaryPoint,
    JwcText, JwcTextStyle, is_jwc, read_jwc, write_jwc,
};
pub use reader::{
    JwwArc, JwwBase, JwwBlock, JwwBlockDefinition, JwwCircleSolid, JwwDimension, JwwDocument, JwwEntity, JwwLayer,
    JwwLayerGroup, JwwLine, JwwPoint, JwwSolid, JwwText, WidthUnit, is_jww, read_jww,
};
pub use scene::jww_to_scene;
pub use style::{DEFAULT_PEN_WIDTH, is_embedded_image, is_random_line, is_setting, paper_size, text_layout_input};
pub use sxf_tables::{SXF_COLORS, SXF_LINE_PITCHES};

use zumen_core::{DrawingReader, ReadError, ReadOptions, Scene};

/// read_drawing に渡す JWW の読み取り
pub struct Jww;

/// JWW の読み取り（`@zumenjs/jww` の `jww`）
pub const JWW: Jww = Jww;

impl DrawingReader for Jww {
    fn formats(&self) -> &'static [&'static str] {
        &["jww", "jwc"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        if is_jww(bytes) { Some("jww") } else { is_jwc(bytes).then_some("jwc") }
    }

    fn read(&self, bytes: &[u8], format: &str, _options: &ReadOptions) -> Result<Scene, ReadError> {
        match format {
            "jww" => Ok(jww_to_scene(&read_jww(bytes)?)),
            "jwc" => Ok(jwc_to_scene(&read_jwc(bytes)?)),
            _ => Err(ReadError::new(format!("JWW の読み取りは {format} を読めません"))),
        }
    }
}

/// JWC の図面（JWW と同じ図面の内容に写したもの）の表示用の図面。形式は jwc
fn jwc_to_scene(doc: &JwwDocument) -> Scene {
    let mut scene = jww_to_scene(doc);
    scene.format = "jwc".to_string();
    scene.version = "JWC".to_string();
    if let Some(format) = scene.metadata.iter_mut().find(|property| property.key == "format") {
        *format = zumen_core::Property::new("format", "形式", "JWC（Jw_cad for DOS）");
    }
    scene
}
