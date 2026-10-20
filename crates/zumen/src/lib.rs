//! zumen: 建築・土木の 2D 図面を読み、形式に依存しない表示用モデル（`Scene`）にする。
//!
//! 形式は拡張子ではなく中身で判定する。読める形式は SXF（SFC・P21、圧縮した P2Z・SFZ）・JWW（JWC・JWS も）・DXF・DWG。
//! 受け取った図面を信用できないときは、`ReadDrawingOptions::limits` でバイト数・図形の数・時間の上限を指定する。
//! どの図面も AutoCAD の図面のモデルを経て、DXF・DWG・JWW・SXF に書き直せる（`convert`）。
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
    vec![&zumen_sxf::SXF, &zumen_jww::JWW, &zumen_jww::JWC, &zumen_jww::JWS, &zumen_dxf::DXF, &zumen_dwg::DWG]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadDrawingOptions {
    /// 分かっている場合の形式（sfc・p21・p2z・sfz・jww・jwc・jws・dxf・dwg）。省略すると中身から判定する
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

/// 書き出せる形式。dxf は AutoCAD 2013 の DXF（版を選べる）、dwg は AutoCAD 2013 の DWG、jww は Jw_cad 8 の JWW、
/// sfc・p21（圧縮した sfz・p2z）は SXF Ver.3.1（版を選べる）
pub const WRITABLE_FORMATS: [&str; 7] = ["dxf", "dwg", "jww", "sfc", "p21", "sfz", "p2z"];

/// 図面を読み、別の形式に書き直す。`to` は書く形式（`WRITABLE_FORMATS`）。どの図面も AutoCAD の図面のモデル
/// （`dxf::DxfDocument`）を経る。DXF・DWG はそのまま読み、SXF・JWW（JWC・JWS）はモデルに写す（`mapping`。モデル空間は
/// 実寸、縮尺ごとに用紙のレイアウトのビューポート）。DXF・DWG はモデルをそのまま書き、JWW はモデルを JWW の図面に
/// （`mapping::dxf_to_jww`）、SXF はモデルを SXF の図面に写して書く（`mapping::dxf_to_sxf`。ビューポートは部分図、
/// ブロックは作図部品）
pub fn convert(bytes: &[u8], to: &str, options: &ReadDrawingOptions) -> Result<Converted, ReadError> {
    convert_with(bytes, to, options, &WriteOptions::default())
}

/// 書き出しの指定
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// 書く版。DXF は R12・2000・2004・2007・2010・2013・2018（`dxf::DxfVersion::parse` が読む名前）、SXF は
    /// 2.0・3.0・3.1（`sxf::SxfVersion::parse`）。DWG と JWW は選べない。省略すると形式の既定（DXF は 2013、SXF は 3.1）
    pub version: Option<String>,
}

/// 書く形式と版
enum Target {
    Dxf(dxf::DxfVersion),
    Dwg,
    Jww,
    Sxf(&'static str, sxf::SxfVersion),
}

impl Target {
    fn parse(to: &str, version: Option<&str>) -> Result<Target, ReadError> {
        let Some(&to) = WRITABLE_FORMATS.iter().find(|format| **format == to) else {
            let writable: Vec<String> = WRITABLE_FORMATS.iter().map(|format| format.to_uppercase()).collect();
            return Err(ReadError::new(format!(
                "{} には書き出せません（書き出せるのは {}）",
                to.to_uppercase(),
                writable.join("・")
            )));
        };
        Ok(match (to, version) {
            ("dxf", None) => Target::Dxf(dxf::DxfVersion::R2013),
            ("dxf", Some(name)) => Target::Dxf(dxf::DxfVersion::parse(name).ok_or_else(|| {
                ReadError::new(format!(
                    "DXF の版 {name} は書けません（書けるのは R12・2000・2004・2007・2010・2013・2018）"
                ))
            })?),
            ("sfc" | "p21" | "sfz" | "p2z", None) => Target::Sxf(to, sxf::SxfVersion::V3_1),
            ("sfc" | "p21" | "sfz" | "p2z", Some(name)) => Target::Sxf(
                to,
                sxf::SxfVersion::parse(name).ok_or_else(|| {
                    ReadError::new(format!("SXF の版 {name} は書けません（書けるのは 2.0・3.0・3.1）"))
                })?,
            ),
            ("dwg", None) => Target::Dwg,
            ("jww", None) => Target::Jww,
            (other, Some(_)) => {
                return Err(ReadError::new(format!("{} の版はまだ選べません", other.to_uppercase())));
            }
            _ => unreachable!(),
        })
    }
}

/// `convert` に書き出しの指定（版など）を加えたもの
pub fn convert_with(
    bytes: &[u8],
    to: &str,
    options: &ReadDrawingOptions,
    write: &WriteOptions,
) -> Result<Converted, ReadError> {
    let target = Target::parse(to, write.version.as_deref())?;
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
        let jww = match format {
            "dxf" => return Ok(dxf::parse_dxf(bytes)),
            "dwg" => return dwg::read_dwg(bytes).map_err(ReadError::new),
            "jww" => jww::read_jww(bytes)?,
            "jwc" => jww::read_jwc(bytes)?,
            // JWS（図形）は図形の内容を JWW の図面と同じに写す
            "jws" => jww::read_jws(bytes)?.content,
            format if sxf::SXF.formats().contains(&format) => {
                let mapped = mapping::sxf_to_dxf(&sxf::parse_sxf(bytes, format)?);
                notices = NoticeList::from_notices(&mapped.notices);
                return Ok(mapped.document);
            }
            other => return Err(ReadError::new(format!("{} の図面は書き直せません", other.to_uppercase()))),
        };
        let mapped = mapping::jww_to_dxf(&jww);
        notices = NoticeList::from_notices(&mapped.notices);
        Ok(mapped.document)
    });
    // 上限（バイト数・時間）を超えたら、途中まで読んだ図面は書かない
    if let Some(limit) = exceeded {
        return Err(limit_error(limit, limits));
    }
    let doc = doc?;
    let (written, exceeded) = zumen_core::limits::with_limits(limits, || -> Result<_, ReadError> {
        Ok(match target {
            Target::Dxf(version) => {
                let written = dxf::write_dxf_as(&doc, version);
                (written.bytes, written.notices)
            }
            Target::Dwg => {
                let written = dwg::write_dwg(&doc);
                (written.bytes, written.notices)
            }
            Target::Jww => {
                let mapped = mapping::dxf_to_jww(&doc);
                let written = mapped.write();
                (written.bytes, [mapped.notices, written.notices].concat())
            }
            Target::Sxf(format, version) => {
                let mapped = mapping::dxf_to_sxf(&doc);
                let written = sxf::write_sxf_version(&mapped.drawing, format, version)?;
                (written.bytes, [mapped.notices, written.notices].concat())
            }
        })
    });
    if let Some(limit) = exceeded {
        return Err(limit_error(limit, limits));
    }
    let (bytes, written) = written?;
    for notice in &written {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    Ok(Converted { bytes, notices: notices.into_vec() })
}

/// その場で編集できる形式
pub const EDITABLE_FORMATS: [&str; 7] = ["dxf", "dwg", "jww", "sfc", "p21", "sfz", "p2z"];

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
        "dwg" => dwg::edit_dwg(bytes, operations),
        "jww" => jww::edit_jww(bytes, operations),
        "sfc" | "p21" | "sfz" | "p2z" => sxf::edit_sxf(bytes, format, operations),
        other => Err(unsupported_edit(other)),
    }
}

/// 新しい図面を作り、操作（add・add-layer など）を当てる。to は形式（`EDITABLE_FORMATS`）、units は長さの単位
/// （mm、cm、m、inch、foot、unitless）。JWW と SXF の長さはいつも mm なので、mm 以外は受け付けない
pub fn create(to: &str, units: &str, operations: &[edit::EditOperation]) -> Result<edit::Edited, edit::EditError> {
    match to {
        "dxf" => dxf::edit_dxf(&dxf::new_dxf(units)?, operations),
        "dwg" => dwg::edit_dwg(&dwg::new_dwg(units)?, operations),
        "jww" => jww::edit_jww(&jww::new_jww(), operations_in_mm(to, units, operations)?),
        "sfc" | "p21" | "sfz" | "p2z" => {
            sxf::edit_sxf(&sxf::new_sxf(to)?, to, operations_in_mm(to, units, operations)?)
        }
        other => Err(unsupported_edit(other)),
    }
}

/// 長さが mm だけの形式で、units が mm か確かめる
fn operations_in_mm<'a>(
    to: &str,
    units: &str,
    operations: &'a [edit::EditOperation],
) -> Result<&'a [edit::EditOperation], edit::EditError> {
    if units.trim().eq_ignore_ascii_case("mm") {
        return Ok(operations);
    }
    Err(edit::EditError::new(
        "invalid-value",
        format!("{} の図面の長さは mm だけです（{units} は選べません）", to.to_uppercase()),
    ))
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
