//! MULTILEADER（20.4.48 MLEADER、20.4.86 MLeaderAnnotContext）を DXF の MULTILEADER と同じ値にする。
//! 色は CMC の BL（0xC0… BYLAYER、0xC1… BYBLOCK、0xC2RRGGBB、0xC3… ACI）を DXF と同じく 32 ビットの符号付きの数で持つ。
//! 値の並びとハンドルの並びの順は ACadSharp（MIT）の readMultiLeader・readMultiLeaderAnnotContext と同じ

use crate::bits::{Eof, Read};
use crate::entities::hex;
use crate::objects::Streams;
use zumen_dxf::*;

/// MULTILEADER の色の生の値の BYBLOCK（0xC1000000）
const BY_BLOCK_RAW: f64 = -1_056_964_608.0;

/// CMC を DXF の 91・92 などの生の値（符号付きの 32 ビット）として読む
fn raw_color(s: &mut Streams<'_>) -> Read<f64> {
    let index = s.data.bs()?;
    if !s.format.r2004() {
        // R2000 の CMC は色番号だけ。0 は BYBLOCK、256 は BYLAYER、ほかは ACI（0xC3…）
        let raw: u32 = match index {
            0 => 0xc100_0000,
            256 => 0xc000_0000,
            _ => 0xc300_0000 | (index as u32 & 0xff),
        };
        return Ok(f64::from(raw as i32));
    }
    let raw = s.data.bl()?;
    let flags = s.data.rc()?;
    if flags & 1 != 0 {
        s.tv()?;
    }
    if flags & 2 != 0 {
        s.tv()?;
    }
    Ok(f64::from(raw))
}

fn count(value: i32) -> Read<usize> {
    if (0..=10_000_000).contains(&value) { Ok(value as usize) } else { Err(Eof) }
}

pub(crate) fn mleader(s: &mut Streams<'_>) -> Read<DxfMLeader> {
    if s.format.r2010() {
        s.data.bs()?;
    }
    let context = context(s)?;
    let style_handle = hex(s.handle()?.value);
    s.data.bl()?;
    let leader_type = f64::from(s.data.bs()?);
    let leader_line_color = raw_color(s)?;
    let leader_linetype_handle = hex(s.handle()?.value);
    let leader_lineweight = f64::from(s.data.bl()?);
    s.data.bit()?;
    let has_dogleg = s.data.bit()?;
    s.data.bd()?;
    let arrow_head = s.handle()?.value;
    s.data.bd()?;
    let content_type = f64::from(s.data.bs()?);
    s.handle()?;
    let text_left_attachment_type = f64::from(s.data.bs()?);
    let text_right_attachment_type = f64::from(s.data.bs()?);
    s.data.bs()?;
    s.data.bs()?;
    raw_color(s)?;
    let has_text_frame = s.data.bit()?;
    s.handle()?;
    raw_color(s)?;
    s.data.bd3()?;
    s.data.bd()?;
    s.data.bs()?;
    s.data.bit()?;
    let mut arrow_heads = Vec::new();
    if !s.format.r2010() {
        // R2007 まで: 引出線ごとの矢印（既定か、ブロックのハンドル）。番号は並びの順とする
        for index in 0..count(s.data.bl()?)? {
            s.data.bit()?;
            let handle = s.handle()?.value;
            arrow_heads.push(DxfMLeaderArrowHead { index: index as f64, handle: hex(handle) });
        }
    }
    let mut block_attribs = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        let handle = hex(s.handle()?.value);
        let text = decode_escapes(&s.tv()?);
        let index = f64::from(s.data.bs()?);
        let width = s.data.bd()?;
        block_attribs.push(DxfMLeaderAttrib { handle, index, width, text });
    }
    s.data.bit()?;
    s.data.bs()?;
    s.data.bs()?;
    s.data.bd()?;
    let (mut top, mut bottom) = (9.0, 9.0);
    if s.format.r2010() {
        s.data.bs()?;
        bottom = f64::from(s.data.bs()?);
        top = f64::from(s.data.bs()?);
    }
    if s.format.r2013() {
        s.data.bit()?;
    }
    Ok(DxfMLeader {
        style_handle,
        leader_type,
        leader_line_color,
        leader_linetype_handle,
        leader_lineweight,
        has_dogleg,
        arrow_head_handle: if arrow_head == 0 { String::new() } else { hex(arrow_head) },
        content_type,
        has_text_frame,
        text_left_attachment_type,
        text_right_attachment_type,
        text_top_attachment_type: top,
        text_bottom_attachment_type: bottom,
        arrow_heads,
        block_attribs,
        context,
    })
}

fn context(s: &mut Streams<'_>) -> Read<DxfMLeaderContext> {
    let mut leaders = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        leaders.push(leader(s)?);
    }
    let scale = s.data.bd()?;
    let base_point = s.data.bd3()?;
    let char_height = s.data.bd()?;
    let arrow_head_size = s.data.bd()?;
    let landing_gap_size = s.data.bd()?;
    for _ in 0..4 {
        s.data.bs()?;
    }
    let mut mtext = None;
    let mut block = None;
    if s.data.bit()? {
        let default_content = s.tv()?;
        let extrusion = s.data.bd3()?;
        let style_handle = hex(s.handle()?.value);
        let insert = s.data.bd3()?;
        let text_direction = s.data.bd3()?;
        s.data.bd()?;
        let width = s.data.bd()?;
        s.data.bd()?;
        let line_spacing_factor = s.data.bd()?;
        let line_spacing_style = f64::from(s.data.bs()?);
        let color = raw_color(s)?;
        let alignment = f64::from(s.data.bs()?);
        let flow_direction = f64::from(s.data.bs()?);
        let bg_color = raw_color(s)?;
        let bg_scale_factor = s.data.bd()?;
        s.data.bl()?;
        let use_window_bg_color = s.data.bit()?;
        let has_bg_fill = s.data.bit()?;
        let column_type = f64::from(s.data.bs()?);
        s.data.bit()?;
        s.data.bd()?;
        s.data.bd()?;
        s.data.bit()?;
        for _ in 0..count(s.data.bl()?)? {
            s.data.bd()?;
        }
        s.data.bit()?;
        s.data.bit()?;
        mtext = Some(DxfMLeaderMText {
            default_content,
            extrusion,
            style_handle,
            insert,
            text_direction,
            width,
            line_spacing_factor,
            line_spacing_style,
            color,
            alignment,
            flow_direction,
            bg_color,
            bg_scale_factor,
            use_window_bg_color,
            has_bg_fill,
            column_type,
        });
    } else if s.data.bit()? {
        let block_record_handle = hex(s.handle()?.value);
        let extrusion = s.data.bd3()?;
        let insert = s.data.bd3()?;
        let scale = s.data.bd3()?;
        let rotation = s.data.bd()?;
        let color = raw_color(s)?;
        for _ in 0..16 {
            s.data.bd()?;
        }
        block = Some(DxfMLeaderBlock { block_record_handle, extrusion, insert, scale, rotation, color });
    }
    s.data.bd3()?;
    s.data.bd3()?;
    s.data.bd3()?;
    s.data.bit()?;
    if s.format.r2010() {
        s.data.bs()?;
        s.data.bs()?;
    }
    Ok(DxfMLeaderContext { scale, base_point, char_height, arrow_head_size, landing_gap_size, mtext, block, leaders })
}

/// 引出線のまとまり（LeaderRoot）
fn leader(s: &mut Streams<'_>) -> Read<DxfMLeaderLeader> {
    let has_last_leader_line = s.data.bit()?;
    let has_dogleg_vector = s.data.bit()?;
    let last_leader_point = s.data.bd3()?;
    let dogleg_vector = s.data.bd3()?;
    let mut breaks = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        breaks.push(DxfMLeaderBreak { start: s.data.bd3()?, end: s.data.bd3()? });
    }
    let index = f64::from(s.data.bl()?);
    let dogleg_length = s.data.bd()?;
    let mut lines = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        lines.push(line(s)?);
    }
    let attachment_direction = if s.format.r2010() { f64::from(s.data.bs()?) } else { 0.0 };
    Ok(DxfMLeaderLeader {
        lines,
        has_last_leader_line,
        has_dogleg_vector,
        last_leader_point,
        dogleg_vector,
        dogleg_length,
        index,
        attachment_direction,
        breaks,
    })
}

/// 引出線の 1 本（LeaderLine）。R2010 以降は線ごとの種類・色・線種・太さ・矢印を持つ
fn line(s: &mut Streams<'_>) -> Read<DxfMLeaderLine> {
    let mut vertices = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        vertices.push(s.data.bd3()?);
    }
    let mut breaks = Vec::new();
    for _ in 0..count(s.data.bl()?)? {
        s.data.bl()?;
        for _ in 0..count(s.data.bl()?)? {
            breaks.push(DxfMLeaderBreak { start: s.data.bd3()?, end: s.data.bd3()? });
        }
    }
    let index = f64::from(s.data.bl()?);
    let mut color = BY_BLOCK_RAW;
    if s.format.r2010() {
        s.data.bs()?;
        color = raw_color(s)?;
        s.handle()?;
        s.data.bl()?;
        s.data.bd()?;
        s.handle()?;
        s.data.bl()?;
    }
    Ok(DxfMLeaderLine { vertices, breaks, index, color })
}
