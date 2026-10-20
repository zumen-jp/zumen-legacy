//! 線で閉じた領域（zumen_core::regions）: tests/fixtures の図面を読み、部屋の面積と室名、縮尺の配置の実寸を確かめる。

use std::collections::HashSet;
use std::path::Path;
use zumen::regions::{ClosedRegion, RegionOptions, closed_regions};

fn scene(name: &str, view: Option<&str>) -> zumen::Scene {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures").join(name);
    let bytes = std::fs::read(path).unwrap();
    let options = zumen::ReadDrawingOptions { view: view.map(String::from), ..zumen::ReadDrawingOptions::default() };
    zumen::read(&bytes, &options).unwrap()
}

fn layers(scene: &zumen::Scene, name: &str) -> HashSet<u32> {
    scene.layers.iter().filter(|layer| layer.name == name).map(|layer| layer.id).collect()
}

fn named<'a>(regions: &'a [ClosedRegion], text: &str) -> &'a ClosedRegion {
    regions.iter().find(|region| region.texts.iter().any(|t| t.text == text)).unwrap()
}

#[test]
fn finds_the_rooms_of_a_floor_plan_on_the_wall_layer() {
    let scene = scene("dxf/rooms.dxf", None);
    let options = RegionOptions { layers: Some(layers(&scene, "WALL")), ..RegionOptions::default() };
    let regions = closed_regions(&scene, &options);
    // 外壁と内壁の間、左の部屋、間仕切りの中、右の部屋、柱
    assert_eq!(regions.len(), 5, "{regions:#?}");

    let living = named(&regions, "LDK");
    let real = living.real.as_ref().unwrap();
    // 4,850 × 5,800 mm から、半径 200 mm の柱（描いた多角形）を除く
    assert_eq!(real.outline_area, 4850.0 * 5800.0);
    assert_eq!(real.holes.len(), 1);
    let column = real.outline_area - real.area;
    assert!((column - std::f64::consts::PI * 200.0 * 200.0).abs() < 200.0, "{column}");
    assert_eq!(real.perimeter, 2.0 * (4850.0 + 5800.0));
    assert_eq!(living.placement.as_ref().unwrap().scale_x, 1.0);
    assert!(living.boundary.iter().any(|id| id == "25"), "{:?}", living.boundary);
    // 外に出た線（24）は行き止まりで、どの領域の境界にもならない
    assert!(regions.iter().all(|region| !region.boundary.iter().any(|id| id == "24")));

    let bedroom = named(&regions, "寝室");
    assert_eq!(bedroom.real.as_ref().unwrap().area, 2850.0 * 5800.0);
    assert!(bedroom.real.as_ref().unwrap().holes.is_empty());

    let walls = regions.iter().find(|region| region.boundary == ["20", "21"]).unwrap();
    assert_eq!(walls.real.as_ref().unwrap().area, 8000.0 * 6000.0 - 7800.0 * 5800.0);
    assert!(walls.texts.is_empty());
    // 外の文字（N）はどの領域にも入らない
    assert!(regions.iter().all(|region| region.texts.iter().all(|t| t.text != "N")));
}

#[test]
fn lines_on_other_layers_become_regions_and_holes() {
    let scene = scene("dxf/rooms.dxf", None);
    let regions = closed_regions(&scene, &RegionOptions::default());
    assert_eq!(regions.len(), 6);
    // 家具は寝室の穴になり、それ自身も領域になる
    let bedroom = named(&regions, "寝室");
    assert_eq!(bedroom.real.as_ref().unwrap().area, 2850.0 * 5800.0 - 1000.0 * 1000.0);
    assert!(regions.iter().any(|region| region.boundary == ["26"]));
    // 狭い領域を除く
    let large = RegionOptions { min_area: 1_000_000.0, ..RegionOptions::default() };
    assert_eq!(closed_regions(&scene, &large).len(), 4);
}

#[test]
fn gives_real_areas_through_the_scale_of_a_viewport() {
    let scene = scene("dxf/viewports.dxf", None);
    let regions = closed_regions(&scene, &RegionOptions::default());
    let region = regions
        .iter()
        .find(|region| region.placement.as_ref().is_some_and(|placement| placement.scale_x == 0.002))
        .unwrap();
    // 1:500 のビューポート: 用紙の 1 mm² は実寸の 500 × 500 mm²
    let real = region.real.as_ref().unwrap();
    assert!(
        (real.area - region.paper.area * 250_000.0).abs() < 1e-3 * real.area,
        "{} {}",
        real.area,
        region.paper.area
    );
}
