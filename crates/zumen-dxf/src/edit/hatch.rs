//! 足すハッチングの模様（docs/decisions/0017）。値の読み方と組み込みの模様は形式によらず `zumen_core::edit::hatch` に
//! あり、DXF と DWG の編集はそれを HATCH の模様の線（DXF Reference「HATCH」の Pattern Data、53・43・44・45・46・79・49）
//! に書く。

pub use zumen_core::edit::hatch::*;

/// HATCH の模様の種類（DXF Reference「HATCH」の 76: 1 定義済み、2 カスタム）
pub fn pattern_type(input: &HatchInput) -> i64 {
    if input.custom { 2 } else { 1 }
}
