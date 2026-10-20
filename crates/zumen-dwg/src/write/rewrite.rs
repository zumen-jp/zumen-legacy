//! DWG を読み、オブジェクトをビット列のまま写して同じ版で書き直す。オブジェクトは元の並びのまま新しい
//! AcDb:AcDbObjects（R2000 はファイル）に並べ、オブジェクトの表（23）を作り直す。ヘッダ変数とクラスの節は中身を
//! 変えずに CRC を計算し直し、ほかの節（AuxHeader、SummaryInfo、Preview、AppInfo、FileDepList、RevHistory、
//! ObjFreeSpace、Template、AcDsPrototype_1b など）はそのまま写す

use super::container::{DataSection, Objects, PREVIEW_END, PREVIEW_START, Preview, R2000, R2004};
use super::database::has_extra;
use super::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL};
use super::{DwgWritten, container};
use crate::bits::TextCodec;
use crate::crc::{CRC8_SEED, crc8};
use crate::file::{self, MAX_SECTION_BYTES, R2004Container, Version, is_dwg};
use crate::header::{Header, read_header};
use crate::objects::{Format, object_length, read_object_map};
use std::collections::HashMap;
use zumen_core::{NoticeLevel, NoticeList, limits};

/// DWG を同じ版で書き直す。オブジェクトはビット列のまま写す（読み直して書くことはしない）。
/// 読めない・対応していない版の図面は、利用者に見せるメッセージを返す
pub fn rewrite_dwg(bytes: &[u8]) -> Result<DwgWritten, String> {
    if !is_dwg(bytes) {
        return Err("DWG の図面ではありません".into());
    }
    let name = String::from_utf8_lossy(&bytes[..6]).into_owned();
    let version = Version::from_magic(&bytes[..6]).ok_or_else(|| format!("対応していない DWG の版です（{name}）"))?;
    if bytes.len() < 0x100 {
        return Err("DWG のファイルの頭が途中で切れています".into());
    }
    let mut notices = NoticeList::new();
    let written = match version {
        Version::R13 | Version::R14 => {
            return Err(format!("{name}（AutoCAD R13・R14）の DWG は書き直せません。R2000 以降の DWG を書き直せます"));
        }
        Version::R2007 => return Err(format!("{name}（AutoCAD 2007）の DWG はまだ書き直せません")),
        Version::R2000 => rewrite_r2000(bytes, &mut notices)?,
        _ => rewrite_r2004(bytes, version, &mut notices)?,
    };
    Ok(DwgWritten { bytes: written, notices: notices.into_vec() })
}

/// 区切りで囲む節の CRC を計算し直す。区切りが読めなければ、そのまま写す
fn reframe(
    section: &[u8],
    start: &[u8; 16],
    end: &[u8; 16],
    extra: bool,
    trailing: usize,
    name: &str,
    notices: &mut NoticeList,
) -> Vec<u8> {
    match Framed::parse(section, start, end, extra) {
        Some(framed) => {
            if !framed.crc_ok {
                notices.add(
                    NoticeLevel::Warning,
                    "dwg-crc",
                    format!("DWG の節 {name} の CRC が中身と合わないため、中身から計算し直しました"),
                );
            }
            framed.bytes(start, end, trailing)
        }
        None => {
            notices.add(
                NoticeLevel::Warning,
                "dwg-section-copied",
                format!("DWG の節 {name} の区切りが読めないため、そのまま写しました"),
            );
            section.to_vec()
        }
    }
}

/// オブジェクトの表が指すオブジェクトを、元の並び（位置の順）のまま写す。同じ位置を指すハンドルは 1 つの
/// オブジェクトを指したままにする。範囲の外のオブジェクトは写さず、CRC の合わないオブジェクトはそのまま写して数える
fn copy_objects(objects: &[u8], map: &HashMap<u64, usize>, r2010: bool, notices: &mut NoticeList) -> Objects {
    let mut entries: Vec<(usize, u64)> = map.iter().map(|(handle, offset)| (*offset, *handle)).collect();
    entries.sort_unstable();
    let mut out = Objects::default();
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let (mut broken, mut bad_crc) = (0u64, 0u64);
    for (offset, handle) in entries {
        if limits::tick() {
            break;
        }
        if let Some(&index) = index_of.get(&offset) {
            out.handles.push((handle, index));
            continue;
        }
        let Some(length) = object_length(objects, offset, r2010) else {
            broken += 1;
            continue;
        };
        let bytes = &objects[offset..offset + length];
        let stored = u16::from_le_bytes([bytes[length - 2], bytes[length - 1]]);
        if crc8(CRC8_SEED, &bytes[..length - 2]) != stored {
            bad_crc += 1;
        }
        index_of.insert(offset, out.items.len());
        out.handles.push((handle, out.items.len()));
        out.items.push(bytes.to_vec());
    }
    if broken > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-broken-object",
            "DWG のオブジェクトの表が指す位置にオブジェクトがないため、写しませんでした",
            broken,
        );
    }
    if bad_crc > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-object-crc",
            "DWG のオブジェクトの CRC が中身と合いません（そのまま写しました）",
            bad_crc,
        );
    }
    out
}

/// ヘッダ変数（2 つ目のファイルの頭に書くハンドルを取る）。読めなければ None
fn header_variables(section: &[u8], version: Version, maintenance: u8) -> Option<Header> {
    let format = Format { version, maintenance, codec: TextCodec::Bytes(encoding_rs::WINDOWS_1252) };
    read_header(section, format).ok()
}

// --- R2000 ------------------------------------------------------------------

fn rewrite_r2000(bytes: &[u8], notices: &mut NoticeList) -> Result<Vec<u8>, String> {
    let records = file::r2000_records(bytes)?;
    let slice = |number: u8| file::r2000_slice(bytes, &records, number);
    let missing = |name: &str| format!("DWG の節（{name}）が見つかりません");
    let header_section = slice(0).ok_or_else(|| missing("ヘッダ変数"))?;
    let classes_section = slice(1).ok_or_else(|| missing("クラス"))?;
    let map_section = slice(2).ok_or_else(|| missing("オブジェクトの表"))?;
    let maintenance = bytes[0x0b];
    let map = read_object_map(map_section);
    if map.is_empty() {
        return Err("DWG のオブジェクトの表が読めません".into());
    }
    let header = header_variables(header_section, Version::R2000, maintenance)
        .ok_or_else(|| "DWG のヘッダ変数が読めません".to_string())?;
    let mut head = [0u8; 0x15];
    head.copy_from_slice(&bytes[..0x15]);
    let file = R2000 {
        head,
        aux_header: slice(5).map(<[u8]>::to_vec),
        preview: r2000_preview(bytes).unwrap_or_else(Preview::empty),
        header: reframe(header_section, &HEADER_SENTINEL, &HEADER_END, false, 0, "AcDb:Header", notices),
        classes: reframe(classes_section, &CLASSES_SENTINEL, &CLASSES_END, false, 0, "AcDb:Classes", notices),
        objects: copy_objects(bytes, &map, false, notices),
        template: slice(4).map(<[u8]>::to_vec),
        handles: [
            header.handseed,
            header.block_control,
            header.layer_control,
            header.style_control,
            header.linetype_control,
            header.view_control,
            header.ucs_control,
            header.vport_control,
            header.appid_control,
            header.dimstyle_control,
            header.vpent_control,
            header.named_objects,
            header.mlinestyle_dictionary,
            header.groups,
        ],
    };
    Ok(container::write_r2000(&file))
}

/// R2000 のプレビュー（14.2）。ファイルの頭の 0x0D の位置から、区切り・大きさ・中身・区切り
fn r2000_preview(bytes: &[u8]) -> Option<Preview> {
    let at = u32::from_le_bytes(bytes.get(0x0d..0x11)?.try_into().ok()?) as usize;
    let data = bytes.get(at..)?;
    if !data.starts_with(&PREVIEW_START) {
        return None;
    }
    let size = u32::from_le_bytes(data.get(16..20)?.try_into().ok()?) as usize;
    let end = 20usize.checked_add(size)?;
    if data.get(end..end + 16)? != PREVIEW_END {
        return None;
    }
    Some(Preview { data: data[..end + 16].to_vec(), base: at as u64 })
}

// --- R2004・R2010〜R2018 ------------------------------------------------------

fn rewrite_r2004(bytes: &[u8], version: Version, notices: &mut NoticeList) -> Result<Vec<u8>, String> {
    // 4.1 の 0x18: 1 はデータの節の暗号化（パスワードの付いた図面）。中身を読めないので書き直さない
    let security = u32::from_le_bytes(bytes[0x18..0x1c].try_into().unwrap_or_default());
    if security & 1 != 0 {
        return Err("パスワードで暗号化した DWG は書き直せません".into());
    }
    let container = R2004Container::open(bytes)?;
    let maintenance = bytes[0x0b];
    let extra = has_extra(version, maintenance);
    let r2010 = version >= Version::R2010;
    let mut sections = Vec::with_capacity(container.sections.len());
    let mut addresses = Vec::with_capacity(container.sections.len());
    let mut objects_at = None;
    let mut handles_at = None;
    for info in &container.sections {
        if info.size >= MAX_SECTION_BYTES as u64 {
            return Err(format!("DWG の節 {} が大きすぎるため、書き直せません", info.name));
        }
        // 4.5 Max Decompressed Size is input data too. The reader bounds it before allocating;
        // reject an oversized declaration here before the writer pads a page to that size.
        if info.max_size > MAX_SECTION_BYTES {
            return Err(format!("DWG の節 {} のページが大きすぎるため、書き直せません", info.name));
        }
        let mut data = container.data(info, notices);
        // 0 だけのページは書かれていない（4.5）。ページのない節も、節の大きさの 0 として写す
        data.resize(info.size as usize, 0);
        let data = match info.name.as_str() {
            "AcDb:Header" => reframe(&data, &HEADER_SENTINEL, &HEADER_END, extra, 0, &info.name, notices),
            // R2004 以降のクラスの節は、終わりの区切りの後に 8 バイトの 0 が続く（10.1）
            "AcDb:Classes" => reframe(&data, &CLASSES_SENTINEL, &CLASSES_END, extra, 8, &info.name, notices),
            "AcDb:AcDbObjects" => {
                objects_at = Some(sections.len());
                data
            }
            "AcDb:Handles" => {
                handles_at = Some(sections.len());
                data
            }
            _ => data,
        };
        addresses.push(container.address(info));
        let preview_base =
            (info.name == "AcDb:Preview").then(|| container.address(info).map_or(0, |at| at as u64 + 0x20));
        sections.push(DataSection {
            name: info.name.clone(),
            data,
            page_size: info.max_size,
            compressed: info.compressed,
            encrypted: info.encrypted,
            preview_base,
        });
    }
    let (Some(objects_at), Some(handles_at)) = (objects_at, handles_at) else {
        return Err("DWG の節（AcDb:AcDbObjects・AcDb:Handles）が見つかりません".into());
    };
    let map = read_object_map(&sections[handles_at].data);
    if map.is_empty() {
        return Err("DWG のオブジェクトの表が読めません".into());
    }
    // オブジェクトを並べ直し、表を作り直す。AcDb:AcDbObjects の中の位置は 0x0DCA の RL の 4 バイトから
    let objects = copy_objects(&sections[objects_at].data, &map, r2010, notices);
    let (stream, locations) = objects.lay_out(0);
    sections[objects_at].data = stream;
    sections[handles_at].data = super::sections::object_map(&locations);

    // ファイルの中の順は元の図面と同じ（ページのない節は終わりに）
    let mut stream_order: Vec<usize> = (0..sections.len()).collect();
    stream_order.sort_by_key(|&index| addresses[index].unwrap_or(usize::MAX));
    let mut head = [0u8; 0x80];
    head.copy_from_slice(&bytes[..0x80]);
    Ok(container::write_r2004(&R2004 { head, sections, stream_order }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unbounded_page_size_is_refused_before_rewriting() {
        let mut head = [0; 0x80];
        head[..6].copy_from_slice(b"AC1032");
        // An empty section lets us build a malformed section map without allocating its declared page.
        let bytes = container::write_r2004(&R2004 {
            head,
            sections: vec![DataSection {
                name: "AcDb:Header".into(),
                data: Vec::new(),
                page_size: u32::MAX as usize,
                compressed: false,
                encrypted: 0,
                preview_base: None,
            }],
            stream_order: vec![0],
        });
        let error = rewrite_dwg(&bytes).unwrap_err();
        assert!(error.contains("ページが大きすぎる"), "{error}");
    }
}
