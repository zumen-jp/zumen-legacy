//! SXF の図面（`SxfDrawing`）を P21（STEP AP202 のサブセット、SXF Ver.3.1）に書く。
//!
//! フィーチャごとのインスタンスの組み立ては、SXF Ver.3.1 仕様書・同解説 STEP/AP202 サブセット編の 2 章
//! （テンプレート）と 3 章（P21 の見本）に従う。仕様が決めていない値（名前の ' '、文字の配置点のずらし、
//! BOX_WIDTH、寸法・引出し線の要素の並びとレイヤの割り当て、端末記号の向き）は、SXF の出力ライブラリが書いた
//! 同じ図面の SFC・P21 の組と同じにする（docs/formats/sxf.md の「SXF を書く」）。
//!
//! - 書く順: 単位、色・線種・文字フォント、複合曲線、複合図形（配置される複合図形を先に）、用紙の要素、
//!   図面表題欄と用紙、レイヤ、非表示の指定。インスタンス番号は 10 から 10 ずつ振る。
//! - フィーチャごとに自分の点・向き・配置を持ち、ほかのフィーチャと共有するのは色・線種・線幅・文字フォントと、
//!   それを束ねた線の見え方（PRESENTATION_STYLE_ASSIGNMENT）だけにする（編集で消すインスタンスを決めやすくする）。
//! - 読み取り（p21.rs）が使う参照の形を変えない。寸法補助線の基点は LINE の点、トリムの点は補助線の両端に書く。

use crate::document::{CompositeCurve, SubFigure, SxfDocument, SxfDrawing};
use crate::features::{Annotation, Arrow, DrawingAttribute, DrawingSheet, ExtensionLine, Region, Styled, SxfFeature};
use crate::step::{Body, Part, StepWriter, V, part};
use crate::style::DEFAULT_WIDTH;
use crate::tables::{ARROW_NAMES, PREDEFINED_COLORS, PREDEFINED_LINE_TYPES};
use crate::tables::{index, sheet_size};
use crate::write::{SxfWritten, sub_figure_order};
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::{PI, TAU};
use zumen_core::{NoticeLevel, NoticeList, js};

/// AP202 の draughting_pre_defined_colour の名前（既定義色のコード 1〜8）。9〜16 は SXF だけの色で、
/// 名前に $$SXF_ を付けた COLOUR_RGB に書く
const AP202_COLOURS: usize = 8;

/// 子図を入れる仮のレイヤ（レイヤのない複合図形の配置を入れる）
pub(crate) const DUMMY_LAYER: &str = "$$SXF_dummy_layer_for_subfigure";

/// 複合図形の種類（sfig_org_feature の flag）の接頭語
fn sfig_prefix(flag: f64) -> &'static str {
    match index(flag) {
        Some(1) => "$$SXF_FM_",
        Some(2) => "$$SXF_FG_",
        Some(4) => "$$SXF_P_",
        _ => "$$SXF_G_",
    }
}

/// 既定義色の RGB の割合。SXF の既定義色の 64・128・192 は 1/4 の倍数で書く（SXF の出力ライブラリの P21）
fn colour_fraction(value: u8) -> f64 {
    if value == 255 { 1.0 } else { f64::from(value) / 256.0 }
}

/// コード（f64）を整数にする。数でなければ None
fn code(value: f64) -> Option<i64> {
    value.is_finite().then(|| value.trunc() as i64)
}

/// 文字の配置基点（1〜9）の行（0: 下、1: 中、2: 上）と列（0: 左、1: 中、2: 右）
fn anchor_parts(anchor: f64) -> Option<(usize, usize)> {
    let anchor = index(anchor)?;
    (1..=9).contains(&anchor).then(|| ((anchor - 1) / 3, (anchor - 1) % 3))
}

const ROWS: [&str; 3] = ["baseline", "middleline", "topline"];
/// 点マーカ（コード 1〜7）の pre_defined_point_marker_symbol の名前（tables::MARKER_NAMES の小文字）
const MARKERS: [&str; 7] = ["asterisk", "circle", "dot", "plus", "square", "triangle", "x"];
const COLUMNS: [&str; 3] = ["left", "centre", "right"];

/// 書いた図形の数え方（書けなかった・形を変えた内容の種類）
#[derive(Default)]
struct Counts {
    unknown_code: u64,
    skipped: u64,
    empty_layer: u64,
}

/// P21 のインスタンスを組み立てる。書き出し（このファイル）と、元の P21 の編集（p21_edit.rs）が使う
pub(crate) struct Graph<'a> {
    pub doc: &'a SxfDocument,
    /// 組み立てたインスタンス（番号と本体）。組み立てた順
    pub instances: Vec<(u64, Body)>,
    pub next: u64,
    pub step: u64,
    /// 長さの単位（線幅の LENGTH_MEASURE_WITH_UNIT が参照する）
    pub length_unit: Option<u64>,
    /// 外部の定義（文字フォント・既定義シンボル・外部定義ハッチング）の出どころ
    pub source: Option<u64>,
    /// 色コード → 色のインスタンス
    pub colours: HashMap<i64, u64>,
    /// 線種コード → 線種のインスタンス
    pub line_types: HashMap<i64, u64>,
    /// 線幅（mm の値のビット列）→ LENGTH_MEASURE_WITH_UNIT
    pub widths: HashMap<u64, u64>,
    /// 文字フォントコード → EXTERNALLY_DEFINED_TEXT_FONT
    pub fonts: HashMap<i64, u64>,
    /// (色, 線種, 線幅) のインスタンス → CURVE_STYLE
    curve_styles: HashMap<(u64, u64, u64), u64>,
    /// CURVE_STYLE → PRESENTATION_STYLE_ASSIGNMENT
    curve_assignments: HashMap<u64, u64>,
    /// 色のインスタンス → 記号の PRESENTATION_STYLE_ASSIGNMENT
    symbol_styles: HashMap<u64, u64>,
    null_style: Option<u64>,
    /// レイヤコード → レイヤに割り当てる要素。0 は子図を入れる仮のレイヤ
    pub layer_items: BTreeMap<i64, Vec<u64>>,
    /// 非表示にする複合曲線の occurrence
    pub invisible: Vec<u64>,
    /// 複合曲線の番号（1 から）→ COMPOSITE_CURVE
    pub composite: HashMap<i64, u64>,
    /// 複合図形の名前 → SYMBOL_REPRESENTATION_MAP（まだ組み立てていない循環する配置の先は、番号だけを先に決める）
    pub maps: HashMap<String, u64>,
    /// 図面のインスタンス番号 → 書いたインスタンス番号（書いた順）
    pub ids: Vec<(f64, f64)>,
    counts: Counts,
}

impl<'a> Graph<'a> {
    pub(crate) fn new(doc: &'a SxfDocument, first: u64, step: u64) -> Self {
        Graph {
            doc,
            instances: Vec::new(),
            next: first,
            step,
            length_unit: None,
            source: None,
            colours: HashMap::new(),
            line_types: HashMap::new(),
            widths: HashMap::new(),
            fonts: HashMap::new(),
            curve_styles: HashMap::new(),
            curve_assignments: HashMap::new(),
            symbol_styles: HashMap::new(),
            null_style: None,
            layer_items: BTreeMap::new(),
            invisible: Vec::new(),
            composite: HashMap::new(),
            maps: HashMap::new(),
            ids: Vec::new(),
            counts: Counts::default(),
        }
    }

    fn reserve(&mut self) -> u64 {
        let id = self.next;
        self.next += self.step;
        id
    }

    fn emit(&mut self, id: u64, body: Body) -> u64 {
        self.instances.push((id, body));
        id
    }

    pub(crate) fn add(&mut self, name: &'static str, args: Vec<V>) -> u64 {
        let id = self.reserve();
        self.emit(id, Body::Simple(part(name, args)))
    }

    pub(crate) fn complex(&mut self, parts: Vec<Part>) -> u64 {
        let id = self.reserve();
        self.emit(id, Body::Complex(parts))
    }

    fn point(&mut self, x: f64, y: f64) -> u64 {
        self.add("CARTESIAN_POINT", vec![V::str(" "), V::List(vec![V::Real(x), V::Real(y)])])
    }

    fn direction(&mut self, dx: f64, dy: f64) -> u64 {
        // 長さ 0 の向きは STEP にないので X 軸にする
        let (dx, dy) = if dx == 0.0 && dy == 0.0 || !dx.is_finite() || !dy.is_finite() { (1.0, 0.0) } else { (dx, dy) };
        self.add("DIRECTION", vec![V::str(" "), V::List(vec![V::Real(dx), V::Real(dy)])])
    }

    /// AXIS2_PLACEMENT_2D。向き（度）がなければ X 軸の向きを省く（$）
    fn placement(&mut self, x: f64, y: f64, angle: Option<f64>) -> u64 {
        let direction = angle.map(|angle| {
            let theta = angle * PI / 180.0;
            self.direction(js::cos(theta), js::sin(theta))
        });
        let point = self.point(x, y);
        self.add("AXIS2_PLACEMENT_2D", vec![V::str(" "), V::Ref(point), direction.map_or(V::Unset, V::Ref)])
    }

    /// 長さの単位と角度の単位と、それを使う表現の文脈（GEOMETRIC_REPRESENTATION_CONTEXT）
    fn context(&mut self) -> u64 {
        let angle = self.complex(vec![
            part("NAMED_UNIT", vec![V::Derived]),
            part("PLANE_ANGLE_UNIT", vec![]),
            part("SI_UNIT", vec![V::Unset, V::Enum("RADIAN")]),
        ]);
        let length = self.length_unit_instance();
        self.complex(vec![
            part("GEOMETRIC_REPRESENTATION_CONTEXT", vec![V::Int(2)]),
            part("GLOBAL_UNIT_ASSIGNED_CONTEXT", vec![V::refs(&[angle, length])]),
            part("REPRESENTATION_CONTEXT", vec![V::str("ID1"), V::str("2D")]),
        ])
    }

    fn length_unit_instance(&mut self) -> u64 {
        self.complex(vec![
            part("LENGTH_UNIT", vec![]),
            part("NAMED_UNIT", vec![V::Derived]),
            part("SI_UNIT", vec![V::Enum("MILLI"), V::Enum("METRE")]),
        ])
    }

    fn length_unit(&mut self) -> u64 {
        match self.length_unit {
            Some(id) => id,
            None => {
                let id = self.length_unit_instance();
                self.length_unit = Some(id);
                id
            }
        }
    }

    fn external_source(&mut self) -> u64 {
        match self.source {
            Some(id) => id,
            None => {
                let id = self.add("EXTERNAL_SOURCE", vec![V::typed("IDENTIFIER", V::str("scadec"))]);
                self.source = Some(id);
                id
            }
        }
    }

    /// 色コードのインスタンス。定義のない色は、表示と同じく白にする
    pub(crate) fn colour(&mut self, value: f64) -> u64 {
        let code = match code(value).filter(|_| self.doc.color(value).is_some()) {
            Some(code) => code,
            None => {
                self.counts.unknown_code += 1;
                8
            }
        };
        if let Some(id) = self.colours.get(&code) {
            return *id;
        }
        let id = match usize::try_from(code - 1).ok().and_then(|index| PREDEFINED_COLORS.get(index)) {
            Some(predefined) if (code as usize) <= AP202_COLOURS => {
                self.add("DRAUGHTING_PRE_DEFINED_COLOUR", vec![V::str(predefined.name)])
            }
            Some(predefined) => {
                let [r, g, b] = predefined.rgb;
                self.add(
                    "COLOUR_RGB",
                    vec![
                        V::str(format!("$$SXF_{}", predefined.name)),
                        V::Real(colour_fraction(r)),
                        V::Real(colour_fraction(g)),
                        V::Real(colour_fraction(b)),
                    ],
                )
            }
            None => {
                let user = &self.doc.user_colors[(code - 17) as usize];
                let (r, g, b) = (user.r / 255.0, user.g / 255.0, user.b / 255.0);
                self.add("COLOUR_RGB", vec![V::str(" "), V::Real(r), V::Real(g), V::Real(b)])
            }
        };
        self.colours.insert(code, id);
        id
    }

    /// 線種コードのインスタンス。表にない既定義線種（16）と定義のない線種は、表示と同じく実線にする
    pub(crate) fn line_type(&mut self, value: f64) -> u64 {
        let user_count = self.doc.user_line_types.len() as i64;
        let code = match code(value) {
            Some(code @ 1..=15) => code,
            Some(code) if code >= 17 && code - 17 < user_count => code,
            _ => {
                self.counts.unknown_code += 1;
                1
            }
        };
        if let Some(id) = self.line_types.get(&code) {
            return *id;
        }
        let id = if code <= 15 {
            let name = PREDEFINED_LINE_TYPES[(code - 1) as usize].name;
            self.add("DRAUGHTING_PRE_DEFINED_CURVE_FONT", vec![V::str(name)])
        } else {
            let user = &self.doc.user_line_types[(code - 17) as usize];
            // 線分と空白の対。対にならない最後の線分は空白 0 と組にする（表示は実線のまま）
            let pairs: Vec<(f64, f64)> =
                user.pitch.chunks(2).map(|pair| (pair[0], pair.get(1).copied().unwrap_or(0.0))).collect();
            let patterns: Vec<u64> = pairs
                .into_iter()
                .map(|(visible, invisible)| {
                    self.add("CURVE_STYLE_FONT_PATTERN", vec![V::Real(visible), V::Real(invisible)])
                })
                .collect();
            let name = user.name.clone();
            self.add("CURVE_STYLE_FONT", vec![V::Str(name), V::refs(&patterns)])
        };
        self.line_types.insert(code, id);
        id
    }

    /// 線幅コードの LENGTH_MEASURE_WITH_UNIT。0 と定義のない線幅は、表示と同じく 0.13 mm にする
    pub(crate) fn width(&mut self, value: f64) -> u64 {
        let width = match self.doc.width(value) {
            Some(width) => width,
            None => {
                self.counts.unknown_code += 1;
                DEFAULT_WIDTH
            }
        };
        if let Some(id) = self.widths.get(&width.to_bits()) {
            return *id;
        }
        let unit = self.length_unit();
        let id = self
            .add("LENGTH_MEASURE_WITH_UNIT", vec![V::typed("POSITIVE_LENGTH_MEASURE", V::Real(width)), V::Ref(unit)]);
        self.widths.insert(width.to_bits(), id);
        id
    }

    /// 文字フォントコードの EXTERNALLY_DEFINED_TEXT_FONT。0 と定義のないフォントは値なし（$）
    pub(crate) fn text_font(&mut self, value: f64) -> V {
        let Some(code) = code(value).filter(|code| *code >= 1 && *code as usize <= self.doc.fonts.len()) else {
            if value != 0.0 {
                self.counts.unknown_code += 1;
            }
            return V::Unset;
        };
        if let Some(id) = self.fonts.get(&code) {
            return V::Ref(*id);
        }
        let source = self.external_source();
        let name = self.doc.fonts[(code - 1) as usize].name.clone();
        let id = self.add("EXTERNALLY_DEFINED_TEXT_FONT", vec![V::typed("IDENTIFIER", V::Str(name)), V::Ref(source)]);
        self.fonts.insert(code, id);
        V::Ref(id)
    }

    /// 表の定義（ユーザ定義の色・線種、文字フォント）を表の順に書く。読み取りはインスタンス番号の順にコードを
    /// 割り当てるので、使われていなくても順に書いてコードを保つ
    pub(crate) fn tables(&mut self) {
        for index in 0..self.doc.user_colors.len() {
            let id = self.colour(17.0 + index as f64);
            self.ids.push((self.doc.user_colors[index].id, id as f64));
        }
        for index in 0..self.doc.user_line_types.len() {
            let id = self.line_type(17.0 + index as f64);
            self.ids.push((self.doc.user_line_types[index].id, id as f64));
        }
        for index in 0..self.doc.fonts.len() {
            if let V::Ref(id) = self.text_font(1.0 + index as f64) {
                self.ids.push((self.doc.fonts[index].id, id as f64));
            }
        }
    }

    fn curve_style_instance(&mut self, color: f64, line_type: f64, width: f64) -> u64 {
        let font = self.line_type(line_type);
        let width = self.width(width);
        let colour = self.colour(color);
        let key = (colour, font, width);
        if let Some(id) = self.curve_styles.get(&key) {
            return *id;
        }
        let id = self.add("CURVE_STYLE", vec![V::str(" "), V::Ref(font), V::Ref(width), V::Ref(colour)]);
        self.curve_styles.insert(key, id);
        id
    }

    /// 線の見え方（CURVE_STYLE を入れた PRESENTATION_STYLE_ASSIGNMENT）
    pub(crate) fn curve_style(&mut self, color: f64, line_type: f64, width: f64) -> u64 {
        let style = self.curve_style_instance(color, line_type, width);
        if let Some(id) = self.curve_assignments.get(&style) {
            return *id;
        }
        let id = self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![V::refs(&[style])]);
        self.curve_assignments.insert(style, id);
        id
    }

    fn symbol_style(&mut self, color: f64) -> u64 {
        let colour = self.colour(color);
        if let Some(id) = self.symbol_styles.get(&colour) {
            return *id;
        }
        let symbol_colour = self.add("SYMBOL_COLOUR", vec![V::Ref(colour)]);
        let style = self.add("SYMBOL_STYLE", vec![V::str(" "), V::Ref(symbol_colour)]);
        let id = self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![V::refs(&[style])]);
        self.symbol_styles.insert(colour, id);
        id
    }

    fn null_style(&mut self) -> u64 {
        match self.null_style {
            Some(id) => id,
            None => {
                let id = self
                    .add("PRESENTATION_STYLE_ASSIGNMENT", vec![V::List(vec![V::typed("NULL_STYLE", V::Enum("NULL"))])]);
                self.null_style = Some(id);
                id
            }
        }
    }

    /// 表記の occurrence（ANNOTATION_OCCURRENCE と、その種類の部分）
    fn occurrence(&mut self, kinds: Vec<Part>, name: &str, style: u64, item: u64) -> u64 {
        let mut parts = vec![
            part("ANNOTATION_OCCURRENCE", vec![]),
            part("DRAUGHTING_ANNOTATION_OCCURRENCE", vec![]),
            part("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            part("REPRESENTATION_ITEM", vec![V::str(name)]),
            part("STYLED_ITEM", vec![V::refs(&[style]), V::Ref(item)]),
        ];
        parts.extend(kinds);
        self.complex(parts)
    }

    /// 要素をレイヤに割り当てる。0 はレイヤなし、定義のないレイヤコードは割り当てずに知らせる
    fn assign_layer(&mut self, layer: f64, item: u64) {
        match code(layer) {
            Some(0) => {}
            Some(code) if code >= 1 && code as usize <= self.doc.layers.len() => {
                self.layer_items.entry(code).or_default().push(item);
            }
            _ => self.counts.unknown_code += 1,
        }
    }

    /// 線分（TRIMMED_CURVE）。base は LINE の点（寸法補助線の基点）。省けば始点
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, base: Option<(f64, f64)>) -> u64 {
        let start = self.point(x1, y1);
        let end = self.point(x2, y2);
        let direction = self.direction(x2 - x1, y2 - y1);
        let vector = self.add("VECTOR", vec![V::str(" "), V::Ref(direction), V::Real(1.0)]);
        let origin = match base {
            Some((x, y)) => self.point(x, y),
            None => start,
        };
        let line = self.add("LINE", vec![V::str(" "), V::Ref(origin), V::Ref(vector)]);
        self.add(
            "TRIMMED_CURVE",
            vec![V::str(" "), V::Ref(line), V::refs(&[start]), V::refs(&[end]), V::logical(true), V::Enum("CARTESIAN")],
        )
    }

    /// 円・楕円の円弧（パラメータ（ラジアン）でトリムした TRIMMED_CURVE）
    fn trimmed(&mut self, basis: u64, start: f64, end: f64, sense: bool) -> u64 {
        self.add(
            "TRIMMED_CURVE",
            vec![
                V::str(" "),
                V::Ref(basis),
                V::List(vec![V::typed("PARAMETER_VALUE", V::Real(start))]),
                V::List(vec![V::typed("PARAMETER_VALUE", V::Real(end))]),
                V::logical(sense),
                V::Enum("PARAMETER"),
            ],
        )
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64) -> u64 {
        let placement = self.placement(cx, cy, None);
        self.add("CIRCLE", vec![V::str(" "), V::Ref(placement), V::Real(r)])
    }

    fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, rotation: f64) -> u64 {
        let placement = self.placement(cx, cy, Some(rotation));
        self.add("ELLIPSE", vec![V::str(" "), V::Ref(placement), V::Real(rx), V::Real(ry)])
    }

    fn polyline(&mut self, xs: &[f64], ys: &[f64]) -> u64 {
        let points: Vec<u64> = xs.iter().zip(ys).map(|(x, y)| self.point(*x, *y)).collect();
        self.add("POLYLINE", vec![V::str(" "), V::refs(&points)])
    }

    /// 曲線の幾何。bounded なら、閉じた円・楕円も始点と終点のある曲線（0〜2π の TRIMMED_CURVE）にする
    /// （複合曲線の構成要素は bounded_curve。AP202 の composite_curve_segment）
    fn curve(&mut self, feature: &SxfFeature, bounded: bool) -> Option<u64> {
        Some(match feature {
            SxfFeature::Line { x1, y1, x2, y2, .. } => self.line(*x1, *y1, *x2, *y2, None),
            SxfFeature::Polyline { xs, ys, .. } => self.polyline(xs, ys),
            SxfFeature::Circle { cx, cy, r, .. } => {
                let circle = self.circle(*cx, *cy, *r);
                if bounded { self.trimmed(circle, 0.0, TAU, true) } else { circle }
            }
            SxfFeature::Arc { cx, cy, r, ccw, start, end, .. } => {
                let circle = self.circle(*cx, *cy, *r);
                // 円弧の向き: SFC の 0（反時計回り）は sense_agreement の .T.
                self.trimmed(circle, start * PI / 180.0, end * PI / 180.0, *ccw)
            }
            SxfFeature::Ellipse { cx, cy, rx, ry, rotation, .. } => {
                let ellipse = self.ellipse(*cx, *cy, *rx, *ry, *rotation);
                if bounded { self.trimmed(ellipse, 0.0, TAU, true) } else { ellipse }
            }
            SxfFeature::EllipseArc { cx, cy, rx, ry, ccw, rotation, start, end, .. } => {
                let ellipse = self.ellipse(*cx, *cy, *rx, *ry, *rotation);
                self.trimmed(ellipse, start * PI / 180.0, end * PI / 180.0, *ccw)
            }
            SxfFeature::Spline { closed, xs, ys, .. } => {
                // 3 次ベジェの区間をつないだ曲線は、1 つの BEZIER_CURVE（次数 3、制御点 3n+1）で表せる（ISO 10303-42）
                let points: Vec<u64> = xs.iter().zip(ys).map(|(x, y)| self.point(*x, *y)).collect();
                self.add(
                    "BEZIER_CURVE",
                    vec![
                        V::str(" "),
                        V::Int(3),
                        V::refs(&points),
                        V::Enum("UNSPECIFIED"),
                        V::logical(*closed),
                        V::Enum("U"),
                    ],
                )
            }
            SxfFeature::Clothoid { x, y, parameter, ccw, angle, start_length, end_length, .. } => {
                let placement = self.placement(*x, *y, Some(*angle));
                // 向きは CLOTHOID の name に書く（0: 反時計回り、1: 時計回り）。トリムは曲線長
                let flag = if *ccw { "0" } else { "1" };
                let clothoid = self.add("CLOTHOID", vec![V::str(flag), V::Ref(placement), V::Real(*parameter)]);
                self.trimmed(clothoid, *start_length, *end_length, true)
            }
            _ => return None,
        })
    }

    /// 文字（TEXT_LITERAL_WITH_EXTENT と文字の見え方）の occurrence。SXF の配置基点を、出力ライブラリと同じく
    /// 行ごとに高さの 1/2・−1/2・−1 だけ文字の上方向へずらした点に書く（読み取りが戻す）
    fn text(&mut self, text: &Annotation, color: f64, name: &str) -> u64 {
        let (row, column) = anchor_parts(text.anchor).unwrap_or_else(|| {
            self.counts.unknown_code += 1;
            (0, 0)
        });
        let extent = self.add("PLANAR_EXTENT", vec![V::str(" "), V::Real(text.width), V::Real(text.height)]);
        let theta = text.angle * PI / 180.0;
        let (sin, cos) = (js::sin(theta), js::cos(theta));
        let offset = match row {
            0 => text.height / 2.0,
            1 => -text.height / 2.0,
            _ => -text.height,
        };
        let direction = self.direction(cos, sin);
        let point = self.point(text.x - sin * offset, text.y + cos * offset);
        let placement = self.add("AXIS2_PLACEMENT_2D", vec![V::str(" "), V::Ref(point), V::Ref(direction)]);
        let font = self.text_font(text.font);
        let literal = self.add(
            "TEXT_LITERAL_WITH_EXTENT",
            vec![
                V::str(format!("$$SXF_{} {}", ROWS[row], COLUMNS[column])),
                V::str(text.text.as_str()),
                V::Ref(placement),
                V::str(format!("baseline {}", COLUMNS[column])),
                V::Enum(if text.direction == 2.0 { "DOWN" } else { "RIGHT" }),
                font,
                V::Ref(extent),
            ],
        );
        let colour = self.colour(color);
        let appearance = self.add("TEXT_STYLE_FOR_DEFINED_FONT", vec![V::Ref(colour)]);
        // 文字の幅（BOX_WIDTH）は、出力ライブラリと同じく高さの 0.8 倍。回転は BOX_ROTATE_ANGLE にも書く
        let style = self.complex(vec![
            part("TEXT_STYLE", vec![V::str(" "), V::Ref(appearance)]),
            part(
                "TEXT_STYLE_WITH_BOX_CHARACTERISTICS",
                vec![V::List(vec![
                    V::typed("BOX_HEIGHT", V::Real(text.height)),
                    V::typed("BOX_WIDTH", V::Real(text.height * 0.8)),
                    V::typed("BOX_SLANT_ANGLE", V::Real(text.slant * PI / 180.0)),
                    V::typed("BOX_ROTATE_ANGLE", V::Real(theta)),
                ])],
            ),
            part("TEXT_STYLE_WITH_SPACING", vec![V::typed("LENGTH_MEASURE", V::Real(text.spacing))]),
        ]);
        let assignment = self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![V::refs(&[style])]);
        self.occurrence(vec![part("ANNOTATION_TEXT_OCCURRENCE", vec![])], name, assignment, literal)
    }

    /// 記号（点マーカ・既定義シンボル・端末記号）の occurrence。定義は SYMBOL_TARGET の後に作る（出力ライブラリの順）
    #[allow(clippy::too_many_arguments)]
    fn symbol(
        &mut self,
        definition: Definition,
        (x, y): (f64, f64),
        (dx, dy): (f64, f64),
        (scale_x, scale_y): (f64, f64),
        style: u64,
        kinds: Vec<Part>,
        name: &str,
    ) -> u64 {
        let direction = self.direction(dx, dy);
        let point = self.point(x, y);
        let placement = self.add("AXIS2_PLACEMENT_2D", vec![V::str(" "), V::Ref(point), V::Ref(direction)]);
        let target =
            self.add("SYMBOL_TARGET", vec![V::str(" "), V::Ref(placement), V::Real(scale_x), V::Real(scale_y)]);
        let definition = match definition {
            Definition::Terminator(name) => self.add("PRE_DEFINED_TERMINATOR_SYMBOL", vec![V::str(name)]),
            Definition::Marker(name) => self.add("PRE_DEFINED_POINT_MARKER_SYMBOL", vec![V::str(name)]),
            Definition::External(name) => {
                let source = self.external_source();
                self.add("EXTERNALLY_DEFINED_SYMBOL", vec![V::typed("IDENTIFIER", V::Str(name)), V::Ref(source)])
            }
        };
        let symbol = self.add("DEFINED_SYMBOL", vec![V::str(" "), V::Ref(definition), V::Ref(target)]);
        let mut parts = vec![part("ANNOTATION_SYMBOL_OCCURRENCE", vec![])];
        parts.extend(kinds);
        self.occurrence(parts, name, style, symbol)
    }

    /// 寸法線・引出し線の矢印の名前。定義のない矢印コードは書かずに知らせる（表示も矢印を描かない）
    fn arrow_name(&mut self, code: f64) -> Option<&'static str> {
        let name = index(code).and_then(|code| code.checked_sub(1)).and_then(|index| ARROW_NAMES.get(index));
        if name.is_none() {
            self.counts.unknown_code += 1;
        }
        name.copied()
    }

    /// 用紙・複合図形の要素を書き、要素の並びに入れるインスタンス（occurrence・寸法の callout）を返す
    pub(crate) fn member(&mut self, feature: &SxfFeature) -> Option<u64> {
        let root = match feature {
            SxfFeature::Line { style, .. }
            | SxfFeature::Polyline { style, .. }
            | SxfFeature::Circle { style, .. }
            | SxfFeature::Arc { style, .. }
            | SxfFeature::Ellipse { style, .. }
            | SxfFeature::EllipseArc { style, .. }
            | SxfFeature::Spline { style, .. }
            | SxfFeature::Clothoid { style, .. } => {
                let geometry = self.curve(feature, false)?;
                let assignment = self.curve_style(style.color, style.line_type, style.line_width);
                let occurrence =
                    self.occurrence(vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![])], " ", assignment, geometry);
                self.assign_layer(style.layer, occurrence);
                occurrence
            }
            SxfFeature::TextString { layer, color, text, .. } => {
                let occurrence = self.text(text, *color, " ");
                self.assign_layer(*layer, occurrence);
                occurrence
            }
            SxfFeature::PointMarker { layer, color, x, y, code: marker, angle, scale, .. } => {
                // 定義のない点マーカは、表示と同じく点にする
                let name = match index(*marker).and_then(|code| code.checked_sub(1)).and_then(|i| MARKERS.get(i)) {
                    Some(name) => *name,
                    None => {
                        self.counts.unknown_code += 1;
                        "dot"
                    }
                };
                let theta = angle * PI / 180.0;
                let style = self.symbol_style(*color);
                let occurrence = self.symbol(
                    Definition::Marker(name),
                    (*x, *y),
                    (js::cos(theta), js::sin(theta)),
                    (*scale, *scale),
                    style,
                    vec![],
                    " ",
                );
                self.assign_layer(*layer, occurrence);
                occurrence
            }
            SxfFeature::ExternallyDefinedSymbol { layer, color, name, x, y, angle, scale, .. } => {
                let theta = angle * PI / 180.0;
                let style = match color {
                    Some(color) => self.symbol_style(*color),
                    None => self.null_style(),
                };
                let occurrence = self.symbol(
                    Definition::External(name.clone()),
                    (*x, *y),
                    (js::cos(theta), js::sin(theta)),
                    (*scale, *scale),
                    style,
                    vec![],
                    " ",
                );
                self.assign_layer(*layer, occurrence);
                occurrence
            }
            SxfFeature::SfigLocate { layer, name, x, y, angle, ratio_x, ratio_y, .. } => {
                self.sfig_locate(*layer, name, (*x, *y), *angle, (*ratio_x, *ratio_y))?
            }
            SxfFeature::LinearDim { .. }
            | SxfFeature::CurveDim(_)
            | SxfFeature::AngularDim(_)
            | SxfFeature::RadiusDim { .. }
            | SxfFeature::DiameterDim { .. } => self.dimension(feature),
            SxfFeature::Label { style, xs, ys, arrow_code, arrow_scale, text } => {
                self.leader(style, xs, ys, None, (*arrow_code, *arrow_scale), text)
            }
            SxfFeature::Balloon { style, xs, ys, cx, cy, radius, arrow_code, arrow_scale, text } => {
                self.leader(style, xs, ys, Some((*cx, *cy, *radius)), (*arrow_code, *arrow_scale), text)
            }
            SxfFeature::ExternallyDefinedHatch { .. }
            | SxfFeature::FillAreaStyleColour { .. }
            | SxfFeature::FillAreaStyleHatching { .. }
            | SxfFeature::FillAreaStyleTiles { .. } => self.fill(feature)?,
            _ => {
                self.counts.skipped += 1;
                return None;
            }
        };
        self.ids.push((feature.id(), root as f64));
        Some(root)
    }

    fn sfig_locate(
        &mut self,
        layer: f64,
        name: &str,
        (x, y): (f64, f64),
        angle: f64,
        (ratio_x, ratio_y): (f64, f64),
    ) -> Option<u64> {
        let Some(figure) = self.doc.sub_figures.get(name) else {
            // 定義のない複合図形の配置は書けない
            self.counts.skipped += 1;
            return None;
        };
        let prefix = sfig_prefix(figure.feature.flag);
        let map = match self.maps.get(name) {
            Some(map) => *map,
            None => {
                // 循環する配置の先（まだ組み立てていない複合図形）は、番号だけを先に決める
                let map = self.reserve();
                self.maps.insert(name.to_string(), map);
                map
            }
        };
        let placement = self.placement(x, y, Some(angle));
        let target =
            self.add("SYMBOL_TARGET", vec![V::str(" "), V::Ref(placement), V::Real(ratio_x), V::Real(ratio_y)]);
        let mapped = self.complex(vec![
            part("ANNOTATION_SYMBOL", vec![]),
            part("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            part("MAPPED_ITEM", vec![V::Ref(map), V::Ref(target)]),
            part("REPRESENTATION_ITEM", vec![V::str(" ")]),
        ]);
        let style = self.null_style();
        let occurrence = self.occurrence(
            vec![part("ANNOTATION_SUBFIGURE_OCCURRENCE", vec![]), part("ANNOTATION_SYMBOL_OCCURRENCE", vec![])],
            &format!("{prefix}{name}"),
            style,
            mapped,
        );
        // レイヤのない配置は、子図を入れる仮のレイヤに入れる
        if code(layer) == Some(0) {
            self.layer_items.entry(0).or_default().push(occurrence);
        } else {
            self.assign_layer(layer, occurrence);
        }
        Some(occurrence)
    }

    /// 寸法（直線・弧長・角度・半径・直径）の DRAUGHTING_CALLOUT。要素の並びは寸法線、寸法補助線、寸法値、矢印。
    /// 寸法値があれば、STRUCTURED_DIMENSION_CALLOUT と DIMENSION_CALLOUT_RELATIONSHIP で結ぶ
    fn dimension(&mut self, feature: &SxfFeature) -> u64 {
        let dimension = DimensionParts::of(feature);
        let DimensionParts { kind, style, shape, extensions, arrows, text } = dimension;
        let assignment = self.curve_style(style.color, style.line_type, style.line_width);
        let geometry = match &shape {
            Shape::Line([x1, y1, x2, y2]) => self.line(*x1, *y1, *x2, *y2, None),
            Shape::Arc { cx, cy, radius, angle0, angle1 } => {
                // 寸法の円弧は始角から終角へ反時計回り（SFC 編 1-2-24・1-2-25）
                let circle = self.circle(*cx, *cy, *radius);
                self.trimmed(circle, angle0 * PI / 180.0, angle1 * PI / 180.0, true)
            }
        };
        let curve = self.occurrence(
            vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![]), part("DIMENSION_CURVE", vec![])],
            " ",
            assignment,
            geometry,
        );
        self.assign_layer(style.layer, curve);
        let mut contents = vec![curve];
        for (position, extension) in extensions.into_iter().enumerate() {
            let Some(extension) = extension else {
                continue;
            };
            let geometry =
                self.line(extension.x1, extension.y1, extension.x2, extension.y2, Some((extension.x0, extension.y0)));
            contents.push(self.occurrence(
                vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![]), part("PROJECTION_CURVE", vec![])],
                &format!("$$SXF_prj_{}", position + 1),
                assignment,
                geometry,
            ));
        }
        let text_occurrence = text.as_ref().map(|text| {
            let occurrence = self.text(text, style.color, "dimension value");
            self.assign_layer(style.layer, occurrence);
            occurrence
        });
        contents.extend(text_occurrence);
        let mut number = 0;
        for (role, arrow) in arrows {
            number += 1;
            let Some(arrow) = arrow else {
                continue;
            };
            let Some(name) = self.arrow_name(arrow.code) else {
                continue;
            };
            let outward = match &shape {
                Shape::Line(line) => line_outward(line, arrow),
                Shape::Arc { cx, cy, .. } => {
                    let angle = js::atan2(arrow.y - cy, arrow.x - cx);
                    if role == "ORIGIN" { (js::sin(angle), -js::cos(angle)) } else { (-js::sin(angle), js::cos(angle)) }
                }
            };
            // 端末記号の X 軸は矢印の指す向き。SFC の 1 は寸法線の外向き、2 は内向き
            let sign = if arrow.direction == 2.0 { -1.0 } else { 1.0 };
            let length = js::hypot2(outward.0, outward.1);
            let direction =
                if length > 0.0 { (sign * outward.0 / length, sign * outward.1 / length) } else { (sign, 0.0) };
            let style = self.symbol_style(style.color);
            contents.push(self.symbol(
                Definition::Terminator(name),
                (arrow.x, arrow.y),
                direction,
                (arrow.scale, arrow.scale),
                style,
                vec![
                    part("DIMENSION_CURVE_TERMINATOR", vec![V::Enum(role)]),
                    part("TERMINATOR_SYMBOL", vec![V::Ref(curve)]),
                ],
                &format!("$$SXF_arw_{number}"),
            ));
        }
        let callout = self.complex(vec![
            part("DIMENSION_CURVE_DIRECTED_CALLOUT", vec![]),
            part("DRAUGHTING_CALLOUT", vec![V::refs(&contents)]),
            part("DRAUGHTING_ELEMENTS", vec![]),
            part("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            part(kind, vec![]),
            part("REPRESENTATION_ITEM", vec![V::str(" ")]),
        ]);
        if let Some(text) = text_occurrence {
            let structured = self.complex(vec![
                part("DRAUGHTING_CALLOUT", vec![V::refs(&[text])]),
                part("DRAUGHTING_ELEMENTS", vec![]),
                part("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
                part("REPRESENTATION_ITEM", vec![V::str(" ")]),
                part("STRUCTURED_DIMENSION_CALLOUT", vec![]),
            ]);
            self.add(
                "DIMENSION_CALLOUT_RELATIONSHIP",
                vec![V::str("primary"), V::str(" "), V::Ref(callout), V::Ref(structured)],
            );
        }
        callout
    }

    /// 引出し線・バルーンの DRAUGHTING_CALLOUT（LEADER_DIRECTED_CALLOUT）。要素の並びは線、バルーンの円、文字、矢印。
    /// 矢印は最初の点に、2 点目から最初の点へ向けて置く
    fn leader(
        &mut self,
        style: &Styled,
        xs: &[f64],
        ys: &[f64],
        balloon: Option<(f64, f64, f64)>,
        (arrow_code, arrow_scale): (f64, f64),
        text: &Option<Annotation>,
    ) -> u64 {
        let assignment = self.curve_style(style.color, style.line_type, style.line_width);
        let geometry = self.polyline(xs, ys);
        let curve = self.occurrence(
            vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![]), part("LEADER_CURVE", vec![])],
            " ",
            assignment,
            geometry,
        );
        self.assign_layer(style.layer, curve);
        let mut contents = vec![curve];
        if let Some((cx, cy, radius)) = balloon {
            let circle = self.circle(cx, cy, radius);
            contents.push(self.occurrence(vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![])], " ", assignment, circle));
        }
        if let Some(text) = text {
            let occurrence = self.text(text, style.color, "dimension value");
            self.assign_layer(style.layer, occurrence);
            contents.push(occurrence);
        }
        if arrow_code != 0.0
            && let Some(name) = self.arrow_name(arrow_code)
        {
            let (x0, y0) = (xs.first().copied().unwrap_or(0.0), ys.first().copied().unwrap_or(0.0));
            let direction = match (xs.get(1), ys.get(1)) {
                (Some(x1), Some(y1)) => (x0 - x1, y0 - y1),
                _ => (1.0, 0.0),
            };
            let symbol_style = self.symbol_style(style.color);
            contents.push(self.symbol(
                Definition::Terminator(name),
                (x0, y0),
                direction,
                (arrow_scale, arrow_scale),
                symbol_style,
                vec![part("LEADER_TERMINATOR", vec![]), part("TERMINATOR_SYMBOL", vec![V::Ref(curve)])],
                " ",
            ));
        }
        self.complex(vec![
            part("DRAUGHTING_CALLOUT", vec![V::refs(&contents)]),
            part("DRAUGHTING_ELEMENTS", vec![]),
            part("GEOMETRIC_REPRESENTATION_ITEM", vec![]),
            part("LEADER_DIRECTED_CALLOUT", vec![]),
            part("REPRESENTATION_ITEM", vec![V::str(" ")]),
        ])
    }

    /// ハッチングの ANNOTATION_FILL_AREA_OCCURRENCE。外形・中抜きは複合曲線の番号から COMPOSITE_CURVE を引く
    fn fill(&mut self, feature: &SxfFeature) -> Option<u64> {
        let (layer, region) = match feature {
            SxfFeature::ExternallyDefinedHatch { layer, region, .. }
            | SxfFeature::FillAreaStyleColour { layer, region, .. }
            | SxfFeature::FillAreaStyleHatching { layer, region, .. }
            | SxfFeature::FillAreaStyleTiles { layer, region, .. } => (*layer, region),
            _ => return None,
        };
        let Some(boundaries) = self.boundaries(region) else {
            // 定義のない複合曲線を外形・中抜きにしたハッチングは書けない
            self.counts.skipped += 1;
            return None;
        };
        let area = self.add("ANNOTATION_FILL_AREA", vec![V::str(" "), V::refs(&boundaries)]);
        let mut target = None;
        let styles: Vec<u64> = match feature {
            SxfFeature::FillAreaStyleColour { color, .. } => {
                let colour = self.colour(*color);
                vec![self.add("FILL_AREA_STYLE_COLOUR", vec![V::str(" "), V::Ref(colour)])]
            }
            SxfFeature::ExternallyDefinedHatch { name, .. } => {
                let source = self.external_source();
                vec![self.add(
                    "EXTERNALLY_DEFINED_HATCH_STYLE",
                    vec![V::typed("IDENTIFIER", V::str(name.as_str())), V::Ref(source), V::str(" ")],
                )]
            }
            SxfFeature::FillAreaStyleHatching { patterns, .. } => {
                let mut styles = Vec::new();
                for pattern in patterns {
                    let start = self.point(pattern.x, pattern.y);
                    target.get_or_insert(start);
                    // 繰り返しの向きはハッチング線に直交する向き（角度 + 90°）、長さが間隔
                    let theta = pattern.angle * PI / 180.0;
                    let direction = self.direction(-js::sin(theta), js::cos(theta));
                    let vector = self.add("VECTOR", vec![V::str(" "), V::Ref(direction), V::Real(pattern.spacing)]);
                    let repeat = self.add("ONE_DIRECTION_REPEAT_FACTOR", vec![V::str(" "), V::Ref(vector)]);
                    let curve_style = self.curve_style_instance(pattern.color, pattern.line_type, pattern.line_width);
                    styles.push(self.add(
                        "FILL_AREA_STYLE_HATCHING",
                        vec![
                            V::str(" "),
                            V::Ref(curve_style),
                            V::Ref(repeat),
                            V::Ref(start),
                            V::Ref(start),
                            V::Real(theta),
                        ],
                    ));
                }
                styles
            }
            SxfFeature::FillAreaStyleTiles {
                name,
                color,
                x,
                y,
                vector1,
                vector1_angle,
                vector2,
                vector2_angle,
                scale_x,
                scale_y,
                angle,
                ..
            } => {
                let theta = angle * PI / 180.0;
                let style = self.symbol_style(*color);
                let symbol = self.symbol(
                    Definition::External(name.clone()),
                    (*x, *y),
                    (js::cos(theta), js::sin(theta)),
                    (*scale_x, *scale_y),
                    style,
                    vec![],
                    " ",
                );
                let tile = self.add("FILL_AREA_STYLE_TILE_SYMBOL_WITH_STYLE", vec![V::str(" "), V::Ref(symbol)]);
                let mut vector = |length: f64, angle: f64| {
                    let theta = angle * PI / 180.0;
                    let direction = self.direction(js::cos(theta), js::sin(theta));
                    self.add("VECTOR", vec![V::str(" "), V::Ref(direction), V::Real(length)])
                };
                let first = vector(*vector1, *vector1_angle);
                let second = vector(*vector2, *vector2_angle);
                let repeat = self.add("TWO_DIRECTION_REPEAT_FACTOR", vec![V::str(" "), V::Ref(first), V::Ref(second)]);
                target = Some(self.point(*x, *y));
                vec![
                    self.add(
                        "FILL_AREA_STYLE_TILES",
                        vec![V::str(" "), V::Ref(repeat), V::refs(&[tile]), V::Real(1.0)],
                    ),
                ]
            }
            _ => return None,
        };
        let fill_style = self.add("FILL_AREA_STYLE", vec![V::str(" "), V::refs(&styles)]);
        let assignment = self.add("PRESENTATION_STYLE_ASSIGNMENT", vec![V::refs(&[fill_style])]);
        // 塗りの基準点（fill_style_target）は、ハッチング線の始点か、外形の最初の点
        let target = match target {
            Some(target) => target,
            None => {
                let (x, y) = self.first_point(region.out_id);
                self.point(x, y)
            }
        };
        let occurrence =
            self.occurrence(vec![part("ANNOTATION_FILL_AREA_OCCURRENCE", vec![V::Ref(target)])], " ", assignment, area);
        self.assign_layer(layer, occurrence);
        Some(occurrence)
    }

    fn boundaries(&self, region: &Region) -> Option<Vec<u64>> {
        std::iter::once(region.out_id)
            .chain(region.in_ids.iter().copied())
            .map(|number| code(number).and_then(|number| self.composite.get(&number).copied()))
            .collect()
    }

    /// 複合曲線の最初の構成要素の始点
    fn first_point(&self, number: f64) -> (f64, f64) {
        let curve = code(number)
            .and_then(|number| usize::try_from(number - 1).ok())
            .and_then(|index| self.doc.composite_curves.get(index));
        let point = curve.and_then(|curve| curve.members.first()).and_then(|member| match member {
            SxfFeature::Line { x1, y1, .. } => Some((*x1, *y1)),
            SxfFeature::Polyline { xs, ys, .. } | SxfFeature::Spline { xs, ys, .. } => {
                Some((*xs.first()?, *ys.first()?))
            }
            SxfFeature::Arc { cx, cy, r, start, .. } => {
                let theta = start * PI / 180.0;
                Some((cx + r * js::cos(theta), cy + r * js::sin(theta)))
            }
            SxfFeature::Clothoid { x, y, .. } => Some((*x, *y)),
            SxfFeature::Circle { cx, cy, .. }
            | SxfFeature::Ellipse { cx, cy, .. }
            | SxfFeature::EllipseArc { cx, cy, .. } => Some((*cx, *cy)),
            _ => None,
        });
        point.unwrap_or((0.0, 0.0))
    }

    /// 複合曲線（COMPOSITE_CURVE と、その線の見え方を持つ occurrence）。occurrence は要素の並びに入れず、
    /// 非表示なら INVISIBILITY に入れる（出力ライブラリと同じ）
    pub(crate) fn composite_curve(&mut self, number: i64, curve: &CompositeCurve) -> u64 {
        let mut segments = Vec::new();
        for member in &curve.members {
            let Some(geometry) = self.curve(member, true) else {
                self.counts.skipped += 1;
                continue;
            };
            self.ids.push((member.id(), geometry as f64));
            segments.push(
                self.add("COMPOSITE_CURVE_SEGMENT", vec![V::Enum("CONTINUOUS"), V::logical(true), V::Ref(geometry)]),
            );
        }
        let geometry = self.add("COMPOSITE_CURVE", vec![V::str(" "), V::refs(&segments), V::logical(false)]);
        let feature = &curve.feature;
        let assignment = self.curve_style(feature.color, feature.line_type, feature.line_width);
        let occurrence = self.occurrence(vec![part("ANNOTATION_CURVE_OCCURRENCE", vec![])], " ", assignment, geometry);
        if !feature.visible {
            self.invisible.push(occurrence);
        }
        self.composite.insert(number, geometry);
        self.ids.push((feature.id, geometry as f64));
        geometry
    }

    /// 複合図形（DRAUGHTING_SUBFIGURE_REPRESENTATION と SYMBOL_REPRESENTATION_MAP）。要素の並びの最後に原点の配置を置く
    pub(crate) fn sub_figure(&mut self, figure: &SubFigure) -> u64 {
        let mut items: Vec<u64> = figure.members.iter().filter_map(|member| self.member(member)).collect();
        let context = self.context();
        let origin = self.placement(0.0, 0.0, None);
        items.push(origin);
        let name = format!("{}{}", sfig_prefix(figure.feature.flag), figure.feature.name);
        let representation =
            self.add("DRAUGHTING_SUBFIGURE_REPRESENTATION", vec![V::Str(name), V::refs(&items), V::Ref(context)]);
        let body = Body::Simple(part("SYMBOL_REPRESENTATION_MAP", vec![V::Ref(origin), V::Ref(representation)]));
        match self.maps.get(&figure.feature.name) {
            // 循環する配置が先に番号を決めていれば、その番号で書く
            Some(map) => {
                let map = *map;
                self.emit(map, body);
            }
            None => {
                let map = self.reserve();
                self.emit(map, body);
                self.maps.insert(figure.feature.name.clone(), map);
            }
        }
        self.ids.push((figure.feature.id, representation as f64));
        representation
    }

    /// 図面表題欄と用紙（DRAWING_SHEET_REVISION とその大きさ）。要素の並びの最後に用紙の枠（PLANAR_BOX）を置く。
    /// 用紙の名前は図面表題欄の図面名、なければ用紙の名前を DRAUGHTING_TITLE に書く（読み取りは用紙の名前にする）
    pub(crate) fn sheet(
        &mut self,
        sheet: Option<&DrawingSheet>,
        title: Option<&DrawingAttribute>,
        mut items: Vec<u64>,
    ) -> u64 {
        let value = |key: &str| -> String {
            let value = title.and_then(|title| title.values.iter().find(|value| value.key == key));
            match value {
                Some(value) if !value.value.is_empty() => value.value.clone(),
                _ => " ".into(),
            }
        };
        let blank = |text: &str| js::trim(text).is_empty();
        let name = match (value("drawingName"), sheet) {
            (name, _) if !blank(&name) => name,
            (_, Some(sheet)) if !sheet.name.is_empty() => sheet.name.clone(),
            _ => " ".into(),
        };
        let definition =
            self.add("DRAWING_DEFINITION", vec![V::Str(value("drawingNumber")), V::Str(value("drawingType"))]);
        let revision =
            self.add("DRAUGHTING_DRAWING_REVISION", vec![V::str("01"), V::Ref(definition), V::Str(value("scale"))]);
        self.add("DRAUGHTING_TITLE", vec![V::refs(&[revision]), V::str("JAPANESE"), V::Str(name)]);
        let (size_name, width, height) = match sheet {
            Some(sheet) => match sheet_size(sheet.sheet_type) {
                Some((label, width, height)) => {
                    let landscape = sheet.orient != 0.0;
                    let orientation = if landscape { "horizontal" } else { "vertical" };
                    let (width, height) = if landscape { (height, width) } else { (width, height) };
                    (format!("{label}_{orientation}"), width, height)
                }
                None => ("FREE".into(), sheet.x, sheet.y),
            },
            None => ("FREE".into(), 0.0, 0.0),
        };
        let direction = self.direction(1.0, 0.0);
        let point = self.point(0.0, 0.0);
        let placement = self.add("AXIS2_PLACEMENT_2D", vec![V::str(" "), V::Ref(point), V::Ref(direction)]);
        let planar_box = self.add("PLANAR_BOX", vec![V::str(" "), V::Real(width), V::Real(height), V::Ref(placement)]);
        let context = self.context();
        items.push(planar_box);
        let sheet_id =
            self.add("DRAWING_SHEET_REVISION", vec![V::Str(size_name), V::refs(&items), V::Ref(context), V::str("01")]);
        self.add("DRAWING_SHEET_REVISION_USAGE", vec![V::Ref(sheet_id), V::Ref(revision), V::str("01")]);
        self.add("PRESENTATION_SIZE", vec![V::Ref(sheet_id), V::Ref(planar_box)]);
        if let Some(title) = title {
            // 契約区分・工事名・事業名、作成年月日、受注会社名・発注事業者名（附属書 SFC 編 1-2 の図面表題欄）
            let contract_type = self.add("CONTRACT_TYPE", vec![V::Str(value("contractType"))]);
            let contract = self.add(
                "CONTRACT",
                vec![V::Str(value("constructionName")), V::Str(value("businessName")), V::Ref(contract_type)],
            );
            self.add("DRAUGHTING_CONTRACT_ASSIGNMENT", vec![V::Ref(contract), V::refs(&[revision])]);
            let status = self.add("APPROVAL_STATUS", vec![V::str(" ")]);
            let approval = self.add("APPROVAL", vec![V::Ref(status), V::str(" ")]);
            let number = |key: &str, fallback: i64| {
                let value = js::parse_float(&value(key));
                if value.is_finite() { value.trunc() as i64 } else { fallback }
            };
            // CALENDAR_DATE(年, 日, 月)
            let date = self.add(
                "CALENDAR_DATE",
                vec![V::Int(number("year", 0)), V::Int(number("day", 1)), V::Int(number("month", 1))],
            );
            self.add("APPROVAL_DATE_TIME", vec![V::Ref(date), V::Ref(approval)]);
            self.add("DRAUGHTING_APPROVAL_ASSIGNMENT", vec![V::Ref(approval), V::refs(&[revision])]);
            for (role, key) in [("creator", "contractor"), ("owner", "client")] {
                let role = self.add("ORGANIZATION_ROLE", vec![V::str(role)]);
                let organization = self.add("ORGANIZATION", vec![V::Unset, V::Str(value(key)), V::str(" ")]);
                self.add(
                    "DRAUGHTING_ORGANIZATION_ASSIGNMENT",
                    vec![V::Ref(organization), V::Ref(role), V::refs(&[revision])],
                );
            }
            self.ids.push((title.id, revision as f64));
        }
        if let Some(sheet) = sheet {
            self.ids.push((sheet.id, sheet_id as f64));
        }
        sheet_id
    }

    /// レイヤ（PRESENTATION_LAYER_ASSIGNMENT と PRESENTATION_LAYER_USAGE）と非表示の指定（INVISIBILITY）。
    /// 要素のないレイヤは書けない（assigned_items は 1 つ以上）ので省き、件数を知らせる
    pub(crate) fn layers(&mut self, sheet: u64) {
        if let Some(items) = self.layer_items.get(&0).cloned() {
            let assignment =
                self.add("PRESENTATION_LAYER_ASSIGNMENT", vec![V::str(DUMMY_LAYER), V::str(" "), V::refs(&items)]);
            self.add("PRESENTATION_LAYER_USAGE", vec![V::Ref(assignment), V::Ref(sheet)]);
        }
        let mut invisible = Vec::new();
        for (index, layer) in self.doc.layers.iter().enumerate() {
            let Some(items) = self.layer_items.get(&(index as i64 + 1)).cloned() else {
                self.counts.empty_layer += 1;
                continue;
            };
            let assignment = self
                .add("PRESENTATION_LAYER_ASSIGNMENT", vec![V::str(layer.name.as_str()), V::str(" "), V::refs(&items)]);
            self.add("PRESENTATION_LAYER_USAGE", vec![V::Ref(assignment), V::Ref(sheet)]);
            if !layer.visible {
                invisible.push(assignment);
            }
            self.ids.push((layer.id, assignment as f64));
        }
        invisible.extend(self.invisible.iter().copied());
        if !invisible.is_empty() {
            self.add("INVISIBILITY", vec![V::refs(&invisible)]);
        }
    }

    /// 書けなかった・形を変えた内容
    pub(crate) fn take_notices(&mut self, notices: &mut NoticeList) {
        let counts = std::mem::take(&mut self.counts);
        if counts.unknown_code > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "unknown-code",
                "定義のない色・線種・線幅・文字フォント・レイヤ・記号のコードを、表示と同じ値にして書きました",
                counts.unknown_code,
            );
        }
        if counts.skipped > 0 {
            notices.add_count(
                NoticeLevel::Warning,
                "not-written",
                "定義のない複合図形・複合曲線を参照する要素と、要素にならないフィーチャを書いていません",
                counts.skipped,
            );
        }
        if counts.empty_layer > 0 {
            notices.add_count(
                NoticeLevel::Info,
                "empty-layer",
                "要素のないレイヤは P21 に書けないため省きました",
                counts.empty_layer,
            );
        }
    }
}

/// 寸法線の形
enum Shape {
    Line([f64; 4]),
    Arc { cx: f64, cy: f64, radius: f64, angle0: f64, angle1: f64 },
}

/// 寸法のフィーチャを、P21 の寸法の組み立てに使う部分に分けたもの
struct DimensionParts<'f> {
    /// 寸法の種類（LINEAR_DIMENSION など）
    kind: &'static str,
    style: &'f Styled,
    shape: Shape,
    extensions: Vec<&'f Option<ExtensionLine>>,
    /// 矢印の役割（ORIGIN・TARGET）と矢印
    arrows: Vec<(&'static str, &'f Option<Arrow>)>,
    text: &'f Option<Annotation>,
}

impl<'f> DimensionParts<'f> {
    fn of(feature: &'f SxfFeature) -> Self {
        match feature {
            SxfFeature::LinearDim { style, line, ext1, ext2, arrow1, arrow2, text } => DimensionParts {
                kind: "LINEAR_DIMENSION",
                style,
                shape: Shape::Line(*line),
                extensions: vec![ext1, ext2],
                arrows: vec![("ORIGIN", arrow1), ("TARGET", arrow2)],
                text,
            },
            SxfFeature::CurveDim(dimension) | SxfFeature::AngularDim(dimension) => DimensionParts {
                kind: if matches!(feature, SxfFeature::CurveDim(_)) { "CURVE_DIMENSION" } else { "ANGULAR_DIMENSION" },
                style: &dimension.style,
                shape: Shape::Arc {
                    cx: dimension.cx,
                    cy: dimension.cy,
                    radius: dimension.radius,
                    angle0: dimension.angle0,
                    angle1: dimension.angle1,
                },
                extensions: vec![&dimension.ext1, &dimension.ext2],
                arrows: vec![("ORIGIN", &dimension.arrow1), ("TARGET", &dimension.arrow2)],
                text: &dimension.text,
            },
            SxfFeature::RadiusDim { style, line, arrow, text } => DimensionParts {
                kind: "RADIUS_DIMENSION",
                style,
                shape: Shape::Line(*line),
                extensions: vec![],
                arrows: vec![("ORIGIN", arrow)],
                text,
            },
            SxfFeature::DiameterDim { style, line, arrow1, arrow2, text } => DimensionParts {
                kind: "DIAMETER_DIMENSION",
                style,
                shape: Shape::Line(*line),
                extensions: vec![],
                arrows: vec![("ORIGIN", arrow1), ("TARGET", arrow2)],
                text,
            },
            _ => unreachable!("寸法のフィーチャだけを渡す"),
        }
    }
}

/// 記号の定義
enum Definition {
    Terminator(&'static str),
    Marker(&'static str),
    External(String),
}

/// 直線の寸法の矢印の、寸法線の外向き。矢印に近い方の端から外へ向かう（読み取りと同じ判定）
fn line_outward([x1, y1, x2, y2]: &[f64; 4], arrow: &Arrow) -> (f64, f64) {
    let near_start = js::hypot2(arrow.x - x1, arrow.y - y1) <= js::hypot2(arrow.x - x2, arrow.y - y2);
    if near_start { (x1 - x2, y1 - y2) } else { (x2 - x1, y2 - y1) }
}

/// 書いたインスタンスを、番号の順に P21 のファイルにする
pub(crate) fn file(header: &str, instances: &[(u64, Body)], writer: &mut StepWriter) -> Vec<u8> {
    let mut sorted: Vec<&(u64, Body)> = instances.iter().collect();
    sorted.sort_by_key(|(id, _)| *id);
    let mut out = String::from(header);
    for (id, body) in sorted {
        out.push_str(&writer.instance(*id, body));
        out.push('\n');
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out.into_bytes()
}

/// 新しく書く P21 のヘッダ。ファイル名の拡張子は .p21、出力ライブラリ名の末尾の $$3.1 が SXF の版
pub(crate) fn header(drawing: &SxfDrawing) -> String {
    let header = &drawing.header;
    let file_name = match header.file_name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => format!("{stem}.p21"),
        _ if !header.file_name.is_empty() => format!("{}.p21", header.file_name),
        _ => "drawing.p21".into(),
    };
    let system = if header.originating_system.is_empty() { "zumen" } else { header.originating_system.as_str() };
    let mut out =
        String::from("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('SCADEC level2 AP202_mode'),'2;1');\nFILE_NAME(");
    crate::step::string(&file_name, &mut out);
    out.push_str(",'',(''),(''),'zumen$$3.1',");
    crate::step::string(system, &mut out);
    out.push_str(",'');\nFILE_SCHEMA(('ASSOCIATIVE_DRAUGHTING'));\nENDSEC;\nDATA;\n");
    out
}

/// SXF の図面を P21 ファイル（SXF Ver.3.1、AP202）に新しく書く
pub(crate) fn write_p21_new(drawing: &SxfDrawing) -> SxfWritten {
    let doc = &drawing.document;
    let mut graph = Graph::new(doc, 10, 10);
    graph.length_unit();
    graph.tables();
    for (index, curve) in doc.composite_curves.iter().enumerate() {
        graph.composite_curve(index as i64 + 1, curve);
    }
    for name in sub_figure_order(doc) {
        if let Some(figure) = doc.sub_figures.get(&name) {
            graph.sub_figure(figure);
        }
    }
    let items: Vec<u64> = doc.sheet_members.iter().filter_map(|member| graph.member(member)).collect();
    let mut notices = NoticeList::new();
    if doc.sheet.is_none() {
        notices.add(
            NoticeLevel::Info,
            "no-sheet",
            "用紙のない図面は、要素を大きさ 0 の自由な用紙（FREE）に置いて書きました（P21 は用紙から要素を辿る）",
        );
    }
    let sheet = graph.sheet(doc.sheet.as_ref(), doc.title.as_ref(), items);
    graph.layers(sheet);
    graph.take_notices(&mut notices);
    let mut writer = StepWriter::default();
    let bytes = file(&header(drawing), &graph.instances, &mut writer);
    if writer.nonfinite > 0 {
        notices.add_count(NoticeLevel::Warning, "invalid-number", "数でない値を 0 にして書きました", writer.nonfinite);
    }
    SxfWritten { bytes, notices: notices.into_vec(), ids: graph.ids }
}
