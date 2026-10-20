//! 国交省の電子納品の SXF（SFC・P21、Ver.2.0〜3.1）を読み、zumen の表示用モデル（`Scene`）にする。
//! SXF の図面を SFC（Ver.3.1）に書く（`write_sfc`）。
//! `@zumenjs/sxf` の Rust 版で、同じ図面から同じ `Scene` を返す。圧縮した SXF（P2Z・SFZ、ZIP の書庫）も展開して読む。
#![forbid(unsafe_code)]

mod document;
mod features;
mod header;
mod p21;
mod p21_edit;
mod p21_reader;
mod p21_writer;
mod scene;
mod sfc;
mod sfc_edit;
mod sfc_reader;
mod source;
mod splice;
mod step;
mod style;
mod symbols;
pub mod tables;
mod version;
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
pub use source::{SxfArchive, SxfSource};
pub use style::DEFAULT_WIDTH;
pub use symbols::{arrow_primitives, marker_primitives};
pub use version::SxfVersion;
pub use write::{SxfWritten, written_ids};

/// SXF の図面を SFC ファイルに書く。SFC から読んだ図面（`source` があるもの）は、元のファイルを書き換えて、
/// 変えたフィーチャだけを書き直す（変えていないフィーチャは元の文字列のまま）。ほかの図面は、フィーチャを定義の順に
/// 並べ、番号を 10 から振って、図面の版（`SxfDrawing::version`、分からなければ Ver.3.1）で新しく書く
pub fn write_sfc(drawing: &SxfDrawing) -> SxfWritten {
    write_sfc_as(drawing, None)
}

/// SFC に書く。version を選べば、読んだ図面も新しく書く
fn write_sfc_as(drawing: &SxfDrawing, version: Option<SxfVersion>) -> SxfWritten {
    match (&drawing.source, version) {
        (Some(source), None) if drawing.format == "sfc" => sfc_edit::write_sfc_edit(drawing, &source.bytes),
        _ => write::write_sfc_new(drawing, version.unwrap_or_else(|| SxfVersion::written(drawing))),
    }
}

/// SXF の図面を P21 ファイル（STEP AP202 のサブセット）に書く。P21 から読んだ図面（`source` があるもの）は、
/// 元のファイルを書き換えて、変えたフィーチャのインスタンスだけを書き直す。ほかの図面は、フィーチャごとの
/// インスタンスを AP202 サブセット編のテンプレートで組み立て、図面の版（分からなければ Ver.3.1）で新しく書く
pub fn write_p21(drawing: &SxfDrawing) -> SxfWritten {
    write_p21_as(drawing, None)
}

/// P21 に書く。version を選べば、読んだ図面も新しく書く
fn write_p21_as(drawing: &SxfDrawing, version: Option<SxfVersion>) -> SxfWritten {
    match (&drawing.source, version) {
        (Some(source), None) if drawing.format == "p21" => p21_edit::write_p21_edit(drawing, &source.bytes),
        _ => p21_writer::write_p21_new(drawing, version.unwrap_or_else(|| SxfVersion::written(drawing))),
    }
}

/// SXF の図面を、版を選んで format（sfc・p21・sfz・p2z）で書く。図面の版と同じなら `write_sxf` と同じ（読んだ
/// 図面は元のファイルを書き換える）。違う版は新しく書き、その版にないフィーチャを近い形にする（Ver.3.0 より前は
/// 図面表題欄を除き、Ver.3.1 より前はクロソイド曲線を折線に、弧長寸法を角度寸法にして、notices で知らせる）
pub fn write_sxf_version(drawing: &SxfDrawing, format: &str, version: SxfVersion) -> Result<SxfWritten, ReadError> {
    let keep =
        drawing.source.is_some() && SxfVersion::of(drawing) == Some(version) && SxfVersion::written(drawing) <= version;
    write_sxf_as(drawing, format, (!keep).then_some(version))
}

/// SXF の図面を format（sfc・p21、圧縮した sfz・p2z）で書く。読んだ図面を同じ形式に書くときは、変えていない
/// フィーチャ・インスタンスを元の文字列のまま書く。圧縮した SXF から読んだ図面を同じ形式に書くときは、元の書庫の
/// SXF だけを置き換え、同じ書庫のほかのファイル（属性ファイル・画像）と名前・日時を残す
pub fn write_sxf(drawing: &SxfDrawing, format: &str) -> Result<SxfWritten, ReadError> {
    write_sxf_as(drawing, format, None)
}

/// format に書く。version を選べば、図面の内容を新しく書く。元の書庫があれば付属ファイルは残す
fn write_sxf_as(drawing: &SxfDrawing, format: &str, version: Option<SxfVersion>) -> Result<SxfWritten, ReadError> {
    let inner = match format {
        "sfc" => return Ok(write_sfc_as(drawing, version)),
        "p21" => return Ok(write_p21_as(drawing, version)),
        "sfz" => "sfc",
        "p2z" => "p21",
        _ => return Err(ReadError::new(format!("SXF は {format} に書けません（書けるのは SFC・P21・SFZ・P2Z）"))),
    };
    let mut written = if inner == "sfc" { write_sfc_as(drawing, version) } else { write_p21_as(drawing, version) };
    let source = drawing.source.as_ref().filter(|_| drawing.format == inner);
    if let Some((source, archive)) = source.and_then(|source| Some((source, source.archive.as_ref()?))) {
        if *written.bytes == *source.bytes {
            written.bytes = archive.bytes.to_vec();
            return Ok(written);
        }
        if let Some(bytes) = zip::replace_sxf(&archive.bytes, &written.bytes) {
            written.bytes = bytes;
            return Ok(written);
        }
        written.notices.push(zumen_core::Notice {
            level: zumen_core::NoticeLevel::Warning,
            code: "archive".into(),
            message:
                "元の書庫を読み直せないため、SXF だけを入れた書庫を新しく作りました（ほかのファイルは残していません）"
                    .into(),
            count: 1,
        });
    }
    let name = archive_member_name(drawing, inner);
    written.bytes = zip::archive(&name, &written.bytes)
        .ok_or_else(|| ReadError::new("4 GiB を超える SXF は圧縮できません（ZIP64 には対応していません）"))?;
    Ok(written)
}

/// 新しく作る書庫の中の SXF の名前。図面のファイル名の拡張子を、中の形式（拡張子が大文字なら大文字）にする
fn archive_member_name(drawing: &SxfDrawing, inner: &str) -> String {
    let name = match &drawing.source.as_ref().and_then(|source| source.archive.as_ref()) {
        Some(archive) => archive.name.clone(),
        None => drawing.header.file_name.clone(),
    };
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => {
            let upper = extension.chars().any(|c| c.is_ascii_uppercase());
            format!("{stem}.{}", if upper { inner.to_ascii_uppercase() } else { inner.to_string() })
        }
        _ if !name.is_empty() => format!("{name}.{inner}"),
        _ => format!("drawing.{inner}"),
    }
}

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
            let (content, inner, name) = unzip_sxf(bytes)?;
            let mut drawing = parse_sxf(&content, inner)?;
            // 書き直すとき同じ書庫に入れ直せるよう、元の書庫を持つ
            if let Some(source) = &mut drawing.source {
                source.archive = Some(SxfArchive { bytes: std::sync::Arc::from(bytes), name });
            }
            Ok(drawing)
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
            scene
                .metadata
                .insert(at, Property::new("archive", "圧縮", format!("{}（{}）", format.to_uppercase(), name)));
            return Ok(scene);
        }
        Ok(sxf_to_scene(&parse_sxf(bytes, format)?))
    }
}
