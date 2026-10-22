//! 図形でないオブジェクト: DICTIONARY（20.4.44）、DICTIONARYWDFLT（20.4.45）、ACDBPLACEHOLDER、LAYOUT（20.4.84）、
//! MLINESTYLE（20.4.73）、MLEADERSTYLE（20.4.87）、XRECORD（20.4.104）、DICTIONARYVAR（20.4.74）、SCALE（20.4.92）、
//! SORTENTSTABLE（20.4.93）、GROUP（20.4.72）、IMAGEDEF（20.4.82）、IMAGEDEF_REACTOR（20.4.83）、MATERIAL（仕様にない。ACadSharp の並びに従い、実物の図面で確かめた）

use super::object::ObjectCommon;
use super::{CmColor, In, RawText};
use crate::bits::{Eof, HandleRef, Read};
use crate::file::Version;
use crate::write::object::ObjectWriter;

/// DICTIONARY・DICTIONARYWDFLT
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Dictionary {
    pub common: ObjectCommon,
    /// 複製のときの扱い（281）と、項目を所有するか（280）。R2000 以降
    pub cloning: i16,
    pub hard_owner: u8,
    /// R14 だけの不明の RC（仕様は「いつも 0」）
    pub r14_unknown: u8,
    pub entries: Vec<(RawText, HandleRef)>,
    /// DICTIONARYWDFLT の既定の項目
    pub default: Option<HandleRef>,
}

impl Dictionary {
    pub fn read(s: &mut In<'_>, with_default: bool) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let count = s.data.bl()?;
        // 既定の項目を持つ辞書（R2000 のクラス）は、R13・R14 の図面でも R2000 の形（AutoCAD の図面で確かめた）
        let r2000 = s.r2000() || with_default;
        let r14_unknown = if s.version == Version::R14 && !with_default { s.data.rc()? } else { 0 };
        let (cloning, hard_owner) = if r2000 { (s.data.bs()?, s.data.rc()?) } else { (1, 0) };
        let mut names = Vec::new();
        for _ in 0..count.clamp(0, 1 << 20) {
            names.push(s.text()?);
        }
        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            entries.push((name, s.handle()?));
        }
        let default = if with_default { Some(s.handle()?) } else { None };
        Ok(Self { common, cloning, hard_owner, r14_unknown, entries, default })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bl(self.entries.len() as i32);
        let with_default = self.default.is_some();
        if o.version == Version::R14 && !with_default {
            o.data.rc(self.r14_unknown);
        }
        if o.r2000() || with_default {
            o.data.bs(self.cloning);
            o.data.rc(self.hard_owner);
        }
        for (name, _) in &self.entries {
            o.text(name);
        }
        for (_, entry) in &self.entries {
            o.href(*entry);
        }
        if let Some(default) = self.default {
            o.href(default);
        }
    }
}

/// ACDBPLACEHOLDER（共通の値だけ）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placeholder {
    pub common: ObjectCommon,
}

impl Placeholder {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        Ok(Self { common: ObjectCommon::read(s)? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
    }
}

/// LAYOUT（20.4.84）と、その前の PLOTSETTINGS の値
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Layout {
    pub common: ObjectCommon,
    pub page_setup: RawText,
    pub printer: RawText,
    pub plot_flags: i16,
    /// 左・下・右・上（mm）
    pub margins: [f64; 4],
    pub paper: [f64; 2],
    pub paper_size: RawText,
    pub plot_origin: [f64; 2],
    pub paper_units: i16,
    pub plot_rotation: i16,
    pub plot_type: i16,
    pub window_min: [f64; 2],
    pub window_max: [f64; 2],
    /// R2000 の印刷するビューの名前
    pub plot_view_name: Option<RawText>,
    pub real_units: f64,
    pub drawing_units: f64,
    pub style_sheet: RawText,
    pub scale_type: i16,
    pub scale_factor: f64,
    pub paper_image_origin: [f64; 2],
    /// R2004 以降の陰影の印刷
    pub shade_mode: i16,
    pub shade_level: i16,
    pub shade_dpi: i16,
    pub name: RawText,
    pub tab_order: i32,
    pub flags: i16,
    pub ucs_origin: [f64; 3],
    pub limmin: [f64; 2],
    pub limmax: [f64; 2],
    pub insbase: [f64; 3],
    pub ucs_x: [f64; 3],
    pub ucs_y: [f64; 3],
    pub elevation: f64,
    pub ortho_type: i16,
    pub extmin: [f64; 3],
    pub extmax: [f64; 3],
    /// R2004 以降の印刷するビュー、R2007 以降の表示スタイル
    pub plot_view: Option<HandleRef>,
    pub visual_style: Option<HandleRef>,
    pub block_record: HandleRef,
    pub last_viewport: HandleRef,
    pub base_ucs: HandleRef,
    pub named_ucs: HandleRef,
    /// R2004 以降のビューポート
    pub viewports: Vec<HandleRef>,
}

impl Layout {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let page_setup = s.text()?;
        let printer = s.text()?;
        let plot_flags = s.data.bs()?;
        let margins = [s.data.bd()?, s.data.bd()?, s.data.bd()?, s.data.bd()?];
        let paper = s.data.bd2()?;
        let paper_size = s.text()?;
        let plot_origin = s.data.bd2()?;
        let paper_units = s.data.bs()?;
        let plot_rotation = s.data.bs()?;
        let plot_type = s.data.bs()?;
        let window_min = s.data.bd2()?;
        let window_max = s.data.bd2()?;
        let plot_view_name = if s.r2004() { None } else { Some(s.text()?) };
        let real_units = s.data.bd()?;
        let drawing_units = s.data.bd()?;
        let style_sheet = s.text()?;
        let scale_type = s.data.bs()?;
        let scale_factor = s.data.bd()?;
        let paper_image_origin = s.data.bd2()?;
        let (mut shade_mode, mut shade_level, mut shade_dpi) = (0, 0, 0);
        if s.r2004() {
            shade_mode = s.data.bs()?;
            shade_level = s.data.bs()?;
            shade_dpi = s.data.bs()?;
        }
        let name = s.text()?;
        let tab_order = s.data.bl()?;
        let flags = s.data.bs()?;
        let ucs_origin = s.data.bd3()?;
        let limmin = s.data.rd2()?;
        let limmax = s.data.rd2()?;
        let insbase = s.data.bd3()?;
        let ucs_x = s.data.bd3()?;
        let ucs_y = s.data.bd3()?;
        let elevation = s.data.bd()?;
        let ortho_type = s.data.bs()?;
        let extmin = s.data.bd3()?;
        let extmax = s.data.bd3()?;
        let viewport_count = if s.r2004() { s.data.bl()? } else { 0 };
        let plot_view = if s.r2004() { Some(s.handle()?) } else { None };
        let visual_style = if s.r2007() { Some(s.handle()?) } else { None };
        let block_record = s.handle()?;
        let last_viewport = s.handle()?;
        let base_ucs = s.handle()?;
        let named_ucs = s.handle()?;
        let mut viewports = Vec::new();
        for _ in 0..viewport_count.clamp(0, 1 << 16) {
            viewports.push(s.handle()?);
        }
        Ok(Self {
            common,
            page_setup,
            printer,
            plot_flags,
            margins,
            paper,
            paper_size,
            plot_origin,
            paper_units,
            plot_rotation,
            plot_type,
            window_min,
            window_max,
            plot_view_name,
            real_units,
            drawing_units,
            style_sheet,
            scale_type,
            scale_factor,
            paper_image_origin,
            shade_mode,
            shade_level,
            shade_dpi,
            name,
            tab_order,
            flags,
            ucs_origin,
            limmin,
            limmax,
            insbase,
            ucs_x,
            ucs_y,
            elevation,
            ortho_type,
            extmin,
            extmax,
            plot_view,
            visual_style,
            block_record,
            last_viewport,
            base_ucs,
            named_ucs,
            viewports,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.text(&self.page_setup);
        o.text(&self.printer);
        o.data.bs(self.plot_flags);
        for margin in self.margins {
            o.data.bd(margin);
        }
        o.data.bd2(self.paper);
        o.text(&self.paper_size);
        o.data.bd2(self.plot_origin);
        o.data.bs(self.paper_units);
        o.data.bs(self.plot_rotation);
        o.data.bs(self.plot_type);
        o.data.bd2(self.window_min);
        o.data.bd2(self.window_max);
        if !o.r2004() {
            o.text(self.plot_view_name.as_ref().unwrap_or(&RawText::empty(o.version)));
        }
        o.data.bd(self.real_units);
        o.data.bd(self.drawing_units);
        o.text(&self.style_sheet);
        o.data.bs(self.scale_type);
        o.data.bd(self.scale_factor);
        o.data.bd2(self.paper_image_origin);
        if o.r2004() {
            o.data.bs(self.shade_mode);
            o.data.bs(self.shade_level);
            o.data.bs(self.shade_dpi);
        }
        o.text(&self.name);
        o.data.bl(self.tab_order);
        o.data.bs(self.flags);
        o.data.bd3(self.ucs_origin);
        o.data.rd2(self.limmin);
        o.data.rd2(self.limmax);
        o.data.bd3(self.insbase);
        o.data.bd3(self.ucs_x);
        o.data.bd3(self.ucs_y);
        o.data.bd(self.elevation);
        o.data.bs(self.ortho_type);
        o.data.bd3(self.extmin);
        o.data.bd3(self.extmax);
        if o.r2004() {
            o.data.bl(self.viewports.len() as i32);
            o.href(self.plot_view.unwrap_or(HandleRef { code: 5, value: 0 }));
        }
        if o.r2007() {
            o.href(self.visual_style.unwrap_or(HandleRef { code: 4, value: 0 }));
        }
        o.href(self.block_record);
        o.href(self.last_viewport);
        o.href(self.base_ucs);
        o.href(self.named_ucs);
        if o.r2004() {
            for viewport in &self.viewports {
                o.href(*viewport);
            }
        }
    }
}

/// MLINESTYLE の要素
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLineElement {
    pub offset: f64,
    pub color: CmColor,
    /// R2018 より前は線種の番号（32767 BYLAYER、32766 BYBLOCK）、R2018 以降は線種のハンドル
    pub linetype_index: i16,
    pub linetype: Option<HandleRef>,
}

/// MLINESTYLE（20.4.73）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLineStyle {
    pub common: ObjectCommon,
    pub name: RawText,
    pub description: RawText,
    /// DWG のビット（DXF の 70 とは並びが違う。document.rs の読み取りを見る）
    pub flags: i16,
    pub fill_color: CmColor,
    pub start_angle: f64,
    pub end_angle: f64,
    pub elements: Vec<MLineElement>,
}

impl MLineStyle {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = s.text()?;
        let description = s.text()?;
        let flags = s.data.bs()?;
        let fill_color = s.cmc()?;
        let start_angle = s.data.bd()?;
        let end_angle = s.data.bd()?;
        let count = s.data.rc()?;
        let r2018 = s.version >= Version::R2018;
        let mut elements = Vec::new();
        for _ in 0..count {
            let offset = s.data.bd()?;
            let color = s.cmc()?;
            let linetype_index = if r2018 { 0 } else { s.data.bs()? };
            elements.push(MLineElement { offset, color, linetype_index, linetype: None });
        }
        if r2018 {
            for element in &mut elements {
                element.linetype = Some(s.handle()?);
            }
        }
        Ok(Self { common, name, description, flags, fill_color, start_angle, end_angle, elements })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.text(&self.name);
        o.text(&self.description);
        o.data.bs(self.flags);
        o.cmc(&self.fill_color);
        o.data.bd(self.start_angle);
        o.data.bd(self.end_angle);
        o.data.rc(self.elements.len().min(255) as u8);
        let r2018 = o.r2018();
        for element in self.elements.iter().take(255) {
            o.data.bd(element.offset);
            o.cmc(&element.color);
            if !r2018 {
                o.data.bs(element.linetype_index);
            }
        }
        if r2018 {
            for element in self.elements.iter().take(255) {
                o.href(element.linetype.unwrap_or(HandleRef { code: 5, value: 0 }));
            }
        }
    }
}

/// MLEADERSTYLE（20.4.87）。R2010 以降は版（BS 2）が先にある。仕様は文字の角度と左揃えを「新しい形」だけに置くが、
/// どの版の図面にもある（ACadSharp と同じ。実物の図面で確かめた）。R2013 以降は終わりに 1 ビット（DXF の 298）がある
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MLeaderStyle {
    pub common: ObjectCommon,
    pub version: Option<i16>,
    pub content_type: i16,
    pub draw_mleader_order: i16,
    pub draw_leader_order: i16,
    pub max_points: i32,
    pub first_angle: f64,
    pub second_angle: f64,
    pub leader_type: i16,
    pub line_color: CmColor,
    pub line_type: HandleRef,
    pub line_weight: i32,
    pub landing: bool,
    pub landing_gap: f64,
    pub dogleg: bool,
    pub landing_distance: f64,
    pub description: RawText,
    pub arrow: HandleRef,
    pub arrow_size: f64,
    pub text_default: RawText,
    pub text_style: HandleRef,
    pub left_attachment: i16,
    pub right_attachment: i16,
    pub text_angle_type: i16,
    pub text_alignment: i16,
    pub text_color: CmColor,
    pub text_height: f64,
    pub text_frame: bool,
    pub align_left: bool,
    pub align_space: f64,
    pub block: HandleRef,
    pub block_color: CmColor,
    pub block_scale: [f64; 3],
    pub block_scale_enabled: bool,
    pub block_rotation: f64,
    pub block_rotation_enabled: bool,
    pub block_connection: i16,
    pub scale: f64,
    pub changed: bool,
    pub annotative: bool,
    pub break_size: f64,
    /// R2010 以降
    pub attachment_direction: i16,
    pub top_attachment: i16,
    pub bottom_attachment: i16,
    /// R2013 以降
    pub unknown298: bool,
}

impl MLeaderStyle {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let new_format = s.version >= Version::R2010;
        let version = if new_format { Some(s.data.bs()?) } else { None };
        let content_type = s.data.bs()?;
        let draw_mleader_order = s.data.bs()?;
        let draw_leader_order = s.data.bs()?;
        let max_points = s.data.bl()?;
        let first_angle = s.data.bd()?;
        let second_angle = s.data.bd()?;
        let leader_type = s.data.bs()?;
        let line_color = s.cmc()?;
        let line_type = s.handle()?;
        let line_weight = s.data.bl()?;
        let landing = s.data.bit()?;
        let landing_gap = s.data.bd()?;
        let dogleg = s.data.bit()?;
        let landing_distance = s.data.bd()?;
        let description = s.text()?;
        let arrow = s.handle()?;
        let arrow_size = s.data.bd()?;
        let text_default = s.text()?;
        let text_style = s.handle()?;
        let left_attachment = s.data.bs()?;
        let right_attachment = s.data.bs()?;
        let text_angle_type = s.data.bs()?;
        let text_alignment = s.data.bs()?;
        let text_color = s.cmc()?;
        let text_height = s.data.bd()?;
        let text_frame = s.data.bit()?;
        let align_left = s.data.bit()?;
        let align_space = s.data.bd()?;
        let block = s.handle()?;
        let block_color = s.cmc()?;
        let block_scale = s.data.bd3()?;
        let block_scale_enabled = s.data.bit()?;
        let block_rotation = s.data.bd()?;
        let block_rotation_enabled = s.data.bit()?;
        let block_connection = s.data.bs()?;
        let scale = s.data.bd()?;
        let changed = s.data.bit()?;
        let annotative = s.data.bit()?;
        let break_size = s.data.bd()?;
        let (mut attachment_direction, mut top_attachment, mut bottom_attachment) = (0, 0, 0);
        if new_format {
            attachment_direction = s.data.bs()?;
            top_attachment = s.data.bs()?;
            bottom_attachment = s.data.bs()?;
        }
        let unknown298 = s.version >= Version::R2013 && s.data.bit()?;
        Ok(Self {
            common,
            version,
            content_type,
            draw_mleader_order,
            draw_leader_order,
            max_points,
            first_angle,
            second_angle,
            leader_type,
            line_color,
            line_type,
            line_weight,
            landing,
            landing_gap,
            dogleg,
            landing_distance,
            description,
            arrow,
            arrow_size,
            text_default,
            text_style,
            left_attachment,
            right_attachment,
            text_angle_type,
            text_alignment,
            text_color,
            text_height,
            text_frame,
            align_left,
            align_space,
            block,
            block_color,
            block_scale,
            block_scale_enabled,
            block_rotation,
            block_rotation_enabled,
            block_connection,
            scale,
            changed,
            annotative,
            break_size,
            attachment_direction,
            top_attachment,
            bottom_attachment,
            unknown298,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        let new_format = o.r2010();
        if new_format {
            o.data.bs(self.version.unwrap_or(2));
        }
        o.data.bs(self.content_type);
        o.data.bs(self.draw_mleader_order);
        o.data.bs(self.draw_leader_order);
        o.data.bl(self.max_points);
        o.data.bd(self.first_angle);
        o.data.bd(self.second_angle);
        o.data.bs(self.leader_type);
        o.cmc(&self.line_color);
        o.href(self.line_type);
        o.data.bl(self.line_weight);
        o.data.bit(self.landing);
        o.data.bd(self.landing_gap);
        o.data.bit(self.dogleg);
        o.data.bd(self.landing_distance);
        o.text(&self.description);
        o.href(self.arrow);
        o.data.bd(self.arrow_size);
        o.text(&self.text_default);
        o.href(self.text_style);
        o.data.bs(self.left_attachment);
        o.data.bs(self.right_attachment);
        o.data.bs(self.text_angle_type);
        o.data.bs(self.text_alignment);
        o.cmc(&self.text_color);
        o.data.bd(self.text_height);
        o.data.bit(self.text_frame);
        o.data.bit(self.align_left);
        o.data.bd(self.align_space);
        o.href(self.block);
        o.cmc(&self.block_color);
        o.data.bd3(self.block_scale);
        o.data.bit(self.block_scale_enabled);
        o.data.bd(self.block_rotation);
        o.data.bit(self.block_rotation_enabled);
        o.data.bs(self.block_connection);
        o.data.bd(self.scale);
        o.data.bit(self.changed);
        o.data.bit(self.annotative);
        o.data.bd(self.break_size);
        if new_format {
            o.data.bs(self.attachment_direction);
            o.data.bs(self.top_attachment);
            o.data.bs(self.bottom_attachment);
        }
        if o.r2013() {
            o.data.bit(self.unknown298);
        }
    }
}

/// XRECORD（20.4.104）。値はバイトのまま、後ろに複製の扱い（R2000 以降）とハンドル
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XRecord {
    pub common: ObjectCommon,
    pub data: Vec<u8>,
    pub cloning: i16,
    pub handles: Vec<HandleRef>,
}

impl XRecord {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let size = s.data.bl()?;
        let data = s.data.bytes(usize::try_from(size).map_err(|_| Eof)?)?;
        let cloning = if s.r2000() { s.data.bs()? } else { 0 };
        // ハンドルの並びの終わり（埋めたビットを除く）まで読む
        let mut handles = Vec::new();
        while s.handles.as_ref().is_some_and(|stream| stream.remaining() >= 8) {
            match s.handle() {
                Ok(handle) => handles.push(handle),
                Err(_) => break,
            }
        }
        Ok(Self { common, data, cloning, handles })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bl(self.data.len() as i32);
        o.data.raw(&self.data);
        if o.r2000() {
            o.data.bs(self.cloning);
        }
        for handle in &self.handles {
            o.href(*handle);
        }
    }
}

/// DICTIONARYVAR（20.4.74）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DictionaryVar {
    pub common: ObjectCommon,
    pub schema: u8,
    pub value: RawText,
}

impl DictionaryVar {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        Ok(Self { common, schema: s.data.rc()?, value: s.text()? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.rc(self.schema);
        o.text(&self.value);
    }
}

/// IMAGEDEF（20.4.82）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImageDef {
    pub common: ObjectCommon,
    pub class_version: i32,
    /// 画素の数（幅、高さ）
    pub size: [f64; 2],
    pub path: RawText,
    pub loaded: bool,
    /// 0 なし、2 cm、5 インチ
    pub resolution_units: u8,
    /// 画素 1 つ分の大きさ
    pub pixel_size: [f64; 2],
}

impl ImageDef {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        Ok(Self {
            common,
            class_version: s.data.bl()?,
            size: s.data.rd2()?,
            path: s.text()?,
            loaded: s.data.bit()?,
            resolution_units: s.data.rc()?,
            pixel_size: s.data.rd2()?,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bl(self.class_version);
        o.data.rd2(self.size);
        o.text(&self.path);
        o.data.bit(self.loaded);
        o.data.rc(self.resolution_units);
        o.data.rd2(self.pixel_size);
    }
}

/// IMAGEDEF_REACTOR（20.4.83）。所有者は画像（IMAGE）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImageDefReactor {
    pub common: ObjectCommon,
    pub class_version: i32,
}

impl ImageDefReactor {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        Ok(Self { common, class_version: s.data.bl()? })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bl(self.class_version);
    }
}

/// SCALE（20.4.92）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Scale {
    pub common: ObjectCommon,
    pub unknown: i16,
    pub name: RawText,
    pub paper_units: f64,
    pub drawing_units: f64,
    pub unit_scale: bool,
}

impl Scale {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        Ok(Self {
            common,
            unknown: s.data.bs()?,
            name: s.text()?,
            paper_units: s.data.bd()?,
            drawing_units: s.data.bd()?,
            unit_scale: s.data.bit()?,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bs(self.unknown);
        o.text(&self.name);
        o.data.bd(self.paper_units);
        o.data.bd(self.drawing_units);
        o.data.bit(self.unit_scale);
    }
}

/// SORTENTSTABLE（20.4.93）。並べ替えのハンドルは値の並び（CODE 0）、図形のハンドルはハンドルの並び
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SortEntsTable {
    pub common: ObjectCommon,
    pub owner: HandleRef,
    /// (並べ替えのハンドル, 図形)
    pub entries: Vec<(HandleRef, HandleRef)>,
}

impl SortEntsTable {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let count = s.data.bl()?;
        let mut sorts = Vec::new();
        for _ in 0..count.clamp(0, 1 << 24) {
            sorts.push(s.data.handle()?);
        }
        let owner = s.handle()?;
        let mut entries = Vec::with_capacity(sorts.len());
        for sort in sorts {
            entries.push((sort, s.handle()?));
        }
        Ok(Self { common, owner, entries })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.data.bl(self.entries.len() as i32);
        for (sort, _) in &self.entries {
            o.data.handle(sort.code, sort.value);
        }
        o.href(self.owner);
        for (_, entity) in &self.entries {
            o.href(*entity);
        }
    }
}

/// GROUP（20.4.72）
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Group {
    pub common: ObjectCommon,
    pub name: RawText,
    pub unnamed: i16,
    pub selectable: i16,
    pub entries: Vec<HandleRef>,
}

impl Group {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = s.text()?;
        let unnamed = s.data.bs()?;
        let selectable = s.data.bs()?;
        let count = s.data.bl()?;
        let mut entries = Vec::new();
        for _ in 0..count.clamp(0, 1 << 24) {
            entries.push(s.handle()?);
        }
        Ok(Self { common, name, unnamed, selectable, entries })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.text(&self.name);
        o.data.bs(self.unnamed);
        o.data.bs(self.selectable);
        o.data.bl(self.entries.len() as i32);
        for entry in &self.entries {
            o.href(*entry);
        }
    }
}

/// MATERIAL の色（方法 RC、係数 BD、方法が 1（上書き）なら RGB の BL）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterialColor {
    pub method: u8,
    pub factor: f64,
    pub rgb: Option<i32>,
}

impl MaterialColor {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let method = s.data.rc()?;
        let factor = s.data.bd()?;
        let rgb = if method == 1 { Some(s.data.bl()?) } else { None };
        Ok(Self { method, factor, rgb })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.rc(self.method);
        o.data.bd(self.factor);
        if self.method == 1 {
            o.data.bl(self.rgb.unwrap_or(0));
        }
    }
}

/// MATERIAL の画像の写し方（混ぜる割合 BD、投影・並べ方・自動の変換 RC、16 の BD の行列、画像の元 RC、
/// 元が 1（画像のファイル）ならファイルの名前）
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MaterialMap {
    pub blend: f64,
    pub projection: u8,
    pub tiling: u8,
    pub auto_transform: u8,
    pub matrix: [f64; 16],
    pub source: u8,
    pub file: Option<RawText>,
}

impl MaterialMap {
    fn read(s: &mut In<'_>) -> Read<Self> {
        let blend = s.data.bd()?;
        let projection = s.data.rc()?;
        let tiling = s.data.rc()?;
        let auto_transform = s.data.rc()?;
        let mut matrix = [0.0; 16];
        for value in &mut matrix {
            *value = s.data.bd()?;
        }
        let source = s.data.rc()?;
        let file = if source == 1 { Some(s.text()?) } else { None };
        Ok(Self { blend, projection, tiling, auto_transform, matrix, source, file })
    }

    fn write(&self, o: &mut ObjectWriter) {
        o.data.bd(self.blend);
        o.data.rc(self.projection);
        o.data.rc(self.tiling);
        o.data.rc(self.auto_transform);
        for value in self.matrix {
            o.data.bd(value);
        }
        o.data.rc(self.source);
        if self.source == 1 {
            o.text(self.file.as_ref().unwrap_or(&RawText::empty(o.version)));
        }
    }
}

/// MATERIAL。名前・説明、周囲・拡散の色と画像、鏡面の色・画像・光沢、反射の画像と不透明度、不透明度の画像、
/// 凹凸の画像と屈折率、屈折の画像、（R2007 以降）半透明・自己発光・反射率と 3 つの BL
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Material {
    pub common: ObjectCommon,
    pub name: RawText,
    pub description: RawText,
    pub ambient: MaterialColor,
    pub diffuse: MaterialColor,
    pub diffuse_map: MaterialMap,
    pub specular: MaterialColor,
    pub specular_map: MaterialMap,
    pub gloss: f64,
    pub reflection_map: MaterialMap,
    pub opacity: f64,
    pub opacity_map: MaterialMap,
    pub bump_map: MaterialMap,
    pub refraction_index: f64,
    pub refraction_map: MaterialMap,
    /// R2007 以降の半透明・自己発光・反射率と、照明のモデル・チャンネルの印・モード
    pub tail: Option<([f64; 3], [i32; 3])>,
}

impl Material {
    pub fn read(s: &mut In<'_>) -> Read<Self> {
        let common = ObjectCommon::read(s)?;
        let name = s.text()?;
        let description = s.text()?;
        let ambient = MaterialColor::read(s)?;
        let diffuse = MaterialColor::read(s)?;
        let diffuse_map = MaterialMap::read(s)?;
        let specular = MaterialColor::read(s)?;
        let specular_map = MaterialMap::read(s)?;
        let gloss = s.data.bd()?;
        let reflection_map = MaterialMap::read(s)?;
        let opacity = s.data.bd()?;
        let opacity_map = MaterialMap::read(s)?;
        let bump_map = MaterialMap::read(s)?;
        let refraction_index = s.data.bd()?;
        let refraction_map = MaterialMap::read(s)?;
        let tail = if s.r2007() {
            Some(([s.data.bd()?, s.data.bd()?, s.data.bd()?], [s.data.bl()?, s.data.bl()?, s.data.bl()?]))
        } else {
            None
        };
        Ok(Self {
            common,
            name,
            description,
            ambient,
            diffuse,
            diffuse_map,
            specular,
            specular_map,
            gloss,
            reflection_map,
            opacity,
            opacity_map,
            bump_map,
            refraction_index,
            refraction_map,
            tail,
        })
    }

    pub fn write(&self, o: &mut ObjectWriter) {
        self.common.write(o);
        o.text(&self.name);
        o.text(&self.description);
        self.ambient.write(o);
        self.diffuse.write(o);
        self.diffuse_map.write(o);
        self.specular.write(o);
        self.specular_map.write(o);
        o.data.bd(self.gloss);
        self.reflection_map.write(o);
        o.data.bd(self.opacity);
        self.opacity_map.write(o);
        self.bump_map.write(o);
        o.data.bd(self.refraction_index);
        self.refraction_map.write(o);
        if o.r2007() {
            let (doubles, longs) = self.tail.unwrap_or(([0.0; 3], [0; 3]));
            for value in doubles {
                o.data.bd(value);
            }
            for value in longs {
                o.data.bl(value);
            }
        }
    }
}
