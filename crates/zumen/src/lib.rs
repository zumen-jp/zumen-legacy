//! zumen: 建築・土木の 2D 図面を読み、形式に依存しない表示用モデル（`Scene`）にする。
//!
//! 形式は拡張子ではなく中身で判定する。読める形式は SXF（SFC・P21）・JWW・DXF・DWG。
//!
//! ```no_run
//! let bytes = std::fs::read("plan.jww").unwrap();
//! let scene = zumen::read(&bytes, &zumen::ReadDrawingOptions::default()).unwrap();
//! println!("{} entities", scene.entities.len());
//! ```
#![forbid(unsafe_code)]

pub use zumen_core::*;
pub use zumen_dwg as dwg;
pub use zumen_dxf as dxf;
pub use zumen_jww as jww;
pub use zumen_sxf as sxf;

/// 組み込みの読み取り（判定する順）
pub fn readers() -> Vec<&'static dyn DrawingReader> {
    vec![&zumen_sxf::SXF, &zumen_jww::JWW, &zumen_dxf::DXF, &zumen_dwg::DWG]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadDrawingOptions {
    /// 分かっている場合の形式（sfc・p21・jww・dxf・dwg など）。省略すると中身から判定する
    pub format: Option<String>,
    /// 描く表示（Scene.views の id）。省略すると形式ごとの既定の表示
    pub view: Option<String>,
}

/// 図面の形式を中身から判定する。読めない形式なら None
pub fn detect(bytes: &[u8]) -> Option<&'static str> {
    detect_format(bytes, &readers())
}

/// DXF・DWG の図面を DXF（ASCII、AutoCAD 2013 形式、UTF-8）に書き直す。書けなかった図形の種類と件数も返す。
/// SXF・JWW は AutoCAD の図面のモデルにまだ変換できない
pub fn convert_to_dxf(bytes: &[u8], options: &ReadDrawingOptions) -> Result<dxf::DxfWritten, ReadError> {
    let format = match options.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| ReadError::new("対応していない形式のファイルです"))?,
    };
    let doc = match format {
        "dxf" => dxf::parse_dxf(bytes),
        "dwg" => dwg::read_dwg(bytes).map_err(ReadError::new)?,
        other => {
            return Err(ReadError::new(format!(
                "{} は DXF に変換できません（変換できるのは DXF・DWG）",
                other.to_uppercase()
            )));
        }
    };
    Ok(dxf::write_dxf(&doc))
}

/// 図面のバイト列を読み、表示用の図面にする
pub fn read(bytes: &[u8], options: &ReadDrawingOptions) -> Result<Scene, ReadError> {
    let read_options = ReadOptions { view: options.view.clone() };
    read_drawing(bytes, &readers(), options.format.as_deref(), &read_options)
}
