//! zumen: 建築・土木の 2D 図面を読み、形式に依存しない表示用モデル（`Scene`）にする。
//!
//! 形式は拡張子ではなく中身で判定する。読める形式は SXF（SFC・P21、圧縮した P2Z・SFZ）・JWW・DXF・DWG。
//! 受け取った図面を信用できないときは、`ReadDrawingOptions::limits` でバイト数・図形の数・時間の上限を指定する。
//! どの図面も AutoCAD の図面のモデルを経て、AutoCAD 2013 の DXF と SXF（SFC）に書き直せる（`convert`）。
//! 図面をその場で編集し（`edit`）、新しい図面を作れる（`create`）。操作は形式によらない（`edit::parse_operations`）。
//!
//! ```no_run
//! let bytes = std::fs::read("plan.jww").unwrap();
//! let scene = zumen::read(&bytes, &zumen::ReadDrawingOptions::default()).unwrap();
//! println!("{} entities", scene.entities.len());
//!
//! let dwg = std::fs::read("plan.dwg").unwrap();
//! let converted = zumen::convert(&dwg, "dxf", &zumen::ReadDrawingOptions::default()).unwrap();
//! std::fs::write("plan.dxf", &converted.bytes).unwrap();
//! ```
#![forbid(unsafe_code)]

pub use zumen_convert as mapping;
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

/// 書き出した図面と、書けなかった・形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}

/// 書き出せる形式（dxf は AutoCAD 2013 の DXF、sfc は SXF Ver.3.1 の SFC）
pub const WRITABLE_FORMATS: [&str; 2] = ["dxf", "sfc"];

/// 図面を読み、別の形式に書き直す。`to` は書く形式で、dxf（ASCII、AutoCAD 2013 形式、UTF-8）か sfc（SXF Ver.3.1、
/// CP932）。どの図面も AutoCAD の図面のモデル（`dxf::DxfDocument`）を経る。DXF・DWG はそのまま読み、SXF・JWW は
/// モデルに写す（`mapping`。モデル空間は実寸、縮尺ごとに用紙のレイアウトのビューポート）。SFC は、モデルを SXF の
/// 図面に写して書く（`mapping::dxf_to_sxf`。ビューポートは部分図、ブロックは作図部品）
pub fn convert(bytes: &[u8], to: &str, options: &ReadDrawingOptions) -> Result<Converted, ReadError> {
    convert_with(bytes, to, options, &WriteOptions::default())
}

/// 書き出しの指定
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// 書く版。DXF は 2000・2004・2007・2010・2013・2018（`dxf::DxfVersion::parse` が読む名前。R12 はまだ書けない）。
    /// 省略すると形式の既定（DXF は 2013）
    pub version: Option<String>,
}

/// `convert` に書き出しの指定（版など）を加えたもの
pub fn convert_with(
    bytes: &[u8],
    to: &str,
    options: &ReadDrawingOptions,
    write: &WriteOptions,
) -> Result<Converted, ReadError> {
    if !WRITABLE_FORMATS.contains(&to) {
        return Err(ReadError::new(format!("{} には書き出せません（書き出せるのは DXF・SFC）", to.to_uppercase())));
    }
    let dxf_version = match (to, write.version.as_deref()) {
        (_, None) => dxf::DxfVersion::R2013,
        ("dxf", Some(name)) => dxf::DxfVersion::parse(name).ok_or_else(|| {
            ReadError::new(format!("DXF の版 {name} は書けません（書けるのは 2000・2004・2007・2010・2013・2018）"))
        })?,
        (other, Some(_)) => {
            return Err(ReadError::new(format!("{} の版はまだ選べません", other.to_uppercase())));
        }
    };
    let limits = &options.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(limit_error(Limit::Bytes, limits));
    }
    let format = match options.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| ReadError::new("対応していない形式のファイルです"))?,
    };
    let mut notices = NoticeList::new();
    let (doc, exceeded) = zumen_core::limits::with_limits(limits, || -> Result<_, ReadError> {
        Ok(match format {
            "dxf" => dxf::parse_dxf(bytes),
            "dwg" => dwg::read_dwg(bytes).map_err(ReadError::new)?,
            "jww" => {
                let mapped = mapping::jww_to_dxf(&jww::read_jww(bytes)?);
                notices = NoticeList::from_notices(&mapped.notices);
                mapped.document
            }
            format if sxf::SXF.formats().contains(&format) => {
                let mapped = mapping::sxf_to_dxf(&sxf::parse_sxf(bytes, format)?);
                notices = NoticeList::from_notices(&mapped.notices);
                mapped.document
            }
            other => return Err(ReadError::new(format!("{} の図面は書き直せません", other.to_uppercase()))),
        })
    });
    // 上限（バイト数・時間）を超えたら、途中まで読んだ図面は書かない
    if let Some(limit) = exceeded {
        return Err(limit_error(limit, limits));
    }
    let doc = doc?;
    let (bytes, written) = if to == "sfc" {
        let (mapped, exceeded) = zumen_core::limits::with_limits(limits, || mapping::dxf_to_sxf(&doc));
        if let Some(limit) = exceeded {
            return Err(limit_error(limit, limits));
        }
        for notice in &mapped.notices {
            notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
        }
        let written = sxf::write_sfc(&mapped.drawing);
        (written.bytes, written.notices)
    } else {
        let written = dxf::write_dxf_as(&doc, dxf_version).map_err(ReadError::new)?;
        (written.bytes, written.notices)
    };
    for notice in &written {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    Ok(Converted { bytes, notices: notices.into_vec() })
}

/// その場で編集できる形式
pub const EDITABLE_FORMATS: [&str; 1] = ["dxf"];

/// 図面をその場で編集する（docs/decisions/0012）。元のファイルを保ち、操作した図形だけを書き換えるので、
/// 形式・版・文字コードと、読み取りが扱わない内容（拡張データ、未対応の図形・オブジェクトなど）は元のまま残る。
/// すべての操作を確かめてから書き、1 つでも通らなければ何も書かずにエラーを返す。
/// `options.format` は分かっている場合の形式、`options.limits.max_bytes` は受け付ける大きさの上限
pub fn edit(
    bytes: &[u8],
    operations: &[edit::EditOperation],
    options: &ReadDrawingOptions,
) -> Result<edit::Edited, edit::EditError> {
    let limits = &options.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(edit::EditError::new("too-large", limit_error(Limit::Bytes, limits).to_string()));
    }
    let format = match options.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| edit::EditError::new("unreadable", "対応していない形式のファイルです"))?,
    };
    match format {
        "dxf" => dxf::edit_dxf(bytes, operations),
        other => Err(unsupported_edit(other)),
    }
}

/// 新しい図面を作り、操作（add・add-layer など）を当てる。to は形式、units は長さの単位
/// （mm、cm、m、inch、foot、unitless）
pub fn create(to: &str, units: &str, operations: &[edit::EditOperation]) -> Result<edit::Edited, edit::EditError> {
    match to {
        "dxf" => dxf::edit_dxf(&dxf::new_dxf(units)?, operations),
        other => Err(unsupported_edit(other)),
    }
}

fn unsupported_edit(format: &str) -> edit::EditError {
    let editable: Vec<String> = EDITABLE_FORMATS.iter().map(|format| format.to_uppercase()).collect();
    edit::EditError::new(
        "unsupported-format",
        format!("{} の図面はまだ編集できません（編集できるのは {}）", format.to_uppercase(), editable.join("・")),
    )
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
