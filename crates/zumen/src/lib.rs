//! zumen: 建築・土木の 2D 図面を読み、形式に依存しない表示用モデル（`Scene`）にする。
//!
//! 形式は拡張子ではなく中身で判定する。読める形式は SXF（SFC・P21、圧縮した P2Z・SFZ）・JWW（JWC・JWS も）・DXF・DWG・
//! PDF（読むだけ。ページがビューで、図形はページの下敷きが描く）。
//! 受け取った図面を信用できないときは、`ReadDrawingOptions::limits` でバイト数・図形の数・時間の上限を指定する。
//! PDF 以外の図面は AutoCAD の図面のモデルを経て、DXF・DWG・JWW・SXF に書き直せる（`convert`）。
//! 図面をその場で編集し（`edit`）、新しい図面を作れる（`create`）。操作は形式によらない（`edit::parse_operations`）。
//! 図面の仕様（JSON）から、用紙・ビュー・寸法・表のそろった図面を描ける（`draft`、決定 0018）。
//!
//! ```no_run
//! let bytes = std::fs::read("plan.jww").unwrap();
//! let scene = zumen::read(&bytes, &zumen::ReadDrawingOptions::default()).unwrap();
//! println!("{} entities", scene.entities.len());
//!
//! let dwg = std::fs::read("plan.dwg").unwrap();
//! let converted = zumen::convert(&dwg, "dxf", &zumen::ReadDrawingOptions::default()).unwrap();
//! std::fs::write("plan.dxf", &converted.bytes).unwrap();
//! ```
#![forbid(unsafe_code)]

pub use zumen_convert as mapping;
pub use zumen_core::*;
pub use zumen_draft as draft;
pub use zumen_dwg as dwg;
pub use zumen_dxf as dxf;
pub use zumen_jww as jww;
#[cfg(feature = "pdf")]
pub use zumen_pdf as pdf;
pub use zumen_sxf as sxf;

/// 組み込みの読み取り（判定する順）。PDF は機能 pdf のとき
pub fn readers() -> Vec<&'static dyn DrawingReader> {
    vec![
        &zumen_sxf::SXF,
        &zumen_jww::JWW,
        &zumen_jww::JWC,
        &zumen_jww::JWS,
        &zumen_dxf::DXF,
        &zumen_dwg::DWG,
        #[cfg(feature = "pdf")]
        &zumen_pdf::PDF,
    ]
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadDrawingOptions {
    /// 分かっている場合の形式（sfc・p21・p2z・sfz・jww・jwc・jws・dxf・dwg・pdf）。省略すると中身から判定する
    pub format: Option<String>,
    /// 描く表示（Scene.views の id。DXF・DWG のレイアウト、PDF のページの番号）。省略すると形式ごとの既定の表示
    pub view: Option<String>,
    /// 読み取りの上限。超えたら途中でやめ、`ReadError::exceeded` で上限の種類が分かるエラーを返す
    pub limits: ReadLimits,
}

/// 図面の形式を中身から判定する。読めない形式なら None
pub fn detect(bytes: &[u8]) -> Option<&'static str> {
    detect_format(bytes, &readers())
}

/// PDF は読むだけ（決定 0020）。図面は zumen-render の render_pdf で PDF に書ける
const READ_ONLY_PDF: &str =
    "PDF の図面は読むだけで、書き直し・編集・描き込みはできません（図面を PDF にするには描画の PDF 出力を使います）";

/// 書き出した図面と、書けなかった・形を変えて書いた内容（読み取りの notices と同じ形）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub notices: Vec<Notice>,
}

/// 書き出せる形式。dxf は AutoCAD 2013 の DXF（版を選べる）、dwg は AutoCAD 2013 の DWG、jww は Jw_cad 8 の JWW、
/// sfc・p21（圧縮した sfz・p2z）は SXF Ver.3.1（版を選べる）
pub const WRITABLE_FORMATS: [&str; 7] = ["dxf", "dwg", "jww", "sfc", "p21", "sfz", "p2z"];

/// 図面を読み、別の形式に書き直す。`to` は書く形式（`WRITABLE_FORMATS`）。どの図面も AutoCAD の図面のモデル
/// （`dxf::DxfDocument`）を経る。DXF・DWG はそのまま読み、SXF・JWW（JWC・JWS）はモデルに写す（`mapping`。モデル空間は
/// 実寸、縮尺ごとに用紙のレイアウトのビューポート）。DXF・DWG はモデルをそのまま書き、JWW はモデルを JWW の図面に
/// （`mapping::dxf_to_jww`）、SXF はモデルを SXF の図面に写して書く（`mapping::dxf_to_sxf`。ビューポートは部分図、
/// ブロックは作図部品）
pub fn convert(bytes: &[u8], to: &str, options: &ReadDrawingOptions) -> Result<Converted, ReadError> {
    convert_with(bytes, to, options, &WriteOptions::default())
}

/// 書き出しの指定
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// 書く版。DXF は R12・2000・2004・2007・2010・2013・2018（`dxf::DxfVersion::parse` が読む名前）、SXF は
    /// 2.0・3.0・3.1（`sxf::SxfVersion::parse`）。DWG と JWW は選べない。省略すると形式の既定（DXF は 2013、SXF は 3.1）
    pub version: Option<String>,
}

/// 書く形式と版
enum Target {
    Dxf(dxf::DxfVersion),
    Dwg,
    Jww,
    Sxf(&'static str, sxf::SxfVersion),
}

impl Target {
    fn parse(to: &str, version: Option<&str>) -> Result<Target, ReadError> {
        let Some(&to) = WRITABLE_FORMATS.iter().find(|format| **format == to) else {
            let writable: Vec<String> = WRITABLE_FORMATS.iter().map(|format| format.to_uppercase()).collect();
            return Err(ReadError::new(format!(
                "{} には書き出せません（書き出せるのは {}）",
                to.to_uppercase(),
                writable.join("・")
            )));
        };
        Ok(match (to, version) {
            ("dxf", None) => Target::Dxf(dxf::DxfVersion::R2013),
            ("dxf", Some(name)) => Target::Dxf(dxf::DxfVersion::parse(name).ok_or_else(|| {
                ReadError::new(format!(
                    "DXF の版 {name} は書けません（書けるのは R12・2000・2004・2007・2010・2013・2018）"
                ))
            })?),
            ("sfc" | "p21" | "sfz" | "p2z", None) => Target::Sxf(to, sxf::SxfVersion::V3_1),
            ("sfc" | "p21" | "sfz" | "p2z", Some(name)) => Target::Sxf(
                to,
                sxf::SxfVersion::parse(name).ok_or_else(|| {
                    ReadError::new(format!("SXF の版 {name} は書けません（書けるのは 2.0・3.0・3.1）"))
                })?,
            ),
            ("dwg", None) => Target::Dwg,
            ("jww", None) => Target::Jww,
            (other, Some(_)) => {
                return Err(ReadError::new(format!("{} の版はまだ選べません", other.to_uppercase())));
            }
            _ => unreachable!(),
        })
    }
}

/// `convert` に書き出しの指定（版など）を加えたもの
pub fn convert_with(
    bytes: &[u8],
    to: &str,
    options: &ReadDrawingOptions,
    write: &WriteOptions,
) -> Result<Converted, ReadError> {
    let target = Target::parse(to, write.version.as_deref())?;
    let limits = &options.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(limit_error(Limit::Bytes, limits));
    }
    let format = match options.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| ReadError::new("対応していない形式のファイルです"))?,
    };
    let mut notices = NoticeList::new();
    let (doc, exceeded) = zumen_core::limits::with_limits(limits, || -> Result<_, ReadError> {
        let jww = match format {
            "dxf" => return Ok(dxf::parse_dxf(bytes)),
            "dwg" => return dwg::read_dwg(bytes).map_err(ReadError::new),
            "jww" => jww::read_jww(bytes)?,
            "jwc" => jww::read_jwc(bytes)?,
            // JWS（図形）は図形の内容を JWW の図面と同じに写す
            "jws" => jww::read_jws(bytes)?.content,
            format if sxf::SXF.formats().contains(&format) => {
                let mapped = mapping::sxf_to_dxf(&sxf::parse_sxf(bytes, format)?);
                notices = NoticeList::from_notices(&mapped.notices);
                return Ok(mapped.document);
            }
            "pdf" => return Err(ReadError::new(READ_ONLY_PDF)),
            other => return Err(ReadError::new(format!("{} の図面は書き直せません", other.to_uppercase()))),
        };
        let mapped = mapping::jww_to_dxf(&jww);
        notices = NoticeList::from_notices(&mapped.notices);
        Ok(mapped.document)
    });
    // 上限（バイト数・時間）を超えたら、途中まで読んだ図面は書かない
    if let Some(limit) = exceeded {
        return Err(limit_error(limit, limits));
    }
    let doc = doc?;
    let (written, exceeded) = zumen_core::limits::with_limits(limits, || write_document(&doc, &target));
    if let Some(limit) = exceeded {
        return Err(limit_error(limit, limits));
    }
    let (bytes, written) = written?;
    for notice in &written {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    Ok(Converted { bytes, notices: notices.into_vec() })
}

/// AutoCAD の図面のモデルを、形式と版に書く。返すのは書いたバイト列と、書けなかった・形を変えて書いた内容
fn write_document(doc: &dxf::DxfDocument, target: &Target) -> Result<(Vec<u8>, Vec<Notice>), ReadError> {
    Ok(match *target {
        Target::Dxf(version) => {
            let written = dxf::write_dxf_as(doc, version);
            (written.bytes, written.notices)
        }
        Target::Dwg => {
            let written = dwg::write_dwg(doc);
            (written.bytes, written.notices)
        }
        Target::Jww => {
            let mapped = mapping::dxf_to_jww(doc);
            let written = mapped.write();
            (written.bytes, [mapped.notices, written.notices].concat())
        }
        Target::Sxf(format, version) => {
            let mapped = mapping::dxf_to_sxf(doc);
            let written = sxf::write_sxf_version(&mapped.drawing, format, version)?;
            (written.bytes, [mapped.notices, written.notices].concat())
        }
    })
}

/// `draft` の指定
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DraftOptions {
    pub write: WriteOptions,
    /// 描く用紙の名前。省略するとすべての用紙（JWW と SXF は用紙を 1 枚しか持たないので、1 枚目だけを書く）
    pub sheet: Option<String>,
}

/// 図面の仕様（JSON、決定 0018）から、用紙・縮尺のあるビュー・寸法・ハッチング・表・表題欄のそろった図面を描き、
/// `to` の形式（`WRITABLE_FORMATS`）と `write.version` の版で書く。仕様の読み違いは、場所（`sheets[0].items[2]`）と
/// 変わらない code のある `draft::DraftError` で返す
pub fn draft(spec: &str, to: &str, options: &DraftOptions) -> Result<Converted, draft::DraftError> {
    let target = Target::parse(to, options.write.version.as_deref())
        .map_err(|error| draft::DraftError::new("unsupported-format", "", error.to_string()))?;
    let mut value: serde_json::Value = serde_json::from_str(spec)
        .map_err(|error| draft::DraftError::new("invalid-spec", "", format!("仕様の JSON を読めません: {error}")))?;
    if let Some(sheet) = &options.sheet {
        draft::select_sheet(&mut value, sheet)?;
    }
    let drafted = draft::draft(&value)?;
    let (bytes, written) = write_document(&drafted.document, &target)
        .map_err(|error| draft::DraftError::new("unwritable", "", error.to_string()))?;
    let mut notices = NoticeList::from_notices(&drafted.notices);
    for notice in &written {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    Ok(Converted { bytes, notices: notices.into_vec() })
}

/// 受け取った図面に描く指定
#[derive(Debug, Clone, Default)]
pub struct DraftIntoOptions {
    /// 描く用紙の名前。省略するとすべての用紙（JWW・SXF は用紙を 1 枚しか持たないので、1 枚目だけ）
    pub sheet: Option<String>,
    /// 描き込む図面の形式（dxf・dwg・jww・sfc・p21・sfz・p2z）と、読み取りの上限
    pub read: ReadDrawingOptions,
}

/// 描き込む先の図面
enum Template {
    Dxf(Box<dxf::DxfDocument>),
    Jww(Box<jww::JwwDocument>),
    Sxf(Box<sxf::SxfDrawing>),
}

/// 図面の仕様（決定 0018）を、受け取った図面（テンプレート）にその場で描く。描いたものは決定 0017 の編集の操作
/// （`draft::plan`、JWW は `draft::plan_jww`、SXF は `draft::plan_sxf`）にして `edit` で当てるので、元の図形・
/// ハンドル・拡張データ・zumen が読まない内容は残る。図面の版と形式は元のまま。
///
/// - DXF・DWG: 用紙の名前と同じレイアウトがあればそこに描き（用紙の大きさもそのレイアウトのもの）、なければ
///   レイアウトを足す。用紙の `fields` は、その用紙のブロック参照（表題欄）の属性を埋める。
/// - JWW・SXF（決定 0027）: 図面の 1 枚の用紙に、1 枚目（か `sheet`）の用紙を描く。ビューは JWW ではその縮尺の
///   レイヤグループ、SXF ではビューごとの部分図に描く
pub fn draft_into(bytes: &[u8], spec: &str, options: &DraftIntoOptions) -> Result<Converted, draft::DraftError> {
    let error = |code: &'static str, message: String| draft::DraftError::new(code, "", message);
    let limits = &options.read.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(error("too-large", limit_error(Limit::Bytes, limits).to_string()));
    }
    let format = match options.read.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| error("unreadable", "対応していない形式のファイルです".into()))?,
    };
    if format == "pdf" {
        return Err(error("unsupported-format", READ_ONLY_PDF.into()));
    }
    // 編集できる形式なら描き込める
    if !EDITABLE_FORMATS.contains(&format) {
        return Err(error(
            "unsupported-format",
            format!("{} の図面には描き込めません（描き込めるのは DXF・DWG・JWW・SXF）", format.to_uppercase()),
        ));
    }
    let mut value: serde_json::Value = serde_json::from_str(spec)
        .map_err(|e| draft::DraftError::new("invalid-spec", "", format!("仕様の JSON を読めません: {e}")))?;
    if let Some(sheet) = &options.sheet {
        draft::select_sheet(&mut value, sheet)?;
    }
    let (template, exceeded) = zumen_core::limits::with_limits(limits, || -> Result<Template, String> {
        Ok(match format {
            "dxf" => Template::Dxf(Box::new(dxf::parse_dxf(bytes))),
            "dwg" => Template::Dxf(Box::new(dwg::read_dwg(bytes)?)),
            "jww" => Template::Jww(Box::new(jww::read_jww(bytes).map_err(|error| error.to_string())?)),
            _ => Template::Sxf(Box::new(sxf::parse_sxf(bytes, format).map_err(|error| error.to_string())?)),
        })
    });
    if let Some(limit) = exceeded {
        return Err(error("too-large", limit_error(limit, limits).to_string()));
    }
    let template = template.map_err(|message| error("unreadable", message.to_string()))?;
    let planned = match &template {
        Template::Dxf(template) => draft::plan(&value, template)?,
        Template::Jww(template) => draft::plan_jww(&value, template)?,
        Template::Sxf(template) => draft::plan_sxf(&value, template)?,
    };
    let operations = edit::operations_from_json(&planned.operations)
        .map_err(|e| error("unwritable", format!("描き込む操作を作れませんでした: {e}")))?;
    let edit_options = ReadDrawingOptions { format: Some(format.to_string()), ..options.read.clone() };
    let edited = edit(bytes, &operations, &edit_options).map_err(|e| {
        // 編集の誤りは、その操作の元の仕様の図形の場所で返す
        let path = e.operation.and_then(|index| planned.sources.get(index)).cloned().unwrap_or_default();
        draft::DraftError::new(e.code, &path, e.message)
    })?;
    let mut notices = NoticeList::from_notices(&planned.notices);
    for notice in &edited.notices {
        notices.add_count(notice.level, &notice.code, notice.message.clone(), notice.count);
    }
    Ok(Converted { bytes: edited.bytes, notices: notices.into_vec() })
}

/// その場で編集できる形式
pub const EDITABLE_FORMATS: [&str; 7] = ["dxf", "dwg", "jww", "sfc", "p21", "sfz", "p2z"];

/// 図面をその場で編集する（docs/decisions/0012）。元のファイルを保ち、操作した図形だけを書き換えるので、
/// 形式・版・文字コードと、読み取りが扱わない内容（拡張データ、未対応の図形・オブジェクトなど）は元のまま残る。
/// すべての操作を確かめてから書き、1 つでも通らなければ何も書かずにエラーを返す。
/// `options.format` は分かっている場合の形式、`options.limits.max_bytes` は受け付ける大きさの上限
pub fn edit(
    bytes: &[u8],
    operations: &[edit::EditOperation],
    options: &ReadDrawingOptions,
) -> Result<edit::Edited, edit::EditError> {
    let limits = &options.limits;
    if limits.max_bytes.is_some_and(|max| bytes.len() as u64 > max) {
        return Err(edit::EditError::new("too-large", limit_error(Limit::Bytes, limits).to_string()));
    }
    let format = match options.format.as_deref() {
        Some(format) => format,
        None => detect(bytes).ok_or_else(|| edit::EditError::new("unreadable", "対応していない形式のファイルです"))?,
    };
    match format {
        "dxf" => dxf::edit_dxf(bytes, operations),
        "dwg" => dwg::edit_dwg(bytes, operations),
        "jww" => jww::edit_jww(bytes, operations),
        "sfc" | "p21" | "sfz" | "p2z" => sxf::edit_sxf(bytes, format, operations),
        "pdf" => Err(edit::EditError::new("unsupported-format", READ_ONLY_PDF)),
        other => Err(unsupported_edit(other)),
    }
}

/// 新しい図面を作り、操作（add・add-layer など）を当てる。to は形式（`EDITABLE_FORMATS`）、units は長さの単位
/// （mm、cm、m、inch、foot、unitless）。JWW と SXF の長さはいつも mm なので、mm 以外は受け付けない
pub fn create(to: &str, units: &str, operations: &[edit::EditOperation]) -> Result<edit::Edited, edit::EditError> {
    match to {
        "dxf" => dxf::edit_dxf(&dxf::new_dxf(units)?, operations),
        "dwg" => dwg::edit_dwg(&dwg::new_dwg(units)?, operations),
        "jww" => jww::edit_jww(&jww::new_jww(), operations_in_mm(to, units, operations)?),
        "sfc" | "p21" | "sfz" | "p2z" => {
            sxf::edit_sxf(&sxf::new_sxf(to)?, to, operations_in_mm(to, units, operations)?)
        }
        other => Err(unsupported_edit(other)),
    }
}

/// 長さが mm だけの形式で、units が mm か確かめる
fn operations_in_mm<'a>(
    to: &str,
    units: &str,
    operations: &'a [edit::EditOperation],
) -> Result<&'a [edit::EditOperation], edit::EditError> {
    if units.trim().eq_ignore_ascii_case("mm") {
        return Ok(operations);
    }
    Err(edit::EditError::new(
        "invalid-value",
        format!("{} の図面の長さは mm だけです（{units} は選べません）", to.to_uppercase()),
    ))
}

fn unsupported_edit(format: &str) -> edit::EditError {
    let editable: Vec<String> = EDITABLE_FORMATS.iter().map(|format| format.to_uppercase()).collect();
    edit::EditError::new(
        "unsupported-format",
        format!("{} の図面はまだ編集できません（編集できるのは {}）", format.to_uppercase(), editable.join("・")),
    )
}

/// `carry_markup` の指定
#[derive(Debug, Clone)]
pub struct CarryOptions {
    /// 比べ方。既定では隠したレイヤの図形も比べる（項目はどのレイヤの図形も指せる）
    pub compare: compare::CompareOptions,
    /// 前の版のビューの id から、新しい版のビューの id への対応（DXF のレイアウトと、そこから印刷した PDF のページなど）。
    /// 対応のないビューは同じ id のビューへ移す
    pub views: std::collections::HashMap<String, String>,
    /// 2 つの版を読む上限
    pub limits: ReadLimits,
    /// 文書の drawing.sha256 が前の版のバイトの SHA-256 と違っても移す。false なら `drawing-mismatch` の誤り
    pub ignore_hash: bool,
}

impl Default for CarryOptions {
    fn default() -> Self {
        Self {
            compare: compare::CompareOptions { all_layers: true, ..compare::CompareOptions::default() },
            views: std::collections::HashMap::new(),
            limits: ReadLimits::default(),
            ignore_hash: false,
        }
    }
}

/// 新しい版へ移したマークアップの文書と、印（carried.state）ごとの項目の数
#[derive(Debug, Clone, PartialEq)]
pub struct CarriedMarkup {
    pub document: markup::MarkupDocument,
    pub unchanged: usize,
    pub changed: usize,
    pub removed: usize,
}

/// マークアップを新しい版へ移せなかった。code は drawing-mismatch（文書が前の版のものでない）、unreadable（読めない）、
/// too-large（上限を超えた）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarryError {
    pub code: &'static str,
    pub message: String,
}

impl std::fmt::Display for CarryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CarryError {}

/// 図面がビュー view を持つか。None は既定のビューで、どの図面にもある
fn has_view(scene: &Scene, view: Option<&str>) -> bool {
    match (view, &scene.views) {
        (None, _) => true,
        (Some(view), Some(views)) => views.iter().any(|candidate| candidate.id == view),
        (Some(_), None) => false,
    }
}

/// マークアップの文書 document を、それを付けた図面 before（バイト列）から新しい版 after へ移す（docs/decisions/0021 の
/// Revisions）。項目のあるビューごとに、前の版のそのビューと、新しい版の同じ id のビュー（options.views で対応を
/// 替えられる）を比べ（`compare::compare_scenes`）、`markup::carry_view` で項目を動かし、entities を付け替え、carried
/// の印を付ける。新しい版にないビューの項目は動かさず removed にする。文書の drawing は新しい版のバイトの SHA-256 と
/// 形式になり、項目の view は新しい版のビューの id になる。項目の順は変えない
pub fn carry_markup(
    document: &markup::MarkupDocument,
    before: &[u8],
    after: &[u8],
    options: &CarryOptions,
) -> Result<CarriedMarkup, CarryError> {
    let from = markup::drawing_hash(before);
    if !options.ignore_hash && from != document.drawing.sha256 {
        return Err(CarryError {
            code: "drawing-mismatch",
            message: format!(
                "マークアップは別の図面（SHA-256 {}）のものです。前の版の図面の SHA-256 は {from} です",
                document.drawing.sha256
            ),
        });
    }
    let read_view = |bytes: &[u8], view: Option<&str>| {
        read(bytes, &ReadDrawingOptions { format: None, view: view.map(str::to_string), limits: options.limits })
            .map_err(|error| CarryError {
                code: if error.exceeded().is_some() { "too-large" } else { "unreadable" },
                message: error.to_string(),
            })
    };
    let before_default = read_view(before, None)?;
    let after_default = read_view(after, None)?;
    let mut views: Vec<Option<&str>> = Vec::new();
    for item in &document.items {
        if !views.contains(&item.view.as_deref()) {
            views.push(item.view.as_deref());
        }
    }
    let mut carried: Vec<Option<markup::MarkupItem>> = vec![None; document.items.len()];
    for view in views {
        let positions: Vec<usize> =
            (0..document.items.len()).filter(|&index| document.items[index].view.as_deref() == view).collect();
        let items: Vec<markup::MarkupItem> = positions.iter().map(|&index| document.items[index].clone()).collect();
        let target = view.map(|view| options.views.get(view).map_or(view, String::as_str));
        let moved = if has_view(&before_default, view) && has_view(&after_default, target) {
            let before_scene = if view.is_none() { before_default.clone() } else { read_view(before, view)? };
            let after_scene = if target.is_none() { after_default.clone() } else { read_view(after, target)? };
            let diff = compare::compare_scenes(&before_scene, &after_scene, &options.compare);
            let mut moved = markup::carry_view(&items, &before_scene, &after_scene, &diff, &from);
            for item in &mut moved {
                item.view = after_scene.view.clone();
            }
            moved
        } else {
            markup::carry_removed(&items, &from)
        };
        for (index, item) in positions.into_iter().zip(moved) {
            carried[index] = Some(item);
        }
    }
    let items: Vec<markup::MarkupItem> = carried.into_iter().flatten().collect();
    let count = |state: markup::CarriedState| {
        items.iter().filter(|item| item.carried.as_ref().is_some_and(|carried| carried.state == state)).count()
    };
    let (unchanged, changed, removed) = (
        count(markup::CarriedState::Unchanged),
        count(markup::CarriedState::Changed),
        count(markup::CarriedState::Removed),
    );
    let mut drawing = document.drawing.clone();
    drawing.sha256 = markup::drawing_hash(after);
    drawing.format = after_default.format.clone();
    Ok(CarriedMarkup {
        document: markup::MarkupDocument { drawing, items, ..document.clone() },
        unchanged,
        changed,
        removed,
    })
}

/// 図面のバイト列を読み、表示用の図面にする。上限（`options.limits`）を超えたら、途中でやめて上限のエラーを返す
pub fn read(bytes: &[u8], options: &ReadDrawingOptions) -> Result<Scene, ReadError> {
    let read_options = ReadOptions { view: options.view.clone() };
    read_drawing_with_limits(bytes, &readers(), options.format.as_deref(), &read_options, &options.limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn fixture(path: &str) -> Vec<u8> {
        std::fs::read(format!("{}/../../tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn limited(limits: ReadLimits) -> ReadDrawingOptions {
        ReadDrawingOptions { limits, ..ReadDrawingOptions::default() }
    }

    #[test]
    fn stops_reading_at_each_limit() {
        for path in ["sfc/features.sfc", "p21/features.p21", "jww/minimal-v700.jww", "dxf/features.dxf"] {
            let bytes = fixture(path);
            let all = read(&bytes, &ReadDrawingOptions::default()).unwrap();
            // 上限が十分なら、上限なしと同じ結果
            let roomy = ReadLimits {
                max_bytes: Some(bytes.len() as u64),
                max_entities: Some(all.entities.len() as u64),
                max_duration: Some(Duration::from_secs(60)),
            };
            assert_eq!(read(&bytes, &limited(roomy)).unwrap(), all, "{path}");
            let few = ReadLimits { max_entities: Some(all.entities.len() as u64 - 1), ..ReadLimits::default() };
            let error = read(&bytes, &limited(few)).unwrap_err();
            assert_eq!(error.exceeded(), Some(Limit::Entities), "{path}");
            let small = ReadLimits { max_bytes: Some(bytes.len() as u64 - 1), ..ReadLimits::default() };
            assert_eq!(read(&bytes, &limited(small)).unwrap_err().exceeded(), Some(Limit::Bytes), "{path}");
            let now = ReadLimits { max_duration: Some(Duration::ZERO), ..ReadLimits::default() };
            assert_eq!(read(&bytes, &limited(now)).unwrap_err().exceeded(), Some(Limit::Time), "{path}");
        }
        let error =
            read(&fixture("sfc/features.sfc"), &limited(ReadLimits { max_entities: Some(5), ..ReadLimits::default() }))
                .unwrap_err();
        assert_eq!(error.message(), "図形が上限の 5 個を超えたため、読むのを途中でやめました");
    }

    // IFC は SXF と同じく ISO-10303-21 で始まるが、FILE_SCHEMA が SXF のスキーマでないので、空の SXF として読まない
    #[test]
    fn does_not_read_ifc_as_sxf() {
        let ifc = b"ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('ViewDefinition [ReferenceView_V1.2]'),'2;1');\n\
            FILE_NAME('model.ifc','2026-10-23T00:00:00',(''),(''),'','','');\nFILE_SCHEMA(('IFC4'));\nENDSEC;\n\
            DATA;\n#1=IFCPROJECT('0YvctVUKr0kugbFTf53O9L',$,'Project',$,$,$,$,$,$);\nENDSEC;\nEND-ISO-10303-21;\n";
        assert_eq!(detect(ifc), None);
        let error = read(ifc, &ReadDrawingOptions::default()).unwrap_err();
        // The supported formats differ between the CAD-only module and the facade with its PDF feature.
        assert!(error.message().starts_with("対応していない形式のファイルです（読める形式:"));
    }
}
