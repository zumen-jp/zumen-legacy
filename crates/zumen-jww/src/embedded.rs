//! 同梱画像（版 700 以降）。ブロック定義の一覧の後ろに、件数（DWORD）と、画像ごとにファイルの名前（CString）・
//! 大きさ（DWORD）・ファイルのバイト列が並ぶ（jwdatafmt.txt「同梱画像ファイルの個数」「同梱画像ファイルのファイル名、
//! サイズ、実体」）。名前が .gz で終わるファイルは保存の直前に圧縮したもの（同じ説明。ezjww の読み取りと同じく gzip）。
//! 画像の配置の文字（^@BM）は、%temp% と .gz を除いた名前で指す。Jw_cad は開くときに一時フォルダへ展開する。

use crate::archive::Reader;
use crate::file::JwwFile;
use zumen_core::limits::{self, Limit};
use zumen_core::{NoticeLevel, NoticeList, ReadError};

/// 上限を指定しないときの、展開した後の大きさの上限（SXF の圧縮ファイルと同じ）
const DEFAULT_MAX_BYTES: u64 = 2 << 30;
/// 画像の配置の文字が同梱画像を指す前置き
const TEMP_PREFIX: &str = "%temp%";

/// JWW に同梱した画像のファイルを、画像の配置の文字が指す名前（`%temp%<名前>`）と展開した中身で返す。版 700 より前の
/// 図面は空。展開した大きさの合計は読み取りの上限（`zumen_core::limits`）、上限がなければ 2 GiB まで。途中で終わる
/// 一覧は読めたところまで、展開できない画像は飛ばして notices で知らせる
pub fn embedded_images(bytes: &[u8], notices: &mut NoticeList) -> Result<Vec<(String, Vec<u8>)>, ReadError> {
    let file = JwwFile::read(bytes)?;
    if file.version < 700 || file.trailer.is_empty() {
        return Ok(Vec::new());
    }
    let max = limits::max_bytes().unwrap_or(DEFAULT_MAX_BYTES);
    let mut reader = Reader::new(&file.trailer, 0);
    let truncated = |notices: &mut NoticeList| {
        notices.add(
            NoticeLevel::Warning,
            "embedded-truncated",
            "同梱画像の一覧が途中で終わっているため、読めた画像だけを返しました",
        );
    };
    let Ok(count) = reader.u32() else {
        truncated(notices);
        return Ok(Vec::new());
    };
    let mut total: u64 = 0;
    let mut out = Vec::new();
    for _ in 0..count {
        // 読み取りの上限（zumen_core::limits）
        if limits::tick() {
            break;
        }
        let entry = reader.cstring().and_then(|name| {
            let size = usize::try_from(reader.u32()?).map_err(|_| ReadError::new("大きさを読めません"))?;
            Ok((name, reader.take(size)?))
        });
        let Ok((stored, data)) = entry else {
            truncated(notices);
            break;
        };
        // 名前はフォルダを除く
        let stored = stored.rsplit(['\\', '/']).next().unwrap_or(&stored).to_string();
        let (name, content) = match stored.len().checked_sub(3).filter(|at| stored.is_char_boundary(*at)) {
            Some(at) if stored[at..].eq_ignore_ascii_case(".gz") => match gunzip(data, max) {
                Ok(content) => (stored[..at].to_string(), content),
                Err(Some(error)) => return Err(error),
                Err(None) => {
                    notices.add(
                        NoticeLevel::Warning,
                        "embedded-broken",
                        format!("同梱画像 {stored} を展開できないため、省きました"),
                    );
                    continue;
                }
            },
            _ => (stored, data.to_vec()),
        };
        total = total.saturating_add(content.len() as u64);
        if total > max {
            return Err(too_large(max));
        }
        out.push((format!("{TEMP_PREFIX}{name}"), content));
    }
    Ok(out)
}

fn too_large(max: u64) -> ReadError {
    limits::exceed(Limit::Bytes);
    ReadError::limit(Limit::Bytes, format!("同梱画像を展開すると上限の {max} バイトを超えるため、読みませんでした"))
}

/// gzip（RFC 1952）を展開する。上限を超えれば Some のエラー、壊れていれば None
fn gunzip(data: &[u8], max: u64) -> Result<Vec<u8>, Option<ReadError>> {
    const FHCRC: u8 = 0x02;
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;
    if data.len() < 18 || data[0] != 0x1f || data[1] != 0x8b || data[2] != 8 {
        return Err(None);
    }
    let flags = data[3];
    // RFC 1952, 2.3.1.2: a decompressor must reject nonzero reserved flag bits.
    if flags & 0xe0 != 0 {
        return Err(None);
    }
    let mut at = 10;
    if flags & FEXTRA != 0 {
        let length = usize::from(u16::from_le_bytes([*data.get(at).ok_or(None)?, *data.get(at + 1).ok_or(None)?]));
        at += 2 + length;
    }
    for flag in [FNAME, FCOMMENT] {
        if flags & flag != 0 {
            at += data.get(at..).ok_or(None)?.iter().position(|byte| *byte == 0).ok_or(None)? + 1;
        }
    }
    if flags & FHCRC != 0 {
        at += 2;
    }
    let body = data.get(at..data.len() - 8).ok_or(None)?;
    let limit = usize::try_from(max).unwrap_or(usize::MAX);
    let content = miniz_oxide::inflate::decompress_to_vec_with_limit(body, limit).map_err(|error| {
        if error.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput { Some(too_large(max)) } else { None }
    })?;
    let trailer = &data[data.len() - 8..];
    let crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    if crc32(&content) != crc || content.len() as u32 != size {
        return Err(None);
    }
    Ok(content)
}

/// CRC-32（gzip と同じ多項式 0xEDB88320）
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gzip（名前つき）にする
    fn gzip(content: &[u8]) -> Vec<u8> {
        let mut out = vec![0x1f, 0x8b, 8, 0x08, 0, 0, 0, 0, 0, 0xff];
        out.extend_from_slice(b"a.bmp\0");
        out.extend(miniz_oxide::deflate::compress_to_vec(content, 6));
        out.extend(crc32(content).to_le_bytes());
        out.extend((content.len() as u32).to_le_bytes());
        out
    }

    #[test]
    fn rejects_reserved_gzip_flags() {
        for reserved in [0x20, 0x40, 0x80] {
            let mut data = gzip(b"bitmap");
            data[3] |= reserved;
            assert!(matches!(gunzip(&data, 1024), Err(None)));
        }
    }

    #[test]
    fn gunzips_what_it_checks() {
        let content = b"BM image bytes".repeat(20);
        assert_eq!(gunzip(&gzip(&content), 1 << 20).unwrap(), content);
        let mut broken = gzip(&content);
        let at = broken.len() - 5;
        broken[at] ^= 1;
        assert!(matches!(gunzip(&broken, 1 << 20), Err(None)));
        assert!(matches!(gunzip(&gzip(&content), 10), Err(Some(_))));
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    /// 版 700 の図面（tests/fixtures/jww の images-v700.jww、同梱画像の件数 0 で終わる）に同梱画像を付ける
    fn with_images(images: &[(&str, &[u8])]) -> Vec<u8> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/jww/images-v700.jww");
        let mut bytes = std::fs::read(path).unwrap();
        bytes.truncate(bytes.len() - 4);
        bytes.extend((images.len() as u32).to_le_bytes());
        for (name, data) in images {
            // CString（UTF-16LE）
            bytes.extend([0xff, 0xfe, 0xff, name.chars().count() as u8]);
            for unit in name.encode_utf16() {
                bytes.extend(unit.to_le_bytes());
            }
            bytes.extend((data.len() as u32).to_le_bytes());
            bytes.extend_from_slice(data);
        }
        bytes
    }

    #[test]
    fn returns_embedded_images_by_the_name_that_places_them() {
        let bmp = b"BM\x46\0\0\0 a small bitmap".to_vec();
        let bytes = with_images(&[("photo.bmp.gz", &gzip(&bmp)), ("plain.jpg", b"jpeg bytes")]);
        let mut notices = NoticeList::new();
        let files = embedded_images(&bytes, &mut notices).unwrap();
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["%temp%photo.bmp", "%temp%plain.jpg"]);
        assert_eq!(files[0].1, bmp);
        assert_eq!(files[1].1, b"jpeg bytes");
        assert!(notices.into_vec().is_empty());
    }

    #[test]
    fn skips_broken_images_and_keeps_what_comes_before_a_truncated_list() {
        let mut bytes = with_images(&[("ok.png", b"png"), ("bad.bmp.gz", b"not gzip"), ("cut.png", b"0123456789")]);
        bytes.truncate(bytes.len() - 3);
        let mut notices = NoticeList::new();
        let files = embedded_images(&bytes, &mut notices).unwrap();
        assert_eq!(files, [("%temp%ok.png".to_string(), b"png".to_vec())]);
        let codes: Vec<String> = notices.into_vec().into_iter().map(|notice| notice.code).collect();
        assert_eq!(codes, ["embedded-broken", "embedded-truncated"]);
    }
}
