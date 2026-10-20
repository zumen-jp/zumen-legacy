//! DWG をその場で編集する（開発用。ODA File Converter などで確かめる図面を作る）。操作は docs/decisions/0012 の
//! JSON の配列で、省くとレイヤを足し、線分と文字を足し、最初の図形のレイヤを変え、次の線分・円などを消す
//! （crates/zumen/tests/edit.rs と同じ操作）。
//!
//!   cargo run --release -p zumen-dwg --example edit -- input.dwg output.dwg ['[{"op": "delete", "id": "2A"}]']
fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: edit input.dwg output.dwg [operations]");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).unwrap();
    let json = args.next().unwrap_or_else(|| default_operations(&bytes));
    let operations = zumen_core::edit::parse_operations(&json).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let start = std::time::Instant::now();
    let edited = zumen_dwg::edit_dwg(&bytes, &operations).unwrap_or_else(|error| {
        eprintln!("{input}: {error}");
        std::process::exit(1);
    });
    eprintln!(
        "{input}: {} → {} bytes in {:?}, created {:?}, notices {:?}",
        bytes.len(),
        edited.bytes.len(),
        start.elapsed(),
        edited.created,
        edited.notices.iter().map(|notice| notice.code.as_str()).collect::<Vec<_>>()
    );
    std::fs::write(&output, &edited.bytes).unwrap();
}

/// 適合テストと同じ操作。変える図形はモデル空間の最初の図形、消す図形はその次の線分・円などの単純な図形
fn default_operations(bytes: &[u8]) -> String {
    let doc = zumen_dwg::read_dwg(bytes).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    let simple = ["LINE", "CIRCLE", "ARC", "LWPOLYLINE", "POINT", "TEXT", "ELLIPSE"];
    let model: Vec<&zumen_dxf::DxfEntity> =
        doc.entities.iter().filter(|entity| entity.base.space == zumen_dxf::DxfSpace::Model).collect();
    let mut operations = vec![
        serde_json::json!({"op": "add-layer", "name": "ZUMEN-EDIT-CHECK", "color": 1}),
        serde_json::json!({"op": "add", "kind": "line", "layer": "ZUMEN-EDIT-CHECK", "start": [0, 0], "end": [1000, 500]}),
        serde_json::json!({"op": "add", "kind": "text", "layer": "ZUMEN-EDIT-CHECK", "position": [0, -100], "text": "編集の確認", "height": 25}),
    ];
    if let Some(first) = model.first() {
        operations
            .push(serde_json::json!({"op": "update", "id": first.base.handle, "set": {"layer": "ZUMEN-EDIT-CHECK"}}));
    }
    if let Some(entity) = model.iter().skip(1).find(|entity| simple.contains(&entity.base.kind.as_str())) {
        operations.push(serde_json::json!({"op": "delete", "id": entity.base.handle}));
    }
    serde_json::Value::Array(operations).to_string()
}
