//! PDF の読み取り（zumen-pdf）を、答えの分かる PDF で確かめる（決定 0020）。tests/fixtures のどの図面のどの表示も、
//! `render_pdf` で PDF に描いて読み直すと、描いた線・塗り・文字と同じ形になる。
//!
//! 比べるもの（ページの mm。render_pdf は用紙の左下をページの原点にする）:
//! - 線: 描いた図形の線の点列（2 点ずつの線分の並びは 1 本ずつ）が、読んだ線の部分経路にある。render_pdf は同じ色・太さ・
//!   破線の線を 1 つの経路にまとめて書くので、図形ではなく部分経路で比べる。ハッチングの線（切り抜きの中の線）は、
//!   切り抜きの外にあるものを読み取りが読まないので、読んだ線はそれらと点の印（塗りの正方形）を含めて描いたものにある
//! - 塗り: 描いた塗りの輪郭が、読んだ塗りの輪郭にある
//! - 文字: 描いた 1 文字ずつ（空白と、同梱のフォントにない文字を除く）が、読んだ文字のセルに同じ文字・同じ中心である。
//!   render_pdf は字送りの中央と em の中央をセルの中心に置き、字送り（W）と em の上端・下端（Ascent・Descent）を
//!   PDF に書くので、読み取りは同じ中心を求める
//!
//! render_pdf は数を 32 ビットで書くので、2e-3 mm とページの大きさの 2e-7 倍まで同じとみなす。PDF の図面（tests/fixtures/pdf）は図形を描かない
//! （下敷きのページが描く）ので比べない。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use zumen::extract::{EntityFilter, select_entities};
use zumen::{BBox, Primitive, ReadDrawingOptions, Scene};
use zumen_render::{RenderOptions, render_pdf};

const TOLERANCE: f64 = 2e-3;

/// 32 ビットの数の、大きさに対する誤差
const RELATIVE: f64 = 2e-7;

thread_local! {
    /// 比べている図面の許容差
    static SLACK: std::cell::Cell<f64> = const { std::cell::Cell::new(TOLERANCE) };
}

fn slack() -> f64 {
    SLACK.get()
}

/// 点の印（SXF の点マーカ）の一辺（render_pdf の DOT_MM、3 px）
const DOT_MM: f64 = 3.0 * 25.4 / 96.0;

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut files = Vec::new();
    for format in std::fs::read_dir(root).unwrap() {
        let format = format.unwrap().path();
        if !format.is_dir() || format.ends_with("pdf") {
            continue;
        }
        for file in std::fs::read_dir(&format).unwrap() {
            let file = file.unwrap().path();
            let name = file.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".json") && !name.starts_with('.') {
                files.push(file);
            }
        }
    }
    files.sort();
    files
}

#[derive(Default)]
struct Shapes {
    strokes: Vec<Vec<f64>>,
    /// 読んだ図形にあってもよい線（ハッチングの線）
    optional_strokes: Vec<Vec<f64>>,
    rings: Vec<Vec<f64>>,
    /// 読んだ図形にあってもよい塗り（ページの背景と点の印）
    optional_rings: Vec<Vec<f64>>,
    glyphs: Vec<(String, f64, f64)>,
}

/// 点列と、閉じているか。始点に戻って終わる点列は閉じたものとし、重なる最後の点を除く（hayro は矩形の経路を
/// 左下の角から閉じた矩形にして渡すので、閉じた点列は始点と向きによらず比べる）
fn open(mut points: Vec<f64>, closed: bool) -> Vec<f64> {
    let n = points.len();
    if n > 4 && (points[0] - points[n - 2]).abs() <= slack() && (points[1] - points[n - 1]).abs() <= slack() {
        points.truncate(n - 2);
        return closed_marker(points);
    }
    if closed { closed_marker(points) } else { points }
}

/// 閉じた点列の印（点の数を奇数にする NaN）。same_points が見る
fn closed_marker(mut points: Vec<f64>) -> Vec<f64> {
    points.push(f64::NAN);
    points
}

fn local(points: &[f64], region: &BBox) -> Vec<f64> {
    points.as_chunks::<2>().0.iter().flat_map(|[x, y]| [x - region.min_x, y - region.min_y]).collect()
}

fn apply(matrix: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (matrix[0] * x + matrix[2] * y + matrix[4], matrix[1] * x + matrix[3] * y + matrix[5])
}

/// render_pdf が描いた形（ページの mm）
fn drawn(scene: &Scene, region: &BBox, missing: &BTreeSet<char>) -> Shapes {
    // ページの背景（用紙の色の矩形）
    let (width, height) = (region.max_x - region.min_x, region.max_y - region.min_y);
    let page = open(vec![0.0, 0.0, width, 0.0, width, height, 0.0, height], true);
    let mut shapes = Shapes { optional_rings: vec![page], ..Shapes::default() };
    let filter = EntityFilter { region: Some(*region), hidden_layers: None };
    let strokes = |stroke: &zumen::StrokePrimitive, into: &mut Vec<Vec<f64>>| {
        if stroke.color == "transparent" || stroke.points.len() < 4 {
            return;
        }
        if stroke.segments == Some(true) {
            into.extend(stroke.points.as_chunks::<4>().0.iter().map(|pair| local(pair, region)));
        } else {
            into.push(open(local(&stroke.points, region), stroke.closed));
        }
    };
    for entity in select_entities(scene, &filter).into_iter().filter(|entity| entity.drawn) {
        for primitive in &entity.primitives {
            match primitive {
                Primitive::Stroke(stroke) => strokes(stroke, &mut shapes.strokes),
                Primitive::Clipped(clipped) => {
                    for stroke in &clipped.strokes {
                        strokes(stroke, &mut shapes.optional_strokes);
                    }
                }
                Primitive::Fill(fill) => shapes.rings.extend(
                    fill.rings.iter().filter(|ring| ring.len() >= 6).map(|ring| open(local(ring, region), true)),
                ),
                Primitive::Dot(dot) => {
                    let (x, y, half) = (dot.x - region.min_x, dot.y - region.min_y, DOT_MM / 2.0);
                    let square = vec![x - half, y - half, x + half, y - half, x + half, y + half, x - half, y + half];
                    shapes.optional_rings.push(open(square, true));
                }
                Primitive::Text(text) => {
                    if !(text.height.is_finite() && text.height > 0.0) {
                        continue;
                    }
                    for cell in &text.cells {
                        let Some(character) = cell.character.chars().next() else { continue };
                        if character.is_whitespace() || missing.contains(&character) {
                            continue;
                        }
                        let (x, y) = apply(&text.matrix, cell.x, cell.y);
                        let (x, y) = (x - region.min_x, y - region.min_y);
                        // ページの外の文字は、その字形だけの文字として書かれ、読み取りがページの外の内容として読まない
                        if (0.0..=width).contains(&x) && (0.0..=height).contains(&y) {
                            shapes.glyphs.push((cell.character.clone(), x, y));
                        }
                    }
                }
            }
        }
    }
    shapes
}

/// 読んだ PDF の形
fn read_back(scene: &Scene) -> Shapes {
    let mut shapes = Shapes::default();
    for entity in &scene.entities {
        assert!(!entity.drawn, "{}: PDF の図形は描かない", entity.id);
        for primitive in &entity.primitives {
            match primitive {
                Primitive::Stroke(stroke) if stroke.segments == Some(true) => {
                    shapes.strokes.extend(stroke.points.as_chunks::<4>().0.iter().map(|pair| pair.to_vec()));
                }
                Primitive::Stroke(stroke) => shapes.strokes.push(open(stroke.points.clone(), stroke.closed)),
                Primitive::Fill(fill) => shapes.rings.extend(fill.rings.iter().map(|ring| open(ring.clone(), true))),
                Primitive::Text(text) => {
                    for cell in &text.cells {
                        let (x, y) = apply(&text.matrix, cell.x, cell.y);
                        shapes.glyphs.push((cell.character.clone(), x, y));
                    }
                }
                other => panic!("{}: PDF の読み取りが作らない要素 {other:?}", entity.id),
            }
        }
    }
    shapes
}

fn same_points(a: &[f64], b: &[f64]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let near = |a: &[f64], b: &[f64]| (a[0] - b[0]).abs() <= slack() && (a[1] - b[1]).abs() <= slack();
    if a.len().is_multiple_of(2) {
        return a.as_chunks::<2>().0.iter().zip(b.as_chunks::<2>().0).all(|(a, b)| near(a, b));
    }
    // 閉じた点列: どこから、どちら向きに回っても同じ
    let (a, b) = (&a[..a.len() - 1], &b[..b.len() - 1]);
    let n = a.len() / 2;
    let point = |points: &[f64], index: usize| [points[2 * (index % n)], points[2 * (index % n) + 1]];
    (0..n).any(|shift| {
        (0..n).all(|index| near(&point(a, index), &point(b, index + shift)))
            || (0..n).all(|index| near(&point(a, index), &point(b, shift + n - index)))
    })
}

/// expected のどれもが、まだ使っていない actual のどれかと同じ。同じなら使ったことにする
fn missing_from(expected: &[Vec<f64>], actual: &[Vec<f64>], used: &mut [bool]) -> Vec<Vec<f64>> {
    let mut missing = Vec::new();
    for points in expected {
        match actual.iter().enumerate().position(|(index, candidate)| !used[index] && same_points(points, candidate)) {
            Some(index) => used[index] = true,
            None => missing.push(points.clone()),
        }
    }
    missing
}

#[test]
fn reads_back_what_render_pdf_draws() {
    let mut checked = 0;
    for file in fixtures() {
        let bytes = std::fs::read(&file).unwrap();
        let first = zumen::read(&bytes, &ReadDrawingOptions::default()).unwrap();
        let views: Vec<Option<String>> = match &first.views {
            Some(views) => views.iter().map(|view| Some(view.id.clone())).collect(),
            None => vec![None],
        };
        for view in views {
            let label =
                format!("{}{}", file.display(), view.as_deref().map(|view| format!(" @{view}")).unwrap_or_default());
            let scene =
                zumen::read(&bytes, &ReadDrawingOptions { view: view.clone(), ..ReadDrawingOptions::default() })
                    .unwrap();
            let options = RenderOptions { sheet_frame: false, ..RenderOptions::default() };
            let Ok((pdf, report)) = render_pdf(&scene, None, &options) else { continue };
            let region = match &scene.sheet {
                Some(sheet) => BBox { min_x: 0.0, min_y: 0.0, max_x: sheet.width, max_y: sheet.height },
                None => scene.bounds,
            };
            SLACK.set(TOLERANCE + RELATIVE * (region.max_x - region.min_x).max(region.max_y - region.min_y));
            let expected = drawn(&scene, &region, &report.missing_glyphs);
            let read =
                zumen::read(&pdf, &ReadDrawingOptions::default()).unwrap_or_else(|error| panic!("{label}: {error}"));
            assert_eq!(read.format, "pdf");
            let actual = read_back(&read);

            let mut used = vec![false; actual.strokes.len()];
            let missing = missing_from(&expected.strokes, &actual.strokes, &mut used);
            assert!(missing.is_empty(), "{label}: {} lines not read back, the first {:?}", missing.len(), missing[0]);
            missing_from(&expected.optional_strokes, &actual.strokes, &mut used);
            let extra = used.iter().filter(|used| !**used).count();
            assert_eq!(extra, 0, "{label}: {extra} lines read that were not drawn");

            let mut used = vec![false; actual.rings.len()];
            let missing = missing_from(&expected.rings, &actual.rings, &mut used);
            assert!(missing.is_empty(), "{label}: {} fills not read back, the first {:?}", missing.len(), missing[0]);
            missing_from(&expected.optional_rings, &actual.rings, &mut used);
            assert!(used.iter().all(|used| *used), "{label}: fills read that were not drawn");

            let mut glyphs = actual.glyphs.clone();
            for (character, x, y) in &expected.glyphs {
                let found = glyphs.iter().position(|(other, ox, oy)| {
                    other == character && (ox - x).abs() <= slack() && (oy - y).abs() <= slack()
                });
                match found {
                    Some(index) => {
                        glyphs.swap_remove(index);
                    }
                    None => {
                        let near: Vec<_> = glyphs.iter().filter(|(other, _, _)| other == character).collect();
                        panic!("{label}: {character} at {x}, {y} is not read back; read {near:?}")
                    }
                }
            }
            let page = |x: &f64, y: &f64| {
                (0.0..=region.max_x - region.min_x).contains(x) && (0.0..=region.max_y - region.min_y).contains(y)
            };
            glyphs.retain(|(_, x, y)| page(x, y));
            assert!(glyphs.is_empty(), "{label}: characters read that were not drawn: {glyphs:?}");
            checked += 1;
        }
    }
    println!("read back {checked} drawings and views");
    assert!(checked >= 20, "checked {checked} drawings");
}

fn fixture_pdf() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf/drawing.pdf")).unwrap()
}

/// PDF は zumen::read と zumen::detect で読み、書き直し・編集・描き込みは断る（決定 0020）
#[test]
fn reads_pdf_and_refuses_to_write_it() {
    let bytes = fixture_pdf();
    assert_eq!(zumen::detect(&bytes), Some("pdf"));
    let scene =
        zumen::read(&bytes, &ReadDrawingOptions { view: Some("2".into()), ..ReadDrawingOptions::default() }).unwrap();
    assert_eq!((scene.format.as_str(), scene.view.as_deref()), ("pdf", Some("2")));
    let options = ReadDrawingOptions::default();
    let converted = zumen::convert(&bytes, "dxf", &options).unwrap_err();
    assert!(converted.message().contains("PDF の図面は読むだけ"), "{converted}");
    let edited = zumen::edit(&bytes, &[], &options).unwrap_err();
    assert_eq!(edited.code, "unsupported-format");
    assert!(edited.message.contains("PDF の図面は読むだけ"), "{}", edited.message);
    let into = zumen::draft_into(&bytes, r#"{"sheets": []}"#, &zumen::DraftIntoOptions::default()).unwrap_err();
    assert_eq!(into.code, "unsupported-format");
}
