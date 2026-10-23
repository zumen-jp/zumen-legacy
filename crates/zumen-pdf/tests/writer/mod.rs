//! テストの PDF を組み立てる（pdf-writer）。どのページも同じ資源（フォント・XObject・シェーディング・パターン）を持ち、
//! 内容はページの内容のストリームの演算子をそのまま書く。

use pdf_writer::types::{CidFontType, FontFlags, FunctionShadingType, PaintType, SystemInfo, TilingType, UnicodeCmap};
use pdf_writer::writers::{Catalog, Page};
use pdf_writer::{Finish, Name, Pdf, Rect, Ref, Str};

/// A4 縦（pt）
pub const A4: [f32; 4] = [0.0, 0.0, 595.276, 841.89];

pub struct Writer {
    pub pdf: Pdf,
    next: i32,
    catalog: Ref,
    tree: Ref,
    resources: Ref,
    pages: Vec<Ref>,
    /// 資源の Properties の oc1（optional content group。8.11.2）
    pub group: Ref,
}

impl Writer {
    pub fn new() -> Self {
        let mut writer = Self {
            pdf: Pdf::new(),
            next: 1,
            catalog: Ref::new(1),
            tree: Ref::new(2),
            resources: Ref::new(3),
            pages: Vec::new(),
            group: Ref::new(4),
        };
        writer.next = 5;
        writer.write_resources();
        writer
    }

    pub fn id(&mut self) -> Ref {
        let id = Ref::new(self.next);
        self.next += 1;
        id
    }

    /// 内容 content のページを足す。page でページの辞書に書き足す（CropBox、Rotate、VP など）
    pub fn page(&mut self, media: [f32; 4], content: &str, page: impl FnOnce(&mut Page<'_>)) -> Ref {
        let (id, contents) = (self.id(), self.id());
        self.pdf.stream(contents, content.as_bytes());
        let mut writer = self.pdf.page(id);
        writer.parent(self.tree).media_box(Rect::new(media[0], media[1], media[2], media[3])).contents(contents);
        writer.pair(Name(b"Resources"), self.resources);
        page(&mut writer);
        writer.finish();
        self.pages.push(id);
        id
    }

    /// PDF を書き終える。catalog でカタログに書き足す（PageLabels、OCProperties など）
    pub fn finish(mut self, catalog: impl FnOnce(&mut Catalog<'_>)) -> Vec<u8> {
        let count = self.pages.len() as i32;
        self.pdf.pages(self.tree).kids(self.pages.iter().copied()).count(count);
        let mut writer = self.pdf.catalog(self.catalog);
        writer.pages(self.tree);
        catalog(&mut writer);
        writer.finish();
        self.pdf.finish()
    }

    /// 資源。F1 は埋め込まない Helvetica、F2 は埋め込まない Type 0 の日本語のフォント（Identity-H と ToUnicode）、
    /// F3 は ToUnicode のない Type 3 のフォント、Fm1 はフォーム XObject、Im1 は 2 × 1 画素の画像、Sh1 は軸の
    /// シェーディング、P1 はタイリングパターン、oc1 は optional content group
    fn write_resources(&mut self) {
        let mut group = self.pdf.indirect(self.group).dict();
        group.pair(Name(b"Type"), Name(b"OCG")).pair(Name(b"Name"), pdf_writer::TextStr("寸法"));
        group.finish();
        let [helvetica, type0, cid, descriptor, to_unicode, type3, glyph, form, image, shading, function, pattern] =
            std::array::from_fn(|_| self.id());
        self.pdf.type1_font(helvetica).base_font(Name(b"Helvetica")).encoding_predefined(Name(b"WinAnsiEncoding"));

        self.pdf
            .type0_font(type0)
            .base_font(Name(b"MS-Gothic"))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(cid)
            .to_unicode(to_unicode);
        let mut font = self.pdf.cid_font(cid);
        font.subtype(CidFontType::Type2)
            .base_font(Name(b"MS-Gothic"))
            .system_info(SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 })
            .font_descriptor(descriptor)
            .default_width(1000.0);
        font.finish();
        self.pdf
            .font_descriptor(descriptor)
            .name(Name(b"MS-Gothic"))
            .flags(FontFlags::SYMBOLIC)
            .bbox(Rect::new(0.0, -120.0, 1000.0, 880.0))
            .italic_angle(0.0)
            .ascent(880.0)
            .descent(-120.0)
            .cap_height(700.0)
            .stem_v(80.0);
        let mut cmap = UnicodeCmap::new(
            Name(b"Custom"),
            SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 },
        );
        cmap.pair(1u16, '図');
        cmap.pair(2u16, '面');
        cmap.pair_with_multiple(3u16, "ab".chars());
        self.pdf.cmap(to_unicode, &cmap.finish());

        let mut font = self.pdf.type3_font(type3);
        font.bbox(Rect::new(0.0, 0.0, 1000.0, 1000.0))
            .matrix([0.001, 0.0, 0.0, 0.001, 0.0, 0.0])
            .first_char(97)
            .widths([600.0]);
        font.char_procs().pair(Name(b"a"), glyph);
        font.encoding_custom().differences().consecutive(97, [Name(b"a")]);
        font.finish();
        self.pdf.stream(glyph, b"600 0 0 0 500 700 d1 0 0 m 500 700 l S");

        self.pdf
            .form_xobject(form, b"0 0 m 10 0 l S")
            .bbox(Rect::new(-50.0, -50.0, 50.0, 50.0))
            .matrix([2.0, 0.0, 0.0, 2.0, 100.0, 100.0]);

        let mut writer = self.pdf.image_xobject(image, &[255, 0, 0, 0, 0, 255]);
        writer.width(2).height(1).color_space().device_rgb();
        writer.bits_per_component(8);
        writer.finish();

        self.pdf.exponential_function(function).domain([0.0, 1.0]).c0([0.0]).c1([1.0]).n(1.0);
        let mut writer = self.pdf.function_shading(shading);
        writer.shading_type(FunctionShadingType::Axial).coords([0.0, 0.0, 100.0, 0.0]).function(function);
        writer.color_space().device_gray();
        writer.finish();

        let mut writer = self.pdf.tiling_pattern(pattern, b"0 0 m 5 5 l S");
        writer
            .tiling_type(TilingType::ConstantSpacing)
            .paint_type(PaintType::Colored)
            .bbox(Rect::new(0.0, 0.0, 5.0, 5.0))
            .x_step(5.0)
            .y_step(5.0);
        writer.finish();

        let mut writer = self.pdf.indirect(self.resources).dict();
        let mut fonts = writer.insert(Name(b"Font")).dict();
        fonts.pair(Name(b"F1"), helvetica).pair(Name(b"F2"), type0).pair(Name(b"F3"), type3);
        fonts.finish();
        writer.insert(Name(b"XObject")).dict().pair(Name(b"Fm1"), form).pair(Name(b"Im1"), image);
        writer.insert(Name(b"Shading")).dict().pair(Name(b"Sh1"), shading);
        writer.insert(Name(b"Pattern")).dict().pair(Name(b"P1"), pattern);
        writer.insert(Name(b"Properties")).dict().pair(Name(b"oc1"), self.group);
        writer.finish();
    }
}
