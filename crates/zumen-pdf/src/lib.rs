//! PDF の図面を読み、表示用の図面（`Scene`）にする（docs/decisions/0020-pdf-drawings-in-rust.md、docs/formats/pdf.md）。
//!
//! ページの内容は hayro-interpret が解釈し、塗った経路・字形の並び・画像を順に `Device` に渡す。このクレートはそれを
//! 図形にする。ページはビュー（id は "1"、"2" …）で、ページそのものが図形の下に描く underlay になり、図形は描かない
//! （`drawn: false`。選択・吸着・計測・取り出しには使う）。座標は用紙の mm で、クロップボックスの左下が原点、
//! ページの回転（Rotate）と UserUnit を済ませた向きと大きさ。節の番号は ISO 32000-1:2008。
#![forbid(unsafe_code)]

mod device;
mod fonts;
mod measure;
mod page;
mod strings;

use device::Collector;
use hayro_interpret::hayro_syntax::object::{Array, Dict};
use hayro_interpret::hayro_syntax::{LoadPdfError, Pdf as Document, PdfVersion};
use hayro_interpret::{Context, InterpreterCache, InterpreterSettings, InterpreterWarning, interpret_page};
use kurbo::Rect;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use zumen_core::geometry::{empty_box, union_box};
use zumen_core::{
    Background, DrawingReader, LengthUnit, NoticeLevel, NoticeList, Property, ReadError, ReadOptions, Scene,
    SceneLayer, SceneView, Underlay, UnderlayKind,
};

/// read_drawing に渡す PDF の読み取り。options.view はページの番号（"1" から）
pub struct Pdf;

/// PDF の読み取り
pub const PDF: Pdf = Pdf;

/// 中身が PDF かを見る範囲。ヘッダの前に 1,024 バイトまでの余計なバイトがあっても読む（Acrobat と pdf.js と同じ）
const HEADER_WINDOW: usize = 1024;

/// 中身が PDF か。先頭の 1,024 バイトの中に、ファイルのヘッダ %PDF- があれば PDF とみなす（7.5.2）
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(HEADER_WINDOW)].windows(5).any(|window| window == b"%PDF-")
}

impl DrawingReader for Pdf {
    fn formats(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    fn detect(&self, bytes: &[u8]) -> Option<&'static str> {
        is_pdf(bytes).then_some("pdf")
    }

    fn read(&self, bytes: &[u8], format: &str, options: &ReadOptions) -> Result<Scene, ReadError> {
        if format != "pdf" {
            return Err(ReadError::new(format!("PDF の読み取りは {format} を読めません")));
        }
        read_pdf(bytes, options.view.as_deref())
    }
}

/// PDF を読み、view のページ（"1" から。省略すると 1 ページ目）を表示用の図面にする
pub fn read_pdf(bytes: &[u8], view: Option<&str>) -> Result<Scene, ReadError> {
    let document = Document::new(bytes.to_vec()).map_err(|error| match error {
        LoadPdfError::Decryption(_) => ReadError::new("パスワードで保護された PDF のため、読めません"),
        LoadPdfError::Invalid => ReadError::new("PDF の構造を読めません（壊れているか、PDF ではありません）"),
    })?;
    let pages = document.pages();
    if pages.is_empty() {
        return Err(ReadError::new("PDF にページがありません"));
    }
    let xref = document.xref();
    let catalog = xref.get::<Dict<'_>>(xref.root_id());
    let labels = page::labels(catalog.as_ref(), pages.len());
    let views: Vec<SceneView> = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let id = (index + 1).to_string();
            SceneView { name: label.clone().unwrap_or_else(|| id.clone()), id }
        })
        .collect();
    let mut notices = NoticeList::new();
    let index = match view {
        None => 0,
        Some(id) => views.iter().position(|candidate| candidate.id == id).unwrap_or_else(|| {
            notices.add(
                NoticeLevel::Warning,
                "unknown-page",
                format!("ページ {id} が見つからないため、1 ページ目を表示しました"),
            );
            0
        }),
    };
    let page = &pages[index];
    let geometry = page::Geometry::of(page);
    let number = index + 1;

    // hayro-interpret が読めなかったフォント（標準のものでない符号化の CID フォント）の数
    let unsupported_fonts = Arc::new(AtomicU64::new(0));
    let counter = unsupported_fonts.clone();
    let settings = InterpreterSettings {
        // 注釈は図形として読まない（PDF の注釈は ParaPDF の領分。決定 0002）。数だけを notices で伝える
        render_annotations: false,
        warning_sink: Arc::new(move |warning| {
            if matches!(warning, InterpreterWarning::UnsupportedFont) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        }),
        ..InterpreterSettings::default()
    };
    let cache = InterpreterCache::new();
    let paper = Rect::new(0.0, 0.0, geometry.paper_width(), geometry.paper_height());
    let mut context = Context::new(geometry.user_space, paper, &cache, xref, settings);
    let mut collector = Collector::new(fonts::Fonts::new(page.resources()), paper);
    interpret_page(page, &mut context, &mut collector);
    let (mut entities, device_notices) = collector.finish();
    for notice in device_notices {
        notices.add_count(notice.level, &notice.code, notice.message, notice.count);
    }
    let unsupported = unsupported_fonts.load(Ordering::Relaxed);
    if unsupported > 0 {
        notices.add_count(
            NoticeLevel::Warning,
            "unsupported-font",
            "読めない符号化のフォントの文字は読みませんでした",
            unsupported,
        );
    }
    let annotations = page::annotation_count(page);
    if annotations > 0 {
        notices.add_count(
            NoticeLevel::Info,
            "annotation-not-read",
            "注釈は図形として読みません（ページの下敷きには描かれます）",
            annotations,
        );
    }
    let groups = catalog
        .as_ref()
        .and_then(|catalog| catalog.get::<Dict<'_>>(b"OCProperties"))
        .and_then(|properties| properties.get::<Array<'_>>(b"OCGs"))
        .map_or(0, |groups| groups.raw_iter().count() as u64);
    if groups > 0 || xref.has_optional_content_groups() {
        notices.add_count(
            NoticeLevel::Info,
            "optional-content",
            "レイヤ（optional content）は分けずにページを 1 つのレイヤとして読み、既定で隠れる内容は読みませんでした",
            groups.max(1),
        );
    }

    for viewport in measure::viewports(page, &geometry, &mut notices) {
        viewport.place(&mut entities);
    }

    let underlay = Underlay {
        id: format!("page-{number}"),
        kind: UnderlayKind::Pdf,
        file: None,
        page: Some(number as u32),
        // 中身の座標は表示したページの pt（1/72 インチ。UserUnit を掛けた大きさ）で、左上が原点、y は下向き。
        // pdf.js の page.getViewport({ scale: 1 }) と同じ（pdf.js も UserUnit を掛ける）
        width: geometry.paper_width() / page::MM_PER_PT,
        height: geometry.paper_height() / page::MM_PER_PT,
        matrix: [page::MM_PER_PT, 0.0, 0.0, -page::MM_PER_PT, 0.0, geometry.paper_height()],
        clip: None,
        opacity: None,
        layers: vec![0],
        content_layers: None,
        bbox: zumen_core::BBox {
            min_x: 0.0,
            min_y: 0.0,
            max_x: geometry.paper_width(),
            max_y: geometry.paper_height(),
        },
    };
    let mut bounds = empty_box();
    union_box(&mut bounds, &underlay.bbox);
    for entity in &entities {
        union_box(&mut bounds, &entity.bbox);
    }
    let view = views[index].clone();
    let sheet = page::sheet(&view.name, geometry.paper_width(), geometry.paper_height());
    let version = version_name(document.version());
    let mut metadata = vec![
        Property::new("format", "形式", format!("PDF（版 {version}）")),
        Property::new("sheet", "用紙", page::sheet_label(&sheet)),
        Property::new("view", "表示", format!("ページ {}", view.name)).raw(view.id.clone()),
        Property::new("pageCount", "ページ", format!("{} ページ", pages.len())).raw(pages.len() as f64),
    ];
    let info = document.metadata();
    for (key, label, value) in [
        ("title", "表題", &info.title),
        ("author", "作成者", &info.author),
        ("subject", "主題", &info.subject),
        ("software", "作成ソフト", &info.creator),
        ("library", "出力ライブラリ", &info.producer),
    ] {
        if let Some(text) = value.as_deref().map(strings::text_string).filter(|text| !text.trim().is_empty()) {
            metadata.push(Property::new(key, label, text));
        }
    }
    for (key, label, value) in
        [("created", "作成日時", &info.creation_date), ("modified", "更新日時", &info.modification_date)]
    {
        if let Some(date) = value {
            metadata.push(Property::new(key, label, strings::date(date)));
        }
    }

    Ok(Scene {
        format: "pdf".into(),
        version,
        unit: Some(LengthUnit::Millimeter),
        views: Some(views),
        view: Some(view.id),
        sheet: Some(sheet),
        bounds,
        layers: vec![SceneLayer { id: 0, name: "ページ".into(), visible: true, group: None }],
        entities,
        underlays: vec![underlay],
        notices: notices.into_vec(),
        metadata,
        background: Background::Light,
    })
}

fn version_name(version: PdfVersion) -> String {
    match version {
        PdfVersion::Pdf10 => "1.0",
        PdfVersion::Pdf11 => "1.1",
        PdfVersion::Pdf12 => "1.2",
        PdfVersion::Pdf13 => "1.3",
        PdfVersion::Pdf14 => "1.4",
        PdfVersion::Pdf15 => "1.5",
        PdfVersion::Pdf16 => "1.6",
        PdfVersion::Pdf17 => "1.7",
        PdfVersion::Pdf20 => "2.0",
    }
    .into()
}
