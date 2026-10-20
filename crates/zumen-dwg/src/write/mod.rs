//! DWG を書く。読み取りと同じく Open Design Specification for .dwg files 5.4.1 に従い、節の番号を注記に書く。
//!
//! - `rewrite_dwg`: 読んだ DWG を、オブジェクトをビット列のまま写して同じ版で書き直す（その場の編集の土台）
//! - 入れ物（container）・オブジェクト（object）・区切りで囲む節（sections）・圧縮（compress）は、どの書き方にも使う

pub(crate) mod bits;
pub(crate) mod classes;
pub(crate) mod compress;
pub(crate) mod container;
pub(crate) mod database;
pub(crate) mod object;
mod rewrite;
pub(crate) mod sections;

pub use rewrite::rewrite_dwg;
use zumen_core::Notice;

/// 書いた DWG と、書けなかった・形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DwgWritten {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}
