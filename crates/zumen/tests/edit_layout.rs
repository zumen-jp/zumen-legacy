use zumen::edit::parse_operations;
use zumen::{ReadDrawingOptions, dwg, dxf};

fn document(format: &str, bytes: &[u8]) -> dxf::DxfDocument {
    if format == "dxf" { dxf::parse_dxf(bytes) } else { dwg::read_dwg(bytes).unwrap() }
}

#[test]
fn layout_added_in_the_transaction_can_be_resized() {
    let operations = parse_operations(
        r#"[
        {"op":"add-layout","name":"S","paperSize":[100,100]},
        {"op":"update-layout","name":"S","set":{"paperSize":[420,297]}}
    ]"#,
    )
    .unwrap();
    for format in ["dxf", "dwg"] {
        let result = zumen::create(format, "mm", &operations).unwrap();
        assert_eq!(result.created, ["S"]);
        let doc = document(format, &result.bytes);
        let layout = doc.layouts.iter().find(|layout| layout.name == "S").unwrap();
        assert_eq!((layout.paper_width, layout.paper_height), (420.0, 297.0));
        let viewport = doc
            .entities
            .iter()
            .find_map(|entity| match &entity.data {
                dxf::DxfEntityData::Viewport(viewport)
                    if entity.base.block.as_deref() == Some(&layout.block) && viewport.id == 1.0 =>
                {
                    Some(viewport)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!((viewport.width, viewport.height), (420.0, 297.0));
    }
}

#[test]
fn resized_layout_keeps_plot_settings_and_converts_paper_space_units() {
    let created = zumen::create(
        "dxf",
        "mm",
        &parse_operations(
            r#"[
        {"op":"add-layout","name":"S","paperSize":[100,100],"margins":[1,2,3,4]}
    ]"#,
        )
        .unwrap(),
    )
    .unwrap();
    let mut doc = dxf::parse_dxf(&created.bytes);
    let layout = doc.layouts.iter_mut().find(|layout| layout.name == "S").unwrap();
    layout.paper_units = 0.0;
    layout.plot_scale = [2.0, 1.0];
    layout.plot_rotation = 1.0;
    layout.limmin = [1.0, 2.0];
    let operations = parse_operations(r#"[{"op":"update-layout","name":"S","set":{"paperSize":[508,254]}}]"#).unwrap();
    for format in ["dxf", "dwg"] {
        let bytes = if format == "dxf" { dxf::write_dxf(&doc).bytes } else { dwg::write_dwg(&doc).bytes };
        let before = document(format, &bytes);
        let old = before.layouts.iter().find(|layout| layout.name == "S").unwrap();
        let updated = zumen::edit(&bytes, &operations, &ReadDrawingOptions::default()).unwrap();
        let after = document(format, &updated.bytes);
        let layout = after.layouts.iter().find(|layout| layout.name == "S").unwrap();
        assert_eq!((layout.paper_width, layout.paper_height), (508.0, 254.0), "{format}");
        assert_eq!((layout.paper_units, layout.plot_scale, layout.plot_rotation), (0.0, [2.0, 1.0], 1.0), "{format}");
        assert_eq!((layout.handle.as_str(), layout.margins), (old.handle.as_str(), old.margins), "{format}");
        assert_eq!(layout.limmax, [6.0, 12.0], "{format}");
        let viewport = after
            .entities
            .iter()
            .find_map(|entity| match &entity.data {
                dxf::DxfEntityData::Viewport(viewport)
                    if entity.base.block.as_deref() == Some(&layout.block) && viewport.id == 1.0 =>
                {
                    Some(viewport)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!((viewport.width, viewport.height, viewport.center), (5.0, 10.0, [3.5, 7.0, 0.0]), "{format}");
    }
}
