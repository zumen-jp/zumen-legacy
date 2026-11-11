//! underlay（図形の下に描くラスタの中身）を描く（docs/decisions/0022、0020）。PDF の図面のページ（kind が pdf で、
//! file のない underlay）を、描く側が渡す図面のバイト列（`RenderOptions::underlays`）から hayro で描く。画像と、ほかの
//! ファイルの PDF は中身を受け取らないので描かず、notices の underlay-not-drawn で数える。
//!
//! 描き方は pdf.js（`@zumenjs/pdf`）と同じにする: 白いページの上にページの内容を描き、注釈の外観も描く。中身の座標は
//! 表示したページの pt（UserUnit を掛けた大きさ）で、左上が原点、y は下向き。

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::TransformExt;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::page::Page;
use hayro::vello_cpu::color::palette::css::TRANSPARENT;
use hayro::vello_cpu::{Pixmap as Tile, RasterizerSettings, RenderContext, Resources, TargetInit};
use hayro::{RenderCache, RenderSettings, render_into};
use kurbo::Affine;
use std::fmt;
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};
use zumen_core::{Matrix, Notice, NoticeLevel, Underlay, UnderlayKind};

/// 一度に hayro に描かせる画像の一辺の上限（px）。hayro は描くたびにページ全体を解釈するので、なるべく 1 度で描き、
/// これを超える画像（PNG だけ）は分けて描く
const TILE: u32 = 16_384;

/// 描く側が渡す underlay の中身
#[derive(Clone, Default)]
pub struct UnderlayContent {
    /// 図面のファイルそのもののバイト列。file のない underlay（PDF の図面のページ）の中身
    pub drawing: Option<Arc<Vec<u8>>>,
}

impl UnderlayContent {
    /// 図面のファイルのバイト列を中身にする
    pub fn drawing(bytes: impl Into<Vec<u8>>) -> Self {
        Self { drawing: Some(Arc::new(bytes.into())) }
    }
}

impl fmt::Debug for UnderlayContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UnderlayContent").field("drawing", &self.drawing.as_ref().map(|bytes| bytes.len())).finish()
    }
}

impl PartialEq for UnderlayContent {
    fn eq(&self, other: &Self) -> bool {
        match (&self.drawing, &other.drawing) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
            (None, None) => true,
            _ => false,
        }
    }
}

/// 描画 1 回の間の、読んだ PDF と描けなかった underlay の数
pub(crate) struct Underlays<'a> {
    content: &'a UnderlayContent,
    /// 図面の PDF（初めて要るときに読む。読めなければ None）
    document: Option<Option<Pdf>>,
    not_drawn: u64,
}

impl<'a> Underlays<'a> {
    pub(crate) fn new(content: &'a UnderlayContent) -> Self {
        Self { content, document: None, not_drawn: 0 }
    }

    /// 描けなかった underlay の notices
    pub(crate) fn notices(&self) -> Vec<Notice> {
        if self.not_drawn == 0 {
            return Vec::new();
        }
        vec![Notice {
            level: NoticeLevel::Warning,
            code: "underlay-not-drawn".into(),
            message: "underlay（画像・PDF のページ）の中身を受け取らなかったか読めなかったので、描いていません".into(),
            count: self.not_drawn,
        }]
    }

    /// 大きさの分からない underlay（置いていない DXF・DWG の PDF の underlay、docs/decisions/0034）は描かずに数える
    pub(crate) fn skip_unplaced(&mut self, underlay: &Underlay) -> bool {
        let unplaced = !(underlay.width > 0.0 && underlay.height > 0.0);
        if unplaced {
            self.not_drawn += 1;
        }
        unplaced
    }

    /// underlay を、中身の座標 → 画像の px の変換 to_pixels で width × height の画像に描く（premultiplied RGBA）。
    /// 白いページの上に内容を描き、ページの外は透明。描けなければ None を返して数える
    pub(crate) fn rasterize(
        &mut self,
        underlay: &Underlay,
        to_pixels: &Matrix,
        width: u32,
        height: u32,
    ) -> Option<Pixmap> {
        let image = self.try_rasterize(underlay, to_pixels, width, height);
        if image.is_none() {
            self.not_drawn += 1;
        }
        image
    }

    fn try_rasterize(&mut self, underlay: &Underlay, to_pixels: &Matrix, width: u32, height: u32) -> Option<Pixmap> {
        if underlay.kind != UnderlayKind::Pdf || underlay.file.is_some() {
            return None;
        }
        let content = self.content;
        let document = self.document.get_or_insert_with(|| {
            let bytes = content.drawing.clone()?;
            Pdf::new(bytes).ok()
        });
        let page = document.as_ref()?.pages().get(usize::try_from(underlay.page?).ok()?.checked_sub(1)?)?;
        let mut image = Pixmap::new(width, height)?;
        let to_pixels = affine(to_pixels);
        // 白いページ（中身の範囲）
        let mut path = PathBuilder::new();
        let corners = [(0.0, 0.0), (underlay.width, 0.0), (underlay.width, underlay.height), (0.0, underlay.height)];
        for (index, (x, y)) in corners.into_iter().enumerate() {
            let point = to_pixels * kurbo::Point::new(x, y);
            if index == 0 {
                path.move_to(point.x as f32, point.y as f32);
            } else {
                path.line_to(point.x as f32, point.y as f32);
            }
        }
        path.close();
        let mut white = Paint::default();
        white.set_color(Color::WHITE);
        image.fill_path(&path.finish()?, &white, FillRule::Winding, Transform::identity(), None);
        // ページの内容。hayro の変換は既定のユーザー空間 → 画像の px
        let transform = to_pixels * content_from_user(page);
        let cache = RenderCache::new();
        let settings = InterpreterSettings::default();
        for top in (0..height).step_by(TILE as usize) {
            for left in (0..width).step_by(TILE as usize) {
                let (w, h) = ((width - left).min(TILE), (height - top).min(TILE));
                let mut context = RenderContext::new(w as u16, h as u16);
                let shifted = Affine::translate((-f64::from(left), -f64::from(top))) * transform;
                render_into(page, &cache, &settings, &RenderSettings::default(), &mut context, shifted);
                context.flush();
                let mut tile = Tile::new(w as u16, h as u16);
                context.render_with(
                    &mut tile,
                    &mut Resources::default(),
                    RasterizerSettings { target_init: TargetInit::Clear(TRANSPARENT), ..RasterizerSettings::default() },
                );
                let tile = Pixmap::from_vec(tile.data_as_u8_slice().to_vec(), tiny_skia::IntSize::from_wh(w, h)?)?;
                image.draw_pixmap(
                    left as i32,
                    top as i32,
                    tile.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
        Some(image)
    }
}

/// 既定のユーザー空間 → 中身の座標（表示したページの pt。クロップボックスの左上が原点、y は下向き、Rotate を済ませ、
/// UserUnit を掛ける。zumen-pdf の下敷きと同じ）
fn content_from_user(page: &Page<'_>) -> Affine {
    let user_unit = page.raw().get::<f64>(b"UserUnit").filter(|unit| unit.is_finite() && *unit > 0.0).unwrap_or(1.0);
    Affine::scale(user_unit) * page.initial_transform(true).to_kurbo()
}

fn affine(m: &Matrix) -> Affine {
    Affine::new(*m)
}
