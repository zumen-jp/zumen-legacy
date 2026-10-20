//! 圧縮した SXF（P2Z・SFZ）。ZIP の書庫に P21・SFC のファイルが 1 つ入っている。
//! 中央ディレクトリから、名前が .p21・.sfc で終わる最初のファイルを探して展開する（APPNOTE 4.3.7・4.3.12・4.3.16）。
//! 展開した後の大きさは読み取りの上限（`zumen_core::limits`）まで、上限がなければ 2 GiB までにする（ZIP 爆弾）。

use zumen_core::ReadError;
use zumen_core::limits::{self, Limit};

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL: u32 = 0x0605_4b50;
/// 上限を指定しないときの、展開した後の大きさの上限
const DEFAULT_MAX_BYTES: u64 = 2 << 30;

/// 書庫の中の SXF のファイル
pub(crate) struct Member {
    /// 中身の形式（p21・sfc）
    pub format: &'static str,
    /// 書庫の中の名前
    pub name: String,
    method: u16,
    flags: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    local: u64,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// ZIP の書庫か（先頭がローカルファイルヘッダ）
pub(crate) fn is_zip(bytes: &[u8]) -> bool {
    u32_at(bytes, 0) == Some(LOCAL_HEADER)
}

/// 書庫の中の、名前が .p21・.sfc で終わる最初のファイル。書庫でないか、SXF がなければ None
pub(crate) fn find_sxf(bytes: &[u8]) -> Option<Member> {
    if !is_zip(bytes) {
        return None;
    }
    // 中央ディレクトリの終わり（後ろの 22 バイト以降、コメントは 65535 バイトまで）
    let earliest = bytes.len().saturating_sub(22 + 65_535);
    let end = (earliest..=bytes.len().checked_sub(22)?).rev().find(|&at| u32_at(bytes, at) == Some(END_OF_CENTRAL))?;
    let count = u16_at(bytes, end + 10)?;
    let mut at = u32_at(bytes, end + 16)? as usize;
    for _ in 0..count {
        if u32_at(bytes, at)? != CENTRAL_HEADER {
            return None;
        }
        let flags = u16_at(bytes, at + 8)?;
        let method = u16_at(bytes, at + 10)?;
        let crc = u32_at(bytes, at + 16)?;
        let compressed = u32_at(bytes, at + 20)?;
        let size = u32_at(bytes, at + 24)?;
        let name_length = usize::from(u16_at(bytes, at + 28)?);
        let extra_length = usize::from(u16_at(bytes, at + 30)?);
        let comment_length = usize::from(u16_at(bytes, at + 32)?);
        let local = u32_at(bytes, at + 42)?;
        let raw_name = bytes.get(at + 46..at + 46 + name_length)?;
        at += 46 + name_length + extra_length + comment_length;
        let lower: Vec<u8> = raw_name.to_ascii_lowercase();
        let format = if lower.ends_with(b".p21") {
            "p21"
        } else if lower.ends_with(b".sfc") {
            "sfc"
        } else {
            continue;
        };
        // 名前は UTF-8 の印（汎用フラグの 11 ビット目）がなければ Shift_JIS
        let name = if flags & 0x800 != 0 {
            String::from_utf8_lossy(raw_name).into_owned()
        } else {
            encoding_rs::SHIFT_JIS.decode_without_bom_handling(raw_name).0.into_owned()
        };
        return Some(Member {
            format,
            name,
            method,
            flags,
            crc,
            compressed: u64::from(compressed),
            size: u64::from(size),
            local: u64::from(local),
        });
    }
    None
}

/// 書庫の中の SXF を展開する
pub(crate) fn extract(bytes: &[u8], member: &Member) -> Result<Vec<u8>, ReadError> {
    if member.flags & 0x1 != 0 {
        return Err(ReadError::new("暗号化した圧縮ファイルは読めません"));
    }
    if member.compressed == u64::from(u32::MAX) || member.size == u64::from(u32::MAX) {
        return Err(ReadError::new("4 GiB を超える圧縮ファイル（ZIP64）には対応していません"));
    }
    let max = limits::max_bytes().unwrap_or(DEFAULT_MAX_BYTES);
    let too_large = || {
        limits::exceed(Limit::Bytes);
        ReadError::limit(
            Limit::Bytes,
            format!("圧縮ファイルを展開すると上限の {max} バイトを超えるため、読みませんでした"),
        )
    };
    if member.size > max {
        return Err(too_large());
    }
    let broken = || ReadError::new("圧縮ファイルが壊れています");
    let local = usize::try_from(member.local).map_err(|_| broken())?;
    if u32_at(bytes, local) != Some(LOCAL_HEADER) {
        return Err(broken());
    }
    let start = local
        + 30
        + usize::from(u16_at(bytes, local + 26).ok_or_else(broken)?)
        + usize::from(u16_at(bytes, local + 28).ok_or_else(broken)?);
    let length = usize::try_from(member.compressed).map_err(|_| broken())?;
    let data = bytes.get(start..start.checked_add(length).ok_or_else(broken)?).ok_or_else(broken)?;
    let content = match member.method {
        0 => data.to_vec(),
        8 => {
            let limit = usize::try_from(max).unwrap_or(usize::MAX);
            miniz_oxide::inflate::decompress_to_vec_with_limit(data, limit).map_err(|error| {
                if error.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput { too_large() } else { broken() }
            })?
        }
        method => return Err(ReadError::new(format!("圧縮ファイルの圧縮方式（{method}）に対応していません"))),
    };
    if content.len() as u64 > max {
        return Err(too_large());
    }
    if crc32(&content) != member.crc {
        return Err(broken());
    }
    Ok(content)
}

/// CRC-32（ZIP と同じ多項式 0xEDB88320）
fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut value = i as u32;
            let mut bit = 0;
            while bit < 8 {
                value = if value & 1 != 0 { 0xEDB8_8320 ^ (value >> 1) } else { value >> 1 };
                bit += 1;
            }
            table[i] = value;
            i += 1;
        }
        table
    };
    !data.iter().fold(!0u32, |crc, byte| TABLE[((crc ^ u32::from(*byte)) & 0xff) as usize] ^ (crc >> 8))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zumen_core::ReadLimits;

    /// 1 つのファイルを入れた ZIP（method 0 は無圧縮、8 は deflate）
    pub(crate) fn zip(name: &[u8], content: &[u8], method: u16, flags: u16) -> Vec<u8> {
        let data = if method == 8 { miniz_oxide::deflate::compress_to_vec(content, 6) } else { content.to_vec() };
        let crc = crc32(content);
        let mut out = Vec::new();
        let header = |out: &mut Vec<u8>, signature: u32, central: bool| {
            out.extend(signature.to_le_bytes());
            if central {
                out.extend(20u16.to_le_bytes());
            }
            out.extend(20u16.to_le_bytes());
            out.extend(flags.to_le_bytes());
            out.extend(method.to_le_bytes());
            out.extend([0u8; 4]);
            out.extend(crc.to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((content.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
        };
        header(&mut out, LOCAL_HEADER, false);
        out.extend(name);
        out.extend(&data);
        let central = out.len();
        header(&mut out, CENTRAL_HEADER, true);
        // コメントの長さ・開始ディスク・内部属性・外部属性、ローカルファイルヘッダの位置（0）
        out.extend([0u8; 10]);
        out.extend(0u32.to_le_bytes());
        out.extend(name);
        let size = out.len() - central;
        out.extend(END_OF_CENTRAL.to_le_bytes());
        out.extend([0u8; 4]);
        out.extend(1u16.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend((size as u32).to_le_bytes());
        out.extend((central as u32).to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out
    }

    #[test]
    fn finds_and_extracts_the_sxf_in_an_archive() {
        let content = b"ISO-10303-21;\nDATA;\nENDSEC;\n".repeat(100);
        for method in [0, 8] {
            let archive = zip(b"DRAWING/D0001.P21", &content, method, 0);
            let member = find_sxf(&archive).unwrap();
            assert_eq!((member.format, member.name.as_str()), ("p21", "DRAWING/D0001.P21"));
            assert_eq!(extract(&archive, &member).unwrap(), content);
        }
        // 名前は Shift_JIS、UTF-8 の印があれば UTF-8
        let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("平面図.sfc");
        assert_eq!(find_sxf(&zip(&sjis, b"x", 0, 0)).unwrap().name, "平面図.sfc");
        assert_eq!(find_sxf(&zip("平面図.SFC".as_bytes(), b"x", 0, 0x800)).unwrap().format, "sfc");
        assert!(find_sxf(&zip(b"readme.txt", b"x", 0, 0)).is_none());
        assert!(find_sxf(b"ISO-10303-21;").is_none());
    }

    #[test]
    fn stops_at_the_byte_limit_and_rejects_broken_archives() {
        let bomb = zip(b"bomb.p21", &vec![b' '; 1 << 20], 8, 0);
        assert!(bomb.len() < 10_000);
        let member = find_sxf(&bomb).unwrap();
        let limits = ReadLimits { max_bytes: Some(1000), ..ReadLimits::default() };
        let (result, exceeded) = limits::with_limits(&limits, || extract(&bomb, &member));
        assert_eq!((result.unwrap_err().exceeded(), exceeded), (Some(Limit::Bytes), Some(Limit::Bytes)));
        // 宣言した大きさが嘘でも、展開しながら止める
        let mut lying = bomb.clone();
        let central = lying.len() - 22 - (46 + 8);
        lying[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
        let member = find_sxf(&lying).unwrap();
        let (result, _) = limits::with_limits(&limits, || extract(&lying, &member));
        assert_eq!(result.unwrap_err().exceeded(), Some(Limit::Bytes));
        // CRC が合わない、暗号化した
        let mut broken = zip(b"a.p21", b"ISO-10303-21;", 0, 0);
        let last = broken.len() - 22 - (46 + 5) - 1;
        broken[last] ^= 0xff;
        let member = find_sxf(&broken).unwrap();
        assert_eq!(extract(&broken, &member).unwrap_err().message(), "圧縮ファイルが壊れています");
        let encrypted = zip(b"a.p21", b"x", 0, 1);
        assert!(extract(&encrypted, &find_sxf(&encrypted).unwrap()).is_err());
    }
}
