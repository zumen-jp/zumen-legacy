//! SXF Ver.3.1 のフィーチャ（34 種、`@zumenjs/sxf` の features.ts）。引数の並びは 附属書 SFC 編 1-2 に従う。
//! P21 からも同じ形に変換して、図面の組み立てと描画を共通にする。
//!
//! 数値は JavaScript と同じく f64 で持つ。整数の引数（レイヤ・色などのコード）は小数部を切り捨てた値。

use crate::sfc_reader::{SfcRecord, SfcValue};
use zumen_core::js;

/// フィーチャの種類（SFC のフィーチャ名から _feature を除いたもの）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FeatureKind {
    DrawingAttribute,
    DrawingSheet,
    Layer,
    PreDefinedFont,
    UserDefinedFont,
    PreDefinedColour,
    UserDefinedColour,
    Width,
    TextFont,
    PointMarker,
    Line,
    Polyline,
    Circle,
    Arc,
    Ellipse,
    EllipseArc,
    TextString,
    Spline,
    Clothoid,
    SfigOrg,
    SfigLocate,
    ExternallyDefinedSymbol,
    LinearDim,
    CurveDim,
    AngularDim,
    RadiusDim,
    DiameterDim,
    Label,
    Balloon,
    ExternallyDefinedHatch,
    FillAreaStyleColour,
    FillAreaStyleHatching,
    FillAreaStyleTiles,
    CompositeCurveOrg,
}

impl FeatureKind {
    pub const ALL: [FeatureKind; 34] = [
        Self::DrawingAttribute,
        Self::DrawingSheet,
        Self::Layer,
        Self::PreDefinedFont,
        Self::UserDefinedFont,
        Self::PreDefinedColour,
        Self::UserDefinedColour,
        Self::Width,
        Self::TextFont,
        Self::PointMarker,
        Self::Line,
        Self::Polyline,
        Self::Circle,
        Self::Arc,
        Self::Ellipse,
        Self::EllipseArc,
        Self::TextString,
        Self::Spline,
        Self::Clothoid,
        Self::SfigOrg,
        Self::SfigLocate,
        Self::ExternallyDefinedSymbol,
        Self::LinearDim,
        Self::CurveDim,
        Self::AngularDim,
        Self::RadiusDim,
        Self::DiameterDim,
        Self::Label,
        Self::Balloon,
        Self::ExternallyDefinedHatch,
        Self::FillAreaStyleColour,
        Self::FillAreaStyleHatching,
        Self::FillAreaStyleTiles,
        Self::CompositeCurveOrg,
    ];

    /// SFC のフィーチャ名から _feature を除いた名前（drawing_sheet など）
    pub fn name(self) -> &'static str {
        match self {
            Self::DrawingAttribute => "drawing_attribute",
            Self::DrawingSheet => "drawing_sheet",
            Self::Layer => "layer",
            Self::PreDefinedFont => "pre_defined_font",
            Self::UserDefinedFont => "user_defined_font",
            Self::PreDefinedColour => "pre_defined_colour",
            Self::UserDefinedColour => "user_defined_colour",
            Self::Width => "width",
            Self::TextFont => "text_font",
            Self::PointMarker => "point_marker",
            Self::Line => "line",
            Self::Polyline => "polyline",
            Self::Circle => "circle",
            Self::Arc => "arc",
            Self::Ellipse => "ellipse",
            Self::EllipseArc => "ellipse_arc",
            Self::TextString => "text_string",
            Self::Spline => "spline",
            Self::Clothoid => "clothoid",
            Self::SfigOrg => "sfig_org",
            Self::SfigLocate => "sfig_locate",
            Self::ExternallyDefinedSymbol => "externally_defined_symbol",
            Self::LinearDim => "linear_dim",
            Self::CurveDim => "curve_dim",
            Self::AngularDim => "angular_dim",
            Self::RadiusDim => "radius_dim",
            Self::DiameterDim => "diameter_dim",
            Self::Label => "label",
            Self::Balloon => "balloon",
            Self::ExternallyDefinedHatch => "externally_defined_hatch",
            Self::FillAreaStyleColour => "fill_area_style_colour",
            Self::FillAreaStyleHatching => "fill_area_style_hatching",
            Self::FillAreaStyleTiles => "fill_area_style_tiles",
            Self::CompositeCurveOrg => "composite_curve_org",
        }
    }

    /// 日本語のフィーチャ名
    pub fn label(self) -> &'static str {
        match self {
            Self::DrawingAttribute => "図面表題欄",
            Self::DrawingSheet => "用紙",
            Self::Layer => "レイヤ",
            Self::PreDefinedFont => "既定義線種",
            Self::UserDefinedFont => "ユーザ定義線種",
            Self::PreDefinedColour => "既定義色",
            Self::UserDefinedColour => "ユーザ定義色",
            Self::Width => "線幅",
            Self::TextFont => "文字フォント",
            Self::PointMarker => "点マーカ",
            Self::Line => "線分",
            Self::Polyline => "折線",
            Self::Circle => "円",
            Self::Arc => "円弧",
            Self::Ellipse => "楕円",
            Self::EllipseArc => "楕円弧",
            Self::TextString => "文字",
            Self::Spline => "スプライン",
            Self::Clothoid => "クロソイド",
            Self::SfigOrg => "複合図形定義",
            Self::SfigLocate => "複合図形配置",
            Self::ExternallyDefinedSymbol => "既定義シンボル",
            Self::LinearDim => "直線寸法",
            Self::CurveDim => "弧長寸法",
            Self::AngularDim => "角度寸法",
            Self::RadiusDim => "半径寸法",
            Self::DiameterDim => "直径寸法",
            Self::Label => "引出し線",
            Self::Balloon => "バルーン",
            Self::ExternallyDefinedHatch => "ハッチング（外部定義）",
            Self::FillAreaStyleColour => "ハッチング（塗り）",
            Self::FillAreaStyleHatching => "ハッチング（ユーザ定義）",
            Self::FillAreaStyleTiles => "ハッチング（パターン）",
            Self::CompositeCurveOrg => "複合曲線定義",
        }
    }

    /// フィーチャ名（小文字）から種類を決める。仕様書の表と見本で名前が違うものもそろえる
    pub fn from_feature_name(name: &str) -> Option<Self> {
        match name {
            "symbol_externally_defined_feature" => return Some(Self::ExternallyDefinedSymbol),
            "fill_area_style_tiles_hatching_feature" => return Some(Self::FillAreaStyleTiles),
            "composite_curve_feature" => return Some(Self::CompositeCurveOrg),
            _ => {}
        }
        let kind = name.strip_suffix("_feature")?;
        Self::ALL.into_iter().find(|candidate| candidate.name() == kind)
    }
}

/// 線の見え方を持つ要素の共通の値
#[derive(Debug, Clone, PartialEq)]
pub struct Styled {
    pub id: f64,
    pub layer: f64,
    pub color: f64,
    pub line_type: f64,
    pub line_width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionLine {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub code: f64,
    /// 1: 外向き、2: 内向き
    pub direction: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
}

/// 文字要素と、寸法値・引出し線の文字
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub font: f64,
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub height: f64,
    pub width: f64,
    pub spacing: f64,
    pub angle: f64,
    pub slant: f64,
    pub anchor: f64,
    /// 1: 横書き、2: 縦書き
    pub direction: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HatchPattern {
    pub color: f64,
    pub line_type: f64,
    pub line_width: f64,
    pub x: f64,
    pub y: f64,
    pub spacing: f64,
    pub angle: f64,
}

/// ハッチングの領域。外形と中抜きの複合曲線の番号（出現順、1 から）
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub out_id: f64,
    pub in_ids: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TitleValue {
    /// 形式によらないキー（docs/architecture.md の「Stable keys」）
    pub key: &'static str,
    pub label: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawingAttribute {
    pub id: f64,
    pub values: Vec<TitleValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawingSheet {
    pub id: f64,
    pub name: String,
    /// 用紙サイズ種別（0〜4: A0〜A4、9: 自由）
    pub sheet_type: f64,
    /// 0: 縦、1: 横
    pub orient: f64,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LayerDefinition {
    pub id: f64,
    pub name: String,
    pub visible: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserColour {
    pub id: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserLineType {
    pub id: f64,
    pub name: String,
    pub pitch: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WidthDefinition {
    pub id: f64,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextFont {
    pub id: f64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SfigOrg {
    pub id: f64,
    pub name: String,
    /// 1: 部分図（数学座標系）、2: 部分図（測地座標系）、3: 作図グループ、4: 作図部品
    pub flag: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeCurveOrg {
    pub id: f64,
    pub color: f64,
    pub line_type: f64,
    pub line_width: f64,
    pub visible: bool,
}

/// 弧長寸法・角度寸法
#[derive(Debug, Clone, PartialEq)]
pub struct ArcDimension {
    pub style: Styled,
    pub cx: f64,
    pub cy: f64,
    pub radius: f64,
    pub angle0: f64,
    pub angle1: f64,
    pub ext1: Option<ExtensionLine>,
    pub ext2: Option<ExtensionLine>,
    pub arrow1: Option<Arrow>,
    pub arrow2: Option<Arrow>,
    pub text: Option<Annotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SxfFeature {
    DrawingAttribute(DrawingAttribute),
    DrawingSheet(DrawingSheet),
    Layer(LayerDefinition),
    PreDefinedFont {
        id: f64,
        name: String,
    },
    UserDefinedFont(UserLineType),
    PreDefinedColour {
        id: f64,
        name: String,
    },
    UserDefinedColour(UserColour),
    Width(WidthDefinition),
    TextFont(TextFont),
    PointMarker {
        id: f64,
        layer: f64,
        color: f64,
        x: f64,
        y: f64,
        code: f64,
        angle: f64,
        scale: f64,
    },
    Line {
        style: Styled,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    },
    Polyline {
        style: Styled,
        xs: Vec<f64>,
        ys: Vec<f64>,
    },
    Circle {
        style: Styled,
        cx: f64,
        cy: f64,
        r: f64,
    },
    Arc {
        style: Styled,
        cx: f64,
        cy: f64,
        r: f64,
        ccw: bool,
        start: f64,
        end: f64,
    },
    Ellipse {
        style: Styled,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        rotation: f64,
    },
    EllipseArc {
        style: Styled,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        ccw: bool,
        rotation: f64,
        start: f64,
        end: f64,
    },
    TextString {
        id: f64,
        layer: f64,
        color: f64,
        text: Annotation,
    },
    Spline {
        style: Styled,
        closed: bool,
        xs: Vec<f64>,
        ys: Vec<f64>,
    },
    Clothoid {
        style: Styled,
        x: f64,
        y: f64,
        parameter: f64,
        ccw: bool,
        angle: f64,
        start_length: f64,
        end_length: f64,
    },
    SfigOrg(SfigOrg),
    SfigLocate {
        id: f64,
        layer: f64,
        name: String,
        x: f64,
        y: f64,
        angle: f64,
        ratio_x: f64,
        ratio_y: f64,
    },
    ExternallyDefinedSymbol {
        id: f64,
        layer: f64,
        color: Option<f64>,
        name: String,
        x: f64,
        y: f64,
        angle: f64,
        scale: f64,
    },
    LinearDim {
        style: Styled,
        line: [f64; 4],
        ext1: Option<ExtensionLine>,
        ext2: Option<ExtensionLine>,
        arrow1: Option<Arrow>,
        arrow2: Option<Arrow>,
        text: Option<Annotation>,
    },
    CurveDim(ArcDimension),
    AngularDim(ArcDimension),
    RadiusDim {
        style: Styled,
        line: [f64; 4],
        arrow: Option<Arrow>,
        text: Option<Annotation>,
    },
    DiameterDim {
        style: Styled,
        line: [f64; 4],
        arrow1: Option<Arrow>,
        arrow2: Option<Arrow>,
        text: Option<Annotation>,
    },
    Label {
        style: Styled,
        xs: Vec<f64>,
        ys: Vec<f64>,
        arrow_code: f64,
        arrow_scale: f64,
        text: Option<Annotation>,
    },
    Balloon {
        style: Styled,
        xs: Vec<f64>,
        ys: Vec<f64>,
        cx: f64,
        cy: f64,
        radius: f64,
        arrow_code: f64,
        arrow_scale: f64,
        text: Option<Annotation>,
    },
    ExternallyDefinedHatch {
        id: f64,
        layer: f64,
        name: String,
        region: Region,
    },
    FillAreaStyleColour {
        id: f64,
        layer: f64,
        color: f64,
        region: Region,
    },
    FillAreaStyleHatching {
        id: f64,
        layer: f64,
        patterns: Vec<HatchPattern>,
        region: Region,
    },
    FillAreaStyleTiles {
        id: f64,
        layer: f64,
        name: String,
        color: f64,
        x: f64,
        y: f64,
        vector1: f64,
        vector1_angle: f64,
        vector2: f64,
        vector2_angle: f64,
        scale_x: f64,
        scale_y: f64,
        angle: f64,
        region: Region,
    },
    CompositeCurveOrg(CompositeCurveOrg),
}

impl SxfFeature {
    pub fn kind(&self) -> FeatureKind {
        match self {
            Self::DrawingAttribute(_) => FeatureKind::DrawingAttribute,
            Self::DrawingSheet(_) => FeatureKind::DrawingSheet,
            Self::Layer(_) => FeatureKind::Layer,
            Self::PreDefinedFont { .. } => FeatureKind::PreDefinedFont,
            Self::UserDefinedFont(_) => FeatureKind::UserDefinedFont,
            Self::PreDefinedColour { .. } => FeatureKind::PreDefinedColour,
            Self::UserDefinedColour(_) => FeatureKind::UserDefinedColour,
            Self::Width(_) => FeatureKind::Width,
            Self::TextFont(_) => FeatureKind::TextFont,
            Self::PointMarker { .. } => FeatureKind::PointMarker,
            Self::Line { .. } => FeatureKind::Line,
            Self::Polyline { .. } => FeatureKind::Polyline,
            Self::Circle { .. } => FeatureKind::Circle,
            Self::Arc { .. } => FeatureKind::Arc,
            Self::Ellipse { .. } => FeatureKind::Ellipse,
            Self::EllipseArc { .. } => FeatureKind::EllipseArc,
            Self::TextString { .. } => FeatureKind::TextString,
            Self::Spline { .. } => FeatureKind::Spline,
            Self::Clothoid { .. } => FeatureKind::Clothoid,
            Self::SfigOrg(_) => FeatureKind::SfigOrg,
            Self::SfigLocate { .. } => FeatureKind::SfigLocate,
            Self::ExternallyDefinedSymbol { .. } => FeatureKind::ExternallyDefinedSymbol,
            Self::LinearDim { .. } => FeatureKind::LinearDim,
            Self::CurveDim(_) => FeatureKind::CurveDim,
            Self::AngularDim(_) => FeatureKind::AngularDim,
            Self::RadiusDim { .. } => FeatureKind::RadiusDim,
            Self::DiameterDim { .. } => FeatureKind::DiameterDim,
            Self::Label { .. } => FeatureKind::Label,
            Self::Balloon { .. } => FeatureKind::Balloon,
            Self::ExternallyDefinedHatch { .. } => FeatureKind::ExternallyDefinedHatch,
            Self::FillAreaStyleColour { .. } => FeatureKind::FillAreaStyleColour,
            Self::FillAreaStyleHatching { .. } => FeatureKind::FillAreaStyleHatching,
            Self::FillAreaStyleTiles { .. } => FeatureKind::FillAreaStyleTiles,
            Self::CompositeCurveOrg(_) => FeatureKind::CompositeCurveOrg,
        }
    }

    /// インスタンス番号
    pub fn id(&self) -> f64 {
        match self {
            Self::DrawingAttribute(feature) => feature.id,
            Self::DrawingSheet(feature) => feature.id,
            Self::Layer(feature) => feature.id,
            Self::UserDefinedFont(feature) => feature.id,
            Self::UserDefinedColour(feature) => feature.id,
            Self::Width(feature) => feature.id,
            Self::TextFont(feature) => feature.id,
            Self::SfigOrg(feature) => feature.id,
            Self::CompositeCurveOrg(feature) => feature.id,
            Self::CurveDim(dimension) | Self::AngularDim(dimension) => dimension.style.id,
            Self::PreDefinedFont { id, .. }
            | Self::PreDefinedColour { id, .. }
            | Self::PointMarker { id, .. }
            | Self::TextString { id, .. }
            | Self::SfigLocate { id, .. }
            | Self::ExternallyDefinedSymbol { id, .. }
            | Self::ExternallyDefinedHatch { id, .. }
            | Self::FillAreaStyleColour { id, .. }
            | Self::FillAreaStyleHatching { id, .. }
            | Self::FillAreaStyleTiles { id, .. } => *id,
            Self::Line { style, .. }
            | Self::Polyline { style, .. }
            | Self::Circle { style, .. }
            | Self::Arc { style, .. }
            | Self::Ellipse { style, .. }
            | Self::EllipseArc { style, .. }
            | Self::Spline { style, .. }
            | Self::Clothoid { style, .. }
            | Self::LinearDim { style, .. }
            | Self::RadiusDim { style, .. }
            | Self::DiameterDim { style, .. }
            | Self::Label { style, .. }
            | Self::Balloon { style, .. } => style.id,
        }
    }
}

/// 足りない・数値でない引数
pub(crate) struct InvalidFeature;

/// `String(value)`。集合は `Array.prototype.join(",")` と同じく要素を , でつなぐ
fn join(value: &SfcValue) -> String {
    match value {
        SfcValue::Text(text) => text.clone(),
        SfcValue::List(items) => items.iter().map(join).collect::<Vec<_>>().join(","),
    }
}

fn split_collection(value: &str) -> Vec<String> {
    let trimmed = js::trim(value);
    let trimmed = trimmed.strip_prefix('(').unwrap_or(trimmed);
    let trimmed = js::trim(trimmed.strip_suffix(')').unwrap_or(trimmed));
    if trimmed.is_empty() { Vec::new() } else { trimmed.split(',').map(|item| js::trim(item).to_string()).collect() }
}

/// レコードの引数を型に合わせて取り出す
struct Args<'a> {
    values: &'a [SfcValue],
}

impl Args<'_> {
    fn raw(&self, index: usize) -> Result<&SfcValue, InvalidFeature> {
        self.values.get(index).ok_or(InvalidFeature)
    }

    fn str(&self, index: usize) -> Result<String, InvalidFeature> {
        self.raw(index).map(join)
    }

    fn num(&self, index: usize) -> Result<f64, InvalidFeature> {
        let value = js::parse_float(&self.str(index)?);
        if value.is_finite() { Ok(value) } else { Err(InvalidFeature) }
    }

    fn int(&self, index: usize) -> Result<f64, InvalidFeature> {
        Ok(self.num(index)?.trunc())
    }

    /// 省略可能な整数（読めなければ fallback）
    fn int_or(&self, index: usize, fallback: f64) -> f64 {
        let value = js::parse_float(&self.values.get(index).map(join).unwrap_or_default());
        if value.is_finite() { value.trunc() } else { fallback }
    }

    fn nums(&self, index: usize) -> Result<Vec<f64>, InvalidFeature> {
        let items = match self.raw(index)? {
            SfcValue::Text(text) => split_collection(text),
            SfcValue::List(items) => items.iter().map(join).collect(),
        };
        items
            .iter()
            .map(|item| {
                let value = js::parse_float(item);
                if value.is_finite() { Ok(value) } else { Err(InvalidFeature) }
            })
            .collect()
    }
}

fn styled(args: &Args<'_>, id: f64) -> Result<Styled, InvalidFeature> {
    Ok(Styled { id, layer: args.int(0)?, color: args.int(1)?, line_type: args.int(2)?, line_width: args.int(3)? })
}

fn extension(args: &Args<'_>, at: usize) -> Result<Option<ExtensionLine>, InvalidFeature> {
    if args.int(at)? == 0.0 {
        return Ok(None);
    }
    Ok(Some(ExtensionLine {
        x0: args.num(at + 1)?,
        y0: args.num(at + 2)?,
        x1: args.num(at + 3)?,
        y1: args.num(at + 4)?,
        x2: args.num(at + 5)?,
        y2: args.num(at + 6)?,
    }))
}

fn arrow(args: &Args<'_>, at: usize) -> Result<Option<Arrow>, InvalidFeature> {
    let direction = args.int(at + 1)?;
    if direction == 0.0 {
        return Ok(None);
    }
    Ok(Some(Arrow {
        code: args.int(at)?,
        direction,
        x: args.num(at + 2)?,
        y: args.num(at + 3)?,
        scale: args.num(at + 4)?,
    }))
}

/// 寸法値・引出し線の文字。at は有無フラグの位置
fn annotation(args: &Args<'_>, at: usize) -> Result<Option<Annotation>, InvalidFeature> {
    if args.int(at)? == 0.0 {
        return Ok(None);
    }
    Ok(Some(Annotation {
        font: args.int_or(at + 1, 0.0),
        text: args.str(at + 2)?,
        x: args.num(at + 3)?,
        y: args.num(at + 4)?,
        height: args.num(at + 5)?,
        width: args.num(at + 6)?,
        spacing: args.num(at + 7)?,
        angle: args.num(at + 8)?,
        slant: args.num(at + 9)?,
        anchor: args.int(at + 10)?,
        direction: args.int_or(at + 11, 1.0),
    }))
}

fn region(args: &Args<'_>, at: usize) -> Result<Region, InvalidFeature> {
    Ok(Region { out_id: args.int(at)?, in_ids: args.nums(at + 2)?.into_iter().map(f64::trunc).collect() })
}

/// 図面表題欄の項目の、形式によらないキーと名前（docs/architecture.md の「Stable keys」）
const TITLE_LABELS: [(&str, &str); 12] = [
    ("businessName", "事業名"),
    ("constructionName", "工事名"),
    ("contractType", "契約区分"),
    ("drawingName", "図面名"),
    ("drawingNumber", "図面番号"),
    ("drawingType", "図面種別"),
    ("scale", "尺度"),
    ("year", "作成年"),
    ("month", "作成月"),
    ("day", "作成日"),
    ("contractor", "受注会社名"),
    ("client", "発注事業者名"),
];

/// SFC のレコードをフィーチャにする。未対応の名前は Ok(None)、引数の不備は Err
pub(crate) fn feature_from_record(record: &SfcRecord) -> Result<Option<SxfFeature>, InvalidFeature> {
    let Some(kind) = FeatureKind::from_feature_name(&record.name) else {
        return Ok(None);
    };
    convert(kind, &Args { values: &record.params }, record.id).map(Some)
}

fn convert(kind: FeatureKind, a: &Args<'_>, id: f64) -> Result<SxfFeature, InvalidFeature> {
    Ok(match kind {
        FeatureKind::DrawingAttribute => SxfFeature::DrawingAttribute(DrawingAttribute {
            id,
            values: TITLE_LABELS
                .iter()
                .enumerate()
                .map(|(index, &(key, label))| {
                    let value =
                        if index < a.values.len() { js::trim(&a.str(index)?).to_string() } else { String::new() };
                    Ok(TitleValue { key, label, value })
                })
                .collect::<Result<_, _>>()?,
        }),
        FeatureKind::DrawingSheet => SxfFeature::DrawingSheet(DrawingSheet {
            id,
            name: a.str(0)?,
            sheet_type: a.int(1)?,
            orient: a.int_or(2, 1.0),
            x: a.num(3)?,
            y: a.num(4)?,
        }),
        FeatureKind::Layer => {
            SxfFeature::Layer(LayerDefinition { id, name: a.str(0)?, visible: a.int_or(1, 1.0) != 0.0 })
        }
        FeatureKind::PreDefinedFont => SxfFeature::PreDefinedFont { id, name: a.str(0)? },
        FeatureKind::PreDefinedColour => SxfFeature::PreDefinedColour { id, name: a.str(0)? },
        FeatureKind::TextFont => SxfFeature::TextFont(TextFont { id, name: a.str(0)? }),
        FeatureKind::UserDefinedFont => {
            SxfFeature::UserDefinedFont(UserLineType { id, name: a.str(0)?, pitch: a.nums(2)? })
        }
        FeatureKind::UserDefinedColour => {
            SxfFeature::UserDefinedColour(UserColour { id, r: a.int(0)?, g: a.int(1)?, b: a.int(2)? })
        }
        FeatureKind::Width => SxfFeature::Width(WidthDefinition { id, width: a.num(0)? }),
        FeatureKind::PointMarker => SxfFeature::PointMarker {
            id,
            layer: a.int(0)?,
            color: a.int(1)?,
            x: a.num(2)?,
            y: a.num(3)?,
            code: a.int(4)?,
            angle: a.num(5)?,
            scale: a.num(6)?,
        },
        FeatureKind::Line => {
            SxfFeature::Line { style: styled(a, id)?, x1: a.num(4)?, y1: a.num(5)?, x2: a.num(6)?, y2: a.num(7)? }
        }
        FeatureKind::Polyline => SxfFeature::Polyline { style: styled(a, id)?, xs: a.nums(5)?, ys: a.nums(6)? },
        FeatureKind::Circle => SxfFeature::Circle { style: styled(a, id)?, cx: a.num(4)?, cy: a.num(5)?, r: a.num(6)? },
        FeatureKind::Arc => SxfFeature::Arc {
            style: styled(a, id)?,
            cx: a.num(4)?,
            cy: a.num(5)?,
            r: a.num(6)?,
            ccw: a.int(7)? == 0.0,
            start: a.num(8)?,
            end: a.num(9)?,
        },
        FeatureKind::Ellipse => SxfFeature::Ellipse {
            style: styled(a, id)?,
            cx: a.num(4)?,
            cy: a.num(5)?,
            rx: a.num(6)?,
            ry: a.num(7)?,
            rotation: a.num(8)?,
        },
        FeatureKind::EllipseArc => SxfFeature::EllipseArc {
            style: styled(a, id)?,
            cx: a.num(4)?,
            cy: a.num(5)?,
            rx: a.num(6)?,
            ry: a.num(7)?,
            ccw: a.int(8)? == 0.0,
            rotation: a.num(9)?,
            start: a.num(10)?,
            end: a.num(11)?,
        },
        FeatureKind::TextString => SxfFeature::TextString {
            id,
            layer: a.int(0)?,
            color: a.int(1)?,
            text: Annotation {
                font: a.int(2)?,
                text: a.str(3)?,
                x: a.num(4)?,
                y: a.num(5)?,
                height: a.num(6)?,
                width: a.num(7)?,
                spacing: a.num(8)?,
                angle: a.num(9)?,
                slant: a.num(10)?,
                anchor: a.int(11)?,
                direction: a.int_or(12, 1.0),
            },
        },
        FeatureKind::Spline => {
            SxfFeature::Spline { style: styled(a, id)?, closed: a.int(4)? == 0.0, xs: a.nums(6)?, ys: a.nums(7)? }
        }
        FeatureKind::Clothoid => SxfFeature::Clothoid {
            style: styled(a, id)?,
            x: a.num(4)?,
            y: a.num(5)?,
            parameter: a.num(6)?,
            ccw: a.int(7)? == 0.0,
            angle: a.num(8)?,
            start_length: a.num(9)?,
            end_length: a.num(10)?,
        },
        FeatureKind::SfigOrg => SxfFeature::SfigOrg(SfigOrg { id, name: a.str(0)?, flag: a.int(1)? }),
        FeatureKind::SfigLocate => SxfFeature::SfigLocate {
            id,
            layer: a.int(0)?,
            name: a.str(1)?,
            x: a.num(2)?,
            y: a.num(3)?,
            angle: a.num(4)?,
            ratio_x: a.num(5)?,
            ratio_y: a.num(6)?,
        },
        FeatureKind::ExternallyDefinedSymbol => SxfFeature::ExternallyDefinedSymbol {
            id,
            layer: a.int(0)?,
            color: if a.int(1)? == 0.0 { None } else { Some(a.int(2)?) },
            name: a.str(3)?,
            x: a.num(4)?,
            y: a.num(5)?,
            angle: a.num(6)?,
            scale: a.num(7)?,
        },
        FeatureKind::LinearDim => SxfFeature::LinearDim {
            style: styled(a, id)?,
            line: [a.num(4)?, a.num(5)?, a.num(6)?, a.num(7)?],
            ext1: extension(a, 8)?,
            ext2: extension(a, 15)?,
            arrow1: arrow(a, 22)?,
            arrow2: arrow(a, 27)?,
            text: annotation(a, 32)?,
        },
        FeatureKind::CurveDim | FeatureKind::AngularDim => {
            let dimension = ArcDimension {
                style: styled(a, id)?,
                cx: a.num(4)?,
                cy: a.num(5)?,
                radius: a.num(6)?,
                angle0: a.num(7)?,
                angle1: a.num(8)?,
                ext1: extension(a, 9)?,
                ext2: extension(a, 16)?,
                arrow1: arrow(a, 23)?,
                arrow2: arrow(a, 28)?,
                text: annotation(a, 33)?,
            };
            if kind == FeatureKind::CurveDim {
                SxfFeature::CurveDim(dimension)
            } else {
                SxfFeature::AngularDim(dimension)
            }
        }
        FeatureKind::RadiusDim => SxfFeature::RadiusDim {
            style: styled(a, id)?,
            line: [a.num(4)?, a.num(5)?, a.num(6)?, a.num(7)?],
            arrow: arrow(a, 8)?,
            text: annotation(a, 13)?,
        },
        FeatureKind::DiameterDim => SxfFeature::DiameterDim {
            style: styled(a, id)?,
            line: [a.num(4)?, a.num(5)?, a.num(6)?, a.num(7)?],
            arrow1: arrow(a, 8)?,
            arrow2: arrow(a, 13)?,
            text: annotation(a, 18)?,
        },
        FeatureKind::Label => SxfFeature::Label {
            style: styled(a, id)?,
            xs: a.nums(5)?,
            ys: a.nums(6)?,
            arrow_code: a.int(7)?,
            arrow_scale: a.num(8)?,
            text: annotation(a, 9)?,
        },
        FeatureKind::Balloon => SxfFeature::Balloon {
            style: styled(a, id)?,
            xs: a.nums(5)?,
            ys: a.nums(6)?,
            cx: a.num(7)?,
            cy: a.num(8)?,
            radius: a.num(9)?,
            arrow_code: a.int(10)?,
            arrow_scale: a.num(11)?,
            text: annotation(a, 12)?,
        },
        FeatureKind::ExternallyDefinedHatch => {
            SxfFeature::ExternallyDefinedHatch { id, layer: a.int(0)?, name: a.str(1)?, region: region(a, 2)? }
        }
        FeatureKind::FillAreaStyleColour => {
            SxfFeature::FillAreaStyleColour { id, layer: a.int(0)?, color: a.int(1)?, region: region(a, 2)? }
        }
        FeatureKind::FillAreaStyleHatching => {
            let count = a.int(1)?;
            let mut patterns = Vec::new();
            let mut i = 0usize;
            while (i as f64) < count {
                let values = a.nums(2 + i)?;
                let [color, line_type, line_width, x, y, spacing, angle, ..] = values[..] else {
                    return Err(InvalidFeature);
                };
                patterns.push(HatchPattern {
                    color: color.trunc(),
                    line_type: line_type.trunc(),
                    line_width: line_width.trunc(),
                    x,
                    y,
                    spacing,
                    angle,
                });
                i += 1;
            }
            // count が負なら、TypeScript と同じく 2 + count の位置を読む
            let at = 2.0 + count;
            if at < 0.0 {
                return Err(InvalidFeature);
            }
            SxfFeature::FillAreaStyleHatching { id, layer: a.int(0)?, patterns, region: region(a, at as usize)? }
        }
        FeatureKind::FillAreaStyleTiles => SxfFeature::FillAreaStyleTiles {
            id,
            layer: a.int(0)?,
            name: a.str(1)?,
            color: a.int(2)?,
            x: a.num(3)?,
            y: a.num(4)?,
            vector1: a.num(5)?,
            vector1_angle: a.num(6)?,
            vector2: a.num(7)?,
            vector2_angle: a.num(8)?,
            scale_x: a.num(9)?,
            scale_y: a.num(10)?,
            angle: a.num(11)?,
            region: region(a, 12)?,
        },
        FeatureKind::CompositeCurveOrg => SxfFeature::CompositeCurveOrg(CompositeCurveOrg {
            id,
            color: a.int(0)?,
            line_type: a.int(1)?,
            line_width: a.int(2)?,
            visible: a.int_or(3, 1.0) != 0.0,
        }),
    })
}
