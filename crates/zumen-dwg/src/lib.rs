//! AutoCAD の DWG（R2000〜2018）を読み、DXF と同じ AutoCAD の図面のモデル（`zumen_dxf::DxfDocument`）にする。
//! 表示用の図面（`Scene`）は `zumen_dxf::dxf_to_scene` で作る（docs/decisions/0003-dwg-in-rust.md）。
//! 読んだ DWG を同じ版で書き直せる（`rewrite_dwg`。オブジェクトはビット列のまま写す）。
//! 読み方・書き方は Open Design Alliance の Open Design Specification for .dwg files 5.4.1 に従い、節の番号を注記に書く。
#![forbid(unsafe_code)]

mod bits;
mod codec;
mod compress;
mod crc;
mod document;
mod edit;
mod entities;
mod file;
mod header;
mod mleader;
mod objects;
mod r12;
#[cfg(test)]
mod samples;
mod write;

pub use document::read_dwg;
pub use edit::{edit_dwg, new_dwg};
pub use file::is_dwg;
pub use write::{DwgWritten, rewrite_dwg, write_dwg};

/// テストが規則を確かめる最小の図面を組み立てるための、書き出しの部品（tests/writer が使う）。安定した API ではない
#[doc(hidden)]
pub mod raw {
    pub use crate::file::Version;
    pub use crate::write::bits::BitWriter;
    pub use crate::write::classes::ClassDef;
    pub use crate::write::database::Database;
    pub use crate::write::object::ObjectWriter;
}

use zumen_core::{DrawingReader, ReadError, ReadOptions, Scene};
use zumen_dxf::dxf_to_scene;

/// read_drawing に渡す DWG の読み取り。options.view はモデル空間（Model）かレイアウトの名前
pub struct Dwg;

/// DWG の読み取り
pub const DWG: Dwg = Dwg;

impl DrawingReader for Dwg {
    fn formats(&self) -> &'static [&'static str] {
        &["dwg"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_dwg(bytes).then_some("dwg")
    }

    fn read(&self, bytes: &[u8], format: &str, options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "dwg" {
            return Err(ReadError::new(format!("DWG の読み取りは {format} を読めません")));
        }
        let doc = read_dwg(bytes).map_err(ReadError::new)?;
        Ok(dxf_to_scene(&doc, options.view.as_deref()))
    }
}
