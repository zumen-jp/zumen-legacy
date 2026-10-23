//! 壊した PDF を読み、panic と時間のかかりすぎを探す（開発用。決定 0020）。
//!
//!   RUSTFLAGS="-C overflow-checks=on -C debug-assertions=on" CARGO_TARGET_DIR=target/fuzz \
//!     cargo build --release -p zumen-pdf --example fuzz
//!   target/fuzz/release/examples/fuzz <rounds> <seed> drawing.pdf…
//!
//! 図面ごとに rounds 回、途中で切る・1〜8 バイトを変える・最大 64 バイトを消す・一部を繰り返して挿むのどれかで壊し、
//! 読めたらすべてのページを読む。panic した入力は一時フォルダーに書き、壊す前の図面の 4 倍と 2 秒のどちらよりも時間が
//! かかったものを SLOW と表示する。壊す前の図面の 20 倍と 60 秒のどちらも超えたら、入力を書いて HANG と表示して終える。
//! hayro の中で起きた panic と止まらない読み取りは、壊した入力とともに hayro に伝える（読み取りの上限は hayro の中の
//! 処理を止められない）。時間は実時間なので、ほかの処理で混んだ機械では誤って SLOW・HANG になることがある。
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static LAST_PANIC: Mutex<String> = Mutex::new(String::new());
/// 読んでいる入力。見張りのスレッドが見る
static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

struct Current {
    start: Instant,
    /// 止まったとみなす時間
    hang: Duration,
    label: String,
    bytes: Vec<u8>,
}

fn keep(name: &str, bytes: &[u8]) -> PathBuf {
    let out = std::env::temp_dir().join(format!("zumen-pdf-fuzz-{name}.pdf"));
    std::fs::write(&out, bytes).expect("write the input");
    out
}

fn damage(original: &[u8], round: usize, random: &mut impl FnMut(usize) -> usize) -> Vec<u8> {
    let mut bytes = original.to_vec();
    match round % 4 {
        0 => bytes.truncate(random(bytes.len())),
        1 => {
            for _ in 0..1 + random(8) {
                let at = random(bytes.len());
                bytes[at] = random(256) as u8;
            }
        }
        2 => {
            let at = random(bytes.len());
            let length = random(64).min(bytes.len() - at);
            bytes.drain(at..at + length);
        }
        _ => {
            let at = random(bytes.len());
            let length = random(16).min(bytes.len() - at);
            let copy = bytes[at..at + length].to_vec();
            bytes.splice(at..at, copy.iter().copied().cycle().take(length * random(4)));
        }
    }
    bytes
}

/// 読み、読めたらほかのページも読む。読めたかを返す
fn read_and_draw(bytes: &[u8]) -> bool {
    let Ok(scene) = zumen_pdf::read_pdf(bytes, None) else { return false };
    for view in scene.views.iter().flatten().skip(1) {
        let _ = zumen_pdf::read_pdf(bytes, Some(&view.id));
    }
    true
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: fuzz <rounds> <seed> drawing.pdf…");
        std::process::exit(2);
    }
    let rounds: usize = args[1].parse().expect("rounds");
    let first_seed: u64 = args[2].parse().expect("seed");
    let mut seed = first_seed;
    let mut random = move |bound: usize| {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize % bound.max(1)
    };
    panic::set_hook(Box::new(|info| *LAST_PANIC.lock().unwrap() = info.to_string()));
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(1));
            if let Some(current) = &*CURRENT.lock().unwrap()
                && current.start.elapsed() > current.hang
            {
                println!("HANG {}\n  input: {}", current.label, keep(&current.label, &current.bytes).display());
                std::process::exit(1);
            }
        }
    });
    let (mut read, mut failed, mut panicked, mut slow) = (0, 0, 0, 0);
    for path in &args[3..] {
        let original = std::fs::read(path).expect("read the drawing");
        let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().into_owned();
        let start = Instant::now();
        read_and_draw(&original);
        let baseline = start.elapsed().as_secs_f64();
        let limit = (baseline * 4.0).max(2.0);
        let hang = Duration::from_secs_f64((baseline * 20.0).max(60.0));
        for round in 0..rounds {
            let bytes = damage(&original, round, &mut random);
            let label = format!("{first_seed}-{name}-{round}");
            let start = Instant::now();
            *CURRENT.lock().unwrap() = Some(Current { start, hang, label: label.clone(), bytes: bytes.clone() });
            let result = panic::catch_unwind(AssertUnwindSafe(|| read_and_draw(&bytes)));
            let elapsed = start.elapsed();
            *CURRENT.lock().unwrap() = None;
            match result {
                Ok(true) => read += 1,
                Ok(false) => failed += 1,
                Err(_) => {
                    panicked += 1;
                    let out = keep(&label, &bytes);
                    println!("PANIC {name} round {round}: {}\n  input: {}", LAST_PANIC.lock().unwrap(), out.display());
                }
            }
            if elapsed.as_secs_f64() > limit {
                slow += 1;
                println!("SLOW {name} round {round}: {:.1} s", elapsed.as_secs_f64());
            }
        }
    }
    println!("{} files × {rounds}: read {read}, failed {failed}, panicked {panicked}, slow {slow}", args.len() - 3);
}
