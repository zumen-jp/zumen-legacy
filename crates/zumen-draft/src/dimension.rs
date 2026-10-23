//! 寸法。DIMENSION と、その表示のブロック（*D1 など: 寸法補助線・寸法線・端の記号・寸法の値）を描く。
//! 寸法を計算しない読み取り（zumen も）はブロックを描き、AutoCAD もブロックを描いて、寸法を編集したときに
//! 寸法スタイルから描き直す（DXF Reference「DIMENSION」、決定 0017）。
//!
//! 端の記号は、閉じた塗りの矢印（AutoCAD の既定と同じ幅 1/3）、斜線（建築の寸法でよく使う 45° の短い線）、
//! 黒丸のいずれか。寸法の値は寸法線の上に、読める向き（-90° より大きく 90° 以下）で置く。

use crate::spec::{DimensionChainSpec, DimensionLook, DimensionSpec, Point, StyleSpec};
use crate::text::{self, BOTTOM_CENTER, Placed};
use crate::{Ctx, DraftError, Drafter, MAX_PARTS, number, point, points, positive};
use zumen_convert::builder::{Look, Space, lineweight, xyz};
use zumen_dxf::{DxfDimStyle, DxfDimension, DxfEntityData};

/// 端の記号
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EndMark {
    Arrow,
    Tick,
    Dot,
    None,
}

impl EndMark {
    pub fn parse(name: Option<&str>, path: &str) -> Result<EndMark, DraftError> {
        Ok(match name.unwrap_or("arrow") {
            "arrow" => EndMark::Arrow,
            "tick" => EndMark::Tick,
            "dot" => EndMark::Dot,
            "none" => EndMark::None,
            other => {
                return Err(DraftError::new(
                    "invalid-value",
                    path,
                    format!("arrow の {other} はありません（arrow・tick・dot・none）"),
                ));
            }
        })
    }
}

/// 寸法の見た目（用紙の mm）
#[derive(Debug, Clone)]
pub(crate) struct DimStyle {
    pub arrow: EndMark,
    pub arrow_size: f64,
    pub text_height: f64,
    pub precision: u32,
    pub thousands: bool,
    pub gap: f64,
    pub beyond: f64,
}

/// 寸法の値と寸法線の隙間（用紙の mm）
pub(crate) const TEXT_GAP: f64 = 0.8;
/// 寸法線の位置を省略したときの、測る点からの距離（用紙の mm）
pub(crate) const DEFAULT_OFFSET: f64 = 10.0;
/// 全体の寸法を、並びの寸法からさらに離す距離（用紙の mm）
const TOTAL_OFFSET: f64 = 8.0;

pub(crate) fn style(drafter: &Drafter, look: &DimensionLook, path: &str) -> Result<DimStyle, DraftError> {
    let d = &drafter.defaults;
    let size = |value: Option<f64>, fallback: Option<f64>, default: f64, key: &str| -> Result<f64, DraftError> {
        positive(value.or(fallback).unwrap_or(default), key, path)
    };
    let text_height = size(look.text_height, d.dimension_text_height, drafter.text_height(), "textHeight")?;
    Ok(DimStyle {
        arrow: EndMark::parse(look.arrow.as_deref().or(d.arrow.as_deref()), path)?,
        arrow_size: size(look.arrow_size, d.arrow_size, 2.5, "arrowSize")?,
        text_height,
        precision: look.precision.or(d.precision).unwrap_or(0).min(8),
        thousands: look.thousands.or(d.thousands).unwrap_or(true),
        gap: look.extension_gap.map_or(Ok(1.0), |gap| number(gap, "extensionGap", path).map(f64::abs))?,
        beyond: look.extension_beyond.map_or(Ok(2.0), |value| number(value, "extensionBeyond", path).map(f64::abs))?,
    })
}

/// 数を桁数で丸め、3 桁ごとに区切る
pub(crate) fn format_number(value: f64, precision: u32, thousands: bool) -> String {
    let text = format!("{:.*}", precision as usize, value.abs());
    let (whole, fraction) = text.split_once('.').map_or((text.as_str(), None), |(w, f)| (w, Some(f)));
    let mut out = String::new();
    if value < 0.0 && text.chars().any(|c| c.is_ascii_digit() && c != '0') {
        out.push('-');
    }
    let digits: Vec<char> = whole.chars().collect();
    for (index, c) in digits.iter().enumerate() {
        if thousands && index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    if let Some(fraction) = fraction {
        out.push('.');
        out.push_str(fraction);
    }
    out
}

fn add(a: Point, b: Point) -> Point {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1]]
}

fn mul(a: Point, k: f64) -> Point {
    [a[0] * k, a[1] * k]
}

fn dot(a: Point, b: Point) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(degrees: f64) -> Point {
    let (sin, cos) = degrees.to_radians().sin_cos();
    [cos, sin]
}

/// 寸法スタイルの名前。縮尺と見た目ごとに作り、DIMSCALE を縮尺、矢印と文字の大きさを用紙の mm にする
fn dimension_style(drafter: &mut Drafter, scale: f64, style: &DimStyle) -> String {
    let key = style_key(scale, style);
    if let Some(name) = drafter.dimension_styles.get(&key) {
        return name.clone();
    }
    // 同じ縮尺で見た目の違う寸法スタイルには、番号を付ける
    let base = format!("ZUMEN-1-{}", crate::dimension::format_number(scale, 0, false));
    let same_scale = drafter.dimension_looks.values().filter(|(other, _)| *other == scale).count();
    let name = if same_scale == 0 { base } else { format!("{base}-{}", same_scale + 1) };
    drafter.dimension_looks.insert(name.clone(), (scale, style.clone()));
    let handle = drafter.builder.handle();
    drafter.builder.doc.dimstyles.push(DxfDimStyle {
        handle,
        name: name.clone(),
        dimscale: scale,
        dimasz: style.arrow_size,
        dimtxt: style.text_height,
    });
    drafter.dimension_styles.insert(key, name.clone());
    name
}

/// 縮尺と寸法の見た目を区別する鍵
fn style_key(scale: f64, style: &DimStyle) -> String {
    format!(
        "{}:{:?}:{}:{}:{}:{}:{}:{}",
        scale.to_bits(),
        style.arrow,
        style.arrow_size.to_bits(),
        style.text_height.to_bits(),
        style.precision,
        style.thousands,
        style.gap.to_bits(),
        style.beyond.to_bits()
    )
}

/// 端の記号を描く。tip は寸法線の端、inward は寸法線の内側への向き（単位ベクトル）
#[allow(clippy::too_many_arguments)]
pub(crate) fn end_mark(
    drafter: &mut Drafter,
    space: &Space,
    look: &Look,
    arrow: EndMark,
    size: f64,
    tip: Point,
    inward: Point,
    path: &str,
) {
    let normal = [-inward[1], inward[0]];
    match arrow {
        EndMark::Arrow => {
            let base = add(tip, mul(inward, size));
            let (left, right) = (add(base, mul(normal, size / 6.0)), add(base, mul(normal, -size / 6.0)));
            let data = DxfEntityData::Solid {
                vtx0: xyz(tip),
                vtx1: xyz(left),
                vtx2: xyz(right),
                vtx3: xyz(right),
                invisible_edges: None,
            };
            drafter.add(space, look, "SOLID", data, path);
        }
        EndMark::Tick => {
            let (sin, cos) = std::f64::consts::FRAC_PI_4.sin_cos();
            let slash = [inward[0] * cos - inward[1] * sin, inward[0] * sin + inward[1] * cos];
            let half = mul(slash, size / 2.0);
            let look = Look { lineweight: lineweight(0.35).0, ..look.clone() };
            let data = DxfEntityData::Line { start: xyz(sub(tip, half)), end: xyz(add(tip, half)) };
            drafter.add(space, &look, "LINE", data, path);
        }
        EndMark::Dot => crate::symbols::dot(drafter, space, look, tip, size * 0.4, path),
        EndMark::None => {}
    }
}

/// 向き along の線の中点 middle の上（読む向きの上）に、読める向き（-90° より大きく 90° 以下）で寸法の値を置く。
/// raw は TEXT の文字列（%% の制御コードを含んでよい）。文字の中点を返す
#[allow(clippy::too_many_arguments)]
fn put_value(
    drafter: &mut Drafter,
    inner: &Space,
    style: &DimStyle,
    s: f64,
    middle: Point,
    along: Point,
    raw: String,
    path: &str,
) -> Point {
    let mut reading = along[1].atan2(along[0]).to_degrees();
    if reading > 90.0 + 1e-9 {
        reading -= 180.0;
    } else if reading <= -90.0 + 1e-9 {
        reading += 180.0;
    }
    let up = unit(reading + 90.0);
    let height = style.text_height * s;
    let at = add(middle, mul(up, TEXT_GAP * s));
    let text_style = drafter.text_style(None);
    let placed = Placed {
        at,
        lines: &[],
        height,
        align: BOTTOM_CENTER,
        rotation: reading,
        width_factor: 1.0,
        line_spacing: 1.0,
        style: text_style,
    };
    text::put_line(drafter, inner, &Look::by_block(), placed, raw, path);
    add(at, mul(up, height / 2.0))
}

/// 寸法の文字の TEXT の文字列。text の <> を value（制御コードを含む TEXT の文字列）にし、ほかはそのまま書く
fn value_text(text: Option<&str>, value: &str) -> String {
    match text {
        Some(text) => text.split("<>").map(text::escape_text).collect::<Vec<_>>().join(value),
        None => value.to_string(),
    }
}

/// 利用者の文字（DIMENSION の 1）。<> だけなら空にし、AutoCAD が描き直すときも測った値を書かせる
fn user_text(text: Option<&str>) -> String {
    match text {
        Some(text) if text != "<>" => text::escape_text(text),
        _ => String::new(),
    }
}

/// 1 つの寸法の値
struct Linear<'a> {
    from: Point,
    to: Point,
    /// 寸法線の向き（度）
    angle: f64,
    /// 寸法線が通る点
    through: Point,
    aligned: bool,
    text: Option<&'a str>,
}

/// 寸法を描き、DIMENSION のハンドルを返す
fn linear(drafter: &mut Drafter, ctx: &Ctx, look: &Look, style: &DimStyle, dim: Linear<'_>, path: &str) -> String {
    let u = unit(dim.angle);
    let n = [-u[1], u[0]];
    let project = |p: Point| add(p, mul(n, dot(sub(dim.through, p), n)));
    let (q1, q2) = (project(dim.from), project(dim.to));
    let measured = dot(sub(dim.to, dim.from), u).abs();
    let block = drafter.builder.dimension_block();
    let inner = Space::Block(block.clone());
    // 表示のブロックの図形は、寸法の色・線種・線の太さを継ぐ（BYBLOCK）
    let by_block = Look::by_block();
    let s = ctx.scale;
    // 寸法補助線
    for (p, q) in [(dim.from, q1), (dim.to, q2)] {
        let distance = dot(sub(dim.through, p), n);
        if distance.abs() <= 1e-9 {
            continue;
        }
        let side = distance.signum();
        let start = if style.gap * s < distance.abs() { add(p, mul(n, side * style.gap * s)) } else { p };
        let end = add(q, mul(n, side * style.beyond * s));
        drafter.add(&inner, &by_block, "LINE", DxfEntityData::Line { start: xyz(start), end: xyz(end) }, path);
    }
    // 寸法線と端の記号
    drafter.add(&inner, &by_block, "LINE", DxfEntityData::Line { start: xyz(q1), end: xyz(q2) }, path);
    let length = sub(q2, q1);
    let span = length[0].hypot(length[1]);
    let e = if span > 1e-9 { mul(length, 1.0 / span) } else { u };
    end_mark(drafter, &inner, &by_block, style.arrow, style.arrow_size * s, q1, e, path);
    end_mark(drafter, &inner, &by_block, style.arrow, style.arrow_size * s, q2, mul(e, -1.0), path);
    let value = format_number(measured, style.precision, style.thousands);
    let shown = match dim.text {
        Some(text) => text.replace("<>", &value),
        None => value,
    };
    let text_midpoint = put_value(drafter, &inner, style, s, mul(add(q1, q2), 0.5), e, text::escape_text(&shown), path);
    let dimstyle = dimension_style(drafter, s, style);
    let dimension = DxfDimension {
        anonymous_block_name: block,
        // 下位 3 ビットが 0 回転・1 平行、32 は表示のブロックがこの寸法だけのもの（DXF Reference「DIMENSION」の 70）
        dimtype: if dim.aligned { 33.0 } else { 32.0 },
        defpoint: xyz(q2),
        defpoint2: Some(xyz(dim.from)),
        defpoint3: Some(xyz(dim.to)),
        defpoint4: None,
        defpoint5: None,
        text_midpoint: xyz(text_midpoint),
        insert: None,
        // 測った値のまま（<> と同じ）なら空にし、AutoCAD が描き直すときも測った値を書かせる
        text: match dim.text {
            Some(text) if text != "<>" => shown,
            _ => String::new(),
        },
        actual_measurement: Some(measured),
        attachment_point: Some(5.0),
        text_rotation: None,
        horizontal_direction: None,
        angle: (!dim.aligned).then(|| dim.angle.rem_euclid(360.0)),
        oblique_angle: None,
        dimstyle,
    };
    drafter.add(&ctx.space, look, "DIMENSION", DxfEntityData::Dimension(dimension), path)
}

/// 2 点の向きを、水平か垂直にそろえた角度
fn axis_angle(from: Point, to: Point) -> f64 {
    let d = sub(to, from);
    if d[0].abs() >= d[1].abs() { 0.0 } else { 90.0 }
}

/// 寸法線が通る点。at か、from から offset（用紙の mm、from → to の左が正）
fn through(
    ctx: &Ctx,
    from: Point,
    to: Point,
    angle: f64,
    at: Option<Point>,
    offset: Option<f64>,
    path: &str,
) -> Result<Point, DraftError> {
    if let Some(at) = at {
        return point(at, path);
    }
    let u = unit(angle);
    let n = [-u[1], u[0]];
    let side = if dot(sub(to, from), u) >= 0.0 { 1.0 } else { -1.0 };
    let offset = number(offset.unwrap_or(DEFAULT_OFFSET), "offset", path)?;
    Ok(add(from, mul(n, side * ctx.size(offset))))
}

pub(crate) fn draw(
    drafter: &mut Drafter,
    ctx: &Ctx,
    item_style: &StyleSpec,
    spec: &DimensionSpec,
    path: &str,
) -> Result<(), DraftError> {
    let default_layer = drafter.defaults.dimension_layer.clone();
    let look = drafter.look(ctx, item_style, default_layer.as_deref(), path)?;
    let style = style(drafter, &spec.look(), path)?;
    let kind = spec.kind.as_deref().unwrap_or("linear");
    let used: &[&str] = match kind {
        "linear" => &["from", "to", "offset", "at", "angle"],
        "aligned" => &["from", "to", "offset", "at"],
        "angular" => &["center", "from", "to", "at", "radius"],
        "radius" | "diameter" => &["center", "radius", "angle"],
        other => {
            return Err(DraftError::new(
                "invalid-value",
                path,
                format!("kind の {other} はありません（linear・aligned・angular・radius・diameter）"),
            ));
        }
    };
    let given = [
        ("from", spec.from.is_some()),
        ("to", spec.to.is_some()),
        ("center", spec.center.is_some()),
        ("radius", spec.radius.is_some()),
        ("offset", spec.offset.is_some()),
        ("at", spec.at.is_some()),
        ("angle", spec.angle.is_some()),
    ];
    if let Some((key, _)) = given.iter().find(|(key, given)| *given && !used.contains(key)) {
        return Err(DraftError::new("unknown-key", path, format!("{kind} の寸法に {key} は指定できません")));
    }
    let required = |value: Option<Point>, key: &str| -> Result<Point, DraftError> {
        point(
            value
                .ok_or_else(|| DraftError::new("invalid-spec", path, format!("{kind} の寸法には {key} が要ります")))?,
            path,
        )
    };
    match kind {
        "angular" => {
            let (center, from, to) =
                (required(spec.center, "center")?, required(spec.from, "from")?, required(spec.to, "to")?);
            let at = spec.at.map(|at| point(at, path)).transpose()?;
            let radius = spec.radius.map(|radius| positive(radius, "radius", path)).transpose()?;
            if at.is_some() && radius.is_some() {
                return Err(DraftError::new(
                    "invalid-value",
                    path,
                    "angular の寸法の弧は at か radius のどちらかで指定してください",
                ));
            }
            let dim = Angular { center, from, to, at, radius, text: spec.text.as_deref() };
            return angular(drafter, ctx, &look, &style, dim, path);
        }
        "radius" | "diameter" => {
            let center = required(spec.center, "center")?;
            let radius = positive(
                spec.radius.ok_or_else(|| {
                    DraftError::new("invalid-spec", path, format!("{kind} の寸法には radius が要ります"))
                })?,
                "radius",
                path,
            )?;
            let angle = number(spec.angle.unwrap_or(45.0), "angle", path)?;
            radial(drafter, ctx, &look, &style, kind == "diameter", center, radius, angle, spec.text.as_deref(), path);
            return Ok(());
        }
        _ => {}
    }
    let (from, to) = (required(spec.from, "from")?, required(spec.to, "to")?);
    let aligned = kind == "aligned";
    let angle = if aligned {
        let d = sub(to, from);
        if d[0].hypot(d[1]) <= 0.0 {
            return Err(DraftError::new("invalid-value", path, "aligned の寸法の 2 点が同じです"));
        }
        d[1].atan2(d[0]).to_degrees()
    } else {
        match spec.angle {
            Some(angle) => number(angle, "angle", path)?,
            None => axis_angle(from, to),
        }
    };
    let through = through(ctx, from, to, angle, spec.at, spec.offset, path)?;
    let dim = Linear { from, to, angle, through, aligned, text: spec.text.as_deref() };
    linear(drafter, ctx, &look, &style, dim, path);
    Ok(())
}

/// 角度寸法の値
struct Angular<'a> {
    center: Point,
    from: Point,
    to: Point,
    /// 寸法の弧の上の点。その向きを含む角を測る
    at: Option<Point>,
    /// 寸法の弧の半径。at もなければ、center から from・to の近いほうまでの半分
    radius: Option<f64>,
    text: Option<&'a str>,
}

/// 3 点角度寸法を描く（DXF Reference「DIMENSION」の 70 の 5、13・14 は辺の上の点、15 は頂点、10 は弧の上の点）。
/// 角は at の向きを含むほう、at がなければ from から反時計回りに to まで。寸法補助線は弧の内側の点からだけ描く
/// （外の点は、その点までの線がもう弧に届いている）
fn angular(
    drafter: &mut Drafter,
    ctx: &Ctx,
    look: &Look,
    style: &DimStyle,
    dim: Angular<'_>,
    path: &str,
) -> Result<(), DraftError> {
    use std::f64::consts::TAU;
    let length = |p: Point| sub(p, dim.center)[0].hypot(sub(p, dim.center)[1]);
    let (near_from, near_to) = (length(dim.from), length(dim.to));
    if near_from <= 1e-9 || near_to <= 1e-9 {
        return Err(DraftError::new("invalid-value", path, "angular の寸法の from・to は center から離してください"));
    }
    let direction = |p: Point| {
        let d = sub(p, dim.center);
        d[1].atan2(d[0])
    };
    let ccw = |from: f64, to: f64| (to - from).rem_euclid(TAU);
    let (a, b) = (direction(dim.from), direction(dim.to));
    let mut span = ccw(a, b);
    if span <= 1e-9 || TAU - span <= 1e-9 {
        return Err(DraftError::new("invalid-value", path, "angular の寸法の from と to は違う向きにしてください"));
    }
    let mut start = a;
    let radius = match dim.at {
        Some(at) => {
            let radius = length(at);
            if radius <= 1e-9 {
                return Err(DraftError::new("invalid-value", path, "angular の寸法の at は center から離してください"));
            }
            if ccw(a, direction(at)) > span {
                (start, span) = (b, TAU - span);
            }
            radius
        }
        None => dim.radius.unwrap_or(near_from.min(near_to) / 2.0),
    };
    let s = ctx.scale;
    let block = drafter.builder.dimension_block();
    let inner = Space::Block(block.clone());
    let by_block = Look::by_block();
    let at_angle = |angle: f64| add(dim.center, mul([angle.cos(), angle.sin()], radius));
    // 寸法補助線
    for (p, distance) in [(dim.from, near_from), (dim.to, near_to)] {
        if distance >= radius {
            continue;
        }
        let u = mul(sub(p, dim.center), 1.0 / distance);
        let begin = if style.gap * s < radius - distance { distance + style.gap * s } else { distance };
        let (begin, end) = (add(dim.center, mul(u, begin)), add(dim.center, mul(u, radius + style.beyond * s)));
        drafter.add(&inner, &by_block, "LINE", DxfEntityData::Line { start: xyz(begin), end: xyz(end) }, path);
    }
    // 寸法の弧と端の記号（弧に接する向きに内側へ）
    let end = start + span;
    let arc = DxfEntityData::Arc {
        center: xyz(dim.center),
        radius,
        start_angle: start.to_degrees().rem_euclid(360.0),
        end_angle: end.to_degrees().rem_euclid(360.0),
    };
    drafter.add(&inner, &by_block, "ARC", arc, path);
    let size = style.arrow_size * s;
    end_mark(drafter, &inner, &by_block, style.arrow, size, at_angle(start), [-start.sin(), start.cos()], path);
    end_mark(drafter, &inner, &by_block, style.arrow, size, at_angle(end), [end.sin(), -end.cos()], path);
    // 寸法の値（度）は弧の中央に、弧に沿って置く
    let middle = start + span / 2.0;
    let value = format!("{}%%d", format_number(span.to_degrees(), style.precision, false));
    let raw = value_text(dim.text, &value);
    let tangent = [-middle.sin(), middle.cos()];
    let text_midpoint = put_value(drafter, &inner, style, s, at_angle(middle), tangent, raw, path);
    let dimstyle = dimension_style(drafter, s, style);
    let dimension = DxfDimension {
        anonymous_block_name: block,
        dimtype: 37.0,
        defpoint: xyz(at_angle(middle)),
        defpoint2: Some(xyz(dim.from)),
        defpoint3: Some(xyz(dim.to)),
        defpoint4: Some(xyz(dim.center)),
        defpoint5: None,
        text_midpoint: xyz(text_midpoint),
        insert: None,
        text: user_text(dim.text),
        // 角度寸法の計測値はラジアン
        actual_measurement: Some(span),
        attachment_point: Some(5.0),
        text_rotation: None,
        horizontal_direction: None,
        angle: None,
        oblique_angle: None,
        dimstyle,
    };
    drafter.add(&ctx.space, look, "DIMENSION", DxfEntityData::Dimension(dimension), path);
    Ok(())
}

/// 半径寸法（70 の 4: 10 は中心、15 は円の上の点）か直径寸法（70 の 3: 10 と 15 は円の上の向かい合う点）を描く。
/// 寸法線は angle の向きに、中心（直径は向かいの点）から円の上の点まで引き、円の上の端に端の記号を付ける。値の前に
/// R か直径の記号（%%c）を付ける
#[allow(clippy::too_many_arguments)]
fn radial(
    drafter: &mut Drafter,
    ctx: &Ctx,
    look: &Look,
    style: &DimStyle,
    diameter: bool,
    center: Point,
    radius: f64,
    angle: f64,
    text: Option<&str>,
    path: &str,
) {
    let u = unit(angle);
    let on_circle = add(center, mul(u, radius));
    let first = if diameter { sub(center, mul(u, radius)) } else { center };
    let s = ctx.scale;
    let block = drafter.builder.dimension_block();
    let inner = Space::Block(block.clone());
    let by_block = Look::by_block();
    drafter.add(&inner, &by_block, "LINE", DxfEntityData::Line { start: xyz(first), end: xyz(on_circle) }, path);
    let size = style.arrow_size * s;
    end_mark(drafter, &inner, &by_block, style.arrow, size, on_circle, mul(u, -1.0), path);
    if diameter {
        end_mark(drafter, &inner, &by_block, style.arrow, size, first, u, path);
    }
    let measured = if diameter { radius * 2.0 } else { radius };
    let prefix = if diameter { "%%c" } else { "R" };
    let value = format!("{prefix}{}", text::escape_text(&format_number(measured, style.precision, style.thousands)));
    let raw = value_text(text, &value);
    let text_midpoint = put_value(drafter, &inner, style, s, mul(add(first, on_circle), 0.5), u, raw, path);
    let dimstyle = dimension_style(drafter, s, style);
    let dimension = DxfDimension {
        anonymous_block_name: block,
        dimtype: if diameter { 35.0 } else { 36.0 },
        defpoint: xyz(first),
        defpoint2: None,
        defpoint3: None,
        defpoint4: Some(xyz(on_circle)),
        defpoint5: None,
        text_midpoint: xyz(text_midpoint),
        insert: None,
        text: user_text(text),
        actual_measurement: Some(measured),
        attachment_point: Some(5.0),
        text_rotation: None,
        horizontal_direction: None,
        angle: None,
        oblique_angle: None,
        dimstyle,
    };
    drafter.add(&ctx.space, look, "DIMENSION", DxfEntityData::Dimension(dimension), path);
}

/// 寸法の並びを描く。points は順に測る点で、隣り合う 2 点ごとに寸法を描く
#[allow(clippy::too_many_arguments)]
pub(crate) fn chain(
    drafter: &mut Drafter,
    ctx: &Ctx,
    look: &Look,
    style: &DimStyle,
    points: &[Point],
    angle: f64,
    through: Point,
    total: Option<f64>,
    path: &str,
) {
    for pair in points.windows(2) {
        let dim = Linear { from: pair[0], to: pair[1], angle, through, aligned: false, text: None };
        linear(drafter, ctx, look, style, dim, path);
    }
    if let (Some(extra), Some(first), Some(last)) = (total, points.first(), points.last())
        && points.len() > 2
    {
        let u = unit(angle);
        let n = [-u[1], u[0]];
        let side = dot(sub(through, *first), n);
        let side = if side.abs() > 1e-9 { side.signum() } else { 1.0 };
        let outer = add(through, mul(n, side * ctx.size(extra)));
        let dim = Linear { from: *first, to: *last, angle, through: outer, aligned: false, text: None };
        linear(drafter, ctx, look, style, dim, path);
    }
}

pub(crate) fn draw_chain(
    drafter: &mut Drafter,
    ctx: &Ctx,
    item_style: &StyleSpec,
    spec: &DimensionChainSpec,
    path: &str,
) -> Result<(), DraftError> {
    let default_layer = drafter.defaults.dimension_layer.clone();
    let look = drafter.look(ctx, item_style, default_layer.as_deref(), path)?;
    let style = style(drafter, &spec.look(), path)?;
    let points = points(&spec.points, 2, path)?;
    if points.len() > MAX_PARTS / 8 {
        return Err(DraftError::new("invalid-value", path, "寸法の並びの点が多すぎます"));
    }
    let (first, last) = (points[0], points[points.len() - 1]);
    let angle = match spec.angle {
        Some(angle) => number(angle, "angle", path)?,
        None => axis_angle(first, last),
    };
    let through = through(ctx, first, last, angle, spec.at, spec.offset, path)?;
    // 全体の寸法は total か totalOffset を指定したときに描く
    let total = if spec.total.unwrap_or(spec.total_offset.is_some()) {
        Some(number(spec.total_offset.unwrap_or(TOTAL_OFFSET), "totalOffset", path)?)
    } else {
        None
    };
    chain(drafter, ctx, &look, &style, &points, angle, through, total, path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers_with_separators() {
        assert_eq!(format_number(6400.0, 0, true), "6,400");
        assert_eq!(format_number(1029.184, 2, true), "1,029.18");
        assert_eq!(format_number(950.0, 0, true), "950");
        assert_eq!(format_number(123456789.0, 0, false), "123456789");
        assert_eq!(format_number(-0.0001, 0, true), "0");
        assert_eq!(format_number(-1500.0, 0, true), "-1,500");
    }
}
