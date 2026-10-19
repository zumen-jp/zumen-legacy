//! DWG を読み、図形の種類ごとの数と、寸法のブロックの有無を表示する（開発用）
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let doc = zumen_dwg::read_dwg(&std::fs::read(&path).unwrap()).unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for entity in &doc.entities {
        *counts.entry((entity.base.kind.clone(), format!("{:?}", entity.base.space))).or_insert(0) += 1;
        if let zumen_dxf::DxfEntityData::Dimension(dimension) = &entity.data {
            let found = doc.blocks.iter().any(|block| block.name.eq_ignore_ascii_case(&dimension.anonymous_block_name));
            if !found {
                println!("dimension {} block {:?} missing", entity.base.handle, dimension.anonymous_block_name);
            }
        }
    }
    println!("{counts:?}");
    println!("blocks {:?}", doc.blocks.iter().map(|block| block.name.clone()).collect::<Vec<_>>());
    println!("notices {:?}", doc.notices);
}
