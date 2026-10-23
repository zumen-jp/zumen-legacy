//! 図面を 1 ページの PDF にする。ページは用紙の大きさで、内容は用紙の mm のまま描く（ページの変換で pt にする）。
//! 線の太さは図面のとおりで、太さ 0 の線は PDF の最も細い線（太さ 0）にする。文字は同梱の BIZ UDゴシックから
//! 使った字形だけを埋め込み（CID フォント、Identity-H）、ToUnicode を付けて検索・コピーできるようにする。
//! 字形の置き方は PNG と同じ（字送りの中央と em の中央をセルの中心に置く）。

use crate::underlay::Underlays;
use crate::vector::{DOT_MM, Page, PageImage, dash_lengths, page_image};
use crate::{BUNDLED_FONT, RenderError, RenderOptions, RenderReport, background_rgb, resolve_color, visible_underlays};
use pdf_writer::types::{CidFontType, FontFlags, LineCapStyle, LineJoinStyle, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use std::collections::BTreeMap;
use subsetter::GlyphRemapper;
use ttf_parser::{Face, GlyphId};
use zumen_core::{BBox, Background, Primitive, Scene, StrokePrimitive, TextPrimitive, Underlay};

/// 1 mm が何 pt か
const PT_PER_MM: f64 = 72.0 / 25.4;

/// 図面の範囲 region（用紙の mm、None なら用紙、用紙がなければ図面全体）を 1 ページの PDF にする
pub fn render_pdf(
    scene: &Scene,
    region: Option<BBox>,
    options: &RenderOptions,
) -> Result<(Vec<u8>, RenderReport), RenderError> {
    let page = Page::new(scene, region, options)?;
    let theme = options.theme.unwrap_or(scene.background);
    let face = Face::parse(BUNDLED_FONT, 0).map_err(|_| RenderError::new("同梱のフォントを読めません"))?;
    let mut painter = Painter {
        content: Content::new(),
        page: &page,
        theme,
        options,
        face,
        glyphs: GlyphRemapper::new(),
        used: BTreeMap::new(),
        alphas: BTreeMap::new(),
        report: RenderReport::default(),
        batch: None,
        images: Vec::new(),
    };
    let k = PT_PER_MM as f32;
    painter.content.transform([k, 0.0, 0.0, k, 0.0, 0.0]);
    if theme == Background::Dark {
        let (r, g, b) = background_rgb(theme);
        painter.content.set_fill_rgb(f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0);
        painter.content.rect(0.0, 0.0, page.width() as f32, page.height() as f32).fill_nonzero();
    }
    painter.content.set_line_cap(LineCapStyle::RoundCap).set_line_join(LineJoinStyle::RoundJoin);
    // 背景の上、用紙の枠と図形の下に underlay の画像
    let mut underlays = Underlays::new(&options.underlays);
    for underlay in visible_underlays(scene, page.region, options) {
        if let Some(placed) = page_image(underlay, &page, options.underlay_dpi, &mut underlays) {
            painter.image(underlay, placed);
        }
    }
    painter.report.notices = underlays.notices();
    if page.frame
        && let Some(corners) = page.sheet_corners(scene)
    {
        let color = match theme {
            Background::Light => [0, 0, 0, 89],
            Background::Dark => [255, 255, 255, 89],
        };
        painter.begin_alpha(color[3]);
        painter.set_stroke(color);
        painter.content.set_line_width(0.0);
        for (index, (x, y)) in corners.iter().enumerate() {
            if index == 0 {
                painter.content.move_to(*x as f32, *y as f32);
            } else {
                painter.content.line_to(*x as f32, *y as f32);
            }
        }
        painter.content.close_path().stroke();
        painter.end_alpha(color[3]);
    }
    for entity in page.entities(scene, options) {
        for primitive in &entity.primitives {
            painter.primitive(primitive);
        }
    }
    painter.flush();
    painter.finish_pdf(scene)
}

/// 色・太さ・破線が同じ線をまとめて 1 つの path で描く
struct Batch {
    color: [u8; 4],
    width: f64,
    dashes: Vec<f64>,
}

struct Painter<'a> {
    content: Content,
    page: &'a Page,
    theme: Background,
    options: &'a RenderOptions,
    face: Face<'static>,
    glyphs: GlyphRemapper,
    /// 埋め込む字形（新しい番号 → 元の番号と文字）
    used: BTreeMap<u16, (u16, char)>,
    /// 使った不透明度（ExtGState の名前を作る）
    alphas: BTreeMap<u8, ()>,
    report: RenderReport,
    batch: Option<Batch>,
    /// 埋め込む underlay の画像（Im0、Im1 …）
    images: Vec<Image>,
}

/// 埋め込む画像。RGB と、不透明でない画素があればその不透明度（SMask）
struct Image {
    width: u32,
    height: u32,
    rgb: Vec<u8>,
    alpha: Option<Vec<u8>>,
}

impl Image {
    fn of(pixmap: &tiny_skia::Pixmap) -> Self {
        let mut rgb = Vec::with_capacity(pixmap.pixels().len() * 3);
        let mut alpha = Vec::with_capacity(pixmap.pixels().len());
        for pixel in pixmap.pixels() {
            let color = pixel.demultiply();
            rgb.extend([color.red(), color.green(), color.blue()]);
            alpha.push(color.alpha());
        }
        let opaque = alpha.iter().all(|value| *value == 255);
        Self { width: pixmap.width(), height: pixmap.height(), rgb, alpha: (!opaque).then_some(alpha) }
    }
}

fn unit(value: u8) -> f32 {
    f32::from(value) / 255.0
}

fn alpha_name(alpha: u8) -> String {
    format!("A{alpha}")
}

impl Painter<'_> {
    fn point(&self, x: f64, y: f64) -> (f32, f32) {
        let (x, y) = self.page.local(x, y);
        (x as f32, y as f32)
    }

    fn set_stroke(&mut self, color: [u8; 4]) {
        self.content.set_stroke_rgb(unit(color[0]), unit(color[1]), unit(color[2]));
    }

    fn set_fill(&mut self, color: [u8; 4]) {
        self.content.set_fill_rgb(unit(color[0]), unit(color[1]), unit(color[2]));
    }

    /// 不透明でない色は、状態を保存して ExtGState で不透明度を設定する
    fn begin_alpha(&mut self, alpha: u8) {
        if alpha < 255 {
            self.alphas.insert(alpha, ());
            self.content.save_state();
            self.content.set_parameters(Name(alpha_name(alpha).as_bytes()));
        }
    }

    fn end_alpha(&mut self, alpha: u8) {
        if alpha < 255 {
            self.content.restore_state();
        }
    }

    fn flush(&mut self) {
        let Some(batch) = self.batch.take() else {
            return;
        };
        self.content.stroke();
        self.end_alpha(batch.color[3]);
    }

    fn stroke(&mut self, stroke: &StrokePrimitive) {
        if stroke.color == "transparent" || stroke.points.len() < 4 {
            return;
        }
        let Some(color) = resolve_color(&stroke.color, self.theme) else {
            return;
        };
        let width = if self.options.line_weights && stroke.width.is_finite() { stroke.width.max(0.0) } else { 0.0 };
        let dashes = dash_lengths(&stroke.dashes);
        if self.batch.as_ref().is_none_or(|b| b.color != color || b.width != width || b.dashes != dashes) {
            self.flush();
            self.begin_alpha(color[3]);
            self.set_stroke(color);
            self.content.set_line_width(width as f32);
            self.content.set_dash_pattern(dashes.iter().map(|value| *value as f32), 0.0);
            self.batch = Some(Batch { color, width, dashes });
        }
        let points: Vec<(f32, f32)> =
            stroke.points.as_chunks::<2>().0.iter().map(|[x, y]| self.point(*x, *y)).collect();
        if stroke.segments == Some(true) {
            for [start, end] in points.as_chunks::<2>().0 {
                self.content.move_to(start.0, start.1).line_to(end.0, end.1);
            }
            return;
        }
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        self.content.move_to(first.0, first.1);
        for point in rest {
            self.content.line_to(point.0, point.1);
        }
        if stroke.closed {
            self.content.close_path();
        }
    }

    /// underlay の画像を、ページの mm の rect に置く。clip で切り取り、opacity で重ねる
    fn image(&mut self, underlay: &Underlay, placed: PageImage) {
        let name = format!("Im{}", self.images.len());
        self.images.push(Image::of(&placed.image));
        self.content.save_state();
        if let Some(rings) = &underlay.clip
            && self.rings(rings)
        {
            self.content.clip_even_odd().end_path();
        }
        let alpha = (underlay.opacity.unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8;
        self.begin_alpha(alpha);
        let rect = placed.rect;
        let (w, h) = ((rect.max_x - rect.min_x) as f32, (rect.max_y - rect.min_y) as f32);
        self.content.transform([w, 0.0, 0.0, h, rect.min_x as f32, rect.min_y as f32]);
        self.content.x_object(Name(name.as_bytes()));
        self.end_alpha(alpha);
        self.content.restore_state();
    }

    /// 外形と中抜きの path を作る。描く点がなければ false
    fn rings(&mut self, rings: &[Vec<f64>]) -> bool {
        let mut any = false;
        for ring in rings.iter().filter(|ring| ring.len() >= 6) {
            let points: Vec<(f32, f32)> = ring.as_chunks::<2>().0.iter().map(|[x, y]| self.point(*x, *y)).collect();
            self.content.move_to(points[0].0, points[0].1);
            for point in &points[1..] {
                self.content.line_to(point.0, point.1);
            }
            self.content.close_path();
            any = true;
        }
        any
    }

    fn primitive(&mut self, primitive: &Primitive) {
        match primitive {
            Primitive::Stroke(stroke) => self.stroke(stroke),
            Primitive::Fill(fill) => {
                self.flush();
                let Some(color) = resolve_color(&fill.color, self.theme) else {
                    return;
                };
                self.begin_alpha(color[3]);
                self.set_fill(color);
                if self.rings(&fill.rings) {
                    self.content.fill_even_odd();
                }
                self.end_alpha(color[3]);
            }
            Primitive::Clipped(clipped) => {
                self.flush();
                self.content.save_state();
                if self.rings(&clipped.rings) {
                    self.content.clip_even_odd().end_path();
                    for stroke in &clipped.strokes {
                        self.stroke(stroke);
                    }
                    self.flush();
                }
                self.content.restore_state();
            }
            Primitive::Dot(dot) => {
                self.flush();
                let Some(color) = resolve_color(&dot.color, self.theme) else {
                    return;
                };
                let (x, y) = self.page.local(dot.x, dot.y);
                self.begin_alpha(color[3]);
                self.set_fill(color);
                let half = DOT_MM / 2.0;
                self.content.rect((x - half) as f32, (y - half) as f32, DOT_MM as f32, DOT_MM as f32).fill_nonzero();
                self.end_alpha(color[3]);
            }
            Primitive::Text(text) => {
                self.flush();
                self.text(text);
            }
        }
    }

    fn text(&mut self, text: &TextPrimitive) {
        let Some(color) = resolve_color(&text.color, self.theme) else {
            return;
        };
        if !(text.height.is_finite() && text.height > 0.0 && text.scale_x.is_finite()) || text.cells.is_empty() {
            return;
        }
        let units_per_em = f64::from(self.face.units_per_em());
        // textBaseline = "middle" と同じく、em の枠の中央を文字の中心に合わせる
        let middle = (f64::from(self.face.ascender()) + f64::from(self.face.descender())) / 2.0;
        let mut shown: Vec<(u16, f64, f64)> = Vec::new();
        for cell in &text.cells {
            let Some(character) = cell.character.chars().next() else {
                continue;
            };
            if character.is_whitespace() {
                continue;
            }
            let Some(id) = self.face.glyph_index(character) else {
                self.report.missing_glyphs.insert(character);
                continue;
            };
            let advance = f64::from(self.face.glyph_hor_advance(id).unwrap_or(0)) / units_per_em;
            let cid = self.glyphs.remap(id.0);
            self.used.entry(cid).or_insert((id.0, character));
            let x = cell.x - advance * text.height * text.scale_x / 2.0;
            let y = cell.y - middle / units_per_em * text.height;
            shown.push((cid, x, y));
        }
        if shown.is_empty() {
            return;
        }
        let [a, b, c, d, e, f] = text.matrix;
        let (e, f) = self.page.local(e, f);
        self.begin_alpha(color[3]);
        self.content.save_state();
        self.content.transform([a as f32, b as f32, c as f32, d as f32, e as f32, f as f32]);
        self.set_fill(color);
        self.content.begin_text();
        self.content.set_font(Name(b"F0"), 1.0);
        let (sx, sy) = ((text.height * text.scale_x) as f32, text.height as f32);
        for (cid, x, y) in shown {
            self.content.set_text_matrix([sx, 0.0, 0.0, sy, x as f32, y as f32]);
            self.content.show(Str(&cid.to_be_bytes()));
        }
        self.content.end_text();
        self.content.restore_state();
        self.end_alpha(color[3]);
    }

    /// PDF を組み立てる（カタログ、ページ、内容、不透明度、埋め込むフォント、文書の情報）
    fn finish_pdf(self, scene: &Scene) -> Result<(Vec<u8>, RenderReport), RenderError> {
        let mut pdf = Pdf::new();
        let mut next = 1;
        let mut new_ref = || {
            let id = Ref::new(next);
            next += 1;
            id
        };
        let (catalog, pages, page_id, content_id, info) = (new_ref(), new_ref(), new_ref(), new_ref(), new_ref());
        let font = (!self.used.is_empty()).then(|| (new_ref(), new_ref(), new_ref(), new_ref(), new_ref()));
        let alpha_refs: Vec<(u8, Ref)> = self.alphas.keys().map(|&alpha| (alpha, new_ref())).collect();
        let image_refs: Vec<(Ref, Option<Ref>)> =
            self.images.iter().map(|image| (new_ref(), image.alpha.as_ref().map(|_| new_ref()))).collect();

        pdf.catalog(catalog).pages(pages);
        pdf.pages(pages).kids([page_id]).count(1);
        let (w, h) = ((self.page.width() * PT_PER_MM) as f32, (self.page.height() * PT_PER_MM) as f32);
        let mut page = pdf.page(page_id);
        page.media_box(Rect::new(0.0, 0.0, w, h)).parent(pages).contents(content_id);
        let mut resources = page.resources();
        if let Some((type0, ..)) = font {
            resources.fonts().pair(Name(b"F0"), type0);
        }
        if !alpha_refs.is_empty() {
            let mut states = resources.ext_g_states();
            for (alpha, id) in &alpha_refs {
                states.pair(Name(alpha_name(*alpha).as_bytes()), *id);
            }
        }
        if !image_refs.is_empty() {
            let mut objects = resources.x_objects();
            for (index, (id, _)) in image_refs.iter().enumerate() {
                objects.pair(Name(format!("Im{index}").as_bytes()), *id);
            }
        }
        resources.finish();
        page.finish();

        for (image, (id, mask)) in self.images.iter().zip(&image_refs) {
            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&image.rgb, 6);
            let mut xobject = pdf.image_xobject(*id, &compressed);
            xobject.width(image.width as i32).height(image.height as i32).interpolate(true);
            xobject.filter(Filter::FlateDecode);
            xobject.color_space().device_rgb();
            xobject.bits_per_component(8);
            if let Some(mask) = mask {
                xobject.s_mask(*mask);
            }
            xobject.finish();
            if let (Some(alpha), Some(mask)) = (&image.alpha, mask) {
                let compressed = miniz_oxide::deflate::compress_to_vec_zlib(alpha, 6);
                let mut xobject = pdf.image_xobject(*mask, &compressed);
                xobject.width(image.width as i32).height(image.height as i32);
                xobject.filter(Filter::FlateDecode);
                xobject.color_space().device_gray();
                xobject.bits_per_component(8);
                xobject.finish();
            }
        }

        let content = self.content.finish();
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&content, 6);
        pdf.stream(content_id, &compressed).filter(Filter::FlateDecode);

        for (alpha, id) in &alpha_refs {
            let value = unit(*alpha);
            pdf.ext_graphics(*id).stroking_alpha(value).non_stroking_alpha(value);
        }

        if let Some((type0, cid_font, descriptor, file, cmap)) = font {
            let subset = subsetter::subset(BUNDLED_FONT, 0, &self.glyphs)
                .map_err(|error| RenderError::new(format!("フォントの部分集合を作れません: {error:?}")))?;
            // 部分集合の名前は 6 文字の大文字（PDF 1.7 の 9.6.4）。使った字形から決める
            let tag: String = {
                let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
                for (cid, (gid, _)) in &self.used {
                    for byte in cid.to_be_bytes().into_iter().chain(gid.to_be_bytes()) {
                        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
                    }
                }
                (0..6).map(|i| char::from(b'A' + ((hash >> (i * 5)) % 26) as u8)).collect()
            };
            let base = format!("{tag}+BIZUDGothic-Regular");
            let em = f32::from(self.face.units_per_em());
            let to_pdf = |value: i16| f32::from(value) * 1000.0 / em;
            let system = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };

            pdf.type0_font(type0)
                .base_font(Name(base.as_bytes()))
                .encoding_predefined(Name(b"Identity-H"))
                .descendant_font(cid_font)
                .to_unicode(cmap);

            let mut cid = pdf.cid_font(cid_font);
            cid.subtype(CidFontType::Type2)
                .base_font(Name(base.as_bytes()))
                .system_info(system)
                .font_descriptor(descriptor)
                .cid_to_gid_map_predefined(Name(b"Identity"));
            let mut widths = cid.widths();
            for (new, (old, _)) in &self.used {
                let advance = self.face.glyph_hor_advance(GlyphId(*old)).unwrap_or(0);
                widths.consecutive(*new, [f32::from(advance) * 1000.0 / em]);
            }
            widths.finish();
            cid.finish();

            let bbox = self.face.global_bounding_box();
            pdf.font_descriptor(descriptor)
                .name(Name(base.as_bytes()))
                .flags(FontFlags::SYMBOLIC)
                .bbox(Rect::new(to_pdf(bbox.x_min), to_pdf(bbox.y_min), to_pdf(bbox.x_max), to_pdf(bbox.y_max)))
                .italic_angle(0.0)
                .ascent(to_pdf(self.face.ascender()))
                .descent(to_pdf(self.face.descender()))
                .cap_height(to_pdf(self.face.capital_height().unwrap_or(self.face.ascender())))
                .stem_v(80.0)
                .font_file2(file);

            let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&subset, 6);
            pdf.stream(file, &compressed).filter(Filter::FlateDecode).pair(Name(b"Length1"), subset.len() as i32);

            let mut unicode = UnicodeCmap::new(Name(b"Custom"), system);
            for (new, (_, character)) in &self.used {
                unicode.pair(*new, *character);
            }
            let cmap_data = unicode.finish();
            pdf.cmap(cmap, &cmap_data).name(Name(b"Custom")).system_info(system);
        }

        let producer = format!("zumen {}", env!("CARGO_PKG_VERSION"));
        let mut document = pdf.document_info(info);
        document.producer(pdf_writer::TextStr(&producer));
        if let Some(sheet) = scene.sheet.as_ref().filter(|sheet| !sheet.name.trim().is_empty()) {
            document.title(pdf_writer::TextStr(sheet.name.trim()));
        }
        document.finish();
        Ok((pdf.finish(), self.report))
    }
}
