//! 書き出す SXF の版（Ver.2.0・3.0・3.1）。新しい版で加わったフィーチャは、古い版には近い形で書く。
//! Ver.3.0 で図面表題欄（drawing_attribute_feature）が、Ver.3.1 でクロソイド曲線（clothoid_feature）と弧長寸法
//! （curve_dim_feature）が加わった。SFC では、その版で加わったフィーチャを /*SXF3 … SXF3*/・/*SXF3.1 … SXF3.1*/ で囲む。

use crate::document::{SxfDocument, SxfDrawing};
use crate::features::SxfFeature;
use std::borrow::Cow;
use std::collections::HashMap;
use zumen_core::geometry::clothoid_points;
use zumen_core::{NoticeLevel, NoticeList};

/// SXF の版
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum SxfVersion {
    V2_0,
    V3_0,
    #[default]
    V3_1,
}

impl SxfVersion {
    pub const ALL: [SxfVersion; 3] = [SxfVersion::V2_0, SxfVersion::V3_0, SxfVersion::V3_1];

    /// 「2.0」「3.0」「3.1」（「2」「3」、「SXF 3.1」「Ver.3.1」も）
    pub fn parse(text: &str) -> Option<SxfVersion> {
        let text = text.trim();
        let text = text.strip_prefix("SXF").map_or(text, str::trim_start);
        let text = text.strip_prefix("Ver.").or_else(|| text.strip_prefix("ver.")).unwrap_or(text).trim();
        match text {
            "2" | "2.0" => Some(SxfVersion::V2_0),
            "3" | "3.0" => Some(SxfVersion::V3_0),
            "3.1" => Some(SxfVersion::V3_1),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SxfVersion::V2_0 => "2.0",
            SxfVersion::V3_0 => "3.0",
            SxfVersion::V3_1 => "3.1",
        }
    }

    /// 図面の版（SxfDrawing::version の「SXF 3.1」など）。読めなければ None
    pub fn of(drawing: &SxfDrawing) -> Option<SxfVersion> {
        SxfVersion::parse(&drawing.version)
    }

    /// 版を選ばずに新しく書く版。図面の版（分からなければ Ver.3.1）か、図面のフィーチャが要る版の新しい方
    /// （版を選ばない書き出しは、フィーチャを変えない）
    pub(crate) fn written(drawing: &SxfDrawing) -> SxfVersion {
        SxfVersion::of(drawing).unwrap_or_default().max(required(&drawing.document))
    }
}

/// 図面の中身が要る版（クロソイド曲線・弧長寸法は Ver.3.1、図面表題欄は Ver.3.0）
fn required(doc: &SxfDocument) -> SxfVersion {
    if features(doc).any(|feature| matches!(feature, SxfFeature::Clothoid { .. } | SxfFeature::CurveDim(_))) {
        SxfVersion::V3_1
    } else if doc.title.is_some() {
        SxfVersion::V3_0
    } else {
        SxfVersion::V2_0
    }
}

/// 図面のすべての要素（用紙・複合図形・複合曲線の中）
fn features(doc: &SxfDocument) -> impl Iterator<Item = &SxfFeature> {
    doc.sheet_members
        .iter()
        .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
        .chain(doc.composite_curves.iter().flat_map(|curve| curve.members.iter()))
}

/// クロソイド曲線を折線にするときの、曲線からの離れの上限（図形の座標の長さ）
const CLOTHOID_TOLERANCE: f64 = 0.01;

/// 版にないフィーチャを近い形にした図面の中身。Ver.3.1 より前はクロソイド曲線を折線に、弧長寸法を角度寸法にし、
/// Ver.3.0 より前は図面表題欄を除く。変えるものがなければ元のまま
pub(crate) fn downgrade<'a>(
    doc: &'a SxfDocument,
    version: SxfVersion,
    notices: &mut NoticeList,
) -> Cow<'a, SxfDocument> {
    let old_title = version < SxfVersion::V3_0 && doc.title.is_some();
    let newer = version < SxfVersion::V3_1
        && features(doc).any(|feature| matches!(feature, SxfFeature::Clothoid { .. } | SxfFeature::CurveDim(_)));
    if !old_title && !newer {
        return Cow::Borrowed(doc);
    }
    let mut out = doc.clone();
    if old_title {
        out.title = None;
        notices.add(
            NoticeLevel::Warning,
            "drawing-attribute",
            format!("SXF Ver.{} には図面表題欄がないため、図面表題欄を書いていません", version.as_str()),
        );
    }
    if newer {
        let geodetic = geodetic_figures(doc);
        let (mut clothoids, mut curve_dims) = (0u64, 0u64);
        let mut convert = |members: &mut Vec<SxfFeature>, geodetic: bool| {
            for member in members.iter_mut() {
                match member {
                    SxfFeature::Clothoid { style, x, y, parameter, ccw, angle, start_length, end_length } => {
                        let points =
                            clothoid_polyline(*x, *y, *parameter, *ccw, *angle, *start_length, *end_length, geodetic);
                        let (xs, ys) =
                            points.chunks(2).filter(|point| point.len() == 2).map(|point| (point[0], point[1])).unzip();
                        *member = SxfFeature::Polyline { style: style.clone(), xs, ys };
                        clothoids += 1;
                    }
                    SxfFeature::CurveDim(dimension) => {
                        *member = SxfFeature::AngularDim(dimension.clone());
                        curve_dims += 1;
                    }
                    _ => {}
                }
            }
        };
        convert(&mut out.sheet_members, false);
        for curve in &mut out.composite_curves {
            convert(&mut curve.members, false);
        }
        let figures: Vec<_> = out.sub_figures.values().cloned().collect();
        for mut figure in figures {
            let geodetic = geodetic.get(&figure.feature.name).copied().unwrap_or(false);
            convert(&mut figure.members, geodetic);
            out.sub_figures.set(figure.feature.name.clone(), figure);
        }
        if clothoids > 0 {
            notices.add_count(
                NoticeLevel::Info,
                "clothoid",
                format!("SXF Ver.{} にはクロソイド曲線がないため、表示と同じ形の折線にしました", version.as_str()),
                clothoids,
            );
        }
        if curve_dims > 0 {
            notices.add_count(
                NoticeLevel::Info,
                "curve-dim",
                format!("SXF Ver.{} には弧長寸法がないため、同じ形の角度寸法にしました", version.as_str()),
                curve_dims,
            );
        }
    }
    Cow::Owned(out)
}

/// クロソイド曲線の折線（図形の座標）。測地座標系の部分図の中は、表示と同じく X と Y を入れ替えた向きで曲線を
/// 求め、図形の座標に戻す
#[allow(clippy::too_many_arguments)]
fn clothoid_polyline(x: f64, y: f64, a: f64, ccw: bool, angle: f64, s0: f64, s1: f64, geodetic: bool) -> Vec<f64> {
    // 曲率 s/A² の弧を弦で結ぶ離れ（κ h² / 8）が上限に収まる分割数
    let curvature = s0.abs().max(s1.abs()) / (a * a).max(1e-12);
    let step = (8.0 * CLOTHOID_TOLERANCE / curvature.max(1e-12)).sqrt();
    let length = (s1 - s0).abs();
    let segments = (length / step.max(1e-9)).ceil();
    let segments = if segments.is_finite() { segments.clamp(16.0, 2048.0) as usize } else { 16 };
    let (bx, by) = if geodetic { (y, x) } else { (x, y) };
    let mut points = clothoid_points(bx, by, a, ccw, angle, s0, s1, segments);
    if geodetic {
        for pair in points.chunks_mut(2).filter(|pair| pair.len() == 2) {
            pair.swap(0, 1);
        }
    }
    points
}

/// 測地座標系で描く複合図形の名前（表示と同じく、部分図（測地座標系）の中と、そこに配置した作図グループ・
/// 作図部品）。用紙から配置を辿り、最初に辿った向きにする
fn geodetic_figures(doc: &SxfDocument) -> HashMap<String, bool> {
    let mut out: HashMap<String, bool> = HashMap::new();
    let mut queue: Vec<(&SxfFeature, bool)> = doc.sheet_members.iter().map(|member| (member, false)).collect();
    while let Some((member, parent)) = queue.pop() {
        let SxfFeature::SfigLocate { name, .. } = member else {
            continue;
        };
        if out.contains_key(name) {
            continue;
        }
        let Some(figure) = doc.sub_figures.get(name) else {
            continue;
        };
        let geodetic = match figure.feature.flag {
            2.0 => true,
            1.0 => false,
            _ => parent,
        };
        out.insert(name.clone(), geodetic);
        queue.extend(figure.members.iter().map(|member| (member, geodetic)));
    }
    out
}
