//! 読んだ SXF のファイル。書くとき、変えていないフィーチャ（SFC）・インスタンス（P21）を元の文字列のまま
//! 書き戻すために、図面（`SxfDrawing`）が持つ

use std::sync::Arc;

/// 読んだ SFC・P21 のファイル
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SxfSource {
    /// SFC・P21 のファイルのバイト列（圧縮した SXF は展開した後）
    pub bytes: Arc<[u8]>,
    /// 圧縮した SXF（P2Z・SFZ）なら、その書庫
    pub archive: Option<SxfArchive>,
}

/// 圧縮した SXF（P2Z・SFZ）の書庫
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SxfArchive {
    /// 書庫のバイト列（同じ書庫に入っている属性ファイル・画像も、書き直すときにそのまま残す）
    pub bytes: Arc<[u8]>,
    /// 書庫の中の SXF のファイルの名前
    pub name: String,
}

impl SxfSource {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        SxfSource { bytes: Arc::from(bytes), archive: None }
    }
}
