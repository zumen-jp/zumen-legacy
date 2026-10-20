//! 国交省の電子納品の SXF（SFC・P21、Ver.2.0〜3.1）を読み、zumen の表示用モデル（`Scene`）にする。
//! SXF の図面を SFC（Ver.3.1）に書く（`write_sfc`）。
//! `@zumenjs/sxf` の Rust 版で、同じ図面から同じ `Scene` を返す。圧縮した SXF（P2Z・SFZ、ZIP の書庫）も展開して読む。
#![forbid(unsafe_code)]

mod document;
mod features;
mod header;
mod p21;
mod p21_reader;
mod scene;
mod sfc;
mod sfc_reader;
mod style;
mod symbols;
pub mod tables;
mod write;
mod zip;

pub use document::{CompositeCurve, SubFigure, SubFigures, SxfDocument, SxfDrawing};
pub use features::{
    Annotation, ArcDimension, Arrow, CompositeCurveOrg, DrawingAttribute, DrawingSheet, ExtensionLine, FeatureKind,
    HatchPattern, LayerDefinition, Region, SfigOrg, Styled, SxfFeature, TextFont, TitleValue, UserColour, UserLineType,
    WidthDefinition,
};
pub use header::SxfHeader;
pub use p21::parse_p21;
pub use scene::{SxfSceneOptions, sxf_to_scene, sxf_to_scene_with};
pub use sfc::parse_sfc;
pub use style::DEFAULT_WIDTH;
pub use symbols::{arrow_primitives, marker_primitives};
pub use write::{SfcWritten, write_sfc, written_ids};

use zumen_core::{DrawingReader, Property, ReadError, ReadOptions, Scene};

/// 中身が SXF なら形式（sfc・p21、圧縮したものは sfz・p2z）を返す。SFC と P21 はどちらも ISO-10303-21 で始まり、
/// SFC はフィーチャを /*SXF のコメントに書く（FILE_DESCRIPTION が feature_mode のものも SFC）。
/// 圧縮したものは、名前が .p21・.sfc で終わるファイルを入れた ZIP の書庫
pub fn detect_sxf(bytes: &[u8]) -> Option<&'static str> {
    if zip::is_zip(bytes) {
        return zip::find_sxf(bytes).map(|member| if member.format == "sfc" { "sfz" } else { "p2z" });
    }
    let start = bytes.iter().position(|byte| !matches!(byte, b' ' | b'\r' | b'\n')).unwrap_or(bytes.len());
    if !bytes[start..].starts_with(b"ISO-10303-21") {
        return None;
    }
    let head = &bytes[..bytes.len().min(64 * 1024)];
    if head.windows(5).any(|window| window == b"/*SXF") || is_feature_mode(head) {
        return Some("sfc");
    }
    Some("p21")
}

/// `/FILE_DESCRIPTION\s*\(\s*\(\s*'SCADEC level\d feature_mode'/`（1 バイトを 1 文字として読む）
fn is_feature_mode(head: &[u8]) -> bool {
    // ISO 8859-1 の文字のうち、JavaScript の \s に当たるもの
    let skip = |mut i: usize| {
        while head.get(i).is_some_and(|byte| matches!(byte, 0x09..=0x0d | 0x20 | 0xa0)) {
            i += 1;
        }
        i
    };
    const NAME: &[u8] = b"FILE_DESCRIPTION";
    head.windows(NAME.len()).enumerate().filter(|(_, window)| *window == NAME).any(|(at, _)| {
        let mut i = skip(at + NAME.len());
        for _ in 0..2 {
            if head.get(i) != Some(&b'(') {
                return false;
            }
            i = skip(i + 1);
        }
        let rest = &head[i..];
        rest.starts_with(b"'SCADEC level")
            && rest.get(13).is_some_and(u8::is_ascii_digit)
            && rest[14..].starts_with(b" feature_mode'")
    })
}

/// 形式（sfc・p21、圧縮した sfz・p2z）ごとに SXF の図面を読む。表示用の図面にする前の、SXF のフィーチャの図面を返す
pub fn parse_sxf(bytes: &[u8], format: &str) -> Result<SxfDrawing, ReadError> {
    match format {
        "sfc" => parse_sfc(bytes),
        "p21" => parse_p21(bytes),
        "sfz" | "p2z" => {
            let (content, inner, _) = unzip_sxf(bytes)?;
            parse_sxf(&content, inner)
        }
        _ => Err(ReadError::new(format!("SXF の読み取りは {format} を読めません"))),
    }
}

/// 圧縮した SXF（P2Z・SFZ）の中の SXF を展開する。展開したバイト列、その形式（sfc・p21）、書庫の中の名前を返す
fn unzip_sxf(bytes: &[u8]) -> Result<(Vec<u8>, &'static str, String), ReadError> {
    let member =
        zip::find_sxf(bytes).ok_or_else(|| ReadError::new("圧縮ファイルの中に SXF（.p21・.sfc）がありません"))?;
    let content = zip::extract(bytes, &member)?;
    let inner = detect_sxf(&content).filter(|inner| matches!(*inner, "sfc" | "p21")).unwrap_or(member.format);
    Ok((content, inner, member.name))
}

/// read_drawing に渡す SXF（SFC・P21）の読み取り
pub struct Sxf;

/// SXF の読み取り（`@zumenjs/sxf` の `sxf`）
pub const SXF: Sxf = Sxf;

impl DrawingReader for Sxf {
    fn formats(&self) -> &'static [&'static str] {
        &["sfc", "p21", "sfz", "p2z"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        detect_sxf(bytes)
    }

    fn read(&self, bytes: &[u8], format: &str, _options: &ReadOptions) -> Result<Scene, ReadError> {
        if matches!(format, "sfz" | "p2z") {
            let (content, inner, name) = unzip_sxf(bytes)?;
            let mut scene = self.read(&content, inner, _options)?;
            // 形式の次に、展開した書庫と中のファイルの名前を示す
            let at = usize::from(!scene.metadata.is_empty());
            scene.metadata.insert(at, Property::new("圧縮", format!("{}（{}）", format.to_uppercase(), name)));
            return Ok(scene);
        }
        Ok(sxf_to_scene(&parse_sxf(bytes, format)?))
    }
}
