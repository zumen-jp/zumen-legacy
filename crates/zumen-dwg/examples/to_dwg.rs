//! DXF・DWG を AutoCAD 2013 の DWG に書く（確かめるための例）。
//! `cargo run --release -p zumen-dwg --example to_dwg -- in.dxf out.dwg`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: to_dwg <in.dxf|in.dwg> <out.dwg>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(input).expect("read");
    let doc = if zumen_dwg::is_dwg(&bytes) {
        zumen_dwg::read_dwg(&bytes).expect("dwg")
    } else {
        zumen_dxf::parse_dxf(&bytes)
    };
    let written = zumen_dwg::write_dwg(&doc);
    for notice in &written.notices {
        eprintln!("{} ({}): {}", notice.code, notice.count, notice.message);
    }
    std::fs::write(output, &written.bytes).expect("write");
}
