//! フィーチャを並び順のとおりに組み立てた SXF の図面（`@zumenjs/sxf` の document.ts）。
//! 複合図形定義・複合曲線定義は、その直前の定義以降に書かれた要素を構成要素にし、
//! 用紙は残りの要素を持つ（附属書 SFC 編 1-3）。コードは出現順に割り当てる。

use crate::features::{
    CompositeCurveOrg, DrawingAttribute, DrawingSheet, FeatureKind, LayerDefinition, SfigOrg, SxfFeature, TextFont,
    UserColour, UserLineType, WidthDefinition,
};
use crate::header::SxfHeader;
use crate::source::SxfSource;
use crate::tables::predefined_width_code;
use std::collections::HashMap;
use zumen_core::{Notice, NoticeLevel, NoticeList};

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCurve {
    pub feature: CompositeCurveOrg,
    pub members: Vec<SxfFeature>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubFigure {
    pub feature: SfigOrg,
    pub members: Vec<SxfFeature>,
}

/// 名前の付いた複合図形定義。JavaScript の Map と同じく、最初に加えた順に並び、同じ名前は値だけを置き換える
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubFigures {
    items: Vec<(String, SubFigure)>,
    index: HashMap<String, usize>,
}

impl SubFigures {
    pub fn get(&self, name: &str) -> Option<&SubFigure> {
        self.index.get(name).map(|index| &self.items[*index].1)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    pub fn set(&mut self, name: String, figure: SubFigure) {
        if let Some(index) = self.index.get(&name) {
            self.items[*index].1 = figure;
        } else {
            self.index.insert(name.clone(), self.items.len());
            self.items.push((name, figure));
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn values(&self) -> impl Iterator<Item = &SubFigure> {
        self.items.iter().map(|(_, figure)| figure)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SxfDocument {
    pub sheet: Option<DrawingSheet>,
    pub title: Option<DrawingAttribute>,
    /// レイヤコード 1 から
    pub layers: Vec<LayerDefinition>,
    /// 色コード 17 から
    pub user_colors: Vec<UserColour>,
    /// 線種コード 17 から
    pub user_line_types: Vec<UserLineType>,
    /// 線幅コード 11 から（既定義の値の線幅は 1〜9 の固定コード）
    pub user_widths: Vec<WidthDefinition>,
    /// 文字フォントコード 1 から
    pub fonts: Vec<TextFont>,
    /// 複合曲線の番号 1 から
    pub composite_curves: Vec<CompositeCurve>,
    pub sub_figures: SubFigures,
    pub sheet_members: Vec<SxfFeature>,
    /// フィーチャの種類ごとの件数（最初に現れた順）
    pub counts: Vec<(FeatureKind, u64)>,
}

impl SxfDocument {
    pub(crate) fn count(&mut self, kind: FeatureKind) {
        if let Some(entry) = self.counts.iter_mut().find(|(candidate, _)| *candidate == kind) {
            entry.1 += 1;
        } else {
            self.counts.push((kind, 1));
        }
    }
}

/// SFC・P21 を読んだ結果。P21 も SFC と同じフィーチャに変換して持つ
#[derive(Debug, Clone, PartialEq)]
pub struct SxfDrawing {
    /// sfc か p21
    pub format: &'static str,
    /// SXF の版（SXF 3.1 など）
    pub version: String,
    pub header: SxfHeader,
    pub document: SxfDocument,
    /// 読み取りで省略・代替した内容
    pub notices: Vec<Notice>,
    /// 読んだファイル。書き出し（`write_sxf`）は、同じ形式なら変えていないフィーチャを元の文字列のまま書く。
    /// 新しく作った図面は None
    pub source: Option<SxfSource>,
}

/// 複合曲線の構成要素にできる曲線（仕様は円弧・楕円弧・折線・スプライン。線分なども受け付ける）
pub(crate) fn is_curve(kind: FeatureKind) -> bool {
    matches!(
        kind,
        FeatureKind::Arc
            | FeatureKind::EllipseArc
            | FeatureKind::Polyline
            | FeatureKind::Spline
            | FeatureKind::Line
            | FeatureKind::Clothoid
    )
}

pub(crate) fn assemble_document(features: Vec<SxfFeature>, notices: &mut NoticeList) -> SxfDocument {
    let mut doc = SxfDocument::default();
    let mut pending: Vec<SxfFeature> = Vec::new();
    let mut after_sheet = 0;
    for feature in features {
        doc.count(feature.kind());
        match feature {
            SxfFeature::Layer(layer) => doc.layers.push(layer),
            SxfFeature::UserDefinedColour(color) => doc.user_colors.push(color),
            SxfFeature::UserDefinedFont(line_type) => doc.user_line_types.push(line_type),
            SxfFeature::Width(width) => {
                if predefined_width_code(width.width) == 0 {
                    doc.user_widths.push(width);
                }
            }
            SxfFeature::TextFont(font) => doc.fonts.push(font),
            SxfFeature::DrawingAttribute(title) => doc.title = Some(title),
            SxfFeature::PreDefinedFont { .. } | SxfFeature::PreDefinedColour { .. } => {}
            SxfFeature::CompositeCurveOrg(curve) => {
                let total = pending.len();
                let members: Vec<SxfFeature> =
                    std::mem::take(&mut pending).into_iter().filter(|member| is_curve(member.kind())).collect();
                let rest = total - members.len();
                if rest > 0 {
                    notices.add_count(
                        NoticeLevel::Warning,
                        "composite-member",
                        "複合曲線に使えない要素を構成要素から除きました",
                        rest as u64,
                    );
                }
                doc.composite_curves.push(CompositeCurve { feature: curve, members });
            }
            SxfFeature::SfigOrg(figure) => {
                if doc.sub_figures.contains(&figure.name) {
                    notices.add(NoticeLevel::Warning, "duplicate-sfig", "同じ名前の複合図形定義は後の定義を使いました");
                }
                let members = std::mem::take(&mut pending);
                doc.sub_figures.set(figure.name.clone(), SubFigure { feature: figure, members });
            }
            SxfFeature::DrawingSheet(sheet) => {
                if doc.sheet.is_some() {
                    notices.add(NoticeLevel::Warning, "duplicate-sheet", "2 つ目以降の用紙を無視しました");
                    continue;
                }
                doc.sheet = Some(sheet);
                doc.sheet_members = std::mem::take(&mut pending);
            }
            feature => {
                if doc.sheet.is_some() {
                    after_sheet += 1;
                }
                pending.push(feature);
            }
        }
    }
    if doc.sheet.is_none() {
        notices.add(NoticeLevel::Warning, "no-sheet", "用紙フィーチャがないため、すべての要素を用紙に置きました");
        doc.sheet_members = pending;
    } else if !pending.is_empty() {
        // 仕様では用紙より後の要素は無効。読み落としを避けるため用紙に加え、件数を知らせる
        doc.sheet_members.extend(pending);
        notices.add_count(
            NoticeLevel::Warning,
            "after-sheet",
            "用紙より後に書かれた要素を用紙に加えました",
            after_sheet,
        );
    }
    doc
}
