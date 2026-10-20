//! ヘッダ変数（9「Data section AcDb:Header」）のすべての値。仕様の並びを表（FIELDS）にし、読むのも書くのも
//! この表をたどる。名前は DXF の変数の名前（紙空間の値は $P…）で、DXF にない値は仕様の説明から付けた名前。
//! R2007 以降は、値の並びの前にビット数（RL）、後ろに文字列の並び、その後ろにハンドルの並びを置く（5.9）。
//! それより前はハンドルも文字列も値の並びの中

use super::{CmColor, In, Out, RawText};
use crate::bits::{Bits, HandleRef, Read};
use crate::file::Version;
use crate::objects::string_stream;

/// 値の形
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    B,
    Bs,
    Bl,
    Bll,
    Bd,
    Rc,
    Rd2,
    Bd3,
    Tv,
    Cmc,
    /// ハンドル（R2007 以降はハンドルの並び）
    H,
    /// HANDSEED（つねに値の並び。CODE 0）
    Seed,
    /// 終わりの 4 つの BS（9 の「R14+: unknown short」）。R2007 以降はハンドルの並びの後ろにある（実物の図面で確かめた）
    TailBs,
}

/// 値がある版
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    All,
    /// R2000（R2004 より前）だけ
    Before2004,
    From2004,
    From2007,
    From2010,
    From2013,
}

impl When {
    fn holds(self, version: Version) -> bool {
        match self {
            When::All => true,
            When::Before2004 => version < Version::R2004,
            When::From2004 => version >= Version::R2004,
            When::From2007 => version >= Version::R2007,
            When::From2010 => version >= Version::R2010,
            When::From2013 => version >= Version::R2013,
        }
    }
}

use Kind::*;
use When::*;

/// 9 のヘッダ変数の並び（R2000 以降。R13・R14 だけの値は除く）。CPSNID は CEPSNTYPE が 3 のときだけある
const FIELDS: &[(&str, Kind, When)] = &[
    ("$REQUIREDVERSIONS", Bll, From2013),
    ("UNKNOWN_BD1", Bd, All),
    ("UNKNOWN_BD2", Bd, All),
    ("UNKNOWN_BD3", Bd, All),
    ("UNKNOWN_BD4", Bd, All),
    ("UNKNOWN_TV1", Tv, All),
    ("UNKNOWN_TV2", Tv, All),
    ("UNKNOWN_TV3", Tv, All),
    ("UNKNOWN_TV4", Tv, All),
    ("UNKNOWN_BL1", Bl, All),
    ("UNKNOWN_BL2", Bl, All),
    ("VIEWPORT_ENTITY", H, Before2004),
    ("$DIMASO", B, All),
    ("$DIMSHO", B, All),
    ("$PLINEGEN", B, All),
    ("$ORTHOMODE", B, All),
    ("$REGENMODE", B, All),
    ("$FILLMODE", B, All),
    ("$QTEXTMODE", B, All),
    ("$PSLTSCALE", B, All),
    ("$LIMCHECK", B, All),
    ("UNKNOWN_B1", B, From2004),
    ("$USRTIMER", B, All),
    ("$SKPOLY", B, All),
    ("$ANGDIR", B, All),
    ("$SPLFRAME", B, All),
    ("$MIRRTEXT", B, All),
    ("$WORLDVIEW", B, All),
    ("$TILEMODE", B, All),
    ("$PLIMCHECK", B, All),
    ("$VISRETAIN", B, All),
    ("$DISPSILH", B, All),
    ("$PELLIPSE", B, All),
    ("$PROXYGRAPHICS", Bs, All),
    ("$TREEDEPTH", Bs, All),
    ("$LUNITS", Bs, All),
    ("$LUPREC", Bs, All),
    ("$AUNITS", Bs, All),
    ("$AUPREC", Bs, All),
    ("$ATTMODE", Bs, All),
    ("$PDMODE", Bs, All),
    ("UNKNOWN_BL3", Bl, From2004),
    ("UNKNOWN_BL4", Bl, From2004),
    ("UNKNOWN_BL5", Bl, From2004),
    ("$USERI1", Bs, All),
    ("$USERI2", Bs, All),
    ("$USERI3", Bs, All),
    ("$USERI4", Bs, All),
    ("$USERI5", Bs, All),
    ("$SPLINESEGS", Bs, All),
    ("$SURFU", Bs, All),
    ("$SURFV", Bs, All),
    ("$SURFTYPE", Bs, All),
    ("$SURFTAB1", Bs, All),
    ("$SURFTAB2", Bs, All),
    ("$SPLINETYPE", Bs, All),
    ("$SHADEDGE", Bs, All),
    ("$SHADEDIF", Bs, All),
    ("$UNITMODE", Bs, All),
    ("$MAXACTVP", Bs, All),
    ("$ISOLINES", Bs, All),
    ("$CMLJUST", Bs, All),
    ("$TEXTQLTY", Bs, All),
    ("$LTSCALE", Bd, All),
    ("$TEXTSIZE", Bd, All),
    ("$TRACEWID", Bd, All),
    ("$SKETCHINC", Bd, All),
    ("$FILLETRAD", Bd, All),
    ("$THICKNESS", Bd, All),
    ("$ANGBASE", Bd, All),
    ("$PDSIZE", Bd, All),
    ("$PLINEWID", Bd, All),
    ("$USERR1", Bd, All),
    ("$USERR2", Bd, All),
    ("$USERR3", Bd, All),
    ("$USERR4", Bd, All),
    ("$USERR5", Bd, All),
    ("$CHAMFERA", Bd, All),
    ("$CHAMFERB", Bd, All),
    ("$CHAMFERC", Bd, All),
    ("$CHAMFERD", Bd, All),
    ("$FACETRES", Bd, All),
    ("$CMLSCALE", Bd, All),
    ("$CELTSCALE", Bd, All),
    // 仕様は R2004 までと書くが、どの版にもある（ACadSharp・読み取りと同じ）
    ("$MENU", Tv, All),
    ("$TDCREATE", Bl, All),
    ("$TDCREATE_MS", Bl, All),
    ("$TDUPDATE", Bl, All),
    ("$TDUPDATE_MS", Bl, All),
    ("UNKNOWN_BL6", Bl, From2004),
    ("UNKNOWN_BL7", Bl, From2004),
    ("UNKNOWN_BL8", Bl, From2004),
    ("$TDINDWG", Bl, All),
    ("$TDINDWG_MS", Bl, All),
    ("$TDUSRTIMER", Bl, All),
    ("$TDUSRTIMER_MS", Bl, All),
    ("$CECOLOR", Cmc, All),
    ("$HANDSEED", Seed, All),
    ("$CLAYER", H, All),
    ("$TEXTSTYLE", H, All),
    ("$CELTYPE", H, All),
    ("$CMATERIAL", H, From2007),
    ("$DIMSTYLE", H, All),
    ("$CMLSTYLE", H, All),
    ("$PSVPSCALE", Bd, All),
    ("$PINSBASE", Bd3, All),
    ("$PEXTMIN", Bd3, All),
    ("$PEXTMAX", Bd3, All),
    ("$PLIMMIN", Rd2, All),
    ("$PLIMMAX", Rd2, All),
    ("$PELEVATION", Bd, All),
    ("$PUCSORG", Bd3, All),
    ("$PUCSXDIR", Bd3, All),
    ("$PUCSYDIR", Bd3, All),
    ("$PUCSNAME", H, All),
    ("$PUCSORTHOREF", H, All),
    ("$PUCSORTHOVIEW", Bs, All),
    ("$PUCSBASE", H, All),
    ("$PUCSORGTOP", Bd3, All),
    ("$PUCSORGBOTTOM", Bd3, All),
    ("$PUCSORGLEFT", Bd3, All),
    ("$PUCSORGRIGHT", Bd3, All),
    ("$PUCSORGFRONT", Bd3, All),
    ("$PUCSORGBACK", Bd3, All),
    ("$INSBASE", Bd3, All),
    ("$EXTMIN", Bd3, All),
    ("$EXTMAX", Bd3, All),
    ("$LIMMIN", Rd2, All),
    ("$LIMMAX", Rd2, All),
    ("$ELEVATION", Bd, All),
    ("$UCSORG", Bd3, All),
    ("$UCSXDIR", Bd3, All),
    ("$UCSYDIR", Bd3, All),
    ("$UCSNAME", H, All),
    ("$UCSORTHOREF", H, All),
    ("$UCSORTHOVIEW", Bs, All),
    ("$UCSBASE", H, All),
    ("$UCSORGTOP", Bd3, All),
    ("$UCSORGBOTTOM", Bd3, All),
    ("$UCSORGLEFT", Bd3, All),
    ("$UCSORGRIGHT", Bd3, All),
    ("$UCSORGFRONT", Bd3, All),
    ("$UCSORGBACK", Bd3, All),
    ("$DIMPOST", Tv, All),
    ("$DIMAPOST", Tv, All),
    ("$DIMSCALE", Bd, All),
    ("$DIMASZ", Bd, All),
    ("$DIMEXO", Bd, All),
    ("$DIMDLI", Bd, All),
    ("$DIMEXE", Bd, All),
    ("$DIMRND", Bd, All),
    ("$DIMDLE", Bd, All),
    ("$DIMTP", Bd, All),
    ("$DIMTM", Bd, All),
    ("$DIMFXL", Bd, From2007),
    ("$DIMJOGANG", Bd, From2007),
    ("$DIMTFILL", Bs, From2007),
    ("$DIMTFILLCLR", Cmc, From2007),
    ("$DIMTOL", B, All),
    ("$DIMLIM", B, All),
    ("$DIMTIH", B, All),
    ("$DIMTOH", B, All),
    ("$DIMSE1", B, All),
    ("$DIMSE2", B, All),
    ("$DIMTAD", Bs, All),
    ("$DIMZIN", Bs, All),
    ("$DIMAZIN", Bs, All),
    ("$DIMARCSYM", Bs, From2007),
    ("$DIMTXT", Bd, All),
    ("$DIMCEN", Bd, All),
    ("$DIMTSZ", Bd, All),
    ("$DIMALTF", Bd, All),
    ("$DIMLFAC", Bd, All),
    ("$DIMTVP", Bd, All),
    ("$DIMTFAC", Bd, All),
    ("$DIMGAP", Bd, All),
    ("$DIMALTRND", Bd, All),
    ("$DIMALT", B, All),
    ("$DIMALTD", Bs, All),
    ("$DIMTOFL", B, All),
    ("$DIMSAH", B, All),
    ("$DIMTIX", B, All),
    ("$DIMSOXD", B, All),
    ("$DIMCLRD", Cmc, All),
    ("$DIMCLRE", Cmc, All),
    ("$DIMCLRT", Cmc, All),
    ("$DIMADEC", Bs, All),
    ("$DIMDEC", Bs, All),
    ("$DIMTDEC", Bs, All),
    ("$DIMALTU", Bs, All),
    ("$DIMALTTD", Bs, All),
    ("$DIMAUNIT", Bs, All),
    ("$DIMFRAC", Bs, All),
    ("$DIMLUNIT", Bs, All),
    ("$DIMDSEP", Bs, All),
    ("$DIMTMOVE", Bs, All),
    ("$DIMJUST", Bs, All),
    ("$DIMSD1", B, All),
    ("$DIMSD2", B, All),
    ("$DIMTOLJ", Bs, All),
    ("$DIMTZIN", Bs, All),
    ("$DIMALTZ", Bs, All),
    ("$DIMALTTZ", Bs, All),
    ("$DIMUPT", B, All),
    ("$DIMATFIT", Bs, All),
    ("$DIMFXLON", B, From2007),
    ("$DIMTXTDIRECTION", B, From2010),
    ("$DIMALTMZF", Bd, From2010),
    ("$DIMALTMZS", Tv, From2010),
    ("$DIMMZF", Bd, From2010),
    ("$DIMMZS", Tv, From2010),
    ("$DIMTXSTY", H, All),
    ("$DIMLDRBLK", H, All),
    ("$DIMBLK", H, All),
    ("$DIMBLK1", H, All),
    ("$DIMBLK2", H, All),
    ("$DIMLTYPE", H, From2007),
    ("$DIMLTEX1", H, From2007),
    ("$DIMLTEX2", H, From2007),
    ("$DIMLWD", Bs, All),
    ("$DIMLWE", Bs, All),
    ("BLOCK_CONTROL", H, All),
    ("LAYER_CONTROL", H, All),
    ("STYLE_CONTROL", H, All),
    ("LTYPE_CONTROL", H, All),
    ("VIEW_CONTROL", H, All),
    ("UCS_CONTROL", H, All),
    ("VPORT_CONTROL", H, All),
    ("APPID_CONTROL", H, All),
    ("DIMSTYLE_CONTROL", H, All),
    ("VPENTHDR_CONTROL", H, Before2004),
    ("DICTIONARY_GROUP", H, All),
    ("DICTIONARY_MLINESTYLE", H, All),
    ("DICTIONARY_NAMED_OBJECTS", H, All),
    ("$TSTACKALIGN", Bs, All),
    ("$TSTACKSIZE", Bs, All),
    ("$HYPERLINKBASE", Tv, All),
    ("$STYLESHEET", Tv, All),
    ("DICTIONARY_LAYOUTS", H, All),
    ("DICTIONARY_PLOTSETTINGS", H, All),
    ("DICTIONARY_PLOTSTYLES", H, All),
    ("DICTIONARY_MATERIALS", H, From2004),
    ("DICTIONARY_COLORS", H, From2004),
    ("DICTIONARY_VISUALSTYLE", H, From2007),
    ("UNKNOWN_H1", H, From2013),
    ("FLAGS", Bl, All),
    ("$INSUNITS", Bs, All),
    ("$CEPSNTYPE", Bs, All),
    ("$CPSNID", H, All),
    ("$FINGERPRINTGUID", Tv, All),
    ("$VERSIONGUID", Tv, All),
    ("$SORTENTS", Rc, From2004),
    ("$INDEXCTL", Rc, From2004),
    ("$HIDETEXT", Rc, From2004),
    ("$XCLIPFRAME", Rc, From2004),
    ("$DIMASSOC", Rc, From2004),
    ("$HALOGAP", Rc, From2004),
    ("$OBSCUREDCOLOR", Bs, From2004),
    ("$INTERSECTIONCOLOR", Bs, From2004),
    ("$OBSCUREDLTYPE", Rc, From2004),
    ("$INTERSECTIONDISPLAY", Rc, From2004),
    ("$PROJECTNAME", Tv, From2004),
    ("BLOCK_RECORD_PAPER_SPACE", H, All),
    ("BLOCK_RECORD_MODEL_SPACE", H, All),
    ("LTYPE_BYLAYER", H, All),
    ("LTYPE_BYBLOCK", H, All),
    ("LTYPE_CONTINUOUS", H, All),
    ("$CAMERADISPLAY", B, From2007),
    ("UNKNOWN_BL9", Bl, From2007),
    ("UNKNOWN_BL10", Bl, From2007),
    ("UNKNOWN_BD5", Bd, From2007),
    ("$STEPSPERSEC", Bd, From2007),
    ("$STEPSIZE", Bd, From2007),
    ("$3DDWFPREC", Bd, From2007),
    ("$LENSLENGTH", Bd, From2007),
    ("$CAMERAHEIGHT", Bd, From2007),
    ("$SOLIDHIST", Rc, From2007),
    ("$SHOWHIST", Rc, From2007),
    ("$PSOLWIDTH", Bd, From2007),
    ("$PSOLHEIGHT", Bd, From2007),
    ("$LOFTANG1", Bd, From2007),
    ("$LOFTANG2", Bd, From2007),
    ("$LOFTMAG1", Bd, From2007),
    ("$LOFTMAG2", Bd, From2007),
    ("$LOFTPARAM", Bs, From2007),
    ("$LOFTNORMALS", Rc, From2007),
    ("$LATITUDE", Bd, From2007),
    ("$LONGITUDE", Bd, From2007),
    ("$NORTHDIRECTION", Bd, From2007),
    ("$TIMEZONE", Bl, From2007),
    ("$LIGHTGLYPHDISPLAY", Rc, From2007),
    ("$TILEMODELIGHTSYNCH", Rc, From2007),
    ("$DWFFRAME", Rc, From2007),
    ("$DGNFRAME", Rc, From2007),
    ("UNKNOWN_B2", B, From2007),
    ("$INTERFERECOLOR", Cmc, From2007),
    ("$INTERFEREOBJVS", H, From2007),
    ("$INTERFEREVPVS", H, From2007),
    ("$DRAGVS", H, From2007),
    ("$CSHADOW", Rc, From2007),
    ("UNKNOWN_BD6", Bd, From2007),
    ("UNKNOWN_BS1", TailBs, All),
    ("UNKNOWN_BS2", TailBs, All),
    ("UNKNOWN_BS3", TailBs, All),
    ("UNKNOWN_BS4", TailBs, All),
];

/// 1 つの値
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Bit(bool),
    Short(i16),
    Long(i32),
    LongLong(u64),
    Double(f64),
    Byte(u8),
    Point2([f64; 2]),
    Point3([f64; 3]),
    Text(RawText),
    Color(CmColor),
    Handle(HandleRef),
}

/// ヘッダ変数の値（FIELDS の順。その版にない値は持たない）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HeaderVars {
    pub version: Version,
    pub values: Vec<(&'static str, Value)>,
}

impl HeaderVars {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.iter().find(|(key, _)| *key == name).map(|(_, value)| value)
    }

    /// 値を置き換える（その版にない値は無視する）
    pub fn set(&mut self, name: &str, value: Value) {
        if let Some(slot) = self.values.iter_mut().find(|(key, _)| *key == name) {
            slot.1 = value;
        }
    }

    pub fn handle(&self, name: &str) -> Option<u64> {
        match self.get(name)? {
            Value::Handle(handle) => Some(handle.value),
            _ => None,
        }
    }
}

/// CPSNID（印刷スタイルのハンドル）は CEPSNTYPE が 3 のときだけある
fn present(name: &str, values: &[(&'static str, Value)]) -> bool {
    if name != "$CPSNID" {
        return true;
    }
    matches!(values.iter().rev().find(|(key, _)| *key == "$CEPSNTYPE"), Some((_, Value::Short(3))))
}

fn read_value(s: &mut In<'_>, kind: Kind) -> Read<Value> {
    Ok(match kind {
        B => Value::Bit(s.data.bit()?),
        Bs => Value::Short(s.data.bs()?),
        Bl => Value::Long(s.data.bl()?),
        Bll => Value::LongLong(s.data.bll()?),
        Bd => Value::Double(s.data.bd()?),
        Rc => Value::Byte(s.data.rc()?),
        Rd2 => Value::Point2(s.data.rd2()?),
        Bd3 => Value::Point3(s.data.bd3()?),
        Tv => Value::Text(s.text()?),
        Cmc => Value::Color(s.cmc()?),
        H => Value::Handle(s.handle()?),
        Seed => Value::Handle(s.data.handle()?),
        TailBs => Value::Short(match &mut s.handles {
            Some(handles) => handles.bs()?,
            None => s.data.bs()?,
        }),
    })
}

fn write_value(out: &mut Out, kind: Kind, value: &Value) {
    match (kind, value) {
        (B, Value::Bit(value)) => out.data.bit(*value),
        (Bs, Value::Short(value)) => out.data.bs(*value),
        (Bl, Value::Long(value)) => out.data.bl(*value),
        (Bll, Value::LongLong(value)) => out.data.bll(*value),
        (Bd, Value::Double(value)) => out.data.bd(*value),
        (Rc, Value::Byte(value)) => out.data.rc(*value),
        (Rd2, Value::Point2(value)) => out.data.rd2(*value),
        (Bd3, Value::Point3(value)) => out.data.bd3(*value),
        (Tv, Value::Text(value)) => out.text(value),
        (Cmc, Value::Color(value)) => out.cmc(value),
        (H, Value::Handle(value)) => out.handle(*value),
        (Seed, Value::Handle(value)) => out.data.handle(value.code, value.value),
        (TailBs, Value::Short(value)) => {
            if out.inline_handles {
                out.data.bs(*value)
            } else {
                out.handles.bs(*value)
            }
        }
        // 形の合わない値は既定の値（0）で書く
        (kind, _) => write_value(out, kind, &zero(kind, out.version)),
    }
}

/// その形の 0 の値
fn zero(kind: Kind, version: Version) -> Value {
    match kind {
        B => Value::Bit(false),
        Bs => Value::Short(0),
        Bl => Value::Long(0),
        Bll => Value::LongLong(0),
        Bd => Value::Double(0.0),
        Rc => Value::Byte(0),
        Rd2 => Value::Point2([0.0; 2]),
        Bd3 => Value::Point3([0.0; 3]),
        Tv => Value::Text(RawText::empty(version)),
        Cmc => Value::Color(CmColor::by_layer()),
        H => Value::Handle(HandleRef { code: 5, value: 0 }),
        Seed => Value::Handle(HandleRef { code: 0, value: 0 }),
        TailBs => Value::Short(0),
    }
}

/// ヘッダ変数の値（節の大きさの後から。R2010 以降の不明の RL の後）を読む。ビット数の RL から終わりまで
pub(crate) fn read_header_vars(data: &[u8], version: Version) -> Read<HeaderVars> {
    let mut bits = Bits::new(data);
    let mut s = if version >= Version::R2007 {
        // ビット数（RL 自身から文字列の並びの印まで）。ハンドルの並びはその後ろ
        let size = u64::from(bits.rl()?);
        let flag = size.checked_sub(1).ok_or(crate::bits::Eof)?;
        // 値の並びは文字列の並びの始まり（文字列がなければ印）まで
        let (text, end) = string_stream(data, flag);
        let strings = match text {
            crate::objects::Text::Stream(strings) => Some(strings),
            _ => None,
        };
        let mut handles = Bits::new(data);
        handles.set_pos(size);
        bits.set_end(end);
        In { data: bits, strings, handles: Some(handles), version }
    } else {
        In { data: bits, strings: None, handles: None, version }
    };
    let mut values: Vec<(&'static str, Value)> = Vec::with_capacity(FIELDS.len());
    for &(name, kind, when) in FIELDS {
        if !when.holds(version) || !present(name, &values) {
            continue;
        }
        let value = read_value(&mut s, kind)?;
        values.push((name, value));
    }
    #[cfg(test)]
    LEFTOVER.set([
        s.data.remaining(),
        s.strings.as_ref().map_or(0, Bits::remaining),
        s.handles.as_ref().map_or(0, Bits::remaining),
    ]);
    Ok(HeaderVars { version, values })
}

#[cfg(test)]
thread_local! {
    /// 最後に読んだヘッダ変数の、読み残した値・文字列・ハンドルのビットの数（テストで並びを確かめる）
    pub(crate) static LEFTOVER: std::cell::Cell<[u64; 3]> = const { std::cell::Cell::new([0; 3]) };
}

/// ヘッダ変数の値を書く（区切り・大きさ・CRC は sections の Framed が付ける）。終わりはバイトの境目まで 0
pub(crate) fn write_header_vars(vars: &HeaderVars) -> Vec<u8> {
    let mut bits = header_bits(vars);
    bits.pad();
    bits.into_bytes()
}

/// ヘッダ変数の値の、埋める前のビット
pub(crate) fn header_bits(vars: &HeaderVars) -> crate::write::bits::BitWriter {
    let version = vars.version;
    let r2007 = version >= Version::R2007;
    let mut out = Out::new(version, !r2007);
    let mut written: Vec<(&'static str, Value)> = Vec::with_capacity(FIELDS.len());
    for &(name, kind, when) in FIELDS {
        if !when.holds(version) || !present(name, &written) {
            continue;
        }
        let value = vars.get(name).cloned().unwrap_or_else(|| zero(kind, version));
        write_value(&mut out, kind, &value);
        written.push((name, value));
    }
    if !r2007 {
        return out.data;
    }
    let mut data = out.data;
    crate::write::object::append_strings(&mut data, &out.strings);
    let mut all = crate::write::bits::BitWriter::new();
    all.rl((32 + data.bit_len()) as u32);
    all.append(&data);
    all.append(&out.handles);
    all
}
