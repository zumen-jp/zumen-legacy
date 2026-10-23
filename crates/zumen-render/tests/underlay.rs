//! PDF の図面のページ（underlay）を hayro で描く（#91、決定 0020・0022）。tests/fixtures/pdf/drawing.pdf の 1 ページ目
//! （A3 横）には、(700, 100) pt から 60 × 30 pt の画像があり、左半分が赤、右半分が青。

use zumen_core::{BBox, Background, Scene, UnderlayKind};
use zumen_render::{RenderOptions, UnderlayContent, View, render, render_pdf, render_svg};

fn fixture() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/pdf/drawing.pdf")).unwrap()
}

fn scene(bytes: &[u8]) -> Scene {
    zumen_pdf::read_pdf(bytes, None).unwrap()
}

/// 1 pt が 1 px になる 72 dpi で用紙を描き、(x, y) px の色を返す
fn draw(scene: &Scene, options: &RenderOptions) -> (tiny_skia::Pixmap, Vec<String>) {
    let sheet = scene.sheet.as_ref().unwrap();
    let region = BBox { min_x: 0.0, min_y: 0.0, max_x: sheet.width, max_y: sheet.height };
    let (view, width, height) = View::for_region(&region, 72.0).unwrap();
    let (pixmap, report) = render(scene, &view, width, height, options).unwrap();
    (pixmap, report.notices.iter().map(|notice| format!("{} {}", notice.code, notice.count)).collect())
}

fn color(pixmap: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 3] {
    let pixel = pixmap.pixel(x, y).unwrap().demultiply();
    [pixel.red(), pixel.green(), pixel.blue()]
}

fn with_content(bytes: &[u8]) -> RenderOptions {
    RenderOptions { underlays: UnderlayContent::drawing(bytes), ..RenderOptions::default() }
}

/// 図面のバイト列を渡すと、ページを白い紙の上に描く（用紙の枠と図形の下）。渡さなければ描かずに数える
#[test]
fn draws_the_page_of_a_pdf_drawing_in_png() {
    let bytes = fixture();
    let scene = scene(&bytes);
    let dark = RenderOptions { theme: Some(Background::Dark), sheet_frame: false, ..with_content(&bytes) };
    let (pixmap, notices) = draw(&scene, &dark);
    assert!(notices.is_empty(), "{notices:?}");
    // 画像の左半分と右半分。y は上から（842 − 130 = 712 から 30 px）
    assert_eq!(color(&pixmap, 715, 727), [255, 0, 0]);
    assert_eq!(color(&pixmap, 745, 727), [0, 0, 255]);
    // 何もない所は、黒い背景ではなく白いページ
    assert_eq!(color(&pixmap, 900, 300), [255, 255, 255]);

    let without = RenderOptions { theme: Some(Background::Dark), sheet_frame: false, ..RenderOptions::default() };
    let (pixmap, notices) = draw(&scene, &without);
    assert_eq!(notices, ["underlay-not-drawn 1"]);
    assert_ne!(color(&pixmap, 900, 300), [255, 255, 255]);

    // ページのレイヤを隠せば描かない（数えない）
    let hidden = RenderOptions { hidden_layers: Some([0].into()), ..dark.clone() };
    let (pixmap, notices) = draw(&scene, &hidden);
    assert!(notices.is_empty());
    assert_ne!(color(&pixmap, 715, 727), [255, 0, 0]);
}

/// clip と opacity（決定 0022）。clip の外は描かず、opacity で背景と重ねる
#[test]
fn clips_and_fades_the_page() {
    let bytes = fixture();
    let mut scene = scene(&bytes);
    let k = 25.4 / 72.0;
    // 画像の左半分（赤）だけを残す
    let (x0, x1, y0, y1) = (690.0 * k, 730.0 * k, 90.0 * k, 140.0 * k);
    scene.underlays[0].clip = Some(vec![vec![x0, y0, x1, y0, x1, y1, x0, y1]]);
    scene.underlays[0].opacity = Some(0.5);
    let options = RenderOptions { theme: Some(Background::Dark), sheet_frame: false, ..with_content(&bytes) };
    let (pixmap, _) = draw(&scene, &options);
    let red = color(&pixmap, 715, 727);
    assert!(red[0] > 100 && red[0] < 160 && red[2] < 40, "{red:?}");
    // clip の外（画像の右半分とページ）は背景のまま
    assert_eq!(color(&pixmap, 745, 727), color(&pixmap, 1000, 500));
    assert_ne!(color(&pixmap, 1000, 500), [255, 255, 255]);
}

/// SVG と PDF には、用紙に写る部分を underlay_dpi の画像にして埋め込む
#[test]
fn embeds_the_page_as_an_image_in_svg_and_pdf() {
    let bytes = fixture();
    let scene = scene(&bytes);
    let options = RenderOptions { underlay_dpi: 36.0, ..with_content(&bytes) };
    let (svg, report) = render_svg(&scene, None, &options).unwrap();
    assert!(report.notices.is_empty());
    let image = svg.find("<image ").expect("an image");
    // 背景の後、用紙の枠と図形の前
    assert!(svg.find("<rect ").unwrap() < image);
    assert!(
        svg[image..].starts_with("<image x=\"0\" y=\"0\" width=\"420\" height=\"297\""),
        "{}",
        &svg[image..image + 120]
    );
    assert!(svg[image..].contains("href=\"data:image/png;base64,iVBORw0KGgo"));

    let (pdf, report) = render_pdf(&scene, None, &options).unwrap();
    assert!(report.notices.is_empty());
    // 埋め込んだ画像は、PDF の読み取りで 1 つの画像の図形として読める（36 dpi で 420 mm は 596 px）
    let read = zumen_pdf::read_pdf(&pdf, None).unwrap();
    let images: Vec<_> = read.entities.iter().filter(|entity| entity.kind == zumen_core::EntityKind::Image).collect();
    assert_eq!(images.len(), 1);
    let size = images[0].properties.iter().find(|property| property.key == "imageSize").unwrap();
    assert_eq!(size.value, "596 × 421");

    let (_, report) = render_svg(&scene, None, &RenderOptions::default()).unwrap();
    assert_eq!(report.notices[0].code, "underlay-not-drawn");
}

/// 画像の underlay と、ほかのファイルの PDF は中身を受け取らないので描かず、数える
#[test]
fn counts_underlays_it_cannot_draw() {
    let bytes = fixture();
    let mut scene = scene(&bytes);
    let mut image = scene.underlays[0].clone();
    image.kind = UnderlayKind::Image;
    image.id = "image".into();
    let mut other = scene.underlays[0].clone();
    other.file = Some("other.pdf".into());
    other.id = "other".into();
    scene.underlays.extend([image, other]);
    let (_, notices) = draw(&scene, &with_content(&bytes));
    assert_eq!(notices, ["underlay-not-drawn 2"]);
    // 読めない PDF も数える
    let broken =
        RenderOptions { underlays: UnderlayContent::drawing(b"%PDF-1.7 broken".to_vec()), ..RenderOptions::default() };
    let (_, notices) = draw(&scene, &broken);
    assert_eq!(notices, ["underlay-not-drawn 3"]);
}
