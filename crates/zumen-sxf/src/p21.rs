//! SXF の P21（STEP AP202 のサブセット）を、SFC と同じフィーチャに変換する（`@zumenjs/sxf` の p21.ts）。
//! 対応は SXF Ver.3.1 仕様書・同解説 STEP/AP202 サブセット編の 2 章（テンプレート）と 3 章（P21 の見本）に従う。

use crate::document::{CompositeCurve, SubFigure, SxfDocument, SxfDrawing};
use crate::features::{
    Annotation, ArcDimension, Arrow, CompositeCurveOrg, DrawingSheet, ExtensionLine, HatchPattern, LayerDefinition,
    Region, SfigOrg, Styled, SxfFeature, TextFont, UserColour, UserLineType, WidthDefinition,
};
use crate::p21_reader::{P21Entity, P21File, P21Value, read_p21};
use crate::sfc::suffix_version;
use crate::source::SxfSource;
use crate::tables::{ARROW_NAMES, predefined_color_code, predefined_line_type_code, predefined_width_code};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use zumen_core::{NoticeLevel, NoticeList, ReadError, js};

const DEG: f64 = 180.0 / PI;
/// 子図の定義の入れ子の上限（描くのは 32 段まで）。深すぎる定義は読まずに件数を知らせる
const MAX_DEFINITION_DEPTH: usize = 64;
const SUBFIGURE_PREFIXES: [(&str, f64); 4] =
    [("$$SXF_FM_", 1.0), ("$$SXF_FG_", 2.0), ("$$SXF_G_", 3.0), ("$$SXF_P_", 4.0)];

/// P21 の点マーカの名前（ISO の pre_defined_point_marker_symbol）と SXF のコード
fn marker_code(name: &str) -> Option<f64> {
    Some(match name {
        "asterisk" => 1.0,
        "circle" => 2.0,
        "dot" => 3.0,
        "plus" => 4.0,
        "square" => 5.0,
        "triangle" => 6.0,
        "x" => 7.0,
        _ => return None,
    })
}

fn arrow_code(name: &str) -> Option<f64> {
    ARROW_NAMES.iter().position(|candidate| *candidate == name).map(|index| (index + 1) as f64)
}

/// 文字の配置基点（$$SXF_<行> <列>）を SFC の b_pnt にする
fn anchor_row(name: &str) -> Option<f64> {
    Some(match name {
        "baseline" => 0.0,
        "middleline" => 1.0,
        "topline" => 2.0,
        _ => return None,
    })
}

fn anchor_column(name: &str) -> Option<f64> {
    Some(match name {
        "left" => 1.0,
        "centre" | "center" => 2.0,
        "right" => 3.0,
        _ => return None,
    })
}

#[derive(Clone, Copy)]
struct CurveStyle {
    color: f64,
    line_type: f64,
    line_width: f64,
}

const DEFAULT_STYLE: CurveStyle = CurveStyle { color: 8.0, line_type: 1.0, line_width: 1.0 };

fn styled_with(id: f64, layer: f64, style: CurveStyle) -> Styled {
    Styled { id, layer, color: style.color, line_type: style.line_type, line_width: style.line_width }
}

struct Placement2 {
    x: f64,
    y: f64,
    angle: f64,
    dx: f64,
    dy: f64,
}

struct Terminator {
    code: f64,
    x: f64,
    y: f64,
    dx: f64,
    dy: f64,
    scale: f64,
}

#[derive(Default)]
struct Trim {
    parameter: Option<f64>,
    point: Option<(f64, f64)>,
}

/// 文字と色
struct TextValue {
    annotation: Annotation,
    color: f64,
}

fn key(id: f64) -> u64 {
    id.to_bits()
}

fn reference(value: Option<&P21Value>) -> Option<f64> {
    match value {
        Some(P21Value::Ref(id)) => Some(*id),
        _ => None,
    }
}

fn references(value: Option<&P21Value>) -> Vec<f64> {
    match value {
        Some(P21Value::List(items)) => items.iter().filter_map(|item| reference(Some(item))).collect(),
        _ => Vec::new(),
    }
}

fn number(value: Option<&P21Value>) -> f64 {
    match value {
        Some(P21Value::Number(value)) => *value,
        Some(P21Value::List(items)) => number(items.first()),
        Some(P21Value::Typed(_, inner)) => number(Some(inner)),
        _ => f64::NAN,
    }
}

fn text(value: Option<&P21Value>) -> &str {
    match value {
        Some(P21Value::Text(value)) => value,
        Some(P21Value::Typed(_, inner)) => text(Some(inner)),
        _ => "",
    }
}

fn enumeration(value: Option<&P21Value>) -> Option<&str> {
    match value {
        Some(P21Value::Enum(name)) => Some(name),
        _ => None,
    }
}

/// `Math.hypot(x, y) || 1`
fn length_or_one(x: f64, y: f64) -> f64 {
    let length = js::hypot2(x, y);
    if length == 0.0 || length.is_nan() { 1.0 } else { length }
}

/// `value || 1`
fn or_one(value: f64) -> f64 {
    if value == 0.0 || value.is_nan() { 1.0 } else { value }
}

struct Converter<'a> {
    file: &'a P21File,
    notices: NoticeList,
    doc: SxfDocument,
    colors: HashMap<u64, f64>,
    line_types: HashMap<u64, f64>,
    widths: HashMap<String, f64>,
    fonts: HashMap<u64, f64>,
    /// 要素（表記の occurrence）からレイヤコードへ
    layer_of: HashMap<u64, f64>,
    invisible: HashSet<u64>,
    composite_codes: HashMap<u64, f64>,
    /// 複合曲線を描く ANNOTATION_CURVE_OCCURRENCE
    curve_occurrence_of: HashMap<u64, f64>,
    definitions: HashMap<u64, String>,
    depth: usize,
}

impl<'a> Converter<'a> {
    fn get(&self, id: Option<f64>) -> Option<&'a P21Entity> {
        self.file.entities.get(id?)
    }

    fn args(entity: Option<&'a P21Entity>, name: &str) -> Option<&'a [P21Value]> {
        entity?.args(name)
    }

    fn has(entity: Option<&P21Entity>, name: &str) -> bool {
        entity.is_some_and(|entity| entity.has(name))
    }

    fn point(&self, id: Option<f64>) -> (f64, f64) {
        match Self::args(self.get(id), "CARTESIAN_POINT").and_then(|args| args.get(1)) {
            Some(P21Value::List(values)) => (number(values.first()), number(values.get(1))),
            _ => (f64::NAN, f64::NAN),
        }
    }

    fn direction(&self, id: Option<f64>) -> (f64, f64) {
        match Self::args(self.get(id), "DIRECTION").and_then(|args| args.get(1)) {
            Some(P21Value::List(values)) => (number(values.first()), number(values.get(1))),
            _ => (1.0, 0.0),
        }
    }

    /// AXIS2_PLACEMENT_2D の原点と X 軸の向き（度）
    fn placement(&self, id: Option<f64>) -> Placement2 {
        let args = Self::args(self.get(id), "AXIS2_PLACEMENT_2D");
        let (x, y) = self.point(reference(args.and_then(|args| args.get(1))));
        let direction_id = reference(args.and_then(|args| args.get(2)));
        let (dx, dy) = if direction_id.is_none() { (1.0, 0.0) } else { self.direction(direction_id) };
        let length = length_or_one(dx, dy);
        Placement2 { x, y, angle: js::atan2(dy, dx) * DEG, dx: dx / length, dy: dy / length }
    }

    fn sorted_ids(&self, name: &str) -> Vec<f64> {
        let mut ids: Vec<f64> =
            self.file.entities.values().filter(|entity| entity.has(name)).map(|entity| entity.id).collect();
        ids.sort_by(f64::total_cmp);
        ids
    }

    /// 色・線種・線幅・フォント・レイヤのコードを、インスタンス番号の順に割り当てる
    fn prepare_tables(&mut self) {
        for id in self.sorted_ids("COLOUR_RGB") {
            let args = Self::args(self.get(Some(id)), "COLOUR_RGB").unwrap_or(&[]);
            let name = js::trim(text(args.first()));
            let predefined = match name.strip_prefix("$$SXF_") {
                Some(rest) => predefined_color_code(rest),
                None => 0,
            };
            if predefined > 0 {
                self.colors.insert(key(id), predefined as f64);
                continue;
            }
            let channel = |index: usize| js::round(number(args.get(index)) * 255.0);
            self.doc.user_colors.push(UserColour { id, r: channel(1), g: channel(2), b: channel(3) });
            self.colors.insert(key(id), 16.0 + self.doc.user_colors.len() as f64);
        }
        for id in self.sorted_ids("DRAUGHTING_PRE_DEFINED_COLOUR") {
            let name =
                text(Self::args(self.get(Some(id)), "DRAUGHTING_PRE_DEFINED_COLOUR").and_then(|args| args.first()));
            self.colors.insert(key(id), predefined_color_code(name) as f64);
        }
        for id in self.sorted_ids("DRAUGHTING_PRE_DEFINED_CURVE_FONT") {
            let name =
                text(Self::args(self.get(Some(id)), "DRAUGHTING_PRE_DEFINED_CURVE_FONT").and_then(|args| args.first()));
            let code = predefined_line_type_code(name);
            self.line_types.insert(key(id), if code == 0 { 1.0 } else { code as f64 });
        }
        for id in self.sorted_ids("CURVE_STYLE_FONT") {
            let args = Self::args(self.get(Some(id)), "CURVE_STYLE_FONT").unwrap_or(&[]);
            let mut pitch = Vec::new();
            for pattern_id in references(args.get(1)) {
                let pattern = Self::args(self.get(Some(pattern_id)), "CURVE_STYLE_FONT_PATTERN").unwrap_or(&[]);
                pitch.push(number(pattern.first()));
                pitch.push(number(pattern.get(1)));
            }
            self.doc.user_line_types.push(UserLineType { id, name: text(args.first()).to_string(), pitch });
            self.line_types.insert(key(id), 16.0 + self.doc.user_line_types.len() as f64);
        }
        for id in self.sorted_ids("EXTERNALLY_DEFINED_TEXT_FONT") {
            let name =
                text(Self::args(self.get(Some(id)), "EXTERNALLY_DEFINED_TEXT_FONT").and_then(|args| args.first()));
            self.doc.fonts.push(TextFont { id, name: name.to_string() });
            self.fonts.insert(key(id), self.doc.fonts.len() as f64);
        }
        for id in self.sorted_ids("PRESENTATION_LAYER_ASSIGNMENT") {
            let args = Self::args(self.get(Some(id)), "PRESENTATION_LAYER_ASSIGNMENT").unwrap_or(&[]);
            let name = text(args.first());
            // 子図を入れるための仮のレイヤ（$$SXF_dummy_layer_for_subfigure）は、レイヤとして扱わない
            if name.starts_with("$$SXF_dummy") {
                continue;
            }
            self.doc.layers.push(LayerDefinition { id, name: name.to_string(), visible: true });
            let code = self.doc.layers.len() as f64;
            for item in references(args.get(2)) {
                self.layer_of.insert(key(item), code);
            }
        }
        for id in self.sorted_ids("INVISIBILITY") {
            for item in references(Self::args(self.get(Some(id)), "INVISIBILITY").and_then(|args| args.first())) {
                if let Some(layer) = self.doc.layers.iter_mut().find(|candidate| candidate.id == item) {
                    layer.visible = false;
                } else {
                    self.invisible.insert(key(item));
                }
            }
        }
        for entity in self.file.entities.values() {
            if !entity.has("ANNOTATION_CURVE_OCCURRENCE") {
                continue;
            }
            let item = reference(entity.args("STYLED_ITEM").and_then(|args| args.get(1)));
            if item.is_some() && Self::has(self.get(item), "COMPOSITE_CURVE") {
                self.curve_occurrence_of.insert(key(item.unwrap_or_default()), entity.id);
            }
        }
    }

    fn color(&mut self, id: Option<f64>) -> f64 {
        let Some(id) = id else {
            return 0.0;
        };
        if let Some(code) = self.colors.get(&key(id)) {
            return *code;
        }
        self.notices.add(NoticeLevel::Warning, "unknown-color", "定義のない色を白で表示しました");
        8.0
    }

    fn line_type(&self, id: Option<f64>) -> f64 {
        id.map_or(1.0, |id| self.line_types.get(&key(id)).copied().unwrap_or(1.0))
    }

    /// 線幅（LENGTH_MEASURE_WITH_UNIT か数値）を、既定義なら 1〜9、それ以外は 11 からのコードにする
    fn line_width(&mut self, value: Option<&P21Value>) -> f64 {
        let id = reference(value);
        let width = match id {
            None => number(value),
            Some(_) => number(Self::args(self.get(id), "LENGTH_MEASURE_WITH_UNIT").and_then(|args| args.first())),
        };
        if !width.is_finite() {
            return 1.0;
        }
        let predefined = predefined_width_code(width);
        if predefined > 0 {
            return predefined as f64;
        }
        let width_key = js::to_fixed(width, 6);
        if let Some(code) = self.widths.get(&width_key) {
            return *code;
        }
        self.doc.user_widths.push(WidthDefinition { id: id.unwrap_or(0.0), width });
        let code = 10.0 + self.doc.user_widths.len() as f64;
        self.widths.insert(width_key, code);
        code
    }

    fn font(&self, id: Option<f64>) -> f64 {
        id.map_or(0.0, |id| self.fonts.get(&key(id)).copied().unwrap_or(0.0))
    }

    /// PRESENTATION_STYLE_ASSIGNMENT の中の CURVE_STYLE
    fn curve_style(&mut self, assignment: Option<f64>) -> CurveStyle {
        let styles =
            references(Self::args(self.get(assignment), "PRESENTATION_STYLE_ASSIGNMENT").and_then(|args| args.first()));
        for style_id in styles {
            let Some(args) = Self::args(self.get(Some(style_id)), "CURVE_STYLE") else {
                continue;
            };
            let line_type = self.line_type(reference(args.get(1)));
            let line_width = self.line_width(args.get(2));
            let color = self.color(reference(args.get(3)));
            return CurveStyle { color, line_type, line_width };
        }
        DEFAULT_STYLE
    }

    /// STYLED_ITEM の (スタイル, 対象)。複合インスタンスでは name を REPRESENTATION_ITEM が持つ
    fn styled(entity: Option<&P21Entity>) -> (Option<f64>, Option<f64>) {
        let args = entity.and_then(|entity| entity.args("STYLED_ITEM")).unwrap_or(&[]);
        let offset = if args.len() >= 3 { 1 } else { 0 };
        (references(args.get(offset)).first().copied(), reference(args.get(offset + 1)))
    }

    fn composite_code(&mut self, curve_id: f64) -> f64 {
        if let Some(code) = self.composite_codes.get(&key(curve_id)) {
            return *code;
        }
        let occurrence = self.get(self.curve_occurrence_of.get(&key(curve_id)).copied());
        let style = match occurrence {
            Some(occurrence) => self.curve_style(Self::styled(Some(occurrence)).0),
            None => DEFAULT_STYLE,
        };
        let visible = occurrence.is_some_and(|occurrence| !self.invisible.contains(&key(occurrence.id)));
        let mut members = Vec::new();
        let segments = references(Self::args(self.get(Some(curve_id)), "COMPOSITE_CURVE").and_then(|args| args.get(1)));
        for segment_id in segments {
            let segment = Self::args(self.get(Some(segment_id)), "COMPOSITE_CURVE_SEGMENT");
            let parent = reference(segment.and_then(|args| args.get(2)));
            if let Some(feature) = self.curve(parent, styled_with(parent.unwrap_or(0.0), 0.0, style)) {
                members.push(feature);
            }
        }
        self.doc.composite_curves.push(CompositeCurve {
            feature: CompositeCurveOrg {
                id: curve_id,
                color: style.color,
                line_type: style.line_type,
                line_width: style.line_width,
                visible,
            },
            members,
        });
        let code = self.doc.composite_curves.len() as f64;
        self.composite_codes.insert(key(curve_id), code);
        code
    }

    /// 曲線の幾何を、SFC のフィーチャ（線分・円弧・折線など）にする
    fn curve(&self, id: Option<f64>, style: Styled) -> Option<SxfFeature> {
        let entity = self.get(id)?;
        if let Some(trimmed) = entity.args("TRIMMED_CURVE") {
            return self.trimmed(trimmed, style);
        }
        if let Some(polyline) = entity.args("POLYLINE") {
            let points: Vec<(f64, f64)> =
                references(polyline.get(1)).into_iter().map(|point| self.point(Some(point))).collect();
            return Some(SxfFeature::Polyline {
                style,
                xs: points.iter().map(|point| point.0).collect(),
                ys: points.iter().map(|point| point.1).collect(),
            });
        }
        if let Some(circle) = entity.args("CIRCLE") {
            let p = self.placement(reference(circle.get(1)));
            return Some(SxfFeature::Circle { style, cx: p.x, cy: p.y, r: number(circle.get(2)) });
        }
        if let Some(ellipse) = entity.args("ELLIPSE") {
            let p = self.placement(reference(ellipse.get(1)));
            return Some(SxfFeature::Ellipse {
                style,
                cx: p.x,
                cy: p.y,
                rx: number(ellipse.get(2)),
                ry: number(ellipse.get(3)),
                rotation: p.angle,
            });
        }
        if let Some(bezier) = entity.args("BEZIER_CURVE") {
            let points: Vec<(f64, f64)> =
                references(bezier.get(2)).into_iter().map(|point| self.point(Some(point))).collect();
            let closed = enumeration(bezier.get(4)) == Some("T");
            return Some(SxfFeature::Spline {
                style,
                closed,
                xs: points.iter().map(|point| point.0).collect(),
                ys: points.iter().map(|point| point.1).collect(),
            });
        }
        None
    }

    /// トリム値。PARAMETER_VALUE（ラジアン・曲線長）か CARTESIAN_POINT
    fn trim(&self, value: Option<&P21Value>) -> Trim {
        let items: Vec<Option<&P21Value>> = match value {
            Some(P21Value::List(items)) => items.iter().map(Some).collect(),
            other => vec![other],
        };
        let mut trim = Trim::default();
        for item in items {
            if let Some(P21Value::Typed(name, inner)) = item
                && name == "PARAMETER_VALUE"
            {
                trim.parameter = Some(number(Some(inner)));
            } else {
                let id = reference(item);
                if id.is_some() && Self::has(self.get(id), "CARTESIAN_POINT") {
                    trim.point = Some(self.point(id));
                }
            }
        }
        trim
    }

    fn trimmed(&self, args: &[P21Value], style: Styled) -> Option<SxfFeature> {
        let basis = self.get(reference(args.get(1)));
        let t1 = self.trim(args.get(2));
        let t2 = self.trim(args.get(3));
        let sense = enumeration(args.get(4)) != Some("F");
        if let Some(line) = Self::args(basis, "LINE") {
            let origin = self.point(reference(line.get(1)));
            let vector = Self::args(self.get(reference(line.get(2))), "VECTOR");
            let (dx, dy) = self.direction(reference(vector.and_then(|vector| vector.get(1))));
            let length = length_or_one(dx, dy);
            let magnitude = or_one(number(vector.and_then(|vector| vector.get(2))));
            let at = |trim: &Trim| match (trim.point, trim.parameter) {
                (Some(point), _) => point,
                (None, Some(parameter)) => {
                    (origin.0 + (dx / length) * magnitude * parameter, origin.1 + (dy / length) * magnitude * parameter)
                }
                (None, None) => origin,
            };
            let (x1, y1) = at(&t1);
            let (x2, y2) = at(&t2);
            return Some(SxfFeature::Line { style, x1, y1, x2, y2 });
        }
        let circle = Self::args(basis, "CIRCLE");
        let ellipse = Self::args(basis, "ELLIPSE");
        if let Some(conic) = circle.or(ellipse) {
            let p = self.placement(reference(conic.get(1)));
            let angle_of = |trim: &Trim| match (trim.parameter, trim.point) {
                (Some(parameter), _) => parameter * DEG,
                (None, Some(point)) => js::atan2(point.1 - p.y, point.0 - p.x) * DEG - p.angle,
                (None, None) => 0.0,
            };
            let start = angle_of(&t1);
            let end = angle_of(&t2);
            if let Some(circle) = circle {
                return Some(SxfFeature::Arc {
                    style,
                    cx: p.x,
                    cy: p.y,
                    r: number(circle.get(2)),
                    ccw: sense,
                    start: start + p.angle,
                    end: end + p.angle,
                });
            }
            return Some(SxfFeature::EllipseArc {
                style,
                cx: p.x,
                cy: p.y,
                rx: number(ellipse.and_then(|args| args.get(2))),
                ry: number(ellipse.and_then(|args| args.get(3))),
                ccw: sense,
                rotation: p.angle,
                start,
                end,
            });
        }
        if let Some(clothoid) = Self::args(basis, "CLOTHOID") {
            let p = self.placement(reference(clothoid.get(1)));
            return Some(SxfFeature::Clothoid {
                style,
                x: p.x,
                y: p.y,
                parameter: number(clothoid.get(2)),
                // 向きフラグは CLOTHOID の name に書かれる（0: 反時計回り、1: 時計回り）
                ccw: js::trim(text(clothoid.first())) != "1",
                angle: p.angle,
                start_length: t1.parameter.unwrap_or(0.0),
                end_length: t2.parameter.unwrap_or(0.0),
            });
        }
        None
    }

    /// 文字の occurrence を SFC の文字の値にする。配置点はベースラインからの差を戻して、SXF の配置基点にする
    fn annotation_of(&mut self, occurrence: Option<&'a P21Entity>) -> Option<TextValue> {
        let occurrence = occurrence?;
        let (style, item) = Self::styled(Some(occurrence));
        let literal = Self::args(self.get(item), "TEXT_LITERAL_WITH_EXTENT")?;
        let alignment = text(literal.first());
        let alignment = js::trim(alignment.strip_prefix("$$SXF_").unwrap_or(alignment));
        let mut names = alignment.split(js::is_whitespace).filter(|name| !name.is_empty());
        let row = anchor_row(names.next().unwrap_or("")).unwrap_or(0.0);
        let column = anchor_column(names.next().unwrap_or("")).unwrap_or(1.0);
        let placement = self.placement(reference(literal.get(2)));
        let extent = Self::args(self.get(reference(literal.get(6))), "PLANAR_EXTENT");
        let width = number(extent.and_then(|args| args.get(1)));
        let height = number(extent.and_then(|args| args.get(2)));
        let mut color = 8.0;
        let mut slant = 0.0;
        let mut spacing = 0.0;
        let styles =
            references(Self::args(self.get(style), "PRESENTATION_STYLE_ASSIGNMENT").and_then(|args| args.first()));
        for style_id in styles {
            let style_entity = self.get(Some(style_id));
            let text_style = Self::args(style_entity, "TEXT_STYLE");
            let defined_font =
                Self::args(self.get(reference(text_style.and_then(|args| args.get(1)))), "TEXT_STYLE_FOR_DEFINED_FONT");
            if let Some(defined_font) = defined_font {
                color = self.color(reference(defined_font.first()));
            }
            let characteristics =
                Self::args(style_entity, "TEXT_STYLE_WITH_BOX_CHARACTERISTICS").and_then(|args| args.first());
            if let Some(P21Value::List(characteristics)) = characteristics {
                for characteristic in characteristics {
                    if let P21Value::Typed(name, value) = characteristic
                        && name == "BOX_SLANT_ANGLE"
                    {
                        slant = number(Some(value)) * DEG;
                    }
                }
            }
            if let Some(value) = Self::args(style_entity, "TEXT_STYLE_WITH_SPACING").and_then(|args| args.first()) {
                spacing = number(Some(value));
            }
        }
        // 出力ライブラリは配置基点を、下・中・上の行ごとに高さの 1/2・-1/2・-1 だけ文字の上方向へずらして書く
        let offset = if row == 0.0 {
            height / 2.0
        } else if row == 1.0 {
            -height / 2.0
        } else {
            -height
        };
        let x = placement.x + placement.dy * offset;
        let y = placement.y - placement.dx * offset;
        let path = enumeration(literal.get(4));
        Some(TextValue {
            annotation: Annotation {
                font: self.font(reference(literal.get(5))),
                text: text(literal.get(1)).to_string(),
                x,
                y,
                height,
                width,
                spacing,
                angle: placement.angle,
                slant,
                anchor: row * 3.0 + column,
                direction: if path == Some("DOWN") { 2.0 } else { 1.0 },
            },
            color,
        })
    }

    /// 寸法線・引出し線の端末記号（矢印）
    fn terminator_of(&self, occurrence: &P21Entity) -> Option<Terminator> {
        let symbol = Self::args(self.get(Self::styled(Some(occurrence)).1), "DEFINED_SYMBOL")?;
        let name = text(
            Self::args(self.get(reference(symbol.get(1))), "PRE_DEFINED_TERMINATOR_SYMBOL")
                .and_then(|args| args.first()),
        );
        let target = Self::args(self.get(reference(symbol.get(2))), "SYMBOL_TARGET");
        let placement = self.placement(reference(target.and_then(|args| args.get(1))));
        Some(Terminator {
            code: arrow_code(&js::trim(name).to_lowercase()).unwrap_or(9.0),
            x: placement.x,
            y: placement.y,
            dx: placement.dx,
            dy: placement.dy,
            scale: or_one(number(target.and_then(|args| args.get(2)))),
        })
    }

    fn layer(&self, id: f64) -> f64 {
        self.layer_of.get(&key(id)).copied().unwrap_or(0.0)
    }

    /// 用紙・子図の items の 1 件を、フィーチャにする（対象外は空）
    fn item(&mut self, id: f64) -> Vec<SxfFeature> {
        let Some(entity) = self.get(Some(id)) else {
            return Vec::new();
        };
        if entity.has("DRAUGHTING_CALLOUT") {
            return self.callout(entity);
        }
        if entity.has("ANNOTATION_SUBFIGURE_OCCURRENCE") {
            return self.subfigure(entity);
        }
        if entity.has("ANNOTATION_FILL_AREA_OCCURRENCE") {
            return self.fill(entity);
        }
        if entity.has("ANNOTATION_TEXT_OCCURRENCE") {
            return match self.annotation_of(Some(entity)) {
                Some(value) => vec![SxfFeature::TextString {
                    id,
                    layer: self.layer(id),
                    color: value.color,
                    text: value.annotation,
                }],
                None => Vec::new(),
            };
        }
        if entity.has("ANNOTATION_SYMBOL_OCCURRENCE") {
            return self.symbol(entity);
        }
        if entity.has("ANNOTATION_CURVE_OCCURRENCE") {
            let (style, item) = Self::styled(Some(entity));
            if item.is_some() && Self::has(self.get(item), "COMPOSITE_CURVE") {
                return Vec::new();
            }
            let layer = self.layer(id);
            let curve_style = self.curve_style(style);
            let curve = self.curve(item, styled_with(id, layer, curve_style));
            if curve.is_none() {
                self.notices.add(NoticeLevel::Warning, "p21-curve", "対応していない曲線を省きました");
            }
            return curve.into_iter().collect();
        }
        let target = Self::styled(Some(entity)).1;
        if target.is_some()
            && let Some(callout) = self.get(target).filter(|candidate| candidate.has("DRAUGHTING_CALLOUT"))
        {
            return self.callout(callout);
        }
        Vec::new()
    }

    fn symbol(&mut self, entity: &'a P21Entity) -> Vec<SxfFeature> {
        let (style, item) = Self::styled(Some(entity));
        let Some(symbol) = Self::args(self.get(item), "DEFINED_SYMBOL") else {
            return Vec::new();
        };
        let target = Self::args(self.get(reference(symbol.get(2))), "SYMBOL_TARGET");
        let placement = self.placement(reference(target.and_then(|args| args.get(1))));
        let mut color = None;
        let styles =
            references(Self::args(self.get(style), "PRESENTATION_STYLE_ASSIGNMENT").and_then(|args| args.first()));
        for style_id in styles {
            let symbol_style = Self::args(self.get(Some(style_id)), "SYMBOL_STYLE");
            let symbol_colour =
                Self::args(self.get(reference(symbol_style.and_then(|args| args.get(1)))), "SYMBOL_COLOUR");
            if let Some(symbol_colour) = symbol_colour {
                color = Some(self.color(reference(symbol_colour.first())));
            }
        }
        let definition = self.get(reference(symbol.get(1)));
        let scale = or_one(number(target.and_then(|args| args.get(2))));
        if let Some(marker) = Self::args(definition, "PRE_DEFINED_POINT_MARKER_SYMBOL") {
            return vec![SxfFeature::PointMarker {
                id: entity.id,
                layer: self.layer(entity.id),
                color: color.unwrap_or(8.0),
                x: placement.x,
                y: placement.y,
                code: marker_code(&js::trim(text(marker.first())).to_lowercase()).unwrap_or(3.0),
                angle: placement.angle,
                scale,
            }];
        }
        if let Some(external) = Self::args(definition, "EXTERNALLY_DEFINED_SYMBOL") {
            return vec![SxfFeature::ExternallyDefinedSymbol {
                id: entity.id,
                layer: self.layer(entity.id),
                color,
                name: text(external.first()).to_string(),
                x: placement.x,
                y: placement.y,
                angle: placement.angle,
                scale,
            }];
        }
        Vec::new()
    }

    fn subfigure(&mut self, entity: &'a P21Entity) -> Vec<SxfFeature> {
        let mapped = Self::args(self.get(Self::styled(Some(entity)).1), "MAPPED_ITEM");
        let map = Self::args(self.get(reference(mapped.and_then(|args| args.first()))), "SYMBOL_REPRESENTATION_MAP");
        let representation = reference(map.and_then(|args| args.get(1)));
        let target = Self::args(self.get(reference(mapped.and_then(|args| args.get(1)))), "SYMBOL_TARGET");
        let (Some(representation), Some(target)) = (representation, target) else {
            return Vec::new();
        };
        let Some(name) = self.definition(representation).filter(|name| !name.is_empty()) else {
            return Vec::new();
        };
        let placement = self.placement(reference(target.get(1)));
        vec![SxfFeature::SfigLocate {
            id: entity.id,
            layer: self.layer(entity.id),
            name,
            x: placement.x,
            y: placement.y,
            angle: placement.angle,
            ratio_x: number(target.get(2)),
            ratio_y: number(target.get(3)),
        }]
    }

    /// DRAUGHTING_SUBFIGURE_REPRESENTATION を複合図形定義にして、名前を返す
    fn definition(&mut self, id: f64) -> Option<String> {
        if let Some(known) = self.definitions.get(&key(id)) {
            return Some(known.clone());
        }
        let args = Self::args(self.get(Some(id)), "DRAUGHTING_SUBFIGURE_REPRESENTATION")?;
        if self.depth >= MAX_DEFINITION_DEPTH {
            self.notices.add(NoticeLevel::Warning, "sfig-depth", "入れ子が深すぎる子図の定義を省きました");
            return None;
        }
        let full_name = text(args.first());
        let prefix = SUBFIGURE_PREFIXES.iter().find(|(candidate, _)| full_name.starts_with(candidate));
        let name = match prefix {
            Some((candidate, _)) => full_name[candidate.len()..].to_string(),
            None => full_name.to_string(),
        };
        self.definitions.insert(key(id), name.clone());
        let mut members = Vec::new();
        self.depth += 1;
        for item in references(args.get(1)) {
            members.extend(self.item(item));
        }
        self.depth -= 1;
        self.doc.sub_figures.set(
            name.clone(),
            SubFigure {
                feature: SfigOrg { id, name: name.clone(), flag: prefix.map_or(3.0, |(_, flag)| *flag) },
                members,
            },
        );
        Some(name)
    }

    fn fill(&mut self, entity: &'a P21Entity) -> Vec<SxfFeature> {
        let (style, item) = Self::styled(Some(entity));
        let area = Self::args(self.get(item), "ANNOTATION_FILL_AREA");
        let boundaries: Vec<f64> = references(area.and_then(|args| args.get(1)))
            .into_iter()
            .filter(|id| Self::has(self.get(Some(*id)), "COMPOSITE_CURVE"))
            .collect();
        if boundaries.is_empty() {
            self.notices.add(NoticeLevel::Warning, "hatch-boundary", "外形の複合曲線がないハッチングを省きました");
            return Vec::new();
        }
        let codes: Vec<f64> = boundaries.into_iter().map(|id| self.composite_code(id)).collect();
        let region = || Region { out_id: codes[0], in_ids: codes[1..].to_vec() };
        let layer = self.layer(entity.id);
        let mut patterns = Vec::new();
        let styles =
            references(Self::args(self.get(style), "PRESENTATION_STYLE_ASSIGNMENT").and_then(|args| args.first()));
        for style_id in styles {
            let fill_styles =
                references(Self::args(self.get(Some(style_id)), "FILL_AREA_STYLE").and_then(|args| args.get(1)));
            for fill_style_id in fill_styles {
                let fill_style = self.get(Some(fill_style_id));
                if let Some(colour) = Self::args(fill_style, "FILL_AREA_STYLE_COLOUR") {
                    let color = self.color(reference(colour.get(1)));
                    return vec![SxfFeature::FillAreaStyleColour { id: entity.id, layer, color, region: region() }];
                }
                if let Some(hatching) = Self::args(fill_style, "FILL_AREA_STYLE_HATCHING") {
                    let curve_style = Self::args(self.get(reference(hatching.get(1))), "CURVE_STYLE");
                    let repeat = Self::args(self.get(reference(hatching.get(2))), "ONE_DIRECTION_REPEAT_FACTOR");
                    let vector = Self::args(self.get(reference(repeat.and_then(|args| args.get(1)))), "VECTOR");
                    let (x, y) = self.point(reference(hatching.get(3)));
                    let color = self.color(reference(curve_style.and_then(|args| args.get(3))));
                    let line_type = self.line_type(reference(curve_style.and_then(|args| args.get(1))));
                    let line_width = self.line_width(curve_style.and_then(|args| args.get(2)));
                    patterns.push(HatchPattern {
                        color,
                        line_type,
                        line_width,
                        x,
                        y,
                        spacing: number(vector.and_then(|args| args.get(2))),
                        angle: number(hatching.get(5)) * DEG,
                    });
                    continue;
                }
                if let Some(external) = Self::args(fill_style, "EXTERNALLY_DEFINED_HATCH_STYLE") {
                    return vec![SxfFeature::ExternallyDefinedHatch {
                        id: entity.id,
                        layer,
                        name: text(external.first()).to_string(),
                        region: region(),
                    }];
                }
                if Self::has(fill_style, "FILL_AREA_STYLE_TILES") {
                    return vec![SxfFeature::FillAreaStyleTiles {
                        id: entity.id,
                        layer,
                        name: String::new(),
                        color: 8.0,
                        x: 0.0,
                        y: 0.0,
                        vector1: 0.0,
                        vector1_angle: 0.0,
                        vector2: 0.0,
                        vector2_angle: 0.0,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        angle: 0.0,
                        region: region(),
                    }];
                }
            }
        }
        if patterns.is_empty() {
            return Vec::new();
        }
        vec![SxfFeature::FillAreaStyleHatching { id: entity.id, layer, patterns, region: region() }]
    }

    fn callout(&mut self, entity: &'a P21Entity) -> Vec<SxfFeature> {
        if entity.has("STRUCTURED_DIMENSION_CALLOUT") {
            return Vec::new();
        }
        let contents: Vec<Option<&'a P21Entity>> =
            references(entity.args("DRAUGHTING_CALLOUT").and_then(|args| args.first()))
                .into_iter()
                .map(|id| self.get(Some(id)))
                .collect();
        let find = |name: &str| contents.iter().copied().find(|item| Self::has(*item, name)).flatten();
        let text_occurrence = find("ANNOTATION_TEXT_OCCURRENCE");
        let annotation = self.annotation_of(text_occurrence).map(|value| value.annotation);
        if entity.has("LEADER_DIRECTED_CALLOUT") {
            return self.leader(entity, &contents, annotation);
        }
        let Some(dimension_curve) = find("DIMENSION_CURVE") else {
            return Vec::new();
        };
        let (style, item) = Self::styled(Some(dimension_curve));
        let curve_style = self.curve_style(style);
        let styled = styled_with(entity.id, self.layer(dimension_curve.id), curve_style);
        let curve = self.curve(item, styled.clone());
        // 寸法補助線は名前（$$SXF_prj_1・$$SXF_prj_2）の順。利用者のロケールで結果が変わらないよう、符号単位で比べる
        let mut projections: Vec<&'a P21Entity> =
            contents.iter().copied().flatten().filter(|item| item.has("PROJECTION_CURVE")).collect();
        fn projection_name(item: &P21Entity) -> &str {
            text(item.args("REPRESENTATION_ITEM").and_then(|args| args.first()))
        }
        projections.sort_by(|a, b| projection_name(a).encode_utf16().cmp(projection_name(b).encode_utf16()));
        let extension_style = styled.clone();
        let extension = |occurrence: Option<&&'a P21Entity>| -> Option<ExtensionLine> {
            let occurrence = occurrence?;
            let target = Self::styled(Some(occurrence)).1;
            let trimmed = Self::args(self.get(target), "TRIMMED_CURVE");
            let line = Self::args(self.get(reference(trimmed.and_then(|args| args.get(1)))), "LINE");
            let Some(SxfFeature::Line { x1, y1, x2, y2, .. }) = self.curve(target, extension_style.clone()) else {
                return None;
            };
            let (x0, y0) = self.point(reference(line.and_then(|args| args.get(1))));
            Some(ExtensionLine { x0, y0, x1, y1, x2, y2 })
        };
        let terminators: Vec<(Option<&str>, Option<Terminator>)> = contents
            .iter()
            .copied()
            .flatten()
            .filter(|item| item.has("DIMENSION_CURVE_TERMINATOR"))
            .map(|item| {
                (
                    enumeration(item.args("DIMENSION_CURVE_TERMINATOR").and_then(|args| args.first())),
                    self.terminator_of(item),
                )
            })
            .collect();
        let ends = match &curve {
            Some(SxfFeature::Line { x1, y1, x2, y2, .. }) => Some([*x1, *y1, *x2, *y2]),
            _ => None,
        };
        // P21 の矢印の向き（X 軸が矢印の指す向き）を、SFC の外向き・内向きにする
        let arrow = |role: &str| -> Option<Arrow> {
            let symbol = terminators.iter().find(|(candidate, _)| *candidate == Some(role))?.1.as_ref()?;
            let mut outward = (symbol.dx, symbol.dy);
            if let Some([x1, y1, x2, y2]) = ends {
                let near_start = js::hypot2(symbol.x - x1, symbol.y - y1) <= js::hypot2(symbol.x - x2, symbol.y - y2);
                outward = if near_start { (x1 - x2, y1 - y2) } else { (x2 - x1, y2 - y1) };
            } else if let Some(SxfFeature::Arc { cx, cy, .. }) = &curve {
                let angle = js::atan2(symbol.y - cy, symbol.x - cx);
                outward = if role == "ORIGIN" {
                    (js::sin(angle), -js::cos(angle))
                } else {
                    (-js::sin(angle), js::cos(angle))
                };
            }
            let direction = if outward.0 * symbol.dx + outward.1 * symbol.dy >= 0.0 { 1.0 } else { 2.0 };
            Some(Arrow { code: symbol.code, direction, x: symbol.x, y: symbol.y, scale: symbol.scale })
        };
        if entity.has("LINEAR_DIMENSION")
            && let Some(line) = ends
        {
            return vec![SxfFeature::LinearDim {
                style: styled,
                line,
                ext1: extension(projections.first()),
                ext2: extension(projections.get(1)),
                arrow1: arrow("ORIGIN"),
                arrow2: arrow("TARGET"),
                text: annotation,
            }];
        }
        if (entity.has("CURVE_DIMENSION") || entity.has("ANGULAR_DIMENSION"))
            && let Some(SxfFeature::Arc { cx, cy, r, start, end, .. }) = &curve
        {
            // 寸法の円弧は、始角から終角へ反時計回り（SFC 編 1-2-24・1-2-25）
            let dimension = ArcDimension {
                style: styled.clone(),
                cx: *cx,
                cy: *cy,
                radius: *r,
                angle0: *start,
                angle1: *end,
                ext1: extension(projections.first()),
                ext2: extension(projections.get(1)),
                arrow1: arrow("ORIGIN"),
                arrow2: arrow("TARGET"),
                text: annotation,
            };
            return vec![if entity.has("CURVE_DIMENSION") {
                SxfFeature::CurveDim(dimension)
            } else {
                SxfFeature::AngularDim(dimension)
            }];
        }
        if entity.has("RADIUS_DIMENSION")
            && let Some(line) = ends
        {
            return vec![SxfFeature::RadiusDim {
                style: styled,
                line,
                arrow: arrow("ORIGIN").or_else(|| arrow("TARGET")),
                text: annotation,
            }];
        }
        if entity.has("DIAMETER_DIMENSION")
            && let Some(line) = ends
        {
            return vec![SxfFeature::DiameterDim {
                style: styled,
                line,
                arrow1: arrow("ORIGIN"),
                arrow2: arrow("TARGET"),
                text: annotation,
            }];
        }
        self.notices.add(NoticeLevel::Warning, "p21-callout", "対応していない寸法の表記を省きました");
        Vec::new()
    }

    fn leader(
        &mut self,
        entity: &'a P21Entity,
        contents: &[Option<&'a P21Entity>],
        annotation: Option<Annotation>,
    ) -> Vec<SxfFeature> {
        let Some(leader_curve) = contents.iter().copied().flatten().find(|item| item.has("LEADER_CURVE")) else {
            return Vec::new();
        };
        let (style, item) = Self::styled(Some(leader_curve));
        let layer = self.layer(leader_curve.id);
        let curve_style = self.curve_style(style);
        let styled = styled_with(entity.id, layer, curve_style);
        let Some(SxfFeature::Polyline { xs, ys, .. }) = self.curve(item, styled.clone()) else {
            return Vec::new();
        };
        let terminator = contents.iter().copied().flatten().find(|candidate| candidate.has("LEADER_TERMINATOR"));
        let symbol = terminator.and_then(|terminator| self.terminator_of(terminator));
        let balloon = contents
            .iter()
            .copied()
            .flatten()
            .filter(|candidate| candidate.has("ANNOTATION_CURVE_OCCURRENCE") && !candidate.has("LEADER_CURVE"))
            .map(|candidate| self.curve(Self::styled(Some(candidate)).1, styled.clone()))
            .collect::<Vec<_>>()
            .into_iter()
            .find_map(|curve| match curve {
                Some(SxfFeature::Circle { cx, cy, r, .. }) => Some((cx, cy, r)),
                _ => None,
            });
        let arrow_code = symbol.as_ref().map_or(0.0, |symbol| symbol.code);
        let arrow_scale = symbol.as_ref().map_or(1.0, |symbol| symbol.scale);
        if let Some((cx, cy, radius)) = balloon {
            return vec![SxfFeature::Balloon {
                style: styled,
                xs,
                ys,
                cx,
                cy,
                radius,
                arrow_code,
                arrow_scale,
                text: annotation,
            }];
        }
        vec![SxfFeature::Label { style: styled, xs, ys, arrow_code, arrow_scale, text: annotation }]
    }

    fn sheet(&self) -> (Option<DrawingSheet>, Vec<f64>) {
        let Some(sheet_entity) = self.file.entities.values().find(|entity| entity.has("DRAWING_SHEET_REVISION")) else {
            return (None, Vec::new());
        };
        let args = sheet_entity.args("DRAWING_SHEET_REVISION").unwrap_or(&[]);
        let size_name = text(args.first());
        let items = references(args.get(1));
        let mut x = 0.0;
        let mut y = 0.0;
        for entity in self.file.entities.values() {
            if let Some(size) = entity.args("PRESENTATION_SIZE")
                && reference(size.first()) == Some(sheet_entity.id)
            {
                let planar_box = Self::args(self.get(reference(size.get(1))), "PLANAR_BOX");
                x = number(planar_box.and_then(|args| args.get(1)));
                y = number(planar_box.and_then(|args| args.get(2)));
            }
        }
        let title = self.file.entities.values().find_map(|entity| entity.args("DRAUGHTING_TITLE"));
        // /^(A[0-4])_(horizontal|vertical)$/i
        let size = js::trim(size_name);
        let parsed = size.get(..3).and_then(|head| {
            let bytes = head.as_bytes();
            let valid = bytes[0].eq_ignore_ascii_case(&b'A') && (b'0'..=b'4').contains(&bytes[1]) && bytes[2] == b'_';
            let orientation = &size[3..];
            if !valid {
                None
            } else if orientation.eq_ignore_ascii_case("horizontal") {
                Some((f64::from(bytes[1] - b'0'), 1.0))
            } else if orientation.eq_ignore_ascii_case("vertical") {
                Some((f64::from(bytes[1] - b'0'), 0.0))
            } else {
                None
            }
        });
        let (sheet_type, orient) = parsed.unwrap_or((9.0, 1.0));
        let sheet = DrawingSheet {
            id: sheet_entity.id,
            name: js::trim(text(title.and_then(|args| args.get(2)))).to_string(),
            sheet_type,
            orient,
            x: if x.is_finite() { x } else { 0.0 },
            y: if y.is_finite() { y } else { 0.0 },
        };
        (Some(sheet), items)
    }
}

/// P21 ファイルのバイト列を読み、SFC と同じフィーチャに変換して組み立てる
pub fn parse_p21(bytes: &[u8]) -> Result<SxfDrawing, ReadError> {
    let mut notices = NoticeList::new();
    let file = read_p21(bytes)?;
    if file.skipped > 0 {
        notices.add_count(NoticeLevel::Warning, "unreadable", "書式を読めないインスタンスを省きました", file.skipped);
    }
    let mut converter = Converter {
        file: &file,
        notices,
        doc: SxfDocument::default(),
        colors: HashMap::new(),
        line_types: HashMap::new(),
        widths: HashMap::new(),
        fonts: HashMap::new(),
        layer_of: HashMap::new(),
        invisible: HashSet::new(),
        composite_codes: HashMap::new(),
        curve_occurrence_of: HashMap::new(),
        definitions: HashMap::new(),
        depth: 0,
    };
    converter.prepare_tables();
    let (sheet, items) = converter.sheet();
    if sheet.is_none() {
        converter.notices.add(NoticeLevel::Warning, "no-sheet", "用紙（drawing_sheet_revision）がありません");
    }
    converter.doc.sheet = sheet;
    for item in items {
        let members = converter.item(item);
        converter.doc.sheet_members.extend(members);
    }
    let mut doc = converter.doc;
    let notices = converter.notices;
    let kinds: Vec<_> = doc
        .sheet_members
        .iter()
        .chain(doc.sub_figures.values().flat_map(|figure| figure.members.iter()))
        .map(SxfFeature::kind)
        .collect();
    for kind in kinds {
        doc.count(kind);
    }
    let version =
        suffix_version(&file.header.preprocessor).map_or("SXF".to_string(), |version| format!("SXF {version}"));
    Ok(SxfDrawing {
        format: "p21",
        version,
        header: file.header.clone(),
        document: doc,
        notices: notices.into_vec(),
        source: Some(SxfSource::new(bytes)),
    })
}
