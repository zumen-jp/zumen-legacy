//! 版ごとの入れ物に置く前の図面（ヘッダ変数・クラス・オブジェクトなどの節）を、版の入れ物に並べる

use super::classes::{ClassDef, classes_data};
use super::container::{self, DataSection, Objects, Preview, R2000, R2004};
use super::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL, object_map};
use crate::file::Version;

/// 書く図面の節
#[derive(Debug, Clone)]
pub struct Database {
    pub version: Version,
    /// 保守版（ファイルの頭の 0x0B）
    pub maintenance: u8,
    /// 図面の文字コードの番号（3.2.5）
    pub codepage: u16,
    /// ヘッダ変数の値（9。区切り・大きさ・CRC を除く。R2007 以降はビット数の RL から）
    pub header: Vec<u8>,
    /// クラス（10）
    pub classes: Vec<ClassDef>,
    /// オブジェクト（ハンドル, MS の大きさから CRC まで）。この順に並べる
    pub objects: Vec<(u64, Vec<u8>)>,
    /// MEASUREMENT（22。0 はインチ、1 はメートル）
    pub measurement: u16,
    /// R2000 の 2 つ目のファイルの頭に書くハンドル（container::R2000::handles と同じ並び）
    pub second_header: [u64; 14],
}

/// ヘッダ変数・クラスの節の大きさの後に不明の RL があるか（9、10.2）
pub(crate) fn has_extra(version: Version, maintenance: u8) -> bool {
    (version >= Version::R2010 && maintenance > 3) || version >= Version::R2018
}

impl Database {
    /// 版の入れ物に並べた DWG。R2007 はまだ書けないので None
    pub fn bytes(&self) -> Option<Vec<u8>> {
        let extra = has_extra(self.version, self.maintenance).then_some([0u8; 4]);
        let framed = |data: &[u8]| Framed { extra, data: data.to_vec(), crc_ok: true };
        let trailing = if self.version >= Version::R2004 { 8 } else { 0 };
        let header = framed(&self.header).bytes(&HEADER_SENTINEL, &HEADER_END, 0);
        let classes = classes_data(self.version, encoding_rs::WINDOWS_1252, &self.classes);
        let classes = framed(&classes).bytes(&CLASSES_SENTINEL, &CLASSES_END, trailing);
        let objects = Objects {
            items: self.objects.iter().map(|(_, bytes)| bytes.clone()).collect(),
            handles: self.objects.iter().enumerate().map(|(index, (handle, _))| (*handle, index)).collect(),
        };
        // Template は説明の長さ（RS、空）と MEASUREMENT（RS）
        let mut template = 0u16.to_le_bytes().to_vec();
        template.extend_from_slice(&self.measurement.to_le_bytes());
        match self.version {
            Version::R13 | Version::R14 | Version::R2007 => None,
            Version::R2000 => {
                let mut head = [0u8; 0x15];
                head[..6].copy_from_slice(self.version.name().as_bytes());
                head[0x0b] = self.maintenance;
                head[0x0c] = 1;
                head[0x13..0x15].copy_from_slice(&self.codepage.to_le_bytes());
                Some(container::write_r2000(&R2000 {
                    head,
                    aux_header: None,
                    preview: Preview::empty(),
                    header,
                    classes,
                    objects,
                    template: Some(template),
                    handles: self.second_header,
                }))
            }
            _ => {
                let (stream, locations) = objects.lay_out(0);
                let section = |name: &str, data: Vec<u8>| DataSection {
                    name: name.into(),
                    data,
                    page_size: 0x7400,
                    compressed: true,
                    encrypted: 0,
                    preview_base: None,
                    keep_zero_pages: false,
                };
                let sections = vec![
                    section("", Vec::new()),
                    section("AcDb:AcDbObjects", stream),
                    section("AcDb:Template", template),
                    section("AcDb:Handles", object_map(&locations)),
                    section("AcDb:Classes", classes),
                    section("AcDb:Header", header),
                ];
                let mut head = [0u8; 0x80];
                head[..6].copy_from_slice(self.version.name().as_bytes());
                head[0x0b] = self.maintenance;
                head[0x0c] = 3;
                head[0x13..0x15].copy_from_slice(&self.codepage.to_le_bytes());
                head[0x28] = 0x80;
                let stream_order = (1..sections.len()).collect();
                Some(container::write_r2004(&R2004 { head, sections, stream_order }))
            }
        }
    }
}
