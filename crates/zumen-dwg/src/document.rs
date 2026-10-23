//! DWG の表・ブロック・レイアウト・図形を、DXF から読んだときと同じ AutoCAD の図面のモデル（DxfDocument）に組み立てる。
//! 図形はブロック（モデル空間・紙空間を含む）ごとに、ブロックが持つ順に読む（20.4.52「BLOCK HEADER」）

use crate::bits::{Read, TextCodec};
use crate::entities::{Body, Children, Decoded, Names, deg, hex, read_entity};
use crate::file::{self, Version};
use crate::header::{Header, read_header, read_measurement};
use crate::objects::{EedValue, EntityCommon, Format, Object, non_entity, open_object, read_classes, read_object_map};
use std::collections::{HashMap, HashSet};
use zumen_core::{NoticeLevel, NoticeList, js};
use zumen_dxf::*;

/// 読む図形の数の上限（1 つのブロックの図形の連なりが輪になっていても止まる）
const MAX_OBJECTS_PER_BLOCK: usize = 10_000_000;

/// 固定の型の番号（20.3「Object types」）。図形でないものは使う型だけ
fn fixed_kind(kind: u16) -> Option<&'static str> {
    Some(match kind {
        1 => "TEXT",
        2 => "ATTRIB",
        3 => "ATTDEF",
        4 => "BLOCK",
        5 => "ENDBLK",
        6 => "SEQEND",
        7 => "INSERT",
        8 => "MINSERT",
        10 => "VERTEX_2D",
        11 => "VERTEX_3D",
        12 => "VERTEX_MESH",
        13 => "VERTEX_PFACE",
        14 => "VERTEX_PFACE_FACE",
        15 => "POLYLINE_2D",
        16 => "POLYLINE_3D",
        17 => "ARC",
        18 => "CIRCLE",
        19 => "LINE",
        20 => "DIMENSION_ORDINATE",
        21 => "DIMENSION_LINEAR",
        22 => "DIMENSION_ALIGNED",
        23 => "DIMENSION_ANG3PT",
        24 => "DIMENSION_ANG2LN",
        25 => "DIMENSION_RADIUS",
        26 => "DIMENSION_DIAMETER",
        27 => "POINT",
        28 => "3DFACE",
        29 => "POLYLINE_PFACE",
        30 => "POLYLINE_MESH",
        31 => "SOLID",
        32 => "TRACE",
        33 => "SHAPE",
        34 => "VIEWPORT",
        35 => "ELLIPSE",
        36 => "SPLINE",
        37 => "REGION",
        38 => "3DSOLID",
        39 => "BODY",
        40 => "RAY",
        41 => "XLINE",
        42 => "DICTIONARY",
        43 => "OLEFRAME",
        44 => "MTEXT",
        45 => "LEADER",
        46 => "TOLERANCE",
        47 => "MLINE",
        48 => "BLOCK_CONTROL",
        49 => "BLOCK_HEADER",
        50 => "LAYER_CONTROL",
        51 => "LAYER",
        52 => "STYLE_CONTROL",
        53 => "STYLE",
        56 => "LTYPE_CONTROL",
        57 => "LTYPE",
        66 => "APPID_CONTROL",
        67 => "APPID",
        68 => "DIMSTYLE_CONTROL",
        69 => "DIMSTYLE",
        73 => "MLINESTYLE",
        74 => "OLE2FRAME",
        77 => "LWPOLYLINE",
        78 => "HATCH",
        79 => "XRECORD",
        82 => "LAYOUT",
        498 => "ACAD_PROXY_ENTITY",
        499 => "ACAD_PROXY_OBJECT",
        _ => return None,
    })
}

/// DXF に書いたときの図形の種類
fn dxf_kind(kind: &str) -> &str {
    match kind {
        "MINSERT" => "INSERT",
        "POLYLINE_2D" | "POLYLINE_3D" | "POLYLINE_PFACE" | "POLYLINE_MESH" => "POLYLINE",
        "DIMENSION_ORDINATE" | "DIMENSION_LINEAR" | "DIMENSION_ALIGNED" | "DIMENSION_ANG3PT" | "DIMENSION_ANG2LN"
        | "DIMENSION_RADIUS" | "DIMENSION_DIAMETER" => "DIMENSION",
        "MLEADER" => "MULTILEADER",
        "ACDBDICTIONARYWDFLT" => "DICTIONARYWDFLT",
        other => other,
    }
}

/// 読む図形（ほかは skipped に数える）
const READ_KINDS: [&str; 39] = [
    "LINE",
    "POINT",
    "CIRCLE",
    "ARC",
    "ELLIPSE",
    "LWPOLYLINE",
    "POLYLINE_2D",
    "POLYLINE_3D",
    "POLYLINE_PFACE",
    "POLYLINE_MESH",
    "SPLINE",
    "TEXT",
    "ATTRIB",
    "ATTDEF",
    "MTEXT",
    "INSERT",
    "MINSERT",
    "DIMENSION_ORDINATE",
    "DIMENSION_LINEAR",
    "DIMENSION_ALIGNED",
    "DIMENSION_ANG3PT",
    "DIMENSION_ANG2LN",
    "DIMENSION_RADIUS",
    "DIMENSION_DIAMETER",
    "HATCH",
    "SOLID",
    "TRACE",
    "3DFACE",
    "LEADER",
    "MULTILEADER",
    "MLINE",
    "VIEWPORT",
    "XLINE",
    "RAY",
    "IMAGE",
    "WIPEOUT",
    "ACAD_TABLE",
    "ARC_DIMENSION",
    "LARGE_RADIAL_DIMENSION",
];

/// 図形でないので、ブロックの中にあっても数えない
const MARKERS: [&str; 9] = [
    "BLOCK",
    "ENDBLK",
    "SEQEND",
    "VERTEX_2D",
    "VERTEX_3D",
    "VERTEX_MESH",
    "VERTEX_PFACE",
    "VERTEX_PFACE_FACE",
    "ATTRIB",
];

/// DWG の文字コードの番号（3.2.5）と DXF の $DWGCODEPAGE の名前。AutoCAD の番号の順（ACadSharp の CadUtils と同じ並び）
/// R2007 より前の文字列の文字コード（ファイルの頭の文字コードの番号から。分からなければ Windows-1252。読み取りと同じ）
pub(crate) fn text_encoding(codepage: u16) -> &'static encoding_rs::Encoding {
    codepage_name(codepage)
        .and_then(codepage_encoding)
        .and_then(|label| encoding_rs::Encoding::for_label(label.as_bytes()))
        .unwrap_or(encoding_rs::WINDOWS_1252)
}

pub(crate) fn codepage_name(number: u16) -> Option<&'static str> {
    const NAMES: [&str; 45] = [
        "",
        "ASCII",
        "ISO8859_1",
        "ISO8859_2",
        "ISO8859_3",
        "ISO8859_4",
        "ISO8859_5",
        "ISO8859_6",
        "ISO8859_7",
        "ISO8859_8",
        "ISO8859_9",
        "DOS437",
        "DOS850",
        "DOS852",
        "DOS855",
        "DOS857",
        "DOS860",
        "DOS861",
        "DOS863",
        "DOS864",
        "DOS865",
        "DOS869",
        "DOS932",
        "MAC_ROMAN",
        "BIG5",
        "KSC5601",
        "JOHAB",
        "DOS866",
        "ANSI_1250",
        "ANSI_1251",
        "ANSI_1252",
        "GB2312",
        "ANSI_1253",
        "ANSI_1254",
        "ANSI_1255",
        "ANSI_1256",
        "ANSI_1257",
        "ANSI_874",
        "ANSI_932",
        "ANSI_936",
        "ANSI_949",
        "ANSI_950",
        "ANSI_1361",
        "ANSI_1200",
        "ANSI_1258",
    ];
    NAMES.get(usize::from(number)).copied().filter(|name| !name.is_empty())
}

struct Reader<'a> {
    objects: &'a [u8],
    map: HashMap<u64, usize>,
    format: Format,
    /// クラスの番号 → DXF の名前
    classes: HashMap<u16, String>,
    notices: NoticeList,
    /// 読めなかったオブジェクトの種類と数
    broken: Vec<(String, u64)>,
    skipped: Vec<(String, u64)>,
}

impl<'a> Reader<'a> {
    fn kind_name(&self, kind: u16) -> String {
        match fixed_kind(kind) {
            Some(name) => name.to_string(),
            None if kind >= 500 => self.classes.get(&kind).cloned().unwrap_or_else(|| format!("CLASS{kind}")),
            None => format!("TYPE{kind}"),
        }
    }

    fn count(list: &mut Vec<(String, u64)>, name: &str) {
        match list.iter_mut().find(|(key, _)| key == name) {
            Some(entry) => entry.1 += 1,
            None => list.push((name.to_string(), 1)),
        }
    }

    /// オブジェクトを開く。表にないハンドルは None
    fn open(&self, handle: u64) -> Option<Read<(Object<'a>, String)>> {
        let &offset = self.map.get(&handle)?;
        Some(open_object(self.objects, offset, self.format).map(|object| {
            let name = self.kind_name(object.kind);
            (object, name)
        }))
    }

    /// 図形でないオブジェクトを読む。expected の種類でなければ None
    fn object<T>(
        &mut self,
        handle: u64,
        expected: &[&str],
        read: impl FnOnce(&mut Object<'a>, &str) -> Read<T>,
    ) -> Option<T> {
        let opened = self.open(handle)?;
        let Ok((mut object, name)) = opened else {
            Self::count(&mut self.broken, "?");
            return None;
        };
        if !expected.contains(&name.as_str()) {
            return None;
        }
        match read(&mut object, &name) {
            Ok(value) => Some(value),
            Err(_) => {
                Self::count(&mut self.broken, &name);
                None
            }
        }
    }

    /// 表の管理のオブジェクト（20.4.51 など）の項目のハンドル。extra は項目の後に続くハンドルの数
    fn control(&mut self, handle: u64, kind: &str, extra: usize) -> (Vec<u64>, Vec<u64>) {
        self.object(handle, &[kind], |object, _| {
            non_entity(object)?;
            let s = &mut object.streams;
            let count = s.data.bl()?;
            let mut entries = Vec::new();
            for _ in 0..count.clamp(0, 1 << 20) {
                entries.push(s.handle()?.value);
            }
            let mut after = Vec::new();
            for _ in 0..extra {
                after.push(s.handle()?.value);
            }
            Ok((entries, after))
        })
        .unwrap_or_default()
    }

    /// 辞書（20.4.44）の項目（名前とハンドル）
    fn dictionary(&mut self, handle: u64) -> Vec<(String, u64)> {
        self.object(handle, &["DICTIONARY", "DICTIONARYWDFLT", "ACDBDICTIONARYWDFLT"], |object, name| {
            non_entity(object)?;
            let s = &mut object.streams;
            let count = s.data.bl()?;
            // 複製のフラグ（BS）と持ち主のフラグ（RC）は R2000 から。R14 の DICTIONARY は不明の RC（つねに 0）。
            // 既定の項目を持つ辞書（R2000 からのクラス）は R14 でも R2000 の形（AutoCAD が R14 に保存した図面と同じ）
            if s.format.r2000() || name != "DICTIONARY" {
                s.data.bs()?;
                s.data.rc()?;
            } else if s.format.version == Version::R14 {
                s.data.rc()?;
            }
            let mut entries = Vec::new();
            for _ in 0..count.clamp(0, 1 << 20) {
                let name = s.tv()?;
                let item = s.handle()?.value;
                if item != 0 && !name.is_empty() {
                    entries.push((name, item));
                }
            }
            Ok(entries)
        })
        .unwrap_or_default()
    }
}

/// 表の項目の名前と、外部参照のビット（R2007 以降は BS、それより前は B・BS・B）
fn entry_name(object: &mut Object<'_>) -> Read<String> {
    let s = &mut object.streams;
    let name = s.tv()?;
    if s.format.r2007() {
        s.data.bs()?;
    } else {
        s.data.bit()?;
        s.data.bs()?;
        s.data.bit()?;
    }
    Ok(name)
}

/// 拡張データの app（APPID の名前）の最初の code の値
fn eed_value<'v>(object: &'v Object<'_>, appids: &HashMap<u64, String>, app: &str, code: i32) -> Option<&'v EedValue> {
    object
        .eed
        .apps
        .iter()
        .filter(|(handle, _)| appids.get(handle).is_some_and(|name| name.eq_ignore_ascii_case(app)))
        .flat_map(|(_, values)| values.iter())
        .find(|(group, _)| *group == code)
        .map(|(_, value)| value)
}

/// ブロック（BLOCK_HEADER）を読んだところ
struct Record {
    handle: u64,
    block: DxfBlock,
    /// R2004 以降の図形の並び
    owned: Vec<u64>,
    /// R2000 の最初と最後の図形
    first_last: Option<(u64, u64)>,
    xdictionary: Option<u64>,
}

/// DWG を AutoCAD の図面のモデルにする。読めないファイルは利用者に見せるメッセージを返す
pub fn read_dwg(bytes: &[u8]) -> Result<DxfDocument, String> {
    // R13 より前の図面は形が違う（r12）
    if let Some(version) = crate::r12::Old::from_magic(bytes) {
        return crate::r12::read_old(bytes, version);
    }
    let mut notices = NoticeList::new();
    let dwg = file::open(bytes, &mut notices)?;
    let version = dwg.version;
    let codepage = codepage_name(dwg.codepage);
    let label = codepage.and_then(codepage_encoding);
    let (encoding, encoding_source) = if version >= Version::R2007 {
        ("utf-16le", EncodingSource::Version)
    } else if let Some(label) = label {
        (label, EncodingSource::Codepage)
    } else {
        notices.add(
            NoticeLevel::Warning,
            "unknown-codepage",
            format!(
                "対応していない文字コード {} の文字列を西欧の文字として読みました",
                codepage.map_or_else(|| dwg.codepage.to_string(), str::to_string)
            ),
        );
        ("windows-1252", EncodingSource::Default)
    };
    let codec = if version >= Version::R2007 {
        TextCodec::Utf16
    } else {
        TextCodec::Bytes(encoding_rs::Encoding::for_label(encoding.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252))
    };
    let format = Format { version, maintenance: dwg.maintenance, codec };
    let sections = &dwg.sections;
    let header = read_header(&sections.header, format).map_err(|_| "DWG のヘッダ変数が読めません".to_string())?;
    let measurement = sections.template.as_deref().and_then(|template| read_measurement(template, version));
    let classes =
        read_classes(&sections.classes, format).into_iter().map(|class| (class.number, class.dxf_name)).collect();
    let map = read_object_map(&sections.handles);
    if map.is_empty() {
        return Err("DWG のオブジェクトの表が読めません".into());
    }
    let mut reader =
        Reader { objects: &sections.objects, map, format, classes, notices, broken: Vec::new(), skipped: Vec::new() };
    let mut doc = DxfDocument {
        format: "dwg",
        version: version.name().to_string(),
        binary: true,
        encoding,
        encoding_source,
        header: dxf_header(&header, measurement, codepage, version),
        layers: Vec::new(),
        linetypes: Vec::new(),
        styles: Vec::new(),
        dimstyles: Vec::new(),
        mline_styles: Some(Vec::new()),
        blocks: Vec::new(),
        layouts: Vec::new(),
        entities: Vec::new(),
        sort_handles: Vec::new(),
        image_frame: None,
        skipped: Vec::new(),
        notices: Vec::new(),
    };
    let mut names = Names::default();

    let appids = read_appids(&mut reader, &header);
    read_linetypes(&mut reader, &header, &mut doc, &mut names);
    read_layers(&mut reader, &header, &appids, &mut doc, &mut names);
    read_styles(&mut reader, &header, &appids, &mut doc, &mut names);
    read_dimstyles(&mut reader, &header, &mut doc, &mut names);
    let records = read_block_records(&mut reader, &header, &mut names);
    read_mline_styles(&mut reader, &header, &mut doc, &mut names);
    // LAYOUT は R2000 から。AutoCAD 2000 以降が R13・R14 に保存した図面は、名前付きオブジェクトの辞書の ACAD_LAYOUT に
    // LAYOUT を持つ（AutoCAD 2000 以降はそのレイアウトを開く）
    let layouts = if version_has_layouts(version) {
        header.layouts
    } else {
        let named = reader.dictionary(header.named_objects);
        named.iter().find(|(name, _)| name.eq_ignore_ascii_case("ACAD_LAYOUT")).map_or(0, |(_, handle)| *handle)
    };
    read_layouts(&mut reader, layouts, &mut doc, &names);
    read_book_colors(&mut reader, &header, &mut names);

    for record in &records {
        // 読み取りの上限（zumen_core::limits）
        if zumen_core::limits::tick() {
            break;
        }
        let space = if record.handle == header.model_space || is_model_name(&record.block.name) {
            DxfSpace::Model
        } else if record.block.layout {
            DxfSpace::Paper
        } else {
            DxfSpace::Block
        };
        read_block_entities(&mut reader, record, space, &names, &mut doc);
        if let Some(dictionary) = record.xdictionary {
            read_sort_table(&mut reader, dictionary, &record.block.name, &mut doc);
        }
    }
    doc.blocks = records.into_iter().map(|record| record.block).collect();
    read_image_paths(&mut reader, header.named_objects, &mut doc);
    if !version_has_layouts(version) && !doc.layouts.iter().any(|layout| !layout.model) {
        add_paper_layout(&mut doc);
    }

    js::order_object_keys(&mut doc.sort_handles);
    doc.skipped = std::mem::take(&mut reader.skipped);
    js::order_object_keys(&mut doc.skipped);
    let mut notices = std::mem::take(&mut reader.notices);
    if !doc.skipped.is_empty() {
        let total = doc.skipped.iter().map(|(_, count)| count).sum();
        let list: Vec<String> = doc.skipped.iter().map(|(kind, count)| format!("{kind} {count}")).collect();
        notices.add_count(
            NoticeLevel::Info,
            "skipped-types",
            format!("読み取りに対応していない図形を表示していません（{}）", list.join("、")),
            total,
        );
    }
    if !reader.broken.is_empty() {
        let total = reader.broken.iter().map(|(_, count)| count).sum();
        let list: Vec<String> = reader.broken.iter().map(|(kind, count)| format!("{kind} {count}")).collect();
        notices.add_count(
            NoticeLevel::Warning,
            "dwg-broken-object",
            format!("読めなかったオブジェクトを省きました（{}）", list.join("、")),
            total,
        );
    }
    doc.notices = notices.into_vec();
    Ok(doc)
}

/// LAYOUT は R2000 から
fn version_has_layouts(version: Version) -> bool {
    version >= Version::R2000
}

/// LAYOUT のない図面（R12 以前と、AutoCAD R13・R14 が保存した図面）。紙空間の図形があれば、DXF の R12 の読み取りと
/// 同じく 1 つのレイアウト（Layout1）として持つ
pub(crate) fn add_paper_layout(doc: &mut DxfDocument) {
    let Some(paper) = doc.entities.iter().find(|entity| entity.base.space == DxfSpace::Paper) else {
        return;
    };
    let block = paper.base.block.clone().unwrap_or_else(|| "*Paper_Space".into());
    doc.layouts.push(DxfLayout {
        handle: String::new(),
        name: "Layout1".into(),
        tab_order: 1.0,
        model: false,
        active: true,
        block,
        block_record_handle: None,
        paper_width: 0.0,
        paper_height: 0.0,
        margins: [0.0, 0.0, 0.0, 0.0],
        paper_size: String::new(),
        // Inferred for pre-LAYOUT files: template MEASUREMENT (ODA section 24)
        // selects the units of paper-space limits, as it does the drawing units.
        paper_units: if doc.header.measurement == Some(0.0) { 0.0 } else { 1.0 },
        plot_rotation: 0.0,
        plot_scale: [1.0, 1.0],
        plot_origin: [0.0, 0.0],
        limmin: doc.header.plimmin.unwrap_or([0.0, 0.0]),
        limmax: doc.header.plimmax.unwrap_or([0.0, 0.0]),
    });
}

fn is_model_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("*Model_Space")
}

fn is_paper_name(name: &str) -> bool {
    name.to_uppercase().starts_with("*PAPER_SPACE")
}

/// ヘッダ変数を DXF の読み取りと同じ値にする（parse.rs の header と同じ既定の値）
fn dxf_header(h: &Header, measurement: Option<f64>, codepage: Option<&str>, version: Version) -> DxfHeader {
    let point = |p: [f64; 3]| DxfHeaderValue::Point(p.to_vec());
    let mut variables = vec![("$ACADVER".to_string(), DxfHeaderValue::Text(version.name().into()))];
    if let Some(codepage) = codepage {
        variables.push(("$DWGCODEPAGE".into(), DxfHeaderValue::Text(codepage.into())));
    }
    variables.extend([
        ("$INSUNITS".to_string(), DxfHeaderValue::Number(h.insunits)),
        ("$LTSCALE".into(), DxfHeaderValue::Number(h.ltscale)),
        ("$PSLTSCALE".into(), DxfHeaderValue::Number(f64::from(u8::from(h.psltscale)))),
        ("$TEXTSIZE".into(), DxfHeaderValue::Number(h.textsize)),
        ("$DIMSCALE".into(), DxfHeaderValue::Number(h.dimscale)),
        ("$DIMASZ".into(), DxfHeaderValue::Number(h.dimasz)),
        ("$DIMTXT".into(), DxfHeaderValue::Number(h.dimtxt)),
        ("$TILEMODE".into(), DxfHeaderValue::Number(f64::from(u8::from(h.tilemode)))),
        ("$EXTMIN".into(), point(h.extmin)),
        ("$EXTMAX".into(), point(h.extmax)),
        ("$LIMMIN".into(), DxfHeaderValue::Point(h.limmin.to_vec())),
        ("$LIMMAX".into(), DxfHeaderValue::Point(h.limmax.to_vec())),
        ("$PEXTMIN".into(), point(h.pextmin)),
        ("$PEXTMAX".into(), point(h.pextmax)),
        ("$PLIMMIN".into(), DxfHeaderValue::Point(h.plimmin.to_vec())),
        ("$PLIMMAX".into(), DxfHeaderValue::Point(h.plimmax.to_vec())),
    ]);
    if let Some(measurement) = measurement {
        variables.push(("$MEASUREMENT".into(), DxfHeaderValue::Number(measurement)));
    }
    let metric = measurement == Some(1.0);
    let finite = |value: f64, fallback: f64| if value.is_finite() { value } else { fallback };
    DxfHeader {
        insunits: finite(h.insunits, 0.0),
        measurement,
        codepage: codepage.map(str::to_string),
        ltscale: finite(h.ltscale, 1.0),
        psltscale: f64::from(u8::from(h.psltscale)),
        textsize: Some(h.textsize),
        dimscale: finite(h.dimscale, 1.0),
        dimasz: finite(h.dimasz, if metric { 2.5 } else { 0.18 }),
        dimtxt: finite(h.dimtxt, if metric { 2.5 } else { 0.18 }),
        tilemode: f64::from(u8::from(h.tilemode)),
        extmin: Some(h.extmin),
        extmax: Some(h.extmax),
        limmin: Some(h.limmin),
        limmax: Some(h.limmax),
        plimmin: Some(h.plimmin),
        plimmax: Some(h.plimmax),
        variables,
    }
}

// --- 表 ---------------------------------------------------------------------

/// APPID（20.4.66）の名前。拡張データの持ち主を名前で引く
fn read_appids(reader: &mut Reader<'_>, header: &Header) -> HashMap<u64, String> {
    let (entries, _) = reader.control(header.appid_control, "APPID_CONTROL", 0);
    let mut names = HashMap::new();
    for handle in entries {
        if let Some(name) = reader.object(handle, &["APPID"], |object, _| {
            non_entity(object)?;
            entry_name(object)
        }) {
            names.insert(handle, name);
        }
    }
    names
}

/// LTYPE（20.4.58）。表の項目の後に BYLAYER・BYBLOCK が続く
fn read_linetypes(reader: &mut Reader<'_>, header: &Header, doc: &mut DxfDocument, names: &mut Names) {
    let (mut entries, after) = reader.control(header.linetype_control, "LTYPE_CONTROL", 2);
    for handle in after {
        if handle != 0 && !entries.contains(&handle) {
            entries.push(handle);
        }
    }
    for handle in entries {
        let linetype = reader.object(handle, &["LTYPE"], |object, _| {
            non_entity(object)?;
            let name = entry_name(object)?;
            let s = &mut object.streams;
            let description = s.tv()?;
            let pattern_length = s.data.bd()?;
            s.data.rc()?;
            let count = s.data.rc()?;
            let mut dashes = Vec::new();
            let mut complex = false;
            for _ in 0..count {
                dashes.push(s.data.bd()?);
                s.data.bs()?;
                s.data.rd2()?;
                s.data.bd()?;
                s.data.bd()?;
                if s.data.bs()? != 0 {
                    complex = true;
                }
            }
            Ok(DxfLinetype {
                handle: hex(handle),
                name: decode_escapes(&name),
                description: decode_escapes(&description),
                pattern_length,
                dashes,
                complex,
            })
        });
        if let Some(linetype) = linetype {
            names.linetypes.insert(handle, linetype.name.clone());
            doc.linetypes.push(linetype);
        }
    }
    names.byblock = names.linetypes.get(&header.byblock).cloned().unwrap_or_else(|| "ByBlock".into());
    names.continuous = names.linetypes.get(&header.continuous).cloned().unwrap_or_else(|| "Continuous".into());
}

/// LAYER（20.4.54）。R2000 以降の BS は 1 凍結、2 非表示、4 新しいビューポートで凍結、8 ロック、16 印刷、0x3E0 線の太さ。
/// 2 は仕様では「表示」だが、ACadSharp と同じく立っていれば非表示として読む（実際の図面で確かめた）。R13・R14 は
/// 凍結・非表示・新しいビューポートで凍結・ロックの 4 つの B で、印刷する・線の太さは既定（DXF の読み取りと同じ）
fn read_layers(
    reader: &mut Reader<'_>,
    header: &Header,
    appids: &HashMap<u64, String>,
    doc: &mut DxfDocument,
    names: &mut Names,
) {
    let (entries, _) = reader.control(header.layer_control, "LAYER_CONTROL", 0);
    for handle in entries {
        let linetypes = &names.linetypes;
        let layer = reader.object(handle, &["LAYER"], |object, _| {
            non_entity(object)?;
            let name = entry_name(object)?;
            let s = &mut object.streams;
            let values = if s.format.r2000() {
                s.data.bs()?
            } else {
                let mut values = 16 | (31 << 5);
                for bit in [1, 2, 4, 8] {
                    if s.data.bit()? {
                        values |= bit;
                    }
                }
                values
            };
            let color = s.cmc()?;
            // 外部参照のブロック、（R2000 以降）印刷スタイル、（R2007 以降）マテリアル、線種
            s.handle()?;
            if s.format.r2000() {
                s.handle()?;
            }
            if s.format.r2007() {
                s.handle()?;
            }
            let linetype = s.handle()?.value;
            let transparency = match eed_value(object, appids, "AcCmTransparency", 1071) {
                Some(EedValue::Number(value)) => Some(*value),
                _ => None,
            };
            let index = color.index.abs();
            Ok(DxfLayer {
                handle: hex(handle),
                name: decode_escapes(&name),
                color_index: if index == 0.0 || index.is_nan() || index > 255.0 { 7.0 } else { index },
                true_color: color.rgb,
                frozen: values & 1 != 0,
                off: values & 2 != 0 || color.index < 0.0,
                locked: values & 8 != 0,
                plot: values & 16 != 0,
                linetype: linetypes.get(&linetype).cloned().unwrap_or_else(|| "CONTINUOUS".into()),
                lineweight: crate::objects::lineweight(((values as u16 & 0x3e0) >> 5) as u8),
                transparency,
            })
        });
        if let Some(layer) = layer {
            names.layers.insert(handle, layer.name.clone());
            doc.layers.push(layer);
        }
    }
}

/// STYLE（20.4.56）。TrueType のフォント名は拡張データ ACAD の 1000
fn read_styles(
    reader: &mut Reader<'_>,
    header: &Header,
    appids: &HashMap<u64, String>,
    doc: &mut DxfDocument,
    names: &mut Names,
) {
    let (entries, _) = reader.control(header.style_control, "STYLE_CONTROL", 0);
    for handle in entries {
        let style = reader.object(handle, &["STYLE"], |object, _| {
            non_entity(object)?;
            let name = entry_name(object)?;
            let s = &mut object.streams;
            // 形のファイル（1）と縦書き（4）。ACadSharp と同じく形のファイルが先
            let shape = s.data.bit()?;
            let vertical = s.data.bit()?;
            let height = s.data.bd()?;
            let width = s.data.bd()?;
            let oblique = deg(s.data.bd()?);
            let generation = f64::from(s.data.rc()?);
            s.data.bd()?;
            let font = s.tv()?;
            let bigfont = s.tv()?;
            let font_family = match eed_value(object, appids, "ACAD", 1000) {
                Some(EedValue::Text(text)) => Some(decode_escapes(text)),
                _ => None,
            };
            Ok(DxfTextStyle {
                handle: hex(handle),
                name: decode_escapes(&name),
                font: decode_escapes(&font),
                bigfont: decode_escapes(&bigfont),
                height,
                width: if width == 0.0 || width.is_nan() { 1.0 } else { width },
                oblique,
                flags: f64::from(u8::from(shape) | (u8::from(vertical) << 2)),
                generation_flags: generation,
                font_family,
            })
        });
        if let Some(style) = style {
            names.styles.insert(handle, style.name.clone());
            doc.styles.push(style);
        }
    }
}

/// DIMSTYLE（20.4.68）。描画に使う DIMSCALE・DIMASZ・DIMTXT まで読む
fn read_dimstyles(reader: &mut Reader<'_>, header: &Header, doc: &mut DxfDocument, names: &mut Names) {
    let (entries, _) = reader.control(header.dimstyle_control, "DIMSTYLE_CONTROL", 0);
    for handle in entries {
        let style = reader.object(handle, &["DIMSTYLE"], |object, _| {
            non_entity(object)?;
            let name = entry_name(object)?;
            let s = &mut object.streams;
            if s.format.r2000() {
                // DIMPOST、DIMAPOST
                s.tv()?;
                s.tv()?;
            } else {
                // R13・R14: DIMTOL〜DIMSOXD（B × 11）、DIMALTD・DIMZIN（RC）、DIMSD1・DIMSD2（B）、DIMTOLJ・DIMJUST・DIMFIT
                // （RC）、DIMUPT（B）、DIMTZIN・DIMALTZ・DIMALTTZ・DIMTAD（RC）、DIMUNIT〜DIMALTTD（BS × 6）
                for _ in 0..11 {
                    s.data.bit()?;
                }
                s.data.rc()?;
                s.data.rc()?;
                s.data.bit()?;
                s.data.bit()?;
                for _ in 0..3 {
                    s.data.rc()?;
                }
                s.data.bit()?;
                for _ in 0..4 {
                    s.data.rc()?;
                }
                for _ in 0..6 {
                    s.data.bs()?;
                }
            }
            let dimscale = s.data.bd()?;
            let dimasz = s.data.bd()?;
            for _ in 0..7 {
                s.data.bd()?;
            }
            if s.format.r2007() {
                s.data.bd()?;
                s.data.bd()?;
                s.data.bs()?;
                s.cmc()?;
            }
            if s.format.r2000() {
                for _ in 0..6 {
                    s.data.bit()?;
                }
                for _ in 0..3 {
                    s.data.bs()?;
                }
            }
            if s.format.r2007() {
                s.data.bs()?;
            }
            let dimtxt = s.data.bd()?;
            Ok(DxfDimStyle { handle: hex(handle), name: decode_escapes(&name), dimscale, dimasz, dimtxt })
        });
        if let Some(style) = style {
            names.dimstyles.insert(handle, style.name.clone());
            names.arrows.insert(handle, (style.dimasz, style.dimscale));
            doc.dimstyles.push(style);
        }
    }
}

/// BLOCK_HEADER（20.4.52）と BLOCK（20.4.6）。無名のブロックは BLOCK_HEADER が名前の頭（*D など）しか持たないことがあり、
/// 名前は BLOCK の図形から取る（ACadSharp と同じ）
fn read_block_records(reader: &mut Reader<'_>, header: &Header, names: &mut Names) -> Vec<Record> {
    let (entries, after) = reader.control(header.block_control, "BLOCK_CONTROL", 2);
    let mut handles: Vec<u64> = Vec::new();
    for handle in [header.model_space, header.paper_space].into_iter().chain(after).chain(entries.iter().copied()) {
        if handle != 0 && !handles.contains(&handle) {
            handles.push(handle);
        }
    }
    let mut records = Vec::new();
    for handle in handles {
        let position = entries.iter().position(|entry| *entry == handle);
        let record = reader.object(handle, &["BLOCK_HEADER"], |object, _| {
            let common = non_entity(object)?;
            let name = entry_name(object)?;
            let s = &mut object.streams;
            let anonymous = s.data.bit()?;
            let has_attributes = s.data.bit()?;
            let xref = s.data.bit()?;
            let overlaid = s.data.bit()?;
            // 読み込んだ外部参照（R2000 から）
            if s.format.r2000() {
                s.data.bit()?;
            }
            let owned_count = if s.format.r2004() && !xref && !overlaid { s.data.bl()?.max(0) as usize } else { 0 };
            let base_point = s.data.bd3()?;
            let xref_path = s.tv()?;
            // 挿入の数・説明・プレビュー（R2000 から）
            let mut inserts = 0usize;
            let mut description = String::new();
            if s.format.r2000() {
                while s.data.rc()? != 0 {
                    inserts += 1;
                }
                description = s.tv()?;
                let preview = s.data.bl()?.max(0) as u64;
                s.data.skip(preview * 8)?;
            }
            if s.format.r2007() {
                s.data.bs()?;
                s.data.bit()?;
                s.data.rc()?;
            }
            s.handle()?;
            let block_entity = s.handle()?.value;
            let mut first_last = None;
            if !s.format.r2004() && !xref && !overlaid {
                first_last = Some((s.handle()?.value, s.handle()?.value));
            }
            let mut owned = Vec::with_capacity(owned_count.min(1 << 20));
            for _ in 0..owned_count {
                owned.push(s.handle()?.value);
            }
            // ENDBLK、挿入のハンドル、レイアウト
            let _ = s.handle();
            for _ in 0..inserts {
                let _ = s.handle();
            }
            let flags = i32::from(anonymous)
                | (i32::from(has_attributes) << 1)
                | (i32::from(xref) << 2)
                | (i32::from(overlaid) << 3);
            Ok((name, block_entity, flags, base_point, xref_path, description, owned, first_last, common.xdictionary))
        });
        let Some((name, block_entity, flags, base_point, xref_path, description, owned, first_last, xdictionary)) =
            record
        else {
            continue;
        };
        // BLOCK の図形の名前（無名のブロックの名前はこちらが正しい）
        let block_name = reader
            .object(block_entity, &["BLOCK"], |object, _| {
                crate::objects::entity_common(object)?;
                object.streams.tv()
            })
            .filter(|name| !name.is_empty());
        let name = decode_escapes(&block_name_of(&name, block_name.as_deref(), position));
        let layout = is_model_name(&name) || is_paper_name(&name);
        names.blocks.insert(handle, name.clone());
        records.push(Record {
            handle,
            block: DxfBlock {
                handle: hex(block_entity),
                anonymous: flags & 1 != 0 || name.starts_with('*'),
                name,
                base_point,
                flags: f64::from(flags),
                layout,
                block_record_handle: Some(hex(handle)),
                xref_path: (!xref_path.is_empty()).then(|| decode_escapes(&xref_path)),
                description: (!description.is_empty()).then(|| decode_escapes(&description)),
            },
            owned,
            first_last,
            xdictionary,
        });
    }
    records
}

/// ブロックの名前。BLOCK_HEADER は紙空間を *Paper_Space、無名のブロックを *D・*U などの頭だけで持つことがあり、
/// そのときは BLOCK の名前（*Paper_Space0、*D3 など）を使う。BLOCK の名前が別の名前（R2000 に保存した動的ブロックは
/// 元の名前）なら、頭と BLOCK_CONTROL の中の順の番号にする（ACadSharp の見本の DXF の *U17 と同じになる。推定）
fn block_name_of(header: &str, block: Option<&str>, position: Option<usize>) -> String {
    let starts =
        |name: &str, prefix: &str| name.len() > prefix.len() && name.to_uppercase().starts_with(&prefix.to_uppercase());
    let prefix_only = header.starts_with('*') && header.chars().count() <= 2;
    if is_model_name(header) || header.eq_ignore_ascii_case("*Paper_Space") || prefix_only {
        if let Some(block) = block.filter(|block| starts(block, header)) {
            return block.to_string();
        }
        if prefix_only && let Some(position) = position {
            return format!("{header}{position}");
        }
    }
    header.to_string()
}

/// MLINESTYLE（20.4.73）。DWG のフラグのビットは DXF の 70 と並びが違う（1↔2、32↔64、512↔1024）
fn read_mline_styles(reader: &mut Reader<'_>, header: &Header, doc: &mut DxfDocument, names: &mut Names) {
    let entries = reader.dictionary(header.mlinestyle_dictionary);
    let linetype_handles: Vec<u64> =
        doc.linetypes.iter().filter_map(|linetype| u64::from_str_radix(&linetype.handle, 16).ok()).collect();
    for (_, handle) in entries {
        let linetypes = &names.linetypes;
        let style = reader.object(handle, &["MLINESTYLE"], |object, _| {
            non_entity(object)?;
            let s = &mut object.streams;
            let name = s.tv()?;
            let description = s.tv()?;
            let bits = s.data.bs()?;
            let pairs = [(1, 2), (2, 1), (16, 16), (32, 64), (64, 32), (256, 256), (512, 1024), (1024, 512)];
            let flags = pairs.iter().filter(|(dwg, _)| bits & dwg != 0).fold(0, |sum, (_, dxf)| sum | dxf);
            let fill = s.cmc()?;
            let start_angle = deg(s.data.bd()?);
            let end_angle = deg(s.data.bd()?);
            let count = s.data.rc()?;
            let mut elements = Vec::new();
            for _ in 0..count {
                let offset = s.data.bd()?;
                let color = s.cmc()?;
                let linetype = if s.format.r2018() {
                    // DXF は BYLAYER・BYBLOCK の線種を大文字で書く
                    let handle = s.handle()?.value;
                    match linetypes.get(&handle) {
                        Some(name) if name.eq_ignore_ascii_case("ByLayer") || name.eq_ignore_ascii_case("ByBlock") => {
                            name.to_uppercase()
                        }
                        Some(name) => name.clone(),
                        None => "BYLAYER".into(),
                    }
                } else {
                    // 線種の番号。32767 は BYLAYER、32766 は BYBLOCK、ほかは線種の表の順
                    match s.data.bs()? {
                        32767 | -1 => "BYLAYER".into(),
                        32766 => "BYBLOCK".into(),
                        index => linetype_handles
                            .get(index.max(0) as usize)
                            .and_then(|handle| linetypes.get(handle))
                            .cloned()
                            .unwrap_or_else(|| "BYLAYER".into()),
                    }
                };
                elements.push(DxfMLineStyleElement { offset, color: color.index, linetype });
            }
            Ok(DxfMLineStyle {
                handle: hex(handle),
                name: decode_escapes(&name),
                flags: f64::from(flags),
                description: decode_escapes(&description),
                fill_color: fill.index,
                start_angle,
                end_angle,
                elements,
            })
        });
        if let Some(style) = style {
            names.mlinestyles.insert(handle, style.name.clone());
            if let Some(list) = doc.mline_styles.as_mut() {
                list.push(style);
            }
        }
    }
}

/// 画像（IMAGE）が指す IMAGEDEF（20.4.82）の画像のファイルの名前と読み込んだ状態、画像があれば
/// 名前付きオブジェクトの辞書の ACAD_IMAGE_VARS にある RASTERVARIABLES の外形の表示（IMAGEFRAME）
fn read_image_paths(reader: &mut Reader<'_>, named_objects: u64, doc: &mut DxfDocument) {
    let mut definitions: HashMap<u64, Option<(String, bool)>> = HashMap::new();
    let mut images = false;
    for entity in &mut doc.entities {
        let DxfEntityData::Image(image) = &mut entity.data else { continue };
        let Some(handle) = image.definition_handle.as_deref().and_then(|handle| u64::from_str_radix(handle, 16).ok())
        else {
            continue;
        };
        images = true;
        let definition = definitions
            .entry(handle)
            .or_insert_with(|| {
                reader.object(handle, &["IMAGEDEF"], |object, _| {
                    non_entity(object)?;
                    let s = &mut object.streams;
                    s.data.bl()?;
                    s.data.rd2()?;
                    let path = decode_escapes(&s.tv()?);
                    Ok((path, s.data.bit()?))
                })
            })
            .clone();
        if let Some((path, loaded)) = definition {
            image.path = Some(path);
            image.loaded = loaded;
        }
    }
    if !images {
        return;
    }
    let named = reader.dictionary(named_objects);
    let Some(&(_, handle)) = named.iter().find(|(name, _)| name.eq_ignore_ascii_case("ACAD_IMAGE_VARS")) else {
        return;
    };
    doc.image_frame = reader.object(handle, &["RASTERVARIABLES"], |object, _| {
        non_entity(object)?;
        let s = &mut object.streams;
        s.data.bl()?;
        Ok(f64::from(s.data.bs()?))
    });
}

/// LAYOUT（20.4.84）と、その前の PLOTSETTINGS の値。dictionary はレイアウトの辞書（ACAD_LAYOUT）
fn read_layouts(reader: &mut Reader<'_>, dictionary: u64, doc: &mut DxfDocument, names: &Names) {
    let entries = reader.dictionary(dictionary);
    for (_, handle) in entries {
        let layout = reader.object(handle, &["LAYOUT"], |object, _| {
            non_entity(object)?;
            let s = &mut object.streams;
            s.tv()?;
            s.tv()?;
            s.data.bs()?;
            let margins = [s.data.bd()?, s.data.bd()?, s.data.bd()?, s.data.bd()?];
            let paper_width = s.data.bd()?;
            let paper_height = s.data.bd()?;
            let paper_size = s.tv()?;
            let plot_origin = [s.data.bd()?, s.data.bd()?];
            let paper_units = f64::from(s.data.bs()?);
            let plot_rotation = f64::from(s.data.bs()?);
            s.data.bs()?;
            for _ in 0..4 {
                s.data.bd()?;
            }
            if !s.format.r2004() {
                s.tv()?;
            }
            let plot_scale = [s.data.bd()?, s.data.bd()?];
            s.tv()?;
            s.data.bs()?;
            s.data.bd()?;
            s.data.bd2()?;
            if s.format.r2004() {
                s.data.bs()?;
                s.data.bs()?;
                s.data.bs()?;
                s.handle()?;
            }
            if s.format.r2007() {
                s.handle()?;
            }
            let name = s.tv()?;
            let tab_order = f64::from(s.data.bl()?);
            s.data.bs()?;
            s.data.bd3()?;
            let limmin = s.data.rd2()?;
            let limmax = s.data.rd2()?;
            let record = s.handle()?.value;
            Ok((
                name,
                tab_order,
                margins,
                paper_width,
                paper_height,
                paper_size,
                plot_origin,
                paper_units,
                plot_rotation,
                plot_scale,
                limmin,
                limmax,
                record,
            ))
        });
        let Some((
            name,
            tab_order,
            margins,
            paper_width,
            paper_height,
            paper_size,
            plot_origin,
            paper_units,
            plot_rotation,
            plot_scale,
            limmin,
            limmax,
            record,
        )) = layout
        else {
            continue;
        };
        let block = names.blocks.get(&record).cloned().unwrap_or_default();
        let name = decode_escapes(&name);
        doc.layouts.push(DxfLayout {
            handle: hex(handle),
            model: is_model_name(&block) || name.eq_ignore_ascii_case("MODEL"),
            active: block.eq_ignore_ascii_case("*Paper_Space"),
            name: if name.is_empty() { "Layout".into() } else { name },
            tab_order,
            block,
            block_record_handle: (record != 0).then(|| hex(record)),
            paper_width,
            paper_height,
            margins,
            paper_size: decode_escapes(&paper_size),
            paper_units,
            plot_rotation,
            plot_scale,
            plot_origin,
            limmin,
            limmax,
        });
    }
    js::sort_by(&mut doc.layouts, |a, b| a.tab_order - b.tab_order);
}

/// 色見本の色（DBCOLOR）。ACAD_COLOR の辞書にあり、図形は色の代わりにそのハンドルを持つ（2.11 の ENC の 0x4000）
fn read_book_colors(reader: &mut Reader<'_>, header: &Header, names: &mut Names) {
    if header.colors == 0 {
        return;
    }
    for (_, handle) in reader.dictionary(header.colors) {
        if let Some(color) = reader.object(handle, &["DBCOLOR"], |object, _| {
            non_entity(object)?;
            object.streams.cmc()
        }) {
            names.book_colors.insert(handle, color);
        }
    }
}

/// SORTENTSTABLE（20.4.93）。ブロックの拡張辞書の ACAD_SORTENTS にある。並べ替えのハンドルは値の並びにある
fn read_sort_table(reader: &mut Reader<'_>, dictionary: u64, block: &str, doc: &mut DxfDocument) {
    let entries = reader.dictionary(dictionary);
    let Some(&(_, handle)) = entries.iter().find(|(name, _)| name == "ACAD_SORTENTS") else {
        return;
    };
    let table = reader.object(handle, &["SORTENTSTABLE"], |object, _| {
        non_entity(object)?;
        let s = &mut object.streams;
        s.handle()?;
        let count = s.data.bl()?;
        let mut handles = HashMap::new();
        for _ in 0..count.clamp(0, 1 << 22) {
            let sort = s.data.handle()?.value;
            let entity = s.handle()?.value;
            handles.insert(hex(entity), hex(sort));
        }
        Ok(handles)
    });
    if let Some(handles) = table {
        match doc.sort_handles.iter_mut().find(|(name, _)| name == block) {
            Some(entry) => entry.1 = handles,
            None => doc.sort_handles.push((block.to_string(), handles)),
        }
    }
}

// --- 図形 ---------------------------------------------------------------------

/// 持ち主（ブロック）の図形の並び。R2004 以降は BLOCK_HEADER の並び、R2000 は最初の図形から前後のつながりをたどる
fn chain(
    reader: &mut Reader<'_>,
    owned: &[u64],
    first_last: Option<(u64, u64)>,
    mut visit: impl FnMut(&mut Reader<'_>, u64) -> Option<u64>,
) {
    if !owned.is_empty() || first_last.is_none() {
        for &handle in owned {
            visit(reader, handle);
        }
        return;
    }
    let Some((first, last)) = first_last else { return };
    if first == 0 {
        return;
    }
    let mut seen: HashSet<u64> = HashSet::new();
    let mut current = first;
    while seen.insert(current) && seen.len() <= MAX_OBJECTS_PER_BLOCK {
        let next = visit(reader, current);
        if current == last {
            break;
        }
        match next {
            Some(next) if next != 0 => current = next,
            _ => break,
        }
    }
}

/// R2000 の図形の次の図形のハンドル。Nolinks が 1 ならハンドル + 1
fn next_of(handle: u64, common: &EntityCommon) -> u64 {
    common.links.map_or(handle + 1, |(_, next)| next)
}

fn read_block_entities(
    reader: &mut Reader<'_>,
    record: &Record,
    space: DxfSpace,
    names: &Names,
    doc: &mut DxfDocument,
) {
    let block = match space {
        DxfSpace::Model => None,
        _ => Some(record.block.name.clone()),
    };
    let owner = hex(record.handle);
    let mut viewports = 0.0;
    chain(reader, &record.owned, record.first_last, |reader, handle| {
        // 読み取りの上限（zumen_core::limits）
        if zumen_core::limits::tick() {
            return None;
        }
        let opened = reader.open(handle)?;
        let Ok((mut object, kind)) = opened else {
            Reader::count(&mut reader.broken, "?");
            return None;
        };
        if MARKERS.contains(&kind.as_str()) {
            // ブロックの中の BLOCK・ENDBLK などは図形でない。次の図形へ
            let common = crate::objects::entity_common(&mut object).ok()?;
            return Some(next_of(handle, &common));
        }
        let read_kind = if kind == "MLEADER" { "MULTILEADER" } else { kind.as_str() };
        if !READ_KINDS.contains(&read_kind) {
            let next = crate::objects::entity_common(&mut object).ok().map(|common| next_of(handle, &common));
            Reader::count(&mut reader.skipped, dxf_kind(&kind));
            return next.or(Some(handle + 1));
        }
        let decoded = match read_entity(&mut object, read_kind, names) {
            Ok(decoded) => decoded,
            Err(_) => {
                Reader::count(&mut reader.broken, dxf_kind(&kind));
                return Some(handle + 1);
            }
        };
        let next = next_of(handle, &decoded.common);
        let base = entity_base(dxf_kind(read_kind), handle, &decoded, &owner, space, block.as_deref(), names);
        let Decoded { body, children, .. } = decoded;
        let Body::Entity(mut data) = body else {
            return Some(next);
        };
        match &mut data {
            DxfEntityData::Polyline(polyline) => {
                read_vertices(reader, polyline, children.unwrap_or_default(), read_kind == "POLYLINE_2D", names);
            }
            DxfEntityData::Insert(insert) => {
                if let Some(children) = children {
                    insert.attribs = read_attribs(reader, children, &base, &hex(handle), names);
                }
            }
            DxfEntityData::Viewport(viewport) => {
                // DXF の 69（番号）はレイアウトの中の順（ACadSharp と同じ）。68（重なりの順）は DWG にないので、
                // 切ったビューポート（90 の 0x20000）は 0、ほかは番号と同じとする（ACadSharp の見本の DXF と同じになる。推定）
                viewports += 1.0;
                viewport.id = viewports;
                viewport.status = if js::to_int32(viewport.status_flags) & 0x20000 != 0 { 0.0 } else { viewports };
            }
            _ => {}
        }
        doc.entities.push(DxfEntity { base, data });
        Some(next)
    });
}

fn entity_base(
    kind: &str,
    handle: u64,
    decoded: &Decoded,
    owner: &str,
    space: DxfSpace,
    block: Option<&str>,
    names: &Names,
) -> DxfEntityBase {
    let common = &decoded.common;
    let linetype = match common.ltype_flags {
        0 => "BYLAYER".to_string(),
        1 => names.byblock.clone(),
        2 => names.continuous.clone(),
        _ => common.ltype.and_then(|handle| names.linetypes.get(&handle).cloned()).unwrap_or_else(|| "BYLAYER".into()),
    };
    let extrusion = decoded.extrusion;
    let book = common.book_color.and_then(|handle| names.book_colors.get(&handle).copied());
    DxfEntityBase {
        kind: kind.to_string(),
        handle: hex(handle),
        ordinal: 0,
        owner: Some(owner.to_string()),
        space,
        block: block.map(str::to_string),
        layer: names.layers.get(&common.layer).cloned().unwrap_or_else(|| "0".into()),
        linetype,
        color_index: common.color.index,
        true_color: book.map_or(common.color.rgb, |color| color.rgb),
        transparency: common.transparency,
        lineweight: common.lineweight,
        linetype_scale: if common.linetype_scale == 0.0 || common.linetype_scale.is_nan() {
            1.0
        } else {
            common.linetype_scale
        },
        invisible: common.invisible,
        thickness: (decoded.thickness != 0.0).then_some(decoded.thickness),
        extrusion: (extrusion != [0.0, 0.0, 1.0]).then_some(extrusion),
    }
}

/// POLYLINE の頂点（VERTEX）と、ポリフェースメッシュの面。2D の頂点の Z は POLYLINE の標高
fn read_vertices(reader: &mut Reader<'_>, polyline: &mut DxfPolyline, children: Children, two_d: bool, names: &Names) {
    let elevation = if two_d { polyline.vertices.points.first().map_or(0.0, |point| point[2]) } else { 0.0 };
    let defaults = if two_d { polyline.vertices.widths.first().copied().unwrap_or([0.0, 0.0]) } else { [0.0, 0.0] };
    polyline.vertices.points.clear();
    polyline.vertices.widths.clear();
    let mut faces: Vec<Vec<f64>> = Vec::new();
    let mut vertices: Vec<(f64, [f64; 3], [f64; 2], f64)> = Vec::new();
    chain(reader, &children.owned, children.first_last, |reader, handle| {
        let opened = reader.open(handle)?;
        let Ok((mut object, kind)) = opened else { return None };
        let decoded = read_entity(&mut object, &kind, names).ok()?;
        let next = next_of(handle, &decoded.common);
        match decoded.body {
            Body::Vertex(vertex) => {
                let mut point = vertex.point;
                if two_d && point[2] == 0.0 {
                    point[2] = elevation;
                }
                vertices.push((vertex.flags, point, vertex.widths.unwrap_or(defaults), vertex.bulge));
            }
            Body::Face(face) => faces.push(face.iter().copied().filter(|index| *index != 0.0).collect()),
            _ => {}
        }
        Some(next)
    });
    for (flags, point, widths, bulge) in vertices {
        polyline.vertices.points.push(point);
        polyline.vertices.widths.push(widths);
        polyline.vertices.bulges.push(bulge);
        polyline.vertex_flags.push(flags);
    }
    if !faces.is_empty() {
        polyline.faces = Some(faces);
    }
}

/// INSERT の属性（ATTRIB）。配置と同じ空間・ブロックに属する
fn read_attribs(
    reader: &mut Reader<'_>,
    children: Children,
    insert: &DxfEntityBase,
    owner: &str,
    names: &Names,
) -> Vec<DxfEntity> {
    let mut attribs = Vec::new();
    chain(reader, &children.owned, children.first_last, |reader, handle| {
        let opened = reader.open(handle)?;
        let Ok((mut object, kind)) = opened else { return None };
        if kind != "ATTRIB" {
            return Some(handle + 1);
        }
        let decoded = match read_entity(&mut object, "ATTRIB", names) {
            Ok(decoded) => decoded,
            Err(_) => {
                Reader::count(&mut reader.broken, "ATTRIB");
                return None;
            }
        };
        let next = next_of(handle, &decoded.common);
        let base = entity_base("ATTRIB", handle, &decoded, owner, insert.space, insert.block.as_deref(), names);
        if let Body::Entity(data) = decoded.body {
            attribs.push(DxfEntity { base, data });
        }
        Some(next)
    });
    attribs
}
