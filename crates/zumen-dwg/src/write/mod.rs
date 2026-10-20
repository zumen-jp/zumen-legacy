//! DWG を書く。読み取りと同じく Open Design Specification for .dwg files 5.4.1 に従い、節の番号を注記に書く。
//!
//! - `rewrite_dwg`: 読んだ DWG を、オブジェクトをビット列のまま写して同じ版で書き直す（その場の編集の土台）
//! - `write_dwg`: AutoCAD の図面のモデルを AutoCAD 2013 の DWG（AC1027）に書く。`write_dwg_as` は版を選ぶ
//!   （R13〜2018。その版が持てない値は近い形にして notices で知らせる）
//! - 入れ物（container）・オブジェクト（object）・区切りで囲む節（sections）・圧縮（compress）は、どの書き方にも使う

pub(crate) mod bits;
pub(crate) mod classes;
pub(crate) mod compress;
mod compress21;
pub(crate) mod container;
pub(crate) mod container21;
pub(crate) mod database;
pub(crate) mod drawing;
pub(crate) mod object;
pub(crate) mod rewrite;
pub(crate) mod sections;
pub(crate) mod version;

pub use drawing::{write_dwg, write_dwg_as};
pub use rewrite::rewrite_dwg;
pub use version::DwgVersion;
use zumen_core::Notice;

/// 書いた DWG と、書けなかった・形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DwgWritten {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}
