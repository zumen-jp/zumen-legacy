//! AutoCAD Color Index（ACI）の 1〜255 の RGB（`@zumenjs/dxf` の colors.ts）。Autodesk は表を公開しておらず、
//! ブラウザの DXF の表示部品（dxf-parser・dxf-viewer など）と同じ表を計算で作る。10〜249 は 15 度ずつの色相 24 色 ×
//! 明るさ 5 段 × 濃・淡、250〜255 は灰色。7 は前景色（白）で、表示部品が白背景では黒にする。

use std::sync::LazyLock;
use zumen_core::js;

const LEVELS: [f64; 5] = [255.0, 204.0, 153.0, 127.0, 76.0];
const GRAYS: [u32; 6] = [51, 91, 132, 173, 214, 255];
const BASE: [[u32; 3]; 10] = [
    [0, 0, 0],
    [255, 0, 0],
    [255, 255, 0],
    [0, 255, 0],
    [0, 255, 255],
    [0, 0, 255],
    [255, 0, 255],
    [255, 255, 255],
    [128, 128, 128],
    [192, 192, 192],
];

fn to_hex(r: u32, g: u32, b: u32) -> String {
    format!("#{:02x}{:02x}{:02x}", r.min(255), g.min(255), b.min(255))
}

fn build() -> Vec<String> {
    let mut table: Vec<String> = BASE.iter().map(|[r, g, b]| to_hex(*r, *g, *b)).collect();
    for index in 10..250u32 {
        let hue = f64::from((index - 10) / 10) * 15.0;
        let shade = (index - 10) % 10;
        let level = LEVELS[(shade / 2) as usize];
        let pale = shade % 2 == 1;
        let x = 1.0 - (((hue / 60.0) % 2.0) - 1.0).abs();
        let sector = ((hue / 60.0).floor() as usize) % 6;
        let unit = [[1.0, x, 0.0], [x, 1.0, 0.0], [0.0, 1.0, x], [0.0, x, 1.0], [x, 0.0, 1.0], [1.0, 0.0, x]][sector];
        let channel = |c: f64| -> u32 {
            let value = (level * (if pale { c + (1.0 - c) / 2.0 } else { c })).floor();
            // AutoCAD の表は明るさ 153 の段で、色相 60 度以上の 153 を 152 にしている（dxf-parser などの表も同じ）
            if level == 153.0 && hue >= 60.0 && value == 153.0 { 152 } else { value as u32 }
        };
        table.push(to_hex(channel(unit[0]), channel(unit[1]), channel(unit[2])));
    }
    for gray in GRAYS {
        table.push(to_hex(gray, gray, gray));
    }
    table
}

/// ACI の色（#rrggbb）。0 は使わない
pub static ACI: LazyLock<Vec<String>> = LazyLock::new(build);

/// `ACI[Math.abs(Math.trunc(index))] ?? ACI[7]`
pub fn aci_color(index: f64) -> String {
    let i = index.trunc().abs();
    if i.is_finite() && i < ACI.len() as f64 { ACI[i as usize].clone() } else { ACI[7].clone() }
}

/// トゥルーカラー（420 の値、0xRRGGBB）に最も近い ACI（1〜255）
pub(crate) fn nearest_aci(value: f64) -> f64 {
    let rgb = js::to_int32(value) & 0xff_ffff;
    let channels = |value: i32| [(value >> 16) & 0xff, (value >> 8) & 0xff, value & 0xff];
    let target = channels(rgb);
    let mut best = (7.0, i32::MAX);
    for (index, hex) in ACI.iter().enumerate().skip(1) {
        let Ok(value) = i32::from_str_radix(hex.trim_start_matches('#'), 16) else {
            continue;
        };
        let color = channels(value);
        let distance: i32 = (0..3).map(|i| (color[i] - target[i]).pow(2)).sum();
        if distance < best.1 {
            best = (index as f64, distance);
        }
    }
    best.0
}

pub fn true_color(value: f64) -> String {
    let rgb = js::to_int32(value) & 0xff_ffff;
    to_hex(((rgb >> 16) & 0xff) as u32, ((rgb >> 8) & 0xff) as u32, (rgb & 0xff) as u32)
}

/// 透過度（440）。0x020000TT の TT が不透明さ（255 で不透明）。BYLAYER・BYBLOCK は不透明として扱う
pub fn is_transparent(value: Option<f64>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let value = js::to_int32(value);
    (value & (0xff00_0000u32 as i32)) == 0x0200_0000 && (value & 0xff) < 255
}
