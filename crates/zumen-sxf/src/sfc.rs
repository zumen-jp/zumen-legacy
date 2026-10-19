//! SFC ファイルを読み、フィーチャを並び順のとおりに組み立てる（`@zumenjs/sxf` の sfc.ts）

use crate::document::{SxfDrawing, assemble_document};
use crate::features::{InvalidFeature, feature_from_record};
use crate::sfc_reader::read_sfc;
use zumen_core::{NoticeLevel, NoticeList, ReadError, js};

/// 出力ライブラリ名の末尾の `$$3.1` などの版（`/\$\$(\d+(?:\.\d+)?)\s*$/`）
pub(crate) fn suffix_version(preprocessor: &str) -> Option<&str> {
    let bytes = preprocessor.as_bytes();
    let mut from = 0;
    while let Some(offset) = preprocessor[from..].find("$$") {
        let start = from + offset + 2;
        let digits = |at: usize| bytes[at..].iter().take_while(|byte| byte.is_ascii_digit()).count();
        let integer = digits(start);
        if integer > 0 {
            let mut end = start + integer;
            if bytes.get(end) == Some(&b'.') {
                let fraction = digits(end + 1);
                if fraction > 0 {
                    end += 1 + fraction;
                }
            }
            if preprocessor[end..].chars().all(js::is_whitespace) {
                return Some(&preprocessor[start..end]);
            }
        }
        from = from + offset + 1;
    }
    None
}

/// SXF のファイルの版。出力ライブラリ名の末尾（$$3.1 など）か、フィーチャの囲み（/*SXF3.1）から決める
fn sxf_version(preprocessor: &str, record_versions: &[&str]) -> String {
    if let Some(suffix) = suffix_version(preprocessor) {
        return format!("SXF {suffix}");
    }
    let mut highest = "2.0";
    for version in record_versions {
        if *version == "3.1" {
            highest = "3.1";
        } else if *version == "3" && highest != "3.1" {
            highest = "3.0";
        }
    }
    format!("SXF {highest}")
}

/// SFC ファイルのバイト列を読み、フィーチャを並び順のとおりに組み立てる
pub fn parse_sfc(bytes: &[u8]) -> Result<SxfDrawing, ReadError> {
    let mut notices = NoticeList::new();
    let read = read_sfc(bytes);
    if read.skipped > 0 {
        notices.add_count(NoticeLevel::Warning, "unreadable", "書式を読めないフィーチャを省きました", read.skipped);
    }
    let mut features = Vec::new();
    let mut versions: Vec<&str> = Vec::new();
    for record in &read.records {
        if !versions.contains(&record.version) {
            versions.push(record.version);
        }
        match feature_from_record(record) {
            Ok(Some(feature)) => features.push(feature),
            Ok(None) => notices.add(NoticeLevel::Warning, "unknown-feature", "対応していないフィーチャを省きました"),
            Err(InvalidFeature) => {
                notices.add(NoticeLevel::Warning, "invalid-feature", "引数が足りない・数値でないフィーチャを省きました")
            }
        }
    }
    if read.records.is_empty() {
        return Err(ReadError::new("SFC のフィーチャが見つかりません"));
    }
    let document = assemble_document(features, &mut notices);
    Ok(SxfDrawing {
        format: "sfc",
        version: sxf_version(&read.header.preprocessor, &versions),
        header: read.header,
        document,
        notices: notices.into_vec(),
    })
}
