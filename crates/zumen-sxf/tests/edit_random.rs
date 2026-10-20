//! その場の書き直しに、決まった種の乱数で編集を当て、書いて読み直すと編集した図面と同じ表示になる

mod common;

use common::{compare_entities, read_fixture};
use std::collections::HashMap;
use zumen_sxf::{
    CompositeCurve, CompositeCurveOrg, LayerDefinition, Region, SfigOrg, Styled, SubFigure, SubFigures, SxfDrawing,
    SxfFeature, parse_sxf, sxf_to_scene, write_sxf,
};

/// 線形合同法の乱数（同じ種から同じ並び）
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
}

/// 要素の並び（用紙と複合図形）の名前。None は用紙
fn containers(drawing: &SxfDrawing) -> Vec<Option<String>> {
    let mut out = vec![None];
    out.extend(drawing.document.sub_figures.values().map(|figure| Some(figure.feature.name.clone())));
    out
}

fn members(drawing: &SxfDrawing, container: &Option<String>) -> Option<Vec<SxfFeature>> {
    match container {
        None => Some(drawing.document.sheet_members.clone()),
        Some(name) => drawing.document.sub_figures.get(name).map(|figure| figure.members.clone()),
    }
}

fn set_members(drawing: &mut SxfDrawing, container: &Option<String>, members: Vec<SxfFeature>) {
    match container {
        None => drawing.document.sheet_members = members,
        Some(name) => {
            let mut figure = drawing.document.sub_figures.get(name).unwrap().clone();
            figure.members = members;
            drawing.document.sub_figures.set(name.clone(), figure);
        }
    }
}

/// 要素の座標をずらす（ずらせない種類なら false）
fn shift(feature: &mut SxfFeature, dx: f64) -> bool {
    match feature {
        SxfFeature::Line { x1, x2, .. } => {
            *x1 += dx;
            *x2 += dx;
        }
        SxfFeature::Circle { cx, .. } | SxfFeature::Arc { cx, .. } | SxfFeature::Ellipse { cx, .. } => *cx += dx,
        SxfFeature::Polyline { xs, .. } => xs.iter_mut().for_each(|x| *x += dx),
        SxfFeature::TextString { text, .. } => text.x += dx,
        SxfFeature::PointMarker { x, .. } | SxfFeature::SfigLocate { x, .. } => *x += dx,
        _ => return false,
    }
    true
}

/// 乱数で編集を 1 つ当てる
fn edit(drawing: &mut SxfDrawing, random: &mut Random, serial: &mut f64) {
    let containers = containers(drawing);
    let container = containers[random.below(containers.len())].clone();
    let Some(mut list) = members(drawing, &container) else {
        return;
    };
    match random.below(11) {
        0 | 1 if !list.is_empty() => {
            let index = random.below(list.len());
            if shift(&mut list[index], 10.0 + random.below(100) as f64) {
                set_members(drawing, &container, list);
            }
        }
        2 if !list.is_empty() => {
            // 複合図形の配置は、配置の循環を作らないよう消すだけにする
            let index = random.below(list.len());
            list.remove(index);
            set_members(drawing, &container, list);
        }
        3 => {
            *serial += 1.0;
            let layers = drawing.document.layers.len();
            let index = random.below(list.len() + 1);
            list.insert(
                index,
                SxfFeature::Line {
                    style: Styled {
                        id: *serial,
                        layer: random.below(layers + 1) as f64,
                        color: 1.0 + random.below(16) as f64,
                        line_type: 1.0 + random.below(15) as f64,
                        line_width: 1.0 + random.below(9) as f64,
                    },
                    x1: random.below(500) as f64,
                    y1: random.below(500) as f64,
                    x2: random.below(500) as f64,
                    y2: random.below(500) as f64,
                },
            );
            set_members(drawing, &container, list);
        }
        4 if !drawing.document.layers.is_empty() => {
            let index = random.below(drawing.document.layers.len());
            let layer = &mut drawing.document.layers[index];
            layer.visible = !layer.visible;
        }
        5 => {
            *serial += 1.0;
            drawing
                .document
                .layers
                .push(LayerDefinition { id: *serial, name: format!("追加{serial}"), visible: true });
        }
        6 if !drawing.document.layers.is_empty() => {
            let index = random.below(drawing.document.layers.len());
            drawing.document.layers[index].name = format!("改名{serial}");
        }
        7 if drawing.document.sub_figures.len() > 1 => {
            // 複合図形を 1 つ消し、それを置く配置も消す
            let names: Vec<String> = drawing.document.sub_figures.values().map(|f| f.feature.name.clone()).collect();
            let name = names[random.below(names.len())].clone();
            let placed = |feature: &SxfFeature| matches!(feature, SxfFeature::SfigLocate { name: placed, .. } if *placed == name);
            let mut figures = SubFigures::default();
            for figure in drawing.document.sub_figures.values().filter(|figure| figure.feature.name != name) {
                let mut figure = figure.clone();
                figure.members.retain(|feature| !placed(feature));
                figures.set(figure.feature.name.clone(), figure);
            }
            drawing.document.sub_figures = figures;
            drawing.document.sheet_members.retain(|feature| !placed(feature));
        }
        8 => {
            // 新しい作図部品を作って用紙に置く
            *serial += 1.0;
            let name = format!("部品{serial}");
            let member = SxfFeature::Circle {
                style: Styled { id: *serial + 0.5e6, layer: 0.0, color: 3.0, line_type: 1.0, line_width: 2.0 },
                cx: 0.0,
                cy: 0.0,
                r: 5.0 + random.below(10) as f64,
            };
            drawing.document.sub_figures.set(
                name.clone(),
                SubFigure { feature: SfigOrg { id: *serial, name: name.clone(), flag: 4.0 }, members: vec![member] },
            );
            *serial += 1.0;
            drawing.document.sheet_members.push(SxfFeature::SfigLocate {
                id: *serial,
                layer: 0.0,
                name,
                x: random.below(400) as f64,
                y: random.below(400) as f64,
                angle: random.below(360) as f64,
                ratio_x: 1.0,
                ratio_y: 1.0,
            });
        }
        9 => {
            // 新しい複合曲線と、それを外形にする塗りを用紙に置く
            *serial += 1.0;
            let (x, y) = (random.below(400) as f64, random.below(400) as f64);
            drawing.document.composite_curves.push(CompositeCurve {
                feature: CompositeCurveOrg {
                    id: *serial,
                    color: 4.0,
                    line_type: 1.0,
                    line_width: 1.0,
                    visible: random.below(2) == 0,
                },
                members: vec![SxfFeature::Polyline {
                    style: Styled { id: *serial + 0.5e6, layer: 0.0, color: 4.0, line_type: 1.0, line_width: 1.0 },
                    xs: vec![x, x + 30.0, x + 30.0, x],
                    ys: vec![y, y, y + 20.0, y],
                }],
            });
            let number = drawing.document.composite_curves.len() as f64;
            *serial += 1.0;
            drawing.document.sheet_members.push(SxfFeature::FillAreaStyleColour {
                id: *serial,
                layer: 0.0,
                color: 1.0 + random.below(16) as f64,
                region: Region { out_id: number, in_ids: vec![] },
            });
        }
        _ if !list.is_empty() && containers.len() > 1 => {
            // 要素を別の並びに移す（配置は循環を作らないよう移さない）
            let index = random.below(list.len());
            if matches!(list[index], SxfFeature::SfigLocate { .. }) {
                return;
            }
            let feature = list.remove(index);
            set_members(drawing, &container, list);
            let target = containers[random.below(containers.len())].clone();
            if let Some(mut other) = members(drawing, &target) {
                let at = random.below(other.len() + 1);
                other.insert(at, feature);
                set_members(drawing, &target, other);
            }
        }
        _ => {}
    }
}

fn check(name: &str, format: &str, bytes: &[u8], seeds: u64, steps: usize) {
    for seed in 0..seeds {
        let original = parse_sxf(bytes, format).unwrap();
        let mut drawing = original.clone();
        let mut random = Random(seed * 7919 + 17);
        let mut serial = 9_000_000.0;
        for _ in 0..steps {
            edit(&mut drawing, &mut random, &mut serial);
        }
        let written = write_sxf(&drawing, format).unwrap();
        let again = parse_sxf(&written.bytes, format).unwrap();
        let ids: HashMap<u64, f64> = written.ids.iter().map(|(id, new)| (id.to_bits(), *new)).collect();
        let (before, after) = (sxf_to_scene(&drawing), sxf_to_scene(&again));
        // P21 から読んだ図面は同じインスタンスを複数の並びに入れることがあり、番号の読み替えが一意に決まらないので、
        // id は SFC だけで比べる（図形は並びの順に比べる）
        let ids = (format == "sfc").then_some(&ids);
        compare_entities(&before, &after, ids, 1e-9)
            .unwrap_or_else(|error| panic!("{name}.{format} seed {seed}: {error}"));
        let names = |scene: &zumen_core::Scene| {
            let mut names: Vec<(String, bool)> =
                scene.layers.iter().map(|layer| (layer.name.clone(), layer.visible)).collect();
            names.sort();
            names
        };
        // 要素のないレイヤは P21 に書けないので、レイヤの一覧は要素のあるものだけを比べる
        if format == "sfc" {
            assert_eq!(names(&before), names(&after), "{name}.{format} seed {seed}");
        }
    }
}

#[test]
fn random_edits_read_back_as_the_edited_drawing() {
    for (format, name) in [("sfc", "sample"), ("sfc", "features"), ("p21", "sample"), ("p21", "features")] {
        check(name, format, &read_fixture(format, name), 40, 15);
    }
}

#[test]
#[ignore = "ZUMEN_SAMPLES の図面を使う"]
fn random_edits_of_local_samples_read_back_as_the_edited_drawing() {
    let Ok(folder) = std::env::var("ZUMEN_SAMPLES") else {
        return;
    };
    let mut paths: Vec<_> = std::fs::read_dir(folder).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        let Some(format) = path.extension().map(|ext| ext.to_string_lossy().to_lowercase()) else {
            continue;
        };
        if matches!(format.as_str(), "sfc" | "p21") {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            check(&name, &format, &std::fs::read(&path).unwrap(), 10, 30);
        }
    }
}
