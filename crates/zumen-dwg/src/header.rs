//! ヘッダ変数（9「Data section AcDb:Header」）。図面の描き方に使う変数と、表・辞書のオブジェクトのハンドルを読む。
//! R2007 以降は、文字列とハンドルが値の後ろの別の並びにある（5.9）

use crate::bits::{Bits, Read};
use crate::objects::{Format, Streams, string_stream};

/// 読んだヘッダ変数
#[derive(Debug, Clone, Default)]
pub(crate) struct Header {
    pub ltscale: f64,
    pub textsize: f64,
    pub psltscale: bool,
    pub tilemode: bool,
    pub dimscale: f64,
    pub dimasz: f64,
    pub dimtxt: f64,
    pub insunits: f64,
    pub extmin: [f64; 3],
    pub extmax: [f64; 3],
    pub limmin: [f64; 2],
    pub limmax: [f64; 2],
    pub pextmin: [f64; 3],
    pub pextmax: [f64; 3],
    pub plimmin: [f64; 2],
    pub plimmax: [f64; 2],
    pub block_control: u64,
    pub layer_control: u64,
    pub style_control: u64,
    pub linetype_control: u64,
    pub appid_control: u64,
    pub dimstyle_control: u64,
    pub mlinestyle_dictionary: u64,
    pub named_objects: u64,
    pub layouts: u64,
    /// 色見本の色（DBCOLOR）の辞書（R2004 以降）
    pub colors: u64,
    pub paper_space: u64,
    pub model_space: u64,
    pub bylayer: u64,
    pub byblock: u64,
    pub continuous: u64,
    /// 次に振るハンドル（HANDSEED）と、その値の節の中のビットの範囲（その場の編集が書き換える）
    pub handseed: u64,
    pub handseed_at: (u64, u64),
    pub view_control: u64,
    pub ucs_control: u64,
    pub vport_control: u64,
    /// ビューポートの図形のヘッダの表（R2000 まで）
    pub vpent_control: u64,
    /// グループの辞書（ACAD_GROUP）
    pub groups: u64,
}

/// ヘッダ変数の節を読む。途中で読めなくなったら Err（それまでの値は使わない）
pub(crate) fn read_header(section: &[u8], format: Format) -> Read<Header> {
    let mut data = Bits::new(section);
    data.skip(16 * 8)?;
    let size = u64::from(data.rl()?);
    if (format.r2010() && format.maintenance > 3) || format.r2018() {
        data.rl()?;
    }
    let start = data.pos();
    data.set_end(start + size * 8);
    let mut streams = if format.r2007() {
        // R2007 以降: RL のビット数で値の並びの終わりが決まり、その後ろにハンドルの並び、前に文字列の並びがある
        let bits = u64::from(data.rl()?);
        let flag = start + bits - 1;
        let (text, _) = string_stream(section, flag);
        let mut handles = Bits::new(section);
        handles.set_pos(start + bits);
        handles.set_end(start + size * 8);
        data.set_end(flag);
        Streams::with_text(data, text, handles, format)
    } else {
        Streams::inline(data, format)
    };
    read_variables(&mut streams)
}

fn read_variables(s: &mut Streams<'_>) -> Read<Header> {
    let f = s.format;
    let mut h = Header::default();
    if f.r2013() {
        s.data.bll()?;
    }
    for _ in 0..4 {
        s.data.bd()?;
    }
    for _ in 0..4 {
        s.tv()?;
    }
    s.data.bl()?;
    s.data.bl()?;
    // R13・R14 だけの値（不明の BS、DIMSAV、BLIPMODE など。R2000 からは登録簿にある）は r14 が真のときに読む
    let r14 = !f.r2000();
    if r14 {
        s.data.bs()?;
    }
    if !f.r2004() {
        // 今のビューポートの図形のヘッダ
        s.handle()?;
    }
    // DIMASO、DIMSHO、（R13・R14）DIMSAV、PLINEGEN、ORTHOMODE、REGENMODE、FILLMODE、QTEXTMODE
    for _ in 0..if r14 { 8 } else { 7 } {
        s.data.bit()?;
    }
    h.psltscale = s.data.bit()?;
    // LIMCHECK、（R13・R14）BLIPMODE、（R2004 以降）不明
    s.data.bit()?;
    if r14 || f.r2004() {
        s.data.bit()?;
    }
    // USRTIMER、SKPOLY、ANGDIR、SPLFRAME、（R13・R14）ATTREQ、ATTDIA、MIRRTEXT、WORLDVIEW、（R13・R14）WIREFRAME
    for _ in 0..if r14 { 9 } else { 6 } {
        s.data.bit()?;
    }
    h.tilemode = s.data.bit()?;
    // PLIMCHECK、VISRETAIN、（R13・R14）DELOBJ、DISPSILH、PELLIPSE
    for _ in 0..if r14 { 5 } else { 4 } {
        s.data.bit()?;
    }
    // PROXYGRAPHICS、（R13・R14）DRAGMODE、TREEDEPTH、LUNITS、LUPREC、AUNITS、AUPREC、（R13・R14）OSMODE、ATTMODE、
    // （R13・R14）COORDS、PDMODE、（R13・R14）PICKSTYLE
    for _ in 0..if r14 { 12 } else { 8 } {
        s.data.bs()?;
    }
    if f.r2004() {
        for _ in 0..3 {
            s.data.bl()?;
        }
    }
    // USERI1〜5、SPLINESEGS、SURFU、SURFV、SURFTYPE、SURFTAB1、SURFTAB2、SPLINETYPE、SHADEDGE、SHADEDIF、
    // UNITMODE、MAXACTVP、ISOLINES、CMLJUST、TEXTQLTY
    for _ in 0..19 {
        s.data.bs()?;
    }
    h.ltscale = s.data.bd()?;
    h.textsize = s.data.bd()?;
    // TRACEWID、SKETCHINC、FILLETRAD、THICKNESS、ANGBASE、PDSIZE、PLINEWID、USERR1〜5、CHAMFERA〜D、FACETRES、
    // CMLSCALE、CELTSCALE
    for _ in 0..19 {
        s.data.bd()?;
    }
    // MENUNAME（R2007 以降は文字列の並びの中。ACadSharp と同じく読む）
    s.tv()?;
    // TDCREATE、TDUPDATE
    for _ in 0..4 {
        s.data.bl()?;
    }
    if f.r2004() {
        for _ in 0..3 {
            s.data.bl()?;
        }
    }
    // TDINDWG、TDUSRTIMER
    for _ in 0..4 {
        s.data.bl()?;
    }
    s.cmc()?;
    // HANDSEED は R2007 以降も値の並びの中
    let at = s.data.pos();
    h.handseed = s.data.handle()?.value;
    h.handseed_at = (at, s.data.pos());
    // CLAYER、TEXTSTYLE、CELTYPE、（R2007 以降）CMATERIAL、DIMSTYLE、CMLSTYLE
    for _ in 0..if f.r2007() { 6 } else { 5 } {
        s.handle()?;
    }
    // PSVPSCALE（R2000 から）
    if !r14 {
        s.data.bd()?;
    }
    // 紙空間: INSBASE、EXTMIN、EXTMAX、LIMMIN、LIMMAX、ELEVATION、UCSORG、UCSXDIR、UCSYDIR、UCSNAME
    s.data.bd3()?;
    h.pextmin = s.data.bd3()?;
    h.pextmax = s.data.bd3()?;
    h.plimmin = s.data.rd2()?;
    h.plimmax = s.data.rd2()?;
    s.data.bd()?;
    for _ in 0..3 {
        s.data.bd3()?;
    }
    s.handle()?;
    // PUCSORTHOREF、PUCSORTHOVIEW、PUCSBASE、PUCSORGTOP〜BACK（R2000 から）
    if !r14 {
        s.handle()?;
        s.data.bs()?;
        s.handle()?;
        for _ in 0..6 {
            s.data.bd3()?;
        }
    }
    // モデル空間
    s.data.bd3()?;
    h.extmin = s.data.bd3()?;
    h.extmax = s.data.bd3()?;
    h.limmin = s.data.rd2()?;
    h.limmax = s.data.rd2()?;
    s.data.bd()?;
    for _ in 0..3 {
        s.data.bd3()?;
    }
    s.handle()?;
    if !r14 {
        // UCSORTHOREF、UCSORTHOVIEW、UCSBASE、UCSORGTOP〜BACK、DIMPOST、DIMAPOST
        s.handle()?;
        s.data.bs()?;
        s.handle()?;
        for _ in 0..6 {
            s.data.bd3()?;
        }
        s.tv()?;
        s.tv()?;
    } else {
        // R13・R14 の寸法の値: DIMTOL、DIMLIM、DIMTIH、DIMTOH、DIMSE1、DIMSE2、DIMALT、DIMTOFL、DIMSAH、DIMTIX、DIMSOXD、
        // DIMALTD（RC）、DIMZIN（RC）、DIMSD1、DIMSD2、DIMTOLJ（RC）、DIMJUST（RC）、DIMFIT（RC）、DIMUPT、DIMTZIN（RC）、
        // DIMALTZ（RC）、DIMALTTZ（RC）、DIMTAD（RC）、DIMUNIT、DIMAUNIT、DIMDEC、DIMTDEC、DIMALTU、DIMALTTD、DIMTXSTY
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
        s.handle()?;
    }
    h.dimscale = s.data.bd()?;
    h.dimasz = s.data.bd()?;
    // DIMEXO、DIMDLI、DIMEXE、DIMRND、DIMDLE、DIMTP、DIMTM
    for _ in 0..7 {
        s.data.bd()?;
    }
    if f.r2007() {
        // DIMFXL、DIMJOGANG、DIMTFILL、DIMTFILLCLR
        s.data.bd()?;
        s.data.bd()?;
        s.data.bs()?;
        s.cmc()?;
    }
    // DIMTOL、DIMLIM、DIMTIH、DIMTOH、DIMSE1、DIMSE2、DIMTAD、DIMZIN、DIMAZIN（R2000 から）
    if !r14 {
        for _ in 0..6 {
            s.data.bit()?;
        }
        for _ in 0..3 {
            s.data.bs()?;
        }
    }
    if f.r2007() {
        s.data.bs()?;
    }
    h.dimtxt = s.data.bd()?;
    // DIMCEN、DIMTSZ、DIMALTF、DIMLFAC、DIMTVP、DIMTFAC、DIMGAP
    for _ in 0..7 {
        s.data.bd()?;
    }
    if r14 {
        // DIMPOST、DIMAPOST、DIMBLK、DIMBLK1、DIMBLK2（R13・R14 は名前の文字列）
        for _ in 0..5 {
            s.tv()?;
        }
    } else {
        // DIMALTRND、DIMALT、DIMALTD、DIMTOFL、DIMSAH、DIMTIX、DIMSOXD
        s.data.bd()?;
        s.data.bit()?;
        s.data.bs()?;
        for _ in 0..4 {
            s.data.bit()?;
        }
    }
    // DIMCLRD、DIMCLRE、DIMCLRT
    for _ in 0..3 {
        s.cmc()?;
    }
    if r14 {
        return read_controls(s, h);
    }
    // DIMADEC、DIMDEC、DIMTDEC、DIMALTU、DIMALTTD、DIMAUNIT、DIMFRAC、DIMLUNIT、DIMDSEP、DIMTMOVE、DIMJUST
    for _ in 0..11 {
        s.data.bs()?;
    }
    // DIMSD1、DIMSD2、DIMTOLJ、DIMTZIN、DIMALTZ、DIMALTTZ、DIMUPT、DIMATFIT
    s.data.bit()?;
    s.data.bit()?;
    for _ in 0..4 {
        s.data.bs()?;
    }
    s.data.bit()?;
    s.data.bs()?;
    if f.r2007() {
        s.data.bit()?;
    }
    if f.r2010() {
        // DIMTXTDIRECTION、DIMALTMZF、DIMALTMZS、DIMMZF、DIMMZS
        s.data.bit()?;
        s.data.bd()?;
        s.tv()?;
        s.data.bd()?;
        s.tv()?;
    }
    // DIMTXSTY、DIMLDRBLK、DIMBLK、DIMBLK1、DIMBLK2、（R2007 以降）DIMLTYPE、DIMLTEX1、DIMLTEX2
    for _ in 0..if f.r2007() { 8 } else { 5 } {
        s.handle()?;
    }
    // DIMLWD、DIMLWE
    s.data.bs()?;
    s.data.bs()?;
    read_controls(s, h)
}

/// 表の管理のオブジェクトから終わりまで（R13・R14 は R2000 からの値を除く）
fn read_controls(s: &mut Streams<'_>, mut h: Header) -> Read<Header> {
    let f = s.format;
    h.block_control = s.handle()?.value;
    h.layer_control = s.handle()?.value;
    h.style_control = s.handle()?.value;
    h.linetype_control = s.handle()?.value;
    h.view_control = s.handle()?.value;
    h.ucs_control = s.handle()?.value;
    h.vport_control = s.handle()?.value;
    h.appid_control = s.handle()?.value;
    h.dimstyle_control = s.handle()?.value;
    if !f.r2004() {
        // ビューポートの図形のヘッダの表（R13〜R2000）
        h.vpent_control = s.handle()?.value;
    }
    // ACAD_GROUP
    h.groups = s.handle()?.value;
    h.mlinestyle_dictionary = s.handle()?.value;
    h.named_objects = s.handle()?.value;
    if !f.r2000() {
        return read_tail_handles(s, h);
    }
    // TSTACKALIGN、TSTACKSIZE、HYPERLINKBASE、STYLESHEET
    s.data.bs()?;
    s.data.bs()?;
    s.tv()?;
    s.tv()?;
    h.layouts = s.handle()?.value;
    // PLOTSETTINGS、PLOTSTYLES、（R2004 以降）MATERIALS、COLORS、（R2007 以降）VISUALSTYLE、（R2013 以降）不明
    s.handle()?;
    s.handle()?;
    let mut dictionaries = 0;
    if f.r2004() {
        s.handle()?;
        h.colors = s.handle()?.value;
    }
    if f.r2007() {
        dictionaries += 1;
    }
    if f.r2013() {
        dictionaries += 1;
    }
    for _ in 0..dictionaries {
        s.handle()?;
    }
    // CELWEIGHT などのフラグ
    s.data.bl()?;
    h.insunits = f64::from(s.data.bs()?);
    if s.data.bs()? == 3 {
        s.handle()?;
    }
    // FINGERPRINTGUID、VERSIONGUID
    s.tv()?;
    s.tv()?;
    if f.r2004() {
        // SORTENTS、INDEXCTL、HIDETEXT、XCLIPFRAME、DIMASSOC、HALOGAP
        for _ in 0..6 {
            s.data.rc()?;
        }
        // OBSCUREDCOLOR、INTERSECTIONCOLOR
        s.data.bs()?;
        s.data.bs()?;
        // OBSCUREDLTYPE、INTERSECTIONDISPLAY、PROJECTNAME
        s.data.rc()?;
        s.data.rc()?;
        s.tv()?;
    }
    read_tail_handles(s, h)
}

/// 紙空間・モデル空間の BLOCK_HEADER と BYLAYER・BYBLOCK・CONTINUOUS の線種
fn read_tail_handles(s: &mut Streams<'_>, mut h: Header) -> Read<Header> {
    h.paper_space = s.handle()?.value;
    h.model_space = s.handle()?.value;
    h.bylayer = s.handle()?.value;
    h.byblock = s.handle()?.value;
    h.continuous = s.handle()?.value;
    Ok(h)
}

/// MEASUREMENT（22「AcDb:Template」）。説明の長さ（RS）・説明・MEASUREMENT（RS）
pub(crate) fn read_measurement(section: &[u8]) -> Option<f64> {
    let mut reader = Bits::new(section);
    let length = reader.rs().ok()?;
    reader.skip(u64::from(length) * 8).ok()?;
    reader.rs().ok().map(f64::from)
}
