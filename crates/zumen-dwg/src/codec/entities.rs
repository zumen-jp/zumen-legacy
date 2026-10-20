//! 図形（20.4「OBJECT PRESCRIPTIONS」）のすべての値。共通の値（EntityCommon）の後の、種類ごとの値を読み書きする

use super::object::EntityCommon;
use super::{CmColor, In, RawText};
use crate::bits::{Eof, HandleRef, Read};
use crate::file::Version;
use crate::write::object::ObjectWriter;

/// 数（BL）。負や大きすぎる数は読めない印にする
fn count(value: i32) -> Read<usize> {
    if (0..=10_000_000).contains(&value) { Ok(value as usize) } else { Err(Eof) }
}

/// 図形
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entity {
    pub common: EntityCommon,
    pub body: Body,
}

/// 種類ごとの値
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Body {
    Line(Line),
    Circle(Circle),
    Arc(Arc),
    Point(Point),
    Ellipse(Ellipse),
    Face(Face),
    /// SOLID・TRACE
    Solid(Solid),
    /// RAY・XLINE
    Ray(Ray),
    Text(Text),
    Attrib(Attribute),
    Attdef(Attribute),
    /// BLOCK
    Block(RawText),
    /// ENDBLK・SEQEND（値なし）
    Empty,
    /// INSERT・MINSERT
    Insert(Insert),
    Vertex2d(Vertex2d),
    /// VERTEX_3D・VERTEX_MESH・VERTEX_PFACE
    Vertex3d(u8, [f64; 3]),
    FaceRecord([i16; 4]),
    Polyline2d(Polyline2d),
    Polyline3d(Polyline3d),
    PolylinePface(PolylinePface),
    PolylineMesh(PolylineMesh),
    Dimension(Dimension),
    Viewport(Box<Viewport>),
    Spline(Spline),
    MText(Box<MText>),
    LwPolyline(LwPolyline),
    Leader(Leader),
    MLine(MLine),
    Hatch(Box<Hatch>),
    /// IMAGE・WIPEOUT
    Image(Box<Image>),
    MLeader(Box<MLeader>),
}

/// 型の番号の図形の DXF の名前（固定の型、20.3）。クラスの図形はクラスの DXF の名前を使う
pub(crate) fn fixed_name(kind: u16) -> Option<&'static str> {
    Some(match kind {
        1 => "TEXT",
        2 => "ATTRIB",
        3 => "ATTDEF",
        4 => "BLOCK",
        5 => "ENDBLK",
        6 => "SEQEND",
        7 => "INSERT",
        8 => "MINSERT",
        10 => "VERTEX_2D",
        11 => "VERTEX_3D",
        12 => "VERTEX_MESH",
        13 => "VERTEX_PFACE",
        14 => "VERTEX_PFACE_FACE",
        15 => "POLYLINE_2D",
        16 => "POLYLINE_3D",
        17 => "ARC",
        18 => "CIRCLE",
        19 => "LINE",
        20 => "DIMENSION_ORDINATE",
        21 => "DIMENSION_LINEAR",
        22 => "DIMENSION_ALIGNED",
        23 => "DIMENSION_ANG3PT",
        24 => "DIMENSION_ANG2LN",
        25 => "DIMENSION_RADIUS",
        26 => "DIMENSION_DIAMETER",
        27 => "POINT",
        28 => "3DFACE",
        29 => "POLYLINE_PFACE",
        30 => "POLYLINE_MESH",
        31 => "SOLID",
        32 => "TRACE",
        34 => "VIEWPORT",
        35 => "ELLIPSE",
        36 => "SPLINE",
        40 => "RAY",
        41 => "XLINE",
        44 => "MTEXT",
        45 => "LEADER",
        47 => "MLINE",
        77 => "LWPOLYLINE",
        78 => "HATCH",
        _ => return None,
    })
}

/// 種類ごとの値を読む関数
type BodyReader = fn(&mut In<'_>, &str) -> Read<Body>;

/// 種類（fixed_name かクラスの DXF の名前）の値を読む関数
fn body_reader(kind: &str) -> Option<BodyReader> {
    Some(match kind {
        "LINE" => |s, _| Ok(Body::Line(Line::read(s)?)),
        "CIRCLE" => |s, _| Ok(Body::Circle(Circle::read(s)?)),
        "ARC" => |s, _| Ok(Body::Arc(Arc::read(s)?)),
        "POINT" => |s, _| Ok(Body::Point(Point::read(s)?)),
        "ELLIPSE" => |s, _| Ok(Body::Ellipse(Ellipse::read(s)?)),
        "3DFACE" => |s, _| Ok(Body::Face(Face::read(s)?)),
        "SOLID" | "TRACE" => |s, _| Ok(Body::Solid(Solid::read(s)?)),
        "RAY" | "XLINE" => |s, _| Ok(Body::Ray(Ray::read(s)?)),
        "TEXT" => |s, _| Ok(Body::Text(Text::read(s)?)),
        "ATTRIB" => |s, _| Ok(Body::Attrib(Attribute::read(s, false)?)),
        "ATTDEF" => |s, _| Ok(Body::Attdef(Attribute::read(s, true)?)),
        "BLOCK" => |s, _| Ok(Body::Block(s.text()?)),
        "ENDBLK" | "SEQEND" => |_, _| Ok(Body::Empty),
        "INSERT" | "MINSERT" => |s, kind| Ok(Body::Insert(Insert::read(s, kind == "MINSERT")?)),
        "VERTEX_2D" => |s, _| Ok(Body::Vertex2d(Vertex2d::read(s)?)),
        "VERTEX_3D" | "VERTEX_MESH" | "VERTEX_PFACE" => |s, _| Ok(Body::Vertex3d(s.data.rc()?, s.data.bd3()?)),
        "VERTEX_PFACE_FACE" => |s, _| Ok(Body::FaceRecord([s.data.bs()?, s.data.bs()?, s.data.bs()?, s.data.bs()?])),
        "POLYLINE_2D" => |s, _| Ok(Body::Polyline2d(Polyline2d::read(s)?)),
        "POLYLINE_3D" => |s, _| Ok(Body::Polyline3d(Polyline3d::read(s)?)),
        "POLYLINE_PFACE" => |s, _| Ok(Body::PolylinePface(PolylinePface::read(s)?)),
        "POLYLINE_MESH" => |s, _| Ok(Body::PolylineMesh(PolylineMesh::read(s)?)),
        "DIMENSION_ORDINATE"
        | "DIMENSION_LINEAR"
        | "DIMENSION_ALIGNED"
        | "DIMENSION_ANG3PT"
        | "DIMENSION_ANG2LN"
        | "DIMENSION_RADIUS"
        | "DIMENSION_DIAMETER"
        | "ARC_DIMENSION"
        | "LARGE_RADIAL_DIMENSION" => |s, kind| Ok(Body::Dimension(Dimension::read(s, kind)?)),
        "VIEWPORT" => |s, _| Ok(Body::Viewport(Box::new(Viewport::read(s)?))),
        "SPLINE" => |s, _| Ok(Body::Spline(Spline::read(s)?)),
        "MTEXT" => |s, _| Ok(Body::MText(Box::new(MText::read(s)?))),
        "LWPOLYLINE" => |s, _| Ok(Body::LwPolyline(LwPolyline::read(s)?)),
        "LEADER" => |s, _| Ok(Body::Leader(Leader::read(s)?)),
        "MLINE" => |s, _| Ok(Body::MLine(MLine::read(s)?)),
        "HATCH" => |s, _| Ok(Body::Hatch(Box::new(Hatch::read(s)?))),
        "IMAGE" | "WIPEOUT" => |s, _| Ok(Body::Image(Box::new(Image::read(s)?))),
        "MULTILEADER" => |s, _| Ok(Body::MLeader(Box::new(MLeader::read(s)?))),
        _ => return None,
    })
}

impl Entity {
    /// 読み書きできる種類か
    pub fn knows(kind: &str) -> bool {
        body_reader(kind).is_some()
    }

    /// 種類（fixed_name かクラスの DXF の名前）の図形を読む。読めない種類は None
    pub fn read(s: &mut In<'_>, kind: &str) -> Read<Option<Self>> {
        let Some(reader) = body_reader(kind) else { return Ok(None) };
        let common = EntityCommon::read(s)?;
        let body = reader(s, kind)?;
        Ok(Some(Self { common, body }))
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        match &self.body {
            Body::Line(line) => line.write(o),
            Body::Circle(circle) => circle.write(o),
            Body::Arc(arc) => arc.write(o),
            Body::Point(point) => point.write(o),
            Body::Ellipse(ellipse) => ellipse.write(o),
            Body::Face(face) => face.write(o),
            Body::Solid(solid) => solid.write(o),
            Body::Ray(ray) => ray.write(o),
            Body::Text(text) => text.write(o),
            Body::Attrib(attribute) | Body::Attdef(attribute) => attribute.write(o),
            Body::Block(name) => o.text(name),
            Body::Empty => {}
            Body::Insert(insert) => insert.write(o),
            Body::Vertex2d(vertex) => vertex.write(o),
            Body::Vertex3d(flags, point) => {
                o.data.rc(*flags);
                o.data.bd3(*point);
            }
            Body::FaceRecord(indices) => {
                for index in indices {
                    o.data.bs(*index);
                }
            }
            Body::Polyline2d(polyline) => polyline.write(o),
            Body::Polyline3d(polyline) => polyline.write(o),
            Body::PolylinePface(polyline) => polyline.write(o),
            Body::PolylineMesh(polyline) => polyline.write(o),
            Body::Dimension(dimension) => dimension.write(o),
            Body::Viewport(viewport) => viewport.write(o),
            Body::Spline(spline) => spline.write(o),
            Body::MText(mtext) => mtext.write(o),
            Body::LwPolyline(polyline) => polyline.write(o),
            Body::Leader(leader) => leader.write(o),
            Body::MLine(mline) => mline.write(o),
            Body::Hatch(hatch) => hatch.write(o),
            Body::Image(image) => image.write(o),
            Body::MLeader(mleader) => mleader.write(o),
        }
    }
}

/// LINE（20.4.21）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Line {
    pub start: [f64; 3],
    pub end: [f64; 3],
    pub thickness: f64,
    pub extrusion: [f64; 3],
}

impl Line {
    fn read(s: &mut In<'_>) -> Read<Self> {
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
        Ok(Self { start: [x0, y0, z0], end: [x1, y1, z1], thickness: s.data.bt(true)?, extrusion: s.data.be(true)? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        // 2 つの Z がどちらも +0.0 なら Z を書かない（-0.0 は書く）
        let z_zero = self.start[2].to_bits() == 0 && self.end[2].to_bits() == 0;
        o.data.bit(z_zero);
        o.data.rd(self.start[0]);
        o.data.dd(self.end[0], self.start[0]);
        o.data.rd(self.start[1]);
        o.data.dd(self.end[1], self.start[1]);
        if !z_zero {
            o.data.rd(self.start[2]);
            o.data.dd(self.end[2], self.start[2]);
        }
        o.data.bt(self.thickness);
        o.data.be(self.extrusion);
    }
}

/// CIRCLE（20.4.20）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Circle {
    pub center: [f64; 3],
    pub radius: f64,
    pub thickness: f64,
    pub extrusion: [f64; 3],
}

impl Circle {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self {
            center: s.data.bd3()?,
            radius: s.data.bd()?,
            thickness: s.data.bt(true)?,
            extrusion: s.data.be(true)?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.center);
        o.data.bd(self.radius);
        o.data.bt(self.thickness);
        o.data.be(self.extrusion);
    }
}

/// ARC（20.4.18）。角度はラジアン
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Arc {
    pub circle: Circle,
    pub start_angle: f64,
    pub end_angle: f64,
}

impl Arc {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self { circle: Circle::read(s)?, start_angle: s.data.bd()?, end_angle: s.data.bd()? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        self.circle.write(o);
        o.data.bd(self.start_angle);
        o.data.bd(self.end_angle);
    }
}

/// POINT（20.4.31）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Point {
    pub location: [f64; 3],
    pub thickness: f64,
    pub extrusion: [f64; 3],
    pub x_axis_angle: f64,
}

impl Point {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self {
            location: s.data.bd3()?,
            thickness: s.data.bt(true)?,
            extrusion: s.data.be(true)?,
            x_axis_angle: s.data.bd()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.location);
        o.data.bt(self.thickness);
        o.data.be(self.extrusion);
        o.data.bd(self.x_axis_angle);
    }
}

/// ELLIPSE（20.4.39）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ellipse {
    pub center: [f64; 3],
    pub major_axis: [f64; 3],
    pub extrusion: [f64; 3],
    pub ratio: f64,
    pub start: f64,
    pub end: f64,
}

impl Ellipse {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self {
            center: s.data.bd3()?,
            major_axis: s.data.bd3()?,
            extrusion: s.data.bd3()?,
            ratio: s.data.bd()?,
            start: s.data.bd()?,
            end: s.data.bd()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.center);
        o.data.bd3(self.major_axis);
        o.data.bd3(self.extrusion);
        o.data.bd(self.ratio);
        o.data.bd(self.start);
        o.data.bd(self.end);
    }
}

/// 3DFACE（20.4.32）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Face {
    pub corners: [[f64; 3]; 4],
    /// 見えない辺の印。None なら「印なし」のビットが立つ
    pub invisible: Option<i16>,
}

impl Face {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let no_flags = s.data.bit()?;
        let z_zero = s.data.bit()?;
        let x = s.data.rd()?;
        let y = s.data.rd()?;
        let z = if z_zero { 0.0 } else { s.data.rd()? };
        let first = [x, y, z];
        let second = s.data.dd3(first)?;
        let third = s.data.dd3(second)?;
        let fourth = s.data.dd3(third)?;
        let invisible = if no_flags { None } else { Some(s.data.bs()?) };
        Ok(Self { corners: [first, second, third, fourth], invisible })
    }

    fn write(&self, o: &mut ObjectWriter) {
        let [first, second, third, fourth] = self.corners;
        o.data.bit(self.invisible.is_none());
        let z_zero = first[2].to_bits() == 0;
        o.data.bit(z_zero);
        o.data.rd(first[0]);
        o.data.rd(first[1]);
        if !z_zero {
            o.data.rd(first[2]);
        }
        o.data.dd3(second, first);
        o.data.dd3(third, second);
        o.data.dd3(fourth, third);
        if let Some(invisible) = self.invisible {
            o.data.bs(invisible);
        }
    }
}

/// SOLID・TRACE（20.4.35、20.4.36）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Solid {
    pub thickness: f64,
    pub elevation: f64,
    pub corners: [[f64; 2]; 4],
    pub extrusion: [f64; 3],
}

impl Solid {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let thickness = s.data.bt(true)?;
        let elevation = s.data.bd()?;
        let corners = [s.data.rd2()?, s.data.rd2()?, s.data.rd2()?, s.data.rd2()?];
        Ok(Self { thickness, elevation, corners, extrusion: s.data.be(true)? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bt(self.thickness);
        o.data.bd(self.elevation);
        for corner in self.corners {
            o.data.rd2(corner);
        }
        o.data.be(self.extrusion);
    }
}

/// RAY・XLINE（20.4.42、20.4.43）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Ray {
    pub point: [f64; 3],
    pub vector: [f64; 3],
}

impl Ray {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self { point: s.data.bd3()?, vector: s.data.bd3()? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.point);
        o.data.bd3(self.vector);
    }
}

/// TEXT（20.4.3）。R2000 以降は DataFlags のビットが立った値を省く（Option の None）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Text {
    pub elevation: Option<f64>,
    pub insert: [f64; 2],
    pub align: Option<[f64; 2]>,
    pub extrusion: [f64; 3],
    pub thickness: f64,
    pub oblique: Option<f64>,
    pub rotation: Option<f64>,
    pub height: f64,
    pub width: Option<f64>,
    pub value: RawText,
    pub generation: Option<i16>,
    pub halign: Option<i16>,
    pub valign: Option<i16>,
    pub style: HandleRef,
}

impl Text {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let flags = s.data.rc()?;
        let elevation = if flags & 0x01 == 0 { Some(s.data.rd()?) } else { None };
        let insert = s.data.rd2()?;
        let align = if flags & 0x02 == 0 { Some(s.data.dd2(insert)?) } else { None };
        let extrusion = s.data.be(true)?;
        let thickness = s.data.bt(true)?;
        let oblique = if flags & 0x04 == 0 { Some(s.data.rd()?) } else { None };
        let rotation = if flags & 0x08 == 0 { Some(s.data.rd()?) } else { None };
        let height = s.data.rd()?;
        let width = if flags & 0x10 == 0 { Some(s.data.rd()?) } else { None };
        let value = s.text()?;
        let generation = if flags & 0x20 == 0 { Some(s.data.bs()?) } else { None };
        let halign = if flags & 0x40 == 0 { Some(s.data.bs()?) } else { None };
        let valign = if flags & 0x80 == 0 { Some(s.data.bs()?) } else { None };
        let style = s.handle()?;
        Ok(Self {
            elevation,
            insert,
            align,
            extrusion,
            thickness,
            oblique,
            rotation,
            height,
            width,
            value,
            generation,
            halign,
            valign,
            style,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        let mut flags = 0u8;
        for (bit, absent) in [
            (0x01, self.elevation.is_none()),
            (0x02, self.align.is_none()),
            (0x04, self.oblique.is_none()),
            (0x08, self.rotation.is_none()),
            (0x10, self.width.is_none()),
            (0x20, self.generation.is_none()),
            (0x40, self.halign.is_none()),
            (0x80, self.valign.is_none()),
        ] {
            if absent {
                flags |= bit;
            }
        }
        o.data.rc(flags);
        if let Some(elevation) = self.elevation {
            o.data.rd(elevation);
        }
        o.data.rd2(self.insert);
        if let Some(align) = self.align {
            o.data.dd2(align, self.insert);
        }
        o.data.be(self.extrusion);
        o.data.bt(self.thickness);
        if let Some(oblique) = self.oblique {
            o.data.rd(oblique);
        }
        if let Some(rotation) = self.rotation {
            o.data.rd(rotation);
        }
        o.data.rd(self.height);
        if let Some(width) = self.width {
            o.data.rd(width);
        }
        o.text(&self.value);
        for value in [self.generation, self.halign, self.valign].into_iter().flatten() {
            o.data.bs(value);
        }
        o.href(self.style);
    }
}

/// 複数行の属性（R2018 以降）の埋め込みの MTEXT と注釈の値
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EmbeddedMText {
    pub common: EntityCommon,
    pub mtext: Box<MText>,
    /// 注釈の値のバイト列と、登録したアプリケーション、不明の BS（大きさが 0 なら None）
    pub annotative: Option<(Vec<u8>, HandleRef, i16)>,
}

/// ATTRIB・ATTDEF（20.4.4、20.4.5）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Attribute {
    pub text: Text,
    /// R2010 以降の版
    pub version: u8,
    /// R2018 以降の種類（1 は 1 行、2 は複数行の属性、4 は複数行の属性定義）
    pub attribute_type: u8,
    pub embedded: Option<EmbeddedMText>,
    pub tag: RawText,
    pub field_length: i16,
    pub flags: u8,
    /// R2007 以降の位置の固定
    pub lock: bool,
    /// ATTDEF の R2010 以降の版と問い合わせの文字
    pub prompt: Option<(u8, RawText)>,
}

impl Attribute {
    fn read(s: &mut In<'_>, definition: bool) -> Read<Self> {
        let text = Text::read(s)?;
        let version = if s.version >= Version::R2010 { s.data.rc()? } else { 0 };
        let mut attribute_type = 1;
        let mut embedded = None;
        if s.version >= Version::R2018 {
            attribute_type = s.data.rc()?;
            if attribute_type == 2 || attribute_type == 4 {
                let common = EntityCommon::read_mode(s, None)?;
                let mtext = Box::new(MText::read(s)?);
                let size = s.data.bs()?;
                let annotative = if size > 0 {
                    let bytes = s.data.bytes(size as u16 as usize)?;
                    Some((bytes, s.handle()?, s.data.bs()?))
                } else {
                    None
                };
                embedded = Some(EmbeddedMText { common, mtext, annotative });
            }
        }
        let tag = s.text()?;
        let field_length = s.data.bs()?;
        let flags = s.data.rc()?;
        let lock = s.r2007() && s.data.bit()?;
        let prompt = if definition {
            let version = if s.version >= Version::R2010 { s.data.rc()? } else { 0 };
            Some((version, s.text()?))
        } else {
            None
        };
        Ok(Self { text, version, attribute_type, embedded, tag, field_length, flags, lock, prompt })
    }

    fn write(&self, o: &mut ObjectWriter) {
        self.text.write(o);
        if o.r2010() {
            o.data.rc(self.version);
        }
        if o.r2018() {
            o.data.rc(self.attribute_type);
            if let Some(embedded) = &self.embedded {
                embedded.common.write_mode(o);
                embedded.mtext.write(o);
                match &embedded.annotative {
                    Some((bytes, app, unknown)) => {
                        o.data.bs(bytes.len() as i16);
                        o.data.raw(bytes);
                        o.href(*app);
                        o.data.bs(*unknown);
                    }
                    None => o.data.bs(0),
                }
            }
        }
        o.text(&self.tag);
        o.data.bs(self.field_length);
        o.data.rc(self.flags);
        if o.r2007() {
            o.data.bit(self.lock);
        }
        if let Some((version, prompt)) = &self.prompt {
            if o.r2010() {
                o.data.rc(*version);
            }
            o.text(prompt);
        }
    }
}

/// MTEXT の値（20.4.46）。R2018 以降の「Is NOT annotative」が 1 のときは、同じ値と段組みの値が続く
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MText {
    pub insert: [f64; 3],
    pub extrusion: [f64; 3],
    pub direction: [f64; 3],
    pub rect_width: f64,
    /// R2007 以降の枠の高さ
    pub rect_height: Option<f64>,
    pub char_height: f64,
    pub attachment: i16,
    pub drawing_direction: i16,
    pub extents_height: f64,
    pub extents_width: f64,
    pub text: RawText,
    pub style: HandleRef,
    pub line_spacing_style: i16,
    pub line_spacing_factor: f64,
    pub unknown_bit: bool,
    /// R2004 以降の背景の印と、背景（印の 1 のとき）の倍率・色・透過度
    pub background_flags: i32,
    pub background: Option<(f64, CmColor, i32)>,
    /// R2018 以降の「Is NOT annotative」と、そのときの値
    pub not_annotative: Option<Box<MTextRedundant>>,
    pub annotative_flag: Option<bool>,
}

/// R2018 以降の MTEXT の同じ値（REDUNDANT FIELDS）と段組み
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MTextRedundant {
    pub version: i16,
    pub default_flag: bool,
    pub registered_app: HandleRef,
    pub attachment: i32,
    pub direction: [f64; 3],
    pub insert: [f64; 3],
    pub rect_width: f64,
    pub rect_height: f64,
    pub extents_width: f64,
    pub extents_height: f64,
    pub column_type: i16,
    /// 段組みがあるとき: 高さの数、幅、間隔、自動の高さ、流れの向き、高さ
    pub columns: Option<(i32, f64, f64, bool, bool, Vec<f64>)>,
}

impl MText {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let insert = s.data.bd3()?;
        let extrusion = s.data.bd3()?;
        let direction = s.data.bd3()?;
        let rect_width = s.data.bd()?;
        let rect_height = if s.r2007() { Some(s.data.bd()?) } else { None };
        let char_height = s.data.bd()?;
        let attachment = s.data.bs()?;
        let drawing_direction = s.data.bs()?;
        let extents_height = s.data.bd()?;
        let extents_width = s.data.bd()?;
        let text = s.text()?;
        let style = s.handle()?;
        let line_spacing_style = s.data.bs()?;
        let line_spacing_factor = s.data.bd()?;
        let unknown_bit = s.data.bit()?;
        let mut background_flags = 0;
        let mut background = None;
        if s.r2004() {
            background_flags = s.data.bl()?;
            if background_flags & 1 != 0 || (s.version >= Version::R2018 && background_flags & 0x10 != 0) {
                background = Some((s.data.bd()?, s.cmc()?, s.data.bl()?));
            }
        }
        let (mut not_annotative, mut annotative_flag) = (None, None);
        if s.version >= Version::R2018 {
            let flag = s.data.bit()?;
            annotative_flag = Some(flag);
            if flag {
                let version = s.data.bs()?;
                let default_flag = s.data.bit()?;
                let registered_app = s.handle()?;
                let attachment = s.data.bl()?;
                let direction = s.data.bd3()?;
                let insert = s.data.bd3()?;
                let rect_width = s.data.bd()?;
                let rect_height = s.data.bd()?;
                let extents_width = s.data.bd()?;
                let extents_height = s.data.bd()?;
                let column_type = s.data.bs()?;
                let columns = if column_type != 0 {
                    let heights = s.data.bl()?;
                    let width = s.data.bd()?;
                    let gutter = s.data.bd()?;
                    let auto_height = s.data.bit()?;
                    let flow = s.data.bit()?;
                    let mut list = Vec::new();
                    if !auto_height && column_type == 2 {
                        for _ in 0..count(heights)? {
                            list.push(s.data.bd()?);
                        }
                    }
                    Some((heights, width, gutter, auto_height, flow, list))
                } else {
                    None
                };
                not_annotative = Some(Box::new(MTextRedundant {
                    version,
                    default_flag,
                    registered_app,
                    attachment,
                    direction,
                    insert,
                    rect_width,
                    rect_height,
                    extents_width,
                    extents_height,
                    column_type,
                    columns,
                }));
            }
        }
        Ok(Self {
            insert,
            extrusion,
            direction,
            rect_width,
            rect_height,
            char_height,
            attachment,
            drawing_direction,
            extents_height,
            extents_width,
            text,
            style,
            line_spacing_style,
            line_spacing_factor,
            unknown_bit,
            background_flags,
            background,
            not_annotative,
            annotative_flag,
        })
    }

    pub(crate) fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.insert);
        o.data.bd3(self.extrusion);
        o.data.bd3(self.direction);
        o.data.bd(self.rect_width);
        if o.r2007() {
            o.data.bd(self.rect_height.unwrap_or(0.0));
        }
        o.data.bd(self.char_height);
        o.data.bs(self.attachment);
        o.data.bs(self.drawing_direction);
        o.data.bd(self.extents_height);
        o.data.bd(self.extents_width);
        o.text(&self.text);
        o.href(self.style);
        o.data.bs(self.line_spacing_style);
        o.data.bd(self.line_spacing_factor);
        o.data.bit(self.unknown_bit);
        if o.r2004() {
            o.data.bl(self.background_flags);
            if let Some((scale, color, transparency)) = &self.background {
                o.data.bd(*scale);
                o.cmc(color);
                o.data.bl(*transparency);
            }
        }
        if o.r2018() {
            match &self.not_annotative {
                Some(redundant) => {
                    o.data.bit(true);
                    o.data.bs(redundant.version);
                    o.data.bit(redundant.default_flag);
                    o.href(redundant.registered_app);
                    o.data.bl(redundant.attachment);
                    o.data.bd3(redundant.direction);
                    o.data.bd3(redundant.insert);
                    o.data.bd(redundant.rect_width);
                    o.data.bd(redundant.rect_height);
                    o.data.bd(redundant.extents_width);
                    o.data.bd(redundant.extents_height);
                    o.data.bs(redundant.column_type);
                    if let Some((heights, width, gutter, auto_height, flow, list)) = &redundant.columns {
                        o.data.bl(*heights);
                        o.data.bd(*width);
                        o.data.bd(*gutter);
                        o.data.bit(*auto_height);
                        o.data.bit(*flow);
                        for height in list {
                            o.data.bd(*height);
                        }
                    }
                }
                None => o.data.bit(self.annotative_flag.unwrap_or(false)),
            }
        }
    }
}

/// INSERT・MINSERT（20.4.9、20.4.10）。尺度は DWG の書き方（BB の印と RD・DD）のまま持つ
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Insert {
    pub point: [f64; 3],
    pub scale: [f64; 3],
    /// 尺度の印（00 すべて、01 X が 1、10 すべて同じ、11 すべて 1）
    pub scale_flags: u8,
    pub rotation: f64,
    pub extrusion: [f64; 3],
    pub has_attributes: bool,
    /// MINSERT の列・行の数と間隔
    pub array: Option<(i16, i16, f64, f64)>,
    pub block: HandleRef,
    /// R2004 以降の属性
    pub attributes: Vec<HandleRef>,
    /// R2000 の最初と最後の属性
    pub first_last: Option<(HandleRef, HandleRef)>,
    pub seqend: Option<HandleRef>,
}

impl Insert {
    /// 尺度の印（AutoCAD と同じく、縮めて書ける形を選ぶ）
    pub fn scale_flags_of(scale: [f64; 3]) -> u8 {
        let one = 1f64.to_bits();
        if scale.map(f64::to_bits) == [one, one, one] {
            3
        } else if scale[0].to_bits() == one {
            1
        } else if scale[1].to_bits() == scale[0].to_bits() && scale[2].to_bits() == scale[0].to_bits() {
            2
        } else {
            0
        }
    }

    fn read(s: &mut In<'_>, minsert: bool) -> Read<Self> {
        let point = s.data.bd3()?;
        let scale_flags = s.data.bb()?;
        let scale = match scale_flags {
            0 => {
                let x = s.data.rd()?;
                [x, s.data.dd(x)?, s.data.dd(x)?]
            }
            1 => [1.0, s.data.dd(1.0)?, s.data.dd(1.0)?],
            2 => {
                let x = s.data.rd()?;
                [x, x, x]
            }
            _ => [1.0, 1.0, 1.0],
        };
        let rotation = s.data.bd()?;
        let extrusion = s.data.bd3()?;
        let has_attributes = s.data.bit()?;
        let owned = if s.r2004() && has_attributes { count(s.data.bl()?)? } else { 0 };
        let array = if minsert { Some((s.data.bs()?, s.data.bs()?, s.data.bd()?, s.data.bd()?)) } else { None };
        let block = s.handle()?;
        let (mut attributes, mut first_last, mut seqend) = (Vec::new(), None, None);
        if has_attributes {
            if s.r2004() {
                for _ in 0..owned {
                    attributes.push(s.handle()?);
                }
            } else {
                first_last = Some((s.handle()?, s.handle()?));
            }
            seqend = Some(s.handle()?);
        }
        Ok(Self {
            point,
            scale,
            scale_flags,
            rotation,
            extrusion,
            has_attributes,
            array,
            block,
            attributes,
            first_last,
            seqend,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.point);
        o.data.bb(self.scale_flags);
        match self.scale_flags {
            0 => {
                o.data.rd(self.scale[0]);
                o.data.dd(self.scale[1], self.scale[0]);
                o.data.dd(self.scale[2], self.scale[0]);
            }
            1 => {
                o.data.dd(self.scale[1], 1.0);
                o.data.dd(self.scale[2], 1.0);
            }
            2 => o.data.rd(self.scale[0]),
            _ => {}
        }
        o.data.bd(self.rotation);
        o.data.bd3(self.extrusion);
        o.data.bit(self.has_attributes);
        if o.r2004() && self.has_attributes {
            o.data.bl(self.attributes.len() as i32);
        }
        if let Some((columns, rows, column_spacing, row_spacing)) = self.array {
            o.data.bs(columns);
            o.data.bs(rows);
            o.data.bd(column_spacing);
            o.data.bd(row_spacing);
        }
        o.href(self.block);
        if self.has_attributes {
            if o.r2004() {
                for attribute in &self.attributes {
                    o.href(*attribute);
                }
            } else if let Some((first, last)) = self.first_last {
                o.href(first);
                o.href(last);
            }
            o.href(self.seqend.unwrap_or(HandleRef { code: 3, value: 0 }));
        }
    }
}

/// VERTEX_2D（20.4.11）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Vertex2d {
    pub flags: u8,
    pub point: [f64; 3],
    pub start_width: f64,
    /// 始めと同じ幅（始めの幅を負にして書く）なら None
    pub end_width: Option<f64>,
    pub bulge: f64,
    /// R2010 以降の頂点の番号
    pub id: i32,
    pub tangent: f64,
}

impl Vertex2d {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let flags = s.data.rc()?;
        let point = s.data.bd3()?;
        let start_width = s.data.bd()?;
        let end_width = if start_width < 0.0 { None } else { Some(s.data.bd()?) };
        let bulge = s.data.bd()?;
        let id = if s.version >= Version::R2010 { s.data.bl()? } else { 0 };
        let tangent = s.data.bd()?;
        Ok(Self { flags, point, start_width, end_width, bulge, id, tangent })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.rc(self.flags);
        o.data.bd3(self.point);
        o.data.bd(self.start_width);
        if let Some(end) = self.end_width {
            o.data.bd(end);
        }
        o.data.bd(self.bulge);
        if o.r2010() {
            o.data.bl(self.id);
        }
        o.data.bd(self.tangent);
    }
}

/// POLYLINE の頂点（R2004 以降は持つ図形の並び、それより前は最初と最後）と SEQEND
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Owned {
    pub vertices: Vec<HandleRef>,
    pub first_last: Option<(HandleRef, HandleRef)>,
    pub seqend: HandleRef,
}

impl Owned {
    /// R2004 以降の数（BL）は値の並び、ハンドルはハンドルの並び。count は先に読んだ数
    fn read(s: &mut In<'_>, count: usize) -> Read<Self> {
        let (mut vertices, mut first_last) = (Vec::new(), None);
        if s.r2004() {
            for _ in 0..count {
                vertices.push(s.handle()?);
            }
        } else {
            first_last = Some((s.handle()?, s.handle()?));
        }
        Ok(Self { vertices, first_last, seqend: s.handle()? })
    }

    fn count(s: &mut In<'_>) -> Read<usize> {
        if s.r2004() { count(s.data.bl()?) } else { Ok(0) }
    }

    fn write_count(&self, o: &mut ObjectWriter) {
        if o.r2004() {
            o.data.bl(self.vertices.len() as i32);
        }
    }

    fn write(&self, o: &mut ObjectWriter) {
        if o.r2004() {
            for vertex in &self.vertices {
                o.href(*vertex);
            }
        } else {
            let none = HandleRef { code: 4, value: 0 };
            let (first, last) = self.first_last.unwrap_or((none, none));
            o.href(first);
            o.href(last);
        }
        o.href(self.seqend);
    }
}

/// POLYLINE_2D（20.4.16）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Polyline2d {
    pub flags: i16,
    pub curve_type: i16,
    pub start_width: f64,
    pub end_width: f64,
    pub thickness: f64,
    pub elevation: f64,
    pub extrusion: [f64; 3],
    pub owned: Owned,
}

impl Polyline2d {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let flags = s.data.bs()?;
        let curve_type = s.data.bs()?;
        let start_width = s.data.bd()?;
        let end_width = s.data.bd()?;
        let thickness = s.data.bt(true)?;
        let elevation = s.data.bd()?;
        let extrusion = s.data.be(true)?;
        let count = Owned::count(s)?;
        let owned = Owned::read(s, count)?;
        Ok(Self { flags, curve_type, start_width, end_width, thickness, elevation, extrusion, owned })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bs(self.flags);
        o.data.bs(self.curve_type);
        o.data.bd(self.start_width);
        o.data.bd(self.end_width);
        o.data.bt(self.thickness);
        o.data.bd(self.elevation);
        o.data.be(self.extrusion);
        self.owned.write_count(o);
        self.owned.write(o);
    }
}

/// POLYLINE_3D（20.4.17）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Polyline3d {
    /// 曲線の印（1・2 はスプライン）と閉じる印（1）
    pub spline_flags: u8,
    pub closed_flags: u8,
    pub owned: Owned,
}

impl Polyline3d {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let spline_flags = s.data.rc()?;
        let closed_flags = s.data.rc()?;
        let count = Owned::count(s)?;
        Ok(Self { spline_flags, closed_flags, owned: Owned::read(s, count)? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.rc(self.spline_flags);
        o.data.rc(self.closed_flags);
        self.owned.write_count(o);
        self.owned.write(o);
    }
}

/// POLYLINE_PFACE（20.4.33）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PolylinePface {
    pub vertex_count: i16,
    pub face_count: i16,
    pub owned: Owned,
}

impl PolylinePface {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let vertex_count = s.data.bs()?;
        let face_count = s.data.bs()?;
        let count = Owned::count(s)?;
        Ok(Self { vertex_count, face_count, owned: Owned::read(s, count)? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bs(self.vertex_count);
        o.data.bs(self.face_count);
        self.owned.write_count(o);
        self.owned.write(o);
    }
}

/// POLYLINE_MESH（20.4.34）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PolylineMesh {
    pub flags: i16,
    pub curve_type: i16,
    pub m_count: i16,
    pub n_count: i16,
    pub m_density: i16,
    pub n_density: i16,
    pub owned: Owned,
}

impl PolylineMesh {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let flags = s.data.bs()?;
        let curve_type = s.data.bs()?;
        let m_count = s.data.bs()?;
        let n_count = s.data.bs()?;
        let m_density = s.data.bs()?;
        let n_density = s.data.bs()?;
        let count = Owned::count(s)?;
        Ok(Self { flags, curve_type, m_count, n_count, m_density, n_density, owned: Owned::read(s, count)? })
    }

    fn write(&self, o: &mut ObjectWriter) {
        for value in [self.flags, self.curve_type, self.m_count, self.n_count, self.m_density, self.n_density] {
            o.data.bs(value);
        }
        self.owned.write_count(o);
        self.owned.write(o);
    }
}

/// 寸法（20.4.22〜20.4.30）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dimension {
    /// R2010 以降の版
    pub version: u8,
    pub extrusion: [f64; 3],
    pub text_midpoint: [f64; 2],
    pub elevation: f64,
    pub flags1: u8,
    pub text: RawText,
    pub text_rotation: f64,
    pub horizontal_direction: f64,
    pub insert_scale: [f64; 3],
    pub insert_rotation: f64,
    pub attachment: i16,
    pub line_spacing_style: i16,
    pub line_spacing_factor: f64,
    pub measurement: f64,
    /// R2007 以降の不明・矢印の向きの反転（2 つ）
    pub flips: [bool; 3],
    pub insert: [f64; 2],
    pub kind: DimensionKind,
    pub dimstyle: HandleRef,
    pub block: HandleRef,
}

/// 寸法の種類ごとの値（定義点は DXF の番号）
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DimensionKind {
    /// 10・13・14 と Flags 2
    Ordinate {
        p10: [f64; 3],
        p13: [f64; 3],
        p14: [f64; 3],
        flags2: u8,
    },
    /// 13・14・10、寸法補助線の角度（52）と寸法線の回転（50、平行寸法では None）
    Linear {
        p13: [f64; 3],
        p14: [f64; 3],
        p10: [f64; 3],
        oblique: f64,
        rotation: Option<f64>,
    },
    /// 10・13・14・15
    Angular3 {
        p10: [f64; 3],
        p13: [f64; 3],
        p14: [f64; 3],
        p15: [f64; 3],
    },
    /// 16（2RD）・13・14・15・10
    Angular2 {
        p16: [f64; 2],
        p13: [f64; 3],
        p14: [f64; 3],
        p15: [f64; 3],
        p10: [f64; 3],
    },
    /// 半径は 10・15、直径は 15・10 と、引出線の長さ
    Radius {
        p10: [f64; 3],
        p15: [f64; 3],
        leader: f64,
    },
    Diameter {
        p15: [f64; 3],
        p10: [f64; 3],
        leader: f64,
    },
    /// ARC_DIMENSION（20.4.19）
    Arc {
        p10: [f64; 3],
        p13: [f64; 3],
        p14: [f64; 3],
        p15: [f64; 3],
        partial: bool,
        start: f64,
        end: f64,
        leader: bool,
        p16: [f64; 3],
        p17: [f64; 3],
    },
    /// LARGE_RADIAL_DIMENSION（20.4.30）
    LargeRadial {
        p10: [f64; 3],
        p13: [f64; 3],
        unknown: f64,
        p14: [f64; 3],
        p15: [f64; 3],
    },
}

impl Dimension {
    fn read(s: &mut In<'_>, kind: &str) -> Read<Self> {
        let version = if s.version >= Version::R2010 { s.data.rc()? } else { 0 };
        let extrusion = s.data.bd3()?;
        let text_midpoint = s.data.rd2()?;
        let elevation = s.data.bd()?;
        let flags1 = s.data.rc()?;
        let text = s.text()?;
        let text_rotation = s.data.bd()?;
        let horizontal_direction = s.data.bd()?;
        let insert_scale = s.data.bd3()?;
        let insert_rotation = s.data.bd()?;
        let attachment = s.data.bs()?;
        let line_spacing_style = s.data.bs()?;
        let line_spacing_factor = s.data.bd()?;
        let measurement = s.data.bd()?;
        let flips = if s.r2007() { [s.data.bit()?, s.data.bit()?, s.data.bit()?] } else { [false; 3] };
        let insert = s.data.rd2()?;
        let kind = match kind {
            "DIMENSION_ORDINATE" => DimensionKind::Ordinate {
                p10: s.data.bd3()?,
                p13: s.data.bd3()?,
                p14: s.data.bd3()?,
                flags2: s.data.rc()?,
            },
            "DIMENSION_LINEAR" | "DIMENSION_ALIGNED" => DimensionKind::Linear {
                p13: s.data.bd3()?,
                p14: s.data.bd3()?,
                p10: s.data.bd3()?,
                oblique: s.data.bd()?,
                rotation: if kind == "DIMENSION_LINEAR" { Some(s.data.bd()?) } else { None },
            },
            "DIMENSION_ANG3PT" => DimensionKind::Angular3 {
                p10: s.data.bd3()?,
                p13: s.data.bd3()?,
                p14: s.data.bd3()?,
                p15: s.data.bd3()?,
            },
            "DIMENSION_ANG2LN" => DimensionKind::Angular2 {
                p16: s.data.rd2()?,
                p13: s.data.bd3()?,
                p14: s.data.bd3()?,
                p15: s.data.bd3()?,
                p10: s.data.bd3()?,
            },
            "DIMENSION_RADIUS" => {
                DimensionKind::Radius { p10: s.data.bd3()?, p15: s.data.bd3()?, leader: s.data.bd()? }
            }
            "DIMENSION_DIAMETER" => {
                DimensionKind::Diameter { p15: s.data.bd3()?, p10: s.data.bd3()?, leader: s.data.bd()? }
            }
            "ARC_DIMENSION" => DimensionKind::Arc {
                p10: s.data.bd3()?,
                p13: s.data.bd3()?,
                p14: s.data.bd3()?,
                p15: s.data.bd3()?,
                partial: s.data.bit()?,
                start: s.data.bd()?,
                end: s.data.bd()?,
                leader: s.data.bit()?,
                p16: s.data.bd3()?,
                p17: s.data.bd3()?,
            },
            _ => DimensionKind::LargeRadial {
                p10: s.data.bd3()?,
                p13: s.data.bd3()?,
                unknown: s.data.bd()?,
                p14: s.data.bd3()?,
                p15: s.data.bd3()?,
            },
        };
        Ok(Self {
            version,
            extrusion,
            text_midpoint,
            elevation,
            flags1,
            text,
            text_rotation,
            horizontal_direction,
            insert_scale,
            insert_rotation,
            attachment,
            line_spacing_style,
            line_spacing_factor,
            measurement,
            flips,
            insert,
            kind,
            dimstyle: s.handle()?,
            block: s.handle()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        if o.r2010() {
            o.data.rc(self.version);
        }
        o.data.bd3(self.extrusion);
        o.data.rd2(self.text_midpoint);
        o.data.bd(self.elevation);
        o.data.rc(self.flags1);
        o.text(&self.text);
        o.data.bd(self.text_rotation);
        o.data.bd(self.horizontal_direction);
        o.data.bd3(self.insert_scale);
        o.data.bd(self.insert_rotation);
        o.data.bs(self.attachment);
        o.data.bs(self.line_spacing_style);
        o.data.bd(self.line_spacing_factor);
        o.data.bd(self.measurement);
        if o.r2007() {
            for flip in self.flips {
                o.data.bit(flip);
            }
        }
        o.data.rd2(self.insert);
        match &self.kind {
            DimensionKind::Ordinate { p10, p13, p14, flags2 } => {
                o.data.bd3(*p10);
                o.data.bd3(*p13);
                o.data.bd3(*p14);
                o.data.rc(*flags2);
            }
            DimensionKind::Linear { p13, p14, p10, oblique, rotation } => {
                o.data.bd3(*p13);
                o.data.bd3(*p14);
                o.data.bd3(*p10);
                o.data.bd(*oblique);
                if let Some(rotation) = rotation {
                    o.data.bd(*rotation);
                }
            }
            DimensionKind::Angular3 { p10, p13, p14, p15 } => {
                for point in [p10, p13, p14, p15] {
                    o.data.bd3(*point);
                }
            }
            DimensionKind::Angular2 { p16, p13, p14, p15, p10 } => {
                o.data.rd2(*p16);
                for point in [p13, p14, p15, p10] {
                    o.data.bd3(*point);
                }
            }
            DimensionKind::Radius { p10, p15, leader } => {
                o.data.bd3(*p10);
                o.data.bd3(*p15);
                o.data.bd(*leader);
            }
            DimensionKind::Diameter { p15, p10, leader } => {
                o.data.bd3(*p15);
                o.data.bd3(*p10);
                o.data.bd(*leader);
            }
            DimensionKind::Arc { p10, p13, p14, p15, partial, start, end, leader, p16, p17 } => {
                for point in [p10, p13, p14, p15] {
                    o.data.bd3(*point);
                }
                o.data.bit(*partial);
                o.data.bd(*start);
                o.data.bd(*end);
                o.data.bit(*leader);
                o.data.bd3(*p16);
                o.data.bd3(*p17);
            }
            DimensionKind::LargeRadial { p10, p13, unknown, p14, p15 } => {
                o.data.bd3(*p10);
                o.data.bd3(*p13);
                o.data.bd(*unknown);
                o.data.bd3(*p14);
                o.data.bd3(*p15);
            }
        }
        o.href(self.dimstyle);
        o.href(self.block);
    }
}

/// VIEWPORT（20.4.38）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Viewport {
    pub center: [f64; 3],
    pub width: f64,
    pub height: f64,
    pub view_target: [f64; 3],
    pub view_direction: [f64; 3],
    pub twist: f64,
    pub view_height: f64,
    pub lens: f64,
    pub front_clip: f64,
    pub back_clip: f64,
    pub snap_angle: f64,
    pub view_center: [f64; 2],
    pub snap_base: [f64; 2],
    pub snap_spacing: [f64; 2],
    pub grid_spacing: [f64; 2],
    pub circle_zoom: i16,
    /// R2007 以降
    pub grid_major: i16,
    pub status_flags: i32,
    pub style_sheet: RawText,
    pub render_mode: u8,
    pub ucs_at_origin: bool,
    pub ucs_per_viewport: bool,
    pub ucs_origin: [f64; 3],
    pub ucs_x: [f64; 3],
    pub ucs_y: [f64; 3],
    pub ucs_elevation: f64,
    pub ucs_ortho: i16,
    /// R2004 以降
    pub shade_plot_mode: i16,
    /// R2007 以降の照明
    pub default_lights: bool,
    pub lighting_type: u8,
    pub brightness: f64,
    pub contrast: f64,
    pub ambient: Option<CmColor>,
    pub frozen_layers: Vec<HandleRef>,
    pub clip_boundary: HandleRef,
    /// R2000 のビューポートの図形のヘッダ
    pub vp_header: Option<HandleRef>,
    pub named_ucs: HandleRef,
    pub base_ucs: HandleRef,
    /// R2007 以降の背景・表示スタイル・陰影の印刷・太陽
    pub r2007_handles: Option<[HandleRef; 4]>,
}

impl Viewport {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let center = s.data.bd3()?;
        let width = s.data.bd()?;
        let height = s.data.bd()?;
        let view_target = s.data.bd3()?;
        let view_direction = s.data.bd3()?;
        let twist = s.data.bd()?;
        let view_height = s.data.bd()?;
        let lens = s.data.bd()?;
        let front_clip = s.data.bd()?;
        let back_clip = s.data.bd()?;
        let snap_angle = s.data.bd()?;
        let view_center = s.data.rd2()?;
        let snap_base = s.data.rd2()?;
        let snap_spacing = s.data.rd2()?;
        let grid_spacing = s.data.rd2()?;
        let circle_zoom = s.data.bs()?;
        let grid_major = if s.r2007() { s.data.bs()? } else { 0 };
        let frozen = count(s.data.bl()?)?;
        let status_flags = s.data.bl()?;
        let style_sheet = s.text()?;
        let render_mode = s.data.rc()?;
        let ucs_at_origin = s.data.bit()?;
        let ucs_per_viewport = s.data.bit()?;
        let ucs_origin = s.data.bd3()?;
        let ucs_x = s.data.bd3()?;
        let ucs_y = s.data.bd3()?;
        let ucs_elevation = s.data.bd()?;
        let ucs_ortho = s.data.bs()?;
        let shade_plot_mode = if s.r2004() { s.data.bs()? } else { 0 };
        let (mut default_lights, mut lighting_type, mut brightness, mut contrast, mut ambient) =
            (false, 0, 0.0, 0.0, None);
        if s.r2007() {
            default_lights = s.data.bit()?;
            lighting_type = s.data.rc()?;
            brightness = s.data.bd()?;
            contrast = s.data.bd()?;
            ambient = Some(s.cmc()?);
        }
        let mut frozen_layers = Vec::with_capacity(frozen.min(1 << 12));
        for _ in 0..frozen {
            frozen_layers.push(s.handle()?);
        }
        let clip_boundary = s.handle()?;
        let vp_header = if s.r2004() { None } else { Some(s.handle()?) };
        let named_ucs = s.handle()?;
        let base_ucs = s.handle()?;
        let r2007_handles = if s.r2007() { Some([s.handle()?, s.handle()?, s.handle()?, s.handle()?]) } else { None };
        Ok(Self {
            center,
            width,
            height,
            view_target,
            view_direction,
            twist,
            view_height,
            lens,
            front_clip,
            back_clip,
            snap_angle,
            view_center,
            snap_base,
            snap_spacing,
            grid_spacing,
            circle_zoom,
            grid_major,
            status_flags,
            style_sheet,
            render_mode,
            ucs_at_origin,
            ucs_per_viewport,
            ucs_origin,
            ucs_x,
            ucs_y,
            ucs_elevation,
            ucs_ortho,
            shade_plot_mode,
            default_lights,
            lighting_type,
            brightness,
            contrast,
            ambient,
            frozen_layers,
            clip_boundary,
            vp_header,
            named_ucs,
            base_ucs,
            r2007_handles,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd3(self.center);
        o.data.bd(self.width);
        o.data.bd(self.height);
        o.data.bd3(self.view_target);
        o.data.bd3(self.view_direction);
        o.data.bd(self.twist);
        o.data.bd(self.view_height);
        o.data.bd(self.lens);
        o.data.bd(self.front_clip);
        o.data.bd(self.back_clip);
        o.data.bd(self.snap_angle);
        o.data.rd2(self.view_center);
        o.data.rd2(self.snap_base);
        o.data.rd2(self.snap_spacing);
        o.data.rd2(self.grid_spacing);
        o.data.bs(self.circle_zoom);
        if o.r2007() {
            o.data.bs(self.grid_major);
        }
        o.data.bl(self.frozen_layers.len() as i32);
        o.data.bl(self.status_flags);
        o.text(&self.style_sheet);
        o.data.rc(self.render_mode);
        o.data.bit(self.ucs_at_origin);
        o.data.bit(self.ucs_per_viewport);
        o.data.bd3(self.ucs_origin);
        o.data.bd3(self.ucs_x);
        o.data.bd3(self.ucs_y);
        o.data.bd(self.ucs_elevation);
        o.data.bs(self.ucs_ortho);
        if o.r2004() {
            o.data.bs(self.shade_plot_mode);
        }
        if o.r2007() {
            o.data.bit(self.default_lights);
            o.data.rc(self.lighting_type);
            o.data.bd(self.brightness);
            o.data.bd(self.contrast);
            o.cmc(self.ambient.as_ref().unwrap_or(&CmColor::aci(250)));
        }
        for layer in &self.frozen_layers {
            o.href(*layer);
        }
        o.href(self.clip_boundary);
        if !o.r2004() {
            o.href(self.vp_header.unwrap_or(HandleRef { code: 5, value: 0 }));
        }
        o.href(self.named_ucs);
        o.href(self.base_ucs);
        if o.r2007() {
            let none = HandleRef { code: 5, value: 0 };
            for handle in self.r2007_handles.unwrap_or([none; 4]) {
                o.href(handle);
            }
        }
    }
}

/// SPLINE（20.4.40）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Spline {
    /// scenario の BL（1 制御点、2 当てはめ点）。R2013 以降の AutoCAD はつねに 1 を書き、どちらかは印と節点の取り方で決まる
    pub scenario: i32,
    /// R2013 以降の印（1 当てはめ点、2 制御点の枠を表示、4 閉じる、8 節点の取り方を使う）と節点の取り方
    /// （0 弦長、1 平方根、2 一様、15 任意）
    pub flags1: i32,
    pub knot_parameter: i32,
    pub degree: i32,
    pub fit: Option<SplineFit>,
    pub control: Option<SplineControl>,
}

/// scenario 2: 当てはめ点
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SplineFit {
    pub tolerance: f64,
    pub start_tangent: [f64; 3],
    pub end_tangent: [f64; 3],
    pub points: Vec<[f64; 3]>,
}

/// scenario 1: 節点と制御点
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SplineControl {
    pub rational: bool,
    pub closed: bool,
    pub periodic: bool,
    pub knot_tolerance: f64,
    pub control_tolerance: f64,
    pub weighted: bool,
    pub knots: Vec<f64>,
    pub points: Vec<[f64; 3]>,
    pub weights: Vec<f64>,
}

impl Spline {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let scenario = s.data.bl()?;
        let (mut flags1, mut knot_parameter) = (0, 0);
        if s.version >= Version::R2013 {
            flags1 = s.data.bl()?;
            knot_parameter = s.data.bl()?;
        }
        let degree = s.data.bl()?;
        // R2013 以降は、節点の取り方が任意（15）か節点の取り方を使わない（印の 8 がない）とき制御点（読み取り・ACadSharp と同じ）
        let fit_data =
            if s.version >= Version::R2013 { knot_parameter != 15 && flags1 & 8 != 0 } else { scenario == 2 };
        let (mut fit, mut control) = (None, None);
        if fit_data {
            let tolerance = s.data.bd()?;
            let start_tangent = s.data.bd3()?;
            let end_tangent = s.data.bd3()?;
            let count = count(s.data.bl()?)?;
            let mut points = Vec::with_capacity(count.min(1 << 16));
            for _ in 0..count {
                points.push(s.data.bd3()?);
            }
            fit = Some(SplineFit { tolerance, start_tangent, end_tangent, points });
        } else {
            let rational = s.data.bit()?;
            let closed = s.data.bit()?;
            let periodic = s.data.bit()?;
            let knot_tolerance = s.data.bd()?;
            let control_tolerance = s.data.bd()?;
            let knot_count = count(s.data.bl()?)?;
            let point_count = count(s.data.bl()?)?;
            let weighted = s.data.bit()?;
            let mut knots = Vec::with_capacity(knot_count.min(1 << 16));
            for _ in 0..knot_count {
                knots.push(s.data.bd()?);
            }
            let mut points = Vec::with_capacity(point_count.min(1 << 16));
            let mut weights = Vec::new();
            for _ in 0..point_count {
                points.push(s.data.bd3()?);
                if weighted {
                    weights.push(s.data.bd()?);
                }
            }
            control = Some(SplineControl {
                rational,
                closed,
                periodic,
                knot_tolerance,
                control_tolerance,
                weighted,
                knots,
                points,
                weights,
            });
        }
        Ok(Self { scenario, flags1, knot_parameter, degree, fit, control })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bl(self.scenario);
        if o.r2013() {
            o.data.bl(self.flags1);
            o.data.bl(self.knot_parameter);
        }
        o.data.bl(self.degree);
        if let Some(fit) = &self.fit {
            o.data.bd(fit.tolerance);
            o.data.bd3(fit.start_tangent);
            o.data.bd3(fit.end_tangent);
            o.data.bl(fit.points.len() as i32);
            for point in &fit.points {
                o.data.bd3(*point);
            }
        } else if let Some(control) = &self.control {
            o.data.bit(control.rational);
            o.data.bit(control.closed);
            o.data.bit(control.periodic);
            o.data.bd(control.knot_tolerance);
            o.data.bd(control.control_tolerance);
            o.data.bl(control.knots.len() as i32);
            o.data.bl(control.points.len() as i32);
            o.data.bit(control.weighted);
            for knot in &control.knots {
                o.data.bd(*knot);
            }
            for (index, point) in control.points.iter().enumerate() {
                o.data.bd3(*point);
                if control.weighted {
                    o.data.bd(control.weights.get(index).copied().unwrap_or(1.0));
                }
            }
        }
    }
}

/// LWPOLYLINE（20.4.85）。印（1 押し出し方向、2 厚さ、4 一定の幅、8 高さ、16 膨らみ、32 幅、256 線種を続ける、
/// 512 閉じる、1024 頂点の番号）のビットが立った値だけを持つ
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LwPolyline {
    pub flags: i16,
    pub const_width: f64,
    pub elevation: f64,
    pub thickness: f64,
    pub extrusion: [f64; 3],
    pub points: Vec<[f64; 2]>,
    pub bulges: Vec<f64>,
    /// R2010 以降の頂点の番号
    pub ids: Vec<i32>,
    pub widths: Vec<[f64; 2]>,
}

impl LwPolyline {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let flags = s.data.bs()?;
        let const_width = if flags & 4 != 0 { s.data.bd()? } else { 0.0 };
        let elevation = if flags & 8 != 0 { s.data.bd()? } else { 0.0 };
        let thickness = if flags & 2 != 0 { s.data.bd()? } else { 0.0 };
        let extrusion = if flags & 1 != 0 { s.data.bd3()? } else { [0.0, 0.0, 1.0] };
        let count = count(s.data.bl()?)?;
        let bulge_count = if flags & 16 != 0 { self::count(s.data.bl()?)? } else { 0 };
        let id_count = if s.version >= Version::R2010 && flags & 1024 != 0 { self::count(s.data.bl()?)? } else { 0 };
        let width_count = if flags & 32 != 0 { self::count(s.data.bl()?)? } else { 0 };
        let mut points = Vec::with_capacity(count.min(1 << 16));
        for index in 0..count {
            let point = if index == 0 { s.data.rd2()? } else { s.data.dd2(points[index - 1])? };
            points.push(point);
        }
        let mut bulges = Vec::with_capacity(bulge_count.min(1 << 16));
        for _ in 0..bulge_count {
            bulges.push(s.data.bd()?);
        }
        let mut ids = Vec::with_capacity(id_count.min(1 << 16));
        for _ in 0..id_count {
            ids.push(s.data.bl()?);
        }
        let mut widths = Vec::with_capacity(width_count.min(1 << 16));
        for _ in 0..width_count {
            widths.push(s.data.bd2()?);
        }
        Ok(Self { flags, const_width, elevation, thickness, extrusion, points, bulges, ids, widths })
    }

    fn write(&self, o: &mut ObjectWriter) {
        let flags = self.flags;
        o.data.bs(flags);
        if flags & 4 != 0 {
            o.data.bd(self.const_width);
        }
        if flags & 8 != 0 {
            o.data.bd(self.elevation);
        }
        if flags & 2 != 0 {
            o.data.bd(self.thickness);
        }
        if flags & 1 != 0 {
            o.data.bd3(self.extrusion);
        }
        o.data.bl(self.points.len() as i32);
        if flags & 16 != 0 {
            o.data.bl(self.bulges.len() as i32);
        }
        let ids = o.r2010() && flags & 1024 != 0;
        if ids {
            o.data.bl(self.ids.len() as i32);
        }
        if flags & 32 != 0 {
            o.data.bl(self.widths.len() as i32);
        }
        for (index, point) in self.points.iter().enumerate() {
            if index == 0 {
                o.data.rd2(*point);
            } else {
                o.data.dd2(*point, self.points[index - 1]);
            }
        }
        if flags & 16 != 0 {
            for bulge in &self.bulges {
                o.data.bd(*bulge);
            }
        }
        if ids {
            for id in &self.ids {
                o.data.bl(*id);
            }
        }
        if flags & 32 != 0 {
            for width in &self.widths {
                o.data.bd2(*width);
            }
        }
    }
}

/// LEADER（20.4.47）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Leader {
    pub unknown_bit: bool,
    /// 注釈の種類（0 MTEXT、1 TOLERANCE、2 INSERT、3 なし）
    pub annotation_type: i16,
    pub path_type: i16,
    pub points: Vec<[f64; 3]>,
    pub origin: [f64; 3],
    pub extrusion: [f64; 3],
    pub x_direction: [f64; 3],
    pub block_offset: [f64; 3],
    pub end_projection: [f64; 3],
    /// R2007 までの注釈の枠の高さと幅
    pub box_size: Option<[f64; 2]>,
    pub hook_on_x: bool,
    pub arrowhead: bool,
    pub unknown_short: i16,
    pub unknown_bits: [bool; 2],
    pub annotation: HandleRef,
    pub dimstyle: HandleRef,
}

impl Leader {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let unknown_bit = s.data.bit()?;
        let annotation_type = s.data.bs()?;
        let path_type = s.data.bs()?;
        let count = count(s.data.bl()?)?;
        let mut points = Vec::with_capacity(count.min(1 << 16));
        for _ in 0..count {
            points.push(s.data.bd3()?);
        }
        let origin = s.data.bd3()?;
        let extrusion = s.data.bd3()?;
        let x_direction = s.data.bd3()?;
        let block_offset = s.data.bd3()?;
        let end_projection = s.data.bd3()?;
        // 枠の大きさは R2007 まで（ACadSharp と同じ。仕様書は全版とする）
        let box_size = if s.version <= Version::R2007 { Some([s.data.bd()?, s.data.bd()?]) } else { None };
        let hook_on_x = s.data.bit()?;
        let arrowhead = s.data.bit()?;
        let unknown_short = s.data.bs()?;
        let unknown_bits = [s.data.bit()?, s.data.bit()?];
        Ok(Self {
            unknown_bit,
            annotation_type,
            path_type,
            points,
            origin,
            extrusion,
            x_direction,
            block_offset,
            end_projection,
            box_size,
            hook_on_x,
            arrowhead,
            unknown_short,
            unknown_bits,
            annotation: s.handle()?,
            dimstyle: s.handle()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bit(self.unknown_bit);
        o.data.bs(self.annotation_type);
        o.data.bs(self.path_type);
        o.data.bl(self.points.len() as i32);
        for point in &self.points {
            o.data.bd3(*point);
        }
        o.data.bd3(self.origin);
        o.data.bd3(self.extrusion);
        o.data.bd3(self.x_direction);
        o.data.bd3(self.block_offset);
        o.data.bd3(self.end_projection);
        if o.version <= Version::R2007 {
            let [height, width] = self.box_size.unwrap_or([0.0, 0.0]);
            o.data.bd(height);
            o.data.bd(width);
        }
        o.data.bit(self.hook_on_x);
        o.data.bit(self.arrowhead);
        o.data.bs(self.unknown_short);
        o.data.bit(self.unknown_bits[0]);
        o.data.bit(self.unknown_bits[1]);
        o.href(self.annotation);
        o.href(self.dimstyle);
    }
}

/// MLINE（20.4.50）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLine {
    pub scale: f64,
    /// 位置合わせ（0 上、1 中心、2 下）
    pub justification: u8,
    pub base_point: [f64; 3],
    pub extrusion: [f64; 3],
    /// 開いている（1）か閉じている（3）か
    pub open_closed: i16,
    pub lines_in_style: u8,
    pub vertices: Vec<MLineVertex>,
    pub style: HandleRef,
}

/// MLINE の頂点
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLineVertex {
    pub point: [f64; 3],
    pub direction: [f64; 3],
    pub miter: [f64; 3],
    /// 要素ごとの線のパラメータと塗りのパラメータ
    pub params: Vec<(Vec<f64>, Vec<f64>)>,
}

impl MLine {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let scale = s.data.bd()?;
        let justification = s.data.rc()?;
        let base_point = s.data.bd3()?;
        let extrusion = s.data.bd3()?;
        let open_closed = s.data.bs()?;
        let lines_in_style = s.data.rc()?;
        let count = s.data.bs()?.max(0) as usize;
        let mut vertices = Vec::with_capacity(count);
        for _ in 0..count {
            let point = s.data.bd3()?;
            let direction = s.data.bd3()?;
            let miter = s.data.bd3()?;
            let mut params = Vec::with_capacity(usize::from(lines_in_style));
            for _ in 0..lines_in_style {
                let mut line = Vec::new();
                for _ in 0..s.data.bs()?.max(0) {
                    line.push(s.data.bd()?);
                }
                let mut fill = Vec::new();
                for _ in 0..s.data.bs()?.max(0) {
                    fill.push(s.data.bd()?);
                }
                params.push((line, fill));
            }
            vertices.push(MLineVertex { point, direction, miter, params });
        }
        Ok(Self {
            scale,
            justification,
            base_point,
            extrusion,
            open_closed,
            lines_in_style,
            vertices,
            style: s.handle()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd(self.scale);
        o.data.rc(self.justification);
        o.data.bd3(self.base_point);
        o.data.bd3(self.extrusion);
        o.data.bs(self.open_closed);
        o.data.rc(self.lines_in_style);
        o.data.bs(self.vertices.len() as i16);
        for vertex in &self.vertices {
            o.data.bd3(vertex.point);
            o.data.bd3(vertex.direction);
            o.data.bd3(vertex.miter);
            for index in 0..usize::from(self.lines_in_style) {
                let (line, fill) = vertex.params.get(index).map_or((&[][..], &[][..]), |(l, f)| (&l[..], &f[..]));
                o.data.bs(line.len() as i16);
                for value in line {
                    o.data.bd(*value);
                }
                o.data.bs(fill.len() as i16);
                for value in fill {
                    o.data.bd(*value);
                }
            }
        }
        o.href(self.style);
    }
}

/// HATCH（20.4.75）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hatch {
    /// R2004 以降のグラデーション
    pub gradient: Option<Box<HatchGradient>>,
    pub elevation: f64,
    pub extrusion: [f64; 3],
    pub name: RawText,
    pub solid_fill: bool,
    pub associative: bool,
    pub paths: Vec<HatchPath>,
    /// 0 交互、1 最も外側、2 無視
    pub style: i16,
    /// 0 ユーザー定義、1 定義済み、2 カスタム
    pub pattern_type: i16,
    /// 塗りつぶしでないときの模様
    pub pattern: Option<HatchPattern>,
    /// 境界の印に 4 があるときの画素の大きさ
    pub pixel_size: Option<f64>,
    pub seeds: Vec<[f64; 2]>,
}

/// HATCH のグラデーション（R2004 以降）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HatchGradient {
    pub enabled: i32,
    pub reserved: i32,
    pub angle: f64,
    pub shift: f64,
    pub single_color: i32,
    pub tint: f64,
    /// 値と色
    pub colors: Vec<(f64, CmColor)>,
    pub name: RawText,
}

/// HATCH の境界
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HatchPath {
    /// 1 外側、2 ポリライン、4 派生、16 最も外側
    pub flags: i32,
    pub boundary: HatchBoundary,
    /// 境界の図形（ソフトポインタ）
    pub objects: Vec<HandleRef>,
}

/// 境界の形（印の 2 がポリライン）
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HatchBoundary {
    Edges(Vec<HatchEdge>),
    Polyline { has_bulges: bool, closed: bool, points: Vec<([f64; 2], f64)> },
}

/// 境界の辺（72 の 1 線、2 円弧、3 楕円弧、4 スプライン）。角度はラジアン
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HatchEdge {
    Line([f64; 2], [f64; 2]),
    Arc { center: [f64; 2], radius: f64, start: f64, end: f64, ccw: bool },
    Ellipse { center: [f64; 2], major_axis: [f64; 2], ratio: f64, start: f64, end: f64, ccw: bool },
    Spline(Box<HatchSpline>),
}

/// 境界のスプラインの辺
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HatchSpline {
    pub degree: i32,
    pub rational: bool,
    pub periodic: bool,
    pub knots: Vec<f64>,
    /// 制御点と重み（有理のときだけ書く）
    pub control: Vec<([f64; 2], f64)>,
    /// R2010 以降の当てはめ点と始めと終わりの接線（当てはめ点があるときだけ）
    pub fit: Vec<[f64; 2]>,
    pub tangents: [[f64; 2]; 2],
}

/// HATCH の模様
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HatchPattern {
    pub angle: f64,
    pub scale: f64,
    pub double: bool,
    pub lines: Vec<PatternLine>,
}

/// 模様の線
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PatternLine {
    pub angle: f64,
    pub base: [f64; 2],
    pub offset: [f64; 2],
    pub dashes: Vec<f64>,
}

impl Hatch {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let gradient = if s.r2004() {
            let enabled = s.data.bl()?;
            let reserved = s.data.bl()?;
            let angle = s.data.bd()?;
            let shift = s.data.bd()?;
            let single_color = s.data.bl()?;
            let tint = s.data.bd()?;
            let count = count(s.data.bl()?)?;
            let mut colors = Vec::with_capacity(count.min(256));
            for _ in 0..count {
                colors.push((s.data.bd()?, s.cmc()?));
            }
            let name = s.text()?;
            Some(Box::new(HatchGradient { enabled, reserved, angle, shift, single_color, tint, colors, name }))
        } else {
            None
        };
        let elevation = s.data.bd()?;
        let extrusion = s.data.bd3()?;
        let name = s.text()?;
        let solid_fill = s.data.bit()?;
        let associative = s.data.bit()?;
        let path_count = count(s.data.bl()?)?;
        let mut paths = Vec::with_capacity(path_count.min(1 << 12));
        for _ in 0..path_count {
            let flags = s.data.bl()?;
            let boundary = if flags & 2 == 0 {
                let segments = count(s.data.bl()?)?;
                let mut edges = Vec::with_capacity(segments.min(1 << 12));
                for _ in 0..segments {
                    edges.push(HatchEdge::read(s)?);
                }
                HatchBoundary::Edges(edges)
            } else {
                let has_bulges = s.data.bit()?;
                let closed = s.data.bit()?;
                let count = count(s.data.bl()?)?;
                let mut points = Vec::with_capacity(count.min(1 << 16));
                for _ in 0..count {
                    let point = s.data.rd2()?;
                    points.push((point, if has_bulges { s.data.bd()? } else { 0.0 }));
                }
                HatchBoundary::Polyline { has_bulges, closed, points }
            };
            let count = count(s.data.bl()?)?;
            let mut objects = Vec::with_capacity(count.min(1 << 12));
            for _ in 0..count {
                objects.push(s.handle()?);
            }
            paths.push(HatchPath { flags, boundary, objects });
        }
        let style = s.data.bs()?;
        let pattern_type = s.data.bs()?;
        let pattern = if solid_fill {
            None
        } else {
            let angle = s.data.bd()?;
            let scale = s.data.bd()?;
            let double = s.data.bit()?;
            let count = s.data.bs()?.max(0) as usize;
            let mut lines = Vec::with_capacity(count);
            for _ in 0..count {
                let angle = s.data.bd()?;
                let base = s.data.bd2()?;
                let offset = s.data.bd2()?;
                let dashes = s.data.bs()?.max(0) as usize;
                let mut list = Vec::with_capacity(dashes);
                for _ in 0..dashes {
                    list.push(s.data.bd()?);
                }
                lines.push(PatternLine { angle, base, offset, dashes: list });
            }
            Some(HatchPattern { angle, scale, double, lines })
        };
        let pixel_size = if paths.iter().any(|path| path.flags & 4 != 0) { Some(s.data.bd()?) } else { None };
        let count = count(s.data.bl()?)?;
        let mut seeds = Vec::with_capacity(count.min(1 << 12));
        for _ in 0..count {
            seeds.push(s.data.rd2()?);
        }
        Ok(Self {
            gradient,
            elevation,
            extrusion,
            name,
            solid_fill,
            associative,
            paths,
            style,
            pattern_type,
            pattern,
            pixel_size,
            seeds,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        if o.r2004() {
            let empty = HatchGradient {
                enabled: 0,
                reserved: 0,
                angle: 0.0,
                shift: 0.0,
                single_color: 0,
                tint: 0.0,
                colors: Vec::new(),
                name: RawText::empty(o.version),
            };
            let gradient = self.gradient.as_deref().unwrap_or(&empty);
            o.data.bl(gradient.enabled);
            o.data.bl(gradient.reserved);
            o.data.bd(gradient.angle);
            o.data.bd(gradient.shift);
            o.data.bl(gradient.single_color);
            o.data.bd(gradient.tint);
            o.data.bl(gradient.colors.len() as i32);
            for (value, color) in &gradient.colors {
                o.data.bd(*value);
                o.cmc(color);
            }
            o.text(&gradient.name);
        }
        o.data.bd(self.elevation);
        o.data.bd3(self.extrusion);
        o.text(&self.name);
        o.data.bit(self.solid_fill);
        o.data.bit(self.associative);
        o.data.bl(self.paths.len() as i32);
        for path in &self.paths {
            o.data.bl(path.flags);
            match &path.boundary {
                HatchBoundary::Edges(edges) => {
                    o.data.bl(edges.len() as i32);
                    for edge in edges {
                        edge.write(o);
                    }
                }
                HatchBoundary::Polyline { has_bulges, closed, points } => {
                    o.data.bit(*has_bulges);
                    o.data.bit(*closed);
                    o.data.bl(points.len() as i32);
                    for (point, bulge) in points {
                        o.data.rd2(*point);
                        if *has_bulges {
                            o.data.bd(*bulge);
                        }
                    }
                }
            }
            o.data.bl(path.objects.len() as i32);
            for object in &path.objects {
                o.href(*object);
            }
        }
        o.data.bs(self.style);
        o.data.bs(self.pattern_type);
        if !self.solid_fill {
            let empty = HatchPattern { angle: 0.0, scale: 1.0, double: false, lines: Vec::new() };
            let pattern = self.pattern.as_ref().unwrap_or(&empty);
            o.data.bd(pattern.angle);
            o.data.bd(pattern.scale);
            o.data.bit(pattern.double);
            o.data.bs(pattern.lines.len() as i16);
            for line in &pattern.lines {
                o.data.bd(line.angle);
                o.data.bd2(line.base);
                o.data.bd2(line.offset);
                o.data.bs(line.dashes.len() as i16);
                for dash in &line.dashes {
                    o.data.bd(*dash);
                }
            }
        }
        if self.paths.iter().any(|path| path.flags & 4 != 0) {
            o.data.bd(self.pixel_size.unwrap_or(0.0));
        }
        o.data.bl(self.seeds.len() as i32);
        for seed in &self.seeds {
            o.data.rd2(*seed);
        }
    }
}

impl HatchEdge {
    fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(match s.data.rc()? {
            1 => Self::Line(s.data.rd2()?, s.data.rd2()?),
            2 => Self::Arc {
                center: s.data.rd2()?,
                radius: s.data.bd()?,
                start: s.data.bd()?,
                end: s.data.bd()?,
                ccw: s.data.bit()?,
            },
            3 => Self::Ellipse {
                center: s.data.rd2()?,
                major_axis: s.data.rd2()?,
                ratio: s.data.bd()?,
                start: s.data.bd()?,
                end: s.data.bd()?,
                ccw: s.data.bit()?,
            },
            4 => {
                let degree = s.data.bl()?;
                let rational = s.data.bit()?;
                let periodic = s.data.bit()?;
                let knot_count = count(s.data.bl()?)?;
                let control_count = count(s.data.bl()?)?;
                let mut knots = Vec::with_capacity(knot_count.min(1 << 16));
                for _ in 0..knot_count {
                    knots.push(s.data.bd()?);
                }
                let mut control = Vec::with_capacity(control_count.min(1 << 16));
                for _ in 0..control_count {
                    let point = s.data.rd2()?;
                    control.push((point, if rational { s.data.bd()? } else { 1.0 }));
                }
                let (mut fit, mut tangents) = (Vec::new(), [[0.0; 2]; 2]);
                if s.version >= Version::R2010 {
                    let count = count(s.data.bl()?)?;
                    if count > 0 {
                        for _ in 0..count {
                            fit.push(s.data.rd2()?);
                        }
                        tangents = [s.data.rd2()?, s.data.rd2()?];
                    }
                }
                Self::Spline(Box::new(HatchSpline { degree, rational, periodic, knots, control, fit, tangents }))
            }
            _ => return Err(Eof),
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        match self {
            Self::Line(start, end) => {
                o.data.rc(1);
                o.data.rd2(*start);
                o.data.rd2(*end);
            }
            Self::Arc { center, radius, start, end, ccw } => {
                o.data.rc(2);
                o.data.rd2(*center);
                o.data.bd(*radius);
                o.data.bd(*start);
                o.data.bd(*end);
                o.data.bit(*ccw);
            }
            Self::Ellipse { center, major_axis, ratio, start, end, ccw } => {
                o.data.rc(3);
                o.data.rd2(*center);
                o.data.rd2(*major_axis);
                o.data.bd(*ratio);
                o.data.bd(*start);
                o.data.bd(*end);
                o.data.bit(*ccw);
            }
            Self::Spline(spline) => {
                o.data.rc(4);
                o.data.bl(spline.degree);
                o.data.bit(spline.rational);
                o.data.bit(spline.periodic);
                o.data.bl(spline.knots.len() as i32);
                o.data.bl(spline.control.len() as i32);
                for knot in &spline.knots {
                    o.data.bd(*knot);
                }
                for (point, weight) in &spline.control {
                    o.data.rd2(*point);
                    if spline.rational {
                        o.data.bd(*weight);
                    }
                }
                if o.r2010() {
                    o.data.bl(spline.fit.len() as i32);
                    if !spline.fit.is_empty() {
                        for point in &spline.fit {
                            o.data.rd2(*point);
                        }
                        o.data.rd2(spline.tangents[0]);
                        o.data.rd2(spline.tangents[1]);
                    }
                }
            }
        }
    }
}

/// IMAGE・WIPEOUT（20.4.80）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Image {
    pub class_version: i32,
    pub insert: [f64; 3],
    pub u_vector: [f64; 3],
    pub v_vector: [f64; 3],
    pub size: [f64; 2],
    /// 1 表示、2 傾いても表示、4 切り抜き、8 透過
    pub display: i16,
    pub clipping: bool,
    pub brightness: u8,
    pub contrast: u8,
    pub fade: u8,
    /// R2010 以降の切り抜きの向き（0 外側、1 内側）
    pub clip_mode: bool,
    /// 1 矩形、2 多角形
    pub clip_type: i16,
    pub boundary: Vec<[f64; 2]>,
    pub imagedef: HandleRef,
    pub reactor: HandleRef,
}

impl Image {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let class_version = s.data.bl()?;
        let insert = s.data.bd3()?;
        let u_vector = s.data.bd3()?;
        let v_vector = s.data.bd3()?;
        let size = s.data.rd2()?;
        let display = s.data.bs()?;
        let clipping = s.data.bit()?;
        let brightness = s.data.rc()?;
        let contrast = s.data.rc()?;
        let fade = s.data.rc()?;
        let clip_mode = s.version >= Version::R2010 && s.data.bit()?;
        let clip_type = s.data.bs()?;
        let mut boundary = Vec::new();
        if clip_type == 1 {
            boundary.push(s.data.rd2()?);
            boundary.push(s.data.rd2()?);
        } else {
            for _ in 0..count(s.data.bl()?)? {
                boundary.push(s.data.rd2()?);
            }
        }
        Ok(Self {
            class_version,
            insert,
            u_vector,
            v_vector,
            size,
            display,
            clipping,
            brightness,
            contrast,
            fade,
            clip_mode,
            clip_type,
            boundary,
            imagedef: s.handle()?,
            reactor: s.handle()?,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bl(self.class_version);
        o.data.bd3(self.insert);
        o.data.bd3(self.u_vector);
        o.data.bd3(self.v_vector);
        o.data.rd2(self.size);
        o.data.bs(self.display);
        o.data.bit(self.clipping);
        o.data.rc(self.brightness);
        o.data.rc(self.contrast);
        o.data.rc(self.fade);
        if o.r2010() {
            o.data.bit(self.clip_mode);
        }
        o.data.bs(self.clip_type);
        if self.clip_type == 1 {
            for index in 0..2 {
                o.data.rd2(self.boundary.get(index).copied().unwrap_or([0.0, 0.0]));
            }
        } else {
            o.data.bl(self.boundary.len() as i32);
            for point in &self.boundary {
                o.data.rd2(*point);
            }
        }
        o.href(self.imagedef);
        o.href(self.reactor);
    }
}

/// MULTILEADER（20.4.48）。値の並びとハンドルの並びの順は ACadSharp の readMultiLeader と同じ（mleader.rs）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLeader {
    /// R2010 以降の版（2）
    pub version: i16,
    pub context: AnnotContext,
    pub style: HandleRef,
    /// スタイルから変えた値の印
    pub overrides: i32,
    pub leader_type: i16,
    pub line_color: CmColor,
    pub linetype: HandleRef,
    pub lineweight: i32,
    pub landing: bool,
    pub dogleg: bool,
    pub landing_distance: f64,
    pub arrow_head: HandleRef,
    pub arrow_size: f64,
    pub content_type: i16,
    pub text_style: HandleRef,
    pub left_attachment: i16,
    pub right_attachment: i16,
    pub text_angle_type: i16,
    pub unknown: i16,
    pub text_color: CmColor,
    pub text_frame: bool,
    pub block: HandleRef,
    pub block_color: CmColor,
    pub block_scale: [f64; 3],
    pub block_rotation: f64,
    pub block_connection: i16,
    pub annotative: bool,
    /// R2007 までの引出線ごとの矢印（既定か、ブロック）
    pub arrow_heads: Vec<(bool, HandleRef)>,
    /// ブロックの属性（ATTDEF、値、番号、幅）
    pub labels: Vec<(HandleRef, RawText, i16, f64)>,
    pub text_direction_negative: bool,
    pub ipe_align: i16,
    pub justification: i16,
    pub scale: f64,
    /// R2010 以降の接続の向きと、上・下の接続
    pub attachments: Option<[i16; 3]>,
    /// R2013 以降の、引出線を文字まで伸ばす
    pub extended_to_text: bool,
}

/// MLeaderAnnotContext（20.4.86）の MULTILEADER に埋め込まれた値
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnnotContext {
    pub roots: Vec<LeaderRoot>,
    pub scale: f64,
    pub base_point: [f64; 3],
    pub text_height: f64,
    pub arrow_size: f64,
    pub landing_gap: f64,
    /// 左・右の接続、文字の揃え、接続の種類
    pub attachments: [i16; 4],
    pub content: Content,
    /// 基点・基準の向き・基準の垂直の向き
    pub base: [[f64; 3]; 3],
    pub normal_reversed: bool,
    /// R2010 以降の上・下の接続
    pub top_bottom: Option<[i16; 2]>,
}

/// 内容（文字かブロック）
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Content {
    Text(Box<ContextText>),
    Block(Box<ContextBlock>),
    None,
}

/// 引出線のまとまり
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LeaderRoot {
    pub content_valid: bool,
    pub unknown: bool,
    pub connection: [f64; 3],
    pub direction: [f64; 3],
    pub breaks: Vec<[[f64; 3]; 2]>,
    pub index: i32,
    pub landing_distance: f64,
    pub lines: Vec<LeaderLine>,
    /// R2010 以降の接続の向き
    pub attachment_direction: i16,
}

/// 引出線の 1 本
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LeaderLine {
    pub points: Vec<[f64; 3]>,
    /// 線分の番号と、切れ目の始めと終わり
    pub breaks: Vec<(i32, Vec<[[f64; 3]; 2]>)>,
    pub index: i32,
    /// R2010 以降の線ごとの種類・色・線種・太さ・矢印の大きさ・矢印・変えた値の印
    pub style: Option<Box<LeaderLineStyle>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LeaderLineStyle {
    pub leader_type: i16,
    pub color: CmColor,
    pub linetype: HandleRef,
    pub lineweight: i32,
    pub arrow_size: f64,
    pub arrow: HandleRef,
    pub overrides: i32,
}

/// 文字の内容
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextText {
    pub label: RawText,
    pub normal: [f64; 3],
    pub style: HandleRef,
    pub location: [f64; 3],
    pub direction: [f64; 3],
    pub rotation: f64,
    pub width: f64,
    pub height: f64,
    pub line_spacing_factor: f64,
    pub line_spacing_style: i16,
    pub color: CmColor,
    pub alignment: i16,
    pub flow: i16,
    pub bg_color: CmColor,
    pub bg_scale: f64,
    pub bg_transparency: i32,
    pub bg_fill: bool,
    pub bg_mask: bool,
    pub column_type: i16,
    pub auto_height: bool,
    pub column_width: f64,
    pub gutter: f64,
    pub flow_reversed: bool,
    pub column_sizes: Vec<f64>,
    pub word_break: bool,
    pub unknown: bool,
}

/// ブロックの内容
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextBlock {
    pub block: HandleRef,
    pub normal: [f64; 3],
    pub location: [f64; 3],
    pub scale: [f64; 3],
    pub rotation: f64,
    pub color: CmColor,
    pub matrix: [f64; 16],
}

impl MLeader {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let version = if s.version >= Version::R2010 { s.data.bs()? } else { 0 };
        let context = AnnotContext::read(s)?;
        let style = s.handle()?;
        let overrides = s.data.bl()?;
        let leader_type = s.data.bs()?;
        let line_color = s.cmc()?;
        let linetype = s.handle()?;
        let lineweight = s.data.bl()?;
        let landing = s.data.bit()?;
        let dogleg = s.data.bit()?;
        let landing_distance = s.data.bd()?;
        let arrow_head = s.handle()?;
        let arrow_size = s.data.bd()?;
        let content_type = s.data.bs()?;
        let text_style = s.handle()?;
        let left_attachment = s.data.bs()?;
        let right_attachment = s.data.bs()?;
        let text_angle_type = s.data.bs()?;
        let unknown = s.data.bs()?;
        let text_color = s.cmc()?;
        let text_frame = s.data.bit()?;
        let block = s.handle()?;
        let block_color = s.cmc()?;
        let block_scale = s.data.bd3()?;
        let block_rotation = s.data.bd()?;
        let block_connection = s.data.bs()?;
        let annotative = s.data.bit()?;
        let mut arrow_heads = Vec::new();
        if s.version < Version::R2010 {
            for _ in 0..count(s.data.bl()?)? {
                arrow_heads.push((s.data.bit()?, s.handle()?));
            }
        }
        let mut labels = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            labels.push((s.handle()?, s.text()?, s.data.bs()?, s.data.bd()?));
        }
        let text_direction_negative = s.data.bit()?;
        let ipe_align = s.data.bs()?;
        let justification = s.data.bs()?;
        let scale = s.data.bd()?;
        let attachments =
            if s.version >= Version::R2010 { Some([s.data.bs()?, s.data.bs()?, s.data.bs()?]) } else { None };
        let extended_to_text = s.version >= Version::R2013 && s.data.bit()?;
        Ok(Self {
            version,
            context,
            style,
            overrides,
            leader_type,
            line_color,
            linetype,
            lineweight,
            landing,
            dogleg,
            landing_distance,
            arrow_head,
            arrow_size,
            content_type,
            text_style,
            left_attachment,
            right_attachment,
            text_angle_type,
            unknown,
            text_color,
            text_frame,
            block,
            block_color,
            block_scale,
            block_rotation,
            block_connection,
            annotative,
            arrow_heads,
            labels,
            text_direction_negative,
            ipe_align,
            justification,
            scale,
            attachments,
            extended_to_text,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        if o.r2010() {
            o.data.bs(self.version);
        }
        self.context.write(o);
        o.href(self.style);
        o.data.bl(self.overrides);
        o.data.bs(self.leader_type);
        o.cmc(&self.line_color);
        o.href(self.linetype);
        o.data.bl(self.lineweight);
        o.data.bit(self.landing);
        o.data.bit(self.dogleg);
        o.data.bd(self.landing_distance);
        o.href(self.arrow_head);
        o.data.bd(self.arrow_size);
        o.data.bs(self.content_type);
        o.href(self.text_style);
        o.data.bs(self.left_attachment);
        o.data.bs(self.right_attachment);
        o.data.bs(self.text_angle_type);
        o.data.bs(self.unknown);
        o.cmc(&self.text_color);
        o.data.bit(self.text_frame);
        o.href(self.block);
        o.cmc(&self.block_color);
        o.data.bd3(self.block_scale);
        o.data.bd(self.block_rotation);
        o.data.bs(self.block_connection);
        o.data.bit(self.annotative);
        if !o.r2010() {
            o.data.bl(self.arrow_heads.len() as i32);
            for (default, handle) in &self.arrow_heads {
                o.data.bit(*default);
                o.href(*handle);
            }
        }
        o.data.bl(self.labels.len() as i32);
        for (handle, text, index, width) in &self.labels {
            o.href(*handle);
            o.text(text);
            o.data.bs(*index);
            o.data.bd(*width);
        }
        o.data.bit(self.text_direction_negative);
        o.data.bs(self.ipe_align);
        o.data.bs(self.justification);
        o.data.bd(self.scale);
        if o.r2010() {
            for value in self.attachments.unwrap_or([0, 9, 9]) {
                o.data.bs(value);
            }
        }
        if o.r2013() {
            o.data.bit(self.extended_to_text);
        }
    }
}

impl AnnotContext {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let mut roots = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            roots.push(LeaderRoot::read(s)?);
        }
        let scale = s.data.bd()?;
        let base_point = s.data.bd3()?;
        let text_height = s.data.bd()?;
        let arrow_size = s.data.bd()?;
        let landing_gap = s.data.bd()?;
        let attachments = [s.data.bs()?, s.data.bs()?, s.data.bs()?, s.data.bs()?];
        let content = if s.data.bit()? {
            let label = s.text()?;
            let normal = s.data.bd3()?;
            let style = s.handle()?;
            let location = s.data.bd3()?;
            let direction = s.data.bd3()?;
            let rotation = s.data.bd()?;
            let width = s.data.bd()?;
            let height = s.data.bd()?;
            let line_spacing_factor = s.data.bd()?;
            let line_spacing_style = s.data.bs()?;
            let color = s.cmc()?;
            let alignment = s.data.bs()?;
            let flow = s.data.bs()?;
            let bg_color = s.cmc()?;
            let bg_scale = s.data.bd()?;
            let bg_transparency = s.data.bl()?;
            let bg_fill = s.data.bit()?;
            let bg_mask = s.data.bit()?;
            let column_type = s.data.bs()?;
            let auto_height = s.data.bit()?;
            let column_width = s.data.bd()?;
            let gutter = s.data.bd()?;
            let flow_reversed = s.data.bit()?;
            let mut column_sizes = Vec::new();
            for _ in 0..count(s.data.bl()?)? {
                column_sizes.push(s.data.bd()?);
            }
            let word_break = s.data.bit()?;
            let unknown = s.data.bit()?;
            Content::Text(Box::new(ContextText {
                label,
                normal,
                style,
                location,
                direction,
                rotation,
                width,
                height,
                line_spacing_factor,
                line_spacing_style,
                color,
                alignment,
                flow,
                bg_color,
                bg_scale,
                bg_transparency,
                bg_fill,
                bg_mask,
                column_type,
                auto_height,
                column_width,
                gutter,
                flow_reversed,
                column_sizes,
                word_break,
                unknown,
            }))
        } else if s.data.bit()? {
            let block = s.handle()?;
            let normal = s.data.bd3()?;
            let location = s.data.bd3()?;
            let scale = s.data.bd3()?;
            let rotation = s.data.bd()?;
            let color = s.cmc()?;
            let mut matrix = [0.0; 16];
            for value in &mut matrix {
                *value = s.data.bd()?;
            }
            Content::Block(Box::new(ContextBlock { block, normal, location, scale, rotation, color, matrix }))
        } else {
            Content::None
        };
        let base = [s.data.bd3()?, s.data.bd3()?, s.data.bd3()?];
        let normal_reversed = s.data.bit()?;
        let top_bottom = if s.version >= Version::R2010 { Some([s.data.bs()?, s.data.bs()?]) } else { None };
        Ok(Self {
            roots,
            scale,
            base_point,
            text_height,
            arrow_size,
            landing_gap,
            attachments,
            content,
            base,
            normal_reversed,
            top_bottom,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bl(self.roots.len() as i32);
        for root in &self.roots {
            root.write(o);
        }
        o.data.bd(self.scale);
        o.data.bd3(self.base_point);
        o.data.bd(self.text_height);
        o.data.bd(self.arrow_size);
        o.data.bd(self.landing_gap);
        for value in self.attachments {
            o.data.bs(value);
        }
        match &self.content {
            Content::Text(text) => {
                o.data.bit(true);
                o.text(&text.label);
                o.data.bd3(text.normal);
                o.href(text.style);
                o.data.bd3(text.location);
                o.data.bd3(text.direction);
                o.data.bd(text.rotation);
                o.data.bd(text.width);
                o.data.bd(text.height);
                o.data.bd(text.line_spacing_factor);
                o.data.bs(text.line_spacing_style);
                o.cmc(&text.color);
                o.data.bs(text.alignment);
                o.data.bs(text.flow);
                o.cmc(&text.bg_color);
                o.data.bd(text.bg_scale);
                o.data.bl(text.bg_transparency);
                o.data.bit(text.bg_fill);
                o.data.bit(text.bg_mask);
                o.data.bs(text.column_type);
                o.data.bit(text.auto_height);
                o.data.bd(text.column_width);
                o.data.bd(text.gutter);
                o.data.bit(text.flow_reversed);
                o.data.bl(text.column_sizes.len() as i32);
                for size in &text.column_sizes {
                    o.data.bd(*size);
                }
                o.data.bit(text.word_break);
                o.data.bit(text.unknown);
            }
            Content::Block(block) => {
                o.data.bit(false);
                o.data.bit(true);
                o.href(block.block);
                o.data.bd3(block.normal);
                o.data.bd3(block.location);
                o.data.bd3(block.scale);
                o.data.bd(block.rotation);
                o.cmc(&block.color);
                for value in block.matrix {
                    o.data.bd(value);
                }
            }
            Content::None => {
                o.data.bit(false);
                o.data.bit(false);
            }
        }
        for vector in self.base {
            o.data.bd3(vector);
        }
        o.data.bit(self.normal_reversed);
        if o.r2010() {
            for value in self.top_bottom.unwrap_or([9, 9]) {
                o.data.bs(value);
            }
        }
    }
}

impl LeaderRoot {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let content_valid = s.data.bit()?;
        let unknown = s.data.bit()?;
        let connection = s.data.bd3()?;
        let direction = s.data.bd3()?;
        let mut breaks = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            breaks.push([s.data.bd3()?, s.data.bd3()?]);
        }
        let index = s.data.bl()?;
        let landing_distance = s.data.bd()?;
        let mut lines = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            lines.push(LeaderLine::read(s)?);
        }
        let attachment_direction = if s.version >= Version::R2010 { s.data.bs()? } else { 0 };
        Ok(Self {
            content_valid,
            unknown,
            connection,
            direction,
            breaks,
            index,
            landing_distance,
            lines,
            attachment_direction,
        })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bit(self.content_valid);
        o.data.bit(self.unknown);
        o.data.bd3(self.connection);
        o.data.bd3(self.direction);
        o.data.bl(self.breaks.len() as i32);
        for [start, end] in &self.breaks {
            o.data.bd3(*start);
            o.data.bd3(*end);
        }
        o.data.bl(self.index);
        o.data.bd(self.landing_distance);
        o.data.bl(self.lines.len() as i32);
        for line in &self.lines {
            line.write(o);
        }
        if o.r2010() {
            o.data.bs(self.attachment_direction);
        }
    }
}

impl LeaderLine {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let mut points = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            points.push(s.data.bd3()?);
        }
        let mut breaks = Vec::new();
        for _ in 0..count(s.data.bl()?)? {
            let segment = s.data.bl()?;
            let mut pairs = Vec::new();
            for _ in 0..count(s.data.bl()?)? {
                pairs.push([s.data.bd3()?, s.data.bd3()?]);
            }
            breaks.push((segment, pairs));
        }
        let index = s.data.bl()?;
        let style = if s.version >= Version::R2010 {
            Some(Box::new(LeaderLineStyle {
                leader_type: s.data.bs()?,
                color: s.cmc()?,
                linetype: s.handle()?,
                lineweight: s.data.bl()?,
                arrow_size: s.data.bd()?,
                arrow: s.handle()?,
                overrides: s.data.bl()?,
            }))
        } else {
            None
        };
        Ok(Self { points, breaks, index, style })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bl(self.points.len() as i32);
        for point in &self.points {
            o.data.bd3(*point);
        }
        o.data.bl(self.breaks.len() as i32);
        for (segment, pairs) in &self.breaks {
            o.data.bl(*segment);
            o.data.bl(pairs.len() as i32);
            for [start, end] in pairs {
                o.data.bd3(*start);
                o.data.bd3(*end);
            }
        }
        o.data.bl(self.index);
        if o.r2010() {
            let none = HandleRef { code: 5, value: 0 };
            let style = self.style.as_deref();
            o.data.bs(style.map_or(1, |style| style.leader_type));
            o.cmc(style.map_or(&CmColor::by_block(), |style| &style.color));
            o.href(style.map_or(none, |style| style.linetype));
            o.data.bl(style.map_or(-2, |style| style.lineweight));
            o.data.bd(style.map_or(0.18, |style| style.arrow_size));
            o.href(style.map_or(none, |style| style.arrow));
            o.data.bl(style.map_or(0, |style| style.overrides));
        }
    }
}
