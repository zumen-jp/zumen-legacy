//! DXF・DWG を DWG に書く（確かめるための例）。版を省くと AutoCAD 2013
//! `cargo run --release -p zumen-dwg --example to_dwg -- in.dxf out.dwg [2000|2004|2007|2010|2013|2018]`
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: to_dwg <in.dxf|in.dwg> <out.dwg> [version]");
        std::process::exit(2);
    };
    let version = match args.get(3) {
        Some(name) => zumen_dwg::DwgVersion::parse(name).unwrap_or_else(|| {
            eprintln!("unknown version {name}");
            std::process::exit(2);
        }),
        None => zumen_dwg::DwgVersion::R2013,
    };
    let bytes = std::fs::read(input).expect("read");
    let doc = if zumen_dwg::is_dwg(&bytes) {
        zumen_dwg::read_dwg(&bytes).expect("dwg")
    } else {
        zumen_dxf::parse_dxf(&bytes)
    };
    let written = zumen_dwg::write_dwg_as(&doc, version);
    for notice in &written.notices {
        eprintln!("{} ({}): {}", notice.code, notice.count, notice.message);
    }
    std::fs::write(output, &written.bytes).expect("write");
}
