//! 足す寸法の表示の図形（docs/decisions/0017）。DXF と DWG の編集が、寸法（DIMENSION）とその表示のブロック（*D…）に
//! 書く寸法線・寸法補助線・矢印・文字を、寸法スタイルの変数から求める。
//!
//! 変数の意味は DXF Reference「DIMSTYLE」と AutoCAD の System Variables（DIMSCALE、DIMASZ、DIMEXO、DIMEXE、DIMTXT、
//! DIMGAP、DIMTAD、DIMDEC、DIMZIN、DIMLFAC、DIMTSZ、DIMCLRD、DIMCLRE、DIMCLRT）。矢印は塗りつぶしの閉じた矢印
//! （長さ DIMASZ、幅はその 1/3）、DIMTSZ が 0 より大きければ斜線（ezdxf の ARCHITECTURAL_TICK と同じ形）。
//! 文字は寸法線の中央に置き、DIMTAD の配置と DIMGAP に従う。0 なら寸法線を文字の幅と DIMGAP だけ切る。

pub use zumen_core::edit::dimension::{DimensionInput, DimensionKind, KEYS};
use zumen_core::edit::{EditError, ValueReader};

/// 寸法スタイルの値（図面の単位。DIMSCALE を掛ける前）
#[derive(Debug, Clone, PartialEq)]
pub struct DimStyleValues {
    pub scale: f64,
    pub arrow_size: f64,
    pub extension_offset: f64,
    pub extension_extension: f64,
    pub text_height: f64,
    pub gap: f64,
    pub text_above: i64,
    pub decimals: i64,
    pub zero_suppression: i64,
    pub linear_factor: f64,
    pub tick_size: f64,
    /// 角度の小数の桁（DIMADEC。-1 は DIMDEC）と 0 の省略（DIMAZIN）、単位（DIMAUNIT。0 は 10 進の度）
    pub angle_decimals: i64,
    pub angle_zero_suppression: i64,
    pub angle_unit: i64,
    /// 寸法線・寸法補助線・文字の色（ACI。0 は BYBLOCK、256 は BYLAYER）
    pub colors: [i64; 3],
}

impl Default for DimStyleValues {
    /// AutoCAD の既定の値（インチの図面。DIMSTYLE の項目にない変数はこの値）
    fn default() -> Self {
        Self {
            scale: 1.0,
            arrow_size: 0.18,
            extension_offset: 0.0625,
            extension_extension: 0.18,
            text_height: 0.18,
            gap: 0.09,
            text_above: 0,
            decimals: 4,
            zero_suppression: 0,
            linear_factor: 1.0,
            tick_size: 0.0,
            angle_decimals: 0,
            angle_zero_suppression: 0,
            angle_unit: 0,
            colors: [0, 0, 0],
        }
    }
}

impl DimStyleValues {
    /// DXF の変数の名前（$ なし）で値を入れる。知らない名前は無視する
    pub fn set(&mut self, name: &str, value: f64) {
        if !value.is_finite() {
            return;
        }
        match name {
            "DIMSCALE" => self.scale = value,
            "DIMASZ" => self.arrow_size = value,
            "DIMEXO" => self.extension_offset = value,
            "DIMEXE" => self.extension_extension = value,
            "DIMTXT" => self.text_height = value,
            "DIMGAP" => self.gap = value,
            "DIMTAD" => self.text_above = value as i64,
            "DIMDEC" => self.decimals = value as i64,
            "DIMZIN" => self.zero_suppression = value as i64,
            "DIMLFAC" => self.linear_factor = value,
            "DIMTSZ" => self.tick_size = value,
            "DIMADEC" => self.angle_decimals = value as i64,
            "DIMAZIN" => self.angle_zero_suppression = value as i64,
            "DIMAUNIT" => self.angle_unit = value as i64,
            "DIMCLRD" => self.colors[0] = value as i64,
            "DIMCLRE" => self.colors[1] = value as i64,
            "DIMCLRT" => self.colors[2] = value as i64,
            _ => {}
        }
    }

    /// DIMSTYLE の項目のグループコードと変数の名前（DXF Reference「DIMSTYLE」）
    pub const CODES: [(i32, &'static str); 17] = [
        (40, "DIMSCALE"),
        (41, "DIMASZ"),
        (42, "DIMEXO"),
        (44, "DIMEXE"),
        (140, "DIMTXT"),
        (147, "DIMGAP"),
        (77, "DIMTAD"),
        (271, "DIMDEC"),
        (78, "DIMZIN"),
        (144, "DIMLFAC"),
        (142, "DIMTSZ"),
        (179, "DIMADEC"),
        (79, "DIMAZIN"),
        (275, "DIMAUNIT"),
        (176, "DIMCLRD"),
        (177, "DIMCLRE"),
        (178, "DIMCLRT"),
    ];

    /// 図面の尺度（DIMSCALE。0 は 1 として扱う）
    fn factor(&self) -> f64 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }
}

/// add-dimension-style の値のキー（docs/decisions/0017）。from は写す寸法スタイル、textStyle は DIMTXSTY
pub const STYLE_KEYS: [&str; 13] = [
    "from",
    "textStyle",
    "scale",
    "textHeight",
    "arrowSize",
    "extensionOffset",
    "extensionExtension",
    "textGap",
    "textAbove",
    "decimals",
    "suppressTrailingZeros",
    "tickSize",
    "linearFactor",
];

/// add-dimension-style の数の値と変数の名前
const STYLE_NUMBERS: [(&str, &str); 8] = [
    ("scale", "DIMSCALE"),
    ("textHeight", "DIMTXT"),
    ("arrowSize", "DIMASZ"),
    ("extensionOffset", "DIMEXO"),
    ("extensionExtension", "DIMEXE"),
    ("textGap", "DIMGAP"),
    ("tickSize", "DIMTSZ"),
    ("linearFactor", "DIMLFAC"),
];

/// add-dimension-style の値を、寸法の変数の名前と値にする（指定されたものだけ）。長さは 0 以上、decimals は 0〜8
pub fn style_variables(reader: &ValueReader<'_>, index: usize) -> Result<Vec<(&'static str, f64)>, EditError> {
    let mut out = Vec::new();
    for (key, variable) in STYLE_NUMBERS {
        if let Some(value) = reader.number(key)? {
            let valid = if key == "linearFactor" { value != 0.0 } else { value >= 0.0 };
            if !valid {
                return Err(EditError::at(
                    index,
                    "invalid-value",
                    format!("{key} は 0 以上（linearFactor は 0 以外）の数にしてください"),
                ));
            }
            out.push((variable, value));
        }
    }
    if let Some(above) = reader.bool("textAbove")? {
        out.push(("DIMTAD", f64::from(u8::from(above))));
    }
    if let Some(decimals) = reader.number("decimals")? {
        if !(0.0..=8.0).contains(&decimals) || decimals.fract() != 0.0 {
            return Err(EditError::at(index, "invalid-value", "decimals は 0〜8 の整数にしてください"));
        }
        out.push(("DIMDEC", decimals));
    }
    if let Some(suppress) = reader.bool("suppressTrailingZeros")? {
        out.push(("DIMZIN", if suppress { 8.0 } else { 0.0 }));
    }
    Ok(out)
}

/// 足す寸法の DXF・DWG の値（70 の種類と、表示のブロックの図形）
pub trait DxfDimension {
    /// DXF の 70 の種類（DXF Reference「DIMENSION」の 70: 0 回転、1 平行、3 直径、4 半径、5 3 点角度）に、表示の
    /// ブロックをこの寸法だけが使う印（32）を足したもの
    fn flags(&self) -> i64;
    /// 寸法の値と表示の図形。点が重なって寸法にならなければ、その理由
    fn layout(&self, text: &str, style: &DimStyleValues) -> Result<DimensionLayout, &'static str>;
}

impl DxfDimension for DimensionInput {
    fn flags(&self) -> i64 {
        32 + match self {
            Self::Linear { kind: DimensionKind::Linear(_), .. } => 0,
            Self::Linear { kind: DimensionKind::Aligned, .. } => 1,
            Self::Diameter { .. } => 3,
            Self::Radius { .. } => 4,
            Self::Angular { .. } => 5,
        }
    }

    fn layout(&self, text: &str, style: &DimStyleValues) -> Result<DimensionLayout, &'static str> {
        match *self {
            Self::Linear { kind, start, end, position } => layout(kind, start, end, position, text, style)
                .ok_or("start と end は寸法の向きで離れた点にしてください"),
            Self::Angular { center, start, end, position } => angular_layout(center, start, end, position, text, style)
                .ok_or("start・end・position は center から離し、start と end は違う向きにしてください"),
            Self::Radius { center, point } => {
                radial_layout(false, center, point, text, style).ok_or("point は center から離してください")
            }
            Self::Diameter { center, point } => {
                radial_layout(true, center, point, text, style).ok_or("point は center から離してください")
            }
        }
    }
}

/// 図形が使う寸法スタイルの色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    DimensionLine,
    ExtensionLine,
    Text,
}

/// 表示のブロックの図形（WCS、z は寸法の高さ）
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Line([f64; 3], [f64; 3]),
    /// 反時計回りの円弧（角度は度）
    Arc {
        center: [f64; 3],
        radius: f64,
        angle: [f64; 2],
    },
    /// 塗りつぶしの三角形（SOLID の 10・11・12。13 は 12 と同じ）
    Solid([[f64; 3]; 3]),
    /// 中央・中段に合わせた文字（MTEXT の 71 = 5、TEXT の 72 = 1・73 = 2）
    Text {
        position: [f64; 3],
        height: f64,
        rotation: f64,
        text: String,
    },
}

/// 寸法の値と表示の図形
#[derive(Debug, Clone, PartialEq)]
pub struct DimensionLayout {
    /// 定義点（10）。回転・平行寸法は寸法線と 2 本目の寸法補助線の交点、角度寸法は寸法の弧の上の点、半径寸法は
    /// 中心、直径寸法は point の反対の点
    pub definition: [f64; 3],
    /// 文字の中点（11）
    pub text_midpoint: [f64; 3],
    /// 計測値（42。角度寸法はラジアン）
    pub measurement: f64,
    /// 表示の文字（<> を計測値にしたもの）
    pub text: String,
    pub parts: Vec<(Part, Role)>,
}

/// 計測値の文字。DIMLFAC を掛け、DIMDEC の桁に丸め、DIMZIN の 4（前の 0）・8（後ろの 0）を省く
pub fn format_measurement(value: f64, style: &DimStyleValues) -> String {
    let decimals = style.decimals.clamp(0, 8) as usize;
    trim_number(
        format!("{:.*}", decimals, value * style.linear_factor),
        style.zero_suppression & 4 != 0,
        style.zero_suppression & 8 != 0,
    )
}

/// 角度の文字（度）。AutoCAD System Variables「DIMADEC」（-1 は DIMDEC）の桁に丸め、「DIMAZIN」の 1（前の 0）・
/// 2（後ろの 0）を省き、度の記号（%%d）を付ける
pub fn format_angle(degrees: f64, style: &DimStyleValues) -> String {
    let decimals = if style.angle_decimals < 0 { style.decimals } else { style.angle_decimals }.clamp(0, 8) as usize;
    let suppress = style.angle_zero_suppression;
    format!("{}%%d", trim_number(format!("{degrees:.decimals$}"), suppress & 1 != 0, suppress & 2 != 0))
}

fn trim_number(mut text: String, leading: bool, trailing: bool) -> String {
    if text.starts_with('-') && text.trim_start_matches(['-', '0', '.']).is_empty() {
        text.remove(0);
    }
    if trailing && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if leading && (text.starts_with("0.") || text.starts_with("-0.")) {
        text = text.replacen("0.", ".", 1);
    }
    text
}

/// 文字の幅の見積もり（寸法線を切る長さに使う）。半角は高さの 0.7、全角は高さと同じ。%% の記号は 1 文字に数える
fn text_width(text: &str, height: f64) -> f64 {
    let symbols = text.matches("%%").count() as f64 * 2.0 * 0.7;
    text.chars().map(|c| if c.is_ascii() { 0.7 } else { 1.0 }).sum::<f64>() * height - symbols * height
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scaled(v: [f64; 3], k: f64) -> [f64; 3] {
    [v[0] * k, v[1] * k, v[2] * k]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = v[0].hypot(v[1]);
    (length > 1e-12).then(|| [v[0] / length, v[1] / length, 0.0])
}

/// 寸法の文字。<> を計測値にする（空も計測値）
fn shown_text(text: &str, measured: &str) -> String {
    let content = if text.is_empty() { "<>" } else { text };
    content.replace("<>", measured)
}

/// 向き along の文字の角度（度）と読む向き。読める向き（-90° より大きく 90° 以下）にする
fn reading(along: [f64; 3]) -> (f64, [f64; 3]) {
    let angle = along[1].atan2(along[0]).to_degrees();
    if angle > 90.0 + 1e-9 || angle <= -90.0 + 1e-9 {
        (if angle > 0.0 { angle - 180.0 } else { angle + 180.0 }, scaled(along, -1.0))
    } else {
        (angle, along)
    }
}

/// 文字を寸法線のどちら側に置くか（1 は読む向きの上、-1 は下、0 は線の上）。AutoCAD System Variables「DIMTAD」:
/// 2 は定義点から遠い側（outside は up の向きで定義点から遠ければ正）、4 は下。3 の JIS 配置は上で近似し、
/// 呼び出し元が知らせる。
fn text_side(style: &DimStyleValues, outside: f64) -> f64 {
    match style.text_above {
        0 => 0.0,
        2 if outside < 0.0 => -1.0,
        4 => -1.0,
        _ => 1.0,
    }
}

/// 矢印（tip から inward の向きに伸びる）か斜線
fn end_mark(parts: &mut Vec<(Part, Role)>, tip: [f64; 3], inward: [f64; 3], style: &DimStyleValues) {
    let factor = style.factor();
    if style.tick_size > 0.0 {
        let size = style.tick_size * factor;
        let normal = [-inward[1], inward[0], 0.0];
        let slant = scaled(add(inward, normal), size / 2.0);
        parts.push((Part::Line(sub(tip, slant), add(tip, slant)), Role::DimensionLine));
    } else if style.arrow_size > 0.0 {
        let size = style.arrow_size * factor;
        let base = add(tip, scaled(inward, size));
        let side = scaled([-inward[1], inward[0], 0.0], size / 6.0);
        parts.push((Part::Solid([tip, add(base, side), sub(base, side)]), Role::DimensionLine));
    }
}

/// first から second への寸法線と、marks の端の矢印、中央の文字。reference は DIMTAD=2 で遠ざける点。文字の中点を返す
#[allow(clippy::too_many_arguments)]
fn dimension_line(
    parts: &mut Vec<(Part, Role)>,
    first: [f64; 3],
    second: [f64; 3],
    fallback: [f64; 3],
    reference: [f64; 3],
    shown: &str,
    marks: [bool; 2],
    style: &DimStyleValues,
) -> [f64; 3] {
    let factor = style.factor();
    let along = unit(sub(second, first)).unwrap_or(fallback);
    let length = dot(sub(second, first), along).abs();
    let (angle, reading) = reading(along);
    let up = [-reading[1], reading[0], 0.0];
    let height = style.text_height * factor;
    let gap = style.gap.abs() * factor;
    let middle = scaled(add(first, second), 0.5);
    let side = text_side(style, dot(sub(middle, reference), up));
    let text_midpoint = add(middle, scaled(up, side * (gap + height / 2.0)));
    // 寸法線（DIMTAD が 0 なら文字のところで切る）
    let half = length / 2.0;
    let cut = if style.text_above == 0 && !shown.trim().is_empty() {
        (text_width(shown, height) / 2.0 + gap).min(half)
    } else {
        0.0
    };
    if cut > 0.0 {
        parts.push((Part::Line(first, add(middle, scaled(along, -cut))), Role::DimensionLine));
        parts.push((Part::Line(add(middle, scaled(along, cut)), second), Role::DimensionLine));
    } else {
        parts.push((Part::Line(first, second), Role::DimensionLine));
    }
    // 矢印（寸法線の内側から端を指す）か斜線
    for ((tip, inward), mark) in [(first, along), (second, scaled(along, -1.0))].into_iter().zip(marks) {
        if mark {
            end_mark(parts, tip, inward, style);
        }
    }
    if !shown.trim().is_empty() {
        parts.push((
            Part::Text { position: text_midpoint, height, rotation: angle, text: shown.to_string() },
            Role::Text,
        ));
    }
    text_midpoint
}

/// 回転寸法・平行寸法の値と表示の図形を求める。text の <> は計測値（空も計測値）。寸法補助線の起点が向きの上で
/// 重なれば None
pub fn layout(
    kind: DimensionKind,
    start: [f64; 3],
    end: [f64; 3],
    position: [f64; 3],
    text: &str,
    style: &DimStyleValues,
) -> Option<DimensionLayout> {
    let z = position[2];
    let flat = |p: [f64; 3]| [p[0], p[1], z];
    let (start, end, position) = (flat(start), flat(end), flat(position));
    let direction = match kind {
        DimensionKind::Linear(angle) => {
            let radians = angle.to_radians();
            [radians.cos(), radians.sin(), 0.0]
        }
        DimensionKind::Aligned => unit(sub(end, start))?,
    };
    let measurement = dot(sub(end, start), direction).abs();
    if measurement <= 1e-12 {
        return None;
    }
    // 寸法線の上の、寸法補助線の足
    let foot = |p: [f64; 3]| add(position, scaled(direction, dot(sub(p, position), direction)));
    let (first, second) = (foot(start), foot(end));
    let factor = style.factor();
    let mut parts = Vec::new();
    // 寸法補助線: 起点から DIMEXO 離して始め、寸法線を DIMEXE 越える
    for (origin, at) in [(start, first), (end, second)] {
        if let Some(out) = unit(sub(at, origin)) {
            parts.push((
                Part::Line(
                    add(origin, scaled(out, style.extension_offset * factor)),
                    add(at, scaled(out, style.extension_extension * factor)),
                ),
                Role::ExtensionLine,
            ));
        }
    }
    let shown = shown_text(text, &format_measurement(measurement, style));
    let text_midpoint = dimension_line(&mut parts, first, second, direction, start, &shown, [true, true], style);
    Some(DimensionLayout { definition: second, text_midpoint, measurement, text: shown, parts })
}

/// 半径寸法・直径寸法の値と表示の図形。寸法線は中心から point まで（直径は反対の点から point まで）で、円の上の端に
/// 矢印を付ける。<> は R（直径は %%c）の付いた計測値
pub fn radial_layout(
    diameter: bool,
    center: [f64; 3],
    point: [f64; 3],
    text: &str,
    style: &DimStyleValues,
) -> Option<DimensionLayout> {
    let z = point[2];
    let (center, point) = ([center[0], center[1], z], [point[0], point[1], z]);
    let out = unit(sub(point, center))?;
    let radius = dot(sub(point, center), out);
    let opposite = sub(scaled(center, 2.0), point);
    let (from, measurement, prefix) = if diameter { (opposite, radius * 2.0, "%%c") } else { (center, radius, "R") };
    let shown = shown_text(text, &format!("{prefix}{}", format_measurement(measurement, style)));
    let mut parts = Vec::new();
    let text_midpoint = dimension_line(&mut parts, from, point, out, center, &shown, [diameter, true], style);
    Some(DimensionLayout { definition: from, text_midpoint, measurement, text: shown, parts })
}

/// 3 点角度寸法の値と表示の図形。寸法の弧の半径は center から position まで。寸法補助線は、弧の内側の起点から
/// DIMEXO 離して始め、弧を DIMEXE 越える（起点が弧の外なら、起点までの線が弧に届くので描かない）。弧の両端に矢印、
/// 中央に文字を置く
pub fn angular_layout(
    center: [f64; 3],
    start: [f64; 3],
    end: [f64; 3],
    position: [f64; 3],
    text: &str,
    style: &DimStyleValues,
) -> Option<DimensionLayout> {
    use std::f64::consts::TAU;
    let z = position[2];
    let flat = |p: [f64; 3]| [p[0], p[1], z];
    let (center, start, end, position) = (flat(center), flat(start), flat(end), flat(position));
    let first = unit(sub(start, center))?;
    let second = unit(sub(end, center))?;
    let toward = unit(sub(position, center))?;
    let radius = dot(sub(position, center), toward);
    let direction = |v: [f64; 3]| v[1].atan2(v[0]);
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(TAU);
    let (a, b) = (direction(first), direction(second));
    let span = ccw(a, b);
    if span <= 1e-9 || TAU - span <= 1e-9 {
        return None;
    }
    // 弧は反時計回りに from から span だけ。position の向きを含むほうの角を測る
    let (from, span) = if ccw(a, direction(toward)) <= span { (a, span) } else { (b, TAU - span) };
    let factor = style.factor();
    let at = |angle: f64| add(center, [radius * angle.cos(), radius * angle.sin(), 0.0]);
    let mut parts = Vec::new();
    for (origin, out) in [(start, first), (end, second)] {
        let (begin, finish) = (
            dot(sub(origin, center), out) + style.extension_offset * factor,
            radius + style.extension_extension * factor,
        );
        if begin < finish {
            parts.push((
                Part::Line(add(center, scaled(out, begin)), add(center, scaled(out, finish))),
                Role::ExtensionLine,
            ));
        }
    }
    let measurement = span;
    let shown = shown_text(text, &format_angle(span.to_degrees(), style));
    let height = style.text_height * factor;
    let gap = style.gap.abs() * factor;
    let middle_angle = from + span / 2.0;
    let middle = at(middle_angle);
    let tangent = [-middle_angle.sin(), middle_angle.cos(), 0.0];
    let (rotation, reading) = reading(tangent);
    let up = [-reading[1], reading[0], 0.0];
    let side = text_side(style, dot(sub(middle, center), up));
    let text_midpoint = add(middle, scaled(up, side * (gap + height / 2.0)));
    // 寸法の弧（DIMTAD が 0 なら文字のところで切る）
    let cut = if style.text_above == 0 && !shown.trim().is_empty() {
        ((text_width(&shown, height) / 2.0 + gap) / radius).min(span / 2.0)
    } else {
        0.0
    };
    let degrees = |from: f64, to: f64| [from.to_degrees().rem_euclid(360.0), to.to_degrees().rem_euclid(360.0)];
    if cut > 0.0 {
        parts.push((Part::Arc { center, radius, angle: degrees(from, middle_angle - cut) }, Role::DimensionLine));
        parts
            .push((Part::Arc { center, radius, angle: degrees(middle_angle + cut, from + span) }, Role::DimensionLine));
    } else {
        parts.push((Part::Arc { center, radius, angle: degrees(from, from + span) }, Role::DimensionLine));
    }
    // 矢印は弧の端で、弧に接する向きに内側へ
    let to = from + span;
    end_mark(&mut parts, at(from), [-from.sin(), from.cos(), 0.0], style);
    end_mark(&mut parts, at(to), [to.sin(), -to.cos(), 0.0], style);
    if !shown.trim().is_empty() {
        parts.push((Part::Text { position: text_midpoint, height, rotation, text: shown.clone() }, Role::Text));
    }
    Some(DimensionLayout { definition: position, text_midpoint, measurement, text: shown, parts })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric() -> DimStyleValues {
        let mut style = DimStyleValues::default();
        for (name, value) in [
            ("DIMASZ", 2.5),
            ("DIMEXO", 0.625),
            ("DIMEXE", 1.25),
            ("DIMTXT", 2.5),
            ("DIMGAP", 0.625),
            ("DIMTAD", 1.0),
            ("DIMDEC", 0.0),
            ("DIMSCALE", 100.0),
        ] {
            style.set(name, value);
        }
        style
    }

    #[test]
    fn lays_out_a_linear_dimension_above_its_line() {
        let style = metric();
        let layout = layout(
            DimensionKind::Linear(0.0),
            [0.0, 0.0, 0.0],
            [5000.0, 300.0, 0.0],
            [2000.0, -800.0, 0.0],
            "<>",
            &style,
        )
        .unwrap();
        assert_eq!(layout.measurement, 5000.0);
        assert_eq!(layout.text, "5000");
        assert_eq!(layout.definition, [5000.0, -800.0, 0.0]);
        // 文字は寸法線の DIMGAP と文字の高さの半分だけ上
        assert_eq!(layout.text_midpoint, [2500.0, -800.0 + 62.5 + 125.0, 0.0]);
        let lines: Vec<_> = layout.parts.iter().filter(|(part, _)| matches!(part, Part::Line(..))).collect();
        // 寸法補助線 2 本と寸法線 1 本
        assert_eq!(lines.len(), 3);
        assert_eq!(
            lines[0].0,
            Part::Line([0.0, -62.5, 0.0], [0.0, -925.0, 0.0]),
            "the first extension line starts DIMEXO away and passes the dimension line by DIMEXE"
        );
        let solids = layout.parts.iter().filter(|(part, _)| matches!(part, Part::Solid(_))).count();
        assert_eq!(solids, 2);
    }

    /// AutoCAD System Variables「DIMTAD」の 2（定義点から遠い側）・4（下）は、寸法線が上下のどちらでも配置を守る。
    #[test]
    fn positions_dimension_text_outside_or_below_the_line() {
        let mut style = metric();
        let offset = (style.gap + style.text_height / 2.0) * style.scale;
        for y in [-800.0_f64, 800.0] {
            for (placement, side) in [(2, y.signum()), (4, -1.0)] {
                style.text_above = placement;
                for (start, end) in [([0.0; 3], [5000.0, 0.0, 0.0]), ([5000.0, 0.0, 0.0], [0.0; 3])] {
                    let layout = layout(DimensionKind::Aligned, start, end, [0.0, y, 0.0], "<>", &style).unwrap();
                    assert_eq!(layout.text_midpoint, [2500.0, y + side * offset, 0.0], "DIMTAD={placement}, y={y}");
                }
            }
        }
    }

    #[test]
    fn aligned_dimensions_follow_their_points_and_read_upright() {
        let mut style = metric();
        style.set("DIMTAD", 0.0);
        let layout =
            layout(DimensionKind::Aligned, [3000.0, 3000.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], "W=<>", &style)
                .unwrap();
        assert!((layout.measurement - 3000.0 * 2f64.sqrt()).abs() < 1e-9);
        assert_eq!(layout.text, "W=4243");
        let Some((Part::Text { rotation, .. }, _)) =
            layout.parts.iter().find(|(part, _)| matches!(part, Part::Text { .. }))
        else {
            panic!()
        };
        assert!((rotation - 45.0).abs() < 1e-9, "{rotation}");
        // DIMTAD が 0 なので寸法線は文字のところで 2 本に切れる
        let dimension_lines =
            layout.parts.iter().filter(|(part, role)| matches!(part, Part::Line(..)) && *role == Role::DimensionLine);
        assert_eq!(dimension_lines.count(), 2);
    }

    #[test]
    fn formats_measurements_like_the_dimension_variables() {
        let mut style = DimStyleValues::default();
        assert_eq!(format_measurement(12.5, &style), "12.5000");
        style.set("DIMZIN", 8.0);
        assert_eq!(format_measurement(12.5, &style), "12.5");
        style.set("DIMZIN", 12.0);
        assert_eq!(format_measurement(0.25, &style), ".25");
        style.set("DIMDEC", 0.0);
        style.set("DIMLFAC", 0.001);
        assert_eq!(format_measurement(5000.0, &style), "5");
        assert!(layout(DimensionKind::Linear(90.0), [0.0; 3], [10.0, 0.0, 0.0], [0.0; 3], "", &style).is_none());
    }

    #[test]
    fn ticks_replace_arrows_when_their_size_is_set() {
        let mut style = metric();
        style.set("DIMTSZ", 1.0);
        let layout =
            layout(DimensionKind::Linear(0.0), [0.0; 3], [1000.0, 0.0, 0.0], [0.0, 500.0, 0.0], "", &style).unwrap();
        assert!(layout.parts.iter().all(|(part, _)| !matches!(part, Part::Solid(_))));
        assert!(layout.parts.contains(&(Part::Line([-50.0, 450.0, 0.0], [50.0, 550.0, 0.0]), Role::DimensionLine)));
    }

    fn text_of(layout: &DimensionLayout) -> (&str, [f64; 3], f64) {
        layout
            .parts
            .iter()
            .find_map(|(part, _)| match part {
                Part::Text { text, position, rotation, .. } => Some((text.as_str(), *position, *rotation)),
                _ => None,
            })
            .unwrap()
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6
    }

    /// 3 点角度寸法は position の向きを含む角を測り、弧・寸法補助線・矢印 2 つ・度の文字を描く
    #[test]
    fn angular_dimensions_measure_the_angle_that_holds_their_position() {
        let style = metric();
        let (center, start, end) = ([0.0; 3], [1000.0, 0.0, 0.0], [0.0, 1000.0, 0.0]);
        let inside = angular_layout(center, start, end, [1414.0, 1414.0, 0.0], "<>", &style).unwrap();
        assert!((inside.measurement - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(inside.text, "90%%d");
        assert_eq!(inside.definition, [1414.0, 1414.0, 0.0]);
        let radius = 1414.0_f64.hypot(1414.0);
        let arcs: Vec<_> = inside.parts.iter().filter(|(part, _)| matches!(part, Part::Arc { .. })).collect();
        assert_eq!(arcs.len(), 1);
        let Part::Arc { radius: drawn, angle, .. } = &arcs[0].0 else { panic!() };
        assert!((drawn - radius).abs() < 1e-9 && angle[0].abs() < 1e-9 && (angle[1] - 90.0).abs() < 1e-9, "{angle:?}");
        // 起点は弧の内側なので、寸法補助線は外へ向かい弧を DIMEXE 越える
        let extension = inside.parts.iter().find(|(_, role)| *role == Role::ExtensionLine).unwrap();
        let Part::Line(from, to) = extension.0 else { panic!() };
        assert!(close(from, [1062.5, 0.0, 0.0]) && close(to, [radius + 125.0, 0.0, 0.0]), "{from:?} {to:?}");
        assert_eq!(inside.parts.iter().filter(|(part, _)| matches!(part, Part::Solid(_))).count(), 2);
        // 文字は弧の中央の外側（読む向きの上）に、弧に沿って置く
        let (_, position, rotation) = text_of(&inside);
        let offset = radius + 62.5 + 125.0;
        let diagonal = std::f64::consts::FRAC_1_SQRT_2 * offset;
        assert!(close(position, [diagonal, diagonal, 0.0]), "{position:?}");
        assert!((rotation + 45.0).abs() < 1e-9, "{rotation}");
        let outside = angular_layout(center, start, end, [-500.0, -500.0, 0.0], "<>", &style).unwrap();
        assert_eq!(outside.text, "270%%d");
        assert!(outside.parts.iter().all(|(_, role)| *role != Role::ExtensionLine), "the arc lies inside both rays");
        assert!(angular_layout(center, start, [2000.0, 0.0, 0.0], [0.0, 1.0, 0.0], "", &style).is_none());
        assert!(angular_layout(center, center, end, [1.0, 1.0, 0.0], "", &style).is_none());
    }

    #[test]
    fn formats_angles_with_their_own_variables() {
        let mut style = DimStyleValues::default();
        assert_eq!(format_angle(45.0, &style), "45%%d");
        style.set("DIMADEC", 2.0);
        assert_eq!(format_angle(0.5, &style), "0.50%%d");
        style.set("DIMAZIN", 3.0);
        assert_eq!(format_angle(0.5, &style), ".5%%d");
        // -1 は DIMDEC（既定の 4 桁）
        style.set("DIMADEC", -1.0);
        style.set("DIMAZIN", 0.0);
        assert_eq!(format_angle(30.0, &style), "30.0000%%d");
    }

    /// 半径寸法は中心から円の上の点まで（矢印は円の上だけ）、直径寸法は反対の点から（両端に矢印）
    #[test]
    fn radial_dimensions_draw_from_the_center_or_across_the_circle() {
        let style = metric();
        let radius = radial_layout(false, [100.0, 100.0, 0.0], [100.0, 600.0, 0.0], "<>", &style).unwrap();
        assert_eq!((radius.measurement, radius.text.as_str()), (500.0, "R500"));
        assert_eq!(radius.definition, [100.0, 100.0, 0.0]);
        assert!(radius.parts.contains(&(Part::Line([100.0, 100.0, 0.0], [100.0, 600.0, 0.0]), Role::DimensionLine)));
        let solids: Vec<_> = radius
            .parts
            .iter()
            .filter_map(|(part, _)| match part {
                Part::Solid(corners) => Some(corners[0]),
                _ => None,
            })
            .collect();
        assert_eq!(solids, [[100.0, 600.0, 0.0]]);
        assert!(radius.parts.iter().all(|(_, role)| *role != Role::ExtensionLine));
        let diameter = radial_layout(true, [0.0; 3], [300.0, 400.0, 0.0], "φ=<>", &style).unwrap();
        assert_eq!((diameter.measurement, diameter.text.as_str()), (1000.0, "φ=%%c1000"));
        assert_eq!(diameter.definition, [-300.0, -400.0, 0.0]);
        assert_eq!(diameter.parts.iter().filter(|(part, _)| matches!(part, Part::Solid(_))).count(), 2);
        assert!(radial_layout(true, [0.0; 3], [0.0; 3], "", &style).is_none());
        // %%c は 1 文字に数えて寸法線を切る
        let mut cut = metric();
        cut.set("DIMTAD", 0.0);
        let diameter = radial_layout(true, [0.0; 3], [1000.0, 0.0, 0.0], "", &cut).unwrap();
        let first = diameter.parts.iter().find(|(_, role)| *role == Role::DimensionLine).unwrap();
        let half = (5.0 * 0.7 * 250.0) / 2.0 + 62.5;
        assert_eq!(first.0, Part::Line([-1000.0, 0.0, 0.0], [-half, 0.0, 0.0]));
    }
}
