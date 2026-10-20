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
pub(crate) enum Kind {
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
    /// R2004 以降の BS の後の BL・BL・B（仕様にない。実物の図面で確かめた。ACadSharp も書く）
    TailBl,
    TailB,
}

/// 値がある版
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    All,
    /// R13・R14 だけ
    Before2000,
    /// R14 から
    From14,
    /// R2000 から
    From2000,
    /// R2004 より前（R13〜R2000）だけ
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
            When::Before2000 => version < Version::R2000,
            When::From14 => version >= Version::R14,
            When::From2000 => version >= Version::R2000,
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

/// 9 のヘッダ変数の並び。R13・R14 だけの値（DIMSAV・BLIPMODE など。R2000 からは登録簿にある）と R13・R14 の寸法の
/// 値（R2000 と別の位置で、RC と名前の文字列を含む）は Before2000。CPSNID は CEPSNTYPE が 3 のときだけある
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
    ("UNKNOWN_BS_R14", Bs, Before2000),
    ("VIEWPORT_ENTITY", H, Before2004),
    ("$DIMASO", B, All),
    ("$DIMSHO", B, All),
    ("$DIMSAV", B, Before2000),
    ("$PLINEGEN", B, All),
    ("$ORTHOMODE", B, All),
    ("$REGENMODE", B, All),
    ("$FILLMODE", B, All),
    ("$QTEXTMODE", B, All),
    ("$PSLTSCALE", B, All),
    ("$LIMCHECK", B, All),
    ("$BLIPMODE", B, Before2000),
    ("UNKNOWN_B1", B, From2004),
    ("$USRTIMER", B, All),
    ("$SKPOLY", B, All),
    ("$ANGDIR", B, All),
    ("$SPLFRAME", B, All),
    ("$ATTREQ", B, Before2000),
    ("$ATTDIA", B, Before2000),
    ("$MIRRTEXT", B, All),
    ("$WORLDVIEW", B, All),
    ("$WIREFRAME", B, Before2000),
    ("$TILEMODE", B, All),
    ("$PLIMCHECK", B, All),
    ("$VISRETAIN", B, All),
    ("$DELOBJ", B, Before2000),
    ("$DISPSILH", B, All),
    ("$PELLIPSE", B, All),
    ("$PROXYGRAPHICS", Bs, All),
    ("$DRAGMODE", Bs, Before2000),
    ("$TREEDEPTH", Bs, All),
    ("$LUNITS", Bs, All),
    ("$LUPREC", Bs, All),
    ("$AUNITS", Bs, All),
    ("$AUPREC", Bs, All),
    ("$OSMODE", Bs, Before2000),
    ("$ATTMODE", Bs, All),
    ("$COORDS", Bs, Before2000),
    ("$PDMODE", Bs, All),
    ("$PICKSTYLE", Bs, Before2000),
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
    ("$PSVPSCALE", Bd, From2000),
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
    ("$PUCSORTHOREF", H, From2000),
    ("$PUCSORTHOVIEW", Bs, From2000),
    ("$PUCSBASE", H, From2000),
    ("$PUCSORGTOP", Bd3, From2000),
    ("$PUCSORGBOTTOM", Bd3, From2000),
    ("$PUCSORGLEFT", Bd3, From2000),
    ("$PUCSORGRIGHT", Bd3, From2000),
    ("$PUCSORGFRONT", Bd3, From2000),
    ("$PUCSORGBACK", Bd3, From2000),
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
    ("$UCSORTHOREF", H, From2000),
    ("$UCSORTHOVIEW", Bs, From2000),
    ("$UCSBASE", H, From2000),
    ("$UCSORGTOP", Bd3, From2000),
    ("$UCSORGBOTTOM", Bd3, From2000),
    ("$UCSORGLEFT", Bd3, From2000),
    ("$UCSORGRIGHT", Bd3, From2000),
    ("$UCSORGFRONT", Bd3, From2000),
    ("$UCSORGBACK", Bd3, From2000),
    ("$DIMPOST", Tv, From2000),
    ("$DIMAPOST", Tv, From2000),
    ("$DIMTOL", B, Before2000),
    ("$DIMLIM", B, Before2000),
    ("$DIMTIH", B, Before2000),
    ("$DIMTOH", B, Before2000),
    ("$DIMSE1", B, Before2000),
    ("$DIMSE2", B, Before2000),
    ("$DIMALT", B, Before2000),
    ("$DIMTOFL", B, Before2000),
    ("$DIMSAH", B, Before2000),
    ("$DIMTIX", B, Before2000),
    ("$DIMSOXD", B, Before2000),
    ("$DIMALTD", Rc, Before2000),
    ("$DIMZIN", Rc, Before2000),
    ("$DIMSD1", B, Before2000),
    ("$DIMSD2", B, Before2000),
    ("$DIMTOLJ", Rc, Before2000),
    ("$DIMJUST", Rc, Before2000),
    ("$DIMFIT", Rc, Before2000),
    ("$DIMUPT", B, Before2000),
    ("$DIMTZIN", Rc, Before2000),
    ("$DIMALTZ", Rc, Before2000),
    ("$DIMALTTZ", Rc, Before2000),
    ("$DIMTAD", Rc, Before2000),
    ("$DIMUNIT", Bs, Before2000),
    ("$DIMAUNIT", Bs, Before2000),
    ("$DIMDEC", Bs, Before2000),
    ("$DIMTDEC", Bs, Before2000),
    ("$DIMALTU", Bs, Before2000),
    ("$DIMALTTD", Bs, Before2000),
    ("$DIMTXSTY", H, Before2000),
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
    ("$DIMTOL", B, From2000),
    ("$DIMLIM", B, From2000),
    ("$DIMTIH", B, From2000),
    ("$DIMTOH", B, From2000),
    ("$DIMSE1", B, From2000),
    ("$DIMSE2", B, From2000),
    ("$DIMTAD", Bs, From2000),
    ("$DIMZIN", Bs, From2000),
    ("$DIMAZIN", Bs, From2000),
    ("$DIMARCSYM", Bs, From2007),
    ("$DIMTXT", Bd, All),
    ("$DIMCEN", Bd, All),
    ("$DIMTSZ", Bd, All),
    ("$DIMALTF", Bd, All),
    ("$DIMLFAC", Bd, All),
    ("$DIMTVP", Bd, All),
    ("$DIMTFAC", Bd, All),
    ("$DIMGAP", Bd, All),
    ("$DIMPOST", Tv, Before2000),
    ("$DIMAPOST", Tv, Before2000),
    ("$DIMBLK", Tv, Before2000),
    ("$DIMBLK1", Tv, Before2000),
    ("$DIMBLK2", Tv, Before2000),
    ("$DIMALTRND", Bd, From2000),
    ("$DIMALT", B, From2000),
    ("$DIMALTD", Bs, From2000),
    ("$DIMTOFL", B, From2000),
    ("$DIMSAH", B, From2000),
    ("$DIMTIX", B, From2000),
    ("$DIMSOXD", B, From2000),
    ("$DIMCLRD", Cmc, All),
    ("$DIMCLRE", Cmc, All),
    ("$DIMCLRT", Cmc, All),
    ("$DIMADEC", Bs, From2000),
    ("$DIMDEC", Bs, From2000),
    ("$DIMTDEC", Bs, From2000),
    ("$DIMALTU", Bs, From2000),
    ("$DIMALTTD", Bs, From2000),
    ("$DIMAUNIT", Bs, From2000),
    ("$DIMFRAC", Bs, From2000),
    ("$DIMLUNIT", Bs, From2000),
    ("$DIMDSEP", Bs, From2000),
    ("$DIMTMOVE", Bs, From2000),
    ("$DIMJUST", Bs, From2000),
    ("$DIMSD1", B, From2000),
    ("$DIMSD2", B, From2000),
    ("$DIMTOLJ", Bs, From2000),
    ("$DIMTZIN", Bs, From2000),
    ("$DIMALTZ", Bs, From2000),
    ("$DIMALTTZ", Bs, From2000),
    ("$DIMUPT", B, From2000),
    ("$DIMATFIT", Bs, From2000),
    ("$DIMFXLON", B, From2007),
    ("$DIMTXTDIRECTION", B, From2010),
    ("$DIMALTMZF", Bd, From2010),
    ("$DIMALTMZS", Tv, From2010),
    ("$DIMMZF", Bd, From2010),
    ("$DIMMZS", Tv, From2010),
    ("$DIMTXSTY", H, From2000),
    ("$DIMLDRBLK", H, From2000),
    ("$DIMBLK", H, From2000),
    ("$DIMBLK1", H, From2000),
    ("$DIMBLK2", H, From2000),
    ("$DIMLTYPE", H, From2007),
    ("$DIMLTEX1", H, From2007),
    ("$DIMLTEX2", H, From2007),
    ("$DIMLWD", Bs, From2000),
    ("$DIMLWE", Bs, From2000),
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
    ("$TSTACKALIGN", Bs, From2000),
    ("$TSTACKSIZE", Bs, From2000),
    ("$HYPERLINKBASE", Tv, From2000),
    ("$STYLESHEET", Tv, From2000),
    ("DICTIONARY_LAYOUTS", H, From2000),
    ("DICTIONARY_PLOTSETTINGS", H, From2000),
    ("DICTIONARY_PLOTSTYLES", H, From2000),
    ("DICTIONARY_MATERIALS", H, From2004),
    ("DICTIONARY_COLORS", H, From2004),
    ("DICTIONARY_VISUALSTYLE", H, From2007),
    ("UNKNOWN_H1", H, From2013),
    ("FLAGS", Bl, From2000),
    ("$INSUNITS", Bs, From2000),
    ("$CEPSNTYPE", Bs, From2000),
    ("$CPSNID", H, From2000),
    ("$FINGERPRINTGUID", Tv, From2000),
    ("$VERSIONGUID", Tv, From2000),
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
    ("UNKNOWN_BS1", TailBs, From14),
    ("UNKNOWN_BS2", TailBs, From14),
    ("UNKNOWN_BS3", TailBs, From14),
    ("UNKNOWN_BS4", TailBs, From14),
    ("UNKNOWN_BL11", TailBl, From2004),
    ("UNKNOWN_BL12", TailBl, From2004),
    ("UNKNOWN_B3", TailB, From2004),
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

/// 名前のヘッダ変数のその版の形（R13・R14 は寸法の値などが別の位置・形）
pub(crate) fn field_kind(name: &str, version: Version) -> Option<Kind> {
    FIELDS.iter().find(|(key, _, when)| *key == name && when.holds(version)).map(|(_, kind, _)| *kind)
}

impl HeaderVars {
    /// 版にあるすべての値を、default の値（なければその形の 0）で作る
    pub fn with_defaults(version: Version, default: impl Fn(&str) -> Option<Value>) -> Self {
        let values = FIELDS
            .iter()
            .filter(|(_, _, when)| when.holds(version))
            .map(|&(name, kind, _)| (name, default(name).unwrap_or_else(|| zero(kind, version))))
            .collect();
        Self { version, values }
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.iter().find(|(key, _)| *key == name).map(|(_, value)| value)
    }

    /// 値を置き換える（その版にない値は無視する）
    pub fn set(&mut self, name: &str, value: Value) {
        if let Some(slot) = self.values.iter_mut().find(|(key, _)| *key == name) {
            slot.1 = value;
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
        TailBl => Value::Long(match &mut s.handles {
            Some(handles) => handles.bl()?,
            None => s.data.bl()?,
        }),
        TailB => Value::Bit(match &mut s.handles {
            Some(handles) => handles.bit()?,
            None => s.data.bit()?,
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
        (TailBl, Value::Long(value)) => {
            if out.inline_handles {
                out.data.bl(*value)
            } else {
                out.handles.bl(*value)
            }
        }
        (TailB, Value::Bit(value)) => {
            if out.inline_handles {
                out.data.bit(*value)
            } else {
                out.handles.bit(*value)
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
        TailBl => Value::Long(0),
        TailB => Value::Bit(false),
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
        In { data: bits, strings, handles: Some(handles), version, start: 0 }
    } else {
        In { data: bits, strings: None, handles: None, version, start: 0 }
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
