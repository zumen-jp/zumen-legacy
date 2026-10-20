//! DWG を読み、オブジェクトをビット列のまま写して同じ版で書き直す。オブジェクトは元の並びのまま新しい
//! AcDb:AcDbObjects（R2000 はファイル）に並べ、オブジェクトの表（23）を作り直す。ヘッダ変数とクラスの節は中身を
//! 変えずに CRC を計算し直し、ほかの節（AuxHeader、SummaryInfo、Preview、AppInfo、FileDepList、RevHistory、
//! ObjFreeSpace、Template、AcDsPrototype_1b など）はそのまま写す。
//!
//! 開いた図面（Source）は、その場の編集（crate::edit）も使う: 編集はオブジェクトとヘッダ変数を変えてから書く

use super::container::{DataSection, Objects, PREVIEW_END, PREVIEW_START, Preview, R2000, R2004};
use super::container21::{DataSection21, R2007, write_r2007};
use super::database::has_extra;
use super::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL};
use super::{DwgWritten, container};
use crate::bits::TextCodec;
use crate::crc::{CRC8_SEED, crc8};
use crate::file::{self, MAX_SECTION_BYTES, R2004Container, R2007Container, Version, is_dwg, r21};
use crate::header::{Header, read_header};
use crate::objects::{Format, object_length, read_object_map};
use std::collections::HashMap;
use zumen_core::{NoticeLevel, NoticeList, limits};

/// DWG を同じ版で書き直す。オブジェクトはビット列のまま写す（読み直して書くことはしない）。
/// 読めない・対応していない版の図面は、利用者に見せるメッセージを返す
pub fn rewrite_dwg(bytes: &[u8]) -> Result<DwgWritten, String> {
    let mut notices = NoticeList::new();
    let source = Source::open(bytes, &mut notices)?;
    Ok(DwgWritten { bytes: source.write(), notices: notices.into_vec() })
}

/// 開いた図面: 版の入れ物の値、ヘッダ変数・クラスの節（区切りから区切りまで）、オブジェクト
pub(crate) struct Source {
    pub version: Version,
    pub maintenance: u8,
    /// 図面の文字コードの番号（ファイルの頭の 0x13）
    pub codepage: u16,
    /// ヘッダ変数の節（区切りから区切りまで。区切りが読めなければ元のバイト列）
    pub header: Vec<u8>,
    /// クラスの節（同じ。R2004 以降は終わりの区切りの後の 8 バイトの 0 まで）
    pub classes: Vec<u8>,
    pub objects: Objects,
    container: Container,
}

/// 版ごとの入れ物の値
enum Container {
    R2000 { head: [u8; 0x15], aux_header: Option<Vec<u8>>, preview: Preview, template: Option<Vec<u8>> },
    R2004 { head: [u8; 0x80], sections: Vec<DataSection>, stream_order: Vec<usize>, slots: Slots },
    R2007 { head: [u8; 0x80], sections: Vec<DataSection21>, stream_order: Vec<usize>, random_seed: u64, slots: Slots },
}

/// 節の並びの中の、書くときに中身を入れる節の位置
#[derive(Debug, Clone, Copy, Default)]
struct Slots {
    header: Option<usize>,
    classes: Option<usize>,
    objects: usize,
    handles: usize,
    aux: Option<usize>,
}

impl Source {
    /// DWG を開く。読めない・対応していない版の図面は、利用者に見せるメッセージを返す
    pub fn open(bytes: &[u8], notices: &mut NoticeList) -> Result<Self, String> {
        if !is_dwg(bytes) {
            return Err("DWG の図面ではありません".into());
        }
        let name = String::from_utf8_lossy(&bytes[..6]).into_owned();
        let version =
            Version::from_magic(&bytes[..6]).ok_or_else(|| format!("対応していない DWG の版です（{name}）"))?;
        if bytes.len() < 0x100 {
            return Err("DWG のファイルの頭が途中で切れています".into());
        }
        match version {
            Version::R13 | Version::R14 => {
                Err(format!("{name}（AutoCAD R13・R14）の DWG は書き直せません。R2000 以降の DWG を書き直せます"))
            }
            Version::R2000 => open_r2000(bytes, notices),
            Version::R2007 => open_r2007(bytes, notices),
            _ => open_r2004(bytes, version, notices),
        }
    }

    /// AcDb:AuxHeader（27）。ないなら None
    pub fn aux_header_mut(&mut self) -> Option<&mut Vec<u8>> {
        match &mut self.container {
            Container::R2000 { aux_header, .. } => aux_header.as_mut(),
            Container::R2004 { sections, slots, .. } => slots.aux.map(|index| &mut sections[index].data),
            Container::R2007 { sections, slots, .. } => slots.aux.map(|index| &mut sections[index].data),
        }
    }

    /// 版の入れ物に書く
    pub fn write(&self) -> Vec<u8> {
        match &self.container {
            Container::R2000 { head, aux_header, preview, template } => {
                let header = header_variables(&self.header, Version::R2000, self.maintenance);
                let handles = header.map_or([0; 14], |header| {
                    [
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
                    ]
                });
                container::write_r2000(&R2000 {
                    head: *head,
                    aux_header: aux_header.clone(),
                    preview: preview.clone(),
                    header: self.header.clone(),
                    classes: self.classes.clone(),
                    objects: self.objects.clone(),
                    template: template.clone(),
                    handles,
                })
            }
            Container::R2004 { head, sections, stream_order, slots } => {
                let mut sections = sections.clone();
                self.fill(slots, |index, data| sections[index].data = data);
                container::write_r2004(&R2004 { head: *head, sections, stream_order: stream_order.clone() })
            }
            Container::R2007 { head, sections, stream_order, random_seed, slots } => {
                let mut sections: Vec<DataSection21> = sections
                    .iter()
                    .map(|section| DataSection21 {
                        name: section.name.clone(),
                        data: section.data.clone(),
                        max_size: section.max_size,
                        encryption: section.encryption,
                        hash: section.hash,
                        encoding: section.encoding,
                        preview_base: section.preview_base,
                    })
                    .collect();
                self.fill(slots, |index, data| sections[index].data = data);
                write_r2007(&R2007 {
                    head: *head,
                    sections,
                    stream_order: stream_order.clone(),
                    random_seed: *random_seed,
                })
            }
        }
    }

    /// 節の並びに、ヘッダ変数・クラス・オブジェクト・オブジェクトの表を入れる
    fn fill(&self, slots: &Slots, mut put: impl FnMut(usize, Vec<u8>)) {
        if let Some(index) = slots.header {
            put(index, self.header.clone());
        }
        if let Some(index) = slots.classes {
            put(index, self.classes.clone());
        }
        // AcDb:AcDbObjects の中の位置は 0x0DCA の RL の 4 バイトから
        let (stream, locations) = self.objects.lay_out(0);
        put(slots.objects, stream);
        put(slots.handles, super::sections::object_map(&locations));
    }
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

fn open_r2000(bytes: &[u8], notices: &mut NoticeList) -> Result<Source, String> {
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
    // 2 つ目のファイルの頭（26）に書くハンドルをヘッダ変数から取るので、読めなければ書き直せない
    header_variables(header_section, Version::R2000, maintenance)
        .ok_or_else(|| "DWG のヘッダ変数が読めません".to_string())?;
    let mut head = [0u8; 0x15];
    head.copy_from_slice(&bytes[..0x15]);
    Ok(Source {
        version: Version::R2000,
        maintenance,
        codepage: u16::from_le_bytes([bytes[0x13], bytes[0x14]]),
        header: reframe(header_section, &HEADER_SENTINEL, &HEADER_END, false, 0, "AcDb:Header", notices),
        classes: reframe(classes_section, &CLASSES_SENTINEL, &CLASSES_END, false, 0, "AcDb:Classes", notices),
        objects: copy_objects(bytes, &map, false, notices),
        container: Container::R2000 {
            head,
            aux_header: slice(5).map(<[u8]>::to_vec),
            preview: r2000_preview(bytes).unwrap_or_else(Preview::empty),
            template: slice(4).map(<[u8]>::to_vec),
        },
    })
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

fn open_r2004(bytes: &[u8], version: Version, notices: &mut NoticeList) -> Result<Source, String> {
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
    let mut slots = Slots::default();
    let (mut header, mut classes) = (Vec::new(), Vec::new());
    let (mut objects_at, mut handles_at) = (None, None);
    let mut objects_data = Vec::new();
    let mut map_data = Vec::new();
    for info in &container.sections {
        if info.size >= MAX_SECTION_BYTES as u64 {
            return Err(format!("DWG の節 {} が大きすぎるため、書き直せません", info.name));
        }
        let mut data = container.data(info, notices);
        // 0 だけのページは書かれていない（4.5）。ページのない節も、節の大きさの 0 として写す
        data.resize(info.size as usize, 0);
        let data = match info.name.as_str() {
            "AcDb:Header" => {
                slots.header = Some(sections.len());
                header = reframe(&data, &HEADER_SENTINEL, &HEADER_END, extra, 0, &info.name, notices);
                Vec::new()
            }
            // R2004 以降のクラスの節は、終わりの区切りの後に 8 バイトの 0 が続く（10.1）
            "AcDb:Classes" => {
                slots.classes = Some(sections.len());
                classes = reframe(&data, &CLASSES_SENTINEL, &CLASSES_END, extra, 8, &info.name, notices);
                Vec::new()
            }
            "AcDb:AcDbObjects" => {
                objects_at = Some(sections.len());
                objects_data = data;
                Vec::new()
            }
            "AcDb:Handles" => {
                handles_at = Some(sections.len());
                map_data = data;
                Vec::new()
            }
            name => {
                if name == "AcDb:AuxHeader" {
                    slots.aux = Some(sections.len());
                }
                data
            }
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
            keep_zero_pages: false,
        });
    }
    let (Some(objects_at), Some(handles_at)) = (objects_at, handles_at) else {
        return Err("DWG の節（AcDb:AcDbObjects・AcDb:Handles）が見つかりません".into());
    };
    let map = read_object_map(&map_data);
    if map.is_empty() {
        return Err("DWG のオブジェクトの表が読めません".into());
    }
    // オブジェクトを並べ直し、表を作り直す
    let objects = copy_objects(&objects_data, &map, r2010, notices);
    slots.objects = objects_at;
    slots.handles = handles_at;
    // ファイルの中の順は元の図面と同じ（ページのない節は終わりに）
    let mut stream_order: Vec<usize> = (0..sections.len()).collect();
    stream_order.sort_by_key(|&index| addresses[index].unwrap_or(usize::MAX));
    let mut head = [0u8; 0x80];
    head.copy_from_slice(&bytes[..0x80]);
    Ok(Source {
        version,
        maintenance,
        codepage: u16::from_le_bytes([bytes[0x13], bytes[0x14]]),
        header,
        classes,
        objects,
        container: Container::R2004 { head, sections, stream_order, slots },
    })
}

// --- R2007 ------------------------------------------------------------------

fn open_r2007(bytes: &[u8], notices: &mut NoticeList) -> Result<Source, String> {
    // 5.2 の 0x18 は R2004 と同じ（4.1）: 1 はデータの節の暗号化
    let security = u32::from_le_bytes(bytes[0x18..0x1c].try_into().unwrap_or_default());
    if security & 1 != 0 {
        return Err("パスワードで暗号化した DWG は書き直せません".into());
    }
    let container = R2007Container::open(bytes)?;
    let mut sections = Vec::with_capacity(container.sections.len());
    let mut addresses = Vec::with_capacity(container.sections.len());
    let mut slots = Slots::default();
    let (mut header, mut classes) = (Vec::new(), Vec::new());
    let (mut objects_at, mut handles_at) = (None, None);
    let mut objects_data = Vec::new();
    let mut map_data = Vec::new();
    for info in &container.sections {
        if info.size >= MAX_SECTION_BYTES as u64 {
            return Err(format!("DWG の節 {} が大きすぎるため、書き直せません", info.name));
        }
        if info.encryption == 1 {
            return Err(format!("DWG の節 {} は暗号化されているため、書き直せません", info.name));
        }
        let mut data = container.data(info, notices);
        data.resize(info.size as usize, 0);
        let data = match info.name.as_str() {
            "AcDb:Header" => {
                slots.header = Some(sections.len());
                header = reframe(&data, &HEADER_SENTINEL, &HEADER_END, false, 0, &info.name, notices);
                Vec::new()
            }
            // R2004 以降のクラスの節は、終わりの区切りの後に 8 バイトの 0 が続く（10.1）
            "AcDb:Classes" => {
                slots.classes = Some(sections.len());
                classes = reframe(&data, &CLASSES_SENTINEL, &CLASSES_END, false, 8, &info.name, notices);
                Vec::new()
            }
            "AcDb:AcDbObjects" => {
                objects_at = Some(sections.len());
                objects_data = data;
                Vec::new()
            }
            "AcDb:Handles" => {
                handles_at = Some(sections.len());
                map_data = data;
                Vec::new()
            }
            name => {
                if name == "AcDb:AuxHeader" {
                    slots.aux = Some(sections.len());
                }
                data
            }
        };
        let address = info.pages.first().and_then(|page| container.page(page.id)).map(|(at, _)| at);
        addresses.push(address);
        // プレビューのデータはページの始まりに置かれ、中の画像の位置はファイルの中の位置
        let preview = info.name == "AcDb:Preview";
        let mut max_size = if info.max_size == 0 { 0xf800 } else { info.max_size as usize };
        if preview && data.len() > max_size {
            max_size = data.len().div_ceil(0x400) * 0x400;
        }
        sections.push(DataSection21 {
            name: info.name.clone(),
            data,
            max_size,
            encryption: info.encryption,
            hash: info.hash,
            encoding: info.encoding,
            preview_base: preview.then(|| address.map_or(0, |at| at as u64)),
        });
    }
    let (Some(objects_at), Some(handles_at)) = (objects_at, handles_at) else {
        return Err("DWG の節（AcDb:AcDbObjects・AcDb:Handles）が見つかりません".into());
    };
    let map = read_object_map(&map_data);
    if map.is_empty() {
        return Err("DWG のオブジェクトの表が読めません".into());
    }
    let objects = copy_objects(&objects_data, &map, false, notices);
    slots.objects = objects_at;
    slots.handles = handles_at;
    let mut stream_order: Vec<usize> = (0..sections.len()).collect();
    stream_order.sort_by_key(|&index| addresses[index].unwrap_or(usize::MAX));
    let mut head = [0u8; 0x80];
    head.copy_from_slice(&bytes[..0x80]);
    Ok(Source {
        version: Version::R2007,
        maintenance: bytes[0x0b],
        codepage: u16::from_le_bytes([bytes[0x13], bytes[0x14]]),
        header,
        classes,
        objects,
        container: Container::R2007 {
            head,
            sections,
            stream_order,
            random_seed: container.header[r21::RANDOM_SEED],
            slots,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write::object::ObjectWriter;
    use crate::write::sections::object_map;

    /// 書き直せる最小の R2007 の図面（読めるヘッダ変数はない）: 区切りで囲むヘッダ変数・クラスの節、オブジェクトと
    /// オブジェクトの表、圧縮しない節
    fn r2007_drawing() -> Vec<u8> {
        let framed = |data: Vec<u8>, start: &[u8; 16], end: &[u8; 16], trailing: usize| {
            Framed { extra: None, data, crc_ok: true }.bytes(start, end, trailing)
        };
        let header = framed((0..200).map(|index| index as u8).collect(), &HEADER_SENTINEL, &HEADER_END, 0);
        let classes = framed(vec![0; 40], &CLASSES_SENTINEL, &CLASSES_END, 8);
        let mut objects = Objects::default();
        for index in 0..60usize {
            let mut object = ObjectWriter::new(Version::R2007, encoding_rs::UTF_8, 0x2a, 0x100 + index as u64);
            object.data.bl(index as i32);
            object.tv("名前");
            object.h(4, 0x0c);
            objects.items.push(object.bytes());
            objects.handles.push((0x100 + index as u64, index));
        }
        let (stream, locations) = objects.lay_out(0);
        let section = |name: &str, data: Vec<u8>, max_size: usize, hash: u64, encoding: u64| DataSection21 {
            name: name.into(),
            data,
            max_size,
            encryption: 0,
            hash,
            encoding,
            preview_base: None,
        };
        let sections = vec![
            section("AcDb:SummaryInfo", vec![1; 0x40], 0x80, 0x717a_060f, 1),
            section("AcDb:AcDbObjects", stream, 0xf800, 0x674c_05a9, 4),
            section("AcDb:Handles", object_map(&locations), 0xf800, 0x3f6e_0450, 4),
            section("AcDb:Classes", classes, 0xf800, 0x3f54_045f, 4),
            section("AcDb:Header", header, 0x800, 0x32b8_03d9, 4),
        ];
        let mut head = [0u8; 0x80];
        head[..6].copy_from_slice(b"AC1021");
        head[0x0c] = 3;
        head[0x28] = 0x80;
        write_r2007(&R2007 { head, sections, stream_order: vec![0, 1, 2, 3, 4], random_seed: 0x5eed })
    }

    #[test]
    fn r2007_drawings_are_rewritten_with_the_same_sections() {
        let bytes = r2007_drawing();
        let written = rewrite_dwg(&bytes).unwrap();
        assert!(written.notices.is_empty(), "{:?}", written.notices);
        crate::samples::check_r2007_container("rewritten", &written.bytes);
        let (before, after) = (R2007Container::open(&bytes).unwrap(), R2007Container::open(&written.bytes).unwrap());
        let mut notices = NoticeList::new();
        for info in &before.sections {
            let other = after.info(&info.name).unwrap();
            assert_eq!(before.data(info, &mut notices), after.data(other, &mut notices), "{}", info.name);
        }
        // 書き直した図面をもう一度書き直しても同じバイト列
        assert_eq!(rewrite_dwg(&written.bytes).unwrap().bytes, written.bytes);
    }

    /// 壊れた・途中で切れた R2007 の図面でも panic せず、すぐに終わる
    #[test]
    fn damaged_r2007_drawings_rewrite_without_panicking() {
        let original = r2007_drawing();
        let mut seed: u64 = 0x5eed_0007;
        let mut random = move |bound: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % bound.max(1)
        };
        for round in 0..300 {
            let mut bytes = original.clone();
            match round % 3 {
                0 => bytes.truncate(random(bytes.len())),
                1 => {
                    for _ in 0..1 + random(8) {
                        let at = random(bytes.len());
                        bytes[at] = random(256) as u8;
                    }
                }
                _ => {
                    let at = random(bytes.len());
                    let length = random(64).min(bytes.len() - at);
                    bytes.drain(at..at + length);
                }
            }
            let start = std::time::Instant::now();
            if let Ok(written) = rewrite_dwg(&bytes) {
                assert!(R2007Container::open(&written.bytes).is_ok(), "round {round}");
            }
            assert!(start.elapsed().as_secs() < 5, "round {round} took too long");
        }
    }
}
