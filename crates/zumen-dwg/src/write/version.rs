//! モデルから書く DWG の版（`write_dwg_as`）と、版ごとのファイルの頭・AcDb:AuxHeader（27）の値

use crate::file::Version;

/// モデルから書く DWG の版
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DwgVersion {
    R13,
    R14,
    R2000,
    R2004,
    R2007,
    R2010,
    R2013,
    R2018,
}

impl DwgVersion {
    pub const ALL: [DwgVersion; 8] = [
        DwgVersion::R13,
        DwgVersion::R14,
        DwgVersion::R2000,
        DwgVersion::R2004,
        DwgVersion::R2007,
        DwgVersion::R2010,
        DwgVersion::R2013,
        DwgVersion::R2018,
    ];

    /// ファイルの頭の版の文字列（AC1015 など）
    pub fn code(self) -> &'static str {
        self.version().name()
    }

    /// 人に見せる名前（R13、R14、2000 など）
    pub fn name(self) -> &'static str {
        match self {
            DwgVersion::R13 => "R13",
            DwgVersion::R14 => "R14",
            DwgVersion::R2000 => "2000",
            DwgVersion::R2004 => "2004",
            DwgVersion::R2007 => "2007",
            DwgVersion::R2010 => "2010",
            DwgVersion::R2013 => "2013",
            DwgVersion::R2018 => "2018",
        }
    }

    /// 名前・版の文字列から（R14・14・AC1014・1014、2000・R2000・AC1015・1015 など。大文字・小文字は問わない。
    /// `zumen_dxf::DxfVersion::parse` と同じ読み方）
    pub fn parse(text: &str) -> Option<DwgVersion> {
        let text = text.trim().to_ascii_uppercase();
        let text = text.strip_prefix('R').unwrap_or(&text);
        DwgVersion::ALL.into_iter().find(|version| {
            version.code() == text || version.name().trim_start_matches('R') == text || version.code()[2..] == *text
        })
    }

    pub(crate) fn version(self) -> Version {
        match self {
            DwgVersion::R13 => Version::R13,
            DwgVersion::R14 => Version::R14,
            DwgVersion::R2000 => Version::R2000,
            DwgVersion::R2004 => Version::R2004,
            DwgVersion::R2007 => Version::R2007,
            DwgVersion::R2010 => Version::R2010,
            DwgVersion::R2013 => Version::R2013,
            DwgVersion::R2018 => Version::R2018,
        }
    }

    /// 保守版（ファイルの頭の 0x0B と 0x12、AcDb:AuxHeader）。AutoCAD がその版で保存した図面の値（2013 は
    /// AutoCAD 2013 の値。R13 は見本がないので 0）。R2010・R2013 は 3 より大きいので、ヘッダ変数・クラスの節は大きさの
    /// 後に RL を持つ
    pub(crate) fn maintenance(self) -> u8 {
        match self {
            DwgVersion::R13 => 0,
            DwgVersion::R14 => 10,
            DwgVersion::R2000 => 15,
            DwgVersion::R2004 => 104,
            DwgVersion::R2007 => 50,
            DwgVersion::R2010 => 226,
            DwgVersion::R2013 => 8,
            DwgVersion::R2018 => 0,
        }
    }

    /// 書いたアプリケーションの版（ファイルの頭の 0x11、AcDb:AuxHeader の版。27 の表）。その版を書く AutoCAD の値
    pub(crate) fn release(self) -> u16 {
        match self {
            DwgVersion::R13 => 19,
            DwgVersion::R14 => 21,
            DwgVersion::R2000 => 23,
            DwgVersion::R2004 => 25,
            DwgVersion::R2007 => 27,
            DwgVersion::R2010 => 29,
            DwgVersion::R2013 => 31,
            DwgVersion::R2018 => 33,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_read_from_their_names_and_codes() {
        for (text, version) in [
            ("R13", DwgVersion::R13),
            ("14", DwgVersion::R14),
            ("ac1014", DwgVersion::R14),
            ("1012", DwgVersion::R13),
            ("2000", DwgVersion::R2000),
            ("R2004", DwgVersion::R2004),
            ("ac1021", DwgVersion::R2007),
            ("1024", DwgVersion::R2010),
            (" r2013 ", DwgVersion::R2013),
            ("AC1032", DwgVersion::R2018),
        ] {
            assert_eq!(DwgVersion::parse(text), Some(version), "{text}");
        }
        for text in ["", "R12", "AC1009", "2019", "R"] {
            assert_eq!(DwgVersion::parse(text), None, "{text}");
        }
        for version in DwgVersion::ALL {
            assert_eq!(DwgVersion::parse(version.code()), Some(version));
            assert_eq!(DwgVersion::parse(version.name()), Some(version));
        }
    }
}
