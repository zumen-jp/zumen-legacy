//! zumen: 建築・土木の 2D 図面を読み、形式に依存しない表示用モデル（`Scene`）にする。
//!
//! 形式は拡張子ではなく中身で判定する。読める形式は SXF（SFC・P21、圧縮した P2Z・SFZ）・JWW・DXF・DWG。
//! 受け取った図面を信用できないときは、`ReadDrawingOptions::limits` でバイト数・図形の数・時間の上限を指定する。
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
    /// 分かっている場合の形式（sfc・p21・p2z・sfz・jww・dxf・dwg など）。省略すると中身から判定する
    pub format: Option<String>,
    /// 描く表示（Scene.views の id）。省略すると形式ごとの既定の表示
    pub view: Option<String>,
    /// 読み取りの上限。超えたら途中でやめ、`ReadError::exceeded` で上限の種類が分かるエラーを返す
    pub limits: ReadLimits,
}

/// 図面の形式を中身から判定する。読めない形式なら None
pub fn detect(bytes: &[u8]) -> Option<&'static str> {
    detect_format(bytes, &readers())
}

/// 図面のバイト列を読み、表示用の図面にする。上限（`options.limits`）を超えたら、途中でやめて上限のエラーを返す
pub fn read(bytes: &[u8], options: &ReadDrawingOptions) -> Result<Scene, ReadError> {
    let limits = &options.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(limit_error(Limit::Bytes, limits));
    }
    let read_options = ReadOptions { view: options.view.clone() };
    let (result, exceeded) = zumen_core::limits::with_limits(limits, || {
        read_drawing(bytes, &readers(), options.format.as_deref(), &read_options)
    });
    // 上限を超えたときは、途中までの結果や、途中でやめたために起きた読み取りのエラーより上限のエラーを返す
    match (exceeded, result) {
        (Some(_), Err(error)) if error.exceeded().is_some() => Err(error),
        (Some(limit), _) => Err(limit_error(limit, limits)),
        (None, result) => result,
    }
}

/// 上限を超えたときのエラー（利用者に見せる文）
pub fn limit_error(limit: Limit, limits: &ReadLimits) -> ReadError {
    let count = |value: Option<u64>| value.map_or_else(String::new, |value| js::format_ja(value as f64, 0));
    let message = match limit {
        Limit::Bytes => format!("図面が上限の {} バイトより大きいため、読みませんでした", count(limits.max_bytes)),
        Limit::Entities => {
            format!("図形が上限の {} 個を超えたため、読むのを途中でやめました", count(limits.max_entities))
        }
        Limit::Time => {
            let seconds = limits.max_duration.map_or(0.0, |duration| duration.as_secs_f64());
            format!("読み取りが上限の {} 秒を超えたため、途中でやめました", js::number_to_string(seconds))
        }
    };
    ReadError::limit(limit, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fixture(path: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn limited(limits: ReadLimits) -> ReadDrawingOptions {
        ReadDrawingOptions { limits, ..ReadDrawingOptions::default() }
    }

    #[test]
    fn stops_reading_at_each_limit() {
        for path in ["sfc/features.sfc", "p21/features.p21", "jww/minimal-v700.jww", "dxf/features.dxf"] {
            let bytes = fixture(path);
            let all = read(&bytes, &ReadDrawingOptions::default()).unwrap();
            // 上限が十分なら、上限なしと同じ結果
            let roomy = ReadLimits {
                max_bytes: Some(bytes.len() as u64),
                max_entities: Some(all.entities.len() as u64),
                max_duration: Some(Duration::from_secs(60)),
            };
            assert_eq!(read(&bytes, &limited(roomy)).unwrap(), all, "{path}");
            let few = ReadLimits { max_entities: Some(all.entities.len() as u64 - 1), ..ReadLimits::default() };
            let error = read(&bytes, &limited(few)).unwrap_err();
            assert_eq!(error.exceeded(), Some(Limit::Entities), "{path}");
            let small = ReadLimits { max_bytes: Some(bytes.len() as u64 - 1), ..ReadLimits::default() };
            assert_eq!(read(&bytes, &limited(small)).unwrap_err().exceeded(), Some(Limit::Bytes), "{path}");
            let now = ReadLimits { max_duration: Some(Duration::ZERO), ..ReadLimits::default() };
            assert_eq!(read(&bytes, &limited(now)).unwrap_err().exceeded(), Some(Limit::Time), "{path}");
        }
        let error =
            read(&fixture("sfc/features.sfc"), &limited(ReadLimits { max_entities: Some(5), ..ReadLimits::default() }))
                .unwrap_err();
        assert_eq!(error.message(), "図形が上限の 5 個を超えたため、読むのを途中でやめました");
    }
}
