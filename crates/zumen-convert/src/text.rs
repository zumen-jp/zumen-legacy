//! 文字範囲に並べた文字（`TextPrimitive`）を、DXF の TEXT にする。
//!
//! SXF・JWW の文字は文字範囲（高さ・幅・文字間隔）で決まる。DXF の TEXT は、高さ（漢字・大文字の高さ）・
//! 幅の係数・回転・斜体の角度で決まり、zumen-dxf はその高さの 1/0.88（全角を含む文字）か 1/0.72 を字形の
//! em とし、em の下端から 0.12 em 上をベースラインとして描く。文字範囲の高さを em にし、ベースラインの
//! 左端を挿入点にして、元の文字範囲と同じ所に同じ大きさで描かせる。
//! 縦書きは Windows の縦書きのフォント（名前の先頭が @）を 270° 回して書く（AutoCAD の縦書きの書き方）。

use zumen_core::Matrix;
use zumen_core::TextPrimitive;
use zumen_core::text::is_half_width;
use zumen_dxf::{DxfTextBase, TEXT_DESCENT, text_em};

/// 文字列を TEXT の文字列にする。% が 2 つ続くと制御コード（%%c など）になるので、続く % の前の % を %%% にし、
/// ^ はテキストの DXF の書き方（「^ 」）にする（DXF Reference「Control Codes and Special Characters」）
pub(crate) fn escape(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, c) in chars.iter().enumerate() {
        match c {
            '%' if chars.get(index + 1) == Some(&'%') => out.push_str("%%%"),
            '^' => out.push_str("^ "),
            c => out.push(*c),
        }
    }
    out
}

/// 全角を 1、半角を 0.5 とした文字の数
fn units(text: &str) -> f64 {
    text.chars().fold(0.0, |total, c| total + if is_half_width(c) { 0.5 } else { 1.0 })
}

/// TEXT の値（style は呼び出し側で決める）。文字範囲の matrix は、文字の座標系から図形を置く座標系への変換で、
/// 回転・一様な拡大・斜体（X 方向のずれ）だけを含む
pub(crate) fn text_entity(primitive: &TextPrimitive, vertical: bool, slant: f64, style: &str) -> Option<DxfTextBase> {
    let text = primitive.text.as_str();
    let m: &Matrix = &primitive.matrix;
    let scale = m[0].hypot(m[1]);
    let total = units(text);
    if !(scale > 0.0 && total > 0.0 && primitive.box_width > 0.0 && primitive.box_height > 0.0) {
        return None;
    }
    let angle = m[1].atan2(m[0]).to_degrees();
    let origin = [m[4], m[5]];
    let em_factor = text_em(text);
    let (em, length, rotation) = if vertical {
        (primitive.box_width * scale, primitive.box_height * scale, angle - 90.0)
    } else {
        (primitive.box_height * scale, primitive.box_width * scale, angle)
    };
    let (cos, sin) = (rotation.to_radians().cos(), rotation.to_radians().sin());
    // 横書きは em の下端から 0.12 em 上がベースライン。縦書きは文字範囲の上端からベースラインが下へ進み、
    // ベースラインの左（文字範囲の左端から 0.12 em 右）を通る
    let insert = if vertical {
        let (bx, by) = (length, -TEXT_DESCENT * em);
        [origin[0] - (cos * bx - sin * by), origin[1] - (sin * bx + cos * by)]
    } else {
        let (bx, by) = (0.0, TEXT_DESCENT * em);
        [origin[0] + cos * bx - sin * by, origin[1] + sin * bx + cos * by]
    };
    Some(DxfTextBase {
        text: escape(text),
        insert: [insert[0], insert[1], 0.0],
        align_point: None,
        height: em / em_factor,
        rotation: normalize(rotation),
        oblique: if slant.is_finite() { slant.clamp(-85.0, 85.0) } else { 0.0 },
        width: length / (total * em),
        style: style.to_string(),
        halign: 0.0,
        valign: 0.0,
        text_generation_flag: 0.0,
    })
}

/// 角度を 0 以上 360 未満にする
pub(crate) fn normalize(degrees: f64) -> f64 {
    let value = degrees.rem_euclid(360.0);
    if (value - 360.0).abs() < 1e-12 { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_codes_and_carets() {
        assert_eq!(escape("50%"), "50%");
        assert_eq!(escape("%%c"), "%%%%c");
        assert_eq!(escape("a^b"), "a^ b");
        let runs = zumen_dxf::parse_text_codes(&escape("%%c 100% ^I"), 1.0, 1.0, 0.0);
        let plain: String = runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(plain, "%%c 100% ^I");
    }
}
