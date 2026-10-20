//! 書き出す SXF の版（write_sxf_version）: Ver.2.0・3.0・3.1 のヘッダと SFC の囲みで書き、その版にないフィーチャを
//! 近い形にする（Ver.3.0 で図面表題欄、Ver.3.1 でクロソイド曲線・弧長寸法が加わった）

mod common;

use common::read_fixture;
use zumen_core::{Primitive, Scene};
use zumen_sxf::{
    FeatureKind, SxfDrawing, SxfFeature, SxfVersion, parse_sxf, sxf_to_scene, write_sxf, write_sxf_version, written_ids,
};

fn codes(notices: &[zumen_core::Notice]) -> Vec<&str> {
    notices.iter().map(|notice| notice.code.as_str()).collect()
}

fn text(bytes: &[u8]) -> String {
    encoding_rs::SHIFT_JIS.decode(bytes).0.into_owned()
}

/// 図面のすべての要素
fn features(drawing: &SxfDrawing) -> Vec<&SxfFeature> {
    let doc = &drawing.document;
    doc.sheet_members
        .iter()
        .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
        .chain(doc.composite_curves.iter().flat_map(|curve| curve.members.iter()))
        .collect()
}

fn count(drawing: &SxfDrawing, kind: FeatureKind) -> usize {
    features(drawing).iter().filter(|feature| feature.kind() == kind).count()
}

/// 線の要素の点列の、相手の点列までの距離の最大（ハウスドルフ距離）
fn stroke_distance(a: &Primitive, b: &Primitive) -> f64 {
    let points = |primitive: &Primitive| match primitive {
        Primitive::Stroke(stroke) => stroke.points.clone(),
        _ => Vec::new(),
    };
    let (pa, pb) = (points(a), points(b));
    let one = |from: &[f64], to: &[f64]| {
        from.chunks(2)
            .map(|p| {
                to.windows(4)
                    .step_by(2)
                    .map(|s| {
                        zumen_core::geometry::distance_to_segment_squared(p[0], p[1], s[0], s[1], s[2], s[3]).sqrt()
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max)
    };
    one(&pa, &pb).max(one(&pb, &pa))
}

#[test]
fn reads_version_names() {
    assert_eq!(SxfVersion::parse("3.1"), Some(SxfVersion::V3_1));
    assert_eq!(SxfVersion::parse("SXF 3.0"), Some(SxfVersion::V3_0));
    assert_eq!(SxfVersion::parse("Ver.2.0"), Some(SxfVersion::V2_0));
    assert_eq!(SxfVersion::parse("2"), Some(SxfVersion::V2_0));
    assert_eq!(SxfVersion::parse("4.0"), None);
    assert_eq!(SxfVersion::default(), SxfVersion::V3_1);
    assert!(SxfVersion::V2_0 < SxfVersion::V3_0 && SxfVersion::V3_0 < SxfVersion::V3_1);
    assert_eq!(SxfVersion::ALL.map(SxfVersion::as_str), ["2.0", "3.0", "3.1"]);
}

#[test]
fn writes_each_version_with_its_header_markers_and_features() {
    let bytes = read_fixture("sfc", "features");
    let drawing = parse_sxf(&bytes, "sfc").unwrap();
    assert_eq!(SxfVersion::of(&drawing), Some(SxfVersion::V3_1));
    let original = sxf_to_scene(&drawing);
    // 同じ版は元のファイルを書き換える（変えていないので元のバイト列）
    let same = write_sxf_version(&drawing, "sfc", SxfVersion::V3_1).unwrap();
    assert_eq!(same.bytes, bytes);
    assert!(same.notices.is_empty());
    for version in [SxfVersion::V2_0, SxfVersion::V3_0] {
        let written = write_sxf_version(&drawing, "sfc", version).unwrap();
        let content = text(&written.bytes);
        assert!(content.contains(&format!("'zumen$${}'", version.as_str())), "{version:?}");
        // その版で加わったフィーチャはない。Ver.3.1 の囲みもない
        assert!(!content.contains("/*SXF3.1"));
        assert!(!content.contains("clothoid_feature") && !content.contains("curve_dim_feature"));
        assert_eq!(content.contains("/*SXF3\n#"), version == SxfVersion::V3_0, "{version:?}");
        assert_eq!(content.contains("drawing_attribute_feature"), version == SxfVersion::V3_0);
        let notices = codes(&written.notices);
        assert!(notices.contains(&"clothoid") && notices.contains(&"curve-dim"), "{notices:?}");
        assert_eq!(notices.contains(&"drawing-attribute"), version == SxfVersion::V2_0);
        // 読み直すとその版で、クロソイド曲線は同じ形の折線、弧長寸法は同じ形の角度寸法
        let again = parse_sxf(&written.bytes, "sfc").unwrap();
        assert_eq!(SxfVersion::of(&again), Some(version));
        assert_eq!(count(&again, FeatureKind::Clothoid), 0);
        assert_eq!(count(&again, FeatureKind::CurveDim), 0);
        assert_eq!(
            count(&again, FeatureKind::AngularDim),
            count(&drawing, FeatureKind::AngularDim) + count(&drawing, FeatureKind::CurveDim)
        );
        assert_eq!(again.document.title.is_some(), version == SxfVersion::V3_0);
        let scene = sxf_to_scene(&again);
        let ids = written_ids(&written);
        check_shapes(&drawing, &original, &scene, &ids);
    }
}

/// クロソイド曲線の接線の回る角（s²/2A² の差、ラジアン）
fn turning(drawing: &SxfDrawing, id: &str) -> Option<f64> {
    let number: f64 = id.rsplit('#').next()?.parse().ok()?;
    features(drawing).into_iter().find_map(|feature| match feature {
        SxfFeature::Clothoid { style, parameter, start_length, end_length, .. } if style.id == number => {
            Some((end_length * end_length - start_length * start_length).abs() / (2.0 * parameter * parameter))
        }
        _ => None,
    })
}

/// 版を変えて書いた図面の表示が、元の表示と同じ形（クロソイド曲線は折線、弧長寸法は角度寸法の種類）
fn check_shapes(drawing: &SxfDrawing, original: &Scene, scene: &Scene, ids: &std::collections::HashMap<u64, f64>) {
    assert_eq!(original.entities.len(), scene.entities.len());
    for (a, b) in original.entities.iter().zip(&scene.entities) {
        assert_eq!(common::renumbered(&a.id, ids), b.id);
        assert_eq!(a.primitives.len(), b.primitives.len(), "{}", a.id);
        if a.type_name == "クロソイド" {
            assert_eq!(b.type_name, "折線");
            // 2 周より多く巻く渦（#802 は A = 0.1 で 30 の長さ）は、表示の点列も曲線を追えないので比べない
            if turning(drawing, &a.id).is_some_and(|angle| angle <= 4.0 * std::f64::consts::PI) {
                let distance = stroke_distance(&a.primitives[0], &b.primitives[0]);
                assert!(distance < 0.05, "{}: {distance}", a.id);
            }
            continue;
        }
        if a.type_name == "弧長寸法" {
            assert_eq!(b.type_name, "角度寸法");
        } else {
            assert_eq!(a.type_name, b.type_name);
        }
        let (pa, pb) = (serde_json::to_value(&a.primitives).unwrap(), serde_json::to_value(&b.primitives).unwrap());
        if let Err(difference) = common::close_json(&pa, &pb, 1e-6, &a.id) {
            // 輪郭にクロソイド曲線を持つ複合曲線・塗り・ハッチングは、点列が折線の点になる（形は同じ）
            for (x, y) in a.primitives.iter().zip(&b.primitives) {
                let distance = match (x, y) {
                    (Primitive::Stroke(_), Primitive::Stroke(_)) => stroke_distance(x, y),
                    _ => rings(x).iter().zip(rings(y)).map(|(ra, rb)| ring_distance(ra, &rb)).fold(0.0, f64::max),
                };
                assert!(distance < 0.05, "{difference}: {distance}");
            }
        }
    }
}

fn rings(primitive: &Primitive) -> Vec<Vec<f64>> {
    match primitive {
        Primitive::Fill(fill) => fill.rings.clone(),
        Primitive::Clipped(clipped) => clipped.rings.clone(),
        other => panic!("{other:?}"),
    }
}

/// 閉じた点列どうしのハウスドルフ距離
fn ring_distance(a: &[f64], b: &[f64]) -> f64 {
    let close = |ring: &[f64]| {
        let mut ring = ring.to_vec();
        let start = [ring[0], ring[1]];
        ring.extend_from_slice(&start);
        ring
    };
    let as_stroke = |ring: &[f64]| {
        Primitive::Stroke(zumen_core::StrokePrimitive {
            points: close(ring),
            closed: false,
            segments: None,
            color: String::new(),
            width: 0.0,
            dashes: Vec::new(),
        })
    };
    stroke_distance(&as_stroke(a), &as_stroke(b))
}

#[test]
fn writes_p21_and_compressed_sxf_at_the_chosen_version() {
    let drawing = parse_sxf(&read_fixture("sfc", "features"), "sfc").unwrap();
    for format in ["p21", "sfz", "p2z"] {
        let written = write_sxf_version(&drawing, format, SxfVersion::V2_0).unwrap();
        let again = parse_sxf(&written.bytes, format).unwrap();
        assert_eq!(SxfVersion::of(&again), Some(SxfVersion::V2_0), "{format}");
        assert_eq!(count(&again, FeatureKind::Clothoid), 0, "{format}");
        assert_eq!(count(&again, FeatureKind::CurveDim), 0, "{format}");
        assert!(again.document.title.is_none(), "{format}");
    }
    // 圧縮した SXF から読んだ図面も、違う版は書庫を作り直す
    let sfz = write_sxf(&drawing, "sfz").unwrap();
    let read = parse_sxf(&sfz.bytes, "sfz").unwrap();
    let older = write_sxf_version(&read, "sfz", SxfVersion::V3_0).unwrap();
    assert_eq!(SxfVersion::of(&parse_sxf(&older.bytes, "sfz").unwrap()), Some(SxfVersion::V3_0));
    let same = write_sxf_version(&read, "sfz", SxfVersion::V3_1).unwrap();
    assert_eq!(same.bytes, sfz.bytes, "同じ版は元の書庫のまま");
}

#[test]
fn writes_new_drawings_at_the_version_their_features_need() {
    // features.p21 は Ver.3.0 と書いてあるが、Ver.3.1 のクロソイド曲線・弧長寸法を持つ。版を選ばずに新しく書くときは
    // フィーチャを変えず、要る版（Ver.3.1）で書く
    let mut drawing = parse_sxf(&read_fixture("p21", "features"), "p21").unwrap();
    assert_eq!(SxfVersion::of(&drawing), Some(SxfVersion::V3_0));
    assert!(count(&drawing, FeatureKind::Clothoid) > 0);
    drawing.source = None;
    let written = write_sxf(&drawing, "sfc").unwrap();
    assert!(written.notices.is_empty(), "{:?}", written.notices);
    assert!(text(&written.bytes).contains("'zumen$$3.1'"));
    assert!(text(&written.bytes).contains("/*SXF3.1\n#"));
    // 新しいフィーチャのない図面は、図面の版のまま
    let mut plain = parse_sxf(&read_fixture("sfc", "sample"), "sfc").unwrap();
    plain.source = None;
    plain.version = "SXF 2.0".into();
    let written = write_sxf(&plain, "p21").unwrap();
    assert!(String::from_utf8_lossy(&written.bytes).contains("'zumen$$2.0'"));
}
