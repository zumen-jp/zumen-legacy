//! zumen-pdf の規則を、その規則だけを持つ最小の PDF（pdf-writer で組み立てる）で確かめる。節の番号は
//! ISO 32000-1:2008。PDF の TypeScript の読み取りはないので、正解はこのテストが決める（決定 0020）。

mod writer;

use pdf_writer::types::NumberingStyle;
use pdf_writer::writers::PageLabel;
use pdf_writer::{Finish, Name, Rect, TextStr};
use writer::{A4, Writer};
use zumen_core::{
    EntityKind, Limit, MeasurePoint, NoticeLevel, Primitive, PropertyRaw, ReadLimits, ReadOptions, Scene, SceneEntity,
    measure_distance,
};

/// 1 pt が何 mm か
const K: f64 = 25.4 / 72.0;

fn read(bytes: &[u8]) -> Scene {
    zumen_pdf::read_pdf(bytes, None).unwrap()
}

fn read_view(bytes: &[u8], view: &str) -> Scene {
    zumen_pdf::read_pdf(bytes, Some(view)).unwrap()
}

/// A4 の 1 ページに content を描いた PDF
fn one_page(content: &str) -> Vec<u8> {
    let mut writer = Writer::new();
    writer.page(A4, content, |_| {});
    writer.finish(|_| {})
}

fn raw<'a>(entity: &'a SceneEntity, key: &str) -> Option<&'a PropertyRaw> {
    entity.properties.iter().find(|property| property.key == key).and_then(|property| property.raw.as_ref())
}

fn value<'a>(entity: &'a SceneEntity, key: &str) -> Option<&'a str> {
    entity.properties.iter().find(|property| property.key == key).map(|property| property.value.as_str())
}

fn number(entity: &SceneEntity, key: &str) -> f64 {
    match raw(entity, key) {
        Some(PropertyRaw::Number(value)) => *value,
        other => panic!("{key}: {other:?}"),
    }
}

fn point(entity: &SceneEntity, key: &str) -> [f64; 2] {
    match raw(entity, key) {
        Some(PropertyRaw::Numbers(values)) if values.len() == 2 => [values[0], values[1]],
        other => panic!("{key}: {other:?}"),
    }
}

/// PDF の数は 32 ビットの浮動小数点数で読まれるので、1e-4 mm まで同じとみなす
#[track_caller]
fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-4, "{actual} ≠ {expected}");
}

#[track_caller]
fn close_point(actual: [f64; 2], expected: [f64; 2]) {
    close(actual[0], expected[0]);
    close(actual[1], expected[1]);
}

fn ids(scene: &Scene) -> Vec<&str> {
    scene.entities.iter().map(|entity| entity.id.as_str()).collect()
}

fn notice(scene: &Scene, code: &str) -> u64 {
    scene.notices.iter().find(|notice| notice.code == code).map_or(0, |notice| notice.count)
}

/// ファイルのヘッダ（7.5.2）。前に 1,024 バイトまでの余計なバイトがあってもよい
#[test]
fn detects_pdf_by_the_header_in_the_first_kilobyte() {
    use zumen_core::DrawingReader;
    let with_prefix = |length: usize| [vec![b' '; length], b"%PDF-1.7\n".to_vec()].concat();
    assert!(zumen_pdf::is_pdf(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3"));
    assert!(zumen_pdf::is_pdf(&with_prefix(1019)));
    assert!(!zumen_pdf::is_pdf(&with_prefix(1020)));
    assert!(!zumen_pdf::is_pdf(b"%PDF"));
    assert!(!zumen_pdf::is_pdf(b""));
    assert_eq!(zumen_pdf::PDF.detect(&one_page("")), Some("pdf"));
    assert_eq!(zumen_pdf::PDF.formats(), ["pdf"]);
}

/// ページはビュー（id は 1 から）、名前はページラベル（12.4.2）。ページが下敷きで、図形は描かない
#[test]
fn reads_each_page_as_a_view_with_its_underlay() {
    let mut writer = Writer::new();
    writer.page(A4, "72 72 m 144 72 l S", |_| {});
    writer.page([0.0, 0.0, 1190.551, 841.89], "10 10 m 20 20 l S 30 30 m 40 30 l S", |_| {});
    writer.page(A4, "", |_| {});
    let label = writer.id();
    let roman = writer.id();
    writer.pdf.indirect(label).start::<PageLabel>().prefix(TextStr("表紙"));
    writer.pdf.indirect(roman).start::<PageLabel>().style(NumberingStyle::UpperRoman).prefix(TextStr("A-")).offset(2);
    let bytes = writer.finish(|catalog| {
        catalog.page_labels().nums().insert(0, label).insert(1, roman);
    });

    let first = read(&bytes);
    assert_eq!((first.format.as_str(), first.version.as_str()), ("pdf", "1.7"));
    assert_eq!(first.unit, Some(zumen_core::LengthUnit::Millimeter));
    let views: Vec<(&str, &str)> =
        first.views.as_ref().unwrap().iter().map(|view| (view.id.as_str(), view.name.as_str())).collect();
    assert_eq!(views, [("1", "表紙"), ("2", "A-II"), ("3", "A-III")]);
    assert_eq!(first.view.as_deref(), Some("1"));
    let sheet = first.sheet.as_ref().unwrap();
    assert_eq!((sheet.name.as_str(), sheet.size.as_str()), ("表紙", "A4 縦"));
    close(sheet.width, 595.276 * K);
    close(sheet.height, 841.89 * K);
    assert_eq!(ids(&first), ["#1"]);
    assert!(first.entities.iter().all(|entity| !entity.drawn && entity.layers == [0]));
    assert_eq!(first.layers.len(), 1);
    assert_eq!(first.background, zumen_core::Background::Light);

    let underlay = &first.underlays[0];
    assert_eq!(
        (underlay.id.as_str(), underlay.kind, underlay.page, underlay.file.as_deref()),
        ("page-1", zumen_core::UnderlayKind::Pdf, Some(1), None)
    );
    close(underlay.width, 595.276);
    close(underlay.height, 841.89);
    // 中身の pt（左上が原点、y は下向き）→ 用紙の mm
    let [a, b, c, d, e, f] = underlay.matrix;
    close(a, K);
    close(d, -K);
    close(f, 841.89 * K);
    assert_eq!((b, c, e), (0.0, 0.0, 0.0));
    assert_eq!(underlay.layers, [0]);
    close(first.bounds.max_x, 595.276 * K);
    close(first.bounds.max_y, 841.89 * K);

    let second = read_view(&bytes, "2");
    assert_eq!(second.view.as_deref(), Some("2"));
    assert_eq!(second.sheet.as_ref().unwrap().size, "A3 横");
    assert_eq!(second.underlays[0].page, Some(2));
    assert_eq!(second.underlays[0].id, "page-2");
    assert_eq!(ids(&second), ["#1", "#2"]);
    let metadata: Vec<&str> = second.metadata.iter().map(|property| property.key).collect();
    assert_eq!(metadata, ["format", "sheet", "view", "pageCount"]);
    assert_eq!(second.metadata[2].value, "ページ A-II");
    assert_eq!(second.metadata[3].raw, Some(PropertyRaw::Number(3.0)));

    // ないページは 1 ページ目にして伝える
    let unknown = read_view(&bytes, "9");
    assert_eq!(unknown.view.as_deref(), Some("1"));
    assert_eq!(notice(&unknown, "unknown-page"), 1);
}

/// 経路の座標は変換行列（cm、8.4.4）を当てた用紙の mm。線の太さも変換の倍率を掛ける
#[test]
fn places_paths_in_paper_millimeters_through_the_transformation_matrix() {
    let scene = read(&one_page("q 2 0 0 2 72 72 cm 0 0 m 36 0 l S Q q 0 1 -1 0 300 100 cm 0.5 w 0 0 m 72 0 l S Q"));
    let [scaled, turned] = &scene.entities[..] else { panic!("{:?}", ids(&scene)) };
    close_point(point(scaled, "start"), [72.0 * K, 72.0 * K]);
    close_point(point(scaled, "end"), [144.0 * K, 72.0 * K]);
    // 既定の線の太さ 1 の 2 倍
    close(number(scaled, "lineWidth"), 2.0 * K);
    close_point(point(turned, "start"), [300.0 * K, 100.0 * K]);
    close_point(point(turned, "end"), [300.0 * K, 172.0 * K]);
    close(number(turned, "lineWidth"), 0.5 * K);
    close(number(turned, "length"), 72.0 * K);
}

/// 塗った経路は 1 つの図形（8.5.3）。線を引くなら線分・折線・曲線、塗るだけなら塗り。塗りと線（B）は 1 つにまとめる
#[test]
fn gives_each_painted_path_its_kind() {
    let scene = read(&one_page(
        "10 10 m 20 10 l S \
         10 10 m 20 10 l 20 20 l S \
         10 10 m 20 20 30 20 40 10 c S \
         0.2 0.4 0.6 rg 10 10 30 30 re f \
         1 0 0 RG 10 10 30 30 re B \
         10 10 m 20 10 l 30 10 m 40 10 l S \
         [6 3] 0 d 10 10 m 20 10 l 30 30 l h S",
    ));
    let kinds: Vec<EntityKind> = scene.entities.iter().map(|entity| entity.kind).collect();
    assert_eq!(
        kinds,
        [
            EntityKind::Line,
            EntityKind::Polyline,
            EntityKind::Spline,
            EntityKind::Solid,
            EntityKind::Polyline,
            EntityKind::Polyline,
            EntityKind::Polyline
        ]
    );
    // 塗りと線を 1 つの図形にしても、番号は塗った順に 1 つずつ
    assert_eq!(ids(&scene), ["#1", "#2", "#3", "#4", "#5", "#6", "#7"]);
    let [line, polyline, spline, solid, both, segments, dashed] = &scene.entities[..] else { unreachable!() };
    assert_eq!(value(line, "type"), Some("線分"));
    assert_eq!(number(polyline, "vertexCount"), 3.0);
    let Primitive::Stroke(curve) = &spline.primitives[0] else { panic!() };
    assert!(curve.points.len() > 8, "a flattened curve");
    close_point([curve.points[0], curve.points[1]], [10.0 * K, 10.0 * K]);
    close_point([curve.points[curve.points.len() - 2], curve.points[curve.points.len() - 1]], [40.0 * K, 10.0 * K]);
    assert_eq!(value(solid, "color"), Some("#336699"));
    assert!(
        matches!(&solid.primitives[..], [Primitive::Fill(fill)] if fill.rings.len() == 1 && fill.rings[0].len() == 8)
    );
    assert_eq!(value(both, "color"), Some("#ff0000"));
    assert_eq!(value(both, "fillColor"), Some("#336699"));
    assert!(matches!(&both.primitives[..], [Primitive::Fill(_), Primitive::Stroke(stroke)] if stroke.closed));
    assert_eq!(value(both, "vertexCount"), Some("4 点（閉じる）"));
    // 2 点ずつの部分経路は 1 つの線分の並び
    assert!(
        matches!(&segments.primitives[..], [Primitive::Stroke(stroke)] if stroke.segments == Some(true) && stroke.points.len() == 8)
    );
    let Primitive::Stroke(stroke) = &dashed.primitives[0] else { panic!() };
    assert!(stroke.closed);
    close(stroke.dashes[0], 6.0 * K);
    close(stroke.dashes[1], 3.0 * K);
    assert_eq!(value(dashed, "lineType"), Some("破線（2.1167, 1.0583 mm）"));
    // 閉じる線分も長さに入る
    close(number(dashed, "length"), (10.0 + 500f64.sqrt() + 800f64.sqrt()) * K);
}

/// 字形の並び（9.4）は文字。位置はベースラインの始め、大きさはフォントの大きさ（em）、角度は文字行列から
#[test]
fn reads_texts_with_their_position_height_angle_and_font() {
    let scene = read(&one_page(
        "BT /F1 12 Tf 72 720 Td (Hello) Tj ET \
         BT /F1 10 Tf 0 1 -1 0 300 400 Tm (A) Tj ET \
         q BT /F1 10 Tf 50 Tz 100 100 Td (W) Tj ET Q \
         BT /F2 20 Tf 100 600 Td <000100020003> Tj ET \
         BT /F1 10 Tf 2 Tr 100 200 Td (B) Tj ET \
         BT /F1 10 Tf ( ) Tj ET \
         BT /F1 10 Tf 3 Tr 100 300 Td (OCR) Tj ET",
    ));
    // 空白だけの字形の並びは図形にしないが、番号は使う
    assert_eq!(ids(&scene), ["#1", "#2", "#3", "#4", "#5", "#7"]);
    assert!(scene.entities.iter().all(|entity| entity.kind == EntityKind::Text));
    let [hello, turned, narrow, japanese, stroked, invisible] = &scene.entities[..] else { unreachable!() };
    assert_eq!(value(hello, "text"), Some("Hello"));
    assert_eq!(value(hello, "font"), Some("Helvetica"));
    close_point(point(hello, "position"), [72.0 * K, 720.0 * K]);
    close(number(hello, "height"), 12.0 * K);
    close(number(hello, "rotation"), 0.0);
    let Primitive::Text(text) = &hello.primitives[0] else { panic!() };
    assert_eq!(text.cells.iter().map(|cell| cell.character.as_str()).collect::<String>(), "Hello");
    close(text.height, 12.0 * K);
    // Helvetica の字送りは H 722、e 556、l 222、o 556（em の 1/1000）
    close(text.box_width, 2278.0 / 1000.0 * 12.0 * K);
    close(text.cells[0].x, 722.0 / 2000.0 * 12.0 * K);
    close(text.scale_x, 1.0);

    close(number(turned, "rotation"), 90.0);
    close_point(point(turned, "position"), [300.0 * K, 400.0 * K]);
    let Primitive::Text(text) = &narrow.primitives[0] else { panic!() };
    close(text.scale_x, 0.5);

    // Type 0 のフォントの Unicode は ToUnicode（9.10.3）。1 つの字形が 2 文字のこともある
    assert_eq!(value(japanese, "text"), Some("図面ab"));
    assert_eq!(value(japanese, "font"), Some("MS-Gothic"));
    let Primitive::Text(text) = &japanese.primitives[0] else { panic!() };
    assert_eq!(text.cells.iter().map(|cell| cell.character.as_str()).collect::<Vec<_>>(), ["図", "面", "ab"]);
    // 文字範囲は FontDescriptor の Descent から Ascent まで、セルは字送りと em の中央
    close(text.box_height, 20.0 * K);
    close(text.cells[1].x, 1.5 * 20.0 * K);
    close(text.cells[1].y, 10.0 * K);
    close(text.matrix[5], 600.0 * K - 0.12 * 20.0 * K);

    // 塗りと線の文字（描画モード 2）は 1 つ、見えない文字（3）も読む
    assert_eq!(value(stroked, "text"), Some("B"));
    assert_eq!(value(invisible, "text"), Some("OCR"));
    assert!(value(invisible, "visibility").is_some());
    assert!(value(hello, "visibility").is_none());
    assert!(scene.notices.is_empty(), "{:?}", scene.notices);
}

/// Unicode の分からない字形（ToUnicode のない Type 3 のフォント）は U+FFFD にして数える
#[test]
fn reports_glyphs_without_unicode() {
    let scene = read(&one_page("BT /F3 20 Tf 100 100 Td (aaa) Tj ET"));
    assert_eq!(value(&scene.entities[0], "text"), Some("\u{fffd}\u{fffd}\u{fffd}"));
    assert_eq!(value(&scene.entities[0], "font"), None);
    let notice = scene.notices.iter().find(|notice| notice.code == "glyph-without-unicode").unwrap();
    assert_eq!((notice.level, notice.count), (NoticeLevel::Warning, 3));
}

/// ページの範囲はクロップボックス、向きは Rotate（時計回り、7.7.3.3）。用紙はクロップボックスの左下を原点にした表示の向き
#[test]
fn turns_the_page_with_rotate_and_starts_at_the_crop_box() {
    // クロップボックスの下の辺を、左から右へ引く
    let content = "50 20 m 350 20 l S";
    let scene = |rotate: i32| {
        let mut writer = Writer::new();
        writer.page([0.0, 0.0, 400.0, 200.0], content, |page| {
            page.crop_box(Rect::new(50.0, 20.0, 350.0, 180.0)).rotate(rotate);
        });
        read(&writer.finish(|_| {}))
    };
    let (w, h) = (300.0 * K, 160.0 * K);
    for (rotate, size, start, end) in [
        (0, (300.0, 160.0), [0.0, 0.0], [w, 0.0]),
        // 時計回りに 90 度: 下の辺は左の辺になり、左の端が上へ
        (90, (160.0, 300.0), [0.0, w], [0.0, 0.0]),
        (180, (300.0, 160.0), [w, h], [0.0, h]),
        (270, (160.0, 300.0), [h, 0.0], [h, w]),
        (-90, (160.0, 300.0), [h, 0.0], [h, w]),
    ] {
        let scene = scene(rotate);
        let underlay = &scene.underlays[0];
        close(underlay.width, size.0);
        close(underlay.height, size.1);
        close(scene.sheet.as_ref().unwrap().width, size.0 * K);
        let line = &scene.entities[0];
        close_point(point(line, "start"), start);
        close_point(point(line, "end"), end);
    }
}

/// UserUnit（表 30）は既定のユーザー空間の 1 単位の大きさ（1/72 インチの倍数）。下敷きの中身は表示の pt（pdf.js の
/// getViewport と同じく UserUnit を掛けた大きさ）
#[test]
fn scales_by_user_unit() {
    let mut writer = Writer::new();
    writer.page([0.0, 0.0, 100.0, 100.0], "10 10 m 20 10 l S", |page| {
        page.user_unit(2.0);
    });
    let scene = read(&writer.finish(|_| {}));
    close(scene.sheet.as_ref().unwrap().width, 200.0 * K);
    close(scene.underlays[0].width, 200.0);
    close(scene.underlays[0].matrix[0], K);
    close_point(point(&scene.entities[0], "end"), [40.0 * K, 20.0 * K]);
}

/// フォーム XObject（8.10）の中の図形も塗った順に数え、フォームの行列を当てる
#[test]
fn numbers_form_xobjects_in_paint_order() {
    let scene = read(&one_page("0 0 m 1 0 l S /Fm1 Do 5 5 m 6 6 l S"));
    assert_eq!(ids(&scene), ["#1", "#2", "#3"]);
    let form = &scene.entities[1];
    close_point(point(form, "start"), [100.0 * K, 100.0 * K]);
    close_point(point(form, "end"), [120.0 * K, 100.0 * K]);
}

/// 画像（8.9）は枠だけの図形。画素の数を持ち、中身は下敷きのページが描く
#[test]
fn reads_images_as_frames() {
    let scene = read(&one_page("q 100 0 0 50 200 300 cm /Im1 Do Q"));
    let image = &scene.entities[0];
    assert_eq!((image.kind, image.source.as_str()), (EntityKind::Image, "image"));
    assert_eq!(raw(image, "imageSize"), Some(&PropertyRaw::Numbers(vec![2.0, 1.0])));
    close_point(point(image, "position"), [200.0 * K, 300.0 * K]);
    close(image.bbox.max_x, 300.0 * K);
    close(image.bbox.max_y, 350.0 * K);
}

/// 読まない内容（シェーディング、パターンの塗り、見えない内容、注釈、optional content）は種類と数を伝える。番号は使う
#[test]
fn reports_what_it_does_not_read() {
    let content = "/Sh1 sh \
                   /Pattern cs /P1 scn 0 0 50 50 re f \
                   q 0 0 10 10 re W n 100 100 m 200 100 l S Q \
                   -100 -100 m -50 -100 l S \
                   /OC /oc1 BDC 0 0 m 10 0 l S EMC \
                   0 0 m 20 0 l S";
    let mut writer = Writer::new();
    let (annotation, appearance) = (writer.id(), writer.id());
    writer.page(A4, content, |page| {
        page.annotations([annotation]);
    });
    writer.pdf.form_xobject(appearance, b"0 0 m 10 10 l S").bbox(Rect::new(0.0, 0.0, 10.0, 10.0));
    let mut square = writer.pdf.annotation(annotation);
    square.subtype(pdf_writer::types::AnnotationType::Square).rect(Rect::new(0.0, 0.0, 10.0, 10.0));
    square.appearance().normal().stream(appearance);
    square.finish();
    let group = writer.group;
    let bytes = writer.finish(|catalog| {
        let mut properties = catalog.insert(Name(b"OCProperties")).dict();
        properties.insert(Name(b"OCGs")).array().item(group);
        properties.insert(Name(b"D")).dict().insert(Name(b"OFF")).array().item(group);
    });
    let scene = read(&bytes);
    // シェーディングとパターンの塗り、切り抜きの外、ページの外は図形にしない。隠れた内容は hayro が渡さない
    assert_eq!(ids(&scene), ["#5"]);
    assert_eq!(notice(&scene, "shading-not-read"), 1);
    assert_eq!(notice(&scene, "pattern-not-read"), 1);
    assert_eq!(notice(&scene, "clipped-out"), 2);
    assert_eq!(notice(&scene, "annotation-not-read"), 1);
    assert_eq!(notice(&scene, "optional-content"), 1);
}

/// ビューポートの計測辞書（12.9）が直交座標系なら、範囲の中の図形に縮尺のある配置を付ける。重なれば後のビューポート
#[test]
fn places_entities_in_measured_viewports() {
    let mut writer = Writer::new();
    writer.page(A4, "10 10 m 110 10 l S 210 10 m 250 10 l S 400 400 m 410 400 l S", |page| {
        let mut viewports = page.insert(Name(b"VP")).array();
        for (bbox, name, factor, unit) in [
            ([0.0, 0.0, 300.0, 300.0], Some("平面図"), 100.0 * K, "mm"),
            ([200.0, 0.0, 300.0, 300.0], None, 50.0 * K / 1000.0, "m"),
            ([0.0, 350.0, 100.0, 450.0], Some("不明"), 1.0, "尺"),
        ] {
            let mut viewport = viewports.push().dict();
            viewport.pair(Name(b"Type"), Name(b"Viewport")).insert(Name(b"BBox")).array().items(bbox);
            if let Some(name) = name {
                viewport.pair(Name(b"Name"), TextStr(name));
            }
            let mut measure = viewport.insert(Name(b"Measure")).dict();
            measure.pair(Name(b"Type"), Name(b"Measure")).pair(Name(b"Subtype"), Name(b"RL"));
            let mut x = measure.insert(Name(b"X")).array();
            x.push()
                .dict()
                .pair(Name(b"Type"), Name(b"NumberFormat"))
                .pair(Name(b"U"), TextStr(unit))
                .pair(Name(b"C"), factor as f32);
        }
    });
    let scene = read(&writer.finish(|_| {}));
    let [plan, detail, outside] = &scene.entities[..] else { panic!() };
    let placement = plan.placement.as_deref().unwrap();
    close(placement.scale_x, 0.01);
    assert!(placement.name.starts_with("平面図（1/"), "{}", placement.name);
    // 用紙の 100 pt は実寸の 100 pt × 100
    let distance = measure_distance(
        MeasurePoint { x: 10.0 * K, y: 10.0 * K, entity: Some(plan) },
        MeasurePoint { x: 110.0 * K, y: 10.0 * K, entity: Some(plan) },
    );
    // 換算係数は 32 ビットの数で書いたので、相対の誤差で比べる
    close(distance.real.unwrap() / (100.0 * K * 100.0), 1.0);
    // 2 つ目のビューポートの中は 1/50
    close(detail.placement.as_deref().unwrap().scale_x, 0.02);
    assert!(detail.placement.as_deref().unwrap().name.starts_with("ビューポート 2"));
    // 単位の分からないビューポートは縮尺にしない
    assert!(outside.placement.is_none());
    assert_eq!(notice(&scene, "measure-unit-unknown"), 1);
}

/// 読み取りの上限（zumen_core::limits）。図形の数を超えたら途中でやめる
#[test]
fn stops_at_the_limits() {
    let bytes = one_page("0 0 m 10 0 l S 0 0 m 20 0 l S 0 0 m 30 0 l S");
    let read = |limits: ReadLimits| {
        zumen_core::read_drawing_with_limits(&bytes, &[&zumen_pdf::PDF], None, &ReadOptions::default(), &limits)
    };
    assert_eq!(read(ReadLimits::default()).unwrap().entities.len(), 3);
    let few = ReadLimits { max_entities: Some(2), ..ReadLimits::default() };
    assert_eq!(read(few).unwrap_err().exceeded(), Some(Limit::Entities));
    let small = ReadLimits { max_bytes: Some(10), ..ReadLimits::default() };
    assert_eq!(read(small).unwrap_err().exceeded(), Some(Limit::Bytes));
}

/// 読めない PDF は、利用者に見せる文のエラーにする
#[test]
fn refuses_what_is_not_a_pdf() {
    for bytes in [&b"%PDF-1.7\nnot a document"[..], b"%PDF-", &one_page("")[..40]] {
        let error = zumen_pdf::read_pdf(bytes, None).unwrap_err();
        assert!(error.message().contains("PDF"), "{}", error.message());
    }
}

/// tests/fixtures/pdf/drawing.pdf（2 ページ）。1 ページ目は A3 横で、縮尺 1/100 のビューポートの中に線・折線・曲線・
/// 塗り・フォームと、Helvetica と日本語（Type 0）の文字、画像。2 ページ目は Rotate 90 とクロップボックスのある A4
fn drawing() -> Vec<u8> {
    let mut writer = Writer::new();
    writer.page(
        [0.0, 0.0, 1190.551, 841.89],
        "0.25 w 0 0 1 RG 100 100 m 500 100 l S \
         1 0 0 RG 100 150 m 200 150 l 200 250 l 100 250 l h S \
         0 0.5 0 RG [4 2] 0 d 300 150 m 350 250 400 250 450 150 c S [] 0 d \
         0.8 0.8 0.8 rg 0 0 0 RG 520 100 80 60 re B \
         q 1 0 0 1 200 300 cm /Fm1 Do Q \
         BT /F1 14 Tf 100 400 Td (PLAN 1:100) Tj ET \
         BT /F2 12 Tf 1 0 0 1 100 450 Tm <00010002> Tj ET \
         q 60 0 0 30 700 100 cm /Im1 Do Q \
         2 w 50 50 1090.551 741.89 re S",
        |page| {
            let mut viewports = page.insert(Name(b"VP")).array();
            let mut viewport = viewports.push().dict();
            viewport
                .pair(Name(b"Type"), Name(b"Viewport"))
                .insert(Name(b"BBox"))
                .array()
                .items([90.0, 90.0, 800.0, 500.0]);
            viewport.pair(Name(b"Name"), TextStr("平面図"));
            let mut measure = viewport.insert(Name(b"Measure")).dict();
            measure
                .pair(Name(b"Type"), Name(b"Measure"))
                .pair(Name(b"Subtype"), Name(b"RL"))
                .pair(Name(b"R"), TextStr("1:100"));
            let mut x = measure.insert(Name(b"X")).array();
            x.push()
                .dict()
                .pair(Name(b"Type"), Name(b"NumberFormat"))
                .pair(Name(b"U"), TextStr("m"))
                .pair(Name(b"C"), 0.035277778f32);
        },
    );
    writer.page([0.0, 0.0, 841.89, 595.276], "BT /F1 10 Tf 60 60 Td (DETAIL) Tj ET 50 50 m 790 50 l S", |page| {
        page.crop_box(Rect::new(20.0, 20.0, 821.89, 575.276)).rotate(90);
    });
    let (cover, details) = (writer.id(), writer.id());
    writer.pdf.indirect(cover).start::<PageLabel>().style(NumberingStyle::Arabic);
    writer.pdf.indirect(details).start::<PageLabel>().prefix(TextStr("詳細-")).style(NumberingStyle::Arabic);
    writer.finish(|catalog| {
        catalog.page_labels().nums().insert(0, cover).insert(1, details);
    })
}

/// tests/fixtures/pdf の図面は、このテストの図面。@zumenjs/reader のテストが WebAssembly で読んで正解の Scene を書き
/// （pnpm run golden）、crates/zumen/tests/golden.rs が同じ Scene になることを、crates/zumen/tests/ids.rs が id を確かめる。
/// テストの図面を変えたら UPDATE_GOLDEN=1 cargo test -p zumen-pdf で書き直し、pnpm run golden を走らせ、id を記録し直す
#[test]
fn the_fixture_is_the_test_drawing() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pdf");
    let bytes = drawing();
    let path = fixtures.join("drawing.pdf");
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(&fixtures).unwrap();
        std::fs::write(&path, &bytes).unwrap();
    } else {
        assert!(
            std::fs::read(&path).is_ok_and(|written| written == bytes),
            "tests/fixtures/pdf/drawing.pdf is not the test drawing: run UPDATE_GOLDEN=1 cargo test -p zumen-pdf"
        );
    }
    let plan = read(&bytes);
    assert_eq!(plan.sheet.as_ref().unwrap().size, "A3 横");
    assert_eq!(plan.views.as_ref().unwrap()[1].name, "詳細-1");
    // 枠（最後の図形）のほかは、縮尺 1/100 のビューポートの中
    let (frame, inside) = plan.entities.split_last().unwrap();
    assert!(frame.placement.is_none());
    assert!(inside.iter().all(|entity| {
        entity.placement.as_deref().is_some_and(|placement| (placement.scale_x - 0.01).abs() < 1e-6)
    }));
    // クロップボックスの大きさを、時計回りに 90 度回した向き
    let detail = read_view(&bytes, "2");
    let sheet = detail.sheet.as_ref().unwrap();
    assert_eq!(sheet.size, "自由");
    close(sheet.width, 555.276 * K);
    close(sheet.height, 801.89 * K);
}
