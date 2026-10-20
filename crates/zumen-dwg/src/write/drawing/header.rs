//! ヘッダ変数（9）と、新しい図面のほかの節: AcDb:AuxHeader（27）、AcDb:SummaryInfo（13）、AcDb:Preview（14）、
//! AcDb:AppInfo（16）、AcDb:FileDepList（17）、AcDb:RevHistory（18）、AcDb:ObjFreeSpace（21）、AcDb:Template（22）と、
//! 版ごとの入れ物（R2000 は 3、R2004・R2010〜R2018 は 4、R2007 は 5）。
//! ヘッダ変数はすべての変数を AutoCAD の既定の値（acad.dwt、インチの図面）で作り、モデルの値で置き換える

use super::{Writer, href};
use crate::codec::header::{HeaderVars, Kind, Value, field_kind, write_header_vars};
use crate::codec::tables::DimValue;
use crate::codec::{CmColor, RawText};
use crate::file::Version;
use crate::write::classes::{ClassDef, classes_data};
use crate::write::container::{DataSection, Objects, Preview, R2000, R2004, write_r2000, write_r2004};
use crate::write::container21::{DataSection21, R2007, write_r2007};
use crate::write::database::has_extra;
use crate::write::sections::{CLASSES_END, CLASSES_SENTINEL, Framed, HEADER_END, HEADER_SENTINEL, object_map};
use crate::write::version::DwgVersion;
use encoding_rs::Encoding;
use zumen_dxf::{DxfDocument, DxfHeaderValue};

/// BYBLOCK の CMC（R2004 以降は RGB の値の種類、それより前は色の番号 0）
fn by_block(version: Version) -> CmColor {
    if version >= Version::R2004 {
        CmColor { index: 0, rgb: 0xc100_0000, name: None, book: None }
    } else {
        CmColor { index: 0, rgb: 0, name: None, book: None }
    }
}

/// 寸法の変数（DIMSTYLE と $DIM…）の AutoCAD の既定の値（インチの図面）
pub(super) fn dim_default(name: &str, version: Version) -> DimValue {
    let text = |value: &str| DimValue::Text(RawText::new(value, version, encoding_rs::WINDOWS_1252));
    match name {
        "DIMPOST" | "DIMAPOST" | "DIMALTMZS" | "DIMMZS" => text(""),
        "DIMSCALE" | "DIMFXL" | "DIMLFAC" | "DIMTFAC" => DimValue::Double(1.0),
        "DIMASZ" | "DIMEXE" | "DIMTXT" => DimValue::Double(0.18),
        "DIMEXO" => DimValue::Double(0.0625),
        "DIMDLI" => DimValue::Double(0.38),
        "DIMJOGANG" => DimValue::Double(std::f64::consts::FRAC_PI_4),
        "DIMCEN" | "DIMGAP" => DimValue::Double(0.09),
        "DIMALTF" => DimValue::Double(25.4),
        "DIMALTMZF" | "DIMMZF" => DimValue::Double(100.0),
        "DIMTFILLCLR" | "DIMCLRD" | "DIMCLRE" | "DIMCLRT" => DimValue::Color(by_block(version)),
        "DIMTIH" | "DIMTOH" => DimValue::Bit(true),
        "DIMALTD" | "DIMALTU" | "DIMALTTD" | "DIMLUNIT" => DimValue::Short(2),
        "DIMDEC" | "DIMTDEC" => DimValue::Short(4),
        "DIMDSEP" => DimValue::Short(46),
        "DIMTOLJ" => DimValue::Short(1),
        "DIMATFIT" => DimValue::Short(3),
        "DIMLWD" | "DIMLWE" => DimValue::Short(-2),
        _ => match name {
            "DIMTOL" | "DIMLIM" | "DIMSE1" | "DIMSE2" | "DIMALT" | "DIMTOFL" | "DIMSAH" | "DIMTIX" | "DIMSOXD"
            | "DIMSD1" | "DIMSD2" | "DIMUPT" | "DIMFXLON" | "DIMTXTDIRECTION" | "FLAG" => DimValue::Bit(false),
            "DIMRND" | "DIMDLE" | "DIMTP" | "DIMTM" | "DIMTSZ" | "DIMTVP" | "DIMALTRND" => DimValue::Double(0.0),
            _ => DimValue::Short(0),
        },
    }
}

/// ヘッダ変数の AutoCAD の既定の値（acad.dwt を AutoCAD 2013 の形で保存した値。ハンドルは書くときに決める）。
/// 色は R2004 より前は色の番号（256 BYLAYER）
fn default(name: &str, version: Version) -> Option<Value> {
    use Value::*;
    let text = |value: &str| Text(RawText::new(value, version, encoding_rs::WINDOWS_1252));
    // 寸法スタイルの値と同じ $DIM… は、DIMSTYLE の既定の値（$DIMASSOC などは寸法スタイルの値でない）
    if let Some(dim) = name.strip_prefix('$')
        && crate::codec::tables::DimStyle::fields(version).any(|(key, _)| key == dim)
    {
        return Some(match dim_default(dim, version) {
            DimValue::Bit(value) => Bit(value),
            DimValue::Short(value) => Short(value),
            DimValue::Double(value) => Double(value),
            DimValue::Text(value) => Text(value),
            DimValue::Color(value) => Color(value),
        });
    }
    Some(match name {
        "UNKNOWN_BD1" => Double(412_148_564_080.0),
        "UNKNOWN_BD2" | "UNKNOWN_BD3" | "UNKNOWN_BD4" | "UNKNOWN_BD5" => Double(1.0),
        "UNKNOWN_TV1" => text("m"),
        "UNKNOWN_BL1" => Long(24),
        "UNKNOWN_BL10" => Long(10),
        "UNKNOWN_B2" => Bit(true),
        "$DIMASO" | "$DIMSHO" | "$REGENMODE" | "$FILLMODE" | "$PSLTSCALE" | "$USRTIMER" | "$WORLDVIEW"
        | "$TILEMODE" | "$VISRETAIN" => Bit(true),
        "$PROXYGRAPHICS" | "$ATTMODE" | "$TSTACKALIGN" => Short(1),
        "$TREEDEPTH" => Short(3020),
        "$LUNITS" => Short(2),
        "$LUPREC" => Short(4),
        "$SPLINESEGS" => Short(8),
        "$SURFU" | "$SURFV" | "$SURFTYPE" | "$SURFTAB1" | "$SURFTAB2" | "$SPLINETYPE" => Short(6),
        "$SHADEDGE" => Short(3),
        "$SHADEDIF" | "$TSTACKSIZE" => Short(70),
        "$MAXACTVP" => Short(64),
        "$ISOLINES" => Short(4),
        "$TEXTQLTY" => Short(50),
        "$LTSCALE" | "$CMLSCALE" | "$CELTSCALE" => Double(1.0),
        "$TEXTSIZE" => Double(0.2),
        "$TRACEWID" => Double(0.05),
        "$SKETCHINC" => Double(0.1),
        "$FACETRES" => Double(0.5),
        "$MENU" => text("acad"),
        "$CECOLOR" if version < Version::R2004 => Color(CmColor { index: 256, rgb: 0, name: None, book: None }),
        "$CECOLOR" => Color(CmColor { index: 0, rgb: 0xc000_0000, name: None, book: None }),
        "$PUCSXDIR" | "$UCSXDIR" => Point3([1.0, 0.0, 0.0]),
        "$PUCSYDIR" | "$UCSYDIR" => Point3([0.0, 1.0, 0.0]),
        "$PLIMMAX" | "$LIMMAX" => Point2([12.0, 9.0]),
        "$EXTMIN" | "$PEXTMIN" => Point3([1e20, 1e20, 1e20]),
        "$EXTMAX" | "$PEXTMAX" => Point3([-1e20, -1e20, -1e20]),
        // CELWEIGHT（0x1F）は BYLAYER（29）、EXTNAMES（0x800）、LWDISPLAY を切る（0x200）、PSTYLEMODE（0x2000）
        "FLAGS" => Long(0x2a1d),
        "$INSUNITS" => Short(1),
        "$SORTENTS" => Byte(127),
        "$HIDETEXT" | "$SHOWHIST" | "$LOFTNORMALS" | "$LIGHTGLYPHDISPLAY" | "$TILEMODELIGHTSYNCH" => Byte(1),
        "$XCLIPFRAME" | "$DIMASSOC" | "$DWFFRAME" => Byte(2),
        "$OBSCUREDCOLOR" | "$INTERSECTIONCOLOR" => Short(257),
        "$STEPSPERSEC" | "$3DDWFPREC" => Double(2.0),
        "$STEPSIZE" => Double(6.0),
        "$LENSLENGTH" => Double(50.0),
        "$PSOLWIDTH" => Double(0.25),
        "$PSOLHEIGHT" => Double(4.0),
        "$LOFTANG1" | "$LOFTANG2" => Double(std::f64::consts::FRAC_PI_2),
        "$LOFTPARAM" => Short(7),
        "$LATITUDE" => Double(37.795),
        "$LONGITUDE" => Double(-122.394),
        "$TIMEZONE" => Long(-8000),
        "$INTERFERECOLOR" => Color(CmColor { index: 0, rgb: 0xc300_0001, name: None, book: None }),
        _ => return None,
    })
}

/// DXF の Julian の日付（日と、日の中の割合）を DWG の日と 1/1000 秒の 2 つの BL に
fn julian(value: f64) -> (i32, i32) {
    if !value.is_finite() || value <= 0.0 {
        return (0, 0);
    }
    let days = value.floor();
    let ms = ((value - days) * 86_400_000.0).round().min(86_399_999.0);
    (days.min(f64::from(i32::MAX)) as i32, ms as i32)
}

/// 日付の変数の DWG の 2 つの名前（日と 1/1000 秒）
const DATES: [(&str, &str, &str); 4] = [
    ("$TDCREATE", "$TDCREATE", "$TDCREATE_MS"),
    ("$TDUPDATE", "$TDUPDATE", "$TDUPDATE_MS"),
    ("$TDINDWG", "$TDINDWG", "$TDINDWG_MS"),
    ("$TDUSRTIMER", "$TDUSRTIMER", "$TDUSRTIMER_MS"),
];

/// モデルの変数（DXF の値）をヘッダ変数の形にする。形が合わないもの・ハンドルは None。文字列は図形と同じく
/// キャレット表記を戻す（Writer::text）
fn from_dxf(w: &Writer<'_>, kind: Kind, value: &DxfHeaderValue) -> Option<Value> {
    let number = |value: &DxfHeaderValue| match value {
        DxfHeaderValue::Number(number) if number.is_finite() => Some(*number),
        _ => None,
    };
    Some(match kind {
        Kind::B => Value::Bit(number(value)? != 0.0),
        Kind::Bs => Value::Short(number(value)?.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16),
        Kind::Bl => Value::Long(number(value)?.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32),
        Kind::Rc => Value::Byte(number(value)?.clamp(0.0, 255.0) as u8),
        Kind::Bd => Value::Double(number(value)?),
        Kind::Rd2 | Kind::Bd3 => {
            let DxfHeaderValue::Point(point) = value else { return None };
            if point.iter().any(|v| !v.is_finite()) || point.len() < 2 {
                return None;
            }
            if kind == Kind::Rd2 {
                Value::Point2([point[0], point[1]])
            } else {
                Value::Point3([point[0], point[1], point.get(2).copied().unwrap_or(0.0)])
            }
        }
        Kind::Tv => {
            let DxfHeaderValue::Text(text) = value else { return None };
            Value::Text(w.text(text))
        }
        _ => return None,
    })
}

/// 文字コードの名前（$DWGCODEPAGE）の番号（3.2.5。読み取りと同じ表）
pub(super) fn codepage_index(name: &str) -> Option<u16> {
    (1..64u16)
        .find(|&number| crate::document::codepage_name(number).is_some_and(|known| known.eq_ignore_ascii_case(name)))
}

/// 図面の文字コードの番号（3.2.5）。$DWGCODEPAGE の名前から。分からなければ ANSI_1252（30）
pub(super) fn codepage_number(doc: &DxfDocument) -> u16 {
    doc.header.codepage.as_deref().and_then(codepage_index).unwrap_or(30)
}

/// ヘッダ変数の値（区切り・大きさ・CRC を除く）
pub(super) fn header_section(w: &Writer<'_>, seed: u64) -> Vec<u8> {
    let doc = w.doc;
    let version = w.version;
    let mut vars = HeaderVars::with_defaults(version, |name| default(name, version));
    // モデルの変数（DXF から読んだ図面はすべての変数、DWG から読んだ図面は読み取りが持つ変数）
    for (name, value) in &doc.header.variables {
        if let Some((_, days, ms)) = DATES.iter().find(|(dxf, _, _)| dxf == name) {
            if let DxfHeaderValue::Number(number) = value {
                let (day, millisecond) = julian(*number);
                vars.set(days, Value::Long(day));
                vars.set(ms, Value::Long(millisecond));
            }
            continue;
        }
        if matches!(name.as_str(), "$HANDSEED" | "$ACADVER" | "$ACADMAINTVER" | "$DWGCODEPAGE") {
            continue;
        }
        let Some(kind) = field_kind(name) else { continue };
        if let Some(value) = from_dxf(w, kind, value) {
            vars.set(name, value);
        }
    }
    // 表示が使う変数は、モデルの値（DXF の読み取りが既定の値を補った値）で書く
    let header = &doc.header;
    vars.set("$INSUNITS", Value::Short(header.insunits.clamp(0.0, 32767.0) as i16));
    vars.set("$LTSCALE", Value::Double(header.ltscale));
    vars.set("$PSLTSCALE", Value::Bit(header.psltscale != 0.0));
    vars.set("$TEXTSIZE", Value::Double(header.textsize.unwrap_or(2.5)));
    vars.set("$DIMSCALE", Value::Double(header.dimscale));
    vars.set("$DIMASZ", Value::Double(header.dimasz));
    vars.set("$DIMTXT", Value::Double(header.dimtxt));
    vars.set("$TILEMODE", Value::Bit(header.tilemode != 0.0));
    for (name, point) in [("$EXTMIN", header.extmin), ("$EXTMAX", header.extmax)] {
        if let Some(point) = point {
            vars.set(name, Value::Point3(point));
        }
    }
    for (name, point) in [
        ("$LIMMIN", header.limmin),
        ("$LIMMAX", header.limmax),
        ("$PLIMMIN", header.plimmin),
        ("$PLIMMAX", header.plimmax),
    ] {
        if let Some(point) = point {
            vars.set(name, Value::Point2(point));
        }
    }
    // 現在の値（レイヤ・スタイルなど）と、表・辞書のハンドル
    let fixed = &w.fixed;
    let handle = |code: u8, value: u64| Value::Handle(href(code, value));
    let standard_style = w.style_handles.get("STANDARD").copied().unwrap_or(0);
    let model = w.blocks[w.block_index[super::MODEL]].record;
    let paper = w.blocks[w.block_index[super::PAPER]].record;
    let linetype = |name: &str| w.linetype_handles.get(name).copied().unwrap_or(0);
    let mline_standard = w
        .mline_styles
        .iter()
        .find(|(_, style)| style.name.eq_ignore_ascii_case("STANDARD"))
        .map_or(0, |(handle, _)| *handle);
    for (name, value) in [
        ("$HANDSEED", Value::Handle(href(0, seed))),
        ("$CLAYER", handle(5, w.layer_handles.get("0").copied().unwrap_or(0))),
        ("$TEXTSTYLE", handle(5, standard_style)),
        ("$CELTYPE", handle(5, linetype("BYLAYER"))),
        ("$CMATERIAL", handle(5, fixed.material_bylayer)),
        ("$DIMSTYLE", handle(5, w.dimstyle_handles.get("STANDARD").copied().unwrap_or(0))),
        ("$CMLSTYLE", handle(5, mline_standard)),
        ("$DIMTXSTY", handle(5, standard_style)),
        ("BLOCK_CONTROL", handle(3, fixed.block_control)),
        ("LAYER_CONTROL", handle(3, fixed.layer_control)),
        ("STYLE_CONTROL", handle(3, fixed.style_control)),
        ("LTYPE_CONTROL", handle(3, fixed.ltype_control)),
        ("VIEW_CONTROL", handle(3, fixed.view_control)),
        ("UCS_CONTROL", handle(3, fixed.ucs_control)),
        ("VPORT_CONTROL", handle(3, fixed.vport_control)),
        ("APPID_CONTROL", handle(3, fixed.appid_control)),
        ("DIMSTYLE_CONTROL", handle(3, fixed.dimstyle_control)),
        ("DICTIONARY_GROUP", handle(5, fixed.group_dict)),
        ("DICTIONARY_MLINESTYLE", handle(5, fixed.mline_dict)),
        ("DICTIONARY_NAMED_OBJECTS", handle(3, fixed.root)),
        ("DICTIONARY_LAYOUTS", handle(5, fixed.layout_dict)),
        ("DICTIONARY_PLOTSETTINGS", handle(5, fixed.plot_dict)),
        ("DICTIONARY_PLOTSTYLES", handle(5, fixed.plotstyle_dict)),
        ("DICTIONARY_MATERIALS", handle(5, fixed.material_dict)),
        ("DICTIONARY_COLORS", handle(5, fixed.color_dict)),
        ("DICTIONARY_VISUALSTYLE", handle(5, fixed.visualstyle_dict)),
        ("BLOCK_RECORD_PAPER_SPACE", handle(5, paper)),
        ("BLOCK_RECORD_MODEL_SPACE", handle(5, model)),
        ("LTYPE_BYLAYER", handle(5, linetype("BYLAYER"))),
        ("LTYPE_BYBLOCK", handle(5, linetype("BYBLOCK"))),
        ("LTYPE_CONTINUOUS", handle(5, linetype("CONTINUOUS"))),
        // R2000 だけ: VIEWPORT ENTITY HEADER の表と、現在のもの（紙空間の最初のビューポートの項目）
        ("VPENTHDR_CONTROL", handle(3, fixed.vpent_control)),
        ("VIEWPORT_ENTITY", handle(5, w.viewport_headers.first().map_or(0, |(_, header)| *header))),
    ] {
        vars.set(name, value);
    }
    // 図面の GUID（モデルになければ、書いたオブジェクトから決める。同じ図面からは同じ値）
    for (name, salt) in [("$FINGERPRINTGUID", 0x9e37_79b9_u32), ("$VERSIONGUID", 0x85eb_ca6b)] {
        let present = doc
            .header
            .variable(name)
            .is_some_and(|value| matches!(value, DxfHeaderValue::Text(text) if !text.is_empty()));
        if !present {
            vars.set(name, Value::Text(RawText::new(&guid(w, salt), version, encoding_rs::WINDOWS_1252)));
        }
    }
    write_header_vars(&vars)
}

/// 書いたオブジェクトから作る GUID の形の文字列（{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}）
fn guid(w: &Writer<'_>, salt: u32) -> String {
    let mut words = [salt, salt ^ 0x5bd1_e995, salt.rotate_left(13), !salt];
    for (handle, bytes) in &w.objects {
        let value = crate::crc::crc32(*handle as u32, bytes);
        for (index, word) in words.iter_mut().enumerate() {
            *word = word.rotate_left(5 + index as u32) ^ value.wrapping_mul(0x0100_0193 + index as u32);
        }
    }
    let [a, b, c, d] = words;
    format!(
        "{{{a:08X}-{:04X}-{:04X}-{:04X}-{:04X}{d:08X}}}",
        b >> 16,
        (b & 0x0fff) | 0x4000,
        (c >> 16 & 0x3fff) | 0x8000,
        c & 0xffff
    )
}

/// 日付の変数（日, 1/1000 秒）
fn date(doc: &DxfDocument, name: &str) -> (i32, i32) {
    match doc.header.variable(name) {
        Some(DxfHeaderValue::Number(value)) => julian(*value),
        _ => (0, 0),
    }
}

/// 文字列（RS の文字の数（終わりの 0 を含む）、文字、0）。AcDb:SummaryInfo・AcDb:AppInfo の書き方。R2007 以降（wide）は
/// UTF-16、それより前は図面の文字コードのバイト（13、16.1）
fn string(out: &mut Vec<u8>, value: &str, wide: bool, encoding: &'static Encoding) {
    if wide {
        let units: Vec<u16> = value.encode_utf16().take(0x7ffe).collect();
        out.extend_from_slice(&(units.len() as u16 + 1).to_le_bytes());
        for unit in units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    } else {
        let (bytes, _, _) = encoding.encode(value);
        let bytes = &bytes[..bytes.len().min(0x7ffe)];
        out.extend_from_slice(&(bytes.len() as u16 + 1).to_le_bytes());
        out.extend_from_slice(bytes);
        out.push(0);
    }
}

/// AcDb:AuxHeader（27）。版・保存の回数・日付・HANDSEED。版は書いたアプリケーションの版（ファイルの頭の 0x11 と同じ）。
/// R2018 は保守版が RL（実物の図面で確かめた。仕様は RS で、終わりに 3 つの RS を足すが、AutoCAD の図面にはない）
fn aux_header(doc: &DxfDocument, seed: u64, target: DwgVersion) -> Vec<u8> {
    let mut out = vec![0xff, 0x77, 0x01];
    let rs = |out: &mut Vec<u8>, value: u16| out.extend_from_slice(&value.to_le_bytes());
    let rl = |out: &mut Vec<u8>, value: u32| out.extend_from_slice(&value.to_le_bytes());
    let release = |out: &mut Vec<u8>| {
        out.extend_from_slice(&target.release().to_le_bytes());
        let maintenance = target.maintenance();
        if target >= DwgVersion::R2018 {
            out.extend_from_slice(&u32::from(maintenance).to_le_bytes());
        } else {
            out.extend_from_slice(&u16::from(maintenance).to_le_bytes());
        }
    };
    release(&mut out);
    rl(&mut out, 1);
    rl(&mut out, u32::MAX);
    rs(&mut out, 1);
    rs(&mut out, 0);
    rl(&mut out, 0);
    for _ in 0..2 {
        release(&mut out);
    }
    for value in [5, 0x893, 5, 0x893, 0, 1] {
        rs(&mut out, value);
    }
    for _ in 0..5 {
        rl(&mut out, 0);
    }
    for name in ["$TDCREATE", "$TDUPDATE"] {
        let (days, ms) = date(doc, name);
        rl(&mut out, days as u32);
        rl(&mut out, ms as u32);
    }
    rl(&mut out, if seed <= 0x7fff_ffff { seed as u32 } else { u32::MAX });
    rl(&mut out, 0);
    rs(&mut out, 0);
    rs(&mut out, 1);
    for _ in 0..3 {
        rl(&mut out, 0);
    }
    rl(&mut out, 1);
    for _ in 0..4 {
        rl(&mut out, 0);
    }
    out
}

/// AcDb:SummaryInfo（13）。題名などは空、日付はモデルの $TDCREATE・$TDUPDATE
fn summary_info(doc: &DxfDocument, wide: bool, encoding: &'static Encoding) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..8 {
        string(&mut out, "", wide, encoding);
    }
    out.extend_from_slice(&[0; 8]);
    for name in ["$TDCREATE", "$TDUPDATE"] {
        let (days, ms) = date(doc, name);
        out.extend_from_slice(&days.to_le_bytes());
        out.extend_from_slice(&ms.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    out
}

/// AcDb:AppInfo（16）。書いたアプリケーションの名前と版。形は書いたアプリケーションの版（ファイルの頭の 0x11）による:
/// R2007 以降は 16.2 の R21〜27 の形、R2004 は 16.1 の R18 の形
fn app_info(target: DwgVersion, encoding: &'static Encoding) -> Vec<u8> {
    let version = env!("CARGO_PKG_VERSION");
    let product = format!(
        "<ProductInformation name =\"zumen\" build_version=\"{version}\" registry_version=\"{version}\" install_id_string=\"zumen\" registry_localeID=\"1033\"/>"
    );
    if target < DwgVersion::R2007 {
        let mut out = Vec::new();
        string(&mut out, "AppInfoDataList", false, encoding);
        out.extend_from_slice(&2u32.to_le_bytes());
        string(&mut out, "4001", false, encoding);
        string(&mut out, &product, false, encoding);
        string(&mut out, version, false, encoding);
        return out;
    }
    let mut out = 2u32.to_le_bytes().to_vec();
    string(&mut out, "AppInfoDataList", true, encoding);
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    string(&mut out, version, true, encoding);
    out.extend_from_slice(&[0; 16]);
    string(&mut out, "", true, encoding);
    out.extend_from_slice(&[0; 16]);
    string(&mut out, &product, true, encoding);
    out
}

/// AcDb:ObjFreeSpace（21）。AutoCAD の図面と同じ形（0、オブジェクトの数、$TDUPDATE、オブジェクトの節の位置、4 つの値の組）。
/// R2007 までは 21.1 の 32 ビットの値、R2010 以降は 64 ビットの値（AutoCAD の図面で確かめた）
fn obj_free_space(doc: &DxfDocument, count: usize, target: DwgVersion) -> Vec<u8> {
    let (days, ms) = date(doc, "$TDUPDATE");
    let values = [0x32u64, 0, 0x64, 0, 0x200, 0, 0xffff_ffff, 0];
    let mut out = Vec::new();
    if target < DwgVersion::R2010 {
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(count.min(u32::MAX as usize) as u32).to_le_bytes());
        out.extend_from_slice(&days.to_le_bytes());
        out.extend_from_slice(&ms.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(4);
        for value in values {
            out.extend_from_slice(&(value as u32).to_le_bytes());
        }
        return out;
    }
    out.extend_from_slice(&[0; 8]);
    out.extend_from_slice(&(count as u64).to_le_bytes());
    out.extend_from_slice(&days.to_le_bytes());
    out.extend_from_slice(&ms.to_le_bytes());
    out.push(4);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// 版の入れ物に並べた図面。header はヘッダ変数の値、objects は書いたオブジェクト（ハンドルの順）
#[allow(clippy::too_many_arguments)]
pub(super) fn container(
    w: &Writer<'_>,
    header: Vec<u8>,
    classes: &[ClassDef],
    objects: Objects,
    count: usize,
    measurement: Option<f64>,
    seed: u64,
) -> Vec<u8> {
    let doc = w.doc;
    let (target, version) = (w.target, w.version);
    let maintenance = target.maintenance();
    let extra = has_extra(version, maintenance).then_some([0u8; 4]);
    let framed = |data: Vec<u8>| Framed { extra, data, crc_ok: true };
    let header = framed(header).bytes(&HEADER_SENTINEL, &HEADER_END, 0);
    // R2004 以降のクラスの節は、終わりの区切りの後に 8 バイトの 0 が続く（10.1）
    let trailing = if version >= Version::R2004 { 8 } else { 0 };
    let classes = framed(classes_data(version, w.encoding, classes)).bytes(&CLASSES_SENTINEL, &CLASSES_END, trailing);
    // Template: 説明の長さ（RS、空）と MEASUREMENT（RS）。0 だけでも、モデルが MEASUREMENT を持つなら置く
    // （AutoCAD は 0 だけのページを省き、読み取りはそのとき MEASUREMENT を持たない図面と読む）
    let mut template = 0u16.to_le_bytes().to_vec();
    template.extend_from_slice(&(measurement.unwrap_or(0.0).clamp(0.0, 1.0) as u16).to_le_bytes());
    if version < Version::R2004 {
        return r2000(w, header, classes, objects, measurement.map(|_| template), seed);
    }
    let (stream, locations) = objects.lay_out(0);
    let wide = version >= Version::R2007;
    let preview = Preview::empty();
    let app_info = app_info(target, w.encoding);
    let mut rev_history = vec![0u8; 16];
    rev_history[8] = 1;
    let summary = summary_info(doc, wide, w.encoding);
    let map = object_map(&locations);
    let free = obj_free_space(doc, count, target);
    let aux = aux_header(doc, seed, target);
    let mut head = [0u8; 0x80];
    head[..6].copy_from_slice(version.name().as_bytes());
    head[0x0b] = maintenance;
    head[0x0c] = 3;
    // 書いたアプリケーションの版と保守版、文字コード
    head[0x11] = target.release() as u8;
    head[0x12] = maintenance;
    head[0x13..0x15].copy_from_slice(&w.codepage.to_le_bytes());
    head[0x28] = 0x80;
    if version == Version::R2007 {
        // 5.1 の節の表の値（名前・ハッシュ・ページの大きさ・暗号化・符号化）。ページの大きさは仕様の値（AutoCAD の図面と同じ）
        let section = |name: &str, data: Vec<u8>, max_size: usize, hash: u64, encoding: u64| DataSection21 {
            name: name.into(),
            data,
            max_size,
            encryption: 0,
            hash,
            encoding,
            preview_base: None,
        };
        let mut file_deps = section("AcDb:FileDepList", vec![0; 8], 0x100, 0x6c42_05ca, 1);
        file_deps.encryption = 2;
        let app_page = app_info.len().div_ceil(0x20).max(0x18) * 0x20;
        let mut preview_section = section("AcDb:Preview", preview.data, 0x400, 0x40aa_0473, 1);
        preview_section.preview_base = Some(preview.base);
        let random_seed = (u64::from(crate::crc::crc32(0, &header)) << 32) | u64::from(crate::crc::crc32(0, &stream));
        let sections = vec![
            file_deps,
            section("AcDb:AppInfo", app_info, app_page, 0x3fa0_043e, 1),
            preview_section,
            section("AcDb:SummaryInfo", summary, 0x80, 0x717a_060f, 1),
            section("AcDb:RevHistory", rev_history, 0x1000, 0x60a2_05b3, 4),
            section("AcDb:AcDbObjects", stream, 0xf800, 0x674c_05a9, 4),
            section("AcDb:ObjFreeSpace", free, 0xf800, 0x77e2_061f, 4),
            // MEASUREMENT がなければ空（R2007 の入れ物は 0 だけのページも書くので、読み取りが 0 と読まないよう中身を置かない）
            section("AcDb:Template", if measurement.is_some() { template } else { Vec::new() }, 0x400, 0x4a14_04ce, 4),
            section("AcDb:Handles", map, 0xf800, 0x3f6e_0450, 4),
            section("AcDb:Classes", classes, 0xf800, 0x3f54_045f, 4),
            section("AcDb:AuxHeader", aux, 0x800, 0x54f0_050a, 4),
            section("AcDb:Header", header, 0x800, 0x32b8_03d9, 4),
        ];
        let names: Vec<&str> = sections.iter().map(|section| section.name.as_str()).collect();
        let stream_order = stream_order(&names);
        return write_r2007(&R2007 { head, sections, stream_order, random_seed });
    }
    let section = |name: &str, data: Vec<u8>, page_size: usize, compressed: bool| DataSection {
        name: name.into(),
        data,
        page_size,
        compressed,
        encrypted: 0,
        preview_base: None,
        keep_zero_pages: false,
    };
    let app_page = app_info.len().div_ceil(0x80) * 0x80;
    let mut file_deps = section("AcDb:FileDepList", vec![0; 8], 0x80, false);
    file_deps.encrypted = 2;
    let mut preview_section = section("AcDb:Preview", preview.data, 0x400, false);
    preview_section.preview_base = Some(preview.base);
    let mut template = section("AcDb:Template", template, 0x7400, true);
    template.keep_zero_pages = measurement.is_some();
    let sections = vec![
        section("", Vec::new(), 0x7400, true),
        file_deps,
        section("AcDb:AppInfo", app_info, app_page, false),
        preview_section,
        section("AcDb:SummaryInfo", summary, 0x100, false),
        section("AcDb:RevHistory", rev_history, 0x7400, true),
        section("AcDb:AcDbObjects", stream, 0x7400, true),
        section("AcDb:ObjFreeSpace", free, 0x7400, true),
        template,
        section("AcDb:Handles", map, 0x7400, true),
        section("AcDb:Classes", classes, 0x7400, true),
        section("AcDb:AuxHeader", aux, 0x7400, true),
        section("AcDb:Header", header, 0x7400, true),
    ];
    let names: Vec<&str> = sections.iter().map(|section| section.name.as_str()).collect();
    let stream_order = stream_order(&names);
    write_r2004(&R2004 { head, sections, stream_order })
}

/// R2000 の入れ物（3）: ファイルの頭、AuxHeader、プレビュー、ヘッダ変数、クラス、オブジェクト、オブジェクトの表、
/// 2 つ目のファイルの頭（26）と、モデルが MEASUREMENT を持つなら Template
fn r2000(
    w: &Writer<'_>,
    header: Vec<u8>,
    classes: Vec<u8>,
    objects: Objects,
    template: Option<Vec<u8>>,
    seed: u64,
) -> Vec<u8> {
    let target = w.target;
    let maintenance = target.maintenance();
    let mut head = [0u8; 0x15];
    head[..6].copy_from_slice(w.version.name().as_bytes());
    head[0x0b] = maintenance;
    head[0x0c] = 1;
    head[0x11] = target.release() as u8;
    head[0x12] = maintenance;
    head[0x13..0x15].copy_from_slice(&w.codepage.to_le_bytes());
    let fixed = &w.fixed;
    let handles = [
        seed,
        fixed.block_control,
        fixed.layer_control,
        fixed.style_control,
        fixed.ltype_control,
        fixed.view_control,
        fixed.ucs_control,
        fixed.vport_control,
        fixed.appid_control,
        fixed.dimstyle_control,
        fixed.vpent_control,
        fixed.root,
        fixed.mline_dict,
        fixed.group_dict,
    ];
    write_r2000(&R2000 {
        head,
        aux_header: Some(aux_header(w.doc, seed, target)),
        preview: Preview::empty(),
        header,
        classes,
        objects,
        template,
        handles,
    })
}

/// ファイルの中に置く順（AutoCAD と同じく SummaryInfo・Preview・AppInfo・FileDepList を先に、ほかは節の表の順）
fn stream_order(names: &[&str]) -> Vec<usize> {
    let first = ["AcDb:SummaryInfo", "AcDb:Preview", "AcDb:AppInfo", "AcDb:FileDepList"];
    let mut order: Vec<usize> = first.iter().filter_map(|name| names.iter().position(|known| known == name)).collect();
    for (index, name) in names.iter().enumerate() {
        if !name.is_empty() && !order.contains(&index) {
            order.push(index);
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn julian_dates_split_into_days_and_milliseconds() {
        assert_eq!(julian(2_459_000.5), (2_459_000, 43_200_000));
        assert_eq!(julian(-1.0), (0, 0));
    }

    /// 節の大きさは、それぞれの版の AutoCAD の図面と同じ（AuxHeader は R2018 だけ 129、ObjFreeSpace は R2007 まで 53）
    #[test]
    fn sections_have_the_sizes_of_autocad_drawings() {
        let doc = zumen_dxf::parse_dxf(b"0\nEOF\n");
        let latin = encoding_rs::WINDOWS_1252;
        for target in DwgVersion::ALL {
            let aux = if target >= DwgVersion::R2018 { 129 } else { 123 };
            assert_eq!(aux_header(&doc, 0x20, target).len(), aux, "{target:?}");
            let free = if target >= DwgVersion::R2010 { 89 } else { 53 };
            assert_eq!(obj_free_space(&doc, 3, target).len(), free, "{target:?}");
        }
        assert_eq!(summary_info(&doc, true, latin).len(), 8 * 4 + 8 + 16 + 2 + 8);
        assert_eq!(summary_info(&doc, false, latin).len(), 8 * 3 + 8 + 16 + 2 + 8);
        // R18 の AppInfo は 8 ビットの文字列（AppInfoDataList、2、4001、製品の XML、版）
        let info = app_info(DwgVersion::R2004, latin);
        assert_eq!(&info[..18], b"\x10\x00AppInfoDataList\x00");
        assert_eq!(&info[18..22], &2u32.to_le_bytes());
        assert_eq!(&info[22..29], b"\x05\x004001\x00");
    }
}
