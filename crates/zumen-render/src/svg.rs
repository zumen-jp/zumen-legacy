//! 図面を SVG にする。単位は用紙の mm（width・height は mm、viewBox はページの mm）。
//! 線・塗り・切り取りは PNG と同じ規則で描き、文字は検索や選択ができる `<text>` にする（字形は表示する側の
//! フォント。1 文字ずつ、セルの中心に置く）。線は 100% の表示で 1 px（0.2646 mm）より細くしない。

use crate::underlay::Underlays;
use crate::vector::{DOT_MM, PX_MM, Page, dash_lengths, page_image};
use crate::{RenderError, RenderOptions, RenderReport, background_rgb, markup_entities_for, resolve_color, visible_underlays};
use std::fmt::Write;
use zumen_core::{BBox, Primitive, Scene, StrokePrimitive, TextPrimitive};

/// 図面の範囲 region（用紙の mm、None なら用紙、用紙がなければ図面全体）を SVG にする。文字は字形を埋め込まないので、
/// 報告（`RenderReport`）には描かなかった内容の notices だけが入る
pub fn render_svg(
    scene: &Scene,
    region: Option<BBox>,
    options: &RenderOptions,
) -> Result<(String, RenderReport), RenderError> {
    let page = Page::new(scene, region, options)?;
    let theme = options.theme.unwrap_or(scene.background);
    let (w, h) = (page.width(), page.height());
    let mut writer = Svg { out: String::new(), page: &page, theme, options, clips: 0, batch: None };
    let (r, g, b) = background_rgb(theme);
    let _ = write!(
        writer.out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}mm\" height=\"{h}mm\" viewBox=\"0 0 {w} {h}\">\n\
         <rect width=\"{w}\" height=\"{h}\" fill=\"#{r:02x}{g:02x}{b:02x}\"/>\n\
         <g transform=\"matrix(1 0 0 -1 0 {h})\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        w = num(w),
        h = num(h),
    );
    // 背景の上、用紙の枠と図形の下に underlay の画像（PNG）。y を反転した中なので、画像はその中央で反転し直す
    let mut underlays = Underlays::new(&options.underlays);
    for underlay in visible_underlays(scene, page.region, options) {
        let Some(placed) = page_image(underlay, &page, options.underlay_dpi, &mut underlays) else { continue };
        let Ok(png) = placed.image.encode_png() else { continue };
        let clip = underlay.clip.as_deref().map(|rings| writer.rings(rings));
        if let Some(d) = &clip {
            writer.clips += 1;
            let id = writer.clips;
            let _ = writeln!(
                writer.out,
                "<clipPath id=\"c{id}\"><path d=\"{d}\" clip-rule=\"evenodd\"/></clipPath>\n<g clip-path=\"url(#c{id})\">"
            );
        }
        let rect = placed.rect;
        let opacity = underlay
            .opacity
            .filter(|opacity| *opacity < 1.0)
            .map(|opacity| format!(" opacity=\"{}\"", num(opacity.max(0.0))));
        let _ = writeln!(
            writer.out,
            "<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" transform=\"matrix(1 0 0 -1 0 {})\" preserveAspectRatio=\"none\"{} href=\"data:image/png;base64,{}\"/>",
            num(rect.min_x),
            num(rect.min_y),
            num(rect.max_x - rect.min_x),
            num(rect.max_y - rect.min_y),
            num(rect.min_y + rect.max_y),
            opacity.unwrap_or_default(),
            base64(&png)
        );
        if clip.is_some() {
            writer.out.push_str("</g>\n");
        }
    }
    if page.frame
        && let Some(corners) = page.sheet_corners(scene)
    {
        let alpha = "0.35";
        let color = match theme {
            zumen_core::Background::Light => "#000000",
            zumen_core::Background::Dark => "#ffffff",
        };
        let mut d = String::new();
        path_points(&mut d, corners.iter().flat_map(|(x, y)| [*x, *y]), true);
        let _ = writeln!(
            writer.out,
            "<path d=\"{d}\" fill=\"none\" stroke=\"{color}\" stroke-opacity=\"{alpha}\" stroke-width=\"{}\"/>",
            num(PX_MM)
        );
    }
    for entity in page.entities(scene, options) {
        for primitive in &entity.primitives {
            writer.primitive(primitive);
        }
    }
    for entity in markup_entities_for(options, PX_MM) {
        for primitive in &entity.primitives {
            writer.primitive(primitive);
        }
    }
    writer.flush();
    writer.out.push_str("</g>\n</svg>\n");
    let report = RenderReport { notices: underlays.notices(), ..RenderReport::default() };
    Ok((writer.out, report))
}

/// RFC 4648 の base64（data URL の画像）
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        for index in 0..4 {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[(value >> (18 - 6 * index) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 数を短く書く（0.001 mm まで。末尾の 0 と小数点は省く）
fn num(value: f64) -> String {
    let mut text = format!("{value:.3}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".into() } else { text }
}

/// XML の文字にする。XML 1.0 で使えない制御文字は除く
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(' '),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => {}
            c => out.push(c),
        }
    }
    out
}

fn path_points(d: &mut String, points: impl IntoIterator<Item = f64>, close: bool) {
    let mut iter = points.into_iter();
    let mut first = true;
    while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
        let _ = write!(d, "{}{} {}", if first { "M" } else { "L" }, num(x), num(y));
        first = false;
    }
    if close && !first {
        d.push('Z');
    }
}

/// 色と不透明度
fn paint(color: [u8; 4]) -> (String, Option<String>) {
    let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let opacity = (color[3] < 255).then(|| num(f64::from(color[3]) / 255.0));
    (hex, opacity)
}

/// 色・太さ・破線が同じ線をまとめた 1 つの path
struct Batch {
    color: [u8; 4],
    width: f64,
    dashes: Vec<f64>,
    d: String,
}

struct Svg<'a> {
    out: String,
    page: &'a Page,
    theme: zumen_core::Background,
    options: &'a RenderOptions,
    clips: usize,
    batch: Option<Batch>,
}

impl Svg<'_> {
    fn local_points(&self, points: &[f64]) -> Vec<f64> {
        points
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|[x, y]| {
                let (lx, ly) = self.page.local(*x, *y);
                [lx, ly]
            })
            .collect()
    }

    fn flush(&mut self) {
        let Some(batch) = self.batch.take() else {
            return;
        };
        if batch.d.is_empty() {
            return;
        }
        let (color, opacity) = paint(batch.color);
        let _ = write!(
            self.out,
            "<path d=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{}\"",
            batch.d,
            num(batch.width)
        );
        if let Some(opacity) = opacity {
            let _ = write!(self.out, " stroke-opacity=\"{opacity}\"");
        }
        if !batch.dashes.is_empty() {
            let dashes: Vec<String> = batch.dashes.iter().map(|value| num(*value)).collect();
            let _ = write!(self.out, " stroke-dasharray=\"{}\"", dashes.join(" "));
        }
        self.out.push_str("/>\n");
    }

    fn stroke(&mut self, stroke: &StrokePrimitive) {
        if stroke.color == "transparent" {
            return;
        }
        let Some(color) = resolve_color(&stroke.color, self.theme) else {
            return;
        };
        let width = if self.options.line_weights { stroke.width.max(PX_MM) } else { PX_MM };
        let width = if width.is_finite() { width } else { PX_MM };
        let dashes = dash_lengths(&stroke.dashes);
        if self.batch.as_ref().is_none_or(|b| b.color != color || b.width != width || b.dashes != dashes) {
            self.flush();
            self.batch = Some(Batch { color, width, dashes, d: String::new() });
        }
        let points = self.local_points(&stroke.points);
        let d = &mut self.batch.as_mut().expect("batch").d;
        if stroke.segments == Some(true) {
            for segment in points.as_chunks::<4>().0 {
                path_points(d, segment.iter().copied(), false);
            }
        } else {
            path_points(d, points, stroke.closed);
        }
    }

    fn rings(&self, rings: &[Vec<f64>]) -> String {
        let mut d = String::new();
        for ring in rings.iter().filter(|ring| ring.len() >= 6) {
            path_points(&mut d, self.local_points(ring), true);
        }
        d
    }

    fn primitive(&mut self, primitive: &Primitive) {
        match primitive {
            Primitive::Stroke(stroke) => self.stroke(stroke),
            Primitive::Fill(fill) => {
                self.flush();
                let Some(color) = resolve_color(&fill.color, self.theme) else {
                    return;
                };
                let d = self.rings(&fill.rings);
                if d.is_empty() {
                    return;
                }
                let (color, opacity) = paint(color);
                let _ = write!(self.out, "<path d=\"{d}\" fill=\"{color}\" fill-rule=\"evenodd\"");
                if let Some(opacity) = opacity {
                    let _ = write!(self.out, " fill-opacity=\"{opacity}\"");
                }
                self.out.push_str("/>\n");
            }
            Primitive::Clipped(clipped) => {
                self.flush();
                let d = self.rings(&clipped.rings);
                if d.is_empty() {
                    return;
                }
                self.clips += 1;
                let id = self.clips;
                let _ = writeln!(
                    self.out,
                    "<clipPath id=\"c{id}\"><path d=\"{d}\" clip-rule=\"evenodd\"/></clipPath>\n<g clip-path=\"url(#c{id})\">"
                );
                for stroke in &clipped.strokes {
                    self.stroke(stroke);
                }
                self.flush();
                self.out.push_str("</g>\n");
            }
            Primitive::Dot(dot) => {
                self.flush();
                let Some(color) = resolve_color(&dot.color, self.theme) else {
                    return;
                };
                let (x, y) = self.page.local(dot.x, dot.y);
                let (color, opacity) = paint(color);
                let _ = write!(
                    self.out,
                    "<rect x=\"{}\" y=\"{}\" width=\"{size}\" height=\"{size}\" fill=\"{color}\"",
                    num(x - DOT_MM / 2.0),
                    num(y - DOT_MM / 2.0),
                    size = num(DOT_MM),
                );
                if let Some(opacity) = opacity {
                    let _ = write!(self.out, " fill-opacity=\"{opacity}\"");
                }
                self.out.push_str("/>\n");
            }
            Primitive::Text(text) => {
                self.flush();
                self.text(text);
            }
        }
    }

    /// 文字。文字の座標（Y 上向き）を、字形が正立するよう Y を反転してから用紙に置き、1 文字ずつセルの中心に置く
    fn text(&mut self, text: &TextPrimitive) {
        let Some(color) = resolve_color(&text.color, self.theme) else {
            return;
        };
        let sx = text.scale_x;
        if !(sx.is_finite() && sx != 0.0 && text.height.is_finite() && text.height > 0.0) || text.cells.is_empty() {
            return;
        }
        let [a, b, c, d, e, f] = text.matrix;
        let (e, f) = self.page.local(e, f);
        let (color, opacity) = paint(color);
        let _ = write!(
            self.out,
            "<text transform=\"matrix({} {} {} {} {} {})\" font-family=\"{}\" font-size=\"{}\" fill=\"{color}\" \
             text-anchor=\"middle\" dominant-baseline=\"central\"",
            num_fine(a * sx),
            num_fine(b * sx),
            num_fine(-c),
            num_fine(-d),
            num(e),
            num(f),
            escape(&text.font_family),
            num_fine(text.height),
        );
        if let Some(opacity) = opacity {
            let _ = write!(self.out, " fill-opacity=\"{opacity}\"");
        }
        self.out.push('>');
        for cell in &text.cells {
            let character = escape(&cell.character);
            if character.trim().is_empty() {
                continue;
            }
            let _ = write!(
                self.out,
                "<tspan x=\"{}\" y=\"{}\">{character}</tspan>",
                num_fine(cell.x / sx),
                num_fine(-cell.y)
            );
        }
        self.out.push_str("</text>\n");
    }
}

/// 変換の係数と文字の座標は、小さな値が多いので有効数字で書く
fn num_fine(value: f64) -> String {
    if value == 0.0 || !value.is_finite() {
        return "0".into();
    }
    let digits = (6 - value.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let mut text = format!("{value:.digits$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".into() } else { text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_short_numbers_and_escapes_text() {
        assert_eq!(
            (num(1.0), num(0.1234), num(-0.0004), num(12.5)),
            ("1".into(), "0.123".into(), "0".into(), "12.5".into())
        );
        assert_eq!(
            (num_fine(0.012345678), num_fine(-1.0), num_fine(0.0)),
            ("0.01234568".into(), "-1".into(), "0".into())
        );
        assert_eq!(escape("A&B <\"c\">\u{1}"), "A&amp;B &lt;&quot;c&quot;&gt;");
    }
}
