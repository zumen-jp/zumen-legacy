//! DWG を読み、オブジェクトをビット列のまま写して同じ版で書き直す（開発用。ODA File Converter などで確かめる図面を作る）。
//!
//!   cargo run --release -p zumen-dwg --example rewrite -- input.dwg output.dwg
fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: rewrite input.dwg output.dwg");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&input).unwrap();
    let start = std::time::Instant::now();
    let written = zumen_dwg::rewrite_dwg(&bytes).unwrap_or_else(|error| {
        eprintln!("{input}: {error}");
        std::process::exit(1);
    });
    eprintln!("{input}: {} → {} bytes in {:?}, notices {:?}", bytes.len(), written.bytes.len(), start.elapsed(), written.notices);
    std::fs::write(&output, &written.bytes).unwrap();
}
