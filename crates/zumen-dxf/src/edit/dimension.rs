//! 足す寸法の表示の図形（docs/decisions/0017）。DXF と DWG の編集が、寸法（DIMENSION）とその表示のブロック（*D…）に
//! 書く寸法線・寸法補助線・矢印・文字を、寸法スタイルの変数から求める。
//!
//! 変数の意味は DXF Reference「DIMSTYLE」と AutoCAD の System Variables（DIMSCALE、DIMASZ、DIMEXO、DIMEXE、DIMTXT、
//! DIMGAP、DIMTAD、DIMDEC、DIMZIN、DIMLFAC、DIMTSZ、DIMCLRD、DIMCLRE、DIMCLRT）。矢印は塗りつぶしの閉じた矢印
//! （長さ DIMASZ、幅はその 1/3）、DIMTSZ が 0 より大きければ斜線（ezdxf の ARCHITECTURAL_TICK と同じ形）。
//! 文字は寸法線の中央に置き、DIMTAD の配置と DIMGAP に従う。0 なら寸法線を文字の幅と DIMGAP だけ切る。

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
            "DIMCLRD" => self.colors[0] = value as i64,
            "DIMCLRE" => self.colors[1] = value as i64,
            "DIMCLRT" => self.colors[2] = value as i64,
            _ => {}
        }
    }

    /// DIMSTYLE の項目のグループコードと変数の名前（DXF Reference「DIMSTYLE」）
    pub const CODES: [(i32, &'static str); 14] = [
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

/// 寸法の種類
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DimensionKind {
    /// 回転寸法（寸法線の角度、度）
    Linear(f64),
    /// 平行寸法（寸法補助線の起点を結ぶ向き）
    Aligned,
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
    /// 定義点（10。寸法線と 2 本目の寸法補助線の交点）
    pub definition: [f64; 3],
    /// 文字の中点（11）
    pub text_midpoint: [f64; 3],
    pub measurement: f64,
    /// 表示の文字（<> を計測値にしたもの）
    pub text: String,
    pub parts: Vec<(Part, Role)>,
}

/// 計測値の文字。DIMLFAC を掛け、DIMDEC の桁に丸め、DIMZIN の 4（前の 0）・8（後ろの 0）を省く
pub fn format_measurement(value: f64, style: &DimStyleValues) -> String {
    let decimals = style.decimals.clamp(0, 8) as usize;
    let mut text = format!("{:.*}", decimals, value * style.linear_factor);
    if text.starts_with('-') && text.trim_start_matches(['-', '0', '.']).is_empty() {
        text.remove(0);
    }
    if style.zero_suppression & 8 != 0 && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if style.zero_suppression & 4 != 0 && (text.starts_with("0.") || text.starts_with("-0.")) {
        text = text.replacen("0.", ".", 1);
    }
    text
}

/// 文字の幅の見積もり（寸法線を切る長さに使う）。半角は高さの 0.7、全角は高さと同じ
fn text_width(text: &str, height: f64) -> f64 {
    text.chars().map(|c| if c.is_ascii() { 0.7 } else { 1.0 }).sum::<f64>() * height
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

/// 寸法の値と表示の図形を求める。text の <> は計測値（空も計測値）。寸法補助線の起点が向きの上で重なれば None
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
    // 文字の向き: 寸法線の向きを、読める向き（-90° より大きく 90° 以下）にする
    let along = unit(sub(second, first)).unwrap_or(direction);
    let mut angle = along[1].atan2(along[0]).to_degrees();
    let mut reading = along;
    if angle > 90.0 + 1e-9 || angle <= -90.0 + 1e-9 {
        angle = if angle > 0.0 { angle - 180.0 } else { angle + 180.0 };
        reading = scaled(along, -1.0);
    }
    let up = [-reading[1], reading[0], 0.0];
    let height = style.text_height * factor;
    let gap = style.gap.abs() * factor;
    let content = if text.is_empty() { "<>".to_string() } else { text.to_string() };
    let shown = content.replace("<>", &format_measurement(measurement, style));
    let middle = scaled(add(first, second), 0.5);
    // AutoCAD System Variables「DIMTAD」: 2 は定義点から遠い側、4 は下。3 の JIS 配置は上で近似し、呼び出し元が知らせる。
    let side = match style.text_above {
        0 => 0.0,
        2 if dot(sub(middle, start), up) < 0.0 => -1.0,
        4 => -1.0,
        _ => 1.0,
    };
    let text_midpoint = add(middle, scaled(up, side * (gap + height / 2.0)));
    // 寸法線（DIMTAD が 0 なら文字のところで切る）
    let half = measurement / 2.0;
    let cut = if style.text_above == 0 && !shown.trim().is_empty() {
        (text_width(&shown, height) / 2.0 + gap).min(half)
    } else {
        0.0
    };
    if cut > 0.0 {
        parts.push((Part::Line(first, add(middle, scaled(along, -cut))), Role::DimensionLine));
        parts.push((Part::Line(add(middle, scaled(along, cut)), second), Role::DimensionLine));
    } else {
        parts.push((Part::Line(first, second), Role::DimensionLine));
    }
    // 矢印（寸法線の内側から足を指す）か斜線
    for (tip, inward) in [(first, along), (second, scaled(along, -1.0))] {
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
    if !shown.trim().is_empty() {
        parts.push((Part::Text { position: text_midpoint, height, rotation: angle, text: shown.clone() }, Role::Text));
    }
    Some(DimensionLayout { definition: second, text_midpoint, measurement, text: shown, parts })
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
}
