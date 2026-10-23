//! 文字範囲に並べた文字（`TextPrimitive`）を、DXF の TEXT にする。
//!
//! SXF・JWW の文字は文字範囲（高さ・幅・文字間隔）で決まり、字形の em を文字範囲の高さに描く。DXF の TEXT は、
//! 高さ（漢字・大文字の高さ）・幅の係数・回転・斜体の角度で決まり、zumen-dxf はその高さの 1/0.88（全角を含む
//! 文字）か 1/0.72 を字形の em とし、em の下端から 0.12 em 上をベースラインとして描く。どちらの形式でも、
//! 使う人が入れる「文字の高さ」は文字範囲の高さと TEXT の高さで、CAD製図基準（1-5-11）の文字の高さもその値
//! なので、その値を変えずに書き、字形はそれぞれの形式の描き方で描かせる（decision 0019）。ベースラインの
//! 左端（縦書きは上端）を挿入点にし、字形の縦横の比は元の文字範囲と同じにする。
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
        // SXF・JWW の文字範囲の高さを、そのまま TEXT の高さにする（字形は DXF の描き方で 1/0.88 か 1/0.72 の em）
        height: em,
        rotation: normalize(rotation),
        oblique: if slant.is_finite() { slant.clamp(-85.0, 85.0) } else { 0.0 },
        width: length / (total * em),
        style: style.to_string(),
        halign: 0.0,
        valign: 0.0,
        text_generation_flag: 0.0,
    })
}

/// 表示の文字（字形を並べた文字範囲）が縦書きか。字の並ぶ向き（字が 1 つなら文字範囲の縦横）で決める
pub(crate) fn is_vertical(text: &TextPrimitive) -> bool {
    match text.cells.as_slice() {
        [first, second, ..] => (first.y - second.y).abs() > (first.x - second.x).abs(),
        _ => (text.height - text.box_height).abs() > 1e-9 * text.box_height.max(1e-12),
    }
}

/// DXF の表示の文字（文字範囲の高さが字形の em）を、TEXT の高さ（em の 0.88 か 0.72）を高さとする SXF・JWW の
/// 文字範囲にする（`text_entity` の逆）。TEXT の挿入点（横書きはベースラインの左端、縦書きは上端）を中心に、
/// matrix を em に対する文字の高さの比で縮める
pub(crate) fn text_height_box(text: &TextPrimitive, vertical: bool) -> TextPrimitive {
    let k = 1.0 / text_em(&text.text);
    let [px, py] = baseline_start(text, vertical);
    let m = &text.matrix;
    let mut out = text.clone();
    out.matrix = [k * m[0], k * m[1], k * m[2], k * m[3], px + k * (m[4] - px), py + k * (m[5] - py)];
    out
}

/// 文字範囲の、TEXT の挿入点（用紙の座標）。matrix は回転・一様な拡大・裏返し・斜体のずれ（文字の X 方向に、
/// 文字範囲の下端からの高さに比例する）で、斜体のずれを除いた文字の座標で、横書きは (0, 0.12 em)、縦書きは
/// (0.12 em, 文字範囲の高さ)
fn baseline_start(text: &TextPrimitive, vertical: bool) -> [f64; 2] {
    let m = &text.matrix;
    let (ax, ay) = (m[0], m[1]);
    let length = ax * ax + ay * ay;
    let shear = if length > 0.0 { (ax * m[2] + ay * m[3]) / length } else { 0.0 };
    // 文字の上の向き（斜体のずれを除く）
    let (ux, uy) = (m[2] - shear * ax, m[3] - shear * ay);
    let (x, y) =
        if vertical { (TEXT_DESCENT * text.box_width, text.box_height) } else { (0.0, TEXT_DESCENT * text.box_height) };
    [m[4] + ax * x + ux * y, m[5] + ay * x + uy * y]
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
