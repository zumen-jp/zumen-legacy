//! ページのビューポートと計測辞書（12.9）から、縮尺のある配置を作る
//!
//! ビューポート（表 260）は、ページの矩形の範囲と、その中の計測のしかた（計測辞書、表 261）を持つ。計測辞書が
//! 直交座標系（Subtype RL）なら、X の最初の数の書き方（表 262）の換算係数 C と単位 U から、用紙の 1 mm が実寸の
//! 何 mm かが分かる。範囲の中の図形にその配置を付け、計測で実寸を出せるようにする（DXF のビューポートと同じ）。

use crate::page::Geometry;
use crate::strings;
use hayro_interpret::hayro_syntax::object::{Array, Dict, Name, Rect as PdfRect, String as PdfString};
use hayro_interpret::hayro_syntax::page::Page;
use kurbo::Point;
use std::sync::Arc;
use zumen_core::geometry::{empty_box, extend_box};
use zumen_core::{BBox, NoticeLevel, NoticeList, Placement, SceneEntity, js, limits};

/// 読むビューポートの数の上限
const MAX_VIEWPORTS: usize = 1024;

/// 縮尺のあるビューポート
pub(crate) struct Viewport {
    /// 範囲（用紙の mm）
    bbox: BBox,
    placement: Arc<Placement>,
}

impl Viewport {
    /// 範囲の中に収まる図形に配置を付ける。重なるビューポートは、配列の後ろのものを使う（12.9.2）ので、
    /// 配列の順に当てて後のもので上書きする
    pub(crate) fn place(&self, entities: &mut [SceneEntity]) {
        const SLACK: f64 = 1e-6;
        for entity in entities {
            let inside = entity.bbox.min_x >= self.bbox.min_x - SLACK
                && entity.bbox.max_x <= self.bbox.max_x + SLACK
                && entity.bbox.min_y >= self.bbox.min_y - SLACK
                && entity.bbox.max_y <= self.bbox.max_y + SLACK;
            if inside {
                entity.placement = Some(self.placement.clone());
            }
        }
    }
}

/// ページの VP（表 30）のビューポートのうち、直交座標系の計測辞書を持つもの
pub(crate) fn viewports(page: &Page<'_>, geometry: &Geometry, notices: &mut NoticeList) -> Vec<Viewport> {
    let Some(array) = page.raw().get::<Array<'_>>(b"VP") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (index, viewport) in array.iter::<Dict<'_>>().take(MAX_VIEWPORTS).enumerate() {
        if limits::tick() {
            break;
        }
        let Some(measure) = viewport.get::<Dict<'_>>(b"Measure") else { continue };
        if measure.get::<Name<'_>>(b"Subtype").as_deref().is_some_and(|subtype| subtype != b"RL") {
            notices.add(
                NoticeLevel::Info,
                "measure-not-read",
                "地理座標系の計測辞書（GEO）は縮尺として読みませんでした",
            );
            continue;
        }
        let Some(rect) = viewport.get::<PdfRect>(b"BBox") else { continue };
        let Some((x_factor, x_unit)) = first_format(&measure, b"X") else {
            notices.add(
                NoticeLevel::Warning,
                "measure-unit-unknown",
                "単位の分からない計測辞書は縮尺として読みませんでした",
            );
            continue;
        };
        // Y がなければ、y も X で測る
        let (y_factor, y_unit) = match measure.get::<Array<'_>>(b"Y") {
            Some(_) => match first_format(&measure, b"Y") {
                Some(format) => format,
                None => {
                    notices.add(
                        NoticeLevel::Warning,
                        "measure-unit-unknown",
                        "単位の分からない計測辞書は縮尺として読みませんでした",
                    );
                    continue;
                }
            },
            None => (x_factor, x_unit),
        };
        // ユーザー空間の 1 単位は用紙の geometry.scale mm で、実寸の C × 単位の mm
        let scale_x = geometry.scale / (x_factor * x_unit);
        let scale_y = geometry.scale / (y_factor * y_unit);
        if !(scale_x.is_finite() && scale_x > 0.0 && scale_y.is_finite() && scale_y > 0.0) {
            continue;
        }
        // 原点は O（既定のユーザー空間）、なければ範囲の最初の角
        let origin = measure.get::<[f64; 2]>(b"O").unwrap_or([rect.x0, rect.y0]);
        let origin = geometry.user_space * Point::new(origin[0], origin[1]);
        let mut bbox = empty_box();
        for (x, y) in [(rect.x0, rect.y0), (rect.x1, rect.y0), (rect.x1, rect.y1), (rect.x0, rect.y1)] {
            let corner = geometry.user_space * Point::new(x, y);
            extend_box(&mut bbox, corner.x, corner.y);
        }
        let name = viewport
            .get::<PdfString<'_>>(b"Name")
            .map(|name| strings::text_string(name.as_bytes()))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("ビューポート {}", index + 1));
        let ratio = measure
            .get::<PdfString<'_>>(b"R")
            .map(|ratio| strings::text_string(ratio.as_bytes()))
            .filter(|ratio| !ratio.trim().is_empty())
            .unwrap_or_else(|| format!("1/{}", js::format_ja(1.0 / scale_x, 4)));
        // 用紙の座標（mm）→ 実寸の座標（mm）。向きはページの回転を済ませた用紙の向き
        let inverse = [1.0 / scale_x, 0.0, 0.0, 1.0 / scale_y, -origin.x / scale_x, -origin.y / scale_y];
        out.push(Viewport {
            bbox,
            placement: Arc::new(Placement { name: format!("{name}（{ratio}）"), scale_x, scale_y, inverse }),
        });
    }
    out
}

/// 数の書き方の配列（表 262）の最初の要素の換算係数 C（ユーザー空間の 1 単位が U の何倍か）と、単位 U の mm
fn first_format(measure: &Dict<'_>, key: &[u8]) -> Option<(f64, f64)> {
    let format = measure.get::<Array<'_>>(key)?.iter::<Dict<'_>>().next()?;
    let factor = format.get::<f64>(b"C").filter(|factor| factor.is_finite() && *factor > 0.0)?;
    let unit = format.get::<PdfString<'_>>(b"U").map(|unit| strings::text_string(unit.as_bytes()))?;
    Some((factor, unit_mm(&unit)?))
}

/// 単位の名前（U は人に見せる文字列で、決まった値はない）の 1 単位が何 mm か。Acrobat が書く名前を読む
fn unit_mm(unit: &str) -> Option<f64> {
    Some(match unit.trim().to_lowercase().as_str() {
        "mm" | "millimeter" | "millimeters" | "millimetre" | "millimetres" | "ミリ" | "ミリメートル" => 1.0,
        "cm" | "centimeter" | "centimeters" | "centimetre" | "centimetres" | "センチ" | "センチメートル" => {
            10.0
        }
        "m" | "meter" | "meters" | "metre" | "metres" | "メートル" => 1000.0,
        "km" | "kilometer" | "kilometers" | "kilometre" | "kilometres" | "キロメートル" => 1_000_000.0,
        "in" | "inch" | "inches" | "\"" => 25.4,
        "ft" | "foot" | "feet" | "'" => 304.8,
        "yd" | "yard" | "yards" => 914.4,
        "mi" | "mile" | "miles" => 1_609_344.0,
        "pt" | "point" | "points" => 25.4 / 72.0,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_units_acrobat_writes() {
        assert_eq!(unit_mm("mm"), Some(1.0));
        assert_eq!(unit_mm(" M "), Some(1000.0));
        assert_eq!(unit_mm("ft"), Some(304.8));
        assert_eq!(unit_mm("尺"), None);
    }
}
