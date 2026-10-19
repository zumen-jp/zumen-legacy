//! 図形（20.4「OBJECT PRESCRIPTIONS」）を AutoCAD の図面のモデルの図形にする。値は DXF に書いたときと同じ形にそろえる:
//! DWG のラジアンの角度は DXF と同じ度に、表・ブロック・スタイルのハンドルは名前にする

use crate::bits::{Eof, Read};
use crate::objects::{Color, EntityCommon, Object, Streams, entity_common};
use std::collections::HashMap;
use zumen_dxf::*;

/// ラジアンを DXF の度に
pub(crate) fn deg(radians: f64) -> f64 {
    radians * 180.0 / std::f64::consts::PI
}

/// ハンドルを DXF と同じ 16 進の大文字に
pub(crate) fn hex(handle: u64) -> String {
    format!("{handle:X}")
}

/// 表・スタイル・ブロックの名前
#[derive(Debug, Default)]
pub(crate) struct Names {
    pub layers: HashMap<u64, String>,
    pub linetypes: HashMap<u64, String>,
    pub styles: HashMap<u64, String>,
    pub dimstyles: HashMap<u64, String>,
    /// BLOCK_HEADER のハンドル → ブロックの名前
    pub blocks: HashMap<u64, String>,
    pub mlinestyles: HashMap<u64, String>,
    /// 寸法スタイルの DIMASZ と DIMSCALE
    pub arrows: HashMap<u64, (f64, f64)>,
    /// 色見本の色（DBCOLOR）
    pub book_colors: HashMap<u64, crate::objects::Color>,
    /// 線種のフラグ 01・10 の名前（ヘッダ変数の BYBLOCK・CONTINUOUS の線種）
    pub byblock: String,
    pub continuous: String,
}

impl Names {
    fn style(&self, handle: u64) -> String {
        self.styles.get(&handle).cloned().unwrap_or_else(|| "STANDARD".into())
    }

    fn dimstyle(&self, handle: u64) -> String {
        self.dimstyles.get(&handle).cloned().unwrap_or_else(|| "STANDARD".into())
    }

    /// 寸法スタイルの矢印の大きさ × DIMSCALE（0 の DIMSCALE は 1）
    fn arrow_size(&self, handle: u64) -> f64 {
        self.arrows.get(&handle).map_or(0.0, |(size, scale)| size * if *scale == 0.0 { 1.0 } else { *scale })
    }
}

/// 図形に続く図形（POLYLINE の頂点、INSERT の属性）。R2004 以降は持っている図形の並び、それより前は最初と最後
#[derive(Debug, Clone, Default)]
pub(crate) struct Children {
    pub owned: Vec<u64>,
    pub first_last: Option<(u64, u64)>,
}

/// 頂点（VERTEX）の値
#[derive(Debug, Clone, Copy)]
pub(crate) struct Vertex {
    pub flags: f64,
    pub point: [f64; 3],
    pub widths: Option<[f64; 2]>,
    pub bulge: f64,
}

/// 読んだ図形の中身。オブジェクトごとに 1 つ作ってすぐに取り出すので、大きい図形も箱に入れない
#[allow(clippy::large_enum_variant)]
pub(crate) enum Body {
    Entity(DxfEntityData),
    Vertex(Vertex),
    /// ポリフェースメッシュの面（頂点の番号）
    Face([f64; 4]),
}

pub(crate) struct Decoded {
    pub common: EntityCommon,
    pub body: Body,
    pub thickness: f64,
    pub extrusion: [f64; 3],
    pub children: Option<Children>,
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];

/// 図形を読む。kind は DXF の図形の種類（固定の型の番号かクラスの DXF 名から決めた名前）
pub(crate) fn read_entity(object: &mut Object<'_>, kind: &str, names: &Names) -> Read<Decoded> {
    let common = entity_common(object)?;
    let handle = object.handle;
    let s = &mut object.streams;
    let r2000 = true;
    let mut thickness = 0.0;
    let mut extrusion = Z;
    let mut children = None;
    let body = match kind {
        "LINE" => {
            let z_zero = s.data.bit()?;
            let x0 = s.data.rd()?;
            let x1 = s.data.dd(x0)?;
            let y0 = s.data.rd()?;
            let y1 = s.data.dd(y0)?;
            let (mut z0, mut z1) = (0.0, 0.0);
            if !z_zero {
                z0 = s.data.rd()?;
                z1 = s.data.dd(z0)?;
            }
            thickness = s.data.bt(r2000)?;
            extrusion = s.data.be(r2000)?;
            Body::Entity(DxfEntityData::Line { start: [x0, y0, z0], end: [x1, y1, z1] })
        }
        "CIRCLE" | "ARC" => {
            let center = s.data.bd3()?;
            let radius = s.data.bd()?;
            thickness = s.data.bt(r2000)?;
            extrusion = s.data.be(r2000)?;
            if kind == "ARC" {
                let start_angle = deg(s.data.bd()?);
                let end_angle = deg(s.data.bd()?);
                Body::Entity(DxfEntityData::Arc { center, radius, start_angle, end_angle })
            } else {
                Body::Entity(DxfEntityData::Circle { center, radius })
            }
        }
        "POINT" => {
            let location = s.data.bd3()?;
            thickness = s.data.bt(r2000)?;
            extrusion = s.data.be(r2000)?;
            let x_axis_angle = deg(s.data.bd()?);
            Body::Entity(DxfEntityData::Point { location, x_axis_angle })
        }
        "ELLIPSE" => {
            let center = s.data.bd3()?;
            let major_axis = s.data.bd3()?;
            extrusion = s.data.bd3()?;
            let ratio = s.data.bd()?;
            let start_param = s.data.bd()?;
            let end_param = s.data.bd()?;
            Body::Entity(DxfEntityData::Ellipse { center, major_axis, ratio, start_param, end_param })
        }
        "3DFACE" => {
            let no_flags = s.data.bit()?;
            let z_zero = s.data.bit()?;
            let x = s.data.rd()?;
            let y = s.data.rd()?;
            let z = if z_zero { 0.0 } else { s.data.rd()? };
            let vtx0 = [x, y, z];
            let vtx1 = s.data.dd3(vtx0)?;
            let vtx2 = s.data.dd3(vtx1)?;
            let vtx3 = s.data.dd3(vtx2)?;
            let invisible = if no_flags { 0.0 } else { f64::from(s.data.bs()?) };
            Body::Entity(DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges: Some(invisible) })
        }
        "SOLID" | "TRACE" => {
            thickness = s.data.bt(r2000)?;
            let elevation = s.data.bd()?;
            let mut corners = [[0.0; 3]; 4];
            for corner in &mut corners {
                let [x, y] = s.data.rd2()?;
                *corner = [x, y, elevation];
            }
            extrusion = s.data.be(r2000)?;
            let [vtx0, vtx1, vtx2, vtx3] = corners;
            Body::Entity(DxfEntityData::Solid { vtx0, vtx1, vtx2, vtx3, invisible_edges: None })
        }
        "RAY" | "XLINE" => {
            let start = s.data.bd3()?;
            let unit_vector = s.data.bd3()?;
            Body::Entity(DxfEntityData::XLine { start, unit_vector })
        }
        "LWPOLYLINE" => {
            let (polyline, t, e) = lwpolyline(s)?;
            thickness = t;
            extrusion = e;
            Body::Entity(DxfEntityData::LwPolyline(polyline))
        }
        "SPLINE" => Body::Entity(DxfEntityData::Spline(spline(s)?)),
        "TEXT" => {
            let (text, t, e) = text(s, names)?;
            thickness = t;
            extrusion = e;
            Body::Entity(DxfEntityData::Text(text))
        }
        "ATTRIB" | "ATTDEF" => {
            let (text, t, e) = text(s, names)?;
            thickness = t;
            extrusion = e;
            let (tag, attribute_flags, mtext) = attribute(s, handle, names)?;
            if kind == "ATTDEF" {
                if s.format.r2010() {
                    s.data.rc()?;
                }
                let prompt = s.tv()?;
                Body::Entity(DxfEntityData::Attdef { text, tag, prompt, attribute_flags, mtext })
            } else {
                Body::Entity(DxfEntityData::Attrib { text, tag, attribute_flags, mtext })
            }
        }
        "MTEXT" => {
            let (mtext, e) = mtext(s, names)?;
            extrusion = e;
            Body::Entity(DxfEntityData::MText(mtext))
        }
        "INSERT" | "MINSERT" | "ACAD_TABLE" => {
            let (insert, e, owned) = insert(s, kind == "MINSERT", names)?;
            extrusion = e;
            children = owned;
            if kind == "ACAD_TABLE" {
                // 表は INSERT と同じ頭（ブロックの名前と位置）だけを読む（DXF の読み取りと同じ）
                Body::Entity(DxfEntityData::Table(DxfTable {
                    name: insert.name,
                    insert: insert.insert,
                    xscale: insert.xscale,
                    yscale: insert.yscale,
                    zscale: insert.zscale,
                    rotation: insert.rotation,
                }))
            } else {
                Body::Entity(DxfEntityData::Insert(insert))
            }
        }
        "POLYLINE_2D" | "POLYLINE_3D" | "POLYLINE_PFACE" | "POLYLINE_MESH" => {
            let (polyline, t, e, owned) = polyline(s, kind)?;
            thickness = t;
            extrusion = e;
            children = Some(owned);
            Body::Entity(DxfEntityData::Polyline(polyline))
        }
        "VERTEX_2D" => {
            let flags = f64::from(s.data.rc()?);
            let point = s.data.bd3()?;
            let start = s.data.bd()?;
            let widths = if start < 0.0 { [-start, -start] } else { [start, s.data.bd()?] };
            let bulge = s.data.bd()?;
            if s.format.r2010() {
                s.data.bl()?;
            }
            s.data.bd()?;
            Body::Vertex(Vertex { flags, point, widths: Some(widths), bulge })
        }
        "VERTEX_3D" | "VERTEX_MESH" | "VERTEX_PFACE" => {
            let flags = f64::from(s.data.rc()?);
            let point = s.data.bd3()?;
            Body::Vertex(Vertex { flags, point, widths: None, bulge: 0.0 })
        }
        "VERTEX_PFACE_FACE" => {
            let mut face = [0.0; 4];
            for index in &mut face {
                *index = f64::from(s.data.bs()?);
            }
            Body::Face(face)
        }
        "DIMENSION_ORDINATE"
        | "DIMENSION_LINEAR"
        | "DIMENSION_ALIGNED"
        | "DIMENSION_ANG3PT"
        | "DIMENSION_ANG2LN"
        | "DIMENSION_RADIUS"
        | "DIMENSION_DIAMETER"
        | "ARC_DIMENSION"
        | "LARGE_RADIAL_DIMENSION" => {
            let (dimension, e) = dimension(s, kind, names)?;
            extrusion = e;
            Body::Entity(DxfEntityData::Dimension(dimension))
        }
        "LEADER" => {
            let (leader, e) = leader(s, names)?;
            extrusion = e;
            Body::Entity(DxfEntityData::Leader(leader))
        }
        "HATCH" => {
            let (hatch, e) = hatch(s)?;
            extrusion = e;
            Body::Entity(DxfEntityData::Hatch(hatch))
        }
        "VIEWPORT" => Body::Entity(DxfEntityData::Viewport(viewport(s)?)),
        "IMAGE" | "WIPEOUT" => Body::Entity(DxfEntityData::Image(image(s)?)),
        "MLINE" => {
            let (mline, e) = mline(s, names)?;
            extrusion = e;
            Body::Entity(DxfEntityData::MLine(mline))
        }
        "MULTILEADER" => Body::Entity(DxfEntityData::MLeader(Box::new(crate::mleader::mleader(s)?))),
        _ => return Err(Eof),
    };
    Ok(Decoded { common, body, thickness, extrusion, children })
}

/// LWPOLYLINE（20.4.85）。フラグ（BS）の 4 幅、8 標高、2 厚さ、1 押し出し、16 膨らみ、1024 頂点の番号、32 幅の並び、
/// 512 閉じる、256 線種を続ける
fn lwpolyline(s: &mut Streams<'_>) -> Read<(DxfLwPolyline, f64, [f64; 3])> {
    let flags = s.data.bs()?;
    let const_width = if flags & 4 != 0 { s.data.bd()? } else { 0.0 };
    let elevation = if flags & 8 != 0 { s.data.bd()? } else { 0.0 };
    let thickness = if flags & 2 != 0 { s.data.bd()? } else { 0.0 };
    let extrusion = if flags & 1 != 0 { s.data.bd3()? } else { Z };
    let count = count(s.data.bl()?)?;
    let bulge_count = if flags & 16 != 0 { count_of(s.data.bl()?)? } else { 0 };
    let id_count = if flags & 1024 != 0 { count_of(s.data.bl()?)? } else { 0 };
    let width_count = if flags & 32 != 0 { count_of(s.data.bl()?)? } else { 0 };
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(count.min(1 << 16));
    let mut last = [0.0, 0.0];
    for index in 0..count {
        last = if index == 0 { s.data.rd2()? } else { s.data.dd2(last)? };
        points.push([last[0], last[1], elevation]);
    }
    let mut bulges = vec![0.0; points.len()];
    for index in 0..bulge_count {
        let bulge = s.data.bd()?;
        if let Some(slot) = bulges.get_mut(index) {
            *slot = bulge;
        }
    }
    for _ in 0..id_count {
        s.data.bl()?;
    }
    let mut widths = vec![[0.0, 0.0]; points.len()];
    for index in 0..width_count {
        let pair = [s.data.bd()?, s.data.bd()?];
        if let Some(slot) = widths.get_mut(index) {
            *slot = pair;
        }
    }
    // DXF の 70 は 1 閉じる、128 線種を続ける
    let closed = flags & 512 != 0;
    let dxf_flags = f64::from(i16::from(closed) | if flags & 256 != 0 { 128 } else { 0 });
    let polyline = DxfLwPolyline {
        vertices: DxfVertexList { points, bulges, widths, closed, flags: dxf_flags },
        const_width: (const_width != 0.0).then_some(const_width),
        elevation: (elevation != 0.0).then_some(elevation),
    };
    Ok((polyline, thickness, extrusion))
}

/// 数（BL）。負や大きすぎる数は読めない印にする（残りのビットより多くは読めない）
fn count(value: i32) -> Read<usize> {
    count_of(value)
}

fn count_of(value: i32) -> Read<usize> {
    if !(0..=10_000_000).contains(&value) { Err(Eof) } else { Ok(value as usize) }
}

/// SPLINE（20.4.40）。scenario 1 は制御点と節点、2 は当てはめ点。R2013 以降は節点の取り方で scenario が決まる
fn spline(s: &mut Streams<'_>) -> Read<DxfSpline> {
    let mut scenario = s.data.bl()?;
    let mut closed = false;
    let mut knot_parameter = 0;
    if s.format.r2013() {
        let flags1 = s.data.bl()?;
        closed = flags1 & 4 != 0;
        knot_parameter = s.data.bl()?;
        scenario = if knot_parameter == 15 || flags1 & 8 == 0 { 1 } else { 2 };
    }
    let degree = f64::from(s.data.bl()?);
    let (mut rational, mut periodic) = (false, false);
    let (mut knots, mut controls, mut fits) = (0, 0, 0);
    let mut weighted = false;
    let mut start_tangent = None;
    let mut end_tangent = None;
    if scenario == 2 {
        s.data.bd()?;
        start_tangent = Some(s.data.bd3()?);
        end_tangent = Some(s.data.bd3()?);
        fits = count(s.data.bl()?)?;
    } else {
        rational = s.data.bit()?;
        closed = s.data.bit()? || closed;
        periodic = s.data.bit()?;
        s.data.bd()?;
        s.data.bd()?;
        knots = count(s.data.bl()?)?;
        controls = count(s.data.bl()?)?;
        weighted = s.data.bit()?;
    }
    let mut spline = DxfSpline {
        degree,
        flags: 0.0,
        closed,
        periodic,
        rational,
        knots: Vec::new(),
        control_points: Vec::new(),
        weights: Vec::new(),
        fit_points: Vec::new(),
        start_tangent: None,
        end_tangent: None,
    };
    for _ in 0..knots {
        spline.knots.push(s.data.bd()?);
    }
    for _ in 0..controls {
        spline.control_points.push(s.data.bd3()?);
        if weighted {
            spline.weights.push(s.data.bd()?);
        }
    }
    for _ in 0..fits {
        spline.fit_points.push(s.data.bd3()?);
    }
    // 接線は 0 でなければ DXF の 12・13 として持つ
    let nonzero = |v: &[f64; 3]| v.iter().any(|c| *c != 0.0);
    spline.start_tangent = start_tangent.filter(nonzero);
    spline.end_tangent = end_tangent.filter(nonzero);
    // 当てはめ点だけの 3 次のスプライン（R2013 以降に当てはめ点で作ったもの）は、AutoCAD が DXF に書くのと同じく
    // 制御点と節点を求める
    if scenario == 2
        && degree == 3.0
        && !closed
        && let (Some(d0), Some(dn)) = (spline.start_tangent, spline.end_tangent)
        && let Some((knots, controls)) = interpolate(&spline.fit_points, d0, dn, knot_parameter)
    {
        spline.knots = knots;
        spline.control_points = controls;
    }
    // DXF の 70: 1 閉じる、2 周期、4 有理、8 平面
    spline.flags = f64::from(i32::from(closed) | (i32::from(periodic) << 1) | (i32::from(rational) << 2));
    Ok(spline)
}

/// 当てはめ点 Q0〜Qn を通り、両端の微分が d0・dn の 3 次の B スプライン（The NURBS Book 9.2.2・9.2.3 の端の微分を
/// 与えた大域補間）。媒介変数は弦の長さ（knot_parameter 0）、その平方根（1）、等間隔（2）で 0〜1 に取り、内側の節点は
/// 媒介変数そのもの。DWG の接線は 0〜1 の媒介変数での微分として使う（2 点の見本で AutoCAD の DXF の制御点と同じになる。推定）
fn interpolate(fit: &[[f64; 3]], d0: [f64; 3], dn: [f64; 3], knot_parameter: i32) -> Option<(Vec<f64>, Vec<[f64; 3]>)> {
    let n = fit.len().checked_sub(1).filter(|n| *n >= 1)?;
    let mut lengths = Vec::with_capacity(n);
    for k in 1..=n {
        let chord =
            zumen_core::js::hypot(&[fit[k][0] - fit[k - 1][0], fit[k][1] - fit[k - 1][1], fit[k][2] - fit[k - 1][2]]);
        lengths.push(match knot_parameter {
            1 => chord.sqrt(),
            2 => 1.0,
            _ => chord,
        });
    }
    let total: f64 = lengths.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let mut params = vec![0.0; n + 1];
    for k in 1..n {
        params[k] = params[k - 1] + lengths[k - 1] / total;
    }
    params[n] = 1.0;
    let mut knots = vec![0.0; 4];
    knots.extend_from_slice(&params[1..n]);
    knots.extend_from_slice(&[1.0; 4]);
    let along =
        |p: [f64; 3], d: [f64; 3], factor: f64| [p[0] + d[0] * factor, p[1] + d[1] * factor, p[2] + d[2] * factor];
    let mut controls = vec![[0.0; 3]; n + 3];
    controls[0] = fit[0];
    controls[1] = along(fit[0], d0, params[1] / 3.0);
    controls[n + 1] = along(fit[n], dn, -(1.0 - params[n - 1]) / 3.0);
    controls[n + 2] = fit[n];
    // 内側の点 Qk（k = 1〜n−1）で N_k・N_k+1・N_k+2 だけが 0 でない。P2〜Pn を三重対角の連立方程式で解く
    if n >= 2 {
        let rows = n - 1;
        let mut lower = vec![0.0; rows];
        let mut diagonal = vec![0.0; rows];
        let mut upper = vec![0.0; rows];
        let mut right = vec![[0.0; 3]; rows];
        for row in 0..rows {
            let k = row + 1;
            let basis = cubic_basis(&knots, k + 3, params[k]);
            let mut value = fit[k];
            if k == 1 {
                for axis in 0..3 {
                    value[axis] -= basis[0] * controls[1][axis];
                }
            } else {
                lower[row] = basis[0];
            }
            diagonal[row] = basis[1];
            if k + 2 == n + 1 {
                for axis in 0..3 {
                    value[axis] -= basis[2] * controls[n + 1][axis];
                }
            } else {
                upper[row] = basis[2];
            }
            right[row] = value;
        }
        // トーマス法
        for row in 1..rows {
            if diagonal[row - 1] == 0.0 {
                return None;
            }
            let factor = lower[row] / diagonal[row - 1];
            diagonal[row] -= factor * upper[row - 1];
            let previous = right[row - 1];
            for (value, previous) in right[row].iter_mut().zip(previous) {
                *value -= factor * previous;
            }
        }
        for row in (0..rows).rev() {
            if diagonal[row] == 0.0 {
                return None;
            }
            let mut value = right[row];
            if row + 1 < rows {
                for axis in 0..3 {
                    value[axis] -= upper[row] * controls[row + 3][axis];
                }
            }
            controls[row + 2] = value.map(|v| v / diagonal[row]);
        }
    }
    controls.iter().all(|p| p.iter().all(|v| v.is_finite())).then_some((knots, controls))
}

/// 節点 knots の span（knots[span] ≤ u < knots[span + 1]）での 3 次の基底関数 N_span−3〜N_span のうち、
/// 前の 3 つ（The NURBS Book A2.2 BasisFuns）
fn cubic_basis(knots: &[f64], span: usize, u: f64) -> [f64; 3] {
    let mut values = [1.0, 0.0, 0.0, 0.0];
    let mut left = [0.0; 4];
    let mut right = [0.0; 4];
    for j in 1..=3 {
        left[j] = u - knots[span + 1 - j];
        right[j] = knots[span + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let denominator = right[r + 1] + left[j - r];
            let temp = if denominator == 0.0 { 0.0 } else { values[r] / denominator };
            values[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        values[j] = saved;
    }
    [values[0], values[1], values[2]]
}

/// TEXT・ATTRIB・ATTDEF に共通する値（20.4.3）。R2000 以降は DataFlags のビットが立った値を省く
fn text(s: &mut Streams<'_>, names: &Names) -> Read<(DxfTextBase, f64, [f64; 3])> {
    let flags = s.data.rc()?;
    let elevation = if flags & 0x01 == 0 { s.data.rd()? } else { 0.0 };
    let [x, y] = s.data.rd2()?;
    let align_point = if flags & 0x02 == 0 { Some(s.data.dd2([x, y])?) } else { None };
    let extrusion = s.data.be(true)?;
    let thickness = s.data.bt(true)?;
    let oblique = if flags & 0x04 == 0 { deg(s.data.rd()?) } else { 0.0 };
    let rotation = if flags & 0x08 == 0 { deg(s.data.rd()?) } else { 0.0 };
    let height = s.data.rd()?;
    let width = if flags & 0x10 == 0 { s.data.rd()? } else { 1.0 };
    let value = s.tv()?;
    let generation = if flags & 0x20 == 0 { f64::from(s.data.bs()?) } else { 0.0 };
    let halign = if flags & 0x40 == 0 { f64::from(s.data.bs()?) } else { 0.0 };
    let valign = if flags & 0x80 == 0 { f64::from(s.data.bs()?) } else { 0.0 };
    let style = s.handle()?.value;
    let text = DxfTextBase {
        text: decode_escapes(&value),
        insert: [x, y, elevation],
        align_point: align_point.map(|[ax, ay]| [ax, ay, elevation]),
        height,
        rotation,
        oblique,
        width: if width == 0.0 || width.is_nan() { 1.0 } else { width },
        style: names.style(style),
        halign,
        valign,
        text_generation_flag: generation,
    };
    Ok((text, thickness, extrusion))
}

/// ATTRIB・ATTDEF の値（20.4.4）。R2018 以降の複数行の属性（種類 2・4）は、文字を埋め込みの MTEXT に持つ
fn attribute(s: &mut Streams<'_>, handle: u64, names: &Names) -> Read<(String, f64, Option<Box<DxfMText>>)> {
    if s.format.r2010() {
        s.data.rc()?;
    }
    let mut mtext = None;
    if s.format.r2018() {
        let kind = s.data.rc()?;
        if kind == 2 || kind == 4 {
            // 埋め込みの MTEXT: 図形のモード（BB）からの共通の値と MTEXT の値、注釈の値。色・レイヤ・押し出し方向は
            // DXF の読み取りと同じく属性のものを使う
            crate::objects::embedded_entity_common(s, handle)?;
            mtext = Some(Box::new(mtext_fields(s, names)?.0));
            let size = s.data.bs()?;
            if size > 0 {
                s.data.skip(u64::from(size as u16) * 8)?;
                s.handle()?;
                s.data.bs()?;
            }
        }
    }
    let tag = decode_escapes(&s.tv()?);
    s.data.bs()?;
    let flags = f64::from(s.data.rc()?);
    if s.format.r2007() {
        s.data.bit()?;
    }
    Ok((tag, flags, mtext))
}

/// MTEXT（20.4.46）
fn mtext(s: &mut Streams<'_>, names: &Names) -> Read<(DxfMText, [f64; 3])> {
    mtext_fields(s, names)
}

fn mtext_fields(s: &mut Streams<'_>, names: &Names) -> Read<(DxfMText, [f64; 3])> {
    let insert = s.data.bd3()?;
    let extrusion = s.data.bd3()?;
    let direction = s.data.bd3()?;
    let rect_width = s.data.bd()?;
    if s.format.r2007() {
        s.data.bd()?;
    }
    let char_height = s.data.bd()?;
    let attachment_point = f64::from(s.data.bs()?);
    let drawing_direction = f64::from(s.data.bs()?);
    s.data.bd()?;
    s.data.bd()?;
    let raw = s.tv()?;
    let style = s.handle()?.value;
    let line_spacing_style = f64::from(s.data.bs()?);
    let line_spacing_factor = s.data.bd()?;
    s.data.bit()?;
    let mut background_flags = 0.0;
    let mut background: Option<(f64, Color)> = None;
    if s.format.r2004() {
        let flags = s.data.bl()?;
        background_flags = f64::from(flags);
        if flags & 1 != 0 || (s.format.r2018() && flags & 0x10 != 0) {
            let scale = s.data.bd()?;
            let color = s.cmc()?;
            s.data.bl()?;
            background = Some((scale, color));
        }
    }
    if s.format.r2018() {
        // 「Is NOT annotative」が 1 の（注釈でない）MTEXT は、同じ値（REDUNDANT FIELDS）と段組みの値を続ける。
        // 逆に読むと、続く値（複数行の属性のタグ・フラグ）がずれる
        if s.data.bit()? {
            s.data.bs()?;
            s.data.bit()?;
            s.handle()?;
            s.data.bl()?;
            s.data.bd3()?;
            s.data.bd3()?;
            for _ in 0..4 {
                s.data.bd()?;
            }
            let columns = s.data.bs()?;
            if columns != 0 {
                let heights = s.data.bl()?;
                s.data.bd()?;
                s.data.bd()?;
                let auto_height = s.data.bit()?;
                s.data.bit()?;
                if !auto_height && columns == 2 {
                    for _ in 0..count(heights)? {
                        s.data.bd()?;
                    }
                }
            }
        }
    }
    let mtext = DxfMText {
        text: mtext_plain_text(&raw),
        raw_text: raw,
        insert,
        char_height,
        rect_width,
        attachment_point,
        drawing_direction,
        // DXF は向きが (1, 0, 0) のとき 11 を書かない
        text_direction: (direction != [1.0, 0.0, 0.0]).then_some(direction),
        rotation: 0.0,
        style: names.style(style),
        line_spacing_style,
        line_spacing_factor,
        background_flags,
        background_color_index: background.map(|(_, color)| color.index),
        background_true_color: background.and_then(|(_, color)| color.rgb),
        background_scale: background.map(|(scale, _)| scale),
    };
    Ok((mtext, extrusion))
}

/// INSERT・MINSERT（20.4.9、20.4.10）と ACAD_TABLE の頭。属性の図形を返す
fn insert(s: &mut Streams<'_>, minsert: bool, names: &Names) -> Read<(DxfInsert, [f64; 3], Option<Children>)> {
    let point = s.data.bd3()?;
    let (xscale, yscale, zscale) = match s.data.bb()? {
        0 => {
            let x = s.data.rd()?;
            (x, s.data.dd(x)?, s.data.dd(x)?)
        }
        1 => (1.0, s.data.dd(1.0)?, s.data.dd(1.0)?),
        2 => {
            let x = s.data.rd()?;
            (x, x, x)
        }
        _ => (1.0, 1.0, 1.0),
    };
    let rotation = deg(s.data.bd()?);
    let extrusion = s.data.bd3()?;
    let has_attribs = s.data.bit()?;
    let owned_count = if s.format.r2004() && has_attribs { count(s.data.bl()?)? } else { 0 };
    let (mut columns, mut rows, mut column_spacing, mut row_spacing) = (1.0, 1.0, 0.0, 0.0);
    if minsert {
        columns = f64::from(s.data.bs()?);
        rows = f64::from(s.data.bs()?);
        column_spacing = s.data.bd()?;
        row_spacing = s.data.bd()?;
    }
    let block = s.handle()?.value;
    let mut children = None;
    if has_attribs {
        let mut list = Children::default();
        if s.format.r2004() {
            for _ in 0..owned_count {
                list.owned.push(s.handle()?.value);
            }
        } else {
            list.first_last = Some((s.handle()?.value, s.handle()?.value));
        }
        s.handle()?;
        children = Some(list);
    }
    let insert = DxfInsert {
        name: names.blocks.get(&block).cloned().unwrap_or_default(),
        insert: point,
        xscale,
        yscale,
        zscale,
        rotation,
        column_count: columns.max(1.0),
        row_count: rows.max(1.0),
        column_spacing,
        row_spacing,
        attribs: Vec::new(),
    };
    Ok((insert, extrusion, children))
}

/// POLYLINE（2D・3D・ポリフェース・ポリゴンメッシュ、20.4.16、20.4.17、20.4.33、20.4.34）。頂点は後で読む
fn polyline(s: &mut Streams<'_>, kind: &str) -> Read<(DxfPolyline, f64, [f64; 3], Children)> {
    let mut thickness = 0.0;
    let mut extrusion = Z;
    let mut polyline = DxfPolyline {
        vertices: DxfVertexList::default(),
        vertex_flags: Vec::new(),
        faces: None,
        m_count: None,
        n_count: None,
    };
    let flags: i32;
    match kind {
        "POLYLINE_2D" => {
            flags = i32::from(s.data.bs()?);
            s.data.bs()?;
            let start = s.data.bd()?;
            let end = s.data.bd()?;
            thickness = s.data.bt(true)?;
            let elevation = s.data.bd()?;
            extrusion = s.data.be(true)?;
            // 既定の幅と標高は頂点に移す（DXF の読み取りと同じ）
            polyline.vertices.widths = vec![[start, end]];
            polyline.vertices.points = vec![[0.0, 0.0, elevation]];
        }
        "POLYLINE_3D" => {
            let spline = s.data.rc()?;
            let closed = s.data.rc()? & 1 != 0;
            flags = 8 | i32::from(closed) | if spline & 3 != 0 { 4 } else { 0 };
        }
        "POLYLINE_PFACE" => {
            s.data.bs()?;
            s.data.bs()?;
            flags = 64;
        }
        _ => {
            flags = i32::from(s.data.bs()?);
            s.data.bs()?;
            polyline.m_count = Some(f64::from(s.data.bs()?));
            polyline.n_count = Some(f64::from(s.data.bs()?));
            s.data.bs()?;
            s.data.bs()?;
        }
    }
    let mut children = Children::default();
    if s.format.r2004() {
        let owned = count(s.data.bl()?)?;
        for _ in 0..owned {
            children.owned.push(s.handle()?.value);
        }
    } else {
        children.first_last = Some((s.handle()?.value, s.handle()?.value));
    }
    s.handle()?;
    polyline.vertices.flags = f64::from(flags);
    polyline.vertices.closed = flags & 1 != 0;
    if flags & 16 != 0 && polyline.m_count.is_none() {
        polyline.m_count = Some(0.0);
        polyline.n_count = Some(0.0);
    }
    Ok((polyline, thickness, extrusion, children))
}

/// 寸法（20.4.22〜20.4.30）。DXF の 70 は種類（0 回転、1 平行、2 角度、3 直径、4 半径、5 3 点角度、6 座標）と、
/// Flags 1 の 2 ビット目（32）、0 ビット目の反対（128）、座標寸法の X（64）
fn dimension(s: &mut Streams<'_>, kind: &str, names: &Names) -> Read<(DxfDimension, [f64; 3])> {
    if s.format.r2010() {
        s.data.rc()?;
    }
    let extrusion = s.data.bd3()?;
    let [mx, my] = s.data.rd2()?;
    let elevation = s.data.bd()?;
    let flags1 = s.data.rc()?;
    let text = s.tv()?;
    let text_rotation = deg(s.data.bd()?);
    let horizontal_direction = deg(s.data.bd()?);
    // 挿入の尺度と回転（DXF には出ない）
    s.data.bd3()?;
    s.data.bd()?;
    let attachment_point = f64::from(s.data.bs()?);
    s.data.bs()?;
    s.data.bd()?;
    let actual_measurement = s.data.bd()?;
    if s.format.r2007() {
        s.data.bit()?;
        s.data.bit()?;
        s.data.bit()?;
    }
    let [ix, iy] = s.data.rd2()?;
    let mut dimension = DxfDimension {
        anonymous_block_name: String::new(),
        dimtype: 0.0,
        defpoint: [0.0; 3],
        defpoint2: None,
        defpoint3: None,
        defpoint4: None,
        defpoint5: None,
        text_midpoint: [mx, my, elevation],
        // DXF は 12 が原点のとき書かない
        insert: (ix != 0.0 || iy != 0.0 || elevation != 0.0).then_some([ix, iy, elevation]),
        text: decode_escapes(&text),
        actual_measurement: Some(actual_measurement),
        attachment_point: Some(attachment_point),
        text_rotation: (text_rotation != 0.0).then_some(text_rotation),
        horizontal_direction: (horizontal_direction != 0.0).then_some(horizontal_direction),
        angle: None,
        oblique_angle: None,
        dimstyle: String::new(),
    };
    let mut dimtype: i32;
    match kind {
        "DIMENSION_LINEAR" | "DIMENSION_ALIGNED" => {
            dimension.defpoint2 = Some(s.data.bd3()?);
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimension.defpoint = s.data.bd3()?;
            // 寸法補助線の角度（52）と回転（50）は、DXF と同じく 0 でないときだけ持つ
            let oblique = deg(s.data.bd()?);
            dimension.oblique_angle = (oblique != 0.0).then_some(oblique);
            if kind == "DIMENSION_LINEAR" {
                let angle = deg(s.data.bd()?);
                dimension.angle = (angle != 0.0).then_some(angle);
                dimtype = 0;
            } else {
                dimtype = 1;
            }
        }
        "DIMENSION_ORDINATE" => {
            dimension.defpoint = s.data.bd3()?;
            dimension.defpoint2 = Some(s.data.bd3()?);
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimtype = 6;
            if s.data.rc()? & 1 != 0 {
                dimtype |= 64;
            }
        }
        "DIMENSION_ANG3PT" => {
            dimension.defpoint = s.data.bd3()?;
            dimension.defpoint2 = Some(s.data.bd3()?);
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimension.defpoint4 = Some(s.data.bd3()?);
            dimtype = 5;
        }
        "DIMENSION_ANG2LN" => {
            let [x, y] = s.data.rd2()?;
            dimension.defpoint5 = Some([x, y, elevation]);
            dimension.defpoint2 = Some(s.data.bd3()?);
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimension.defpoint4 = Some(s.data.bd3()?);
            dimension.defpoint = s.data.bd3()?;
            dimtype = 2;
        }
        "DIMENSION_RADIUS" | "DIMENSION_DIAMETER" => {
            if kind == "DIMENSION_RADIUS" {
                dimension.defpoint = s.data.bd3()?;
                dimension.defpoint4 = Some(s.data.bd3()?);
                dimtype = 4;
            } else {
                dimension.defpoint4 = Some(s.data.bd3()?);
                dimension.defpoint = s.data.bd3()?;
                dimtype = 3;
            }
            s.data.bd()?;
        }
        "ARC_DIMENSION" => {
            // 20.4.19。弧の点（10）、補助線の点（13・14）、弧の中心（15）、部分（B）、角度（BD × 2）、引出線（B と 16・17）
            dimension.defpoint = s.data.bd3()?;
            dimension.defpoint2 = Some(s.data.bd3()?);
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimension.defpoint4 = Some(s.data.bd3()?);
            s.data.bit()?;
            s.data.bd()?;
            s.data.bd()?;
            s.data.bit()?;
            dimension.defpoint5 = Some(s.data.bd3()?);
            s.data.bd3()?;
            dimtype = 0;
        }
        "LARGE_RADIAL_DIMENSION" => {
            // 20.4.30。中心（10）、弦の点（13）、不明（BD）、表示の中心（14）、折れの点（15）
            dimension.defpoint = s.data.bd3()?;
            dimension.defpoint2 = Some(s.data.bd3()?);
            s.data.bd()?;
            dimension.defpoint3 = Some(s.data.bd3()?);
            dimension.defpoint4 = Some(s.data.bd3()?);
            dimtype = 0;
        }
        _ => return Err(Eof),
    }
    if flags1 & 2 != 0 {
        dimtype |= 32;
    }
    if flags1 & 1 == 0 {
        dimtype |= 128;
    }
    dimension.dimtype = f64::from(dimtype);
    dimension.dimstyle = names.dimstyle(s.handle()?.value);
    let block = s.handle()?.value;
    dimension.anonymous_block_name = names.blocks.get(&block).cloned().unwrap_or_default();
    Ok((dimension, extrusion))
}

/// LEADER（20.4.47）
fn leader(s: &mut Streams<'_>, names: &Names) -> Read<(DxfLeader, [f64; 3])> {
    s.data.bit()?;
    let annotation_type = f64::from(s.data.bs()?);
    let path_type = f64::from(s.data.bs()?);
    let count = count(s.data.bl()?)?;
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        points.push(s.data.bd3()?);
    }
    s.data.bd3()?;
    let extrusion = s.data.bd3()?;
    let x_direction = s.data.bd3()?;
    s.data.bd3()?;
    s.data.bd3()?;
    if s.format.version <= crate::file::Version::R2007 {
        s.data.bd()?;
        s.data.bd()?;
    }
    let hook_on_x = s.data.bit()?;
    let has_arrowhead = s.data.bit()?;
    s.data.bs()?;
    s.data.bit()?;
    s.data.bit()?;
    let annotation = s.handle()?.value;
    let style = s.handle()?.value;
    let dimstyle = names.dimstyle(style);
    // 注釈を持つ引出線の終わりの横線（フック）は DWG に頂点がなく、DXF には AutoCAD が頂点を足して書く（76 の頂点の数、
    // 75 のフックの有無）。横の向き（211）の反対（Hooklineonxdir が 1 なら同じ向き）に、矢印の大きさ × DIMSCALE だけ戻った
    // 点を最後の頂点の前に足す（ACadSharp の見本の DWG と DXF で確かめた。推定）
    if annotation != 0 && annotation_type < 3.0 && points.len() >= 2 {
        let length = names.arrow_size(style);
        let sign = if hook_on_x { 1.0 } else { -1.0 };
        let last = points[points.len() - 1];
        let hook = [
            last[0] + sign * x_direction[0] * length,
            last[1] + sign * x_direction[1] * length,
            last[2] + sign * x_direction[2] * length,
        ];
        if length > 0.0 && hook != points[points.len() - 2] {
            points.insert(points.len() - 1, hook);
        }
    }
    Ok((DxfLeader { points, path_type, annotation_type, has_arrowhead, dimstyle }, extrusion))
}

/// HATCH（20.4.75）。境界の円弧・楕円弧の角度はラジアンから DXF の度にする
fn hatch(s: &mut Streams<'_>) -> Read<(DxfHatch, [f64; 3])> {
    let mut gradient: Option<String> = None;
    let mut gradient_colors: Option<Vec<f64>> = None;
    if s.format.r2004() {
        let enabled = s.data.bl()? != 0;
        s.data.bl()?;
        s.data.bd()?;
        s.data.bd()?;
        s.data.bl()?;
        s.data.bd()?;
        let colors = count(s.data.bl()?)?;
        let mut list = Vec::new();
        for _ in 0..colors {
            s.data.bd()?;
            let color = s.cmc()?;
            list.push(color.rgb.unwrap_or_else(|| aci_rgb(color.index)));
        }
        let name = s.tv()?;
        if enabled {
            gradient = Some(name);
            if !list.is_empty() {
                gradient_colors = Some(list);
            }
        }
    }
    let elevation = s.data.bd()?;
    let extrusion = s.data.bd3()?;
    let pattern_name = s.tv()?;
    let solid_fill = s.data.bit()?;
    let associative = s.data.bit()?;
    let path_count = count(s.data.bl()?)?;
    let mut paths = Vec::with_capacity(path_count.min(1 << 12));
    let mut derived = false;
    for _ in 0..path_count {
        let flags = s.data.bl()?;
        if flags & 4 != 0 {
            derived = true;
        }
        let mut path =
            DxfHatchPath { flags: f64::from(flags), closed: true, points: Vec::new(), bulges: None, edges: None };
        if flags & 2 == 0 {
            let segments = count(s.data.bl()?)?;
            let mut edges = Vec::with_capacity(segments.min(1 << 12));
            for _ in 0..segments {
                if let Some(edge) = hatch_edge(s)? {
                    edges.push(edge);
                }
            }
            path.edges = Some(edges);
        } else {
            let has_bulges = s.data.bit()?;
            path.closed = s.data.bit()?;
            let points = count(s.data.bl()?)?;
            let mut bulges = Vec::with_capacity(points.min(1 << 16));
            for _ in 0..points {
                let [x, y] = s.data.rd2()?;
                path.points.push([x, y, elevation]);
                bulges.push(if has_bulges { s.data.bd()? } else { 0.0 });
            }
            if bulges.iter().any(|bulge| *bulge != 0.0) {
                path.bulges = Some(bulges);
            }
        }
        for _ in 0..count(s.data.bl()?)? {
            s.handle()?;
        }
        paths.push(path);
    }
    let hatch_style = f64::from(s.data.bs()?);
    let pattern_type = f64::from(s.data.bs()?);
    let mut hatch = DxfHatch {
        pattern_name,
        solid_fill,
        associative,
        hatch_style,
        pattern_type,
        pattern_angle: 0.0,
        pattern_scale: 1.0,
        pattern_double: false,
        elevation,
        paths,
        pattern_lines: Vec::new(),
        gradient,
        gradient_colors,
    };
    if !solid_fill {
        hatch.pattern_angle = deg(s.data.bd()?);
        hatch.pattern_scale = s.data.bd()?;
        hatch.pattern_double = s.data.bit()?;
        let lines = s.data.bs()?.max(0) as usize;
        for _ in 0..lines {
            let angle = deg(s.data.bd()?);
            let base = s.data.bd2()?;
            let offset = s.data.bd2()?;
            let dashes = s.data.bs()?.max(0) as usize;
            let mut line = DxfPatternLine { angle, base, offset, dashes: Vec::with_capacity(dashes) };
            for _ in 0..dashes {
                line.dashes.push(s.data.bd()?);
            }
            hatch.pattern_lines.push(line);
        }
    }
    if derived {
        s.data.bd()?;
    }
    // 種の点（98・10）は描画に使わない
    Ok((hatch, extrusion))
}

/// 境界の辺（72 の 1 線、2 円弧、3 楕円弧、4 スプライン）
fn hatch_edge(s: &mut Streams<'_>) -> Read<Option<DxfHatchEdge>> {
    Ok(match s.data.rc()? {
        1 => Some(DxfHatchEdge::Line { start: s.data.rd2()?, end: s.data.rd2()? }),
        2 => {
            let center = s.data.rd2()?;
            let radius = s.data.bd()?;
            let start_angle = deg(s.data.bd()?);
            let end_angle = deg(s.data.bd()?);
            let ccw = s.data.bit()?;
            Some(DxfHatchEdge::Arc { center, radius, start_angle, end_angle, ccw })
        }
        3 => {
            let center = s.data.rd2()?;
            let major_axis = s.data.rd2()?;
            let ratio = s.data.bd()?;
            let start_angle = deg(s.data.bd()?);
            let end_angle = deg(s.data.bd()?);
            let ccw = s.data.bit()?;
            Some(DxfHatchEdge::Ellipse { center, major_axis, ratio, start_angle, end_angle, ccw })
        }
        4 => {
            let degree = f64::from(s.data.bl()?);
            let rational = s.data.bit()?;
            let periodic = s.data.bit()?;
            let knot_count = count(s.data.bl()?)?;
            let control_count = count(s.data.bl()?)?;
            let mut knots = Vec::with_capacity(knot_count.min(1 << 16));
            for _ in 0..knot_count {
                knots.push(s.data.bd()?);
            }
            let mut control_points = Vec::with_capacity(control_count.min(1 << 16));
            let mut weights = Vec::new();
            for _ in 0..control_count {
                control_points.push(s.data.rd2()?);
                if rational {
                    weights.push(s.data.bd()?);
                }
            }
            let mut fit_points = Vec::new();
            if s.format.r2010() {
                let fits = count(s.data.bl()?)?;
                if fits > 0 {
                    for _ in 0..fits {
                        fit_points.push(s.data.rd2()?);
                    }
                    s.data.rd2()?;
                    s.data.rd2()?;
                }
            }
            Some(DxfHatchEdge::Spline { degree, rational, periodic, knots, control_points, weights, fit_points })
        }
        _ => return Err(Eof),
    })
}

/// ACI の色を 0xRRGGBB に（グラデーションの色が ACI のとき）
fn aci_rgb(index: f64) -> f64 {
    let color = aci_color(index);
    f64::from(u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0))
}

/// VIEWPORT（20.4.38）。DXF の 68（重なりの順）と 69（番号）は DWG にないので、文書を組み立てるときに決める
fn viewport(s: &mut Streams<'_>) -> Read<DxfViewport> {
    let center = s.data.bd3()?;
    let width = s.data.bd()?;
    let height = s.data.bd()?;
    let view_target = s.data.bd3()?;
    let view_direction = s.data.bd3()?;
    let view_twist_angle = deg(s.data.bd()?);
    let view_height = s.data.bd()?;
    // 焦点距離、前・後ろの切り取り、スナップの角度
    for _ in 0..4 {
        s.data.bd()?;
    }
    let view_center = s.data.rd2()?;
    // スナップの基点・間隔、グリッドの間隔
    for _ in 0..3 {
        s.data.rd2()?;
    }
    s.data.bs()?;
    if s.format.r2007() {
        s.data.bs()?;
    }
    let frozen_count = count(s.data.bl()?)?;
    let status_flags = f64::from(s.data.bl()?);
    s.tv()?;
    s.data.rc()?;
    s.data.bit()?;
    s.data.bit()?;
    for _ in 0..3 {
        s.data.bd3()?;
    }
    s.data.bd()?;
    s.data.bs()?;
    let mut frozen_layer_handles = Vec::with_capacity(frozen_count.min(1 << 12));
    for _ in 0..frozen_count {
        frozen_layer_handles.push(hex(s.handle()?.value));
    }
    let clip = s.handle()?.value;
    Ok(DxfViewport {
        center,
        width,
        height,
        status: 1.0,
        id: 0.0,
        view_center,
        view_target,
        view_direction,
        view_height,
        view_twist_angle,
        status_flags,
        frozen_layer_handles,
        clipping_boundary_handle: (clip != 0).then(|| hex(clip)),
    })
}

/// IMAGE・WIPEOUT（20.4.80）
fn image(s: &mut Streams<'_>) -> Read<DxfImage> {
    s.data.bl()?;
    let insert = s.data.bd3()?;
    let u_pixel = s.data.bd3()?;
    let v_pixel = s.data.bd3()?;
    let size = s.data.rd2()?;
    let flags = f64::from(s.data.bs()?);
    let clipping = s.data.bit()?;
    s.data.rc()?;
    s.data.rc()?;
    s.data.rc()?;
    if s.format.r2010() {
        s.data.bit()?;
    }
    let mut boundary = Vec::new();
    match s.data.bs()? {
        1 => {
            boundary.push(s.data.rd2()?);
            boundary.push(s.data.rd2()?);
        }
        2 => {
            for _ in 0..count(s.data.bl()?)? {
                boundary.push(s.data.rd2()?);
            }
            // DXF は多角形の境界を最初の点で閉じて書く（ACadSharp の DXF の見本で確かめた）
            if let (Some(first), Some(last)) = (boundary.first().copied(), boundary.last().copied())
                && boundary.len() > 2
                && first != last
            {
                boundary.push(first);
            }
        }
        _ => {}
    }
    let or_one = |value: f64| if value == 0.0 || value.is_nan() { 1.0 } else { value };
    Ok(DxfImage { insert, u_pixel, v_pixel, image_size: [or_one(size[0]), or_one(size[1])], boundary, flags, clipping })
}

/// MLINE（20.4.50）。頂点ごと・要素ごとの線と塗りのパラメータ（DXF の 41・42）も持つ
fn mline(s: &mut Streams<'_>, names: &Names) -> Read<(DxfMLine, [f64; 3])> {
    let scale = s.data.bd()?;
    let justification = f64::from(s.data.rc()?);
    let base_point = s.data.bd3()?;
    let extrusion = s.data.bd3()?;
    // 開いている（1）か閉じている（3）か。DXF の 71 の 4・8（端を描かない）は DWG にない
    let flags = f64::from(s.data.bs()?);
    let line_count = s.data.rc()?;
    let vertex_count = s.data.bs()?.max(0) as usize;
    let mut mline = DxfMLine {
        style_name: "Standard".into(),
        mlinestyle_handle: None,
        scale,
        justification,
        flags,
        closed: js_int(flags) & 2 != 0,
        line_count: f64::from(line_count),
        base_point,
        points: Vec::with_capacity(vertex_count),
        vertex_directions: Vec::with_capacity(vertex_count),
        miter_directions: Vec::with_capacity(vertex_count),
        line_params: Some(Vec::with_capacity(vertex_count)),
        fill_params: Some(Vec::with_capacity(vertex_count)),
    };
    for _ in 0..vertex_count {
        mline.points.push(s.data.bd3()?);
        mline.vertex_directions.push(s.data.bd3()?);
        mline.miter_directions.push(s.data.bd3()?);
        let mut lines = Vec::with_capacity(usize::from(line_count));
        let mut fills = Vec::with_capacity(usize::from(line_count));
        for _ in 0..line_count {
            let mut line = Vec::new();
            for _ in 0..s.data.bs()?.max(0) {
                line.push(s.data.bd()?);
            }
            let mut fill = Vec::new();
            for _ in 0..s.data.bs()?.max(0) {
                fill.push(s.data.bd()?);
            }
            lines.push(line);
            fills.push(fill);
        }
        if let Some(list) = mline.line_params.as_mut() {
            list.push(lines);
        }
        if let Some(list) = mline.fill_params.as_mut() {
            list.push(fills);
        }
    }
    let style = s.handle()?.value;
    if let Some(name) = names.mlinestyles.get(&style) {
        mline.style_name = name.clone();
    }
    if style != 0 {
        mline.mlinestyle_handle = Some(hex(style));
    }
    Ok((mline, extrusion))
}

fn js_int(value: f64) -> i32 {
    zumen_core::js::to_int32(value)
}
