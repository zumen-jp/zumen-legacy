//! Jw_cad の JWW ファイルを読み、zumen の表示用モデル（`Scene`）にする。`@zumenjs/jww` の Rust 版。
//! JWW を書く（`JwwFile` は読んだファイルを元と同じバイト列に書き戻し、`write_jww` は新しい図面を書く）。
//! JWW をその場で編集する（`edit_jww`。docs/decisions/0012）。Jw_cad for DOS の JWC も読み書きする（`JwcFile`、
//! `read_jwc`、`write_jwc`、読み取りは `JWC`。TypeScript の読み取りはない）。図形ファイル JWS も同じく読み書きする
//! （`JwsFile`、`read_jws`、`write_jws`、読み取りは `JWS`）。
#![forbid(unsafe_code)]

mod archive;
mod create;
mod edit;
mod file;
mod header;
mod jwc;
mod jws;
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
pub use jws::{JWS_SIGNATURE, JwsDocument, JwsFile, JwsHeader, is_jws, jws_to_scene, read_jws, write_jws};
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

/// read_drawing に渡す JWC（Jw_cad for DOS の図面）の読み取り。Rust だけで、TypeScript の読み取りはない
pub struct Jwc;

/// JWC の読み取り。識別子 `jw_cad(c)data` で判定し、JWW と同じ図面の内容に写して描く
pub const JWC: Jwc = Jwc;

impl DrawingReader for Jwc {
    fn formats(&self) -> &'static [&'static str] {
        &["jwc"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_jwc(bytes).then_some("jwc")
    }

    fn read(&self, bytes: &[u8], format: &str, _options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "jwc" {
            return Err(ReadError::new(format!("JWC の読み取りは {format} を読めません")));
        }
        Ok(jwc_to_scene(&read_jwc(bytes)?))
    }
}

/// read_drawing に渡す JWS（Jw_cad の図形ファイル）の読み取り。Rust だけで、TypeScript の読み取りはない
pub struct Jws;

/// JWS の読み取り。識別子 `JwsData.` で判定し、JWW と同じ図形を用紙なしで描く
pub const JWS: Jws = Jws;

impl DrawingReader for Jws {
    fn formats(&self) -> &'static [&'static str] {
        &["jws"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_jws(bytes).then_some("jws")
    }

    fn read(&self, bytes: &[u8], format: &str, _options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "jws" {
            return Err(ReadError::new(format!("JWS の読み取りは {format} を読めません")));
        }
        Ok(jws_to_scene(&read_jws(bytes)?))
    }
}

/// JWC の図面（`read_jwc` で JWW と同じ図面の内容に写したもの）の表示用の図面。形式は jwc、版は JWC
pub fn jwc_to_scene(doc: &JwwDocument) -> Scene {
    let mut scene = jww_to_scene(doc);
    scene.format = "jwc".to_string();
    scene.version = "JWC".to_string();
    if let Some(format) = scene.metadata.iter_mut().find(|property| property.key == "format") {
        *format = zumen_core::Property::new("format", "形式", "JWC（Jw_cad for DOS）");
    }
    scene
}
