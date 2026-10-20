//! 表（20.4.51〜20.4.70）: 表の管理のオブジェクトと、LTYPE・LAYER・STYLE・VPORT・APPID・DIMSTYLE・BLOCK_HEADER の
//! 項目。値は DXF の書き出しと同じ（モデルにない値は AutoCAD の既定の値）

use super::header::dim_default;
use super::{MODEL, PAPER, VERSION, Writer, href, owned, rad, upper};
use crate::codec::object::ObjectCommon;
use crate::codec::tables::{
    Appid, BlockHeader, Control, Dash, DimStyle, DimValue, EntryName, Layer, Linetype, TextStyle, Vport,
};
use crate::codec::{CmColor, RawText};
use std::collections::HashMap;

/// R2004 以降の CMC の色。ACI（0 BYBLOCK、256 BYLAYER）とトゥルーカラー（0xRRGGBB）
pub(super) fn cmc(index: f64, true_color: Option<f64>) -> CmColor {
    if let Some(rgb) = true_color {
        return CmColor {
            index: 0,
            rgb: 0xc200_0000 | (zumen_core::js::to_int32(rgb) as u32 & 0xff_ffff),
            name: None,
            book: None,
        };
    }
    let rgb = match index as i32 {
        0 => 0xc100_0000,
        aci @ 1..=255 => 0xc300_0000 | aci as u32,
        _ => 0xc000_0000,
    };
    CmColor { index: 0, rgb, name: None, book: None }
}

/// MULTILEADER などの色の生の値（DXF の 91 など、符号付きの 32 ビット）の CMC
pub(super) fn cmc_raw(raw: f64) -> CmColor {
    CmColor { index: 0, rgb: zumen_core::js::to_int32(raw) as u32, name: None, book: None }
}

/// 線の太さ（DXF の 370、1/100 mm）の番号（R2000 以降の RC。28・29 BYLAYER、30 BYBLOCK、31 既定）。表にない太さは近いもの
pub(super) fn lineweight_index(value: f64) -> u8 {
    const WEIGHTS: [f64; 24] = [
        0.0, 5.0, 9.0, 13.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0, 53.0, 60.0, 70.0, 80.0, 90.0, 100.0,
        106.0, 120.0, 140.0, 158.0, 200.0, 211.0,
    ];
    if value == -1.0 {
        return 29;
    }
    if value == -2.0 {
        return 30;
    }
    if value.is_nan() || value < 0.0 {
        return 31;
    }
    WEIGHTS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| (*a - value).abs().total_cmp(&(*b - value).abs()))
        .map_or(31, |(index, _)| index as u8)
}

/// 拡張データの文字列（R2007 以降: 1000 の印 0、RS の文字の数、UTF-16）
pub(super) fn eed_string(out: &mut Vec<u8>, value: &str) {
    let units: Vec<u16> = value.encode_utf16().take(i16::MAX as usize).collect();
    out.push(0);
    out.extend_from_slice(&(units.len() as u16).to_le_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

/// 拡張データの 32 ビットの整数（1071 の印 71 と RL）
pub(super) fn eed_long(out: &mut Vec<u8>, value: i32) {
    out.push(71);
    out.extend_from_slice(&value.to_le_bytes());
}

/// 表の項目の共通の値（持ち主は表の管理のオブジェクト）
fn entry(owner: u64) -> ObjectCommon {
    owned(owner, false)
}

impl Writer<'_> {
    fn name(&self, value: &str) -> EntryName {
        EntryName::new(self.text(value))
    }

    fn control(&mut self, kind: u16, handle: u64, entries: Vec<u64>, extra: Vec<u64>) {
        let control = Control {
            common: ObjectCommon { owner: href(4, 0), reactors: Vec::new(), xdictionary: None, ds_binary: false },
            entries: entries.into_iter().map(|entry| href(2, entry)).collect(),
            extra: extra.into_iter().map(|entry| href(3, entry)).collect(),
        };
        let mut out = self.object(kind, handle);
        control.write(&mut out, kind);
        self.push(&out);
    }

    /// 表をすべて書く（図形の後。BLOCK_HEADER は持つ図形の並びを使う）
    pub(super) fn write_tables(&mut self) {
        let fixed = std::mem::take(&mut self.fixed);
        self.write_linetypes(&fixed);
        self.write_layers(&fixed);
        self.write_styles(&fixed);
        self.write_vport(&fixed);
        self.write_appids(&fixed);
        self.write_dimstyles(&fixed);
        self.write_block_headers(&fixed);
        self.control(60, fixed.view_control, Vec::new(), Vec::new());
        self.control(62, fixed.ucs_control, Vec::new(), Vec::new());
        self.fixed = fixed;
    }

    /// LTYPE（20.4.58）。ByBlock・ByLayer は表の管理のオブジェクトの終わりの 2 つのハンドルに置く（AutoCAD と同じ）
    fn write_linetypes(&mut self, fixed: &super::Fixed) {
        let mut entries = Vec::new();
        let mut extra = Vec::new();
        for index in 0..self.linetypes.len() {
            let plan = &self.linetypes[index];
            let (name, handle) = (plan.name.clone(), plan.handle);
            match upper(&name).as_str() {
                "BYBLOCK" => extra.insert(0, handle),
                "BYLAYER" => extra.push(handle),
                _ => entries.push(handle),
            }
            let dashes: Vec<Dash> = plan
                .dashes
                .iter()
                .map(|length| Dash {
                    length: *length,
                    shape: 0,
                    offset: [0.0, 0.0],
                    scale: 1.0,
                    rotation: 0.0,
                    flags: 0,
                })
                .collect();
            let linetype = Linetype {
                common: entry(fixed.ltype_control),
                name: self.name(&name),
                description: self.text(&plan.description),
                pattern_length: if dashes.is_empty() { 0.0 } else { plan.length },
                alignment: b'A',
                shapefiles: vec![href(5, 0); dashes.len()],
                dashes,
                strings: Vec::new(),
                xref_block: href(5, 0),
            };
            let mut out = self.object(57, handle);
            linetype.write(&mut out);
            self.push(&out);
        }
        self.control(56, fixed.ltype_control, entries, extra);
    }

    /// LAYER（20.4.54）。印（BS）は 1 凍結、2 非表示、8 ロック、16 印刷と、0x3E0 の線の太さの番号。透過度は拡張データ
    /// AcCmTransparency の 1071
    fn write_layers(&mut self, fixed: &super::Fixed) {
        let transparency_app =
            fixed.appids.iter().find(|(name, _)| *name == "AcCmTransparency").map(|(_, handle)| *handle);
        let mut entries = Vec::new();
        for index in 0..self.layers.len() {
            let plan = &self.layers[index];
            entries.push(plan.handle);
            let values = i16::from(plan.frozen)
                | (i16::from(plan.off) << 1)
                | (i16::from(plan.locked) << 3)
                | (i16::from(plan.plot) << 4)
                | (i16::from(lineweight_index(plan.lineweight)) << 5);
            let linetype = self
                .linetype_handles
                .get(&upper(&plan.linetype))
                .or_else(|| self.linetype_handles.get("CONTINUOUS"))
                .copied()
                .unwrap_or(0);
            let layer = Layer {
                common: entry(fixed.layer_control),
                name: self.name(&plan.name),
                values,
                color: cmc(plan.color, plan.true_color),
                xref_block: href(5, 0),
                plotstyle: href(5, fixed.placeholder),
                material: Some(href(5, fixed.material_global)),
                linetype: href(5, linetype),
                unknown: Some(href(5, 0)),
            };
            let mut out = self.object(51, plan.handle);
            if let (Some(app), Some(value)) = (transparency_app, plan.transparency) {
                let mut data = Vec::new();
                eed_long(&mut data, zumen_core::js::to_int32(value));
                out.eed.push((app, data));
            }
            layer.write(&mut out);
            self.push(&out);
        }
        self.control(50, fixed.layer_control, entries, Vec::new());
    }

    /// STYLE（20.4.56）。TrueType のフォント名は拡張データ ACAD の 1000（DXF の書き出しと同じ）
    fn write_styles(&mut self, fixed: &super::Fixed) {
        let acad = fixed.appids.iter().find(|(name, _)| *name == "ACAD").map(|(_, handle)| *handle);
        let mut entries = Vec::new();
        for index in 0..self.styles.len() {
            let (name, handle, source) = (self.styles[index].0.clone(), self.styles[index].1, self.styles[index].2);
            entries.push(handle);
            let style = match source {
                Some(style) => TextStyle {
                    common: entry(fixed.style_control),
                    name: self.name(&name),
                    shape: zumen_core::js::to_int32(style.flags) & 1 != 0,
                    vertical: zumen_core::js::to_int32(style.flags) & 4 != 0,
                    height: style.height,
                    width: if style.width > 0.0 { style.width } else { 1.0 },
                    oblique: rad(style.oblique),
                    generation: zumen_core::js::to_int32(style.generation_flags) as u8,
                    last_height: 2.5,
                    font: self.text(if style.font.is_empty() { "txt" } else { &style.font }),
                    bigfont: self.text(&style.bigfont),
                    xref_block: href(5, 0),
                },
                None => TextStyle {
                    common: entry(fixed.style_control),
                    name: self.name(&name),
                    shape: false,
                    vertical: false,
                    height: 0.0,
                    width: 1.0,
                    oblique: 0.0,
                    generation: 0,
                    last_height: 2.5,
                    font: self.text("txt"),
                    bigfont: RawText::empty(VERSION),
                    xref_block: href(5, 0),
                },
            };
            let mut out = self.object(53, handle);
            if let (Some(app), Some(family)) = (acad, source.and_then(|style| style.font_family.as_deref())) {
                let mut data = Vec::new();
                eed_string(&mut data, family);
                eed_long(&mut data, 0);
                out.eed.push((app, data));
            }
            style.write(&mut out);
            self.push(&out);
        }
        self.control(52, fixed.style_control, entries, Vec::new());
    }

    /// VPORT *Active（20.4.64）。表示の中心と高さは図面の範囲から（DXF の書き出しと同じ）、ほかは AutoCAD の既定の値
    fn write_vport(&mut self, fixed: &super::Fixed) {
        let header = &self.doc.header;
        let (center, height) = match (header.extmin, header.extmax) {
            (Some(min), Some(max)) if min[0] <= max[0] && min[1] <= max[1] => {
                ([(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0], (max[1] - min[1]).max(max[0] - min[0]).max(1.0))
            }
            _ => ([0.0, 0.0], 1000.0),
        };
        let vport = Vport {
            common: entry(fixed.vport_control),
            name: self.name("*Active"),
            view_height: height,
            aspect: 1.34 * height,
            view_center: center,
            view_target: [0.0; 3],
            view_direction: [0.0, 0.0, 1.0],
            twist: 0.0,
            lens: 50.0,
            front_clip: 0.0,
            back_clip: 0.0,
            view_mode: [false, false, false, true],
            render_mode: 0,
            default_lights: true,
            lighting_type: 1,
            brightness: 0.0,
            contrast: 0.0,
            ambient: Some(CmColor { index: 0, rgb: 0xc233_3333, name: None, book: None }),
            lower_left: [0.0, 0.0],
            upper_right: [1.0, 1.0],
            ucs_follow: false,
            circle_zoom: 1000,
            fast_zoom: true,
            ucs_icon: [true, true],
            grid: false,
            grid_spacing: [0.5, 0.5],
            snap: false,
            snap_style: false,
            snap_isopair: 0,
            snap_rotation: 0.0,
            snap_base: [0.0, 0.0],
            snap_spacing: [0.5, 0.5],
            unknown: false,
            ucs_per_viewport: true,
            ucs_origin: [0.0; 3],
            ucs_x: [1.0, 0.0, 0.0],
            ucs_y: [0.0, 1.0, 0.0],
            ucs_elevation: 0.0,
            ucs_ortho: 0,
            grid_flags: 3,
            grid_major: 5,
            xref_block: href(5, 0),
            background: Some(href(4, 0)),
            visual_style: Some(href(5, 0)),
            sun: Some(href(3, 0)),
            named_ucs: href(5, 0),
            base_ucs: href(5, 0),
        };
        let mut out = self.object(65, fixed.vport);
        vport.write(&mut out);
        self.push(&out);
        self.control(64, fixed.vport_control, vec![fixed.vport], Vec::new());
    }

    /// APPID（20.4.66）
    fn write_appids(&mut self, fixed: &super::Fixed) {
        for &(name, handle) in &fixed.appids {
            let appid =
                Appid { common: entry(fixed.appid_control), name: self.name(name), unknown: 0, xref_block: href(5, 0) };
            let mut out = self.object(67, handle);
            appid.write(&mut out);
            self.push(&out);
        }
        let entries = fixed.appids.iter().map(|(_, handle)| *handle).collect();
        self.control(66, fixed.appid_control, entries, Vec::new());
    }

    /// DIMSTYLE（20.4.68）。モデルが持つ DIMSCALE・DIMASZ・DIMTXT のほかは AutoCAD の既定の値。モデルにない寸法スタイルは
    /// DXF の書き出しと同じく 1・2.5・2.5
    fn write_dimstyles(&mut self, fixed: &super::Fixed) {
        let standard = self.style_handles.get("STANDARD").copied().unwrap_or(0);
        let mut entries = Vec::new();
        for index in 0..self.dimstyles.len() {
            let (name, handle, source) =
                (self.dimstyles[index].0.clone(), self.dimstyles[index].1, self.dimstyles[index].2);
            entries.push(handle);
            let (scale, arrow, text) =
                source.map_or((1.0, 2.5, 2.5), |style| (style.dimscale, style.dimasz, style.dimtxt));
            let values = DimStyle::fields(VERSION)
                .map(|(key, _)| {
                    let value = match key {
                        "DIMSCALE" => DimValue::Double(scale),
                        "DIMASZ" => DimValue::Double(arrow),
                        "DIMTXT" => DimValue::Double(text),
                        "FLAG" => DimValue::Bit(false),
                        _ => dim_default(key),
                    };
                    (key, value)
                })
                .collect();
            let mut handles = vec![href(5, standard)];
            handles.extend(std::iter::repeat_n(href(5, 0), 7));
            let style = DimStyle {
                common: entry(fixed.dimstyle_control),
                name: self.name(&name),
                values,
                xref_block: href(5, 0),
                handles,
            };
            let mut out = self.object(69, handle);
            style.write(&mut out);
            self.push(&out);
        }
        self.control(68, fixed.dimstyle_control, entries, Vec::new());
    }

    /// BLOCK_HEADER（20.4.52）。名前は AutoCAD と同じく、無名のブロックは頭の 2 文字（*D など）、紙空間のブロックは
    /// 番号を除いた *Paper_Space（読み取りは BLOCK の名前を使う）。モデル空間・紙空間は表の管理のオブジェクトの
    /// 終わりのハンドルに置く
    fn write_block_headers(&mut self, fixed: &super::Fixed) {
        let mut entries = Vec::new();
        let mut extra = vec![0, 0];
        // ブロック → 描画順の拡張辞書
        let sort_dictionaries: HashMap<usize, u64> =
            self.sort_tables.iter().map(|(block, dictionary, ..)| (*block, *dictionary)).collect();
        for index in 0..self.blocks.len() {
            let block = &self.blocks[index];
            let source = block.source.map(|index| &self.doc.blocks[index]);
            let key = upper(&block.name);
            let flags = source.map_or(0, |block| zumen_core::js::to_int32(block.flags));
            let anonymous = flags & 1 != 0;
            let xref = flags & 4 != 0;
            let overlaid = flags & 8 != 0;
            let header_name = if key == MODEL {
                "*Model_Space".to_string()
            } else if key.starts_with(PAPER) {
                "*Paper_Space".to_string()
            } else if anonymous && block.name.starts_with('*') {
                block.name.chars().take(2).collect()
            } else {
                block.name.clone()
            };
            match key.as_str() {
                MODEL => extra[0] = block.record,
                PAPER => extra[1] = block.record,
                _ => entries.push(block.record),
            }
            let header = BlockHeader {
                common: entry(fixed.block_control),
                name: self.name(&header_name),
                anonymous,
                has_attributes: block.has_attributes,
                xref,
                overlaid,
                loaded: false,
                base: source.map_or([0.0; 3], |block| block.base_point),
                xref_path: self.text(source.and_then(|block| block.xref_path.as_deref()).unwrap_or("")),
                insert_count: vec![1; block.inserts.len()],
                description: self.text(source.and_then(|block| block.description.as_deref()).unwrap_or("")),
                preview: Vec::new(),
                insert_units: 0,
                explodable: true,
                scaling: 0,
                null: href(5, 0),
                block: href(3, block.begin),
                entities: block.entities.iter().map(|entity| href(3, *entity)).collect(),
                first_last: None,
                endblk: href(3, block.end),
                inserts: block.inserts.iter().map(|insert| href(4, *insert)).collect(),
                layout: href(5, block.layout),
            };
            let mut out = self.object(49, block.record);
            let mut header = header;
            if let Some(&dictionary) = sort_dictionaries.get(&index) {
                header.common.xdictionary = Some(href(3, dictionary));
            }
            header.write(&mut out);
            self.push(&out);
        }
        self.control(48, fixed.block_control, entries, extra);
    }
}
